use crate::db::Catalog;
use crate::service::HomeSnapshot;
use crate::pages::all_books::{import_summary, ImportTally};
use crate::service::LibraryService;
use crate::widgets::book_row::{CARD_H, CARD_W, COVER_H, COVER_W};
use gtk::prelude::*;
use relm4::prelude::*;
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

/// Navigation requests from Home.
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
    ImportStep {
        done: usize,
        total: usize,
        title: String,
    },
    ImportFinished(ImportTally),
    Loaded(Box<HomeSnapshot>),
    /// Search bar gained focus: show the search box under the bar.
    SearchFocused,
    /// Search bar lost focus: hide the search box after a short delay.
    SearchBlurred,
    /// User typed in the search bar.
    SearchChanged(String),
    /// User pressed Enter in the bar: open the full search page.
    SearchActivate,
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
    search_query: String,
    search_focused: bool,
    search_inside_books: bool,
    search_history: Rc<RefCell<Vec<String>>>,
    search_count_lbl: gtk::Label,
    search_entry: gtk::Entry,
    #[allow(dead_code)]
    search_picker_btn: gtk::Button,
    search_box_host: gtk::Box,
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
                        set_icon_name: Some("edit-find-symbolic"),
                        set_pixel_size: 18,
                    },

                    #[name = "search_entry"]
                    gtk::Entry {
                        set_placeholder_text: Some("Search your library"),
                        set_hexpand: true,
                        set_activates_default: true,
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
                        #[wrap(Some)]
                        set_child = &gtk::Image {
                            add_css_class: "kalam-home-search-picker-icon",
                            set_icon_name: Some("emblem-system-symbolic"),
                            set_pixel_size: 20,
                        },
                        set_tooltip_text: Some("Search options"),
                        #[wrap(Some)]
                        set_popover = &gtk::Popover {
                            add_css_class: "kalam-home-search-picker",
                            set_position: gtk::PositionType::Bottom,
                            set_autohide: true,
                            set_halign: gtk::Align::End,
                            set_valign: gtk::Align::Start,
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 0,
                                set_margin_all: 8,

                                gtk::Label {
                                    add_css_class: "kalam-home-search-picker-heading",
                                    set_label: "Search in",
                                    set_halign: gtk::Align::Start,
                                    set_hexpand: false,
                                },

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 6,
                                    set_margin_start: 12,
                                    set_margin_end: 12,
                                    set_margin_bottom: 8,
                                    set_halign: gtk::Align::Start,

                                    gtk::CheckButton {
                                        set_active: true,
                                        set_label: "Library",
                                        set_halign: gtk::Align::Start,
                                    },
                                },

                                gtk::Label {
                                    add_css_class: "kalam-home-search-picker-heading",
                                    set_label: "Online · not available yet",
                                    set_halign: gtk::Align::Start,
                                    set_hexpand: false,
                                    set_margin_top: 6,
                                },

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_spacing: 2,
                                    set_margin_start: 12,
                                    set_margin_end: 12,
                                    set_halign: gtk::Align::Start,
                                    set_visible: false,

                                    gtk::Button {
                                        set_label: "AO3",
                                        set_halign: gtk::Align::Start,
                                        set_hexpand: false,
                                        set_sensitive: false,
                                    },
                                    gtk::Button {
                                        set_label: "Fanfiction.net",
                                        set_halign: gtk::Align::Start,
                                        set_hexpand: false,
                                        set_sensitive: false,
                                    },
                                    gtk::Button {
                                        set_label: "Royal Road",
                                        set_halign: gtk::Align::Start,
                                        set_hexpand: false,
                                        set_sensitive: false,
                                    },
                                    gtk::Button {
                                        set_label: "MangaDex",
                                        set_halign: gtk::Align::Start,
                                        set_hexpand: false,
                                        set_sensitive: false,
                                    },
                                },

                                gtk::Separator {
                                    add_css_class: "kalam-home-search-picker-divider",
                                    set_margin_start: 12,
                                    set_margin_end: 12,
                                },

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 12,
                                    set_margin_start: 12,
                                    set_margin_end: 12,
                                    set_halign: gtk::Align::Start,

                                    #[name = "search_inside_books_switch"]
                                    gtk::Switch {
                                        set_active: false,
                                    },

                                    gtk::Label {
                                        set_label: "Search inside books",
                                        set_halign: gtk::Align::Start,
                                        set_hexpand: true,
                                        set_tooltip_text: Some("Looks through the full text. Slower."),
                                    },
                                },

                                gtk::Label {
                                    add_css_class: "kalam-home-search-switch-caption",
                                    set_label: "Looks through the full text. Slower.",
                                    set_halign: gtk::Align::Start,
                                    set_hexpand: true,
                                    set_margin_start: 36,
                                    set_margin_bottom: 8,
                                },
                            },
                        },
                    },
                },
            },

            // ── Search box dropped under the bar (§197) ──
            #[name = "search_box_host"]
            gtk::Box {
                add_css_class: "kalam-home-search-box",
                set_hexpand: false,
                set_width_request: 520,
                set_valign: gtk::Align::Start,
                set_visible: false,
                set_margin_top: -1,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 0,

                    gtk::Label {
                        add_css_class: "kalam-home-search-heading",
                        set_label: "Recent searches",
                        set_halign: gtk::Align::Start,
                        set_hexpand: false,
                        set_margin_start: 12,
                        set_margin_top: 8,
                    },

                    #[name = "search_history_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 0,
                    },

                    #[name = "search_clear_btn"]
                    gtk::Button {
                        add_css_class: "kalam-home-search-clear",
                        set_label: "Clear",
                        set_halign: gtk::Align::Start,
                        set_margin_start: 12,
                        set_margin_top: 8,
                        set_margin_bottom: 8,
                        set_visible: false,
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
                    set_label: if self.importing { "Importing…" } else { "+ Add books" },
                    add_css_class: "kalam-primary-btn",
                    #[watch]
                    set_sensitive: !self.importing,
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
                set_label: &self.status,
                #[watch]
                set_visible: !self.status.is_empty(),
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

                gtk::Label {
                    set_label: "Loading your library…",
                    add_css_class: "kalam-muted",
                    set_halign: gtk::Align::Start,
                },
            },

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

        // Focus tracking on the entry: show/hide the search box.
        let s4 = sender.clone();
        widgets.search_entry.connect_has_focus_notify(move |entry| {
            if entry.is_focus() {
                s4.input(HomeMsg::SearchFocused);
            } else {
                s4.input(HomeMsg::SearchBlurred);
            }
        });

        // Clear button.
        let s_clear = sender.clone();
        widgets.search_clear_btn.connect_clicked(move |_| {
            s_clear.input(HomeMsg::SearchClear);
        });

        let model = HomePageModel {
            service: LibraryService::new(catalog),
            importing: false,
            status: String::new(),
            search_query: String::new(),
            search_focused: false,
            search_inside_books: false,
            search_history: Rc::new(RefCell::new(Vec::new())),
            search_count_lbl: widgets.search_count_lbl.clone(),
            search_entry,
            search_picker_btn: widgets.search_picker_btn.clone(),
            search_box_host: widgets.search_box_host.clone(),
        };

        request_snapshot(&model.service, &sender);

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
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
                widgets.search_box_host.set_visible(true);
            }
            HomeMsg::SearchBlurred => {
                self.search_focused = false;
                widgets.search_box_host.set_visible(false);
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
            HomeMsg::SearchHistoryItem { query } => {
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
                widgets.search_clear_btn.set_visible(false);
                clear_box(&widgets.search_history_host);
            }
            HomeMsg::SearchPickerToggle => {
                // Popover is auto-managed by GTK (set_autohide: true).
            }
            HomeMsg::SearchInsideBooksToggled(enabled) => {
                self.search_inside_books = enabled;
            }
        }

        self.update_view(widgets, sender);
    }

    fn update_view(
        &mut self,
        widgets: &mut Self::Widgets,
        _sender: ComponentSender<Self>,
    ) {
        // Clear button visibility (updated when history changes).
        let has_history = !self.search_history.borrow().is_empty();
        widgets.search_clear_btn.set_visible(has_history);
    }
}

fn clear_box(host: &gtk::Box) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
}

/// Ask the worker for a fresh snapshot.
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
fn apply_snapshot(
    widgets: &HomePageModelWidgets,
    snap: Box<HomeSnapshot>,
    sender: &ComponentSender<HomePageModel>,
) {
    let snap = *snap;
    let _t = crate::timing::measure("home_fill");
    let _a = crate::timing::activity("home_fill");
    for err in &snap.errors {
        crate::notify::error("Could not read the library", err);
    }

    clear_box(&widgets.counts_host);
    clear_box(&widgets.continue_host);
    clear_box(&widgets.tbr_host);
    clear_box(&widgets.recent_host);
}
