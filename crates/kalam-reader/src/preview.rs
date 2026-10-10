//! Headless preview rendering — one chapter's pages rasterized with no
//! widget at all (Phase 6.9).
//!
//! The raw source pane edits an entry's bytes and wants to show what
//! they will *read* like, but the engine's one [`Session`] cache has no
//! invalidation API, and the pane's bytes are not on disk anyway — they
//! are typed into a buffer. So a preview re-opens the book from its
//! path with an [`EntryFilter`] that substitutes the edited bytes at
//! read time, lays the chapter out at a fixed page size, rasterizes the
//! first few pages, and stacks them into one tall pixmap. That pixmap
//! is plain data (`Send`, no GTK types), so the whole render happens on
//! a service thread and the main loop only turns the bytes into a
//! texture.
//!
//! A preview is not the reader: it is a glance, paid for on every
//! debounce tick. It borrows the reader's font source, settings and
//! theme so the glance is honest — what it shows is what a save will
//! make the book read like — and nothing else.

use std::path::{Path, PathBuf};

use chapbook_core::{ChapbookError, EdgeSizes, PageMetrics, Result, Size};
use chapbook_reader::{Session, SessionConfig, SettingsScope, tiny_skia};

use crate::{DEFAULT_CACHE_BUDGET, EntryFilter, KalamPrefs, font_source};

/// The page geometry a preview lays out at. Not a [`PageMetrics`] copy:
/// a preview fixes what the widget measures — one column, a calm
/// margin, scale 1 — so previews are comparable tick to tick.
pub const PREVIEW_MARGIN: f32 = 48.0;

/// How many pages a preview stacks, one screenful of reading under the
/// edit being made. Enough to see a change flow across a page break;
/// not so many that a tick costs a chapter's whole layout twice.
pub const PREVIEW_PAGES: usize = 3;

/// What to render: the bytes (through `filter`), the place, and the
/// page size. A struct rather than an argument list because the pieces
/// travel together — there is no call that wants some of them.
pub struct Preview {
    /// The virtual-edit seam, substituted at open. The caller builds
    /// it — the raw pane's edited bytes for its chapter, the pending
    /// patches for everything else.
    pub filter: EntryFilter,
    /// The chapter (spine index) to render.
    pub spine: usize,
    /// The page within the chapter to start at.
    pub start_page: usize,
    /// How many pages to stack; [`PREVIEW_PAGES`] is the usual.
    pub pages: usize,
    /// Page width in layout units; the reader's `column_px` plus room
    /// for the margins is the usual choice.
    pub page_width: u32,
    /// Page height in layout units.
    pub page_height: u32,
}

/// Render one chapter's first pages into a stacked pixmap, off the UI
/// thread by contract (the caller's worker owns this call). The
/// pixmap's background is the theme's paper, so a chapter that renders
/// short still looks like paper.
///
/// The order inside is the order that keeps anything from being laid
/// out twice: filter first (before a single entry is read), then
/// settings, then metrics, then the jump to the chapter.
pub fn render(
    path: &Path,
    prefs: &KalamPrefs,
    fonts_dir: Option<PathBuf>,
    preview: Preview,
) -> Result<tiny_skia::Pixmap> {
    let config = SessionConfig::new(font_source(false, fonts_dir))
        .with_cache_budget(DEFAULT_CACHE_BUDGET);
    let mut session = Session::open_with(path, config)?;
    if preview.filter.is_set() {
        session.set_entry_filter(preview.filter);
    }
    session.set_settings(prefs.reading_settings(), SettingsScope::ThisBook);
    let metrics = PageMetrics::new(
        Size::new(preview.page_width as f32, preview.page_height as f32),
        EdgeSizes::uniform(PREVIEW_MARGIN),
        1.0,
    );
    session.set_metrics(metrics);

    // How many pages this chapter actually has from the start page —
    // a short chapter stacks fewer pages, never blank filler.
    let total = session.page_extents(preview.spine).len();
    let pages = preview.pages.min(total.saturating_sub(preview.start_page).max(1)).max(1);

    let width = preview.page_width;
    let height = preview.page_height;
    let mut combined = tiny_skia::Pixmap::new(width, height * pages as u32)
        .ok_or_else(|| ChapbookError::Layout("empty preview size".into()))?;
    let bg = prefs.theme.background();
    combined.fill(tiny_skia::Color::from_rgba8(bg.r, bg.g, bg.b, bg.a));

    for i in 0..pages {
        session.set_position(preview.spine, preview.start_page + i);
        let y = (i as u32 * height) as i32;
        if let Some(page) = session.render() {
            combined.draw_pixmap(
                0,
                y,
                page.as_ref(),
                &tiny_skia::PixmapPaint::default(),
                tiny_skia::Transform::identity(),
                None,
            );
        }
    }
    Ok(combined)
}
