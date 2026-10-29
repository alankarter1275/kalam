use crate::db::{Annotation, Catalog, SortKey};
use crate::models::Book;
use crate::service::HomeSnapshot;
use crate::pages::all_books::{import_summary, ImportTally};
use crate::service::LibraryService;
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

/// Grouped search results shown in the search box while typing.
#[derive(Debug, Clone, Default)]
pub struct SearchResults {
    pub books: Vec<Book>,
    pub authors: Vec<String>,
    pub series: Vec<String>,
    pub quotes: Vec<(Annotation, Option<Book>)>,
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
    /// Recent search item deleted from history.
    SearchHistoryDelete { query: String },
    /// Clear all recent searches.
    SearchClear,
    /// Search inside books switch toggled.
    SearchInsideBooksToggled(bool),
    /// Search results for the current query, grouped by type.
    SearchResults(SearchResults),
}

pub struct HomePageModel {
    service: LibraryService,
    importing: bool,
    status: String,
    search_query: String,
    #[allow(dead_code)]
    search_focused: bool,
    search_inside_books: bool,
    search_history: Rc<RefCell<Vec<String>>>,
    search_count_lbl: gtk::Label,
    search_entry: gtk::Entry,
    search_picker_btn: gtk::MenuButton,
    search_box_host: gtk::Box,
    search_scroll_content: gtk::Box,
    search_scroll: gtk::ScrolledWindow,
    search_fade: gtk::Box,
    home_quote: gtk::Label,
    home_quote_source: gtk::Label,
    home_book_host: gtk::Box,
    home_updates_host: gtk::Box,
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

                gtk::Box {
                    add_css_class: "kalam-home-search-bar",
                    set_orientation: gtk::Orientation::Horizontal,
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
                    gtk::MenuButton {
                        add_css_class: "kalam-home-search-picker-btn",
                        set_always_show_arrow: false,
                        #[wrap(Some)]
                        set_child = &gtk::Image {
                            add_css_class: "kalam-home-search-picker-icon",
                            set_icon_name: Some("emblem-system-symbolic"),
                            set_pixel_size: 20,
                        },
                        set_tooltip_text: Some("Search options"),
                    },
                },
            },

            // ── Search box dropped under the bar (§197) ──
            #[name = "search_box_host"]
            gtk::Box {
                add_css_class: "kalam-home-search-box",
                set_orientation: gtk::Orientation::Vertical,
                set_hexpand: false,
                set_width_request: 520,
                set_valign: gtk::Align::Start,
                set_visible: false,
                set_margin_top: -1,

                // Scrollable content with bottom fade.
                gtk::Overlay {
                    set_hexpand: true,
                    set_vexpand: false,

                    #[name = "search_scroll"]
                    gtk::ScrolledWindow {
                        set_policy: (gtk::PolicyType::Never, gtk::PolicyType::External),
                        set_hexpand: true,
                        set_vexpand: false,
                        set_overlay_scrolling: true,

                        #[name = "search_scroll_content"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 0,

                            #[name = "search_history_heading"]
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

                    // Bottom fade gradient — indicates scrollable content.
                    #[name = "search_fade"]
                    gtk::Box {
                        add_css_class: "k-fade-bottom",
                        set_valign: gtk::Align::End,
                        set_hexpand: true,
                        set_size_request: (-1, 16),
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
                    set_label: "+ Add books",
                    add_css_class: "kalam-primary-btn",
                    connect_clicked => HomeMsg::AddBooks,
                },
            },

            gtk::Label {
                set_label: "Continue reading and recently added · click = float · Ctrl+click = full page",
                add_css_class: "kalam-page-sub",
                set_halign: gtk::Align::Start,
            },

            // ── Home quote (§63, decision 187) ──
            #[name = "home_quote"]
            gtk::Label {
                add_css_class: "kalam-home-quote",
                set_label: "“There is no friend as loyal as a book.”",
                set_halign: gtk::Align::Start,
                set_hexpand: false,
                set_wrap: true,
                set_max_width_chars: 70,
            },

            #[name = "home_quote_source"]
            gtk::Label {
                add_css_class: "kalam-home-quote-source",
                set_label: "— Ernest Hemingway",
                set_halign: gtk::Align::Start,
                set_hexpand: false,
            },

            // ── Home book (§60, decision 188) ──
            #[name = "home_book_host"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                set_halign: gtk::Align::Start,
                set_hexpand: false,
                set_margin_start: 12,
                set_margin_top: 24,
                set_margin_bottom: 16,
            },

            // ── Home updates (§62, decision 189) ──
            #[name = "home_updates_host"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                set_halign: gtk::Align::Start,
                set_hexpand: false,
                set_margin_start: 12,
                set_margin_bottom: 16,
            },

            #[name = "status_label"]
            gtk::Label {
                set_label: "",
                add_css_class: "kalam-muted",
                set_halign: gtk::Align::Start,
                set_visible: false,
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

        // "N results" count label: clicking opens the full search page.
        let count = widgets.search_count_lbl.clone();
        let s_count = sender.clone();
        let click = gtk::EventControllerClick::new();
        click.connect_released(move |_, _, _, _| {
            if !count.text().is_empty() && count.text() != "0 results" {
                let _ = s_count.output(HomeOut::Search {
                    query: String::new(),
                    inside_books: false,
                });
            }
        });
        count.add_controller(click);

        let model = HomePageModel {
            service: LibraryService::new(catalog),
            importing: false,
            status: String::new(),
            search_query: String::new(),
            search_focused: false,
            search_inside_books: false,
    search_history: Rc::new(RefCell::new(Vec::new())),
    search_results: SearchResults::default(),
    search_count_lbl: widgets.search_count_lbl.clone(),
            search_entry,
            search_picker_btn: widgets.search_picker_btn.clone(),
            search_box_host: widgets.search_box_host.clone(),
            search_scroll_content: widgets.search_scroll_content.clone(),
            search_scroll: widgets.search_scroll.clone(),
            search_fade: widgets.search_fade.clone(),
            home_quote: widgets.home_quote.clone(),
            home_quote_source: widgets.home_quote_source.clone(),
            home_book_host: widgets.home_book_host.clone(),
            home_updates_host: widgets.home_updates_host.clone(),
        };

        // ── Option picker popover (§57, decision 200) ──
        let picker_popover = gtk::Popover::new();
        picker_popover.add_css_class("kalam-home-search-picker");
        picker_popover.set_has_arrow(false);
        picker_popover.set_position(gtk::PositionType::Bottom);
        picker_popover.set_autohide(true);

        let picker_list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        picker_list.set_hexpand(true);

        // "Search in" heading.
        let search_in_heading = gtk::Label::new(Some("Search in"));
        search_in_heading.add_css_class("kalam-home-search-picker-heading");
        search_in_heading.set_halign(gtk::Align::Start);
        search_in_heading.set_margin_start(12);
        search_in_heading.set_margin_top(8);
        search_in_heading.set_margin_bottom(4);
        picker_list.append(&search_in_heading);

        // Library row (ticked — always the active choice; online sites are
        // disabled until Part 2, decision 8).
        let library_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        library_row.set_hexpand(true);
        library_row.set_valign(gtk::Align::Center);
        library_row.set_margin_start(12);
        library_row.set_margin_end(12);
        library_row.set_margin_top(6);
        library_row.set_margin_bottom(6);

        let library_lbl = gtk::Label::new(Some("Library"));
        library_lbl.set_halign(gtk::Align::Start);
        library_lbl.set_hexpand(true);
        library_row.append(&library_lbl);

        let library_tick = gtk::Label::new(Some("✓"));
        library_tick.add_css_class("kalam-ui-menu-check");
        library_tick.add_css_class("checked");
        library_tick.set_size_request(22, 22);
        library_tick.set_halign(gtk::Align::Center);
        library_row.append(&library_tick);

        let library_btn = gtk::Button::new();
        library_btn.add_css_class("kalam-ui-menu-item");
        library_btn.set_hexpand(true);
        library_btn.set_can_focus(true);
        library_btn.set_child(Some(&library_row));
        picker_list.append(&library_btn);

        // "Online · not available yet" sub-heading (dimmed).
        let online_heading = gtk::Label::new(Some("Online · not available yet"));
        online_heading.add_css_class("kalam-home-search-picker-dim");
        online_heading.set_halign(gtk::Align::Start);
        online_heading.set_margin_start(12);
        online_heading.set_margin_top(8);
        online_heading.set_margin_bottom(4);
        picker_list.append(&online_heading);

        // Four online sites, dimmed and disabled (Part 2).
        for site in ["AO3", "Fanfiction.net", "Royal Road", "MangaDex"] {
            let site_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            site_row.set_hexpand(true);
            site_row.set_valign(gtk::Align::Center);
            site_row.set_margin_start(12);
            site_row.set_margin_end(12);
            site_row.set_margin_top(4);
            site_row.set_margin_bottom(4);

            let site_lbl = gtk::Label::new(Some(site));
            site_lbl.add_css_class("kalam-home-search-picker-dim");
            site_lbl.set_halign(gtk::Align::Start);
            site_lbl.set_hexpand(true);
            site_row.append(&site_lbl);

            let site_btn = gtk::Button::new();
            site_btn.add_css_class("kalam-ui-menu-item");
            site_btn.set_sensitive(false);
            site_btn.set_hexpand(true);
            site_btn.set_can_focus(false);
            site_btn.set_child(Some(&site_row));
            picker_list.append(&site_btn);
        }

        // Divider.
        let divider = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        divider.add_css_class("kalam-home-search-picker-divider");
        divider.set_size_request(-1, 1);
        picker_list.append(&divider);

        // "Search inside books" switch row (off by default, decision 199).
        let switch_row = gtk::Box::new(gtk::Orientation::Vertical, 0);
        switch_row.add_css_class("kalam-home-search-switch-row");
        switch_row.set_hexpand(true);
        switch_row.set_margin_start(12);
        switch_row.set_margin_end(12);
        switch_row.set_margin_top(8);
        switch_row.set_margin_bottom(8);

        let switch_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let switch_lbl = gtk::Label::new(Some("Search inside books"));
        switch_lbl.set_hexpand(true);
        switch_lbl.set_halign(gtk::Align::Start);
        let switch_widget = gtk::Switch::new();
        switch_widget.set_active(model.search_inside_books);
        switch_box.append(&switch_lbl);
        switch_box.append(&switch_widget);
        switch_row.append(&switch_box);

        let switch_caption =
            gtk::Label::new(Some("Looks through the full text. Slower."));
        switch_caption.add_css_class("kalam-home-search-switch-caption");
        switch_caption.set_halign(gtk::Align::Start);
        switch_caption.set_margin_top(2);
        switch_row.append(&switch_caption);

        picker_list.append(&switch_row);

        picker_popover.set_child(Some(&picker_list));
        widgets.search_picker_btn.set_popover(Some(&picker_popover));

        // Switch toggle → SearchInsideBooksToggled.
        let s_switch = sender.clone();
        switch_widget.connect_active_notify(move |sw| {
            s_switch.input(HomeMsg::SearchInsideBooksToggled(sw.is_active()));
        });

        // Show bottom fade when scrollable content overflows.
        let fade = model.search_fade.clone();
        let content = model.search_scroll_content.clone();
        if let Some(adj) = model.search_scroll.vadjustment() {
            adj.connect_value_changed(move |adj| {
                let at_bottom = adj.get_value() + adj.get_page_size() >= adj.get_upper() - 1.0;
                let has_content = content.first_child().is_some();
                fade.set_visible(has_content && !at_bottom);
            });
            // Initial fade state.
            let at_bottom = adj.get_value() + adj.get_page_size() >= adj.get_upper() - 1.0;
            let has_content = content.first_child().is_some();
            fade.set_visible(has_content && !at_bottom);
        }

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
            HomeMsg::AddBooks => {
                // Handled by the import dialog; kept for match completeness.
            }
            HomeMsg::FilesChosen(_) => {
                // Handled by the import worker; kept for match completeness.
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
                    self.search_results = SearchResults::default();
                } else {
                    widgets.search_count_lbl.set_visible(true);
                    widgets.search_count_lbl.set_label(&format!("{query} results"));
                    self.search_results = SearchResults::default();
                    search_library(
                        self.service.catalog().clone(),
                        query.clone(),
                        self.search_inside_books,
                        &sender,
                    );
                }
            }
            HomeMsg::SearchActivate => {
                if self.search_query.is_empty() {
                    return;
                }
                // Enter opens the first result if any, otherwise the full
                // search page. "First result" = first book in the Books
                // group, else first author, else first series, else first
                // saved quote.
                if let Some(book) = self.search_results.books.first() {
                    sender.output(HomeOut::Book { book_id: book.id }).ok();
                } else if let Some(author) = self.search_results.authors.first() {
                    sender.output(HomeOut::Search {
                        query: author.clone(),
                        inside_books: false,
                    }).ok();
                } else if let Some(series) = self.search_results.series.first() {
                    sender.output(HomeOut::Search {
                        query: series.clone(),
                        inside_books: false,
                    }).ok();
                } else if let Some((anno, _)) = self.search_results.quotes.first() {
                    sender.output(HomeOut::BookDialog {
                        book_id: anno.book_id,
                    }).ok();
                } else {
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
            HomeMsg::SearchHistoryDelete { query } => {
                self.search_history.borrow_mut().retain(|q| q != &query);
            }
            HomeMsg::SearchClear => {
                self.search_history.borrow_mut().clear();
                widgets.search_clear_btn.set_visible(false);
                clear_box(&widgets.search_history_host);
            }
            HomeMsg::SearchInsideBooksToggled(enabled) => {
                self.search_inside_books = enabled;
            }
            HomeMsg::SearchResults(results) => {
                self.search_results = results;
            }
        }

        // Import button / status (no #[watch] — updated explicitly here).
        widgets.add_btn.set_label(if self.importing {
            "Importing…"
        } else {
            "+ Add books"
        });
        widgets.add_btn.set_sensitive(!self.importing);
        widgets.status_label.set_label(&self.status);
        widgets.status_label.set_visible(!self.status.is_empty());

        // When there's a query, show result groups instead of recent searches.
        let has_query = !self.search_query.is_empty();
        widgets.search_history_heading.set_visible(!has_query);
        if has_query {
            widgets.search_clear_btn.set_visible(false);
            render_search_results(
                widgets,
                &sender,
                &self.search_results,
                &self.search_query,
                self.search_inside_books,
            );
        } else {
            // Update history rows.
            let has_history = !self.search_history.borrow().is_empty();
            widgets.search_clear_btn.set_visible(has_history);
            clear_box(&widgets.search_history_host);
            let hist = self.search_history.borrow();
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
                        .icon_name("edit-delete-symbolic")
                        .pixel_size(14)
                        .build())
                    .css_classes(["kalam-home-search-history-cross"])
                    .halign(gtk::Align::End)
                    .build();
                let s_hist = sender.clone();
                cross.connect_clicked(move |_| {
                    s_hist.input(HomeMsg::SearchHistoryDelete { query: q.clone() });
                });
                row.append(&cross);

                widgets.search_history_host.append(&row);
            }
        }
    }
}

/// Render grouped search results in the search box history host.
/// Render grouped search results in the search box history host.
/// Shows top 3 of each group with a "See all N" row.
fn render_search_results(
    widgets: &mut HomePageModelWidgets,
    sender: &ComponentSender<HomePageModel>,
    results: &SearchResults,
    query: &str,
    inside_books: bool,
) {
    clear_box(&widgets.search_history_host);

    if results.books.is_empty()
        && results.authors.is_empty()
        && results.series.is_empty()
        && results.quotes.is_empty()
    {
        // No results — show a quiet notice.
        let notice = gtk::Label::new(Some("No results"));
        notice.add_css_class("kalam-home-search-no-results");
        notice.set_halign(gtk::Align::Start);
        notice.set_margin_start(12);
        notice.set_margin_top(6);
        widgets.search_history_host.append(&notice);
        return;
    }

    // Books group.
    if !results.books.is_empty() {
        render_book_group(widgets, sender, &results.books, query, inside_books);
    }

    // Authors group.
    if !results.authors.is_empty() {
        render_string_group(
            widgets,
            sender,
            "Authors",
            &results.authors,
            query,
            inside_books,
        );
    }

    // Series group.
    if !results.series.is_empty() {
        render_string_group(
            widgets,
            sender,
            "Series",
            &results.series,
            query,
            inside_books,
        );
    }

    // Saved quotes group.
    if !results.quotes.is_empty() {
        render_quote_group(widgets, sender, &results.quotes, query, inside_books);
    }
}

/// Render the books group: heading, up to 3 rows, "See all N" if more.
fn render_book_group(
    widgets: &mut HomePageModelWidgets,
    sender: &ComponentSender<HomePageModel>,
    books: &[Book],
    query: &str,
    inside_books: bool,
) {
    let total = books.len();
    let show_all = total <= 3;
    let display: &[Book] = if show_all { books } else { &books[..3] };

    let h = gtk::Label::new(Some(&format!("Books · {total}")));
    h.add_css_class("kalam-home-search-heading");
    h.set_halign(gtk::Align::Start);
    h.set_margin_start(12);
    h.set_margin_top(6);
    h.set_margin_bottom(2);
    widgets.search_history_host.append(&h);

    for book in display {
        let row = result_row(
            widgets,
            &book.title,
            if book.authors.is_empty() { None } else { Some(&book.authors) },
        );
        widgets.search_history_host.append(&row);
    }

    if !show_all {
        let see_all = see_all_button(sender, &format!("See all {total}"), query, inside_books);
        widgets.search_history_host.append(&see_all);
    }
}

/// Render a group of plain strings (authors, series): heading, up to 3 rows,
/// "See all N" if more.
fn render_string_group<T: ToString>(
    widgets: &mut HomePageModelWidgets,
    sender: &ComponentSender<HomePageModel>,
    label: &str,
    items: &[T],
    query: &str,
    inside_books: bool,
) {
    let total = items.len();
    let show_all = total <= 3;
    let display: &[T] = if show_all { items } else { &items[..3] };

    let h = gtk::Label::new(Some(&format!("{label} · {total}")));
    h.add_css_class("kalam-home-search-heading");
    h.set_halign(gtk::Align::Start);
    h.set_margin_start(12);
    h.set_margin_top(6);
    h.set_margin_bottom(2);
    widgets.search_history_host.append(&h);

    for item in display {
        let row = result_row(widgets, item.to_string().as_str(), None);
        widgets.search_history_host.append(&row);
    }

    if !show_all {
        let see_all = see_all_button(sender, &format!("See all {total}"), query, inside_books);
        widgets.search_history_host.append(&see_all);
    }
}

/// Render the saved quotes group.
fn render_quote_group(
    widgets: &mut HomePageModelWidgets,
    sender: &ComponentSender<HomePageModel>,
    quotes: &[(Annotation, Option<Book>)],
    query: &str,
    inside_books: bool,
) {
    let total = quotes.len();
    let show_all = total <= 3;
    let display: &[(Annotation, Option<Book>)] = if show_all { quotes } else { &quotes[..3] };

    let h = gtk::Label::new(Some(&format!("Saved quotes · {total}")));
    h.add_css_class("kalam-home-search-heading");
    h.set_halign(gtk::Align::Start);
    h.set_margin_start(12);
    h.set_margin_top(6);
    h.set_margin_bottom(2);
    widgets.search_history_host.append(&h);

    for (anno, book) in display {
        let subtitle = book.as_ref().map(|b| b.title.as_str());
        let row = result_row(
            widgets,
            anno.text_excerpt.as_str(),
            subtitle,
        );
        widgets.search_history_host.append(&row);
    }

    if !show_all {
        let see_all = see_all_button(sender, &format!("See all {total}"), query, inside_books);
        widgets.search_history_host.append(&see_all);
    }
}

/// Build a single result row: title at the left, optional subtitle at the right.
fn result_row(
    widgets: &mut HomePageModelWidgets,
    title: &str,
    subtitle: Option<&str>,
) -> gtk::Box {
    let row = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(8)
        .margin_start(12)
        .margin_end(12)
        .margin_top(2)
        .margin_bottom(2)
        .valign(gtk::Align::Center)
        .hexpand(true)
        .build();
    row.add_css_class("kalam-home-search-result-row");

    let title_lbl = gtk::Label::builder()
        .label(title)
        .halign(gtk::Align::Start)
        .hexpand(true)
        .build();
    row.append(&title_lbl);

    if let Some(sub) = subtitle {
        let sub_lbl = gtk::Label::builder()
            .label(sub)
            .halign(gtk::Align::Start)
            .hexpand(false)
            .build();
        sub_lbl.add_css_class("kalam-home-search-result-sub");
        row.append(&sub_lbl);
    }

    row
}

/// "See all N" button for a result group. Clicking opens the full search page
/// with the current query.
fn see_all_button(
    sender: &ComponentSender<HomePageModel>,
    label: &str,
    query: &str,
    inside_books: bool,
) -> gtk::Button {
    let btn = gtk::Button::builder()
        .label(label)
        .halign(gtk::Align::Start)
        .hexpand(false)
        .margin_start(12)
        .margin_top(6)
        .margin_bottom(8)
        .build();
    btn.add_css_class("kalam-home-search-see-all");
    let s = sender.clone();
    let q = query.to_string();
    btn.connect_clicked(move |_| {
        let _ = s.output(HomeOut::Search {
            query: q.clone(),
            inside_books,
        });
    });
    btn
}

/// Search the library for the given query, grouped by type.
fn search_library(
    catalog: Arc<Catalog>,
    query: String,
    inside_books: bool,
    sender: &ComponentSender<HomePageModel>,
) {
    let service = LibraryService::new(catalog);
    let s = sender.input_sender().clone();
    crate::tasks::spawn_internal(
        "Home search",
        move |_reporter| {
            let mut results = SearchResults::default();

            if inside_books {
                let snap = service.search_content(&query);
                results.books = snap
                    .results
                    .into_iter()
                    .map(|r| r.book)
                    .collect();
            } else {
                let snap = service.all_books(SortKey::Title, &query);
                results.books = snap.books;
            }

            // Extract distinct authors from books.
            let mut authors: Vec<String> = results
                .books
                .iter()
                .flat_map(|b| b.authors.split(','))
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            authors.sort();
            authors.dedup();

            // Extract distinct series from books.
            let mut series: Vec<String> = results
                .books
                .iter()
                .filter_map(|b| b.series.as_deref())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            series.sort();
            series.dedup();

            // Search saved quotes.
            let quote_snap = service.quotes(&query);
            results.quotes = quote_snap.quotes;

            results
        },
        |_| {},
        move |results| {
            let _ = s.send(HomeMsg::SearchResults(results));
        },
    );
}

fn clear_box(host: &gtk::Box) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
}

/// Build a state card (§64, decision 194): dashed border, title, message.
fn state_card(title: &str, message: &str) -> gtk::Box {
    let card = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(8)
        .halign(gtk::Align::Center)
        .hexpand(false)
        .vexpand(false)
        .margin_top(24)
        .margin_bottom(24)
        .build();
    card.add_css_class("kalam-home-state-card");

    let title_lbl = gtk::Label::builder()
        .label(title)
        .halign(gtk::Align::Center)
        .hexpand(false)
        .build();
    title_lbl.add_css_class("kalam-home-state-title");
    card.append(&title_lbl);

    let msg_lbl = gtk::Label::builder()
        .label(message)
        .halign(gtk::Align::Center)
        .hexpand(false)
        .wrap(true)
        .max_width_chars(40)
        .build();
    msg_lbl.add_css_class("kalam-home-state-message");
    card.append(&msg_lbl);

    card
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
    _sender: &ComponentSender<HomePageModel>,
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
    clear_box(&widgets.home_book_host);
    clear_box(&widgets.home_updates_host);

    // ── State cards (§64, decision 194) ──
    // Shown instead of normal content when the library is in a special state.
    if !snap.errors.is_empty() {
        let card = state_card(
            "Error loading library",
            "Could not read some or all of your library. Try again or check your files.",
        );
        widgets.counts_host.append(&card);
    } else if snap.continue_reading.is_empty() && snap.recent.is_empty() {
        let card = state_card(
            "Your library is empty",
            "Add books to get started. Use \"+ Add books\" to import your first EPUB.",
        );
        widgets.counts_host.append(&card);
    } else {
        // Home book from continue_reading (§60, decision 188).
        if let Some(book) = snap.continue_reading.first() {
            let book_widget = build_home_book(book);
            widgets.home_book_host.append(&book_widget);
        }

        // Home updates (§62, decision 189): thin vertical timeline, four rows
        // visible, scrolls. Shown only when there is a saved source.
        if !snap.reading_list.is_empty() {
        let heading = gtk::Label::builder()
            .label("Updates")
            .halign(gtk::Align::Start)
            .hexpand(false)
            .build();
        heading.add_css_class("kalam-home-updates-heading");
        widgets.home_updates_host.append(&heading);

        let limit = snap.reading_list.len().min(4);
        for entry in snap.reading_list.iter().take(limit) {
            let row = gtk::Box::builder()
                .orientation(gtk::Orientation::Horizontal)
                .spacing(8)
                .halign(gtk::Align::Start)
                .hexpand(false)
                .margin_top(4)
                .margin_bottom(4)
                .build();
            row.add_css_class("kalam-home-update-row");

            let dot = gtk::Box::builder()
                .size_request(6, 6)
                .build();
            dot.add_css_class("kalam-home-update-dot");
            row.append(&dot);

            let lbl = gtk::Label::builder()
                .label(&entry.book_title)
                .halign(gtk::Align::Start)
                .hexpand(false)
                .build();
            lbl.add_css_class("kalam-home-update-label");
            row.append(&lbl);

            widgets.home_updates_host.append(&row);
        }
    }
    }
}

/// Build the Home book widget (§60, decision 188): cover page, reading page
/// with ribbon, play button, pager of 5 dots.
fn build_home_book(book: &Book) -> gtk::Box {
    let host = gtk::Box::builder()
        .orientation(gtk::Orientation::Vertical)
        .spacing(0)
        .build();
    host.add_css_class("kalam-home-book");

    let heading = gtk::Label::builder()
        .label("Continue reading")
        .halign(gtk::Align::Start)
        .hexpand(false)
        .build();
    heading.add_css_class("kalam-home-book-heading");
    host.append(&heading);

    // Book spread: cover + reading page.
    let spread = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(0)
        .build();
    spread.add_css_class("kalam-home-book-spread");

    // Cover page.
    let cover = gtk::Picture::builder()
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Start)
        .build();
    cover.add_css_class("kalam-home-book-cover");
    cover.set_size_request(200, 300);
    if let Some(cover_path) = &book.cover_path {
        cover.set_filename(Some(cover_path));
    }
    spread.append(&cover);

    // Reading page.
    let page = gtk::Picture::builder()
        .halign(gtk::Align::Start)
        .valign(gtk::Align::Start)
        .build();
    page.add_css_class("kalam-home-book-page");
    page.set_size_request(200, 300);
    if let Some(cover_path) = &book.cover_path {
        page.set_filename(Some(cover_path));
    }
    spread.append(&page);

    host.append(&spread);

    // Ribbon + play button + pager.
    let controls = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(12)
        .halign(gtk::Align::Center)
        .set_margin_top(8)
        .set_margin_bottom(8)
        .build();
    controls.add_css_class("kalam-home-book-controls");

    // Ribbon indicator.
    let ribbon = gtk::Image::builder()
        .icon_name("emblem-bookmarks-symbolic")
        .pixel_size(16)
        .build();
    ribbon.add_css_class("kalam-home-book-ribbon");
    controls.append(&ribbon);

    // Play button.
    let play = gtk::Button::builder()
        .label("Play")
        .build();
    play.add_css_class("kalam-home-book-play");
    controls.append(&play);

    // Pager dots (5 dots, first two active).
    let pager = gtk::Box::builder()
        .orientation(gtk::Orientation::Horizontal)
        .spacing(4)
        .build();
    pager.add_css_class("kalam-home-book-pager");
    for i in 0..5 {
        let dot = gtk::Box::builder()
            .size_request(8, 8)
            .build();
        dot.add_css_class("kalam-home-book-pager-dot");
        if i < 2 {
            dot.add_css_class("active");
        }
        pager.append(&dot);
    }
    controls.append(&pager);

    host.append(&controls);

    host
}
