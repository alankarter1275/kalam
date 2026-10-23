//! Author page (author-page-spec.md, decisions 104–117).
//!
//! A scrolling page on plain `bg`: header (label, name, bio, pills) with
//! the outlined facts panel on the right, the author's works as series
//! cards under In library / Not in library headings, an open-series panel
//! overlaid under its card, a sideways highlights row, and the
//! bibliography footer. The bibliography is whatever Find metadata last
//! cached — opening the page never touches the network (decision 110).
//!
//! The grouping (cached bibliography merged with owned books) lives in
//! `LibraryService::author_page`; this file only renders it. Dialogs:
//! Create shelf (§31) and the author-scoped Annotations dialog
//! (decision 115) at the bottom of the file.

use crate::author::{self, display_author_name, initials};
use crate::db::{Annotation, AuthorProfile, Catalog, HighlightColor, ShelfKind};
use crate::models::{Book, BookFormat};
use crate::pages::history::pretty_day;
use crate::service::{
    AuthorEntrySnapshot, AuthorGroupSnapshot, AuthorPageSnapshot, LibraryService,
    AUTHOR_SHELF_PREF_PREFIX,
};
use crate::shelf_rules::{Rule, RuleField, RuleOp, RuleSet};
use crate::widgets::book_row::{cache_decoded_cover, cover_widget};
use crate::widgets::{
    chip::{filter_chip, ChipSize},
    cover_state::{book_cover, missing_cover, missing_cover_with_disabled_fetch, CoverAvailability},
    dialog::{dialog_shell, present_dialog, text_field, DialogSize},
    in_app_dialog::{DialogExit, InAppDialog},
    menu::{menu_button, menu_popover, MenuWidth},
    pill_button::{boxed_ghost_button, icon_button, primary_button},
    progress::{linear_progress, segmented_progress, SegmentState},
    segmented::{segmented_control, SegmentedSize},
    surface::hairline,
};
use gtk::prelude::*;
use relm4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;

const FAN_W: i32 = 56;
const FAN_H: i32 = 84;
const PANEL_COVER_W: i32 = 180;
const PANEL_COVER_H: i32 = 270;
const HL_COVER_W: i32 = 72;
const HL_COVER_H: i32 = 108;
const PORTRAIT: i32 = 64;

#[derive(Debug)]
pub enum AuthorPageOut {
    OpenBook { book_id: i64 },
    /// A comic series card — chapters are only ever seen on the series page.
    ComicSeries { series_name: String },
    OpenSeries { series: String, first_author: String },
    OpenShelf { shelf_id: i64 },
    OpenReaderAt { book_id: i64, chapter: usize },
    ShowInLibrary { query: String },
}

#[derive(Debug)]
pub enum AuthorPageMsg {
    /// Re-read the snapshot (first load, and after any write).
    Refresh,
    FindMetadata,
    // Boxed: AuthorProfile is ~376 bytes, so an unboxed variant made every
    // AuthorPageMsg that large -- including the far more frequent Refresh.
    Fetched(Box<Result<AuthorProfile, String>>),
    /// The page's data arrived from the worker (7.1 step 2c): profile,
    /// owned books, the series grouping — and the covers the worker
    /// decoded for the fill. Boxed with `Fetched` for the same size
    /// reason.
    Loaded(Box<AuthorPageLoad>),
    ToggleGroup(usize),
    ClosePanel,
    OpenCreateShelf,
    OpenAnnotations,
    AddAllToReadingList,
    ExportHighlights,
}

/// The snapshot plus the covers its worker decoded for the fill — the
/// book page's `PageLoad` recipe (7.1 step 2a.2). The fill builds its
/// covers with deferred/synchronous cache lookups, and the heaviest of
/// them (the author photo) lives in the authors dir, outside the
/// library, so no thumbnail exists for it: decoding it on the UI thread
/// was the fill's whole weight (323 ms in field runs #2–#5). The worker
/// decodes them instead; the handler caches them; the fill then takes
/// its "already decoded" branch everywhere.
#[derive(Debug)]
pub struct AuthorPageLoad {
    snap: AuthorPageSnapshot,
    covers: Vec<crate::preload::DecodedCover>,
}

pub struct AuthorPageModel {
    service: LibraryService,
    requested_name: String,
    snapshot: Option<AuthorPageSnapshot>,
    fetching: bool,
    snapshot_seen: bool,
    /// The open panel's group, by title so a re-snapshot that reorders
    /// groups keeps the same card open.
    open_group_title: Option<String>,
    /// Card buttons aligned with `snapshot.groups`, for anchoring the
    /// open-series panel under the open card.
    card_buttons: Vec<Option<gtk::Button>>,
    /// Overlay children (⋯ host, panel) built in `init` outside the
    /// `view!` tree, like the series page's scrolled window handle.
    more_host: gtk::Box,
    panel_revealer: gtk::Revealer,
    panel_box: gtk::Box,
    /// Shared with the root key handler: while the ⋯ menu is open its Esc
    /// belongs to the menu, not the panel.
    menu_open: Rc<Cell<bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EntryState {
    Missing,
    Read,
    InProgress,
    Unread,
}

fn entry_state(entry: &AuthorEntrySnapshot, finished: &HashSet<i64>) -> EntryState {
    let Some(book) = &entry.book else {
        return EntryState::Missing;
    };
    if finished.contains(&book.id) || book.progress >= 100 {
        EntryState::Read
    } else if book.progress > 0 {
        EntryState::InProgress
    } else {
        EntryState::Unread
    }
}

fn entry_segment(entry: &AuthorEntrySnapshot, finished: &HashSet<i64>) -> SegmentState {
    match entry_state(entry, finished) {
        EntryState::Missing => SegmentState::Missing,
        EntryState::Read => SegmentState::Read,
        EntryState::InProgress => SegmentState::InProgress(
            f64::from(entry.book.as_ref().map(|b| b.progress).unwrap_or_default()) / 100.0,
        ),
        EntryState::Unread => SegmentState::Unread,
    }
}

fn book_segment(book: &Book, finished: &HashSet<i64>) -> SegmentState {
    if finished.contains(&book.id) || book.progress >= 100 {
        SegmentState::Read
    } else if book.progress > 0 {
        SegmentState::InProgress(f64::from(book.progress) / 100.0)
    } else {
        SegmentState::Unread
    }
}

fn is_read(book: &Book, finished: &HashSet<i64>) -> bool {
    finished.contains(&book.id) || book.progress >= 100
}

fn is_comic(book: &Book) -> bool {
    matches!(book.format, BookFormat::Cbz | BookFormat::Cbr)
}

fn has_bibliography(snap: &AuthorPageSnapshot) -> bool {
    snap.profile
        .as_ref()
        .is_some_and(|profile| !profile.works.is_empty())
}

fn display_name(model: &AuthorPageModel) -> String {
    model
        .snapshot
        .as_ref()
        .and_then(|snap| snap.profile.as_ref())
        .map(|profile| profile.canonical_name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| display_author_name(&model.requested_name))
}

fn highlights_of(snap: &AuthorPageSnapshot) -> Vec<&Annotation> {
    snap.annotations
        .iter()
        .filter(|anno| !anno.text_excerpt.trim().is_empty())
        .collect()
}

fn year_range_text(entries: &[AuthorEntrySnapshot]) -> Option<String> {
    let mut years: Vec<i64> = entries.iter().filter_map(|e| e.year).collect();
    years.sort_unstable();
    match (years.first(), years.last()) {
        (Some(a), Some(b)) if a == b => Some(a.to_string()),
        (Some(a), Some(b)) => Some(format!("{a}–{b}")),
        _ => None,
    }
}

/// The first plausible year in a free-form Open Library date string
/// ("21 October 1929", "1929", "1929-10-21").
fn parse_year(text: &str) -> Option<i64> {
    let bytes = text.as_bytes();
    if bytes.len() < 4 {
        return None;
    }
    for i in 0..=bytes.len() - 4 {
        if bytes[i].is_ascii_digit()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3].is_ascii_digit()
        {
            if let Ok(year) = text[i..i + 4].parse::<i64>() {
                if (800..=2999).contains(&year) {
                    return Some(year);
                }
            }
        }
    }
    None
}

/// Days since the civil epoch for a YYYY-MM-DD prefix (Howard Hinnant's
/// algorithm), for the footer's "updated N days ago".
fn days_from_iso(text: &str) -> Option<i64> {
    let date = text.get(..10)?;
    let year: i64 = date.get(0..4)?.parse().ok()?;
    let month: i64 = date.get(5..7)?.parse().ok()?;
    let day: i64 = date.get(8..10)?.parse().ok()?;
    let y = if month <= 2 { year - 1 } else { year };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (month + 9) % 12;
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    Some(era * 146097 + doe)
}

fn updated_text(fetched_at: &str) -> Option<String> {
    let then = days_from_iso(fetched_at)?;
    let today = days_from_iso(&crate::db::chrono_like_now())?;
    let days = (today - then).max(0);
    let text = match days {
        0 => "today".to_string(),
        1 => "yesterday".to_string(),
        2..=29 => format!("{days} days ago"),
        30..=364 => {
            let months = days / 30;
            format!("{months} month{} ago", if months == 1 { "" } else { "s" })
        }
        _ => {
            let years = days / 365;
            format!("{years} year{} ago", if years == 1 { "" } else { "s" })
        }
    };
    Some(text)
}

fn hl_color_class(color: &str) -> &'static str {
    match color.to_ascii_lowercase().as_str() {
        "green" => "kalam-hl-bar-green",
        "blue" => "kalam-hl-bar-blue",
        "pink" | "rose" => "kalam-hl-bar-pink",
        "orange" => "kalam-hl-bar-orange",
        _ => "kalam-hl-bar-yellow",
    }
}

fn set_accessible_label(widget: &impl IsA<gtk::Accessible>, label: &str) {
    widget.update_property(&[gtk::accessible::Property::Label(label)]);
}

fn clear_box(host: &gtk::Box) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
}

#[relm4::component(pub)]
impl Component for AuthorPageModel {
    type Init = (Arc<Catalog>, String);
    type Input = AuthorPageMsg;
    type Output = AuthorPageOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Overlay {
            add_css_class: "kalam-authorpage",
            set_hexpand: true,
            set_vexpand: false,

            #[wrap(Some)]
            set_child = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                add_css_class: "kalam-authorpage-inner",
                set_spacing: 0,

                #[name = "header_host"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 32,
                    set_valign: gtk::Align::Start,
                },

                #[name = "notice_host"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                },

                #[name = "groups_host"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                },

                #[name = "highlights_host"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                },

                #[name = "footer_host"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                },
            },
        }
    }

    fn init(
        (catalog, author_name): Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Overlay children: the open-series panel (under its card, over
        // the content below — decision 108) and the ⋯ button (top-right,
        // level with the shell's Back pill — the host's CSS transform
        // lifts it into the content host's 72px back-pill band because
        // GTK4 clamps negative margins; 24+32 = the spec's 56px inset).
        let panel_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        panel_box.add_css_class("kalam-authorpage-panel");

        let panel_revealer = gtk::Revealer::new();
        panel_revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);
        panel_revealer.set_transition_duration(250);
        panel_revealer.set_halign(gtk::Align::Start);
        panel_revealer.set_valign(gtk::Align::Start);
        panel_revealer.set_reveal_child(false);
        panel_revealer.set_visible(false);
        panel_revealer.set_child(Some(&panel_box));

        let more_host = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        more_host.add_css_class("kalam-authorpage-more-host");
        more_host.set_halign(gtk::Align::End);
        more_host.set_valign(gtk::Align::Start);
        more_host.set_margin_end(32);

        let menu_open = Rc::new(Cell::new(false));

        let mut model = AuthorPageModel {
            service: LibraryService::new(catalog),
            requested_name: author_name,
            snapshot: None,
            fetching: false,
            snapshot_seen: false,
            open_group_title: None,
            card_buttons: Vec::new(),
            more_host: more_host.clone(),
            panel_revealer: panel_revealer.clone(),
            panel_box: panel_box.clone(),
            menu_open: menu_open.clone(),
        };
        let widgets = view_output!();

        root.add_overlay(&panel_revealer);
        root.add_overlay(&more_host);

        // Esc closes the open panel (decision 108). Bubble phase (the
        // default): focused widgets and the ⋯ popover consume theirs
        // first; in-app dialogs live outside this subtree entirely.
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
            if keyval == Key::Escape {
                key_sender.input(AuthorPageMsg::ClosePanel);
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        root.add_controller(keys);

        // Skeleton fill (name + loading line), then the worker read.
        // No network fetch is started here or on load (decision 110).
        model.rebuild(&widgets, &root, &sender);
        request_snapshot(&model.service, &model.requested_name, &sender);

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        let mut rebuild = true;
        match msg {
            AuthorPageMsg::Refresh => {
                request_snapshot(&self.service, &self.requested_name, &sender);
                rebuild = false;
            }
            AuthorPageMsg::FindMetadata => {
                if !self.fetching {
                    self.fetching = true;
                    spawn_author_fetch(self, &sender);
                }
            }
            AuthorPageMsg::Fetched(result) => {
                self.fetching = false;
                match *result {
                    Ok(_) => {
                        request_snapshot(&self.service, &self.requested_name, &sender);
                    }
                    Err(err) => {
                        crate::notify::error("Could not fetch author info", &err);
                    }
                }
            }
            AuthorPageMsg::Loaded(load) => {
                let load = *load;
                // Cache what the worker decoded before the fill runs, so
                // every cover below takes its "already decoded" branch
                // instead of decoding on the UI thread.
                for decoded in &load.covers {
                    cache_decoded_cover(decoded);
                }
                let first_load = !self.snapshot_seen;
                self.snapshot_seen = true;
                self.snapshot = Some(load.snap);

                if first_load {
                    // CI screenshot hooks, mirroring the book page's
                    // KALAM_DIALOG: photograph the open-series panel and
                    // this page's two dialogs headlessly.
                    match std::env::var("KALAM_DIALOG").as_deref() {
                        Ok("panel") => {
                            self.open_group_title = self.snapshot.as_ref().and_then(|snap| {
                                snap.groups
                                    .iter()
                                    .find(|g| {
                                        g.series.is_some()
                                            && g.entries.iter().any(|e| e.book.is_some())
                                    })
                                    .or_else(|| {
                                        snap.groups
                                            .iter()
                                            .find(|g| g.entries.iter().any(|e| e.book.is_some()))
                                    })
                                    .map(|g| g.title.clone())
                            });
                            self.warm_panel_covers();
                        }
                        Ok("shelf") => sender.input(AuthorPageMsg::OpenCreateShelf),
                        Ok("annotations") => sender.input(AuthorPageMsg::OpenAnnotations),
                        _ => {}
                    }
                }
            }
            AuthorPageMsg::ToggleGroup(index) => {
                let title = self
                    .snapshot
                    .as_ref()
                    .and_then(|snap| snap.groups.get(index))
                    .map(|group| group.title.clone());
                if let Some(title) = title {
                    if self.open_group_title.as_deref() == Some(title.as_str()) {
                        self.open_group_title = None;
                    } else {
                        self.open_group_title = Some(title);
                        self.warm_panel_covers();
                    }
                }
            }
            AuthorPageMsg::ClosePanel => {
                if self.open_group_title.take().is_none() {
                    rebuild = false;
                }
            }
            AuthorPageMsg::OpenCreateShelf => {
                present_create_shelf_dialog(root, self, &sender);
                rebuild = false;
            }
            AuthorPageMsg::OpenAnnotations => {
                present_author_annotations_dialog(root, self, &sender);
                rebuild = false;
            }
            AuthorPageMsg::AddAllToReadingList => {
                let catalog = self.service.catalog();
                let mut added = 0usize;
                if let Some(snap) = &self.snapshot {
                    for book in &snap.owned_books {
                        if catalog.add_to_reading_list(book.id).is_ok() {
                            added += 1;
                        }
                    }
                }
                if added > 0 {
                    crate::notify::success(
                        &format!(
                            "Added {added} book{} to your reading list",
                            if added == 1 { "" } else { "s" }
                        ),
                        &display_name(self),
                    );
                } else {
                    crate::notify::info("Nothing to add", "No owned books by this author.");
                }
                rebuild = false;
            }
            AuthorPageMsg::ExportHighlights => {
                export_highlights(self);
                rebuild = false;
            }
        }

        if rebuild {
            self.rebuild(widgets, root, &sender);
        }
        self.update_view(widgets, sender);
    }
}

impl AuthorPageModel {
    fn rebuild(
        &mut self,
        widgets: &AuthorPageModelWidgets,
        root: &gtk::Overlay,
        sender: &ComponentSender<Self>,
    ) {
        // The same attribution pair the other migrated pages use: a timing
        // span for KALAM_TIMING=1 and an activity label so a stall during
        // this build says "while author_fill" (7.1 step 2c).
        let _t = crate::timing::measure("author_fill");
        let _a = crate::timing::activity("author_fill");
        self.menu_open.set(false);
        self.fill_header(widgets, sender);
        self.fill_notice(widgets, sender);
        self.fill_groups(widgets, sender);
        self.fill_highlights(widgets, sender);
        self.fill_footer(widgets);
        self.update_panel(root, sender);
    }

    fn open_group_index(&self) -> Option<usize> {
        let title = self.open_group_title.as_deref()?;
        self.snapshot
            .as_ref()?
            .groups
            .iter()
            .position(|group| group.title == title)
    }

    fn warm_panel_covers(&self) {
        let Some(index) = self.open_group_index() else {
            return;
        };
        let Some(snap) = &self.snapshot else { return };
        let paths: Vec<std::path::PathBuf> = snap
            .groups
            .get(index)
            .map(|group| {
                group
                    .entries
                    .iter()
                    .filter_map(|entry| entry.book.as_ref())
                    .filter_map(|book| book.cover_path.clone())
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        if !paths.is_empty() {
            crate::preload::warm_covers(paths, PANEL_COVER_W, PANEL_COVER_H);
        }
    }

    fn fill_header(&self, widgets: &AuthorPageModelWidgets, sender: &ComponentSender<Self>) {
        clear_box(&widgets.header_host);
        clear_box(&self.more_host);

        let name = display_name(self);
        let left = gtk::Box::new(gtk::Orientation::Vertical, 8);
        left.set_size_request(640, -1);
        left.set_hexpand(false);
        left.set_valign(gtk::Align::Start);

        let eyebrow = gtk::Label::new(Some("AUTHOR"));
        eyebrow.add_css_class("kalam-authorpage-eyebrow");
        eyebrow.set_halign(gtk::Align::Start);
        eyebrow.set_xalign(0.0);
        left.append(&eyebrow);

        let title = gtk::Label::new(Some(&name));
        title.add_css_class("kalam-authorpage-title");
        title.set_halign(gtk::Align::Start);
        title.set_xalign(0.0);
        title.set_wrap(true);
        title.set_lines(2);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title.set_max_width_chars(24);
        left.append(&title);

        let snap = self.snapshot.as_ref();
        match snap {
            None => {
                let loading = gtk::Label::new(Some("Loading author…"));
                loading.add_css_class("kalam-ui-meta");
                loading.set_halign(gtk::Align::Start);
                left.append(&loading);
            }
            Some(snap) => {
                if let Some(bio) = snap
                    .profile
                    .as_ref()
                    .map(|profile| profile.bio.trim())
                    .filter(|bio| !bio.is_empty())
                {
                    let bio_label = gtk::Label::new(Some(bio));
                    bio_label.add_css_class("kalam-authorpage-bio");
                    bio_label.set_halign(gtk::Align::Start);
                    bio_label.set_xalign(0.0);
                    bio_label.set_wrap(true);
                    bio_label.set_lines(3);
                    bio_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    bio_label.set_max_width_chars(84);
                    left.append(&bio_label);

                    // The More toggle exists only when the three lines
                    // actually cut the bio short — asked of the laid-out
                    // text on map, not guessed from a character count
                    // (a count misjudges at real font metrics, CI caught
                    // a More under a fully-shown bio).
                    let more = gtk::Button::with_label("More");
                    more.add_css_class("kalam-authorpage-more");
                    more.set_halign(gtk::Align::Start);
                    more.set_visible(false);
                    let more_for_map = more.clone();
                    bio_label.connect_map(move |label| {
                        more_for_map.set_visible(label.layout().is_ellipsized());
                    });
                    let expanded = Rc::new(Cell::new(false));
                    let bio_for_toggle = bio_label.clone();
                    more.connect_clicked(move |button| {
                        let next = !expanded.get();
                        expanded.set(next);
                        if next {
                            bio_for_toggle.set_lines(-1);
                            bio_for_toggle.set_ellipsize(gtk::pango::EllipsizeMode::None);
                            button.set_label("Less");
                        } else {
                            bio_for_toggle.set_lines(3);
                            bio_for_toggle.set_ellipsize(gtk::pango::EllipsizeMode::End);
                            button.set_label("More");
                        }
                    });
                    left.append(&more);
                }

                let pills = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                pills.set_margin_top(20);
                pills.set_halign(gtk::Align::Start);

                match snap.shelf {
                    Some((shelf_id, _)) => {
                        let s = sender.clone();
                        pills.append(&boxed_ghost_button("Open shelf", move || {
                            s.output(AuthorPageOut::OpenShelf { shelf_id }).ok();
                        }));
                    }
                    None => {
                        let s = sender.clone();
                        pills.append(&boxed_ghost_button("Create shelf", move || {
                            s.input(AuthorPageMsg::OpenCreateShelf);
                        }));
                    }
                }

                let s = sender.clone();
                let find = boxed_ghost_button("Find metadata", move || {
                    s.input(AuthorPageMsg::FindMetadata);
                });
                find.set_sensitive(!self.fetching);
                find.set_tooltip_text(Some(
                    "Fetch this author's bibliography from Open Library",
                ));
                pills.append(&find);
                left.append(&pills);
            }
        }
        widgets.header_host.append(&left);

        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        widgets.header_host.append(&spacer);

        if let Some(snap) = snap {
            widgets.header_host.append(&facts_panel(snap, &name));
            self.fill_more_menu(snap, &name, sender);
        }
    }

    fn fill_more_menu(
        &self,
        snap: &AuthorPageSnapshot,
        name: &str,
        sender: &ComponentSender<Self>,
    ) {
        // Edit author… and Merge into another author… belong to this menu
        // (decision 112) but their dialogs are not designed yet; like the
        // book page's Edit book, they stay out until they are.
        let menu = menu_popover(MenuWidth::Wide);

        match snap.shelf {
            Some((shelf_id, _)) => {
                let s = sender.clone();
                menu.add_item("Open shelf", move || {
                    s.output(AuthorPageOut::OpenShelf { shelf_id }).ok();
                });
            }
            None => {
                let s = sender.clone();
                menu.add_item("Create shelf…", move || {
                    s.input(AuthorPageMsg::OpenCreateShelf);
                });
            }
        }
        menu.add_separator();

        let owned = snap.owned_books.len();
        let s = sender.clone();
        menu.add_item(
            &format!(
                "Find metadata for {owned} book{}…",
                if owned == 1 { "" } else { "s" }
            ),
            move || {
                s.input(AuthorPageMsg::FindMetadata);
            },
        );
        let s = sender.clone();
        menu.add_item("Add all to reading list", move || {
            s.input(AuthorPageMsg::AddAllToReadingList);
        });
        let query = name.to_string();
        let s = sender.clone();
        menu.add_item("Show books in library", move || {
            s.output(AuthorPageOut::ShowInLibrary {
                query: query.clone(),
            })
            .ok();
        });
        let s = sender.clone();
        menu.add_item("Export highlights…", move || {
            s.input(AuthorPageMsg::ExportHighlights);
        });

        let more = menu_button("view-more-symbolic", "Author actions", menu);
        more.add_css_class("kalam-authorpage-more-btn");
        more.set_size_request(44, 44);
        more.set_sensitive(!self.fetching);
        let menu_open = self.menu_open.clone();
        more.connect_active_notify(move |button| {
            menu_open.set(button.is_active());
        });
        set_accessible_label(&more, "Author actions");
        self.more_host.append(&more);
    }

    fn fill_notice(&self, widgets: &AuthorPageModelWidgets, sender: &ComponentSender<Self>) {
        clear_box(&widgets.notice_host);
        let Some(snap) = &self.snapshot else { return };
        if has_bibliography(snap) {
            return;
        }
        // The quiet notice (decision 111): one Meta line under a hairline,
        // with Find metadata as an accent link inside the sentence.
        let host = &widgets.notice_host;
        let rule = hairline();
        rule.set_margin_top(32);
        host.append(&rule);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        row.set_margin_top(16);
        row.set_halign(gtk::Align::Start);

        let before = gtk::Label::new(Some("No bibliography yet. "));
        before.add_css_class("kalam-authorpage-notice");
        before.set_valign(gtk::Align::Center);
        row.append(&before);

        let s = sender.clone();
        let link = gtk::Button::with_label("Find metadata");
        link.add_css_class("kalam-authorpage-notice-link");
        link.set_valign(gtk::Align::Center);
        link.set_sensitive(!self.fetching);
        link.connect_clicked(move |_| {
            s.input(AuthorPageMsg::FindMetadata);
        });
        row.append(&link);

        let after = gtk::Label::new(Some(
            " to see this author's other works. Your own books are shown below.",
        ));
        after.add_css_class("kalam-authorpage-notice");
        after.set_valign(gtk::Align::Center);
        after.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.append(&after);
        host.append(&row);
    }

    fn fill_groups(&mut self, widgets: &AuthorPageModelWidgets, sender: &ComponentSender<Self>) {
        clear_box(&widgets.groups_host);
        self.card_buttons.clear();
        let Some(snap) = &self.snapshot else { return };
        self.card_buttons = (0..snap.groups.len()).map(|_| None).collect();

        let bib = has_bibliography(snap);
        let open_index = self.open_group_index();

        for in_library in [true, false] {
            let indices: Vec<usize> = snap
                .groups
                .iter()
                .enumerate()
                .filter(|(_, group)| {
                    group.entries.iter().any(|e| e.book.is_some()) == in_library
                })
                .map(|(index, _)| index)
                .collect();
            if indices.is_empty() {
                continue;
            }

            let heading = gtk::Label::new(Some(if in_library {
                "IN LIBRARY"
            } else {
                "NOT IN LIBRARY"
            }));
            heading.add_css_class("kalam-ui-label");
            heading.set_halign(gtk::Align::Start);
            // 48 above, 16 below (decision 107), whichever side of the
            // header or the other group it follows.
            heading.set_margin_top(48);
            heading.set_margin_bottom(16);
            widgets.groups_host.append(&heading);

            let grid = gtk::Grid::new();
            grid.set_column_homogeneous(true);
            grid.set_column_spacing(16);
            grid.set_row_spacing(16);
            grid.set_hexpand(true);

            for (slot, &index) in indices.iter().enumerate() {
                let group = &snap.groups[index];
                let card = series_card(
                    index,
                    group,
                    bib,
                    in_library,
                    open_index == Some(index),
                    snap,
                    sender,
                );
                self.card_buttons[index] = Some(card.clone());
                grid.attach(&card, (slot % 2) as i32, (slot / 2) as i32, 1, 1);
            }
            // A lone card in the last row still gets only its half: the
            // empty cell keeps the second homogeneous column in being,
            // or the single occupied column stretches full-width.
            if indices.len() % 2 == 1 {
                let filler = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                grid.attach(&filler, 1, (indices.len() / 2) as i32, 1, 1);
            }
            widgets.groups_host.append(&grid);
        }

        if snap.groups.is_empty() {
            let empty = gtk::Label::new(Some("No books by this author yet."));
            empty.add_css_class("kalam-ui-meta");
            empty.set_halign(gtk::Align::Start);
            empty.set_margin_top(48);
            widgets.groups_host.append(&empty);
        }
    }

    fn fill_highlights(&self, widgets: &AuthorPageModelWidgets, sender: &ComponentSender<Self>) {
        clear_box(&widgets.highlights_host);
        let Some(snap) = &self.snapshot else { return };
        let highlights = highlights_of(snap);
        if highlights.is_empty() {
            return;
        }
        let books: HashMap<i64, &Book> =
            snap.owned_books.iter().map(|book| (book.id, book)).collect();

        let heading_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        heading_row.set_margin_top(48);
        heading_row.set_margin_bottom(16);

        let heading = gtk::Label::new(Some("Your highlights"));
        heading.add_css_class("kalam-ui-section-title");
        heading.set_halign(gtk::Align::Start);
        heading.set_valign(gtk::Align::Center);
        heading_row.append(&heading);

        let total = snap.annotations.len();
        let s = sender.clone();
        let see_all = gtk::Button::with_label(&format!("See all {total}"));
        see_all.add_css_class("kalam-authorpage-see-all");
        see_all.set_valign(gtk::Align::Center);
        see_all.set_tooltip_text(Some("Open all annotations for this author"));
        see_all.connect_clicked(move |_| {
            s.input(AuthorPageMsg::OpenAnnotations);
        });
        heading_row.append(&see_all);

        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        heading_row.append(&spacer);

        // The scroller first: the arrows drive its adjustment.
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .overlay_scrolling(true)
            .hexpand(true)
            .build();

        let adjustment = scroller.hadjustment();
        let adj_prev = adjustment.clone();
        let prev = icon_button("go-previous-symbolic", "Scroll highlights back", 36, true, move || {
            scroll_highlights(&adj_prev, -1.0);
        });
        let adj_next = adjustment.clone();
        let next = icon_button("go-next-symbolic", "Scroll highlights forward", 36, true, move || {
            scroll_highlights(&adj_next, 1.0);
        });
        let sync_arrows = {
            let prev = prev.clone();
            let next = next.clone();
            let adjustment = adjustment.clone();
            Rc::new(move || {
                let max = (adjustment.upper() - adjustment.page_size()).max(0.0);
                let value = adjustment.value();
                set_arrow_state(&prev, value > 0.5);
                set_arrow_state(&next, value < max - 0.5);
            })
        };
        {
            let sync = sync_arrows.clone();
            adjustment.connect_value_changed(move |_| sync());
            let sync = sync_arrows.clone();
            adjustment.connect_upper_notify(move |_| sync());
            let sync = sync_arrows.clone();
            adjustment.connect_page_size_notify(move |_| sync());
        }
        heading_row.append(&prev);
        heading_row.append(&next);
        widgets.highlights_host.append(&heading_row);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        row.set_halign(gtk::Align::Start);
        row.set_valign(gtk::Align::Start);
        for anno in highlights {
            let Some(book) = books.get(&anno.book_id) else {
                continue;
            };
            row.append(&highlight_card(anno, book, snap, sender));
        }
        scroller.set_child(Some(&row));
        widgets.highlights_host.append(&scroller);
        sync_arrows();
    }

    fn fill_footer(&self, widgets: &AuthorPageModelWidgets) {
        clear_box(&widgets.footer_host);
        let Some(snap) = &self.snapshot else { return };
        let Some(profile) = &snap.profile else { return };
        let Some(updated) = updated_text(&profile.fetched_at) else {
            return;
        };
        let footer = gtk::Label::new(Some(&format!(
            "Bibliography from Open Library · updated {updated}"
        )));
        footer.add_css_class("kalam-authorpage-footer");
        footer.set_halign(gtk::Align::Start);
        footer.set_xalign(0.0);
        footer.set_margin_top(32);
        widgets.footer_host.append(&footer);
    }

    /// Rebuild and anchor the open-series panel (decision 108): directly
    /// under its card, over the content below, nothing else moves.
    fn update_panel(&self, root: &gtk::Overlay, sender: &ComponentSender<Self>) {
        let open = self.open_group_index().zip(self.snapshot.as_ref());
        let Some((index, snap)) = open else {
            self.panel_revealer.set_reveal_child(false);
            self.panel_revealer.set_visible(false);
            return;
        };
        let group = &snap.groups[index];

        clear_box(&self.panel_box);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let count = group.entries.len();
        let caption = gtk::Label::new(Some(&format!(
            "{} · {count} book{}",
            group.title,
            if count == 1 { "" } else { "s" }
        )));
        caption.add_css_class("kalam-ui-meta");
        caption.set_halign(gtk::Align::Start);
        caption.set_valign(gtk::Align::Center);
        caption.set_ellipsize(gtk::pango::EllipsizeMode::End);
        header.append(&caption);

        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        header.append(&spacer);

        if let Some(series) = &group.series {
            let comic = group
                .entries
                .iter()
                .filter_map(|entry| entry.book.as_ref())
                .any(is_comic);
            let series = series.clone();
            let author = display_name(self);
            let s = sender.clone();
            let open_series = gtk::Button::with_label("Open series ›");
            open_series.add_css_class("kalam-authorpage-open-series");
            open_series.set_valign(gtk::Align::Center);
            open_series.set_tooltip_text(Some("Open the series page"));
            open_series.connect_clicked(move |_| {
                if comic {
                    s.output(AuthorPageOut::ComicSeries {
                        series_name: series.clone(),
                    })
                    .ok();
                } else {
                    s.output(AuthorPageOut::OpenSeries {
                        series: series.clone(),
                        first_author: author.clone(),
                    })
                    .ok();
                }
            });
            header.append(&open_series);
        }
        self.panel_box.append(&header);

        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .overlay_scrolling(true)
            .hexpand(true)
            .build();
        scroller.set_margin_top(16);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 20);
        row.set_margin_start(4);
        row.set_margin_end(4);
        row.set_margin_top(4);
        row.set_margin_bottom(4);
        row.set_halign(gtk::Align::Start);
        row.set_valign(gtk::Align::Start);
        for entry in &group.entries {
            row.append(&panel_entry_column(entry, group, snap, sender));
        }
        scroller.set_child(Some(&row));
        self.panel_box.append(&scroller);

        // Anchor under the card once it has a real allocation: a single
        // idle is not enough on first load, when the fill that built the
        // card hasn't been allocated yet and compute_point would read
        // the unallocated origin (panel over the header, CI caught it).
        // Capped so a card torn down by a rebuild in between cannot spin
        // forever; the next rebuild re-anchors anyway.
        if let Some(card) = self.card_buttons.get(index).cloned().flatten() {
            let revealer = self.panel_revealer.clone();
            let overlay = root.clone();
            let mut tries = 0;
            gtk::glib::idle_add_local(move || {
                tries += 1;
                let ready = card.is_mapped() && card.width() > 1;
                if !ready && tries < 90 {
                    return gtk::glib::ControlFlow::Continue;
                }
                if let Some(point) =
                    card.compute_point(&overlay, &gtk::graphene::Point::new(0.0, 0.0))
                {
                    revealer.set_margin_start(point.x() as i32);
                    revealer.set_margin_top(point.y() as i32 + card.height());
                    revealer.set_size_request(card.width(), -1);
                }
                revealer.set_visible(true);
                revealer.set_reveal_child(true);
                gtk::glib::ControlFlow::Break
            });
        } else {
            self.panel_revealer.set_visible(true);
            self.panel_revealer.set_reveal_child(true);
        }
    }
}

fn scroll_highlights(adjustment: &gtk::Adjustment, direction: f64) {
    let max = (adjustment.upper() - adjustment.page_size()).max(0.0);
    let step = 420.0 + 16.0;
    adjustment.set_value((adjustment.value() + direction * step).clamp(0.0, max));
}

fn set_arrow_state(button: &gtk::Button, enabled: bool) {
    button.set_sensitive(enabled);
    button.set_opacity(if enabled { 1.0 } else { 0.4 });
}

/// §25 facts panel: outlined 340, portrait + lifespan, Works / Series /
/// You've read rows over hairlines.
fn facts_panel(snap: &AuthorPageSnapshot, name: &str) -> gtk::Box {
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.add_css_class("kalam-authorpage-facts");
    panel.set_size_request(340, -1);
    panel.set_hexpand(false);
    panel.set_valign(gtk::Align::Start);

    let bib = has_bibliography(snap);
    let profile = snap.profile.as_ref();

    let top = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    top.set_valign(gtk::Align::Start);

    // Existence was settled on the worker (service::author_page); the
    // path is either a real file or already None.
    let photo = profile.and_then(|p| p.photo_path.as_deref());
    let portrait: gtk::Widget = match photo {
        Some(path) => {
            let widget = cover_widget(Some(path), PORTRAIT, PORTRAIT);
            widget.add_css_class("kalam-authorpage-portrait");
            widget.set_size_request(PORTRAIT, PORTRAIT);
            widget
        }
        None => {
            let disc = gtk::Box::new(gtk::Orientation::Vertical, 0);
            disc.add_css_class("kalam-authorpage-portrait-initials");
            disc.set_size_request(PORTRAIT, PORTRAIT);
            disc.set_halign(gtk::Align::Start);
            let label = gtk::Label::new(Some(&initials(name)));
            label.add_css_class("kalam-authorpage-initials-text");
            label.set_halign(gtk::Align::Center);
            label.set_valign(gtk::Align::Center);
            label.set_vexpand(true);
            disc.append(&label);
            disc.upcast()
        }
    };
    portrait.set_valign(gtk::Align::Start);
    top.append(&portrait);

    let birth = profile.and_then(|p| parse_year(&p.birth_date));
    let death = profile.and_then(|p| parse_year(&p.death_date));
    match (birth, death) {
        (Some(b), Some(d)) => top.append(&lifespan_block(&format!("{b}–{d}"))),
        (Some(b), None) => top.append(&lifespan_block(&format!("Born {b}"))),
        (None, Some(d)) => top.append(&lifespan_block(&format!("Died {d}"))),
        (None, None) => {
            let none = gtk::Label::new(Some("No details yet"));
            none.add_css_class("kalam-ui-meta");
            none.set_halign(gtk::Align::Start);
            none.set_valign(gtk::Align::Center);
            none.set_vexpand(true);
            top.append(&none);
        }
    }
    panel.append(&top);

    let owned = snap.owned_books.len();
    let total: usize = snap.groups.iter().map(|g| g.entries.len()).sum();
    let works_value = if bib {
        format!("{total} · {owned} in your library")
    } else {
        format!("{owned} in your library")
    };
    panel.append(&fact_row("Works", &works_value, None));

    let series_groups: Vec<&AuthorGroupSnapshot> = snap
        .groups
        .iter()
        .filter(|g| g.series.is_some())
        .collect();
    if bib && !series_groups.is_empty() {
        let in_library = series_groups
            .iter()
            .filter(|g| g.entries.iter().any(|e| e.book.is_some()))
            .count();
        panel.append(&fact_row(
            "Series",
            &format!("{} · {in_library} in your library", series_groups.len()),
            None,
        ));
    }

    let read = snap
        .owned_books
        .iter()
        .filter(|book| is_read(book, &snap.finished))
        .count();
    let segments: Vec<SegmentState> = snap
        .owned_books
        .iter()
        .map(|book| book_segment(book, &snap.finished))
        .collect();
    let bar = if segments.is_empty() {
        None
    } else {
        Some(segmented_progress(&segments, 300))
    };
    panel.append(&fact_row("You've read", &format!("{read} of {owned}"), bar));

    panel
}

fn lifespan_block(value: &str) -> gtk::Box {
    let block = gtk::Box::new(gtk::Orientation::Vertical, 2);
    block.set_valign(gtk::Align::Center);
    block.set_vexpand(true);
    let key = gtk::Label::new(Some("LIFESPAN"));
    key.add_css_class("kalam-ui-label");
    key.set_halign(gtk::Align::Start);
    block.append(&key);
    let val = gtk::Label::new(Some(value));
    val.add_css_class("kalam-authorpage-lifespan");
    val.set_halign(gtk::Align::Start);
    block.append(&val);
    block
}

fn fact_row(key: &str, value: &str, bar: Option<gtk::Box>) -> gtk::Box {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let rule = hairline();
    rule.set_margin_top(14);
    wrap.append(&rule);

    let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
    row.set_margin_top(14);
    let key_label = gtk::Label::new(Some(&key.to_uppercase()));
    key_label.add_css_class("kalam-ui-label");
    key_label.set_halign(gtk::Align::Start);
    key_label.set_xalign(0.0);
    row.append(&key_label);

    let value_label = gtk::Label::new(Some(value));
    value_label.add_css_class("kalam-authorpage-fact-value");
    value_label.set_halign(gtk::Align::Start);
    value_label.set_xalign(0.0);
    row.append(&value_label);

    if let Some(bar) = bar {
        bar.set_margin_top(8);
        bar.set_halign(gtk::Align::Start);
        row.append(&bar);
    }
    wrap.append(&row);
    wrap
}

/// §26 series card: title, segmented bar, Meta line, cover fan, chevron.
fn series_card(
    index: usize,
    group: &AuthorGroupSnapshot,
    bib: bool,
    in_library: bool,
    open: bool,
    snap: &AuthorPageSnapshot,
    sender: &ComponentSender<AuthorPageModel>,
) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("kalam-authorpage-card");
    if open {
        button.add_css_class("open");
    }
    button.set_size_request(-1, 124);
    button.set_hexpand(true);
    button.set_can_focus(true);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    row.set_valign(gtk::Align::Center);

    let info = gtk::Box::new(gtk::Orientation::Vertical, 0);
    info.set_valign(gtk::Align::Center);
    info.set_hexpand(true);

    let title = gtk::Label::new(Some(&group.title));
    title.add_css_class("kalam-ui-card-title");
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(34);
    info.append(&title);

    let segments: Vec<SegmentState> = group
        .entries
        .iter()
        .map(|entry| entry_segment(entry, &snap.finished))
        .collect();
    let bar = segmented_progress(&segments, 220);
    bar.set_margin_top(10);
    bar.set_halign(gtk::Align::Start);
    info.append(&bar);

    let owned = group.entries.iter().filter(|e| e.book.is_some()).count();
    let total = group.entries.len();
    let read = group
        .entries
        .iter()
        .filter_map(|e| e.book.as_ref())
        .filter(|book| is_read(book, &snap.finished))
        .count();
    let mut meta = if bib {
        format!("{owned} of {total} owned")
    } else {
        format!("{owned} owned")
    };
    if in_library {
        meta.push_str(&format!(" · {read} read"));
    }
    if let Some(years) = year_range_text(&group.entries) {
        meta.push_str(&format!(" · {years}"));
    }
    let meta_label = gtk::Label::new(Some(&meta));
    meta_label.add_css_class("kalam-ui-meta");
    meta_label.set_halign(gtk::Align::Start);
    meta_label.set_xalign(0.0);
    meta_label.set_margin_top(8);
    meta_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    info.append(&meta_label);
    row.append(&info);

    row.append(&cover_fan(group, snap));

    let chevron = gtk::Image::from_icon_name(if open {
        "go-up-symbolic"
    } else {
        "go-down-symbolic"
    });
    chevron.set_pixel_size(20);
    chevron.add_css_class("kalam-authorpage-chevron");
    chevron.set_valign(gtk::Align::Center);
    row.append(&chevron);

    button.set_child(Some(&row));
    let action = if open {
        format!("Close {}", group.title)
    } else {
        format!("Open {}", group.title)
    };
    button.set_tooltip_text(Some(&action));
    set_accessible_label(&button, &action);
    let s = sender.clone();
    button.connect_clicked(move |_| {
        s.input(AuthorPageMsg::ToggleGroup(index));
    });
    button
}

/// Three 56×84 covers overlapped by 22, first on top, no rotation (§26).
fn cover_fan(group: &AuthorGroupSnapshot, snap: &AuthorPageSnapshot) -> gtk::Overlay {
    let fan = gtk::Overlay::new();
    fan.set_size_request(100, FAN_H);
    fan.set_halign(gtk::Align::Start);
    fan.set_valign(gtk::Align::Center);
    fan.set_hexpand(false);

    let entries: Vec<&AuthorEntrySnapshot> = group.entries.iter().take(3).collect();
    if entries.is_empty() {
        return fan;
    }
    // Overlay children paint above the main child in add order, so lay
    // the fan back-to-front: last cover as the child, first on top.
    let make = |entry: &AuthorEntrySnapshot, offset: i32| -> gtk::Widget {
        let widget = match &entry.book {
            Some(book) => book_cover(
                book,
                FAN_W,
                FAN_H,
                false,
                CoverAvailability::Available,
                true,
            ),
            None => missing_cover(&entry.title, FAN_W, FAN_H).upcast(),
        };
        widget.add_css_class("kalam-authorpage-fan-cover");
        widget.set_halign(gtk::Align::Start);
        widget.set_valign(gtk::Align::Start);
        widget.set_margin_start(offset);
        widget.set_size_request(FAN_W, FAN_H);
        widget
    };
    let _ = snap;
    let last = entries.len() - 1;
    fan.set_child(Some(&make(entries[last], 22 * last as i32)));
    for i in (0..last).rev() {
        fan.add_overlay(&make(entries[i], 22 * i as i32));
    }
    fan
}

/// One cover column in the open-series panel (§27).
fn panel_entry_column(
    entry: &AuthorEntrySnapshot,
    group: &AuthorGroupSnapshot,
    snap: &AuthorPageSnapshot,
    sender: &ComponentSender<AuthorPageModel>,
) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.set_size_request(PANEL_COVER_W, -1);
    column.set_hexpand(false);
    column.set_valign(gtk::Align::Start);

    match &entry.book {
        Some(book) => {
            let finished = is_read(book, &snap.finished);
            let button = gtk::Button::new();
            button.add_css_class("kalam-authorpage-panel-cover");
            button.set_size_request(PANEL_COVER_W, PANEL_COVER_H);
            button.set_halign(gtk::Align::Start);
            button.set_can_focus(true);
            button.set_child(Some(&book_cover(
                book,
                PANEL_COVER_W,
                PANEL_COVER_H,
                finished,
                CoverAvailability::Available,
                true,
            )));
            let tip = format!("Open {}", book.title);
            button.set_tooltip_text(Some(&tip));
            set_accessible_label(&button, &tip);
            let book_id = book.id;
            let comic_name = if is_comic(book) {
                book.series
                    .clone()
                    .filter(|s| !s.trim().is_empty())
                    .unwrap_or_else(|| group.title.clone())
            } else {
                String::new()
            };
            let comic = is_comic(book);
            let s = sender.clone();
            button.connect_clicked(move |_| {
                if comic {
                    s.output(AuthorPageOut::ComicSeries {
                        series_name: comic_name.clone(),
                    })
                    .ok();
                } else {
                    s.output(AuthorPageOut::OpenBook { book_id }).ok();
                }
            });
            column.append(&button);

            let progress_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
            progress_host.set_size_request(PANEL_COVER_W, 3);
            progress_host.set_margin_top(10);
            if entry_state(entry, &snap.finished) == EntryState::InProgress {
                let bar = linear_progress(f64::from(book.progress) / 100.0);
                bar.set_size_request(PANEL_COVER_W, 3);
                bar.set_hexpand(false);
                progress_host.append(&bar);
            }
            column.append(&progress_host);
        }
        None => {
            let missing = missing_cover_with_disabled_fetch(
                &entry.title,
                PANEL_COVER_W,
                PANEL_COVER_H,
            );
            missing.add_css_class("kalam-authorpage-panel-missing");
            column.append(&missing);

            let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
            spacer.set_size_request(PANEL_COVER_W, 3);
            spacer.set_margin_top(10);
            column.append(&spacer);
        }
    }

    let title = gtk::Label::new(Some(&entry.title));
    title.add_css_class("kalam-authorpage-panel-title");
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    title.set_wrap(true);
    title.set_lines(2);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(21);
    title.set_margin_top(8);
    column.append(&title);

    if let Some(year) = entry.year {
        let year_label = gtk::Label::new(Some(&year.to_string()));
        year_label.add_css_class("kalam-ui-meta");
        year_label.set_halign(gtk::Align::Start);
        column.append(&year_label);
    }

    column
}

/// §29 highlight card: header (book title, colour dot, chapter/date),
/// hairline, cover + four-line quote.
fn highlight_card(
    anno: &Annotation,
    book: &Book,
    snap: &AuthorPageSnapshot,
    sender: &ComponentSender<AuthorPageModel>,
) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("kalam-authorpage-hl-card");
    button.set_size_request(420, -1);
    button.set_hexpand(false);
    button.set_valign(gtk::Align::Start);
    button.set_can_focus(true);

    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.set_size_request(420, -1);

    let head = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    head.add_css_class("kalam-authorpage-hl-head");

    let head_text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    head_text.set_hexpand(true);
    let title = gtk::Label::new(Some(&book.title));
    title.add_css_class("kalam-ui-card-title");
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_max_width_chars(40);
    head_text.append(&title);

    let chapter = anno.chapter_index.max(0) + 1;
    let day = anno
        .created_at
        .get(..10)
        .map(pretty_day)
        .unwrap_or_else(|| anno.created_at.clone());
    let meta = gtk::Label::new(Some(&format!("Chapter {chapter} · Saved {day}")));
    meta.add_css_class("kalam-ui-meta");
    meta.set_halign(gtk::Align::Start);
    meta.set_xalign(0.0);
    head_text.append(&meta);
    head.append(&head_text);

    let dot = gtk::Box::new(gtk::Orientation::Vertical, 0);
    dot.add_css_class("kalam-authorpage-hl-dot");
    dot.add_css_class(hl_color_class(&anno.color));
    dot.set_size_request(10, 10);
    dot.set_valign(gtk::Align::Start);
    dot.set_halign(gtk::Align::End);
    head.append(&dot);
    card.append(&head);

    card.append(&hairline());

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    body.add_css_class("kalam-authorpage-hl-body");
    body.set_valign(gtk::Align::Start);

    let cover = book_cover(
        book,
        HL_COVER_W,
        HL_COVER_H,
        false,
        CoverAvailability::Available,
        true,
    );
    let cover_wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);
    cover_wrap.add_css_class("kalam-authorpage-hl-cover");
    cover_wrap.set_size_request(HL_COVER_W, HL_COVER_H);
    cover_wrap.append(&cover);
    body.append(&cover_wrap);

    let quote = gtk::Label::new(Some(anno.text_excerpt.trim()));
    quote.add_css_class("kalam-authorpage-hl-quote");
    quote.set_halign(gtk::Align::Start);
    quote.set_xalign(0.0);
    quote.set_wrap(true);
    quote.set_lines(4);
    quote.set_ellipsize(gtk::pango::EllipsizeMode::End);
    quote.set_max_width_chars(44);
    quote.set_valign(gtk::Align::Start);
    body.append(&quote);
    card.append(&body);

    button.set_child(Some(&card));
    let tip = format!("Open {} at this highlight", book.title);
    button.set_tooltip_text(Some(&tip));
    set_accessible_label(&button, &tip);
    let _ = snap;
    let book_id = book.id;
    let chapter = anno.chapter_index.max(0) as usize;
    let s = sender.clone();
    button.connect_clicked(move |_| {
        s.output(AuthorPageOut::OpenReaderAt { book_id, chapter })
            .ok();
    });
    button
}

fn export_highlights(model: &AuthorPageModel) {
    let Some(snap) = &model.snapshot else { return };
    let highlights = highlights_of(snap);
    if highlights.is_empty() {
        crate::notify::info("No highlights to export", "This author has no highlights yet.");
        return;
    }
    let name = display_name(model);
    let books: HashMap<i64, &Book> = snap.owned_books.iter().map(|b| (b.id, b)).collect();

    let mut by_book: Vec<(String, Vec<&Annotation>)> = Vec::new();
    let mut index: HashMap<i64, usize> = HashMap::new();
    for anno in highlights {
        let title = books
            .get(&anno.book_id)
            .map(|book| book.title.clone())
            .unwrap_or_else(|| "Unknown book".to_string());
        let slot = *index.entry(anno.book_id).or_insert_with(|| {
            by_book.push((title, Vec::new()));
            by_book.len() - 1
        });
        by_book[slot].1.push(anno);
    }

    let count: usize = by_book.iter().map(|(_, items)| items.len()).sum();
    let mut md = format!("# Highlights — {name}\n");
    for (title, items) in &by_book {
        md.push_str(&format!("\n## {title}\n"));
        for anno in items {
            let excerpt = anno
                .text_excerpt
                .trim()
                .replace("\r\n", "\n")
                .replace('\n', " ");
            md.push_str(&format!("\n- > {excerpt}\n"));
            let chapter = anno.chapter_index.max(0) + 1;
            let day = anno
                .created_at
                .get(..10)
                .map(pretty_day)
                .unwrap_or_else(|| anno.created_at.clone());
            md.push_str(&format!("  — Chapter {chapter} · Saved {day}\n"));
            let note = anno.note.trim();
            if !note.is_empty() {
                md.push_str(&format!("  - Note: {}\n", note.replace('\n', " ")));
            }
        }
    }

    let safe_name: String = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == ' ' || *c == '-' || *c == '_')
        .collect::<String>()
        .trim()
        .to_string();
    let safe_name = if safe_name.is_empty() {
        "Author".to_string()
    } else {
        safe_name
    };
    let out_path = crate::paths::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(format!("Kalam-Highlights-{safe_name}.md"));
    let out_done = out_path.clone();
    crate::tasks::spawn(
        "Exporting highlights",
        move |_| std::fs::write(&out_path, md).map_err(|e| e.to_string()),
        |_| {},
        move |res| match res {
            Ok(()) => crate::notify::success(
                &format!(
                    "{count} highlight{} exported",
                    if count == 1 { "" } else { "s" }
                ),
                &out_done.display().to_string(),
            ),
            Err(err) => crate::notify::error("Could not export highlights", &err),
        },
    );
}

/// Ask the worker for the page's data (7.1 step 2c): profile, owned books
/// and the series grouping, in one snapshot. Called at init, and again
/// after a network fetch lands, so the fill always rebuilds from fresh
/// owned data the way the old inline re-read did.
fn request_snapshot(
    service: &LibraryService,
    author_name: &str,
    sender: &ComponentSender<AuthorPageModel>,
) {
    let service = LibraryService::new(service.catalog().clone());
    let name = author_name.to_string();
    let s = sender.input_sender().clone();
    crate::tasks::spawn_internal(
        "Reading author",
        move |_reporter| {
            let snap = service.author_page(&name);
            let covers = warm_author_covers(&snap);
            AuthorPageLoad { snap, covers }
        },
        |_| {},
        move |load| {
            let _ = s.send(AuthorPageMsg::Loaded(Box::new(load)));
        },
    );
}

/// Decode, off the UI thread, every cover the fill will show on it.
///
/// The sizes are the fill's own sizes; if a size changes, this list must
/// change with it — a drift does not break anything, it just puts the
/// fill's synchronous decode back on the UI thread, visible again as a
/// slow `author_fill` line. Panel covers (180×270) warm on panel open
/// instead: most page views never open one.
fn warm_author_covers(snap: &AuthorPageSnapshot) -> Vec<crate::preload::DecodedCover> {
    let mut covers = Vec::new();
    let mut seen: HashSet<(std::path::PathBuf, i32, i32)> = HashSet::new();
    let mut want = |path: Option<&Path>, w: i32, h: i32, covers: &mut Vec<crate::preload::DecodedCover>| {
        let Some(path) = path else { return };
        if !seen.insert((path.to_path_buf(), w, h)) {
            return;
        }
        if let Some(d) = crate::preload::decode_for_cache(path, w, h) {
            covers.push(d);
        }
    };

    if let Some(profile) = &snap.profile {
        want(profile.photo_path.as_deref(), PORTRAIT, PORTRAIT, &mut covers);
    }
    // The cards' fans: the first three covers of every group.
    for group in &snap.groups {
        for entry in group.entries.iter().take(3) {
            if let Some(book) = &entry.book {
                want(book.cover_path.as_deref(), FAN_W, FAN_H, &mut covers);
            }
        }
    }
    // The highlights row's book covers.
    let books: HashMap<i64, &Book> = snap.owned_books.iter().map(|b| (b.id, b)).collect();
    for anno in highlights_of(snap) {
        if let Some(book) = books.get(&anno.book_id) {
            want(book.cover_path.as_deref(), HL_COVER_W, HL_COVER_H, &mut covers);
        }
    }
    covers
}

fn spawn_author_fetch(model: &AuthorPageModel, sender: &ComponentSender<AuthorPageModel>) {
    // A0 step 4. The old version posted straight into the input sender from
    // the worker, which happens to be safe (relm4 senders are `Send`) but left
    // the page with no way to be told to stop. Going through the seam means
    // this fetch is registered and gets cancelled with everything else when
    // the window closes.
    let catalog = model.service.catalog().clone();
    let author_name = model.requested_name.clone();
    let owned_books = model
        .snapshot
        .as_ref()
        .map(|snap| snap.owned_books.clone())
        .unwrap_or_default();
    let tx = sender.input_sender().clone();
    crate::tasks::spawn(
        "Fetching author info",
        move |_reporter| author::fetch_and_cache_author(&catalog, &author_name, &owned_books),
        |_update| {},
        move |result| {
            let _ = tx.send(AuthorPageMsg::Fetched(Box::new(result)));
        },
    );
}

// ---------------------------------------------------------------------------
// Create shelf dialog (§31, decision 113)
// ---------------------------------------------------------------------------

fn present_create_shelf_dialog(
    anchor: &gtk::Overlay,
    model: &AuthorPageModel,
    sender: &ComponentSender<AuthorPageModel>,
) {
    let Some(snap) = &model.snapshot else { return };
    let author = display_name(model);
    let requested_name = model.requested_name.clone();
    let owned_ids: Vec<i64> = snap.owned_books.iter().map(|b| b.id).collect();
    let service = model.service.clone();

    let ruleset = RuleSet {
        match_mode: "all".to_string(),
        rules: vec![Rule::new(RuleField::Author, RuleOp::Is, author.clone())],
    };
    let smart_count = service
        .catalog()
        .count_matching_rules(&ruleset)
        .unwrap_or(owned_ids.len());
    let owned_count = owned_ids.len();

    let shell = dialog_shell(DialogSize::Form, Some(&format!("for {author}")));

    let name_field = text_field("Name", &author, &author);
    name_field.entry().set_sensitive(false);
    shell.body().append(&name_field.widget());

    // "Name the shelf after the author": on = the field is disabled and
    // the shelf follows a future author rename (recorded in the pref).
    let switch_row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    switch_row.set_margin_top(16);
    let switch_text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    switch_text.set_hexpand(true);
    let switch_label = gtk::Label::new(Some("Name the shelf after the author"));
    switch_label.add_css_class("kalam-ui-body");
    switch_label.set_halign(gtk::Align::Start);
    switch_label.set_xalign(0.0);
    switch_text.append(&switch_label);
    let switch_caption = gtk::Label::new(Some("Renamed automatically if the author is renamed."));
    switch_caption.add_css_class("kalam-ui-meta");
    switch_caption.set_halign(gtk::Align::Start);
    switch_caption.set_xalign(0.0);
    switch_text.append(&switch_caption);
    switch_row.append(&switch_text);

    let follow_switch = gtk::Switch::new();
    follow_switch.set_active(true);
    follow_switch.set_valign(gtk::Align::Center);
    set_accessible_label(&follow_switch, "Name the shelf after the author");
    let field_for_switch = name_field.entry();
    let author_for_switch = author.clone();
    follow_switch.connect_active_notify(move |sw| {
        let on = sw.is_active();
        field_for_switch.set_sensitive(!on);
        if on {
            field_for_switch.set_text(&author_for_switch);
        }
    });
    switch_row.append(&follow_switch);
    shell.body().append(&switch_row);

    let smart = Rc::new(Cell::new(true));
    let explanation = gtk::Label::new(None);
    explanation.add_css_class("kalam-ui-meta");
    explanation.set_halign(gtk::Align::Start);
    explanation.set_xalign(0.0);
    explanation.set_wrap(true);
    explanation.set_max_width_chars(52);
    explanation.set_margin_top(8);
    let smart_text = format!(
        "Books by {author} join automatically, including future imports. {smart_count} book{} match now.",
        if smart_count == 1 { "" } else { "s" }
    );
    let manual_text = format!(
        "Adds the {owned_count} book{} you own now. New books are not added automatically.",
        if owned_count == 1 { "" } else { "s" }
    );
    explanation.set_text(&smart_text);

    let type_caption = gtk::Label::new(Some("TYPE"));
    type_caption.add_css_class("kalam-ui-field-label");
    type_caption.set_halign(gtk::Align::Start);
    type_caption.set_margin_top(16);
    shell.body().append(&type_caption);

    let explanation_for_seg = explanation.clone();
    let smart_for_seg = smart.clone();
    let type_switch = segmented_control(
        &["Smart", "Manual"],
        0,
        SegmentedSize::Small,
        move |index| {
            let is_smart = index == 0;
            smart_for_seg.set(is_smart);
            explanation_for_seg.set_text(if is_smart {
                &smart_text
            } else {
                &manual_text
            });
        },
    );
    let type_widget = type_switch.widget();
    type_widget.set_halign(gtk::Align::Start);
    type_widget.set_margin_top(6);
    shell.body().append(&type_widget);
    shell.body().append(&explanation);

    let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
    let slot_for_cancel = slot.clone();
    shell.footer().append(&boxed_ghost_button("Cancel", move || {
        if let Some(dialog) = slot_for_cancel.borrow().clone() {
            dialog.close();
        }
    }));

    let slot_for_create = slot.clone();
    let s = sender.input_sender().clone();
    shell.footer().append(&primary_button("Create shelf", move || {
        let follow = follow_switch.is_active();
        let name = if follow {
            author.clone()
        } else {
            name_field.value().trim().to_string()
        };
        if name.is_empty() {
            crate::notify::error("Name the shelf", "The shelf needs a name first.");
            return;
        }
        let catalog = service.catalog();
        match catalog.shelf_name_taken(&name, None) {
            Ok(true) => {
                crate::notify::error(
                    "That shelf name is taken",
                    "Pick a different name for this shelf.",
                );
                return;
            }
            Ok(false) => {}
            Err(err) => {
                crate::notify::error("Could not check the shelf name", &err.to_string());
                return;
            }
        }
        let created = if smart.get() {
            catalog.create_shelf(&name, ShelfKind::Smart, "", &ruleset.to_json())
        } else {
            catalog.create_shelf(&name, ShelfKind::Manual, "", "")
        };
        let shelf_id = match created {
            Ok(id) => id,
            Err(err) => {
                crate::notify::error("Could not create the shelf", &err.to_string());
                return;
            }
        };
        if !smart.get() {
            for book_id in &owned_ids {
                let _ = catalog.add_book_to_shelf(shelf_id, *book_id);
            }
        }
        catalog.set_pref(
            &format!(
                "{AUTHOR_SHELF_PREF_PREFIX}{}",
                crate::author::normalize_author_name(&requested_name)
            ),
            &format!("{shelf_id}{}", if follow { ":linked" } else { "" }),
        );
        crate::notify::success("Shelf created", &name);
        if let Some(dialog) = slot_for_create.borrow().clone() {
            dialog.close();
        }
        let _ = s.send(AuthorPageMsg::Refresh);
    }));

    match present_dialog(anchor, "Create shelf", DialogExit::OwnButtons, &shell) {
        Some(dialog) => *slot.borrow_mut() = Some(dialog),
        None => crate::notify::error(
            "Could not open the dialog",
            "The create-shelf dialog could not be opened.",
        ),
    }
}

// ---------------------------------------------------------------------------
// Annotations dialog, author scope (decision 115)
// ---------------------------------------------------------------------------

fn author_dialog_scroll(child: &gtk::Box) -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::External);
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_child(Some(child));
    scroll
}

fn author_annotations_empty(host: &gtk::Box, text: &str) {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-ui-meta");
    label.set_halign(gtk::Align::Start);
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.set_max_width_chars(58);
    host.append(&label);
}

fn book_heading(host: &gtk::Box, title: &str) {
    let heading = gtk::Label::new(Some(&title.to_uppercase()));
    heading.add_css_class("kalam-ui-label");
    heading.set_halign(gtk::Align::Start);
    // Wrap, not ellipsize: a non-wrapping ellipsized label asks for only
    // its minimum width, which cut "CHART OF SMALL HOURS" mid-word well
    // inside the dialog's measure.
    heading.set_wrap(true);
    heading.set_xalign(0.0);
    heading.set_margin_top(4);
    host.append(&heading);
}

/// Group dialog items under book headings, books in title order.
fn group_by_book<'a, T>(
    items: &[&'a T],
    book_of: impl Fn(&T) -> i64,
    titles: &HashMap<i64, String>,
) -> Vec<(String, Vec<&'a T>)> {
    let mut by_id: BTreeMap<i64, Vec<&T>> = BTreeMap::new();
    for item in items {
        by_id.entry(book_of(item)).or_default().push(item);
    }
    let mut groups: Vec<(String, Vec<&T>)> = by_id
        .into_iter()
        .map(|(book_id, items)| {
            (
                titles
                    .get(&book_id)
                    .cloned()
                    .unwrap_or_else(|| "Unknown book".to_string()),
                items,
            )
        })
        .collect();
    groups.sort_by(|a, b| a.0.cmp(&b.0));
    groups
}

fn present_author_annotations_dialog(
    anchor: &gtk::Overlay,
    model: &AuthorPageModel,
    sender: &ComponentSender<AuthorPageModel>,
) {
    let Some(snap) = &model.snapshot else { return };
    let author = display_name(model);
    let service = model.service.clone();
    let titles: HashMap<i64, String> = snap
        .owned_books
        .iter()
        .map(|book| (book.id, book.title.clone()))
        .collect();

    let shell = dialog_shell(DialogSize::List, Some(&author));

    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.add_named(
        &author_highlights_page(
            service.clone(),
            Rc::new(RefCell::new(snap.annotations.clone())),
            titles.clone(),
        ),
        Some("highlights"),
    );
    stack.add_named(
        &author_bookmarks_page(
            service.clone(),
            Rc::new(RefCell::new(snap.bookmarks.clone())),
            titles.clone(),
        ),
        Some("bookmarks"),
    );
    stack.add_named(
        &author_words_page(
            service,
            Rc::new(RefCell::new(snap.words.clone())),
            titles,
        ),
        Some("words"),
    );

    let labels = [
        format!("Highlights {}", snap.annotations.len()),
        format!("Bookmarks {}", snap.bookmarks.len()),
        format!("Words {}", snap.words.len()),
    ];
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let stack_for_tabs = stack.clone();
    let tabs = segmented_control(&label_refs, 0, SegmentedSize::Panel, move |index| {
        stack_for_tabs.set_visible_child_name(match index {
            1 => "bookmarks",
            2 => "words",
            _ => "highlights",
        });
    });
    let tabs_widget = tabs.widget();
    tabs_widget.set_halign(gtk::Align::Center);
    shell.body().append(&tabs_widget);
    shell.body().append(&stack);

    let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
    let slot_for_done = slot.clone();
    let s = sender.input_sender().clone();
    shell.footer().append(&primary_button("Done", move || {
        if let Some(dialog) = slot_for_done.borrow().clone() {
            dialog.close();
        }
        let _ = s.send(AuthorPageMsg::Refresh);
    }));

    match present_dialog(anchor, "Annotations", DialogExit::OwnButtons, &shell) {
        Some(dialog) => *slot.borrow_mut() = Some(dialog),
        None => crate::notify::error(
            "Could not open annotations",
            "The dialog could not be opened.",
        ),
    }
}

fn author_highlights_page(
    service: LibraryService,
    annotations: Rc<RefCell<Vec<Annotation>>>,
    titles: HashMap<i64, String>,
) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_vexpand(true);
    let filter = Rc::new(Cell::new(None::<HighlightColor>));
    let list_host = gtk::Box::new(gtk::Orientation::Vertical, 10);

    // Self-referential rebuild, the panels' established pattern: delete
    // buttons and the colour filter re-run it through the holder slot.
    let holder: crate::pages::SelfRebuild = Rc::new(RefCell::new(None));
    let rebuild: Rc<dyn Fn()> = Rc::new({
        let host = list_host.clone();
        let annotations = annotations.clone();
        let filter = filter.clone();
        let titles = titles.clone();
        let service = service.clone();
        let holder = holder.clone();
        move || {
            clear_box(&host);
            let current = annotations.borrow();
            let shown: Vec<&Annotation> = current
                .iter()
                .filter(|a| {
                    filter
                        .get()
                        .is_none_or(|c| HighlightColor::from_str_lossy(&a.color) == c)
                })
                .collect();
            if shown.is_empty() {
                author_annotations_empty(
                    &host,
                    if filter.get().is_some() {
                        "No highlights in this colour."
                    } else {
                        "No highlights yet — select some text in the reader."
                    },
                );
                return;
            }
            for (book_title, items) in group_by_book(&shown, |a| a.book_id, &titles) {
                book_heading(&host, &book_title);
                for anno in items {
                    host.append(&hairline());
                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                    let bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
                    bar.add_css_class("kalam-hl-bar");
                    bar.add_css_class(hl_color_class(&anno.color));
                    bar.set_valign(gtk::Align::Fill);
                    row.append(&bar);

                    let info = gtk::Box::new(gtk::Orientation::Vertical, 3);
                    info.set_hexpand(true);
                    let text = anno.text_excerpt.trim();
                    if !text.is_empty() {
                        let quote = gtk::Label::new(Some(text));
                        quote.add_css_class("kalam-bookpage-annotation-quote");
                        quote.set_halign(gtk::Align::Start);
                        quote.set_wrap(true);
                        quote.set_xalign(0.0);
                        quote.set_max_width_chars(58);
                        info.append(&quote);
                    }
                    let chapter = anno.chapter_index.max(0) + 1;
                    let day = anno
                        .created_at
                        .get(..10)
                        .map(pretty_day)
                        .unwrap_or_else(|| anno.created_at.clone());
                    let mut meta_parts = vec![format!("Chapter {chapter}"), day];
                    let note = anno.note.trim();
                    if !note.is_empty() {
                        let short: String = note.chars().take(90).collect();
                        meta_parts.push(short);
                    }
                    let meta = gtk::Label::new(Some(&meta_parts.join(" · ")));
                    meta.add_css_class("kalam-ui-meta");
                    meta.set_halign(gtk::Align::Start);
                    meta.set_wrap(true);
                    meta.set_xalign(0.0);
                    meta.set_max_width_chars(58);
                    info.append(&meta);
                    row.append(&info);

                    let id = anno.id;
                    let service = service.clone();
                    let annotations = annotations.clone();
                    let holder = holder.clone();
                    row.append(&icon_button(
                        "user-trash-symbolic",
                        "Delete highlight",
                        32,
                        false,
                        move || {
                            match service.catalog().delete_annotation(id) {
                                Ok(()) => {
                                    annotations.borrow_mut().retain(|a| a.id != id);
                                    crate::notify::compact("Annotation deleted", "");
                                }
                                Err(err) => crate::notify::error(
                                    "Could not delete the annotation",
                                    &err.to_string(),
                                ),
                            }
                            if let Some(refresh) = holder.borrow().clone() {
                                refresh();
                            }
                        },
                    ));
                    host.append(&row);
                }
            }
        }
    });
    *holder.borrow_mut() = Some(rebuild.clone());

    // An All chip and the five colour dots (decision 115); tapping the
    // active dot again clears back to All.
    let filter_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let clear_actives = |row: &gtk::Box| {
        let mut child = row.first_child();
        while let Some(widget) = child {
            let next = widget.next_sibling();
            widget.remove_css_class("active");
            child = next;
        }
    };

    let all_chip = filter_chip("All", true, ChipSize::Standard, {
        let filter = filter.clone();
        let holder = holder.clone();
        let row = filter_row.clone();
        move |active| {
            if active {
                filter.set(None);
                // Dots are plain buttons: drop their active class.
                let mut child = row.first_child();
                while let Some(widget) = child {
                    let next = widget.next_sibling();
                    if widget.css_classes().iter().any(|c| c == "k-color-dot") {
                        widget.remove_css_class("active");
                    }
                    child = next;
                }
                if let Some(refresh) = holder.borrow().clone() {
                    refresh();
                }
            }
        }
    });
    filter_row.append(&all_chip);

    for &color in HighlightColor::SOFT_FIVE {
        let dot = gtk::Button::new();
        dot.add_css_class("k-color-dot");
        dot.add_css_class(&format!("k-color-dot-{}", color.as_str()));
        dot.set_size_request(24, 24);
        dot.set_hexpand(false);
        dot.set_halign(gtk::Align::Start);
        dot.set_valign(gtk::Align::Center);
        dot.set_tooltip_text(Some(&format!("Show {} highlights", color.as_str())));
        set_accessible_label(&dot, &format!("Show {} highlights", color.as_str()));
        let filter = filter.clone();
        let holder = holder.clone();
        let all_chip = all_chip.clone();
        let row = filter_row.clone();
        dot.connect_clicked(move |clicked| {
            let next = if filter.get() == Some(color) {
                None
            } else {
                Some(color)
            };
            filter.set(next);
            clear_actives(&row);
            if next.is_some() {
                clicked.add_css_class("active");
                all_chip.set_active(false);
            } else {
                all_chip.set_active(true);
            }
            if let Some(refresh) = holder.borrow().clone() {
                refresh();
            }
        });
        filter_row.append(&dot);
    }
    root.append(&filter_row);
    root.append(&author_dialog_scroll(&list_host));

    rebuild();
    root
}

fn author_bookmarks_page(
    service: LibraryService,
    bookmarks: Rc<RefCell<Vec<crate::db::ReadingBookmark>>>,
    titles: HashMap<i64, String>,
) -> gtk::Box {
    let list_host = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let holder: crate::pages::SelfRebuild = Rc::new(RefCell::new(None));
    let rebuild: Rc<dyn Fn()> = Rc::new({
        let host = list_host.clone();
        let bookmarks_state = bookmarks.clone();
        let service = service.clone();
        let holder = holder.clone();
        move || {
            clear_box(&host);
            let current = bookmarks_state.borrow();
            if current.is_empty() {
                author_annotations_empty(&host, "No bookmarks in this author's books yet.");
                return;
            }
            let refs: Vec<&crate::db::ReadingBookmark> = current.iter().collect();
            for (book_title, items) in group_by_book(&refs, |b| b.book_id, &titles) {
                book_heading(&host, &book_title);
                for bookmark in items {
                    host.append(&hairline());
                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                    let icon = gtk::Image::from_icon_name("bookmark-new-symbolic");
                    icon.set_pixel_size(15);
                    icon.set_valign(gtk::Align::Center);
                    row.append(&icon);

                    let chapter = format!("Chapter {}", bookmark.chapter_index.max(0) + 1);
                    let label = bookmark.label.trim();
                    let info = gtk::Box::new(gtk::Orientation::Vertical, 3);
                    info.set_hexpand(true);
                    let title = gtk::Label::new(Some(if label.is_empty() {
                        chapter.as_str()
                    } else {
                        label
                    }));
                    title.add_css_class("kalam-ui-body");
                    title.set_halign(gtk::Align::Start);
                    title.set_wrap(true);
                    title.set_xalign(0.0);
                    title.set_max_width_chars(58);
                    info.append(&title);
                    let pct = (bookmark.fraction.clamp(0.0, 1.0) * 100.0).round() as i64;
                    let day = bookmark
                        .created_at
                        .get(..10)
                        .map(pretty_day)
                        .unwrap_or_else(|| bookmark.created_at.clone());
                    let meta_text = if label.is_empty() {
                        format!("{day} · {pct}%")
                    } else {
                        format!("{chapter} · {day} · {pct}%")
                    };
                    let meta = gtk::Label::new(Some(&meta_text));
                    meta.add_css_class("kalam-ui-meta");
                    meta.set_halign(gtk::Align::Start);
                    meta.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    info.append(&meta);
                    row.append(&info);

                    let id = bookmark.id;
                    let service = service.clone();
                    let bookmarks_state = bookmarks_state.clone();
                    let holder = holder.clone();
                    row.append(&icon_button(
                        "user-trash-symbolic",
                        "Delete bookmark",
                        32,
                        false,
                        move || {
                            match service.catalog().delete_reading_bookmark(id) {
                                Ok(()) => {
                                    bookmarks_state.borrow_mut().retain(|b| b.id != id);
                                    crate::notify::compact("Bookmark deleted", "");
                                }
                                Err(err) => crate::notify::error(
                                    "Could not delete the bookmark",
                                    &err.to_string(),
                                ),
                            }
                            if let Some(refresh) = holder.borrow().clone() {
                                refresh();
                            }
                        },
                    ));
                    host.append(&row);
                }
            }
        }
    });
    *holder.borrow_mut() = Some(rebuild.clone());
    rebuild();

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.set_vexpand(true);
    root.append(&author_dialog_scroll(&list_host));
    root
}

fn author_words_page(
    service: LibraryService,
    words: Rc<RefCell<Vec<crate::db::SavedWord>>>,
    titles: HashMap<i64, String>,
) -> gtk::Box {
    let list_host = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let holder: crate::pages::SelfRebuild = Rc::new(RefCell::new(None));
    let rebuild: Rc<dyn Fn()> = Rc::new({
        let host = list_host.clone();
        let words_state = words.clone();
        let service = service.clone();
        let holder = holder.clone();
        move || {
            clear_box(&host);
            let current = words_state.borrow();
            if current.is_empty() {
                author_annotations_empty(&host, "No saved words from this author's books yet.");
                return;
            }
            let refs: Vec<&crate::db::SavedWord> = current.iter().collect();
            for (book_title, items) in
                group_by_book(&refs, |w| w.book_id.unwrap_or_default(), &titles)
            {
                book_heading(&host, &book_title);
                for word in items {
                    host.append(&hairline());
                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                    let info = gtk::Box::new(gtk::Orientation::Vertical, 3);
                    info.set_hexpand(true);
                    let headword = gtk::Label::new(Some(&word.word));
                    headword.add_css_class("kalam-ui-card-title");
                    headword.set_halign(gtk::Align::Start);
                    headword.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    info.append(&headword);

                    let meaning = word
                        .definition
                        .lines()
                        .map(str::trim)
                        .find(|line| !line.is_empty())
                        .unwrap_or("");
                    if !meaning.is_empty() {
                        let meaning_label = gtk::Label::new(Some(meaning));
                        meaning_label.add_css_class("kalam-ui-meta");
                        meaning_label.set_halign(gtk::Align::Start);
                        meaning_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                        meaning_label.set_max_width_chars(58);
                        info.append(&meaning_label);
                    }
                    let mut meta_parts: Vec<String> = Vec::new();
                    if let Some(chapter_index) = word.chapter_index {
                        meta_parts.push(format!("Chapter {}", chapter_index.max(0) + 1));
                    }
                    if word.known {
                        meta_parts.push("Known".to_string());
                    }
                    if !meta_parts.is_empty() {
                        let meta = gtk::Label::new(Some(&meta_parts.join(" · ")));
                        meta.add_css_class("kalam-ui-caption");
                        meta.set_halign(gtk::Align::Start);
                        meta.set_ellipsize(gtk::pango::EllipsizeMode::End);
                        info.append(&meta);
                    }
                    row.append(&info);

                    let id = word.id;
                    let word_text = word.word.clone();
                    let service = service.clone();
                    let words_state = words_state.clone();
                    let holder = holder.clone();
                    row.append(&icon_button(
                        "user-trash-symbolic",
                        "Delete saved word",
                        32,
                        false,
                        move || {
                            match service.catalog().delete_saved_word(id) {
                                Ok(()) => {
                                    words_state.borrow_mut().retain(|w| w.id != id);
                                    crate::notify::compact("Saved word deleted", &word_text);
                                }
                                Err(err) => crate::notify::error(
                                    "Could not delete the saved word",
                                    &err.to_string(),
                                ),
                            }
                            if let Some(refresh) = holder.borrow().clone() {
                                refresh();
                            }
                        },
                    ));
                    host.append(&row);
                }
            }
        }
    });
    *holder.borrow_mut() = Some(rebuild.clone());
    rebuild();

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.set_vexpand(true);
    root.append(&author_dialog_scroll(&list_host));
    root
}
