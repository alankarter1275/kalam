//! Series page (series-page-spec.md, decisions 94–101).
//!
//! A full page on plain `bg`: a compact header with the segmented
//! progress, then the books as a cover strip (or the plain list, remembered
//! per series), a focused-book row, and the source line. It replaces the
//! retired series float; the comics series page is a different page and is
//! untouched.
//!
//! The listing comes from the service's merged snapshot: the cached Open
//! Library order with owned books matched into it. A cache miss still
//! fetches in the background (as the float did) and the ⋯ menu's refresh
//! re-fetches on request. There is no series-level summary or unowned-book
//! description in the cached listing, so those parts of the layout simply
//! stay empty rather than inventing copy.

use crate::db::{Catalog, SeriesWork};
use crate::models::BookFormat;
use crate::pages::history::pretty_day;
use crate::service::{LibraryService, SeriesEntrySnapshot, SeriesPageSnapshot};
use crate::widgets::book_row::cache_decoded_cover;
use crate::widgets::{
    cover_state::{book_cover, finished_badge, missing_cover, CoverAvailability},
    menu::{menu_button, menu_popover, MenuWidth},
    pill_button::{icon_button, primary_button},
    progress::{linear_progress, segmented_progress, SegmentState},
    segmented::{segmented_control, SegmentedSize},
    surface::{hairline, state_card},
};
use gtk::prelude::*;
use relm4::prelude::*;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

const FOCUSED_W: i32 = 260;
const FOCUSED_H: i32 = 390;
const COVER_W: i32 = 180;
const COVER_H: i32 = 270;
const STRIP_GAP: i32 = 28;
const LIST_THUMB_W: i32 = 36;
const LIST_THUMB_H: i32 = 50;

#[derive(Debug)]
pub enum SeriesPageOut {
    OpenBook { book_id: i64 },
    OpenReader { book_id: i64 },
    OpenAuthor { name: String },
    Back,
}

/// The page's construction inputs: the service seam and the series
/// identity (name + first author, as the cached listing is keyed).
pub struct SeriesPageInit {
    pub service: LibraryService,
    pub series: String,
    pub first_author: String,
}

/// What the snapshot worker delivers: the snapshot plus the covers it
/// decoded on the same thread, so the fill finds them cached (the book
/// page's 7.1 step 2a.2 pattern).
#[derive(Debug)]
struct PageLoad {
    snap: SeriesPageSnapshot,
    covers: Vec<crate::preload::DecodedCover>,
}

#[derive(Debug)]
pub enum SeriesPageMsg {
    Loaded(Box<PageLoad>),
    EntryClicked(usize),
    MoveFocus(i32),
    SetViewList(bool),
    OpenFocused,
    OpenContinue,
    OpenAuthor(String),
    RefreshListing,
    Fetched(Result<Vec<SeriesWork>, String>),
    ChapterCountLoaded { book_id: i64, count: usize },
    Back,
}

pub struct SeriesPageModel {
    service: LibraryService,
    series_name: String,
    requested_first_author: String,
    snapshot: Option<SeriesPageSnapshot>,
    loading: bool,
    focus: usize,
    view_list: bool,
    fetching: bool,
    fetch_manual: bool,
    auto_fetch_tried: bool,
    fetch_notice: Option<String>,
    chapter_counts: HashMap<i64, usize>,
    chapter_requests: HashSet<i64>,
    entry_buttons: Vec<gtk::Button>,
    /// Shared with the root key handler: while the ⋯ menu is open its
    /// keys (arrows, Esc) belong to the menu, not the page.
    menu_open: Rc<Cell<bool>>,
    strip_scroll: Option<gtk::ScrolledWindow>,
    strip_scroll_value: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryState {
    Missing,
    Read,
    InProgress,
    Unread,
}

fn entry_state(entry: &SeriesEntrySnapshot) -> EntryState {
    let Some(book) = &entry.book else {
        return EntryState::Missing;
    };
    if entry.finished_at.is_some() || book.progress >= 100 {
        EntryState::Read
    } else if book.progress > 0 || entry.chapter_index.is_some() {
        EntryState::InProgress
    } else {
        EntryState::Unread
    }
}

fn entry_fraction(entry: &SeriesEntrySnapshot) -> f64 {
    match entry_state(entry) {
        EntryState::Read => 1.0,
        EntryState::InProgress => f64::from(
            entry
                .book
                .as_ref()
                .map(|book| book.progress)
                .unwrap_or_default(),
        ) / 100.0,
        EntryState::Missing | EntryState::Unread => 0.0,
    }
}

/// The spec's opening focus: the next owned book not finished; when every
/// owned book is read, the last owned one.
fn default_focus(entries: &[SeriesEntrySnapshot]) -> usize {
    if let Some(index) = entries
        .iter()
        .position(|entry| entry.book.is_some() && entry_state(entry) != EntryState::Read)
    {
        return index;
    }
    entries
        .iter()
        .rposition(|entry| entry.book.is_some())
        .unwrap_or(0)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ContinueKind {
    Start,
    Continue,
    Again,
}

/// The Continue button's book and verb, in the same order the book page's
/// "Next in series" uses: the first owned unfinished volume. `Start` only
/// when nothing owned has been started; `Again` when all of it is read.
fn continue_target(entries: &[SeriesEntrySnapshot]) -> Option<(usize, ContinueKind)> {
    let owned: Vec<usize> = entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| entry.book.is_some())
        .map(|(index, _)| index)
        .collect();
    let first_owned = *owned.first()?;
    if let Some(index) = owned
        .iter()
        .copied()
        .find(|&index| entry_state(&entries[index]) != EntryState::Read)
    {
        let started = owned
            .iter()
            .any(|&index| entry_state(&entries[index]) != EntryState::Unread);
        return Some((
            index,
            if started {
                ContinueKind::Continue
            } else {
                ContinueKind::Start
            },
        ));
    }
    Some((first_owned, ContinueKind::Again))
}

fn format_series_index(idx: f32) -> String {
    if idx.fract().abs() < f32::EPSILON {
        format!("{}", idx as i64)
    } else {
        format!("{idx}")
    }
}

#[relm4::component(pub)]
impl Component for SeriesPageModel {
    type Init = SeriesPageInit;
    type Input = SeriesPageMsg;
    type Output = SeriesPageOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "kalam-seriespage",
            set_can_focus: true,
            set_hexpand: true,
            set_vexpand: false,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                add_css_class: "kalam-seriespage-inner",
                set_spacing: 0,

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 24,
                    set_valign: gtk::Align::Start,

                    #[name = "header_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 8,
                        set_size_request: (700, -1),
                        set_hexpand: false,
                        set_valign: gtk::Align::Start,
                    },

                    gtk::Box {
                        set_hexpand: true,
                    },

                    #[name = "actions_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 12,
                        set_margin_top: 20,
                        set_valign: gtk::Align::Start,
                    },
                },

                #[name = "books_host"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                },

                #[name = "content_host"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                },

                #[name = "focused_host"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                },

                #[name = "source_host"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let menu_open = Rc::new(Cell::new(false));
        let mut model = SeriesPageModel {
            service: init.service,
            series_name: init.series,
            requested_first_author: init.first_author,
            snapshot: None,
            loading: true,
            focus: 0,
            view_list: false,
            fetching: false,
            fetch_manual: false,
            auto_fetch_tried: false,
            fetch_notice: None,
            chapter_counts: HashMap::new(),
            chapter_requests: HashSet::new(),
            entry_buttons: Vec::new(),
            menu_open: menu_open.clone(),
            strip_scroll: None,
            strip_scroll_value: 0.0,
        };
        let widgets = view_output!();

        // Left/Right/Enter/Esc (decision 97). Bubble phase (the default):
        // a focused cover button gets Enter first and its own click does
        // the select-or-open, and a popover keeps its Esc — this handler
        // only sees what the focused widget did not consume.
        let keys = gtk::EventControllerKey::new();
        let key_sender = sender.clone();
        keys.connect_key_pressed(move |_, keyval, _, state| {
            use gtk::gdk::Key;
            if state.contains(gtk::gdk::ModifierType::CONTROL_MASK)
                || state.contains(gtk::gdk::ModifierType::ALT_MASK)
            {
                return gtk::glib::Propagation::Proceed;
            }
            if menu_open.get() {
                return gtk::glib::Propagation::Proceed;
            }
            match keyval {
                Key::Left => key_sender.input(SeriesPageMsg::MoveFocus(-1)),
                Key::Right => key_sender.input(SeriesPageMsg::MoveFocus(1)),
                Key::Return | Key::KP_Enter => key_sender.input(SeriesPageMsg::OpenFocused),
                Key::Escape => key_sender.input(SeriesPageMsg::Back),
                _ => return gtk::glib::Propagation::Proceed,
            }
            gtk::glib::Propagation::Stop
        });
        root.add_controller(keys);

        model.request_snapshot(&sender);
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
        self.capture_strip_scroll();
        let mut rebuild = true;
        let mut grab_entry = false;
        let mut grab_root = false;

        match msg {
            SeriesPageMsg::Loaded(load) => {
                let first_load = std::mem::replace(&mut self.loading, false);
                let PageLoad { snap, covers } = *load;
                for decoded in &covers {
                    cache_decoded_cover(decoded);
                }
                report_errors(&snap.errors);

                // Keep the reader's place across a fetch reload: the same
                // book (or the same listing title) stays focused when the
                // merged order shifts under it.
                let previous = self.snapshot.as_ref().and_then(|old| {
                    old.entries.get(self.focus).map(|entry| {
                        (
                            entry.book.as_ref().map(|book| book.id),
                            normalise_title(&entry.title),
                        )
                    })
                });
                self.focus = previous
                    .and_then(|(book_id, title)| {
                        if let Some(book_id) = book_id {
                            snap.entries
                                .iter()
                                .position(|entry| {
                                    entry.book.as_ref().is_some_and(|b| b.id == book_id)
                                })
                        } else {
                            snap.entries
                                .iter()
                                .position(|entry| normalise_title(&entry.title) == title)
                        }
                    })
                    .unwrap_or_else(|| default_focus(&snap.entries));

                if first_load {
                    // CI screenshot hook, the series counterpart of
                    // KALAM_DIALOG: `KALAM_SERIES_VIEW=list|strip` picks the
                    // opening view so the headless run can photograph both.
                    // It never writes the remembered pref.
                    self.view_list = match std::env::var("KALAM_SERIES_VIEW").as_deref() {
                        Ok("list") => true,
                        Ok("strip") => false,
                        _ => snap.view_list,
                    };
                    grab_root = true;
                }

                let needs_fetch = snap.listed_total.is_none();
                self.snapshot = Some(snap);
                if needs_fetch && !self.auto_fetch_tried && !self.fetching {
                    self.auto_fetch_tried = true;
                    self.start_fetch(&sender, false);
                }
                self.request_chapter_count(&sender);
            }
            SeriesPageMsg::EntryClicked(index) => {
                let clicked_book = self
                    .snapshot
                    .as_ref()
                    .and_then(|snap| snap.entries.get(index))
                    .and_then(|entry| entry.book.as_ref().map(|book| book.id));
                if index == self.focus {
                    if let Some(book_id) = clicked_book {
                        sender
                            .output(SeriesPageOut::OpenBook { book_id })
                            .ok();
                        rebuild = false;
                    }
                } else if self
                    .snapshot
                    .as_ref()
                    .is_some_and(|snap| index < snap.entries.len())
                {
                    self.focus = index;
                    grab_entry = true;
                    self.warm_focus_cover();
                    self.request_chapter_count(&sender);
                }
            }
            SeriesPageMsg::MoveFocus(delta) => {
                let len = self
                    .snapshot
                    .as_ref()
                    .map(|snap| snap.entries.len())
                    .unwrap_or(0);
                if len > 0 {
                    let next =
                        (self.focus as i32 + delta).clamp(0, len.saturating_sub(1) as i32)
                            as usize;
                    if next != self.focus {
                        self.focus = next;
                        grab_entry = true;
                        self.warm_focus_cover();
                        self.request_chapter_count(&sender);
                    }
                }
            }
            SeriesPageMsg::SetViewList(list) => {
                if self.view_list != list {
                    self.view_list = list;
                    if let Some(key) = self
                        .snapshot
                        .as_ref()
                        .map(|snap| snap.view_pref_key.clone())
                    {
                        self.service
                            .catalog()
                            .set_pref(&key, if list { "list" } else { "strip" });
                    }
                    self.warm_all_covers();
                    grab_root = true;
                }
            }
            SeriesPageMsg::OpenFocused => {
                if let Some(book_id) = self
                    .focused_entry()
                    .and_then(|entry| entry.book.as_ref().map(|book| book.id))
                {
                    sender.output(SeriesPageOut::OpenBook { book_id }).ok();
                }
                rebuild = false;
            }
            SeriesPageMsg::OpenContinue => {
                if let Some(book_id) = self.snapshot.as_ref().and_then(|snap| {
                    continue_target(&snap.entries).and_then(|(index, _)| {
                        snap.entries[index]
                            .book
                            .as_ref()
                            .map(|book| book.id)
                    })
                }) {
                    sender
                        .output(SeriesPageOut::OpenReader { book_id })
                        .ok();
                }
                rebuild = false;
            }
            SeriesPageMsg::OpenAuthor(name) => {
                sender.output(SeriesPageOut::OpenAuthor { name }).ok();
                rebuild = false;
            }
            SeriesPageMsg::RefreshListing => {
                self.auto_fetch_tried = true;
                self.start_fetch(&sender, true);
            }
            SeriesPageMsg::Fetched(result) => {
                self.fetching = false;
                match result {
                    Ok(works) if !works.is_empty() => {
                        self.fetch_notice = None;
                        self.request_snapshot(&sender);
                    }
                    Ok(_) => {
                        self.fetch_notice =
                            Some("Open Library doesn't list this series yet.".to_string());
                    }
                    Err(detail) => {
                        self.fetch_notice =
                            Some("Couldn't fetch the series listing.".to_string());
                        if self.fetch_manual {
                            crate::notify::error("Could not fetch the series", &detail);
                        }
                    }
                }
            }
            SeriesPageMsg::ChapterCountLoaded { book_id, count } => {
                self.chapter_requests.remove(&book_id);
                self.chapter_counts.insert(book_id, count);
                // Only the focused row prints a chapter count; refilling
                // just it keeps the strip (and its scroll/focus) intact.
                self.fill_focused(widgets, &sender);
                rebuild = false;
            }
            SeriesPageMsg::Back => {
                sender.output(SeriesPageOut::Back).ok();
                rebuild = false;
            }
        }

        if rebuild {
            self.rebuild(widgets, &sender);
            if grab_entry {
                if let Some(button) = self.entry_buttons.get(self.focus) {
                    button.grab_focus();
                }
            } else if grab_root {
                root.grab_focus();
            }
        }
    }
}

impl SeriesPageModel {
    fn focused_entry(&self) -> Option<&SeriesEntrySnapshot> {
        self.snapshot
            .as_ref()
            .and_then(|snap| snap.entries.get(self.focus))
    }

    fn request_snapshot(&self, sender: &ComponentSender<Self>) {
        let service = LibraryService::new(self.service.catalog().clone());
        let series = self.series_name.clone();
        let first_author = self.requested_first_author.clone();
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Reading series",
            move |_reporter| {
                let snap = service.series_page(&series, &first_author);
                let focus = default_focus(&snap.entries);
                let mut covers = Vec::new();
                for (index, entry) in snap.entries.iter().enumerate() {
                    let Some(book) = &entry.book else { continue };
                    let Some(path) = &book.cover_path else {
                        continue;
                    };
                    if snap.view_list {
                        if let Some(decoded) =
                            crate::preload::decode_for_cache(path, LIST_THUMB_W, LIST_THUMB_H)
                        {
                            covers.push(decoded);
                        }
                    } else {
                        if let Some(decoded) =
                            crate::preload::decode_for_cache(path, COVER_W, COVER_H)
                        {
                            covers.push(decoded);
                        }
                        if index == focus {
                            if let Some(decoded) =
                                crate::preload::decode_for_cache(path, FOCUSED_W, FOCUSED_H)
                            {
                                covers.push(decoded);
                            }
                        }
                    }
                }
                PageLoad { snap, covers }
            },
            |_| {},
            move |load| {
                let _ = s.send(SeriesPageMsg::Loaded(Box::new(load)));
            },
        );
    }

    fn start_fetch(&mut self, sender: &ComponentSender<Self>, manual: bool) {
        if self.fetching {
            return;
        }
        self.fetching = true;
        self.fetch_manual = manual;
        self.fetch_notice = None;

        let catalog = self.service.catalog().clone();
        let name = self.series_name.clone();
        let key = self
            .snapshot
            .as_ref()
            .map(|snap| snap.series_key.clone())
            .unwrap_or_else(|| {
                crate::db::series_key(&self.series_name, &self.requested_first_author)
            });
        let s = sender.input_sender().clone();
        crate::tasks::spawn(
            "Fetching series covers",
            move |_reporter| fetch_and_cache(&catalog, &name, &key),
            |_update| {},
            move |result| {
                let _ = s.send(SeriesPageMsg::Fetched(result));
            },
        );
    }

    /// The focused EPUB's chapter count, for "Chapter 9 of 31" — read on a
    /// worker like the book page's chapter titles, once per book.
    fn request_chapter_count(&mut self, sender: &ComponentSender<Self>) {
        let Some(entry) = self.focused_entry() else {
            return;
        };
        if entry_state(entry) != EntryState::InProgress {
            return;
        }
        let Some(book) = &entry.book else { return };
        if book.format != BookFormat::Epub
            || self.chapter_counts.contains_key(&book.id)
            || !self.chapter_requests.insert(book.id)
        {
            return;
        }
        let book_id = book.id;
        let path = book.file_path.clone();
        let cache = crate::paths::reader_cache_dir(&book.uuid);
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Reading chapter count",
            move |_reporter| {
                crate::epub_book::OpenBook::open(&path, &cache)
                    .map(|opened| opened.spine.len())
                    .unwrap_or(0)
            },
            |_| {},
            move |count| {
                let _ = s.send(SeriesPageMsg::ChapterCountLoaded { book_id, count });
            },
        );
    }

    fn warm_focus_cover(&self) {
        if self.view_list {
            return;
        }
        if let Some(path) = self
            .focused_entry()
            .and_then(|entry| entry.book.as_ref())
            .and_then(|book| book.cover_path.clone())
        {
            crate::preload::warm_covers(vec![path], FOCUSED_W, FOCUSED_H);
        }
    }

    fn warm_all_covers(&self) {
        let Some(snap) = &self.snapshot else { return };
        let paths: Vec<std::path::PathBuf> = snap
            .entries
            .iter()
            .filter_map(|entry| entry.book.as_ref())
            .filter_map(|book| book.cover_path.clone())
            .collect();
        if self.view_list {
            crate::preload::warm_covers(paths, LIST_THUMB_W, LIST_THUMB_H);
        } else {
            crate::preload::warm_covers(paths, COVER_W, COVER_H);
            self.warm_focus_cover();
        }
    }

    fn capture_strip_scroll(&mut self) {
        if let Some(scroll) = &self.strip_scroll {
            self.strip_scroll_value = scroll.hadjustment().value();
        }
    }

    fn schedule_strip_scroll(&self, scroll: &gtk::ScrolledWindow) {
        let scroll = scroll.clone();
        let focus = self.focus;
        let previous = self.strip_scroll_value;
        gtk::glib::idle_add_local_once(move || {
            let adjustment = scroll.hadjustment();
            let page = adjustment.page_size();
            if page <= 0.0 {
                return;
            }
            // Every cover before the focused one is unfocused-sized, so
            // the focused cover's left edge is a plain multiple.
            let left = focus as f64 * f64::from(COVER_W + STRIP_GAP);
            let right = left + f64::from(FOCUSED_W);
            let mut target = previous;
            if left < target {
                target = left;
            }
            if right > target + page {
                target = right - page;
            }
            let max = (adjustment.upper() - page).max(0.0);
            adjustment.set_value(target.clamp(0.0, max));
        });
    }

    fn rebuild(&mut self, widgets: &SeriesPageModelWidgets, sender: &ComponentSender<Self>) {
        let _span = crate::timing::measure("series_page_rebuild");
        self.fill_header(widgets, sender);
        self.fill_books_row(widgets, sender);
        self.fill_content(widgets, sender);
        self.fill_focused(widgets, sender);
        self.fill_source(widgets);
    }

    fn fill_header(&self, widgets: &SeriesPageModelWidgets, sender: &ComponentSender<Self>) {
        // A rebuild replaces the ⋯ button, so any open menu is gone.
        self.menu_open.set(false);
        clear_box(&widgets.header_host);
        clear_box(&widgets.actions_host);

        let label = gtk::Label::new(Some("SERIES"));
        label.add_css_class("kalam-seriespage-eyebrow");
        label.set_halign(gtk::Align::Start);
        label.set_xalign(0.0);
        widgets.header_host.append(&label);

        let title = gtk::Label::new(Some(&self.series_name));
        title.add_css_class("kalam-seriespage-title");
        title.set_halign(gtk::Align::Start);
        title.set_xalign(0.0);
        title.set_wrap(true);
        title.set_lines(2);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_max_width_chars(30);
        widgets.header_host.append(&title);

        let author = self
            .snapshot
            .as_ref()
            .map(|snap| snap.first_author.clone())
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| self.requested_first_author.clone());
        if !author.trim().is_empty() {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.set_halign(gtk::Align::Start);
            let link = gtk::Button::with_label(&author);
            link.add_css_class("kalam-seriespage-author");
            link.set_halign(gtk::Align::Start);
            link.set_tooltip_text(Some(&format!("Open {author}'s page")));
            let s = sender.clone();
            link.connect_clicked(move |_| {
                s.input(SeriesPageMsg::OpenAuthor(author.clone()));
            });
            row.append(&link);

            if let Some(years) = self.snapshot.as_ref().and_then(year_range) {
                let facts = gtk::Label::new(Some(&format!("· {years}")));
                facts.add_css_class("kalam-ui-meta");
                facts.set_valign(gtk::Align::Center);
                row.append(&facts);
            }
            widgets.header_host.append(&row);
        }

        let Some(snap) = &self.snapshot else {
            let loading = gtk::Label::new(Some("Loading…"));
            loading.add_css_class("kalam-ui-meta");
            loading.set_halign(gtk::Align::Start);
            widgets.header_host.append(&loading);
            return;
        };

        if !snap.entries.is_empty() {
            let segments: Vec<SegmentState> = snap
                .entries
                .iter()
                .map(|entry| match entry_state(entry) {
                    EntryState::Read => SegmentState::Read,
                    EntryState::InProgress => SegmentState::InProgress(entry_fraction(entry)),
                    EntryState::Unread => SegmentState::Unread,
                    EntryState::Missing => SegmentState::Missing,
                })
                .collect();
            let bar = segmented_progress(&segments, 220);
            bar.set_margin_top(10);
            widgets.header_host.append(&bar);

            let line = gtk::Label::new(Some(&progress_line(snap)));
            line.add_css_class("kalam-ui-meta");
            line.set_halign(gtk::Align::Start);
            line.set_xalign(0.0);
            widgets.header_host.append(&line);
        }

        // No series summary exists in the listing data; the header simply
        // ends at the progress line instead of repeating a book blurb.

        if let Some((index, kind)) = continue_target(&snap.entries) {
            let entry = &snap.entries[index];
            let position = format_series_index(entry.position);
            let text = match kind {
                ContinueKind::Start => format!("Start Book {position}"),
                ContinueKind::Continue => format!("Continue Book {position}"),
                ContinueKind::Again => format!("Read Book {position} again"),
            };
            let s = sender.clone();
            let button = primary_button(&text, move || {
                s.input(SeriesPageMsg::OpenContinue);
            });
            button.set_size_request(-1, 44);
            button.set_tooltip_text(Some(&format!("Open {} in the reader", entry.title)));
            widgets.actions_host.append(&button);
        }

        let menu = menu_popover(MenuWidth::Short);
        let s = sender.clone();
        menu.add_item(
            if snap.listed_total.is_some() {
                "Refresh from Open Library"
            } else {
                "Fetch from Open Library"
            },
            move || {
                s.input(SeriesPageMsg::RefreshListing);
            },
        );
        let more = menu_button("view-more-symbolic", "Series actions", menu);
        more.add_css_class("kalam-seriespage-more");
        more.set_size_request(44, 44);
        more.set_sensitive(!self.fetching);
        let menu_open = self.menu_open.clone();
        more.connect_active_notify(move |button| {
            menu_open.set(button.is_active());
        });
        set_accessible_label(&more, "Series actions");
        widgets.actions_host.append(&more);
    }

    fn fill_books_row(&self, widgets: &SeriesPageModelWidgets, sender: &ComponentSender<Self>) {
        clear_box(&widgets.books_host);
        let Some(snap) = &self.snapshot else { return };
        if snap.entries.is_empty() {
            return;
        }

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.set_margin_top(36);
        row.set_valign(gtk::Align::Center);

        let label = gtk::Label::new(Some("BOOKS"));
        label.add_css_class("kalam-seriespage-books-label");
        label.set_halign(gtk::Align::Start);
        label.set_valign(gtk::Align::Center);
        row.append(&label);

        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        row.append(&spacer);

        let s = sender.clone();
        let switch = segmented_control(
            &["Strip", "List"],
            usize::from(self.view_list),
            SegmentedSize::Small,
            move |index| {
                s.input(SeriesPageMsg::SetViewList(index == 1));
            },
        );
        row.append(&switch.widget());
        widgets.books_host.append(&row);
    }

    fn fill_content(&mut self, widgets: &SeriesPageModelWidgets, sender: &ComponentSender<Self>) {
        clear_box(&widgets.content_host);
        self.entry_buttons.clear();
        self.strip_scroll = None;

        let Some(snap) = &self.snapshot else {
            let wrap = gtk::Box::new(gtk::Orientation::Vertical, 12);
            wrap.set_margin_top(48);
            wrap.set_halign(gtk::Align::Center);
            let spinner = gtk::Spinner::new();
            spinner.set_spinning(true);
            spinner.set_size_request(28, 28);
            spinner.set_halign(gtk::Align::Center);
            wrap.append(&spinner);
            let label = gtk::Label::new(Some("Loading the series…"));
            label.add_css_class("kalam-ui-meta");
            wrap.append(&label);
            widgets.content_host.append(&wrap);
            return;
        };

        if snap.entries.is_empty() {
            let card = state_card(
                "No books found",
                "This series has no books in your library yet.",
            );
            card.set_margin_top(36);
            card.set_halign(gtk::Align::Start);
            card.set_size_request(420, -1);
            widgets.content_host.append(&card);
            return;
        }

        if self.view_list {
            self.fill_list(widgets, sender);
        } else {
            self.fill_strip(widgets, sender);
        }
    }

    fn fill_strip(&mut self, widgets: &SeriesPageModelWidgets, sender: &ComponentSender<Self>) {
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .overlay_scrolling(true)
            .hexpand(true)
            .build();
        scroll.set_margin_top(16);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, STRIP_GAP as u32);
        row.add_css_class("kalam-series-strip");
        row.set_halign(gtk::Align::Start);
        row.set_valign(gtk::Align::Start);

        let mut buttons: Vec<gtk::Button> = Vec::new();
        if let Some(snap) = &self.snapshot {
            for (index, entry) in snap.entries.iter().enumerate() {
                let focused = index == self.focus;
                let (width, height) = if focused {
                    (FOCUSED_W, FOCUSED_H)
                } else {
                    (COVER_W, COVER_H)
                };

                let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
                column.set_size_request(width, -1);
                column.set_hexpand(false);
                column.set_valign(gtk::Align::Start);

                // A fixed 390 slot bottom-aligns every cover, focused or
                // not, and keeps the titles on one baseline.
                let slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
                slot.set_size_request(width, FOCUSED_H);

                let button = gtk::Button::new();
                button.add_css_class("kalam-seriespage-cover-button");
                button.set_size_request(width, height);
                button.set_halign(gtk::Align::Start);
                button.set_valign(gtk::Align::End);
                button.set_can_focus(true);
                button.set_child(Some(&strip_cover(entry, width, height)));
                let action = entry_action(entry, focused);
                button.set_tooltip_text(Some(&action));
                set_accessible_label(&button, &action);
                let s = sender.clone();
                button.connect_clicked(move |_| {
                    s.input(SeriesPageMsg::EntryClicked(index));
                });
                slot.append(&button);
                buttons.push(button);
                column.append(&slot);

                let progress_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
                progress_host.set_size_request(width, 3);
                progress_host.set_margin_top(10);
                if entry_state(entry) == EntryState::InProgress {
                    let bar = linear_progress(entry_fraction(entry));
                    bar.set_size_request(width, 3);
                    bar.set_hexpand(false);
                    progress_host.append(&bar);
                }
                column.append(&progress_host);

                let title = gtk::Label::new(Some(&entry.title));
                title.add_css_class("kalam-seriespage-strip-title");
                if focused {
                    title.add_css_class("focused");
                }
                title.set_halign(gtk::Align::Start);
                title.set_xalign(0.0);
                title.set_wrap(true);
                title.set_lines(2);
                title.set_ellipsize(gtk::pango::EllipsizeMode::End);
                title.set_max_width_chars(if focused { 30 } else { 21 });
                title.set_margin_top(8);
                column.append(&title);

                if let Some(year) = entry.year {
                    let year_label = gtk::Label::new(Some(&year.to_string()));
                    year_label.add_css_class("kalam-ui-meta");
                    year_label.set_halign(gtk::Align::Start);
                    year_label.set_xalign(0.0);
                    year_label.set_margin_top(2);
                    column.append(&year_label);
                }

                row.append(&column);
            }
        }

        scroll.set_child(Some(&row));
        widgets.content_host.append(&scroll);
        self.entry_buttons = buttons;
        self.strip_scroll = Some(scroll.clone());
        self.schedule_strip_scroll(&scroll);
    }

    fn fill_list(&mut self, widgets: &SeriesPageModelWidgets, sender: &ComponentSender<Self>) {
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        list.add_css_class("kalam-series-list-view");
        list.set_margin_top(16);

        let mut buttons: Vec<gtk::Button> = Vec::new();
        if let Some(snap) = &self.snapshot {
            for (index, entry) in snap.entries.iter().enumerate() {
                let focused = index == self.focus;
                let state = entry_state(entry);

                // The wrapper reaches 14px into the page margin so the
                // current row's accent bar sits left of the row proper
                // while every thumbnail stays on the margin line.
                let wrapper = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                wrapper.set_margin_start(-14);
                wrapper.set_hexpand(true);
                if state == EntryState::Missing {
                    wrapper.set_opacity(0.55);
                }

                let slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                slot.set_size_request(14, 64);
                if focused {
                    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                    bar.add_css_class("kalam-seriespage-current-bar");
                    bar.set_size_request(3, 32);
                    bar.set_halign(gtk::Align::Start);
                    bar.set_valign(gtk::Align::Center);
                    slot.append(&bar);
                }
                wrapper.append(&slot);

                let button = gtk::Button::new();
                button.add_css_class("kalam-seriespage-list-row");
                button.set_size_request(-1, 64);
                button.set_hexpand(true);
                button.set_can_focus(true);

                let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
                row.set_valign(gtk::Align::Center);

                let thumb = list_cover(entry);
                thumb.set_valign(gtk::Align::Center);
                row.append(&thumb);

                let position = gtk::Label::new(Some(&format!(
                    "Book {}",
                    format_series_index(entry.position)
                )));
                position.add_css_class("kalam-ui-meta");
                position.set_size_request(56, -1);
                position.set_halign(gtk::Align::Start);
                position.set_xalign(0.0);
                position.set_valign(gtk::Align::Center);
                row.append(&position);

                let title_year = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                title_year.set_hexpand(true);
                title_year.set_valign(gtk::Align::Center);
                let title = gtk::Label::new(Some(&entry.title));
                title.add_css_class("kalam-seriespage-list-title");
                title.add_css_class(if focused {
                    "current"
                } else if state == EntryState::Read {
                    "read"
                } else {
                    "unread"
                });
                title.set_halign(gtk::Align::Start);
                title.set_xalign(0.0);
                title.set_ellipsize(gtk::pango::EllipsizeMode::End);
                title.set_max_width_chars(56);
                title_year.append(&title);
                if let Some(year) = entry.year {
                    let year_label = gtk::Label::new(Some(&year.to_string()));
                    year_label.add_css_class("kalam-ui-meta");
                    year_label.set_valign(gtk::Align::Center);
                    title_year.append(&year_label);
                }
                row.append(&title_year);

                let state_host = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                state_host.set_halign(gtk::Align::End);
                state_host.set_valign(gtk::Align::Center);
                match state {
                    EntryState::Read => {
                        state_host.append(&finished_badge());
                        let text = gtk::Label::new(Some("Read"));
                        text.add_css_class("kalam-ui-meta");
                        text.set_valign(gtk::Align::Center);
                        state_host.append(&text);
                    }
                    EntryState::InProgress => {
                        let bar_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
                        bar_host.set_size_request(100, 3);
                        bar_host.set_valign(gtk::Align::Center);
                        let bar = linear_progress(entry_fraction(entry));
                        bar.set_size_request(100, 3);
                        bar.set_hexpand(false);
                        bar_host.append(&bar);
                        state_host.append(&bar_host);

                        let pct = entry
                            .book
                            .as_ref()
                            .map(|book| book.progress)
                            .unwrap_or_default();
                        let text = gtk::Label::new(Some(&format!("{pct}%")));
                        text.add_css_class("kalam-ui-meta");
                        text.set_valign(gtk::Align::Center);
                        state_host.append(&text);
                    }
                    EntryState::Unread => {}
                    EntryState::Missing => {
                        let text = gtk::Label::new(Some("Not in library"));
                        text.add_css_class("kalam-ui-meta");
                        text.set_valign(gtk::Align::Center);
                        state_host.append(&text);
                    }
                }
                row.append(&state_host);

                button.set_child(Some(&row));
                let action = entry_action(entry, focused);
                button.set_tooltip_text(Some(&action));
                set_accessible_label(
                    &button,
                    &format!(
                        "Book {}: {}, {}",
                        format_series_index(entry.position),
                        entry.title,
                        state_word(state)
                    ),
                );
                let s = sender.clone();
                button.connect_clicked(move |_| {
                    s.input(SeriesPageMsg::EntryClicked(index));
                });
                buttons.push(button.clone());
                wrapper.append(&button);
                list.append(&wrapper);
                list.append(&hairline());
            }
        }

        widgets.content_host.append(&list);
        self.entry_buttons = buttons;
    }

    fn fill_focused(&self, widgets: &SeriesPageModelWidgets, sender: &ComponentSender<Self>) {
        clear_box(&widgets.focused_host);
        let Some(snap) = &self.snapshot else { return };
        let Some(entry) = snap.entries.get(self.focus) else {
            return;
        };

        let rule = hairline();
        rule.set_margin_top(28);
        widgets.focused_host.append(&rule);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 48);
        row.set_margin_top(20);
        row.set_valign(gtk::Align::Start);

        let left = gtk::Box::new(gtk::Orientation::Vertical, 8);
        left.set_size_request(400, -1);
        left.set_hexpand(false);
        left.set_valign(gtk::Align::Start);

        let meta_text = match entry.year {
            Some(year) => format!(
                "BOOK {} · {}",
                format_series_index(entry.position),
                year
            ),
            None => format!("BOOK {}", format_series_index(entry.position)),
        };
        let meta = gtk::Label::new(Some(&meta_text));
        meta.add_css_class("kalam-seriespage-focused-meta");
        meta.set_halign(gtk::Align::Start);
        meta.set_xalign(0.0);
        left.append(&meta);

        let title = gtk::Label::new(Some(&entry.title));
        title.add_css_class("kalam-seriespage-focused-title");
        title.set_halign(gtk::Align::Start);
        title.set_xalign(0.0);
        title.set_wrap(true);
        title.set_lines(2);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_max_width_chars(34);
        left.append(&title);

        let state_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        state_row.set_halign(gtk::Align::Start);
        state_row.set_valign(gtk::Align::Center);
        match entry_state(entry) {
            EntryState::InProgress => {
                let bar_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
                bar_host.set_size_request(240, 3);
                bar_host.set_valign(gtk::Align::Center);
                let bar = linear_progress(entry_fraction(entry));
                bar.set_size_request(240, 3);
                bar.set_hexpand(false);
                bar_host.append(&bar);
                state_row.append(&bar_host);

                let text = gtk::Label::new(Some(&self.in_progress_text(entry)));
                text.add_css_class("kalam-ui-meta");
                text.set_valign(gtk::Align::Center);
                state_row.append(&text);
            }
            EntryState::Read => {
                state_row.append(&finished_badge());
                let text = gtk::Label::new(Some(&match &entry.finished_at {
                    Some(iso) => format!("Finished {}", pretty_date(iso)),
                    None => "Finished".to_string(),
                }));
                text.add_css_class("kalam-ui-meta");
                text.set_valign(gtk::Align::Center);
                state_row.append(&text);
            }
            EntryState::Unread => {
                let text = gtk::Label::new(Some("Unread"));
                text.add_css_class("kalam-ui-meta");
                state_row.append(&text);
            }
            EntryState::Missing => {
                let text = gtk::Label::new(Some("Not in library"));
                text.add_css_class("kalam-ui-meta");
                state_row.append(&text);
            }
        }
        left.append(&state_row);
        row.append(&left);

        // The description is the owned book's own; the listing carries no
        // blurb for a missing volume, so that column simply stays empty.
        if let Some(description) = entry
            .book
            .as_ref()
            .map(|book| book.description.trim())
            .filter(|text| !text.is_empty())
        {
            let label = gtk::Label::new(Some(description));
            label.add_css_class("kalam-seriespage-description");
            label.set_size_request(520, -1);
            label.set_hexpand(false);
            label.set_halign(gtk::Align::Start);
            label.set_xalign(0.0);
            label.set_valign(gtk::Align::Start);
            label.set_wrap(true);
            label.set_lines(2);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            label.set_max_width_chars(72);
            row.append(&label);
        }

        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        row.append(&spacer);

        let controls = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        controls.set_halign(gtk::Align::End);
        controls.set_valign(gtk::Align::Center);

        let s = sender.clone();
        let prev = icon_button("go-previous-symbolic", "Previous book", 36, true, move || {
            s.input(SeriesPageMsg::MoveFocus(-1));
        });
        prev.set_sensitive(self.focus > 0);
        set_accessible_label(&prev, "Previous book");
        controls.append(&prev);

        let counter = gtk::Label::new(Some(&format!(
            "{} / {}",
            self.focus + 1,
            snap.entries.len()
        )));
        counter.add_css_class("kalam-ui-meta");
        counter.set_valign(gtk::Align::Center);
        controls.append(&counter);

        let s = sender.clone();
        let next = icon_button("go-next-symbolic", "Next book", 36, true, move || {
            s.input(SeriesPageMsg::MoveFocus(1));
        });
        next.set_sensitive(self.focus + 1 < snap.entries.len());
        set_accessible_label(&next, "Next book");
        controls.append(&next);

        row.append(&controls);
        widgets.focused_host.append(&row);
    }

    fn in_progress_text(&self, entry: &SeriesEntrySnapshot) -> String {
        let Some(book) = &entry.book else {
            return String::new();
        };
        let pct = book.progress;
        let Some(chapter) = entry.chapter_index else {
            return format!("{pct}%");
        };
        match self
            .chapter_counts
            .get(&book.id)
            .copied()
            .filter(|count| *count > 0)
        {
            Some(count) => format!(
                "{pct}% · Chapter {} of {}",
                (chapter + 1).min(count),
                count
            ),
            None => format!("{pct}% · Chapter {}", chapter + 1),
        }
    }

    fn fill_source(&self, widgets: &SeriesPageModelWidgets) {
        clear_box(&widgets.source_host);
        let Some(snap) = &self.snapshot else { return };

        let text = if self.fetching {
            "Fetching the series listing from Open Library…".to_string()
        } else if let Some(fetched_at) = &snap.fetched_at {
            let source = match snap.source.as_deref() {
                Some("openlibrary") | None => "Open Library",
                Some(other) => other,
            };
            let mut line =
                format!("Series listing from {source} · Fetched {}", pretty_date(fetched_at));
            if let Some(notice) = &self.fetch_notice {
                line.push_str(&format!(" · {notice}"));
            }
            line
        } else {
            match &self.fetch_notice {
                Some(notice) => format!("Local library · {notice}"),
                None => "Local library".to_string(),
            }
        };

        let label = gtk::Label::new(Some(&text));
        label.add_css_class("kalam-ui-caption");
        label.set_halign(gtk::Align::Start);
        label.set_xalign(0.0);
        label.set_wrap(true);
        label.set_margin_top(24);
        label.set_margin_bottom(8);
        widgets.source_host.append(&label);
    }
}

fn strip_cover(entry: &SeriesEntrySnapshot, width: i32, height: i32) -> gtk::Widget {
    match &entry.book {
        Some(book) => book_cover(
            book,
            width,
            height,
            entry_state(entry) == EntryState::Read,
            CoverAvailability::Available,
            true,
        ),
        None => missing_cover(&entry.title, width, height).upcast(),
    }
}

fn list_cover(entry: &SeriesEntrySnapshot) -> gtk::Widget {
    let cover = match &entry.book {
        Some(book) => book_cover(
            book,
            LIST_THUMB_W,
            LIST_THUMB_H,
            false,
            CoverAvailability::Available,
            true,
        ),
        None => missing_cover(&entry.title, LIST_THUMB_W, LIST_THUMB_H).upcast(),
    };
    cover.add_css_class("kalam-seriespage-list-thumb");
    cover
}

fn entry_action(entry: &SeriesEntrySnapshot, focused: bool) -> String {
    if entry.book.is_none() {
        format!("Show {}, not in library", entry.title)
    } else if focused {
        format!("Open {}", entry.title)
    } else {
        format!("Show {}", entry.title)
    }
}

fn state_word(state: EntryState) -> &'static str {
    match state {
        EntryState::Missing => "not in library",
        EntryState::Read => "read",
        EntryState::InProgress => "in progress",
        EntryState::Unread => "unread",
    }
}

fn progress_line(snap: &SeriesPageSnapshot) -> String {
    let owned = snap
        .entries
        .iter()
        .filter(|entry| entry.book.is_some())
        .count();
    let read = snap
        .entries
        .iter()
        .filter(|entry| entry_state(entry) == EntryState::Read)
        .count();
    let books = |count: usize| if count == 1 { "book" } else { "books" };
    match snap.listed_total {
        Some(total) if total > 0 && read == owned && owned == total => {
            format!("{read} of {total} {} read · Series complete", books(total))
        }
        Some(total) => format!(
            "{read} of {owned} {} read · You own {owned} of {total}",
            books(owned)
        ),
        None => format!("{read} of {owned} {} read", books(owned)),
    }
}

fn year_range(snap: &SeriesPageSnapshot) -> Option<String> {
    let mut years: Vec<i64> = snap.entries.iter().filter_map(|entry| entry.year).collect();
    years.sort_unstable();
    match (years.first(), years.last()) {
        (Some(first), Some(last)) if first == last => Some(first.to_string()),
        (Some(first), Some(last)) => Some(format!("{first}–{last}")),
        _ => None,
    }
}

fn pretty_date(iso: &str) -> String {
    if iso.len() >= 10 {
        pretty_day(&iso[..10])
    } else {
        iso.to_string()
    }
}

fn normalise_title(title: &str) -> String {
    title
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

fn set_accessible_label(widget: &impl IsA<gtk::Accessible>, label: &str) {
    widget.update_property(&[gtk::accessible::Property::Label(label)]);
}

fn clear_box(host: &gtk::Box) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
}

fn report_errors(errors: &[String]) {
    for err in errors {
        crate::notify::error("Could not read this series", err);
    }
}

/// Search, download covers, and cache the listing. Runs off the main
/// thread — the retired float's worker, unchanged in behaviour.
fn fetch_and_cache(
    catalog: &Catalog,
    series_name: &str,
    series_key: &str,
) -> Result<Vec<SeriesWork>, String> {
    let remote = crate::metadata::series::search_series(series_name)
        .map_err(|err| err.to_string())?;

    let works: Vec<SeriesWork> = remote
        .into_iter()
        .map(|work| {
            // A failed cover download must not kill the whole listing.
            let cover_path = work
                .cover_i
                .map(crate::metadata::series::download_cover)
                .transpose()
                .ok()
                .flatten()
                .map(|path| path.to_string_lossy().into_owned())
                .unwrap_or_default();
            SeriesWork {
                title: work.title,
                key: work.key,
                cover_path,
                year: work.year,
                author: work.author,
            }
        })
        .collect();

    // Only a real listing is worth caching.
    if !works.is_empty() {
        catalog
            .upsert_series_cache(series_key, "openlibrary", &works)
            .map_err(|err| format!("Could not cache the listing: {err}"))?;
    }
    Ok(works)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::Book;

    fn entry(title: &str, position: f32, book: Option<Book>) -> SeriesEntrySnapshot {
        SeriesEntrySnapshot {
            title: title.into(),
            position,
            year: None,
            book,
            finished_at: None,
            chapter_index: None,
        }
    }

    fn book(id: i64, progress: u8) -> Book {
        Book {
            id,
            uuid: String::new(),
            title: format!("Book {id}"),
            authors: String::new(),
            series: None,
            description: String::new(),
            format: BookFormat::Epub,
            file_name: String::new(),
            file_hash: String::new(),
            cover_name: None,
            added_at: String::new(),
            progress,
            rating: 0,
            publisher: String::new(),
            published: String::new(),
            series_index: 0.0,
            tags: Vec::new(),
            cover_path: None,
            file_path: std::path::PathBuf::new(),
        }
    }

    #[test]
    fn default_focus_skips_missing_and_finished() {
        let entries = vec![
            entry("Gap", 1.0, None),
            entry("Done", 2.0, Some(book(1, 100))),
            entry("Next", 3.0, Some(book(2, 0))),
        ];
        assert_eq!(default_focus(&entries), 2);
    }

    #[test]
    fn continue_verbs_follow_owned_progress() {
        let fresh = vec![
            entry("One", 1.0, Some(book(1, 0))),
            entry("Two", 2.0, Some(book(2, 0))),
        ];
        assert_eq!(continue_target(&fresh), Some((0, ContinueKind::Start)));

        let underway = vec![
            entry("One", 1.0, Some(book(1, 100))),
            entry("Two", 2.0, Some(book(2, 0))),
        ];
        assert_eq!(
            continue_target(&underway),
            Some((1, ContinueKind::Continue))
        );

        let done = vec![
            entry("Gap", 1.0, None),
            entry("One", 2.0, Some(book(1, 100))),
        ];
        assert_eq!(continue_target(&done), Some((1, ContinueKind::Again)));
        assert_eq!(default_focus(&done), 1);
    }
}
