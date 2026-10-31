//! The editor's instant tools and the Find & replace engine
//! (decisions 128 and 131): pure text transforms the workspace
//! applies as one undo step per file. Everything here is
//! conservative by design — where a transform cannot be sure it
//! is safe, it leaves the text alone.

use crate::epub_container::FileKind;
use crate::epub_css::{elem_from_start_tag, selector_matches, Elem};

// ── Smarten punctuation ────────────────────────────────────────────

/// Curly quotes, dashes and ellipses in the *text* of a document.
/// Tags, attributes and the content of script/style/code/pre are
/// never touched. Returns the new text and how many replacements
/// were made.
pub fn smarten_html(html: &str) -> (String, usize) {
    let mut out = String::with_capacity(html.len());
    let mut count = 0usize;
    let bytes = html.as_bytes();
    let mut i = 0usize;
    let mut raw_depth = 0usize; // inside script/style/code/pre
    while i < bytes.len() {
        if bytes[i] == b'<' {
            // Comment/CDATA pass through whole.
            if html[i..].starts_with("<!--") {
                let end = html[i + 4..]
                    .find("-->")
                    .map(|n| i + 4 + n + 3)
                    .unwrap_or(bytes.len());
                out.push_str(&html[i..end]);
                i = end;
                continue;
            }
            let closing = i + 1 < bytes.len() && bytes[i + 1] == b'/';
            let name_start = if closing { i + 2 } else { i + 1 };
            let mut j = name_start;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || matches!(bytes[j], b':' | b'-'))
            {
                j += 1;
            }
            let name = html[name_start..j].to_ascii_lowercase();
            // Tag end, respecting quoted attributes.
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
            let self_closed = k > j && bytes[k - 1] == b'/';
            if matches!(name.as_str(), "script" | "style" | "code" | "pre" | "kbd") {
                if closing {
                    raw_depth = raw_depth.saturating_sub(1);
                } else if !self_closed {
                    raw_depth += 1;
                }
            }
            let end = (k + 1).min(bytes.len());
            out.push_str(&html[i..end]);
            i = end;
            continue;
        }
        // Text run to the next tag.
        let mut j = i;
        while j < bytes.len() && bytes[j] != b'<' {
            j += 1;
        }
        if raw_depth > 0 {
            out.push_str(&html[i..j]);
        } else {
            out.push_str(&smarten_text(&html[i..j], &mut count));
        }
        i = j;
    }
    (out, count)
}

fn smarten_text(text: &str, count: &mut usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    let mut out: Vec<char> = Vec::with_capacity(chars.len());
    let mut i = 0usize;
    let opens = |prev: Option<char>| match prev {
        None => true,
        Some(c) => c.is_whitespace() || matches!(c, '(' | '[' | '{' | '“' | '‘' | '—' | '–' | '/'),
    };
    while i < chars.len() {
        let c = chars[i];
        let prev = out.last().copied();
        let next = chars.get(i + 1).copied();
        match c {
            '.' if next == Some('.') && chars.get(i + 2) == Some(&'.') => {
                out.push('…');
                *count += 1;
                i += 3;
                // Any fourth dot (end of sentence) stays.
            }
            '-' if next == Some('-') && chars.get(i + 2) == Some(&'-') => {
                out.push('—');
                *count += 1;
                i += 3;
            }
            '-' if next == Some('-') => {
                out.push('–');
                *count += 1;
                i += 2;
            }
            '`' if next == Some('`') => {
                out.push('“');
                *count += 1;
                i += 2;
            }
            '`' => {
                out.push('‘');
                *count += 1;
                i += 1;
            }
            '\'' if next == Some('\'') => {
                out.push('”');
                *count += 1;
                i += 2;
            }
            '"' => {
                out.push(if opens(prev) { '“' } else { '”' });
                *count += 1;
                i += 1;
            }
            '\'' => {
                // Apostrophe inside a word, or a closing quote.
                let in_word = prev.is_some_and(|c| c.is_alphanumeric())
                    || (opens(prev) && next.is_some_and(|c| c.is_ascii_digit())); // '90s
                if opens(prev) && !in_word {
                    out.push('‘');
                } else {
                    out.push('’');
                }
                *count += 1;
                i += 1;
            }
            _ => {
                out.push(c);
                i += 1;
            }
        }
    }
    out.into_iter().collect()
}

// ── Reformat code ──────────────────────────────────────────────────

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "source", "track",
    "wbr",
];
const INLINE: &[&str] = &[
    "a", "abbr", "b", "bdi", "bdo", "br", "cite", "code", "data", "dfn", "em", "i", "kbd", "mark",
    "q", "rp", "rt", "ruby", "s", "samp", "small", "span", "strong", "sub", "sup", "time", "u",
    "var", "wbr", "img", "audio", "video",
];
const RAW: &[&str] = &["pre", "script", "style", "textarea"];

/// Pretty-print markup or CSS. `None` when the text is too broken
/// to move safely (unbalanced CSS braces).
pub fn reformat(text: &str, kind: FileKind) -> Option<String> {
    match kind {
        FileKind::Css => reformat_css(text),
        FileKind::Html | FileKind::Xml => Some(reformat_markup(text)),
        _ => None,
    }
}

enum Token {
    /// A whole tag as written, with its lowered name.
    Tag {
        raw: String,
        name: String,
        closing: bool,
        self_closed: bool,
    },
    /// <?xml?>, <!DOCTYPE>, comments: own line, verbatim.
    Misc(String),
    Text(String),
}

fn tokenize_markup(text: &str) -> Vec<Token> {
    let bytes = text.as_bytes();
    let mut out = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] == b'<' {
            if text[i..].starts_with("<!--") {
                let end = text[i + 4..]
                    .find("-->")
                    .map(|n| i + 4 + n + 3)
                    .unwrap_or(bytes.len());
                out.push(Token::Misc(text[i..end].trim().to_string()));
                i = end;
                continue;
            }
            if text[i..].starts_with("<![CDATA[") {
                let end = text[i + 9..]
                    .find("]]>")
                    .map(|n| i + 9 + n + 3)
                    .unwrap_or(bytes.len());
                out.push(Token::Misc(text[i..end].to_string()));
                i = end;
                continue;
            }
            if text[i..].starts_with("<?") || text[i..].starts_with("<!") {
                let end = text[i..]
                    .find('>')
                    .map(|n| i + n + 1)
                    .unwrap_or(bytes.len());
                out.push(Token::Misc(text[i..end].trim().to_string()));
                i = end;
                continue;
            }
            let closing = i + 1 < bytes.len() && bytes[i + 1] == b'/';
            let name_start = if closing { i + 2 } else { i + 1 };
            let mut j = name_start;
            while j < bytes.len()
                && (bytes[j].is_ascii_alphanumeric() || matches!(bytes[j], b':' | b'-'))
            {
                j += 1;
            }
            if j == name_start {
                // A stray "<": treat as text.
                i = push_text(&mut out, text, i, i + 1);
                continue;
            }
            let name = text[name_start..j].to_ascii_lowercase();
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
            let end = (k + 1).min(bytes.len());
            let raw = &text[i..end];
            let self_closed = VOID.contains(&name.as_str())
                || raw
                    .trim_end()
                    .trim_end_matches('>')
                    .trim_end()
                    .ends_with('/');
            out.push(Token::Tag {
                raw: raw.trim().to_string(),
                name: name.clone(),
                closing,
                self_closed,
            });
            i = end;
            // Raw elements: swallow content verbatim to the close.
            if !closing && !self_closed && RAW.contains(&name.as_str()) {
                let close = format!("</{name}");
                if let Some(at) = text[i..].to_ascii_lowercase().find(&close) {
                    if at > 0 {
                        out.push(Token::Text(text[i..i + at].to_string()));
                    }
                    i += at;
                }
            }
            continue;
        }
        let mut j = i;
        while j < bytes.len() && bytes[j] != b'<' {
            j += 1;
        }
        i = push_text(&mut out, text, i, j);
    }
    out
}

fn push_text(out: &mut Vec<Token>, text: &str, from: usize, to: usize) -> usize {
    if to > from {
        out.push(Token::Text(text[from..to].to_string()));
    }
    to
}

/// Whitespace runs become one space — including at the edges,
/// where a single space is render-significant between inlines.
fn collapse_ws(text: &str) -> String {
    let mut out = String::new();
    if text.starts_with(char::is_whitespace) {
        out.push(' ');
    }
    out.push_str(&text.split_whitespace().collect::<Vec<_>>().join(" "));
    if text.ends_with(char::is_whitespace) && !out.ends_with(' ') {
        out.push(' ');
    }
    out
}

fn reformat_markup(text: &str) -> String {
    let tokens = tokenize_markup(text);
    let mut lines: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut indent = 0usize;
    // Is the element on top of the stack raw (pre/script/…)?
    let mut raw_stack: Vec<String> = Vec::new();

    let flush = |lines: &mut Vec<String>, current: &mut String, indent: usize| {
        let trimmed = current.trim_end();
        if !trimmed.is_empty() {
            lines.push(format!("{}{}", "  ".repeat(indent), trimmed));
        }
        current.clear();
    };

    for token in tokens {
        match token {
            Token::Misc(raw) => {
                flush(&mut lines, &mut current, indent);
                lines.push(format!("{}{}", "  ".repeat(indent), raw));
            }
            Token::Text(raw) => {
                if !raw_stack.is_empty() {
                    // Verbatim; only the join with the tags is trimmed.
                    current.push_str(&raw);
                } else {
                    let collapsed = collapse_ws(&raw);
                    if current.is_empty() {
                        current.push_str(collapsed.trim_start());
                    } else {
                        current.push_str(&collapsed);
                    }
                }
            }
            Token::Tag {
                raw,
                name,
                closing,
                self_closed,
            } => {
                let inline = INLINE.contains(&name.as_str());
                if closing {
                    if let Some(pos) = raw_stack.iter().rposition(|n| *n == name) {
                        raw_stack.remove(pos);
                        current.push_str(&raw);
                        let body = std::mem::take(&mut current);
                        lines.push(format!("{}{body}", "  ".repeat(indent)));
                    } else if inline {
                        current.push_str(&raw);
                    } else {
                        flush(&mut lines, &mut current, indent);
                        indent = indent.saturating_sub(1);
                        current.push_str(&raw);
                        flush(&mut lines, &mut current, indent);
                    }
                } else if self_closed {
                    if inline {
                        current.push_str(&raw);
                    } else {
                        flush(&mut lines, &mut current, indent);
                        lines.push(format!("{}{}", "  ".repeat(indent), raw));
                    }
                } else if RAW.contains(&name.as_str()) {
                    // Open tag, verbatim body and close tag stay
                    // one unbroken entry; only the open tag indents.
                    flush(&mut lines, &mut current, indent);
                    current.push_str(&raw);
                    raw_stack.push(name);
                } else if inline {
                    current.push_str(&raw);
                } else {
                    flush(&mut lines, &mut current, indent);
                    current.push_str(&raw);
                    indent += 1;
                    // The open tag stays on its own line only when a
                    // block follows; text after it continues the line
                    // (flush decides when the next block arrives, so
                    // emit the tag now and let text start fresh).
                    let tag_line = std::mem::take(&mut current);
                    lines.push(format!("{}{}", "  ".repeat(indent - 1), tag_line));
                }
            }
        }
    }
    flush(&mut lines, &mut current, indent);
    // No leading blank line; one trailing newline.
    while lines.first().is_some_and(|l| l.trim().is_empty()) {
        lines.remove(0);
    }
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    out
}

fn reformat_css(text: &str) -> Option<String> {
    // Brace balance check (outside strings/comments).
    let cleaned = strip_css_comments(text);
    let mut depth = 0i32;
    for b in cleaned.bytes() {
        match b {
            b'{' => depth += 1,
            b'}' => depth -= 1,
            _ => {}
        }
        if depth < 0 {
            return None;
        }
    }
    if depth != 0 {
        return None;
    }

    let mut lines: Vec<String> = Vec::new();
    let mut indent = 0usize;
    let mut buf = String::new();
    let mut chars = text.chars().peekable();
    let mut in_str: Option<char> = None;
    let mut in_comment = false;
    let emit = |lines: &mut Vec<String>, indent: usize, s: &str| {
        let s = s.trim();
        if !s.is_empty() {
            lines.push(format!("{}{}", "  ".repeat(indent), s));
        }
    };
    while let Some(c) = chars.next() {
        if in_comment {
            buf.push(c);
            if c == '*' && chars.peek() == Some(&'/') {
                buf.push(chars.next().unwrap_or('/'));
                in_comment = false;
                emit(&mut lines, indent, &buf);
                buf.clear();
            }
            continue;
        }
        if let Some(q) = in_str {
            buf.push(c);
            if c == q {
                in_str = None;
            }
            continue;
        }
        match c {
            '/' if chars.peek() == Some(&'*') => {
                if !buf.trim().is_empty() {
                    buf.push(c);
                } else {
                    buf.clear();
                    buf.push(c);
                }
                buf.push(chars.next().unwrap_or('*'));
                in_comment = true;
            }
            '"' | '\'' => {
                in_str = Some(c);
                buf.push(c);
            }
            '{' => {
                let prelude = buf.split_whitespace().collect::<Vec<_>>().join(" ");
                lines.push(format!("{}{} {{", "  ".repeat(indent), prelude));
                indent += 1;
                buf.clear();
            }
            '}' => {
                emit(&mut lines, indent, &with_semi(&buf));
                buf.clear();
                indent = indent.saturating_sub(1);
                lines.push(format!("{}}}", "  ".repeat(indent)));
            }
            ';' => {
                buf.push(';');
                if indent > 0 {
                    emit(&mut lines, indent, &buf);
                    buf.clear();
                } else {
                    // A statement at-rule (@import …;) keeps its line.
                    emit(&mut lines, 0, &buf);
                    buf.clear();
                }
            }
            '\n' => buf.push(' '),
            _ => buf.push(c),
        }
    }
    emit(&mut lines, indent, &buf);
    let mut out = lines.join("\n");
    if !out.is_empty() {
        out.push('\n');
    }
    Some(out)
}

fn with_semi(buf: &str) -> String {
    let trimmed = buf.trim();
    if trimmed.is_empty() || trimmed.ends_with(';') {
        trimmed.to_string()
    } else {
        format!("{trimmed};")
    }
}

/// Comments → a single space each, for brace counting.
fn strip_css_comments(text: &str) -> String {
    let mut out = String::new();
    let mut chars = text.chars().peekable();
    let mut in_str: Option<char> = None;
    while let Some(c) = chars.next() {
        if let Some(q) = in_str {
            out.push(c);
            if c == q {
                in_str = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => {
                in_str = Some(c);
                out.push(c);
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                out.push(' ');
                while let Some(c) = chars.next() {
                    if c == '*' && chars.peek() == Some(&'/') {
                        chars.next();
                        break;
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

// ── Remove unused CSS ──────────────────────────────────────────────

/// Every element's ancestor chain in a document (root first, the
/// element itself last) — the audience CSS selectors play to.
pub fn all_element_chains(html: &str) -> Vec<Vec<Elem>> {
    let bytes = html.as_bytes();
    let mut stack: Vec<Elem> = Vec::new();
    let mut out: Vec<Vec<Elem>> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if html[i..].starts_with("<!--") {
            i = html[i + 4..]
                .find("-->")
                .map(|n| i + 4 + n + 3)
                .unwrap_or(bytes.len());
            continue;
        }
        if html[i..].starts_with("<!") || html[i..].starts_with("<?") {
            i = html[i..]
                .find('>')
                .map(|n| i + n + 1)
                .unwrap_or(bytes.len());
            continue;
        }
        let closing = i + 1 < bytes.len() && bytes[i + 1] == b'/';
        let name_start = if closing { i + 2 } else { i + 1 };
        let mut j = name_start;
        while j < bytes.len()
            && (bytes[j].is_ascii_alphanumeric() || matches!(bytes[j], b':' | b'-'))
        {
            j += 1;
        }
        if j == name_start {
            i += 1;
            continue;
        }
        let name = html[name_start..j].to_ascii_lowercase();
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
        if closing {
            if let Some(pos) = stack.iter().rposition(|e| e.tag == name) {
                stack.truncate(pos);
            }
        } else {
            let elem = elem_from_start_tag(&name, &html[j..k]);
            let mut chain = stack.clone();
            chain.push(elem.clone());
            out.push(chain);
            let self_closed = VOID.contains(&name.as_str()) || html[j..k].trim_end().ends_with('/');
            if !self_closed {
                stack.push(elem);
            }
        }
        i = (k + 1).min(bytes.len());
    }
    out
}

/// Cut top-level rules no element in the book can match. Nested
/// (conditional-group) rules are always kept. Returns the new
/// stylesheet and how many rules went.
pub fn remove_unused_css(css: &str, chains: &[Vec<Elem>]) -> (String, usize) {
    let rules = crate::epub_css::parse_rules(css);
    let mut dead: Vec<(usize, usize)> = Vec::new();
    for rule in &rules {
        if rule.nested {
            continue;
        }
        let used = rule.selectors.iter().any(|selector| {
            chains
                .iter()
                .any(|chain| selector_matches(selector, chain).is_some())
        });
        if !used {
            dead.push(rule.span);
        }
    }
    if dead.is_empty() {
        return (css.to_string(), 0);
    }
    let removed = dead.len();
    let mut out = String::with_capacity(css.len());
    let mut at = 0usize;
    for (start, end) in dead {
        if start >= at {
            out.push_str(&css[at..start]);
            at = end;
        }
    }
    out.push_str(&css[at..]);
    // Collapse the blank runs the cuts leave behind.
    let mut tidy = String::with_capacity(out.len());
    let mut blank_run = 0usize;
    for line in out.lines() {
        if line.trim().is_empty() {
            blank_run += 1;
            if blank_run > 1 {
                continue;
            }
        } else {
            blank_run = 0;
        }
        tidy.push_str(line);
        tidy.push('\n');
    }
    (tidy.trim_start_matches('\n').to_string(), removed)
}

// ── Find & replace ─────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Default)]
pub struct SearchOptions {
    pub match_case: bool,
    pub whole_word: bool,
    pub regex: bool,
}

pub struct Searcher {
    re: regex::Regex,
    expand: bool,
}

impl Searcher {
    pub fn new(query: &str, options: SearchOptions) -> Result<Searcher, String> {
        if query.is_empty() {
            return Err("Nothing to search for".to_string());
        }
        let mut pattern = if options.regex {
            query.to_string()
        } else {
            regex::escape(query)
        };
        if options.whole_word {
            pattern = format!(r"\b(?:{pattern})\b");
        }
        let re = regex::RegexBuilder::new(&pattern)
            .case_insensitive(!options.match_case)
            .build()
            .map_err(|e| e.to_string())?;
        Ok(Searcher {
            re,
            expand: options.regex,
        })
    }

    /// Byte spans of every match.
    pub fn find_all(&self, text: &str) -> Vec<(usize, usize)> {
        self.re
            .find_iter(text)
            .map(|m| (m.start(), m.end()))
            .collect()
    }

    /// Replace every match; `$1` groups expand in Regex mode only.
    pub fn replace_all(&self, text: &str, replacement: &str) -> (String, usize) {
        let count = self.re.find_iter(text).count();
        if count == 0 {
            return (text.to_string(), 0);
        }
        let out = if self.expand {
            self.re.replace_all(text, replacement)
        } else {
            self.re.replace_all(text, regex::NoExpand(replacement))
        };
        (out.into_owned(), count)
    }
}

// ── Tag mapper (decision 141) ─────────────────────────────────────

/// One tag-mapping rule: `from` is a bare tag (`b`) or a tag with
/// one class (`div.poem`); `to` is the replacement tag name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagRule {
    pub from: String,
    pub to: String,
}

/// Rename elements by the rules, opening and closing tags in step
/// — a small stack remembers what each open element became, since
/// a closing tag carries no class to match on. Attributes,
/// including the selecting class, are kept exactly as written;
/// only tag names change. Returns the new text and how many
/// elements were renamed.
pub fn retag(html: &str, rules: &[TagRule]) -> (String, usize) {
    if rules.is_empty() {
        return (html.to_string(), 0);
    }
    let parsed: Vec<(String, Option<String>, String)> = rules
        .iter()
        .map(|r| match r.from.split_once('.') {
            Some((tag, class)) => (
                tag.to_ascii_lowercase(),
                Some(class.to_string()),
                r.to.clone(),
            ),
            None => (r.from.to_ascii_lowercase(), None, r.to.clone()),
        })
        .collect();
    let bare = |name: &str| -> Option<String> {
        parsed
            .iter()
            .find(|(tag, class, _)| class.is_none() && tag == name)
            .map(|(_, _, to)| to.clone())
    };
    let for_open = |name: &str, classes: &[&str]| -> Option<String> {
        parsed
            .iter()
            .find(|(tag, class, _)| {
                tag == name
                    && match class {
                        None => true,
                        Some(want) => classes.iter().any(|c| c == want),
                    }
            })
            .map(|(_, _, to)| to.clone())
    };

    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len());
    let mut count = 0usize;
    let mut stack: Vec<(String, Option<String>)> = Vec::new();
    let mut pos = 0usize; // input flushed into `out` up to here
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        let rest = &html[i..];
        if rest.starts_with("<!--") {
            i = rest.find("-->").map(|e| i + e + 3).unwrap_or(bytes.len());
            continue;
        }
        if rest.starts_with("<![CDATA[") {
            i = rest.find("]]>").map(|e| i + e + 3).unwrap_or(bytes.len());
            continue;
        }
        let next = bytes.get(i + 1).copied();
        if matches!(next, Some(b'!') | Some(b'?')) {
            i = rest.find('>').map(|e| i + e + 1).unwrap_or(bytes.len());
            continue;
        }
        let closing = next == Some(b'/');
        let name_start = i + if closing { 2 } else { 1 };
        if !bytes
            .get(name_start)
            .map(|b| b.is_ascii_alphabetic())
            .unwrap_or(false)
        {
            i += 1;
            continue;
        }
        let mut name_end = name_start;
        while name_end < bytes.len()
            && (bytes[name_end].is_ascii_alphanumeric()
                || bytes[name_end] == b'-'
                || bytes[name_end] == b':')
        {
            name_end += 1;
        }
        let name = html[name_start..name_end].to_ascii_lowercase();
        // The tag's closing '>', quote-aware.
        let mut end = name_end;
        let mut quote: Option<u8> = None;
        while end < bytes.len() {
            let b = bytes[end];
            match quote {
                Some(q) if b == q => quote = None,
                Some(_) => {}
                None if b == b'"' || b == b'\'' => quote = Some(b),
                None if b == b'>' => break,
                None => {}
            }
            end += 1;
        }
        if end >= bytes.len() {
            break;
        }
        let replacement: Option<String> = if closing {
            match stack.iter().rposition(|(open, _)| *open == name) {
                Some(at) => {
                    let (_, renamed) = stack[at].clone();
                    stack.truncate(at);
                    renamed.or_else(|| bare(&name))
                }
                None => bare(&name),
            }
        } else {
            let inner = &html[name_end..end];
            let classes = class_tokens(inner);
            let renamed = for_open(&name, &classes);
            let self_closing = inner.trim_end().ends_with('/') || VOID.contains(&name.as_str());
            if renamed.is_some() {
                count += 1;
            }
            if !self_closing {
                stack.push((name.clone(), renamed.clone()));
            }
            renamed
        };
        if let Some(new_name) = replacement {
            if new_name != html[name_start..name_end] {
                out.push_str(&html[pos..name_start]);
                out.push_str(&new_name);
                pos = name_end;
            }
        }
        i = end + 1;
    }
    out.push_str(&html[pos..]);
    (out, count)
}

/// The class tokens of a start tag's inner text (` id="a" class="x y"`).
fn class_tokens(inner: &str) -> Vec<&str> {
    let bytes = inner.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        if (i == 0 || bytes[i - 1].is_ascii_whitespace())
            && inner[i..].to_ascii_lowercase().starts_with("class")
            && bytes
                .get(i + 5)
                .map(|b| b.is_ascii_whitespace() || *b == b'=')
                .unwrap_or(true)
        {
            let mut j = i + 5;
            while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                j += 1;
            }
            if bytes.get(j) == Some(&b'=') {
                j += 1;
                while j < bytes.len() && bytes[j].is_ascii_whitespace() {
                    j += 1;
                }
                if let Some(q) = bytes.get(j).copied() {
                    if q == b'"' || q == b'\'' {
                        let start = j + 1;
                        let mut end = start;
                        while end < bytes.len() && bytes[end] != q {
                            end += 1;
                        }
                        return inner[start..end].split_ascii_whitespace().collect();
                    }
                }
            }
            return Vec::new();
        }
        i += 1;
    }
    Vec::new()
}

// ── Filter style information (decision 129) ────────────────────────

/// Does `property` fall under the filter set? Entries are exact
/// property names, or prefixes when they end in `-` (`margin-`
/// takes the whole family, plus bare `margin` itself).
fn filtered_property(property: &str, filter: &[&str]) -> bool {
    let property = property.to_ascii_lowercase();
    filter.iter().any(|f| {
        let f = f.to_ascii_lowercase();
        if let Some(prefix) = f.strip_suffix('-') {
            property == prefix || property.starts_with(&f)
        } else {
            property == f
        }
    })
}

fn render_declarations(decls: &[crate::epub_css::CssDecl]) -> String {
    let mut out = String::new();
    for decl in decls {
        out.push_str(&decl.property);
        out.push_str(": ");
        out.push_str(&decl.value);
        if decl.important {
            out.push_str(" !important");
        }
        out.push_str("; ");
    }
    out.trim_end().to_string()
}

/// Drop declarations whose property is filtered from a
/// stylesheet. Rules left empty go entirely; rules the filter
/// never touches keep their exact text. Returns the new
/// stylesheet and how many declarations went.
pub fn filter_stylesheet(css: &str, filter: &[&str]) -> (String, usize) {
    if filter.is_empty() {
        return (css.to_string(), 0);
    }
    let rules = crate::epub_css::parse_rules(css);
    let mut edits: Vec<(usize, usize, String)> = Vec::new();
    let mut removed = 0usize;
    for rule in &rules {
        let kept: Vec<crate::epub_css::CssDecl> = rule
            .declarations
            .iter()
            .filter(|d| !filtered_property(&d.property, filter))
            .cloned()
            .collect();
        let gone = rule.declarations.len() - kept.len();
        if gone == 0 {
            continue;
        }
        removed += gone;
        let text = if kept.is_empty() {
            String::new()
        } else {
            format!(
                "{} {{ {} }}",
                rule.selectors.join(", "),
                render_declarations(&kept)
            )
        };
        edits.push((rule.span.0, rule.span.1, text));
    }
    if edits.is_empty() {
        return (css.to_string(), 0);
    }
    let mut out = String::with_capacity(css.len());
    let mut at = 0usize;
    for (start, end, text) in edits {
        if start >= at {
            out.push_str(&css[at..start]);
            out.push_str(&text);
            at = end;
        }
    }
    out.push_str(&css[at..]);
    (out, removed)
}

/// The inline half of the filter: `style="…"` attributes in a
/// document lose the filtered declarations, and an attribute
/// left with none is removed outright.
pub fn filter_inline_styles(html: &str, filter: &[&str]) -> (String, usize) {
    if filter.is_empty() {
        return (html.to_string(), 0);
    }
    let bytes = html.as_bytes();
    let mut out = String::with_capacity(html.len());
    let mut removed = 0usize;
    let mut pos = 0usize;
    let mut i = 0usize;
    while i + 5 < bytes.len() {
        let at_style = (i == 0 || bytes[i - 1].is_ascii_whitespace())
            && html[i..].to_ascii_lowercase().starts_with("style");
        if !at_style {
            i += 1;
            continue;
        }
        let mut j = i + 5;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        if bytes.get(j) != Some(&b'=') {
            i += 1;
            continue;
        }
        j += 1;
        while j < bytes.len() && bytes[j].is_ascii_whitespace() {
            j += 1;
        }
        let Some(q) = bytes.get(j).copied().filter(|b| *b == b'"' || *b == b'\'') else {
            i += 1;
            continue;
        };
        let value_start = j + 1;
        let mut value_end = value_start;
        while value_end < bytes.len() && bytes[value_end] != q {
            value_end += 1;
        }
        if value_end >= bytes.len() {
            break;
        }
        let decls = crate::epub_css::parse_declarations(&html[value_start..value_end]);
        let kept: Vec<crate::epub_css::CssDecl> = decls
            .iter()
            .filter(|d| !filtered_property(&d.property, filter))
            .cloned()
            .collect();
        let gone = decls.len() - kept.len();
        if gone > 0 {
            removed += gone;
            // Rewrite from the whitespace before `style` (pos
            // sits before it) so an emptied attribute takes its
            // separating space with it.
            let attr_start = i.saturating_sub(1);
            out.push_str(&html[pos..attr_start]);
            if !kept.is_empty() {
                out.push(' ');
                out.push_str(&html[i..value_start]);
                out.push_str(&render_declarations(&kept));
                out.push(q as char);
            }
            pos = value_end + 1;
        }
        i = value_end + 1;
    }
    out.push_str(&html[pos..]);
    (out, removed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smartens_quotes_dashes_and_ellipses_in_text_only() {
        let html = "<p class=\"x\">\"Don't,\" she said -- 'wait...' </p><code>\"raw\"</code>";
        let (out, count) = smarten_html(html);
        assert_eq!(
            out,
            "<p class=\"x\">“Don’t,” she said – ‘wait…’ </p><code>\"raw\"</code>"
        );
        assert!(count >= 6, "{count}");
    }

    #[test]
    fn reformats_markup_and_keeps_pre_verbatim() {
        let html = "<html><head><title>T</title></head><body><p>Hello <em>there</em> world</p><pre>  a\n b</pre></body></html>";
        let out = reformat(html, FileKind::Html).expect("reformat");
        assert!(out.contains("Hello <em>there</em> world"), "{out}");
        assert!(out.contains("<pre>  a\n b</pre>"), "{out}");
    }

    #[test]
    fn reformats_css_one_declaration_per_line() {
        let out = reformat("p{margin:0;color:red}.a  { x: 1; }", FileKind::Css).expect("css");
        assert_eq!(out, "p {\n  margin:0;\n  color:red;\n}\n.a {\n  x: 1;\n}\n");
        assert!(reformat("p {", FileKind::Css).is_none());
    }

    #[test]
    fn removes_dead_css_but_keeps_used_and_nested() {
        let chains = all_element_chains("<html><body><p class=\"a\">x</p></body></html>");
        let css = "p.a { color: red; }\n.gone { color: blue; }\n@media (max-width: 1em) {\n.gone2 { color: green; }\n}\n";
        let (out, removed) = remove_unused_css(css, &chains);
        assert_eq!(removed, 1);
        assert!(out.contains("p.a"));
        assert!(out.contains("gone2"), "nested rules stay: {out}");
        assert!(!out.contains(".gone {"));
    }

    #[test]
    fn searcher_literal_whole_word_and_regex_groups() {
        let s = Searcher::new("cat", SearchOptions::default()).expect("searcher");
        assert_eq!(s.find_all("Cat scatter cat").len(), 3);
        let s = Searcher::new(
            "cat",
            SearchOptions {
                whole_word: true,
                match_case: true,
                regex: false,
            },
        )
        .expect("searcher");
        assert_eq!(s.find_all("Cat scatter cat").len(), 1);
        let s = Searcher::new(
            r"(\w+)@(\w+)",
            SearchOptions {
                regex: true,
                ..Default::default()
            },
        )
        .expect("searcher");
        let (out, n) = s.replace_all("a@b!", "$2/$1");
        assert_eq!(out, "b/a!");
        assert_eq!(n, 1);
        // Literal mode never expands $.
        let s = Searcher::new("a", SearchOptions::default()).expect("searcher");
        assert_eq!(s.replace_all("a a", "$1").0, "$1 $1");
        assert!(Searcher::new(
            "(",
            SearchOptions {
                regex: true,
                ..Default::default()
            }
        )
        .is_err());
    }

    fn rule(from: &str, to: &str) -> TagRule {
        TagRule {
            from: from.to_string(),
            to: to.to_string(),
        }
    }

    #[test]
    fn retag_renames_pairs_and_counts_elements() {
        let (out, n) = retag(
            "<p><b>bold</b> and <b class=\"x\">more</b></p>",
            &[rule("b", "strong")],
        );
        assert_eq!(
            out,
            "<p><strong>bold</strong> and <strong class=\"x\">more</strong></p>"
        );
        assert_eq!(n, 2);
    }

    #[test]
    fn retag_class_rule_only_takes_its_class() {
        let (out, n) = retag(
            "<div class=\"poem\">a</div><div class=\"note\">b</div>",
            &[rule("div.poem", "blockquote")],
        );
        assert_eq!(
            out,
            "<blockquote class=\"poem\">a</blockquote><div class=\"note\">b</div>"
        );
        assert_eq!(n, 1);
    }

    #[test]
    fn retag_closing_follows_its_opener_through_nesting() {
        let (out, _) = retag(
            "<div class=\"poem\"><p>x</p></div><div>y</div>",
            &[rule("div.poem", "blockquote")],
        );
        assert_eq!(
            out,
            "<blockquote class=\"poem\"><p>x</p></blockquote><div>y</div>"
        );
    }

    #[test]
    fn filter_stylesheet_drops_properties_and_empty_rules() {
        let css = "p { color: red; margin: 0; }\nh1 { color: blue; }\nem { font-style: italic; }";
        let (out, n) = filter_stylesheet(css, &["color"]);
        assert_eq!(n, 2);
        assert!(out.contains("p { margin: 0; }"), "{out}");
        assert!(!out.contains("h1"), "{out}");
        assert!(out.contains("em { font-style: italic; }"), "{out}");
    }

    #[test]
    fn filter_inline_styles_rewrites_and_removes() {
        let html = "<p style=\"color: red; margin: 0\">a</p><p style=\"color: blue\">b</p>";
        let (out, n) = filter_inline_styles(html, &["color"]);
        assert_eq!(n, 2);
        assert_eq!(out, "<p style=\"margin: 0;\">a</p><p>b</p>");
    }

    #[test]
    fn filter_prefix_takes_the_whole_family() {
        let css = "p { margin-top: 1em; margin: 0; padding-left: 2em; }";
        let (out, n) = filter_stylesheet(css, &["margin-"]);
        assert_eq!(n, 2);
        assert!(out.contains("padding-left"), "{out}");
    }
}
