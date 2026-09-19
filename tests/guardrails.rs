//! App-level guardrails, written as **ratchets**.
//!
//! `crates/kalam-reader` inherits strict boundaries from upstream, which is a
//! large part of why that code stayed clean. `src/` had no equivalent, and the
//! rules that did exist lived in `docs/WORKING.md` as prose. Prose does not
//! constrain anyone — least of all an agent. The engine's own archived
//! `RESTRICTIONS.md` is the proof: it banned `stylo` while the workspace
//! pinned five stylo crates, and mandated `lol_html` as the CSS sanitizer when
//! no such dependency existed anywhere. Two of its four rules were false and
//! nothing noticed, because nothing checked.
//!
//! So these are tests, and they are ratchets rather than ideals:
//!
//! 1. Count today's violations.
//! 2. Assert the count is **no greater than** that number.
//! 3. Fix some, then lower the constant.
//!
//! The number can never go up. That is the whole mechanism, and it is what
//! makes a rule practical in a codebase where most of the code is written by
//! an agent and most of the rules are already broken in a few places.
//!
//! Deliberately crude: the shapes being looked for are string literals, and a
//! syntax tree would buy nothing. Same reasoning as
//! `crates/chapbook-core/tests/fixture_discipline.rs`.

use std::path::{Path, PathBuf};

/// Production `.unwrap()` and `.expect()` calls across `src/`.
///
/// `docs/WORKING.md` §1 states this as zero. It is not zero, and a test that
/// asserted zero would fail on the day it was written and then be deleted. A
/// ratchet at the real number does the useful job: it stops the count growing,
/// and every fix lowers it. Where the 12 were, as of 2026-09-19: `perf.rs` 4
/// (a headless measurement harness with no production callers), `db/
/// dictionaries.rs` 3, `timing.rs` 2, `pages/browse.rs` 1,
/// `pages/shelf_editor.rs` 1, `service.rs` 1.
const MAX_PROD_PANICS: usize = 12;

/// Occurrences of `Arc<Catalog>` in `src/pages/`.
///
/// The rule behind it: pages ask `LibraryService`, they do not hold the
/// database handle. That is what made moving queries off the UI thread a
/// change in one place (roadmap 1.2b) rather than a sweep through every page.
/// 58 across 22 files today — this counts occurrences, not files, so a page
/// that stops holding a handle lowers it by however many it mentioned.
const MAX_ARC_CATALOG_IN_PAGES: usize = 58;

/// Literal hex colours in `resources/style.css`.
///
/// Colours are meant to live in `theme.rs`, with the stylesheet holding shape
/// only, so that a theme change is one file. The stylesheet has 184 literal
/// colours today and `theme.rs` has 169, which means the two duplicate each
/// other and neither is the single source. Ratcheting at 184 does not fix
/// that; it stops it getting worse while the duplication is worked down.
const MAX_HEX_IN_CSS: usize = 184;

// ---------------------------------------------------------------------------
// The checks
// ---------------------------------------------------------------------------

#[test]
fn production_code_panics_do_not_grow() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut per_file: Vec<(String, usize)> = Vec::new();

    for (path, text) in sources_under(&root) {
        let n = count_prod_panics(&text);
        if n > 0 {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();
            per_file.push((format!("src/{rel}"), n));
        }
    }

    per_file.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let total: usize = per_file.iter().map(|(_, n)| n).sum();

    assert!(
        total <= MAX_PROD_PANICS,
        "\n\
         Production `.unwrap()`/`.expect()` grew from {MAX_PROD_PANICS} to {total}.\n\
         \n\
         WORKING.md §1 forbids them outright: an unhandled panic crashes the\n\
         app and can corrupt data mid-write. Handle the error with `?`, a\n\
         `match`, or `unwrap_or`.\n\
         \n\
         If you genuinely removed some and the count is now lower, lower\n\
         MAX_PROD_PANICS in tests/guardrails.rs — that is the ratchet, and\n\
         leaving it loose wastes the work.\n\
         \n\
         Per file:\n{}\
         ",
        per_file
            .iter()
            .map(|(f, n)| format!("    {n:>3}  {f}\n"))
            .collect::<String>()
    );
}

#[test]
fn pages_do_not_gain_catalog_handles() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/pages");
    let mut per_file: Vec<(String, usize)> = Vec::new();

    for (path, text) in sources_under(&root) {
        let n = text.matches("Arc<Catalog>").count();
        if n > 0 {
            let rel = path
                .strip_prefix(&root)
                .unwrap_or(&path)
                .display()
                .to_string();
            per_file.push((format!("src/pages/{rel}"), n));
        }
    }

    per_file.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    let total: usize = per_file.iter().map(|(_, n)| n).sum();

    assert!(
        total <= MAX_ARC_CATALOG_IN_PAGES,
        "\n\
         `Arc<Catalog>` in src/pages/ grew from {MAX_ARC_CATALOG_IN_PAGES} to {total}.\n\
         \n\
         A page should ask `LibraryService` for a snapshot rather than hold\n\
         the database handle. That separation is what let roadmap 1.2b move\n\
         queries onto worker threads in one place; a page holding `Catalog`\n\
         can query on the UI thread and nothing stops it.\n\
         \n\
         New pages take a `LibraryService`. Lower MAX_ARC_CATALOG_IN_PAGES\n\
         when you convert one.\n\
         \n\
         Per file:\n{}\
         ",
        per_file
            .iter()
            .map(|(f, n)| format!("    {n:>3}  {f}\n"))
            .collect::<String>()
    );
}

#[test]
fn stylesheet_does_not_gain_hex_colours() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("resources/style.css");
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()));
    let total = count_hex_literals(&text);

    assert!(
        total <= MAX_HEX_IN_CSS,
        "\n\
         Literal hex colours in resources/style.css grew from \
         {MAX_HEX_IN_CSS} to {total}.\n\
         \n\
         Colours belong in theme.rs so a theme change is one file. Use a CSS\n\
         variable the theme sets (@define-color / var(--…)) instead of a hex\n\
         literal.\n\
         \n\
         Lower MAX_HEX_IN_CSS when you move one.\n\
         "
    );
}

// ---------------------------------------------------------------------------
// Scanners
// ---------------------------------------------------------------------------

/// Every `.rs` file under `dir`, with its contents.
fn sources_under(dir: &Path) -> Vec<(PathBuf, String)> {
    let mut out = Vec::new();
    fn walk(dir: &Path, out: &mut Vec<(PathBuf, String)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, out);
            } else if path.extension().is_some_and(|e| e == "rs") {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    out.push((path, text));
                }
            }
        }
    }
    walk(dir, &mut out);
    out
}

/// Count `.unwrap()` and `.expect(` that are **not** inside a test item.
///
/// Tracking the attribute is not enough on its own: `#[cfg(test)]` is applied
/// to whole `mod tests` blocks in most files, but `db.rs` also applies it to a
/// single `open_in_memory` function. Cutting a file off at the first attribute
/// would silently skip the rest of that file, so this walks brace depth and
/// suppresses exactly the annotated item — whether that is a module or one
/// function.
fn count_prod_panics(text: &str) -> usize {
    let mut depth: i32 = 0;
    /// While `Some(d)`, lines at depth `>= d` are inside a test item.
    let mut suppress_below: Option<i32> = None;
    /// A test attribute was seen; the next opening brace starts the item it
    /// annotates.
    let mut pending = false;
    let mut n = 0;

    for line in text.lines() {
        let trimmed = line.trim();

        if trimmed.starts_with("#[cfg(test)]")
            || trimmed.starts_with("#[test]")
            || trimmed.starts_with("#[tokio::test]")
        {
            pending = true;
        }

        let in_test = suppress_below.is_some_and(|d| depth >= d);
        if !in_test && !trimmed.starts_with("//") {
            n += line.matches(".unwrap()").count();
            n += line.matches(".expect(").count();
        }

        let opens = line.matches('{').count() as i32;
        let closes = line.matches('}').count() as i32;

        if pending && opens > 0 {
            suppress_below = Some(depth + 1);
            pending = false;
        }
        depth += opens - closes;
        if suppress_below.is_some_and(|d| depth < d) {
            suppress_below = None;
        }
    }

    n
}

/// Count `#rgb` / `#rrggbb` / longer hex colour literals.
///
/// Hand-rolled because `regex` is not a dependency and adding one to count
/// hashes would be a poor trade. Matches the same shapes as
/// `#[0-9a-fA-F]{3,8}` followed by a non-hex character: a run of nine or more
/// hex digits is not a colour, and is not counted.
fn count_hex_literals(text: &str) -> usize {
    let b = text.as_bytes();
    let mut n = 0;
    let mut i = 0;

    while i < b.len() {
        if b[i] == b'#' {
            let start = i + 1;
            let mut j = start;
            while j < b.len() && b[j].is_ascii_hexdigit() {
                j += 1;
            }
            if (3..=8).contains(&(j - start)) {
                n += 1;
            }
            i = j.max(i + 1);
        } else {
            i += 1;
        }
    }

    n
}

// ---------------------------------------------------------------------------
// The scanners are fiddly enough to be worth testing in their own right: a
// ratchet that counts wrong is worse than no ratchet, because it looks like
// enforcement.
// ---------------------------------------------------------------------------

#[test]
fn panic_scanner_skips_annotated_items_whether_mod_or_fn() {
    let prod = "fn a() { let x = y.unwrap(); }\nfn b() { z.expect(\"why\"); }\n";
    assert_eq!(count_prod_panics(prod), 2);

    let test_mod = "fn a() { y.unwrap(); }\n#[cfg(test)]\nmod tests {\n    fn t() { y.unwrap(); y.unwrap(); }\n}\n";
    assert_eq!(count_prod_panics(test_mod), 1, "the mod's unwraps must not count");

    let test_fn = "fn a() { y.unwrap(); }\n#[cfg(test)]\npub fn helper() { y.unwrap(); }\nfn b() { z.unwrap(); }\n";
    assert_eq!(
        count_prod_panics(test_fn),
        2,
        "an annotated fn is skipped, but code after it is still production"
    );

    let attr_fn = "#[test]\nfn t() { y.unwrap(); }\n";
    assert_eq!(count_prod_panics(attr_fn), 0);
}

#[test]
fn hex_scanner_counts_colours_and_ignores_longer_runs() {
    assert_eq!(count_hex_literals("#000"), 1);
    assert_eq!(count_hex_literals("alpha(#000, 0.28)"), 1);
    assert_eq!(count_hex_literals("#61afef #343842"), 2);
    assert_eq!(count_hex_literals("#ab"), 0, "two digits is not a colour");
    assert_eq!(count_hex_literals("#0123456789"), 0, "ten digits is not a colour");
    assert_eq!(count_hex_literals("#01234567"), 1, "eight is the longest colour");
}
