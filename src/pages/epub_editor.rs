//! The full EPUB editor: chapters use Kalam's native reader renderer, with
//! paragraph editing and a deep raw-source workspace for XHTML, HTML, and
//! CSS. Text edits write directly into the EPUB; the Patches panel remains
//! for legacy pending edits. Structural chapter-order and cover actions stay
//! disabled until they can write directly instead of creating patches.
//!
//! Paragraph editing reuses the reader's editor widget and planning machinery.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use relm4::prelude::*;
use sourceview5::prelude::*;

use kalam_reader::{ParagraphIdentity, ReaderView};

use crate::db::Catalog;
use crate::service::LibraryService;
use crate::pages::reader::{
    engine, toc_title, DirectEditSaved, EditScope, EditorWidget, InlineEdit,
};

/// The editor's messages. A slice of the reader's — paragraph taps,
/// selection reports (the click-away commit), the commit/verdict pair
/// — plus the chrome: chapter jumps, the review panel, the resource
/// browser/source pane, and the way back.
#[derive(Debug, Clone)]
pub enum EpubEditorMsg {
    /// A drag or tap changed the selection. No chip, no popovers here
    /// — this is only the click-away commit and the paragraph tap's
    /// own selection.
    EngineSelection(Option<(String, gtk::gdk::Rectangle)>),
    /// The selection's rect after the frame that moved it — the open
    /// editor rides along.
    EngineSelectionMoved(Option<gtk::gdk::Rectangle>),
    /// A paragraph tap: the identity when the tapped paragraph is
    /// editable, and the rect its editor opens over.
    EngineParagraphTap(Option<(ParagraphIdentity, gtk::gdk::Rectangle)>),
    /// Where the view is, so the chapter list can follow along.
    EnginePosition(usize, f64),
    /// Commit the open paragraph edit with this text.
    CommitInlineEdit(String),
    /// Abandon the open paragraph edit.
    CancelInlineEdit,
    /// The background direct save finished; failures carry toast text
    /// plus the editor's serial so the open buffer can be kept intact.
    InlineEditSaved(Result<DirectEditSaved, (String, u64)>),
    /// A chapter was chosen in the list — the row's position in the
    /// *displayed* order, which the pending spine op may have shuffled.
    TocSelect(usize),
    /// The review panel's open state — a plain desired state, not a
    /// toggle, so the button's own toggled signal and the panel's Done
    /// agree without a feedback loop.
    SetPatchesOpen(bool),
    /// The raw source pane's open state, the same plain-desired-state
    /// contract as the review panel's.
    SetSourceOpen(bool),
    /// Which part of the source workspace is visible.
    SetSourceLayout(SourceLayout),
    /// The preview mode: publisher-default appearance or current Kalam settings.
    SetPreviewAsShipped(bool),
    /// Expand or collapse the image/font asset group in the file rail.
    ToggleAssets,
    /// Close one tab without discarding a dirty buffer.
    CloseSourceTab(String),
    /// Restore the most recent saved text snapshot into the buffer.
    RestorePreviousSavedVersion,
    /// The background ZIP scan found safe editor-relevant EPUB resources.
    SourceResourcesLoaded(Result<Vec<engine::EditorResource>, String>),
    /// Open one file from the source-mode resource tree.
    SelectSourceResource(String),
    /// The source buffer changed by hand (not the pane's own loads —
    /// those are suppressed): dirty tracking and the preview's
    /// debounce start here.
    SourceEdited,
    /// Loading the selected editable file finished: source text or a refusal.
    SourceLoad(Result<engine::SourceEntry, String>),
    /// The direct source save finished: saved, stale, or an error. The
    /// source buffer remains available on failure.
    SourceSave(Result<engine::DirectWriteOutcome, String>),
    /// The Save button.
    SaveSource,
    /// The Revert button: throw the buffer away and reload the selected
    /// resource. Also the stale banner's way out.
    RevertSource,
    /// The preview debounce timer fired for this generation — only the
    /// last-scheduled generation is still wanted.
    SourceDebounce(u64),
    /// A preview raster or its failure finished for this generation.
    SourcePreview(Result<engine::PreviewPixels, (u64, String)>),
    /// Escape, from the page's key controller: dirty source reverts,
    /// clean source closes the pane, otherwise this leaves the page.
    Escape,
    /// Back: the reader (or wherever the editor was opened from) is
    /// one Back away.
    Close,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceLayout {
    Code,
    Split,
    Preview,
}

#[derive(Debug)]
pub enum EpubEditorOut {
    Close,
}

pub struct EpubEditorModel {
    service: LibraryService,
    book_id: i64,
    book_path: Option<PathBuf>,
    book_hash: String,
    book_title: String,
    book_subtitle: String,
    view: Option<ReaderView>,
    /// The overlay the view lives in; a committed edit swaps the view
    /// inside it, and the inline editor stays its child (the reader's
    /// rule, kept here for the same reason).
    overlay: Option<gtk::Overlay>,
    chapter_titles: Vec<String>,
    chapter_count: usize,
    chapter: usize,
    inline_edit: Option<InlineEdit>,
    next_edit_serial: u64,
    /// A reload (after a commit or a panel action) is in flight; taps
    /// and edits stand down until the new view lands.
    reloading: bool,
    patches_open: bool,
    /// The pending count when the panel opened — compared at close so
    /// a bake, an accept or a reject (all of which change what the
    /// engine should be showing) reloads the view.
    patches_snapshot: usize,
    patches_count: usize,
    font_family: Option<String>,
    font_px: u32,
    /// The deep source workspace: `source` is built once, when a book
    /// actually opened, and its widgets live for the page's life.
    source_open: bool,
    /// Source-workspace layout and preview fidelity survive source-pane closes.
    source_layout: SourceLayout,
    preview_open: bool,
    preview_as_shipped: bool,
    assets_expanded: bool,
    open_source_tabs: Vec<String>,
    source: Option<SourcePane>,
    /// Display order for the chapter list. Existing legacy spine patches
    /// remain visible in their proposed order; new chapter-order edits are
    /// deferred until they can write directly to the EPUB.
    chapter_order: Vec<usize>,
    editor_resources: Vec<engine::EditorResource>,
    selected_resource_href: String,
    resource_list_loaded: bool,
    resource_list_error: Option<String>,
}

/// The raw source pane: the widgets and the state they carry. Held in
/// the model like the engine view is — GTK refs on a main-thread
/// model, the shape this page already uses for `overlay`.
///
/// The mutable bits are `Cell`/`RefCell` because the pane's own
/// callbacks (the buffer's `changed`) must read them without touching
/// the model; everything runs on the main loop, so there is no racing.
struct SourcePane {
    buffer: sourceview5::Buffer,
    source_view: sourceview5::View,
    /// Native preview, outline, problems and live-CSS panels.
    preview_box: gtk::Notebook,
    editor_column: gtk::Box,
    picture: gtk::Picture,
    status_label: gtk::Label,
    stale_label: gtk::Label,
    chapter_label: gtk::Label,
    save_btn: gtk::Button,
    revert_btn: gtk::Button,
    tabs_box: gtk::Box,
    cursor_label: gtk::Label,
    footer_status: gtk::Label,
    syntax_label: gtk::Label,
    validation_label: gtk::Label,
    problems_label: gtk::Label,
    outline_list: gtk::Box,
    code_btn: gtk::Button,
    split_btn: gtk::Button,
    preview_btn: gtk::Button,
    format_buttons: Vec<gtk::Button>,
    history_button: gtk::MenuButton,
    history_restore_btn: gtk::Button,
    history_note: gtk::Label,
    history: RefCell<Option<(String, String)>>,
    editable: Cell<bool>,
    /// True while the pane is putting its own text into the buffer —
    /// the changed signal must not read that as an edit.
    suppress: Rc<Cell<bool>>,
    loading: Cell<bool>,
    saving: Cell<bool>,
    dirty: Cell<bool>,
    stale: Cell<bool>,
    /// The debounce generation: bumped by every edit; only the raster
    /// answering the current one is shown.
    preview_gen: Cell<u64>,
    href: RefCell<String>,
    /// The hash of the entry as loaded — every save's guard.
    baseline_hash: RefCell<String>,
    /// The entry as loaded, the dirty check's other half.
    loaded_text: RefCell<String>,
    preview_chapter: Cell<usize>,
    /// The (href, guard, text) triple a Fresh verdict stores — kept
    /// here because the worker only checks; the insert is the main
    /// loop's, with the catalog.
    save_attempt: RefCell<Option<(String, String, String)>>,
}

#[relm4::component(pub)]
impl Component for EpubEditorModel {
    type Init = (Arc<Catalog>, i64);
    type Input = EpubEditorMsg;
    type Output = EpubEditorOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Overlay {
            add_css_class: "kalam-editor",
            set_hexpand: true,
            set_vexpand: true,

            #[wrap(Some)]
            set_child = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_hexpand: true,
                set_vexpand: true,

                // Persistent editor chrome, following the deep-editor design.
                gtk::Box {
                    add_css_class: "kalam-editor-bar",
                    set_orientation: gtk::Orientation::Horizontal,

                    gtk::Button {
                        set_child: Some(&crate::icons::labelled(
                            "go-previous-symbolic",
                            16,
                            "Back",
                            6,
                        )),
                        add_css_class: "kalam-reader-back",
                        set_tooltip_text: Some("Back (Esc)"),
                        connect_clicked => EpubEditorMsg::Close,
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_hexpand: true,
                        set_halign: gtk::Align::Start,
                        set_margin_start: 10,
                        set_valign: gtk::Align::Center,

                        gtk::Label {
                            add_css_class: "kalam-editor-title",
                            add_css_class: "kalam-title-serif",
                            set_label: &model.book_title,
                            set_halign: gtk::Align::Start,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_single_line_mode: true,
                        },
                        gtk::Label {
                            add_css_class: "kalam-editor-subtitle",
                            set_label: &model.book_subtitle,
                            set_halign: gtk::Align::Start,
                        },
                    },

                    #[name = "source_btn"]
                    gtk::ToggleButton {
                        add_css_class: "kalam-editor-top-button",
                        set_label: "Deep editor",
                        set_tooltip_text: Some("Open the EPUB file workspace"),
                        #[watch]
                        set_sensitive: model.view.is_some(),
                        connect_toggled[sender] => move |btn| {
                            sender.input(EpubEditorMsg::SetSourceOpen(btn.is_active()));
                        },
                    },

                    #[name = "tools_btn"]
                    gtk::MenuButton {
                        add_css_class: "kalam-editor-top-button",
                        set_label: "Tools",
                        set_tooltip_text: Some("Review pending edits and editor tools"),
                    },

                    #[name = "history_btn"]
                    gtk::MenuButton {
                        add_css_class: "kalam-editor-top-button",
                        set_label: "History",
                        set_tooltip_text: Some("Restore a previous saved source version"),
                        #[watch]
                        set_visible: model.source_open,
                    },

                    #[name = "save_btn"]
                    gtk::Button {
                        add_css_class: "kalam-editor-save-button",
                        set_label: "Save",
                        set_tooltip_text: Some("Save this file directly into the EPUB"),
                        set_sensitive: false,
                        #[watch]
                        set_visible: model.source_open,
                        connect_clicked => EpubEditorMsg::SaveSource,
                    },
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_hexpand: true,
                    set_vexpand: true,

                    // One left rail, two useful views: book contents while
                    // proofreading; the EPUB's editable files while in Source.
                    #[name = "chapter_scroller"]
                    gtk::ScrolledWindow {
                        add_css_class: "kalam-editor-chapters",
                        set_width_request: 250,
                        set_hscrollbar_policy: gtk::PolicyType::Never,
                        set_vscrollbar_policy: gtk::PolicyType::Automatic,

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_margin_top: 8,
                            set_margin_bottom: 8,

                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                #[watch]
                                set_visible: !model.source_open,

                                gtk::Label {
                                    set_label: "Contents",
                                    set_halign: gtk::Align::Start,
                                    set_margin_start: 12,
                                    set_margin_bottom: 6,
                                    add_css_class: "kalam-editor-tree-heading",
                                },
                                #[name = "chapter_list"]
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_hexpand: true,
                                },
                            },

                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                #[watch]
                                set_visible: model.source_open,

                                gtk::Label {
                                    set_label: "BOOK FILES",
                                    set_halign: gtk::Align::Start,
                                    set_margin_start: 12,
                                    set_margin_bottom: 6,
                                    add_css_class: "kalam-editor-tree-heading",
                                },
                                #[name = "resource_list"]
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_hexpand: true,
                                },
                            },
                        },
                    },

                    #[name = "stage_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_hexpand: true,
                        set_vexpand: true,
                        add_css_class: "kalam-editor-stage",

                        // The proofreading surface and the raw source
                        // pane share this stage — one visible at a
                        // time, both alive. Swapping visibility, not
                        // children, so the engine keeps its scroll and
                        // the buffer keeps its undo.
                        #[name = "web_host"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_hexpand: true,
                            set_vexpand: true,
                            #[watch]
                            set_visible: !model.source_open,
                        },

                        #[name = "source_host"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_hexpand: true,
                            set_vexpand: true,
                            add_css_class: "kalam-editor-source",
                            #[watch]
                            set_visible: model.source_open,
                        },
                    },
                },
            },

            // The review panel over the right edge, the reader's
            // sidebar-shell shape: a revealer that slides, never a
            // resize of the work surface.
            add_overlay = &gtk::Revealer {
                add_css_class: "kalam-editor-panel-shell",
                set_transition_type: gtk::RevealerTransitionType::SlideLeft,
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Fill,
                #[watch]
                set_reveal_child: model.patches_open,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,

                    #[name = "patches_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                    },
                },
            },
        }
    }

    fn init(
        (catalog, book_id): Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let _timing = crate::timing::measure("editor_open");
        let _activity = crate::timing::activity("editor_open");

        let service = LibraryService::new(catalog.clone());
        let snap = service.reader(book_id);
        for err in &snap.errors {
            log::warn!("editor: {err}");
        }
        let book = snap.book.clone();
        let book_hash = book
            .as_ref()
            .map(|book| book.file_hash.clone())
            .unwrap_or_default();

        let mut open_failure: Option<String> = None;
        let (title, path, view, chapter_titles, chapter_count, font_bits) = match &book {
            Some(book) if matches!(book.format, crate::models::BookFormat::Epub) => {
                let patches = service
                    .catalog()
                    .get_pending_patches_for_book(book_id)
                    .unwrap_or_else(|err| {
                        log::warn!(
                            "patches for book {book_id} unreadable: {err:#} — opening unpatched"
                        );
                        Vec::new()
                    });
                let prefs = engine::current_engine_prefs(service.catalog());
                let font_px = prefs.font_px.max(1.0) as u32;
                let font_family = prefs.font_family.clone();
                match engine::open_engine(&book.file_path, prefs, patches) {
                    Ok(view) => {
                        let chapter_count = view.chapter_count();
                        let toc = view.toc();
                        let titles = (0..chapter_count)
                            .map(|i| {
                                toc_title(&toc, i)
                                    .unwrap_or_else(|| format!("Chapter {}", i + 1))
                            })
                            .collect();
                        (
                            book.title.clone(),
                            Some(book.file_path.clone()),
                            Some(view),
                            titles,
                            chapter_count,
                            Some((font_family, font_px)),
                        )
                    }
                    Err(err) => {
                        open_failure = Some(format!("{err:#}"));
                        (
                            book.title.clone(),
                            None,
                            None,
                            Vec::new(),
                            0,
                            None,
                        )
                    }
                }
            }
            _ => ("Missing book".to_string(), None, None, Vec::new(), 0, None),
        };
        let patches_count = service
            .catalog()
            .get_pending_patches_for_book(book_id)
            .map(|p| p.len())
            .unwrap_or(0);

        let mut model = EpubEditorModel {
            service,
            book_id,
            book_path: path,
            book_hash,
            book_title: title,
            book_subtitle: book
                .as_ref()
                .map(|book| {
                    if !book.authors.trim().is_empty() {
                        book.authors.trim().to_string()
                    } else {
                        book.series_display()
                            .unwrap_or_else(|| "EPUB DEEP EDITOR".to_string())
                    }
                })
                .unwrap_or_else(|| "EPUB DEEP EDITOR".to_string()),
            view,
            overlay: None,
            chapter_titles,
            chapter_count,
            chapter: 0,
            inline_edit: None,
            next_edit_serial: 0,
            reloading: false,
            patches_open: false,
            patches_snapshot: 0,
            patches_count,
            font_family: None,
            font_px: 17,
            source_open: false,
            source_layout: SourceLayout::Split,
            preview_open: true,
            preview_as_shipped: true,
            assets_expanded: false,
            open_source_tabs: Vec::new(),
            source: None,
            chapter_order: Vec::new(),
            editor_resources: Vec::new(),
            selected_resource_href: String::new(),
            resource_list_loaded: false,
            resource_list_error: None,
        };
        // A spine op left pending from an earlier session still owns
        // the list's order; the identity otherwise.
        model.sync_chapter_order();

        let widgets = view_output!();
        let tools_popover = build_tools_popover(&model, &sender);
        widgets.tools_btn.set_popover(Some(&tools_popover));
        if let Some((family, px)) = font_bits {
            model.font_family = family;
            model.font_px = px;
        }

        if let Some(view) = &model.view {
            let overlay = gtk::Overlay::new();
            overlay.set_hexpand(true);
            overlay.set_vexpand(true);
            overlay.set_child(Some(view.widget()));
            widgets.web_host.append(&overlay);
            model.overlay = Some(overlay);

            // The raw pane is built with the page, once: it is hidden
            // until the Source toggle says, and building it per-open
            // would throw away the buffer's undo stack every time.
            let pane = build_source_pane(
                &widgets.source_host,
                &widgets.save_btn,
                &widgets.history_btn,
                model.source_layout,
                model.preview_as_shipped,
                &sender,
            );
            model.source = Some(pane);

            wire(view, &sender);
            // The whole page is proofreading: taps edit, the pointer
            // never auto-hides over a work surface.
            view.set_proofreading(true);
            view.set_autohide_cursor(false);
            view.widget().grab_focus();
        } else {
            let message = match &open_failure {
                Some(err) => format!("Could not open book\n\n{err}\n\nPress Esc to go back."),
                None => "Missing book\n\nThis library row has no EPUB file to edit.\n\nPress Esc to go back.".to_string(),
            };
            let label = gtk::Label::new(Some(&message));
            label.set_wrap(true);
            label.set_justify(gtk::Justification::Center);
            label.set_halign(gtk::Align::Center);
            label.set_valign(gtk::Align::Center);
            label.set_margin_top(24);
            label.set_margin_bottom(24);
            label.set_margin_start(24);
            label.set_margin_end(24);
            label.add_css_class("kalam-reader-open-error");
            widgets.web_host.append(&label);
        }

        rebuild_chapter_list(&widgets.chapter_list, &model, &sender);
        rebuild_source_resource_list(&widgets.resource_list, &model, &sender);
        if let Some(pane) = &model.source {
            rebuild_source_tabs(&pane.tabs_box, &model, pane, None, &sender);
        }
        if let Some(path) = model.book_path.clone() {
            let (tx, rx) = async_channel::unbounded::<
                Result<Vec<engine::EditorResource>, String>,
            >();
            let itx = sender.input_sender().clone();
            gtk::glib::spawn_future_local(async move {
                if let Ok(verdict) = rx.recv().await {
                    let _ = itx.send(EpubEditorMsg::SourceResourcesLoaded(verdict));
                }
            });
            engine::load_editor_resources(path, tx);
        }
        refresh_patches_panel(&model, &widgets.patches_host, &sender);
        // Escape's ladder: a dirty source buffer reverts, a clean
        // source pane closes, and only then does Escape leave the
        // page. An open paragraph editor's own key controller stops
        // Escape first, so none of these ever fight.
        let key = gtk::EventControllerKey::new();
        let tx = sender.input_sender().clone();
        key.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gtk::gdk::Key::Escape {
                let _ = tx.send(EpubEditorMsg::Escape);
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        root.add_controller(key);

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            EpubEditorMsg::Close => {
                // Back is the whole way out; a dirty source buffer is
                // the one thing that can hold it — leaving would
                // silently drop the edit.
                if self
                    .source
                    .as_ref()
                    .is_some_and(source_buffer_is_dirty)
                {
                    crate::notify::info(
                        "Unsaved source",
                        "Save or revert the raw source before leaving.",
                    );
                    return;
                }
                self.close_editor(&sender);
            }
            EpubEditorMsg::Escape => {
                if self.source_open {
                    let Some(pane) = &self.source else {
                        self.close_editor(&sender);
                        return;
                    };
                    if source_buffer_is_dirty(pane) || pane.stale.get() {
                        // First Esc: back to the loaded text. A
                        // second Esc (clean now) closes the pane.
                        self.load_source(&sender);
                    } else {
                        self.source_open = false;
                        root.remove_css_class("kalam-editor-deep");
                        widgets.source_btn.set_active(false);
                    }
                } else {
                    self.close_editor(&sender);
                }
            }
            EpubEditorMsg::TocSelect(idx) => {
                // The row's position in the displayed order; the
                // engine only ever hears real spine indices.
                let Some(&real) = self.chapter_order.get(idx) else {
                    return;
                };
                if let Some(view) = &self.view {
                    if real != self.chapter {
                        let dirty = self.source.as_ref().is_some_and(source_buffer_is_dirty);
                        if self.source_open && dirty {
                            crate::notify::info(
                                "Unsaved source",
                                "Save or revert the raw source before changing chapter.",
                            );
                            return;
                        }
                        view.goto_chapter(real, 0.0);
                        self.chapter = real;
                        rebuild_chapter_list(&widgets.chapter_list, self, &sender);
                        if self.source_open {
                            self.load_source(&sender);
                        }
                    }
                }
            }
            EpubEditorMsg::EnginePosition(chapter, _fraction) => {
                // The engine reports real spine indices; the list
                // highlights the row whose displayed chapter it is.
                if chapter != self.chapter && chapter < self.chapter_count {
                    self.chapter = chapter;
                    rebuild_chapter_list(&widgets.chapter_list, self, &sender);
                }
            }
            EpubEditorMsg::EngineSelection(sel) => {
                // The click-away commit: a fresh selection (the tap
                // that made it, or a drag) while an editor stands is
                // the commit; a cleared selection is too.
                if sel.is_some() && self.inline_edit.is_some() {
                    if let Some(edit) = &self.inline_edit {
                        sender.input(EpubEditorMsg::CommitInlineEdit(edit.editor.text()));
                    }
                }
                if sel.is_none() && self.inline_edit.is_some() {
                    if let Some(edit) = &self.inline_edit {
                        sender.input(EpubEditorMsg::CommitInlineEdit(edit.editor.text()));
                    }
                }
            }
            EpubEditorMsg::EngineSelectionMoved(rect) => {
                if let Some(rect) = rect {
                    if let Some(edit) = &self.inline_edit {
                        engine::position_inline_editor(edit.editor.widget(), &rect);
                    }
                }
            }
            EpubEditorMsg::EngineParagraphTap(tap) => {
                if self.inline_edit.is_some() || self.reloading {
                    // An editor already stood: the tap's selection
                    // report already committed it, and its verdict is
                    // pending — a second tap waits for the page to be
                    // quiet, exactly as in the reader.
                    return;
                }
                let Some(overlay) = self.overlay.clone() else { return };
                let Some((identity, rect)) = tap else {
                    crate::notify::info(
                        "Editing",
                        "That paragraph cannot be edited here.",
                    );
                    return;
                };
                let (textview, provider) = engine::build_paragraph_editor(
                    &identity.text,
                    self.font_family.as_deref(),
                    self.font_px,
                    {
                        let tx = sender.input_sender().clone();
                        move |text| {
                            let _ = tx.send(EpubEditorMsg::CommitInlineEdit(text));
                        }
                    },
                    {
                        let tx = sender.input_sender().clone();
                        move || {
                            let _ = tx.send(EpubEditorMsg::CancelInlineEdit);
                        }
                    },
                );
                engine::position_inline_editor(&textview, &rect);
                overlay.add_overlay(&textview);
                textview.grab_focus();
                let serial = self.next_edit_serial;
                self.next_edit_serial += 1;
                self.inline_edit = Some(InlineEdit {
                    original: identity.text,
                    chapter: self.chapter,
                    scope: EditScope::Paragraph {
                        prev: identity.prev,
                        next: identity.next,
                    },
                    serial,
                    committed: false,
                    editor: EditorWidget::Paragraph(textview),
                    provider,
                });
            }
            EpubEditorMsg::CommitInlineEdit(corrected) => {
                let Some(edit) = self.inline_edit.as_ref() else { return };
                if edit.committed {
                    return;
                }
                if corrected == edit.original {
                    crate::notify::info("Nothing to fix", "The text already says that.");
                    return;
                }
                let Some(path) = self.book_path.clone() else { return };
                let chapter = edit.chapter;
                let original = edit.original.clone();
                let serial = edit.serial;
                let paragraph_context = match &edit.scope {
                    EditScope::Selection => None,
                    EditScope::Paragraph { prev, next } => Some((prev.clone(), next.clone())),
                };
                let service = self.service.clone();
                let (verdict_tx, verdict_rx) =
                    async_channel::unbounded::<Result<DirectEditSaved, (String, u64)>>();
                let tx = sender.input_sender().clone();
                gtk::glib::spawn_future_local(async move {
                    if let Ok(verdict) = verdict_rx.recv().await {
                        let _ = tx.send(EpubEditorMsg::InlineEditSaved(verdict));
                    }
                });
                engine::save_inline_edit(
                    service,
                    self.book_id,
                    path,
                    self.book_hash.clone(),
                    chapter,
                    original,
                    paragraph_context,
                    corrected,
                    crate::epub_sanitizer::DirectSaveBackup::Timestamped,
                    serial,
                    verdict_tx,
                );
                if let Some(edit) = self.inline_edit.as_mut() {
                    edit.committed = true;
                    edit.editor.widget().set_sensitive(false);
                }
            }
            EpubEditorMsg::CancelInlineEdit => {
                if self
                    .inline_edit
                    .as_ref()
                    .is_some_and(|edit| edit.committed)
                {
                    crate::notify::info("Saving edit", "Wait for this edit to finish saving.");
                    return;
                }
                self.close_inline_edit();
            }
            EpubEditorMsg::InlineEditSaved(verdict) => {
                match verdict {
                    Ok(saved) => {
                        self.book_hash = saved.book_hash.clone();
                        let current = self
                            .inline_edit
                            .as_ref()
                            .is_some_and(|edit| edit.serial == saved.serial);
                        if current {
                            self.close_inline_edit();
                        }
                        if self.inline_edit.is_none() {
                            self.reload(&sender);
                            if self.source_open
                                && self.source.as_ref().is_some_and(|pane| {
                                    !source_buffer_is_dirty(pane)
                                })
                            {
                                self.load_source(&sender);
                            }
                        }
                        if let Some(warning) = saved.warning {
                            crate::notify::info("Saved with a warning", &warning);
                        } else {
                            crate::notify::success(
                                "Saved to the EPUB",
                                "The edit is now permanent.",
                            );
                        }
                    }
                    Err((message, serial)) => {
                        crate::notify::error("Could not save the edit", &message);
                        if let Some(edit) = self.inline_edit.as_mut() {
                            if edit.serial == serial {
                                edit.committed = false;
                                edit.editor.widget().set_sensitive(true);
                            }
                        }
                    }
                }
            }
            EpubEditorMsg::SetPatchesOpen(open) => {
                if open == self.patches_open {
                    // The button's toggled signal fires for programmatic
                    // transitions too; this is that echo, not a click.
                    return;
                }
                if open {
                    self.patches_snapshot = self.patches_count;
                } else {
                    let current_count = self.pending_count();
                    let changed = current_count != self.patches_snapshot;
                    self.patches_count = current_count;
                    if changed {
                        // A legacy bake may have changed the EPUB and its
                        // catalog hash; refresh this editor's stale-write baseline.
                        self.refresh_book_hash();
                        // The panel baked, accepted or rejected while it
                        // was open: what the engine is showing is stale.
                        self.reload(&sender);
                        // And what a standing source pane loaded is past
                        // too — a bake rewrites the entry for real. Clean
                        // panes re-read; dirty ones meet their guard at
                        // save time.
                        if self.source_open
                            && self
                                .source
                                .as_ref()
                                .is_some_and(|pane| !source_buffer_is_dirty(pane))
                        {
                            self.load_source(&sender);
                        }
                    }
                }
                self.patches_open = open;
            }
            EpubEditorMsg::SetSourceOpen(open) => {
                if open == self.source_open {
                    // The toggle's own programmatic echo, not a click.
                    return;
                }
                let pane_ready = self.source.as_ref().is_some_and(|pane| {
                    !pane.loading.get() && !pane.saving.get()
                });
                if open && !pane_ready {
                    // Nothing to open (no book) or a load already in
                    // flight: snap the toggle back.
                    widgets.source_btn.set_active(false);
                    return;
                }
                if !open {
                    if self
                        .source
                        .as_ref()
                        .is_some_and(source_buffer_is_dirty)
                    {
                        crate::notify::info(
                            "Unsaved source",
                            "Save or revert the raw source before closing it.",
                        );
                        widgets.source_btn.set_active(true);
                        return;
                    }
                    if let Some(view) = &self.view {
                        view.widget().grab_focus();
                    }
                }
                self.source_open = open;
                if open {
                    root.add_css_class("kalam-editor-deep");
                    self.load_source(&sender);
                } else {
                    root.remove_css_class("kalam-editor-deep");
                }
            }
            EpubEditorMsg::SourceResourcesLoaded(verdict) => {
                self.resource_list_loaded = true;
                match verdict {
                    Ok(resources) => {
                        self.resource_list_error = None;
                        if self.selected_resource_href.is_empty() {
                            self.selected_resource_href = resources
                                .iter()
                                .find(|resource| {
                                    resource.editable && resource.spine_index == Some(self.chapter)
                                })
                                .or_else(|| resources.iter().find(|resource| resource.editable))
                                .map(|resource| resource.href.clone())
                                .unwrap_or_default();
                        }
                        self.editor_resources = resources;
                    }
                    Err(message) => {
                        self.resource_list_error = Some(message.clone());
                        self.editor_resources.clear();
                        crate::notify::error("Could not list EPUB files", &message);
                    }
                }
                rebuild_source_resource_list(&widgets.resource_list, self, &sender);
                if self.source_open && !self.selected_resource_href.is_empty() {
                    self.load_source(&sender);
                }
            }
            EpubEditorMsg::SelectSourceResource(href) => {
                if href == self.selected_resource_href
                    || !self.editor_resources.iter().any(|resource| resource.href == href)
                {
                    return;
                }
                if let Some(pane) = &self.source {
                    if source_buffer_is_dirty(pane) || pane.saving.get() {
                        crate::notify::info(
                            "Unsaved source",
                            "Save or revert the open file before switching to another EPUB file.",
                        );
                        return;
                    }
                    if pane.loading.get() {
                        crate::notify::info(
                            "File is still loading",
                            "Wait for the current EPUB file to finish loading before switching.",
                        );
                        return;
                    }
                }
                let spine_index = self
                    .editor_resources
                    .iter()
                    .find(|resource| resource.href == href)
                    .and_then(|resource| resource.spine_index);
                self.selected_resource_href = href;
                if let Some(chapter) = spine_index {
                    self.chapter = chapter;
                    if let Some(view) = &self.view {
                        view.goto_chapter(chapter, 0.0);
                    }
                    rebuild_chapter_list(&widgets.chapter_list, self, &sender);
                }
                if let Some(pane) = &self.source {
                    pane.picture.set_paintable(None::<&gtk::gdk::Paintable>);
                    pane.status_label.set_text("Loading selected EPUB file…");
                }
                rebuild_source_resource_list(&widgets.resource_list, self, &sender);
                if self.source_open {
                    self.load_source(&sender);
                }
            }
            EpubEditorMsg::SetSourceLayout(layout) => {
                if self.source_layout == layout {
                    return;
                }
                self.source_layout = layout;
                self.preview_open = layout != SourceLayout::Code;
                if let Some(pane) = &self.source {
                    apply_source_layout(pane, layout);
                    let generation = pane.preview_gen.get() + 1;
                    pane.preview_gen.set(generation);
                    if self.preview_open && self.source_open && !pane.loading.get() {
                        self.kick_preview(&sender);
                    }
                }
            }
            EpubEditorMsg::SetPreviewAsShipped(as_shipped) => {
                if self.preview_as_shipped == as_shipped {
                    return;
                }
                self.preview_as_shipped = as_shipped;
                if let Some(pane) = &self.source {
                    pane.preview_gen.set(pane.preview_gen.get() + 1);
                    if self.source_open && self.preview_open {
                        self.kick_preview(&sender);
                    }
                }
            }
            EpubEditorMsg::ToggleAssets => {
                self.assets_expanded = !self.assets_expanded;
                rebuild_source_resource_list(&widgets.resource_list, self, &sender);
            }
            EpubEditorMsg::CloseSourceTab(href) => {
                let Some(pane) = &self.source else { return };
                if pane.loading.get() || pane.saving.get() {
                    crate::notify::info(
                        "File is busy",
                        "Wait for the current load or save to finish before closing a file tab.",
                    );
                    return;
                }
                if href == self.selected_resource_href && source_buffer_is_dirty(pane) {
                    crate::notify::info(
                        "Unsaved source",
                        "Save or revert this file before closing its tab.",
                    );
                    return;
                }
                let Some(index) = self.open_source_tabs.iter().position(|open| open == &href) else {
                    return;
                };
                self.open_source_tabs.remove(index);
                if href == self.selected_resource_href {
                    if let Some(next_href) = self
                        .open_source_tabs
                        .get(index.min(self.open_source_tabs.len().saturating_sub(1)))
                        .cloned()
                    {
                        self.selected_resource_href = next_href;
                        self.load_source(&sender);
                    } else {
                        self.selected_resource_href.clear();
                        pane.suppress.set(true);
                        pane.buffer.set_text("");
                        pane.suppress.set(false);
                        pane.href.borrow_mut().clear();
                        pane.loaded_text.borrow_mut().clear();
                        pane.editable.set(false);
                        update_history_controls(pane);
                        pane.source_view.set_editable(false);
                        pane.save_btn.set_sensitive(false);
                        pane.revert_btn.set_sensitive(false);
                        pane.chapter_label.set_text("No file open");
                        pane.syntax_label.set_text("—");
                        pane.validation_label.set_text("No file open");
                        pane.footer_status.set_text("Choose an EPUB resource from the file rail");
                        pane.picture.set_paintable(None::<&gtk::gdk::Paintable>);
                    }
                }
                rebuild_source_tabs(&pane.tabs_box, self, pane, None, &sender);
                rebuild_source_resource_list(&widgets.resource_list, self, &sender);
            }
            EpubEditorMsg::RestorePreviousSavedVersion => {
                let Some(pane) = &self.source else { return };
                if pane.loading.get() || pane.saving.get() || pane.stale.get() || !pane.editable.get() {
                    return;
                }
                if source_buffer_is_dirty(pane) {
                    crate::notify::info(
                        "Unsaved source",
                        "Save or revert the current buffer before restoring a saved version.",
                    );
                    return;
                }
                let current_href = pane.href.borrow().clone();
                let Some(previous) = pane
                    .history
                    .borrow()
                    .as_ref()
                    .filter(|(history_href, _)| history_href == &current_href)
                    .map(|(_, text)| text.clone())
                else {
                    return;
                };
                pane.suppress.set(true);
                pane.buffer.set_text(&previous);
                pane.suppress.set(false);
                pane.dirty.set(previous != *pane.loaded_text.borrow());
                pane.save_btn.set_sensitive(pane.dirty.get());
                pane.revert_btn.set_sensitive(pane.dirty.get());
                pane.footer_status.set_text("Previous version restored · unsaved");
                pane.validation_label.set_text("Checks run on Save");
                rebuild_source_tabs(&pane.tabs_box, self, pane, None, &sender);
                sender.input(EpubEditorMsg::SourceEdited);
            }
            EpubEditorMsg::SourceEdited => {
                let Some(pane) = &self.source else { return };
                if pane.suppress.get() || pane.loading.get() {
                    return;
                }
                let current_text = source_text(&pane.buffer);
                let dirty = pane.editable.get() && current_text != *pane.loaded_text.borrow();
                let dirty_changed = dirty != pane.dirty.get();
                pane.dirty.set(dirty);
                pane.save_btn.set_sensitive(dirty && !pane.stale.get());
                pane.revert_btn.set_sensitive(dirty || pane.stale.get());
                pane.footer_status.set_text(if dirty {
                    "Unsaved changes"
                } else if pane.editable.get() {
                    "Saved to the EPUB"
                } else {
                    "Read-only EPUB resource"
                });
                pane.validation_label.set_text(if dirty {
                    "Checks run on Save"
                } else {
                    "Ready"
                });
                if dirty {
                    pane.problems_label.set_text(
                        "Unsaved edits have not been checked yet. Save validates the resource before writing; a refusal leaves this buffer intact.",
                    );
                }
                update_cursor_status(&pane.buffer, &pane.cursor_label);
                // The dirty marker changes only once per edit session; don't
                // rebuild tab widgets on every keystroke.
                if dirty_changed {
                    rebuild_source_tabs(&pane.tabs_box, self, pane, Some(dirty), &sender);
                }
                let gen = pane.preview_gen.get() + 1;
                pane.preview_gen.set(gen);
                if self.preview_open && pane.editable.get() {
                    let tx = sender.input_sender().clone();
                    gtk::glib::timeout_add_local_once(
                        std::time::Duration::from_millis(700),
                        move || {
                            let _ = tx.send(EpubEditorMsg::SourceDebounce(gen));
                        },
                    );
                }
            }
            EpubEditorMsg::SourceLoad(verdict) => {
                let Some(pane) = &self.source else { return };
                pane.loading.set(false);
                match verdict {
                    Ok(entry) => {
                        if entry.href != self.selected_resource_href {
                            // A different file was chosen while this read
                            // was in flight. Discard the old verdict and
                            // load the currently selected resource.
                            self.load_source(&sender);
                            return;
                        }
                        pane.source_view.set_editable(entry.editable);
                        pane.editable.set(entry.editable);
                        let is_css = entry.href.to_ascii_lowercase().ends_with(".css");
                        for button in &pane.format_buttons {
                            button.set_sensitive(entry.editable && !is_css);
                        }
                        pane.suppress.set(true);
                        pane.buffer.set_text(&entry.text);
                        pane.suppress.set(false);
                        *pane.href.borrow_mut() = entry.href.clone();
                        update_history_controls(pane);
                        *pane.baseline_hash.borrow_mut() = entry.hash.clone();
                        *pane.loaded_text.borrow_mut() = entry.text.clone();
                        pane.preview_chapter.set(entry.preview_chapter);
                        pane.dirty.set(false);
                        update_cursor_status(&pane.buffer, &pane.cursor_label);
                        pane.stale.set(false);
                        pane.stale_label.set_visible(false);
                        pane.save_btn.set_sensitive(false);
                        pane.revert_btn.set_sensitive(false);
                        pane.chapter_label.set_text(&entry.href);
                        pane.syntax_label.set_text(resource_syntax(&entry.href));
                        pane.validation_label.set_text(if entry.editable {
                            "Ready"
                        } else {
                            "Read-only"
                        });
                        pane.problems_label.set_text(
                            "XHTML and CSS safety checks run before a direct write. A refused save keeps the editor buffer open.",
                        );
                        pane.footer_status.set_text(if entry.editable {
                            "Saved to the EPUB"
                        } else {
                            "Read-only EPUB resource"
                        });
                        if !self.open_source_tabs.contains(&entry.href) {
                            self.open_source_tabs.push(entry.href.clone());
                        }
                        rebuild_source_tabs(&pane.tabs_box, self, pane, Some(false), &sender);
                        if entry.text.len() <= 1024 * 1024 {
                            rebuild_outline(&pane.outline_list, &entry.text);
                        } else {
                            while let Some(child) = pane.outline_list.first_child() {
                                pane.outline_list.remove(&child);
                            }
                            append_empty_group_message(
                                &pane.outline_list,
                                "Outline is paused for resources larger than 1 MiB.",
                            );
                        }
                        // Choose source highlighting by the file type.
                        let lower_href = entry.href.to_ascii_lowercase();
                        let lang_id = if lower_href.ends_with(".css") {
                            "css"
                        } else if lower_href.ends_with(".html") || lower_href.ends_with(".htm") {
                            "html"
                        } else {
                            "xml"
                        };
                        if let Some(lang) = sourceview5::LanguageManager::new().language(lang_id) {
                            pane.buffer.set_language(Some(&lang));
                        }
                        if self.preview_open && entry.editable {
                            self.kick_preview(&sender);
                        } else {
                            pane.picture.set_paintable(None::<&gtk::gdk::Paintable>);
                            pane.status_label.set_text(
                                "Preview is available for XHTML, HTML, and CSS resources.",
                            );
                        }
                    }
                    Err(toast) => {
                        // A bad resource must not close the editor or replace
                        // the previous buffer. Restore its tree selection when
                        // one was already loaded; otherwise keep the error in
                        // the open pane so the user can choose another file.
                        crate::notify::error("Could not open the EPUB file", &toast);
                        let previous_href = pane.href.borrow().clone();
                        pane.source_view.set_editable(pane.editable.get());
                        let is_css = previous_href.to_ascii_lowercase().ends_with(".css");
                        for button in &pane.format_buttons {
                            button.set_sensitive(pane.editable.get() && !is_css);
                        }
                        if !previous_href.is_empty() {
                            self.selected_resource_href = previous_href.clone();
                            if !self.open_source_tabs.contains(&previous_href) {
                                self.open_source_tabs.push(previous_href);
                            }
                            rebuild_source_tabs(&pane.tabs_box, self, pane, None, &sender);
                            rebuild_source_resource_list(
                                &widgets.resource_list,
                                self,
                                &sender,
                            );
                        }
                        pane.status_label.set_text(&toast);
                    }
                }
            }
            EpubEditorMsg::SaveSource => {
                let Some(pane) = &self.source else { return };
                if pane.loading.get() || pane.saving.get() || pane.stale.get() {
                    return;
                }
                if !pane.editable.get() {
                    crate::notify::info(
                        "Read-only resource",
                        "Only XHTML, HTML, and CSS files can be saved from the deep editor.",
                    );
                    return;
                }
                let text = source_text(&pane.buffer);
                if text == *pane.loaded_text.borrow() {
                    crate::notify::info("Nothing to fix", "The source already says that.");
                    return;
                }
                let Some(path) = self.book_path.clone() else { return };
                pane.saving.set(true);
                pane.save_btn.set_sensitive(false);
                pane.revert_btn.set_sensitive(false);
                pane.footer_status.set_text("Saving directly to the EPUB…");
                let href = pane.href.borrow().clone();
                let baseline = pane.baseline_hash.borrow().clone();
                *pane.save_attempt.borrow_mut() =
                    Some((href.clone(), baseline.clone(), text.clone()));
                let (tx, rx) =
                    async_channel::unbounded::<Result<engine::DirectWriteOutcome, String>>();
                let itx = sender.input_sender().clone();
                gtk::glib::spawn_future_local(async move {
                    if let Ok(verdict) = rx.recv().await {
                        let _ = itx.send(EpubEditorMsg::SourceSave(verdict));
                    }
                });
                engine::save_source_entry_direct(
                    self.service.clone(),
                    self.book_id,
                    path,
                    self.book_hash.clone(),
                    href,
                    baseline,
                    text,
                    crate::epub_sanitizer::DirectSaveBackup::Timestamped,
                    tx,
                );
            }
            EpubEditorMsg::RevertSource => {
                // The buffer goes back to whatever the book says now —
                // which is also the stale banner's way out: the reload
                // clears both the text and the flag.
                self.load_source(&sender);
            }
            EpubEditorMsg::SourceSave(verdict) => {
                let Some(pane) = &self.source else { return };
                pane.saving.set(false);
                match verdict {
                    Ok(engine::DirectWriteOutcome::Saved {
                        entry_hash,
                        book_hash,
                        warning,
                    }) => {
                        let Some((_href, _baseline, text)) = pane.save_attempt.borrow_mut().take()
                        else {
                            return;
                        };
                        // Keep one bounded in-memory restore point; the full,
                        // timestamped EPUB backup remains the durable history.
                        let previous = pane.loaded_text.borrow().clone();
                        let current_href = pane.href.borrow().clone();
                        *pane.history.borrow_mut() =
                            (previous.len() <= 8 * 1024 * 1024 && !previous.is_empty())
                                .then_some((current_href, previous));
                        update_history_controls(pane);
                        // The worker saved this snapshot of the buffer.
                        // Any typing that raced the write stays dirty.
                        *pane.loaded_text.borrow_mut() = text;
                        *pane.baseline_hash.borrow_mut() = entry_hash;
                        self.book_hash = book_hash;
                        let still_dirty = source_text(&pane.buffer) != *pane.loaded_text.borrow();
                        pane.dirty.set(still_dirty);
                        pane.save_btn.set_sensitive(still_dirty && pane.editable.get());
                        pane.revert_btn.set_sensitive(still_dirty);
                        pane.footer_status.set_text(if still_dirty {
                            "Saved · newer edits remain unsaved"
                        } else {
                            "Saved to the EPUB"
                        });
                        pane.validation_label.set_text(if still_dirty {
                            "New edits not checked"
                        } else {
                            "Saved"
                        });
                        pane.problems_label.set_text(
                            "The latest direct save completed. Timestamped backup and stale-write checks were applied.",
                        );
                        rebuild_source_tabs(&pane.tabs_box, self, pane, Some(still_dirty), &sender);
                        let visible_text = source_text(&pane.buffer);
                        if visible_text.len() <= 1024 * 1024 {
                            rebuild_outline(&pane.outline_list, &visible_text);
                        }
                        pane.stale.set(false);
                        pane.stale_label.set_visible(false);
                        self.reload(&sender);
                        if self.source_open
                            && self.source.as_ref().is_some_and(|pane| {
                                !source_buffer_is_dirty(pane)
                                    && pane.preview_chapter.get() != self.chapter
                            })
                        {
                            self.load_source(&sender);
                        }
                        if let Some(warning) = warning {
                            crate::notify::info("Saved with a warning", &warning);
                        } else {
                            crate::notify::success(
                                "Saved to the EPUB",
                                "The source edit is now permanent.",
                            );
                        }
                    }
                    Ok(engine::DirectWriteOutcome::Stale) => {
                        *pane.save_attempt.borrow_mut() = None;
                        pane.stale.set(true);
                        pane.save_btn.set_sensitive(false);
                        pane.footer_status.set_text("Stale file · reload before saving");
                        pane.validation_label.set_text("Stale source");
                        pane.problems_label.set_text(
                            "The EPUB changed after this resource loaded. No write was made; revert to reload the current text, then reapply your edit.",
                        );
                        pane.stale_label.set_visible(true);
                        crate::notify::info(
                            "The EPUB file changed",
                            "This EPUB file changed after the pane loaded it. Revert to load the current text, then make the edit again.",
                        );
                    }
                    Err(message) => {
                        *pane.save_attempt.borrow_mut() = None;
                        let dirty = source_buffer_is_dirty(pane);
                        pane.save_btn.set_sensitive(dirty && !pane.stale.get() && pane.editable.get());
                        pane.revert_btn.set_sensitive(dirty || pane.stale.get());
                        pane.footer_status.set_text("Save refused · your buffer is preserved");
                        pane.validation_label.set_text("Save refused");
                        pane.problems_label.set_text(&message);
                        crate::notify::error("Could not save the source edit", &message);
                    }
                }
            }
            EpubEditorMsg::SourceDebounce(gen) => {
                // Only the last edit's timer matters; every earlier
                // one fires into this check and dies.
                if self.source_open
                    && self.preview_open
                    && self
                        .source
                        .as_ref()
                        .is_some_and(|pane| pane.preview_gen.get() == gen)
                {
                    self.kick_preview(&sender);
                }
            }
            EpubEditorMsg::SourcePreview(verdict) => {
                let Some(pane) = &self.source else { return };
                match verdict {
                    Ok(pixels) => {
                        if pixels.generation != pane.preview_gen.get() {
                            // An edit landed since this raster started;
                            // a newer one is already on its way.
                            return;
                        }
                        let bytes = gtk::glib::Bytes::from_owned(pixels.rgba);
                        let texture = gtk::gdk::MemoryTexture::new(
                            pixels.width as i32,
                            pixels.height as i32,
                            gtk::gdk::MemoryFormat::R8g8b8a8Premultiplied,
                            &bytes,
                            pixels.width as usize * 4,
                        );
                        pane.picture.set_paintable(Some(&texture));
                        pane.status_label
                            .set_text("Native preview of the current chapter.");
                    }
                    Err((generation, toast)) => {
                        if generation != pane.preview_gen.get() {
                            return;
                        }
                        pane.picture.set_paintable(None::<&gtk::gdk::Paintable>);
                        pane.status_label.set_text(&toast);
                    }
                }
            }
        }
    }
}

impl EpubEditorModel {
    /// Refresh the file-version guard after the legacy review panel may
    /// have applied a bake to this book.
    fn refresh_book_hash(&mut self) {
        match self.service.catalog().get_book(self.book_id) {
            Ok(Some(book)) => self.book_hash = book.file_hash,
            Ok(None) => log::warn!("editor: book {} disappeared from the catalog", self.book_id),
            Err(err) => log::warn!("editor: could not refresh book hash: {err}"),
        }
    }

    /// The pending count, for the badge.
    fn pending_count(&self) -> usize {
        self.service
            .catalog()
            .get_pending_patches_for_book(self.book_id)
            .map(|p| p.len())
            .unwrap_or(0)
    }

    /// The book's pending patches, logged-not-fatal when unreadable —
    /// the reload's own rule. Every source worker takes them so spine
    /// resources are loaded and previewed as the reader currently shows them.
    fn pending_patches(&self) -> Vec<crate::db::PatchRecord> {
        self.service
            .catalog()
            .get_pending_patches_for_book(self.book_id)
            .unwrap_or_else(|err| {
                log::warn!(
                    "patches for book {} unreadable: {err:#} — reading unpatched",
                    self.book_id
                );
                Vec::new()
            })
    }

    /// Re-read the pending reading order from the patches: the last
    /// pending spine op's stored order when it is a true permutation of
    /// the chapters there are, the book's own order otherwise (a bad
    /// row is logged and ignored — the list must always show something
    /// coherent, never a hole).
    fn sync_chapter_order(&mut self) {
        let identity: Vec<usize> = (0..self.chapter_count).collect();
        let mut order = identity.clone();
        // rev().find, not filter().next_back(): clippy's double-ended
        // lints police the whole family — last, next_back, and the
        // filter-composed form each have their own lint.
        if let Some(op) = self
            .pending_patches()
            .iter()
            .rev()
            .find(|p| p.kind == "spine")
        {
            if let Some(parsed) = crate::epub_sanitizer::parse_spine_order(&op.replace_text) {
                let mut sorted = parsed.clone();
                sorted.sort_unstable();
                if parsed.len() == self.chapter_count && sorted == identity {
                    order = parsed;
                } else {
                    log::warn!(
                        "editor: pending spine op {} is not a permutation of {} chapters — \
                         showing the book's own order",
                        op.id,
                        self.chapter_count
                    );
                }
            }
        }
        self.chapter_order = order;
    }



    fn close_inline_edit(&mut self) {
        if let Some(edit) = self.inline_edit.take() {
            engine::remove_inline_edit_provider(&edit.provider);
            if let Some(overlay) = &self.overlay {
                overlay.remove_overlay(edit.editor.widget());
            }
            if let Some(view) = &self.view {
                view.widget().grab_focus();
            }
        }
    }

    /// The way back, shared by Back and the last rung of Escape.
    fn close_editor(&mut self, sender: &ComponentSender<Self>) {
        if let Some(view) = &self.view {
            view.close();
        }
        sender.output(EpubEditorOut::Close).ok();
    }

    /// Load the selected XHTML/HTML/CSS resource off the UI thread. One
    /// flight at a time (`loading`); a resource selected during a read is
    /// picked up when that earlier verdict arrives.
    fn load_source(&mut self, sender: &ComponentSender<Self>) {
        let Some(pane) = &self.source else { return };
        if pane.loading.get() || pane.saving.get() {
            return;
        }
        let Some(path) = self.book_path.clone() else { return };
        let Some(resource) = self
            .editor_resources
            .iter()
            .find(|resource| resource.href == self.selected_resource_href)
        else {
            if let Some(error) = self.resource_list_error.as_deref() {
                pane.status_label.set_text(error);
            } else if !self.resource_list_loaded {
                pane.status_label.set_text("Finding text, styles, assets, and contents…");
            } else {
                pane.status_label
                    .set_text("Choose an editable EPUB file from the left-hand list.");
            }
            return;
        };
        let href = resource.href.clone();
        let preview_chapter = resource.spine_index.unwrap_or(self.chapter);
        pane.preview_gen.set(pane.preview_gen.get() + 1);
        pane.picture.set_paintable(None::<&gtk::gdk::Paintable>);
        pane.source_view.set_editable(false);
        for button in &pane.format_buttons {
            button.set_sensitive(false);
        }
        pane.loading.set(true);
        pane.stale.set(false);
        pane.stale_label.set_visible(false);
        pane.save_btn.set_sensitive(false);
        pane.status_label.set_text("Loading selected EPUB file…");
        let prior = self.pending_patches();
        let (tx, rx) = async_channel::unbounded::<Result<engine::SourceEntry, String>>();
        let itx = sender.input_sender().clone();
        gtk::glib::spawn_future_local(async move {
            if let Ok(verdict) = rx.recv().await {
                let _ = itx.send(EpubEditorMsg::SourceLoad(verdict));
            }
        });
        engine::load_source_resource(path, href, preview_chapter, prior, tx);
    }

    /// Render the selected resource through the headless preview, with
    /// the debounce generation stamped on the flight. Called by the
    /// toggle (immediately) and by the debounce timer (after quiet).
    fn kick_preview(&mut self, sender: &ComponentSender<Self>) {
        let Some(pane) = &self.source else { return };
        if pane.loading.get()
            || !pane.editable.get()
            || !self.preview_open
            || !self.source_open
        {
            return;
        }
        let Some(path) = self.book_path.clone() else { return };
        let mut prefs = engine::current_engine_prefs(self.service.catalog());
        if self.preview_as_shipped {
            prefs.publisher_styles = true;
        }
        let fonts_dir = Some(crate::paths::fonts_dir());
        let href = pane.href.borrow().clone();
        let text = source_text(&pane.buffer);
        let spine = pane.preview_chapter.get();
        let gen = pane.preview_gen.get();
        let prior = self.pending_patches();
        let page = preview_page_size(&prefs);
        let (tx, rx) =
            async_channel::unbounded::<Result<engine::PreviewPixels, (u64, String)>>();
        let itx = sender.input_sender().clone();
        gtk::glib::spawn_future_local(async move {
            if let Ok(verdict) = rx.recv().await {
                let _ = itx.send(EpubEditorMsg::SourcePreview(verdict));
            }
        });
        engine::render_source_preview(
            path, prefs, fonts_dir, href, text, prior, spine, gen, page, tx,
        );
        pane.status_label.set_text("Rendering preview…");
    }

    /// Reopen the book the editor is editing — after a commit, or
    /// after the review panel changed what should be showing. The
    /// reader's reload shape: open before touching the old view, swap
    /// the view inside the overlay (never a container mutation of a
    /// list, §48), restore the chapter, and time it (§45).
    fn reload(&mut self, sender: &ComponentSender<Self>) {
        if self.reloading {
            return;
        }
        self.reloading = true;
        let _timing = crate::timing::measure("editor_reload");
        let _activity = crate::timing::activity("editor_reload");

        self.close_inline_edit();

        let Some(path) = self.book_path.clone() else {
            self.reloading = false;
            return;
        };
        let prefs = engine::current_engine_prefs(self.service.catalog());
        let patches = self
            .service
            .catalog()
            .get_pending_patches_for_book(self.book_id)
            .unwrap_or_else(|err| {
                log::warn!(
                    "patches for book {} unreadable: {err:#} — reloading unpatched",
                    self.book_id
                );
                Vec::new()
            });
        let new_view = match engine::open_engine(&path, prefs, patches) {
            Ok(view) => view,
            Err(err) => {
                self.reloading = false;
                crate::notify::error(
                    "Could not reopen the book",
                    &format!("{err:#}\nThe fix is saved and will show next time you open it."),
                );
                return;
            }
        };

        // Where the editor was, said as the old view would say it —
        // by href, because a bake may have reordered the spine
        // underneath (a chapter index would land somewhere else; the
        // locator's href survived the move). The locator re-anchors by
        // quote when the text differs; the index is only the fallback
        // when there was no position at all.
        let locator = self.view.as_ref().and_then(|v| v.locator());
        if let Some(old) = self.view.take() {
            old.close();
        }
        self.view = Some(new_view.clone());
        if let Some(overlay) = &self.overlay {
            overlay.set_child(Some(new_view.widget()));
        }
        wire(&new_view, sender);
        new_view.set_proofreading(true);
        new_view.set_autohide_cursor(false);
        let restored = locator
            .as_ref()
            .is_some_and(|loc| new_view.goto_locator(loc, false));
        if !restored {
            new_view.goto_chapter(self.chapter.min(self.chapter_count.saturating_sub(1)), 0.0);
        }
        new_view.widget().grab_focus();

        self.reloading = false;
    }
}

/// The editor's engine wiring: the paragraph-editing callbacks and the
/// position the chapter list follows. Nothing else — no search, no
/// highlights, no notes: the reader's `wire` is the reader's.
fn wire(view: &ReaderView, sender: &ComponentSender<EpubEditorModel>) {
    let tx = sender.input_sender().clone();
    view.connect_selection(move |sel: Option<&kalam_reader::SelectedText>| {
        let _ = tx.send(EpubEditorMsg::EngineSelection(
            sel.map(|s| (s.text.clone(), engine::gdk_rect(s.rect))),
        ));
    });

    let tx = sender.input_sender().clone();
    view.connect_selection_moved(move |rect: Option<kalam_reader::Rect>| {
        let _ = tx.send(EpubEditorMsg::EngineSelectionMoved(rect.map(engine::gdk_rect)));
    });

    let tx = sender.input_sender().clone();
    view.connect_paragraph_tap(move |tap: Option<kalam_reader::ParagraphTap>| {
        let _ = tx.send(EpubEditorMsg::EngineParagraphTap(tap.map(|tap| {
            (tap.identity, engine::gdk_rect(tap.rect))
        })));
    });

    let tx = sender.input_sender().clone();
    view.connect_position(move |pos: &kalam_reader::ReadingPosition| {
        let _ = tx.send(EpubEditorMsg::EnginePosition(pos.chapter, pos.fraction));
    });
}

/// The chapter list: a flat column of navigation rows, with the current
/// chapter marked. Existing legacy spine patches can still define the
/// displayed order; new structural editing is deferred to a later phase.
fn rebuild_chapter_list(
    list: &gtk::Box,
    model: &EpubEditorModel,
    sender: &ComponentSender<EpubEditorModel>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    for (pos, &real) in model.chapter_order.iter().enumerate() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
        row.add_css_class("kalam-editor-chapter-row");
        let title = model
            .chapter_titles
            .get(real)
            .cloned()
            .unwrap_or_else(|| format!("Chapter {}", real + 1));
        let btn = gtk::Button::with_label(&title);
        btn.add_css_class("kalam-editor-chapter");
        if real == model.chapter {
            btn.add_css_class("kalam-editor-chapter-current");
        }
        btn.set_hexpand(true);
        btn.set_halign(gtk::Align::Fill);
        let tx = sender.input_sender().clone();
        btn.connect_clicked(move |_| {
            let _ = tx.send(EpubEditorMsg::TocSelect(pos));
        });
        row.append(&btn);

        list.append(&row);
    }
}

/// Rebuild the source-mode rail as grouped EPUB resources. It deliberately
/// never exposes Markdown or unknown file types; binary assets are labels,
/// while the only files that can be saved are XHTML/HTML/CSS.
fn rebuild_source_resource_list(
    list: &gtk::Box,
    model: &EpubEditorModel,
    sender: &ComponentSender<EpubEditorModel>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    if model.editor_resources.is_empty() {
        let message = if let Some(error) = &model.resource_list_error {
            error.as_str()
        } else if model.resource_list_loaded {
            "No supported EPUB resources were found."
        } else {
            "Scanning EPUB resources…"
        };
        let label = gtk::Label::new(Some(message));
        label.set_wrap(true);
        label.set_halign(gtk::Align::Start);
        label.set_xalign(0.0);
        label.set_margin_start(12);
        label.set_margin_end(12);
        label.set_margin_top(8);
        label.add_css_class("kalam-editor-tree-empty");
        list.append(&label);
        return;
    }

    append_resource_group(list, model, sender, engine::ResourceGroup::Text, "TEXT");
    append_resource_group(list, model, sender, engine::ResourceGroup::Styles, "STYLES");

    let asset_count = model
        .editor_resources
        .iter()
        .filter(|resource| resource.group == engine::ResourceGroup::Assets)
        .count();
    let assets_button = gtk::Button::with_label(&format!(
        "IMAGES · FONTS   {}   {}",
        asset_count,
        if model.assets_expanded { "⌄" } else { "›" }
    ));
    assets_button.add_css_class("kalam-editor-tree-heading-button");
    assets_button.set_halign(gtk::Align::Fill);
    assets_button.set_hexpand(true);
    let tx = sender.input_sender().clone();
    assets_button.connect_clicked(move |_| {
        let _ = tx.send(EpubEditorMsg::ToggleAssets);
    });
    list.append(&assets_button);
    if model.assets_expanded {
        for resource in model
            .editor_resources
            .iter()
            .filter(|resource| resource.group == engine::ResourceGroup::Assets)
        {
            let label = gtk::Label::new(Some(&format!("  {}", resource_file_name(&resource.href))));
            label.set_halign(gtk::Align::Start);
            label.set_xalign(0.0);
            label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            label.set_tooltip_text(Some(&resource.href));
            label.add_css_class("kalam-editor-asset");
            list.append(&label);
        }
        if asset_count == 0 {
            append_empty_group_message(list, "No image or font assets.");
        }
    }

    append_resource_group(
        list,
        model,
        sender,
        engine::ResourceGroup::Contents,
        "CONTENTS",
    );
}

fn append_resource_group(
    list: &gtk::Box,
    model: &EpubEditorModel,
    sender: &ComponentSender<EpubEditorModel>,
    group: engine::ResourceGroup,
    title: &str,
) {
    let resources: Vec<_> = model
        .editor_resources
        .iter()
        .filter(|resource| resource.group == group)
        .collect();
    let heading = gtk::Label::new(Some(&format!("{title}   {}", resources.len())));
    heading.set_halign(gtk::Align::Start);
    heading.set_margin_start(12);
    heading.set_margin_top(13);
    heading.set_margin_bottom(4);
    heading.add_css_class("kalam-editor-tree-heading");
    list.append(&heading);

    if resources.is_empty() {
        append_empty_group_message(
            list,
            match group {
                engine::ResourceGroup::Text => "No XHTML or HTML text files.",
                engine::ResourceGroup::Styles => "No CSS stylesheets.",
                engine::ResourceGroup::Assets => "No assets.",
                engine::ResourceGroup::Contents => "No navigation or package files.",
            },
        );
        return;
    }

    for resource in resources {
        let mut title = resource_file_name(&resource.href).to_string();
        if !resource.editable {
            title.push_str("  ·  read-only");
        }
        let button = gtk::Button::with_label(&title);
        button.add_css_class("kalam-editor-resource");
        button.set_halign(gtk::Align::Fill);
        button.set_hexpand(true);
        button.set_tooltip_text(Some(&resource.href));
        if resource.href == model.selected_resource_href {
            button.add_css_class("kalam-editor-resource-selected");
        }
        if !resource.editable {
            button.add_css_class("kalam-editor-resource-readonly");
        }
        let href = resource.href.clone();
        let tx = sender.input_sender().clone();
        button.connect_clicked(move |_| {
            let _ = tx.send(EpubEditorMsg::SelectSourceResource(href.clone()));
        });
        list.append(&button);
    }
}

fn append_empty_group_message(list: &gtk::Box, message: &str) {
    let label = gtk::Label::new(Some(message));
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    label.set_margin_start(14);
    label.set_margin_top(3);
    label.add_css_class("kalam-editor-tree-empty");
    list.append(&label);
}

fn resource_file_name(href: &str) -> &str {
    href.rsplit('/').next().unwrap_or(href)
}

/// Rebuild the review panel inside its host. The panel is the step-5
/// one, unchanged; its Done collapses the revealer, and anything it
/// did (bake, accept, reject) is caught by the count at close.
fn refresh_patches_panel(
    model: &EpubEditorModel,
    host: &gtk::Box,
    sender: &ComponentSender<EpubEditorModel>,
) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    let tx = sender.input_sender().clone();
    let catalog = model.service.catalog().clone();
    let panel = crate::pages::edits_panel::build_edits_panel(catalog, model.book_id, move || {
        let _ = tx.send(EpubEditorMsg::SetPatchesOpen(false));
    });
    // The float's size, so the panel means the same thing here as it
    // does over the book page.
    panel.set_size_request(520, 540);
    host.append(&panel);
}

/// Build the deep-editor workspace once per page so the GtkSourceView undo
/// stack and native preview survive source-mode toggles.
fn build_source_pane(
    host: &gtk::Box,
    save_btn: &gtk::Button,
    history_button: &gtk::MenuButton,
    layout: SourceLayout,
    preview_as_shipped: bool,
    sender: &ComponentSender<EpubEditorModel>,
) -> SourcePane {
    let buffer = sourceview5::Buffer::new(None);
    if let Some(lang) = sourceview5::LanguageManager::new().language("xml") {
        buffer.set_language(Some(&lang));
    }
    let view = sourceview5::View::with_buffer(&buffer);
    view.set_editable(false);
    view.set_show_line_numbers(true);
    view.set_highlight_current_line(true);
    view.set_monospace(true);
    view.set_tab_width(4);
    view.set_wrap_mode(gtk::WrapMode::None);
    view.add_css_class("kalam-source-view");

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_hexpand(true);
    scroller.set_vexpand(true);
    scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    scroller.set_child(Some(&view));

    let editor_column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    editor_column.set_hexpand(true);
    editor_column.set_vexpand(true);
    editor_column.set_size_request(460, -1);
    editor_column.add_css_class("kalam-source-editor-column");

    // Source formatting helpers operate on selected XHTML/HTML text; CSS
    // remains plain CSS and disables these controls when it is active.
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 5);
    toolbar.add_css_class("kalam-editor-source-bar");
    let format_buttons = vec![
        format_button(&buffer, "B", "<strong>", "</strong>", "Bold / strong"),
        format_button(&buffer, "I", "<em>", "</em>", "Emphasis"),
        format_button(&buffer, "↗", "<a href=\"\">", "</a>", "Link"),
        format_button(&buffer, "H₂", "<h2>", "</h2>", "Heading level 2"),
        format_button(&buffer, "❝", "<blockquote>", "</blockquote>", "Block quote"),
    ];
    for button in &format_buttons {
        button.set_sensitive(false);
        toolbar.append(button);
    }
    let divider = gtk::Separator::new(gtk::Orientation::Vertical);
    divider.add_css_class("kalam-source-toolbar-divider");
    toolbar.append(&divider);

    let chapter_label = gtk::Label::new(Some("No file open"));
    chapter_label.set_halign(gtk::Align::Start);
    chapter_label.set_xalign(0.0);
    chapter_label.set_hexpand(true);
    chapter_label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    chapter_label.set_tooltip_text(Some("Current EPUB resource"));
    chapter_label.add_css_class("kalam-source-href");
    toolbar.append(&chapter_label);

    let find_button = gtk::Button::with_label("Find & replace");
    find_button.add_css_class("kalam-source-toolbar-button");
    find_button.set_tooltip_text(Some("Search or replace text in the open EPUB file"));
    toolbar.append(&find_button);

    let revert_btn = gtk::Button::with_label("Revert");
    revert_btn.add_css_class("kalam-source-toolbar-button");
    revert_btn.set_tooltip_text(Some("Reload the saved EPUB resource"));
    revert_btn.set_sensitive(false);
    {
        let tx = sender.input_sender().clone();
        revert_btn.connect_clicked(move |_| {
            let _ = tx.send(EpubEditorMsg::RevertSource);
        });
    }
    toolbar.append(&revert_btn);

    let find_entry = gtk::Entry::new();
    find_entry.set_hexpand(true);
    find_entry.set_placeholder_text(Some("Find"));
    find_entry.set_tooltip_text(Some("Text to find in the open resource"));
    let replace_entry = gtk::Entry::new();
    replace_entry.set_hexpand(true);
    replace_entry.set_placeholder_text(Some("Replace with"));
    replace_entry.set_tooltip_text(Some("Replacement text"));
    let find_next_button = gtk::Button::with_label("Find next");
    let replace_button = gtk::Button::with_label("Replace");
    let replace_all_button = gtk::Button::with_label("Replace all");
    let close_find_button = gtk::Button::with_label("×");
    close_find_button.set_tooltip_text(Some("Close Find & replace"));
    let find_revealer = gtk::Revealer::new();
    find_revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);
    find_revealer.set_reveal_child(false);
    find_revealer.add_css_class("kalam-source-find-revealer");
    let find_row = gtk::Box::new(gtk::Orientation::Horizontal, 7);
    find_row.add_css_class("kalam-source-find-row");
    find_row.append(&find_entry);
    find_row.append(&replace_entry);
    find_row.append(&find_next_button);
    find_row.append(&replace_button);
    find_row.append(&replace_all_button);
    find_row.append(&close_find_button);
    find_revealer.set_child(Some(&find_row));
    {
        let revealer = find_revealer.clone();
        find_button.connect_clicked(move |_| {
            revealer.set_reveal_child(!revealer.reveals_child());
        });
    }
    {
        let revealer = find_revealer.clone();
        close_find_button.connect_clicked(move |_| revealer.set_reveal_child(false));
    }
    {
        let buffer = buffer.clone();
        let view = view.clone();
        let entry = find_entry.clone();
        find_next_button.connect_clicked(move |_| {
            if !find_source_text(&buffer, &view, entry.text().as_str()) {
                entry.add_css_class("kalam-source-find-miss");
            } else {
                entry.remove_css_class("kalam-source-find-miss");
            }
        });
    }
    {
        let buffer = buffer.clone();
        let view = view.clone();
        let entry = find_entry.clone();
        find_entry.connect_activate(move |_| {
            let _ = find_source_text(&buffer, &view, entry.text().as_str());
        });
    }
    {
        let buffer = buffer.clone();
        let find = find_entry.clone();
        let replace = replace_entry.clone();
        replace_button.connect_clicked(move |_| {
            replace_selected_source_text(
                &buffer,
                find.text().as_str(),
                replace.text().as_str(),
            );
        });
    }
    {
        let buffer = buffer.clone();
        let find = find_entry.clone();
        let replace = replace_entry.clone();
        replace_all_button.connect_clicked(move |_| {
            replace_all_source_text(
                &buffer,
                find.text().as_str(),
                replace.text().as_str(),
            );
        });
    }

    let tabs_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    tabs_box.add_css_class("kalam-source-tabs");
    let tabs_scroll = gtk::ScrolledWindow::new();
    tabs_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    tabs_scroll.set_hexpand(true);
    tabs_scroll.set_child(Some(&tabs_box));
    editor_column.append(&toolbar);
    editor_column.append(&find_revealer);
    editor_column.append(&tabs_scroll);
    editor_column.append(&scroller);

    // The renderer is Kalam's own engine, never a browser/WebKit view.
    let picture = gtk::Picture::new();
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_hexpand(true);
    picture.set_vexpand(true);
    picture.add_css_class("kalam-source-preview-page");
    let preview_scroll = gtk::ScrolledWindow::new();
    preview_scroll.set_hexpand(true);
    preview_scroll.set_vexpand(true);
    preview_scroll.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    preview_scroll.set_child(Some(&picture));
    let status_label = gtk::Label::new(Some(
        "Native Kalam preview of the selected chapter with this resource applied where referenced.",
    ));
    status_label.set_wrap(true);
    status_label.set_halign(gtk::Align::Start);
    status_label.set_xalign(0.0);
    status_label.add_css_class("kalam-source-status");
    let preview_page = gtk::Box::new(gtk::Orientation::Vertical, 7);
    preview_page.set_hexpand(true);
    preview_page.set_vexpand(true);
    preview_page.add_css_class("kalam-source-preview-page-host");
    let preview_toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    preview_toolbar.add_css_class("kalam-source-preview-toolbar");
    let preview_mode_label = gtk::Label::new(Some("Appearance"));
    preview_mode_label.add_css_class("kalam-source-preview-mode-label");
    let preview_mode = gtk::DropDown::from_strings(&["As the book ships", "As in Kalam"]);
    preview_mode.set_selected(if preview_as_shipped { 0 } else { 1 });
    preview_mode.set_tooltip_text(Some(
        "Compare the publisher's stylesheet settings with Kalam's saved reader settings",
    ));
    {
        let tx = sender.input_sender().clone();
        preview_mode.connect_selected_notify(move |dropdown| {
            let _ = tx.send(EpubEditorMsg::SetPreviewAsShipped(dropdown.selected() == 0));
        });
    }
    preview_toolbar.append(&preview_mode_label);
    preview_toolbar.append(&preview_mode);
    preview_page.append(&preview_toolbar);
    preview_page.append(&status_label);
    preview_page.append(&preview_scroll);

    let preview_box = gtk::Notebook::new();
    preview_box.set_hexpand(true);
    preview_box.set_vexpand(true);
    preview_box.set_size_request(360, -1);
    preview_box.set_scrollable(true);
    preview_box.set_show_border(false);
    preview_box.add_css_class("kalam-source-panels");
    preview_box.append_page(&preview_page, Some(&gtk::Label::new(Some("Preview"))));

    let outline_list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    outline_list.set_margin_top(12);
    outline_list.set_margin_bottom(12);
    outline_list.set_margin_start(12);
    outline_list.set_margin_end(12);
    let outline_scroll = gtk::ScrolledWindow::new();
    outline_scroll.set_hexpand(true);
    outline_scroll.set_vexpand(true);
    outline_scroll.set_child(Some(&outline_list));
    preview_box.append_page(&outline_scroll, Some(&gtk::Label::new(Some("Outline"))));

    let problems_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    problems_page.set_margin_top(18);
    problems_page.set_margin_start(18);
    problems_page.set_margin_end(18);
    let problems_label = gtk::Label::new(Some(
        "Save validates the resource before replacing anything in the EPUB. If validation or a stale-file check fails, your current buffer stays open.",
    ));
    problems_label.set_wrap(true);
    problems_label.set_halign(gtk::Align::Start);
    problems_label.set_xalign(0.0);
    problems_label.add_css_class("kalam-source-panel-copy");
    problems_page.append(&problems_label);
    preview_box.append_page(&problems_page, Some(&gtk::Label::new(Some("Problems"))));

    let live_css_page = gtk::Box::new(gtk::Orientation::Vertical, 8);
    live_css_page.set_margin_top(18);
    live_css_page.set_margin_start(18);
    live_css_page.set_margin_end(18);
    let live_css_label = gtk::Label::new(Some(
        "Edit a stylesheet from the Styles group. The native preview applies the selected CSS without writing it until you Save.",
    ));
    live_css_label.set_wrap(true);
    live_css_label.set_halign(gtk::Align::Start);
    live_css_label.set_xalign(0.0);
    live_css_label.add_css_class("kalam-source-panel-copy");
    live_css_page.append(&live_css_label);
    preview_box.append_page(&live_css_page, Some(&gtk::Label::new(Some("Live CSS"))));

    // A status/footer row keeps cursor position, encoding and view controls
    // visible without taking editor space.
    let cursor_label = gtk::Label::new(Some("Ln 1, Col 1"));
    cursor_label.set_halign(gtk::Align::Start);
    cursor_label.add_css_class("kalam-source-cursor-status");
    let syntax_label = gtk::Label::new(Some("—"));
    syntax_label.add_css_class("kalam-source-encoding");
    let encoding_label = gtk::Label::new(Some("UTF-8"));
    encoding_label.add_css_class("kalam-source-encoding");
    let validation_label = gtk::Label::new(Some("Ready"));
    validation_label.add_css_class("kalam-source-encoding");
    validation_label.set_tooltip_text(Some("Structural validation runs before Save"));
    let footer_status = gtk::Label::new(Some("Choose a file from the EPUB rail"));
    footer_status.set_halign(gtk::Align::Start);
    footer_status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    footer_status.set_hexpand(true);
    footer_status.add_css_class("kalam-source-footer-status");

    let code_btn = gtk::Button::with_label("Code");
    let split_btn = gtk::Button::with_label("Split");
    let preview_btn = gtk::Button::with_label("Preview");
    for button in [&code_btn, &split_btn, &preview_btn] {
        button.add_css_class("kalam-source-layout-button");
    }
    {
        let tx = sender.input_sender().clone();
        code_btn.connect_clicked(move |_| {
            let _ = tx.send(EpubEditorMsg::SetSourceLayout(SourceLayout::Code));
        });
    }
    {
        let tx = sender.input_sender().clone();
        split_btn.connect_clicked(move |_| {
            let _ = tx.send(EpubEditorMsg::SetSourceLayout(SourceLayout::Split));
        });
    }
    {
        let tx = sender.input_sender().clone();
        preview_btn.connect_clicked(move |_| {
            let _ = tx.send(EpubEditorMsg::SetSourceLayout(SourceLayout::Preview));
        });
    }
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 9);
    footer.add_css_class("kalam-source-footer");
    footer.append(&cursor_label);
    footer.append(&syntax_label);
    footer.append(&encoding_label);
    footer.append(&validation_label);
    footer.append(&footer_status);
    footer.append(&code_btn);
    footer.append(&split_btn);
    footer.append(&preview_btn);

    let stale_label = gtk::Label::new(Some(
        "This EPUB file changed after it loaded. Revert to read the current version, then reapply your edit.",
    ));
    stale_label.set_wrap(true);
    stale_label.set_halign(gtk::Align::Start);
    stale_label.set_xalign(0.0);
    stale_label.add_css_class("kalam-source-stale");
    stale_label.set_visible(false);

    let workspace = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    workspace.set_hexpand(true);
    workspace.set_vexpand(true);
    workspace.add_css_class("kalam-source-workspace");
    workspace.append(&editor_column);
    workspace.append(&preview_box);

    host.append(&stale_label);
    host.append(&workspace);
    host.append(&footer);

    let history_popover = gtk::Popover::new();
    history_popover.add_css_class("kalam-source-history-popover");
    let history_body = gtk::Box::new(gtk::Orientation::Vertical, 8);
    history_body.set_margin_top(12);
    history_body.set_margin_bottom(12);
    history_body.set_margin_start(12);
    history_body.set_margin_end(12);
    let history_title = gtk::Label::new(Some("Saved versions"));
    history_title.set_halign(gtk::Align::Start);
    history_title.add_css_class("kalam-source-history-title");
    let history_note = gtk::Label::new(Some(
        "Save a file to keep a restore point here. Each save also creates a timestamped EPUB backup beside the book.",
    ));
    history_note.set_wrap(true);
    history_note.set_halign(gtk::Align::Start);
    history_note.set_xalign(0.0);
    history_note.add_css_class("kalam-source-panel-copy");
    let history_restore_btn = gtk::Button::with_label("Restore previous saved text");
    history_restore_btn.set_sensitive(false);
    history_body.append(&history_title);
    history_body.append(&history_note);
    history_body.append(&history_restore_btn);
    history_popover.set_child(Some(&history_body));
    history_button.set_popover(Some(&history_popover));
    history_button.set_sensitive(false);
    {
        let tx = sender.input_sender().clone();
        let popover = history_popover.clone();
        history_restore_btn.connect_clicked(move |_| {
            let _ = tx.send(EpubEditorMsg::RestorePreviousSavedVersion);
            popover.popdown();
        });
    }

    let suppress = Rc::new(Cell::new(false));
    {
        let sup = suppress.clone();
        let tx = sender.input_sender().clone();
        buffer.connect_changed(move |_| {
            if !sup.get() {
                let _ = tx.send(EpubEditorMsg::SourceEdited);
            }
        });
    }
    {
        let cursor_label = cursor_label.clone();
        buffer.connect_cursor_position_notify(move |buffer| {
            update_cursor_status(buffer, &cursor_label);
        });
    }

    let pane = SourcePane {
        buffer,
        source_view: view,
        preview_box,
        editor_column,
        picture,
        status_label,
        stale_label,
        chapter_label,
        save_btn: save_btn.clone(),
        revert_btn,
        tabs_box,
        cursor_label,
        footer_status,
        syntax_label,
        validation_label,
        problems_label,
        outline_list,
        code_btn,
        split_btn,
        preview_btn,
        format_buttons,
        history_button: history_button.clone(),
        history_restore_btn,
        history_note,
        history: RefCell::new(None),
        editable: Cell::new(false),
        suppress,
        loading: Cell::new(false),
        saving: Cell::new(false),
        dirty: Cell::new(false),
        stale: Cell::new(false),
        preview_gen: Cell::new(0),
        href: RefCell::new(String::new()),
        baseline_hash: RefCell::new(String::new()),
        loaded_text: RefCell::new(String::new()),
        preview_chapter: Cell::new(0),
        save_attempt: RefCell::new(None),
    };
    apply_source_layout(&pane, layout);
    update_cursor_status(&pane.buffer, &pane.cursor_label);
    pane
}

fn format_button(
    buffer: &sourceview5::Buffer,
    title: &str,
    opening: &'static str,
    closing: &'static str,
    tooltip: &str,
) -> gtk::Button {
    let button = gtk::Button::with_label(title);
    button.add_css_class("kalam-source-format-button");
    button.set_tooltip_text(Some(tooltip));
    let buffer = buffer.clone();
    button.connect_clicked(move |_| wrap_source_selection(&buffer, opening, closing));
    button
}

fn wrap_source_selection(buffer: &sourceview5::Buffer, opening: &str, closing: &str) {
    let Some((mut start, mut end)) = buffer.selection_bounds() else {
        return;
    };
    let selected = buffer.text(&start, &end, false).to_string();
    buffer.begin_user_action();
    buffer.delete(&mut start, &mut end);
    buffer.insert(&mut start, &format!("{opening}{selected}{closing}"));
    buffer.end_user_action();
}

fn find_source_text(buffer: &sourceview5::Buffer, view: &sourceview5::View, needle: &str) -> bool {
    if needle.is_empty() {
        return false;
    }
    let flags = gtk::TextSearchFlags::TEXT_ONLY;
    let cursor = buffer.iter_at_offset(buffer.cursor_position());
    let found = cursor
        .forward_search(needle, flags, None)
        .or_else(|| buffer.start_iter().forward_search(needle, flags, None));
    let Some((start, end)) = found else {
        return false;
    };
    buffer.select_range(&end, &start);
    let mut scroll_to = start;
    view.scroll_to_iter(&mut scroll_to, 0.15, false, 0.0, 0.0);
    true
}

fn replace_selected_source_text(buffer: &sourceview5::Buffer, needle: &str, replacement: &str) {
    if needle.is_empty() {
        return;
    }
    let Some((mut start, mut end)) = buffer.selection_bounds() else {
        return;
    };
    if buffer.text(&start, &end, false).as_str() != needle {
        return;
    }
    buffer.begin_user_action();
    buffer.delete(&mut start, &mut end);
    buffer.insert(&mut start, replacement);
    buffer.end_user_action();
}

fn replace_all_source_text(buffer: &sourceview5::Buffer, needle: &str, replacement: &str) {
    if needle.is_empty() {
        return;
    }
    let mut scan_offset = 0;
    let mut replaced = 0;
    buffer.begin_user_action();
    loop {
        let search_from = buffer.iter_at_offset(scan_offset);
        let Some((mut start, mut end)) =
            search_from.forward_search(needle, gtk::TextSearchFlags::TEXT_ONLY, None)
        else {
            break;
        };
        let match_offset = start.offset();
        buffer.delete(&mut start, &mut end);
        buffer.insert(&mut start, replacement);
        scan_offset = match_offset + replacement.chars().count() as i32;
        replaced += 1;
        if scan_offset > buffer.char_count() {
            break;
        }
    }
    buffer.end_user_action();
    if replaced > 0 {
        log::debug!("replaced {replaced} source occurrences");
    }
}

fn apply_source_layout(pane: &SourcePane, layout: SourceLayout) {
    pane.editor_column.set_visible(layout != SourceLayout::Preview);
    pane.preview_box.set_visible(layout != SourceLayout::Code);
    set_layout_button_active(&pane.code_btn, layout == SourceLayout::Code);
    set_layout_button_active(&pane.split_btn, layout == SourceLayout::Split);
    set_layout_button_active(&pane.preview_btn, layout == SourceLayout::Preview);
}

fn set_layout_button_active(button: &gtk::Button, active: bool) {
    if active {
        button.add_css_class("kalam-source-layout-active");
    } else {
        button.remove_css_class("kalam-source-layout-active");
    }
}

fn resource_syntax(href: &str) -> &'static str {
    let lower = href.to_ascii_lowercase();
    if lower.ends_with(".css") {
        "CSS"
    } else if lower.ends_with(".html") || lower.ends_with(".htm") {
        "HTML"
    } else if lower.ends_with(".xhtml") {
        "XHTML"
    } else {
        "XML"
    }
}

fn update_cursor_status(buffer: &sourceview5::Buffer, label: &gtk::Label) {
    let iter = buffer.iter_at_offset(buffer.cursor_position());
    label.set_text(&format!("Ln {}, Col {}", iter.line() + 1, iter.line_offset() + 1));
}

fn rebuild_source_tabs(
    tabs: &gtk::Box,
    model: &EpubEditorModel,
    pane: &SourcePane,
    current_dirty: Option<bool>,
    sender: &ComponentSender<EpubEditorModel>,
) {
    while let Some(child) = tabs.first_child() {
        tabs.remove(&child);
    }
    for href in &model.open_source_tabs {
        let active = href == &model.selected_resource_href;
        let dirty = active
            && current_dirty.unwrap_or_else(|| source_buffer_is_dirty(pane));
        let mut title = resource_file_name(href).to_string();
        if dirty {
            title.insert_str(0, "● ");
        }
        let button = gtk::Button::with_label(&title);
        button.add_css_class("kalam-source-file-tab");
        if active {
            button.add_css_class("kalam-source-file-tab-active");
        }
        if dirty {
            button.add_css_class("kalam-source-file-tab-dirty");
        }
        button.set_tooltip_text(Some(href));
        let select_href = href.clone();
        let tx = sender.input_sender().clone();
        button.connect_clicked(move |_| {
            let _ = tx.send(EpubEditorMsg::SelectSourceResource(select_href.clone()));
        });
        let close = gtk::Button::with_label("×");
        close.add_css_class("kalam-source-file-tab-close");
        close.set_tooltip_text(Some("Close file tab"));
        let close_href = href.clone();
        let tx = sender.input_sender().clone();
        close.connect_clicked(move |_| {
            let _ = tx.send(EpubEditorMsg::CloseSourceTab(close_href.clone()));
        });
        let tab = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        tab.add_css_class("kalam-source-file-tab-row");
        tab.append(&button);
        tab.append(&close);
        tabs.append(&tab);
    }
}

fn rebuild_outline(list: &gtk::Box, text: &str) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    let mut headings = Vec::new();
    for line in text.lines() {
        let lower = line.to_ascii_lowercase();
        let Some(start) = lower.find("<h") else { continue };
        let Some(level) = lower[start + 2..]
            .chars()
            .next()
            .and_then(|digit| digit.to_digit(10))
            .filter(|level| (1..=6).contains(level))
        else {
            continue;
        };
        let Some(open_end) = line[start..].find('>') else { continue };
        let content_start = start + open_end + 1;
        let closing = format!("</h{level}");
        let content_end = lower[content_start..]
            .find(&closing)
            .map(|offset| content_start + offset)
            .unwrap_or(line.len());
        let heading = strip_markup(&line[content_start..content_end]);
        if !heading.trim().is_empty() {
            headings.push((level, heading.trim().to_string()));
        }
    }
    if headings.is_empty() {
        append_empty_group_message(list, "No headings in the current resource.");
        return;
    }
    for (level, title) in headings {
        let label = gtk::Label::new(Some(&format!("{}{}", "  ".repeat(level as usize - 1), title)));
        label.set_halign(gtk::Align::Start);
        label.set_xalign(0.0);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.add_css_class("kalam-source-outline-item");
        list.append(&label);
    }
}

fn strip_markup(text: &str) -> String {
    let mut output = String::new();
    let mut inside_tag = false;
    for ch in text.chars() {
        match ch {
            '<' => inside_tag = true,
            '>' => inside_tag = false,
            _ if !inside_tag => output.push(ch),
            _ => {}
        }
    }
    output
}

fn update_history_controls(pane: &SourcePane) {
    let current_href = pane.href.borrow().clone();
    let available = pane.editable.get()
        && pane
            .history
            .borrow()
            .as_ref()
            .is_some_and(|(history_href, _)| history_href == &current_href);
    pane.history_button.set_sensitive(available);
    pane.history_restore_btn.set_sensitive(available);
    if available {
        pane.history_note.set_text(
            "Restore the previous saved source text. The timestamped EPUB backup is also kept beside the book.",
        );
    } else {
        pane.history_note.set_text(
            "No in-app restore point is available for this file. Timestamped EPUB backups are kept beside the book after saves.",
        );
    }
}

fn build_tools_popover(
    model: &EpubEditorModel,
    sender: &ComponentSender<EpubEditorModel>,
) -> gtk::Popover {
    let popover = gtk::Popover::new();
    let body = gtk::Box::new(gtk::Orientation::Vertical, 5);
    body.set_margin_top(8);
    body.set_margin_bottom(8);
    body.set_margin_start(8);
    body.set_margin_end(8);
    let patches = gtk::Button::with_label(&format!(
        "Review pending patches · {}",
        model.patches_count
    ));
    patches.set_halign(gtk::Align::Fill);
    let tx = sender.input_sender().clone();
    let pop = popover.clone();
    patches.connect_clicked(move |_| {
        let _ = tx.send(EpubEditorMsg::SetPatchesOpen(true));
        pop.popdown();
    });
    let cover = gtk::Button::with_label("Replace cover");
    cover.set_tooltip_text(Some(
        "Cover writes are disabled until the direct-save safety path supports them",
    ));
    cover.set_sensitive(false);
    body.append(&patches);
    body.append(&cover);
    popover.set_child(Some(&body));
    popover
}

/// The buffer's whole text. `TextBuffer::text` wants bounds, and a
/// raw pane is always whole-buffer.
fn source_text(buffer: &sourceview5::Buffer) -> String {
    let (start, end) = buffer.bounds();
    buffer.text(&start, &end, false).to_string()
}

/// Read live buffer contents as well as the asynchronous dirty bit. A key
/// press can be followed by a file switch before the changed-message callback
/// has been handled; this guard must still protect those just-typed bytes.
fn source_buffer_is_dirty(pane: &SourcePane) -> bool {
    pane.dirty.get() || source_text(&pane.buffer) != *pane.loaded_text.borrow()
}

/// The preview's page size from the reading prefs: the reader's own
/// column width plus its margins, and a portrait page. Same prefs as
/// the engine, so the preview is what the book will read like.
fn preview_page_size(prefs: &kalam_reader::KalamPrefs) -> (u32, u32) {
    let width = (prefs.column_px + 2.0 * kalam_reader::preview::PREVIEW_MARGIN)
        .round()
        .max(320.0) as u32;
    let height = (width as f32 * 1.4).round() as u32;
    (width, height)
}
