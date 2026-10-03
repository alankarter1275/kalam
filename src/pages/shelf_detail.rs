//! P4 — one shelf: its books, plus membership editing for manual shelves.

use crate::db::{Catalog, Shelf, ShelfKind, SortKey};
use crate::models::Book;
use crate::pages::shelf_editor::{open_shelf_editor, ShelfEditorMode};
use crate::service::LibraryService;
use crate::widgets::book_row::build_book_grid;
use crate::widgets::in_app_dialog;
use gtk::prelude::*;
use relm4::prelude::*;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Debug)]
pub enum ShelfDetailOut {
    OpenBook { book_id: i64 },
    OpenBookDialog { book_id: i64 },
    /// A comic series card — chapters are only ever seen on the series page.
    ComicSeries { series_name: String },
}

#[derive(Debug)]
pub enum ShelfDetailMsg {
    SearchChanged(String),
    SortChanged(SortKey),
    EditShelf,
    AddBooks,
    Refresh,
}

pub struct ShelfDetailModel {
    service: LibraryService,
    shelf: Option<Shelf>,
    books: Vec<Book>,
    query: String,
    sort: SortKey,
}

#[relm4::component(pub)]
impl Component for ShelfDetailModel {
    type Init = (Arc<Catalog>, i64);
    type Input = ShelfDetailMsg;
    type Output = ShelfDetailOut;
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

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,

                        #[name = "title"]
                        gtk::Label {
                            add_css_class: "kalam-page-title",
                            set_halign: gtk::Align::Start,
                        },
                        #[name = "kind_badge"]
                        gtk::Label {
                            add_css_class: "kalam-card-badge",
                            set_valign: gtk::Align::Center,
                        },
                    },
                    #[name = "subtitle"]
                    gtk::Label {
                        add_css_class: "kalam-page-sub",
                        set_halign: gtk::Align::Start,
                        set_xalign: 0.0,
                        set_wrap: true,
                    },
                },

                #[name = "add_btn"]
                gtk::Button {
                    set_label: "+ Add books",
                    add_css_class: "kalam-secondary-btn",
                    set_valign: gtk::Align::Center,
                    connect_clicked => ShelfDetailMsg::AddBooks,
                },
                gtk::Button {
                    set_label: "Edit",
                    add_css_class: "kalam-secondary-btn",
                    set_valign: gtk::Align::Center,
                    connect_clicked => ShelfDetailMsg::EditShelf,
                },
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                gtk::SearchEntry {
                    set_hexpand: true,
                    set_placeholder_text: Some("Search this shelf…"),
                    connect_search_changed[sender] => move |e| {
                        sender.input(ShelfDetailMsg::SearchChanged(e.text().to_string()));
                    },
                },

                #[name = "sort_box"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 4,
                },
            },

            #[name = "status"]
            gtk::Label {
                add_css_class: "kalam-muted",
                set_halign: gtk::Align::Start,
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
        (catalog, shelf_id): Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let sort = SortKey::Added;
        let service = LibraryService::new(catalog);
        let snap = service.shelf_detail(shelf_id, sort, "");
        report_errors(&snap.errors);

        let model = ShelfDetailModel {
            service,
            shelf: snap.shelf,
            books: snap.books,
            query: String::new(),
            sort,
        };
        let widgets = view_output!();

        for key in SortKey::ALL {
            let label = if *key == SortKey::Added && model.is_manual() {
                // Manual shelves keep their hand-sorted order under "Added".
                "Shelf order"
            } else {
                key.label()
            };
            let btn = gtk::ToggleButton::with_label(label);
            btn.add_css_class("kalam-secondary-btn");
            if *key == SortKey::Added {
                btn.set_active(true);
            }
            let k = *key;
            let s = sender.clone();
            btn.connect_toggled(move |b| {
                if b.is_active() {
                    s.input(ShelfDetailMsg::SortChanged(k));
                }
            });
            widgets.sort_box.append(&btn);
        }
        group_toggles(&widgets.sort_box);

        model.refresh_header(&widgets);
        model.rebuild(&widgets, &sender);
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
            ShelfDetailMsg::SearchChanged(q) => {
                self.query = q;
                self.reload();
            }
            ShelfDetailMsg::SortChanged(sort) => {
                self.sort = sort;
                self.reload();
            }
            ShelfDetailMsg::EditShelf => {
                if let Some(shelf) = &self.shelf {
                    let s = sender.clone();
                    open_shelf_editor(
                        root,
                        self.service.catalog().clone(),
                        ShelfEditorMode::Edit { shelf_id: shelf.id },
                        move || {
                            // Non-modal dialog: the page can be closed before
                            // Save is clicked. Quiet no-op instead of panic.
                            let _ = s.input_sender().send(ShelfDetailMsg::Refresh);
                        },
                    );
                }
            }
            ShelfDetailMsg::AddBooks => {
                if let Some(shelf) = &self.shelf {
                    if shelf.kind == ShelfKind::Manual {
                        let s = sender.clone();
                        open_book_picker(
                            root,
                            self.service.catalog().clone(),
                            shelf.id,
                            move || {
                                // Non-modal dialog: quiet no-op if the page
                                // is gone by the time it closes.
                                let _ = s.input_sender().send(ShelfDetailMsg::Refresh);
                            },
                        );
                    }
                }
            }
            ShelfDetailMsg::Refresh => {
                self.reload();
            }
        }

        self.refresh_header(widgets);
        self.rebuild(widgets, &sender);
        self.update_view(widgets, sender);
    }
}

/// Surface read failures. Without this a database problem looked like an
/// empty shelf.
fn report_errors(errors: &[String]) {
    for err in errors {
        crate::notify::error("Could not read this shelf", err);
    }
}

impl ShelfDetailModel {
    fn is_manual(&self) -> bool {
        self.shelf
            .as_ref()
            .map(|s| s.kind == ShelfKind::Manual)
            .unwrap_or(false)
    }

    /// Re-read the shelf **and** its books together. The shelf row itself can
    /// change under us (rename, cover, kind), so refreshing only the books
    /// left the header stale.
    fn reload(&mut self) {
        let Some(id) = self.shelf.as_ref().map(|s| s.id) else {
            self.books.clear();
            return;
        };
        let snap = self.service.shelf_detail(id, self.sort, &self.query);
        report_errors(&snap.errors);
        self.shelf = snap.shelf;
        self.books = snap.books;
    }

    fn refresh_header(&self, widgets: &ShelfDetailModelWidgets) {
        match &self.shelf {
            Some(shelf) => {
                widgets.title.set_label(&shelf.name);
                widgets.kind_badge.set_label(shelf.kind.label());
                widgets.kind_badge.set_visible(true);
                widgets.subtitle.set_label(&shelf.summary());
                // Only manual shelves have membership to edit.
                widgets.add_btn.set_visible(shelf.kind == ShelfKind::Manual);

                let n = self.books.len();
                widgets.status.set_label(&format!(
                    "{n} book{} · click cover for float · Ctrl+click for full page",
                    if n == 1 { "" } else { "s" }
                ));
            }
            None => {
                widgets.title.set_label("Shelf not found");
                widgets.kind_badge.set_visible(false);
                widgets.subtitle.set_label("This shelf was deleted.");
                widgets.add_btn.set_visible(false);
                widgets.status.set_label("");
            }
        }
    }

    fn rebuild(&self, widgets: &ShelfDetailModelWidgets, sender: &ComponentSender<Self>) {
        let list = &widgets.list;
        while let Some(child) = list.first_child() {
            list.remove(&child);
        }

        if self.shelf.is_none() {
            return;
        }

        if self.books.is_empty() {
            let empty = gtk::Label::new(Some(if !self.query.trim().is_empty() {
                "Nothing on this shelf matches your search."
            } else if self.is_manual() {
                "This shelf is empty — use “+ Add books” to put something on it."
            } else {
                "No books match these rules yet. Use “Edit” to adjust them."
            }));
            empty.add_css_class("kalam-placeholder");
            empty.set_wrap(true);
            empty.set_halign(gtk::Align::Start);
            list.append(&empty);
            return;
        }

        let s1 = sender.clone();
        let s2 = sender.clone();
        // Comic cards open the series page, not the chapter's book page —
        // the all_books pattern.
        let comics_map: std::collections::HashMap<i64, Option<String>> = self
            .books
            .iter()
            .filter(|b| {
                matches!(
                    b.format,
                    crate::models::BookFormat::Cbz | crate::models::BookFormat::Cbr
                )
            })
            .map(|b| (b.id, b.series.clone()))
            .collect();
        let cm = std::rc::Rc::new(comics_map);
        let grid = build_book_grid(
            &self.books,
            move |id| {
                if let Some(Some(ser)) = cm.get(&id) {
                    s1.output(ShelfDetailOut::ComicSeries {
                        series_name: ser.clone(),
                    })
                    .ok();
                    return;
                }
                s1.output(ShelfDetailOut::OpenBook { book_id: id }).ok();
            },
            move |id| {
                s2.output(ShelfDetailOut::OpenBookDialog { book_id: id })
                    .ok();
            },
        );
        list.append(&grid);

        // Manual shelves get a per-book remove/reorder strip under the grid.
        if self.is_manual() && self.query.trim().is_empty() {
            let manage = gtk::Expander::new(Some("Manage shelf order"));
            manage.add_css_class("kalam-manage-expander");
            let rows = gtk::Box::new(gtk::Orientation::Vertical, 4);
            rows.set_margin_top(8);

            let shelf_id = self.shelf.as_ref().map(|s| s.id).unwrap_or(0);
            for book in &self.books {
                rows.append(&self.manage_row(shelf_id, book, sender));
            }
            manage.set_child(Some(&rows));
            list.append(&manage);
        }
    }

    fn manage_row(&self, shelf_id: i64, book: &Book, sender: &ComponentSender<Self>) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("kalam-manage-row");

        let title = gtk::Label::new(Some(&book.title));
        title.set_halign(gtk::Align::Start);
        title.set_hexpand(true);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.append(&title);

        let book_id = book.id;
        let sortable = self.sort == SortKey::Added;

        let up = gtk::Button::new();
        up.set_child(Some(&crate::icons::symbolic_with_classes(
            "go-up-symbolic",
            16,
            &["kalam-inline-icon"],
        )));
        up.add_css_class("kalam-mini-btn");
        up.set_sensitive(sortable);
        up.set_tooltip_text(Some(if sortable {
            "Move up"
        } else {
            "Switch to “Shelf order” to reorder"
        }));
        {
            let catalog = self.service.catalog().clone();
            let s = sender.clone();
            up.connect_clicked(move |_| {
                // Reordering is visible in the list itself, so only a failure
                // needs saying — a toast per arrow click would be noise.
                crate::notify::report(
                    catalog.move_shelf_book(shelf_id, book_id, -1),
                    "Could not reorder the shelf",
                );
                s.input(ShelfDetailMsg::Refresh);
            });
        }
        row.append(&up);

        let down = gtk::Button::new();
        down.set_child(Some(&crate::icons::symbolic_with_classes(
            "go-down-symbolic",
            16,
            &["kalam-inline-icon"],
        )));
        down.add_css_class("kalam-mini-btn");
        down.set_sensitive(sortable);
        down.set_tooltip_text(Some("Move down"));
        {
            let catalog = self.service.catalog().clone();
            let s = sender.clone();
            down.connect_clicked(move |_| {
                crate::notify::report(
                    catalog.move_shelf_book(shelf_id, book_id, 1),
                    "Could not reorder the shelf",
                );
                s.input(ShelfDetailMsg::Refresh);
            });
        }
        row.append(&down);

        let remove = gtk::Button::with_label("Remove");
        remove.add_css_class("kalam-mini-btn");
        remove.add_css_class("kalam-mini-btn-danger");
        // A comic row is the whole series (the list is collapsed), so Remove
        // clears every chapter of it from the shelf — not just the
        // representative chapter the row happens to carry.
        let is_comic = matches!(
            book.format,
            crate::models::BookFormat::Cbz | crate::models::BookFormat::Cbr
        );
        {
            let catalog = self.service.catalog().clone();
            let s = sender.clone();
            let book_title = book.title.clone();
            remove.connect_clicked(move |_| {
                let result = if is_comic {
                    catalog.remove_series_from_shelf(shelf_id, book_id)
                } else {
                    catalog.remove_book_from_shelf(shelf_id, book_id)
                };
                crate::notify::outcome_info(
                    result,
                    "Removed from shelf",
                    &book_title,
                    "Could not remove from the shelf",
                );
                s.input(ShelfDetailMsg::Refresh);
            });
        }
        row.append(&remove);

        row
    }
}

fn group_toggles(box_: &gtk::Box) {
    let mut leader: Option<gtk::ToggleButton> = None;
    let mut child = box_.first_child();
    while let Some(w) = child {
        let next = w.next_sibling();
        if let Ok(btn) = w.downcast::<gtk::ToggleButton>() {
            match &leader {
                Some(l) => btn.set_group(Some(l)),
                None => leader = Some(btn),
            }
        }
        child = next;
    }
}

/// Checklist of every book in the library, ticked for the ones already on this
/// manual shelf. Toggling writes straight through to the DB.
///
/// A1: drawn inside the window rather than as a `gtk::Window`. Exit is
/// [`in_app_dialog::DialogExit::OwnButtons`] — each tick writes straight to
/// the DB, so "Done", Esc and a backdrop click all mean the same thing.
fn open_book_picker(
    anchor: &gtk::Box,
    catalog: Arc<Catalog>,
    shelf_id: i64,
    on_changed: impl Fn() + 'static,
) {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    // Fit the window, whatever its width: the panel sits inside the app with
    // its borders visible, or it means nothing. 520 is the comfortable size;
    // a narrower window gets 120 px less than itself (host margins + body
    // padding + border), floored so it never collapses.
    let win_w = anchor
        .root()
        .map(|r| r.upcast_ref::<gtk::Widget>().width())
        .unwrap_or(0);
    let want = if win_w > 0 {
        520.min((win_w - 120).max(320))
    } else {
        520
    };
    root.set_size_request(want, 520);
    // The width is verified by data, not screenshots: the photo uploads do
    // not reach the agent's workspace, but the timing log does. The window's
    // width goes in now; the picker's final allocated width lands after the
    // first layout pass, one idle dispatch later.
    if win_w > 0 {
        crate::timing::note("picker_window", win_w as usize);
    }

    let hint = gtk::Label::new(Some("Tick the books that belong on this shelf."));
    hint.add_css_class("kalam-muted");
    hint.set_halign(gtk::Align::Start);
    root.append(&hint);

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
            // The owner's rule holds in pickers too: one row per comic
            // series, never one per chapter. A 700-chapter import used to
            // flood this list and its wide chapter titles forced the panel
            // past the dialog's sides.
            catalog.collapse_comic_chapters(&mut books);
            let on_shelf: std::collections::HashSet<i64> = catalog
                .get_shelf(shelf_id)
                .ok()
                .flatten()
                .and_then(|s| catalog.shelf_books(&s, SortKey::Title, "").ok())
                .unwrap_or_default()
                .iter()
                .map(|b| b.id)
                .collect();

            if books.is_empty() {
                let empty = gtk::Label::new(Some("No books match."));
                empty.add_css_class("kalam-muted");
                empty.set_halign(gtk::Align::Start);
                list.append(&empty);
                return;
            }

            for book in books {
                // The label is ours, not the button's internal one. Reaching
                // into `with_label`'s child to cap it silently did nothing —
                // the owner's log measured the picker at 1906 px on a 1350 px
                // window (2026-10-03). A widget we built ourselves is one we
                // can constrain.
                let check = gtk::CheckButton::new();
                let label = gtk::Label::new(Some(&format!(
                    "{} — {}",
                    book.title,
                    book.authors_display()
                )));
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                label.set_max_width_chars(44);
                label.set_xalign(0.0);
                label.set_hexpand(true);
                check.set_child(Some(&label));
                check.set_hexpand(true);
                check.set_halign(gtk::Align::Fill);
                check.add_css_class("kalam-picker-row");

                // A collapsed comic row is the whole series: ticked when any
                // chapter is on the shelf, tick/untick add/remove every
                // chapter. Regular books skip the lookup — the fill runs on
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
                check.set_active(peers.iter().any(|p| on_shelf.contains(p)));

                let catalog = catalog.clone();
                let on_changed = on_changed.clone();
                let book_id = book.id;
                let book_title = book.title.clone();
                check.connect_toggled(move |c| {
                    if c.is_active() {
                        crate::notify::outcome(
                            catalog.add_series_to_shelf(shelf_id, book_id),
                            "Added to shelf",
                            &book_title,
                            "Could not add to the shelf",
                        );
                    } else {
                        crate::notify::outcome_info(
                            catalog.remove_series_from_shelf(shelf_id, book_id),
                            "Removed from shelf",
                            &book_title,
                            "Could not remove from the shelf",
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
        "Add books to shelf",
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

    // Allocated only after the first layout pass; idle (priority 200) runs
    // after resize (110), so the width is real by the time this fires. The
    // natural width is measured too — if a cap ever fails again, the log
    // will say which one (natural still huge = the rows; natural small but
    // width huge = the allocation).
    {
        let r = root.clone();
        gtk::glib::idle_add_local_once(move || {
            let (_, nat, _, _) = r.measure(gtk::Orientation::Horizontal, -1);
            crate::timing::note("picker_natural", nat as usize);
            crate::timing::note("picker_width", r.width() as usize);
        });
    }
}
