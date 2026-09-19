//! A0 step 1 — in-app GUI timing harness, gated behind `KALAM_TIMING=1`.
//!
//! The data-layer cost was measured by `perf.rs` (headless, automated). The
//! GUI half — cold start, book open, chapter turn, dict lookup — needs a real
//! display, so it is measured by *you* on the Arch machine. This module makes
//! that trivial and repeatable: set `KALAM_TIMING=1`, do the action, and the
//! app prints elapsed milliseconds to the terminal.
//!
//! It is deliberately tiny and thread-safe (a `Mutex<HashMap>` for open spans,
//! an `OnceLock<bool>` for the "enabled" flag). When the env var is absent every
//! call returns immediately and prints nothing, so there is zero overhead in
//! normal use.
//!
//! ```text
//! KALAM_TIMING=1 cargo run --release
//! ```
//!
//! Then read the `[timing]` lines. They mark the boundaries A0 cares about:
//!   window_shown  → first window drawn, measured from process start
//!                   (cold start; a `now` snapshot, not a span)
//!   startup_gtk_init / startup_icons / startup_style / startup_libraries /
//!   startup_db_open / startup_theme / startup_dicts / startup_first_page
//!                 → the pieces of work that run before first paint, so a slow
//!                   `window_shown` can be attributed rather than guessed at.
//!                   `startup_dicts` is large only on the very first run (it
//!                   imports the bundled packs) and `startup_first_page`
//!                   scales with library size. The rest were added by roadmap
//!                   1.12 after a nine-second cold start turned out to have
//!                   only 2.4 of its 9.2 seconds attributed to anything.
//!   pre_run       → last instant `main()` can time, because `app.run` never
//!                   returns. `window_shown - pre_run` is therefore the cost
//!                   of `AppModel::init` plus GTK's first realize.
//!   book_open     → EPUB parsed and the reader initialised
//!   chapter_load  → chapter HTML laid out by `kalam-reader` *until* the page
//!                   was painted — i.e. the whole chapter turn
//!   dict_lookup   → dictionary search returned
//!   service_*     → one `LibraryService` snapshot query, one line per call,
//!                   so "which read blocks the UI thread" is answered by a log
//!                   rather than by guessing (roadmap 1.2a)
//!
//! Note that a span prints **one** line, under the label it was opened with,
//! when it ends. `chapter_load` therefore reports the full load→rendered
//! duration; there is no separate "done" line to wait for.
//!
//! A book open + one chapter turn is enough to answer whether the reader paths
//! need preloaders (A0 step 5).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

static ENABLED: OnceLock<bool> = OnceLock::new();
static START: OnceLock<Instant> = OnceLock::new();
static SPANS: OnceLock<Mutex<HashMap<&'static str, Instant>>> = OnceLock::new();

fn enabled() -> bool {
    *ENABLED.get_or_init(|| std::env::var_os("KALAM_TIMING").is_some())
}

fn start_anchor() -> Instant {
    *START.get_or_init(Instant::now)
}

/// Record the process-start anchor. Call once, near the top of `main()` (before
/// the GTK main loop). Safe to call more than once: the first call wins.
pub fn start() {
    let _ = START.get_or_init(Instant::now);
}

/// Print a single snapshot: `label` as elapsed ms since the process began.
/// For cold start, call this at `window_shown` (no-op unless timing is on).
pub fn now(label: &'static str) {
    if !enabled() {
        return;
    }
    let el = start_anchor().elapsed().as_secs_f64() * 1000.0;
    println!("[timing] {label:<18} {el:>8.1} ms");
}

/// Print a labelled count, e.g. how many tasks were still running at exit.
///
/// Like the rest of this module it is a no-op unless `KALAM_TIMING=1`, so it
/// costs nothing in a normal run.
pub fn note(label: &'static str, value: usize) {
    if !enabled() {
        return;
    }
    println!("[timing] {label:<18} {value:>8}");
}

/// Print a duration the caller measured itself, e.g. one service query.
///
/// This is the half of the module that does not need a matching pair of calls:
/// [`measure`] starts a [`Guard`] whose `Drop` reports the elapsed time, so an
/// early `return` cannot leave a span open and no `*_end` call can be forgotten.
/// No-op unless `KALAM_TIMING=1`.
pub fn duration(label: &'static str, d: std::time::Duration) {
    if !enabled() {
        return;
    }
    println!("[timing] {label:<18} {:>8.1} ms", d.as_secs_f64() * 1000.0);
}

/// Measures the scope it is bound to and prints on drop. Returned by
/// [`measure`].
///
/// Bind it with a name — `let _t = timing::measure("service_home");`. Binding
/// it to `_` instead drops it immediately and reports ~0 ms, which reads as a
/// fast query and is the one way to use this wrong silently.
#[must_use = "dropping this immediately reports ~0 ms; bind it to a name"]
pub struct Guard {
    label: &'static str,
    start: Instant,
}

impl Drop for Guard {
    fn drop(&mut self) {
        duration(self.label, self.start.elapsed());
    }
}

/// Start timing `label`. Costs one `Instant::now()` and prints nothing unless
/// `KALAM_TIMING=1`.
///
/// ```ignore
/// pub fn home(&self) -> HomeSnapshot {
///     let _t = crate::timing::measure("service_home");
///     ...
/// }
/// ```
pub fn measure(label: &'static str) -> Guard {
    Guard {
        label,
        start: Instant::now(),
    }
}

/// Begin a named span (e.g. "book_open"). If a span with the same label is
/// already open it is replaced.
pub fn span(label: &'static str) {
    if !enabled() {
        return;
    }
    SPANS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("timing span lock")
        .insert(label, Instant::now());
}

/// End a named span opened by `span(label)` and print its elapsed ms.
///
/// The line is printed under the label the span was *opened* with, so
/// `span("chapter_load")` … `span_end("chapter_load")` yields a single
/// `chapter_load` line covering the whole interval.
pub fn span_end(label: &'static str) {
    if !enabled() {
        return;
    }
    let Some(start) = SPANS
        .get_or_init(|| Mutex::new(HashMap::new()))
        .lock()
        .expect("timing span lock")
        .remove(label)
    else {
        return;
    };
    let el = start.elapsed().as_secs_f64() * 1000.0;
    println!("[timing] {label:<18} {el:>8.1} ms");
}
