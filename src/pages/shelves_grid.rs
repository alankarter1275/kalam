//! P4 — Shelves: two-column grid of manual and smart collections.

use crate::db::{Catalog, Shelf, ShelfKind};
use crate::pages::shelf_editor::{open_shelf_editor, ShelfEditorMode};
use crate::service::LibraryService;
use crate::widgets::in_app_dialog;
use gtk::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

#[derive(Debug)]
pub enum ShelvesOut {
    OpenShelf { shelf_id: i64 },
}

#[derive(Debug)]
pub enum ShelvesMsg {
    NewManual,
    NewSmart,
    Edit(i64),
    Delete(i64),
    Refresh,
}

pub struct ShelvesGridModel {
    service: LibraryService,
    shelves: Vec<Shelf>,
}

#[relm4::component(pub)]
impl Component for ShelvesGridModel {
    type Init = Arc<Catalog>;
    type Input = ShelvesMsg;
    type Output = ShelvesOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,
            set_hexpand: true,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 10,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_hexpand: true,

                    gtk::Label {
                        set_label: "Shelves",
                        add_css_class: "kalam-page-title",
                        set_halign: gtk::Align::Start,
                    },
                    gtk::Label {
                        #[watch]
                        set_label: &summary_line(&model.shelves),
                        add_css_class: "kalam-page-sub",
                        set_halign: gtk::Align::Start,
                    },
                },

                gtk::Button {
                    set_label: "+ Shelf",
                    add_css_class: "kalam-secondary-btn",
                    set_valign: gtk::Align::Center,
                    set_tooltip_text: Some("Hand-picked collection"),
                    connect_clicked => ShelvesMsg::NewManual,
                },
                gtk::Button {
                    set_label: "+ Smart shelf",
                    add_css_class: "kalam-primary-btn",
                    set_valign: gtk::Align::Center,
                    set_tooltip_text: Some("Collection defined by rules"),
                    connect_clicked => ShelvesMsg::NewSmart,
                },
            },

            gtk::ScrolledWindow {
                set_vexpand: true,
                set_hexpand: true,
                set_hscrollbar_policy: gtk::PolicyType::Never,

                #[name = "grid_host"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,
                },
            },
        }
    }

    fn init(
        catalog: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let service = LibraryService::new(catalog);
        let snap = service.shelves();
        report_errors(&snap.errors);
        let model = ShelvesGridModel {
            service,
            shelves: snap.shelves,
        };
        let widgets = view_output!();
        rebuild_grid(&widgets.grid_host, &model.shelves, &sender);
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            ShelvesMsg::NewManual | ShelvesMsg::NewSmart => {
                let kind = match msg {
                    ShelvesMsg::NewSmart => ShelfKind::Smart,
                    _ => ShelfKind::Manual,
                };
                let s = sender.clone();
                open_shelf_editor(
                    root,
                    self.service.catalog().clone(),
                    ShelfEditorMode::Create(kind),
                    move || {
                        // Non-modal dialog: quiet no-op if this page is gone.
                        let _ = s.input_sender().send(ShelvesMsg::Refresh);
                    },
                );
            }
            ShelvesMsg::Edit(shelf_id) => {
                let s = sender.clone();
                open_shelf_editor(
                    root,
                    self.service.catalog().clone(),
                    ShelfEditorMode::Edit { shelf_id },
                    move || {
                        // Non-modal dialog: quiet no-op if this page is gone.
                        let _ = s.input_sender().send(ShelvesMsg::Refresh);
                    },
                );
            }
            ShelvesMsg::Delete(shelf_id) => {
                // Grab the name before the row goes, so the toast can say
                // which shelf was deleted rather than just "a shelf".
                let name = self
                    .shelves
                    .iter()
                    .find(|s| s.id == shelf_id)
                    .map(|s| s.name.clone())
                    .unwrap_or_default();
                // The closure below moves `name` into the toast, so the
                // prompt needs its own copy.
                let title_name = name.clone();
                confirm_delete(root, &title_name, {
                    let catalog = self.service.catalog().clone();
                    let s = sender.clone();
                    move || {
                        crate::notify::outcome(
                            catalog.delete_shelf(shelf_id),
                            "Shelf deleted",
                            &name,
                            "Could not delete the shelf",
                        );
                        // The confirm popover can outlive this page; send
                        // through the raw channel so a closed page is a
                        // quiet no-op rather than a panic.
                        let _ = s.input_sender().send(ShelvesMsg::Refresh);
                    }
                });
            }
            ShelvesMsg::Refresh => {
                let snap = self.service.shelves();
                report_errors(&snap.errors);
                self.shelves = snap.shelves;
                rebuild_grid(&widgets.grid_host, &self.shelves, &sender);
            }
        }
        self.update_view(widgets, sender);
    }
}

fn summary_line(shelves: &[Shelf]) -> String {
    if shelves.is_empty() {
        return "Smart filters and manual collections — like Calibre virtual libraries.".into();
    }
    let smart = shelves
        .iter()
        .filter(|s| s.kind == ShelfKind::Smart)
        .count();
    let manual = shelves.len() - smart;
    format!(
        "{} shelf{} · {manual} manual · {smart} smart",
        shelves.len(),
        if shelves.len() == 1 { "" } else { "es" }
    )
}

/// Surface read failures instead of rendering them as an empty shelf grid.
/// The service collects them; deciding what the user sees stays with the UI.
fn report_errors(errors: &[String]) {
    for err in errors {
        crate::notify::error("Could not read your shelves", err);
    }
}

fn rebuild_grid(host: &gtk::Box, shelves: &[Shelf], sender: &ComponentSender<ShelvesGridModel>) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }

    if shelves.is_empty() {
        let empty = gtk::Label::new(Some(concat!(
            "No shelves yet.\n\n",
            "“+ Shelf” makes a hand-picked collection you add books to from the book page.\n",
            "“+ Smart shelf” builds itself from rules — tag, author, format, progress, and more."
        )));
        empty.add_css_class("kalam-placeholder");
        empty.set_wrap(true);
        empty.set_halign(gtk::Align::Start);
        host.append(&empty);
        return;
    }

    // Two columns, as locked in the P0 design notes.
    let grid = gtk::Grid::new();
    grid.set_column_spacing(12);
    grid.set_row_spacing(12);
    grid.set_column_homogeneous(true);
    grid.set_hexpand(true);

    for (i, shelf) in shelves.iter().enumerate() {
        let card = build_shelf_card(shelf, sender);
        let col = (i % 2) as i32;
        let row = (i / 2) as i32;
        grid.attach(&card, col, row, 1, 1);
    }

    host.append(&grid);
}

fn build_shelf_card(shelf: &Shelf, sender: &ComponentSender<ShelvesGridModel>) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
    card.add_css_class("kalam-card");
    card.add_css_class("kalam-shelf-card");
    card.set_hexpand(true);

    // Header: name + kind badge
    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let name = gtk::Label::new(Some(&shelf.name));
    name.add_css_class("kalam-card-title");
    name.set_halign(gtk::Align::Start);
    name.set_hexpand(true);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    header.append(&name);

    let badge = gtk::Label::new(Some(shelf.kind.label()));
    badge.add_css_class("kalam-card-badge");
    badge.add_css_class(match shelf.kind {
        ShelfKind::Smart => "kalam-badge-smart",
        ShelfKind::Manual => "kalam-badge-manual",
    });
    badge.set_valign(gtk::Align::Center);
    header.append(&badge);
    card.append(&header);

    // Count
    let count = gtk::Label::new(Some(&format!(
        "{} book{}",
        shelf.book_count,
        if shelf.book_count == 1 { "" } else { "s" }
    )));
    count.add_css_class("kalam-progress");
    count.set_halign(gtk::Align::Start);
    card.append(&count);

    // Rule / description summary
    let summary = gtk::Label::new(Some(&shelf.summary()));
    summary.add_css_class("kalam-card-meta");
    summary.set_halign(gtk::Align::Start);
    summary.set_xalign(0.0);
    summary.set_wrap(true);
    summary.set_lines(2);
    summary.set_ellipsize(gtk::pango::EllipsizeMode::End);
    card.append(&summary);

    // Actions
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    actions.set_halign(gtk::Align::End);
    actions.set_margin_top(4);

    let edit = gtk::Button::with_label("Edit");
    edit.add_css_class("kalam-mini-btn");
    let id = shelf.id;
    let s = sender.clone();
    edit.connect_clicked(move |_| s.input(ShelvesMsg::Edit(id)));
    actions.append(&edit);

    let delete = gtk::Button::with_label("Delete");
    delete.add_css_class("kalam-mini-btn");
    delete.add_css_class("kalam-mini-btn-danger");
    let s = sender.clone();
    delete.connect_clicked(move |_| s.input(ShelvesMsg::Delete(id)));
    actions.append(&delete);
    card.append(&actions);

    // Clicking the card body opens the shelf.
    let click = gtk::GestureClick::new();
    click.set_button(1);
    let s = sender.clone();
    click.connect_released(move |_, _, _, _| {
        s.output(ShelvesOut::OpenShelf { shelf_id: id }).ok();
    });
    card.add_controller(click);
    card.set_cursor_from_name(Some("pointer"));
    card.set_tooltip_text(Some(&format!("Open “{}”", shelf.name)));

    card
}

/// Confirm deleting a shelf. Destructive enough to deserve a prompt, even
/// though the books themselves are never touched.
///
/// A1: drawn **inside** the window. As a `gtk::Window` this was a real
/// top-level that Sway could tile beside the app or move to another
/// workspace — for a modal confirmation that is simply wrong.
///
/// Exit is [`in_app_dialog::DialogExit::OwnButtons`]: two named outcomes, so no ✕
/// (a third, vaguer exit next to "Cancel" and "Delete" only adds doubt).
/// Clicking the backdrop cancels, which is safe here because nothing is lost.
fn confirm_delete(anchor: &gtk::Box, shelf_name: &str, on_confirm: impl Fn() + 'static) {
    let body = gtk::Box::new(gtk::Orientation::Vertical, 14);
    body.set_size_request(360, -1);

    let text = gtk::Label::new(Some(&format!(
        "Delete “{shelf_name}”?\n\nThe books stay in your library — only the collection is removed."
    )));
    text.set_wrap(true);
    text.set_halign(gtk::Align::Start);
    text.set_xalign(0.0);
    body.append(&text);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    actions.set_halign(gtk::Align::End);
    let cancel = gtk::Button::with_label("Cancel");
    cancel.add_css_class("kalam-secondary-btn");
    let confirm = gtk::Button::with_label("Delete");
    confirm.add_css_class("kalam-primary-btn");
    actions.append(&cancel);
    actions.append(&confirm);
    body.append(&actions);

    let Some(dialog) = in_app_dialog::present(
        anchor,
        "Delete shelf",
        in_app_dialog::DialogExit::OwnButtons,
        &body,
    ) else {
        // No overlay found means the page is not in the window yet, which
        // should not happen from a button press. Do nothing rather than
        // silently deleting without asking.
        crate::notify::error(
            "Could not show the confirmation",
            "Please try again once the page has finished loading.",
        );
        return;
    };

    {
        let dialog = dialog.clone();
        cancel.connect_clicked(move |_| dialog.close());
    }
    {
        let dialog = dialog.clone();
        confirm.connect_clicked(move |_| {
            on_confirm();
            dialog.close();
        });
    }
}
