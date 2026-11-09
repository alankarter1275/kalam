//! Who holds the catalogue's one connection lock, and for how long.
//!
//! The stall watchdog (`src/stall.rs`) can prove the screen froze and name the
//! activity that was open, but an activity label describes the UI thread's
//! *own* work. When the screen is frozen waiting for the database instead, the
//! label is whatever happened to run last, and it goes stale: the owner's log
//! blamed a 4.6 second block on a task that had finished 27 seconds earlier,
//! and the real holder was never named (pitfalls §109).
//!
//! This module answers the question the watchdog cannot. `Catalog::conn`
//! reports both ends of every acquisition:
//!
//! - **on the way in**, the moment it started waiting and the moment the lock
//!   landed. That gives the wait, and the record it overwrites gives the
//!   holder it waited for — so the job that froze the screen is named at the
//!   instant somebody else was made to wait for it.
//! - **on the way out**, when the guard drops, so the record says honestly
//!   whether the lock is held *right now*. That is what lets a stall line end
//!   with `catalogue lock: free` and point the hunt back at the UI thread's
//!   own work instead of at contention.
//!
//! The watchdog also asks the record on every poll, which produces a third
//! line: a hold that runs past [`LONG_HOLD`] is named once, whether or not
//! anything was waiting for it. A job that keeps the catalogue for ten seconds
//! is the freeze the next page build is going to suffer, and waiting for the
//! screen to actually freeze to hear about it is how §105 and §106 were both
//! found a season late.
//!
//! And every stall line carries the screen's *accumulated* wait, because a
//! threshold per wait cannot see a convoy: twenty waits of 20 ms freeze the
//! screen for 400 ms and not one of them is long enough to report itself. The
//! first run with this instrument showed exactly that — every contended stall
//! in CI named `"Checking dictionary packs"` holding the lock for 0 to 15 ms,
//! with no single wait worth a line.
//!
//! ## Cost, stated because it sits on the hot path
//!
//! Every read of the catalogue goes through `conn()`, so the record has to be
//! cheaper than the query it measures: one monotonic clock read and two
//! relaxed atomic stores on the way in, one compare-exchange on the way out.
//! The thread-name table is a `Mutex`, but it is taken once per thread and
//! again only while a line is actually being printed, and it is never held
//! across a catalogue call — so it cannot join the connection lock in a
//! cycle, which is the shape that froze startup in §106.
//!
//! One cost is stated rather than hidden: a reported wait writes its line
//! while the waiter is holding the lock it just got, because that is the only
//! moment the previous holder is still on the record. That is one file append
//! on a path that has already waited at least 100 ms, at most four times a
//! second, and 400 times per run.
//!
//! ## Where the lines go
//!
//! The same two places as `[stall]`: stderr, and appended to
//! `~/.local/share/kalam/kalam.log`, tagged `[lock]`. The CI smoke report
//! prints both tags in its stall section, so a contention regression shows up
//! in `ci-logs/` on the day it happens.

use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// A wait at least this long earns a line when the *screen* is the waiter —
/// the watchdog's own threshold, because a screen made to wait this long for
/// the database is a stall by the definition `src/stall.rs` already uses.
const UI_WAIT: Duration = Duration::from_millis(100);

/// A worker waiting for the database is ordinary; that is what the lock is
/// for. A worker's wait only earns a line once it is this long, which is when
/// whoever holds the lock has a problem worth naming.
const WORKER_WAIT: Duration = Duration::from_secs(1);

/// At most one line per this often, so a bout of contention writes a paragraph
/// rather than a novel.
const MIN_GAP: Duration = Duration::from_millis(250);

/// A wait this long is news every time: it skips both the gap and the cap.
const ALWAYS: Duration = Duration::from_secs(1);

/// Lines per run, so pathological contention cannot fill the log file. A wait
/// over [`ALWAYS`] is printed regardless — the cap exists to stop chatter, not
/// to hide a multi-second freeze.
const MAX_LINES: u64 = 400;

/// A hold that lasts this long is named on its own, waited for or not. Two
/// seconds is the watchdog's own repeat interval for a frozen screen, so the
/// two lines arrive at the same cadence and read as one story.
const LONG_HOLD: Duration = Duration::from_secs(2);

/// A wait at least this long counts toward the screen's total. Shorter is the
/// lock doing its job; counting it would turn the number into a measure of how
/// often the screen reads the catalogue rather than of how long it was made to
/// wait.
const COUNTED_WAIT: Duration = Duration::from_millis(1);

/// The clock every number here is measured against: microseconds since the
/// first call. `Instant` cannot live in an atomic, and these records are read
/// by threads other than the one that wrote them.
static EPOCH: OnceLock<Instant> = OnceLock::new();

/// Thread number holding the lock right now; zero when nobody does.
///
/// Zero is also "nothing recorded yet", which is the same thing: before the
/// first acquisition, nobody holds it.
static HOLDER: AtomicU64 = AtomicU64::new(0);

/// When the current holder took the lock, in [`now_micros`] units.
static HELD_SINCE: AtomicI64 = AtomicI64::new(0);

/// The thread that owns the UI loop, recorded by `main()`. Zero until then, so
/// a wait during startup is treated as a worker's — it is not the screen's loop
/// that is blocked at that point, the loop has not started.
static UI_THREAD: AtomicU64 = AtomicU64::new(0);

/// When the last line was printed, and how many have been printed.
///
/// The epoch starts at zero, so a zero here would put the first line inside
/// the gap it is meant to enforce and swallow any wait in the first quarter
/// second of a run — which is exactly when startup contention happens. A
/// thousand seconds before the epoch is far enough back to mean "never".
static LAST_LINE: AtomicI64 = AtomicI64::new(-1_000_000_000);
static LINES: AtomicU64 = AtomicU64::new(0);

/// Thread numbers, handed out in order.
static NEXT_THREAD: AtomicU64 = AtomicU64::new(1);

/// The `HELD_SINCE` value the last long-hold line reported, so one hold earns
/// one line rather than one per poll. Minus one because a real `since` starts
/// at zero and must never be mistaken for "already reported".
static REPORTED_HOLD: AtomicI64 = AtomicI64::new(-1);

/// What the screen has spent waiting for the catalogue since a stall line last
/// read it: total microseconds, and how many separate waits. Written by the
/// screen alone, read and reset by the watchdog, so no window mixes two
/// blocks. `swap` rather than a load-and-store: a wait landing between the two
/// belongs to the next window, which is where it would be reported anyway.
static SCREEN_WAIT_TOTAL: AtomicI64 = AtomicI64::new(0);
static SCREEN_WAIT_COUNT: AtomicU64 = AtomicU64::new(0);

/// Number to name. Written once per thread and once more if that thread names
/// its own job later; read only while a line is being printed.
static NAMES: OnceLock<Mutex<Vec<(u64, String)>>> = OnceLock::new();

thread_local! {
    /// This thread's number, zero until it is registered.
    static ME: std::cell::Cell<u64> = const { std::cell::Cell::new(0) };
}

/// Microseconds on the shared clock. One `Instant` read; the epoch is written
/// once by whichever thread gets here first.
pub(crate) fn now_micros() -> i64 {
    EPOCH.get_or_init(Instant::now).elapsed().as_micros() as i64
}

/// Whole milliseconds, for the lines people read.
fn millis(micros: i64) -> i64 {
    micros / 1000
}

/// Record this thread as the one that owns the UI loop. `main()` calls it next
/// to `tasks::note_main_thread()`, before the catalogue is opened.
pub(crate) fn note_ui_thread() {
    let n = me();
    UI_THREAD.store(n, Ordering::Relaxed);
    insert_name(n, "the UI thread");
}

/// Name the job this thread is running.
///
/// `tasks.rs` calls it as a worker's first act, with the task's own label, so
/// a line reads `"Organizing comic series folders"` instead of `an unnamed
/// thread (#7)`. A thread that never calls it is still recorded: it takes a
/// number the first time it reaches for the lock, and is named after the
/// thread's OS name if it has one.
pub(crate) fn name_this_thread(label: &str) {
    insert_name(me(), label);
}

/// Called by `Catalog::conn` once the lock is in hand: report the wait this
/// thread just served, then record it as the new holder.
///
/// `started` is the [`now_micros`] reading taken before the blocking call, so
/// an uncontended acquisition measures the lock itself — tens of nanoseconds,
/// never worth a line.
pub(crate) fn note_acquire(started: i64) {
    let now = now_micros();
    let waiter = me();
    let screen = waiter == UI_THREAD.load(Ordering::Relaxed);
    let waited = now - started;
    // Read the old record before overwriting it: that is the holder this
    // thread waited for, and the only witness to how long it held on.
    let holder = HOLDER.load(Ordering::Relaxed);
    let since = HELD_SINCE.load(Ordering::Relaxed);
    HOLDER.store(waiter, Ordering::Relaxed);
    HELD_SINCE.store(now, Ordering::Relaxed);

    // The screen's waits are counted whether or not any of them is long
    // enough to earn its own line: a convoy is only visible as a total.
    if screen && waited >= micros(COUNTED_WAIT) {
        SCREEN_WAIT_TOTAL.fetch_add(waited, Ordering::Relaxed);
        SCREEN_WAIT_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    // Nobody held it, so there was nothing to wait for — the ordinary case.
    if holder == 0 {
        return;
    }
    report_wait(waiter, waited, holder, now - since, now, screen);
}

/// Called when the guard drops. The compare-exchange matters: a thread that
/// released long ago must not erase the record of whoever holds it now.
pub(crate) fn note_release() {
    let n = me();
    let _ = HOLDER.compare_exchange(n, 0, Ordering::Relaxed, Ordering::Relaxed);
}

/// What the stall watchdog appends to its line, read while the block is still
/// in progress. Two atomics and, when the lock is held, one short table scan.
pub(crate) fn catalogue_state() -> String {
    let holder = HOLDER.load(Ordering::Relaxed);
    let state = if holder == 0 {
        free_line()
    } else {
        let held = now_micros() - HELD_SINCE.load(Ordering::Relaxed);
        held_line(&who(holder), held)
    };
    // Drained here, so the number belongs to this block and the resumed line
    // that follows reports only what came after it.
    format!("{state}{}", screen_wait_summary())
}

/// What the screen spent waiting for the catalogue since the last line that
/// asked. Empty when the window holds nothing, so a healthy run's lines read
/// exactly as they did before this module existed.
pub(crate) fn screen_wait_summary() -> String {
    let (total, count) = drain_screen_wait();
    if count == 0 {
        return String::new();
    }
    wait_summary(total, count)
}

/// Read the screen's accumulated wait and close the window. The watchdog also
/// calls this whenever the heartbeat advances, so waits from a healthy moment
/// never ride along on the next stall's line.
pub(crate) fn drain_screen_wait() -> (i64, u64) {
    let total = SCREEN_WAIT_TOTAL.swap(0, Ordering::Relaxed);
    let count = SCREEN_WAIT_COUNT.swap(0, Ordering::Relaxed);
    (millis(total), count)
}

/// The accumulated-wait tail, on its own so the wording can be pinned.
fn wait_summary(total: i64, count: u64) -> String {
    format!("; the screen waited {total} ms for the catalogue lock in {count} separate waits")
}

/// Called by the stall watchdog on every poll: name a hold that has run past
/// [`LONG_HOLD`], once per hold.
///
/// This is the half that does not need a victim. A worker indexing a large
/// folder can keep the catalogue for seconds while the screen happens to be
/// idle, and the only evidence of that would otherwise be the freeze it causes
/// later, blamed on whatever activity label was freshest at the time.
pub(crate) fn note_long_hold() {
    let holder = HOLDER.load(Ordering::Relaxed);
    if holder == 0 {
        return;
    }
    let since = HELD_SINCE.load(Ordering::Relaxed);
    let reported = REPORTED_HOLD.load(Ordering::Relaxed);
    let now = now_micros();
    if !worth_a_hold_line(now - since, reported, since) {
        return;
    }
    REPORTED_HOLD.store(since, Ordering::Relaxed);
    crate::stall::emit("lock", &hold_line(&who(holder), now - since));
}

/// Whether one poll should name a hold: it has run long, and this particular
/// hold has not been named yet.
fn worth_a_hold_line(held: i64, reported_since: i64, since: i64) -> bool {
    since != reported_since && held >= micros(LONG_HOLD)
}

/// One `[lock]` line for a hold that has run long, waited for or not.
fn hold_line(holder: &str, held: i64) -> String {
    format!(
        "{holder} has held the catalogue lock for {} ms",
        millis(held)
    )
}

/// The tail of a stall line when nothing holds the catalogue lock — which is
/// the answer that sends the hunt back to the UI thread's own work.
fn free_line() -> String {
    " — catalogue lock: free".to_string()
}

/// The tail of a stall line when something holds it, naming the holder.
fn held_line(holder: &str, held: i64) -> String {
    format!(
        " — catalogue lock: held by {holder} for {} ms",
        millis(held)
    )
}

/// One `[lock]` line: who waited, how long, and who they waited for.
///
/// The holder's duration is a lower bound and says so — the record holds when
/// the holder *took* the lock, not when it let go, and by the time the waiter
/// prints, the holder may have released a moment ago or may still be inside.
fn wait_line(waiter: &str, waited: i64, holder: &str, held: i64) -> String {
    format!(
        "{waiter} waited {} ms for the catalogue lock — {holder} had held it \
         for at least {} ms",
        millis(waited),
        millis(held)
    )
}

/// Whether one wait earns a line.
///
/// Pure and separate from the printing so the policy can be tested without
/// touching the process-wide records — the shape `tasks.rs` uses for
/// `may_deliver`, for the same reason: `cargo test` runs tests in parallel,
/// and a test that asserted on the shared holder record would be asserting on
/// whatever the neighbouring database tests happened to be doing.
fn worth_reporting(waited: i64, screen: bool, since_last_line: i64, printed: u64) -> bool {
    let floor = if screen { UI_WAIT } else { WORKER_WAIT };
    if waited < micros(floor) {
        return false;
    }
    // A multi-second wait is the whole point of the instrument: neither the
    // gap nor the cap may swallow it.
    if waited >= micros(ALWAYS) {
        return true;
    }
    if since_last_line < micros(MIN_GAP) {
        return false;
    }
    printed < MAX_LINES
}

fn micros(d: Duration) -> i64 {
    d.as_micros() as i64
}

/// Print one wait, if [`worth_reporting`] says it earned the line.
fn report_wait(waiter: u64, waited: i64, holder: u64, held: i64, now: i64, screen: bool) {
    let since_last_line = now - LAST_LINE.load(Ordering::Relaxed);
    let printed = LINES.load(Ordering::Relaxed);
    if !worth_reporting(waited, screen, since_last_line, printed) {
        return;
    }
    LAST_LINE.store(now, Ordering::Relaxed);
    LINES.store(printed + 1, Ordering::Relaxed);
    crate::stall::emit("lock", &wait_line(&who(waiter), waited, &who(holder), held));
}

/// How a thread reads in a line: the screen as prose, a job as a quoted label.
fn who(n: u64) -> String {
    let name = name_of(n);
    if n == UI_THREAD.load(Ordering::Relaxed) {
        name
    } else {
        format!("\"{name}\"")
    }
}

/// This thread's number, registering it the first time it is asked.
fn me() -> u64 {
    ME.with(|slot| {
        if slot.get() == 0 {
            let n = NEXT_THREAD.fetch_add(1, Ordering::Relaxed);
            slot.set(n);
            insert_name(n, &default_name());
        }
        slot.get()
    })
}

/// The name for a thread nobody introduced: its OS name if it was given one,
/// which is what makes an unlabelled worker still findable in `top -H`.
fn default_name() -> String {
    match std::thread::current().name() {
        Some(name) => name.to_string(),
        None => "an unnamed thread".to_string(),
    }
}

fn table() -> &'static Mutex<Vec<(u64, String)>> {
    NAMES.get_or_init(|| Mutex::new(Vec::new()))
}

/// Write one thread's name. A poisoned table — a panic in the middle of a
/// lookup, which can only come from an allocation failing — loses names, not
/// correctness: the numbers still identify the threads, so the lines still
/// say who waited for whom.
fn insert_name(n: u64, label: &str) {
    let mut names = match table().lock() {
        Ok(names) => names,
        Err(poisoned) => poisoned.into_inner(),
    };
    match names.iter_mut().find(|(id, _)| *id == n) {
        Some(slot) => slot.1 = label.to_string(),
        None => names.push((n, label.to_string())),
    }
}

fn name_of(n: u64) -> String {
    if n == 0 {
        return "nobody".to_string();
    }
    let names = match table().lock() {
        Ok(names) => names,
        Err(poisoned) => poisoned.into_inner(),
    };
    match names.iter().find(|(id, _)| *id == n) {
        Some((_, label)) => label.clone(),
        None => format!("an unnamed thread (#{n})"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The policy is the part worth pinning: too eager and every ordinary
    /// worker wait fills the log, too shy and the freeze that prompted this
    /// module stays unnamed.
    #[test]
    fn the_reporting_policy_names_a_frozen_screen_and_not_an_ordinary_worker() {
        let ms = |n: i64| n * 1000;

        assert!(
            !worth_reporting(ms(20), true, ms(1000), 0),
            "a screen waiting 20 ms is inside the watchdog's own budget"
        );
        assert!(
            worth_reporting(ms(150), true, ms(1000), 0),
            "a screen waiting 150 ms is a stall by the definition stall.rs uses"
        );
        assert!(
            !worth_reporting(ms(150), false, ms(1000), 0),
            "a worker waiting 150 ms is what the lock is for"
        );
        assert!(
            worth_reporting(ms(1400), false, ms(1000), 0),
            "a worker waiting over a second names a holder worth naming"
        );
    }

    /// The gap and the cap exist to stop chatter. Neither may swallow the
    /// multi-second wait the instrument was built for — that is the line the
    /// owner's 4657 ms block would have produced.
    #[test]
    fn the_rate_limit_suppresses_chatter_but_never_a_long_wait() {
        let ms = |n: i64| n * 1000;

        assert!(
            !worth_reporting(ms(150), true, ms(10), 0),
            "a second line 10 ms after the first is chatter"
        );
        assert!(
            worth_reporting(ms(4657), true, ms(10), 0),
            "a 4.6 second wait is printed even straight after another line"
        );
        assert!(
            !worth_reporting(ms(150), true, ms(1000), MAX_LINES),
            "the cap stops a contention storm from filling the log"
        );
        assert!(
            worth_reporting(ms(1400), false, ms(10), MAX_LINES),
            "the cap does not hide a wait of over a second"
        );
    }

    /// Both halves of a line have to be readable on their own: the waiter is
    /// the thing that was frozen, the holder is the thing to go and fix.
    #[test]
    fn a_wait_line_names_both_sides_in_whole_milliseconds() {
        let line = wait_line(
            "the UI thread",
            4_657_400,
            "\"Organizing comic series folders\"",
            4_702_900,
        );
        assert_eq!(
            line,
            "the UI thread waited 4657 ms for the catalogue lock — \
             \"Organizing comic series folders\" had held it for at least 4702 ms"
        );
    }

    /// A long hold is named once, and only once it is long: one line per
    /// hold, however many polls the watchdog makes while it lasts.
    #[test]
    fn a_long_hold_is_named_once_and_not_before_it_is_long() {
        let ms = |n: i64| n * 1000;

        assert!(
            !worth_a_hold_line(ms(1500), -1, ms(9_000)),
            "a second and a half is inside what a big library legitimately needs"
        );
        assert!(
            worth_a_hold_line(ms(2500), -1, ms(9_000)),
            "past two seconds the holder is the thing to go and fix"
        );
        assert!(
            !worth_a_hold_line(ms(9_000), ms(9_000), ms(9_000)),
            "the same hold is not named again on the next poll"
        );
        assert!(
            worth_a_hold_line(ms(2500), ms(9_000), ms(12_000)),
            "a later hold is a different hold, and gets its own line"
        );
        assert_eq!(
            hold_line("\"Rebuilding cover thumbnails\"", 10_400_900),
            "\"Rebuilding cover thumbnails\" has held the catalogue lock for 10400 ms"
        );
    }

    /// The convoy number has to read as a total, not as one wait: it is the
    /// answer to "was the screen frozen *on the database*", which no single
    /// hold in the first instrumented run was long enough to answer.
    #[test]
    fn the_accumulated_wait_reads_as_a_total() {
        assert_eq!(
            wait_summary(412, 27),
            "; the screen waited 412 ms for the catalogue lock in 27 separate waits"
        );
        assert_eq!(screen_wait_summary(), "", "an empty window adds nothing");
    }

    /// The stall watchdog's tail is the other half of the instrument: "free"
    /// is as much an answer as a name, because it rules contention out and
    /// sends the hunt back to the UI thread's own work.
    #[test]
    fn the_stall_tail_says_whether_the_lock_was_held() {
        assert_eq!(free_line(), " — catalogue lock: free");
        assert_eq!(
            held_line("\"Preloading covers\"", 4_702_900),
            " — catalogue lock: held by \"Preloading covers\" for 4702 ms"
        );
    }
}
