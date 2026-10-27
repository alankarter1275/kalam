//! An EPUB as an editable set of files (deep editor; editor-spec.md,
//! decisions 118–143).
//!
//! The reader opens an EPUB to render it; the editor needs the same
//! archive laid out as *files* — text in reading order, styles, images
//! and fonts, and the contents documents — with bytes on hand for the
//! ones being edited and a way to write changed files back without
//! disturbing the rest. That is all this module does. It knows
//! nothing about GTK, buffers or snapshots: load, group, read, write.
//!
//! Writing mirrors `epub_metadata::write_metadata_to_epub`, the other
//! place Kalam rewrites a book it owns: build a replacement beside the
//! original, `mimetype` first and stored, unchanged entries raw-copied
//! so their bytes (and compression) survive exactly, verify the
//! result opens, then rename over the book. The caller keeps the
//! snapshot/backup policy (decision 135); this only rewrites.

use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// What a file is, for how the workspace treats it: source tabs for
/// text kinds, an image tab for images, a specimen tab for fonts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Html,
    Css,
    Xml,
    Image,
    Font,
    Binary,
}

impl FileKind {
    /// Editable as text in a source tab. First caller is Check book
    /// (E2); kept with the kind it describes rather than re-derived there.
    #[allow(dead_code)]
    pub fn is_text(self) -> bool {
        matches!(self, FileKind::Html | FileKind::Css | FileKind::Xml)
    }

    /// The status bar's file-type word (decision 122).
    pub fn label(self) -> &'static str {
        match self {
            FileKind::Html => "HTML",
            FileKind::Css => "CSS",
            FileKind::Xml => "XML",
            FileKind::Image => "Image",
            FileKind::Font => "Font",
            FileKind::Binary => "Binary",
        }
    }

    /// The GtkSourceView language id for a text kind.
    pub fn language_id(self) -> Option<&'static str> {
        match self {
            FileKind::Html => Some("html"),
            FileKind::Css => Some("css"),
            FileKind::Xml => Some("xml"),
            _ => None,
        }
    }
}

/// One file in the book, as the file tree lists it (decision 125).
#[derive(Debug, Clone)]
pub struct EpubFile {
    /// Zip entry name, the identity everything keys on.
    pub path: String,
    pub media_type: String,
    pub kind: FileKind,
    /// Reading order position for spine documents.
    pub spine_index: Option<usize>,
    /// The EPUB 3 navigation document (`properties="nav"`).
    pub is_nav: bool,
    /// The OPF cover image; read by Replace cover from E5.
    #[allow(dead_code)]
    pub is_cover_image: bool,
    /// Declared in the OPF manifest. Files the manifest never
    /// mentions exist in the archive only — Check book flags them.
    pub in_manifest: bool,
}

/// The tree's four sections (decision 125).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TreeSection {
    Text,
    Styles,
    ImagesAndFonts,
    Contents,
}

impl TreeSection {
    pub const ALL: [TreeSection; 4] = [
        TreeSection::Text,
        TreeSection::Styles,
        TreeSection::ImagesAndFonts,
        TreeSection::Contents,
    ];

    pub fn title(self) -> &'static str {
        match self {
            TreeSection::Text => "Text",
            TreeSection::Styles => "Styles",
            TreeSection::ImagesAndFonts => "Images and Fonts",
            TreeSection::Contents => "Contents",
        }
    }
}

/// A whole EPUB read into memory: the manifest's view of its files
/// plus every entry's bytes. Books are a few megabytes; holding them
/// whole keeps tab-opening and tool passes simple, and the save path
/// re-reads from disk anyway (see `write_with_replacements`).
pub struct EpubContainer {
    pub opf_path: String,
    /// The EPUB 3 nav document; Edit contents (E4) writes it.
    #[allow(dead_code)]
    pub nav_path: Option<String>,
    pub ncx_path: Option<String>,
    pub files: Vec<EpubFile>,
    bytes: HashMap<String, Vec<u8>>,
}

/// Hand-written so load results can ride in Relm4 messages (which
/// require `Debug`) without ever printing a book's bytes into a log.
impl std::fmt::Debug for EpubContainer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "EpubContainer {{ opf: {:?}, files: {} }}",
            self.opf_path,
            self.files.len()
        )
    }
}

impl EpubContainer {
    pub fn open(epub_path: &Path) -> Result<Self> {
        let file = fs::File::open(epub_path)
            .with_context(|| format!("open {}", epub_path.display()))?;
        let mut archive = ZipArchive::new(file).context("open EPUB")?;
        let opf_path = crate::epub::find_opf_path_pub(&mut archive)?;

        let mut bytes: HashMap<String, Vec<u8>> = HashMap::new();
        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            let name = entry.name().to_string();
            if name.ends_with('/') {
                continue;
            }
            let mut buf = Vec::with_capacity(entry.size() as usize);
            entry.read_to_end(&mut buf)?;
            bytes.insert(name, buf);
        }

        let opf_text = decode_text(bytes.get(&opf_path).map(Vec::as_slice).unwrap_or(&[]));
        let parsed = parse_opf(&opf_text, &opf_path);

        // Every manifest item, then any archive entry the manifest
        // never mentions — an editor must not hide files from its
        // user just because the book forgot to declare them.
        let mut files: Vec<EpubFile> = parsed.files;
        let declared: std::collections::HashSet<&str> =
            files.iter().map(|f| f.path.as_str()).collect();
        let mut undeclared: Vec<&String> = bytes
            .keys()
            .filter(|name| !declared.contains(name.as_str()))
            .collect();
        undeclared.sort();
        for name in undeclared {
            let media_type = guess_media_type(name).to_string();
            files.push(EpubFile {
                kind: classify(name, &media_type),
                path: name.clone(),
                media_type,
                spine_index: None,
                is_nav: false,
                is_cover_image: false,
                in_manifest: false,
            });
        }

        Ok(Self {
            nav_path: files.iter().find(|f| f.is_nav).map(|f| f.path.clone()),
            ncx_path: parsed.ncx_path,
            opf_path,
            files,
            bytes,
        })
    }

    pub fn file(&self, path: &str) -> Option<&EpubFile> {
        self.files.iter().find(|f| f.path == path)
    }

    pub fn bytes(&self, path: &str) -> Option<&[u8]> {
        self.bytes.get(path).map(Vec::as_slice)
    }

    /// Record bytes the editor has just written to disk, so reopening
    /// a closed tab after a save shows the saved text, not the copy
    /// read when the book was opened.
    pub fn set_bytes(&mut self, path: &str, bytes: Vec<u8>) {
        self.bytes.insert(path.to_string(), bytes);
    }

    /// The file as editable text. EPUB text files are UTF-8 (a UTF-16
    /// straggler decodes lossily and would be saved back as UTF-8 —
    /// the same trade Calibre makes).
    pub fn text(&self, path: &str) -> Option<String> {
        self.bytes.get(path).map(|b| decode_text(b))
    }

    /// One section's files in the tree's order: Text follows the
    /// spine, the rest sort by path. The `mimetype` entry is never
    /// listed — it is container plumbing, not a file to edit.
    pub fn section_files(&self, section: TreeSection) -> Vec<&EpubFile> {
        let mut out: Vec<&EpubFile> = self
            .files
            .iter()
            .filter(|f| f.path != "mimetype" && self.section_of(f) == section)
            .collect();
        match section {
            TreeSection::Text => out.sort_by(|a, b| {
                a.spine_index
                    .unwrap_or(usize::MAX)
                    .cmp(&b.spine_index.unwrap_or(usize::MAX))
                    .then_with(|| a.path.cmp(&b.path))
            }),
            _ => out.sort_by(|a, b| a.path.cmp(&b.path)),
        }
        out
    }

    fn section_of(&self, file: &EpubFile) -> TreeSection {
        if file.is_nav
            || file.path == self.opf_path
            || Some(file.path.as_str()) == self.ncx_path.as_deref()
            || file.path == "META-INF/container.xml"
        {
            return TreeSection::Contents;
        }
        match file.kind {
            FileKind::Html => TreeSection::Text,
            FileKind::Css => TreeSection::Styles,
            FileKind::Image | FileKind::Font => TreeSection::ImagesAndFonts,
            // Package-adjacent XML and anything unrecognised sits with
            // the contents documents rather than among the chapters.
            FileKind::Xml | FileKind::Binary => TreeSection::Contents,
        }
    }

    // ── E4: structural edits ───────────────────────────────────
    // The editor's structure tools mutate the in-memory book and
    // let Save replay the result (snapshot first, as always).
    // These keep `files`, `bytes` and the nav/NCX pointers in
    // step so the tree, Check book and the Preview always agree.

    /// Add a file the book did not have (split piece, import…).
    pub fn add_file(&mut self, file: EpubFile, bytes: Vec<u8>) {
        self.bytes.insert(file.path.clone(), bytes);
        if file.is_nav {
            self.nav_path = Some(file.path.clone());
        }
        self.files.push(file);
    }

    /// Drop a file from the book (delete, merge-away).
    pub fn remove_file(&mut self, path: &str) {
        self.files.retain(|f| f.path != path);
        self.bytes.remove(path);
        if self.nav_path.as_deref() == Some(path) {
            self.nav_path = None;
        }
        if self.ncx_path.as_deref() == Some(path) {
            self.ncx_path = None;
        }
    }

    /// Move a file to a new container path.
    pub fn rename_file(&mut self, old: &str, new: &str) {
        if let Some(bytes) = self.bytes.remove(old) {
            self.bytes.insert(new.to_string(), bytes);
        }
        for file in &mut self.files {
            if file.path == old {
                file.path = new.to_string();
                // A new extension can mean a new nature
                // (page.html → page.xhtml); kind follows.
                file.kind = classify(new, &file.media_type);
            }
        }
        if self.nav_path.as_deref() == Some(old) {
            self.nav_path = Some(new.to_string());
        }
        if self.ncx_path.as_deref() == Some(old) {
            self.ncx_path = Some(new.to_string());
        }
        if self.opf_path == old {
            self.opf_path = new.to_string();
        }
    }

    /// Whole-state capture/restore for the structure-undo pill:
    /// the file list plus every entry's bytes. Books are a few
    /// megabytes; cloning beats reconstructing an inverse op.
    pub fn snapshot_state(&self) -> (Vec<EpubFile>, HashMap<String, Vec<u8>>) {
        (self.files.clone(), self.bytes.clone())
    }

    pub fn restore_state(&mut self, files: Vec<EpubFile>, bytes: HashMap<String, Vec<u8>>) {
        self.nav_path = files.iter().find(|f| f.is_nav).map(|f| f.path.clone());
        if self
            .ncx_path
            .as_ref()
            .is_some_and(|p| !files.iter().any(|f| &f.path == p))
        {
            self.ncx_path = None;
        }
        self.files = files;
        self.bytes = bytes;
    }
}

struct ParsedOpf {
    files: Vec<EpubFile>,
    ncx_path: Option<String>,
}

/// Manifest → files, spine → reading order, `toc` → NCX, cover marks.
/// Deliberately tolerant like the reader's own OPF reading: a book
/// whose OPF will not parse still opens with its files classified by
/// name, because repairing such a book is a reason to open an editor.
fn parse_opf(opf_xml: &str, opf_path: &str) -> ParsedOpf {
    let mut out = ParsedOpf {
        files: Vec::new(),
        ncx_path: None,
    };
    let Ok(doc) = roxmltree::Document::parse(opf_xml) else {
        return out;
    };
    let opf_dir = match opf_path.rfind('/') {
        Some(idx) => &opf_path[..=idx],
        None => "",
    };

    // id -> (href, media-type, properties)
    let mut manifest: HashMap<String, (String, String, String)> = HashMap::new();
    let mut manifest_order: Vec<String> = Vec::new();
    let mut spine_ids: Vec<String> = Vec::new();
    let mut spine_toc: Option<String> = None;
    let mut cover_id: Option<String> = None;

    for node in doc.descendants().filter(|n| n.is_element()) {
        match node.tag_name().name() {
            "item" => {
                let (Some(id), Some(href)) = (node.attribute("id"), node.attribute("href"))
                else {
                    continue;
                };
                manifest_order.push(id.to_string());
                manifest.insert(
                    id.to_string(),
                    (
                        resolve_href(opf_dir, href),
                        node.attribute("media-type").unwrap_or("").to_string(),
                        node.attribute("properties").unwrap_or("").to_string(),
                    ),
                );
            }
            "spine" => {
                spine_toc = node.attribute("toc").map(str::to_string);
            }
            "itemref" => {
                if let Some(idref) = node.attribute("idref") {
                    spine_ids.push(idref.to_string());
                }
            }
            "meta" if node.attribute("name") == Some("cover") => {
                cover_id = node.attribute("content").map(str::to_string);
            }
            _ => {}
        }
    }

    let spine_index: HashMap<&str, usize> = spine_ids
        .iter()
        .enumerate()
        .map(|(i, id)| (id.as_str(), i))
        .collect();
    out.ncx_path = spine_toc
        .as_ref()
        .and_then(|id| manifest.get(id))
        .map(|(path, _, _)| path.clone());

    for id in &manifest_order {
        let Some((path, media_type, properties)) = manifest.get(id) else {
            continue;
        };
        out.files.push(EpubFile {
            kind: classify(path, media_type),
            path: path.clone(),
            media_type: media_type.clone(),
            spine_index: spine_index.get(id.as_str()).copied(),
            is_nav: properties.split_whitespace().any(|p| p == "nav"),
            is_cover_image: properties.split_whitespace().any(|p| p == "cover-image")
                || cover_id.as_deref() == Some(id.as_str()),
            in_manifest: true,
        });
    }
    out
}

/// OPF-relative href → zip entry name: strip the fragment, percent-
/// decode, join onto the OPF's folder and normalise `.`/`..` away.
/// The base is whatever folder the linking file lives in (`""` or
/// `"OEBPS/"`), so Check book resolves chapter links with it too.
pub(crate) fn resolve_href(opf_dir: &str, href: &str) -> String {
    let href = href.split('#').next().unwrap_or("");
    let decoded = crate::epub::percent_decode(href);
    if let Some(rest) = decoded.strip_prefix('/') {
        return normalize_zip_path(rest);
    }
    normalize_zip_path(&format!("{opf_dir}{decoded}"))
}

fn normalize_zip_path(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

pub(crate) fn classify(path: &str, media_type: &str) -> FileKind {
    let lower_path = path.to_ascii_lowercase();
    let ext = lower_path.rsplit('.').next().unwrap_or("");
    let mt = media_type.to_ascii_lowercase();
    if mt.contains("xhtml") || mt == "text/html" || matches!(ext, "xhtml" | "html" | "htm") && !mt.contains("xml") {
        return FileKind::Html;
    }
    if mt == "text/css" || ext == "css" {
        return FileKind::Css;
    }
    if mt.starts_with("image/") || matches!(ext, "png" | "jpg" | "jpeg" | "gif" | "webp" | "svg") {
        return FileKind::Image;
    }
    if mt.starts_with("font/")
        || mt.contains("font")
        || matches!(ext, "woff" | "woff2" | "ttf" | "otf")
    {
        return FileKind::Font;
    }
    if mt.contains("xml") || matches!(ext, "xml" | "opf" | "ncx") {
        return FileKind::Xml;
    }
    FileKind::Binary
}

pub(crate) fn guess_media_type(path: &str) -> &'static str {
    match path
        .to_ascii_lowercase()
        .rsplit('.')
        .next()
        .unwrap_or("")
    {
        "xhtml" | "html" | "htm" => "application/xhtml+xml",
        "css" => "text/css",
        "xml" | "opf" => "application/oebps-package+xml",
        "ncx" => "application/x-dtbncx+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        _ => "application/octet-stream",
    }
}

fn decode_text(bytes: &[u8]) -> String {
    let text = match std::str::from_utf8(bytes) {
        Ok(s) => s.to_string(),
        Err(_) => String::from_utf8_lossy(bytes).into_owned(),
    };
    text.strip_prefix('\u{feff}')
        .map(str::to_string)
        .unwrap_or(text)
}

/// Rewrite the EPUB at `epub_path` with `replacements` applied,
/// keeping every other entry's stored bytes exactly. Entries named
/// in `replacements` but absent from the archive are appended. The
/// replacement is verified to open before it replaces the book.
pub fn write_with_replacements(
    epub_path: &Path,
    replacements: &HashMap<String, Vec<u8>>,
) -> Result<()> {
    rewrite_epub(
        epub_path,
        epub_path,
        replacements,
        &std::collections::HashSet::new(),
    )
}

/// Write a rewritten copy of the EPUB at `source` to `dest`, applying
/// `replacements` and omitting `removals`. With `dest == source` this
/// is the in-place save; with a different `dest` it is how the editor
/// builds the file its live Preview renders (E2) — the book itself is
/// never touched by a preview write.
pub fn rewrite_epub(
    source: &Path,
    dest: &Path,
    replacements: &HashMap<String, Vec<u8>>,
    removals: &std::collections::HashSet<String>,
) -> Result<()> {
    let tmp = dest.with_extension("epub.kalam-tmp");
    let result = (|| -> Result<()> {
        let src = fs::File::open(source)?;
        let mut archive = ZipArchive::new(src)?;
        let dest = fs::File::create(&tmp)?;
        let mut out = ZipWriter::new(dest);

        // The spec requires `mimetype` first and stored uncompressed.
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        if archive.by_name("mimetype").is_ok() {
            out.start_file("mimetype", stored)?;
            out.write_all(b"application/epub+zip")?;
        }
        let deflated =
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

        let mut written: std::collections::HashSet<String> = std::collections::HashSet::new();
        for i in 0..archive.len() {
            let entry = archive.by_index(i)?;
            let name = entry.name().to_string();
            if name == "mimetype" || name.ends_with('/') {
                continue;
            }
            if removals.contains(&name) {
                continue;
            }
            match replacements.get(&name) {
                Some(bytes) => {
                    drop(entry);
                    out.start_file(&name, deflated)?;
                    out.write_all(bytes)?;
                    written.insert(name);
                }
                // Copied without recompressing, preserving the exact bytes.
                None => out.raw_copy_file(entry)?,
            }
        }
        for (name, bytes) in replacements {
            if !written.contains(name) && name != "mimetype" && !removals.contains(name) {
                out.start_file(name, deflated)?;
                out.write_all(bytes)?;
            }
        }
        out.finish()?;

        // A rewrite that cannot be reopened must never replace the book.
        let check = fs::File::open(&tmp)?;
        let mut check = ZipArchive::new(check)?;
        crate::epub::find_opf_path_pub(&mut check)?;
        Ok(())
    })();
    if let Err(err) = result {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }
    fs::rename(&tmp, dest).with_context(|| format!("replace {}", dest.display()))?;
    Ok(())
}

/// The first XML well-formedness problem in `text`, as (1-based line,
/// message) — `None` when the file parses. Deliberately as lenient as
/// quick-xml: tag nesting is checked, undefined HTML entities are
/// left for Check book (decision 132), so a book written by other
/// tools is not flagged for its house style.
pub fn xml_problem(text: &str) -> Option<(usize, String)> {
    use quick_xml::events::Event;
    use quick_xml::Reader;

    let mut reader = Reader::from_str(text);
    reader.config_mut().check_end_names = true;
    let mut buf = Vec::new();
    let mut depth = 0usize;
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(_)) => depth += 1,
            Ok(Event::End(_)) => depth = depth.saturating_sub(1),
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(err) => {
                let pos = (reader.error_position() as usize).min(text.len());
                let line = 1 + text[..pos].matches('\n').count();
                return Some((line, err.to_string()));
            }
        }
        buf.clear();
    }
    if depth > 0 {
        return Some((
            text.matches('\n').count() + 1,
            "Unclosed elements at the end of the file".to_string(),
        ));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONTAINER: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;

    const OPF: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="bookid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="bookid">urn:uuid:test</dc:identifier>
    <dc:title>Fixture</dc:title><dc:language>en</dc:language>
  </metadata>
  <manifest>
    <item id="nav" href="nav.xhtml" media-type="application/xhtml+xml" properties="nav"/>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    <item id="ch2" href="text/ch2.xhtml" media-type="application/xhtml+xml"/>
    <item id="ch1" href="text/ch1.xhtml" media-type="application/xhtml+xml"/>
    <item id="css" href="style/main.css" media-type="text/css"/>
    <item id="cover" href="images/cover.jpg" media-type="image/jpeg" properties="cover-image"/>
    <item id="font" href="fonts/book.woff2" media-type="font/woff2"/>
  </manifest>
  <spine toc="ncx">
    <itemref idref="ch1"/><itemref idref="ch2"/>
  </spine>
</package>"#;

    /// A minimal but valid EPUB in a fresh temp folder, plus its path.
    fn fixture() -> (std::path::PathBuf, std::path::PathBuf) {
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "kalam-epub-container-test-{}-{stamp}",
            std::process::id()
        ));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join("book.epub");

        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        let deflated =
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        let mut zip = ZipWriter::new(fs::File::create(&path).expect("create epub"));
        let entries: &[(&str, &[u8], bool)] = &[
            ("mimetype", b"application/epub+zip", true),
            ("META-INF/container.xml", CONTAINER.as_bytes(), false),
            ("OEBPS/content.opf", OPF.as_bytes(), false),
            ("OEBPS/nav.xhtml", b"<html><body><nav/></body></html>", false),
            ("OEBPS/toc.ncx", b"<ncx/>", false),
            ("OEBPS/text/ch1.xhtml", b"<html><body><p>One</p></body></html>", false),
            ("OEBPS/text/ch2.xhtml", b"<html><body><p>Two</p></body></html>", false),
            ("OEBPS/style/main.css", b"p { margin: 0; }", false),
            ("OEBPS/images/cover.jpg", b"\xff\xd8\xff\xd9", false),
            ("OEBPS/fonts/book.woff2", b"wOF2", false),
        ];
        for (name, bytes, is_stored) in entries {
            zip.start_file(*name, if *is_stored { stored } else { deflated })
                .expect("start entry");
            zip.write_all(bytes).expect("write entry");
        }
        zip.finish().expect("finish epub");
        (dir, path)
    }

    #[test]
    fn groups_files_into_tree_sections() {
        let (dir, path) = fixture();
        let book = EpubContainer::open(&path).expect("open fixture");

        let text: Vec<&str> = book
            .section_files(TreeSection::Text)
            .iter()
            .map(|f| f.path.as_str())
            .collect();
        // Spine order, not manifest order (ch2 is declared first).
        assert_eq!(text, ["OEBPS/text/ch1.xhtml", "OEBPS/text/ch2.xhtml"]);

        let styles: Vec<&str> = book
            .section_files(TreeSection::Styles)
            .iter()
            .map(|f| f.path.as_str())
            .collect();
        assert_eq!(styles, ["OEBPS/style/main.css"]);

        let media: Vec<&str> = book
            .section_files(TreeSection::ImagesAndFonts)
            .iter()
            .map(|f| f.path.as_str())
            .collect();
        assert_eq!(media, ["OEBPS/fonts/book.woff2", "OEBPS/images/cover.jpg"]);

        let contents: Vec<&str> = book
            .section_files(TreeSection::Contents)
            .iter()
            .map(|f| f.path.as_str())
            .collect();
        assert!(contents.contains(&"OEBPS/nav.xhtml"), "{contents:?}");
        assert!(contents.contains(&"OEBPS/toc.ncx"), "{contents:?}");
        assert!(contents.contains(&"OEBPS/content.opf"), "{contents:?}");
        assert!(contents.contains(&"META-INF/container.xml"), "{contents:?}");

        assert_eq!(book.nav_path.as_deref(), Some("OEBPS/nav.xhtml"));
        assert_eq!(book.ncx_path.as_deref(), Some("OEBPS/toc.ncx"));
        assert!(
            book.file("OEBPS/images/cover.jpg")
                .map(|f| f.is_cover_image)
                .unwrap_or(false)
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn write_round_trip_replaces_only_the_named_file() {
        let (dir, path) = fixture();
        let mut replacements = HashMap::new();
        replacements.insert(
            "OEBPS/text/ch1.xhtml".to_string(),
            b"<html><body><p>Edited</p></body></html>".to_vec(),
        );
        write_with_replacements(&path, &replacements).expect("rewrite");

        let book = EpubContainer::open(&path).expect("reopen");
        assert_eq!(
            book.text("OEBPS/text/ch1.xhtml").as_deref(),
            Some("<html><body><p>Edited</p></body></html>")
        );
        assert_eq!(
            book.bytes("OEBPS/style/main.css"),
            Some(b"p { margin: 0; }".as_slice())
        );
        assert_eq!(
            book.bytes("OEBPS/images/cover.jpg"),
            Some(b"\xff\xd8\xff\xd9".as_slice())
        );
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn xml_problem_reports_line_of_first_error() {
        assert_eq!(xml_problem("<html><body><p>x</p></body></html>"), None);
        let (line, _) = xml_problem("<html>\n<body>\n<p>x</body>\n</html>")
            .expect("mismatched tags are a problem");
        assert_eq!(line, 3);
        assert!(xml_problem("<html><body>").is_some(), "unclosed tags");
    }
}
