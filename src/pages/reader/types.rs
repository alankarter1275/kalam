//! Reader component messages, payload types, preference structures, and sidebar enums.

use crate::db::HighlightColor;
use relm4::prelude::*;

#[derive(Debug)]
pub enum ReaderOut {
    Close,
    MinimizeToBubble { book_id: i64 },
    OpenAuthor { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftSidebarTab {
    Toc,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightSidebarTab {
    Highlights,
    Bookmarks,
    Words,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightFilter {
    All,
    Yellow,
    Green,
    Blue,
    Pink,
    Orange,
    Underline,
    Quotes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WordScope {
    Chapter,
    Book,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderSettingsPane {
    Reading,
    Shortcuts,
}

pub(crate) struct ReaderSettingsControls {
    pub(crate) root: gtk::Box,
    pub(crate) settings_stack: gtk::Stack,
    pub(crate) pane_buttons: Vec<(ReaderSettingsPane, gtk::Button)>,
    pub(crate) font_size_label: gtk::Label,
    pub(crate) line_height_label: gtk::Label,
    pub(crate) column_width_label: gtk::Label,
    pub(crate) wheel_step_label: gtk::Label,
    pub(crate) arrow_step_label: gtk::Label,
    pub(crate) arrow_step_row: gtk::Box,
    pub(crate) keybind_buttons: Vec<(super::keybinds::ReaderAction, gtk::Button)>,
    pub(crate) dual_page_switch: gtk::Switch,
}

#[derive(Debug, Clone)]
#[allow(dead_code)] // reader->app messages; some variants are not yet emitted
pub enum ReaderMsg {
    Close,
    MinimizeToBubble,
    TocSelect(usize),
    PrevChapter,
    NextChapter,
    FontDelta(i32),
    LineHeightDelta(i32),
    ColumnWidthDelta(i32),
    /// Font picker (roadmap 2.3): the chosen family, "" = default.
    SetFontFamily(String),
    /// Roadmap 2.4 text-layout toggles.
    SetJustify(bool),
    SetPublisherStyles(bool),
    /// Roadmap 2.7: facing pages, or one page at a time.
    SetDualPage(bool),
    /// Roadmap 2.9: hide the pointer when it sits still.
    SetAutohideCursor(bool),
    /// Roadmap 2.9: how far one wheel notch scrolls.
    WheelStepDelta(i32),
    /// How far one arrow key step / hold tick scrolls in continuous mode.
    ArrowStepDelta(i32),
    /// Roadmap 2.9: a reader moved an action to a different key.
    SetKeyBinding(super::keybinds::ReaderAction, gtk::gdk::Key, bool),
    /// Roadmap 2.9: every action back on the key it shipped with.
    ResetKeyBindings,
    /// Roadmap 2.8: open the folder a reader drops typefaces into.
    OpenFontsFolder,
    /// Roadmap 2.5 footnotes: the engine has the text behind an internal
    /// link and is offering it before the reader moves. `x`/`y` are the
    /// press, in the drawing area's coordinates.
    ShowNote {
        href: String,
        text: String,
        x: f64,
        y: f64,
    },
    /// The note card's escape hatch: go to the note itself.
    GoToNote(String),
    /// The note card closed on its own.
    ClearNote,
    SwitchSettingsPane(ReaderSettingsPane),
    SetDictSenseHint(bool),
    /// Phase 10: `dict_history_enabled` — records every dictionary lookup.
    SetDictHistory(bool),
    /// Single-tap word lookup setting preference.
    /// From the engine: single-tap word event (word, sentence, anchor rect, optional highlight ID).
    /// From the engine: the chapter on screen and how far into it.
    EnginePosition(usize, f64),
    /// From the engine: a finished selection (text, where), or cleared.
    EngineSelection(Option<(String, gtk::gdk::Rectangle)>),
    /// From the engine, after layout moved the selected text's visible rect.
    EngineSelectionMoved(Option<gtk::gdk::Rectangle>),
    /// From the selection chip.
    HighlightSelection(String),
    SaveAnnotationDetails {
        color: String,
        style: String,
        note: String,
    },
    EditAnnotationDetails {
        id: i64,
        color: String,
        style: String,
        note: String,
    },
    HighlightTapped(i64, f64, f64),
    WordMemoryTapped(String, f64, f64),
    WordMemoryHover(Option<String>, f64, f64),
    SetWordMemoryScope(String),
    QuoteSelection,
    LookUpSelection,
    CopySelection,
    /// Settings: one page at a time, or one long strip.
    SetScrolled(bool),
    Progress(f64),
    AnnotationsReload,
    DeleteAnnotation(i64),
    RecolorAnnotation(i64, HighlightColor),
    AnnotationSearchChanged(String),
    DeleteBookmark(i64),
    ToggleAnnotation(i64),
    AnnotationNoteChanged(i64, String),
    SaveAnnotationNote(i64, String),
    JumpToChapter(usize),
    /// Roadmap 2.6: back to where the last jump started.
    JumpBack,
    JumpToLocation(usize, f64),
    DictSearch(String),
    DictSearchSelect(String),
    SaveCurrentWord,
    ClearDict,
    /// The ribbon and the B key: set the mark here, or take it away
    /// (decision 16). Quiet either way — the ribbon is the feedback.
    ToggleBookmark,
    OpenAuthor(String),
    OpenLeftSidebar,
    OpenRightSidebar,
    SwitchLeftTab(LeftSidebarTab),
    SwitchRightTab(RightSidebarTab),
    SetHighlightFilter(HighlightFilter),
    /// Write the book's highlights/quotes/notes to ~/Highlights.md.
    ExportAnnotations,
    SetWordScope(WordScope),
    CloseSidebars,
    /// One Escape, layered (decisions 14, 20, 31): the frontmost open
    /// thing closes; only a bare page exits the book.
    Escape,
    ShowBackChrome,
    /// The pointer entered or left the top-left rest zone (decision 15).
    BackZoneHover(bool),
    /// The 250 ms rest elapsed; reveal if the generation still stands.
    BackRevealDue(u64),
    /// The 300 ms grace elapsed; hide if the generation still stands.
    BackHideDue(u64),
    /// Hover over the location text: it names the chapter (decision 17).
    LocationHover(bool),
    ToggleSearch,
    UpdateSearchQuery(String),
    NextSearchResult,
    PrevSearchResult,
    JumpToSearchResult(usize),
    CloseSearch,
    OpenImageLightbox(u32, u32, Vec<u8>),
    CloseImageLightbox,
    LightboxZoomIn,
    LightboxZoomOut,
    LightboxReset,
    LightboxRotateLeft,
    LightboxRotateRight,
    UserScrolled,
}
