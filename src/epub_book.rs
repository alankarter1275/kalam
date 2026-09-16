//! Open an on-disk EPUB for reading: spine, TOC, chapter HTML.
//! P3 adds highlight CSS + selection chip + dictionary JS.

use anyhow::{anyhow, Context, Result};
use roxmltree::Document;
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub struct TocEntry {
    pub label: String,
    #[allow(dead_code)]
    pub href: String,
    /// Spine index if this href maps to a spine item.
    pub spine_index: Option<usize>,
}

#[derive(Debug, Clone)]
pub struct SpineItem {
    #[allow(dead_code)]
    pub id: String,
    pub href: String,
    /// Absolute path on disk after extract (under cache dir).
    pub path: PathBuf,
    pub title: String,
}

#[derive(Debug)]
pub struct OpenBook {
    #[allow(dead_code)]
    pub title: String,
    pub spine: Vec<SpineItem>,
    #[allow(dead_code)]
    opf_dir: String,
}

impl OpenBook {

    /// Unzip EPUB into `cache_dir/uuid/` (or reuse if present) and parse spine/TOC.
    pub fn open(epub_path: &Path, cache_dir: &Path) -> Result<Self> {
        fs::create_dir_all(cache_dir)?;
        // Marker so we know extract finished.
        let marker = cache_dir.join(".kalam_extracted");
        if !marker.exists() {
            extract_zip(epub_path, cache_dir)?;
            File::create(&marker)?;
        }

        let container = read_file_string(&cache_dir.join("META-INF/container.xml"))
            .or_else(|_| read_file_string(&cache_dir.join("meta-inf/container.xml")))
            .context("container.xml")?;
        let opf_rel = find_opf_path(&container)?;
        let opf_path = cache_dir.join(&opf_rel);
        let opf_xml = read_file_string(&opf_path)?;
        let opf_dir = parent_zip_path(&opf_rel);

        let (title, manifest, spine_ids) = parse_opf_spine(&opf_xml)?;
        let mut spine = Vec::new();
        for (i, id) in spine_ids.iter().enumerate() {
            let href = manifest
                .iter()
                .find(|(mid, _, _)| mid == id)
                .map(|(_, h, _)| h.clone())
                .ok_or_else(|| anyhow!("spine id {id} missing from manifest"))?;
            let rel = join_zip_path(&opf_dir, &href);
            let path = cache_dir.join(&rel);
            let chap_title = format!("Chapter {}", i + 1);
            spine.push(SpineItem {
                id: id.clone(),
                href: rel,
                path,
                title: chap_title,
            });
        }

        let toc = parse_nav_or_ncx(cache_dir, &opf_dir, &opf_xml, &spine)?;
        // Improve spine titles from TOC when possible.
        for entry in &toc {
            if let Some(idx) = entry.spine_index {
                if let Some(item) = spine.get_mut(idx) {
                    if !entry.label.is_empty() {
                        item.title = entry.label.clone();
                    }
                }
            }
        }

        if spine.is_empty() {
            return Err(anyhow!("EPUB has empty spine"));
        }

        Ok(Self {
            title,
            spine,
            opf_dir,
        })
    }

    pub fn chapter_count(&self) -> usize {
        self.spine.len()
    }

    pub fn chapter_body(&self, index: usize) -> Result<String> {
        let item = self
            .spine
            .get(index)
            .ok_or_else(|| anyhow!("chapter index {index} out of range"))?;
        let raw = fs::read_to_string(&item.path)
            .with_context(|| format!("read chapter {}", item.path.display()))?;
        Ok(extract_body_content(&raw))
    }
}

/// Total bytes we are willing to write for one book.
///
/// An EPUB is a zip, and a zip's declared sizes are chosen by whoever built
/// it. A "zip bomb" — nested deflate streams, or a flat entry of a few
/// kilobytes that expands to gigabytes — would otherwise fill
/// `~/.local/share/kalam/cache/reader/` until the disk is full, which on the
/// 4 GB laptop this app targets wedges the whole session rather than just
/// failing one book. 512 MB is far beyond any real EPUB (a heavily
/// illustrated one is tens of MB) and far below "the disk is gone".
const MAX_EXTRACT_BYTES: u64 = 512 * 1024 * 1024;

/// Entry-count ceiling, for the same reason: a zip can declare millions of
/// tiny entries and cost us a `create`/`close` syscall pair for each.
const MAX_EXTRACT_ENTRIES: usize = 10_000;

fn extract_zip(epub: &Path, dest: &Path) -> Result<()> {
    let file = File::open(epub)?;
    let mut archive = ZipArchive::new(file)?;

    if archive.len() > MAX_EXTRACT_ENTRIES {
        return Err(anyhow!(
            "EPUB has {} entries (limit {MAX_EXTRACT_ENTRIES}) — refusing to extract",
            archive.len()
        ));
    }

    let mut written: u64 = 0;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i)?;

        // SECURITY: `entry.name()` is attacker-controlled, and `Path::join`
        // has two behaviours that turn it into arbitrary file write:
        //   * a relative name may contain `..` and climb out of `dest`,
        //   * an *absolute* name replaces `dest` entirely, because
        //     `a.join("/etc/x") == "/etc/x"`.
        // Either one lets an EPUB downloaded from a stranger drop a file into
        // ~/.config/autostart and run code at next login. `enclosed_name()`
        // is the zip crate's answer: it returns `None` for exactly the names
        // that can escape the destination directory.
        //
        // Skipped rather than fatal: one hostile or malformed entry should
        // not make an otherwise readable book refuse to open. The rest of the
        // archive still extracts, and a book missing a chapter is a visible
        // problem the user can act on.
        let Some(relative) = entry.enclosed_name() else {
            eprintln!(
                "kalam: skipping unsafe EPUB entry path {:?} in {}",
                entry.name(),
                epub.display()
            );
            continue;
        };

        // Note on symlinks: zip can carry a unix symlink entry, whose body is
        // the link target. We write that body as a plain file, which is inert.
        // Do not "fix" this by honouring unix modes — a symlink pointing at
        // ~/.ssh/authorized_keys plus a later entry writing through it is the
        // two-step version of the traversal guarded against above.
        let out_path = dest.join(&relative);

        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
            continue;
        }

        if let Some(parent) = out_path.parent() {
            fs::create_dir_all(parent)?;
        }

        // The remaining budget is enforced on bytes actually copied, via
        // `take`. Deliberately NOT on `entry.size()`: that is the archive's
        // *declared* uncompressed size, which is attacker-controlled and can
        // lie in either direction. Capping the real read is what makes the
        // limit real.
        let remaining = MAX_EXTRACT_BYTES.saturating_sub(written);
        if remaining == 0 {
            return Err(anyhow!(
                "EPUB expands past {MAX_EXTRACT_BYTES} bytes — refusing to extract"
            ));
        }
        let mut out = File::create(&out_path)?;
        let copied = std::io::copy(&mut entry.by_ref().take(remaining), &mut out)?;
        written += copied;

        // `copied == remaining` means we stopped because we hit the cap, not
        // because the entry ended, so the file on disk is truncated garbage.
        // One more `read` distinguishes the two. Not `bytes()`: that yields a
        // `Result` per byte through an unbuffered decompressor, and clippy
        // rejects it — we only need to know whether a single further byte
        // exists.
        let mut probe = [0u8; 1];
        let overflowed = copied == remaining && entry.read(&mut probe)? > 0;
        if overflowed {
            drop(out);
            let _ = fs::remove_file(&out_path);
            return Err(anyhow!(
                "EPUB expands past {MAX_EXTRACT_BYTES} bytes — refusing to extract"
            ));
        }
    }
    Ok(())
}

fn find_opf_path(container_xml: &str) -> Result<String> {
    let doc = Document::parse(container_xml)?;
    for node in doc.descendants() {
        if node.tag_name().name() == "rootfile" {
            if let Some(full) = node.attribute("full-path") {
                return Ok(full.to_string());
            }
        }
    }
    Err(anyhow!("no rootfile in container.xml"))
}

/// (id, href, media-type)
type ManifestItem = (String, String, Option<String>);

fn parse_opf_spine(opf: &str) -> Result<(String, Vec<ManifestItem>, Vec<String>)> {
    let doc = Document::parse(opf)?;
    let mut title = String::from("Untitled");
    let mut manifest = Vec::new();
    let mut spine = Vec::new();
    let mut title_set = false;

    for node in doc.descendants() {
        match node.tag_name().name() {
            "title" if !title_set => {
                let t = node.text().unwrap_or("").trim();
                if !t.is_empty() {
                    title = t.to_string();
                    title_set = true;
                }
            }
            "item" => {
                let id = node.attribute("id").unwrap_or("").to_string();
                let href = node.attribute("href").unwrap_or("").to_string();
                let mt = node.attribute("media-type").map(|s| s.to_string());
                if !id.is_empty() && !href.is_empty() {
                    manifest.push((id, href, mt));
                }
            }
            "itemref" => {
                if let Some(idref) = node.attribute("idref") {
                    spine.push(idref.to_string());
                }
            }
            _ => {}
        }
    }
    Ok((title, manifest, spine))
}

fn parse_nav_or_ncx(
    root: &Path,
    opf_dir: &str,
    opf_xml: &str,
    spine: &[SpineItem],
) -> Result<Vec<TocEntry>> {
    // Try EPUB3 nav
    if let Ok(nav_href) = find_nav_href(opf_xml) {
        let rel = join_zip_path(opf_dir, &nav_href);
        let path = root.join(&rel);
        if let Ok(html) = fs::read_to_string(&path) {
            let toc = parse_nav_html(&html, spine);
            if !toc.is_empty() {
                return Ok(toc);
            }
        }
    }
    // EPUB2 NCX
    if let Ok(ncx_href) = find_ncx_href(opf_xml) {
        let rel = join_zip_path(opf_dir, &ncx_href);
        let path = root.join(&rel);
        if let Ok(xml) = fs::read_to_string(&path) {
            let toc = parse_ncx(&xml, spine);
            if !toc.is_empty() {
                return Ok(toc);
            }
        }
    }
    // Fallback: one TOC entry per spine item
    Ok(spine
        .iter()
        .enumerate()
        .map(|(i, s)| TocEntry {
            label: s.title.clone(),
            href: s.href.clone(),
            spine_index: Some(i),
        })
        .collect())
}

fn find_nav_href(opf: &str) -> Result<String> {
    let doc = Document::parse(opf)?;
    for node in doc.descendants() {
        if node.tag_name().name() == "item" {
            let props = node.attribute("properties").unwrap_or("");
            if props.split_whitespace().any(|p| p == "nav") {
                if let Some(href) = node.attribute("href") {
                    return Ok(href.to_string());
                }
            }
        }
    }
    Err(anyhow!("no nav document"))
}

fn find_ncx_href(opf: &str) -> Result<String> {
    let doc = Document::parse(opf)?;
    for node in doc.descendants() {
        if node.tag_name().name() == "item" {
            let mt = node.attribute("media-type").unwrap_or("");
            if mt == "application/x-dtbncx+xml" {
                if let Some(href) = node.attribute("href") {
                    return Ok(href.to_string());
                }
            }
        }
    }
    Err(anyhow!("no ncx"))
}

fn parse_nav_html(html: &str, spine: &[SpineItem]) -> Vec<TocEntry> {
    // Lightweight: find <a href="...">label</a> inside nav
    let mut toc = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut search = html;
    // Prefer <nav epub:type="toc">
    if let Some(start) = lower.find("<nav") {
        search = &html[start..];
    }
    let bytes = search.as_bytes();
    let mut i = 0;
    while i + 2 < bytes.len() {
        // find href=
        if search[i..].to_ascii_lowercase().starts_with("href=") {
            let rest = &search[i + 5..];
            let quote = rest.chars().next().unwrap_or('"');
            if quote == '"' || quote == '\'' {
                if let Some(end) = rest[1..].find(quote) {
                    let href = &rest[1..1 + end];
                    // find > after tag
                    if let Some(gt) = search[i..].find('>') {
                        let after = &search[i + gt + 1..];
                        if let Some(close) = after.to_ascii_lowercase().find("</a>") {
                            let label = strip_tags(&after[..close]).trim().to_string();
                            if !label.is_empty() && !href.starts_with('#') {
                                let spine_index = spine_index_for(spine, href);
                                toc.push(TocEntry {
                                    label,
                                    href: href.to_string(),
                                    spine_index,
                                });
                            }
                            i += gt + close + 4;
                            continue;
                        }
                    }
                }
            }
        }
        i += 1;
    }
    toc
}

fn parse_ncx(xml: &str, spine: &[SpineItem]) -> Vec<TocEntry> {
    let mut toc = Vec::new();
    let Ok(doc) = Document::parse(xml) else {
        return toc;
    };
    for node in doc.descendants() {
        if node.tag_name().name() != "navPoint" {
            continue;
        }
        let mut label = String::new();
        let mut href = String::new();
        for child in node.descendants() {
            match child.tag_name().name() {
                "text" if label.is_empty() => {
                    label = child.text().unwrap_or("").trim().to_string();
                }
                "content" if href.is_empty() => {
                    href = child.attribute("src").unwrap_or("").to_string();
                }
                _ => {}
            }
        }
        if !label.is_empty() && !href.is_empty() {
            let spine_index = spine_index_for(spine, &href);
            toc.push(TocEntry {
                label,
                href,
                spine_index,
            });
        }
    }
    toc
}

fn spine_index_for(spine: &[SpineItem], href: &str) -> Option<usize> {
    let clean = href.split('#').next().unwrap_or(href);
    let clean = clean.trim_start_matches("./");
    spine.iter().position(|s| {
        s.href == clean
            || s.href.ends_with(clean)
            || clean.ends_with(&s.href)
            || Path::new(&s.href).file_name() == Path::new(clean).file_name()
    })
}

fn strip_tags(s: &str) -> String {
    let mut out = String::new();
    let mut in_tag = false;
    for ch in s.chars() {
        if in_tag {
            if ch == '>' {
                in_tag = false;
            }
            continue;
        }
        if ch == '<' {
            in_tag = true;
            continue;
        }
        out.push(ch);
    }
    out
}

pub fn extract_body_content(raw_html: &str) -> String {
    let lower = raw_html.to_ascii_lowercase();
    if let Some(start_pos) = lower.find("<body") {
        if let Some(gt) = raw_html[start_pos..].find('>') {
            let content_start = start_pos + gt + 1;
            let content_end = lower.rfind("</body>").unwrap_or(raw_html.len());
            if content_start <= content_end {
                return raw_html[content_start..content_end].to_string();
            }
        }
    }
    raw_html.to_string()
}

fn read_file_string(path: &Path) -> Result<String> {
    let mut f = File::open(path)?;
    let mut s = String::new();
    f.read_to_string(&mut s)?;
    Ok(s)
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReadingTheme {
    Light,
    Sepia,
    Dark,
    Ink,
}

impl ReadingTheme {
    pub fn as_str(self) -> &'static str {
        match self {
            ReadingTheme::Light => "light",
            ReadingTheme::Sepia => "sepia",
            ReadingTheme::Dark => "dark",
            ReadingTheme::Ink => "ink",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "light" => ReadingTheme::Light,
            "dark" => ReadingTheme::Dark,
            "ink" => ReadingTheme::Ink,
            _ => ReadingTheme::Sepia,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    /// A scratch directory that removes itself.
    ///
    /// The repo has no `tempfile` dependency and this is not worth adding one
    /// for; the pattern matches `thumbs.rs` and `preload.rs`.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!("kalam-{tag}-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&p).expect("scratch dir");
            Self(p)
        }
        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// Build a zip at `at` from `(name, bytes)` pairs, writing the names
    /// verbatim so a test can put a traversal path in one.
    fn write_zip(at: &Path, entries: &[(&str, &[u8])]) {
        let file = File::create(at).expect("create zip");
        let mut zip = ZipWriter::new(file);
        let opts = SimpleFileOptions::default();
        for (name, body) in entries {
            zip.start_file(*name, opts).expect("start entry");
            zip.write_all(body).expect("write entry");
        }
        zip.finish().expect("finish zip");
    }

    /// Assert that `archive` really contains an entry named `name`.
    ///
    /// Guards against the failure mode in `docs/pitfalls.md` §19: if
    /// `ZipWriter` ever normalises a hostile path on the way *in*, the
    /// traversal tests below would pass without exercising anything. This
    /// makes that situation a loud failure telling the next person to build
    /// the fixture bytes by hand instead.
    fn assert_archive_contains(at: &Path, name: &str) {
        let file = File::open(at).expect("reopen zip");
        let archive = ZipArchive::new(file).expect("parse zip");
        let names: Vec<String> = archive.file_names().map(|n| n.to_string()).collect();
        assert!(
            names.iter().any(|n| n == name),
            "fixture did not survive ZipWriter: wanted an entry literally named \
             {name:?}, archive holds {names:?}. The traversal tests are only \
             meaningful if the hostile name is really in the archive."
        );
    }

    #[test]
    fn extract_refuses_to_climb_out_of_the_destination() {
        // The zip-slip attack: an EPUB whose entry name walks up out of the
        // cache directory. Before `enclosed_name()` this wrote a real file
        // into the parent, which in the shipping layout is the shared
        // `cache/reader/` root -- and with enough `..` segments, anywhere the
        // user can write.
        let scratch = Scratch::new("zipslip");
        let epub = scratch.path().join("evil.epub");
        let dest = scratch.path().join("dest");
        fs::create_dir_all(&dest).unwrap();

        write_zip(
            &epub,
            &[
                ("../escaped.txt", b"pwned" as &[u8]),
                ("../../escaped-twice.txt", b"pwned"),
                ("ok.txt", b"fine"),
            ],
        );

        assert_archive_contains(&epub, "../escaped.txt");

        extract_zip(&epub, &dest).expect("a hostile entry is skipped, not fatal");

        assert!(
            !scratch.path().join("escaped.txt").exists(),
            "entry with `..` escaped the destination directory"
        );
        assert!(
            !scratch.path().join("escaped-twice.txt").exists(),
            "entry with `../..` escaped the destination directory"
        );
        assert!(
            dest.join("ok.txt").exists(),
            "the safe entries must still extract -- one bad path should not \
             make an otherwise readable book refuse to open"
        );
    }

    #[test]
    fn extract_refuses_an_absolute_entry_path() {
        // The subtler half of zip-slip, and the reason a `..` check alone is
        // not enough: `Path::join` DISCARDS the base when the argument is
        // absolute, so `dest.join("/tmp/x")` is just "/tmp/x". No `..`
        // required.
        let scratch = Scratch::new("zipabs");
        let epub = scratch.path().join("evil.epub");
        let dest = scratch.path().join("dest");
        fs::create_dir_all(&dest).unwrap();

        let absolute = scratch.path().join("absolute-escape.txt");
        let absolute_name = absolute.to_string_lossy().into_owned();
        write_zip(&epub, &[(absolute_name.as_str(), b"pwned" as &[u8])]);
        assert_archive_contains(&epub, absolute_name.as_str());

        extract_zip(&epub, &dest).expect("a hostile entry is skipped, not fatal");

        assert!(
            !absolute.exists(),
            "an absolute entry name wrote outside the destination directory"
        );
    }

    #[test]
    fn extract_stops_at_the_size_ceiling() {
        // Stand-in for a zip bomb. The point is not the ratio -- it is that
        // the ceiling is enforced on bytes actually written, so an entry that
        // lies about its size cannot get past it.
        let scratch = Scratch::new("zipbomb");
        let epub = scratch.path().join("big.epub");
        let dest = scratch.path().join("dest");
        fs::create_dir_all(&dest).unwrap();

        // Highly compressible, so the archive on disk stays tiny.
        let chunk = vec![b'A'; 1024 * 1024];
        let entries: Vec<(String, Vec<u8>)> = (0..600)
            .map(|i| (format!("big-{i}.txt"), chunk.clone()))
            .collect();
        let refs: Vec<(&str, &[u8])> = entries
            .iter()
            .map(|(n, b)| (n.as_str(), b.as_slice()))
            .collect();
        write_zip(&epub, &refs);

        let err =
            extract_zip(&epub, &dest).expect_err("600 MB of payload must trip the 512 MB ceiling");
        assert!(
            err.to_string().contains("refusing to extract"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn extract_rejects_an_absurd_entry_count() {
        let scratch = Scratch::new("zipcount");
        let epub = scratch.path().join("many.epub");
        let dest = scratch.path().join("dest");
        fs::create_dir_all(&dest).unwrap();

        let names: Vec<String> = (0..MAX_EXTRACT_ENTRIES + 1)
            .map(|i| format!("f{i}.txt"))
            .collect();
        let refs: Vec<(&str, &[u8])> = names.iter().map(|n| (n.as_str(), b"x" as &[u8])).collect();
        write_zip(&epub, &refs);

        let err = extract_zip(&epub, &dest).expect_err("entry count must be capped");
        assert!(
            err.to_string().contains("refusing to extract"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn extract_writes_normal_entries_including_nested_directories() {
        // The guard must not break the ordinary case: a real EPUB is mostly
        // nested paths like OEBPS/Text/chapter1.xhtml.
        let scratch = Scratch::new("zipok");
        let epub = scratch.path().join("book.epub");
        let dest = scratch.path().join("dest");
        fs::create_dir_all(&dest).unwrap();

        write_zip(
            &epub,
            &[
                ("mimetype", b"application/epub+zip" as &[u8]),
                ("META-INF/container.xml", b"<container/>"),
                ("OEBPS/Text/chapter1.xhtml", b"<html/>"),
                ("./OEBPS/Text/chapter2.xhtml", b"<html/>"),
            ],
        );

        extract_zip(&epub, &dest).expect("a normal EPUB extracts");

        assert!(dest.join("mimetype").exists());
        assert!(dest.join("META-INF/container.xml").exists());
        assert!(dest.join("OEBPS/Text/chapter1.xhtml").exists());
        // `enclosed_name` normalises a leading `./` away rather than
        // rejecting it -- a curly but legal path must still land.
        assert!(dest.join("OEBPS/Text/chapter2.xhtml").exists());
    }

    #[test]
    fn find_opf_path_extracts_rootfile_full_path() {
        let container = r#"<?xml version="1.0"?>
        <container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
            <rootfiles>
                <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
            </rootfiles>
        </container>"#;
        assert_eq!(find_opf_path(container).unwrap(), "OEBPS/content.opf");

        let missing = r#"<container><rootfiles></rootfiles></container>"#;
        assert!(find_opf_path(missing).is_err());
    }

    #[test]
    fn parse_opf_spine_extracts_items_in_order() {
        let opf = r#"<package>
            <metadata><dc:title xmlns:dc="http://purl.org/dc/elements/1.1/">Book Title</dc:title></metadata>
            <manifest>
                <item id="c1" href="ch1.xhtml" media-type="application/xhtml+xml"/>
                <item id="c2" href="ch2.xhtml" media-type="application/xhtml+xml"/>
                <item id="nav" href="nav.xhtml" properties="nav" media-type="application/xhtml+xml"/>
            </manifest>
            <spine>
                <itemref idref="c1"/>
                <itemref idref="c2"/>
            </spine>
        </package>"#;
        let (title, manifest, spine) = parse_opf_spine(opf).unwrap();
        assert_eq!(title, "Book Title");
        assert_eq!(manifest.len(), 3);
        assert_eq!(spine, vec!["c1", "c2"]);
        assert_eq!(find_nav_href(opf).unwrap(), "nav.xhtml");
    }

    #[test]
    fn spine_index_for_handles_fragments_and_paths() {
        let spine = [
            SpineItem { id: "c1".into(), href: "OEBPS/text/ch1.xhtml".into(), title: "Ch 1".into(), path: std::path::PathBuf::new() },
            SpineItem { id: "c2".into(), href: "OEBPS/text/ch2.xhtml".into(), title: "Ch 2".into(), path: std::path::PathBuf::new() },
        ];
        assert_eq!(spine_index_for(&spine, "OEBPS/text/ch1.xhtml#section1"), Some(0));
        assert_eq!(spine_index_for(&spine, "./ch2.xhtml"), Some(1));
        assert_eq!(spine_index_for(&spine, "nonexistent.xhtml"), None);
    }

    #[test]
    fn strip_tags_removes_markup_cleanly() {
        assert_eq!(strip_tags("<p>Hello <b>World</b>!</p>"), "Hello World!");
        assert_eq!(strip_tags("No tags"), "No tags");
        assert_eq!(strip_tags("Unclosed <tag"), "Unclosed ");
    }

    #[test]
    fn reading_theme_from_str_lossy_parses_variants() {
        assert_eq!(ReadingTheme::from_str_lossy("light"), ReadingTheme::Light);
        assert_eq!(ReadingTheme::from_str_lossy("DARK"), ReadingTheme::Dark);
        assert_eq!(ReadingTheme::from_str_lossy("ink"), ReadingTheme::Ink);
        assert_eq!(ReadingTheme::from_str_lossy("sepia"), ReadingTheme::Sepia);
        assert_eq!(ReadingTheme::from_str_lossy("unknown"), ReadingTheme::Sepia);
    }
}
