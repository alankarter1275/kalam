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
    /// The source pane's right-hand native preview.
    SetPreviewOpen(bool),
    /// The background ZIP scan found the editor's supported text files.
    SourceResourcesLoaded(Result<Vec<engine::EditableResource>, String>),
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
    /// Whether the source pane's right-hand preview is on. Kept
    /// separate from `source_open` so reopening the pane restores it.
    preview_open: bool,
    source: Option<SourcePane>,
    /// Display order for the chapter list. Existing legacy spine patches
    /// remain visible in their proposed order; new chapter-order edits are
    /// deferred until they can write directly to the EPUB.
    chapter_order: Vec<usize>,
    editable_resources: Vec<engine::EditableResource>,
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
    /// The preview's pages, stacked into one tall texture.
    picture: gtk::Picture,
    /// The preview column: hidden until the Preview toggle says, and
    /// only meaningful while the source pane is open.
    preview_box: gtk::Box,
    status_label: gtk::Label,
    /// The stale banner: the selected file changed under the pane; saves are
    /// refused until it is reverted.
    stale_label: gtk::Label,
    chapter_label: gtk::Label,
    save_btn: gtk::Button,
    revert_btn: gtk::Button,
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

                // The chrome: back, the book, and the review panel's
                // door with its count. Persistent, not auto-hiding —
                // this is a work surface, not a reading surface.
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

                    #[name = "title_label"]
                    gtk::Label {
                        add_css_class: "kalam-editor-title",
                        add_css_class: "kalam-title-serif",
                        set_label: &model.book_title,
                        set_halign: gtk::Align::Start,
                        set_hexpand: true,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_single_line_mode: true,
                        set_margin_start: 8,
                    },

                    #[name = "patches_btn"]
                    gtk::ToggleButton {
                        add_css_class: "kalam-editor-patches-btn",
                        set_tooltip_text: Some("This book's pending edits and the bake"),
                        #[watch]
                        set_label: &if model.patches_count == 0 {
                            "Patches".to_string()
                        } else {
                            format!("Patches · {}", model.patches_count)
                        },
                        connect_toggled[sender] => move |btn| {
                            sender.input(EpubEditorMsg::SetPatchesOpen(btn.is_active()));
                        },
                    },

                    // The deep-editor source mode: browse EPUB text
                    // resources and edit one file at a time.
                    #[name = "source_btn"]
                    gtk::ToggleButton {
                        add_css_class: "kalam-editor-patches-btn",
                        set_label: "Source",
                        set_tooltip_text: Some("Browse and edit EPUB XHTML, HTML, and CSS files"),
                        #[watch]
                        set_sensitive: model.view.is_some(),
                        connect_toggled[sender] => move |btn| {
                            sender.input(EpubEditorMsg::SetSourceOpen(btn.is_active()));
                        },
                    },

                    #[name = "preview_btn"]
                    gtk::ToggleButton {
                        add_css_class: "kalam-editor-patches-btn",
                        set_label: "Preview",
                        set_tooltip_text: Some("Preview the current chapter with the selected resource where used"),
                        #[watch]
                        set_visible: model.source_open,
                        connect_toggled[sender] => move |btn| {
                            sender.input(EpubEditorMsg::SetPreviewOpen(btn.is_active()));
                        },
                    },

                    // Cover replacement is temporarily unavailable until
                    // it has a direct-write path rather than staging a patch.
                    #[name = "cover_btn"]
                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic(
                            "image-x-generic-symbolic",
                            16,
                        )),
                        add_css_class: "kalam-icon-btn",
                        set_tooltip_text: Some("Direct cover saving will follow the core editor update"),
                        set_sensitive: false,
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
                        set_width_request: 220,
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
                                    set_label: "EPUB files",
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
            preview_open: false,
            source: None,
            chapter_order: Vec::new(),
            editable_resources: Vec::new(),
            selected_resource_href: String::new(),
            resource_list_loaded: false,
            resource_list_error: None,
        };
        // A spine op left pending from an earlier session still owns
        // the list's order; the identity otherwise.
        model.sync_chapter_order();

        let widgets = view_output!();
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
            let pane = build_source_pane(&widgets.source_host, &sender);
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
        if let Some(path) = model.book_path.clone() {
            let (tx, rx) = async_channel::unbounded::<
                Result<Vec<engine::EditableResource>, String>,
            >();
            let itx = sender.input_sender().clone();
            gtk::glib::spawn_future_local(async move {
                if let Ok(verdict) = rx.recv().await {
                    let _ = itx.send(EpubEditorMsg::SourceResourcesLoaded(verdict));
                }
            });
            engine::load_editable_resources(path, tx);
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
        _root: &Self::Root,
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
                    self.load_source(&sender);
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
                                .find(|resource| resource.spine_index == Some(self.chapter))
                                .or_else(|| resources.first())
                                .map(|resource| resource.href.clone())
                                .unwrap_or_default();
                        }
                        self.editable_resources = resources;
                    }
                    Err(message) => {
                        self.resource_list_error = Some(message.clone());
                        self.editable_resources.clear();
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
                    || !self.editable_resources.iter().any(|resource| resource.href == href)
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
                    .editable_resources
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
            EpubEditorMsg::SetPreviewOpen(open) => {
                if open == self.preview_open {
                    return;
                }
                self.preview_open = open;
                if let Some(pane) = &self.source {
                    let gen = pane.preview_gen.get() + 1;
                    pane.preview_gen.set(gen);
                    pane.preview_box.set_visible(open);
                    if open && self.source_open && !pane.loading.get() {
                        self.kick_preview(&sender);
                    }
                }
            }
            EpubEditorMsg::SourceEdited => {
                let Some(pane) = &self.source else { return };
                if pane.suppress.get() || pane.loading.get() {
                    return;
                }
                let dirty = source_text(&pane.buffer) != *pane.loaded_text.borrow();
                pane.dirty.set(dirty);
                pane.save_btn.set_sensitive(dirty && !pane.stale.get());
                pane.revert_btn.set_sensitive(dirty || pane.stale.get());
                let gen = pane.preview_gen.get() + 1;
                pane.preview_gen.set(gen);
                if self.preview_open {
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
                        pane.source_view.set_editable(true);
                        pane.suppress.set(true);
                        pane.buffer.set_text(&entry.text);
                        pane.suppress.set(false);
                        *pane.href.borrow_mut() = entry.href.clone();
                        *pane.baseline_hash.borrow_mut() = entry.hash.clone();
                        *pane.loaded_text.borrow_mut() = entry.text.clone();
                        pane.preview_chapter.set(entry.preview_chapter);
                        pane.dirty.set(false);
                        pane.stale.set(false);
                        pane.stale_label.set_visible(false);
                        pane.save_btn.set_sensitive(false);
                        pane.revert_btn.set_sensitive(false);
                        pane.chapter_label.set_text(&entry.href);
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
                        if self.preview_open {
                            self.kick_preview(&sender);
                        }
                    }
                    Err(toast) => {
                        // A bad resource must not close the editor or replace
                        // the previous buffer. Restore its tree selection when
                        // one was already loaded; otherwise keep the error in
                        // the open pane so the user can choose another file.
                        crate::notify::error("Could not open the EPUB file", &toast);
                        let previous_href = pane.href.borrow().clone();
                        pane.source_view.set_editable(!previous_href.is_empty());
                        if !previous_href.is_empty() {
                            self.selected_resource_href = previous_href;
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
                let text = source_text(&pane.buffer);
                if text == *pane.loaded_text.borrow() {
                    crate::notify::info("Nothing to fix", "The source already says that.");
                    return;
                }
                let Some(path) = self.book_path.clone() else { return };
                pane.saving.set(true);
                pane.save_btn.set_sensitive(false);
                pane.revert_btn.set_sensitive(false);
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
                        // The worker saved this snapshot of the buffer.
                        // Any typing that raced the write stays dirty.
                        *pane.loaded_text.borrow_mut() = text;
                        *pane.baseline_hash.borrow_mut() = entry_hash;
                        self.book_hash = book_hash;
                        let still_dirty = source_text(&pane.buffer) != *pane.loaded_text.borrow();
                        pane.dirty.set(still_dirty);
                        pane.save_btn.set_sensitive(still_dirty);
                        pane.revert_btn.set_sensitive(still_dirty);
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
                        pane.stale_label.set_visible(true);
                        crate::notify::info(
                            "The EPUB file changed",
                            "This EPUB file changed after the pane loaded it. Revert to load the current text, then make the edit again.",
                        );
                    }
                    Err(message) => {
                        *pane.save_attempt.borrow_mut() = None;
                        let dirty = source_buffer_is_dirty(pane);
                        pane.save_btn.set_sensitive(dirty && !pane.stale.get());
                        pane.revert_btn.set_sensitive(dirty || pane.stale.get());
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
            .editable_resources
            .iter()
            .find(|resource| resource.href == self.selected_resource_href)
        else {
            if let Some(error) = self.resource_list_error.as_deref() {
                pane.status_label.set_text(error);
            } else if !self.resource_list_loaded {
                pane.status_label.set_text("Finding editable XHTML, HTML, and CSS files…");
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
        if pane.loading.get() || !self.preview_open || !self.source_open {
            return;
        }
        let Some(path) = self.book_path.clone() else { return };
        let prefs = engine::current_engine_prefs(self.service.catalog());
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

/// Rebuild the source-mode file list. Directory labels make the flat,
/// searchable-by-eye list read like an EPUB tree without hiding files behind
/// a nested expander. Only editable XHTML/HTML/CSS entries are listed.
fn rebuild_source_resource_list(
    list: &gtk::Box,
    model: &EpubEditorModel,
    sender: &ComponentSender<EpubEditorModel>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    if model.editable_resources.is_empty() {
        let message = if let Some(error) = &model.resource_list_error {
            error.as_str()
        } else if model.resource_list_loaded {
            "This EPUB has no editable XHTML, HTML, or CSS files."
        } else {
            "Finding editable EPUB files…"
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

    let mut last_directory: Option<String> = None;
    for resource in &model.editable_resources {
        let (directory, file_name) = resource
            .href
            .rsplit_once('/')
            .unwrap_or(("", resource.href.as_str()));
        if last_directory.as_deref() != Some(directory) {
            let label = gtk::Label::new(Some(if directory.is_empty() {
                "Book root"
            } else {
                directory
            }));
            label.set_halign(gtk::Align::Start);
            label.set_xalign(0.0);
            label.set_margin_start(12);
            label.set_margin_top(10);
            label.set_margin_bottom(2);
            label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
            label.set_tooltip_text(Some(directory));
            label.add_css_class("kalam-editor-tree-directory");
            list.append(&label);
            last_directory = Some(directory.to_string());
        }

        let button = gtk::Button::with_label(file_name);
        button.add_css_class("kalam-editor-resource");
        button.set_halign(gtk::Align::Fill);
        button.set_hexpand(true);
        button.set_tooltip_text(Some(&resource.href));
        if resource.href == model.selected_resource_href {
            button.add_css_class("kalam-editor-resource-selected");
        }
        let href = resource.href.clone();
        let tx = sender.input_sender().clone();
        button.connect_clicked(move |_| {
            let _ = tx.send(EpubEditorMsg::SelectSourceResource(href.clone()));
        });
        list.append(&button);
    }
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

/// Build the raw source pane into its host, once per page. Everything
/// is imperative: `sourceview5::View` is a subclass of `TextView`,
/// but the relm4 macro knows the gtk types by name, so the pane is
/// assembled the way the reader's overlay is — by hand, in init.
///
/// The pane starts empty and hidden; `load_source` fills it when the
/// Source toggle first opens it.
fn build_source_pane(host: &gtk::Box, sender: &ComponentSender<EpubEditorModel>) -> SourcePane {
    // The editor: GtkSourceView's buffer and view, XML by default (a
    // chapter is XHTML; CSS re-targets the language at load).
    let buffer = sourceview5::Buffer::new(None);
    if let Some(lang) = sourceview5::LanguageManager::new().language("xml") {
        buffer.set_language(Some(&lang));
    }
    let view = sourceview5::View::with_buffer(&buffer);
    view.set_editable(false);
    view.set_show_line_numbers(true);
    view.set_monospace(true);
    view.set_tab_width(4);
    view.set_wrap_mode(gtk::WrapMode::Word);
    view.add_css_class("kalam-source-view");
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_hexpand(true);
    scroller.set_vexpand(true);
    scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Automatic);
    scroller.set_child(Some(&view));

    // The right-hand native preview: a tall picture of the current
    // reading-order chapter, scrolled like paper.
    let picture = gtk::Picture::new();
    picture.set_can_shrink(true);
    picture.set_hexpand(true);
    picture.set_vexpand(true);
    picture.add_css_class("kalam-source-preview-page");
    let preview_scroll = gtk::ScrolledWindow::new();
    preview_scroll.set_hexpand(true);
    preview_scroll.set_vexpand(true);
    preview_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    preview_scroll.set_child(Some(&picture));
    let status_label = gtk::Label::new(Some(
        "Native preview of the current chapter; the selected text file is applied where referenced.",
    ));
    status_label.set_wrap(true);
    status_label.set_halign(gtk::Align::Start);
    status_label.set_xalign(0.0);
    status_label.add_css_class("kalam-source-status");
    let preview_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    preview_box.add_css_class("kalam-source-preview");
    preview_box.set_margin_top(8);
    preview_box.set_margin_bottom(8);
    preview_box.set_margin_start(8);
    preview_box.set_margin_end(8);
    preview_box.set_size_request(320, -1);
    preview_box.append(&status_label);
    preview_box.append(&preview_scroll);
    preview_box.set_visible(false);

    let content = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    content.set_hexpand(true);
    content.set_vexpand(true);
    content.append(&scroller);
    content.append(&preview_box);

    // The toolbar: Save, Revert, and which entry is open.
    let save_btn = gtk::Button::with_label("Save");
    save_btn.add_css_class("suggested-action");
    save_btn.set_tooltip_text(Some("Store this source as a whole-file edit"));
    save_btn.set_sensitive(false);
    {
        let tx = sender.input_sender().clone();
        save_btn.connect_clicked(move |_| {
            let _ = tx.send(EpubEditorMsg::SaveSource);
        });
    }
    let revert_btn = gtk::Button::with_label("Revert");
    revert_btn.set_tooltip_text(Some("Throw the edits away and reload this EPUB file"));
    revert_btn.set_sensitive(false);
    {
        let tx = sender.input_sender().clone();
        revert_btn.connect_clicked(move |_| {
            let _ = tx.send(EpubEditorMsg::RevertSource);
        });
    }
    let chapter_label = gtk::Label::new(None);
    chapter_label.set_halign(gtk::Align::Start);
    chapter_label.set_xalign(0.0);
    chapter_label.set_hexpand(true);
    chapter_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    chapter_label.add_css_class("kalam-source-href");
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    toolbar.add_css_class("kalam-editor-source-bar");
    toolbar.append(&save_btn);
    toolbar.append(&revert_btn);
    toolbar.append(&chapter_label);

    // The stale banner: saves are refused until the pane is reverted.
    let stale_label = gtk::Label::new(Some(
        "This EPUB file changed after the pane loaded it. Revert to load the current text, then make the edit again.",
    ));
    stale_label.set_wrap(true);
    stale_label.set_halign(gtk::Align::Start);
    stale_label.set_xalign(0.0);
    stale_label.add_css_class("kalam-source-stale");
    stale_label.set_visible(false);

    host.append(&toolbar);
    host.append(&stale_label);
    host.append(&content);

    // The buffer's own loads are suppressed; a hand edit is the only
    // thing that reads as one.
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

    SourcePane {
        buffer,
        source_view: view,
        picture,
        preview_box,
        status_label,
        stale_label,
        chapter_label,
        save_btn,
        revert_btn,
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
    }
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
