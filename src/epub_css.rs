//! A small CSS reader for the editor's Live CSS panel (decision
//! 133): parse a stylesheet into rules with line numbers, then ask
//! which of them match an element (described by its ancestor chain).
//!
//! Deliberately a subset — element, class, id and attribute
//! selectors with descendant/child combinators cover what book
//! stylesheets actually write. Pseudo-classes are ignored for
//! matching (a `:first-child` rule still "applies") but counted for
//! specificity. This is a reading tool, not a cascade engine; where
//! it is unsure it shows the rule rather than hiding it.

#[derive(Debug, Clone)]
pub struct CssDecl {
    pub property: String,
    pub value: String,
    pub important: bool,
}

#[derive(Debug, Clone)]
pub struct CssRule {
    /// The selector text as written (one entry per comma part).
    pub selectors: Vec<String>,
    pub declarations: Vec<CssDecl>,
    /// 1-based line of the selector in its source text.
    pub line: usize,
}

/// One element in the ancestor chain (root first, element last).
#[derive(Debug, Clone, Default)]
pub struct Elem {
    pub tag: String,
    pub id: Option<String>,
    pub classes: Vec<String>,
    /// Lowercased attribute names to values (`epub:type` kept whole).
    pub attrs: Vec<(String, String)>,
}

impl Elem {
    pub fn attr(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.attrs
            .iter()
            .find(|(k, _)| *k == name)
            .map(|(_, v)| v.as_str())
    }

    /// `p.chapter-title#top` — the panel's element summary.
    pub fn describe(&self) -> String {
        let mut out = self.tag.clone();
        if let Some(id) = &self.id {
            out.push('#');
            out.push_str(id);
        }
        for class in &self.classes {
            out.push('.');
            out.push_str(class);
        }
        out
    }
}

// ── parsing ────────────────────────────────────────────────────────

pub fn parse_rules(css: &str) -> Vec<CssRule> {
    let cleaned = strip_comments(css);
    let mut rules = Vec::new();
    parse_block(&cleaned, 0, &mut rules);
    rules
}

/// Replace comments with spaces, preserving newlines so line numbers
/// still refer to the original text.
fn strip_comments(css: &str) -> String {
    let mut out = String::with_capacity(css.len());
    let mut chars = css.chars().peekable();
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
                out.push(' ');
                let mut depth = 1;
                while depth > 0 {
                    match chars.next() {
                        Some('*') if chars.peek() == Some(&'/') => {
                            chars.next();
                            out.push(' ');
                            out.push(' ');
                            depth -= 1;
                        }
                        Some('\n') => out.push('\n'),
                        Some(_) => out.push(' '),
                        None => break,
                    }
                }
            }
            _ => out.push(c),
        }
    }
    out
}

fn parse_block(text: &str, base_line: usize, rules: &mut Vec<CssRule>) {
    let bytes = text.as_bytes();
    let mut i = 0usize;
    while i < bytes.len() {
        // Prelude: up to the next { or ; at paren depth 0.
        let prelude_start = i;
        let mut depth_paren = 0i32;
        let mut in_str = 0u8;
        while i < bytes.len() {
            let b = bytes[i];
            if in_str != 0 {
                if b == in_str {
                    in_str = 0;
                }
            } else if b == b'"' || b == b'\'' {
                in_str = b;
            } else if b == b'(' {
                depth_paren += 1;
            } else if b == b')' {
                depth_paren -= 1;
            } else if depth_paren == 0 && (b == b'{' || b == b';' || b == b'}') {
                break;
            }
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }
        let terminator = bytes[i];
        let prelude = text[prelude_start..i].trim();
        if terminator == b';' || terminator == b'}' {
            i += 1;
            continue;
        }
        // Block body to the matching }.
        let body_start = i + 1;
        let mut j = body_start;
        let mut depth = 1i32;
        let mut in_str = 0u8;
        while j < bytes.len() && depth > 0 {
            let b = bytes[j];
            if in_str != 0 {
                if b == in_str {
                    in_str = 0;
                }
            } else if b == b'"' || b == b'\'' {
                in_str = b;
            } else if b == b'{' {
                depth += 1;
            } else if b == b'}' {
                depth -= 1;
            }
            j += 1;
        }
        let body = &text[body_start..(j - 1).min(text.len())];
        let first = prelude_start + (text[prelude_start..i].len() - text[prelude_start..i].trim_start().len());
        let line = base_line + line_of(text, first);

        if prelude.starts_with('@') {
            let name: String = prelude
                .chars()
                .skip(1)
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                .collect::<String>()
                .to_ascii_lowercase();
            if matches!(name.as_str(), "media" | "supports" | "layer" | "document") {
                // Conditional group: the conditions decide at read
                // time; the panel shows the rules regardless.
                parse_block(body, line, rules);
            }
            // @font-face/@page/@keyframes hold no element rules.
        } else if !prelude.is_empty() {
            let selectors: Vec<String> = split_top(prelude, ',')
                .into_iter()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            let declarations = parse_declarations(body);
            if !selectors.is_empty() && !declarations.is_empty() {
                rules.push(CssRule {
                    selectors,
                    declarations,
                    line,
                });
            }
        }
        i = j;
    }
}

pub fn parse_declarations(body: &str) -> Vec<CssDecl> {
    let mut out = Vec::new();
    for part in split_top(body, ';') {
        let part = part.trim();
        let Some(colon) = part.find(':') else { continue };
        let property = part[..colon].trim().to_ascii_lowercase();
        let mut value = part[colon + 1..].trim().to_string();
        let important = value.to_ascii_lowercase().ends_with("!important");
        if important {
            value = value[..value.len() - "!important".len()].trim().to_string();
        }
        if !property.is_empty() && !value.is_empty() {
            out.push(CssDecl {
                property,
                value,
                important,
            });
        }
    }
    out
}

/// Split on `sep` outside of (), [] and quotes.
fn split_top(text: &str, sep: char) -> Vec<String> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut depth_paren = 0i32;
    let mut depth_bracket = 0i32;
    let mut in_str: Option<char> = None;
    for c in text.chars() {
        if let Some(q) = in_str {
            current.push(c);
            if c == q {
                in_str = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => {
                in_str = Some(c);
                current.push(c);
            }
            '(' => {
                depth_paren += 1;
                current.push(c);
            }
            ')' => {
                depth_paren -= 1;
                current.push(c);
            }
            '[' => {
                depth_bracket += 1;
                current.push(c);
            }
            ']' => {
                depth_bracket -= 1;
                current.push(c);
            }
            _ if c == sep && depth_paren == 0 && depth_bracket == 0 => {
                parts.push(std::mem::take(&mut current));
            }
            _ => current.push(c),
        }
    }
    if !current.is_empty() {
        parts.push(current);
    }
    parts
}

// ── matching ───────────────────────────────────────────────────────

/// Specificity (ids, classes, elements) if `selector` matches the
/// last element of `chain` (root first).
pub fn selector_matches(selector: &str, chain: &[Elem]) -> Option<(u32, u32, u32)> {
    if chain.is_empty() {
        return None;
    }
    let parts = split_complex(selector);
    if parts.is_empty() {
        return None;
    }
    let mut spec = (0u32, 0u32, 0u32);
    for (compound, _) in &parts {
        let s = compound_specificity(compound);
        spec.0 += s.0;
        spec.1 += s.1;
        spec.2 += s.2;
    }
    // Rightmost compound matches the element; walk left.
    let mut idx = chain.len() - 1;
    if !compound_matches(&parts[parts.len() - 1].0, &chain[idx]) {
        return None;
    }
    let mut part = parts.len() - 1;
    while part > 0 {
        let combinator = parts[part].1;
        let want = &parts[part - 1].0;
        match combinator {
            '>' => {
                if idx == 0 {
                    return None;
                }
                idx -= 1;
                if !compound_matches(want, &chain[idx]) {
                    return None;
                }
            }
            _ => {
                // Descendant (and anything fancier, treated loosely):
                // some ancestor must match.
                let mut found = false;
                while idx > 0 {
                    idx -= 1;
                    if compound_matches(want, &chain[idx]) {
                        found = true;
                        break;
                    }
                }
                if !found {
                    return None;
                }
            }
        }
        part -= 1;
    }
    Some(spec)
}

/// (compound, combinator-before-it). The first compound's combinator
/// is ' ' and unused.
fn split_complex(selector: &str) -> Vec<(String, char)> {
    let mut parts: Vec<(String, char)> = Vec::new();
    let mut current = String::new();
    let mut combinator = ' ';
    let mut depth_bracket = 0i32;
    let mut depth_paren = 0i32;
    let mut in_str: Option<char> = None;
    let mut chars = selector.chars().peekable();
    while let Some(c) = chars.next() {
        if let Some(q) = in_str {
            current.push(c);
            if c == q {
                in_str = None;
            }
            continue;
        }
        match c {
            '"' | '\'' => {
                in_str = Some(c);
                current.push(c);
            }
            '[' => {
                depth_bracket += 1;
                current.push(c);
            }
            ']' => {
                depth_bracket -= 1;
                current.push(c);
            }
            '(' => {
                depth_paren += 1;
                current.push(c);
            }
            ')' => {
                depth_paren -= 1;
                current.push(c);
            }
            '>' | '+' | '~' if depth_bracket == 0 && depth_paren == 0 => {
                let done = current.trim().to_string();
                if !done.is_empty() {
                    parts.push((done, combinator));
                    current = String::new();
                }
                combinator = c;
            }
            c if c.is_whitespace() && depth_bracket == 0 && depth_paren == 0 => {
                let done = current.trim().to_string();
                if !done.is_empty() {
                    parts.push((done, combinator));
                    current = String::new();
                    combinator = ' ';
                }
                // Swallow the run.
                while matches!(chars.peek(), Some(c) if c.is_whitespace()) {
                    chars.next();
                }
            }
            _ => current.push(c),
        }
    }
    let done = current.trim().to_string();
    if !done.is_empty() {
        parts.push((done, combinator));
    }
    parts
}

fn compound_specificity(compound: &str) -> (u32, u32, u32) {
    let mut spec = (0, 0, 0);
    for part in compound_parts(compound) {
        match part {
            Part::Id => spec.0 += 1,
            Part::Class | Part::Attr | Part::PseudoClass => spec.1 += 1,
            Part::Tag | Part::PseudoElement => spec.2 += 1,
            Part::Universal => {}
        }
    }
    spec
}

enum Part {
    Universal,
    Tag,
    Id,
    Class,
    Attr,
    PseudoClass,
    PseudoElement,
}

/// Break a compound selector (`p.note#x[epub|type~="toc"]:hover`)
/// into checkable pieces. Text forms are kept where matching needs
/// them.
fn compound_parts(compound: &str) -> Vec<Part> {
    let mut parts = Vec::new();
    let bytes = compound.as_bytes();
    let mut i = 0usize;
    let read_name = |from: usize| -> usize {
        let mut j = from;
        while j < bytes.len()
            && (bytes[j].is_ascii_alphanumeric() || matches!(bytes[j], b'-' | b'_' | b'|' | b'\\'))
        {
            j += 1;
        }
        j
    };
    // Leading tag or *.
    if i < bytes.len() && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'*') {
        if bytes[i] == b'*' {
            // `*` or `ns|*`.
            parts.push(Part::Universal);
            i += 1;
        } else {
            let j = read_name(i);
            let name = &compound[i..j];
            let tag = name.rsplit('|').next().unwrap_or(name);
            if tag == "*" {
                parts.push(Part::Universal);
            } else {
                parts.push(Part::Tag);
            }
            i = j;
        }
    }
    while i < bytes.len() {
        match bytes[i] {
            b'#' => {
                i = read_name(i + 1);
                parts.push(Part::Id);
            }
            b'.' => {
                i = read_name(i + 1);
                parts.push(Part::Class);
            }
            b'[' => {
                let end = compound[i..].find(']').map(|n| i + n).unwrap_or(bytes.len() - 1);
                parts.push(Part::Attr);
                i = end + 1;
            }
            b':' => {
                let pseudo_element = i + 1 < bytes.len() && bytes[i + 1] == b':';
                let start = if pseudo_element { i + 2 } else { i + 1 };
                let mut j = read_name(start);
                // Skip functional arguments.
                if j < bytes.len() && bytes[j] == b'(' {
                    let mut depth = 0i32;
                    while j < bytes.len() {
                        if bytes[j] == b'(' {
                            depth += 1;
                        } else if bytes[j] == b')' {
                            depth -= 1;
                            if depth == 0 {
                                j += 1;
                                break;
                            }
                        }
                        j += 1;
                    }
                }
                parts.push(if pseudo_element {
                    Part::PseudoElement
                } else {
                    Part::PseudoClass
                });
                i = j;
            }
            _ => i += 1,
        }
    }
    parts
}

fn compound_matches(compound: &str, elem: &Elem) -> bool {
    // Re-scan with values (compound_parts drops them for specificity).
    let bytes = compound.as_bytes();
    let mut i = 0usize;
    let read_name = |from: usize| -> usize {
        let mut j = from;
        while j < bytes.len()
            && (bytes[j].is_ascii_alphanumeric()
                || matches!(bytes[j], b'-' | b'_' | b'|' | b'\\' | b'%'))
        {
            j += 1;
        }
        j
    };
    if i < bytes.len() && (bytes[i].is_ascii_alphabetic() || bytes[i] == b'*') {
        if bytes[i] == b'*' {
            i += 1;
        } else {
            let j = read_name(i);
            let name = &compound[i..j];
            let tag = name.rsplit('|').next().unwrap_or(name);
            if tag != "*" && !elem.tag.eq_ignore_ascii_case(tag) {
                return false;
            }
            i = j;
        }
    }
    while i < bytes.len() {
        match bytes[i] {
            b'#' => {
                let j = read_name(i + 1);
                let want = &compound[i + 1..j];
                if elem.id.as_deref() != Some(want) {
                    return false;
                }
                i = j;
            }
            b'.' => {
                let j = read_name(i + 1);
                let want = &compound[i + 1..j];
                if !elem.classes.iter().any(|c| c == want) {
                    return false;
                }
                i = j;
            }
            b'[' => {
                let end = compound[i..].find(']').map(|n| i + n).unwrap_or(bytes.len() - 1);
                if !attr_matches(&compound[i + 1..end], elem) {
                    return false;
                }
                i = end + 1;
            }
            b':' => {
                // Pseudo anything: skip (name + optional args).
                let start = if i + 1 < bytes.len() && bytes[i + 1] == b':' {
                    i + 2
                } else {
                    i + 1
                };
                let mut j = read_name(start);
                if j < bytes.len() && bytes[j] == b'(' {
                    let mut depth = 0i32;
                    while j < bytes.len() {
                        if bytes[j] == b'(' {
                            depth += 1;
                        } else if bytes[j] == b')' {
                            depth -= 1;
                            if depth == 0 {
                                j += 1;
                                break;
                            }
                        }
                        j += 1;
                    }
                }
                i = j;
            }
            _ => i += 1,
        }
    }
    true
}

/// `[name]`, `[name="v"]`, `[name~="v"]`, `[name|="v"]`,
/// `[name^="v"]`, `[name$="v"]`, `[name*="v"]`. A namespaced name
/// (`epub|type`) matches the attribute as written in the source
/// (`epub:type`) or bare (`type`).
fn attr_matches(inside: &str, elem: &Elem) -> bool {
    let inside = inside.trim();
    let mut op = "";
    let mut at = inside.len();
    for candidate in ["~=", "|=", "^=", "$=", "*=", "="] {
        if let Some(pos) = inside.find(candidate) {
            op = candidate;
            at = pos;
            break;
        }
    }
    let name = inside[..at].trim();
    let value = if op.is_empty() {
        ""
    } else {
        let raw = inside[at + op.len()..].trim();
        let raw = raw
            .strip_suffix(" i")
            .or_else(|| raw.strip_suffix(" s"))
            .unwrap_or(raw)
            .trim();
        raw.trim_matches(['"', '\''])
    };
    let mut candidates: Vec<String> = vec![name.to_string()];
    if let Some(bar) = name.find('|') {
        candidates.push(name.replace('|', ":"));
        candidates.push(name[bar + 1..].to_string());
    }
    for candidate in &candidates {
        let Some(actual) = elem.attr(candidate) else {
            continue;
        };
        let ok = match op {
            "" => true,
            "=" => actual == value,
            "~=" => actual.split_whitespace().any(|w| w == value),
            "|=" => actual == value || actual.starts_with(&format!("{value}-")),
            "^=" => actual.starts_with(value),
            "$=" => actual.ends_with(value),
            "*=" => actual.contains(value),
            _ => false,
        };
        if ok {
            return true;
        }
    }
    false
}

fn line_of(text: &str, byte: usize) -> usize {
    text[..byte.min(text.len())].bytes().filter(|b| *b == b'\n').count() + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn elem(tag: &str, id: Option<&str>, classes: &[&str]) -> Elem {
        Elem {
            tag: tag.to_string(),
            id: id.map(str::to_string),
            classes: classes.iter().map(|s| s.to_string()).collect(),
            attrs: Vec::new(),
        }
    }

    #[test]
    fn parses_rules_with_lines_inside_media() {
        let css = "p { margin: 0; }\n@media (max-width: 30em) {\n  .note { color: red; }\n}\n@font-face { font-family: X; src: url(x.woff); }\n";
        let rules = parse_rules(css);
        assert_eq!(rules.len(), 2);
        assert_eq!(rules[0].selectors, vec!["p".to_string()]);
        assert_eq!(rules[0].line, 1);
        assert_eq!(rules[1].selectors, vec![".note".to_string()]);
        assert_eq!(rules[1].line, 3);
    }

    #[test]
    fn matches_descendant_child_and_attrs() {
        let mut p = elem("p", None, &["note"]);
        p.attrs.push(("epub:type".to_string(), "footnote".to_string()));
        let chain = vec![elem("body", None, &[]), elem("section", Some("s1"), &[]), p];
        assert!(selector_matches("p.note", &chain).is_some());
        assert!(selector_matches("body p", &chain).is_some());
        assert!(selector_matches("section > p", &chain).is_some());
        assert!(selector_matches("body > p", &chain).is_none());
        assert!(selector_matches("#s1 .note", &chain).is_some());
        assert!(selector_matches("[epub|type~=\"footnote\"]", &chain).is_some());
        assert!(selector_matches("div p", &chain).is_none());
        let spec = selector_matches("#s1 p.note", &chain).expect("spec");
        assert_eq!(spec, (1, 1, 1));
    }
}
