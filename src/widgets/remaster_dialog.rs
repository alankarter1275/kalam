//! The comic remaster dialog.
//!
//! Owner rule, 2026-09-29: every option in a remaster is the user's to
//! decide — nothing is hardcoded. The dialog collects the choices and runs
//! the job; the engine itself is `comics::remaster_comic_archive`.
//!
//! Both comic-capable book pages (the detail page and the floating bubble
//! reader) present this same dialog, so the two never drift apart.

use crate::models::Book;
use gtk::prelude::*;

/// Show the dialog and, on confirm, start the background remaster.
///
/// `on_finished` runs back on the main thread once the job ends — whether
/// it succeeded or failed — so the calling page can refresh whatever it
/// shows about the file. A cancel does not call it: nothing changed.
pub fn present(
    anchor: &impl IsA<gtk::Widget>,
    book: &Book,
    catalog: &std::sync::Arc<crate::db::Catalog>,
    on_finished: impl Fn() + 'static,
) {
    // The confirm button's handler is an `Fn` closure, so it cannot hand
    // `on_finished` away by value more than zero times. Wrapping it in an
    // Rc lets each invocation pass a fresh caller that borrows the same
    // callback — the job runs at most once per dialog either way.
    let on_finished = std::rc::Rc::new(on_finished);
    // The button handler must own what it uses: it outlives this call.
    let book = book.clone();
    let catalog = catalog.clone();

    // Page count, for the info line and the range spinners' limits. A
    // file that cannot be opened still opens the dialog; the job itself
    // will say why it failed, in the same words a reader would.
    let page_count = crate::comics::list_comic_pages(&book.file_path)
        .map(|pages| pages.len())
        .ok();

    let body = gtk::Box::new(gtk::Orientation::Vertical, 14);

    let intro = gtk::Label::new(Some(&match page_count {
        Some(n) => format!("“{}” — {} pages. Untouched pages are copied through unchanged.", book.title, n),
        None => format!("“{}”.", book.title),
    }));
    intro.set_halign(gtk::Align::Start);
    intro.set_wrap(true);
    intro.set_xalign(0.0);
    intro.add_css_class("kalam-card-meta");
    body.append(&intro);

    // --- Scale ---------------------------------------------------------
    let scale_drop = gtk::DropDown::from_strings(&["1.5× — gentle", "2× — recommended", "3× — heavy"]);
    scale_drop.set_selected(1);
    body.append(&row("How much to enlarge", &scale_drop));

    // --- Which pages ---------------------------------------------------
    let pages_drop = gtk::DropDown::from_strings(&["All pages", "Only small pages", "A page range"]);

    let width_spin = gtk::SpinButton::with_range(100.0, 8000.0, 50.0);
    width_spin.set_value(1400.0);
    let width_row = row("Upscale pages narrower than (pixels)", &width_spin);

    let max_page = page_count.unwrap_or(9999).max(1);
    let from_spin = gtk::SpinButton::with_range(1.0, max_page as f64, 1.0);
    from_spin.set_value(1.0);
    let to_spin = gtk::SpinButton::with_range(1.0, max_page as f64, 1.0);
    to_spin.set_value(max_page as f64);
    let range_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    range_box.append(&from_spin);
    range_box.append(&gtk::Label::new(Some("to")));
    range_box.append(&to_spin);
    let range_row = row("Pages (first to last)", &range_box);

    // Only the controls belonging to the selected mode are shown.
    width_row.set_visible(false);
    range_row.set_visible(false);
    {
        let width_row = width_row.clone();
        let range_row = range_row.clone();
        pages_drop.connect_selected_notify(move |drop| {
            width_row.set_visible(drop.selected() == 1);
            range_row.set_visible(drop.selected() == 2);
        });
    }
    body.append(&row("Which pages", &pages_drop));
    body.append(&width_row);
    body.append(&range_row);

    // --- Output --------------------------------------------------------
    let output_drop = gtk::DropDown::from_strings(&[
        "Replace the original — keeps a .bak backup",
        "Save a copy next to the original",
    ]);
    body.append(&row("Where the result goes", &output_drop));

    // --- Format --------------------------------------------------------
    let format_drop = gtk::DropDown::from_strings(&["JPEG — smaller files", "PNG — lossless, larger"]);

    let quality_spin = gtk::SpinButton::with_range(60.0, 100.0, 1.0);
    quality_spin.set_value(90.0);
    let quality_row = row("JPEG quality", &quality_spin);
    {
        let quality_row = quality_row.clone();
        format_drop.connect_selected_notify(move |drop| {
            quality_row.set_visible(drop.selected() == 0);
        });
    }
    body.append(&row("Page format", &format_drop));
    body.append(&quality_row);

    let note = gtk::Label::new(Some(
        "Runs in the background with progress. Lanczos3 keeps ink edges sharp; on a slow machine expect roughly half a second to a second per upscaled page.",
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
    let confirm = gtk::Button::with_label("Remaster");
    confirm.add_css_class("kalam-btn-filled");
    actions.append(&cancel);
    actions.append(&confirm);
    body.append(&actions);

    let Some(dialog) = crate::widgets::in_app_dialog::present(
        anchor,
        "Remaster comic",
        crate::widgets::in_app_dialog::DialogExit::OwnButtons,
        &body,
    ) else {
        crate::notify::error(
            "Could not show the remaster dialog",
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
            let scale = match scale_drop.selected() {
                0 => 1.5f32,
                1 => 2.0,
                _ => 3.0,
            };
            let pages = match pages_drop.selected() {
                0 => crate::comics::RemasterPages::All,
                1 => crate::comics::RemasterPages::BelowWidth(width_spin.value_as_int().max(100) as u32),
                _ => {
                    let from = from_spin.value_as_int().max(1) as usize;
                    let to = to_spin.value_as_int().max(1) as usize;
                    crate::comics::RemasterPages::Range { from: from.min(to), to: from.max(to) }
                }
            };
            let format = if format_drop.selected() == 0 {
                crate::comics::RemasterFormat::Jpeg(quality_spin.value_as_int().clamp(60, 100) as u8)
            } else {
                crate::comics::RemasterFormat::Png
            };
            let replace = output_drop.selected() == 0;

            dialog.close();
            let on_finished = on_finished.clone();
            start_remaster(&book, &catalog, scale, pages, format, replace, move || on_finished());
        });
    }
}

/// One labelled control, stacked vertically so the dialog stays narrow.
fn row(label: &str, control: &impl IsA<gtk::Widget>) -> gtk::Box {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let lbl = gtk::Label::new(Some(label));
    lbl.set_halign(gtk::Align::Start);
    lbl.add_css_class("kalam-section-label");
    box_.append(&lbl);
    box_.append(control);
    box_
}

/// Run the chosen remaster as a background task.
///
/// The result is `Ok(None)` for a cancel (nothing changed, no toast-worthy
/// failure), `Ok(Some(message))` for success, `Err` for a real failure.
fn start_remaster(
    book: &Book,
    catalog: &std::sync::Arc<crate::db::Catalog>,
    scale: f32,
    pages: crate::comics::RemasterPages,
    format: crate::comics::RemasterFormat,
    replace: bool,
    on_finished: impl Fn() + 'static,
) {
    let book = book.clone();
    let catalog = catalog.clone();
    let opts = crate::comics::RemasterOptions { scale, pages, format };
    let ext = book
        .file_path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_string())
        .unwrap_or_else(|| "cbz".to_string());

    crate::tasks::spawn(
        format!("Remastering {}", book.title),
        move |reporter| -> Result<Option<String>, String> {
            let path = book.file_path.clone();
            let dir = path
                .parent()
                .ok_or_else(|| "the book's folder could not be determined".to_string())?
                .to_path_buf();
            let tmp = dir.join(format!(".kalam-remaster-tmp.{ext}"));

            let outcome = (|| -> Result<Option<String>, String> {
                let report = crate::comics::remaster_comic_archive(&path, &tmp, &opts, |done, total| {
                    reporter.step(done, total, format!("Page {done} of {total}"));
                    !reporter.cancelled()
                })
                .map_err(|e| e.to_string())?;

                if replace {
                    // Backup first, then swap. If the swap fails, the
                    // original goes back: a remaster must never leave a
                    // book without its file.
                    let bak = dir.join(format!("book.{ext}.bak"));
                    std::fs::rename(&path, &bak)
                        .map_err(|e| format!("could not back up the original: {e}"))?;
                    if let Err(e) = std::fs::rename(&tmp, &path) {
                        let _ = std::fs::rename(&bak, &path);
                        return Err(format!("could not move the remastered file into place: {e}"));
                    }
                    // The file's bytes changed, so the stored hash is stale.
                    // Left alone that would break duplicate detection on
                    // re-import and orphan the remembered metadata edits —
                    // the same reasons EPUB metadata writes re-hash.
                    let new_hash = crate::db::hash_file(&path)
                        .map_err(|e| format!("could not re-read the book's hash: {e}"))?;
                    catalog
                        .rehash_book(book.id, &book.file_hash, &new_hash)
                        .map_err(|e| format!("could not update the book record: {e}"))?;
                    Ok(Some(format!(
                        "Upscaled {} of {} pages at {scale}×. The original is kept as book.{ext}.bak.",
                        report.pages_remastered, report.pages_total
                    )))
                } else {
                    let out = dir.join(format!("remastered-{scale}x.{ext}"));
                    std::fs::rename(&tmp, &out)
                        .map_err(|e| format!("could not save the copy: {e}"))?;
                    let name = out
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "a new file".to_string());
                    Ok(Some(format!(
                        "Upscaled {} of {} pages at {scale}× into {name}.",
                        report.pages_remastered, report.pages_total
                    )))
                }
            })();

            if outcome.is_err() {
                // The engine removes its partial output on cancel; this
                // catches every other failure path too.
                let _ = std::fs::remove_file(&tmp);
            }
            outcome
        },
        |_| {},
        move |res| match res {
            Ok(Some(message)) => {
                crate::notify::success("Comic remastered", &message);
                on_finished();
            }
            Ok(None) => {
                crate::notify::info("Remaster cancelled", "Nothing was changed.");
            }
            Err(err) => {
                crate::notify::error("Remaster failed", &err);
                on_finished();
            }
        },
    );
}
