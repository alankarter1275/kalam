//! A0 measurement harness (headless — no display needed, safe in CI).
//!
//! Two kinds of thing live here, and the difference matters:
//!
//! - **Query-count budgets** (A0 step 7, bottom of the file) — these are
//!   ordinary tests. They run on every `cargo test` and they gate CI. They
//!   assert that a call's *statement count* does not grow with the number of
//!   rows it touches, which is the N+1 regression this repo has shipped three
//!   times. An integer count is machine-independent, so it cannot flake.
//! - **Timing probes** (below, `#[ignore]`d) — these only print. They are run
//!   by hand on real hardware for a baseline. They are *not* budgets, because
//!   the CI runner's wall-clock varies ~60% between identical runs, so any
//!   ceiling loose enough to survive that cannot catch a real regression.
//!
//! The two `#[ignore]`d probes:
//!
//! - **Data layer** (`perf_data_layer_report`): the cost of the queries that
//!   feed every list page, seeded with 2,000 books.
//! - **Cover decode** (`perf_cover_decode`): the A0 step 3 win — decoding a
//!   full cover vs the persistent thumbnail for a grid slot.
//!
//! Run them on the Arch machine for a baseline / regression check:
//!
//! ```text
//! cargo test --release perf -- --ignored --nocapture
//! ```
//!
//! The other half of A0 (cold start, book open, chapter turn, grid scroll) is
//! measured in-app via `KALAM_TIMING=1` (see `src/timing.rs`).

#![cfg(test)]

use crate::db::{Catalog, SortKey};
use crate::models::BookFormat;
use crate::service::LibraryService;
use std::time::Instant;

const N: usize = 2000;

/// Seed `n` varied books (unique uuid/hash; author, series and tags rotate so
/// every sort order is deterministic-but-non-trivial).
// Item-level on purpose: the panic-count scanner in tests/guardrails.rs
// reads item attributes and cannot see this file's `#![cfg(test)]` — an
// unannotated helper's `.unwrap()`s count as production panics.
#[cfg(test)]
fn seed(cat: &Catalog, n: usize) {
    for i in 0..n {
        let title = format!("Book {i:04} — The Something of the Elsewhere");
        let uuid = format!("perf-uuid-{i:04}");
        let hash = format!("perf-hash-{i:04}");
        let author = format!("Author {}", i % 100);
        let series = (i % 3 == 0).then(|| format!("Series {}", i % 20));
        let tags = vec!["scifi".to_string(), format!("tag{}", i % 50)];
        cat.insert_book(
            &uuid,
            &title,
            &author,
            series.as_deref(),
            "",
            BookFormat::Epub,
            "book.epub",
            &hash,
            None,
            &tags,
        )
        .expect("seed book");
    }
}

/// Time one call, print the label + duration, return just the duration in ms.
macro_rules! timed {
    ($label:expr, $body:expr) => {{
        let start = Instant::now();
        #[allow(clippy::let_unit_value)]
        let _ = $body();
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        println!("{:<40} {:>8.2} ms", $label, ms);
        ms
    }};
}

/// Loose ceilings. The point is to catch an accidental O(n²) regression (a new
/// `for book in books` loop, an added per-row query), not to assert a hard
/// budget — the real numbers come from running this and from the user's Arch
/// machine. Set generously for CI hosts; tighten after a baseline.
const CEILING_MS: f64 = 500.0;

fn assert_under(label: &str, ms: f64) {
    assert!(
        ms < CEILING_MS,
        "perf regression: {label} took {ms:.2} ms (ceiling {CEILING_MS:.2})"
    );
}

#[test]
#[ignore = "run manually: cargo test --release perf -- --ignored --nocapture"]
fn perf_data_layer_report() {
    let cat = Catalog::open_in_memory().unwrap();
    let seed_start = Instant::now();
    seed(&cat, N);
    let seed_ms = seed_start.elapsed().as_secs_f64() * 1000.0;
    println!("== Kalam data-layer probe: {N} books ==");
    println!("{:<40} {:>8.2} ms", "seed", seed_ms);

    // Whole-library listing (the cost behind an "All books" grid refresh).
    let ms = timed!("list_books(Title, \"\")", || {
        cat.list_books(SortKey::Title, "").unwrap()
    });
    assert_under("list_books(Title, \"\")", ms);

    let ms = timed!("list_books(Author, \"\")", || {
        cat.list_books(SortKey::Author, "").unwrap()
    });
    assert_under("list_books(Author, \"\")", ms);

    let ms = timed!("list_books(Added, \"\")", || {
        cat.list_books(SortKey::Added, "").unwrap()
    });
    assert_under("list_books(Added, \"\")", ms);

    // Search (LIKE + ESCAPE) over the whole table.
    let ms = timed!("list_books(Title, \"something\")", || {
        cat.list_books(SortKey::Title, "something").unwrap()
    });
    assert_under("list_books(Title, search)", ms);

    // Small capped pulls (dashboard cards).
    let ms = timed!("recent_books(30)", || cat.recent_books(30).unwrap());
    assert_under("recent_books(30)", ms);

    // Aggregates.
    let ms = timed!("library_stats()", || cat.library_stats().unwrap());
    assert_under("library_stats()", ms);

    let ms = timed!("list_tags_with_counts()", || {
        cat.list_tags_with_counts().unwrap()
    });
    assert_under("list_tags_with_counts()", ms);

    let ms = timed!("books_with_tag(scifi)", || {
        cat.books_with_tag("scifi", SortKey::Title).unwrap()
    });
    assert_under("books_with_tag(scifi)", ms);

    println!("\nAll data-layer probes under {CEILING_MS:.0} ms — no O(n²) regression visible.");
}

/// A0 step 3 — cover decode: full cover vs the persistent thumbnail. This is
/// the cost behind every grid card on a cold launch. Creates a real cover image
/// in a scratch dir and times both decode paths (gdk-pixbuf).
#[test]
#[ignore = "run manually: cargo test --release perf -- --ignored --nocapture"]
fn perf_cover_decode() {
    use gdk_pixbuf::{InterpType, Pixbuf};

    let scratch = std::env::temp_dir().join(format!("kalam-perf-cover-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();
    let cover = scratch.join("cover.png");
    let thumb = scratch.join("thumb.png");

    // A believable full cover (600×900) and the 256×408 thumbnail.
    let w = 600u32;
    let h = 900u32;
    let buf: Vec<u8> = (0..(w * h)).flat_map(|_| [40u8, 70, 130]).collect();
    image::save_buffer(&cover, &buf, w, h, image::ExtendedColorType::Rgb8).unwrap();
    assert!(crate::thumbs::generate_thumbnail(&cover, &thumb));

    println!("== Cover decode: full cover vs thumbnail (grid slot 128×204) ==");

    let ms = timed!("gtk decode FULL cover → 128×204", || {
        let p = Pixbuf::from_file_at_scale(&cover, 128, 204, false).unwrap();
        let _ = p.scale_simple(128, 204, InterpType::Bilinear).unwrap();
    });
    println!("{:<40} {:>8.2} ms", "(full cover decode)", ms);

    let ms = timed!("gtk decode THUMB → 128×204", || {
        let p = Pixbuf::from_file_at_scale(&thumb, 128, 204, false).unwrap();
        let _ = p.scale_simple(128, 204, InterpType::Bilinear).unwrap();
    });
    println!("{:<40} {:>8.2} ms", "(thumbnail decode)", ms);

    println!("\nGrid-slot decode of the thumbnail is the hot path after import; the full-cover decode is what a cold launch pays per card today.");

    let _ = std::fs::remove_dir_all(&scratch);
}

// ---------------------------------------------------------------------------
// A0 step 7 — query-count budgets
//
// These are the perf-budget tests the roadmap asked for, in the shape the
// evidence pushed them into. The original plan said "seed 2,000 books, assert
// grid build under N ms". Two things were wrong with that:
//
//  1. **Time thresholds cannot work on this runner.** Byte-identical work has
//     been measured at 2774 ms and 3740 ms on consecutive CI runs. A ceiling
//     loose enough to survive that variance would not notice a 2× regression.
//  2. **The bug class that actually bites is not slowness, it is N+1.** Three
//     separate incidents in this repo (pitfalls §16, §18, and the cover
//     preloader) were all "a loop that issues one query per row". That is
//     invisible in a timing on 139 books and fatal at 2,000.
//
// A statement count is an integer. It is identical on every machine, it does
// not flake, and it fails the moment somebody reintroduces a per-row query.
//
// **These were verified by deliberately breaking the code** (pitfalls §19: a
// check that has never failed is not known to work). `hydrate_books` was
// temporarily changed back to a per-book tag query and pushed to CI:
// 4 of the 6 budgets failed and the other 270 tests passed, which is exactly
// the intended blast radius. The sabotage was reverted in the next commit.
//
// **These tests are NOT `#[ignore]`d** — unlike the two probes above, they run
// on every `cargo test`, which is the entire point of a budget. They stay fast
// by asserting on a small library where the *count* is already meaningful:
// N+1 shows up at 20 books just as clearly as at 2,000, because the assertion
// is "does this scale with row count", not "is this fast".
// ---------------------------------------------------------------------------

/// A small library. Deliberately not 2,000: the assertions are about whether
/// the query count grows with the number of rows, and 20 rows answers that in
/// milliseconds. `SMALL` and `LARGER` differ so a count can be compared
/// between two sizes.
const SMALL: usize = 20;
const LARGER: usize = 60;

/// Seed `n` books whose ids do not collide with a previous `seed` call.
///
/// The `seed` used by the probes above numbers uuids from 0, so calling it
/// twice on one catalog violates the unique constraint. Tests that need to
/// add a book *after* measuring take this instead.
// Item-level on purpose: the panic-count scanner in tests/guardrails.rs
// reads item attributes and cannot see this file's `#![cfg(test)]` — an
// unannotated helper's `.unwrap()`s count as production panics.
#[cfg(test)]
fn seed_more(cat: &Catalog, tag: &str, n: usize) {
    for i in 0..n {
        cat.insert_book(
            &format!("extra-uuid-{tag}-{i:04}"),
            &format!("Extra {tag} {i:04}"),
            "Extra Author",
            None,
            "",
            BookFormat::Epub,
            "book.epub",
            &format!("extra-hash-{tag}-{i:04}"),
            None,
            &["scifi".to_string()],
        )
        .expect("seed extra book");
    }
}

/// The invariant these budgets exist to defend: **the number of SQL statements
/// a call issues must not depend on how many rows it touches.**
///
/// Deliberately expressed as a comparison between two library sizes rather
/// than as a hard-coded number. A literal like `assert_eq!(n, 2)` bakes in an
/// implementation detail — whether tags are hydrated in one query or two is a
/// free choice, and a legitimate refactor would fail the test for no reason,
/// which teaches people to edit the number until it passes. Comparing 20 books
/// against 60 cannot be satisfied that way: the only way to pass is to not
/// issue queries per row. The absolute count is printed, not asserted, so a
/// change is visible in the log without being a failure.
fn assert_constant_in_library_size(label: &str, small: usize, larger: usize) {
    println!("{label:<44} {small} statements @{SMALL} books, {larger} @{LARGER}");
    assert_eq!(
        small, larger,
        "{label}: {small} SQL statements for {SMALL} books but {larger} for \
         {LARGER}. The count scales with row count, which is the N+1 pattern \
         behind pitfalls §16 and §18 -- look for a query inside a loop over \
         rows."
    );
    // A call that issues nothing is not evidence of anything; it usually means
    // the measurement itself broke (pitfalls §19).
    assert!(
        small > 0,
        "{label} recorded 0 statements -- the counter is not wired up, so this \
         test cannot fail and is not a test"
    );
}

/// Two catalogs of different sizes, for the comparison above.
// Item-level on purpose: the panic-count scanner in tests/guardrails.rs
// reads item attributes and cannot see this file's `#![cfg(test)]` — an
// unannotated helper's `.unwrap()`s count as production panics.
#[cfg(test)]
fn small_and_larger() -> (Catalog, Catalog) {
    let small = Catalog::open_in_memory().unwrap();
    seed(&small, SMALL);
    let larger = Catalog::open_in_memory().unwrap();
    seed(&larger, LARGER);
    (small, larger)
}

#[test]
fn list_books_does_not_scale_queries_with_library_size() {
    // `list_books` issues one SELECT for the books and one for all their tags
    // (`hydrate_books`). If either becomes per-book, the two counts diverge.
    let (small, larger) = small_and_larger();
    let a = small.count_queries(|| small.list_books(SortKey::Title, "").unwrap());
    let b = larger.count_queries(|| larger.list_books(SortKey::Title, "").unwrap());
    assert_constant_in_library_size("list_books(Title, \"\")", a, b);
}

#[test]
fn list_books_search_path_does_not_scale_either() {
    // The search branch is a separate SQL string and can regress on its own.
    let (small, larger) = small_and_larger();
    let a = small.count_queries(|| small.list_books(SortKey::Title, "Something").unwrap());
    let b = larger.count_queries(|| larger.list_books(SortKey::Title, "Something").unwrap());
    assert_constant_in_library_size("list_books(Title, search)", a, b);
}

#[test]
fn recent_books_cost_does_not_depend_on_how_many_it_returns() {
    // Home asks for a handful of covers. This used to load the whole library.
    let cat = Catalog::open_in_memory().unwrap();
    seed(&cat, LARGER);
    let six = cat.count_queries(|| cat.recent_books(6).unwrap());
    let thirty = cat.count_queries(|| cat.recent_books(30).unwrap());
    println!("recent_books                                 {six} @6, {thirty} @30");
    assert_eq!(
        six, thirty,
        "recent_books(6) cost {six} statements and recent_books(30) cost \
         {thirty} -- the count must not depend on how many rows come back"
    );
    assert!(
        six > 0,
        "recent_books recorded 0 statements -- counter not wired"
    );
}

#[test]
fn books_by_ids_batches_instead_of_looping() {
    // `books_by_ids` exists *because* callers were fetching one book at a
    // time. It chunks at 500, so 50 ids must cost the same as 1.
    let cat = Catalog::open_in_memory().unwrap();
    seed(&cat, LARGER);
    let ids: Vec<i64> = cat
        .list_books(SortKey::Title, "")
        .unwrap()
        .iter()
        .take(50)
        .map(|b| b.id)
        .collect();
    assert_eq!(
        ids.len(),
        50,
        "need 50 seeded books for this to mean anything"
    );

    let one = cat.count_queries(|| cat.books_by_ids(&ids[..1]).unwrap());
    let fifty = cat.count_queries(|| cat.books_by_ids(&ids).unwrap());
    println!("books_by_ids                                 {one} @1 id, {fifty} @50 ids");
    assert_eq!(
        one, fifty,
        "books_by_ids cost {one} statements for 1 id and {fifty} for 50 -- it \
         is looping, which is the exact bug it was written to remove"
    );
    assert!(
        one > 0,
        "books_by_ids recorded 0 statements -- counter not wired"
    );
}

#[test]
fn books_with_tag_does_not_query_per_book() {
    // Every seeded book carries "scifi", so this returns the whole library and
    // a per-row query is unmissable.
    let (small, larger) = small_and_larger();
    let a = small.count_queries(|| small.books_with_tag("scifi", SortKey::Title).unwrap());
    let b = larger.count_queries(|| larger.books_with_tag("scifi", SortKey::Title).unwrap());
    assert_constant_in_library_size("books_with_tag(scifi)", a, b);
}

#[test]
fn library_stats_is_memoised_and_invalidates_on_write() {
    // ~20 aggregates, called from Home, the Library dashboard and Analytics.
    // The memo is keyed on total_changes(); if that wiring breaks, the cost
    // silently triples on every page load, or -- worse -- the numbers go
    // stale. Both directions are asserted.
    let cat = Catalog::open_in_memory().unwrap();
    seed(&cat, SMALL);

    let cold = cat.count_queries(|| cat.library_stats().unwrap());
    assert!(
        cold > 1,
        "library_stats ran {cold} statements -- expected a batch of aggregates"
    );

    let warm = cat.count_queries(|| cat.library_stats().unwrap());
    println!("library_stats                                {cold} cold, {warm} warm");
    assert_eq!(
        warm, 0,
        "the second library_stats() ran {warm} statements; it must come from \
         the memo. Check the total_changes() cache key."
    );

    // A write must invalidate it, or the dashboard shows stale counts.
    seed_more(&cat, "stats", 1);
    let after_write = cat.count_queries(|| cat.library_stats().unwrap());
    assert_eq!(
        after_write, cold,
        "after a write, library_stats() ran {after_write} statements but a cold \
         call costs {cold} -- the memo did not invalidate, so the numbers on \
         screen are now stale"
    );
}

// ---------------------------------------------------------------------------
// Step 4 (7.7) — per-screen snapshot budgets
//
// The step-3 survey pages open by reading one LibraryService snapshot. The
// enforcement shape for them: the statement count of a screen's read must
// not depend on how many rows the screen shows — the same
// integer-not-milliseconds reasoning as the catalog budgets above, with the
// growth axis moved from library size to the page's own row count. A
// reading list with 5 entries and one with 15 must cost the same number of
// statements.
//
// Deliberately not covered yet (later step-4 increments, in order): the
// step-2 migrated pages (home, dashboard, book page/detail, settings,
// author, all books) and the picker read; the zero-queries-at-construction
// half needs the static boundary check, which comes last. These tests reuse
// the sabotage-verified harness but have not themselves been
// sabotage-verified — owed before step 4 closes (§19: a check that has
// never failed is not known to work). The row-count assert below guards the
// cheaper failure: a seed that silently produced no rows.
// ---------------------------------------------------------------------------

/// Page-data sizes: enough rows that a per-row query is unmissable, small
/// enough to stay instant.
const FEW: usize = 5;
const MANY: usize = 15;

/// Two services over the same SMALL library — one with FEW page rows
/// seeded, one with MANY. The seed closure returns a value per side (the
/// shelf test uses it to hand back the shelf id); most tests return ().
///
/// Book ids are read back from the catalog rather than assumed, so a broken
/// seed fails loudly here instead of passing vacuously.
// Item-level on purpose: the panic-count scanner in tests/guardrails.rs
// reads item attributes and cannot see this file's `#![cfg(test)]` — an
// unannotated helper's `.unwrap()`s count as production panics.
#[cfg(test)]
fn few_and_many_page<T>(
    seed_page_rows: impl Fn(&Catalog, &[i64]) -> T,
) -> (LibraryService, LibraryService, T, T) {
    let build = |rows: usize| -> (LibraryService, T) {
        let cat = Catalog::open_in_memory().unwrap();
        seed(&cat, SMALL);
        let ids: Vec<i64> = cat
            .list_books(SortKey::Title, "")
            .unwrap()
            .into_iter()
            .map(|b| b.id)
            .collect();
        assert!(
            ids.len() >= rows,
            "seeding {rows} page rows but the library only has {} books",
            ids.len()
        );
        let extra = seed_page_rows(&cat, &ids[..rows]);
        (LibraryService::new(std::sync::Arc::new(cat)), extra)
    };
    let (few, a) = build(FEW);
    let (many, b) = build(MANY);
    (few, many, a, b)
}

/// The screen-budget invariant, with the vacuous-pass guard built in: the
/// row counts must actually differ before the statement counts compare.
fn assert_constant_in_page_rows(
    label: &str,
    few_rows: usize,
    many_rows: usize,
    few_stmts: usize,
    many_stmts: usize,
) {
    assert!(
        few_rows > 0 && many_rows > few_rows,
        "{label}: seeded {few_rows} vs {many_rows} rows — the seed is broken, \
         so this test cannot fail and is not a test (§19)"
    );
    println!(
        "{label:<44} {few_stmts} statements @{few_rows} rows, {many_stmts} @{many_rows}"
    );
    assert_eq!(
        few_stmts, many_stmts,
        "{label}: {few_stmts} SQL statements for {few_rows} rows but {many_stmts} for \
         {many_rows}. The read scales with the number of rows it returns — the N+1 \
         pattern behind pitfalls §16 and §18. Look for a query inside a loop over rows."
    );
    assert!(
        few_stmts > 0,
        "{label} recorded 0 statements — the counter is not wired up, so this \
         test cannot fail and is not a test"
    );
}

#[test]
fn reading_list_snapshot_does_not_scale_with_entries() {
    let (few, many, (), ()) = few_and_many_page(|cat, ids| {
        for id in ids {
            cat.add_to_reading_list(*id).expect("seed reading list");
        }
    });
    let rows = |svc: &LibraryService, out: &std::cell::Cell<usize>| {
        svc.catalog()
            .count_queries(|| out.set(svc.reading_list().entries.len()))
    };
    let (ra, rb) = (
        std::cell::Cell::new(0usize),
        std::cell::Cell::new(0usize),
    );
    let a = rows(&few, &ra);
    let b = rows(&many, &rb);
    assert_constant_in_page_rows("service.reading_list()", ra.get(), rb.get(), a, b);
}

#[test]
fn shelf_detail_snapshot_does_not_scale_with_shelf_books() {
    let (few, many, sa, sb) = few_and_many_page(|cat, ids| {
        let shelf = cat
            .create_shelf("Budget", crate::db::ShelfKind::Manual, "", "")
            .expect("create shelf");
        for id in ids {
            cat.add_series_to_shelf(shelf, *id)
                .expect("seed shelf book");
        }
        shelf
    });
    let rows =
        |svc: &LibraryService, shelf: i64, out: &std::cell::Cell<usize>| {
            svc.catalog().count_queries(|| {
                out.set(svc.shelf_detail(shelf, SortKey::Title, "").books.len())
            })
        };
    let (ra, rb) = (
        std::cell::Cell::new(0usize),
        std::cell::Cell::new(0usize),
    );
    let a = rows(&few, sa, &ra);
    let b = rows(&many, sb, &rb);
    assert_constant_in_page_rows(
        "service.shelf_detail()",
        ra.get(),
        rb.get(),
        a,
        b,
    );
}

#[test]
fn quotes_snapshot_does_not_scale_with_saved_quotes() {
    let (few, many, (), ()) = few_and_many_page(|cat, ids| {
        for (i, id) in ids.iter().enumerate() {
            cat.insert_annotation(
                *id,
                "quote",
                0,
                "",
                0,
                "",
                0,
                "",
                "",
                &format!("Budget excerpt {i:02}"),
                "",
            )
            .expect("seed quote");
        }
    });
    let rows = |svc: &LibraryService, out: &std::cell::Cell<usize>| {
        svc.catalog()
            .count_queries(|| out.set(svc.quotes("").quotes.len()))
    };
    let (ra, rb) = (
        std::cell::Cell::new(0usize),
        std::cell::Cell::new(0usize),
    );
    let a = rows(&few, &ra);
    let b = rows(&many, &rb);
    assert_constant_in_page_rows("service.quotes()", ra.get(), rb.get(), a, b);
}

#[test]
fn words_snapshot_does_not_scale_with_saved_words() {
    let (few, many, (), ()) = few_and_many_page(|cat, ids| {
        for (i, _id) in ids.iter().enumerate() {
            cat.insert_saved_word(&format!("budget{i:02}"), "a definition", None, None, None, None)
                .expect("seed word");
        }
    });
    let rows = |svc: &LibraryService, out: &std::cell::Cell<usize>| {
        svc.catalog()
            .count_queries(|| out.set(svc.words("", None).words.len()))
    };
    let (ra, rb) = (
        std::cell::Cell::new(0usize),
        std::cell::Cell::new(0usize),
    );
    let a = rows(&few, &ra);
    let b = rows(&many, &rb);
    assert_constant_in_page_rows("service.words()", ra.get(), rb.get(), a, b);
}

#[test]
fn history_snapshot_does_not_scale_with_events() {
    let (few, many, (), ()) = few_and_many_page(|cat, ids| {
        for id in ids {
            cat.log_event(*id, crate::db::EventKind::Opened, "budget seed")
                .expect("seed event");
        }
    });
    let rows = |svc: &LibraryService, out: &std::cell::Cell<usize>| {
        svc.catalog()
            .count_queries(|| out.set(svc.history(None, "", 100).events.len()))
    };
    let (ra, rb) = (
        std::cell::Cell::new(0usize),
        std::cell::Cell::new(0usize),
    );
    let a = rows(&few, &ra);
    let b = rows(&many, &rb);
    assert_constant_in_page_rows("service.history()", ra.get(), rb.get(), a, b);
}

#[test]
fn lookup_history_snapshot_does_not_scale_with_lookups() {
    let (few, many, (), ()) = few_and_many_page(|cat, ids| {
        for (i, _id) in ids.iter().enumerate() {
            // Distinct words: log_dict_lookup collapses repeats within an
            // hour, and identical seeds would collapse to one row.
            cat.log_dict_lookup(&format!("budget-word-{i:02}"), None, None, None, true)
                .expect("seed lookup");
        }
    });
    let rows = |svc: &LibraryService, out: &std::cell::Cell<usize>| {
        svc.catalog()
            .count_queries(|| out.set(svc.lookup_history("", 100).lookups.len()))
    };
    let (ra, rb) = (
        std::cell::Cell::new(0usize),
        std::cell::Cell::new(0usize),
    );
    let a = rows(&few, &ra);
    let b = rows(&many, &rb);
    assert_constant_in_page_rows(
        "service.lookup_history()",
        ra.get(),
        rb.get(),
        a,
        b,
    );
}
