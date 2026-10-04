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
const MAX_PROD_PANICS: usize = 8;
// Was 12 until 2026-10-04: four of those were perf.rs test helpers
// miscounted as production (the scanner cannot see the file's
// `#![cfg(test)]`); they now carry item-level `#[cfg(test)]`, and the
// ratchet drops to the true production count.

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

/// Deferred covers without their warming call — the My Library bug of
/// 2026-10-03, as a rule.
///
/// `cover_widget_deferred` parks a placeholder and fills it only when a
/// texture for its exact (path, w, h) key lands in the cache — and the only
/// thing that produces those textures is a `warm_books`/`warm_covers` call at
/// that same size. Step 2b converted the dashboard's three covers to deferred
/// without the warm calls, and the page showed placeholders for the life of
/// the page on every launch, cold or warm. The rule: a file that builds
/// deferred covers warms them itself, at the size it asked for. Zero
/// offenders today; the bug itself would have been this test's first failure.
#[test]
fn deferred_covers_are_warmed_by_the_page_that_builds_them() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut offenders: Vec<String> = Vec::new();

    for (path, text) in sources_under(&root) {
        // Call sites only — the paren keeps `use` imports out of the match.
        if text.contains("cover_widget_deferred(")
            && !text.contains("warm_books(")
            && !text.contains("warm_covers(")
        {
            offenders.push(path.display().to_string());
        }
    }

    assert!(
        offenders.is_empty(),
        "\n\
         These files build deferred covers but never warm them:\n{}\
         \n\
         `cover_widget_deferred` fills only when a texture for its exact\n\
         (path, w, h) key is decoded — and nothing decodes it unless the page\n\
         that built the cards calls warm_books/warm_covers at that size.\n\
         My Library shipped blank covers for a day because step 2b skipped\n\
         this half of the contract (pitfalls §53).\n\
         ",
        offenders
            .iter()
            .map(|f| format!("    {f}\n"))
            .collect::<String>()
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
    // A STACK of depths at which a test item (mod or fn) opened. While the
    // stack is non-empty and `depth >= top`, we are inside test code. It
    // must be a stack, not a single level: a `#[test]` fn inside a
    // `#[cfg(test)]` mod replaces the mod's level while it is open, and
    // when the fn closes the mod's level must be RESTORED — a single slot
    // cleared instead, so an unannotated helper after the first inner test
    // leaked into the count. It only ever worked because helpers happened
    // to sit before the first test in every file (found 2026-10-04, when
    // a new test was inserted above one; pitfalls §62).
    let mut suppress: Vec<i32> = Vec::new();
    // A test attribute was seen; the next opening brace starts the item it
    // annotates.
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

        let opens = line.matches('{').count() as i32;
        let closes = line.matches('}').count() as i32;

        // This has to be decided *before* counting, not after. A single-line
        // item such as `pub fn helper() { x.unwrap(); }` opens and closes on
        // the same line, so setting suppression afterwards cleared it again
        // before the line was ever looked at — and the first version of this
        // counted it. The test below caught that on its first run in CI.
        let starts_test_item = pending && opens > 0;
        if starts_test_item {
            suppress.push(depth + 1);
            pending = false;
        }

        // `depth` is still the pre-line depth here, which is why a line that
        // starts the item needs the explicit `|| starts_test_item`.
        let in_test = suppress.last().is_some_and(|d| depth >= *d) || starts_test_item;
        if !in_test && !trimmed.starts_with("//") {
            n += line.matches(".unwrap()").count();
            n += line.matches(".expect(").count();
        }

        depth += opens - closes;
        while suppress.last().is_some_and(|top| depth < *top) {
            suppress.pop();
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

    // The regression that made suppression a stack (pitfalls §62): an
    // unannotated helper AFTER an inner test, inside a test mod. The single-
    // level version cleared suppression when the inner test closed, so the
    // helper's panics leaked into the count. It surfaced when a new test
    // was inserted above a helper that had always sat before the first one.
    let nested = concat!(
        "#[cfg(test)]\n",
        "mod tests {\n",
        "    #[test]\n",
        "    fn t() { y.unwrap(); }\n",
        "    fn helper() { z.unwrap(); w.expect(\"why\"); }\n",
        "    #[test]\n",
        "    fn u() { v.unwrap(); }\n",
        "}\n",
        "fn b() { q.unwrap(); }\n",
    );
    assert_eq!(
        count_prod_panics(nested),
        1,
        "everything inside the test mod is skipped, production after it counts"
    );
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

// ---------------------------------------------------------------------------
// 7.7 — the static boundary check: the UI thread never touches disk,
// database or parsing (ARCH.md). The database leg of that rule is enforced
// by `pages_do_not_gain_catalog_handles` above and the per-screen statement
// budgets in `src/perf.rs`; this check owns the disk and document legs.
// ---------------------------------------------------------------------------

/// Files wholly excluded: the readers. The owner excluded the readers from
/// the async campaign (2026-10-02) — they own their documents and run their
/// engines on service threads (ARCH.md's threading model). Everything else
/// under `src/pages/` is in scope.
const READER_PATH_PREFIXES: &[&str] = &[
    "src/pages/reader/",
    "src/pages/pdf_reader.rs",
    "src/pages/comics_reader/",
];

/// Surviving UI-thread sites, each with its written reason — the 7.7
/// arbitration. A site is `(file, needle)`: the entry covers every line in
/// that file containing the needle. A new entry needs a reason a reviewer
/// can check; the default answer is to move the work into a
/// `crate::tasks::spawn` worker, not to grow this list.
const ALLOWED_UI_THREAD_SITES: &[(&str, &str, &str)] = &[
    (
        "src/pages/all_books.rs",
        "if cover_path.exists() {",
        "micro-op stat: does this row's cover file exist before the widget shows it",
    ),
    (
        "src/pages/author.rs",
        "if path.is_file() {",
        "micro-op stat: the author avatar photo (the 7.1 step-2a allowlist question)",
    ),
    (
        "src/pages/author.rs",
        "path.is_file().then_some(path)",
        "micro-op stat: an author-work cover before the widget shows it",
    ),
    (
        "src/pages/book.rs",
        "photo.filter(|p| p.is_file())",
        "micro-op stat: the author avatar photo on the book page",
    ),
    (
        "src/pages/comics.rs",
        "fn expand_comic_paths(paths",
        "expansion helper definition; it runs only from the scanning task",
    ),
    (
        "src/pages/comics.rs",
        "std::fs::read_dir(&p)",
        "the expansion helper's body — see above; a synchronous call site is still caught",
    ),
    (
        "src/pages/metadata_editor.rs",
        "match std::fs::read(&path) {",
        "file-dialog callback reads the chosen cover; recorded follow-up — the editor's save flow is its own migration",
    ),
    (
        "src/pages/metadata_editor.rs",
        "crate::epub::replace_cover_bytes(&catalog, &fresh, &bytes)",
        "save handler writes the cover into the EPUB; same recorded follow-up",
    ),
    (
        "src/pages/remote_detail.rs",
        "Pixbuf::from_stream_at_scale",
        "apply-on-arrival decode of a bounded 160x230 thumbnail",
    ),
    (
        "src/pages/remote_detail.rs",
        "OpenBook::open(&b.file_path, &cache_dir)",
        "ReadChapter checks its EPUB cache synchronously in the handler; recorded follow-up with the metadata editor",
    ),
    (
        "src/pages/remote_detail.rs",
        "std::fs::remove_dir_all(&cache_dir)",
        "same ReadChapter flow: stale cache removal; same follow-up",
    ),
    (
        "src/pages/saved_quotes.rs",
        "pub fn export_all_quotes_markdown(",
        "export helper definition; it runs only from task workers now",
    ),
    (
        "src/pages/saved_quotes.rs",
        "std::fs::write(&out_path, markdown)",
        "the export helper's body — see above; a synchronous call is still caught",
    ),
    (
        "src/pages/saved_words.rs",
        "pub fn export_saved_words_csv(",
        "export helper definition; it runs only from task workers now",
    ),
    (
        "src/pages/saved_words.rs",
        "pub fn export_saved_words_anki(",
        "export helper definition; it runs only from task workers now",
    ),
    (
        "src/pages/saved_words.rs",
        "std::fs::write(&out_path, csv)",
        "the export helper's body — see above; a synchronous call is still caught",
    ),
    (
        "src/pages/saved_words.rs",
        "std::fs::write(&out_path, tsv)",
        "the export helper's body — see above; a synchronous call is still caught",
    ),
];

/// Lines that touch the disk or open/parse a document while NOT inside a
/// `crate::tasks::spawn` **worker** closure — i.e. on the UI thread.
///
/// Semantics, deliberately narrow: a line containing `tasks::spawn` arms a
/// pending flag, and the **first** `{` after it opens the suppression (the
/// worker closure — the second argument is always a label string in this
/// codebase). The progress and done callbacks after it run on the main
/// thread and stay scanned: disk work in them is a real violation. Comment
/// stripping is a crude `split("//")` — good enough for this codebase's
/// tidy lines. A tripwire, not a proof (roadmap 7.7): what it catches is
/// the honest mistake, a disk or document call typed straight into a
/// handler.
fn ui_thread_disk_or_parser_touches(text: &str) -> Vec<(usize, &'static str)> {
    const PATTERNS: &[&str] = &[
        "std::fs::",
        "fs::read",
        "fs::write",
        "fs::remove",
        "fs::create",
        "fs::rename",
        "fs::metadata",
        "fs::read_dir",
        "File::open",
        "File::create",
        ".exists()",
        ".is_file()",
        "read_to_string",
        "write_all",
        "import_epub(",
        "EpubBook::open",
        "OpenBook::open",
        "PdfDocument::open",
        "replace_cover_bytes(",
        "Pixbuf::from_file",
        "Pixbuf::from_stream",
        "Pixbuf::from_bytes",
        "expand_comic_paths(",
        "export_reading_data_markdown(",
        "export_saved_words_csv(",
        "export_saved_words_anki(",
        "export_all_quotes_markdown(",
    ];
    let mut hits = Vec::new();
    let mut depth: i32 = 0;
    let mut suppress: Vec<i32> = Vec::new();
    let mut pending = false;
    for (idx, raw) in text.lines().enumerate() {
        let line = raw.split("//").next().unwrap_or("");
        if line.contains("tasks::spawn") {
            pending = true;
        }
        for ch in line.chars() {
            match ch {
                '{' => {
                    depth += 1;
                    if pending {
                        suppress.push(depth);
                        pending = false;
                    }
                }
                '}' => {
                    while let Some(&top) = suppress.last() {
                        if depth <= top {
                            suppress.pop();
                        } else {
                            break;
                        }
                    }
                    depth -= 1;
                }
                _ => {}
            }
        }
        if suppress.is_empty() && !pending {
            if let Some(pat) = PATTERNS.iter().find(|p| line.contains(*p)) {
                hits.push((idx + 1, *pat));
            }
        }
    }
    hits
}

#[test]
fn pages_touch_disk_or_documents_only_inside_tasks() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/pages");
    let mut violations = Vec::new();
    let mut allowed_seen = vec![false; ALLOWED_UI_THREAD_SITES.len()];

    for (path, text) in sources_under(&root) {
        let rel = format!(
            "src/pages/{}",
            path.strip_prefix(&root).unwrap_or(&path).display()
        );
        if READER_PATH_PREFIXES.iter().any(|p| rel.starts_with(p)) {
            continue;
        }
        for (lineno, pat) in ui_thread_disk_or_parser_touches(&text) {
            let line = text.lines().nth(lineno - 1).unwrap_or("");
            match ALLOWED_UI_THREAD_SITES
                .iter()
                .position(|(f, needle, _)| rel == *f && line.contains(needle))
            {
                Some(i) => allowed_seen[i] = true,
                None => violations.push(format!("{rel}:{lineno}  [{pat}]  {}", line.trim())),
            }
        }
    }

    // An allowlist entry that matched nothing is dead text — the line moved
    // or was renamed, and the entry must go with it, or the list silently
    // rots into decoration.
    let stale: Vec<&str> = ALLOWED_UI_THREAD_SITES
        .iter()
        .enumerate()
        .filter(|(i, _)| !allowed_seen[*i])
        .map(|(_, (f, _, _))| *f)
        .collect();

    assert!(
        violations.is_empty() && stale.is_empty(),
        "\n\
         The UI thread never touches disk, database or parsing (ARCH.md).\n\
         \n\
         New violations — move the work into a crate::tasks::spawn worker:\n\
         {}\n\
         {}\
         Stale allowlist entries (matched nothing — delete them):\n\
         {}",
        if violations.is_empty() {
            "    (none)\n".to_string()
        } else {
            violations
                .iter()
                .map(|v| format!("    {v}\n"))
                .collect::<String>()
        },
        if stale.is_empty() { "" } else { "\n" },
        if stale.is_empty() {
            "    (none)".to_string()
        } else {
            stale
                .iter()
                .map(|f| format!("    {f}\n"))
                .collect::<String>()
        },
    );
}

#[test]
fn boundary_scanner_suppresses_workers_not_done_callbacks() {
    // The worker closure is the task layer; the progress and done
    // callbacks run on the main thread and must stay scanned.
    let code = concat!(
        "fn a() { std::fs::write(&p, x); }\n",
        "crate::tasks::spawn(\"t\", move |_| {\n",
        "    std::fs::write(&q, y);\n",
        "    if p.exists() {}\n",
        "}, |_| {}, move |r| {\n",
        "    crate::epub::import_epub(&c, &z);\n",
        "});\n",
        "fn c() { crate::epub_book::OpenBook::open(&f, &d); }\n",
    );
    let hits = ui_thread_disk_or_parser_touches(code);
    let lines: Vec<usize> = hits.iter().map(|(l, _)| *l).collect();
    assert_eq!(
        lines,
        vec![1, 6, 8],
        "line 1 = plain fn, line 6 = done callback (main thread), \
         line 8 = plain fn; the worker body (3-4) is suppressed: {hits:?}"
    );
}
