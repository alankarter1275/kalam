//! Book detail page (P5.5 design).
//!
//! A fixed chrome row (back pill + metadata pencil), then a scrolling page:
//! hero (cover, progress, meta, title, author, rating, tags, actions,
//! description) over a card grid — stats & history, highlights, author,
//! reading journey, book file.
//!
//! The series is not a card: only books in a series show it, and it lives in
//! a float (see `series_float.rs`) opened from the hero's Series row.

use crate::db::{Catalog, ShelfKind};
use crate::models::{Book, BookFormat};
use crate::pages::history::pretty_day;
use crate::pages::metadata_editor::open_metadata_editor;
use crate::service::LibraryService;
use crate::widgets::author_links::replace_author_links;
use crate::widgets::book_row::{cover_widget_deferred, invalidate_cover_cache};
use crate::widgets::charts::star_picker;
use gtk::prelude::*;
use relm4::prelude::*;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Debug)]
pub enum BookPageOut {
    OpenReader,
    OpenAuthor {
        name: String,
    },
    /// Open another book's page (author card thumbnails).
    OpenBook {
        book_id: i64,
    },
    /// Open the series float for a book's series.
    OpenSeries {
        series: String,
        first_author: String,
    },
    /// Open the highlights & quotes panel (in-app float).
    ViewHighlights,
    /// Open the shelves checklist panel (in-app float).
    ShowShelves,
    /// Open the tags panel (in-app float) from the hero's "+" chip.
    ShowTags,
    Deleted {
        #[allow(dead_code)] // carried for the open/delete round-trip; some arms ignore it
        book_id: i64,
    },
}

#[derive(Debug)]
pub enum BookPageMsg {
    Delete,
    /// The single-book delete finished on its worker.
    DeleteDone {
        id: i64,
        title: String,
        res: Result<(), String>,
    },
    ToggleReadingList,
    ToggleFinished,
    SetRating(u8),
    OpenAuthor(String),
    OpenSeries {
        series: String,
        first_author: String,
    },
    EditMetadata,
    ShowShelfMenu,
    Refresh,
    /// The author card's "View page →" — always the first listed author.
    OpenFirstAuthor,
    /// Open the tags panel (in-app float) — tags are managed there.
    ShowTags,
    /// Toggle the journey between 6 chapters and the full list.
    ToggleJourney,
    /// Open the full annotations dialog from the highlights card.
    ViewHighlights,
    /// Remaster comic archive using Lanczos3 upscaler.
    RemasterComic,
}

pub struct BookPageModel {
    service: LibraryService,
    book: Option<Book>,
    in_reading_list: bool,
    finished: bool,
    journey_expanded: bool,
    /// Spine chapter titles, cached per book id — opening the EPUB cache is
    /// cheap once extracted, but there is no reason to repeat it.
    chapter_titles: Vec<String>,
    chapter_titles_for: i64,
}

#[relm4::component(pub)]
impl Component for BookPageModel {
    type Init = (Arc<Catalog>, i64);
    type Input = BookPageMsg;
    type Output = BookPageOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "kalam-bookpage",
            set_hexpand: true,
            set_vexpand: false,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                add_css_class: "kalam-bookpage-inner",

                    // ── hero ────────────────────────────────────────────
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        add_css_class: "kalam-bookpage-hero",
                        set_spacing: 36,

                        // left: cover, progress, meta
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 16,
                            set_width_request: 148,
                            set_valign: gtk::Align::Start,

                            // Plain cover — no 3D page edge.
                            #[name = "cover_host"]
                            gtk::Box {
                                add_css_class: "kalam-cover-face",
                                set_halign: gtk::Align::Start,
                                set_valign: gtk::Align::Start,
                            },

                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 5,
                                set_width_request: 148,

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    #[name = "prog_pct"]
                                    gtk::Label {
                                        add_css_class: "kalam-prog-pct",
                                        set_halign: gtk::Align::Start,
                                        set_hexpand: true,
                                    },
                                    #[name = "prog_loc"]
                                    gtk::Label {
                                        add_css_class: "kalam-prog-loc",
                                        set_halign: gtk::Align::End,
                                    },
                                },
                                #[name = "prog_track"]
                                gtk::Box {
                                    add_css_class: "kalam-prog-track",
                                    #[name = "prog_fill"]
                                    gtk::Box {
                                        add_css_class: "kalam-prog-fill",
                                        set_halign: gtk::Align::Start,
                                        set_valign: gtk::Align::Fill,
                                    },
                                },
                            },

                            #[name = "meta_host"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                add_css_class: "kalam-meta-block",
                                set_spacing: 10,
                            },
                        },

                        // center: identity + actions + description
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_hexpand: true,
                            set_valign: gtk::Align::Start,

                            #[name = "title"]
                            gtk::Label {
                                add_css_class: "kalam-bookpage-title",
                                set_halign: gtk::Align::Start,
                                set_wrap: true,
                                set_xalign: 0.0,
                                set_max_width_chars: 40,
                            },

                            #[name = "author_host"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_halign: gtk::Align::Start,
                                set_margin_top: 2,
                                set_margin_bottom: 10,
                            },

                            #[name = "rating_host"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                add_css_class: "kalam-rating-badge",
                                set_spacing: 8,
                                set_halign: gtk::Align::Start,
                            },

                            #[name = "tags_flow"]
                            gtk::FlowBox {
                                add_css_class: "kalam-tags",
                                set_halign: gtk::Align::Start,
                                set_selection_mode: gtk::SelectionMode::None,
                                set_column_spacing: 6,
                                set_row_spacing: 6,
                                set_margin_top: 8,
                                set_margin_bottom: 12,
                            },

                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                add_css_class: "kalam-actions",
                                set_spacing: 8,

                                gtk::Button {
                                    add_css_class: "kalam-btn-read",
                                    set_focus_on_click: false,
                                    set_child: Some(&crate::icons::labelled(
                                        "media-playback-start-symbolic",
                                        14,
                                        "Read",
                                        6,
                                    )),
                                    connect_clicked[sender] => move |_| {
                                        sender.output(BookPageOut::OpenReader).ok();
                                    },
                                },

                                gtk::Button {
                                    add_css_class: "kalam-icon-btn",
                                    set_focus_on_click: false,
                                    set_tooltip_text: Some("Edit metadata"),
                                    set_child: Some(&crate::icons::symbolic(
                                        "document-edit-symbolic",
                                        16,
                                    )),
                                    connect_clicked => BookPageMsg::EditMetadata,
                                },

                                #[name = "tbr_btn"]
                                gtk::Button {
                                    add_css_class: "kalam-icon-btn",
                                    set_focus_on_click: false,
                                    set_tooltip_text: Some("Reading list"),
                                    set_child: Some(&crate::icons::symbolic(
                                        "bookmark-new-symbolic",
                                        16,
                                    )),
                                    connect_clicked => BookPageMsg::ToggleReadingList,
                                },

                                #[name = "shelf_btn"]
                                gtk::Button {
                                    add_css_class: "kalam-icon-btn",
                                    set_focus_on_click: false,
                                    set_tooltip_text: Some("Shelves"),
                                    set_child: Some(&crate::icons::symbolic(
                                        "view-grid-symbolic",
                                        16,
                                    )),
                                    connect_clicked => BookPageMsg::ShowShelfMenu,
                                },

                                #[name = "finish_btn"]
                                gtk::Button {
                                    add_css_class: "kalam-icon-btn",
                                    set_focus_on_click: false,
                                    set_child: Some(&crate::icons::symbolic(
                                        "object-select-symbolic",
                                        16,
                                    )),
                                    connect_clicked => BookPageMsg::ToggleFinished,
                                },

                                #[name = "remaster_btn"]
                                gtk::Button {
                                    add_css_class: "kalam-icon-btn",
                                    set_focus_on_click: false,
                                    set_tooltip_text: Some("Remaster Comic (Lanczos3 upscaler)"),
                                    set_child: Some(&crate::icons::symbolic(
                                        "zoom-in-symbolic",
                                        16,
                                    )),
                                    connect_clicked => BookPageMsg::RemasterComic,
                                },

                                gtk::Button {
                                    add_css_class: "kalam-icon-btn",
                                    add_css_class: "kalam-icon-btn-danger",
                                    set_focus_on_click: false,
                                    set_tooltip_text: Some("Remove book"),
                                    set_child: Some(&crate::icons::symbolic(
                                        "user-trash-symbolic",
                                        16,
                                    )),
                                    connect_clicked => BookPageMsg::Delete,
                                },
                            },

                            gtk::Label {
                                set_label: "Description",
                                add_css_class: "kalam-section-label",
                                set_halign: gtk::Align::Start,
                                set_margin_top: 18,
                            },
                            #[name = "description"]
                            gtk::Label {
                                add_css_class: "kalam-desc-serif",
                                set_halign: gtk::Align::Start,
                                set_wrap: true,
                                set_xalign: 0.0,
                                set_max_width_chars: 88,
                                set_margin_top: 6,
                            },
                        },

                        // Reading journey
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            add_css_class: "kalam-detail-card",
                            set_width_request: 300,
                            set_valign: gtk::Align::Start,

                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 7,
                                gtk::Image {
                                    add_css_class: "kalam-detail-card-icon",
                                    set_icon_name: Some("view-list-symbolic"),
                                    set_pixel_size: 15,
                                },
                                gtk::Label {
                                    set_label: "Reading journey",
                                    add_css_class: "kalam-detail-card-title",
                                },
                            },

                            #[name = "journey_host"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_margin_top: 8,
                            },
                            #[name = "journey_more"]
                            gtk::Button {
                                add_css_class: "kalam-journey-more",
                                set_focus_on_click: false,
                                set_halign: gtk::Align::Start,
                                connect_clicked => BookPageMsg::ToggleJourney,
                            },
                        },

                    },

                    // ── row 1: stats & history | highlights ────────────
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        add_css_class: "kalam-card-row",
                        set_spacing: 14,

                        // Reading stats & history
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            add_css_class: "kalam-detail-card",
                            set_hexpand: true,
                            set_valign: gtk::Align::Start,

                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 7,
                                gtk::Image {
                                    add_css_class: "kalam-detail-card-icon",
                                    set_icon_name: Some("view-statistics-chart-symbolic"),
                                    set_pixel_size: 15,
                                },
                                gtk::Label {
                                    set_label: "Reading stats & history",
                                    add_css_class: "kalam-detail-card-title",
                                },
                            },

                            #[name = "stat_host"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 8,
                                set_margin_top: 12,
                            },

                            gtk::Label {
                                set_label: "Last 7 days",
                                add_css_class: "kalam-section-label",
                                set_halign: gtk::Align::Start,
                                set_margin_top: 14,
                            },
                            #[name = "bars_host"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                add_css_class: "kalam-bars",
                                set_spacing: 4,
                                set_margin_top: 8,
                            },
                            #[name = "bar_labels_host"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_margin_top: 4,
                            },

                            gtk::Box {
                                add_css_class: "kalam-divider",
                                set_margin_top: 14,
                            },

                            #[name = "timeline_host"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_margin_top: 12,
                            },
                        },

                        // Highlights & quotes
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            add_css_class: "kalam-detail-card",
                            set_hexpand: true,
                            set_valign: gtk::Align::Start,

                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 7,
                                gtk::Image {
                                    add_css_class: "kalam-detail-card-icon",
                                    set_icon_name: Some("text-x-generic-symbolic"),
                                    set_pixel_size: 15,
                                },
                                gtk::Label {
                                    set_label: "Highlights & quotes",
                                    add_css_class: "kalam-detail-card-title",
                                    set_hexpand: true,
                                    set_halign: gtk::Align::Start,
                                },
                                gtk::Button {
                                    set_label: "View all",
                                    add_css_class: "kalam-detail-card-link",
                                    set_focus_on_click: false,
                                    connect_clicked => BookPageMsg::ViewHighlights,
                                },
                            },

                            #[name = "highlights_host"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_margin_top: 8,
                            },
                        },

                    },

                    // ── row 2: author | file ────────────────────────────
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        add_css_class: "kalam-card-row",
                        set_spacing: 14,

                        // Author
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            add_css_class: "kalam-detail-card",
                            set_hexpand: true,
                            set_valign: gtk::Align::Start,

                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 7,
                                gtk::Image {
                                    add_css_class: "kalam-detail-card-icon",
                                    set_icon_name: Some("system-users-symbolic"),
                                    set_pixel_size: 15,
                                },
                                gtk::Label {
                                    set_label: "Author",
                                    add_css_class: "kalam-detail-card-title",
                                    set_hexpand: true,
                                    set_halign: gtk::Align::Start,
                                },
                                gtk::Button {
                                    set_label: "View page →",
                                    add_css_class: "kalam-detail-card-link",
                                    set_focus_on_click: false,
                                    connect_clicked => BookPageMsg::OpenFirstAuthor,
                                },
                            },

                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 11,
                                set_margin_top: 12,

                                #[name = "author_avatar_host"]
                                gtk::Box {
                                    add_css_class: "kalam-author-avatar",
                                    set_valign: gtk::Align::Start,
                                },

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_hexpand: true,
                                    set_valign: gtk::Align::Center,
                                    #[name = "author_name"]
                                    gtk::Label {
                                        add_css_class: "kalam-author-name",
                                        set_halign: gtk::Align::Start,
                                        set_hexpand: true,
                                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                                    },
                                    #[name = "author_sub"]
                                    gtk::Label {
                                        add_css_class: "kalam-author-sub",
                                        set_halign: gtk::Align::Start,
                                        set_hexpand: true,
                                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                                    },
                                },
                            },

                            #[name = "author_bio"]
                            gtk::Label {
                                add_css_class: "kalam-author-bio",
                                set_halign: gtk::Align::Start,
                                set_wrap: true,
                                set_xalign: 0.0,
                                set_max_width_chars: 34,
                                set_margin_top: 10,
                            },

                            gtk::Label {
                                set_label: "Also in your library",
                                add_css_class: "kalam-section-label",
                                set_halign: gtk::Align::Start,
                                set_margin_top: 12,
                            },
                            #[name = "author_books_host"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 8,
                                set_margin_top: 8,
                            },
                        },

                        // Book file
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            add_css_class: "kalam-detail-card",
                            set_hexpand: true,
                            set_valign: gtk::Align::Start,

                            gtk::Box {
                                set_orientation: gtk::Orientation::Horizontal,
                                set_spacing: 7,
                                gtk::Image {
                                    add_css_class: "kalam-detail-card-icon",
                                    set_icon_name: Some("x-office-document-symbolic"),
                                    set_pixel_size: 15,
                                },
                                gtk::Label {
                                    set_label: "Book file",
                                    add_css_class: "kalam-detail-card-title",
                                },
                            },

                            #[name = "file_name"]
                            gtk::Label {
                                add_css_class: "kalam-file-name",
                                set_halign: gtk::Align::Start,
                                set_hexpand: true,
                                set_ellipsize: gtk::pango::EllipsizeMode::Middle,
                                set_selectable: true,
                                set_margin_top: 12,
                            },

                            #[name = "file_rows_host"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_margin_top: 10,
                            },

                            #[name = "file_open_btn"]
                            gtk::Button {
                                set_label: "Show in file manager",
                                add_css_class: "kalam-secondary-btn",
                                set_focus_on_click: false,
                                set_halign: gtk::Align::Start,
                                set_margin_top: 14,
                            },
                        },
                    },
                },
        }
    }

    fn init(
        (catalog, book_id): Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let service = LibraryService::new(catalog);
        let snap = service.book_detail(book_id);
        report_errors(&snap.errors);
        let mut model = BookPageModel {
            in_reading_list: snap.in_reading_list,
            finished: snap.finished,
            journey_expanded: false,
            chapter_titles: Vec::new(),
            chapter_titles_for: 0,
            service,
            book: snap.book,
        };
        let widgets = view_output!();

        // Wired once, not in rebuild (which runs on every message): the file
        // path is fixed for the page's lifetime, so stacking handlers would
        // open N file managers after N messages.
        if let Some(book) = &model.book {
            let path = book.file_path.clone();
            widgets.file_open_btn.connect_clicked(move |_| {
                open_in_file_manager(&path);
            });
        }

        model.reload_state(book_id);
        model.rebuild(&widgets, &sender);
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
            BookPageMsg::Delete => {
                if let Some(book) = &self.book {
                    let id = book.id;
                    let title = book.title.clone();
                    // Drop the cached texture on the main thread (it is a GTK
                    // texture cache) before the worker removes the book.
                    if let Some(path) = &book.cover_path {
                        invalidate_cover_cache(path);
                    }
                    let catalog = self.service.catalog().clone();
                    let s = sender.input_sender().clone();
                    // A delete is a task like any other operation, even though
                    // one book is far too fast to cancel: it belongs in the
                    // history, and the page must not block on removing the
                    // book's directory.
                    crate::tasks::spawn(
                        format!("Deleting {title}"),
                        move |_reporter| catalog.delete_book(id).map_err(|e| e.to_string()),
                        |_| {},
                        move |res| {
                            s.send(BookPageMsg::DeleteDone { id, title, res }).ok();
                        },
                    );
                }
            }
            BookPageMsg::DeleteDone { id, title, res } => match res {
                Ok(()) => {
                    crate::notify::success("Book removed", &title);
                    self.book = None;
                    sender.output(BookPageOut::Deleted { book_id: id }).ok();
                }
                // Silently doing nothing was the worst outcome here: the book
                // stayed and no reason was given.
                Err(err) => crate::notify::error("Could not remove the book", &err),
            },
            BookPageMsg::ToggleReadingList => {
                if let Some(book) = &self.book {
                    let id = book.id;
                    let title = book.title.clone();
                    if self.in_reading_list {
                        if crate::notify::report(
                            self.service.catalog().remove_from_reading_list(id),
                            "Could not update the reading list",
                        ) {
                            crate::notify::info("Removed from reading list", &title);
                        }
                    } else if crate::notify::report(
                        self.service.catalog().add_to_reading_list(id),
                        "Could not update the reading list",
                    ) {
                        crate::notify::success("Added to reading list", &title);
                    }
                    self.reload_state(id);
                }
            }
            BookPageMsg::SetRating(half_stars) => {
                if let Some(book) = &self.book {
                    let id = book.id;
                    let stars = half_stars as f32 / 2.0;
                    let detail = if half_stars == 0 {
                        "Rating cleared".to_string()
                    } else {
                        format!("{stars} of 5 \u{2605}")
                    };
                    crate::notify::outcome(
                        self.service.catalog().set_book_rating(id, half_stars),
                        "Rating saved",
                        &detail,
                        "Could not save the rating",
                    );
                    self.reload_state(id);
                }
            }
            BookPageMsg::OpenAuthor(name) => {
                sender.output(BookPageOut::OpenAuthor { name }).ok();
            }
            BookPageMsg::OpenSeries {
                series,
                first_author,
            } => {
                sender
                    .output(BookPageOut::OpenSeries {
                        series,
                        first_author,
                    })
                    .ok();
            }
            BookPageMsg::OpenFirstAuthor => {
                if let Some(book) = &self.book {
                    let name = book
                        .authors
                        .split(',')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string();
                    if !name.is_empty() {
                        sender.output(BookPageOut::OpenAuthor { name }).ok();
                    }
                }
            }
            BookPageMsg::ToggleFinished => {
                if let Some(book) = &self.book {
                    let id = book.id;
                    let becoming = !self.finished;
                    let title = book.title.clone();
                    if crate::notify::report(
                        self.service.catalog().set_book_finished(id, becoming),
                        "Could not update the book",
                    ) {
                        if becoming {
                            crate::notify::success("Marked as finished", &title);
                        } else {
                            crate::notify::info("Marked as unread", &title);
                        }
                    }
                    self.reload_state(id);
                }
            }
            BookPageMsg::EditMetadata => {
                if let Some(book) = &self.book {
                    let id = book.id;
                    let s = sender.clone();
                    open_metadata_editor(root, self.service.catalog().clone(), id, move || {
                        s.input(BookPageMsg::Refresh)
                    });
                }
            }
            BookPageMsg::ShowShelfMenu => {
                sender.output(BookPageOut::ShowShelves).ok();
            }
            BookPageMsg::ShowTags => {
                sender.output(BookPageOut::ShowTags).ok();
            }
            BookPageMsg::ToggleJourney => {
                self.journey_expanded = !self.journey_expanded;
            }
            BookPageMsg::ViewHighlights => {
                sender.output(BookPageOut::ViewHighlights).ok();
            }
            BookPageMsg::Refresh => {
                if let Some(id) = self.book.as_ref().map(|b| b.id) {
                    self.reload_state(id);
                }
            }
            BookPageMsg::RemasterComic => {
                if let Some(book) = &self.book {
                    if matches!(book.format, BookFormat::Cbz | BookFormat::Cbr) {
                        let path = book.file_path.clone();
                        let title = book.title.clone();
                        let s = sender.clone();
                        crate::notify::info("Remastering comic...", &format!("Rescaling {} with Lanczos3 filter", title));
                        crate::tasks::spawn(
                            format!("Remastering {title}"),
                            move |reporter| -> anyhow::Result<()> {
                                let tmp = path.with_extension("remastered.cbz");
                                crate::comics::remaster_comic_cbz(&path, &tmp, 2.0, |done, total| {
                                    reporter.step(done, total, format!("Page {done}/{total}"));
                                })?;
                                std::fs::rename(&tmp, &path)?;
                                Ok(())
                            },
                            |_| {},
                            move |res| {
                                match res {
                                    Ok(()) => {
                                        crate::notify::success("Comic Remastered", &format!("Successfully remastered {}", title));
                                        s.input(BookPageMsg::Refresh);
                                    }
                                    Err(err) => {
                                        crate::notify::error("Remaster failed", &err.to_string());
                                    }
                                }
                            },
                        );
                    } else {
                        crate::notify::info("Not a comic", "Remastering is only available for CBZ comic archives");
                    }
                }
            }
        }
        self.rebuild(widgets, &sender);
    }
}

/// Surface read failures. Without this a database problem looked like a
/// book that had been deleted.
fn report_errors(errors: &[String]) {
    for err in errors {
        crate::notify::error("Could not read this book", err);
    }
}

impl BookPageModel {
    /// Re-read the book row and the mutable state the widgets must reflect.
    ///
    /// This used to be three separate swallowed reads spread over four call
    /// sites, two of which re-read the book row themselves first.
    fn reload_state(&mut self, book_id: i64) {
        let snap = self.service.book_detail(book_id);
        report_errors(&snap.errors);
        self.book = snap.book;
        self.in_reading_list = snap.in_reading_list;
        self.finished = snap.finished;
    }

    /// Spine titles for the journey + progress location. Cached per book;
    /// returned owned so callers can keep using `self` afterwards (a
    /// `&mut self`-rooted reference would pin the page's whole borrow).
    fn chapter_titles(&mut self) -> Vec<String> {
        if let Some(book) = &self.book {
            if self.chapter_titles_for != book.id || self.chapter_titles.is_empty() {
                self.chapter_titles = if book.format == BookFormat::Epub {
                    crate::epub_book::OpenBook::open(
                        &book.file_path,
                        &crate::paths::reader_cache_dir(&book.uuid),
                    )
                    .map(|ob| ob.spine.iter().map(|s| s.title.clone()).collect())
                    .unwrap_or_default()
                } else {
                    Vec::new()
                };
                self.chapter_titles_for = book.id;
            }
        }
        self.chapter_titles.clone()
    }

    /// Refill every host from model state. Called from init and after every
    /// message — the page is small enough that this is cheaper than wiring
    /// per-widget invalidation, and it can't drift out of sync.
    fn rebuild(&mut self, widgets: &BookPageModelWidgets, sender: &ComponentSender<Self>) {
        // Hero.
        let chapters = self.chapter_titles();
        fill_cover(&widgets.cover_host, self.book.as_ref());
        fill_meta(&widgets.meta_host, self.book.as_ref(), sender);
        fill_progress(
            &widgets.prog_pct,
            &widgets.prog_loc,
            &widgets.prog_fill,
            self.service.catalog(),
            self.book.as_ref(),
            &chapters,
        );

        if let Some(book) = &self.book {
            widgets.title.set_label(&book.title);
            let tx = sender.input_sender().clone();
            replace_author_links(
                &widgets.author_host,
                book.authors_display(),
                "kalam-author-link-page",
                Rc::new(move |name| {
                    let _ = tx.send(BookPageMsg::OpenAuthor(name));
                }),
            );
            fill_rating(&widgets.rating_host, book, sender);
            fill_tags(&widgets.tags_flow, book, sender);
            fill_description(&widgets.description, book);
            // The upscaler rewrites a CBZ in place, so it only exists for
            // comic archives. On an EPUB or PDF there was nothing for it to
            // do: the button was drawn unconditionally and clicking it could
            // only produce a "Not a comic" toast. Hide it instead.
            widgets.remaster_btn.set_visible(matches!(
                book.format,
                crate::models::BookFormat::Cbz | crate::models::BookFormat::Cbr
            ));
        } else {
            widgets.title.set_label("Book not found");
            widgets.remaster_btn.set_visible(false);
            widgets
                .description
                .set_label("This book was removed or does not exist.");
            while let Some(c) = widgets.author_host.first_child() {
                widgets.author_host.remove(&c);
            }
            while let Some(c) = widgets.rating_host.first_child() {
                widgets.rating_host.remove(&c);
            }
            while let Some(c) = widgets.tags_flow.first_child() {
                widgets.tags_flow.remove(&c);
            }
        }

        // Action button states.
        let finish = &widgets.finish_btn;
        finish.set_tooltip_text(Some(if self.finished {
            "Mark unread"
        } else {
            "Mark finished"
        }));
        widgets
            .tbr_btn
            .set_tooltip_text(Some(if self.in_reading_list {
                "In reading list — remove"
            } else {
                "Add to reading list"
            }));

        // Cards.
        fill_stats_card(widgets, self, &chapters);
        fill_highlights_card(&widgets.highlights_host, self, &chapters);
        fill_author_card(widgets, self.book.as_ref(), self.service.catalog(), sender);
        fill_journey_card(widgets, self, &chapters);
        fill_file_card(widgets, self.book.as_ref());
    }
}

// ---------------------------------------------------------------------------
// Hero
// ---------------------------------------------------------------------------

/// The hero cover, fixed at 148×214.
const COVER_W: i32 = 148;
const COVER_H: i32 = 214;

fn fill_cover(host: &gtk::Box, book: Option<&Book>) {
    host.set_size_request(COVER_W, COVER_H);
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    let path = book.and_then(|b| b.cover_path.as_deref());
    let cover = cover_widget_deferred(path, COVER_W, COVER_H);
    host.append(&cover);
    if let Some(path) = path {
        crate::preload::warm_covers(vec![path.to_path_buf()], COVER_W, COVER_H);
    }
}

/// A `KEY` over `value` line, used by the hero meta block and the file card.
fn meta_row<V: gtk::prelude::IsA<gtk::Widget>>(key: &str, value: &V) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
    row.add_css_class("kalam-meta-row");
    let k = gtk::Label::new(Some(key));
    k.add_css_class("kalam-meta-key");
    k.set_halign(gtk::Align::Start);
    row.append(&k);
    row.append(value);
    row
}

fn fill_meta(host: &gtk::Box, book: Option<&Book>, sender: &ComponentSender<BookPageModel>) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    let Some(book) = book else {
        return;
    };

    let format_label = gtk::Label::new(Some(book.format.as_str()));
    format_label.add_css_class("kalam-meta-val");
    format_label.set_halign(gtk::Align::Start);
    host.append(&meta_row("Format", &format_label));

    if let Some(series) = book
        .series
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        let label = book.series_display().unwrap_or_else(|| series.to_string());
        let series = series.to_string();
        let first_author = book
            .authors
            .split(',')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        let btn = gtk::Button::new();
        btn.add_css_class("kalam-series-link");
        btn.set_has_frame(false);
        btn.set_focus_on_click(false);
        btn.set_halign(gtk::Align::Start);
        btn.set_tooltip_text(Some("View the full series"));
        btn.set_label(&label);
        let s = sender.clone();
        btn.connect_clicked(move |_| {
            s.input(BookPageMsg::OpenSeries {
                series: series.clone(),
                first_author: first_author.clone(),
            });
        });
        host.append(&meta_row("Series", &btn));
    }

    let publisher = book.publisher.trim();
    let published = book.published.trim();
    if !publisher.is_empty() || !published.is_empty() {
        let value = match (publisher.is_empty(), published.is_empty()) {
            (true, true) => String::new(),
            (true, false) => published.to_string(),
            (false, true) => publisher.to_string(),
            (false, false) => format!("{publisher} · {published}"),
        };
        let l = gtk::Label::new(Some(&value));
        l.add_css_class("kalam-meta-val");
        l.set_halign(gtk::Align::Start);
        l.set_wrap(true);
        l.set_xalign(0.0);
        host.append(&meta_row("Publisher · Year", &l));
    }
}

fn fill_progress(
    pct: &gtk::Label,
    loc: &gtk::Label,
    fill: &gtk::Box,
    catalog: &Catalog,
    book: Option<&Book>,
    chapters: &[String],
) {
    let Some(book) = book else {
        pct.set_label("");
        loc.set_label("");
        fill.set_width_request(0);
        return;
    };
    pct.set_label(&format!("{}%", book.progress));
    if chapters.is_empty() {
        loc.set_label("complete");
    } else {
        let chapter = catalog
            .get_reading_progress(book.id)
            .ok()
            .flatten()
            .map(|(ci, _)| ci)
            .unwrap_or(0)
            .min(chapters.len() - 1);
        loc.set_label(&format!("Ch. {} / {}", chapter + 1, chapters.len()));
    }
    // Fill width is a fraction of the cover-width track; at 0% it is 0.
    let w = if book.progress == 0 {
        0
    } else {
        (COVER_W * book.progress as i32 / 100).max(3)
    };
    fill.set_width_request(w);
}

fn fill_rating(host: &gtk::Box, book: &Book, sender: &ComponentSender<BookPageModel>) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    let s = sender.clone();
    host.append(&star_picker(book.rating, move |v| {
        s.input(BookPageMsg::SetRating(v))
    }));
    let text = match book.rating_stars() {
        Some(v) => format!("{v:.1} / 5 — click again to clear"),
        None => "Not rated yet".to_string(),
    };
    let hint = gtk::Label::new(Some(&text));
    hint.add_css_class("kalam-muted");
    hint.set_valign(gtk::Align::Center);
    host.append(&hint);
}

fn fill_tags(flow: &gtk::FlowBox, book: &Book, sender: &ComponentSender<BookPageModel>) {
    while let Some(child) = flow.first_child() {
        flow.remove(&child);
    }
    for tag in &book.tags {
        // Display-only: management (add/remove) lives in the tags panel.
        let btn = gtk::Button::new();
        btn.add_css_class("kalam-tag-chip");
        btn.set_has_frame(false);
        btn.set_focus_on_click(false);
        btn.set_can_target(false);
        btn.set_label(tag);
        flow.append(&btn);
    }
    let add = gtk::Button::new();
    add.add_css_class("kalam-tag-add");
    add.set_has_frame(false);
    add.set_focus_on_click(false);
    // Plain text plus — theme icons render inconsistently across icon sets.
    let plus = gtk::Label::new(Some("+"));
    plus.add_css_class("kalam-tag-plus");
    add.set_child(Some(&plus));
    add.set_tooltip_text(Some("Manage tags"));
    let s = sender.clone();
    add.connect_clicked(move |_| s.input(BookPageMsg::ShowTags));
    flow.append(&add);
}

fn fill_description(label: &gtk::Label, book: &Book) {
    let desc = crate::epub::strip_html(&book.description);
    if desc.trim().is_empty() {
        label.set_label("No description.");
    } else {
        label.set_label(&desc);
    }
}

// ---------------------------------------------------------------------------
// Cards
// ---------------------------------------------------------------------------

fn format_duration(total_secs: i64) -> String {
    if total_secs <= 0 {
        return "—".into();
    }
    let m = total_secs / 60;
    if m < 60 {
        format!("{m}m")
    } else {
        format!("{}h {:02}m", m / 60, m % 60)
    }
}

fn stat_tile(host: &gtk::Box, value: &str, label: &str) {
    let tile = gtk::Box::new(gtk::Orientation::Vertical, 2);
    tile.add_css_class("kalam-stat-tile");
    tile.set_hexpand(true);
    let v = gtk::Label::new(Some(value));
    v.add_css_class("kalam-stat-val");
    v.set_halign(gtk::Align::Start);
    v.set_ellipsize(gtk::pango::EllipsizeMode::End);
    tile.append(&v);
    let l = gtk::Label::new(Some(label));
    l.add_css_class("kalam-stat-label");
    l.set_halign(gtk::Align::Start);
    l.set_ellipsize(gtk::pango::EllipsizeMode::End);
    tile.append(&l);
    host.append(&tile);
}

fn fill_stats_card(widgets: &BookPageModelWidgets, model: &BookPageModel, chapters: &[String]) {
    let host = &widgets.stat_host;
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }

    // One read for the whole panel. These six queries used to be scattered
    // through the function and every one of them was swallowed, so a broken
    // database drew a page saying you had never read this book.
    let stats = model
        .book
        .as_ref()
        .map(|b| model.service.book_stats(b.id, 7, 3))
        .unwrap_or_default();
    report_errors(&stats.errors);

    let (total_secs, sessions, est, pace) = if let Some(book) = &model.book {
        let total = stats.total_seconds;
        let sessions = stats.session_count;

        let est: String = if book.progress >= 100 || model.finished {
            "Done".into()
        } else if book.progress > 0 && total > 0 {
            let secs_left =
                total.saturating_mul(100 - book.progress as i64) / book.progress.max(1) as i64;
            format!("≈ {}", format_duration(secs_left))
        } else {
            "—".into()
        };

        let chapters_done = {
            let ci = stats.chapter_index;
            if book.progress >= 100 {
                chapters.len().max(ci)
            } else {
                ci
            }
        };
        let pace: String = if chapters_done > 0 && total > 0 {
            format_duration(total / chapters_done as i64)
        } else {
            "—".into()
        };
        (total, sessions, est, pace)
    } else {
        (0, 0, "—".into(), "—".into())
    };

    stat_tile(host, &format_duration(total_secs), "Time reading");
    stat_tile(host, &est, "Est. to finish");
    stat_tile(host, &pace, "Per chapter");
    stat_tile(host, &sessions.to_string(), "Sessions");

    // 7-day bars.
    let by_day = model
        .book
        .as_ref()
        .map(|_| stats.seconds_by_day.clone())
        .unwrap_or_default();
    let bars = &widgets.bars_host;
    while let Some(child) = bars.first_child() {
        bars.remove(&child);
    }
    let labels = &widgets.bar_labels_host;
    while let Some(child) = labels.first_child() {
        labels.remove(&child);
    }
    if by_day.is_empty() {
        let none = gtk::Label::new(Some("No reading yet."));
        none.add_css_class("kalam-muted");
        bars.append(&none);
    } else {
        let max = by_day.iter().map(|(_, s)| *s).max().unwrap_or(0).max(1);
        for (i, (day, secs)) in by_day.iter().enumerate() {
            let bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
            bar.add_css_class("kalam-bar");
            if *secs == 0 {
                bar.add_css_class("kalam-bar-zero");
            }
            if i + 1 == by_day.len() {
                bar.add_css_class("kalam-bar-today");
            }
            bar.set_hexpand(true);
            bar.set_valign(gtk::Align::End);
            bar.set_tooltip_text(Some(&format!(
                "{}: {}",
                pretty_day(day),
                format_duration(*secs)
            )));
            let h = (40 * *secs as i32 / max as i32).max(2);
            bar.set_size_request(-1, h);
            bars.append(&bar);

            let day_num = day.get(8..10).unwrap_or("");
            let l = gtk::Label::new(Some(day_num));
            l.add_css_class("kalam-bar-label");
            l.set_hexpand(true);
            l.set_justify(gtk::Justification::Center);
            labels.append(&l);
        }
    }

    // Timeline: recent sessions + first opened + finished, newest first.
    let timeline = &widgets.timeline_host;
    while let Some(child) = timeline.first_child() {
        timeline.remove(&child);
    }
    if model.book.is_some() {
        let mut items: Vec<(String, &str, &str, Option<i64>)> = Vec::new();
        for s in &stats.recent_sessions {
            items.push((
                s.started_at.clone(),
                "Reading session",
                "kalam-tl-dot",
                Some(s.seconds),
            ));
        }
        if let Some(finished_at) = &stats.finished_at {
            items.push((
                finished_at.clone(),
                "Finished",
                "kalam-tl-dot-success",
                None,
            ));
        }
        if let Some(first) = &stats.first_opened {
            items.push((first.clone(), "First opened", "kalam-tl-dot-dim", None));
        }
        items.sort_by(|a, b| b.0.cmp(&a.0));

        if items.is_empty() {
            let none = gtk::Label::new(Some("No reading history yet."));
            none.add_css_class("kalam-muted");
            timeline.append(&none);
        } else {
            for (i, (at, event, dot_class, secs)) in items.iter().take(4).enumerate() {
                timeline.append(&build_timeline_row(
                    at,
                    event,
                    dot_class,
                    *secs,
                    i + 1 == items.len().min(4),
                ));
            }
        }
    }
}

fn build_timeline_row(
    at: &str,
    event: &str,
    dot_class: &str,
    secs: Option<i64>,
    last: bool,
) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.add_css_class("kalam-tl-item");

    let dot_col = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let dot = gtk::Box::new(gtk::Orientation::Vertical, 0);
    dot.add_css_class("kalam-tl-dot");
    dot.add_css_class(dot_class);
    dot_col.append(&dot);
    if !last {
        let line = gtk::Box::new(gtk::Orientation::Vertical, 0);
        line.add_css_class("kalam-tl-line");
        line.set_vexpand(true);
        dot_col.append(&line);
    }
    row.append(&dot_col);

    let info = gtk::Box::new(gtk::Orientation::Vertical, 1);
    info.set_hexpand(true);
    let e = gtk::Label::new(Some(event));
    e.add_css_class("kalam-tl-event");
    e.set_halign(gtk::Align::Start);
    info.append(&e);
    let when = if at.len() >= 16 {
        format!("{}, {}", pretty_day(&at[..10]), &at[11..16])
    } else {
        at.to_string()
    };
    let w = gtk::Label::new(Some(&when));
    w.add_css_class("kalam-tl-when");
    w.set_halign(gtk::Align::Start);
    info.append(&w);
    row.append(&info);

    if let Some(secs) = secs {
        let d = gtk::Label::new(Some(&format_duration(secs)));
        d.add_css_class("kalam-tl-dur");
        d.set_valign(gtk::Align::Center);
        row.append(&d);
    }
    row
}

/// Map a stored highlight color to its CSS class.
fn hl_color_class(color: &str) -> &'static str {
    match color.to_ascii_lowercase().as_str() {
        "green" => "kalam-hl-bar-green",
        "blue" => "kalam-hl-bar-blue",
        "pink" | "rose" => "kalam-hl-bar-pink",
        "orange" => "kalam-hl-bar-orange",
        _ => "kalam-hl-bar-yellow",
    }
}

fn chapter_label(chapters: &[String], chapter_index: i64) -> String {
    let i = chapter_index as usize;
    chapters
        .get(i)
        .cloned()
        .unwrap_or_else(|| format!("Chapter {}", i + 1))
}

fn fill_highlights_card(host: &gtk::Box, model: &BookPageModel, chapters: &[String]) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    let Some(book) = &model.book else {
        return;
    };
    let annos = match model.service.catalog().get_annotations_for_book(book.id) {
        Ok(rows) => rows,
        Err(err) => {
            crate::notify::error("Could not read your highlights", &err.to_string());
            Vec::new()
        }
    };
    if annos.is_empty() {
        let none = gtk::Label::new(Some("No highlights yet — select some text in the reader."));
        none.add_css_class("kalam-muted");
        none.set_wrap(true);
        none.set_xalign(0.0);
        host.append(&none);
        return;
    }
    for a in annos.iter().take(4) {
        let item = gtk::Box::new(gtk::Orientation::Horizontal, 9);
        item.add_css_class("kalam-hl-item");
        let bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
        bar.add_css_class("kalam-hl-bar");
        bar.add_css_class(hl_color_class(&a.color));
        bar.set_valign(gtk::Align::Fill);
        item.append(&bar);

        let info = gtk::Box::new(gtk::Orientation::Vertical, 3);
        info.set_hexpand(true);
        let text = gtk::Label::new(Some(&a.text_excerpt));
        text.add_css_class("kalam-hl-text");
        text.set_halign(gtk::Align::Start);
        text.set_wrap(true);
        text.set_xalign(0.0);
        text.set_max_width_chars(40);
        info.append(&text);
        let meta = gtk::Label::new(Some(&chapter_label(chapters, a.chapter_index)));
        meta.add_css_class("kalam-hl-meta");
        meta.set_halign(gtk::Align::Start);
        info.append(&meta);
        item.append(&info);
        host.append(&item);
    }
}

fn fill_author_card(
    widgets: &BookPageModelWidgets,
    book: Option<&Book>,
    catalog: &Catalog,
    sender: &ComponentSender<BookPageModel>,
) {
    let Some(book) = book else {
        return;
    };
    let first_author = book.authors.split(',').next().unwrap_or("").trim();
    let profile = catalog
        .get_author_profile_by_name(first_author)
        .ok()
        .flatten();

    // Avatar: photo if cached, else initials.
    let avatar = &widgets.author_avatar_host;
    while let Some(child) = avatar.first_child() {
        avatar.remove(&child);
    }
    let photo = profile.as_ref().and_then(|p| p.photo_path.clone());
    match photo.filter(|p| p.is_file()) {
        Some(path) => {
            let cover = cover_widget_deferred(Some(&path), 40, 40);
            cover.add_css_class("kalam-author-avatar-img");
            avatar.append(&cover);
            crate::preload::warm_covers(vec![path], 40, 40);
        }
        None => {
            let initials = crate::author::initials(first_author);
            let l = gtk::Label::new(Some(&initials));
            l.add_css_class("kalam-author-initials");
            l.set_hexpand(true);
            l.set_vexpand(true);
            l.set_justify(gtk::Justification::Center);
            avatar.append(&l);
        }
    }

    widgets.author_name.set_label(
        profile
            .as_ref()
            .map(|p| p.canonical_name.as_str())
            .unwrap_or(first_author),
    );

    // Sub-line: birth year when the cached profile has one — never invented.
    let birth_year = profile.as_ref().and_then(|p| extract_year(&p.birth_date));
    let sub = match birth_year {
        Some(y) => format!("b. {y}"),
        None => String::new(),
    };
    widgets.author_sub.set_label(&sub);

    let bio = profile.as_ref().map(|p| p.bio.as_str()).unwrap_or("");
    widgets.author_bio.set_label(if bio.trim().is_empty() {
        "No bio saved yet — the author page can fetch one from Open Library."
    } else {
        bio
    });

    // Other books by this author in the library.
    let books_host = &widgets.author_books_host;
    while let Some(child) = books_host.first_child() {
        books_host.remove(&child);
    }
    let others = crate::author::owned_books_for_author(catalog, first_author)
        .into_iter()
        .filter(|b| b.id != book.id)
        .take(4)
        .collect::<Vec<_>>();
    if others.is_empty() {
        let none = gtk::Label::new(Some("Just this one for now."));
        none.add_css_class("kalam-muted");
        books_host.append(&none);
    } else {
        let other_covers: Vec<_> = others.iter().filter_map(|b| b.cover_path.clone()).collect();
        if !other_covers.is_empty() {
            crate::preload::warm_covers(other_covers, 36, 52);
        }
        for b in others {
            let thumb = gtk::Box::new(gtk::Orientation::Vertical, 4);
            thumb.set_valign(gtk::Align::Start);
            let cover = cover_widget_deferred(b.cover_path.as_deref(), 36, 52);
            cover.add_css_class("kalam-author-thumb-cover");
            thumb.append(&cover);
            let title = gtk::Label::new(Some(&b.title));
            title.add_css_class("kalam-author-book-title");
            title.set_wrap(true);
            title.set_max_width_chars(8);
            title.set_lines(2);
            title.set_justify(gtk::Justification::Center);
            thumb.append(&title);
            let id = b.id;
            let s = sender.clone();
            thumb.set_cursor_from_name(Some("pointer"));
            thumb.set_tooltip_text(Some(&b.title));
            let click = gtk::GestureClick::new();
            click.set_button(1);
            click.connect_released(move |_, _, _, _| {
                s.output(BookPageOut::OpenBook { book_id: id }).ok();
            });
            thumb.add_controller(click);
            books_host.append(&thumb);
        }
    }
}

/// First plausible year inside a free-text birth date ("1973", "1973-07-06").
fn extract_year(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    // `checked_sub` — `saturating_sub` would yield `0..=0` for a short/empty
    // string and index out of bounds below (empty birth dates are common).
    let last = bytes.len().checked_sub(4)?;
    for i in 0..=last {
        if bytes[i].is_ascii_digit()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3].is_ascii_digit()
        {
            let y: i32 = s[i..i + 4].parse().ok()?;
            if (1900..=2030).contains(&y) {
                return Some(s[i..i + 4].to_string());
            }
        }
    }
    None
}

fn fill_journey_card(widgets: &BookPageModelWidgets, model: &BookPageModel, chapters: &[String]) {
    let host = &widgets.journey_host;
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    let more = &widgets.journey_more;
    more.set_visible(false);

    let Some(book) = &model.book else {
        return;
    };
    if chapters.is_empty() {
        // No TOC (or not an EPUB) — hide the whole card's content gracefully.
        let none = gtk::Label::new(Some("Chapter list unavailable for this format."));
        none.add_css_class("kalam-muted");
        host.append(&none);
        return;
    }

    let (chapter_index, _frac) = model
        .service
        .catalog()
        .get_reading_progress(book.id)
        .ok()
        .flatten()
        .unwrap_or((0, 0.0));
    let current = chapter_index.min(chapters.len() - 1);
    let visible = if model.journey_expanded {
        chapters.len()
    } else {
        6.min(chapters.len())
    };

    for (i, title) in chapters.iter().enumerate().take(visible) {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 9);
        row.add_css_class("kalam-journey-row");

        let (glyph, icon_class, label_class) = if i < current {
            ("\u{2713}", "kalam-journey-done", "kalam-journey-label-done")
        } else if i == current {
            (
                "\u{25CF}",
                "kalam-journey-current",
                "kalam-journey-label-current",
            )
        } else {
            ("\u{25CB}", "kalam-journey-todo", "kalam-journey-label-todo")
        };
        let icon = gtk::Label::new(Some(glyph));
        icon.add_css_class("kalam-journey-icon");
        icon.add_css_class(icon_class);
        icon.set_halign(gtk::Align::Center);
        row.append(&icon);

        let label = gtk::Label::new(Some(title));
        label.add_css_class("kalam-journey-label");
        label.add_css_class(label_class);
        label.set_hexpand(true);
        label.set_halign(gtk::Align::Start);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.append(&label);

        if i == current && book.progress < 100 {
            let pct = gtk::Label::new(Some(&format!("{}%", book.progress)));
            pct.add_css_class("kalam-journey-pct");
            row.append(&pct);
        }
        host.append(&row);
    }

    if chapters.len() > 6 {
        let hidden = chapters.len() - visible;
        more.set_visible(true);
        let label = if model.journey_expanded {
            "Show fewer".to_string()
        } else {
            format!("+ {hidden} more chapters")
        };
        more.set_label(&label);
    }
}

fn human_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = KB * 1024;
    const GB: u64 = MB * 1024;
    if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.0} KB", bytes as f64 / KB as f64)
    } else {
        format!("{bytes} B")
    }
}

fn fill_file_card(widgets: &BookPageModelWidgets, book: Option<&Book>) {
    let Some(book) = book else {
        return;
    };
    widgets.file_name.set_label(&book.file_name);

    let rows = &widgets.file_rows_host;
    while let Some(child) = rows.first_child() {
        rows.remove(&child);
    }
    let size = std::fs::metadata(&book.file_path)
        .map(|m| human_size(m.len()))
        .unwrap_or_else(|_| "—".into());
    let size_label = gtk::Label::new(Some(&size));
    size_label.add_css_class("kalam-meta-val");
    size_label.set_halign(gtk::Align::Start);
    rows.append(&meta_row("Size", &size_label));
    let imported = gtk::Label::new(Some(&pretty_imported(&book.added_at)));
    imported.add_css_class("kalam-meta-val");
    imported.set_halign(gtk::Align::Start);
    rows.append(&meta_row("Imported", &imported));
    let hash = format!("{}…", book.file_hash.chars().take(12).collect::<String>());
    let hash_label = gtk::Label::new(Some(&hash));
    hash_label.add_css_class("kalam-meta-val");
    hash_label.set_halign(gtk::Align::Start);
    hash_label.set_selectable(true);
    rows.append(&meta_row("File hash", &hash_label));

    // The "open folder" button is wired once in init — the path is fixed for
    // the page's lifetime, so rebuilding must not stack another handler.
}

fn pretty_imported(iso: &str) -> String {
    if iso.len() >= 10 {
        pretty_day(&iso[..10])
    } else {
        iso.to_string()
    }
}

/// Reveal the book's folder in the system file manager.
fn open_in_file_manager(file: &std::path::Path) {
    let Some(dir) = file.parent() else {
        return;
    };
    let bytes = dir.as_os_str().as_encoded_bytes();
    let mut url = String::from("file://");
    for &b in bytes {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                url.push(b as char)
            }
            _ => url.push_str(&format!("%{b:02X}")),
        }
    }
    let _ = gio::AppInfo::launch_default_for_uri(&url, None::<&gio::AppLaunchContext>);
}

// ---------------------------------------------------------------------------
// Dialogs
// ---------------------------------------------------------------------------

/// Title bar for the three in-app panels (annotations, shelves, tags).
///
/// They had no titles at all, while the A1 dialogs in
/// `crate::widgets::in_app_dialog` do — so the same app showed two different
/// kinds of panel. Same markup and CSS class as that helper's header, so the
/// whole family matches.
///
/// No close button here on purpose: these panels are dismissed by clicking
/// the dimmed backdrop or pressing Esc, and each already ends in a Done
/// button.
fn panel_title(text: &str) -> gtk::Box {
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    head.add_css_class("kalam-in-app-dialog-head");
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-card-title");
    label.set_halign(gtk::Align::Start);
    label.set_hexpand(true);
    label.set_xalign(0.0);
    head.append(&label);
    head
}

/// Full list of a book's highlights/quotes, with per-row delete.
/// The highlights & quotes panel, hosted in the app's in-app float layer
/// (see AppModel::open_annotations_floating) instead of a separate window,
/// so the compositor can't tile it onto another workspace.
pub fn build_annotations_panel(
    catalog: Arc<Catalog>,
    book_id: i64,
    on_done: impl Fn() + 'static,
) -> gtk::Box {
    let on_done = std::rc::Rc::new(on_done);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("kalam-annotations-float");
    root.set_overflow(gtk::Overflow::Hidden);
    root.append(&panel_title("Highlights & quotes"));

    let list_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list_host.set_margin_all(16);

    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list_host)
        .build();
    root.append(&scroll);

    let done = gtk::Button::with_label("Done");
    done.add_css_class("kalam-primary-btn");
    done.set_halign(gtk::Align::End);
    done.set_margin_all(12);
    let done_fn = on_done.clone();
    done.connect_clicked(move |_| done_fn());
    root.append(&done);

    // Self-referential refresh: the delete buttons need to re-run it, so the
    // closure finds itself through a slot it fills in after construction.
    let holder: crate::pages::SelfRebuild = Rc::new(std::cell::RefCell::new(None));
    let closure: Rc<dyn Fn()> = Rc::new({
        let host = list_host.clone();
        let catalog = catalog.clone();
        let holder = holder.clone();
        move || {
            while let Some(child) = host.first_child() {
                host.remove(&child);
            }
            let annos = catalog
                .get_annotations_for_book(book_id)
                .unwrap_or_default();
            if annos.is_empty() {
                let none = gtk::Label::new(Some("No highlights or quotes for this book yet."));
                none.add_css_class("kalam-placeholder");
                none.set_wrap(true);
                host.append(&none);
                return;
            }
            for a in annos {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 9);
                row.set_margin_top(8);
                row.set_margin_bottom(8);
                let bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
                bar.add_css_class("kalam-hl-bar");
                bar.add_css_class(hl_color_class(&a.color));
                row.append(&bar);
                let info = gtk::Box::new(gtk::Orientation::Vertical, 2);
                info.set_hexpand(true);
                let text = gtk::Label::new(Some(&a.text_excerpt));
                text.add_css_class("kalam-hl-text");
                text.set_halign(gtk::Align::Start);
                text.set_wrap(true);
                text.set_xalign(0.0);
                info.append(&text);
                let kind = match a.kind.as_str() {
                    "quote" => "Quote",
                    "note" => "Note",
                    _ => "Highlight",
                };
                let meta = gtk::Label::new(Some(&format!(
                    "{kind} · {}",
                    a.created_at.get(..10).unwrap_or("")
                )));
                meta.add_css_class("kalam-hl-meta");
                meta.set_halign(gtk::Align::Start);
                info.append(&meta);
                row.append(&info);

                let del = gtk::Button::new();
                del.add_css_class("kalam-icon-btn");
                del.set_focus_on_click(false);
                del.set_tooltip_text(Some("Delete"));
                del.set_child(Some(&crate::icons::symbolic("user-trash-symbolic", 15)));
                let id = a.id;
                let catalog = catalog.clone();
                let holder = holder.clone();
                del.connect_clicked(move |_| {
                    if let Err(err) = catalog.delete_annotation(id) {
                        crate::notify::error("Could not delete the annotation", &err.to_string());
                    } else {
                        crate::notify::compact("Annotation deleted", "");
                    }
                    if let Some(refresh) = holder.borrow().clone() {
                        refresh();
                    }
                });
                row.append(&del);
                host.append(&row);
            }
        }
    });
    *holder.borrow_mut() = Some(closure.clone());
    closure();
    root
}

/// Checklist of manual shelves for one book, hosted in the app's in-app
/// float layer (see AppModel::open_shelves_floating) instead of a separate
/// window, so the compositor can't tile it onto another workspace. The
/// book page underneath refreshes when the float closes.
pub fn build_shelves_panel(
    catalog: Arc<Catalog>,
    book_id: i64,
    on_done: impl Fn() + 'static,
) -> gtk::Box {
    let on_done = std::rc::Rc::new(on_done);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("kalam-shelves-float");
    root.set_overflow(gtk::Overflow::Hidden);
    root.append(&panel_title("Shelves"));

    let list_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list_host.set_margin_all(16);

    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list_host)
        .build();
    root.append(&scroll);

    let done = gtk::Button::with_label("Done");
    done.add_css_class("kalam-primary-btn");
    done.set_halign(gtk::Align::End);
    done.set_margin_all(12);
    let done_fn = on_done.clone();
    done.connect_clicked(move |_| done_fn());
    root.append(&done);

    let list = list_host.clone();
    let all: Vec<_> = catalog
        .list_shelves()
        .unwrap_or_default()
        .into_iter()
        .filter(|s| s.kind == ShelfKind::Manual)
        .collect();

    if all.is_empty() {
        let empty = gtk::Label::new(Some(
            "No manual shelves yet.\n\nCreate one from the Shelves page, then add books to it here.\n\nSmart shelves fill themselves from rules \u{2014} they can't be edited by hand.",
        ));
        empty.add_css_class("kalam-placeholder");
        empty.set_wrap(true);
        empty.set_xalign(0.0);
        empty.set_halign(gtk::Align::Start);
        list.append(&empty);
    } else {
        let hint = gtk::Label::new(Some("Tick the shelves this book belongs on."));
        hint.add_css_class("kalam-muted");
        hint.set_halign(gtk::Align::Start);
        hint.set_margin_bottom(6);
        list.append(&hint);

        let current: Vec<i64> = catalog
            .shelves_for_book(book_id)
            .unwrap_or_default()
            .iter()
            .map(|(id, _)| *id)
            .collect();

        for shelf in all {
            let check = gtk::CheckButton::with_label(&format!(
                "{}  ({} book{})",
                shelf.name,
                shelf.book_count,
                if shelf.book_count == 1 { "" } else { "s" }
            ));
            check.add_css_class("kalam-picker-row");
            check.set_active(current.contains(&shelf.id));

            let catalog = catalog.clone();
            let shelf_id = shelf.id;
            let shelf_name = shelf.name.clone();
            check.connect_toggled(move |c| {
                if c.is_active() {
                    crate::notify::outcome(
                        catalog.add_book_to_shelf(shelf_id, book_id),
                        "Added to shelf",
                        &shelf_name,
                        "Could not add to the shelf",
                    );
                } else {
                    crate::notify::outcome_info(
                        catalog.remove_book_from_shelf(shelf_id, book_id),
                        "Removed from shelf",
                        &shelf_name,
                        "Could not remove from the shelf",
                    );
                }
            });
            list.append(&check);
        }
    }

    root
}

/// All of a book's tags in one place: add new ones, remove existing. Hosted
/// in the app's in-app float layer (see AppModel::open_tags_floating).
pub fn build_tags_panel(
    catalog: Arc<Catalog>,
    book_id: i64,
    on_done: impl Fn() + 'static,
) -> gtk::Box {
    let on_done = std::rc::Rc::new(on_done);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("kalam-tags-float");
    root.set_overflow(gtk::Overflow::Hidden);
    root.append(&panel_title("Tags"));

    let add_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    add_row.set_margin_top(4);
    add_row.set_margin_start(16);
    add_row.set_margin_end(16);
    add_row.set_margin_bottom(4);

    let entry = gtk::Entry::builder()
        .placeholder_text("e.g. fantasy")
        .build();
    entry.set_hexpand(true);
    add_row.append(&entry);

    let add_btn = gtk::Button::with_label("Add");
    add_btn.add_css_class("kalam-primary-btn");
    add_row.append(&add_btn);
    root.append(&add_row);

    let section = gtk::Label::new(Some("Current tags"));
    section.add_css_class("kalam-section-label");
    section.set_halign(gtk::Align::Start);
    section.set_margin_top(8);
    section.set_margin_start(16);
    root.append(&section);

    let list_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list_host.set_margin_start(16);
    list_host.set_margin_end(16);
    list_host.set_margin_bottom(8);

    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list_host)
        .build();
    root.append(&scroll);

    let done = gtk::Button::with_label("Done");
    done.add_css_class("kalam-primary-btn");
    done.set_halign(gtk::Align::End);
    done.set_margin_all(12);
    let done_fn = on_done.clone();
    done.connect_clicked(move |_| done_fn());
    root.append(&done);

    // Self-referential refresh: add/remove need to re-run it, so the
    // closure finds itself through a slot it fills in after construction.
    let holder: crate::pages::SelfRebuild = Rc::new(std::cell::RefCell::new(None));
    let closure: Rc<dyn Fn()> = Rc::new({
        let host = list_host.clone();
        let catalog = catalog.clone();
        let holder = holder.clone();
        move || {
            while let Some(child) = host.first_child() {
                host.remove(&child);
            }
            let tags = catalog
                .get_book(book_id)
                .ok()
                .flatten()
                .map(|b| b.tags)
                .unwrap_or_default();
            if tags.is_empty() {
                let none = gtk::Label::new(Some("No tags yet \u{2014} add one above."));
                none.add_css_class("kalam-placeholder");
                none.set_xalign(0.0);
                host.append(&none);
                return;
            }
            for tag in tags {
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 9);
                row.set_margin_top(4);
                row.set_margin_bottom(4);
                let text = gtk::Label::new(Some(&tag));
                text.add_css_class("kalam-hl-text");
                text.set_halign(gtk::Align::Start);
                text.set_hexpand(true);
                text.set_ellipsize(gtk::pango::EllipsizeMode::End);
                row.append(&text);

                let del = gtk::Button::new();
                del.add_css_class("kalam-icon-btn");
                del.set_focus_on_click(false);
                del.set_tooltip_text(Some("Remove tag"));
                del.set_child(Some(&crate::icons::symbolic("user-trash-symbolic", 15)));
                let catalog = catalog.clone();
                let holder = holder.clone();
                del.connect_clicked(move |_| {
                    if let Err(err) = catalog.remove_book_tag(book_id, &tag) {
                        crate::notify::error("Could not remove the tag", &err.to_string());
                    } else {
                        crate::notify::compact("Tag removed", &tag);
                    }
                    if let Some(refresh) = holder.borrow().clone() {
                        refresh();
                    }
                });
                row.append(&del);
                host.append(&row);
            }
        }
    });
    *holder.borrow_mut() = Some(closure.clone());

    let entry_add = entry.clone();
    let add_catalog = catalog.clone();
    let holder_add = holder.clone();
    add_btn.connect_clicked(move |_| {
        let text = entry_add.text().to_string();
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        if let Err(err) = add_catalog.add_book_tag(book_id, &text) {
            crate::notify::error("Could not add the tag", &err.to_string());
        } else {
            crate::notify::compact("Tag added", &text);
            entry_add.set_text("");
        }
        if let Some(refresh) = holder_add.borrow().clone() {
            refresh();
        }
    });

    closure();
    root
}
