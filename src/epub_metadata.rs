//! Write metadata back into the EPUB file itself.
//!
//! Until now edits lived only in `catalog.db`, so opening the same book in
//! Calibre or Foliate showed the original values. Writing the OPF makes your
//! corrections travel with the file.
//!
//! Safety rules, because this touches the user's actual books:
//!
//! * The original is copied to `<name>.epub.orig` before the first rewrite.
//! * The new archive is built in a temporary file and only renamed into place
//!   once it is complete and re-openable, so an interrupted write cannot leave
//!   a truncated book behind.
//! * Every entry we are not deliberately changing is copied across verbatim
//!   with `raw_copy_file`, which preserves the compressed bytes exactly.
//! * `mimetype` is re-emitted first and uncompressed, as the EPUB spec
//!   requires.

use crate::models::Book;
use anyhow::{anyhow, Context, Result};
use std::fs;
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// What was changed, for the status line.
#[derive(Debug, Default, Clone, Copy)]
pub struct WriteReport {
    pub wrote_metadata: bool,
    pub wrote_cover: bool,
    pub backed_up: bool,
}

/// Write `book`'s catalog metadata into its EPUB.
///
/// `cover_bytes` replaces the cover image when supplied and the book declares
/// one. Returns what actually changed.
pub fn write_metadata_to_epub(book: &Book, cover_bytes: Option<&[u8]>) -> Result<WriteReport> {
    let path = &book.file_path;
    if !path.is_file() {
        return Err(anyhow!("book file is missing: {}", path.display()));
    }

    let mut report = WriteReport::default();

    // ── read the existing archive ───────────────────────────────────────
    let (opf_path, opf_xml, cover_entry) = {
        let file = fs::File::open(path)?;
        let mut archive = ZipArchive::new(file).context("open EPUB")?;
        let opf_path = crate::epub::find_opf_path_pub(&mut archive)?;
        let opf_xml = read_entry_string(&mut archive, &opf_path)?;
        let cover_entry = cover_bytes
            .is_some()
            .then(|| locate_cover_entry(&opf_xml, &opf_path))
            .flatten();
        (opf_path, opf_xml, cover_entry)
    };

    let new_opf = rewrite_opf(&opf_xml, book)?;

    // ── back up the untouched original, once ────────────────────────────
    let backup = backup_path(path);
    if !backup.exists() {
        fs::copy(path, &backup).with_context(|| format!("back up to {}", backup.display()))?;
        report.backed_up = true;
    }

    // ── build the replacement beside the original ───────────────────────
    let tmp = path.with_extension("epub.kalam-tmp");
    {
        let src = fs::File::open(path)?;
        let mut archive = ZipArchive::new(src)?;
        let dest = fs::File::create(&tmp)?;
        let mut out = ZipWriter::new(dest);

        // The spec requires `mimetype` first and stored uncompressed.
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        if archive.by_name("mimetype").is_ok() {
            out.start_file("mimetype", stored)?;
            out.write_all(b"application/epub+zip")?;
        }

        let deflated = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        for i in 0..archive.len() {
            let entry = archive.by_index(i)?;
            let name = entry.name().to_string();

            if name == "mimetype" || name.ends_with('/') {
                // Already emitted, or a directory marker that is implicit.
                continue;
            }

            // Work out the replacement before touching the entry, so the read
            // borrow ends cleanly either way.
            let replacement: Option<&[u8]> = if name == opf_path {
                Some(new_opf.as_bytes())
            } else if cover_entry.as_deref() == Some(name.as_str()) {
                cover_bytes
            } else {
                None
            };

            match replacement {
                Some(bytes) => {
                    drop(entry);
                    out.start_file(&name, deflated)?;
                    out.write_all(bytes)?;
                    if name == opf_path {
                        report.wrote_metadata = true;
                    } else {
                        report.wrote_cover = true;
                    }
                }
                // Copied without recompressing, preserving the exact bytes.
                None => out.raw_copy_file(entry)?,
            }
        }

        out.finish()?;
    }

    // ── verify before replacing ─────────────────────────────────────────
    // A rewrite that cannot be reopened must never overwrite the real book.
    if let Err(err) = verify_archive(&tmp, &opf_path) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }

    fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))?;
    Ok(report)
}

/// Path of the one-time backup taken before the first rewrite.
pub fn backup_path(epub: &Path) -> PathBuf {
    let mut name = epub.file_name().unwrap_or_default().to_os_string();
    name.push(".orig");
    epub.with_file_name(name)
}

fn verify_archive(path: &Path, opf_path: &str) -> Result<()> {
    let file = fs::File::open(path)?;
    let mut archive = ZipArchive::new(file).context("rewritten EPUB is not a valid zip")?;
    archive
        .by_name(opf_path)
        .map(|_| ())
        .context("rewritten EPUB has no OPF")?;
    Ok(())
}

fn read_entry_string<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Result<String> {
    let mut entry = archive
        .by_name(name)
        .with_context(|| format!("missing {name} in EPUB"))?;
    let mut buf = String::new();
    entry.read_to_string(&mut buf)?;
    Ok(buf)
}

/// Resolve the cover image's zip path from the OPF manifest.
fn locate_cover_entry(opf_xml: &str, opf_path: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(opf_xml).ok()?;

    // EPUB 3 marks it with properties="cover-image"; EPUB 2 uses a
    // <meta name="cover" content="<id>"> pointing at a manifest item.
    let mut href = doc
        .descendants()
        .find(|n| {
            n.tag_name().name() == "item"
                && n.attribute("properties")
                    .is_some_and(|p| p.split_whitespace().any(|t| t == "cover-image"))
        })
        .and_then(|n| n.attribute("href"))
        .map(|h| h.to_string());

    if href.is_none() {
        let cover_id = doc
            .descendants()
            .find(|n| n.tag_name().name() == "meta" && n.attribute("name") == Some("cover"))
            .and_then(|n| n.attribute("content"))?;
        href = doc
            .descendants()
            .find(|n| n.tag_name().name() == "item" && n.attribute("id") == Some(cover_id))
            .and_then(|n| n.attribute("href"))
            .map(|h| h.to_string());
    }

    // hrefs are relative to the OPF, which is usually inside OEBPS/.
    let href = href?;
    let dir = opf_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    Some(if dir.is_empty() {
        href
    } else {
        format!("{dir}/{href}")
    })
}

// ---------------------------------------------------------------------------
// OPF rewriting
// ---------------------------------------------------------------------------

/// Replace the Dublin Core metadata in an OPF with the catalog's values.
///
/// Done as a targeted string edit rather than a full XML re-serialisation:
/// EPUBs carry manifests, spines and vendor extensions we have no business
/// reformatting, and roxmltree is read-only.
fn rewrite_opf(xml: &str, book: &Book) -> Result<String> {
    let (start, end) =
        find_metadata_block(xml).ok_or_else(|| anyhow!("OPF has no <metadata> block"))?;

    let original = &xml[start..end];
    let prefix = detect_dc_prefix(original);
    let mut rebuilt = String::with_capacity(original.len() + 512);

    // Keep anything we do not manage: identifiers, language, modified stamps,
    // the cover pointer, and any vendor metadata.
    //
    // This walks *elements*, not lines. Splitting on newlines assumed every
    // metadata tag sat on its own line, which is only true of pretty-printed
    // OPFs. Plenty of real EPUBs put the whole block on one line, and then a
    // single "line" contained both managed and unmanaged tags: either the lot
    // was dropped, or nothing was, leaving the old title in place beside the
    // new one. Two <dc:title>s is why Foliate kept showing the original.
    for element in metadata_elements(original) {
        if is_managed_element(element, &prefix) {
            continue;
        }
        rebuilt.push_str("    ");
        rebuilt.push_str(element.trim());
        rebuilt.push('\n');
    }

    let push = |out: &mut String, tag: &str, value: &str| {
        if value.trim().is_empty() {
            return;
        }
        out.push_str(&format!(
            "    <{p}{tag}>{}</{p}{tag}>\n",
            escape_xml(value.trim()),
            p = prefix,
            tag = tag
        ));
    };

    push(&mut rebuilt, "title", &book.title);
    for author in book
        .authors
        .split(',')
        .map(str::trim)
        .filter(|a| !a.is_empty())
    {
        rebuilt.push_str(&format!(
            "    <{p}creator>{}</{p}creator>\n",
            escape_xml(author),
            p = prefix
        ));
    }
    push(&mut rebuilt, "description", &book.description);
    push(&mut rebuilt, "publisher", &book.publisher);
    push(&mut rebuilt, "date", &book.published);
    for tag in &book.tags {
        rebuilt.push_str(&format!(
            "    <{p}subject>{}</{p}subject>\n",
            escape_xml(tag),
            p = prefix
        ));
    }

    // Series has no Dublin Core element; Calibre's convention is a meta tag,
    // and other readers understand it.
    if let Some(series) = book.series.as_deref().filter(|s| !s.trim().is_empty()) {
        rebuilt.push_str(&format!(
            "    <meta name=\"calibre:series\" content=\"{}\"/>\n",
            escape_xml(series.trim())
        ));
        if book.series_index > 0.0 {
            rebuilt.push_str(&format!(
                "    <meta name=\"calibre:series_index\" content=\"{}\"/>\n",
                book.series_index
            ));
        }
    }

    Ok(format!("{}{}{}", &xml[..start], rebuilt, &xml[end..]))
}

/// Byte range of the metadata block's *contents*.
fn find_metadata_block(xml: &str) -> Option<(usize, usize)> {
    let lower = xml.to_lowercase();
    let open = lower.find("<metadata")?;
    let content_start = lower[open..].find('>')? + open + 1;
    let close = lower[content_start..].find("</metadata")? + content_start;
    Some((content_start, close))
}

/// EPUBs write either `<dc:title>` or a default-namespaced `<title>`.
fn detect_dc_prefix(metadata: &str) -> String {
    if metadata.contains("<dc:") {
        "dc:".into()
    } else {
        String::new()
    }
}

/// Split a metadata block into top-level elements, regardless of line breaks.
///
/// Deliberately small rather than a full parser: it tracks whether it is
/// inside a quoted attribute or a comment so that a `>` in either does not
/// end an element early. Nested children (rare in OPF metadata, but legal)
/// come back attached to their parent, which is what we want — the parent is
/// either kept whole or dropped whole.
fn metadata_elements(block: &str) -> Vec<&str> {
    let bytes = block.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    let mut depth = 0i32;
    let mut start = 0usize;
    let mut quote: Option<u8> = None;

    while i < bytes.len() {
        let c = bytes[i];

        if let Some(q) = quote {
            if c == q {
                quote = None;
            }
            i += 1;
            continue;
        }

        match c {
            b'"' | b'\'' if depth > 0 => quote = Some(c),
            b'<' => {
                // Comments and CDATA/processing instructions are copied as-is.
                if block[i..].starts_with("<!--") {
                    let stop = block[i..]
                        .find("-->")
                        .map(|p| i + p + 3)
                        .unwrap_or(bytes.len());
                    if depth == 0 {
                        out.push(&block[i..stop]);
                    }
                    i = stop;
                    continue;
                }
                if depth == 0 {
                    start = i;
                }
                // A closing tag `</x>` pairs with the open that preceded it.
                if !block[i..].starts_with("</") {
                    depth += 1;
                } else {
                    depth -= 1;
                }
            }
            b'>' => {
                // Self-closing `<x/>` opened and closed in one tag.
                if i > 0 && bytes[i - 1] == b'/' {
                    depth -= 1;
                }
                if depth <= 0 {
                    let piece = block[start..=i].trim();
                    if !piece.is_empty() {
                        out.push(piece);
                    }
                    depth = 0;
                }
            }
            _ => {}
        }
        i += 1;
    }

    out
}

/// Tags we replace wholesale, so old values are not left behind.
fn is_managed_element(element: &str, prefix: &str) -> bool {
    const MANAGED: [&str; 6] = [
        "title",
        "creator",
        "description",
        "publisher",
        "date",
        "subject",
    ];
    let t = element.trim_start();
    for tag in MANAGED {
        let open = format!("<{prefix}{tag}");
        if let Some(rest) = t.strip_prefix(&open) {
            // Guard against <dc:date> matching <dc:dateCopyrighted>: the name
            // must actually end here.
            if rest.starts_with('>')
                || rest.starts_with('/')
                || rest.starts_with(char::is_whitespace)
            {
                return true;
            }
        }
    }
    // Calibre series markers are rewritten too.
    t.contains("name=\"calibre:series\"") || t.contains("name=\"calibre:series_index\"")
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

// ---------------------------------------------------------------------------
// Orchestration
// ---------------------------------------------------------------------------

/// Preference key controlling whether saves reach the file.
pub const PREF_WRITE_TO_FILE: &str = "epub.write_metadata";

/// True unless the user has explicitly turned it off.
pub fn write_enabled(catalog: &crate::db::Catalog) -> bool {
    catalog
        .get_pref(PREF_WRITE_TO_FILE)
        .map(|v| v != "0")
        .unwrap_or(true)
}

pub fn set_write_enabled(catalog: &crate::db::Catalog, on: bool) {
    catalog.set_pref(PREF_WRITE_TO_FILE, if on { "1" } else { "0" });
}

/// Push the catalog's metadata for `book_id` into its EPUB, then re-point the
/// catalog at the rewritten file's hash.
///
/// A failure here is reported but never rolled back into the database: the
/// edit is already saved in Kalam, and only portability is lost.
pub fn sync_book_to_file(catalog: &crate::db::Catalog, book_id: i64) -> Result<WriteReport> {
    let book = catalog
        .get_book(book_id)?
        .ok_or_else(|| anyhow!("book not found"))?;

    if !matches!(book.format, crate::models::BookFormat::Epub) {
        return Ok(WriteReport::default());
    }

    // Send the cover we are actually displaying, so the file matches Kalam.
    let cover_bytes = book
        .cover_path
        .as_ref()
        .filter(|p| p.is_file())
        .and_then(|p| fs::read(p).ok());

    let report = write_metadata_to_epub(&book, cover_bytes.as_deref())?;

    // The bytes changed, so the stored hash is stale.
    if report.wrote_metadata || report.wrote_cover {
        if let Ok(new_hash) = crate::db::hash_file(&book.file_path) {
            catalog.rehash_book(book_id, &book.file_hash, &new_hash)?;
        }
    }
    Ok(report)
}

/// Every `.epub.orig` backup under the library, with its size.
pub fn list_backups() -> Vec<(PathBuf, u64)> {
    let mut out = Vec::new();
    let Ok(entries) = fs::read_dir(crate::paths::library_dir()) else {
        return out;
    };
    // Backups sit inside each book's own uuid directory.
    for book_dir in entries.flatten() {
        let Ok(files) = fs::read_dir(book_dir.path()) else {
            continue;
        };
        for file in files.flatten() {
            let path = file.path();
            if path.extension().and_then(|e| e.to_str()) == Some("orig") {
                let size = file.metadata().map(|m| m.len()).unwrap_or(0);
                out.push((path, size));
            }
        }
    }
    out
}

/// Delete every backup. Returns how many went and how much was freed.
pub fn delete_backups() -> (usize, u64) {
    let mut count = 0;
    let mut freed = 0;
    for (path, size) in list_backups() {
        if fs::remove_file(&path).is_ok() {
            count += 1;
            freed += size;
        }
    }
    (count, freed)
}

/// `1.4 GB`, `812 MB`, `44 KB`.
pub fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{value:.1} {}", UNITS[unit])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BookFormat;

    fn book() -> Book {
        Book {
            id: 1,
            uuid: "u".into(),
            title: "New Title".into(),
            authors: "Alice Smith, Bob Jones".into(),
            series: Some("The Saga".into()),
            description: "A tale of <angle> brackets & ampersands.".into(),
            format: BookFormat::Epub,
            file_name: "book.epub".into(),
            file_hash: "h".into(),
            cover_name: None,
            added_at: String::new(),
            progress: 0,
            rating: 0,
            publisher: "Acme".into(),
            published: "2019".into(),
            series_index: 2.5,
            tags: vec!["scifi".into(), "ya".into()],
            cover_path: None,
            file_path: PathBuf::new(),
        }
    }

    const OPF: &str = r#"<?xml version="1.0"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Old Title</dc:title>
    <dc:creator>Someone Else</dc:creator>
    <dc:identifier id="pub-id">urn:uuid:1234</dc:identifier>
    <dc:language>en</dc:language>
    <meta name="cover" content="cover-img"/>
  </metadata>
  <manifest><item id="cover-img" href="images/cover.jpg" media-type="image/jpeg"/></manifest>
</package>"#;

    #[test]
    fn replaces_managed_fields_only() {
        let out = rewrite_opf(OPF, &book()).unwrap();
        assert!(out.contains("<dc:title>New Title</dc:title>"));
        assert!(!out.contains("Old Title"));
        assert!(!out.contains("Someone Else"));
        // Untouched entries must survive.
        assert!(out.contains("urn:uuid:1234"));
        assert!(out.contains("<dc:language>en</dc:language>"));
        assert!(out.contains(r#"<meta name="cover" content="cover-img"/>"#));
        // And the rest of the package is intact.
        assert!(out.contains("<manifest>"));
    }

    #[test]
    fn splits_multiple_authors() {
        let out = rewrite_opf(OPF, &book()).unwrap();
        assert!(out.contains("<dc:creator>Alice Smith</dc:creator>"));
        assert!(out.contains("<dc:creator>Bob Jones</dc:creator>"));
    }

    #[test]
    fn escapes_xml_in_values() {
        let out = rewrite_opf(OPF, &book()).unwrap();
        assert!(out.contains("&lt;angle&gt;"));
        assert!(out.contains("&amp; ampersands"));
        assert!(!out.contains("<angle>"));
    }

    #[test]
    fn writes_series_as_calibre_meta() {
        let out = rewrite_opf(OPF, &book()).unwrap();
        assert!(out.contains(r#"name="calibre:series" content="The Saga""#));
        assert!(out.contains(r#"name="calibre:series_index" content="2.5""#));
    }

    #[test]
    fn handles_opf_without_a_dc_prefix() {
        let plain = r#"<package><metadata><title>Old</title></metadata></package>"#;
        let out = rewrite_opf(plain, &book()).unwrap();
        assert!(out.contains("<title>New Title</title>"), "{out}");
        assert!(!out.contains("<dc:title>"));
    }

    #[test]
    fn missing_metadata_block_is_an_error_not_a_panic() {
        assert!(rewrite_opf("<package></package>", &book()).is_err());
    }

    /// The bug Foliate exposed: an OPF whose metadata is all on one line.
    /// The old line-based filter could not drop individual tags, so the
    /// original title survived alongside the new one and readers showed the
    /// stale value.
    #[test]
    fn rewrites_single_line_metadata() {
        let opf = concat!(
            r#"<package><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">"#,
            r#"<dc:identifier id="BookId">urn:uuid:abc</dc:identifier>"#,
            r#"<dc:title>Old Title</dc:title>"#,
            r#"<dc:creator>Someone Else</dc:creator>"#,
            r#"<dc:language>en</dc:language>"#,
            r#"</metadata></package>"#
        );
        let out = rewrite_opf(opf, &book()).unwrap();
        assert!(out.contains("<dc:title>New Title</dc:title>"), "{out}");
        assert!(!out.contains("Old Title"), "stale title survived: {out}");
        assert!(
            !out.contains("Someone Else"),
            "stale author survived: {out}"
        );
        assert_eq!(out.matches("<dc:title>").count(), 1, "duplicated: {out}");
        // Unmanaged entries still have to survive.
        assert!(out.contains("urn:uuid:abc"));
        assert!(out.contains("<dc:language>en</dc:language>"));
    }

    #[test]
    fn keeps_a_greater_than_inside_an_attribute() {
        let opf = concat!(
            r#"<package><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">"#,
            r#"<dc:title>Old</dc:title><meta name="x" content="a > b"/>"#,
            r#"</metadata></package>"#
        );
        let out = rewrite_opf(opf, &book()).unwrap();
        assert!(out.contains(r#"<meta name="x" content="a > b"/>"#), "{out}");
        assert!(!out.contains(">Old<"), "{out}");
    }

    /// `<dc:date>` is managed; `<dc:dateCopyrighted>` merely starts with it.
    #[test]
    fn prefix_match_does_not_eat_longer_tag_names() {
        let opf = concat!(
            r#"<package><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">"#,
            r#"<dc:date>2001</dc:date><dc:dateCopyrighted>1999</dc:dateCopyrighted>"#,
            r#"</metadata></package>"#
        );
        let out = rewrite_opf(opf, &book()).unwrap();
        assert!(
            out.contains("<dc:dateCopyrighted>1999</dc:dateCopyrighted>"),
            "{out}"
        );
        assert!(!out.contains(">2001<"), "{out}");
    }

    #[test]
    fn title_with_attributes_is_still_replaced() {
        let opf = concat!(
            r#"<package><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">"#,
            r#"<dc:title id="t1" xml:lang="en">Old Title</dc:title>"#,
            r#"</metadata></package>"#
        );
        let out = rewrite_opf(opf, &book()).unwrap();
        assert!(!out.contains("Old Title"), "{out}");
        assert!(out.contains("<dc:title>New Title</dc:title>"), "{out}");
    }

    #[test]
    fn comments_survive_the_rewrite() {
        let opf = concat!(
            r#"<package><metadata xmlns:dc="http://purl.org/dc/elements/1.1/">"#,
            r#"<!-- built by something --><dc:title>Old</dc:title>"#,
            r#"</metadata></package>"#
        );
        let out = rewrite_opf(opf, &book()).unwrap();
        assert!(out.contains("<!-- built by something -->"), "{out}");
    }

    #[test]
    fn finds_cover_via_epub2_meta() {
        let found = locate_cover_entry(OPF, "OEBPS/content.opf");
        assert_eq!(found.as_deref(), Some("OEBPS/images/cover.jpg"));
    }

    #[test]
    fn finds_cover_via_epub3_properties() {
        let opf = r#"<package><metadata></metadata><manifest>
            <item id="c" href="cover.png" properties="cover-image"/>
        </manifest></package>"#;
        assert_eq!(
            locate_cover_entry(opf, "content.opf").as_deref(),
            Some("cover.png")
        );
    }

    #[test]
    fn human_size_scales_units() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(512), "512 B");
        assert_eq!(human_size(2048), "2.0 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
        assert!(human_size(3 * 1024 * 1024 * 1024).ends_with("GB"));
    }

    #[test]
    fn backup_name_sits_beside_the_book() {
        let p = backup_path(Path::new("/lib/uuid/book.epub"));
        assert_eq!(p, PathBuf::from("/lib/uuid/book.epub.orig"));
    }
}
