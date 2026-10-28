//! Immersive EPUB reader module.

<<<<<<< Updated upstream
pub(crate) mod book_settings;
=======
mod book_settings;
>>>>>>> Stashed changes
mod chapter;
mod chrome;
pub(crate) mod engine;
mod dictionary_popover;
mod keybinds;
mod lists;
mod mod_model;
mod panels;
mod session;
mod settings_panel;
mod types;

pub use mod_model::ReaderModel;
pub use types::ReaderOut;

use chrome::{
    connect_hover_open, overlay_child_box, rebuild_cover_host, reader_sidebar_tab_content,
    sync_reader_controls, sync_reader_stacks, sync_ribbon, sync_sidebar_tabs,
    update_chrome_labels, update_sidebar_header,
};
use lists::{rebuild_notes_list, rebuild_toc};
use panels::build_notes_panel;
use settings_panel::build_reader_settings_panel;
use types::*;

use crate::db::Catalog;
use crate::models::Book;
use crate::service::LibraryService;
use crate::widgets::segmented::{segmented_control, SegmentedSize};
use gtk::glib;
use gtk::prelude::*;
use relm4::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

fn report_errors(errors: &[String]) {
    for err in errors {
        crate::notify::error("Could not open this book properly", err);
    }
}


/// Install (or replace) the window-global shortcut for jump-back from the
/// current binding (roadmaps 2.6 and 2.9). There is deliberately no
/// on-screen undo button — it read as clutter — so the key must answer
/// wherever the focus happens to sit right after a jump: a sidebar row,
/// a popup, nowhere at all. A window-global shortcut with a typing guard
/// does that, the same route Ctrl+F always used; the focus-dependent
/// capture path on the reader root could miss, which is what the first
/// field report showed. When nothing jumpable is fresh, the handler
/// declines.
fn install_jump_back_shortcut(
    root: &gtk::Overlay,
    slot: &std::rc::Rc<std::cell::RefCell<Option<gtk::ShortcutController>>>,
    keybinds: &std::rc::Rc<std::cell::RefCell<keybinds::KeyBindings>>,
    sender: &ComponentSender<ReaderModel>,
) {
    if let Some(old) = slot.borrow_mut().take() {
        root.remove_controller(&old);
    }
    let pref = keybinds
        .borrow()
        .binding(keybinds::ReaderAction::JumpBack)
        .to_pref();
    if pref == keybinds::UNBOUND {
        return;
    }
    let accel = match pref.strip_prefix("<ctrl>") {
        Some(rest) => format!("<Control>{rest}"),
        None => pref,
    };
    let Some(trigger) = gtk::ShortcutTrigger::parse_string(&accel) else {
        return;
    };
    let shortcuts = gtk::ShortcutController::new();
    shortcuts.set_scope(gtk::ShortcutScope::Global);
    let s = sender.clone();
    let action = gtk::CallbackAction::new(move |w, _| {
        // Deleting a character in the search box is not a jump.
        let typing = w
            .root()
            .and_then(|r| r.focus())
            .is_some_and(|f| f.is::<gtk::Editable>() || f.is::<gtk::Text>());
        if typing {
            return gtk::glib::Propagation::Proceed;
        }
        s.input(ReaderMsg::JumpBack);
        gtk::glib::Propagation::Stop
    });
    shortcuts.add_shortcut(
        gtk::Shortcut::builder()
            .trigger(&trigger)
            .action(&action)
            .build(),
    );
    root.add_controller(shortcuts.clone());
    *slot.borrow_mut() = Some(shortcuts);
}

#[relm4::component(pub)]
impl Component for ReaderModel {
    type Init = (Arc<Catalog>, i64);
    type Input = ReaderMsg;
    type Output = ReaderOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Overlay {
            add_css_class: "kalam-reader",
            set_hexpand: true,
            set_vexpand: true,

            #[wrap(Some)]
            set_child = &gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_hexpand: true,
                set_vexpand: true,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_hexpand: true,
                    set_vexpand: true,
                    add_css_class: "kalam-reader-stage",

                    #[name = "web_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_hexpand: true,
                        set_vexpand: true,
                    },
                },
            },

            // No dim layer (decision 20): panels float over a live page,
            // and a click on the page — caught in init — puts them away.

            add_overlay = &gtk::Box {
                add_css_class: "kalam-reader-hover-edge",
                add_css_class: "kalam-reader-hover-edge-left",
                set_width_request: 15,
                set_hexpand: false,
                set_vexpand: true,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Fill,
            },

            add_overlay = &gtk::Box {
                add_css_class: "kalam-reader-hover-edge",
                add_css_class: "kalam-reader-hover-edge-right",
                set_width_request: 15,
                set_hexpand: false,
                set_vexpand: true,
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Fill,
            },

            // The Back pill with its Bubble slide-out (decision 15),
            // hidden until the pointer rests in the top-left zone. The
            // unit itself is built in init (the shared slide-out pill)
            // and appended into `back_host`.
            #[name = "back_revealer"]
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_back_button && !model.left_sidebar_open,
                set_transition_type: gtk::RevealerTransitionType::SlideDown,
                set_transition_duration: 220,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Start,

                #[name = "back_host"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_margin_start: 18,
                    set_margin_top: 14,
                },
            },

            // The bookmark ribbon (decision 16): always there, hanging
            // from the top edge; the icon and the `.set` class carry
            // the state (chrome::sync_ribbon).
            #[name = "ribbon_button"]
            add_overlay = &gtk::Button {
                add_css_class: "kalam-reader-ribbon",
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Start,
                set_margin_end: 48,
                connect_clicked => ReaderMsg::ToggleBookmark,

                #[name = "ribbon_image"]
                gtk::Image {
                    set_icon_name: Some("kalam-ribbon-outline-symbolic"),
                    set_pixel_size: 26,
                },
            },

            // The location text (decision 17): `NN%`, naming the
            // chapter while hovered. The bottom pill is gone entirely
            // (decisions 17/18) — this is the page's only furniture
            // besides the ribbon.
            #[name = "location_label"]
            add_overlay = &gtk::Label {
                add_css_class: "kalam-reader-location",
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::End,
                set_margin_end: 24,
                set_margin_bottom: 16,
            },

            #[name = "left_revealer"]
            add_overlay = &gtk::Revealer {
                add_css_class: "kalam-reader-sidebar-shell",
                add_css_class: "kalam-reader-sidebar-shell-left",
                #[watch]
                set_reveal_child: model.left_sidebar_open,
                set_transition_type: gtk::RevealerTransitionType::SlideRight,
                set_transition_duration: 220,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Fill,
                // Panels float 16 from every edge (decision 20).
                set_margin_start: 16,
                set_margin_top: 16,
                set_margin_bottom: 16,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_size_request: (340, -1),
                    add_css_class: "kalam-reader-sidebar",
                    add_css_class: "kalam-reader-sidebar-left",

                    gtk::Box {
                        add_css_class: "kalam-reader-book-head",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 10,

                        #[name = "left_cover_host"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            add_css_class: "kalam-reader-cover-slot",
                            set_width_request: 48,
                        },

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 4,
                            set_hexpand: true,

                            #[name = "sidebar_book_title"]
                            gtk::Label {
                                add_css_class: "kalam-reader-book-title",
                                add_css_class: "kalam-title-serif",
                                set_halign: gtk::Align::Start,
                                set_wrap: true,
                                set_wrap_mode: gtk::pango::WrapMode::WordChar,
                                set_ellipsize: gtk::pango::EllipsizeMode::End,
                                set_max_width_chars: 24,
                                set_width_chars: 1,
                                set_lines: 2,
                                set_xalign: 0.0,
                            },

                            #[name = "sidebar_book_author"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_halign: gtk::Align::Start,
                            },
                        },

                        // The pencil leaves the book for its library
                        // page (decision 20) — metadata edits live
                        // there.
                        gtk::Button {
                            add_css_class: "kalam-btn-icon",
                            set_valign: gtk::Align::Start,
                            set_tooltip_text: Some("Book details"),
                            set_child: Some(&crate::icons::symbolic_with_classes("document-edit-symbolic", 16, &["kalam-inline-icon"])),
                            connect_clicked => ReaderMsg::OpenBookDetails,
                        },
                    },

                    #[name = "left_panel_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_hexpand: true,
                        set_vexpand: true,
                    },

                    gtk::Box {
                        add_css_class: "kalam-reader-tabbar",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 2,
                        set_homogeneous: true,

                        #[name = "left_contents_tab"]
                        gtk::Button {
                            set_child: Some(&reader_sidebar_tab_content("view-list-bullet-symbolic", "Contents")),
                            add_css_class: "kalam-reader-tab",
                            set_hexpand: true,
                            connect_clicked => ReaderMsg::SwitchLeftTab(LeftSidebarTab::Contents),
                        },

                        #[name = "left_notes_tab"]
                        gtk::Button {
                            set_child: Some(&reader_sidebar_tab_content("highlight-symbolic", "Notes")),
                            add_css_class: "kalam-reader-tab",
                            set_hexpand: true,
                            connect_clicked => ReaderMsg::SwitchLeftTab(LeftSidebarTab::Notes),
                        },
                    },
                },
            },

            #[name = "right_revealer"]
            add_overlay = &gtk::Revealer {
                add_css_class: "kalam-reader-sidebar-shell",
                add_css_class: "kalam-reader-sidebar-shell-right",
                #[watch]
                set_reveal_child: model.right_sidebar_open,
                set_transition_type: gtk::RevealerTransitionType::SlideLeft,
                set_transition_duration: 220,
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Fill,
                set_margin_end: 16,
                set_margin_top: 16,
                set_margin_bottom: 16,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_size_request: (340, -1),
                    add_css_class: "kalam-reader-sidebar",
                    add_css_class: "kalam-reader-sidebar-right",

                    // The right panel is Settings now (decision 23):
                    // a titled head over its Reading | Shortcuts tabs.
                    gtk::Box {
                        add_css_class: "kalam-reader-panel-head",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,

                        gtk::Label {
                            add_css_class: "kalam-reader-panel-title",
                            set_label: "Settings",
                            set_halign: gtk::Align::Start,
                            set_hexpand: true,
                        },

                        gtk::Button {
                            add_css_class: "kalam-btn-icon",
                            set_tooltip_text: Some("Close (Esc)"),
                            set_child: Some(&crate::icons::symbolic_with_classes("window-close-symbolic", 14, &["kalam-inline-icon"])),
                            connect_clicked => ReaderMsg::ToggleRightSidebar,
                        },
                    },

                    #[name = "right_panel_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_hexpand: true,
                        set_vexpand: true,
                    },
                },
            },

            add_overlay = &gtk::Revealer {
                add_css_class: "kalam-reader-search-shell",
                #[watch]
                set_reveal_child: model.search_active,
                set_transition_type: gtk::RevealerTransitionType::SlideDown,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Start,
                set_margin_top: 14,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    add_css_class: "kalam-reader-search-bar",
                    add_css_class: "epub-search",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    set_height_request: 52,
                    set_valign: gtk::Align::Center,

                    #[name = "search_entry"]
                    gtk::SearchEntry {
                        add_css_class: "k-reader-search-entry",
                        #[watch]
                        set_placeholder_text: Some(if model.search_scope == SearchScope::Chapter {
                            "Search this chapter…"
                        } else {
                            "Search this book…"
                        }),
                        set_width_request: 240,
                        set_valign: gtk::Align::Center,
                        connect_search_changed[sender] => move |entry| {
                            sender.input(ReaderMsg::UpdateSearchQuery(entry.text().to_string()));
                        },
                        connect_activate[sender] => move |_| {
                            sender.input(ReaderMsg::NextSearchResult);
                        },
                    },

                    #[name = "search_count_label"]
                    gtk::Label {
                        add_css_class: "kalam-reader-search-count",
                        #[watch]
                        set_css_classes: if model.search_no_matches() {
                            &["kalam-reader-search-count", "no-matches"]
                        } else {
                            &["kalam-reader-search-count"]
                        },
                        set_valign: gtk::Align::Center,
                        #[watch]
                        set_label: &model.search_count_label(),
                    },

                    gtk::Separator {
                        add_css_class: "k-reader-search-divider",
                        set_orientation: gtk::Orientation::Vertical,
                        set_valign: gtk::Align::Center,
                        set_height_request: 24,
                    },

                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes("pan-up-symbolic", 16, &["kalam-inline-icon"])),
                        add_css_class: "kalam-reader-search-btn",
                        #[watch]
                        set_visible: !model.search_no_matches(),
                        set_valign: gtk::Align::Center,
                        set_tooltip_text: Some("Previous match (Shift+Enter)"),
                        connect_clicked => ReaderMsg::PrevSearchResult,
                    },

                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes("pan-down-symbolic", 16, &["kalam-inline-icon"])),
                        add_css_class: "kalam-reader-search-btn",
                        #[watch]
                        set_visible: !model.search_no_matches(),
                        set_valign: gtk::Align::Center,
                        set_tooltip_text: Some("Next match (Enter)"),
                        connect_clicked => ReaderMsg::NextSearchResult,
                    },

                    #[name = "search_scope_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_valign: gtk::Align::Center,
                    },

                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes("window-close-symbolic", 14, &["kalam-inline-icon"])),
                        add_css_class: "kalam-reader-search-btn",
                        set_valign: gtk::Align::Center,
                        set_tooltip_text: Some("Close (Esc)"),
                        connect_clicked => ReaderMsg::CloseSearch,
                    },
                },
            },

            add_overlay = &gtk::Revealer {
                add_css_class: "kalam-reader-lightbox-shell",
                #[watch]
                set_visible: model.lightbox_active,
                #[watch]
                set_reveal_child: model.lightbox_active,
                set_transition_type: gtk::RevealerTransitionType::Crossfade,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Fill,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    add_css_class: "kalam-lightbox-backdrop",
                    set_orientation: gtk::Orientation::Vertical,
                    set_hexpand: true,
                    set_vexpand: true,

                    gtk::Box {
                        add_css_class: "kalam-lightbox-toolbar-dock",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Start,
                        set_margin_top: 20,

                        gtk::Box {
                            add_css_class: "kalam-lightbox-toolbar",
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 6,

                            gtk::Button {
                                set_child: Some(&crate::icons::symbolic_with_classes("zoom-in-symbolic", 16, &["kalam-inline-icon"])),
                                add_css_class: "kalam-lightbox-tool-btn",
                                set_tooltip_text: Some("Zoom In (+)"),
                                connect_clicked => ReaderMsg::LightboxZoomIn,
                            },

                            gtk::Button {
                                set_child: Some(&crate::icons::symbolic_with_classes("zoom-out-symbolic", 16, &["kalam-inline-icon"])),
                                add_css_class: "kalam-lightbox-tool-btn",
                                set_tooltip_text: Some("Zoom Out (-)"),
                                connect_clicked => ReaderMsg::LightboxZoomOut,
                            },

                            gtk::Button {
                                set_child: Some(&crate::icons::symbolic_with_classes("zoom-original-symbolic", 16, &["kalam-inline-icon"])),
                                add_css_class: "kalam-lightbox-tool-btn",
                                set_tooltip_text: Some("Reset Zoom (1:1)"),
                                connect_clicked => ReaderMsg::LightboxReset,
                            },

                            gtk::Box {
                                add_css_class: "kalam-lightbox-sep",
                            },

                            gtk::Button {
                                set_child: Some(&crate::icons::symbolic_with_classes("object-rotate-left-symbolic", 16, &["kalam-inline-icon"])),
                                add_css_class: "kalam-lightbox-tool-btn",
                                set_tooltip_text: Some("Rotate Left (↺)"),
                                connect_clicked => ReaderMsg::LightboxRotateLeft,
                            },

                            gtk::Button {
                                set_child: Some(&crate::icons::symbolic_with_classes("object-rotate-right-symbolic", 16, &["kalam-inline-icon"])),
                                add_css_class: "kalam-lightbox-tool-btn",
                                set_tooltip_text: Some("Rotate Right (↻)"),
                                connect_clicked => ReaderMsg::LightboxRotateRight,
                            },

                            gtk::Box {
                                add_css_class: "kalam-lightbox-sep",
                            },

                            gtk::Button {
                                set_child: Some(&crate::icons::symbolic_with_classes("window-close-symbolic", 16, &["kalam-inline-icon"])),
                                add_css_class: "kalam-lightbox-tool-btn",
                                add_css_class: "close",
                                set_tooltip_text: Some("Close Lightbox (Esc)"),
                                connect_clicked => ReaderMsg::CloseImageLightbox,
                            },
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_hexpand: true,
                        set_vexpand: true,
                        set_halign: gtk::Align::Center,
                        set_valign: gtk::Align::Center,

                        #[name = "lightbox_picture"]
                        gtk::Picture {
                            set_content_fit: gtk::ContentFit::Contain,
                            set_can_shrink: true,
                            set_hexpand: true,
                            set_vexpand: true,
                        },
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
        let service = LibraryService::new(catalog);
        let catalog = service.catalog();
        let snap = service.reader(book_id);
        report_errors(&snap.errors);
        let book = snap.book.clone();

        // Reading settings: the app defaults, or this book's own
        // record where one is kept (decision 147). The page follows
        // the app theme (decisions 19/145) unless the book's record
        // carries a theme. The retired `reader.theme` pref is
        // deliberately not mapped — a leftover value must not keep the
        // page off the app's colours.
        let global_settings = book_settings::BookReadingSettings::load_globals(catalog);
        let book_override = book_settings::BookReadingSettings::load_book(catalog, book_id);
        let effective = book_override.clone().unwrap_or(global_settings);
        let theme_override = effective.theme.clone();
        // The same resolution refresh_palette does from here on:
        // a stored id counts only when it round-trips.
        let palette = {
            let theme = theme_override
                .as_deref()
                .map(|id| (id, crate::theme::choice_by_id(id).resolve()))
                .filter(|(id, theme)| theme.id == *id)
                .map(|(_, theme)| theme)
                .unwrap_or_else(|| crate::theme::current(catalog));
            engine::reader_palette(&theme)
        };
        let catalog_font = effective.font_px;
        let catalog_line_height = effective.line_height;
        let catalog_column = effective.column_px;
        let catalog_family = effective.font_family.clone();
        let catalog_justify = effective.justify;
        let catalog_publisher = effective.publisher_styles;
        let scrolled = effective.scrolled;

        // The engine opens the EPUB itself: one widget holds the book, and
        // its failure is the only thing that leaves the page without text.
        let mut open_failure: Option<String> = None;
        let (book_meta, view, chapter, fraction) = if let Some(book) = book.clone() {
            let prefs = engine::engine_prefs(
                palette,
                catalog_font,
                catalog_line_height,
                catalog_column,
                catalog_family.clone(),
                catalog_justify,
                false, // no hyphenation: retired after the round-1 field report
                catalog_publisher,
            );
            match engine::open_engine(&book.file_path, prefs) {
                Ok(view) => {
                    let (ch, frac) = catalog
                        .get_reading_progress(book_id)
                        .ok()
                        .flatten()
                        .unwrap_or((0, 0.0));
                    let ch = ch.min(view.chapter_count().saturating_sub(1));
                    (book, Some(view), ch, frac)
                }
                Err(err) => {
                    eprintln!("kalam: open epub failed: {err:#}");
                    open_failure = Some(format!("{err:#}"));
                    (book, None, 0, 0.0)
                }
            }
        } else {
            (
                Book {
                    id: book_id,
                    uuid: String::new(),
                    title: "Missing book".into(),
                    authors: String::new(),
                    series: None,
                    description: String::new(),
                    format: crate::models::BookFormat::Epub,
                    file_name: String::new(),
                    file_hash: String::new(),
                    cover_name: None,
                    added_at: String::new(),
                    progress: 0,
                    rating: 0,
                    publisher: String::new(),
                    published: String::new(),
                    series_index: 0.0,
                    tags: Vec::new(),
                    cover_path: None,
                    file_path: PathBuf::new(),
                },
                None,
                0,
                0.0,
            )
        };
        let chapter_count = view.as_ref().map(|v| v.chapter_count()).unwrap_or(0);
        // Titles come from the engine's table of contents, so the pill, the
        // TOC list and the engine's own chapter dividers agree on a name.
        let chapter_titles: Vec<String> = match &view {
            Some(v) => {
                let toc = v.toc();
                (0..chapter_count)
                    .map(|i| {
                        toc_title(&toc, i).unwrap_or_else(|| format!("Chapter {}", i + 1))
                    })
                    .collect()
            }
            None => Vec::new(),
        };

        // The Reading pane's preview (spec §3): this chapter's own
        // words where a search can find a decent run of them, the
        // app's sample otherwise.
        let preview_heading = chapter_titles
            .get(chapter)
            .cloned()
            .unwrap_or_else(|| "Preview".to_string());
        let preview_text = view
            .as_ref()
            .and_then(|v| {
                v.search(" the ")
                    .into_iter()
                    .find(|r| r.chapter == chapter)
                    .map(|r| r.snippet)
            })
            .filter(|t| t.chars().count() > 120)
            .unwrap_or_else(|| crate::pages::settings::PREVIEW_TEXT.to_string());

        let chapter_annotations = if chapter_count > 0 {
            catalog
                .get_annotations_for_chapter(book_id, chapter as i64)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let all_book_annotations = snap.annotations;
        let bookmarks = snap.bookmarks;
        let saved_words = snap.saved_words;

        // The standing choices resolved above, under their working
        // names (the "catalog" ones are the effective values now).
        let catalog_dual_page = effective.dual_page;
        let catalog_autohide = effective.autohide_cursor;
        let catalog_wheel = effective.wheel_step;
        let catalog_arrow = effective.arrow_step;
        let word_memory_scope = effective.word_memory_scope.clone();
        // A fresh highlight's colour and style (decision 26): plain
        // global prefs, written whenever the chip or the annotation
        // popover leaves them somewhere new.
        let last_highlight_color = crate::db::HighlightColor::from_str_lossy(
            catalog.get_pref("reader.last_highlight_color").as_deref().unwrap_or("yellow"),
        );
        let last_highlight_style = crate::db::AnnotationStyle::from_str_lossy(
            catalog.get_pref("reader.last_highlight_style").as_deref().unwrap_or("solid"),
        );
        if let Some(v) = &view {
            let wm_words = catalog
                .get_saved_words_for_scope(book_id, &word_memory_scope)
                .unwrap_or_default();
            v.set_word_memory(wm_words.into_iter().map(|w| w.word));
        }
        let font_families = view
            .as_ref()
            .map(|v| v.font_families())
            .unwrap_or_default();

        let toc_list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        toc_list.set_hexpand(true);
        toc_list.set_vexpand(true);
        let left_stack = gtk::Stack::new();
        left_stack.set_hexpand(true);
        left_stack.set_vexpand(true);
        let toc_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .child(&toc_list)
            .build();
        toc_scroll.add_css_class("kalam-reader-panel-scroll");
        // The scroll sits inside a plain page box, the same shape as
        // the Notes page, so both stack pages are built alike.
        let contents_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
        contents_page.append(&toc_scroll);
        left_stack.add_named(&contents_page, Some("contents"));

        // Roadmap 2.9: the reader's keys. One table, shared with the key
        // controller below, so a change in settings takes effect at once.
        let keybinds = keybinds::KeyBindings::shared(catalog);

        // Filled by install_jump_back_shortcut below, and swapped whenever
        // the key is rebound in Settings.
        let jump_back_shortcut: std::rc::Rc<
            std::cell::RefCell<Option<gtk::ShortcutController>>,
        > = std::rc::Rc::new(std::cell::RefCell::new(None));

        let ReaderSettingsControls {
            root: settings_panel,
            settings_stack,
            pane_buttons: settings_pane_buttons,
            font_size_label,
            line_height_label,
            column_width_label,
            wheel_step_label,
            arrow_step_label,
            arrow_step_row,
            keybind_buttons,
            dual_page_switch,
            set_book_override_switch,
            set_book_theme_choice,
            use_defaults_btn,
            keybind_hint,
            preview_column,
            preview_body,
        } = build_reader_settings_panel(
            &sender,
            catalog_font,
            catalog_line_height,
            catalog_column,
            scrolled,
            book_override.is_some(),
            theme_override.clone(),
            preview_heading,
            preview_text.clone(),
            catalog.get_pref_i64("dict_history_enabled", 1) != 0,
            catalog_family.clone(),
            font_families,
            catalog_justify,
            catalog_publisher,
            catalog_dual_page,
            catalog_autohide,
            catalog_wheel,
            catalog_arrow,
            &keybinds,
            &word_memory_scope,
        );
        let settings_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .child(&settings_panel)
            .build();
        settings_scroll.add_css_class("kalam-reader-panel-scroll");

        // The Notes page (decisions 20–22): one list under the four
        // chips, beside Contents in the left panel.
        let notes_list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        notes_list.set_hexpand(true);
        notes_list.set_vexpand(true);
        let panels::NotesPanelControls {
            root: notes_panel,
            filter_buttons: notes_filter_buttons,
            highlight_filter_buttons,
            word_scope_buttons,
            highlights_controls: notes_highlights_controls,
            bookmarks_controls: notes_bookmarks_controls,
            words_controls: notes_words_controls,
        } = build_notes_panel(&sender, &notes_list);
        left_stack.add_named(&notes_panel, Some("notes"));

        let model = ReaderModel {
            service,
            book_id,
            book_title: book_meta.title.clone(),
            book_authors: book_meta.authors_display().to_string(),
            book_cover_path: book_meta.cover_path.clone(),
            view,
            chapter_count,
            chapter_titles,
            chapter,
            fraction,
            palette,
            font_px: catalog_font,
            line_height: catalog_line_height,
            column_px: catalog_column,
            font_family: catalog_family,
            justify: catalog_justify,
            publisher_styles: catalog_publisher,
            dual_page: catalog_dual_page,
            keybinds,
            jump_back_shortcut,
            autohide_cursor: catalog_autohide,
            wheel_step: catalog_wheel,
            arrow_step: catalog_arrow,
            can_jump_back: false,
            back_depth: 0,
            selection_chip: None,
            annotation_popover: None,
            word_preview_popover: None,
            note_preview_popover: None,
            note_preview_id: None,
            word_memory_scope,
            note_popover: None,
            dict_popover: None,
            dict_anchor: None,
            dict_suppress_clear: 0,
            scrolled,
            strip_scrollbar: None,
            chapter_annotations,
            all_book_annotations,
            annotation_search_query: String::new(),
            editing_annotation: None,
            annotation_note_draft: None,
            bookmarks,
            saved_words,
            deleted_marks: Vec::new(),
            dict_query: String::new(),
            dict_results: Vec::new(),
            dict_lookup_word: None,
            dict_lookup_def: None,
            dict_context: None,
            dict_back_stack: Vec::new(),
            last_selection: None,
            last_highlight_color,
            last_highlight_style,
            session_id: None,
            session_start: std::time::Instant::now(),
            session_start_pct: 0,
            left_tab: LeftSidebarTab::Contents,
            notes_filter: NotesFilter::All,
            book_override,
            theme_override,
            left_sidebar_open: false,
            right_sidebar_open: false,
            highlight_filter: HighlightFilter::All,
            word_scope: WordScope::Chapter,
            settings_pane: ReaderSettingsPane::Reading,
            // The page opens bare (decision 15): no 5-second courtesy.
            show_back_button: false,
            back_hover: false,
            back_gen: 0,
            back_unit: None,
            location_hover: false,
            left_sidebar_shell: None,
            right_sidebar_shell: None,
            left_sidebar_box: None,
            right_sidebar_box: None,
            left_stack,
            toc_scroll,
            toc_list,
            notes_list,
            notes_filter_buttons,
            notes_highlights_controls,
            notes_bookmarks_controls,
            notes_words_controls,
            settings_stack,
            settings_pane_buttons,
            font_size_label,
            line_height_label,
            column_width_label,
            wheel_step_label,
            arrow_step_label,
            arrow_step_row,
            keybind_buttons,
            dual_page_switch,
            set_book_override_switch,
            set_book_theme_choice,
            use_defaults_btn,
            keybind_hint,
            preview_column,
            preview_body,
            preview_text,
            highlight_filter_buttons,
            word_scope_buttons,
            search_active: false,
            search_query: String::new(),
            search_results: Vec::new(),
            search_index: 0,
            search_scope: SearchScope::Chapter,
            search_scope_control: None,
            lightbox_active: false,
            lightbox_popover: None,
            lightbox_pixbuf: None,
            lightbox_zoom: 1.0,
            lightbox_rotation: 0.0,
        };

        let mut model = model;
        if model.chapter_count > 0 {
            let _ = model.service.catalog().mark_book_opened(book_id);
            let start_pct = book.as_ref().map(|b| b.progress as i64).unwrap_or(0);
            model.session_start_pct = start_pct;
            model.session_start = std::time::Instant::now();
            model.session_id = model
                .service
                .catalog()
                .start_reading_session(book_id, start_pct)
                .ok();
        }

        let widgets = view_output!();
        if let Some(view) = &model.view {
            // The widget, with a scrollbar lying over its right edge for
            // strip mode. Over, not beside: beside it, showing the bar
            // would change the widget's width, and a new width is a fresh
            // layout of every chapter.
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
            widgets.web_host.append(&overlay);

            engine::wire(view, &sender);
            if model.scrolled {
                view.set_mode(kalam_reader::ReadingMode::Scrolled);
            }
            // Roadmap 2.7: the spread is a choice now, not a consequence
            // of the window being wide.
            view.set_dual_page(model.dual_page);
            // Roadmap 2.9: the pointer and the wheel.
            view.set_autohide_cursor(model.autohide_cursor);
            view.set_wheel_step(model.wheel_step as f32);
            view.set_arrow_step(model.arrow_step as f32);
            scrollbar.set_visible(model.scrolled);
            model.strip_scrollbar = Some(scrollbar);
            view.widget().grab_focus();
        } else {
            // The book could not be opened (or there is no file): the same
            // "press Esc to go back" page the WebKit reader showed, as a
            // plain label now that there is no HTML surface to draw it on.
            let message = match &open_failure {
                Some(err) => format!("Could not open book\n\n{err}\n\nPress Esc to go back."),
                None => "Missing book\n\nThis library row has no file to open.\n\nPress Esc to go back.".to_string(),
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
        widgets.left_panel_host.append(&model.left_stack);
        widgets.right_panel_host.append(&settings_scroll);
        let left_sidebar_box = widgets
            .left_panel_host
            .parent()
            .and_then(|w| w.downcast::<gtk::Box>().ok());
        let right_sidebar_box = widgets
            .right_panel_host
            .parent()
            .and_then(|w| w.downcast::<gtk::Box>().ok());
        model.left_sidebar_shell = left_sidebar_box
            .as_ref()
            .and_then(|sidebar| sidebar.parent())
            .and_then(|w| w.downcast::<gtk::Revealer>().ok());
        model.right_sidebar_shell = right_sidebar_box
            .as_ref()
            .and_then(|sidebar| sidebar.parent())
            .and_then(|w| w.downcast::<gtk::Revealer>().ok());
        model.left_sidebar_box = left_sidebar_box.clone();
        model.right_sidebar_box = right_sidebar_box.clone();
        rebuild_cover_host(&widgets.left_cover_host, model.book_cover_path.as_deref());
        update_chrome_labels(&widgets, &model);
        sync_ribbon(&widgets, &model);
        update_sidebar_header(&widgets, &model, &sender);
        sync_sidebar_tabs(&widgets, &model);
        sync_reader_stacks(&model);
        sync_reader_controls(&model);
        let toc_entries = model.toc_entries();
        rebuild_toc(
            &model.toc_list,
            &toc_entries,
            &model.chapter_titles,
            model.chapter,
            &sender,
        );
        rebuild_notes_list(&model, &sender);

        // The Back+Bubble unit (decision 15): the shell's own Back pill
        // — same classes, same placement (18 from the left, 14 from the
        // top, in `back_host`) — with the shared slide-out carrying an
        // icon-only Bubble behind it. Focus entering the unit reveals
        // it too, so keyboard readers meet it by tabbing.
        {
            let back_button = gtk::Button::new();
            back_button.set_child(Some(&crate::icons::labelled(
                "go-previous-symbolic",
                16,
                "Back",
                6,
            )));
            back_button.add_css_class("kalam-back-btn");
            back_button.add_css_class("kalam-back-float");
            back_button.set_can_focus(true);
            let tx = sender.input_sender().clone();
            back_button.connect_clicked(move |_| {
                let _ = tx.send(ReaderMsg::Close);
            });
            let tx = sender.input_sender().clone();
            back_button.connect_has_focus_notify(move |btn| {
                if btn.has_focus() {
                    let _ = tx.send(ReaderMsg::ShowBackChrome);
                }
            });

            let bubble_button = gtk::Button::new();
            bubble_button.set_child(Some(&crate::icons::symbolic_with_classes(
                "view-dual-symbolic",
                16,
                &["kalam-inline-icon"],
            )));
            bubble_button.set_tooltip_text(Some("Minimize to Bubble"));
            bubble_button.update_property(&[gtk::accessible::Property::Label(
                "Minimize to Bubble",
            )]);
            bubble_button.set_size_request(44, 44);

            let tx = sender.input_sender().clone();
            let unit = crate::widgets::slide_out_pill::slide_out_pill_with_button(
                &back_button,
                bubble_button,
                move || {
                    let _ = tx.send(ReaderMsg::MinimizeToBubble);
                },
            );
            widgets.back_host.append(&unit.widget());
            model.back_unit = Some(unit);
        }

        // Reduced motion turns the panel slides into fades (decision 15,
        // system §9) — decided once at open, like the other surfaces.
        if crate::appearance::reduce_motion() {
            widgets
                .back_revealer
                .set_transition_type(gtk::RevealerTransitionType::Crossfade);
            widgets
                .left_revealer
                .set_transition_type(gtk::RevealerTransitionType::Crossfade);
            widgets
                .right_revealer
                .set_transition_type(gtk::RevealerTransitionType::Crossfade);
        }

        // The location text names the chapter under the pointer (17).
        {
            let motion = gtk::EventControllerMotion::new();
            let tx = sender.input_sender().clone();
            motion.connect_enter(move |_, _, _| {
                let _ = tx.send(ReaderMsg::LocationHover(true));
            });
            let tx = sender.input_sender().clone();
            motion.connect_leave(move |_| {
                let _ = tx.send(ReaderMsg::LocationHover(false));
            });
            widgets.location_label.add_controller(motion);
        }

        // A click on the page while a panel stands puts the panel away
        // and is spent doing it (decision 20) — the page does not also
        // turn or select. Capture phase, so the claim lands before the
        // engine's own click handling; the point is tested against the
        // open panel's bounds, and clicks on other chrome pass through.
        {
            let click = gtk::GestureClick::new();
            click.set_propagation_phase(gtk::PropagationPhase::Capture);
            let tx = sender.input_sender().clone();
            let left_box = model.left_sidebar_box.clone();
            let right_box = model.right_sidebar_box.clone();
            click.connect_pressed(move |gesture, _, x, y| {
                let Some(widget) = gesture.widget() else { return };
<<<<<<< Updated upstream
                // `is_visible()` would be the wrong question here: a
                // GtkRevealer conceals its child without clearing the
                // child's (or its parents') visible flags, so a shut
                // panel still reports visible — and this capture-phase
                // claim then swallows *every* press, Back button and
                // selection drags included. Mapped is what's actually
                // on screen.
                let any_open = [left_box.as_ref(), right_box.as_ref()]
                    .into_iter()
                    .flatten()
                    .any(|panel| panel.is_mapped());
                if !any_open {
                    return;
                }
=======
>>>>>>> Stashed changes
                let point = gtk::graphene::Point::new(x as f32, y as f32);
                let on_panel = [left_box.as_ref(), right_box.as_ref()]
                    .into_iter()
                    .flatten()
                    .any(|panel| {
<<<<<<< Updated upstream
                        panel.is_mapped()
=======
                        panel.is_visible()
>>>>>>> Stashed changes
                            && widget
                                .compute_point(panel, &point)
                                .is_some_and(|p| {
                                    p.x() >= 0.0
                                        && p.y() >= 0.0
                                        && p.x() <= panel.width() as f32
                                        && p.y() <= panel.height() as f32
                                })
                    });
                if !on_panel {
                    let _ = tx.send(ReaderMsg::CloseSidebars);
<<<<<<< Updated upstream
                    gesture.set_state(gtk::EventSequenceState::Claimed);
=======
                    // Claim only when there was something to close: the
                    // message handler reports through the model, so here
                    // we claim only if a panel was visible at all.
                    let any_visible = [left_box.as_ref(), right_box.as_ref()]
                        .into_iter()
                        .flatten()
                        .any(|panel| panel.is_visible());
                    if any_visible {
                        gesture.set_state(gtk::EventSequenceState::Claimed);
                    }
>>>>>>> Stashed changes
                }
            });
            root.add_controller(click);
        }

        // Root scroll controller: hide chrome immediately when the user starts scrolling
        let scroll_ctrl = gtk::EventControllerScroll::new(
            gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::BOTH_AXES,
        );
        scroll_ctrl.set_propagation_phase(gtk::PropagationPhase::Capture);
        let tx_scroll = sender.input_sender().clone();
        scroll_ctrl.connect_scroll(move |_, _, _| {
            let _ = tx_scroll.send(ReaderMsg::UserScrolled);
            gtk::glib::Propagation::Proceed
        });
        root.add_controller(scroll_ctrl);

        // The Back unit's rest zone (decision 15): the top-left
        // 180 × 68. Enter/leave transitions are reported once each; the
        // 250 ms rest and the 300 ms grace live in the update arms.
        {
            let root_motion = gtk::EventControllerMotion::new();
            let tx = sender.input_sender().clone();
            let was_in_zone = std::rc::Rc::new(std::cell::Cell::new(false));
            let was_in_zone_move = was_in_zone.clone();
            root_motion.connect_motion(move |_, x, y| {
                let in_zone = x < 180.0 && y < 68.0;
                if in_zone != was_in_zone_move.get() {
                    was_in_zone_move.set(in_zone);
                    let _ = tx.send(ReaderMsg::BackZoneHover(in_zone));
                }
            });
            let tx_leave = sender.input_sender().clone();
            root_motion.connect_leave(move |_| {
                if was_in_zone.get() {
                    was_in_zone.set(false);
                    let _ = tx_leave.send(ReaderMsg::BackZoneHover(false));
                }
            });
            root.add_controller(root_motion);
        }
        // Hover still summons the panels (decision 20 keeps it); the
        // edge strips are overlay children 1 and 2 (the stage is 0).
        if let Some(left_hover) = overlay_child_box(&root, 1) {
            connect_hover_open(&left_hover, &sender, ReaderMsg::OpenLeftSidebar);
        }
        if let Some(right_hover) = overlay_child_box(&root, 2) {
            connect_hover_open(&right_hover, &sender, ReaderMsg::OpenRightSidebar);
        }

        if let Some(view) = &model.view {
            view.goto_chapter(model.chapter, model.fraction);
            engine::show_all_highlights(view, &model.all_book_annotations);
        }

        // The keys need the view too: Escape drops a standing selection
        // before it does anything else, the way it did in the old shell.
        let view_for_keys = model.view.clone();
        let key = gtk::EventControllerKey::new();
        key.set_propagation_phase(gtk::PropagationPhase::Capture);
        let s = sender.clone();
        let keys = model.keybinds.clone();
        // The shell's own key table: its Close-panel key closes the
        // reader's frontmost layer too (decision 158).
        let shell_keys = crate::shortcuts::shared(model.service.catalog());
        key.connect_key_pressed(move |controller, keyval, _, state| {
            use gtk::gdk::Key;
            let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            // Search is answered before the typing check, so Ctrl+F opens
            // search from inside the search box too — as it always did.
            if ctrl
                && keys
                    .borrow()
                    .binding(keybinds::ReaderAction::Search)
                    .is_key(keyval)
            {
                s.input(ReaderMsg::ToggleSearch);
                return gtk::glib::Propagation::Stop;
            }

            let is_typing = controller
                .widget()
                .and_then(|w| w.root())
                .and_then(|r| r.focus())
                .is_some_and(|f| {
                    f.is::<gtk::Editable>() || f.is::<gtk::Text>() || f.is::<gtk::TextView>()
                });

            if keyval == Key::Escape {
                // One layered Escape (decisions 14, 20, 31): the arm
                // closes the frontmost thing and only exits the book
                // from a bare page. It must not leave focus typing
                // either way — hand it back to the page.
                s.input(ReaderMsg::Escape);
                if let Some(view) = &view_for_keys {
                    let w = view.widget().clone();
                    w.grab_focus();
                    glib::idle_add_local_once(move || {
                        w.grab_focus();
                    });
                }
                return gtk::glib::Propagation::Stop;
            }

            if is_typing {
                return gtk::glib::Propagation::Proceed;
            }

            // The shell's rebindable Close-panel key (decision 158):
            // hardwired Escape above keeps its full layered behaviour;
            // this only ever closes a layer, never the book.
            if shell_keys.borrow().action_for(keyval, state)
                == Some(crate::shortcuts::ShortcutAction::ClosePanel)
            {
                s.input(ReaderMsg::ClosePanelKey);
                return gtk::glib::Propagation::Stop;
            }

<<<<<<< Updated upstream
            // The shell's "Undo the last delete" (Ctrl+Z by default)
            // is deliberately unclaimed out there; in the reader it
            // has a stack to answer with (decision 30). The typing
            // guard above keeps a note box's own text undo its own.
            if shell_keys.borrow().action_for(keyval, state)
                == Some(crate::shortcuts::ShortcutAction::UndoDelete)
            {
                s.input(ReaderMsg::UndoDelete);
                return gtk::glib::Propagation::Stop;
            }

=======
>>>>>>> Stashed changes
            // Roadmap 2.9: what a key does is the reader's to decide.
            match keys.borrow().action_for(keyval, ctrl) {
                Some(action) => {
                    s.input(action.message());
                    gtk::glib::Propagation::Stop
                }
                None => gtk::glib::Propagation::Proceed,
            }
        });
        root.add_controller(key);
        root.set_can_focus(true);

        let shortcut_ctrl = gtk::ShortcutController::new();
        shortcut_ctrl.set_scope(gtk::ShortcutScope::Global);
        let s_ctrl_f = sender.clone();
        let trigger = gtk::ShortcutTrigger::parse_string("<Control>f");
        let action = gtk::CallbackAction::new(move |_, _| {
            s_ctrl_f.input(ReaderMsg::ToggleSearch);
            glib::Propagation::Stop
        });
        if let Some(trigger) = trigger {
            let shortcut = gtk::Shortcut::builder()
                .trigger(&trigger)
                .action(&action)
                .build();
            shortcut_ctrl.add_shortcut(shortcut);
        }
        root.add_controller(shortcut_ctrl);

        // Roadmap 2.6/2.9: jump back is keyboard-only, and it must answer
        // wherever the focus happens to sit right after a jump, so it rides
        // the window-global route above rather than the focus-dependent
        // capture path. Rebinding in Settings swaps it through the slot.
        install_jump_back_shortcut(
            &root,
            &model.jump_back_shortcut,
            &model.keybinds,
            &sender,
        );

        // The search bar's Chapter / Book switch (decision 34), the
        // shared small segmented control (§22). Chapter stands
        // selected; closing the bar puts it back.
        let scope_sender = sender.clone();
        let scope_control = segmented_control(
            &["Chapter", "Book"],
            0,
            SegmentedSize::Small,
            move |index| {
                scope_sender.input(ReaderMsg::SetSearchScope(if index == 1 {
                    SearchScope::Book
                } else {
                    SearchScope::Chapter
                }));
            },
        );
        widgets.search_scope_host.append(&scope_control.widget());
        model.search_scope_control = Some(scope_control);

        let entry_key = gtk::EventControllerKey::new();
        let s_entry = sender.clone();
        entry_key.connect_key_pressed(move |_, keyval, _code, state| {
            if keyval == gtk::gdk::Key::Return || keyval == gtk::gdk::Key::KP_Enter {
                if state.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
                    s_entry.input(ReaderMsg::PrevSearchResult);
                } else {
                    s_entry.input(ReaderMsg::NextSearchResult);
                }
                gtk::glib::Propagation::Stop
            } else if keyval == gtk::gdk::Key::Escape {
                s_entry.input(ReaderMsg::CloseSearch);
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        });
        widgets.search_entry.add_controller(entry_key);

        // Screenshot loop: the page opens bare (decision 15), so a
        // proof frame of the summoned chrome has to ask for it.
        if std::env::var("KALAM_READER_ACTION").is_ok_and(|v| v == "chrome") {
            sender.input(ReaderMsg::ShowBackChrome);
        }
        // The panel frames: summon each side the way a key would.
        if std::env::var("KALAM_READER_ACTION").is_ok_and(|v| v == "contents") {
            sender.input(ReaderMsg::OpenContents);
        }
        if std::env::var("KALAM_READER_ACTION").is_ok_and(|v| v == "notes") {
            sender.input(ReaderMsg::OpenNotes(NotesFilter::All));
        }
        if std::env::var("KALAM_READER_ACTION").is_ok_and(|v| v == "settings") {
            sender.input(ReaderMsg::OpenSettingsPanel);
        }
        // The T4 popover frames. The chip stands over a selection, so
        // the frame fakes the engine's report of one (the handler and
        // the surface are the real ones); the annotation frame makes
        // a real highlight on the opening chapter, then taps it the
        // way the pointer would; the dictionary frames look up the
        // seeder's fixture words. All wait out the first paint.
        if std::env::var("KALAM_READER_ACTION").is_ok_and(|v| v == "dict") {
            let tx = sender.input_sender().clone();
            gtk::glib::timeout_add_local_once(std::time::Duration::from_secs(9), move || {
                let _ = tx.send(ReaderMsg::DictSearchSelect("bank".to_string()));
            });
        }
        if std::env::var("KALAM_READER_ACTION").is_ok_and(|v| v == "dict-missing") {
            let tx = sender.input_sender().clone();
            gtk::glib::timeout_add_local_once(std::time::Duration::from_secs(9), move || {
                let _ = tx.send(ReaderMsg::DictSearchSelect("riv".to_string()));
            });
        }
<<<<<<< Updated upstream
        // The search frames (decision 34): raise the bar and type
        // into it the way a reader would, so the entry, the count
        // and the current-match jump are all the real ones. The
        // seeded text holds "water" a handful of times a chapter;
        // "rver" is the board's own miss.
        if std::env::var("KALAM_READER_ACTION").is_ok_and(|v| v == "search") {
            let tx = sender.input_sender().clone();
            let entry = widgets.search_entry.clone();
            gtk::glib::timeout_add_local_once(std::time::Duration::from_secs(9), move || {
                let _ = tx.send(ReaderMsg::ToggleSearch);
                entry.set_text("water");
            });
        }
        if std::env::var("KALAM_READER_ACTION").is_ok_and(|v| v == "search-missing") {
            let tx = sender.input_sender().clone();
            let entry = widgets.search_entry.clone();
            gtk::glib::timeout_add_local_once(std::time::Duration::from_secs(9), move || {
                let _ = tx.send(ReaderMsg::ToggleSearch);
                entry.set_text("rver");
            });
        }
=======
>>>>>>> Stashed changes
        if std::env::var("KALAM_READER_ACTION").is_ok_and(|v| v == "chip") {
            if let Some(view) = model.view.clone() {
                let tx = sender.input_sender().clone();
                gtk::glib::timeout_add_local_once(std::time::Duration::from_secs(9), move || {
                    let w = view.widget().width();
                    let rect = gtk::gdk::Rectangle::new((w / 2 - 130).max(24), 230, 260, 26);
                    let _ = tx.send(ReaderMsg::EngineSelection(Some((
                        "the river".to_string(),
                        rect,
                    ))));
                });
            }
        }
        if std::env::var("KALAM_READER_ACTION").is_ok_and(|v| v == "annotation") {
            let made = model.view.clone().and_then(|view| {
                let res = view
                    .search(" the ")
                    .into_iter()
                    .find(|r| r.chapter == model.chapter)?;
                // The match is "the": the search code's own arithmetic
                // (start at the match, end a trimmed query later).
                let end = kalam_reader::LayeredLocator {
                    char_offset: res.locator.char_offset + 3,
                    ..res.locator.clone()
                };
                let h = kalam_reader::NewHighlight {
                    color: kalam_reader::HighlightColor::Pink,
                    style: "squiggly".to_string(),
                    text: res.locator.quote.exact.clone(),
                    start: res.locator.clone(),
                    end,
<<<<<<< Updated upstream
                    has_note: true,
=======
>>>>>>> Stashed changes
                };
                let cfi = engine::range_to_json(&h.start, &h.end);
                let id = model
                    .service
                    .catalog()
                    .insert_annotation(
                        model.book_id,
                        "highlight",
                        h.start.spine_index as i64,
                        "",
                        0,
                        "",
                        0,
                        "pink",
                        "squiggly",
                        &h.text,
                        "The turn the whole chapter hangs on.",
                    )
                    .ok()?;
                let _ = model.service.catalog().update_annotation_cfi(id, &cfi);
                view.show_highlight(id, &h);
                Some((view, id, res.chapter, res.locator.spine_fraction))
            });
            if let Some((view, id, chapter, frac)) = made {
                model.reload_annotations();
                let tx = sender.input_sender().clone();
                gtk::glib::timeout_add_local_once(std::time::Duration::from_secs(9), move || {
                    let _ = tx.send(ReaderMsg::JumpToLocation(chapter, frac));
                    let tx2 = tx.clone();
                    gtk::glib::timeout_add_local_once(
                        std::time::Duration::from_millis(1200),
                        move || {
                            let w = view.widget().width() as f64;
                            let _ = tx2.send(ReaderMsg::HighlightTapped(id, w * 0.42, 210.0));
                        },
                    );
                });
            }
        }

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        let mut refresh_sidebar_header = false;
        let mut refresh_toc = false;
        let mut refresh_notes = false;
        let mut refresh_controls = false;
        let mut refresh_chrome = false;
        let mut refresh_tabs = false;

        match msg {
            ReaderMsg::ToggleSearch => {
                self.search_active = !self.search_active;
                if !self.search_active {
                    self.search_query.clear();
                    self.search_results.clear();
                    self.search_index = 0;
                    self.reset_search_scope();
                    if let Some(view) = &self.view {
                        view.clear_search_matches();
                        let w = view.widget().clone();
                        w.grab_focus();
                        glib::idle_add_local_once(move || {
                            w.grab_focus();
                        });
                    }
                } else {
                    // The bar always opens on this chapter (34).
                    self.reset_search_scope();
                    let entry = widgets.search_entry.clone();
                    gtk::glib::idle_add_local_once(move || {
                        entry.grab_focus();
                        entry.select_region(0, -1);
                    });
                }
            }
            ReaderMsg::UpdateSearchQuery(query) => {
                self.search_query = query.clone();
                self.search_index = 0;
                if let Some(view) = &self.view {
                    self.search_results = view.search(&query);
                } else {
                    self.search_results.clear();
                }
                self.highlight_current_search_match();
            }
            ReaderMsg::NextSearchResult => {
                let len = self.scoped_search_len();
                if len > 0 {
                    self.search_index = if self.search_index >= len {
                        0
                    } else {
                        (self.search_index + 1) % len
                    };
                    self.highlight_current_search_match();
                }
            }
            ReaderMsg::PrevSearchResult => {
                let len = self.scoped_search_len();
                if len > 0 {
                    self.search_index = if self.search_index == 0 || self.search_index >= len {
                        len - 1
                    } else {
                        self.search_index - 1
                    };
                    self.highlight_current_search_match();
                }
            }
            ReaderMsg::SetSearchScope(scope) => {
                if self.search_scope != scope {
                    // Stay on the match being read where the new
                    // scope can still see it; otherwise start the
                    // scope at its first match.
                    let standing = self.current_search_result_index();
                    self.search_scope = scope;
                    let scoped = self.scoped_search_indices();
                    self.search_index = standing
                        .and_then(|global| scoped.iter().position(|idx| *idx == global))
                        .unwrap_or(0);
                    self.highlight_current_search_match();
                }
            }
            ReaderMsg::CloseSearch => {
                self.search_active = false;
                self.search_query.clear();
                self.search_results.clear();
                self.search_index = 0;
                self.reset_search_scope();
                if let Some(view) = &self.view {
                    view.clear_search_matches();
                    let w = view.widget().clone();
                    w.grab_focus();
                    glib::idle_add_local_once(move || {
                        w.grab_focus();
                    });
                }
            }
            ReaderMsg::OpenImageLightbox(w, h, bytes) => {
                self.show_image_lightbox(widgets, w, h, &bytes);
            }
            ReaderMsg::CloseImageLightbox => {
                self.lightbox_active = false;
                if let Some(popover) = self.lightbox_popover.take() {
                    popover.popdown();
                }
            }
            ReaderMsg::Close => {
                self.close_annotation_editor();
                if self.left_sidebar_open || self.right_sidebar_open {
                    self.close_sidebars();
                    refresh_tabs = true;
                } else {
                    self.dismiss_chip_surfaces();
                    engine::dismiss(self.dict_popover.take());
                    self.save_progress();
                    sender.output(ReaderOut::Close).ok();
                }
            }
            ReaderMsg::MinimizeToBubble => {
                self.dismiss_chip_surfaces();
                engine::dismiss(self.dict_popover.take());
                self.save_progress();
                sender.output(ReaderOut::MinimizeToBubble { book_id: self.book_id }).ok();
            }
            ReaderMsg::TocSelect(idx) | ReaderMsg::JumpToChapter(idx) => {
                if idx < self.chapter_count {
                    self.go_chapter(idx, 0.0);
                    refresh_sidebar_header = true;
                    refresh_toc = true;
                    refresh_notes = true;
                    refresh_chrome = true;
                }
            }
            ReaderMsg::ToggleAnnotation(id) => {
                let Some(annotation) = self
                    .all_book_annotations
                    .iter()
                    .find(|annotation| annotation.id == id)
                    .cloned()
                else {
                    return;
                };
                let idx = annotation.chapter_index as usize;
                if idx >= self.chapter_count {
                    return;
                }
                if self.editing_annotation == Some(annotation.id) {
                    self.flush_annotation_note_draft();
                    self.editing_annotation = None;
                    refresh_notes = true;
                    refresh_tabs = true;
                } else {
                    self.flush_annotation_note_draft();
                    // Notes lives in the left panel now (decision 20).
                    self.left_tab = LeftSidebarTab::Notes;
                    self.left_sidebar_open = true;
                    self.right_sidebar_open = false;
                    self.editing_annotation = Some(annotation.id);
                    // A row the engine painted is a place as well as a row:
                    // jump straight to the highlight. A WebKit-era row has no
                    // locator, so its chapter is as close as we can get.
                    let jumped = engine::highlight_of(&annotation).is_some()
                        && self
                            .view
                            .as_ref()
                            .map(|v| v.goto_highlight(id))
                            .unwrap_or(false);
                    if !jumped {
                        self.go_chapter(idx, 0.0);
                        refresh_sidebar_header = true;
                        refresh_toc = true;
                        refresh_chrome = true;
                    }
                    refresh_tabs = true;
                    refresh_notes = true;
                }
            }
            ReaderMsg::JumpToLocation(idx, frac) => {
                if idx < self.chapter_count {
                    self.go_chapter(idx, frac);
                    refresh_sidebar_header = true;
                    refresh_toc = true;
                    refresh_notes = true;
                    refresh_chrome = true;
                }
            }
            ReaderMsg::PrevChapter => {
                self.flush_annotation_note_draft();
                self.editing_annotation = None;
                self.with_view(|v| v.prev_chapter());
            }
            ReaderMsg::NextChapter => {
                self.flush_annotation_note_draft();
                self.editing_annotation = None;
                self.with_view(|v| v.next_chapter());
            }
            ReaderMsg::FontDelta(delta) => {
                let next = (self.font_px as i32 + delta).clamp(13, 24) as u32;
                if next != self.font_px {
                    self.close_annotation_editor();
                    self.font_px = next;
                    self.store_book_setting();
                    self.with_view(|v| v.set_font_px(next as f32));
                    refresh_controls = true;
                }
            }
            ReaderMsg::LineHeightDelta(delta) => {
                let next =
                    ((self.line_height * 10.0).round() as i32 + delta).clamp(13, 25) as f32 / 10.0;
                if (next - self.line_height).abs() > f32::EPSILON {
                    self.close_annotation_editor();
                    self.line_height = next;
                    self.store_book_setting();
                    self.with_view(|v| v.set_line_height(next));
                    refresh_controls = true;
                }
            }
            ReaderMsg::ColumnWidthDelta(delta) => {
                let next = (self.column_px as i32 + delta).clamp(400, 860) as u32;
                if next != self.column_px {
                    self.close_annotation_editor();
                    self.column_px = next;
                    self.store_book_setting();
                    self.with_view(|v| v.set_column_px(next as f32));
                    refresh_controls = true;
                }
            }
            ReaderMsg::SetFontFamily(family) => {
                let family = if family.trim().is_empty() {
                    None
                } else {
                    Some(family)
                };
                if family != self.font_family {
                    self.font_family = family.clone();
                    self.store_book_setting();
                    self.with_view(move |v| v.set_font_family(family));
                    refresh_controls = true;
                }
            }
            ReaderMsg::SetJustify(on) => {
                if on != self.justify {
                    self.justify = on;
                    self.store_book_setting();
                    self.with_view(move |v| v.set_justify(on));
                    refresh_controls = true;
                }
            }
            ReaderMsg::SetPublisherStyles(on) => {
                if on != self.publisher_styles {
                    self.publisher_styles = on;
                    self.store_book_setting();
                    self.with_view(move |v| v.set_publisher_styles(on));
                    refresh_controls = true;
                }
            }
            ReaderMsg::ShowNote { href, text, x, y } => {
                // A note is worth more than a stale selection chip under
                // the reader's finger.
                self.dismiss_chip_surfaces();
                engine::dismiss(self.note_popover.take());
                let Some(view) = &self.view else { return };
                let rect = gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
                let card = engine::build_note_popover(
                    view.widget().upcast_ref(),
                    &rect,
                    &text,
                    &href,
                    &sender,
                );
                card.popup();
                self.note_popover = Some(card);
            }
            ReaderMsg::GoToNote(href) => {
                engine::dismiss(self.note_popover.take());
                // with_view's closure returns nothing; follow_link's bool
                // answer is the reader's to keep.
                self.with_view(|v| {
                    v.follow_link(&href);
                });
            }
            ReaderMsg::ClearNote => {
                engine::dismiss(self.note_popover.take());
            }
            ReaderMsg::JumpBack => {
                // Keyboard-only now, and a keystroke can land long after
                // the jump went stale — the guard that used to be the
                // button's visibility moves in here. The engine pops its
                // own trail; the position report that follows withdraws
                // the offer.
                if self.can_jump_back {
                    self.can_jump_back = false;
                    self.with_view(|v| {
                        v.go_back();
                    });
                }
            }
            ReaderMsg::SetDualPage(on) => {
                if on != self.dual_page {
                    self.dual_page = on;
                    self.store_book_setting();
                    self.with_view(move |v| v.set_dual_page(on));
                    refresh_controls = true;
                }
            }
            ReaderMsg::OpenFontsFolder => {
                // Created on demand too, so the first use is not a missing
                // folder in a file manager.
                let dir = crate::paths::fonts_dir();
                if let Err(e) = std::fs::create_dir_all(&dir) {
                    log::warn!("cannot create {}: {e}", dir.display());
                }
                let uri = gtk::gio::File::for_path(&dir).uri().to_string();
                gtk::UriLauncher::new(&uri).launch(
                    None::<&gtk::Window>,
                    gtk::gio::Cancellable::NONE,
                    move |res| {
                        if let Err(e) = res {
                            log::warn!("cannot open {}: {e}", dir.display());
                            // No desktop file manager on this machine (a
                            // terminal-only setup, e.g. yazi): the next
                            // best thing is the path in the clipboard and
                            // a toast saying so.
                            if let Some(display) = gtk::gdk::Display::default() {
                                display.clipboard().set_text(&dir.display().to_string());
                                crate::notify::error(
                                    "Could not open the fonts folder",
                                    "Its path is on the clipboard instead",
                                );
                            }
                        }
                    },
                );
            }
            ReaderMsg::SetAutohideCursor(on) => {
                if on != self.autohide_cursor {
                    self.autohide_cursor = on;
                    self.store_book_setting();
                    self.with_view(move |v| v.set_autohide_cursor(on));
                    refresh_controls = true;
                }
            }
            ReaderMsg::WheelStepDelta(delta) => {
                let next = (self.wheel_step + delta).clamp(20, 400);
                if next != self.wheel_step {
                    self.wheel_step = next;
                    self.store_book_setting();
                    self.with_view(move |v| v.set_wheel_step(next as f32));
                    refresh_controls = true;
                }
            }
            ReaderMsg::ArrowStepDelta(delta) => {
                let next = (self.arrow_step + delta).clamp(10, 200);
                if next != self.arrow_step {
                    self.arrow_step = next;
                    self.store_book_setting();
                    self.with_view(move |v| v.set_arrow_step(next as f32));
                    refresh_controls = true;
                }
            }
            ReaderMsg::SetKeyBinding(action, key, ctrl) => {
                let Some(binding) = keybinds::KeyBinding::capture(key, ctrl) else {
                    return;
                };
                // A key does one thing (decision 24): one already
                // owned elsewhere refuses and says whose it is; it is
                // never stolen from under the other action.
                let refused_owner = self
                    .keybinds
                    .borrow()
                    .owner_of(&binding)
                    .filter(|owner| *owner != action);
                if let Some(owner) = refused_owner {
                    self.keybind_hint.set_label(&format!(
                        "“{}” already belongs to “{}” — unbind it there first.",
                        binding.label(),
                        owner.label()
                    ));
                    self.keybind_hint.set_visible(true);
                } else {
                    self.keybind_hint.set_visible(false);
                    {
                        let mut bindings = self.keybinds.borrow_mut();
                        bindings.bind(action, binding);
                        bindings.save(self.service.catalog(), action);
                    }
                    install_jump_back_shortcut(
                        _root,
                        &self.jump_back_shortcut,
                        &self.keybinds,
                        &sender,
                    );
                }
                // Sync restores a refused keycap to the standing
                // binding, or shows the key that just landed.
                refresh_controls = true;
            }
            ReaderMsg::ResetKeyBindings => {
                self.keybinds.borrow_mut().reset_all(self.service.catalog());
                install_jump_back_shortcut(
                    _root,
                    &self.jump_back_shortcut,
                    &self.keybinds,
                    &sender,
                );
                refresh_controls = true;
            }
            ReaderMsg::SwitchSettingsPane(pane) => {
                if pane != self.settings_pane {
                    self.settings_pane = pane;
                    refresh_controls = true;
                }
            }
            ReaderMsg::SetDictHistory(on) => {
                // A library-wide privacy pref, deliberately outside the
                // per-book record: it writes the shared pref directly.
                self.service
                    .catalog()
                    .set_pref("dict_history_enabled", if on { "1" } else { "0" });
            }
            ReaderMsg::SetBookOverride(on) => {
                // Decision 147: off forgets the book's record and the
                // app defaults come back; on keeps the standing
                // choices as this book's own from here on.
                if on {
                    self.store_book_setting();
                } else {
                    book_settings::BookReadingSettings::clear_book(
                        self.service.catalog(),
                        self.book_id,
                    );
                    self.book_override = None;
                    let globals =
                        book_settings::BookReadingSettings::load_globals(self.service.catalog());
                    self.apply_settings(&globals);
                }
                refresh_controls = true;
            }
            ReaderMsg::UseSettingsAsDefaults => {
                // Copy the standing choices over the defaults
                // (decision 147) — never the book's theme, which has
                // no global counterpart. If the book's record now
                // says nothing the defaults don't, it can go; the
                // book just follows them, theme and all.
                let current = self.current_settings();
                current.write_globals(self.service.catalog());
                if self.book_override.is_some() && current.theme.is_none() {
                    book_settings::BookReadingSettings::clear_book(
                        self.service.catalog(),
                        self.book_id,
                    );
                    self.book_override = None;
                }
                refresh_controls = true;
            }
            ReaderMsg::SetBookTheme(theme) => {
                if theme != self.theme_override {
                    self.theme_override = theme;
                    self.store_book_setting();
                    self.refresh_palette();
                    refresh_controls = true;
                }
            }
            ReaderMsg::SetScrolled(on) => {
                self.scrolled = on;
                self.store_book_setting();
                self.with_view(|v| v.set_mode(engine::mode_from_pref(on as i64)));
                if let Some(bar) = &self.strip_scrollbar {
                    bar.set_visible(on);
                }
                refresh_controls = true;
            }
            ReaderMsg::EnginePosition(chapter, fraction) => {
                // The old "progress" + "chapter-changed" bridge messages, in
                // one: where the engine is, on every page turn.
                //
                // A page turn means the tap that caused it already dropped
                // the selection in the widget, so the chip must not outlive
                // the page it was pointing at. A tap in the middle band
                // clears the selection too, but the widget says nothing on
                // that path; that one is with the engine.
                self.dismiss_chip_surfaces();
                // Roadmap 2.6: the engine keeps its own trail of jumps. A
                // depth that grew means something just moved the reader —
                // including a tap on a link the engine followed itself —
                // so offer the way back. The next ordinary turn, or a
                // return trip, withdraws the offer.
                let depth = self.view.as_ref().map_or(0, |v| v.back_depth());
                self.can_jump_back = depth > self.back_depth && depth > 0;
                self.back_depth = depth;
                let changed = chapter != self.chapter;
                let moved = changed || (self.fraction - fraction).abs() > 0.001;
                self.chapter = chapter.min(self.chapter_count.saturating_sub(1));
                self.fraction = fraction.clamp(0.0, 1.0);
                self.save_progress();
                self.checkpoint_session();
                if moved && self.show_back_button && !self.back_hover {
                    // A page turn takes the Back unit with it — unless
                    // the pointer is resting on it.
                    self.back_gen += 1;
                    self.show_back_button = false;
                    if let Some(unit) = &self.back_unit {
                        unit.hide();
                    }
                }
                if changed {
                    self.reload_annotations();
                    self.reload_bookmarks();
                    self.reload_saved_words();
<<<<<<< Updated upstream
                    if self.search_active {
                        // Chapter scope is named for the chapter
                        // being read, so a manual turn into a new
                        // one repaints the wash for it — in place,
                        // never as a jump.
                        self.paint_search_matches(false);
                    }
=======
>>>>>>> Stashed changes
                    refresh_notes = true;
                    refresh_toc = true;
                }
                refresh_sidebar_header = true;
                refresh_chrome = true;
            }
            ReaderMsg::EngineSelection(sel) => {
                self.dismiss_chip_surfaces();
                self.last_selection = sel.as_ref().map(|(text, _)| text.clone());
                self.dict_anchor = sel.as_ref().map(|(_, rect)| *rect);
                if let Some((_, rect)) = &sel {
                    let Some(view) = &self.view else { return };
                    let chip = engine::build_selection_chip(
                        view.widget().upcast_ref(),
                        rect,
                        self.last_highlight_color,
                        &sender,
                    );
                    chip.popup();
                    self.selection_chip = Some(chip);
                }
            }
            ReaderMsg::EngineSelectionMoved(rect) => {
                if let Some(rect) = rect {
                    self.dict_anchor = Some(rect);
                }
            }
            ReaderMsg::NoteSelection => {
                // Decision 26: Note highlights first (last-used colour
                // and style), then opens the note box on the result.
                self.dismiss_chip_surfaces();
                let color = self.last_highlight_color;
                let style = self.last_highlight_style;
                if let Some(id) = self.create_highlight(color, style) {
                    refresh_notes = true;
                    if let Some(anno) =
                        self.all_book_annotations.iter().find(|a| a.id == id).cloned()
                    {
                        let rect = self.dict_anchor_rect();
                        self.open_annotation_popover(&anno, rect, true, &sender);
                    }
                }
            }
            ReaderMsg::RestyleAnnotation { id, color, style } => {
                // Live from the standing popover: the page behind it
                // shows the new face at once. The note keeps its
                // stored text; the popover's close lands the draft.
                let color_val = crate::db::HighlightColor::from_str_lossy(&color);
                let style_val = crate::db::AnnotationStyle::from_str_lossy(&style);
                let note = self
                    .all_book_annotations
                    .iter()
                    .find(|a| a.id == id)
                    .map(|a| a.note.clone())
                    .unwrap_or_default();
                let res = self.service.catalog().update_annotation_details(
                    id,
                    color_val.as_str(),
                    style_val.as_str(),
                    &note,
                );
                match res {
                    Ok(()) => {
                        let eng = engine::engine_color(&color);
                        if let Some(view) = self.view.clone() {
                            view.update_highlight(id, eng, style_val.as_str());
                        }
                        self.reload_annotations();
                        refresh_notes = true;
                        self.remember_highlight_choice(color_val, style_val);
                    }
                    Err(e) => crate::notify::error("Could not update highlight", &e.to_string()),
                }
            }
            ReaderMsg::AnnotationEditClosed { id, color, style, note } => {
                // Autosave on close (decision 29). The popover that
                // sent this is already down; clear only its own slot,
                // so a chip raised after it is never taken down here.
                engine::dismiss(self.annotation_popover.take());
                let color_val = crate::db::HighlightColor::from_str_lossy(&color);
                let style_val = crate::db::AnnotationStyle::from_str_lossy(&style);
                let unchanged = self.all_book_annotations.iter().find(|a| a.id == id).is_some_and(|a| {
                    a.color.eq_ignore_ascii_case(color_val.as_str())
                        && a.style.eq_ignore_ascii_case(style_val.as_str())
                        && a.note == note
                });
                if !unchanged {
                    let res = self.service.catalog().update_annotation_details(
                        id,
                        color_val.as_str(),
                        style_val.as_str(),
                        &note,
                    );
                    match res {
                        Ok(()) => {
                            let eng = engine::engine_color(&color);
                            if let Some(view) = self.view.clone() {
                                view.update_highlight(id, eng, style_val.as_str());
<<<<<<< Updated upstream
                                view.set_highlight_note(id, !note.trim().is_empty());
=======
>>>>>>> Stashed changes
                            }
                            self.reload_annotations();
                            refresh_notes = true;
                        }
                        Err(e) => {
                            crate::notify::error("Could not update highlight", &e.to_string())
                        }
                    }
                }
                self.remember_highlight_choice(color_val, style_val);
            }
            ReaderMsg::HighlightTapped(id, x, y) => {
                self.dismiss_chip_surfaces();
                engine::dismiss(self.dict_popover.take());
                engine::dismiss(self.word_preview_popover.take());
                let anno_opt = self.service.catalog().get_annotation_by_id(id).ok().flatten();
                if let Some(anno) = anno_opt {
                    let rect = gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
                    self.open_annotation_popover(&anno, rect, false, &sender);
                }
            }
            ReaderMsg::WordMemoryTapped(word, x, y) => {
                self.dismiss_chip_surfaces();
                engine::dismiss(self.dict_popover.take());
                engine::dismiss(self.word_preview_popover.take());
                let rect = gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
                self.dict_lookup(word, None, rect, &sender);
            }
            ReaderMsg::WordMemoryHover(word_opt, x, y) => {
                if word_opt.is_none() {
                    engine::dismiss(self.word_preview_popover.take());
                    return;
                }
                let word = word_opt.unwrap_or_default();
                if self.dict_popover.is_some()
                    || self.selection_chip.is_some()
                    || self.annotation_popover.is_some()
<<<<<<< Updated upstream
                    || self.note_preview_popover.is_some()
=======
>>>>>>> Stashed changes
                {
                    return;
                }
                let rect = gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
                if let Some(view) = self.view.clone() {
                    engine::dismiss(self.word_preview_popover.take());
                    if let Some(pop) = engine::build_word_preview_tooltip(
                        view.widget().upcast_ref(),
                        &rect,
                        &word,
                        self.service.catalog(),
                    ) {
                        pop.popup();
                        self.word_preview_popover = Some(pop);
                    }
                }
            }
            ReaderMsg::NoteDotHover(id_opt, x, y) => {
                let Some(id) = id_opt else {
                    engine::dismiss(self.note_preview_popover.take());
                    self.note_preview_id = None;
                    return;
                };
                // Motion inside the same dot reports again and again;
                // the card already standing is the answer.
                if self.note_preview_id == Some(id) && self.note_preview_popover.is_some() {
                    return;
                }
                if self.dict_popover.is_some()
                    || self.selection_chip.is_some()
                    || self.annotation_popover.is_some()
                {
                    engine::dismiss(self.note_preview_popover.take());
                    self.note_preview_id = None;
                    return;
                }
                let note = self
                    .all_book_annotations
                    .iter()
                    .find(|annotation| annotation.id == id)
                    .map(|annotation| annotation.note.clone())
                    .filter(|note| !note.trim().is_empty());
                let Some(note) = note else {
                    engine::dismiss(self.note_preview_popover.take());
                    self.note_preview_id = None;
                    return;
                };
                engine::dismiss(self.word_preview_popover.take());
                engine::dismiss(self.note_preview_popover.take());
                self.note_preview_id = None;
                let rect = gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1);
                if let Some(view) = self.view.clone() {
                    if let Some(pop) = engine::build_annotation_note_preview(
                        view.widget().upcast_ref(),
                        &rect,
                        &note,
                    ) {
                        pop.popup();
                        self.note_preview_popover = Some(pop);
                        self.note_preview_id = Some(id);
                    }
                }
            }
            ReaderMsg::SetWordMemoryScope(scope) => {
                self.word_memory_scope = scope.clone();
                self.store_book_setting();
                if let Some(view) = &self.view {
                    let wm_words = self
                        .service
                        .catalog()
                        .get_saved_words_for_scope(self.book_id, &self.word_memory_scope)
                        .unwrap_or_default();
                    view.set_word_memory(wm_words.into_iter().map(|w| w.word));
                }
                refresh_controls = true;
            }
            ReaderMsg::HighlightSelection(color_name) => {
                // One tap on a dot (decision 26): the tapped colour,
                // the last-used style, no questions asked.
                self.dismiss_chip_surfaces();
                let color = crate::db::HighlightColor::from_str_lossy(&color_name);
                let style = self.last_highlight_style;
                if self.create_highlight(color, style).is_some() {
                    refresh_notes = true;
                }
                self.remember_highlight_choice(color, style);
            }
            ReaderMsg::QuoteSelection => {
                self.dismiss_chip_surfaces();
                let Some(view) = self.view.clone() else {
                    return;
                };
                let color = engine::engine_color("yellow");
                let Some(h) = view.capture_highlight(color) else {
                    return;
                };
                let cfi = engine::range_to_json(&h.start, &h.end);
                let inserted = self.service.catalog().insert_annotation(
                    self.book_id,
                    "quote",
                    h.start.spine_index as i64,
                    "",
                    0,
                    "",
                    0,
                    "yellow",
                    "solid",
                    &h.text,
                    "",
                );
                view.clear_selection();
                match inserted {
                    Ok(id) => {
                        crate::notify::report(
                            self.service.catalog().update_annotation_cfi(id, &cfi),
                            "Could not place the quote",
                        );
                        crate::notify::compact("Quote saved", "");
                        self.reload_annotations();
                        refresh_notes = true;
                    }
                    Err(e) => crate::notify::error("Could not save the quote", &e.to_string()),
                }
            }
            ReaderMsg::CopySelection => {
                self.dismiss_chip_surfaces();
                if let Some(view) = self.view.clone() {
                    if let Some(text) = view.selected_text() {
                        view.widget().clipboard().set_text(&text);
                        view.clear_selection();
                    }
                }
            }
            ReaderMsg::LookUpSelection => {
                self.dismiss_chip_surfaces();
                let Some(view) = self.view.clone() else {
                    return;
                };
                let Some(word) = view.selected_text() else {
                    return;
                };
                view.clear_selection();
                let rect = self.dict_anchor_rect();
                self.dict_lookup(word, None, rect, &sender);
            }
            ReaderMsg::Progress(frac) => {
                self.fraction = frac.clamp(0.0, 1.0);
                refresh_sidebar_header = true;
                refresh_chrome = true;
            }
            ReaderMsg::AnnotationsReload => {
                self.flush_annotation_note_draft();
                self.reload_annotations();
                self.reload_bookmarks();
                self.reload_saved_words();
                if let Some(view) = &self.view {
                    engine::show_all_highlights(view, &self.all_book_annotations);
                }
                refresh_notes = true;
                refresh_sidebar_header = true;
                refresh_chrome = true;
                refresh_toc = true;
            }
            ReaderMsg::RecolorAnnotation(id, color) => {
                self.flush_annotation_note_draft();
                let color_name = color.as_str();
                let is_highlight = self
                    .all_book_annotations
                    .iter()
                    .find(|annotation| annotation.id == id)
                    .is_some_and(|annotation| annotation.kind == "highlight");
                if !is_highlight {
                    return;
                }
                let unchanged = self
                    .all_book_annotations
                    .iter()
                    .find(|annotation| annotation.id == id)
                    .is_some_and(|annotation| annotation.color.eq_ignore_ascii_case(color_name));
                if unchanged {
                    return;
                }
                match self
                    .service
                    .catalog()
                    .update_annotation_color(id, color_name)
                {
                    Ok(()) => {
                        for annotation in &mut self.all_book_annotations {
                            if annotation.id == id {
                                annotation.color = color_name.to_string();
                            }
                        }
                        for annotation in &mut self.chapter_annotations {
                            if annotation.id == id {
                                annotation.color = color_name.to_string();
                            }
                        }
                        self.with_view(|v| v.recolor_highlight(id, engine::engine_color(color_name)));
                        refresh_notes = true;
                    }
                    Err(err) => {
                        crate::notify::error("Could not recolor the highlight", &err.to_string());
                    }
                }
            }
            ReaderMsg::AnnotationSearchChanged(query) => {
                self.close_annotation_editor();
                let query = query.trim().to_string();
                if query != self.annotation_search_query {
                    self.annotation_search_query = query;
                    refresh_notes = true;
                }
            }
            ReaderMsg::DeleteAnnotation(id) => {
                self.flush_annotation_note_draft();
                // The row as it stands, before it goes: Ctrl+Z puts
                // this back (decision 30).
                let row = self
                    .service
                    .catalog()
                    .get_annotation_by_id(id)
                    .ok()
                    .flatten();
                if crate::notify::report(
                    self.service.catalog().delete_annotation(id),
                    "Could not delete the highlight",
                ) {
                    if self.note_preview_id == Some(id) {
                        engine::dismiss(self.note_preview_popover.take());
                        self.note_preview_id = None;
                    }
                    if let Some(row) = row {
                        self.remember_deleted(DeletedMark::Annotation(Box::new(row)));
                    }
                }
                if self.editing_annotation == Some(id) {
                    self.editing_annotation = None;
                    self.annotation_note_draft = None;
                }
                self.reload_annotations();
                self.with_view(|v| v.remove_highlight(id));
                refresh_notes = true;
            }
            ReaderMsg::AnnotationNoteChanged(id, note) => {
                if self.editing_annotation == Some(id) {
                    self.annotation_note_draft = Some((id, note));
                }
            }
            ReaderMsg::SaveAnnotationNote(id, note) => {
                if self.persist_annotation_note(id, &note)
                    && matches!(
                        self.annotation_note_draft.as_ref(),
                        Some((draft_id, _)) if *draft_id == id
                    )
                {
                    self.annotation_note_draft = None;
                }
            }
            ReaderMsg::DeleteBookmark(id) => {
                let row = self.bookmarks.iter().find(|m| m.id == id).cloned();
                if crate::notify::report(
                    self.service.catalog().delete_reading_bookmark(id),
                    "Could not delete the mark",
                ) {
                    if let Some(row) = row {
                        self.remember_deleted(DeletedMark::Bookmark(Box::new(row)));
                    }
                }
                self.reload_bookmarks();
                refresh_notes = true;
<<<<<<< Updated upstream
                refresh_chrome = true;
            }
            ReaderMsg::DeleteSavedWord(id) => {
                let row = self.saved_words.iter().find(|w| w.id == id).cloned();
                if crate::notify::report(
                    self.service.catalog().delete_saved_word(id),
                    "Could not forget the word",
                ) {
                    if let Some(row) = row {
                        self.remember_deleted(DeletedMark::SavedWords(vec![row]));
                    }
                }
=======
            }
            ReaderMsg::DeleteSavedWord(id) => {
                crate::notify::report(
                    self.service.catalog().delete_saved_word(id),
                    "Could not forget the word",
                );
>>>>>>> Stashed changes
                // reload_saved_words re-pushes the word-memory set, so
                // the dotted underline goes with the row.
                self.reload_saved_words();
                refresh_notes = true;
<<<<<<< Updated upstream
            }
            ReaderMsg::UndoDelete => {
                // Decision 30: deletions are direct because Ctrl+Z
                // brings them back — the row as it stood, restored
                // exactly. A failed restore keeps its place on the
                // stack rather than being lost with the attempt.
                if let Some(mark) = self.deleted_marks.pop() {
                    let catalog = self.service.catalog();
                    match mark {
                        DeletedMark::Annotation(row) => {
                            match catalog.restore_annotation(&row) {
                                Ok(()) => {
                                    self.reload_annotations();
                                    if let Some(view) = &self.view {
                                        if let Some(h) = engine::highlight_of(&row) {
                                            view.show_highlight(row.id, &h);
                                        }
                                    }
                                    refresh_notes = true;
                                }
                                Err(err) => {
                                    self.deleted_marks
                                        .push(DeletedMark::Annotation(row));
                                    crate::notify::error(
                                        "Could not bring the highlight back",
                                        &err.to_string(),
                                    );
                                }
                            }
                        }
                        DeletedMark::Bookmark(row) => {
                            match catalog.restore_reading_bookmark(&row) {
                                Ok(()) => {
                                    self.reload_bookmarks();
                                    refresh_notes = true;
                                    refresh_chrome = true;
                                }
                                Err(err) => {
                                    self.deleted_marks.push(DeletedMark::Bookmark(row));
                                    crate::notify::error(
                                        "Could not bring the bookmark back",
                                        &err.to_string(),
                                    );
                                }
                            }
                        }
                        DeletedMark::SavedWords(rows) => {
                            let mut failed = Vec::new();
                            let mut first_error = None;
                            for row in rows {
                                if let Err(err) = catalog.restore_saved_word(&row) {
                                    first_error.get_or_insert(err);
                                    failed.push(row);
                                }
                            }
                            self.reload_saved_words();
                            refresh_notes = true;
                            if !failed.is_empty() {
                                self.deleted_marks
                                    .push(DeletedMark::SavedWords(failed));
                                if let Some(err) = first_error {
                                    crate::notify::error(
                                        "Could not bring the word back",
                                        &err.to_string(),
                                    );
                                }
                            }
                        }
                    }
                }
=======
>>>>>>> Stashed changes
            }
            ReaderMsg::DictSearch(q) => {
                self.dict_query = q.clone();
                if q.trim().is_empty() {
                    self.dict_results.clear();
                } else {
                    self.dict_results = self.lookup_dict(&q, 30);
                }
                refresh_notes = true;
            }
            ReaderMsg::DictSearchSelect(word) => {
                // In-popup navigation (decision 31): following a chip
                // while the popover is up leaves a way back. A select
                // with no popover up — a row in the Notes panel's Words
                // view — starts a fresh lookup instead.
                if self.dict_popover.is_some() {
                    if let Some(cur) = self.dict_lookup_word.clone() {
                        if !cur.eq_ignore_ascii_case(&word) {
                            self.dict_back_stack.push(cur);
                        }
                    }
                } else {
                    self.dict_back_stack.clear();
                }
                self.dict_show_word(&word, &sender);
            }
            ReaderMsg::DictGoBack => {
                if let Some(word) = self.dict_back_stack.pop() {
                    self.dict_show_word(&word, &sender);
                }
            }
            ReaderMsg::ToggleSaveWord => {
                // The toggle (decision 31) flips its own face the
                // moment it is tapped; this lands the same answer in
                // the store and keeps the Words list true. The popover
                // stays up either way.
                if let Some(word) = self.dict_lookup_word.clone() {
                    let catalog = self.service.catalog();
                    let saved = catalog.saved_word_exists(&word, self.book_id).unwrap_or(false);
                    if saved {
<<<<<<< Updated upstream
                        // Every row this stroke forgets, kept for
                        // Ctrl+Z (decisions 30, 31).
                        let rows: Vec<_> = self
                            .saved_words
                            .iter()
                            .filter(|w| {
                                w.book_id == Some(self.book_id)
                                    && w.word.eq_ignore_ascii_case(&word)
                            })
                            .cloned()
                            .collect();
                        match catalog.delete_saved_word_for(&word, self.book_id) {
                            Ok(()) => {
                                if !rows.is_empty() {
                                    self.remember_deleted(DeletedMark::SavedWords(rows));
                                }
=======
                        match catalog.delete_saved_word_for(&word, self.book_id) {
                            Ok(()) => {
>>>>>>> Stashed changes
                                crate::notify::compact("Word forgotten", &word);
                                self.reload_saved_words();
                                refresh_notes = true;
                            }
                            Err(e) => crate::notify::error(
                                "Could not forget the word",
                                &e.to_string(),
                            ),
                        }
                    } else if let Some(def) = self.dict_lookup_def.clone() {
                        match catalog.insert_saved_word(
                            &word,
                            &def,
                            None,
                            Some(self.book_id),
                            Some(self.chapter as i64),
                            self.dict_context.as_deref(),
                        ) {
                            Ok(_) => {
                                crate::notify::compact("Word saved", &word);
                                self.reload_saved_words();
                                refresh_notes = true;
                            }
                            Err(e) => crate::notify::error(
                                "Could not save the word",
                                &e.to_string(),
                            ),
                        }
                    }
                }
            }
            ReaderMsg::ClearDict => {
                // A popover that a newer one replaced still reports itself
                // closed; that report must not take the new popover down.
                if self.dict_suppress_clear > 0 {
                    self.dict_suppress_clear -= 1;
                    return;
                }
                engine::dismiss(self.dict_popover.take());
                self.dict_lookup_word = None;
                self.dict_lookup_def = None;
                self.dict_context = None;
                self.dict_back_stack.clear();
            }
            ReaderMsg::ToggleBookmark => {
                // The ribbon (decision 16): set or take away the mark
                // here, quietly — no panel pulled open, no toast; the
                // ribbon filling in is the whole reply. The Marks list
                // stays true underneath for when it is next opened.
                if self.chapter_count > 0 {
                    if let Some(id) = self.bookmark_here_id() {
<<<<<<< Updated upstream
                        let row = self.bookmarks.iter().find(|m| m.id == id).cloned();
                        match self.service.catalog().delete_reading_bookmark(id) {
                            Ok(()) => {
                                if let Some(row) = row {
                                    self.remember_deleted(DeletedMark::Bookmark(Box::new(row)));
                                }
                            }
                            Err(e) => crate::notify::error(
                                "Could not remove the bookmark",
                                &e.to_string(),
                            ),
=======
                        if let Err(e) = self.service.catalog().delete_reading_bookmark(id) {
                            crate::notify::error(
                                "Could not remove the bookmark",
                                &e.to_string(),
                            );
>>>>>>> Stashed changes
                        }
                    } else {
                        let label = self.current_chapter_title().to_string();
                        if let Err(e) = self.service.catalog().insert_reading_bookmark(
                            self.book_id,
                            self.chapter as i64,
                            self.fraction,
                            &label,
                        ) {
                            crate::notify::error(
                                "Could not save the bookmark",
                                &e.to_string(),
                            );
                        }
                    }
                    self.reload_bookmarks();
                    refresh_notes = true;
                    refresh_chrome = true;
                }
            }
            ReaderMsg::OpenAuthor(name) => {
                sender.output(ReaderOut::OpenAuthor { name }).ok();
            }
            ReaderMsg::OpenBookDetails => {
                sender
                    .output(ReaderOut::OpenBook {
                        book_id: self.book_id,
                    })
                    .ok();
            }
            ReaderMsg::OpenLeftSidebar => {
                // Popover keep-alive: only ever opens, never toggles.
                self.right_sidebar_open = false;
                if self.left_tab == LeftSidebarTab::Contents {
                    self.position_toc_scroll();
                }
                self.left_sidebar_open = true;
                refresh_tabs = true;
            }
            ReaderMsg::OpenRightSidebar => {
                self.left_sidebar_open = false;
                self.right_sidebar_open = true;
                refresh_tabs = true;
            }
            ReaderMsg::ToggleRightSidebar => {
                self.right_sidebar_open = !self.right_sidebar_open;
                if self.right_sidebar_open {
                    self.left_sidebar_open = false;
                }
                refresh_tabs = true;
            }
            ReaderMsg::OpenContents => {
                // The summon key (decision 19): second press closes.
                if self.left_sidebar_open && self.left_tab == LeftSidebarTab::Contents {
                    self.close_annotation_editor();
                    self.left_sidebar_open = false;
                } else {
                    self.left_tab = LeftSidebarTab::Contents;
                    self.right_sidebar_open = false;
                    self.position_toc_scroll();
                    self.left_sidebar_open = true;
                }
                refresh_tabs = true;
            }
            ReaderMsg::OpenNotes(filter) => {
                if self.left_sidebar_open
                    && self.left_tab == LeftSidebarTab::Notes
                    && self.notes_filter == filter
                {
                    self.close_annotation_editor();
                    self.left_sidebar_open = false;
                } else {
                    if self.notes_filter != filter {
                        self.close_annotation_editor();
                    }
                    self.left_tab = LeftSidebarTab::Notes;
                    self.notes_filter = filter;
                    self.right_sidebar_open = false;
                    self.left_sidebar_open = true;
                    refresh_controls = true;
                    refresh_notes = true;
                }
                refresh_tabs = true;
            }
            ReaderMsg::OpenSettingsPanel => {
                if self.right_sidebar_open {
                    self.right_sidebar_open = false;
                } else {
                    self.left_sidebar_open = false;
                    self.right_sidebar_open = true;
                    refresh_controls = true;
                }
                refresh_tabs = true;
            }
            ReaderMsg::SwitchLeftTab(tab) => {
                self.close_annotation_editor();
                self.left_tab = tab;
                self.right_sidebar_open = false;
                if matches!(tab, LeftSidebarTab::Contents) {
                    self.position_toc_scroll();
                }
                self.left_sidebar_open = true;
                refresh_tabs = true;
            }
            ReaderMsg::SetNotesFilter(filter) => {
                if filter != self.notes_filter {
                    self.close_annotation_editor();
                    self.notes_filter = filter;
                }
                refresh_controls = true;
                refresh_notes = true;
            }
            ReaderMsg::SetHighlightFilter(filter) => {
                self.close_annotation_editor();
                self.highlight_filter = filter;
                refresh_controls = true;
                refresh_notes = true;
            }
            ReaderMsg::SetWordScope(scope) => {
                self.word_scope = scope;
                refresh_controls = true;
                refresh_notes = true;
            }
            ReaderMsg::ExportAnnotations => {
                let catalog = self.service.catalog();
                match lists::export_highlights_markdown(
                    catalog,
                    self.book_id,
                    &self.book_title,
                ) {
                    Ok((n, path)) => crate::notify::success(
                        &format!("Exported {n} highlights & notes"),
                        &path.display().to_string(),
                    ),
                    Err(err) => crate::notify::error("Could not export highlights", &err),
                }
            }
            ReaderMsg::CloseSidebars => {
                self.close_annotation_editor();
                self.close_sidebars();
                refresh_tabs = true;
            }
            ReaderMsg::Escape => {
                // The layers, front to back (decisions 14, 20, 31):
                // dictionary popover → annotation popover → selection
                // chip → selection → search → lightbox → panels →
                // exit the book. Each press takes exactly one layer.
                if self.dict_popover.as_ref().is_some_and(|p| p.is_visible()) {
                    sender.input(ReaderMsg::ClearDict);
                } else if self.annotation_popover.as_ref().is_some_and(|p| p.is_visible()) {
                    engine::dismiss(self.annotation_popover.take());
                } else if self.selection_chip.as_ref().is_some_and(|p| p.is_visible()) {
                    engine::dismiss(self.selection_chip.take());
                } else if self.view.as_ref().is_some_and(|v| v.selected_text().is_some()) {
                    self.with_view(|v| v.clear_selection());
                } else if self.search_active {
                    sender.input(ReaderMsg::CloseSearch);
                } else if self.lightbox_active {
                    sender.input(ReaderMsg::CloseImageLightbox);
                } else if self.left_sidebar_open || self.right_sidebar_open {
                    sender.input(ReaderMsg::CloseSidebars);
                } else {
                    sender.input(ReaderMsg::Close);
                }
            }
            ReaderMsg::ClosePanelKey => {
                // The shell's rebindable Close-panel key (decision
                // 158): the same layers as Escape, minus the selection
                // (a text gesture, not a panel) and minus ever leaving
                // the book — reassigning it must not become a way out.
                if self.dict_popover.as_ref().is_some_and(|p| p.is_visible()) {
                    sender.input(ReaderMsg::ClearDict);
                } else if self.annotation_popover.as_ref().is_some_and(|p| p.is_visible()) {
                    engine::dismiss(self.annotation_popover.take());
                } else if self.selection_chip.as_ref().is_some_and(|p| p.is_visible()) {
                    engine::dismiss(self.selection_chip.take());
                } else if self.search_active {
                    sender.input(ReaderMsg::CloseSearch);
                } else if self.lightbox_active {
                    sender.input(ReaderMsg::CloseImageLightbox);
                } else if self.left_sidebar_open || self.right_sidebar_open {
                    sender.input(ReaderMsg::CloseSidebars);
                }
            }
            ReaderMsg::ShowBackChrome => {
                self.back_hover = true;
                self.back_gen += 1;
                self.show_back_button = true;
                refresh_chrome = true;
            }
            ReaderMsg::BackZoneHover(hovering) => {
                if !hovering && !self.back_hover {
                    // Already accounted away (a focus reveal never
                    // entered the zone) — nothing to schedule.
                    return;
                }
                self.back_hover = hovering;
                self.back_gen += 1;
                let gen = self.back_gen;
                let tx = sender.input_sender().clone();
                if hovering {
                    // The 250 ms rest before the unit drops in.
                    glib::timeout_add_local_once(Duration::from_millis(250), move || {
                        let _ = tx.send(ReaderMsg::BackRevealDue(gen));
                    });
                } else {
                    if let Some(unit) = &self.back_unit {
                        unit.hide();
                    }
                    // The 300 ms grace, then the whole unit goes.
                    glib::timeout_add_local_once(Duration::from_millis(300), move || {
                        let _ = tx.send(ReaderMsg::BackHideDue(gen));
                    });
                }
                refresh_chrome = true;
            }
            ReaderMsg::BackRevealDue(gen) => {
                if gen == self.back_gen && self.back_hover && !self.left_sidebar_open {
                    self.show_back_button = true;
                }
                refresh_chrome = true;
            }
            ReaderMsg::BackHideDue(gen) => {
                if gen == self.back_gen && !self.back_hover {
                    self.show_back_button = false;
                }
                refresh_chrome = true;
            }
            ReaderMsg::LocationHover(hovering) => {
                self.location_hover = hovering;
                refresh_chrome = true;
            }
            ReaderMsg::LightboxZoomIn => {
                self.lightbox_zoom = (self.lightbox_zoom * 1.25).min(5.0);
                self.update_lightbox_image(widgets);
            }
            ReaderMsg::LightboxZoomOut => {
                self.lightbox_zoom = (self.lightbox_zoom / 1.25).max(0.2);
                self.update_lightbox_image(widgets);
            }
            ReaderMsg::LightboxReset => {
                self.lightbox_zoom = 1.0;
                self.lightbox_rotation = 0.0;
                self.update_lightbox_image(widgets);
            }
            ReaderMsg::LightboxRotateLeft => {
                self.lightbox_rotation = (self.lightbox_rotation - 90.0) % 360.0;
                self.update_lightbox_image(widgets);
            }
            ReaderMsg::LightboxRotateRight => {
                self.lightbox_rotation = (self.lightbox_rotation + 90.0) % 360.0;
                self.update_lightbox_image(widgets);
            }
            ReaderMsg::UserScrolled => {
                // Reading again: the Back unit goes away at once.
                if self.show_back_button {
                    self.back_gen += 1;
                    self.back_hover = false;
                    self.show_back_button = false;
                    if let Some(unit) = &self.back_unit {
                        unit.hide();
                    }
                    refresh_chrome = true;
                }
            }
        }

        if refresh_sidebar_header {
            update_sidebar_header(widgets, self, &sender);
        }
        if refresh_chrome {
            update_chrome_labels(widgets, self);
            sync_ribbon(widgets, self);
        }
        if refresh_controls || refresh_tabs {
            sync_reader_stacks(self);
        }
        if refresh_controls {
            sync_reader_controls(self);
        }
        if refresh_tabs {
            sync_sidebar_tabs(widgets, self);
        }
        if refresh_toc {
            let entries = self.toc_entries();
            rebuild_toc(
                &self.toc_list,
                &entries,
                &self.chapter_titles,
                self.chapter,
                &sender,
            );
            if self.left_sidebar_open && self.left_tab == LeftSidebarTab::Contents {
                self.position_toc_scroll();
            }
        }
        if refresh_notes {
            rebuild_notes_list(self, &sender);
        }

        self.update_view(widgets, sender);
    }

    fn shutdown(&mut self, _widgets: &mut Self::Widgets, _output: relm4::Sender<Self::Output>) {
        self.close_annotation_editor();
        self.save_progress();
        self.close_session();
        self.dismiss_chip_surfaces();
        engine::dismiss(self.dict_popover.take());
        engine::dismiss(self.word_preview_popover.take());
        if let Some(view) = self.view.take() {
            view.close();
        }
    }
}

impl ReaderModel {
    /// Take the selection chip and the annotation popover down
    /// together. The popover's autosave rides its `closed`, so a
    /// dismiss here still lands any note typed into it.
    fn dismiss_chip_surfaces(&mut self) {
        engine::dismiss(self.selection_chip.take());
        engine::dismiss(self.annotation_popover.take());
<<<<<<< Updated upstream
        engine::dismiss(self.note_preview_popover.take());
        self.note_preview_id = None;
=======
>>>>>>> Stashed changes
    }

    /// Where the chip and the annotation popover last left the
    /// marker (decision 26): a fresh highlight takes this colour and
    /// style. Plain global prefs, like the dictionary-history one.
    fn remember_highlight_choice(
        &mut self,
        color: crate::db::HighlightColor,
        style: crate::db::AnnotationStyle,
    ) {
        if self.last_highlight_color != color {
            self.last_highlight_color = color;
            self.service
                .catalog()
                .set_pref("reader.last_highlight_color", color.as_str());
        }
        if self.last_highlight_style != style {
            self.last_highlight_style = style;
            self.service
                .catalog()
                .set_pref("reader.last_highlight_style", style.as_str());
        }
    }

    /// Capture the standing selection as a highlight of `color` and
    /// `style`, place it, paint it and reload. The new row's id, or
    /// `None` when there was nothing to capture or the save failed.
    fn create_highlight(
        &mut self,
        color: crate::db::HighlightColor,
        style: crate::db::AnnotationStyle,
    ) -> Option<i64> {
        let view = self.view.clone()?;
        let eng = engine::engine_color(color.as_str());
        let h = view.capture_highlight_with_style(eng, style.as_str())?;
        let cfi = engine::range_to_json(&h.start, &h.end);
        let inserted = self.service.catalog().insert_annotation(
            self.book_id,
            "highlight",
            h.start.spine_index as i64,
            "",
            0,
            "",
            0,
            color.as_str(),
            style.as_str(),
            &h.text,
            "",
        );
        match inserted {
            Ok(id) => {
                crate::notify::report(
                    self.service.catalog().update_annotation_cfi(id, &cfi),
                    "Could not place the highlight",
                );
                view.show_highlight(id, &h);
                self.reload_annotations();
                Some(id)
            }
            Err(e) => {
                crate::notify::error("Could not save the highlight", &e.to_string());
                None
            }
        }
    }

    /// Raise the annotation popover (decision 29) over `rect`.
    /// `focus_note` is the chip's Note path: the reader asked for
    /// the note box by name, so the typing starts there.
    fn open_annotation_popover(
        &mut self,
        anno: &crate::db::Annotation,
        rect: gtk::gdk::Rectangle,
        focus_note: bool,
        sender: &ComponentSender<Self>,
    ) {
        let Some(view) = &self.view else { return };
        let popover = engine::build_annotation_edit_popover(
            view.widget().upcast_ref(),
            &rect,
            anno,
            focus_note,
            sender,
        );
        popover.popup();
        self.annotation_popover = Some(popover);
    }

    /// Show `word`'s entry at the standing anchor without touching the
    /// back stack — the shared tail of chip navigation and DictGoBack.
    fn dict_show_word(&mut self, word: &str, sender: &ComponentSender<Self>) {
        let data = self
            .service
            .catalog()
            .lookup_entry(word)
            .unwrap_or_else(|_| crate::db::EntryData {
                word: word.to_string(),
                ..Default::default()
            });
        self.dict_lookup_word = Some(data.word.clone());
        self.dict_lookup_def = data
            .senses
            .first()
            .map(|s| s.def.clone())
            .or_else(|| (!data.suggestions.is_empty()).then(|| data.word.clone()));
        let saved = self
            .service
            .catalog()
            .saved_word_exists(&data.word, self.book_id)
            .unwrap_or(false);
        let rect = self.dict_anchor_rect();
        self.show_dict(&data, saved, rect, sender);
    }

    /// The dictionary lookup every word-tap and chip-lookup goes through:
    /// find the entry, log it, and show the popover. A fresh lookup from
    /// the page, so the in-popup way back starts empty. `sentence` is
    /// `None` when the lookup came from the Words sidebar instead of
    /// from a tap.
    fn dict_lookup(
        &mut self,
        word: String,
        sentence: Option<String>,
        rect: gtk::gdk::Rectangle,
        sender: &ComponentSender<Self>,
    ) {
        if word.trim().is_empty() {
            return;
        }
        self.dict_context = sentence.clone();
        self.dict_anchor = Some(rect);
        self.dict_back_stack.clear();
        let data = self
            .service
            .catalog()
            .lookup_entry(&word)
            .unwrap_or_else(|_| crate::db::EntryData {
                word: word.clone(),
                ..Default::default()
            });
        self.dict_lookup_word = Some(data.word.clone());
        self.dict_lookup_def = data
            .senses
            .first()
            .map(|s| s.def.clone())
            .or_else(|| (!data.suggestions.is_empty()).then(|| data.word.clone()));
        let _ = self.service.catalog().log_dict_lookup(
            &word,
            Some(self.book_id),
            Some(self.chapter as i64),
            sentence.as_deref(),
            !data.senses.is_empty(),
        );
        let saved = self
            .service
            .catalog()
            .saved_word_exists(&data.word, self.book_id)
            .unwrap_or(false);
        self.show_dict(&data, saved, rect, sender);
    }

    /// Where to point a popover when the message did not carry a spot on
    /// the page: the last place the engine reported, or the middle of the
    /// reading area.
    fn dict_anchor_rect(&self) -> gtk::gdk::Rectangle {
        self.dict_anchor.unwrap_or_else(|| {
            let (w, h) = self
                .view
                .as_ref()
                .map(|v| (v.widget().width(), v.widget().height()))
                .unwrap_or((0, 0));
            gtk::gdk::Rectangle::new(w / 2, h / 2, 1, 1)
        })
    }

    fn highlight_current_search_match(&self) {
        self.paint_search_matches(true);
    }

    /// Paint every match the standing scope can see (decision 34):
    /// the rest in the faint neutral wash, the current one in the
    /// accent inside its ring — the engine's transient search layer,
    /// never a stored highlight. `navigate` also takes the reader
    /// to the current match; a repaint after a manual page turn
    /// must not.
    fn paint_search_matches(&self, navigate: bool) {
        let Some(view) = &self.view else { return };
        view.clear_search_matches();
        let query_trimmed = self.search_query.trim();
        if !self.search_active || query_trimmed.is_empty() {
            return;
        }
        // The standing position counts inside the Chapter / Book
        // scope; the raw list is the engine's whole-book answer
        // underneath it.
        let scoped = self.scoped_search_indices();
        if scoped.is_empty() {
            return;
        }
        let current_pos = self.search_index.min(scoped.len() - 1);
        let query_len = query_trimmed.chars().count() as u32;
        let matches = scoped.iter().enumerate().map(|(pos, global)| {
            let res = &self.search_results[*global];
            let end_locator = kalam_reader::LayeredLocator {
                char_offset: res.locator.char_offset + query_len,
                ..res.locator.clone()
            };
            (res.locator.clone(), end_locator, pos == current_pos)
        });
        view.set_search_matches(matches);
        if navigate {
            let res = &self.search_results[scoped[current_pos]];
            view.goto_locator(&res.locator, true);
        }
<<<<<<< Updated upstream
=======

        let total = self.search_results.len();
        if total == 0 {
            badge_lbl.set_label("0 matches");
            let empty_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
            empty_box.set_margin_all(20);
            empty_box.set_halign(gtk::Align::Center);

            let empty_lbl = gtk::Label::new(Some(if self.search_query.trim().is_empty() {
                "Type to search across the entire book"
            } else {
                "No matches found"
            }));
            empty_lbl.add_css_class("kalam-search-popover-badge");
            empty_box.append(&empty_lbl);
            list_box.append(&empty_box);
            return;
        }

        badge_lbl.set_label(&format!("{} matches", total));

        let display_limit = 100.min(total);
        for idx in 0..display_limit {
            let res = &self.search_results[idx];
            let row = gtk::Box::new(gtk::Orientation::Vertical, 3);
            row.add_css_class("kalam-search-snippet-row");
            if idx == self.search_index {
                row.add_css_class("active");
            }

            let top_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            let ch_title = res.chapter_title.clone().unwrap_or_else(|| format!("Chapter {}", res.chapter + 1));
            let ch_label = gtk::Label::new(Some(&ch_title));
            ch_label.add_css_class("kalam-search-snippet-chapter");
            ch_label.set_hexpand(true);
            ch_label.set_halign(gtk::Align::Start);
            ch_label.set_ellipsize(gtk::pango::EllipsizeMode::End);

            let idx_label = gtk::Label::new(Some(&format!("#{}", idx + 1)));
            idx_label.add_css_class("kalam-search-snippet-index");

            top_row.append(&ch_label);
            top_row.append(&idx_label);

            let text_label = gtk::Label::new(None);
            text_label.add_css_class("kalam-search-snippet-text");
            text_label.set_use_markup(true);
            text_label.set_markup(&format_search_snippet(&res.snippet, &self.search_query));
            text_label.set_wrap(true);
            text_label.set_wrap_mode(gtk::pango::WrapMode::Word);
            text_label.set_halign(gtk::Align::Start);
            text_label.set_xalign(0.0);

            row.append(&top_row);
            row.append(&text_label);

            let gesture = gtk::GestureClick::new();
            let s = sender.clone();
            gesture.connect_released(move |_, _, _, _| {
                s.input(ReaderMsg::JumpToSearchResult(idx));
            });
            row.add_controller(gesture);

            list_box.append(&row);
        }

        if total > display_limit {
            let more_lbl = gtk::Label::new(Some(&format!("+ {} more matches (use Next/Prev)", total - display_limit)));
            more_lbl.add_css_class("kalam-search-snippet-index");
            more_lbl.set_margin_all(8);
            list_box.append(&more_lbl);
        }
>>>>>>> Stashed changes
    }

    fn show_image_lightbox(&mut self, widgets: &mut <Self as relm4::Component>::Widgets, w: u32, h: u32, rgba: &[u8]) {
        let bytes = glib::Bytes::from_owned(rgba.to_vec());
        let pixbuf = gdk_pixbuf::Pixbuf::from_bytes(
            &bytes,
            gdk_pixbuf::Colorspace::Rgb,
            true,
            8,
            w as i32,
            h as i32,
            (w * 4) as i32,
        );
        self.lightbox_pixbuf = Some(pixbuf);
        self.lightbox_zoom = 1.0;
        self.lightbox_rotation = 0.0;
        self.update_lightbox_image(widgets);
        self.lightbox_active = true;
    }

    fn update_lightbox_image(&self, widgets: &mut <Self as relm4::Component>::Widgets) {
        let Some(pixbuf) = &self.lightbox_pixbuf else { return };
        let mut result = pixbuf.clone();
        let angle = ((self.lightbox_rotation % 360.0) + 360.0) % 360.0;
        if (angle - 90.0).abs() < 1.0 {
            if let Some(rotated) = pixbuf.rotate_simple(gdk_pixbuf::PixbufRotation::Clockwise) {
                result = rotated;
            }
        } else if (angle - 180.0).abs() < 1.0 {
            if let Some(rotated) = pixbuf.rotate_simple(gdk_pixbuf::PixbufRotation::Upsidedown) {
                result = rotated;
            }
        } else if (angle - 270.0).abs() < 1.0 {
            if let Some(rotated) = pixbuf.rotate_simple(gdk_pixbuf::PixbufRotation::Counterclockwise) {
                result = rotated;
            }
        }

        let orig_w = result.width();
        let orig_h = result.height();
        let scaled_w = ((orig_w as f64) * self.lightbox_zoom).round() as i32;
        let scaled_h = ((orig_h as f64) * self.lightbox_zoom).round() as i32;

        if scaled_w > 0 && scaled_h > 0 && (scaled_w != orig_w || scaled_h != orig_h) {
            if let Some(scaled) = result.scale_simple(scaled_w, scaled_h, gdk_pixbuf::InterpType::Bilinear) {
                result = scaled;
            }
        }

        let texture = gtk::gdk::Texture::for_pixbuf(&result);
        widgets.lightbox_picture.set_paintable(Some(&texture));
    }
}

/// The first table-of-contents label that points at chapter `spine`,
/// searching through nested entries.
pub(crate) fn toc_title(entries: &[kalam_reader::TocEntry], spine: usize) -> Option<String> {
    for entry in entries {
        if entry.spine_index == Some(spine) && !entry.label.trim().is_empty() {
            return Some(entry.label.trim().to_string());
        }
        if let Some(found) = toc_title(&entry.children, spine) {
            return Some(found);
        }
    }
    None
}

