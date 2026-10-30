//! Custom menu and submenu popovers from design-system §17.
//!
//! GTK's built-in menu rendering cannot produce the approved 14px-radius
//! card, 40px rows, tick circles, count badges or open-submenu fill without
//! becoming a stack of page-local classes. This small builder keeps those
//! pieces in one place while leaving each row a real GTK button, so focus,
//! keyboard activation and popover dismissal remain native.

use super::chip::count_badge;
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

/// The three fixed menu widths named by §17. The widest is reserved for the
/// reader's Tools submenus, whose labels are materially longer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuWidth {
    Short,
    Wide,
    Tools,
}

impl MenuWidth {
    pub fn pixels(self) -> i32 {
        match self {
            Self::Short => 210,
            Self::Wide => 240,
            Self::Tools => 260,
        }
    }
}

/// A popover plus the vertical list its builder methods append to.
///
/// Cloning this value clones GTK handles, not the menu. A menu is consumed
/// by [`menu_button`] or by another menu's [`MenuPopover::add_submenu`].
#[derive(Clone)]
pub struct MenuPopover {
    popover: gtk::Popover,
    list: gtk::Box,
}

impl MenuPopover {
    pub fn popover(&self) -> gtk::Popover {
        self.popover.clone()
    }

    pub fn list(&self) -> gtk::Box {
        self.list.clone()
    }

    /// Add a normal action row. Selecting it closes this menu and every
    /// ancestor menu before `on_select` runs.
    pub fn add_item(&self, label: &str, on_select: impl Fn() + 'static) {
        self.add_action_item(label, None, false, on_select);
    }

    /// Add an action row with the §17 count badge on its right edge.
    pub fn add_item_with_count(
        &self,
        label: &str,
        count: usize,
        on_select: impl Fn() + 'static,
    ) {
        self.add_action_item(
            label,
            Some(count_badge(count).upcast::<gtk::Widget>()),
            false,
            on_select,
        );
    }

    /// Add an action row with a muted shortcut hint on its right
    /// edge (the editor's Tools menu: Ctrl+Shift+F, F7).
    pub fn add_item_with_hint(
        &self,
        label: &str,
        hint: &str,
        on_select: impl Fn() + 'static,
    ) {
        let hint_label = gtk::Label::new(Some(hint));
        hint_label.add_css_class("kalam-ui-menu-hint");
        hint_label.set_halign(gtk::Align::End);
        hint_label.set_valign(gtk::Align::Center);
        self.add_action_item(
            label,
            Some(hint_label.upcast::<gtk::Widget>()),
            false,
            on_select,
        );
    }

    /// Add a destructive action row. §17 uses accent text for destructive
    /// menu items; there is no filled danger surface in a menu.
    pub fn add_destructive_item(&self, label: &str, on_select: impl Fn() + 'static) {
        self.add_action_item(label, None, true, on_select);
    }

    /// Add a checklist row. Unlike an action item it stays open, because a
    /// checklist is normally adjusted more than once per opening.
    pub fn add_check_item(
        &self,
        label: &str,
        checked: bool,
        on_toggle: impl Fn(bool) + 'static,
    ) {
        let checked_state = Rc::new(Cell::new(checked));

        let tick = gtk::Label::new(Some("✓"));
        tick.add_css_class("kalam-ui-menu-check");
        if checked {
            tick.add_css_class("checked");
        }
        tick.set_size_request(22, 22);
        tick.set_halign(gtk::Align::Center);
        tick.set_valign(gtk::Align::Center);
        tick.set_xalign(0.5);
        tick.set_yalign(0.5);

        let button = gtk::Button::new();
        style_item_button(&button, false);
        button.set_child(Some(&item_content(
            label,
            Some(tick.clone().upcast::<gtk::Widget>()),
            None,
        )));

        let checked_for_click = checked_state.clone();
        button.connect_clicked(move |_| {
            let next = !checked_for_click.get();
            checked_for_click.set(next);
            if next {
                tick.add_css_class("checked");
            } else {
                tick.remove_css_class("checked");
            }
            on_toggle(next);
        });
        self.list.append(&button);
    }

    /// Add the hairline that separates action groups.
    pub fn add_separator(&self) {
        let separator = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        separator.add_css_class("kalam-ui-menu-separator");
        separator.set_size_request(-1, 1);
        self.list.append(&separator);
    }

    /// Add a side-opening submenu. `GtkMenuButton` supplies the toggle and
    /// popover parentage; this builder adds the row shape, chevron, the
    /// open-parent fill, and the one-open-at-a-time rule.
    pub fn add_submenu(&self, label: &str, submenu: MenuPopover) {
        let chevron = gtk::Label::new(Some("›"));
        chevron.add_css_class("kalam-ui-menu-chevron");
        chevron.set_halign(gtk::Align::End);
        chevron.set_valign(gtk::Align::Center);

        let button = gtk::MenuButton::new();
        button.add_css_class("kalam-ui-menu-item");
        button.set_always_show_arrow(false);
        button.set_direction(gtk::ArrowType::Right);
        button.set_hexpand(true);
        button.set_halign(gtk::Align::Fill);
        button.set_can_focus(true);
        button.set_child(Some(&item_content(
            label,
            None,
            Some(chevron.upcast::<gtk::Widget>()),
        )));

        submenu.popover.set_position(gtk::PositionType::Right);
        button.set_popover(Some(&submenu.popover));
        button.connect_active_notify(|button| {
            if button.is_active() {
                close_sibling_submenus(button);
                button.add_css_class("open");
            } else {
                button.remove_css_class("open");
            }
        });
        self.list.append(&button);
    }

    fn add_action_item(
        &self,
        label: &str,
        trailing: Option<gtk::Widget>,
        destructive: bool,
        on_select: impl Fn() + 'static,
    ) {
        let button = gtk::Button::new();
        style_item_button(&button, destructive);
        button.set_child(Some(&item_content(label, None, trailing)));
        button.connect_clicked(move |clicked| {
            close_ancestor_popovers(clicked.upcast_ref::<gtk::Widget>());
            on_select();
        });
        self.list.append(&button);
    }
}

/// Build an empty §17 popover at one of the approved widths.
pub fn menu_popover(width: MenuWidth) -> MenuPopover {
    let popover = gtk::Popover::new();
    popover.add_css_class("kalam-ui-menu");
    popover.set_has_arrow(false);
    popover.set_position(gtk::PositionType::Bottom);
    popover.set_autohide(true);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.add_css_class("kalam-ui-menu-list");
    list.set_size_request(width.pixels(), -1);
    popover.set_child(Some(&list));

    // A submenu is visually a child of this popover. If a backdrop click or
    // Escape closes the parent first, do not leave its side menu floating.
    let list_for_close = list.clone();
    popover.connect_closed(move |_| close_child_submenus(&list_for_close));

    MenuPopover { popover, list }
}

/// Attach a menu to a circular, arrowless icon trigger.
pub fn menu_button(icon_name: &str, label: &str, menu: MenuPopover) -> gtk::MenuButton {
    let button = gtk::MenuButton::new();
    button.add_css_class("kalam-ui-menu-trigger");
    button.set_icon_name(icon_name);
    button.set_always_show_arrow(false);
    button.set_tooltip_text(Some(label));
    button.set_size_request(
        crate::style::size::CONTROL,
        crate::style::size::CONTROL,
    );
    button.set_popover(Some(&menu.popover));
    button
}

fn style_item_button(button: &gtk::Button, destructive: bool) {
    button.add_css_class("kalam-ui-menu-item");
    if destructive {
        button.add_css_class("destructive");
    }
    button.set_hexpand(true);
    button.set_halign(gtk::Align::Fill);
    button.set_can_focus(true);
}

fn item_content(
    label: &str,
    leading: Option<gtk::Widget>,
    trailing: Option<gtk::Widget>,
) -> gtk::Box {
    let content = gtk::Box::new(
        gtk::Orientation::Horizontal,
        crate::style::spacing::XS,
    );
    content.add_css_class("kalam-ui-menu-item-content");
    content.set_hexpand(true);
    content.set_valign(gtk::Align::Center);

    if let Some(leading) = leading {
        leading.set_valign(gtk::Align::Center);
        content.append(&leading);
    }

    let text = gtk::Label::new(Some(label));
    text.add_css_class("kalam-ui-menu-item-label");
    text.set_hexpand(true);
    text.set_halign(gtk::Align::Start);
    text.set_xalign(0.0);
    text.set_ellipsize(gtk::pango::EllipsizeMode::End);
    content.append(&text);

    if let Some(trailing) = trailing {
        trailing.set_halign(gtk::Align::End);
        trailing.set_valign(gtk::Align::Center);
        content.append(&trailing);
    }
    content
}

fn close_ancestor_popovers(widget: &gtk::Widget) {
    let mut current = Some(widget.clone());
    while let Some(candidate) = current {
        if let Ok(popover) = candidate.clone().downcast::<gtk::Popover>() {
            popover.popdown();
        }
        current = candidate.parent();
    }
}

fn close_sibling_submenus(active: &gtk::MenuButton) {
    let Some(parent) = active.parent() else {
        return;
    };
    let mut child = parent.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        if let Ok(menu_button) = widget.downcast::<gtk::MenuButton>() {
            if menu_button != *active && menu_button.is_active() {
                menu_button.popdown();
            }
        }
        child = next;
    }
}

fn close_child_submenus(list: &gtk::Box) {
    let mut child = list.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        if let Ok(menu_button) = widget.downcast::<gtk::MenuButton>() {
            if menu_button.is_active() {
                menu_button.popdown();
            }
        }
        child = next;
    }
}

#[cfg(test)]
mod tests {
    use super::MenuWidth;

    #[test]
    fn menu_widths_match_section_17() {
        assert_eq!(MenuWidth::Short.pixels(), 210);
        assert_eq!(MenuWidth::Wide.pixels(), 240);
        assert_eq!(MenuWidth::Tools.pixels(), 260);
    }
}
