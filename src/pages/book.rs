//! Book details page (book-details-spec.md, decisions 77–93).
//!
//! A scrolling page on plain `bg`: a 200×300 cover with its progress rail
//! beside the header block (kind, title, series link, credits, facts,
//! rating, tags, description, actions), then two columns — the outlined
//! Contents timeline and Annotations — over a one-line file footer. The
//! old hero/card grid (stats, author card, details grid) is gone by
//! design; its data and actions live on in this layout.
//!
//! "Your impression" (decision 91) is a designed-but-unbuilt feature with
//! no storage yet, scheduled separately: the right column is built to
//! work without that box, exactly as the spec's unfinished state does.
//! The ⋯ menu, cover popover, Annotations dialog and Tag dialog are the
//! designed forms; the shelves checklist in the ⋯ submenu is designed
//! too, while the app-level shelves float (opened from the book float)
//! still uses the older panel below.
//!
//! The series link opens the series page (`series.rs`, decision 94).

use crate::db::{Catalog, HighlightColor, ShelfKind};
use crate::models::{Book, BookFormat};
use crate::pages::history::pretty_day;
use crate::pages::metadata_editor::open_metadata_editor;
use crate::service::{BookPageSnapshot, LibraryService};
use crate::widgets::author_links::replace_author_links;
use crate::widgets::book_row::{cache_decoded_cover, invalidate_cover_cache};
use crate::widgets::{
    chip::chip_label,
    cover_state::{book_cover, CoverAvailability},
    dialog::{
        confirmation_dialog, dialog_shell, present_dialog, text_field, ConfirmationKind, DialogSize,
    },
    in_app_dialog::{DialogExit, InAppDialog},
    menu::{menu_button, menu_popover, MenuWidth},
    pill_button::{boxed_ghost_button, icon_button, primary_button},
    progress::linear_progress,
    rating::rating_control,
    segmented::{segmented_control, SegmentedSize},
    slide_out_pill::slide_out_pill,
    surface::{hairline, outlined_section, quiet_notice, state_card},
    timeline::{timeline, timeline_row, TimelineEntry, TimelineLinks, TimelineState},
};
use gtk::prelude::*;
use relm4::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Debug)]
pub enum BookPageOut {
    OpenReader,
    OpenAuthor {
        name: String,
    },
    /// Open another book's page (the Finished slide-out's Next pill).
    OpenBook {
        book_id: i64,
    },
    /// Open the series page for a book's series.
    OpenSeries {
        series: String,
        first_author: String,
    },
    Deleted {
        #[allow(dead_code)] // carried for the open/delete round-trip; some arms ignore it
        book_id: i64,
    },
    /// Open the deep editor on this book (the ⋯ menu's Edit book…,
    /// decision 86) — straight to the workspace, no picker.
    EditBook,
}

/// What the page's worker delivers: the service snapshot plus the
/// covers it decoded on the same thread. The apply puts the covers in
/// the cache *before* any cover frame is built, so every frame takes
/// its already-cached branch and no swap is ever scheduled for this
/// page (7.1 step 2a.2) — the cover arrives with the data, never after
/// it.
#[derive(Debug)]
pub struct PageLoad {
    pub snap: BookPageSnapshot,
    pub covers: Vec<crate::preload::DecodedCover>,
}

#[derive(Debug)]
pub enum BookPageMsg {
    /// The page snapshot finished on its worker (7.1 step 2a) — the page
    /// painted its skeleton in `init` and fills everything from this.
    Loaded(Box<PageLoad>),
    /// The chapter-titles parse finished on its worker. Arrives after
    /// `Loaded` (it is only requested once the book's format is known)
    /// and is applied whenever it lands — titles are per book id.
    ChaptersLoaded(Vec<String>),
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
    Refresh,
    /// Open the Tag dialog from the hero's "+" chip.
    ShowTags,
    /// Toggle the journey between 6 chapters and the full list.
    ToggleJourney,
    /// Open the full annotations dialog from the highlights card.
    ViewHighlights,
    /// Remaster comic archive using Lanczos3 upscaler.
    RemasterComic,
    /// Expand/collapse the header description past its three lines.
    ToggleDescription,
    /// Ask before removing the book (the ⋯ menu's destructive item).
    ConfirmDelete,
    /// Open the deep editor on this book (the ⋯ menu's Edit book…).
    EditBook,
    /// Reveal the book's folder — the file footer's button, the ⋯ menu
    /// item, and the file-missing state's "Locate file" primary.
    LocateFile,
    /// A shelf checklist row in the ⋯ submenu was toggled. Applied
    /// without a rebuild so the open submenu survives the toggle.
    SetShelf {
        shelf_id: i64,
        member: bool,
    },
    /// "New shelf…" from the shelves submenu: name prompt, then the
    /// book lands on the shelf it just created.
    NewShelf,
    /// Cover popover: pick an image file as the new cover.
    ChooseCoverImage,
    /// The file picked by `ChooseCoverImage`'s dialog.
    CoverImageChosen(PathBuf),
    /// Cover popover: search online (the metadata dialog's cover search
    /// until the dedicated cover picker is designed).
    FindCoverOnline,
    /// Cover popover: put the file's own cover back.
    RestoreOriginalCover,
    /// A cover replacement/restore finished on its worker.
    CoverChanged(Result<(), String>),
}

pub struct BookPageModel {
    service: LibraryService,
    /// The page's book id, known from `init` — the route is per book, so a
    /// Refresh racing the first snapshot still has the id to ask for.
    book_id: i64,
    book: Option<Book>,
    in_reading_list: bool,
    finished: bool,
    /// Contents expanded to its fixed-height scrolling form.
    contents_expanded: bool,
    /// Header description shown past its three-line clamp.
    description_expanded: bool,
    /// Spine chapter titles, cached per book id — opening the EPUB cache is
    /// cheap once extracted, but there is no reason to repeat it.
    chapter_titles: Vec<String>,
    chapter_titles_for: i64,
    /// True from `init` until the first `Loaded` arrives: the skeleton
    /// phase. The not-found wording must not flash during it.
    loading: bool,
    /// A chapter-titles parse is in flight; Contents says
    /// "Loading chapter list…" rather than vanishing while it is.
    chapters_loading: bool,
    // The snapshot state below replaced live reads in the fill functions
    // (7.1 step 2a): rebuild is now pure view-from-model.
    progress: Option<(usize, f64)>,
    stats: crate::service::BookStatsSnapshot,
    annotations: Vec<crate::db::Annotation>,
    bookmarks: Vec<crate::db::ReadingBookmark>,
    saved_words: Vec<crate::db::SavedWord>,
    shelves: Vec<crate::db::Shelf>,
    shelf_ids: Vec<i64>,
    next_in_series: Option<Book>,
    next_series_title: Option<String>,
    series_position: Option<usize>,
    series_total: Option<usize>,
    file_size: Option<u64>,
}

#[relm4::component(pub)]
impl Component for BookPageModel {
    type Init = (LibraryService, i64);
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
                set_spacing: 28,

                // ── hero: cover rail + header block ───────────────────
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    add_css_class: "kalam-bookpage-hero",
                    set_spacing: 32,

                    // left: 200×300 cover, progress beneath it
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 14,
                        set_width_request: 200,
                        // Explicit, not computed: the progress bar's own
                        // hexpand would otherwise propagate up and the
                        // hero row would split its extra width with this
                        // rail (seen in the first CI captures).
                        set_hexpand: false,
                        set_valign: gtk::Align::Start,

                        #[name = "cover_host"]
                        gtk::Box {
                            set_halign: gtk::Align::Start,
                            set_valign: gtk::Align::Start,
                        },

                        #[name = "progress_host"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 8,
                            set_width_request: 200,
                        },
                    },

                    // header block: kind, title, series, credits, facts,
                    // rating, tags, description, actions
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 8,
                        set_hexpand: true,
                        set_valign: gtk::Align::Start,

                        #[name = "kind"]
                        gtk::Label {
                            add_css_class: "kalam-bookpage-kind",
                            set_halign: gtk::Align::Start,
                            set_xalign: 0.0,
                        },

                        #[name = "title"]
                        gtk::Label {
                            add_css_class: "kalam-bookpage-title",
                            set_halign: gtk::Align::Start,
                            set_wrap: true,
                            set_xalign: 0.0,
                            set_max_width_chars: 34,
                        },

                        #[name = "series_host"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_halign: gtk::Align::Start,
                        },

                        #[name = "credits_host"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_halign: gtk::Align::Start,
                        },

                        #[name = "facts"]
                        gtk::Label {
                            add_css_class: "kalam-bookpage-facts",
                            set_halign: gtk::Align::Start,
                            set_wrap: true,
                            set_xalign: 0.0,
                            set_max_width_chars: 88,
                        },

                        #[name = "rating_host"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            set_halign: gtk::Align::Start,
                        },

                        #[name = "tags_flow"]
                        gtk::FlowBox {
                            add_css_class: "kalam-tags",
                            set_halign: gtk::Align::Start,
                            set_selection_mode: gtk::SelectionMode::None,
                            set_column_spacing: 6,
                            set_row_spacing: 6,
                        },

                        #[name = "description_host"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 4,
                        },

                        #[name = "actions_host"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Horizontal,
                            add_css_class: "kalam-actions",
                            set_spacing: 10,
                            set_margin_top: 12,
                        },
                    },
                },

                // ── sections: Contents (outlined) | Annotations ───────
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 52,
                    set_valign: gtk::Align::Start,

                    #[name = "contents_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_size_request: (560, -1),
                        set_hexpand: false,
                        set_valign: gtk::Align::Start,
                    },

                    #[name = "annotations_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 10,
                        set_hexpand: true,
                        set_valign: gtk::Align::Start,
                    },
                },

                // ── file line ─────────────────────────────────────────
                #[name = "file_host"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 10,
                },
            },
        }
    }

    fn init(
        (service, book_id): Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Roadmap 7.1 step 2a: the page paints its skeleton immediately and
        // every read — database, the EPUB parse for chapter titles, even the
        // file-size stat — happens on a worker. `route_open:book` used to
        // measure 96-686 ms on the owner's machine; the skeleton is the
        // ~15 ms that remains.
        let mut model = BookPageModel {
            book_id,
            in_reading_list: false,
            finished: false,
            contents_expanded: false,
            description_expanded: false,
            chapter_titles: Vec::new(),
            chapter_titles_for: 0,
            loading: true,
            chapters_loading: false,
            progress: None,
            stats: Default::default(),
            annotations: Vec::new(),
            bookmarks: Vec::new(),
            saved_words: Vec::new(),
            shelves: Vec::new(),
            shelf_ids: Vec::new(),
            next_in_series: None,
            next_series_title: None,
            series_position: None,
            series_total: None,
            file_size: None,
            service,
            book: None,
        };
        let widgets = view_output!();
        model.request_snapshot(book_id, &sender);
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
        // One message opts out of the rebuild: a shelf toggle inside the
        // open ⋯ submenu, where rebuilding the actions would destroy the
        // very menu being used.
        let mut rebuild = true;
        match msg {
            BookPageMsg::Loaded(load) => {
                // The worker's snapshot is the only source of page state.
                // Every read the old sync path did inline — including the
                // two `book_detail` calls — is this one arrival.
                let first_load = std::mem::replace(&mut self.loading, false);
                let PageLoad { snap, covers } = *load;
                // The covers were decoded on the snapshot's own worker.
                // Into the cache now, before the rebuild below builds any
                // cover frame: each then takes its already-cached branch,
                // and this page never schedules a cover swap (2a.2).
                for decoded in &covers {
                    cache_decoded_cover(decoded);
                }
                let BookPageSnapshot {
                    detail,
                    progress,
                    stats,
                    annotations,
                    annotations_error,
                    bookmarks,
                    saved_words,
                    shelves,
                    shelf_ids,
                    next_in_series,
                    next_series_title,
                    series_position,
                    series_total,
                    file_size,
                } = snap;
                report_errors(&detail.errors);
                report_errors(&stats.errors);
                if let Some(err) = annotations_error {
                    crate::notify::error("Could not read your highlights", &err);
                }
                self.book = detail.book;
                self.in_reading_list = detail.in_reading_list;
                self.finished = detail.finished;
                self.progress = progress;
                self.stats = stats;
                self.annotations = annotations;
                self.bookmarks = bookmarks;
                self.saved_words = saved_words;
                self.shelves = shelves;
                self.shelf_ids = shelf_ids;
                self.next_in_series = next_in_series;
                self.next_series_title = next_series_title;
                self.series_position = series_position;
                self.series_total = series_total;
                self.file_size = file_size;

                // CI screenshot hook, the dialog counterpart of
                // KALAM_ROUTE: `KALAM_DIALOG=tags|annotations` opens that
                // dialog over the loaded page so the headless run can
                // photograph it — no route reaches a dialog, and a dialog
                // the screenshot loop cannot open is a dialog it cannot
                // check. First load only, so a Refresh never reopens it.
                if first_load {
                    match std::env::var("KALAM_DIALOG").as_deref() {
                        Ok("tags") => sender.input(BookPageMsg::ShowTags),
                        Ok("annotations") => sender.input(BookPageMsg::ViewHighlights),
                        _ => {}
                    }
                }

                // Chapter titles are the expensive half (an EPUB open and
                // spine walk): parsed on their own worker and filled in
                // whenever they arrive. Only EPUBs have a spine to read.
                if let Some(book) = &self.book {
                    if book.format == BookFormat::Epub && self.chapter_titles_for != book.id {
                        self.chapters_loading = true;
                        let path = book.file_path.clone();
                        let cache = crate::paths::reader_cache_dir(&book.uuid);
                        let s = sender.input_sender().clone();
                        crate::tasks::spawn_internal(
                            "Reading chapter list",
                            move |_reporter| {
                                crate::epub_book::OpenBook::open(&path, &cache)
                                    .map(|ob| ob.spine.iter().map(|c| c.title.clone()).collect())
                                    .unwrap_or_default()
                            },
                            |_| {},
                            move |titles| {
                                let _ = s.send(BookPageMsg::ChaptersLoaded(titles));
                            },
                        );
                    }
                }
            }
            BookPageMsg::ChaptersLoaded(titles) => {
                self.chapters_loading = false;
                if let Some(book) = &self.book {
                    self.chapter_titles = titles;
                    self.chapter_titles_for = book.id;
                }
            }
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
                    // Drop the snapshot state too: the cards read it, and
                    // a deleted book's numbers under "Book not found"
                    // would be a lie.
                    self.progress = None;
                    self.stats = Default::default();
                    self.annotations = Vec::new();
                    self.bookmarks = Vec::new();
                    self.saved_words = Vec::new();
                    self.shelf_ids = Vec::new();
                    self.next_in_series = None;
                    self.next_series_title = None;
                    self.file_size = None;
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
                            self.in_reading_list = false;
                            crate::notify::info("Removed from reading list", &title);
                        }
                    } else if crate::notify::report(
                        self.service.catalog().add_to_reading_list(id),
                        "Could not update the reading list",
                    ) {
                        self.in_reading_list = true;
                        crate::notify::success("Added to reading list", &title);
                    }
                    // No re-read: the flag above is the write's outcome. The
                    // old `reload_state` paid three queries on the UI thread
                    // to learn what this branch already knows (7.1 step 2a).
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
                    let saved = crate::notify::outcome(
                        self.service.catalog().set_book_rating(id, half_stars),
                        "Rating saved",
                        &detail,
                        "Could not save the rating",
                    );
                    if saved {
                        if let Some(book) = self.book.as_mut() {
                            // Half-stars, the same unit the picker shows.
                            book.rating = half_stars;
                        }
                    }
                    // The star picker redraws from `book.rating` on the
                    // rebuild below — no re-read needed.
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
            BookPageMsg::ToggleFinished => {
                if let Some(book) = &self.book {
                    let id = book.id;
                    let becoming = !self.finished;
                    let title = book.title.clone();
                    if crate::notify::report(
                        self.service.catalog().set_book_finished(id, becoming),
                        "Could not update the book",
                    ) {
                        self.finished = becoming;
                        if becoming {
                            crate::notify::success("Marked as finished", &title);
                        } else {
                            crate::notify::info("Marked as unread", &title);
                        }
                    }
                }
            }
            BookPageMsg::EditMetadata => {
                if let Some(book) = &self.book {
                    let id = book.id;
                    let s = sender.clone();
                    open_metadata_editor(root, self.service.catalog().clone(), id, move || {
                        // The editor dialog is non-modal and can outlive this
                        // page. relm4's `input()` panics if the component
                        // runtime is gone, so send through the raw channel,
                        // which quietly drops the message instead.
                        let _ = s.input_sender().send(BookPageMsg::Refresh);
                    });
                }
            }
            BookPageMsg::ShowTags => {
                present_tags_dialog(root, self, &sender);
            }
            BookPageMsg::ToggleJourney => {
                self.contents_expanded = !self.contents_expanded;
            }
            BookPageMsg::ViewHighlights => {
                present_annotations_dialog(root, self, self.chapter_titles.clone(), &sender);
            }
            BookPageMsg::Refresh => {
                // After a metadata save or a remaster the page needs fresh
                // rows — same worker path as the initial load. Chapter
                // titles are cached per book id and are not re-parsed.
                self.request_snapshot(self.book_id, &sender);
            }
            BookPageMsg::ToggleDescription => {
                self.description_expanded = !self.description_expanded;
            }
            BookPageMsg::ConfirmDelete => {
                if let Some(book) = &self.book {
                    let title = book.title.clone();
                    let s = sender.input_sender().clone();
                    let dialog = confirmation_dialog(
                        root,
                        "Remove this book?",
                        &format!(
                            "“{title}” will be removed from your library and its files \
                             deleted. This cannot be undone."
                        ),
                        "Keep book",
                        "Remove book",
                        ConfirmationKind::Destructive,
                        move || {
                            let _ = s.send(BookPageMsg::Delete);
                        },
                    );
                    if dialog.is_none() {
                        crate::notify::error(
                            "Could not remove the book",
                            "The confirmation dialog could not be opened.",
                        );
                    }
                }
            }
            BookPageMsg::LocateFile => {
                if let Some(book) = &self.book {
                    open_in_file_manager(&book.file_path.clone());
                }
            }
            BookPageMsg::EditBook => {
                let _ = sender.output(BookPageOut::EditBook);
            }
            BookPageMsg::SetShelf { shelf_id, member } => {
                if let Some(book) = &self.book {
                    let catalog = self.service.catalog();
                    let result = if member {
                        catalog.add_book_to_shelf(shelf_id, book.id)
                    } else {
                        catalog.remove_book_from_shelf(shelf_id, book.id)
                    };
                    match result {
                        Ok(()) => {
                            // The menu's tick already shows the new state;
                            // mirror it in the model and skip the rebuild
                            // so the submenu stays open for the next toggle.
                            if member {
                                if !self.shelf_ids.contains(&shelf_id) {
                                    self.shelf_ids.push(shelf_id);
                                }
                            } else {
                                self.shelf_ids.retain(|id| *id != shelf_id);
                            }
                            rebuild = false;
                        }
                        Err(err) => {
                            crate::notify::error("Could not update the shelf", &err.to_string());
                        }
                    }
                }
            }
            BookPageMsg::NewShelf => {
                if let Some(book) = &self.book {
                    let book_id = book.id;
                    let shell = dialog_shell(
                        DialogSize::Form,
                        Some("This book will be added to the new shelf"),
                    );
                    let field = text_field("Shelf name", "e.g. Favourites", "");
                    shell.body().append(&field.widget());

                    let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
                    let slot_for_cancel = slot.clone();
                    shell
                        .footer()
                        .append(&boxed_ghost_button("Cancel", move || {
                            if let Some(dialog) = slot_for_cancel.borrow().clone() {
                                dialog.close();
                            }
                        }));

                    let slot_for_create = slot.clone();
                    let field_for_create = field.clone();
                    let catalog = self.service.catalog().clone();
                    let s = sender.input_sender().clone();
                    shell
                        .footer()
                        .append(&primary_button("Create shelf", move || {
                            let name = field_for_create.value();
                            let name = name.trim();
                            if name.is_empty() {
                                return;
                            }
                            if let Some(dialog) = slot_for_create.borrow().clone() {
                                dialog.close();
                            }
                            match catalog.create_shelf(name, ShelfKind::Manual, "", "") {
                                Ok(shelf_id) => {
                                    let added = crate::notify::outcome(
                                        catalog.add_book_to_shelf(shelf_id, book_id),
                                        "Shelf created",
                                        name,
                                        "Could not add the book to the new shelf",
                                    );
                                    if added {
                                        let _ = s.send(BookPageMsg::Refresh);
                                    }
                                }
                                Err(err) => {
                                    crate::notify::error(
                                        "Could not create the shelf",
                                        &err.to_string(),
                                    );
                                }
                            }
                        }));

                    match present_dialog(root, "New shelf", DialogExit::UnsavedInput, &shell) {
                        Some(dialog) => *slot.borrow_mut() = Some(dialog),
                        None => crate::notify::error(
                            "Could not create the shelf",
                            "The dialog could not be opened.",
                        ),
                    }
                }
            }
            BookPageMsg::ChooseCoverImage => {
                if self.book.is_some() {
                    let dialog = gtk::FileDialog::builder()
                        .title("Choose a cover image")
                        .modal(true)
                        .build();
                    let filter = gtk::FileFilter::new();
                    filter.set_name(Some("Images"));
                    for suffix in ["png", "jpg", "jpeg", "webp", "gif"] {
                        filter.add_suffix(suffix);
                    }
                    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
                    filters.append(&filter);
                    dialog.set_filters(Some(&filters));

                    let window = root
                        .root()
                        .and_then(|r| r.downcast::<gtk::Window>().ok())
                        .or_else(|| {
                            relm4::main_application()
                                .active_window()
                                .and_then(|w| w.downcast::<gtk::Window>().ok())
                        });
                    let s = sender.input_sender().clone();
                    dialog.open(window.as_ref(), gtk::gio::Cancellable::NONE, move |res| {
                        if let Ok(file) = res {
                            if let Some(path) = file.path() {
                                let _ = s.send(BookPageMsg::CoverImageChosen(path));
                            }
                        }
                    });
                }
            }
            BookPageMsg::CoverImageChosen(path) => {
                if let Some(book) = self.book.clone() {
                    let catalog = self.service.catalog().clone();
                    let s = sender.input_sender().clone();
                    crate::tasks::spawn(
                        "Replacing cover",
                        move |_reporter| {
                            let bytes = std::fs::read(&path)
                                .map_err(|e| format!("Could not read that file: {e}"))?;
                            if bytes.is_empty() {
                                return Err("That image file is empty.".to_string());
                            }
                            crate::epub::replace_cover_bytes(&catalog, &book, &bytes)
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        },
                        |_| {},
                        move |res| {
                            let _ = s.send(BookPageMsg::CoverChanged(res));
                        },
                    );
                }
            }
            BookPageMsg::FindCoverOnline => {
                // The online cover search lives in the metadata dialog
                // today (its cover block); until the metadata redesign
                // gives covers a picker of their own, this is that door.
                if let Some(book) = &self.book {
                    let id = book.id;
                    let s = sender.clone();
                    open_metadata_editor(root, self.service.catalog().clone(), id, move || {
                        let _ = s.input_sender().send(BookPageMsg::Refresh);
                    });
                }
            }
            BookPageMsg::RestoreOriginalCover => {
                if let Some(book) = self.book.clone() {
                    let catalog = self.service.catalog().clone();
                    let s = sender.input_sender().clone();
                    crate::tasks::spawn(
                        "Restoring cover",
                        move |_reporter| {
                            crate::epub::restore_original_cover(&catalog, &book)
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        },
                        |_| {},
                        move |res| {
                            let _ = s.send(BookPageMsg::CoverChanged(res));
                        },
                    );
                }
            }
            BookPageMsg::CoverChanged(result) => match result {
                Ok(()) => {
                    let detail = self
                        .book
                        .as_ref()
                        .map(|b| b.title.clone())
                        .unwrap_or_default();
                    crate::notify::success("Cover updated", &detail);
                    self.request_snapshot(self.book_id, &sender);
                }
                Err(err) => crate::notify::error("Could not change the cover", &err),
            },
            BookPageMsg::RemasterComic => {
                if let Some(book) = &self.book {
                    if matches!(book.format, BookFormat::Cbz | BookFormat::Cbr) {
                        let catalog = self.service.catalog().clone();
                        let s = sender.clone();
                        // Owner rule, 2026-09-29: every option is the user's
                        // to decide — scale, which pages, replace-or-copy,
                        // format. The old handler hardcoded 2×, every page,
                        // JPEG 92, and replaced the file with no backup and
                        // no re-hash; the dialog fixes all of that.
                        crate::widgets::remaster_dialog::present(root, book, &catalog, move || {
                            // A remaster runs for minutes; the page
                            // may be closed before it finishes. The
                            // raw sender drops the message quietly
                            // instead of panicking on a dead page.
                            let _ = s.input_sender().send(BookPageMsg::Refresh);
                        });
                    } else {
                        crate::notify::info(
                            "Not a comic",
                            "Remastering is only available for comic archives (CBZ/CBR)",
                        );
                    }
                }
            }
        }
        if rebuild {
            self.rebuild(widgets, &sender);
        }
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
    /// Ask a worker for the page snapshot — the initial load and every
    /// `Refresh` use the same path.
    ///
    /// The whole read (book row, flags, progress, stats, highlights, author
    /// profile) happens off the UI thread; the page applies it when
    /// `Loaded` arrives. The file-size stat rides along in the worker: it
    /// is a filesystem question, not a catalog one, so it does not belong
    /// in `LibraryService`.
    fn request_snapshot(&self, book_id: i64, sender: &ComponentSender<Self>) {
        let service = LibraryService::new(self.service.catalog().clone());
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Reading book details",
            move |_reporter| {
                let mut snap = service.book_page(book_id);
                let mut covers = Vec::new();
                if let Some(book) = &snap.detail.book {
                    snap.file_size = std::fs::metadata(&book.file_path).ok().map(|m| m.len());
                    // The hero cover, decoded here at its displayed size
                    // so the apply finds it cached (7.1 step 2a.2). The
                    // redesigned page shows no other cover.
                    if let Some(cover) = &book.cover_path {
                        if let Some(d) = crate::preload::decode_for_cache(cover, COVER_W, COVER_H) {
                            covers.push(d);
                        }
                    }
                }
                PageLoad { snap, covers }
            },
            |_| {},
            move |load| {
                let _ = s.send(BookPageMsg::Loaded(Box::new(load)));
            },
        );
    }

    /// Refill every host from model state. Called from init and after every
    /// message — the page is small enough that this is cheaper than wiring
    /// per-widget invalidation, and it can't drift out of sync.
    ///
    /// Guarded and spanned (7.1 step 2a.1): this is the apply path's real
    /// cost, and the watchdog must be able to split "while
    /// book_page_rebuild" (this code) from "after book_page_rebuild"
    /// (GTK realize/paint of what it built).
    fn rebuild(&mut self, widgets: &BookPageModelWidgets, sender: &ComponentSender<Self>) {
        let _t = crate::timing::measure("book_page_rebuild");
        let _a = crate::timing::activity("book_page_rebuild");
        let chapters = self.chapter_titles.clone();

        fill_cover(&widgets.cover_host, self, sender);
        fill_progress_rail(&widgets.progress_host, self, &chapters);

        if self.loading && self.book.is_none() {
            // Skeleton phase (7.1 step 2a): the snapshot is still on its
            // worker. Say so, rather than flashing "was removed" for a book
            // that is merely still loading.
            widgets.kind.set_label("");
            widgets.title.set_label("Loading…");
            widgets.facts.set_label("");
            clear_box(&widgets.series_host);
            clear_box(&widgets.credits_host);
            clear_box(&widgets.rating_host);
            clear_box(&widgets.actions_host);
            while let Some(c) = widgets.tags_flow.first_child() {
                widgets.tags_flow.remove(&c);
            }
            set_description_message(&widgets.description_host, "Reading this book's details…");
        } else if let Some(book) = &self.book {
            widgets.kind.set_label(book.format.as_str());
            widgets.title.set_label(&book.title);
            fill_series_link(&widgets.series_host, self, sender);
            let tx = sender.input_sender().clone();
            replace_author_links(
                &widgets.credits_host,
                book.authors_display(),
                "kalam-bookpage-author",
                Rc::new(move |name| {
                    let _ = tx.send(BookPageMsg::OpenAuthor(name));
                }),
            );
            fill_facts(&widgets.facts, book, &chapters);
            fill_rating(&widgets.rating_host, book, sender);
            fill_tags(&widgets.tags_flow, book, sender);
            fill_description(&widgets.description_host, self, sender);
            fill_actions(&widgets.actions_host, self, sender);
        } else {
            widgets.kind.set_label("");
            widgets.title.set_label("Book not found");
            widgets.facts.set_label("");
            clear_box(&widgets.series_host);
            clear_box(&widgets.credits_host);
            clear_box(&widgets.rating_host);
            clear_box(&widgets.actions_host);
            while let Some(c) = widgets.tags_flow.first_child() {
                widgets.tags_flow.remove(&c);
            }
            set_description_message(
                &widgets.description_host,
                "This book was removed or does not exist.",
            );
        }

        // Sections and the file line are pure view-from-model; each fill
        // hides its host when there is nothing true to show.
        fill_contents(&widgets.contents_host, self, &chapters, sender);
        fill_annotations(&widgets.annotations_host, self, &chapters, sender);
        fill_file_line(&widgets.file_host, self);
    }
}

// ---------------------------------------------------------------------------
// Hero
// ---------------------------------------------------------------------------

/// The hero cover, fixed at 200×300 (book-details-spec §1).
const COVER_W: i32 = 200;
const COVER_H: i32 = 300;

fn clear_box(host: &gtk::Box) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
}

/// The snapshot worker stats the book file on its way back; `None` after
/// a completed load is the file-missing state (decision 93).
fn file_missing(model: &BookPageModel) -> bool {
    !model.loading && model.book.is_some() && model.file_size.is_none()
}

fn fill_cover(host: &gtk::Box, model: &BookPageModel, sender: &ComponentSender<BookPageModel>) {
    host.set_size_request(COVER_W, COVER_H);
    clear_box(host);
    let Some(book) = &model.book else {
        return;
    };

    let cover = book_cover(
        book,
        COVER_W,
        COVER_H,
        false,
        CoverAvailability::Available,
        true,
    );
    if file_missing(model) {
        cover.set_opacity(0.45);
    }

    let overlay = gtk::Overlay::new();
    overlay.set_size_request(COVER_W, COVER_H);
    overlay.set_halign(gtk::Align::Start);
    overlay.set_valign(gtk::Align::Start);
    overlay.set_child(Some(&cover));

    // The cover pencil and its 210px popover (decision 80).
    let menu = menu_popover(MenuWidth::Short);
    let s = sender.clone();
    menu.add_item("Choose image…", move || {
        s.input(BookPageMsg::ChooseCoverImage);
    });
    let s = sender.clone();
    menu.add_item("Find online", move || {
        s.input(BookPageMsg::FindCoverOnline);
    });
    let s = sender.clone();
    menu.add_item("Restore original", move || {
        s.input(BookPageMsg::RestoreOriginalCover);
    });
    let pencil = menu_button("document-edit-symbolic", "Edit cover", menu);
    pencil.add_css_class("kalam-bookpage-cover-edit");
    pencil.set_size_request(36, 36);
    pencil.set_halign(gtk::Align::End);
    pencil.set_valign(gtk::Align::End);
    pencil.set_margin_end(8);
    pencil.set_margin_bottom(8);
    overlay.add_overlay(&pencil);

    host.append(&overlay);
    if let Some(path) = &book.cover_path {
        crate::preload::warm_covers(vec![path.clone()], COVER_W, COVER_H);
    }
}

/// Cap a state card's labels so the card wraps inside the 200px cover
/// rail instead of reporting its one-line natural width.
fn constrain_state_card(card: &gtk::Box) {
    let mut child = card.first_child();
    while let Some(widget) = child {
        let next = widget.next_sibling();
        if let Ok(label) = widget.downcast::<gtk::Label>() {
            label.set_max_width_chars(24);
        }
        child = next;
    }
}

fn fill_progress_rail(host: &gtk::Box, model: &BookPageModel, chapters: &[String]) {
    clear_box(host);
    let Some(book) = &model.book else {
        return;
    };

    if file_missing(model) {
        let card = state_card("File not found", "This book's file could not be found.");
        constrain_state_card(&card);
        host.append(&card);
        return;
    }

    let started =
        book.progress > 0 || model.progress.is_some() || model.stats.first_opened.is_some();
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let left_text = if model.finished {
        "Finished".to_string()
    } else if started {
        format!("{}%", book.progress)
    } else {
        "Not started".to_string()
    };
    let left = gtk::Label::new(Some(&left_text));
    left.add_css_class("kalam-bookpage-progress-pct");
    left.set_halign(gtk::Align::Start);
    left.set_hexpand(true);
    row.append(&left);

    let right_text = if model.finished {
        model.stats.finished_at.as_deref().map(pretty_imported)
    } else if started && !chapters.is_empty() {
        let chapter = model
            .progress
            .map(|(ci, _)| ci)
            .unwrap_or(model.stats.chapter_index)
            .min(chapters.len() - 1);
        Some(format!("Ch. {} / {}", chapter + 1, chapters.len()))
    } else {
        None
    };
    if let Some(text) = right_text {
        let right = gtk::Label::new(Some(&text));
        right.add_css_class("kalam-bookpage-progress-meta");
        right.set_halign(gtk::Align::End);
        right.set_valign(gtk::Align::End);
        row.append(&right);
    }
    host.append(&row);

    let fraction = if model.finished {
        1.0
    } else {
        f64::from(book.progress) / 100.0
    };
    let bar = linear_progress(fraction);
    // The rail is exactly the cover's 200px; the bar fills it without
    // asking the hero row for more width.
    bar.set_hexpand(false);
    host.append(&bar);

    if !model.finished && started {
        if let Some(first) = &model.stats.first_opened {
            let started_label =
                gtk::Label::new(Some(&format!("Started {}", pretty_imported(first))));
            started_label.add_css_class("kalam-bookpage-progress-meta");
            started_label.set_halign(gtk::Align::Start);
            host.append(&started_label);
        }
    }
}

fn format_series_index(idx: f32) -> String {
    if (idx.fract()).abs() < f32::EPSILON {
        format!("{}", idx as i64)
    } else {
        format!("{idx}")
    }
}

fn fill_series_link(
    host: &gtk::Box,
    model: &BookPageModel,
    sender: &ComponentSender<BookPageModel>,
) {
    clear_box(host);
    let Some(book) = &model.book else {
        host.set_visible(false);
        return;
    };
    let Some(series) = book
        .series
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    else {
        host.set_visible(false);
        return;
    };
    host.set_visible(true);

    // "Series · Book N of M ›" (decision 81): N from the book's metadata
    // (the merged listing's position is only a fallback), M only when a
    // fetched listing backs it — never an owned-book count.
    let (clean_series, ch_opt) = crate::comics::sanitize_comic_series(series);
    let name = if clean_series.is_empty() {
        series.to_string()
    } else {
        clean_series
    };
    let idx = if book.series_index > 0.0 {
        Some(book.series_index)
    } else {
        ch_opt.filter(|v| *v > 0.0)
    };
    let idx_text = idx
        .map(format_series_index)
        .or_else(|| model.series_position.map(|n| n.to_string()));
    let label = match (idx_text, model.series_total) {
        (Some(n), Some(m)) => format!("{name} · Book {n} of {m} ›"),
        (Some(n), None) => format!("{name} · Book {n} ›"),
        (None, _) => format!("{name} ›"),
    };

    let series = series.to_string();
    let first_author = book
        .authors
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    let btn = gtk::Button::with_label(&label);
    btn.add_css_class("kalam-bookpage-series");
    btn.set_has_frame(false);
    btn.set_focus_on_click(false);
    btn.set_halign(gtk::Align::Start);
    btn.set_tooltip_text(Some("View the full series"));
    let s = sender.clone();
    btn.connect_clicked(move |_| {
        s.input(BookPageMsg::OpenSeries {
            series: series.clone(),
            first_author: first_author.clone(),
        });
    });
    host.append(&btn);
}

fn fill_facts(label: &gtk::Label, book: &Book, chapters: &[String]) {
    // One Meta line (decision 79): only facts with a stored source. The
    // catalog has no page count, language or ISBN today, so the line is
    // year · publisher · chapter count, and hides when it has nothing.
    let mut parts: Vec<String> = Vec::new();
    let published = book.published.trim();
    if !published.is_empty() {
        parts.push(extract_year(published).unwrap_or_else(|| published.to_string()));
    }
    let publisher = book.publisher.trim();
    if !publisher.is_empty() {
        parts.push(publisher.to_string());
    }
    if !chapters.is_empty() {
        parts.push(format!("{} chapters", chapters.len()));
    }
    label.set_label(&parts.join(" · "));
    label.set_visible(!parts.is_empty());
}

fn fill_rating(host: &gtk::Box, book: &Book, sender: &ComponentSender<BookPageModel>) {
    clear_box(host);
    let s = sender.clone();
    let rating = rating_control(book.rating, move |value| {
        s.input(BookPageMsg::SetRating(value));
    });
    host.append(&rating.widget());
}

fn fill_tags(flow: &gtk::FlowBox, book: &Book, sender: &ComponentSender<BookPageModel>) {
    while let Some(child) = flow.first_child() {
        flow.remove(&child);
    }
    for tag in &book.tags {
        flow.append(&chip_label(tag, false));
    }
    let add = gtk::Button::with_label("+");
    add.add_css_class("kalam-ui-chip");
    add.set_size_request(28, 28);
    add.set_halign(gtk::Align::Start);
    add.set_focus_on_click(false);
    add.set_tooltip_text(Some("Manage tags"));
    let s = sender.clone();
    add.connect_clicked(move |_| s.input(BookPageMsg::ShowTags));
    flow.append(&add);
}

fn set_description_message(host: &gtk::Box, text: &str) {
    clear_box(host);
    host.set_visible(true);
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-bookpage-description");
    label.set_halign(gtk::Align::Start);
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.set_max_width_chars(88);
    host.append(&label);
}

fn fill_description(
    host: &gtk::Box,
    model: &BookPageModel,
    sender: &ComponentSender<BookPageModel>,
) {
    clear_box(host);
    let Some(book) = &model.book else {
        return;
    };
    let desc = crate::epub::strip_html(&book.description);
    let desc = desc.trim();
    if desc.is_empty() {
        host.set_visible(false);
        return;
    }
    host.set_visible(true);

    let label = gtk::Label::new(Some(desc));
    label.add_css_class("kalam-bookpage-description");
    label.set_halign(gtk::Align::Start);
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.set_max_width_chars(88);
    if model.description_expanded {
        label.set_ellipsize(gtk::pango::EllipsizeMode::None);
        label.set_lines(-1);
    } else {
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_lines(3);
    }
    host.append(&label);

    // "More" only when three lines cannot hold the text (roughly: the
    // label wraps near 88 chars, so three lines is ~260).
    if desc.chars().count() > 260 || desc.lines().count() > 3 {
        let more = gtk::Button::with_label(if model.description_expanded {
            "Less"
        } else {
            "More"
        });
        more.add_css_class("kalam-detail-card-link");
        more.set_focus_on_click(false);
        more.set_halign(gtk::Align::Start);
        let s = sender.clone();
        more.connect_clicked(move |_| s.input(BookPageMsg::ToggleDescription));
        host.append(&more);
    }
}

fn shorten_title(title: &str, max_chars: usize) -> String {
    if title.chars().count() <= max_chars {
        title.to_string()
    } else {
        let kept: String = title.chars().take(max_chars.saturating_sub(1)).collect();
        format!("{}…", kept.trim_end())
    }
}

fn fill_actions(host: &gtk::Box, model: &BookPageModel, sender: &ComponentSender<BookPageModel>) {
    clear_box(host);
    let Some(book) = &model.book else {
        return;
    };
    let missing = file_missing(model);

    // Primary: Read / Read again, or Locate file when the file is gone.
    let primary = if missing {
        let s = sender.clone();
        primary_button("Locate file", move || {
            s.input(BookPageMsg::LocateFile);
        })
    } else {
        let s = sender.clone();
        primary_button(
            if model.finished { "Read again" } else { "Read" },
            move || {
                let _ = s.output(BookPageOut::OpenReader);
            },
        )
    };
    primary.set_size_request(-1, 44);
    host.append(&primary);

    // Finished toggle; when finished and a next volume is known, the
    // Next pill slides out from behind it (decision 87).
    let s = sender.clone();
    let finished_btn = boxed_ghost_button(
        if model.finished {
            "Finished ✓"
        } else {
            "Finished"
        },
        move || {
            s.input(BookPageMsg::ToggleFinished);
        },
    );
    finished_btn.set_size_request(-1, 44);
    finished_btn.set_tooltip_text(Some(if model.finished {
        "Mark unread"
    } else {
        "Mark finished"
    }));
    let next_owned = model
        .next_in_series
        .as_ref()
        .map(|b| (b.id, b.title.clone()));
    let next_known = next_owned
        .clone()
        .map(|(id, title)| (Some(id), title))
        .or_else(|| model.next_series_title.clone().map(|title| (None, title)));
    if model.finished {
        if let Some((next_id, next_title)) = next_known {
            let short = shorten_title(&next_title, 32);
            let label = if next_id.is_some() {
                format!("Next: {short} ›")
            } else {
                format!("Next: {short} · Not in library")
            };
            let s = sender.clone();
            let slide = slide_out_pill(&finished_btn, &label, move || {
                if let Some(book_id) = next_id {
                    let _ = s.output(BookPageOut::OpenBook { book_id });
                }
            });
            slide.secondary_button().set_tooltip_text(Some(&next_title));
            slide.set_secondary_sensitive(next_id.is_some());
            host.append(&slide.widget());
        } else {
            host.append(&finished_btn);
        }
    } else {
        host.append(&finished_btn);
    }

    // Reading list.
    let s = sender.clone();
    let reading_list = boxed_ghost_button(
        if model.in_reading_list {
            "In reading list"
        } else {
            "Reading list"
        },
        move || {
            s.input(BookPageMsg::ToggleReadingList);
        },
    );
    reading_list.set_size_request(-1, 44);
    reading_list.set_tooltip_text(Some(if model.in_reading_list {
        "Remove from reading list"
    } else {
        "Add to reading list"
    }));
    host.append(&reading_list);

    // Edit metadata pencil.
    let s = sender.clone();
    let edit = icon_button(
        "document-edit-symbolic",
        "Edit metadata",
        44,
        true,
        move || {
            s.input(BookPageMsg::EditMetadata);
        },
    );
    host.append(&edit);

    // The ⋯ menu (decision 86, minus the editor entries the deep-editor
    // exclusion removes). Remaster rides along for comics: the spec
    // covers EPUB/PDF, and this keeps the comic action reachable.
    let shelves_menu = menu_popover(MenuWidth::Wide);
    for shelf in &model.shelves {
        let shelf_id = shelf.id;
        let member = model.shelf_ids.contains(&shelf_id);
        let s = sender.clone();
        shelves_menu.add_check_item(&shelf.name, member, move |checked| {
            s.input(BookPageMsg::SetShelf {
                shelf_id,
                member: checked,
            });
        });
    }
    if !model.shelves.is_empty() {
        shelves_menu.add_separator();
    }
    let s = sender.clone();
    shelves_menu.add_item("New shelf…", move || {
        s.input(BookPageMsg::NewShelf);
    });

    let menu = menu_popover(MenuWidth::Wide);
    menu.add_submenu("Shelves", shelves_menu);
    if matches!(book.format, BookFormat::Epub) {
        let s = sender.clone();
        menu.add_item("Edit book…", move || {
            s.input(BookPageMsg::EditBook);
        });
    }
    if matches!(book.format, BookFormat::Cbz | BookFormat::Cbr) {
        let s = sender.clone();
        menu.add_item("Remaster…", move || {
            s.input(BookPageMsg::RemasterComic);
        });
    }
    let s = sender.clone();
    menu.add_item("Show in file manager", move || {
        s.input(BookPageMsg::LocateFile);
    });
    menu.add_separator();
    let s = sender.clone();
    menu.add_destructive_item("Remove from library", move || {
        s.input(BookPageMsg::ConfirmDelete);
    });

    let more = menu_button("view-more-symbolic", "More actions", menu);
    more.add_css_class("kalam-bookpage-more");
    more.set_size_request(44, 44);
    host.append(&more);
}

// ---------------------------------------------------------------------------
// Sections: Contents, Annotations, file line
// ---------------------------------------------------------------------------

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

/// Per-chapter read state, derived from the reading position. Kalam
/// stores no per-chapter completion dates (reading events are per
/// book), so read rows deliberately carry no date rather than an
/// invented one — the audit's §5.4 flag.
fn chapter_states(model: &BookPageModel, chapters: &[String]) -> Vec<TimelineState> {
    let Some(book) = &model.book else {
        return Vec::new();
    };
    if model.finished || book.progress >= 100 {
        return vec![TimelineState::Read; chapters.len()];
    }
    let started = model.progress.is_some() || book.progress > 0;
    if !started {
        return vec![TimelineState::Upcoming; chapters.len()];
    }
    let current = model
        .progress
        .map(|(ci, _)| ci)
        .unwrap_or(model.stats.chapter_index)
        .min(chapters.len().saturating_sub(1));
    (0..chapters.len())
        .map(|i| {
            if i < current {
                TimelineState::Read
            } else if i == current {
                TimelineState::Current
            } else {
                TimelineState::Upcoming
            }
        })
        .collect()
}

fn contents_entries(model: &BookPageModel, chapters: &[String]) -> Vec<TimelineEntry> {
    let states = chapter_states(model, chapters);
    let mut bookmark_counts: HashMap<usize, u32> = HashMap::new();
    for bookmark in &model.bookmarks {
        if let Ok(index) = usize::try_from(bookmark.chapter_index) {
            *bookmark_counts.entry(index).or_default() += 1;
        }
    }
    let progress_pct = model.book.as_ref().map(|b| b.progress).unwrap_or(0);

    chapters
        .iter()
        .enumerate()
        .map(|(i, title)| {
            let state = states.get(i).copied().unwrap_or(TimelineState::Upcoming);
            let title = title.trim();
            let mut entry = TimelineEntry::new(
                if title.is_empty() {
                    format!("Chapter {}", i + 1)
                } else {
                    title.to_string()
                },
                state,
            )
            .with_subtitle_first()
            .with_bookmarks(bookmark_counts.get(&i).copied().unwrap_or(0));
            // A spine whose titles are literally "Chapter N" would read
            // the same line twice; the kicker only earns its place over
            // a real chapter title.
            if !title.is_empty() && !title.eq_ignore_ascii_case(&format!("Chapter {}", i + 1)) {
                entry = entry.with_subtitle(format!("Chapter {}", i + 1));
            }
            if state == TimelineState::Current {
                entry = entry.with_right(format!("{progress_pct}%"));
            }
            entry
        })
        .collect()
}

fn link_is_accent(before: TimelineState, after: TimelineState) -> bool {
    before != TimelineState::Upcoming && after != TimelineState::Upcoming
}

fn fill_contents(
    host: &gtk::Box,
    model: &BookPageModel,
    chapters: &[String],
    sender: &ComponentSender<BookPageModel>,
) {
    clear_box(host);
    if model.book.is_none() {
        host.set_visible(false);
        return;
    }
    if chapters.is_empty() {
        // EPUBs still parsing keep the section with a quiet line; a
        // format with no chapter list (PDF, comic) has no Contents.
        host.set_visible(model.chapters_loading);
        if model.chapters_loading {
            let section = outlined_section("Contents");
            section
                .body()
                .append(&quiet_notice("Loading chapter list…"));
            host.append(&section.widget());
        }
        return;
    }
    host.set_visible(true);

    let section = outlined_section("Contents");
    let entries = contents_entries(model, chapters);

    if model.contents_expanded {
        // Fixed height: 8 rows of 64px scroll inside (decision 89).
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::External);
        scroll.set_size_request(-1, 512);
        scroll.set_child(Some(&timeline(&entries)));
        section.body().append(&scroll);
    } else {
        // Five rows, windowed so the current chapter is among them.
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let start = if entries.len() <= 5 {
            0
        } else if let Some(current) = entries
            .iter()
            .position(|e| e.state == TimelineState::Current)
        {
            current.saturating_sub(2).min(entries.len() - 5)
        } else {
            0
        };
        for index in start..(start + 5).min(entries.len()) {
            let entry = &entries[index];
            let upper = (index > 0).then(|| link_is_accent(entries[index - 1].state, entry.state));
            let lower = entries
                .get(index + 1)
                .map(|next| link_is_accent(entry.state, next.state));
            list.append(&timeline_row(entry, TimelineLinks { upper, lower }));
        }
        section.body().append(&list);
    }

    if entries.len() > 5 {
        let toggle_label = if model.contents_expanded {
            "Show less".to_string()
        } else {
            format!("Show all {}", entries.len())
        };
        let toggle = gtk::Button::with_label(&toggle_label);
        toggle.add_css_class("kalam-detail-card-link");
        toggle.set_focus_on_click(false);
        toggle.set_halign(gtk::Align::Start);
        let s = sender.clone();
        toggle.connect_clicked(move |_| s.input(BookPageMsg::ToggleJourney));
        section.body().append(&toggle);
    }

    host.append(&section.widget());
}

fn count_noun(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}

fn fill_annotations(
    host: &gtk::Box,
    model: &BookPageModel,
    chapters: &[String],
    sender: &ComponentSender<BookPageModel>,
) {
    clear_box(host);
    if model.book.is_none() {
        host.set_visible(false);
        return;
    }
    let total = model.annotations.len() + model.bookmarks.len() + model.saved_words.len();
    if total == 0 {
        host.set_visible(false);
        return;
    }
    host.set_visible(true);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let heading = gtk::Label::new(Some("Annotations"));
    heading.add_css_class("kalam-ui-section-title");
    heading.set_halign(gtk::Align::Start);
    heading.set_hexpand(true);
    header.append(&heading);
    let see_all = gtk::Button::with_label(&format!("See all {total}"));
    see_all.add_css_class("kalam-detail-card-link");
    see_all.set_focus_on_click(false);
    let s = sender.clone();
    see_all.connect_clicked(move |_| s.input(BookPageMsg::ViewHighlights));
    header.append(&see_all);
    host.append(&header);

    // Two highlights on the page (decision 92); the rest live behind
    // "See all".
    for anno in model
        .annotations
        .iter()
        .filter(|a| !a.text_excerpt.trim().is_empty())
        .take(2)
    {
        host.append(&hairline());
        let item = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        let bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
        bar.add_css_class("kalam-hl-bar");
        bar.add_css_class(hl_color_class(&anno.color));
        bar.set_valign(gtk::Align::Fill);
        item.append(&bar);

        let info = gtk::Box::new(gtk::Orientation::Vertical, 3);
        info.set_hexpand(true);
        let text = gtk::Label::new(Some(anno.text_excerpt.trim()));
        text.add_css_class("kalam-bookpage-annotation-quote");
        text.set_halign(gtk::Align::Start);
        text.set_wrap(true);
        text.set_xalign(0.0);
        text.set_lines(3);
        text.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.set_max_width_chars(52);
        info.append(&text);

        let mut meta_parts = vec![chapter_label(chapters, anno.chapter_index)];
        let note = anno.note.trim();
        if !note.is_empty() {
            meta_parts.push(shorten_title(note, 90));
        }
        let meta = gtk::Label::new(Some(&meta_parts.join(" · ")));
        meta.add_css_class("kalam-ui-meta");
        meta.set_halign(gtk::Align::Start);
        meta.set_wrap(true);
        meta.set_xalign(0.0);
        meta.set_max_width_chars(52);
        info.append(&meta);
        item.append(&info);
        host.append(&item);
    }

    let mut counts: Vec<String> = Vec::new();
    if !model.annotations.is_empty() {
        counts.push(count_noun(
            model.annotations.len(),
            "highlight",
            "highlights",
        ));
    }
    if !model.bookmarks.is_empty() {
        counts.push(count_noun(model.bookmarks.len(), "bookmark", "bookmarks"));
    }
    if !model.saved_words.is_empty() {
        counts.push(count_noun(
            model.saved_words.len(),
            "saved word",
            "saved words",
        ));
    }
    let count_line = gtk::Label::new(Some(&counts.join(" · ")));
    count_line.add_css_class("kalam-ui-meta");
    count_line.set_halign(gtk::Align::Start);
    host.append(&count_line);
}

/// What one Ctrl+Z in the Tag dialog walks back (decision 84's "instant
/// apply, Ctrl+Z undoes"): this book's own tag adds and removes from this
/// sitting. Library-wide rename/merge/delete are confirmed up front with
/// their book counts instead, and clear this history — a deleted tag's
/// inverse is not this dialog's to promise.
enum TagUndo {
    Added(String),
    Removed(String),
}

fn books_noun(count: i64) -> &'static str {
    if count == 1 {
        "book"
    } else {
        "books"
    }
}

fn tags_on_book(service: &LibraryService, book_id: i64) -> Vec<String> {
    service
        .catalog()
        .get_book(book_id)
        .ok()
        .flatten()
        .map(|b| b.tags)
        .unwrap_or_default()
}

fn tags_section_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(&text.to_ascii_uppercase()));
    label.add_css_class("kalam-ui-field-label");
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    label
}

fn tags_meta_note(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-ui-meta");
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_max_width_chars(53);
    label
}

fn tags_flow() -> gtk::FlowBox {
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_homogeneous(false);
    flow.set_row_spacing(6);
    flow.set_column_spacing(6);
    flow
}

/// Chips scroll inside the dialog past three rows (28px chips, 6px row
/// spacing) instead of growing the 420 form off the bottom of the window.
fn tags_flow_scroll(flow: &gtk::FlowBox) -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::External);
    scroll.set_propagate_natural_height(true);
    scroll.set_max_content_height(3 * 28 + 2 * 6);
    scroll.set_child(Some(flow));
    scroll
}

/// An "On this book" chip: the standard chip with its remove × (decision 84).
fn tag_remove_chip(name: &str, on_remove: impl Fn() + 'static) -> gtk::Button {
    let content = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    content.append(&gtk::Label::new(Some(name)));
    content.append(&gtk::Label::new(Some("×")));
    let chip = gtk::Button::new();
    chip.add_css_class("kalam-ui-chip");
    chip.set_child(Some(&content));
    chip.set_halign(gtk::Align::Start);
    chip.set_valign(gtk::Align::Center);
    chip.set_tooltip_text(Some("Remove tag"));
    chip.set_focus_on_click(false);
    chip.connect_clicked(move |_| on_remove());
    chip
}

/// A "Suggested from the file" chip: one tap puts the subject on the book.
fn tag_add_chip(name: &str, on_add: impl Fn() + 'static) -> gtk::Button {
    let chip = gtk::Button::with_label(&format!("+ {name}"));
    chip.add_css_class("kalam-ui-chip");
    chip.set_halign(gtk::Align::Start);
    chip.set_valign(gtk::Align::Center);
    chip.set_tooltip_text(Some("Add tag"));
    chip.set_focus_on_click(false);
    chip.connect_clicked(move |_| on_add());
    chip
}

/// The designed Tag dialog (decision 84): a 420 form dialog that applies
/// instantly — type to add (with library autocomplete and a Create row),
/// tap a chip's × to remove, tick a library row to toggle. Each library
/// row's ⋯ carries the library-wide actions (rename, merge, delete),
/// each confirmed with its book count first.
///
/// Like the panel it replaces, the dialog re-reads the catalog on every
/// rebuild: with instant apply the database *is* the state, and a rename
/// or merge then shows up without any local bookkeeping.
fn present_tags_dialog(
    anchor: &gtk::Box,
    model: &BookPageModel,
    sender: &ComponentSender<BookPageModel>,
) {
    let Some(book) = &model.book else {
        return;
    };
    let book_id = book.id;
    let service = model.service.clone();
    // The subjects the file itself declares, read once on open; only
    // EPUBs expose them today. Which of them still need offering is
    // decided per rebuild, against the tags the book currently carries.
    let file_subjects: Rc<Vec<String>> = Rc::new(if book.format == BookFormat::Epub {
        crate::epub::epub_subjects(&book.file_path)
    } else {
        Vec::new()
    });

    let shell = dialog_shell(DialogSize::Form, Some(&book.title));
    let body = shell.body();
    let anchor_widget: gtk::Widget = shell.widget().upcast();

    let entry = gtk::Entry::new();
    entry.add_css_class("kalam-ui-field");
    entry.set_placeholder_text(Some("Add a tag…"));
    entry.set_hexpand(true);
    body.append(&entry);

    // Sections group their label with their content at XS spacing; the
    // shell body's own MD spacing then separates the sections.
    let on_book_section = gtk::Box::new(gtk::Orientation::Vertical, crate::style::spacing::XS);
    on_book_section.append(&tags_section_label("On this book"));
    let on_book_flow = tags_flow();
    let on_book_scroll = tags_flow_scroll(&on_book_flow);
    on_book_section.append(&on_book_scroll);
    let on_book_empty = tags_meta_note("No tags yet — add one above.");
    on_book_section.append(&on_book_empty);
    body.append(&on_book_section);

    let suggested_section = gtk::Box::new(gtk::Orientation::Vertical, crate::style::spacing::XS);
    suggested_section.append(&tags_section_label("Suggested from the file"));
    let suggested_flow = tags_flow();
    let suggested_scroll = tags_flow_scroll(&suggested_flow);
    suggested_section.append(&suggested_scroll);
    body.append(&suggested_section);

    let library_section = gtk::Box::new(gtk::Orientation::Vertical, crate::style::spacing::XS);
    let library_head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let library_label = tags_section_label("Your library");
    library_label.set_hexpand(true);
    library_label.set_valign(gtk::Align::Center);
    library_head.append(&library_label);
    library_section.append(&library_head);

    let list_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let list_scroll = gtk::ScrolledWindow::new();
    list_scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::External);
    list_scroll.set_propagate_natural_height(true);
    // Six 44px rows; a long library scrolls here, not by growing the form.
    list_scroll.set_max_content_height(6 * 44);
    list_scroll.set_child(Some(&list_host));
    library_section.append(&list_scroll);
    let library_empty = tags_meta_note("No tags in your library yet.");
    library_section.append(&library_empty);
    body.append(&library_section);

    let sort_az = Rc::new(Cell::new(false));
    let undo_stack: Rc<RefCell<Vec<TagUndo>>> = Rc::new(RefCell::new(Vec::new()));
    let holder: crate::pages::SelfRebuild = Rc::new(RefCell::new(None));

    // The one write path for this book's tags: apply, remember the inverse
    // for Ctrl+Z (unless this call *is* the undo), rebuild.
    let apply_membership: Rc<dyn Fn(String, bool, bool)> = Rc::new({
        let service = service.clone();
        let undo_stack = undo_stack.clone();
        let holder = holder.clone();
        move |name: String, add: bool, record: bool| {
            let result = if add {
                service.catalog().add_book_tag(book_id, &name)
            } else {
                service.catalog().remove_book_tag(book_id, &name)
            };
            match result {
                Ok(()) => {
                    if record {
                        undo_stack.borrow_mut().push(if add {
                            TagUndo::Added(name)
                        } else {
                            TagUndo::Removed(name)
                        });
                    }
                }
                Err(err) => crate::notify::error(
                    if add {
                        "Could not add the tag"
                    } else {
                        "Could not remove the tag"
                    },
                    &err.to_string(),
                ),
            }
            if let Some(rebuild) = holder.borrow().clone() {
                rebuild();
            }
        }
    });

    // After a library-wide rename/merge/delete: the undo history may name
    // tags that no longer exist, so it goes, and the dialog re-reads.
    let after_library_op: Rc<dyn Fn()> = Rc::new({
        let undo_stack = undo_stack.clone();
        let holder = holder.clone();
        move || {
            undo_stack.borrow_mut().clear();
            if let Some(rebuild) = holder.borrow().clone() {
                rebuild();
            }
        }
    });

    let rebuild: Rc<dyn Fn()> = Rc::new({
        let service = service.clone();
        let on_book_flow = on_book_flow.clone();
        let on_book_scroll = on_book_scroll.clone();
        let on_book_empty = on_book_empty.clone();
        let suggested_section = suggested_section.clone();
        let suggested_flow = suggested_flow.clone();
        let list_host = list_host.clone();
        let list_scroll = list_scroll.clone();
        let library_empty = library_empty.clone();
        let sort_az = sort_az.clone();
        let file_subjects = file_subjects.clone();
        let apply = apply_membership.clone();
        let after_library_op = after_library_op.clone();
        let anchor_widget = anchor_widget.clone();
        move || {
            let on_book = tags_on_book(&service, book_id);
            let mut library = service
                .catalog()
                .list_tags_with_counts()
                .unwrap_or_default();
            if sort_az.get() {
                library.sort_by_cached_key(|(name, _)| name.to_lowercase());
            }
            let has = |name: &str| on_book.iter().any(|t| t.eq_ignore_ascii_case(name));

            while let Some(child) = on_book_flow.first_child() {
                on_book_flow.remove(&child);
            }
            for tag in &on_book {
                let apply = apply.clone();
                let name = tag.clone();
                on_book_flow.append(&tag_remove_chip(tag, move || {
                    apply(name.clone(), false, true);
                }));
            }
            on_book_scroll.set_visible(!on_book.is_empty());
            on_book_empty.set_visible(on_book.is_empty());

            let mut suggested: Vec<String> = Vec::new();
            for subject in file_subjects.iter() {
                let subject = subject.trim();
                if subject.is_empty()
                    || has(subject)
                    || suggested
                        .iter()
                        .any(|s: &String| s.eq_ignore_ascii_case(subject))
                {
                    continue;
                }
                suggested.push(subject.to_string());
            }
            while let Some(child) = suggested_flow.first_child() {
                suggested_flow.remove(&child);
            }
            for subject in &suggested {
                let apply = apply.clone();
                let name = subject.clone();
                suggested_flow.append(&tag_add_chip(subject, move || {
                    apply(name.clone(), true, true);
                }));
            }
            suggested_section.set_visible(!suggested.is_empty());

            clear_box(&list_host);
            for (name, count) in &library {
                let on = has(name);
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);

                let toggle = gtk::Button::new();
                toggle.add_css_class("kalam-bookpage-tag-row");
                toggle.set_hexpand(true);
                toggle.set_halign(gtk::Align::Fill);
                let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                let tick = gtk::Label::new(Some("✓"));
                tick.add_css_class("kalam-ui-menu-check");
                if on {
                    tick.add_css_class("checked");
                }
                tick.set_size_request(22, 22);
                tick.set_halign(gtk::Align::Center);
                tick.set_valign(gtk::Align::Center);
                tick.set_xalign(0.5);
                tick.set_yalign(0.5);
                content.append(&tick);
                let name_label = gtk::Label::new(Some(name));
                name_label.set_halign(gtk::Align::Start);
                name_label.set_hexpand(true);
                name_label.set_xalign(0.0);
                name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                content.append(&name_label);
                let count_label = gtk::Label::new(Some(&count.to_string()));
                count_label.add_css_class("kalam-ui-meta");
                count_label.set_valign(gtk::Align::Center);
                content.append(&count_label);
                toggle.set_child(Some(&content));
                let apply = apply.clone();
                let tag_name = name.clone();
                toggle.connect_clicked(move |_| {
                    apply(tag_name.clone(), !on, true);
                });
                row.append(&toggle);

                let menu = menu_popover(MenuWidth::Short);
                let count = *count;
                menu.add_item("Rename everywhere", {
                    let anchor = anchor_widget.clone();
                    let service = service.clone();
                    let tag_name = name.clone();
                    let done = after_library_op.clone();
                    move || {
                        present_tag_rename_dialog(
                            &anchor,
                            service.clone(),
                            tag_name.clone(),
                            count,
                            done.clone(),
                        );
                    }
                });
                menu.add_item("Merge into…", {
                    let anchor = anchor_widget.clone();
                    let service = service.clone();
                    let tag_name = name.clone();
                    let done = after_library_op.clone();
                    move || {
                        present_tag_merge_picker(
                            &anchor,
                            service.clone(),
                            tag_name.clone(),
                            count,
                            done.clone(),
                        );
                    }
                });
                menu.add_separator();
                menu.add_destructive_item("Delete from library", {
                    let anchor = anchor_widget.clone();
                    let service = service.clone();
                    let tag_name = name.clone();
                    let done = after_library_op.clone();
                    move || {
                        present_tag_delete_confirm(
                            &anchor,
                            service.clone(),
                            tag_name.clone(),
                            count,
                            done.clone(),
                        );
                    }
                });
                let more = menu_button("view-more-symbolic", "Tag actions", menu);
                more.set_valign(gtk::Align::Center);
                row.append(&more);
                list_host.append(&row);
            }
            list_scroll.set_visible(!library.is_empty());
            library_empty.set_visible(library.is_empty());
        }
    });
    *holder.borrow_mut() = Some(rebuild.clone());

    let sort_switch = segmented_control(&["Most used", "A–Z"], 0, SegmentedSize::Small, {
        let sort_az = sort_az.clone();
        let holder = holder.clone();
        move |index| {
            sort_az.set(index == 1);
            if let Some(rebuild) = holder.borrow().clone() {
                rebuild();
            }
        }
    });
    library_head.append(&sort_switch.widget());

    // Autocomplete: the menu popover reused as a completion list under the
    // field. `autohide` stays off so typing never loses the field; the
    // list is rebuilt per keystroke and dismissed by choosing, clearing
    // the field, or activating it.
    let completion = menu_popover(MenuWidth::Wide);
    completion.popover().set_autohide(false);
    completion.popover().set_parent(&entry);
    let refresh_completion: Rc<dyn Fn()> = Rc::new({
        let entry = entry.clone();
        let completion = completion.clone();
        let service = service.clone();
        let apply = apply_membership.clone();
        move || {
            let query = entry.text().to_string();
            let query = query.trim().to_string();
            let list = completion.list();
            while let Some(child) = list.first_child() {
                list.remove(&child);
            }
            if query.is_empty() {
                completion.popover().popdown();
                return;
            }
            let on_book = tags_on_book(&service, book_id);
            let library = service
                .catalog()
                .list_tags_with_counts()
                .unwrap_or_default();
            let query_lc = query.to_lowercase();
            let exact = library.iter().any(|(n, _)| n.eq_ignore_ascii_case(&query));
            let matches: Vec<(String, i64)> = library
                .iter()
                .filter(|(n, _)| {
                    n.to_lowercase().contains(&query_lc)
                        && !on_book.iter().any(|t| t.eq_ignore_ascii_case(n))
                })
                .take(8)
                .cloned()
                .collect();
            if matches.is_empty() && exact {
                // The typed tag exists and is already on this book (or no
                // suggestion would add anything): nothing to offer.
                completion.popover().popdown();
                return;
            }
            let width = entry.width();
            if width > 0 {
                list.set_size_request(width, -1);
            }
            for (name, count) in &matches {
                let apply = apply.clone();
                let entry = entry.clone();
                let tag_name = name.to_string();
                completion.add_item_with_count(
                    name,
                    usize::try_from(*count).unwrap_or(0),
                    move || {
                        apply(tag_name.clone(), true, true);
                        entry.set_text("");
                    },
                );
            }
            if !exact {
                let apply = apply.clone();
                let entry = entry.clone();
                let new_name = query.clone();
                completion.add_item(&format!("Create \"{query}\""), move || {
                    apply(new_name.clone(), true, true);
                    entry.set_text("");
                });
            }
            completion.popover().popup();
        }
    });
    entry.connect_changed({
        let refresh = refresh_completion.clone();
        move |_| refresh()
    });
    entry.connect_activate({
        let service = service.clone();
        let apply = apply_membership.clone();
        let completion = completion.clone();
        move |entry| {
            let query = entry.text().to_string();
            let query = query.trim();
            if query.is_empty() {
                return;
            }
            let on_book = tags_on_book(&service, book_id);
            if on_book.iter().any(|t| t.eq_ignore_ascii_case(query)) {
                completion.popover().popdown();
                return;
            }
            // An existing tag keeps its library casing; a new one is
            // created as typed.
            let name = service
                .catalog()
                .list_tags_with_counts()
                .unwrap_or_default()
                .into_iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(query))
                .map(|(n, _)| n)
                .unwrap_or_else(|| query.to_string());
            apply(name, true, true);
            entry.set_text("");
        }
    });

    // Ctrl+Z (decision 84), capture-phase so it answers wherever focus is
    // in the dialog — except the field, where it stays the entry's own
    // text undo.
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed({
        let entry = entry.clone();
        let undo_stack = undo_stack.clone();
        let apply = apply_membership.clone();
        move |_, keyval, _, state| {
            let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            if !ctrl || (keyval != gtk::gdk::Key::z && keyval != gtk::gdk::Key::Z) {
                return gtk::glib::Propagation::Proceed;
            }
            if entry.has_focus() {
                return gtk::glib::Propagation::Proceed;
            }
            if let Some(op) = undo_stack.borrow_mut().pop() {
                match op {
                    TagUndo::Added(name) => apply(name, false, false),
                    TagUndo::Removed(name) => apply(name, true, false),
                }
            }
            gtk::glib::Propagation::Stop
        }
    });
    shell.widget().add_controller(keys);

    let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
    shell.footer().append(&primary_button("Done", {
        let slot = slot.clone();
        move || {
            if let Some(dialog) = slot.borrow().clone() {
                dialog.close();
            }
        }
    }));

    rebuild();
    match present_dialog(anchor, "Tags", DialogExit::OwnButtons, &shell) {
        Some(dialog) => {
            *slot.borrow_mut() = Some(dialog);
            entry.grab_focus();
        }
        None => {
            crate::notify::error("Could not open tags", "The dialog could not be opened.");
            return;
        }
    }

    // Every exit — Done, Esc, backdrop click — unmaps the shell. Refresh
    // the page once on the way out so its chips match what instant apply
    // already wrote. Sent on the raw channel: if the page went away under
    // the dialog, the message drops quietly instead of panicking.
    let input = sender.input_sender().clone();
    let refreshed = Rc::new(Cell::new(false));
    shell.widget().connect_unmap(move |_| {
        if !refreshed.replace(true) {
            let _ = input.send(BookPageMsg::Refresh);
        }
    });
}

/// Rename a tag on every book that carries it. Renaming onto a name that
/// already exists is a merge in disguise, so that path detours through
/// the merge confirmation (decision 84: library-wide actions confirm
/// with counts).
fn present_tag_rename_dialog(
    anchor: &gtk::Widget,
    service: LibraryService,
    old_name: String,
    count: i64,
    on_done: Rc<dyn Fn()>,
) {
    let shell = dialog_shell(DialogSize::Form, None);
    let field = text_field("Tag name", "Tag name", &old_name);
    shell.body().append(&field.widget());
    shell.body().append(&tags_meta_note(&format!(
        "On {} {} — the new name applies to all of them.",
        count,
        books_noun(count),
    )));

    let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
    let close: Rc<dyn Fn()> = Rc::new({
        let slot = slot.clone();
        move || {
            if let Some(dialog) = slot.borrow().clone() {
                dialog.close();
            }
        }
    });
    shell.footer().append(&boxed_ghost_button("Cancel", {
        let close = close.clone();
        move || close()
    }));
    let save: Rc<dyn Fn()> = Rc::new({
        let field = field.clone();
        let service = service.clone();
        let anchor = anchor.clone();
        let old_name = old_name.clone();
        let on_done = on_done.clone();
        let close = close.clone();
        move || {
            let new_name = field.value();
            let new_name = new_name.trim().to_string();
            if new_name.is_empty() || new_name == old_name {
                close();
                return;
            }
            let existing = service
                .catalog()
                .list_tags_with_counts()
                .unwrap_or_default()
                .into_iter()
                .find(|(n, _)| {
                    !n.eq_ignore_ascii_case(&old_name) && n.eq_ignore_ascii_case(&new_name)
                });
            close();
            if let Some((target, _)) = existing {
                present_tag_merge_confirm(
                    &anchor,
                    service.clone(),
                    old_name.clone(),
                    target,
                    count,
                    on_done.clone(),
                );
                return;
            }
            match service.catalog().rename_tag(&old_name, &new_name) {
                Ok(()) => {
                    crate::notify::compact(
                        "Tag renamed",
                        &format!("\"{old_name}\" is now \"{new_name}\"."),
                    );
                    on_done();
                }
                Err(err) => {
                    crate::notify::error("Could not rename the tag", &err.to_string());
                }
            }
        }
    });
    shell.footer().append(&primary_button("Rename", {
        let save = save.clone();
        move || save()
    }));
    field.entry().connect_activate(move |_| save());

    match present_dialog(anchor, "Rename tag", DialogExit::OwnButtons, &shell) {
        Some(dialog) => {
            *slot.borrow_mut() = Some(dialog);
            field.entry().grab_focus();
            field.entry().select_region(0, -1);
        }
        None => crate::notify::error(
            "Could not rename the tag",
            "The dialog could not be opened.",
        ),
    }
}

/// Pick which tag `source` merges into. (The picker was not drawn; this
/// is the library list at its plainest — name and count, most used
/// first.) Choosing one still asks through the merge confirmation.
fn present_tag_merge_picker(
    anchor: &gtk::Widget,
    service: LibraryService,
    source: String,
    source_count: i64,
    on_done: Rc<dyn Fn()>,
) {
    let others: Vec<(String, i64)> = service
        .catalog()
        .list_tags_with_counts()
        .unwrap_or_default()
        .into_iter()
        .filter(|(n, _)| !n.eq_ignore_ascii_case(&source))
        .collect();
    let shell = dialog_shell(
        DialogSize::Form,
        Some(&format!(
            "\"{source}\" · on {} {}",
            source_count,
            books_noun(source_count),
        )),
    );

    let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
    let close: Rc<dyn Fn()> = Rc::new({
        let slot = slot.clone();
        move || {
            if let Some(dialog) = slot.borrow().clone() {
                dialog.close();
            }
        }
    });

    if others.is_empty() {
        shell
            .body()
            .append(&tags_meta_note("There are no other tags to merge into."));
    } else {
        let list_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for (name, count) in &others {
            let row = gtk::Button::new();
            row.add_css_class("kalam-bookpage-tag-row");
            row.set_hexpand(true);
            row.set_halign(gtk::Align::Fill);
            let content = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            let name_label = gtk::Label::new(Some(name));
            name_label.set_halign(gtk::Align::Start);
            name_label.set_hexpand(true);
            name_label.set_xalign(0.0);
            name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            content.append(&name_label);
            let count_label = gtk::Label::new(Some(&count.to_string()));
            count_label.add_css_class("kalam-ui-meta");
            count_label.set_valign(gtk::Align::Center);
            content.append(&count_label);
            row.set_child(Some(&content));
            row.connect_clicked({
                let anchor = anchor.clone();
                let service = service.clone();
                let source = source.clone();
                let target = name.clone();
                let on_done = on_done.clone();
                let close = close.clone();
                move |_| {
                    close();
                    present_tag_merge_confirm(
                        &anchor,
                        service.clone(),
                        source.clone(),
                        target.clone(),
                        source_count,
                        on_done.clone(),
                    );
                }
            });
            list_host.append(&row);
        }
        let scroll = gtk::ScrolledWindow::new();
        scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::External);
        scroll.set_propagate_natural_height(true);
        scroll.set_max_content_height(6 * 44);
        scroll.set_child(Some(&list_host));
        shell.body().append(&scroll);
    }

    shell.footer().append(&boxed_ghost_button("Cancel", {
        let close = close.clone();
        move || close()
    }));
    match present_dialog(anchor, "Merge into…", DialogExit::OwnButtons, &shell) {
        Some(dialog) => *slot.borrow_mut() = Some(dialog),
        None => crate::notify::error("Could not merge the tag", "The dialog could not be opened."),
    }
}

fn present_tag_merge_confirm(
    anchor: &gtk::Widget,
    service: LibraryService,
    source: String,
    target: String,
    source_count: i64,
    on_done: Rc<dyn Fn()>,
) {
    let title = format!("Merge \"{source}\" into \"{target}\"?");
    let message = format!(
        "{} {} tagged \"{}\". They will be tagged \"{}\" instead, and \"{}\" will be removed.",
        source_count,
        if source_count == 1 {
            "book is"
        } else {
            "books are"
        },
        source,
        target,
        source,
    );
    let _ = confirmation_dialog(
        anchor,
        &title,
        &message,
        "Cancel",
        "Merge",
        ConfirmationKind::Primary,
        move || match service.catalog().merge_tags(&source, &target) {
            Ok(()) => {
                crate::notify::compact(
                    "Tags merged",
                    &format!("\"{source}\" merged into \"{target}\"."),
                );
                on_done();
            }
            Err(err) => {
                crate::notify::error("Could not merge the tags", &err.to_string());
            }
        },
    );
}

/// The designed delete-tag confirmation: the count in the body, ghost
/// Cancel, accent-fill Delete tag (decision 84's wording).
fn present_tag_delete_confirm(
    anchor: &gtk::Widget,
    service: LibraryService,
    name: String,
    count: i64,
    on_done: Rc<dyn Fn()>,
) {
    let title = format!("Delete \"{name}\"?");
    let message = format!(
        "It will be removed from {} {}. The books themselves are not changed.",
        count,
        books_noun(count),
    );
    let _ = confirmation_dialog(
        anchor,
        &title,
        &message,
        "Cancel",
        "Delete tag",
        ConfirmationKind::Primary,
        move || match service.catalog().delete_tag(&name) {
            Ok(()) => {
                crate::notify::compact(
                    "Tag deleted",
                    &format!("\"{name}\" removed from the library."),
                );
                on_done();
            }
            Err(err) => {
                crate::notify::error("Could not delete the tag", &err.to_string());
            }
        },
    );
}

/// The designed Annotations dialog (decision 92): a 560×660 list dialog
/// with Highlights / Bookmarks / Words tabs. It replaces the old
/// highlights-only floating panel — the page's "See all N" counts all
/// three kinds, so the thing it opens has to show all three.
///
/// The lists are built from the page snapshot's rows (cloned into local
/// state); a delete removes the row locally and in the catalog, and
/// Done sends the page a Refresh so its counts catch up.
fn present_annotations_dialog(
    anchor: &gtk::Box,
    model: &BookPageModel,
    chapters: Vec<String>,
    sender: &ComponentSender<BookPageModel>,
) {
    let Some(book) = &model.book else {
        return;
    };
    let service = model.service.clone();
    let shell = dialog_shell(DialogSize::List, Some(&book.title));

    let stack = gtk::Stack::new();
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.add_named(
        &annotations_highlights_page(
            service.clone(),
            Rc::new(RefCell::new(model.annotations.clone())),
            chapters.clone(),
        ),
        Some("highlights"),
    );
    stack.add_named(
        &annotations_bookmarks_page(
            service.clone(),
            Rc::new(RefCell::new(model.bookmarks.clone())),
            chapters.clone(),
        ),
        Some("bookmarks"),
    );
    stack.add_named(
        &annotations_words_page(
            service,
            Rc::new(RefCell::new(model.saved_words.clone())),
            chapters,
        ),
        Some("words"),
    );

    let labels = [
        format!("Highlights {}", model.annotations.len()),
        format!("Bookmarks {}", model.bookmarks.len()),
        format!("Words {}", model.saved_words.len()),
    ];
    let label_refs: Vec<&str> = labels.iter().map(String::as_str).collect();
    let stack_for_tabs = stack.clone();
    let tabs = segmented_control(&label_refs, 0, SegmentedSize::Panel, move |index| {
        stack_for_tabs.set_visible_child_name(match index {
            1 => "bookmarks",
            2 => "words",
            _ => "highlights",
        });
    });
    let tabs_widget = tabs.widget();
    tabs_widget.set_halign(gtk::Align::Center);
    shell.body().append(&tabs_widget);
    shell.body().append(&stack);

    let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));
    let slot_for_done = slot.clone();
    let s = sender.input_sender().clone();
    shell.footer().append(&primary_button("Done", move || {
        if let Some(dialog) = slot_for_done.borrow().clone() {
            dialog.close();
        }
        let _ = s.send(BookPageMsg::Refresh);
    }));

    match present_dialog(anchor, "Annotations", DialogExit::OwnButtons, &shell) {
        Some(dialog) => *slot.borrow_mut() = Some(dialog),
        None => crate::notify::error(
            "Could not open annotations",
            "The dialog could not be opened.",
        ),
    }
}

fn dialog_scroll(child: &gtk::Box) -> gtk::ScrolledWindow {
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_policy(gtk::PolicyType::Never, gtk::PolicyType::External);
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_child(Some(child));
    scroll
}

fn annotations_empty_note(host: &gtk::Box, text: &str) {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-ui-meta");
    label.set_halign(gtk::Align::Start);
    label.set_wrap(true);
    label.set_xalign(0.0);
    label.set_max_width_chars(58);
    host.append(&label);
}

fn annotations_highlights_page(
    service: LibraryService,
    annotations: Rc<RefCell<Vec<crate::db::Annotation>>>,
    chapters: Vec<String>,
) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_vexpand(true);
    let filter = Rc::new(Cell::new(None::<HighlightColor>));
    let list_host = gtk::Box::new(gtk::Orientation::Vertical, 10);

    // Self-referential rebuild, the panels' established pattern: delete
    // buttons and the colour filter re-run it through the holder slot.
    let holder: crate::pages::SelfRebuild = Rc::new(RefCell::new(None));
    let rebuild: Rc<dyn Fn()> = Rc::new({
        let host = list_host.clone();
        let annotations = annotations.clone();
        let filter = filter.clone();
        let chapters = chapters.clone();
        let service = service.clone();
        let holder = holder.clone();
        move || {
            clear_box(&host);
            let current = annotations.borrow();
            let shown: Vec<&crate::db::Annotation> = current
                .iter()
                .filter(|a| {
                    filter
                        .get()
                        .is_none_or(|c| HighlightColor::from_str_lossy(&a.color) == c)
                })
                .collect();
            if shown.is_empty() {
                annotations_empty_note(
                    &host,
                    if filter.get().is_some() {
                        "No highlights in this colour."
                    } else {
                        "No highlights yet — select some text in the reader."
                    },
                );
                return;
            }
            let mut by_chapter: BTreeMap<i64, Vec<&crate::db::Annotation>> = BTreeMap::new();
            for anno in shown {
                by_chapter.entry(anno.chapter_index).or_default().push(anno);
            }
            for (chapter_index, items) in by_chapter {
                let heading = gtk::Label::new(Some(
                    &chapter_label(&chapters, chapter_index).to_uppercase(),
                ));
                heading.add_css_class("kalam-ui-label");
                heading.set_halign(gtk::Align::Start);
                heading.set_ellipsize(gtk::pango::EllipsizeMode::End);
                host.append(&heading);

                for anno in items {
                    host.append(&hairline());
                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                    let bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
                    bar.add_css_class("kalam-hl-bar");
                    bar.add_css_class(hl_color_class(&anno.color));
                    bar.set_valign(gtk::Align::Fill);
                    row.append(&bar);

                    let info = gtk::Box::new(gtk::Orientation::Vertical, 3);
                    info.set_hexpand(true);
                    let text = gtk::Label::new(Some(anno.text_excerpt.trim()));
                    text.add_css_class("kalam-bookpage-annotation-quote");
                    text.set_halign(gtk::Align::Start);
                    text.set_wrap(true);
                    text.set_xalign(0.0);
                    text.set_max_width_chars(58);
                    info.append(&text);
                    let kind = match anno.kind.as_str() {
                        "quote" => "Quote",
                        "note" => "Note",
                        _ => "Highlight",
                    };
                    let mut meta_parts = vec![kind.to_string(), pretty_imported(&anno.created_at)];
                    let note = anno.note.trim();
                    if !note.is_empty() {
                        meta_parts.push(shorten_title(note, 90));
                    }
                    let meta = gtk::Label::new(Some(&meta_parts.join(" · ")));
                    meta.add_css_class("kalam-ui-meta");
                    meta.set_halign(gtk::Align::Start);
                    meta.set_wrap(true);
                    meta.set_xalign(0.0);
                    meta.set_max_width_chars(58);
                    info.append(&meta);
                    row.append(&info);

                    let id = anno.id;
                    let service = service.clone();
                    let annotations = annotations.clone();
                    let holder = holder.clone();
                    row.append(&icon_button(
                        "user-trash-symbolic",
                        "Delete highlight",
                        32,
                        false,
                        move || {
                            match service.catalog().delete_annotation(id) {
                                Ok(()) => {
                                    annotations.borrow_mut().retain(|a| a.id != id);
                                    crate::notify::compact("Annotation deleted", "");
                                }
                                Err(err) => crate::notify::error(
                                    "Could not delete the annotation",
                                    &err.to_string(),
                                ),
                            }
                            if let Some(refresh) = holder.borrow().clone() {
                                refresh();
                            }
                        },
                    ));
                    host.append(&row);
                }
            }
        }
    });
    *holder.borrow_mut() = Some(rebuild.clone());

    // Five colour dots as the filter (decision 92); tapping the active
    // dot again clears back to all colours.
    let filter_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let filter_label = gtk::Label::new(Some("Filter"));
    filter_label.add_css_class("kalam-ui-meta");
    filter_row.append(&filter_label);
    for &color in HighlightColor::SOFT_FIVE {
        let dot = gtk::Button::new();
        dot.add_css_class("k-color-dot");
        dot.add_css_class(&format!("k-color-dot-{}", color.as_str()));
        dot.set_size_request(24, 24);
        dot.set_hexpand(false);
        dot.set_halign(gtk::Align::Start);
        dot.set_valign(gtk::Align::Center);
        dot.set_tooltip_text(Some(&format!("Show {} highlights", color.as_str())));
        let filter = filter.clone();
        let holder = holder.clone();
        dot.connect_clicked(move |clicked| {
            let next = if filter.get() == Some(color) {
                None
            } else {
                Some(color)
            };
            filter.set(next);
            if let Some(parent) = clicked.parent() {
                let mut child = parent.first_child();
                while let Some(widget) = child {
                    let next_sibling = widget.next_sibling();
                    if let Ok(btn) = widget.downcast::<gtk::Button>() {
                        btn.remove_css_class("active");
                    }
                    child = next_sibling;
                }
            }
            if next.is_some() {
                clicked.add_css_class("active");
            }
            if let Some(refresh) = holder.borrow().clone() {
                refresh();
            }
        });
        filter_row.append(&dot);
    }
    root.append(&filter_row);
    root.append(&dialog_scroll(&list_host));

    rebuild();
    root
}

fn annotations_bookmarks_page(
    service: LibraryService,
    bookmarks: Rc<RefCell<Vec<crate::db::ReadingBookmark>>>,
    chapters: Vec<String>,
) -> gtk::Box {
    let list_host = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let holder: crate::pages::SelfRebuild = Rc::new(RefCell::new(None));
    let rebuild: Rc<dyn Fn()> = Rc::new({
        let host = list_host.clone();
        let bookmarks_state = bookmarks.clone();
        let chapters = chapters.clone();
        let service = service.clone();
        let holder = holder.clone();
        move || {
            clear_box(&host);
            let current = bookmarks_state.borrow();
            if current.is_empty() {
                annotations_empty_note(&host, "No bookmarks in this book yet.");
                return;
            }
            for bookmark in current.iter() {
                host.append(&hairline());
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                let icon = gtk::Image::from_icon_name("bookmark-new-symbolic");
                icon.set_pixel_size(15);
                icon.set_valign(gtk::Align::Center);
                row.append(&icon);

                let chapter = chapter_label(&chapters, bookmark.chapter_index);
                let label = bookmark.label.trim();
                let info = gtk::Box::new(gtk::Orientation::Vertical, 3);
                info.set_hexpand(true);
                let title = gtk::Label::new(Some(if label.is_empty() {
                    chapter.as_str()
                } else {
                    label
                }));
                title.add_css_class("kalam-ui-body");
                title.set_halign(gtk::Align::Start);
                title.set_wrap(true);
                title.set_xalign(0.0);
                title.set_max_width_chars(58);
                info.append(&title);
                let pct = (bookmark.fraction.clamp(0.0, 1.0) * 100.0).round() as i64;
                let meta_text = if label.is_empty() {
                    format!("{} · {pct}%", pretty_imported(&bookmark.created_at))
                } else {
                    format!(
                        "{chapter} · {} · {pct}%",
                        pretty_imported(&bookmark.created_at)
                    )
                };
                let meta = gtk::Label::new(Some(&meta_text));
                meta.add_css_class("kalam-ui-meta");
                meta.set_halign(gtk::Align::Start);
                meta.set_ellipsize(gtk::pango::EllipsizeMode::End);
                info.append(&meta);
                row.append(&info);

                let id = bookmark.id;
                let service = service.clone();
                let bookmarks_state = bookmarks_state.clone();
                let holder = holder.clone();
                row.append(&icon_button(
                    "user-trash-symbolic",
                    "Delete bookmark",
                    32,
                    false,
                    move || {
                        match service.catalog().delete_reading_bookmark(id) {
                            Ok(()) => {
                                bookmarks_state.borrow_mut().retain(|b| b.id != id);
                                crate::notify::compact("Bookmark deleted", "");
                            }
                            Err(err) => crate::notify::error(
                                "Could not delete the bookmark",
                                &err.to_string(),
                            ),
                        }
                        if let Some(refresh) = holder.borrow().clone() {
                            refresh();
                        }
                    },
                ));
                host.append(&row);
            }
        }
    });
    *holder.borrow_mut() = Some(rebuild.clone());
    rebuild();

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.set_vexpand(true);
    root.append(&dialog_scroll(&list_host));
    root
}

fn annotations_words_page(
    service: LibraryService,
    words: Rc<RefCell<Vec<crate::db::SavedWord>>>,
    chapters: Vec<String>,
) -> gtk::Box {
    let list_host = gtk::Box::new(gtk::Orientation::Vertical, 10);
    let holder: crate::pages::SelfRebuild = Rc::new(RefCell::new(None));
    let rebuild: Rc<dyn Fn()> = Rc::new({
        let host = list_host.clone();
        let words_state = words.clone();
        let chapters = chapters.clone();
        let service = service.clone();
        let holder = holder.clone();
        move || {
            clear_box(&host);
            let current = words_state.borrow();
            if current.is_empty() {
                annotations_empty_note(&host, "No saved words from this book yet.");
                return;
            }
            for word in current.iter() {
                host.append(&hairline());
                let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
                let info = gtk::Box::new(gtk::Orientation::Vertical, 3);
                info.set_hexpand(true);
                let headword = gtk::Label::new(Some(&word.word));
                headword.add_css_class("kalam-ui-card-title");
                headword.set_halign(gtk::Align::Start);
                headword.set_ellipsize(gtk::pango::EllipsizeMode::End);
                info.append(&headword);

                let meaning = word
                    .definition
                    .lines()
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .unwrap_or("");
                if !meaning.is_empty() {
                    let meaning_label = gtk::Label::new(Some(meaning));
                    meaning_label.add_css_class("kalam-ui-meta");
                    meaning_label.set_halign(gtk::Align::Start);
                    meaning_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    meaning_label.set_max_width_chars(58);
                    info.append(&meaning_label);
                }
                let mut meta_parts: Vec<String> = Vec::new();
                if let Some(chapter_index) = word.chapter_index {
                    meta_parts.push(chapter_label(&chapters, chapter_index));
                }
                if word.known {
                    meta_parts.push("Known".to_string());
                }
                if !meta_parts.is_empty() {
                    let meta = gtk::Label::new(Some(&meta_parts.join(" · ")));
                    meta.add_css_class("kalam-ui-caption");
                    meta.set_halign(gtk::Align::Start);
                    meta.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    info.append(&meta);
                }
                row.append(&info);

                let id = word.id;
                let word_text = word.word.clone();
                let service = service.clone();
                let words_state = words_state.clone();
                let holder = holder.clone();
                row.append(&icon_button(
                    "user-trash-symbolic",
                    "Delete saved word",
                    32,
                    false,
                    move || {
                        match service.catalog().delete_saved_word(id) {
                            Ok(()) => {
                                words_state.borrow_mut().retain(|w| w.id != id);
                                crate::notify::compact("Saved word deleted", &word_text);
                            }
                            Err(err) => crate::notify::error(
                                "Could not delete the saved word",
                                &err.to_string(),
                            ),
                        }
                        if let Some(refresh) = holder.borrow().clone() {
                            refresh();
                        }
                    },
                ));
                host.append(&row);
            }
        }
    });
    *holder.borrow_mut() = Some(rebuild.clone());
    rebuild();

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.set_vexpand(true);
    root.append(&dialog_scroll(&list_host));
    root
}

fn fill_file_line(host: &gtk::Box, model: &BookPageModel) {
    clear_box(host);
    let Some(book) = &model.book else {
        host.set_visible(false);
        return;
    };
    host.set_visible(true);

    let name = std::path::Path::new(&book.file_name)
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| book.file_name.clone());
    let size = match model.file_size {
        Some(bytes) => human_size(bytes),
        None if file_missing(model) => "file missing".to_string(),
        None => "size unknown".to_string(),
    };
    let text = format!(
        "{name} · {size} · Added {}",
        pretty_imported(&book.added_at)
    );
    let label = gtk::Label::new(Some(&text));
    label.add_css_class("kalam-ui-caption");
    label.set_halign(gtk::Align::Start);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    host.append(&label);

    let show = gtk::Button::with_label("Show in file manager");
    show.add_css_class("kalam-detail-card-link");
    show.set_focus_on_click(false);
    let path = book.file_path.clone();
    show.connect_clicked(move |_| open_in_file_manager(&path));
    host.append(&show);
}

fn pretty_imported(iso: &str) -> String {
    if iso.len() >= 10 {
        pretty_day(&iso[..10])
    } else {
        iso.to_string()
    }
}

/// Reveal the book's folder in the system file manager.
pub(crate) fn open_in_file_manager(file: &std::path::Path) {
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
pub(crate) fn panel_title(text: &str) -> gtk::Box {
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
