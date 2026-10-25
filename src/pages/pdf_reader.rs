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

use crate::db::{Catalog, ReadingBookmark};
use crate::epub_book::ReadingTheme;
use crate::pdf::{PdfDocument, PdfPageText, PdfSearchResult, PdfTocEntry};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PageSlot {
    Fixed(usize),
    PagedSingle,
    PagedSpreadLeft,
    PagedSpreadRight,
}

pub type PdfSearchHighlight = (PageSlot, usize, Vec<(f64, f64, f64, f64)>);

/// Which end of an existing selection a drag is taking hold of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfHandleDrag {
    Start,
    End,
}

/// Is (`x`, `y`) over one of the selection's handle grips -- the zone a
/// press there takes hold of? Mirrors the hit zones `SelectionDragBegin`
/// uses, so the hover cursor never promises a grab that will not happen.
fn over_selection_handle(sel: &PdfActiveSelection, x: f64, y: f64) -> bool {
    let (sx, sy, _) = sel.start_handle;
    let (ex, ey, eh) = sel.end_handle;
    ((x - ex).abs() < 24.0 && (y - (ey + eh)).abs() < 28.0)
        || ((x - sx).abs() < 24.0 && (y - sy).abs() < 28.0)
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
}

#[derive(Debug, Clone)]
pub struct PdfRenderRequest {
    pub generation: u64,
    pub page: usize,
    pub scale: f32,
    pub smart_crop: bool,
    pub path: PathBuf,
}

/// Document facts gathered once, off the UI thread, by the background doc
/// service (2.20: the UI thread never opens, parses or searches the file).
#[derive(Debug)]
pub struct PdfDocInfo {
    pub total_pages: usize,
    pub toc_entries: Vec<PdfTocEntry>,
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

/// The location text for one reading position. Wrapped scrolling
/// lays pages out one by one even when a spread pairing is chosen,
/// so it names one page, exactly as the viewport does.
fn format_location_label(
    spread_mode: PdfSpreadMode,
    scroll_mode: PdfScrollMode,
    total_pages: usize,
    current_page: usize,
) -> String {
    if total_pages == 0 {
        return "0 / 0".to_string();
    }
    let current = current_page.clamp(1, total_pages);
    if spread_mode == PdfSpreadMode::NoSpreads
        || scroll_mode == PdfScrollMode::WrappedScrolling
    {
        return format!("{current} / {total_pages}");
    }
    let (left, right) = calculate_spread_for_page(spread_mode, total_pages, current);
    match right {
        Some(right) => format!("{left}–{right} / {total_pages}"),
        None => format!("{left} / {total_pages}"),
    }
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
    Bookmarks,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfRightTab {
    Reading,
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
    pub scroll_page_btn: gtk::Button,
    pub scroll_vertical_btn: gtk::Button,
    pub scroll_horizontal_btn: gtk::Button,
    pub scroll_wrapped_btn: gtk::Button,
    pub spread_none_btn: gtk::Button,
    pub spread_odd_btn: gtk::Button,
    pub spread_even_btn: gtk::Button,
    pub gap_0_btn: gtk::Button,
    pub gap_4_btn: gtk::Button,
    pub gap_8_btn: gtk::Button,
    pub gap_12_btn: gtk::Button,
    pub gap_16_btn: gtk::Button,
    pub spread_gap_box: gtk::Box,
    pub smart_crop_switch: gtk::Switch,
    pub zoom_label: gtk::Label,
}

#[derive(Debug)]
pub enum PdfReaderMsg {
    NextPage,
    PrevPage,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    FitToPage,
    FitToWidth,
    GoToFirstPage,
    GoToLastPage,
    SetScrollMode(PdfScrollMode),
    SetSpreadMode(PdfSpreadMode),
    SetSpreadGap(i32),
    ToggleBookmark,
    DeleteBookmark(i64),
    JumpToPage(usize),
    SetSmartCrop(bool),
    ToggleSmartCrop,
    SetLeftTab(PdfLeftTab),
    SetRightTab(PdfRightTab),
    OpenLeftPanel,
    OpenRightPanel,
    OpenContents,
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
    NextSearchResult,
    PrevSearchResult,
    JumpToSearchResult(usize),
    CloseSearch,
    PageRendered {
        generation: u64,
        page: usize,
        texture: gdk::Texture,
        width: i32,
        height: i32,
    },
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
        info: Box<Result<PdfDocInfo, String>>,
    },
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
        results: Vec<PdfSearchResult>,
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
    pub smart_crop: bool,
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
    pub bookmarks: Vec<ReadingBookmark>,
    pub textures: HashMap<usize, CachedPageTexture>,
    pub pending_loads: HashSet<usize>,
    pub arrow_step: f32,
    pub is_loading: bool,
    pub status_text: String,
    pub session_id: Option<i64>,
    pub session_start: std::time::Instant,
    pub page_pictures: HashMap<usize, gtk::Picture>,
    pub paged_picture: Option<gtk::Picture>,
    pub paged_loading_box: Option<gtk::Box>,
    pub paged_label: Option<gtk::Label>,
    pub two_page_left_pic: Option<gtk::Picture>,
    pub two_page_right_pic: Option<gtk::Picture>,
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
    pub bookmarks_list_box: Option<gtk::Box>,
    pub settings_widgets: Option<PdfSettingsWidgets>,
    pub screenshot_action_applied: bool,
    pub render_tx: Option<async_channel::Sender<PdfRenderRequest>>,
    pub render_generation: u64,
    pub active_generation: Option<Arc<AtomicU64>>,
    pub base_page_width: f64,
    pub base_page_height: f64,
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
    pub search_index: usize,
    pub search_popover: Option<gtk::Popover>,
    pub search_list_box: Option<gtk::ListBox>,
    pub search_popover_badge: Option<gtk::Label>,
    pub search_highlight: std::rc::Rc<std::cell::RefCell<Option<PdfSearchHighlight>>>,
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

        // 2. Spread Mode: Check per-book setting first, fallback to global default, fallback to NoSpreads
        let book_spread = init.catalog.get_pref(&format!("book.{}.pdf.spread_mode", init.book_id));
        let global_spread = init.catalog.get_pref("reader.pdf.spread_mode");
        let spread_mode = match book_spread.as_deref().or(global_spread.as_deref()) {
            Some("odd") => PdfSpreadMode::OddSpreads,
            Some("even") => PdfSpreadMode::EvenSpreads,
            _ => PdfSpreadMode::NoSpreads,
        };

        // 3. Zoom level: specific to this document
        let book_zoom = init.catalog.get_pref(&format!("book.{}.pdf.zoom", init.book_id));
        let zoom_level = book_zoom
            .and_then(|z| z.parse::<f64>().ok())
            .unwrap_or(1.0)
            .clamp(0.4, 3.0);

        // 4. Two-page spread gap: global default
        let two_page_gap = (init.catalog.get_pref_i64("reader.pdf.two_page_gap", 12) as i32).clamp(0, 48);

        // 5. Smart crop: per-book setting
        let smart_crop_pref = init
            .catalog
            .get_pref(&format!("book.{}.pdf.smart_crop", init.book_id));
        let smart_crop = smart_crop_pref.as_deref() == Some("1");

        // 6. Panel pins are remembered per side (decision 57's
        // approved assumption). They describe the next open as much
        // as this one: a pinned side reopens as page layout, not as
        // a floating layer.
        let left_panel_pinned =
            init.catalog.get_pref_i64("reader.pdf.left_panel_pinned", 0) != 0;
        let right_panel_pinned =
            init.catalog.get_pref_i64("reader.pdf.right_panel_pinned", 0) != 0;

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
            smart_crop,
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
            bookmarks: Vec::new(),
            textures: HashMap::new(),
            pending_loads: HashSet::new(),
            arrow_step,
            is_loading: true,
            status_text: "Opening PDF...".to_string(),
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
            bookmarks_list_box: None,
            settings_widgets: None,
            screenshot_action_applied: false,
            render_tx: None,
            render_generation: 1,
            active_generation: None,
            base_page_width: 595.0,
            base_page_height: 842.0,
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
            ocr_tx: None,
            ocr_in_progress: HashSet::new(),
            ocr_started_fp: HashMap::new(),
            show_zoom_osd: false,
            zoom_osd_seq: 0,
            search_active: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_index: 0,
            search_popover: None,
            search_list_box: None,
            search_popover_badge: None,
            search_highlight: std::rc::Rc::new(std::cell::RefCell::new(None)),
        };

        model.reload_bookmarks();

        model
    }

    pub fn reload_bookmarks(&mut self) {
        self.bookmarks = self
            .catalog
            .list_reading_bookmarks(self.book_id)
            .unwrap_or_default();
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
    /// a spread, or the one page in front of the reader.
    pub fn location_label(&self) -> String {
        format_location_label(
            self.spread_mode,
            self.scroll_mode,
            self.total_pages,
            self.current_page,
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

    pub fn compute_fit_zoom(&self, scroll: &gtk::ScrolledWindow, fit_width_only: bool) -> f64 {
        let win_w = scroll.width() as f64;
        let win_h = scroll.height() as f64;
        let w = if win_w > 100.0 { win_w } else { 850.0 };
        let h = if win_h > 100.0 { win_h } else { 900.0 };

        // Available area inside viewport, accounting for side padding and scrollbars
        let avail_w = (w - 48.0).max(100.0);
        let avail_h = (h - 72.0).max(100.0);

        let base_w = if self.base_page_width > 10.0 { self.base_page_width } else { 595.0 };
        let base_h = if self.base_page_height > 10.0 { self.base_page_height } else { 842.0 };

        let spreads_active = self.spread_mode != PdfSpreadMode::NoSpreads
            && self.scroll_mode != PdfScrollMode::WrappedScrolling;

        let needed_w = if spreads_active {
            (base_w * 2.0) + (self.two_page_gap as f64)
        } else {
            base_w
        };

        let zoom_w = avail_w / needed_w;
        let fit = if fit_width_only {
            zoom_w
        } else {
            let zoom_h = avail_h / base_h;
            zoom_w.min(zoom_h)
        };
        (fit.clamp(0.2, 3.5) * 100.0).round() / 100.0
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
                pic.set_paintable(None::<&gdk::Texture>);
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
        for pic in self.page_pictures.values() {
            pic.set_paintable(None::<&gdk::Texture>);
        }
        if let Some(ref pic) = self.paged_picture {
            pic.set_paintable(None::<&gdk::Texture>);
        }
        if let Some(ref pic) = self.two_page_left_pic {
            pic.set_paintable(None::<&gdk::Texture>);
        }
        if let Some(ref pic) = self.two_page_right_pic {
            pic.set_paintable(None::<&gdk::Texture>);
        }

        self.textures.clear();
        self.page_pictures.clear();
        self.page_overlays.clear();
        self.page_draw_areas.clear();
        self.page_text_cache.clear();
        self.pending_loads.clear();
        self.ocr_in_progress.clear();
        self.ocr_started_fp.clear();
        self.text_in_progress.clear();
        self.pending_selection = None;
        self.dismiss_recognizing_popover();
        self.doc_ready = false;

        self.doc_tx = None;
        self.render_tx = None;
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
        let smart_crop = self.smart_crop;
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

    fn wrap_page(
        &mut self,
        slot: PageSlot,
        pic: gtk::Picture,
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
        overlay.set_cursor_from_name(Some("text"));

        let draw_area = gtk::DrawingArea::new();
        draw_area.set_can_target(false);

        let active_sel_ref = self.active_selection.clone();
        let search_hl_ref = self.search_highlight.clone();
        let slot_copy = slot;
        let sel_handle_color = self.handle_color();
        draw_area.set_draw_func(move |_, cr, _w, _h| {
            // 1. Draw search match highlight (soft golden glow)
            if let Some((sel_slot, _page, ref s_rects)) = *search_hl_ref.borrow() {
                if sel_slot == slot_copy && !s_rects.is_empty() {
                    // Soft golden fill: rgba(244, 211, 94, 0.45)
                    cr.set_source_rgba(244.0 / 255.0, 211.0 / 255.0, 94.0 / 255.0, 0.45);
                    for &(rx, ry, rw, rh) in s_rects {
                        cr.rectangle(rx, ry, rw, rh);
                        let _ = cr.fill();
                    }
                    // Golden border outline: rgba(217, 119, 6, 0.85)
                    cr.set_source_rgba(217.0 / 255.0, 119.0 / 255.0, 6.0 / 255.0, 0.85);
                    cr.set_line_width(1.5);
                    for &(rx, ry, rw, rh) in s_rects {
                        cr.rectangle(rx, ry, rw, rh);
                        let _ = cr.stroke();
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
                        let (sx, sy, sh) = sel.start_handle;
                        cr.rectangle(sx - 1.0, sy, 2.0, sh);
                        let _ = cr.fill();
                        draw_teardrop(cr, sx, sy + 1.0, true);
                        let _ = cr.fill();

                        let (ex, ey, eh) = sel.end_handle;
                        cr.rectangle(ex - 1.0, ey, 2.0, eh);
                        let _ = cr.fill();
                        draw_teardrop(cr, ex, ey + eh - 1.0, false);
                        let _ = cr.fill();
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
        let multi_click = std::rc::Rc::new(std::cell::Cell::new(0i32));
        let mc_for_click = multi_click.clone();
        click.connect_pressed(move |_, n_press, x, y| {
            mc_for_click.set(n_press);
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
        let cursor_overlay = overlay.clone();
        let cursor_dragging = self.handle_dragging.clone();
        motion.connect_motion(move |_, x, y| {
            // A grip is being dragged: the closed hand stays until the
            // release, like the EPUB reader.
            if cursor_dragging.get() {
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
            leave_overlay.set_cursor_from_name(Some("text"));
        });
        overlay.add_controller(motion);

        // Drag controller for range and block selection (Alt+Drag for block marquee)
        let drag = gtk::GestureDrag::new();
        drag.set_button(1);
        let tx_drag_begin = sender.input_sender().clone();
        let tx_drag_update = sender.input_sender().clone();
        let tx_drag_end = sender.input_sender().clone();

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
            let is_block = drag_ctrl_up.current_event_state().contains(gdk::ModifierType::ALT_MASK);
            let _ = tx_drag_update.send(PdfReaderMsg::SelectionDragUpdate { slot: slot_copy, dx, dy, is_block });
        });
        let mc_for_end = multi_click.clone();
        drag.connect_drag_end(move |_, dx, dy| {
            // The live press count at release time, then reset: a
            // multi-click's release belongs to the click gesture, never
            // the drag (the EPUB reader gates its drag the same way).
            let press_count = mc_for_end.replace(0);
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

                        let pic = gtk::Picture::new();
                        pic.set_can_shrink(true);
                        pic.set_content_fit(gtk::ContentFit::Contain);
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
                            pic.set_paintable(Some(&cached.texture));
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

                        let left_pic = gtk::Picture::new();
                        left_pic.set_can_shrink(true);
                        left_pic.set_content_fit(gtk::ContentFit::Contain);
                        left_pic.set_halign(if right.is_some() { gtk::Align::End } else { gtk::Align::Center });
                        left_pic.set_valign(gtk::Align::Start);
                        left_pic.set_size_request(page_w, page_h);
                        left_pic.add_css_class("kalam-pdf-page-image");
                        left_pic.add_css_class("kalam-pdf-two-page");

                        let right_pic = gtk::Picture::new();
                        right_pic.set_can_shrink(true);
                        right_pic.set_content_fit(gtk::ContentFit::Contain);
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
                                left_pic.set_paintable(Some(&cached.texture));
                                left_pic.set_visible(true);
                            }
                            if right.is_some() {
                                if let Some(cached) = right_loaded {
                                    right_pic.set_paintable(Some(&cached.texture));
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
                let container = gtk::Box::new(gtk::Orientation::Vertical, 20);
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

                            let pic = gtk::Picture::new();
                            pic.set_can_shrink(true);
                            pic.set_content_fit(gtk::ContentFit::Contain);
                            pic.set_halign(gtk::Align::Center);
                            pic.set_valign(gtk::Align::Start);
                            pic.set_size_request(page_w, page_h);

                            if let Some(cached) = self.textures.get(&p) {
                                pic.set_paintable(Some(&cached.texture));
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

                            let left_pic = gtk::Picture::new();
                            left_pic.set_can_shrink(true);
                            left_pic.set_content_fit(gtk::ContentFit::Contain);
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
                                    left_pic.set_paintable(Some(&cached.texture));
                                    left_pic.add_css_class("kalam-pdf-page-image");
                                }
                            } else {
                                left_pic.add_css_class("kalam-pdf-placeholder");
                            }

                            self.page_pictures.insert(left, left_pic.clone());
                            let left_widget = self.wrap_page(PageSlot::Fixed(left), left_pic, sender);
                            spread_inner.append(&left_widget);

                            if let Some(r) = right {
                                let right_pic = gtk::Picture::new();
                                right_pic.set_can_shrink(true);
                                right_pic.set_content_fit(gtk::ContentFit::Contain);
                                right_pic.set_halign(gtk::Align::Start);
                                right_pic.set_valign(gtk::Align::Start);
                                right_pic.set_size_request(page_w, page_h);
                                right_pic.add_css_class("kalam-pdf-two-page");

                                if spread_ready {
                                    if let Some(cached) = self.textures.get(&r) {
                                        right_pic.set_paintable(Some(&cached.texture));
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
                let container = gtk::Box::new(gtk::Orientation::Horizontal, 24);
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
                            let pic = gtk::Picture::new();
                            pic.set_can_shrink(true);
                            pic.set_content_fit(gtk::ContentFit::Contain);
                            pic.set_valign(gtk::Align::Start);
                            pic.set_size_request(page_w, page_h);

                            if let Some(cached) = self.textures.get(&p) {
                                pic.set_paintable(Some(&cached.texture));
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

                            let left_pic = gtk::Picture::new();
                            left_pic.set_can_shrink(true);
                            left_pic.set_content_fit(gtk::ContentFit::Contain);
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
                                    left_pic.set_paintable(Some(&cached.texture));
                                    left_pic.add_css_class("kalam-pdf-page-image");
                                }
                            } else {
                                left_pic.add_css_class("kalam-pdf-placeholder");
                            }

                            self.page_pictures.insert(left, left_pic.clone());
                            let left_widget = self.wrap_page(PageSlot::Fixed(left), left_pic, sender);
                            spread_box.append(&left_widget);

                            if let Some(r) = right {
                                let right_pic = gtk::Picture::new();
                                right_pic.set_can_shrink(true);
                                right_pic.set_content_fit(gtk::ContentFit::Contain);
                                right_pic.set_valign(gtk::Align::Start);
                                right_pic.set_size_request(page_w, page_h);
                                right_pic.add_css_class("kalam-pdf-two-page");

                                if spread_ready {
                                    if let Some(cached) = self.textures.get(&r) {
                                        right_pic.set_paintable(Some(&cached.texture));
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
                flow_box.set_column_spacing(20);
                flow_box.set_row_spacing(20);
                flow_box.set_selection_mode(gtk::SelectionMode::None);

                let thumb_w = (self.base_page_width * 0.6 * self.zoom_level) as i32;
                let thumb_h = (self.base_page_height * 0.6 * self.zoom_level) as i32;

                for p in 1..=self.total_pages {
                    let pic = gtk::Picture::new();
                    pic.set_can_shrink(true);
                    pic.set_content_fit(gtk::ContentFit::Contain);
                    pic.set_valign(gtk::Align::Start);
                    pic.set_size_request(thumb_w, thumb_h);

                    if let Some(cached) = self.textures.get(&p) {
                        pic.set_paintable(Some(&cached.texture));
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

        let page_w = (self.base_page_width * self.zoom_level) as i32;
        let page_h = (self.base_page_height * self.zoom_level) as i32;
        pic.set_size_request(page_w, page_h);

        if let Some(cached) = self.textures.get(&self.current_page) {
            pic.set_paintable(Some(&cached.texture));
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
                left_pic.set_paintable(Some(&cached.texture));
                left_pic.set_halign(if right.is_some() { gtk::Align::End } else { gtk::Align::Center });
                left_pic.set_valign(gtk::Align::Start);
                left_pic.set_visible(true);
            }
            if right.is_some() {
                if let Some(cached) = right_loaded {
                    right_pic.set_paintable(Some(&cached.texture));
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

    /// Synchronize settings controls with active state.
    pub fn sync_settings_ui(&self, sw: &PdfSettingsWidgets) {
        toggle_active(&sw.scroll_page_btn, self.scroll_mode == PdfScrollMode::PageScrolling);
        toggle_active(&sw.scroll_vertical_btn, self.scroll_mode == PdfScrollMode::VerticalScrolling);
        toggle_active(&sw.scroll_horizontal_btn, self.scroll_mode == PdfScrollMode::HorizontalScrolling);
        toggle_active(&sw.scroll_wrapped_btn, self.scroll_mode == PdfScrollMode::WrappedScrolling);

        toggle_active(&sw.spread_none_btn, self.spread_mode == PdfSpreadMode::NoSpreads);
        toggle_active(&sw.spread_odd_btn, self.spread_mode == PdfSpreadMode::OddSpreads);
        toggle_active(&sw.spread_even_btn, self.spread_mode == PdfSpreadMode::EvenSpreads);

        sw.spread_gap_box.set_visible(true);
        sw.spread_gap_box.set_sensitive(self.spread_mode != PdfSpreadMode::NoSpreads);
        toggle_active(&sw.gap_0_btn, self.two_page_gap == 0);
        toggle_active(&sw.gap_4_btn, self.two_page_gap == 4);
        toggle_active(&sw.gap_8_btn, self.two_page_gap == 8);
        toggle_active(&sw.gap_12_btn, self.two_page_gap == 12);
        toggle_active(&sw.gap_16_btn, self.two_page_gap == 16);

        if sw.smart_crop_switch.is_active() != self.smart_crop {
            sw.smart_crop_switch.set_active(self.smart_crop);
        }

        sw.zoom_label.set_label(&format!("{}%", (self.zoom_level * 100.0).round() as i32));
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
            #[name = "viewport_scroll"]
            gtk::ScrolledWindow {
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
                set_reveal_child: model.show_back_button && !model.left_panel_open,
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
                set_visible: !(model.right_panel_open && model.right_panel_pinned),
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
                set_visible: !(model.right_panel_open && model.right_panel_pinned),
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

            // ── 8. The left panel: Contents and Bookmarks ───────────
            // Floating 16 from every edge (decision 52); the tab
            // pill and pin ride at the top (decision 57). Pages and
            // Notes join this panel in their own tranches.
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

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        set_margin_start: 20,
                        set_margin_end: 20,
                        set_margin_top: 16,

                        #[name = "left_tabs_host"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_hexpand: true,
                        },

                        #[name = "left_pin_button"]
                        gtk::Button {
                            set_child: Some(&crate::icons::symbolic_with_classes(
                                "kalam-pin-symbolic",
                                16,
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
                                16,
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

            // ── Floating Top-Right Search Bar ──────────────────────────
            add_overlay = &gtk::Revealer {
                add_css_class: "kalam-reader-search-shell",
                #[watch]
                set_reveal_child: model.search_active,
                set_transition_type: gtk::RevealerTransitionType::SlideDown,
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Start,
                set_margin_top: 14,
                set_margin_end: 24,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    add_css_class: "kalam-reader-search-bar",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 6,

                    #[name = "search_entry"]
                    gtk::SearchEntry {
                        set_placeholder_text: Some("Search in document..."),
                        set_width_request: 220,
                        connect_search_changed[sender] => move |entry| {
                            sender.input(PdfReaderMsg::UpdateSearchQuery(entry.text().to_string()));
                        },
                        connect_activate[sender] => move |_| {
                            sender.input(PdfReaderMsg::NextSearchResult);
                        },
                    },

                    #[name = "search_count_btn"]
                    gtk::MenuButton {
                        add_css_class: "kalam-reader-search-count-btn",
                        set_tooltip_text: Some("Matches at a glance"),
                        #[wrap(Some)]
                        set_child = &gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 4,

                            #[name = "search_count_label"]
                            gtk::Label {
                                add_css_class: "kalam-reader-search-count",
                                #[watch]
                                set_label: &if model.search_query.is_empty() {
                                    String::new()
                                } else if model.search_results.is_empty() {
                                    "0 matches".to_string()
                                } else {
                                    format!("{} of {}", model.search_index + 1, model.search_results.len())
                                },
                            },

                            gtk::Label {
                                add_css_class: "kalam-reader-search-chevron",
                                set_label: "▾",
                            },
                        },
                    },

                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes("go-up-symbolic", 14, &["kalam-inline-icon"])),
                        add_css_class: "kalam-reader-search-btn",
                        set_tooltip_text: Some("Previous match (Shift+Enter)"),
                        connect_clicked => PdfReaderMsg::PrevSearchResult,
                    },

                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes("go-down-symbolic", 14, &["kalam-inline-icon"])),
                        add_css_class: "kalam-reader-search-btn",
                        set_tooltip_text: Some("Next match (Enter)"),
                        connect_clicked => PdfReaderMsg::NextSearchResult,
                    },

                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes("window-close-symbolic", 14, &["kalam-inline-icon"])),
                        add_css_class: "kalam-reader-search-btn",
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
                    set_label: &format!("{:.0}%", model.zoom_level * 100.0),
                },
            },
        }
    }
    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = Self::new(init);
        let widgets = view_output!();

        // 1. The left panel stack: Contents (filter over the
        // outline) and the stored Bookmarks list. Pages joins this
        // stack in its own tranche; Notes replaces neither list
        // until its data model lands.
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

        let bookmarks_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .build();
        bookmarks_scroll.add_css_class("kalam-reader-panel-scroll");
        let bookmarks_list_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        bookmarks_list_box.set_hexpand(true);
        bookmarks_list_box.set_vexpand(true);
        bookmarks_scroll.set_child(Some(&bookmarks_list_box));

        left_stack.add_named(&contents_pane, Some("contents"));
        left_stack.add_named(&bookmarks_scroll, Some("bookmarks"));
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
        right_stack.add_named(&shortcuts_scroll, Some("shortcuts"));
        widgets.right_panel_host.append(&right_stack);

        model.left_stack = Some(left_stack);
        model.right_stack = Some(right_stack);
        model.toc_list_box = Some(toc_list_box);
        model.toc_scroll = Some(toc_scroll);
        model.bookmarks_list_box = Some(bookmarks_list_box);
        model.settings_widgets = Some(settings_w);

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
                &["Contents", "Bookmarks"],
                &widgets.left_tabs_host,
                Rc::new(move |idx| {
                    let _ = tx.send(PdfReaderMsg::SetLeftTab(if idx == 1 {
                        PdfLeftTab::Bookmarks
                    } else {
                        PdfLeftTab::Contents
                    }));
                }),
            ));
            let tx = sender.input_sender().clone();
            model.right_tabs = Some(make_tabs(
                &["Reading", "Shortcuts"],
                &widgets.right_tabs_host,
                Rc::new(move |idx| {
                    let _ = tx.send(PdfReaderMsg::SetRightTab(if idx == 1 {
                        PdfRightTab::Shortcuts
                    } else {
                        PdfRightTab::Reading
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
                Key::s | Key::S => {
                    let _ = tx_key.send(PdfReaderMsg::OpenReadingPanel);
                    gtk::glib::Propagation::Stop
                }
                Key::h | Key::H => {
                    let _ = tx_key.send(PdfReaderMsg::OpenShortcutsPanel);
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
            root_motion.connect_motion(move |_, x, y| {
                let in_zone = x < 180.0 && y < 68.0;
                if in_zone != was_in_zone_move.get() {
                    was_in_zone_move.set(in_zone);
                    let _ = tx_rm.send(PdfReaderMsg::BackZoneHover(in_zone));
                }
            });
            let tx_leave = tx.clone();
            root_motion.connect_leave(move |_| {
                if was_in_zone.get() {
                    was_in_zone.set(false);
                    let _ = tx_leave.send(PdfReaderMsg::BackZoneHover(false));
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
        widgets.viewport_scroll.set_child(Some(&child));
        model.update_scroll_policies(&widgets.viewport_scroll);

        // Start bounded background rendering workers (exactly 2 threads for lifetime of reader)
        let (render_tx, render_rx) = async_channel::unbounded::<PdfRenderRequest>();
        let active_gen = Arc::new(AtomicU64::new(1));

        for worker_id in 0..2 {
            let rx = render_rx.clone();
            let gen = active_gen.clone();
            let tx_msg = sender.input_sender().clone();

            let _ = std::thread::Builder::new()
                .name(format!("kalam-pdf-worker-{}", worker_id))
                .spawn(move || {
                    let mut cached_path: Option<PathBuf> = None;
                    let mut cached_doc: Option<PdfDocument> = None;

                    while let Ok(req) = rx.recv_blocking() {
                        if req.generation != gen.load(Ordering::Relaxed) {
                            continue;
                        }

                        if cached_path.as_ref() != Some(&req.path) {
                            cached_path = Some(req.path.clone());
                            cached_doc = PdfDocument::open(&req.path).ok();
                        }

                        if let Some(ref doc) = cached_doc {
                            if let Ok(rendered) = doc.render_page_rgba(req.page, req.scale, req.smart_crop) {
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

        // Start background OCR worker thread for scanned PDF pages
        let (ocr_tx, ocr_rx) = async_channel::unbounded::<crate::ocr::PdfOcrRequest>();
        let tx_ocr_msg = sender.input_sender().clone();

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

                    if cached_path.as_ref() != Some(&req.path) {
                        cached_path = Some(req.path.clone());
                        cached_doc = PdfDocument::open(&req.path).ok();
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
        let _ = std::thread::Builder::new()
            .name("kalam-pdf-doc-service".to_string())
            .spawn(move || {
                let mut cached_path: Option<PathBuf> = None;
                let mut cached_doc: Option<PdfDocument> = None;
                while let Ok(req) = doc_rx.recv_blocking() {
                    let path = match &req {
                        PdfDocRequest::Info { path, .. }
                        | PdfDocRequest::ExtractText { path, .. }
                        | PdfDocRequest::Search { path, .. } => path.clone(),
                    };
                    if cached_path.as_ref() != Some(&path) {
                        crate::timing::span("pdf_open_doc");
                        cached_path = Some(path.clone());
                        cached_doc = PdfDocument::open(&path).ok();
                        crate::timing::span_end("pdf_open_doc");
                    }
                    let Some(ref doc) = cached_doc else {
                        let _ = tx_doc_msg.send(PdfReaderMsg::DocumentLoaded {
                            info: Box::new(Err("Failed to open PDF document".to_string())),
                        });
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
                            let results = doc.search_document(&query, &toc, 500);
                            let _ = tx_doc_msg.send(PdfReaderMsg::SearchReady { query, results });
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
                info: Box::new(Err("PDF file not found".to_string())),
            });
        }

        let cur = model.current_page;
        let _ = model.ensure_page_text(cur);

        if model.doc_ready {
            model.trigger_loads(&sender);
        }
        model.update_bookmark_icon_state(&widgets);

        let (search_pop, search_lb, search_badge) = build_pdf_search_snippets_popover();
        widgets.search_count_btn.set_popover(Some(&search_pop));
        model.search_popover = Some(search_pop);
        model.search_list_box = Some(search_lb);
        model.search_popover_badge = Some(search_badge);

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
                s_entry.input(PdfReaderMsg::CloseSearch);
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
        widgets.search_entry.add_controller(entry_key);

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
            PdfReaderMsg::EscapeKey => {
                // One layer at a time (decision 52): search, the
                // jump field, a selection, the floating panels,
                // then the book. A pinned panel is page layout, not
                // a layer, so it does not spend an Escape.
                if self.search_active {
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
                self.search_active = !self.search_active;
                if !self.search_active {
                    self.search_query.clear();
                    self.search_results.clear();
                    self.search_index = 0;
                    self.search_highlight.replace(None);
                    for da in self.page_draw_areas.values() {
                        da.queue_draw();
                    }
                    if let Some(popover) = &self.search_popover {
                        popover.popdown();
                    }
                    _root.grab_focus();
                } else {
                    self.jump_open = false;
                    let entry = widgets.search_entry.clone();
                    glib::idle_add_local_once(move || {
                        entry.grab_focus();
                        entry.select_region(0, -1);
                    });
                }
            }
            PdfReaderMsg::UpdateSearchQuery(query) => {
                self.search_query = query.clone();
                self.search_index = 0;
                if query.trim().is_empty() {
                    self.search_results.clear();
                    self.highlight_current_search_match(widgets);
                    self.update_search_snippets_popover(&sender);
                } else if let (Some(path), Some(tx)) =
                    (self.file_path.clone(), self.doc_tx.as_ref())
                {
                    // Search runs on the doc service; results arrive in
                    // SearchReady and are applied only if the query is
                    // still current (stale answers are dropped silently).
                    let _ = tx.send_blocking(PdfDocRequest::Search {
                        path,
                        query,
                        toc: self.toc_entries.clone(),
                    });
                }
            }
            PdfReaderMsg::SearchReady { query, results } => {
                if query == self.search_query {
                    self.search_results = results;
                    self.search_index = 0;
                    self.highlight_current_search_match(widgets);
                    self.update_search_snippets_popover(&sender);
                }
            }
            PdfReaderMsg::NextSearchResult => {
                if !self.search_results.is_empty() {
                    self.search_index = (self.search_index + 1) % self.search_results.len();
                    self.highlight_current_search_match(widgets);
                    self.update_search_snippets_popover(&sender);
                }
            }
            PdfReaderMsg::PrevSearchResult => {
                if !self.search_results.is_empty() {
                    if self.search_index == 0 {
                        self.search_index = self.search_results.len() - 1;
                    } else {
                        self.search_index -= 1;
                    }
                    self.highlight_current_search_match(widgets);
                    self.update_search_snippets_popover(&sender);
                }
            }
            PdfReaderMsg::JumpToSearchResult(idx) => {
                if idx < self.search_results.len() {
                    self.search_index = idx;
                    self.highlight_current_search_match(widgets);
                    self.update_search_snippets_popover(&sender);
                    if let Some(popover) = &self.search_popover {
                        popover.popdown();
                    }
                }
            }
            PdfReaderMsg::CloseSearch => {
                self.search_active = false;
                self.search_query.clear();
                self.search_results.clear();
                self.search_index = 0;
                self.search_highlight.replace(None);
                for da in self.page_draw_areas.values() {
                    da.queue_draw();
                }
                if let Some(popover) = &self.search_popover {
                    popover.popdown();
                }
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
                let old_zoom = self.zoom_level;
                self.zoom_level = ((self.zoom_level + 0.10).min(3.5) * 100.0).round() / 100.0;
                self.catalog.set_pref(&format!("book.{}.pdf.zoom", self.book_id), &format!("{:.2}", self.zoom_level));
                self.apply_zoom_change(old_zoom, &sender, &widgets.viewport_scroll);
            }
            PdfReaderMsg::ZoomOut => {
                let old_zoom = self.zoom_level;
                self.zoom_level = ((self.zoom_level - 0.10).max(0.3) * 100.0).round() / 100.0;
                self.catalog.set_pref(&format!("book.{}.pdf.zoom", self.book_id), &format!("{:.2}", self.zoom_level));
                self.apply_zoom_change(old_zoom, &sender, &widgets.viewport_scroll);
            }
            PdfReaderMsg::ResetZoom => {
                let old_zoom = self.zoom_level;
                self.zoom_level = 1.0;
                self.catalog.set_pref(&format!("book.{}.pdf.zoom", self.book_id), &format!("{:.2}", self.zoom_level));
                self.apply_zoom_change(old_zoom, &sender, &widgets.viewport_scroll);
            }
            PdfReaderMsg::FitToPage => {
                let old_zoom = self.zoom_level;
                let fit = self.compute_fit_zoom(&widgets.viewport_scroll, false);
                self.zoom_level = fit;
                self.catalog.set_pref(&format!("book.{}.pdf.zoom", self.book_id), &format!("{:.2}", self.zoom_level));
                self.apply_zoom_change(old_zoom, &sender, &widgets.viewport_scroll);
            }
            PdfReaderMsg::FitToWidth => {
                let old_zoom = self.zoom_level;
                let fit = self.compute_fit_zoom(&widgets.viewport_scroll, true);
                self.zoom_level = fit;
                self.catalog.set_pref(&format!("book.{}.pdf.zoom", self.book_id), &format!("{:.2}", self.zoom_level));
                self.apply_zoom_change(old_zoom, &sender, &widgets.viewport_scroll);
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

                    // Adjust scrollbar policies
                    self.update_scroll_policies(&widgets.viewport_scroll);

                    let child = self.build_viewport_widget(&sender);
                    widgets.viewport_scroll.set_child(Some(&child));
                    self.prune_textures();
                    self.trigger_loads(&sender);
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

                    self.update_scroll_policies(&widgets.viewport_scroll);
                    let child = self.build_viewport_widget(&sender);
                    widgets.viewport_scroll.set_child(Some(&child));
                    self.prune_textures();
                    self.trigger_loads(&sender);
                    // Deferred scroll, same reason as SetScrollMode above.
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
                        self.update_scroll_policies(&widgets.viewport_scroll);
                        let child = self.build_viewport_widget(&sender);
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
                if let Some(mark) = self.bookmarks.iter().find(|b| b.chapter_index as usize == self.current_page) {
                    let _ = self.catalog.delete_reading_bookmark(mark.id);
                    crate::notify::compact("Bookmark removed", "");
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
                if let Some(ref list) = self.bookmarks_list_box {
                    populate_bookmarks_list(list, &self.bookmarks, &sender);
                }
                self.update_bookmark_icon_state(widgets);
            }
            PdfReaderMsg::DeleteBookmark(id) => {
                let _ = self.catalog.delete_reading_bookmark(id);
                self.reload_bookmarks();
                if let Some(ref list) = self.bookmarks_list_box {
                    populate_bookmarks_list(list, &self.bookmarks, &sender);
                }
                self.update_bookmark_icon_state(widgets);
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
                if self.smart_crop != crop {
                    self.smart_crop = crop;
                    self.catalog.set_pref(
                        &format!("book.{}.pdf.smart_crop", self.book_id),
                        if self.smart_crop { "1" } else { "0" },
                    );
                    self.apply_zoom_change(self.zoom_level, &sender, &widgets.viewport_scroll);
                }
            }
            PdfReaderMsg::ToggleSmartCrop => {
                let _ = sender.input_sender().send(PdfReaderMsg::SetSmartCrop(!self.smart_crop));
            }
            PdfReaderMsg::SetLeftTab(tab) => {
                self.left_tab = tab;
                self.refresh_left_panel(widgets, &sender);
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
                if !was_open && self.left_tab == PdfLeftTab::Contents {
                    self.scroll_toc_to_current();
                }
            }
            PdfReaderMsg::OpenRightPanel => {
                self.right_panel_open = true;
                widgets.right_revealer.set_reveal_child(true);
                self.refresh_right_panel(widgets);
            }
            PdfReaderMsg::OpenContents => {
                if self.left_panel_open && self.left_tab == PdfLeftTab::Contents {
                    self.left_panel_open = false;
                    self.sync_panel_layout(widgets);
                } else {
                    self.left_tab = PdfLeftTab::Contents;
                    let _ = sender.input_sender().send(PdfReaderMsg::OpenLeftPanel);
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
                            pic.set_paintable(Some(&texture));
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
                                    pic.set_paintable(Some(&texture));
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
                                            pic.set_paintable(Some(&left_tex.texture));
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
                                                pic.set_paintable(Some(&right_tex.texture));
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
            PdfReaderMsg::DocumentLoaded { info } => {
                crate::timing::span_end("pdf_open_total");
                match *info {
                    Ok(info) => {
                        self.total_pages = info.total_pages;
                        self.toc_entries = info.toc_entries;
                        self.base_page_width = info.base_page_width;
                        self.base_page_height = info.base_page_height;
                        if self.current_page > self.total_pages {
                            self.current_page = 1;
                        }
                        self.is_loading = false;
                        self.status_text.clear();
                        self.doc_ready = true;

                        self.expand_current_toc_branch();
                        self.refresh_left_panel(widgets, &sender);
                        self.sync_jump_mode_control();

                        // Rebuild the viewport now that the real page count
                        // and dimensions are known (the widget built during
                        // init is a skeleton of placeholders).
                        let child = self.build_viewport_widget(&sender);
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
                        self.apply_screenshot_action(&sender);
                    }
                    Err(err) => {
                        self.is_loading = false;
                        self.status_text = err.clone();
                        log::warn!("PDF open failed: {err}");
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
                let adjusting = if let Some(ref sel) = *self.active_selection.borrow() {
                    if sel.slot == slot && !is_block {
                        let (ex, ey, eh) = sel.end_handle;
                        if (x - ex).abs() < 24.0 && (y - (ey + eh)).abs() < 28.0 {
                            Some(PdfHandleDrag::End)
                        } else {
                            let (sx, sy, _) = sel.start_handle;
                            if (x - sx).abs() < 24.0 && (y - sy).abs() < 28.0 {
                                Some(PdfHandleDrag::Start)
                            } else {
                                None
                            }
                        }
                    } else {
                        None
                    }
                } else {
                    None
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

                let scale_x = target_w / (page_text.width_pts as f64);
                let scale_y = target_h / (page_text.height_pts as f64);

                let start_pt = ((start_x / scale_x) as f32, (start_y / scale_y) as f32);
                let curr_pt = ((curr_x / scale_x) as f32, (curr_y / scale_y) as f32);
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
                    let mut screen_rects = Vec::with_capacity(highlight_rects.len());
                    let mut min_x = f64::MAX;
                    let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN;
                    let mut max_y = f64::MIN;

                    for r in &highlight_rects {
                        let rx = r.0 as f64 * scale_x;
                        let ry = r.1 as f64 * scale_y;
                        let rw = (r.2 - r.0).max(1.0) as f64 * scale_x;
                        let rh = (r.3 - r.1).max(1.0) as f64 * scale_y;
                        screen_rects.push((rx, ry, rw, rh));
                        min_x = min_x.min(rx);
                        min_y = min_y.min(ry);
                        max_x = max_x.max(rx + rw);
                        max_y = max_y.max(ry + rh);
                    }

                    let first = screen_rects.first().copied().unwrap_or((0.0, 0.0, 0.0, 0.0));
                    let last = screen_rects.last().copied().unwrap_or((0.0, 0.0, 0.0, 0.0));

                    let start_handle = (first.0, first.1, first.3);
                    let end_handle = (last.0 + last.2, last.1, last.3);
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
                    }));
                }

                if let Some(da) = self.page_draw_areas.get(&slot) {
                    da.queue_draw();
                }
            }
            PdfReaderMsg::SelectionDragEnd { slot, dx, dy, press_count } => {
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

                let scale_x = target_w / (page_text.width_pts as f64);
                let scale_y = target_h / (page_text.height_pts as f64);

                let pt = ((x / scale_x) as f32, (y / scale_y) as f32);
                if let Some((word_text, highlight_rects)) = page_text.word_at(pt) {
                    let mut screen_rects = Vec::with_capacity(highlight_rects.len());
                    let mut min_x = f64::MAX;
                    let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN;
                    let mut max_y = f64::MIN;

                    for r in &highlight_rects {
                        let rx = r.0 as f64 * scale_x;
                        let ry = r.1 as f64 * scale_y;
                        let rw = (r.2 - r.0).max(1.0) as f64 * scale_x;
                        let rh = (r.3 - r.1).max(1.0) as f64 * scale_y;
                        screen_rects.push((rx, ry, rw, rh));
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

                    let start_handle = (first.0, first.1, first.3);
                    let end_handle = (last.0 + last.2, last.1, last.3);
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
                    }));

                    if let Some(da) = self.page_draw_areas.get(&slot) {
                        da.queue_draw();
                    }
                    self.show_selection_chip(slot, &sender);
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

                let scale_x = target_w / (page_text.width_pts as f64);
                let scale_y = target_h / (page_text.height_pts as f64);

                let pt = ((x / scale_x) as f32, (y / scale_y) as f32);
                if let Some((line_text, highlight_rects)) = page_text.line_at(pt) {
                    let mut screen_rects = Vec::with_capacity(highlight_rects.len());
                    let mut min_x = f64::MAX;
                    let mut min_y = f64::MAX;
                    let mut max_x = f64::MIN;
                    let mut max_y = f64::MIN;

                    for r in &highlight_rects {
                        let rx = r.0 as f64 * scale_x;
                        let ry = r.1 as f64 * scale_y;
                        let rw = (r.2 - r.0).max(1.0) as f64 * scale_x;
                        let rh = (r.3 - r.1).max(1.0) as f64 * scale_y;
                        screen_rects.push((rx, ry, rw, rh));
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

                    let start_handle = (first.0, first.1, first.3);
                    let end_handle = (last.0 + last.2, last.1, last.3);
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
                }
                self.clear_selection();
            }
            PdfReaderMsg::LookUpWord(word) => {
                let trimmed = word.trim().to_string();
                if !trimmed.is_empty() {
                    let entry = self.catalog.lookup_entry(&trimmed).unwrap_or_else(|_| crate::db::EntryData {
                        word: trimmed.clone(),
                        ..Default::default()
                    });
                    let def = entry.senses.first().map(|s| s.def.clone()).unwrap_or_else(|| "No dictionary definition found".to_string());
                    crate::notify::info(&trimmed, &def);
                    let _ = self.catalog.log_dict_lookup(&trimmed, Some(self.book_id), Some(self.current_page as i64), None, !entry.senses.is_empty());
                }
                self.clear_selection();
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

        if let Some(ref sw) = self.settings_widgets {
            self.sync_settings_ui(sw);
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
        if let Some(tabs) = &self.left_tabs {
            tabs.set_selected(match self.left_tab {
                PdfLeftTab::Contents => 0,
                PdfLeftTab::Bookmarks => 1,
            });
        }
        if let Some(stack) = &self.left_stack {
            stack.set_visible_child_name(match self.left_tab {
                PdfLeftTab::Contents => "contents",
                PdfLeftTab::Bookmarks => "bookmarks",
            });
        }
        match self.left_tab {
            PdfLeftTab::Contents => {
                if let Some(list) = &self.toc_list_box {
                    let (rows, current_visible) = populate_toc_list(
                        list,
                        &self.toc_entries,
                        &self.collapsed_toc_entries,
                        &self.toc_filter,
                        self.current_page,
                        self.total_pages,
                        sender,
                    );
                    self.toc_rows = rows;
                    self.toc_current_visible_row = current_visible;
                    self.toc_current_page_synced = self.current_page;
                }
            }
            PdfLeftTab::Bookmarks => {
                if let Some(list) = &self.bookmarks_list_box {
                    populate_bookmarks_list(list, &self.bookmarks, sender);
                }
            }
        }
        self.sync_panel_layout(widgets);
    }

    pub fn refresh_right_panel(&self, widgets: &PdfReaderModelWidgets) {
        if let Some(tabs) = &self.right_tabs {
            tabs.set_selected(match self.right_tab {
                PdfRightTab::Reading => 0,
                PdfRightTab::Shortcuts => 1,
            });
        }
        if let Some(stack) = &self.right_stack {
            stack.set_visible_child_name(match self.right_tab {
                PdfRightTab::Reading => "reading",
                PdfRightTab::Shortcuts => "shortcuts",
            });
        }
        if self.right_tab == PdfRightTab::Reading {
            if let Some(sw) = &self.settings_widgets {
                self.sync_settings_ui(sw);
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
            Ok("bookmarks") => {
                sender.input(PdfReaderMsg::SetLeftTab(PdfLeftTab::Bookmarks));
                sender.input(PdfReaderMsg::OpenLeftPanel);
            }
            Ok("reading") => sender.input(PdfReaderMsg::OpenRightPanel),
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

    pub fn highlight_current_search_match(&mut self, widgets: &PdfReaderModelWidgets) {
        if !self.search_active || self.search_results.is_empty() || self.search_index >= self.search_results.len() {
            self.search_highlight.replace(None);
            for da in self.page_draw_areas.values() {
                da.queue_draw();
            }
            return;
        }

        let res = self.search_results[self.search_index].clone();
        let target_page = res.page;

        // Jump to page if not current
        if self.current_page != target_page {
            self.current_page = target_page.clamp(1, self.total_pages);
            self.save_progress();
            self.prune_textures();

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

        let slot = match self.scroll_mode {
            PdfScrollMode::PageScrolling => match self.spread_mode {
                PdfSpreadMode::NoSpreads => PageSlot::PagedSingle,
                _ => {
                    let (l, r) = self.spread_for_page(self.current_page);
                    if l == target_page {
                        PageSlot::PagedSpreadLeft
                    } else if r == Some(target_page) {
                        PageSlot::PagedSpreadRight
                    } else {
                        PageSlot::PagedSingle
                    }
                }
            },
            _ => PageSlot::Fixed(target_page),
        };

        let (page_w_pts, page_h_pts) = if let Some(text) = self.page_text_cache.get(&target_page) {
            (text.width_pts as f64, text.height_pts as f64)
        } else {
            (self.base_page_width, self.base_page_height)
        };

        let (target_w, target_h) = if let Some(da) = self.page_draw_areas.get(&slot) {
            let w = da.width() as f64;
            let h = da.height() as f64;
            if w > 10.0 && h > 10.0 {
                (w, h)
            } else {
                (self.base_page_width * self.zoom_level, self.base_page_height * self.zoom_level)
            }
        } else {
            (self.base_page_width * self.zoom_level, self.base_page_height * self.zoom_level)
        };

        let scale_x = if page_w_pts > 0.0 { target_w / page_w_pts } else { 1.0 };
        let scale_y = if page_h_pts > 0.0 { target_h / page_h_pts } else { 1.0 };

        let screen_rects: Vec<(f64, f64, f64, f64)> = res.rects.iter().map(|&(x0, y0, x1, y1)| {
            let rx = x0 as f64 * scale_x;
            let ry = y0 as f64 * scale_y;
            let rw = ((x1 - x0) as f64 * scale_x).max(4.0);
            let rh = ((y1 - y0) as f64 * scale_y).max(4.0);
            (rx, ry, rw, rh)
        }).collect();

        self.search_highlight.replace(Some((slot, target_page, screen_rects)));

        for da in self.page_draw_areas.values() {
            da.queue_draw();
        }
    }

    pub fn update_search_snippets_popover(&self, sender: &ComponentSender<Self>) {
        let Some(ref list_box) = self.search_list_box else { return };
        let Some(ref badge_lbl) = self.search_popover_badge else { return };

        while let Some(child) = list_box.first_child() {
            list_box.remove(&child);
        }

        let total = self.search_results.len();
        if total == 0 {
            badge_lbl.set_label("0 matches");
            let empty_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
            empty_box.set_margin_all(20);
            empty_box.set_halign(gtk::Align::Center);

            let empty_lbl = gtk::Label::new(Some(if self.search_query.trim().is_empty() {
                "Type to search across document"
            } else {
                "No matches found"
            }));
            empty_lbl.add_css_class("kalam-search-popover-badge");
            empty_box.append(&empty_lbl);
            list_box.append(&empty_box);
            return;
        }

        badge_lbl.set_label(&format!("{} matches", total));

        let display_limit = 100.min(total);
        for idx in 0..display_limit {
            let res = &self.search_results[idx];
            let row = gtk::Box::new(gtk::Orientation::Vertical, 3);
            row.add_css_class("kalam-search-snippet-row");
            if idx == self.search_index {
                row.add_css_class("active");
            }

            let top_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let header_text = if let Some(ref sec) = res.section_title {
                format!("Page {} · {}", res.page, sec)
            } else {
                format!("Page {}", res.page)
            };
            let sec_label = gtk::Label::new(Some(&header_text));
            sec_label.add_css_class("kalam-search-snippet-chapter");
            sec_label.set_hexpand(true);
            sec_label.set_halign(gtk::Align::Start);
            sec_label.set_ellipsize(gtk::pango::EllipsizeMode::End);

            let idx_label = gtk::Label::new(Some(&format!("#{}", idx + 1)));
            idx_label.add_css_class("kalam-search-snippet-index");

            top_row.append(&sec_label);
            top_row.append(&idx_label);

            let text_label = gtk::Label::new(None);
            text_label.add_css_class("kalam-search-snippet-text");
            text_label.set_use_markup(true);
            text_label.set_markup(&format_pdf_search_snippet(&res.snippet, &self.search_query));
            text_label.set_wrap(true);
            text_label.set_wrap_mode(gtk::pango::WrapMode::Word);
            text_label.set_halign(gtk::Align::Start);
            text_label.set_xalign(0.0);

            row.append(&top_row);
            row.append(&text_label);

            let gesture = gtk::GestureClick::new();
            let s = sender.clone();
            gesture.connect_released(move |_, _, _, _| {
                s.input(PdfReaderMsg::JumpToSearchResult(idx));
            });
            row.add_controller(gesture);

            list_box.append(&row);
        }

        if total > display_limit {
            let more_lbl = gtk::Label::new(Some(&format!("+ {} more matches (use Next/Prev)", total - display_limit)));
            more_lbl.add_css_class("kalam-search-snippet-index");
            more_lbl.set_margin_all(8);
            list_box.append(&more_lbl);
        }
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

fn format_pdf_search_snippet(snippet: &str, query: &str) -> String {
    let query_lower = query.trim().to_lowercase();
    let snippet_lower = snippet.to_lowercase();
    if query_lower.is_empty() {
        return glib::markup_escape_text(snippet).to_string();
    }
    if let Some(idx) = snippet_lower.find(&query_lower) {
        let prefix = &snippet[..idx];
        let matched = &snippet[idx..idx + query.trim().len().min(snippet.len() - idx)];
        let suffix = &snippet[(idx + matched.len()).min(snippet.len())..];
        format!(
            "{}<span weight=\"bold\" foreground=\"#d97706\" background=\"#f4d35e\" background_alpha=\"30%\">{}</span>{}",
            glib::markup_escape_text(prefix),
            glib::markup_escape_text(matched),
            glib::markup_escape_text(suffix)
        )
    } else {
        glib::markup_escape_text(snippet).to_string()
    }
}

fn build_pdf_search_snippets_popover() -> (gtk::Popover, gtk::ListBox, gtk::Label) {
    let popover = gtk::Popover::new();
    popover.add_css_class("kalam-search-snippet-popover");
    popover.set_has_arrow(true);
    popover.set_position(gtk::PositionType::Bottom);

    let content_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
    content_box.set_width_request(340);

    let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header_box.add_css_class("kalam-search-popover-header");

    let title_lbl = gtk::Label::new(Some("Matches at a Glance"));
    title_lbl.add_css_class("kalam-search-popover-title");
    title_lbl.set_hexpand(true);
    title_lbl.set_halign(gtk::Align::Start);

    let badge_lbl = gtk::Label::new(Some("0 matches"));
    badge_lbl.add_css_class("kalam-search-popover-badge");

    header_box.append(&title_lbl);
    header_box.append(&badge_lbl);
    content_box.append(&header_box);

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .max_content_height(340)
        .propagate_natural_height(true)
        .hexpand(true)
        .vexpand(true)
        .build();

    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::None);
    list_box.add_css_class("kalam-search-popover-list");
    scroll.set_child(Some(&list_box));

    content_box.append(&scroll);
    popover.set_child(Some(&content_box));

    (popover, list_box, badge_lbl)
}

fn populate_toc_list(
    list_box: &gtk::Box,
    entries: &[PdfTocEntry],
    collapsed: &HashSet<usize>,
    filter: &str,
    current_page: usize,
    total_pages: usize,
    sender: &ComponentSender<PdfReaderModel>,
) -> (Vec<(usize, gtk::Box)>, Option<usize>) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    if entries.is_empty() {
        let empty_label = gtk::Label::new(Some(
            "This file has no outline. Press G to jump to a page.",
        ));
        empty_label.add_css_class("dim-label");
        empty_label.set_wrap(true);
        empty_label.set_xalign(0.0);
        empty_label.set_margin_top(24);
        empty_label.set_margin_start(20);
        empty_label.set_margin_end(20);
        list_box.append(&empty_label);
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

        let page_lbl = gtk::Label::new(Some(&page_target.to_string()));
        page_lbl.add_css_class("kalam-pdf-contents-page");
        page_lbl.set_halign(gtk::Align::End);
        page_lbl.set_valign(gtk::Align::Center);
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

fn populate_bookmarks_list(
    list_box: &gtk::Box,
    bookmarks: &[ReadingBookmark],
    sender: &ComponentSender<PdfReaderModel>,
) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    if bookmarks.is_empty() {
        let empty_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        empty_box.set_margin_top(32);
        empty_box.set_margin_start(16);
        empty_box.set_margin_end(16);

        let icon = crate::icons::symbolic_with_classes("bookmark-new-symbolic", 32, &["dim-label"]);
        icon.set_halign(gtk::Align::Center);
        empty_box.append(&icon);

        let label = gtk::Label::new(Some("No bookmarks yet."));
        label.add_css_class("kalam-reader-section-label");
        label.set_halign(gtk::Align::Center);
        empty_box.append(&label);

        let hint = gtk::Label::new(Some("Press B or tap the ribbon at the top of the page to bookmark any page."));
        hint.add_css_class("dim-label");
        hint.set_wrap(true);
        hint.set_justify(gtk::Justification::Center);
        empty_box.append(&hint);

        list_box.append(&empty_box);
        return;
    }

    for mark in bookmarks {
        let page_num = mark.chapter_index as usize;
        let mark_id = mark.id;

        let row_btn = gtk::Button::new();
        row_btn.add_css_class("flat");
        row_btn.add_css_class("kalam-reader-bookmark-row");

        let h_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);

        let icon = crate::icons::symbolic_with_classes("bookmark-new-symbolic", 16, &["kalam-reader-bookmark-icon"]);
        h_box.append(&icon);

        let v_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        v_box.set_hexpand(true);

        let title_lbl = gtk::Label::new(Some(&mark.label));
        title_lbl.add_css_class("kalam-reader-bookmark-title");
        title_lbl.set_halign(gtk::Align::Start);
        title_lbl.set_wrap(true);
        title_lbl.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        title_lbl.set_width_chars(1);
        title_lbl.set_lines(-1);
        v_box.append(&title_lbl);

        let progress_lbl = gtk::Label::new(Some(&format!("{:.0}% through book", mark.fraction * 100.0)));
        progress_lbl.add_css_class("dim-label");
        progress_lbl.set_halign(gtk::Align::Start);
        v_box.append(&progress_lbl);

        h_box.append(&v_box);

        let del_btn = gtk::Button::new();
        del_btn.set_child(Some(&crate::icons::symbolic_with_classes("edit-delete-symbolic", 14, &["dim-label"])));
        del_btn.add_css_class("flat");
        del_btn.set_tooltip_text(Some("Delete Bookmark"));
        let tx_del = sender.input_sender().clone();
        del_btn.connect_clicked(move |_| {
            let _ = tx_del.send(PdfReaderMsg::DeleteBookmark(mark_id));
        });
        h_box.append(&del_btn);

        row_btn.set_child(Some(&h_box));
        let tx_jump = sender.input_sender().clone();
        row_btn.connect_clicked(move |_| {
            let _ = tx_jump.send(PdfReaderMsg::JumpToPage(page_num));
        });

        list_box.append(&row_btn);
    }
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
        ],
    );
    pdf_shortcut_group(
        host,
        "Panels",
        &[
            ("Contents", &["T"][..]),
            ("Reading settings", &["S"][..]),
            ("Shortcuts", &["H"][..]),
            ("Close layer or exit", &["Esc"][..]),
        ],
    );
}

fn setting_icon_btn(icon: &str, label: &str) -> gtk::Button {
    let btn = gtk::Button::new();
    btn.add_css_class("kalam-reader-seg-btn");
    btn.set_halign(gtk::Align::Fill);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.append(&crate::icons::symbolic_with_classes(
        icon,
        16,
        &["kalam-inline-icon"],
    ));
    let lbl = gtk::Label::new(Some(label));
    lbl.set_halign(gtk::Align::Start);
    lbl.set_hexpand(true);
    row.append(&lbl);
    btn.set_child(Some(&row));
    btn
}

fn build_pdf_settings_panel(
    model: &PdfReaderModel,
    sender: &ComponentSender<PdfReaderModel>,
) -> (gtk::Box, PdfSettingsWidgets) {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 16);
    wrap.set_margin_start(16);
    wrap.set_margin_end(16);
    wrap.set_margin_top(16);
    wrap.set_margin_bottom(24);

    // ── 1. Scrolling Section (Matching Photo) ────────────────
    let scroll_section = reader_settings_section("Scrolling");

    let scroll_box = gtk::Box::new(gtk::Orientation::Vertical, 6);

    let scroll_page_btn = setting_icon_btn("kalam-pdf-page-scroll-symbolic", "Page Scrolling");
    let tx = sender.input_sender().clone();
    scroll_page_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetScrollMode(PdfScrollMode::PageScrolling));
    });
    scroll_box.append(&scroll_page_btn);

    let scroll_vertical_btn = setting_icon_btn("kalam-pdf-vertical-scroll-symbolic", "Vertical Scrolling");
    let tx = sender.input_sender().clone();
    scroll_vertical_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetScrollMode(PdfScrollMode::VerticalScrolling));
    });
    scroll_box.append(&scroll_vertical_btn);

    let scroll_horizontal_btn = setting_icon_btn("kalam-pdf-horizontal-scroll-symbolic", "Horizontal Scrolling");
    let tx = sender.input_sender().clone();
    scroll_horizontal_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetScrollMode(PdfScrollMode::HorizontalScrolling));
    });
    scroll_box.append(&scroll_horizontal_btn);

    let scroll_wrapped_btn = setting_icon_btn("kalam-pdf-wrapped-scroll-symbolic", "Wrapped Scrolling");
    let tx = sender.input_sender().clone();
    scroll_wrapped_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetScrollMode(PdfScrollMode::WrappedScrolling));
    });
    scroll_box.append(&scroll_wrapped_btn);

    scroll_section.append(&scroll_box);
    wrap.append(&scroll_section);

    let divider = gtk::Separator::new(gtk::Orientation::Horizontal);
    divider.add_css_class("kalam-section-divider");
    wrap.append(&divider);

    // ── 2. Spreads Section (Matching Photo) ──────────────────
    let spread_section = reader_settings_section("Spreads");

    let spread_box = gtk::Box::new(gtk::Orientation::Vertical, 6);

    let spread_none_btn = setting_icon_btn("kalam-pdf-no-spreads-symbolic", "No Spreads");
    let tx = sender.input_sender().clone();
    spread_none_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetSpreadMode(PdfSpreadMode::NoSpreads));
    });
    spread_box.append(&spread_none_btn);

    let spread_odd_btn = setting_icon_btn("kalam-pdf-odd-spreads-symbolic", "Odd Spreads");
    let tx = sender.input_sender().clone();
    spread_odd_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetSpreadMode(PdfSpreadMode::OddSpreads));
    });
    spread_box.append(&spread_odd_btn);

    let spread_even_btn = setting_icon_btn("kalam-pdf-even-spreads-symbolic", "Even Spreads");
    let tx = sender.input_sender().clone();
    spread_even_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetSpreadMode(PdfSpreadMode::EvenSpreads));
    });
    spread_box.append(&spread_even_btn);

    spread_section.append(&spread_box);

    // Spread gap options box (permanently visible under Spreads)
    let spread_gap_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    spread_gap_box.add_css_class("kalam-reader-spread-gap-box");
    spread_gap_box.set_margin_top(8);

    let gap_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    gap_row.add_css_class("kalam-reader-setting-row");
    let gap_label = gtk::Label::new(Some("Spread Gap"));
    gap_label.add_css_class("kalam-reader-setting-name");
    gap_label.set_hexpand(true);
    gap_label.set_halign(gtk::Align::Start);
    gap_row.append(&gap_label);

    let gap_switcher = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    gap_switcher.add_css_class("linked");

    let gap_0_btn = gtk::Button::with_label("0px");
    gap_0_btn.add_css_class("kalam-reader-seg-btn");
    let tx = sender.input_sender().clone();
    gap_0_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetSpreadGap(0));
    });
    gap_switcher.append(&gap_0_btn);

    let gap_4_btn = gtk::Button::with_label("4px");
    gap_4_btn.add_css_class("kalam-reader-seg-btn");
    let tx = sender.input_sender().clone();
    gap_4_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetSpreadGap(4));
    });
    gap_switcher.append(&gap_4_btn);

    let gap_8_btn = gtk::Button::with_label("8px");
    gap_8_btn.add_css_class("kalam-reader-seg-btn");
    let tx = sender.input_sender().clone();
    gap_8_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetSpreadGap(8));
    });
    gap_switcher.append(&gap_8_btn);

    let gap_12_btn = gtk::Button::with_label("12px");
    gap_12_btn.add_css_class("kalam-reader-seg-btn");
    let tx = sender.input_sender().clone();
    gap_12_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetSpreadGap(12));
    });
    gap_switcher.append(&gap_12_btn);

    let gap_16_btn = gtk::Button::with_label("16px");
    gap_16_btn.add_css_class("kalam-reader-seg-btn");
    let tx = sender.input_sender().clone();
    gap_16_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetSpreadGap(16));
    });
    gap_switcher.append(&gap_16_btn);
    gap_row.append(&gap_switcher);
    spread_gap_box.append(&gap_row);

    let gap_hint = gtk::Label::new(Some("0px gives seamless cross-page spreads and illustrations."));
    gap_hint.add_css_class("kalam-reader-setting-hint");
    gap_hint.set_wrap(true);
    gap_hint.set_xalign(0.0);
    spread_gap_box.append(&gap_hint);

    spread_section.append(&spread_gap_box);
    wrap.append(&spread_section);

    let divider2 = gtk::Separator::new(gtk::Orientation::Horizontal);
    divider2.add_css_class("kalam-section-divider");
    wrap.append(&divider2);

    // ── 3. Zoom & View Sizing Section ─────────────────────────
    let zoom_section = reader_settings_section("Zoom & View Sizing");

    let fit_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    fit_row.add_css_class("kalam-reader-setting-row");
    let fit_label = gtk::Label::new(Some("Page Sizing"));
    fit_label.add_css_class("kalam-reader-setting-name");
    fit_label.set_hexpand(true);
    fit_label.set_halign(gtk::Align::Start);
    fit_row.append(&fit_label);

    let fit_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    fit_box.add_css_class("linked");

    let fit_page_btn = gtk::Button::with_label("Fit Page");
    fit_page_btn.add_css_class("kalam-reader-seg-btn");
    let tx = sender.input_sender().clone();
    fit_page_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::FitToPage);
    });
    fit_box.append(&fit_page_btn);

    let fit_width_btn = gtk::Button::with_label("Fit Width");
    fit_width_btn.add_css_class("kalam-reader-seg-btn");
    let tx = sender.input_sender().clone();
    fit_width_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::FitToWidth);
    });
    fit_box.append(&fit_width_btn);

    fit_row.append(&fit_box);
    zoom_section.append(&fit_row);

    let zoom_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    zoom_row.add_css_class("kalam-reader-setting-row");
    let zoom_name = gtk::Label::new(Some("Magnification"));
    zoom_name.add_css_class("kalam-reader-setting-name");
    zoom_name.set_hexpand(true);
    zoom_name.set_halign(gtk::Align::Start);
    zoom_row.append(&zoom_name);

    let zoom_stepper = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    zoom_stepper.add_css_class("kalam-reader-stepper");

    let minus_btn = gtk::Button::new();
    minus_btn.add_css_class("kalam-reader-stepper-btn");
    minus_btn.set_size_request(30, 30);
    minus_btn.set_child(Some(&crate::icons::symbolic_with_classes(
        "list-remove-symbolic",
        14,
        &["kalam-inline-icon"],
    )));
    let tx = sender.input_sender().clone();
    minus_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::ZoomOut);
    });
    zoom_stepper.append(&minus_btn);

    let zoom_label = gtk::Label::new(Some(&format!(
        "{}%",
        (model.zoom_level * 100.0).round() as i32
    )));
    zoom_label.add_css_class("kalam-reader-stepper-value");
    zoom_stepper.append(&zoom_label);

    let plus_btn = gtk::Button::new();
    plus_btn.add_css_class("kalam-reader-stepper-btn");
    plus_btn.set_size_request(30, 30);
    plus_btn.set_child(Some(&crate::icons::symbolic_with_classes(
        "list-add-symbolic",
        14,
        &["kalam-inline-icon"],
    )));
    let tx = sender.input_sender().clone();
    plus_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::ZoomIn);
    });
    zoom_stepper.append(&plus_btn);

    let reset_btn = gtk::Button::with_label("100%");
    reset_btn.add_css_class("flat");
    let tx = sender.input_sender().clone();
    reset_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::ResetZoom);
    });
    zoom_stepper.append(&reset_btn);

    zoom_row.append(&zoom_stepper);
    zoom_section.append(&zoom_row);

    wrap.append(&zoom_section);

    let divider3 = gtk::Separator::new(gtk::Orientation::Horizontal);
    divider3.add_css_class("kalam-section-divider");
    wrap.append(&divider3);

    // ── 4. Margins & Enhancements Section ─────────────────────
    let crop_section = reader_settings_section("Margins & Enhancements");

    let crop_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    crop_row.add_css_class("kalam-reader-setting-row");
    let crop_label = gtk::Label::new(Some("Smart Crop (Trim Margins)"));
    crop_label.add_css_class("kalam-reader-setting-name");
    crop_label.set_hexpand(true);
    crop_label.set_halign(gtk::Align::Start);
    crop_row.append(&crop_label);

    let tx = sender.input_sender().clone();
    let smart_crop_switch = crate::pages::settings::toggle_switch(model.smart_crop, move |on| {
        let _ = tx.send(PdfReaderMsg::SetSmartCrop(on));
    });
    crop_row.append(&smart_crop_switch);
    crop_section.append(&crop_row);

    let crop_hint = gtk::Label::new(Some(
        "Auto-detects margins around ink content to maximize reading area while preserving ink safety buffers.",
    ));
    crop_hint.add_css_class("kalam-reader-setting-hint");
    crop_hint.set_wrap(true);
    crop_hint.set_xalign(0.0);
    crop_section.append(&crop_hint);

    wrap.append(&crop_section);

    let divider5 = gtk::Separator::new(gtk::Orientation::Horizontal);
    divider5.add_css_class("kalam-section-divider");
    wrap.append(&divider5);

    // ── 6. Document Properties Section ────────────────────────
    let info_section = reader_settings_section("Document Properties");

    let title_prop = prop_row("Title", &model.title);
    info_section.append(&title_prop);

    let author_prop = prop_row("Author", &model.author);
    info_section.append(&author_prop);

    let pages_prop = prop_row("Total Pages", &format!("{} pages", model.total_pages));
    info_section.append(&pages_prop);

    let engine_prop = prop_row("Renderer", "MuPDF v0.8.0 (RGBA)");
    info_section.append(&engine_prop);

    wrap.append(&info_section);

    let widgets = PdfSettingsWidgets {
        scroll_page_btn,
        scroll_vertical_btn,
        scroll_horizontal_btn,
        scroll_wrapped_btn,
        spread_none_btn,
        spread_odd_btn,
        spread_even_btn,
        gap_0_btn,
        gap_4_btn,
        gap_8_btn,
        gap_12_btn,
        gap_16_btn,
        spread_gap_box,
        smart_crop_switch,
        zoom_label,
    };

    (wrap, widgets)
}

fn prop_row(label: &str, value: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("kalam-reader-setting-row");
    let name_lbl = gtk::Label::new(Some(label));
    name_lbl.add_css_class("dim-label");
    name_lbl.set_halign(gtk::Align::Start);
    name_lbl.set_hexpand(true);
    row.append(&name_lbl);

    let val_lbl = gtk::Label::new(Some(value));
    val_lbl.set_halign(gtk::Align::End);
    val_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
    val_lbl.set_max_width_chars(22);
    row.append(&val_lbl);
    row
}

fn reader_settings_section(label: &str) -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 8);
    section.add_css_class("kalam-reader-section");
    let heading = gtk::Label::new(Some(label));
    heading.add_css_class("kalam-reader-section-label");
    heading.set_halign(gtk::Align::Start);
    section.append(&heading);
    section
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

    #[test]
    fn test_location_label_names_the_whole_spread() {
        assert_eq!(
            format_location_label(
                PdfSpreadMode::NoSpreads,
                PdfScrollMode::VerticalScrolling,
                12,
                4
            ),
            "4 / 12"
        );
        // Cover on its own: page 4 opens the 4–5 spread.
        assert_eq!(
            format_location_label(
                PdfSpreadMode::OddSpreads,
                PdfScrollMode::PageScrolling,
                12,
                4
            ),
            "4–5 / 12"
        );
        assert_eq!(
            format_location_label(
                PdfSpreadMode::OddSpreads,
                PdfScrollMode::PageScrolling,
                12,
                1
            ),
            "1 / 12"
        );
        // Wrapped lays pages out singly even with a pairing chosen.
        assert_eq!(
            format_location_label(
                PdfSpreadMode::OddSpreads,
                PdfScrollMode::WrappedScrolling,
                12,
                4
            ),
            "4 / 12"
        );
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
}
