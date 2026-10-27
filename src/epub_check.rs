//! Check book (decision 132): the pure checks behind the editor's
//! Problems tab. Everything here works on decomposed text — the
//! editor collects each file's *current* text (open buffers win over
//! the bytes on disk), hands it over, and gets problems back. No IO,
//! so the whole pass runs on a task worker and is unit-testable
//! without a zip in sight.
//!
//! Severity follows Calibre's instincts, Kalam's wording: a book a
//! reading system may refuse or mangle is an **error**; a book that
//! is merely untidy is a **warning**. A fix is offered only where the
//! change is mechanical and safe (decision 132); everything else is
//! Go to. Unreferenced files offer Remove.

use crate::epub_container::{resolve_href, xml_problem, FileKind};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Severity {
    Error,
    Warning,
}

/// A mechanical, previewable edit (decision 132's Fix). Byte range in
/// the file's text as checked; the editor verifies the original text
/// is still there before applying, so a fix can never land on text
/// the author has since rewritten.
#[derive(Debug, Clone)]
pub struct ProblemFix {
    pub start: usize,
    pub end: usize,
    pub original: String,
    pub replacement: String,
    /// The full line before and after, for the fix preview.
    pub before_line: String,
    pub after_line: String,
}

#[derive(Debug, Clone)]
pub struct BookProblem {
    pub severity: Severity,
    pub path: String,
    /// 1-based. 1 when the problem is the file itself.
    pub line: usize,
    pub message: String,
    pub fix: Option<ProblemFix>,
    /// The file is used by nothing; the row's button is Remove.
    pub removable: bool,
}

/// One file as the checker sees it.
#[derive(Debug, Clone)]
pub struct CheckFile {
    pub path: String,
    pub kind: FileKind,
    pub spine_index: Option<usize>,
    pub in_manifest: bool,
    pub is_nav: bool,
    pub is_cover_image: bool,
    /// Current text for Html/Css/Xml files; `None` for binaries.
    pub text: Option<String>,
}

/// Void elements that XHTML requires to be written self-closed.
const VOID_TAGS: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source",
    "track", "wbr",
];

/// Run every check over the book. Sorted errors-first, then by file
/// and line — the order the Problems tab shows inside each group.
pub fn check_book(
    files: &[CheckFile],
    opf_path: &str,
    ncx_path: Option<&str>,
) -> Vec<BookProblem> {
    let mut problems = Vec::new();
    let file_set: HashSet<&str> = files.iter().map(|f| f.path.as_str()).collect();

    // Pass 1: ids per HTML file (fragment targets + duplicates).
    let mut ids_by_file: HashMap<String, Vec<(String, usize)>> = HashMap::new();
    for file in files {
        if file.kind != FileKind::Html {
            continue;
        }
        if let Some(text) = &file.text {
            ids_by_file.insert(file.path.clone(), find_ids(text));
        }
    }

    // Pass 2: per-file checks; collect every referenced target.
    let mut referenced: HashSet<String> = HashSet::new();
    for file in files {
        if file.spine_index.is_some() || file.is_nav || file.is_cover_image {
            referenced.insert(file.path.clone());
        }
    }
    referenced.insert(opf_path.to_string());
    referenced.insert("META-INF/container.xml".to_string());
    if let Some(ncx) = ncx_path {
        referenced.insert(ncx.to_string());
    }

    for file in files {
        let text = match &file.text {
            Some(text) => text,
            None => continue,
        };
        match file.kind {
            FileKind::Html | FileKind::Xml => {
                if let Some((line, message)) = xml_problem(text) {
                    problems.push(problem(Severity::Error, file, line, message));
                }
            }
            _ => {}
        }
        if file.kind == FileKind::Html {
            check_html(file, text, &file_set, &ids_by_file, &mut referenced, &mut problems);
        } else if file.kind == FileKind::Css {
            check_css(file, text, &file_set, &mut referenced, &mut problems);
        } else if file.path == opf_path {
            // Guide/landmark references count as uses; manifest items
            // are declarations, not uses, so skip those hrefs.
            for r in find_refs(text, /* skip_manifest_items */ true) {
                if let Some(target) = resolve_ref_target(&file.path, &r.href) {
                    referenced.insert(target);
                }
            }
        }
        if matches!(file.kind, FileKind::Html | FileKind::Css | FileKind::Xml)
            && text.trim().is_empty()
        {
            problems.push(problem(
                Severity::Warning,
                file,
                1,
                "File is empty".to_string(),
            ));
        }
    }

    // Pass 3: whole-book checks.
    let mut lower_seen: HashMap<String, String> = HashMap::new();
    for file in files {
        if file.path == "mimetype" {
            continue;
        }
        if !file.in_manifest && file.path != "META-INF/container.xml" {
            problems.push(problem(
                Severity::Warning,
                file,
                1,
                "Not listed in the manifest".to_string(),
            ));
        }
        if !referenced.contains(&file.path) {
            let mut p = problem(
                Severity::Warning,
                file,
                1,
                "Not used anywhere in the book".to_string(),
            );
            p.removable = true;
            problems.push(p);
        }
        let lower = file.path.to_ascii_lowercase();
        if let Some(other) = lower_seen.get(&lower) {
            problems.push(problem(
                Severity::Warning,
                file,
                1,
                format!("Same name as {other} except for case"),
            ));
        } else {
            lower_seen.insert(lower, file.path.clone());
        }
    }

    problems.sort_by(|a, b| {
        let rank = |s: Severity| match s {
            Severity::Error => 0,
            Severity::Warning => 1,
        };
        rank(a.severity)
            .cmp(&rank(b.severity))
            .then(a.path.cmp(&b.path))
            .then(a.line.cmp(&b.line))
    });
    problems
}

fn problem(severity: Severity, file: &CheckFile, line: usize, message: String) -> BookProblem {
    BookProblem {
        severity,
        path: file.path.clone(),
        line,
        message,
        fix: None,
        removable: false,
    }
}

fn check_html(
    file: &CheckFile,
    text: &str,
    file_set: &HashSet<&str>,
    ids_by_file: &HashMap<String, Vec<(String, usize)>>,
    referenced: &mut HashSet<String>,
    problems: &mut Vec<BookProblem>,
) {
    // Links and embedded resources.
    for r in find_refs(text, false) {
        let (target_path, fragment) = split_fragment(&r.href);
        if is_external(&r.href) {
            continue;
        }
        let target = if target_path.is_empty() {
            file.path.clone()
        } else {
            match resolve_ref_target(&file.path, target_path) {
                Some(t) => t,
                None => continue,
            }
        };
        referenced.insert(target.clone());
        if !file_set.contains(target.as_str()) {
            problems.push(problem(
                Severity::Error,
                file,
                r.line,
                format!("Link to missing file {}", file_name(&target)),
            ));
            continue;
        }
        if let Some(frag) = fragment {
            if !frag.is_empty() {
                let decoded = crate::epub::percent_decode(frag);
                let exists = ids_by_file
                    .get(&target)
                    .map(|ids| ids.iter().any(|(id, _)| *id == decoded))
                    .unwrap_or(false);
                if !exists {
                    problems.push(problem(
                        Severity::Error,
                        file,
                        r.line,
                        format!("Link to missing anchor #{decoded}"),
                    ));
                }
            }
        }
    }

    // Duplicate ids.
    if let Some(ids) = ids_by_file.get(&file.path) {
        let mut seen: HashSet<&str> = HashSet::new();
        for (id, line) in ids {
            if !seen.insert(id.as_str()) {
                problems.push(problem(
                    Severity::Warning,
                    file,
                    *line,
                    format!("Duplicate id \"{id}\""),
                ));
            }
        }
    }

    // Tags: images without alt, void elements not self-closed.
    for tag in scan_tags(text) {
        if VOID_TAGS.contains(&tag.name.as_str()) && !tag.self_closing {
            let original = &text[tag.start..tag.end];
            let replacement = format!("{}/>", &original[..original.len() - 1]);
            let mut p = problem(
                Severity::Error,
                file,
                tag.line,
                format!("<{}> should be written <{}/>", tag.name, tag.name),
            );
            p.fix = Some(make_fix(text, tag.start, tag.end, replacement));
            problems.push(p);
        }
        if tag.name == "img" && !tag.has_alt {
            // One problem per image: if the tag is also not
            // self-closed, the void fix above already covers the
            // tag; the alt fix then inserts before its `>`.
            let insert_at = if tag.self_closing { tag.end - 2 } else { tag.end - 1 };
            let mut p = problem(
                Severity::Warning,
                file,
                tag.line,
                "Image has no alt text".to_string(),
            );
            p.fix = Some(make_fix(text, insert_at, insert_at, " alt=\"\"".to_string()));
            problems.push(p);
        }
    }

    // Bare ampersands in text (the most common XHTML breakage).
    for amp in find_bare_ampersands(text) {
        let mut p = problem(
            Severity::Error,
            file,
            line_of(text, amp),
            "Unescaped & — write &amp;".to_string(),
        );
        p.fix = Some(make_fix(text, amp, amp + 1, "&amp;".to_string()));
        problems.push(p);
    }

    // A title in the head.
    let lower = text.to_ascii_lowercase();
    if !lower.contains("<title") {
        let line = lower.find("<head").map(|b| line_of(text, b)).unwrap_or(1);
        problems.push(problem(
            Severity::Warning,
            file,
            line,
            "Page has no title".to_string(),
        ));
    }

    // Reading order.
    if file.spine_index.is_none() && !file.is_nav {
        problems.push(problem(
            Severity::Warning,
            file,
            1,
            "Not in the reading order".to_string(),
        ));
    }
}

fn check_css(
    file: &CheckFile,
    text: &str,
    file_set: &HashSet<&str>,
    referenced: &mut HashSet<String>,
    problems: &mut Vec<BookProblem>,
) {
    for r in find_css_urls(text) {
        if is_external(&r.href) || r.href.starts_with('#') {
            continue;
        }
        let (target_path, _) = split_fragment(&r.href);
        let Some(target) = resolve_ref_target(&file.path, target_path) else {
            continue;
        };
        referenced.insert(target.clone());
        if !file_set.contains(target.as_str()) {
            problems.push(problem(
                Severity::Error,
                file,
                r.line,
                format!("Link to missing file {}", file_name(&target)),
            ));
        }
    }
}

// ── scanning ───────────────────────────────────────────────────────

pub(crate) struct Ref {
    pub href: String,
    pub line: usize,
}

/// `href=`/`src=` attribute values with their lines. A substring
/// scan, not a parse: attribute-shaped text inside prose can false-
/// positive in theory, but quoted `href="…"` in body text is rarer
/// than the broken books this exists to catch.
pub(crate) fn find_refs(text: &str, skip_manifest_items: bool) -> Vec<Ref> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Word-boundary-ish start of an attribute name.
        if (matches_at(text, i, "href") || matches_at(text, i, "src"))
            && (i == 0 || matches!(bytes[i - 1], b' ' | b'\t' | b'\n' | b'\r' | b':'))
        {
            let name_len = if matches_at(text, i, "href") { 4 } else { 3 };
            let mut j = i + name_len;
            while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'=' {
                j += 1;
                while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
                    j += 1;
                }
                if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                    let quote = bytes[j];
                    let start = j + 1;
                    let mut end = start;
                    while end < bytes.len() && bytes[end] != quote {
                        end += 1;
                    }
                    let href = &text[start..end];
                    let line_start = text[..i].rfind('\n').map(|n| n + 1).unwrap_or(0);
                    let line_text = &text[line_start..i];
                    if !(skip_manifest_items && line_text.contains("<item")) && !href.is_empty() {
                        out.push(Ref {
                            href: href.to_string(),
                            line: line_of(text, i),
                        });
                    }
                    i = end + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// `url(…)` targets in a stylesheet.
fn find_css_urls(text: &str) -> Vec<Ref> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i + 4 < bytes.len() {
        if matches_at(text, i, "url(") {
            let mut j = i + 4;
            while j < bytes.len() && matches!(bytes[j], b' ' | b'\t' | b'\n' | b'\r') {
                j += 1;
            }
            let (start, closer) = if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                (j + 1, bytes[j])
            } else {
                (j, b')')
            };
            let mut end = start;
            while end < bytes.len() && bytes[end] != closer {
                end += 1;
            }
            let href = text[start..end].trim();
            if !href.is_empty() {
                out.push(Ref {
                    href: href.to_string(),
                    line: line_of(text, i),
                });
            }
            i = end + 1;
            continue;
        }
        i += 1;
    }
    out
}

fn find_ids(text: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i + 3 < bytes.len() {
        if matches_at(text, i, "id")
            && (i == 0 || matches!(bytes[i - 1], b' ' | b'\t' | b'\n' | b'\r'))
        {
            let mut j = i + 2;
            while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
                j += 1;
            }
            if j < bytes.len() && bytes[j] == b'=' {
                j += 1;
                while j < bytes.len() && matches!(bytes[j], b' ' | b'\t') {
                    j += 1;
                }
                if j < bytes.len() && (bytes[j] == b'"' || bytes[j] == b'\'') {
                    let quote = bytes[j];
                    let start = j + 1;
                    let mut end = start;
                    while end < bytes.len() && bytes[end] != quote {
                        end += 1;
                    }
                    out.push((text[start..end].to_string(), line_of(text, i)));
                    i = end + 1;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

struct TagHit {
    name: String,
    start: usize,
    /// One past the closing `>`.
    end: usize,
    line: usize,
    self_closing: bool,
    has_alt: bool,
}

/// Opening tags, skipping comments, declarations and closing tags.
fn scan_tags(text: &str) -> Vec<TagHit> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if matches_at(text, i, "<!--") {
            i = find_after(text, i + 4, "-->");
            continue;
        }
        if i + 1 >= bytes.len() {
            break;
        }
        let next = bytes[i + 1];
        if next == b'/' || next == b'!' || next == b'?' {
            i = find_after(text, i + 2, ">");
            continue;
        }
        if !next.is_ascii_alphabetic() {
            i += 1;
            continue;
        }
        let name_start = i + 1;
        let mut j = name_start;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || bytes[j] == b'-') {
            j += 1;
        }
        let name = text[name_start..j].to_ascii_lowercase();
        // To the closing >, respecting quoted attribute values.
        let mut k = j;
        let mut quote = 0u8;
        while k < bytes.len() {
            let b = bytes[k];
            if quote != 0 {
                if b == quote {
                    quote = 0;
                }
            } else if b == b'"' || b == b'\'' {
                quote = b;
            } else if b == b'>' {
                break;
            }
            k += 1;
        }
        if k >= bytes.len() {
            break;
        }
        let inner = &text[j..k];
        out.push(TagHit {
            name,
            start: i,
            end: k + 1,
            line: line_of(text, i),
            self_closing: inner.trim_end().ends_with('/'),
            has_alt: contains_attr(inner, "alt"),
        });
        i = k + 1;
    }
    out
}

fn contains_attr(tag_inner: &str, attr: &str) -> bool {
    let lower = tag_inner.to_ascii_lowercase();
    let needle = format!(" {attr}=");
    let needle2 = format!(" {attr} ");
    lower.contains(&needle) || lower.contains(&needle2) || lower.trim_end().ends_with(&format!(" {attr}"))
}

/// `&` in character data that does not begin an entity-shaped
/// reference (`&name;`, `&#123;`, `&#x1F;`).
fn find_bare_ampersands(text: &str) -> Vec<usize> {
    let mut out = Vec::new();
    let bytes = text.as_bytes();
    let mut i = 0usize;
    let mut in_tag = false;
    while i < bytes.len() {
        match bytes[i] {
            b'<' => {
                if matches_at(text, i, "<!--") {
                    i = find_after(text, i + 4, "-->");
                    continue;
                }
                if matches_at(text, i, "<![CDATA[") {
                    i = find_after(text, i + 9, "]]>");
                    continue;
                }
                in_tag = true;
            }
            b'>' => in_tag = false,
            b'&' if !in_tag && !is_entity_ref(&text[i + 1..]) => out.push(i),
            _ => {}
        }
        i += 1;
    }
    out
}

fn is_entity_ref(after: &str) -> bool {
    let bytes = after.as_bytes();
    if bytes.is_empty() {
        return false;
    }
    let (mut i, ok_start) = if bytes[0] == b'#' {
        if after.len() > 1 && (bytes[1] == b'x' || bytes[1] == b'X') {
            (2, true)
        } else {
            (1, true)
        }
    } else if bytes[0].is_ascii_alphabetic() {
        (0, true)
    } else {
        return false;
    };
    if !ok_start {
        return false;
    }
    let hex = after.starts_with("#x") || after.starts_with("#X");
    let start = i;
    while i < bytes.len() {
        let b = bytes[i];
        let valid = if hex {
            b.is_ascii_hexdigit()
        } else if bytes[0] == b'#' {
            b.is_ascii_digit()
        } else {
            b.is_ascii_alphanumeric()
        };
        if !valid {
            break;
        }
        i += 1;
    }
    i > start && i < bytes.len() && bytes[i] == b';' && (hex || bytes[0] == b'#' || i >= 2)
}

// ── helpers ────────────────────────────────────────────────────────

fn make_fix(text: &str, start: usize, end: usize, replacement: String) -> ProblemFix {
    let line_start = text[..start].rfind('\n').map(|n| n + 1).unwrap_or(0);
    let line_end = text[end..]
        .find('\n')
        .map(|n| end + n)
        .unwrap_or(text.len());
    let before_line = text[line_start..line_end].to_string();
    let mut after_line = before_line.clone();
    after_line.replace_range(start - line_start..end - line_start, &replacement);
    ProblemFix {
        start,
        end,
        original: text[start..end].to_string(),
        replacement,
        before_line,
        after_line,
    }
}

fn is_external(href: &str) -> bool {
    let lower = href.to_ascii_lowercase();
    lower.starts_with("http:")
        || lower.starts_with("https:")
        || lower.starts_with("mailto:")
        || lower.starts_with("tel:")
        || lower.starts_with("data:")
        || lower.starts_with("ftp:")
        || href.starts_with("//")
}

fn split_fragment(href: &str) -> (&str, Option<&str>) {
    match href.find('#') {
        Some(at) => (&href[..at], Some(&href[at + 1..])),
        None => (href, None),
    }
}

/// Resolve a link from `from_path` to a zip entry name.
fn resolve_ref_target(from_path: &str, href_path: &str) -> Option<String> {
    if href_path.is_empty() {
        return None;
    }
    let dir = match from_path.rfind('/') {
        Some(slash) => &from_path[..slash + 1],
        None => "",
    };
    Some(resolve_href(dir, href_path))
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn line_of(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].bytes().filter(|b| *b == b'\n').count() + 1
}

fn matches_at(text: &str, at: usize, needle: &str) -> bool {
    text.len() >= at + needle.len() && &text[at..at + needle.len()] == needle
}

fn find_after(text: &str, from: usize, needle: &str) -> usize {
    text[from.min(text.len())..]
        .find(needle)
        .map(|n| from + n + needle.len())
        .unwrap_or(text.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn html(path: &str, text: &str) -> CheckFile {
        CheckFile {
            path: path.to_string(),
            kind: FileKind::Html,
            spine_index: Some(0),
            in_manifest: true,
            is_nav: false,
            is_cover_image: false,
            text: Some(text.to_string()),
        }
    }

    fn messages(problems: &[BookProblem]) -> Vec<&str> {
        problems.iter().map(|p| p.message.as_str()).collect()
    }

    #[test]
    fn broken_links_and_fragments_are_errors() {
        let files = vec![
            html(
                "OEBPS/ch1.xhtml",
                "<html><head><title>T</title></head><body>\
                 <a href=\"gone.xhtml\">x</a><a href=\"ch1.xhtml#nope\">y</a>\
                 <a href=\"#here\">z</a><p id=\"here\">t</p></body></html>",
            ),
            CheckFile {
                path: "OEBPS/content.opf".to_string(),
                kind: FileKind::Xml,
                spine_index: None,
                in_manifest: false,
                is_nav: false,
                is_cover_image: false,
                text: Some("<package/>".to_string()),
            },
        ];
        let problems = check_book(&files, "OEBPS/content.opf", None);
        let msgs = messages(&problems);
        assert!(msgs.iter().any(|m| m.contains("missing file gone.xhtml")), "{msgs:?}");
        assert!(msgs.iter().any(|m| m.contains("missing anchor #nope")), "{msgs:?}");
        assert!(problems.iter().all(|p| !p.message.contains("#here")));
    }

    #[test]
    fn void_tags_and_bare_ampersands_offer_fixes() {
        let files = vec![html(
            "ch1.xhtml",
            "<html><head><title>T</title></head><body><p>Salt & vinegar<br>chips</p></body></html>",
        )];
        let problems = check_book(&files, "content.opf", None);
        let fixable: Vec<&BookProblem> = problems.iter().filter(|p| p.fix.is_some()).collect();
        assert_eq!(fixable.len(), 2, "{problems:?}");
        let br = fixable
            .iter()
            .find(|p| p.message.contains("<br>"))
            .expect("br fix");
        let fix = br.fix.as_ref().expect("fix");
        assert_eq!(fix.original, "<br>");
        assert_eq!(fix.replacement, "<br/>");
        assert!(fix.after_line.contains("vinegar<br/>chips"));
    }

    #[test]
    fn alt_missing_and_unreferenced_and_manifest_warnings() {
        let mut orphan = html("OEBPS/orphan.xhtml", "<html><head><title>O</title></head><body/></html>");
        orphan.spine_index = None;
        let files = vec![
            html(
                "OEBPS/ch1.xhtml",
                "<html><head><title>T</title></head><body><img src=\"i.jpg\"/></body></html>",
            ),
            CheckFile {
                path: "OEBPS/i.jpg".to_string(),
                kind: FileKind::Image,
                spine_index: None,
                in_manifest: true,
                is_nav: false,
                is_cover_image: false,
                text: None,
            },
            orphan,
            CheckFile {
                path: "OEBPS/stray.css".to_string(),
                kind: FileKind::Css,
                spine_index: None,
                in_manifest: false,
                is_nav: false,
                is_cover_image: false,
                text: Some("p { color: red; }".to_string()),
            },
        ];
        let problems = check_book(&files, "OEBPS/content.opf", None);
        let msgs = messages(&problems);
        assert!(msgs.contains(&"Image has no alt text"), "{msgs:?}");
        assert!(msgs.contains(&"Not in the reading order"), "{msgs:?}");
        assert!(msgs.contains(&"Not listed in the manifest"), "{msgs:?}");
        let removable: Vec<&str> = problems
            .iter()
            .filter(|p| p.removable)
            .map(|p| p.path.as_str())
            .collect();
        assert!(removable.contains(&"OEBPS/orphan.xhtml"), "{removable:?}");
        assert!(removable.contains(&"OEBPS/stray.css"), "{removable:?}");
        assert!(!removable.contains(&"OEBPS/i.jpg"), "referenced image must stay");
    }

    #[test]
    fn entities_are_not_bare_ampersands() {
        let files = vec![html(
            "ch1.xhtml",
            "<html><head><title>T</title></head><body><p>fish &amp; chips &nbsp; &#8212; done</p></body></html>",
        )];
        let problems = check_book(&files, "content.opf", None);
        assert!(
            problems.iter().all(|p| !p.message.contains("Unescaped")),
            "{problems:?}"
        );
    }
}
