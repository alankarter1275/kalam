//! The rail bell and its notification panel (design-system §45,
//! decision 148).
//!
//! The bell is a plain rail button carrying an overlay dot; the panel
//! is the shared §9 side-panel shell inside a `GtkRevealer`, meant to
//! be added to the app's root overlay by the shell. A transparent
//! catcher sits under the panel while it is open: it paints nothing
//! (the design has no dim layer here) and only turns the first click
//! outside the panel into a close. Esc closes through
//! [`NotificationPanel::attach_escape`].

use super::surface::{hairline, side_panel};
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

/// Events the panel lists. The history keeps more; the drawn list is
/// the last 25 (settings-spec §7), same as the retired Settings pane.
const MAX_PANEL_ENTRIES: usize = 25;
const PANEL_TRANSITION_MS: u32 = 180;

#[derive(Clone)]
pub struct NotificationPanel {
    button: gtk::Button,
    dot: gtk::Box,
    catcher: gtk::Box,
    revealer: gtk::Revealer,
    list: gtk::Box,
    clear: gtk::Button,
    open: Rc<Cell<bool>>,
}

impl NotificationPanel {
    pub fn new() -> Self {
        // The bell face: 22px glyph centred in the 44px rail disc,
        // with the 8px new-event dot at its top-right (§45).
        let icon = crate::icons::symbolic_with_classes(
            "preferences-system-notifications-symbolic",
            22,
            &["kalam-bell-icon"],
        );
        icon.set_halign(gtk::Align::Center);
        icon.set_valign(gtk::Align::Center);

        let dot = gtk::Box::new(gtk::Orientation::Vertical, 0);
        dot.add_css_class("kalam-bell-dot");
        dot.set_size_request(8, 8);
        dot.set_halign(gtk::Align::End);
        dot.set_valign(gtk::Align::Start);
        dot.set_margin_top(6);
        dot.set_margin_end(6);
        dot.set_can_target(false);
        dot.set_visible(false);

        let face = gtk::Overlay::new();
        face.set_child(Some(&icon));
        face.add_overlay(&dot);
        face.set_size_request(44, 44);

        let button = gtk::Button::new();
        button.add_css_class("kalam-nav-btn");
        button.add_css_class("kalam-bell-btn");
        button.set_child(Some(&face));
        button.set_halign(gtk::Align::Center);
        button.set_hexpand(false);
        button.set_focus_on_click(false);
        button.set_widget_name("nav-Bell");
        button.set_tooltip_text(Some("Notifications"));
        button.update_property(&[gtk::accessible::Property::Label("Notifications")]);

        // Invisible outside-click catcher (no dim layer, decision 148).
        let catcher = gtk::Box::new(gtk::Orientation::Vertical, 0);
        catcher.add_css_class("kalam-notify-catcher");
        catcher.set_halign(gtk::Align::Fill);
        catcher.set_valign(gtk::Align::Fill);
        catcher.set_hexpand(true);
        catcher.set_vexpand(true);
        catcher.set_can_target(true);
        catcher.set_visible(false);

        // The §9 panel shell, stretched to the overlay's height.
        let panel = side_panel(Some("Notifications"));
        let panel_root = panel.widget();
        panel_root.add_css_class("kalam-notify-panel");
        panel_root.set_valign(gtk::Align::Fill);
        panel_root.set_vexpand(true);
        panel.body().set_vexpand(true);

        let clear = gtk::Button::with_label("Clear history");
        clear.add_css_class("kalam-notify-clear");
        clear.set_halign(gtk::Align::End);
        clear.set_valign(gtk::Align::Center);
        panel.header().append(&clear);

        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        list.set_hexpand(true);
        list.set_valign(gtk::Align::Start);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_hexpand(true);
        scroller.set_vexpand(true);
        scroller.set_overlay_scrolling(true);
        scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroller.set_vscrollbar_policy(gtk::PolicyType::Automatic);
        scroller.set_child(Some(&list));
        panel.body().append(&scroller);

        let revealer = gtk::Revealer::new();
        revealer.set_transition_duration(PANEL_TRANSITION_MS);
        revealer.set_reveal_child(false);
        revealer.set_visible(false);
        revealer.set_halign(gtk::Align::Start);
        revealer.set_valign(gtk::Align::Fill);
        // 16 from the rail's outer edge (the rail is 68 wide), 16 from
        // the window's top and bottom (§45).
        revealer.set_margin_start(68 + 16);
        revealer.set_margin_top(16);
        revealer.set_margin_bottom(16);
        revealer.set_child(Some(&panel_root));

        let panel = Self {
            button,
            dot,
            catcher,
            revealer,
            list,
            clear,
            open: Rc::new(Cell::new(false)),
        };
        panel.wire();
        panel.refresh();
        panel
    }

    pub fn button(&self) -> gtk::Button {
        self.button.clone()
    }

    pub fn catcher(&self) -> gtk::Box {
        self.catcher.clone()
    }

    pub fn revealer(&self) -> gtk::Revealer {
        self.revealer.clone()
    }

    pub fn is_open(&self) -> bool {
        self.open.get()
    }

    pub fn close(&self) {
        if self.open.get() {
            self.set_open(false);
        }
    }

    /// Close on Esc while open (decisions 20 and 35). The shell adds
    /// this to the window next to its other permanent key handlers;
    /// it is inert while the panel is shut.
    pub fn attach_escape(&self, root: &gtk::Window) {
        let controller = gtk::EventControllerKey::new();
        let panel = self.clone();
        controller.connect_key_pressed(move |_, keyval, _, _| {
            if panel.is_open() && keyval == gtk::gdk::Key::Escape {
                panel.set_open(false);
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        root.add_controller(controller);
    }

    /// Rebuild the list from the history and re-set the dot. Called
    /// by the shell's `notify::on_history_changed` listener, and
    /// directly around open/close so the first paint is never stale.
    pub fn refresh(&self) {
        let unseen = crate::notify::has_unseen();
        self.dot.set_visible(!self.open.get() && unseen);
        self.button.set_tooltip_text(Some(if unseen {
            "Notifications — new events"
        } else {
            "Notifications"
        }));

        while let Some(child) = self.list.first_child() {
            self.list.remove(&child);
        }

        let entries: Vec<crate::notify::Entry> = crate::notify::history()
            .into_iter()
            .take(MAX_PANEL_ENTRIES)
            .collect();
        self.clear.set_visible(!entries.is_empty());

        if entries.is_empty() {
            self.list.append(&hairline());
            let empty = gtk::Label::new(Some(
                "Nothing yet. Imports, backups and problems show up here.",
            ));
            empty.add_css_class("kalam-notify-empty");
            empty.set_halign(gtk::Align::Start);
            empty.set_xalign(0.0);
            empty.set_wrap(true);
            empty.set_width_chars(1);
            empty.set_max_width_chars(42);
            empty.set_margin_top(12);
            self.list.append(&empty);
            return;
        }

        let today = crate::notify::today_stamp();
        let (todays, earlier): (Vec<_>, Vec<_>) =
            entries.iter().partition(|entry| entry.day == today);
        append_group(&self.list, "TODAY", &todays);
        append_group(&self.list, "EARLIER", &earlier);
    }

    fn wire(&self) {
        {
            let panel = self.clone();
            self.button.connect_clicked(move |_| {
                let next = !panel.is_open();
                panel.set_open(next);
            });
        }
        {
            let panel = self.clone();
            let click = gtk::GestureClick::new();
            click.connect_pressed(move |_, _, _, _| panel.close());
            self.catcher.add_controller(click);
        }
        {
            let panel = self.clone();
            self.clear.connect_clicked(move |_| {
                crate::notify::clear_history();
                panel.refresh();
            });
        }
    }

    fn set_open(&self, open: bool) {
        if self.open.get() == open {
            return;
        }
        self.open.set(open);

        // Opening clears the dot (decision 148); closing does too,
        // for anything that arrived while the list was on screen —
        // it was seen.
        crate::notify::mark_all_seen();

        self.catcher.set_visible(open);
        if open {
            self.button.add_css_class("open");
        } else {
            self.button.remove_css_class("open");
        }

        // Re-ask on every toggle: the Appearance pane can flip the
        // reduce-motion choice while the shell lives.
        self.revealer
            .set_transition_type(if crate::appearance::reduce_motion() {
                gtk::RevealerTransitionType::Crossfade
            } else {
                gtk::RevealerTransitionType::SlideRight
            });

        if open {
            self.refresh();
            self.revealer.set_visible(true);
            self.revealer.set_reveal_child(true);
            if self.clear.is_visible() {
                self.clear.grab_focus();
            }
        } else {
            self.revealer.set_reveal_child(false);
            // Take the revealer out of the overlay once the slide has
            // finished, so a shut panel is never allocated at all.
            let revealer = self.revealer.clone();
            let still_open = self.open.clone();
            gtk::glib::timeout_add_local_once(
                std::time::Duration::from_millis(PANEL_TRANSITION_MS as u64 + 40),
                move || {
                    if !still_open.get() {
                        revealer.set_visible(false);
                    }
                },
            );
            self.button.grab_focus();
        }
    }
}

fn append_group(list: &gtk::Box, heading: &str, entries: &[&crate::notify::Entry]) {
    if entries.is_empty() {
        return;
    }
    let label = gtk::Label::new(Some(heading));
    label.add_css_class("kalam-ui-label");
    label.add_css_class("kalam-notify-group");
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    if list.first_child().is_some() {
        label.set_margin_top(16);
    }
    list.append(&label);

    for entry in entries {
        list.append(&notification_row(entry));
    }
}

fn notification_row(entry: &crate::notify::Entry) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("kalam-notify-row");

    let marker = gtk::Box::new(gtk::Orientation::Vertical, 0);
    marker.add_css_class("kalam-notify-marker");
    if entry.kind == crate::notify::Kind::Error {
        marker.add_css_class("problem");
    }
    marker.set_size_request(8, 8);
    marker.set_valign(gtk::Align::Start);
    marker.set_margin_top(6);
    marker.set_can_target(false);
    row.append(&marker);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);

    // One message line (§45): the toast's title, with its detail
    // after a dash when it has one — an error's detail is the part
    // worth re-reading after the toast has faded.
    let detail = entry.detail.trim();
    let message_text = if detail.is_empty() {
        entry.title.clone()
    } else {
        format!("{} — {detail}", entry.title)
    };
    let message = gtk::Label::new(Some(&message_text));
    message.add_css_class("kalam-notify-message");
    message.set_halign(gtk::Align::Start);
    message.set_xalign(0.0);
    message.set_wrap(true);
    message.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    // A wrapping label's natural width is its one-line width; cap it
    // so a long path in an error cannot stretch the 340 panel.
    message.set_width_chars(1);
    message.set_max_width_chars(42);
    text.append(&message);

    let at = gtk::Label::new(Some(&entry.at));
    at.add_css_class("kalam-ui-meta");
    at.set_halign(gtk::Align::Start);
    at.set_xalign(0.0);
    text.append(&at);

    row.append(&text);
    row
}
