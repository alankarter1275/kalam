//! Reader component messages, payload types, preference structures, and sidebar enums.

use crate::db::HighlightColor;
use relm4::prelude::*;

/// Move the book-theme dropdown to a theme id without firing it
/// (`None` = follow the app theme) — the silent setter the model's
/// control sync drives (decision 145).
pub(crate) type BookThemeSetter = std::rc::Rc<dyn Fn(Option<&str>)>;

#[derive(Debug)]
pub enum ReaderOut {
    Close,
    MinimizeToBubble { book_id: i64 },
    OpenAuthor { name: String },
    /// The book head's edit pencil (decision 20): the book's own page,
    /// pushed over the reader so Back returns to the page.
    OpenBook { book_id: i64 },
}

/// The left panel's two tabs (decision 20). Settings lives alone in
/// the right panel now (decision 23), so there is no right tab enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeftSidebarTab {
    Contents,
    Notes,
}

/// The Notes tab's filter chips (decision 20): one list holding the
/// book's highlights, bookmarks and saved words.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotesFilter {
    All,
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
    /// The Reading preview's page text and its column, repainted by
    /// `chrome::sync_reader_controls` as the choices move.
    pub(crate) preview_body: gtk::Label,
    pub(crate) preview_column: gtk::Box,
    /// Set the "Settings for this book" switch without re-firing it:
    /// editing a value turns the override on from *inside* the model
    /// (decision 147), so the switch must be settable from sync.
    pub(crate) set_book_override_switch: std::rc::Rc<dyn Fn(bool)>,
    /// Same for the book-theme dropdown's standing choice.
    pub(crate) set_book_theme_choice: BookThemeSetter,
    /// "Use these settings as my defaults" / "Matches your defaults".
    pub(crate) use_defaults_btn: gtk::Button,
    /// The Shortcuts pane's hint line: where a refused key names its
    /// owner (decision 24).
    pub(crate) keybind_hint: gtk::Label,
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
    /// From the selection chip: one tap on a colour dot highlights
    /// with the last-used style (decision 26).
    HighlightSelection(String),
    /// The chip's Note: highlight in the last-used colour and style,
    /// then open the annotation popover on its note box (decision 26).
    NoteSelection,
    /// A colour dot or style sample in the standing annotation
    /// popover: restyle at once, the page behind shows it (29).
    RestyleAnnotation {
        id: i64,
        color: String,
        style: String,
    },
    /// The annotation popover went away: its final colour, style and
    /// note land together (autosave on close, decision 29).
    AnnotationEditClosed {
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
    /// In-popup navigation (decision 31): back to the word the reader
    /// came from. The stack resets on every fresh lookup.
    DictGoBack,
    /// The header's circular toggle (decision 31): save the standing
    /// word, or forget it. The popover stays up either way.
    ToggleSaveWord,
    /// A saved word's delete in the Notes list (decision 22).
    DeleteSavedWord(i64),
    ClearDict,
    /// The ribbon and the B key: set the mark here, or take it away
    /// (decision 16). Quiet either way — the ribbon is the feedback.
    ToggleBookmark,
    OpenAuthor(String),
    /// The book head's edit pencil: out to the shell as
    /// `ReaderOut::OpenBook`.
    OpenBookDetails,
    OpenLeftSidebar,
    OpenRightSidebar,
    ToggleRightSidebar,
    /// The right panel holds Settings only (decision 23). Summoning
    /// it while it stands puts it away again.
    OpenSettingsPanel,
    /// The Contents summon key: a toggle, where `SwitchLeftTab`
    /// (the tab buttons) only ever switches.
    OpenContents,
    SwitchLeftTab(LeftSidebarTab),
    /// Open Notes on one of its filters (the H and W keys); asking
    /// for the view already standing closes the panel.
    OpenNotes(NotesFilter),
    SetNotesFilter(NotesFilter),
    SetHighlightFilter(HighlightFilter),
    /// "Settings for this book" (decision 147): on snapshots the
    /// standing choices as the book's own; off returns to the
    /// defaults (Reset to my defaults is the same gesture).
    SetBookOverride(bool),
    /// Copy the standing choices over the global `reader.*` defaults.
    UseSettingsAsDefaults,
    /// The book's own theme, or `None` to follow the app (145).
    SetBookTheme(Option<String>),
    /// The shell's configurable Close-panel key, when it is not
    /// Escape: Escape's layering minus leaving the book.
    ClosePanelKey,
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
