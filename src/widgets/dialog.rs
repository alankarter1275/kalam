//! Redesign dialog shells layered on `in_app_dialog`.
//!
//! The overlay, scrim, Esc handling, focus trap and widget teardown already
//! exist and are easy to break; this module does not reimplement any of
//! them. It supplies the approved 420/560 panel sizes, body/footer layout,
//! form field, and the common confirmation shape on top of that machinery.

use super::in_app_dialog::{present_redesign, DialogExit, InAppDialog};
use super::pill_button::{boxed_ghost_button, pill_button, PillButtonKind};
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// The two dialog sizes from design-system §18.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialogSize {
    Form,
    List,
}

impl DialogSize {
    pub fn width(self) -> i32 {
        match self {
            Self::Form => 420,
            Self::List => 560,
        }
    }

    pub fn height(self) -> Option<i32> {
        match self {
            Self::Form => None,
            Self::List => Some(660),
        }
    }
}

/// A dialog's content column: optional meta subtitle, a body the caller
/// fills, and the right-aligned footer its actions belong in.
#[derive(Clone)]
pub struct DialogShell {
    root: gtk::Box,
    body: gtk::Box,
    footer: gtk::Box,
    size: DialogSize,
}

impl DialogShell {
    pub fn widget(&self) -> gtk::Box {
        self.root.clone()
    }

    pub fn body(&self) -> gtk::Box {
        self.body.clone()
    }

    pub fn footer(&self) -> gtk::Box {
        self.footer.clone()
    }

    pub fn size(&self) -> DialogSize {
        self.size
    }
}

/// Build the content shell for a redesign dialog. The title is supplied
/// when presenting, because `in_app_dialog` owns the panel header.
pub fn dialog_shell(size: DialogSize, subtitle: Option<&str>) -> DialogShell {
    let root = gtk::Box::new(
        gtk::Orientation::Vertical,
        crate::style::spacing::MD,
    );
    root.set_hexpand(true);
    root.set_vexpand(size == DialogSize::List);
    // The panel's 24px body padding sits outside this shell. Requesting the
    // remaining width is what makes wrapping labels measure against the
    // approved 420/560 size instead of widening the panel to their unwrapped
    // natural width.
    root.set_size_request(size.width() - 48, -1);

    if let Some(subtitle) = subtitle {
        let label = gtk::Label::new(Some(subtitle));
        label.add_css_class("kalam-ui-meta");
        label.set_halign(gtk::Align::Start);
        label.set_xalign(0.0);
        label.set_wrap(true);
        root.append(&label);
    }

    let body = gtk::Box::new(
        gtk::Orientation::Vertical,
        crate::style::spacing::MD,
    );
    body.set_hexpand(true);
    body.set_vexpand(size == DialogSize::List);
    root.append(&body);

    let footer = gtk::Box::new(
        gtk::Orientation::Horizontal,
        crate::style::spacing::SM,
    );
    footer.add_css_class("kalam-ui-dialog-footer");
    footer.set_halign(gtk::Align::End);
    footer.set_valign(gtk::Align::Center);
    root.append(&footer);

    DialogShell {
        root,
        body,
        footer,
        size,
    }
}

/// Present a shell through the existing in-app dialog machinery.
pub fn present_dialog(
    anchor: &impl IsA<gtk::Widget>,
    title: &str,
    exit: DialogExit,
    shell: &DialogShell,
) -> Option<InAppDialog> {
    present_redesign(
        anchor,
        title,
        exit,
        &shell.root,
        shell.size.width(),
        shell.size.height(),
    )
}

/// Which action a confirmation's filled/accent button represents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationKind {
    Primary,
    Destructive,
}

impl ConfirmationKind {
    fn button_kind(self) -> PillButtonKind {
        match self {
            Self::Primary => PillButtonKind::Primary,
            Self::Destructive => PillButtonKind::Danger,
        }
    }
}

/// Present the standard question dialog: section title, 13.5px body copy,
/// boxed-ghost safe choice, and one primary/destructive choice.
///
/// The dialog handle is stored only after `present_redesign` returns, so
/// the buttons close through the shared slot below. `in_app_dialog` removes
/// the panel's children during teardown, which breaks the resulting
/// handle↔button cycle when any exit fires.
pub fn confirmation_dialog(
    anchor: &impl IsA<gtk::Widget>,
    title: &str,
    message: &str,
    cancel_label: &str,
    confirm_label: &str,
    kind: ConfirmationKind,
    on_confirm: impl Fn() + 'static,
) -> Option<InAppDialog> {
    let shell = dialog_shell(DialogSize::Form, None);

    let message_label = gtk::Label::new(Some(message));
    message_label.add_css_class("kalam-ui-dialog-message");
    message_label.set_halign(gtk::Align::Start);
    message_label.set_xalign(0.0);
    message_label.set_wrap(true);
    shell.body.append(&message_label);

    let dialog_slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));

    let slot_for_cancel = dialog_slot.clone();
    shell.footer.append(&boxed_ghost_button(cancel_label, move || {
        close_from_slot(&slot_for_cancel);
    }));

    let slot_for_confirm = dialog_slot.clone();
    shell.footer.append(&pill_button(
        confirm_label,
        kind.button_kind(),
        move || {
            close_from_slot(&slot_for_confirm);
            on_confirm();
        },
    ));

    let dialog = present_dialog(anchor, title, DialogExit::OwnButtons, &shell)?;
    *dialog_slot.borrow_mut() = Some(dialog.clone());
    Some(dialog)
}

fn close_from_slot(slot: &Rc<RefCell<Option<InAppDialog>>>) {
    let dialog = slot.borrow().clone();
    if let Some(dialog) = dialog {
        dialog.close();
    }
}

/// A labelled 44px text field for redesign form dialogs (§31).
#[derive(Clone)]
pub struct TextField {
    root: gtk::Box,
    entry: gtk::Entry,
}

impl TextField {
    pub fn widget(&self) -> gtk::Box {
        self.root.clone()
    }

    pub fn entry(&self) -> gtk::Entry {
        self.entry.clone()
    }

    pub fn value(&self) -> String {
        self.entry.text().to_string()
    }

    pub fn set_value(&self, value: &str) {
        self.entry.set_text(value);
    }
}

/// Build a §31 field: Label-style caption above a 44px entry.
pub fn text_field(label: &str, placeholder: &str, initial: &str) -> TextField {
    let root = gtk::Box::new(
        gtk::Orientation::Vertical,
        crate::style::spacing::XS,
    );

    let caption = gtk::Label::new(Some(&label.to_ascii_uppercase()));
    caption.add_css_class("kalam-ui-field-label");
    caption.set_halign(gtk::Align::Start);
    caption.set_xalign(0.0);
    root.append(&caption);

    let entry = gtk::Entry::new();
    entry.add_css_class("kalam-ui-field");
    entry.set_placeholder_text(Some(placeholder));
    entry.set_text(initial);
    entry.set_size_request(-1, crate::style::size::TOUCH_TARGET);
    entry.set_hexpand(true);
    root.append(&entry);

    TextField { root, entry }
}

#[cfg(test)]
mod tests {
    use super::{ConfirmationKind, DialogSize};
    use crate::widgets::pill_button::PillButtonKind;

    #[test]
    fn dialog_sizes_match_section_18() {
        assert_eq!(DialogSize::Form.width(), 420);
        assert_eq!(DialogSize::Form.height(), None);
        assert_eq!(DialogSize::List.width(), 560);
        assert_eq!(DialogSize::List.height(), Some(660));
    }

    #[test]
    fn confirmation_kinds_map_to_the_two_filled_choices() {
        assert_eq!(
            ConfirmationKind::Primary.button_kind(),
            PillButtonKind::Primary
        );
        assert_eq!(
            ConfirmationKind::Destructive.button_kind(),
            PillButtonKind::Danger
        );
    }
}
