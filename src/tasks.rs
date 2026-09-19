//! Background tasks: one place to run slow work off the UI thread.
//!
//! # Why this exists
//!
//! Kalam had four different ways of doing background work — a bare
//! `thread::spawn`, `thread::spawn` plus an `async_channel`, relm4's
//! `spawn_command`, and "just do it on the UI thread and hope it is quick".
//! The last one is not a strategy: importing a dictionary froze the window for
//! as long as the parse took, with nothing on screen to say why.
//!
//! This module is A0 step 4. It gives that work a single shape: a worker
//! thread for the slow part, progress reported as it goes, a result delivered
//! back **on the main thread**, and a cancellation flag the work can check.
//!
//! # The rule it enforces
//!
//! GTK widgets and [`crate::notify`] belong to the main thread. A worker must
//! never touch them. So the closure that *does* the work is `Send` and gets no
//! access to the UI, while the closures that report progress and handle the
//! result are **not** `Send` — they are only ever run on the main thread, so
//! they can freely touch widgets and raise toasts.
//!
//! That split is the whole point: the type system stops you handing a widget
//! to a worker thread, which is a bug you otherwise find by crashing.
//!
//! # The registry
//!
//! Every running task is recorded with a **label** and its most recent
//! progress, so the task manager (roadmap 1.14) can show what is happening and
//! cancel one task rather than all of them. Before this, the registry held
//! only an id and a flag — enough to `cancel_all()` at shutdown, which was
//! the only thing that ever called it, and no way to see the work at all.
//!
//! Progress is written to the registry **from the main thread**, by the reader
//! that is already receiving the updates. Workers never take this lock, so a
//! worker cannot block the UI by holding it and the UI cannot block a worker.
//!
//! # No tokio
//!
//! `thread::spawn` + `async-channel` + the GLib main loop, per the roadmap.
//! relm4 is already the actor framework; a second runtime would be a second
//! scheduler fighting the first.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

/// One running task, as the task manager sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TaskInfo {
    pub id: u64,
    /// What the task is doing, in words a reader would recognise —
    /// "Importing EPUBs", not "spawn at all_books.rs:83".
    pub label: String,
    /// Units finished, from the most recent [`Reporter::step`]. `0` for a task
    /// that has not reported yet.
    pub done: usize,
    /// Total units, or `0` when the task cannot know it up front.
    pub total: usize,
    /// The detail string from the most recent report — a file name, a page.
    pub detail: String,
}

/// A task that has finished, kept briefly so the panel can show what just
/// happened rather than flickering to empty.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinishedTask {
    pub id: u64,
    pub label: String,
    /// Whether it stopped because it was asked to.
    pub cancelled: bool,
}

/// How many finished tasks the panel remembers. Bounded because this list is
/// only there to say "that just happened"; unbounded would grow for the life
/// of the process.
const FINISHED_LIMIT: usize = 20;

type Running = Vec<(TaskInfo, Arc<AtomicBool>)>;

/// Every task currently running, so the panel can list them and [`cancel`]
/// can reach one of them.
///
/// A plain `Vec` because there are single digits of these at once; a map would
/// be more code for no measurable gain.
type Registry = Mutex<Running>;

static RUNNING: OnceLock<Registry> = OnceLock::new();
static FINISHED: OnceLock<Mutex<VecDeque<FinishedTask>>> = OnceLock::new();
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

fn registry() -> &'static Registry {
    RUNNING.get_or_init(|| Mutex::new(Vec::new()))
}

fn finished() -> &'static Mutex<VecDeque<FinishedTask>> {
    FINISHED.get_or_init(|| Mutex::new(VecDeque::new()))
}

/// A lock that survives a panic in another task.
///
/// `Mutex::lock` returns `Err` once a holder has panicked, and the usual
/// `.expect()` would then turn one failed task into a crash on the next one.
/// The registry is a plain list of flags — a poisoned one is still perfectly
/// readable, so take the data and carry on.
fn locked<R>(f: impl FnOnce(&mut Running) -> R) -> R {
    let mut guard = match registry().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    f(&mut guard)
}

/// Same, for the finished list.
fn locked_finished<R>(f: impl FnOnce(&mut VecDeque<FinishedTask>) -> R) -> R {
    let mut guard = match finished().lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    f(&mut guard)
}

/// Handed to the worker closure. Its only two jobs are reporting progress and
/// answering "should I stop?".
///
/// Deliberately **not** `Sync`-friendly beyond what it needs: it carries a
/// channel and a flag, and no way to reach the UI.
pub struct Reporter {
    tx: async_channel::Sender<Update>,
    cancelled: Arc<AtomicBool>,
}

/// A progress report from a worker.
#[derive(Debug, Clone)]
pub struct Update {
    /// Units finished so far.
    pub done: usize,
    /// Total units, or `0` when the worker cannot know it up front.
    pub total: usize,
    /// What is happening right now — a file name, a dictionary name.
    pub detail: String,
}

impl Reporter {
    /// Report progress. Cheap and non-blocking; safe to call in a tight loop.
    ///
    /// The send is deliberately ignored on failure: the only way it fails is
    /// the UI side having gone away, and a worker should not care.
    pub fn step(&self, done: usize, total: usize, detail: impl Into<String>) {
        let _ = self.tx.send_blocking(Update {
            done,
            total,
            detail: detail.into(),
        });
    }

    /// Whether the task has been asked to stop.
    ///
    /// Cancellation is cooperative — nothing can safely kill a thread
    /// mid-write — so long-running work must check this between units and
    /// return early when it is true.
    pub fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::Relaxed)
    }
}

/// Record a progress report against a running task.
///
/// Called on the main thread by the reader in [`spawn`], never by a worker —
/// see the note on the registry at the top of this module.
fn set_progress(id: u64, update: &Update) {
    locked(|running| {
        if let Some((info, _)) = running.iter_mut().find(|(info, _)| info.id == id) {
            info.done = update.done;
            info.total = update.total;
            info.detail = update.detail.clone();
        }
    });
}

/// Move a task from running to finished.
fn finish(id: u64, cancelled: bool) {
    let label = locked(|running| {
        let pos = running.iter().position(|(info, _)| info.id == id);
        pos.map(|i| running.remove(i).0.label)
    });
    let Some(label) = label else { return };
    locked_finished(|list| {
        list.push_back(FinishedTask {
            id,
            label,
            cancelled,
        });
        while list.len() > FINISHED_LIMIT {
            list.pop_front();
        }
    });
}

/// Run `work` on a worker thread; report progress and the result on the main
/// thread.
///
/// * `label` is what the task manager shows. Name the *operation*, not the
///   call site — "Importing EPUBs", not "reload". A task with no meaningful
///   name is invisible in every way that matters.
/// * `work` is `Send` and receives a [`Reporter`]. It must not touch GTK or
///   [`crate::notify`].
/// * `on_progress` runs on the main thread for each [`Reporter::step`].
/// * `on_done` runs on the main thread once, with whatever `work` returned.
///   It runs even when the task was cancelled — the worker decides what a
///   cancelled result looks like, because only it knows how far it got.
///
/// Must be called from the main thread: it attaches a receiver to the GLib
/// main context.
pub fn spawn<T, W, P, D>(label: impl Into<String>, work: W, on_progress: P, on_done: D)
where
    T: Send + 'static,
    W: FnOnce(Reporter) -> T + Send + 'static,
    P: Fn(Update) + 'static,
    D: FnOnce(T) + 'static,
{
    // Two channels rather than one enum: it keeps `Reporter` non-generic, so
    // worker code does not have to name the task's result type to report a
    // percentage.
    let (progress_tx, progress_rx) = async_channel::unbounded::<Update>();
    let (result_tx, result_rx) = async_channel::unbounded::<T>();

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let cancelled = Arc::new(AtomicBool::new(false));
    locked(|running| {
        running.push((
            TaskInfo {
                id,
                label: label.into(),
                done: 0,
                total: 0,
                detail: String::new(),
            },
            cancelled.clone(),
        ))
    });

    std::thread::spawn(move || {
        let reporter = Reporter {
            tx: progress_tx,
            cancelled,
        };
        let out = work(reporter);
        // `reporter` is dropped here, which closes the progress channel and
        // is how the reader below knows to stop waiting for updates.
        let _ = result_tx.send_blocking(out);
    });

    gtk::glib::spawn_future_local(async move {
        // Drain progress to exhaustion *first*, then take the result. One
        // sequential future rather than two concurrent ones, because two would
        // race: the completion toast could land before the last progress
        // update and leave a stale "importing 3 of 5" on screen afterwards.
        while let Ok(update) = progress_rx.recv().await {
            set_progress(id, &update);
            on_progress(update);
        }
        if let Ok(value) = result_rx.recv().await {
            on_done(value);
        }
        finish(id, was_cancelled(id));
    });
}

/// Run `work` on a worker thread, delivering each item it produces to the main
/// thread as it is produced.
///
/// [`spawn`] answers "do this, tell me when it is finished". This answers
/// "keep producing things until you run out". A preloader is the second shape:
/// it decodes twenty covers and each one should appear the moment it is ready,
/// not twenty covers later.
///
/// `work` gets an [`Emit`] as well as a [`Reporter`]. `on_item` runs on the
/// main thread once per emitted value, so it may touch widgets; like `spawn`,
/// it is deliberately not `Send`.
pub fn spawn_stream<T, W, F>(label: impl Into<String>, work: W, on_item: F)
where
    T: Send + 'static,
    W: FnOnce(Reporter, Emit<T>) + Send + 'static,
    F: Fn(T) + 'static,
{
    // A stream reports by emitting items, so there is no separate progress
    // channel to listen on. The `Reporter` still exists for `cancelled()`;
    // its sender goes nowhere, which is fine because `step` ignores send
    // failures by design.
    let (progress_tx, progress_rx) = async_channel::unbounded::<Update>();
    drop(progress_rx);
    let (item_tx, item_rx) = async_channel::unbounded::<T>();

    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    let cancelled = Arc::new(AtomicBool::new(false));
    locked(|running| {
        running.push((
            TaskInfo {
                id,
                label: label.into(),
                done: 0,
                total: 0,
                detail: String::new(),
            },
            cancelled.clone(),
        ))
    });

    std::thread::spawn(move || {
        let reporter = Reporter {
            tx: progress_tx,
            cancelled,
        };
        work(reporter, Emit { tx: item_tx });
    });

    gtk::glib::spawn_future_local(async move {
        // Ends when the worker drops its `Emit`, which closes the channel.
        while let Ok(item) = item_rx.recv().await {
            on_item(item);
        }
        finish(id, was_cancelled(id));
    });
}

/// The worker half of [`spawn_stream`]: hands finished items back one at a
/// time.
pub struct Emit<T> {
    tx: async_channel::Sender<T>,
}

impl<T> Emit<T> {
    /// Deliver one finished item to the main thread.
    ///
    /// Returns `false` once the UI side has gone away, which is a worker's cue
    /// to stop early — a preloader has no reason to keep decoding for a page
    /// that has been closed.
    pub fn send(&self, item: T) -> bool {
        self.tx.send_blocking(item).is_ok()
    }
}

/// Whether a task was asked to stop. Read just before it leaves the registry,
/// so a finished task can be recorded as cancelled or completed.
///
/// Returns `false` for an id that has already gone, which is the right answer
/// for a task that finished on its own a moment ago.
fn was_cancelled(id: u64) -> bool {
    locked(|running| {
        running
            .iter()
            .find(|(info, _)| info.id == id)
            .is_some_and(|(_, flag)| flag.load(Ordering::Relaxed))
    })
}

/// Ask one task to stop. Returns whether it was still running.
///
/// Cancellation is cooperative: this sets a flag, and work that never checks
/// [`Reporter::cancelled`] runs to completion. That is correct for a short
/// query and wrong for a long download, which is why the long ones check.
pub fn cancel(id: u64) -> bool {
    locked(|running| {
        if let Some((_, flag)) = running.iter().find(|(info, _)| info.id == id) {
            flag.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    })
}

/// Ask every running task to stop.
///
/// Called when the window is closing. Cancellation is cooperative, so this
/// only sets flags — a task that never checks [`Reporter::cancelled`] runs to
/// completion, which is correct for short ones.
pub fn cancel_all() {
    locked(|running| {
        for (_, flag) in running.iter() {
            flag.store(true, Ordering::Relaxed);
        }
    });
}

/// The tasks running right now, oldest first.
pub fn tasks() -> Vec<TaskInfo> {
    locked(|running| running.iter().map(|(info, _)| info.clone()).collect())
}

/// The tasks that finished most recently, newest last. Bounded — see
/// [`FINISHED_LIMIT`].
pub fn recent() -> Vec<FinishedTask> {
    locked_finished(|list| list.iter().cloned().collect())
}

/// Forget the finished list. For the panel's clear button; the running tasks
/// are untouched.
pub fn clear_finished() {
    locked_finished(VecDeque::clear);
}

/// How many tasks are running. Used by the shutdown path and the tests.
pub fn running_count() -> usize {
    locked(|running| running.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;
    use std::rc::Rc;

    /// Held while a test touches the process-wide registry or finished list.
    ///
    /// Not optional, and the reason is a real false positive: tests share a
    /// process and run in parallel, and `the_recent_list_is_bounded` clears the
    /// finished list both before and after its assertions. Dropped between
    /// another test's `finish()` and its `recent()` read, that produced a
    /// failure reading exactly like the bug it was written to catch — a task
    /// that finished and never appeared in the panel. The product was fine;
    /// the test was racing its neighbour.
    static TEST_LOCK: Mutex<()> = Mutex::new(const { Mutex::new(()) });

    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        match TEST_LOCK.lock() {
            Ok(guard) => guard,
            // A panic in one test must not make the rest unrunnable.
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// Build a reporter without going through `spawn`, which needs a GLib main
    /// context and therefore cannot run in CI (no display).
    fn reporter() -> (Reporter, async_channel::Receiver<Update>, Arc<AtomicBool>) {
        let (tx, rx) = async_channel::unbounded::<Update>();
        let flag = Arc::new(AtomicBool::new(false));
        (
            Reporter {
                tx,
                cancelled: flag.clone(),
            },
            rx,
            flag,
        )
    }

    /// Register a fake task and hand back its flag, for tests that exercise
    /// the registry without a GLib main loop.
    ///
    /// Uses ids far from the real counter so it cannot collide with a task
    /// another test started. Tests share a process, so every one of these
    /// cleans up after itself.
    fn register_fake(id: u64, label: &str) -> Arc<AtomicBool> {
        let flag = Arc::new(AtomicBool::new(false));
        locked(|running| {
            running.push((
                TaskInfo {
                    id,
                    label: label.to_string(),
                    done: 0,
                    total: 0,
                    detail: String::new(),
                },
                flag.clone(),
            ))
        });
        flag
    }

    fn unregister(id: u64) {
        locked(|running| running.retain(|(info, _)| info.id != id));
    }

    #[test]
    fn progress_reaches_the_other_side() {
        let (reporter, rx, _flag) = reporter();
        reporter.step(3, 10, "chapter-3.xhtml");
        let got = rx.try_recv().expect("an update was sent");
        assert_eq!(got.done, 3);
        assert_eq!(got.total, 10);
        assert_eq!(got.detail, "chapter-3.xhtml");
    }

    #[test]
    fn a_worker_sees_the_cancel_flag() {
        let (reporter, _rx, flag) = reporter();
        assert!(!reporter.cancelled(), "starts running");
        flag.store(true, Ordering::Relaxed);
        assert!(reporter.cancelled(), "sees the flag flip");
    }

    #[test]
    fn reporting_after_the_ui_is_gone_is_not_an_error() {
        // The window can close while a worker is mid-loop. Dropping the
        // receiver must not panic the worker or make it bail out early.
        let (reporter, rx, _flag) = reporter();
        drop(rx);
        reporter.step(1, 2, "still going");
        assert!(!reporter.cancelled());
    }

    #[test]
    fn emit_reports_when_the_ui_side_has_gone() {
        // A preloader uses this as its stop signal: once the page is torn
        // down there is nothing to decode for, so `send` must say so rather
        // than fail silently and let the worker grind on.
        let (tx, rx) = async_channel::unbounded::<u8>();
        let emit = Emit { tx };
        assert!(emit.send(1), "delivers while the receiver lives");
        drop(rx);
        assert!(!emit.send(2), "reports the receiver being gone");
    }

    #[test]
    fn cancel_all_flags_every_registered_task() {
        let _guard = test_guard();
        // Uses the real registry, so clean up after itself rather than
        // assuming it starts empty -- tests share the process.
        let before = running_count();
        let a = register_fake(90_001, "test task a");
        let b = register_fake(90_002, "test task b");
        assert_eq!(running_count(), before + 2);

        cancel_all();
        assert!(a.load(Ordering::Relaxed));
        assert!(b.load(Ordering::Relaxed));

        unregister(90_001);
        unregister(90_002);
        assert_eq!(running_count(), before);
    }

    #[test]
    fn cancel_reaches_one_task_and_leaves_the_others_running() {
        let _guard = test_guard();
        // The whole point of 1.14: cancelling the download you did not mean to
        // start must not also cancel the thumbnail rebuild.
        let before = running_count();
        let keep = register_fake(90_011, "keep me");
        let stop = register_fake(90_012, "stop me");

        assert!(cancel(90_012), "the task was running");
        assert!(stop.load(Ordering::Relaxed), "the named task stopped");
        assert!(!keep.load(Ordering::Relaxed), "its neighbour did not");
        assert!(!cancel(99_999), "an unknown id is reported as not running");

        unregister(90_011);
        unregister(90_012);
        assert_eq!(running_count(), before);
    }

    #[test]
    fn cancelling_twice_is_harmless() {
        let _guard = test_guard();
        let before = running_count();
        register_fake(90_021, "cancel me twice");
        assert!(cancel(90_021));
        // Still registered — cancellation only sets a flag; the task leaves the
        // registry when it actually finishes — so this must still succeed.
        assert!(cancel(90_021), "the task is still running, so so is the flag");
        unregister(90_021);
        assert_eq!(running_count(), before);
    }

    #[test]
    fn the_panel_sees_labels_and_progress() {
        let _guard = test_guard();
        let before = running_count();
        register_fake(90_031, "Importing EPUBs");
        set_progress(
            90_031,
            &Update {
                done: 3,
                total: 5,
                detail: "dune.epub".to_string(),
            },
        );

        let listed = tasks();
        let found = listed.iter().find(|t| t.id == 90_031).expect("listed");
        assert_eq!(found.label, "Importing EPUBs");
        assert_eq!(found.done, 3);
        assert_eq!(found.total, 5);
        assert_eq!(found.detail, "dune.epub");

        unregister(90_031);
        assert_eq!(running_count(), before);
    }

    #[test]
    fn finishing_moves_a_task_to_the_recent_list() {
        let _guard = test_guard();
        let before = running_count();
        register_fake(90_041, "Downloading a chapter");
        cancel(90_041);
        finish(90_041, was_cancelled(90_041));

        assert_eq!(running_count(), before, "no longer running");
        let recent = recent();
        let found = recent.iter().find(|t| t.id == 90_041).expect("remembered");
        assert_eq!(found.label, "Downloading a chapter");
        assert!(found.cancelled, "recorded as cancelled, not completed");

        locked_finished(|list| list.retain(|t| t.id != 90_041));
    }

    #[test]
    fn the_recent_list_is_bounded() {
        let _guard = test_guard();
        // Unbounded would grow for the life of the process, and this list is
        // only there to say "that just happened".
        locked_finished(VecDeque::clear);
        for i in 0..(FINISHED_LIMIT + 5) {
            locked_finished(|list| {
                list.push_back(FinishedTask {
                    id: 90_100 + i as u64,
                    label: format!("task {i}"),
                    cancelled: false,
                });
                while list.len() > FINISHED_LIMIT {
                    list.pop_front();
                }
            });
        }
        let recent = recent();
        assert_eq!(recent.len(), FINISHED_LIMIT);
        assert_eq!(
            recent.first().map(|t| t.label.as_str()),
            Some("task 5"),
            "the oldest were dropped, not the newest"
        );
        locked_finished(VecDeque::clear);
    }

    /// A task that actually ran must be *visible* afterwards.
    ///
    /// The other tests here poke the registry directly, which proves the
    /// registry works but not that `spawn` reaches it — and "reaches it" is
    /// the half the owner found broken: an import finished and never showed up
    /// in the list. So this drives the real thing: a real `spawn`, a real
    /// worker thread, a real main context, and then looks at what the panel
    /// would see.
    ///
    /// Needs no display. A GLib main context is not GTK, so this runs in the
    /// ordinary `cargo test` step rather than only in the headless-sway smoke
    /// test — which matters, because that smoke job is `continue-on-error`.
    ///
    /// The poll loop is bounded so a regression fails the test in a few
    /// seconds instead of hanging CI until it times out.
    #[test]
    fn a_task_that_ran_lands_in_the_recent_list() {
        let _guard = test_guard();
        const LABEL: &str = "headless registry test";

        let done = Rc::new(Cell::new(false));
        let progress = Rc::new(Cell::new(false));
        let value = Rc::new(Cell::new(0u32));

        // Three sets, because `block_on` takes the async block by value: the
        // two callbacks and the poll loop each need their own clone moved in,
        // and the originals have to survive outside so the assertions after
        // `block_on` can read them.
        let (done_cb, progress_cb, value_cb) = (done.clone(), progress.clone(), value.clone());
        let done_poll = done.clone();

        // The default context, because `spawn_future_local` resolves against
        // the thread-default and falls back to it, and `block_on` is what
        // iterates it. Anything else and the future would be spawned onto a
        // context nobody was running.
        let ctx = gtk::glib::MainContext::default();
        ctx.block_on(async move {
            spawn(
                LABEL,
                |reporter| {
                    reporter.step(1, 2, "halfway");
                    42u32
                },
                move |_| progress_cb.set(true),
                move |v| {
                    value_cb.set(v);
                    done_cb.set(true);
                },
            );

            for _ in 0..500 {
                if done_poll.get() && !tasks().iter().any(|t| t.label == LABEL) {
                    break;
                }
                gtk::glib::timeout_future(std::time::Duration::from_millis(10)).await;
            }
        });

        assert_eq!(value.get(), 42, "the result never reached on_done");
        assert!(done.get(), "on_done never ran, so the future did not finish");
        assert!(
            progress.get(),
            "the progress report never reached the main thread"
        );
        assert!(
            recent().iter().any(|t| t.label == LABEL),
            "the task finished but never reached the recent list — the panel \
             would show nothing, which is what the owner reported"
        );
        assert!(
            !tasks().iter().any(|t| t.label == LABEL),
            "the task finished but is still listed as running"
        );
        assert!(
            recent().iter().any(|t| t.label == LABEL && !t.cancelled),
            "the task was recorded as cancelled when it was not"
        );

        // Clean up: the recent list is process-wide and other tests read it.
        locked_finished(|list| list.retain(|t| t.label != LABEL));
    }

    #[test]
    fn a_poisoned_registry_does_not_take_the_next_task_down() {
        let _guard = test_guard();
        // One task panicking must not turn every later task into a crash.
        let _ = std::panic::catch_unwind(|| {
            locked(|_| panic!("worker exploded while holding the lock"));
        });
        // The lock is poisoned now; this must still work.
        let _ = running_count();
        let _ = tasks();
        cancel_all();
    }
}
