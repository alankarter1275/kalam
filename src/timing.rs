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
//!   startup_gtk_init / startup_style / startup_libraries / startup_db_open /
//!   startup_theme / startup_dicts / startup_first_page
//!                 → the pieces of work that run before first paint, so a slow
//!                   `window_shown` can be attributed rather than guessed at.
//!                   `startup_dicts` is large only on the very first run (it
//!                   imports the bundled packs) and `startup_first_page`
//!                   scales with library size. The rest were added by roadmap
//!                   1.12 after a nine-second cold start turned out to have
//!                   only 2.4 of its 9.2 seconds attributed to anything — and
//!                   then turned out to be a cold OS page cache, with the warm
//!                   start at 1.36 s.
//!   icons_write / icons_theme
//!                 → registering Kalam's own two symbolic icons. Written in
//!                   0.1 ms; the theme rescan that follows cost 493–505 ms,
//!                   which is why roadmap 1.13 moved it to an idle callback
//!                   after the window is up. These now appear *after*
//!                   `window_shown`.
//!   pre_run       → last instant `main()` can time, because `app.run` never
//!                   returns. `window_shown - pre_run` is therefore the cost
//!                   of `AppModel::init` plus GTK's first realize.
//!   init_done     → the boundary between those two: `init_done - pre_run` is
//!                   `AppModel::init` building the widget tree, and
//!                   `window_shown - init_done` is GTK creating the surface.
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
use std::time::{Duration, Instant};

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

// ---------------------------------------------------------------------------
// UI-thread activity tracking (roadmap 7.1 step 0, 7.7 layer 2)
// ---------------------------------------------------------------------------
//
// The stall watchdog (`src/stall.rs`) has to answer "what was the UI thread
// doing when it blocked?" — and it must be able to answer on an ordinary
// launch, where `KALAM_TIMING` is unset and everything above is a no-op.
// So this half is deliberately NOT gated: route and dialog construction
// push a label onto a small stack, and the watchdog thread reads the top
// of it while the UI thread is blocked inside that work. The lock is held
// only for the microseconds of a push or pop, so it is free mid-block.
//
// The cost on a normal run is one `Instant::now()` and a `Vec` push per
// screen open — nothing per frame, nothing per query.

#[derive(Default)]
struct ActivityState {
    /// Currently-open activities, outermost first. In practice this is
    /// zero or one entry deep; it is a stack so a dialog opened from a
    /// route's message handler cannot corrupt its parent's label.
    stack: Vec<(&'static str, Instant)>,
    /// The most recent activity to finish, so a block that happens just
    /// *after* construction (widget realize, say) can still be blamed.
    last_ended: Option<(&'static str, Duration, Instant)>,
}

static ACTIVITY: OnceLock<Mutex<ActivityState>> = OnceLock::new();

fn activity_state() -> &'static Mutex<ActivityState> {
    ACTIVITY.get_or_init(|| Mutex::new(ActivityState::default()))
}

/// Marks the start of a named UI-thread activity — building a route or a
/// dialog. Dropping the guard ends it. Like [`measure`], bind it to a name:
/// `let _a = timing::activity("route_open:book");`.
#[must_use = "dropping this immediately pops the activity it pushed"]
pub struct ActivityGuard {
    label: &'static str,
}

impl Drop for ActivityGuard {
    fn drop(&mut self) {
        let mut state = activity_state().lock().expect("activity lock");
        // Remove the most recent entry with this label rather than a blind
        // pop: a mis-nested pair of guards must not delete some other
        // activity's label and mis-attribute a stall.
        if let Some(idx) = state.stack.iter().rposition(|(l, _)| *l == self.label) {
            let started = state.stack.remove(idx).1;
            let ended = Instant::now();
            state.last_ended = Some((self.label, started.elapsed(), ended));
        }
    }
}

/// Begin a named UI-thread activity. See [`ActivityGuard`]. Always on —
/// not gated by `KALAM_TIMING`, because the watchdog needs it precisely on
/// ordinary launches.
pub fn activity(label: &'static str) -> ActivityGuard {
    activity_state()
        .lock()
        .expect("activity lock")
        .stack
        .push((label, Instant::now()));
    ActivityGuard { label }
}

/// What the UI thread is doing right now, and for how long. Read by the
/// stall watchdog thread while the UI thread is blocked inside that work.
pub fn current_activity() -> Option<(&'static str, Duration)> {
    let state = activity_state()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    state
        .stack
        .last()
        .map(|(label, started)| (*label, started.elapsed()))
}

/// The most recent activity to finish: its label, how long it took, and how
/// long ago it ended. A poisoned lock would mean the process is already
/// unwinding; the watchdog should still be able to read, hence `into_inner`.
pub fn last_ended_activity() -> Option<(&'static str, Duration, Duration)> {
    let state = activity_state()
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    state
        .last_ended
        .map(|(label, took, ended)| (label, took, ended.elapsed()))
}

#[cfg(test)]
mod activity_tests {
    use super::{activity, current_activity, last_ended_activity, ActivityGuard};

    /// One test, not three: the activity state is a process-wide static and
    /// `cargo test` runs tests in parallel, so separate tests would race on
    /// "which activity is current" and flake (the same lesson as the
    /// query-count counter's thread-local in `db.rs`).
    #[test]
    fn open_nested_and_orphaned_activities_behave() {
        // Open → current; dropped → last-ended.
        let guard = activity("test:open");
        let (label, _) = current_activity().expect("activity should be current");
        assert_eq!(label, "test:open");
        drop(guard);
        assert!(current_activity().is_none());
        let (label, _, _) = last_ended_activity().expect("activity should be last-ended");
        assert_eq!(label, "test:open");

        // Nested: the inner is current, and ends without eating the outer's
        // label.
        let outer = activity("test:outer");
        let inner = activity("test:inner");
        let (label, _) = current_activity().expect("inner should be current");
        assert_eq!(label, "test:inner");
        drop(inner);
        let (label, _) = current_activity().expect("outer should be current again");
        assert_eq!(label, "test:outer");
        let (label, _, _) = last_ended_activity().expect("inner should be last-ended");
        assert_eq!(label, "test:inner");

        // An orphan guard (its entry already gone) must not remove some
        // other activity's entry.
        let orphan = ActivityGuard {
            label: "test:never-pushed",
        };
        drop(orphan);
        let (label, _) = current_activity().expect("real activity must survive");
        assert_eq!(label, "test:outer");
        drop(outer);
        assert!(current_activity().is_none());
    }
}
