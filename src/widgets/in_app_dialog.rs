//! In-app dialogs: modal panels drawn **inside** the main window.
//!
//! # Why this exists
//!
//! A `gtk::Window` is a real top-level window. On a tiling compositor (Sway)
//! that means the compositor owns it: it can be tiled beside the app, sent to
//! another workspace, or left behind when you switch. None of that is what a
//! modal dialog means. `app.rs` already had a float layer for exactly this
//! reason, but only half the dialogs used it.
//!
//! # Why it is not the `app.rs` float layer
//!
//! That layer is driven by `AppMsg` and hosts *Relm4 components*. These
//! dialogs are plain functions that take an `on_confirm: impl Fn()` closure,
//! and a closure cannot travel through a `#[derive(Debug)]` message enum. So
//! this helper finds the nearest `gtk::Overlay` by walking up from any widget
//! and adds its own scrim + host to it. No app plumbing, no message round
//! trip, and pages stay unaware of `AppModel`.
//!
//! # Before you change this
//!
//! A `gtk::Window` used to do three things for these dialogs that an overlay
//! does not: destroy the widget tree on close (which breaks the
//! widget-holds-callback-holds-widget cycle), bound their height, and act as a
//! real top-level for portal dialogs. All three are replaced by hand here.
//! `docs/pitfalls.md` §2 explains each one — read it before simplifying
//! `teardown()` or removing a height cap.
//!
//! # Dismissal
//!
//! Three ways out, and the visible one is chosen per dialog:
//!
//! * **The backdrop** — clicking the dimmed area closes it.
//! * **Esc** — always.
//! * **A visible control** — see [`DialogExit`]. Backdrop and Esc are both
//!   *invisible* affordances, so a dialog whose only exits are invisible is
//!   still a trap. Every dialog here keeps one visible control; which one
//!   depends on what the dialog *is*, not on habit. (The book and series
//!   floats are the deliberate exception, and they do not use this helper —
//!   see [`DialogExit`] for why.)
//!
//! # Focus
//!
//! Tab is confined to the panel by `crate::widgets::focus_trap`. An overlay is
//! not a focus scope the way a window is, so without it Tab walks out of a
//! modal dialog and into the page behind — reachable, invisible, clickable
//! with Enter. The scrim stops the mouse; the trap stops the keyboard.

use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

/// The visible way out, chosen to match what the dialog is for.
///
/// Dialogs exist for different reasons and should not all be dismissed
/// identically — a ✕ on a half-filled form is ambiguous about what happens to
/// the typing, and a ✕ next to "Cancel" and "Delete" is just a vaguer third
/// option.
///
/// Two variants, and that is the finished set — not a stub waiting for more.
///
/// An earlier note here said the "detour" (‹ Back) and "transient overlay" (✕)
/// kinds would join the enum once the book and series float headers were
/// revisited. They will not, and the reason is worth keeping:
///
/// * The two floats were given **no visible exit at all** (user's call,
///   2026-09-03: read-only detours, so a stray click outside loses nothing).
///   An enum variant for "renders no control" that no caller can pass would be
///   dead code, and dead code fails the `-D warnings` build.
/// * The floats do not go through this helper anyway. They are Relm4
///   components hosted by the float layer in `app.rs`, which owns their scrim,
///   Esc key and header. Folding them in would mean routing a closure through
///   a `#[derive(Debug)]` message enum, which is the very thing this helper
///   exists to avoid.
///
/// What the two systems *do* share is behaviour, not types: Esc closes,
/// backdrop closes, and focus stays inside the panel
/// (`crate::widgets::focus_trap`, used by both).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogExit {
    /// The content supplies its own named buttons — "Done" on a picker,
    /// "Cancel"/"Delete" on a confirmation. The header adds no control,
    /// because two competing "get out" affordances are worse than one clear
    /// pair.
    ///
    /// Backdrop-click dismisses: these dialogs either write through as you go
    /// or are asking a question, so a click outside loses nothing.
    OwnButtons,
    /// A form holding **unsaved input**. Also supplies its own Cancel/Save, so
    /// again no header control.
    ///
    /// Backdrop-click is **disabled**: silently discarding a half-typed
    /// description because a click landed slightly off target is a bad trade.
    /// Esc still works, and Cancel is right there.
    UnsavedInput,
}

impl DialogExit {
    /// Whether clicking the dimmed backdrop dismisses the dialog.
    fn backdrop_closes(self) -> bool {
        !matches!(self, DialogExit::UnsavedInput)
    }
}

/// A dialog currently on screen. Dropping this does **not** close it; call
/// [`InAppDialog::close`], or use one of the built-in exits.
///
/// Every field is a cheap handle, so cloning this into a button callback is
/// fine. Clones share one `closed` flag, so whichever exit fires first wins
/// and the rest become no-ops.
#[derive(Clone)]
pub struct InAppDialog {
    overlay: gtk::Overlay,
    scrim: gtk::Box,
    host: gtk::Box,
    key_controller: gtk::EventControllerKey,
    /// Confines Tab to the panel. Lives on the root like `key_controller`, so
    /// it must come off at teardown too.
    focus_controller: gtk::EventControllerKey,
    root: gtk::Widget,
    closed: Rc<Cell<bool>>,
}

impl InAppDialog {
    /// Remove the dialog, its scrim, and its Esc handler.
    ///
    /// Safe to call twice: Esc, a backdrop click and a button can all race,
    /// and GTK warns if you remove an overlay child or a controller twice.
    pub fn close(&self) {
        teardown(
            &self.closed,
            &self.overlay,
            Some(&self.scrim),
            &self.host,
            &self.root,
            &self.key_controller,
            Some(&self.focus_controller),
        );
    }
}

/// Undo everything [`present`] added, exactly once.
///
/// A free function rather than a method so the Esc and backdrop handlers can
/// call it while capturing only what they need. That matters: a closure owned
/// (directly or not) by one of these widgets must not hold a strong reference
/// back to it, or the pair keeps each other alive forever — a leak of the
/// entire dialog, repeated on every open.
///
/// `scrim` is optional because the backdrop handler holds a weak reference to
/// it: by the time that handler runs the scrim may already be gone.
fn teardown(
    closed: &Rc<Cell<bool>>,
    overlay: &gtk::Overlay,
    scrim: Option<&gtk::Box>,
    host: &gtk::Box,
    root: &gtk::Widget,
    key_controller: &gtk::EventControllerKey,
    focus_controller: Option<&gtk::EventControllerKey>,
) {
    if closed.replace(true) {
        return;
    }
    if let Some(scrim) = scrim {
        if scrim.parent().is_some() {
            overlay.remove_overlay(scrim);
        }
    }
    if host.parent().is_some() {
        overlay.remove_overlay(host);
    }

    // Take the panel out of the host. Widget trees and their callbacks
    // reference each other in a loop — a button's closure holds the dialog,
    // the dialog holds the widgets, the widgets hold the button. Destroying a
    // `gtk::Window` used to cut that loop for us; removing the panel here does
    // the same job, so a closed dialog is actually freed instead of lingering
    // for the life of the process.
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }

    // The Esc handler lives on the window root and outlives the dialog, so it
    // must come off: otherwise every dialog ever opened leaves one behind, and
    // each would swallow Esc from whatever needs it next.
    root.remove_controller(key_controller);

    // Same reasoning for the focus trap: left attached, it would keep
    // swallowing Tab for a panel that is no longer on screen.
    if let Some(focus_controller) = focus_controller {
        root.remove_controller(focus_controller);
    }
}

/// Walk up from `widget` to the nearest enclosing [`gtk::Overlay`].
///
/// The app's root is an overlay (`root_overlay` in `app.rs`), so any widget
/// inside the window finds it. Returns `None` only if the widget is not in a
/// window yet, in which case the caller should fall back to a real window.
fn nearest_overlay(widget: &impl IsA<gtk::Widget>) -> Option<gtk::Overlay> {
    let mut current = widget.as_ref().parent();
    let mut found = None;
    while let Some(w) = current {
        if let Ok(overlay) = w.clone().downcast::<gtk::Overlay>() {
            // Keep going: the outermost overlay is the app's root one, and
            // that is the one that covers the whole window.
            found = Some(overlay);
        }
        current = w.parent();
    }
    found
}

/// Show `content` as a modal panel inside the window containing `anchor`.
///
/// Returns `None` when there is no enclosing overlay — the caller should then
/// fall back to a `gtk::Window`, which is worse on Sway but better than
/// showing nothing.
///
/// `title` labels the panel. `exit` decides the visible control and whether
/// backdrop-click dismisses it.
pub fn present(
    anchor: &impl IsA<gtk::Widget>,
    title: &str,
    exit: DialogExit,
    content: &impl IsA<gtk::Widget>,
) -> Option<InAppDialog> {
    present_inner(anchor, title, exit, content, None, None, false)
}

/// Show `content` in the redesign dialog surface (design-system §§13, 18):
/// a fixed 420px form width or a 560×660 list request, card fill, radius 14,
/// no shadow, and the Playfair section title. The scrim, Esc handling,
/// focus trap and teardown are exactly those of [`present`]; only the
/// panel's measurable size and classes differ.
pub fn present_redesign(
    anchor: &impl IsA<gtk::Widget>,
    title: &str,
    exit: DialogExit,
    content: &impl IsA<gtk::Widget>,
    width: i32,
    height: Option<i32>,
) -> Option<InAppDialog> {
    present_inner(
        anchor,
        title,
        exit,
        content,
        Some(width.max(1)),
        height.map(|height| height.max(1)),
        true,
    )
}

fn present_inner(
    anchor: &impl IsA<gtk::Widget>,
    title: &str,
    exit: DialogExit,
    content: &impl IsA<gtk::Widget>,
    panel_width: Option<i32>,
    panel_height: Option<i32>,
    redesign: bool,
) -> Option<InAppDialog> {
    let overlay = nearest_overlay(anchor)?;

    // Dimmed backdrop. `can_target` is what stops clicks reaching the page
    // underneath; without it a "modal" dialog is decoration.
    let scrim = gtk::Box::new(gtk::Orientation::Vertical, 0);
    scrim.add_css_class("kalam-float-scrim");
    scrim.set_halign(gtk::Align::Fill);
    scrim.set_valign(gtk::Align::Fill);
    scrim.set_hexpand(true);
    scrim.set_vexpand(true);
    scrim.set_can_target(true);

    // The panel itself, centred. Added to the overlay *after* the scrim so it
    // sits on top and keeps its own clicks.
    let host = gtk::Box::new(gtk::Orientation::Vertical, 0);
    host.add_css_class("kalam-float-stage");
    host.set_halign(gtk::Align::Center);
    host.set_valign(gtk::Align::Center);
    host.set_margin_top(24);
    host.set_margin_bottom(24);
    host.set_margin_start(24);
    host.set_margin_end(24);

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.add_css_class("kalam-float");
    panel.add_css_class("kalam-in-app-dialog");
    if redesign {
        panel.add_css_class("kalam-ui-dialog-panel");
    }
    if let Some(width) = panel_width {
        // Height stays content-sized for form dialogs. For list dialogs it
        // is a request rather than a hard allocation: the list inside the
        // panel owns scrolling, and GTK CSS has no usable max-height here.
        panel.set_size_request(width, panel_height.unwrap_or(-1));
    }

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    header.add_css_class("kalam-in-app-dialog-head");
    if redesign {
        header.add_css_class("kalam-ui-dialog-head");
    }
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("kalam-card-title");
    if redesign {
        title_label.add_css_class("kalam-ui-dialog-title");
    }
    title_label.set_halign(gtk::Align::Start);
    title_label.set_hexpand(true);
    title_label.set_xalign(0.0);
    header.append(&title_label);

    panel.append(&header);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 0);
    body.add_css_class("kalam-in-app-dialog-body");
    if redesign {
        body.add_css_class("kalam-ui-dialog-body");
    }
    body.append(content.as_ref());
    panel.append(&body);
    host.append(&panel);

    overlay.add_overlay(&scrim);
    overlay.add_overlay(&host);

    // Esc, on the window root so it works wherever focus currently is.
    let key_controller = gtk::EventControllerKey::new();
    let root: gtk::Widget = anchor
        .as_ref()
        .root()
        .map(|r| r.upcast::<gtk::Widget>())
        .unwrap_or_else(|| overlay.clone().upcast());

    // Tab must not walk out of a modal panel into the page behind it. The
    // scrim already blocks the mouse; this blocks the keyboard.
    let focus_controller = crate::widgets::focus_trap::controller(&host);
    root.add_controller(focus_controller.clone());

    let closed = Rc::new(Cell::new(false));
    let dialog = InAppDialog {
        overlay: overlay.clone(),
        scrim: scrim.clone(),
        host: host.clone(),
        key_controller: key_controller.clone(),
        focus_controller: focus_controller.clone(),
        root: root.clone(),
        closed: closed.clone(),
    };

    // Capture phase: Esc must work wherever focus happens to be, including
    // inside an entry in the dialog.
    key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        // Parts, not the dialog struct: this closure is owned by the key
        // controller, and the struct owns that controller.
        let overlay = overlay.clone();
        let scrim = scrim.clone();
        let host = host.clone();
        let root_for_key = root.clone();
        let closed = closed.clone();
        let focus_for_key = focus_controller.clone();
        key_controller.connect_key_pressed(move |controller, keyval, _, _| {
            if keyval != gtk::gdk::Key::Escape {
                return gtk::glib::Propagation::Proceed;
            }
            // Removing a controller from inside its own handler would drop
            // this closure mid-call, so do the removal once the event is done.
            let root_for_key = root_for_key.clone();
            let controller = controller.clone();
            let closed = closed.clone();
            let overlay = overlay.clone();
            let scrim = scrim.clone();
            let host = host.clone();
            let focus_for_key = focus_for_key.clone();
            gtk::glib::idle_add_local_once(move || {
                teardown(
                    &closed,
                    &overlay,
                    Some(&scrim),
                    &host,
                    &root_for_key,
                    &controller,
                    Some(&focus_for_key),
                );
            });
            gtk::glib::Propagation::Stop
        });
    }
    root.add_controller(key_controller.clone());

    let click = gtk::GestureClick::new();
    if exit.backdrop_closes() {
        let overlay = overlay.clone();
        let host = host.clone();
        // Weak: this closure ends up owned by the scrim, so a strong handle
        // back to the scrim would keep the whole dialog alive after closing.
        let scrim_weak = scrim.downgrade();
        let root_for_click = root.clone();
        let key_for_click = key_controller.clone();
        let focus_for_click = focus_controller.clone();
        let closed = closed.clone();
        click.connect_pressed(move |_, _, _, _| {
            teardown(
                &closed,
                &overlay,
                scrim_weak.upgrade().as_ref(),
                &host,
                &root_for_click,
                &key_for_click,
                Some(&focus_for_click),
            );
        });
    }
    // Attached either way: even when a backdrop click must not dismiss the
    // dialog, it still has to be swallowed so it cannot reach the page behind.
    scrim.add_controller(click);

    Some(dialog)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn form_dialogs_do_not_close_on_a_stray_backdrop_click() {
        // A half-typed shelf description must not vanish because a click
        // landed slightly outside the panel.
        assert!(!DialogExit::UnsavedInput.backdrop_closes());
        // A picker writes through as you tick, and a confirmation is asking a
        // question — clicking away loses nothing in either case.
        assert!(DialogExit::OwnButtons.backdrop_closes());
    }
}
