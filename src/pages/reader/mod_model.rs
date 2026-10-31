//! ReaderModel struct definition.

use super::types::*;
use crate::db::{
    Annotation, AnnotationStyle, DictEntry, HighlightColor, ReadingBookmark, SavedWord,
};
use crate::service::LibraryService;
use kalam_reader::ReaderPalette;
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
    /// The page's resolved palette (decisions 19/145): the app theme,
    /// or the book's own theme when it keeps one.
    pub(crate) palette: ReaderPalette,
    pub(crate) font_px: u32,
    pub(crate) line_height: f32,
    pub(crate) column_px: u32,
    /// The body typeface chosen in the font picker; `None` = default.
    pub(crate) font_family: Option<String>,
    /// Roadmap 2.4 text-layout toggles.
    pub(crate) justify: bool,
    pub(crate) publisher_styles: bool,
    /// Roadmap 2.7: facing pages when the window is wide enough.
    pub(crate) dual_page: bool,
    /// Roadmap 2.9: the reader's keys. Shared with the key controller, so
    /// a change in settings reaches the handler without rebuilding it.
    pub(crate) keybinds: std::rc::Rc<std::cell::RefCell<super::keybinds::KeyBindings>>,
    /// The buttons that show those keys, so a change can repaint them.
    pub(crate) keybind_buttons: Vec<(super::keybinds::ReaderAction, gtk::Button)>,
    /// Roadmap 2.7's spread toggle, kept so the shell can grey it while
    /// the reader is in continuous scroll, where a spread never applies
    /// (sensitivity only — the pref itself is untouched).
    pub(crate) dual_page_switch: gtk::Switch,
    /// The window-global jump-back shortcut (roadmaps 2.6/2.9), swapped in
    /// place whenever the key is rebound. Keyboard-only on purpose: the
    /// on-screen undo button read as clutter and went away.
    pub(crate) jump_back_shortcut: std::rc::Rc<std::cell::RefCell<Option<gtk::ShortcutController>>>,
    /// Roadmap 2.9: hide the pointer when it sits still.
    pub(crate) autohide_cursor: bool,
    /// Roadmap 2.9: CSS px per wheel notch, and the label showing it.
    pub(crate) wheel_step: i32,
    pub(crate) wheel_step_label: gtk::Label,
    /// CSS px per arrow step in continuous scroll, its label and row widget.
    pub(crate) arrow_step: i32,
    pub(crate) arrow_step_label: gtk::Label,
    pub(crate) arrow_step_row: gtk::Box,
    /// The chip over the current selection and the dictionary popover,
    /// so they can be taken down again.
    /// Roadmap 2.6: the jump-back offer, standing while it is fresh.
    pub(crate) can_jump_back: bool,
    /// Last seen depth of the engine's back trail, so a jump shows up as
    /// the number going up.
    pub(crate) back_depth: usize,
    pub(crate) selection_chip: Option<gtk::Popover>,
    /// The open annotation editor (decision 29), in its own slot: its
    /// closed-autosave lands by message, and that message must only
    /// ever clear this popover, never a chip raised after it.
    pub(crate) annotation_popover: Option<gtk::Popover>,
    pub(crate) word_preview_popover: Option<gtk::Popover>,
    /// The read-only note shown while a highlight's dot is hovered
    /// (decision 29), and the annotation it belongs to — so motion
    /// inside the same dot does not rebuild it.
    pub(crate) note_preview_popover: Option<gtk::Popover>,
    pub(crate) note_preview_id: Option<i64>,
    pub(crate) word_memory_scope: String,
    /// Roadmap 2.5: the footnote card, while it stands.
    pub(crate) note_popover: Option<gtk::Popover>,
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
    /// The reading session's deletions, last several, for Ctrl+Z
    /// (decision 30). Rows as they stood, so an undo is a restore,
    /// not a fresh mark.
    pub(crate) deleted_marks: Vec<DeletedMark>,
    pub(crate) dict_query: String,
    pub(crate) dict_results: Vec<DictEntry>,
    pub(crate) dict_lookup_word: Option<String>,
    pub(crate) dict_lookup_def: Option<String>,
    pub(crate) dict_context: Option<String>,
    /// In-popup navigation (decision 31): words the reader wandered
    /// from via synonym/antonym/did-you-mean chips, most recent last.
    /// A fresh lookup from the page empties it.
    pub(crate) dict_back_stack: Vec<String>,
    pub(crate) last_selection: Option<String>,
    /// The colour and style a fresh highlight takes (decision 26):
    /// wherever the selection chip's dots and the annotation popover
    /// last left them, kept across sessions as plain prefs.
    pub(crate) last_highlight_color: HighlightColor,
    pub(crate) last_highlight_style: AnnotationStyle,
    pub(crate) session_id: Option<i64>,
    pub(crate) session_start: std::time::Instant,
    pub(crate) session_start_pct: i64,
    pub(crate) left_tab: LeftSidebarTab,
    pub(crate) left_sidebar_open: bool,
    pub(crate) right_sidebar_open: bool,
    pub(crate) notes_filter: NotesFilter,
    pub(crate) highlight_filter: HighlightFilter,
    pub(crate) word_scope: WordScope,
    pub(crate) settings_pane: ReaderSettingsPane,
    /// The book's own reading settings, when it keeps them (decision
    /// 147). `Some` is the "Settings for this book" switch; the rest
    /// of the settings fields mirror it while it stands.
    pub(crate) book_override: Option<super::book_settings::BookReadingSettings>,
    /// The book's own theme id, when its page does not follow the
    /// app theme (decision 145).
    pub(crate) theme_override: Option<String>,
    /// The Back+Bubble unit's shown state (decision 15).
    pub(crate) show_back_button: bool,
    /// Pointer resting in the top-left zone or on the unit itself.
    pub(crate) back_hover: bool,
    /// Generation for the Back unit's reveal/hide timers; a timer only
    /// acts while its generation is still current.
    pub(crate) back_gen: u64,
    /// The Back pill with its Bubble slide-out, built at open.
    pub(crate) back_unit: Option<crate::widgets::slide_out_pill::SlideOutPill>,
    /// Pointer over the location text: it names the chapter (17).
    pub(crate) location_hover: bool,
    pub(crate) left_sidebar_shell: Option<gtk::Revealer>,
    pub(crate) right_sidebar_shell: Option<gtk::Revealer>,
    pub(crate) left_sidebar_box: Option<gtk::Box>,
    pub(crate) right_sidebar_box: Option<gtk::Box>,
    pub(crate) left_stack: gtk::Stack,
    pub(crate) toc_scroll: gtk::ScrolledWindow,
    pub(crate) toc_list: gtk::Box,
    /// The one list the Notes tab renders into, whatever its filter.
    pub(crate) notes_list: gtk::Box,
    pub(crate) notes_filter_buttons: Vec<(NotesFilter, gtk::Button)>,
    /// The contextual control rows under the Notes chips, shown one
    /// at a time by the standing filter.
    pub(crate) notes_highlights_controls: gtk::Box,
    pub(crate) notes_bookmarks_controls: gtk::Box,
    pub(crate) notes_words_controls: gtk::Box,
    pub(crate) settings_stack: gtk::Stack,
    pub(crate) settings_pane_buttons: Vec<(ReaderSettingsPane, gtk::Button)>,
    pub(crate) font_size_label: gtk::Label,
    pub(crate) line_height_label: gtk::Label,
    pub(crate) column_width_label: gtk::Label,
    pub(crate) highlight_filter_buttons: Vec<(HighlightFilter, gtk::Button)>,
    pub(crate) word_scope_buttons: Vec<(WordScope, gtk::Button)>,
    pub(crate) preview_body: gtk::Label,
    pub(crate) preview_column: gtk::Box,
    /// The preview's words: this book's own where the chapter could
    /// be read, the app's sample otherwise.
    pub(crate) preview_text: String,
    pub(crate) set_book_override_switch: std::rc::Rc<dyn Fn(bool)>,
    pub(crate) set_book_theme_choice: super::types::BookThemeSetter,
    pub(crate) use_defaults_btn: gtk::Button,
    pub(crate) keybind_hint: gtk::Label,
    pub(crate) search_active: bool,
    pub(crate) search_query: String,
    /// The engine's whole-book results; the Chapter / Book switch
    /// (decision 34) is a view over them, never a second search.
    pub(crate) search_results: Vec<kalam_reader::SearchResult>,
    /// Position inside the *scoped* results, not the raw list.
    pub(crate) search_index: usize,
    pub(crate) search_scope: SearchScope,
    /// The bar's Chapter / Book switch, kept so closing the bar can
    /// put it back on Chapter (the standing choice, decision 34).
    pub(crate) search_scope_control: Option<crate::widgets::segmented::SegmentedControl>,
    pub(crate) lightbox_active: bool,
    pub(crate) lightbox_popover: Option<gtk::Popover>,
    pub(crate) lightbox_pixbuf: Option<gdk_pixbuf::Pixbuf>,
    pub(crate) lightbox_zoom: f64,
    pub(crate) lightbox_rotation: f64,
}

impl ReaderModel {
    /// Run `f` on the reading widget, if the book opened.
    pub(crate) fn with_view(&self, f: impl FnOnce(&kalam_reader::ReaderView)) {
        if let Some(view) = &self.view {
            f(view);
        }
    }

    /// The whole-book results the standing Chapter / Book scope can
    /// see, as indices into `search_results` (decision 34).
    pub(crate) fn scoped_search_indices(&self) -> Vec<usize> {
        scoped_search_indices(
            self.search_results.iter().map(|res| res.chapter),
            self.search_scope,
            self.chapter,
        )
    }

    pub(crate) fn scoped_search_len(&self) -> usize {
        self.scoped_search_indices().len()
    }

    /// The current match's index into the raw result list, if the
    /// scoped position still lands on one.
    pub(crate) fn current_search_result_index(&self) -> Option<usize> {
        self.scoped_search_indices().get(self.search_index).copied()
    }

    pub(crate) fn search_no_matches(&self) -> bool {
        search_has_no_matches(&self.search_query, self.scoped_search_len())
    }

    pub(crate) fn search_count_label(&self) -> String {
        search_count_text(
            &self.search_query,
            self.scoped_search_len(),
            self.search_index,
        )
    }

    /// Keep a deletion for Ctrl+Z (decision 30): the last several,
    /// for this reading session only.
    pub(crate) fn remember_deleted(&mut self, mark: DeletedMark) {
        const KEPT: usize = 20;
        self.deleted_marks.push(mark);
        if self.deleted_marks.len() > KEPT {
            self.deleted_marks.remove(0);
        }
    }

    /// Back to Chapter, switch included (decision 34): every fresh
    /// bar starts there, whatever the last one was left on.
    pub(crate) fn reset_search_scope(&mut self) {
        self.search_scope = SearchScope::Chapter;
        if let Some(control) = &self.search_scope_control {
            control.set_selected(0);
        }
    }
}
