//! Choosing which parts of a book's reading data to keep.
//!
//! One shape, four places: deleting a book, deleting many, a re-import that
//! found something on the rescue shelf, and opening a book that is still
//! waiting for an answer. All four ask the same question, so all four are built
//! from the same five rows rather than each inventing its own wording.
//!
//! The five kinds are exactly the flags on `db::ReadingKinds`. Shelf and
//! reading-list placement are deliberately absent: that is where a book lives
//! rather than work done inside it, so it always comes back and is never
//! offered. `KIND_ROWS` is checked against the struct by a test below, because
//! the default is `ALL` — a sixth flag added without a row here would be kept
//! silently, and the reader would never have been asked.

use super::dialog::{close_from_slot, dialog_message, dialog_shell, present_dialog, DialogSize};
use super::in_app_dialog::{DialogExit, InAppDialog};
use super::pill_button::{boxed_ghost_button, pill_button, PillButtonKind};
use crate::db::{ClearTally, ReadingKinds};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// One row of the chooser.
///
/// `get`/`set` are function pointers rather than a field index so a single loop
/// can drive all five checkboxes, and so the bulk-delete table can build its
/// columns from the same rows. Adding a kind means adding one entry here and
/// one field there, and the test below fails until both exist.
pub struct KindRow {
    /// The checkbox's own words.
    pub label: &'static str,
    /// The grey line under it, saying what is actually inside this kind.
    pub note: &'static str,
    /// How the kind reads in a sentence: "and your bookmarks".
    pub noun: &'static str,
    pub get: fn(&ReadingKinds) -> bool,
    pub set: fn(&mut ReadingKinds, bool),
}

/// The five kinds, in the order they are shown. Position first because losing
/// your place is the one people notice; ink last because it applies to PDFs
/// only and is the rarest.
pub const KIND_ROWS: [KindRow; 5] = [
    KindRow {
        label: "My place in the book",
        note: "Where you stopped reading.",
        noun: "your place in the book",
        get: |k| k.position,
        set: |k, v| k.position = v,
    },
    KindRow {
        label: "Bookmarks",
        note: "Pages you marked to come back to.",
        noun: "your bookmarks",
        get: |k| k.bookmarks,
        set: |k, v| k.bookmarks = v,
    },
    KindRow {
        label: "Highlights",
        note: "Passages you marked, including underlined ones.",
        noun: "your highlights",
        get: |k| k.highlights,
        set: |k, v| k.highlights = v,
    },
    KindRow {
        label: "Quotes",
        note: "Passages you saved as quotes.",
        noun: "your quotes",
        get: |k| k.quotes,
        set: |k, v| k.quotes = v,
    },
    KindRow {
        label: "Ink on PDF pages",
        note: "Pen, highlighter, shapes and notes you drew.",
        noun: "your ink",
        get: |k| k.ink,
        set: |k, v| k.ink = v,
    },
];

/// What a choice keeps, in plain words, for a confirmation line or a toast.
///
/// Reads "everything" and "nothing" rather than listing five ticks, because
/// those are the two answers that matter and a list of all five would bury the
/// point.
pub fn describe(kinds: ReadingKinds) -> String {
    if kinds.is_all() {
        return "everything".to_string();
    }
    if kinds.is_empty() {
        return "nothing".to_string();
    }
    let chosen: Vec<&str> = KIND_ROWS
        .iter()
        .filter(|row| (row.get)(&kinds))
        .map(|row| row.noun)
        .collect();
    match chosen.split_last() {
        None => "nothing".to_string(),
        Some((only, [])) => (*only).to_string(),
        Some((last, rest)) => format!("{} and {}", rest.join(", "), last),
    }
}

/// The running sentence shown under the five ticks.
fn summary_line(kinds: ReadingKinds) -> String {
    format!("Keeping {}.", describe(kinds))
}

/// What a clear actually removed, in plain words, for the toast that follows.
///
/// Counts rather than kind names, because "3 highlights" tells the reader
/// whether the button did what they meant and "highlights" does not. The
/// position has no count: there is only ever one.
pub fn describe_cleared(tally: ClearTally) -> String {
    let mut parts: Vec<String> = Vec::new();
    if tally.position {
        parts.push("your place in the book".to_string());
    }
    for (count, one, many) in [
        (tally.bookmarks, "bookmark", "bookmarks"),
        (tally.highlights, "highlight", "highlights"),
        (tally.quotes, "quote", "quotes"),
        (tally.ink, "ink mark", "ink marks"),
    ] {
        match count {
            0 => {}
            1 => parts.push(format!("1 {one}")),
            n => parts.push(format!("{n} {many}")),
        }
    }
    match parts.split_last() {
        None => "nothing".to_string(),
        Some((only, [])) => only.clone(),
        Some((last, rest)) => format!("{} and {}", rest.join(", "), last),
    }
}

/// The five checkboxes, sharing one answer.
///
/// Cloning shares the state rather than copying it, so a dialog's confirm
/// button and its panel can be handed to different closures and still agree.
#[derive(Clone)]
pub struct KindPicker {
    root: gtk::Box,
    state: Rc<RefCell<ReadingKinds>>,
    checks: Vec<gtk::CheckButton>,
    /// Set while `set_kinds` is writing to the checkboxes. Without it a tick
    /// from code would fire the change callback, which would write back, which
    /// would fire it again.
    syncing: Rc<Cell<bool>>,
}

impl KindPicker {
    /// Build the panel. `on_change` fires for every tick the reader makes, and
    /// not for ticks made through `set_kinds`.
    pub fn new(initial: ReadingKinds, on_change: impl Fn(ReadingKinds) + 'static) -> Self {
        let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
        let state = Rc::new(RefCell::new(initial));
        let syncing = Rc::new(Cell::new(false));
        let on_change = Rc::new(on_change);
        let mut checks = Vec::with_capacity(KIND_ROWS.len());

        for row in &KIND_ROWS {
            let check = gtk::CheckButton::with_label(row.label);
            check.add_css_class("kalam-comic-series-check");
            check.set_active((row.get)(&initial));
            check.set_halign(gtk::Align::Start);

            let note = gtk::Label::new(Some(row.note));
            note.add_css_class("kalam-ui-meta");
            note.set_halign(gtk::Align::Start);
            note.set_xalign(0.0);
            note.set_wrap(true);
            // Indented to line up under the checkbox's label, not its box.
            note.set_margin_start(26);

            let cell = gtk::Box::new(gtk::Orientation::Vertical, 2);
            cell.append(&check);
            cell.append(&note);
            root.append(&cell);

            let set = row.set;
            let state_for_tick = state.clone();
            let syncing_for_tick = syncing.clone();
            let on_change_for_tick = on_change.clone();
            check.connect_toggled(move |button| {
                if syncing_for_tick.get() {
                    return;
                }
                let next = {
                    let mut kinds = state_for_tick.borrow_mut();
                    (set)(&mut kinds, button.is_active());
                    *kinds
                };
                (*on_change_for_tick)(next);
            });
            checks.push(check);
        }

        Self {
            root,
            state,
            checks,
            syncing,
        }
    }

    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    pub fn kinds(&self) -> ReadingKinds {
        *self.state.borrow()
    }

    /// Move all five checkboxes at once, without firing `on_change`.
    pub fn set_kinds(&self, kinds: ReadingKinds) {
        self.syncing.set(true);
        *self.state.borrow_mut() = kinds;
        for (row, check) in KIND_ROWS.iter().zip(&self.checks) {
            check.set_active((row.get)(&kinds));
        }
        self.syncing.set(false);
    }
}

/// Ask which parts to keep, with a confirm button that says what it will do.
///
/// Used wherever the question stands on its own: clearing a book that stays in
/// the library, a re-import of one file, and a book opened while still waiting
/// for an answer.
pub fn choose_kinds_dialog(
    anchor: &impl IsA<gtk::Widget>,
    title: &str,
    message: &str,
    initial: ReadingKinds,
    confirm_label: &str,
    confirm_kind: PillButtonKind,
    on_confirm: impl Fn(ReadingKinds) + 'static,
) -> Option<InAppDialog> {
    let shell = dialog_shell(DialogSize::Form, None);
    shell.body().append(&dialog_message(&shell, message));

    // The change callback is empty because nothing outside reads the ticks
    // until confirm; the state is shared, so the button can just ask.
    let picker = KindPicker::new(initial, |_| {});
    shell.body().append(picker.widget());

    let dialog_slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));

    let slot_for_cancel = dialog_slot.clone();
    shell
        .footer()
        .append(&boxed_ghost_button("Cancel", move || {
            close_from_slot(&slot_for_cancel);
        }));

    let slot_for_confirm = dialog_slot.clone();
    let picker_for_confirm = picker.clone();
    shell.footer().append(&pill_button(
        confirm_label,
        confirm_kind,
        move || {
            let kinds = picker_for_confirm.kinds();
            close_from_slot(&slot_for_confirm);
            on_confirm(kinds);
        },
    ));

    let dialog = present_dialog(anchor, title, DialogExit::OwnButtons, &shell)?;
    *dialog_slot.borrow_mut() = Some(dialog.clone());
    Some(dialog)
}

/// The one-book delete dialog: a single checkbox for the common answer, plus a
/// link that opens the five granular choices.
///
/// The checkbox is what most people want and it stays one tick away. The link
/// is there so that keeping some of it is possible without the dialog leading
/// with five questions nobody usually needs to answer.
///
/// The two controls are kept in step: the checkbox reads as ticked only when
/// nothing at all is kept, so a partial choice never looks like a total one.
pub fn delete_book_dialog(
    anchor: &impl IsA<gtk::Widget>,
    title: &str,
    on_confirm: impl Fn(ReadingKinds) + 'static,
) -> Option<InAppDialog> {
    let shell = dialog_shell(DialogSize::Form, None);
    shell.body().append(&dialog_message(
        &shell,
        &format!(
            "“{title}” will be removed from your library and its files deleted. \
             This cannot be undone."
        ),
    ));

    let wipe = gtk::CheckButton::with_label("Also remove my notes and marks");
    wipe.add_css_class("kalam-comic-series-check");
    wipe.set_halign(gtk::Align::Start);
    shell.body().append(&wipe);

    let link = gtk::Button::with_label("Choose what to keep");
    link.add_css_class("kalam-settings-link");
    link.set_halign(gtk::Align::Start);
    link.set_margin_top(4);
    shell.body().append(&link);

    // Hidden until the link is used: the five rows are an answer to a question
    // most deletions do not ask.
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.set_visible(false);
    panel.set_margin_top(12);

    // A running sentence rather than five ticks to read back. The reader is
    // about to destroy something, and "Keeping your highlights and your quotes"
    // says what survives far more plainly than two ticked boxes do.
    let summary = gtk::Label::new(Some(&summary_line(ReadingKinds::ALL)));
    summary.add_css_class("kalam-ui-meta");
    summary.set_halign(gtk::Align::Start);
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    summary.set_margin_top(8);

    // Writing to one control must not echo back into itself. The picker guards
    // its own checkboxes; this guards `wipe`.
    let syncing = Rc::new(Cell::new(false));

    let wipe_for_picker = wipe.clone();
    let syncing_for_picker = syncing.clone();
    let summary_for_picker = summary.clone();
    let picker = KindPicker::new(ReadingKinds::ALL, move |kinds| {
        summary_for_picker.set_text(&summary_line(kinds));
        syncing_for_picker.set(true);
        wipe_for_picker.set_active(kinds.is_empty());
        syncing_for_picker.set(false);
    });
    panel.append(picker.widget());
    panel.append(&summary);
    shell.body().append(&panel);

    let panel_for_link = panel.clone();
    let link_for_link = link.clone();
    link.connect_clicked(move |_| {
        let show = !panel_for_link.is_visible();
        panel_for_link.set_visible(show);
        link_for_link.set_label(if show {
            "Hide the choices"
        } else {
            "Choose what to keep"
        });
    });

    let picker_for_wipe = picker.clone();
    let syncing_for_wipe = syncing.clone();
    let summary_for_wipe = summary.clone();
    wipe.connect_toggled(move |button| {
        if syncing_for_wipe.get() {
            return;
        }
        let kinds = if button.is_active() {
            ReadingKinds::NONE
        } else {
            ReadingKinds::ALL
        };
        picker_for_wipe.set_kinds(kinds);
        // `set_kinds` is guarded against echoing, so it does not fire the
        // callback that would otherwise have updated this.
        summary_for_wipe.set_text(&summary_line(kinds));
    });

    let dialog_slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));

    let slot_for_cancel = dialog_slot.clone();
    shell
        .footer()
        .append(&boxed_ghost_button("Keep book", move || {
            close_from_slot(&slot_for_cancel);
        }));

    let slot_for_confirm = dialog_slot.clone();
    let picker_for_confirm = picker.clone();
    shell.footer().append(&pill_button(
        "Remove book",
        PillButtonKind::Danger,
        move || {
            let kinds = picker_for_confirm.kinds();
            close_from_slot(&slot_for_confirm);
            on_confirm(kinds);
        },
    ));

    let dialog = present_dialog(anchor, "Remove this book?", DialogExit::OwnButtons, &shell)?;
    *dialog_slot.borrow_mut() = Some(dialog.clone());
    Some(dialog)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The row table must drive exactly the five flags: one row each, no row
    /// reaching into another's flag, and all five together reaching `ALL`.
    ///
    /// This is the guard that makes the chooser safe to extend. `Default` is
    /// `ALL`, so a sixth flag with no row here would be kept without ever being
    /// offered, and no other test would notice.
    #[test]
    fn the_rows_cover_every_flag_exactly_once() {
        assert_eq!(KIND_ROWS.len(), 5);

        let mut together = ReadingKinds::NONE;
        for (i, row) in KIND_ROWS.iter().enumerate() {
            let mut single = ReadingKinds::NONE;
            (row.set)(&mut single, true);
            assert!(
                (row.get)(&single),
                "row {i} ({}) does not read back what it wrote",
                row.label
            );
            for (j, other) in KIND_ROWS.iter().enumerate() {
                if j != i {
                    assert!(
                        !(other.get)(&single),
                        "row {i} ({}) also drove row {j} ({})",
                        row.label,
                        other.label
                    );
                }
            }
            (row.set)(&mut together, true);
        }
        assert_eq!(
            together,
            ReadingKinds::ALL,
            "the five rows together do not reach ALL"
        );
    }

    #[test]
    fn the_description_names_only_what_is_chosen() {
        assert_eq!(describe(ReadingKinds::ALL), "everything");
        assert_eq!(describe(ReadingKinds::NONE), "nothing");

        let one = ReadingKinds {
            highlights: true,
            ..ReadingKinds::NONE
        };
        assert_eq!(describe(one), "your highlights");

        let two = ReadingKinds {
            position: true,
            quotes: true,
            ..ReadingKinds::NONE
        };
        assert_eq!(describe(two), "your place in the book and your quotes");

        let three = ReadingKinds {
            bookmarks: true,
            highlights: true,
            ink: true,
            ..ReadingKinds::NONE
        };
        assert_eq!(
            describe(three),
            "your bookmarks, your highlights and your ink"
        );
    }

    #[test]
    fn the_cleared_summary_counts_and_never_invents_rows() {
        assert_eq!(describe_cleared(ClearTally::default()), "nothing");

        let position_only = ClearTally {
            position: true,
            ..ClearTally::default()
        };
        assert_eq!(describe_cleared(position_only), "your place in the book");

        // Singular and plural are different words, and a count of one must not
        // read "1 highlights".
        let one = ClearTally {
            highlights: 1,
            ..ClearTally::default()
        };
        assert_eq!(describe_cleared(one), "1 highlight");

        let mixed = ClearTally {
            position: true,
            bookmarks: 2,
            highlights: 3,
            quotes: 1,
            ink: 0,
        };
        assert_eq!(
            describe_cleared(mixed),
            "your place in the book, 2 bookmarks, 3 highlights and 1 quote"
        );
        assert!(
            !describe_cleared(mixed).contains("ink"),
            "ink was not cleared, so it must not be mentioned"
        );
    }

    /// The labels are what the reader sees in four different dialogs, so a
    /// stray duplicate would be confusing in a way no other test would catch.
    #[test]
    fn no_two_rows_say_the_same_thing() {
        for (i, a) in KIND_ROWS.iter().enumerate() {
            for (j, b) in KIND_ROWS.iter().enumerate() {
                if i == j {
                    continue;
                }
                assert_ne!(a.label, b.label, "rows {i} and {j} share a label");
                assert_ne!(a.noun, b.noun, "rows {i} and {j} share a noun");
            }
        }
    }
}
