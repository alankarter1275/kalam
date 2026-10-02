//! Main-thread stall watchdog — roadmap 7.1 step 0 (measurement) and 7.7
//! layer 2 (enforcement), owner-approved 2026-10-02, output log-only.
//!
//! The owner's principle: "the UI thread never touches disk, database, or
//! parsing." This module is the instrument that catches every violation of
//! it as it happens, instead of waiting for a field report that "the app
//! feels slow".
//!
//! ## How it works
//!
//! A GLib timeout source on the main loop bumps an atomic counter every
//! 50 ms — twenty `fetch_add`s a second, unmeasurable. A watcher thread
//! polls that counter. If it does not advance, the main loop is blocked,
//! and the watcher can say two things no profiler run can:
//!
//! 1. **How long** — reported honestly as a *lower bound* ("at least N
//!    ms"), because the block began somewhere within the last healthy
//!    heartbeat window.
//! 2. **Doing what** — `timing::current_activity()` names the activity
//!    open while the block is still in progress (the activity lock is
//!    only held for microseconds around push/pop, so it is readable
//!    mid-block): a route or dialog being constructed, a page's apply
//!    rebuild, a task's main-thread callback, a named idle job. If no
//!    span is open, the last one that ended is named — a block just
//!    after construction is usually realization of the widgets that
//!    construction built.
//!
//! Two lines per stall: one when the silent window first proves a block of
//! ≥ 100 ms (so a *permanent* freeze still gets its line), one when the
//! loop resumes with the final lower bound. The accounting is deliberately
//! conservative — one heartbeat period of grace, so a line can never cry
//! wolf on ordinary main-loop batching — which means a block in the 100–150
//! ms range may slip through unreported. Blocks under 100 ms are ignored:
//! that is the budget, not a defect.
//!
//! ## Where the lines go
//!
//! stderr, and appended to `~/.local/share/kalam/kalam.log`. The file is
//! created by this event and only by this event: an ordinary launch with
//! no stalls still writes no log file, which is the contract `logging.rs`
//! documents. (The `log` facade is no use here — no logger is installed
//! unless `RUST_LOG` is set, and the watchdog must work on ordinary
//! launches.)
//!
//! In CI, the smoke session captures stderr into its `kalam.log` and the
//! report carries every `[stall]` line, so a regression shows up in
//! `ci-logs/` on the day it happens.
//!
//! ## Known limits, stated honestly
//!
//! - A block shorter than the poll interval plus grace can slip through
//!   unreported; the watchdog is a tripwire, not an oscilloscope.
//! - At process teardown the main loop stops while the process lives a
//!   moment longer; if that window crosses the threshold, one
//!   uninteresting teardown line may appear. Read end-of-run lines with
//!   that in mind.
//! - It sees duration and the active span, not a stack trace. Naming the
//!   screen is what makes the fix findable; the span timing lines under
//!   `KALAM_TIMING=1` then say which read inside it was slow.

use std::io::Write;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

/// How often the main loop must prove it is alive.
const HEARTBEAT: Duration = Duration::from_millis(50);
/// How often the watcher thread checks that proof.
const POLL: Duration = Duration::from_millis(100);
/// A silent window must prove at least this much blocked time before it is
/// called a stall. The owner's rule of thumb from 7.7: "logs any UI-loop
/// block over ~100 ms".
const THRESHOLD: Duration = Duration::from_millis(100);

static HEARTBEATS: AtomicU64 = AtomicU64::new(0);

/// Install the watchdog: the main-loop heartbeat source plus the watcher
/// thread. Call once, on the main thread, after GTK is initialized —
/// `stall::install()` in `main()`, right after `RelmApp::new`.
///
/// The watcher does not judge anything until it has seen the *first*
/// heartbeat, so the whole of startup before the loop runs (splash, catalog
/// open, `AppModel::init`) is not mistaken for a stall; the startup spans
/// in `timing.rs` already measure that part.
pub fn install() {
    gtk::glib::timeout_add_local(HEARTBEAT, || {
        HEARTBEATS.fetch_add(1, Ordering::Relaxed);
        gtk::glib::ControlFlow::Continue
    });
    let _ = std::thread::Builder::new()
        .name("kalam-stall-watchdog".to_string())
        .spawn(watch);
}

/// The watcher thread's whole life: sleep, check, report.
fn watch() {
    let mut last_value = HEARTBEATS.load(Ordering::Relaxed);
    let mut last_advance = Instant::now();
    // False until the first heartbeat is seen: the loop not having started
    // yet is not a stall.
    let mut loop_seen = false;
    let mut reported = false;

    loop {
        std::thread::sleep(POLL);
        let value = HEARTBEATS.load(Ordering::Relaxed);
        if value != last_value {
            if reported {
                report(&format!(
                    "UI thread unblocked — was blocked for at least {} ms",
                    millis(last_advance.elapsed().saturating_sub(HEARTBEAT))
                ));
                reported = false;
            }
            last_value = value;
            last_advance = Instant::now();
            loop_seen = true;
        } else if loop_seen {
            let silent = last_advance.elapsed();
            // One heartbeat period of grace: a healthy loop bumps the
            // counter every 50 ms, so "silent for 150 ms" proves a block
            // of at least 100 ms.
            if silent >= THRESHOLD + HEARTBEAT && !reported {
                reported = true;
                report(&format!(
                    "UI thread blocked for at least {} ms (still blocked)",
                    millis(silent.saturating_sub(HEARTBEAT))
                ));
            }
        }
    }
}

/// Whole milliseconds, floored, for the report lines.
fn millis(d: Duration) -> u64 {
    d.as_millis() as u64
}

/// One report line: to stderr and to `kalam.log`, with attribution.
fn report(headline: &str) {
    let attribution = attribution();
    let line = format!("[stall] {headline}{attribution}");
    eprintln!("{line}");

    // Best-effort, like `logging::Tee`: a file that cannot be opened must
    // not turn a performance report into an error path.
    let path = crate::paths::log_file();
    if let Some(mut file) = crate::logging::open_log_file(&path) {
        let _ = writeln!(file, "{line}");
    }
}

/// Name what the UI thread was doing, for the report line.
fn attribution() -> String {
    if let Some((label, open_for)) = crate::timing::current_activity() {
        return format!(" — while {label} (open {} ms)", millis(open_for));
    }
    if let Some((label, took, ago)) = crate::timing::last_ended_activity() {
        return format!(
            " — after {label} (took {} ms, ended {} ms ago)",
            millis(took),
            millis(ago)
        );
    }
    // Not just routes and dialogs any more: task callbacks, the apply
    // paths and the idle work all carry activity labels now (2a.1), so
    // an unnamed block is genuinely unknown — the log says so plainly.
    " — no activity was open".to_string()
}
