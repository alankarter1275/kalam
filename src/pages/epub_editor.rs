//! Deep EPUB editor workspace (editor-spec.md, decisions 118–143).
//!
//! A docked, flat-chrome page: top bar (Back pill, title, Save), the
//! file tree, the editing area (formatting toolbar, tabs, source
//! view), the right panel (Preview, Outline, Problems, Live CSS) and
//! the status bar with the Code / Split / Preview switch.
//!
//! This landed in tranches like the readers did. E1 is the shell,
//! source editing and the save path (snapshots, validation, both
//! save dialogs); E2 the engine-backed Preview, Check book and Live
//! CSS; E3 Tools, Find & replace and the command palette; E4 Edit
//! contents, the structure tools, the Insert dialogs and Spell
//! check; E5 opens with History and Compare (decision 134) — dead
//! controls are never drawn (the same rule the book page's ⋯ menu
//! followed).

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

/// A History version opened for Compare: the old book plus the
/// identity History knows it by (decision 134).
type CompareLoad = Result<(EpubContainer, Option<i64>, String, String), String>;

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
    // ── E4: Edit contents (decision 136) ──
    OpenContents,
    TocSelect(usize),
    TocMoveSelection(i32),
    /// +1 indents the selected subtree, −1 outdents it.
    TocIndent(i32),
    TocAddEntry,
    TocGenerate,
    TocRenameStart(usize),
    /// Empty text cancels the rename (names are never blank).
    TocRenameCommit(String),
    TocDelete,
    /// Drag: flattened indices; `after` = drop below the row.
    TocDrop {
        from: usize,
        to: usize,
        after: bool,
    },
    // ── E4: structure tools + file-tree menu (decisions 125, 139) ──
    /// Snapshot taken: mutate the book's structure now.
    ApplyStructure(Box<StructurePlan>),
    /// Preset: 0 = at cursor, 1 = at every heading, 2 = by size.
    OpenSplitDialog(u8),
    RunSplit {
        mode: u8,
        max_kb: usize,
    },
    MergeSelected,
    RenameActive,
    OpenRename(String),
    RunRename {
        from: String,
        to: String,
    },
    OpenReorder,
    RunReorder(Vec<String>),
    OpenSemantics(String),
    RunSemantics {
        path: String,
        value: Option<String>,
    },
    OpenLinkCss(String),
    PalettePickSemantics,
    PalettePickLinkCss,
    RunLinkCss {
        path: String,
        css: Vec<String>,
    },
    TreeMenu(String),
    TreeToggleSelect(String),
    TreeDelete(String),
    TreeDeleteConfirmed(String),
    MarkCover(String),
    TreeAddFiles,
    FilesLoaded(Vec<(String, Vec<u8>)>),
    TreeExport(String),
    TreeReplace(String),
    /// (book path, new bytes, the snapshot's History label).
    ReplaceLoaded(String, Vec<u8>, &'static str),
    TreeOpenExternal(String),
    ExternalReady(std::path::PathBuf),
    // ── E4: Insert dialogs (decision 138) ──
    OpenInsertChar,
    OpenInsertLink,
    OpenInsertImage,
    OpenInsertInlineContents,
    /// Raw text/markup in at the cursor (selection replaced).
    InsertText(String),
    /// Wrap the selection (or drop an empty pair) in a link.
    InsertLink(String),
    /// (Disk file name, bytes, alt text) for a new book image.
    InsertImageLoaded(String, Vec<u8>, String),
    RunInsertInlineContents {
        after: String,
        parts: bool,
        to_spine: bool,
    },
    // ── E4: spell check + personal dictionary (decision 137) ──
    OpenSpellCheck,
    SpellReady(Vec<String>),
    SpellPick(String),
    SpellIgnore,
    SpellAddWord,
    SpellChange,
    OpenPersonalDict,
    PersonalAdd(String),
    PersonalRemove(String),
    // ── E5: History and Compare (decision 134) ──
    OpenHistory,
    HistoryLoaded(Vec<crate::db::EditorVersion>),
    RestoreVersion {
        id: Option<i64>,
        label: String,
    },
    RestoreConfirmed {
        id: Option<i64>,
        label: String,
    },
    VersionRestored(String),
    CompareVersion {
        id: Option<i64>,
        label: String,
        created_at: String,
    },
    CompareLoaded(Box<CompareLoad>),
    CompareBack,
    CompareRestore,
    /// Step between the changed files of a comparison.
    CompareFile(i32),
    // ── E5: Export for editing (decision 140) ──
    TreeExportForEditing(String),
    /// (book path, the copy in ~/Kalam/Editing).
    EditingExported(String, std::path::PathBuf),
    /// Bring the edited copy back into the book.
    EditingImport(String, std::path::PathBuf),
    // ── E5: image and font tabs, Replace cover (decisions 126, 139) ──
    CompressImageTab(String),
    /// (book path, re-encoded bytes, original length).
    ImageCompressed(String, Vec<u8>, usize),
    /// (book path, materialised font file, family name).
    FontPreviewReady(String, std::path::PathBuf, String),
    OpenReplaceCover,
    /// (disk file name, chosen image bytes).
    RunReplaceCover(String, Vec<u8>),
    /// The cover bytes, converted to the standing cover's format.
    CoverPrepared(String, Vec<u8>),
    // ── E5: Reports (decision 141) ──
    OpenReports,
    // ── E5: Tag mapper, Filter style, Upgrade internals ──
    OpenTagMapper,
    RunTagMapper(Vec<crate::epub_tools::TagRule>),
    OpenFilterStyle,
    RunFilterStyle(Vec<String>),
    OpenUpgradeInternals,
    RunUpgradeInternals,
    // ── E5: Fonts and images (decision 141) ──
    /// From the menu (everything ticked) or one font tab.
    OpenSubsetFonts(Option<String>),
    SubsetPreviewsReady(Vec<SubsetPreview>, Option<String>),
    RunSubsetFonts(ByteChanges),
    OpenEmbedFonts,
    EmbedScanReady(Vec<(String, Option<std::path::PathBuf>)>),
    RunEmbedFonts(Vec<(String, std::path::PathBuf)>),
    /// (family, disk file name, bytes) per embedded font.
    EmbedLoaded(Vec<(String, String, Vec<u8>)>),
    OpenCompressImages,
    CompressPreviewReady(CompressPreview),
    RunCompressImages(ByteChanges),
    OpenCheckLinks,
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

/// One structural operation, fully computed before its snapshot
/// is taken (decision 139): text edits ride the buffer/pending
/// rails, container mutations ride beside them, and Save
/// replays the whole shape (added/removed path sets included).
#[derive(Debug)]
pub(crate) struct StructurePlan {
    summary: String,
    snapshot_label: String,
    /// (path, new text) — OPF included where it changes. Paths
    /// are post-rename.
    text_changes: Vec<(String, String)>,
    /// (path, new bytes) for binary files (Replace from file).
    byte_changes: Vec<(String, Vec<u8>)>,
    new_files: Vec<(EpubFile, Vec<u8>)>,
    removed_files: Vec<String>,
    renames: Vec<(String, String)>,
    /// Full new reading order (container paths), when it moves.
    spine_order: Option<Vec<String>>,
    /// Path of the image to wear cover-image (Mark as cover).
    set_cover: Option<String>,
}

impl StructurePlan {
    fn new(summary: String, snapshot_label: &str) -> Self {
        StructurePlan {
            summary,
            snapshot_label: snapshot_label.to_string(),
            text_changes: Vec::new(),
            byte_changes: Vec::new(),
            new_files: Vec::new(),
            removed_files: Vec::new(),
            renames: Vec::new(),
            spine_order: None,
            set_cover: None,
        }
    }

    fn files_changed(&self) -> i64 {
        (self.text_changes.len()
            + self.byte_changes.len()
            + self.new_files.len()
            + self.removed_files.len()
            + self.renames.len()) as i64
    }
}

/// One unknown word Spell check is standing on (decision 137).
#[derive(Debug, Clone)]
struct SpellHit {
    word: String,
    /// Char span in the file's source text.
    src_start: usize,
    src_end: usize,
    /// The visible sentence around the word, for the dialog.
    sentence: String,
    /// Char span of the word inside `sentence`.
    s_start: usize,
    s_end: usize,
}

/// A running spell check over one file's hits.
struct SpellSession {
    file: String,
    hits: Vec<SpellHit>,
    at: usize,
    /// Folded words ignored this session.
    ignored: HashSet<String>,
    picked: Option<String>,
}

/// One changed text file of a comparison (decision 134).
struct CompareEntry {
    path: String,
    kind: FileKind,
    old_text: String,
    new_text: String,
}

/// A running comparison against one History version.
struct CompareState {
    version_id: Option<i64>,
    label: String,
    created_at: String,
    entries: Vec<CompareEntry>,
    at: usize,
}

/// The Compare page's widgets, built once at init.
struct CompareWidgets {
    title: gtk::Label,
    meta: gtk::Label,
    prev_btn: gtk::Button,
    next_btn: gtk::Button,
    left_head: gtk::Label,
    old_buffer: sourceview5::Buffer,
    new_buffer: sourceview5::Buffer,
    old_numbers: sourceview5::Buffer,
    new_numbers: sourceview5::Buffer,
    removed_probe: gtk::Label,
    added_probe: gtk::Label,
    vadjustment: gtk::Adjustment,
}

/// The widgets of an open Spell check dialog the model drives.
struct SpellUi {
    slot: Rc<RefCell<Option<InAppDialog>>>,
    count: gtk::Label,
    sentence: gtk::Label,
    suggestions: gtk::Box,
}

/// One tab's pre-structure state: (path, pinned, (text,
/// modified) for buffered tabs).
type TabSnapshot = (String, bool, Option<(String, bool)>);

/// The working state before a structure run — the pill's Undo
/// for operations buffers alone cannot take back.
struct StructureUndo {
    files: Vec<EpubFile>,
    bytes: HashMap<String, Vec<u8>>,
    pending: HashMap<String, String>,
    added_paths: HashSet<String>,
    removed_paths: HashSet<String>,
    /// (path, pinned, (text, modified) for buffered tabs)
    tabs: Vec<TabSnapshot>,
    active_path: Option<String>,
}

#[derive(Clone)]
enum PaletteAction {
    Save,
    CheckBook,
    FindReplace,
    EditContents,
    SplitCursor,
    SplitDialog(u8),
    MergeSelected,
    RenameActive,
    Reorder,
    Semantics,
    LinkCss,
    AddFiles,
    InsertChar,
    InsertLink,
    InsertImage,
    InsertInlineContents,
    SpellCheck,
    PersonalDict,
    History,
    ReplaceCover,
    Reports,
    TagMapper,
    FilterStyle,
    UpgradeInternals,
    SubsetFonts,
    EmbedFonts,
    CompressImages,
    CheckLinks,
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
    // ── E4: structural state ──
    /// Files created since the last save; Save writes their
    /// current bytes even when no dirty flag names them.
    added_paths: HashSet<String>,
    /// Archive entries to omit at the next save: deletions,
    /// merge-aways and rename sources.
    removed_paths: HashSet<String>,
    // ── E4: Edit contents (decision 136) ──
    toolbar_format: gtk::Box,
    toolbar_contents: gtk::Box,
    contents_host: gtk::Box,
    toc: Vec<crate::epub_structure::TocEntry>,
    /// Flattened-row index of the selected entry.
    toc_selected: Option<usize>,
    toc_editing: Option<usize>,
    /// Hash of the nav+NCX texts the working TOC last came
    /// from (or was written to): a mismatch means the files
    /// were hand-edited and win on re-entry.
    toc_source_hash: u64,
    // ── E4: structure tools + tree menu ──
    /// Ctrl+clicked tree rows, in click order (Merge selected).
    tree_selected: Vec<String>,
    tree_menu: Option<MenuPopover>,
    /// Live tree row widgets by path, for anchoring the
    /// right-click menu. Interior because `rebuild_tree` is
    /// shared-`&self`.
    tree_rows: Rc<RefCell<Vec<(String, gtk::Button)>>>,
    /// Whether Ctrl is currently held (tree click routing).
    ctrl_down: Rc<RefCell<bool>>,
    structure_undo: Option<StructureUndo>,
    // ── E4: Insert dialogs ──
    /// (file, char start, char end) captured when an Insert
    /// dialog opened — where an Add-from-file image lands.
    insert_anchor: Option<(String, usize, usize)>,
    // ── E4: spell check ──
    /// Dictionary headwords, loaded once per editor session.
    spell_dict: Option<Rc<HashSet<String>>>,
    spell: Option<SpellSession>,
    spell_ui: Option<SpellUi>,
    // ── E5: History and Compare ──
    compare_widgets: CompareWidgets,
    compare: Option<CompareState>,
    // ── E5: font specimen tabs ──
    /// (book path, drawing area, colour probe) per open font
    /// tab; interior because tab widgets are built from `&self`.
    font_areas: Rc<RefCell<Vec<(String, gtk::DrawingArea, gtk::Label)>>>,
}

/// (container path, new bytes) pairs — the shape every
/// whole-book binary tool hands the structure rail.
type ByteChanges = Vec<(String, Vec<u8>)>;

/// Compress images measures both of its modes up front:
/// (lossless wins, smaller wins).
type CompressPreview = Box<(ByteChanges, ByteChanges)>;

/// Replace cover's picked file, shared into its chooser reply.
type ChosenBytes = Rc<RefCell<Option<(String, Vec<u8>)>>>;

/// A dialog whose rows rebuild each other keeps this
/// self-reference (Tag mapper's row list).
type SelfSlot = Rc<RefCell<Option<Rc<dyn Fn()>>>>;

/// One font through the subsetter, before the dialog opens:
/// its real sizes come from having actually run the engine.
#[derive(Debug)]
pub(crate) struct SubsetPreview {
    path: String,
    label: String,
    original: usize,
    outcome: Result<Vec<u8>, String>,
}

/// One Reports table (decision 141): headers, rows and the
/// standing sort. `numeric` marks columns that sort by the
/// row's numeric key rather than its text.
struct ReportTable {
    headers: Vec<&'static str>,
    widths: Vec<i32>,
    numeric: Vec<bool>,
    rows: Vec<ReportRow>,
    sort_col: Option<usize>,
    sort_desc: bool,
}

struct ReportRow {
    cells: Vec<String>,
    keys: Vec<i64>,
    /// The file a row click opens, when it names one.
    open: Option<String>,
}

const EMPTY_TAB: &str = "__kalam_empty";
/// The Edit contents surface rides the tab strip like a file,
/// but is not one (decision 136).
const CONTENTS_TAB: &str = "__kalam_contents";

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
        let history_btn = gtk::Button::with_label("History");
        history_btn.add_css_class("kalam-ed-ghost");
        history_btn.set_valign(gtk::Align::Center);
        history_btn.set_margin_end(8);
        history_btn.set_focus_on_click(false);
        {
            let s = sender.input_sender().clone();
            history_btn.connect_clicked(move |_| {
                let _ = s.send(EpubEditorMsg::OpenHistory);
            });
        }
        top_bar.append(&history_btn);
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
        // The 48px toolbar row holds two toolboxes: formatting for
        // source tabs, contents actions for the Contents tab
        // (decision 136 puts them "in the formatting toolbar's
        // position"). `activate_tab` shows one, hides the other.
        let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        toolbar.add_css_class("kalam-ed-toolbar");
        toolbar.set_size_request(-1, 48);
        let toolbar_format = gtk::Box::new(gtk::Orientation::Horizontal, 4);
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
            toolbar_format.append(&btn);
        }
        toolbar.append(&toolbar_format);

        let toolbar_contents = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        {
            let add_btn = gtk::Button::with_label("+ Add entry");
            add_btn.add_css_class("kalam-ed-ghost");
            add_btn.set_valign(gtk::Align::Center);
            add_btn.set_focus_on_click(false);
            let s = sender.clone();
            add_btn.connect_clicked(move |_| s.input(EpubEditorMsg::TocAddEntry));
            toolbar_contents.append(&add_btn);
            let gen_btn = gtk::Button::with_label("Generate from headings");
            gen_btn.add_css_class("kalam-ed-ghost");
            gen_btn.set_valign(gtk::Align::Center);
            gen_btn.set_focus_on_click(false);
            let s = sender.clone();
            gen_btn.connect_clicked(move |_| s.input(EpubEditorMsg::TocGenerate));
            toolbar_contents.append(&gen_btn);
        }
        toolbar_contents.set_visible(false);
        toolbar.append(&toolbar_contents);

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
        let (compare_page, compare_widgets) = build_compare_page(&sender);
        body_stack.add_named(&compare_page, Some("compare"));
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

        // Ctrl+S saves, from anywhere in the workspace. F2
        // renames the open file; Ctrl's press state routes tree
        // clicks (multi-select for Merge).
        let ctrl_down = Rc::new(RefCell::new(false));
        let keys = gtk::EventControllerKey::new();
        {
            let s = sender.clone();
            let ctrl_cell = ctrl_down.clone();
            keys.connect_key_pressed(move |_, keyval, _, state| {
                let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
                let stop = gtk::glib::Propagation::Stop;
                if matches!(keyval, gtk::gdk::Key::Control_L | gtk::gdk::Key::Control_R) {
                    *ctrl_cell.borrow_mut() = true;
                }
                if keyval == gtk::gdk::Key::F2 {
                    s.input(EpubEditorMsg::RenameActive);
                    return stop;
                }
                if keyval == gtk::gdk::Key::F7 {
                    s.input(EpubEditorMsg::OpenSpellCheck);
                    return stop;
                }
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
            let ctrl_cell = ctrl_down.clone();
            keys.connect_key_released(move |_, keyval, _, _| {
                if matches!(keyval, gtk::gdk::Key::Control_L | gtk::gdk::Key::Control_R) {
                    *ctrl_cell.borrow_mut() = false;
                }
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
            added_paths: HashSet::new(),
            removed_paths: HashSet::new(),
            toolbar_format,
            toolbar_contents,
            contents_host: gtk::Box::new(gtk::Orientation::Vertical, 0),
            toc: Vec::new(),
            toc_selected: None,
            toc_editing: None,
            toc_source_hash: 0,
            tree_selected: Vec::new(),
            tree_menu: None,
            tree_rows: Rc::new(RefCell::new(Vec::new())),
            ctrl_down,
            structure_undo: None,
            insert_anchor: None,
            spell_dict: None,
            spell: None,
            spell_ui: None,
            compare_widgets,
            compare: None,
            font_areas: Rc::new(RefCell::new(Vec::new())),
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
                    Ok("editor-contents") => {
                        self.open_contents(&sender);
                    }
                    Ok("editor-split") => {
                        self.present_split_dialog(1, &sender);
                    }
                    Ok("editor-reorder") => {
                        self.present_reorder_dialog(&sender);
                    }
                    Ok("editor-char") => {
                        if self.capture_insert_anchor() {
                            self.present_insert_char_dialog(&sender);
                        }
                    }
                    Ok("editor-spell") => {
                        // Deterministic typos for the camera, and a
                        // stand-in dictionary that knows the rest
                        // of the chapter (CI installs none).
                        if let Some(buffer) = self.active_buffer() {
                            let mut at = buffer.iter_at_offset(0);
                            buffer.insert(&mut at, "Teh quikc ckecking speling. ");
                            let text = buffer_text(&buffer);
                            let stripped = strip_map(&text);
                            let mut dict: HashSet<String> =
                                crate::epub_structure::spell_words(&stripped.text)
                                    .into_iter()
                                    .map(|(_, _, w)| fold_word(&w))
                                    .collect();
                            for typo in ["teh", "quikc", "ckecking", "speling"] {
                                dict.remove(typo);
                            }
                            for good in ["the", "quick", "checking", "spelling"] {
                                dict.insert(good.to_string());
                            }
                            self.spell_dict = Some(Rc::new(dict));
                        }
                        self.start_spell_check(&sender);
                    }
                    Ok("editor-dict") => {
                        self.present_personal_dict_dialog(&sender);
                    }
                    Ok("editor-history") => {
                        // A board-shaped stand-in: CI books have
                        // never been saved from the editor, so the
                        // real history is empty.
                        let now = crate::db::chrono_like_now();
                        let today = now.get(..11).unwrap_or("2026-10-27T");
                        let mk = |id, label: &str, created_at: String, files_changed| {
                            crate::db::EditorVersion {
                                snapshot_id: id,
                                label: label.to_string(),
                                created_at,
                                files_changed,
                                missing: false,
                            }
                        };
                        self.present_history_dialog(
                            vec![
                                mk(
                                    Some(3),
                                    "Before save",
                                    format!("{today}14:32:00Z"),
                                    Some(3),
                                ),
                                mk(
                                    Some(2),
                                    "Before Replace all",
                                    format!("{today}14:10:00Z"),
                                    Some(5),
                                ),
                                mk(
                                    None,
                                    "Original file",
                                    "2026-09-12T09:15:00Z".to_string(),
                                    None,
                                ),
                            ],
                            &sender,
                        );
                    }
                    Ok("editor-compare") => {
                        // Compare the open chapter against a
                        // lightly rewritten twin of itself.
                        let now = crate::db::chrono_like_now();
                        let today = now.get(..11).unwrap_or("2026-10-27T");
                        if let Some(path) = self.active_path.clone() {
                            let kind = self
                                .container
                                .as_ref()
                                .and_then(|c| c.file(&path))
                                .map(|f| f.kind);
                            if let (Some(kind), Some(text)) =
                                (kind, self.file_text(&path))
                            {
                                // Mutate whatever the fixture
                                // offers: reword its longest line
                                // on the old side and drop the
                                // doctype there, so the frame shows
                                // a paired row and an added row.
                                let mut lines: Vec<&str> =
                                    text.split('\n').collect();
                                let mut old_text = text.clone();
                                if let Some(longest) = (0..lines.len())
                                    .max_by_key(|&at| lines[at].len())
                                {
                                    let edited = lines[longest]
                                        .replacen(' ', " (earlier) ", 1);
                                    lines[longest] =
                                        Box::leak(edited.into_boxed_str());
                                    if let Some(doctype) = lines
                                        .iter()
                                        .position(|l| l.contains("DOCTYPE"))
                                    {
                                        lines.remove(doctype);
                                    }
                                    old_text = lines.join("\n");
                                }
                                self.compare = Some(CompareState {
                                    version_id: Some(3),
                                    label: "Before save".to_string(),
                                    created_at: format!("{today}14:32:00Z"),
                                    entries: vec![CompareEntry {
                                        path,
                                        kind,
                                        old_text,
                                        new_text: text,
                                    }],
                                    at: 0,
                                });
                                self.body_stack.set_visible_child_name("compare");
                                self.render_compare();
                            }
                        }
                    }
                    Ok("editor-image") | Ok("editor-font") => {
                        let want = if std::env::var("KALAM_DIALOG").as_deref()
                            == Ok("editor-font")
                        {
                            FileKind::Font
                        } else {
                            FileKind::Image
                        };
                        let target = self.container.as_ref().and_then(|c| {
                            c.files
                                .iter()
                                .find(|f| f.kind == want)
                                .map(|f| f.path.clone())
                        });
                        if let Some(path) = target {
                            self.open_file(&path, true, &sender);
                        }
                    }
                    Ok("editor-cover") => {
                        self.present_replace_cover_dialog(&sender);
                    }
                    Ok("editor-reports") => {
                        // The board's frame stands on Images.
                        self.present_reports_dialog_at(2, &sender);
                    }
                    Ok("editor-tagmapper") => {
                        self.present_tag_mapper_dialog(&sender);
                    }
                    Ok("editor-filterstyle") => {
                        self.present_filter_style_dialog(&sender);
                    }
                    Ok("editor-upgrade") => {
                        self.present_upgrade_dialog(&sender);
                    }
                    Ok("editor-subset") => {
                        // The seeded book embeds the repo's own
                        // DM Mono, so the sizes are the engine's
                        // real work.
                        self.start_subset_fonts(None, &sender);
                    }
                    Ok("editor-embed") => {
                        // The seeded CSS names DejaVu Sans (on
                        // every CI box) and Missing Demo Font.
                        self.start_embed_fonts(&sender);
                    }
                    Ok("editor-compress") => {
                        self.start_compress_images(&sender);
                    }
                    Ok("editor-links") => {
                        // The pre-run state: staging must never
                        // touch the network.
                        self.present_check_links_dialog(&sender);
                    }
                    Ok("editor-editing") => {
                        // Export the first image for outside
                        // editing; the dialog opens on the reply.
                        let image = self.container.as_ref().and_then(|c| {
                            c.files
                                .iter()
                                .find(|f| f.kind == FileKind::Image)
                                .map(|f| f.path.clone())
                        });
                        if let Some(path) = image.or_else(|| self.active_path.clone()) {
                            self.start_export_for_editing(&path, &sender);
                        }
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
                self.added_paths.clear();
                self.removed_paths.clear();
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
                if !self.has_unsaved() {
                    let _ = sender.output(EpubEditorOut::Leave);
                } else {
                    let n = self
                        .dirty_paths()
                        .union(&self.removed_paths)
                        .count();
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
                if self.contents_active() {
                    self.toc_retarget(chapter, fraction, &word, &sender);
                } else {
                    self.preview_word_jump(chapter, fraction, &word, &sender);
                }
            }
            EpubEditorMsg::PreviewTextSelected => {
                if self.contents_active() {
                    self.toc_retarget_selection(&sender);
                } else {
                    self.preview_selection_jump(&sender);
                }
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
            EpubEditorMsg::PillUndo => {
                if self.structure_undo.is_some() {
                    self.undo_structure(&sender);
                } else {
                    self.undo_tool(&sender);
                }
            }
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
            EpubEditorMsg::OpenContents => self.open_contents(&sender),
            EpubEditorMsg::TocSelect(index) => {
                self.toc_selected = Some(index);
                self.rebuild_contents(&sender);
                self.mark_toc_target();
            }
            EpubEditorMsg::TocMoveSelection(delta) => {
                if self.toc_editing.is_some() {
                    return;
                }
                let count = crate::epub_structure::toc_count(&self.toc);
                if count > 0 {
                    let cur = self.toc_selected.unwrap_or(0) as i32;
                    self.toc_selected =
                        Some((cur + delta).clamp(0, count as i32 - 1) as usize);
                    self.rebuild_contents(&sender);
                    self.mark_toc_target();
                }
            }
            EpubEditorMsg::TocIndent(delta) => {
                if self.toc_editing.is_none() {
                    self.toc_indent(delta, &sender);
                }
            }
            EpubEditorMsg::TocAddEntry => self.toc_add_entry(&sender),
            EpubEditorMsg::TocGenerate => self.toc_generate(&sender),
            EpubEditorMsg::TocRenameStart(index) => {
                let index = if index == usize::MAX {
                    self.toc_selected.unwrap_or(0)
                } else {
                    index
                };
                if index < crate::epub_structure::toc_count(&self.toc) {
                    self.toc_selected = Some(index);
                    self.toc_editing = Some(index);
                    self.rebuild_contents(&sender);
                }
            }
            EpubEditorMsg::TocRenameCommit(text) => self.toc_rename_commit(text, &sender),
            EpubEditorMsg::TocDelete => {
                if self.toc_editing.is_none() {
                    self.toc_delete(&sender);
                }
            }
            EpubEditorMsg::TocDrop { from, to, after } => {
                self.toc_drop(from, to, after, &sender)
            }
            // ── E4: structure tools + tree menu ──
            EpubEditorMsg::ApplyStructure(plan) => self.apply_structure(*plan, &sender),
            EpubEditorMsg::OpenSplitDialog(preset) => {
                self.present_split_dialog(preset, &sender)
            }
            EpubEditorMsg::RunSplit { mode, max_kb } => {
                self.run_split(mode, max_kb, &sender)
            }
            EpubEditorMsg::MergeSelected => self.run_merge(&sender),
            EpubEditorMsg::RenameActive => {
                if let Some(path) = self
                    .active_path
                    .clone()
                    .filter(|p| p != CONTENTS_TAB)
                {
                    self.present_rename_dialog(&path, &sender);
                }
            }
            EpubEditorMsg::OpenRename(path) => self.present_rename_dialog(&path, &sender),
            EpubEditorMsg::RunRename { from, to } => self.run_rename(from, to, &sender),
            EpubEditorMsg::OpenReorder => self.present_reorder_dialog(&sender),
            EpubEditorMsg::RunReorder(order) => self.run_reorder(order, &sender),
            EpubEditorMsg::OpenSemantics(path) => {
                self.present_semantics_dialog(&path, &sender)
            }
            EpubEditorMsg::RunSemantics { path, value } => {
                self.run_semantics(path, value, &sender)
            }
            EpubEditorMsg::OpenLinkCss(path) => {
                self.present_link_css_dialog(&path, &sender)
            }
            EpubEditorMsg::RunLinkCss { path, css } => {
                self.run_link_css(path, css, &sender)
            }
            EpubEditorMsg::PalettePickSemantics => {
                if let Some(path) = self.active_path.clone().filter(|p| {
                    self.container
                        .as_ref()
                        .and_then(|c| c.file(p))
                        .map(|f| f.kind == FileKind::Html)
                        .unwrap_or(false)
                }) {
                    self.present_semantics_dialog(&path, &sender);
                } else {
                    self.show_pill("Open a page first", false, &sender);
                }
            }
            EpubEditorMsg::PalettePickLinkCss => {
                if let Some(path) = self.active_path.clone().filter(|p| {
                    self.container
                        .as_ref()
                        .and_then(|c| c.file(p))
                        .map(|f| f.kind == FileKind::Html)
                        .unwrap_or(false)
                }) {
                    self.present_link_css_dialog(&path, &sender);
                } else {
                    self.show_pill("Open a page first", false, &sender);
                }
            }
            EpubEditorMsg::TreeMenu(path) => self.present_tree_menu(&path, &sender),
            EpubEditorMsg::TreeToggleSelect(path) => {
                if let Some(at) = self.tree_selected.iter().position(|p| p == &path) {
                    self.tree_selected.remove(at);
                } else {
                    self.tree_selected.push(path);
                }
                self.rebuild_tree(&sender);
            }
            EpubEditorMsg::TreeDelete(path) => {
                let s = sender.input_sender().clone();
                let name = file_name(&path).to_string();
                let _ = crate::widgets::dialog::confirmation_dialog(
                    &self.root_overlay,
                    &format!("Delete {name}?"),
                    "The file leaves the book when you save. Links pointing \
                     at it will break — Check book will list them.",
                    "Cancel",
                    "Delete",
                    crate::widgets::dialog::ConfirmationKind::Destructive,
                    move || {
                        let _ = s.send(EpubEditorMsg::TreeDeleteConfirmed(path.clone()));
                    },
                );
            }
            EpubEditorMsg::TreeDeleteConfirmed(path) => {
                self.run_tree_delete(path, &sender)
            }
            EpubEditorMsg::MarkCover(path) => self.run_mark_cover(path, &sender),
            EpubEditorMsg::TreeAddFiles => self.pick_add_files(&sender),
            EpubEditorMsg::FilesLoaded(imported) => {
                self.run_files_loaded(imported, &sender)
            }
            EpubEditorMsg::TreeExport(path) => self.pick_export_file(&path, &sender),
            EpubEditorMsg::TreeReplace(path) => self.pick_replace_file(&path, &sender),
            EpubEditorMsg::ReplaceLoaded(path, bytes, label) => {
                self.run_replace_loaded(path, bytes, label, &sender)
            }
            EpubEditorMsg::TreeOpenExternal(path) => self.open_external(&path, &sender),
            // ── E4: Insert dialogs ──
            EpubEditorMsg::OpenInsertChar => {
                if self.capture_insert_anchor() {
                    self.present_insert_char_dialog(&sender);
                } else {
                    self.show_pill("Open a text file first", false, &sender);
                }
            }
            EpubEditorMsg::OpenInsertLink => {
                if self.capture_insert_anchor() {
                    self.present_insert_link_dialog(&sender);
                } else {
                    self.show_pill("Open a text file first", false, &sender);
                }
            }
            EpubEditorMsg::OpenInsertImage => {
                if self.capture_insert_anchor() {
                    self.present_insert_image_dialog(&sender);
                } else {
                    self.show_pill("Open a text file first", false, &sender);
                }
            }
            EpubEditorMsg::OpenInsertInlineContents => {
                self.present_insert_inline_contents_dialog(&sender)
            }
            EpubEditorMsg::InsertText(text) => self.insert_text_at_cursor(&text),
            EpubEditorMsg::InsertLink(href) => self.insert_link_markup(&href),
            EpubEditorMsg::InsertImageLoaded(name, bytes, alt) => {
                self.run_insert_image_loaded(name, bytes, alt, &sender)
            }
            EpubEditorMsg::RunInsertInlineContents {
                after,
                parts,
                to_spine,
            } => self.run_insert_inline_contents(after, parts, to_spine, &sender),
            // ── E4: spell check ──
            EpubEditorMsg::OpenSpellCheck => self.start_spell_check(&sender),
            EpubEditorMsg::SpellReady(words) => {
                if words.is_empty() {
                    self.show_pill(
                        "No dictionary is installed — add one in Settings → Dictionaries",
                        false,
                        &sender,
                    );
                } else {
                    self.spell_dict = Some(Rc::new(words.into_iter().collect()));
                    if let Some(path) = self.active_path.clone() {
                        self.begin_spell(&path, &sender);
                    }
                }
            }
            EpubEditorMsg::SpellPick(word) => {
                if let Some(session) = &mut self.spell {
                    session.picked = Some(word);
                }
                self.spell_render(&sender);
            }
            EpubEditorMsg::SpellIgnore => {
                if let Some(session) = &mut self.spell {
                    if let Some(hit) = session.hits.get(session.at) {
                        let folded = fold_word(&hit.word);
                        session.ignored.insert(folded);
                    }
                }
                self.spell_advance(&sender);
            }
            EpubEditorMsg::SpellAddWord => {
                let word = self
                    .spell
                    .as_ref()
                    .and_then(|s| s.hits.get(s.at))
                    .map(|h| fold_word(&h.word));
                if let Some(word) = word {
                    let mut words = self.personal_words();
                    if !words.contains(&word) {
                        words.push(word.clone());
                        self.save_personal(&words);
                    }
                    if let Some(session) = &mut self.spell {
                        session.ignored.insert(word);
                    }
                }
                self.spell_advance(&sender);
            }
            EpubEditorMsg::SpellChange => self.spell_change(&sender),
            EpubEditorMsg::OpenPersonalDict => {
                self.present_personal_dict_dialog(&sender)
            }
            EpubEditorMsg::PersonalAdd(word) => {
                let mut words = self.personal_words();
                let word = fold_word(word.trim());
                if !word.is_empty() && !words.contains(&word) {
                    words.push(word);
                    self.save_personal(&words);
                }
            }
            EpubEditorMsg::PersonalRemove(word) => {
                let mut words = self.personal_words();
                words.retain(|w| w != &word);
                self.save_personal(&words);
            }
            // ── E5: History and Compare (decision 134) ──
            EpubEditorMsg::OpenHistory => self.open_history(&sender),
            EpubEditorMsg::HistoryLoaded(versions) => {
                self.present_history_dialog(versions, &sender)
            }
            EpubEditorMsg::RestoreVersion { id, label } => {
                if self.has_unsaved() {
                    let s = sender.input_sender().clone();
                    let picked = label.clone();
                    let _ = crate::widgets::dialog::confirmation_dialog(
                        &self.root_overlay,
                        "Discard unsaved edits?",
                        "Restoring replaces the book with this version. \
                         Your unsaved edits would be lost — the current \
                         saved state is kept as a snapshot first.",
                        "Cancel",
                        "Restore this version",
                        crate::widgets::dialog::ConfirmationKind::Destructive,
                        move || {
                            let _ = s.send(EpubEditorMsg::RestoreConfirmed {
                                id,
                                label: picked.clone(),
                            });
                        },
                    );
                } else {
                    sender.input(EpubEditorMsg::RestoreConfirmed { id, label });
                }
            }
            EpubEditorMsg::RestoreConfirmed { id, label } => {
                let Some(book) = self.book.clone() else {
                    return;
                };
                let service = self.service.clone();
                let s = sender.input_sender().clone();
                crate::tasks::spawn_internal(
                    "Restoring an earlier version",
                    move |_reporter| {
                        service
                            .restore_editor_version(&book, id)
                            .map(|_| label)
                    },
                    |_| {},
                    move |result| match result {
                        Ok(label) => {
                            let _ = s.send(EpubEditorMsg::VersionRestored(label));
                        }
                        Err(message) => {
                            let _ = s.send(EpubEditorMsg::ActionFailed(message));
                        }
                    },
                );
            }
            EpubEditorMsg::VersionRestored(label) => {
                let paths: Vec<String> = self.tabs.iter().map(|t| t.path.clone()).collect();
                for path in paths {
                    self.close_tab_quietly(&path);
                }
                self.pending.clear();
                self.added_paths.clear();
                self.removed_paths.clear();
                self.active_path = None;
                self.compare = None;
                self.body_stack.set_visible_child_name("workspace");
                self.request_load(&sender);
                self.show_pill(&format!("Restored “{label}”"), false, &sender);
            }
            EpubEditorMsg::CompareVersion {
                id,
                label,
                created_at,
            } => {
                let Some(book) = self.book.clone() else {
                    return;
                };
                let service = self.service.clone();
                let s = sender.input_sender().clone();
                crate::tasks::spawn_internal(
                    "Opening an earlier version",
                    move |_reporter| {
                        service
                            .open_editor_version(&book, id)
                            .map(|container| (container, id, label, created_at))
                    },
                    |_| {},
                    move |result| {
                        let _ = s.send(EpubEditorMsg::CompareLoaded(Box::new(result)));
                    },
                );
            }
            EpubEditorMsg::CompareLoaded(load) => match *load {
                Ok((old_container, id, label, created_at)) => {
                    let entries = self.build_compare_entries(&old_container);
                    if entries.is_empty() {
                        self.show_pill(
                            "Nothing differs from this version",
                            false,
                            &sender,
                        );
                    } else {
                        self.compare = Some(CompareState {
                            version_id: id,
                            label,
                            created_at,
                            entries,
                            at: 0,
                        });
                        self.body_stack.set_visible_child_name("compare");
                        self.render_compare();
                    }
                }
                Err(message) => {
                    sender.input(EpubEditorMsg::ActionFailed(message));
                }
            },
            EpubEditorMsg::CompareBack => {
                self.compare = None;
                self.body_stack.set_visible_child_name("workspace");
            }
            EpubEditorMsg::CompareRestore => {
                if let Some(state) = &self.compare {
                    sender.input(EpubEditorMsg::RestoreVersion {
                        id: state.version_id,
                        label: state.label.clone(),
                    });
                }
            }
            EpubEditorMsg::CompareFile(delta) => {
                let moved = match &mut self.compare {
                    Some(state) if !state.entries.is_empty() => {
                        let next = (state.at as i32 + delta)
                            .clamp(0, state.entries.len() as i32 - 1) as usize;
                        let changed = next != state.at;
                        state.at = next;
                        changed
                    }
                    _ => false,
                };
                if moved {
                    self.render_compare();
                }
            }
            // ── E5: Export for editing (decision 140) ──
            EpubEditorMsg::TreeExportForEditing(path) => {
                self.start_export_for_editing(&path, &sender)
            }
            EpubEditorMsg::EditingExported(path, dest) => {
                self.present_editing_dialog(&path, dest, &sender)
            }
            EpubEditorMsg::EditingImport(path, dest) => {
                let service = self.service.clone();
                let s = sender.input_sender().clone();
                crate::tasks::spawn_internal(
                    "Reading the edited file",
                    move |_reporter| service.read_editor_import(&dest),
                    |_| {},
                    move |result| match result {
                        Ok(bytes) => {
                            let _ = s.send(EpubEditorMsg::ReplaceLoaded(
                                path,
                                bytes,
                                "Before import edited file",
                            ));
                        }
                        Err(message) => {
                            let _ = s.send(EpubEditorMsg::ActionFailed(message));
                        }
                    },
                );
            }
            // ── E5: image and font tabs, Replace cover ──
            EpubEditorMsg::CompressImageTab(path) => {
                let Some(bytes) = self.file_bytes(&path) else {
                    return;
                };
                let original = bytes.len();
                let service = self.service.clone();
                let s = sender.input_sender().clone();
                crate::tasks::spawn_internal(
                    "Compressing the image",
                    move |_reporter| service.compress_image_lossless(&bytes),
                    |_| {},
                    move |result| match result {
                        Ok(smaller) => {
                            let _ = s.send(EpubEditorMsg::ImageCompressed(
                                path.clone(),
                                smaller,
                                original,
                            ));
                        }
                        Err(message) => {
                            let _ = s.send(EpubEditorMsg::ActionFailed(message));
                        }
                    },
                );
            }
            EpubEditorMsg::ImageCompressed(path, bytes, original) => {
                if bytes.len() < original {
                    let mut plan = StructurePlan::new(
                        format!(
                            "Compressed {}: {} → {}",
                            file_name(&path),
                            human_size(original),
                            human_size(bytes.len())
                        ),
                        "Before compress image",
                    );
                    plan.byte_changes.push((path, bytes));
                    self.finish_structure(plan, &sender);
                } else {
                    self.show_pill(
                        "Already as small as lossless gets",
                        false,
                        &sender,
                    );
                }
            }
            EpubEditorMsg::FontPreviewReady(path, disk, family) => {
                self.font_preview_ready(&path, &disk, &family)
            }
            EpubEditorMsg::OpenReplaceCover => {
                self.present_replace_cover_dialog(&sender)
            }
            EpubEditorMsg::RunReplaceCover(name, bytes) => {
                let cover_ext = self
                    .container
                    .as_ref()
                    .and_then(|c| c.files.iter().find(|f| f.is_cover_image))
                    .and_then(|f| {
                        f.path
                            .rsplit('.')
                            .next()
                            .map(|ext| ext.to_string())
                    });
                let service = self.service.clone();
                let s = sender.input_sender().clone();
                crate::tasks::spawn_internal(
                    "Preparing the new cover",
                    move |_reporter| {
                        service.prepare_cover_image(&bytes, cover_ext.as_deref())
                    },
                    |_| {},
                    move |result| match result {
                        Ok(prepared) => {
                            let _ = s.send(EpubEditorMsg::CoverPrepared(
                                name.clone(),
                                prepared,
                            ));
                        }
                        Err(message) => {
                            let _ = s.send(EpubEditorMsg::ActionFailed(message));
                        }
                    },
                );
            }
            EpubEditorMsg::CoverPrepared(name, bytes) => {
                self.run_replace_cover(name, bytes, &sender)
            }
            EpubEditorMsg::OpenReports => self.present_reports_dialog(&sender),
            EpubEditorMsg::OpenTagMapper => self.present_tag_mapper_dialog(&sender),
            EpubEditorMsg::RunTagMapper(rules) => {
                let (changes, elements) = self.tag_mapper_changes(&rules);
                if changes.is_empty() {
                    self.show_pill("Nothing to change", false, &sender);
                } else {
                    let summary = format!(
                        "Renamed {elements} {} in {} {}",
                        if elements == 1 { "element" } else { "elements" },
                        changes.len(),
                        if changes.len() == 1 { "file" } else { "files" }
                    );
                    self.start_apply(
                        "Before tag mapper".to_string(),
                        summary,
                        changes,
                        &sender,
                    );
                }
            }
            EpubEditorMsg::OpenFilterStyle => {
                self.present_filter_style_dialog(&sender)
            }
            EpubEditorMsg::RunFilterStyle(filter) => {
                let (changes, declarations) = self.filter_style_changes(&filter);
                if changes.is_empty() {
                    self.show_pill("Nothing to change", false, &sender);
                } else {
                    let summary = format!(
                        "Filtered {declarations} {} from {} {}",
                        if declarations == 1 {
                            "declaration"
                        } else {
                            "declarations"
                        },
                        changes.len(),
                        if changes.len() == 1 { "file" } else { "files" }
                    );
                    self.start_apply(
                        "Before filter style information".to_string(),
                        summary,
                        changes,
                        &sender,
                    );
                }
            }
            EpubEditorMsg::OpenUpgradeInternals => {
                self.present_upgrade_dialog(&sender)
            }
            EpubEditorMsg::RunUpgradeInternals => {
                self.run_upgrade_internals(&sender)
            }
            EpubEditorMsg::OpenSubsetFonts(preselect) => {
                self.start_subset_fonts(preselect, &sender)
            }
            EpubEditorMsg::SubsetPreviewsReady(previews, preselect) => {
                self.present_subset_fonts_dialog(previews, preselect, &sender)
            }
            EpubEditorMsg::RunSubsetFonts(chosen) => {
                let before: usize = chosen
                    .iter()
                    .map(|(path, _)| {
                        self.file_bytes(path).map(|b| b.len()).unwrap_or(0)
                    })
                    .sum();
                let after: usize = chosen.iter().map(|(_, b)| b.len()).sum();
                let mut plan = StructurePlan::new(
                    format!(
                        "Subset {} {} · {} → {}",
                        chosen.len(),
                        if chosen.len() == 1 { "font" } else { "fonts" },
                        human_size(before),
                        human_size(after)
                    ),
                    "Before subset fonts",
                );
                plan.byte_changes = chosen;
                self.finish_structure(plan, &sender);
            }
            EpubEditorMsg::OpenEmbedFonts => self.start_embed_fonts(&sender),
            EpubEditorMsg::EmbedScanReady(candidates) => {
                self.present_embed_fonts_dialog(candidates, &sender)
            }
            EpubEditorMsg::RunEmbedFonts(chosen) => {
                let service = self.service.clone();
                let s = sender.input_sender().clone();
                crate::tasks::spawn_internal(
                    "Reading the chosen fonts",
                    move |_reporter| {
                        let mut out = Vec::new();
                        for (family, path) in &chosen {
                            let bytes = service.read_editor_import(path)?;
                            let name = path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "font.ttf".to_string());
                            out.push((family.clone(), name, bytes));
                        }
                        Ok::<_, String>(out)
                    },
                    |_| {},
                    move |result| match result {
                        Ok(loaded) => {
                            let _ = s.send(EpubEditorMsg::EmbedLoaded(loaded));
                        }
                        Err(message) => {
                            let _ = s.send(EpubEditorMsg::ActionFailed(message));
                        }
                    },
                );
            }
            EpubEditorMsg::EmbedLoaded(loaded) => {
                self.run_embed_loaded(loaded, &sender)
            }
            EpubEditorMsg::OpenCompressImages => {
                self.start_compress_images(&sender)
            }
            EpubEditorMsg::CompressPreviewReady(pair) => {
                let (lossless, smaller) = *pair;
                self.present_compress_dialog(lossless, smaller, &sender);
            }
            EpubEditorMsg::RunCompressImages(wins) => {
                let before: usize = wins
                    .iter()
                    .map(|(path, _)| {
                        self.file_bytes(path).map(|b| b.len()).unwrap_or(0)
                    })
                    .sum();
                let after: usize = wins.iter().map(|(_, b)| b.len()).sum();
                let mut plan = StructurePlan::new(
                    format!(
                        "Compressed {} {} · {} → {}",
                        wins.len(),
                        if wins.len() == 1 { "image" } else { "images" },
                        human_size(before),
                        human_size(after)
                    ),
                    "Before compress images",
                );
                plan.byte_changes = wins;
                self.finish_structure(plan, &sender);
            }
            EpubEditorMsg::OpenCheckLinks => {
                self.present_check_links_dialog(&sender)
            }
            EpubEditorMsg::ExternalReady(path) => {
                let file = gtk::gio::File::for_path(&path);
                let _ = gtk::gio::AppInfo::launch_default_for_uri(
                    file.uri().as_str(),
                    None::<&gtk::gio::AppLaunchContext>,
                );
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
        // A second press on a navigation file opens Edit contents
        // instead of pinning its source (decision 136).
        if pin && self.tabs.iter().any(|t| t.path == path) && self.is_navigation_file(path) {
            self.open_contents(sender);
            return;
        }
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
                let bytes = container.bytes(&file.path);
                let mut dims = None;
                if let Some(bytes) = bytes {
                    let glib_bytes = gtk::glib::Bytes::from(bytes);
                    if let Ok(texture) = gtk::gdk::Texture::from_bytes(&glib_bytes) {
                        dims = Some((texture.width(), texture.height()));
                        let picture = gtk::Picture::for_paintable(&texture);
                        picture.set_content_fit(gtk::ContentFit::Contain);
                        picture.set_size_request(-1, 420);
                        host.append(&picture);
                    }
                }
                // Decision 126: dimensions, size, where it is used,
                // and the three things you can do about it.
                let used = self.referencing_files(&file.path);
                let size = human_size(bytes.map(|b| b.len()).unwrap_or(0));
                let usage = match used.len() {
                    0 => "not used in any file".to_string(),
                    1 => "used in 1 file".to_string(),
                    n => format!("used in {n} files"),
                };
                let meta = match dims {
                    Some((w, h)) => format!("{w} x {h} · {size} · {usage}"),
                    None => format!("{size} · {usage}"),
                };
                host.append(&meta_label(&meta));

                let where_host = gtk::Box::new(gtk::Orientation::Vertical, 2);
                where_host.set_visible(false);
                for (used_path, line) in &used {
                    let row = gtk::Button::with_label(&format!(
                        "{} · line {line}",
                        file_name(used_path)
                    ));
                    row.add_css_class("kalam-ed-ghost");
                    row.set_halign(gtk::Align::Start);
                    let s = sender.input_sender().clone();
                    let p = used_path.clone();
                    let line = *line;
                    row.connect_clicked(move |_| {
                        let _ = s.send(EpubEditorMsg::GoTo {
                            path: p.clone(),
                            line: Some(line),
                            offset: None,
                        });
                    });
                    where_host.append(&row);
                }

                let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                {
                    let s = sender.input_sender().clone();
                    let p = file.path.clone();
                    actions.append(&boxed_ghost_button("Replace image…", move || {
                        let _ = s.send(EpubEditorMsg::TreeReplace(p.clone()));
                    }));
                }
                {
                    let s = sender.input_sender().clone();
                    let p = file.path.clone();
                    actions.append(&boxed_ghost_button("Compress", move || {
                        let _ = s.send(EpubEditorMsg::CompressImageTab(p.clone()));
                    }));
                }
                if !used.is_empty() {
                    let toggle = gtk::Button::with_label("Show where used");
                    toggle.add_css_class("kalam-ed-ghost");
                    let host2 = where_host.clone();
                    toggle.connect_clicked(move |_| {
                        host2.set_visible(!host2.is_visible());
                    });
                    actions.append(&toggle);
                }
                host.append(&actions);
                host.append(&where_host);
                (scroll_host(&host).upcast(), None)
            }
            FileKind::Font => {
                // Decision 126: a specimen tab. The preview
                // itself is async (the font materialises through
                // the service); the frame and facts draw now.
                let host = gtk::Box::new(gtk::Orientation::Vertical, 12);
                host.set_margin_all(20);
                let eyebrow = gtk::Label::new(Some("EMBEDDED FONT"));
                eyebrow.add_css_class("kalam-ed-compare-colhead");
                eyebrow.set_halign(gtk::Align::Start);
                host.append(&eyebrow);
                let area = gtk::DrawingArea::new();
                area.set_content_height(196);
                area.set_hexpand(true);
                host.append(&area);
                let probe = crate::widgets::paint::color_probe("kalam-ed-specimen");
                host.append(&probe);
                let used = self.referencing_files(&file.path);
                let size = human_size(
                    container
                        .bytes(&file.path)
                        .map(|b| b.len())
                        .unwrap_or(0),
                );
                let meta = match used.first() {
                    Some((used_path, line)) => format!(
                        "Used by {}, line {line} · {size}",
                        file_name(used_path)
                    ),
                    None => format!("Not used in any file · {size}"),
                };
                host.append(&meta_label(&meta));
                let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                {
                    let s = sender.input_sender().clone();
                    let p = file.path.clone();
                    actions.append(&boxed_ghost_button("Subset font", move || {
                        let _ = s.send(EpubEditorMsg::OpenSubsetFonts(Some(
                            p.clone(),
                        )));
                    }));
                }
                {
                    let remove = gtk::Button::with_label("Remove");
                    remove.add_css_class("kalam-ed-ghost");
                    let s = sender.input_sender().clone();
                    let p = file.path.clone();
                    remove.connect_clicked(move |_| {
                        let _ = s.send(EpubEditorMsg::TreeDelete(p.clone()));
                    });
                    actions.append(&remove);
                }
                host.append(&actions);
                self.font_areas
                    .borrow_mut()
                    .push((file.path.clone(), area, probe));
                if let (Some(book), Some(bytes)) =
                    (self.book.clone(), self.file_bytes(&file.path))
                {
                    let inner = file.path.clone();
                    let reply_path = file.path.clone();
                    let service = self.service.clone();
                    let s = sender.input_sender().clone();
                    crate::tasks::spawn_internal(
                        "Preparing the font specimen",
                        move |_reporter| {
                            service.editor_font_preview(&book, &inner, &bytes)
                        },
                        |_| {},
                        move |result| {
                            if let Ok((disk, family)) = result {
                                let _ = s.send(EpubEditorMsg::FontPreviewReady(
                                    reply_path.clone(),
                                    disk,
                                    family,
                                ));
                            }
                        },
                    );
                }
                (scroll_host(&host).upcast(), None)
            }
            FileKind::Binary => {
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
        let contents = path == CONTENTS_TAB;
        self.toolbar_format.set_visible(!contents);
        self.toolbar_contents.set_visible(contents);
        if contents {
            self.maybe_reload_toc();
            self.rebuild_contents(sender);
            self.mark_toc_target();
        } else {
            if let Some(view) = self.preview_view.as_ref() {
                view.clear_search_matches();
            }
            if let Some(tab) = self.tabs.iter().find(|t| t.path == path) {
                self.status_type.set_label(tab.kind.label());
                if let Some(buffer) = &tab.buffer {
                    let iter = buffer.iter_at_offset(buffer.cursor_position());
                    self.status_pos.set_label(&format!(
                        "Ln {}, Col {}",
                        iter.line() + 1,
                        iter.line_offset() + 1
                    ));
                }
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
        dirty.extend(self.added_paths.iter().cloned());
        dirty.extend(
            self.tabs
                .iter()
                .filter(|t| t.buffer.as_ref().map(|b| b.is_modified()).unwrap_or(false))
                .map(|t| t.path.clone()),
        );
        dirty
    }

    // ── E4: Edit contents (decision 136) ───────────────────────
    //
    // The Contents tab edits the book's navigation as data. Every
    // change writes through to nav.xhtml / toc.ncx as ordinary
    // buffer/pending text, so Save, Check book and the Preview
    // need no special cases; the tab itself is just a view.

    fn is_navigation_file(&self, path: &str) -> bool {
        self.container
            .as_ref()
            .map(|c| {
                c.nav_path.as_deref() == Some(path) || c.ncx_path.as_deref() == Some(path)
            })
            .unwrap_or(false)
    }

    fn contents_active(&self) -> bool {
        self.active_path.as_deref() == Some(CONTENTS_TAB)
    }

    fn toc_dirty(&self) -> bool {
        let Some(container) = &self.container else {
            return false;
        };
        [container.nav_path.as_ref(), container.ncx_path.as_ref()]
            .into_iter()
            .flatten()
            .any(|p| self.is_modified(p))
    }

    fn toc_hash_now(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        if let Some(container) = &self.container {
            for path in [&container.nav_path, &container.ncx_path]
                .into_iter()
                .flatten()
            {
                self.file_text(path).hash(&mut hasher);
            }
        }
        hasher.finish()
    }

    fn load_toc(&mut self) {
        let mut entries = Vec::new();
        if let Some(container) = &self.container {
            if let Some(nav) = &container.nav_path {
                if let Some(text) = self.file_text(nav) {
                    entries = crate::epub_structure::parse_nav(&text, nav);
                }
            }
            if entries.is_empty() {
                if let Some(ncx) = &container.ncx_path {
                    if let Some(text) = self.file_text(ncx) {
                        entries = crate::epub_structure::parse_ncx(&text, ncx);
                    }
                }
            }
        }
        self.toc = entries;
        self.toc_source_hash = self.toc_hash_now();
        self.toc_selected = (!self.toc.is_empty()).then_some(0);
        self.toc_editing = None;
    }

    fn maybe_reload_toc(&mut self) {
        if self.toc_hash_now() != self.toc_source_hash {
            self.load_toc();
        }
    }

    fn open_contents(&mut self, sender: &ComponentSender<Self>) {
        if self.tabs.iter().any(|t| t.path == CONTENTS_TAB) {
            self.activate_tab(CONTENTS_TAB, sender);
            return;
        }
        let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
        panel.add_css_class("kalam-ed-contents");
        let hint = gtk::Label::new(Some(
            "Tab indents · Shift+Tab outdents · drag to reorder · double-click renames",
        ));
        hint.add_css_class("kalam-ui-meta");
        hint.set_halign(gtk::Align::Start);
        hint.set_margin_start(20);
        hint.set_margin_top(12);
        hint.set_margin_bottom(8);
        panel.append(&hint);
        let scroller = scroll_host(&self.contents_host);
        scroller.set_vexpand(true);
        panel.append(&scroller);
        {
            // Contents keys. Bubble phase: a rename Entry inside
            // a row eats its own editing keys first; what it does
            // not want (Tab, arrows, F2) reaches the panel before
            // the window turns them into focus travel.
            let keys = gtk::EventControllerKey::new();
            let s = sender.input_sender().clone();
            keys.connect_key_pressed(move |_, keyval, _, _| {
                let stop = gtk::glib::Propagation::Stop;
                match keyval {
                    gtk::gdk::Key::Tab => {
                        let _ = s.send(EpubEditorMsg::TocIndent(1));
                        stop
                    }
                    gtk::gdk::Key::ISO_Left_Tab => {
                        let _ = s.send(EpubEditorMsg::TocIndent(-1));
                        stop
                    }
                    gtk::gdk::Key::Up => {
                        let _ = s.send(EpubEditorMsg::TocMoveSelection(-1));
                        stop
                    }
                    gtk::gdk::Key::Down => {
                        let _ = s.send(EpubEditorMsg::TocMoveSelection(1));
                        stop
                    }
                    gtk::gdk::Key::Delete | gtk::gdk::Key::BackSpace => {
                        let _ = s.send(EpubEditorMsg::TocDelete);
                        stop
                    }
                    gtk::gdk::Key::F2 => {
                        // usize::MAX = "the selected row".
                        let _ = s.send(EpubEditorMsg::TocRenameStart(usize::MAX));
                        stop
                    }
                    _ => gtk::glib::Propagation::Proceed,
                }
            });
            panel.add_controller(keys);
        }
        self.tab_stack.add_named(&panel, Some(CONTENTS_TAB));
        self.tabs.push(OpenTab {
            path: CONTENTS_TAB.to_string(),
            kind: FileKind::Xml,
            pinned: true,
            buffer: None,
        });
        self.load_toc();
        self.activate_tab(CONTENTS_TAB, sender);
    }

    fn sync_contents_status(&self) {
        let n = crate::epub_structure::toc_count(&self.toc);
        self.status_pos.set_label(&format!(
            "{n} entr{}",
            if n == 1 { "y" } else { "ies" }
        ));
        let kind = match &self.container {
            Some(c) if c.nav_path.is_some() && c.ncx_path.is_some() => "Nav + NCX",
            Some(c) if c.nav_path.is_some() => "Nav",
            Some(c) if c.ncx_path.is_some() => "NCX",
            _ => "Contents",
        };
        self.status_type.set_label(kind);
    }

    fn rebuild_contents(&self, sender: &ComponentSender<Self>) {
        clear_box(&self.contents_host);
        self.sync_contents_status();
        let flat = crate::epub_structure::toc_flatten(self.toc.clone());
        if flat.is_empty() {
            let label = gtk::Label::new(Some(
                "No contents yet — add an entry or generate from headings",
            ));
            label.add_css_class("kalam-ui-meta");
            label.set_halign(gtk::Align::Start);
            label.set_margin_all(20);
            self.contents_host.append(&label);
            return;
        }
        for (index, (depth, entry)) in flat.iter().enumerate() {
            let row = gtk::Button::new();
            row.add_css_class("kalam-ed-toc-row");
            if self.toc_selected == Some(index) {
                row.add_css_class("selected");
            }
            row.set_size_request(-1, 36);
            row.set_focus_on_click(false);

            let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            content.set_margin_start(12 + (*depth as i32) * 24);
            content.set_margin_end(12);
            if self.toc_editing == Some(index) {
                let entry_widget = gtk::Entry::new();
                entry_widget.add_css_class("kalam-ed-entry");
                entry_widget.set_text(&entry.text);
                entry_widget.set_hexpand(true);
                let s = sender.input_sender().clone();
                entry_widget.connect_activate(move |e| {
                    let _ = s.send(EpubEditorMsg::TocRenameCommit(e.text().to_string()));
                });
                let focus = entry_widget.clone();
                gtk::glib::idle_add_local_once(move || {
                    focus.grab_focus();
                    focus.select_region(0, -1);
                });
                content.append(&entry_widget);
            } else {
                let name = gtk::Label::new(Some(&entry.text));
                name.set_halign(gtk::Align::Start);
                name.set_ellipsize(gtk::pango::EllipsizeMode::End);
                if *depth == 0 {
                    name.add_css_class("kalam-ed-toc-part");
                }
                content.append(&name);
                let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                spacer.set_hexpand(true);
                content.append(&spacer);
                if !entry.href.is_empty() {
                    let target = gtk::Label::new(Some(file_name(&entry.href)));
                    target.add_css_class("kalam-ui-meta");
                    content.append(&target);
                }
            }
            row.set_child(Some(&content));

            {
                let s = sender.clone();
                row.connect_clicked(move |_| {
                    s.input(EpubEditorMsg::TocSelect(index));
                });
            }
            {
                // Double-click renames in place.
                let gesture = gtk::GestureClick::new();
                gesture.set_button(1);
                let s = sender.input_sender().clone();
                gesture.connect_pressed(move |_, n, _, _| {
                    if n == 2 {
                        let _ = s.send(EpubEditorMsg::TocRenameStart(index));
                    }
                });
                row.add_controller(gesture);
            }
            {
                let provider = gtk::gdk::ContentProvider::for_value(&(index as i64).to_value());
                let source = gtk::DragSource::new();
                source.set_actions(gtk::gdk::DragAction::MOVE);
                source.set_content(Some(&provider));
                let row_begin = row.clone();
                source.connect_drag_begin(move |source, _| {
                    row_begin.set_opacity(0.4);
                    let paintable = gtk::WidgetPaintable::new(Some(&row_begin));
                    source.set_icon(Some(&paintable), 14, 18);
                });
                let row_end = row.clone();
                source.connect_drag_end(move |_, _, _| row_end.set_opacity(1.0));
                let row_cancel = row.clone();
                source.connect_drag_cancel(move |_, _, _| {
                    row_cancel.set_opacity(1.0);
                    false
                });
                row.add_controller(source);
            }
            {
                let target = gtk::DropTarget::new(
                    i64::static_type(),
                    gtk::gdk::DragAction::MOVE,
                );
                let row_motion = row.clone();
                target.connect_enter(move |_, _, y| {
                    set_toc_drop(&row_motion, y);
                    gtk::gdk::DragAction::MOVE
                });
                let row_motion2 = row.clone();
                target.connect_motion(move |_, _, y| {
                    set_toc_drop(&row_motion2, y);
                    gtk::gdk::DragAction::MOVE
                });
                let row_leave = row.clone();
                target.connect_leave(move |_| {
                    row_leave.remove_css_class("drop-before");
                    row_leave.remove_css_class("drop-after");
                });
                let row_drop = row.clone();
                let s = sender.input_sender().clone();
                target.connect_drop(move |_, value, _, y| {
                    let after = y >= row_drop.height() as f64 / 2.0;
                    row_drop.remove_css_class("drop-before");
                    row_drop.remove_css_class("drop-after");
                    let Ok(from) = value.get::<i64>() else {
                        return false;
                    };
                    let _ = s.send(EpubEditorMsg::TocDrop {
                        from: from as usize,
                        to: index,
                        after,
                    });
                    true
                });
                row.add_controller(target);
            }
            self.contents_host.append(&row);
        }
    }

    /// Rows of the working TOC with depths — every contents edit
    /// happens on this flat form, then unflattens back.
    fn toc_flat(&self) -> Vec<(usize, crate::epub_structure::TocEntry)> {
        crate::epub_structure::toc_flatten(self.toc.clone())
    }

    fn toc_set_flat(&mut self, flat: Vec<(usize, crate::epub_structure::TocEntry)>) {
        self.toc = crate::epub_structure::toc_unflatten(flat);
    }

    /// Length of the subtree run starting at flat index `i`
    /// (the entry plus its deeper followers).
    fn toc_run_len(flat: &[(usize, crate::epub_structure::TocEntry)], i: usize) -> usize {
        let depth = flat[i].0;
        let mut len = 1;
        while i + len < flat.len() && flat[i + len].0 > depth {
            len += 1;
        }
        len
    }

    fn toc_indent(&mut self, delta: i32, sender: &ComponentSender<Self>) {
        let Some(sel) = self.toc_selected else { return };
        let mut flat = self.toc_flat();
        if sel >= flat.len() {
            return;
        }
        let run = Self::toc_run_len(&flat, sel);
        if delta > 0 {
            // Indent: at most one deeper than the previous row.
            if sel == 0 || flat[sel].0 > flat[sel - 1].0 {
                return;
            }
            for row in &mut flat[sel..sel + run] {
                row.0 += 1;
            }
        } else {
            if flat[sel].0 == 0 {
                return;
            }
            for row in &mut flat[sel..sel + run] {
                row.0 -= 1;
            }
        }
        self.toc_set_flat(flat);
        self.toc_changed(sender);
    }

    fn toc_add_entry(&mut self, sender: &ComponentSender<Self>) {
        let mut flat = self.toc_flat();
        let fallback_href = self
            .container
            .as_ref()
            .and_then(|c| {
                c.files
                    .iter()
                    .filter(|f| f.spine_index.is_some() && f.kind == FileKind::Html)
                    .min_by_key(|f| f.spine_index)
                    .map(|f| f.path.clone())
            })
            .unwrap_or_default();
        let (at, depth, href) = match self.toc_selected {
            Some(sel) if sel < flat.len() => (
                sel + Self::toc_run_len(&flat, sel),
                flat[sel].0,
                flat[sel].1.href.clone(),
            ),
            _ => (flat.len(), 0, fallback_href),
        };
        flat.insert(
            at,
            (
                depth,
                crate::epub_structure::TocEntry::leaf("New entry".to_string(), href, None),
            ),
        );
        self.toc_set_flat(flat);
        self.toc_selected = Some(at);
        self.toc_editing = Some(at);
        self.toc_changed(sender);
    }

    fn toc_generate(&mut self, sender: &ComponentSender<Self>) {
        let Some(container) = &self.container else {
            return;
        };
        // Reading order, not manifest order.
        let mut spined: Vec<&EpubFile> = container
            .files
            .iter()
            .filter(|f| f.kind == FileKind::Html && f.spine_index.is_some() && !f.is_nav)
            .collect();
        spined.sort_by_key(|f| f.spine_index);
        let files: Vec<(String, String)> = spined
            .iter()
            .map(|f| (f.path.clone(), self.file_text(&f.path).unwrap_or_default()))
            .collect();
        let generated = crate::epub_structure::generate_from_headings(&files);
        if generated.is_empty() {
            self.show_pill("No headings found in this book", false, sender);
            return;
        }
        self.toc = generated;
        self.toc_selected = Some(0);
        self.toc_editing = None;
        self.toc_changed(sender);
    }

    fn toc_rename_commit(&mut self, text: String, sender: &ComponentSender<Self>) {
        let Some(index) = self.toc_editing.take() else {
            return;
        };
        let text = text.trim().to_string();
        if text.is_empty() {
            self.rebuild_contents(sender);
            return;
        }
        let mut flat = self.toc_flat();
        if let Some((_, entry)) = flat.get_mut(index) {
            entry.text = text;
        }
        self.toc_set_flat(flat);
        self.toc_changed(sender);
    }

    fn toc_delete(&mut self, sender: &ComponentSender<Self>) {
        let Some(sel) = self.toc_selected else { return };
        let mut flat = self.toc_flat();
        if sel >= flat.len() {
            return;
        }
        let run = Self::toc_run_len(&flat, sel);
        flat.drain(sel..sel + run);
        self.toc_set_flat(flat);
        self.toc_selected = if self.toc.is_empty() {
            None
        } else {
            Some(sel.min(crate::epub_structure::toc_count(&self.toc) - 1))
        };
        self.toc_changed(sender);
    }

    fn toc_drop(&mut self, from: usize, to: usize, after: bool, sender: &ComponentSender<Self>) {
        let mut flat = self.toc_flat();
        if from >= flat.len() || to >= flat.len() || from == to {
            return;
        }
        let run = Self::toc_run_len(&flat, from);
        // Dropping into the dragged subtree is meaningless.
        if to > from && to < from + run {
            return;
        }
        let target_depth = flat[to].0;
        let insert_at = if after {
            to + Self::toc_run_len(&flat, to)
        } else {
            to
        };
        let piece: Vec<(usize, crate::epub_structure::TocEntry)> =
            flat.drain(from..from + run).collect();
        let insert_at = if insert_at > from {
            insert_at - run
        } else {
            insert_at
        };
        // The moved run adopts the target row's depth, keeping
        // its internal offsets.
        let shift = target_depth as i32 - piece[0].0 as i32;
        let piece: Vec<(usize, crate::epub_structure::TocEntry)> = piece
            .into_iter()
            .map(|(d, e)| ((d as i32 + shift).max(0) as usize, e))
            .collect();
        let landed = insert_at.min(flat.len());
        for (i, row) in piece.into_iter().enumerate() {
            flat.insert(landed + i, row);
        }
        self.toc_set_flat(flat);
        self.toc_selected = Some(landed);
        self.toc_changed(sender);
    }

    /// Write the working TOC back to nav.xhtml and toc.ncx as
    /// ordinary text edits (buffers where open, pending where
    /// not), then refresh everything that follows text changes.
    fn toc_changed(&mut self, sender: &ComponentSender<Self>) {
        let (nav_path, ncx_path, opf_path, title) = match (&self.container, &self.book) {
            (Some(c), Some(b)) => (
                c.nav_path.clone(),
                c.ncx_path.clone(),
                c.opf_path.clone(),
                b.title.clone(),
            ),
            _ => return,
        };
        if nav_path.is_none() && ncx_path.is_none() && !self.toc.is_empty() {
            // A book with no navigation document at all gains a
            // nav.xhtml, registered in the OPF.
            if let Some(container) = &mut self.container {
                let dir = crate::epub_structure::parent_dir(&opf_path);
                let mut candidate = format!("{dir}nav.xhtml");
                let mut n = 1;
                while container.file(&candidate).is_some() {
                    n += 1;
                    candidate = format!("{dir}nav{n}.xhtml");
                }
                let text = crate::epub_structure::render_nav(None, &self.toc, &candidate);
                container.add_file(
                    EpubFile {
                        path: candidate.clone(),
                        kind: FileKind::Html,
                        media_type: "application/xhtml+xml".to_string(),
                        spine_index: None,
                        is_nav: true,
                        is_cover_image: false,
                        in_manifest: true,
                    },
                    text.into_bytes(),
                );
                self.added_paths.insert(candidate);
            }
            if let Some(opf_text) = self.opf_text_for_current_files() {
                self.set_text_quiet(&opf_path, &opf_text);
            }
            self.rebuild_tree(sender);
        } else {
            if let Some(nav) = &nav_path {
                let current = self.file_text(nav);
                let text =
                    crate::epub_structure::render_nav(current.as_deref(), &self.toc, nav);
                self.set_text_quiet(nav, &text);
            }
            if let Some(ncx) = &ncx_path {
                let current = self.file_text(ncx);
                let text = crate::epub_structure::render_ncx(
                    current.as_deref(),
                    &self.toc,
                    ncx,
                    &title,
                );
                self.set_text_quiet(ncx, &text);
            }
        }
        self.toc_source_hash = self.toc_hash_now();
        self.run_check(sender);
        self.request_preview_write(sender);
        self.sync_chrome(sender);
        self.rebuild_contents(sender);
        self.mark_toc_target();
    }

    /// Replace a file's text without snapshot or pill: the
    /// buffer where one is open (one undo step), a pending edit
    /// where not. Contents writes and OPF rebuilds ride this.
    fn set_text_quiet(&mut self, path: &str, text: &str) {
        if let Some(buffer) = self
            .tabs
            .iter()
            .find(|t| t.path == path)
            .and_then(|t| t.buffer.clone())
        {
            if buffer_text(&buffer) == text {
                return;
            }
            let cursor = buffer.cursor_position();
            buffer.begin_user_action();
            let (mut a, mut b) = (buffer.start_iter(), buffer.end_iter());
            buffer.delete(&mut a, &mut b);
            let mut at = buffer.start_iter();
            buffer.insert(&mut at, text);
            buffer.end_user_action();
            let pos = buffer.iter_at_offset(cursor.min(buffer.char_count()));
            buffer.place_cursor(&pos);
        } else if self.file_text(path).as_deref() != Some(text) {
            self.pending.insert(path.to_string(), text.to_string());
        }
    }

    /// The OPF rewritten to match the container's current file
    /// list, preserving ids, properties and everything outside
    /// manifest/spine. Structure ops (E4) all lean on this.
    fn opf_text_for_current_files(&self) -> Option<String> {
        let container = self.container.as_ref()?;
        let ncx = container.ncx_path.clone();
        self.opf_text_for_files(&container.files, ncx.as_deref(), None)
    }

    /// As above, against a file list a structure plan is
    /// simulating before anything is applied. `base` overrides
    /// the OPF text to preserve (a plan's link-fixed copy, so
    /// guide hrefs keep their retargeting).
    fn opf_text_for_files(
        &self,
        files: &[EpubFile],
        ncx_path: Option<&str>,
        base: Option<&str>,
    ) -> Option<String> {
        let container = self.container.as_ref()?;
        let opf_path = container.opf_path.clone();
        let current = match base {
            Some(text) => text.to_string(),
            None => self.file_text(&opf_path)?,
        };
        let (old_items, old_spine, _) =
            crate::epub_structure::parse_opf_data(&current, &opf_path);
        let old_id_path: HashMap<String, String> = old_items
            .iter()
            .map(|i| (i.id.clone(), i.href.clone()))
            .collect();
        let old_linear: HashMap<String, bool> = old_spine
            .iter()
            .filter_map(|(id, linear)| old_id_path.get(id).map(|p| (p.clone(), *linear)))
            .collect();
        let mut used: HashSet<String> = old_items.iter().map(|i| i.id.clone()).collect();
        let mut items: Vec<crate::epub_structure::OpfItem> = Vec::new();
        let mut id_by_path: HashMap<String, String> = HashMap::new();
        for file in files.iter().filter(|f| f.in_manifest) {
            let old = old_items.iter().find(|i| i.href == file.path);
            let id = match old {
                Some(o) => o.id.clone(),
                None => mint_opf_id(&file.path, &mut used),
            };
            used.insert(id.clone());
            let mut properties: Vec<String> =
                old.map(|o| o.properties.clone()).unwrap_or_default();
            sync_opf_prop(&mut properties, "nav", file.is_nav);
            sync_opf_prop(&mut properties, "cover-image", file.is_cover_image);
            let media_type = if file.media_type.is_empty() {
                crate::epub_container::guess_media_type(&file.path).to_string()
            } else {
                file.media_type.clone()
            };
            id_by_path.insert(file.path.clone(), id.clone());
            items.push(crate::epub_structure::OpfItem {
                id,
                href: file.path.clone(),
                media_type,
                properties,
            });
        }
        let mut spined: Vec<&EpubFile> = files
            .iter()
            .filter(|f| f.in_manifest && f.spine_index.is_some())
            .collect();
        spined.sort_by_key(|f| f.spine_index);
        let spine: Vec<(String, bool)> = spined
            .iter()
            .filter_map(|f| {
                id_by_path.get(&f.path).map(|id| {
                    (
                        id.clone(),
                        old_linear.get(&f.path).copied().unwrap_or(true),
                    )
                })
            })
            .collect();
        let toc_id = ncx_path.and_then(|p| id_by_path.get(p)).cloned();
        Some(crate::epub_structure::rebuild_opf(
            &current,
            &opf_path,
            &items,
            &spine,
            toc_id.as_deref(),
        ))
    }

    /// Ring the selected entry's target heading in the Preview
    /// (the reader's current-search-match accent ring, decision
    /// 136's "2px accent outline"), and take the preview there.
    fn mark_toc_target(&mut self) {
        if !self.contents_active() {
            return;
        }
        let Some(view) = self.preview_view.as_ref() else {
            return;
        };
        let flat = self.toc_flat();
        let Some((_, entry)) = self.toc_selected.and_then(|i| flat.get(i)) else {
            view.clear_search_matches();
            return;
        };
        if entry.href.is_empty() {
            view.clear_search_matches();
            return;
        }
        let spine_index = self
            .container
            .as_ref()
            .and_then(|c| c.file(&entry.href))
            .and_then(|f| f.spine_index);
        let mut marked = false;
        if !entry.text.trim().is_empty() {
            let results = view.search(entry.text.trim());
            let hit = results
                .iter()
                .find(|r| Some(r.chapter) == spine_index)
                .or_else(|| results.first());
            if let Some(hit) = hit {
                let end = kalam_reader::LayeredLocator {
                    char_offset: hit.locator.char_offset
                        + entry.text.trim().chars().count() as u32,
                    ..hit.locator.clone()
                };
                view.set_search_matches([(hit.locator.clone(), end, true)]);
                view.goto_locator(&hit.locator, true);
                self.preview_chapter = hit.chapter as i32;
                marked = true;
            }
        }
        if !marked {
            view.clear_search_matches();
            let href = match &entry.fragment {
                Some(f) => format!("{}#{}", entry.href, f),
                None => entry.href.clone(),
            };
            if !view.follow_link(&href) {
                if let Some(index) = spine_index {
                    view.goto_chapter(index, 0.0);
                    self.preview_chapter = index as i32;
                }
            }
        }
    }

    /// A click in the Preview while Contents is active moves the
    /// selected entry's target there (decision 136): the file
    /// clicked in, fragmented to the nearest heading above the
    /// click that carries an id.
    fn toc_retarget(
        &mut self,
        chapter: usize,
        fraction: f64,
        word: &str,
        sender: &ComponentSender<Self>,
    ) {
        let Some(sel) = self.toc_selected else { return };
        let Some(path) = self.spine_path(chapter) else {
            return;
        };
        let Some(text) = self.file_text(&path) else {
            return;
        };
        let map = strip_map(&text);
        let needle = normalize_ws(word);
        let offset = if needle.is_empty() {
            0
        } else {
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
            best.map(|at| map.source_span(at, at + needle.chars().count()).0)
                .unwrap_or(0)
        };
        let fragment = crate::epub_structure::headings_with_ids(&text)
            .into_iter()
            .rfind(|h| h.2 <= offset)
            .and_then(|h| h.3);
        let mut flat = self.toc_flat();
        if let Some((_, entry)) = flat.get_mut(sel) {
            entry.href = path;
            entry.fragment = fragment;
        }
        self.toc_set_flat(flat);
        self.toc_changed(sender);
    }

    /// Selection form of the same move: retarget to the
    /// selection's chapter and nearest heading id.
    fn toc_retarget_selection(&mut self, sender: &ComponentSender<Self>) {
        let Some(view) = self.preview_view.as_ref() else {
            return;
        };
        let Some(captured) = view.capture_highlight(kalam_reader::HighlightColor::Yellow)
        else {
            return;
        };
        let start = captured.start;
        let exact = start.quote.exact.clone();
        self.toc_retarget(start.spine_index, start.spine_fraction, &exact, sender);
    }

    /// A file's current bytes, for saves and exports: text files
    /// from their buffer/pending/container text, others from
    /// the container's bytes.
    fn file_bytes(&self, path: &str) -> Option<Vec<u8>> {
        let container = self.container.as_ref()?;
        let file = container.file(path)?;
        if file.kind.is_text() {
            self.file_text(path).map(String::into_bytes)
        } else {
            container.bytes(path).map(|b| b.to_vec())
        }
    }

    /// Any unsaved work at all, structural deletions included.
    fn has_unsaved(&self) -> bool {
        !self.dirty_paths().is_empty() || !self.removed_paths.is_empty()
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

    // ── E4: Insert (decision 138) ──────────────────────────────────

    /// Record where an Insert dialog's result should land. False
    /// when no editable text tab is in front.
    fn capture_insert_anchor(&mut self) -> bool {
        let Some(tab) = self
            .active_tab()
            .filter(|t| t.kind.is_text() && t.path != CONTENTS_TAB)
        else {
            return false;
        };
        let Some(buffer) = tab.buffer.clone() else {
            return false;
        };
        let path = tab.path.clone();
        let (start, end) = match buffer.selection_bounds() {
            Some((s, e)) => (s.offset(), e.offset()),
            None => {
                let at = buffer.cursor_position();
                (at, at)
            }
        };
        self.insert_anchor = Some((path, start as usize, end as usize));
        true
    }

    /// Text in at the cursor, replacing any selection — the
    /// buffer-level half of Insert character / image.
    fn insert_text_at_cursor(&mut self, text: &str) {
        let Some(buffer) = self.active_buffer() else {
            return;
        };
        buffer.begin_user_action();
        if let Some((mut start, mut end)) = buffer.selection_bounds() {
            buffer.delete(&mut start, &mut end);
        }
        let mut at = buffer.iter_at_offset(buffer.cursor_position());
        buffer.insert(&mut at, text);
        buffer.end_user_action();
        if let Some(view) = self.active_view() {
            view.grab_focus();
        }
    }

    /// A link around the selection, or an empty pair to type
    /// into (decision 138's Insert link).
    fn insert_link_markup(&mut self, href: &str) {
        let Some(buffer) = self.active_buffer() else {
            return;
        };
        let open = format!("<a href=\"{}\">", esc_attr(href));
        let close = "</a>";
        buffer.begin_user_action();
        if let Some((start, mut end)) = buffer.selection_bounds() {
            buffer.insert(&mut end, close);
            let mut at = start;
            buffer.insert(&mut at, &open);
            let new_start = buffer.iter_at_offset(at.offset());
            let new_end =
                buffer.iter_at_offset(end.offset() + open.chars().count() as i32);
            buffer.select_range(&new_start, &new_end);
        } else {
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

    /// The `<img>` an Insert image choice drops into the text.
    fn img_markup(&self, html_path: &str, image_path: &str, alt: &str) -> String {
        let dir = crate::epub_structure::parent_dir(html_path);
        let src = crate::epub_structure::rel_href(&dir, image_path, None);
        format!(
            "<img alt=\"{}\" src=\"{}\"/>",
            esc_attr(alt),
            esc_attr(&src)
        )
    }

    fn present_insert_char_dialog(&self, sender: &ComponentSender<Self>) {
        let shell = dialog_shell(DialogSize::Form, None);
        let picked: Rc<RefCell<Option<(char, &'static str)>>> =
            Rc::new(RefCell::new(None));
        let search = gtk::Entry::new();
        search.add_css_class("kalam-ed-entry");
        search.set_placeholder_text(Some("Search by name"));
        shell.body().append(&search);
        let grid = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(220);
        scroll.set_child(Some(&grid));
        shell.body().append(&scroll);
        let chosen = gtk::Label::new(Some("Pick a character"));
        chosen.add_css_class("kalam-ui-meta");
        chosen.set_halign(gtk::Align::Start);
        chosen.set_margin_top(8);
        shell.body().append(&chosen);

        let s = sender.input_sender().clone();
        let rebuild = {
            let grid = grid.clone();
            let picked = picked.clone();
            let chosen = chosen.clone();
            let search = search.clone();
            move || {
                clear_box(&grid);
                let query = search.text().trim().to_lowercase();
                let mut row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
                let mut in_row = 0;
                for (glyph, name) in crate::epub_structure::SPECIAL_CHARS {
                    if !query.is_empty() && !name.to_lowercase().contains(&query) {
                        continue;
                    }
                    if in_row == 8 {
                        grid.append(&row);
                        row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
                        in_row = 0;
                    }
                    let btn = gtk::Button::new();
                    btn.add_css_class("kalam-ed-glyph");
                    if picked.borrow().map(|(g, _)| g) == Some(*glyph) {
                        btn.add_css_class("selected");
                    }
                    btn.set_size_request(40, 40);
                    let label = gtk::Label::new(Some(&glyph.to_string()));
                    label.add_css_class("kalam-ed-glyph-label");
                    btn.set_child(Some(&label));
                    btn.set_tooltip_text(Some(name));
                    let picked = picked.clone();
                    let chosen = chosen.clone();
                    let glyph = *glyph;
                    let name = *name;
                    btn.connect_clicked(move |btn| {
                        *picked.borrow_mut() = Some((glyph, name));
                        chosen.set_label(&format!("{name} · U+{:04X}", glyph as u32));
                        if let Some(grid) = btn.parent().and_then(|row| row.parent()) {
                            let mut row = grid.first_child();
                            while let Some(r) = row {
                                let mut cell = r.first_child();
                                while let Some(c) = cell {
                                    c.remove_css_class("selected");
                                    cell = c.next_sibling();
                                }
                                row = r.next_sibling();
                            }
                        }
                        btn.add_css_class("selected");
                    });
                    row.append(&btn);
                    in_row += 1;
                }
                if in_row > 0 {
                    grid.append(&row);
                }
            }
        };
        let rebuild_search = rebuild.clone();
        search.connect_changed(move |_| rebuild_search());
        rebuild();

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let slot_close = slot.clone();
        shell.footer().append(&boxed_ghost_button("Close", move || {
            close_slot(&slot_close);
        }));
        shell.footer().append(&primary_button("Insert", move || {
            if let Some((glyph, _)) = *picked.borrow() {
                let _ = s.send(EpubEditorMsg::InsertText(glyph.to_string()));
            }
        }));
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Insert character",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
            search.grab_focus();
        }
    }

    fn present_insert_link_dialog(&self, sender: &ComponentSender<Self>) {
        let Some((anchor_path, _, _)) = self.insert_anchor.clone() else {
            return;
        };
        let dir = crate::epub_structure::parent_dir(&anchor_path);
        // (label, detail, href, depth) for every target in the book.
        let mut targets: Vec<(String, String, String, i32)> = Vec::new();
        for path in self.spine_order_paths() {
            targets.push((
                file_name(&path).to_string(),
                path.clone(),
                crate::epub_structure::rel_href(&dir, &path, None),
                0,
            ));
            if let Some(text) = self.file_text(&path) {
                for (_level, heading, _off, id) in
                    crate::epub_structure::headings_with_ids(&text)
                {
                    if let Some(id) = id {
                        targets.push((
                            heading,
                            file_name(&path).to_string(),
                            crate::epub_structure::rel_href(&dir, &path, Some(&id)),
                            1,
                        ));
                    }
                }
            }
        }
        let targets = Rc::new(targets);
        let picked: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let picked_btn: Rc<RefCell<Option<gtk::Button>>> = Rc::new(RefCell::new(None));
        let mode = Rc::new(std::cell::Cell::new(0u8));

        let shell = dialog_shell(DialogSize::Form, None);
        let (book_row, book_tick) = option_row("In this book", "");
        book_row.set_size_request(-1, 40);
        let (web_row, web_tick) = option_row("Web address", "");
        web_row.set_size_request(-1, 40);
        shell.body().append(&book_row);
        shell.body().append(&web_row);

        let stack = gtk::Stack::new();
        stack.set_margin_top(8);
        let book_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let search = gtk::Entry::new();
        search.add_css_class("kalam-ed-entry");
        search.set_placeholder_text(Some("Search files and headings"));
        book_page.append(&search);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(220);
        scroll.set_child(Some(&list));
        book_page.append(&scroll);
        stack.add_named(&book_page, Some("book"));
        let web_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
        let url = gtk::Entry::new();
        url.add_css_class("kalam-ed-entry");
        url.set_placeholder_text(Some("https://…"));
        web_page.append(&url);
        stack.add_named(&web_page, Some("web"));
        shell.body().append(&stack);

        let rebuild = {
            let list = list.clone();
            let targets = targets.clone();
            let picked = picked.clone();
            let picked_btn = picked_btn.clone();
            let search = search.clone();
            move || {
                clear_box(&list);
                *picked_btn.borrow_mut() = None;
                let query = search.text().trim().to_lowercase();
                for (label, detail, href, depth) in targets.iter() {
                    if !query.is_empty()
                        && !label.to_lowercase().contains(&query)
                        && !detail.to_lowercase().contains(&query)
                    {
                        continue;
                    }
                    let row = gtk::Button::new();
                    row.add_css_class("kalam-ed-pick-row");
                    if picked.borrow().as_deref() == Some(href.as_str()) {
                        row.add_css_class("selected");
                        *picked_btn.borrow_mut() = Some(row.clone());
                    }
                    row.set_size_request(-1, 36);
                    let h = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                    h.set_margin_start(depth * 18);
                    let name = gtk::Label::new(Some(label));
                    name.add_css_class(if *depth == 0 {
                        "kalam-ed-option-title"
                    } else {
                        "kalam-ed-pick-label"
                    });
                    name.set_halign(gtk::Align::Start);
                    name.set_hexpand(true);
                    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    h.append(&name);
                    let meta = gtk::Label::new(Some(detail));
                    meta.add_css_class("kalam-ui-meta");
                    meta.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
                    h.append(&meta);
                    row.set_child(Some(&h));
                    let picked = picked.clone();
                    let picked_btn = picked_btn.clone();
                    let href = href.clone();
                    row.connect_clicked(move |btn| {
                        if let Some(old) = picked_btn.borrow().as_ref() {
                            old.remove_css_class("selected");
                        }
                        btn.add_css_class("selected");
                        *picked_btn.borrow_mut() = Some(btn.clone());
                        *picked.borrow_mut() = Some(href.clone());
                    });
                    list.append(&row);
                }
            }
        };
        let rebuild_search = rebuild.clone();
        search.connect_changed(move |_| rebuild_search());
        rebuild();

        let sync_mode: Rc<dyn Fn()> = {
            let mode = mode.clone();
            let stack = stack.clone();
            Rc::new(move || {
                set_tick(&book_tick, mode.get() == 0);
                set_tick(&web_tick, mode.get() == 1);
                stack.set_visible_child_name(if mode.get() == 0 { "book" } else { "web" });
            })
        };
        {
            let m = mode.clone();
            let sync = sync_mode.clone();
            book_row.connect_clicked(move |_| {
                m.set(0);
                sync();
            });
            let m = mode.clone();
            let sync = sync_mode.clone();
            web_row.connect_clicked(move |_| {
                m.set(1);
                sync();
            });
        }
        sync_mode();

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let slot_cancel = slot.clone();
        shell.footer().append(&boxed_ghost_button("Cancel", move || {
            close_slot(&slot_cancel);
        }));
        let slot_ok = slot.clone();
        let s = sender.input_sender().clone();
        shell.footer().append(&primary_button("Insert", move || {
            let href = if mode.get() == 0 {
                picked.borrow().clone()
            } else {
                let text = url.text().trim().to_string();
                (!text.is_empty()).then_some(text)
            };
            let Some(href) = href else {
                return;
            };
            close_slot(&slot_ok);
            let _ = s.send(EpubEditorMsg::InsertLink(href));
        }));
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Insert link",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
            search.grab_focus();
        }
    }

    fn present_insert_image_dialog(&self, sender: &ComponentSender<Self>) {
        let (Some(container), Some((anchor_path, _, _))) =
            (&self.container, self.insert_anchor.clone())
        else {
            return;
        };
        let images: Vec<String> = container
            .files
            .iter()
            .filter(|f| f.kind == FileKind::Image)
            .map(|f| f.path.clone())
            .collect();
        let picked: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(None));
        let picked_btn: Rc<RefCell<Option<gtk::Button>>> = Rc::new(RefCell::new(None));

        let shell = dialog_shell(
            DialogSize::Form,
            Some("Choose from the book or add one"),
        );
        let grid = gtk::Box::new(gtk::Orientation::Vertical, 6);
        let mut row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let mut in_row = 0;
        for path in &images {
            if in_row == 4 {
                grid.append(&row);
                row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
                in_row = 0;
            }
            let btn = gtk::Button::new();
            btn.add_css_class("kalam-ed-thumb");
            btn.set_size_request(84, 84);
            btn.set_tooltip_text(Some(path));
            if let Some(bytes) = self.file_bytes(path) {
                let glib_bytes = gtk::glib::Bytes::from(bytes.as_slice());
                if let Ok(texture) = gtk::gdk::Texture::from_bytes(&glib_bytes) {
                    let picture = gtk::Picture::for_paintable(&texture);
                    picture.set_content_fit(gtk::ContentFit::Contain);
                    picture.set_size_request(76, 76);
                    btn.set_child(Some(&picture));
                }
            }
            let picked = picked.clone();
            let picked_btn = picked_btn.clone();
            let path = path.clone();
            btn.connect_clicked(move |btn| {
                if let Some(old) = picked_btn.borrow().as_ref() {
                    old.remove_css_class("selected");
                }
                btn.add_css_class("selected");
                *picked_btn.borrow_mut() = Some(btn.clone());
                *picked.borrow_mut() = Some(path.clone());
            });
            row.append(&btn);
            in_row += 1;
        }
        if in_row > 0 || images.is_empty() {
            grid.append(&row);
        }
        if images.is_empty() {
            let label = gtk::Label::new(Some("No images in this book yet"));
            label.add_css_class("kalam-ui-meta");
            label.set_halign(gtk::Align::Start);
            grid.append(&label);
        }
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(190);
        scroll.set_child(Some(&grid));
        shell.body().append(&scroll);

        let alt = gtk::Entry::new();
        alt.add_css_class("kalam-ed-entry");
        alt.set_placeholder_text(Some("Alt text — what the image shows"));
        alt.set_margin_top(8);
        shell.body().append(&alt);

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        // Add from file… sits at the footer's leading edge.
        let slot_add = slot.clone();
        let s_add = sender.input_sender().clone();
        let alt_add = alt.clone();
        let service = self.service.clone();
        let window = self.host_window();
        let add_btn = boxed_ghost_button("Add from file…", move || {
            let dialog = gtk::FileDialog::builder()
                .title("Add an image")
                .modal(true)
                .build();
            let filter = gtk::FileFilter::new();
            filter.set_name(Some("Images"));
            for suffix in ["png", "jpg", "jpeg", "gif", "webp", "svg"] {
                filter.add_suffix(suffix);
            }
            let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
            filters.append(&filter);
            dialog.set_filters(Some(&filters));
            let service = service.clone();
            let s = s_add.clone();
            let slot = slot_add.clone();
            let alt = alt_add.clone();
            dialog.open(window.as_ref(), gtk::gio::Cancellable::NONE, move |result| {
                let Ok(file) = result else {
                    return;
                };
                let Some(path) = file.path() else {
                    return;
                };
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| "image".to_string());
                let alt_text = alt.text().to_string();
                close_slot(&slot);
                let s2 = s.clone();
                crate::tasks::spawn_internal(
                    "Reading image",
                    move |_reporter| service.read_editor_import(&path),
                    |_| {},
                    move |result| match result {
                        Ok(bytes) => {
                            let _ = s2.send(EpubEditorMsg::InsertImageLoaded(
                                name.clone(),
                                bytes,
                                alt_text.clone(),
                            ));
                        }
                        Err(message) => {
                            let _ = s2.send(EpubEditorMsg::ActionFailed(message));
                        }
                    },
                );
            });
        });
        add_btn.set_hexpand(true);
        add_btn.set_halign(gtk::Align::Start);
        shell.footer().append(&add_btn);
        let slot_cancel = slot.clone();
        shell.footer().append(&boxed_ghost_button("Cancel", move || {
            close_slot(&slot_cancel);
        }));
        let slot_ok = slot.clone();
        let s = sender.input_sender().clone();
        let dir = crate::epub_structure::parent_dir(&anchor_path);
        shell.footer().append(&primary_button("Insert", move || {
            let Some(path) = picked.borrow().clone() else {
                return;
            };
            let src = crate::epub_structure::rel_href(&dir, &path, None);
            let markup = format!(
                "<img alt=\"{}\" src=\"{}\"/>",
                esc_attr(alt.text().trim()),
                esc_attr(&src)
            );
            close_slot(&slot_ok);
            let _ = s.send(EpubEditorMsg::InsertText(markup));
        }));
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Insert image",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    /// An image picked from disk joins the book (snapshot
    /// first, like every structural change) and its `<img>`
    /// lands where the dialog was opened.
    fn run_insert_image_loaded(
        &mut self,
        name: String,
        bytes: Vec<u8>,
        alt: String,
        sender: &ComponentSender<Self>,
    ) {
        let (Some(container), Some((anchor, start, end))) =
            (&self.container, self.insert_anchor.clone())
        else {
            return;
        };
        let bare = file_name(&name).to_string();
        let mut plan = StructurePlan::new(
            format!("Inserted {bare}"),
            "Before insert image",
        );
        let image_path = if let Some(existing) = container
            .files
            .iter()
            .find(|f| file_name(&f.path) == bare)
            .map(|f| f.path.clone())
        {
            plan.byte_changes.push((existing.clone(), bytes));
            existing
        } else {
            let dir = container
                .files
                .iter()
                .find(|f| f.kind == FileKind::Image)
                .map(|f| crate::epub_structure::parent_dir(&f.path))
                .unwrap_or_else(|| {
                    crate::epub_structure::parent_dir(&container.opf_path)
                });
            let path = format!("{dir}{bare}");
            let media = crate::epub_container::guess_media_type(&bare);
            plan.new_files.push((
                EpubFile {
                    path: path.clone(),
                    kind: FileKind::Image,
                    media_type: media.to_string(),
                    spine_index: None,
                    is_nav: false,
                    is_cover_image: false,
                    in_manifest: true,
                },
                bytes,
            ));
            path
        };
        let Some(text) = self.file_text(&anchor) else {
            return;
        };
        let markup = self.img_markup(&anchor, &image_path, &alt);
        let chars: Vec<char> = text.chars().collect();
        let start = start.min(chars.len());
        let end = end.clamp(start, chars.len());
        let mut next: String = chars[..start].iter().collect();
        next.push_str(&markup);
        next.extend(&chars[end..]);
        // The markup rides in as a text change on the anchor —
        // unless the anchor is the OPF, which finish_structure
        // is about to rebuild from this very change as its base.
        plan.text_changes.push((anchor, next));
        self.finish_structure(plan, sender);
    }

    fn present_insert_inline_contents_dialog(&self, sender: &ComponentSender<Self>) {
        let order = self.spine_order_paths();
        let Some(first) = order.first().cloned() else {
            return;
        };
        let after: Rc<RefCell<String>> = Rc::new(RefCell::new(first));
        let after_btn: Rc<RefCell<Option<gtk::Button>>> = Rc::new(RefCell::new(None));
        let parts = Rc::new(std::cell::Cell::new(true));
        let to_spine = Rc::new(std::cell::Cell::new(true));

        let shell = dialog_shell(
            DialogSize::Form,
            Some("A chapter inside the book that lists its chapters"),
        );
        let after_label = gtk::Label::new(Some("Insert after"));
        after_label.add_css_class("kalam-ed-option-title");
        after_label.set_halign(gtk::Align::Start);
        shell.body().append(&after_label);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        for path in &order {
            let row = gtk::Button::new();
            row.add_css_class("kalam-ed-pick-row");
            if after.borrow().as_str() == path.as_str() {
                row.add_css_class("selected");
                *after_btn.borrow_mut() = Some(row.clone());
            }
            row.set_size_request(-1, 36);
            let name = gtk::Label::new(Some(file_name(path)));
            name.add_css_class("kalam-ed-pick-label");
            name.set_halign(gtk::Align::Start);
            row.set_child(Some(&name));
            let after = after.clone();
            let slot = after_btn.clone();
            let path = path.clone();
            row.connect_clicked(move |btn| {
                if let Some(old) = slot.borrow().as_ref() {
                    old.remove_css_class("selected");
                }
                btn.add_css_class("selected");
                *slot.borrow_mut() = Some(btn.clone());
                *after.borrow_mut() = path.clone();
            });
            list.append(&row);
        }
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(180);
        scroll.set_child(Some(&list));
        shell.body().append(&scroll);

        let (parts_row, parts_tick) = option_row("Parts and chapters", "");
        parts_row.set_size_request(-1, 40);
        let (chapters_row, chapters_tick) = option_row("Chapters only", "");
        chapters_row.set_size_request(-1, 40);
        let sync_parts: Rc<dyn Fn()> = {
            let parts = parts.clone();
            Rc::new(move || {
                set_tick(&parts_tick, parts.get());
                set_tick(&chapters_tick, !parts.get());
            })
        };
        {
            let p = parts.clone();
            let sync = sync_parts.clone();
            parts_row.connect_clicked(move |_| {
                p.set(true);
                sync();
            });
            let p = parts.clone();
            let sync = sync_parts.clone();
            chapters_row.connect_clicked(move |_| {
                p.set(false);
                sync();
            });
        }
        sync_parts();
        shell.body().append(&parts_row);
        shell.body().append(&chapters_row);

        let (spine_row, spine_tick) = option_row("Add to the reading order", "");
        spine_row.set_size_request(-1, 40);
        set_tick(&spine_tick, true);
        let spine_cell = to_spine.clone();
        spine_row.connect_clicked(move |_| {
            spine_cell.set(!spine_cell.get());
            set_tick(&spine_tick, spine_cell.get());
        });
        shell.body().append(&spine_row);

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let slot_cancel = slot.clone();
        shell.footer().append(&boxed_ghost_button("Cancel", move || {
            close_slot(&slot_cancel);
        }));
        let slot_ok = slot.clone();
        let s = sender.input_sender().clone();
        shell.footer().append(&primary_button("Insert", move || {
            close_slot(&slot_ok);
            let _ = s.send(EpubEditorMsg::RunInsertInlineContents {
                after: after.borrow().clone(),
                parts: parts.get(),
                to_spine: to_spine.get(),
            });
        }));
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Insert contents page",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    fn run_insert_inline_contents(
        &mut self,
        after: String,
        parts: bool,
        to_spine: bool,
        sender: &ComponentSender<Self>,
    ) {
        if self.container.is_none() {
            return;
        }
        if self.toc.is_empty() {
            self.load_toc();
        }
        if self.toc.is_empty() {
            self.show_pill(
                "This book has no contents to list yet — use Edit contents first",
                false,
                sender,
            );
            return;
        }
        let dir = crate::epub_structure::parent_dir(&after);
        let taken: HashSet<String> = self
            .container
            .as_ref()
            .map(|c| c.files.iter().map(|f| f.path.clone()).collect())
            .unwrap_or_default();
        let mut path = format!("{dir}contents.xhtml");
        let mut n = 2;
        while taken.contains(path.as_str()) {
            path = format!("{dir}contents{n}.xhtml");
            n += 1;
        }
        let text = crate::epub_structure::inline_contents_page(&self.toc, &path, parts);
        let mut plan = StructurePlan::new(
            format!("Inserted {}", file_name(&path)),
            "Before insert contents page",
        );
        plan.new_files.push((
            EpubFile {
                path: path.clone(),
                kind: FileKind::Html,
                media_type: "application/xhtml+xml".to_string(),
                spine_index: None,
                is_nav: false,
                is_cover_image: false,
                in_manifest: true,
            },
            text.into_bytes(),
        ));
        if to_spine {
            let mut order = self.spine_order_paths();
            let at = order
                .iter()
                .position(|p| p == &after)
                .map(|i| i + 1)
                .unwrap_or(order.len());
            order.insert(at, path.clone());
            plan.spine_order = Some(order);
            plan.new_files[0].0.spine_index = Some(at);
        }
        self.finish_structure(plan, sender);
    }

    // ── E4: spell check + personal dictionary (decision 137) ──────

    fn personal_words(&self) -> Vec<String> {
        self.service
            .catalog()
            .get_pref("editor.personal_dictionary")
            .and_then(|raw| serde_json::from_str::<Vec<String>>(&raw).ok())
            .unwrap_or_default()
    }

    fn save_personal(&self, words: &[String]) {
        let mut words: Vec<String> = words.to_vec();
        words.sort();
        words.dedup();
        if let Ok(raw) = serde_json::to_string(&words) {
            self.service
                .catalog()
                .set_pref("editor.personal_dictionary", &raw);
        }
    }

    /// F7: load the word list once, then walk the open file.
    fn start_spell_check(&mut self, sender: &ComponentSender<Self>) {
        let Some(tab) = self
            .active_tab()
            .filter(|t| t.kind == FileKind::Html && t.buffer.is_some())
        else {
            self.show_pill("Open a page to spell-check it", false, sender);
            return;
        };
        let path = tab.path.clone();
        if self.spell_dict.is_some() {
            self.begin_spell(&path, sender);
            return;
        }
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Loading the dictionary",
            move |_reporter| service.spell_wordlist(),
            |_| {},
            move |result| match result {
                Ok(words) => {
                    let _ = s.send(EpubEditorMsg::SpellReady(words));
                }
                Err(message) => {
                    let _ = s.send(EpubEditorMsg::ActionFailed(message));
                }
            },
        );
    }

    /// Unknown words in a file's visible text, with source spans.
    fn spell_hits(
        &self,
        text: &str,
        dict: &HashSet<String>,
        personal: &HashSet<String>,
    ) -> Vec<SpellHit> {
        let stripped = strip_map(text);
        let src_chars: Vec<char> = text.chars().collect();
        let plain: Vec<char> = stripped.text.chars().collect();
        let mut hits = Vec::new();
        for (start, end, word) in crate::epub_structure::spell_words(&stripped.text) {
            if crate::epub_structure::word_known(&word, dict)
                || personal.contains(&fold_word(&word))
            {
                continue;
            }
            let (src_start, src_end) = stripped.source_span(start, end);
            // A word split by markup is not ours to rewrite.
            if src_chars[src_start..src_end.min(src_chars.len())].contains(&'<')
            {
                continue;
            }
            let mut s_from = start;
            while s_from > 0 {
                let c = plain[s_from - 1];
                if matches!(c, '.' | '!' | '?' | '\n') || start - s_from > 160 {
                    break;
                }
                s_from -= 1;
            }
            while s_from < start && plain[s_from] == ' ' {
                s_from += 1;
            }
            let mut s_to = end;
            while s_to < plain.len() {
                let c = plain[s_to];
                s_to += 1;
                if matches!(c, '.' | '!' | '?' | '\n') || s_to - end > 160 {
                    break;
                }
            }
            let sentence: String = plain[s_from..s_to].iter().collect();
            hits.push(SpellHit {
                word,
                src_start,
                src_end,
                sentence,
                s_start: start - s_from,
                s_end: end - s_from,
            });
        }
        hits
    }

    fn begin_spell(&mut self, path: &str, sender: &ComponentSender<Self>) {
        let Some(dict) = self.spell_dict.clone() else {
            return;
        };
        let personal: HashSet<String> =
            self.personal_words().iter().map(|w| fold_word(w)).collect();
        let Some(text) = self.file_text(path) else {
            return;
        };
        let hits = self.spell_hits(&text, &dict, &personal);
        if hits.is_empty() {
            self.show_pill(
                &format!("No unknown words in {}", file_name(path)),
                false,
                sender,
            );
            return;
        }
        self.spell = Some(SpellSession {
            file: path.to_string(),
            hits,
            at: 0,
            ignored: HashSet::new(),
            picked: None,
        });
        self.present_spell_dialog(sender);
        self.spell_render(sender);
        self.spell_reveal();
    }

    fn present_spell_dialog(&mut self, sender: &ComponentSender<Self>) {
        let shell = dialog_shell(DialogSize::Form, None);
        let count = gtk::Label::new(None);
        count.add_css_class("kalam-ui-meta");
        count.set_halign(gtk::Align::Start);
        shell.body().append(&count);
        let sentence = gtk::Label::new(None);
        sentence.add_css_class("kalam-ed-spell-sentence");
        sentence.set_halign(gtk::Align::Start);
        sentence.set_xalign(0.0);
        sentence.set_wrap(true);
        sentence.set_margin_top(8);
        shell.body().append(&sentence);
        let suggestions = gtk::Box::new(gtk::Orientation::Vertical, 2);
        suggestions.set_margin_top(8);
        shell.body().append(&suggestions);

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let s = sender.input_sender().clone();
        let add = boxed_ghost_button("Add to dictionary", move || {
            let _ = s.send(EpubEditorMsg::SpellAddWord);
        });
        add.add_css_class("kalam-ed-accent-text");
        shell.footer().append(&add);
        let s = sender.input_sender().clone();
        shell.footer().append(&boxed_ghost_button("Ignore", move || {
            let _ = s.send(EpubEditorMsg::SpellIgnore);
        }));
        let s = sender.input_sender().clone();
        shell.footer().append(&primary_button("Change", move || {
            let _ = s.send(EpubEditorMsg::SpellChange);
        }));
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Spell check",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
        self.spell_ui = Some(SpellUi {
            slot,
            count,
            sentence,
            suggestions,
        });
    }

    fn spell_render(&mut self, sender: &ComponentSender<Self>) {
        let (Some(session), Some(ui), Some(dict)) = (
            &mut self.spell,
            &self.spell_ui,
            self.spell_dict.clone(),
        ) else {
            return;
        };
        let Some(hit) = session.hits.get(session.at).cloned() else {
            return;
        };
        let remaining = session
            .hits
            .iter()
            .skip(session.at)
            .filter(|h| !session.ignored.contains(&fold_word(&h.word)))
            .count();
        ui.count.set_label(&format!(
            "{} · {} of {}",
            file_name(&session.file),
            session.at + 1,
            session.at + remaining
        ));
        let accent = crate::theme::current(self.service.catalog()).accent;
        let chars: Vec<char> = hit.sentence.chars().collect();
        let esc = |s: &str| {
            s.replace('&', "&amp;")
                .replace('<', "&lt;")
                .replace('>', "&gt;")
        };
        let before: String = chars[..hit.s_start].iter().collect();
        let word: String = chars[hit.s_start..hit.s_end].iter().collect();
        let after: String = chars[hit.s_end..].iter().collect();
        ui.sentence.set_markup(&format!(
            "{}<span underline=\"error\" underline_color=\"{}\">{}</span>{}",
            esc(&before),
            accent,
            esc(&word),
            esc(&after)
        ));
        clear_box(&ui.suggestions);
        let mut options = crate::epub_structure::suggestions(&hit.word, &dict);
        // Match the book's capitalisation of the first letter.
        if hit.word.chars().next().map(|c| c.is_uppercase()).unwrap_or(false) {
            for option in &mut options {
                let mut cs = option.chars();
                if let Some(first) = cs.next() {
                    *option = first.to_uppercase().collect::<String>() + cs.as_str();
                }
            }
        }
        if session
            .picked
            .as_ref()
            .map(|p| !options.contains(p))
            .unwrap_or(true)
        {
            session.picked = options.first().cloned();
        }
        if options.is_empty() {
            let label = gtk::Label::new(Some("No suggestions"));
            label.add_css_class("kalam-ui-meta");
            label.set_halign(gtk::Align::Start);
            ui.suggestions.append(&label);
        }
        for option in options {
            let (row, tick) = option_row(&option, "");
            row.set_size_request(-1, 36);
            set_tick(&tick, session.picked.as_deref() == Some(option.as_str()));
            let s = sender.input_sender().clone();
            row.connect_clicked(move |_| {
                let _ = s.send(EpubEditorMsg::SpellPick(option.clone()));
            });
            ui.suggestions.append(&row);
        }
    }

    /// Show the hit in its buffer: selected, scrolled into view.
    fn spell_reveal(&self) {
        let Some(session) = &self.spell else {
            return;
        };
        let Some(hit) = session.hits.get(session.at) else {
            return;
        };
        let Some(buffer) = self
            .tabs
            .iter()
            .find(|t| t.path == session.file)
            .and_then(|t| t.buffer.clone())
        else {
            return;
        };
        let start = buffer.iter_at_offset(hit.src_start as i32);
        let end = buffer.iter_at_offset(hit.src_end as i32);
        buffer.select_range(&start, &end);
        if let Some(view) = self
            .tab_stack
            .child_by_name(&session.file)
            .and_then(|w| w.downcast::<gtk::ScrolledWindow>().ok())
            .and_then(|s| s.child())
            .and_then(|c| c.downcast::<sourceview5::View>().ok())
        {
            let mut at = start;
            view.scroll_to_iter(&mut at, 0.25, false, 0.0, 0.0);
        }
    }

    /// Step to the next live hit, or finish the session.
    fn spell_advance(&mut self, sender: &ComponentSender<Self>) {
        let done = match &mut self.spell {
            None => return,
            Some(session) => {
                session.at += 1;
                session.picked = None;
                while session.at < session.hits.len()
                    && session
                        .ignored
                        .contains(&fold_word(&session.hits[session.at].word))
                {
                    session.at += 1;
                }
                session.at >= session.hits.len()
            }
        };
        if done {
            let file = self
                .spell
                .as_ref()
                .map(|s| file_name(&s.file).to_string())
                .unwrap_or_default();
            self.spell = None;
            if let Some(ui) = self.spell_ui.take() {
                close_slot(&ui.slot);
            }
            self.show_pill(
                &format!("Spell check complete — {file} is clean"),
                false,
                sender,
            );
        } else {
            self.spell_render(sender);
            self.spell_reveal();
        }
    }

    fn spell_change(&mut self, sender: &ComponentSender<Self>) {
        let Some(session) = &self.spell else {
            return;
        };
        let (file, hit, replacement) = (
            session.file.clone(),
            session.hits.get(session.at).cloned(),
            session.picked.clone(),
        );
        let (Some(hit), Some(replacement)) = (hit, replacement) else {
            return;
        };
        let Some(buffer) = self
            .tabs
            .iter()
            .find(|t| t.path == file)
            .and_then(|t| t.buffer.clone())
        else {
            return;
        };
        let delta =
            replacement.chars().count() as i64 - (hit.src_end - hit.src_start) as i64;
        buffer.begin_user_action();
        let mut start = buffer.iter_at_offset(hit.src_start as i32);
        let mut end = buffer.iter_at_offset(hit.src_end as i32);
        buffer.delete(&mut start, &mut end);
        let mut at = buffer.iter_at_offset(hit.src_start as i32);
        buffer.insert(&mut at, &replacement);
        buffer.end_user_action();
        if let Some(session) = &mut self.spell {
            for later in session.hits.iter_mut().skip(session.at + 1) {
                later.src_start = (later.src_start as i64 + delta).max(0) as usize;
                later.src_end = (later.src_end as i64 + delta).max(0) as usize;
            }
        }
        self.spell_advance(sender);
    }

    fn present_personal_dict_dialog(&mut self, sender: &ComponentSender<Self>) {
        let words: Rc<RefCell<Vec<String>>> =
            Rc::new(RefCell::new(self.personal_words()));
        let shell = dialog_shell(
            DialogSize::Form,
            Some("Words you added in Spell check"),
        );
        let add_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let entry = gtk::Entry::new();
        entry.add_css_class("kalam-ed-entry");
        entry.set_placeholder_text(Some("Add a word"));
        entry.set_hexpand(true);
        add_row.append(&entry);
        shell.body().append(&add_row);
        let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(240);
        scroll.set_child(Some(&list));
        shell.body().append(&scroll);

        let rebuild: Rc<dyn Fn()> = {
            let list = list.clone();
            let words = words.clone();
            let s = sender.input_sender().clone();
            Rc::new(move || {
                clear_box(&list);
                let current = words.borrow().clone();
                if current.is_empty() {
                    let label = gtk::Label::new(Some("No words yet"));
                    label.add_css_class("kalam-ui-meta");
                    label.set_halign(gtk::Align::Start);
                    list.append(&label);
                    return;
                }
                for word in current {
                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    row.set_size_request(-1, 36);
                    let label = gtk::Label::new(Some(&word));
                    label.add_css_class("kalam-ed-pick-label");
                    label.set_halign(gtk::Align::Start);
                    label.set_hexpand(true);
                    label.set_valign(gtk::Align::Center);
                    row.append(&label);
                    let remove = gtk::Button::new();
                    remove.add_css_class("kalam-ed-glyph");
                    remove.set_size_request(32, 32);
                    remove.set_valign(gtk::Align::Center);
                    let x = gtk::Label::new(Some("✕"));
                    remove.set_child(Some(&x));
                    let words = words.clone();
                    let list_rm = list.clone();
                    let s = s.clone();
                    let gone = word.clone();
                    remove.connect_clicked(move |btn| {
                        words.borrow_mut().retain(|w| w != &gone);
                        if let Some(parent) = btn.parent() {
                            list_rm.remove(&parent);
                        }
                        let _ = s.send(EpubEditorMsg::PersonalRemove(gone.clone()));
                    });
                    row.append(&remove);
                    list.append(&row);
                }
            })
        };
        rebuild();
        let s = sender.input_sender().clone();
        let words_add = words.clone();
        let rebuild_add = rebuild.clone();
        add_row.append(&boxed_ghost_button("Add", move || {
            let word = entry.text().trim().to_string();
            if word.is_empty() {
                return;
            }
            entry.set_text("");
            {
                let mut list = words_add.borrow_mut();
                if !list.contains(&word) {
                    list.push(word.clone());
                    list.sort();
                }
            }
            rebuild_add();
            let _ = s.send(EpubEditorMsg::PersonalAdd(word));
        }));

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let slot_close = slot.clone();
        shell.footer().append(&primary_button("Done", move || {
            close_slot(&slot_close);
        }));
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Personal dictionary",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    // ── E5: History and Compare (decision 134) ──

    fn open_history(&mut self, sender: &ComponentSender<Self>) {
        let Some(book) = self.book.clone() else {
            return;
        };
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Reading the book's history",
            move |_reporter| service.editor_history(&book),
            |_| {},
            move |versions| {
                let _ = s.send(EpubEditorMsg::HistoryLoaded(versions));
            },
        );
    }

    fn present_history_dialog(
        &mut self,
        versions: Vec<crate::db::EditorVersion>,
        sender: &ComponentSender<Self>,
    ) {
        let title = self
            .book
            .as_ref()
            .map(|b| b.title.clone())
            .unwrap_or_default();
        let shell = dialog_shell(
            DialogSize::List,
            Some(&format!(
                "{title} · a snapshot is kept before each save"
            )),
        );
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(320);
        scroll.set_child(Some(&list));
        shell.body().append(&scroll);

        let versions: Rc<Vec<crate::db::EditorVersion>> = Rc::new(versions);
        let selected: Rc<std::cell::Cell<usize>> = Rc::new(std::cell::Cell::new(0));
        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));

        let compare_btn = {
            let versions = versions.clone();
            let selected = selected.clone();
            let slot = slot.clone();
            let s = sender.input_sender().clone();
            boxed_ghost_button("Compare", move || {
                if let Some(version) = versions.get(selected.get()) {
                    if version.missing {
                        return;
                    }
                    let message = EpubEditorMsg::CompareVersion {
                        id: version.snapshot_id,
                        label: version.label.clone(),
                        created_at: version.created_at.clone(),
                    };
                    close_slot(&slot);
                    let _ = s.send(message);
                }
            })
        };
        let restore_btn = {
            let versions = versions.clone();
            let selected = selected.clone();
            let slot = slot.clone();
            let s = sender.input_sender().clone();
            primary_button("Restore this version", move || {
                if let Some(version) = versions.get(selected.get()) {
                    if version.missing {
                        return;
                    }
                    let message = EpubEditorMsg::RestoreVersion {
                        id: version.snapshot_id,
                        label: version.label.clone(),
                    };
                    close_slot(&slot);
                    let _ = s.send(message);
                }
            })
        };
        shell.footer().append(&compare_btn);
        shell.footer().append(&restore_btn);

        if versions.is_empty() {
            let label = gtk::Label::new(Some(
                "No earlier versions yet — a snapshot is kept before each save.",
            ));
            label.add_css_class("kalam-ui-meta");
            label.set_halign(gtk::Align::Start);
            label.set_margin_top(8);
            list.append(&label);
            compare_btn.set_sensitive(false);
            restore_btn.set_sensitive(false);
        }
        let rows: Rc<RefCell<Vec<gtk::Button>>> = Rc::new(RefCell::new(Vec::new()));
        for (index, version) in versions.iter().enumerate() {
            let row = gtk::Button::new();
            row.add_css_class("kalam-ed-history-row");
            if index == 0 {
                row.add_css_class("selected");
            }
            row.set_size_request(-1, 64);
            let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
            text.set_valign(gtk::Align::Center);
            text.set_margin_start(16);
            let name = gtk::Label::new(Some(&version.label));
            name.add_css_class("kalam-ed-history-name");
            name.set_halign(gtk::Align::Start);
            text.append(&name);
            let meta = gtk::Label::new(Some(&Self::version_meta(version)));
            meta.add_css_class("kalam-ui-meta");
            meta.set_halign(gtk::Align::Start);
            text.append(&meta);
            row.set_child(Some(&text));
            {
                let versions = versions.clone();
                let selected = selected.clone();
                let rows = rows.clone();
                let compare_btn = compare_btn.clone();
                let restore_btn = restore_btn.clone();
                row.connect_clicked(move |_| {
                    selected.set(index);
                    for (at, other) in rows.borrow().iter().enumerate() {
                        if at == index {
                            other.add_css_class("selected");
                        } else {
                            other.remove_css_class("selected");
                        }
                    }
                    let usable = versions
                        .get(index)
                        .map(|v| !v.missing)
                        .unwrap_or(false);
                    compare_btn.set_sensitive(usable);
                    restore_btn.set_sensitive(usable);
                });
            }
            rows.borrow_mut().push(row.clone());
            list.append(&row);
        }
        if let Some(first) = versions.first() {
            compare_btn.set_sensitive(!first.missing);
            restore_btn.set_sensitive(!first.missing);
        }

        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "History",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    /// "Today 14:32 · 3 files changed", or "Added 12 Sep 2026"
    /// for the original file (decision 134's row copy).
    fn version_meta(version: &crate::db::EditorVersion) -> String {
        if version.snapshot_id.is_none() {
            return format!("Added {}", Self::pretty_date(&version.created_at));
        }
        let mut text = Self::version_when(&version.created_at);
        if let Some(count) = version.files_changed {
            text.push_str(&format!(
                " · {count} file{} changed",
                if count == 1 { "" } else { "s" }
            ));
        }
        if version.missing {
            text.push_str(" · snapshot file missing");
        }
        text
    }

    fn version_when(iso: &str) -> String {
        let now = crate::db::chrono_like_now();
        let time = iso.get(11..16).unwrap_or_default();
        if iso.get(..10) == now.get(..10) && !time.is_empty() {
            format!("Today {time}")
        } else if time.is_empty() {
            Self::pretty_date(iso)
        } else {
            format!("{} · {time}", Self::pretty_date(iso))
        }
    }

    fn pretty_date(iso: &str) -> String {
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct",
            "Nov", "Dec",
        ];
        let year = iso.get(0..4).unwrap_or_default();
        let month: usize = iso.get(5..7).and_then(|m| m.parse().ok()).unwrap_or(0);
        let day: u32 = iso.get(8..10).and_then(|d| d.parse().ok()).unwrap_or(0);
        match MONTHS.get(month.wrapping_sub(1)) {
            Some(name) if day > 0 && !year.is_empty() => {
                format!("{day} {name} {year}")
            }
            _ => iso.get(..10).unwrap_or(iso).to_string(),
        }
    }

    /// Every text file whose current words differ from the
    /// opened version, current book order first.
    fn build_compare_entries(
        &self,
        old: &EpubContainer,
    ) -> Vec<CompareEntry> {
        let mut paths: Vec<String> = Vec::new();
        let mut seen: HashSet<String> = HashSet::new();
        if let Some(current) = &self.container {
            for file in &current.files {
                if seen.insert(file.path.clone()) {
                    paths.push(file.path.clone());
                }
            }
        }
        for file in &old.files {
            if seen.insert(file.path.clone()) {
                paths.push(file.path.clone());
            }
        }
        let mut entries = Vec::new();
        for path in paths {
            let kind = self
                .container
                .as_ref()
                .and_then(|c| c.file(&path))
                .or_else(|| old.file(&path))
                .map(|f| f.kind);
            let Some(kind) = kind.filter(|k| k.is_text()) else {
                continue;
            };
            let old_text = old.text(&path).unwrap_or_default();
            let new_text = if self.removed_paths.contains(&path) {
                String::new()
            } else {
                self.file_text(&path).unwrap_or_default()
            };
            if old_text != new_text {
                entries.push(CompareEntry {
                    path,
                    kind,
                    old_text,
                    new_text,
                });
            }
        }
        entries
    }

    /// Paint the standing comparison: aligned lines and true
    /// line numbers into the four buffers, removed lines on the
    /// `match` tint and added lines on `diff-add` (decision 134).
    fn render_compare(&mut self) {
        let Some(state) = self.compare.as_ref() else {
            return;
        };
        let Some(entry) = state.entries.get(state.at) else {
            return;
        };
        let (rows, hunks) =
            crate::text_diff::align(&entry.old_text, &entry.new_text);
        let old_lines = crate::text_diff::lines_of(&entry.old_text);
        let new_lines = crate::text_diff::lines_of(&entry.new_text);
        let mut old_src = String::new();
        let mut new_src = String::new();
        let mut old_nums = String::new();
        let mut new_nums = String::new();
        for (index, row) in rows.iter().enumerate() {
            if index > 0 {
                old_src.push('\n');
                new_src.push('\n');
                old_nums.push('\n');
                new_nums.push('\n');
            }
            if let Some(line) = row.old {
                old_src.push_str(old_lines.get(line).copied().unwrap_or_default());
                old_nums.push_str(&(line + 1).to_string());
            }
            if let Some(line) = row.new {
                new_src.push_str(new_lines.get(line).copied().unwrap_or_default());
                new_nums.push_str(&(line + 1).to_string());
            }
        }

        let widgets = &self.compare_widgets;
        if let Some(id) = entry.kind.language_id() {
            if let Some(language) =
                sourceview5::LanguageManager::default().language(id)
            {
                widgets.old_buffer.set_language(Some(&language));
                widgets.new_buffer.set_language(Some(&language));
            }
        }
        let scheme = current_style_scheme();
        widgets.old_buffer.set_style_scheme(scheme.as_ref());
        widgets.new_buffer.set_style_scheme(scheme.as_ref());
        widgets.old_buffer.set_text(&old_src);
        widgets.new_buffer.set_text(&new_src);
        widgets.old_numbers.set_text(&old_nums);
        widgets.new_numbers.set_text(&new_nums);

        let removed_tag = Self::diff_tag(
            &widgets.old_buffer,
            "kalam-diff-removed",
            &widgets.removed_probe,
        );
        let added_tag = Self::diff_tag(
            &widgets.new_buffer,
            "kalam-diff-added",
            &widgets.added_probe,
        );
        for (index, row) in rows.iter().enumerate() {
            if row.kind == crate::text_diff::RowKind::Same {
                continue;
            }
            let line = index as i32;
            if row.kind == crate::text_diff::RowKind::Removed {
                Self::tag_line(&widgets.old_buffer, &removed_tag, line);
            }
            if row.new.is_some() {
                Self::tag_line(&widgets.new_buffer, &added_tag, line);
            }
        }

        widgets
            .title
            .set_label(&format!("Compare · {}", file_name(&entry.path)));
        let changes = if hunks == 1 {
            "1 change".to_string()
        } else {
            format!("{hunks} changes")
        };
        widgets.meta.set_label(&if state.entries.len() > 1 {
            format!(
                "{changes} · file {} of {}",
                state.at + 1,
                state.entries.len()
            )
        } else {
            changes
        });
        let head = if state.version_id.is_none() {
            state.label.to_uppercase()
        } else {
            match state.created_at.get(11..16) {
                Some(time) => format!("{} · {time}", state.label.to_uppercase()),
                None => state.label.to_uppercase(),
            }
        };
        widgets.left_head.set_label(&head);
        let many = state.entries.len() > 1;
        widgets.prev_btn.set_visible(many);
        widgets.next_btn.set_visible(many);
        widgets.prev_btn.set_sensitive(state.at > 0);
        widgets
            .next_btn
            .set_sensitive(state.at + 1 < state.entries.len());
        widgets.vadjustment.set_value(0.0);
    }

    /// The paragraph-background tag for one side of the diff,
    /// created once per buffer from the token colour its probe
    /// label resolves (the `widgets/paint.rs` pattern).
    fn diff_tag(
        buffer: &sourceview5::Buffer,
        name: &str,
        probe: &gtk::Label,
    ) -> gtk::TextTag {
        if let Some(tag) = buffer.tag_table().lookup(name) {
            return tag;
        }
        let tag = gtk::TextTag::new(Some(name));
        let rgba = crate::widgets::paint::probe_color(probe);
        tag.set_paragraph_background_rgba(Some(&rgba));
        buffer.tag_table().add(&tag);
        tag
    }

    fn tag_line(buffer: &sourceview5::Buffer, tag: &gtk::TextTag, line: i32) {
        let Some(start) = buffer.iter_at_line(line) else {
            return;
        };
        let end = buffer
            .iter_at_line(line + 1)
            .unwrap_or_else(|| buffer.end_iter());
        buffer.apply_tag(tag, &start, &end);
    }

    // ── E5: Export for editing (decision 140) ──

    fn start_export_for_editing(
        &self,
        path: &str,
        sender: &ComponentSender<Self>,
    ) {
        let Some(bytes) = self.file_bytes(path) else {
            return;
        };
        let inner = path.to_string();
        let reply_path = path.to_string();
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Copying the file for editing",
            move |_reporter| service.export_for_editing(&inner, &bytes),
            |_| {},
            move |result| match result {
                Ok(dest) => {
                    let _ = s.send(EpubEditorMsg::EditingExported(
                        reply_path.clone(),
                        dest,
                    ));
                }
                Err(message) => {
                    let _ = s.send(EpubEditorMsg::ActionFailed(message));
                }
            },
        );
    }

    /// The 420 dialog that walks a file out to another app and
    /// back (decision 140): Kalam watches the copy, says when it
    /// changed, and Import returns it through the replace rail.
    fn present_editing_dialog(
        &mut self,
        path: &str,
        dest: std::path::PathBuf,
        sender: &ComponentSender<Self>,
    ) {
        let name = file_name(path).to_string();
        let shell = dialog_shell(
            DialogSize::Form,
            Some("A copy is in ~/Kalam/Editing/"),
        );
        let file_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let file_text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        file_text.set_hexpand(true);
        let file_name_label = gtk::Label::new(Some(&name));
        file_name_label.add_css_class("kalam-ed-history-name");
        file_name_label.set_halign(gtk::Align::Start);
        file_text.append(&file_name_label);
        let waiting = gtk::Label::new(Some("Waiting for changes is automatic"));
        waiting.add_css_class("kalam-ui-meta");
        waiting.set_halign(gtk::Align::Start);
        file_text.append(&waiting);
        file_row.append(&file_text);
        let status = gtk::Label::new(Some("Not changed yet"));
        status.add_css_class("kalam-ui-meta");
        status.set_valign(gtk::Align::Center);
        file_row.append(&status);
        shell.body().append(&file_row);
        let copy = gtk::Label::new(Some(
            "Edit it in any app, save, then bring it back. \
             A snapshot is kept first.",
        ));
        copy.add_css_class("kalam-ui-meta");
        copy.set_halign(gtk::Align::Start);
        copy.set_xalign(0.0);
        copy.set_wrap(true);
        shell.body().append(&copy);

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        {
            let dest = dest.clone();
            shell.footer().append(&boxed_ghost_button(
                "Show in folder",
                move || {
                    crate::pages::book::open_in_file_manager(&dest);
                },
            ));
        }
        {
            let dest = dest.clone();
            shell.footer().append(&boxed_ghost_button("Open in app", move || {
                let file = gtk::gio::File::for_path(&dest);
                let _ = gtk::gio::AppInfo::launch_default_for_uri(
                    file.uri().as_str(),
                    None::<&gtk::gio::AppLaunchContext>,
                );
            }));
        }
        {
            let slot = slot.clone();
            let s = sender.input_sender().clone();
            let path = path.to_string();
            let dest = dest.clone();
            shell.footer().append(&primary_button("Import changes", move || {
                close_slot(&slot);
                let _ = s.send(EpubEditorMsg::EditingImport(
                    path.clone(),
                    dest.clone(),
                ));
            }));
        }

        // Watch the Editing folder (many apps save by rename,
        // which a file monitor on the copy itself would lose)
        // and speak only about our copy. The tick keeps the
        // "N min ago" honest and retires the monitor once the
        // dialog is gone.
        let changed_at: Rc<std::cell::Cell<Option<std::time::Instant>>> =
            Rc::new(std::cell::Cell::new(None));
        let monitor: Rc<RefCell<Option<gtk::gio::FileMonitor>>> =
            Rc::new(RefCell::new(None));
        if let Some(dir) = dest.parent().map(std::path::Path::to_path_buf) {
            let gio_dir = gtk::gio::File::for_path(&dir);
            let flags = gtk::gio::FileMonitorFlags::SEND_MOVED;
            if let Ok(found) =
                gio_dir.monitor_directory(flags, gtk::gio::Cancellable::NONE)
            {
                let watch = dest.clone();
                let changed_at = changed_at.clone();
                let status = status.clone();
                found.connect_changed(move |_mon, file, other, event| {
                    use gtk::gio::FileMonitorEvent;
                    let touches = [Some(file), other]
                        .into_iter()
                        .flatten()
                        .any(|f| f.path().as_deref() == Some(watch.as_path()));
                    if !touches {
                        return;
                    }
                    match event {
                        FileMonitorEvent::Created
                        | FileMonitorEvent::Changed
                        | FileMonitorEvent::ChangesDoneHint
                        | FileMonitorEvent::MovedIn
                        | FileMonitorEvent::Moved
                        | FileMonitorEvent::Renamed => {
                            changed_at.set(Some(std::time::Instant::now()));
                            status.set_label("Changed just now");
                        }
                        _ => {}
                    }
                });
                *monitor.borrow_mut() = Some(found);
            }
        }
        {
            let status = status.clone();
            let changed_at = changed_at.clone();
            let monitor = monitor.clone();
            let _tick = gtk::glib::timeout_add_local(std::time::Duration::from_secs(20), move || {
                if status.root().is_none() {
                    *monitor.borrow_mut() = None;
                    return gtk::glib::ControlFlow::Break;
                }
                if let Some(when) = changed_at.get() {
                    let mins = when.elapsed().as_secs() / 60;
                    status.set_label(&if mins == 0 {
                        "Changed just now".to_string()
                    } else if mins == 1 {
                        "Changed 1 min ago".to_string()
                    } else {
                        format!("Changed {mins} min ago")
                    });
                }
                gtk::glib::ControlFlow::Continue
            });
        }

        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            &format!("Edit {name} outside Kalam"),
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    // ── E5: font specimen + Replace cover (decisions 126, 139) ──

    /// The embedded font reached disk: load it into a private
    /// Pango/Cairo font map (this book only, never the app's)
    /// and give the tab's drawing area its specimen lines.
    fn font_preview_ready(&self, path: &str, disk: &std::path::Path, family: &str) {
        use pango::prelude::*;
        let found = {
            let mut areas = self.font_areas.borrow_mut();
            areas.retain(|(_, area, _)| area.root().is_some());
            areas
                .iter()
                .find(|(p, _, _)| p == path)
                .map(|(_, area, probe)| (area.clone(), probe.clone()))
        };
        let Some((area, probe)) = found else {
            return;
        };
        let font_map = pangocairo::FontMap::new();
        if font_map.add_font_file(disk).is_err() {
            return;
        }
        let context = font_map.create_context();
        let lines: [(&str, f64); 4] = [
            (family, 30.0),
            ("The road bent north past the mill.", 17.0),
            ("She counted the lamps.", 24.0),
            ("Aa Bb Cc 0123", 30.0),
        ];
        let mut layouts: Vec<(pango::Layout, f64)> = Vec::new();
        let mut y = 4.0f64;
        for (text, size) in lines {
            let layout = pango::Layout::new(&context);
            let description =
                pango::FontDescription::from_string(&format!("{family} {size}"));
            layout.set_font_description(Some(&description));
            layout.set_text(text);
            let height = layout.pixel_size().1 as f64;
            layouts.push((layout, y));
            y += height + 10.0;
        }
        area.set_content_height((y + 4.0) as i32);
        let keep_map = font_map;
        area.set_draw_func(move |_area, cr, _width, _height| {
            let _keep = &keep_map;
            let rgba = crate::widgets::paint::probe_color(&probe);
            crate::widgets::paint::set_source(cr, rgba, rgba.alpha() as f64);
            for (layout, at) in &layouts {
                cr.move_to(0.0, *at);
                pangocairo::functions::update_layout(cr, layout);
                pangocairo::functions::show_layout(cr, layout);
            }
        });
        area.queue_draw();
    }

    /// The 420 Replace cover dialog (decision 139): the current
    /// cover, a chooser, and Replace once a picture is chosen.
    fn present_replace_cover_dialog(&mut self, sender: &ComponentSender<Self>) {
        let Some(book) = self.book.clone() else {
            return;
        };
        let cover_path = self
            .container
            .as_ref()
            .and_then(|c| c.files.iter().find(|f| f.is_cover_image))
            .map(|f| f.path.clone());
        let shell = dialog_shell(DialogSize::Form, Some(&book.title));

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
        let mut current_text = "This book has no cover image yet".to_string();
        if let Some(path) = &cover_path {
            if let Some(bytes) = self.file_bytes(path) {
                let glib_bytes = gtk::glib::Bytes::from(&bytes[..]);
                if let Ok(texture) = gtk::gdk::Texture::from_bytes(&glib_bytes) {
                    current_text = format!(
                        "Current cover, {} x {}",
                        texture.width(),
                        texture.height()
                    );
                    let picture = gtk::Picture::for_paintable(&texture);
                    picture.set_content_fit(gtk::ContentFit::Contain);
                    picture.set_size_request(96, 144);
                    row.append(&picture);
                }
            }
        }
        let side = gtk::Box::new(gtk::Orientation::Vertical, 10);
        side.set_valign(gtk::Align::Center);
        let current_label = gtk::Label::new(Some(&current_text));
        current_label.add_css_class("kalam-ui-meta");
        current_label.set_halign(gtk::Align::Start);
        current_label.set_xalign(0.0);
        current_label.set_wrap(true);
        side.append(&current_label);
        let chosen_label = gtk::Label::new(None);
        chosen_label.add_css_class("kalam-ui-meta");
        chosen_label.set_halign(gtk::Align::Start);
        chosen_label.set_xalign(0.0);
        chosen_label.set_wrap(true);
        row.append(&side);
        shell.body().append(&row);

        let chosen: ChosenBytes =
            Rc::new(RefCell::new(None));
        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));

        let replace_btn = {
            let chosen = chosen.clone();
            let slot = slot.clone();
            let s = sender.input_sender().clone();
            primary_button("Replace", move || {
                if let Some((name, bytes)) = chosen.borrow().clone() {
                    close_slot(&slot);
                    let _ = s.send(EpubEditorMsg::RunReplaceCover(name, bytes));
                }
            })
        };
        replace_btn.set_sensitive(false);

        let choose_btn = {
            let chosen = chosen.clone();
            let chosen_label = chosen_label.clone();
            let replace_btn = replace_btn.clone();
            let service = self.service.clone();
            let window = self.host_window();
            let s_failed = sender.input_sender().clone();
            boxed_ghost_button("Choose image…", move || {
                let dialog = gtk::FileDialog::builder()
                    .title("Choose a cover image")
                    .modal(true)
                    .build();
                let filter = gtk::FileFilter::new();
                filter.set_name(Some("Images"));
                for suffix in ["png", "jpg", "jpeg", "gif", "webp", "svg"] {
                    filter.add_suffix(suffix);
                }
                let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
                filters.append(&filter);
                dialog.set_filters(Some(&filters));
                let chosen = chosen.clone();
                let chosen_label = chosen_label.clone();
                let replace_btn = replace_btn.clone();
                let service = service.clone();
                let s_failed = s_failed.clone();
                dialog.open(
                    window.as_ref(),
                    gtk::gio::Cancellable::NONE,
                    move |result| {
                        let Ok(file) = result else {
                            return;
                        };
                        let Some(disk) = file.path() else {
                            return;
                        };
                        let name = disk
                            .file_name()
                            .map(|n| n.to_string_lossy().to_string())
                            .unwrap_or_else(|| "cover".to_string());
                        crate::tasks::spawn_internal(
                            "Reading the chosen image",
                            move |_reporter| service.read_editor_import(&disk),
                            |_| {},
                            move |result| match result {
                                Ok(bytes) => {
                                    let dims = gtk::glib::Bytes::from(&bytes[..]);
                                    let text =
                                        match gtk::gdk::Texture::from_bytes(&dims) {
                                            Ok(texture) => format!(
                                                "New cover: {name}, {} x {}",
                                                texture.width(),
                                                texture.height()
                                            ),
                                            Err(_) => format!("New cover: {name}"),
                                        };
                                    chosen_label.set_label(&text);
                                    *chosen.borrow_mut() = Some((name, bytes));
                                    replace_btn.set_sensitive(true);
                                }
                                Err(message) => {
                                    let _ = s_failed
                                        .send(EpubEditorMsg::ActionFailed(message));
                                }
                            },
                        );
                    },
                );
            })
        };
        side.append(&choose_btn);
        side.append(&chosen_label);

        let slot_cancel = slot.clone();
        shell.footer().append(&boxed_ghost_button("Cancel", move || {
            close_slot(&slot_cancel);
        }));
        shell.footer().append(&replace_btn);

        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Replace cover",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    /// Swap the cover bytes in, or — coverless book — land a new
    /// cover file beside the images and crown it.
    fn run_replace_cover(
        &mut self,
        name: String,
        bytes: Vec<u8>,
        sender: &ComponentSender<Self>,
    ) {
        let Some(container) = &self.container else {
            return;
        };
        let mut plan = StructurePlan::new(
            "Replaced the cover".to_string(),
            "Before replace cover",
        );
        let current = container
            .files
            .iter()
            .find(|f| f.is_cover_image)
            .map(|f| f.path.clone());
        match current {
            Some(path) => plan.byte_changes.push((path, bytes)),
            None => {
                let dir = container
                    .files
                    .iter()
                    .find(|f| f.kind == FileKind::Image)
                    .map(|f| crate::epub_structure::parent_dir(&f.path))
                    .unwrap_or_else(|| {
                        crate::epub_structure::parent_dir(&container.opf_path)
                    });
                let ext = name
                    .rsplit('.')
                    .next()
                    .filter(|e| !e.is_empty() && e.len() <= 5)
                    .unwrap_or("jpg");
                let mut path = format!("{dir}cover.{ext}");
                let mut n = 1;
                while container.file(&path).is_some() {
                    n += 1;
                    path = format!("{dir}cover{n}.{ext}");
                }
                let media = crate::epub_container::guess_media_type(&path);
                plan.new_files.push((
                    EpubFile {
                        path: path.clone(),
                        kind: FileKind::Image,
                        media_type: media.to_string(),
                        spine_index: None,
                        is_nav: false,
                        is_cover_image: true,
                        in_manifest: true,
                    },
                    bytes,
                ));
                plan.set_cover = Some(path);
            }
        }
        self.finish_structure(plan, sender);
    }

    // ── E5: Reports (decision 141) ──

    fn present_reports_dialog(&mut self, sender: &ComponentSender<Self>) {
        self.present_reports_dialog_at(0, sender);
    }

    fn present_reports_dialog_at(
        &mut self,
        start: usize,
        sender: &ComponentSender<Self>,
    ) {
        let tables: Vec<(&'static str, ReportTable)> = vec![
            ("Files", self.report_files()),
            ("Words", self.report_words()),
            ("Images", self.report_images()),
            ("Links", self.report_links()),
            ("CSS", self.report_css()),
            ("Fonts", self.report_fonts()),
        ];
        let shell = dialog_shell(DialogSize::Wide, None);
        let tabs_row = gtk::Box::new(gtk::Orientation::Horizontal, 18);
        shell.body().append(&tabs_row);
        let stack = gtk::Stack::new();
        stack.set_vexpand(true);
        shell.body().append(&stack);
        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));

        let mut tab_switches: Vec<(gtk::Button, String)> = Vec::new();
        for (index, (name, table)) in tables.into_iter().enumerate() {
            let page_name = format!("report-{index}");
            let tab_btn = gtk::Button::with_label(name);
            tab_btn.add_css_class("kalam-ed-texttab");
            if index == start {
                tab_btn.add_css_class("active");
            }
            tabs_row.append(&tab_btn);
            tab_switches.push((tab_btn, page_name.clone()));

            let state = Rc::new(RefCell::new(table));
            let host = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let header_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            let header_btns: Rc<RefCell<Vec<gtk::Button>>> =
                Rc::new(RefCell::new(Vec::new()));
            {
                let table = state.borrow();
                for (col, header) in table.headers.iter().enumerate() {
                    let btn = gtk::Button::with_label(header);
                    btn.add_css_class("kalam-ed-report-head");
                    btn.set_halign(gtk::Align::Start);
                    let width = table.widths[col];
                    if width > 0 {
                        btn.set_size_request(width, -1);
                    } else {
                        btn.set_hexpand(true);
                    }
                    header_row.append(&btn);
                    header_btns.borrow_mut().push(btn);
                }
            }
            host.append(&header_row);
            let rows_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
            host.append(&rows_host);
            let scroll = gtk::ScrolledWindow::new();
            scroll.set_policy(
                gtk::PolicyType::Never,
                gtk::PolicyType::Automatic,
            );
            scroll.set_vexpand(true);
            scroll.set_child(Some(&host));
            stack.add_named(&scroll, Some(&page_name));

            let render: Rc<dyn Fn()> = {
                let state = state.clone();
                let rows_host = rows_host.clone();
                let header_btns = header_btns.clone();
                let slot = slot.clone();
                let s = sender.input_sender().clone();
                Rc::new(move || {
                    clear_box(&rows_host);
                    let table = state.borrow();
                    for (col, btn) in header_btns.borrow().iter().enumerate() {
                        let mut label = table.headers[col].to_string();
                        if table.sort_col == Some(col) {
                            label.push_str(if table.sort_desc {
                                " ↓"
                            } else {
                                " ↑"
                            });
                        }
                        btn.set_label(&label);
                    }
                    if table.rows.is_empty() {
                        let label = gtk::Label::new(Some("Nothing to report"));
                        label.add_css_class("kalam-ui-meta");
                        label.set_halign(gtk::Align::Start);
                        label.set_margin_top(10);
                        rows_host.append(&label);
                        return;
                    }
                    let mut order: Vec<usize> =
                        (0..table.rows.len()).collect();
                    if let Some(col) = table.sort_col {
                        let numeric = table.numeric[col];
                        let desc = table.sort_desc;
                        order.sort_by(|&a, &b| {
                            let ord = if numeric {
                                table.rows[a].keys[col]
                                    .cmp(&table.rows[b].keys[col])
                            } else {
                                table.rows[a].cells[col]
                                    .to_lowercase()
                                    .cmp(&table.rows[b].cells[col].to_lowercase())
                            };
                            if desc { ord.reverse() } else { ord }
                        });
                    }
                    for at in order {
                        let row = &table.rows[at];
                        let line = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                        line.add_css_class("kalam-ed-report-row");
                        line.set_size_request(-1, 36);
                        for (col, cell) in row.cells.iter().enumerate() {
                            let width = table.widths[col];
                            match (col, &row.open) {
                                (0, Some(open)) => {
                                    let btn = gtk::Button::with_label(cell);
                                    btn.add_css_class("kalam-ed-ghost");
                                    btn.set_halign(gtk::Align::Start);
                                    if width > 0 {
                                        btn.set_size_request(width, -1);
                                    } else {
                                        btn.set_hexpand(true);
                                    }
                                    let s = s.clone();
                                    let slot = slot.clone();
                                    let open = open.clone();
                                    btn.connect_clicked(move |_| {
                                        close_slot(&slot);
                                        let _ = s.send(EpubEditorMsg::OpenFile {
                                            path: open.clone(),
                                            pin: false,
                                        });
                                    });
                                    line.append(&btn);
                                }
                                _ => {
                                    let label = gtk::Label::new(Some(cell));
                                    label.add_css_class("kalam-ed-report-cell");
                                    label.set_halign(gtk::Align::Start);
                                    label.set_xalign(0.0);
                                    label.set_valign(gtk::Align::Center);
                                    label.set_ellipsize(
                                        gtk::pango::EllipsizeMode::End,
                                    );
                                    if width > 0 {
                                        label.set_size_request(width, -1);
                                    } else {
                                        label.set_hexpand(true);
                                    }
                                    line.append(&label);
                                }
                            }
                        }
                        rows_host.append(&line);
                    }
                })
            };
            render();
            for (col, btn) in header_btns.borrow().iter().enumerate() {
                let state = state.clone();
                let render = render.clone();
                btn.connect_clicked(move |_| {
                    {
                        let mut table = state.borrow_mut();
                        if table.sort_col == Some(col) {
                            table.sort_desc = !table.sort_desc;
                        } else {
                            table.sort_col = Some(col);
                            table.sort_desc = false;
                        }
                    }
                    render();
                });
            }
        }
        let tab_btns: Rc<Vec<(gtk::Button, String)>> = Rc::new(tab_switches);
        for (index, (btn, page_name)) in tab_btns.iter().enumerate() {
            let stack = stack.clone();
            let tab_btns = tab_btns.clone();
            let page_name = page_name.clone();
            btn.connect_clicked(move |_| {
                stack.set_visible_child_name(&page_name);
                for (at, (other, _)) in tab_btns.iter().enumerate() {
                    if at == index {
                        other.add_css_class("active");
                    } else {
                        other.remove_css_class("active");
                    }
                }
            });
        }

        stack.set_visible_child_name(&format!("report-{start}"));
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Reports",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    fn report_files(&self) -> ReportTable {
        let mut rows = Vec::new();
        if let Some(container) = &self.container {
            for file in &container.files {
                if self.removed_paths.contains(&file.path) {
                    continue;
                }
                let size = self.file_bytes(&file.path).map(|b| b.len()).unwrap_or(0);
                rows.push(ReportRow {
                    cells: vec![
                        file.path.clone(),
                        file.kind.label().to_string(),
                        human_size(size),
                    ],
                    keys: vec![0, 0, size as i64],
                    open: Some(file.path.clone()),
                });
            }
        }
        ReportTable {
            headers: vec!["FILE", "KIND", "SIZE"],
            widths: vec![-1, 90, 80],
            numeric: vec![false, false, true],
            rows,
            sort_col: None,
            sort_desc: false,
        }
    }

    fn report_words(&self) -> ReportTable {
        let mut rows = Vec::new();
        if let Some(container) = &self.container {
            for file in &container.files {
                if file.kind != FileKind::Html
                    || self.removed_paths.contains(&file.path)
                {
                    continue;
                }
                let Some(text) = self.file_text(&file.path) else {
                    continue;
                };
                let count = crate::epub_structure::spell_words(
                    &strip_map(&text).text,
                )
                .len();
                rows.push(ReportRow {
                    cells: vec![
                        file_name(&file.path).to_string(),
                        grouped(count),
                    ],
                    keys: vec![0, count as i64],
                    open: Some(file.path.clone()),
                });
            }
        }
        ReportTable {
            headers: vec!["FILE", "WORDS"],
            widths: vec![-1, 110],
            numeric: vec![false, true],
            rows,
            sort_col: None,
            sort_desc: false,
        }
    }

    fn report_images(&self) -> ReportTable {
        let mut rows = Vec::new();
        if let Some(container) = &self.container {
            for file in &container.files {
                if file.kind != FileKind::Image
                    || self.removed_paths.contains(&file.path)
                {
                    continue;
                }
                let size = self.file_bytes(&file.path).map(|b| b.len()).unwrap_or(0);
                let used = self.referencing_files(&file.path).len();
                rows.push(ReportRow {
                    cells: vec![
                        file_name(&file.path).to_string(),
                        human_size(size),
                        match used {
                            0 => "Not used".to_string(),
                            1 => "1 file".to_string(),
                            n => format!("{n} files"),
                        },
                    ],
                    keys: vec![0, size as i64, used as i64],
                    open: Some(file.path.clone()),
                });
            }
        }
        ReportTable {
            headers: vec!["IMAGE", "SIZE", "USED IN"],
            widths: vec![-1, 80, 110],
            numeric: vec![false, true, true],
            rows,
            sort_col: None,
            sort_desc: false,
        }
    }

    fn report_links(&self) -> ReportTable {
        let mut rows = Vec::new();
        if let Some(container) = &self.container {
            let mut seen: HashSet<(String, String)> = HashSet::new();
            for file in &container.files {
                if !file.kind.is_text() || self.removed_paths.contains(&file.path)
                {
                    continue;
                }
                let Some(text) = self.file_text(&file.path) else {
                    continue;
                };
                for found in crate::epub_check::find_refs(&text, false) {
                    if found.href.starts_with('#') {
                        continue;
                    }
                    if !seen.insert((found.href.clone(), file.path.clone())) {
                        continue;
                    }
                    let shown = if found.href.chars().count() > 64 {
                        let cut: String =
                            found.href.chars().take(63).collect();
                        format!("{cut}…")
                    } else {
                        found.href.clone()
                    };
                    rows.push(ReportRow {
                        cells: vec![
                            shown,
                            file_name(&file.path).to_string(),
                        ],
                        keys: vec![0, 0],
                        open: Some(file.path.clone()),
                    });
                }
            }
        }
        ReportTable {
            headers: vec!["LINK", "FOUND IN"],
            widths: vec![-1, 180],
            numeric: vec![false, false],
            rows,
            sort_col: None,
            sort_desc: false,
        }
    }

    fn report_css(&self) -> ReportTable {
        let mut rows = Vec::new();
        if let Some(container) = &self.container {
            for file in &container.files {
                if file.kind != FileKind::Css
                    || self.removed_paths.contains(&file.path)
                {
                    continue;
                }
                let rules = self
                    .file_text(&file.path)
                    .map(|text| crate::epub_css::parse_rules(&text).len())
                    .unwrap_or(0);
                let size = self.file_bytes(&file.path).map(|b| b.len()).unwrap_or(0);
                rows.push(ReportRow {
                    cells: vec![
                        file_name(&file.path).to_string(),
                        grouped(rules),
                        human_size(size),
                    ],
                    keys: vec![0, rules as i64, size as i64],
                    open: Some(file.path.clone()),
                });
            }
        }
        ReportTable {
            headers: vec!["STYLESHEET", "RULES", "SIZE"],
            widths: vec![-1, 80, 80],
            numeric: vec![false, true, true],
            rows,
            sort_col: None,
            sort_desc: false,
        }
    }

    fn report_fonts(&self) -> ReportTable {
        let mut rows = Vec::new();
        if let Some(container) = &self.container {
            for file in &container.files {
                if file.kind != FileKind::Font
                    || self.removed_paths.contains(&file.path)
                {
                    continue;
                }
                let size = self.file_bytes(&file.path).map(|b| b.len()).unwrap_or(0);
                let used_by = self
                    .referencing_files(&file.path)
                    .first()
                    .map(|(p, _)| file_name(p).to_string())
                    .unwrap_or_else(|| "Not used".to_string());
                rows.push(ReportRow {
                    cells: vec![
                        file_name(&file.path).to_string(),
                        human_size(size),
                        used_by,
                    ],
                    keys: vec![0, size as i64, 0],
                    open: Some(file.path.clone()),
                });
            }
        }
        ReportTable {
            headers: vec!["FONT", "SIZE", "USED BY"],
            widths: vec![-1, 80, 140],
            numeric: vec![false, true, false],
            rows,
            sort_col: None,
            sort_desc: false,
        }
    }

    // ── E5: Tag mapper (decision 141) ──

    /// The standing rules: (active, rule), board defaults first.
    fn tag_rules(&self) -> Vec<(bool, crate::epub_tools::TagRule)> {
        let stored: Option<Vec<(bool, String, String)>> = self
            .service
            .catalog()
            .get_pref("editor.tag_rules")
            .and_then(|raw| serde_json::from_str(&raw).ok());
        match stored {
            Some(rows) => rows
                .into_iter()
                .map(|(on, from, to)| (on, crate::epub_tools::TagRule { from, to }))
                .collect(),
            None => vec![
                (true, crate::epub_tools::TagRule { from: "b".into(), to: "strong".into() }),
                (true, crate::epub_tools::TagRule { from: "i".into(), to: "em".into() }),
                (false, crate::epub_tools::TagRule { from: "div.poem".into(), to: "blockquote".into() }),
            ],
        }
    }

    /// Whole-file replacements for the active rules, plus the
    /// element count the dialog announces.
    fn tag_mapper_changes(
        &self,
        rules: &[crate::epub_tools::TagRule],
    ) -> (Vec<(String, String)>, usize) {
        let mut changes = Vec::new();
        let mut elements = 0usize;
        if let Some(container) = &self.container {
            for file in &container.files {
                if file.kind != FileKind::Html
                    || self.removed_paths.contains(&file.path)
                {
                    continue;
                }
                let Some(text) = self.file_text(&file.path) else {
                    continue;
                };
                let (new, n) = crate::epub_tools::retag(&text, rules);
                if n > 0 && new != text {
                    elements += n;
                    changes.push((file.path.clone(), new));
                }
            }
        }
        (changes, elements)
    }

    fn present_tag_mapper_dialog(&mut self, sender: &ComponentSender<Self>) {
        // Recounts run against the texts as the dialog opens;
        // Apply recomputes against the living book.
        let texts: Vec<String> = self
            .container
            .as_ref()
            .map(|c| {
                c.files
                    .iter()
                    .filter(|f| {
                        f.kind == FileKind::Html
                            && !self.removed_paths.contains(&f.path)
                    })
                    .filter_map(|f| self.file_text(&f.path))
                    .collect()
            })
            .unwrap_or_default();
        let rules: Rc<RefCell<Vec<(bool, crate::epub_tools::TagRule)>>> =
            Rc::new(RefCell::new(self.tag_rules()));
        let shell = dialog_shell(
            DialogSize::List,
            Some("Change tags across the book"),
        );
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(260);
        scroll.set_child(Some(&list));
        shell.body().append(&scroll);

        let foot_row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let count_label = gtk::Label::new(None);
        count_label.add_css_class("kalam-ui-meta");
        count_label.set_valign(gtk::Align::Center);
        foot_row.append(&count_label);
        shell.body().append(&foot_row);

        let add_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        add_row.set_visible(false);
        let from_entry = gtk::Entry::new();
        from_entry.add_css_class("kalam-ed-entry");
        from_entry.set_placeholder_text(Some("b or div.poem"));
        from_entry.set_hexpand(true);
        add_row.append(&from_entry);
        let arrow = gtk::Label::new(Some("→"));
        arrow.add_css_class("kalam-ui-meta");
        arrow.set_valign(gtk::Align::Center);
        add_row.append(&arrow);
        let to_entry = gtk::Entry::new();
        to_entry.add_css_class("kalam-ed-entry");
        to_entry.set_placeholder_text(Some("strong"));
        to_entry.set_hexpand(true);
        add_row.append(&to_entry);
        shell.body().append(&add_row);

        let persist: Rc<dyn Fn()> = {
            let rules = rules.clone();
            let service = self.service.clone();
            Rc::new(move || {
                let rows: Vec<(bool, String, String)> = rules
                    .borrow()
                    .iter()
                    .map(|(on, r)| (*on, r.from.clone(), r.to.clone()))
                    .collect();
                if let Ok(raw) = serde_json::to_string(&rows) {
                    service.catalog().set_pref("editor.tag_rules", &raw);
                }
            })
        };
        let recount: Rc<dyn Fn()> = {
            let rules = rules.clone();
            let count_label = count_label.clone();
            Rc::new(move || {
                let active: Vec<crate::epub_tools::TagRule> = rules
                    .borrow()
                    .iter()
                    .filter(|(on, _)| *on)
                    .map(|(_, r)| r.clone())
                    .collect();
                let (mut elements, mut files) = (0usize, 0usize);
                for text in &texts {
                    let (_, n) = crate::epub_tools::retag(text, &active);
                    if n > 0 {
                        elements += n;
                        files += 1;
                    }
                }
                count_label.set_label(&if elements == 0 {
                    "Changes nothing with these rules".to_string()
                } else {
                    format!(
                        "Changes {elements} {} in {files} {}",
                        if elements == 1 { "element" } else { "elements" },
                        if files == 1 { "file" } else { "files" }
                    )
                });
            })
        };
        let rebuild: Rc<dyn Fn()> = {
            let list = list.clone();
            let rules = rules.clone();
            let persist = persist.clone();
            let recount = recount.clone();
            // The rows call back into rebuild when one is removed:
            // an Rc slot filled right below breaks the cycle.
            let self_slot: SelfSlot =
                Rc::new(RefCell::new(None));
            let rebuild = {
                let self_slot = self_slot.clone();
                Rc::new(move || {
                    clear_box(&list);
                    let current = rules.borrow().clone();
                    if current.is_empty() {
                        let label = gtk::Label::new(Some("No rules yet"));
                        label.add_css_class("kalam-ui-meta");
                        label.set_halign(gtk::Align::Start);
                        label.set_margin_top(8);
                        list.append(&label);
                        return;
                    }
                    for (index, (on, rule)) in current.iter().enumerate() {
                        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                        row.add_css_class("kalam-ed-report-row");
                        row.set_size_request(-1, 44);
                        let toggle = gtk::Button::new();
                        toggle.add_css_class("kalam-ed-tickrow");
                        toggle.set_hexpand(true);
                        let inner = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                        let circle = tick_circle(*on);
                        inner.append(&circle);
                        let label = gtk::Label::new(Some(&format!(
                            "{} → {}",
                            rule.from, rule.to
                        )));
                        label.add_css_class("kalam-ed-mono");
                        label.set_halign(gtk::Align::Start);
                        label.set_valign(gtk::Align::Center);
                        inner.append(&label);
                        toggle.set_child(Some(&inner));
                        {
                            let rules = rules.clone();
                            let persist = persist.clone();
                            let recount = recount.clone();
                            toggle.connect_clicked(move |_| {
                                {
                                    let mut all = rules.borrow_mut();
                                    if let Some(entry) = all.get_mut(index) {
                                        entry.0 = !entry.0;
                                    }
                                }
                                set_tick_circle(&circle, rules.borrow()[index].0);
                                persist();
                                recount();
                            });
                        }
                        row.append(&toggle);
                        let remove = gtk::Button::new();
                        remove.add_css_class("kalam-ed-glyph");
                        remove.set_size_request(32, 32);
                        remove.set_valign(gtk::Align::Center);
                        remove.set_child(Some(&gtk::Label::new(Some("✕"))));
                        {
                            let rules = rules.clone();
                            let persist = persist.clone();
                            let recount = recount.clone();
                            let self_slot = self_slot.clone();
                            remove.connect_clicked(move |_| {
                                rules.borrow_mut().remove(index);
                                persist();
                                recount();
                                if let Some(rebuild) = self_slot.borrow().as_ref() {
                                    rebuild();
                                }
                            });
                        }
                        row.append(&remove);
                        list.append(&row);
                    }
                }) as Rc<dyn Fn()>
            };
            *self_slot.borrow_mut() = Some(rebuild.clone());
            rebuild
        };
        {
            let add_row = add_row.clone();
            let add_btn = boxed_ghost_button("+ Add rule", move || {
                add_row.set_visible(!add_row.is_visible());
            });
            add_btn.add_css_class("kalam-ed-accent-text");
            foot_row.append(&add_btn);
        }
        {
            let rules = rules.clone();
            let persist = persist.clone();
            let recount = recount.clone();
            let rebuild = rebuild.clone();
            let from_entry = from_entry.clone();
            let to_entry = to_entry.clone();
            let add_row = add_row.clone();
            let add_row_hide = add_row.clone();
            let add_inner = boxed_ghost_button("Add", move || {
                let from = from_entry.text().trim().to_string();
                let to = to_entry.text().trim().to_string();
                if !valid_tag_rule(&from, &to) {
                    return;
                }
                {
                    let mut all = rules.borrow_mut();
                    if all.iter().any(|(_, r)| r.from == from) {
                        return;
                    }
                    all.push((
                        true,
                        crate::epub_tools::TagRule { from, to },
                    ));
                }
                from_entry.set_text("");
                to_entry.set_text("");
                add_row_hide.set_visible(false);
                persist();
                rebuild();
                recount();
            });
            add_row.append(&add_inner);
        }
        rebuild();
        recount();

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let slot_cancel = slot.clone();
        shell.footer().append(&boxed_ghost_button("Cancel", move || {
            close_slot(&slot_cancel);
        }));
        {
            let slot = slot.clone();
            let rules = rules.clone();
            let s = sender.input_sender().clone();
            shell.footer().append(&primary_button("Apply", move || {
                let active: Vec<crate::epub_tools::TagRule> = rules
                    .borrow()
                    .iter()
                    .filter(|(on, _)| *on)
                    .map(|(_, r)| r.clone())
                    .collect();
                close_slot(&slot);
                let _ = s.send(EpubEditorMsg::RunTagMapper(active));
            }));
        }
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Tag mapper",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    /// Whole-file replacements for Filter style information:
    /// declarations dropped from stylesheets, the same from
    /// inline `style` attributes, one undo step per file.
    fn filter_style_changes(
        &self,
        filter: &[String],
    ) -> (Vec<(String, String)>, usize) {
        let props: Vec<&str> = filter.iter().map(String::as_str).collect();
        let mut changes = Vec::new();
        let mut declarations = 0usize;
        if let Some(container) = &self.container {
            for file in &container.files {
                if self.removed_paths.contains(&file.path) {
                    continue;
                }
                let outcome = match file.kind {
                    FileKind::Css => self
                        .file_text(&file.path)
                        .map(|text| crate::epub_tools::filter_stylesheet(&text, &props)),
                    FileKind::Html => self
                        .file_text(&file.path)
                        .map(|text| crate::epub_tools::filter_inline_styles(&text, &props)),
                    _ => None,
                };
                if let Some((new, n)) = outcome {
                    if n > 0 {
                        declarations += n;
                        changes.push((file.path.clone(), new));
                    }
                }
            }
        }
        (changes, declarations)
    }

    fn present_filter_style_dialog(&mut self, sender: &ComponentSender<Self>) {
        // (row label, the properties it takes with it)
        const GROUPS: [(&str, &[&str]); 6] = [
            ("Colours", &["color", "background", "background-color"]),
            ("Fonts", &["font", "font-family", "font-size", "font-weight", "font-style"]),
            ("Margins", &["margin-"]),
            ("Padding", &["padding-"]),
            ("Text alignment", &["text-align", "text-indent", "line-height"]),
            ("Borders", &["border-"]),
        ];
        let texts: Vec<(FileKind, String)> = self
            .container
            .as_ref()
            .map(|c| {
                c.files
                    .iter()
                    .filter(|f| {
                        matches!(f.kind, FileKind::Css | FileKind::Html)
                            && !self.removed_paths.contains(&f.path)
                    })
                    .filter_map(|f| self.file_text(&f.path).map(|t| (f.kind, t)))
                    .collect()
            })
            .unwrap_or_default();
        let chosen: Rc<std::cell::Cell<[bool; 6]>> =
            Rc::new(std::cell::Cell::new([true, false, false, false, false, false]));
        let shell = dialog_shell(
            DialogSize::Form,
            Some("Remove chosen CSS wherever it appears"),
        );
        let count_label = gtk::Label::new(None);
        count_label.add_css_class("kalam-ui-meta");
        count_label.set_halign(gtk::Align::Start);

        let recount: Rc<dyn Fn()> = {
            let chosen = chosen.clone();
            let count_label = count_label.clone();
            Rc::new(move || {
                let filter: Vec<&str> = GROUPS
                    .iter()
                    .zip(chosen.get().iter())
                    .filter(|(_, on)| **on)
                    .flat_map(|((_, props), _)| props.iter().copied())
                    .collect();
                let (mut decls, mut files) = (0usize, 0usize);
                for (kind, text) in &texts {
                    let (_, n) = match kind {
                        FileKind::Css => {
                            crate::epub_tools::filter_stylesheet(text, &filter)
                        }
                        _ => crate::epub_tools::filter_inline_styles(text, &filter),
                    };
                    if n > 0 {
                        decls += n;
                        files += 1;
                    }
                }
                count_label.set_label(&if decls == 0 {
                    "Removes nothing with these choices".to_string()
                } else {
                    format!(
                        "Removes {decls} {} in {files} {}",
                        if decls == 1 { "declaration" } else { "declarations" },
                        if files == 1 { "file" } else { "files" }
                    )
                });
            })
        };

        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for (index, (name, props)) in GROUPS.iter().enumerate() {
            let row = gtk::Button::new();
            row.add_css_class("kalam-ed-tickrow");
            if index > 0 {
                row.add_css_class("kalam-ed-report-row");
            }
            row.set_size_request(-1, 44);
            let inner = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            let circle = tick_circle(chosen.get()[index]);
            inner.append(&circle);
            let label = gtk::Label::new(Some(name));
            label.set_halign(gtk::Align::Start);
            label.set_valign(gtk::Align::Center);
            label.set_hexpand(true);
            inner.append(&label);
            let shown: Vec<String> = props
                .iter()
                .map(|p| p.strip_suffix('-').map(|s| format!("{s}-*")).unwrap_or_else(|| p.to_string()))
                .collect();
            let meta = gtk::Label::new(Some(&shown.join(", ")));
            meta.add_css_class("kalam-ui-meta");
            meta.set_valign(gtk::Align::Center);
            inner.append(&meta);
            row.set_child(Some(&inner));
            let chosen = chosen.clone();
            let recount = recount.clone();
            row.connect_clicked(move |_| {
                let mut state = chosen.get();
                state[index] = !state[index];
                chosen.set(state);
                set_tick_circle(&circle, state[index]);
                recount();
            });
            list.append(&row);
        }
        shell.body().append(&list);
        shell.body().append(&count_label);
        recount();

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let slot_cancel = slot.clone();
        shell.footer().append(&boxed_ghost_button("Cancel", move || {
            close_slot(&slot_cancel);
        }));
        {
            let slot = slot.clone();
            let chosen = chosen.clone();
            let s = sender.input_sender().clone();
            shell.footer().append(&primary_button("Remove", move || {
                let filter: Vec<String> = GROUPS
                    .iter()
                    .zip(chosen.get().iter())
                    .filter(|(_, on)| **on)
                    .flat_map(|((_, props), _)| {
                        props.iter().map(|p| p.to_string())
                    })
                    .collect();
                close_slot(&slot);
                let _ = s.send(EpubEditorMsg::RunFilterStyle(filter));
            }));
        }
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Filter style information",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    // ── E5: Upgrade book internals (decision 129) ──

    /// What an upgrade would do to this book, as dialog rows and
    /// a nothing-to-do flag.
    fn upgrade_facts(&self) -> Vec<(&'static str, String, bool)> {
        let Some(container) = &self.container else {
            return Vec::new();
        };
        let opf = self
            .file_text(&container.opf_path)
            .unwrap_or_default();
        let version = crate::epub_structure::opf_package_version(&opf);
        let modern = version
            .as_deref()
            .map(|v| v.starts_with('3'))
            .unwrap_or(false);
        let has_modified = opf.contains("dcterms:modified");
        let has_nav = container.nav_path.is_some();
        vec![
            (
                "Package version",
                if modern {
                    "Already 3.0".to_string()
                } else {
                    format!("{} → 3.0", version.as_deref().unwrap_or("Unwritten"))
                },
                !modern,
            ),
            (
                "Modified stamp",
                if has_modified {
                    "Present".to_string()
                } else {
                    "Will be added".to_string()
                },
                !has_modified,
            ),
            (
                "Navigation document",
                if has_nav {
                    "Present".to_string()
                } else if container.ncx_path.is_some() {
                    "Will be created from the contents".to_string()
                } else {
                    "Will be created from headings".to_string()
                },
                !has_nav,
            ),
        ]
    }

    fn present_upgrade_dialog(&mut self, sender: &ComponentSender<Self>) {
        let Some(book) = self.book.clone() else {
            return;
        };
        let facts = self.upgrade_facts();
        let pending = facts.iter().any(|(_, _, todo)| *todo);
        let shell = dialog_shell(DialogSize::Form, Some(&book.title));
        if !pending {
            let label = gtk::Label::new(Some(
                "This book's internals are up to date.",
            ));
            label.add_css_class("kalam-ui-meta");
            label.set_halign(gtk::Align::Start);
            shell.body().append(&label);
        }
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for (index, (name, state, _)) in facts.iter().enumerate() {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            if index > 0 {
                row.add_css_class("kalam-ed-report-row");
            }
            row.set_size_request(-1, 40);
            let label = gtk::Label::new(Some(name));
            label.set_halign(gtk::Align::Start);
            label.set_valign(gtk::Align::Center);
            label.set_hexpand(true);
            row.append(&label);
            let meta = gtk::Label::new(Some(state));
            meta.add_css_class("kalam-ui-meta");
            meta.set_valign(gtk::Align::Center);
            row.append(&meta);
            list.append(&row);
        }
        shell.body().append(&list);

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        if pending {
            let slot_cancel = slot.clone();
            shell.footer().append(&boxed_ghost_button("Cancel", move || {
                close_slot(&slot_cancel);
            }));
            let slot2 = slot.clone();
            let s = sender.input_sender().clone();
            shell.footer().append(&primary_button("Upgrade", move || {
                close_slot(&slot2);
                let _ = s.send(EpubEditorMsg::RunUpgradeInternals);
            }));
        } else {
            let slot_done = slot.clone();
            shell.footer().append(&primary_button("Done", move || {
                close_slot(&slot_done);
            }));
        }
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Upgrade book internals",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    fn run_upgrade_internals(&mut self, sender: &ComponentSender<Self>) {
        let Some(container) = &self.container else {
            return;
        };
        let opf_path = container.opf_path.clone();
        let Some(opf) = self.file_text(&opf_path) else {
            return;
        };
        let mut plan = StructurePlan::new(
            "Upgraded the book's internals".to_string(),
            "Before upgrade internals",
        );
        let upgraded =
            crate::epub_structure::upgrade_opf(&opf, &crate::db::chrono_like_now());
        if upgraded != opf {
            plan.text_changes.push((opf_path.clone(), upgraded));
        }
        if container.nav_path.is_none() {
            let mut entries = match &container.ncx_path {
                Some(ncx) => self
                    .file_text(ncx)
                    .map(|text| crate::epub_structure::parse_ncx(&text, ncx))
                    .unwrap_or_default(),
                None => Vec::new(),
            };
            if entries.is_empty() {
                let mut spined: Vec<&EpubFile> = container
                    .files
                    .iter()
                    .filter(|f| {
                        f.kind == FileKind::Html
                            && f.spine_index.is_some()
                            && !f.is_nav
                    })
                    .collect();
                spined.sort_by_key(|f| f.spine_index);
                let files: Vec<(String, String)> = spined
                    .iter()
                    .map(|f| {
                        (
                            f.path.clone(),
                            self.file_text(&f.path).unwrap_or_default(),
                        )
                    })
                    .collect();
                entries = crate::epub_structure::generate_from_headings(&files);
            }
            if !entries.is_empty() {
                let dir = crate::epub_structure::parent_dir(&opf_path);
                let mut nav_path = format!("{dir}nav.xhtml");
                let mut n = 1;
                while container.file(&nav_path).is_some() {
                    n += 1;
                    nav_path = format!("{dir}nav{n}.xhtml");
                }
                let nav_text =
                    crate::epub_structure::render_nav(None, &entries, &nav_path);
                plan.new_files.push((
                    EpubFile {
                        path: nav_path.clone(),
                        kind: FileKind::Html,
                        media_type: "application/xhtml+xml".to_string(),
                        spine_index: None,
                        is_nav: true,
                        is_cover_image: false,
                        in_manifest: true,
                    },
                    nav_text.into_bytes(),
                ));
                let mut order = self.spine_order_paths();
                order.push(nav_path);
                plan.spine_order = Some(order);
            }
        }
        if plan.text_changes.is_empty() && plan.new_files.is_empty() {
            self.show_pill("Already up to date", false, sender);
            return;
        }
        self.finish_structure(plan, sender);
    }

    // ── E5: Subset and Embed fonts (decision 141) ──

    /// Every character the book's visible text uses, plus the
    /// printable ASCII range so a subset font can still set a
    /// line the author writes tomorrow.
    fn book_chars(&self) -> HashSet<char> {
        let mut chars: HashSet<char> =
            (0x20u8..=0x7e).map(|b| b as char).collect();
        if let Some(container) = &self.container {
            for file in &container.files {
                if file.kind != FileKind::Html
                    || self.removed_paths.contains(&file.path)
                {
                    continue;
                }
                if let Some(text) = self.file_text(&file.path) {
                    chars.extend(strip_map(&text).text.chars());
                }
            }
        }
        chars
    }

    fn start_subset_fonts(
        &mut self,
        preselect: Option<String>,
        sender: &ComponentSender<Self>,
    ) {
        let fonts: Vec<(String, Vec<u8>)> = self
            .container
            .as_ref()
            .map(|c| {
                c.files
                    .iter()
                    .filter(|f| {
                        f.kind == FileKind::Font
                            && !self.removed_paths.contains(&f.path)
                    })
                    .filter_map(|f| {
                        self.file_bytes(&f.path).map(|b| (f.path.clone(), b))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if fonts.is_empty() {
            self.show_pill("No fonts in this book", false, sender);
            return;
        }
        let chars = self.book_chars();
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Measuring the subsets",
            move |_reporter| {
                fonts
                    .into_iter()
                    .map(|(path, bytes)| SubsetPreview {
                        label: service
                            .font_family(&bytes)
                            .unwrap_or_else(|| file_name(&path).to_string()),
                        original: bytes.len(),
                        outcome: service.editor_subset_font(&bytes, &chars),
                        path,
                    })
                    .collect::<Vec<_>>()
            },
            |_| {},
            move |previews| {
                let _ = s.send(EpubEditorMsg::SubsetPreviewsReady(
                    previews,
                    preselect.clone(),
                ));
            },
        );
    }

    fn present_subset_fonts_dialog(
        &mut self,
        previews: Vec<SubsetPreview>,
        preselect: Option<String>,
        sender: &ComponentSender<Self>,
    ) {
        let shrinkable = |p: &SubsetPreview| {
            p.outcome
                .as_ref()
                .map(|bytes| bytes.len() < p.original)
                .unwrap_or(false)
        };
        let ticked: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(
            previews
                .iter()
                .map(|p| {
                    shrinkable(p)
                        && preselect
                            .as_ref()
                            .map(|want| want == &p.path)
                            .unwrap_or(true)
                })
                .collect(),
        ));
        let previews: Rc<Vec<SubsetPreview>> = Rc::new(previews);
        let shell = dialog_shell(
            DialogSize::Form,
            Some("Keeps only the characters the book uses"),
        );
        let saving_label = gtk::Label::new(None);
        saving_label.add_css_class("kalam-ui-meta");
        saving_label.set_halign(gtk::Align::Start);
        saving_label.set_xalign(0.0);
        saving_label.set_wrap(true);

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let subset_btn = {
            let slot = slot.clone();
            let ticked = ticked.clone();
            let previews = previews.clone();
            let s = sender.input_sender().clone();
            primary_button("Subset", move || {
                let chosen: Vec<(String, Vec<u8>)> = previews
                    .iter()
                    .zip(ticked.borrow().iter())
                    .filter(|(_, on)| **on)
                    .filter_map(|(p, _)| {
                        p.outcome
                            .as_ref()
                            .ok()
                            .map(|bytes| (p.path.clone(), bytes.clone()))
                    })
                    .collect();
                close_slot(&slot);
                let _ = s.send(EpubEditorMsg::RunSubsetFonts(chosen));
            })
        };
        let recount: Rc<dyn Fn()> = {
            let ticked = ticked.clone();
            let previews = previews.clone();
            let saving_label = saving_label.clone();
            let subset_btn = subset_btn.clone();
            Rc::new(move || {
                let mut saving = 0usize;
                let mut any = false;
                for (p, on) in previews.iter().zip(ticked.borrow().iter()) {
                    if *on {
                        if let Ok(bytes) = &p.outcome {
                            saving += p.original.saturating_sub(bytes.len());
                            any = true;
                        }
                    }
                }
                subset_btn.set_sensitive(any);
                saving_label.set_label(&if saving > 0 {
                    format!(
                        "Saves about {}. Fonts you add later may need \
                         subsetting again.",
                        human_size(saving)
                    )
                } else {
                    "Fonts you add later may need subsetting again.".to_string()
                });
            })
        };

        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for (index, preview) in previews.iter().enumerate() {
            let (meta_text, can) = match &preview.outcome {
                Ok(bytes) if bytes.len() < preview.original => (
                    format!(
                        "{} → {}",
                        human_size(preview.original),
                        human_size(bytes.len())
                    ),
                    true,
                ),
                Ok(_) => (format!("{} · no saving", human_size(preview.original)), false),
                Err(message) => {
                    let why = if message.contains("WOFF2") {
                        "WOFF2 — cannot subset yet"
                    } else if message.contains("CFF") {
                        "CFF — cannot subset yet"
                    } else {
                        "Cannot subset"
                    };
                    (why.to_string(), false)
                }
            };
            let row = gtk::Button::new();
            row.add_css_class("kalam-ed-tickrow");
            if index > 0 {
                row.add_css_class("kalam-ed-report-row");
            }
            row.set_size_request(-1, 48);
            row.set_sensitive(can);
            let inner = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            let circle = tick_circle(ticked.borrow()[index]);
            inner.append(&circle);
            let label = gtk::Label::new(Some(&preview.label));
            label.set_halign(gtk::Align::Start);
            label.set_valign(gtk::Align::Center);
            label.set_hexpand(true);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            inner.append(&label);
            let meta = gtk::Label::new(Some(&meta_text));
            meta.add_css_class("kalam-ui-meta");
            meta.set_valign(gtk::Align::Center);
            inner.append(&meta);
            row.set_child(Some(&inner));
            {
                let ticked = ticked.clone();
                let recount = recount.clone();
                row.connect_clicked(move |_| {
                    let now = {
                        let mut all = ticked.borrow_mut();
                        all[index] = !all[index];
                        all[index]
                    };
                    set_tick_circle(&circle, now);
                    recount();
                });
            }
            list.append(&row);
        }
        shell.body().append(&list);
        shell.body().append(&saving_label);
        recount();

        let slot_cancel = slot.clone();
        shell.footer().append(&boxed_ghost_button("Cancel", move || {
            close_slot(&slot_cancel);
        }));
        shell.footer().append(&subset_btn);
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Subset fonts",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    fn start_embed_fonts(&mut self, sender: &ComponentSender<Self>) {
        let mut used: Vec<String> = Vec::new();
        let mut embedded: Vec<Vec<u8>> = Vec::new();
        if let Some(container) = &self.container {
            for file in &container.files {
                if self.removed_paths.contains(&file.path) {
                    continue;
                }
                match file.kind {
                    FileKind::Css => {
                        if let Some(text) = self.file_text(&file.path) {
                            for rule in crate::epub_css::parse_rules(&text) {
                                for decl in &rule.declarations {
                                    let prop = decl.property.to_ascii_lowercase();
                                    if prop == "font-family" {
                                        for part in decl.value.split(',') {
                                            let name = part
                                                .trim()
                                                .trim_matches(|c| c == '"' || c == '\'')
                                                .trim()
                                                .to_string();
                                            if !name.is_empty() {
                                                used.push(name);
                                            }
                                        }
                                    } else if prop == "font" {
                                        // The shorthand: only quoted
                                        // family names are read, the
                                        // rest is sizes and weights.
                                        for part in decl.value.split(',') {
                                            let part = part.trim();
                                            if let Some(start) =
                                                part.find(['"', '\''])
                                            {
                                                let rest = &part[start + 1..];
                                                if let Some(end) =
                                                    rest.find(part[start..].chars().next().unwrap_or('"'))
                                                {
                                                    let name = rest[..end].trim().to_string();
                                                    if !name.is_empty() {
                                                        used.push(name);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                    FileKind::Font => {
                        if let Some(bytes) = self.file_bytes(&file.path) {
                            embedded.push(bytes);
                        }
                    }
                    _ => {}
                }
            }
        }
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Checking which fonts are missing",
            move |_reporter| service.embed_font_candidates(used, embedded),
            |_| {},
            move |candidates| {
                let _ = s.send(EpubEditorMsg::EmbedScanReady(candidates));
            },
        );
    }

    fn present_embed_fonts_dialog(
        &mut self,
        candidates: Vec<(String, Option<std::path::PathBuf>)>,
        sender: &ComponentSender<Self>,
    ) {
        let shell = dialog_shell(
            DialogSize::Form,
            Some("Used by your CSS but not in the book"),
        );
        let ticked: Rc<RefCell<Vec<bool>>> = Rc::new(RefCell::new(
            candidates.iter().map(|(_, p)| p.is_some()).collect(),
        ));
        let candidates: Rc<Vec<(String, Option<std::path::PathBuf>)>> =
            Rc::new(candidates);
        if candidates.is_empty() {
            let label = gtk::Label::new(Some(
                "Every font your CSS names is already in the book.",
            ));
            label.add_css_class("kalam-ui-meta");
            label.set_halign(gtk::Align::Start);
            shell.body().append(&label);
        }
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for (index, (family, found)) in candidates.iter().enumerate() {
            let row = gtk::Button::new();
            row.add_css_class("kalam-ed-tickrow");
            if index > 0 {
                row.add_css_class("kalam-ed-report-row");
            }
            row.set_size_request(-1, 48);
            row.set_sensitive(found.is_some());
            let inner = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            if found.is_some() {
                let circle = tick_circle(true);
                inner.append(&circle);
                let ticked = ticked.clone();
                row.connect_clicked(move |_| {
                    let now = {
                        let mut all = ticked.borrow_mut();
                        all[index] = !all[index];
                        all[index]
                    };
                    set_tick_circle(&circle, now);
                });
            } else {
                let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                spacer.set_size_request(18, 18);
                inner.append(&spacer);
            }
            let label = gtk::Label::new(Some(family));
            label.set_halign(gtk::Align::Start);
            label.set_valign(gtk::Align::Center);
            label.set_hexpand(true);
            inner.append(&label);
            let meta = gtk::Label::new(Some(if found.is_some() {
                "On this computer"
            } else {
                "Not found on this computer"
            }));
            meta.add_css_class("kalam-ui-meta");
            meta.set_valign(gtk::Align::Center);
            inner.append(&meta);
            row.set_child(Some(&inner));
            list.append(&row);
        }
        shell.body().append(&list);
        if !candidates.is_empty() {
            let licence = gtk::Label::new(Some(
                "Check that the font licence allows embedding.",
            ));
            licence.add_css_class("kalam-ui-meta");
            licence.set_halign(gtk::Align::Start);
            shell.body().append(&licence);
        }

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        if candidates.is_empty() {
            let slot_done = slot.clone();
            shell.footer().append(&primary_button("Done", move || {
                close_slot(&slot_done);
            }));
        } else {
            let slot_cancel = slot.clone();
            shell.footer().append(&boxed_ghost_button("Cancel", move || {
                close_slot(&slot_cancel);
            }));
            let slot2 = slot.clone();
            let ticked = ticked.clone();
            let candidates = candidates.clone();
            let any_found = candidates.iter().any(|(_, p)| p.is_some());
            let s = sender.input_sender().clone();
            let embed_btn = primary_button("Embed", move || {
                let chosen: Vec<(String, std::path::PathBuf)> = candidates
                    .iter()
                    .zip(ticked.borrow().iter())
                    .filter(|(_, on)| **on)
                    .filter_map(|((family, found), _)| {
                        found.clone().map(|path| (family.clone(), path))
                    })
                    .collect();
                close_slot(&slot2);
                let _ = s.send(EpubEditorMsg::RunEmbedFonts(chosen));
            });
            embed_btn.set_sensitive(any_found);
            shell.footer().append(&embed_btn);
        }
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Embed fonts",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    fn run_embed_loaded(
        &mut self,
        loaded: Vec<(String, String, Vec<u8>)>,
        sender: &ComponentSender<Self>,
    ) {
        let Some(container) = &self.container else {
            return;
        };
        if loaded.is_empty() {
            self.show_pill("Nothing to embed", false, sender);
            return;
        }
        let Some(css_path) = container
            .files
            .iter()
            .find(|f| f.kind == FileKind::Css)
            .map(|f| f.path.clone())
        else {
            self.show_pill(
                "Add a stylesheet first — @font-face has nowhere to live",
                false,
                sender,
            );
            return;
        };
        let dir = container
            .files
            .iter()
            .find(|f| f.kind == FileKind::Font)
            .map(|f| crate::epub_structure::parent_dir(&f.path))
            .unwrap_or_else(|| crate::epub_structure::parent_dir(&container.opf_path));
        let summary = if loaded.len() == 1 {
            format!("Embedded {}", loaded[0].0)
        } else {
            format!("Embedded {} fonts", loaded.len())
        };
        let mut plan = StructurePlan::new(summary, "Before embed fonts");
        let mut faces = String::new();
        let mut taken: Vec<String> =
            container.files.iter().map(|f| f.path.clone()).collect();
        for (family, name, bytes) in loaded {
            let mut path = format!("{dir}{}", file_name(&name));
            let mut n = 1;
            while taken.contains(&path) {
                n += 1;
                let bare = file_name(&name);
                let (stem, ext) = match bare.rsplit_once('.') {
                    Some((stem, ext)) => (stem.to_string(), format!(".{ext}")),
                    None => (bare.to_string(), String::new()),
                };
                path = format!("{dir}{stem}{n}{ext}");
            }
            taken.push(path.clone());
            let href = crate::epub_structure::rel_href(
                &crate::epub_structure::parent_dir(&css_path),
                &path,
                None,
            );
            faces.push_str(&format!(
                "@font-face {{ font-family: \"{family}\"; src: url(\"{href}\"); }}\n"
            ));
            let media = crate::epub_container::guess_media_type(&path);
            plan.new_files.push((
                EpubFile {
                    path,
                    kind: FileKind::Font,
                    media_type: media.to_string(),
                    spine_index: None,
                    is_nav: false,
                    is_cover_image: false,
                    in_manifest: true,
                },
                bytes,
            ));
        }
        let css_text = self.file_text(&css_path).unwrap_or_default();
        let new_css = match css_text
            .lines()
            .next()
            .filter(|line| line.trim_start().to_ascii_lowercase().starts_with("@charset"))
        {
            Some(first) => {
                let at = first.len() + 1;
                format!(
                    "{}{}\n{}",
                    &css_text[..at.min(css_text.len())],
                    faces.trim_end(),
                    &css_text[at.min(css_text.len())..]
                )
            }
            None => format!("{faces}{css_text}"),
        };
        plan.text_changes.push((css_path, new_css));
        self.finish_structure(plan, sender);
    }

    // ── E5: Compress images + Check external links ──

    fn start_compress_images(&mut self, sender: &ComponentSender<Self>) {
        let images: Vec<(String, Vec<u8>)> = self
            .container
            .as_ref()
            .map(|c| {
                c.files
                    .iter()
                    .filter(|f| {
                        f.kind == FileKind::Image
                            && !self.removed_paths.contains(&f.path)
                    })
                    .filter_map(|f| {
                        self.file_bytes(&f.path).map(|b| (f.path.clone(), b))
                    })
                    .collect()
            })
            .unwrap_or_default();
        if images.is_empty() {
            self.show_pill("No images in this book", false, sender);
            return;
        }
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Measuring image savings",
            move |_reporter| {
                let mut lossless = Vec::new();
                let mut smaller = Vec::new();
                for (path, bytes) in &images {
                    if let Ok(out) = service.compress_book_image(bytes, false) {
                        if out.len() < bytes.len() {
                            lossless.push((path.clone(), out));
                        }
                    }
                    if let Ok(out) = service.compress_book_image(bytes, true) {
                        if out.len() < bytes.len() {
                            smaller.push((path.clone(), out));
                        }
                    }
                }
                (lossless, smaller)
            },
            |_| {},
            move |pair| {
                let _ =
                    s.send(EpubEditorMsg::CompressPreviewReady(Box::new(pair)));
            },
        );
    }

    fn present_compress_dialog(
        &mut self,
        lossless: Vec<(String, Vec<u8>)>,
        smaller: Vec<(String, Vec<u8>)>,
        sender: &ComponentSender<Self>,
    ) {
        let images = self
            .container
            .as_ref()
            .map(|c| {
                c.files
                    .iter()
                    .filter(|f| f.kind == FileKind::Image)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let total: usize = images
            .iter()
            .map(|f| self.file_bytes(&f.path).map(|b| b.len()).unwrap_or(0))
            .sum();
        let n = images.len();
        let subtitle = format!(
            "{n} {} · {}",
            if n == 1 { "image" } else { "images" },
            human_size(total)
        );
        let shell = dialog_shell(DialogSize::Form, Some(&subtitle));
        // Savings per mode, from the real re-encoded bytes.
        let saving = |wins: &Vec<(String, Vec<u8>)>| -> usize {
            wins.iter()
                .map(|(path, out)| {
                    self.file_bytes(path)
                        .map(|b| b.len())
                        .unwrap_or(0)
                        .saturating_sub(out.len())
                })
                .sum()
        };
        let lossless_saving = saving(&lossless);
        let smaller_saving = saving(&smaller);
        let wins: Rc<(ByteChanges, ByteChanges)> =
            Rc::new((lossless, smaller));
        let mode = Rc::new(std::cell::Cell::new(false)); // false = Lossless

        let body_label = gtk::Label::new(None);
        body_label.set_halign(gtk::Align::Start);
        body_label.set_xalign(0.0);
        body_label.set_wrap(true);

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let compress_btn = {
            let slot = slot.clone();
            let wins = wins.clone();
            let mode = mode.clone();
            let s = sender.input_sender().clone();
            primary_button("Compress", move || {
                let chosen = if mode.get() { wins.1.clone() } else { wins.0.clone() };
                close_slot(&slot);
                let _ = s.send(EpubEditorMsg::RunCompressImages(chosen));
            })
        };
        let refresh_body: Rc<dyn Fn()> = {
            let mode = mode.clone();
            let body_label = body_label.clone();
            let compress_btn = compress_btn.clone();
            Rc::new(move || {
                let (text, any) = if mode.get() {
                    if smaller_saving > 0 {
                        (
                            format!(
                                "JPEGs take a slightly gentler quality. \
                                 Estimated saving {}.",
                                human_size(smaller_saving)
                            ),
                            true,
                        )
                    } else {
                        ("No further saving that way.".to_string(), false)
                    }
                } else if lossless_saving > 0 {
                    (
                        format!(
                            "Same pixels, smaller files. Estimated saving {}.",
                            human_size(lossless_saving)
                        ),
                        true,
                    )
                } else {
                    (
                        "Every image is already as small as lossless gets."
                            .to_string(),
                        false,
                    )
                };
                body_label.set_label(&text);
                compress_btn.set_sensitive(any);
            })
        };

        // The mode tabs: text tabs, flat, 28px.
        let tabs = gtk::Box::new(gtk::Orientation::Horizontal, 20);
        tabs.set_margin_start(2);
        let tab_btns: Rc<RefCell<Vec<gtk::Button>>> = Rc::new(RefCell::new(Vec::new()));
        for (i, title) in ["Lossless", "Smaller"].into_iter().enumerate() {
            let btn = gtk::Button::with_label(title);
            btn.add_css_class("kalam-ed-rep-tab");
            if i == 0 {
                btn.add_css_class("active");
            }
            {
                let mode = mode.clone();
                let refresh_body = refresh_body.clone();
                let tab_btns = tab_btns.clone();
                btn.connect_clicked(move |clicked| {
                    mode.set(i == 1);
                    for other in tab_btns.borrow().iter() {
                        other.remove_css_class("active");
                    }
                    clicked.add_css_class("active");
                    refresh_body();
                });
            }
            tab_btns.borrow_mut().push(btn.clone());
            tabs.append(&btn);
        }
        shell.body().append(&tabs);
        shell.body().append(&body_label);
        refresh_body();

        let slot_cancel = slot.clone();
        shell.footer().append(&boxed_ghost_button("Cancel", move || {
            close_slot(&slot_cancel);
        }));
        shell.footer().append(&compress_btn);
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Compress images",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    /// Decision 141's only network tool: it runs solely when the
    /// author presses Start, and opening a book never touches
    /// the network.
    fn present_check_links_dialog(&mut self, _sender: &ComponentSender<Self>) {
        let mut links: Vec<(String, String, usize)> = Vec::new(); // url, file, line
        if let Some(container) = &self.container {
            for file in &container.files {
                if !(file.kind == FileKind::Html || file.kind == FileKind::Css)
                    || self.removed_paths.contains(&file.path)
                {
                    continue;
                }
                let Some(text) = self.file_text(&file.path) else {
                    continue;
                };
                for found in crate::epub_check::find_refs(&text, false) {
                    let url = found.href.trim();
                    let lower = url.to_ascii_lowercase();
                    if !(lower.starts_with("http://") || lower.starts_with("https://")) {
                        continue;
                    }
                    let url = url.split('#').next().unwrap_or(url).to_string();
                    if url.is_empty()
                        || links.iter().any(|(seen, _, _)| *seen == url)
                    {
                        continue;
                    }
                    links.push((url, file.path.clone(), found.line));
                }
            }
        }
        let shell = dialog_shell(
            DialogSize::List,
            Some("Kalam asks the web about each link"),
        );
        let total = links.len();
        let mut status_labels: Vec<gtk::Label> = Vec::new();
        let mut source_labels: Vec<gtk::Label> = Vec::new();
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        if links.is_empty() {
            let label = gtk::Label::new(Some(
                "This book links nowhere outside itself.",
            ));
            label.add_css_class("kalam-ui-meta");
            label.set_halign(gtk::Align::Start);
            list.append(&label);
        }
        for (index, (url, file, line)) in links.iter().enumerate() {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            row.set_margin_top(9);
            row.set_margin_bottom(9);
            if index > 0 {
                row.add_css_class("kalam-ed-report-row");
            }
            let texts = gtk::Box::new(gtk::Orientation::Vertical, 2);
            texts.set_hexpand(true);
            let url_label = gtk::Label::new(Some(url));
            url_label.add_css_class("kalam-ed-mono");
            url_label.set_halign(gtk::Align::Start);
            url_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            texts.append(&url_label);
            let source = gtk::Label::new(Some(&format!(
                "{} · line {line}",
                file_name(file)
            )));
            source.add_css_class("kalam-ui-meta");
            source.set_halign(gtk::Align::Start);
            source.set_visible(false);
            texts.append(&source);
            source_labels.push(source);
            row.append(&texts);
            let status = gtk::Label::new(Some("Not checked"));
            status.add_css_class("kalam-ui-meta");
            status.set_valign(gtk::Align::Start);
            row.append(&status);
            status_labels.push(status);
            list.append(&row);
        }
        let scrolled = gtk::ScrolledWindow::new();
        scrolled.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scrolled.set_vexpand(true);
        scrolled.set_min_content_height(220);
        scrolled.set_child(Some(&list));
        shell.body().append(&scrolled);

        let progress = gtk::ProgressBar::new();
        progress.add_css_class("kalam-ed-progress");
        progress.set_visible(false);
        shell.body().append(&progress);
        let checked_label = gtk::Label::new(Some(""));
        checked_label.add_css_class("kalam-ui-meta");
        checked_label.set_halign(gtk::Align::Start);
        shell.body().append(&checked_label);

        let stop_flag = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let stop_btn = {
            let flag = stop_flag.clone();
            boxed_ghost_button("Stop", move || {
                flag.store(true, std::sync::atomic::Ordering::SeqCst);
            })
        };
        stop_btn.set_visible(false);
        let primary = gtk::Button::with_label("Start checking");
        primary.add_css_class("kalam-ui-pill-button");
        primary.add_css_class("boxed-primary");
        primary.set_size_request(96, 34);
        primary.set_sensitive(total > 0);
        if total == 0 {
            primary.set_label("Done");
            primary.set_sensitive(true);
        }
        {
            let service = self.service.clone();
            let urls: Vec<String> =
                links.into_iter().map(|(url, _, _)| url).collect();
            let labels = Rc::new(status_labels);
            let sources = Rc::new(source_labels);
            let started = Rc::new(std::cell::Cell::new(false));
            let slot2 = slot.clone();
            let flag = stop_flag.clone();
            let stop_btn2 = stop_btn.clone();
            let progress2 = progress.clone();
            let checked2 = checked_label.clone();
            primary.connect_clicked(move |btn| {
                if started.get() {
                    // Now wearing the Done hat.
                    flag.store(true, std::sync::atomic::Ordering::SeqCst);
                    close_slot(&slot2);
                    return;
                }
                started.set(true);
                btn.set_label("Done");
                stop_btn2.set_visible(true);
                progress2.set_visible(true);
                checked2.set_label(&format!("0 of {total} checked"));
                let urls = urls.clone();
                let service = service.clone();
                let flag2 = flag.clone();
                let labels2 = labels.clone();
                let sources2 = sources.clone();
                let progress3 = progress2.clone();
                let checked3 = checked2.clone();
                let checked4 = checked2.clone();
                let stop3 = stop_btn2.clone();
                crate::tasks::spawn_internal(
                    "Checking external links",
                    move |reporter| {
                        let mut done = 0usize;
                        for (index, url) in urls.iter().enumerate() {
                            if flag2.load(std::sync::atomic::Ordering::SeqCst) {
                                break;
                            }
                            let verdict = service.check_external_link(url);
                            done += 1;
                            reporter.step(done, urls.len(), format!("{index}|{verdict}"));
                        }
                        done
                    },
                    move |update| {
                        if let Some((index, verdict)) =
                            update.detail.split_once('|')
                        {
                            if let Ok(index) = index.parse::<usize>() {
                                if let Some(label) = labels2.get(index) {
                                    label.set_label(verdict);
                                }
                                if verdict != "Works" {
                                    if let Some(source) = sources2.get(index) {
                                        source.set_visible(true);
                                    }
                                }
                            }
                        }
                        progress3.set_fraction(
                            update.done as f64 / update.total.max(1) as f64,
                        );
                        checked3.set_label(&format!(
                            "{} of {} checked",
                            update.done, update.total
                        ));
                    },
                    move |done: usize| {
                        stop3.set_visible(false);
                        if done < total {
                            checked4.set_label(&format!(
                                "Stopped — {done} of {total} checked"
                            ));
                        } else {
                            checked4.set_label(&format!("{total} of {total} checked"));
                        }
                    },
                );
            });
        }
        shell.footer().append(&stop_btn);
        shell.footer().append(&primary);
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Check external links",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
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
        // Files born during this session must be written even
        // when neither a buffer edit nor a pending edit names
        // them (their bytes live in the container).
        for path in &self.added_paths {
            if !replacements.contains_key(path) {
                if let Some(bytes) = self.file_bytes(path) {
                    replacements.insert(path.clone(), bytes);
                }
            }
        }
        let removals = self.removed_paths.clone();
        if replacements.is_empty() && removals.is_empty() {
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
            move |_reporter| {
                service.save_editor_changes(&book, &replacements, &removals, "Before save")
            },
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
        self.tree_rows.borrow_mut().clear();
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
        if self.tree_selected.contains(&file.path) {
            row.add_css_class("checked");
        }
        row.set_size_request(-1, 36);
        row.set_tooltip_text(Some(&file.path));
        self.tree_rows
            .borrow_mut()
            .push((file.path.clone(), row.clone()));

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

        // Ctrl+click toggles the merge selection instead of
        // opening (decision 139's "Merge selected").
        {
            let ctrl = self.ctrl_down.clone();
            let path = file.path.clone();
            let s = sender.clone();
            let click = gtk::GestureClick::new();
            click.set_button(1);
            click.connect_pressed(move |gesture, _, _, _| {
                if *ctrl.borrow() {
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                    s.input(EpubEditorMsg::TreeToggleSelect(path.clone()));
                }
            });
            row.add_controller(click);
        }
        // Right-click opens the file's menu (decision 139).
        {
            let path = file.path.clone();
            let s = sender.clone();
            let click = gtk::GestureClick::new();
            click.set_button(3);
            click.connect_pressed(move |gesture, _, _, _| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                s.input(EpubEditorMsg::TreeMenu(path.clone()));
            });
            row.add_controller(click);
        }
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
            let name = gtk::Label::new(Some(if tab.path == CONTENTS_TAB {
                "Contents"
            } else {
                file_name(&tab.path)
            }));
            if !tab.pinned {
                name.add_css_class("kalam-ed-tab-temporary");
            }
            content.append(&name);
            if tab
                .buffer
                .as_ref()
                .map(|b| b.is_modified())
                .unwrap_or(false)
                || (tab.path == CONTENTS_TAB && self.toc_dirty())
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
        for path in &self.added_paths {
            if replacements.contains_key(path) {
                continue;
            }
            if let Some(bytes) = self.file_bytes(path) {
                replacements.insert(path.clone(), bytes);
            }
        }
        let removals = self.removed_paths.clone();
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Preparing preview",
            move |_reporter| service.write_editor_preview(&book, &replacements, &removals),
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
        // A fresh engine forgot the contents mark; re-ring it.
        if self.contents_active() {
            self.mark_toc_target();
        }
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

    /// Text files that point at `target`, each with the 1-based
    /// line of the first pointer — the image/font tabs' "used in
    /// N files" (decision 126) and Reports' unused flags. A
    /// pointer is the href the file would actually write
    /// (relative to its own folder) or the bare file name, which
    /// covers root-relative and in-folder references alike.
    fn referencing_files(&self, target: &str) -> Vec<(String, usize)> {
        let Some(container) = &self.container else {
            return Vec::new();
        };
        let name = file_name(target);
        let mut out = Vec::new();
        for file in &container.files {
            if !file.kind.is_text()
                || file.path == target
                || self.removed_paths.contains(&file.path)
            {
                continue;
            }
            let Some(text) = self.file_text(&file.path) else {
                continue;
            };
            let href = crate::epub_structure::rel_href(
                &crate::epub_structure::parent_dir(&file.path),
                target,
                None,
            );
            let at = text
                .find(&href)
                .or_else(|| if href != name { text.find(name) } else { None });
            if let Some(offset) = at {
                out.push((
                    file.path.clone(),
                    text[..offset].matches('\n').count() + 1,
                ));
            }
        }
        out
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
        menu.add_item_with_hint("Find & replace…", "Ctrl+Shift+F", move || {
            let _ = s.send(EpubEditorMsg::ToggleFind);
        });
        let s = sender.input_sender().clone();
        menu.add_item("Edit contents…", move || {
            let _ = s.send(EpubEditorMsg::OpenContents);
        });
        let s = sender.input_sender().clone();
        menu.add_item_with_hint("Spell check…", "F7", move || {
            let _ = s.send(EpubEditorMsg::OpenSpellCheck);
        });
        let s = sender.input_sender().clone();
        menu.add_item("Personal dictionary…", move || {
            let _ = s.send(EpubEditorMsg::OpenPersonalDict);
        });
        menu.add_separator();
        let insert =
            crate::widgets::menu::menu_popover(crate::widgets::menu::MenuWidth::Tools);
        {
            let s = sender.input_sender().clone();
            insert.add_item("Character…", move || {
                let _ = s.send(EpubEditorMsg::OpenInsertChar);
            });
            let s = sender.input_sender().clone();
            insert.add_item("Link…", move || {
                let _ = s.send(EpubEditorMsg::OpenInsertLink);
            });
            let s = sender.input_sender().clone();
            insert.add_item("Image…", move || {
                let _ = s.send(EpubEditorMsg::OpenInsertImage);
            });
            let s = sender.input_sender().clone();
            insert.add_item("Contents page…", move || {
                let _ = s.send(EpubEditorMsg::OpenInsertInlineContents);
            });
        }
        menu.add_submenu("Insert", insert);
        let structure =
            crate::widgets::menu::menu_popover(crate::widgets::menu::MenuWidth::Tools);
        {
            let s = sender.input_sender().clone();
            structure.add_item("Split file…", move || {
                let _ = s.send(EpubEditorMsg::OpenSplitDialog(0));
            });
            let s = sender.input_sender().clone();
            structure.add_item("Merge files", move || {
                let _ = s.send(EpubEditorMsg::MergeSelected);
            });
            let s = sender.input_sender().clone();
            structure.add_item_with_hint("Rename file…", "F2", move || {
                let _ = s.send(EpubEditorMsg::RenameActive);
            });
            let s = sender.input_sender().clone();
            structure.add_item("Reorder files…", move || {
                let _ = s.send(EpubEditorMsg::OpenReorder);
            });
            let s = sender.input_sender().clone();
            structure.add_item("Set semantics…", move || {
                let _ = s.send(EpubEditorMsg::PalettePickSemantics);
            });
            let s = sender.input_sender().clone();
            structure.add_item("Link stylesheets…", move || {
                let _ = s.send(EpubEditorMsg::PalettePickLinkCss);
            });
            structure.add_separator();
            let s = sender.input_sender().clone();
            structure.add_item("Add files…", move || {
                let _ = s.send(EpubEditorMsg::TreeAddFiles);
            });
        }
        menu.add_submenu("Structure", structure);
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
        {
            let s = sender.input_sender().clone();
            clean.add_item("Filter style information…", move || {
                let _ = s.send(EpubEditorMsg::OpenFilterStyle);
            });
            let s = sender.input_sender().clone();
            clean.add_item("Tag mapper…", move || {
                let _ = s.send(EpubEditorMsg::OpenTagMapper);
            });
            let s = sender.input_sender().clone();
            clean.add_item("Upgrade book internals…", move || {
                let _ = s.send(EpubEditorMsg::OpenUpgradeInternals);
            });
        }
        menu.add_submenu("Clean up", clean);
        let fonts_images =
            crate::widgets::menu::menu_popover(crate::widgets::menu::MenuWidth::Tools);
        {
            let s = sender.input_sender().clone();
            fonts_images.add_item("Subset fonts…", move || {
                let _ = s.send(EpubEditorMsg::OpenSubsetFonts(None));
            });
            let s = sender.input_sender().clone();
            fonts_images.add_item("Embed fonts…", move || {
                let _ = s.send(EpubEditorMsg::OpenEmbedFonts);
            });
            let s = sender.input_sender().clone();
            fonts_images.add_item("Compress images…", move || {
                let _ = s.send(EpubEditorMsg::OpenCompressImages);
            });
            let s = sender.input_sender().clone();
            fonts_images.add_item("Replace cover…", move || {
                let _ = s.send(EpubEditorMsg::OpenReplaceCover);
            });
            let s = sender.input_sender().clone();
            fonts_images.add_item("Check external links…", move || {
                let _ = s.send(EpubEditorMsg::OpenCheckLinks);
            });
        }
        menu.add_submenu("Fonts and images", fonts_images);
        menu.add_separator();
        let s = sender.input_sender().clone();
        menu.add_item("Reports…", move || {
            let _ = s.send(EpubEditorMsg::OpenReports);
        });
        let s = sender.input_sender().clone();
        menu.add_item_with_hint("Command palette…", "Ctrl+Shift+P", move || {
            let _ = s.send(EpubEditorMsg::TogglePalette);
        });
        menu.popover().set_parent(&self.tools_btn);
        menu.popover().popup();
        self.tools_menu = Some(menu);
    }

    /// "This file or Whole book?" (decision 128): two option rows
    /// (section 38) and the tool's own name on the filled button.
    fn present_scope_dialog(&self, kind: InstantTool, sender: &ComponentSender<Self>) {
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

    // ── E4: structural operations (decisions 125, 139) ───────
    //
    // Every structure tool computes a full StructurePlan from
    // the book's current texts, snapshots, then applies. Undo
    // restores the captured working state wholesale — the only
    // honest inverse for renames and removals.

    /// Snapshot, then apply a computed plan.
    fn start_structure(&self, plan: StructurePlan, sender: &ComponentSender<Self>) {
        let Some(book) = self.book.clone() else {
            return;
        };
        let files = plan.files_changed();
        let label = plan.snapshot_label.clone();
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Taking snapshot",
            move |_reporter| {
                service
                    .editor_snapshot(&book, &label, files)
                    .map(|()| plan)
            },
            |_| {},
            move |result| match result {
                Ok(plan) => {
                    let _ = s.send(EpubEditorMsg::ApplyStructure(Box::new(plan)));
                }
                Err(message) => {
                    let _ = s.send(EpubEditorMsg::ActionFailed(message));
                }
            },
        );
    }

    fn capture_structure_undo(&self) -> StructureUndo {
        let (files, bytes) = self
            .container
            .as_ref()
            .map(|c| c.snapshot_state())
            .unwrap_or_default();
        StructureUndo {
            files,
            bytes,
            pending: self.pending.clone(),
            added_paths: self.added_paths.clone(),
            removed_paths: self.removed_paths.clone(),
            tabs: self
                .tabs
                .iter()
                .map(|t| {
                    (
                        t.path.clone(),
                        t.pinned,
                        t.buffer
                            .as_ref()
                            .map(|b| (buffer_text(b), b.is_modified())),
                    )
                })
                .collect(),
            active_path: self.active_path.clone(),
        }
    }

    fn apply_structure(&mut self, plan: StructurePlan, sender: &ComponentSender<Self>) {
        self.structure_undo = Some(self.capture_structure_undo());
        self.tool_undo = None;

        // Pending edits travel with renames.
        for (old, new) in &plan.renames {
            if let Some(text) = self.pending.remove(old) {
                self.pending.insert(new.clone(), text);
            }
        }
        if let Some(container) = &mut self.container {
            for (old, new) in &plan.renames {
                container.rename_file(old, new);
            }
            if let Some(order) = &plan.spine_order {
                for file in &mut container.files {
                    file.spine_index = order.iter().position(|p| p == &file.path);
                }
            }
            if let Some(cover) = &plan.set_cover {
                for file in &mut container.files {
                    file.is_cover_image = &file.path == cover;
                }
            }
            for (file, bytes) in &plan.new_files {
                container.add_file(file.clone(), bytes.clone());
            }
            for path in &plan.removed_files {
                container.remove_file(path);
            }
            for (path, bytes) in &plan.byte_changes {
                container.set_bytes(path, bytes.clone());
            }
        }
        for (old, new) in &plan.renames {
            self.removed_paths.insert(old.clone());
            self.added_paths.remove(old);
            self.added_paths.insert(new.clone());
        }
        for (file, _) in &plan.new_files {
            self.added_paths.insert(file.path.clone());
        }
        for path in &plan.removed_files {
            if !self.added_paths.remove(path) {
                self.removed_paths.insert(path.clone());
            }
            self.pending.remove(path);
            self.tree_selected.retain(|p| p != path);
        }
        for (path, _) in &plan.byte_changes {
            self.added_paths.insert(path.clone());
        }

        // Tabs follow renames and removals.
        for (old, new) in &plan.renames {
            if let Some(tab) = self.tabs.iter_mut().find(|t| &t.path == old) {
                tab.path = new.clone();
                tab.kind = self
                    .container
                    .as_ref()
                    .and_then(|c| c.file(new))
                    .map(|f| f.kind)
                    .unwrap_or(tab.kind);
            }
            if let Some(child) = self.tab_stack.child_by_name(old) {
                self.tab_stack.remove(&child);
                self.tab_stack.add_named(&child, Some(new.as_str()));
            }
            if self.active_path.as_deref() == Some(old.as_str()) {
                self.active_path = Some(new.clone());
                self.tab_stack.set_visible_child_name(new);
            }
        }
        for path in &plan.removed_files {
            self.close_tab_quietly(path);
        }
        // A replaced binary's tab re-renders from the new bytes.
        for (path, _) in &plan.byte_changes {
            let rebuild = self
                .tabs
                .iter()
                .find(|t| &t.path == path)
                .map(|t| (t.buffer.is_none(), t.pinned))
                .unwrap_or((false, false));
            if rebuild.0 {
                self.close_tab_quietly(path);
                self.open_file(path, rebuild.1, sender);
            }
        }

        for (path, text) in &plan.text_changes {
            self.set_text_quiet(path, text);
        }

        // Keep an active tab standing if the op removed it.
        if self
            .active_path
            .as_ref()
            .is_some_and(|p| !self.tabs.iter().any(|t| &t.path == p))
        {
            match self.tabs.last().map(|t| t.path.clone()) {
                Some(next) => self.activate_tab(&next, sender),
                None => {
                    self.active_path = None;
                    self.tab_stack.set_visible_child_name(EMPTY_TAB);
                }
            }
        }
        if self.contents_active() {
            self.maybe_reload_toc();
            self.rebuild_contents(sender);
        }
        self.show_pill(&plan.summary, true, sender);
        self.run_check(sender);
        self.request_preview_write(sender);
        self.sync_chrome(sender);
        self.rebuild_tree(sender);
        self.rebuild_tab_strip(sender);
        self.rebuild_outline(sender);
        if self.find_panel.is_visible() {
            self.run_find(sender);
        }
    }

    /// Take back the last structure run, buffers and all.
    fn undo_structure(&mut self, sender: &ComponentSender<Self>) {
        let Some(undo) = self.structure_undo.take() else {
            return;
        };
        let paths: Vec<String> = self.tabs.iter().map(|t| t.path.clone()).collect();
        for path in paths {
            self.close_tab_quietly(&path);
        }
        if let Some(container) = &mut self.container {
            container.restore_state(undo.files, undo.bytes);
        }
        self.pending = undo.pending;
        self.added_paths = undo.added_paths;
        self.removed_paths = undo.removed_paths;
        self.tree_selected.clear();
        for (path, pinned, saved) in &undo.tabs {
            if path == CONTENTS_TAB {
                self.open_contents(sender);
                continue;
            }
            self.open_file(path, *pinned, sender);
            if let Some((text, modified)) = saved {
                if let Some(buffer) = self
                    .tabs
                    .iter()
                    .find(|t| &t.path == path)
                    .and_then(|t| t.buffer.clone())
                {
                    if buffer_text(&buffer) != *text {
                        let undo_levels = buffer.max_undo_levels();
                        buffer.set_max_undo_levels(0);
                        buffer.set_text(text);
                        buffer.set_max_undo_levels(undo_levels);
                        buffer.place_cursor(&buffer.start_iter());
                    }
                    buffer.set_modified(*modified);
                }
            }
        }
        match undo.active_path {
            Some(path) if self.tabs.iter().any(|t| t.path == path) => {
                self.activate_tab(&path, sender)
            }
            _ => {
                if let Some(last) = self.tabs.last().map(|t| t.path.clone()) {
                    self.activate_tab(&last, sender);
                } else {
                    self.tab_stack.set_visible_child_name(EMPTY_TAB);
                }
            }
        }
        self.hide_pill();
        self.run_check(sender);
        self.request_preview_write(sender);
        self.sync_chrome(sender);
        self.rebuild_tree(sender);
        self.rebuild_tab_strip(sender);
    }

    /// Reading-order paths of the book's spine files.
    fn spine_order_paths(&self) -> Vec<String> {
        let Some(container) = &self.container else {
            return Vec::new();
        };
        let mut spined: Vec<&EpubFile> = container
            .files
            .iter()
            .filter(|f| f.spine_index.is_some())
            .collect();
        spined.sort_by_key(|f| f.spine_index);
        spined.iter().map(|f| f.path.clone()).collect()
    }

    /// Simulate a plan's file-list mutations on a clone, so the
    /// OPF in the plan matches the book it will meet on apply.
    fn simulated_files(&self, plan: &StructurePlan) -> Vec<EpubFile> {
        let mut files: Vec<EpubFile> = self
            .container
            .as_ref()
            .map(|c| c.files.clone())
            .unwrap_or_default();
        for (old, new) in &plan.renames {
            if let Some(file) = files.iter_mut().find(|f| &f.path == old) {
                file.path = new.clone();
                file.kind = crate::epub_container::classify(new, &file.media_type);
            }
        }
        files.retain(|f| !plan.removed_files.contains(&f.path));
        for (file, _) in &plan.new_files {
            files.push(file.clone());
        }
        if let Some(order) = &plan.spine_order {
            for file in &mut files {
                file.spine_index = order.iter().position(|p| p == &file.path);
            }
        }
        if let Some(cover) = &plan.set_cover {
            for file in &mut files {
                file.is_cover_image = &file.path == cover;
            }
        }
        files
    }

    /// Finish a plan: append the OPF rewrite that matches its
    /// simulated end state, then snapshot+apply. Returns false
    /// (with a pill) when the OPF cannot be rebuilt.
    fn finish_structure(
        &mut self,
        mut plan: StructurePlan,
        sender: &ComponentSender<Self>,
    ) {
        let files = self.simulated_files(&plan);
        let ncx = self
            .container
            .as_ref()
            .and_then(|c| c.ncx_path.clone())
            .map(|p| {
                plan.renames
                    .iter()
                    .find(|(old, _)| old == &p)
                    .map(|(_, new)| new.clone())
                    .unwrap_or(p)
            })
            .filter(|p| files.iter().any(|f| &f.path == p));
        let opf_path_now = self
            .container
            .as_ref()
            .map(|c| c.opf_path.clone())
            .unwrap_or_default();
        let base = plan
            .text_changes
            .iter()
            .find(|(p, _)| p == &opf_path_now)
            .map(|(_, t)| t.clone());
        plan.text_changes.retain(|(p, _)| p != &opf_path_now);
        let Some(mut opf_text) =
            self.opf_text_for_files(&files, ncx.as_deref(), base.as_deref())
        else {
            self.show_pill("Could not read the book's package file", false, sender);
            return;
        };
        if let Some(cover) = &plan.set_cover {
            let opf_path = self
                .container
                .as_ref()
                .map(|c| c.opf_path.clone())
                .unwrap_or_default();
            let (items, _, _) =
                crate::epub_structure::parse_opf_data(&opf_text, &opf_path);
            if let Some(id) = items
                .iter()
                .find(|i| &i.href == cover)
                .map(|i| i.id.clone())
            {
                opf_text = crate::epub_structure::set_cover_meta(&opf_text, &id);
            }
        }
        if let Some(container) = &self.container {
            let opf_path = container.opf_path.clone();
            if self.file_text(&opf_path).as_deref() != Some(opf_text.as_str()) {
                plan.text_changes.push((opf_path, opf_text));
            }
        }
        self.start_structure(plan, sender);
    }

    // ── E4: split & merge (decision 139) ───────────────────────

    fn present_split_dialog(&self, preset: u8, sender: &ComponentSender<Self>) {
        let Some(tab) = self.active_tab().filter(|t| t.kind == FileKind::Html) else {
            return;
        };
        let Some(buffer) = tab.buffer.clone() else {
            return;
        };
        let path = tab.path.clone();
        let text = Rc::new(buffer_text(&buffer));
        let cursor = self.cursor_char_offset().unwrap_or(0);
        let mode = Rc::new(std::cell::Cell::new(preset.min(2)));

        let shell = dialog_shell(
            DialogSize::Form,
            Some("Links to the new files are fixed automatically"),
        );
        let rows: Vec<(gtk::Button, gtk::Label)> = [
            option_row("At the cursor", ""),
            option_row("At every heading", ""),
            option_row("By size", "Split when a piece grows past the size below"),
        ]
        .into_iter()
        .collect();
        let size_entry = gtk::Entry::new();
        size_entry.add_css_class("kalam-ed-entry");
        size_entry.set_text("250");
        size_entry.set_width_chars(6);
        let size_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let size_label = gtk::Label::new(Some("Max size"));
        size_label.add_css_class("kalam-ed-option-title");
        size_row.append(&size_label);
        size_row.append(&size_entry);
        let kb = gtk::Label::new(Some("KB"));
        kb.add_css_class("kalam-ui-meta");
        size_row.append(&kb);
        let count = gtk::Label::new(None);
        count.add_css_class("kalam-ui-meta");
        count.set_halign(gtk::Align::Start);
        count.set_margin_top(8);

        let recount: Rc<dyn Fn()> = {
            let text = text.clone();
            let mode = mode.clone();
            let entry = size_entry.clone();
            let label = count.clone();
            Rc::new(move || {
                let kb: usize = entry.text().trim().parse().unwrap_or(250).max(1);
                let m = match mode.get() {
                    0 => crate::epub_structure::SplitMode::AtChar(cursor),
                    1 => crate::epub_structure::SplitMode::AtHeadings,
                    _ => crate::epub_structure::SplitMode::BySize(kb * 1024),
                };
                let n = crate::epub_structure::split_document(&text, m).len();
                label.set_label(&if n < 2 {
                    "This file stays whole — nothing to split here.".to_string()
                } else {
                    format!("This creates {n} files.")
                });
            })
        };
        let sync_ticks: Rc<dyn Fn()> = {
            let ticks: Vec<gtk::Label> = rows.iter().map(|(_, t)| t.clone()).collect();
            let mode = mode.clone();
            let recount = recount.clone();
            Rc::new(move || {
                for (i, tick) in ticks.iter().enumerate() {
                    set_tick(tick, mode.get() == i as u8);
                }
                recount();
            })
        };
        for (i, (row, _)) in rows.iter().enumerate() {
            let mode = mode.clone();
            let sync = sync_ticks.clone();
            row.connect_clicked(move |_| {
                mode.set(i as u8);
                sync();
            });
            shell.body().append(row);
        }
        shell.body().append(&size_row);
        {
            let recount = recount.clone();
            size_entry.connect_changed(move |_| recount());
        }
        shell.body().append(&count);
        sync_ticks();

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let slot_cancel = slot.clone();
        shell.footer().append(&boxed_ghost_button("Cancel", move || {
            close_slot(&slot_cancel);
        }));
        let slot_ok = slot.clone();
        let s = sender.input_sender().clone();
        let mode_ok = mode.clone();
        let entry_ok = size_entry.clone();
        shell.footer().append(&primary_button("Split", move || {
            close_slot(&slot_ok);
            let kb: usize = entry_ok.text().trim().parse().unwrap_or(250).max(1);
            let _ = s.send(EpubEditorMsg::RunSplit {
                mode: mode_ok.get(),
                max_kb: kb,
            });
        }));
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            &format!("Split {}", file_name(&path)),
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    fn run_split(&mut self, mode: u8, max_kb: usize, sender: &ComponentSender<Self>) {
        let Some(tab) = self.active_tab().filter(|t| t.kind == FileKind::Html) else {
            return;
        };
        let orig = tab.path.clone();
        let Some(text) = self.file_text(&orig) else {
            return;
        };
        let split_mode = match mode {
            0 => crate::epub_structure::SplitMode::AtChar(
                self.cursor_char_offset().unwrap_or(0),
            ),
            1 => crate::epub_structure::SplitMode::AtHeadings,
            _ => crate::epub_structure::SplitMode::BySize(max_kb * 1024),
        };
        let pieces = crate::epub_structure::split_document(&text, split_mode);
        if pieces.len() < 2 {
            self.show_pill("Nothing to split", false, sender);
            return;
        }
        let dir = crate::epub_structure::parent_dir(&orig);
        let base = file_name(&orig).to_string();
        let (stem, ext) = match base.rsplit_once('.') {
            Some((s, e)) => (s.to_string(), format!(".{e}")),
            None => (base.clone(), String::new()),
        };
        let taken: HashSet<String> = self
            .container
            .as_ref()
            .map(|c| c.files.iter().map(|f| f.path.clone()).collect())
            .unwrap_or_default();
        let mut names: Vec<String> = vec![orig.clone()];
        for i in 1..pieces.len() {
            let mut n = i + 1;
            let mut candidate = format!("{dir}{stem}_part{n}{ext}");
            while taken.contains(&candidate) || names.contains(&candidate) {
                n += 1;
                candidate = format!("{dir}{stem}_part{n}{ext}");
            }
            names.push(candidate);
        }
        // Which anchors moved to which new file.
        let mut id_map: HashMap<String, String> = HashMap::new();
        for (i, piece) in pieces.iter().enumerate().skip(1) {
            for id in crate::epub_structure::ids_in(piece) {
                id_map.insert(id, names[i].clone());
            }
        }
        let orig_for_map = orig.clone();
        let map = move |p: &str, frag: Option<&str>| -> Option<(String, Option<String>)> {
            if p == orig_for_map {
                if let Some(f) = frag {
                    if let Some(np) = id_map.get(f) {
                        return Some((np.clone(), Some(f.to_string())));
                    }
                }
            }
            None
        };

        let orig_spine = self
            .container
            .as_ref()
            .and_then(|c| c.file(&orig))
            .and_then(|f| f.spine_index);
        let mut plan = StructurePlan::new(
            format!(
                "Split {} into {} files",
                file_name(&orig),
                pieces.len()
            ),
            "Before split",
        );
        for (i, piece) in pieces.iter().enumerate() {
            let fixed =
                crate::epub_structure::rewrite_links(piece, &names[i], &orig, &map);
            if i == 0 {
                plan.text_changes.push((orig.clone(), fixed));
            } else {
                plan.new_files.push((
                    EpubFile {
                        path: names[i].clone(),
                        kind: FileKind::Html,
                        media_type: "application/xhtml+xml".to_string(),
                        spine_index: orig_spine.map(|s| s + i),
                        is_nav: false,
                        is_cover_image: false,
                        in_manifest: true,
                    },
                    fixed.into_bytes(),
                ));
            }
        }
        if let Some(container) = &self.container {
            for file in &container.files {
                if file.path == orig || !file.kind.is_text() {
                    continue;
                }
                if let Some(current) = self.file_text(&file.path) {
                    let fixed = crate::epub_structure::rewrite_links(
                        &current,
                        &file.path,
                        &file.path,
                        &map,
                    );
                    if fixed != current {
                        plan.text_changes.push((file.path.clone(), fixed));
                    }
                }
            }
        }
        if orig_spine.is_some() {
            let mut order = self.spine_order_paths();
            if let Some(at) = order.iter().position(|p| p == &orig) {
                for (j, name) in names.iter().skip(1).enumerate() {
                    order.insert(at + 1 + j, name.clone());
                }
            }
            plan.spine_order = Some(order);
        }
        self.finish_structure(plan, sender);
    }

    fn run_merge(&mut self, sender: &ComponentSender<Self>) {
        let Some(container) = &self.container else {
            return;
        };
        let order = self.spine_order_paths();
        let mut targets: Vec<String> = self
            .tree_selected
            .iter()
            .filter(|p| {
                container
                    .file(p)
                    .map(|f| f.kind == FileKind::Html && f.spine_index.is_some())
                    .unwrap_or(false)
            })
            .cloned()
            .collect();
        targets.sort_by_key(|p| order.iter().position(|o| o == p).unwrap_or(usize::MAX));
        targets.dedup();
        if targets.len() < 2 {
            // Fall back to the open chapter and the one after it.
            targets.clear();
            if let Some(active) = self.active_path.clone() {
                if let Some(at) = order.iter().position(|p| p == &active) {
                    if at + 1 < order.len() {
                        targets = vec![order[at].clone(), order[at + 1].clone()];
                    }
                }
            }
        }
        if targets.len() < 2 {
            self.show_pill(
                "Select two or more files in the tree (Ctrl+click) to merge",
                false,
                sender,
            );
            return;
        }
        let survivor = targets[0].clone();
        let texts: Vec<String> = targets
            .iter()
            .filter_map(|p| self.file_text(p))
            .collect();
        if texts.len() != targets.len() {
            return;
        }
        let Some(mut merged) = crate::epub_structure::merge_documents(&texts) else {
            self.show_pill("Those files could not be merged", false, sender);
            return;
        };
        let removed: HashSet<String> = targets[1..].iter().cloned().collect();
        let survivor_map = survivor.clone();
        let removed_for_map = removed.clone();
        let map = move |p: &str, frag: Option<&str>| -> Option<(String, Option<String>)> {
            if removed_for_map.contains(p) {
                Some((survivor_map.clone(), frag.map(str::to_string)))
            } else {
                None
            }
        };
        merged =
            crate::epub_structure::rewrite_links(&merged, &survivor, &survivor, &map);
        let mut plan = StructurePlan::new(
            format!(
                "Merged {} files into {}",
                targets.len(),
                file_name(&survivor)
            ),
            "Before merge",
        );
        plan.text_changes.push((survivor.clone(), merged));
        plan.removed_files = targets[1..].to_vec();
        if let Some(container) = &self.container {
            for file in &container.files {
                if targets.contains(&file.path) || !file.kind.is_text() {
                    continue;
                }
                if let Some(current) = self.file_text(&file.path) {
                    let fixed = crate::epub_structure::rewrite_links(
                        &current,
                        &file.path,
                        &file.path,
                        &map,
                    );
                    if fixed != current {
                        plan.text_changes.push((file.path.clone(), fixed));
                    }
                }
            }
        }
        plan.spine_order = Some(
            order
                .into_iter()
                .filter(|p| !removed.contains(p))
                .collect(),
        );
        self.finish_structure(plan, sender);
    }

    // ── E4: rename (decision 139) ──────────────────────────────

    fn present_rename_dialog(&self, path: &str, sender: &ComponentSender<Self>) {
        let dir = crate::epub_structure::parent_dir(path);
        let shell = dialog_shell(
            DialogSize::Form,
            Some("Links to the new name are fixed automatically"),
        );
        let entry = gtk::Entry::new();
        entry.add_css_class("kalam-ed-entry");
        entry.set_text(file_name(path));
        if let Some(dot) = entry.text().rfind('.') {
            entry.select_region(0, dot as i32);
        }
        shell.body().append(&entry);
        let note = gtk::Label::new(None);
        note.add_css_class("kalam-ui-meta");
        note.set_halign(gtk::Align::Start);
        note.set_margin_top(8);
        shell.body().append(&note);

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let slot_cancel = slot.clone();
        shell.footer().append(&boxed_ghost_button("Cancel", move || {
            close_slot(&slot_cancel);
        }));
        let slot_ok = slot.clone();
        let from = path.to_string();
        let entry_ok = entry.clone();
        let s = sender.input_sender().clone();
        shell.footer().append(&primary_button("Rename", move || {
            let name = entry_ok.text().trim().to_string();
            if name.is_empty() || name.contains('/') {
                return;
            }
            close_slot(&slot_ok);
            let _ = s.send(EpubEditorMsg::RunRename {
                from: from.clone(),
                to: format!("{dir}{name}"),
            });
        }));
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            &format!("Rename {}", file_name(path)),
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
            entry.grab_focus();
        }
    }

    fn run_rename(&mut self, from: String, to: String, sender: &ComponentSender<Self>) {
        let Some(container) = &self.container else {
            return;
        };
        if from == to || container.file(&from).is_none() {
            return;
        }
        if container.file(&to).is_some() {
            self.show_pill(
                &format!("A file named {} already exists", file_name(&to)),
                false,
                sender,
            );
            return;
        }
        if from == container.opf_path || from == "META-INF/container.xml" || from == "mimetype" {
            self.show_pill("That file keeps its name — the package points at it", false, sender);
            return;
        }
        let old_for_map = from.clone();
        let new_for_map = to.clone();
        let map = move |p: &str, frag: Option<&str>| -> Option<(String, Option<String>)> {
            if p == old_for_map {
                Some((new_for_map.clone(), frag.map(str::to_string)))
            } else {
                None
            }
        };
        let mut plan = StructurePlan::new(
            format!(
                "Renamed {} to {}",
                file_name(&from),
                file_name(&to)
            ),
            "Before rename",
        );
        plan.renames.push((from.clone(), to.clone()));
        if let Some(container) = &self.container {
            for file in &container.files {
                if file.path == from || !file.kind.is_text() {
                    continue;
                }
                if let Some(current) = self.file_text(&file.path) {
                    let fixed = crate::epub_structure::rewrite_links(
                        &current,
                        &file.path,
                        &file.path,
                        &map,
                    );
                    if fixed != current {
                        plan.text_changes.push((file.path.clone(), fixed));
                    }
                }
            }
        }
        // The renamed file's own links re-base under its new path.
        let kind = container.file(&from).map(|f| f.kind);
        if kind.map(|k| k.is_text()).unwrap_or(false) {
            if let Some(current) = self.file_text(&from) {
                let fixed = crate::epub_structure::rewrite_links(
                    &current, &to, &from, &map,
                );
                if fixed != current {
                    plan.text_changes.push((to.clone(), fixed));
                }
            }
        }
        self.finish_structure(plan, sender);
    }

    // ── E4: reorder, semantics, linked stylesheets ─────────────

    fn present_reorder_dialog(&self, sender: &ComponentSender<Self>) {
        let order = self.spine_order_paths();
        if order.len() < 2 {
            return;
        }
        let order = Rc::new(RefCell::new(order));
        let shell = dialog_shell(
            DialogSize::List,
            Some("Drag files into reading order"),
        );
        let list = gtk::Box::new(gtk::Orientation::Vertical, 4);
        build_reorder_rows(&list, &order);
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(360);
        scroll.set_child(Some(&list));
        shell.body().append(&scroll);

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let slot_cancel = slot.clone();
        shell.footer().append(&boxed_ghost_button("Cancel", move || {
            close_slot(&slot_cancel);
        }));
        let slot_ok = slot.clone();
        let s = sender.input_sender().clone();
        let order_ok = order.clone();
        shell.footer().append(&primary_button("Save order", move || {
            close_slot(&slot_ok);
            let _ = s.send(EpubEditorMsg::RunReorder(order_ok.borrow().clone()));
        }));
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Reorder files",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    fn run_reorder(&mut self, order: Vec<String>, sender: &ComponentSender<Self>) {
        if order == self.spine_order_paths() {
            self.show_pill("The reading order is unchanged", false, sender);
            return;
        }
        let mut plan = StructurePlan::new(
            "Reordered the reading order".to_string(),
            "Before reorder",
        );
        plan.spine_order = Some(order);
        self.finish_structure(plan, sender);
    }

    fn present_semantics_dialog(&self, path: &str, sender: &ComponentSender<Self>) {
        let Some(text) = self.file_text(path) else {
            return;
        };
        let current = crate::epub_structure::body_semantics(&text);
        // The dialog picks one standing type (decision 139); a
        // page wearing several shows none ticked and choosing
        // replaces the set.
        let current = if current.len() == 1 {
            Some(current[0].clone())
        } else {
            None
        };
        let choice: Rc<RefCell<Option<String>>> = Rc::new(RefCell::new(current));
        let shell = dialog_shell(DialogSize::Form, Some(file_name(path)));
        let list = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let mut ticks: Vec<(Option<String>, gtk::Label)> = Vec::new();
        let add = |label: &str, value: Option<&str>, ticks: &mut Vec<(Option<String>, gtk::Label)>| {
            let (row, tick) = option_row(label, "");
            row.set_size_request(-1, 40);
            ticks.push((value.map(str::to_string), tick.clone()));
            list.append(&row);
            (row, tick)
        };
        let mut rows: Vec<(Option<String>, gtk::Button)> = Vec::new();
        let (row, _) = add("No semantics", None, &mut ticks);
        rows.push((None, row));
        for (label, value) in SEMANTICS_CHOICES {
            let (row, _) = add(label, Some(value), &mut ticks);
            rows.push((Some(value.to_string()), row));
        }
        let sync: Rc<dyn Fn()> = {
            let choice = choice.clone();
            let ticks = ticks.clone();
            Rc::new(move || {
                let current = choice.borrow().clone();
                for (value, tick) in &ticks {
                    set_tick(tick, *value == current);
                }
            })
        };
        for (value, row) in rows {
            let choice = choice.clone();
            let sync = sync.clone();
            row.connect_clicked(move |_| {
                *choice.borrow_mut() = value.clone();
                sync();
            });
        }
        sync();
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(320);
        scroll.set_child(Some(&list));
        shell.body().append(&scroll);

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let slot_cancel = slot.clone();
        shell.footer().append(&boxed_ghost_button("Cancel", move || {
            close_slot(&slot_cancel);
        }));
        let slot_ok = slot.clone();
        let s = sender.input_sender().clone();
        let path_ok = path.to_string();
        shell.footer().append(&primary_button("Set", move || {
            close_slot(&slot_ok);
            let _ = s.send(EpubEditorMsg::RunSemantics {
                path: path_ok.clone(),
                value: choice.borrow().clone(),
            });
        }));
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Set semantics",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    fn run_semantics(
        &mut self,
        path: String,
        value: Option<String>,
        sender: &ComponentSender<Self>,
    ) {
        let Some(text) = self.file_text(&path) else {
            return;
        };
        let types: Vec<&str> = value.as_deref().into_iter().collect();
        let next = crate::epub_structure::set_semantics(&text, &types);
        if next == text {
            self.show_pill("The semantics are unchanged", false, sender);
            return;
        }
        let mut plan = StructurePlan::new(
            format!("Set semantics for {}", file_name(&path)),
            "Before set semantics",
        );
        plan.text_changes.push((path, next));
        self.finish_structure(plan, sender);
    }

    fn present_link_css_dialog(&mut self, path: &str, sender: &ComponentSender<Self>) {
        let Some(container) = &self.container else {
            return;
        };
        let Some(text) = self.file_text(path) else {
            return;
        };
        let linked: HashSet<String> =
            crate::epub_structure::linked_stylesheets(&text, path)
                .into_iter()
                .collect();
        let css_files: Vec<String> = container
            .files
            .iter()
            .filter(|f| f.kind == FileKind::Css)
            .map(|f| f.path.clone())
            .collect();
        if css_files.is_empty() {
            self.show_pill("This book has no stylesheets", false, sender);
            return;
        }
        let selected: Rc<RefCell<HashSet<String>>> = Rc::new(RefCell::new(linked));
        let shell = dialog_shell(DialogSize::Form, Some(file_name(path)));
        let list = gtk::Box::new(gtk::Orientation::Vertical, 4);
        for css in &css_files {
            let (row, tick) = option_row(file_name(css), css);
            row.set_size_request(-1, 48);
            set_tick(&tick, selected.borrow().contains(css));
            let selected = selected.clone();
            let css = css.clone();
            row.connect_clicked(move |_| {
                let on = {
                    let mut set = selected.borrow_mut();
                    if set.contains(&css) {
                        set.remove(&css);
                        false
                    } else {
                        set.insert(css.clone());
                        true
                    }
                };
                set_tick(&tick, on);
            });
            list.append(&row);
        }
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroll.set_min_content_height(240);
        scroll.set_child(Some(&list));
        shell.body().append(&scroll);

        let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
        let slot_cancel = slot.clone();
        shell.footer().append(&boxed_ghost_button("Cancel", move || {
            close_slot(&slot_cancel);
        }));
        let slot_ok = slot.clone();
        let s = sender.input_sender().clone();
        let path_ok = path.to_string();
        let order = css_files.clone();
        shell.footer().append(&primary_button("Link", move || {
            close_slot(&slot_ok);
            let chosen: Vec<String> = order
                .iter()
                .filter(|p| selected.borrow().contains(*p))
                .cloned()
                .collect();
            let _ = s.send(EpubEditorMsg::RunLinkCss {
                path: path_ok.clone(),
                css: chosen,
            });
        }));
        if let Some(dialog) = present_dialog(
            &self.root_overlay,
            "Link stylesheets",
            DialogExit::OwnButtons,
            &shell,
        ) {
            *slot.borrow_mut() = Some(dialog);
        }
    }

    fn run_link_css(
        &mut self,
        path: String,
        css: Vec<String>,
        sender: &ComponentSender<Self>,
    ) {
        let Some(text) = self.file_text(&path) else {
            return;
        };
        let next = crate::epub_structure::set_linked_stylesheets(&text, &path, &css);
        if next == text {
            self.show_pill("The linked stylesheets are unchanged", false, sender);
            return;
        }
        let mut plan = StructurePlan::new(
            format!("Updated stylesheets for {}", file_name(&path)),
            "Before link stylesheets",
        );
        plan.text_changes.push((path, next));
        self.finish_structure(plan, sender);
    }

    // ── E4: the file-tree menu (decision 139) ──────────────────

    fn present_tree_menu(&mut self, path: &str, sender: &ComponentSender<Self>) {
        if let Some(menu) = self.tree_menu.take() {
            menu.popover().popdown();
            menu.popover().unparent();
        }
        let Some(container) = &self.container else {
            return;
        };
        let Some(file) = container.file(path).cloned() else {
            return;
        };
        let protected = path == container.opf_path
            || path == "META-INF/container.xml"
            || path == "mimetype";
        let menu = crate::widgets::menu::menu_popover(crate::widgets::menu::MenuWidth::Wide);
        if !protected {
            let s = sender.input_sender().clone();
            let p = path.to_string();
            menu.add_item_with_hint("Rename…", "F2", move || {
                let _ = s.send(EpubEditorMsg::OpenRename(p.clone()));
            });
            let s = sender.input_sender().clone();
            let p = path.to_string();
            menu.add_destructive_item("Delete", move || {
                let _ = s.send(EpubEditorMsg::TreeDelete(p.clone()));
            });
        }
        if self.tree_selected.len() >= 2 {
            let s = sender.input_sender().clone();
            menu.add_item("Merge selected", move || {
                let _ = s.send(EpubEditorMsg::MergeSelected);
            });
        }
        menu.add_separator();
        let s = sender.input_sender().clone();
        menu.add_item("Add files…", move || {
            let _ = s.send(EpubEditorMsg::TreeAddFiles);
        });
        let s = sender.input_sender().clone();
        let p = path.to_string();
        menu.add_item("Export file…", move || {
            let _ = s.send(EpubEditorMsg::TreeExport(p.clone()));
        });
        if !protected {
            let s = sender.input_sender().clone();
            let p = path.to_string();
            menu.add_item("Replace from file…", move || {
                let _ = s.send(EpubEditorMsg::TreeReplace(p.clone()));
            });
            let s = sender.input_sender().clone();
            let p = path.to_string();
            menu.add_item("Export for editing…", move || {
                let _ = s.send(EpubEditorMsg::TreeExportForEditing(p.clone()));
            });
        }
        let s = sender.input_sender().clone();
        let p = path.to_string();
        menu.add_item("Open in external app", move || {
            let _ = s.send(EpubEditorMsg::TreeOpenExternal(p.clone()));
        });
        menu.add_separator();
        if file.kind == FileKind::Image {
            let s = sender.input_sender().clone();
            let p = path.to_string();
            menu.add_check_item("Mark as cover", file.is_cover_image, move |_| {
                let _ = s.send(EpubEditorMsg::MarkCover(p.clone()));
            });
        }
        if file.kind == FileKind::Html {
            let s = sender.input_sender().clone();
            let p = path.to_string();
            menu.add_item("Set semantics…", move || {
                let _ = s.send(EpubEditorMsg::OpenSemantics(p.clone()));
            });
            let s = sender.input_sender().clone();
            let p = path.to_string();
            menu.add_item("Link stylesheets…", move || {
                let _ = s.send(EpubEditorMsg::OpenLinkCss(p.clone()));
            });
        }
        let anchor = self
            .tree_rows
            .borrow()
            .iter()
            .find(|(p, _)| p == path)
            .map(|(_, row)| row.clone());
        if let Some(row) = anchor {
            menu.popover().set_parent(&row);
            menu.popover().popup();
            self.tree_menu = Some(menu);
        }
    }

    fn run_tree_delete(&mut self, path: String, sender: &ComponentSender<Self>) {
        let in_spine = self
            .container
            .as_ref()
            .and_then(|c| c.file(&path))
            .map(|f| f.spine_index.is_some())
            .unwrap_or(false);
        let mut plan = StructurePlan::new(
            format!("Deleted {}", file_name(&path)),
            "Before delete",
        );
        plan.removed_files.push(path.clone());
        if in_spine {
            plan.spine_order = Some(
                self.spine_order_paths()
                    .into_iter()
                    .filter(|p| p != &path)
                    .collect(),
            );
        }
        self.finish_structure(plan, sender);
    }

    fn run_mark_cover(&mut self, path: String, sender: &ComponentSender<Self>) {
        let already = self
            .container
            .as_ref()
            .and_then(|c| c.file(&path))
            .map(|f| f.is_cover_image)
            .unwrap_or(false);
        if already {
            self.show_pill("That image is already the cover", false, sender);
            return;
        }
        let mut plan = StructurePlan::new(
            format!("{} is now the cover", file_name(&path)),
            "Before mark as cover",
        );
        plan.set_cover = Some(path);
        self.finish_structure(plan, sender);
    }

    /// Add files… imports (disk name, bytes) into the book:
    /// same-named existing files are replaced in place, new
    /// ones land beside their kind and join the manifest;
    /// chapters also join the reading order at the end.
    fn run_files_loaded(
        &mut self,
        imported: Vec<(String, Vec<u8>)>,
        sender: &ComponentSender<Self>,
    ) {
        let Some(container) = &self.container else {
            return;
        };
        if imported.is_empty() {
            return;
        }
        let mut plan = StructurePlan::new(
            if imported.len() == 1 {
                format!("Added {}", imported[0].0)
            } else {
                format!("Added {} files", imported.len())
            },
            "Before add files",
        );
        // Donor directories by kind, from files already present.
        let donor_dir = |kind: FileKind| -> String {
            container
                .files
                .iter()
                .find(|f| f.kind == kind)
                .map(|f| crate::epub_structure::parent_dir(&f.path))
                .unwrap_or_else(|| crate::epub_structure::parent_dir(&container.opf_path))
        };
        let mut order = self.spine_order_paths();
        let mut order_changed = false;
        for (name, bytes) in imported {
            let bare = file_name(&name).to_string();
            let media = crate::epub_container::guess_media_type(&bare);
            let kind = crate::epub_container::classify(&bare, media);
            if let Some(existing) = container
                .files
                .iter()
                .find(|f| file_name(&f.path) == bare)
                .map(|f| f.path.clone())
            {
                match String::from_utf8(bytes.clone()) {
                    Ok(text) if kind.is_text() => {
                        plan.text_changes.push((existing, text));
                    }
                    _ => plan.byte_changes.push((existing, bytes)),
                }
                continue;
            }
            let path = format!("{}{}", donor_dir(kind), bare);
            let spined = kind == FileKind::Html;
            if spined {
                order.push(path.clone());
                order_changed = true;
            }
            plan.new_files.push((
                EpubFile {
                    path,
                    kind,
                    media_type: media.to_string(),
                    spine_index: spined.then_some(order.len() - 1),
                    is_nav: false,
                    is_cover_image: false,
                    in_manifest: true,
                },
                bytes,
            ));
        }
        if order_changed {
            plan.spine_order = Some(order);
        }
        self.finish_structure(plan, sender);
    }

    fn run_replace_loaded(
        &mut self,
        path: String,
        bytes: Vec<u8>,
        snapshot_label: &str,
        sender: &ComponentSender<Self>,
    ) {
        let kind = self
            .container
            .as_ref()
            .and_then(|c| c.file(&path))
            .map(|f| f.kind);
        let mut plan = StructurePlan::new(
            format!("Replaced {}", file_name(&path)),
            snapshot_label,
        );
        match String::from_utf8(bytes.clone()) {
            Ok(text) if kind.map(|k| k.is_text()).unwrap_or(false) => {
                plan.text_changes.push((path, text));
            }
            _ => plan.byte_changes.push((path, bytes)),
        }
        self.finish_structure(plan, sender);
    }

    // ── E4: file dialogs for the tree menu ─────────────────────

    fn host_window(&self) -> Option<gtk::Window> {
        self.root_overlay
            .root()
            .and_then(|r| r.downcast::<gtk::Window>().ok())
            .or_else(|| {
                relm4::main_application()
                    .active_window()
                    .and_then(|w| w.downcast::<gtk::Window>().ok())
            })
    }

    fn pick_add_files(&self, sender: &ComponentSender<Self>) {
        let dialog = gtk::FileDialog::builder()
            .title("Add files to the book")
            .modal(true)
            .build();
        let filter = gtk::FileFilter::new();
        filter.set_name(Some("Book files"));
        for suffix in [
            "xhtml", "html", "htm", "css", "xml", "ncx", "opf", "txt", "svg", "png",
            "jpg", "jpeg", "gif", "webp", "ttf", "otf", "woff", "woff2",
        ] {
            filter.add_suffix(suffix);
        }
        let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
        filters.append(&filter);
        dialog.set_filters(Some(&filters));
        let window = self.host_window();
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        dialog.open_multiple(
            window.as_ref(),
            gtk::gio::Cancellable::NONE,
            move |result| {
                let Ok(files) = result else {
                    return;
                };
                let mut paths = Vec::new();
                for i in 0..files.n_items() {
                    if let Some(file) = files
                        .item(i)
                        .and_then(|obj| obj.downcast::<gtk::gio::File>().ok())
                    {
                        if let Some(p) = file.path() {
                            paths.push(p);
                        }
                    }
                }
                if paths.is_empty() {
                    return;
                }
                let s2 = s.clone();
                crate::tasks::spawn_internal(
                    "Reading files",
                    move |_reporter| {
                        let mut out = Vec::new();
                        for path in &paths {
                            let name = path
                                .file_name()
                                .map(|n| n.to_string_lossy().to_string())
                                .unwrap_or_else(|| "file".to_string());
                            let bytes = service.read_editor_import(path)?;
                            out.push((name, bytes));
                        }
                        Ok::<_, String>(out)
                    },
                    |_| {},
                    move |result| match result {
                        Ok(imported) => {
                            let _ = s2.send(EpubEditorMsg::FilesLoaded(imported));
                        }
                        Err(message) => {
                            let _ = s2.send(EpubEditorMsg::ActionFailed(message));
                        }
                    },
                );
            },
        );
    }

    fn pick_replace_file(&self, target: &str, sender: &ComponentSender<Self>) {
        let dialog = gtk::FileDialog::builder()
            .title(format!("Replace {}", file_name(target)))
            .modal(true)
            .build();
        let window = self.host_window();
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        let target = target.to_string();
        dialog.open(window.as_ref(), gtk::gio::Cancellable::NONE, move |result| {
            let Ok(file) = result else {
                return;
            };
            let Some(path) = file.path() else {
                return;
            };
            let s2 = s.clone();
            let target = target.clone();
            crate::tasks::spawn_internal(
                "Reading file",
                move |_reporter| service.read_editor_import(&path),
                |_| {},
                move |result| match result {
                    Ok(bytes) => {
                        let _ = s2.send(EpubEditorMsg::ReplaceLoaded(
                            target.clone(),
                            bytes,
                            "Before replace file",
                        ));
                    }
                    Err(message) => {
                        let _ = s2.send(EpubEditorMsg::ActionFailed(message));
                    }
                },
            );
        });
    }

    fn pick_export_file(&self, path: &str, sender: &ComponentSender<Self>) {
        let Some(bytes) = self.file_bytes(path) else {
            return;
        };
        let dialog = gtk::FileDialog::builder()
            .title(format!("Export {}", file_name(path)))
            .modal(true)
            .initial_name(file_name(path))
            .build();
        let window = self.host_window();
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        dialog.save(window.as_ref(), gtk::gio::Cancellable::NONE, move |result| {
            let Ok(file) = result else {
                return;
            };
            let Some(dest) = file.path() else {
                return;
            };
            let s2 = s.clone();
            crate::tasks::spawn_internal(
                "Exporting file",
                move |_reporter| service.write_editor_export(&dest, &bytes),
                |_| {},
                move |result| {
                    if let Err(message) = result {
                        let _ = s2.send(EpubEditorMsg::ActionFailed(message));
                    }
                },
            );
        });
    }

    fn open_external(&self, path: &str, sender: &ComponentSender<Self>) {
        let (Some(book), Some(bytes)) = (self.book.clone(), self.file_bytes(path))
        else {
            return;
        };
        let inner = path.to_string();
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Preparing file",
            move |_reporter| service.materialize_editor_file(&book, &inner, &bytes),
            |_| {},
            move |result| match result {
                Ok(path) => {
                    let _ = s.send(EpubEditorMsg::ExternalReady(path));
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
        self.structure_undo = None;
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
            item("Edit contents", "", None, PaletteAction::EditContents),
            item("Spell check", "", Some("F7"), PaletteAction::SpellCheck),
            item("Personal dictionary…", "", None, PaletteAction::PersonalDict),
            item("History…", "", None, PaletteAction::History),
            item("Replace cover…", "", None, PaletteAction::ReplaceCover),
            item("Reports…", "", None, PaletteAction::Reports),
            item("Split at cursor", "Structure", None, PaletteAction::SplitCursor),
            item("Split at every heading…", "Structure", None, PaletteAction::SplitDialog(1)),
            item("Split chapter by size…", "Structure", None, PaletteAction::SplitDialog(2)),
            item("Merge files", "Structure", None, PaletteAction::MergeSelected),
            item("Rename file…", "Structure", Some("F2"), PaletteAction::RenameActive),
            item("Reorder files…", "Structure", None, PaletteAction::Reorder),
            item("Set semantics…", "Structure", None, PaletteAction::Semantics),
            item("Link stylesheets…", "Structure", None, PaletteAction::LinkCss),
            item("Add files…", "Structure", None, PaletteAction::AddFiles),
            item("Insert character…", "Insert", None, PaletteAction::InsertChar),
            item("Insert link…", "Insert", None, PaletteAction::InsertLink),
            item("Insert image…", "Insert", None, PaletteAction::InsertImage),
            item("Insert contents page…", "Insert", None, PaletteAction::InsertInlineContents),
            item("Filter style information…", "Clean up", None, PaletteAction::FilterStyle),
            item("Tag mapper…", "Clean up", None, PaletteAction::TagMapper),
            item("Upgrade book internals…", "Clean up", None, PaletteAction::UpgradeInternals),
            item("Subset fonts…", "Fonts and images", None, PaletteAction::SubsetFonts),
            item("Embed fonts…", "Fonts and images", None, PaletteAction::EmbedFonts),
            item("Compress images…", "Fonts and images", None, PaletteAction::CompressImages),
            item("Check external links…", "Fonts and images", None, PaletteAction::CheckLinks),
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
            PaletteAction::EditContents => self.open_contents(sender),
            PaletteAction::SpellCheck => self.start_spell_check(sender),
            PaletteAction::PersonalDict => self.present_personal_dict_dialog(sender),
            PaletteAction::History => self.open_history(sender),
            PaletteAction::ReplaceCover => self.present_replace_cover_dialog(sender),
            PaletteAction::Reports => self.present_reports_dialog(sender),
            PaletteAction::TagMapper => self.present_tag_mapper_dialog(sender),
            PaletteAction::FilterStyle => self.present_filter_style_dialog(sender),
            PaletteAction::UpgradeInternals => self.present_upgrade_dialog(sender),
            PaletteAction::SubsetFonts => {
                self.start_subset_fonts(None, sender)
            }
            PaletteAction::EmbedFonts => self.start_embed_fonts(sender),
            PaletteAction::CompressImages => self.start_compress_images(sender),
            PaletteAction::CheckLinks => self.present_check_links_dialog(sender),
            PaletteAction::SplitCursor => self.run_split(0, 250, sender),
            PaletteAction::SplitDialog(preset) => {
                self.present_split_dialog(preset, sender)
            }
            PaletteAction::MergeSelected => self.run_merge(sender),
            PaletteAction::RenameActive => sender.input(EpubEditorMsg::RenameActive),
            PaletteAction::Reorder => self.present_reorder_dialog(sender),
            PaletteAction::Semantics => {
                sender.input(EpubEditorMsg::PalettePickSemantics)
            }
            PaletteAction::LinkCss => sender.input(EpubEditorMsg::PalettePickLinkCss),
            PaletteAction::AddFiles => sender.input(EpubEditorMsg::TreeAddFiles),
            PaletteAction::InsertChar => sender.input(EpubEditorMsg::OpenInsertChar),
            PaletteAction::InsertLink => sender.input(EpubEditorMsg::OpenInsertLink),
            PaletteAction::InsertImage => sender.input(EpubEditorMsg::OpenInsertImage),
            PaletteAction::InsertInlineContents => {
                sender.input(EpubEditorMsg::OpenInsertInlineContents)
            }
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
        self.save_btn
            .set_sensitive(self.has_unsaved() && !self.saving);
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

/// Light or darken a §38 option row's tick circle.
fn set_tick(tick: &gtk::Label, on: bool) {
    if on {
        tick.add_css_class("on");
    } else {
        tick.remove_css_class("on");
    }
}

/// One section-38 option row: tick circle, title, optional meta.
/// Returns the row button and its tick for state flips.
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
    if !meta.is_empty() {
        let m = gtk::Label::new(Some(meta));
        m.add_css_class("kalam-ui-meta");
        m.set_halign(gtk::Align::Start);
        texts.append(&m);
    }
    h.append(&texts);
    row.set_child(Some(&h));
    (row, tick)
}

/// The semantics a page can wear (decision 139's Set
/// semantics): display name and `epub:type` value.
const SEMANTICS_CHOICES: &[(&str, &str)] = &[
    ("Cover page", "cover"),
    ("Title page", "titlepage"),
    ("Contents", "toc"),
    ("Chapter", "chapter"),
    ("Part", "part"),
    ("Preface", "preface"),
    ("Foreword", "foreword"),
    ("Introduction", "introduction"),
    ("Prologue", "prologue"),
    ("Epilogue", "epilogue"),
    ("Appendix", "appendix"),
    ("Afterword", "afterword"),
    ("Acknowledgements", "acknowledgments"),
    ("Copyright page", "copyright-page"),
    ("Dedication", "dedication"),
    ("Glossary", "glossary"),
    ("Bibliography", "bibliography"),
    ("Index", "index"),
    ("Footnotes", "footnotes"),
    ("Endnotes", "endnotes"),
];

/// (Re)build the Reorder dialog's draggable rows: grip dots,
/// sequence number, file name; a drop moves the path in
/// `order` and rebuilds so indices stay honest.
fn build_reorder_rows(list: &gtk::Box, order: &Rc<RefCell<Vec<String>>>) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let paths = order.borrow().clone();
    for (index, path) in paths.iter().enumerate() {
        let row = gtk::Button::new();
        row.add_css_class("kalam-ed-order-row");
        row.set_size_request(-1, 44);
        let h = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let grip = gtk::Label::new(Some("⠿"));
        grip.add_css_class("kalam-ed-grip");
        grip.set_valign(gtk::Align::Center);
        h.append(&grip);
        let num = gtk::Label::new(Some(&format!("{}", index + 1)));
        num.add_css_class("kalam-ui-meta");
        num.set_width_chars(3);
        num.set_valign(gtk::Align::Center);
        h.append(&num);
        let name = gtk::Label::new(Some(file_name(path)));
        name.add_css_class("kalam-ed-option-title");
        name.set_halign(gtk::Align::Start);
        name.set_valign(gtk::Align::Center);
        h.append(&name);
        row.set_child(Some(&h));

        let provider = gtk::gdk::ContentProvider::for_value(&(index as i64).to_value());
        let source = gtk::DragSource::new();
        source.set_actions(gtk::gdk::DragAction::MOVE);
        source.set_content(Some(&provider));
        let row_begin = row.clone();
        source.connect_drag_begin(move |source, _| {
            row_begin.set_opacity(0.4);
            let paintable = gtk::WidgetPaintable::new(Some(&row_begin));
            source.set_icon(Some(&paintable), 14, 18);
        });
        let row_end = row.clone();
        source.connect_drag_end(move |_, _, _| row_end.set_opacity(1.0));
        let row_cancel = row.clone();
        source.connect_drag_cancel(move |_, _, _| {
            row_cancel.set_opacity(1.0);
            false
        });
        row.add_controller(source);

        let target = gtk::DropTarget::new(i64::static_type(), gtk::gdk::DragAction::MOVE);
        let row_motion = row.clone();
        target.connect_enter(move |_, _, y| {
            set_toc_drop(&row_motion, y);
            gtk::gdk::DragAction::MOVE
        });
        let row_motion2 = row.clone();
        target.connect_motion(move |_, _, y| {
            set_toc_drop(&row_motion2, y);
            gtk::gdk::DragAction::MOVE
        });
        let row_leave = row.clone();
        target.connect_leave(move |_| {
            row_leave.remove_css_class("drop-before");
            row_leave.remove_css_class("drop-after");
        });
        let row_drop = row.clone();
        let list_drop = list.clone();
        let order_drop = order.clone();
        target.connect_drop(move |_, value, _, y| {
            let after = y >= row_drop.height() as f64 / 2.0;
            row_drop.remove_css_class("drop-before");
            row_drop.remove_css_class("drop-after");
            let Ok(from) = value.get::<i64>() else {
                return false;
            };
            let from = from as usize;
            {
                let mut paths = order_drop.borrow_mut();
                if from >= paths.len() || from == index {
                    return false;
                }
                let item = paths.remove(from);
                let mut to = if after { index + 1 } else { index };
                if from < to {
                    to -= 1;
                }
                paths.insert(to, item);
            }
            build_reorder_rows(&list_drop, &order_drop);
            true
        });
        row.add_controller(target);
        list.append(&row);
    }
}

/// The boards' tick circle (Tag mapper, Subset fonts, Embed
/// fonts): an accent disc with a white check when on, a hollow
/// ring when off. Returns the disc for `set_tick` to flip.
fn tick_circle(on: bool) -> gtk::Box {
    let circle = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    circle.add_css_class("kalam-ed-tick");
    circle.set_size_request(18, 18);
    circle.set_valign(gtk::Align::Center);
    let mark = gtk::Label::new(Some("✓"));
    mark.add_css_class("kalam-ed-tick-mark");
    mark.set_hexpand(true);
    mark.set_valign(gtk::Align::Center);
    circle.append(&mark);
    set_tick_circle(&circle, on);
    circle
}

/// A Tag mapper rule is well-formed when both sides parse as
/// the mapper reads them: `tag` or `tag.class` into a bare tag.
fn valid_tag_rule(from: &str, to: &str) -> bool {
    let name_ok = |s: &str, class: bool| {
        !s.is_empty()
            && s.chars().next().map(|c| c.is_ascii_alphabetic()).unwrap_or(false)
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || (class && c == '_'))
    };
    let from_ok = match from.split_once('.') {
        Some((tag, class)) => name_ok(tag, false) && name_ok(class, true),
        None => name_ok(from, false),
    };
    from_ok && name_ok(to, false)
}

fn set_tick_circle(circle: &gtk::Box, on: bool) {
    if on {
        circle.add_css_class("on");
    } else {
        circle.remove_css_class("on");
    }
    if let Some(mark) = circle.first_child() {
        mark.set_visible(on);
    }
}

/// The Compare page (decision 134): a header over two
/// side-by-side source columns with number gutters, all four
/// scrollers sharing one vertical adjustment so the columns
/// cannot drift apart. Buffers are filled per comparison by
/// `render_compare`.
fn build_compare_page(
    sender: &ComponentSender<EpubEditorModel>,
) -> (gtk::Box, CompareWidgets) {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    header.add_css_class("kalam-ed-compare-header");
    header.set_size_request(-1, 56);
    header.set_margin_start(20);
    header.set_margin_end(20);
    let title = gtk::Label::new(None);
    title.add_css_class("kalam-ed-compare-title");
    title.set_valign(gtk::Align::Center);
    header.append(&title);
    let meta = gtk::Label::new(None);
    meta.add_css_class("kalam-ui-meta");
    meta.set_valign(gtk::Align::Center);
    header.append(&meta);
    let removed_probe = crate::widgets::paint::color_probe("kalam-ed-diff-removed");
    header.append(&removed_probe);
    let added_probe = crate::widgets::paint::color_probe("kalam-ed-diff-added");
    header.append(&added_probe);
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    header.append(&spacer);
    let prev_btn = gtk::Button::with_label("‹");
    prev_btn.add_css_class("kalam-ed-ghost");
    prev_btn.set_valign(gtk::Align::Center);
    prev_btn.set_tooltip_text(Some("Previous changed file"));
    {
        let s = sender.input_sender().clone();
        prev_btn.connect_clicked(move |_| {
            let _ = s.send(EpubEditorMsg::CompareFile(-1));
        });
    }
    header.append(&prev_btn);
    let next_btn = gtk::Button::with_label("›");
    next_btn.add_css_class("kalam-ed-ghost");
    next_btn.set_valign(gtk::Align::Center);
    next_btn.set_tooltip_text(Some("Next changed file"));
    {
        let s = sender.input_sender().clone();
        next_btn.connect_clicked(move |_| {
            let _ = s.send(EpubEditorMsg::CompareFile(1));
        });
    }
    header.append(&next_btn);
    let s = sender.input_sender().clone();
    let back = boxed_ghost_button("Back to editor", move || {
        let _ = s.send(EpubEditorMsg::CompareBack);
    });
    back.set_valign(gtk::Align::Center);
    header.append(&back);
    let s = sender.input_sender().clone();
    let restore = primary_button("Restore this version", move || {
        let _ = s.send(EpubEditorMsg::CompareRestore);
    });
    restore.set_valign(gtk::Align::Center);
    header.append(&restore);
    page.append(&header);

    let cols = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    cols.set_homogeneous(true);
    cols.set_vexpand(true);
    let vadjustment = gtk::Adjustment::new(0.0, 0.0, 0.0, 1.0, 10.0, 0.0);

    let make_side = |vadjustment: &gtk::Adjustment| {
        let side = gtk::Box::new(gtk::Orientation::Vertical, 0);
        side.set_hexpand(true);
        let head = gtk::Label::new(None);
        head.add_css_class("kalam-ed-compare-colhead");
        head.set_halign(gtk::Align::Start);
        head.set_margin_start(20);
        head.set_margin_top(10);
        head.set_margin_bottom(6);
        side.append(&head);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        row.set_vexpand(true);

        let numbers_buffer = sourceview5::Buffer::new(None::<&gtk::TextTagTable>);
        let numbers_view = sourceview5::View::with_buffer(&numbers_buffer);
        numbers_view.add_css_class("kalam-ed-source");
        numbers_view.add_css_class("kalam-ed-linenumbers");
        numbers_view.set_editable(false);
        numbers_view.set_cursor_visible(false);
        numbers_view.set_show_line_numbers(false);
        numbers_view.set_highlight_current_line(false);
        numbers_view.set_wrap_mode(gtk::WrapMode::None);
        numbers_view.set_justification(gtk::Justification::Right);
        numbers_view.set_top_margin(4);
        let numbers_scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::External)
            .width_request(52)
            .vexpand(true)
            .child(&numbers_view)
            .build();
        numbers_scroller.set_vadjustment(Some(vadjustment));
        row.append(&numbers_scroller);

        let buffer = sourceview5::Buffer::new(None::<&gtk::TextTagTable>);
        buffer.set_highlight_syntax(true);
        let view = sourceview5::View::with_buffer(&buffer);
        view.add_css_class("kalam-ed-source");
        view.set_editable(false);
        view.set_show_line_numbers(false);
        view.set_highlight_current_line(false);
        view.set_wrap_mode(gtk::WrapMode::None);
        view.set_top_margin(4);
        view.set_hexpand(true);
        view.set_vexpand(true);
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Automatic)
            .vscrollbar_policy(gtk::PolicyType::External)
            .vexpand(true)
            .hexpand(true)
            .child(&view)
            .build();
        scroller.set_vadjustment(Some(vadjustment));
        row.append(&scroller);

        side.append(&row);
        (side, head, buffer, numbers_buffer)
    };
    let (left, left_head, old_buffer, old_numbers) = make_side(&vadjustment);
    let (right, _right_head, new_buffer, new_numbers) = make_side(&vadjustment);
    _right_head.set_label("CURRENT");
    // The divider is a border on the left column, not a third
    // child: in a homogeneous box a separator child would be
    // granted a full third of the width.
    left.add_css_class("kalam-ed-compare-left");
    cols.append(&left);
    cols.append(&right);
    page.append(&cols);

    (
        page,
        CompareWidgets {
            title,
            meta,
            prev_btn,
            next_btn,
            left_head,
            old_buffer,
            new_buffer,
            old_numbers,
            new_numbers,
            removed_probe,
            added_probe,
            vadjustment,
        },
    )
}

/// The 3px accent drop bar on a contents row, before or after
/// by pointer half (decision 136's "drag to reorder").
fn set_toc_drop(row: &gtk::Button, y: f64) {
    if y >= row.height() as f64 / 2.0 {
        row.remove_css_class("drop-before");
        row.add_css_class("drop-after");
    } else {
        row.remove_css_class("drop-after");
        row.add_css_class("drop-before");
    }
}

/// A fresh manifest id for a file the OPF never had: the file
/// stem, sanitised to an XML name, numbered until unique.
fn mint_opf_id(path: &str, used: &mut HashSet<String>) -> String {
    let stem = file_name(path).rsplit_once('.').map(|(s, _)| s).unwrap_or(path);
    let mut base: String = stem
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '-'
            }
        })
        .collect();
    if base.is_empty()
        || !base
            .chars()
            .next()
            .map(|c| c.is_ascii_alphabetic() || c == '_')
            .unwrap_or(false)
    {
        base = format!("id-{base}");
    }
    let mut candidate = base.clone();
    let mut n = 1;
    while used.contains(&candidate) {
        n += 1;
        candidate = format!("{base}-{n}");
    }
    used.insert(candidate.clone());
    candidate
}

/// Keep one flag in a manifest `properties` list in step with
/// the container's view (OPF rebuilds preserve the rest).
fn sync_opf_prop(properties: &mut Vec<String>, name: &str, on: bool) {
    if on {
        if !properties.iter().any(|p| p == name) {
            properties.push(name.to_string());
        }
    } else {
        properties.retain(|p| p != name);
    }
}

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

/// The folded form dictionaries are keyed by: curly apostrophe
/// straightened, lowercased (mirrors `word_known`).
fn fold_word(word: &str) -> String {
    word.chars()
        .map(|c| if c == '\u{2019}' { '\'' } else { c })
        .collect::<String>()
        .to_lowercase()
}

/// Escape a value for a double-quoted XML attribute.
fn esc_attr(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn file_name(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

fn file_summary(file: &EpubFile, bytes: Option<&[u8]>) -> String {
    format!(
        "{} · {} · {}",
        file.path,
        human_size(bytes.map(|b| b.len()).unwrap_or(0)),
        file.media_type
    )
}

/// Thousands-grouped counts for Reports: `12,340`.
fn grouped(n: usize) -> String {
    let digits = n.to_string();
    let mut out = String::new();
    for (index, ch) in digits.chars().enumerate() {
        if index > 0 && (digits.len() - index).is_multiple_of(3) {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

/// Board-style file sizes: `214 KB`, `8.2 MB`.
fn human_size(size: usize) -> String {
    if size >= 1024 * 1024 {
        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
    } else if size >= 1024 {
        format!("{:.0} KB", size as f64 / 1024.0)
    } else {
        format!("{size} B")
    }
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
