//! Types and message definitions for the comics reader component.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
            PageStyle::Double => "Double",
            PageStyle::Fade => "Fade",
            PageStyle::LongStrip => "Long Strip",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComicSidebarTab {
    Pages,
    Bookmarks,
    Settings,
}

#[derive(Debug)]
pub enum ComicsReaderMsg {
    PageLoaded(usize, Option<gtk::gdk::Texture>),
    SetPage(usize),
    UpdateScrollPage(usize),
    NextPage,
    PrevPage,
    KeyLeft,
    KeyRight,
    TapAtRatio(f64),
    #[allow(dead_code)]
    ToggleDirection,
    SetDirection(ReadingDirection),
    SetPageStyle(PageStyle),
    ToggleFitMode,
    SetFitMode(FitMode),
    SetSpreadGap(i32),
    ToggleSidebar,
    CloseSidebar,
    SetSidebarTab(ComicSidebarTab),
    ToggleBookmark,
    DeleteBookmark(i64),
    TopEdgeHover(bool),
    BottomEdgeHover(bool),
    LeftEdgeHover(bool),
    SidebarHover(bool),
    BackHideTimerTick(u64),
    BottomHideTimerTick(u64),
    SidebarCloseTimerTick(u64),
    UserScrolled,
    HideOsd(u64),
    #[allow(dead_code)]
    ToggleChrome,
    #[allow(dead_code)]
    ToggleSettings,
    #[allow(dead_code)]
    CloseSettings,
    Close,
}

#[derive(Debug)]
pub enum ComicsReaderOut {
    Close,
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
