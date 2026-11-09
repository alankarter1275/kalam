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
//!   landed. That gives the wait, and the hold that has just *finished* gives
//!   the job it waited for — named at the instant somebody else was made to
//!   wait for it. Not the record of who holds it now: a release clears that,
//!   so by the time the lock lands it always reads "nobody", which is the bug
//!   that made the first version of this module report nothing at all.
//! - **on the way out**, when the guard drops and while the mutex is still
//!   this thread's. That writes the hold's exact length where the next thread
//!   in can read it, and clears the live record — which is what lets a stall
//!   line end with `catalogue lock: free` and point the hunt back at the UI
//!   thread's own work instead of at contention.
//!
//! The watchdog also asks the record on every poll, which produces a third
//! line: a hold that runs past [`LONG_HOLD`] is named once, whether or not
//! anything was waiting for it. A job that keeps the catalogue for ten seconds
//! is the freeze the next page build is going to suffer, and waiting for the
//! screen to actually freeze to hear about it is how §105 and §106 were both
//! found a season late.
//!
//! ## The screen goes first
//!
//! Recording who holds the lock produced an answer nobody had guessed. The
//! screen's waits in CI ran 370 to 625 ms, and the hold each one waited out
//! was 0 to 4 ms long: `std::sync::Mutex` on Linux gives the lock to whoever
//! asks next, not to whoever has waited longest, so two background jobs taking
//! it a hundred times each kept the screen's single request out for half a
//! second. Naming that is not enough, so the screen raises a flag before it
//! blocks and a worker that sees the flag sleeps instead of asking. Bounded,
//! so a flag left set by a bug costs the workers 50 ms and not the session.
//!
//! Every wait line also counts the handovers it waited through, because that
//! number is the difference between a queue (many short holds — give way and
//! it goes away) and a thread that was simply not scheduled (no holds at all —
//! which no lock record can fix, and which would be a lie to pretend
//! otherwise).
//!
//! And every stall line carries the screen's *accumulated* wait, because a
//! threshold per wait cannot see a convoy: twenty waits of 20 ms freeze the
//! screen for 400 ms and not one of them is long enough to report itself. The
//! convoy turned out not to be the shape CI had — the accumulated number said
//! the screen spent 427 to 1110 ms of its blocks in *one* wait each, which is
//! one long hold, and the reason no `[lock]` line named it was the bug above.
//! The counter stays: it is what turned "somebody held the lock for 5 ms when
//! I looked" into a number that could not be argued with.
//!
//! ## Cost, stated because it sits on the hot path
//!
//! Every read of the catalogue goes through `conn()`, so the record has to be
//! cheaper than the query it measures: one monotonic clock read and two
//! relaxed atomic stores on the way in, one compare-exchange, a second clock
//! read and two more stores on the way out. Tens of nanoseconds against a
//! query that costs microseconds.
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

use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU64, Ordering};
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

/// How long a worker gives way on one acquisition before taking the lock
/// anyway. Bounded so that a flag left set by a bug costs the workers 50 ms
/// rather than the rest of the session.
const YIELD_CAP: Duration = Duration::from_millis(50);

/// One step of that sleep. Two milliseconds is under the shortest hold the
/// instrument has seen, so a worker that gives way once misses nothing.
const YIELD_STEP: Duration = Duration::from_millis(2);

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

/// Set while the screen is waiting for the lock, so the workers can give way.
static SCREEN_WAITING: AtomicBool = AtomicBool::new(false);

/// How many times the lock has changed hands. A waiter reads it before and
/// after, and the difference is the queue it waited through.
static HANDOVERS: AtomicU64 = AtomicU64::new(0);

/// The last *completed* hold: who held it, and for exactly how long.
///
/// Written on the way out and read on the way in, because that is the only
/// pairing that works. A release clears [`HOLDER`], so the obvious place to
/// look for the thread this one waited for — after the lock is in hand — is
/// always empty, and the first version of this module reported nothing at all
/// for that reason: every wait in a CI run, including one of 1110 ms, read
/// "nobody held it" and stayed silent. Both writes happen while the releasing
/// thread still has the mutex, so the next thread in cannot see them half
/// done.
static LAST_HOLDER: AtomicU64 = AtomicU64::new(0);
static LAST_HOLD: AtomicI64 = AtomicI64::new(0);

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
fn now_micros() -> i64 {
    EPOCH.get_or_init(Instant::now).elapsed().as_micros() as i64
}

/// What `Catalog::conn` records before it blocks on the lock, and hands back
/// to [`note_acquire`] the moment the lock lands.
pub(crate) struct Waiting {
    started: i64,
    handovers: u64,
    /// Whether this thread raised the screen's flag, and so must lower it.
    raised: bool,
}

/// The whole of `conn()`'s business before the blocking call: note the clock
/// and the handover count, then either raise the screen's flag or give way to
/// it.
pub(crate) fn before_blocking() -> Waiting {
    let handovers = HANDOVERS.load(Ordering::Relaxed);
    let raised = if is_screen() {
        SCREEN_WAITING.store(true, Ordering::Relaxed);
        true
    } else {
        give_way();
        false
    };
    Waiting {
        started: now_micros(),
        handovers,
        raised,
    }
}

/// Whether this is the thread that owns the UI loop. Zero means `main()` has
/// not recorded it yet, so nothing is the screen — which is the right answer
/// during startup, when the loop has not begun and no flag should be raised.
fn is_screen() -> bool {
    let ui = UI_THREAD.load(Ordering::Relaxed);
    ui != 0 && me() == ui
}

/// A worker's half of the handover: while the screen is waiting, sleep instead
/// of asking for the lock, so the screen's wait ends with the current holder
/// rather than with the whole queue behind it.
fn give_way() {
    let mut yielded = 0;
    while keep_yielding(SCREEN_WAITING.load(Ordering::Relaxed), yielded) {
        std::thread::sleep(YIELD_STEP);
        yielded += micros(YIELD_STEP);
    }
}

/// Whether a worker should keep giving way. Pure so the cap can be pinned: an
/// unbounded sleep here would turn one stuck flag into a frozen library.
fn keep_yielding(screen_waiting: bool, yielded: i64) -> bool {
    screen_waiting && yielded < micros(YIELD_CAP)
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
/// [`Waiting`] carries the reading taken before the blocking call, so an
/// uncontended acquisition measures the lock itself — tens of nanoseconds,
/// never worth a line.
pub(crate) fn note_acquire(waiting: Waiting) {
    // Lowered before anything else: every microsecond this stays set is a
    // microsecond the workers are asleep on the screen's behalf.
    if waiting.raised {
        SCREEN_WAITING.store(false, Ordering::Relaxed);
    }
    let now = now_micros();
    let waiter = me();
    let screen = waiter == UI_THREAD.load(Ordering::Relaxed);
    let waited = now - waiting.started;
    // The handovers this thread waited through, not counting its own.
    let others = HANDOVERS.fetch_add(1, Ordering::Relaxed) - waiting.handovers;
    // The hold this thread waited out is the one that has just finished, not
    // the one on `HOLDER` — that is always clear by now, see `LAST_HOLDER`.
    let holder = LAST_HOLDER.load(Ordering::Relaxed);
    let held = LAST_HOLD.load(Ordering::Relaxed);
    HOLDER.store(waiter, Ordering::Relaxed);
    HELD_SINCE.store(now, Ordering::Relaxed);

    // The screen's waits are counted whether or not any of them is long
    // enough to earn its own line: a convoy is only visible as a total.
    if screen && waited >= micros(COUNTED_WAIT) {
        SCREEN_WAIT_TOTAL.fetch_add(waited, Ordering::Relaxed);
        SCREEN_WAIT_COUNT.fetch_add(1, Ordering::Relaxed);
    }

    // No hold has ever finished, so this is the first acquisition of the run
    // and there was nothing to wait for.
    if holder == 0 {
        return;
    }
    report_wait(waiter, waited, others, holder, held, now, screen);
}

/// Called when the guard drops, which is before the mutex itself lets go: the
/// thread still has it, so the record it leaves behind is complete by the time
/// anybody else can read it.
///
/// The compare-exchange matters twice over — a thread that released long ago
/// must not erase the record of whoever holds it now, and must not overwrite
/// the finished hold that thread is about to hand on.
pub(crate) fn note_release() {
    let n = me();
    let released = HOLDER.compare_exchange(n, 0, Ordering::Relaxed, Ordering::Relaxed);
    if released.is_err() {
        // Somebody else holds it already: a late release must neither erase
        // their record nor hand on a hold that was not this thread's.
        return;
    }
    let held = now_micros() - HELD_SINCE.load(Ordering::Relaxed);
    LAST_HOLDER.store(n, Ordering::Relaxed);
    LAST_HOLD.store(held, Ordering::Relaxed);
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
    let shape = if count == 1 {
        "1 wait".to_string()
    } else {
        format!("{count} separate waits")
    };
    format!("; the screen waited {total} ms for the catalogue lock in {shape}")
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

/// One `[lock]` line: who waited, how long, how many handovers they waited
/// through, and who held it last.
///
/// The holder's duration is exact rather than a bound, because it is written
/// by the holder on its way out and read by the waiter on its way in. The
/// handover count is what separates the two shapes a long wait can have: a
/// queue of short holds, which giving way fixes, and no holds at all, which
/// means the thread was not scheduled and no lock record can help it.
fn wait_line(waiter: &str, waited: i64, others: u64, holder: &str, held: i64) -> String {
    let waited_ms = millis(waited);
    let held_ms = millis(held);
    match others {
        0 => format!(
            "{waiter} waited {waited_ms} ms for the catalogue lock — {holder} held it \
             for {held_ms} ms and nothing else took it in between"
        ),
        1 => format!(
            "{waiter} waited {waited_ms} ms for the catalogue lock — 1 other hold in \
             between, the last by {holder} for {held_ms} ms"
        ),
        _ => format!(
            "{waiter} waited {waited_ms} ms for the catalogue lock — {others} other \
             holds in between, the last by {holder} for {held_ms} ms"
        ),
    }
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
fn report_wait(
    waiter: u64,
    waited: i64,
    others: u64,
    holder: u64,
    held: i64,
    now: i64,
    screen: bool,
) {
    let since_last_line = now - LAST_LINE.load(Ordering::Relaxed);
    let printed = LINES.load(Ordering::Relaxed);
    if !worth_reporting(waited, screen, since_last_line, printed) {
        return;
    }
    LAST_LINE.store(now, Ordering::Relaxed);
    LINES.store(printed + 1, Ordering::Relaxed);
    let line = wait_line(&who(waiter), waited, others, &who(holder), held);
    crate::stall::emit("lock", &line);
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
    /// the thing that was frozen, the holder is the thing to go and fix. The
    /// handover count is the third thing, and it is what says whether the wait
    /// was a queue or a single hold.
    #[test]
    fn a_wait_line_names_both_sides_and_the_queue_between_them() {
        let queue = wait_line(
            "the UI thread",
            4_657_400,
            214,
            "\"Organizing comic series folders\"",
            4_702_900,
        );
        assert_eq!(
            queue,
            "the UI thread waited 4657 ms for the catalogue lock — 214 other \
             holds in between, the last by \"Organizing comic series folders\" \
             for 4702 ms"
        );

        let single = wait_line(
            "the UI thread",
            620_000,
            1,
            "\"Checking dictionary packs\"",
            4_100,
        );
        assert_eq!(
            single,
            "the UI thread waited 620 ms for the catalogue lock — 1 other hold in \
             between, the last by \"Checking dictionary packs\" for 4 ms"
        );

        let alone = wait_line(
            "the UI thread",
            620_000,
            0,
            "\"Checking dictionary packs\"",
            604_000,
        );
        assert_eq!(
            alone,
            "the UI thread waited 620 ms for the catalogue lock — \
             \"Checking dictionary packs\" held it for 604 ms and nothing else \
             took it in between",
            "no handovers means one hold, and the line must not imply a queue"
        );
    }

    /// Giving way has to stop. A worker that sleeps while a flag is set is one
    /// stuck flag away from a library that never loads, so the cap is the
    /// whole safety of the idea and it is pinned here.
    #[test]
    fn a_worker_gives_way_until_the_screen_is_through_or_the_cap_says_stop() {
        let ms = |n: i64| n * 1000;

        assert!(keep_yielding(true, 0), "the screen is waiting, so sleep");
        assert!(
            keep_yielding(true, ms(48)),
            "still under the cap, still giving way"
        );
        assert!(
            !keep_yielding(true, ms(50)),
            "at the cap a worker takes the lock anyway"
        );
        assert!(
            !keep_yielding(false, 0),
            "with the screen through, a worker does not sleep at all"
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
        assert_eq!(
            wait_summary(777, 1),
            "; the screen waited 777 ms for the catalogue lock in 1 wait",
            "one wait is not several, and the line should not say so"
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
