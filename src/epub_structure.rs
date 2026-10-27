//! The deep editor's structural toolbox (E4): tables of contents,
//! document splitting and merging, link retargeting, and the OPF
//! manifest/spine surgery those need. Pure text work, like
//! `epub_check` and `epub_tools` — the editor page orchestrates,
//! snapshots and saves; nothing here touches disk or GTK.
//!
//! Hrefs inside a book are always handled in two forms: the
//! *container path* (`OEBPS/text/chapter01.xhtml`, what
//! `EpubContainer` keys on) and the *relative href* written into a
//! document (`../text/chapter01.xhtml#start`). `rel_href` and the
//! container's `resolve_href` convert between them.

// Several tools below (split/merge, semantics, spell check)
// gain their editor UI in later E4 commits; until then only
// this module's tests exercise them. Drop this when the last
// E4 surface lands.
#![allow(dead_code)]

use crate::epub_container::resolve_href;

// ── shared helpers ──────────────────────────────────────────────────

/// The folder portion of a container path, keeping the slash:
/// `OEBPS/text/a.xhtml` → `OEBPS/text/`, `a.xhtml` → `""`.
pub fn parent_dir(path: &str) -> String {
    match path.rfind('/') {
        Some(idx) => path[..=idx].to_string(),
        None => String::new(),
    }
}

/// Percent-encode what an EPUB href cannot carry bare. Keeps the
/// RFC 3986 unreserved set plus `/`; everything else becomes UTF-8
/// percent-escapes (space links are the common case in books).
fn encode_href(path: &str) -> String {
    let mut out = String::with_capacity(path.len());
    for b in path.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'.' | b'_' | b'~' | b'/' => {
                out.push(b as char)
            }
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Relative href from a document living in `from_dir` to
/// `to_path` (both container paths), with an optional fragment.
pub fn rel_href(from_dir: &str, to_path: &str, fragment: Option<&str>) -> String {
    let from: Vec<&str> = from_dir.split('/').filter(|p| !p.is_empty()).collect();
    let to: Vec<&str> = to_path.split('/').collect();
    let mut common = 0;
    while common < from.len()
        && common < to.len().saturating_sub(1)
        && from[common] == to[common]
    {
        common += 1;
    }
    let mut rel = String::new();
    for _ in common..from.len() {
        rel.push_str("../");
    }
    rel.push_str(&to[common..].join("/"));
    let mut out = encode_href(&rel);
    if let Some(frag) = fragment {
        if !frag.is_empty() {
            out.push('#');
            out.push_str(frag);
        }
    }
    out
}

/// Escape text for XML content, and (with `attr`) for a
/// double-quoted attribute.
fn esc(text: &str, attr: bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' if attr => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Byte spans of an element: `(start_tag_start, inner_start,
/// inner_end, end_tag_end)`, found by local name with quick-xml.
/// `accept` inspects the start tag's attributes (raw key/value
/// pairs, keys as written, e.g. `epub:type`). Nested elements with
/// the same name are counted so the *matching* end tag is found.
pub fn element_span(
    text: &str,
    local: &str,
    accept: &dyn Fn(&[(String, String)]) -> bool,
) -> Option<(usize, usize, usize, usize)> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(text);
    let mut depth = 0usize;
    let mut open: Option<(usize, usize)> = None; // (tag start, inner start)
    loop {
        let pos_before = reader.buffer_position() as usize;
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                if open.is_none() && name == local {
                    let attrs: Vec<(String, String)> = e
                        .attributes()
                        .filter_map(|a| a.ok())
                        .map(|a| {
                            (
                                String::from_utf8_lossy(a.key.as_ref()).into_owned(),
                                String::from_utf8_lossy(&a.value).into_owned(),
                            )
                        })
                        .collect();
                    if accept(&attrs) {
                        open = Some((pos_before, reader.buffer_position() as usize));
                        depth = 1;
                    }
                } else if open.is_some() && name == local {
                    depth += 1;
                }
            }
            Ok(Event::End(e)) => {
                if open.is_some() {
                    let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                    if name == local {
                        depth = depth.saturating_sub(1);
                        if depth == 0 {
                            let (tag_start, inner_start) = open?;
                            return Some((
                                tag_start,
                                inner_start,
                                pos_before,
                                reader.buffer_position() as usize,
                            ));
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => return None,
            _ => {}
        }
    }
}

/// Every `id="…"` value in a document, in order. Split uses this
/// to learn which piece each anchor landed in.
pub fn ids_in(text: &str) -> Vec<String> {
    let Ok(re) = regex::Regex::new(r#"(?i)\bid\s*=\s*(?:"([^"]*)"|'([^']*)')"#) else {
        return Vec::new();
    };
    re.captures_iter(text)
        .filter_map(|c| c.get(1).or_else(|| c.get(2)))
        .map(|m| m.as_str().to_string())
        .collect()
}

// ── headings ────────────────────────────────────────────────────────

/// One heading: level (1–6), visible text, char offset of its
/// start tag, and its `id` when it has one (its own, or the first
/// descendant anchor's — books love `<h2><a id="ch3"/>…</h2>`).
pub fn headings_with_ids(html: &str) -> Vec<(usize, String, usize, Option<String>)> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(html);
    let mut out = Vec::new();
    let mut current: Option<(usize, String, usize, Option<String>)> = None;
    loop {
        let pos_before = reader.buffer_position() as usize;
        match reader.read_event() {
            Ok(Event::Start(e)) | Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                let id = e
                    .attributes()
                    .filter_map(|a| a.ok())
                    .find(|a| a.key.as_ref() == b"id")
                    .map(|a| String::from_utf8_lossy(&a.value).into_owned());
                if (name.len() == 2
                    && name.starts_with('h')
                    && name[1..].chars().all(|c| ('1'..='6').contains(&c)))
                    && current.is_none()
                {
                    let level = name[1..].parse::<usize>().unwrap_or(1);
                    let char_off = html[..pos_before.min(html.len())].chars().count();
                    current = Some((level, String::new(), char_off, id));
                } else if let Some((_, _, _, ref mut hid)) = current {
                    if hid.is_none() && id.is_some() {
                        *hid = id;
                    }
                }
            }
            Ok(Event::Text(t)) => {
                if let Some((_, ref mut text, _, _)) = current {
                    if let Ok(s) = t.unescape() {
                        text.push_str(&s);
                    }
                }
            }
            Ok(Event::End(e)) => {
                let name = String::from_utf8_lossy(e.local_name().as_ref()).into_owned();
                if current.is_some()
                    && name.len() == 2
                    && name.starts_with('h')
                    && name[1..].chars().all(|c| ('1'..='6').contains(&c))
                {
                    if let Some((level, text, off, id)) = current.take() {
                        let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                        if !text.is_empty() {
                            out.push((level, text, off, id));
                        }
                    }
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    out
}

// ── table of contents (decision 136) ───────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TocEntry {
    pub text: String,
    /// Container path of the target file ("" when the entry had no
    /// usable link — generation always produces one).
    pub href: String,
    pub fragment: Option<String>,
    pub children: Vec<TocEntry>,
}

impl TocEntry {
    pub fn leaf(text: String, href: String, fragment: Option<String>) -> Self {
        TocEntry {
            text,
            href,
            fragment,
            children: Vec::new(),
        }
    }

    pub fn count(&self) -> usize {
        1 + self.children.iter().map(TocEntry::count).sum::<usize>()
    }
}

/// Total entries in a forest.
pub fn toc_count(entries: &[TocEntry]) -> usize {
    entries.iter().map(TocEntry::count).sum()
}

/// Flatten a forest into (depth, leaf) rows in reading order —
/// the form every Edit-contents gesture works on (decision
/// 136). Children ride in the rows below, so each emitted
/// entry is childless.
pub fn toc_flatten(entries: Vec<TocEntry>) -> Vec<(usize, TocEntry)> {
    fn walk(entries: Vec<TocEntry>, depth: usize, out: &mut Vec<(usize, TocEntry)>) {
        for mut entry in entries {
            let children = std::mem::take(&mut entry.children);
            out.push((depth, entry));
            walk(children, depth + 1, out);
        }
    }
    let mut out = Vec::new();
    walk(entries, 0, &mut out);
    out
}

/// Rebuild a forest from flattened rows. Depths are clamped —
/// the first row is a root and no row sits more than one
/// deeper than the row above — so a hand-broken depth jump
/// nests, never drops.
pub fn toc_unflatten(flat: Vec<(usize, TocEntry)>) -> Vec<TocEntry> {
    fn build(rows: &[(usize, TocEntry)], pos: &mut usize, depth: usize) -> Vec<TocEntry> {
        let mut out = Vec::new();
        while *pos < rows.len() && rows[*pos].0 == depth {
            let mut entry = rows[*pos].1.clone();
            entry.children = Vec::new();
            *pos += 1;
            if *pos < rows.len() && rows[*pos].0 > depth {
                entry.children = build(rows, pos, depth + 1);
            }
            out.push(entry);
        }
        out
    }
    let mut rows: Vec<(usize, TocEntry)> = Vec::with_capacity(flat.len());
    for (depth, entry) in flat {
        let clamped = match rows.last() {
            None => 0,
            Some((prev, _)) => depth.min(prev + 1),
        };
        rows.push((clamped, entry));
    }
    let mut pos = 0;
    build(&rows, &mut pos, 0)
}

/// Nesting depth of a forest (1 for a flat list, 0 for none).
pub fn toc_depth(entries: &[TocEntry]) -> usize {
    entries
        .iter()
        .map(|e| 1 + toc_depth(&e.children))
        .max()
        .unwrap_or(0)
}

fn split_href(href: &str) -> (&str, Option<String>) {
    match href.split_once('#') {
        Some((path, frag)) if !frag.is_empty() => (path, Some(frag.to_string())),
        Some((path, _)) => (path, None),
        None => (href, None),
    }
}

/// Parse an NCX's navMap into entries (hrefs resolved against the
/// NCX's own folder into container paths).
pub fn parse_ncx(text: &str, ncx_path: &str) -> Vec<TocEntry> {
    let dir = parent_dir(ncx_path);
    let Ok(doc) = roxmltree::Document::parse(text) else {
        return Vec::new();
    };
    fn walk(node: roxmltree::Node, dir: &str) -> Vec<TocEntry> {
        node.children()
            .filter(|n| n.is_element() && n.tag_name().name() == "navPoint")
            .map(|point| {
                let label = point
                    .children()
                    .find(|n| n.tag_name().name() == "navLabel")
                    .and_then(|l| {
                        l.children()
                            .find(|n| n.tag_name().name() == "text")
                            .and_then(|t| t.text())
                    })
                    .unwrap_or("")
                    .split_whitespace()
                    .collect::<Vec<_>>()
                    .join(" ");
                let src = point
                    .children()
                    .find(|n| n.tag_name().name() == "content")
                    .and_then(|c| c.attribute("src"))
                    .unwrap_or("");
                let (path, frag) = split_href(src);
                let href = if path.is_empty() {
                    String::new()
                } else {
                    resolve_href(dir, path)
                };
                TocEntry {
                    text: label,
                    href,
                    fragment: frag,
                    children: walk(point, dir),
                }
            })
            .collect()
    }
    match doc.descendants().find(|n| n.tag_name().name() == "navMap") {
        Some(map) => walk(map, &dir),
        None => Vec::new(),
    }
}

/// Parse the EPUB 3 nav document's `toc` list.
pub fn parse_nav(text: &str, nav_path: &str) -> Vec<TocEntry> {
    let dir = parent_dir(nav_path);
    let Ok(doc) = roxmltree::Document::parse(text) else {
        return Vec::new();
    };
    let toc_nav = doc.descendants().find(|n| {
        n.is_element()
            && n.tag_name().name() == "nav"
            && n.attributes().any(|a| {
                a.name() == "type" && a.value().split_whitespace().any(|t| t == "toc")
            })
    });
    let Some(nav) = toc_nav else {
        return Vec::new();
    };
    fn node_text(node: roxmltree::Node) -> String {
        node.descendants()
            .filter_map(|n| n.text())
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
    }
    fn walk_list(list: roxmltree::Node, dir: &str) -> Vec<TocEntry> {
        list.children()
            .filter(|n| n.is_element() && n.tag_name().name() == "li")
            .map(|li| {
                // The entry's own link: an <a> (or bare <span>)
                // direct child, not one inside a nested list.
                let link = li
                    .children()
                    .find(|n| n.is_element() && matches!(n.tag_name().name(), "a" | "span"));
                let (label, raw) = match link {
                    Some(a) => (node_text(a), a.attribute("href").unwrap_or("")),
                    None => (String::new(), ""),
                };
                let (path, frag) = split_href(raw);
                let href = if path.is_empty() {
                    String::new()
                } else {
                    resolve_href(dir, path)
                };
                let children = li
                    .children()
                    .find(|n| n.is_element() && matches!(n.tag_name().name(), "ol" | "ul"))
                    .map(|sub| walk_list(sub, dir))
                    .unwrap_or_default();
                TocEntry {
                    text: label,
                    href,
                    fragment: frag,
                    children,
                }
            })
            .collect()
    }
    match nav
        .descendants()
        .find(|n| n.is_element() && matches!(n.tag_name().name(), "ol" | "ul"))
    {
        Some(list) => walk_list(list, &dir),
        None => Vec::new(),
    }
}

fn render_navpoints(entries: &[TocEntry], dir: &str, order: &mut usize, out: &mut String) {
    for entry in entries {
        *order += 1;
        let n = *order;
        out.push_str(&format!(
            "<navPoint id=\"navPoint-{n}\" playOrder=\"{n}\"><navLabel><text>{}</text></navLabel>",
            esc(&entry.text, false)
        ));
        if !entry.href.is_empty() {
            out.push_str(&format!(
                "<content src=\"{}\"/>",
                esc(&rel_href(dir, &entry.href, entry.fragment.as_deref()), true)
            ));
        }
        render_navpoints(&entry.children, dir, order, out);
        out.push_str("</navPoint>");
    }
}

/// Write entries into an NCX document, replacing the navMap's
/// contents and keeping the head/docTitle exactly as they were.
/// With no existing document, a fresh NCX is generated.
pub fn render_ncx(existing: Option<&str>, entries: &[TocEntry], ncx_path: &str, title: &str) -> String {
    let dir = parent_dir(ncx_path);
    let mut inner = String::new();
    let mut order = 0usize;
    render_navpoints(entries, &dir, &mut order, &mut inner);
    if let Some(text) = existing {
        if let Some((_, inner_start, inner_end, _)) = element_span(text, "navMap", &|_| true) {
            let mut out = String::with_capacity(text.len());
            out.push_str(&text[..inner_start]);
            out.push_str(&inner);
            out.push_str(&text[inner_end..]);
            return update_ncx_depth(&out, toc_depth(entries));
        }
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<ncx xmlns=\"http://www.daisy.org/z3986/2005/ncx/\" version=\"2005-1\"><head><meta name=\"dtb:depth\" content=\"{}\"/></head><docTitle><text>{}</text></docTitle><navMap>{}</navMap></ncx>\n",
        toc_depth(entries).max(1),
        esc(title, false),
        inner
    )
}

fn update_ncx_depth(ncx: &str, depth: usize) -> String {
    let depth = depth.max(1).to_string();
    for pattern in [
        r#"(?i)(<meta[^>]*name\s*=\s*"dtb:depth"[^>]*content\s*=\s*")[0-9]+(")"#,
        r#"(?i)(<meta[^>]*content\s*=\s*")[0-9]+("[^>]*name\s*=\s*"dtb:depth")"#,
    ] {
        if let Ok(re) = regex::Regex::new(pattern) {
            if re.is_match(ncx) {
                return re
                    .replace(ncx, |c: &regex::Captures| format!("{}{}{}", &c[1], depth, &c[2]))
                    .into_owned();
            }
        }
    }
    ncx.to_string()
}

fn render_nav_items(entries: &[TocEntry], dir: &str, out: &mut String) {
    for entry in entries {
        out.push_str("<li>");
        if entry.href.is_empty() {
            out.push_str(&format!("<span>{}</span>", esc(&entry.text, false)));
        } else {
            out.push_str(&format!(
                "<a href=\"{}\">{}</a>",
                esc(&rel_href(dir, &entry.href, entry.fragment.as_deref()), true),
                esc(&entry.text, false)
            ));
        }
        if !entry.children.is_empty() {
            out.push_str("<ol>");
            render_nav_items(&entry.children, dir, out);
            out.push_str("</ol>");
        }
        out.push_str("</li>");
    }
}

/// Write entries into a nav document: the toc nav's first list is
/// replaced, everything else (landmarks, page lists, headings)
/// stays byte-for-byte. With no existing document, a fresh EPUB 3
/// nav is generated.
pub fn render_nav(existing: Option<&str>, entries: &[TocEntry], nav_path: &str) -> String {
    let dir = parent_dir(nav_path);
    let mut list = String::from("<ol>");
    render_nav_items(entries, &dir, &mut list);
    list.push_str("</ol>");
    if let Some(text) = existing {
        let toc_nav = element_span(text, "nav", &|attrs| {
            attrs.iter().any(|(k, v)| {
                k.ends_with("type") && v.split_whitespace().any(|t| t == "toc")
            })
        });
        if let Some((_, inner_start, inner_end, _)) = toc_nav {
            // The first list inside this nav is the contents list.
            let inner = &text[inner_start..inner_end];
            if let Some((ls, _, _, le)) = element_span(inner, "ol", &|_| true)
                .or_else(|| element_span(inner, "ul", &|_| true))
            {
                let mut out = String::with_capacity(text.len());
                out.push_str(&text[..inner_start + ls]);
                out.push_str(&list);
                out.push_str(&text[inner_start + le..]);
                return out;
            }
            let mut out = String::with_capacity(text.len() + list.len());
            out.push_str(&text[..inner_end]);
            out.push_str(&list);
            out.push_str(&text[inner_end..]);
            return out;
        }
        if let Some((_, _, body_inner_end, _)) = element_span(text, "body", &|_| true) {
            let mut out = String::with_capacity(text.len() + list.len() + 40);
            out.push_str(&text[..body_inner_end]);
            out.push_str("<nav epub:type=\"toc\" id=\"toc\">");
            out.push_str(&list);
            out.push_str("</nav>");
            out.push_str(&text[body_inner_end..]);
            return out;
        }
    }
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\" xmlns:epub=\"http://www.idpf.org/2007/ops\"><head><title>Contents</title></head><body><nav epub:type=\"toc\" id=\"toc\">{}</nav></body></html>\n",
        list
    )
}

/// Build contents from the book's headings (decision 136's
/// "Generate from headings"): h1 opens a top-level entry, h2
/// nests under the most recent h1. Files are given in reading
/// order as (container path, current text).
pub fn generate_from_headings(files: &[(String, String)]) -> Vec<TocEntry> {
    let mut out: Vec<TocEntry> = Vec::new();
    for (path, text) in files {
        for (level, heading, _off, id) in headings_with_ids(text) {
            let entry = TocEntry::leaf(heading, path.clone(), id);
            match level {
                1 => out.push(entry),
                2 => match out.last_mut() {
                    Some(parent) => parent.children.push(entry),
                    None => out.push(entry),
                },
                _ => {}
            }
        }
    }
    out
}

/// A standalone inline-contents chapter (decision 138): a normal
/// XHTML document whose body lists the entries as links. With
/// `parts_and_chapters` off only leaf chapters are listed, flat.
pub fn inline_contents_page(
    entries: &[TocEntry],
    page_path: &str,
    parts_and_chapters: bool,
) -> String {
    let dir = parent_dir(page_path);
    fn items(entries: &[TocEntry], dir: &str, leaves_only: bool, out: &mut String) {
        for entry in entries {
            let is_leaf = entry.children.is_empty();
            if !leaves_only || is_leaf {
                out.push_str("<li>");
                if entry.href.is_empty() {
                    out.push_str(&esc(&entry.text, false));
                } else {
                    out.push_str(&format!(
                        "<a href=\"{}\">{}</a>",
                        esc(&rel_href(dir, &entry.href, entry.fragment.as_deref()), true),
                        esc(&entry.text, false)
                    ));
                }
                if !entry.children.is_empty() && !leaves_only {
                    out.push_str("<ul>");
                    items(&entry.children, dir, leaves_only, out);
                    out.push_str("</ul>");
                }
                out.push_str("</li>");
            }
            if leaves_only && !is_leaf {
                items(&entry.children, dir, leaves_only, out);
            }
        }
    }
    let mut body = String::from("<h1>Contents</h1><ul>");
    items(entries, &dir, !parts_and_chapters, &mut body);
    body.push_str("</ul>");
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<!DOCTYPE html>\n<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>Contents</title></head><body>{}</body></html>\n",
        body
    )
}

// ── OPF manifest/spine surgery ──────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OpfItem {
    pub id: String,
    /// Container path (resolved from the manifest href).
    pub href: String,
    pub media_type: String,
    pub properties: Vec<String>,
}

/// Read the OPF's manifest + spine back out for rebuilding.
/// `spine` is (item id, linear) in reading order.
pub fn parse_opf_data(opf: &str, opf_path: &str) -> (Vec<OpfItem>, Vec<(String, bool)>, Option<String>) {
    let dir = parent_dir(opf_path);
    let Ok(doc) = roxmltree::Document::parse(opf) else {
        return (Vec::new(), Vec::new(), None);
    };
    let mut items = Vec::new();
    let mut spine = Vec::new();
    let mut toc_id = None;
    for node in doc.descendants().filter(|n| n.is_element()) {
        match node.tag_name().name() {
            "item" => {
                if let (Some(id), Some(href)) = (node.attribute("id"), node.attribute("href")) {
                    items.push(OpfItem {
                        id: id.to_string(),
                        href: resolve_href(&dir, href),
                        media_type: node.attribute("media-type").unwrap_or("").to_string(),
                        properties: node
                            .attribute("properties")
                            .unwrap_or("")
                            .split_whitespace()
                            .map(str::to_string)
                            .collect(),
                    });
                }
            }
            "spine" => toc_id = node.attribute("toc").map(str::to_string),
            "itemref" => {
                if let Some(idref) = node.attribute("idref") {
                    spine.push((idref.to_string(), node.attribute("linear") != Some("no")));
                }
            }
            _ => {}
        }
    }
    (items, spine, toc_id)
}

/// Replace the manifest's items and the spine's itemrefs in an
/// OPF document, preserving metadata, guide, and everything else
/// byte-for-byte. Hrefs are written relative to the OPF.
pub fn rebuild_opf(
    opf: &str,
    opf_path: &str,
    items: &[OpfItem],
    spine: &[(String, bool)],
    toc_id: Option<&str>,
) -> String {
    let dir = parent_dir(opf_path);
    let mut manifest = String::new();
    for item in items {
        manifest.push_str(&format!(
            "<item id=\"{}\" href=\"{}\" media-type=\"{}\"",
            esc(&item.id, true),
            esc(&rel_href(&dir, &item.href, None), true),
            esc(&item.media_type, true)
        ));
        if !item.properties.is_empty() {
            manifest.push_str(&format!(
                " properties=\"{}\"",
                esc(&item.properties.join(" "), true)
            ));
        }
        manifest.push_str("/>");
    }
    let mut refs = String::new();
    for (id, linear) in spine {
        refs.push_str(&format!("<itemref idref=\"{}\"", esc(id, true)));
        if !linear {
            refs.push_str(" linear=\"no\"");
        }
        refs.push_str("/>");
    }

    let mut out = opf.to_string();
    // Spine first (later in the document, so offsets stay simple
    // doing manifest after — each splice re-searches anyway).
    if let Some((tag_start, inner_start, inner_end, _)) = element_span(&out, "spine", &|_| true) {
        let mut start_tag = out[tag_start..inner_start].to_string();
        if let Ok(re) = regex::Regex::new(r#"\stoc\s*=\s*"[^"]*"|\stoc\s*=\s*'[^']*'"#) {
            start_tag = re.replace(&start_tag, "").into_owned();
        }
        if let Some(toc) = toc_id {
            if let Some(gt) = start_tag.rfind('>') {
                start_tag.insert_str(gt, &format!(" toc=\"{}\"", esc(toc, true)));
            }
        }
        let mut next = String::with_capacity(out.len());
        next.push_str(&out[..tag_start]);
        next.push_str(&start_tag);
        next.push_str(&refs);
        next.push_str(&out[inner_end..]);
        out = next;
    }
    if let Some((_, inner_start, inner_end, _)) = element_span(&out, "manifest", &|_| true) {
        let mut next = String::with_capacity(out.len());
        next.push_str(&out[..inner_start]);
        next.push_str(&manifest);
        next.push_str(&out[inner_end..]);
        out = next;
    }
    out
}

/// Point the EPUB 2 `<meta name="cover">` at a manifest id (or
/// create the meta just inside `<metadata>` when there is none).
pub fn set_cover_meta(opf: &str, cover_id: &str) -> String {
    let patterns = [
        (
            r#"(?i)(<meta[^>]*name\s*=\s*"cover"[^>]*content\s*=\s*")[^"]*(")"#,
            2usize,
        ),
        (
            r#"(?i)(<meta[^>]*content\s*=\s*")[^"]*("[^>]*name\s*=\s*"cover")"#,
            2usize,
        ),
    ];
    for (pattern, _) in patterns {
        if let Ok(re) = regex::Regex::new(pattern) {
            if re.is_match(opf) {
                return re
                    .replace(opf, |c: &regex::Captures| {
                        format!("{}{}{}", &c[1], cover_id, &c[2])
                    })
                    .into_owned();
            }
        }
    }
    if let Some((_, inner_start, _, _)) = element_span(opf, "metadata", &|_| true) {
        let mut out = String::with_capacity(opf.len() + 48);
        out.push_str(&opf[..inner_start]);
        out.push_str(&format!(
            "<meta name=\"cover\" content=\"{}\"/>",
            esc(cover_id, true)
        ));
        out.push_str(&opf[inner_start..]);
        return out;
    }
    opf.to_string()
}

// ── link retargeting ────────────────────────────────────────────────

/// Rewrite every `href`/`src` in a document through `map`. The
/// map receives the resolved (container path, fragment) of each
/// in-book link and returns its replacement, or `None` to leave
/// the link alone. Pure-fragment links (`#note`) resolve against
/// `frag_base_path` — for split pieces that is the *original*
/// file, since the links were written against it; normally it
/// equals `self_path`. External links are never touched.
pub fn rewrite_links(
    text: &str,
    self_path: &str,
    frag_base_path: &str,
    map: &dyn Fn(&str, Option<&str>) -> Option<(String, Option<String>)>,
) -> String {
    let Ok(re) = regex::Regex::new(r#"(?i)(\b(?:href|src)\s*=\s*)("([^"]*)"|'([^']*)')"#)
    else {
        return text.to_string();
    };
    let self_dir = parent_dir(self_path);
    re.replace_all(text, |c: &regex::Captures| {
        let whole = c.get(0).map(|m| m.as_str()).unwrap_or("");
        let prefix = c.get(1).map(|m| m.as_str()).unwrap_or("");
        let (value, quote) = match (c.get(3), c.get(4)) {
            (Some(v), _) => (v.as_str(), '"'),
            (None, Some(v)) => (v.as_str(), '\''),
            _ => return whole.to_string(),
        };
        let lower = value.to_ascii_lowercase();
        if lower.starts_with("http:")
            || lower.starts_with("https:")
            || lower.starts_with("mailto:")
            || lower.starts_with("data:")
            || value.is_empty()
        {
            return whole.to_string();
        }
        let (path_part, frag) = split_href(value);
        let target_path = if path_part.is_empty() {
            frag_base_path.to_string()
        } else {
            resolve_href(&parent_dir(frag_base_path), path_part)
        };
        let Some((new_path, new_frag)) = map(&target_path, frag.as_deref()) else {
            return whole.to_string();
        };
        let href = if new_path == self_path {
            match &new_frag {
                Some(f) => format!("#{f}"),
                None => rel_href(&self_dir, &new_path, None),
            }
        } else {
            rel_href(&self_dir, &new_path, new_frag.as_deref())
        };
        format!("{prefix}{quote}{href}{quote}")
    })
    .into_owned()
}

// ── splitting and merging ───────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitMode {
    /// Split at the top-level block containing this char offset.
    AtChar(usize),
    /// A new file at every top-level h1/h2.
    AtHeadings,
    /// Fill each file to roughly this many bytes of body markup.
    BySize(usize),
}

/// Split a document's `<body>` content at top-level element
/// boundaries. Returns the pieces as complete documents (same
/// prolog and head). A body holding a single wrapper element
/// (`<div>`/`<section>`) is split inside the wrapper, which each
/// piece re-wears — Calibre-built books are wrapped like that.
pub fn split_document(text: &str, mode: SplitMode) -> Vec<String> {
    let Some((_, body_start, body_end, _)) = element_span(text, "body", &|_| true) else {
        return vec![text.to_string()];
    };
    let body = &text[body_start..body_end];
    let blocks = top_level_blocks(body);
    if blocks.len() < 2 {
        return vec![text.to_string()];
    }
    // The cursor split arrives as a char offset into the whole
    // document; grouping works in body-relative bytes.
    let mode = match mode {
        SplitMode::AtChar(chars) => {
            let byte = text
                .char_indices()
                .nth(chars)
                .map(|(b, _)| b)
                .unwrap_or(text.len());
            SplitMode::AtChar(byte.saturating_sub(body_start))
        }
        other => other,
    };
    let groups = group_blocks(body, &blocks, mode);
    if groups.len() < 2 {
        return vec![text.to_string()];
    }
    groups
        .iter()
        .map(|(s, e)| {
            let mut doc = String::with_capacity(text.len());
            doc.push_str(&text[..body_start]);
            doc.push_str(&body[*s..*e]);
            doc.push_str(&text[body_end..]);
            doc
        })
        .collect()
}

/// (start, end, tag) of each top-level child of a body inner
/// string. Text between elements attaches to the following
/// block. A lone wrapper element is descended into.
fn top_level_blocks(body: &str) -> Vec<(usize, usize, String)> {
    let blocks = scan_blocks(body, 0);
    let element_blocks: Vec<&(usize, usize, String)> =
        blocks.iter().filter(|b| !b.2.is_empty()).collect();
    if element_blocks.len() == 1 {
        let (s, e, tag) = element_blocks[0];
        if matches!(tag.as_str(), "div" | "section" | "article" | "main") {
            // Rescan inside the wrapper; wrapper re-wrapping is
            // the caller's grouping concern via returned spans:
            // mark spans so group_blocks can include the shell.
            if let Some((_, inner_s, inner_e, _)) =
                element_span(&body[*s..*e], tag, &|_| true)
            {
                let inner = scan_blocks(&body[s + inner_s..s + inner_e], s + inner_s);
                if inner.iter().filter(|b| !b.2.is_empty()).count() > 1 {
                    let mut wrapped: Vec<(usize, usize, String)> = inner
                        .into_iter()
                        .map(|(bs, be, t)| (bs, be, t))
                        .collect();
                    // The first/last "blocks" absorb the wrapper
                    // shell by extending their spans.
                    if let Some(first) = wrapped.first_mut() {
                        first.0 = *s;
                    }
                    if let Some(last) = wrapped.last_mut() {
                        last.1 = *e;
                    }
                    return wrapped;
                }
            }
        }
    }
    blocks
}

fn scan_blocks(inner: &str, base: usize) -> Vec<(usize, usize, String)> {
    use quick_xml::events::Event;
    let mut reader = quick_xml::Reader::from_str(inner);
    let mut blocks: Vec<(usize, usize, String)> = Vec::new();
    let mut depth = 0usize;
    let mut block_start = 0usize;
    let mut block_tag = String::new();
    loop {
        let pos_before = reader.buffer_position() as usize;
        match reader.read_event() {
            Ok(Event::Start(e)) => {
                if depth == 0 {
                    block_tag = String::from_utf8_lossy(e.local_name().as_ref())
                        .to_ascii_lowercase();
                    // Leading text (whitespace) joins this block:
                    // block_start is already where it began.
                    let _ = pos_before;
                }
                depth += 1;
            }
            Ok(Event::Empty(_)) => {
                if depth == 0 {
                    let end = reader.buffer_position() as usize;
                    blocks.push((base + block_start, base + end, String::new()));
                    block_start = end;
                }
            }
            Ok(Event::End(_)) => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    let end = reader.buffer_position() as usize;
                    blocks.push((
                        base + block_start,
                        base + end,
                        std::mem::take(&mut block_tag),
                    ));
                    block_start = end;
                }
            }
            Ok(Event::Eof) | Err(_) => break,
            _ => {}
        }
    }
    if block_start < inner.len() {
        blocks.push((base + block_start, base + inner.len(), String::new()));
    }
    blocks
}

fn group_blocks(
    body: &str,
    blocks: &[(usize, usize, String)],
    mode: SplitMode,
) -> Vec<(usize, usize)> {
    let mut starts: Vec<usize> = vec![0];
    match mode {
        SplitMode::AtChar(offset) => {
            // Byte offset in the whole document: callers pass the
            // body's inner offset (see split_document caller in
            // the page). Find the block containing it; split
            // before that block (never before the first).
            let mut k = blocks
                .iter()
                .position(|(_, e, _)| offset < *e)
                .unwrap_or(blocks.len() - 1);
            if k == 0 {
                k = 1.min(blocks.len() - 1);
            }
            if k > 0 && k < blocks.len() {
                starts.push(blocks[k].0);
            }
        }
        SplitMode::AtHeadings => {
            for (i, (s, _, tag)) in blocks.iter().enumerate() {
                if i > 0 && (tag == "h1" || tag == "h2") {
                    starts.push(*s);
                }
            }
        }
        SplitMode::BySize(max) => {
            let mut size = 0usize;
            for (i, (s, e, _)) in blocks.iter().enumerate() {
                let len = e - s;
                if i > 0 && size + len > max && size > 0 {
                    starts.push(*s);
                    size = 0;
                }
                size += len;
            }
        }
    }
    starts.sort_unstable();
    starts.dedup();
    let mut groups = Vec::new();
    for w in starts.windows(2) {
        groups.push((w[0], w[1]));
    }
    if let Some(last) = starts.last() {
        groups.push((*last, body.len()));
    }
    groups.retain(|(s, e)| !body[*s..*e].trim().is_empty());
    groups
}

/// Merge documents: the first document's shell (prolog + head)
/// with every document's body content in order.
pub fn merge_documents(texts: &[String]) -> Option<String> {
    let first = texts.first()?;
    let (_, body_start, body_end, _) = element_span(first, "body", &|_| true)?;
    let mut out = String::with_capacity(texts.iter().map(String::len).sum());
    out.push_str(&first[..body_start]);
    for text in texts {
        match element_span(text, "body", &|_| true) {
            Some((_, s, e, _)) => {
                out.push_str(text[s..e].trim());
                out.push('\n');
            }
            None => return None,
        }
    }
    out.push_str(&first[body_end..]);
    Some(out)
}

// ── smaller whole-document edits ────────────────────────────────────

/// The `epub:type` values on the document's `<body>`.
pub fn body_semantics(html: &str) -> Vec<String> {
    let Some((tag_start, inner_start, _, _)) = element_span(html, "body", &|_| true) else {
        return Vec::new();
    };
    let tag = &html[tag_start..inner_start];
    for pattern in [
        r#"(?i)epub:type\s*=\s*"([^"]*)""#,
        r#"(?i)epub:type\s*=\s*'([^']*)'"#,
    ] {
        if let Ok(re) = regex::Regex::new(pattern) {
            if let Some(c) = re.captures(tag) {
                return c[1].split_whitespace().map(str::to_string).collect();
            }
        }
    }
    Vec::new()
}

/// Set (or with an empty list, clear) the `<body>`'s `epub:type`.
pub fn set_semantics(html: &str, types: &[&str]) -> String {
    let Some((tag_start, inner_start, _, _)) = element_span(html, "body", &|_| true) else {
        return html.to_string();
    };
    let mut out = html.to_string();
    let mut tag = out[tag_start..inner_start].to_string();
    if let Ok(re) = regex::Regex::new(r#"(?i)\s*epub:type\s*=\s*(?:"[^"]*"|'[^']*')"#) {
        tag = re.replace(&tag, "").into_owned();
    }
    if !types.is_empty() {
        if let Some(gt) = tag.rfind('>') {
            tag.insert_str(gt, &format!(" epub:type=\"{}\"", types.join(" ")));
        }
        // The epub prefix must be declared on <html>.
        if !out.contains("xmlns:epub") {
            if let Some((hs, hse, _, _)) = element_span(&out, "html", &|_| true) {
                let html_tag = out[hs..hse].to_string();
                if let Some(gt) = html_tag.rfind('>') {
                    let mut new_tag = html_tag.clone();
                    new_tag.insert_str(
                        gt,
                        " xmlns:epub=\"http://www.idpf.org/2007/ops\"",
                    );
                    out.replace_range(hs..hse, &new_tag);
                    // Recompute the body span after the shift.
                    if let Some((tag_start, inner_start, _, _)) =
                        element_span(&out, "body", &|_| true)
                    {
                        out.replace_range(tag_start..inner_start, &tag);
                        return out;
                    }
                    return out;
                }
            }
        }
    }
    out.replace_range(tag_start..inner_start, &tag);
    out
}

/// Stylesheets a document already links (resolved to container
/// paths against the document's own folder).
pub fn linked_stylesheets(html: &str, self_path: &str) -> Vec<String> {
    let dir = parent_dir(self_path);
    let (Ok(re), Ok(href_re)) = (
        regex::Regex::new(r#"(?i)<link\b[^>]*>"#),
        regex::Regex::new(r#"(?i)href\s*=\s*(?:"([^"]*)"|'([^']*)')"#),
    ) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for m in re.find_iter(html) {
        let tag = m.as_str();
        if !tag.to_ascii_lowercase().contains("stylesheet") {
            continue;
        }
        if let Some(c) = href_re.captures(tag) {
            let raw = c.get(1).or_else(|| c.get(2)).map(|m| m.as_str()).unwrap_or("");
            let (path, _) = split_href(raw);
            if !path.is_empty() {
                out.push(resolve_href(&dir, path));
            }
        }
    }
    out
}

/// Add `<link>`s for any of `css_paths` the document does not
/// already link, just before `</head>`.
pub fn link_stylesheets(html: &str, self_path: &str, css_paths: &[String]) -> String {
    let dir = parent_dir(self_path);
    let linked = linked_stylesheets(html, self_path);
    let mut add = String::new();
    for css in css_paths {
        if !linked.contains(css) {
            add.push_str(&format!(
                "<link href=\"{}\" rel=\"stylesheet\" type=\"text/css\"/>",
                esc(&rel_href(&dir, css, None), true)
            ));
        }
    }
    if add.is_empty() {
        return html.to_string();
    }
    match element_span(html, "head", &|_| true) {
        Some((_, _, inner_end, _)) => {
            let mut out = String::with_capacity(html.len() + add.len());
            out.push_str(&html[..inner_end]);
            out.push_str(&add);
            out.push_str(&html[inner_end..]);
            out
        }
        None => html.to_string(),
    }
}

/// Make the document link exactly `css_paths`: stylesheet
/// `<link>`s pointing elsewhere are removed, missing ones are
/// added. Non-stylesheet links are left alone.
pub fn set_linked_stylesheets(html: &str, self_path: &str, css_paths: &[String]) -> String {
    let dir = parent_dir(self_path);
    let (Ok(re), Ok(href_re)) = (
        regex::Regex::new(r#"(?i)<link\b[^>]*>"#),
        regex::Regex::new(r#"(?i)href\s*=\s*(?:"([^"]*)"|'([^']*)')"#),
    ) else {
        return html.to_string();
    };
    let stripped = re.replace_all(html, |c: &regex::Captures| {
        let tag = c.get(0).map(|m| m.as_str()).unwrap_or("");
        if !tag.to_ascii_lowercase().contains("stylesheet") {
            return tag.to_string();
        }
        let keep = href_re
            .captures(tag)
            .and_then(|caps| {
                let raw = caps
                    .get(1)
                    .or_else(|| caps.get(2))
                    .map(|m| m.as_str())
                    .unwrap_or("");
                let (path, _) = split_href(raw);
                let resolved = resolve_href(&dir, path);
                css_paths.iter().any(|p| p == &resolved).then_some(())
            })
            .is_some();
        if keep {
            tag.to_string()
        } else {
            String::new()
        }
    });
    link_stylesheets(&stripped, self_path, css_paths)
}

// ── spell checking ──────────────────────────────────────────────────

/// Words worth spell-checking in visible text: alphabetic runs
/// with internal apostrophes, length ≥ 2, as (char start, char
/// end) spans into `text` plus the word itself. Pure-number and
/// ALL-CAPS tokens (acronyms) are skipped by the caller-side
/// known-word test, not here.
pub fn spell_words(text: &str) -> Vec<(usize, usize, String)> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        if chars[i].is_alphabetic() {
            let start = i;
            let mut word = String::new();
            while i < chars.len()
                && (chars[i].is_alphabetic()
                    || ((chars[i] == '\'' || chars[i] == '\u{2019}')
                        && i + 1 < chars.len()
                        && chars[i + 1].is_alphabetic()))
            {
                word.push(chars[i]);
                i += 1;
            }
            if word.chars().count() >= 2 {
                out.push((start, i, word));
            }
        } else {
            i += 1;
        }
    }
    out
}

/// Is this word spelled acceptably? Case-folded membership plus
/// the usual tolerances: acronyms, possessives, and hyphenated
/// halves are not flagged (hyphens split words before this runs).
pub fn word_known(word: &str, known: &std::collections::HashSet<String>) -> bool {
    if word.chars().count() < 2 {
        return true;
    }
    let folded: String = word
        .chars()
        .map(|c| if c == '\u{2019}' { '\'' } else { c })
        .collect::<String>()
        .to_lowercase();
    if known.contains(&folded) {
        return true;
    }
    // Acronyms (NASA, EPUB) are the author's business.
    if word.chars().all(|c| !c.is_lowercase()) && word.chars().any(|c| c.is_uppercase()) {
        return true;
    }
    // Possessives and contractions of known words.
    if let Some(base) = folded
        .strip_suffix("'s")
        .or_else(|| folded.strip_suffix('\''))
    {
        if known.contains(base) {
            return true;
        }
    }
    if let Some((head, _)) = folded.split_once('\'') {
        if known.contains(head) {
            return true;
        }
    }
    false
}

/// Norvig-style suggestions: known words one edit away, then two
/// edits away if needed. Ranked by edit distance, then by length
/// closeness, then alphabetically; at most six.
pub fn suggestions(word: &str, known: &std::collections::HashSet<String>) -> Vec<String> {
    let w: String = word
        .chars()
        .map(|c| if c == '\u{2019}' { '\'' } else { c })
        .collect::<String>()
        .to_lowercase();
    fn edits1(word: &str) -> Vec<String> {
        let chars: Vec<char> = word.chars().collect();
        let mut out = Vec::new();
        for i in 0..chars.len() {
            // delete
            let mut v = chars.clone();
            v.remove(i);
            out.push(v.into_iter().collect());
            // transpose
            if i + 1 < chars.len() {
                let mut v = chars.clone();
                v.swap(i, i + 1);
                out.push(v.into_iter().collect());
            }
            // replace
            for c in 'a'..='z' {
                if c != chars[i] {
                    let mut v = chars.clone();
                    v[i] = c;
                    out.push(v.into_iter().collect());
                }
            }
        }
        // insert
        for i in 0..=chars.len() {
            for c in 'a'..='z' {
                let mut v = chars.clone();
                v.insert(i, c);
                out.push(v.into_iter().collect());
            }
        }
        out
    }
    let mut found: Vec<String> = edits1(&w)
        .into_iter()
        .filter(|c| known.contains(c))
        .collect();
    found.sort();
    found.dedup();
    if found.len() < 3 {
        let mut second: Vec<String> = Vec::new();
        for e1 in edits1(&w) {
            for e2 in edits1(&e1) {
                if known.contains(&e2) && !found.contains(&e2) {
                    second.push(e2);
                }
            }
            if second.len() > 400 {
                break;
            }
        }
        second.sort();
        second.dedup();
        second.sort_by_key(|c| (c.len() as i32 - w.len() as i32).abs());
        found.extend(second);
    }
    found.sort_by(|a, b| {
        let la = (a.len() as i32 - w.len() as i32).abs();
        let lb = (b.len() as i32 - w.len() as i32).abs();
        la.cmp(&lb).then_with(|| a.cmp(b))
    });
    found.dedup();
    found.truncate(6);
    // Restore the original's capitalisation pattern.
    let capitalized = word
        .chars()
        .next()
        .map(|c| c.is_uppercase())
        .unwrap_or(false)
        && word.chars().skip(1).all(|c| !c.is_uppercase());
    if capitalized {
        found = found
            .into_iter()
            .map(|s| {
                let mut cs = s.chars();
                match cs.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + cs.as_str(),
                    None => s,
                }
            })
            .collect();
    }
    found
}

// ── character table (Insert character, decision 138) ───────────────

/// The glyph grid's cast: (character, name) in display order —
/// the punctuation and symbols book text actually reaches for.
pub const SPECIAL_CHARS: &[(char, &str)] = &[
    ('\u{2014}', "Em dash"),
    ('\u{2013}', "En dash"),
    ('\u{2026}', "Horizontal ellipsis"),
    ('\u{201C}', "Left double quote"),
    ('\u{201D}', "Right double quote"),
    ('\u{2018}', "Left single quote"),
    ('\u{2019}', "Right single quote"),
    ('\u{00AB}', "Left guillemet"),
    ('\u{00BB}', "Right guillemet"),
    ('\u{00B0}', "Degree sign"),
    ('\u{00BD}', "One half"),
    ('\u{00A9}', "Copyright sign"),
    ('\u{00A3}', "Pound sign"),
    ('\u{20AC}', "Euro sign"),
    ('\u{00B1}', "Plus-minus sign"),
    ('\u{00D7}', "Multiplication sign"),
    ('\u{00F7}', "Division sign"),
    ('\u{00BC}', "One quarter"),
    ('\u{00BE}', "Three quarters"),
    ('\u{2153}', "One third"),
    ('\u{2154}', "Two thirds"),
    ('\u{00A5}', "Yen sign"),
    ('\u{20B9}', "Rupee sign"),
    ('\u{00A2}', "Cent sign"),
    ('\u{00AE}', "Registered sign"),
    ('\u{2122}', "Trade mark sign"),
    ('\u{00A7}', "Section sign"),
    ('\u{00B6}', "Pilcrow sign"),
    ('\u{2020}', "Dagger"),
    ('\u{2021}', "Double dagger"),
    ('\u{2022}', "Bullet"),
    ('\u{00B7}', "Middle dot"),
    ('\u{2010}', "Hyphen"),
    ('\u{2011}', "Non-breaking hyphen"),
    ('\u{00A0}', "Non-breaking space"),
    ('\u{2009}', "Thin space"),
    ('\u{201E}', "Double low quote"),
    ('\u{201A}', "Single low quote"),
    ('\u{2039}', "Single left guillemet"),
    ('\u{203A}', "Single right guillemet"),
    ('\u{2032}', "Prime"),
    ('\u{2033}', "Double prime"),
    ('\u{2192}', "Rightwards arrow"),
    ('\u{2190}', "Leftwards arrow"),
    ('\u{2194}', "Left right arrow"),
    ('\u{2015}', "Horizontal bar"),
    ('\u{FFFD}', "Replacement character"),
    ('\u{2605}', "Black star"),
    ('\u{2606}', "White star"),
    ('\u{2713}', "Check mark"),
    ('\u{2717}', "Ballot X"),
    ('\u{00E0}', "a with grave"),
    ('\u{00E1}', "a with acute"),
    ('\u{00E8}', "e with grave"),
    ('\u{00E9}', "e with acute"),
    ('\u{00EA}', "e with circumflex"),
    ('\u{00EF}', "i with diaeresis"),
    ('\u{00F4}', "o with circumflex"),
    ('\u{00FC}', "u with diaeresis"),
    ('\u{00E7}', "c with cedilla"),
    ('\u{00F1}', "n with tilde"),
    ('\u{00DF}', "Sharp s"),
    ('\u{0153}', "oe ligature"),
    ('\u{00E6}', "ae ligature"),
    ('\u{FB01}', "fi ligature"),
    ('\u{FB02}', "fl ligature"),
];

#[cfg(test)]
mod tests {
    use super::*;

    const NAV: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops"><head><title>Contents</title></head>
<body><nav epub:type="toc" id="toc"><h1>Contents</h1><ol><li><a href="text/part1.xhtml">Part One</a><ol><li><a href="text/ch1.xhtml#start">Chapter 1</a></li></ol></li><li><a href="text/ch2.xhtml">Chapter 2</a></li></ol></nav>
<nav epub:type="landmarks"><ol><li><a epub:type="bodymatter" href="text/ch1.xhtml">Start</a></li></ol></nav></body></html>"#;

    const NCX: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1"><head><meta name="dtb:depth" content="2"/></head><docTitle><text>Book</text></docTitle><navMap><navPoint id="navPoint-1" playOrder="1"><navLabel><text>Part One</text></navLabel><content src="text/part1.xhtml"/><navPoint id="navPoint-2" playOrder="2"><navLabel><text>Chapter 1</text></navLabel><content src="text/ch1.xhtml#start"/></navPoint></navPoint></navMap></ncx>"#;

    #[test]
    fn nav_and_ncx_parse_to_the_same_tree() {
        let from_nav = parse_nav(NAV, "OEBPS/nav.xhtml");
        let from_ncx = parse_ncx(NCX, "OEBPS/toc.ncx");
        assert_eq!(toc_count(&from_nav), 3);
        assert_eq!(from_nav[0].text, "Part One");
        assert_eq!(from_nav[0].href, "OEBPS/text/part1.xhtml");
        assert_eq!(from_nav[0].children[0].fragment.as_deref(), Some("start"));
        assert_eq!(from_ncx[0].children[0].href, "OEBPS/text/ch1.xhtml");
        assert_eq!(from_ncx[0].children[0].fragment.as_deref(), Some("start"));
    }

    #[test]
    fn render_round_trips_both_forms() {
        let entries = parse_nav(NAV, "OEBPS/nav.xhtml");
        let nav = render_nav(Some(NAV), &entries, "OEBPS/nav.xhtml");
        assert!(nav.contains("landmarks"), "other navs survive: {nav}");
        assert_eq!(parse_nav(&nav, "OEBPS/nav.xhtml"), entries);
        let ncx = render_ncx(Some(NCX), &entries, "OEBPS/toc.ncx", "Book");
        assert!(ncx.contains("<text>Book</text>"));
        let back = parse_ncx(&ncx, "OEBPS/toc.ncx");
        assert_eq!(toc_count(&back), 3);
        assert_eq!(back[0].children[0].fragment.as_deref(), Some("start"));
    }

    #[test]
    fn headings_generate_nested_contents() {
        let files = vec![
            (
                "OEBPS/ch1.xhtml".to_string(),
                "<html><body><h1 id=\"p1\">Part One</h1><p>x</p><h2 id=\"s1\">Scene</h2></body></html>".to_string(),
            ),
            (
                "OEBPS/ch2.xhtml".to_string(),
                "<html><body><h1><a id=\"c2\"/>Chapter Two</h1></body></html>".to_string(),
            ),
        ];
        let entries = generate_from_headings(&files);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].fragment.as_deref(), Some("p1"));
        assert_eq!(entries[0].children[0].text, "Scene");
        assert_eq!(entries[1].fragment.as_deref(), Some("c2"));
    }

    const OPF: &str = r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" version="3.0"><metadata xmlns:dc="http://purl.org/dc/elements/1.1/"><dc:title>Book</dc:title><meta name="cover" content="img1"/></metadata>
<manifest><item id="ch1" href="ch1.xhtml" media-type="application/xhtml+xml"/><item id="img1" href="cover.jpg" media-type="image/jpeg" properties="cover-image"/><item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/></manifest>
<spine toc="ncx" page-progression-direction="rtl"><itemref idref="ch1"/></spine>
<guide><reference type="text" href="ch1.xhtml"/></guide></package>"#;

    #[test]
    fn opf_rebuild_keeps_everything_else() {
        let (items, spine, toc) = parse_opf_data(OPF, "content.opf");
        assert_eq!(items.len(), 3);
        assert_eq!(toc.as_deref(), Some("ncx"));
        let out = rebuild_opf(OPF, "content.opf", &items, &spine, toc.as_deref());
        assert!(out.contains("page-progression-direction=\"rtl\""));
        assert!(out.contains("<guide><reference type=\"text\" href=\"ch1.xhtml\"/></guide>"));
        assert!(out.contains("toc=\"ncx\""));
        let covered = set_cover_meta(&out, "img9");
        assert!(covered.contains("content=\"img9\""));
    }

    #[test]
    fn split_at_headings_and_cursor() {
        let doc = "<html><head><title>t</title></head><body><h1 id=\"a\">One</h1><p>first</p><h1 id=\"b\">Two</h1><p>second</p></body></html>";
        let pieces = split_document(doc, SplitMode::AtHeadings);
        assert_eq!(pieces.len(), 2);
        assert!(pieces[0].contains("first") && !pieces[0].contains("second"));
        assert!(pieces[1].contains("id=\"b\""));
        assert!(pieces[1].contains("<title>t</title>"));
        let by_size = split_document(doc, SplitMode::BySize(30));
        assert!(by_size.len() >= 2);
    }

    #[test]
    fn merge_concatenates_bodies() {
        let a = "<html><head><title>A</title></head><body><p>one</p></body></html>".to_string();
        let b = "<html><head><title>B</title></head><body><p>two</p></body></html>".to_string();
        let merged = merge_documents(&[a, b]).expect("merge");
        assert!(merged.contains("<title>A</title>"));
        assert!(merged.contains("<p>one</p>") && merged.contains("<p>two</p>"));
    }

    #[test]
    fn rewrite_links_retargets_fragments() {
        let text = "<p><a href=\"ch1.xhtml#moved\">x</a><a href=\"#here\">y</a><a href=\"https://example.com\">z</a></p>";
        let map = |path: &str, frag: Option<&str>| match (path, frag) {
            ("OEBPS/ch1.xhtml", Some("moved")) => {
                Some(("OEBPS/ch1_part2.xhtml".to_string(), Some("moved".to_string())))
            }
            _ => None,
        };
        let out = rewrite_links(text, "OEBPS/ch2.xhtml", "OEBPS/ch2.xhtml", &map);
        assert!(out.contains("href=\"ch1_part2.xhtml#moved\""), "{out}");
        assert!(out.contains("href=\"#here\""));
        assert!(out.contains("https://example.com"));
    }

    #[test]
    fn semantics_and_stylesheets() {
        let html = "<html xmlns=\"http://www.w3.org/1999/xhtml\"><head><title>t</title></head><body><p>x</p></body></html>";
        let set = set_semantics(html, &["preface"]);
        assert!(set.contains("epub:type=\"preface\""), "{set}");
        assert!(set.contains("xmlns:epub"));
        assert_eq!(body_semantics(&set), vec!["preface".to_string()]);
        let linked = link_stylesheets(html, "OEBPS/ch.xhtml", &["OEBPS/style.css".to_string()]);
        assert!(linked.contains("href=\"style.css\""));
        let again = link_stylesheets(&linked, "OEBPS/ch.xhtml", &["OEBPS/style.css".to_string()]);
        assert_eq!(linked, again, "linking twice must not duplicate");
    }

    #[test]
    fn spell_suggestions_find_harrowed() {
        let known: std::collections::HashSet<String> = [
            "harrowed", "harrow", "harried", "she", "the", "lamps",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert!(!word_known("harroed", &known));
        assert!(word_known("lamps", &known));
        assert!(word_known("EPUB", &known), "acronyms pass");
        let got = suggestions("harroed", &known);
        assert_eq!(got.first().map(String::as_str), Some("harrowed"), "{got:?}");
        let words = spell_words("She harroed the lamps.");
        assert_eq!(words.len(), 4);
        assert_eq!(words[1].2, "harroed");
    }

    #[test]
    fn flatten_unflatten_is_identity_and_clamps() {
        let entries = parse_nav(NAV, "OEBPS/nav.xhtml");
        let flat = toc_flatten(entries.clone());
        assert_eq!(flat.len(), 3);
        assert_eq!(flat[1].0, 1, "chapter nests under its part");
        assert_eq!(toc_unflatten(flat), entries);
        // A broken depth jump is clamped, never dropped.
        let wild = toc_unflatten(vec![
            (0, TocEntry::leaf("A".into(), "a.xhtml".into(), None)),
            (5, TocEntry::leaf("B".into(), "b.xhtml".into(), None)),
        ]);
        assert_eq!(toc_count(&wild), 2);
        assert_eq!(wild[0].children[0].text, "B");
    }
}
