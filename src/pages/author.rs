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
//! Edit author and Merge into another author (decision 112), Create
//! shelf (§31), and the author-scoped Annotations dialog
//! (decision 115).

use crate::author::{self, display_author_name, initials, normalize_author_name, sort_author_name};
use crate::db::{
    Annotation, AuthorProfile, Catalog, HighlightColor, ShelfKind, AUTHOR_FIELD_ALIASES,
    AUTHOR_FIELD_BIO, AUTHOR_FIELD_BIRTH_DATE, AUTHOR_FIELD_DEATH_DATE, AUTHOR_FIELD_NAME,
    AUTHOR_FIELD_PHOTO, AUTHOR_FIELD_SORT_NAME,
};
use crate::models::{Book, BookFormat};
use crate::pages::history::pretty_day;
use crate::service::{
    AuthorEditInput, AuthorEntrySnapshot, AuthorGroupSnapshot, AuthorIndexEntry,
    AuthorPageSnapshot, AuthorPortraitEdit, LibraryService, AUTHOR_SHELF_PREF_PREFIX,
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
    OpenEditAuthor,
    OpenMergeAuthor,
    /// An edit or merge renamed the identity this page is about;
    /// follow it to the new name and re-read.
    IdentityChanged {
        name: String,
    },
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
    /// The joined outline around the open card + panel, stroked in
    /// Cairo. A CSS border with differing sides is not an option:
    /// GskBorderNode's mesh-pattern path under the Cairo renderer
    /// drops the right and bottom sides (GTK 4.22.4, seen in CI), so
    /// the open card and panel are borderless and this draws the one
    /// rounded rect around both.
    panel_outline: gtk::DrawingArea,
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

        // The joined outline around the open card and its panel (see
        // the field's note): one rounded rect, top corners on the
        // card, bottom corners on the panel, straight sides through
        // the seam. The anchoring tick in update_panel sizes it; the
        // colour is the CSS class's `color`, read at draw time.
        let panel_outline = gtk::DrawingArea::new();
        panel_outline.add_css_class("kalam-authorpage-panel-outline");
        panel_outline.set_halign(gtk::Align::Start);
        panel_outline.set_valign(gtk::Align::Start);
        panel_outline.set_can_target(false);
        panel_outline.set_visible(false);
        panel_outline.set_draw_func(|area, cr, width, height| {
            let (w, h) = (width as f64, height as f64);
            if w < 2.0 || h < 2.0 {
                return;
            }
            // §26/§27 radius 14, stroke centred on the edge so the
            // outer pixel row matches the closed card's CSS border.
            let r = 14.0_f64.min(w / 2.0).min(h / 2.0);
            let (x, y, w, h) = (0.5, 0.5, w - 1.0, h - 1.0);
            use std::f64::consts::{FRAC_PI_2, PI};
            cr.new_sub_path();
            cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
            cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
            cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
            cr.arc(x + r, y + r, r, PI, 3.0 * FRAC_PI_2);
            cr.close_path();
            let colour = area.color();
            cr.set_source_rgba(
                colour.red() as f64,
                colour.green() as f64,
                colour.blue() as f64,
                colour.alpha() as f64,
            );
            cr.set_line_width(1.0);
            let _ = cr.stroke();
        });

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
            panel_outline: panel_outline.clone(),
            menu_open: menu_open.clone(),
        };
        let widgets = view_output!();

        root.add_overlay(&panel_revealer);
        // Above the card and panel it outlines; can_target(false), so
        // it never eats a click meant for either.
        root.add_overlay(&panel_outline);
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
                        Ok("edit-author") => sender.input(AuthorPageMsg::OpenEditAuthor),
                        Ok("merge-author") => sender.input(AuthorPageMsg::OpenMergeAuthor),
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
            AuthorPageMsg::OpenEditAuthor => {
                present_edit_author_dialog(root, self, &sender);
                rebuild = false;
            }
            AuthorPageMsg::OpenMergeAuthor => {
                present_merge_author_dialog(root, self, &sender);
                rebuild = false;
            }
            AuthorPageMsg::IdentityChanged { name } => {
                self.requested_name = name;
                self.snapshot = None;
                self.open_group_title = None;
                request_snapshot(&self.service, &self.requested_name, &sender);
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
                    // text, not guessed from a character count (a count
                    // misjudges at real font metrics). The ask waits for
                    // the label's width to hold still across two frames:
                    // at map time the layout may still be computed for a
                    // transient narrow allocation and report a phantom
                    // ellipsis (CI caught a More under a complete bio).
                    let more = gtk::Button::with_label("More");
                    more.add_css_class("kalam-authorpage-more");
                    more.set_halign(gtk::Align::Start);
                    more.set_visible(false);
                    let more_for_tick = more.clone();
                    let label_for_tick = bio_label.clone();
                    let stable = Rc::new(Cell::new((-1i32, 0u32)));
                    bio_label.add_tick_callback(move |_widget, _clock| {
                        let (last_w, tries) = stable.get();
                        let w = label_for_tick.width();
                        if (w <= 1 || w != last_w) && tries < 180 {
                            stable.set((w, tries + 1));
                            return gtk::glib::ControlFlow::Continue;
                        }
                        more_for_tick
                            .set_visible(label_for_tick.layout().is_ellipsized());
                        gtk::glib::ControlFlow::Break
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
        let menu = menu_popover(MenuWidth::Wide);

        let s = sender.clone();
        menu.add_item("Edit author…", move || {
            s.input(AuthorPageMsg::OpenEditAuthor);
        });

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
        menu.add_separator();

        let s = sender.clone();
        menu.add_item("Merge into another author…", move || {
            s.input(AuthorPageMsg::OpenMergeAuthor);
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
            self.panel_outline.set_visible(false);
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

        // Anchor under the card once it has a real allocation — and
        // keep anchoring it. This waits on the frame clock, not an
        // idle: idles can burn through any retry cap before the first
        // frame is even allocated, and bounds on an unallocated card
        // read the origin (the panel landed over the header twice
        // before CI taught us). It stays on the clock while the
        // panel stands because the card moves under it — page scroll,
        // window resize, covers settling — and a one-shot anchor left
        // the panel floating beside its card instead of joined to it
        // (§27). A rebuild destroys the old card and this callback
        // with it, then re-anchors from the new one; closing the
        // panel hides the revealer, which ends the callback too.
        //
        // The anchor must come from compute_bounds: that is the
        // card's border area, the rectangle its outline is drawn in.
        // compute_point / width() / height() speak in the CSS content
        // box, which on this card sits one border + 19px of padding
        // in from every edge — anchoring to it inset the panel 20px
        // on both sides and started it 20px up inside the card, so
        // it read as a smaller floating card (§27 wants the panel to
        // be the card's own lower half, flush at the card's width).
        if let Some(card) = self.card_buttons.get(index).cloned().flatten() {
            let revealer = self.panel_revealer.clone();
            let outline = self.panel_outline.clone();
            let overlay = root.clone();
            let tries = Rc::new(Cell::new(0u32));
            let shown = Rc::new(Cell::new(false));
            let anchored = Rc::new(Cell::new(None::<(i32, i32, i32)>));
            let outlined = Rc::new(Cell::new(None::<(i32, i32, i32, i32)>));
            let outline_colour = Rc::new(Cell::new((0.0f32, 0.0f32, 0.0f32, 0.0f32)));
            card.add_tick_callback(move |widget, _clock| {
                if shown.get() && !revealer.is_visible() {
                    outline.set_visible(false);
                    return gtk::glib::ControlFlow::Break;
                }
                tries.set(tries.get() + 1);
                let bounds = (widget.is_mapped() && widget.width() > 1)
                    .then(|| widget.compute_bounds(&overlay))
                    .flatten();
                if bounds.is_none() && !shown.get() && tries.get() < 180 {
                    return gtk::glib::ControlFlow::Continue;
                }
                if let Some(bounds) = bounds {
                    // Sit on the card's bottom pixel row, not below
                    // it: with both halves borderless the overlap
                    // keeps the panel's square-topped background
                    // flush against the card, and the outline below
                    // draws the side lines straight through the
                    // seam. Only the revealer takes the width
                    // request; one on the panel box too is counted
                    // in its content box and inflates the panel past
                    // the revealer's allocation.
                    let geometry = (
                        bounds.x() as i32,
                        (bounds.y() + bounds.height()) as i32 - 1,
                        bounds.width() as i32,
                    );
                    if anchored.replace(Some(geometry)) != Some(geometry) {
                        revealer.set_margin_start(geometry.0);
                        revealer.set_margin_top(geometry.1);
                        revealer.set_size_request(geometry.2, -1);
                    }
                    // The joined outline spans the card's top to the
                    // panel's (possibly still sliding) bottom: the
                    // revealer's measured height is scaled by the
                    // animation fraction during SlideDown, so its
                    // allocation height is the visible panel height.
                    // The colour is re-read every frame so a palette
                    // switch restrokes without waiting for geometry
                    // to move (a bare `color` change does not queue
                    // a DrawingArea redraw on its own).
                    let rect = (
                        geometry.0,
                        bounds.y() as i32,
                        geometry.2,
                        geometry.1 + revealer.height() - bounds.y() as i32,
                    );
                    let colour = outline.color();
                    let colour = (
                        colour.red(),
                        colour.green(),
                        colour.blue(),
                        colour.alpha(),
                    );
                    let restroke = outline_colour.replace(colour) != colour;
                    if outlined.replace(Some(rect)) != Some(rect) {
                        outline.set_margin_start(rect.0);
                        outline.set_margin_top(rect.1);
                        outline.set_size_request(rect.2, rect.3);
                        outline.queue_draw();
                    } else if restroke {
                        outline.queue_draw();
                    }
                }
                if !shown.get() {
                    revealer.set_visible(true);
                    revealer.set_reveal_child(true);
                    outline.set_visible(true);
                    shown.set(true);
                }
                gtk::glib::ControlFlow::Continue
            });
        } else {
            // No card to anchor to: show the panel where it was, but
            // never a stale outline around some previous card.
            self.panel_outline.set_visible(false);
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
// Edit author dialog (decision 112)
// ---------------------------------------------------------------------------

/// A form label that says when its field is already hand-edited —
/// locked against metadata refresh (decision 48).
fn locked_label(base: &str, profile: Option<&AuthorProfile>, field: &str) -> String {
    if profile.is_some_and(|p| p.manual_fields.iter().any(|f| f == field)) {
        format!("{base} · edited")
    } else {
        base.to_string()
    }
}

/// The neutral initials disc, sized for the slot it stands in.
fn author_initials_disc(name: &str, size: i32) -> gtk::Widget {
    let disc = gtk::Box::new(gtk::Orientation::Vertical, 0);
    disc.add_css_class("kalam-authorpage-portrait-initials");
    disc.set_size_request(size, size);
    disc.set_halign(gtk::Align::Start);
    let label = gtk::Label::new(Some(&initials(name)));
    label.add_css_class("kalam-authorpage-initials-text");
    label.set_halign(gtk::Align::Center);
    label.set_valign(gtk::Align::Center);
    label.set_vexpand(true);
    disc.append(&label);
    disc.upcast()
}

/// The aliases the form shows: the profile's, minus the canonical
/// and sort spellings the name fields already own.
fn alias_field_initial(
    profile: Option<&AuthorProfile>,
    author: &str,
    sort_name: &str,
    current_name: &str,
) -> String {
    let Some(profile) = profile else {
        return String::new();
    };
    let excluded: HashSet<String> = [
        normalize_author_name(author),
        normalize_author_name(sort_name),
        normalize_author_name(current_name),
        profile.normalized_name.trim().to_string(),
    ]
    .into_iter()
    .filter(|k| !k.is_empty())
    .collect();
    let mut seen = HashSet::new();
    profile
        .aliases
        .iter()
        .map(|a| a.trim())
        .filter(|a| !a.is_empty())
        .filter(|a| {
            let key = normalize_author_name(a);
            !excluded.contains(&key) && seen.insert(key)
        })
        .collect::<Vec<_>>()
        .join("; ")
}

fn present_edit_author_dialog(
    anchor: &gtk::Overlay,
    model: &AuthorPageModel,
    sender: &ComponentSender<AuthorPageModel>,
) {
    let Some(snap) = &model.snapshot else { return };
    let profile = snap.profile.clone();
    let current_name = model.requested_name.clone();
    let author = display_name(model);
    let service = model.service.clone();
    let effective_sort = profile
        .as_ref()
        .map(|p| p.sort_name.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| sort_author_name(&author));

    let shell = dialog_shell(DialogSize::Form, Some(&author));

    // --- Portrait -----------------------------------------------------
    let portrait_state: Rc<RefCell<AuthorPortraitEdit>> =
        Rc::new(RefCell::new(AuthorPortraitEdit::Keep));
    // The chosen image, decoded on the worker when it was picked, so
    // the preview only wraps pixels in a texture.
    let portrait_preview: Rc<RefCell<Option<crate::preload::DecodedPixels>>> =
        Rc::new(RefCell::new(None));
    let portrait_holder: crate::pages::SelfRebuild = Rc::new(RefCell::new(None));
    let portrait_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
    portrait_host.set_size_request(PORTRAIT, PORTRAIT);
    portrait_host.set_valign(gtk::Align::Start);
    let portrait_status = gtk::Label::new(None);
    portrait_status.add_css_class("kalam-ui-meta");
    portrait_status.set_halign(gtk::Align::Start);
    portrait_status.set_wrap(true);
    portrait_status.set_xalign(0.0);
    portrait_status.set_max_width_chars(44);

    let anchor_for_choose = anchor.clone();
    let state_for_choose = portrait_state.clone();
    let preview_for_choose = portrait_preview.clone();
    let holder_for_choose = portrait_holder.clone();
    let choose_button = boxed_ghost_button("Choose…", move || {
        let dialog = gtk::FileDialog::builder()
            .title("Choose a portrait image")
            .modal(true)
            .build();
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Images"));
        for suffix in ["png", "jpg", "jpeg", "webp", "gif"] {
            filter.add_suffix(suffix);
        }
        let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        dialog.set_filters(Some(&filters));
        let window = anchor_for_choose
            .root()
            .and_then(|r| r.downcast::<gtk::Window>().ok())
            .or_else(|| {
                relm4::main_application()
                    .active_window()
                    .and_then(|w| w.downcast::<gtk::Window>().ok())
            });
        let state = state_for_choose.clone();
        let preview = preview_for_choose.clone();
        let holder = holder_for_choose.clone();
        dialog.open(window.as_ref(), gtk::gio::Cancellable::NONE, move |res| {
            let Ok(file) = res else { return };
            let Some(path) = file.path() else { return };
            crate::tasks::spawn_internal(
                "Reading portrait",
                move |_reporter| {
                    let bytes = std::fs::read(&path)
                        .map_err(|e| format!("Could not read that file: {e}"))?;
                    if bytes.is_empty() {
                        return Err("That image file is empty.".to_string());
                    }
                    if bytes.len() > 20 * 1024 * 1024 {
                        return Err("That image is larger than 20 MB.".to_string());
                    }
                    let pixels =
                        crate::preload::decode_rgba_bytes(&bytes, PORTRAIT, PORTRAIT)
                            .ok_or_else(|| {
                                "That file is not an image Kalam can read.".to_string()
                            })?;
                    Ok((bytes, pixels))
                },
                |_| {},
                move |result| match result {
                    Ok((bytes, pixels)) => {
                        *preview.borrow_mut() = Some(pixels);
                        *state.borrow_mut() = AuthorPortraitEdit::Replace(bytes);
                        if let Some(refresh) = holder.borrow().clone() {
                            refresh();
                        }
                    }
                    Err(err) => {
                        crate::notify::error("Could not use that image", &err);
                    }
                },
            );
        });
    });

    let profile_for_remove = profile.clone();
    let state_for_remove = portrait_state.clone();
    let holder_for_remove = portrait_holder.clone();
    let remove_button = boxed_ghost_button("Remove", move || {
        let next = match &*state_for_remove.borrow() {
            AuthorPortraitEdit::Replace(_)
                if profile_for_remove
                    .as_ref()
                    .is_none_or(|p| p.photo_file.is_none()) =>
            {
                AuthorPortraitEdit::Keep
            }
            _ => AuthorPortraitEdit::Remove,
        };
        *state_for_remove.borrow_mut() = next;
        if let Some(refresh) = holder_for_remove.borrow().clone() {
            refresh();
        }
    });

    {
        let host = portrait_host.clone();
        let status = portrait_status.clone();
        let state = portrait_state.clone();
        let preview = portrait_preview.clone();
        let profile = profile.clone();
        let author = author.clone();
        let remove = remove_button.clone();
        let refresh: Rc<dyn Fn()> = Rc::new(move || {
            clear_box(&host);
            match &*state.borrow() {
                AuthorPortraitEdit::Keep => {
                    match profile.as_ref().and_then(|p| p.photo_path.as_deref()) {
                        Some(path) => {
                            let widget = cover_widget(Some(path), PORTRAIT, PORTRAIT);
                            widget.add_css_class("kalam-authorpage-portrait");
                            widget.set_size_request(PORTRAIT, PORTRAIT);
                            host.append(&widget);
                            status.set_text(
                                if profile.as_ref().is_some_and(|p| {
                                    p.manual_fields.iter().any(|f| f == AUTHOR_FIELD_PHOTO)
                                }) {
                                    "Your portrait. Metadata fetches leave it alone."
                                } else {
                                    "Portrait from Open Library."
                                },
                            );
                        }
                        None => {
                            host.append(&author_initials_disc(&author, PORTRAIT));
                            status.set_text("Initials are shown until a portrait is set.");
                        }
                    }
                    remove.set_sensitive(
                        profile.as_ref().is_some_and(|p| p.photo_file.is_some()),
                    );
                }
                AuthorPortraitEdit::Remove => {
                    host.append(&author_initials_disc(&author, PORTRAIT));
                    status.set_text("The portrait will be removed when you save.");
                    remove.set_sensitive(false);
                }
                AuthorPortraitEdit::Replace(_) => {
                    let preview = preview.borrow();
                    match preview.as_ref() {
                        Some(pixels)
                            if pixels.width > 0
                                && pixels.height > 0
                                && pixels.rgba.len()
                                    == (pixels.width as usize) * (pixels.height as usize) * 4 =>
                        {
                            let bytes = gtk::glib::Bytes::from(&pixels.rgba[..]);
                            let texture: gtk::gdk::Texture = gtk::gdk::MemoryTexture::new(
                                pixels.width,
                                pixels.height,
                                gtk::gdk::MemoryFormat::R8g8b8a8,
                                &bytes,
                                (pixels.width as usize) * 4,
                            )
                            .upcast();
                            let picture = gtk::Picture::for_paintable(&texture);
                            picture.add_css_class("kalam-authorpage-portrait");
                            picture.set_size_request(PORTRAIT, PORTRAIT);
                            picture.set_content_fit(gtk::ContentFit::Cover);
                            host.append(&picture);
                        }
                        _ => host.append(&author_initials_disc(&author, PORTRAIT)),
                    }
                    drop(preview);
                    status.set_text("New portrait selected — press Save to keep it.");
                    remove.set_sensitive(true);
                }
            }
        });
        *portrait_holder.borrow_mut() = Some(refresh.clone());
        refresh();
    }

    let portrait_row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    portrait_row.append(&portrait_host);
    let portrait_side = gtk::Box::new(gtk::Orientation::Vertical, 6);
    portrait_side.set_hexpand(true);
    portrait_side.set_valign(gtk::Align::Center);
    let portrait_caption = gtk::Label::new(Some(
        &locked_label("Portrait", profile.as_ref(), AUTHOR_FIELD_PHOTO).to_uppercase(),
    ));
    portrait_caption.add_css_class("kalam-ui-field-label");
    portrait_caption.set_halign(gtk::Align::Start);
    portrait_side.append(&portrait_caption);
    portrait_side.append(&portrait_status);
    let portrait_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    portrait_buttons.set_halign(gtk::Align::Start);
    portrait_buttons.append(&choose_button);
    portrait_buttons.append(&remove_button);
    portrait_side.append(&portrait_buttons);
    portrait_row.append(&portrait_side);
    shell.body().append(&portrait_row);

    // --- Name fields ----------------------------------------------------
    let name_field = text_field(
        &locked_label("Name", profile.as_ref(), AUTHOR_FIELD_NAME),
        &author,
        &author,
    );
    name_field.widget().set_margin_top(20);
    shell.body().append(&name_field.widget());

    let sort_field = text_field(
        &locked_label("Sort name", profile.as_ref(), AUTHOR_FIELD_SORT_NAME),
        &effective_sort,
        &effective_sort,
    );
    sort_field.widget().set_margin_top(16);
    shell.body().append(&sort_field.widget());

    // The sort name follows the name until the reader types in it
    // themselves (or already locked one by hand on an earlier edit).
    let sort_dirty = Rc::new(Cell::new(profile.as_ref().is_some_and(|p| {
        p.manual_fields.iter().any(|f| f == AUTHOR_FIELD_SORT_NAME)
    })));
    let updating_sort = Rc::new(Cell::new(false));
    {
        let dirty = sort_dirty.clone();
        let updating = updating_sort.clone();
        sort_field.entry().connect_changed(move |_| {
            if !updating.get() {
                dirty.set(true);
            }
        });
    }
    {
        let dirty = sort_dirty;
        let updating = updating_sort;
        let sort_entry = sort_field.entry();
        name_field.entry().connect_changed(move |entry| {
            if !dirty.get() {
                updating.set(true);
                sort_entry.set_text(&sort_author_name(entry.text().as_ref()));
                updating.set(false);
            }
        });
    }

    let dates_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    dates_row.set_margin_top(16);
    let birth_field = text_field(
        &locked_label("Born", profile.as_ref(), AUTHOR_FIELD_BIRTH_DATE),
        "e.g. 21 October 1929",
        profile.as_ref().map(|p| p.birth_date.as_str()).unwrap_or(""),
    );
    birth_field.widget().set_hexpand(true);
    dates_row.append(&birth_field.widget());
    let death_field = text_field(
        &locked_label("Died", profile.as_ref(), AUTHOR_FIELD_DEATH_DATE),
        "e.g. 22 January 2018",
        profile.as_ref().map(|p| p.death_date.as_str()).unwrap_or(""),
    );
    death_field.widget().set_hexpand(true);
    dates_row.append(&death_field.widget());
    shell.body().append(&dates_row);

    let aliases_field = text_field(
        &locked_label("Also known as", profile.as_ref(), AUTHOR_FIELD_ALIASES),
        "Other names, separated by semicolons",
        &alias_field_initial(profile.as_ref(), &author, &effective_sort, &current_name),
    );
    aliases_field.widget().set_margin_top(16);
    shell.body().append(&aliases_field.widget());

    let bio_caption = gtk::Label::new(Some(
        &locked_label("Biography", profile.as_ref(), AUTHOR_FIELD_BIO).to_uppercase(),
    ));
    bio_caption.add_css_class("kalam-ui-field-label");
    bio_caption.set_halign(gtk::Align::Start);
    bio_caption.set_margin_top(16);
    shell.body().append(&bio_caption);

    let bio_view = gtk::TextView::new();
    bio_view.add_css_class("kalam-author-bio-editor");
    bio_view.set_wrap_mode(gtk::WrapMode::WordChar);
    bio_view.set_accepts_tab(false);
    bio_view.set_left_margin(10);
    bio_view.set_right_margin(10);
    bio_view.set_top_margin(8);
    bio_view.set_bottom_margin(8);
    bio_view
        .buffer()
        .set_text(profile.as_ref().map(|p| p.bio.as_str()).unwrap_or(""));
    let bio_scroll = gtk::ScrolledWindow::builder()
        .min_content_height(132)
        .max_content_height(132)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&bio_view)
        .build();
    bio_scroll.add_css_class("kalam-author-bio-editor-scroll");
    bio_scroll.set_margin_top(6);
    shell.body().append(&bio_scroll);

    let lock_note = gtk::Label::new(Some(
        "Fields you change here stay as you set them when metadata is fetched again.",
    ));
    lock_note.add_css_class("kalam-ui-meta");
    lock_note.set_halign(gtk::Align::Start);
    lock_note.set_wrap(true);
    lock_note.set_xalign(0.0);
    lock_note.set_max_width_chars(52);
    lock_note.set_margin_top(12);
    shell.body().append(&lock_note);

    let status = gtk::Label::new(None);
    status.add_css_class("kalam-ui-meta");
    status.set_halign(gtk::Align::Start);
    status.set_wrap(true);
    status.set_xalign(0.0);
    status.set_max_width_chars(52);
    status.set_margin_top(8);
    shell.body().append(&status);

    // --- Footer ---------------------------------------------------------
    let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
    let footer = shell.footer();
    let slot_for_cancel = slot.clone();
    footer.append(&boxed_ghost_button("Cancel", move || {
        if let Some(dialog) = slot_for_cancel.borrow().clone() {
            dialog.close();
        }
    }));

    let slot_for_save = slot.clone();
    let s = sender.input_sender().clone();
    let footer_for_save = footer.clone();
    footer.append(&primary_button("Save author", move || {
        let name = name_field.value().trim().to_string();
        if name.is_empty() {
            status.set_text("The author needs a name.");
            return;
        }
        let buffer = bio_view.buffer();
        let input = AuthorEditInput {
            name,
            sort_name: sort_field.value(),
            bio: buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .to_string(),
            birth_date: birth_field.value(),
            death_date: death_field.value(),
            aliases: aliases_field
                .value()
                .split([';', '\n'])
                .map(|a| a.trim().to_string())
                .filter(|a| !a.is_empty())
                .collect(),
            portrait: portrait_state.borrow().clone(),
        };
        footer_for_save.set_sensitive(false);
        status.set_text("Saving…");
        let service = service.clone();
        let current_name = current_name.clone();
        let slot = slot_for_save.clone();
        let s = s.clone();
        let footer = footer_for_save.clone();
        let status = status.clone();
        crate::tasks::spawn(
            "Saving author",
            move |_reporter| service.edit_author(&current_name, input),
            |_| {},
            move |result| match result {
                Ok(outcome) => {
                    crate::notify::success(
                        "Author updated",
                        &if outcome.books_updated > 0 {
                            format!(
                                "{} book{} now under {}.",
                                outcome.books_updated,
                                if outcome.books_updated == 1 { "" } else { "s" },
                                outcome.name
                            )
                        } else {
                            outcome.name.clone()
                        },
                    );
                    if let Some(dialog) = slot.borrow().clone() {
                        dialog.close();
                    }
                    let _ = s.send(AuthorPageMsg::IdentityChanged {
                        name: outcome.name,
                    });
                }
                Err(err) => {
                    footer.set_sensitive(true);
                    status.set_text(&err);
                    crate::notify::error("Could not save the author", &err);
                }
            },
        );
    }));

    match present_dialog(anchor, "Edit author", DialogExit::UnsavedInput, &shell) {
        Some(dialog) => *slot.borrow_mut() = Some(dialog),
        None => crate::notify::error(
            "Could not open the dialog",
            "The edit-author dialog could not be opened.",
        ),
    }
}

// ---------------------------------------------------------------------------
// Merge into another author dialog (decision 112)
// ---------------------------------------------------------------------------

fn present_merge_author_dialog(
    anchor: &gtk::Overlay,
    model: &AuthorPageModel,
    sender: &ComponentSender<AuthorPageModel>,
) {
    let Some(snap) = &model.snapshot else { return };
    let source_name = model.requested_name.clone();
    let source_profile = snap.profile.clone();
    let source_display = display_name(model);
    let source_books = snap.owned_books.len();
    let service = model.service.clone();

    let shell = dialog_shell(DialogSize::Form, Some(&source_display));

    let intro = gtk::Label::new(Some(&format!(
        "Move every book by {source_display} to another author. \
         {source_display} stays as an alternate name, so old spellings \
         still find the same author."
    )));
    intro.add_css_class("kalam-ui-meta");
    intro.set_halign(gtk::Align::Start);
    intro.set_wrap(true);
    intro.set_xalign(0.0);
    intro.set_max_width_chars(52);
    shell.body().append(&intro);

    let search_field = text_field("Merge into", "Search authors…", "");
    search_field.widget().set_margin_top(16);
    shell.body().append(&search_field.widget());

    let list_host = gtk::Box::new(gtk::Orientation::Vertical, 6);
    list_host.set_margin_top(8);
    list_host.set_margin_bottom(8);
    list_host.set_margin_start(8);
    list_host.set_margin_end(8);
    let list_scroll = gtk::ScrolledWindow::builder()
        .min_content_height(264)
        .max_content_height(264)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list_host)
        .build();
    list_scroll.add_css_class("kalam-author-merge-scroll");
    list_scroll.set_margin_top(12);
    shell.body().append(&list_scroll);

    let summary = gtk::Label::new(Some("Loading authors…"));
    summary.add_css_class("kalam-ui-meta");
    summary.set_halign(gtk::Align::Start);
    summary.set_wrap(true);
    summary.set_xalign(0.0);
    summary.set_max_width_chars(52);
    summary.set_margin_top(12);
    shell.body().append(&summary);

    let candidates: Rc<RefCell<Vec<AuthorIndexEntry>>> = Rc::new(RefCell::new(Vec::new()));
    let loaded = Rc::new(Cell::new(false));
    let load_error: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
    let selected: Rc<RefCell<Option<AuthorIndexEntry>>> = Rc::new(RefCell::new(None));

    // Footer first: the list rebuild enables Merge when a target is
    // chosen, so it needs the button handle.
    let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
    let footer = shell.footer();
    let slot_for_cancel = slot.clone();
    footer.append(&boxed_ghost_button("Cancel", move || {
        if let Some(dialog) = slot_for_cancel.borrow().clone() {
            dialog.close();
        }
    }));

    let slot_for_merge = slot.clone();
    let s = sender.input_sender().clone();
    let selected_for_merge = selected.clone();
    let source_name_for_merge = source_name.clone();
    let service_for_merge = service.clone();
    let footer_for_merge = footer.clone();
    let summary_for_merge = summary.clone();
    let merge_button = primary_button("Merge authors", move || {
        let Some(target) = selected_for_merge.borrow().clone() else {
            return;
        };
        footer_for_merge.set_sensitive(false);
        summary_for_merge.set_text("Merging…");
        let service = service_for_merge.clone();
        let source_name = source_name_for_merge.clone();
        let slot = slot_for_merge.clone();
        let s = s.clone();
        let footer = footer_for_merge.clone();
        let summary = summary_for_merge.clone();
        crate::tasks::spawn(
            "Merging authors",
            move |_reporter| service.merge_authors(&source_name, &target.name),
            |_| {},
            move |result| match result {
                Ok(outcome) => {
                    crate::notify::success(
                        "Authors merged",
                        &format!(
                            "{} book{} now under {}.",
                            outcome.books_updated,
                            if outcome.books_updated == 1 { "" } else { "s" },
                            outcome.name
                        ),
                    );
                    if let Some(dialog) = slot.borrow().clone() {
                        dialog.close();
                    }
                    let _ = s.send(AuthorPageMsg::IdentityChanged {
                        name: outcome.name,
                    });
                }
                Err(err) => {
                    footer.set_sensitive(true);
                    summary.set_text(&err);
                    crate::notify::error("Could not merge the authors", &err);
                }
            },
        );
    });
    merge_button.set_sensitive(false);
    footer.append(&merge_button);

    // The candidate list, rebuilt on search, selection and load.
    let holder: crate::pages::SelfRebuild = Rc::new(RefCell::new(None));
    {
        let host = list_host.clone();
        let candidates = candidates.clone();
        let loaded = loaded.clone();
        let load_error = load_error.clone();
        let selected = selected.clone();
        let search = search_field.clone();
        let summary = summary.clone();
        let merge_button = merge_button.clone();
        let holder_for_rebuild = holder.clone();
        let rebuild: Rc<dyn Fn()> = Rc::new(move || {
            clear_box(&host);
            let chosen = selected.borrow().clone();
            match &chosen {
                Some(target) => {
                    merge_button.set_sensitive(true);
                    summary.set_text(&format!(
                        "{source_books} book{} will move to {}.",
                        if source_books == 1 { "" } else { "s" },
                        target.name
                    ));
                }
                None => {
                    merge_button.set_sensitive(false);
                    summary.set_text(if loaded.get() {
                        "Choose the author who should keep these books."
                    } else {
                        "Loading authors…"
                    });
                }
            }

            if !loaded.get() {
                let label = gtk::Label::new(Some("Loading authors…"));
                label.add_css_class("kalam-ui-meta");
                label.set_halign(gtk::Align::Start);
                host.append(&label);
                return;
            }
            let entries = candidates.borrow().clone();
            if entries.is_empty() {
                let text = match load_error.borrow().clone() {
                    Some(err) => format!("Authors could not be loaded: {err}"),
                    None => "No other authors in your library.".to_string(),
                };
                let label = gtk::Label::new(Some(&text));
                label.add_css_class("kalam-ui-meta");
                label.set_halign(gtk::Align::Start);
                label.set_wrap(true);
                label.set_xalign(0.0);
                label.set_max_width_chars(48);
                host.append(&label);
                return;
            }

            let query = normalize_author_name(search.value().trim());
            let visible: Vec<&AuthorIndexEntry> = entries
                .iter()
                .filter(|entry| {
                    query.is_empty()
                        || normalize_author_name(&entry.name).contains(&query)
                        || normalize_author_name(&entry.sort_name).contains(&query)
                        || entry
                            .aliases
                            .iter()
                            .any(|alias| normalize_author_name(alias).contains(&query))
                })
                .collect();
            if visible.is_empty() {
                let label = gtk::Label::new(Some("No authors match that search."));
                label.add_css_class("kalam-ui-meta");
                label.set_halign(gtk::Align::Start);
                host.append(&label);
                return;
            }

            for entry in visible {
                let is_chosen = chosen
                    .as_ref()
                    .is_some_and(|c| c.normalized_name == entry.normalized_name);
                let row_button = gtk::Button::new();
                row_button.add_css_class("kalam-author-merge-row");
                if is_chosen {
                    row_button.add_css_class("selected");
                }
                row_button.set_can_focus(true);

                let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                row.set_valign(gtk::Align::Center);
                row.append(&author_initials_disc(&entry.name, 40));

                let info = gtk::Box::new(gtk::Orientation::Vertical, 2);
                info.set_hexpand(true);
                info.set_valign(gtk::Align::Center);
                let name = gtk::Label::new(Some(&entry.name));
                name.add_css_class("kalam-ui-body");
                name.set_halign(gtk::Align::Start);
                name.set_xalign(0.0);
                name.set_ellipsize(gtk::pango::EllipsizeMode::End);
                info.append(&name);
                let meta = gtk::Label::new(Some(&format!(
                    "{} in library · {} read",
                    entry.book_count, entry.read_count
                )));
                meta.add_css_class("kalam-ui-meta");
                meta.set_halign(gtk::Align::Start);
                info.append(&meta);
                row.append(&info);

                if is_chosen {
                    let check = gtk::Image::from_icon_name("object-select-symbolic");
                    check.set_pixel_size(18);
                    check.add_css_class("kalam-author-merge-check");
                    check.set_valign(gtk::Align::Center);
                    row.append(&check);
                }
                row_button.set_child(Some(&row));
                set_accessible_label(&row_button, &format!("Merge into {}", entry.name));

                let picked = entry.clone();
                let selected = selected.clone();
                let holder = holder_for_rebuild.clone();
                row_button.connect_clicked(move |_| {
                    *selected.borrow_mut() = Some(picked.clone());
                    if let Some(rebuild) = holder.borrow().clone() {
                        rebuild();
                    }
                });
                host.append(&row_button);
            }
        });
        *holder.borrow_mut() = Some(rebuild.clone());
        rebuild();
    }
    {
        let holder = holder.clone();
        search_field.entry().connect_changed(move |_| {
            if let Some(rebuild) = holder.borrow().clone() {
                rebuild();
            }
        });
    }

    // Load the roster off the UI thread, minus the source identity.
    {
        let mut source_keys: HashSet<String> = HashSet::new();
        let mut claim = |value: &str| {
            let key = normalize_author_name(value);
            if !key.is_empty() {
                source_keys.insert(key);
            }
        };
        claim(&source_name);
        claim(&source_display);
        if let Some(profile) = &source_profile {
            claim(&profile.canonical_name);
            claim(&profile.sort_name);
            if !profile.normalized_name.trim().is_empty() {
                claim(profile.normalized_name.trim());
            }
            for alias in &profile.aliases {
                claim(alias);
            }
        }
        let candidates = candidates.clone();
        let loaded = loaded.clone();
        let load_error = load_error.clone();
        let holder = holder.clone();
        crate::tasks::spawn_internal(
            "Reading authors",
            move |_reporter| service.authors(),
            |_| {},
            move |snap| {
                let mut entries = snap.entries;
                entries.retain(|entry| {
                    !source_keys.contains(&entry.normalized_name)
                        && !entry
                            .aliases
                            .iter()
                            .any(|alias| source_keys.contains(&normalize_author_name(alias)))
                });
                if entries.is_empty() {
                    *load_error.borrow_mut() = snap.errors.into_iter().next();
                }
                *candidates.borrow_mut() = entries;
                loaded.set(true);
                if let Some(rebuild) = holder.borrow().clone() {
                    rebuild();
                }
            },
        );
    }

    match present_dialog(anchor, "Merge into another author", DialogExit::UnsavedInput, &shell) {
        Some(dialog) => *slot.borrow_mut() = Some(dialog),
        None => crate::notify::error(
            "Could not open the dialog",
            "The merge dialog could not be opened.",
        ),
    }
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
