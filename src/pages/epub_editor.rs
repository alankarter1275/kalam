//! Deep EPUB editor workspace (editor-spec.md, decisions 118–143).
//!
//! A docked, flat-chrome page: top bar (Back pill, title, Save), the
//! file tree, the editing area (formatting toolbar, tabs, source
//! view), the right panel (Preview, Outline, Problems, Live CSS) and
//! the status bar with the Code / Split / Preview switch.
//!
//! This lands in tranches like the readers did. E1 is the shell,
//! source editing and the save path (snapshots, validation, both
//! save dialogs); the right panel's Preview is a plain-text render
//! until the engine-backed one arrives in E2, where Live CSS also
//! lands. Tools, Find & replace and the command palette are E3, so
//! the top bar has no Tools/History buttons yet — dead controls are
//! not drawn (the same rule the book page's ⋯ menu followed).

use crate::epub_check::{BookProblem, ProblemFix, Severity};
use crate::epub_container::{EpubContainer, EpubFile, FileKind, TreeSection};
use crate::epub_css::CssRule;
use crate::models::Book;
use crate::service::LibraryService;
use crate::widgets::dialog::{dialog_shell, present_dialog, DialogSize};
use crate::widgets::in_app_dialog::{DialogExit, InAppDialog};
use crate::widgets::menu::MenuPopover;
use crate::widgets::pill_button::{boxed_ghost_button, primary_button};
use gtk::prelude::*;
use relm4::prelude::*;
use sourceview5::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;

#[derive(Debug)]
pub enum EpubEditorOut {
    Leave,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViewMode {
    Code,
    Split,
    Preview,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RightTab {
    Preview,
    Outline,
    Problems,
    LiveCss,
}

/// The Preview tab's arrow menu (decision 123): the engine's
/// neutral defaults ("As the book ships", the default) or the
/// reader's own settings for this book ("As in Kalam").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewMode {
    Ships,
    Kalam,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InlineTag {
    Bold,
    Italic,
}

/// What to do once a save triggered from a dialog finishes.
#[derive(Debug, Clone, PartialEq, Eq)]
enum AfterSave {
    Nothing,
    Leave,
    CloseTab(String),
}

#[derive(Debug)]
pub enum EpubEditorMsg {
    Loaded(Box<Result<(Book, EpubContainer), String>>),
    OpenFile { path: String, pin: bool },
    CloseTab { path: String },
    ActivateTab { path: String },
    ToggleSection(TreeSection),
    BufferChanged,
    CursorMoved { line: i32, col: i32 },
    Format(InlineTag),
    SavePressed,
    SaveAnywayPressed,
    Saved(Result<(), String>),
    BackPressed,
    DiscardAndLeave,
    DiscardCloseTab { path: String },
    DialogSaveAndContinue,
    SetViewMode(ViewMode),
    SetRightTab(RightTab),
    GoTo {
        path: String,
        line: Option<usize>,
        offset: Option<i32>,
    },
    ThemeApplied,
    SchemeReady,
    ShowInFolder,
    // ── E2: engine preview, Check book, Live CSS ──
    SetPreviewMode(PreviewMode),
    /// 800ms after the last change: write the preview copy.
    PreviewWriteDue,
    /// The preview write finished; hand off to the UI thread.
    PreviewWritten(std::path::PathBuf),
    /// 1.2s after the last change: run Check book.
    CheckDue,
    FixClicked(usize),
    /// Apply the staged fix as one undo step.
    FixApply(ProblemFix, String),
    FixSkip,
    RemoveProblem(usize),
    /// The removed path, so open tabs and the tree can follow.
    FileRemoved(String),
    ActionFailed(String),
    /// A word was tapped in the preview.
    PreviewGoToWord {
        chapter: usize,
        fraction: f64,
        word: String,
    },
    /// A selection finished in the preview.
    PreviewTextSelected,
    /// A Live CSS rule header was clicked.
    GoToCssRule {
        path: String,
        line: usize,
    },
    // ── E3: tools, Find & replace, command palette ──
    OpenToolsMenu,
    /// A Clean up tool was chosen: ask scope, then run it.
    ToolChosen(InstantTool),
    RunInstantTool(InstantTool, bool),
    /// Snapshot taken: apply these whole-file replacements now.
    ApplyChanges {
        summary: String,
        changes: Vec<(String, String)>,
    },
    PillUndo,
    PillHide,
    ToggleFind,
    FindChanged,
    FindSetScope(FindScope),
    FindJump(usize),
    FindNext,
    FindReplaceOne,
    FindReplaceAll,
    SavedSearchesMenu,
    SaveThisSearch,
    UseSavedSearch(String, String),
    TogglePalette,
    PaletteChanged,
    PaletteMove(i32),
    PaletteAccept,
    PaletteRun(usize),
    /// Escape: fold away whichever floating panel is open.
    ClosePanels,
}

struct OpenTab {
    path: String,
    kind: FileKind,
    pinned: bool,
    buffer: Option<sourceview5::Buffer>,
}

/// The Clean up submenu's instant tools (decision 128): ask scope,
/// snapshot, apply, offer Undo in the after-tool pill.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstantTool {
    Smarten,
    Reformat,
    RemoveUnusedCss,
    FixHtml,
}

/// Find & replace scopes (decision 131).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FindScope {
    ThisFile,
    AllText,
    WholeBook,
}

/// One found match, ready to render as a result row.
struct FindHit {
    path: String,
    /// Char offsets into the file's current text.
    start: usize,
    end: usize,
    line: usize,
    before: String,
    matched: String,
    after: String,
}

/// How to take back one tool run (one undo step per file).
struct ToolUndo {
    entries: Vec<ToolUndoEntry>,
}

enum ToolUndoEntry {
    /// The file has a buffer: one buffer undo restores it.
    Buffer(String),
    /// The file had no tab: restore (or drop) its pending edit.
    Pending(String, Option<String>),
}

#[derive(Clone)]
enum PaletteAction {
    Save,
    CheckBook,
    FindReplace,
    Tool(InstantTool),
    View(ViewMode),
    Tab(RightTab),
    OpenFile(String),
}

#[derive(Clone)]
struct PaletteItem {
    label: String,
    detail: String,
    shortcut: Option<&'static str>,
    action: PaletteAction,
}

pub struct EpubEditorModel {
    service: LibraryService,
    book_id: i64,
    book: Option<Book>,
    container: Option<EpubContainer>,

    root_overlay: gtk::Overlay,
    body_stack: gtk::Stack,
    title_label: gtk::Label,
    save_btn: gtk::Button,
    tree_host: gtk::Box,
    tree_scroller: gtk::ScrolledWindow,
    center_box: gtk::Box,
    tab_strip: gtk::Box,
    tab_stack: gtk::Stack,
    right_panel: gtk::Box,
    right_stack: gtk::Stack,
    right_tab_btns: Vec<(RightTab, gtk::Button)>,
    view_switch_btns: Vec<(ViewMode, gtk::Button)>,
    /// The box the engine preview sits in. Until the first preview
    /// copy is written, a placeholder label fills it; the engine
    /// widget replaces the label once there is something to render.
    preview_host: gtk::Box,
    livecss_host: gtk::Box,
    outline_host: gtk::Box,
    problems_host: gtk::Box,
    status_pos: gtk::Label,
    status_type: gtk::Label,
    status_problems: gtk::Label,
    status_bar: gtk::Box,
    error_label: gtk::Label,

    tabs: Vec<OpenTab>,
    active_path: Option<String>,
    collapsed: HashSet<String>,
    view_mode: ViewMode,
    right_tab: RightTab,
    problems: Vec<BookProblem>,
    /// The modified-path set as last rendered, so tree/tab dots only
    /// rebuild when the *set* changes, not on every keystroke.
    shown_dirty: HashSet<String>,
    after_save: AfterSave,
    saving: bool,
    // ── E2: engine preview state ──
    preview_view: Option<Rc<kalam_reader::ReaderView>>,
    preview_mode: PreviewMode,
    preview_menu: Option<MenuPopover>,
    preview_write_src: Option<gtk::glib::SourceId>,
    preview_chapter: i32,
    // ── E2: Check book state ──
    check_src: Option<gtk::glib::SourceId>,
    fix_popover: Option<gtk::Popover>,
    // ── E2: outline + Live CSS state ──
    /// Heading rows of the active file: (level, text, char offset).
    outline_headings: Vec<(usize, String, i32)>,
    /// The book's parsed stylesheets, cached by content hash so
    /// cursor moves don't re-parse the CSS.
    css_cache_key: u64,
    css_cache: Vec<(String, CssRule)>,
    /// Tool and Replace-all edits to files that have no open tab
    /// (E3). A file's current text is its buffer, else its pending
    /// edit, else the container copy; Save writes pending edits
    /// alongside modified buffers.
    pending: HashMap<String, String>,
    // ── E3: tools + pill ──
    tools_btn: gtk::Button,
    tools_menu: Option<MenuPopover>,
    tool_undo: Option<ToolUndo>,
    pill: gtk::Box,
    pill_label: gtk::Label,
    pill_undo_btn: gtk::Button,
    pill_src: Option<gtk::glib::SourceId>,
    // ── E3: Find & replace ──
    find_panel: gtk::Box,
    find_entry: gtk::Entry,
    replace_entry: gtk::Entry,
    find_scope_btns: Vec<(FindScope, gtk::Button)>,
    find_scope: FindScope,
    chip_case: gtk::ToggleButton,
    chip_word: gtk::ToggleButton,
    chip_regex: gtk::ToggleButton,
    find_count: gtk::Label,
    find_results_host: gtk::Box,
    find_hits: Vec<FindHit>,
    find_current: Option<usize>,
    saved_menu: Option<MenuPopover>,
    // ── E3: command palette ──
    palette_panel: gtk::Box,
    palette_entry: gtk::Entry,
    palette_host: gtk::Box,
    palette_items: Vec<PaletteItem>,
    palette_selected: usize,
}

const EMPTY_TAB: &str = "__kalam_empty";

#[relm4::component(pub)]
impl Component for EpubEditorModel {
    type Init = (LibraryService, i64);
    type Input = EpubEditorMsg;
    type Output = EpubEditorOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Overlay {}
    }

    fn init(
        (service, book_id): Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // ── top bar (56, §32) ──────────────────────────────────────
        let back_btn = gtk::Button::new();
        back_btn.add_css_class("kalam-back-btn");
        let back_content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        back_content.append(&crate::icons::symbolic("go-previous-symbolic", 16));
        let back_label = gtk::Label::new(Some("Back"));
        back_content.append(&back_label);
        back_btn.set_child(Some(&back_content));
        back_btn.set_valign(gtk::Align::Center);
        {
            let s = sender.clone();
            back_btn.connect_clicked(move |_| s.input(EpubEditorMsg::BackPressed));
        }

        let title_label = gtk::Label::new(Some("Opening…"));
        title_label.add_css_class("kalam-ed-title");
        title_label.set_halign(gtk::Align::Start);
        title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let caption = gtk::Label::new(Some("Editing Kalam's copy · original untouched"));
        caption.add_css_class("kalam-ui-meta");
        caption.set_halign(gtk::Align::Start);
        let title_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        title_box.set_valign(gtk::Align::Center);
        title_box.set_margin_start(16);
        title_box.append(&title_label);
        title_box.append(&caption);

        let save_btn = primary_button("Save", {
            let s = sender.clone();
            move || s.input(EpubEditorMsg::SavePressed)
        });
        save_btn.add_css_class("kalam-ed-save");
        save_btn.set_valign(gtk::Align::Center);
        save_btn.set_sensitive(false);
        save_btn.set_tooltip_text(Some("Save (Ctrl+S)"));

        let top_bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        top_bar.add_css_class("kalam-ed-topbar");
        top_bar.set_size_request(-1, 56);
        top_bar.append(&back_btn);
        top_bar.append(&title_box);
        let top_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        top_spacer.set_hexpand(true);
        top_bar.append(&top_spacer);
        let tools_btn = gtk::Button::with_label("Tools");
        tools_btn.add_css_class("kalam-ed-ghost");
        tools_btn.set_valign(gtk::Align::Center);
        tools_btn.set_margin_end(8);
        tools_btn.set_focus_on_click(false);
        {
            let s = sender.input_sender().clone();
            tools_btn.connect_clicked(move |_| {
                let _ = s.send(EpubEditorMsg::OpenToolsMenu);
            });
        }
        top_bar.append(&tools_btn);
        top_bar.append(&save_btn);

        // ── file tree (240, §36) ───────────────────────────────────
        let tree_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        tree_host.set_vexpand(false);
        let tree_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::External)
            .vexpand(true)
            .build();
        tree_scroller.add_css_class("kalam-ed-tree");
        tree_scroller.set_size_request(240, -1);
        tree_scroller.set_hexpand(false);
        tree_scroller.set_child(Some(&tree_host));

        // ── editing area: toolbar, tab strip, stack ───────────────
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        toolbar.add_css_class("kalam-ed-toolbar");
        toolbar.set_size_request(-1, 48);
        for (icon, tip, tag) in [
            ("format-text-bold-symbolic", "Bold (strong)", InlineTag::Bold),
            ("format-text-italic-symbolic", "Italic (em)", InlineTag::Italic),
        ] {
            let btn = gtk::Button::new();
            btn.add_css_class("kalam-ed-tool-btn");
            btn.set_size_request(32, 32);
            btn.set_valign(gtk::Align::Center);
            btn.set_child(Some(&crate::icons::symbolic(icon, 16)));
            btn.set_tooltip_text(Some(tip));
            btn.set_focus_on_click(false);
            let s = sender.clone();
            btn.connect_clicked(move |_| s.input(EpubEditorMsg::Format(tag)));
            toolbar.append(&btn);
        }

        let tab_strip = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        tab_strip.add_css_class("kalam-ed-tabstrip");
        tab_strip.set_size_request(-1, 40);
        let tab_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .build();
        tab_scroller.set_child(Some(&tab_strip));

        let tab_stack = gtk::Stack::new();
        tab_stack.set_vexpand(true);
        tab_stack.set_hexpand(true);
        let empty_label = gtk::Label::new(Some("Open a file from the tree"));
        empty_label.add_css_class("kalam-ui-meta");
        tab_stack.add_named(&empty_label, Some(EMPTY_TAB));
        tab_stack.set_visible_child_name(EMPTY_TAB);

        let center_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        center_box.set_hexpand(true);
        center_box.append(&toolbar);
        center_box.append(&tab_scroller);
        center_box.append(&tab_stack);

        // ── right panel (440, §§32–33) ─────────────────────────────
        let mut right_tab_btns: Vec<(RightTab, gtk::Button)> = Vec::new();
        let tab_row = gtk::Box::new(gtk::Orientation::Horizontal, 20);
        tab_row.add_css_class("kalam-ed-tabrow");
        tab_row.set_size_request(-1, 48);
        for (tab, label) in [
            (RightTab::Preview, "Preview"),
            (RightTab::Outline, "Outline"),
            (RightTab::Problems, "Problems"),
            (RightTab::LiveCss, "Live CSS"),
        ] {
            let btn = gtk::Button::with_label(label);
            btn.add_css_class("kalam-ed-flat-tab");
            btn.set_valign(gtk::Align::Fill);
            btn.set_focus_on_click(false);
            let s = sender.clone();
            btn.connect_clicked(move |_| s.input(EpubEditorMsg::SetRightTab(tab)));
            tab_row.append(&btn);
            right_tab_btns.push((tab, btn));
        }

        let preview_label = gtk::Label::new(Some("Nothing to preview"));
        preview_label.add_css_class("kalam-ed-preview");
        preview_label.set_wrap(true);
        preview_label.set_xalign(0.0);
        preview_label.set_yalign(0.0);
        preview_label.set_margin_all(20);
        let preview_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        preview_host.set_hexpand(true);
        preview_host.set_vexpand(true);
        preview_host.append(&preview_label);

        let outline_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let outline_scroller = scroll_host(&outline_host);

        let problems_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let problems_scroller = scroll_host(&problems_host);

        let livecss_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        livecss_host.append(&panel_note(
            "Place the cursor inside an element in an HTML file to see the CSS that applies to it.",
        ));
        let livecss_scroller = scroll_host(&livecss_host);

        let right_stack = gtk::Stack::new();
        right_stack.set_vexpand(true);
        right_stack.add_named(&preview_host, Some("preview"));
        right_stack.add_named(&outline_scroller, Some("outline"));
        right_stack.add_named(&problems_scroller, Some("problems"));
        right_stack.add_named(&livecss_scroller, Some("livecss"));
        right_stack.set_visible_child_name("preview");

        let right_panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
        right_panel.add_css_class("kalam-ed-side");
        right_panel.set_size_request(440, -1);
        right_panel.set_hexpand(false);
        right_panel.append(&tab_row);
        right_panel.append(&right_stack);

        // ── middle + can't-open state, in a stack ──────────────────
        let middle = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        middle.set_vexpand(true);
        middle.append(&tree_scroller);
        middle.append(&center_box);
        middle.append(&right_panel);

        let error_label = gtk::Label::new(None);
        error_label.add_css_class("kalam-ui-meta");
        error_label.set_wrap(true);
        error_label.set_halign(gtk::Align::Start);
        let error_card = gtk::Box::new(gtk::Orientation::Vertical, 12);
        error_card.add_css_class("kalam-ui-state-card");
        error_card.set_halign(gtk::Align::Center);
        error_card.set_valign(gtk::Align::Center);
        error_card.append(&error_label);
        let show_folder = boxed_ghost_button("Show in file manager", {
            let s = sender.clone();
            move || s.input(EpubEditorMsg::ShowInFolder)
        });
        show_folder.set_halign(gtk::Align::Start);
        error_card.append(&show_folder);
        let error_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        error_host.set_valign(gtk::Align::Center);
        error_host.append(&error_card);

        let body_stack = gtk::Stack::new();
        body_stack.set_vexpand(true);
        body_stack.add_named(&middle, Some("workspace"));
        body_stack.add_named(&error_host, Some("state"));
        body_stack.set_visible_child_name("workspace");

        // ── status bar (32, decision 122) ──────────────────────────
        let status_pos = status_label("Ln 1, Col 1");
        let status_type = status_label("");
        let status_problems = status_label("");
        status_problems.add_css_class("kalam-ed-status-problems");
        status_problems.set_visible(false);
        let snapshot_note = status_label("Snapshot kept before each save");
        let status_bar = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        status_bar.add_css_class("kalam-ed-statusbar");
        status_bar.set_size_request(-1, 32);
        status_bar.append(&status_pos);
        status_bar.append(&status_type);
        status_bar.append(&status_problems);
        status_bar.append(&snapshot_note);
        let status_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        status_spacer.set_hexpand(true);
        status_bar.append(&status_spacer);

        let mut view_switch_btns: Vec<(ViewMode, gtk::Button)> = Vec::new();
        let switch = gtk::Box::new(gtk::Orientation::Horizontal, 20);
        for (mode, label) in [
            (ViewMode::Code, "Code"),
            (ViewMode::Split, "Split"),
            (ViewMode::Preview, "Preview"),
        ] {
            let btn = gtk::Button::with_label(label);
            btn.add_css_class("kalam-ed-flat-tab");
            btn.add_css_class("kalam-ed-switch");
            btn.set_focus_on_click(false);
            let s = sender.clone();
            btn.connect_clicked(move |_| s.input(EpubEditorMsg::SetViewMode(mode)));
            switch.append(&btn);
            view_switch_btns.push((mode, btn));
        }
        status_bar.append(&switch);

        // ── assemble ───────────────────────────────────────────────
        let main_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        main_box.add_css_class("kalam-editor");
        main_box.set_vexpand(true);
        main_box.append(&top_bar);
        main_box.append(&body_stack);
        main_box.append(&status_bar);
        root.set_child(Some(&main_box));

        // ── E3 floating surfaces: after-tool pill, Find & replace,
        // command palette. All start hidden.
        let pill_label = gtk::Label::new(None);
        pill_label.add_css_class("kalam-ed-pill-text");
        pill_label.set_hexpand(true);
        pill_label.set_halign(gtk::Align::Start);
        pill_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let pill_undo_btn = gtk::Button::with_label("Undo");
        pill_undo_btn.add_css_class("kalam-ed-pill-undo");
        pill_undo_btn.set_focus_on_click(false);
        {
            let s = sender.input_sender().clone();
            pill_undo_btn.connect_clicked(move |_| {
                let _ = s.send(EpubEditorMsg::PillUndo);
            });
        }
        let pill = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        pill.add_css_class("kalam-ed-pill");
        pill.set_size_request(380, 48);
        pill.set_halign(gtk::Align::Center);
        pill.set_valign(gtk::Align::End);
        pill.set_margin_bottom(44);
        pill.append(&pill_label);
        pill.append(&pill_undo_btn);
        pill.set_visible(false);
        root.add_overlay(&pill);

        // Find & replace (decision 131): 440 card, padding 24.
        let find_inner = gtk::Box::new(gtk::Orientation::Vertical, 10);
        find_inner.set_margin_all(24);
        let find_title = gtk::Label::new(Some("Find & replace"));
        find_title.add_css_class("kalam-ed-find-title");
        find_title.set_halign(gtk::Align::Start);
        let find_close = gtk::Button::new();
        find_close.add_css_class("kalam-ed-ghost");
        find_close.set_child(Some(&crate::icons::symbolic("window-close-symbolic", 14)));
        find_close.set_tooltip_text(Some("Close (Esc)"));
        find_close.set_focus_on_click(false);
        {
            let s = sender.input_sender().clone();
            find_close.connect_clicked(move |_| {
                let _ = s.send(EpubEditorMsg::ClosePanels);
            });
        }
        let find_head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        find_title.set_hexpand(true);
        find_head.append(&find_title);
        find_head.append(&find_close);
        find_inner.append(&find_head);

        let mut find_scope_btns: Vec<(FindScope, gtk::Button)> = Vec::new();
        let scope_row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        for (scope, label) in [
            (FindScope::ThisFile, "This file"),
            (FindScope::AllText, "All text files"),
            (FindScope::WholeBook, "Whole book"),
        ] {
            let btn = gtk::Button::with_label(label);
            btn.add_css_class("kalam-ed-flat-tab");
            btn.add_css_class("kalam-ed-findscope");
            btn.set_focus_on_click(false);
            let s = sender.input_sender().clone();
            btn.connect_clicked(move |_| {
                let _ = s.send(EpubEditorMsg::FindSetScope(scope));
            });
            scope_row.append(&btn);
            find_scope_btns.push((scope, btn));
        }
        find_inner.append(&scope_row);

        let find_entry = gtk::Entry::new();
        find_entry.set_placeholder_text(Some("Find"));
        find_entry.add_css_class("kalam-ed-entry");
        {
            let s = sender.input_sender().clone();
            find_entry.connect_changed(move |_| {
                let _ = s.send(EpubEditorMsg::FindChanged);
            });
            let s = sender.input_sender().clone();
            find_entry.connect_activate(move |_| {
                let _ = s.send(EpubEditorMsg::FindNext);
            });
        }
        find_inner.append(&find_entry);
        let replace_entry = gtk::Entry::new();
        replace_entry.set_placeholder_text(Some("Replace with"));
        replace_entry.add_css_class("kalam-ed-entry");
        find_inner.append(&replace_entry);

        let chip_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let chip_case = gtk::ToggleButton::with_label("Match case");
        let chip_word = gtk::ToggleButton::with_label("Whole word");
        let chip_regex = gtk::ToggleButton::with_label("Regex");
        for chip in [&chip_case, &chip_word, &chip_regex] {
            chip.add_css_class("kalam-ed-chip");
            chip.set_focus_on_click(false);
            let s = sender.input_sender().clone();
            chip.connect_toggled(move |_| {
                let _ = s.send(EpubEditorMsg::FindChanged);
            });
            chip_row.append(chip);
        }
        find_inner.append(&chip_row);

        let find_count = gtk::Label::new(None);
        find_count.add_css_class("kalam-ui-meta");
        find_count.set_halign(gtk::Align::Start);
        find_inner.append(&find_count);

        let find_results_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let find_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::External)
            .max_content_height(240)
            .propagate_natural_height(true)
            .child(&find_results_host)
            .build();
        find_inner.append(&find_scroller);

        let find_foot = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let saved_link = gtk::Button::with_label("Saved searches");
        saved_link.add_css_class("kalam-ed-ghost");
        saved_link.set_focus_on_click(false);
        {
            let s = sender.input_sender().clone();
            saved_link.connect_clicked(move |_| {
                let _ = s.send(EpubEditorMsg::SavedSearchesMenu);
            });
        }
        find_foot.append(&saved_link);
        let foot_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        foot_spacer.set_hexpand(true);
        find_foot.append(&foot_spacer);
        let replace_one = boxed_ghost_button("Replace one", {
            let s = sender.input_sender().clone();
            move || {
                let _ = s.send(EpubEditorMsg::FindReplaceOne);
            }
        });
        find_foot.append(&replace_one);
        let replace_all = primary_button("Replace all", {
            let s = sender.input_sender().clone();
            move || {
                let _ = s.send(EpubEditorMsg::FindReplaceAll);
            }
        });
        find_foot.append(&replace_all);
        find_inner.append(&find_foot);

        let find_panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
        find_panel.add_css_class("kalam-ed-find");
        find_panel.set_size_request(440, -1);
        find_panel.set_halign(gtk::Align::End);
        find_panel.set_valign(gtk::Align::Start);
        find_panel.set_margin_top(64);
        find_panel.set_margin_end(16);
        find_panel.append(&find_inner);
        find_panel.set_visible(false);
        root.add_overlay(&find_panel);

        // Command palette (decision 129, section 38): 640 card.
        let palette_entry = gtk::Entry::new();
        palette_entry.set_placeholder_text(Some("Type a command…"));
        palette_entry.add_css_class("kalam-ed-palette-entry");
        palette_entry.set_size_request(-1, 52);
        {
            let s = sender.input_sender().clone();
            palette_entry.connect_changed(move |_| {
                let _ = s.send(EpubEditorMsg::PaletteChanged);
            });
            let keys = gtk::EventControllerKey::new();
            let s = sender.input_sender().clone();
            keys.connect_key_pressed(move |_, keyval, _, _| {
                let stop = gtk::glib::Propagation::Stop;
                match keyval {
                    gtk::gdk::Key::Down => {
                        let _ = s.send(EpubEditorMsg::PaletteMove(1));
                        stop
                    }
                    gtk::gdk::Key::Up => {
                        let _ = s.send(EpubEditorMsg::PaletteMove(-1));
                        stop
                    }
                    gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter => {
                        let _ = s.send(EpubEditorMsg::PaletteAccept);
                        stop
                    }
                    _ => gtk::glib::Propagation::Proceed,
                }
            });
            palette_entry.add_controller(keys);
        }
        let palette_sep = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        palette_sep.add_css_class("kalam-ed-palette-sep");
        palette_sep.set_size_request(-1, 1);
        let palette_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        palette_host.set_margin_all(6);
        let palette_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::External)
            .max_content_height(400)
            .propagate_natural_height(true)
            .child(&palette_host)
            .build();
        let palette_panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
        palette_panel.add_css_class("kalam-ed-palette");
        palette_panel.set_size_request(640, -1);
        palette_panel.set_halign(gtk::Align::Center);
        palette_panel.set_valign(gtk::Align::Start);
        palette_panel.set_margin_top(72);
        palette_panel.append(&palette_entry);
        palette_panel.append(&palette_sep);
        palette_panel.append(&palette_scroller);
        palette_panel.set_visible(false);
        root.add_overlay(&palette_panel);

        // Ctrl+S saves, from anywhere in the workspace.
        let keys = gtk::EventControllerKey::new();
        {
            let s = sender.clone();
            keys.connect_key_pressed(move |_, keyval, _, state| {
                let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
                let stop = gtk::glib::Propagation::Stop;
                if ctrl && keyval == gtk::gdk::Key::s {
                    s.input(EpubEditorMsg::SavePressed);
                    return stop;
                }
                if ctrl && keyval == gtk::gdk::Key::f {
                    s.input(EpubEditorMsg::ToggleFind);
                    return stop;
                }
                if ctrl
                    && state.contains(gtk::gdk::ModifierType::SHIFT_MASK)
                    && (keyval == gtk::gdk::Key::P || keyval == gtk::gdk::Key::p)
                {
                    s.input(EpubEditorMsg::TogglePalette);
                    return stop;
                }
                if keyval == gtk::gdk::Key::Escape {
                    // Proceed as well, so open popovers still get
                    // their own Escape.
                    s.input(EpubEditorMsg::ClosePanels);
                }
                gtk::glib::Propagation::Proceed
            });
        }
        root.add_controller(keys);

        // Restroke the source scheme when the palette changes.
        {
            let s = sender.input_sender().clone();
            crate::theme::on_theme_applied(Rc::new(move |_theme| {
                let _ = s.send(EpubEditorMsg::ThemeApplied);
            }));
        }

        let mut model = EpubEditorModel {
            service,
            book_id,
            book: None,
            container: None,
            root_overlay: root.clone(),
            body_stack,
            title_label,
            save_btn,
            tree_host,
            tree_scroller,
            center_box,
            tab_strip,
            tab_stack,
            right_panel,
            right_stack,
            right_tab_btns,
            view_switch_btns,
            preview_host,
            livecss_host,
            outline_host,
            problems_host,
            status_pos,
            status_type,
            status_problems,
            status_bar,
            error_label,
            tabs: Vec::new(),
            active_path: None,
            collapsed: HashSet::new(),
            view_mode: ViewMode::Split,
            right_tab: RightTab::Preview,
            problems: Vec::new(),
            shown_dirty: HashSet::new(),
            after_save: AfterSave::Nothing,
            saving: false,
            preview_view: None,
            preview_mode: PreviewMode::Ships,
            preview_menu: None,
            preview_write_src: None,
            preview_chapter: -1,
            check_src: None,
            fix_popover: None,
            outline_headings: Vec::new(),
            css_cache_key: 0,
            css_cache: Vec::new(),
            pending: HashMap::new(),
            tools_btn,
            tools_menu: None,
            tool_undo: None,
            pill,
            pill_label,
            pill_undo_btn,
            pill_src: None,
            find_panel,
            find_entry,
            replace_entry,
            find_scope_btns,
            find_scope: FindScope::ThisFile,
            chip_case,
            chip_word,
            chip_regex,
            find_count,
            find_results_host,
            find_hits: Vec::new(),
            find_current: None,
            saved_menu: None,
            palette_panel,
            palette_entry,
            palette_host,
            palette_items: Vec::new(),
            palette_selected: 0,
        };
        model.sync_switches();
        let widgets = view_output!();

        model.request_load(&sender);
        ComponentParts { model, widgets }
    }

    fn update(
        &mut self,
        message: Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            EpubEditorMsg::Loaded(load) => match *load {
                Ok((book, container)) => {
                self.title_label.set_label(&book.title);
                self.collapsed = self
                    .service
                    .catalog()
                    .get_pref(&format!("editor.collapsed.{}", book.id))
                    .map(|value| {
                        value
                            .split(',')
                            .filter(|s| !s.is_empty())
                            .map(str::to_string)
                            .collect()
                    })
                    .unwrap_or_default();
                self.book = Some(book);
                self.container = Some(container);
                self.body_stack.set_visible_child_name("workspace");
                self.rebuild_tree(&sender);
                // Open the first text file, pinned, so the workspace
                // never greets anyone with an empty editor.
                let first = self
                    .container
                    .as_ref()
                    .and_then(|c| {
                        c.section_files(TreeSection::Text)
                            .first()
                            .map(|f| f.path.clone())
                    });
                if let Some(path) = first {
                    self.open_file(&path, true, &sender);
                }
                self.run_check(&sender);
                self.request_preview_write(&sender);
                self.sync_chrome(&sender);
                // CI staging: photograph each right-panel tab.
                match std::env::var("KALAM_DIALOG").as_deref() {
                    Ok("editor-find") => {
                        self.toggle_find(&sender);
                        self.find_entry.set_text("the");
                    }
                    Ok("editor-palette") => {
                        self.toggle_palette(&sender);
                        self.palette_entry.set_text("ref");
                    }
                    Ok("editor-problems") => {
                        sender.input(EpubEditorMsg::SetRightTab(RightTab::Problems));
                    }
                    Ok("editor-outline") | Ok("editor-livecss") => {
                        if let Some(buffer) = self.active_buffer() {
                            let mid = buffer.iter_at_offset(buffer.char_count() / 2);
                            buffer.place_cursor(&mid);
                        }
                        sender.input(EpubEditorMsg::SetRightTab(
                            if std::env::var("KALAM_DIALOG").as_deref() == Ok("editor-livecss") {
                                RightTab::LiveCss
                            } else {
                                RightTab::Outline
                            },
                        ));
                    }
                    _ => {}
                }
            }
                Err(message) => {
                self.title_label.set_label("Editor");
                self.error_label.set_label(&message);
                self.body_stack.set_visible_child_name("state");
                self.status_bar.set_visible(false);
                self.save_btn.set_visible(false);
                }
            }
            EpubEditorMsg::OpenFile { path, pin } => {
                self.open_file(&path, pin, &sender);
            }
            EpubEditorMsg::CloseTab { path } => {
                if self.is_modified(&path) {
                    let name = file_name(&path).to_string();
                    self.present_unsaved_dialog(
                        &format!("Save changes to {name}?"),
                        UnsavedAction::CloseTab(path),
                        &sender,
                    );
                } else {
                    self.close_tab(&path, &sender);
                }
            }
            EpubEditorMsg::ActivateTab { path } => {
                self.activate_tab(&path, &sender);
            }
            EpubEditorMsg::ToggleSection(section) => {
                let title = section.title().to_string();
                if !self.collapsed.remove(&title) {
                    self.collapsed.insert(title);
                }
                if let Some(book) = &self.book {
                    let value = TreeSection::ALL
                        .iter()
                        .filter(|s| self.collapsed.contains(s.title()))
                        .map(|s| s.title())
                        .collect::<Vec<_>>()
                        .join(",");
                    self.service
                        .catalog()
                        .set_pref(&format!("editor.collapsed.{}", book.id), &value);
                }
                self.rebuild_tree(&sender);
            }
            EpubEditorMsg::BufferChanged => {
                self.pin_temporary_if_edited();
                self.sync_chrome(&sender);
                self.schedule_check(&sender);
                self.schedule_preview_write(&sender);
                if self.right_tab == RightTab::Outline {
                    self.rebuild_outline(&sender);
                }
                if self.right_tab == RightTab::LiveCss {
                    self.rebuild_livecss(&sender);
                }
            }
            EpubEditorMsg::CursorMoved { line, col } => {
                self.status_pos.set_label(&format!("Ln {line}, Col {col}"));
                if self.right_tab == RightTab::Outline {
                    self.mark_outline_current();
                }
                if self.right_tab == RightTab::LiveCss {
                    self.rebuild_livecss(&sender);
                }
                self.sync_preview_to_cursor();
            }
            EpubEditorMsg::Format(tag) => self.format_inline(tag),
            EpubEditorMsg::SavePressed => {
                self.after_save = AfterSave::Nothing;
                self.try_save(false, &sender);
            }
            EpubEditorMsg::SaveAnywayPressed => {
                self.try_save(true, &sender);
            }
            EpubEditorMsg::DialogSaveAndContinue => {
                self.try_save(false, &sender);
            }
            EpubEditorMsg::Saved(Ok(())) => {
                self.saving = false;
                let saved: Vec<(String, Vec<u8>)> = self
                    .tabs
                    .iter()
                    .filter_map(|tab| {
                        let buffer = tab.buffer.as_ref()?;
                        if buffer.is_modified() {
                            Some((tab.path.clone(), buffer_text(buffer).into_bytes()))
                        } else {
                            None
                        }
                    })
                    .collect();
                for tab in &self.tabs {
                    if let Some(buffer) = &tab.buffer {
                        buffer.set_modified(false);
                    }
                }
                let pending_saved: Vec<(String, String)> =
                    self.pending.drain().collect();
                if let Some(container) = &mut self.container {
                    for (path, bytes) in saved {
                        container.set_bytes(&path, bytes);
                    }
                    for (path, text) in pending_saved {
                        container.set_bytes(&path, text.into_bytes());
                    }
                }
                match std::mem::replace(&mut self.after_save, AfterSave::Nothing) {
                    AfterSave::Leave => {
                        let _ = sender.output(EpubEditorOut::Leave);
                    }
                    AfterSave::CloseTab(path) => self.close_tab(&path, &sender),
                    AfterSave::Nothing => {}
                }
                self.run_check(&sender);
                self.request_preview_write(&sender);
                self.sync_chrome(&sender);
            }
            EpubEditorMsg::Saved(Err(message)) => {
                self.saving = false;
                self.after_save = AfterSave::Nothing;
                crate::notify::error("Could not save the book", &message);
                self.sync_chrome(&sender);
            }
            EpubEditorMsg::BackPressed => {
                let dirty = self.dirty_paths();
                if dirty.is_empty() {
                    let _ = sender.output(EpubEditorOut::Leave);
                } else {
                    let n = dirty.len();
                    self.present_unsaved_dialog(
                        &format!(
                            "Save changes to {n} file{}?",
                            if n == 1 { "" } else { "s" }
                        ),
                        UnsavedAction::Leave,
                        &sender,
                    );
                }
            }
            EpubEditorMsg::DiscardAndLeave => {
                let _ = sender.output(EpubEditorOut::Leave);
            }
            EpubEditorMsg::DiscardCloseTab { path } => {
                self.after_save = AfterSave::Nothing;
                self.close_tab(&path, &sender);
            }
            EpubEditorMsg::SetViewMode(mode) => {
                self.view_mode = mode;
                self.apply_view_mode();
                self.sync_switches();
            }
            EpubEditorMsg::SetRightTab(tab) => {
                // Pressing Preview while it is active opens its
                // menu instead of doing nothing (decision 123).
                if tab == RightTab::Preview && self.right_tab == RightTab::Preview {
                    self.toggle_preview_menu(&sender);
                    return;
                }
                self.right_tab = tab;
                self.right_stack.set_visible_child_name(match tab {
                    RightTab::Preview => "preview",
                    RightTab::Outline => "outline",
                    RightTab::Problems => "problems",
                    RightTab::LiveCss => "livecss",
                });
                if self.view_mode == ViewMode::Code {
                    self.view_mode = ViewMode::Split;
                    self.apply_view_mode();
                }
                self.sync_switches();
                if tab == RightTab::Outline {
                    self.rebuild_outline(&sender);
                }
                if tab == RightTab::Problems {
                    self.run_check(&sender);
                    self.rebuild_problems(&sender);
                }
                if tab == RightTab::LiveCss {
                    self.rebuild_livecss(&sender);
                }
            }
            EpubEditorMsg::GoTo { path, line, offset } => {
                self.open_file(&path, true, &sender);
                if let Some(tab) = self.tabs.iter().find(|t| t.path == path) {
                    if let Some(buffer) = &tab.buffer {
                        let mut iter = match (offset, line) {
                            (Some(offset), _) => buffer.iter_at_offset(offset),
                            (None, Some(line)) => buffer
                                .iter_at_line((line.saturating_sub(1)) as i32)
                                .unwrap_or_else(|| buffer.end_iter()),
                            (None, None) => return,
                        };
                        buffer.place_cursor(&iter);
                        if let Some(view) = self.active_view() {
                            view.scroll_to_iter(&mut iter, 0.1, false, 0.0, 0.0);
                            view.grab_focus();
                        }
                    }
                }
            }
            EpubEditorMsg::ThemeApplied => {
                let service = self.service.clone();
                let s = sender.input_sender().clone();
                crate::tasks::spawn_internal(
                    "Refreshing editor colours",
                    move |_reporter| service.write_editor_style_scheme(),
                    |_| {},
                    move |()| {
                        let _ = s.send(EpubEditorMsg::SchemeReady);
                    },
                );
            }
            EpubEditorMsg::SchemeReady => {
                let scheme = current_style_scheme();
                for tab in &self.tabs {
                    if let Some(buffer) = &tab.buffer {
                        buffer.set_style_scheme(scheme.as_ref());
                    }
                }
            }
            EpubEditorMsg::ShowInFolder => {
                if let Ok(Some(book)) = self.service.catalog().get_book(self.book_id) {
                    crate::pages::book::open_in_file_manager(&book.file_path.clone());
                } else if let Some(book) = &self.book {
                    crate::pages::book::open_in_file_manager(&book.file_path.clone());
                }
            }
            EpubEditorMsg::SetPreviewMode(mode) => {
                self.preview_mode = mode;
                self.apply_preview_prefs();
            }
            EpubEditorMsg::PreviewWriteDue => {
                self.preview_write_src = None;
                self.request_preview_write(&sender);
            }
            EpubEditorMsg::PreviewWritten(path) => self.refresh_preview(&path, &sender),
            EpubEditorMsg::CheckDue => {
                self.check_src = None;
                self.run_check(&sender);
            }
            EpubEditorMsg::FixClicked(index) => self.present_fix_preview(index, &sender),
            EpubEditorMsg::FixApply(fix, path) => self.apply_fix(&fix, &path, &sender),
            EpubEditorMsg::FixSkip => {
                if let Some(popover) = self.fix_popover.take() {
                    popover.popdown();
                }
            }
            EpubEditorMsg::RemoveProblem(index) => self.remove_problem_file(index, &sender),
            EpubEditorMsg::FileRemoved(path) => {
                self.close_tab_quietly(&path);
                // Reload the container through the service (the
                // book changed on disk); Loaded rebuilds the tree
                // and leaves the other open buffers alone.
                self.request_load(&sender);
                self.request_preview_write(&sender);
                self.sync_chrome(&sender);
            }
            EpubEditorMsg::ActionFailed(message) => {
                crate::notify::error("That didn't work", &message);
            }
            EpubEditorMsg::PreviewGoToWord { chapter, fraction, word } => {
                self.preview_word_jump(chapter, fraction, &word, &sender);
            }
            EpubEditorMsg::PreviewTextSelected => {
                self.preview_selection_jump(&sender);
            }
            EpubEditorMsg::GoToCssRule { path, line } => {
                sender.input(EpubEditorMsg::GoTo {
                    path,
                    line: Some(line),
                    offset: None,
                });
            }
            EpubEditorMsg::OpenToolsMenu => self.toggle_tools_menu(&sender),
            EpubEditorMsg::ToolChosen(kind) => self.present_scope_dialog(kind, &sender),
            EpubEditorMsg::RunInstantTool(kind, whole_book) => {
                match self.compute_tool_changes(kind, whole_book) {
                    Some((summary, changes)) => {
                        self.start_apply(kind.snapshot_label(), summary, changes, &sender)
                    }
                    None => self.show_pill("Nothing to change", false, &sender),
                }
            }
            EpubEditorMsg::ApplyChanges { summary, changes } => {
                self.apply_changes(&summary, changes, &sender)
            }
            EpubEditorMsg::PillUndo => self.undo_tool(&sender),
            EpubEditorMsg::PillHide => {
                self.pill_src = None;
                self.pill.set_visible(false);
            }
            EpubEditorMsg::ToggleFind => self.toggle_find(&sender),
            EpubEditorMsg::FindChanged => self.run_find(&sender),
            EpubEditorMsg::FindSetScope(scope) => {
                self.find_scope = scope;
                self.sync_find_scope_btns();
                self.run_find(&sender);
            }
            EpubEditorMsg::FindJump(index) => self.jump_to_hit(index, &sender),
            EpubEditorMsg::FindNext => {
                if !self.find_hits.is_empty() {
                    let next = self
                        .find_current
                        .map(|c| (c + 1) % self.find_hits.len())
                        .unwrap_or(0);
                    self.jump_to_hit(next, &sender);
                }
            }
            EpubEditorMsg::FindReplaceOne => self.replace_one(&sender),
            EpubEditorMsg::FindReplaceAll => self.replace_all(&sender),
            EpubEditorMsg::SavedSearchesMenu => self.toggle_saved_menu(&sender),
            EpubEditorMsg::SaveThisSearch => {
                let pair = (
                    self.find_entry.text().to_string(),
                    self.replace_entry.text().to_string(),
                );
                if !pair.0.is_empty() {
                    let mut saved = self.saved_searches();
                    saved.retain(|p| *p != pair);
                    saved.insert(0, pair);
                    saved.truncate(12);
                    self.store_saved_searches(&saved);
                }
            }
            EpubEditorMsg::UseSavedSearch(find, replace) => {
                self.find_entry.set_text(&find);
                self.replace_entry.set_text(&replace);
                self.run_find(&sender);
            }
            EpubEditorMsg::TogglePalette => self.toggle_palette(&sender),
            EpubEditorMsg::PaletteChanged => self.rebuild_palette(&sender),
            EpubEditorMsg::PaletteMove(delta) => {
                let len = self.palette_items.len();
                if len > 0 {
                    self.palette_selected =
                        (self.palette_selected as i32 + delta).rem_euclid(len as i32) as usize;
                    self.build_palette_rows(&sender);
                }
            }
            EpubEditorMsg::PaletteAccept => {
                if let Some(item) = self.palette_items.get(self.palette_selected).cloned() {
                    self.palette_panel.set_visible(false);
                    self.run_palette_action(item.action, &sender);
                }
            }
            EpubEditorMsg::PaletteRun(index) => {
                self.palette_selected = index;
                sender.input(EpubEditorMsg::PaletteAccept);
            }
            EpubEditorMsg::ClosePanels => {
                self.find_panel.set_visible(false);
                self.palette_panel.set_visible(false);
            }
        }
    }
    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        if let Some(src) = self.check_src.take() {
            src.remove();
        }
        if let Some(src) = self.preview_write_src.take() {
            src.remove();
        }
        if let Some(src) = self.pill_src.take() {
            src.remove();
        }
        if let Some(view) = self.preview_view.take() {
            view.close();
        }
    }
}

/// Which pending action an unsaved-changes dialog is deciding.
#[derive(Clone)]
enum UnsavedAction {
    Leave,
    CloseTab(String),
}

impl InstantTool {
    fn label(self) -> &'static str {
        match self {
            InstantTool::Smarten => "Smarten punctuation",
            InstantTool::Reformat => "Reformat code",
            InstantTool::RemoveUnusedCss => "Remove unused CSS",
            InstantTool::FixHtml => "Fix HTML",
        }
    }

    fn snapshot_label(self) -> String {
        format!("Before {}", self.label())
    }

    fn applies_to(self, kind: FileKind) -> bool {
        match self {
            InstantTool::Smarten | InstantTool::FixHtml => kind == FileKind::Html,
            InstantTool::Reformat => matches!(kind, FileKind::Html | FileKind::Css | FileKind::Xml),
            InstantTool::RemoveUnusedCss => kind == FileKind::Css,
        }
    }

    fn whole_meta(self) -> &'static str {
        match self {
            InstantTool::Smarten | InstantTool::FixHtml => "Every HTML file",
            InstantTool::Reformat => "Every text file",
            InstantTool::RemoveUnusedCss => "Every stylesheet",
        }
    }
}

impl EpubEditorModel {
    fn request_load(&mut self, sender: &ComponentSender<Self>) {
        let service = self.service.clone();
        let book_id = self.book_id;
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Opening book in the editor",
            move |_reporter| service.open_for_editing(book_id),
            |_| {},
            move |result| {
                let _ = s.send(EpubEditorMsg::Loaded(Box::new(result)));
            },
        );
    }

    // ── tabs ───────────────────────────────────────────────────────

    fn open_file(&mut self, path: &str, pin: bool, sender: &ComponentSender<Self>) {
        if self.tabs.iter().any(|t| t.path == path) {
            if pin {
                if let Some(tab) = self.tabs.iter_mut().find(|t| t.path == path) {
                    tab.pinned = true;
                }
            }
            self.activate_tab(path, sender);
            return;
        }
        let Some(container) = &self.container else {
            return;
        };
        let Some(file) = container.file(path).cloned() else {
            return;
        };

        // A fresh temporary tab replaces the standing one (decision 126).
        if !pin {
            if let Some(temp) = self
                .tabs
                .iter()
                .find(|t| !t.pinned)
                .map(|t| t.path.clone())
            {
                // Never silently throw away typed changes: an edited
                // temporary tab was pinned by its first change.
                self.close_tab_quietly(&temp);
            }
        }

        let (widget, buffer) = self.build_tab_widget(&file, sender);
        if buffer.is_some() {
            self.pending.remove(path);
        }
        self.tab_stack.add_named(&widget, Some(path));
        self.tabs.push(OpenTab {
            path: path.to_string(),
            kind: file.kind,
            pinned: pin,
            buffer,
        });
        self.activate_tab(path, sender);
    }

    fn build_tab_widget(
        &self,
        file: &EpubFile,
        sender: &ComponentSender<Self>,
    ) -> (gtk::Widget, Option<sourceview5::Buffer>) {
        let Some(container) = &self.container else {
            // Only reachable if a click lands mid-load; an empty tab
            // beats a crash the guardrails exist to prevent.
            return (gtk::Label::new(None).upcast(), None);
        };
        match file.kind {
            FileKind::Html | FileKind::Css | FileKind::Xml => {
                let text = self
                    .pending
                    .get(&file.path)
                    .cloned()
                    .or_else(|| container.text(&file.path))
                    .unwrap_or_default();
                let buffer = sourceview5::Buffer::new(None::<&gtk::TextTagTable>);
                if let Some(id) = file.kind.language_id() {
                    if let Some(language) = sourceview5::LanguageManager::default().language(id)
                    {
                        buffer.set_language(Some(&language));
                    }
                }
                buffer.set_style_scheme(current_style_scheme().as_ref());
                buffer.set_highlight_syntax(true);
                // Loading a file is not an edit: keep it out of the
                // undo history so Ctrl+Z can never empty a fresh tab.
                let undo_levels = buffer.max_undo_levels();
                buffer.set_max_undo_levels(0);
                buffer.set_text(&text);
                buffer.set_max_undo_levels(undo_levels);
                // A pending tool edit is a real unsaved change the
                // moment its file opens (E3).
                buffer.set_modified(self.pending.contains_key(&file.path));
                buffer.place_cursor(&buffer.start_iter());

                let view = sourceview5::View::with_buffer(&buffer);
                view.add_css_class("kalam-ed-source");
                view.set_show_line_numbers(true);
                view.set_highlight_current_line(true);
                view.set_tab_width(2);
                view.set_insert_spaces_instead_of_tabs(true);
                view.set_wrap_mode(gtk::WrapMode::None);
                view.set_hexpand(true);
                view.set_vexpand(true);

                {
                    let s = sender.input_sender().clone();
                    buffer.connect_changed(move |_| {
                        let _ = s.send(EpubEditorMsg::BufferChanged);
                    });
                }
                {
                    let s = sender.input_sender().clone();
                    buffer.connect_modified_changed(move |_| {
                        let _ = s.send(EpubEditorMsg::BufferChanged);
                    });
                }
                {
                    let s = sender.input_sender().clone();
                    buffer.connect_mark_set(move |buf, _iter, mark| {
                        if mark.name().as_deref() == Some("insert") {
                            let iter = buf.iter_at_offset(buf.cursor_position());
                            let _ = s.send(EpubEditorMsg::CursorMoved {
                                line: iter.line() + 1,
                                col: iter.line_offset() + 1,
                            });
                        }
                    });
                }

                let scroller = gtk::ScrolledWindow::builder()
                    .hscrollbar_policy(gtk::PolicyType::External)
                    .vscrollbar_policy(gtk::PolicyType::External)
                    .vexpand(true)
                    .hexpand(true)
                    .child(&view)
                    .build();
                (scroller.upcast(), Some(buffer))
            }
            FileKind::Image => {
                let host = gtk::Box::new(gtk::Orientation::Vertical, 12);
                host.set_margin_all(20);
                if let Some(bytes) = container.bytes(&file.path) {
                    let glib_bytes = gtk::glib::Bytes::from(bytes);
                    if let Ok(texture) = gtk::gdk::Texture::from_bytes(&glib_bytes) {
                        let picture = gtk::Picture::for_paintable(&texture);
                        picture.set_content_fit(gtk::ContentFit::Contain);
                        picture.set_size_request(-1, 420);
                        host.append(&picture);
                    }
                }
                host.append(&meta_label(&file_summary(file, container.bytes(&file.path))));
                (scroll_host(&host).upcast(), None)
            }
            FileKind::Font | FileKind::Binary => {
                let host = gtk::Box::new(gtk::Orientation::Vertical, 12);
                host.set_margin_all(20);
                host.append(&meta_label(&file_summary(file, container.bytes(&file.path))));
                (scroll_host(&host).upcast(), None)
            }
        }
    }

    fn activate_tab(&mut self, path: &str, sender: &ComponentSender<Self>) {
        self.active_path = Some(path.to_string());
        self.tab_stack.set_visible_child_name(path);
        if let Some(tab) = self.tabs.iter().find(|t| t.path == path) {
            self.status_type.set_label(tab.kind.label());
            if let Some(buffer) = &tab.buffer {
                let iter = buffer.iter_at_offset(buffer.cursor_position());
                self.status_pos
                    .set_label(&format!("Ln {}, Col {}", iter.line() + 1, iter.line_offset() + 1));
            }
        }
        self.rebuild_tab_strip(sender);
        self.rebuild_tree(sender);
        self.sync_preview_to_cursor();
        self.rebuild_outline(sender);
    }

    fn close_tab_quietly(&mut self, path: &str) {
        if let Some(child) = self.tab_stack.child_by_name(path) {
            self.tab_stack.remove(&child);
        }
        self.tabs.retain(|t| t.path != path);
        if self.active_path.as_deref() == Some(path) {
            self.active_path = None;
        }
    }

    fn close_tab(&mut self, path: &str, sender: &ComponentSender<Self>) {
        let was_active = self.active_path.as_deref() == Some(path);
        self.close_tab_quietly(path);
        if was_active {
            match self.tabs.last().map(|t| t.path.clone()) {
                Some(next) => self.activate_tab(&next, sender),
                None => {
                    self.tab_stack.set_visible_child_name(EMPTY_TAB);
                    self.status_type.set_label("");
                }
            }
        }
        self.rebuild_tab_strip(sender);
        self.rebuild_tree(sender);
        self.sync_chrome(sender);
        self.sync_preview_to_cursor();
        self.rebuild_outline(sender);
    }

    fn pin_temporary_if_edited(&mut self) {
        for tab in &mut self.tabs {
            if !tab.pinned {
                if let Some(buffer) = &tab.buffer {
                    if buffer.is_modified() {
                        tab.pinned = true;
                    }
                }
            }
        }
    }

    fn active_tab(&self) -> Option<&OpenTab> {
        self.active_path
            .as_ref()
            .and_then(|path| self.tabs.iter().find(|t| &t.path == path))
    }

    fn active_buffer(&self) -> Option<sourceview5::Buffer> {
        self.active_tab().and_then(|t| t.buffer.clone())
    }

    fn active_view(&self) -> Option<sourceview5::View> {
        let path = self.active_path.as_ref()?;
        let scroller = self.tab_stack.child_by_name(path)?;
        let scroller = scroller.downcast::<gtk::ScrolledWindow>().ok()?;
        scroller.child()?.downcast::<sourceview5::View>().ok()
    }

    fn is_modified(&self, path: &str) -> bool {
        self.pending.contains_key(path)
            || self
                .tabs
                .iter()
                .find(|t| t.path == path)
                .and_then(|t| t.buffer.as_ref())
                .map(|b| b.is_modified())
                .unwrap_or(false)
    }

    fn dirty_paths(&self) -> HashSet<String> {
        let mut dirty: HashSet<String> = self.pending.keys().cloned().collect();
        dirty.extend(
            self.tabs
                .iter()
                .filter(|t| t.buffer.as_ref().map(|b| b.is_modified()).unwrap_or(false))
                .map(|t| t.path.clone()),
        );
        dirty
    }

    // ── formatting toolbar ─────────────────────────────────────────

    /// Bold/Italic write real `strong`/`em` around the selection, and
    /// pressing again takes them off (decision 127). One user action,
    /// so one undo step.
    fn format_inline(&mut self, tag: InlineTag) {
        let Some(buffer) = self.active_buffer() else {
            return;
        };
        let (name, open, close) = match tag {
            InlineTag::Bold => ("strong", "<strong>", "</strong>"),
            InlineTag::Italic => ("em", "<em>", "</em>"),
        };
        let _ = name;
        buffer.begin_user_action();
        if let Some((mut start, mut end)) = buffer.selection_bounds() {
            let selected = buffer.text(&start, &end, false).to_string();
            // Selection includes the tags: unwrap.
            if selected.starts_with(open) && selected.ends_with(close) && selected.len() >= open.len() + close.len() {
                let inner = selected[open.len()..selected.len() - close.len()].to_string();
                buffer.delete(&mut start, &mut end);
                let mut at = start;
                buffer.insert(&mut at, &inner);
                let sel_start = buffer.iter_at_offset(start.offset());
                buffer.select_range(&sel_start, &at);
            } else if surrounded_by(&buffer, &start, &end, open, close) {
                // Tags sit immediately outside the selection: remove them,
                // end tag first so the start offset stays valid.
                let mut close_start = end;
                let mut close_end = end;
                close_end.forward_chars(close.chars().count() as i32);
                buffer.delete(&mut close_start, &mut close_end);
                let mut open_end = start;
                let mut open_start = start;
                open_start.backward_chars(open.chars().count() as i32);
                buffer.delete(&mut open_start, &mut open_end);
                let new_start = buffer.iter_at_offset(open_start.offset());
                let new_end = buffer.iter_at_offset(end.offset() - open.chars().count() as i32);
                buffer.select_range(&new_start, &new_end);
            } else {
                buffer.insert(&mut end, close);
                let mut at = start;
                buffer.insert(&mut at, open);
                let new_start = buffer.iter_at_offset(at.offset());
                let new_end =
                    buffer.iter_at_offset(end.offset() + open.chars().count() as i32);
                buffer.select_range(&new_start, &new_end);
            }
        } else {
            // No selection: drop an empty pair, cursor between the tags.
            let mut at = buffer.iter_at_offset(buffer.cursor_position());
            buffer.insert(&mut at, &format!("{open}{close}"));
            let inside = buffer.iter_at_offset(at.offset() - close.chars().count() as i32);
            buffer.place_cursor(&inside);
        }
        buffer.end_user_action();
        if let Some(view) = self.active_view() {
            view.grab_focus();
        }
    }

    // ── save ───────────────────────────────────────────────────────

    fn try_save(&mut self, save_anyway: bool, sender: &ComponentSender<Self>) {
        if self.saving {
            return;
        }
        let Some(book) = self.book.clone() else {
            return;
        };
        let mut replacements: HashMap<String, Vec<u8>> = HashMap::new();
        for tab in &self.tabs {
            if let Some(buffer) = &tab.buffer {
                if buffer.is_modified() {
                    replacements.insert(tab.path.clone(), buffer_text(buffer).into_bytes());
                }
            }
        }
        for (path, text) in &self.pending {
            replacements.insert(path.clone(), text.clone().into_bytes());
        }
        if replacements.is_empty() {
            // A Save from the unsaved dialog with everything already
            // saved (possible after Save anyway): just continue.
            match std::mem::replace(&mut self.after_save, AfterSave::Nothing) {
                AfterSave::Leave => {
                    let _ = sender.output(EpubEditorOut::Leave);
                }
                AfterSave::CloseTab(path) => self.close_tab(&path, sender),
                AfterSave::Nothing => {}
            }
            return;
        }

        if !save_anyway {
            // Errors in the files being saved ask first; warnings
            // never block (decision 143). Computed synchronously
            // from the live buffers, so a stale Check book run
            // can't wave a broken save through.
            let blockers = self.save_blockers();
            if !blockers.is_empty() {
                self.problems = self.collect_check();
                self.update_problem_status();
                self.rebuild_problems(sender);
                self.present_save_with_errors_dialog(&blockers, sender);
                return;
            }
        }

        self.saving = true;
        self.save_btn.set_sensitive(false);
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Saving edits",
            move |_reporter| service.save_editor_changes(&book, &replacements, "Before save"),
            |_| {},
            move |result| {
                let _ = s.send(EpubEditorMsg::Saved(result));
            },
        );
    }

    /// Errors in the files actually being saved: the only thing
    /// Save stops for (decision 143). Fresh from the live buffers.
    fn save_blockers(&self) -> Vec<BookProblem> {
        self.collect_check()
            .into_iter()
            .filter(|p| p.severity == Severity::Error && self.is_modified(&p.path))
            .collect()
    }

    fn update_problem_status(&self) {
        let n = self.problems.len();
        self.status_problems.set_visible(n > 0);
        if n > 0 {
            self.status_problems.set_label(&format!(
                "{n} problem{}",
                if n == 1 { "" } else { "s" }
            ));
        }
    }

    fn present_save_with_errors_dialog(
        &self,
        blockers: &[BookProblem],
        sender: &ComponentSender<Self>,
    ) {
        let n = blockers.len();
        let first = blockers
            .first()
            .map(|p| format!("{} line {}: {}", file_name(&p.path), p.line, p.message))
            .unwrap_or_default();
        let shell = dialog_shell(DialogSize::Form, None);
        let message = gtk::Label::new(Some(&first));
        message.add_css_class("kalam-ui-dialog-message");
        message.set_halign(gtk::Align::Start);
        message.set_xalign(0.0);
        message.set_wrap(true);
        shell.body().append(&message);

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let slot_anyway = slot.clone();
        let s = sender.input_sender().clone();
        shell
            .footer()
            .append(&boxed_ghost_button("Save anyway", move || {
                close_slot(&slot_anyway);
                let _ = s.send(EpubEditorMsg::SaveAnywayPressed);
            }));
        let slot_problems = slot.clone();
        let s = sender.input_sender().clone();
        shell.footer().append(&primary_button("Show problems", move || {
            close_slot(&slot_problems);
            let _ = s.send(EpubEditorMsg::SetRightTab(RightTab::Problems));
        }));

        let title = format!("{} error{} found", n, if n == 1 { "" } else { "s" });
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            &title,
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    fn present_unsaved_dialog(
        &mut self,
        title: &str,
        action: UnsavedAction,
        sender: &ComponentSender<Self>,
    ) {
        let shell = dialog_shell(DialogSize::Form, None);
        let message = gtk::Label::new(Some(
            "The changes live in the editor until you save. Save them, discard them, or keep editing.",
        ));
        message.add_css_class("kalam-ui-dialog-message");
        message.set_halign(gtk::Align::Start);
        message.set_xalign(0.0);
        message.set_wrap(true);
        shell.body().append(&message);

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let after = match &action {
            UnsavedAction::Leave => AfterSave::Leave,
            UnsavedAction::CloseTab(path) => AfterSave::CloseTab(path.clone()),
        };
        self.after_save = after;

        let slot_discard = slot.clone();
        let s = sender.input_sender().clone();
        let discard_action = action.clone();
        shell
            .footer()
            .append(&boxed_ghost_button("Discard", move || {
                close_slot(&slot_discard);
                match &discard_action {
                    UnsavedAction::Leave => {
                        let _ = s.send(EpubEditorMsg::DiscardAndLeave);
                    }
                    UnsavedAction::CloseTab(path) => {
                        let _ = s.send(EpubEditorMsg::DiscardCloseTab {
                            path: path.clone(),
                        });
                    }
                }
            }));
        let slot_keep = slot.clone();
        shell
            .footer()
            .append(&boxed_ghost_button("Keep editing", move || {
                close_slot(&slot_keep);
            }));
        let slot_save = slot.clone();
        let s = sender.input_sender().clone();
        shell.footer().append(&primary_button("Save", move || {
            close_slot(&slot_save);
            let _ = s.send(EpubEditorMsg::DialogSaveAndContinue);
        }));

        if let Some(dialog) =
            present_dialog(&self.root_overlay, title, DialogExit::OwnButtons, &shell)
        {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    // ── tree / tabs / panels ───────────────────────────────────────

    fn rebuild_tree(&self, sender: &ComponentSender<Self>) {
        clear_box(&self.tree_host);
        let Some(container) = &self.container else {
            return;
        };
        let dirty = self.dirty_paths();
        for section in TreeSection::ALL {
            let files = container.section_files(section);
            if files.is_empty() {
                continue;
            }
            let collapsed = self.collapsed.contains(section.title());

            let header = gtk::Button::new();
            header.add_css_class("kalam-ed-tree-section");
            header.set_size_request(-1, 36);
            let header_content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            header_content.append(&crate::icons::symbolic(
                if collapsed {
                    "pan-end-symbolic"
                } else {
                    "pan-down-symbolic"
                },
                12,
            ));
            let title = gtk::Label::new(Some(section.title()));
            title.add_css_class("kalam-ed-tree-section-title");
            header_content.append(&title);
            if collapsed {
                let count = gtk::Label::new(Some(&files.len().to_string()));
                count.add_css_class("kalam-ui-meta");
                header_content.append(&count);
            }
            header.set_child(Some(&header_content));
            let s = sender.clone();
            header.connect_clicked(move |_| s.input(EpubEditorMsg::ToggleSection(section)));
            self.tree_host.append(&header);

            if collapsed {
                continue;
            }
            for file in files {
                self.tree_host
                    .append(&self.tree_row(file, dirty.contains(&file.path), sender));
            }
        }
    }

    fn tree_row(
        &self,
        file: &EpubFile,
        changed: bool,
        sender: &ComponentSender<Self>,
    ) -> gtk::Button {
        let row = gtk::Button::new();
        row.add_css_class("kalam-ed-tree-row");
        if self.active_path.as_deref() == Some(file.path.as_str()) {
            row.add_css_class("open");
        }
        row.set_size_request(-1, 36);
        row.set_tooltip_text(Some(&file.path));

        let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let name = gtk::Label::new(Some(file_name(&file.path)));
        name.set_halign(gtk::Align::Start);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        content.append(&name);
        if changed {
            let dot = gtk::Label::new(Some("•"));
            dot.add_css_class("kalam-ed-dot");
            content.append(&dot);
        }
        row.set_child(Some(&content));

        // Clicks: single opens a temporary tab; a second press on the
        // same file pins it — the observable half of decision 126's
        // single/double-click rule through a plain Button.
        let path = file.path.clone();
        let already_open = self.tabs.iter().any(|t| t.path == path);
        let s = sender.clone();
        row.connect_clicked(move |_| {
            s.input(EpubEditorMsg::OpenFile {
                path: path.clone(),
                pin: already_open,
            });
        });
        row
    }

    fn rebuild_tab_strip(&self, sender: &ComponentSender<Self>) {
        clear_box(&self.tab_strip);
        for tab in &self.tabs {
            let btn = gtk::Button::new();
            btn.add_css_class("kalam-ed-filetab");
            if self.active_path.as_deref() == Some(tab.path.as_str()) {
                btn.add_css_class("active");
            }
            btn.set_tooltip_text(Some(&tab.path));
            btn.set_focus_on_click(false);

            let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let name = gtk::Label::new(Some(file_name(&tab.path)));
            if !tab.pinned {
                name.add_css_class("kalam-ed-tab-temporary");
            }
            content.append(&name);
            if tab
                .buffer
                .as_ref()
                .map(|b| b.is_modified())
                .unwrap_or(false)
            {
                let dot = gtk::Label::new(Some("•"));
                dot.add_css_class("kalam-ed-dot");
                content.append(&dot);
            }
            let close = gtk::Label::new(Some("×"));
            close.add_css_class("kalam-ed-tab-close");
            content.append(&close);
            btn.set_child(Some(&content));

            // The whole tab is one button; a click on its right edge
            // (where × sits) closes instead of activating.
            let path = tab.path.clone();
            let s = sender.input_sender().clone();
            let gesture = gtk::GestureClick::new();
            gesture.set_button(1);
            gesture.connect_pressed(move |gesture, _n, x, _y| {
                let width = gesture
                    .widget()
                    .map(|w| w.width())
                    .unwrap_or(0) as f64;
                if width > 0.0 && x > width - 26.0 {
                    let _ = s.send(EpubEditorMsg::CloseTab { path: path.clone() });
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                }
            });
            btn.add_controller(gesture);

            let path = tab.path.clone();
            let s = sender.clone();
            btn.connect_clicked(move |_| {
                s.input(EpubEditorMsg::ActivateTab { path: path.clone() });
            });
            self.tab_strip.append(&btn);
        }
    }

    /// Problems grouped Errors / Warnings (decision 132): 56px
    /// rows, a filled dot for errors and a hollow ring for
    /// warnings, and 28px ghost actions — Fix where one is known,
    /// Go to always, Remove for files nothing uses.
    fn rebuild_problems(&self, sender: &ComponentSender<Self>) {
        clear_box(&self.problems_host);
        if self.problems.is_empty() {
            let label = gtk::Label::new(Some("No problems found"));
            label.add_css_class("kalam-ui-meta");
            label.set_halign(gtk::Align::Start);
            label.set_margin_all(20);
            self.problems_host.append(&label);
            return;
        }

        let mut group: Option<Severity> = None;
        for (index, problem) in self.problems.iter().enumerate() {
            if group != Some(problem.severity) {
                group = Some(problem.severity);
                let heading = gtk::Label::new(Some(match problem.severity {
                    Severity::Error => "Errors",
                    Severity::Warning => "Warnings",
                }));
                heading.add_css_class("kalam-editorpicker-group");
                heading.set_halign(gtk::Align::Start);
                heading.set_margin_start(20);
                heading.set_margin_top(16);
                heading.set_margin_bottom(4);
                self.problems_host.append(&heading);
            }

            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            row.add_css_class("kalam-ed-problem-row");
            row.set_size_request(-1, 56);
            row.set_margin_start(20);
            row.set_margin_end(20);

            let marker = gtk::Label::new(Some(match problem.severity {
                Severity::Error => "●",
                Severity::Warning => "○",
            }));
            marker.add_css_class(match problem.severity {
                Severity::Error => "kalam-ed-problem-error",
                Severity::Warning => "kalam-ed-problem-warning",
            });
            marker.set_valign(gtk::Align::Center);
            row.append(&marker);

            let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
            texts.set_valign(gtk::Align::Center);
            texts.set_hexpand(true);
            let what = gtk::Label::new(Some(&problem.message));
            what.add_css_class("kalam-ed-problem-title");
            what.set_halign(gtk::Align::Start);
            what.set_ellipsize(gtk::pango::EllipsizeMode::End);
            texts.append(&what);
            let where_text = if problem.line > 0 {
                format!("{} · line {}", file_name(&problem.path), problem.line)
            } else {
                file_name(&problem.path).to_string()
            };
            let whereto = gtk::Label::new(Some(&where_text));
            whereto.add_css_class("kalam-ui-meta");
            whereto.set_halign(gtk::Align::Start);
            whereto.set_ellipsize(gtk::pango::EllipsizeMode::End);
            texts.append(&whereto);
            row.append(&texts);

            if problem.fix.is_some() {
                let s = sender.input_sender().clone();
                let fix = boxed_ghost_button("Fix", move || {
                    let _ = s.send(EpubEditorMsg::FixClicked(index));
                });
                fix.add_css_class("kalam-ed-problem-btn");
                fix.set_valign(gtk::Align::Center);
                fix.set_focus_on_click(false);
                row.append(&fix);
            }

            let path = problem.path.clone();
            let line = problem.line;
            let s = sender.input_sender().clone();
            let go = boxed_ghost_button("Go to", move || {
                let _ = s.send(EpubEditorMsg::GoTo {
                    path: path.clone(),
                    line: Some(line.max(1)),
                    offset: None,
                });
            });
            go.add_css_class("kalam-ed-problem-btn");
            go.set_valign(gtk::Align::Center);
            go.set_focus_on_click(false);
            row.append(&go);

            if problem.removable {
                let s = sender.input_sender().clone();
                let remove = boxed_ghost_button("Remove", move || {
                    let _ = s.send(EpubEditorMsg::RemoveProblem(index));
                });
                remove.add_css_class("kalam-ed-problem-btn");
                remove.set_valign(gtk::Align::Center);
                remove.set_focus_on_click(false);
                row.append(&remove);
            }

            self.problems_host.append(&row);
        }
    }

    fn rebuild_outline(&mut self, sender: &ComponentSender<Self>) {
        clear_box(&self.outline_host);
        self.outline_headings.clear();
        let Some((kind, buffer, path)) = self
            .active_tab()
            .map(|t| (t.kind, t.buffer.clone(), t.path.clone()))
        else {
            return;
        };
        if kind != FileKind::Html {
            self.outline_host.append(&panel_note("No headings"));
            return;
        }
        let Some(buffer) = &buffer else {
            return;
        };
        let headings = extract_headings(&buffer_text(buffer));
        self.outline_headings = headings.clone();
        if headings.is_empty() {
            self.outline_host.append(&panel_note("No headings"));
            return;
        }
        for (level, text, offset) in headings {
            let row = gtk::Button::with_label(&text);
            row.add_css_class("kalam-ed-outline-row");
            row.set_size_request(-1, 36);
            row.set_margin_start(((level.saturating_sub(1)) as i32) * 16);
            row.child()
                .and_then(|c| c.downcast::<gtk::Label>().ok())
                .inspect(|label| {
                    label.set_halign(gtk::Align::Start);
                    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                });
            let path = path.clone();
            let s = sender.input_sender().clone();
            row.connect_clicked(move |_| {
                let _ = s.send(EpubEditorMsg::GoTo {
                    path: path.clone(),
                    line: None,
                    offset: Some(offset),
                });
            });
            self.outline_host.append(&row);
        }
        self.mark_outline_current();
    }

    // ── E2: engine Preview (decision 123) ──────────────────────────

    /// Engine preferences for the current Preview mode. "As the
    /// book ships" is the engine's neutral typography over the app
    /// palette; "As in Kalam" mirrors the reader's own resolution
    /// of this book's settings.
    fn preview_prefs(&self) -> kalam_reader::KalamPrefs {
        use crate::pages::reader::{book_settings, engine};
        let catalog = self.service.catalog();
        match self.preview_mode {
            PreviewMode::Ships => kalam_reader::KalamPrefs {
                palette: engine::reader_palette(&crate::theme::current(catalog)),
                ..kalam_reader::KalamPrefs::default()
            },
            PreviewMode::Kalam => {
                let effective = book_settings::BookReadingSettings::load_book(catalog, self.book_id)
                    .unwrap_or_else(|| book_settings::BookReadingSettings::load_globals(catalog));
                let theme = effective
                    .theme
                    .as_deref()
                    .map(|id| (id, crate::theme::choice_by_id(id).resolve()))
                    .filter(|(id, theme)| theme.id == *id)
                    .map(|(_, theme)| theme)
                    .unwrap_or_else(|| crate::theme::current(catalog));
                engine::engine_prefs(
                    engine::reader_palette(&theme),
                    effective.font_px,
                    effective.line_height,
                    effective.column_px,
                    effective.font_family.clone(),
                    effective.justify,
                    false,
                    effective.publisher_styles,
                )
            }
        }
    }

    fn apply_preview_prefs(&self) {
        if let Some(view) = &self.preview_view {
            view.set_prefs(self.preview_prefs());
        }
    }

    /// Re-write the preview copy 800ms after typing settles.
    fn schedule_preview_write(&mut self, sender: &ComponentSender<Self>) {
        if let Some(src) = self.preview_write_src.take() {
            src.remove();
        }
        let s = sender.input_sender().clone();
        self.preview_write_src = Some(gtk::glib::timeout_add_local_once(
            std::time::Duration::from_millis(800),
            move || {
                let _ = s.send(EpubEditorMsg::PreviewWriteDue);
            },
        ));
    }

    /// Write the preview copy now (book as saved + open buffers).
    /// A buffer that isn't well-formed XML right now keeps the
    /// last good render instead of blanking the preview mid-edit.
    fn request_preview_write(&self, sender: &ComponentSender<Self>) {
        let Some(book) = self.book.clone() else {
            return;
        };
        let mut replacements: HashMap<String, Vec<u8>> = HashMap::new();
        for tab in &self.tabs {
            let Some(buffer) = &tab.buffer else { continue };
            if !buffer.is_modified() {
                continue;
            }
            let text = buffer_text(buffer);
            if matches!(tab.kind, FileKind::Html | FileKind::Xml)
                && crate::epub_container::xml_problem(&text).is_some()
            {
                return;
            }
            replacements.insert(tab.path.clone(), text.into_bytes());
        }
        for (path, text) in &self.pending {
            let kind = self.container.as_ref().and_then(|c| c.file(path)).map(|f| f.kind);
            if matches!(kind, Some(FileKind::Html | FileKind::Xml))
                && crate::epub_container::xml_problem(text).is_some()
            {
                return;
            }
            replacements.insert(path.clone(), text.clone().into_bytes());
        }
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Preparing preview",
            move |_reporter| service.write_editor_preview(&book, &replacements),
            |_| {},
            move |result| {
                if let Ok(path) = result {
                    let _ = s.send(EpubEditorMsg::PreviewWritten(path));
                }
            },
        );
    }

    /// Open (or re-open) the engine on the freshly written preview
    /// copy, keeping the reader's place. A copy the engine refuses
    /// leaves the last good render standing.
    fn refresh_preview(&mut self, path: &std::path::Path, sender: &ComponentSender<Self>) {
        let (chapter, fraction) = self
            .preview_view
            .as_ref()
            .map(|view| {
                let pos = view.position();
                (pos.chapter, pos.fraction)
            })
            .unwrap_or((self.preview_chapter.max(0) as usize, 0.0));
        let Ok(view) = crate::pages::reader::engine::open_engine(path, self.preview_prefs()) else {
            return;
        };
        let view = Rc::new(view);
        view.set_mode(kalam_reader::ReadingMode::Scrolled);
        view.goto_chapter(chapter, fraction);

        // Click-sync, preview → source: a word tap jumps near the
        // word; a finished selection jumps to the exact span.
        {
            let weak = Rc::downgrade(&view);
            let s = sender.input_sender().clone();
            view.connect_word_tap(move |word, _x, _y| {
                if let Some(view) = weak.upgrade() {
                    let pos = view.position();
                    let _ = s.send(EpubEditorMsg::PreviewGoToWord {
                        chapter: pos.chapter,
                        fraction: pos.fraction,
                        word: word.to_string(),
                    });
                }
            });
        }
        {
            let s = sender.input_sender().clone();
            view.connect_selection(move |selected| {
                if let Some(selected) = selected {
                    if !selected.text.trim().is_empty() {
                        let _ = s.send(EpubEditorMsg::PreviewTextSelected);
                    }
                }
            });
        }

        let scrollbar =
            gtk::Scrollbar::new(gtk::Orientation::Vertical, Some(view.vadjustment()));
        scrollbar.set_halign(gtk::Align::End);
        scrollbar.set_valign(gtk::Align::Fill);
        scrollbar.add_css_class("kalam-reader-strip-bar");
        let overlay = gtk::Overlay::new();
        overlay.set_hexpand(true);
        overlay.set_vexpand(true);
        overlay.set_child(Some(view.widget()));
        overlay.add_overlay(&scrollbar);

        clear_box(&self.preview_host);
        self.preview_host.append(&overlay);
        if let Some(old) = self.preview_view.replace(view) {
            old.close();
        }
        self.preview_chapter = chapter as i32;
    }

    /// Source → preview: keep the preview on the chapter being
    /// edited, following big cursor moves within it. Small moves
    /// leave the preview where the reader scrolled it.
    fn sync_preview_to_cursor(&mut self) {
        if self.right_tab != RightTab::Preview {
            return;
        }
        let Some(view) = self.preview_view.as_ref() else {
            return;
        };
        let Some(tab) = self.active_tab() else { return };
        if tab.kind != FileKind::Html {
            return;
        }
        let Some(spine_index) = self
            .container
            .as_ref()
            .and_then(|c| c.file(&tab.path))
            .and_then(|f| f.spine_index)
        else {
            return;
        };
        let Some(buffer) = &tab.buffer else { return };
        let cursor = buffer.iter_at_mark(&buffer.get_insert()).offset();
        let map = strip_map(&buffer_text(buffer));
        let fraction = map.fraction_at(cursor.max(0) as usize);
        let pos = view.position();
        if pos.chapter != spine_index || (pos.fraction - fraction).abs() > 0.12 {
            view.goto_chapter(spine_index, fraction);
            self.preview_chapter = spine_index as i32;
        }
    }

    /// Word tap → the occurrence nearest the tap's place in the
    /// chapter, selected in the source.
    fn preview_word_jump(
        &mut self,
        chapter: usize,
        fraction: f64,
        word: &str,
        sender: &ComponentSender<Self>,
    ) {
        let Some(path) = self.spine_path(chapter) else {
            return;
        };
        let Some(text) = self.file_text(&path) else {
            return;
        };
        let map = strip_map(&text);
        let needle = normalize_ws(word);
        if needle.is_empty() {
            return;
        }
        let target = (fraction * map.text.chars().count() as f64) as usize;
        let mut best: Option<usize> = None;
        for (at, _) in map.text.match_indices(&needle) {
            let at_chars = map.text[..at].chars().count();
            match best {
                Some(prev)
                    if (prev as i64 - target as i64).abs()
                        <= (at_chars as i64 - target as i64).abs() => {}
                _ => best = Some(at_chars),
            }
        }
        if let Some(at) = best {
            self.jump_to_span(&path, &map, at, at + needle.chars().count(), sender);
        }
    }

    /// Selection → the exact quoted span in the source, via the
    /// highlight capture's quote anchor.
    fn preview_selection_jump(&mut self, sender: &ComponentSender<Self>) {
        let Some(view) = self.preview_view.as_ref() else {
            return;
        };
        let Some(captured) = view.capture_highlight(kalam_reader::HighlightColor::Yellow) else {
            return;
        };
        let start = captured.start;
        let Some(path) = self
            .spine_path(start.spine_index)
            .or_else(|| (!start.spine_href.is_empty()).then(|| start.spine_href.clone()))
        else {
            return;
        };
        let Some(text) = self.file_text(&path) else {
            return;
        };
        let map = strip_map(&text);
        let prefix: String = normalize_ws(&start.quote.prefix)
            .chars()
            .rev()
            .take(20)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        let exact = normalize_ws(&start.quote.exact);
        if exact.is_empty() {
            return;
        }
        let needle = format!("{prefix}{exact}");
        let found = map
            .text
            .find(&needle)
            .map(|at| map.text[..at].chars().count() + prefix.chars().count())
            .or_else(|| map.text.find(&exact).map(|at| map.text[..at].chars().count()));
        if let Some(at) = found {
            self.jump_to_span(&path, &map, at, at + exact.chars().count(), sender);
        }
    }

    fn spine_path(&self, chapter: usize) -> Option<String> {
        self.container.as_ref().and_then(|c| {
            c.files
                .iter()
                .find(|f| f.spine_index == Some(chapter))
                .map(|f| f.path.clone())
        })
    }

    /// A file's current text: its open buffer if it has one, else
    /// a pending tool edit, else the container copy.
    fn file_text(&self, path: &str) -> Option<String> {
        if let Some(text) = self
            .tabs
            .iter()
            .find(|t| t.path == path)
            .and_then(|t| t.buffer.as_ref())
            .map(buffer_text)
        {
            return Some(text);
        }
        if let Some(text) = self.pending.get(path) {
            return Some(text.clone());
        }
        self.container.as_ref().and_then(|c| c.text(path))
    }

    /// Open `path` and select the source span a normalized-text
    /// range maps to.
    fn jump_to_span(
        &mut self,
        path: &str,
        map: &StripMap,
        norm_start: usize,
        norm_end: usize,
        sender: &ComponentSender<Self>,
    ) {
        self.open_file(path, true, sender);
        let (start, end) = map.source_span(norm_start, norm_end);
        let Some(buffer) = self
            .tabs
            .iter()
            .find(|t| t.path == path)
            .and_then(|t| t.buffer.clone())
        else {
            return;
        };
        let start_iter = buffer.iter_at_offset(start as i32);
        let end_iter = buffer.iter_at_offset(end as i32);
        buffer.select_range(&start_iter, &end_iter);
        if let Some(view) = self.active_view() {
            let mut iter = start_iter;
            view.scroll_to_iter(&mut iter, 0.1, false, 0.0, 0.0);
            view.grab_focus();
        }
    }

    /// The Preview mode menu (decision 123), rebuilt on each open
    /// so the tick is always current.
    fn toggle_preview_menu(&mut self, sender: &ComponentSender<Self>) {
        if let Some(menu) = self.preview_menu.take() {
            menu.popover().popdown();
            menu.popover().unparent();
            return;
        }
        let Some((_, button)) = self
            .right_tab_btns
            .iter()
            .find(|(tab, _)| *tab == RightTab::Preview)
        else {
            return;
        };
        let menu = crate::widgets::menu::menu_popover(crate::widgets::menu::MenuWidth::Short);
        let popover = menu.popover();
        for (mode, label) in [
            (PreviewMode::Ships, "As the book ships"),
            (PreviewMode::Kalam, "As in Kalam"),
        ] {
            let s = sender.input_sender().clone();
            let pop = popover.clone();
            menu.add_check_item(label, self.preview_mode == mode, move |_| {
                pop.popdown();
                let _ = s.send(EpubEditorMsg::SetPreviewMode(mode));
            });
        }
        popover.set_parent(button);
        popover.popup();
        self.preview_menu = Some(menu);
    }

    // ── E2: Outline mark + Live CSS (decision 133) ─────────────────

    fn cursor_char_offset(&self) -> Option<usize> {
        let buffer = self.active_buffer()?;
        Some(buffer.iter_at_mark(&buffer.get_insert()).offset().max(0) as usize)
    }

    /// Mark the heading the cursor sits under.
    fn mark_outline_current(&self) {
        let Some(cursor) = self.cursor_char_offset() else {
            return;
        };
        let current = self
            .outline_headings
            .iter()
            .rposition(|(_, _, offset)| *offset <= cursor as i32);
        let mut child = self.outline_host.first_child();
        let mut index = 0usize;
        while let Some(widget) = child {
            if Some(index) == current {
                widget.add_css_class("current");
            } else {
                widget.remove_css_class("current");
            }
            child = widget.next_sibling();
            index += 1;
        }
    }

    /// Live CSS: the rules matching the element at the cursor, in
    /// cascade order, overridden declarations struck through.
    /// HTML only (decision 133).
    fn rebuild_livecss(&mut self, sender: &ComponentSender<Self>) {
        clear_box(&self.livecss_host);
        let Some(tab) = self.active_tab() else {
            self.livecss_host.append(&panel_note(
                "Place the cursor inside an element in an HTML file to see the CSS that applies to it.",
            ));
            return;
        };
        if tab.kind != FileKind::Html {
            self.livecss_host.append(&panel_note(
                "Live CSS works in HTML files — it shows the rules that match the element at the cursor.",
            ));
            return;
        }
        let Some(buffer) = &tab.buffer else { return };
        let html = buffer_text(buffer);
        let tab_path = tab.path.clone();
        let Some(cursor) = self.cursor_char_offset() else {
            return;
        };
        let chain = element_chain_at(&html, cursor);
        let Some(element) = chain.last() else {
            self.livecss_host.append(&panel_note(
                "Place the cursor inside an element in an HTML file to see the CSS that applies to it.",
            ));
            return;
        };
        self.refresh_css_cache();

        // Inline <style> blocks of this file join after the sheets.
        let mut rules: Vec<(String, CssRule)> = self.css_cache.clone();
        for (css, first_line) in style_blocks(&html) {
            for mut rule in crate::epub_css::parse_rules(&css) {
                rule.line += first_line.saturating_sub(1);
                rules.push((tab_path.clone(), rule));
            }
        }

        struct Matched {
            selector: String,
            spec: (u32, u32, u32),
            order: usize,
            path: String,
            line: usize,
            decls: Vec<crate::epub_css::CssDecl>,
        }
        let mut matched: Vec<Matched> = Vec::new();
        for (order, (path, rule)) in rules.iter().enumerate() {
            let mut best: Option<((u32, u32, u32), &String)> = None;
            for selector in &rule.selectors {
                if let Some(spec) = crate::epub_css::selector_matches(selector, &chain) {
                    if best.is_none_or(|(b, _)| spec > b) {
                        best = Some((spec, selector));
                    }
                }
            }
            if let Some((spec, selector)) = best {
                matched.push(Matched {
                    selector: selector.clone(),
                    spec,
                    order,
                    path: path.clone(),
                    line: rule.line,
                    decls: rule.declarations.clone(),
                });
            }
        }
        // The style="" attribute outranks every sheet.
        if let Some(style) = element.attr("style") {
            let decls = crate::epub_css::parse_declarations(style);
            if !decls.is_empty() {
                matched.push(Matched {
                    selector: "style attribute".to_string(),
                    spec: (u32::MAX, 0, 0),
                    order: usize::MAX,
                    path: tab_path.clone(),
                    line: 0,
                    decls,
                });
            }
        }
        matched.sort_by_key(|m| (m.spec, m.order));

        // The winning declaration for each property.
        let mut winner: HashMap<String, (bool, usize, usize)> = HashMap::new();
        for (mi, m) in matched.iter().enumerate() {
            for (di, decl) in m.decls.iter().enumerate() {
                match winner.get(&decl.property) {
                    Some((important, _, _)) if *important && !decl.important => {}
                    _ => {
                        winner.insert(decl.property.clone(), (decl.important, mi, di));
                    }
                }
            }
        }

        let header = gtk::Label::new(Some(&element.describe()));
        header.add_css_class("kalam-ed-css-element");
        header.set_halign(gtk::Align::Start);
        header.set_margin_start(20);
        header.set_margin_top(16);
        header.set_margin_bottom(8);
        self.livecss_host.append(&header);

        if matched.is_empty() {
            self.livecss_host
                .append(&panel_note("No rules in the book's stylesheets match this element."));
            return;
        }

        for (mi, m) in matched.iter().enumerate() {
            let source = if m.line > 0 {
                format!("{} · line {}", file_name(&m.path), m.line)
            } else {
                file_name(&m.path).to_string()
            };
            let head = gtk::Button::new();
            head.add_css_class("kalam-ed-css-rule");
            head.set_focus_on_click(false);
            let head_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let selector_label = gtk::Label::new(Some(&m.selector));
            selector_label.add_css_class("kalam-ed-css-selector");
            selector_label.set_halign(gtk::Align::Start);
            selector_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            head_box.append(&selector_label);
            let source_label = gtk::Label::new(Some(&source));
            source_label.add_css_class("kalam-ui-meta");
            source_label.set_halign(gtk::Align::Start);
            head_box.append(&source_label);
            head.set_child(Some(&head_box));
            let path = m.path.clone();
            let line = m.line.max(1);
            let s = sender.input_sender().clone();
            head.connect_clicked(move |_| {
                let _ = s.send(EpubEditorMsg::GoToCssRule {
                    path: path.clone(),
                    line,
                });
            });
            self.livecss_host.append(&head);

            for (di, decl) in m.decls.iter().enumerate() {
                let text = if decl.important {
                    format!("{}: {} !important;", decl.property, decl.value)
                } else {
                    format!("{}: {};", decl.property, decl.value)
                };
                let label = gtk::Label::new(Some(&text));
                label.add_css_class("kalam-ed-css-decl");
                if winner.get(&decl.property).is_some_and(|(_, wmi, wdi)| (*wmi, *wdi) != (mi, di)) {
                    label.add_css_class("overridden");
                }
                label.set_halign(gtk::Align::Start);
                label.set_margin_start(36);
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                self.livecss_host.append(&label);
            }
        }
    }

    /// Parse the book's stylesheets into the cache when their text
    /// changed (open buffers included).
    fn refresh_css_cache(&mut self) {
        let Some(container) = &self.container else {
            return;
        };
        let sheets: Vec<(String, String)> = container
            .section_files(TreeSection::Styles)
            .iter()
            .filter(|f| f.kind == FileKind::Css)
            .filter_map(|f| self.file_text(&f.path).map(|t| (f.path.clone(), t)))
            .collect();
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for (path, text) in &sheets {
            path.hash(&mut hasher);
            text.hash(&mut hasher);
        }
        let key = hasher.finish();
        if key == self.css_cache_key && !self.css_cache.is_empty() {
            return;
        }
        self.css_cache_key = key;
        self.css_cache = sheets
            .iter()
            .flat_map(|(path, text)| {
                crate::epub_css::parse_rules(text)
                    .into_iter()
                    .map(|rule| (path.clone(), rule))
                    .collect::<Vec<_>>()
            })
            .collect();
    }

    // ── E2: Check book (decision 128) ──────────────────────────────

    /// The whole book as the checker sees it: container text with
    /// open buffers laid over it, so unsaved edits are checked too.
    fn collect_check(&self) -> Vec<BookProblem> {
        let Some(container) = &self.container else {
            return Vec::new();
        };
        let files: Vec<crate::epub_check::CheckFile> = container
            .files
            .iter()
            .map(|file| crate::epub_check::CheckFile {
                path: file.path.clone(),
                kind: file.kind,
                spine_index: file.spine_index,
                in_manifest: file.in_manifest,
                is_nav: file.is_nav,
                is_cover_image: file.is_cover_image,
                text: self.file_text(&file.path),
            })
            .collect();
        crate::epub_check::check_book(
            &files,
            &container.opf_path,
            container.ncx_path.as_deref(),
        )
    }

    /// Run Check book now. Pure text work on buffers already in
    /// memory — a whole-book pass is well under a frame, so it runs
    /// here rather than on a worker that could go stale.
    fn run_check(&mut self, sender: &ComponentSender<Self>) {
        self.problems = self.collect_check();
        self.update_problem_status();
        if self.right_tab == RightTab::Problems {
            self.rebuild_problems(sender);
        }
    }

    /// Re-check 1.2s after typing settles.
    fn schedule_check(&mut self, sender: &ComponentSender<Self>) {
        if let Some(src) = self.check_src.take() {
            src.remove();
        }
        let s = sender.input_sender().clone();
        self.check_src = Some(gtk::glib::timeout_add_local_once(
            std::time::Duration::from_millis(1200),
            move || {
                let _ = s.send(EpubEditorMsg::CheckDue);
            },
        ));
    }

    /// The fix mini-diff (decision 132): the changed line before
    /// and after, Skip / Apply at 32px. Apply sends the fix back
    /// as a message so it lands as one undo step.
    fn present_fix_preview(&mut self, index: usize, sender: &ComponentSender<Self>) {
        let Some(problem) = self.problems.get(index) else {
            return;
        };
        let Some(fix) = problem.fix.clone() else {
            return;
        };
        let path = problem.path.clone();
        if let Some(old) = self.fix_popover.take() {
            old.popdown();
            old.unparent();
        }

        let content = gtk::Box::new(gtk::Orientation::Vertical, 8);
        content.set_margin_all(12);
        content.set_size_request(380, -1);
        let before = gtk::Label::new(Some(fix.before_line.trim()));
        before.add_css_class("kalam-ed-diff-line");
        before.set_halign(gtk::Align::Start);
        before.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        content.append(&before);
        let after = gtk::Label::new(Some(fix.after_line.trim()));
        after.add_css_class("kalam-ed-diff-line");
        after.add_css_class("kalam-ed-diff-add");
        after.set_halign(gtk::Align::Start);
        after.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        content.append(&after);

        let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        buttons.set_halign(gtk::Align::End);
        let s = sender.input_sender().clone();
        let skip = boxed_ghost_button("Skip", move || {
            let _ = s.send(EpubEditorMsg::FixSkip);
        });
        skip.set_size_request(-1, 32);
        buttons.append(&skip);
        let s = sender.input_sender().clone();
        let apply = primary_button("Apply fix", move || {
            let _ = s.send(EpubEditorMsg::FixApply(fix.clone(), path.clone()));
        });
        apply.set_size_request(-1, 32);
        buttons.append(&apply);
        content.append(&buttons);

        let popover = gtk::Popover::new();
        popover.set_child(Some(&content));
        popover.set_parent(&self.problems_host);
        popover.set_position(gtk::PositionType::Bottom);
        popover.popup();
        self.fix_popover = Some(popover);
    }

    /// Apply one fix as a single undo step. The offsets are bytes
    /// into the text the checker saw; they only apply if the live
    /// buffer still holds the same original bytes there.
    fn apply_fix(&mut self, fix: &ProblemFix, path: &str, sender: &ComponentSender<Self>) {
        if let Some(popover) = self.fix_popover.take() {
            popover.popdown();
            popover.unparent();
        }
        self.open_file(path, true, sender);
        let Some(buffer) = self
            .tabs
            .iter()
            .find(|t| t.path == path)
            .and_then(|t| t.buffer.clone())
        else {
            crate::notify::error("Could not apply the fix", "That file is not editable here.");
            return;
        };
        let text = buffer_text(&buffer);
        let Some(current) = text.get(fix.start..fix.end) else {
            crate::notify::error(
                "Could not apply the fix",
                "The text changed since the check ran.",
            );
            return;
        };
        if current != fix.original {
            crate::notify::error(
                "Could not apply the fix",
                "The text changed since the check ran.",
            );
            return;
        }
        let start_char = text[..fix.start].chars().count() as i32;
        let end_char = start_char + fix.original.chars().count() as i32;
        let mut start = buffer.iter_at_offset(start_char);
        let mut end = buffer.iter_at_offset(end_char);
        buffer.begin_user_action();
        buffer.delete(&mut start, &mut end);
        let mut at = buffer.iter_at_offset(start_char);
        buffer.insert(&mut at, &fix.replacement);
        buffer.end_user_action();
        // The buffer's changed signal re-checks and re-renders.
    }

    /// Remove an unused file from the book (decision 132). The
    /// service snapshots first; FileRemoved reloads the workspace.
    fn remove_problem_file(&self, index: usize, sender: &ComponentSender<Self>) {
        let Some(problem) = self.problems.get(index) else {
            return;
        };
        if !problem.removable {
            return;
        }
        let Some(book) = self.book.clone() else {
            return;
        };
        let path = problem.path.clone();
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Removing file",
            {
                let path = path.clone();
                move |_reporter| {
                    service
                        .remove_editor_file(&book, &path)
                        .map(|()| path.clone())
                }
            },
            |_| {},
            move |result| match result {
                Ok(path) => {
                    let _ = s.send(EpubEditorMsg::FileRemoved(path));
                }
                Err(message) => {
                    let _ = s.send(EpubEditorMsg::ActionFailed(message));
                }
            },
        );
    }

    // ── E3: Tools menu, instant tools, after-tool pill ─────────────

    fn toggle_tools_menu(&mut self, sender: &ComponentSender<Self>) {
        if let Some(menu) = self.tools_menu.take() {
            menu.popover().popdown();
            menu.popover().unparent();
            return;
        }
        let menu = crate::widgets::menu::menu_popover(crate::widgets::menu::MenuWidth::Wide);
        let count = self.problems.len();
        let s = sender.input_sender().clone();
        if count > 0 {
            menu.add_item_with_count("Check book", count, move || {
                let _ = s.send(EpubEditorMsg::SetRightTab(RightTab::Problems));
            });
        } else {
            menu.add_item("Check book", move || {
                let _ = s.send(EpubEditorMsg::SetRightTab(RightTab::Problems));
            });
        }
        let s = sender.input_sender().clone();
        menu.add_item("Find & replace", move || {
            let _ = s.send(EpubEditorMsg::ToggleFind);
        });
        menu.add_separator();
        let clean = crate::widgets::menu::menu_popover(crate::widgets::menu::MenuWidth::Tools);
        for kind in [
            InstantTool::Reformat,
            InstantTool::Smarten,
            InstantTool::RemoveUnusedCss,
            InstantTool::FixHtml,
        ] {
            let s = sender.input_sender().clone();
            clean.add_item(kind.label(), move || {
                let _ = s.send(EpubEditorMsg::ToolChosen(kind));
            });
        }
        menu.add_submenu("Clean up", clean);
        menu.add_separator();
        let s = sender.input_sender().clone();
        menu.add_item("Command palette", move || {
            let _ = s.send(EpubEditorMsg::TogglePalette);
        });
        menu.popover().set_parent(&self.tools_btn);
        menu.popover().popup();
        self.tools_menu = Some(menu);
    }

    /// "This file or Whole book?" (decision 128): two option rows
    /// (section 38) and the tool's own name on the filled button.
    fn present_scope_dialog(&self, kind: InstantTool, sender: &ComponentSender<Self>) {
        fn set_tick(tick: &gtk::Label, on: bool) {
            if on {
                tick.add_css_class("on");
            } else {
                tick.remove_css_class("on");
            }
        }
        fn option_row(title: &str, meta: &str) -> (gtk::Button, gtk::Label) {
            let row = gtk::Button::new();
            row.add_css_class("kalam-ed-option");
            row.set_size_request(-1, 52);
            let h = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            let tick = gtk::Label::new(Some("✓"));
            tick.add_css_class("kalam-ed-tick");
            tick.set_size_request(22, 22);
            tick.set_valign(gtk::Align::Center);
            h.append(&tick);
            let texts = gtk::Box::new(gtk::Orientation::Vertical, 0);
            texts.set_valign(gtk::Align::Center);
            let t = gtk::Label::new(Some(title));
            t.add_css_class("kalam-ed-option-title");
            t.set_halign(gtk::Align::Start);
            texts.append(&t);
            let m = gtk::Label::new(Some(meta));
            m.add_css_class("kalam-ui-meta");
            m.set_halign(gtk::Align::Start);
            texts.append(&m);
            h.append(&texts);
            row.set_child(Some(&h));
            (row, tick)
        }

        let this_file = self
            .active_tab()
            .filter(|t| kind.applies_to(t.kind))
            .map(|t| file_name(&t.path).to_string());
        let whole = Rc::new(std::cell::Cell::new(this_file.is_none()));

        let shell = dialog_shell(DialogSize::Form, None);
        let (row_file, tick_file) = option_row(
            "This file",
            this_file.as_deref().unwrap_or("No suitable file open"),
        );
        row_file.set_sensitive(this_file.is_some());
        let (row_book, tick_book) = option_row("Whole book", kind.whole_meta());
        set_tick(&tick_file, !whole.get());
        set_tick(&tick_book, whole.get());
        {
            let whole = whole.clone();
            let tick_file = tick_file.clone();
            let tick_book = tick_book.clone();
            row_file.connect_clicked(move |_| {
                whole.set(false);
                set_tick(&tick_file, true);
                set_tick(&tick_book, false);
            });
        }
        {
            let whole = whole.clone();
            row_book.connect_clicked(move |_| {
                whole.set(true);
                set_tick(&tick_file, false);
                set_tick(&tick_book, true);
            });
        }
        shell.body().append(&row_file);
        shell.body().append(&row_book);

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let slot_cancel = slot.clone();
        shell.footer().append(&boxed_ghost_button("Cancel", move || {
            close_slot(&slot_cancel);
        }));
        let slot_run = slot.clone();
        let s = sender.input_sender().clone();
        shell.footer().append(&primary_button(kind.label(), move || {
            close_slot(&slot_run);
            let _ = s.send(EpubEditorMsg::RunInstantTool(kind, whole.get()));
        }));

        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            kind.label(),
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    /// Work out a tool's whole-file replacements without touching
    /// anything; `None` means there is nothing to do.
    fn compute_tool_changes(
        &self,
        kind: InstantTool,
        whole_book: bool,
    ) -> Option<(String, Vec<(String, String)>)> {
        let container = self.container.as_ref()?;
        let targets: Vec<String> = if whole_book {
            container
                .files
                .iter()
                .filter(|f| kind.applies_to(f.kind))
                .map(|f| f.path.clone())
                .collect()
        } else {
            vec![self.active_tab().filter(|t| kind.applies_to(t.kind))?.path.clone()]
        };

        let mut changes: Vec<(String, String)> = Vec::new();
        let mut total = 0usize;
        match kind {
            InstantTool::Smarten => {
                for path in targets {
                    let Some(text) = self.file_text(&path) else { continue };
                    let (new, n) = crate::epub_tools::smarten_html(&text);
                    if n > 0 && new != text {
                        total += n;
                        changes.push((path, new));
                    }
                }
            }
            InstantTool::Reformat => {
                for path in targets {
                    let Some(text) = self.file_text(&path) else { continue };
                    let Some(fkind) = container.file(&path).map(|f| f.kind) else {
                        continue;
                    };
                    if let Some(new) = crate::epub_tools::reformat(&text, fkind) {
                        if new != text {
                            changes.push((path, new));
                        }
                    }
                }
            }
            InstantTool::RemoveUnusedCss => {
                let mut chains: Vec<Vec<crate::epub_css::Elem>> = Vec::new();
                for file in &container.files {
                    if file.kind == FileKind::Html {
                        if let Some(text) = self.file_text(&file.path) {
                            chains.extend(crate::epub_tools::all_element_chains(&text));
                        }
                    }
                }
                for path in targets {
                    let Some(text) = self.file_text(&path) else { continue };
                    let (new, n) = crate::epub_tools::remove_unused_css(&text, &chains);
                    if n > 0 && new != text {
                        total += n;
                        changes.push((path, new));
                    }
                }
            }
            InstantTool::FixHtml => {
                let problems = self.collect_check();
                for path in targets {
                    let mut fixes: Vec<&ProblemFix> = problems
                        .iter()
                        .filter(|p| p.path == path)
                        .filter_map(|p| p.fix.as_ref())
                        .collect();
                    if fixes.is_empty() {
                        continue;
                    }
                    let Some(text) = self.file_text(&path) else { continue };
                    fixes.sort_by_key(|f| std::cmp::Reverse(f.start));
                    let mut new = text;
                    let mut applied = 0usize;
                    for fix in fixes {
                        if new.get(fix.start..fix.end) == Some(fix.original.as_str()) {
                            new.replace_range(fix.start..fix.end, &fix.replacement);
                            applied += 1;
                        }
                    }
                    if applied > 0 {
                        total += applied;
                        changes.push((path, new));
                    }
                }
            }
        }
        if changes.is_empty() {
            return None;
        }
        let files_word = |n: usize| if n == 1 { "file" } else { "files" };
        let summary = if changes.len() == 1 {
            let name = file_name(&changes[0].0);
            match kind {
                InstantTool::Smarten => format!("{total} punctuation marks smartened in {name}"),
                InstantTool::Reformat => format!("Reformatted {name}"),
                InstantTool::RemoveUnusedCss => {
                    format!("{total} {} removed from {name}", if total == 1 { "rule" } else { "rules" })
                }
                InstantTool::FixHtml => {
                    format!("{total} {} applied in {name}", if total == 1 { "fix" } else { "fixes" })
                }
            }
        } else {
            match kind {
                InstantTool::Smarten => {
                    format!("{total} punctuation marks smartened in {} {}", changes.len(), files_word(changes.len()))
                }
                InstantTool::Reformat => {
                    format!("Reformatted {} {}", changes.len(), files_word(changes.len()))
                }
                InstantTool::RemoveUnusedCss => {
                    format!("{total} rules removed from {} {}", changes.len(), files_word(changes.len()))
                }
                InstantTool::FixHtml => {
                    format!("{total} fixes applied in {} {}", changes.len(), files_word(changes.len()))
                }
            }
        };
        Some((summary, changes))
    }

    /// Snapshot first (decision 135), then apply on the UI thread.
    fn start_apply(
        &self,
        snapshot_label: String,
        summary: String,
        changes: Vec<(String, String)>,
        sender: &ComponentSender<Self>,
    ) {
        let Some(book) = self.book.clone() else {
            return;
        };
        let files = changes.len() as i64;
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Taking snapshot",
            move |_reporter| {
                service
                    .editor_snapshot(&book, &snapshot_label, files)
                    .map(|()| (summary, changes))
            },
            |_| {},
            move |result| match result {
                Ok((summary, changes)) => {
                    let _ = s.send(EpubEditorMsg::ApplyChanges { summary, changes });
                }
                Err(message) => {
                    let _ = s.send(EpubEditorMsg::ActionFailed(message));
                }
            },
        );
    }

    /// Apply whole-file replacements: one undo step per open
    /// buffer, a pending edit per unopened file.
    fn apply_changes(
        &mut self,
        summary: &str,
        changes: Vec<(String, String)>,
        sender: &ComponentSender<Self>,
    ) {
        let mut undo = ToolUndo { entries: Vec::new() };
        for (path, new_text) in &changes {
            if let Some(buffer) = self
                .tabs
                .iter()
                .find(|t| &t.path == path)
                .and_then(|t| t.buffer.clone())
            {
                let cursor = buffer.cursor_position();
                buffer.begin_user_action();
                let (mut a, mut b) = (buffer.start_iter(), buffer.end_iter());
                buffer.delete(&mut a, &mut b);
                let mut at = buffer.start_iter();
                buffer.insert(&mut at, new_text);
                buffer.end_user_action();
                let pos = buffer.iter_at_offset(cursor.min(buffer.char_count()));
                buffer.place_cursor(&pos);
                undo.entries.push(ToolUndoEntry::Buffer(path.clone()));
            } else {
                let before = self.pending.insert(path.clone(), new_text.clone());
                undo.entries.push(ToolUndoEntry::Pending(path.clone(), before));
            }
        }
        self.tool_undo = Some(undo);
        self.show_pill(summary, true, sender);
        self.run_check(sender);
        self.request_preview_write(sender);
        self.sync_chrome(sender);
        if self.find_panel.is_visible() {
            self.run_find(sender);
        }
    }

    fn undo_tool(&mut self, sender: &ComponentSender<Self>) {
        if let Some(undo) = self.tool_undo.take() {
            for entry in undo.entries {
                match entry {
                    ToolUndoEntry::Buffer(path) => {
                        if let Some(buffer) = self
                            .tabs
                            .iter()
                            .find(|t| t.path == path)
                            .and_then(|t| t.buffer.clone())
                        {
                            buffer.undo();
                        }
                    }
                    ToolUndoEntry::Pending(path, before) => match before {
                        Some(text) => {
                            self.pending.insert(path, text);
                        }
                        None => {
                            self.pending.remove(&path);
                        }
                    },
                }
            }
        }
        self.hide_pill();
        self.run_check(sender);
        self.request_preview_write(sender);
        self.sync_chrome(sender);
    }

    /// The after-tool pill (section 39): bottom centre, gone in
    /// six seconds. Undo stays valid for buffers after it fades
    /// (it is ordinary buffer undo), but the pill is the way back
    /// for files that were never opened.
    fn show_pill(&mut self, text: &str, with_undo: bool, sender: &ComponentSender<Self>) {
        if let Some(src) = self.pill_src.take() {
            src.remove();
        }
        self.pill_label.set_label(text);
        self.pill_undo_btn.set_visible(with_undo);
        self.pill.set_visible(true);
        let s = sender.input_sender().clone();
        self.pill_src = Some(gtk::glib::timeout_add_local_once(
            std::time::Duration::from_millis(6000),
            move || {
                let _ = s.send(EpubEditorMsg::PillHide);
            },
        ));
    }

    fn hide_pill(&mut self) {
        if let Some(src) = self.pill_src.take() {
            src.remove();
        }
        self.pill.set_visible(false);
    }

    // ── E3: Find & replace (decision 131) ──────────────────────────

    fn toggle_find(&mut self, sender: &ComponentSender<Self>) {
        if self.find_panel.is_visible() {
            self.find_panel.set_visible(false);
            return;
        }
        self.palette_panel.set_visible(false);
        if let Some(buffer) = self.active_buffer() {
            if let Some((a, b)) = buffer.selection_bounds() {
                let text = buffer.text(&a, &b, false);
                if !text.is_empty() && !text.contains('\n') {
                    self.find_entry.set_text(&text);
                }
            }
        }
        self.sync_find_scope_btns();
        self.find_panel.set_visible(true);
        self.find_entry.grab_focus();
        self.find_entry.select_region(0, -1);
        self.run_find(sender);
    }

    fn sync_find_scope_btns(&self) {
        for (scope, btn) in &self.find_scope_btns {
            if *scope == self.find_scope {
                btn.add_css_class("active");
            } else {
                btn.remove_css_class("active");
            }
        }
    }

    fn find_files(&self) -> Vec<String> {
        let Some(container) = &self.container else {
            return Vec::new();
        };
        match self.find_scope {
            FindScope::ThisFile => self
                .active_tab()
                .filter(|t| matches!(t.kind, FileKind::Html | FileKind::Css | FileKind::Xml))
                .map(|t| vec![t.path.clone()])
                .unwrap_or_default(),
            FindScope::AllText => container
                .files
                .iter()
                .filter(|f| matches!(f.kind, FileKind::Html | FileKind::Css))
                .map(|f| f.path.clone())
                .collect(),
            FindScope::WholeBook => container
                .files
                .iter()
                .filter(|f| matches!(f.kind, FileKind::Html | FileKind::Css | FileKind::Xml))
                .map(|f| f.path.clone())
                .collect(),
        }
    }

    fn searcher(&self) -> Result<crate::epub_tools::Searcher, String> {
        crate::epub_tools::Searcher::new(
            &self.find_entry.text(),
            crate::epub_tools::SearchOptions {
                match_case: self.chip_case.is_active(),
                whole_word: self.chip_word.is_active(),
                regex: self.chip_regex.is_active(),
            },
        )
    }

    fn run_find(&mut self, sender: &ComponentSender<Self>) {
        const MAX_HITS: usize = 300;
        clear_box(&self.find_results_host);
        self.find_hits.clear();
        self.find_current = None;
        let query = self.find_entry.text();
        if query.is_empty() {
            self.find_count.set_label("");
            return;
        }
        let searcher = match self.searcher() {
            Ok(searcher) => searcher,
            Err(message) => {
                self.find_count.set_label(&message);
                return;
            }
        };
        let theme = crate::theme::current(self.service.catalog());
        let tint = format!("{}{}", theme.text, if theme.dark { "1a" } else { "14" });

        let mut total = 0usize;
        let mut files_with = 0usize;
        for path in self.find_files() {
            let Some(text) = self.file_text(&path) else { continue };
            let spans = searcher.find_all(&text);
            if spans.is_empty() {
                continue;
            }
            files_with += 1;
            total += spans.len();
            if self.find_scope != FindScope::ThisFile {
                let heading = gtk::Label::new(Some(&format!(
                    "{} — {}",
                    file_name(&path),
                    spans.len()
                )));
                heading.add_css_class("kalam-editorpicker-group");
                heading.set_halign(gtk::Align::Start);
                heading.set_margin_top(10);
                heading.set_margin_bottom(2);
                self.find_results_host.append(&heading);
            }
            for (b0, b1) in spans {
                if self.find_hits.len() >= MAX_HITS {
                    break;
                }
                let start = text[..b0].chars().count();
                let end = start + text[b0..b1].chars().count();
                let line = text[..b0].bytes().filter(|b| *b == b'\n').count() + 1;
                let line_start = text[..b0].rfind('\n').map(|n| n + 1).unwrap_or(0);
                let line_end = text[b1..].find('\n').map(|n| b1 + n).unwrap_or(text.len());
                let before_full = &text[line_start..b0];
                let after_full = &text[b1..line_end];
                let before_chars: Vec<char> = before_full.chars().collect();
                let before = if before_chars.len() > 40 {
                    format!("…{}", before_chars[before_chars.len() - 40..].iter().collect::<String>())
                } else {
                    before_full.to_string()
                };
                let after_chars: Vec<char> = after_full.chars().collect();
                let after = if after_chars.len() > 40 {
                    format!("{}…", after_chars[..40].iter().collect::<String>())
                } else {
                    after_full.to_string()
                };
                let hit = FindHit {
                    path: path.clone(),
                    start,
                    end,
                    line,
                    before: before.trim_start().to_string(),
                    matched: text[b0..b1].replace('\n', " "),
                    after,
                };
                let markup = format!(
                    "{}<span background=\"{tint}\">{}</span>{}",
                    gtk::glib::markup_escape_text(&hit.before),
                    gtk::glib::markup_escape_text(&hit.matched),
                    gtk::glib::markup_escape_text(&hit.after),
                );
                let index = self.find_hits.len();
                let row = gtk::Button::new();
                row.add_css_class("kalam-ed-find-row");
                row.set_focus_on_click(false);
                let nums = gtk::Label::new(Some(&format!("{}", hit.line)));
                nums.add_css_class("kalam-ui-meta");
                nums.set_width_chars(4);
                nums.set_xalign(1.0);
                let label = gtk::Label::new(None);
                label.set_markup(&markup);
                label.set_halign(gtk::Align::Start);
                label.set_hexpand(true);
                label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                row_box.append(&nums);
                row_box.append(&label);
                row.set_child(Some(&row_box));
                let s = sender.input_sender().clone();
                row.connect_clicked(move |_| {
                    let _ = s.send(EpubEditorMsg::FindJump(index));
                });
                self.find_results_host.append(&row);
                self.find_hits.push(hit);
            }
        }
        if total == 0 {
            self.find_count.set_label("No matches");
        } else {
            let mut label = format!(
                "{total} {} in {files_with} {}",
                if total == 1 { "match" } else { "matches" },
                if files_with == 1 { "file" } else { "files" }
            );
            if total > self.find_hits.len() {
                label.push_str(&format!(" · showing first {}", self.find_hits.len()));
            }
            self.find_count.set_label(&label);
        }
    }

    fn jump_to_hit(&mut self, index: usize, sender: &ComponentSender<Self>) {
        let Some(hit) = self.find_hits.get(index) else {
            return;
        };
        let (path, start, end) = (hit.path.clone(), hit.start, hit.end);
        self.open_file(&path, true, sender);
        if let Some(buffer) = self
            .tabs
            .iter()
            .find(|t| t.path == path)
            .and_then(|t| t.buffer.clone())
        {
            let a = buffer.iter_at_offset(start as i32);
            let b = buffer.iter_at_offset(end as i32);
            buffer.select_range(&a, &b);
            if let Some(view) = self.active_view() {
                let mut iter = a;
                view.scroll_to_iter(&mut iter, 0.1, false, 0.0, 0.0);
            }
        }
        self.find_current = Some(index);
        // Keep the typing loop in the panel: Enter goes to the
        // next match without a mouse trip.
        self.find_entry.grab_focus();
    }

    fn replace_one(&mut self, sender: &ComponentSender<Self>) {
        let Some(current) = self.find_current else {
            if !self.find_hits.is_empty() {
                self.jump_to_hit(0, sender);
            }
            return;
        };
        let Some(hit) = self.find_hits.get(current) else {
            return;
        };
        let (path, start, end, matched) = (
            hit.path.clone(),
            hit.start,
            hit.end,
            hit.matched.clone(),
        );
        if !self.tabs.iter().any(|t| t.path == path) {
            // Open it first; the next press replaces.
            self.jump_to_hit(current, sender);
            return;
        }
        let Ok(searcher) = self.searcher() else { return };
        let Some(buffer) = self
            .tabs
            .iter()
            .find(|t| t.path == path)
            .and_then(|t| t.buffer.clone())
        else {
            return;
        };
        let a = buffer.iter_at_offset(start as i32);
        let b = buffer.iter_at_offset(end as i32);
        let current_text = buffer.text(&a, &b, false).to_string();
        if current_text.replace('\n', " ") != matched {
            // The buffer moved under the results; refresh them.
            self.run_find(sender);
            return;
        }
        let replacement = searcher
            .replace_all(&current_text, &self.replace_entry.text())
            .0;
        let (mut a, mut b) = (a, b);
        buffer.begin_user_action();
        buffer.delete(&mut a, &mut b);
        let mut at = buffer.iter_at_offset(start as i32);
        buffer.insert(&mut at, &replacement);
        buffer.end_user_action();
        self.run_find(sender);
        if !self.find_hits.is_empty() {
            let next = current.min(self.find_hits.len() - 1);
            self.jump_to_hit(next, sender);
        }
    }

    fn replace_all(&mut self, sender: &ComponentSender<Self>) {
        let Ok(searcher) = self.searcher() else { return };
        let replacement = self.replace_entry.text().to_string();
        let mut changes: Vec<(String, String)> = Vec::new();
        let mut total = 0usize;
        for path in self.find_files() {
            let Some(text) = self.file_text(&path) else { continue };
            let (new, n) = searcher.replace_all(&text, &replacement);
            if n > 0 {
                total += n;
                changes.push((path, new));
            }
        }
        if changes.is_empty() {
            self.show_pill("No matches to replace", false, sender);
            return;
        }
        let summary = format!(
            "{total} {} in {} {}",
            if total == 1 { "replacement" } else { "replacements" },
            changes.len(),
            if changes.len() == 1 { "file" } else { "files" }
        );
        self.start_apply("Before Replace all".to_string(), summary, changes, sender);
    }

    fn saved_searches(&self) -> Vec<(String, String)> {
        self.service
            .catalog()
            .get_pref("editor.saved_searches")
            .and_then(|raw| serde_json::from_str(&raw).ok())
            .unwrap_or_default()
    }

    fn store_saved_searches(&self, saved: &[(String, String)]) {
        if let Ok(raw) = serde_json::to_string(saved) {
            self.service.catalog().set_pref("editor.saved_searches", &raw);
        }
    }

    fn toggle_saved_menu(&mut self, sender: &ComponentSender<Self>) {
        if let Some(menu) = self.saved_menu.take() {
            menu.popover().popdown();
            menu.popover().unparent();
            return;
        }
        let menu = crate::widgets::menu::menu_popover(crate::widgets::menu::MenuWidth::Tools);
        for (find, replace) in self.saved_searches() {
            let label = if replace.is_empty() {
                find.clone()
            } else {
                format!("{find} → {replace}")
            };
            let s = sender.input_sender().clone();
            menu.add_item(&label, move || {
                let _ = s.send(EpubEditorMsg::UseSavedSearch(find.clone(), replace.clone()));
            });
        }
        menu.add_separator();
        let s = sender.input_sender().clone();
        menu.add_item("Save this search", move || {
            let _ = s.send(EpubEditorMsg::SaveThisSearch);
        });
        menu.popover().set_parent(&self.find_panel);
        menu.popover().popup();
        self.saved_menu = Some(menu);
    }

    // ── E3: command palette (decision 129) ─────────────────────────

    fn toggle_palette(&mut self, sender: &ComponentSender<Self>) {
        if self.palette_panel.is_visible() {
            self.palette_panel.set_visible(false);
            return;
        }
        self.find_panel.set_visible(false);
        self.palette_panel.set_visible(true);
        self.palette_entry.set_text("");
        self.palette_entry.grab_focus();
        self.rebuild_palette(sender);
    }

    fn palette_commands(&self) -> Vec<PaletteItem> {
        let item = |label: &str, detail: &str, shortcut: Option<&'static str>, action: PaletteAction| PaletteItem {
            label: label.to_string(),
            detail: detail.to_string(),
            shortcut,
            action,
        };
        let mut items = vec![
            item("Save", "", Some("Ctrl+S"), PaletteAction::Save),
            item("Check book", "", None, PaletteAction::CheckBook),
            item("Find & replace", "", Some("Ctrl+F"), PaletteAction::FindReplace),
            item("Smarten punctuation", "Clean up", None, PaletteAction::Tool(InstantTool::Smarten)),
            item("Reformat code", "Clean up", None, PaletteAction::Tool(InstantTool::Reformat)),
            item("Remove unused CSS", "Clean up", None, PaletteAction::Tool(InstantTool::RemoveUnusedCss)),
            item("Fix HTML", "Clean up", None, PaletteAction::Tool(InstantTool::FixHtml)),
            item("Code view", "View", None, PaletteAction::View(ViewMode::Code)),
            item("Split view", "View", None, PaletteAction::View(ViewMode::Split)),
            item("Preview view", "View", None, PaletteAction::View(ViewMode::Preview)),
            item("Preview tab", "Right panel", None, PaletteAction::Tab(RightTab::Preview)),
            item("Outline tab", "Right panel", None, PaletteAction::Tab(RightTab::Outline)),
            item("Problems tab", "Right panel", None, PaletteAction::Tab(RightTab::Problems)),
            item("Live CSS tab", "Right panel", None, PaletteAction::Tab(RightTab::LiveCss)),
        ];
        if let Some(container) = &self.container {
            let mut files: Vec<&EpubFile> = container.files.iter().collect();
            files.sort_by(|a, b| a.path.cmp(&b.path));
            for file in files {
                items.push(item(
                    file_name(&file.path),
                    &file.path,
                    None,
                    PaletteAction::OpenFile(file.path.clone()),
                ));
            }
        }
        items
    }

    fn rebuild_palette(&mut self, sender: &ComponentSender<Self>) {
        let query = self.palette_entry.text().to_string().to_lowercase();
        let mut matched: Vec<PaletteItem> = self
            .palette_commands()
            .into_iter()
            .filter(|item| {
                fuzzy_positions(&item.label, &query).is_some()
                    || item.detail.to_lowercase().contains(&query)
            })
            .collect();
        matched.sort_by_key(|item| !item.label.to_lowercase().starts_with(&query));
        self.palette_items = matched;
        self.palette_selected = 0;
        self.build_palette_rows(sender);
    }

    fn build_palette_rows(&self, sender: &ComponentSender<Self>) {
        clear_box(&self.palette_host);
        if self.palette_items.is_empty() {
            let label = gtk::Label::new(Some("No commands"));
            label.add_css_class("kalam-ui-meta");
            label.set_halign(gtk::Align::Start);
            label.set_margin_all(12);
            self.palette_host.append(&label);
            return;
        }
        let query = self.palette_entry.text().to_string().to_lowercase();
        for (index, item) in self.palette_items.iter().enumerate() {
            let row = gtk::Button::new();
            row.add_css_class("kalam-ed-palette-row");
            if index == self.palette_selected {
                row.add_css_class("selected");
            }
            row.set_size_request(-1, 40);
            row.set_focus_on_click(false);
            let h = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            let label = gtk::Label::new(None);
            let positions = fuzzy_positions(&item.label, &query).unwrap_or_default();
            label.set_markup(&bold_positions(&item.label, &positions));
            label.add_css_class("kalam-ed-palette-label");
            label.set_halign(gtk::Align::Start);
            label.set_hexpand(true);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            h.append(&label);
            if !item.detail.is_empty() {
                let detail = gtk::Label::new(Some(&item.detail));
                detail.add_css_class("kalam-ui-meta");
                detail.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
                h.append(&detail);
            }
            if let Some(shortcut) = item.shortcut {
                let chip = gtk::Label::new(Some(shortcut));
                chip.add_css_class("kalam-ed-keychip");
                chip.set_valign(gtk::Align::Center);
                h.append(&chip);
            }
            row.set_child(Some(&h));
            let s = sender.input_sender().clone();
            row.connect_clicked(move |_| {
                let _ = s.send(EpubEditorMsg::PaletteRun(index));
            });
            self.palette_host.append(&row);
        }
    }

    fn run_palette_action(&mut self, action: PaletteAction, sender: &ComponentSender<Self>) {
        match action {
            PaletteAction::Save => sender.input(EpubEditorMsg::SavePressed),
            PaletteAction::CheckBook => {
                sender.input(EpubEditorMsg::SetRightTab(RightTab::Problems))
            }
            PaletteAction::FindReplace => self.toggle_find(sender),
            PaletteAction::Tool(kind) => self.present_scope_dialog(kind, sender),
            PaletteAction::View(mode) => sender.input(EpubEditorMsg::SetViewMode(mode)),
            PaletteAction::Tab(tab) => sender.input(EpubEditorMsg::SetRightTab(tab)),
            PaletteAction::OpenFile(path) => {
                sender.input(EpubEditorMsg::OpenFile { path, pin: true })
            }
        }
    }

    // ── chrome sync ────────────────────────────────────────────────

    fn sync_chrome(&mut self, sender: &ComponentSender<Self>) {
        let dirty = self.dirty_paths();
        self.save_btn.set_sensitive(!dirty.is_empty() && !self.saving);
        if dirty != self.shown_dirty {
            self.shown_dirty = dirty;
            self.rebuild_tab_strip(sender);
            self.rebuild_tree(sender);
        }
        if self.right_tab == RightTab::Problems {
            self.rebuild_problems(sender);
        }
    }

    fn sync_switches(&self) {
        for (tab, btn) in &self.right_tab_btns {
            if *tab == self.right_tab {
                btn.add_css_class("active");
            } else {
                btn.remove_css_class("active");
            }
        }
        for (mode, btn) in &self.view_switch_btns {
            if *mode == self.view_mode {
                btn.add_css_class("active");
            } else {
                btn.remove_css_class("active");
            }
        }
    }

    fn apply_view_mode(&self) {
        match self.view_mode {
            ViewMode::Code => {
                self.tree_scroller.set_visible(true);
                self.center_box.set_visible(true);
                self.right_panel.set_visible(false);
            }
            ViewMode::Split => {
                self.tree_scroller.set_visible(true);
                self.center_box.set_visible(true);
                self.right_panel.set_visible(true);
                self.right_panel.set_size_request(440, -1);
                self.right_panel.set_hexpand(false);
            }
            ViewMode::Preview => {
                self.tree_scroller.set_visible(false);
                self.center_box.set_visible(false);
                self.right_panel.set_visible(true);
                self.right_panel.set_size_request(-1, -1);
                self.right_panel.set_hexpand(true);
                if self.right_tab != RightTab::Preview {
                    // The Preview switch means the Preview tab too;
                    // other tabs stay one click away in the tab row.
                }
            }
        }
    }
}

// ── helpers ────────────────────────────────────────────────────────

fn scroll_host(child: &impl IsA<gtk::Widget>) -> gtk::ScrolledWindow {
    let scroller = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::External)
        .vexpand(true)
        .hexpand(true)
        .build();
    scroller.set_child(Some(child));
    scroller
}

fn status_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-ed-status-text");
    label.set_valign(gtk::Align::Center);
    label
}

fn meta_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-ui-meta");
    label.set_halign(gtk::Align::Start);
    label
}

fn panel_note(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-ui-meta");
    label.set_halign(gtk::Align::Start);
    label.set_margin_all(20);
    label
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn file_summary(file: &EpubFile, bytes: Option<&[u8]>) -> String {
    let size = bytes.map(|b| b.len()).unwrap_or(0);
    let size = if size >= 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else if size >= 1024 {
        format!("{:.0} KB", size as f64 / 1024.0)
    } else {
        format!("{size} B")
    };
    format!("{} · {} · {}", file.path, size, file.media_type)
}

fn buffer_text(buffer: &sourceview5::Buffer) -> String {
    buffer
        .text(&buffer.start_iter(), &buffer.end_iter(), false)
        .to_string()
}

fn clear_box(host: &gtk::Box) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
}

fn close_slot(slot: &Rc<RefCell<Option<InAppDialog>>>) {
    if let Some(dialog) = slot.borrow().clone() {
        dialog.close();
    }
}

/// Are the characters immediately before `start` and after `end`
/// exactly `open`/`close`?
fn surrounded_by(
    buffer: &sourceview5::Buffer,
    start: &gtk::TextIter,
    end: &gtk::TextIter,
    open: &str,
    close: &str,
) -> bool {
    let mut open_start = *start;
    if !open_start.backward_chars(open.chars().count() as i32) {
        return false;
    }
    let mut close_end = *end;
    if !close_end.forward_chars(close.chars().count() as i32) {
        return false;
    }
    buffer.text(&open_start, start, false) == open && buffer.text(end, &close_end, false) == close
}

/// Visible text of an HTML file with a map back to the source:
/// `map[i]` is the source *char* offset of normalized char `i`.
/// This is the bridge both click-sync directions walk (E2).
struct StripMap {
    text: String,
    map: Vec<usize>,
}

impl StripMap {
    /// Source char span covering a normalized char range.
    fn source_span(&self, norm_start: usize, norm_end: usize) -> (usize, usize) {
        if self.map.is_empty() {
            return (0, 0);
        }
        let start = self.map[norm_start.min(self.map.len() - 1)];
        let end = if norm_end > norm_start && norm_end - 1 < self.map.len() {
            self.map[norm_end - 1] + 1
        } else {
            start
        };
        (start, end.max(start))
    }

    /// How far through the visible text a source char offset sits.
    fn fraction_at(&self, source_char: usize) -> f64 {
        if self.map.is_empty() {
            return 0.0;
        }
        let idx = self.map.partition_point(|m| *m <= source_char);
        idx as f64 / self.map.len() as f64
    }
}

/// Collapse whitespace the way `strip_map` does, for needles.
fn normalize_ws(text: &str) -> String {
    let mut out = String::new();
    let mut pending_space = false;
    for c in text.chars() {
        if c.is_whitespace() || c == '\u{00a0}' {
            pending_space = !out.is_empty();
        } else {
            if pending_space {
                out.push(' ');
                pending_space = false;
            }
            out.push(c);
        }
    }
    out
}

/// Build the visible-text map: tags, comments and head/style/script
/// content removed, common entities decoded, whitespace runs
/// collapsed to one space.
fn strip_map(html: &str) -> StripMap {
    let chars: Vec<char> = html.chars().collect();
    let mut text = String::new();
    let mut map: Vec<usize> = Vec::new();
    let push = |c: char, at: usize, text: &mut String, map: &mut Vec<usize>| {
        if c.is_whitespace() || c == '\u{00a0}' {
            if !map.is_empty() && !text.ends_with(' ') {
                text.push(' ');
                map.push(at);
            }
        } else {
            text.push(c);
            map.push(at);
        }
    };

    let mut i = 0usize;
    let mut skip_depth = 0usize; // inside head/style/script
    while i < chars.len() {
        let c = chars[i];
        if c == '<' {
            // Comment / CDATA / doctype / PI: skip wholesale.
            if chars[i..].starts_with(&['<', '!', '-', '-']) {
                let mut j = i + 4;
                while j + 2 < chars.len() && !(chars[j] == '-' && chars[j + 1] == '-' && chars[j + 2] == '>') {
                    j += 1;
                }
                i = (j + 3).min(chars.len());
                continue;
            }
            let mut j = i + 1;
            let closing = j < chars.len() && chars[j] == '/';
            if closing {
                j += 1;
            }
            let name_start = j;
            while j < chars.len() && (chars[j].is_ascii_alphanumeric() || chars[j] == ':') {
                j += 1;
            }
            let name: String = chars[name_start..j].iter().collect::<String>().to_ascii_lowercase();
            // To the closing >, respecting quoted attributes.
            let mut quote = '\0';
            let mut k = j;
            while k < chars.len() {
                let ch = chars[k];
                if quote != '\0' {
                    if ch == quote {
                        quote = '\0';
                    }
                } else if ch == '"' || ch == '\'' {
                    quote = ch;
                } else if ch == '>' {
                    break;
                }
                k += 1;
            }
            let self_closed = k > j && chars[k - 1] == '/';
            let skippable = matches!(name.as_str(), "head" | "style" | "script");
            if skippable {
                if closing {
                    skip_depth = skip_depth.saturating_sub(1);
                } else if !self_closed {
                    skip_depth += 1;
                }
            } else if skip_depth == 0 && matches!(name.as_str(), "br" | "p" | "div" | "li" | "tr" | "h1" | "h2" | "h3" | "h4" | "h5" | "h6") {
                push(' ', i, &mut text, &mut map);
            }
            i = (k + 1).min(chars.len());
            continue;
        }
        if skip_depth > 0 {
            i += 1;
            continue;
        }
        if c == '&' {
            let mut j = i + 1;
            while j < chars.len() && j - i <= 10 && chars[j] != ';' {
                j += 1;
            }
            if j < chars.len() && chars[j] == ';' && j > i + 1 {
                let entity: String = chars[i + 1..j].iter().collect();
                let decoded = match entity.as_str() {
                    "amp" => Some('&'),
                    "lt" => Some('<'),
                    "gt" => Some('>'),
                    "quot" => Some('"'),
                    "apos" => Some('\''),
                    "nbsp" => Some(' '),
                    "mdash" => Some('\u{2014}'),
                    "ndash" => Some('\u{2013}'),
                    "hellip" => Some('\u{2026}'),
                    "rsquo" => Some('\u{2019}'),
                    "lsquo" => Some('\u{2018}'),
                    "rdquo" => Some('\u{201d}'),
                    "ldquo" => Some('\u{201c}'),
                    _ => entity
                        .strip_prefix("#x")
                        .and_then(|n| u32::from_str_radix(n, 16).ok())
                        .and_then(char::from_u32)
                        .or_else(|| {
                            entity
                                .strip_prefix('#')
                                .and_then(|n| n.parse::<u32>().ok())
                                .and_then(char::from_u32)
                        }),
                };
                if let Some(d) = decoded {
                    push(d, i, &mut text, &mut map);
                    i = j + 1;
                    continue;
                }
            }
        }
        push(c, i, &mut text, &mut map);
        i += 1;
    }
    // A trailing collapsed space carries no position worth keeping.
    if text.ends_with(' ') {
        text.pop();
        map.pop();
    }
    StripMap { text, map }
}

/// The ancestor chain (root first) of the element at `char_offset`
/// in possibly-invalid HTML, for Live CSS. A forgiving tag scan:
/// unclosed elements simply stay open, a close tag pops to its
/// match if there is one.
fn element_chain_at(html: &str, char_offset: usize) -> Vec<crate::epub_css::Elem> {
    let byte_offset = html
        .char_indices()
        .nth(char_offset)
        .map(|(b, _)| b)
        .unwrap_or(html.len());
    let before = &html[..byte_offset];
    let bytes = before.as_bytes();
    let mut stack: Vec<crate::epub_css::Elem> = Vec::new();
    let mut i = 0usize;
    while i < bytes.len() {
        if bytes[i] != b'<' {
            i += 1;
            continue;
        }
        if before[i..].starts_with("<!--") {
            i = before[i + 4..].find("-->").map(|n| i + 4 + n + 3).unwrap_or(bytes.len());
            continue;
        }
        if before[i..].starts_with("<!") || before[i..].starts_with("<?") {
            i = before[i..].find('>').map(|n| i + n + 1).unwrap_or(bytes.len());
            continue;
        }
        let closing = i + 1 < bytes.len() && bytes[i + 1] == b'/';
        let name_start = if closing { i + 2 } else { i + 1 };
        let mut j = name_start;
        while j < bytes.len() && (bytes[j].is_ascii_alphanumeric() || matches!(bytes[j], b':' | b'-')) {
            j += 1;
        }
        if j == name_start {
            i += 1;
            continue;
        }
        let name = before[name_start..j].to_ascii_lowercase();
        // Tag end (or the cursor, if it sits inside the tag).
        let mut k = j;
        let mut quote = 0u8;
        while k < bytes.len() {
            let b = bytes[k];
            if quote != 0 {
                if b == quote {
                    quote = 0;
                }
            } else if b == b'"' || b == b'\'' {
                quote = b;
            } else if b == b'>' {
                break;
            }
            k += 1;
        }
        let complete = k < bytes.len();
        let tag_inner = &before[j..k];
        if closing {
            if let Some(pos) = stack.iter().rposition(|e| e.tag == name) {
                stack.truncate(pos);
            }
        } else {
            let elem = crate::epub_css::elem_from_start_tag(&name, tag_inner);
            let self_closed = tag_inner.trim_end().ends_with('/')
                || matches!(
                    name.as_str(),
                    "area" | "base" | "br" | "col" | "embed" | "hr" | "img" | "input" | "link"
                        | "meta" | "source" | "track" | "wbr"
                );
            if !complete {
                // The cursor is inside this very tag: it is the
                // current element.
                stack.push(elem);
                return stack;
            }
            if self_closed {
                // Just before the cursor and self-contained: not
                // an ancestor of what follows.
            } else {
                stack.push(elem);
            }
        }
        i = (k + 1).min(bytes.len());
    }
    stack
}

/// `<style>` blocks of a document: (css text, 1-based line its
/// content starts on).
fn style_blocks(html: &str) -> Vec<(String, usize)> {
    let mut out = Vec::new();
    let lower = html.to_ascii_lowercase();
    let mut from = 0usize;
    while let Some(rel) = lower[from..].find("<style") {
        let at = from + rel;
        let Some(gt) = html[at..].find('>') else { break };
        let content_start = at + gt + 1;
        let Some(close) = lower[content_start..].find("</style") else {
            break;
        };
        let content_end = content_start + close;
        let line = html[..content_start].bytes().filter(|b| *b == b'\n').count() + 1;
        out.push((html[content_start..content_end].to_string(), line));
        from = content_end;
    }
    out
}

/// Subsequence match for the command palette: the char indices of
/// `needle`'s letters inside `haystack`, case-insensitive.
fn fuzzy_positions(haystack: &str, needle: &str) -> Option<Vec<usize>> {
    if needle.is_empty() {
        return Some(Vec::new());
    }
    let hay: Vec<char> = haystack.chars().collect();
    let mut positions = Vec::new();
    let mut at = 0usize;
    for wanted in needle.chars() {
        let mut found = false;
        while at < hay.len() {
            if hay[at].to_ascii_lowercase() == wanted {
                positions.push(at);
                at += 1;
                found = true;
                break;
            }
            at += 1;
        }
        if !found {
            return None;
        }
    }
    Some(positions)
}

/// Pango markup with the given char positions bolded (§38).
fn bold_positions(text: &str, positions: &[usize]) -> String {
    let mut out = String::new();
    let mut open = false;
    for (index, c) in text.chars().enumerate() {
        let hit = positions.contains(&index);
        if hit && !open {
            out.push_str("<b>");
            open = true;
        } else if !hit && open {
            out.push_str("</b>");
            open = false;
        }
        out.push_str(&gtk::glib::markup_escape_text(&c.to_string()));
    }
    if open {
        out.push_str("</b>");
    }
    out
}

/// Headings in an HTML buffer: (level, visible text, char offset of
/// the opening tag). A plain scan — the outline needs positions in
/// the *edited* text, valid XML or not.
fn extract_headings(html: &str) -> Vec<(usize, String, i32)> {
    let mut out = Vec::new();
    let bytes = html.as_bytes();
    let mut i = 0usize;
    while i + 2 < bytes.len() {
        if bytes[i] == b'<'
            && (bytes[i + 1] == b'h' || bytes[i + 1] == b'H')
            && (b'1'..=b'6').contains(&bytes[i + 2])
        {
            let level = (bytes[i + 2] - b'0') as usize;
            let after = i + 3;
            let boundary_ok = after >= bytes.len()
                || matches!(bytes[after], b'>' | b' ' | b'\t' | b'\n' | b'\r' | b'/');
            if boundary_ok {
                if let Some(gt) = html[after..].find('>') {
                    let text_start = after + gt + 1;
                    let close = format!("</h{level}");
                    let text_end = html[text_start..]
                        .to_ascii_lowercase()
                        .find(&close)
                        .map(|pos| text_start + pos)
                        .unwrap_or(html.len());
                    let text = crate::epub::strip_html(&html[text_start..text_end]);
                    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
                    let char_offset = html[..i].chars().count() as i32;
                    out.push((
                        level,
                        if text.is_empty() {
                            format!("Heading {level}")
                        } else {
                            text
                        },
                        char_offset,
                    ));
                    i = text_end;
                    continue;
                }
            }
        }
        i += 1;
    }
    out
}

/// The token-generated GtkSourceView style scheme (decision 127):
/// tags in `accent`, names and text in `text`, values and comments
/// in `text-2`, comments italic. The service writes the XML from the
/// active palette on a worker (the UI thread never touches disk);
/// this only points the scheme manager at the folder and looks it up.
fn current_style_scheme() -> Option<sourceview5::StyleScheme> {
    let dir = crate::paths::source_scheme_dir();
    let manager = sourceview5::StyleSchemeManager::default();
    if let Some(path) = dir.to_str() {
        manager.prepend_search_path(path);
    }
    manager.force_rescan();
    manager.scheme("kalam")
}
