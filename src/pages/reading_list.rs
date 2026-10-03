//! P4 — Reading list: an ordered to-be-read queue.

use crate::db::{Catalog, ReadingListEntry, SortKey};
use crate::service::LibraryService;
use crate::widgets::book_row::cover_widget;
use crate::widgets::in_app_dialog;
use gtk::prelude::*;
use relm4::prelude::*;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Debug)]
pub enum ReadingListOut {
    OpenBook { book_id: i64 },
    OpenReader { book_id: i64 },
    /// A comic series row — chapters are only ever seen on the series page.
    /// (The Read button stays chapter-precise: it resumes exactly where the
    /// reader left that series.)
    ComicSeries { series_name: String },
}

#[derive(Debug)]
pub enum ReadingListMsg {
    Move { book_id: i64, delta: i64 },
    Remove(i64),
    AddBooks,
    Refresh,
}

pub struct ReadingListModel {
    service: LibraryService,
    entries: Vec<ReadingListEntry>,
}

#[relm4::component(pub)]
impl Component for ReadingListModel {
    type Init = Arc<Catalog>;
    type Input = ReadingListMsg;
    type Output = ReadingListOut;
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
                        set_label: "Reading list",
                        add_css_class: "kalam-page-title",
                        set_halign: gtk::Align::Start,
                    },
                    gtk::Label {
                        #[watch]
                        set_label: &status_line(model.entries.len()),
                        add_css_class: "kalam-page-sub",
                        set_halign: gtk::Align::Start,
                    },
                },

                gtk::Button {
                    set_label: "+ Add books",
                    add_css_class: "kalam-primary-btn",
                    set_valign: gtk::Align::Center,
                    connect_clicked => ReadingListMsg::AddBooks,
                },
            },

            gtk::ScrolledWindow {
                set_vexpand: true,
                set_hexpand: true,
                set_hscrollbar_policy: gtk::PolicyType::Never,

                #[name = "list"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,
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
        let snap = service.reading_list();
        report_errors(&snap.errors);
        let model = ReadingListModel {
            service,
            entries: snap.entries,
        };
        let widgets = view_output!();
        rebuild(&widgets.list, &model.entries, &sender);
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
            ReadingListMsg::Move { book_id, delta } => {
                // The list visibly reorders itself, so success needs no toast.
                crate::notify::report(
                    self.service
                        .catalog()
                        .move_reading_list_entry(book_id, delta),
                    "Could not reorder the reading list",
                );
                self.reload();
            }
            ReadingListMsg::Remove(book_id) => {
                let entry = self.entries.iter().find(|e| e.book.id == book_id);
                let title = entry.map(|e| e.book.title.clone()).unwrap_or_default();
                // The list is collapsed, so a comic row is the whole series —
                // Remove clears every chapter of it, not just the
                // representative.
                let is_comic = entry
                    .map(|e| {
                        matches!(
                            e.book.format,
                            crate::models::BookFormat::Cbz | crate::models::BookFormat::Cbr
                        )
                    })
                    .unwrap_or(false);
                let result = if is_comic {
                    self.service
                        .catalog()
                        .remove_series_from_reading_list(book_id)
                } else {
                    self.service.catalog().remove_from_reading_list(book_id)
                };
                crate::notify::outcome_info(
                    result,
                    "Removed from reading list",
                    &title,
                    "Could not update the reading list",
                );
                self.reload();
            }
            ReadingListMsg::AddBooks => {
                let s = sender.clone();
                open_picker(root, self.service.catalog().clone(), move || {
                    s.input(ReadingListMsg::Refresh)
                });
            }
            ReadingListMsg::Refresh => self.reload(),
        }
        rebuild(&widgets.list, &self.entries, &sender);
        self.update_view(widgets, sender);
    }
}

impl ReadingListModel {
    fn reload(&mut self) {
        let snap = self.service.reading_list();
        report_errors(&snap.errors);
        self.entries = snap.entries;
    }
}

/// Surface read failures instead of rendering them as an empty queue. The
/// service collects them; deciding what the user sees stays with the UI.
fn report_errors(errors: &[String]) {
    for err in errors {
        crate::notify::error("Could not read the reading list", err);
    }
}

fn status_line(n: usize) -> String {
    if n == 0 {
        "Books you plan to read next, in the order you want them.".into()
    } else {
        format!("{n} book{} queued up", if n == 1 { "" } else { "s" })
    }
}

fn rebuild(
    list: &gtk::Box,
    entries: &[ReadingListEntry],
    sender: &ComponentSender<ReadingListModel>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if entries.is_empty() {
        let empty = gtk::Label::new(Some(concat!(
            "Your reading list is empty.\n\n",
            "Add books here from the book page, the float panel, or “+ Add books” above.",
        )));
        empty.add_css_class("kalam-placeholder");
        empty.set_wrap(true);
        empty.set_halign(gtk::Align::Start);
        list.append(&empty);
        return;
    }

    let last = entries.len().saturating_sub(1);
    for (i, entry) in entries.iter().enumerate() {
        list.append(&build_row(i, last, entry, sender));
    }
}

fn build_row(
    index: usize,
    last: usize,
    entry: &ReadingListEntry,
    sender: &ComponentSender<ReadingListModel>,
) -> gtk::Box {
    let book = &entry.book;
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("kalam-list-row");

    let ordinal = gtk::Label::new(Some(&format!("{}", index + 1)));
    ordinal.add_css_class("kalam-list-ordinal");
    ordinal.set_valign(gtk::Align::Center);
    ordinal.set_width_chars(2);
    row.append(&ordinal);

    let cover = cover_widget(book.cover_path.as_deref(), 44, 70);
    cover.set_valign(gtk::Align::Center);
    row.append(&cover);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);

    let title = gtk::Label::new(Some(&book.title));
    title.add_css_class("kalam-card-title");
    title.set_halign(gtk::Align::Start);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    text.append(&title);

    let meta = gtk::Label::new(Some(&format!(
        "{} · {}",
        book.authors_display(),
        if book.progress > 0 {
            format!("{}% read", book.progress)
        } else {
            "not started".to_string()
        }
    )));
    meta.add_css_class("kalam-card-meta");
    meta.set_halign(gtk::Align::Start);
    meta.set_ellipsize(gtk::pango::EllipsizeMode::End);
    meta.set_xalign(0.0);
    text.append(&meta);
    row.append(&text);

    let book_id = book.id;

    let up = gtk::Button::new();
    up.set_child(Some(&crate::icons::symbolic_with_classes(
        "go-up-symbolic",
        16,
        &["kalam-inline-icon"],
    )));
    up.add_css_class("kalam-mini-btn");
    up.set_valign(gtk::Align::Center);
    up.set_sensitive(index > 0);
    {
        let s = sender.clone();
        up.connect_clicked(move |_| s.input(ReadingListMsg::Move { book_id, delta: -1 }));
    }
    row.append(&up);

    let down = gtk::Button::new();
    down.set_child(Some(&crate::icons::symbolic_with_classes(
        "go-down-symbolic",
        16,
        &["kalam-inline-icon"],
    )));
    down.add_css_class("kalam-mini-btn");
    down.set_valign(gtk::Align::Center);
    down.set_sensitive(index < last);
    {
        let s = sender.clone();
        down.connect_clicked(move |_| s.input(ReadingListMsg::Move { book_id, delta: 1 }));
    }
    row.append(&down);

    let read = gtk::Button::with_label("Read");
    read.add_css_class("kalam-mini-btn");
    read.set_valign(gtk::Align::Center);
    {
        let s = sender.clone();
        read.connect_clicked(move |_| {
            s.output(ReadingListOut::OpenReader { book_id }).ok();
        });
    }
    row.append(&read);

    let remove = gtk::Button::new();
    remove.set_child(Some(&crate::icons::symbolic_with_classes(
        "window-close-symbolic",
        16,
        &["kalam-inline-icon"],
    )));
    remove.add_css_class("kalam-mini-btn");
    remove.add_css_class("kalam-mini-btn-danger");
    remove.set_valign(gtk::Align::Center);
    remove.set_tooltip_text(Some("Remove from reading list"));
    {
        let s = sender.clone();
        remove.connect_clicked(move |_| s.input(ReadingListMsg::Remove(book_id)));
    }
    row.append(&remove);

    // Clicking the row opens the book page — or the series page for a comic,
    // the same rule every list obeys.
    let click = gtk::GestureClick::new();
    click.set_button(1);
    let comic_series = if matches!(
        book.format,
        crate::models::BookFormat::Cbz | crate::models::BookFormat::Cbr
    ) {
        book.series.clone()
    } else {
        None
    };
    {
        let s = sender.clone();
        click.connect_released(move |_, _, _, _| {
            if let Some(ser) = comic_series.as_ref() {
                s.output(ReadingListOut::ComicSeries {
                    series_name: ser.clone(),
                })
                .ok();
                return;
            }
            s.output(ReadingListOut::OpenBook { book_id }).ok();
        });
    }
    row.add_controller(click);
    row.set_cursor_from_name(Some("pointer"));

    row
}

/// Library checklist for bulk-queueing books.
///
/// A1: drawn inside the window rather than as a `gtk::Window`, which Sway
/// treated as an ordinary tile. Exit is
/// [`in_app_dialog::DialogExit::OwnButtons`]: ticking a box writes straight to
/// the DB, so there is nothing unsaved and "Done" is just "I am finished
/// looking". Backdrop-click and Esc mean the same thing here.
fn open_picker(anchor: &gtk::Box, catalog: Arc<Catalog>, on_changed: impl Fn() + 'static) {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    // Fit the window, whatever its width (same rule as the shelf picker).
    let win_w = anchor
        .as_ref()
        .root()
        .map(|r| r.upcast_ref::<gtk::Widget>().width())
        .unwrap_or(0);
    let want = if win_w > 0 {
        520.min((win_w - 120).max(320))
    } else {
        520
    };
    root.set_size_request(want, 520);

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search library…"));
    root.append(&search);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .build();
    root.append(&scroll);

    let on_changed = Rc::new(on_changed);
    let fill = {
        let catalog = catalog.clone();
        let list = list.clone();
        let on_changed = on_changed.clone();
        Rc::new(move |query: &str| {
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            let mut books = catalog
                .list_books(SortKey::Title, query)
                .unwrap_or_default();
            // One row per comic series, never per chapter — the owner's rule
            // holds in pickers too.
            catalog.collapse_comic_chapters(&mut books);
            if books.is_empty() {
                let empty = gtk::Label::new(Some("No books match."));
                empty.add_css_class("kalam-muted");
                empty.set_halign(gtk::Align::Start);
                list.append(&empty);
                return;
            }
            for book in books {
                let check = gtk::CheckButton::with_label(&format!(
                    "{} — {}",
                    book.title,
                    book.authors_display()
                ));
                // Ellipsize AND cap the natural width — a centred panel
                // sizes to the content's natural width, and ellipsize alone
                // only caps the minimum (the edge-to-edge bug).
                if let Some(label) = check
                    .first_child()
                    .and_then(|l| l.downcast::<gtk::Label>().ok())
                {
                    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    label.set_max_width_chars(44);
                    label.set_xalign(0.0);
                }
                check.set_hexpand(true);
                check.set_halign(gtk::Align::Fill);
                check.add_css_class("kalam-picker-row");

                // A collapsed comic row is the whole series: ticked when any
                // chapter is queued, tick/untick add/remove every chapter.
                // Regular books skip the peers lookup — the fill runs on
                // every keystroke.
                let peers = if matches!(
                    book.format,
                    crate::models::BookFormat::Cbz | crate::models::BookFormat::Cbr
                ) {
                    catalog
                        .comic_series_peers(book.id)
                        .unwrap_or_else(|_| vec![book.id])
                } else {
                    vec![book.id]
                };
                check.set_active(catalog.any_in_reading_list(&peers).unwrap_or(false));

                let catalog = catalog.clone();
                let on_changed = on_changed.clone();
                let book_id = book.id;
                let book_title = book.title.clone();
                check.connect_toggled(move |c| {
                    if c.is_active() {
                        crate::notify::outcome(
                            catalog.add_series_to_reading_list(book_id),
                            "Added to reading list",
                            &book_title,
                            "Could not update the reading list",
                        );
                    } else {
                        crate::notify::outcome_info(
                            catalog.remove_series_from_reading_list(book_id),
                            "Removed from reading list",
                            &book_title,
                            "Could not update the reading list",
                        );
                    }
                    on_changed();
                });
                list.append(&check);
            }
        })
    };

    fill("");
    {
        let fill = fill.clone();
        search.connect_search_changed(move |e| fill(&e.text()));
    }

    let done = gtk::Button::with_label("Done");
    done.add_css_class("kalam-primary-btn");
    done.set_halign(gtk::Align::End);
    root.append(&done);

    let Some(dialog) = in_app_dialog::present(
        anchor,
        "Add to reading list",
        in_app_dialog::DialogExit::OwnButtons,
        &root,
    ) else {
        crate::notify::error(
            "Could not open the picker",
            "Please try again once the page has finished loading.",
        );
        return;
    };
    done.connect_clicked(move |_| dialog.close());
}
