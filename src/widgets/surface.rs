//! Shared surfaces: inset cards, outlined sections, side-panel shells,
//! state cards and quiet notices (design-system §§9, 10, 14 and 30).

use super::pill_button::{ghost_button, icon_button};
use gtk::prelude::*;

/// Recessed card used inside reader side panels. Add rows with
/// [`add_inset_row`] so the first row does not get a leading hairline.
pub fn inset_card() -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("kalam-ui-inset-card");
    card
}

pub fn add_inset_row(card: &gtk::Box, row: &impl IsA<gtk::Widget>) {
    row.as_ref().add_css_class("kalam-ui-inset-row");
    card.append(row.as_ref());
}

#[derive(Clone)]
pub struct OutlinedSection {
    root: gtk::Box,
    body: gtk::Box,
}

impl OutlinedSection {
    pub fn widget(&self) -> gtk::Box {
        self.root.clone()
    }

    pub fn body(&self) -> gtk::Box {
        self.body.clone()
    }
}

/// A bordered page section with the design's section heading. The returned
/// body is separate from the root so callers can append content without
/// disturbing the heading.
pub fn outlined_section(title: &str) -> OutlinedSection {
    let root = gtk::Box::new(gtk::Orientation::Vertical, crate::style::spacing::MD);
    root.add_css_class("kalam-ui-outlined-section");

    let heading = gtk::Label::new(Some(title));
    heading.add_css_class("kalam-ui-section-title");
    heading.set_halign(gtk::Align::Start);
    heading.set_xalign(0.0);
    root.append(&heading);

    let body = gtk::Box::new(gtk::Orientation::Vertical, crate::style::spacing::SM);
    root.append(&body);

    OutlinedSection { root, body }
}

#[derive(Clone)]
pub struct SidePanel {
    root: gtk::Box,
    header: gtk::Box,
    body: gtk::Box,
}

impl SidePanel {
    pub fn widget(&self) -> gtk::Box {
        self.root.clone()
    }

    pub fn header(&self) -> gtk::Box {
        self.header.clone()
    }

    pub fn body(&self) -> gtk::Box {
        self.body.clone()
    }
}

/// 340px floating-panel shell (§9). The header is intentionally exposed:
/// each reader fills it with a different tab/pin/close combination, while
/// the outer geometry and surface remain shared.
pub fn side_panel(title: Option<&str>) -> SidePanel {
    let root = gtk::Box::new(gtk::Orientation::Vertical, crate::style::spacing::MD);
    root.add_css_class("kalam-ui-side-panel");
    root.set_width_request(crate::style::size::SIDE_PANEL_WIDTH);
    root.set_valign(gtk::Align::Start);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, crate::style::spacing::XS);
    if let Some(title) = title {
        let label = gtk::Label::new(Some(title));
        label.add_css_class("kalam-ui-section-title");
        label.set_hexpand(true);
        label.set_halign(gtk::Align::Start);
        label.set_xalign(0.0);
        header.append(&label);
    }
    root.append(&header);

    let body = gtk::Box::new(gtk::Orientation::Vertical, crate::style::spacing::SM);
    root.append(&body);

    SidePanel { root, header, body }
}

/// Convenience close button for a side-panel header.
pub fn side_panel_close_button(on_close: impl Fn() + 'static) -> gtk::Button {
    icon_button("window-close-symbolic", "Close panel", 32, false, on_close)
}

fn state_card_shell(heading: &str, message: &str) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, crate::style::spacing::XS);
    card.add_css_class("kalam-ui-state-card");
    card.set_halign(gtk::Align::Fill);

    let title = gtk::Label::new(Some(heading));
    title.add_css_class("kalam-ui-state-heading");
    title.set_halign(gtk::Align::Center);
    title.set_xalign(0.5);
    title.set_wrap(true);
    card.append(&title);

    let body = gtk::Label::new(Some(message));
    body.add_css_class("kalam-ui-meta");
    body.set_halign(gtk::Align::Center);
    body.set_xalign(0.5);
    body.set_wrap(true);
    card.append(&body);
    card
}

/// Dashed blocking-state card (§10).
pub fn state_card(heading: &str, message: &str) -> gtk::Box {
    state_card_shell(heading, message)
}

/// State card with the optional ghost action used by empty/error states.
pub fn state_card_with_action(
    heading: &str,
    message: &str,
    action_label: &str,
    on_action: impl Fn() + 'static,
) -> gtk::Box {
    let card = state_card_shell(heading, message);
    let action = ghost_button(action_label, on_action);
    action.set_halign(gtk::Align::Center);
    action.set_margin_top(crate::style::spacing::XS);
    card.append(&action);
    card
}

/// Non-blocking inline notice (§30), separated by a single hairline.
pub fn quiet_notice(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-ui-quiet-notice");
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}

pub fn hairline() -> gtk::Separator {
    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    separator.add_css_class("kalam-ui-hairline");
    separator
}
