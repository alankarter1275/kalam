//! Reader component messages, payload types, preference structures, and sidebar enums.

use crate::db::HighlightColor;
use crate::epub_book::ReadingTheme;
use relm4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

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
    Ui,
    Shortcuts,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReaderUiSetting {
    SidebarGap,
    LeftSidebarWidth,
    RightSidebarWidth,
    SidebarRadius,
    SidebarPadding,
    SidebarShadow,
    BottomPillSize,
    BottomPillGap,
    BackChipSize,
    BackTopGap,
    BackSideGap,
    DimStrength,
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct ReaderUiPrefs {
    pub(crate) sidebar_gap: i32,
    pub(crate) left_sidebar_width: i32,
    pub(crate) right_sidebar_width: i32,
    pub(crate) sidebar_radius: i32,
    pub(crate) sidebar_padding: i32,
    pub(crate) sidebar_shadow: i32,
    pub(crate) bottom_pill_size: i32,
    pub(crate) bottom_pill_gap: i32,
    pub(crate) back_chip_size: i32,
    pub(crate) back_top_gap: i32,
    pub(crate) back_side_gap: i32,
    pub(crate) dim_strength: i32,
}

pub(crate) struct ReaderUiSettingControls {
    pub(crate) value_entry: gtk::Entry,
    pub(crate) last_value: Rc<RefCell<i32>>,
    pub(crate) preset_buttons: Vec<(i32, gtk::Button)>,
    pub(crate) custom_button: gtk::Button,
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
    pub(crate) theme_dots: Vec<(ReadingTheme, gtk::Button)>,
    pub(crate) ui_controls: Vec<(ReaderUiSetting, ReaderUiSettingControls)>,
}

pub(crate) const UI_PRESETS_SIDEBAR_GAP: [(&str, i32); 3] = [("Tight", 8), ("Normal", 14), ("Airy", 20)];
pub(crate) const UI_PRESETS_LEFT_WIDTH: [(&str, i32); 3] = [("Narrow", 220), ("Normal", 248), ("Wide", 280)];
pub(crate) const UI_PRESETS_RIGHT_WIDTH: [(&str, i32); 3] = [("Narrow", 180), ("Normal", 212), ("Wide", 252)];
pub(crate) const UI_PRESETS_RADIUS: [(&str, i32); 3] = [("Soft", 14), ("Round", 20), ("Full", 26)];
pub(crate) const UI_PRESETS_PADDING: [(&str, i32); 3] = [("Tight", 0), ("Normal", 4), ("Airy", 8)];
pub(crate) const UI_PRESETS_SHADOW: [(&str, i32); 3] = [("Low", 18), ("Normal", 22), ("Deep", 28)];
pub(crate) const UI_PRESETS_PILL_SIZE: [(&str, i32); 3] = [("Compact", 30), ("Normal", 34), ("Large", 40)];
pub(crate) const UI_PRESETS_PILL_GAP: [(&str, i32); 3] = [("Tight", 12), ("Normal", 18), ("Airy", 26)];
pub(crate) const UI_PRESETS_BACK_SIZE: [(&str, i32); 3] = [("Compact", 28), ("Normal", 32), ("Large", 38)];
pub(crate) const UI_PRESETS_BACK_TOP: [(&str, i32); 3] = [("Tight", 10), ("Normal", 14), ("Airy", 20)];
pub(crate) const UI_PRESETS_BACK_SIDE: [(&str, i32); 3] = [("Tight", 12), ("Normal", 16), ("Airy", 22)];
pub(crate) const UI_PRESETS_DIM: [(&str, i32); 3] = [("Light", 12), ("Medium", 22), ("Strong", 32)];

#[derive(Debug, Clone)]
#[allow(dead_code)] // reader->app messages; some variants are not yet emitted
pub enum ReaderMsg {
    Close,
    MinimizeToBubble,
    TocSelect(usize),
    PrevChapter,
    NextChapter,
    Theme(ReadingTheme),
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
    SetUiSetting(ReaderUiSetting, i32),
    AdjustUiSetting(ReaderUiSetting, i32),
    SetDictSenseHint(bool),
    /// Phase 10: `dict_history_enabled` — records every dictionary lookup.
    SetDictHistory(bool),
    /// Single-tap word lookup setting preference.
    /// From the engine: single-tap word event (word, sentence, anchor rect, optional highlight ID).
    /// From the engine: the chapter on screen and how far into it.
    EnginePosition(usize, f64),
    /// From the engine: a finished selection (text, where), or cleared.
    EngineSelection(Option<(String, gtk::gdk::Rectangle)>),
    /// From the engine, after a draw moved it: where the selection now
    /// sits in widget coordinates (`None` = gone/off screen). The inline
    /// editor follows its text with this.
    EngineSelectionMoved(Option<gtk::gdk::Rectangle>),
    /// Phase 6.4: the pencil on the selection chip was pressed — open
    /// the inline editor over the selection.
    BeginInlineEdit,
    /// Phase 6.7: the proofreading pencil in the chrome was toggled.
    ToggleProofreading(bool),
    /// Phase 6.7: a proofread tap found — or failed to yield — an
    /// editable paragraph. Ok carries the paragraph's identity (its
    /// text and neighbours' texts) and the rect its editor opens over.
    EngineParagraphTap(Option<(kalam_reader::ParagraphIdentity, gtk::gdk::Rectangle)>),
    /// Commit the inline edit with this text (Enter or click-away).
    CommitInlineEdit(String),
    /// Abandon the inline edit (Escape).
    CancelInlineEdit,
    /// The background verification of an inline edit finished. Ok
    /// carries the verified edit — href, patch kind, the serial of the
    /// editor it came from, and the planned patch to store; Err carries
    /// ready-to-show toast text for a refusal (unchanged, not locatable,
    /// ambiguous) plus that editor's serial — a refusal of the
    /// still-open editor re-arms it for another try, while the editor
    /// stays open either way.
    InlineEditVerified(Result<VerifiedEdit, (String, u64)>),
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
    AddBookmark,
    OpenAuthor(String),
    OpenLeftSidebar,
    OpenRightSidebar,
    SwitchLeftTab(LeftSidebarTab),
    SwitchRightTab(RightSidebarTab),
    SetHighlightFilter(HighlightFilter),
    /// Write the book's highlights/quotes/notes to ~/Highlights.md.
    ExportAnnotations,
    SetWordScope(WordScope),
    ScheduleCloseLeft,
    ScheduleCloseRight,
    ForceCloseLeft(u64),
    ForceCloseRight(u64),
    CloseSidebars,
    HideChrome,
    ShowBackChrome,
    ShowBottomChrome,
    ShowAllChrome,
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
    BackChromeTimerTick,
    BottomChromeTimerTick,
    UserScrolled,
    TopEdgeHover(bool),
    BottomEdgeHover(bool),
}

/// The scope an open inline edit covers: a selection-level typo fix
/// (phase 6.4), or a whole paragraph in proofreading mode (phase 6.7)
/// — one editing feel, two granularities.
pub(crate) enum EditScope {
    /// The selected run, matched in text space by the step-2 matcher.
    Selection,
    /// A whole paragraph, matched through the step-6 span mapper. The
    /// neighbours are the identity the proofread tap carried.
    Paragraph {
        prev: Option<String>,
        next: Option<String>,
    },
}

/// The editor widget of an open edit: an entry for a selection, a
/// wrapped multi-line view for a paragraph — a paragraph keeps its
/// `<br>` newlines, which an entry cannot hold.
pub(crate) enum EditorWidget {
    Selection(gtk::Entry),
    Paragraph(gtk::TextView),
}

impl EditorWidget {
    /// The text as it stands — the entry's text, or the view's buffer.
    pub(crate) fn text(&self) -> String {
        match self {
            EditorWidget::Selection(entry) => entry.text().to_string(),
            EditorWidget::Paragraph(view) => {
                let buffer = view.buffer();
                let (start, end) = buffer.bounds();
                buffer.text(&start, &end, false).to_string()
            }
        }
    }

    /// The widget to overlay, position, and eventually unparent.
    pub(crate) fn widget(&self) -> &gtk::Widget {
        match self {
            EditorWidget::Selection(entry) => entry.upcast_ref(),
            EditorWidget::Paragraph(view) => view.upcast_ref(),
        }
    }
}

/// An open inline edit: the original text, the chapter it was made in,
/// its scope, and the editor laid over it with the provider that
/// typesets it in the reader's own face. The editor is the source of
/// truth for the current text; `original` is what a commit is planned
/// against.
pub(crate) struct InlineEdit {
    pub(crate) original: String,
    pub(crate) chapter: usize,
    pub(crate) scope: EditScope,
    /// Which edit this is: verdicts carry the serial of the editor they
    /// verified, so a verdict that lands after its editor was replaced
    /// (a quick second tap) still stores its patch — without tearing
    /// down the editor that replaced it.
    pub(crate) serial: u64,
    /// Set once a commit has been dispatched and no verdict has
    /// landed: Enter, click-away, and focus-out can all race the
    /// verdict, and none of them may re-send what another already
    /// sent. A refusal of this editor's own verdict re-arms it.
    pub(crate) committed: bool,
    pub(crate) editor: EditorWidget,
    /// The editor's typeface provider, registered on the display by
    /// `engine::build_inline_editor` / `engine::build_paragraph_editor`
    /// and unregistered when this edit closes, so sessions do not
    /// accumulate one per edit.
    pub(crate) provider: gtk::CssProvider,
}

/// A verified edit coming back from the worker thread: where it lives,
/// what kind of patch it is, the serial of the editor it verified, and
/// the ready-to-store plan.
pub(crate) struct VerifiedEdit {
    pub(crate) href: String,
    pub(crate) kind: &'static str,
    /// The spine index the edit was made in — the verdict's own, since
    /// the editor it verified may already be gone.
    pub(crate) chapter: usize,
    pub(crate) serial: u64,
    pub(crate) planned: crate::epub_patches::PlannedPatch,
}
