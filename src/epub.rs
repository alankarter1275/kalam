//! Minimal EPUB import: OPF metadata + cover extraction.

use crate::db::{self, Catalog};
use crate::models::BookFormat;
use crate::paths::{book_dir, ensure_data_dirs};
use anyhow::{anyhow, Context, Result};
use roxmltree::Document;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
use uuid::Uuid;
use zip::ZipArchive;

#[derive(Debug)]
pub struct ImportResult {
    /// True when hand-edited metadata was re-applied from a previous import.
    pub restored: bool,
    #[allow(dead_code)]
    pub book_id: i64,
    pub title: String,
    pub duplicate: bool,
}

#[derive(Debug, Default)]
struct OpfMeta {
    title: Option<String>,
    authors: Vec<String>,
    description: Option<String>,
    series: Option<String>,
    subjects: Vec<String>,
    cover_href: Option<String>,
    cover_id: Option<String>,
    /// manifest id -> href
    manifest: Vec<(String, String, Option<String>)>, // id, href, media-type
}

/// Import an EPUB path into the catalog. Copies into library storage.
pub fn import_epub(catalog: &Catalog, source: &Path) -> Result<ImportResult> {
    ensure_data_dirs()?;
    if !source.is_file() {
        return Err(anyhow!("not a file: {}", source.display()));
    }
    let ext = source
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    if ext != "epub" && ext != "cbz" && ext != "cbr" {
        return Err(anyhow!("unsupported file format .{ext} (expected .epub, .cbz, or .cbr)"));
    }

    let hash = db::hash_file(source)?;
    if let Some(existing) = catalog.find_by_hash(&hash)? {
        let title = catalog
            .get_book(existing)?
            .map(|b| b.title)
            .unwrap_or_else(|| "Existing book".into());
        return Ok(ImportResult {
            book_id: existing,
            title,
            duplicate: true,
            restored: false,
        });
    }

    let (title, authors, description, series, tags, format, file_name, cover_name) =
        if ext == "cbz" || ext == "cbr" {
            let title = source
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled Comic".into());
            let authors = "Unknown".to_string();
            let format = if ext == "cbz" {
                BookFormat::Cbz
            } else {
                BookFormat::Cbr
            };
            let file_name = format!("book.{ext}");

            let cover_name = if let Ok(cover_bytes) = crate::comics::extract_comic_cover(source) {
                let img_ext = guess_image_ext(&cover_bytes);
                let cover_file = format!("cover.{img_ext}");
                // Will be written once dest_dir is created below
                Some((cover_file, cover_bytes))
            } else {
                None
            };

            (
                title,
                authors,
                String::new(),
                None,
                Vec::new(),
                format,
                file_name,
                cover_name,
            )
        } else {
            let meta = parse_epub_meta(source)?;
            let title = meta
                .title
                .as_ref()
                .map(|t| t.trim())
                .filter(|t| !t.is_empty())
                .map(|t| t.to_string())
                .unwrap_or_else(|| {
                    source
                        .file_stem()
                        .map(|s| s.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Untitled".into())
                });
            let authors = if meta.authors.is_empty() {
                "Unknown".to_string()
            } else {
                meta.authors.join(", ")
            };
            let description = strip_html(&meta.description.clone().unwrap_or_default());
            let series = meta.series.clone();
            let tags = meta.subjects.clone();

            (
                title,
                authors,
                description,
                series,
                tags,
                BookFormat::Epub,
                "book.epub".to_string(),
                None,
            )
        };

    let uuid = Uuid::new_v4().to_string();
    let dest_dir = book_dir(&uuid);
    fs::create_dir_all(&dest_dir)?;

    let dest_file = dest_dir.join(&file_name);
    fs::copy(source, &dest_file)
        .with_context(|| format!("copy {} → {}", source.display(), dest_file.display()))?;

    let final_cover_name = if format == BookFormat::Epub {
        let meta = parse_epub_meta(source)?;
        extract_cover(source, &meta, &dest_dir)?
    } else if let Some((cover_filename, cover_bytes)) = cover_name {
        if fs::write(dest_dir.join(&cover_filename), cover_bytes).is_ok() {
            Some(cover_filename)
        } else {
            None
        }
    } else {
        None
    };

    if let Some(cover) = &final_cover_name {
        crate::thumbs::generate_thumbnail(
            &dest_dir.join(cover),
            &crate::paths::thumbnail_path(&uuid),
        );
    }

    let id = catalog.insert_book(
        &uuid,
        &title,
        &authors,
        series.as_deref(),
        &description,
        format,
        &file_name,
        &hash,
        final_cover_name.as_deref(),
        &tags,
    )?;

    // P4: imports show up in History. Best-effort — a logging failure must not
    // undo an otherwise successful import.
    let _ = catalog.log_event(id, crate::db::EventKind::Imported, "");

    // If this exact file was in the library before and had hand-edited
    // metadata, put those edits back rather than silently reverting to
    // whatever the EPUB's OPF says.
    let restored = catalog.restore_overrides(id, &hash).unwrap_or(false);
    let title = if restored {
        catalog
            .get_book(id)
            .ok()
            .flatten()
            .map(|b| b.title)
            .unwrap_or(title)
    } else {
        title
    };

    // Write the book's kalam.json now, so a library is self-describing from
    // the moment a book enters it rather than only after its first edit.
    crate::sidecar::refresh_for_book(catalog, id);

    Ok(ImportResult {
        book_id: id,
        title,
        duplicate: false,
        restored,
    })
}

fn parse_epub_meta(path: &Path) -> Result<OpfMeta> {
    let file = File::open(path)?;
    let mut archive = ZipArchive::new(file)?;
    let opf_path = find_opf_path(&mut archive)?;
    let opf_xml = read_zip_string(&mut archive, &opf_path)?;
    let opf_dir = parent_zip_path(&opf_path);
    let mut meta = parse_opf(&opf_xml)?;
    // Resolve cover href relative to OPF directory.
    if let Some(href) = meta.cover_href.clone() {
        meta.cover_href = Some(join_zip_path(&opf_dir, &href));
    } else if let Some(id) = &meta.cover_id {
        if let Some((_, href, _)) = meta.manifest.iter().find(|(mid, _, _)| mid == id) {
            meta.cover_href = Some(join_zip_path(&opf_dir, href));
        }
    } else {
        // Heuristic: first image manifest item with "cover" in id/href.
        for (id, href, mt) in &meta.manifest {
            let is_image = mt
                .as_deref()
                .map(|m| m.starts_with("image/"))
                .unwrap_or_else(|| {
                    let h = href.to_ascii_lowercase();
                    h.ends_with(".jpg")
                        || h.ends_with(".jpeg")
                        || h.ends_with(".png")
                        || h.ends_with(".webp")
                        || h.ends_with(".gif")
                });
            if is_image
                && (id.to_ascii_lowercase().contains("cover")
                    || href.to_ascii_lowercase().contains("cover"))
            {
                meta.cover_href = Some(join_zip_path(&opf_dir, href));
                break;
            }
        }
    }
    Ok(meta)
}

/// Locate the OPF inside an EPUB. Exposed for the writer in `epub_metadata`.
pub fn find_opf_path_pub<R: Read + std::io::Seek>(archive: &mut ZipArchive<R>) -> Result<String> {
    find_opf_path(archive)
}

fn find_opf_path<R: Read + std::io::Seek>(archive: &mut ZipArchive<R>) -> Result<String> {
    // container.xml
    let container = read_zip_string(archive, "META-INF/container.xml")
        .or_else(|_| read_zip_string(archive, "meta-inf/container.xml"))?;
    let doc = Document::parse(&container).context("parse container.xml")?;
    for node in doc.descendants() {
        if node.tag_name().name() == "rootfile" {
            if let Some(full) = node.attribute("full-path") {
                return Ok(full.to_string());
            }
        }
    }
    Err(anyhow!("no rootfile in container.xml"))
}

fn parse_opf(xml: &str) -> Result<OpfMeta> {
    let doc = Document::parse(xml).context("parse OPF")?;
    let mut meta = OpfMeta::default();

    for node in doc.descendants() {
        let name = node.tag_name().name();
        match name {
            "title" if node.parent().map(|p| p.tag_name().name()) == Some("metadata") => {
                if meta.title.is_none() {
                    let t = node.text().unwrap_or("").trim();
                    if !t.is_empty() {
                        meta.title = Some(t.to_string());
                    }
                }
            }
            "creator" => {
                let t = node.text().unwrap_or("").trim();
                if !t.is_empty() {
                    meta.authors.push(t.to_string());
                }
            }
            "description" => {
                if meta.description.is_none() {
                    let t = node.text().unwrap_or("").trim();
                    if !t.is_empty() {
                        meta.description = Some(collapse_ws(t));
                    }
                }
            }
            "subject" => {
                let t = node.text().unwrap_or("").trim();
                if !t.is_empty() {
                    meta.subjects.push(t.to_string());
                }
            }
            "meta" => {
                let name_attr = node.attribute("name").unwrap_or("");
                let prop = node.attribute("property").unwrap_or("");
                let content = node
                    .attribute("content")
                    .map(|s| s.to_string())
                    .or_else(|| node.text().map(|t| t.trim().to_string()))
                    .unwrap_or_default();
                if name_attr.eq_ignore_ascii_case("cover") && meta.cover_id.is_none() {
                    meta.cover_id = Some(content);
                } else if meta.series.is_none()
                    && !content.is_empty()
                    && (prop == "belongs-to-collection"
                        || name_attr.eq_ignore_ascii_case("calibre:series"))
                {
                    meta.series = Some(content);
                }
            }
            "item" => {
                let id = node.attribute("id").unwrap_or("").to_string();
                let href = node.attribute("href").unwrap_or("").to_string();
                let mt = node.attribute("media-type").map(|s| s.to_string());
                let props = node.attribute("properties").unwrap_or("");
                if !id.is_empty() && !href.is_empty() {
                    if props.split_whitespace().any(|p| p == "cover-image") {
                        meta.cover_href = Some(href.clone());
                    }
                    meta.manifest.push((id, href, mt));
                }
            }
            _ => {}
        }
    }
    Ok(meta)
}

fn extract_cover(source: &Path, meta: &OpfMeta, dest_dir: &Path) -> Result<Option<String>> {
    let Some(href) = &meta.cover_href else {
        return Ok(None);
    };
    let file = File::open(source)?;
    let mut archive = ZipArchive::new(file)?;
    let href_norm = href.trim_start_matches("./");
    // Try a few path variants (zip entries vary).
    let candidates = [
        href_norm.to_string(),
        href.replace('\\', "/"),
        percent_decode(href_norm),
    ];
    for cand in &candidates {
        if let Ok(bytes) = read_zip_bytes(&mut archive, cand) {
            let ext = Path::new(cand)
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("jpg")
                .to_ascii_lowercase();
            let ext = match ext.as_str() {
                "jpeg" | "jpg" | "png" | "gif" | "webp" | "svg" => ext,
                _ => "img".into(),
            };
            let name = format!("cover.{ext}");
            let mut out = File::create(dest_dir.join(&name))?;
            out.write_all(&bytes)?;
            return Ok(Some(name));
        }
    }
    Ok(None)
}

fn read_zip_string<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<String> {
    let bytes = read_zip_bytes(archive, name)?;
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn read_zip_bytes<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<Vec<u8>> {
    // Resolve to the archive's own spelling of the name, then open by name.
    // Resolving to an *index* would couple this to `file_names()` yielding
    // entries in `by_index` order -- true today, unnoticeable if it ever
    // stopped being true, and the symptom would be silently reading the
    // wrong file out of the book.
    let resolved = find_zip_name(archive, name)?;
    let mut file = archive.by_name(&resolved)?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf)?;
    Ok(buf)
}

/// Normalise a zip entry name for comparison: forward slashes, no leading
/// `./`, lowercased. Zip entries vary wildly between EPUB producers, so
/// lookups are deliberately forgiving.
fn normalize_zip_name(name: &str) -> String {
    name.replace('\\', "/")
        .trim_start_matches("./")
        .to_ascii_lowercase()
}

/// The archive's exact name for `name`, matched forgivingly.
///
/// `file_names()` reads the already-parsed central directory, so this borrows
/// existing strings instead of having `by_index` construct a `ZipFile` per
/// candidate. The previous version did the latter *and* built two fresh
/// `String`s per entry per lookup, and `extract_cover` calls this up to three
/// times — on a 2,000-entry EPUB that was thousands of allocations to locate
/// one cover.
fn find_zip_name<R: Read + std::io::Seek>(
    archive: &mut ZipArchive<R>,
    name: &str,
) -> Result<String> {
    let target = normalize_zip_name(name);
    archive
        .file_names()
        .find(|n| normalize_zip_name(n) == target)
        .map(|n| n.to_string())
        .ok_or_else(|| anyhow!("zip entry not found: {name}"))
}

fn parent_zip_path(path: &str) -> String {
    let p = path.replace('\\', "/");
    match p.rfind('/') {
        Some(i) => p[..i].to_string(),
        None => String::new(),
    }
}

fn join_zip_path(dir: &str, href: &str) -> String {
    let href = href.trim_start_matches("./").replace('\\', "/");
    if href.starts_with('/') {
        return href.trim_start_matches('/').to_string();
    }
    if dir.is_empty() {
        return href;
    }
    // Resolve .. segments lightly.
    let mut parts: Vec<&str> = dir.split('/').filter(|s| !s.is_empty()).collect();
    for seg in href.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(a), Some(b)) = (from_hex(bytes[i + 1]), from_hex(bytes[i + 2])) {
                out.push((a << 4) | b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn from_hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

fn collapse_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// EPUB OPF descriptions are often HTML (`<p>`, `<b>`, …). Strip to plain text.
///
/// Whether removing a tag leaves a space behind depends on the tag, and
/// getting that wrong is visible in the book description either way:
///
/// - **Block-level** tags are paragraph boundaries. Dropping `</p><p>` with no
///   separator produced `…a great book.Really.` — two sentences run together
///   with no space. Publishers very often write descriptions as a single line
///   of HTML with no newline between paragraphs, so nothing else supplies the
///   gap.
/// - **Inline** tags sit *inside* words. Emphasis on a stem or an affix is
///   common in dictionary-ish and academic blurbs (`<i>bene</i>volent`), and
///   inserting a space there splits the word: `bene volent`.
///
/// So the naive fixes are both wrong — "never add a space" breaks the first
/// case, "always add a space" breaks the second — and the tag name decides.
/// Unrecognised tags are treated as inline, which is the safer default: a
/// missing space between paragraphs is ugly, while a space inserted into the
/// middle of a word is a misspelling.
pub fn strip_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut tag = String::new();
    let mut in_tag = false;
    for ch in input.chars() {
        if in_tag {
            if ch == '>' {
                in_tag = false;
                if is_block_tag(&tag) {
                    out.push(' ');
                }
                tag.clear();
            } else {
                tag.push(ch);
            }
            continue;
        }
        if ch == '<' {
            in_tag = true;
            continue;
        }
        out.push(ch);
    }

    // Entities are decoded *after* the tags are gone, so an escaped `&lt;p&gt;`
    // in the source text cannot be mistaken for a real tag on the way through.
    //
    // `&amp;` is decoded last on purpose: doing it first would turn the
    // literal text `&amp;lt;` into `&lt;` and then into `<`, inventing markup
    // the author escaped precisely to avoid.
    let plain = out
        .replace("&nbsp;", " ")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&");
    collapse_ws(&plain)
}

/// Whether a tag is block-level, i.e. removing it should leave a word break.
///
/// `tag` is the raw text between the angle brackets, so it still carries any
/// closing slash and attributes: `/p`, `br /`, `div class="x"`.
fn is_block_tag(tag: &str) -> bool {
    let name = tag
        .trim()
        .trim_start_matches('/')
        .split([' ', '\t', '\n', '/'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        name.as_str(),
        "p" | "br"
            | "div"
            | "li"
            | "ul"
            | "ol"
            | "tr"
            | "td"
            | "th"
            | "table"
            | "blockquote"
            | "section"
            | "article"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "hr"
            | "pre"
    )
}

// ---------------------------------------------------------------------------
// P5: cover replacement
// ---------------------------------------------------------------------------

/// Write new cover bytes into the book's own directory and point the catalog
/// at them. Returns the stored file name.
///
/// A fresh name is generated each time (`cover-<n>.<ext>`) rather than
/// overwriting: GTK caches textures by path, and reusing the path would leave
/// the previous image on screen until restart.
pub fn replace_cover_bytes(
    catalog: &crate::db::Catalog,
    book: &crate::models::Book,
    bytes: &[u8],
) -> Result<String> {
    let ext = guess_image_ext(bytes);
    let dir = crate::paths::book_dir(&book.uuid);
    fs::create_dir_all(&dir)?;

    // Pick a name that is not currently in use.
    let mut name = format!("cover.{ext}");
    let mut n = 1;
    while dir.join(&name).exists() {
        name = format!("cover-{n}.{ext}");
        n += 1;
    }

    let path = dir.join(&name);
    fs::write(&path, bytes).with_context(|| format!("write cover {}", path.display()))?;

    // Remove the old file only after the new one is safely on disk.
    if let Some(old) = &book.cover_name {
        if old != &name {
            let old_path = dir.join(old);
            if old_path.exists() {
                let _ = fs::remove_file(&old_path);
            }
            crate::widgets::book_row::invalidate_cover_cache(&old_path);
        }
    }

    catalog.set_cover_name(book.id, Some(&name))?;
    // A0 step 3: the cover changed, so regenerate the thumbnail to keep it in
    // sync (the grid prefers the thumbnail when it exists).
    crate::thumbs::generate_thumbnail(&path, &crate::paths::thumbnail_path(&book.uuid));
    Ok(name)
}

/// Sniff the format from magic bytes; extension alone is unreliable.
fn guess_image_ext(bytes: &[u8]) -> &'static str {
    if bytes.starts_with(&[0x89, b'P', b'N', b'G']) {
        "png"
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        "jpg"
    } else if bytes.starts_with(b"GIF8") {
        "gif"
    } else if bytes.len() > 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "webp"
    } else {
        "jpg"
    }
}

// ---------------------------------------------------------------------------
// Tests
//
// This file parses files produced by other people's software, which is the
// one place where being lenient matters and where a regression is invisible
// until a reader opens a book and finds it blank. Every case below is a shape
// that real EPUBs in the wild actually have: EPUB 2 `<meta name="cover">`
// versus EPUB 3 `properties="cover-image"`, percent-escaped hrefs, content
// documents addressed relative to an `OEBPS/` OPF, and descriptions that are
// really HTML.
//
// Per pitfalls §19 these assert on parsed values, not on "it did not error".
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_opf_reads_epub2_metadata_and_cover_id() {
        // EPUB 2 names the cover indirectly: a <meta name="cover"> holds a
        // manifest *id*, which then has to be resolved to an href.
        let xml = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>The Long Goodbye</dc:title>
    <dc:creator>Raymond Chandler</dc:creator>
    <dc:creator>A Translator</dc:creator>
    <dc:subject>Crime</dc:subject>
    <dc:subject>Noir</dc:subject>
    <dc:description>A  detective
    story.</dc:description>
    <meta name="cover" content="cover-img"/>
    <meta name="calibre:series" content="Philip Marlowe"/>
  </metadata>
  <manifest>
    <item id="cover-img" href="images/cover.jpg" media-type="image/jpeg"/>
    <item id="ch1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
</package>"#;
        let meta = parse_opf(xml).expect("valid OPF must parse");
        assert_eq!(meta.title.as_deref(), Some("The Long Goodbye"));
        assert_eq!(meta.authors, vec!["Raymond Chandler", "A Translator"]);
        assert_eq!(meta.subjects, vec!["Crime", "Noir"]);
        // The description's internal newline and run of spaces are collapsed.
        assert_eq!(meta.description.as_deref(), Some("A detective story."));
        assert_eq!(meta.series.as_deref(), Some("Philip Marlowe"));
        assert_eq!(meta.cover_id.as_deref(), Some("cover-img"));
        // EPUB 2 gives no direct href; only the id.
        assert_eq!(meta.cover_href, None);
        assert_eq!(meta.manifest.len(), 2);
        assert_eq!(meta.manifest[0].1, "images/cover.jpg");
    }

    #[test]
    fn parse_opf_reads_epub3_cover_image_property() {
        // EPUB 3 marks the cover on the manifest item itself, and
        // `properties` is a space-separated list the cover may share.
        let xml = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Dune</dc:title>
    <meta property="belongs-to-collection">Dune Chronicles</meta>
  </metadata>
  <manifest>
    <item id="c" href="cover.png" media-type="image/png" properties="cover-image"/>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav scripted"/>
  </manifest>
</package>"#;
        let meta = parse_opf(xml).expect("valid OPF must parse");
        assert_eq!(meta.cover_href.as_deref(), Some("cover.png"));
        // `belongs-to-collection` carries its value as text, not @content.
        assert_eq!(meta.series.as_deref(), Some("Dune Chronicles"));
        // "nav scripted" must not be mistaken for a cover by substring match.
        assert_eq!(meta.manifest.len(), 2);
    }

    #[test]
    fn parse_opf_ignores_title_outside_metadata() {
        // A <title> inside the nav document or a guide reference must not
        // become the book title. This is why the parser checks the parent.
        let xml = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf">
  <metadata><dc:title xmlns:dc="http://purl.org/dc/elements/1.1/">Real Title</dc:title></metadata>
  <guide><reference type="cover" title="Cover Page" href="c.xhtml"/></guide>
  <somewhere><title>Not The Title</title></somewhere>
</package>"#;
        let meta = parse_opf(xml).expect("valid OPF must parse");
        assert_eq!(meta.title.as_deref(), Some("Real Title"));
    }

    #[test]
    fn parse_opf_rejects_malformed_xml() {
        // Truncated downloads happen; the import must fail with an error
        // rather than silently producing an empty book.
        assert!(parse_opf("<package><metadata>").is_err());
    }

    #[test]
    fn join_zip_path_resolves_relative_hrefs() {
        // Content documents are addressed relative to the OPF's directory,
        // and hrefs routinely climb out of it with `..`.
        assert_eq!(
            join_zip_path("OEBPS", "text/ch1.xhtml"),
            "OEBPS/text/ch1.xhtml"
        );
        assert_eq!(
            join_zip_path("OEBPS/text", "../images/c.jpg"),
            "OEBPS/images/c.jpg"
        );
        assert_eq!(join_zip_path("OEBPS", "./cover.jpg"), "OEBPS/cover.jpg");
        // An absolute href is taken from the archive root, not the OPF dir.
        assert_eq!(join_zip_path("OEBPS", "/images/c.jpg"), "images/c.jpg");
        // No OPF directory: the href stands alone.
        assert_eq!(join_zip_path("", "cover.jpg"), "cover.jpg");
        // Windows separators appear in archives written on Windows.
        assert_eq!(
            join_zip_path("OEBPS", "text\\ch1.xhtml"),
            "OEBPS/text/ch1.xhtml"
        );
    }

    #[test]
    fn parent_zip_path_returns_the_opf_directory() {
        assert_eq!(parent_zip_path("OEBPS/content.opf"), "OEBPS");
        assert_eq!(parent_zip_path("a/b/content.opf"), "a/b");
        // An OPF at the archive root has no parent directory.
        assert_eq!(parent_zip_path("content.opf"), "");
    }

    #[test]
    fn percent_decode_handles_escapes_and_leaves_junk_alone() {
        // Hrefs in the OPF are URL-escaped, but zip entry names are not.
        assert_eq!(
            percent_decode("images/my%20cover.jpg"),
            "images/my cover.jpg"
        );
        assert_eq!(percent_decode("caf%C3%A9.xhtml"), "café.xhtml");
        // Lowercase hex is equally valid.
        assert_eq!(percent_decode("a%c3%a9b"), "aéb");
        // A bare '%' is not an escape and must survive untouched, as must a
        // truncated one at the very end of the string.
        assert_eq!(percent_decode("100%"), "100%");
        assert_eq!(percent_decode("a%zz"), "a%zz");
        assert_eq!(percent_decode("a%2"), "a%2");
        assert_eq!(percent_decode("plain.jpg"), "plain.jpg");
    }

    #[test]
    fn normalize_zip_name_is_forgiving_about_spelling() {
        assert_eq!(
            normalize_zip_name("OEBPS\\Text\\Ch1.xhtml"),
            "oebps/text/ch1.xhtml"
        );
        assert_eq!(normalize_zip_name("./content.opf"), "content.opf");
        // Only a *leading* "./" is stripped.
        assert_eq!(normalize_zip_name("a/./b"), "a/./b");
    }

    #[test]
    fn strip_html_separates_paragraphs_but_not_words() {
        // The whole point of the block/inline split. Publishers write
        // descriptions as one line of HTML with no newline between
        // paragraphs, so if `</p><p>` leaves nothing behind the sentences
        // collide -- this used to render "A great book.Really."
        assert_eq!(
            strip_html("<p>A great book.</p><p>Really.</p>"),
            "A great book. Really."
        );
        assert_eq!(
            strip_html("<div class=\"blurb\">One.</div><div>Two.</div>"),
            "One. Two."
        );
        assert_eq!(strip_html("<h1>Title</h1>Body"), "Title Body");
        assert_eq!(strip_html("<ul><li>One</li><li>Two</li></ul>"), "One Two");

        // ...and the reason "just always insert a space" is wrong: inline
        // emphasis lands inside a word, and a space there is a misspelling.
        assert_eq!(
            strip_html("the word <i>bene</i>volent"),
            "the word benevolent"
        );
        assert_eq!(strip_html("<b>Dune</b> is a novel."), "Dune is a novel.");
        assert_eq!(strip_html("a <span>b</span> c"), "a b c");
    }

    #[test]
    fn strip_html_handles_br_in_all_its_spellings() {
        // `<br/>` and `<br />` were previously "handled" by two string
        // replacements that ran *after* tag stripping -- by which point the
        // tags were already gone, so they never matched anything. The line
        // break they were meant to preserve was silently lost.
        for spelling in ["a<br>b", "a<br/>b", "a<br />b", "a<BR/>b"] {
            assert_eq!(strip_html(spelling), "a b", "{spelling}");
        }
    }

    #[test]
    fn strip_html_is_case_insensitive_about_tag_names() {
        // Uppercase tags are legal HTML and appear in older EPUBs.
        assert_eq!(strip_html("<P>Upper.</P><P>Case.</P>"), "Upper. Case.");
        assert_eq!(strip_html("<I>ital</I>ic"), "italic");
    }

    #[test]
    fn strip_html_decodes_entities_without_inventing_markup() {
        assert_eq!(strip_html("Tom &amp; Jerry"), "Tom & Jerry");
        assert_eq!(
            strip_html("&quot;quoted&quot; &apos;and&apos;"),
            "\"quoted\" 'and'"
        );
        assert_eq!(strip_html("a&nbsp;&nbsp;b"), "a b");
        // `&amp;` is decoded last for this case: decoding it first turns the
        // literal text `&amp;lt;` into `&lt;` and then into `<`, fabricating a
        // tag the author escaped specifically to avoid.
        assert_eq!(
            strip_html("escaped &amp;lt;p&amp;gt; stays text"),
            "escaped &lt;p&gt; stays text"
        );
    }

    #[test]
    fn strip_html_leaves_plain_text_alone() {
        assert_eq!(strip_html("no markup here"), "no markup here");
        // Source-indentation whitespace is still collapsed.
        assert_eq!(strip_html("<div>\n   spaced\n   out\n</div>"), "spaced out");
    }

    #[test]
    fn guess_image_ext_sniffs_magic_bytes() {
        // Covers are routinely a PNG named .jpg; the extension on disk has to
        // match the actual bytes or GTK refuses to load the texture.
        assert_eq!(guess_image_ext(&[0x89, b'P', b'N', b'G', 0x0D]), "png");
        assert_eq!(guess_image_ext(&[0xFF, 0xD8, 0xFF, 0xE0]), "jpg");
        assert_eq!(guess_image_ext(b"GIF89a...."), "gif");
        assert_eq!(guess_image_ext(b"RIFF\0\0\0\0WEBPVP8 "), "webp");
        // Unknown bytes fall back to jpg rather than failing the import.
        assert_eq!(guess_image_ext(b"not an image"), "jpg");
        // A RIFF header too short to hold the WEBP tag must not panic on the
        // bytes[8..12] slice.
        assert_eq!(guess_image_ext(b"RIFF"), "jpg");
    }

    #[test]
    fn parse_opf_handles_multiple_creators_and_subjects() {
        let opf = r#"<?xml version="1.0" encoding="utf-8"?>
        <package xmlns="http://www.idpf.org/2007/opf" version="2.0">
          <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
            <dc:title>Test Book</dc:title>
            <dc:creator>Author One</dc:creator>
            <dc:creator>Author Two</dc:creator>
            <dc:subject>Sci-Fi</dc:subject>
            <dc:subject>Space</dc:subject>
          </metadata>
          <manifest>
            <item id="item1" href="ch1.html" media-type="application/xhtml+xml"/>
          </manifest>
          <spine>
            <itemref idref="item1"/>
          </spine>
        </package>"#;
        let meta = parse_opf(opf).unwrap();
        assert_eq!(meta.title.as_deref(), Some("Test Book"));
        assert_eq!(meta.authors, vec!["Author One", "Author Two"]);
        assert_eq!(meta.subjects, vec!["Sci-Fi", "Space"]);
    }

    #[test]
    fn parse_opf_handles_epub3_belongs_to_collection() {
        let opf = r#"<?xml version="1.0" encoding="utf-8"?>
        <package xmlns="http://www.idpf.org/2007/opf" version="3.0">
          <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
            <dc:title>Dune Messiah</dc:title>
            <meta property="belongs-to-collection">Dune Chronicles</meta>
            <meta property="group-position">2</meta>
          </metadata>
          <manifest/>
          <spine/>
        </package>"#;
        let meta = parse_opf(opf).unwrap();
        assert_eq!(meta.series.as_deref(), Some("Dune Chronicles"));
    }

    #[test]
    fn join_zip_path_resolves_underflow_and_slashes() {
        assert_eq!(join_zip_path("OEBPS", "../../cover.jpg"), "cover.jpg");
        assert_eq!(join_zip_path("OEBPS/text", "../images/cover.jpg"), "OEBPS/images/cover.jpg");
        assert_eq!(join_zip_path("OEBPS", "/images/cover.jpg"), "images/cover.jpg");
    }

    #[test]
    fn percent_decode_handles_malformed_and_edge_cases() {
        assert_eq!(percent_decode("hello%20world"), "hello world");
        assert_eq!(percent_decode("invalid%G1percent"), "invalid%G1percent");
        assert_eq!(percent_decode("truncated%1"), "truncated%1");
        assert_eq!(percent_decode("trailing%"), "trailing%");
    }

    #[test]
    fn strip_html_handles_unclosed_tags_and_attributes() {
        assert_eq!(strip_html("Hello <b>world"), "Hello world");
        assert_eq!(strip_html("<img src=\"x.jpg\" alt=\"cover\"/>Text"), "Text");
        assert_eq!(strip_html("<h1>Heading</h1><p>Body</p>"), "Heading Body");
        assert_eq!(strip_html("Line 1<br/>Line 2"), "Line 1 Line 2");
    }

    #[test]
    fn normalize_zip_name_strips_leading_dot_slash_and_backslashes() {
        assert_eq!(normalize_zip_name("./OEBPS\\content.opf"), "oebps/content.opf");
        assert_eq!(normalize_zip_name("OEBPS/content.opf"), "oebps/content.opf");
    }

    #[test]
    fn import_epub_rejects_unsupported_extensions() {
        let cat = Catalog::open_in_memory().unwrap();
        let unsupported = std::env::temp_dir().join("test_unsupported.txt");
        std::fs::write(&unsupported, b"hello").unwrap();
        let err = import_epub(&cat, &unsupported).unwrap_err();
        assert!(err.to_string().contains("unsupported file format"));
        let _ = std::fs::remove_file(&unsupported);
    }
}
