//! EPUB Ingestion Sanitizer & Polish.
//!
//! Cleans up messy and toxic real-world EPUB files upon import:
//! - Strips toxic CSS (tiny hardcoded 8px fonts, wide fixed margins, forced black/white colors)
//! - Cleans toxic inline styles on body/paragraph elements
//! - Repairs malformed XML entities (bare ampersands) and unclosed void tags
//! - Auto-generates Table of Contents (toc.ncx) from headings when missing
//! - Verifies and preserves container compliance (uncompressed mimetype, valid zip)

use anyhow::{anyhow, Context, Result};
use std::fs;
use std::io::{Read, Seek, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Detailed report of cleaning actions performed on an EPUB.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SanitizerReport {
    pub cleaned_css_rules: usize,
    pub cleaned_inline_styles: usize,
    pub fixed_xml_entities: usize,
    pub generated_toc: bool,
    pub modified: bool,
}

/// Chapter heading candidate extracted for Table of Contents generation.
#[derive(Debug, Clone)]
struct TocHeading {
    title: String,
    target_href: String,
    anchor_id: Option<String>,
}

/// Preference key controlling whether EPUBs are polished and sanitized upon import.
pub const PREF_CLEAN_EPUB: &str = "import.clean_epub";

/// True unless the user has explicitly disabled EPUB sanitizing on import.
pub fn clean_on_import_enabled(catalog: &crate::db::Catalog) -> bool {
    catalog
        .get_pref(PREF_CLEAN_EPUB)
        .map(|v| v != "0" && v != "false")
        .unwrap_or(true)
}

/// Set EPUB sanitizing on import preference.
pub fn set_clean_on_import(catalog: &crate::db::Catalog, on: bool) {
    catalog.set_pref(PREF_CLEAN_EPUB, if on { "1" } else { "0" });
}

/// Sanitize an EPUB file in-place, creating a backup `.orig` if requested.
pub fn sanitize_epub(path: &Path, backup: bool) -> Result<SanitizerReport> {
    if !path.is_file() {
        return Err(anyhow!("file not found: {}", path.display()));
    }

    let mut report = SanitizerReport::default();

    // ── 1. Read existing archive and manifest ────────────────────────────
    let (opf_path, opf_xml, entries_to_clean) = {
        let file = fs::File::open(path)?;
        let mut archive = ZipArchive::new(file).context("open EPUB for sanitizing")?;
        let opf_path = crate::epub::find_opf_path_pub(&mut archive)?;
        let opf_xml = read_entry_string(&mut archive, &opf_path)?;
        let items = parse_manifest_items(&opf_xml, &opf_path);
        (opf_path, opf_xml, items)
    };

    // ── 2. Inspect & Sanitize CSS stylesheets ────────────────────────────
    let mut modified_files: std::collections::HashMap<String, Vec<u8>> =
        std::collections::HashMap::new();

    {
        let file = fs::File::open(path)?;
        let mut archive = ZipArchive::new(file)?;

        for item in &entries_to_clean.css_paths {
            if let Ok(css) = read_entry_string(&mut archive, item) {
                let (clean_css, count) = sanitize_css_content(&css);
                if count > 0 {
                    report.cleaned_css_rules += count;
                    report.modified = true;
                    modified_files.insert(item.clone(), clean_css.into_bytes());
                }
            }
        }
    }

    // ── 3. Inspect & Sanitize Spine XHTML documents ──────────────────────
    let mut toc_headings: Vec<TocHeading> = Vec::new();
    let mut heading_counter = 0usize;

    {
        let file = fs::File::open(path)?;
        let mut archive = ZipArchive::new(file)?;

        for spine_href in &entries_to_clean.spine_paths {
            if let Ok(xhtml) = read_entry_string(&mut archive, spine_href) {
                let (clean_xhtml, inline_count, xml_count, headings) =
                    sanitize_xhtml_content(&xhtml, spine_href, &mut heading_counter);

                if inline_count > 0 || xml_count > 0 || clean_xhtml != xhtml {
                    report.cleaned_inline_styles += inline_count;
                    report.fixed_xml_entities += xml_count;
                    report.modified = true;
                    modified_files.insert(spine_href.clone(), clean_xhtml.into_bytes());
                }

                if entries_to_clean.toc_missing {
                    toc_headings.extend(headings);
                }
            }
        }
    }

    // ── 4. Generate Table of Contents if missing ─────────────────────────
    let mut new_opf_xml = opf_xml.clone();
    if entries_to_clean.toc_missing && !toc_headings.is_empty() {
        let opf_dir = opf_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
        let ncx_name = "toc.ncx";
        let ncx_full_path = if opf_dir.is_empty() {
            ncx_name.to_string()
        } else {
            format!("{opf_dir}/{ncx_name}")
        };

        let book_title = extract_title_from_opf(&opf_xml).unwrap_or_else(|| "Book".to_string());
        let ncx_content = build_ncx_content(&book_title, &toc_headings, opf_dir);

        modified_files.insert(ncx_full_path.clone(), ncx_content.into_bytes());

        // Update OPF to reference the new toc.ncx
        new_opf_xml = inject_ncx_into_opf(&new_opf_xml, ncx_name);
        report.generated_toc = true;
        report.modified = true;
    }

    if new_opf_xml != opf_xml {
        modified_files.insert(opf_path.clone(), new_opf_xml.into_bytes());
    }

    // If nothing needed sanitizing, return early with zero mutations.
    if !report.modified {
        return Ok(report);
    }

    // ── 5. Back up original if requested ────────────────────────────────
    if backup {
        let orig_path = crate::epub_metadata::backup_path(path);
        if !orig_path.exists() {
            let _ = fs::copy(path, &orig_path);
        }
    }

    // ── 6. Repack into atomic temporary file ─────────────────────────────
    let tmp = path.with_extension("epub.kalam-tmp");
    {
        let src = fs::File::open(path)?;
        let mut archive = ZipArchive::new(src)?;
        let dest = fs::File::create(&tmp)?;
        let mut out = ZipWriter::new(dest);

        // Spec requires `mimetype` first and stored uncompressed
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
                continue;
            }

            if let Some(replacement) = modified_files.remove(&name) {
                drop(entry);
                out.start_file(&name, deflated)?;
                out.write_all(&replacement)?;
            } else {
                out.raw_copy_file(entry)?;
            }
        }

        // Add any newly generated files (e.g. toc.ncx)
        for (name, content) in modified_files {
            out.start_file(&name, deflated)?;
            out.write_all(&content)?;
        }

        out.finish()?;
    }

    // ── 7. Verify and replace ───────────────────────────────────────────
    if let Err(err) = verify_archive(&tmp, &opf_path) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }

    fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))?;
    Ok(report)
}

/// What one bake did: which patches were written into the file, which
/// could not be (and why, as panel-ready text), and how many entries
/// changed. `failed` patches stay pending — a bake never silently skips
/// a patch it was told to apply.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BakeReport {
    pub applied: Vec<i64>,
    pub failed: Vec<(i64, String)>,
    pub entries_patched: usize,
}

/// Bake pending edits into the EPUB file itself (Phase 6 step 5) — the
/// moment the virtual becomes real.
///
/// Same skeleton as [`sanitize_epub`]: mimetype first and stored, every
/// other entry raw-copied, patched entries written deflated, all into a
/// temp file that is verified before the swap, with the first bake's
/// `.orig` backup kept as the undo story. Two things are stricter here:
/// every patched entry must parse with the reader's own parser before
/// anything is written (a fix that would break the chapter refuses the
/// whole bake — the file on disk is never left worse), and every patch
/// that fails to match is reported in the [`BakeReport`] rather than
/// skipped.
///
/// The caller owns the database side — marking patches applied,
/// re-hashing the book row, reindexing search — so this stays a pure
/// file operation that tests can run against a zip alone.
pub fn bake_epub(path: &Path, patches: &[crate::db::PatchRecord]) -> Result<BakeReport> {
    if !path.is_file() {
        return Err(anyhow!("file not found: {}", path.display()));
    }
    let mut report = BakeReport::default();

    // Group the patches by their href. Patches carry the spine href the
    // reader showed when the edit was made; all patches on one entry
    // share that string, and the matcher matches against exactly it.
    // Records are small strings — cloned per group so the matcher takes
    // its own `&[PatchRecord]`.
    let mut groups: std::collections::HashMap<String, Vec<crate::db::PatchRecord>> =
        std::collections::HashMap::new();
    for p in patches {
        groups.entry(p.href.clone()).or_default().push(p.clone());
    }
    if groups.is_empty() {
        return Ok(report);
    }

    // Resolve each href group to the zip member it names — raw first,
    // then percent-decoded, the tolerance chapbook-epub's manifest
    // lookup applies. A group whose entry is gone fails its patches.
    let mut modified_files: std::collections::HashMap<String, Vec<u8>> =
        std::collections::HashMap::new();
    {
        let file = fs::File::open(path)?;
        let mut archive = ZipArchive::new(file).context("open EPUB for baking")?;
        let mut hrefs: Vec<String> = groups.keys().cloned().collect();
        hrefs.sort();
        for href in hrefs {
            let group = groups.remove(&href).unwrap_or_default();
            let entry_name = if archive.by_name(&href).is_ok() {
                href.clone()
            } else if archive.by_name(&percent_decode_path(&href)).is_ok() {
                percent_decode_path(&href)
            } else {
                for p in &group {
                    report.failed.push((
                        p.id,
                        "its chapter is not in the book's file".to_string(),
                    ));
                }
                continue;
            };
            let mut bytes = Vec::new();
            archive
                .by_name(&entry_name)?
                .read_to_end(&mut bytes)
                .with_context(|| format!("read {entry_name}"))?;
            // The matcher applies the group in creation order — the same
            // order the reader's entry filter applied when the later
            // patches were planned, so what verified then holds here.
            let (new_bytes, outcomes) =
                crate::epub_patches::apply_text_patches(&href, &bytes, &group);
            for outcome in &outcomes {
                match &outcome.resolution {
                    crate::epub_patches::Resolution::Found(_) => report.applied.push(outcome.id),
                    crate::epub_patches::Resolution::NotFound => report.failed.push((
                        outcome.id,
                        "its text is no longer in the chapter — the file changed".to_string(),
                    )),
                    crate::epub_patches::Resolution::Ambiguous(n) => report
                        .failed
                        .push((outcome.id, format!("it now appears {n} times"))),
                    crate::epub_patches::Resolution::Stale => report.failed.push((
                        outcome.id,
                        "the chapter changed after this whole-file edit — re-make it in the \
                         raw editor"
                            .to_string(),
                    )),
                }
            }
            // Unchanged bytes — every patch in this group failed to
            // match — leave the entry exactly as it is.
            if new_bytes == bytes {
                continue;
            }
            // The parse gate. `parse_xhtml`, the reader's parser, has a
            // lenient HTML fallback that would shrug at an unclosed tag
            // our splice introduced — so the gate is stricter than the
            // reader: roxmltree, no fallback. An entry that was already
            // not well-formed XML (real-world books exist) keeps its
            // lenient path; one that was well-formed must stay that way.
            if entry_parses_as_xml(&bytes) && !entry_parses_as_xml(&new_bytes) {
                return Err(anyhow!(
                    "the correction would make {href} unparseable — nothing was written"
                ));
            }
            modified_files.insert(entry_name, new_bytes);
        }
    }

    report.entries_patched = modified_files.len();
    if modified_files.is_empty() {
        // Nothing matched; the file stays as it is, byte for byte.
        return Ok(report);
    }

    // The undo story: the first bake's backup is the pristine file.
    {
        let orig_path = crate::epub_metadata::backup_path(path);
        if !orig_path.exists() {
            fs::copy(path, &orig_path).context("back up the EPUB before baking")?;
        }
    }

    // Repack — the sanitizer's skeleton, verbatim in shape.
    let tmp = path.with_extension("epub.kalam-tmp");
    {
        let src = fs::File::open(path)?;
        let mut archive = ZipArchive::new(src)?;
        let dest = fs::File::create(&tmp)?;
        let mut out = ZipWriter::new(dest);

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
                continue;
            }
            if let Some(replacement) = modified_files.remove(&name) {
                drop(entry);
                out.start_file(&name, deflated)?;
                out.write_all(&replacement)?;
            } else {
                out.raw_copy_file(entry)?;
            }
        }
        for (name, content) in modified_files {
            out.start_file(&name, deflated)?;
            out.write_all(&content)?;
        }
        out.finish()?;
    }

    let opf_path = {
        let file = fs::File::open(&tmp)?;
        let mut archive = ZipArchive::new(file)?;
        crate::epub::find_opf_path_pub(&mut archive)?
    };
    if let Err(err) = verify_archive(&tmp, &opf_path) {
        let _ = fs::remove_file(&tmp);
        return Err(err);
    }

    fs::rename(&tmp, path).with_context(|| format!("replace {}", path.display()))?;
    Ok(report)
}

/// Percent-decode a container path — the tolerance chapbook-epub's
/// manifest lookup applies. A package may list a href encoded while the
/// zip member is decoded; baking has to find the member either way.
fn percent_decode_path(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    let hex = |b: u8| -> Option<u8> {
        match b {
            b'0'..=b'9' => Some(b - b'0'),
            b'a'..=b'f' => Some(b - b'a' + 10),
            b'A'..=b'F' => Some(b - b'A' + 10),
            _ => None,
        }
    };
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            if let (Some(hi), Some(lo)) = (hex(bytes[i + 1]), hex(bytes[i + 2])) {
                out.push(hi << 4 | lo);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

struct ManifestInfo {
    css_paths: Vec<String>,
    spine_paths: Vec<String>,
    toc_missing: bool,
}

fn parse_manifest_items(opf_xml: &str, opf_path: &str) -> ManifestInfo {
    let opf_dir = opf_path.rsplit_once('/').map(|(d, _)| d).unwrap_or("");
    let doc = match roxmltree::Document::parse(opf_xml) {
        Ok(d) => d,
        Err(_) => {
            return ManifestInfo {
                css_paths: Vec::new(),
                spine_paths: Vec::new(),
                toc_missing: false,
            };
        }
    };

    let mut css_paths = Vec::new();
    let mut manifest_map = std::collections::HashMap::new();
    let mut has_ncx = false;
    let mut has_nav = false;

    for node in doc.descendants().filter(|n| n.tag_name().name() == "item") {
        let id = node.attribute("id").unwrap_or("");
        let href = node.attribute("href").unwrap_or("");
        let media_type = node.attribute("media-type").unwrap_or("");
        let properties = node.attribute("properties").unwrap_or("");

        let full_path = if opf_dir.is_empty() {
            href.to_string()
        } else {
            format!("{opf_dir}/{href}")
        };

        if media_type == "text/css" || href.ends_with(".css") {
            css_paths.push(full_path.clone());
        }

        if media_type == "application/x-dtbncx+xml" || id == "ncx" || href.ends_with(".ncx") {
            has_ncx = true;
        }

        if properties.contains("nav") {
            has_nav = true;
        }

        manifest_map.insert(id.to_string(), full_path);
    }

    let mut spine_paths = Vec::new();
    for node in doc.descendants().filter(|n| n.tag_name().name() == "itemref") {
        if let Some(idref) = node.attribute("idref") {
            if let Some(path) = manifest_map.get(idref) {
                spine_paths.push(path.clone());
            }
        }
    }

    ManifestInfo {
        css_paths,
        spine_paths,
        toc_missing: !has_ncx && !has_nav,
    }
}

// ---------------------------------------------------------------------------
// CSS Sanitization
// ---------------------------------------------------------------------------

/// Strips toxic CSS rules from a stylesheet:
/// - Tiny/fixed font-size on body, html, p, div
/// - Forced black/white text or background colors
/// - Fixed wide margins or max-width containers
/// - !important font overrides
pub fn sanitize_css_content(css: &str) -> (String, usize) {
    let mut cleaned_count = 0;
    let mut out = String::with_capacity(css.len());
    let mut chars = css.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '/' && chars.peek() == Some(&'*') {
            // Preserve comment verbatim
            chars.next();
            out.push_str("/*");
            while let Some(c) = chars.next() {
                out.push(c);
                if c == '*' && chars.peek() == Some(&'/') {
                    chars.next();
                    out.push('/');
                    break;
                }
            }
            continue;
        }

        if ch == '{' {
            // Find selector from `out`
            let last_rule_start = out.rfind(['}', ';']).map(|i| i + 1).unwrap_or(0);
            let selector = out[last_rule_start..].trim().to_ascii_lowercase();

            // Extract declarations block
            let mut block = String::new();
            let mut depth = 1;
            for c in chars.by_ref() {
                if c == '{' {
                    depth += 1;
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                }
                block.push(c);
            }

            let (clean_block, count) = clean_css_declarations(&selector, &block);
            cleaned_count += count;
            out.push('{');
            out.push_str(&clean_block);
            out.push('}');
            continue;
        }

        out.push(ch);
    }

    (out, cleaned_count)
}

fn clean_css_declarations(selector: &str, block: &str) -> (String, usize) {
    let is_root_target = selector.contains("body")
        || selector.contains("html")
        || selector.contains(":root")
        || selector == "p"
        || selector.starts_with("p ")
        || selector.ends_with(" p")
        || selector.contains(" p ")
        || selector == "div"
        || selector.starts_with("div ");

    let mut clean_decls = Vec::new();
    let mut cleaned_count = 0;

    for decl in block.split(';') {
        let trimmed = decl.trim();
        if trimmed.is_empty() {
            continue;
        }

        let Some((prop, val)) = trimmed.split_once(':') else {
            clean_decls.push(trimmed.to_string());
            continue;
        };

        let prop_clean = prop.trim().to_ascii_lowercase();
        let val_clean = val.trim().to_ascii_lowercase();

        // 1. Toxic font-size on reading elements
        if prop_clean == "font-size" {
            let has_important = val_clean.contains("!important");
            let is_tiny = is_tiny_font_size(&val_clean);
            if is_root_target && (has_important || is_tiny) {
                cleaned_count += 1;
                continue;
            }
        }

        // 2. Forced black/white text colors
        if prop_clean == "color" && is_root_target && is_forced_color(&val_clean) {
            cleaned_count += 1;
            continue;
        }

        // 3. Forced background colors
        if (prop_clean == "background" || prop_clean == "background-color")
            && is_root_target
            && is_forced_background(&val_clean)
        {
            cleaned_count += 1;
            continue;
        }

        // 4. Fixed width or huge margins on body/html
        if (selector.contains("body") || selector.contains("html"))
            && is_toxic_box_dimension(&prop_clean, &val_clean)
        {
            cleaned_count += 1;
            continue;
        }

        // 5. Collapsed line-height
        if prop_clean == "line-height" && is_collapsed_line_height(&val_clean) {
            cleaned_count += 1;
            continue;
        }

        // 6. Strip !important on font-family
        if prop_clean == "font-family" && val_clean.contains("!important") {
            let without_important = val.replace("!important", "").trim().to_string();
            clean_decls.push(format!("{prop}: {without_important}"));
            cleaned_count += 1;
            continue;
        }

        clean_decls.push(trimmed.to_string());
    }

    let joined = if clean_decls.is_empty() {
        String::new()
    } else {
        format!(" {} ", clean_decls.join("; "))
    };

    (joined, cleaned_count)
}

fn is_tiny_font_size(val: &str) -> bool {
    let clean = val.replace("!important", "").trim().to_ascii_lowercase();
    if let Some(px_str) = clean.strip_suffix("px") {
        if let Ok(px) = px_str.trim().parse::<f32>() {
            return px > 0.0 && px <= 11.0;
        }
    }
    if let Some(pt_str) = clean.strip_suffix("pt") {
        if let Ok(pt) = pt_str.trim().parse::<f32>() {
            return pt > 0.0 && pt <= 9.0;
        }
    }
    if let Some(em_str) = clean.strip_suffix("em") {
        if let Ok(em) = em_str.trim().parse::<f32>() {
            return em > 0.0 && em <= 0.65;
        }
    }
    false
}

fn is_forced_color(val: &str) -> bool {
    let clean = val.replace("!important", "").trim().to_ascii_lowercase();
    matches!(
        clean.as_str(),
        "#000"
            | "#000000"
            | "black"
            | "rgb(0,0,0)"
            | "rgb(0, 0, 0)"
            | "#111"
            | "#111111"
            | "#fff"
            | "#ffffff"
            | "white"
            | "rgb(255,255,255)"
            | "rgb(255, 255, 255)"
    )
}

fn is_forced_background(val: &str) -> bool {
    let clean = val.replace("!important", "").trim().to_ascii_lowercase();
    matches!(
        clean.as_str(),
        "#fff"
            | "#ffffff"
            | "white"
            | "rgb(255,255,255)"
            | "rgb(255, 255, 255)"
            | "#000"
            | "#000000"
            | "black"
            | "rgb(0,0,0)"
            | "rgb(0, 0, 0)"
    )
}

fn is_toxic_box_dimension(prop: &str, val: &str) -> bool {
    let clean = val.replace("!important", "").trim().to_ascii_lowercase();
    if (prop == "width" || prop == "max-width") && (clean.ends_with("px") || clean.ends_with("pt"))
    {
        return true;
    }
    if matches!(
        prop,
        "margin" | "margin-left" | "margin-right" | "padding-left" | "padding-right"
    ) {
        for part in clean.split_whitespace() {
            if let Some(px) = part
                .strip_suffix("px")
                .and_then(|s| s.trim().parse::<f32>().ok())
            {
                if px >= 40.0 {
                    return true;
                }
            } else if let Some(pt) = part
                .strip_suffix("pt")
                .and_then(|s| s.trim().parse::<f32>().ok())
            {
                if pt >= 30.0 {
                    return true;
                }
            }
        }
    }
    false
}

fn is_collapsed_line_height(val: &str) -> bool {
    let clean = val.replace("!important", "").trim().to_ascii_lowercase();
    if let Ok(num) = clean.parse::<f32>() {
        return num > 0.0 && num < 0.9;
    }
    if let Some(px_str) = clean.strip_suffix("px") {
        if let Ok(px) = px_str.trim().parse::<f32>() {
            return px > 0.0 && px <= 11.0;
        }
    }
    false
}

// ---------------------------------------------------------------------------
// XHTML Sanitization
// ---------------------------------------------------------------------------

/// Sanitizes XHTML content:
/// - Fixes unescaped bare ampersands
/// - Closes void tags (<br> -> <br />, <hr> -> <hr />)
/// - Cleans toxic inline styles on reading elements
/// - Collects chapter headings for TOC generation
fn sanitize_xhtml_content(
    xhtml: &str,
    spine_href: &str,
    heading_counter: &mut usize,
) -> (String, usize, usize, Vec<TocHeading>) {
    let mut inline_cleaned = 0;
    let mut xml_fixed = 0;
    let mut headings = Vec::new();
    let mut out = String::with_capacity(xhtml.len() + 128);

    let mut i = 0;
    let bytes = xhtml.as_bytes();
    let len = bytes.len();

    while i < len {
        // 1. Bare ampersand repair (e.g. `Tom & Jerry` -> `Tom &amp; Jerry`)
        if bytes[i] == b'&' {
            let is_entity = is_valid_xml_entity(&xhtml[i..]);
            if !is_entity {
                out.push_str("&amp;");
                xml_fixed += 1;
                i += 1;
                continue;
            }
        }

        // 2. Tag inspection
        if bytes[i] == b'<' && i + 1 < len && bytes[i + 1] != b'!' && bytes[i + 1] != b'?' {
            // Find end of tag
            let tag_start = i;
            let mut tag_end = i + 1;
            while tag_end < len && bytes[tag_end] != b'>' {
                tag_end += 1;
            }

            if tag_end < len {
                let tag_str = &xhtml[tag_start..=tag_end];

                // Check for unclosed void tags: <br> or <hr>
                let tag_lower = tag_str.to_ascii_lowercase();
                if tag_lower.starts_with("<br") && !tag_str.ends_with("/>") {
                    out.push_str("<br />");
                    xml_fixed += 1;
                    i = tag_end + 1;
                    continue;
                }
                if tag_lower.starts_with("<hr") && !tag_str.ends_with("/>") {
                    out.push_str("<hr />");
                    xml_fixed += 1;
                    i = tag_end + 1;
                    continue;
                }

                // Check for heading tag: <h1>, <h2>, <h3>
                let is_heading = tag_lower.starts_with("<h1")
                    || tag_lower.starts_with("<h2")
                    || tag_lower.starts_with("<h3");

                if is_heading && !tag_lower.starts_with("</") {
                    let mut tag_buf = tag_str.to_string();
                    let existing_id = extract_attr_val(tag_str, "id");
                    let anchor_id = if let Some(id) = existing_id {
                        id
                    } else {
                        *heading_counter += 1;
                        let gen_id = format!("kalam-toc-{}", *heading_counter);
                        // Inject id into opening tag
                        if let Some(pos) = tag_buf.find('>') {
                            tag_buf.insert_str(pos, &format!(" id=\"{gen_id}\""));
                        }
                        gen_id
                    };

                    // Extract heading text until closing </h...>
                    let close_tag = match &tag_lower[1..3] {
                        "h1" => "</h1",
                        "h2" => "</h2",
                        _ => "</h3",
                    };
                    let text_start = tag_end + 1;
                    let title_text = if let Some(close_pos) = xhtml[text_start..].to_ascii_lowercase().find(close_tag) {
                        let inner = &xhtml[text_start..text_start + close_pos];
                        strip_xml_tags(inner)
                    } else {
                        String::new()
                    };

                    let clean_title = title_text.trim().to_string();
                    if !clean_title.is_empty() && clean_title.len() < 120 {
                        headings.push(TocHeading {
                            title: clean_title,
                            target_href: spine_href.to_string(),
                            anchor_id: Some(anchor_id),
                        });
                    }

                    out.push_str(&tag_buf);
                    i = tag_end + 1;
                    continue;
                }

                // Check for inline style cleaning
                if tag_str.contains("style=") {
                    let (clean_tag, count) = clean_inline_style_attribute(tag_str);
                    if count > 0 {
                        inline_cleaned += count;
                        out.push_str(&clean_tag);
                        i = tag_end + 1;
                        continue;
                    }
                }
            }
        }

        out.push(xhtml[i..].chars().next().unwrap_or(' '));
        i += xhtml[i..].chars().next().map_or(1, |c| c.len_utf8());
    }

    (out, inline_cleaned, xml_fixed, headings)
}

fn is_valid_xml_entity(slice: &str) -> bool {
    let mut chars = slice.chars();
    if chars.next() != Some('&') {
        return false;
    }
    let mut len = 0;
    for c in chars {
        if c == ';' {
            return len > 0;
        }
        if !c.is_alphanumeric() && c != '#' {
            return false;
        }
        len += 1;
        if len > 10 {
            return false;
        }
    }
    false
}

fn extract_attr_val(tag: &str, attr: &str) -> Option<String> {
    let pattern = format!("{attr}=\"");
    if let Some(start) = tag.find(&pattern) {
        let val_start = start + pattern.len();
        if let Some(end) = tag[val_start..].find('"') {
            return Some(tag[val_start..val_start + end].to_string());
        }
    }
    None
}

fn clean_inline_style_attribute(tag: &str) -> (String, usize) {
    let pattern = "style=\"";
    let Some(start) = tag.find(pattern) else {
        return (tag.to_string(), 0);
    };
    let val_start = start + pattern.len();
    let Some(end) = tag[val_start..].find('"') else {
        return (tag.to_string(), 0);
    };

    let style_val = &tag[val_start..val_start + end];
    let (clean_style, count) = clean_css_declarations("p", style_val);
    if count == 0 {
        return (tag.to_string(), 0);
    }

    let mut out = String::new();
    out.push_str(&tag[..start]);
    let clean_trimmed = clean_style.trim();
    if !clean_trimmed.is_empty() {
        out.push_str(&format!("style=\"{clean_trimmed}\""));
    }
    out.push_str(&tag[val_start + end + 1..]);
    (out, count)
}

fn strip_xml_tags(html: &str) -> String {
    let mut out = String::new();
    let mut inside = false;
    for c in html.chars() {
        if c == '<' {
            inside = true;
        } else if c == '>' {
            inside = false;
        } else if !inside {
            out.push(c);
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Table of Contents (TOC) Generation
// ---------------------------------------------------------------------------

fn extract_title_from_opf(opf_xml: &str) -> Option<String> {
    let doc = roxmltree::Document::parse(opf_xml).ok()?;
    doc.descendants()
        .find(|n| n.tag_name().name() == "title")
        .and_then(|n| n.text())
        .map(|t| t.trim().to_string())
}

fn build_ncx_content(book_title: &str, headings: &[TocHeading], opf_dir: &str) -> String {
    let mut nav_points = String::new();
    for (idx, h) in headings.iter().enumerate() {
        let order = idx + 1;
        let mut rel_href = h.target_href.clone();
        if !opf_dir.is_empty() && rel_href.starts_with(opf_dir) {
            rel_href = rel_href[opf_dir.len()..].trim_start_matches('/').to_string();
        }
        let target = if let Some(anchor) = &h.anchor_id {
            format!("{rel_href}#{anchor}")
        } else {
            rel_href
        };

        let safe_title = quick_xml_escape(&h.title);

        nav_points.push_str(&format!(
            "    <navPoint id=\"navPoint-{order}\" playOrder=\"{order}\">\n      <navLabel><text>{safe_title}</text></navLabel>\n      <content src=\"{target}\"/>\n    </navPoint>\n"
        ));
    }

    let safe_book_title = quick_xml_escape(book_title);
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\">\n\
           <head>\n\
             <meta name=\"dtb:uid\" content=\"urn:uuid:kalam-generated-toc\"/>\n\
             <meta name=\"dtb:depth\" content=\"1\"/>\n\
             <meta name=\"dtb:totalPageCount\" content=\"0\"/>\n\
             <meta name=\"dtb:maxPageNumber\" content=\"0\"/>\n\
           </head>\n\
           <docTitle><text>{safe_book_title}</text></docTitle>\n\
           <navMap>\n{nav_points}  </navMap>\n\
         </ncx>\n"
    )
}

fn inject_ncx_into_opf(opf_xml: &str, ncx_filename: &str) -> String {
    let mut out = opf_xml.to_string();

    // 1. Add item to <manifest>
    let ncx_item = format!(
        "    <item id=\"ncx\" href=\"{ncx_filename}\" media-type=\"application/x-dtbncx+xml\"/>\n"
    );
    if let Some(pos) = out.find("</manifest>") {
        out.insert_str(pos, &ncx_item);
    }

    // 2. Ensure <spine toc="ncx">
    if let Some(pos) = out.find("<spine") {
        if let Some(end) = out[pos..].find('>') {
            let spine_tag = &out[pos..pos + end];
            if !spine_tag.contains("toc=") {
                out.insert_str(pos + 6, " toc=\"ncx\"");
            }
        }
    }

    out
}

fn quick_xml_escape(raw: &str) -> String {
    raw.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ---------------------------------------------------------------------------
// Archive Verification & Helpers
// ---------------------------------------------------------------------------

fn verify_archive(path: &Path, opf_path: &str) -> Result<()> {
    let file = fs::File::open(path)?;
    let mut archive = ZipArchive::new(file).context("rewritten EPUB is not a valid zip")?;
    archive
        .by_name(opf_path)
        .map(|_| ())
        .context("rewritten EPUB has no OPF")?;
    Ok(())
}

/// The bake's parse gate: strict XML, no fallback (see `bake_epub`).
fn entry_parses_as_xml(bytes: &[u8]) -> bool {
    match std::str::from_utf8(bytes) {
        Ok(text) => roxmltree::Document::parse(text).is_ok(),
        Err(_) => false,
    }
}

fn read_entry_string<R: Read + Seek>(archive: &mut ZipArchive<R>, name: &str) -> Result<String> {
    let mut entry = archive
        .by_name(name)
        .with_context(|| format!("missing {name} in EPUB"))?;
    let mut buf = String::new();
    entry.read_to_string(&mut buf)?;
    Ok(buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_css_toxic_styles_cleaned() {
        let dirty_css = r#"
            body {
                font-size: 8px !important;
                color: #000000;
                background-color: white !important;
                margin: 0 120px;
                line-height: 1.6;
            }
            p {
                font-size: 8pt;
                color: black;
                text-indent: 1.5em;
                font-family: Arial !important;
            }
            h1 {
                font-size: 24px;
                color: #e5c07b;
            }
        "#;

        let (clean, count) = sanitize_css_content(dirty_css);
        assert!(count >= 5);
        assert!(!clean.contains("font-size: 8px"));
        assert!(!clean.contains("margin: 0 120px"));
        assert!(!clean.contains("color: #000000"));
        assert!(!clean.contains("font-family: Arial !important"));
        // Desirable properties are preserved
        assert!(clean.contains("line-height: 1.6"));
        assert!(clean.contains("text-indent: 1.5em"));
        assert!(clean.contains("font-size: 24px"));
    }

    #[test]
    fn test_xhtml_entity_and_void_tag_repair() {
        let dirty_xhtml = r#"<div>
            <p>Tom & Jerry went to AT&T</p>
            <br>
            <hr>
            <p style="font-size: 8px; color: black; line-height: 1.5;">Some text</p>
            <h2>Chapter 1: The Beginning</h2>
        </div>"#;

        let mut counter = 0;
        let (clean, inline_count, xml_count, headings) =
            sanitize_xhtml_content(dirty_xhtml, "ch1.xhtml", &mut counter);

        assert!(xml_count >= 3);
        assert!(inline_count >= 1);
        assert!(clean.contains("Tom &amp; Jerry"));
        assert!(clean.contains("AT&amp;T"));
        assert!(clean.contains("<br />"));
        assert!(clean.contains("<hr />"));
        assert!(!clean.contains("font-size: 8px"));
        assert_eq!(headings.len(), 1);
        assert_eq!(headings[0].title, "Chapter 1: The Beginning");
    }

    #[test]
    fn test_ncx_generation() {
        let headings = vec![
            TocHeading {
                title: "Prologue".to_string(),
                target_href: "text/prologue.xhtml".to_string(),
                anchor_id: Some("ch-0".to_string()),
            },
            TocHeading {
                title: "Chapter 1".to_string(),
                target_href: "text/ch1.xhtml".to_string(),
                anchor_id: Some("ch-1".to_string()),
            },
        ];

        let ncx = build_ncx_content("My Great Book", &headings, "text");
        assert!(ncx.contains("<docTitle><text>My Great Book</text></docTitle>"));
        assert!(ncx.contains("playOrder=\"1\""));
        assert!(ncx.contains("<text>Prologue</text>"));
        assert!(ncx.contains("src=\"prologue.xhtml#ch-0\""));
        assert!(ncx.contains("<text>Chapter 1</text>"));
        assert!(ncx.contains("src=\"ch1.xhtml#ch-1\""));
    }

    #[test]
    fn test_sanitize_messy_epub_fixture() {
        let fixture_path = Path::new("sample_books/messy_book.epub");
        if !fixture_path.exists() {
            return;
        }

        let test_epub = std::env::temp_dir().join(format!("kalam-test-messy-{}.epub", uuid::Uuid::new_v4()));
        fs::copy(fixture_path, &test_epub).expect("copy messy epub fixture");

        let report = sanitize_epub(&test_epub, false).expect("sanitize messy epub");
        assert!(report.modified);
        assert!(report.cleaned_css_rules > 0, "cleaned toxic CSS rules");
        assert!(report.fixed_xml_entities > 0, "fixed XML entities");
        assert!(report.generated_toc, "generated missing TOC");

        // Verify the resulting epub is a valid zip and contains toc.ncx
        let file = fs::File::open(&test_epub).expect("open sanitized epub");
        let mut archive = ZipArchive::new(file).expect("sanitized file is valid zip");
        assert!(archive.by_name("mimetype").is_ok());
        assert!(archive.by_name("OEBPS/toc.ncx").is_ok());

        // Verify style.css no longer has 8px font
        let mut css = String::new();
        archive
            .by_name("OEBPS/style.css")
            .unwrap()
            .read_to_string(&mut css)
            .unwrap();
        assert!(!css.contains("8px"));
        assert!(!css.contains("120px"));

        let _ = fs::remove_file(test_epub);
    }
    // ------------------------------------------------------------------
    // Phase 6 step 5: the bake
    // ------------------------------------------------------------------

    fn write_minimal_epub(path: &Path, chapter: &str) {
        let file = fs::File::create(path).expect("create test epub");
        let mut zip = ZipWriter::new(file);
        let stored = SimpleFileOptions::default().compression_method(CompressionMethod::Stored);
        zip.start_file("mimetype", stored).unwrap();
        zip.write_all(b"application/epub+zip").unwrap();
        let deflated =
            SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        zip.start_file("META-INF/container.xml", deflated).unwrap();
        zip.write_all(
            br#"<?xml version="1.0"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#,
        )
        .unwrap();
        zip.start_file("content.opf", deflated).unwrap();
        zip.write_all(
            br#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0" unique-identifier="uid">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="uid">test-book</dc:identifier>
    <dc:title>Bake Test</dc:title>
  </metadata>
  <manifest><item id="c1" href="chapter1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#,
        )
        .unwrap();
        zip.start_file("chapter1.xhtml", deflated).unwrap();
        zip.write_all(chapter.as_bytes()).unwrap();
        zip.finish().unwrap();
    }

    fn bake_patch(
        id: i64,
        href: &str,
        find: &str,
        replace: &str,
        before: &str,
        after: &str,
    ) -> crate::db::PatchRecord {
        crate::db::PatchRecord {
            id,
            book_id: 1,
            kind: "text".into(),
            href: href.into(),
            chapter_index: 0,
            find_text: find.into(),
            replace_text: replace.into(),
            context_before: before.into(),
            context_after: after.into(),
            source: "typo".into(),
            status: "pending".into(),
            created_at: "2026-10-08T00:00:00Z".into(),
            applied_at: None,
            before_hash: String::new(),
        }
    }

    fn chapter_text(path: &Path) -> String {
        let file = fs::File::open(path).expect("reopen baked epub");
        let mut archive = ZipArchive::new(file).expect("baked file is a valid zip");
        let mut text = String::new();
        archive
            .by_name("chapter1.xhtml")
            .expect("chapter survives the bake")
            .read_to_string(&mut text)
            .unwrap();
        text
    }

    fn test_epub_path(tag: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "kalam-test-bake-{tag}-{}.epub",
            uuid::Uuid::new_v4()
        ))
    }

    #[test]
    fn a_bake_writes_the_fix_and_repacks_a_valid_epub() {
        let path = test_epub_path("apply");
        write_minimal_epub(
            &path,
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><head><title>t</title></head><body><p>He said teh word.</p></body></html>"#,
        );
        let report = bake_epub(
            &path,
            &[bake_patch(1, "chapter1.xhtml", "teh", "the", "said ", " word")],
        )
        .expect("bake");

        assert_eq!(report.applied, vec![1]);
        assert!(report.failed.is_empty());
        assert_eq!(report.entries_patched, 1);
        let text = chapter_text(&path);
        assert!(text.contains("He said the word."), "the fix is in the file: {text}");
        assert!(!text.contains("teh"));

        // The repack keeps the spec shape: mimetype first and stored.
        let file = fs::File::open(&path).unwrap();
        let mut archive = ZipArchive::new(file).unwrap();
        assert_eq!(archive.by_index(0).unwrap().name(), "mimetype");
        let mut mt = String::new();
        archive.by_index(0).unwrap().read_to_string(&mut mt).unwrap();
        assert_eq!(mt, "application/epub+zip");
        assert!(archive.by_name("content.opf").is_ok());

        // And the first bake's backup is the pristine original.
        let orig = crate::epub_metadata::backup_path(&path);
        assert!(orig.is_file(), "the .orig backup exists");
        let backup = chapter_text(&orig);
        assert!(backup.contains("teh"), "the backup holds the pre-bake text");
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&orig);
    }

    #[test]
    fn a_patch_that_no_longer_matches_is_reported_not_written() {
        let path = test_epub_path("stale");
        write_minimal_epub(
            &path,
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>He said teh word.</p></body></html>"#,
        );
        let before = fs::read(&path).unwrap();
        let report = bake_epub(
            &path,
            &[bake_patch(7, "chapter1.xhtml", "xyzzy", "nothing", "", "")],
        )
        .expect("a stale patch is a report, not an error");

        assert!(report.applied.is_empty());
        assert_eq!(report.failed.len(), 1);
        assert_eq!(report.failed[0].0, 7);
        assert_eq!(report.entries_patched, 0);
        // Nothing matched, so nothing was rewritten — byte for byte.
        assert_eq!(fs::read(&path).unwrap(), before);
        assert!(
            !crate::epub_metadata::backup_path(&path).exists(),
            "no backup for a no-op bake"
        );
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn a_bake_that_would_break_the_chapter_is_refused() {
        let path = test_epub_path("broken");
        write_minimal_epub(
            &path,
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>He said teh word.</p></body></html>"#,
        );
        let before = fs::read(&path).unwrap();
        // The splice is a raw source edit: an unescaped `<` in the
        // replacement makes the entry unparseable, and the gate refuses
        // the whole bake.
        let err = bake_epub(
            &path,
            &[bake_patch(1, "chapter1.xhtml", "teh", "<b", "said ", " word")],
        )
        .expect_err("the parse gate refuses");
        assert!(err.to_string().contains("unparseable"));
        assert_eq!(fs::read(&path).unwrap(), before, "the file is untouched");
        assert!(!crate::epub_metadata::backup_path(&path).exists());
        let _ = fs::remove_file(&path);
    }

    #[test]
    fn a_bake_composes_patches_in_creation_order() {
        let path = test_epub_path("compose");
        write_minimal_epub(
            &path,
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>He said teh word.</p></body></html>"#,
        );
        // The second patch's text only exists once the first has applied
        // — the same composition the virtual edits guarantee.
        let report = bake_epub(
            &path,
            &[
                bake_patch(1, "chapter1.xhtml", "teh", "the", "said ", " word"),
                bake_patch(2, "chapter1.xhtml", "He said the word.", "She said the word.", "", ""),
            ],
        )
        .expect("bake");
        assert_eq!(report.applied, vec![1, 2]);
        assert_eq!(report.entries_patched, 1);
        let text = chapter_text(&path);
        assert!(text.contains("She said the word."), "both fixes landed: {text}");
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(crate::epub_metadata::backup_path(&path));
    }

    #[test]
    fn the_second_bake_keeps_the_first_bakes_pristine_backup() {
        let path = test_epub_path("twice");
        write_minimal_epub(
            &path,
            r#"<html xmlns="http://www.w3.org/1999/xhtml"><body><p>He said teh word.</p></body></html>"#,
        );
        bake_epub(
            &path,
            &[bake_patch(1, "chapter1.xhtml", "teh", "the", "said ", " word")],
        )
        .expect("first bake");
        let orig = crate::epub_metadata::backup_path(&path);
        let first_backup = fs::read(&orig).unwrap();
        bake_epub(
            &path,
            &[bake_patch(2, "chapter1.xhtml", "the word.", "the sentence.", "said ", "</p>")],
        )
        .expect("second bake");
        // The backup still holds the pre-any-bake original — the undo
        // story unwinds to the pristine file, not to an intermediate.
        assert_eq!(fs::read(&orig).unwrap(), first_backup);
        let text = chapter_text(&path);
        assert!(
            text.contains("He said the sentence."),
            "the second bake composed onto the first: {text}"
        );
        let _ = fs::remove_file(&path);
        let _ = fs::remove_file(&orig);
    }

}
