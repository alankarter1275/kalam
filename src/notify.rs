//! Toast notifications, bottom-right, with a history.
//!
//! Kalam had 177 places that swallowed a failure and 8 that printed to stderr,
//! which is invisible unless the app was launched from a terminal. A cover that
//! failed to copy, a metadata write that was rejected, an import that errored —
//! all silently did nothing. This gives every one of them somewhere to speak.
//!
//! Usage is deliberately a free function so no call site needs a handle:
//!
//! ```ignore
//! notify::error("Could not write metadata", &err.to_string());
//! ```
//!
//! Toasts are queued if the overlay is not mounted yet, so early-startup
//! messages are not lost.
//!
//! **Thread safety.** Showing a toast builds GTK widgets and the queue is
//! `thread_local!`, so that half is main-thread-only. Callers do not have to
//! care: `push` records the history entry immediately (plain data, readable as
//! soon as `push` returns) and bounces only the *display* to the main context.
//! Before that, a toast raised from a worker vanished into that thread's own
//! copy of the queue and was never shown.

use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Mutex;

/// How long a toast stays on screen before fading, by severity.
const DISMISS_MS_INFO: u32 = 4_000;
const DISMISS_MS_ERROR: u32 = 9_000;
/// Compact toasts are acknowledgements, not messages; they go quickly.
const DISMISS_MS_COMPACT: u32 = 2_000;
/// Toasts visible at once; older ones are dropped from the stack.
const MAX_VISIBLE: usize = 4;
/// Entries kept for the history view.
const MAX_HISTORY: usize = 200;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Success,
    Error,
    Info,
    Progress,
}

impl Kind {
    /// Symbolic icon name from the current GTK icon theme.
    fn icon(self) -> &'static str {
        match self {
            Kind::Success => "object-select-symbolic",
            Kind::Error => "dialog-warning-symbolic",
            Kind::Info => "dialog-information-symbolic",
            Kind::Progress => "folder-download-symbolic",
        }
    }

    fn css(self) -> &'static str {
        match self {
            Kind::Success => "kalam-toast-success",
            Kind::Error => "kalam-toast-error",
            Kind::Info => "kalam-toast-info",
            Kind::Progress => "kalam-toast-progress",
        }
    }

    fn dismiss_ms(self) -> u32 {
        match self {
            // Errors linger: they are the ones worth reading.
            Kind::Error => DISMISS_MS_ERROR,
            _ => DISMISS_MS_INFO,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Entry {
    pub kind: Kind,
    pub title: String,
    pub detail: String,
    /// Wall-clock `HH:MM` for the history list.
    pub at: String,
    /// Local calendar day, `YYYY-MM-DD`, for the bell panel's
    /// Today/Earlier grouping (decision 148).
    pub day: String,
    /// Slimmer card, shorter life. For things that recur mid-read and would
    /// otherwise nag.
    pub compact: bool,
}

/// A callback run on the main thread whenever the history or its
/// unseen count changes; see [`on_history_changed`].
pub type HistoryListener = Rc<dyn Fn()>;

thread_local! {
    /// The stack toasts are appended to, once the main window exists.
    static HOST: RefCell<Option<gtk::Box>> = const { RefCell::new(None) };
    /// Messages raised before the overlay was mounted.
    static PENDING: RefCell<Vec<Entry>> = const { RefCell::new(Vec::new()) };
    /// Main-thread listeners for history changes (the rail bell).
    /// Thread-local like the toast host: the callbacks touch GTK
    /// widgets, so they may only ever run where they were registered.
    static HISTORY_LISTENERS: RefCell<Vec<HistoryListener>> =
        const { RefCell::new(Vec::new()) };
}

/// Entries raised since the bell panel was last opened. Process-wide
/// for the same reason the history is: a worker's failure is exactly
/// what the dot exists to announce.
static UNSEEN: AtomicUsize = AtomicUsize::new(0);

/// Everything raised this session, newest first.
///
/// Process-wide, **not** `thread_local!`. It used to be thread-local, which
/// meant a message raised on a worker thread was filed in that thread's own
/// copy: the bell panel reads this from the main thread, so those
/// entries were invisible there and vanished when the worker exited. Since
/// workers are exactly where unattended failures happen — a failed import, a
/// bad dictionary pack — that was the wrong half of the app to lose.
static HISTORY: Mutex<VecDeque<Entry>> = Mutex::new(VecDeque::new());

/// Read the history, surviving a panic in another thread.
///
/// A poisoned lock still holds perfectly good data here, and refusing to show
/// the notification list because some unrelated thread panicked would be a
/// worse outcome than showing it.
fn with_history<R>(f: impl FnOnce(&mut VecDeque<Entry>) -> R) -> R {
    let mut guard = match HISTORY.lock() {
        Ok(guard) => guard,
        Err(poisoned) => poisoned.into_inner(),
    };
    f(&mut guard)
}

/// Attach the overlay's toast container. Called once from the app shell.
pub fn attach(host: gtk::Box) {
    HOST.with(|h| *h.borrow_mut() = Some(host));
    // Flush anything raised during startup.
    let queued: Vec<Entry> = PENDING.with(|p| std::mem::take(&mut *p.borrow_mut()));
    for entry in queued {
        present(&entry);
    }
    // Anything queued before the shell existed may predate the bell's
    // listener registration order in `init`; announce once more so the
    // dot and list agree with the history they were flushed from.
    notify_history_changed();
}

pub fn success(title: &str, detail: &str) {
    push(Kind::Success, title, detail, false);
}

pub fn error(title: &str, detail: &str) {
    push(Kind::Error, title, detail, false);
}

pub fn info(title: &str, detail: &str) {
    push(Kind::Info, title, detail, false);
}

pub fn activity(title: &str, detail: &str) {
    push(Kind::Progress, title, detail, false);
}

/// A small, brief confirmation. Highlighting and saving quotes happen many
/// times in a sitting, so those get a slim card that clears itself quickly
/// rather than a full-sized toast parked over the page.
pub fn compact(title: &str, detail: &str) {
    push(Kind::Success, title, detail, true);
}

/// Report a `Result`'s error, if it has one. Returns whether it was Ok, so
/// call sites can stay terse:
///
/// ```ignore
/// notify::report(catalog.delete_book(id), "Could not remove the book");
/// ```
pub fn report<T, E: std::fmt::Display>(result: Result<T, E>, title: &str) -> bool {
    match result {
        Ok(_) => true,
        Err(err) => {
            error(title, &err.to_string());
            false
        }
    }
}

/// Speak on **both** outcomes.
///
/// `report` alone was a trap: it only raised a toast when something went
/// wrong, so every action that quietly succeeded — removing a book, editing a
/// shelf — looked to the user as if notifications were broken. Anything the
/// user deliberately triggered should confirm itself.
///
/// ```ignore
/// notify::outcome(
///     catalog.delete_shelf(id),
///     "Shelf deleted", &name,
///     "Could not delete the shelf",
/// );
/// ```
pub fn outcome<T, E: std::fmt::Display>(
    result: Result<T, E>,
    ok_title: &str,
    ok_detail: &str,
    err_title: &str,
) -> bool {
    match result {
        Ok(_) => {
            success(ok_title, ok_detail);
            true
        }
        Err(err) => {
            error(err_title, &err.to_string());
            false
        }
    }
}

/// Like [`outcome`], but the confirmation is neutral rather than a green tick.
/// Used for reversals — unread, removed from a list — where "Success" reads
/// oddly.
pub fn outcome_info<T, E: std::fmt::Display>(
    result: Result<T, E>,
    ok_title: &str,
    ok_detail: &str,
    err_title: &str,
) -> bool {
    match result {
        Ok(_) => {
            info(ok_title, ok_detail);
            true
        }
        Err(err) => {
            error(err_title, &err.to_string());
            false
        }
    }
}

fn push(kind: Kind, title: &str, detail: &str, compact: bool) {
    let entry = Entry {
        kind,
        title: title.to_string(),
        detail: detail.to_string(),
        at: clock_now(),
        day: day_now(),
        compact,
    };

    with_history(|h| {
        h.push_front(entry.clone());
        while h.len() > MAX_HISTORY {
            h.pop_back();
        }
    });
    // The dot counts from the same moment the entry becomes readable,
    // not from when its toast happens to paint.
    if UNSEEN.load(Ordering::SeqCst) < MAX_HISTORY {
        UNSEEN.fetch_add(1, Ordering::SeqCst);
    }

    // Showing the toast means building GTK widgets, and `HOST`/`PENDING` are
    // `thread_local!`, so this half is main-thread-only. Worker threads *do*
    // reach here — the import loop reports an unreadable file with
    // `notify::error` from inside its worker — and when they did, the entry
    // went into that thread's own empty PENDING and was never seen again. The
    // message was silently lost: the exact failure mode this module exists to
    // prevent.
    //
    // Bounce rather than make every caller think about threads. `invoke` runs
    // the closure inline when we are already the main-context owner, so the
    // common path costs nothing.
    //
    // The history write above is deliberately *not* bounced: it is plain data,
    // callers (and tests) expect it to be readable the moment `push` returns,
    // and deferring it would make `notify::history()` racy.
    let context = gtk::glib::MainContext::default();
    if context.is_owner() {
        deliver(entry);
    } else {
        context.invoke(move || deliver(entry));
    }
    notify_history_changed();
}

/// Put an entry on screen, or queue it until the overlay is mounted.
///
/// Main thread only — see the note in [`push`].
fn deliver(entry: Entry) {
    let mounted = HOST.with(|h| h.borrow().is_some());
    if mounted {
        present(&entry);
    } else {
        PENDING.with(|p| p.borrow_mut().push(entry));
    }
}

/// Everything shown this session, newest first.
pub fn history() -> Vec<Entry> {
    with_history(|h| h.iter().cloned().collect())
}

pub fn clear_history() {
    with_history(|h| h.clear());
    UNSEEN.store(0, Ordering::SeqCst);
    notify_history_changed();
}

/// How many entries arrived since the bell panel was last opened.
pub fn unseen_count() -> usize {
    UNSEEN.load(Ordering::SeqCst)
}

/// Whether the rail bell should carry its new-event dot.
pub fn has_unseen() -> bool {
    unseen_count() > 0
}

/// Opening the bell panel clears the dot (decision 148).
pub fn mark_all_seen() {
    if UNSEEN.swap(0, Ordering::SeqCst) > 0 {
        notify_history_changed();
    }
}

/// Today's local `YYYY-MM-DD`, the stamp [`Entry::day`] is compared
/// against for the panel's Today group.
pub fn today_stamp() -> String {
    day_now()
}

/// Register `listener` to run on the main thread after every history
/// or unseen-count change — a push from any thread, Clear history,
/// or the panel opening. The current state is *not* replayed; read
/// [`history`] / [`unseen_count`] first, then register for changes
/// (the same contract as `theme::on_theme_applied`).
pub fn on_history_changed(listener: HistoryListener) {
    HISTORY_LISTENERS.with(|listeners| listeners.borrow_mut().push(listener));
}

fn emit_history_changed() {
    // Clone the handles out first: a listener may itself raise a
    // notification (or register another listener), and holding the
    // borrow across the calls would panic.
    let listeners: Vec<HistoryListener> =
        HISTORY_LISTENERS.with(|listeners| listeners.borrow().clone());
    for listener in listeners {
        listener();
    }
}

/// Announce a history change to the (main-thread) listeners, bouncing
/// to the main context when called from a worker — the same bounce
/// the toast display makes, for the same reason.
fn notify_history_changed() {
    let context = gtk::glib::MainContext::default();
    if context.is_owner() {
        emit_history_changed();
    } else {
        context.invoke(emit_history_changed);
    }
}

fn present(entry: &Entry) {
    HOST.with(|host| {
        let Some(host) = host.borrow().clone() else {
            return;
        };

        let card = build_card(entry);
        host.append(&card);
        // The overlay wrapper is hidden while empty (a 0x0 overlay child is an
        // invalid pixman rectangle), so reveal it now that it has content.
        set_host_visible(&host, true);

        // Keep the stack short so a burst of messages cannot cover the app.
        while count_children(&host) > MAX_VISIBLE {
            if let Some(oldest) = host.first_child() {
                host.remove(&oldest);
            }
        }

        // Auto-dismiss. Removing an already-detached widget is harmless
        // because we check its parent first.
        let host_for_timeout = host.clone();
        let card_for_timeout = card.clone();
        let life = if entry.compact {
            DISMISS_MS_COMPACT
        } else {
            entry.kind.dismiss_ms()
        };
        gtk::glib::timeout_add_local_once(
            std::time::Duration::from_millis(life as u64),
            move || {
                if card_for_timeout.parent().is_some() {
                    host_for_timeout.remove(&card_for_timeout);
                }
                if host_for_timeout.first_child().is_none() {
                    set_host_visible(&host_for_timeout, false);
                }
            },
        );
    });
}

/// Toggle the overlay wrapper that holds the toast stack.
///
/// `host` is the inner box; its parent is the overlay child that carries the
/// margin, and that is the widget which must not be allocated while empty.
fn set_host_visible(host: &gtk::Box, visible: bool) {
    if let Some(wrapper) = host.parent() {
        wrapper.set_visible(visible);
    }
}

fn count_children(host: &gtk::Box) -> usize {
    let mut n = 0;
    let mut child = host.first_child();
    while let Some(w) = child {
        n += 1;
        child = w.next_sibling();
    }
    n
}

/// Build one toast with an accent bar, icon, title and detail.
fn build_card(entry: &Entry) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    card.add_css_class("kalam-toast");
    card.add_css_class(entry.kind.css());
    if entry.compact {
        card.add_css_class("kalam-toast-compact");
    }
    card.set_halign(gtk::Align::End);
    // GTK CSS has no `overflow`; clip here so the accent stripe follows the
    // rounded corners.
    card.set_overflow(gtk::Overflow::Hidden);

    // The coloured severity stripe. It is a rounded pill floating inside the
    // card, never touching the left edge.
    //
    // Height is done with margins rather than a fixed pixel height so the bar
    // tracks the card: in a horizontal box a child fills the full height, so
    // equal top/bottom margins leave a centred bar of (card height - 2*inset).
    // A hardcoded height would be wrong the moment a two-line detail made the
    // card taller. The 13px inset gives roughly the 1:3:1 split — one unit of
    // space above, three of bar, one below — on a normal single-detail toast.
    let accent = gtk::Box::new(gtk::Orientation::Vertical, 0);
    accent.add_css_class("kalam-toast-accent");
    accent.set_size_request(5, -1);
    accent.set_valign(gtk::Align::Fill);
    if entry.compact {
        // A compact card is much shorter, so the standard 13px inset would
        // eat the whole bar; the compact rule uses a smaller one.
        accent.add_css_class("kalam-toast-accent-compact");
    }
    card.append(&accent);

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    body.add_css_class("kalam-toast-body");

    let icon = crate::icons::symbolic_with_classes(entry.kind.icon(), 17, &["kalam-toast-icon"]);
    icon.set_valign(gtk::Align::Center);
    body.append(&icon);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_valign(gtk::Align::Center);

    let title = gtk::Label::new(Some(&entry.title));
    title.add_css_class("kalam-toast-title");
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    // Capped so a long message cannot stretch the toast across the window.
    title.set_width_chars(1);
    title.set_max_width_chars(40);
    text.append(&title);

    if !entry.detail.trim().is_empty() {
        let detail = gtk::Label::new(Some(entry.detail.trim()));
        detail.add_css_class("kalam-toast-detail");
        detail.set_halign(gtk::Align::Start);
        detail.set_xalign(0.0);
        detail.set_wrap(true);
        detail.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        detail.set_lines(2);
        detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
        detail.set_width_chars(1);
        detail.set_max_width_chars(44);
        detail.set_tooltip_text(Some(&entry.detail));
        text.append(&detail);
    }

    body.append(&text);
    card.append(&body);

    // Click to dismiss early.
    let click = gtk::GestureClick::new();
    click.set_button(1);
    let card_for_click = card.clone();
    click.connect_released(move |_, _, _, _| {
        if let Some(parent) = card_for_click.parent() {
            if let Some(host) = parent.downcast_ref::<gtk::Box>() {
                host.remove(&card_for_click);
                if host.first_child().is_none() {
                    set_host_visible(host, false);
                }
            }
        }
    });
    card.add_controller(click);
    card.set_cursor_from_name(Some("pointer"));
    card.set_tooltip_text(Some("Click to dismiss"));

    card
}

fn stamp_now(format: &str) -> String {
    gtk::glib::DateTime::now_local()
        .and_then(|d| d.format(format))
        .map(|s| s.to_string())
        .unwrap_or_default()
}

/// Local `HH:MM`, without pulling in a date library.
fn clock_now() -> String {
    stamp_now("%H:%M")
}

/// Local `YYYY-MM-DD`, the bell panel's day boundary.
fn day_now() -> String {
    stamp_now("%Y-%m-%d")
}

#[cfg(test)]
mod tests {
    use super::*;

    // These exercise the non-GTK bookkeeping only; presenting a toast needs a
    // display, which CI does not have.

    /// Serialises the tests that touch the history.
    ///
    /// `HISTORY` is process-wide (it has to be -- a worker's message must be
    /// readable from the main thread), and `cargo test` runs these on
    /// parallel threads of one process. So `clear_history()` at the top of a
    /// test is not isolation: another test can push between that call and the
    /// assertion. Every test that reads or writes the history takes this lock
    /// first.
    ///
    /// Poison-tolerant for the same reason as the rest of the module: one
    /// failing test should report its own failure, not turn every later test
    /// into a mutex panic that hides it.
    static HISTORY_TEST_LOCK: Mutex<()> = Mutex::new(());

    fn history_guard() -> std::sync::MutexGuard<'static, ()> {
        match HISTORY_TEST_LOCK.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    #[test]
    fn history_keeps_newest_first() {
        let _guard = history_guard();
        clear_history();
        push(Kind::Info, "first", "", false);
        push(Kind::Info, "second", "", false);
        let h = history();
        assert_eq!(h[0].title, "second");
        assert_eq!(h[1].title, "first");
        clear_history();
    }

    #[test]
    fn history_is_bounded() {
        let _guard = history_guard();
        clear_history();
        for i in 0..(MAX_HISTORY + 25) {
            push(Kind::Info, &format!("n{i}"), "", false);
        }
        assert_eq!(history().len(), MAX_HISTORY);
        clear_history();
    }

    #[test]
    fn report_passes_ok_through_and_flags_errors() {
        let _guard = history_guard();
        clear_history();
        let ok: Result<(), String> = Ok(());
        assert!(report(ok, "should not appear"));
        assert!(history().is_empty());

        let bad: Result<(), String> = Err("disk full".into());
        assert!(!report(bad, "Could not save"));
        let h = history();
        assert_eq!(h.len(), 1);
        assert_eq!(h[0].kind, Kind::Error);
        assert!(h[0].detail.contains("disk full"));
        clear_history();
    }

    #[test]
    fn a_message_from_a_worker_thread_is_visible_from_the_main_thread() {
        let _guard = history_guard();
        // The real bug this guards. Background work is exactly where
        // unattended failures happen -- a failed import, a bad dictionary
        // pack -- and the bell panel reads `history()` on the main thread. While
        // HISTORY was `thread_local!` those entries went into the worker's own
        // copy: invisible in the notification list, gone when the thread
        // ended. Raise from a worker, read from here.
        clear_history();
        std::thread::spawn(|| {
            push(Kind::Error, "from a worker", "disk full", false);
        })
        .join()
        .expect("worker thread finished");

        let h = history();
        assert_eq!(h.len(), 1, "the worker's message crossed the thread");
        assert_eq!(h[0].title, "from a worker");
        assert_eq!(h[0].kind, Kind::Error);
        clear_history();
    }

    #[test]
    fn errors_linger_longer_than_chatter() {
        assert!(Kind::Error.dismiss_ms() > Kind::Info.dismiss_ms());
    }

    #[test]
    fn unseen_counts_new_entries_until_the_panel_opens() {
        let _guard = history_guard();
        clear_history();
        assert_eq!(unseen_count(), 0);
        assert!(!has_unseen());

        push(Kind::Info, "one", "", false);
        push(Kind::Error, "two", "", false);
        assert_eq!(unseen_count(), 2);
        assert!(has_unseen());

        // Entries carry the local day for the Today/Earlier split.
        let h = history();
        assert_eq!(h[0].day.len(), 10, "day stamp is YYYY-MM-DD");
        assert_eq!(h[0].day.matches('-').count(), 2);

        // Opening the panel clears the dot but not the history.
        mark_all_seen();
        assert_eq!(unseen_count(), 0);
        assert_eq!(history().len(), 2);

        // Clear history empties both.
        push(Kind::Info, "three", "", false);
        assert_eq!(unseen_count(), 1);
        clear_history();
        assert_eq!(unseen_count(), 0);
        assert!(history().is_empty());
    }
}
