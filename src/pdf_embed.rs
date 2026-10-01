//! Embed OCR-recognized text into a scanned PDF as an invisible layer
//! (roadmap 2.21) — the standard "searchable PDF" construction: the
//! recognized words are written over the scanned image in PDF text
//! render mode 3 (invisible), so any application can search, select
//! and copy them while the scan stays pixel-identical.
//!
//! The action is conscious and explicit — a confirmation dialog, a
//! `.bak` backup kept beside the file — and never automatic. The file
//! on disk is only replaced after the rewritten copy has been reopened
//! and verified; every failure path leaves the original untouched.
//!
//! Everything here runs on the embed task's worker thread. The UI
//! thread never touches the file, the database, or parsing (2.20):
//! the only main-thread code is the dialog and the result toast.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use gtk::prelude::*;

use crate::db::Catalog;
use crate::pdf::{PdfPageText, PdfTextLine};
use crate::tasks::Reporter;

/// One word placed on the page: its text and its bounding quad in view
/// coordinates (origin top-left, y downward — the space both text
/// paths already use, and the space `Shape::insert_text` expects: it
/// maps the insertion point through the inverse page CTM itself).
#[derive(Debug, Clone, PartialEq)]
pub struct PdfEmbedWord {
    pub text: String,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// Group one line's characters into words at whitespace boundaries --
/// the same split the reader's double-click selection performs, so what
/// gets embedded is what the reader would select.
pub fn words_from_line(line: &PdfTextLine) -> Vec<PdfEmbedWord> {
    let mut words: Vec<PdfEmbedWord> = Vec::new();
    let mut current: Option<PdfEmbedWord> = None;
    for ch in &line.chars {
        if ch.ch.is_whitespace() {
            if let Some(word) = current.take() {
                words.push(word);
            }
            continue;
        }
        // Quads are normalized (min/max) so a char whose corners are
        // listed in the other order cannot shrink the word's box.
        let cx0 = ch.x0.min(ch.x1);
        let cx1 = ch.x0.max(ch.x1);
        let cy0 = ch.y0.min(ch.y1);
        let cy1 = ch.y0.max(ch.y1);
        match current.as_mut() {
            Some(word) => {
                word.text.push(ch.ch);
                word.x0 = word.x0.min(cx0);
                word.y0 = word.y0.min(cy0);
                word.x1 = word.x1.max(cx1);
                word.y1 = word.y1.max(cy1);
            }
            None => {
                current = Some(PdfEmbedWord {
                    text: ch.ch.to_string(),
                    x0: cx0,
                    y0: cy0,
                    x1: cx1,
                    y1: cy1,
                });
            }
        }
    }
    if let Some(word) = current.take() {
        words.push(word);
    }
    words
}

/// Where and how big one word's invisible text is.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WordPlacement {
    /// Baseline start, in view coordinates.
    pub x: f32,
    pub y: f32,
    /// Font size in points.
    pub fontsize: f32,
}

/// The insertion point and size for one word's invisible text.
///
/// The fontsize is four fifths of the quad height and the baseline sits
/// a fifth of the height above the quad's bottom: the descender
/// allowance keeps the invisible glyphs inside the visual line, so
/// other applications' selection highlights land on the scan's ink
/// rather than on the line below it. A degenerate (zero-height) quad
/// clamps to a minimal size instead of collapsing to nothing, which
/// the PDF text operators would reject.
pub fn word_placement(word: &PdfEmbedWord) -> WordPlacement {
    let height = word.y1 - word.y0;
    if height > 0.0 {
        WordPlacement {
            x: word.x0,
            y: word.y1 - 0.2 * height,
            fontsize: (0.8 * height).max(1.0),
        }
    } else {
        WordPlacement { x: word.x0, y: word.y1, fontsize: 1.0 }
    }
}

/// What embedding did to a document.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct EmbedOutcome {
    /// Pages that received their invisible text layer.
    pub embedded_pages: usize,
    /// Pages left alone because their rotation is not a multiple of
    /// 180 degrees: the writing API emits glyphs along the unrotated
    /// axis and clips against the unrotated media box, so on a 90/270
    /// degree page the words would run sideways across the scan and
    /// some would be silently clipped. Skipped and reported, never
    /// corrupted.
    pub rotated_skipped: usize,
    /// Words not written: control-character-only noise, or characters
    /// the base-14 font's encoding cannot represent (OCR noise, exotic
    /// glyphs). The rest of the page is still embedded.
    pub skipped_words: usize,
}

/// Write the recognized `pages` (1-indexed) into `doc` as invisible
/// text. Pure with respect to the file system: the caller saves.
///
/// `on_page(done, total)` is called before each page and may return
/// `false` to stop early (cancellation); stopping leaves the document
/// partially edited in memory, which is fine because the caller never
/// saves a stopped run.
///
/// Returns the page numbers actually embedded (the verification step
/// samples from these) alongside the aggregate outcome.
pub fn embed_pages(
    doc: &mut mupdf::pdf::PdfDocument,
    pages: &[(usize, PdfPageText)],
    on_page: &mut dyn FnMut(usize, usize) -> bool,
) -> anyhow::Result<(Vec<usize>, EmbedOutcome)> {
    let mut outcome = EmbedOutcome::default();
    let mut embedded: Vec<usize> = Vec::new();
    let total = pages.len();
    for (done, &(page, ref text)) in pages.iter().enumerate() {
        if !on_page(done + 1, total) {
            break;
        }
        // load_pdf_page is 0-indexed; our page numbers are 1-indexed.
        let mut pdf_page = doc
            .load_pdf_page(page as i32 - 1)
            .map_err(|e| anyhow!("page {page}: {e}"))?;
        // Rotated pages are skipped honestly (see EmbedOutcome).
        if pdf_page.rotation().unwrap_or(0) % 180 != 0 {
            outcome.rotated_skipped += 1;
            continue;
        }
        let words: Vec<PdfEmbedWord> =
            text.lines.iter().flat_map(words_from_line).collect();
        if words.is_empty() {
            continue;
        }
        let mut shape = mupdf::shape::Shape::new(&mut pdf_page)
            .map_err(|e| anyhow!("page {page}: {e}"))?;
        for word in &words {
            let place = word_placement(word);
            // Control characters are never text. Everything else is
            // tried as-is; the default font covers Latin-1 plus the
            // common punctuation.
            let cleaned: String =
                word.text.chars().filter(|c| !c.is_control()).collect();
            if cleaned.is_empty() {
                outcome.skipped_words += 1;
                continue;
            }
            let opts = mupdf::shape::TextOptions {
                fontsize: place.fontsize,
                render_mode: 3, // invisible: the searchable-PDF text mode
                ..Default::default()
            };
            match shape.insert_text(
                mupdf::Point::new(place.x, place.y),
                &cleaned,
                &opts,
            ) {
                Ok(_) => {}
                Err(_) => {
                    // The font cannot encode something in this word.
                    // Skip the word, keep the page.
                    outcome.skipped_words += 1;
                }
            }
        }
        shape
            .commit(doc, true)
            .map_err(|e| anyhow!("page {page}: {e}"))?;
        outcome.embedded_pages += 1;
        embedded.push(page);
    }
    Ok((embedded, outcome))
}

/// What the embed task decided, in words a reader can act on.
#[derive(Debug, Clone)]
pub enum EmbedDone {
    /// The file now carries its text; the untouched original is kept at
    /// the backup path.
    Applied {
        embedded_pages: usize,
        rotated_skipped: usize,
        skipped_words: usize,
        backup: PathBuf,
        path: PathBuf,
    },
    /// Some scanned pages have no recognized text yet. Owner rule:
    /// complete recognition first. Nothing was written.
    NotReady { missing: usize },
    /// Every page already carries text (a digital PDF). Nothing to do.
    NothingToEmbed,
    /// Cancelled before the swap; nothing was written.
    Cancelled,
    /// Something failed. The original file is untouched.
    Failed(String),
}

/// Start the embed as a task-manager job: cancellable before the swap,
/// progress per page. `on_applied` runs on the main loop after a
/// successful swap, with the file's path — an open reader uses it to
/// reload the document; other callers may ignore it.
pub fn enqueue(
    catalog: Arc<Catalog>,
    book_id: i64,
    title: String,
    path: PathBuf,
    on_applied: impl Fn(&Path) + 'static,
) {
    gtk::glib::MainContext::default().invoke(move || {
        crate::tasks::spawn(
            format!("Embedding text in {title}"),
            move |reporter| run_embed(catalog, book_id, path, reporter),
            |_update| {},
            move |done: EmbedDone| {
                match &done {
                    EmbedDone::Applied {
                        embedded_pages,
                        rotated_skipped,
                        skipped_words,
                        backup,
                        path,
                    } => {
                        let mut detail = format!(
                            "{embedded_pages} pages now carry their text. The untouched original is kept as {}.",
                            backup.display()
                        );
                        if *rotated_skipped > 0 {
                            detail.push_str(&format!(
                                " {rotated_skipped} rotated page(s) were left untouched."
                            ));
                        }
                        if *skipped_words > 0 {
                            detail.push_str(&format!(
                                " {skipped_words} word(s) had characters the PDF text layer cannot store."
                            ));
                        }
                        crate::notify::info("Text embedded", &detail);
                        on_applied(path);
                    }
                    EmbedDone::NotReady { missing } => {
                        crate::notify::info(
                            "Not recognized yet",
                            &format!(
                                "{missing} page(s) still have no recognized text. \
                                 Let recognition finish first; nothing was changed."
                            ),
                        );
                    }
                    EmbedDone::NothingToEmbed => {
                        crate::notify::info(
                            "Nothing to embed",
                            "Every page already carries text; there is nothing to add.",
                        );
                    }
                    EmbedDone::Cancelled => {}
                    EmbedDone::Failed(why) => {
                        crate::notify::error("Embedding failed", why);
                    }
                }
            },
        );
    });
}

/// The task body, on the worker thread.
fn run_embed(
    catalog: Arc<Catalog>,
    book_id: i64,
    path: PathBuf,
    reporter: Reporter,
) -> EmbedDone {
    match try_run_embed(&catalog, book_id, &path, &reporter) {
        Ok(done) => done,
        Err(why) => {
            reporter.fail();
            EmbedDone::Failed(why.to_string())
        }
    }
}

fn try_run_embed(
    catalog: &Catalog,
    book_id: i64,
    path: &Path,
    reporter: &Reporter,
) -> anyhow::Result<EmbedDone> {
    // 1. The fingerprint the cache rows are keyed by. Computed here, on
    //    the worker -- the UI thread never stats files (2.20).
    let Some(fingerprint) = crate::pages::pdf_reader::pdf_path_fingerprint(path)
    else {
        anyhow::bail!("could not read {}", path.display());
    };

    // 2. The cached OCR pages for this exact file, in one query.
    let cached: HashMap<usize, PdfPageText> = catalog
        .load_page_ocr_pages(book_id, &fingerprint)
        .map_err(|e| anyhow!("could not read the OCR cache: {e}"))?
        .into_iter()
        .collect();

    // 3. One read pass: which pages need text, and are they all ready?
    //    A page that already has vector text is never touched.
    let read_doc = crate::pdf::PdfDocument::open(path)?;
    let page_count = read_doc.page_count();
    let mut to_embed: Vec<(usize, PdfPageText)> = Vec::new();
    let mut missing = 0usize;
    for page in 1..=page_count {
        if reporter.cancelled() {
            return Ok(EmbedDone::Cancelled);
        }
        // A 5,000-page scan takes a while just to check; show that too.
        if page % 100 == 0 {
            reporter.step(page, page_count, format!("checking page {page} of {page_count}"));
        }
        let has_vector_text = read_doc
            .extract_page_text(page)
            .map(|t| t.total_chars() > 0)
            .unwrap_or(false);
        if has_vector_text {
            continue;
        }
        match cached.get(&page) {
            Some(text) => to_embed.push((page, text.clone())),
            None => missing += 1,
        }
    }

    // Owner rule: the embed needs the whole book recognized first.
    // Reporting the count here, rather than pre-computing readiness
    // when the button is drawn, keeps the check fresh and the UI free
    // of file access.
    if missing > 0 {
        return Ok(EmbedDone::NotReady { missing });
    }
    if to_embed.is_empty() {
        return Ok(EmbedDone::NothingToEmbed);
    }

    // 4. The write pass, entirely in memory.
    let path_str = path.to_string_lossy();
    let mut edit_doc = mupdf::pdf::PdfDocument::open(&*path_str)
        .map_err(|e| anyhow!("could not open {} for editing: {e}", path.display()))?;
    let (embedded, outcome) =
        embed_pages(&mut edit_doc, &to_embed, &mut |done, total| {
            reporter.step(done, total, format!("embedding page {done} of {total}"));
            !reporter.cancelled()
        })?;
    if reporter.cancelled() {
        // Stopped mid-pass: the in-memory edits are simply dropped.
        return Ok(EmbedDone::Cancelled);
    }

    // 5. Save beside the original, verify, then swap. A stale backup
    //    from an earlier embed is replaced: it is one step further
    //    from the current original than the new one will be.
    let tmp = sibling(path, ".kalam-embed.tmp");
    let backup = sibling(path, ".bak");
    let sources = to_embed.clone();
    apply_rewrite(path, &tmp, &backup, &mut edit_doc, &|rewritten| {
        verify_embedded(rewritten, page_count, &embedded, &sources)
    })?;

    // 6. The old fingerprint's rows can never be read again (the file
    //    changed); drop them so the cache does not carry dead weight.
    if let Err(e) = catalog.delete_page_ocr(book_id, &fingerprint) {
        log::warn!("could not drop the old OCR cache rows: {e}");
    }

    Ok(EmbedDone::Applied {
        embedded_pages: outcome.embedded_pages,
        rotated_skipped: outcome.rotated_skipped,
        skipped_words: outcome.skipped_words,
        backup,
        path: path.to_path_buf(),
    })
}

/// A sibling path with a suffix appended to the full file name
/// ("Book.pdf" -> "Book.pdf.bak").
fn sibling(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_os_string();
    name.push(suffix);
    PathBuf::from(name)
}

/// Write options for the rewrite: touch as little as possible. A full
/// rewrite (never incremental), no garbage collection or clean-up pass,
/// and no recompression of the scanned images or existing fonts -- the
/// images are the book; re-encoding them is how "add text" turns into
/// "degrade the scans".
fn conservative_write_options() -> mupdf::pdf::PdfWriteOptions {
    let mut options = mupdf::pdf::PdfWriteOptions::default();
    options
        .set_incremental(false)
        .set_pretty(false)
        .set_ascii(false)
        .set_compress(true)
        .set_compress_images(false)
        .set_compress_fonts(false)
        .set_decompress(false)
        .set_garbage(false)
        .set_clean(false)
        .set_sanitize(false)
        .set_appearance(false)
        .set_linear(false);
    options
}

/// Save the rewritten document to `tmp`, verify it, then swap it into
/// `orig` keeping the previous file as `backup`. If either the verify
/// or the swap fails, the original is left exactly as it was and the
/// temporary file is removed. The verify hook is injectable so the
/// rollback path can be tested.
fn apply_rewrite(
    orig: &Path,
    tmp: &Path,
    backup: &Path,
    doc: &mut mupdf::pdf::PdfDocument,
    verify: &dyn Fn(&Path) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    {
        let file = std::fs::File::create(tmp)?;
        let mut writer = std::io::BufWriter::new(file);
        doc.write_to_with_options(&mut writer, conservative_write_options())
            .map_err(|e| anyhow!("could not write the rewritten file: {e}"))?;
        writer.flush()?;
    }
    if let Err(why) = verify(tmp) {
        let _ = std::fs::remove_file(tmp);
        anyhow::bail!("the rewritten file did not verify: {why}");
    }
    std::fs::rename(orig, backup)?;
    if let Err(e) = std::fs::rename(tmp, orig) {
        // Put the untouched original back; the embed failed safely.
        let _ = std::fs::rename(backup, orig);
        let _ = std::fs::remove_file(tmp);
        anyhow::bail!("could not move the rewritten file into place: {e}");
    }
    Ok(())
}

/// Reopen the rewritten file and check it: same page count, and the
/// first, middle and last embedded pages still carry the words we
/// wrote. A rewrite that loses pages or text is never swapped in.
fn verify_embedded(
    path: &Path,
    expected_pages: usize,
    embedded: &[usize],
    sources: &[(usize, PdfPageText)],
) -> anyhow::Result<()> {
    let doc = crate::pdf::PdfDocument::open(path)?;
    if doc.page_count() != expected_pages {
        anyhow::bail!(
            "page count changed from {expected_pages} to {}",
            doc.page_count()
        );
    }
    let mut samples: Vec<usize> = Vec::new();
    if let Some(&first) = embedded.first() {
        samples.push(first);
    }
    if embedded.len() > 2 {
        samples.push(embedded[embedded.len() / 2]);
    }
    if embedded.len() > 1 {
        if let Some(&last) = embedded.last() {
            samples.push(last);
        }
    }
    for &page in &samples {
        let extracted = doc
            .extract_page_text(page)
            .map_err(|e| anyhow!("page {page}: {e}"))?;
        if extracted.total_chars() == 0 {
            anyhow::bail!("page {page} lost its text in the rewrite");
        }
        // At least one known word must come back: the longest plain
        // alphanumeric word the OCR produced for that page.
        let needle = sources
            .iter()
            .find(|(p, _)| *p == page)
            .and_then(|(_, text)| {
                text.lines
                    .iter()
                    .flat_map(words_from_line)
                    .map(|w| w.text)
                    .filter(|w| {
                        w.chars().count() >= 4
                            && w.chars().all(|c| c.is_ascii_alphanumeric())
                    })
                    .max_by_key(|w| w.chars().count())
            });
        if let Some(needle) = needle {
            let haystack: String = extracted
                .lines
                .iter()
                .flat_map(|l| l.chars.iter().map(|c| c.ch))
                .collect::<String>()
                .to_lowercase();
            if !haystack.contains(&needle.to_lowercase()) {
                anyhow::bail!(
                    "page {page} does not contain the word \"{needle}\" after the rewrite"
                );
            }
        }
    }
    Ok(())
}

/// The explicit confirmation the owner requires (2.21): plain words,
/// what will happen, the backup promise, Cancel first. Built on the
/// app's in-app dialog, like the remaster confirm.
pub fn present_confirm(
    anchor: &impl gtk::prelude::IsA<gtk::Widget>,
    book_title: &str,
    on_confirm: impl Fn() + 'static,
) {
    // The confirm handler is an `Fn` closure and must be callable more
    // than zero times by GTK's handler type; sharing it through an Rc
    // hands each invocation a fresh caller. The dialog closes on
    // confirm, so the job still runs at most once.
    let on_confirm = std::rc::Rc::new(on_confirm);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 14);

    let intro = gtk::Label::new(Some(&format!(
        "Write the recognized text of \u{201c}{book_title}\u{201d} into the PDF itself?"
    )));
    intro.set_halign(gtk::Align::Start);
    intro.set_wrap(true);
    intro.set_xalign(0.0);
    intro.add_css_class("kalam-card-meta");
    body.append(&intro);

    let note = gtk::Label::new(Some(
        "The words become an invisible layer over each scanned page, so search, \
         selection and copy work in any reader. The whole book must be \
         recognized first. The file is modified in place; the untouched \
         original is kept beside it as a .bak backup.",
    ));
    note.set_halign(gtk::Align::Start);
    note.set_wrap(true);
    note.set_xalign(0.0);
    note.add_css_class("kalam-muted");
    body.append(&note);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    actions.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    cancel.add_css_class("kalam-secondary-btn");
    let confirm = gtk::Button::with_label("Embed text");
    confirm.add_css_class("kalam-btn-filled");
    actions.append(&cancel);
    actions.append(&confirm);
    body.append(&actions);

    let Some(dialog) = crate::widgets::in_app_dialog::present(
        anchor,
        "Embed recognized text",
        crate::widgets::in_app_dialog::DialogExit::OwnButtons,
        &body,
    ) else {
        crate::notify::error(
            "Could not show the embed dialog",
            "Please try again once the page has finished loading.",
        );
        return;
    };

    {
        let dialog = dialog.clone();
        cancel.connect_clicked(move |_| dialog.close());
    }
    {
        let dialog = dialog.clone();
        confirm.connect_clicked(move |_| {
            dialog.close();
            let on_confirm = on_confirm.clone();
            on_confirm();
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pdf::{PdfTextChar, PdfTextLine};

    fn char_quad(ch: char, x0: f32, y0: f32, x1: f32, y1: f32) -> PdfTextChar {
        PdfTextChar { ch, x0, y0, x1, y1 }
    }

    /// One line of evenly spaced text, `advance` points per character,
    /// baseline-ish quads of height `h` starting at (`x`, `y`).
    fn synthetic_line(text: &str, x: f32, y: f32, h: f32, advance: f32) -> PdfTextLine {
        let mut chars = Vec::new();
        for (i, ch) in text.chars().enumerate() {
            let cx = x + i as f32 * advance;
            chars.push(char_quad(ch, cx, y, cx + advance * 0.9, y + h));
        }
        let text_string: String = chars.iter().map(|c| c.ch).collect();
        PdfTextLine {
            text: text_string,
            x0: x,
            y0: y,
            x1: x + text.chars().count() as f32 * advance,
            y1: y + h,
            chars,
        }
    }

    fn one_page(lines: Vec<PdfTextLine>) -> PdfPageText {
        PdfPageText { page_num: 1, width_pts: 595.0, height_pts: 842.0, lines }
    }

    #[test]
    fn words_from_line_splits_at_whitespace() {
        let line = synthetic_line("one two", 72.0, 100.0, 10.0, 6.0);
        let words = words_from_line(&line);
        assert_eq!(words.len(), 2);
        assert_eq!(words[0].text, "one");
        assert_eq!(words[1].text, "two");
        // Each word's quad covers exactly its own characters.
        assert!((words[0].x0 - 72.0).abs() < 0.01);
        assert!((words[0].y0 - 100.0).abs() < 0.01);
        assert!(words[1].x0 > words[0].x1);
        // A trailing space flushes the last word, not an empty one.
        let line = synthetic_line("end ", 10.0, 10.0, 8.0, 5.0);
        let words = words_from_line(&line);
        assert_eq!(words.len(), 1);
        assert_eq!(words[0].text, "end");
    }

    #[test]
    fn word_placement_stays_inside_the_quad() {
        let word = PdfEmbedWord {
            text: "specimen".into(),
            x0: 72.0,
            y0: 100.0,
            x1: 120.0,
            y1: 110.0,
        };
        let place = word_placement(&word);
        assert!((place.fontsize - 8.0).abs() < 0.01); // 0.8 * height
        assert!((place.y - 108.0).abs() < 0.01); // y1 - 0.2 * height
        assert!((place.x - 72.0).abs() < 0.01);
        // A degenerate quad clamps to a minimal, valid font size.
        let flat = PdfEmbedWord { text: "x".into(), x0: 5.0, y0: 50.0, x1: 6.0, y1: 50.0 };
        let place = word_placement(&flat);
        assert!((place.fontsize - 1.0).abs() < 0.001);
        assert!((place.y - 50.0).abs() < 0.001);
    }

    #[test]
    fn embed_round_trip_with_rewrite_and_verify() -> anyhow::Result<()> {
        let orig = std::env::temp_dir().join(format!(
            "kalam-embed-rt-{}.pdf",
            std::process::id()
        ));
        // A one-page scanned-style PDF: a blank page, no text.
        {
            let mut base = mupdf::pdf::PdfDocument::new();
            base.new_page(mupdf::Size::A4)?;
            base.save(orig.to_str().unwrap())?;
        }

        let text = one_page(vec![synthetic_line(
            "searchable specimen text",
            72.0,
            100.0,
            10.0,
            6.0,
        )]);

        let path_str = orig.to_string_lossy();
        let mut doc = mupdf::pdf::PdfDocument::open(&*path_str)?;
        let (embedded, outcome) =
            embed_pages(&mut doc, &[(1, text.clone())], &mut |_, _| true)?;
        assert_eq!(embedded, vec![1]);
        assert_eq!(outcome.embedded_pages, 1);
        assert_eq!(outcome.rotated_skipped, 0);
        assert_eq!(outcome.skipped_words, 0);

        // Rewrite, verify, swap -- the real pipeline.
        let tmp = sibling(&orig, ".kalam-embed.tmp");
        let backup = sibling(&orig, ".bak");
        apply_rewrite(&orig, &tmp, &backup, &mut doc, &|rewritten| {
            verify_embedded(rewritten, 1, &embedded, &[(1, text.clone())])
        })?;
        assert!(backup.exists());
        assert!(!tmp.exists());

        // The swapped-in file is the one carrying the text.
        let reopened = crate::pdf::PdfDocument::open(&orig)?;
        let extracted = reopened.extract_page_text(1)?;
        assert!(extracted.total_chars() > 0);
        let haystack: String = extracted
            .lines
            .iter()
            .flat_map(|l| l.chars.iter().map(|c| c.ch))
            .collect::<String>()
            .to_lowercase();
        assert!(haystack.contains("specimen"));

        let _ = std::fs::remove_file(&orig);
        let _ = std::fs::remove_file(&backup);
        Ok(())
    }

    #[test]
    fn rotated_page_is_skipped() -> anyhow::Result<()> {
        let mut doc = mupdf::pdf::PdfDocument::new();
        {
            let mut page = doc.new_page(mupdf::Size::A4)?;
            page.set_rotation(90)?;
        }
        let text = one_page(vec![synthetic_line("sideways", 72.0, 100.0, 10.0, 6.0)]);
        let (embedded, outcome) =
            embed_pages(&mut doc, &[(1, text)], &mut |_, _| true)?;
        assert!(embedded.is_empty());
        assert_eq!(outcome.embedded_pages, 0);
        assert_eq!(outcome.rotated_skipped, 1);
        Ok(())
    }

    #[test]
    fn apply_rewrite_rolls_back_when_verify_fails() -> anyhow::Result<()> {
        let orig = std::env::temp_dir().join(format!(
            "kalam-embed-rb-{}.pdf",
            std::process::id()
        ));
        {
            let mut base = mupdf::pdf::PdfDocument::new();
            base.new_page(mupdf::Size::A4)?;
            base.save(orig.to_str().unwrap())?;
        }
        let before = std::fs::read(&orig)?;

        let path_str = orig.to_string_lossy();
        let mut doc = mupdf::pdf::PdfDocument::open(&*path_str)?;
        let tmp = sibling(&orig, ".kalam-embed.tmp");
        let backup = sibling(&orig, ".bak");
        let result = apply_rewrite(&orig, &tmp, &backup, &mut doc, &|_| {
            anyhow::bail!("injected verify failure")
        });

        assert!(result.is_err());
        // The original is untouched and no leftover files remain.
        assert_eq!(std::fs::read(&orig)?, before);
        assert!(!tmp.exists());
        assert!(!backup.exists());

        let _ = std::fs::remove_file(&orig);
        Ok(())
    }
}
