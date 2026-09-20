//! P4 — smart shelf rules.
//!
//! A smart shelf stores a small JSON document describing a **flat** list of
//! rules combined with a single `all` / `any` switch (roadmap decision B):
//!
//! ```json
//! {
//!   "match": "all",
//!   "rules": [
//!     { "field": "tag",      "op": "is",       "value": "fantasy" },
//!     { "field": "progress", "op": "is",       "value": "unread"  },
//!     { "field": "title",    "op": "contains", "value": "empire"  }
//!   ]
//! }
//! ```
//!
//! The document compiles to a SQL `WHERE` fragment over `books` plus a bag of
//! positional parameters. Keeping rules flat (no nested groups) means the whole
//! editor is a list of combo boxes, and the JSON stays forward compatible: a
//! future nested-group syntax can add a `"groups"` key without a migration.

// Module-wide because the rule vocabulary (every `RuleField`, `RuleOp` and
// their `label`/`value_hint` helpers) is deliberately complete, while the
// editor UI surfaces only part of it. Same caveat as `db.rs`: this hides
// future dead code too, so new items should carry their own narrow allow.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Fields
// ---------------------------------------------------------------------------

/// What a rule looks at. Stored as a lowercase string so unknown values coming
/// from a newer Kalam degrade to "ignore this rule" instead of corrupting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleField {
    Tag,
    Author,
    Series,
    Format,
    Progress,
    Title,
    Added,
}

impl RuleField {
    pub const ALL: &'static [RuleField] = &[
        RuleField::Tag,
        RuleField::Author,
        RuleField::Series,
        RuleField::Format,
        RuleField::Progress,
        RuleField::Title,
        RuleField::Added,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            RuleField::Tag => "tag",
            RuleField::Author => "author",
            RuleField::Series => "series",
            RuleField::Format => "format",
            RuleField::Progress => "progress",
            RuleField::Title => "title",
            RuleField::Added => "added",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RuleField::Tag => "Tag",
            RuleField::Author => "Author",
            RuleField::Series => "Series",
            RuleField::Format => "Format",
            RuleField::Progress => "Progress",
            RuleField::Title => "Title",
            RuleField::Added => "Added",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "tag" | "tags" => Some(RuleField::Tag),
            "author" | "authors" => Some(RuleField::Author),
            "series" => Some(RuleField::Series),
            "format" => Some(RuleField::Format),
            "progress" | "status" => Some(RuleField::Progress),
            "title" => Some(RuleField::Title),
            "added" | "added_at" => Some(RuleField::Added),
            _ => None,
        }
    }

    /// Operators that make sense for this field.
    pub fn operators(self) -> &'static [RuleOp] {
        match self {
            // Enumerated values: equality only.
            RuleField::Progress | RuleField::Format => &[RuleOp::Is, RuleOp::IsNot],
            // A relative window; only one sensible operator.
            RuleField::Added => &[RuleOp::WithinDays, RuleOp::BeforeDays],
            // Free text.
            _ => &[
                RuleOp::Is,
                RuleOp::IsNot,
                RuleOp::Contains,
                RuleOp::NotContains,
            ],
        }
    }

    /// Fixed choices for enum-like fields (used by the editor combo box).
    pub fn value_choices(self) -> Option<&'static [(&'static str, &'static str)]> {
        match self {
            RuleField::Progress => Some(&[
                ("unread", "Unread"),
                ("reading", "Reading"),
                ("finished", "Finished"),
            ]),
            RuleField::Format => Some(&[
                ("EPUB", "EPUB"),
                ("PDF", "PDF"),
                ("CBZ", "CBZ"),
                ("CBR", "CBR"),
                ("OTHER", "Other"),
            ]),
            _ => None,
        }
    }

    /// Placeholder text for free-text fields.
    pub fn value_hint(self) -> &'static str {
        match self {
            RuleField::Tag => "fantasy",
            RuleField::Author => "Ursula K. Le Guin",
            RuleField::Series => "Earthsea",
            RuleField::Title => "wizard",
            RuleField::Added => "30",
            _ => "",
        }
    }
}

// ---------------------------------------------------------------------------
// Operators
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RuleOp {
    Is,
    IsNot,
    Contains,
    NotContains,
    /// `added` within the last N days.
    WithinDays,
    /// `added` more than N days ago.
    BeforeDays,
}

impl RuleOp {
    pub fn as_str(self) -> &'static str {
        match self {
            RuleOp::Is => "is",
            RuleOp::IsNot => "is_not",
            RuleOp::Contains => "contains",
            RuleOp::NotContains => "not_contains",
            RuleOp::WithinDays => "within_days",
            RuleOp::BeforeDays => "before_days",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            RuleOp::Is => "is",
            RuleOp::IsNot => "is not",
            RuleOp::Contains => "contains",
            RuleOp::NotContains => "does not contain",
            RuleOp::WithinDays => "in the last (days)",
            RuleOp::BeforeDays => "more than (days) ago",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "is" | "=" | "eq" => Some(RuleOp::Is),
            "is_not" | "isnot" | "!=" | "ne" => Some(RuleOp::IsNot),
            "contains" | "like" => Some(RuleOp::Contains),
            "not_contains" | "notcontains" => Some(RuleOp::NotContains),
            "within_days" | "within" => Some(RuleOp::WithinDays),
            "before_days" | "before" => Some(RuleOp::BeforeDays),
            _ => None,
        }
    }

    fn negated(self) -> bool {
        matches!(self, RuleOp::IsNot | RuleOp::NotContains)
    }
}

// ---------------------------------------------------------------------------
// Documents
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MatchMode {
    All,
    Any,
}

impl MatchMode {
    pub fn as_str(self) -> &'static str {
        match self {
            MatchMode::All => "all",
            MatchMode::Any => "any",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            MatchMode::All => "All",
            MatchMode::Any => "Any",
        }
    }

    pub fn parse(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "any" | "or" => MatchMode::Any,
            _ => MatchMode::All,
        }
    }

    fn sql_join(self) -> &'static str {
        match self {
            MatchMode::All => " AND ",
            MatchMode::Any => " OR ",
        }
    }
}

/// One rule row as stored on disk. Strings keep the format tolerant.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    pub field: String,
    pub op: String,
    #[serde(default)]
    pub value: String,
}

impl Rule {
    pub fn new(field: RuleField, op: RuleOp, value: impl Into<String>) -> Self {
        Self {
            field: field.as_str().to_string(),
            op: op.as_str().to_string(),
            value: value.into(),
        }
    }

    pub fn field_enum(&self) -> Option<RuleField> {
        RuleField::parse(&self.field)
    }

    pub fn op_enum(&self) -> Option<RuleOp> {
        RuleOp::parse(&self.op)
    }

    /// Human-readable summary, e.g. `Tag is fantasy`.
    pub fn describe(&self) -> String {
        let field = self
            .field_enum()
            .map(|f| f.label().to_string())
            .unwrap_or_else(|| self.field.clone());
        let op = self
            .op_enum()
            .map(|o| o.label().to_string())
            .unwrap_or_else(|| self.op.clone());
        let value = if self.value.trim().is_empty() {
            "…".to_string()
        } else {
            self.value.trim().to_string()
        };
        format!("{field} {op} {value}")
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSet {
    /// `all` or `any`.
    #[serde(default = "default_match", rename = "match")]
    pub match_mode: String,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

fn default_match() -> String {
    "all".to_string()
}

impl Default for RuleSet {
    fn default() -> Self {
        Self {
            match_mode: default_match(),
            rules: Vec::new(),
        }
    }
}

impl RuleSet {
    pub fn mode(&self) -> MatchMode {
        MatchMode::parse(&self.match_mode)
    }

    pub fn set_mode(&mut self, mode: MatchMode) {
        self.match_mode = mode.as_str().to_string();
    }

    /// Parse stored JSON; anything unreadable becomes an empty rule set so a
    /// corrupt shelf shows "no rules" instead of crashing the grid.
    pub fn parse(json: &str) -> Self {
        let trimmed = json.trim();
        if trimmed.is_empty() {
            return Self::default();
        }
        serde_json::from_str(trimmed).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{\"match\":\"all\",\"rules\":[]}".into())
    }

    /// Rules that can actually be compiled (known field/op and a usable value).
    pub fn valid_rules(&self) -> Vec<(RuleField, RuleOp, String)> {
        let mut out = Vec::new();
        for rule in &self.rules {
            let (Some(field), Some(op)) = (rule.field_enum(), rule.op_enum()) else {
                continue;
            };
            let value = rule.value.trim();
            if value.is_empty() {
                continue;
            }
            // Day-window rules need a number.
            if matches!(op, RuleOp::WithinDays | RuleOp::BeforeDays)
                && value.parse::<i64>().is_err()
            {
                continue;
            }
            out.push((field, op, value.to_string()));
        }
        out
    }

    pub fn is_empty(&self) -> bool {
        self.valid_rules().is_empty()
    }

    /// One-line summary for shelf cards.
    pub fn describe(&self) -> String {
        let parts: Vec<String> = self.rules.iter().map(|r| r.describe()).collect();
        if parts.is_empty() {
            return "No rules yet — matches nothing".into();
        }
        let joiner = match self.mode() {
            MatchMode::All => " and ",
            MatchMode::Any => " or ",
        };
        parts.join(joiner)
    }

    /// Compile to `(where_sql, params)`.
    ///
    /// The fragment references the `books` table by name, so callers must query
    /// `FROM books` (optionally aliased *as* `books`). When no rule is usable
    /// the result is `0 = 1` — an empty smart shelf matches nothing, which is
    /// less surprising than matching the entire library.
    pub fn to_sql(&self) -> (String, Vec<String>) {
        let rules = self.valid_rules();
        if rules.is_empty() {
            return ("0 = 1".to_string(), Vec::new());
        }

        let mut clauses = Vec::with_capacity(rules.len());
        let mut params = Vec::new();

        for (field, op, value) in rules {
            let (clause, mut rule_params) = compile_rule(field, op, &value);
            clauses.push(format!("({clause})"));
            params.append(&mut rule_params);
        }

        (clauses.join(self.mode().sql_join()), params)
    }
}

// ---------------------------------------------------------------------------
// Compilation
// ---------------------------------------------------------------------------

fn compile_rule(field: RuleField, op: RuleOp, value: &str) -> (String, Vec<String>) {
    match field {
        RuleField::Tag => compile_tag(op, value),
        RuleField::Author => compile_text("books.authors", op, value),
        RuleField::Series => compile_text("IFNULL(books.series, '')", op, value),
        RuleField::Title => compile_text("books.title", op, value),
        RuleField::Format => compile_format(op, value),
        RuleField::Progress => compile_progress(op, value),
        RuleField::Added => compile_added(op, value),
    }
}

fn compile_tag(op: RuleOp, value: &str) -> (String, Vec<String>) {
    let (predicate, param) = match op {
        RuleOp::Contains | RuleOp::NotContains => (
            "t.name LIKE ?".to_string(),
            format!("%{}%", escape_like(value)),
        ),
        _ => ("t.name = ?".to_string(), value.to_string()),
    };

    let exists = format!(
        "EXISTS (SELECT 1 FROM book_tags bt \
         JOIN tags t ON t.id = bt.tag_id \
         WHERE bt.book_id = books.id AND {predicate} {escape})",
        escape = like_escape_suffix(op),
    );

    let clause = if op.negated() {
        format!("NOT {exists}")
    } else {
        exists
    };
    (clause, vec![param])
}

fn compile_text(column: &str, op: RuleOp, value: &str) -> (String, Vec<String>) {
    match op {
        RuleOp::Is => (
            format!("{column} = ? COLLATE NOCASE"),
            vec![value.to_string()],
        ),
        RuleOp::IsNot => (
            format!("{column} <> ? COLLATE NOCASE"),
            vec![value.to_string()],
        ),
        RuleOp::Contains => (
            format!("{column} LIKE ? ESCAPE '\\'"),
            vec![format!("%{}%", escape_like(value))],
        ),
        RuleOp::NotContains => (
            format!("{column} NOT LIKE ? ESCAPE '\\'"),
            vec![format!("%{}%", escape_like(value))],
        ),
        // Day windows are meaningless on text: never match.
        RuleOp::WithinDays | RuleOp::BeforeDays => ("0 = 1".to_string(), Vec::new()),
    }
}

fn compile_format(op: RuleOp, value: &str) -> (String, Vec<String>) {
    let upper = value.trim().to_ascii_uppercase();
    let cmp = if op.negated() { "<>" } else { "=" };
    (format!("books.format {cmp} ?"), vec![upper])
}

fn compile_progress(op: RuleOp, value: &str) -> (String, Vec<String>) {
    let inner = match value.trim().to_ascii_lowercase().as_str() {
        "unread" | "new" => "books.progress <= 0",
        "reading" | "in_progress" => "books.progress > 0 AND books.progress < 100",
        "finished" | "done" | "read" => "books.progress >= 100",
        _ => return ("0 = 1".to_string(), Vec::new()),
    };
    let clause = if op.negated() {
        format!("NOT ({inner})")
    } else {
        inner.to_string()
    };
    (clause, Vec::new())
}

fn compile_added(op: RuleOp, value: &str) -> (String, Vec<String>) {
    let days: i64 = match value.trim().parse() {
        Ok(d) => d,
        Err(_) => return ("0 = 1".to_string(), Vec::new()),
    };
    let cutoff = crate::db::iso_days_ago(days.max(0));
    match op {
        RuleOp::BeforeDays => ("books.added_at < ?".to_string(), vec![cutoff]),
        // WithinDays and anything else read as "recent".
        _ => ("books.added_at >= ?".to_string(), vec![cutoff]),
    }
}

/// `ESCAPE` clause, only emitted for LIKE comparisons.
fn like_escape_suffix(op: RuleOp) -> &'static str {
    match op {
        RuleOp::Contains | RuleOp::NotContains => "ESCAPE '\\'",
        _ => "",
    }
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_ruleset_matches_nothing() {
        let set = RuleSet::default();
        let (sql, params) = set.to_sql();
        assert_eq!(sql, "0 = 1");
        assert!(params.is_empty());
    }

    #[test]
    fn round_trips_through_json() {
        let mut set = RuleSet::default();
        set.set_mode(MatchMode::Any);
        set.rules
            .push(Rule::new(RuleField::Tag, RuleOp::Is, "fantasy"));
        let json = set.to_json();
        let back = RuleSet::parse(&json);
        assert_eq!(back.mode(), MatchMode::Any);
        assert_eq!(back.rules.len(), 1);
        assert_eq!(back.rules[0].value, "fantasy");
    }

    #[test]
    fn garbage_json_is_empty_not_panic() {
        let set = RuleSet::parse("{not json");
        assert!(set.is_empty());
    }

    #[test]
    fn incomplete_rules_are_skipped() {
        let mut set = RuleSet::default();
        set.rules.push(Rule::new(RuleField::Tag, RuleOp::Is, ""));
        set.rules
            .push(Rule::new(RuleField::Added, RuleOp::WithinDays, "abc"));
        assert!(set.is_empty());
    }

    #[test]
    fn combines_with_and_or() {
        let mut set = RuleSet::default();
        set.rules
            .push(Rule::new(RuleField::Tag, RuleOp::Is, "fantasy"));
        set.rules
            .push(Rule::new(RuleField::Progress, RuleOp::Is, "unread"));

        let (sql_all, params_all) = set.to_sql();
        assert!(sql_all.contains(" AND "));
        assert_eq!(params_all.len(), 1);

        set.set_mode(MatchMode::Any);
        let (sql_any, _) = set.to_sql();
        assert!(sql_any.contains(" OR "));
    }

    #[test]
    fn negation_wraps_tag_exists() {
        let mut set = RuleSet::default();
        set.rules
            .push(Rule::new(RuleField::Tag, RuleOp::IsNot, "manga"));
        let (sql, _) = set.to_sql();
        assert!(sql.contains("NOT EXISTS"));
    }

    #[test]
    fn like_values_are_escaped() {
        let mut set = RuleSet::default();
        set.rules
            .push(Rule::new(RuleField::Title, RuleOp::Contains, "100%_sure"));
        let (_, params) = set.to_sql();
        assert_eq!(params[0], "%100\\%\\_sure%");
    }
}
