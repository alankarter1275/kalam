//! The full EPUB editor (Phase 6 step 8): a route of its own, with a
//! way back, where a book's chapters are read in the reader's own
//! renderer and every paragraph is editable — the proofread mode
//! without a toggle, a chapter list instead of a TOC, and the review
//! panel in the chrome so a session of edits ends in a bake without
//! leaving the page.
//!
//! The page owns its own engine on the book's file (`open_engine`,
//! like the reader — never the reader's session), and its paragraph
//! editing is the reader's machinery, not a sibling of it:
//! `build_paragraph_editor` and `verify_paragraph_edit` are the same
//! functions, and `InlineEdit` is the same struct. Edits store with
//! the source `Editor`, so the review panel can tell a proofread fix
//! from an editor session.

use std::path::PathBuf;
use std::sync::Arc;

use gtk::prelude::*;
use relm4::prelude::*;

use kalam_reader::{ParagraphIdentity, ReaderView};

use crate::db::Catalog;
use crate::service::LibraryService;
use crate::pages::reader::{
    engine, toc_title, EditScope, EditorWidget, InlineEdit, VerifiedEdit,
};

/// The editor's messages. A slice of the reader's — paragraph taps,
/// selection reports (the click-away commit), the commit/verdict pair
/// — plus the chrome: chapter jumps, the review panel, and the way
/// back.
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
    /// The background verification finished: the patch to store, or
    /// the refusal's toast plus the editor's serial.
    InlineEditVerified(Result<VerifiedEdit, (String, u64)>),
    /// A chapter was chosen in the list.
    TocSelect(usize),
    /// The review panel's open state — a plain desired state, not a
    /// toggle, so the button's own toggled signal and the panel's Done
    /// agree without a feedback loop.
    SetPatchesOpen(bool),
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
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_hexpand: true,
                    set_vexpand: true,

                    // The chapter list — flat, not the reader's TOC
                    // tree: editing moves chapter by chapter, and a
                    // flat list is one glance for that.
                    #[name = "chapter_scroller"]
                    gtk::ScrolledWindow {
                        add_css_class: "kalam-editor-chapters",
                        set_width_request: 200,
                        set_hscrollbar_policy: gtk::PolicyType::Never,
                        set_vscrollbar_policy: gtk::PolicyType::Automatic,

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_margin_top: 8,
                            set_margin_bottom: 8,

                            #[name = "chapter_list"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_hexpand: true,
                            },
                        },
                    },

                    #[name = "stage_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_hexpand: true,
                        set_vexpand: true,
                        add_css_class: "kalam-editor-stage",

                        #[name = "web_host"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_hexpand: true,
                            set_vexpand: true,
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
        };

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

        rebuild_chapter_list(&widgets.chapter_list, &model.chapter_titles, model.chapter, &sender);
        refresh_patches_panel(&model, &widgets.patches_host, &sender);

        // Escape with no editor open is the way back. An open
        // paragraph editor's own key controller stops Escape first,
        // so the two never fight.
        let key = gtk::EventControllerKey::new();
        let tx = sender.input_sender().clone();
        key.connect_key_pressed(move |_, keyval, _, _| {
            if keyval == gtk::gdk::Key::Escape {
                let _ = tx.send(EpubEditorMsg::Close);
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
                if let Some(view) = &self.view {
                    view.close();
                }
                sender.output(EpubEditorOut::Close).ok();
            }
            EpubEditorMsg::TocSelect(idx) => {
                if let Some(view) = &self.view {
                    if idx < self.chapter_count && idx != self.chapter {
                        view.goto_chapter(idx, 0.0);
                        self.chapter = idx;
                        rebuild_chapter_list(
                            &widgets.chapter_list,
                            &self.chapter_titles,
                            self.chapter,
                            &sender,
                        );
                    }
                }
            }
            EpubEditorMsg::EnginePosition(chapter, _fraction) => {
                if chapter != self.chapter && chapter < self.chapter_count {
                    self.chapter = chapter;
                    rebuild_chapter_list(
                        &widgets.chapter_list,
                        &self.chapter_titles,
                        self.chapter,
                        &sender,
                    );
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
                let (prev, next) = match &edit.scope {
                    EditScope::Selection => (None, None),
                    EditScope::Paragraph { prev, next } => (prev.clone(), next.clone()),
                };
                let prior = match self
                    .service
                    .catalog()
                    .get_pending_patches_for_book(self.book_id)
                {
                    Ok(p) => p,
                    Err(e) => {
                        crate::notify::error(
                            "Could not read this book's saved fixes",
                            &e.to_string(),
                        );
                        return;
                    }
                };
                let (verdict_tx, verdict_rx) =
                    async_channel::unbounded::<Result<VerifiedEdit, (String, u64)>>();
                let tx = sender.input_sender().clone();
                gtk::glib::spawn_future_local(async move {
                    if let Ok(verdict) = verdict_rx.recv().await {
                        let _ = tx.send(EpubEditorMsg::InlineEditVerified(verdict));
                    }
                });
                engine::verify_paragraph_edit(
                    path, chapter, prior, original, prev, next, corrected, serial, verdict_tx,
                );
                if let Some(edit) = self.inline_edit.as_mut() {
                    edit.committed = true;
                }
            }
            EpubEditorMsg::CancelInlineEdit => {
                self.close_inline_edit();
            }
            EpubEditorMsg::InlineEditVerified(verdict) => {
                match verdict {
                    Ok(verified) => {
                        let inserted = self.service.catalog().insert_patch(
                            self.book_id,
                            verified.kind,
                            &verified.href,
                            verified.chapter as i64,
                            &verified.planned.find_text,
                            &verified.planned.replace_text,
                            &verified.planned.context_before,
                            &verified.planned.context_after,
                            "Editor",
                        );
                        match inserted {
                            Ok(_) => {
                                self.patches_count = self.pending_count();
                                refresh_patches_panel(self, &widgets.patches_host, &sender);
                                let current = self
                                    .inline_edit
                                    .as_ref()
                                    .is_some_and(|edit| edit.serial == verified.serial);
                                if current {
                                    self.close_inline_edit();
                                    self.reload(&sender);
                                    crate::notify::success(
                                        "Fixed",
                                        "The edit now shows in the book.",
                                    );
                                } else {
                                    crate::notify::success(
                                        "Fixed",
                                        "The edit is saved — it shows when this paragraph edit lands.",
                                    );
                                }
                            }
                            Err(e) => {
                                crate::notify::error("Could not save the edit", &e.to_string());
                            }
                        }
                    }
                    Err((toast, serial)) => {
                        crate::notify::info("Couldn't save that edit", &toast);
                        if let Some(edit) = self.inline_edit.as_mut() {
                            if edit.serial == serial {
                                edit.committed = false;
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
                } else if self.patches_count != self.patches_snapshot {
                    // The panel baked, accepted or rejected while it
                    // was open: what the engine is showing is stale.
                    self.reload(&sender);
                }
                self.patches_open = open;
            }
        }
    }
}

impl EpubEditorModel {
    /// The pending count, for the badge.
    fn pending_count(&self) -> usize {
        self.service
            .catalog()
            .get_pending_patches_for_book(self.book_id)
            .map(|p| p.len())
            .unwrap_or(0)
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
        new_view
            .goto_chapter(self.chapter.min(self.chapter_count.saturating_sub(1)), 0.0);
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

/// The chapter list: a flat column of buttons, the current chapter
/// marked. Rebuilt rather than patched — a book's chapter count is
/// small and this is not a path anyone scrolls hot.
fn rebuild_chapter_list(
    list: &gtk::Box,
    titles: &[String],
    current: usize,
    sender: &ComponentSender<EpubEditorModel>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    for (idx, title) in titles.iter().enumerate() {
        let btn = gtk::Button::with_label(title);
        btn.add_css_class("kalam-editor-chapter");
        if idx == current {
            btn.add_css_class("kalam-editor-chapter-current");
        }
        btn.set_hexpand(true);
        btn.set_halign(gtk::Align::Fill);
        let tx = sender.input_sender().clone();
        btn.connect_clicked(move |_| {
            let _ = tx.send(EpubEditorMsg::TocSelect(idx));
        });
        list.append(&btn);
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
