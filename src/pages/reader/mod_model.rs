//! ReaderModel struct definition.

use super::types::*;
use crate::db::{Annotation, DictEntry, ReadingBookmark, SavedWord};
use crate::epub_book::ReadingTheme;
use crate::service::LibraryService;
use gtk::glib;
use std::path::PathBuf;

pub struct ReaderModel {
    pub(crate) service: LibraryService,
    pub(crate) book_id: i64,
    pub(crate) book_title: String,
    pub(crate) book_authors: String,
    pub(crate) book_cover_path: Option<PathBuf>,
    /// The reading widget; `None` when the book failed to open.
    pub(crate) view: Option<kalam_reader::ReaderView>,
    /// Chapter count and titles, kept so the page works without a view.
    pub(crate) chapter_count: usize,
    pub(crate) chapter_titles: Vec<String>,
    pub(crate) chapter: usize,
    pub(crate) fraction: f64,
    pub(crate) theme: ReadingTheme,
    pub(crate) font_px: u32,
    pub(crate) line_height: f32,
    pub(crate) column_px: u32,
    /// The chip over the current selection and the dictionary popover,
    /// so they can be taken down again.
    pub(crate) selection_chip: Option<gtk::Popover>,
    pub(crate) dict_popover: Option<gtk::Popover>,
    /// Where the last word tap was, for the popover that follows it.
    pub(crate) dict_anchor: Option<gtk::gdk::Rectangle>,
    /// How many pops-down of a *replaced* dictionary popover are still in
    /// flight. Each replaced popover reports itself closed on its way out
    /// and that report must not close its successor.
    pub(crate) dict_suppress_clear: u32,
    /// One page at a time, or one long strip, and the strip's scrollbar
    /// (shown only in strip mode).
    pub(crate) scrolled: bool,
    pub(crate) strip_scrollbar: Option<gtk::Scrollbar>,
    pub(crate) chapter_annotations: Vec<Annotation>,
    pub(crate) all_book_annotations: Vec<Annotation>,
    pub(crate) annotation_search_query: String,
    pub(crate) editing_annotation: Option<i64>,
    pub(crate) annotation_note_draft: Option<(i64, String)>,
    pub(crate) bookmarks: Vec<ReadingBookmark>,
    pub(crate) saved_words: Vec<SavedWord>,
    pub(crate) dict_query: String,
    pub(crate) dict_results: Vec<DictEntry>,
    pub(crate) dict_lookup_word: Option<String>,
    pub(crate) dict_lookup_def: Option<String>,
    pub(crate) dict_context: Option<String>,
    pub(crate) last_selection: Option<String>,
    pub(crate) session_id: Option<i64>,
    pub(crate) session_start: std::time::Instant,
    pub(crate) session_start_pct: i64,
    pub(crate) left_tab: LeftSidebarTab,
    pub(crate) right_tab: RightSidebarTab,
    pub(crate) left_sidebar_open: bool,
    pub(crate) right_sidebar_open: bool,
    pub(crate) highlight_filter: HighlightFilter,
    pub(crate) word_scope: WordScope,
    pub(crate) settings_pane: ReaderSettingsPane,
    pub(crate) ui_prefs: ReaderUiPrefs,
    pub(crate) show_back_button: bool,
    pub(crate) show_bottom_pill: bool,
    pub(crate) left_close_timer: Option<glib::SourceId>,
    pub(crate) right_close_timer: Option<glib::SourceId>,
    pub(crate) left_close_token: u64,
    pub(crate) right_close_token: u64,
    pub(crate) left_sidebar_shell: Option<gtk::Revealer>,
    pub(crate) right_sidebar_shell: Option<gtk::Revealer>,
    pub(crate) left_sidebar_box: Option<gtk::Box>,
    pub(crate) right_sidebar_box: Option<gtk::Box>,
    pub(crate) back_dock: Option<gtk::Box>,
    pub(crate) bottom_dock: Option<gtk::Box>,
    pub(crate) left_stack: gtk::Stack,
    pub(crate) right_stack: gtk::Stack,
    pub(crate) toc_scroll: gtk::ScrolledWindow,
    pub(crate) toc_list: gtk::Box,
    pub(crate) highlights_list: gtk::Box,
    pub(crate) bookmarks_list: gtk::Box,
    pub(crate) words_list: gtk::Box,
    pub(crate) settings_stack: gtk::Stack,
    pub(crate) settings_pane_buttons: Vec<(ReaderSettingsPane, gtk::Button)>,
    pub(crate) font_size_label: gtk::Label,
    pub(crate) line_height_label: gtk::Label,
    pub(crate) column_width_label: gtk::Label,
    pub(crate) theme_dots: Vec<(ReadingTheme, gtk::Button)>,
    pub(crate) ui_controls: Vec<(ReaderUiSetting, ReaderUiSettingControls)>,
    pub(crate) highlight_filter_buttons: Vec<(HighlightFilter, gtk::Button)>,
    pub(crate) word_scope_buttons: Vec<(WordScope, gtk::Button)>,
    pub(crate) ui_css_provider: gtk::CssProvider,
}

impl ReaderModel {
    /// Run `f` on the reading widget, if the book opened.
    pub(crate) fn with_view(&self, f: impl FnOnce(&kalam_reader::ReaderView)) {
        if let Some(view) = &self.view {
            f(view);
        }
    }
}
