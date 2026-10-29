use crate::db::Catalog;
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
    /// Search inside books switch toggled.
    SearchInsideBooksToggled(bool),
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
            HomeMsg::SearchInsideBooksToggled(enabled) => {
                self.search_inside_books = enabled;
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
                s_hist.input(HomeMsg::SearchHistoryItem { query: q.clone() });
            });
            row.append(&cross);

            widgets.search_history_host.append(&row);
        }
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
}
