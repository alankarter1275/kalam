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

/// The right floating panel's tabs (decision 39). The Image pane joins
/// this enum with its draw-time adjustment engine; until then the panel
/// carries the two panes that already have behaviour behind them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComicRightTab {
    #[default]
    Reading,
    Shortcuts,
}

/// What the page-jump field counts in (decision 50): a series-wide page
/// number, or a chapter by its position in the series.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JumpMode {
    #[default]
    Page,
    Chapter,
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
    /// A decoded page, tagged with the load generation that asked for
    /// it so a chapter switch can disown late arrivals from the chapter
    /// that has already been left.
    PageLoaded(u64, usize, Option<gtk::gdk::Texture>),
    ThumbnailLoaded(u64, usize, Option<gtk::gdk::Texture>),
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
