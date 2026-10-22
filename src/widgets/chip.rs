//! Filter chips, key chips and count badges (design-system §§10, 13, 17).

use gtk::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChipSize {
    Standard,
    Search,
}

/// A toggleable filter chip. GTK's checked state carries the selection, so
/// keyboard and assistive-technology behaviour come from the toolkit.
pub fn filter_chip(
    label: &str,
    active: bool,
    size: ChipSize,
    on_toggled: impl Fn(bool) + 'static,
) -> gtk::ToggleButton {
    let chip = gtk::ToggleButton::with_label(label);
    chip.add_css_class("kalam-ui-chip");
    if size == ChipSize::Search {
        chip.add_css_class("search");
    }
    chip.set_active(active);
    chip.connect_toggled(move |chip| on_toggled(chip.is_active()));
    chip
}

/// A non-interactive chip used for tags and metadata.
pub fn chip_label(label: &str, active: bool) -> gtk::Label {
    let chip = gtk::Label::new(Some(label));
    chip.add_css_class("kalam-ui-chip-label");
    if active {
        chip.add_css_class("active");
    }
    chip.set_valign(gtk::Align::Center);
    chip
}

/// A keyboard-shortcut chip; `recording` is the capture-in-progress state.
pub fn key_chip(label: &str, recording: bool) -> gtk::Label {
    let chip = gtk::Label::new(Some(label));
    chip.add_css_class("kalam-ui-key-chip");
    if recording {
        chip.add_css_class("recording");
    }
    chip.set_valign(gtk::Align::Center);
    chip
}

/// The small outlined accent count used by menus and drag ghosts.
pub fn count_badge(count: usize) -> gtk::Label {
    let badge = gtk::Label::new(Some(&count.to_string()));
    badge.add_css_class("kalam-ui-count-badge");
    badge.set_valign(gtk::Align::Center);
    badge
}
