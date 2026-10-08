//! The source-span mapper and element serializer (Phase 6, step 6).
//!
//! The paragraph machinery: mapping an element in a chapter to its byte
//! range in the entry's source, and writing an edited paragraph back as
//! source. Step 4's typo fixes work in *text* space — the find string is
//! DOM text re-escaped to source — which refuses (correctly) whenever a
//! selection crosses inline markup. Paragraph editing needs more: the
//! patch must own the element's **whole outer source**, tags and inline
//! children included, so a rewording can flatten an `<em>` or drop a
//! `<span>` and still splice cleanly.
//!
//! The parser stack (chapbook-layout, xml5ever/html5ever) builds an arena
//! DOM without source positions, and the markup5ever `TreeSink` interface
//! has nowhere to put them — so this is a **standalone span-tracking
//! scan** over the raw bytes, as planned: a small strict-XML tokenizer
//! that builds its own element tree with byte ranges attached. It is
//! deliberately stricter than the reader's parse:
//!
//! - tags must balance with no implied closes (`<li>` left open, or a
//!   `</div>` closing past an unclosed `<p>`, make the entry
//!   `NotMappable` — the HTML fallback's tree surgery is exactly what
//!   byte ranges must not guess at);
//! - attribute values must be quoted; a stray `<` in text is refused;
//! - the one tolerance is the HTML void set: `<br>`, `<hr>`, `<img>` and
//!   friends count as self-closing even unslashed, because html5ever's
//!   tree builder voids exactly those elements too — the trees agree, so
//!   the spans can be trusted on slightly sloppy-but-agreeing entries.
//!
//! Paragraph identity is the reader's text: [`locate_paragraph`]
//! reproduces `chapbook-layout`'s plain-text extraction (entities
//! decoded, CSS whitespace collapsed, `<br>` as newline) and matches it
//! against the scanned paragraphs, disambiguated by an ordinal among the
//! equal-text ones. An entry the reader HTML-parses can still be scanned
//! when its structure is unambiguous — but any disagreement shows up as
//! a text mismatch and the locate refuses. **A paragraph is never
//! located on guesswork**: not found, not current, or not mappable —
//! each a visible refusal, never a wrong span.
//!
//! The serializer writes only spans this module authored: the element's
//! verbatim opening tag (its class, id and `epub:type` are content, not
//! decoration), the edited text escaped to source with `\n` as `<br/>`,
//! and the verbatim closing tag. Everything outside the outer span is
//! untouched bytes — the same write-back promise the text matcher makes.

// No production caller until step 7 wires paragraph editing into the
// reader; until then only tests construct these items, and dead_code
// fires on test-only reachability (pitfalls §73). The allow comes off
// when the proofreading mode lands.
#![allow(dead_code)]

use crate::epub_patches::{
    ceil_char_boundary, escape_to_source, floor_char_boundary, PlannedPatch, PATCH_CONTEXT_BYTES,
};
use std::ops::Range;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// One located paragraph: its byte ranges in the entry, its verbatim
/// tags, and its reader-space text (the same string the caller asked
/// for — kept so callers can show exactly what matched).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParagraphSpan {
    /// `<p …>` through `</p>` — the range a patch replaces.
    pub outer: Range<usize>,
    /// The element's content, between the opening tag's `>` and the
    /// closing tag's `<`.
    pub inner: Range<usize>,
    /// The opening tag exactly as the source spells it.
    pub open_tag: String,
    /// The closing tag exactly as the source spells it.
    pub close_tag: String,
    /// The paragraph's text, reader-normalized.
    pub text: String,
}

/// What a locate concluded. Mirrors the matcher's `Resolution` shape:
/// found, or a refusal that says which kind.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParagraphLocate {
    Found(ParagraphSpan),
    /// No paragraph carries that text with those neighbours. Either the
    /// text is not paragraph-granular, or the chapter changed under the
    /// caller.
    NotFound,
    /// The text and neighbours match more than one paragraph — a
    /// repeated passage whose surroundings also repeat. The count is
    /// carried for the message; guessing would risk editing the wrong
    /// one, so this is a refusal.
    Ambiguous(usize),
    /// The entry's structure could not be scanned trustworthily
    /// (malformed XML, no single root, not UTF-8). The HTML-fallback
    /// territory; conservative by design.
    NotMappable,
}

/// Why a paragraph patch could not be planned.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParagraphRefusal {
    /// New text normalizes to the old one.
    Unchanged,
    /// No paragraph carries that text with those neighbours.
    NotFound,
    /// The paragraph cannot be told apart from its repeats even with
    /// the neighbours — the count of indistinguishable candidates.
    Ambiguous(usize),
    /// The entry is not span-mappable.
    NotMappable,
}

/// The chapter's paragraph texts, in document order, reader-normalized —
/// the mapper's own enumeration of what a chapter's paragraphs are.
/// `None` when the entry is not span-mappable. The proofread surfaces
/// use this to say whether a chapter offers paragraph editing at all;
/// tests use it to assert the paragraph set directly.
pub fn paragraph_texts(bytes: &[u8]) -> Option<Vec<String>> {
    let text = std::str::from_utf8(bytes).ok()?;
    let scan = Scan::parse(text).ok()?;
    Some(scan.paragraphs().iter().map(|&id| scan.element_text(id)).collect())
}

/// Locate the paragraph whose reader text is `want_text`, told apart
/// from its repeats by its neighbours: `prev` is the preceding
/// paragraph's text, `next` the following one's, and `None` asserts
/// that the paragraph is the chapter's first (resp. last) — the reader
/// always knows, so a mismatch is a chapter that changed underneath
/// the caller, and the locate refuses rather than guesses.
///
/// `want_text` is compared in identity space: trimmed, whitespace-run
/// collapsed — the same normalization the reader's `extract_text_at`
/// applies — so a source-wrapped paragraph matches its rendered form.
/// The neighbours are trimmed the same way before comparing.
pub fn locate_paragraph(
    bytes: &[u8],
    want_text: &str,
    prev: Option<&str>,
    next: Option<&str>,
) -> ParagraphLocate {
    match std::str::from_utf8(bytes) {
        Ok(text) => locate_in_str(text, want_text, prev, next),
        Err(_) => ParagraphLocate::NotMappable,
    }
}

/// The locate body, over source that is known UTF-8 (the planner checks
/// once and shares the result with the span extraction).
fn locate_in_str(
    text: &str,
    want_text: &str,
    prev: Option<&str>,
    next: Option<&str>,
) -> ParagraphLocate {
    let want = want_text.trim_matches(is_css_space);
    if want.is_empty() {
        return ParagraphLocate::NotFound;
    }
    let scan = match Scan::parse(text) {
        Ok(scan) => scan,
        Err(_) => return ParagraphLocate::NotMappable,
    };
    let ids = scan.paragraphs();
    let texts: Vec<String> = ids.iter().map(|&id| scan.element_text(id)).collect();
    let prev = prev.map(|h| h.trim_matches(is_css_space));
    let next = next.map(|h| h.trim_matches(is_css_space));
    let mut hits: Vec<usize> = Vec::new();
    for i in 0..texts.len() {
        if texts[i] != want {
            continue;
        }
        let prev_ok = match &prev {
            None => i == 0,
            Some(hint) => i > 0 && texts[i - 1] == *hint,
        };
        let next_ok = match &next {
            None => i + 1 == texts.len(),
            Some(hint) => i + 1 < texts.len() && texts[i + 1] == *hint,
        };
        if prev_ok && next_ok {
            hits.push(i);
        }
    }
    match hits.len() {
        0 => ParagraphLocate::NotFound,
        1 => {
            let id = ids[hits[0]];
            let node = &scan.nodes[id];
            ParagraphLocate::Found(ParagraphSpan {
                outer: node.outer.clone(),
                inner: node.inner.clone(),
                open_tag: node.open_tag.clone(),
                close_tag: node.close_tag.clone(),
                text: texts[hits[0]].clone(),
            })
        }
        n => ParagraphLocate::Ambiguous(n),
    }
}

/// Serialize an edited paragraph back to source.
///
/// The output is deterministic for spans this module authored: the
/// verbatim opening tag, the new text escaped to source with hard
/// newlines as `<br/>` and CSS whitespace runs collapsed to one space
/// (the reader's own normalization, inverted), and the verbatim closing
/// tag. Inline markup in the old paragraph is *not* carried over — the
/// new text is what the user typed, flat. Empty text writes an empty
/// element (`<p …></p>`), which the reader renders as nothing.
pub fn serialize_paragraph(span: &ParagraphSpan, new_text: &str) -> String {
    let mut out =
        String::with_capacity(span.open_tag.len() + span.close_tag.len() + new_text.len());
    out.push_str(&span.open_tag);
    for (i, seg) in new_text.split('\n').enumerate() {
        if i > 0 {
            out.push_str("<br/>");
        }
        let collapsed = seg
            .split(is_css_space)
            .filter(|word| !word.is_empty())
            .collect::<Vec<_>>()
            .join(" ");
        out.push_str(&escape_to_source(&collapsed));
    }
    out.push_str(&span.close_tag);
    out
}

/// Verify a paragraph edit and turn it into a storable patch — the
/// paragraph sibling of `epub_patches::plan_text_patch`.
///
/// `virtual_bytes` is the entry's source with the book's earlier pending
/// patches already applied, because that is what this patch will run
/// against. `paragraph_text` is the reader's text for the paragraph (the
/// editor's prefill); `prev` and `next` are its neighbours' texts, `None`
/// at the chapter's ends, disambiguating equal-text paragraphs; and
/// `new_text` is the edited text, plain — inline markup in the old
/// source is replaced by it wholesale.
///
/// The returned `PlannedPatch` carries the paragraph's whole outer
/// source as its find, so the literal matcher — render filter, bake,
/// everything that already applies `kind == "text"` patches — can apply
/// it unchanged. The caller stores it with `kind = "paragraph"` so the
/// surfaces can tell the two apart.
pub fn plan_paragraph_patch(
    virtual_bytes: &[u8],
    paragraph_text: &str,
    prev: Option<&str>,
    next: Option<&str>,
    new_text: &str,
) -> Result<PlannedPatch, ParagraphRefusal> {
    // The unchanged check runs in identity space, so cosmetic whitespace
    // differences are not edits, and the reader's trailing newline (a
    // block-boundary artifact of extraction) does not count either.
    let old_id = identity_text(paragraph_text);
    let new_id = identity_text(new_text);
    if old_id == new_id {
        return Err(ParagraphRefusal::Unchanged);
    }
    let text = match std::str::from_utf8(virtual_bytes) {
        Ok(text) => text,
        Err(_) => return Err(ParagraphRefusal::NotMappable),
    };
    let span = match locate_in_str(text, paragraph_text, prev, next) {
        ParagraphLocate::Found(span) => span,
        ParagraphLocate::NotFound => return Err(ParagraphRefusal::NotFound),
        ParagraphLocate::Ambiguous(n) => return Err(ParagraphRefusal::Ambiguous(n)),
        ParagraphLocate::NotMappable => return Err(ParagraphRefusal::NotMappable),
    };
    let find_text = text[span.outer.clone()].to_string();
    let replace_text = serialize_paragraph(&span, new_text);
    // Context anchors, captured from the bytes around the outer span —
    // the same capture-at-verify-time contract as text patches.
    let anchor_start = floor_char_boundary(text, span.outer.start, PATCH_CONTEXT_BYTES);
    let anchor_end = ceil_char_boundary(text, span.outer.end, PATCH_CONTEXT_BYTES);
    let context_before = text[anchor_start..span.outer.start].to_string();
    let context_after = text[span.outer.end..anchor_end].to_string();
    Ok(PlannedPatch {
        find_text,
        replace_text,
        context_before,
        context_after,
    })
}

/// Reader text → identity text: what two paragraph texts are compared
/// as. Hard newlines survive (they are `<br/>`, real structure);
/// everything else CSS-collapsible collapses, and the ends trim.
fn identity_text(raw: &str) -> String {
    let lines: Vec<String> = raw
        .split('\n')
        .map(|seg| {
            seg.split(is_css_space)
                .filter(|word| !word.is_empty())
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect();
    lines.join("\n").trim_matches(is_css_space).to_string()
}

// ---------------------------------------------------------------------------
// The scan
// ---------------------------------------------------------------------------

/// One scanned element: name, byte ranges, verbatim tags, and children
/// in document order (text interleaved with elements, so extraction can
/// walk exactly what the parser saw).
struct ScanNode {
    name: String,
    /// `<name …>` start through the closing `>` of `</name>`.
    outer: Range<usize>,
    /// Content only: after the opening tag, before the closing tag.
    inner: Range<usize>,
    open_tag: String,
    close_tag: String,
    children: Vec<Child>,
}

/// A child in document order: an element, or a run of text (a CDATA
/// section is a text run whose bytes carry no entities).
enum Child {
    Element(usize),
    Text {
        range: Range<usize>,
        cdata: bool,
    },
}

/// The scanned document: the source it came from, an element arena, and
/// the single root (XHTML entries have exactly one; the scan refuses
/// anything else).
struct Scan<'a> {
    text: &'a str,
    nodes: Vec<ScanNode>,
    root: usize,
}

/// The HTML void set, tolerated unslashed: html5ever's tree builder
/// voids exactly these, so a tree this scan builds over them agrees
/// with the reader's — the one place strict XML and lenient HTML see
/// eye to eye.
fn is_void(name: &str) -> bool {
    matches!(
        name,
        "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link" | "meta"
            | "param" | "source" | "track" | "wbr"
    )
}

fn is_name_start(b: u8) -> bool {
    b.is_ascii_alphabetic() || b == b'_' || b == b':'
}

fn is_name_char(b: u8) -> bool {
    is_name_start(b) || b.is_ascii_digit() || b == b'-' || b == b'.'
}

fn is_xml_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\r')
}

fn find_from(bytes: &[u8], from: usize, pat: &[u8]) -> Option<usize> {
    if bytes.len() < pat.len() {
        return None;
    }
    (from..=bytes.len() - pat.len()).find(|&i| &bytes[i..i + pat.len()] == pat)
}

fn starts_with_at(bytes: &[u8], i: usize, pat: &[u8]) -> bool {
    i + pat.len() <= bytes.len() && &bytes[i..i + pat.len()] == pat
}

impl<'a> Scan<'a> {
    /// Parse an entry's source. Every structural oddity the reader's
    /// XML path would reject — unbalanced tags, mis-nested closes, a
    /// stray `<`, an unterminated construct, no single root — is `Err`,
    /// which callers report as `NotMappable`.
    fn parse(text: &'a str) -> Result<Scan<'a>, ()> {
        let bytes = text.as_bytes();
        let mut nodes: Vec<ScanNode> = Vec::new();
        let mut stack: Vec<usize> = Vec::new();
        let mut roots: Vec<usize> = Vec::new();
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] != b'<' {
                // A text run up to the next tag. Every other `<` in the
                // input belongs to a branch below; one that classifies
                // as nothing is an error, so no `<` survives into text.
                let end = bytes[i..]
                    .iter()
                    .position(|&b| b == b'<')
                    .map_or(bytes.len(), |off| i + off);
                Self::push_text(&mut nodes, &stack, i..end, false);
                i = end;
                continue;
            }
            if i + 1 >= bytes.len() {
                return Err(());
            }
            if starts_with_at(bytes, i, b"<?") {
                // XML declaration or processing instruction.
                i = find_from(bytes, i + 2, b"?>").ok_or(())? + 2;
            } else if starts_with_at(bytes, i, b"<!--") {
                i = find_from(bytes, i + 4, b"-->").ok_or(())? + 3;
            } else if starts_with_at(bytes, i, b"<![CDATA[") {
                let content = i + 9;
                let end = find_from(bytes, content, b"]]>").ok_or(())?;
                Self::push_text(&mut nodes, &stack, content..end, true);
                i = end + 3;
            } else if bytes[i + 1] == b'!' {
                // DOCTYPE (internal subsets in EPUB content documents
                // are unheard of; the scan stops at the first `>`).
                i = find_from(bytes, i, b">").ok_or(())? + 1;
            } else if bytes[i + 1] == b'/' {
                let (name, after) = Self::parse_close(text, i)?;
                let top = *stack.last().ok_or(())?;
                if nodes[top].name != name {
                    // A close that skips elements — the HTML fallback's
                    // implied-close territory. Refuse, never guess.
                    return Err(());
                }
                stack.pop();
                nodes[top].inner.end = i;
                nodes[top].outer.end = after;
                nodes[top].close_tag = text[i..after].to_string();
                i = after;
            } else {
                let (name, after, self_closing) = Self::parse_open(text, i)?;
                let node = ScanNode {
                    name: name.to_string(),
                    outer: i..after,
                    inner: after..after,
                    open_tag: text[i..after].to_string(),
                    close_tag: String::new(),
                    children: Vec::new(),
                };
                let id = nodes.len();
                nodes.push(node);
                if let Some(&parent) = stack.last() {
                    nodes[parent].children.push(Child::Element(id));
                } else {
                    roots.push(id);
                }
                if !self_closing {
                    stack.push(id);
                }
                i = after;
            }
        }
        if !stack.is_empty() || roots.len() != 1 {
            return Err(());
        }
        Ok(Scan {
            text,
            nodes,
            root: roots[0],
        })
    }

    /// `</name>` → (name, offset after `>`). Strict: name, optional
    /// whitespace, `>`, nothing else.
    fn parse_close(text: &str, start: usize) -> Result<(String, usize), ()> {
        let bytes = text.as_bytes();
        let mut i = start + 2;
        if i >= bytes.len() || !is_name_start(bytes[i]) {
            return Err(());
        }
        let name_start = i;
        while i < bytes.len() && is_name_char(bytes[i]) {
            i += 1;
        }
        let name = text[name_start..i].to_string();
        while i < bytes.len() && is_xml_space(bytes[i]) {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'>' {
            return Err(());
        }
        Ok((name, i + 1))
    }

    /// `<name attrs>` or `<name attrs/>` → (name, offset after `>`,
    /// self-closing). Attribute values must be quoted (XML's rule); the
    /// void set is self-closing even unslashed, so slightly sloppy —
    /// but html5ever-agreeing — entries stay mappable.
    fn parse_open(text: &str, start: usize) -> Result<(&str, usize, bool), ()> {
        let bytes = text.as_bytes();
        let mut i = start + 1;
        if i >= bytes.len() || !is_name_start(bytes[i]) {
            return Err(());
        }
        let name_start = i;
        while i < bytes.len() && is_name_char(bytes[i]) {
            i += 1;
        }
        let name = &text[name_start..i];
        let mut self_closing = false;
        loop {
            while i < bytes.len() && is_xml_space(bytes[i]) {
                i += 1;
            }
            if i >= bytes.len() {
                return Err(());
            }
            match bytes[i] {
                b'>' => {
                    i += 1;
                    break;
                }
                b'/' => {
                    i += 1;
                    if i < bytes.len() && bytes[i] == b'>' {
                        self_closing = true;
                        i += 1;
                        break;
                    }
                    return Err(());
                }
                _ => {
                    // An attribute. Its name runs to whitespace, `=`,
                    // `>` or `/`; a value must follow, quoted.
                    let attr_start = i;
                    while i < bytes.len()
                        && !is_xml_space(bytes[i])
                        && bytes[i] != b'='
                        && bytes[i] != b'>'
                        && bytes[i] != b'/'
                    {
                        i += 1;
                    }
                    if i == attr_start {
                        return Err(());
                    }
                    while i < bytes.len() && is_xml_space(bytes[i]) {
                        i += 1;
                    }
                    if i >= bytes.len() || bytes[i] != b'=' {
                        // Valueless attribute — malformed XML, and the
                        // reader will have HTML-parsed it. Conservative.
                        return Err(());
                    }
                    i += 1;
                    while i < bytes.len() && is_xml_space(bytes[i]) {
                        i += 1;
                    }
                    if i >= bytes.len() {
                        return Err(());
                    }
                    let quote = bytes[i];
                    if quote != b'"' && quote != b'\'' {
                        return Err(());
                    }
                    i += 1;
                    while i < bytes.len() && bytes[i] != quote {
                        i += 1;
                    }
                    if i >= bytes.len() {
                        return Err(());
                    }
                    i += 1;
                }
            }
        }
        if !self_closing && is_void(name) {
            self_closing = true;
        }
        Ok((name, i, self_closing))
    }

    fn push_text(nodes: &mut [ScanNode], stack: &[usize], range: Range<usize>, cdata: bool) {
        if range.is_empty() {
            return;
        }
        if let Some(&parent) = stack.last() {
            nodes[parent].children.push(Child::Text { range, cdata });
        }
        // Text outside any element is prolog/epilog; nothing anchors it.
    }

    /// The paragraph candidates, in document order: the deepest
    /// text-bearing non-inline elements, skipping the subtrees the
    /// reader's extraction skips.
    fn paragraphs(&self) -> Vec<usize> {
        let mut out = Vec::new();
        self.collect_paragraphs(self.root, &mut out);
        out
    }

    fn collect_paragraphs(&self, id: usize, out: &mut Vec<usize>) {
        let name = self.nodes[id].name.as_str();
        if matches!(name, "head" | "script" | "style" | "template") {
            return;
        }
        if self.is_paragraph(id) {
            out.push(id);
        }
        for child in &self.nodes[id].children {
            if let Child::Element(cid) = child {
                self.collect_paragraphs(*cid, out);
            }
        }
    }

    /// An element is a paragraph when it is not inline, its subtree
    /// carries text, and no non-inline child's subtree does — the
    /// deepest text-bearing block, which is what the reader lays out as
    /// one paragraph. A `div` around `p`s is not; the `p`s are. A bare
    /// text run directly in `body` makes `body` itself the paragraph —
    /// the anonymous block the reader renders.
    fn is_paragraph(&self, id: usize) -> bool {
        let name = self.nodes[id].name.as_str();
        if is_inline(name) {
            return false;
        }
        if !self.subtree_has_text(id) {
            return false;
        }
        for child in &self.nodes[id].children {
            if let Child::Element(cid) = child {
                let child_name = self.nodes[*cid].name.as_str();
                if !is_inline(child_name) && self.subtree_has_text(*cid) {
                    return false;
                }
            }
        }
        true
    }

    fn subtree_has_text(&self, id: usize) -> bool {
        let node = &self.nodes[id];
        if matches!(node.name.as_str(), "script" | "style" | "template") {
            return false;
        }
        for child in &node.children {
            match child {
                Child::Element(cid) => {
                    if self.subtree_has_text(*cid) {
                        return true;
                    }
                }
                Child::Text { range, cdata } => {
                    let piece = &self.text[range.clone()];
                    let meaningful = if *cdata {
                        !piece.trim_matches(is_css_space).is_empty()
                    } else {
                        !decode_entities(piece).trim_matches(is_css_space).is_empty()
                    };
                    if meaningful {
                        return true;
                    }
                }
            }
        }
        false
    }

    /// The element's text, reader-normalized. Mirrors
    /// `chapbook-layout`'s `extract_text_at` walk (see
    /// `crates/chapbook-layout/src/dom/text.rs` — keep in sync):
    /// entities decoded, `<br>` a hard newline, block children bounded,
    /// whitespace collapsed per block.
    fn element_text(&self, id: usize) -> String {
        let mut raw = String::new();
        self.walk_text(id, &mut raw);
        normalize_and_trim(&raw)
    }

    fn walk_text(&self, id: usize, out: &mut String) {
        for child in &self.nodes[id].children {
            match child {
                Child::Text {
                    range,
                    cdata: false,
                } => out.push_str(&decode_entities(&self.text[range.clone()])),
                Child::Text {
                    range,
                    cdata: true,
                } => out.push_str(&self.text[range.clone()]),
                Child::Element(cid) => {
                    let name = self.nodes[*cid].name.as_str();
                    if matches!(name, "script" | "style" | "template") {
                        continue;
                    }
                    if name == "br" {
                        out.push(LINE_BREAK);
                        continue;
                    }
                    let block = is_block(name);
                    if block {
                        out.push(BLOCK_BOUNDARY);
                    }
                    self.walk_text(*cid, out);
                    if block {
                        out.push(BLOCK_BOUNDARY);
                    }
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Text semantics — mirrors of chapbook-layout's dom/text.rs (keep in
// sync; that module is the definition of "the reader's text").
// ---------------------------------------------------------------------------

/// Sentinel between block boxes; becomes a newline. Ordinary `\n` in
/// source text is inline whitespace and collapses like a space.
const BLOCK_BOUNDARY: char = '\u{1}';
/// Sentinel for a forced `<br>` line break within a block.
const LINE_BREAK: char = '\u{2}';

/// The CSS-collapsible whitespace set (space, tab, LF, CR, FF). Notably
/// NOT U+00A0 no-break space — that is content, and it exists to not
/// collapse.
fn is_css_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\x0C')
}

/// Tags treated as block-level for text-boundary purposes — the same
/// default-UA-display set as the reader's extraction.
fn is_block(name: &str) -> bool {
    matches!(
        name,
        "address"
            | "article"
            | "aside"
            | "blockquote"
            | "div"
            | "dd"
            | "dl"
            | "dt"
            | "figcaption"
            | "figure"
            | "footer"
            | "h1"
            | "h2"
            | "h3"
            | "h4"
            | "h5"
            | "h6"
            | "header"
            | "hr"
            | "li"
            | "main"
            | "nav"
            | "ol"
            | "p"
            | "pre"
            | "section"
            | "table"
            | "td"
            | "th"
            | "tr"
            | "ul"
    )
}

/// Elements that never are paragraphs and never block their parent from
/// being one: inline formatting, replaced elements, and leaf media.
/// `svg` and `math` are here as leaves — the reader hands them to their
/// own renderers, and their internal text is not a paragraph of the
/// flow.
fn is_inline(name: &str) -> bool {
    matches!(
        name,
        "a"
            | "abbr"
            | "acronym"
            | "audio"
            | "b"
            | "bdi"
            | "bdo"
            | "big"
            | "br"
            | "button"
            | "canvas"
            | "cite"
            | "code"
            | "data"
            | "datalist"
            | "del"
            | "dfn"
            | "em"
            | "embed"
            | "font"
            | "i"
            | "iframe"
            | "img"
            | "ins"
            | "kbd"
            | "label"
            | "map"
            | "mark"
            | "math"
            | "nobr"
            | "object"
            | "output"
            | "picture"
            | "q"
            | "rb"
            | "rp"
            | "rt"
            | "rtc"
            | "ruby"
            | "s"
            | "samp"
            | "select"
            | "small"
            | "span"
            | "strike"
            | "strong"
            | "sub"
            | "sup"
            | "svg"
            | "time"
            | "tt"
            | "u"
            | "var"
            | "video"
            | "wbr"
    )
}

/// Decode entity references the way the reader's XML parse does: the
/// five predefined names, plus decimal and hex character references.
/// Anything else stays verbatim — which makes the extracted text differ
/// from the reader's (an HTML-fallback entry), so the locate refuses on
/// mismatch rather than mapping a paragraph the reader sees differently.
fn decode_entities(raw: &str) -> String {
    if !raw.contains('&') {
        return raw.to_string();
    }
    let mut out = String::with_capacity(raw.len());
    let mut rest = raw;
    while let Some(pos) = rest.find('&') {
        out.push_str(&rest[..pos]);
        let after = &rest[pos..];
        // An entity is `&…;` with at most a handful of characters
        // between; anything longer is not one (or not one we decode).
        let entity = after.find(';').filter(|&end| (3..=12).contains(&end));
        match entity {
            Some(end) => {
                let name = &after[1..end];
                let decoded = if let Some(hex) =
                    name.strip_prefix("#x").or_else(|| name.strip_prefix("#X"))
                {
                    u32::from_str_radix(hex, 16).ok().and_then(char::from_u32)
                } else if let Some(dec) = name.strip_prefix('#') {
                    dec.parse::<u32>().ok().and_then(char::from_u32)
                } else {
                    match name {
                        "amp" => Some('&'),
                        "lt" => Some('<'),
                        "gt" => Some('>'),
                        "apos" => Some('\''),
                        "quot" => Some('"'),
                        _ => None,
                    }
                };
                match decoded {
                    Some(ch) => out.push(ch),
                    None => out.push_str(&after[..=end]),
                }
                rest = &after[end + 1..];
            }
            None => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

/// Collapse whitespace within blocks; render block boundaries as
/// newlines and `<br>` as a hard newline — the reader's normalize, plus
/// the end-trim that extraction's trailing block boundary would
/// otherwise leave behind.
fn normalize_and_trim(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for block in raw.split(BLOCK_BOUNDARY) {
        let lines: Vec<String> = block
            .split(LINE_BREAK)
            .map(|seg| {
                seg.split(is_css_space)
                    .filter(|word| !word.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect();
        let block_text = lines.join("\n");
        if block_text.trim_matches(is_css_space).is_empty() {
            continue;
        }
        out.push_str(&block_text);
        out.push('\n');
    }
    out.trim_matches(is_css_space).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::PatchRecord;
    use crate::epub_patches::{apply_text_patches, Resolution};

    /// A chapter-shaped entry exercising the machinery: inline markup,
    /// entities, source-wrapped text, duplicates, nested bearers, a
    /// `<br>`, and content the reader's extraction skips.
    const CHAPTER: &str = concat!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
        "<html xmlns=\"http://www.w3.org/1999/xhtml\">\n",
        "<head><title>Chapter title</title></head>\n",
        "<body>\n",
        "<p class=\"first\">He said <em>teh</em> word &amp; left.</p>\n",
        "<p>The second paragraph, wrapped in the\nsource with a numeric &#65; entity.</p>\n",
        "<p>duplicate</p>\n",
        "<p>duplicate</p>\n",
        "<blockquote><p>Quoted line.</p></blockquote>\n",
        "<div><p>Wrapped in a div.</p><p>Second in div.</p></div>\n",
        "<ul><li>One item</li><li>Two items</li></ul>\n",
        "<p>a<br/>b</p>\n",
        "<p><![CDATA[a & b]]></p>\n",
        "</body>\n",
        "</html>\n",
    );

    /// The chapter's paragraphs, in document order — the enumeration
    /// every neighbour hint is drawn from.
    const PARAGRAPHS: [&str; 11] = [
        "He said teh word & left.",
        "The second paragraph, wrapped in the source with a numeric A entity.",
        "duplicate",
        "duplicate",
        "Quoted line.",
        "Wrapped in a div.",
        "Second in div.",
        "One item",
        "Two items",
        "a\nb",
        "a & b",
    ];

    fn paragraph_record(
        id: i64,
        href: &str,
        find: &str,
        replace: &str,
        before: &str,
        after: &str,
    ) -> PatchRecord {
        PatchRecord {
            id,
            book_id: 1,
            kind: "paragraph".into(),
            href: href.into(),
            chapter_index: 0,
            find_text: find.into(),
            replace_text: replace.into(),
            context_before: before.into(),
            context_after: after.into(),
            source: "proofread".into(),
            status: "pending".into(),
            created_at: "2026-10-08T00:00:00Z".into(),
            applied_at: None,
        }
    }

    #[test]
    fn the_paragraph_enumeration_matches_the_reader_text() {
        // The paragraph set itself: reader text semantics end to end —
        // source wrapping collapses, numeric entities decode, the
        // head's title is not a paragraph, blockquote/div/li bear
        // paragraphs while their containers do not, `<br>` is a hard
        // newline, CDATA is raw text.
        let texts = paragraph_texts(CHAPTER.as_bytes()).expect("the chapter is mappable");
        assert_eq!(texts, PARAGRAPHS.to_vec());
        // A structurally untrustworthy entry has no paragraph list.
        let unclosed = "<html><body><ul><li>one<li>two</ul></body></html>";
        assert_eq!(paragraph_texts(unclosed.as_bytes()), None);
    }

    #[test]
    fn locates_a_paragraph_with_exact_spans() {
        // The first paragraph: outer must cover the whole element
        // including the <em>, inner exactly its content, tags verbatim.
        // `prev` of None asserts it is the chapter's first.
        let located = locate_paragraph(
            CHAPTER.as_bytes(),
            PARAGRAPHS[0],
            None,
            Some(PARAGRAPHS[1]),
        );
        let span = match located {
            ParagraphLocate::Found(span) => span,
            other => panic!("expected Found, got {other:?}"),
        };
        assert_eq!(span.open_tag, "<p class=\"first\">");
        assert_eq!(span.close_tag, "</p>");
        assert_eq!(
            &CHAPTER[span.outer.clone()],
            "<p class=\"first\">He said <em>teh</em> word &amp; left.</p>"
        );
        assert_eq!(
            &CHAPTER[span.inner.clone()],
            "He said <em>teh</em> word &amp; left."
        );
        assert_eq!(span.text, PARAGRAPHS[0]);
    }

    #[test]
    fn identical_paragraphs_are_disambiguated_by_neighbours() {
        // Two identical paragraphs: truthful neighbours pick each one
        // out, and the wrong neighbours (or asserting first/last)
        // refuse rather than guess.
        let first = match locate_paragraph(
            CHAPTER.as_bytes(),
            "duplicate",
            Some(PARAGRAPHS[1]),
            Some("duplicate"),
        ) {
            ParagraphLocate::Found(span) => span,
            other => panic!("expected Found, got {other:?}"),
        };
        let second = match locate_paragraph(
            CHAPTER.as_bytes(),
            "duplicate",
            Some("duplicate"),
            Some(PARAGRAPHS[4]),
        ) {
            ParagraphLocate::Found(span) => span,
            other => panic!("expected Found, got {other:?}"),
        };
        assert!(first.outer.start < second.outer.start);
        assert_eq!(&CHAPTER[first.outer.clone()], "<p>duplicate</p>");
        assert_eq!(&CHAPTER[second.outer.clone()], "<p>duplicate</p>");
        // Neither duplicate is the chapter's first paragraph.
        assert_eq!(
            locate_paragraph(CHAPTER.as_bytes(), "duplicate", None, None),
            ParagraphLocate::NotFound
        );
    }

    #[test]
    fn refrains_that_neighbours_cannot_split_refuse() {
        // y refrain y refrain y: both refrains have the same text and
        // the same neighbours, and the truth cannot tell them apart.
        // Two candidates is a refusal, never a guess.
        let refrain = concat!(
            "<html><body>",
            "<p>y</p><p>refrain</p><p>y</p><p>refrain</p><p>y</p>",
            "</body></html>"
        );
        assert_eq!(
            locate_paragraph(refrain.as_bytes(), "refrain", Some("y"), Some("y")),
            ParagraphLocate::Ambiguous(2)
        );
        // A neighbour that does not sit beside a refrain finds nothing.
        assert_eq!(
            locate_paragraph(refrain.as_bytes(), "refrain", Some("x"), Some("y")),
            ParagraphLocate::NotFound
        );
    }

    #[test]
    fn unknown_text_is_not_found() {
        assert_eq!(
            locate_paragraph(CHAPTER.as_bytes(), "no paragraph says this", None, None),
            ParagraphLocate::NotFound
        );
        // Whitespace-only text is never a paragraph identity.
        assert_eq!(
            locate_paragraph(CHAPTER.as_bytes(), "   ", None, None),
            ParagraphLocate::NotFound
        );
    }

    #[test]
    fn plan_refuses_unchanged_edits() {
        // Same text, cosmetic whitespace difference: not an edit. The
        // check runs before any locating, so the hints are moot here.
        let refusal =
            plan_paragraph_patch(CHAPTER.as_bytes(), "Quoted line.", None, None, "Quoted  line.\n");
        assert_eq!(refusal, Err(ParagraphRefusal::Unchanged));
    }

    #[test]
    fn planned_patch_flattens_inline_markup_and_keeps_the_opening_tag() {
        // Rewording a paragraph whose source carries <em>: the find is
        // the whole outer source, the replace keeps the opening tag
        // verbatim and writes the new text flat.
        let planned = plan_paragraph_patch(
            CHAPTER.as_bytes(),
            PARAGRAPHS[0],
            None,
            Some(PARAGRAPHS[1]),
            "He said the word & left, quickly.",
        )
        .expect("the paragraph is locatable");
        assert_eq!(
            planned.find_text,
            "<p class=\"first\">He said <em>teh</em> word &amp; left.</p>"
        );
        assert_eq!(
            planned.replace_text,
            "<p class=\"first\">He said the word &amp; left, quickly.</p>"
        );
    }

    #[test]
    fn a_paragraph_patch_applies_through_the_matcher() {
        // The whole point of carrying the outer source as the find: the
        // existing apply path needs nothing new but the widened kind
        // filter. Everything outside the span must stay identical bytes.
        let planned = plan_paragraph_patch(
            CHAPTER.as_bytes(),
            PARAGRAPHS[4],
            Some("duplicate"),
            Some(PARAGRAPHS[5]),
            "Amended line.",
        )
        .expect("the paragraph is locatable");
        let record = paragraph_record(
            7,
            "c.xhtml",
            &planned.find_text,
            &planned.replace_text,
            &planned.context_before,
            &planned.context_after,
        );
        let (out, outcomes) = apply_text_patches("c.xhtml", CHAPTER.as_bytes(), &[record]);
        assert_eq!(outcomes.len(), 1);
        let start = CHAPTER.find(&planned.find_text).expect("find text present");
        assert_eq!(
            outcomes[0].resolution,
            Resolution::Found(start..start + planned.find_text.len())
        );
        let expected = CHAPTER.replace(
            "<blockquote><p>Quoted line.</p></blockquote>",
            "<blockquote><p>Amended line.</p></blockquote>",
        );
        assert_eq!(String::from_utf8(out).unwrap(), expected);
        // And the edited paragraph locates under its new text.
        assert!(matches!(
            locate_paragraph(
                expected.as_bytes(),
                "Amended line.",
                Some("duplicate"),
                Some(PARAGRAPHS[5])
            ),
            ParagraphLocate::Found(_)
        ));
    }

    #[test]
    fn round_trip_stability_br_and_all() {
        // The <br> paragraph: reader text "a\nb"; editing to "x\ny"
        // must serialize as <br/> and round-trip back to "x\ny".
        let planned = plan_paragraph_patch(
            CHAPTER.as_bytes(),
            PARAGRAPHS[9],
            Some(PARAGRAPHS[8]),
            Some(PARAGRAPHS[10]),
            "x\ny",
        )
        .expect("the paragraph is locatable");
        assert_eq!(planned.replace_text, "<p>x<br/>y</p>");
        let record = paragraph_record(
            9,
            "c.xhtml",
            &planned.find_text,
            &planned.replace_text,
            &planned.context_before,
            &planned.context_after,
        );
        let (out, _) = apply_text_patches("c.xhtml", CHAPTER.as_bytes(), &[record]);
        let text = String::from_utf8(out).unwrap();
        assert!(text.contains("<p>x<br/>y</p>"));
        let again = match locate_paragraph(
            text.as_bytes(),
            "x\ny",
            Some(PARAGRAPHS[8]),
            Some(PARAGRAPHS[10]),
        ) {
            ParagraphLocate::Found(span) => span,
            other => panic!("expected Found, got {other:?}"),
        };
        assert_eq!(again.text, "x\ny");
    }

    #[test]
    fn cdata_text_needs_no_decoding() {
        // CDATA content is raw text to both the reader and this scan —
        // "a & b" matches without any entity involved.
        let span = match locate_paragraph(
            CHAPTER.as_bytes(),
            PARAGRAPHS[10],
            Some(PARAGRAPHS[9]),
            None,
        ) {
            ParagraphLocate::Found(span) => span,
            other => panic!("expected Found, got {other:?}"),
        };
        assert_eq!(&CHAPTER[span.outer.clone()], "<p><![CDATA[a & b]]></p>");
    }

    #[test]
    fn conservative_on_structurally_untrustworthy_entries() {
        // Unclosed <li>: the HTML fallback's implied close — unmappable.
        let unclosed = "<html><body><ul><li>one<li>two</ul></body></html>";
        assert_eq!(
            locate_paragraph(unclosed.as_bytes(), "one", None, None),
            ParagraphLocate::NotMappable
        );
        // A close that skips an open element.
        let misnested = "<html><body><div><p>a</div></body></html>";
        assert_eq!(
            locate_paragraph(misnested.as_bytes(), "a", None, None),
            ParagraphLocate::NotMappable
        );
        // A bare `<` in text is not XML.
        let stray_lt = "<html><body><p>a < b</p></body></html>";
        assert_eq!(
            locate_paragraph(stray_lt.as_bytes(), "a < b", None, None),
            ParagraphLocate::NotMappable
        );
        // No single root.
        let two_roots = "<html></html><html></html>";
        assert_eq!(
            locate_paragraph(two_roots.as_bytes(), "x", None, None),
            ParagraphLocate::NotMappable
        );
        // Not UTF-8.
        assert_eq!(
            locate_paragraph(&[0xff, 0xfe], "x", None, None),
            ParagraphLocate::NotMappable
        );
    }

    #[test]
    fn conservative_on_reader_text_mismatch() {
        // An entry with &nbsp;: XML-invalid, so the reader parses it as
        // HTML and its text carries U+00A0 — while this scan keeps the
        // literal "&nbsp;". The locate must refuse (mismatch), never
        // map a paragraph the reader sees differently.
        let nbsp_entry = "<html><body><p>a &nbsp; b</p></body></html>";
        assert_eq!(
            locate_paragraph(nbsp_entry.as_bytes(), "a\u{a0} b", None, None),
            ParagraphLocate::NotFound
        );
    }

    #[test]
    fn unslashed_void_tags_are_tolerated() {
        // <br> without the slash: html5ever voids it too, so the trees
        // agree and the paragraph is mappable.
        let unslashed = "<html><body><p>a<br>b</p></body></html>";
        let span = match locate_paragraph(unslashed.as_bytes(), "a\nb", None, None) {
            ParagraphLocate::Found(span) => span,
            other => panic!("expected Found, got {other:?}"),
        };
        assert_eq!(span.text, "a\nb");
    }

    #[test]
    fn emptying_a_paragraph_is_a_real_edit() {
        // Whitespace-only new text is a change: the serializer writes
        // the empty element, which the reader renders as nothing.
        let planned = plan_paragraph_patch(
            CHAPTER.as_bytes(),
            "duplicate",
            Some("duplicate"),
            Some(PARAGRAPHS[4]),
            "  ",
        )
        .expect("the paragraph is locatable");
        assert_eq!(planned.replace_text, "<p></p>");
        let record = paragraph_record(
            11,
            "c.xhtml",
            &planned.find_text,
            &planned.replace_text,
            &planned.context_before,
            &planned.context_after,
        );
        let (out, _) = apply_text_patches("c.xhtml", CHAPTER.as_bytes(), &[record]);
        assert!(String::from_utf8(out).unwrap().contains("<p></p>"));
        // And the emptied paragraph's old identity is gone: no
        // "duplicate" is preceded by a "duplicate" any more.
        let expected = CHAPTER
            .replace("<p>duplicate</p>\n<p>duplicate</p>", "<p>duplicate</p>\n<p></p>");
        assert_eq!(
            locate_paragraph(expected.as_bytes(), "duplicate", Some("duplicate"), None),
            ParagraphLocate::NotFound
        );
    }
}
