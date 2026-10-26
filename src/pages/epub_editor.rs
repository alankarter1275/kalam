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

use crate::epub_container::{EpubContainer, EpubFile, FileKind, TreeSection};
use crate::models::Book;
use crate::service::LibraryService;
use crate::widgets::dialog::{dialog_shell, present_dialog, DialogSize};
use crate::widgets::in_app_dialog::{DialogExit, InAppDialog};
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
}

/// One problem in the Problems tab. E1's are XML well-formedness
/// errors from validation; Check book (E2) adds the rest.
#[derive(Debug, Clone)]
struct EditorProblem {
    path: String,
    line: usize,
    message: String,
}

struct OpenTab {
    path: String,
    kind: FileKind,
    pinned: bool,
    buffer: Option<sourceview5::Buffer>,
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
    preview_label: gtk::Label,
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
    problems: Vec<EditorProblem>,
    /// The modified-path set as last rendered, so tree/tab dots only
    /// rebuild when the *set* changes, not on every keystroke.
    shown_dirty: HashSet<String>,
    after_save: AfterSave,
    saving: bool,
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
        let preview_scroller = scroll_host(&preview_label);

        let outline_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let outline_scroller = scroll_host(&outline_host);

        let problems_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let problems_scroller = scroll_host(&problems_host);

        let livecss_label = gtk::Label::new(Some(
            "Place the cursor inside an element in an HTML file to see the CSS that applies to it.",
        ));
        livecss_label.add_css_class("kalam-ui-meta");
        livecss_label.set_wrap(true);
        livecss_label.set_xalign(0.0);
        livecss_label.set_yalign(0.0);
        livecss_label.set_margin_all(20);
        let livecss_scroller = scroll_host(&livecss_label);

        let right_stack = gtk::Stack::new();
        right_stack.set_vexpand(true);
        right_stack.add_named(&preview_scroller, Some("preview"));
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

        // Ctrl+S saves, from anywhere in the workspace.
        let keys = gtk::EventControllerKey::new();
        {
            let s = sender.clone();
            keys.connect_key_pressed(move |_, keyval, _, state| {
                if state.contains(gtk::gdk::ModifierType::CONTROL_MASK)
                    && keyval == gtk::gdk::Key::s
                {
                    s.input(EpubEditorMsg::SavePressed);
                    return gtk::glib::Propagation::Stop;
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
            preview_label,
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
                self.refresh_problems();
                self.sync_chrome(&sender);
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
                self.refresh_problems();
                self.sync_chrome(&sender);
                self.update_preview();
                self.rebuild_outline(&sender);
            }
            EpubEditorMsg::CursorMoved { line, col } => {
                self.status_pos.set_label(&format!("Ln {line}, Col {col}"));
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
                if let Some(container) = &mut self.container {
                    for (path, bytes) in saved {
                        container.set_bytes(&path, bytes);
                    }
                }
                match std::mem::replace(&mut self.after_save, AfterSave::Nothing) {
                    AfterSave::Leave => {
                        let _ = sender.output(EpubEditorOut::Leave);
                    }
                    AfterSave::CloseTab(path) => self.close_tab(&path, &sender),
                    AfterSave::Nothing => {}
                }
                self.refresh_problems();
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
                    self.refresh_problems();
                    self.rebuild_problems(&sender);
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
        }
    }
}

/// Which pending action an unsaved-changes dialog is deciding.
#[derive(Clone)]
enum UnsavedAction {
    Leave,
    CloseTab(String),
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
                let text = container.text(&file.path).unwrap_or_default();
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
                buffer.set_modified(false);

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
        self.update_preview();
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
        self.update_preview();
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
        self.tabs
            .iter()
            .find(|t| t.path == path)
            .and_then(|t| t.buffer.as_ref())
            .map(|b| b.is_modified())
            .unwrap_or(false)
    }

    fn dirty_paths(&self) -> HashSet<String> {
        self.tabs
            .iter()
            .filter(|t| t.buffer.as_ref().map(|b| b.is_modified()).unwrap_or(false))
            .map(|t| t.path.clone())
            .collect()
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
            // Validate every changed markup file before saving;
            // errors ask first, warnings never block (decision 143).
            self.refresh_problems();
            if !self.problems.is_empty() {
                self.rebuild_problems(sender);
                self.present_save_with_errors_dialog(sender);
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

    /// Recompute validation problems across every open, changed
    /// markup buffer (E1's Problems; Check book widens this in E2).
    fn refresh_problems(&mut self) {
        let mut problems = Vec::new();
        for tab in &self.tabs {
            if !matches!(tab.kind, FileKind::Html | FileKind::Xml) {
                continue;
            }
            let Some(buffer) = &tab.buffer else { continue };
            if !buffer.is_modified() {
                continue;
            }
            if let Some((line, message)) =
                crate::epub_container::xml_problem(&buffer_text(buffer))
            {
                problems.push(EditorProblem {
                    path: tab.path.clone(),
                    line,
                    message,
                });
            }
        }
        self.problems = problems;
        let n = self.problems.len();
        self.status_problems.set_visible(n > 0);
        if n > 0 {
            self.status_problems.set_label(&format!(
                "{n} problem{}",
                if n == 1 { "" } else { "s" }
            ));
        }
        if self.right_tab == RightTab::Problems {
            // Rebuilt by caller paths that own a sender.
        }
    }

    fn present_save_with_errors_dialog(&self, sender: &ComponentSender<Self>) {
        let n = self.problems.len();
        let first = self
            .problems
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
        let heading = gtk::Label::new(Some("Errors"));
        heading.add_css_class("kalam-editorpicker-group");
        heading.set_halign(gtk::Align::Start);
        heading.set_margin_start(20);
        heading.set_margin_top(16);
        heading.set_margin_bottom(4);
        self.problems_host.append(&heading);

        for problem in &self.problems {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            row.add_css_class("kalam-ed-problem-row");
            row.set_size_request(-1, 56);
            row.set_margin_start(20);
            row.set_margin_end(20);

            let marker = gtk::Label::new(Some("●"));
            marker.add_css_class("kalam-ed-problem-error");
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
            let whereto = gtk::Label::new(Some(&format!(
                "{} · line {}",
                file_name(&problem.path),
                problem.line
            )));
            whereto.add_css_class("kalam-ui-meta");
            whereto.set_halign(gtk::Align::Start);
            texts.append(&whereto);
            row.append(&texts);

            let path = problem.path.clone();
            let line = problem.line;
            let s = sender.input_sender().clone();
            let go = boxed_ghost_button("Go to", move || {
                let _ = s.send(EpubEditorMsg::GoTo {
                    path: path.clone(),
                    line: Some(line),
                    offset: None,
                });
            });
            go.add_css_class("kalam-ed-problem-btn");
            go.set_valign(gtk::Align::Center);
            row.append(&go);

            self.problems_host.append(&row);
        }
    }

    fn rebuild_outline(&self, sender: &ComponentSender<Self>) {
        clear_box(&self.outline_host);
        let Some(tab) = self.active_tab() else {
            return;
        };
        if tab.kind != FileKind::Html {
            self.outline_host.append(&panel_note("No headings"));
            return;
        }
        let Some(buffer) = &tab.buffer else {
            return;
        };
        let headings = extract_headings(&buffer_text(buffer));
        if headings.is_empty() {
            self.outline_host.append(&panel_note("No headings"));
            return;
        }
        let path = tab.path.clone();
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
    }

    /// Plain-text preview of the active HTML buffer until the
    /// engine-backed Preview lands in E2.
    fn update_preview(&self) {
        let text = self
            .active_tab()
            .filter(|tab| tab.kind == FileKind::Html)
            .and_then(|tab| tab.buffer.as_ref())
            .map(|buffer| crate::epub::strip_html(&buffer_text(buffer)))
            .filter(|text| !text.trim().is_empty())
            .unwrap_or_else(|| "Nothing to preview".to_string());
        self.preview_label.set_label(&text);
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
