//! PDF Reader Component for Kalam.
//!
//! Native PDF reader built on MuPDF rasterization, offering:
//! - Standard PDF viewer modes matching Firefox & Evince:
//!   - Scrolling: Page Scrolling, Vertical Scrolling, Horizontal Scrolling, Wrapped Scrolling
//!   - Spreads: No Spreads, Odd Spreads (Cover alone), Even Spreads (Side-by-side)
//!   - Configurable spread gap: 0px (Seamless for manga/diagrams), 4px, 8px, 12px, 16px
//! - Full Bookmarks support: view saved bookmarks in sidebar, one-click jump, delete, bottom pill toggle (B key)
//! - Top font cut-off protection with Top-aligned containers, safety margins, and dynamic containment
//! - Autohiding edge hover navigation pills matching Kalam's EPUB reader chrome
//! - Slide-in Zen-browser style left sidebar containing TOC, Bookmarks, and Settings
//! - Per-book preferences and global defaults persistence in catalog
//! - Reading history, session time tracking, and catalog persistence for "Continue Reading" on Home

use gtk::cairo;
use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use relm4::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use crate::db::{
    Annotation, Catalog, HighlightColor, PdfDrawing, PdfPageIdentity, ReadingBookmark, SavedWord,
};
use crate::epub_book::ReadingTheme;
use crate::pdf::{
    PdfDocument, PdfOpenOutcome, PdfPageText, PdfSearchOutcome, PdfSearchResult, PdfTocEntry,
};
use crate::widgets::dialog::{confirmation_dialog, ConfirmationKind};

mod drawings;
mod image;
mod notes;
mod pages_grid;
mod search_results;

pub use image::{PdfImageSettings, PdfPageColour, PdfPagePicture};
use drawings::{
    PdfActiveStroke, PdfDrawTool, PdfDrawingDragState, PdfDrawingPaint, PdfDrawingSettings,
    PdfDrawingWidgets, PdfInkColor, PdfStrokeWidth,
};

/// The password that unlocked this reading session, shared with
/// every worker that opens the file for itself. Memory only
/// (decision 73): it is never written to the catalog, the prefs
/// or a log, and closing the reader clears it.
type PdfSessionPassword = Arc<std::sync::RwLock<Option<String>>>;

fn pdf_session_password(shared: &PdfSessionPassword) -> Option<String> {
    match shared.read() {
        Ok(password) => password.clone(),
        Err(poisoned) => poisoned.into_inner().clone(),
    }
}

fn set_pdf_session_password(shared: &PdfSessionPassword, password: Option<String>) {
    match shared.write() {
        Ok(mut slot) => *slot = password,
        Err(poisoned) => *poisoned.into_inner() = password,
    }
}

/// Why the document service could not hand the reader a book.
/// These are states the open card can act on, not error prose:
/// the underlying MuPDF message stays in the log path only.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfLoadError {
    PasswordRequired,
    WrongPassword,
    FileNotFound,
    CantOpen,
}

impl PdfLoadError {
    pub fn log_message(self) -> &'static str {
        match self {
            Self::PasswordRequired => "PDF open needs a password",
            Self::WrongPassword => "PDF password was refused",
            Self::FileNotFound => "PDF file not found",
            Self::CantOpen => "PDF document could not be opened",
        }
    }
}

/// The reader's whole-book open state. `Opening` and `Ready`
/// leave the pure page to itself; the other three stand the
/// dashed state card over it (decision 73).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfOpenState {
    Opening,
    Ready,
    Password { wrong: bool, submitting: bool },
    FileNotFound,
    CantOpen,
}

impl PdfOpenState {
    pub fn from_load_error(error: PdfLoadError) -> Self {
        match error {
            PdfLoadError::PasswordRequired => Self::Password {
                wrong: false,
                submitting: false,
            },
            PdfLoadError::WrongPassword => Self::Password {
                wrong: true,
                submitting: false,
            },
            PdfLoadError::FileNotFound => Self::FileNotFound,
            PdfLoadError::CantOpen => Self::CantOpen,
        }
    }

    pub fn is_blocking(self) -> bool {
        !matches!(self, Self::Opening | Self::Ready)
    }

    pub fn is_password(self) -> bool {
        matches!(self, Self::Password { .. })
    }

    pub fn password_wrong(self) -> bool {
        matches!(self, Self::Password { wrong: true, .. })
    }

    pub fn password_submitting(self) -> bool {
        matches!(self, Self::Password { submitting: true, .. })
    }

    pub fn icon_name(self) -> &'static str {
        match self {
            Self::Password { .. } => "kalam-lock-symbolic",
            Self::CantOpen => "kalam-file-warning-symbolic",
            _ => "kalam-file-question-symbolic",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Self::Password { .. } => "This book is password protected",
            Self::FileNotFound => "File not found",
            Self::CantOpen => "This PDF can\u{2019}t be opened",
            Self::Opening | Self::Ready => "",
        }
    }

    pub fn body(self) -> &'static str {
        match self {
            Self::Password { .. } => {
                "Enter the password to open it. Kalam doesn\u{2019}t store it."
            }
            Self::FileNotFound => {
                "Kalam can\u{2019}t find this book at its saved location. \
                 It may have been moved or deleted."
            }
            Self::CantOpen => "The file is damaged or isn\u{2019}t a valid PDF.",
            Self::Opening | Self::Ready => "",
        }
    }
}

/// Open one worker's own copy of the document. Every worker
/// holds the session password the same way, so an unlocked book
/// renders, thumbs and OCRs as one book, not four locked ones.
fn pdf_open_document(
    path: &std::path::Path,
    shared_password: &PdfSessionPassword,
) -> Result<PdfDocument, PdfLoadError> {
    let password = pdf_session_password(shared_password);
    match PdfDocument::open_with_password(path, password.as_deref()) {
        Ok(PdfOpenOutcome::Ready(doc)) => Ok(doc),
        Ok(PdfOpenOutcome::PasswordRequired) => Err(PdfLoadError::PasswordRequired),
        Ok(PdfOpenOutcome::WrongPassword) => Err(PdfLoadError::WrongPassword),
        Err(_) => Err(if path.is_file() {
            PdfLoadError::CantOpen
        } else {
            PdfLoadError::FileNotFound
        }),
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PageSlot {
    Fixed(usize),
    PagedSingle,
    PagedSpreadLeft,
    PagedSpreadRight,
}

/// One search match painted over a materialised page. Every
/// scoped match on a visible page takes the quiet tint; the
/// current one takes the accent besides (decision 61).
#[derive(Clone, Debug, PartialEq)]
pub struct PdfSearchHighlight {
    pub slot: PageSlot,
    pub page: usize,
    /// Source-space MuPDF rects. The draw callback maps them
    /// against the area's live allocation, so a zoom or a panel
    /// pin cannot leave last size's coordinates behind.
    pub rects: Vec<(f32, f32, f32, f32)>,
    pub source_width: f32,
    pub source_height: f32,
    pub rotation: i32,
    pub match_rgb: [f32; 3],
    pub match_alpha: f32,
    pub current_rgb: [f32; 3],
    pub current: bool,
}

/// How widely search reaches (decision 61): the chapter the
/// reader is standing in, or the whole book. A fresh search
/// always opens on the chapter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PdfSearchScope {
    Chapter,
    Book,
}

impl PdfSearchScope {
    pub fn toggle_index(self) -> usize {
        match self {
            Self::Chapter => 0,
            Self::Book => 1,
        }
    }

    pub fn from_toggle_index(index: usize) -> Self {
        if index == 1 { Self::Book } else { Self::Chapter }
    }

    pub fn placeholder(self) -> &'static str {
        match self {
            Self::Chapter => "Search this chapter…",
            Self::Book => "Search this book…",
        }
    }
}

/// Which end of an existing selection a drag is taking hold of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfHandleDrag {
    Start,
    End,
}

/// Which selection grip, if either, a press at (`x`, `y`) takes hold
/// of. The end grip wins an overlap, as it has since the EPUB reader
/// set the rule. Handle tuples are `(x, y, length)`: a vertical bar
/// at upright or upside-down orientations, a horizontal bar after a
/// quarter turn. The grip tested here is the bar end the teardrop
/// hangs from, which is why each quarter turn names a different end.
fn selection_handle_at(sel: &PdfActiveSelection, x: f64, y: f64) -> Option<PdfHandleDrag> {
    if sel.is_block {
        return None;
    }
    let (sx, sy, sl) = sel.start_handle;
    let (ex, ey, el) = sel.end_handle;
    let (start_hit, end_hit) = match sel.rotation {
        90 => (
            (x - (sx + sl)).abs() < 28.0 && (y - sy).abs() < 24.0,
            (x - ex).abs() < 28.0 && (y - ey).abs() < 24.0,
        ),
        180 => (
            (x - sx).abs() < 24.0 && (y - (sy + sl)).abs() < 28.0,
            (x - ex).abs() < 24.0 && (y - ey).abs() < 28.0,
        ),
        270 => (
            (x - sx).abs() < 28.0 && (y - sy).abs() < 24.0,
            (x - (ex + el)).abs() < 28.0 && (y - ey).abs() < 24.0,
        ),
        _ => (
            (x - sx).abs() < 24.0 && (y - sy).abs() < 28.0,
            (x - ex).abs() < 24.0 && (y - (ey + el)).abs() < 28.0,
        ),
    };
    if end_hit {
        Some(PdfHandleDrag::End)
    } else if start_hit {
        Some(PdfHandleDrag::Start)
    } else {
        None
    }
}

/// Is (`x`, `y`) over one of the selection's handle grips -- the zone a
/// press there takes hold of? Mirrors `SelectionDragBegin` exactly by
/// sharing its decision, so the hover cursor never promises a grab
/// that will not happen.
fn over_selection_handle(sel: &PdfActiveSelection, x: f64, y: f64) -> bool {
    selection_handle_at(sel, x, y).is_some()
}

#[allow(dead_code)]
#[derive(Clone, Debug)]
pub struct PdfActiveSelection {
    pub page: usize,
    pub slot: PageSlot,
    pub text: String,
    pub screen_rects: Vec<(f64, f64, f64, f64)>,
    pub bounds: (f64, f64, f64, f64),
    pub start_handle: (f64, f64, f64),
    pub end_handle: (f64, f64, f64),
    pub anchor_pt: (f32, f32),
    pub active_pt: (f32, f32),
    pub is_block: bool,
    /// The view rotation the screen geometry was computed for. The
    /// selection is cleared when the view turns, so this never has
    /// to be reprojected after the fact; it only tells the painter
    /// and the grip hit-test which way the bars run.
    pub rotation: i32,
}

#[derive(Debug, Clone)]
pub struct PdfRenderRequest {
    pub generation: u64,
    pub page: usize,
    pub scale: f32,
    pub smart_crop: bool,
    pub rotation: i32,
    pub path: PathBuf,
}

/// One low-resolution page for the Pages grid. It travels on its
/// own worker and generation: turning or zooming the reading page
/// must not have to wait behind thumbnails, and a thumbnail must
/// never be mistaken for a reading raster.
#[derive(Debug, Clone)]
pub struct PdfThumbnailRequest {
    pub generation: u64,
    pub page: usize,
    pub target_width: u32,
    pub rotation: i32,
    pub path: PathBuf,
}

/// Document facts gathered once, off the UI thread, by the background doc
/// service (2.20: the UI thread never opens, parses or searches the file).
#[derive(Debug)]
pub struct PdfDocInfo {
    pub total_pages: usize,
    pub toc_entries: Vec<PdfTocEntry>,
    pub page_labels: Vec<String>,
    pub base_page_width: f64,
    pub base_page_height: f64,
}

/// A job for the background PDF document service thread. The service keeps
/// one open `PdfDocument` per path and answers from it; the UI thread only
/// ever sends requests and applies results that arrive as messages.
pub enum PdfDocRequest {
    /// Open the file and report page count, outline and page dimensions
    /// (measured on `page`, the resumed reading position).
    Info { path: PathBuf, page: usize },
    /// Extract the vector text layer of one page, plus the file fingerprint
    /// (computed here so the UI thread never stats the file).
    ExtractText { path: PathBuf, page: usize },
    /// Full-document text search.
    Search { path: PathBuf, query: String, toc: Vec<PdfTocEntry> },
}

/// A click that landed on a page whose text layer was not ready yet
/// (2.20 step 3). The intent is stored and replayed when the text arrives,
/// so the first click on a just-opened page is never a dead click.
#[derive(Debug)]
pub enum PdfPendingSelection {
    Word { slot: PageSlot, x: f64, y: f64 },
    Line { slot: PageSlot, x: f64, y: f64 },
    /// A drag that began before the text was ready. A finished drag cannot
    /// be replayed, so when the text arrives the word at the drag's start
    /// point is selected -- the closest honest answer to the click.
    Drag { slot: PageSlot, x: f64, y: f64 },
}

/// What should happen to a stored click when some page's text arrives.
/// Pure decision, unit-tested; the GTK side only executes it.
#[derive(Debug)]
enum PdfPendingOutcome {
    /// Nothing was pending, or the text that arrived belongs to another
    /// page: keep waiting, keep the signal up.
    KeepWaiting,
    /// The intent was consumed: replay this message.
    Replay(PdfReaderMsg),
    /// The intent was consumed but its page left the viewport: drop it.
    Dropped,
}

impl PdfReaderModel {
    /// Decide what a pending click does now that `page`'s text has
    /// arrived. Takes the intent out of `pending` when it is consumed.
    fn take_pending_replay(
        pending: &mut Option<(usize, PdfPendingSelection)>,
        page: usize,
        page_materialized: bool,
    ) -> PdfPendingOutcome {
        let Some((pending_page, intent)) = pending.take() else {
            return PdfPendingOutcome::KeepWaiting;
        };
        if pending_page != page {
            // A different page's text arrived; keep waiting for ours.
            *pending = Some((pending_page, intent));
            return PdfPendingOutcome::KeepWaiting;
        }
        if !page_materialized {
            return PdfPendingOutcome::Dropped;
        }
        let msg = match intent {
            PdfPendingSelection::Word { slot, x, y } => PdfReaderMsg::SelectionWordAt { slot, x, y },
            PdfPendingSelection::Line { slot, x, y } => PdfReaderMsg::SelectionLineAt { slot, x, y },
            // A finished drag cannot be replayed; the word at its start
            // point is the closest honest response.
            PdfPendingSelection::Drag { slot, x, y } => PdfReaderMsg::SelectionWordAt { slot, x, y },
        };
        PdfPendingOutcome::Replay(msg)
    }
}

pub struct PdfReaderInit {
    pub catalog: Arc<Catalog>,
    pub book_id: i64,
}

impl std::fmt::Debug for PdfReaderInit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfReaderInit")
            .field("book_id", &self.book_id)
            .finish()
    }
}

/// Scrolling modes matching standard PDF viewers (Firefox / Evince):
/// - PageScrolling: Discrete flips, page-at-a-time
/// - VerticalScrolling: Continuous vertical stream
/// - HorizontalScrolling: Continuous horizontal stream
/// - WrappedScrolling: Multi-column wrapped grid flow
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfScrollMode {
    PageScrolling,
    VerticalScrolling,
    HorizontalScrolling,
    WrappedScrolling,
}

/// Spread modes matching standard PDF viewers:
/// - NoSpreads: Single page layout
/// - OddSpreads: Facing pages with cover alone on page 1 ([1], [2, 3], [4, 5]...)
/// - EvenSpreads: Facing pages starting on page 1 ([1, 2], [3, 4]...)
#[allow(clippy::enum_variant_names)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfSpreadMode {
    NoSpreads,
    OddSpreads,
    EvenSpreads,
}

/// The Reading pane's Layout axis (decision 53). The engine's three
/// spread modes fold into it exactly: Spread plus "Cover on its own"
/// is the old OddSpreads, Spread without it is EvenSpreads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfLayout {
    Single,
    Spread,
}

/// The Reading pane's Flow axis (decision 53). Horizontal and
/// wrapped scrolling predate the two-axis pane and remain valid
/// stored values; on this axis they are both Scroll, and choosing
/// Scroll afresh means the vertical stream the design draws.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfFlow {
    Paged,
    Scroll,
}

/// How the page is first sized into the viewport (decision 54).
/// `Original` is one CSS pixel per PDF point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfFitMode {
    Width,
    Height,
    Screen,
    Original,
}

impl PdfFitMode {
    pub fn pref_value(self) -> &'static str {
        match self {
            PdfFitMode::Width => "width",
            PdfFitMode::Height => "height",
            PdfFitMode::Screen => "screen",
            PdfFitMode::Original => "original",
        }
    }

    pub fn from_pref(value: Option<&str>) -> Option<Self> {
        match value {
            Some("width") => Some(PdfFitMode::Width),
            Some("height") => Some(PdfFitMode::Height),
            Some("screen") => Some(PdfFitMode::Screen),
            Some("original") => Some(PdfFitMode::Original),
            _ => None,
        }
    }
}

pub const PDF_MIN_ZOOM_PERCENT: u32 = 25;
pub const PDF_MAX_ZOOM_PERCENT: u32 = 300;
pub const PDF_MAX_SCROLL_GAP: i32 = 64;

fn pdf_layout_for_spread_mode(mode: PdfSpreadMode) -> PdfLayout {
    match mode {
        PdfSpreadMode::NoSpreads => PdfLayout::Single,
        PdfSpreadMode::OddSpreads | PdfSpreadMode::EvenSpreads => PdfLayout::Spread,
    }
}

fn pdf_spread_mode_for_layout(layout: PdfLayout, cover_on_its_own: bool) -> PdfSpreadMode {
    match (layout, cover_on_its_own) {
        (PdfLayout::Single, _) => PdfSpreadMode::NoSpreads,
        (PdfLayout::Spread, true) => PdfSpreadMode::OddSpreads,
        (PdfLayout::Spread, false) => PdfSpreadMode::EvenSpreads,
    }
}

fn pdf_flow_for_scroll_mode(mode: PdfScrollMode) -> PdfFlow {
    match mode {
        PdfScrollMode::PageScrolling => PdfFlow::Paged,
        PdfScrollMode::VerticalScrolling
        | PdfScrollMode::HorizontalScrolling
        | PdfScrollMode::WrappedScrolling => PdfFlow::Scroll,
    }
}

fn pdf_scroll_mode_for_flow(flow: PdfFlow) -> PdfScrollMode {
    match flow {
        PdfFlow::Paged => PdfScrollMode::PageScrolling,
        PdfFlow::Scroll => PdfScrollMode::VerticalScrolling,
    }
}

/// Snaps a stored or accumulated rotation to the four view angles.
/// Anything that is not already a quarter turn reads as upright: a
/// corrupt pref must never leave a book permanently skewed.
fn normalize_pdf_rotation(degrees: i32) -> i32 {
    match degrees.rem_euclid(360) {
        90 => 90,
        180 => 180,
        270 => 270,
        _ => 0,
    }
}

/// The page's dimensions as displayed: quarter turns exchange the
/// sides, half turns keep them. `base_page_width/height` always hold
/// this display pair; the source pair lives beside them.
fn display_page_dimensions(source_w: f64, source_h: f64, rotation: i32) -> (f64, f64) {
    match normalize_pdf_rotation(rotation) {
        90 | 270 => (source_h, source_w),
        _ => (source_w, source_h),
    }
}

/// A source-page point in display pixels. Fractions keep the four
/// quarter turns exact and independent of the page's own size; the
/// caller scales into its own target box.
fn source_fraction_to_display(x: f64, y: f64, rotation: i32) -> (f64, f64) {
    match normalize_pdf_rotation(rotation) {
        90 => (1.0 - y, x),
        180 => (1.0 - x, 1.0 - y),
        270 => (y, 1.0 - x),
        _ => (x, y),
    }
}

/// A display pixel back in source-page points, the space MuPDF's
/// text layer and search results are reported in.
fn display_point_to_source(
    x: f64,
    y: f64,
    display_w: f64,
    display_h: f64,
    source_w: f32,
    source_h: f32,
    rotation: i32,
) -> (f32, f32) {
    if display_w <= 0.0 || display_h <= 0.0 || source_w <= 0.0 || source_h <= 0.0 {
        return (0.0, 0.0);
    }
    let u = (x / display_w).clamp(0.0, 1.0);
    let v = (y / display_h).clamp(0.0, 1.0);
    let (su, sv) = match normalize_pdf_rotation(rotation) {
        90 => (v, 1.0 - u),
        180 => (1.0 - u, 1.0 - v),
        270 => (1.0 - v, u),
        _ => (u, v),
    };
    ((su * source_w as f64) as f32, (sv * source_h as f64) as f32)
}

/// A source-page rectangle as an axis-aligned display rectangle
/// `(x, y, width, height)` in the target box.
fn source_rect_to_display(
    rect: (f32, f32, f32, f32),
    source_w: f32,
    source_h: f32,
    display_w: f64,
    display_h: f64,
    rotation: i32,
) -> (f64, f64, f64, f64) {
    if source_w <= 0.0 || source_h <= 0.0 || display_w <= 0.0 || display_h <= 0.0 {
        return (0.0, 0.0, 0.0, 0.0);
    }
    let (x0, y0, x1, y1) = rect;
    let a = source_fraction_to_display(x0 as f64 / source_w as f64, y0 as f64 / source_h as f64, rotation);
    let b = source_fraction_to_display(x1 as f64 / source_w as f64, y1 as f64 / source_h as f64, rotation);
    let rx = a.0.min(b.0) * display_w;
    let ry = a.1.min(b.1) * display_h;
    let rw = ((a.0.max(b.0) - a.0.min(b.0)) * display_w).max(1.0);
    let rh = ((a.1.max(b.1) - a.1.min(b.1)) * display_h).max(1.0);
    (rx, ry, rw, rh)
}

/// The scale one fit choice asks for, before the zoom percentage is
/// laid over it. A spread is sized as the spread (decision 53):
/// two display pages and the seam between them.
fn pdf_fit_zoom(
    mode: PdfFitMode,
    avail_w: f64,
    avail_h: f64,
    page_w: f64,
    page_h: f64,
    spreads_active: bool,
    seam_gap: i32,
) -> f64 {
    let page_w = if page_w > 10.0 { page_w } else { 595.0 };
    let page_h = if page_h > 10.0 { page_h } else { 842.0 };
    let needed_w = if spreads_active {
        page_w * 2.0 + seam_gap.max(0) as f64
    } else {
        page_w
    };
    let zoom_w = avail_w.max(100.0) / needed_w;
    let zoom_h = avail_h.max(100.0) / page_h;
    let fit = match mode {
        PdfFitMode::Width => zoom_w,
        PdfFitMode::Height => zoom_h,
        PdfFitMode::Screen => zoom_w.min(zoom_h),
        PdfFitMode::Original => 1.0,
    };
    (fit.clamp(0.2, 3.5) * 100.0).round() / 100.0
}

/// The effective scale the viewport renders at: the fit's scale with
/// the Reading pane's zoom percentage over it (100% is the fit
/// itself, as in the comic reader).
fn pdf_effective_zoom(fit_zoom: f64, zoom_percent: u32) -> f64 {
    let percent = zoom_percent.clamp(PDF_MIN_ZOOM_PERCENT, PDF_MAX_ZOOM_PERCENT) as f64 / 100.0;
    ((fit_zoom * percent).clamp(0.2, 3.5) * 100.0).round() / 100.0
}

/// Whether a viewport this shape can carry a spread at fit-height
/// size — the "wide enough" of decision 53, judged on the page's
/// own shape rather than a fixed pixel width so landscape pages
/// and quarter turns each get the answer their geometry deserves.
fn pdf_viewport_fits_spread(
    viewport_w: f64,
    viewport_h: f64,
    page_w: f64,
    page_h: f64,
    seam_gap: i32,
) -> bool {
    if viewport_w <= 100.0 || viewport_h <= 100.0 || page_w <= 0.0 || page_h <= 0.0 {
        return false;
    }
    let height_scale = (viewport_h - 100.0).max(100.0) / page_h;
    (page_w * 2.0 + seam_gap.max(0) as f64) * height_scale <= (viewport_w - 32.0).max(100.0)
}

/// Calculate left and optional right pages for a given target page.
pub fn calculate_spread_for_page(
    mode: PdfSpreadMode,
    total_pages: usize,
    page: usize,
) -> (usize, Option<usize>) {
    if total_pages == 0 {
        return (1, None);
    }
    let p = page.clamp(1, total_pages);
    match mode {
        PdfSpreadMode::NoSpreads => (p, None),
        PdfSpreadMode::OddSpreads => {
            if p <= 1 {
                (1, None)
            } else {
                let left = if p.is_multiple_of(2) { p } else { p - 1 };
                let right = if left < total_pages {
                    Some(left + 1)
                } else {
                    None
                };
                (left, right)
            }
        }
        PdfSpreadMode::EvenSpreads => {
            let left = if !p.is_multiple_of(2) { p } else { p - 1 };
            let right = if left < total_pages {
                Some(left + 1)
            } else {
                None
            };
            (left, right)
        }
    }
}

/// Return the spread preceding the given spread, or None if at the first spread.
pub fn calculate_prev_spread(
    mode: PdfSpreadMode,
    total_pages: usize,
    spread: (usize, Option<usize>),
) -> Option<(usize, Option<usize>)> {
    if spread.0 <= 1 || total_pages == 0 {
        None
    } else {
        Some(calculate_spread_for_page(mode, total_pages, spread.0 - 1))
    }
}

/// Return the spread following the given spread, or None if at the last spread.
pub fn calculate_next_spread(
    mode: PdfSpreadMode,
    total_pages: usize,
    spread: (usize, Option<usize>),
) -> Option<(usize, Option<usize>)> {
    if total_pages == 0 {
        return None;
    }
    let last = spread.1.unwrap_or(spread.0);
    if last >= total_pages {
        None
    } else {
        Some(calculate_spread_for_page(mode, total_pages, last + 1))
    }
}

/// Return list of (left, right) page spreads for streaming.
pub fn calculate_all_spreads(
    mode: PdfSpreadMode,
    total_pages: usize,
) -> Vec<(usize, Option<usize>)> {
    if total_pages == 0 {
        return Vec::new();
    }
    match mode {
        PdfSpreadMode::NoSpreads => (1..=total_pages).map(|p| (p, None)).collect(),
        PdfSpreadMode::OddSpreads => {
            let mut list = vec![(1, None)];
            let mut p = 2;
            while p <= total_pages {
                let right = if p < total_pages { Some(p + 1) } else { None };
                list.push((p, right));
                p += 2;
            }
            list
        }
        PdfSpreadMode::EvenSpreads => {
            let mut list = Vec::new();
            let mut p = 1;
            while p <= total_pages {
                let right = if p < total_pages { Some(p + 1) } else { None };
                list.push((p, right));
                p += 2;
            }
            list
        }
    }
}

/// The name one physical page is shown by: its document-defined
/// label where the catalogue carries one, the physical number
/// where it does not (or where a label list has not arrived yet).
/// The engine has already substituted physical numbers for
/// unlabelled ranges; the trim here keeps a stray blank label
/// from ever painting as an empty name.
fn displayed_page_label(page_labels: &[String], page: usize) -> String {
    if page == 0 {
        return "0".to_string();
    }
    page_labels
        .get(page - 1)
        .map(|label| label.trim())
        .filter(|label| !label.is_empty())
        .map(str::to_string)
        .unwrap_or_else(|| page.to_string())
}

/// The location text for one reading position. Wrapped scrolling
/// lays pages out one by one even when a spread pairing is chosen,
/// so it names one page, exactly as the viewport does. Where the
/// document's own labels differ from the physical pages in view,
/// they lead and the physical position follows (`iv · 8 / 612`,
/// decisions' "specified, not drawn" list); an ordinary book,
/// whose labels are its physical numbers, keeps the plain form.
fn format_location_label(
    spread_mode: PdfSpreadMode,
    scroll_mode: PdfScrollMode,
    total_pages: usize,
    current_page: usize,
    page_labels: &[String],
) -> String {
    if total_pages == 0 {
        return "0 / 0".to_string();
    }
    let current = current_page.clamp(1, total_pages);
    let visible: Vec<usize> = if spread_mode == PdfSpreadMode::NoSpreads
        || scroll_mode == PdfScrollMode::WrappedScrolling
    {
        vec![current]
    } else {
        let (left, right) = calculate_spread_for_page(spread_mode, total_pages, current);
        match right {
            Some(right) => vec![left, right],
            None => vec![left],
        }
    };
    let absolute = match visible.as_slice() {
        [left, right] => format!("{left}–{right} / {total_pages}"),
        [page] => format!("{page} / {total_pages}"),
        _ => format!("0 / {total_pages}"),
    };
    let labels: Vec<String> = visible
        .iter()
        .map(|page| displayed_page_label(page_labels, *page))
        .collect();
    if labels
        .iter()
        .zip(&visible)
        .all(|(label, page)| label == &page.to_string())
    {
        return absolute;
    }
    // A prefix-only range repeats one label on every page; a
    // spread inside it names that label once, not twice.
    let label_text = match labels.as_slice() {
        [first, second] if first == second => first.clone(),
        _ => labels.join("–"),
    };
    format!("{label_text} · {absolute}")
}

/// The pages the Pages grid rings as current for one reading
/// position, under the location text's own spread rules.
fn pdf_pages_current_pages(
    spread_mode: PdfSpreadMode,
    scroll_mode: PdfScrollMode,
    total_pages: usize,
    current_page: usize,
) -> HashSet<usize> {
    if total_pages == 0 {
        return HashSet::new();
    }
    let current = current_page.clamp(1, total_pages);
    if spread_mode == PdfSpreadMode::NoSpreads
        || scroll_mode == PdfScrollMode::WrappedScrolling
    {
        return HashSet::from([current]);
    }
    let (left, right) = calculate_spread_for_page(spread_mode, total_pages, current);
    let mut pages = HashSet::from([left]);
    if let Some(right) = right {
        pages.insert(right);
    }
    pages
}

/// The pages the jump field's Chapter mode counts: the outline's
/// top level, in outline order. MuPDF's flattened outline already
/// forces structural titles (Preface, Chapter, Index) to depth 0,
/// which is the level a reader means by a chapter here.
fn pdf_jump_chapter_pages(entries: &[PdfTocEntry], total_pages: usize) -> Vec<usize> {
    entries
        .iter()
        .filter(|entry| entry.depth == 0 && entry.page >= 1 && entry.page <= total_pages)
        .map(|entry| entry.page)
        .collect()
}

fn pdf_jump_current(
    mode: PdfJumpMode,
    entries: &[PdfTocEntry],
    total_pages: usize,
    current_page: usize,
) -> usize {
    match mode {
        PdfJumpMode::Page => current_page.clamp(1, total_pages.max(1)),
        PdfJumpMode::Chapter => {
            let chapters = pdf_jump_chapter_pages(entries, total_pages);
            chapters
                .iter()
                .rposition(|page| *page <= current_page)
                .map(|idx| idx + 1)
                .unwrap_or(1)
        }
    }
}

fn pdf_jump_target(
    mode: PdfJumpMode,
    text: &str,
    entries: &[PdfTocEntry],
    total_pages: usize,
) -> Option<usize> {
    let value: usize = text.trim().parse().ok()?;
    if value == 0 {
        return None;
    }
    match mode {
        PdfJumpMode::Page => (value <= total_pages).then_some(value),
        PdfJumpMode::Chapter => pdf_jump_chapter_pages(entries, total_pages)
            .get(value - 1)
            .copied(),
    }
}

/// Each flattened outline entry's parent, by depth. The outline
/// arrives flattened, so parentage is the nearest previous entry
/// one level up.
fn toc_parent_indices(entries: &[PdfTocEntry]) -> Vec<Option<usize>> {
    let mut parents = Vec::with_capacity(entries.len());
    let mut stack: Vec<usize> = Vec::new();
    for (index, entry) in entries.iter().enumerate() {
        while stack
            .last()
            .is_some_and(|last| entries[*last].depth >= entry.depth)
        {
            stack.pop();
        }
        parents.push(stack.last().copied());
        stack.push(index);
    }
    parents
}

/// The outline entry the reader is inside: the latest-starting
/// entry at or before the current page, deepest first when several
/// entries share a start page.
fn current_toc_index(
    entries: &[PdfTocEntry],
    current_page: usize,
    total_pages: usize,
) -> Option<usize> {
    entries
        .iter()
        .enumerate()
        .filter(|(_, entry)| {
            entry.page >= 1 && entry.page <= total_pages && entry.page <= current_page
        })
        .max_by_key(|(index, entry)| (entry.page, entry.depth, *index))
        .map(|(index, _)| index)
}

/// The top-level span the Pages panel's sticky heading names for
/// one page. A nested section still belongs to its chapter on the
/// board (`THE EARTH · 47 pages`), so this deliberately resolves
/// to a root outline entry rather than the deepest current entry
/// Contents marks.
#[derive(Debug, Clone, PartialEq, Eq)]
struct PdfPagesHeading {
    title: String,
    start_page: usize,
    end_page: usize,
}

impl PdfPagesHeading {
    fn page_count(&self) -> usize {
        self.end_page.saturating_sub(self.start_page) + 1
    }

    fn contains(&self, page: usize) -> bool {
        (self.start_page..=self.end_page).contains(&page)
    }
}

fn pages_heading_for_page(
    entries: &[PdfTocEntry],
    total_pages: usize,
    page: usize,
    fallback_title: &str,
) -> PdfPagesHeading {
    let total_pages = total_pages.max(1);
    let page = page.clamp(1, total_pages);
    let fallback = || {
        let title = if fallback_title.trim().is_empty() {
            "This book"
        } else {
            fallback_title.trim()
        };
        title.to_string()
    };

    let parents = toc_parent_indices(entries);
    let mut roots: Vec<(usize, usize, String)> = entries
        .iter()
        .enumerate()
        .filter(|(index, entry)| {
            parents.get(*index).copied().flatten().is_none()
                && entry.page >= 1
                && entry.page <= total_pages
        })
        .map(|(index, entry)| (entry.page, index, entry.title.clone()))
        .collect();
    roots.sort_by_key(|(start, index, _)| (*start, *index));
    // Two roots at one destination describe the same opening; the
    // later outline row is the more specific name for that span.
    roots.dedup_by(|later, kept| {
        if later.0 == kept.0 {
            kept.1 = later.1;
            kept.2 = later.2.clone();
            true
        } else {
            false
        }
    });

    if roots.is_empty() {
        return PdfPagesHeading {
            title: fallback(),
            start_page: 1,
            end_page: total_pages,
        };
    }
    if page < roots[0].0 {
        return PdfPagesHeading {
            title: fallback(),
            start_page: 1,
            end_page: roots[0].0 - 1,
        };
    }

    let root_index = roots
        .iter()
        .rposition(|(start, _, _)| *start <= page)
        .expect("the first root starts at or before this page");
    let (start_page, _, title) = roots[root_index].clone();
    let end_page = roots
        .get(root_index + 1)
        .map(|(next_start, _, _)| next_start - 1)
        .unwrap_or(total_pages);
    PdfPagesHeading {
        title,
        start_page,
        end_page,
    }
}

/// The raw whole-book matches one search scope can see. Chapter
/// is the top-level outline span the standing anchor page is in;
/// a book with no outline has one chapter by construction, and
/// so does its front matter before the first outline entry.
fn pdf_search_scoped_indices(
    results: &[PdfSearchResult],
    entries: &[PdfTocEntry],
    total_pages: usize,
    scope: PdfSearchScope,
    chapter_anchor_page: usize,
    fallback_title: &str,
) -> Vec<usize> {
    match scope {
        PdfSearchScope::Book => (0..results.len()).collect(),
        PdfSearchScope::Chapter => {
            let span = pages_heading_for_page(
                entries,
                total_pages,
                chapter_anchor_page,
                fallback_title,
            );
            results
                .iter()
                .enumerate()
                .filter(|(_, result)| span.contains(result.page))
                .map(|(index, _)| index)
                .collect()
        }
    }
}

/// The result a new search lands on: the first scoped match on
/// or after the page the reader was standing on when they asked,
/// wrapping to the scope's first match when the rest are behind.
fn pdf_search_start_position(
    results: &[PdfSearchResult],
    scoped_indices: &[usize],
    anchor_page: usize,
) -> usize {
    scoped_indices
        .iter()
        .position(|raw| {
            results
                .get(*raw)
                .is_some_and(|result| result.page >= anchor_page)
        })
        .unwrap_or(0)
}

/// One result's place in the book: its top-level chapter, and
/// the deepest section it sits in when that is a different name.
/// The results list prints this letter-spaced and uppercase
/// from CSS (`THE EARTH · VOLCANOES`); one sentence-cased
/// chord here beats the flat nearest title the old popover had.
fn pdf_search_section_path(
    entries: &[PdfTocEntry],
    total_pages: usize,
    page: usize,
    fallback_title: &str,
) -> String {
    let Some(current) = current_toc_index(entries, page, total_pages) else {
        return pages_heading_for_page(entries, total_pages, page, fallback_title).title;
    };
    let parents = toc_parent_indices(entries);
    let mut chain = Vec::new();
    let mut cursor = Some(current);
    while let Some(index) = cursor {
        chain.push(entries[index].title.trim().to_string());
        cursor = parents.get(index).copied().flatten();
    }
    chain.reverse();
    chain.retain(|title| !title.is_empty());
    match chain.len() {
        0 => pages_heading_for_page(entries, total_pages, page, fallback_title).title,
        1 => chain.remove(0),
        _ => match (chain.first(), chain.last()) {
            (Some(root), Some(deepest)) if root.eq_ignore_ascii_case(deepest) => {
                root.clone()
            }
            (Some(root), Some(deepest)) => format!("{root} · {deepest}"),
            _ => pages_heading_for_page(entries, total_pages, page, fallback_title).title,
        },
    }
}

/// The lazy results model's entries for one scoped search:
/// a chapter heading each time the running section path
/// changes, then its results in reading order.
fn pdf_search_result_entries(
    results: &[PdfSearchResult],
    scoped_indices: &[usize],
    entries: &[PdfTocEntry],
    total_pages: usize,
    fallback_title: &str,
) -> Vec<search_results::PdfSearchResultsEntry> {
    let mut grouped = Vec::new();
    let mut standing_path: Option<String> = None;
    for raw_index in scoped_indices {
        let Some(result) = results.get(*raw_index) else {
            continue;
        };
        let path = pdf_search_section_path(entries, total_pages, result.page, fallback_title);
        if standing_path.as_deref() != Some(path.as_str()) {
            grouped.push(search_results::PdfSearchResultsEntry::Heading(
                path.clone(),
            ));
            standing_path = Some(path);
        }
        grouped.push(search_results::PdfSearchResultsEntry::Result {
            raw_index: *raw_index,
            page: result.page,
            snippet: result.snippet.clone(),
        });
    }
    grouped
}

fn pdf_search_result_page_count(
    results: &[PdfSearchResult],
    scoped_indices: &[usize],
) -> usize {
    let mut pages = scoped_indices
        .iter()
        .filter_map(|raw| results.get(*raw).map(|result| result.page))
        .collect::<Vec<_>>();
    pages.sort_unstable();
    pages.dedup();
    pages.len()
}

/// The rows Contents shows for one filter/collapse state. A filter
/// keeps its matches, their ancestors and their descendants, and
/// overrides folding while it stands — a match folded inside its
/// parent would be a filter that hides what it found.
fn visible_toc_rows(
    entries: &[PdfTocEntry],
    collapsed: &HashSet<usize>,
    filter: &str,
) -> Vec<VisibleTocRow> {
    let parents = toc_parent_indices(entries);
    let has_children: Vec<bool> = entries
        .iter()
        .enumerate()
        .map(|(index, _)| parents.contains(&Some(index)))
        .collect();
    let query = filter.trim().to_lowercase();

    let included: Vec<bool> = if query.is_empty() {
        entries
            .iter()
            .enumerate()
            .map(|(index, _)| {
                let mut cursor = parents[index];
                while let Some(parent) = cursor {
                    if collapsed.contains(&parent) {
                        return false;
                    }
                    cursor = parents[parent];
                }
                true
            })
            .collect()
    } else {
        let mut included = vec![false; entries.len()];
        for (index, entry) in entries.iter().enumerate() {
            if !entry.title.to_lowercase().contains(&query) {
                continue;
            }
            included[index] = true;
            let mut cursor = parents[index];
            while let Some(parent) = cursor {
                included[parent] = true;
                cursor = parents[parent];
            }
            for (other, _) in entries.iter().enumerate().skip(index + 1) {
                let mut cursor = parents[other];
                while let Some(parent) = cursor {
                    if parent == index {
                        included[other] = true;
                        break;
                    }
                    cursor = parents[parent];
                }
            }
        }
        included
    };

    entries
        .iter()
        .enumerate()
        .filter(|(index, _)| included[*index])
        .map(|(index, _)| VisibleTocRow {
            index,
            has_children: has_children[index],
            collapsed: query.is_empty() && collapsed.contains(&index),
        })
        .collect()
}

#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct CachedPageTexture {
    pub texture: gdk::Texture,
    pub width: i32,
    pub height: i32,
    pub generation: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfLeftTab {
    Contents,
    Pages,
    Notes,
}

/// The Notes tab's filter chips (decision 60): one list holding
/// this book's highlights, bookmarks and saved words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfNotesFilter {
    All,
    Highlights,
    Bookmarks,
    Words,
}

/// Whose saved words the Words filter lists (decision 60): the
/// current top-level chapter, this book, or every book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfWordScope {
    Chapter,
    Book,
    All,
}

impl PdfWordScope {
    pub fn toggle_index(self) -> usize {
        match self {
            Self::Chapter => 0,
            Self::Book => 1,
            Self::All => 2,
        }
    }

    pub fn from_toggle_index(index: usize) -> Self {
        match index {
            1 => Self::Book,
            2 => Self::All,
            _ => Self::Chapter,
        }
    }
}

/// One deletion Ctrl+Z can take back (decision 30): the row as it
/// stood, kept for this reading session.
#[derive(Debug, Clone)]
pub enum PdfDeletedMark {
    Annotation(Box<Annotation>),
    Bookmark(Box<ReadingBookmark>),
    SavedWord(Box<SavedWord>),
    /// A deleted drawing: undo restores the row as it stood.
    Drawing(Box<PdfDrawing>),
    /// A drawing this session created: undo removes it.
    DrawingCreated(Box<PdfDrawing>),
    /// A drawing before a move or edit: undo writes it back.
    DrawingBefore(Box<PdfDrawing>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfRightTab {
    Reading,
    Image,
    Shortcuts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfJumpMode {
    Page,
    Chapter,
}

/// One outline row after the filter and collapsed branches have had
/// their say. `index` is the entry's position in the flattened
/// outline, which is also the identity the collapse set stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct VisibleTocRow {
    index: usize,
    has_children: bool,
    collapsed: bool,
}

pub struct PdfSettingsWidgets {
    pub layout_single_btn: gtk::Button,
    pub layout_spread_btn: gtk::Button,
    pub flow_control: crate::widgets::segmented::SegmentedControl,
    pub scroll_gap_row: gtk::Box,
    pub scroll_gap_scale: gtk::Scale,
    pub scroll_gap_value: gtk::Label,
    pub cover_switch: gtk::Switch,
    pub fit_width_btn: gtk::Button,
    pub fit_height_btn: gtk::Button,
    pub fit_screen_btn: gtk::Button,
    pub fit_original_btn: gtk::Button,
    pub zoom_scale: gtk::Scale,
    pub zoom_value: gtk::Label,
    pub rotate_left_btn: gtk::Button,
    pub rotate_right_btn: gtk::Button,
}

pub struct PdfImageWidgets {
    pub page_colour_control: crate::widgets::segmented::SegmentedControl,
    pub tint_strength_row: gtk::Box,
    pub tint_strength_scale: gtk::Scale,
    pub brightness_scale: gtk::Scale,
    pub contrast_scale: gtk::Scale,
    pub warmth_scale: gtk::Scale,
    pub grayscale_switch: gtk::Switch,
    pub trim_switch: gtk::Switch,
}

pub struct PdfPagesWidgets {
    pub scroll: gtk::ScrolledWindow,
    pub heading_title: gtk::Label,
    pub heading_meta: gtk::Label,
    pub columns_3_button: gtk::Button,
    pub columns_2_button: gtk::Button,
}

pub struct PdfSearchResultsWidgets {
    pub meta_label: gtk::Label,
    pub list: search_results::PdfSearchResultsList,
}

#[derive(Debug)]
pub enum PdfReaderMsg {
    NextPage,
    PrevPage,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    SetZoomPercent(u32),
    SetFitMode(PdfFitMode),
    ToggleFitOriginal,
    GoToFirstPage,
    GoToLastPage,
    SetLayout(PdfLayout),
    SetFlow(PdfFlow),
    SetCoverOnItsOwn(bool),
    SetScrollGap(i32),
    RotateView(i32),
    SetScrollMode(PdfScrollMode),
    SetSpreadMode(PdfSpreadMode),
    ApplyAutoSpreadMode(PdfSpreadMode),
    SetSpreadGap(i32),
    ToggleBookmark,
    DeleteBookmark(i64),
    DeleteAnnotation(i64),
    DeleteSavedWord(i64),
    DeleteDrawing(i64),
    SetDrawTool(PdfDrawTool),
    SetHighlighterColor(PdfInkColor),
    SetHighlighterOpacity(u8),
    SetPenColor(PdfInkColor),
    SetPenWidth(PdfStrokeWidth),
    SetSelectedDrawingColor(PdfInkColor),
    /// A selected stroke's width in tenths of a point, so the
    /// popover's three choices travel without a float in the message.
    SetSelectedDrawingWidthTenths(i64),
    CloseDrawingPopover,
    DrawToolbarZoneHover(bool),
    DrawToolbarHover(bool),
    DrawToolbarRevealDue(u64),
    DrawToolbarHideDue(u64),
    UndoDelete,
    JumpToPage(usize),
    SetSmartCrop(bool),
    ToggleSmartCrop,
    SetPageColour(PdfPageColour),
    SetTintStrength(i32),
    SetBrightness(i32),
    SetContrast(i32),
    SetWarmth(i32),
    SetGrayscale(bool),
    ResetImageSettings,
    SetLeftTab(PdfLeftTab),
    SetRightTab(PdfRightTab),
    OpenLeftPanel,
    OpenRightPanel,
    OpenContents,
    OpenPages,
    OpenNotes,
    SetNotesFilter(PdfNotesFilter),
    ToggleHighlightColourFilter(HighlightColor),
    ToggleHighlightSearch,
    HighlightSearchChanged(String),
    SetWordScope(PdfWordScope),
    WordSearchChanged(String),
    SetPagesColumns(usize),
    OpenReadingPanel,
    OpenShortcutsPanel,
    ToggleLeftPanel,
    ToggleRightPanel,
    ClosePanels,
    ToggleLeftPin,
    ToggleRightPin,
    TocFilterChanged(String),
    ToggleTocEntry(usize),
    OpenJump(Option<String>),
    CloseJump,
    SetJumpMode(PdfJumpMode),
    JumpTextChanged(String),
    SubmitJump,
    ShowBackChrome,
    BackZoneHover(bool),
    BackRevealDue(u64),
    BackHideDue(u64),
    EscapeKey,
    Close,
    MinimizeToBubble,
    ToggleSearch,
    UpdateSearchQuery(String),
    SetSearchScope(PdfSearchScope),
    NextSearchResult,
    PrevSearchResult,
    OpenSearchResults,
    CloseSearchResults,
    JumpToSearchResult(usize),
    CloseSearch,
    PageRendered {
        generation: u64,
        page: usize,
        texture: gdk::Texture,
        width: i32,
        height: i32,
    },
    /// A Pages-grid cell for this page has just been bound. This
    /// is the lazy thumbnail request, not a reading-page load.
    PageThumbnailVisible(usize),
    PageThumbnailReady {
        generation: u64,
        page: usize,
        texture: Option<gdk::Texture>,
    },
    PagesGridScrolled(f64),
    LeftEdgeHover(bool),
    RightEdgeHover(bool),
    LeftPanelHover(bool),
    RightPanelHover(bool),
    LeftCloseTimerTick(u64),
    RightCloseTimerTick(u64),
    UserScrolled,
    ScrollDelta(f64),
    UpdateScrollPage(usize),
    SelectionDragBegin { slot: PageSlot, x: f64, y: f64, is_block: bool },
    SelectionDragUpdate { slot: PageSlot, dx: f64, dy: f64, is_block: bool },
    SelectionDragEnd { slot: PageSlot, dx: f64, dy: f64, press_count: i32 },
    DrawingDragBegin { slot: PageSlot, x: f64, y: f64 },
    DrawingDragUpdate { slot: PageSlot, dx: f64, dy: f64 },
    DrawingDragEnd { slot: PageSlot, dx: f64, dy: f64, press_count: i32 },
    SelectionWordAt { slot: PageSlot, x: f64, y: f64 },
    SelectionLineAt { slot: PageSlot, x: f64, y: f64 },
    PageOcrResult {
        generation: u64,
        page: usize,
        /// Fingerprint the OCR worker computed after recognition; used to
        /// persist the result only when the file is unchanged.
        fingerprint: Option<String>,
        text: Box<Result<PdfPageText, String>>,
    },
    /// The doc service finished opening the document (or failed).
    DocumentLoaded {
        info: Box<Result<PdfDocInfo, PdfLoadError>>,
    },
    /// The password field changed. It carries no text: the entry
    /// is the one place a typed password lives before Unlock.
    PasswordEdited,
    SubmitPassword,
    LocateFile,
    ConfirmRemoveBook,
    RemoveBook,
    RemoveBookDone(Result<(), String>),
    /// The doc service extracted one page's vector text (empty text means
    /// a scanned page: the handler falls back to the OCR cache / worker).
    PageTextReady {
        page: usize,
        fingerprint: Option<String>,
        text: Result<PdfPageText, String>,
    },
    /// The doc service finished a full-document search.
    SearchReady {
        query: String,
        outcome: PdfSearchOutcome,
    },
    CopySelection,
    SaveAnnotation {
        color: String,
        style: String,
        note: String,
    },
    #[allow(dead_code)]
    QuoteSelection,
    LookUpWord(String),
    ToggleSaveWord,
    ClearDict,
    #[allow(dead_code)]
    ClearSelection,
    HideZoomOsd(u64),
    ViewportResized,
    LayoutSettled,
}

#[derive(Debug)]
pub enum PdfReaderOut {
    Close,
    MinimizeToBubble { book_id: i64 },
}

pub struct PdfReaderModel {
    pub book_id: i64,
    pub catalog: Arc<Catalog>,
    pub title: String,
    pub author: String,
    pub cover_path: Option<PathBuf>,
    pub file_path: Option<PathBuf>,
    pub current_page: usize,
    pub total_pages: usize,
    pub zoom_level: f64,
    pub scroll_mode: PdfScrollMode,
    pub spread_mode: PdfSpreadMode,
    pub two_page_gap: i32,
    /// The Image pane's whole state (decisions 54–55): page colour,
    /// adjustments, grayscale and Trim margins — the pre-redesign
    /// smart crop under its designed name.
    pub image_settings: PdfImageSettings,
    /// The palette paper Tint takes white toward, captured from the
    /// theme in force when this book opened.
    pub image_paper_rgb: [f32; 3],
    /// Space between successive pages (or spread rows) in a scroll
    /// flow — the Reading pane's "Gap between pages". The seam
    /// inside a spread stays `two_page_gap`, its pre-redesign pref.
    pub scroll_gap: i32,
    /// How spread pages pair when Layout is Spread; kept as its own
    /// state so choosing Single does not forget the pairing.
    pub cover_on_its_own: bool,
    /// True until the reader (or an earlier Kalam) has ever stored
    /// a layout choice: while it stands, the layout follows the
    /// window's shape instead of a stored answer.
    pub layout_auto: bool,
    pub fit_mode: PdfFitMode,
    /// Zoom as a percentage of the selected fit's size; 100 is the
    /// fit itself. `zoom_level` remains the effective scale every
    /// render and allocation reads.
    pub zoom_percent: u32,
    /// The fit a double-click to Original came from, so the next
    /// double-click can go back to it.
    pub pre_zoom_fit: Option<PdfFitMode>,
    pub rotation_degrees: i32,
    pub left_tab: PdfLeftTab,
    pub right_tab: PdfRightTab,
    pub show_back_button: bool,
    pub left_panel_open: bool,
    pub right_panel_open: bool,
    pub left_panel_pinned: bool,
    pub right_panel_pinned: bool,
    pub back_hover: bool,
    pub mouse_in_left_edge: bool,
    pub mouse_in_right_edge: bool,
    pub mouse_in_left_panel: bool,
    pub mouse_in_right_panel: bool,
    pub back_gen: u64,
    pub left_close_seq: u64,
    pub right_close_seq: u64,
    pub jump_open: bool,
    pub jump_text: String,
    pub jump_mode: PdfJumpMode,
    pub toc_filter: String,
    pub collapsed_toc_entries: HashSet<usize>,
    pub toc_entries: Vec<PdfTocEntry>,
    pub page_labels: Vec<String>,
    pub bookmarks: Vec<ReadingBookmark>,
    pub annotations: Vec<Annotation>,
    pub saved_words: Vec<SavedWord>,
    /// Stable page identities in current page order (decision 69),
    /// and the drawing objects anchored to them (decision 72).
    pub page_identities: Vec<PdfPageIdentity>,
    pub drawings: Vec<PdfDrawing>,
    pub drawing_paints: Rc<std::cell::RefCell<Vec<PdfDrawingPaint>>>,
    pub active_drawing: Rc<std::cell::RefCell<Option<PdfActiveStroke>>>,
    pub draw_tool: PdfDrawTool,
    /// The same answer where page gesture closures can read it
    /// before their messages reach the model.
    pub draw_tool_shared: Rc<std::cell::Cell<PdfDrawTool>>,
    pub draw_settings: PdfDrawingSettings,
    pub draw_accent_rgb: [f32; 3],
    pub draw_toolbar_visible: bool,
    pub draw_toolbar_zone_hover: bool,
    pub draw_toolbar_hover: bool,
    pub draw_toolbar_seq: u64,
    pub draw_options_open: bool,
    pub drawing_widgets: Option<PdfDrawingWidgets>,
    pub drawing_drag: Option<PdfDrawingDragState>,
    pub selected_drawing_id: Option<i64>,
    pub drawing_popover: Option<gtk::Popover>,
    pub notes_filter: PdfNotesFilter,
    pub highlight_colour_filter: Option<HighlightColor>,
    pub highlight_search_open: bool,
    pub highlight_search_query: String,
    pub word_scope: PdfWordScope,
    pub word_search_query: String,
    pub deleted_marks: Vec<PdfDeletedMark>,
    pub notes_words_page_synced: usize,
    pub textures: HashMap<usize, CachedPageTexture>,
    pub pending_loads: HashSet<usize>,
    pub arrow_step: f32,
    pub is_loading: bool,
    pub open_state: PdfOpenState,
    /// The session password shared with every PDF worker.
    /// Memory only; see [`PdfSessionPassword`].
    pub session_password: PdfSessionPassword,
    pub password_has_text: bool,
    pub open_state_removing: bool,
    pub session_id: Option<i64>,
    pub session_start: std::time::Instant,
    pub page_pictures: HashMap<usize, PdfPagePicture>,
    pub paged_picture: Option<PdfPagePicture>,
    pub paged_loading_box: Option<gtk::Box>,
    pub paged_label: Option<gtk::Label>,
    pub two_page_left_pic: Option<PdfPagePicture>,
    pub two_page_right_pic: Option<PdfPagePicture>,
    pub two_page_loading_box: Option<gtk::Box>,
    pub two_page_label: Option<gtk::Label>,
    pub left_stack: Option<gtk::Stack>,
    pub right_stack: Option<gtk::Stack>,
    pub left_tabs: Option<crate::widgets::segmented::SegmentedControl>,
    pub right_tabs: Option<crate::widgets::segmented::SegmentedControl>,
    pub jump_mode_control: Option<crate::widgets::segmented::SegmentedControl>,
    pub back_unit: Option<crate::widgets::slide_out_pill::SlideOutPill>,
    pub toc_list_box: Option<gtk::Box>,
    pub toc_scroll: Option<gtk::ScrolledWindow>,
    /// The visible Contents rows by outline index, so a page turn can
    /// move the current mark without rebuilding the list under the
    /// reader's scroll position.
    pub toc_rows: Vec<(usize, gtk::Box)>,
    pub toc_current_visible_row: Option<usize>,
    pub toc_current_page_synced: usize,
    pub notes_widgets: Option<notes::PdfNotesWidgets>,
    pub pages_grid: Option<pages_grid::PdfPagesGrid>,
    pub pages_widgets: Option<PdfPagesWidgets>,
    pub pages_columns: usize,
    pub thumbnail_tx: Option<async_channel::Sender<PdfThumbnailRequest>>,
    pub thumbnail_generation: u64,
    pub thumbnail_active_generation: Option<Arc<AtomicU64>>,
    pub thumbnail_pending: HashSet<usize>,
    pub settings_widgets: Option<PdfSettingsWidgets>,
    pub image_widgets: Option<PdfImageWidgets>,
    pub screenshot_action_applied: bool,
    pub screenshot_password_action_applied: bool,
    pub render_tx: Option<async_channel::Sender<PdfRenderRequest>>,
    pub render_generation: u64,
    pub active_generation: Option<Arc<AtomicU64>>,
    pub base_page_width: f64,
    pub base_page_height: f64,
    /// The measured page size in the file's own orientation. Text
    /// and search geometry stay in this space however the view is
    /// turned; `base_page_*` is its display pair.
    pub source_page_width: f64,
    pub source_page_height: f64,
    pub doc_tx: Option<async_channel::Sender<PdfDocRequest>>,
    /// True once `DocumentLoaded` applied the doc service's `Info` answer.
    pub doc_ready: bool,
    /// A settings-driven viewport rebuild (scroll mode, spreads, gap) has
    /// installed a fresh child and is waiting for GTK to allocate it; the
    /// deferred "scroll back to the current page" runs on `LayoutSettled`
    /// once the new extent is real. See `PdfReaderMsg::LayoutSettled`.
    pub viewport_rebuild_pending: bool,
    /// Pages whose vector text extraction is in flight (dedup guard).
    pub text_in_progress: HashSet<usize>,
    /// A click waiting for its page's text layer (2.20 step 3), with the
    /// page it belongs to. Replayed when that page's text arrives.
    pub pending_selection: Option<(usize, PdfPendingSelection)>,
    /// The brief "Recognizing page..." signal shown while a pending
    /// selection waits for its text.
    pub recognizing_popover: Option<gtk::Popover>,
    /// The reading theme the EPUB reader uses (shared `reader.theme`
    /// pref). The PDF reader takes its selection-handle colour from it so
    /// both readers show identical grips (2.20 step 5).
    pub reading_theme: ReadingTheme,
    /// True while a selection handle is being dragged. Shared with the
    /// per-page motion controller so the closed hand stays closed for the
    /// whole drag instead of flickering back to the open hand.
    pub handle_dragging: std::rc::Rc<std::cell::Cell<bool>>,
    pub page_overlays: HashMap<PageSlot, gtk::Overlay>,
    pub page_draw_areas: HashMap<PageSlot, gtk::DrawingArea>,
    pub page_text_cache: HashMap<usize, PdfPageText>,
    pub active_selection: std::rc::Rc<std::cell::RefCell<Option<PdfActiveSelection>>>,
    pub selection_drag_state: Option<(PageSlot, f64, f64, Option<PdfHandleDrag>, bool)>,
    pub selection_chip: Option<gtk::Popover>,
    /// The Define card and the word its save toggle answers for.
    pub dict_popover: Option<gtk::Popover>,
    pub dict_lookup_word: Option<String>,
    pub dict_lookup_def: Option<String>,
    pub dict_lookup_page: usize,
    pub dict_context: Option<String>,
    pub ocr_tx: Option<async_channel::Sender<crate::ocr::PdfOcrRequest>>,
    pub ocr_in_progress: HashSet<usize>,
    /// Fingerprint of the book file at the moment an in-flight OCR was
    /// triggered. A finished result is only persisted to the OCR page cache
    /// if the file is still unchanged, so a file replaced mid-OCR can never
    /// be cached under the wrong fingerprint.
    pub ocr_started_fp: HashMap<usize, String>,
    pub show_zoom_osd: bool,
    pub zoom_osd_seq: u64,
    pub search_active: bool,
    pub search_query: String,
    pub search_results: Vec<PdfSearchResult>,
    /// The current match's position inside the scoped results,
    /// not its raw whole-book index (EPUB T4's rule).
    pub search_index: usize,
    pub search_scope: PdfSearchScope,
    /// The page Chapter scope means: where the reader stood when
    /// this search opened, when its query last changed, or when
    /// they last chose Chapter. Result jumps must not quietly
    /// move the chapter out from under the count.
    pub search_chapter_anchor_page: usize,
    /// The page a fresh result set starts from, captured with
    /// the query rather than read after its first jump.
    pub search_query_anchor_page: usize,
    pub search_pending: bool,
    pub search_no_text: bool,
    /// The results list has taken the left panel over (decision
    /// 61); the tab it displaced is remembered for its close.
    pub search_results_open: bool,
    pub search_results_return_tab: PdfLeftTab,
    pub search_scope_control: Option<crate::widgets::segmented::SegmentedControl>,
    pub search_results_widgets: Option<PdfSearchResultsWidgets>,
    pub search_highlights: std::rc::Rc<std::cell::RefCell<Vec<PdfSearchHighlight>>>,
}

impl PdfReaderModel {
    pub fn new(init: PdfReaderInit) -> Self {
        let (title, author, cover_path, file_path) = match init.catalog.get_book(init.book_id) {
            Ok(Some(book)) => (
                book.title,
                book.authors,
                book.cover_path,
                Some(book.file_path),
            ),
            _ => ("PDF Document".to_string(), String::new(), None, None),
        };

        let reading_theme = init
            .catalog
            .get_pref("reader.theme")
            .map(|v| ReadingTheme::from_str_lossy(&v))
            .unwrap_or(ReadingTheme::Sepia);
        let saved_page = match init.catalog.get_reading_progress(init.book_id) {
            Ok(Some((page, _))) if page >= 1 => page,
            _ => 1,
        };

        let arrow_step = init.catalog.get_pref_i64("reader.arrow_step", 45) as f32;

        let _ = init.catalog.mark_book_opened(init.book_id);
        let start_pct = match init.catalog.get_book(init.book_id) {
            Ok(Some(b)) => b.progress as i64,
            _ => 0,
        };
        let session_id = init
            .catalog
            .start_reading_session(init.book_id, start_pct)
            .ok();
        let session_start = std::time::Instant::now();

        // 1. Scroll Mode: Check per-book setting first, fallback to global default, fallback to VerticalScrolling
        let book_scroll = init.catalog.get_pref(&format!("book.{}.pdf.scroll_mode", init.book_id));
        let global_scroll = init.catalog.get_pref("reader.pdf.scroll_mode");
        let scroll_mode = match book_scroll.as_deref().or(global_scroll.as_deref()) {
            Some("page") => PdfScrollMode::PageScrolling,
            Some("horizontal") => PdfScrollMode::HorizontalScrolling,
            Some("wrapped") => PdfScrollMode::WrappedScrolling,
            _ => PdfScrollMode::VerticalScrolling,
        };

        // 2. Spread Mode: Check per-book setting first, fallback to global default, fallback to NoSpreads.
        // Neither stored anywhere means no layout choice has ever been
        // made: the layout then follows the window (decision 53), and
        // NoSpreads below is only the skeleton's provisional shape.
        let book_spread = init.catalog.get_pref(&format!("book.{}.pdf.spread_mode", init.book_id));
        let global_spread = init.catalog.get_pref("reader.pdf.spread_mode");
        let layout_auto = book_spread.is_none() && global_spread.is_none();
        let spread_mode = match book_spread.as_deref().or(global_spread.as_deref()) {
            Some("odd") => PdfSpreadMode::OddSpreads,
            Some("even") => PdfSpreadMode::EvenSpreads,
            _ => PdfSpreadMode::NoSpreads,
        };

        // "Cover on its own" is its own remembered choice. Before it
        // had a pref of its own it lived inside the spread mode, so a
        // stored pairing still answers for it; an unpaired book
        // starts with the cover on its own, as the boards draw it.
        let cover_pref = init
            .catalog
            .get_pref(&format!("book.{}.pdf.cover_on_its_own", init.book_id))
            .or_else(|| init.catalog.get_pref("reader.pdf.cover_on_its_own"));
        let cover_on_its_own = match cover_pref.as_deref() {
            Some("1") => true,
            Some("0") => false,
            _ => spread_mode != PdfSpreadMode::EvenSpreads,
        };

        // 3. Fit and zoom: specific to this document. The zoom the
        // pane shows is a percentage of the fit; a book last read by
        // an older Kalam stored one absolute scale instead, which
        // opens as Original at that same effective size rather than
        // silently resizing under its reader.
        let book_fit = init.catalog.get_pref(&format!("book.{}.pdf.fit_mode", init.book_id));
        let book_zoom_percent = init
            .catalog
            .get_pref(&format!("book.{}.pdf.zoom_percent", init.book_id))
            .and_then(|z| z.parse::<i64>().ok())
            .map(|z| z.clamp(PDF_MIN_ZOOM_PERCENT as i64, PDF_MAX_ZOOM_PERCENT as i64) as u32);
        let legacy_zoom = init
            .catalog
            .get_pref(&format!("book.{}.pdf.zoom", init.book_id))
            .and_then(|z| z.parse::<f64>().ok())
            .filter(|z| z.is_finite() && *z > 0.0);
        let (fit_mode, zoom_percent) = match (PdfFitMode::from_pref(book_fit.as_deref()), book_zoom_percent) {
            (Some(fit), percent) => (fit, percent.unwrap_or(100)),
            (None, Some(percent)) => (PdfFitMode::Height, percent),
            (None, None) => match legacy_zoom {
                Some(scale) => (
                    PdfFitMode::Original,
                    ((scale * 100.0).round() as i64)
                        .clamp(PDF_MIN_ZOOM_PERCENT as i64, PDF_MAX_ZOOM_PERCENT as i64)
                        as u32,
                ),
                None => (PdfFitMode::Height, 100),
            },
        };
        let rotation_degrees = normalize_pdf_rotation(
            init.catalog
                .get_pref_i64(&format!("book.{}.pdf.rotation", init.book_id), 0) as i32,
        );
        // Provisional until the document's real page size and the
        // viewport's real size are both known; DocumentLoaded and
        // the first resize recompute it from the fit.
        let (provisional_w, provisional_h) = display_page_dimensions(595.0, 842.0, rotation_degrees);
        let zoom_level = pdf_effective_zoom(
            pdf_fit_zoom(
                fit_mode,
                850.0 - 32.0,
                900.0 - 100.0,
                provisional_w,
                provisional_h,
                spread_mode != PdfSpreadMode::NoSpreads
                    && scroll_mode != PdfScrollMode::WrappedScrolling,
                12,
            ),
            zoom_percent,
        );

        // 4. Gaps: the two-page seam keeps its global pref; the gap
        // between successive pages in a scroll is per book, like the
        // rest of the page style, with the global as its seed.
        let two_page_gap = (init.catalog.get_pref_i64("reader.pdf.two_page_gap", 12) as i32).clamp(0, 48);
        let book_scroll_gap = init
            .catalog
            .get_pref(&format!("book.{}.pdf.scroll_gap", init.book_id))
            .and_then(|g| g.parse::<i64>().ok());
        let scroll_gap = book_scroll_gap
            .unwrap_or_else(|| init.catalog.get_pref_i64("reader.pdf.scroll_gap", 24))
            .clamp(0, PDF_MAX_SCROLL_GAP as i64) as i32;

        // 5. The Image pane's state (decisions 54–55): remembered
        // for this book, seeded by the last book's answers. Trim
        // margins is the older smart crop, so it keeps answering to
        // its original per-book pref. A book with nothing stored
        // follows the theme in force: Tint in the dark, untouched
        // paper in the light.
        let smart_crop_pref = init
            .catalog
            .get_pref(&format!("book.{}.pdf.smart_crop", init.book_id));
        let smart_crop = smart_crop_pref.as_deref() == Some("1");
        let ui_theme = crate::theme::current(&init.catalog);
        let image_paper_rgb = image::rgb_from_hex(ui_theme.paper);
        let mut image_settings =
            PdfImageSettings::defaults_for_theme(ui_theme.dark, smart_crop);
        let page_colour_pref = init
            .catalog
            .get_pref(&format!("book.{}.pdf.image.page_colour", init.book_id))
            .or_else(|| init.catalog.get_pref("reader.pdf.image.page_colour"));
        if let Some(page_colour) = PdfPageColour::from_pref(page_colour_pref.as_deref()) {
            image_settings.page_colour = page_colour;
        }
        let book_image_i64 = |name: &str| {
            init.catalog
                .get_pref(&format!("book.{}.pdf.image.{}", init.book_id, name))
                .and_then(|value| value.parse::<i64>().ok())
        };
        let image_i64 = |name: &str, default: i64| {
            book_image_i64(name).unwrap_or_else(|| {
                init.catalog
                    .get_pref_i64(&format!("reader.pdf.image.{name}"), default)
            })
        };
        image_settings.tint_strength =
            image_i64("tint_strength", PdfImageSettings::DEFAULT_TINT_STRENGTH as i64) as i32;
        image_settings.brightness = image_i64("brightness", 0) as i32;
        image_settings.contrast = image_i64("contrast", 0) as i32;
        image_settings.warmth = image_i64("warmth", 0) as i32;
        image_settings.grayscale = image_i64("grayscale", 0) != 0;
        let image_settings = image_settings.canonical();

        // 6. Panel pins are remembered per side (decision 57's
        // approved assumption). They describe the next open as much
        // as this one: a pinned side reopens as page layout, not as
        // a floating layer.
        let left_panel_pinned =
            init.catalog.get_pref_i64("reader.pdf.left_panel_pinned", 0) != 0;
        let right_panel_pinned =
            init.catalog.get_pref_i64("reader.pdf.right_panel_pinned", 0) != 0;

        // The Pages grid's density is a reader preference rather
        // than a property of one document; the board's three
        // columns are the starting answer.
        let pages_columns =
            if init.catalog.get_pref_i64("reader.pdf.pages_columns", 3) == 2 {
                2
            } else {
                3
            };

        // The drawing tools' remembered colour and weight, and the
        // accent a selected drawing's outline paints in.
        let draw_settings = PdfDrawingSettings::load(&init.catalog);
        let draw_accent_rgb = image::rgb_from_hex(ui_theme.accent);

        let mut model = Self {
            book_id: init.book_id,
            catalog: init.catalog,
            title,
            author,
            cover_path,
            file_path: file_path.clone(),
            current_page: saved_page,
            total_pages: 1,
            viewport_rebuild_pending: false,
            zoom_level,
            scroll_mode,
            spread_mode,
            two_page_gap,
            image_settings,
            image_paper_rgb,
            scroll_gap,
            cover_on_its_own,
            layout_auto,
            fit_mode,
            zoom_percent,
            pre_zoom_fit: None,
            rotation_degrees,
            left_tab: PdfLeftTab::Contents,
            right_tab: PdfRightTab::Reading,
            show_back_button: false,
            left_panel_open: false,
            right_panel_open: false,
            left_panel_pinned,
            right_panel_pinned,
            back_hover: false,
            mouse_in_left_edge: false,
            mouse_in_right_edge: false,
            mouse_in_left_panel: false,
            mouse_in_right_panel: false,
            back_gen: 0,
            left_close_seq: 0,
            right_close_seq: 0,
            jump_open: false,
            jump_text: String::new(),
            jump_mode: PdfJumpMode::Page,
            toc_filter: String::new(),
            collapsed_toc_entries: HashSet::new(),
            toc_entries: Vec::new(),
            page_labels: Vec::new(),
            bookmarks: Vec::new(),
            annotations: Vec::new(),
            saved_words: Vec::new(),
            page_identities: Vec::new(),
            drawings: Vec::new(),
            drawing_paints: Rc::new(std::cell::RefCell::new(Vec::new())),
            active_drawing: Rc::new(std::cell::RefCell::new(None)),
            draw_tool: PdfDrawTool::Select,
            draw_tool_shared: Rc::new(std::cell::Cell::new(PdfDrawTool::Select)),
            draw_settings,
            draw_accent_rgb,
            draw_toolbar_visible: false,
            draw_toolbar_zone_hover: false,
            draw_toolbar_hover: false,
            draw_toolbar_seq: 0,
            draw_options_open: false,
            drawing_widgets: None,
            drawing_drag: None,
            selected_drawing_id: None,
            drawing_popover: None,
            notes_filter: PdfNotesFilter::Highlights,
            highlight_colour_filter: None,
            highlight_search_open: false,
            highlight_search_query: String::new(),
            word_scope: PdfWordScope::Book,
            word_search_query: String::new(),
            deleted_marks: Vec::new(),
            notes_words_page_synced: 0,
            textures: HashMap::new(),
            pending_loads: HashSet::new(),
            arrow_step,
            is_loading: true,
            open_state: PdfOpenState::Opening,
            session_password: Arc::new(std::sync::RwLock::new(None)),
            password_has_text: false,
            open_state_removing: false,
            session_id,
            session_start,
            page_pictures: HashMap::new(),
            paged_picture: None,
            paged_loading_box: None,
            paged_label: None,
            two_page_left_pic: None,
            two_page_right_pic: None,
            two_page_loading_box: None,
            two_page_label: None,
            left_stack: None,
            right_stack: None,
            left_tabs: None,
            right_tabs: None,
            jump_mode_control: None,
            back_unit: None,
            toc_list_box: None,
            toc_scroll: None,
            toc_rows: Vec::new(),
            toc_current_visible_row: None,
            toc_current_page_synced: 0,
            notes_widgets: None,
            pages_grid: None,
            pages_widgets: None,
            pages_columns,
            thumbnail_tx: None,
            thumbnail_generation: 1,
            thumbnail_active_generation: None,
            thumbnail_pending: HashSet::new(),
            settings_widgets: None,
            image_widgets: None,
            screenshot_action_applied: false,
            screenshot_password_action_applied: false,
            render_tx: None,
            render_generation: 1,
            active_generation: None,
            base_page_width: provisional_w,
            base_page_height: provisional_h,
            source_page_width: 595.0,
            source_page_height: 842.0,
            doc_tx: None,
            doc_ready: false,
            text_in_progress: HashSet::new(),
            pending_selection: None,
            recognizing_popover: None,
            reading_theme,
            handle_dragging: std::rc::Rc::new(std::cell::Cell::new(false)),
            page_overlays: HashMap::new(),
            page_draw_areas: HashMap::new(),
            page_text_cache: HashMap::new(),
            active_selection: std::rc::Rc::new(std::cell::RefCell::new(None)),
            selection_drag_state: None,
            selection_chip: None,
            dict_popover: None,
            dict_lookup_word: None,
            dict_lookup_def: None,
            dict_lookup_page: saved_page,
            dict_context: None,
            ocr_tx: None,
            ocr_in_progress: HashSet::new(),
            ocr_started_fp: HashMap::new(),
            show_zoom_osd: false,
            zoom_osd_seq: 0,
            search_active: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_index: 0,
            search_scope: PdfSearchScope::Chapter,
            search_chapter_anchor_page: saved_page,
            search_query_anchor_page: saved_page,
            search_pending: false,
            search_no_text: false,
            search_results_open: false,
            search_results_return_tab: PdfLeftTab::Contents,
            search_scope_control: None,
            search_results_widgets: None,
            search_highlights: std::rc::Rc::new(std::cell::RefCell::new(Vec::new())),
        };

        model.reload_bookmarks();
        model.reload_annotations();
        model.reload_saved_words();
        model.reload_drawings();

        model
    }

    pub fn reload_bookmarks(&mut self) {
        self.bookmarks = self
            .catalog
            .list_reading_bookmarks(self.book_id)
            .unwrap_or_default();
    }

    pub fn reload_annotations(&mut self) {
        self.annotations = self
            .catalog
            .get_annotations_for_book(self.book_id)
            .unwrap_or_default();
    }

    pub fn reload_saved_words(&mut self) {
        self.saved_words = self.catalog.list_saved_words("", None).unwrap_or_default();
    }

    /// Keep a deletion for Ctrl+Z (decision 30): the last
    /// several, for this reading session only.
    pub fn remember_deleted(&mut self, mark: PdfDeletedMark) {
        const KEPT: usize = 20;
        self.deleted_marks.push(mark);
        if self.deleted_marks.len() > KEPT {
            self.deleted_marks.remove(0);
        }
    }

    pub fn refresh_notes_panel(&mut self, sender: &ComponentSender<Self>) {
        self.notes_words_page_synced = self.current_page;
        notes::populate_notes_panel(self, sender);
    }

    pub fn progress_fraction(&self) -> f64 {
        if self.total_pages > 0 {
            (self.current_page as f64 / self.total_pages as f64).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    pub fn save_progress(&self) {
        if self.total_pages == 0 {
            return;
        }
        let fraction = self.progress_fraction();
        let _ = self.catalog.set_reading_progress(
            self.book_id,
            self.current_page,
            fraction,
            self.total_pages,
        );

        if let Some(sid) = self.session_id {
            let elapsed = self.session_start.elapsed().as_secs() as i64;
            let end_pct = (fraction * 100.0).round() as i64;
            let _ = self.catalog.checkpoint_reading_session(sid, elapsed, end_pct);
        }
    }

    /// Calculate left and optional right pages for a given target page.
    pub fn spread_for_page(&self, page: usize) -> (usize, Option<usize>) {
        calculate_spread_for_page(self.spread_mode, self.total_pages, page)
    }

    /// Return the spread preceding the given spread, or None if at the first spread.
    pub fn prev_spread(&self, spread: (usize, Option<usize>)) -> Option<(usize, Option<usize>)> {
        calculate_prev_spread(self.spread_mode, self.total_pages, spread)
    }

    /// Return the spread following the given spread, or None if at the last spread.
    pub fn next_spread(&self, spread: (usize, Option<usize>)) -> Option<(usize, Option<usize>)> {
        calculate_next_spread(self.spread_mode, self.total_pages, spread)
    }

    /// Return list of (left, right) page spreads for streaming.
    pub fn all_spreads(&self) -> Vec<(usize, Option<usize>)> {
        calculate_all_spreads(self.spread_mode, self.total_pages)
    }

    /// The pages the Pages grid rings as current. A spread rings
    /// both of its pages (decision 59); wrapped scrolling reads
    /// as single pages everywhere else in the reader, so it rings
    /// only the page the location names.
    pub fn pages_current_pages(&self) -> HashSet<usize> {
        pdf_pages_current_pages(
            self.spread_mode,
            self.scroll_mode,
            self.total_pages,
            self.current_page,
        )
    }

    fn pages_heading(&self, first_visible_page: usize) -> PdfPagesHeading {
        pages_heading_for_page(
            &self.toc_entries,
            self.total_pages,
            first_visible_page,
            &self.title,
        )
    }

    /// The annotation toolbar's rest and grace (decision 66):
    /// up after 250 ms at the bottom centre, away 300 ms after
    /// the pointer leaves it and its zone.
    pub fn schedule_draw_toolbar_reveal(&mut self, sender: &ComponentSender<Self>) {
        self.draw_toolbar_seq = self.draw_toolbar_seq.wrapping_add(1);
        let seq = self.draw_toolbar_seq;
        let s = sender.clone();
        glib::timeout_add_local_once(Duration::from_millis(250), move || {
            let _ = s.input_sender().send(PdfReaderMsg::DrawToolbarRevealDue(seq));
        });
    }

    pub fn schedule_draw_toolbar_hide(&mut self, sender: &ComponentSender<Self>) {
        self.draw_toolbar_seq = self.draw_toolbar_seq.wrapping_add(1);
        let seq = self.draw_toolbar_seq;
        let s = sender.clone();
        glib::timeout_add_local_once(Duration::from_millis(300), move || {
            let _ = s.input_sender().send(PdfReaderMsg::DrawToolbarHideDue(seq));
        });
    }

    pub fn schedule_left_close(&mut self, sender: &ComponentSender<Self>) {
        self.left_close_seq = self.left_close_seq.wrapping_add(1);
        let seq = self.left_close_seq;
        let s = sender.clone();
        glib::timeout_add_local_once(Duration::from_millis(600), move || {
            let _ = s.input_sender().send(PdfReaderMsg::LeftCloseTimerTick(seq));
        });
    }

    pub fn schedule_right_close(&mut self, sender: &ComponentSender<Self>) {
        self.right_close_seq = self.right_close_seq.wrapping_add(1);
        let seq = self.right_close_seq;
        let s = sender.clone();
        glib::timeout_add_local_once(Duration::from_millis(600), move || {
            let _ = s.input_sender().send(PdfReaderMsg::RightCloseTimerTick(seq));
        });
    }

    /// The plain bottom-right location (decision 52): both pages of
    /// a spread, or the one page in front of the reader, led by the
    /// document's own page labels where they differ from the
    /// physical pages (`iv · 8 / 612`).
    pub fn location_label(&self) -> String {
        format_location_label(
            self.spread_mode,
            self.scroll_mode,
            self.total_pages,
            self.current_page,
            &self.page_labels,
        )
    }

    pub fn jump_max(&self) -> usize {
        match self.jump_mode {
            PdfJumpMode::Page => self.total_pages,
            PdfJumpMode::Chapter => {
                pdf_jump_chapter_pages(&self.toc_entries, self.total_pages).len()
            }
        }
    }

    pub fn jump_current(&self) -> usize {
        pdf_jump_current(
            self.jump_mode,
            &self.toc_entries,
            self.total_pages,
            self.current_page,
        )
    }

    pub fn jump_out_of_range(&self) -> bool {
        if self.jump_text.trim().is_empty() {
            return false;
        }
        pdf_jump_target(
            self.jump_mode,
            &self.jump_text,
            &self.toc_entries,
            self.total_pages,
        )
        .is_none()
    }

    pub fn jump_hint(&self) -> String {
        if self.jump_out_of_range() {
            match self.jump_mode {
                PdfJumpMode::Page => format!("Pages 1–{}", self.total_pages.max(1)),
                PdfJumpMode::Chapter => format!("Chapters 1–{}", self.jump_max().max(1)),
            }
        } else {
            format!("of {}", self.jump_max())
        }
    }

    pub fn jump_target_page(&self) -> Option<usize> {
        pdf_jump_target(
            self.jump_mode,
            &self.jump_text,
            &self.toc_entries,
            self.total_pages,
        )
    }

    pub fn search_scoped_indices(&self) -> Vec<usize> {
        pdf_search_scoped_indices(
            &self.search_results,
            &self.toc_entries,
            self.total_pages,
            self.search_scope,
            self.search_chapter_anchor_page,
            &self.title,
        )
    }

    pub fn current_search_result_index(&self) -> Option<usize> {
        self.search_scoped_indices()
            .get(self.search_index)
            .copied()
    }

    pub fn search_has_matches(&self) -> bool {
        !self.search_scoped_indices().is_empty()
    }

    pub fn search_controls_visible(&self) -> bool {
        self.search_active
            && !self.search_pending
            && !self.search_no_text
            && self.search_has_matches()
    }

    pub fn search_has_no_matches(&self) -> bool {
        self.search_active
            && self.search_query.trim().chars().count() >= 2
            && !self.search_pending
            && !self.search_no_text
            && !self.search_has_matches()
    }

    pub fn search_status_is_accent(&self) -> bool {
        self.search_no_text || self.search_has_no_matches()
    }

    /// The one line the pill can say besides a count. *No
    /// matches* and *no searchable text* are the board's exact
    /// words; an untouched or still-running search stays quiet.
    pub fn search_count_text(&self) -> String {
        if !self.search_active
            || self.search_query.trim().chars().count() < 2
            || self.search_pending
        {
            return String::new();
        }
        if self.search_no_text {
            return "This book has no searchable text".to_string();
        }
        let scoped = self.search_scoped_indices();
        if scoped.is_empty() {
            "No matches".to_string()
        } else {
            format!(
                "{} of {}",
                self.search_index.min(scoped.len() - 1) + 1,
                scoped.len()
            )
        }
    }

    /// Move the Contents current mark after a page turn. The row
    /// widgets survive page turns; rebuilding them here would throw
    /// away the reader's place in a pinned Contents list.
    pub fn sync_toc_current(&mut self) {
        if self.toc_current_page_synced == self.current_page {
            return;
        }
        self.toc_current_page_synced = self.current_page;
        let current = current_toc_index(&self.toc_entries, self.current_page, self.total_pages);
        self.toc_current_visible_row = self
            .toc_rows
            .iter()
            .position(|(index, _)| Some(*index) == current);
        for (index, row) in &self.toc_rows {
            if Some(*index) == current {
                row.add_css_class("active");
            } else {
                row.remove_css_class("active");
            }
        }
    }

    /// Contents opens on the branch being read (decision 58): a
    /// branch the reader folded earlier in the session does not
    /// stay folded over the current section.
    pub fn expand_current_toc_branch(&mut self) {
        let Some(current) = current_toc_index(
            &self.toc_entries,
            self.current_page,
            self.total_pages,
        ) else {
            return;
        };
        let parents = toc_parent_indices(&self.toc_entries);
        let mut cursor = parents.get(current).copied().flatten();
        while let Some(parent) = cursor {
            self.collapsed_toc_entries.remove(&parent);
            cursor = parents.get(parent).copied().flatten();
        }
    }

    pub fn trigger_zoom_osd(&mut self, sender: &ComponentSender<Self>) {
        self.show_zoom_osd = true;
        self.zoom_osd_seq = self.zoom_osd_seq.wrapping_add(1);
        let seq = self.zoom_osd_seq;
        let s = sender.clone();
        glib::timeout_add_local_once(Duration::from_millis(1000), move || {
            let _ = s.input_sender().send(PdfReaderMsg::HideZoomOsd(seq));
        });
    }

    pub fn update_scroll_policies(&self, scroll: &gtk::ScrolledWindow) {
        match self.scroll_mode {
            PdfScrollMode::HorizontalScrolling => {
                scroll.set_hscrollbar_policy(gtk::PolicyType::Always);
                scroll.set_vscrollbar_policy(gtk::PolicyType::Never);
            }
            PdfScrollMode::PageScrolling => {
                scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
                scroll.set_vscrollbar_policy(gtk::PolicyType::Never);
            }
            PdfScrollMode::WrappedScrolling => {
                scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
                scroll.set_vscrollbar_policy(gtk::PolicyType::Always);
            }
            PdfScrollMode::VerticalScrolling => {
                let page_w = (self.base_page_width * self.zoom_level) as i32;
                let content_w = if self.spread_mode == PdfSpreadMode::NoSpreads {
                    page_w + 32
                } else {
                    page_w * 2 + self.two_page_gap + 32
                };
                let page_size = scroll.hadjustment().page_size() as i32;
                let vp_w = if page_size > 0 { page_size } else { scroll.width() };
                if vp_w > 0 && content_w > vp_w {
                    scroll.set_hscrollbar_policy(gtk::PolicyType::Automatic);
                } else {
                    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
                }
                scroll.set_vscrollbar_policy(gtk::PolicyType::Always);
            }
        }
    }

    /// Keep the display page pair in step with the source pair and
    /// the view rotation. Every allocation in the viewport reads
    /// `base_page_*`, so this one assignment is what turns the
    /// whole layout; the rasterizer turns the page itself.
    pub fn sync_display_page_dimensions(&mut self) {
        let (w, h) = display_page_dimensions(
            self.source_page_width,
            self.source_page_height,
            self.rotation_degrees,
        );
        self.base_page_width = w;
        self.base_page_height = h;
    }

    /// The scale the selected fit asks for in this viewport, before
    /// the zoom percentage. The margins are the ones the viewport
    /// builders actually lay down, so a fit page is neither clipped
    /// by its own padding nor shrunk by padding it does not have.
    pub fn compute_fit_zoom(&self, scroll: &gtk::ScrolledWindow) -> f64 {
        let hadj_size = scroll.hadjustment().page_size();
        let vadj_size = scroll.vadjustment().page_size();
        let w = if hadj_size > 100.0 {
            hadj_size
        } else if scroll.width() > 100 {
            scroll.width() as f64
        } else {
            850.0
        };
        let h = if vadj_size > 100.0 {
            vadj_size
        } else if scroll.height() > 100 {
            scroll.height() as f64
        } else {
            900.0
        };
        let (margin_w, margin_h) = match self.scroll_mode {
            PdfScrollMode::PageScrolling => (32.0, 100.0),
            PdfScrollMode::VerticalScrolling | PdfScrollMode::WrappedScrolling => (32.0, 124.0),
            PdfScrollMode::HorizontalScrolling => (88.0, 92.0),
        };
        pdf_fit_zoom(
            self.fit_mode,
            (w - margin_w).max(100.0),
            (h - margin_h).max(100.0),
            self.base_page_width,
            self.base_page_height,
            self.spread_mode != PdfSpreadMode::NoSpreads
                && self.scroll_mode != PdfScrollMode::WrappedScrolling,
            self.two_page_gap,
        )
    }

    /// Recompute the effective scale from fit + zoom percentage.
    /// Returns whether it moved; callers that are about to rebuild
    /// the viewport use that to retire the old render generation,
    /// callers that are not go through `apply_zoom_change`.
    pub fn refresh_fit_zoom(&mut self, scroll: &gtk::ScrolledWindow) -> bool {
        let next = pdf_effective_zoom(self.compute_fit_zoom(scroll), self.zoom_percent);
        if (next - self.zoom_level).abs() > 0.005 {
            self.zoom_level = next;
            true
        } else {
            false
        }
    }

    /// Retire every in-flight and cached raster: the next
    /// `trigger_loads` re-renders at the new scale or rotation
    /// instead of accepting textures from the old one.
    pub fn invalidate_renders(&mut self) {
        self.render_generation = self.render_generation.wrapping_add(1);
        if let Some(ref gen) = self.active_generation {
            gen.store(self.render_generation, Ordering::Relaxed);
        }
        self.pending_loads.clear();
    }

    /// Point the standing Pages grid at the document facts now in
    /// hand. The grid only replaces its list model when the page
    /// count changes, so calling this from panel refreshes does
    /// not spend the reader's place in a long grid.
    pub fn sync_pages_grid_document(&self) {
        if let Some(grid) = &self.pages_grid {
            grid.set_document(
                if self.doc_ready { self.total_pages } else { 0 },
                self.pages_current_pages(),
                self.base_page_height / self.base_page_width.max(1.0),
                self.image_settings,
                self.image_paper_rgb,
            );
        }
        self.update_pages_heading();
    }

    pub fn sync_pages_current(&self) {
        if let Some(grid) = &self.pages_grid {
            grid.set_current_pages(self.pages_current_pages());
        }
    }

    /// The sticky Pages heading: the top-level chapter at the
    /// grid's top edge, its span, and `reading` when that span is
    /// the one on the page behind the panel.
    pub fn update_pages_heading(&self) {
        let Some(pages) = &self.pages_widgets else {
            return;
        };
        toggle_active(&pages.columns_3_button, self.pages_columns == 3);
        toggle_active(&pages.columns_2_button, self.pages_columns == 2);

        if !self.doc_ready {
            pages.heading_title.set_label(&self.title);
            pages.heading_meta.set_label("Opening PDF...");
            return;
        }

        let heading_anchor = self
            .pages_grid
            .as_ref()
            .and_then(|grid| grid.heading_anchor_page(pages.scroll.vadjustment().value()))
            .unwrap_or(self.current_page);
        let heading = self.pages_heading(heading_anchor);
        let page_word = if heading.page_count() == 1 {
            "page"
        } else {
            "pages"
        };
        let meta = if heading.contains(self.current_page) {
            format!("{} {page_word} · reading", heading.page_count())
        } else {
            format!("{} {page_word}", heading.page_count())
        };
        pages.heading_title.set_label(&heading.title);
        pages.heading_meta.set_label(&meta);
    }

    /// Open Pages where the reader is standing. `scroll_to` first
    /// makes GridView realise the right window; the idle pass
    /// then leaves one row of context above the current spread.
    pub fn scroll_pages_to_current(&self) {
        let (Some(grid), Some(pages)) = (&self.pages_grid, &self.pages_widgets) else {
            return;
        };
        grid.scroll_to_current();
        let grid = grid.clone();
        let adjustment = pages.scroll.vadjustment();
        glib::idle_add_local_once(move || {
            let max = (adjustment.upper() - adjustment.page_size()).max(0.0);
            if let Some(target) = grid.opening_scroll_target(max, adjustment.page_size()) {
                adjustment.set_value(target);
            }
        });
    }

    /// Ask the thumbnail worker for one page the grid has just
    /// bound. Existing, failed and already-pending pages stop
    /// here, which is what keeps cell recycling from becoming a
    /// stream of duplicate MuPDF renders while scrolling.
    pub fn request_page_thumbnail(&mut self, page: usize) {
        if !self.doc_ready || page == 0 || page > self.total_pages {
            return;
        }
        let Some(grid) = self.pages_grid.clone() else {
            return;
        };
        if grid.has_thumbnail_state(page) || !self.thumbnail_pending.insert(page) {
            return;
        }
        let Some(path) = self.file_path.clone() else {
            self.thumbnail_pending.remove(&page);
            grid.update_thumbnail(page, None);
            return;
        };
        let Some(tx) = self.thumbnail_tx.clone() else {
            self.thumbnail_pending.remove(&page);
            grid.update_thumbnail(page, None);
            return;
        };
        let _ = tx.send_blocking(PdfThumbnailRequest {
            generation: self.thumbnail_generation,
            page,
            target_width: pages_grid::THUMBNAIL_TARGET_WIDTH,
            rotation: self.rotation_degrees,
            path,
        });
    }

    /// Retire thumbnails drawn for another orientation. Bound
    /// cells are re-asked explicitly: clearing their textures
    /// does not make GridView bind them again.
    pub fn invalidate_thumbnails(&mut self) {
        self.thumbnail_generation = self.thumbnail_generation.wrapping_add(1);
        if let Some(generation) = &self.thumbnail_active_generation {
            generation.store(self.thumbnail_generation, Ordering::Relaxed);
        }
        self.thumbnail_pending.clear();
        let Some(grid) = self.pages_grid.clone() else {
            return;
        };
        grid.set_page_aspect(self.base_page_height / self.base_page_width.max(1.0));
        let bound_pages = grid.bound_pages();
        grid.clear_thumbnails();
        for page in bound_pages {
            self.request_page_thumbnail(page);
        }
    }

    /// The Reading pane's fit choices are this book's (decision 53:
    /// the choice is remembered per book). The legacy absolute scale
    /// is written alongside, so an older Kalam meeting this book
    /// still opens it at the size its reader left it.
    pub fn persist_fit_zoom(&self) {
        self.catalog.set_pref(
            &format!("book.{}.pdf.fit_mode", self.book_id),
            self.fit_mode.pref_value(),
        );
        self.catalog.set_pref_i64(
            &format!("book.{}.pdf.zoom_percent", self.book_id),
            self.zoom_percent as i64,
        );
        self.catalog.set_pref(
            &format!("book.{}.pdf.zoom", self.book_id),
            &format!("{:.2}", self.zoom_level),
        );
    }

    /// A page surface carrying the Image pane's current answers.
    /// Every viewport builder asks here, so a page materialised
    /// after a slider moved opens already wearing them.
    pub fn make_page_picture(&self) -> PdfPagePicture {
        PdfPagePicture::new(self.image_settings, self.image_paper_rgb)
    }

    /// Repaint every materialised page through the Image pane's
    /// current answers. Colour lives in the paint, not the raster,
    /// so this never retires a render generation.
    pub fn refresh_page_images(&self) {
        for pic in self.page_pictures.values() {
            pic.set_image_settings(self.image_settings, self.image_paper_rgb);
        }
        if let Some(pic) = &self.paged_picture {
            pic.set_image_settings(self.image_settings, self.image_paper_rgb);
        }
        if let Some(pic) = &self.two_page_left_pic {
            pic.set_image_settings(self.image_settings, self.image_paper_rgb);
        }
        if let Some(pic) = &self.two_page_right_pic {
            pic.set_image_settings(self.image_settings, self.image_paper_rgb);
        }
        if let Some(grid) = &self.pages_grid {
            grid.set_image_settings(self.image_settings, self.image_paper_rgb);
        }
    }

    /// Image choices are this book's, with the global pref carrying
    /// them to the next book as its seed — the Reading pane's own
    /// arrangement. Trim margins keeps its older smart-crop pref,
    /// written by `SetSmartCrop`.
    pub fn persist_image_settings(&self) {
        self.catalog.set_pref(
            &format!("book.{}.pdf.image.page_colour", self.book_id),
            self.image_settings.page_colour.pref_value(),
        );
        self.catalog.set_pref(
            "reader.pdf.image.page_colour",
            self.image_settings.page_colour.pref_value(),
        );
        for (name, value) in [
            ("tint_strength", self.image_settings.tint_strength as i64),
            ("brightness", self.image_settings.brightness as i64),
            ("contrast", self.image_settings.contrast as i64),
            ("warmth", self.image_settings.warmth as i64),
            ("grayscale", i64::from(self.image_settings.grayscale)),
        ] {
            self.catalog.set_pref_i64(
                &format!("book.{}.pdf.image.{}", self.book_id, name),
                value,
            );
            self.catalog
                .set_pref_i64(&format!("reader.pdf.image.{name}"), value);
        }
    }

    /// Synchronize the Image pane with active state, on the Reading
    /// pane's terms: sliders only move when they disagree, so the
    /// sync a drag triggers never fights the drag itself.
    pub fn sync_image_ui(&self, iw: &PdfImageWidgets) {
        iw.page_colour_control
            .set_selected(self.image_settings.page_colour.segment_index());
        iw.tint_strength_row
            .set_sensitive(self.image_settings.page_colour == PdfPageColour::Tint);
        for (scale, value) in [
            (&iw.tint_strength_scale, self.image_settings.tint_strength),
            (&iw.brightness_scale, self.image_settings.brightness),
            (&iw.contrast_scale, self.image_settings.contrast),
            (&iw.warmth_scale, self.image_settings.warmth),
        ] {
            if (scale.value() - value as f64).abs() > 0.01 {
                scale.set_value(value as f64);
            }
        }
        if iw.grayscale_switch.is_active() != self.image_settings.grayscale {
            iw.grayscale_switch
                .set_active(self.image_settings.grayscale);
        }
        if iw.trim_switch.is_active() != self.image_settings.trim_margins {
            iw.trim_switch
                .set_active(self.image_settings.trim_margins);
        }
    }

    /// The spread mode the window's own shape asks for, while no
    /// stored layout choice stands (decision 53).
    pub fn desired_auto_spread_mode(&self, scroll: &gtk::ScrolledWindow) -> PdfSpreadMode {
        if pdf_viewport_fits_spread(
            scroll.width() as f64,
            scroll.height() as f64,
            self.base_page_width,
            self.base_page_height,
            self.two_page_gap,
        ) {
            pdf_spread_mode_for_layout(PdfLayout::Spread, self.cover_on_its_own)
        } else {
            PdfSpreadMode::NoSpreads
        }
    }

    /// A display-space point in this page's source points.
    pub fn display_point_to_page(
        &self,
        x: f64,
        y: f64,
        target_w: f64,
        target_h: f64,
        page_text: &PdfPageText,
    ) -> (f32, f32) {
        display_point_to_source(
            x,
            y,
            target_w,
            target_h,
            page_text.width_pts,
            page_text.height_pts,
            self.rotation_degrees,
        )
    }

    /// Source-space highlight rectangles in display pixels, plus
    /// the selection handles for the first and last of them. The
    /// handle bars follow the text's own direction, which is the
    /// view rotation laid over the page's reading direction.
    pub fn page_rects_to_display(
        &self,
        rects: &[(f32, f32, f32, f32)],
        target_w: f64,
        target_h: f64,
        page_text: &PdfPageText,
    ) -> Vec<(f64, f64, f64, f64)> {
        rects
            .iter()
            .map(|rect| {
                source_rect_to_display(
                    *rect,
                    page_text.width_pts,
                    page_text.height_pts,
                    target_w,
                    target_h,
                    self.rotation_degrees,
                )
            })
            .collect()
    }

    pub fn selection_handles(
        &self,
        first: (f64, f64, f64, f64),
        last: (f64, f64, f64, f64),
    ) -> ((f64, f64, f64), (f64, f64, f64)) {
        match normalize_pdf_rotation(self.rotation_degrees) {
            90 => (
                (first.0, first.1, first.2),
                (last.0, last.1 + last.3, last.2),
            ),
            180 => (
                (first.0 + first.2, first.1, first.3),
                (last.0, last.1, last.3),
            ),
            270 => (
                (first.0, first.1 + first.3, first.2),
                (last.0, last.1, last.2),
            ),
            _ => (
                (first.0, first.1, first.3),
                (last.0 + last.2, last.1, last.3),
            ),
        }
    }

    /// The pages a reader "standing" on `current_page` still needs: the
    /// current spread plus `spreads_to_keep` spreads in each direction.
    /// Pure math (no state), so the cache windows can be unit-tested.
    /// Wrapped scrolling reads as single pages, matching `trigger_loads`.
    pub fn keep_window_pages(
        spread_mode: PdfSpreadMode,
        scroll_mode: PdfScrollMode,
        total_pages: usize,
        current_page: usize,
        spreads_to_keep: usize,
    ) -> std::collections::HashSet<usize> {
        let effective_spread_mode = match scroll_mode {
            PdfScrollMode::WrappedScrolling => PdfSpreadMode::NoSpreads,
            _ => spread_mode,
        };

        let cur_spread =
            calculate_spread_for_page(effective_spread_mode, total_pages, current_page);
        let mut keep_pages = std::collections::HashSet::new();

        keep_pages.insert(cur_spread.0);
        if let Some(r) = cur_spread.1 {
            keep_pages.insert(r);
        }

        let mut next_cursor = cur_spread;
        for _ in 0..spreads_to_keep {
            if let Some(ns) = calculate_next_spread(effective_spread_mode, total_pages, next_cursor)
            {
                keep_pages.insert(ns.0);
                if let Some(r) = ns.1 {
                    keep_pages.insert(r);
                }
                next_cursor = ns;
            } else {
                break;
            }
        }

        let mut prev_cursor = cur_spread;
        for _ in 0..spreads_to_keep {
            if let Some(ps) = calculate_prev_spread(effective_spread_mode, total_pages, prev_cursor)
            {
                keep_pages.insert(ps.0);
                if let Some(r) = ps.1 {
                    keep_pages.insert(r);
                }
                prev_cursor = ps;
            } else {
                break;
            }
        }

        keep_pages
    }

    pub fn prune_textures(&mut self) {
        if self.total_pages == 0 {
            return;
        }

        let image_window = match self.scroll_mode {
            PdfScrollMode::PageScrolling => 1,
            _ => 3,
        };
        let keep_pages = Self::keep_window_pages(
            self.spread_mode,
            self.scroll_mode,
            self.total_pages,
            self.current_page,
            image_window,
        );

        let evicted: Vec<usize> = self
            .textures
            .keys()
            .copied()
            .filter(|p| !keep_pages.contains(p))
            .collect();

        for p in &evicted {
            self.textures.remove(p);
            if let Some(pic) = self.page_pictures.get(p) {
                pic.set_texture(None);
                pic.remove_css_class("kalam-pdf-page-image");
                pic.add_css_class("kalam-pdf-placeholder");
            }
        }

        self.pending_loads.retain(|p| keep_pages.contains(p));

        // Text is far cheaper per page than a rendered bitmap (tens of KB,
        // not megabytes), so its keep-window is twice the image window --
        // but it stays bounded: a 10,000-page document cannot accumulate
        // 10,000 text layers while the reader is open.
        let text_keep_pages = Self::keep_window_pages(
            self.spread_mode,
            self.scroll_mode,
            self.total_pages,
            self.current_page,
            image_window * 2,
        );
        self.page_text_cache
            .retain(|p, _| text_keep_pages.contains(p));

        #[cfg(target_os = "linux")]
        if !evicted.is_empty() {
            unsafe {
                libc::malloc_trim(0);
            }
        }
    }

    pub fn cleanup_memory(&mut self) {
        self.dismiss_drawing_popover();
        self.dismiss_dict_popover();
        self.drawing_paints.borrow_mut().clear();
        self.active_drawing.replace(None);
        self.drawing_drag = None;
        self.selected_drawing_id = None;
        self.draw_tool = PdfDrawTool::Select;
        self.draw_tool_shared.set(PdfDrawTool::Select);
        self.draw_toolbar_visible = false;
        self.draw_options_open = false;
        for pic in self.page_pictures.values() {
            pic.set_texture(None);
        }
        if let Some(ref pic) = self.paged_picture {
            pic.set_texture(None);
        }
        if let Some(ref pic) = self.two_page_left_pic {
            pic.set_texture(None);
        }
        if let Some(ref pic) = self.two_page_right_pic {
            pic.set_texture(None);
        }

        self.textures.clear();
        self.page_pictures.clear();
        self.page_overlays.clear();
        self.page_draw_areas.clear();
        self.page_text_cache.clear();
        self.pending_loads.clear();
        self.thumbnail_pending.clear();
        self.thumbnail_generation = self.thumbnail_generation.wrapping_add(1);
        if let Some(generation) = &self.thumbnail_active_generation {
            generation.store(self.thumbnail_generation, Ordering::Relaxed);
        }
        if let Some(grid) = &self.pages_grid {
            grid.clear_thumbnails();
        }
        self.ocr_in_progress.clear();
        self.ocr_started_fp.clear();
        self.text_in_progress.clear();
        self.pending_selection = None;
        self.dismiss_recognizing_popover();
        self.doc_ready = false;
        set_pdf_session_password(&self.session_password, None);
        self.password_has_text = false;
        self.open_state_removing = false;

        self.doc_tx = None;
        self.render_tx = None;
        self.thumbnail_tx = None;
        self.ocr_tx = None;

        #[cfg(target_os = "linux")]
        unsafe {
            libc::malloc_trim(0);
        }
    }
}

impl Drop for PdfReaderModel {
    fn drop(&mut self) {
        self.cleanup_memory();
    }
}

impl PdfReaderModel {

    pub fn trigger_loads(&mut self, _sender: &ComponentSender<Self>) {
        let Some(path) = self.file_path.clone() else {
            return;
        };

        if self.total_pages == 0 || !self.doc_ready {
            return;
        }

        let mut load_order = Vec::new();

        let effective_spread_mode = match self.scroll_mode {
            PdfScrollMode::WrappedScrolling => PdfSpreadMode::NoSpreads,
            _ => self.spread_mode,
        };

        let cur_spread = calculate_spread_for_page(effective_spread_mode, self.total_pages, self.current_page);

        match self.scroll_mode {
            PdfScrollMode::PageScrolling => {
                let mut spreads = vec![cur_spread];
                if let Some(ns) = calculate_next_spread(effective_spread_mode, self.total_pages, cur_spread) {
                    spreads.push(ns);
                }
                if let Some(ps) = calculate_prev_spread(effective_spread_mode, self.total_pages, cur_spread) {
                    spreads.push(ps);
                }
                for (left, right) in spreads {
                    load_order.push(left);
                    if let Some(r) = right {
                        load_order.push(r);
                    }
                }
            }
            _ => {
                let mut spreads = vec![cur_spread];
                let mut next_cursor = cur_spread;
                let mut prev_cursor = cur_spread;
                let mut has_next = true;
                let mut has_prev = true;

                for _ in 0..3 {
                    if has_next {
                        if let Some(ns) = calculate_next_spread(effective_spread_mode, self.total_pages, next_cursor) {
                            spreads.push(ns);
                            next_cursor = ns;
                        } else {
                            has_next = false;
                        }
                    }
                    if has_prev {
                        if let Some(ps) = calculate_prev_spread(effective_spread_mode, self.total_pages, prev_cursor) {
                            spreads.push(ps);
                            prev_cursor = ps;
                        } else {
                            has_prev = false;
                        }
                    }
                }

                for (left, right) in spreads {
                    load_order.push(left);
                    if let Some(r) = right {
                        load_order.push(r);
                    }
                }
            }
        }

        let scale = ((1.5 * self.zoom_level).clamp(0.5, 3.5)) as f32;
        let smart_crop = self.image_settings.trim_margins;
        let rotation = self.rotation_degrees;
        let gen = self.render_generation;

        for page in load_order {
            // Text pre-warm (2.20 step 2): every page whose image is being
            // loaded also gets its text layer requested from the doc
            // service, so selection works the moment the page appears.
            // Runs before the texture check -- a cached image can sit on a
            // page whose text was never extracted.
            let _ = self.ensure_page_text(page);
            if let Some(cached) = self.textures.get(&page) {
                if cached.generation == gen {
                    continue;
                }
            }
            if self.pending_loads.contains(&page) {
                continue;
            }
            self.pending_loads.insert(page);

            if let Some(ref tx) = self.render_tx {
                let _ = tx.send_blocking(PdfRenderRequest {
                    generation: gen,
                    page,
                    scale,
                    smart_crop,
                    rotation,
                    path: path.clone(),
                });
            }
        }
    }

    pub fn page_for_slot(&self, slot: PageSlot) -> Option<usize> {
        match slot {
            PageSlot::Fixed(p) => Some(p),
            PageSlot::PagedSingle => Some(self.current_page),
            PageSlot::PagedSpreadLeft => Some(self.spread_for_page(self.current_page).0),
            PageSlot::PagedSpreadRight => self.spread_for_page(self.current_page).1,
        }
    }

    /// The selection-handle grip colour, identical to the EPUB reader's
    /// (`kalam-reader` prefs): near-black on the light reading themes,
    /// bright amber on the dark ones, so the grips read against the page
    /// (2.20 step 5: same shape, size and colour in both readers).
    pub fn handle_color(&self) -> (f64, f64, f64) {
        match self.reading_theme {
            ReadingTheme::Light | ReadingTheme::Sepia => (11.0 / 255.0, 11.0 / 255.0, 11.0 / 255.0),
            ReadingTheme::Dark | ReadingTheme::Ink => (1.0, 209.0 / 255.0, 102.0 / 255.0),
        }
    }

    pub fn trigger_page_ocr(&mut self, page: usize, fingerprint: Option<String>) {
        if self.page_text_cache.contains_key(&page) {
            return;
        }
        if self.ocr_in_progress.contains(&page) {
            return;
        }
        let Some(ref tx) = self.ocr_tx else {
            return;
        };
        let Some(ref path) = self.file_path else {
            return;
        };

        self.ocr_in_progress.insert(page);
        if let Some(fp) = fingerprint {
            self.ocr_started_fp.insert(page, fp);
        }
        let _ = tx.send_blocking(crate::ocr::PdfOcrRequest {
            generation: self.render_generation,
            page,
            path: path.clone(),
        });
    }

    pub fn ensure_page_text(&mut self, page: usize) -> Option<&PdfPageText> {
        if self.page_text_cache.contains_key(&page) {
            return self.page_text_cache.get(&page);
        }
        // The vector text layer comes from the background doc service; the
        // answer arrives as `PageTextReady` (vector text) or, for scanned
        // pages, via the OCR cache and worker. Nothing here touches the
        // file on the UI thread.
        if self.text_in_progress.insert(page) {
            if let (Some(path), Some(tx)) = (self.file_path.clone(), self.doc_tx.as_ref()) {
                let _ = tx.send_blocking(PdfDocRequest::ExtractText {
                    path,
                    page,
                });
            } else {
                self.text_in_progress.remove(&page);
            }
        }
        self.page_text_cache.get(&page)
    }

    pub fn clear_selection(&mut self) {
        self.active_selection.replace(None);
        self.dismiss_selection_chip();
        // A click elsewhere means the pending one is no longer wanted.
        self.pending_selection = None;
        self.dismiss_recognizing_popover();
        for da in self.page_draw_areas.values() {
            da.queue_draw();
        }
    }

    pub fn dismiss_selection_chip(&mut self) {
        if let Some(popover) = self.selection_chip.take() {
            popover.popdown();
            popover.unparent();
        }
    }

    pub fn dismiss_recognizing_popover(&mut self) {
        if let Some(popover) = self.recognizing_popover.take() {
            popover.popdown();
            popover.unparent();
        }
    }

    /// Park the page cursor on what the motion controller would show at
    /// (`x`, `y`): the open hand over a selection grip, the page's text
    /// cursor anywhere else. Called at drag release -- no motion event
    /// arrives after it until the pointer moves again, so whatever the
    /// release sets is what the user sees (pitfalls 34).
    fn park_cursor_at(&self, slot: PageSlot, x: f64, y: f64) {
        let Some(ov) = self.page_overlays.get(&slot) else { return };
        if !self.draw_tool.is_select() {
            ov.set_cursor_from_name(Some("crosshair"));
            return;
        }
        let over_handle = self
            .active_selection
            .borrow()
            .as_ref()
            .filter(|sel| sel.slot == slot && !sel.is_block)
            .is_some_and(|sel| over_selection_handle(sel, x, y));
        ov.set_cursor_from_name(if over_handle {
            Some("grab")
        } else {
            Some("text")
        });
    }

    /// Store a click that arrived before its page's text layer, and show
    /// the brief "Recognizing page..." signal at the click point (owner
    /// decision 2026-10-01: show it -- silence is what made the first
    /// click look broken).
    pub fn store_pending_selection(
        &mut self,
        page: usize,
        intent: PdfPendingSelection,
        slot: PageSlot,
        x: f64,
        y: f64,
    ) {
        self.pending_selection = Some((page, intent));
        self.dismiss_recognizing_popover();
        let Some(overlay) = self.page_overlays.get(&slot) else { return };

        let popover = gtk::Popover::new();
        popover.set_parent(overlay);
        popover.set_autohide(false);
        popover.set_has_arrow(false);
        popover.set_position(gtk::PositionType::Bottom);
        let anchor = gdk::Rectangle::new(x as i32, (y - 6.0).max(0.0) as i32, 1, 1);
        popover.set_pointing_to(Some(&anchor));
        let label = gtk::Label::new(Some("Recognizing page..."));
        label.add_css_class("dim-label");
        popover.set_child(Some(&label));
        popover.popup();

        // If recognition never produces text (OCR failed, file replaced),
        // the signal must not linger over the page forever.
        let timed_out = popover.clone();
        glib::timeout_add_local_once(std::time::Duration::from_secs(10), move || {
            if timed_out.parent().is_some() {
                timed_out.popdown();
                timed_out.unparent();
            }
        });

        self.recognizing_popover = Some(popover);
    }

    /// Replay a stored click once its page's text has arrived (2.20 step
    /// 3). Called from every handler that inserts page text.
    pub fn try_replay_pending_selection(&mut self, page: usize, sender: &ComponentSender<Self>) {
        // If the user moved on and the page is no longer materialized in
        // the viewport, the intent is stale: it is dropped instead of
        // selecting a word on a page nobody is looking at.
        let materialized = self.page_pictures.contains_key(&page);
        match Self::take_pending_replay(&mut self.pending_selection, page, materialized) {
            PdfPendingOutcome::Replay(msg) => {
                self.dismiss_recognizing_popover();
                let _ = sender.input_sender().send(msg);
            }
            PdfPendingOutcome::Dropped => {
                self.dismiss_recognizing_popover();
            }
            PdfPendingOutcome::KeepWaiting => {}
        }
    }

    pub fn show_selection_chip(&mut self, slot: PageSlot, sender: &ComponentSender<Self>) {
        self.dismiss_selection_chip();
        let sel_opt = self.active_selection.borrow().clone();
        let Some(sel) = sel_opt else { return };
        if sel.text.is_empty() { return };
        let Some(overlay) = self.page_overlays.get(&slot) else { return };

        let root_box = gtk::Box::new(gtk::Orientation::Vertical, 0);

        let popover = gtk::Popover::new();
        popover.set_parent(overlay);
        popover.set_autohide(false);
        popover.set_has_arrow(false);
        popover.set_position(gtk::PositionType::Top);

        let anchor = gdk::Rectangle::new(
            sel.bounds.0 as i32,
            (sel.bounds.1 - 10.0).max(0.0) as i32,
            sel.bounds.2.max(1.0) as i32,
            sel.bounds.3.max(1.0) as i32,
        );
        popover.set_pointing_to(Some(&anchor));
        popover.add_css_class("k-sel-toolbar-popover");

        let pill = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        pill.add_css_class("k-sel-toolbar");

        // 1. Highlight button -> reveals Calibre drawer
        let highlight_btn = gtk::Button::new();
        let hl_icon = crate::icons::symbolic_with_classes("kalam-highlight-symbolic", 14, &["kalam-inline-icon"]);
        highlight_btn.set_child(Some(&hl_icon));
        highlight_btn.set_tooltip_text(Some("Highlight"));
        highlight_btn.add_css_class("k-sel-action");
        highlight_btn.add_css_class("accent");

        let root_ref = root_box.clone();
        let pill_ref = pill.clone();
        let pop_ref = popover.clone();
        let tx_save = sender.input_sender().clone();
        highlight_btn.connect_clicked(move |_| {
            pill_ref.set_visible(false);
            let tx = tx_save.clone();
            let drawer = crate::pages::reader::engine::build_calibre_drawer_box(
                crate::db::HighlightColor::Yellow,
                crate::db::AnnotationStyle::Solid,
                "",
                None,
                move |color, style, note| {
                    let _ = tx.send(PdfReaderMsg::SaveAnnotation {
                        color: color.as_str().to_string(),
                        style: style.as_str().to_string(),
                        note,
                    });
                },
                None,
            );
            root_ref.append(&drawer);
            pop_ref.present();
        });
        pill.append(&highlight_btn);

        let sep = gtk::Box::new(gtk::Orientation::Vertical, 0);
        sep.add_css_class("k-sel-divider");
        pill.append(&sep);

        // 2. Define button
        let dict_btn = gtk::Button::new();
        let dict_icon = crate::icons::symbolic_with_classes("accessories-dictionary-symbolic", 14, &["kalam-inline-icon"]);
        dict_btn.set_child(Some(&dict_icon));
        dict_btn.set_tooltip_text(Some("Define"));
        dict_btn.add_css_class("k-sel-action");
        let tx_dict = sender.input_sender().clone();
        let word_text = sel.text.clone();
        dict_btn.connect_clicked(move |_| {
            let _ = tx_dict.send(PdfReaderMsg::LookUpWord(word_text.clone()));
        });
        pill.append(&dict_btn);

        root_box.append(&pill);
        popover.set_child(Some(&root_box));
        popover.popup();
        self.selection_chip = Some(popover);
    }

    pub fn dismiss_dict_popover(&mut self) {
        if let Some(popover) = self.dict_popover.take() {
            popover.popdown();
            popover.unparent();
        }
    }

    /// The Define card (decision 31's save toggle, on the PDF page):
    /// the entry's senses under the selection, with the circular
    /// toggle that keeps the word for the Notes Words list.
    pub fn show_dict_popover(&mut self, word: String, sender: &ComponentSender<Self>) {
        let trimmed = word.trim().to_string();
        if trimmed.is_empty() {
            return;
        }
        self.dismiss_dict_popover();
        let selection = self.active_selection.borrow().clone();
        let entry = self.catalog.lookup_entry(&trimmed).unwrap_or_else(|_| crate::db::EntryData {
            word: trimmed.clone(),
            ..Default::default()
        });
        let shown_word = if entry.word.is_empty() {
            trimmed.clone()
        } else {
            entry.word.clone()
        };
        let page = selection
            .as_ref()
            .map(|sel| sel.page)
            .unwrap_or(self.current_page);
        let context = selection.as_ref().map(|sel| sel.text.clone());
        self.dict_lookup_word = Some(shown_word.clone());
        self.dict_lookup_def = entry.senses.first().map(|sense| sense.def.clone());
        self.dict_lookup_page = page;
        self.dict_context = context;
        let _ = self.catalog.log_dict_lookup(
            &shown_word,
            Some(self.book_id),
            Some(page as i64),
            self.dict_context.as_deref(),
            !entry.senses.is_empty(),
        );

        let overlay = selection
            .as_ref()
            .and_then(|sel| self.page_overlays.get(&sel.slot).cloned());
        let (Some(sel), Some(overlay)) = (selection, overlay) else {
            // A lookup with no page under it (a shortcut, a future
            // caller) still answers: the toast the reader used before
            // the card existed.
            let def = self
                .dict_lookup_def
                .clone()
                .unwrap_or_else(|| "No dictionary definition found".to_string());
            crate::notify::info(&shown_word, &def);
            return;
        };

        self.dismiss_selection_chip();
        let saved = self
            .catalog
            .saved_word_exists(&shown_word, self.book_id)
            .unwrap_or(false);

        let popover = gtk::Popover::new();
        popover.set_parent(&overlay);
        popover.set_autohide(true);
        popover.set_has_arrow(false);
        popover.set_position(if sel.bounds.1 > overlay.height() as f64 * 0.62 {
            gtk::PositionType::Top
        } else {
            gtk::PositionType::Bottom
        });
        popover.set_pointing_to(Some(&gdk::Rectangle::new(
            sel.bounds.0 as i32,
            sel.bounds.1 as i32,
            sel.bounds.2.max(1.0) as i32,
            sel.bounds.3.max(1.0) as i32,
        )));
        popover.add_css_class("kalam-reader-dict-popover");

        let popup = gtk::Box::new(gtk::Orientation::Vertical, 0);
        popup.add_css_class("k-popup");
        popup.set_size_request(380.min((overlay.width() - 32).max(300)), -1);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        header.add_css_class("k-header");
        let left = gtk::Box::new(gtk::Orientation::Vertical, 2);
        left.set_hexpand(true);
        left.set_valign(gtk::Align::Center);
        let headword = gtk::Label::new(Some(&shown_word));
        headword.add_css_class("k-word");
        headword.set_xalign(0.0);
        headword.set_wrap(true);
        left.append(&headword);
        if let Some(pronunciation) = crate::db::pronunciation_for(&shown_word) {
            let pron = gtk::Label::new(Some(&format!("/{pronunciation}/")));
            pron.add_css_class("k-pronunciation");
            pron.set_xalign(0.0);
            left.append(&pron);
        }
        header.append(&left);

        let save = gtk::Button::new();
        save.add_css_class("k-save-btn");
        let save_icon = gtk::Image::from_icon_name("list-add-symbolic");
        save.set_child(Some(&save_icon));
        save.set_valign(gtk::Align::Center);
        save.set_sensitive(!entry.senses.is_empty());
        save.update_property(&[gtk::accessible::Property::Label("Save word")]);
        let apply_save = |btn: &gtk::Button, icon: &gtk::Image, is_saved: bool| {
            if is_saved {
                icon.set_icon_name(Some("object-select-symbolic"));
                btn.add_css_class("saved");
                btn.set_tooltip_text(Some("Saved — tap to forget"));
            } else {
                icon.set_icon_name(Some("list-add-symbolic"));
                btn.remove_css_class("saved");
                btn.set_tooltip_text(Some("Save word"));
            }
        };
        apply_save(&save, &save_icon, saved);
        let is_saved = std::rc::Rc::new(std::cell::Cell::new(saved));
        {
            let tx = sender.input_sender().clone();
            let icon = save_icon.clone();
            save.connect_clicked(move |btn| {
                let next = !is_saved.get();
                is_saved.set(next);
                apply_save(btn, &icon, next);
                let _ = tx.send(PdfReaderMsg::ToggleSaveWord);
            });
        }
        header.append(&save);
        popup.append(&header);

        let body = gtk::Box::new(gtk::Orientation::Vertical, 10);
        body.add_css_class("k-body");
        if entry.senses.is_empty() {
            let empty = gtk::Label::new(Some("No dictionary definition found"));
            empty.add_css_class("k-def-text");
            empty.set_xalign(0.0);
            empty.set_wrap(true);
            body.append(&empty);
            if !entry.suggestions.is_empty() {
                let flow = gtk::FlowBox::builder()
                    .selection_mode(gtk::SelectionMode::None)
                    .column_spacing(6)
                    .row_spacing(6)
                    .max_children_per_line(3)
                    .halign(gtk::Align::Start)
                    .build();
                flow.add_css_class("k-chips");
                for suggestion in &entry.suggestions {
                    let chip = gtk::Button::with_label(suggestion);
                    chip.add_css_class("k-chip");
                    chip.add_css_class("k-chip-syn");
                    let tx = sender.input_sender().clone();
                    let next = suggestion.clone();
                    chip.connect_clicked(move |_| {
                        let _ = tx.send(PdfReaderMsg::LookUpWord(next.clone()));
                    });
                    flow.insert(&chip, -1);
                }
                body.append(&flow);
            }
        } else {
            for (index, sense) in entry.senses.iter().enumerate() {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                row.add_css_class("k-def-item");
                let num = gtk::Label::new(Some(&format!("{}.", index + 1)));
                num.add_css_class("k-def-num");
                num.set_valign(gtk::Align::Start);
                row.append(&num);
                let texts = gtk::Box::new(gtk::Orientation::Vertical, 3);
                texts.set_hexpand(true);
                if let Some(pos) = &sense.pos {
                    let pos_label = gtk::Label::new(Some(&pos.to_uppercase()));
                    pos_label.add_css_class("k-pos-name");
                    pos_label.set_xalign(0.0);
                    texts.append(&pos_label);
                }
                let def = gtk::Label::new(Some(&sense.def));
                def.add_css_class("k-def-text");
                def.set_wrap(true);
                def.set_xalign(0.0);
                def.set_halign(gtk::Align::Fill);
                texts.append(&def);
                if let Some(example) = &sense.example {
                    let ex = gtk::Label::new(Some(example));
                    ex.add_css_class("k-def-example");
                    ex.set_wrap(true);
                    ex.set_xalign(0.0);
                    ex.set_halign(gtk::Align::Fill);
                    texts.append(&ex);
                }
                row.append(&texts);
                body.append(&row);
            }
        }
        let scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .propagate_natural_height(true)
            .overlay_scrolling(true)
            .max_content_height(380)
            .build();
        scroll.set_child(Some(&body));
        popup.append(&scroll);
        popover.set_child(Some(&popup));
        let tx = sender.input_sender().clone();
        popover.connect_closed(move |_| {
            let _ = tx.send(PdfReaderMsg::ClearDict);
        });
        popover.popup();
        self.dict_popover = Some(popover);
    }

    fn wrap_page(
        &mut self,
        slot: PageSlot,
        pic: PdfPagePicture,
        sender: &ComponentSender<Self>,
    ) -> gtk::Widget {
        let (target_w, target_h) = if self.scroll_mode == PdfScrollMode::WrappedScrolling {
            (
                (self.base_page_width * 0.6 * self.zoom_level) as i32,
                (self.base_page_height * 0.6 * self.zoom_level) as i32,
            )
        } else {
            (
                (self.base_page_width * self.zoom_level) as i32,
                (self.base_page_height * self.zoom_level) as i32,
            )
        };

        let overlay = gtk::Overlay::new();
        overlay.set_halign(pic.halign());
        overlay.set_valign(pic.valign());
        overlay.set_size_request(target_w, target_h);
        overlay.set_child(Some(&pic));
        overlay.set_cursor_from_name(Some(if self.draw_tool.is_select() {
            "text"
        } else {
            "crosshair"
        }));

        let draw_area = gtk::DrawingArea::new();
        draw_area.set_can_target(false);

        let active_sel_ref = self.active_selection.clone();
        let search_hl_ref = self.search_highlights.clone();
        let drawing_paints_ref = self.drawing_paints.clone();
        let active_drawing_ref = self.active_drawing.clone();
        let draw_accent_rgb = self.draw_accent_rgb;
        let slot_copy = slot;
        let sel_handle_color = self.handle_color();
        draw_area.set_draw_func(move |_, cr, width, height| {
            // Saved drawings and the stroke in flight (decision 66):
            // source-space objects mapped against this allocation,
            // under the search tints and the live selection.
            for paint in drawing_paints_ref.borrow().iter() {
                if paint.slot == slot_copy {
                    drawings::paint_drawing(
                        cr,
                        paint,
                        width as f64,
                        height as f64,
                        draw_accent_rgb,
                    );
                }
            }
            if let Some(stroke) = active_drawing_ref.borrow().as_ref() {
                if stroke.slot == slot_copy {
                    drawings::paint_active_stroke(cr, stroke, width as f64, height as f64);
                }
            }

            // 1. Search matches (decision 61, design-system §9):
            // every scoped match on this page in the quiet tint,
            // the current one in the accent with its outline.
            // Rects are stored in source space and mapped here,
            // against the allocation being painted.
            for highlight in search_hl_ref.borrow().iter() {
                if highlight.slot != slot_copy || highlight.rects.is_empty() {
                    continue;
                }
                let screen_rects: Vec<(f64, f64, f64, f64)> = highlight
                    .rects
                    .iter()
                    .map(|rect| {
                        let (rx, ry, rw, rh) = source_rect_to_display(
                            *rect,
                            highlight.source_width,
                            highlight.source_height,
                            width as f64,
                            height as f64,
                            highlight.rotation,
                        );
                        (rx, ry, rw.max(4.0), rh.max(4.0))
                    })
                    .collect();
                if highlight.current {
                    let [r, g, b] = highlight.current_rgb;
                    cr.set_source_rgba(r as f64, g as f64, b as f64, 0.38);
                    for &(rx, ry, rw, rh) in &screen_rects {
                        cr.rectangle(rx, ry, rw, rh);
                        let _ = cr.fill();
                    }
                    cr.set_source_rgba(r as f64, g as f64, b as f64, 0.9);
                    cr.set_line_width(1.5);
                    for &(rx, ry, rw, rh) in &screen_rects {
                        cr.rectangle(rx, ry, rw, rh);
                        let _ = cr.stroke();
                    }
                } else {
                    let [r, g, b] = highlight.match_rgb;
                    cr.set_source_rgba(
                        r as f64,
                        g as f64,
                        b as f64,
                        highlight.match_alpha as f64,
                    );
                    for &(rx, ry, rw, rh) in &screen_rects {
                        cr.rectangle(rx, ry, rw, rh);
                        let _ = cr.fill();
                    }
                }
            }

            // 2. Draw user text selection
            if let Some(ref sel) = *active_sel_ref.borrow() {
                if sel.slot == slot_copy && !sel.screen_rects.is_empty() {
                    // Kalam selection blue: rgba(53, 132, 228, 0.35)
                    cr.set_source_rgba(53.0 / 255.0, 132.0 / 255.0, 228.0 / 255.0, 0.35);
                    for &(rx, ry, rw, rh) in &sel.screen_rects {
                        cr.rectangle(rx, ry, rw, rh);
                        let _ = cr.fill();
                    }

                    // Handles (start and end) or marquee outline
                    if sel.is_block {
                        // Marquee box outline for rectangular selection
                        cr.set_source_rgba(53.0 / 255.0, 132.0 / 255.0, 228.0 / 255.0, 0.85);
                        cr.set_line_width(1.5);
                        cr.rectangle(sel.bounds.0, sel.bounds.1, sel.bounds.2, sel.bounds.3);
                        let _ = cr.stroke();
                    } else {
                        // Handles (start and end) for continuous reading
                        // selection: the EPUB reader's teardrop grips --
                        // a 2 px bar the height of the band, and a 9 px
                        // teardrop whose point touches the bar's outer
                        // end, hanging away from the text. Same shape,
                        // size and colour in both readers (2.20 step 5).
                        let hc = sel_handle_color;
                        cr.set_source_rgba(hc.0, hc.1, hc.2, 1.0);
                        let (sx, sy, sl) = sel.start_handle;
                        let (ex, ey, el) = sel.end_handle;
                        match sel.rotation {
                            90 => {
                                cr.rectangle(sx, sy - 1.0, sl, 2.0);
                                let _ = cr.fill();
                                draw_teardrop_side(cr, sx + sl - 1.0, sy, true);
                                cr.rectangle(ex, ey - 1.0, el, 2.0);
                                let _ = cr.fill();
                                draw_teardrop_side(cr, ex + 1.0, ey, false);
                            }
                            180 => {
                                cr.rectangle(sx - 1.0, sy, 2.0, sl);
                                let _ = cr.fill();
                                draw_teardrop(cr, sx, sy + sl - 1.0, false);
                                let _ = cr.fill();
                                cr.rectangle(ex - 1.0, ey, 2.0, el);
                                let _ = cr.fill();
                                draw_teardrop(cr, ex, ey + 1.0, true);
                                let _ = cr.fill();
                            }
                            270 => {
                                cr.rectangle(sx, sy - 1.0, sl, 2.0);
                                let _ = cr.fill();
                                draw_teardrop_side(cr, sx + 1.0, sy, false);
                                cr.rectangle(ex, ey - 1.0, el, 2.0);
                                let _ = cr.fill();
                                draw_teardrop_side(cr, ex + el - 1.0, ey, true);
                            }
                            _ => {
                                cr.rectangle(sx - 1.0, sy, 2.0, sl);
                                let _ = cr.fill();
                                draw_teardrop(cr, sx, sy + 1.0, true);
                                let _ = cr.fill();
                                cr.rectangle(ex - 1.0, ey, 2.0, el);
                                let _ = cr.fill();
                                draw_teardrop(cr, ex, ey + el - 1.0, false);
                                let _ = cr.fill();
                            }
                        }
                    }
                }
            }
        });

        overlay.add_overlay(&draw_area);

        // Click controller for word (double click) and line (triple
        // click). Added BEFORE the drag gesture on purpose: controllers
        // run in addition order, so the click has counted the press by the
        // time the drag gesture decides whether to stand down for it --
        // the same arrangement the EPUB reader's view uses.
        let click = gtk::GestureClick::new();
        click.set_button(1);
        let tx_click = sender.input_sender().clone();
        let click_tool = self.draw_tool_shared.clone();
        let multi_click = std::rc::Rc::new(std::cell::Cell::new(0i32));
        let mc_for_click = multi_click.clone();
        click.connect_pressed(move |_, n_press, x, y| {
            mc_for_click.set(n_press);
            // A drawing tool owns the press: no word or line
            // selection starts under a pen.
            if !click_tool.get().is_select() {
                return;
            }
            if n_press == 2 {
                let _ = tx_click.send(PdfReaderMsg::SelectionWordAt { slot: slot_copy, x, y });
            } else if n_press >= 3 {
                let _ = tx_click.send(PdfReaderMsg::SelectionLineAt { slot: slot_copy, x, y });
            }
        });
        overlay.add_controller(click);

        // The open hand over the selection's handle grips, like the EPUB
        // reader; the closed hand while a grip is actually dragged is set
        // by the SelectionDragBegin / SelectionDragEnd handlers.
        let motion = gtk::EventControllerMotion::new();
        let cursor_sel = self.active_selection.clone();
        let cursor_tool = self.draw_tool_shared.clone();
        let leave_tool = self.draw_tool_shared.clone();
        let cursor_overlay = overlay.clone();
        let cursor_dragging = self.handle_dragging.clone();
        motion.connect_motion(move |_, x, y| {
            // A grip is being dragged: the closed hand stays until the
            // release, like the EPUB reader.
            if cursor_dragging.get() {
                return;
            }
            if !cursor_tool.get().is_select() {
                cursor_overlay.set_cursor_from_name(Some("crosshair"));
                return;
            }
            let over_handle = cursor_sel
                .borrow()
                .as_ref()
                .filter(|sel| sel.slot == slot_copy && !sel.is_block)
                .is_some_and(|sel| over_selection_handle(sel, x, y));
            // Over a grip: the open hand. Anywhere else on the page:
            // the page's own cursor, the text I-beam `wrap_page` sets --
            // never the plain arrow, which would override it.
            cursor_overlay.set_cursor_from_name(if over_handle {
                Some("grab")
            } else {
                Some("text")
            });
        });
        let leave_overlay = overlay.clone();
        motion.connect_leave(move |_| {
            leave_overlay.set_cursor_from_name(Some(if leave_tool.get().is_select() {
                "text"
            } else {
                "crosshair"
            }));
        });
        overlay.add_controller(motion);

        // Drag controller for range and block selection (Alt+Drag for block marquee)
        let drag = gtk::GestureDrag::new();
        drag.set_button(1);
        let tx_drag_begin = sender.input_sender().clone();
        let tx_drag_update = sender.input_sender().clone();
        let tx_drag_end = sender.input_sender().clone();
        let drag_tool_begin = self.draw_tool_shared.clone();
        let drag_tool_update = self.draw_tool_shared.clone();
        let drag_tool_end = self.draw_tool_shared.clone();

        let drag_ctrl_begin = drag.clone();
        let mc_for_drag = multi_click.clone();
        drag.connect_drag_begin(move |_, x, y| {
            // The second or third press of a multi-click is not a drag:
            // stand down (no SelectionDragBegin, no drag state) so the
            // word that click selects survives its own release -- the
            // drag-end handler would otherwise treat that release as a
            // tap that clears the selection. The EPUB reader coordinates
            // its gestures the same way.
            if mc_for_drag.get() >= 2 {
                return;
            }
            if !drag_tool_begin.get().is_select() {
                let _ = tx_drag_begin.send(PdfReaderMsg::DrawingDragBegin { slot: slot_copy, x, y });
                return;
            }
            let is_block = drag_ctrl_begin.current_event_state().contains(gdk::ModifierType::ALT_MASK);
            let _ = tx_drag_begin.send(PdfReaderMsg::SelectionDragBegin { slot: slot_copy, x, y, is_block });
        });
        let drag_ctrl_up = drag.clone();
        let mc_for_update = multi_click.clone();
        drag.connect_drag_update(move |_, dx, dy| {
            // The movement of a multi-click's second or third press is
            // not a selection drag either -- even if a drag state was
            // left behind by an earlier press whose own release never
            // arrived (the EPUB reader gates update the same way).
            if mc_for_update.get() >= 2 {
                return;
            }
            if !drag_tool_update.get().is_select() {
                let _ = tx_drag_update.send(PdfReaderMsg::DrawingDragUpdate { slot: slot_copy, dx, dy });
                return;
            }
            let is_block = drag_ctrl_up.current_event_state().contains(gdk::ModifierType::ALT_MASK);
            let _ = tx_drag_update.send(PdfReaderMsg::SelectionDragUpdate { slot: slot_copy, dx, dy, is_block });
        });
        let mc_for_end = multi_click.clone();
        drag.connect_drag_end(move |_, dx, dy| {
            // The live press count at release time, then reset: a
            // multi-click's release belongs to the click gesture, never
            // the drag (the EPUB reader gates its drag the same way).
            let press_count = mc_for_end.replace(0);
            if !drag_tool_end.get().is_select() {
                let _ = tx_drag_end.send(PdfReaderMsg::DrawingDragEnd { slot: slot_copy, dx, dy, press_count });
                return;
            }
            let _ = tx_drag_end.send(PdfReaderMsg::SelectionDragEnd { slot: slot_copy, dx, dy, press_count });
        });
        overlay.add_controller(drag);

        self.page_overlays.insert(slot, overlay.clone());
        self.page_draw_areas.insert(slot, draw_area);

        overlay.upcast()
    }

    /// Build the viewport container widget for current scroll mode & spread mode.
    pub fn build_viewport_widget(&mut self, sender: &ComponentSender<Self>) -> gtk::Widget {
        // The recognizing signal is parented to a page overlay that is
        // about to be dropped with the rest of the viewport.
        self.dismiss_recognizing_popover();
        self.dismiss_drawing_popover();
        self.dismiss_dict_popover();
        self.page_pictures.clear();
        self.page_overlays.clear();
        self.page_draw_areas.clear();
        self.paged_picture = None;
        self.paged_loading_box = None;
        self.paged_label = None;
        self.two_page_left_pic = None;
        self.two_page_right_pic = None;
        self.two_page_loading_box = None;
        self.two_page_label = None;

        match self.scroll_mode {
            PdfScrollMode::PageScrolling => {
                match self.spread_mode {
                    PdfSpreadMode::NoSpreads => {
                        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
                        container.set_halign(gtk::Align::Fill);
                        container.set_valign(gtk::Align::Start);
                        container.set_hexpand(true);
                        container.set_vexpand(true);
                        container.set_margin_top(28);
                        container.set_margin_bottom(72);
                        container.set_margin_start(16);
                        container.set_margin_end(16);

                        let page_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                        page_row.set_hexpand(true);
                        page_row.set_valign(gtk::Align::Start);

                        let spacer_left = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                        spacer_left.set_hexpand(true);
                        page_row.append(&spacer_left);

                        let pic = self.make_page_picture();
                        pic.set_halign(gtk::Align::Center);
                        pic.set_valign(gtk::Align::Start);
                        pic.add_css_class("kalam-pdf-page-image");

                        let loading_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
                        loading_box.set_halign(gtk::Align::Center);
                        loading_box.set_valign(gtk::Align::Center);

                        let spinner = gtk::Spinner::new();
                        spinner.start();
                        spinner.set_size_request(32, 32);
                        loading_box.append(&spinner);

                        let label = gtk::Label::new(Some(&format!(
                            "Rendering page {}...",
                            self.current_page
                        )));
                        label.add_css_class("dim-label");
                        loading_box.append(&label);

                        let page_w = (self.base_page_width * self.zoom_level) as i32;
                        let page_h = (self.base_page_height * self.zoom_level) as i32;
                        pic.set_size_request(page_w, page_h);

                        if let Some(cached) = self.textures.get(&self.current_page) {
                            pic.set_texture(Some(&cached.texture));
                            pic.set_visible(true);
                            loading_box.set_visible(false);
                        } else {
                            pic.set_visible(false);
                            loading_box.set_visible(true);
                        }

                        self.paged_picture = Some(pic.clone());
                        self.page_pictures.insert(self.current_page, pic.clone());
                        let page_widget = self.wrap_page(PageSlot::PagedSingle, pic, sender);

                        page_row.append(&page_widget);

                        let spacer_right = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                        spacer_right.set_hexpand(true);
                        page_row.append(&spacer_right);

                        container.append(&page_row);
                        container.append(&loading_box);

                        self.paged_loading_box = Some(loading_box);
                        self.paged_label = Some(label);

                        container.upcast()
                    }
                    _ => {
                        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
                        container.set_halign(gtk::Align::Fill);
                        container.set_valign(gtk::Align::Start);
                        container.set_hexpand(true);
                        container.set_vexpand(true);
                        container.set_margin_top(28);
                        container.set_margin_bottom(72);
                        container.set_margin_start(16);
                        container.set_margin_end(16);

                        let spread_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                        spread_box.set_hexpand(true);
                        spread_box.set_valign(gtk::Align::Start);

                        let spacer_left = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                        spacer_left.set_hexpand(true);
                        spread_box.append(&spacer_left);

                        let spread_inner = gtk::Box::new(gtk::Orientation::Horizontal, self.two_page_gap);
                        spread_inner.set_hexpand(false);
                        spread_inner.set_halign(gtk::Align::Center);
                        spread_inner.set_valign(gtk::Align::Start);

                        let (left, right) = self.spread_for_page(self.current_page);

                        let page_w = (self.base_page_width * self.zoom_level) as i32;
                        let page_h = (self.base_page_height * self.zoom_level) as i32;

                        let left_pic = self.make_page_picture();
                        left_pic.set_halign(if right.is_some() { gtk::Align::End } else { gtk::Align::Center });
                        left_pic.set_valign(gtk::Align::Start);
                        left_pic.set_size_request(page_w, page_h);
                        left_pic.add_css_class("kalam-pdf-page-image");
                        left_pic.add_css_class("kalam-pdf-two-page");

                        let right_pic = self.make_page_picture();
                        right_pic.set_halign(gtk::Align::Start);
                        right_pic.set_valign(gtk::Align::Start);
                        right_pic.set_size_request(page_w, page_h);
                        right_pic.add_css_class("kalam-pdf-page-image");
                        right_pic.add_css_class("kalam-pdf-two-page");

                        let loading_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
                        loading_box.set_halign(gtk::Align::Center);
                        loading_box.set_valign(gtk::Align::Center);
                        let spinner = gtk::Spinner::new();
                        spinner.start();
                        spinner.set_size_request(32, 32);
                        let label = gtk::Label::new(Some(&format!(
                            "Rendering page {}...",
                            if let Some(r) = right {
                                format!("{} - {}", left, r)
                            } else {
                                format!("{}", left)
                            }
                        )));
                        label.add_css_class("dim-label");
                        loading_box.append(&spinner);
                        loading_box.append(&label);

                        let left_loaded = self.textures.get(&left);
                        let right_loaded = right.and_then(|r| self.textures.get(&r));

                        let all_needed_loaded = if right.is_some() {
                            left_loaded.is_some() && right_loaded.is_some()
                        } else {
                            left_loaded.is_some()
                        };

                        if all_needed_loaded {
                            if let Some(cached) = left_loaded {
                                left_pic.set_texture(Some(&cached.texture));
                                left_pic.set_visible(true);
                            }
                            if right.is_some() {
                                if let Some(cached) = right_loaded {
                                    right_pic.set_texture(Some(&cached.texture));
                                    right_pic.set_visible(true);
                                }
                            } else {
                                right_pic.set_visible(false);
                            }
                            loading_box.set_visible(false);
                        } else {
                            left_pic.set_visible(false);
                            right_pic.set_visible(false);
                            loading_box.set_visible(true);
                        }

                        self.two_page_left_pic = Some(left_pic.clone());
                        self.two_page_right_pic = Some(right_pic.clone());
                        self.page_pictures.insert(left, left_pic.clone());
                        if let Some(r) = right {
                            self.page_pictures.insert(r, right_pic.clone());
                        }

                        let left_widget = self.wrap_page(PageSlot::PagedSpreadLeft, left_pic, sender);
                        let right_widget = self.wrap_page(PageSlot::PagedSpreadRight, right_pic, sender);

                        spread_inner.append(&left_widget);
                        spread_inner.append(&right_widget);
                        spread_box.append(&spread_inner);

                        let spacer_right = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                        spacer_right.set_hexpand(true);
                        spread_box.append(&spacer_right);

                        container.append(&spread_box);
                        container.append(&loading_box);

                        self.two_page_loading_box = Some(loading_box);
                        self.two_page_label = Some(label);

                        container.upcast()
                    }
                }
            }
            PdfScrollMode::VerticalScrolling => {
                let container = gtk::Box::new(gtk::Orientation::Vertical, self.scroll_gap);
                container.set_halign(gtk::Align::Fill);
                container.set_valign(gtk::Align::Start);
                container.set_hexpand(true);
                container.set_vexpand(false);
                container.set_margin_top(36);
                container.set_margin_bottom(88);
                container.set_margin_start(16);
                container.set_margin_end(16);

                let page_w = (self.base_page_width * self.zoom_level) as i32;
                let page_h = (self.base_page_height * self.zoom_level) as i32;

                match self.spread_mode {
                    PdfSpreadMode::NoSpreads => {
                        for p in 1..=self.total_pages {
                            let page_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                            page_row.set_hexpand(true);
                            page_row.set_valign(gtk::Align::Start);

                            let spacer_left = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                            spacer_left.set_hexpand(true);
                            page_row.append(&spacer_left);

                            let pic = self.make_page_picture();
                            pic.set_halign(gtk::Align::Center);
                            pic.set_valign(gtk::Align::Start);
                            pic.set_size_request(page_w, page_h);

                            if let Some(cached) = self.textures.get(&p) {
                                pic.set_texture(Some(&cached.texture));
                                pic.add_css_class("kalam-pdf-page-image");
                            } else {
                                pic.add_css_class("kalam-pdf-placeholder");
                            }

                            self.page_pictures.insert(p, pic.clone());
                            let page_widget = self.wrap_page(PageSlot::Fixed(p), pic, sender);

                            page_row.append(&page_widget);

                            let spacer_right = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                            spacer_right.set_hexpand(true);
                            page_row.append(&spacer_right);

                            container.append(&page_row);
                        }
                    }
                    _ => {
                        for (left, right) in self.all_spreads() {
                            let spread_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                            spread_row.set_hexpand(true);
                            spread_row.set_valign(gtk::Align::Start);
                            spread_row.add_css_class("kalam-pdf-spread-row");

                            let spacer_left = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                            spacer_left.set_hexpand(true);
                            spread_row.append(&spacer_left);

                            let spread_inner = gtk::Box::new(gtk::Orientation::Horizontal, self.two_page_gap);
                            spread_inner.set_hexpand(false);
                            spread_inner.set_halign(gtk::Align::Center);
                            spread_inner.set_valign(gtk::Align::Start);

                            let left_pic = self.make_page_picture();
                            left_pic.set_halign(if right.is_some() { gtk::Align::End } else { gtk::Align::Center });
                            left_pic.set_valign(gtk::Align::Start);
                            left_pic.set_size_request(page_w, page_h);
                            left_pic.add_css_class("kalam-pdf-two-page");

                            let spread_ready = if let Some(r) = right {
                                self.textures.contains_key(&left) && self.textures.contains_key(&r)
                            } else {
                                self.textures.contains_key(&left)
                            };

                            if spread_ready {
                                if let Some(cached) = self.textures.get(&left) {
                                    left_pic.set_texture(Some(&cached.texture));
                                    left_pic.add_css_class("kalam-pdf-page-image");
                                }
                            } else {
                                left_pic.add_css_class("kalam-pdf-placeholder");
                            }

                            self.page_pictures.insert(left, left_pic.clone());
                            let left_widget = self.wrap_page(PageSlot::Fixed(left), left_pic, sender);
                            spread_inner.append(&left_widget);

                            if let Some(r) = right {
                                let right_pic = self.make_page_picture();
                                right_pic.set_halign(gtk::Align::Start);
                                right_pic.set_valign(gtk::Align::Start);
                                right_pic.set_size_request(page_w, page_h);
                                right_pic.add_css_class("kalam-pdf-two-page");

                                if spread_ready {
                                    if let Some(cached) = self.textures.get(&r) {
                                        right_pic.set_texture(Some(&cached.texture));
                                        right_pic.add_css_class("kalam-pdf-page-image");
                                    }
                                } else {
                                    right_pic.add_css_class("kalam-pdf-placeholder");
                                }

                                self.page_pictures.insert(r, right_pic.clone());
                                let right_widget = self.wrap_page(PageSlot::Fixed(r), right_pic, sender);
                                spread_inner.append(&right_widget);
                            }

                            spread_row.append(&spread_inner);

                            let spacer_right = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                            spacer_right.set_hexpand(true);
                            spread_row.append(&spacer_right);

                            container.append(&spread_row);
                        }
                    }
                }

                container.upcast()
            }
            PdfScrollMode::HorizontalScrolling => {
                let container = gtk::Box::new(gtk::Orientation::Horizontal, self.scroll_gap);
                container.set_valign(gtk::Align::Start);
                container.set_halign(gtk::Align::Start);
                container.set_vexpand(true);
                container.set_hexpand(false);
                container.set_margin_top(28);
                container.set_margin_bottom(64);
                container.set_margin_start(24);
                container.set_margin_end(64);

                let page_w = (self.base_page_width * self.zoom_level) as i32;
                let page_h = (self.base_page_height * self.zoom_level) as i32;

                match self.spread_mode {
                    PdfSpreadMode::NoSpreads => {
                        for p in 1..=self.total_pages {
                            let pic = self.make_page_picture();
                            pic.set_valign(gtk::Align::Start);
                            pic.set_size_request(page_w, page_h);

                            if let Some(cached) = self.textures.get(&p) {
                                pic.set_texture(Some(&cached.texture));
                                pic.add_css_class("kalam-pdf-page-image");
                            } else {
                                pic.add_css_class("kalam-pdf-placeholder");
                            }

                            self.page_pictures.insert(p, pic.clone());
                            let page_widget = self.wrap_page(PageSlot::Fixed(p), pic, sender);
                            container.append(&page_widget);
                        }
                    }
                    _ => {
                        for (left, right) in self.all_spreads() {
                            let spread_box = gtk::Box::new(gtk::Orientation::Horizontal, self.two_page_gap);
                            spread_box.set_valign(gtk::Align::Start);
                            spread_box.add_css_class("kalam-pdf-spread-row");

                            let left_pic = self.make_page_picture();
                            left_pic.set_valign(gtk::Align::Start);
                            left_pic.set_size_request(page_w, page_h);
                            left_pic.add_css_class("kalam-pdf-two-page");

                            let spread_ready = if let Some(r) = right {
                                self.textures.contains_key(&left) && self.textures.contains_key(&r)
                            } else {
                                self.textures.contains_key(&left)
                            };

                            if spread_ready {
                                if let Some(cached) = self.textures.get(&left) {
                                    left_pic.set_texture(Some(&cached.texture));
                                    left_pic.add_css_class("kalam-pdf-page-image");
                                }
                            } else {
                                left_pic.add_css_class("kalam-pdf-placeholder");
                            }

                            self.page_pictures.insert(left, left_pic.clone());
                            let left_widget = self.wrap_page(PageSlot::Fixed(left), left_pic, sender);
                            spread_box.append(&left_widget);

                            if let Some(r) = right {
                                let right_pic = self.make_page_picture();
                                right_pic.set_valign(gtk::Align::Start);
                                right_pic.set_size_request(page_w, page_h);
                                right_pic.add_css_class("kalam-pdf-two-page");

                                if spread_ready {
                                    if let Some(cached) = self.textures.get(&r) {
                                        right_pic.set_texture(Some(&cached.texture));
                                        right_pic.add_css_class("kalam-pdf-page-image");
                                    }
                                } else {
                                    right_pic.add_css_class("kalam-pdf-placeholder");
                                }

                                self.page_pictures.insert(r, right_pic.clone());
                                let right_widget = self.wrap_page(PageSlot::Fixed(r), right_pic, sender);
                                spread_box.append(&right_widget);
                            }

                            container.append(&spread_box);
                        }
                    }
                }

                container.upcast()
            }
            PdfScrollMode::WrappedScrolling => {
                let flow_box = gtk::FlowBox::new();
                flow_box.set_valign(gtk::Align::Start);
                flow_box.set_halign(gtk::Align::Center);
                flow_box.set_hexpand(true);
                flow_box.set_vexpand(true);
                flow_box.set_margin_top(36);
                flow_box.set_margin_bottom(88);
                flow_box.set_margin_start(16);
                flow_box.set_margin_end(16);
                flow_box.set_column_spacing(self.scroll_gap.max(0) as u32);
                flow_box.set_row_spacing(self.scroll_gap.max(0) as u32);
                flow_box.set_selection_mode(gtk::SelectionMode::None);

                let thumb_w = (self.base_page_width * 0.6 * self.zoom_level) as i32;
                let thumb_h = (self.base_page_height * 0.6 * self.zoom_level) as i32;

                for p in 1..=self.total_pages {
                    let pic = self.make_page_picture();
                    pic.set_valign(gtk::Align::Start);
                    pic.set_size_request(thumb_w, thumb_h);

                    if let Some(cached) = self.textures.get(&p) {
                        pic.set_texture(Some(&cached.texture));
                        pic.add_css_class("kalam-pdf-page-image");
                    } else {
                        pic.add_css_class("kalam-pdf-placeholder");
                    }

                    self.page_pictures.insert(p, pic.clone());
                    let page_widget = self.wrap_page(PageSlot::Fixed(p), pic, sender);
                    flow_box.append(&page_widget);
                }

                flow_box.upcast()
            }
        }
    }

    pub fn update_paged_view(&self) {
        let (Some(ref pic), Some(ref loading_box), Some(ref label)) =
            (&self.paged_picture, &self.paged_loading_box, &self.paged_label)
        else {
            return;
        };
        self.refresh_drawing_paints();
        self.queue_all_drawing_areas();

        let page_w = (self.base_page_width * self.zoom_level) as i32;
        let page_h = (self.base_page_height * self.zoom_level) as i32;
        pic.set_size_request(page_w, page_h);

        if let Some(cached) = self.textures.get(&self.current_page) {
            pic.set_texture(Some(&cached.texture));
            pic.set_visible(true);
            loading_box.set_visible(false);
            pic.queue_resize();
            pic.queue_draw();
        } else {
            pic.set_visible(false);
            loading_box.set_visible(true);
            label.set_label(&format!("Rendering page {}...", self.current_page));
        }
    }

    pub fn update_two_page_view(&self) {
        let (Some(ref left_pic), Some(ref right_pic), Some(ref loading_box), Some(ref label)) = (
            &self.two_page_left_pic,
            &self.two_page_right_pic,
            &self.two_page_loading_box,
            &self.two_page_label,
        ) else {
            return;
        };
        self.refresh_drawing_paints();
        self.queue_all_drawing_areas();

        let page_w = (self.base_page_width * self.zoom_level) as i32;
        let page_h = (self.base_page_height * self.zoom_level) as i32;
        left_pic.set_size_request(page_w, page_h);
        right_pic.set_size_request(page_w, page_h);

        let (left, right) = self.spread_for_page(self.current_page);
        let left_loaded = self.textures.get(&left);
        let right_loaded = right.and_then(|r| self.textures.get(&r));

        let all_needed_loaded = if right.is_some() {
            left_loaded.is_some() && right_loaded.is_some()
        } else {
            left_loaded.is_some()
        };

        if all_needed_loaded {
            if let Some(cached) = left_loaded {
                left_pic.set_texture(Some(&cached.texture));
                left_pic.set_halign(if right.is_some() { gtk::Align::End } else { gtk::Align::Center });
                left_pic.set_valign(gtk::Align::Start);
                left_pic.set_visible(true);
            }
            if right.is_some() {
                if let Some(cached) = right_loaded {
                    right_pic.set_texture(Some(&cached.texture));
                    right_pic.set_halign(gtk::Align::Start);
                    right_pic.set_valign(gtk::Align::Start);
                    right_pic.set_visible(true);
                }
            } else {
                right_pic.set_visible(false);
            }
            loading_box.set_visible(false);
            left_pic.queue_resize();
            left_pic.queue_draw();
            right_pic.queue_resize();
            right_pic.queue_draw();
        } else {
            left_pic.set_visible(false);
            right_pic.set_visible(false);
            loading_box.set_visible(true);
            if let Some(r) = right {
                label.set_label(&format!("Rendering pages {}-{}...", left, r));
            } else {
                label.set_label(&format!("Rendering page {}...", left));
            }
        }
    }

    /// Smooth in-place zoom adjustment without widget recreation or crashes.
    pub fn apply_zoom_change(
        &mut self,
        old_zoom: f64,
        sender: &ComponentSender<Self>,
        scroll: &gtk::ScrolledWindow,
    ) {
        self.clear_selection();
        self.trigger_zoom_osd(sender);
        self.update_scroll_policies(scroll);
        self.render_generation = self.render_generation.wrapping_add(1);
        if let Some(ref gen) = self.active_generation {
            gen.store(self.render_generation, Ordering::Relaxed);
        }

        self.pending_loads.clear();

        let page_w = (self.base_page_width * self.zoom_level) as i32;
        let page_h = (self.base_page_height * self.zoom_level) as i32;

        match self.scroll_mode {
            PdfScrollMode::PageScrolling => {
                if let Some(ref pic) = self.paged_picture {
                    pic.set_size_request(page_w, page_h);
                }
                if let Some(ref left_pic) = self.two_page_left_pic {
                    left_pic.set_size_request(page_w, page_h);
                }
                if let Some(ref right_pic) = self.two_page_right_pic {
                    right_pic.set_size_request(page_w, page_h);
                }
                for overlay in self.page_overlays.values() {
                    overlay.set_size_request(page_w, page_h);
                    overlay.queue_resize();
                    overlay.queue_draw();
                }
                match self.spread_mode {
                    PdfSpreadMode::NoSpreads => self.update_paged_view(),
                    _ => self.update_two_page_view(),
                }
            }
            _ => {
                let (target_w, target_h) = if self.scroll_mode == PdfScrollMode::WrappedScrolling {
                    (
                        (self.base_page_width * 0.6 * self.zoom_level) as i32,
                        (self.base_page_height * 0.6 * self.zoom_level) as i32,
                    )
                } else {
                    (page_w, page_h)
                };

                for pic in self.page_pictures.values() {
                    pic.set_size_request(target_w, target_h);
                    pic.queue_resize();
                    pic.queue_draw();
                }

                for overlay in self.page_overlays.values() {
                    overlay.set_size_request(target_w, target_h);
                    overlay.queue_resize();
                    overlay.queue_draw();
                }

                if old_zoom > 0.05 {
                    if self.scroll_mode == PdfScrollMode::HorizontalScrolling {
                        let hadj = scroll.hadjustment();
                        let page_size = hadj.page_size();
                        let center = hadj.value() + page_size / 2.0;
                        let new_center = center * (self.zoom_level / old_zoom);
                        let target_val = (new_center - page_size / 2.0)
                            .clamp(hadj.lower(), (hadj.upper() - page_size).max(0.0));
                        hadj.set_value(target_val);
                    } else {
                        let vadj = scroll.vadjustment();
                        let page_size = vadj.page_size();
                        let center = vadj.value() + page_size / 2.0;
                        let new_center = center * (self.zoom_level / old_zoom);
                        let target_val = (new_center - page_size / 2.0)
                            .clamp(vadj.lower(), (vadj.upper() - page_size).max(0.0));
                        vadj.set_value(target_val);
                    }
                }

                scroll.queue_draw();
            }
        }

        self.trigger_loads(sender);
    }

    /// Scroll to current page ratio in continuous flow modes.
    pub fn scroll_to_current_page(&self, scroll: &gtk::ScrolledWindow) {
        if self.scroll_mode == PdfScrollMode::PageScrolling || self.total_pages <= 1 {
            return;
        }

        let effective_page = match self.spread_mode {
            PdfSpreadMode::NoSpreads => self.current_page,
            _ => self.spread_for_page(self.current_page).0,
        };

        if self.scroll_mode == PdfScrollMode::HorizontalScrolling {
            let hadj = scroll.hadjustment();
            let max = (hadj.upper() - hadj.page_size()).max(0.0);
            if max > 0.0 {
                let ratio = (effective_page.saturating_sub(1)) as f64 / (self.total_pages - 1) as f64;
                hadj.set_value((ratio * max).clamp(hadj.lower(), max));
            }
        } else {
            let vadj = scroll.vadjustment();
            let max = (vadj.upper() - vadj.page_size()).max(0.0);
            if max > 0.0 {
                let ratio = (effective_page.saturating_sub(1)) as f64 / (self.total_pages - 1) as f64;
                vadj.set_value((ratio * max).clamp(vadj.lower(), max));
            }
        }
    }

    /// Synchronize the Reading pane with active state. Sliders are
    /// only moved when they disagree, so a drag is never fought by
    /// the sync its own messages trigger; the handlers they fire
    /// into ignore a value they already hold.
    pub fn sync_settings_ui(&self, sw: &PdfSettingsWidgets) {
        toggle_active(
            &sw.layout_single_btn,
            pdf_layout_for_spread_mode(self.spread_mode) == PdfLayout::Single,
        );
        toggle_active(
            &sw.layout_spread_btn,
            pdf_layout_for_spread_mode(self.spread_mode) == PdfLayout::Spread,
        );
        sw.flow_control.set_selected(match pdf_flow_for_scroll_mode(self.scroll_mode) {
            PdfFlow::Paged => 0,
            PdfFlow::Scroll => 1,
        });

        // The gap is a Scroll choice (decision 53); in Paged the row
        // stays visible but inert, exactly as the board draws it.
        sw.scroll_gap_row
            .set_sensitive(self.scroll_mode != PdfScrollMode::PageScrolling);
        if (sw.scroll_gap_scale.value() - self.scroll_gap as f64).abs() > 0.01 {
            sw.scroll_gap_scale.set_value(self.scroll_gap as f64);
        }
        sw.scroll_gap_value.set_label(&self.scroll_gap.to_string());

        if sw.cover_switch.is_active() != self.cover_on_its_own {
            sw.cover_switch.set_active(self.cover_on_its_own);
        }

        toggle_active(&sw.fit_width_btn, self.fit_mode == PdfFitMode::Width);
        toggle_active(&sw.fit_height_btn, self.fit_mode == PdfFitMode::Height);
        toggle_active(&sw.fit_screen_btn, self.fit_mode == PdfFitMode::Screen);
        toggle_active(&sw.fit_original_btn, self.fit_mode == PdfFitMode::Original);

        if (sw.zoom_scale.value() - self.zoom_percent as f64).abs() > 0.01 {
            sw.zoom_scale.set_value(self.zoom_percent as f64);
        }
        sw.zoom_value.set_label(&format!("{}%", self.zoom_percent));

        for (button, verb) in [
            (&sw.rotate_left_btn, "Rotate the view left"),
            (&sw.rotate_right_btn, "Rotate the view right"),
        ] {
            let label = format!("{verb} (currently {}°)", self.rotation_degrees);
            button.set_tooltip_text(Some(&label));
            button.update_property(&[gtk::accessible::Property::Label(&label)]);
        }
    }
}

#[relm4::component(pub)]
impl Component for PdfReaderModel {
    type Init = PdfReaderInit;
    type Input = PdfReaderMsg;
    type Output = PdfReaderOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Overlay {
            add_css_class: "kalam-reader-overlay",
            add_css_class: "kalam-pdf-overlay",
            set_hexpand: true,
            set_vexpand: true,

            // ── 1. Main Viewport Scroll ──────────────────────────────
            // It stands down under a whole-book open state: the
            // provisional page skeleton's natural height would
            // otherwise make the root taller than the window and
            // centre that state's card below the fold.
            #[name = "viewport_scroll"]
            gtk::ScrolledWindow {
                #[watch]
                set_visible: !model.open_state.is_blocking(),
                set_hexpand: true,
                set_vexpand: true,
                set_hscrollbar_policy: gtk::PolicyType::Never,
                set_vscrollbar_policy: gtk::PolicyType::Always,
                add_css_class: "kalam-pdf-viewport",
            },

            // ── 2. The click that puts a floating panel away ───────
            // No dim anywhere (decision 52): this transparent layer
            // only exists while an unpinned panel stands, catches
            // the page click that dismisses it, and is spent doing
            // so. The panels ride above it and stay interactive.
            #[name = "panel_click_catcher"]
            add_overlay = &gtk::Box {
                add_css_class: "kalam-pdf-click-catcher",
                #[watch]
                set_visible: (model.left_panel_open && !model.left_panel_pinned)
                    || (model.right_panel_open && !model.right_panel_pinned),
                set_hexpand: true,
                set_vexpand: true,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Fill,
            },

            // ── 3. The 15px edge strips that summon the panels ─────
            #[name = "left_edge_box"]
            add_overlay = &gtk::Box {
                add_css_class: "kalam-reader-hover-edge",
                add_css_class: "kalam-reader-hover-edge-left",
                set_width_request: 15,
                set_hexpand: false,
                set_vexpand: true,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Fill,
            },

            #[name = "right_edge_box"]
            add_overlay = &gtk::Box {
                add_css_class: "kalam-reader-hover-edge",
                add_css_class: "kalam-reader-hover-edge-right",
                set_width_request: 15,
                set_hexpand: false,
                set_vexpand: true,
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Fill,
            },

            // ── 4. The Back pill with its Bubble slide-out ──────────
            // Hidden until the pointer rests in the top-left zone
            // (decision 15, inherited by 52). The shared slide-out
            // unit is built in init and appended into `back_host`.
            #[name = "back_revealer"]
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_back_button
                    && !model.left_panel_open
                    && !model.open_state.is_blocking(),
                set_transition_type: gtk::RevealerTransitionType::SlideDown,
                set_transition_duration: 220,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Start,

                #[name = "back_host"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_margin_start: 18,
                    set_margin_top: 14,
                },
            },

            // ── 5. The bookmark ribbon ──────────────────────────────
            // Always there, hanging from the top edge 48 from the
            // right (decision 16); not drawn under a pinned right
            // panel (decision 57).
            #[name = "ribbon_button"]
            add_overlay = &gtk::Button {
                add_css_class: "kalam-reader-ribbon",
                #[watch]
                set_visible: model.doc_ready
                    && !(model.right_panel_open && model.right_panel_pinned),
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Start,
                set_margin_end: 48,
                connect_clicked => PdfReaderMsg::ToggleBookmark,

                #[name = "ribbon_image"]
                gtk::Image {
                    set_icon_name: Some("kalam-ribbon-outline-symbolic"),
                    set_pixel_size: 26,
                },
            },

            // ── 6. The location text ────────────────────────────────
            // Plain `12–13 / 612`, bottom-right, no pill under it
            // (decision 52). Clicking it opens the jump field.
            #[name = "location_label"]
            add_overlay = &gtk::Label {
                add_css_class: "kalam-reader-location",
                #[watch]
                set_visible: model.doc_ready
                    && !(model.right_panel_open && model.right_panel_pinned),
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::End,
                set_margin_end: 24,
                set_margin_bottom: 16,
                #[watch]
                set_label: &model.location_label(),
            },

            // ── 7. The page-jump field ──────────────────────────────
            // A 52-high pill at the top centre (decisions 50, 53):
            // label, underlined number, `of N` hint (accent range
            // out of bounds), Page/Chapter switch, close.
            #[name = "jump_revealer"]
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.jump_open,
                set_transition_type: gtk::RevealerTransitionType::SlideDown,
                set_transition_duration: 180,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Start,
                set_margin_top: 16,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    add_css_class: "kalam-pdf-jump",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 10,
                    set_size_request: (480, -1),

                    gtk::Label {
                        add_css_class: "kalam-pdf-jump-label",
                        set_label: "Go to",
                        set_valign: gtk::Align::Center,
                    },

                    #[name = "jump_entry"]
                    gtk::Entry {
                        add_css_class: "kalam-pdf-jump-entry",
                        set_width_chars: 5,
                        set_max_width_chars: 7,
                        set_valign: gtk::Align::Center,
                        set_input_purpose: gtk::InputPurpose::Digits,
                        #[watch]
                        set_css_classes: if model.jump_out_of_range() {
                            &["kalam-pdf-jump-entry", "error"]
                        } else {
                            &["kalam-pdf-jump-entry"]
                        },
                    },

                    #[name = "jump_hint"]
                    gtk::Label {
                        add_css_class: "kalam-pdf-jump-hint",
                        set_valign: gtk::Align::Center,
                        set_hexpand: true,
                        set_halign: gtk::Align::End,
                        #[watch]
                        set_label: &model.jump_hint(),
                        #[watch]
                        set_css_classes: if model.jump_out_of_range() {
                            &["kalam-pdf-jump-hint", "error"]
                        } else {
                            &["kalam-pdf-jump-hint"]
                        },
                    },

                    #[name = "jump_mode_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_valign: gtk::Align::Center,
                    },

                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes(
                            "window-close-symbolic",
                            14,
                            &["kalam-inline-icon"],
                        )),
                        add_css_class: "kalam-pdf-jump-close",
                        set_valign: gtk::Align::Center,
                        set_tooltip_text: Some("Close (Esc)"),
                        connect_clicked => PdfReaderMsg::CloseJump,
                    },
                },
            },

            // ── 8. The left panel: Contents, Pages and Bookmarks ──
            // Floating 16 from every edge (decision 52); the tab
            // pill and pin ride at the top (decision 57). Notes
            // joins this panel in its own tranche.
            #[name = "left_revealer"]
            add_overlay = &gtk::Revealer {
                add_css_class: "kalam-reader-sidebar-shell",
                add_css_class: "kalam-reader-sidebar-shell-left",
                #[watch]
                set_reveal_child: model.left_panel_open,
                set_transition_type: gtk::RevealerTransitionType::SlideRight,
                set_transition_duration: 220,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Fill,
                set_margin_start: 16,
                set_margin_top: 16,
                set_margin_bottom: 16,

                #[name = "left_panel_box"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_size_request: (340, -1),
                    set_vexpand: true,
                    add_css_class: "kalam-reader-sidebar",
                    add_css_class: "kalam-reader-sidebar-left",
                    add_css_class: "kalam-pdf-panel",

                    #[name = "left_tabs_row"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        set_margin_start: 20,
                        set_margin_end: 20,
                        set_margin_top: 16,
                        // Search results take the panel over whole:
                        // tabs and pin both wait behind its header
                        // until it closes back to them (decision 61).
                        #[watch]
                        set_visible: !model.search_results_open,

                        #[name = "left_tabs_host"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_hexpand: true,
                        },

                        #[name = "left_pin_button"]
                        gtk::Button {
                            set_child: Some(&crate::icons::symbolic_with_classes(
                                "kalam-pin-symbolic",
                                18,
                                &["kalam-inline-icon"],
                            )),
                            add_css_class: "kalam-pdf-pin",
                            set_size_request: (36, 36),
                            set_valign: gtk::Align::Center,
                            connect_clicked => PdfReaderMsg::ToggleLeftPin,
                        },
                    },

                    #[name = "left_panel_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_hexpand: true,
                        set_vexpand: true,
                        set_margin_bottom: 8,
                    },
                },
            },

            // ── 9. The right panel: Reading and Shortcuts ───────────
            #[name = "right_revealer"]
            add_overlay = &gtk::Revealer {
                add_css_class: "kalam-reader-sidebar-shell",
                add_css_class: "kalam-reader-sidebar-shell-right",
                #[watch]
                set_reveal_child: model.right_panel_open,
                set_transition_type: gtk::RevealerTransitionType::SlideLeft,
                set_transition_duration: 220,
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Fill,
                set_margin_end: 16,
                set_margin_top: 16,
                set_margin_bottom: 16,

                #[name = "right_panel_box"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_size_request: (340, -1),
                    set_vexpand: true,
                    add_css_class: "kalam-reader-sidebar",
                    add_css_class: "kalam-reader-sidebar-right",
                    add_css_class: "kalam-pdf-panel",

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        set_margin_start: 20,
                        set_margin_end: 20,
                        set_margin_top: 16,

                        #[name = "right_tabs_host"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_hexpand: true,
                        },

                        #[name = "right_pin_button"]
                        gtk::Button {
                            set_child: Some(&crate::icons::symbolic_with_classes(
                                "kalam-pin-symbolic",
                                18,
                                &["kalam-inline-icon"],
                            )),
                            add_css_class: "kalam-pdf-pin",
                            set_size_request: (36, 36),
                            set_valign: gtk::Align::Center,
                            connect_clicked => PdfReaderMsg::ToggleRightPin,
                        },
                    },

                    #[name = "right_panel_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_hexpand: true,
                        set_vexpand: true,
                        set_margin_bottom: 8,
                    },
                },
            },

            // ── Search (decision 61): the 640×52 pill centred over
            // the visible page area. Pinned panels move that
            // centre; `sync_panel_layout` mirrors their insets
            // onto this revealer.
            #[name = "search_revealer"]
            add_overlay = &gtk::Revealer {
                add_css_class: "kalam-reader-search-shell",
                #[watch]
                set_reveal_child: model.search_active,
                set_transition_type: gtk::RevealerTransitionType::SlideDown,
                set_transition_duration: 220,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Start,
                set_margin_top: 16,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    add_css_class: "kalam-reader-search-bar",
                    add_css_class: "epub-search",
                    add_css_class: "pdf-search",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 4,
                    set_size_request: (640, 52),
                    set_valign: gtk::Align::Center,

                    #[name = "search_entry"]
                    gtk::SearchEntry {
                        add_css_class: "k-reader-search-entry",
                        #[watch]
                        set_placeholder_text: Some(model.search_scope.placeholder()),
                        set_width_request: 170,
                        set_hexpand: true,
                        set_valign: gtk::Align::Center,
                        connect_search_changed[sender] => move |entry| {
                            sender.input(PdfReaderMsg::UpdateSearchQuery(entry.text().to_string()));
                        },
                        connect_activate[sender] => move |_| {
                            sender.input(PdfReaderMsg::NextSearchResult);
                        },
                    },

                    #[name = "search_count_label"]
                    gtk::Label {
                        add_css_class: "kalam-reader-search-count",
                        #[watch]
                        set_css_classes: if model.search_status_is_accent() {
                            &["kalam-reader-search-count", "no-matches"]
                        } else {
                            &["kalam-reader-search-count"]
                        },
                        set_valign: gtk::Align::Center,
                        set_width_chars: 1,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        #[watch]
                        set_label: &model.search_count_text(),
                    },

                    #[name = "search_prev_btn"]
                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes("pan-up-symbolic", 16, &["kalam-inline-icon"])),
                        add_css_class: "kalam-reader-search-btn",
                        #[watch]
                        set_visible: model.search_controls_visible(),
                        set_valign: gtk::Align::Center,
                        set_tooltip_text: Some("Previous match (Shift+Enter)"),
                        connect_clicked => PdfReaderMsg::PrevSearchResult,
                    },

                    #[name = "search_next_btn"]
                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes("pan-down-symbolic", 16, &["kalam-inline-icon"])),
                        add_css_class: "kalam-reader-search-btn",
                        #[watch]
                        set_visible: model.search_controls_visible(),
                        set_valign: gtk::Align::Center,
                        set_tooltip_text: Some("Next match (Enter)"),
                        connect_clicked => PdfReaderMsg::NextSearchResult,
                    },

                    #[name = "search_results_btn"]
                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes("view-list-symbolic", 16, &["kalam-inline-icon"])),
                        add_css_class: "kalam-reader-search-btn",
                        #[watch]
                        set_css_classes: if model.search_results_open {
                            &["kalam-reader-search-btn", "search-results-open"]
                        } else {
                            &["kalam-reader-search-btn"]
                        },
                        #[watch]
                        set_visible: model.search_controls_visible(),
                        set_valign: gtk::Align::Center,
                        set_tooltip_text: Some("Show all results"),
                        connect_clicked => PdfReaderMsg::OpenSearchResults,
                    },

                    gtk::Separator {
                        add_css_class: "k-reader-search-divider",
                        set_orientation: gtk::Orientation::Vertical,
                        set_valign: gtk::Align::Center,
                        set_height_request: 24,
                    },

                    #[name = "search_scope_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_sensitive: !model.search_no_text,
                        #[watch]
                        set_opacity: if model.search_no_text { 0.45 } else { 1.0 },
                    },

                    #[name = "search_close_btn"]
                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes("window-close-symbolic", 14, &["kalam-inline-icon"])),
                        add_css_class: "kalam-reader-search-btn",
                        set_valign: gtk::Align::Center,
                        set_tooltip_text: Some("Close (Esc)"),
                        connect_clicked => PdfReaderMsg::CloseSearch,
                    },
                },
            },

            // ── The transient zoom readout ───────────────────────────
            // Bottom centre, only while it has something to say
            // (decision 53, the comics readout's home).
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_zoom_osd,
                set_transition_type: gtk::RevealerTransitionType::Crossfade,
                set_transition_duration: 250,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::End,
                set_can_target: false,
                set_margin_bottom: 24,

                #[wrap(Some)]
                set_child = &gtk::Label {
                    add_css_class: "kalam-zoom-osd",
                    #[watch]
                    set_label: &format!("{}%", model.zoom_percent),
                },
            },

            // ── The annotation toolbar (decision 66) ────────────────
            // Bottom centre, summoned by a rest in its zone; the
            // tool options ride in the same host, 10 above the pill.
            #[name = "draw_toolbar_revealer"]
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.draw_toolbar_visible
                    && model.doc_ready
                    && !model.open_state.is_blocking(),
                set_transition_type: gtk::RevealerTransitionType::SlideUp,
                set_transition_duration: 220,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::End,
                set_margin_bottom: 16,

                #[name = "draw_toolbar_host"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                },
            },

            // ── Whole-book open states ────────────────────────────────
            // Above every scrap of reader chrome: a book that will
            // not open gets the plain ground, one dashed card, and
            // Back by itself, with no Bubble behind it (decisions
            // 52 and 73).
            add_overlay = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                add_css_class: "kalam-pdf-open-state-scrim",
                #[watch]
                set_visible: model.open_state.is_blocking(),
                set_hexpand: true,
                set_vexpand: true,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Fill,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    add_css_class: "kalam-ui-state-card",
                    add_css_class: "kalam-pdf-open-state-card",
                    set_width_request: 380,
                    set_hexpand: true,
                    set_vexpand: true,
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    #[watch]
                    set_visible: model.open_state.is_blocking(),

                    gtk::Image {
                        add_css_class: "kalam-pdf-open-state-icon",
                        #[watch]
                        set_icon_name: Some(model.open_state.icon_name()),
                        set_pixel_size: 40,
                        set_halign: gtk::Align::Center,
                    },

                    gtk::Label {
                        add_css_class: "kalam-ui-state-heading",
                        add_css_class: "kalam-pdf-open-state-title",
                        set_halign: gtk::Align::Fill,
                        set_xalign: 0.5,
                        set_wrap: true,
                        #[watch]
                        set_label: model.open_state.title(),
                    },

                    gtk::Label {
                        add_css_class: "kalam-ui-meta",
                        add_css_class: "kalam-pdf-open-state-body",
                        set_halign: gtk::Align::Fill,
                        set_xalign: 0.5,
                        set_wrap: true,
                        set_max_width_chars: 40,
                        #[watch]
                        set_label: model.open_state.body(),
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 8,
                        add_css_class: "kalam-pdf-password-fields",
                        #[watch]
                        set_visible: model.open_state.is_password(),

                        #[name = "password_entry"]
                        gtk::PasswordEntry {
                            set_show_peek_icon: true,
                            set_placeholder_text: Some("Password"),
                            set_activates_default: false,
                            set_hexpand: true,
                            #[watch]
                            set_sensitive: !model.open_state.password_submitting(),
                            #[watch]
                            set_css_classes: if model.open_state.password_wrong() {
                                &[
                                    "kalam-ui-field",
                                    "kalam-pdf-password-entry",
                                    "error",
                                ]
                            } else {
                                &["kalam-ui-field", "kalam-pdf-password-entry"]
                            },
                        },

                        gtk::Label {
                            add_css_class: "kalam-pdf-password-error",
                            set_halign: gtk::Align::Start,
                            set_xalign: 0.0,
                            set_label: "Wrong password. Try again.",
                            #[watch]
                            set_visible: model.open_state.password_wrong(),
                        },
                    },

                    gtk::Button {
                        add_css_class: "kalam-ui-pill-button",
                        add_css_class: "primary",
                        add_css_class: "kalam-pdf-open-state-action",
                        set_halign: gtk::Align::Center,
                        set_label: "Unlock",
                        #[watch]
                        set_visible: model.open_state.is_password(),
                        #[watch]
                        set_sensitive: model.password_has_text
                            && !model.open_state.password_submitting(),
                        connect_clicked => PdfReaderMsg::SubmitPassword,
                    },

                    gtk::Button {
                        add_css_class: "kalam-ui-pill-button",
                        add_css_class: "boxed-ghost",
                        add_css_class: "kalam-pdf-open-state-action",
                        set_halign: gtk::Align::Center,
                        set_label: "Locate file",
                        #[watch]
                        set_visible: model.open_state == PdfOpenState::FileNotFound,
                        connect_clicked => PdfReaderMsg::LocateFile,
                    },

                    gtk::Button {
                        add_css_class: "kalam-ui-pill-button",
                        add_css_class: "boxed-ghost",
                        add_css_class: "kalam-pdf-open-state-action",
                        set_halign: gtk::Align::Center,
                        set_label: "Remove from library",
                        #[watch]
                        set_visible: model.open_state == PdfOpenState::CantOpen,
                        #[watch]
                        set_sensitive: !model.open_state_removing,
                        connect_clicked => PdfReaderMsg::ConfirmRemoveBook,
                    },
                },
            },

            #[name = "open_state_back_btn"]
            add_overlay = &gtk::Button {
                set_child: Some(&crate::icons::labelled(
                    "go-previous-symbolic",
                    16,
                    "Back",
                    6,
                )),
                add_css_class: "kalam-back-btn",
                add_css_class: "kalam-back-float",
                #[watch]
                set_visible: model.open_state.is_blocking(),
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Start,
                set_margin_start: 18,
                set_margin_top: 14,
                connect_clicked => PdfReaderMsg::Close,
            },
        }
    }
    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        crate::icons::init();
        let mut model = Self::new(init);
        let widgets = view_output!();

        // 1. The left panel stack: Contents (filter over the
        // outline), the lazy Pages grid, and Notes — highlights,
        // bookmarks and saved words behind its filter chips.
        let left_stack = gtk::Stack::new();
        left_stack.set_hexpand(true);
        left_stack.set_vexpand(true);
        left_stack.set_transition_type(gtk::StackTransitionType::Crossfade);

        let contents_pane = gtk::Box::new(gtk::Orientation::Vertical, 0);
        contents_pane.set_hexpand(true);
        contents_pane.set_vexpand(true);
        contents_pane.add_css_class("kalam-pdf-contents-pane");

        let toc_filter_entry = gtk::SearchEntry::new();
        toc_filter_entry.set_placeholder_text(Some("Filter contents"));
        toc_filter_entry.add_css_class("kalam-pdf-contents-filter");
        toc_filter_entry.set_hexpand(true);
        toc_filter_entry.set_margin_start(20);
        toc_filter_entry.set_margin_end(20);
        toc_filter_entry.set_margin_top(14);
        toc_filter_entry.set_margin_bottom(8);
        {
            let tx = sender.input_sender().clone();
            toc_filter_entry.connect_search_changed(move |entry| {
                let _ = tx.send(PdfReaderMsg::TocFilterChanged(entry.text().to_string()));
            });
        }
        contents_pane.append(&toc_filter_entry);

        let toc_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .build();
        toc_scroll.add_css_class("kalam-reader-panel-scroll");
        let toc_list_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        toc_list_box.set_hexpand(true);
        toc_list_box.set_vexpand(true);
        toc_scroll.set_child(Some(&toc_list_box));
        contents_pane.append(&toc_scroll);

        let (pages_pane, pages_grid, pages_widgets) =
            build_pdf_pages_pane(&model, &sender);
        {
            let tx = sender.input_sender().clone();
            pages_widgets
                .scroll
                .vadjustment()
                .connect_value_changed(move |adjustment| {
                    let _ = tx.send(PdfReaderMsg::PagesGridScrolled(adjustment.value()));
                });
        }

        let (notes_pane, notes_widgets) =
            notes::build_pdf_notes_pane(&sender, model.word_scope);

        let (search_results_pane, search_results_widgets) =
            build_pdf_search_results_pane(&sender);

        left_stack.add_named(&contents_pane, Some("contents"));
        left_stack.add_named(&pages_pane, Some("pages"));
        left_stack.add_named(&notes_pane, Some("notes"));
        left_stack.add_named(&search_results_pane, Some("search-results"));
        widgets.left_panel_host.append(&left_stack);

        // 2. The right panel stack: the Reading pane (the standing
        // settings builder until the Reading tranche rebuilds it
        // on the two-axis model) and Shortcuts.
        let right_stack = gtk::Stack::new();
        right_stack.set_hexpand(true);
        right_stack.set_vexpand(true);
        right_stack.set_transition_type(gtk::StackTransitionType::Crossfade);

        let settings_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .build();
        settings_scroll.add_css_class("kalam-reader-panel-scroll");
        let (settings_box, settings_w) = build_pdf_settings_panel(&model, &sender);
        settings_scroll.set_child(Some(&settings_box));

        let image_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .build();
        image_scroll.add_css_class("kalam-reader-panel-scroll");
        let (image_box, image_w) = build_pdf_image_panel(&model, &sender);
        image_scroll.set_child(Some(&image_box));

        let shortcuts_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .build();
        shortcuts_scroll.add_css_class("kalam-reader-panel-scroll");
        let shortcuts_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        shortcuts_box.set_hexpand(true);
        shortcuts_box.set_margin_start(20);
        shortcuts_box.set_margin_end(20);
        shortcuts_box.set_margin_bottom(20);
        build_pdf_shortcuts_panel(&shortcuts_box);
        shortcuts_scroll.set_child(Some(&shortcuts_box));

        right_stack.add_named(&settings_scroll, Some("reading"));
        right_stack.add_named(&image_scroll, Some("image"));
        right_stack.add_named(&shortcuts_scroll, Some("shortcuts"));
        widgets.right_panel_host.append(&right_stack);

        model.left_stack = Some(left_stack);
        model.right_stack = Some(right_stack);
        model.toc_list_box = Some(toc_list_box);
        model.toc_scroll = Some(toc_scroll);
        model.notes_widgets = Some(notes_widgets);
        model.pages_grid = Some(pages_grid);
        model.pages_widgets = Some(pages_widgets);
        model.search_results_widgets = Some(search_results_widgets);
        model.settings_widgets = Some(settings_w);
        model.image_widgets = Some(image_w);

        // The annotation toolbar and its tool options (decision 66).
        let drawing_widgets = drawings::build_drawing_toolbar(&sender);
        widgets.draw_toolbar_host.append(&drawing_widgets.root);
        drawings::sync_drawing_toolbar(&drawing_widgets, &model);
        model.drawing_widgets = Some(drawing_widgets);

        // The panel tab pills (decision 54): the shared segmented
        // control at panel size, stretched across the panel head
        // with each tab taking an equal share.
        let make_tabs = |labels: &[&str],
                         host: &gtk::Box,
                         on_change: Rc<dyn Fn(usize) + 'static>|
         -> crate::widgets::segmented::SegmentedControl {
            let control = crate::widgets::segmented::segmented_control(
                labels,
                0,
                crate::widgets::segmented::SegmentedSize::Panel,
                move |idx| on_change(idx),
            );
            let root = control.widget();
            root.set_hexpand(true);
            root.set_halign(gtk::Align::Fill);
            for button in control.buttons() {
                button.set_hexpand(true);
            }
            host.append(&root);
            control
        };
        {
            let tx = sender.input_sender().clone();
            model.left_tabs = Some(make_tabs(
                &["Contents", "Pages", "Notes"],
                &widgets.left_tabs_host,
                Rc::new(move |idx| {
                    let _ = tx.send(PdfReaderMsg::SetLeftTab(match idx {
                        1 => PdfLeftTab::Pages,
                        2 => PdfLeftTab::Notes,
                        _ => PdfLeftTab::Contents,
                    }));
                }),
            ));
            let tx = sender.input_sender().clone();
            model.right_tabs = Some(make_tabs(
                &["Reading", "Image", "Shortcuts"],
                &widgets.right_tabs_host,
                Rc::new(move |idx| {
                    let _ = tx.send(PdfReaderMsg::SetRightTab(match idx {
                        1 => PdfRightTab::Image,
                        2 => PdfRightTab::Shortcuts,
                        _ => PdfRightTab::Reading,
                    }));
                }),
            ));
            let tx = sender.input_sender().clone();
            model.jump_mode_control = Some(crate::widgets::segmented::segmented_control(
                &["Page", "Chapter"],
                0,
                crate::widgets::segmented::SegmentedSize::Small,
                move |idx| {
                    let _ = tx.send(PdfReaderMsg::SetJumpMode(if idx == 1 {
                        PdfJumpMode::Chapter
                    } else {
                        PdfJumpMode::Page
                    }));
                },
            ));
            widgets.jump_mode_host.append(
                &model
                    .jump_mode_control
                    .as_ref()
                    .expect("just built")
                    .widget(),
            );
            let tx = sender.input_sender().clone();
            let search_scope_control = crate::widgets::segmented::segmented_control(
                &["Chapter", "Book"],
                model.search_scope.toggle_index(),
                crate::widgets::segmented::SegmentedSize::Small,
                move |idx| {
                    let _ = tx.send(PdfReaderMsg::SetSearchScope(
                        PdfSearchScope::from_toggle_index(idx),
                    ));
                },
            );
            widgets
                .search_scope_host
                .append(&search_scope_control.widget());
            model.search_scope_control = Some(search_scope_control);
            for (button, label) in [
                (&widgets.search_prev_btn, "Previous match"),
                (&widgets.search_next_btn, "Next match"),
                (&widgets.search_results_btn, "Show all search results"),
                (&widgets.search_close_btn, "Close search"),
            ] {
                button.update_property(&[gtk::accessible::Property::Label(label)]);
            }
        }

        // The Back+Bubble unit (decision 15): the shell's own Back
        // pill, with the shared slide-out carrying an icon-only
        // Bubble behind it. Focus entering the pill reveals the
        // unit too, so keyboard readers meet it by tabbing.
        {
            let back_button = gtk::Button::new();
            back_button.set_child(Some(&crate::icons::labelled(
                "go-previous-symbolic",
                16,
                "Back",
                6,
            )));
            back_button.add_css_class("kalam-back-btn");
            back_button.add_css_class("kalam-back-float");
            back_button.set_can_focus(true);
            let tx = sender.input_sender().clone();
            back_button.connect_clicked(move |_| {
                let _ = tx.send(PdfReaderMsg::Close);
            });
            let tx = sender.input_sender().clone();
            back_button.connect_has_focus_notify(move |btn| {
                if btn.has_focus() {
                    let _ = tx.send(PdfReaderMsg::ShowBackChrome);
                }
            });

            let bubble_button = gtk::Button::new();
            bubble_button.set_child(Some(&crate::icons::symbolic_with_classes(
                "view-dual-symbolic",
                16,
                &["kalam-inline-icon"],
            )));
            bubble_button.set_tooltip_text(Some("Minimize to Bubble"));
            bubble_button.update_property(&[gtk::accessible::Property::Label(
                "Minimize to Bubble",
            )]);
            bubble_button.set_size_request(44, 44);

            let tx = sender.input_sender().clone();
            let unit = crate::widgets::slide_out_pill::slide_out_pill_with_button(
                &back_button,
                bubble_button,
                move || {
                    let _ = tx.send(PdfReaderMsg::MinimizeToBubble);
                },
            );
            widgets.back_host.append(&unit.widget());
            model.back_unit = Some(unit);
        }

        // Reduced motion turns the panel slides into fades
        // (decision 15, system §9) — decided once at open, like
        // the other readers.
        if crate::appearance::reduce_motion() {
            widgets
                .back_revealer
                .set_transition_type(gtk::RevealerTransitionType::Crossfade);
            widgets
                .left_revealer
                .set_transition_type(gtk::RevealerTransitionType::Crossfade);
            widgets
                .right_revealer
                .set_transition_type(gtk::RevealerTransitionType::Crossfade);
            widgets
                .jump_revealer
                .set_transition_type(gtk::RevealerTransitionType::Crossfade);
            widgets
                .search_revealer
                .set_transition_type(gtk::RevealerTransitionType::Crossfade);
            widgets
                .draw_toolbar_revealer
                .set_transition_type(gtk::RevealerTransitionType::Crossfade);
        }

        // The jump field's entry: every keystroke re-validates
        // (the accent range state is live), Enter jumps.
        {
            let tx = sender.input_sender().clone();
            widgets.jump_entry.connect_changed(move |entry| {
                let _ = tx.send(PdfReaderMsg::JumpTextChanged(entry.text().to_string()));
            });
            let tx = sender.input_sender().clone();
            widgets.jump_entry.connect_activate(move |_| {
                let _ = tx.send(PdfReaderMsg::SubmitJump);
            });
        }

        model.refresh_left_panel(&widgets, &sender);
        model.refresh_right_panel(&widgets);

        // 3. Connect Keyboard Controller. The panel and jump keys
        // are decision 53's map; text entry keeps its own keys, so
        // typing in the Contents filter, the jump field or search
        // never turns pages or exits the book.
        let tx = sender.input_sender().clone();
        let key = gtk::EventControllerKey::new();
        let tx_key = tx.clone();
        key.connect_key_pressed(move |controller, keyval, _keycode, state| {
            use gtk::gdk::Key;
            let ctrl = state.contains(gdk::ModifierType::CONTROL_MASK);
            let typing = controller
                .widget()
                .and_then(|widget| widget.root())
                .and_then(|root| root.focus())
                .is_some_and(|focus| focus.is::<gtk::Editable>() || focus.is::<gtk::Text>());
            if typing && keyval != Key::Escape {
                return gtk::glib::Propagation::Proceed;
            }
            match keyval {
                Key::Escape => {
                    let _ = tx_key.send(PdfReaderMsg::EscapeKey);
                    gtk::glib::Propagation::Stop
                }
                Key::BackSpace | Key::q | Key::Q => {
                    let _ = tx_key.send(PdfReaderMsg::Close);
                    gtk::glib::Propagation::Stop
                }
                Key::Left | Key::Page_Up => {
                    let _ = tx_key.send(PdfReaderMsg::PrevPage);
                    gtk::glib::Propagation::Stop
                }
                Key::Right | Key::Page_Down | Key::space => {
                    let _ = tx_key.send(PdfReaderMsg::NextPage);
                    gtk::glib::Propagation::Stop
                }
                Key::Home => {
                    let _ = tx_key.send(PdfReaderMsg::GoToFirstPage);
                    gtk::glib::Propagation::Stop
                }
                Key::End => {
                    let _ = tx_key.send(PdfReaderMsg::GoToLastPage);
                    gtk::glib::Propagation::Stop
                }
                Key::Up | Key::k | Key::K => {
                    let _ = tx_key.send(PdfReaderMsg::ScrollDelta(-1.0));
                    gtk::glib::Propagation::Stop
                }
                Key::Down | Key::j | Key::J => {
                    let _ = tx_key.send(PdfReaderMsg::ScrollDelta(1.0));
                    gtk::glib::Propagation::Stop
                }
                Key::b | Key::B => {
                    let _ = tx_key.send(PdfReaderMsg::ToggleBookmark);
                    gtk::glib::Propagation::Stop
                }
                Key::plus | Key::equal | Key::KP_Add => {
                    let _ = tx_key.send(PdfReaderMsg::ZoomIn);
                    gtk::glib::Propagation::Stop
                }
                Key::minus | Key::KP_Subtract => {
                    let _ = tx_key.send(PdfReaderMsg::ZoomOut);
                    gtk::glib::Propagation::Stop
                }
                Key::_0 | Key::KP_0 => {
                    let _ = tx_key.send(PdfReaderMsg::ResetZoom);
                    gtk::glib::Propagation::Stop
                }
                // Bare `c` toggles smart crop; Ctrl+C is handled by the
                // window-global shortcut below, so guard on the modifier.
                Key::c | Key::C if !ctrl => {
                    let _ = tx_key.send(PdfReaderMsg::ToggleSmartCrop);
                    gtk::glib::Propagation::Stop
                }
                Key::t | Key::T => {
                    let _ = tx_key.send(PdfReaderMsg::OpenContents);
                    gtk::glib::Propagation::Stop
                }
                Key::p | Key::P => {
                    let _ = tx_key.send(PdfReaderMsg::OpenPages);
                    gtk::glib::Propagation::Stop
                }
                Key::s | Key::S => {
                    let _ = tx_key.send(PdfReaderMsg::OpenReadingPanel);
                    gtk::glib::Propagation::Stop
                }
                Key::h | Key::H => {
                    let _ = tx_key.send(PdfReaderMsg::OpenNotes);
                    gtk::glib::Propagation::Stop
                }
                Key::z | Key::Z if ctrl => {
                    let _ = tx_key.send(PdfReaderMsg::UndoDelete);
                    gtk::glib::Propagation::Stop
                }
                Key::g | Key::G => {
                    let _ = tx_key.send(PdfReaderMsg::OpenJump(None));
                    gtk::glib::Propagation::Stop
                }
                _ => gtk::glib::Propagation::Proceed,
            }
        });
        root.add_controller(key);
        root.set_can_focus(true);
        root.grab_focus();

        // 3b. Window-global Ctrl+C / Ctrl+F. The key controller above only
        // fires while a widget inside this reader holds keyboard focus, and
        // nothing on the page canvas is focusable - so after a drag, or once
        // focus wanders to the sidebar, Ctrl+C silently did nothing until a
        // popover pulled focus back into the reader (field report: "I have
        // to press highlight, then cancel, and then it works"). A
        // Global-scope shortcut controller stays active for the whole window
        // without needing focus - the same fix the EPUB reader's jump-back
        // shortcut already uses. Both actions decline while the user is
        // typing in an entry, so the search box keeps its own Ctrl+C.
        if let (Some(copy_trigger), Some(search_trigger)) = (
            gtk::ShortcutTrigger::parse_string("<Control>c"),
            gtk::ShortcutTrigger::parse_string("<Control>f"),
        ) {
            let global = gtk::ShortcutController::new();
            global.set_scope(gtk::ShortcutScope::Global);

            let s_copy = sender.clone();
            global.add_shortcut(
                gtk::Shortcut::builder()
                    .trigger(&copy_trigger)
                    .action(&gtk::CallbackAction::new(move |w, _| {
                        let typing = w
                            .root()
                            .and_then(|r| r.focus())
                            .is_some_and(|f| f.is::<gtk::Editable>() || f.is::<gtk::Text>());
                        if typing {
                            return gtk::glib::Propagation::Proceed;
                        }
                        let _ = s_copy.input_sender().send(PdfReaderMsg::CopySelection);
                        gtk::glib::Propagation::Stop
                    }))
                    .build(),
            );

            let s_search = sender.clone();
            global.add_shortcut(
                gtk::Shortcut::builder()
                    .trigger(&search_trigger)
                    .action(&gtk::CallbackAction::new(move |w, _| {
                        let typing = w
                            .root()
                            .and_then(|r| r.focus())
                            .is_some_and(|f| f.is::<gtk::Editable>() || f.is::<gtk::Text>());
                        if typing {
                            return gtk::glib::Propagation::Proceed;
                        }
                        let _ = s_search.input_sender().send(PdfReaderMsg::ToggleSearch);
                        gtk::glib::Propagation::Stop
                    }))
                    .build(),
            );

            root.add_controller(global);
        }

        // 4. The Back unit's rest zone (decision 15): the top-left
        // 180 × 68. Enter/leave transitions are reported once each;
        // the 250 ms rest and the 300 ms grace live in the update
        // arms.
        {
            let root_motion = gtk::EventControllerMotion::new();
            let tx_rm = tx.clone();
            let was_in_zone = std::rc::Rc::new(std::cell::Cell::new(false));
            let was_in_zone_move = was_in_zone.clone();
            // The annotation toolbar's zone rides the same
            // controller: 252 wide, centred, in the bottom 92.
            let was_in_draw_zone = std::rc::Rc::new(std::cell::Cell::new(false));
            let was_in_draw_zone_move = was_in_draw_zone.clone();
            root_motion.connect_motion(move |controller, x, y| {
                let in_zone = x < 180.0 && y < 68.0;
                if in_zone != was_in_zone_move.get() {
                    was_in_zone_move.set(in_zone);
                    let _ = tx_rm.send(PdfReaderMsg::BackZoneHover(in_zone));
                }
                let (width, height) = controller
                    .widget()
                    .map(|widget| (widget.width() as f64, widget.height() as f64))
                    .unwrap_or((0.0, 0.0));
                let in_draw_zone = width > 0.0
                    && height > 0.0
                    && (x - width / 2.0).abs() <= 126.0
                    && y >= height - 92.0;
                if in_draw_zone != was_in_draw_zone_move.get() {
                    was_in_draw_zone_move.set(in_draw_zone);
                    let _ = tx_rm.send(PdfReaderMsg::DrawToolbarZoneHover(in_draw_zone));
                }
            });
            let tx_leave = tx.clone();
            root_motion.connect_leave(move |_| {
                if was_in_zone.get() {
                    was_in_zone.set(false);
                    let _ = tx_leave.send(PdfReaderMsg::BackZoneHover(false));
                }
                if was_in_draw_zone.get() {
                    was_in_draw_zone.set(false);
                    let _ = tx_leave.send(PdfReaderMsg::DrawToolbarZoneHover(false));
                }
            });
            root.add_controller(root_motion);
        }

        // 5. The edge strips summon their panels; leaving one
        // starts the grace that puts an unpinned panel away.
        {
            let left_edge_motion = gtk::EventControllerMotion::new();
            let tx_enter = tx.clone();
            left_edge_motion.connect_enter(move |_, _, _| {
                let _ = tx_enter.send(PdfReaderMsg::LeftEdgeHover(true));
            });
            let tx_leave = tx.clone();
            left_edge_motion.connect_leave(move |_| {
                let _ = tx_leave.send(PdfReaderMsg::LeftEdgeHover(false));
            });
            widgets.left_edge_box.add_controller(left_edge_motion);

            let right_edge_motion = gtk::EventControllerMotion::new();
            let tx_enter = tx.clone();
            right_edge_motion.connect_enter(move |_, _, _| {
                let _ = tx_enter.send(PdfReaderMsg::RightEdgeHover(true));
            });
            let tx_leave = tx.clone();
            right_edge_motion.connect_leave(move |_| {
                let _ = tx_leave.send(PdfReaderMsg::RightEdgeHover(false));
            });
            widgets.right_edge_box.add_controller(right_edge_motion);
        }

        // 6. Panel hover, the other half of each grace.
        {
            let left_panel_motion = gtk::EventControllerMotion::new();
            let tx_enter = tx.clone();
            left_panel_motion.connect_enter(move |_, _, _| {
                let _ = tx_enter.send(PdfReaderMsg::LeftPanelHover(true));
            });
            let tx_leave = tx.clone();
            left_panel_motion.connect_leave(move |_| {
                let _ = tx_leave.send(PdfReaderMsg::LeftPanelHover(false));
            });
            widgets.left_panel_box.add_controller(left_panel_motion);

            let right_panel_motion = gtk::EventControllerMotion::new();
            let tx_enter = tx.clone();
            right_panel_motion.connect_enter(move |_, _, _| {
                let _ = tx_enter.send(PdfReaderMsg::RightPanelHover(true));
            });
            let tx_leave = tx.clone();
            right_panel_motion.connect_leave(move |_| {
                let _ = tx_leave.send(PdfReaderMsg::RightPanelHover(false));
            });
            widgets.right_panel_box.add_controller(right_panel_motion);

            let draw_toolbar_motion = gtk::EventControllerMotion::new();
            let tx_enter = tx.clone();
            draw_toolbar_motion.connect_enter(move |_, _, _| {
                let _ = tx_enter.send(PdfReaderMsg::DrawToolbarHover(true));
            });
            let tx_leave = tx.clone();
            draw_toolbar_motion.connect_leave(move |_| {
                let _ = tx_leave.send(PdfReaderMsg::DrawToolbarHover(false));
            });
            widgets.draw_toolbar_host.add_controller(draw_toolbar_motion);
        }

        // 7. Clicking the location text opens the jump field: it
        // is the one piece of page furniture that names a place,
        // so it is where naming a different place starts.
        {
            let click = gtk::GestureClick::new();
            let tx_click = tx.clone();
            click.connect_pressed(move |gesture, _, _, _| {
                let _ = tx_click.send(PdfReaderMsg::OpenJump(None));
                gesture.set_state(gtk::EventSequenceState::Claimed);
            });
            widgets.location_label.add_controller(click);
        }

        // The transparent layer over the page: its click is spent
        // putting the floating panels away (decision 52).
        {
            let click = gtk::GestureClick::new();
            let root_for_focus = root.clone();
            let tx_click = tx.clone();
            click.connect_pressed(move |gesture, _, _, _| {
                root_for_focus.grab_focus();
                let _ = tx_click.send(PdfReaderMsg::ClosePanels);
                gesture.set_state(gtk::EventSequenceState::Claimed);
            });
            widgets.panel_click_catcher.add_controller(click);
        }

        // 8. Scroll controller to autohide controls on scrolling or zoom on Ctrl+scroll
        let scroll_ctrl = gtk::EventControllerScroll::new(
            gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::HORIZONTAL,
        );
        let tx_sc = tx.clone();
        let tx_ctrl_zoom = tx.clone();
        let zoom_scroll_accum = std::rc::Rc::new(std::cell::Cell::new(0.0f64));
        scroll_ctrl.connect_scroll(move |controller, _dx, dy| {
            let state = controller.current_event_state();
            if state.contains(gdk::ModifierType::CONTROL_MASK) {
                let acc = zoom_scroll_accum.get() + dy;
                if acc <= -0.8 {
                    zoom_scroll_accum.set(0.0);
                    let _ = tx_ctrl_zoom.send(PdfReaderMsg::ZoomIn);
                } else if acc >= 0.8 {
                    zoom_scroll_accum.set(0.0);
                    let _ = tx_ctrl_zoom.send(PdfReaderMsg::ZoomOut);
                } else {
                    zoom_scroll_accum.set(acc);
                }
                return gtk::glib::Propagation::Stop;
            }
            let _ = tx_sc.send(PdfReaderMsg::UserScrolled);
            gtk::glib::Propagation::Proceed
        });
        widgets.viewport_scroll.add_controller(scroll_ctrl);

        // 9. Track continuous scroll position changes to update current_page
        let vadj = widgets.viewport_scroll.vadjustment();
        let tx_vadj = tx.clone();
        vadj.connect_value_changed(move |_| {
            let _ = tx_vadj.send(PdfReaderMsg::UpdateScrollPage(0));
            let _ = tx_vadj.send(PdfReaderMsg::UserScrolled);
        });
        let hadj = widgets.viewport_scroll.hadjustment();
        let tx_hadj = tx.clone();
        hadj.connect_value_changed(move |_| {
            let _ = tx_hadj.send(PdfReaderMsg::UpdateScrollPage(0));
            let _ = tx_hadj.send(PdfReaderMsg::UserScrolled);
        });

        let tx_resize = tx.clone();
        hadj.connect_page_size_notify(move |_| {
            let _ = tx_resize.send(PdfReaderMsg::ViewportResized);
        });
        let tx_resize_v = tx.clone();
        vadj.connect_page_size_notify(move |_| {
            let _ = tx_resize_v.send(PdfReaderMsg::ViewportResized);
        });

        // A settings-driven viewport rebuild (scroll mode, spreads, gap)
        // swaps the scrolled window's child, but the adjustments only
        // describe the new layout once GTK has allocated it. The upper
        // notify is that "geometry is real" moment; `LayoutSettled` uses
        // it to run the deferred scroll back to the current page.
        let tx_upper_v = tx.clone();
        vadj.connect_upper_notify(move |_| {
            let _ = tx_upper_v.send(PdfReaderMsg::LayoutSettled);
        });
        let tx_upper_h = tx.clone();
        hadj.connect_upper_notify(move |_| {
            let _ = tx_upper_h.send(PdfReaderMsg::LayoutSettled);
        });

        // 10. Pinch zoom gesture for touchpads and touchscreens
        let zoom_gesture = gtk::GestureZoom::new();
        let tx_zg = tx.clone();
        let prev_scale = std::rc::Rc::new(std::cell::Cell::new(1.0f64));
        let prev_scale_clone = prev_scale.clone();
        zoom_gesture.connect_scale_changed(move |_, scale_factor| {
            let ratio = scale_factor / prev_scale.get();
            if ratio > 1.15 {
                prev_scale.set(scale_factor);
                let _ = tx_zg.send(PdfReaderMsg::ZoomIn);
            } else if ratio < 0.85 {
                prev_scale.set(scale_factor);
                let _ = tx_zg.send(PdfReaderMsg::ZoomOut);
            }
        });
        let prev_scale_end = prev_scale_clone;
        zoom_gesture.connect_end(move |_, _| {
            prev_scale_end.set(1.0);
        });
        widgets.viewport_scroll.add_controller(zoom_gesture);

        // Initial child and render triggering
        let child = model.build_viewport_widget(&sender);
        model.refresh_drawing_paints();
        widgets.viewport_scroll.set_child(Some(&child));
        model.update_scroll_policies(&widgets.viewport_scroll);

        // Start bounded background rendering workers (exactly 2 threads for lifetime of reader)
        let (render_tx, render_rx) = async_channel::unbounded::<PdfRenderRequest>();
        let active_gen = Arc::new(AtomicU64::new(1));

        for worker_id in 0..2 {
            let rx = render_rx.clone();
            let gen = active_gen.clone();
            let tx_msg = sender.input_sender().clone();
            let shared_password = model.session_password.clone();

            let _ = std::thread::Builder::new()
                .name(format!("kalam-pdf-worker-{}", worker_id))
                .spawn(move || {
                    let mut cached_path: Option<PathBuf> = None;
                    let mut cached_doc: Option<PdfDocument> = None;

                    while let Ok(req) = rx.recv_blocking() {
                        if req.generation != gen.load(Ordering::Relaxed) {
                            continue;
                        }

                        if cached_path.as_ref() != Some(&req.path) || cached_doc.is_none() {
                            cached_path = Some(req.path.clone());
                            cached_doc = pdf_open_document(&req.path, &shared_password).ok();
                        }

                        if let Some(ref doc) = cached_doc {
                            if let Ok(rendered) = doc.render_page_rgba_rotated(req.page, req.scale, req.smart_crop, req.rotation) {
                                if req.generation == gen.load(Ordering::Relaxed) {
                                    let bytes = glib::Bytes::from_owned(rendered.samples);
                                    let texture = gdk::MemoryTexture::new(
                                        rendered.width,
                                        rendered.height,
                                        gdk::MemoryFormat::R8g8b8a8,
                                        &bytes,
                                        rendered.stride,
                                    );
                                    let _ = tx_msg.send(PdfReaderMsg::PageRendered {
                                        generation: req.generation,
                                        page: rendered.page_num,
                                        texture: texture.upcast(),
                                        width: rendered.width,
                                        height: rendered.height,
                                    });
                                }
                            }
                        }
                    }
                });
        }

        let ocr_gen = active_gen.clone();
        model.render_tx = Some(render_tx);
        model.active_generation = Some(active_gen);
        model.render_generation = 1;

        // The Pages grid's one thumbnail worker. It drains each
        // available batch newest-first and renders a page once,
        // so a fast scroll does not leave the now-visible cells
        // queued behind every thumbnail they passed on the way.
        let (thumbnail_tx, thumbnail_rx) =
            async_channel::unbounded::<PdfThumbnailRequest>();
        let thumbnail_gen = Arc::new(AtomicU64::new(1));
        {
            let rx = thumbnail_rx.clone();
            let active = thumbnail_gen.clone();
            let tx_msg = sender.input_sender().clone();
            let shared_password = model.session_password.clone();
            let _ = std::thread::Builder::new()
                .name("kalam-pdf-thumbnail-worker".to_string())
                .spawn(move || {
                    let mut cached_path: Option<PathBuf> = None;
                    let mut cached_doc: Option<PdfDocument> = None;

                    while let Ok(first) = rx.recv_blocking() {
                        let mut batch = vec![first];
                        while let Ok(req) = rx.try_recv() {
                            batch.push(req);
                        }

                        let mut seen = HashSet::new();
                        for req in batch.into_iter().rev() {
                            if req.generation != active.load(Ordering::Relaxed)
                                || !seen.insert(req.page)
                            {
                                continue;
                            }

                            if cached_path.as_ref() != Some(&req.path) || cached_doc.is_none() {
                                cached_path = Some(req.path.clone());
                                cached_doc = pdf_open_document(&req.path, &shared_password).ok();
                            }

                            let texture = cached_doc.as_ref().and_then(|doc| {
                                let (width, height) =
                                    doc.page_dimensions(req.page).ok()?;
                                let scale = pages_grid::thumbnail_render_scale(
                                    width as f64,
                                    height as f64,
                                    req.rotation,
                                    req.target_width,
                                );
                                let rendered = doc
                                    .render_page_rgba_rotated(
                                        req.page,
                                        scale,
                                        false,
                                        req.rotation,
                                    )
                                    .ok()?;
                                if req.generation != active.load(Ordering::Relaxed) {
                                    return None;
                                }
                                let bytes = glib::Bytes::from_owned(rendered.samples);
                                let texture = gdk::MemoryTexture::new(
                                    rendered.width,
                                    rendered.height,
                                    gdk::MemoryFormat::R8g8b8a8,
                                    &bytes,
                                    rendered.stride,
                                );
                                Some(texture.upcast::<gdk::Texture>())
                            });

                            if req.generation == active.load(Ordering::Relaxed) {
                                let _ = tx_msg.send(PdfReaderMsg::PageThumbnailReady {
                                    generation: req.generation,
                                    page: req.page,
                                    texture,
                                });
                            }
                        }
                    }
                });
        }
        model.thumbnail_tx = Some(thumbnail_tx);
        model.thumbnail_active_generation = Some(thumbnail_gen);
        model.thumbnail_generation = 1;

        // Start background OCR worker thread for scanned PDF pages
        let (ocr_tx, ocr_rx) = async_channel::unbounded::<crate::ocr::PdfOcrRequest>();
        let tx_ocr_msg = sender.input_sender().clone();
        let shared_password = model.session_password.clone();

        let _ = std::thread::Builder::new()
            .name("kalam-pdf-ocr-worker".to_string())
            .spawn(move || {
                let mut cached_path: Option<PathBuf> = None;
                let mut cached_doc: Option<PdfDocument> = None;
                let mut ocr_engine: Option<ocrs::OcrEngine> = None;
                let mut engine_attempted = false;

                while let Ok(req) = ocr_rx.recv_blocking() {
                    if req.generation != ocr_gen.load(Ordering::Relaxed) {
                        continue;
                    }

                    if !engine_attempted {
                        ocr_engine = crate::ocr::init_ocr_engine();
                        engine_attempted = true;
                    }

                    let Some(ref engine) = ocr_engine else {
                        continue;
                    };

                    if cached_path.as_ref() != Some(&req.path) || cached_doc.is_none() {
                        cached_path = Some(req.path.clone());
                        cached_doc = pdf_open_document(&req.path, &shared_password).ok();
                    }

                    if let Some(ref doc) = cached_doc {
                        let scale = 2.0f32; // 144 DPI for crisp OCR character boundaries
                        if let Ok(rendered) = doc.render_page_rgba(req.page, scale, false) {
                            let (width_pts, height_pts) = doc.page_dimensions(req.page)
                                .unwrap_or((rendered.width as f32 / scale, rendered.height as f32 / scale));
                            match crate::ocr::perform_ocr(engine, &rendered, req.page, width_pts, height_pts) {
                                Ok(page_text) => {
                                    if req.generation == ocr_gen.load(Ordering::Relaxed) {
                                        let _ = tx_ocr_msg.send(PdfReaderMsg::PageOcrResult {
                                            generation: req.generation,
                                            page: req.page,
                                            fingerprint: pdf_path_fingerprint(&req.path),
                                            text: Box::new(Ok(page_text)),
                                        });
                                    }
                                }
                                Err(err) => {
                                    if req.generation == ocr_gen.load(Ordering::Relaxed) {
                                        let _ = tx_ocr_msg.send(PdfReaderMsg::PageOcrResult {
                                            generation: req.generation,
                                            page: req.page,
                                            fingerprint: pdf_path_fingerprint(&req.path),
                                            text: Box::new(Err(err.to_string())),
                                        });
                                    }
                                }
                            }
                        }
                    }
                }
            });

        model.ocr_tx = Some(ocr_tx);

        // Background document service (2.20): opens the file, extracts text
        // and runs searches off the UI thread. The UI thread never touches
        // the file; results arrive as messages.
        let (doc_tx, doc_rx) = async_channel::unbounded::<PdfDocRequest>();
        let tx_doc_msg = sender.input_sender().clone();
        let doc_catalog = model.catalog.clone();
        let doc_book_id = model.book_id;
        let shared_password = model.session_password.clone();
        let _ = std::thread::Builder::new()
            .name("kalam-pdf-doc-service".to_string())
            .spawn(move || {
                let mut cached_path: Option<PathBuf> = None;
                let mut cached_doc: Option<PdfDocument> = None;
                let mut cached_error: Option<PdfLoadError> = None;
                while let Ok(req) = doc_rx.recv_blocking() {
                    let path = match &req {
                        PdfDocRequest::Info { path, .. }
                        | PdfDocRequest::ExtractText { path, .. }
                        | PdfDocRequest::Search { path, .. } => path.clone(),
                    };
                    // A failed open is retried rather than cached as
                    // final: the password that unlocks the book can
                    // only arrive after the first refusal.
                    if cached_path.as_ref() != Some(&path) || cached_doc.is_none() {
                        crate::timing::span("pdf_open_doc");
                        cached_path = Some(path.clone());
                        match pdf_open_document(&path, &shared_password) {
                            Ok(doc) => {
                                cached_doc = Some(doc);
                                cached_error = None;
                            }
                            Err(error) => {
                                cached_doc = None;
                                cached_error = Some(error);
                            }
                        }
                        crate::timing::span_end("pdf_open_doc");
                    }
                    let Some(ref doc) = cached_doc else {
                        let error = cached_error.unwrap_or(PdfLoadError::CantOpen);
                        // Only an Info request may move the whole-book
                        // state. Text and search work queued behind a
                        // locked book answers its own caller and stops,
                        // so it can never overwrite the password card
                        // with a generic failure.
                        match &req {
                            PdfDocRequest::Info { .. } => {
                                let _ = tx_doc_msg.send(PdfReaderMsg::DocumentLoaded {
                                    info: Box::new(Err(error)),
                                });
                            }
                            PdfDocRequest::ExtractText { page, .. } => {
                                let _ = tx_doc_msg.send(PdfReaderMsg::PageTextReady {
                                    page: *page,
                                    fingerprint: None,
                                    text: Err(error.log_message().to_string()),
                                });
                            }
                            PdfDocRequest::Search { query, .. } => {
                                let _ = tx_doc_msg.send(PdfReaderMsg::SearchReady {
                                    query: query.clone(),
                                    outcome: PdfSearchOutcome {
                                        results: Vec::new(),
                                        has_searchable_text: false,
                                    },
                                });
                            }
                        }
                        continue;
                    };
                    match req {
                        PdfDocRequest::Info { page, .. } => {
                            let total_pages = doc.page_count();
                            // Measure the resumed page (clamped) so mixed
                            // page sizes keep the right aspect on resume,
                            // exactly as the old synchronous open did.
                            let measure = page.clamp(1, total_pages.max(1));
                            let (w, h) = doc
                                .page_dimensions(measure)
                                .map(|(w, h)| (w as f64, h as f64))
                                .unwrap_or((595.0, 842.0));
                            let info = PdfDocInfo {
                                total_pages,
                                toc_entries: doc.outlines().unwrap_or_default(),
                                page_labels: doc.page_labels(),
                                base_page_width: w,
                                base_page_height: h,
                            };
                            let _ = tx_doc_msg.send(PdfReaderMsg::DocumentLoaded {
                                info: Box::new(Ok(info)),
                            });
                        }
                        PdfDocRequest::ExtractText { page, .. } => {
                            let mut text = doc.extract_page_text(page);
                            // The fingerprint is computed here, off the UI
                            // thread, so the file is never stat'ed there.
                            let fingerprint = pdf_path_fingerprint(&path);
                            if matches!(&text, Ok(t) if t.total_chars() == 0) {
                                // Scanned page: try the persistent OCR cache
                                // first — a page OCR'd on a previous visit
                                // loads instantly instead of recomputing
                                // seconds of neural inference.
                                if let Some(ref fp) = fingerprint {
                                    if let Ok(Some(cached)) =
                                        doc_catalog.load_page_ocr(doc_book_id, page, fp)
                                    {
                                        log::info!(
                                            "OCR cache hit for page {page}: {} lines",
                                            cached.lines.len()
                                        );
                                        text = Ok(cached);
                                    }
                                }
                            }
                            let _ = tx_doc_msg.send(PdfReaderMsg::PageTextReady {
                                page,
                                fingerprint,
                                text: text.map_err(|e| e.to_string()),
                            });
                        }
                        PdfDocRequest::Search { query, toc, .. } => {
                            let outcome = doc.search_document(&query, &toc, 500);
                            let _ = tx_doc_msg.send(PdfReaderMsg::SearchReady { query, outcome });
                        }
                    }
                }
            });
        model.doc_tx = Some(doc_tx);

        // Ask for the document facts; the reader paints its skeleton
        // immediately and fills in when DocumentLoaded arrives.
        crate::timing::span("pdf_open_total");
        if let Some(ref path) = model.file_path {
            if let Some(ref tx) = model.doc_tx {
                let _ = tx.send_blocking(PdfDocRequest::Info {
                    path: path.clone(),
                    page: model.current_page,
                });
            }
        } else {
            let _ = sender.input_sender().send(PdfReaderMsg::DocumentLoaded {
                info: Box::new(Err(PdfLoadError::FileNotFound)),
            });
        }

        let cur = model.current_page;
        let _ = model.ensure_page_text(cur);

        if model.doc_ready {
            model.trigger_loads(&sender);
        }
        model.update_bookmark_icon_state(&widgets);

        let entry_key = gtk::EventControllerKey::new();
        let s_entry = sender.clone();
        entry_key.connect_key_pressed(move |_, keyval, _code, state| {
            if keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter {
                if state.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
                    s_entry.input(PdfReaderMsg::PrevSearchResult);
                } else {
                    s_entry.input(PdfReaderMsg::NextSearchResult);
                }
                gtk::glib::Propagation::Stop
            } else if keyval == gtk::gdk::Key::Escape {
                s_entry.input(PdfReaderMsg::EscapeKey);
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
        widgets.search_entry.add_controller(entry_key);

        // The password card's field. Its changed signal carries
        // no text into the model: before Unlock, the entry is
        // the only place the typed password exists.
        {
            let tx = sender.input_sender().clone();
            widgets.password_entry.connect_changed(move |_| {
                let _ = tx.send(PdfReaderMsg::PasswordEdited);
            });
            let tx = sender.input_sender().clone();
            widgets.password_entry.connect_activate(move |_| {
                let _ = tx.send(PdfReaderMsg::SubmitPassword);
            });
            widgets
                .open_state_back_btn
                .update_property(&[gtk::accessible::Property::Label("Back")]);
        }

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        // A whole-book open state owns the keyboard and chrome
        // until it resolves or the reader leaves. Its own card
        // messages, the document answer it is waiting for, and
        // text answers already in flight are the only traffic
        // allowed through.
        if self.open_state.is_blocking()
            && !matches!(
                msg,
                PdfReaderMsg::Close
                    | PdfReaderMsg::EscapeKey
                    | PdfReaderMsg::DocumentLoaded { .. }
                    | PdfReaderMsg::PageTextReady { .. }
                    | PdfReaderMsg::PasswordEdited
                    | PdfReaderMsg::SubmitPassword
                    | PdfReaderMsg::LocateFile
                    | PdfReaderMsg::ConfirmRemoveBook
                    | PdfReaderMsg::RemoveBook
                    | PdfReaderMsg::RemoveBookDone(_)
            )
        {
            return;
        }
        match msg {
            PdfReaderMsg::Close => {
                self.save_progress();
                if let Some(sid) = self.session_id {
                    let elapsed = self.session_start.elapsed().as_secs() as i64;
                    let pct = if self.total_pages > 0 {
                        ((self.current_page as f64 / self.total_pages as f64) * 100.0).round() as i64
                    } else {
                        0
                    };
                    let _ = self.catalog.end_reading_session(sid, elapsed, pct);
                }
                self.cleanup_memory();
                let _ = sender.output_sender().send(PdfReaderOut::Close);
            }
            PdfReaderMsg::MinimizeToBubble => {
                self.save_progress();
                let _ = sender.output_sender().send(PdfReaderOut::MinimizeToBubble { book_id: self.book_id });
            }
            PdfReaderMsg::PasswordEdited => {
                if let PdfOpenState::Password {
                    wrong,
                    submitting: false,
                } = self.open_state
                {
                    self.password_has_text = !widgets.password_entry.text().is_empty();
                    if wrong {
                        self.open_state = PdfOpenState::Password {
                            wrong: false,
                            submitting: false,
                        };
                    }
                }
            }
            PdfReaderMsg::SubmitPassword => {
                if let PdfOpenState::Password {
                    submitting: false,
                    ..
                } = self.open_state
                {
                    let password = widgets.password_entry.text().to_string();
                    if !password.is_empty() {
                        if let (Some(path), Some(tx)) =
                            (self.file_path.clone(), self.doc_tx.as_ref())
                        {
                            set_pdf_session_password(&self.session_password, Some(password));
                            self.password_has_text = true;
                            self.open_state = PdfOpenState::Password {
                                wrong: false,
                                submitting: true,
                            };
                            self.is_loading = true;
                            crate::timing::span("pdf_open_total");
                            let _ = tx.send_blocking(PdfDocRequest::Info {
                                path,
                                page: self.current_page,
                            });
                        }
                    }
                }
            }
            PdfReaderMsg::LocateFile => {
                if self.open_state == PdfOpenState::FileNotFound {
                    if let Some(path) = self.file_path.clone() {
                        crate::pages::book::open_in_file_manager(&path);
                    }
                }
            }
            PdfReaderMsg::ConfirmRemoveBook => {
                if self.open_state == PdfOpenState::CantOpen && !self.open_state_removing {
                    let title = self.title.clone();
                    let s = sender.input_sender().clone();
                    let dialog = confirmation_dialog(
                        _root,
                        "Remove this book?",
                        &format!(
                            "\u{201c}{title}\u{201d} will be removed from your library and its files \
                             deleted. This cannot be undone."
                        ),
                        "Keep book",
                        "Remove book",
                        ConfirmationKind::Destructive,
                        move || {
                            let _ = s.send(PdfReaderMsg::RemoveBook);
                        },
                    );
                    if dialog.is_none() {
                        crate::notify::error(
                            "Could not remove the book",
                            "The confirmation dialog could not be opened.",
                        );
                    }
                }
            }
            PdfReaderMsg::RemoveBook => {
                if self.open_state == PdfOpenState::CantOpen && !self.open_state_removing {
                    self.open_state_removing = true;
                    let id = self.book_id;
                    let title = self.title.clone();
                    let catalog = self.catalog.clone();
                    let s = sender.input_sender().clone();
                    crate::tasks::spawn(
                        format!("Deleting {title}"),
                        move |_reporter| {
                            catalog.delete_book(id).map_err(|e| e.to_string())
                        },
                        |_| {},
                        move |res| {
                            let _ = s.send(PdfReaderMsg::RemoveBookDone(res));
                        },
                    );
                }
            }
            PdfReaderMsg::RemoveBookDone(result) => {
                self.open_state_removing = false;
                match result {
                    Ok(()) => {
                        let title = self.title.clone();
                        crate::notify::success("Book removed", &title);
                        let _ = sender.input_sender().send(PdfReaderMsg::Close);
                    }
                    Err(err) => {
                        crate::notify::error("Could not remove the book", &err);
                    }
                }
            }
            PdfReaderMsg::EscapeKey => {
                // One layer at a time (decision 52): search, the
                // jump field, a selection, the floating panels,
                // then the book. A pinned panel is page layout, not
                // a layer, so it does not spend an Escape.
                if self.open_state.is_blocking() {
                    let _ = sender.input_sender().send(PdfReaderMsg::Close);
                } else if self.dict_popover.is_some() {
                    self.dismiss_dict_popover();
                } else if self.drawing_popover.is_some() {
                    self.dismiss_drawing_popover();
                } else if !self.draw_tool.is_select() {
                    self.set_draw_tool(PdfDrawTool::Select);
                    if let Some(widgets_draw) = &self.drawing_widgets {
                        drawings::sync_drawing_toolbar(widgets_draw, self);
                    }
                } else if self.selected_drawing_id.is_some() {
                    self.selected_drawing_id = None;
                    self.refresh_drawing_paints();
                    self.queue_all_drawing_areas();
                } else if self.draw_toolbar_visible {
                    self.draw_toolbar_visible = false;
                    self.draw_options_open = false;
                    if let Some(widgets_draw) = &self.drawing_widgets {
                        drawings::sync_drawing_toolbar(widgets_draw, self);
                    }
                } else if self.search_results_open {
                    let _ = sender
                        .input_sender()
                        .send(PdfReaderMsg::CloseSearchResults);
                } else if self.search_active {
                    let _ = sender.input_sender().send(PdfReaderMsg::CloseSearch);
                } else if self.jump_open {
                    self.jump_open = false;
                    _root.grab_focus();
                } else if self.active_selection.borrow().is_some() {
                    self.clear_selection();
                } else if (self.left_panel_open && !self.left_panel_pinned)
                    || (self.right_panel_open && !self.right_panel_pinned)
                {
                    if !self.left_panel_pinned {
                        self.left_panel_open = false;
                    }
                    if !self.right_panel_pinned {
                        self.right_panel_open = false;
                    }
                } else {
                    let _ = sender.input_sender().send(PdfReaderMsg::Close);
                }
            }
            PdfReaderMsg::ToggleSearch => {
                if self.search_active {
                    let _ = sender.input_sender().send(PdfReaderMsg::CloseSearch);
                } else {
                    // A fresh search always opens on the chapter
                    // (decision 61), whatever the last one chose.
                    self.clear_search_state(widgets, &sender);
                    self.search_active = true;
                    self.jump_open = false;
                    let entry = widgets.search_entry.clone();
                    glib::idle_add_local_once(move || {
                        entry.grab_focus();
                        entry.select_region(0, -1);
                    });
                }
            }
            PdfReaderMsg::UpdateSearchQuery(query) => {
                if widgets.search_entry.text().as_str() != query.as_str() {
                    // A caller outside the entry (the screenshot
                    // loop, a future shortcut) speaks through the
                    // entry, so the pill shows the words it is
                    // searching for. Setting the text raises
                    // search-changed; the duplicate that signal
                    // queues then meets the guard below and stops.
                    widgets.search_entry.set_text(&query);
                }
                if query == self.search_query {
                    return;
                }
                self.search_query = query.clone();
                self.search_index = 0;
                self.search_results.clear();
                self.search_no_text = false;
                self.search_chapter_anchor_page = self.current_page;
                self.search_query_anchor_page = self.current_page;
                self.search_pending = query.trim().chars().count() >= 2;
                if self.search_pending {
                    if let (Some(path), Some(tx)) =
                        (self.file_path.clone(), self.doc_tx.as_ref())
                    {
                        // Search runs on the doc service; results
                        // arrive in SearchReady and are applied
                        // only if the query is still current.
                        let _ = tx.send_blocking(PdfDocRequest::Search {
                            path,
                            query,
                            toc: self.toc_entries.clone(),
                        });
                    } else {
                        self.search_pending = false;
                    }
                }
                self.highlight_search_matches(widgets, &sender);
                self.update_search_results_panel();
            }
            PdfReaderMsg::SetSearchScope(scope) => {
                if self.search_no_text {
                    self.sync_search_scope_control();
                    return;
                }
                let standing_raw = self.current_search_result_index();
                let standing_page = standing_raw
                    .and_then(|raw| self.search_results.get(raw))
                    .map(|result| result.page)
                    .unwrap_or(self.current_page);
                if scope == PdfSearchScope::Chapter {
                    // Chapter means the chapter under the reader
                    // now, including the one a Book result just
                    // carried them into.
                    self.search_chapter_anchor_page = self.current_page;
                }
                self.search_scope = scope;
                self.sync_search_scope_control();
                let scoped = self.search_scoped_indices();
                self.search_index = standing_raw
                    .and_then(|raw| scoped.iter().position(|candidate| *candidate == raw))
                    .unwrap_or_else(|| {
                        pdf_search_start_position(
                            &self.search_results,
                            &scoped,
                            standing_page,
                        )
                    });
                self.highlight_search_matches(widgets, &sender);
                self.update_search_results_panel();
                if self.search_results_open {
                    if let Some(results) = &self.search_results_widgets {
                        results.list.scroll_to_current();
                    }
                }
            }
            PdfReaderMsg::SearchReady { query, outcome } => {
                if query == self.search_query {
                    self.search_pending = false;
                    self.search_no_text = !outcome.has_searchable_text;
                    self.search_results = outcome.results;
                    let scoped = self.search_scoped_indices();
                    self.search_index = pdf_search_start_position(
                        &self.search_results,
                        &scoped,
                        self.search_query_anchor_page,
                    );
                    self.highlight_search_matches(widgets, &sender);
                    self.update_search_results_panel();
                    if self.search_results_open {
                        if let Some(results) = &self.search_results_widgets {
                            results.list.scroll_to_current();
                        }
                    }
                }
            }
            PdfReaderMsg::NextSearchResult => {
                let scoped_len = self.search_scoped_indices().len();
                if scoped_len > 0 {
                    self.search_index = (self.search_index + 1) % scoped_len;
                    self.highlight_search_matches(widgets, &sender);
                    self.update_search_results_panel();
                    if self.search_results_open {
                        if let Some(results) = &self.search_results_widgets {
                            results.list.scroll_to_current();
                        }
                    }
                }
            }
            PdfReaderMsg::PrevSearchResult => {
                let scoped_len = self.search_scoped_indices().len();
                if scoped_len > 0 {
                    self.search_index = if self.search_index == 0 {
                        scoped_len - 1
                    } else {
                        self.search_index - 1
                    };
                    self.highlight_search_matches(widgets, &sender);
                    self.update_search_results_panel();
                    if self.search_results_open {
                        if let Some(results) = &self.search_results_widgets {
                            results.list.scroll_to_current();
                        }
                    }
                }
            }
            PdfReaderMsg::OpenSearchResults => {
                if self.search_active {
                    self.open_search_results(widgets, &sender);
                }
            }
            PdfReaderMsg::CloseSearchResults => {
                if self.search_results_open {
                    self.close_search_results(widgets, &sender);
                }
            }
            PdfReaderMsg::JumpToSearchResult(raw_index) => {
                let scoped = self.search_scoped_indices();
                if let Some(position) =
                    scoped.iter().position(|candidate| *candidate == raw_index)
                {
                    self.search_index = position;
                    self.highlight_search_matches(widgets, &sender);
                    self.update_search_results_panel();
                    if self.search_results_open {
                        if let Some(results) = &self.search_results_widgets {
                            results.list.scroll_to_current();
                        }
                    }
                }
            }
            PdfReaderMsg::CloseSearch => {
                self.search_active = false;
                self.clear_search_state(widgets, &sender);
                _root.grab_focus();
            }
            PdfReaderMsg::NextPage => {
                if self.current_page < self.total_pages {
                    self.clear_selection();
                    let target_page = match self.spread_mode {
                        PdfSpreadMode::NoSpreads => self.current_page + 1,
                        _ => {
                            let cur_spread = self.spread_for_page(self.current_page);
                            if let Some(next_s) = self.next_spread(cur_spread) {
                                next_s.0
                            } else {
                                self.total_pages
                            }
                        }
                    };
                    self.current_page = target_page.min(self.total_pages);
                    self.save_progress();
                    self.prune_textures();
                    self.trigger_loads(&sender);

                    match self.scroll_mode {
                        PdfScrollMode::PageScrolling => {
                            match self.spread_mode {
                                PdfSpreadMode::NoSpreads => self.update_paged_view(),
                                _ => self.update_two_page_view(),
                            }
                        }
                        _ => {
                            self.scroll_to_current_page(&widgets.viewport_scroll);
                        }
                    }
                    self.update_bookmark_icon_state(widgets);
                }
            }
            PdfReaderMsg::PrevPage => {
                if self.current_page > 1 {
                    self.clear_selection();
                    let target_page = match self.spread_mode {
                        PdfSpreadMode::NoSpreads => self.current_page.saturating_sub(1),
                        _ => {
                            let cur_spread = self.spread_for_page(self.current_page);
                            if let Some(prev_s) = self.prev_spread(cur_spread) {
                                prev_s.0
                            } else {
                                1
                            }
                        }
                    };
                    self.current_page = target_page.max(1);
                    self.save_progress();
                    self.prune_textures();
                    self.trigger_loads(&sender);

                    match self.scroll_mode {
                        PdfScrollMode::PageScrolling => {
                            match self.spread_mode {
                                PdfSpreadMode::NoSpreads => self.update_paged_view(),
                                _ => self.update_two_page_view(),
                            }
                        }
                        _ => {
                            self.scroll_to_current_page(&widgets.viewport_scroll);
                        }
                    }
                    self.update_bookmark_icon_state(widgets);
                }
            }
            PdfReaderMsg::GoToFirstPage => {
                if self.current_page != 1 {
                    self.clear_selection();
                    self.current_page = 1;
                    self.save_progress();
                    self.prune_textures();
                    self.trigger_loads(&sender);
                    match self.scroll_mode {
                        PdfScrollMode::PageScrolling => {
                            match self.spread_mode {
                                PdfSpreadMode::NoSpreads => self.update_paged_view(),
                                _ => self.update_two_page_view(),
                            }
                        }
                        _ => {
                            self.scroll_to_current_page(&widgets.viewport_scroll);
                        }
                    }
                    self.update_bookmark_icon_state(widgets);
                }
            }
            PdfReaderMsg::GoToLastPage => {
                if self.current_page != self.total_pages {
                    self.clear_selection();
                    self.current_page = self.total_pages;
                    self.save_progress();
                    self.prune_textures();
                    self.trigger_loads(&sender);
                    match self.scroll_mode {
                        PdfScrollMode::PageScrolling => {
                            match self.spread_mode {
                                PdfSpreadMode::NoSpreads => self.update_paged_view(),
                                _ => self.update_two_page_view(),
                            }
                        }
                        _ => {
                            self.scroll_to_current_page(&widgets.viewport_scroll);
                        }
                    }
                    self.update_bookmark_icon_state(widgets);
                }
            }
            PdfReaderMsg::ZoomIn => {
                let _ = sender.input_sender().send(PdfReaderMsg::SetZoomPercent(
                    self.zoom_percent.saturating_add(10),
                ));
            }
            PdfReaderMsg::ZoomOut => {
                let _ = sender.input_sender().send(PdfReaderMsg::SetZoomPercent(
                    self.zoom_percent.saturating_sub(10),
                ));
            }
            PdfReaderMsg::SetZoomPercent(percent) => {
                let clamped = percent.clamp(PDF_MIN_ZOOM_PERCENT, PDF_MAX_ZOOM_PERCENT);
                if self.zoom_percent != clamped {
                    let old_zoom = self.zoom_level;
                    self.zoom_percent = clamped;
                    self.refresh_fit_zoom(&widgets.viewport_scroll);
                    self.persist_fit_zoom();
                    self.apply_zoom_change(old_zoom, &sender, &widgets.viewport_scroll);
                    if self.search_active {
                        self.refresh_search_highlights();
                    }
                }
            }
            PdfReaderMsg::SetFitMode(fit) => {
                // Choosing a fit means that fit exactly: any zoom
                // away from it is forgotten, as in the comic reader.
                if self.fit_mode != fit || self.zoom_percent != 100 {
                    let old_zoom = self.zoom_level;
                    self.fit_mode = fit;
                    self.zoom_percent = 100;
                    self.pre_zoom_fit = None;
                    self.refresh_fit_zoom(&widgets.viewport_scroll);
                    self.persist_fit_zoom();
                    self.apply_zoom_change(old_zoom, &sender, &widgets.viewport_scroll);
                    if self.search_active {
                        self.refresh_search_highlights();
                    }
                }
            }
            PdfReaderMsg::ToggleFitOriginal => {
                // Fit ↔ actual size: a zoomed page first returns to
                // its fit; a fitted page goes to Original, keeping
                // the fit it came from for the way back.
                let old_zoom = self.zoom_level;
                if self.zoom_percent != 100 {
                    self.zoom_percent = 100;
                } else if self.fit_mode != PdfFitMode::Original {
                    self.pre_zoom_fit = Some(self.fit_mode);
                    self.fit_mode = PdfFitMode::Original;
                } else {
                    self.fit_mode = self.pre_zoom_fit.take().unwrap_or(PdfFitMode::Height);
                }
                self.refresh_fit_zoom(&widgets.viewport_scroll);
                self.persist_fit_zoom();
                self.apply_zoom_change(old_zoom, &sender, &widgets.viewport_scroll);
                if self.search_active {
                    self.refresh_search_highlights();
                }
            }
            PdfReaderMsg::ResetZoom => {
                // The 0 key's standing meaning in this reader is
                // actual size (see the Shortcuts pane).
                let _ = sender
                    .input_sender()
                    .send(PdfReaderMsg::SetFitMode(PdfFitMode::Original));
            }
            PdfReaderMsg::SetLayout(layout) => {
                // An explicit tile press ends the window-shaped
                // default, even when it names the layout already on
                // screen: from here the book keeps this answer.
                self.layout_auto = false;
                let target = pdf_spread_mode_for_layout(layout, self.cover_on_its_own);
                if target == self.spread_mode {
                    let mode_str = match target {
                        PdfSpreadMode::NoSpreads => "none",
                        PdfSpreadMode::OddSpreads => "odd",
                        PdfSpreadMode::EvenSpreads => "even",
                    };
                    self.catalog.set_pref(&format!("book.{}.pdf.spread_mode", self.book_id), mode_str);
                    self.catalog.set_pref("reader.pdf.spread_mode", mode_str);
                } else {
                    let _ = sender.input_sender().send(PdfReaderMsg::SetSpreadMode(target));
                }
            }
            PdfReaderMsg::SetFlow(flow) => {
                let _ = sender
                    .input_sender()
                    .send(PdfReaderMsg::SetScrollMode(pdf_scroll_mode_for_flow(flow)));
            }
            PdfReaderMsg::SetCoverOnItsOwn(on) => {
                if self.cover_on_its_own != on {
                    self.cover_on_its_own = on;
                    let value = if on { "1" } else { "0" };
                    self.catalog.set_pref(&format!("book.{}.pdf.cover_on_its_own", self.book_id), value);
                    self.catalog.set_pref("reader.pdf.cover_on_its_own", value);
                    if pdf_layout_for_spread_mode(self.spread_mode) == PdfLayout::Spread {
                        // Repairing is an explicit page-style choice too.
                        self.layout_auto = false;
                        let _ = sender.input_sender().send(PdfReaderMsg::SetSpreadMode(
                            pdf_spread_mode_for_layout(PdfLayout::Spread, on),
                        ));
                    }
                }
            }
            PdfReaderMsg::SetScrollGap(gap) => {
                let clamped = gap.clamp(0, PDF_MAX_SCROLL_GAP);
                if self.scroll_gap != clamped {
                    self.scroll_gap = clamped;
                    self.catalog.set_pref_i64(&format!("book.{}.pdf.scroll_gap", self.book_id), clamped as i64);
                    self.catalog.set_pref_i64("reader.pdf.scroll_gap", clamped as i64);
                    if self.scroll_mode != PdfScrollMode::PageScrolling {
                        // Container spacing is baked at build time.
                        let child = self.build_viewport_widget(&sender);
                        self.refresh_drawing_paints();
                        widgets.viewport_scroll.set_child(Some(&child));
                        self.prune_textures();
                        self.trigger_loads(&sender);
                        // Deferred scroll, same reason as SetScrollMode below.
                        self.viewport_rebuild_pending = self.current_page > 1;
                    }
                }
            }
            PdfReaderMsg::RotateView(delta) => {
                let next = normalize_pdf_rotation(self.rotation_degrees + delta);
                if next != self.rotation_degrees {
                    let old_zoom = self.zoom_level;
                    self.rotation_degrees = next;
                    self.catalog.set_pref_i64(&format!("book.{}.pdf.rotation", self.book_id), next as i64);
                    self.sync_display_page_dimensions();
                    self.invalidate_thumbnails();
                    // A quarter turn changes what every fit means.
                    self.refresh_fit_zoom(&widgets.viewport_scroll);
                    self.apply_zoom_change(old_zoom, &sender, &widgets.viewport_scroll);
                    if self.search_active {
                        self.refresh_search_highlights();
                    }
                }
            }
            PdfReaderMsg::SetScrollMode(mode) => {
                if self.scroll_mode != mode {
                    self.scroll_mode = mode;
                    let mode_str = match self.scroll_mode {
                        PdfScrollMode::PageScrolling => "page",
                        PdfScrollMode::VerticalScrolling => "vertical",
                        PdfScrollMode::HorizontalScrolling => "horizontal",
                        PdfScrollMode::WrappedScrolling => "wrapped",
                    };
                    self.catalog.set_pref(&format!("book.{}.pdf.scroll_mode", self.book_id), mode_str);
                    self.catalog.set_pref("reader.pdf.scroll_mode", mode_str);

                    // The flow is one of the fit's inputs (its margins,
                    // and whether a spread is sized as one).
                    if self.refresh_fit_zoom(&widgets.viewport_scroll) {
                        self.invalidate_renders();
                    }
                    self.update_scroll_policies(&widgets.viewport_scroll);

                    let child = self.build_viewport_widget(&sender);
                    self.refresh_drawing_paints();
                    widgets.viewport_scroll.set_child(Some(&child));
                    self.prune_textures();
                    self.trigger_loads(&sender);
                    if self.search_active {
                        self.refresh_search_highlights();
                    }
                    // The scroll back to the current page is deferred: right
                    // after set_child the adjustment still describes the OLD
                    // layout, so the position lands wrong, the value-changed
                    // handler then recomputes current_page against the new
                    // geometry far from where the reader actually stands, and
                    // the pages on screen stay placeholders until the user
                    // scrolls ("blank until I scroll a bit", 2026-10-05
                    // field run #2). LayoutSettled scrolls once the new
                    // extent has been allocated.
                    self.viewport_rebuild_pending =
                        self.scroll_mode != PdfScrollMode::PageScrolling && self.current_page > 1;
                }
            }
            PdfReaderMsg::SetSpreadMode(mode) => {
                if self.spread_mode != mode {
                    self.spread_mode = mode;
                    let mode_str = match self.spread_mode {
                        PdfSpreadMode::NoSpreads => "none",
                        PdfSpreadMode::OddSpreads => "odd",
                        PdfSpreadMode::EvenSpreads => "even",
                    };
                    self.catalog.set_pref(&format!("book.{}.pdf.spread_mode", self.book_id), mode_str);
                    self.catalog.set_pref("reader.pdf.spread_mode", mode_str);

                    if self.refresh_fit_zoom(&widgets.viewport_scroll) {
                        self.invalidate_renders();
                    }
                    self.update_scroll_policies(&widgets.viewport_scroll);
                    let child = self.build_viewport_widget(&sender);
                    self.refresh_drawing_paints();
                    widgets.viewport_scroll.set_child(Some(&child));
                    self.prune_textures();
                    self.trigger_loads(&sender);
                    if self.search_active {
                        self.refresh_search_highlights();
                    }
                    // Deferred scroll, same reason as SetScrollMode above.
                    self.viewport_rebuild_pending =
                        self.scroll_mode != PdfScrollMode::PageScrolling && self.current_page > 1;
                }
            }
            PdfReaderMsg::ApplyAutoSpreadMode(mode) => {
                // The window-shaped default: same rebuild as an
                // explicit choice, but nothing is stored — the next
                // window (or the next resize) is free to answer
                // differently until the reader picks a tile.
                if self.layout_auto && self.spread_mode != mode {
                    self.spread_mode = mode;
                    if self.refresh_fit_zoom(&widgets.viewport_scroll) {
                        self.invalidate_renders();
                    }
                    self.update_scroll_policies(&widgets.viewport_scroll);
                    let child = self.build_viewport_widget(&sender);
                    self.refresh_drawing_paints();
                    widgets.viewport_scroll.set_child(Some(&child));
                    self.prune_textures();
                    self.trigger_loads(&sender);
                    if self.search_active {
                        self.refresh_search_highlights();
                    }
                    self.viewport_rebuild_pending =
                        self.scroll_mode != PdfScrollMode::PageScrolling && self.current_page > 1;
                }
            }
            PdfReaderMsg::SetSpreadGap(gap) => {
                let clamped = gap.clamp(0, 48);
                if self.two_page_gap != clamped {
                    self.two_page_gap = clamped;
                    self.catalog.set_pref_i64("reader.pdf.two_page_gap", clamped as i64);
                    if self.spread_mode != PdfSpreadMode::NoSpreads {
                        if self.refresh_fit_zoom(&widgets.viewport_scroll) {
                            self.invalidate_renders();
                        }
                        self.update_scroll_policies(&widgets.viewport_scroll);
                        let child = self.build_viewport_widget(&sender);
                        self.refresh_drawing_paints();
                        widgets.viewport_scroll.set_child(Some(&child));
                        self.prune_textures();
                        self.trigger_loads(&sender);
                        // Deferred scroll, same reason as SetScrollMode above.
                        self.viewport_rebuild_pending = self.scroll_mode
                            != PdfScrollMode::PageScrolling
                            && self.current_page > 1;
                    }
                }
            }
            PdfReaderMsg::ToggleBookmark => {
                let standing = self
                    .bookmarks
                    .iter()
                    .find(|b| b.chapter_index as usize == self.current_page)
                    .cloned();
                if let Some(mark) = standing {
                    if crate::notify::report(
                        self.catalog.delete_reading_bookmark(mark.id),
                        "Could not remove the bookmark",
                    ) {
                        self.remember_deleted(PdfDeletedMark::Bookmark(Box::new(mark)));
                        crate::notify::compact("Bookmark removed", "");
                    }
                } else {
                    let _ = self.catalog.insert_reading_bookmark(
                        self.book_id,
                        self.current_page as i64,
                        self.progress_fraction(),
                        &format!("Page {}", self.current_page),
                    );
                    crate::notify::compact("Bookmark saved", &format!("Page {}", self.current_page));
                }
                self.reload_bookmarks();
                self.refresh_notes_panel(&sender);
                self.update_bookmark_icon_state(widgets);
            }
            PdfReaderMsg::DeleteBookmark(id) => {
                let row = self.bookmarks.iter().find(|mark| mark.id == id).cloned();
                if crate::notify::report(
                    self.catalog.delete_reading_bookmark(id),
                    "Could not delete the bookmark",
                ) {
                    if let Some(row) = row {
                        self.remember_deleted(PdfDeletedMark::Bookmark(Box::new(row)));
                    }
                }
                self.reload_bookmarks();
                self.refresh_notes_panel(&sender);
                self.update_bookmark_icon_state(widgets);
            }
            PdfReaderMsg::DeleteAnnotation(id) => {
                let row = self
                    .annotations
                    .iter()
                    .find(|anno| anno.id == id)
                    .cloned()
                    .or_else(|| self.catalog.get_annotation_by_id(id).ok().flatten());
                if crate::notify::report(
                    self.catalog.delete_annotation(id),
                    "Could not delete the highlight",
                ) {
                    if let Some(row) = row {
                        self.remember_deleted(PdfDeletedMark::Annotation(Box::new(row)));
                    }
                }
                self.reload_annotations();
                self.refresh_notes_panel(&sender);
            }
            PdfReaderMsg::DeleteSavedWord(id) => {
                let row = self.saved_words.iter().find(|word| word.id == id).cloned();
                if crate::notify::report(
                    self.catalog.delete_saved_word(id),
                    "Could not forget the word",
                ) {
                    if let Some(row) = row {
                        self.remember_deleted(PdfDeletedMark::SavedWord(Box::new(row)));
                    }
                }
                self.reload_saved_words();
                self.refresh_notes_panel(&sender);
            }
            PdfReaderMsg::DeleteDrawing(id) => {
                if let Some(drawing) = self
                    .drawings
                    .iter()
                    .find(|drawing| drawing.id == id)
                    .cloned()
                {
                    self.delete_drawing_row(drawing);
                }
            }
            PdfReaderMsg::SetDrawTool(tool) => {
                self.set_draw_tool(tool);
                if let Some(widgets_draw) = &self.drawing_widgets {
                    drawings::sync_drawing_toolbar(widgets_draw, self);
                }
            }
            PdfReaderMsg::SetHighlighterColor(color) => {
                self.draw_settings.highlighter_color = color;
                self.draw_settings.persist(&self.catalog);
                if let Some(widgets_draw) = &self.drawing_widgets {
                    drawings::sync_drawing_toolbar(widgets_draw, self);
                }
            }
            PdfReaderMsg::SetHighlighterOpacity(percent) => {
                self.draw_settings.highlighter_opacity_percent = match percent {
                    40 => 40,
                    80 => 80,
                    _ => 60,
                };
                self.draw_settings.persist(&self.catalog);
                if let Some(widgets_draw) = &self.drawing_widgets {
                    drawings::sync_drawing_toolbar(widgets_draw, self);
                }
            }
            PdfReaderMsg::SetPenColor(color) => {
                self.draw_settings.pen_color = color;
                self.draw_settings.persist(&self.catalog);
                if let Some(widgets_draw) = &self.drawing_widgets {
                    drawings::sync_drawing_toolbar(widgets_draw, self);
                }
            }
            PdfReaderMsg::SetPenWidth(width) => {
                self.draw_settings.pen_width = width;
                self.draw_settings.persist(&self.catalog);
                if let Some(widgets_draw) = &self.drawing_widgets {
                    drawings::sync_drawing_toolbar(widgets_draw, self);
                }
            }
            PdfReaderMsg::SetSelectedDrawingColor(color) => {
                if let Some(before) = self.selected_drawing_id.and_then(|id| {
                    self.drawings
                        .iter()
                        .find(|drawing| drawing.id == id)
                        .cloned()
                }) {
                    let mut after = before.clone();
                    after.color = color.as_str().to_string();
                    if crate::notify::report(
                        self.catalog.update_pdf_drawing(&after),
                        "Could not recolour the drawing",
                    ) {
                        self.remember_deleted(PdfDeletedMark::DrawingBefore(Box::new(before)));
                    }
                    self.reload_drawings();
                    self.refresh_drawing_paints();
                    self.queue_all_drawing_areas();
                }
            }
            PdfReaderMsg::SetSelectedDrawingWidthTenths(tenths) => {
                if let Some(before) = self.selected_drawing_id.and_then(|id| {
                    self.drawings
                        .iter()
                        .find(|drawing| drawing.id == id)
                        .cloned()
                }) {
                    let mut after = before.clone();
                    after.width_pts = (tenths as f64 / 10.0).clamp(0.1, 100.0);
                    if crate::notify::report(
                        self.catalog.update_pdf_drawing(&after),
                        "Could not change the drawing",
                    ) {
                        self.remember_deleted(PdfDeletedMark::DrawingBefore(Box::new(before)));
                    }
                    self.reload_drawings();
                    self.refresh_drawing_paints();
                    self.queue_all_drawing_areas();
                }
            }
            PdfReaderMsg::CloseDrawingPopover => {
                self.dismiss_drawing_popover();
            }
            PdfReaderMsg::DrawToolbarZoneHover(hovering) => {
                self.draw_toolbar_zone_hover = hovering;
                if hovering && self.doc_ready {
                    self.schedule_draw_toolbar_reveal(&sender);
                } else if !self.draw_toolbar_hover
                    && self.draw_tool.is_select()
                    && self.selected_drawing_id.is_none()
                {
                    self.schedule_draw_toolbar_hide(&sender);
                }
            }
            PdfReaderMsg::DrawToolbarHover(hovering) => {
                self.draw_toolbar_hover = hovering;
                if hovering {
                    self.draw_toolbar_visible = true;
                } else if !self.draw_toolbar_zone_hover
                    && self.draw_tool.is_select()
                    && self.selected_drawing_id.is_none()
                {
                    self.schedule_draw_toolbar_hide(&sender);
                }
            }
            PdfReaderMsg::DrawToolbarRevealDue(seq) => {
                if seq == self.draw_toolbar_seq
                    && (self.draw_toolbar_zone_hover || self.draw_toolbar_hover)
                {
                    self.draw_toolbar_visible = true;
                }
            }
            PdfReaderMsg::DrawToolbarHideDue(seq) => {
                if seq == self.draw_toolbar_seq
                    && !self.draw_toolbar_zone_hover
                    && !self.draw_toolbar_hover
                    && self.draw_tool.is_select()
                    && self.selected_drawing_id.is_none()
                {
                    self.draw_toolbar_visible = false;
                    self.draw_options_open = false;
                    if let Some(widgets_draw) = &self.drawing_widgets {
                        drawings::sync_drawing_toolbar(widgets_draw, self);
                    }
                }
            }
            PdfReaderMsg::UndoDelete => {
                // Decision 30: deletions are direct because Ctrl+Z
                // brings them back — the row as it stood, restored
                // exactly. A failed restore keeps its place on the
                // stack rather than being lost with the attempt.
                if let Some(mark) = self.deleted_marks.pop() {
                    match mark {
                        PdfDeletedMark::Annotation(row) => {
                            match self.catalog.restore_annotation(&row) {
                                Ok(()) => self.reload_annotations(),
                                Err(err) => {
                                    self.deleted_marks
                                        .push(PdfDeletedMark::Annotation(row));
                                    crate::notify::error(
                                        "Could not bring the highlight back",
                                        &err.to_string(),
                                    );
                                }
                            }
                        }
                        PdfDeletedMark::Bookmark(row) => {
                            match self.catalog.restore_reading_bookmark(&row) {
                                Ok(()) => {
                                    self.reload_bookmarks();
                                    self.update_bookmark_icon_state(widgets);
                                }
                                Err(err) => {
                                    self.deleted_marks
                                        .push(PdfDeletedMark::Bookmark(row));
                                    crate::notify::error(
                                        "Could not bring the bookmark back",
                                        &err.to_string(),
                                    );
                                }
                            }
                        }
                        PdfDeletedMark::SavedWord(row) => {
                            match self.catalog.restore_saved_word(&row) {
                                Ok(()) => self.reload_saved_words(),
                                Err(err) => {
                                    self.deleted_marks
                                        .push(PdfDeletedMark::SavedWord(row));
                                    crate::notify::error(
                                        "Could not bring the word back",
                                        &err.to_string(),
                                    );
                                }
                            }
                        }
                        PdfDeletedMark::Drawing(row) => {
                            match self.catalog.restore_pdf_drawing(&row) {
                                Ok(()) => {
                                    self.reload_drawings();
                                    self.refresh_drawing_paints();
                                    self.queue_all_drawing_areas();
                                }
                                Err(err) => {
                                    self.deleted_marks
                                        .push(PdfDeletedMark::Drawing(row));
                                    crate::notify::error(
                                        "Could not bring the drawing back",
                                        &err.to_string(),
                                    );
                                }
                            }
                        }
                        PdfDeletedMark::DrawingCreated(row) => {
                            match self.catalog.delete_pdf_drawing(row.id) {
                                Ok(()) => {
                                    if self.selected_drawing_id == Some(row.id) {
                                        self.selected_drawing_id = None;
                                        self.dismiss_drawing_popover();
                                    }
                                    self.reload_drawings();
                                    self.refresh_drawing_paints();
                                    self.queue_all_drawing_areas();
                                }
                                Err(err) => {
                                    self.deleted_marks
                                        .push(PdfDeletedMark::DrawingCreated(row));
                                    crate::notify::error(
                                        "Could not remove the drawing",
                                        &err.to_string(),
                                    );
                                }
                            }
                        }
                        PdfDeletedMark::DrawingBefore(row) => {
                            match self.catalog.update_pdf_drawing(&row) {
                                Ok(()) => {
                                    self.reload_drawings();
                                    self.refresh_drawing_paints();
                                    self.queue_all_drawing_areas();
                                }
                                Err(err) => {
                                    self.deleted_marks
                                        .push(PdfDeletedMark::DrawingBefore(row));
                                    crate::notify::error(
                                        "Could not put the drawing back",
                                        &err.to_string(),
                                    );
                                }
                            }
                        }
                    }
                    self.refresh_notes_panel(&sender);
                }
            }
            PdfReaderMsg::JumpToPage(target_page) => {
                self.clear_selection();
                self.current_page = target_page.clamp(1, self.total_pages);
                self.save_progress();
                self.prune_textures();
                self.trigger_loads(&sender);

                match self.scroll_mode {
                    PdfScrollMode::PageScrolling => {
                        match self.spread_mode {
                            PdfSpreadMode::NoSpreads => self.update_paged_view(),
                            _ => self.update_two_page_view(),
                        }
                    }
                    _ => {
                        self.scroll_to_current_page(&widgets.viewport_scroll);
                    }
                }
                self.jump_open = false;
                if !self.left_panel_pinned {
                    self.left_panel_open = false;
                }
                if !self.right_panel_pinned {
                    self.right_panel_open = false;
                }
                self.mouse_in_left_panel = false;
                self.mouse_in_right_panel = false;
                self.mouse_in_left_edge = false;
                self.mouse_in_right_edge = false;
                self.update_bookmark_icon_state(widgets);
            }
            PdfReaderMsg::SetSmartCrop(crop) => {
                if self.image_settings.trim_margins != crop {
                    self.image_settings.trim_margins = crop;
                    self.catalog.set_pref(
                        &format!("book.{}.pdf.smart_crop", self.book_id),
                        if self.image_settings.trim_margins { "1" } else { "0" },
                    );
                    self.apply_zoom_change(self.zoom_level, &sender, &widgets.viewport_scroll);
                }
            }
            PdfReaderMsg::ToggleSmartCrop => {
                let _ = sender.input_sender().send(PdfReaderMsg::SetSmartCrop(!self.image_settings.trim_margins));
            }
            PdfReaderMsg::SetPageColour(colour) => {
                if self.image_settings.page_colour != colour {
                    self.image_settings.page_colour = colour;
                    self.persist_image_settings();
                    self.refresh_page_images();
                }
            }
            PdfReaderMsg::SetTintStrength(value) => {
                let value = value.clamp(0, 100);
                if self.image_settings.tint_strength != value {
                    self.image_settings.tint_strength = value;
                    self.persist_image_settings();
                    self.refresh_page_images();
                }
            }
            PdfReaderMsg::SetBrightness(value) => {
                let value = value.clamp(
                    PdfImageSettings::MIN_ADJUSTMENT,
                    PdfImageSettings::MAX_ADJUSTMENT,
                );
                if self.image_settings.brightness != value {
                    self.image_settings.brightness = value;
                    self.persist_image_settings();
                    self.refresh_page_images();
                }
            }
            PdfReaderMsg::SetContrast(value) => {
                let value = value.clamp(
                    PdfImageSettings::MIN_ADJUSTMENT,
                    PdfImageSettings::MAX_ADJUSTMENT,
                );
                if self.image_settings.contrast != value {
                    self.image_settings.contrast = value;
                    self.persist_image_settings();
                    self.refresh_page_images();
                }
            }
            PdfReaderMsg::SetWarmth(value) => {
                let value = value.clamp(
                    PdfImageSettings::MIN_ADJUSTMENT,
                    PdfImageSettings::MAX_ADJUSTMENT,
                );
                if self.image_settings.warmth != value {
                    self.image_settings.warmth = value;
                    self.persist_image_settings();
                    self.refresh_page_images();
                }
            }
            PdfReaderMsg::SetGrayscale(on) => {
                if self.image_settings.grayscale != on {
                    self.image_settings.grayscale = on;
                    self.persist_image_settings();
                    self.refresh_page_images();
                }
            }
            PdfReaderMsg::ResetImageSettings => {
                // Back to the theme's own starting answers. Trim
                // margins is the one reset that reaches the raster:
                // a trimmed page was rendered cropped, so putting
                // it back goes through the same re-render its own
                // switch uses.
                let theme = crate::theme::current(&self.catalog);
                self.image_paper_rgb = image::rgb_from_hex(theme.paper);
                let was_trimmed = self.image_settings.trim_margins;
                self.image_settings =
                    PdfImageSettings::defaults_for_theme(theme.dark, false);
                self.persist_image_settings();
                self.refresh_page_images();
                if was_trimmed {
                    self.catalog.set_pref(
                        &format!("book.{}.pdf.smart_crop", self.book_id),
                        "0",
                    );
                    self.apply_zoom_change(self.zoom_level, &sender, &widgets.viewport_scroll);
                }
            }
            PdfReaderMsg::SetLeftTab(tab) => {
                // Choosing a tab is one of the ways back out of
                // the search results' takeover (decision 61).
                self.search_results_open = false;
                self.left_tab = tab;
                self.refresh_left_panel(widgets, &sender);
                if tab == PdfLeftTab::Pages {
                    self.scroll_pages_to_current();
                }
            }
            PdfReaderMsg::SetRightTab(tab) => {
                self.right_tab = tab;
                self.refresh_right_panel(widgets);
            }
            PdfReaderMsg::OpenLeftPanel => {
                let was_open = self.left_panel_open;
                self.left_panel_open = true;
                self.show_back_button = false;
                self.back_gen = self.back_gen.wrapping_add(1);
                widgets.left_revealer.set_reveal_child(true);
                self.expand_current_toc_branch();
                self.refresh_left_panel(widgets, &sender);
                if !was_open
                    && self.left_tab == PdfLeftTab::Contents
                    && !self.search_results_open
                {
                    self.scroll_toc_to_current();
                }
                if !was_open
                    && self.left_tab == PdfLeftTab::Pages
                    && !self.search_results_open
                {
                    self.scroll_pages_to_current();
                }
                if !was_open && self.search_results_open {
                    if let Some(results) = &self.search_results_widgets {
                        results.list.scroll_to_current();
                    }
                }
            }
            PdfReaderMsg::OpenRightPanel => {
                self.right_panel_open = true;
                widgets.right_revealer.set_reveal_child(true);
                self.refresh_right_panel(widgets);
            }
            PdfReaderMsg::OpenContents => {
                if self.left_panel_open
                    && self.left_tab == PdfLeftTab::Contents
                    && !self.search_results_open
                {
                    self.left_panel_open = false;
                    self.sync_panel_layout(widgets);
                } else {
                    self.search_results_open = false;
                    self.left_tab = PdfLeftTab::Contents;
                    let _ = sender.input_sender().send(PdfReaderMsg::OpenLeftPanel);
                }
            }
            PdfReaderMsg::OpenPages => {
                if self.left_panel_open
                    && self.left_tab == PdfLeftTab::Pages
                    && !self.search_results_open
                {
                    self.left_panel_open = false;
                    self.sync_panel_layout(widgets);
                } else {
                    // Route through SetLeftTab so choosing Pages
                    // from an already-open Contents or Notes
                    // panel also takes its opening scroll.
                    sender.input(PdfReaderMsg::SetLeftTab(PdfLeftTab::Pages));
                    sender.input(PdfReaderMsg::OpenLeftPanel);
                }
            }
            PdfReaderMsg::OpenNotes => {
                // H summons Notes on Highlights (decisions 60, 76);
                // asking again from that exact place closes it.
                if self.left_panel_open
                    && self.left_tab == PdfLeftTab::Notes
                    && self.notes_filter == PdfNotesFilter::Highlights
                    && !self.search_results_open
                {
                    self.left_panel_open = false;
                    self.sync_panel_layout(widgets);
                } else {
                    self.search_results_open = false;
                    self.left_tab = PdfLeftTab::Notes;
                    self.notes_filter = PdfNotesFilter::Highlights;
                    sender.input(PdfReaderMsg::OpenLeftPanel);
                }
            }
            PdfReaderMsg::SetNotesFilter(filter) => {
                self.notes_filter = filter;
                self.refresh_notes_panel(&sender);
            }
            PdfReaderMsg::ToggleHighlightColourFilter(colour) => {
                self.highlight_colour_filter =
                    if self.highlight_colour_filter == Some(colour) {
                        None
                    } else {
                        Some(colour)
                    };
                self.refresh_notes_panel(&sender);
            }
            PdfReaderMsg::ToggleHighlightSearch => {
                self.highlight_search_open = !self.highlight_search_open;
                if !self.highlight_search_open {
                    self.highlight_search_query.clear();
                }
                self.refresh_notes_panel(&sender);
                if self.highlight_search_open {
                    if let Some(notes) = &self.notes_widgets {
                        notes.focus_highlight_search();
                    }
                }
            }
            PdfReaderMsg::HighlightSearchChanged(query) => {
                if query != self.highlight_search_query {
                    self.highlight_search_query = query;
                    self.refresh_notes_panel(&sender);
                }
            }
            PdfReaderMsg::SetWordScope(scope) => {
                self.word_scope = scope;
                self.refresh_notes_panel(&sender);
            }
            PdfReaderMsg::WordSearchChanged(query) => {
                if query != self.word_search_query {
                    self.word_search_query = query;
                    self.refresh_notes_panel(&sender);
                }
            }
            PdfReaderMsg::SetPagesColumns(columns) => {
                let columns = if columns == 2 { 2 } else { 3 };
                if self.pages_columns != columns {
                    // Anchor the density change on the page at the
                    // grid's top edge, not on whichever pixel row
                    // happens to occupy after every cell resizes.
                    let anchor = self
                        .pages_widgets
                        .as_ref()
                        .and_then(|pages| {
                            self.pages_grid
                                .as_ref()
                                .and_then(|grid| {
                                    grid.first_visible_page(
                                        pages.scroll.vadjustment().value(),
                                    )
                                })
                        })
                        .unwrap_or(self.current_page);
                    self.pages_columns = columns;
                    self.catalog
                        .set_pref_i64("reader.pdf.pages_columns", columns as i64);
                    if let Some(grid) = &self.pages_grid {
                        grid.set_columns(columns);
                        grid.scroll_to_page(anchor);
                    }
                    self.update_pages_heading();
                }
            }
            PdfReaderMsg::OpenReadingPanel => {
                if self.right_panel_open && self.right_tab == PdfRightTab::Reading {
                    self.right_panel_open = false;
                    self.sync_panel_layout(widgets);
                } else {
                    self.right_tab = PdfRightTab::Reading;
                    let _ = sender.input_sender().send(PdfReaderMsg::OpenRightPanel);
                }
            }
            PdfReaderMsg::OpenShortcutsPanel => {
                if self.right_panel_open && self.right_tab == PdfRightTab::Shortcuts {
                    self.right_panel_open = false;
                    self.sync_panel_layout(widgets);
                } else {
                    self.right_tab = PdfRightTab::Shortcuts;
                    let _ = sender.input_sender().send(PdfReaderMsg::OpenRightPanel);
                }
            }
            PdfReaderMsg::ToggleLeftPanel => {
                if self.left_panel_open {
                    self.left_panel_open = false;
                    self.sync_panel_layout(widgets);
                } else {
                    let _ = sender.input_sender().send(PdfReaderMsg::OpenLeftPanel);
                }
            }
            PdfReaderMsg::ToggleRightPanel => {
                if self.right_panel_open {
                    self.right_panel_open = false;
                    self.sync_panel_layout(widgets);
                } else {
                    let _ = sender.input_sender().send(PdfReaderMsg::OpenRightPanel);
                }
            }
            PdfReaderMsg::ClosePanels => {
                // The spent page click (decision 52): floating
                // panels go away, pinned layout stays.
                if !self.left_panel_pinned {
                    self.left_panel_open = false;
                }
                if !self.right_panel_pinned {
                    self.right_panel_open = false;
                }
                self.sync_panel_layout(widgets);
            }
            PdfReaderMsg::ToggleLeftPin => {
                self.left_panel_pinned = !self.left_panel_pinned;
                self.catalog.set_pref_i64(
                    "reader.pdf.left_panel_pinned",
                    i64::from(self.left_panel_pinned),
                );
                if self.left_panel_pinned {
                    self.left_panel_open = true;
                    self.left_close_seq = self.left_close_seq.wrapping_add(1);
                } else if !self.mouse_in_left_panel && !self.mouse_in_left_edge {
                    self.schedule_left_close(&sender);
                }
                self.sync_panel_layout(widgets);
            }
            PdfReaderMsg::ToggleRightPin => {
                self.right_panel_pinned = !self.right_panel_pinned;
                self.catalog.set_pref_i64(
                    "reader.pdf.right_panel_pinned",
                    i64::from(self.right_panel_pinned),
                );
                if self.right_panel_pinned {
                    self.right_panel_open = true;
                    self.right_close_seq = self.right_close_seq.wrapping_add(1);
                } else if !self.mouse_in_right_panel && !self.mouse_in_right_edge {
                    self.schedule_right_close(&sender);
                }
                self.sync_panel_layout(widgets);
            }
            PdfReaderMsg::TocFilterChanged(filter) => {
                if self.toc_filter != filter {
                    self.toc_filter = filter;
                    self.refresh_left_panel(widgets, &sender);
                }
            }
            PdfReaderMsg::ToggleTocEntry(index) => {
                if !self.collapsed_toc_entries.remove(&index) {
                    self.collapsed_toc_entries.insert(index);
                }
                self.refresh_left_panel(widgets, &sender);
            }
            PdfReaderMsg::OpenJump(text) => {
                if self.jump_mode == PdfJumpMode::Chapter && self.jump_max() == 0 {
                    self.jump_mode = PdfJumpMode::Page;
                }
                self.jump_open = true;
                self.jump_text = text.unwrap_or_else(|| self.jump_current().to_string());
                widgets.jump_entry.set_text(&self.jump_text);
                self.sync_jump_mode_control();
                widgets.jump_entry.grab_focus();
                widgets.jump_entry.select_region(0, -1);
            }
            PdfReaderMsg::CloseJump => {
                self.jump_open = false;
                _root.grab_focus();
            }
            PdfReaderMsg::SetJumpMode(mode) => {
                let mode = if mode == PdfJumpMode::Chapter && self.jump_max() == 0 {
                    PdfJumpMode::Page
                } else {
                    mode
                };
                if self.jump_mode != mode {
                    self.jump_mode = mode;
                    self.jump_text = self.jump_current().to_string();
                    widgets.jump_entry.set_text(&self.jump_text);
                    widgets.jump_entry.select_region(0, -1);
                }
                self.sync_jump_mode_control();
            }
            PdfReaderMsg::JumpTextChanged(text) => {
                self.jump_text = text;
            }
            PdfReaderMsg::SubmitJump => {
                if let Some(page) = self.jump_target_page() {
                    self.jump_open = false;
                    let _ = sender.input_sender().send(PdfReaderMsg::JumpToPage(page));
                    _root.grab_focus();
                }
            }
            PdfReaderMsg::ShowBackChrome => {
                self.back_hover = true;
                self.back_gen = self.back_gen.wrapping_add(1);
                self.show_back_button = true;
            }
            PdfReaderMsg::BackZoneHover(hovering) => {
                if !hovering && !self.back_hover {
                    // Already accounted away (a focus reveal never
                    // entered the zone) — nothing to schedule.
                    return;
                }
                self.back_hover = hovering;
                self.back_gen = self.back_gen.wrapping_add(1);
                let gen = self.back_gen;
                let tx = sender.input_sender().clone();
                if hovering {
                    // The 250 ms rest before the unit drops in.
                    glib::timeout_add_local_once(Duration::from_millis(250), move || {
                        let _ = tx.send(PdfReaderMsg::BackRevealDue(gen));
                    });
                } else {
                    if let Some(unit) = &self.back_unit {
                        unit.hide();
                    }
                    // The 300 ms grace, then the whole unit goes.
                    glib::timeout_add_local_once(Duration::from_millis(300), move || {
                        let _ = tx.send(PdfReaderMsg::BackHideDue(gen));
                    });
                }
            }
            PdfReaderMsg::BackRevealDue(gen) => {
                if gen == self.back_gen && self.back_hover && !self.left_panel_open {
                    self.show_back_button = true;
                }
            }
            PdfReaderMsg::BackHideDue(gen) => {
                if gen == self.back_gen && !self.back_hover {
                    self.show_back_button = false;
                }
            }
            PdfReaderMsg::PageRendered {
                generation,
                page,
                texture,
                width,
                height,
            } => {
                if generation != self.render_generation {
                    return;
                }
                self.pending_loads.remove(&page);
                self.textures.insert(
                    page,
                    CachedPageTexture {
                        texture: texture.clone(),
                        width,
                        height,
                        generation,
                    },
                );

                // Auto-detect if scanned page needs OCR
                if !self.page_text_cache.contains_key(&page) {
                    let _ = self.ensure_page_text(page);
                }

                match self.scroll_mode {
                    PdfScrollMode::PageScrolling => {
                        match self.spread_mode {
                            PdfSpreadMode::NoSpreads => {
                                if page == self.current_page {
                                    self.update_paged_view();
                                }
                            }
                            _ => {
                                let (left, right) = self.spread_for_page(self.current_page);
                                if page == left || right == Some(page) {
                                    self.update_two_page_view();
                                }
                            }
                        }
                    }
                    PdfScrollMode::WrappedScrolling => {
                        if let Some(pic) = self.page_pictures.get(&page) {
                            pic.set_texture(Some(&texture));
                            let target_w = (self.base_page_width * 0.6 * self.zoom_level) as i32;
                            let target_h = (self.base_page_height * 0.6 * self.zoom_level) as i32;
                            pic.set_size_request(target_w, target_h);
                            pic.remove_css_class("kalam-pdf-placeholder");
                            pic.add_css_class("kalam-pdf-page-image");
                            pic.queue_resize();
                            pic.queue_draw();
                            widgets.viewport_scroll.queue_draw();
                        }
                    }
                    _ => {
                        match self.spread_mode {
                            PdfSpreadMode::NoSpreads => {
                                if let Some(pic) = self.page_pictures.get(&page) {
                                    pic.set_texture(Some(&texture));
                                    let target_w = (self.base_page_width * self.zoom_level) as i32;
                                    let target_h = (self.base_page_height * self.zoom_level) as i32;
                                    pic.set_size_request(target_w, target_h);
                                    pic.remove_css_class("kalam-pdf-placeholder");
                                    pic.add_css_class("kalam-pdf-page-image");
                                    pic.queue_resize();
                                    pic.queue_draw();
                                    widgets.viewport_scroll.queue_draw();
                                }
                            }
                            _ => {
                                let (left, right) = self.spread_for_page(page);
                                let left_cached = self.textures.get(&left);
                                let right_cached = right.and_then(|r| self.textures.get(&r));

                                let spread_ready = if right.is_some() {
                                    left_cached.is_some() && right_cached.is_some()
                                } else {
                                    left_cached.is_some()
                                };

                                if spread_ready {
                                    let target_w = (self.base_page_width * self.zoom_level) as i32;
                                    let target_h = (self.base_page_height * self.zoom_level) as i32;

                                    if let Some(left_tex) = left_cached {
                                        if let Some(pic) = self.page_pictures.get(&left) {
                                            pic.set_texture(Some(&left_tex.texture));
                                            pic.set_size_request(target_w, target_h);
                                            pic.remove_css_class("kalam-pdf-placeholder");
                                            pic.add_css_class("kalam-pdf-page-image");
                                            pic.queue_resize();
                                            pic.queue_draw();
                                        }
                                    }

                                    if let Some(r) = right {
                                        if let Some(right_tex) = right_cached {
                                            if let Some(pic) = self.page_pictures.get(&r) {
                                                pic.set_texture(Some(&right_tex.texture));
                                                pic.set_size_request(target_w, target_h);
                                                pic.remove_css_class("kalam-pdf-placeholder");
                                                pic.add_css_class("kalam-pdf-page-image");
                                                pic.queue_resize();
                                                pic.queue_draw();
                                            }
                                        }
                                    }

                                    widgets.viewport_scroll.queue_draw();
                                }
                            }
                        }
                    }
                }
            }
            PdfReaderMsg::PageThumbnailVisible(page) => {
                self.request_page_thumbnail(page);
            }
            PdfReaderMsg::PageThumbnailReady {
                generation,
                page,
                texture,
            } => {
                if generation == self.thumbnail_generation {
                    self.thumbnail_pending.remove(&page);
                    if let Some(grid) = &self.pages_grid {
                        grid.update_thumbnail(page, texture);
                    }
                }
            }
            PdfReaderMsg::PagesGridScrolled(_) => {
                if self.left_tab == PdfLeftTab::Pages {
                    self.update_pages_heading();
                }
            }
            PdfReaderMsg::DocumentLoaded { info } => {
                crate::timing::span_end("pdf_open_total");
                match *info {
                    Ok(info) => {
                        self.total_pages = info.total_pages;
                        self.toc_entries = info.toc_entries;
                        self.page_labels = info.page_labels;
                        self.source_page_width = info.base_page_width;
                        self.source_page_height = info.base_page_height;
                        self.sync_display_page_dimensions();
                        if self.current_page > self.total_pages {
                            self.current_page = 1;
                        }
                        self.is_loading = false;
                        self.open_state = PdfOpenState::Ready;
                        self.open_state_removing = false;
                        self.password_has_text = false;
                        widgets.password_entry.set_text("");
                        _root.grab_focus();
                        self.doc_ready = true;

                        // Stable page identities before any page is
                        // wrapped: saved drawings anchor to them, and
                        // the first viewport paints them already.
                        self.ensure_page_identities();

                        self.expand_current_toc_branch();
                        self.refresh_left_panel(widgets, &sender);
                        self.sync_jump_mode_control();

                        // With real page geometry the default layout
                        // and fit can finally be judged; both settle
                        // before the build below so the first real
                        // frame is already the right shape.
                        if self.layout_auto {
                            self.spread_mode = self.desired_auto_spread_mode(&widgets.viewport_scroll);
                        }
                        self.refresh_fit_zoom(&widgets.viewport_scroll);

                        // Rebuild the viewport now that the real page count
                        // and dimensions are known (the widget built during
                        // init is a skeleton of placeholders).
                        let child = self.build_viewport_widget(&sender);
                        self.refresh_drawing_paints();
                        widgets.viewport_scroll.set_child(Some(&child));
                        self.update_scroll_policies(&widgets.viewport_scroll);
                        widgets.viewport_scroll.queue_draw();

                        // Restore the reading position in continuous mode.
                        // This needs the real total_pages, so it lives here
                        // instead of init.
                        if self.scroll_mode != PdfScrollMode::PageScrolling
                            && self.current_page > 1
                            && self.total_pages > 1
                        {
                            let cur = self.current_page;
                            let total = self.total_pages;
                            let vadj = widgets.viewport_scroll.vadjustment();
                            glib::idle_add_local_once(move || {
                                let max = (vadj.upper() - vadj.page_size()).max(0.0);
                                if max > 0.0 {
                                    let ratio =
                                        (cur.saturating_sub(1)) as f64 / (total - 1) as f64;
                                    vadj.set_value((ratio * max).clamp(vadj.lower(), max));
                                }
                            });
                        }

                        let cur = self.current_page;
                        let _ = self.ensure_page_text(cur);
                        self.trigger_loads(&sender);
                        self.sync_pages_grid_document();
                        self.apply_screenshot_action(&sender);
                    }
                    Err(err) => {
                        self.is_loading = false;
                        self.doc_ready = false;
                        self.open_state = PdfOpenState::from_load_error(err);
                        self.open_state_removing = false;
                        set_pdf_session_password(&self.session_password, None);
                        if self.open_state.is_password() {
                            self.password_has_text = !widgets.password_entry.text().is_empty();
                            if err == PdfLoadError::PasswordRequired {
                                self.apply_password_screenshot_action(widgets, &sender);
                            }
                            let entry = widgets.password_entry.clone();
                            let select_all = err == PdfLoadError::WrongPassword;
                            glib::idle_add_local_once(move || {
                                entry.grab_focus();
                                if select_all {
                                    entry.select_region(0, -1);
                                }
                            });
                        }
                        log::warn!("PDF open failed: {}", err.log_message());
                    }
                }
            }
            PdfReaderMsg::PageTextReady {
                page,
                fingerprint,
                text,
            } => {
                self.text_in_progress.remove(&page);
                match text {
                    Ok(page_text) => {
                        if page_text.total_chars() > 0 {
                            self.page_text_cache.insert(page, page_text.clone());
                            // The text layer for this page may now hide the
                            // selection area; repaint without rebuilding.
                            if let Some(pic) = self.page_pictures.get(&page) {
                                pic.queue_draw();
                            }
                            widgets.viewport_scroll.queue_draw();
                            // A click stored while this page's text was
                            // missing can now run (2.20 step 3).
                            self.try_replay_pending_selection(page, &sender);
                        } else {
                            // No embedded text. Landing on a page whose text
                            // is not ready promotes it and its window to the
                            // front of the import-time OCR queue (2.20 step
                            // 4) -- including a jump straight to page 5000;
                            // the background scan continues after them. The
                            // page the reader stands on is also served by its
                            // own OCR worker, because that path drives the
                            // pending-selection signal; the queue skips it
                            // later through the page cache.
                            let image_window = match self.scroll_mode {
                                PdfScrollMode::PageScrolling => 1,
                                _ => 3,
                            };
                            let mut window: Vec<usize> = Self::keep_window_pages(
                                self.spread_mode,
                                self.scroll_mode,
                                self.total_pages,
                                self.current_page,
                                image_window,
                            )
                            .into_iter()
                            .collect();
                            // Reading order: keep_window_pages returns a
                            // set, and the queue runs its window in the
                            // order given.
                            window.sort_unstable();
                            crate::pdf_ocr::promote(self.book_id, &window);
                            self.trigger_page_ocr(page, fingerprint);
                        }
                    }
                    Err(err) => {
                        log::warn!("Text extraction failed for page {page}: {err}");
                        // Extraction failed: the pending click waits for
                        // nothing. End it now.
                        self.pending_selection = None;
                        self.dismiss_recognizing_popover();
                    }
                }
            }
            PdfReaderMsg::PageOcrResult {
                generation,
                page,
                fingerprint,
                text,
            } => {
                if generation != self.render_generation {
                    return;
                }
                self.ocr_in_progress.remove(&page);
                // The finished text belongs to the file as it was when OCR
                // started; persist it only if the file is still unchanged
                // (the worker re-fingerprinted it after recognition), so a
                // replaced mid-OCR file can never poison the cache.
                let started_fp = self.ocr_started_fp.remove(&page);
                let file_unchanged = match (&started_fp, &fingerprint) {
                    (Some(started), Some(current)) => started == current,
                    _ => false,
                };
                match *text {
                    Ok(ocr_text) => {
                        log::info!(
                            "OCR completed for page {page}: {} lines recognized",
                            ocr_text.lines.len()
                        );
                        if let Some(started) = started_fp {
                            if file_unchanged {
                                if let Err(err) = self
                                    .catalog
                                    .save_page_ocr(self.book_id, page, &started, &ocr_text)
                                {
                                    log::warn!(
                                        "Could not cache OCR result for page {page}: {err}"
                                    );
                                }
                            }
                        }
                        self.page_text_cache.insert(page, ocr_text);
                        self.try_replay_pending_selection(page, &sender);
                        for da in self.page_draw_areas.values() {
                            da.queue_draw();
                        }
                    }
                    Err(err) => {
                        log::warn!("OCR failed for page {page}: {err}");
                        // The text this pending click waits for will never
                        // arrive; end the signal now rather than through
                        // its timeout.
                        self.pending_selection = None;
                        self.dismiss_recognizing_popover();
                    }
                }
            }
            PdfReaderMsg::LeftEdgeHover(hovering) => {
                self.mouse_in_left_edge = hovering;
                if hovering {
                    let _ = sender.input_sender().send(PdfReaderMsg::OpenLeftPanel);
                } else if self.left_panel_open
                    && !self.left_panel_pinned
                    && !self.mouse_in_left_panel
                {
                    self.schedule_left_close(&sender);
                }
            }
            PdfReaderMsg::RightEdgeHover(hovering) => {
                self.mouse_in_right_edge = hovering;
                if hovering {
                    let _ = sender.input_sender().send(PdfReaderMsg::OpenRightPanel);
                } else if self.right_panel_open
                    && !self.right_panel_pinned
                    && !self.mouse_in_right_panel
                {
                    self.schedule_right_close(&sender);
                }
            }
            PdfReaderMsg::LeftPanelHover(hovering) => {
                self.mouse_in_left_panel = hovering;
                if hovering {
                    self.left_close_seq = self.left_close_seq.wrapping_add(1);
                } else if self.left_panel_open
                    && !self.left_panel_pinned
                    && !self.mouse_in_left_edge
                {
                    self.schedule_left_close(&sender);
                }
            }
            PdfReaderMsg::RightPanelHover(hovering) => {
                self.mouse_in_right_panel = hovering;
                if hovering {
                    self.right_close_seq = self.right_close_seq.wrapping_add(1);
                } else if self.right_panel_open
                    && !self.right_panel_pinned
                    && !self.mouse_in_right_edge
                {
                    self.schedule_right_close(&sender);
                }
            }
            PdfReaderMsg::LeftCloseTimerTick(seq) => {
                if !self.left_panel_pinned
                    && seq == self.left_close_seq
                    && !self.mouse_in_left_panel
                    && !self.mouse_in_left_edge
                {
                    self.left_panel_open = false;
                    self.sync_panel_layout(widgets);
                }
            }
            PdfReaderMsg::RightCloseTimerTick(seq) => {
                if !self.right_panel_pinned
                    && seq == self.right_close_seq
                    && !self.mouse_in_right_panel
                    && !self.mouse_in_right_edge
                {
                    self.right_panel_open = false;
                    self.sync_panel_layout(widgets);
                }
            }
            PdfReaderMsg::UserScrolled => {
                if self.show_back_button && !self.back_hover {
                    self.show_back_button = false;
                    self.back_gen = self.back_gen.wrapping_add(1);
                }
            }
            PdfReaderMsg::ScrollDelta(dir) => {
                if self.show_back_button && !self.back_hover {
                    self.show_back_button = false;
                    self.back_gen = self.back_gen.wrapping_add(1);
                }
                if self.scroll_mode == PdfScrollMode::HorizontalScrolling {
                    let hadj = widgets.viewport_scroll.hadjustment();
                    let step = (self.arrow_step as f64) * dir;
                    let target = (hadj.value() + step)
                        .clamp(hadj.lower(), (hadj.upper() - hadj.page_size()).max(0.0));
                    hadj.set_value(target);
                } else if self.scroll_mode != PdfScrollMode::PageScrolling {
                    let vadj = widgets.viewport_scroll.vadjustment();
                    let step = (self.arrow_step as f64) * dir;
                    let target = (vadj.value() + step)
                        .clamp(vadj.lower(), (vadj.upper() - vadj.page_size()).max(0.0));
                    vadj.set_value(target);
                } else if dir > 0.0 {
                    let _ = sender.input_sender().send(PdfReaderMsg::NextPage);
                } else {
                    let _ = sender.input_sender().send(PdfReaderMsg::PrevPage);
                }
            }
            PdfReaderMsg::UpdateScrollPage(_ratio_page) => {
                if self.scroll_mode == PdfScrollMode::PageScrolling || self.total_pages <= 1 {
                    return;
                }

                let (pos, max) = if self.scroll_mode == PdfScrollMode::HorizontalScrolling {
                    let hadj = widgets.viewport_scroll.hadjustment();
                    (hadj.value(), (hadj.upper() - hadj.page_size()).max(1.0))
                } else {
                    let vadj = widgets.viewport_scroll.vadjustment();
                    (vadj.value(), (vadj.upper() - vadj.page_size()).max(1.0))
                };

                let ratio = (pos / max).clamp(0.0, 1.0);
                let target = 1 + (ratio * (self.total_pages - 1) as f64).round() as usize;
                if target != self.current_page && target <= self.total_pages {
                    self.current_page = target;
                    self.save_progress();
                    self.prune_textures();
                    self.trigger_loads(&sender);
                    self.update_bookmark_icon_state(widgets);
                }
            }
            PdfReaderMsg::SelectionDragBegin { slot, x, y, is_block } => {
                let Some(page) = self.page_for_slot(slot) else { return };
                self.dismiss_dict_popover();
                if !self.draw_tool.is_select() {
                    self.begin_active_drawing(slot, x, y);
                    return;
                }
                if !is_block && self.begin_drawing_selection(slot, x, y) {
                    return;
                }
                self.dismiss_selection_chip();
                let _ = self.ensure_page_text(page);
                if !self.page_text_cache.contains_key(&page) {
                    // Text not ready yet: remember the click instead of
                    // starting a drag that cannot select anything (2.20
                    // step 3). The drag itself is not started.
                    self.store_pending_selection(
                        page,
                        PdfPendingSelection::Drag { slot, x, y },
                        slot,
                        x,
                        y,
                    );
                    return;
                }

                // A press on one of the selection's handle grips takes
                // hold of that end: the drag moves it and leaves the other
                // end where it is. The end grip is tested first, so when
                // both overlap -- a selection one glyph wide -- the end
                // wins, the one a reader who just dragged rightwards is
                // reaching for (the EPUB reader's rule).
                let adjusting = if is_block {
                    None
                } else {
                    self.active_selection
                        .borrow()
                        .as_ref()
                        .filter(|sel| sel.slot == slot)
                        .and_then(|sel| selection_handle_at(sel, x, y))
                };

                self.handle_dragging.set(adjusting.is_some());
                if adjusting.is_some() {
                    if let Some(ov) = self.page_overlays.get(&slot) {
                        ov.set_cursor_from_name(Some("grabbing"));
                    }
                }
                self.selection_drag_state = Some((slot, x, y, adjusting, is_block));
            }
            PdfReaderMsg::SelectionDragUpdate { slot, dx, dy, is_block } => {
                if !self.draw_tool.is_select() {
                    self.update_active_drawing(slot, dx, dy);
                    return;
                }
                if self.update_drawing_move(slot, dx, dy) {
                    return;
                }
                let Some((drag_slot, start_x, start_y, adjusting, _)) = self.selection_drag_state else {
                    return;
                };
                if drag_slot != slot {
                    return;
                }
                let Some(page) = self.page_for_slot(slot) else { return };

                let (target_w, target_h) = if self.scroll_mode == PdfScrollMode::WrappedScrolling {
                    (
                        self.base_page_width * 0.6 * self.zoom_level,
                        self.base_page_height * 0.6 * self.zoom_level,
                    )
                } else {
                    (
                        self.base_page_width * self.zoom_level,
                        self.base_page_height * self.zoom_level,
                    )
                };

                let curr_x = (start_x + dx).clamp(0.0, target_w);
                let curr_y = (start_y + dy).clamp(0.0, target_h);

                let Some(page_text) = self.page_text_cache.get(&page) else {
                    return;
                };
                if page_text.width_pts <= 0.0 || page_text.height_pts <= 0.0 {
                    return;
                }

                let start_pt = self.display_point_to_page(start_x, start_y, target_w, target_h, page_text);
                let curr_pt = self.display_point_to_page(curr_x, curr_y, target_w, target_h, page_text);
                // A handle drag moves that end and leaves the other where
                // it is. Alt mid-drag turns the same press into a block
                // marquee, so the grip is only honoured while it is not a
                // block selection (the old behaviour, kept).
                let adjusting = if is_block { None } else { adjusting };
                let (p0, p1) = match adjusting {
                    Some(PdfHandleDrag::End) => {
                        if let Some(ref sel) = *self.active_selection.borrow() {
                            (sel.anchor_pt, curr_pt)
                        } else {
                            (start_pt, curr_pt)
                        }
                    }
                    Some(PdfHandleDrag::Start) => {
                        if let Some(ref sel) = *self.active_selection.borrow() {
                            (curr_pt, sel.active_pt)
                        } else {
                            (start_pt, curr_pt)
                        }
                    }
                    None => (start_pt, curr_pt),
                };

                let (selected_text, highlight_rects) = if is_block {
                    page_text.select_rect(p0, p1)
                } else {
                    page_text.select_between(p0, p1)
                };

                if highlight_rects.is_empty() {
                    // The chip must never outlive its selection (the
                    // owner's re-test: an action dialog with no
                    // selection behind it).
                    self.active_selection.replace(None);
                    self.dismiss_selection_chip();
                } else {
                    let screen_rects = self.page_rects_to_display(
                        &highlight_rects,
                        target_w,
                        target_h,
                        page_text,
                    );
                    let mut min_x = f64::MAX;
                    let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN;
                    let mut max_y = f64::MIN;

                    for &(rx, ry, rw, rh) in &screen_rects {
                        min_x = min_x.min(rx);
                        min_y = min_y.min(ry);
                        max_x = max_x.max(rx + rw);
                        max_y = max_y.max(ry + rh);
                    }

                    let first = screen_rects.first().copied().unwrap_or((0.0, 0.0, 0.0, 0.0));
                    let last = screen_rects.last().copied().unwrap_or((0.0, 0.0, 0.0, 0.0));

                    let (start_handle, end_handle) = self.selection_handles(first, last);
                    let bounds = (min_x, min_y, (max_x - min_x).max(1.0), (max_y - min_y).max(1.0));

                    self.active_selection.replace(Some(PdfActiveSelection {
                        page,
                        slot,
                        text: selected_text,
                        screen_rects,
                        bounds,
                        start_handle,
                        end_handle,
                        anchor_pt: p0,
                        active_pt: p1,
                        is_block,
                        rotation: self.rotation_degrees,
                    }));
                }

                if let Some(da) = self.page_draw_areas.get(&slot) {
                    da.queue_draw();
                }
            }
            PdfReaderMsg::SelectionDragEnd { slot, dx, dy, press_count } => {
                if !self.draw_tool.is_select() {
                    self.finish_active_drawing(slot, dx, dy);
                    return;
                }
                if press_count < 2 && self.finish_drawing_move(slot, dx, dy, &sender) {
                    return;
                }
                let drag = self.selection_drag_state.take();
                self.handle_dragging.set(false);
                // The release of a multi-click's second or third press:
                // the click gesture made the selection, and this release
                // must not touch it. Gate on the live press count read at
                // release time, not on whether a drag state exists -- a
                // press whose own release never arrived can leave one
                // behind, and consuming it here must still not destroy
                // what the click selected (the EPUB reader gates its
                // drag-end the same way). The owner's re-test: the word
                // a double-click selected vanished while its chip stayed;
                // the triple-click's line survived only because release
                // two had already consumed the stale state.
                if press_count >= 2 {
                    if let Some((_, press_x, press_y, _, _)) = drag {
                        self.park_cursor_at(slot, press_x + dx, press_y + dy);
                    }
                    return;
                }
                let Some((_, press_x, press_y, adjusting, _)) = drag else {
                    return;
                };
                if dx.abs() > 4.0 || dy.abs() > 4.0 {
                    if self.active_selection.borrow().is_some() {
                        self.show_selection_chip(slot, &sender);
                    }
                } else if adjusting.is_some() {
                    // A handle grip taken and released without moving keeps
                    // the selection (and its toolbar), like the EPUB
                    // reader.
                    if self.active_selection.borrow().is_some() {
                        self.show_selection_chip(slot, &sender);
                    }
                } else if self.active_selection.borrow().is_some() {
                    self.clear_selection();
                }
                // The pointer rests where the drop left it, and no motion
                // event will arrive until it moves again. Park the cursor
                // on exactly what the motion controller would show at
                // that point.
                self.park_cursor_at(slot, press_x + dx, press_y + dy);
            }
            PdfReaderMsg::DrawingDragBegin { slot, x, y } => {
                self.dismiss_dict_popover();
                self.begin_active_drawing(slot, x, y);
            }
            PdfReaderMsg::DrawingDragUpdate { slot, dx, dy } => {
                self.update_active_drawing(slot, dx, dy);
            }
            PdfReaderMsg::DrawingDragEnd { slot, dx, dy, .. } => {
                self.finish_active_drawing(slot, dx, dy);
            }
            PdfReaderMsg::SelectionWordAt { slot, x, y } => {
                let Some(page) = self.page_for_slot(slot) else { return };
                let _ = self.ensure_page_text(page);

                let (target_w, target_h) = if self.scroll_mode == PdfScrollMode::WrappedScrolling {
                    (
                        self.base_page_width * 0.6 * self.zoom_level,
                        self.base_page_height * 0.6 * self.zoom_level,
                    )
                } else {
                    (
                        self.base_page_width * self.zoom_level,
                        self.base_page_height * self.zoom_level,
                    )
                };

                let Some(page_text) = self.page_text_cache.get(&page) else {
                    // First click on a page whose text is not ready: store
                    // the intent and answer with the recognizing signal
                    // (2.20 step 3).
                    self.store_pending_selection(
                        page,
                        PdfPendingSelection::Word { slot, x, y },
                        slot,
                        x,
                        y,
                    );
                    return;
                };
                if page_text.width_pts <= 0.0 || page_text.height_pts <= 0.0 { return }

                let pt = self.display_point_to_page(x, y, target_w, target_h, page_text);
                if let Some((word_text, highlight_rects)) = page_text.word_at(pt) {
                    let screen_rects = self.page_rects_to_display(
                        &highlight_rects,
                        target_w,
                        target_h,
                        page_text,
                    );
                    let mut min_x = f64::MAX;
                    let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN;
                    let mut max_y = f64::MIN;

                    for &(rx, ry, rw, rh) in &screen_rects {
                        min_x = min_x.min(rx);
                        min_y = min_y.min(ry);
                        max_x = max_x.max(rx + rw);
                        max_y = max_y.max(ry + rh);
                    }

                    let first = screen_rects.first().copied().unwrap_or((0.0, 0.0, 0.0, 0.0));
                    let last = screen_rects.last().copied().unwrap_or((0.0, 0.0, 0.0, 0.0));

                    // The ends a later handle-drag takes hold of are the
                    // selection's own ends, never the click point that
                    // made it: anchor and active are the first and last
                    // rect's edges, so dragging the end handle extends
                    // the selection instead of restarting it from the
                    // click (the owner's re-test: "the end handle became
                    // the starting handle").
                    let first_pt = highlight_rects.first().copied().unwrap_or((0.0, 0.0, 0.0, 0.0));
                    let last_pt = highlight_rects.last().copied().unwrap_or((0.0, 0.0, 0.0, 0.0));
                    let anchor_pt = (first_pt.0, (first_pt.1 + first_pt.3) * 0.5);
                    let active_pt = (last_pt.2, (last_pt.1 + last_pt.3) * 0.5);

                    let (start_handle, end_handle) = self.selection_handles(first, last);
                    let bounds = (min_x, min_y, (max_x - min_x).max(1.0), (max_y - min_y).max(1.0));

                    self.active_selection.replace(Some(PdfActiveSelection {
                        page,
                        slot,
                        text: word_text,
                        screen_rects,
                        bounds,
                        start_handle,
                        end_handle,
                        anchor_pt,
                        active_pt,
                        is_block: false,
                        rotation: self.rotation_degrees,
                    }));

                    if let Some(da) = self.page_draw_areas.get(&slot) {
                        da.queue_draw();
                    }
                    self.show_selection_chip(slot, &sender);
                } else {
                    // A double-click that found no word is the
                    // zoom gesture the design asks for (fit ↔
                    // actual size); one that found a word stays a
                    // selection, as it always has in this reader.
                    let _ = sender.input_sender().send(PdfReaderMsg::ToggleFitOriginal);
                }
            }
            PdfReaderMsg::SelectionLineAt { slot, x, y } => {
                let Some(page) = self.page_for_slot(slot) else { return };
                let _ = self.ensure_page_text(page);

                let (target_w, target_h) = if self.scroll_mode == PdfScrollMode::WrappedScrolling {
                    (
                        self.base_page_width * 0.6 * self.zoom_level,
                        self.base_page_height * 0.6 * self.zoom_level,
                    )
                } else {
                    (
                        self.base_page_width * self.zoom_level,
                        self.base_page_height * self.zoom_level,
                    )
                };

                let Some(page_text) = self.page_text_cache.get(&page) else {
                    self.store_pending_selection(
                        page,
                        PdfPendingSelection::Line { slot, x, y },
                        slot,
                        x,
                        y,
                    );
                    return;
                };
                if page_text.width_pts <= 0.0 || page_text.height_pts <= 0.0 { return }

                let pt = self.display_point_to_page(x, y, target_w, target_h, page_text);
                if let Some((line_text, highlight_rects)) = page_text.line_at(pt) {
                    let screen_rects = self.page_rects_to_display(
                        &highlight_rects,
                        target_w,
                        target_h,
                        page_text,
                    );
                    let mut min_x = f64::MAX;
                    let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN;
                    let mut max_y = f64::MIN;

                    for &(rx, ry, rw, rh) in &screen_rects {
                        min_x = min_x.min(rx);
                        min_y = min_y.min(ry);
                        max_x = max_x.max(rx + rw);
                        max_y = max_y.max(ry + rh);
                    }

                    let first = screen_rects.first().copied().unwrap_or((0.0, 0.0, 0.0, 0.0));
                    let last = screen_rects.last().copied().unwrap_or((0.0, 0.0, 0.0, 0.0));

                    // Same as the word selection: the handle-drag anchors
                    // are the selection's ends, not the click point.
                    let first_pt = highlight_rects.first().copied().unwrap_or((0.0, 0.0, 0.0, 0.0));
                    let last_pt = highlight_rects.last().copied().unwrap_or((0.0, 0.0, 0.0, 0.0));
                    let anchor_pt = (first_pt.0, (first_pt.1 + first_pt.3) * 0.5);
                    let active_pt = (last_pt.2, (last_pt.1 + last_pt.3) * 0.5);

                    let (start_handle, end_handle) = self.selection_handles(first, last);
                    let bounds = (min_x, min_y, (max_x - min_x).max(1.0), (max_y - min_y).max(1.0));

                    self.active_selection.replace(Some(PdfActiveSelection {
                        page,
                        slot,
                        text: line_text,
                        screen_rects,
                        bounds,
                        start_handle,
                        end_handle,
                        anchor_pt,
                        active_pt,
                        is_block: false,
                        rotation: self.rotation_degrees,
                    }));

                    if let Some(da) = self.page_draw_areas.get(&slot) {
                        da.queue_draw();
                    }
                    self.show_selection_chip(slot, &sender);
                }
            }
            PdfReaderMsg::CopySelection => {
                let text_opt = self.active_selection.borrow().as_ref().map(|s| s.text.clone());
                if let Some(text) = text_opt {
                    if let Some(display) = gdk::Display::default() {
                        display.clipboard().set_text(&text);
                    }
                    let preview = if text.len() > 36 {
                        format!("{}...", &text[..36])
                    } else {
                        text
                    };
                    crate::notify::info("Copied to clipboard", &preview);
                }
                self.clear_selection();
            }
            PdfReaderMsg::SaveAnnotation { color, style, note } => {
                let text_opt = self.active_selection.borrow().as_ref().map(|s| s.text.clone());
                if let Some(text) = text_opt {
                    let page_num = self.active_selection.borrow().as_ref().map(|s| s.page).unwrap_or(self.current_page);
                    let _ = self.catalog.insert_annotation(
                        self.book_id,
                        "highlight",
                        page_num as i64,
                        "",
                        0,
                        "",
                        0,
                        &color,
                        &style,
                        &text,
                        &note,
                    );
                    crate::notify::info("Highlight saved", "");
                    self.reload_annotations();
                    self.refresh_notes_panel(&sender);
                }
                self.clear_selection();
            }
            PdfReaderMsg::QuoteSelection => {
                let text_opt = self.active_selection.borrow().as_ref().map(|s| s.text.clone());
                if let Some(text) = text_opt {
                    let page_num = self.active_selection.borrow().as_ref().map(|s| s.page).unwrap_or(self.current_page);
                    let _ = self.catalog.insert_annotation(
                        self.book_id,
                        "quote",
                        page_num as i64,
                        "",
                        0,
                        "",
                        0,
                        "yellow",
                        "solid",
                        &text,
                        "",
                    );
                    let preview = if text.len() > 36 {
                        format!("{}...", &text[..36])
                    } else {
                        text
                    };
                    crate::notify::info("Quote saved", &preview);
                    self.reload_annotations();
                    self.refresh_notes_panel(&sender);
                }
                self.clear_selection();
            }
            PdfReaderMsg::LookUpWord(word) => {
                self.show_dict_popover(word, &sender);
            }
            PdfReaderMsg::ToggleSaveWord => {
                let Some(word) = self.dict_lookup_word.clone() else {
                    return;
                };
                let saved = self
                    .catalog
                    .saved_word_exists(&word, self.book_id)
                    .unwrap_or(false);
                if saved {
                    let removed: Vec<SavedWord> = self
                        .saved_words
                        .iter()
                        .filter(|row| {
                            row.book_id == Some(self.book_id)
                                && row.word.eq_ignore_ascii_case(&word)
                        })
                        .cloned()
                        .collect();
                    if crate::notify::report(
                        self.catalog.delete_saved_word_for(&word, self.book_id),
                        "Could not forget the word",
                    ) {
                        for row in removed {
                            self.remember_deleted(PdfDeletedMark::SavedWord(Box::new(row)));
                        }
                        crate::notify::compact("Word forgotten", &word);
                    }
                } else if let Some(definition) = self.dict_lookup_def.clone() {
                    if crate::notify::report(
                        self.catalog.insert_saved_word(
                            &word,
                            &definition,
                            None,
                            Some(self.book_id),
                            Some(self.dict_lookup_page as i64),
                            self.dict_context.as_deref(),
                        ),
                        "Could not save the word",
                    ) {
                        crate::notify::compact("Word saved", &word);
                    }
                }
                self.reload_saved_words();
                self.refresh_notes_panel(&sender);
            }
            PdfReaderMsg::ClearDict => {
                self.dismiss_dict_popover();
            }
            PdfReaderMsg::ClearSelection => {
                self.clear_selection();
            }
            PdfReaderMsg::HideZoomOsd(seq) => {
                if seq == self.zoom_osd_seq {
                    self.show_zoom_osd = false;
                }
            }
            PdfReaderMsg::ViewportResized => {
                self.update_scroll_policies(&widgets.viewport_scroll);
                if self.doc_ready {
                    let auto_change = if self.layout_auto {
                        let desired = self.desired_auto_spread_mode(&widgets.viewport_scroll);
                        if desired != self.spread_mode {
                            let _ = sender
                                .input_sender()
                                .send(PdfReaderMsg::ApplyAutoSpreadMode(desired));
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    };
                    if !auto_change {
                        // A fit is a relationship to the viewport, not a
                        // scale frozen when it was chosen: a resized (or
                        // freshly inset, by a pinned panel) page area
                        // re-derives it, preserving the scroll centre the
                        // way any other zoom change does.
                        let old_zoom = self.zoom_level;
                        if self.refresh_fit_zoom(&widgets.viewport_scroll) {
                            self.apply_zoom_change(old_zoom, &sender, &widgets.viewport_scroll);
                            if self.search_active {
                                self.refresh_search_highlights();
                            }
                        }
                    }
                }
            }
            PdfReaderMsg::LayoutSettled => {
                // Fired from the adjustment's upper notify: the freshly
                // rebuilt viewport has now been allocated and its extent is
                // real. This is the moment the scroll back to the current
                // page can actually compute a correct position — running it
                // immediately after set_child (as this code used to) read
                // the old layout's adjustment, landed the view in the wrong
                // place, and left the pages on screen as placeholders until
                // the user scrolled (2026-10-05 field run #2). The scroll's
                // own value-changed re-anchors current_page and reloads; the
                // explicit trigger covers a scroll that changes nothing.
                if self.viewport_rebuild_pending {
                    self.viewport_rebuild_pending = false;
                    if self.scroll_mode != PdfScrollMode::PageScrolling && self.current_page > 1 {
                        self.scroll_to_current_page(&widgets.viewport_scroll);
                    }
                    self.trigger_loads(&sender);
                }
            }
        }

        self.sync_toc_current();
        self.sync_pages_current();
        // A page turn or a turned page changes which slots the
        // scoped matches can paint on; refreshing here keeps
        // every navigation honest without jumping back.
        self.refresh_search_highlights();
        if self.left_tab == PdfLeftTab::Pages {
            self.update_pages_heading();
        }
        if self.left_tab == PdfLeftTab::Notes
            && self.notes_filter == PdfNotesFilter::Words
            && self.word_scope == PdfWordScope::Chapter
            && self.notes_words_page_synced != self.current_page
        {
            self.refresh_notes_panel(&sender);
        }

        if let Some(ref sw) = self.settings_widgets {
            self.sync_settings_ui(sw);
        }
        if let Some(ref iw) = self.image_widgets {
            self.sync_image_ui(iw);
        }

        self.update_view(widgets, sender);
    }
}

impl PdfReaderModel {
    /// Put the page area and the pin buttons where the panel
    /// state says they belong. A pinned panel takes 16 + 340 + 16
    /// of its side and the viewport sits beside it (decision 57);
    /// a floating panel leaves the page where it was.
    pub fn sync_panel_layout(&self, widgets: &PdfReaderModelWidgets) {
        let left_inset = if self.left_panel_open && self.left_panel_pinned {
            372
        } else {
            0
        };
        let right_inset = if self.right_panel_open && self.right_panel_pinned {
            372
        } else {
            0
        };
        widgets.viewport_scroll.set_margin_start(left_inset);
        widgets.viewport_scroll.set_margin_end(right_inset);
        // The search pill centres over the same visible page
        // area the pinned panels leave behind (decision 61).
        widgets.search_revealer.set_margin_start(left_inset);
        widgets.search_revealer.set_margin_end(right_inset);

        for (button, pinned, side) in [
            (&widgets.left_pin_button, self.left_panel_pinned, "left"),
            (&widgets.right_pin_button, self.right_panel_pinned, "right"),
        ] {
            toggle_active(button, pinned);
            let label = if pinned {
                format!("Unpin the {side} panel")
            } else {
                format!("Pin the {side} panel")
            };
            button.set_tooltip_text(Some(&label));
            button.update_property(&[gtk::accessible::Property::Label(&label)]);
        }
    }

    pub fn refresh_left_panel(
        &mut self,
        widgets: &PdfReaderModelWidgets,
        sender: &ComponentSender<Self>,
    ) {
        if self.search_results_open {
            if let Some(stack) = &self.left_stack {
                stack.set_visible_child_name("search-results");
            }
            self.update_search_results_panel();
            self.sync_panel_layout(widgets);
            return;
        }
        if let Some(tabs) = &self.left_tabs {
            tabs.set_selected(match self.left_tab {
                PdfLeftTab::Contents => 0,
                PdfLeftTab::Pages => 1,
                PdfLeftTab::Notes => 2,
            });
        }
        if let Some(stack) = &self.left_stack {
            stack.set_visible_child_name(match self.left_tab {
                PdfLeftTab::Contents => "contents",
                PdfLeftTab::Pages => "pages",
                PdfLeftTab::Notes => "notes",
            });
        }
        match self.left_tab {
            PdfLeftTab::Contents => {
                if let Some(list) = &self.toc_list_box {
                    let (rows, current_visible) =
                        populate_toc_list(list, self, sender);
                    self.toc_rows = rows;
                    self.toc_current_visible_row = current_visible;
                    self.toc_current_page_synced = self.current_page;
                }
            }
            PdfLeftTab::Pages => {
                self.sync_pages_grid_document();
            }
            PdfLeftTab::Notes => {
                self.refresh_notes_panel(sender);
            }
        }
        self.sync_panel_layout(widgets);
    }

    pub fn refresh_right_panel(&self, widgets: &PdfReaderModelWidgets) {
        if let Some(tabs) = &self.right_tabs {
            tabs.set_selected(match self.right_tab {
                PdfRightTab::Reading => 0,
                PdfRightTab::Image => 1,
                PdfRightTab::Shortcuts => 2,
            });
        }
        if let Some(stack) = &self.right_stack {
            stack.set_visible_child_name(match self.right_tab {
                PdfRightTab::Reading => "reading",
                PdfRightTab::Image => "image",
                PdfRightTab::Shortcuts => "shortcuts",
            });
        }
        if self.right_tab == PdfRightTab::Reading {
            if let Some(sw) = &self.settings_widgets {
                self.sync_settings_ui(sw);
            }
        }
        if self.right_tab == PdfRightTab::Image {
            if let Some(iw) = &self.image_widgets {
                self.sync_image_ui(iw);
            }
        }
        self.sync_panel_layout(widgets);
    }

    pub fn sync_jump_mode_control(&self) {
        if let Some(control) = &self.jump_mode_control {
            control.set_selected(match self.jump_mode {
                PdfJumpMode::Page => 0,
                PdfJumpMode::Chapter => 1,
            });
            if let Some(chapter_button) = control.buttons().get(1) {
                // Chapter mode without an outline would be a range
                // with nothing in it; the switch says so by resting.
                chapter_button.set_sensitive(
                    !pdf_jump_chapter_pages(&self.toc_entries, self.total_pages).is_empty(),
                );
            }
        }
    }

    /// Bring the Contents current row into view when the panel
    /// opens. Rows are the spec's 44px, so the target is arithmetic
    /// rather than a hunt through allocations.
    pub fn scroll_toc_to_current(&self) {
        let (Some(scroll), Some(row)) = (&self.toc_scroll, self.toc_current_visible_row)
        else {
            return;
        };
        let adjustment = scroll.vadjustment();
        glib::idle_add_local_once(move || {
            let max = (adjustment.upper() - adjustment.page_size()).max(adjustment.lower());
            let target = (row as f64 * 44.0 - adjustment.page_size() / 2.0 + 22.0)
                .clamp(adjustment.lower(), max);
            adjustment.set_value(target);
        });
    }

    /// The password card's screenshot asks, applied once when
    /// the document service first asks for a password. The
    /// wrong and unlock frames go through the real submit path,
    /// so the photograph is evidence about MuPDF's answer, not
    /// a state staged behind its back.
    pub fn apply_password_screenshot_action(
        &mut self,
        widgets: &PdfReaderModelWidgets,
        sender: &ComponentSender<Self>,
    ) {
        if self.screenshot_password_action_applied {
            return;
        }
        self.screenshot_password_action_applied = true;
        match std::env::var("KALAM_PDF_ACTION").as_deref() {
            Ok("password") => {
                widgets.password_entry.set_text("kalam-password");
                self.password_has_text = true;
            }
            Ok("password-wrong") => {
                widgets.password_entry.set_text("wrong-password");
                self.password_has_text = true;
                sender.input(PdfReaderMsg::SubmitPassword);
            }
            Ok("password-unlock") => {
                widgets.password_entry.set_text("kalam");
                self.password_has_text = true;
                sender.input(PdfReaderMsg::SubmitPassword);
            }
            _ => {}
        }
    }

    /// The screenshot loop's ask (`KALAM_PDF_ACTION`), applied
    /// once the document facts exist — a frame of Contents or the
    /// jump field taken before `DocumentLoaded` would photograph
    /// the skeleton and call it the reader.
    pub fn apply_screenshot_action(&mut self, sender: &ComponentSender<Self>) {
        if self.screenshot_action_applied {
            return;
        }
        self.screenshot_action_applied = true;
        match std::env::var("KALAM_PDF_ACTION").as_deref() {
            Ok("chrome") => {
                sender.input(PdfReaderMsg::ShowBackChrome);
                if let Some(unit) = &self.back_unit {
                    unit.reveal();
                }
            }
            Ok("contents") => sender.input(PdfReaderMsg::OpenLeftPanel),
            Ok("contents-pinned") => {
                sender.input(PdfReaderMsg::OpenLeftPanel);
                if !self.left_panel_pinned {
                    sender.input(PdfReaderMsg::ToggleLeftPin);
                }
            }
            Ok("notes") | Ok("notes-empty") => {
                sender.input(PdfReaderMsg::OpenNotes);
            }
            Ok("notes-words") => {
                sender.input(PdfReaderMsg::SetNotesFilter(PdfNotesFilter::Words));
                sender.input(PdfReaderMsg::SetWordScope(PdfWordScope::Book));
                sender.input(PdfReaderMsg::SetLeftTab(PdfLeftTab::Notes));
                sender.input(PdfReaderMsg::OpenLeftPanel);
            }
            Ok("notes-bookmarks") | Ok("bookmarks") => {
                sender.input(PdfReaderMsg::SetNotesFilter(PdfNotesFilter::Bookmarks));
                sender.input(PdfReaderMsg::SetLeftTab(PdfLeftTab::Notes));
                sender.input(PdfReaderMsg::OpenLeftPanel);
            }
            Ok("pages") => {
                sender.input(PdfReaderMsg::SetPagesColumns(3));
                sender.input(PdfReaderMsg::SetLeftTab(PdfLeftTab::Pages));
                sender.input(PdfReaderMsg::OpenLeftPanel);
            }
            Ok("pages-2") => {
                sender.input(PdfReaderMsg::SetPagesColumns(2));
                sender.input(PdfReaderMsg::SetLeftTab(PdfLeftTab::Pages));
                sender.input(PdfReaderMsg::OpenLeftPanel);
                if !self.left_panel_pinned {
                    sender.input(PdfReaderMsg::ToggleLeftPin);
                }
            }
            Ok("reading") => {
                // The Reading board's own state: Spread + Scroll,
                // cover on its own, the 24px scroll gap, fit Height.
                sender.input(PdfReaderMsg::SetLayout(PdfLayout::Spread));
                sender.input(PdfReaderMsg::SetCoverOnItsOwn(true));
                sender.input(PdfReaderMsg::SetFlow(PdfFlow::Scroll));
                sender.input(PdfReaderMsg::SetScrollGap(24));
                sender.input(PdfReaderMsg::SetFitMode(PdfFitMode::Height));
                sender.input(PdfReaderMsg::OpenRightPanel);
                if !self.right_panel_pinned {
                    sender.input(PdfReaderMsg::ToggleRightPin);
                }
            }
            Ok("rotated") => {
                sender.input(PdfReaderMsg::SetLayout(PdfLayout::Spread));
                sender.input(PdfReaderMsg::SetFitMode(PdfFitMode::Screen));
                sender.input(PdfReaderMsg::RotateView(90));
            }
            Ok("image") => {
                // The Image board's own state: Tint at its default
                // strength over a spread, the sliders where the
                // board leaves them, the panel pinned beside it.
                sender.input(PdfReaderMsg::SetPageColour(PdfPageColour::Tint));
                sender.input(PdfReaderMsg::SetTintStrength(75));
                sender.input(PdfReaderMsg::SetBrightness(35));
                sender.input(PdfReaderMsg::SetContrast(35));
                sender.input(PdfReaderMsg::SetWarmth(5));
                sender.input(PdfReaderMsg::SetRightTab(PdfRightTab::Image));
                sender.input(PdfReaderMsg::OpenRightPanel);
                if !self.right_panel_pinned {
                    sender.input(PdfReaderMsg::ToggleRightPin);
                }
            }
            Ok("image-invert") => {
                sender.input(PdfReaderMsg::SetLayout(PdfLayout::Spread));
                sender.input(PdfReaderMsg::SetFitMode(PdfFitMode::Height));
                sender.input(PdfReaderMsg::SetPageColour(PdfPageColour::Invert));
            }
            Ok("search-results") => {
                sender.input(PdfReaderMsg::ToggleSearch);
                sender.input(PdfReaderMsg::SetSearchScope(PdfSearchScope::Book));
                sender.input(PdfReaderMsg::UpdateSearchQuery("water".to_string()));
                sender.input(PdfReaderMsg::OpenSearchResults);
            }
            Ok("search-no-matches") => {
                sender.input(PdfReaderMsg::ToggleSearch);
                sender.input(PdfReaderMsg::UpdateSearchQuery(
                    "volcanx".to_string(),
                ));
            }
            Ok("drawing") => {
                self.draw_toolbar_visible = true;
                sender.input(PdfReaderMsg::SetDrawTool(PdfDrawTool::Highlighter));
            }
            Ok("drawing-selected") => {
                self.draw_toolbar_visible = true;
                if let Some(drawing) = self.drawings.first().cloned() {
                    let position = self
                        .page_identities
                        .iter()
                        .find(|identity| identity.page_id == drawing.page_id)
                        .map(|identity| identity.position);
                    let slot = position.and_then(|page| {
                        self.page_overlays
                            .keys()
                            .copied()
                            .find(|slot| self.page_for_slot(*slot) == Some(page))
                    });
                    self.selected_drawing_id = Some(drawing.id);
                    self.refresh_drawing_paints();
                    self.queue_all_drawing_areas();
                    if let Some(slot) = slot {
                        self.show_selected_drawing_popover(slot, sender);
                    }
                }
            }
            Ok("shortcuts") => {
                sender.input(PdfReaderMsg::SetRightTab(PdfRightTab::Shortcuts));
                sender.input(PdfReaderMsg::OpenRightPanel);
            }
            Ok("jump") => sender.input(PdfReaderMsg::OpenJump(None)),
            Ok("jump-range") => {
                sender.input(PdfReaderMsg::OpenJump(Some("999".to_string())))
            }
            _ => {}
        }
    }

    pub fn update_bookmark_icon_state(&self, widgets: &PdfReaderModelWidgets) {
        let is_bookmarked = self
            .bookmarks
            .iter()
            .any(|b| b.chapter_index as usize == self.current_page);
        widgets.ribbon_image.set_icon_name(Some(if is_bookmarked {
            "kalam-ribbon-filled-symbolic"
        } else {
            "kalam-ribbon-outline-symbolic"
        }));
        if is_bookmarked {
            widgets.ribbon_button.add_css_class("set");
        } else {
            widgets.ribbon_button.remove_css_class("set");
        }
        let label = if is_bookmarked {
            "Remove the bookmark here (B)"
        } else {
            "Bookmark this page (B)"
        };
        widgets.ribbon_button.set_tooltip_text(Some(label));
        widgets
            .ribbon_button
            .update_property(&[gtk::accessible::Property::Label(label)]);
    }

    pub fn sync_search_scope_control(&self) {
        if let Some(control) = &self.search_scope_control {
            control.set_selected(self.search_scope.toggle_index());
        }
    }

    /// Put every piece of search back where a fresh Ctrl+F
    /// expects it: no query, no results, Chapter scope, no
    /// results takeover, and the displaced left tab restored.
    pub fn clear_search_state(
        &mut self,
        widgets: &PdfReaderModelWidgets,
        sender: &ComponentSender<Self>,
    ) {
        let restore_tab = self
            .search_results_open
            .then_some(self.search_results_return_tab);
        self.search_query.clear();
        if !widgets.search_entry.text().is_empty() {
            widgets.search_entry.set_text("");
        }
        self.search_results.clear();
        self.search_index = 0;
        self.search_scope = PdfSearchScope::Chapter;
        self.search_chapter_anchor_page = self.current_page;
        self.search_query_anchor_page = self.current_page;
        self.search_pending = false;
        self.search_no_text = false;
        self.search_results_open = false;
        if let Some(tab) = restore_tab {
            self.left_tab = tab;
        }
        self.sync_search_scope_control();
        self.refresh_search_highlights();
        self.update_search_results_panel();
        if restore_tab.is_some() {
            self.refresh_left_panel(widgets, sender);
        }
    }

    /// The list button's act (decision 61): results take the
    /// left panel whole, remembering the tab they displaced.
    pub fn open_search_results(
        &mut self,
        widgets: &PdfReaderModelWidgets,
        sender: &ComponentSender<Self>,
    ) {
        if !self.search_results_open {
            self.search_results_return_tab = self.left_tab;
            self.search_results_open = true;
        }
        self.left_panel_open = true;
        self.show_back_button = false;
        self.back_gen = self.back_gen.wrapping_add(1);
        widgets.left_revealer.set_reveal_child(true);
        self.refresh_left_panel(widgets, sender);
        if let Some(results) = &self.search_results_widgets {
            results.list.scroll_to_current();
        }
    }

    /// The results header's close: back to the tab that was
    /// standing when the results arrived, focus back in the
    /// query where a refining keystroke belongs.
    pub fn close_search_results(
        &mut self,
        widgets: &PdfReaderModelWidgets,
        sender: &ComponentSender<Self>,
    ) {
        self.search_results_open = false;
        self.left_tab = self.search_results_return_tab;
        self.refresh_left_panel(widgets, sender);
        if self.search_active {
            let entry = widgets.search_entry.clone();
            glib::idle_add_local_once(move || {
                entry.grab_focus();
            });
        }
    }

    /// Paint the header and rows decision 61 asks for: the
    /// quoted query with its scoped result/page counts above,
    /// chapter-grouped matches below. An empty scope gets one
    /// plain sentence rather than a blank panel.
    pub fn update_search_results_panel(&self) {
        let Some(panel) = &self.search_results_widgets else {
            return;
        };
        let query = self.search_query.trim();
        let scoped = self.search_scoped_indices();
        let current_raw = self.current_search_result_index();
        let scope_noun = match self.search_scope {
            PdfSearchScope::Chapter => "chapter",
            PdfSearchScope::Book => "book",
        };

        let (meta, entries) = if query.chars().count() < 2 {
            (
                "Search the text of this PDF".to_string(),
                vec![search_results::PdfSearchResultsEntry::Message(format!(
                    "Type to search this {scope_noun}."
                ))],
            )
        } else if self.search_pending {
            (
                format!("\"{query}\" · searching"),
                vec![search_results::PdfSearchResultsEntry::Message(
                    "Searching…".to_string(),
                )],
            )
        } else if self.search_no_text {
            (
                "This book has no searchable text".to_string(),
                Vec::new(),
            )
        } else if scoped.is_empty() {
            if self.search_scope == PdfSearchScope::Chapter && !self.search_results.is_empty() {
                (
                    format!("\"{query}\" · no results in this chapter"),
                    vec![search_results::PdfSearchResultsEntry::Message(
                        "No matches in this chapter. Book scope has the rest.".to_string(),
                    )],
                )
            } else {
                (
                    format!("\"{query}\" · no results"),
                    vec![search_results::PdfSearchResultsEntry::Message(
                        "No matches".to_string(),
                    )],
                )
            }
        } else {
            let pages = pdf_search_result_page_count(&self.search_results, &scoped);
            (
                format!(
                    "\"{query}\" · {} {} on {} {}",
                    scoped.len(),
                    if scoped.len() == 1 { "result" } else { "results" },
                    pages,
                    if pages == 1 { "page" } else { "pages" },
                ),
                pdf_search_result_entries(
                    &self.search_results,
                    &scoped,
                    &self.toc_entries,
                    self.total_pages,
                    &self.title,
                ),
            )
        };

        let theme = crate::theme::current(&self.catalog);
        let paint = search_results::PdfSearchSnippetPaint {
            match_colour: theme.text.to_string(),
            match_alpha_percent: if theme.dark { 10 } else { 8 },
            current_colour: theme.accent.to_string(),
        };
        panel.meta_label.set_label(&meta);
        panel
            .list
            .set_results(entries, query.to_string(), current_raw, paint);
    }

    fn search_slot_for_page(&self, page: usize) -> Option<PageSlot> {
        match self.scroll_mode {
            PdfScrollMode::PageScrolling => match self.spread_mode {
                PdfSpreadMode::NoSpreads => {
                    (page == self.current_page).then_some(PageSlot::PagedSingle)
                }
                _ => {
                    let (left, right) = self.spread_for_page(self.current_page);
                    if page == left {
                        Some(PageSlot::PagedSpreadLeft)
                    } else if right == Some(page) {
                        Some(PageSlot::PagedSpreadRight)
                    } else {
                        None
                    }
                }
            },
            _ => Some(PageSlot::Fixed(page)),
        }
    }

    fn search_page_paint(&self) -> ([f32; 3], f32, [f32; 3]) {
        let theme = crate::theme::current(&self.catalog);
        // Design-system §9's quiet tint is the standing ink at
        // a whisper. On a PDF page that ink is the page's own:
        // black on its pale paper, white once Invert has made
        // the page a negative — the UI's pale dark-mode text
        // would vanish against the paper it is meant to mark.
        let match_rgb = if self.image_settings.page_colour == PdfPageColour::Invert {
            [1.0, 1.0, 1.0]
        } else {
            [0.0, 0.0, 0.0]
        };
        (
            match_rgb,
            if theme.dark { 0.10 } else { 0.08 },
            image::rgb_from_hex(theme.accent),
        )
    }

    /// Rebuild the source-space highlights the page draw
    /// callbacks paint: every scoped match that has a slot in
    /// the standing viewport, current marked. This runs after
    /// page turns too, without jumping — leaving the current
    /// match's page is allowed; its tint simply waits there.
    pub fn refresh_search_highlights(&self) {
        let mut highlights = Vec::new();
        if self.search_active && !self.search_pending {
            let scoped = self.search_scoped_indices();
            let current_raw = scoped.get(self.search_index).copied();
            let (match_rgb, match_alpha, current_rgb) = self.search_page_paint();
            for raw_index in scoped {
                let Some(result) = self.search_results.get(raw_index) else {
                    continue;
                };
                let Some(slot) = self.search_slot_for_page(result.page) else {
                    continue;
                };
                let (source_width, source_height) = self
                    .page_text_cache
                    .get(&result.page)
                    .map(|text| (text.width_pts, text.height_pts))
                    .unwrap_or((self.source_page_width as f32, self.source_page_height as f32));
                highlights.push(PdfSearchHighlight {
                    slot,
                    page: result.page,
                    rects: result.rects.clone(),
                    source_width,
                    source_height,
                    rotation: self.rotation_degrees,
                    match_rgb,
                    match_alpha,
                    current_rgb,
                    current: Some(raw_index) == current_raw,
                });
            }
        }

        let changed = {
            let mut standing = self.search_highlights.borrow_mut();
            if *standing == highlights {
                false
            } else {
                *standing = highlights;
                true
            }
        };
        if changed {
            for draw_area in self.page_draw_areas.values() {
                draw_area.queue_draw();
            }
        }
    }

    /// Search navigation's landing: take the reader to the
    /// current scoped match's page, then let the standing
    /// viewport paint every match it can see.
    pub fn highlight_search_matches(
        &mut self,
        widgets: &PdfReaderModelWidgets,
        sender: &ComponentSender<Self>,
    ) {
        let Some(raw_index) = self.current_search_result_index() else {
            self.refresh_search_highlights();
            return;
        };
        let target_page = self.search_results[raw_index].page;
        if self.current_page != target_page {
            self.current_page = target_page.clamp(1, self.total_pages.max(1));
            self.save_progress();
            self.prune_textures();
            self.trigger_loads(sender);
            match self.scroll_mode {
                PdfScrollMode::PageScrolling => match self.spread_mode {
                    PdfSpreadMode::NoSpreads => self.update_paged_view(),
                    _ => self.update_two_page_view(),
                },
                _ => {
                    self.scroll_to_current_page(&widgets.viewport_scroll);
                }
            }
            self.update_bookmark_icon_state(widgets);
        }
        self.refresh_search_highlights();
    }
}

/// A cheap fingerprint of the book file (size + mtime) used to key the
/// persistent OCR page cache. A content hash would be stronger but would
/// mean reading a possibly-large file on the UI thread; size + mtime
/// catches every real-world change (a remaster rewrites the file, a
/// re-download changes both) for the cost of a stat call. A false "same"
/// would require a changed file with identical size and identical mtime.
/// One selection-handle grip: a 9 px teardrop whose point touches the
/// bar's outer end at (`tip_x`, `tip_y`), its body hanging away from the
/// text -- upward for the start handle, downward for the end. The same
/// shape and size the EPUB reader draws
/// (`crates/kalam-reader/src/handles.rs`), so the two readers look and
/// feel identical (2.20 step 5).
fn draw_teardrop(cr: &cairo::Context, tip_x: f64, tip_y: f64, start: bool) {
    const GRIP: f64 = 9.0;
    let r = GRIP / 2.0;
    let dir = if start { -1.0 } else { 1.0 };
    // The circle's centre sits r*sqrt(2) from the tip along the bar's
    // axis, so the tip is a corner of its bounding square and the two
    // edges from the tip are tangents.
    let c_y = tip_y + dir * r * std::f64::consts::SQRT_2;
    let t = r * std::f64::consts::FRAC_1_SQRT_2;
    cr.move_to(tip_x, tip_y);
    if start {
        // Tip to the left tangent point, then the far three quarters of
        // the circle -- left pole, top, right pole -- to the right
        // tangent, and close back at the tip.
        cr.line_to(tip_x - t, c_y + t);
        cr.arc(
            tip_x,
            c_y,
            r,
            0.75 * std::f64::consts::PI,
            2.25 * std::f64::consts::PI,
        );
    } else {
        // Tip to the right tangent point, then the far side -- right
        // pole, bottom, left pole -- to the left tangent, and close.
        cr.line_to(tip_x + t, c_y - t);
        cr.arc(
            tip_x,
            c_y,
            r,
            -0.25 * std::f64::consts::PI,
            1.25 * std::f64::consts::PI,
        );
    }
    cr.close_path();
}

/// The same teardrop turned on its side, for a selection on a
/// quarter-turned page: the body hangs east or west of the tip,
/// away from the text it belongs to. Drawing it through the
/// upright path under a rotated context keeps one grip shape in
/// the app rather than a second, drifting copy of it.
fn draw_teardrop_side(cr: &cairo::Context, tip_x: f64, tip_y: f64, east: bool) {
    let _ = cr.save();
    cr.translate(tip_x, tip_y);
    cr.rotate(if east {
        std::f64::consts::FRAC_PI_2
    } else {
        -std::f64::consts::FRAC_PI_2
    });
    draw_teardrop(cr, 0.0, 0.0, true);
    let _ = cr.fill();
    let _ = cr.restore();
}

pub fn pdf_path_fingerprint(path: &std::path::Path) -> Option<String> {
    use std::time::UNIX_EPOCH;
    let md = std::fs::metadata(path).ok()?;
    let mtime = md
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()?
        .as_secs();
    Some(format!("{}:{}", md.len(), mtime))
}

/// The search results pane that takes the left panel over
/// (decision 61): its own serif header and close where the tab
/// pill stood, the quoted count under it, and the lazy grouped
/// list running to the panel's foot.
fn build_pdf_search_results_pane(
    sender: &ComponentSender<PdfReaderModel>,
) -> (gtk::Box, PdfSearchResultsWidgets) {
    let list = search_results::PdfSearchResultsList::new(sender.input_sender().clone());

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .build();
    scroll.add_css_class("kalam-reader-panel-scroll");
    scroll.set_child(Some(&list.widget()));

    let title = gtk::Label::new(Some("Search results"));
    title.add_css_class("kalam-pdf-search-results-title");
    title.set_halign(gtk::Align::Fill);
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_width_chars(1);

    let close = gtk::Button::new();
    close.set_child(Some(&crate::icons::symbolic_with_classes(
        "window-close-symbolic",
        14,
        &["kalam-inline-icon"],
    )));
    close.add_css_class("kalam-pdf-search-results-close");
    close.set_size_request(32, 32);
    close.set_valign(gtk::Align::Center);
    close.set_tooltip_text(Some("Back to panel tabs"));
    close.update_property(&[gtk::accessible::Property::Label("Back to panel tabs")]);
    {
        let tx = sender.input_sender().clone();
        close.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::CloseSearchResults);
        });
    }

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.set_margin_start(20);
    header.set_margin_end(14);
    header.set_margin_top(18);
    header.append(&title);
    header.append(&close);

    let meta_label = gtk::Label::new(Some("Search the text of this PDF"));
    meta_label.add_css_class("kalam-pdf-search-results-meta");
    meta_label.set_halign(gtk::Align::Fill);
    meta_label.set_xalign(0.0);
    meta_label.set_wrap(true);
    meta_label.set_margin_start(20);
    meta_label.set_margin_end(20);
    meta_label.set_margin_top(2);
    meta_label.set_margin_bottom(10);

    let pane = gtk::Box::new(gtk::Orientation::Vertical, 0);
    pane.add_css_class("kalam-pdf-search-results-pane");
    pane.set_hexpand(true);
    pane.set_vexpand(true);
    pane.append(&header);
    pane.append(&meta_label);
    pane.append(&scroll);

    (
        pane,
        PdfSearchResultsWidgets { meta_label, list },
    )
}

fn pdf_pages_column_button(
    icon: &str,
    label: &str,
    columns: usize,
    sender: &ComponentSender<PdfReaderModel>,
) -> gtk::Button {
    let button = gtk::Button::new();
    button.set_child(Some(&crate::icons::symbolic_with_classes(
        icon,
        18,
        &["kalam-inline-icon"],
    )));
    button.add_css_class("kalam-pdf-pages-columns-btn");
    button.set_size_request(32, 32);
    button.set_valign(gtk::Align::Center);
    button.set_tooltip_text(Some(label));
    button.update_property(&[gtk::accessible::Property::Label(label)]);
    let tx = sender.input_sender().clone();
    button.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetPagesColumns(columns));
    });
    button
}

/// The Pages pane: a sticky chapter heading and column toggle
/// over the lazy GridView, with the board's soft fade where the
/// grid runs under the panel's rounded bottom.
fn build_pdf_pages_pane(
    model: &PdfReaderModel,
    sender: &ComponentSender<PdfReaderModel>,
) -> (
    gtk::Overlay,
    pages_grid::PdfPagesGrid,
    PdfPagesWidgets,
) {
    let grid = pages_grid::PdfPagesGrid::new(
        sender.input_sender().clone(),
        model.pages_columns,
        model.image_settings,
        model.image_paper_rgb,
    );

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .build();
    scroll.add_css_class("kalam-reader-panel-scroll");
    scroll.set_child(Some(&grid.widget()));

    let heading_title = gtk::Label::new(Some(&model.title));
    heading_title.add_css_class("kalam-pdf-pages-heading-title");
    heading_title.set_halign(gtk::Align::Fill);
    heading_title.set_xalign(0.0);
    heading_title.set_hexpand(true);
    heading_title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    heading_title.set_width_chars(1);

    let heading_meta = gtk::Label::new(Some("Opening PDF..."));
    heading_meta.add_css_class("kalam-pdf-pages-heading-meta");
    heading_meta.set_halign(gtk::Align::Fill);
    heading_meta.set_xalign(0.0);
    heading_meta.set_hexpand(true);
    heading_meta.set_ellipsize(gtk::pango::EllipsizeMode::End);
    heading_meta.set_width_chars(1);

    let heading_text = gtk::Box::new(gtk::Orientation::Vertical, 1);
    heading_text.set_hexpand(true);
    heading_text.append(&heading_title);
    heading_text.append(&heading_meta);

    let columns_3_button = pdf_pages_column_button(
        "kalam-columns-3-symbolic",
        "Three columns",
        3,
        sender,
    );
    let columns_2_button = pdf_pages_column_button(
        "kalam-columns-2-symbolic",
        "Two columns",
        2,
        sender,
    );

    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    heading.add_css_class("kalam-pdf-pages-heading");
    heading.set_margin_start(20);
    heading.set_margin_end(20);
    heading.set_margin_top(14);
    heading.set_margin_bottom(6);
    heading.append(&heading_text);
    heading.append(&columns_3_button);
    heading.append(&columns_2_button);

    let pane = gtk::Box::new(gtk::Orientation::Vertical, 0);
    pane.add_css_class("kalam-pdf-pages-pane");
    pane.set_hexpand(true);
    pane.set_vexpand(true);
    pane.append(&heading);
    pane.append(&scroll);

    let overlay = gtk::Overlay::new();
    overlay.set_hexpand(true);
    overlay.set_vexpand(true);
    overlay.set_child(Some(&pane));

    let fade = gtk::Box::new(gtk::Orientation::Vertical, 0);
    fade.add_css_class("kalam-pdf-pages-fade");
    fade.set_size_request(-1, 48);
    fade.set_halign(gtk::Align::Fill);
    fade.set_valign(gtk::Align::End);
    fade.set_can_target(false);
    overlay.add_overlay(&fade);

    (
        overlay,
        grid,
        PdfPagesWidgets {
            scroll,
            heading_title,
            heading_meta,
            columns_3_button,
            columns_2_button,
        },
    )
}

fn populate_toc_list(
    list_box: &gtk::Box,
    model: &PdfReaderModel,
    sender: &ComponentSender<PdfReaderModel>,
) -> (Vec<(usize, gtk::Box)>, Option<usize>) {
    let entries = &model.toc_entries;
    let page_labels = &model.page_labels;
    let collapsed = &model.collapsed_toc_entries;
    let filter = model.toc_filter.as_str();
    let current_page = model.current_page;
    let total_pages = model.total_pages;
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    if entries.is_empty() {
        let empty_box = gtk::Box::new(gtk::Orientation::Vertical, 10);
        empty_box.add_css_class("kalam-pdf-contents-empty");
        empty_box.set_hexpand(true);
        empty_box.set_vexpand(true);
        empty_box.set_valign(gtk::Align::Center);
        empty_box.set_margin_top(48);
        empty_box.set_margin_start(32);
        empty_box.set_margin_end(32);

        let icon = crate::icons::symbolic_with_classes(
            "view-list-symbolic",
            28,
            &["kalam-pdf-contents-empty-icon"],
        );
        icon.set_halign(gtk::Align::Center);
        empty_box.append(&icon);

        let title = gtk::Label::new(Some("No contents in this book"));
        title.add_css_class("kalam-pdf-contents-empty-title");
        title.set_halign(gtk::Align::Center);
        title.set_wrap(true);
        title.set_justify(gtk::Justification::Center);
        empty_box.append(&title);

        let text = gtk::Label::new(Some(
            "This PDF has no table of contents. Browse by page instead.",
        ));
        text.add_css_class("kalam-pdf-contents-empty-text");
        text.set_halign(gtk::Align::Center);
        text.set_wrap(true);
        text.set_justify(gtk::Justification::Center);
        empty_box.append(&text);

        let open_pages = gtk::Button::with_label("Open Pages");
        open_pages.add_css_class("kalam-pdf-contents-open-pages");
        open_pages.set_halign(gtk::Align::Center);
        open_pages.set_margin_top(8);
        let tx = sender.input_sender().clone();
        open_pages.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::OpenPages);
        });
        empty_box.append(&open_pages);

        list_box.append(&empty_box);
        return (Vec::new(), None);
    }

    let visible = visible_toc_rows(entries, collapsed, filter);
    if visible.is_empty() {
        let empty_label = gtk::Label::new(Some("No matches."));
        empty_label.add_css_class("dim-label");
        empty_label.set_margin_top(24);
        empty_label.set_margin_start(20);
        empty_label.set_margin_end(20);
        list_box.append(&empty_label);
        return (Vec::new(), None);
    }

    let current_index = current_toc_index(entries, current_page, total_pages);
    let filtering = !filter.trim().is_empty();
    let mut rows = Vec::with_capacity(visible.len());
    let mut current_visible = None;

    for visible_row in visible {
        let item = &entries[visible_row.index];
        let page_target = item.page;
        if page_target == 0 || page_target > total_pages {
            continue;
        }

        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        row_box.add_css_class("kalam-pdf-contents-row");
        if item.depth == 0 {
            row_box.add_css_class("top-level");
        }
        if Some(visible_row.index) == current_index {
            row_box.add_css_class("active");
            current_visible = Some(rows.len());
        }
        row_box.set_hexpand(true);
        // The bar below expands inside the row; without an explicit
        // false here, GTK lets the row inherit that expansion and a
        // short outline stretches to fill the whole panel instead of
        // keeping the spec's 44px rows.
        row_box.set_vexpand(false);
        row_box.set_valign(gtk::Align::Start);
        row_box.set_size_request(-1, 44);

        // The current mark lives at the panel's edge (decision
        // 58), so the bar is the row's first cell — always drawn,
        // transparent when idle, so titles never shift.
        let bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
        bar.add_css_class("kalam-pdf-contents-bar");
        bar.set_size_request(3, -1);
        bar.set_vexpand(true);
        row_box.append(&bar);

        let indent = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        indent.set_size_request(17 + (item.depth as i32 * 18), -1);
        row_box.append(&indent);

        if visible_row.has_children && !filtering {
            let chevron = gtk::Button::new();
            chevron.add_css_class("kalam-pdf-contents-chevron");
            chevron.set_size_request(24, 32);
            chevron.set_valign(gtk::Align::Center);
            chevron.set_child(Some(&crate::icons::symbolic_with_classes(
                if visible_row.collapsed {
                    "pan-end-symbolic"
                } else {
                    "pan-down-symbolic"
                },
                14,
                &["kalam-inline-icon"],
            )));
            let chevron_label = if visible_row.collapsed {
                format!("Expand {}", item.title)
            } else {
                format!("Collapse {}", item.title)
            };
            chevron.set_tooltip_text(Some(&chevron_label));
            chevron
                .update_property(&[gtk::accessible::Property::Label(&chevron_label)]);
            let entry_index = visible_row.index;
            let tx = sender.input_sender().clone();
            chevron.connect_clicked(move |_| {
                let _ = tx.send(PdfReaderMsg::ToggleTocEntry(entry_index));
            });
            row_box.append(&chevron);
        } else {
            let chevron_slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            chevron_slot.set_size_request(24, -1);
            row_box.append(&chevron_slot);
        }

        let nav_content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title_lbl = gtk::Label::new(Some(&item.title));
        title_lbl.add_css_class("kalam-pdf-contents-title");
        title_lbl.set_xalign(0.0);
        title_lbl.set_hexpand(true);
        title_lbl.set_valign(gtk::Align::Center);
        title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title_lbl.set_width_chars(1);
        nav_content.append(&title_lbl);

        // The right-hand name is the document's page label
        // (decision 58), not the physical page the row jumps to.
        let page_lbl =
            gtk::Label::new(Some(&displayed_page_label(page_labels, page_target)));
        page_lbl.add_css_class("kalam-pdf-contents-page");
        page_lbl.set_halign(gtk::Align::End);
        page_lbl.set_valign(gtk::Align::Center);
        page_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        page_lbl.set_max_width_chars(16);
        nav_content.append(&page_lbl);

        let nav_button = gtk::Button::new();
        nav_button.set_child(Some(&nav_content));
        nav_button.add_css_class("flat");
        nav_button.add_css_class("kalam-pdf-contents-nav");
        nav_button.set_hexpand(true);
        nav_button.set_valign(gtk::Align::Fill);
        let tx = sender.input_sender().clone();
        nav_button.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::JumpToPage(page_target));
        });
        row_box.append(&nav_button);

        rows.push((visible_row.index, row_box.clone()));
        list_box.append(&row_box);
    }

    (rows, current_visible)
}

fn pdf_shortcut_row(label: &str, keys: &[&str]) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("kalam-pdf-shortcut-row");
    row.set_size_request(-1, 48);
    let lbl = gtk::Label::new(Some(label));
    lbl.set_halign(gtk::Align::Start);
    lbl.set_hexpand(true);
    lbl.set_valign(gtk::Align::Center);
    row.append(&lbl);
    for key in keys {
        let chip = gtk::Label::new(Some(key));
        chip.add_css_class("kalam-ui-key-chip");
        chip.set_valign(gtk::Align::Center);
        row.append(&chip);
    }
    row
}

fn pdf_shortcut_group(host: &gtk::Box, heading: &str, rows: &[(&str, &[&str])]) {
    let heading_lbl = gtk::Label::new(Some(heading));
    heading_lbl.add_css_class("kalam-reader-section-label");
    heading_lbl.set_halign(gtk::Align::Start);
    heading_lbl.set_margin_top(14);
    heading_lbl.set_margin_bottom(8);
    host.append(&heading_lbl);
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("kalam-pdf-inset-card");
    for (label, keys) in rows {
        card.append(&pdf_shortcut_row(label, keys));
    }
    host.append(&card);
}

/// The Shortcuts pane (decision 53): the reader's keys in their
/// three groups. Recording a new key and the conflict refusal
/// ride on the shared keybind store and join here with it; until
/// then the pane is the honest reference for the keys the reader
/// answers.
fn build_pdf_shortcuts_panel(host: &gtk::Box) {
    pdf_shortcut_group(
        host,
        "Navigation",
        &[
            ("Next page", &["→", "Space"][..]),
            ("Previous page", &["←"][..]),
            ("First page", &["Home"][..]),
            ("Last page", &["End"][..]),
            ("Go to page", &["G"][..]),
            ("Toggle bookmark", &["B"][..]),
        ],
    );
    pdf_shortcut_group(
        host,
        "View",
        &[
            ("Zoom in", &["+"][..]),
            ("Zoom out", &["−"][..]),
            ("Actual size", &["0"][..]),
            ("Search", &["Ctrl", "F"][..]),
            ("Copy selection", &["Ctrl", "C"][..]),
            ("Undo delete", &["Ctrl", "Z"][..]),
        ],
    );
    pdf_shortcut_group(
        host,
        "Panels",
        &[
            ("Contents", &["T"][..]),
            ("Pages", &["P"][..]),
            ("Notes", &["H"][..]),
            ("Reading settings", &["S"][..]),
            ("Close layer or exit", &["Esc"][..]),
        ],
    );
}

fn pdf_reading_heading(label: &str) -> gtk::Label {
    let heading = gtk::Label::new(Some(label));
    heading.add_css_class("kalam-reader-section-label");
    heading.set_halign(gtk::Align::Start);
    heading.set_xalign(0.0);
    heading
}

fn pdf_reading_card() -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("kalam-pdf-inset-card");
    card.add_css_class("kalam-pdf-reading-card");
    card.set_hexpand(true);
    card
}

fn pdf_reading_separator() -> gtk::Separator {
    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    separator.add_css_class("kalam-pdf-reading-separator");
    separator
}

fn pdf_tile_button(icon: &str, label: &str, tooltip: &str) -> gtk::Button {
    let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
    content.set_halign(gtk::Align::Center);
    content.set_valign(gtk::Align::Center);
    content.append(&crate::icons::symbolic_with_classes(
        icon,
        20,
        &["kalam-inline-icon"],
    ));
    let text = gtk::Label::new(Some(label));
    text.set_halign(gtk::Align::Center);
    content.append(&text);

    let button = gtk::Button::new();
    button.add_css_class("kalam-reader-seg-btn");
    button.add_css_class("kalam-pdf-tile");
    button.set_child(Some(&content));
    button.set_hexpand(true);
    button.set_size_request(-1, 68);
    button.set_tooltip_text(Some(tooltip));
    button.update_property(&[gtk::accessible::Property::Label(label)]);
    button
}

fn pdf_slider_row(
    label: &str,
    min: f64,
    max: f64,
    step: f64,
    initial: f64,
    suffix: &str,
) -> (gtk::Box, gtk::Scale, gtk::Label) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.add_css_class("kalam-pdf-reading-row");
    row.set_size_request(-1, 52);
    row.set_margin_start(14);
    row.set_margin_end(14);

    let name = gtk::Label::new(Some(label));
    name.add_css_class("kalam-pdf-reading-name");
    name.set_size_request(92, -1);
    name.set_halign(gtk::Align::Start);
    name.set_xalign(0.0);
    name.set_valign(gtk::Align::Center);
    name.set_wrap(true);
    row.append(&name);

    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, min, max, step);
    scale.add_css_class("kalam-ui-slider");
    scale.add_css_class("kalam-pdf-slider");
    scale.set_draw_value(false);
    scale.set_digits(0);
    scale.set_value(initial);
    scale.set_hexpand(true);
    scale.set_valign(gtk::Align::Center);
    row.append(&scale);

    let value = gtk::Label::new(Some(&format!("{}{}", initial.round() as i32, suffix)));
    value.add_css_class("kalam-pdf-reading-value");
    value.set_size_request(44, -1);
    value.set_halign(gtk::Align::End);
    value.set_xalign(1.0);
    value.set_valign(gtk::Align::Center);
    row.append(&value);

    (row, scale, value)
}

fn build_pdf_settings_panel(
    model: &PdfReaderModel,
    sender: &ComponentSender<PdfReaderModel>,
) -> (gtk::Box, PdfSettingsWidgets) {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 12);
    wrap.add_css_class("kalam-pdf-reading");
    wrap.set_margin_start(20);
    wrap.set_margin_end(20);
    wrap.set_margin_top(14);
    wrap.set_margin_bottom(24);

    // ── Page style (decisions 53–54): two independent axes. ──
    wrap.append(&pdf_reading_heading("Page style"));
    let page_style_card = pdf_reading_card();

    let layout_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    layout_row.set_homogeneous(true);
    layout_row.set_margin_start(12);
    layout_row.set_margin_end(12);
    layout_row.set_margin_top(12);
    layout_row.set_margin_bottom(12);

    let layout_single_btn = pdf_tile_button(
        "view-paged-symbolic",
        "Single",
        "Show one page at a time",
    );
    let tx = sender.input_sender().clone();
    layout_single_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetLayout(PdfLayout::Single));
    });
    layout_row.append(&layout_single_btn);

    let layout_spread_btn = pdf_tile_button(
        "view-dual-symbolic",
        "Spread",
        "Show two facing pages",
    );
    let tx = sender.input_sender().clone();
    layout_spread_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetLayout(PdfLayout::Spread));
    });
    layout_row.append(&layout_spread_btn);
    page_style_card.append(&layout_row);
    page_style_card.append(&pdf_reading_separator());

    let flow_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    flow_row.add_css_class("kalam-pdf-reading-row");
    flow_row.set_size_request(-1, 52);
    flow_row.set_margin_start(14);
    flow_row.set_margin_end(14);
    let flow_label = gtk::Label::new(Some("Flow"));
    flow_label.add_css_class("kalam-pdf-reading-name");
    flow_label.set_hexpand(true);
    flow_label.set_halign(gtk::Align::Start);
    flow_label.set_xalign(0.0);
    flow_label.set_valign(gtk::Align::Center);
    flow_row.append(&flow_label);

    let tx = sender.input_sender().clone();
    let flow_control = crate::widgets::segmented::segmented_control(
        &["Paged", "Scroll"],
        match pdf_flow_for_scroll_mode(model.scroll_mode) {
            PdfFlow::Paged => 0,
            PdfFlow::Scroll => 1,
        },
        crate::widgets::segmented::SegmentedSize::Panel,
        move |idx| {
            let _ = tx.send(PdfReaderMsg::SetFlow(if idx == 1 {
                PdfFlow::Scroll
            } else {
                PdfFlow::Paged
            }));
        },
    );
    let flow_root = flow_control.widget();
    flow_root.set_halign(gtk::Align::End);
    flow_root.set_size_request(150, -1);
    for button in flow_control.buttons() {
        button.set_hexpand(true);
    }
    flow_control.buttons()[0].set_tooltip_text(Some("Turn one spread at a time"));
    flow_control.buttons()[1].set_tooltip_text(Some("Read a continuous vertical stream"));
    flow_row.append(&flow_root);
    page_style_card.append(&flow_row);
    page_style_card.append(&pdf_reading_separator());

    let (scroll_gap_row, scroll_gap_scale, scroll_gap_value) = pdf_slider_row(
        "Gap between pages",
        0.0,
        PDF_MAX_SCROLL_GAP as f64,
        2.0,
        model.scroll_gap as f64,
        "",
    );
    {
        let tx = sender.input_sender().clone();
        let value = scroll_gap_value.clone();
        scroll_gap_scale.connect_value_changed(move |scale| {
            let gap = scale.value().round() as i32;
            value.set_label(&gap.to_string());
            let _ = tx.send(PdfReaderMsg::SetScrollGap(gap));
        });
    }
    page_style_card.append(&scroll_gap_row);
    page_style_card.append(&pdf_reading_separator());

    let cover_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    cover_row.add_css_class("kalam-pdf-reading-row");
    cover_row.set_size_request(-1, 52);
    cover_row.set_margin_start(14);
    cover_row.set_margin_end(14);
    let cover_label = gtk::Label::new(Some("Cover on its own"));
    cover_label.add_css_class("kalam-pdf-reading-name");
    cover_label.set_hexpand(true);
    cover_label.set_halign(gtk::Align::Start);
    cover_label.set_xalign(0.0);
    cover_label.set_valign(gtk::Align::Center);
    cover_row.append(&cover_label);
    let tx = sender.input_sender().clone();
    let cover_switch = crate::pages::settings::toggle_switch(model.cover_on_its_own, move |on| {
        let _ = tx.send(PdfReaderMsg::SetCoverOnItsOwn(on));
    });
    cover_switch.set_tooltip_text(Some("Show page 1 alone before pairing spreads"));
    cover_switch.update_property(&[gtk::accessible::Property::Label("Cover on its own")]);
    cover_row.append(&cover_switch);
    page_style_card.append(&cover_row);
    wrap.append(&page_style_card);

    // ── Fit (decision 54). ──
    wrap.append(&pdf_reading_heading("Fit"));
    let fit_card = pdf_reading_card();
    let fit_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    fit_row.set_homogeneous(true);
    fit_row.set_margin_start(12);
    fit_row.set_margin_end(12);
    fit_row.set_margin_top(12);
    fit_row.set_margin_bottom(12);

    let fit_width_btn = pdf_tile_button(
        "zoom-fit-best-symbolic",
        "Width",
        "Fit the page or spread to the width",
    );
    let tx = sender.input_sender().clone();
    fit_width_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetFitMode(PdfFitMode::Width));
    });
    fit_row.append(&fit_width_btn);

    let fit_height_btn = pdf_tile_button(
        "kalam-pdf-page-scroll-symbolic",
        "Height",
        "Fit the page to the height",
    );
    let tx = sender.input_sender().clone();
    fit_height_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetFitMode(PdfFitMode::Height));
    });
    fit_row.append(&fit_height_btn);

    let fit_screen_btn = pdf_tile_button(
        "view-fullscreen-symbolic",
        "Screen",
        "Fit the whole page or spread on screen",
    );
    let tx = sender.input_sender().clone();
    fit_screen_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetFitMode(PdfFitMode::Screen));
    });
    fit_row.append(&fit_screen_btn);

    let fit_original_btn = pdf_tile_button(
        "zoom-original-symbolic",
        "Original",
        "Show the page at its original size",
    );
    let tx = sender.input_sender().clone();
    fit_original_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetFitMode(PdfFitMode::Original));
    });
    fit_row.append(&fit_original_btn);
    fit_card.append(&fit_row);
    wrap.append(&fit_card);

    // ── View: zoom over the fit, and the quarter turns. ──
    wrap.append(&pdf_reading_heading("View"));
    let view_card = pdf_reading_card();
    let (zoom_row, zoom_scale, zoom_value) = pdf_slider_row(
        "Zoom",
        PDF_MIN_ZOOM_PERCENT as f64,
        PDF_MAX_ZOOM_PERCENT as f64,
        5.0,
        model.zoom_percent as f64,
        "%",
    );
    {
        let tx = sender.input_sender().clone();
        let value = zoom_value.clone();
        zoom_scale.connect_value_changed(move |scale| {
            let percent = scale.value().round() as u32;
            value.set_label(&format!("{percent}%"));
            let _ = tx.send(PdfReaderMsg::SetZoomPercent(percent));
        });
    }
    view_card.append(&zoom_row);
    view_card.append(&pdf_reading_separator());

    let rotate_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    rotate_row.add_css_class("kalam-pdf-reading-row");
    rotate_row.set_size_request(-1, 52);
    rotate_row.set_margin_start(14);
    rotate_row.set_margin_end(14);
    let rotate_label = gtk::Label::new(Some("Rotate"));
    rotate_label.add_css_class("kalam-pdf-reading-name");
    rotate_label.set_hexpand(true);
    rotate_label.set_halign(gtk::Align::Start);
    rotate_label.set_xalign(0.0);
    rotate_label.set_valign(gtk::Align::Center);
    rotate_row.append(&rotate_label);

    let rotate_buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    rotate_buttons.set_valign(gtk::Align::Center);
    let rotate_left_btn = gtk::Button::new();
    rotate_left_btn.add_css_class("kalam-pdf-rotate-btn");
    rotate_left_btn.set_size_request(36, 36);
    rotate_left_btn.set_valign(gtk::Align::Center);
    rotate_left_btn.set_child(Some(&crate::icons::symbolic_with_classes(
        "object-rotate-left-symbolic",
        16,
        &["kalam-inline-icon"],
    )));
    let tx = sender.input_sender().clone();
    rotate_left_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::RotateView(-90));
    });
    rotate_left_btn.set_tooltip_text(Some("Rotate the view left"));
    rotate_left_btn
        .update_property(&[gtk::accessible::Property::Label("Rotate the view left")]);
    rotate_buttons.append(&rotate_left_btn);

    let rotate_right_btn = gtk::Button::new();
    rotate_right_btn.add_css_class("kalam-pdf-rotate-btn");
    rotate_right_btn.set_size_request(36, 36);
    rotate_right_btn.set_valign(gtk::Align::Center);
    rotate_right_btn.set_child(Some(&crate::icons::symbolic_with_classes(
        "object-rotate-right-symbolic",
        16,
        &["kalam-inline-icon"],
    )));
    let tx = sender.input_sender().clone();
    rotate_right_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::RotateView(90));
    });
    rotate_right_btn.set_tooltip_text(Some("Rotate the view right"));
    rotate_right_btn
        .update_property(&[gtk::accessible::Property::Label("Rotate the view right")]);
    rotate_buttons.append(&rotate_right_btn);
    rotate_row.append(&rotate_buttons);
    view_card.append(&rotate_row);
    wrap.append(&view_card);

    let widgets = PdfSettingsWidgets {
        layout_single_btn,
        layout_spread_btn,
        flow_control,
        scroll_gap_row,
        scroll_gap_scale,
        scroll_gap_value,
        cover_switch,
        fit_width_btn,
        fit_height_btn,
        fit_screen_btn,
        fit_original_btn,
        zoom_scale,
        zoom_value,
        rotate_left_btn,
        rotate_right_btn,
    };

    (wrap, widgets)
}

fn pdf_image_slider_row(
    label: &str,
    min: f64,
    max: f64,
    step: f64,
    initial: f64,
) -> (gtk::Box, gtk::Scale) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.add_css_class("kalam-pdf-reading-row");
    row.set_size_request(-1, 52);
    row.set_margin_start(14);
    row.set_margin_end(14);

    let name = gtk::Label::new(Some(label));
    name.add_css_class("kalam-pdf-reading-name");
    name.set_size_request(116, -1);
    name.set_halign(gtk::Align::Start);
    name.set_xalign(0.0);
    name.set_valign(gtk::Align::Center);
    name.set_wrap(true);
    row.append(&name);

    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, min, max, step);
    scale.add_css_class("kalam-ui-slider");
    scale.add_css_class("kalam-pdf-slider");
    scale.set_draw_value(false);
    scale.set_digits(0);
    scale.set_value(initial);
    scale.set_hexpand(true);
    scale.set_valign(gtk::Align::Center);
    scale.update_property(&[gtk::accessible::Property::Label(label)]);
    row.append(&scale);

    (row, scale)
}

fn connect_pdf_image_scale(
    scale: &gtk::Scale,
    sender: &ComponentSender<PdfReaderModel>,
    make_msg: Rc<dyn Fn(i32) -> PdfReaderMsg>,
) {
    let tx = sender.input_sender().clone();
    scale.connect_value_changed(move |scale| {
        let _ = tx.send(make_msg(scale.value().round() as i32));
    });
}

fn pdf_image_switch_row(
    label: &str,
    initial: bool,
    tooltip: &str,
    sender: &ComponentSender<PdfReaderModel>,
    make_msg: Rc<dyn Fn(bool) -> PdfReaderMsg>,
) -> (gtk::Box, gtk::Switch) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.add_css_class("kalam-pdf-reading-row");
    row.set_size_request(-1, 52);
    row.set_margin_start(14);
    row.set_margin_end(14);

    let name = gtk::Label::new(Some(label));
    name.add_css_class("kalam-pdf-reading-name");
    name.set_hexpand(true);
    name.set_halign(gtk::Align::Start);
    name.set_xalign(0.0);
    name.set_valign(gtk::Align::Center);
    row.append(&name);

    let tx = sender.input_sender().clone();
    let switch = crate::pages::settings::toggle_switch(initial, move |on| {
        let _ = tx.send(make_msg(on));
    });
    switch.set_tooltip_text(Some(tooltip));
    switch.update_property(&[gtk::accessible::Property::Label(label)]);
    row.append(&switch);

    (row, switch)
}

fn build_pdf_image_panel(
    model: &PdfReaderModel,
    sender: &ComponentSender<PdfReaderModel>,
) -> (gtk::Box, PdfImageWidgets) {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 12);
    wrap.add_css_class("kalam-pdf-image");
    wrap.set_margin_start(20);
    wrap.set_margin_end(20);
    wrap.set_margin_top(14);
    wrap.set_margin_bottom(24);

    // ── Page colour (decisions 54–55). ──
    wrap.append(&pdf_reading_heading("Page colour"));
    let page_colour_card = pdf_reading_card();

    let tx = sender.input_sender().clone();
    let page_colour_control = crate::widgets::segmented::segmented_control(
        &["Off", "Tint", "Invert"],
        model.image_settings.page_colour.segment_index(),
        crate::widgets::segmented::SegmentedSize::Panel,
        move |idx| {
            let _ = tx.send(PdfReaderMsg::SetPageColour(
                PdfPageColour::from_segment_index(idx),
            ));
        },
    );
    let page_colour_root = page_colour_control.widget();
    page_colour_root.set_hexpand(true);
    page_colour_root.set_halign(gtk::Align::Fill);
    for button in page_colour_control.buttons() {
        button.set_hexpand(true);
    }
    page_colour_control.buttons()[0]
        .set_tooltip_text(Some("Show the page exactly as the file drew it"));
    page_colour_control.buttons()[1]
        .set_tooltip_text(Some("Dim white paper toward the palette's paper colour"));
    page_colour_control.buttons()[2]
        .set_tooltip_text(Some("Invert the page; best on text-only pages"));
    let page_colour_row = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page_colour_row.set_margin_start(12);
    page_colour_row.set_margin_end(12);
    page_colour_row.set_margin_top(12);
    page_colour_row.append(&page_colour_root);
    page_colour_card.append(&page_colour_row);

    let caption = gtk::Label::new(Some(
        "Dims white paper toward the page colour; illustrations stay almost \
         unchanged. Invert works best on text-only pages.",
    ));
    caption.add_css_class("kalam-pdf-image-caption");
    caption.set_wrap(true);
    caption.set_xalign(0.0);
    caption.set_margin_start(14);
    caption.set_margin_end(14);
    caption.set_margin_top(12);
    caption.set_margin_bottom(14);
    page_colour_card.append(&caption);
    page_colour_card.append(&pdf_reading_separator());

    let (tint_strength_row, tint_strength_scale) = pdf_image_slider_row(
        "Tint strength",
        0.0,
        100.0,
        1.0,
        model.image_settings.tint_strength as f64,
    );
    connect_pdf_image_scale(
        &tint_strength_scale,
        sender,
        Rc::new(PdfReaderMsg::SetTintStrength),
    );
    page_colour_card.append(&tint_strength_row);
    wrap.append(&page_colour_card);

    // ── Adjust (decision 54). ──
    wrap.append(&pdf_reading_heading("Adjust"));
    let adjust_card = pdf_reading_card();

    let (brightness_row, brightness_scale) = pdf_image_slider_row(
        "Brightness",
        PdfImageSettings::MIN_ADJUSTMENT as f64,
        PdfImageSettings::MAX_ADJUSTMENT as f64,
        1.0,
        model.image_settings.brightness as f64,
    );
    connect_pdf_image_scale(
        &brightness_scale,
        sender,
        Rc::new(PdfReaderMsg::SetBrightness),
    );
    adjust_card.append(&brightness_row);
    adjust_card.append(&pdf_reading_separator());

    let (contrast_row, contrast_scale) = pdf_image_slider_row(
        "Contrast",
        PdfImageSettings::MIN_ADJUSTMENT as f64,
        PdfImageSettings::MAX_ADJUSTMENT as f64,
        1.0,
        model.image_settings.contrast as f64,
    );
    connect_pdf_image_scale(
        &contrast_scale,
        sender,
        Rc::new(PdfReaderMsg::SetContrast),
    );
    adjust_card.append(&contrast_row);
    adjust_card.append(&pdf_reading_separator());

    let (warmth_row, warmth_scale) = pdf_image_slider_row(
        "Warmth",
        PdfImageSettings::MIN_ADJUSTMENT as f64,
        PdfImageSettings::MAX_ADJUSTMENT as f64,
        1.0,
        model.image_settings.warmth as f64,
    );
    connect_pdf_image_scale(
        &warmth_scale,
        sender,
        Rc::new(PdfReaderMsg::SetWarmth),
    );
    adjust_card.append(&warmth_row);
    adjust_card.append(&pdf_reading_separator());

    let (grayscale_row, grayscale_switch) = pdf_image_switch_row(
        "Grayscale",
        model.image_settings.grayscale,
        "Show the page without colour",
        sender,
        Rc::new(PdfReaderMsg::SetGrayscale),
    );
    adjust_card.append(&grayscale_row);
    adjust_card.append(&pdf_reading_separator());

    let (trim_row, trim_switch) = pdf_image_switch_row(
        "Trim margins",
        model.image_settings.trim_margins,
        "Crop each page to its printed area",
        sender,
        Rc::new(PdfReaderMsg::SetSmartCrop),
    );
    adjust_card.append(&trim_row);
    wrap.append(&adjust_card);

    let tx = sender.input_sender().clone();
    let reset_button = crate::widgets::pill_button::boxed_ghost_button(
        "Reset image settings",
        move || {
            let _ = tx.send(PdfReaderMsg::ResetImageSettings);
        },
    );
    reset_button.set_halign(gtk::Align::Center);
    reset_button.set_margin_top(4);
    wrap.append(&reset_button);

    let widgets = PdfImageWidgets {
        page_colour_control,
        tint_strength_row,
        tint_strength_scale,
        brightness_scale,
        contrast_scale,
        warmth_scale,
        grayscale_switch,
        trim_switch,
    };

    (wrap, widgets)
}

fn toggle_active(button: &gtk::Button, active: bool) {
    if active {
        button.add_css_class("active");
    } else {
        button.remove_css_class("active");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window(
        spread_mode: PdfSpreadMode,
        scroll_mode: PdfScrollMode,
        total: usize,
        page: usize,
        spreads: usize,
    ) -> Vec<usize> {
        let mut pages: Vec<usize> =
            PdfReaderModel::keep_window_pages(spread_mode, scroll_mode, total, page, spreads)
                .into_iter()
                .collect();
        pages.sort_unstable();
        pages
    }

    fn toc(title: &str, page: usize, depth: usize) -> PdfTocEntry {
        PdfTocEntry {
            title: title.to_string(),
            page,
            depth,
        }
    }

    fn sample_toc() -> Vec<PdfTocEntry> {
        vec![
            toc("Preface", 1, 0),
            toc("The Earth", 2, 0),
            toc("Structure of the Earth", 3, 1),
            toc("Plate tectonics", 4, 1),
            toc("Oceans", 8, 0),
            toc("Currents", 9, 1),
            toc("Index", 12, 0),
        ]
    }

    fn search_result(page: usize) -> PdfSearchResult {
        PdfSearchResult {
            page,
            section_title: None,
            snippet: format!("…water on page {page}…"),
            match_text: "water".to_string(),
            rects: Vec::new(),
        }
    }

    fn sample_search_results() -> Vec<PdfSearchResult> {
        [1, 2, 3, 4, 7, 8, 9, 9, 12]
            .into_iter()
            .map(search_result)
            .collect()
    }

    #[test]
    fn pdf_search_chapter_scope_uses_the_anchored_top_level_span() {
        let results = sample_search_results();
        let scoped = pdf_search_scoped_indices(
            &results,
            &sample_toc(),
            12,
            PdfSearchScope::Chapter,
            4,
            "The Book",
        );
        assert_eq!(scoped, vec![1, 2, 3, 4]);
        assert_eq!(
            pdf_search_start_position(&results, &scoped, 4),
            2,
            "the first match on the standing page, not the chapter's first"
        );
    }

    #[test]
    fn pdf_search_book_scope_and_start_wrap_the_whole_book() {
        let results = sample_search_results();
        let scoped = pdf_search_scoped_indices(
            &results,
            &sample_toc(),
            12,
            PdfSearchScope::Book,
            4,
            "The Book",
        );
        assert_eq!(scoped.len(), results.len());
        assert_eq!(pdf_search_start_position(&results, &scoped, 10), 8);
        assert_eq!(pdf_search_start_position(&results, &scoped, 13), 0);
        assert_eq!(pdf_search_result_page_count(&results, &scoped), 8);
    }

    #[test]
    fn pdf_search_groups_results_by_chapter_and_section() {
        assert_eq!(
            pdf_search_section_path(&sample_toc(), 12, 4, "The Book"),
            "The Earth · Plate tectonics"
        );
        assert_eq!(
            pdf_search_section_path(&sample_toc(), 12, 9, "The Book"),
            "Oceans · Currents"
        );

        let results = sample_search_results();
        let scoped: Vec<usize> = (0..results.len()).collect();
        let entries = pdf_search_result_entries(
            &results,
            &scoped,
            &sample_toc(),
            12,
            "The Book",
        );
        let headings: Vec<String> = entries
            .iter()
            .filter_map(|entry| match entry {
                search_results::PdfSearchResultsEntry::Heading(title) => {
                    Some(title.clone())
                }
                _ => None,
            })
            .collect();
        assert_eq!(
            headings,
            vec![
                "Preface",
                "The Earth",
                "The Earth · Structure of the Earth",
                "The Earth · Plate tectonics",
                "Oceans",
                "Oceans · Currents",
                "Index",
            ]
        );
    }

    #[test]
    fn pdf_search_without_an_outline_searches_the_whole_book_as_its_chapter() {
        let results = sample_search_results();
        let scoped = pdf_search_scoped_indices(
            &results,
            &[],
            12,
            PdfSearchScope::Chapter,
            4,
            "The Book",
        );
        assert_eq!(scoped.len(), results.len());
        let entries =
            pdf_search_result_entries(&results, &scoped, &[], 12, "The Book");
        assert_eq!(
            entries.first(),
            Some(&search_results::PdfSearchResultsEntry::Heading(
                "The Book".to_string()
            ))
        );
    }

    #[test]
    fn test_location_label_names_the_whole_spread() {
        assert_eq!(
            format_location_label(
                PdfSpreadMode::NoSpreads,
                PdfScrollMode::VerticalScrolling,
                12,
                4,
                &[]
            ),
            "4 / 12"
        );
        // Cover on its own: page 4 opens the 4–5 spread.
        assert_eq!(
            format_location_label(
                PdfSpreadMode::OddSpreads,
                PdfScrollMode::PageScrolling,
                12,
                4,
                &[]
            ),
            "4–5 / 12"
        );
        assert_eq!(
            format_location_label(
                PdfSpreadMode::OddSpreads,
                PdfScrollMode::PageScrolling,
                12,
                1,
                &[]
            ),
            "1 / 12"
        );
        // Wrapped lays pages out singly even with a pairing chosen.
        assert_eq!(
            format_location_label(
                PdfSpreadMode::OddSpreads,
                PdfScrollMode::WrappedScrolling,
                12,
                4,
                &[]
            ),
            "4 / 12"
        );
    }

    #[test]
    fn test_location_label_leads_with_the_documents_page_labels() {
        // Seven roman front-matter pages, then the text proper
        // restarts at 1 — the catalogue shape behind the design's
        // `iv · 8 / 612` example.
        let labels: Vec<String> = [
            "i", "ii", "iii", "iv", "v", "vi", "vii", "1", "2", "3", "4", "5",
        ]
        .iter()
        .map(|label| label.to_string())
        .collect();
        assert_eq!(
            format_location_label(
                PdfSpreadMode::NoSpreads,
                PdfScrollMode::VerticalScrolling,
                12,
                4,
                &labels
            ),
            "iv · 4 / 12"
        );
        // Both pages of a spread carry their labels.
        assert_eq!(
            format_location_label(
                PdfSpreadMode::OddSpreads,
                PdfScrollMode::PageScrolling,
                12,
                4,
                &labels
            ),
            "iv–v · 4–5 / 12"
        );
        // A restarted decimal sequence differs from the physical
        // page even though both are numerals.
        assert_eq!(
            format_location_label(
                PdfSpreadMode::NoSpreads,
                PdfScrollMode::VerticalScrolling,
                12,
                8,
                &labels
            ),
            "1 · 8 / 12"
        );
        // Labels that are the physical numbers keep the plain form.
        let physical: Vec<String> = (1..=12).map(|page| page.to_string()).collect();
        assert_eq!(
            format_location_label(
                PdfSpreadMode::OddSpreads,
                PdfScrollMode::PageScrolling,
                12,
                4,
                &physical
            ),
            "4–5 / 12"
        );
        // A short or blank label list falls back page by page.
        assert_eq!(displayed_page_label(&labels, 3), "iii");
        assert_eq!(displayed_page_label(&labels, 99), "99");
        assert_eq!(displayed_page_label(&[" ".to_string()], 1), "1");
        assert_eq!(displayed_page_label(&[], 0), "0");
    }

    #[test]
    fn test_pages_grid_rings_the_whole_current_spread() {
        assert_eq!(
            pdf_pages_current_pages(
                PdfSpreadMode::OddSpreads,
                PdfScrollMode::PageScrolling,
                12,
                5
            ),
            HashSet::from([4, 5])
        );
        assert_eq!(
            pdf_pages_current_pages(
                PdfSpreadMode::NoSpreads,
                PdfScrollMode::PageScrolling,
                12,
                5
            ),
            HashSet::from([5])
        );
        assert_eq!(
            pdf_pages_current_pages(
                PdfSpreadMode::OddSpreads,
                PdfScrollMode::WrappedScrolling,
                12,
                5
            ),
            HashSet::from([5]),
            "wrapped scrolling reads one page at a time"
        );
        assert!(pdf_pages_current_pages(
            PdfSpreadMode::OddSpreads,
            PdfScrollMode::PageScrolling,
            0,
            1
        )
        .is_empty());
    }

    #[test]
    fn test_pdf_jump_chapters_and_targets() {
        let entries = sample_toc();
        assert_eq!(pdf_jump_chapter_pages(&entries, 12), vec![1, 2, 8, 12]);
        assert_eq!(
            pdf_jump_current(PdfJumpMode::Chapter, &entries, 12, 4),
            2,
            "page 4 stands in the second top-level chapter"
        );
        assert_eq!(
            pdf_jump_target(PdfJumpMode::Chapter, "3", &entries, 12),
            Some(8)
        );
        assert_eq!(
            pdf_jump_target(PdfJumpMode::Chapter, "5", &entries, 12),
            None
        );
        assert_eq!(
            pdf_jump_target(PdfJumpMode::Page, "12", &entries, 12),
            Some(12)
        );
        assert_eq!(pdf_jump_target(PdfJumpMode::Page, "0", &entries, 12), None);
        assert_eq!(pdf_jump_target(PdfJumpMode::Page, "13", &entries, 12), None);
        assert_eq!(pdf_jump_target(PdfJumpMode::Page, "", &entries, 12), None);
    }

    #[test]
    fn test_contents_current_entry_and_visible_rows() {
        let entries = sample_toc();
        assert_eq!(current_toc_index(&entries, 5, 12), Some(3));
        assert_eq!(current_toc_index(&entries, 2, 12), Some(1));
        assert_eq!(
            toc_parent_indices(&entries),
            vec![None, None, Some(1), Some(1), None, Some(4), None]
        );

        let collapsed = HashSet::from([1]);
        let visible: Vec<usize> = visible_toc_rows(&entries, &collapsed, "")
            .iter()
            .map(|row| row.index)
            .collect();
        assert_eq!(visible, vec![0, 1, 4, 5, 6]);

        // A filter must not hide its own match inside a folded
        // branch: the ancestor rides along, the siblings do not.
        let filtered: Vec<usize> = visible_toc_rows(&entries, &collapsed, "plate")
            .iter()
            .map(|row| row.index)
            .collect();
        assert_eq!(filtered, vec![1, 3]);
    }

    #[test]
    fn test_pages_heading_names_the_top_level_span() {
        let entries = sample_toc();
        let heading = pages_heading_for_page(&entries, 12, 5, "The Living Earth");
        assert_eq!(heading.title, "The Earth");
        assert_eq!((heading.start_page, heading.end_page), (2, 7));
        assert_eq!(heading.page_count(), 6);
        assert!(heading.contains(4));
        assert!(!heading.contains(8));

        // The next root owns its own span, nested Currents and all.
        let heading = pages_heading_for_page(&entries, 12, 9, "The Living Earth");
        assert_eq!(heading.title, "Oceans");
        assert_eq!((heading.start_page, heading.end_page), (8, 11));

        // No outline: the book itself is the only honest heading.
        let heading = pages_heading_for_page(&[], 12, 5, "The Living Earth");
        assert_eq!(heading.title, "The Living Earth");
        assert_eq!((heading.start_page, heading.end_page), (1, 12));

        // Front matter before the first root gets the same
        // fallback, bounded by that first chapter.
        let late = vec![toc("Opening chapter", 4, 0), toc("Index", 12, 0)];
        let heading = pages_heading_for_page(&late, 12, 2, "Front Matter");
        assert_eq!(heading.title, "Front Matter");
        assert_eq!((heading.start_page, heading.end_page), (1, 3));
    }

    #[test]
    fn test_keep_window_pages_paged_mode() {
        // Paged reading keeps the current spread plus one spread each way.
        assert_eq!(
            window(PdfSpreadMode::NoSpreads, PdfScrollMode::PageScrolling, 20, 10, 1),
            vec![9, 10, 11]
        );
        // At the start of the book there is no previous spread.
        assert_eq!(
            window(PdfSpreadMode::NoSpreads, PdfScrollMode::PageScrolling, 20, 1, 1),
            vec![1, 2]
        );
        // At the end there is no next spread.
        assert_eq!(
            window(PdfSpreadMode::NoSpreads, PdfScrollMode::PageScrolling, 20, 20, 1),
            vec![19, 20]
        );
    }

    #[test]
    fn test_keep_window_pages_spreads_keep_both_halves() {
        // Odd spreads on page 4: the spread is (4, 5) and the window must
        // keep whole spreads, not loose pages.
        assert_eq!(
            window(
                PdfSpreadMode::OddSpreads,
                PdfScrollMode::VerticalScrolling,
                20,
                4,
                1
            ),
            vec![2, 3, 4, 5, 6, 7]
        );
    }

    #[test]
    fn test_keep_window_pages_text_window_is_wider_than_image_window() {
        // The text cache keeps twice the image window (2.20 design: text is
        // tens of KB per page, so its window may be wider -- but bounded).
        let image = window(
            PdfSpreadMode::NoSpreads,
            PdfScrollMode::VerticalScrolling,
            50,
            25,
            3,
        );
        let text = window(
            PdfSpreadMode::NoSpreads,
            PdfScrollMode::VerticalScrolling,
            50,
            25,
            6,
        );
        assert_eq!(image, vec![22, 23, 24, 25, 26, 27, 28]);
        assert_eq!(text, vec![19, 20, 21, 22, 23, 24, 25, 26, 27, 28, 29, 30, 31]);
        for p in &image {
            assert!(text.contains(p), "text window must cover the image window");
        }
    }

    #[test]
    fn test_keep_window_pages_wrapped_reads_as_single_pages() {
        // Wrapped scrolling has no spreads; the window must not pair pages.
        assert_eq!(
            window(
                PdfSpreadMode::OddSpreads,
                PdfScrollMode::WrappedScrolling,
                20,
                4,
                1
            ),
            vec![3, 4, 5]
        );
    }

    #[test]
    fn test_pending_selection_replays_when_text_arrives() {
        // A word click stored on page 5 replays as SelectionWordAt the
        // moment page 5's text lands (2.20 step 3: the first click is
        // never a dead click).
        let mut pending = Some((5usize, PdfPendingSelection::Word { slot: PageSlot::Fixed(5), x: 120.0, y: 40.0 }));
        match PdfReaderModel::take_pending_replay(&mut pending, 5, true) {
            PdfPendingOutcome::Replay(PdfReaderMsg::SelectionWordAt { x, y, .. }) => {
                assert_eq!((x, y), (120.0, 40.0));
            }
            other => panic!("expected a word replay, got {other:?}"),
        }
        assert!(pending.is_none(), "the intent must be consumed");
    }

    #[test]
    fn test_pending_selection_line_and_drag_replay() {
        // A line click (triple-click) replays as SelectionLineAt.
        let mut pending = Some((3usize, PdfPendingSelection::Line { slot: PageSlot::Fixed(3), x: 10.0, y: 20.0 }));
        match PdfReaderModel::take_pending_replay(&mut pending, 3, true) {
            PdfPendingOutcome::Replay(PdfReaderMsg::SelectionLineAt { x, y, .. }) => {
                assert_eq!((x, y), (10.0, 20.0));
            }
            other => panic!("expected a line replay, got {other:?}"),
        }

        // A drag that began on an unready page cannot be replayed as a
        // drag; the word at its start point is the honest answer.
        let mut pending = Some((7usize, PdfPendingSelection::Drag { slot: PageSlot::Fixed(7), x: 55.0, y: 66.0 }));
        match PdfReaderModel::take_pending_replay(&mut pending, 7, true) {
            PdfPendingOutcome::Replay(PdfReaderMsg::SelectionWordAt { x, y, .. }) => {
                assert_eq!((x, y), (55.0, 66.0));
            }
            other => panic!("expected a word replay for the drag, got {other:?}"),
        }
    }

    #[test]
    fn test_pending_selection_waits_for_its_own_page() {
        // Text arriving for a different page must not consume the intent.
        let mut pending = Some((5usize, PdfPendingSelection::Word { slot: PageSlot::Fixed(5), x: 1.0, y: 2.0 }));
        match PdfReaderModel::take_pending_replay(&mut pending, 9, true) {
            PdfPendingOutcome::KeepWaiting => {}
            other => panic!("expected KeepWaiting, got {other:?}"),
        }
        assert!(pending.is_some(), "the intent must survive other pages' text");
    }

    #[test]
    fn test_pending_selection_dropped_when_page_left_viewport() {
        // The user scrolled on: the intent is stale and must be dropped,
        // not replayed onto a page nobody is looking at.
        let mut pending = Some((5usize, PdfPendingSelection::Word { slot: PageSlot::Fixed(5), x: 1.0, y: 2.0 }));
        match PdfReaderModel::take_pending_replay(&mut pending, 5, false) {
            PdfPendingOutcome::Dropped => {}
            other => panic!("expected Dropped, got {other:?}"),
        }
        assert!(pending.is_none());
    }

    #[test]
    fn test_pending_selection_nothing_pending() {
        let mut pending: Option<(usize, PdfPendingSelection)> = None;
        match PdfReaderModel::take_pending_replay(&mut pending, 5, true) {
            PdfPendingOutcome::KeepWaiting => {}
            other => panic!("expected KeepWaiting, got {other:?}"),
        }
    }

    #[test]
    fn test_over_selection_handle_zones() {
        // The hover cursor must appear exactly where a press takes hold of
        // a grip: the end grip's zone is centred on its bar's bottom end
        // (the teardrop hangs below it), the start grip's on its top end.
        let sel = PdfActiveSelection {
            page: 1,
            slot: PageSlot::Fixed(1),
            text: "word".to_string(),
            screen_rects: Vec::new(),
            bounds: (0.0, 0.0, 10.0, 10.0),
            start_handle: (100.0, 200.0, 16.0),
            end_handle: (300.0, 220.0, 16.0),
            anchor_pt: (0.0, 0.0),
            active_pt: (0.0, 0.0),
            is_block: false,
            rotation: 0,
        };
        assert!(over_selection_handle(&sel, 300.0, 236.0), "end grip centre");
        assert!(over_selection_handle(&sel, 100.0, 200.0), "start grip centre");
        assert!(
            !over_selection_handle(&sel, 200.0, 220.0),
            "the middle of the page is no grip"
        );
        assert!(!over_selection_handle(&sel, 30.0, 40.0), "far from both");
    }

    #[test]
    fn test_calculate_spread_for_page_odd_spreads() {
        let total = 6;
        assert_eq!(calculate_spread_for_page(PdfSpreadMode::OddSpreads, total, 1), (1, None));
        assert_eq!(calculate_spread_for_page(PdfSpreadMode::OddSpreads, total, 2), (2, Some(3)));
        assert_eq!(calculate_spread_for_page(PdfSpreadMode::OddSpreads, total, 3), (2, Some(3)));
        assert_eq!(calculate_spread_for_page(PdfSpreadMode::OddSpreads, total, 4), (4, Some(5)));
        assert_eq!(calculate_spread_for_page(PdfSpreadMode::OddSpreads, total, 5), (4, Some(5)));
        assert_eq!(calculate_spread_for_page(PdfSpreadMode::OddSpreads, total, 6), (6, None));
    }

    #[test]
    fn test_calculate_spread_for_page_even_spreads() {
        let total = 5;
        assert_eq!(calculate_spread_for_page(PdfSpreadMode::EvenSpreads, total, 1), (1, Some(2)));
        assert_eq!(calculate_spread_for_page(PdfSpreadMode::EvenSpreads, total, 2), (1, Some(2)));
        assert_eq!(calculate_spread_for_page(PdfSpreadMode::EvenSpreads, total, 3), (3, Some(4)));
        assert_eq!(calculate_spread_for_page(PdfSpreadMode::EvenSpreads, total, 4), (3, Some(4)));
        assert_eq!(calculate_spread_for_page(PdfSpreadMode::EvenSpreads, total, 5), (5, None));
    }

    #[test]
    fn test_spread_chain_navigation() {
        for mode in [PdfSpreadMode::NoSpreads, PdfSpreadMode::OddSpreads, PdfSpreadMode::EvenSpreads] {
            for total in 1..=20 {
                let spreads = calculate_all_spreads(mode, total);
                assert!(!spreads.is_empty());

                // Forward traversal
                let mut forward = vec![spreads[0]];
                let mut curr = spreads[0];
                while let Some(next) = calculate_next_spread(mode, total, curr) {
                    forward.push(next);
                    curr = next;
                }
                assert_eq!(forward, spreads, "Forward chain mismatch for mode {:?}, total {}", mode, total);

                // Backward traversal
                let mut backward = vec![*spreads.last().unwrap()];
                let mut curr = *spreads.last().unwrap();
                while let Some(prev) = calculate_prev_spread(mode, total, curr) {
                    backward.push(prev);
                    curr = prev;
                }
                backward.reverse();
                assert_eq!(backward, spreads, "Backward chain mismatch for mode {:?}, total {}", mode, total);
            }
        }
    }

    #[test]
    fn test_prune_pair_integrity_invariant() {
        // Ensure that pruning never splits a 2-page spread: both pages must either be kept or evicted together.
        for mode in [PdfSpreadMode::OddSpreads, PdfSpreadMode::EvenSpreads] {
            for total in 1..=25 {
                let all_spreads = calculate_all_spreads(mode, total);
                for cur_page in 1..=total {
                    let cur_spread = calculate_spread_for_page(mode, total, cur_page);
                    let mut keep = std::collections::HashSet::new();
                    keep.insert(cur_spread.0);
                    if let Some(r) = cur_spread.1 {
                        keep.insert(r);
                    }

                    let mut next_cursor = cur_spread;
                    for _ in 0..3 {
                        if let Some(ns) = calculate_next_spread(mode, total, next_cursor) {
                            keep.insert(ns.0);
                            if let Some(r) = ns.1 {
                                keep.insert(r);
                            }
                            next_cursor = ns;
                        } else {
                            break;
                        }
                    }

                    let mut prev_cursor = cur_spread;
                    for _ in 0..3 {
                        if let Some(ps) = calculate_prev_spread(mode, total, prev_cursor) {
                            keep.insert(ps.0);
                            if let Some(r) = ps.1 {
                                keep.insert(r);
                            }
                            prev_cursor = ps;
                        } else {
                            break;
                        }
                    }

                    for spread in &all_spreads {
                        if let Some(r) = spread.1 {
                            let left_kept = keep.contains(&spread.0);
                            let right_kept = keep.contains(&r);
                            assert_eq!(
                                left_kept, right_kept,
                                "Spread pair {:?} split during pruning at page {} (total {}): left_kept={}, right_kept={}",
                                spread, cur_page, total, left_kept, right_kept
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_reading_axes_fold_onto_the_engine_modes() {
        assert_eq!(
            pdf_spread_mode_for_layout(PdfLayout::Single, true),
            PdfSpreadMode::NoSpreads
        );
        assert_eq!(
            pdf_spread_mode_for_layout(PdfLayout::Spread, true),
            PdfSpreadMode::OddSpreads
        );
        assert_eq!(
            pdf_spread_mode_for_layout(PdfLayout::Spread, false),
            PdfSpreadMode::EvenSpreads
        );
        assert_eq!(
            pdf_layout_for_spread_mode(PdfSpreadMode::NoSpreads),
            PdfLayout::Single
        );
        assert_eq!(
            pdf_layout_for_spread_mode(PdfSpreadMode::EvenSpreads),
            PdfLayout::Spread
        );
        // The pre-redesign flows stay readable on the new axis.
        assert_eq!(
            pdf_flow_for_scroll_mode(PdfScrollMode::HorizontalScrolling),
            PdfFlow::Scroll
        );
        assert_eq!(
            pdf_flow_for_scroll_mode(PdfScrollMode::WrappedScrolling),
            PdfFlow::Scroll
        );
        assert_eq!(
            pdf_scroll_mode_for_flow(PdfFlow::Scroll),
            PdfScrollMode::VerticalScrolling
        );
    }

    #[test]
    fn test_fit_modes_and_zoom_percent() {
        for fit in [
            PdfFitMode::Width,
            PdfFitMode::Height,
            PdfFitMode::Screen,
            PdfFitMode::Original,
        ] {
            assert_eq!(PdfFitMode::from_pref(Some(fit.pref_value())), Some(fit));
        }
        assert_eq!(PdfFitMode::from_pref(Some("sideways")), None);

        let zoom = |fit, spreads| pdf_fit_zoom(fit, 1200.0, 900.0, 500.0, 1000.0, spreads, 0);
        assert_eq!(zoom(PdfFitMode::Width, true), 1.2);
        assert_eq!(zoom(PdfFitMode::Width, false), 2.4);
        assert_eq!(zoom(PdfFitMode::Height, true), 0.9);
        assert_eq!(zoom(PdfFitMode::Screen, true), 0.9);
        assert_eq!(zoom(PdfFitMode::Original, true), 1.0);

        assert_eq!(pdf_effective_zoom(1.2, 150), 1.8);
        assert_eq!(pdf_effective_zoom(1.2, 10), 0.3);
        assert_eq!(pdf_effective_zoom(2.0, 400), 3.5);
    }

    #[test]
    fn test_auto_spread_needs_a_wide_enough_window() {
        assert!(pdf_viewport_fits_spread(1600.0, 1000.0, 420.0, 595.0, 0));
        assert!(!pdf_viewport_fits_spread(900.0, 1000.0, 420.0, 595.0, 0));
        assert!(!pdf_viewport_fits_spread(0.0, 1000.0, 420.0, 595.0, 0));
    }

    #[test]
    fn test_rotation_geometry_round_trips() {
        assert_eq!(normalize_pdf_rotation(-90), 270);
        assert_eq!(normalize_pdf_rotation(450), 90);
        assert_eq!(normalize_pdf_rotation(45), 0);
        assert_eq!(display_page_dimensions(420.0, 595.0, 90), (595.0, 420.0));
        assert_eq!(display_page_dimensions(420.0, 595.0, 180), (420.0, 595.0));

        // A source rectangle and its centre, quarter-turned both ways.
        let rect = (10.0, 20.0, 30.0, 60.0);
        let turned = source_rect_to_display(rect, 100.0, 200.0, 400.0, 200.0, 90);
        assert!((turned.0 - 280.0).abs() < 0.001, "{turned:?}");
        assert!((turned.1 - 20.0).abs() < 0.001, "{turned:?}");
        assert!((turned.2 - 80.0).abs() < 0.001, "{turned:?}");
        assert!((turned.3 - 40.0).abs() < 0.001, "{turned:?}");
        let back = display_point_to_source(320.0, 40.0, 400.0, 200.0, 100.0, 200.0, 90);
        assert!((back.0 - 20.0).abs() < 0.001 && (back.1 - 40.0).abs() < 0.001, "{back:?}");

        let back = display_point_to_source(80.0, 160.0, 400.0, 200.0, 100.0, 200.0, 270);
        assert!((back.0 - 20.0).abs() < 0.001 && (back.1 - 40.0).abs() < 0.001, "{back:?}");

        let flipped = source_rect_to_display(rect, 100.0, 200.0, 200.0, 400.0, 180);
        assert!((flipped.0 - 140.0).abs() < 0.001, "{flipped:?}");
        assert!((flipped.1 - 280.0).abs() < 0.001, "{flipped:?}");

        // On a quarter-turned page the bars run horizontally and the
        // grips hang from opposite ends of them.
        let sel = PdfActiveSelection {
            page: 1,
            slot: PageSlot::Fixed(1),
            text: "word".to_string(),
            screen_rects: Vec::new(),
            bounds: (0.0, 0.0, 10.0, 10.0),
            start_handle: (100.0, 200.0, 80.0),
            end_handle: (40.0, 300.0, 80.0),
            anchor_pt: (0.0, 0.0),
            active_pt: (0.0, 0.0),
            is_block: false,
            rotation: 90,
        };
        assert_eq!(
            selection_handle_at(&sel, 180.0, 200.0),
            Some(PdfHandleDrag::Start)
        );
        assert_eq!(
            selection_handle_at(&sel, 40.0, 300.0),
            Some(PdfHandleDrag::End)
        );
        assert_eq!(selection_handle_at(&sel, 100.0, 200.0), None);
    }

    #[test]
    fn test_load_queue_pair_integrity_invariant() {
        // Ensure that load queuing in 2-page modes always requests full spread pairs.
        for mode in [PdfSpreadMode::OddSpreads, PdfSpreadMode::EvenSpreads] {
            for total in 1..=25 {
                let all_spreads = calculate_all_spreads(mode, total);
                for cur_page in 1..=total {
                    let cur_spread = calculate_spread_for_page(mode, total, cur_page);
                    let mut spreads_to_load = vec![cur_spread];
                    let mut next_cursor = cur_spread;
                    let mut prev_cursor = cur_spread;
                    let mut has_next = true;
                    let mut has_prev = true;

                    for _ in 0..3 {
                        if has_next {
                            if let Some(ns) = calculate_next_spread(mode, total, next_cursor) {
                                spreads_to_load.push(ns);
                                next_cursor = ns;
                            } else {
                                has_next = false;
                            }
                        }
                        if has_prev {
                            if let Some(ps) = calculate_prev_spread(mode, total, prev_cursor) {
                                spreads_to_load.push(ps);
                                prev_cursor = ps;
                            } else {
                                has_prev = false;
                            }
                        }
                    }

                    let mut load_order = Vec::new();
                    for s in &spreads_to_load {
                        load_order.push(s.0);
                        if let Some(r) = s.1 {
                            load_order.push(r);
                        }
                    }

                    let queued: std::collections::HashSet<usize> = load_order.into_iter().collect();
                    for spread in &all_spreads {
                        if let Some(r) = spread.1 {
                            let left_queued = queued.contains(&spread.0);
                            let right_queued = queued.contains(&r);
                            assert_eq!(
                                left_queued, right_queued,
                                "Spread pair {:?} split in load queue at page {} (total {}): left_queued={}, right_queued={}",
                                spread, cur_page, total, left_queued, right_queued
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn test_pdf_open_states_say_their_designed_words() {
        assert!(!PdfOpenState::Opening.is_blocking());
        assert!(!PdfOpenState::Ready.is_blocking());

        let password = PdfOpenState::from_load_error(PdfLoadError::PasswordRequired);
        assert_eq!(
            password,
            PdfOpenState::Password {
                wrong: false,
                submitting: false,
            }
        );
        assert!(password.is_blocking());
        assert!(password.is_password());
        assert_eq!(password.icon_name(), "kalam-lock-symbolic");
        assert_eq!(password.title(), "This book is password protected");
        assert_eq!(
            password.body(),
            "Enter the password to open it. Kalam doesn’t store it."
        );

        let wrong = PdfOpenState::from_load_error(PdfLoadError::WrongPassword);
        assert!(wrong.password_wrong());
        assert!(!wrong.password_submitting());

        let missing = PdfOpenState::from_load_error(PdfLoadError::FileNotFound);
        assert_eq!(missing.icon_name(), "kalam-file-question-symbolic");
        assert_eq!(missing.title(), "File not found");
        assert_eq!(
            missing.body(),
            "Kalam can’t find this book at its saved location. \
             It may have been moved or deleted."
        );

        let cant_open = PdfOpenState::from_load_error(PdfLoadError::CantOpen);
        assert_eq!(cant_open.icon_name(), "kalam-file-warning-symbolic");
        assert_eq!(cant_open.title(), "This PDF can’t be opened");
        assert_eq!(
            cant_open.body(),
            "The file is damaged or isn’t a valid PDF."
        );
    }
}
