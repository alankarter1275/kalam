use crate::db::Catalog;
use crate::service::HomeSnapshot;
use crate::models::BookFormat;
use crate::pages::all_books::{import_summary, spawn_import, ImportTally};
use crate::service::LibraryService;
use crate::widgets::book_row::{build_book_card, CARD_H, CARD_W, COVER_H, COVER_W};
use gtk::prelude::*;
use relm4::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
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
    /// Open a comic series in the Comics hub
    ComicSeries {
        series_name: String,
    },
    /// Open the full search page (chat 10) with the current query.
    Search {
        query: String,
        inside_books: bool,
    },
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
    /// The home snapshot finished on its worker (7.1 step 2b) — the
    /// page painted its skeleton in `init` and fills every section from
    /// this. No library read happens on the UI thread.
    Loaded(Box<HomeSnapshot>),
    /// Search bar gained focus: drop the search box under the bar.
    SearchFocused,
    /// Search bar lost focus: hide the box after a short delay.
    SearchBlurred,
    /// User typed in the search bar: update "N results" and show matches.
    SearchChanged(String),
    /// User pressed Enter in the bar: open the full search page.
    SearchActivate,
    /// "N results" clicked: open the full search page with current query.
    SearchOpenResults,
    /// Recent search item chosen: run that search.
    SearchHistoryItem { query: String },
    /// Clear all recent searches.
    SearchClear,
    /// Option picker toggle button clicked.
    SearchPickerToggle,
    /// Search inside books switch toggled.
    SearchInsideBooksToggled(bool),
}

pub struct HomePageModel {
    service: LibraryService,
    importing: bool,
    status: String,
    /// Current search query typed in the bar.
    search_query: String,
    /// Whether the search bar currently has focus (box shown).
    search_focused: bool,
    /// "Search inside books" switch state (§199). Off by default, resets
    /// to off at every restart.
    search_inside_books: bool,
    /// Recent searches, most recent first (max 20, §197).
    search_history: Rc<RefCell<Vec<String>>>,
    /// The search box popover, built lazily on first focus.
    search_box: Option<gtk::Popover>,
    /// The option picker popover under the sliders icon.
    search_picker: Option<gtk::Popover>,
    /// Widgets for dynamic updates.
    #[allow(dead_code)]
    search_count_lbl: gtk::Label,
    search_entry: gtk::Entry,
    search_picker_btn: gtk::Button,
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

            // ── Search bar (§57): top left of the main area, 28 from top ──
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_hexpand: false,
                set_halign: gtk::Align::Start,
                set_margin_top: 28,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    add_css_class: "kalam-home-search-bar",
                    set_hexpand: false,
                    set_width_request: 520,
                    set_height_request: 40,

                    gtk::Image {
                        add_css_class: "kalam-home-search-magnifier",
                        set_from_icon_name: Some("edit-find-symbolic"),
                        set_pixel_size: 18,
                    },

                    #[name = "search_entry"]
                    gtk::Entry {
                        set_placeholder_text: Some("Search your library"),
                        set_hexpand: true,
                    },

                    #[name = "search_count_lbl"]
                    gtk::Label {
                        add_css_class: "kalam-home-search-count",
                        set_visible: false,
                        set_halign: gtk::Align::End,
                        set_hexpand: false,
                        set_label: "0 results",
                    },

                    #[name = "search_picker_btn"]
                    gtk::Button {
                        add_css_class: "kalam-home-search-picker-btn",
                        set_child: Some(&gtk::Image {
                            add_css_class: "kalam-home-search-picker-icon",
                            set_from_icon_name: Some("emblem-system-symbolic"),
                            set_pixel_size: 20,
                        }),
                        set_tooltip_text: Some("Search options"),
                    },
                },
            },

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

                // The skeleton's only moving part: cleared by the first
                // `Loaded` apply along with the rest of the host. Without
                // it, the empty sections paint as if the library were
                // empty (2b; the book page learned this first).
                gtk::Label {
                    set_label: "Loading your library…",
                    add_css_class: "kalam-muted",
                    set_halign: gtk::Align::Start,
                },
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
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();

        let search_entry = widgets.search_entry.clone();
        let s = sender.clone();
        search_entry.connect_changed(move |entry| {
            let query = entry.text().to_string();
            if !query.is_empty() {
                s.input(HomeMsg::SearchChanged(query));
            }
        });
        let s2 = sender.clone();
        search_entry.connect_activate(move |_| {
            s2.input(HomeMsg::SearchActivate);
        });

        let switch = widgets.search_inside_books_switch.clone();
        let s3 = sender.clone();
        switch.connect_active(move |sw| {
            s3.input(HomeMsg::SearchInsideBooksToggled(sw.is_active()));
        });

        // Focus in/out on the entry show/hide the search box (§197).
        let s4 = sender.clone();
        widgets.search_entry.connect_focus_in(move |_, _| {
            s4.input(HomeMsg::SearchFocused);
        });
        let s5 = sender.clone();
        widgets.search_entry.connect_focus_out(move |_, _| {
            s5.input(HomeMsg::SearchBlurred);
        });

        // Build the option picker popover (§200: Library ticked, online sites
        // dimmed, divider, Search inside books switch). 260 wide, right-aligned.
        let picker = gtk::Popover::builder()
            .position(gtk::PositionType::Bottom)
            .autohide(true)
            .halign(gtk::Align::End)
            .valign(gtk::Align::Start)
            .build();
        picker.add_css_class("kalam-home-search-picker");
        let picker_list = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(0)
            .margin_all(8)
            .build();

        let heading = gtk::Label::builder()
            .label("Search in")
            .css_classes(["kalam-home-search-picker-heading"])
            .halign(gtk::Align::Start)
            .hexpand(false)
            .build();
        picker_list.append(&heading);

        let lib_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(6)
            .margin_start(12)
            .margin_end(12)
            .margin_bottom(8)
            .halign(gtk::Align::Start)
            .build();
        let lib_check = gtk::CheckButton::builder()
            .label("Library")
            .active(true)
            .halign(gtk::Align::Start)
            .build();
        lib_row.append(&lib_check);
        picker_list.append(&lib_row);

        let online_heading = gtk::Label::builder()
            .label("Online · not available yet")
            .css_classes(["kalam-home-search-picker-heading"])
            .halign(gtk::Align::Start)
            .hexpand(false)
            .margin_top(6)
            .build();
        picker_list.append(&online_heading);

        let online_rows = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(2)
            .margin_start(12)
            .margin_end(12)
            .halign(gtk::Align::Start)
            .visible(false)
            .build();
        for site in ["AO3", "Fanfiction.net", "Royal Road", "MangaDex"] {
            let btn = gtk::Button::builder()
                .label(site)
                .halign(gtk::Align::Start)
                .hexpand(false)
                .sensitive(false)
                .build();
            online_rows.append(&btn);
        }
        picker_list.append(&online_rows);

        let divider = gtk::Separator::builder()
            .css_classes(["kalam-home-search-picker-divider"])
            .margin_start(12)
            .margin_end(12)
            .build();
        picker_list.append(&divider);

        let switch_row = gtk::Box::builder()
            .orientation(gtk::Orientation::Horizontal)
            .spacing(12)
            .margin_start(12)
            .margin_end(12)
            .halign(gtk::Align::Start)
            .build();
        let inside_switch = gtk::Switch::builder().active(false).build();
        let inside_label = gtk::Label::builder()
            .label("Search inside books")
            .halign(gtk::Align::Start)
            .hexpand(true)
            .tooltip_text(Some("Looks through the full text. Slower."))
            .build();
        switch_row.append(&inside_switch);
        switch_row.append(&inside_label);
        picker_list.append(&switch_row);

        let caption = gtk::Label::builder()
            .label("Looks through the full text. Slower.")
            .css_classes(["kalam-home-search-switch-caption"])
            .halign(gtk::Align::Start)
            .hexpand(true)
            .margin_start(36)
            .margin_bottom(8)
            .build();
        picker_list.append(&caption);

        picker.set_child(Some(&picker_list));
        widgets.search_picker_btn.set_popover(Some(&picker));

        // Inside books switch in picker also updates model state.
        let s_inside = sender.clone();
        inside_switch.connect_active(move |sw| {
            s_inside.input(HomeMsg::SearchInsideBooksToggled(sw.is_active()));
        });

        let model = HomePageModel {
            service: LibraryService::new(catalog),
            importing: false,
            status: String::new(),
            search_query: String::new(),
            search_focused: false,
            search_inside_books: false,
            search_history: Rc::new(RefCell::new(Vec::new())),
            search_box: None,
            search_picker: Some(picker),
            search_count_lbl: widgets.search_count_lbl.clone(),
            search_entry,
            search_picker_btn: widgets.search_picker_btn.clone(),
        };

        // Skeleton only (7.1 step 2b): the sections paint empty with a
        // loading row, and every read runs on the worker below.
        request_snapshot(&model.service, &sender);

        let _ = root;
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
                // Same road as init: the worker reads, the apply builds.
                request_snapshot(&self.service, &sender);
            }
            HomeMsg::Loaded(snap) => {
                apply_snapshot(widgets, snap, &sender);
            }
            HomeMsg::AllBooks => {
                sender.output(HomeOut::AllBooks).ok();
            }
            HomeMsg::SearchFocused => {
                self.search_focused = true;
                self.show_search_box(widgets, &sender);
            }
            HomeMsg::SearchBlurred => {
                // Small delay so clicking inside the box doesn't close it.
                if let Some(box_) = &model.search_box {
                    let box_ = box_.clone();
                    gtk::glib::timeout_add_local(std::time::Duration::from_millis(150), move || {
                        box_.popdown();
                        glib::Continue(false)
                    });
                }
                model.search_focused = false;
            }
            HomeMsg::SearchChanged(query) => {
                self.search_query = query.clone();
                if query.is_empty() {
                    widgets.search_count_lbl.set_visible(false);
                    widgets.search_count_lbl.set_label("0 results");
                } else {
                    widgets.search_count_lbl.set_visible(true);
                    widgets.search_count_lbl.set_label(&format!("{query} results"));
                }
            }
            HomeMsg::SearchActivate => {
                if !self.search_query.is_empty() {
                    sender.output(HomeOut::Search {
                        query: self.search_query.clone(),
                        inside_books: self.search_inside_books,
                    }).ok();
                }
            }
            HomeMsg::SearchOpenResults => {
                if !self.search_query.is_empty() {
                    sender.output(HomeOut::Search {
                        query: self.search_query.clone(),
                        inside_books: self.search_inside_books,
                    }).ok();
                }
            }
            HomeMsg::SearchHistoryItem { query } => {
                // Add to history, front-most, cap at 20 (§197).
                let mut hist = self.search_history.borrow_mut();
                if let Some(pos) = hist.iter().position(|q| q == &query) {
                    hist.remove(pos);
                }
                hist.insert(0, query.clone());
                if hist.len() > 20 {
                    hist.truncate(20);
                }
                drop(hist);
                self.search_query = query.clone();
                widgets.search_entry.set_text(&query);
                sender.output(HomeOut::Search {
                    query,
                    inside_books: self.search_inside_books,
                }).ok();
            }
            HomeMsg::SearchClear => {
                self.search_history.borrow_mut().clear();
            }
            HomeMsg::SearchPickerToggle => {
                // Popover is auto-managed by GTK (set_autohide: true).
            }
            HomeMsg::SearchInsideBooksToggled(enabled) => {
                self.search_inside_books = enabled;
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

/// Build and show the search box popover under the search bar (§197).
/// Lazily constructed on first focus so the box only exists when needed.
fn show_search_box(
    model: &mut HomePageModel,
    widgets: &HomePageModelWidgets,
    sender: &ComponentSender<HomePageModel>,
) {
    if model.search_box.is_some() {
        // Already built; just reposition and pop up.
        if let Some(box_) = &model.search_box {
            box_.set_parent(Some(&widgets.search_entry));
            box_.set_pointing_to(Some(&widgets.search_entry));
            box_.popup();
        }
        return;
    }

    let box_ = gtk::Popover::builder()
        .position(gtk::PositionType::Bottom)
        .autohide(false)
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Top)
        .build();
    box_.add_css_class("kalam-home-search-box");

    let contents = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(0)
        .build();

    // Recent searches heading (§197).
    let hist_heading = gtk::Label::builder()
        .label("Recent searches")
        .css_classes(["kalam-home-search-heading"])
        .halign(gtk::Align::Start)
        .hexpand(false)
        .margin_start(12)
        .margin_top(8)
        .build();
    contents.append(&hist_heading);

    // History rows (built from model.search_history).
    let hist_host = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(0)
        .build();
    contents.append(&hist_host);

    let refresh_history = |hist_host: &gtk::Box, history: &Rc<RefCell<Vec<String>>>,
                           sender: &ComponentSender<HomePageModel>| {
        clear_box(hist_host);
        let hist = history.borrow();
        for query in hist.iter() {
            let q = query.clone();
            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .margin_start(12)
                .margin_end(12)
                .margin_top(6)
                .margin_bottom(6)
                .valign(gtk::Align::Center)
                .hexpand(true)
                .build();
            row.add_css_class("kalam-home-search-history-row");

            let lbl = gtk::Label::builder()
                .label(&q)
                .halign(gtk::Align::Start)
                .hexpand(true)
                .build();
            row.append(&lbl);

            let cross = gtk::Button::builder()
                .child(&gtk::Image::builder()
                    .from_icon_name("edit-delete-symbolic")
                    .pixel_size(14)
                    .build())
                .css_classes(["kalam-home-search-history-cross"])
                .halign(gtk::Align::End)
                .build();
            let s_hist = sender.clone();
            cross.connect_clicked(move |_| {
                s_hist.input(HomeMsg::SearchHistoryItem { query: q.clone() });
            });
            row.append(&cross);

            hist_host.append(&row);
        }

        if !hist.is_empty() {
            let clear_btn = gtk::Button::builder()
                .label("Clear")
                .css_classes(["kalam-home-search-clear"])
                .halign(gtk::Align::Start)
                .margin_start(12)
                .margin_top(8)
                .margin_bottom(8)
                .build();
            let s_clear = sender.clone();
            clear_btn.connect_clicked(move |_| {
                s_clear.input(HomeMsg::SearchClear);
            });
            hist_host.append(&clear_btn);
        }
    };

    refresh_history(&hist_host, &model.search_history, sender);

    box_.set_child(Some(&contents));
    box_.set_parent(Some(&widgets.search_entry));
    box_.set_pointing_to(Some(&widgets.search_entry));
    box_.popup();

    model.search_box = Some(box_);
}

/// Ask the worker for a fresh snapshot. Called at init, after an import,
/// and nowhere else — the UI thread never reads the library (7.1 step 2b).
fn request_snapshot(service: &LibraryService, sender: &ComponentSender<HomePageModel>) {
    let service = LibraryService::new(service.catalog().clone());
    let s = sender.input_sender().clone();
    crate::tasks::spawn_internal(
        "Reading home",
        move |_reporter| service.home(),
        |_| {},
        move |snap| {
            let _ = s.send(HomeMsg::Loaded(Box::new(snap)));
        },
    );
}

/// (Re)populate Home from an owned snapshot the worker already read.
///
/// A0 step 2 made this pure widget building against one snapshot; 2b moved
/// the read itself to the worker, so this is now the apply half of the
/// skeleton/worker/apply recipe — the same shape as the book page.
fn apply_snapshot(
    widgets: &HomePageModelWidgets,
    snap: Box<HomeSnapshot>,
    sender: &ComponentSender<HomePageModel>,
) {
    let snap = *snap;
    // The same attribution pair the book page uses (2a.1): a timing
    // span for KALAM_TIMING=1 and an activity label so a stall during
    // this build says "while home_fill" instead of naming nothing.
    let _t = crate::timing::measure("home_fill");
    let _a = crate::timing::activity("home_fill");
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
            let s_series = book.series.clone();
            let is_comic = matches!(book.format, BookFormat::Cbz | BookFormat::Cbr);
            let s1 = sender.clone();
            let s2 = sender.clone();
            let card = build_book_card(
                book,
                crate::widgets::book_row::CLICK_HINT_LIBRARY,
                {
                    let s = s1.clone();
                    let sname = s_series.clone();
                    move || {
                        if is_comic {
                            if let Some(ref sname) = sname {
                                s.output(HomeOut::ComicSeries { series_name: sname.clone() }).ok();
                                return;
                            }
                        }
                        s.output(HomeOut::Book { book_id: id }).ok();
                    }
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
            let s_series = book.series.clone();
            let is_comic = matches!(book.format, BookFormat::Cbz | BookFormat::Cbr);
            let s1 = sender.clone();
            let s2 = sender.clone();
            let card = build_book_card(
                book,
                crate::widgets::book_row::CLICK_HINT_LIBRARY,
                {
                    let s = s1.clone();
                    let sname = s_series.clone();
                    move || {
                        if is_comic {
                            if let Some(ref sname) = sname {
                                s.output(HomeOut::ComicSeries { series_name: sname.clone() }).ok();
                                return;
                            }
                        }
                        s.output(HomeOut::Book { book_id: id }).ok();
                    }
                },
                {
                    let s = s2.clone();
                    let sname = s_series.clone();
                    move || {
                        if is_comic {
                            if let Some(ref sname) = sname {
                                s.output(HomeOut::ComicSeries { series_name: sname.clone() }).ok();
                                return;
                            }
                        }
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
