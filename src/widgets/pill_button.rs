//! Pill and circular icon buttons from design-system §§6, 12 and 13.

use gtk::prelude::*;

/// The four text-button weights used by the redesign. Destructive actions
/// use accent text/outline rather than becoming a second filled primary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PillButtonKind {
    Primary,
    Ghost,
    BoxedGhost,
    Danger,
}

impl PillButtonKind {
    fn css_class(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Ghost => "ghost",
            Self::BoxedGhost => "boxed-ghost",
            Self::Danger => "danger",
        }
    }
}

/// Build a 40px pill button and connect its click callback.
pub fn pill_button(
    label: &str,
    kind: PillButtonKind,
    on_click: impl Fn() + 'static,
) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.add_css_class("kalam-ui-pill-button");
    button.add_css_class(kind.css_class());
    button.set_size_request(-1, crate::style::size::CONTROL);
    button.set_can_focus(true);
    button.connect_clicked(move |_| on_click());
    button
}

pub fn primary_button(label: &str, on_click: impl Fn() + 'static) -> gtk::Button {
    pill_button(label, PillButtonKind::Primary, on_click)
}

pub fn ghost_button(label: &str, on_click: impl Fn() + 'static) -> gtk::Button {
    pill_button(label, PillButtonKind::Ghost, on_click)
}

pub fn boxed_ghost_button(label: &str, on_click: impl Fn() + 'static) -> gtk::Button {
    pill_button(label, PillButtonKind::BoxedGhost, on_click)
}

/// Circular icon-only button. `label` is the accessible/tooltip text; it is
/// deliberately separate from the symbolic icon name.
pub fn icon_button(
    icon_name: &str,
    label: &str,
    size: i32,
    bordered: bool,
    on_click: impl Fn() + 'static,
) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("kalam-ui-icon-button");
    if bordered {
        button.add_css_class("bordered");
    }
    button.set_size_request(size.clamp(32, 48), size.clamp(32, 48));
    button.set_tooltip_text(Some(label));

    let image = gtk::Image::from_icon_name(icon_name);
    image.set_pixel_size(18);
    button.set_child(Some(&image));
    button.connect_clicked(move |_| on_click());
    button
}

/// Update an icon button's selected appearance without rebuilding it.
pub fn set_icon_button_selected(button: &gtk::Button, selected: bool) {
    if selected {
        button.add_css_class("active");
    } else {
        button.remove_css_class("active");
    }
}
