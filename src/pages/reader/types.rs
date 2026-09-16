//! Reader component messages, payload types, preference structures, and sidebar enums.

use crate::db::HighlightColor;
use crate::epub_book::ReadingTheme;
use relm4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

#[derive(Debug)]
pub enum ReaderOut {
    Close,
    OpenAuthor { name: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum LeftSidebarTab {
    Toc,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RightSidebarTab {
    Highlights,
    Bookmarks,
    Words,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HighlightFilter {
    All,
    Yellow,
    Green,
    Blue,
    Pink,
    Orange,
    Quotes,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WordScope {
    Chapter,
    Book,
    All,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReaderSettingsPane {
    Reading,
    Ui,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReaderUiSetting {
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
#[allow(dead_code)]
pub enum ReaderMsg {
    Close,
    TocSelect(usize),
    PrevChapter,
    NextChapter,
    Theme(ReadingTheme),
    FontDelta(i32),
    LineHeightDelta(i32),
    ColumnWidthDelta(i32),
    SwitchSettingsPane(ReaderSettingsPane),
    SetUiSetting(ReaderUiSetting, i32),
    AdjustUiSetting(ReaderUiSetting, i32),
    SetDictSenseHint(bool),
    /// Phase 10: `dict_history_enabled` — records every dictionary lookup.
    SetDictHistory(bool),
    /// From the engine: the chapter on screen and how far into it.
    EnginePosition(usize, f64),
    /// From the engine: a finished selection (text, where), or cleared.
    EngineSelection(Option<(String, gtk::gdk::Rectangle)>),
    /// From the selection chip.
    HighlightSelection(String),
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
}
