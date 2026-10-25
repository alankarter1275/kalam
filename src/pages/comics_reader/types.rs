//! Types and message definitions for the comics reader component.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ReadingDirection {
    #[default]
    Ltr,
    Rtl,
    Webtoon,
}

impl ReadingDirection {
    pub fn label(self) -> &'static str {
        match self {
            ReadingDirection::Ltr => "Left → Right",
            ReadingDirection::Rtl => "Right → Left (Manga)",
            ReadingDirection::Webtoon => "Webtoon (Vertical)",
        }
    }

    #[allow(dead_code)]
    pub fn short_label(self) -> &'static str {
        match self {
            ReadingDirection::Ltr => "LTR",
            ReadingDirection::Rtl => "RTL",
            ReadingDirection::Webtoon => "Webtoon",
        }
    }

    pub fn next(self) -> Self {
        match self {
            ReadingDirection::Ltr => ReadingDirection::Rtl,
            ReadingDirection::Rtl => ReadingDirection::Webtoon,
            ReadingDirection::Webtoon => ReadingDirection::Ltr,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum PageStyle {
    Single,
    Double,
    Fade,
    #[default]
    LongStrip,
}

impl PageStyle {
    #[allow(dead_code)] // fit-mode labels render in the reader's settings popover
    pub fn label(self) -> &'static str {
        match self {
            PageStyle::Single => "Single",
            PageStyle::Double => "Spread",
            PageStyle::Fade => "Fade",
            PageStyle::LongStrip => "Long strip",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum FitMode {
    #[default]
    Width,
    Height,
    Screen,
    Original,
}

impl FitMode {
    pub fn label(self) -> &'static str {
        match self {
            FitMode::Width => "Fit Width",
            FitMode::Height => "Fit Height",
            FitMode::Screen => "Fit Screen",
            FitMode::Original => "Original",
        }
    }

    #[allow(dead_code)]
    pub fn icon(self) -> &'static str {
        match self {
            FitMode::Width => "zoom-fit-best-symbolic",
            FitMode::Height => "view-fullscreen-symbolic",
            FitMode::Screen => "zoom-fit-best-symbolic",
            FitMode::Original => "zoom-original-symbolic",
        }
    }

    pub fn next(self) -> Self {
        match self {
            FitMode::Width => FitMode::Height,
            FitMode::Height => FitMode::Screen,
            FitMode::Screen => FitMode::Original,
            FitMode::Original => FitMode::Width,
        }
    }
}

/// The left floating panel's two tabs (decision 38).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComicLeftTab {
    #[default]
    Chapters,
    Pages,
}

/// The right floating panel's tabs (decision 39).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComicRightTab {
    #[default]
    Reading,
    Image,
    Shortcuts,
}

/// The colour behind a comic page (decision 41). `Theme` keeps the
/// reader's normal stage colour; the other three are the fixed dots in
/// the Image pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ComicBackground {
    #[default]
    Theme,
    Black,
    Grey,
    White,
}

impl ComicBackground {
    pub const ALL: [Self; 4] = [Self::Theme, Self::Black, Self::Grey, Self::White];
}

/// Everything the Image pane controls (decision 41). Adjustments are
/// centred on zero: 0 leaves that part of the page alone, while the
/// ends of the range are deliberately strong. Crop starts on, as the
/// approved boards show it; all other processing starts neutral.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ComicImageSettings {
    pub background: ComicBackground,
    pub brightness: i32,
    pub contrast: i32,
    pub warmth: i32,
    pub grayscale: bool,
    pub invert_colours: bool,
    pub crop_blank_borders: bool,
}

impl Default for ComicImageSettings {
    fn default() -> Self {
        Self {
            background: ComicBackground::Theme,
            brightness: 0,
            contrast: 0,
            warmth: 0,
            grayscale: false,
            invert_colours: false,
            crop_blank_borders: true,
        }
    }
}

impl ComicImageSettings {
    pub const MIN_ADJUSTMENT: i32 = -100;
    pub const MAX_ADJUSTMENT: i32 = 100;

    pub fn canonical(mut self) -> Self {
        self.brightness = self
            .brightness
            .clamp(Self::MIN_ADJUSTMENT, Self::MAX_ADJUSTMENT);
        self.contrast = self
            .contrast
            .clamp(Self::MIN_ADJUSTMENT, Self::MAX_ADJUSTMENT);
        self.warmth = self
            .warmth
            .clamp(Self::MIN_ADJUSTMENT, Self::MAX_ADJUSTMENT);
        self
    }
}

/// A page rectangle in unit coordinates: `(0, 0)` is the top-left of
/// the decoded image and `(1, 1)` its bottom-right. Crop analysis
/// stores fractions rather than pixels so one result serves every
/// display size and zoom.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CropRect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl CropRect {
    pub const FULL: Self = Self {
        x: 0.0,
        y: 0.0,
        width: 1.0,
        height: 1.0,
    };

    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Self {
        if ![x, y, width, height].iter().all(|v| v.is_finite()) {
            return Self::FULL;
        }
        let x = x.clamp(0.0, 1.0);
        let y = y.clamp(0.0, 1.0);
        let width = width.clamp(0.0, 1.0 - x);
        let height = height.clamp(0.0, 1.0 - y);
        if width <= 0.0 || height <= 0.0 {
            Self::FULL
        } else {
            Self {
                x,
                y,
                width,
                height,
            }
        }
    }

    pub fn is_full(self) -> bool {
        self.x <= 0.0005
            && self.y <= 0.0005
            && self.width >= 0.9995
            && self.height >= 0.9995
    }

    pub fn pixel_size(self, image_width: i32, image_height: i32) -> (i32, i32) {
        (
            ((image_width.max(1) as f32 * self.width).round() as i32).max(1),
            ((image_height.max(1) as f32 * self.height).round() as i32).max(1),
        )
    }
}

impl Default for CropRect {
    fn default() -> Self {
        Self::FULL
    }
}

/// What the page-jump field counts in (decision 50): a series-wide page
/// number, or a chapter by its position in the series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JumpMode {
    #[default]
    Page,
    Chapter,
}

/// One page anywhere in the series being read. The chapter is an index
/// into the reader's ordered chapter list; it is `None` only for a
/// standalone chapter that the catalog could not place in a series.
/// Page numbers are always local to that chapter, matching the Pages
/// grid's numbers (decision 38).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SeriesPageRef {
    pub chapter_idx: Option<usize>,
    pub page_idx: usize,
}

impl SeriesPageRef {
    pub const fn new(chapter_idx: Option<usize>, page_idx: usize) -> Self {
        Self {
            chapter_idx,
            page_idx,
        }
    }
}

/// The chapter hierarchy's own facts about one chapter book: its printed
/// number and volume, and the chapter's title when the file or the
/// catalog knows one. Kept beside the `Book` rows so the Chapters tab
/// can group by volume without losing them to `Book`'s flat shape.
#[derive(Debug, Clone, Default)]
pub struct ChapterMeta {
    pub number: Option<f32>,
    pub volume: Option<f32>,
    pub title: String,
}

#[derive(Debug)]
pub enum ComicsReaderMsg {
    /// A decoded page and its cached blank-border analysis, tagged
    /// with the load generation that asked for it so a chapter switch
    /// can disown late arrivals from the chapter that has already been
    /// left. The crop is `None` only when no texture arrived or the
    /// page took GDK's encoded-bytes fallback and supplied no RGBA
    /// pixels to analyse.
    PageLoaded(u64, usize, Option<gtk::gdk::Texture>, Option<CropRect>),
    /// A Pages-grid thumbnail decode. Unlike a reading page, it stays
    /// valid across a chapter switch because its identity includes the
    /// chapter it belongs to; the generation only disowns work left
    /// over after the reader's thumbnail caches are emptied.
    ThumbnailLoaded(u64, SeriesPageRef, Option<gtk::gdk::Texture>),
    /// A recycled Pages-grid cell has been bound to this page. This is
    /// the lazy window's edge: only now is its thumbnail worth decoding.
    PageThumbnailVisible(SeriesPageRef),
    /// A Pages-grid thumbnail was chosen, in this chapter or another.
    OpenSeriesPage(SeriesPageRef),
    /// The Pages grid scrolled; its sticky heading follows the first
    /// chapter on show rather than the chapter being read.
    PagesGridScrolled(f64),
    SetPage(usize),
    UpdateScrollPage(f64),
    ScrollDelta(f64),
    NextPage,
    PrevPage,
    KeyLeft,
    KeyRight,
    #[allow(dead_code)]
    ToggleDirection,
    SetDirection(ReadingDirection),
    SetPageStyle(PageStyle),
    ToggleSpread,
    ToggleFitMode,
    SetFitMode(FitMode),
    SetSpreadGap(i32),
    SetStripGap(bool),
    /// Zoom as a percentage of the selected fit's size (100 = fit).
    SetZoomPercent(u32),
    ZoomBy(i32),
    /// Double-click: fit ↔ original size (decision 42).
    ToggleZoom100,
    SetAutoNextChapter(bool),
    SetMarkReadOnAdvance(bool),
    /// The Image pane (decision 41). These edits belong to the same
    /// all-or-nothing panel record as the Reading pane.
    SetImageBackground(ComicBackground),
    SetBrightness(i32),
    SetContrast(i32),
    SetWarmth(i32),
    SetGrayscale(bool),
    SetInvertColours(bool),
    SetCropBlankBorders(bool),
    ResetImageSettings,
    /// "Settings for this series" (decisions 40/147): on stores the
    /// standing choices as this series' own; off forgets the record
    /// and the global comic defaults come back.
    SetSeriesOverride(bool),
    /// Copy the standing choices over the global comic defaults.
    UseSettingsAsDefaults,
    ToggleLeftPanel,
    ToggleRightPanel,
    OpenLeftPanel,
    OpenRightPanel,
    ClosePanels,
    SetLeftTab(ComicLeftTab),
    SetRightTab(ComicRightTab),
    ToggleBookmark,
    SwitchChapterAt(i64, Option<usize>),
    NextChapter,
    #[allow(dead_code)]
    PrevChapter,
    ShowBackChrome,
    BackZoneHover(bool),
    BackRevealDue(u64),
    BackHideDue(u64),
    OpenJump(Option<String>),
    CloseJump,
    SetJumpMode(JumpMode),
    JumpTextChanged(String),
    SubmitJump,
    UserScrolled,
    HideOsd(u64),
    ViewportResized,
    Escape,
    Close,
    MinimizeToBubble,
}

#[derive(Debug)]
pub enum ComicsReaderOut {
    Close,
    MinimizeToBubble { book_id: i64 },
}

#[allow(dead_code)] // menu/context actions for the comics reader UI
pub enum ReaderContext {
    Local(i64), // book_id
    Remote { source_id: String, chapter_id: String },
}

pub struct ComicsReaderInit {
    pub title: String,
    pub provider: std::sync::Arc<dyn super::provider::ImageProvider>,
    pub catalog: Option<std::sync::Arc<crate::db::Catalog>>,
    pub book_id: Option<i64>,
    pub cover_path: Option<std::path::PathBuf>,
}
