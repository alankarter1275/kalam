use crate::db::Catalog;
use crate::pages::all_books::{import_summary, spawn_import, ImportTally};
use crate::service::LibraryService;
use crate::widgets::book_row::{build_book_card, CARD_H, CARD_W, COVER_H, COVER_W};
use gtk::prelude::*;
use relm4::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;

/// Navigation requests from Home. Variants are named for their destination
/// rather than sharing an `Open` prefix (clippy::enum_variant_names) -- the
/// same convention `LibraryOut` uses.
#[derive(Debug)]
pub enum HomeOut {
    Book {
        book_id: i64,
    },
    BookDialog {
        book_id: i64,
    },
    /// My Library → All books (the full searchable/sortable grid).
    AllBooks,
}

#[derive(Debug)]
pub enum HomeMsg {
    AddBooks,
    AllBooks,
    FilesChosen(Vec<PathBuf>),
    /// One file finished importing: 1-based index, total, and its title.
    ImportStep {
        done: usize,
        total: usize,
        title: String,
    },
    /// The whole import finished.
    ImportFinished(ImportTally),
}

pub struct HomePageModel {
    service: LibraryService,
    importing: bool,
    status: String,
}

#[relm4::component(pub)]
impl Component for HomePageModel {
    type Init = Arc<Catalog>;
    type Input = HomeMsg;
    type Output = HomeOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 16,
            set_hexpand: true,
            set_vexpand: false,
            set_margin_all: 0,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,

                gtk::Label {
                    set_label: "Home",
                    add_css_class: "kalam-page-title",
                    set_halign: gtk::Align::Start,
                    set_hexpand: true,
                },

                // Sits left of "+ Add books": browsing the whole library is
                // the more common intent, importing the rarer one, but the
                // primary-styled button stays the import action.
                #[name = "all_books_btn"]
                gtk::Button {
                    set_label: "All books",
                    add_css_class: "kalam-secondary-btn",
                    set_tooltip_text: Some("Browse, search and sort every book in your library"),
                    connect_clicked => HomeMsg::AllBooks,
                },

                #[name = "add_btn"]
                gtk::Button {
                    #[watch]
                    set_label: if model.importing { "Importing…" } else { "+ Add books" },
                    add_css_class: "kalam-primary-btn",
                    #[watch]
                    set_sensitive: !model.importing,
                    connect_clicked => HomeMsg::AddBooks,
                },
            },

            gtk::Label {
                set_label: "Continue reading and recently added · click = float · Ctrl+click = full page",
                add_css_class: "kalam-page-sub",
                set_halign: gtk::Align::Start,
            },

            #[name = "status_label"]
            gtk::Label {
                #[watch]
                set_label: &model.status,
                #[watch]
                set_visible: !model.status.is_empty(),
                add_css_class: "kalam-muted",
                set_halign: gtk::Align::Start,
            },

            #[name = "counts_host"]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,
                set_homogeneous: true,
                set_margin_bottom: 4,
            },

            gtk::Label {
                set_label: "CONTINUE",
                add_css_class: "kalam-section-label",
                set_halign: gtk::Align::Start,
            },

            #[name = "continue_host"]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 16,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Start,
                set_hexpand: true,
                set_vexpand: false,
                set_margin_bottom: 8,
            },

            #[name = "tbr_label"]
            gtk::Label {
                set_label: "UP NEXT",
                add_css_class: "kalam-section-label",
                set_halign: gtk::Align::Start,
            },

            #[name = "tbr_host"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 6,
                set_margin_bottom: 8,
            },

            gtk::Label {
                set_label: "RECENTLY ADDED",
                add_css_class: "kalam-section-label",
                set_halign: gtk::Align::Start,
            },

            #[name = "recent_host"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_halign: gtk::Align::Fill,
                set_hexpand: true,
                set_vexpand: false,
                set_spacing: 0,
            },
        }
    }

    fn init(
        catalog: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = HomePageModel {
            service: LibraryService::new(catalog),
            importing: false,
            status: String::new(),
        };
        let widgets = view_output!();

        rebuild(&widgets, &model.service, &sender);

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
            HomeMsg::ImportStep { done, total, title } => {
                self.status = if title.is_empty() {
                    format!("Importing {done} of {total}…")
                } else {
                    format!("Importing {done} of {total} — {title}")
                };
            }
            HomeMsg::ImportFinished(tally) => {
                self.importing = false;

                if tally.imported > 0 {
                    crate::notify::success(
                        &format!(
                            "{} book{} imported",
                            tally.imported,
                            if tally.imported == 1 { "" } else { "s" }
                        ),
                        &tally.last_title,
                    );
                }

                self.status = import_summary(&tally);

                // New books are in the catalog now; refresh this page so the
                // counts / continue / recently-added cards reflect them.
                rebuild(widgets, &self.service, &sender);
            }
            HomeMsg::AllBooks => {
                sender.output(HomeOut::AllBooks).ok();
            }
            HomeMsg::AddBooks => {
                let dialog = gtk::FileDialog::builder()
                    .title("Import EPUB books")
                    .modal(true)
                    .build();

                let filter = gtk::FileFilter::new();
                filter.set_name(Some("Books, Comics, & PDFs (*.epub, *.cbz, *.cbr, *.pdf)"));
                filter.add_suffix("epub");
                filter.add_suffix("cbz");
                filter.add_suffix("cbr");
                filter.add_suffix("pdf");
                filter.add_suffix("pdf");
                filter.add_mime_type("application/epub+zip");
                filter.add_mime_type("application/x-cbz");
                filter.add_mime_type("application/x-cbr");
                filter.add_mime_type("application/pdf");
                let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
                filters.append(&filter);
                dialog.set_filters(Some(&filters));

                let window = root.root().and_then(|r| r.downcast::<gtk::Window>().ok());
                let window = window.or_else(|| {
                    relm4::main_application()
                        .active_window()
                        .and_then(|w| w.downcast::<gtk::Window>().ok())
                });

                let s = sender.clone();
                dialog.open_multiple(
                    window.as_ref(),
                    gtk::gio::Cancellable::NONE,
                    move |result| {
                        let mut paths = Vec::new();
                        if let Ok(files) = result {
                            let n = files.n_items();
                            for i in 0..n {
                                if let Some(obj) = files.item(i) {
                                    if let Ok(file) = obj.downcast::<gtk::gio::File>() {
                                        if let Some(p) = file.path() {
                                            paths.push(p);
                                        }
                                    }
                                }
                            }
                            if !paths.is_empty() {
                                s.input(HomeMsg::FilesChosen(paths));
                            }
                        }
                    },
                );
            }
            HomeMsg::FilesChosen(paths) => {
                // Importing parses, hashes and copies each file. Doing that
                // inline froze the window, so it runs on a worker thread and
                // reports back per file (the All Books page does the same).
                let total = paths.len();
                self.importing = true;
                self.status = format!("Importing 1 of {total}…");

                // Imports still go straight to the catalog: writes are not
                // part of the read-snapshot seam (they belong to the task
                // manager, A0 step 4).
                let catalog = self.service.catalog().clone();
                let step_tx = sender.input_sender().clone();
                let done_tx = sender.input_sender().clone();
                spawn_import(
                    catalog,
                    paths,
                    move |done, total, title| {
                        let _ = step_tx.send(HomeMsg::ImportStep { done, total, title });
                    },
                    move |tally| {
                        let _ = done_tx.send(HomeMsg::ImportFinished(tally));
                    },
                );
            }
        }

        // Overriding `update_with_view` replaces relm4's default
        // `update` + `update_view` pair, so nothing refreshes the `#[watch]`
        // bindings unless we say so. Missing this left the status line and the
        // "Importing…" button state frozen at their initial values.
        self.update_view(widgets, sender);
    }
}

fn clear_box(host: &gtk::Box) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
}

/// (Re)populate Home. Called at init and again after an import.
///
/// A0 step 2: one `service.home()` call replaces four direct catalog reads.
/// Everything below is pure widget building against an owned snapshot — which
/// is what makes moving the query to a worker thread a change in the service
/// rather than in this function.
fn rebuild(
    widgets: &HomePageModelWidgets,
    service: &LibraryService,
    sender: &ComponentSender<HomePageModel>,
) {
    let snap = service.home();
    for err in &snap.errors {
        crate::notify::error("Could not read the library", err);
    }

    clear_box(&widgets.counts_host);
    clear_box(&widgets.continue_host);
    clear_box(&widgets.tbr_host);
    clear_box(&widgets.recent_host);

    // Home builds its cards by hand rather than through `build_book_grid`, so
    // it has to warm its own covers -- the cards defer their decode and would
    // otherwise sit on placeholders for ever. Every strip is warmed together
    // below, once the snapshot is in hand.
    let mut on_screen: Vec<crate::models::Book> = Vec::new();

    // ── counts strip ────────────────────────────────────────────────
    let stats = &snap.stats;
    for (label, value) in [
        ("Books", stats.total_books.to_string()),
        ("Reading", stats.reading.to_string()),
        ("Finished", stats.finished.to_string()),
        ("Up next", stats.reading_list.to_string()),
    ] {
        let tile = gtk::Box::new(gtk::Orientation::Vertical, 2);
        tile.add_css_class("kalam-stat-tile");
        let v = gtk::Label::new(Some(&value));
        v.add_css_class("kalam-stat-value");
        v.set_halign(gtk::Align::Start);
        tile.append(&v);
        let l = gtk::Label::new(Some(label));
        l.add_css_class("kalam-stat-label");
        l.set_halign(gtk::Align::Start);
        tile.append(&l);
        widgets.counts_host.append(&tile);
    }

    // ── continue: most recently opened, newest first ────────────────
    // The fallback chain (opened -> in progress -> newest) now lives in the
    // service, where it is unit-tested.
    let cont = &snap.continue_reading;

    if cont.is_empty() {
        let empty = gtk::Label::new(Some(
            "Nothing to continue — import a book and it will show up here.",
        ));
        empty.add_css_class("kalam-placeholder");
        empty.set_wrap(true);
        empty.set_halign(gtk::Align::Start);
        widgets.continue_host.append(&empty);
    } else {
        for book in cont {
            let id = book.id;
            let s1 = sender.clone();
            let s2 = sender.clone();
            let card = build_book_card(
                book,
                move || {
                    s1.output(HomeOut::Book { book_id: id }).ok();
                },
                move || {
                    s2.output(HomeOut::BookDialog { book_id: id }).ok();
                },
            );
            card.add_css_class("kalam-home-continue");
            widgets.continue_host.append(&card);
            on_screen.push(book.clone());
        }
    }

    // ── reading list peek ───────────────────────────────────────────
    let tbr = &snap.reading_list;
    if tbr.is_empty() {
        widgets.tbr_label.set_visible(false);
        widgets.tbr_host.set_visible(false);
    } else {
        widgets.tbr_label.set_visible(true);
        widgets.tbr_host.set_visible(true);
        for (i, entry) in tbr.iter().take(3).enumerate() {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            row.add_css_class("kalam-list-row");

            let ordinal = gtk::Label::new(Some(&format!("{}", i + 1)));
            ordinal.add_css_class("kalam-list-ordinal");
            ordinal.set_width_chars(2);
            row.append(&ordinal);

            let title = gtk::Label::new(Some(&entry.book.title));
            title.add_css_class("kalam-card-title");
            title.set_halign(gtk::Align::Start);
            title.set_hexpand(true);
            title.set_xalign(0.0);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            row.append(&title);

            let author = gtk::Label::new(Some(entry.book.authors_display()));
            author.add_css_class("kalam-card-meta");
            row.append(&author);

            let id = entry.book.id;
            let s = sender.clone();
            let click = gtk::GestureClick::new();
            click.set_button(1);
            click.connect_released(move |_, _, _, _| {
                s.output(HomeOut::Book { book_id: id }).ok();
            });
            row.add_controller(click);
            row.set_cursor_from_name(Some("pointer"));

            widgets.tbr_host.append(&row);
        }
    }

    // ── recently added ──────────────────────────────────────────────
    // Already bounded to 12 by the service, so no second `take` here.
    let recent = &snap.recent;
    if recent.is_empty() {
        let empty = gtk::Label::new(Some("Your library is empty — add a book above."));
        empty.add_css_class("kalam-muted");
        empty.set_halign(gtk::Align::Start);
        widgets.recent_host.append(&empty);
    } else {
        // FlowBox left-aligned to avoid centered covers
        let flow = gtk::FlowBox::builder()
            .max_children_per_line(6)
            .min_children_per_line(2)
            .selection_mode(gtk::SelectionMode::None)
            .column_spacing(16)
            .row_spacing(20)
            .halign(gtk::Align::Start)
            .valign(gtk::Align::Start)
            .hexpand(true)
            .vexpand(false)
            .build();
        flow.add_css_class("kalam-book-grid");
        flow.add_css_class("kalam-home-flow");

        for book in recent {
            let id = book.id;
            let s1 = sender.clone();
            let s2 = sender.clone();
            let card = build_book_card(
                book,
                {
                    let s = s1.clone();
                    move || {
                        s.output(HomeOut::Book { book_id: id }).ok();
                    }
                },
                {
                    let s = s2.clone();
                    move || {
                        s.output(HomeOut::BookDialog { book_id: id }).ok();
                    }
                },
            );
            // Wrap card in fixed cell to keep uniform size in FlowBox
            let cell = gtk::Box::new(gtk::Orientation::Vertical, 0);
            cell.set_size_request(CARD_W, CARD_H);
            cell.set_halign(gtk::Align::Start);
            cell.set_valign(gtk::Align::Start);
            cell.append(&card);
            flow.insert(&cell, -1);
            on_screen.push(book.clone());
        }
        widgets.recent_host.append(&flow);
    }

    // Both strips are showing placeholders until this runs. The size must be
    // exactly what `build_book_card` asked for -- the cache is keyed on it, so
    // a mismatch decodes into an entry nothing ever reads.
    crate::preload::warm_books(&on_screen, 0, COVER_W, COVER_H);
}
