//! The section-43 dropdown (`design-system.md` section 43, decision
//! 157): a 36-high field with a 1px border, radius 10, 13px value text
//! and a 12px chevron, opening the shared section-17 popover
//! right-aligned at 220 wide. Rows are 40 high with the current choice
//! carrying a 14px accent tick at the right; a footer action (the
//! shelf pickers' "New shelf...") sits below a divider in accent text.
//! Typeface options can render in their own font, the way the same
//! choice renders in the reading panel.
//!
//! Built on the same popover machinery as [`super::menu`] rather than
//! `GtkDropDown`, whose list rows cannot carry per-row fonts or a
//! footer action.
//!
//! First called by the Settings page, which lands in the next tranche
//! of this phase; the allow below comes off with it.
#![allow(dead_code)]

use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

/// One choice in a [`dropdown`]. Typeface pickers pass
/// [`DropdownOption::in_font`] so each row previews its own face.
#[derive(Clone)]
pub struct DropdownOption {
    label: String,
    font_family: Option<String>,
}

impl DropdownOption {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            font_family: None,
        }
    }

    /// An option rendered in the given font family, in the list and in
    /// the field once chosen.
    pub fn in_font(label: impl Into<String>, family: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            font_family: Some(family.into()),
        }
    }
}

/// Handle to a built dropdown: the field widget plus silent
/// selection access for hosts that re-sync from loaded state.
pub struct Dropdown {
    button: gtk::MenuButton,
    value: gtk::Label,
    options: Rc<Vec<DropdownOption>>,
    ticks: Rc<Vec<gtk::Label>>,
    selected: Rc<Cell<usize>>,
}

impl Dropdown {
    pub fn widget(&self) -> gtk::MenuButton {
        self.button.clone()
    }

    pub fn selected(&self) -> usize {
        self.selected.get()
    }

    /// Move the selection without firing the change callback (a host
    /// loading stored state, or reverting a refused choice).
    pub fn set_selected(&self, index: usize) {
        apply_selection(
            &self.value,
            &self.options,
            &self.ticks,
            &self.selected,
            index,
        );
    }
}

fn set_option_text(label: &gtk::Label, option: &DropdownOption) {
    match &option.font_family {
        Some(family) => label.set_markup(&format!(
            "<span font_family=\"{}\">{}</span>",
            gtk::glib::markup_escape_text(family),
            gtk::glib::markup_escape_text(&option.label)
        )),
        None => label.set_text(&option.label),
    }
}

fn apply_selection(
    value: &gtk::Label,
    options: &[DropdownOption],
    ticks: &[gtk::Label],
    selected: &Cell<usize>,
    index: usize,
) {
    let index = index.min(options.len().saturating_sub(1));
    selected.set(index);
    if let Some(option) = options.get(index) {
        set_option_text(value, option);
    }
    for (i, tick) in ticks.iter().enumerate() {
        tick.set_visible(i == index);
    }
}

/// Build a section-43 dropdown. `on_change` fires with the chosen
/// option index after the popover closes; `footer`, when present,
/// renders the accent action row under a divider and fires
/// `on_footer` instead.
pub fn dropdown(
    accessible_label: &str,
    options: Vec<DropdownOption>,
    selected: usize,
    footer: Option<&str>,
    on_change: impl Fn(usize) + 'static,
    on_footer: impl Fn() + 'static,
) -> Dropdown {
    let options = Rc::new(options);
    let selected_cell = Rc::new(Cell::new(
        selected.min(options.len().saturating_sub(1)),
    ));

    // The field: value text left, chevron right (section 43).
    let value = gtk::Label::new(None);
    value.add_css_class("kalam-ui-dropdown-value");
    value.set_halign(gtk::Align::Start);
    value.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let chevron = gtk::Image::from_icon_name("pan-down-symbolic");
    chevron.add_css_class("kalam-ui-dropdown-chevron");
    chevron.set_pixel_size(12);
    let field = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    field.append(&value);
    field.append(&chevron);

    let button = gtk::MenuButton::new();
    button.add_css_class("kalam-ui-menu-trigger");
    button.add_css_class("kalam-ui-dropdown");
    button.set_always_show_arrow(false);
    button.set_child(Some(&field));
    button.set_tooltip_text(Some(accessible_label));
    button.update_property(&[gtk::accessible::Property::Label(accessible_label)]);

    // The list: section-17 popover, 220 wide, right-aligned under the
    // field (section 43).
    let popover = gtk::Popover::new();
    popover.add_css_class("kalam-ui-menu");
    popover.set_has_arrow(false);
    popover.set_position(gtk::PositionType::Bottom);
    popover.set_halign(gtk::Align::End);
    popover.set_autohide(true);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.add_css_class("kalam-ui-menu-list");
    list.set_size_request(220, -1);

    let on_change = Rc::new(on_change);
    let mut tick_labels = Vec::with_capacity(options.len());
    let mut row_buttons = Vec::with_capacity(options.len());
    for (index, option) in options.iter().enumerate() {
        let row = gtk::Button::new();
        row.add_css_class("kalam-ui-menu-item");
        row.set_halign(gtk::Align::Fill);

        let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        content.add_css_class("kalam-ui-dropdown-row-content");
        let text = gtk::Label::new(None);
        text.add_css_class("kalam-ui-menu-item-label");
        text.set_halign(gtk::Align::Start);
        text.set_hexpand(true);
        text.set_ellipsize(gtk::pango::EllipsizeMode::End);
        set_option_text(&text, option);
        let tick = gtk::Label::new(Some("\u{2713}"));
        tick.add_css_class("kalam-ui-dropdown-tick");
        tick.set_halign(gtk::Align::End);
        tick.set_valign(gtk::Align::Center);
        content.append(&text);
        content.append(&tick);
        row.set_child(Some(&content));

        {
            let popover = popover.clone();
            let value = value.clone();
            let options = options.clone();
            let selected_cell = selected_cell.clone();
            let on_change = on_change.clone();
            // The ticks re-sync in the popover's closed handler below,
            // once every row exists.
            row.connect_clicked(move |_| {
                popover.popdown();
                if selected_cell.get() != index {
                    selected_cell.set(index);
                    if let Some(option) = options.get(index) {
                        set_option_text(&value, option);
                    }
                    on_change(index);
                }
            });
        }
        list.append(&row);
        tick_labels.push(tick);
        row_buttons.push(row);
    }
    let ticks = Rc::new(tick_labels);

    // Choosing a row moves every tick; the click closures above only
    // tracked the index, so keep the ticks in sync by watching the
    // popover close after a selection change.
    {
        let ticks = ticks.clone();
        let selected_cell = selected_cell.clone();
        popover.connect_closed(move |_| {
            let current = selected_cell.get();
            for (i, tick) in ticks.iter().enumerate() {
                tick.set_visible(i == current);
            }
        });
    }

    if let Some(footer_label) = footer {
        let separator = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        separator.add_css_class("kalam-ui-menu-separator");
        list.append(&separator);

        let row = gtk::Button::new();
        row.add_css_class("kalam-ui-menu-item");
        row.add_css_class("accent");
        row.set_halign(gtk::Align::Fill);
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        content.add_css_class("kalam-ui-dropdown-row-content");
        let text = gtk::Label::new(Some(footer_label));
        text.add_css_class("kalam-ui-menu-item-label");
        text.set_halign(gtk::Align::Start);
        content.append(&text);
        row.set_child(Some(&content));
        {
            let popover = popover.clone();
            row.connect_clicked(move |_| {
                popover.popdown();
                on_footer();
            });
        }
        list.append(&row);
    }

    // Long lists scroll without a visible bar (section 43).
    if options.len() > 8 {
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::External);
        scroller.set_size_request(220, 8 * 40 + 12);
        scroller.set_child(Some(&list));
        popover.set_child(Some(&scroller));
    } else {
        popover.set_child(Some(&list));
    }

    // Open with the current choice focused, so arrow keys move from
    // the standing value (section 43 keyboard behaviour).
    {
        let row_buttons = row_buttons.clone();
        let selected_cell = selected_cell.clone();
        popover.connect_show(move |_| {
            if let Some(row) = row_buttons.get(selected_cell.get()) {
                row.grab_focus();
            }
        });
    }

    button.set_popover(Some(&popover));

    let handle = Dropdown {
        button,
        value,
        options,
        ticks,
        selected: selected_cell,
    };
    handle.set_selected(selected);
    handle
}
