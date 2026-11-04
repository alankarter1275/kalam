use crate::db::{BulkMetadataEdit, Catalog, ReadingKinds, SortKey};
use crate::models::Book;
use crate::service::{AllBooksSnapshot, LibraryService};
use crate::widgets::book_row::{
    build_book_grid, build_book_grid_selectable, invalidate_cover_cache,
};
use crate::widgets::in_app_dialog;
use crate::widgets::pill_button::PillButtonKind;
use crate::widgets::reading_choice::{
    choose_kinds_table, restored_reading_dialog, ChoiceRow, ChooserButtons,
};
use gtk::prelude::*;
use relm4::prelude::*;
use std::collections::HashSet;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Debug)]
pub enum AllBooksOut {
    OpenBook {
        book_id: i64,
    },
    OpenBookDialog {
        book_id: i64,
    },
    OpenReader {
        book_id: i64,
        chapter: Option<usize>,
    },
    ComicSeries {
        series_name: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchMode {
    #[default]
    Metadata,
    Content,
}

#[derive(Debug)]
pub enum AllBooksMsg {
    SearchChanged(String),
    /// Set the search from outside (the author page's "Show books in
    /// library"): the entry text and the query move together, so what
    /// the reader sees is a search they can keep editing.
    SetQuery(String),
    /// Put keyboard focus in the library search (the shell's Ctrl+F).
    FocusSearch,
    SearchModeChanged(SearchMode),
    SortChanged(SortKey),
    PickFiles,
    FilesChosen(Vec<PathBuf>),
    /// One file finished importing: 1-based index, total, and its title.
    ImportStep {
        done: usize,
        total: usize,
        title: String,
    },
    /// The whole import finished.
    ImportFinished(ImportTally),
    /// A background book-list query finished.
    ///
    /// Carries the generation it was started with, so a slow reply from an
    /// older query cannot land after a newer one and put the wrong list on
    /// screen — which typing quickly into the search box makes easy to hit.
    BooksLoaded {
        gen: u64,
        snap: AllBooksSnapshot,
    },
    ContentSearchLoaded {
        gen: u64,
        snap: crate::service::ContentSearchSnapshot,
    },
    IndexStatusLoaded(crate::service::ContentIndexStatusSnapshot),
    TriggerIndexLibrary,
    TriggerReindexLibrary,
    IndexFinished(Result<usize, String>),
    ToggleExpandBook(i64),
    ToggleSelectionMode,
    ToggleSelectBook(i64),
    SelectAll,
    DeselectAll,
    OpenBulkEditDialog(gtk::Widget),
    ApplyBulkEdit(BulkMetadataEdit),
    /// Ask before deleting the selected books; carries the button as anchor.
    OpenDeleteConfirm(gtk::Widget),
    /// Run the bulk delete on a worker so deleting many books cannot freeze
    /// the grid. Carries what the reader chose to keep for each book, decided
    /// per row in the table rather than once for the whole selection.
    DeleteSelected(Vec<(i64, ReadingKinds)>),
    /// The bulk delete finished; the counts feed the toast and a reload.
    DeleteDone {
        done: usize,
        failed: usize,
    },
    /// An import answer was applied; the page should re-read its list, because
    /// clearing a reading position changes the percentage on the card.
    Settled,
}

/// The buttons on the import question.
///
/// "Ask me later" is not a cancel. Closing the dialog leaves the book flagged
/// as owing an answer, and the question comes back when the book is next
/// opened — which is what the flag is for. Labelling it "Cancel" would have
/// promised the reader that nothing was left outstanding.
const IMPORT_BUTTONS: ChooserButtons = ChooserButtons {
    dismiss: "Ask me later",
    confirm: "Keep these",
    confirm_kind: PillButtonKind::Primary,
};

/// A book an import brought back with its reading data.
#[derive(Debug, Clone)]
pub struct RestoredBook {
    pub id: i64,
    pub title: String,
    /// The grey second line in the chooser table.
    ///
    /// Read on the import worker, which already has the book open, rather than
    /// looked up when the dialog is built. Building a dialog is UI-thread work,
    /// and a page that reaches for the database handle there is exactly what
    /// `LibraryService` exists to prevent — the query would run while the grid
    /// is waiting to be repainted.
    pub authors: String,
}

/// Ask what to do with the reading data an import just brought back.
///
/// Shared by every page that can start an import, because the question and its
/// consequences are the same whichever button began it. Three pages import
/// books, and three copies of this would drift.
///
/// Asked after the import rather than before it. By now the data is already in
/// the book, so this is a subtraction and not a permission — and it means the
/// question can name real titles rather than the file names the reader picked,
/// because reading the titles out of the files is what the import was for.
/// Asking first would also have meant fingerprinting every chosen file twice,
/// since the fingerprint that finds the rescue copy is computed inside.
///
/// One book gets the single dialog. Several get the table, because a stack of
/// dialogs to click through is worse than one screen of ticks.
///
/// `on_settled` runs once the answer has been applied, so the caller can
/// re-read its list.
pub fn ask_about_restored(
    anchor: &impl IsA<gtk::Widget>,
    service: &LibraryService,
    restored: &[RestoredBook],
    on_settled: impl Fn() + 'static,
) {
    if restored.is_empty() {
        return;
    }
    // Boxed as a trait object because both branches hand it to a worker that
    // outlives this call, and a borrow of the caller's closure would not.
    // `on_done` is not `Send`, so `Rc` is enough.
    let settled: Rc<dyn Fn()> = Rc::new(on_settled);
    let service = service.clone();

    if restored.len() == 1 {
        let book = restored[0].clone();
        let service_for_dialog = service.clone();
        let settled_for_dialog = settled.clone();
        restored_reading_dialog(anchor, &book.title, IMPORT_BUTTONS, move |keep| {
            settle_restored(
                service_for_dialog.clone(),
                vec![(book.id, keep)],
                settled_for_dialog.clone(),
            );
        });
        return;
    }

    // The author goes on the row because two books can share a title, and the
    // reader is about to decide what to throw away.
    let rows: Vec<ChoiceRow> = restored
        .iter()
        .map(|b| ChoiceRow {
            id: b.id,
            title: b.title.clone(),
            detail: b.authors.trim().to_string(),
            initial: ReadingKinds::ALL,
        })
        .collect();
    let count = rows.len();
    let service_for_table = service.clone();
    let settled_for_table = settled.clone();
    choose_kinds_table(
        anchor,
        "Your notes came back",
        &format!(
            "{count} of the books you imported came back with the notes and marks \
             you left in them. Choose what to keep for each. Anything unticked is \
             removed for good."
        ),
        rows,
        IMPORT_BUTTONS,
        move |choices| {
            settle_restored(
                service_for_table.clone(),
                choices,
                settled_for_table.clone(),
            );
        },
    );
}

/// Apply the answers on a worker, then say what happened.
///
/// Each book is settled separately rather than in one transaction, so one
/// failure cannot roll back nine good answers — and the count reported is the
/// count that actually landed.
fn settle_restored(
    service: LibraryService,
    choices: Vec<(i64, ReadingKinds)>,
    settled: Rc<dyn Fn()>,
) {
    let total = choices.len();
    // The handle is taken here, at the edge of the worker, and never used to
    // read on the UI thread: `LibraryService` is what a page holds, and the
    // catalog it wraps is only ever handed to the task below.
    let catalog = service.catalog().clone();
    crate::tasks::spawn(
        format!(
            "Applying your choice for {total} book{}",
            if total == 1 { "" } else { "s" }
        ),
        move |reporter| {
            let mut failed = 0usize;
            for (i, (id, keep)) in choices.iter().enumerate() {
                if reporter.cancelled() {
                    break;
                }
                if catalog.settle_restore(*id, *keep).is_err() {
                    failed += 1;
                    reporter.fail();
                }
                reporter.step(i + 1, total, format!("{}/{total}", i + 1));
            }
            (total - failed, failed)
        },
        |_| {},
        move |(done, failed)| {
            if failed == 0 {
                crate::notify::success(
                    "Your choices are saved",
                    &format!("{done} book{} updated", if done == 1 { "" } else { "s" }),
                );
            } else {
                crate::notify::error(
                    "Some choices were not saved",
                    &format!("{failed} of {} could not be updated", done + failed),
                );
            }
            // `Rc<F>` is not itself callable; the call goes through the pointee.
            (*settled)();
        },
    );
}

#[derive(Debug, Default)]
pub struct ImportTally {
    pub imported: usize,
    pub dupes: usize,
    pub errors: usize,
    pub restored: usize,
    /// Books that came back with reading data, as id and title.
    ///
    /// Kept apart from `restored` because they are reported apart: metadata
    /// edits are simply re-applied, while reading data leaves the book owing
    /// the reader a question. Carried as a list rather than a count because the
    /// question is asked per book, and a count that could disagree with the
    /// list would be two answers to the same thing.
    pub restored_books: Vec<RestoredBook>,
    pub last_title: String,
}

/// The one-line summary shown after an import finishes.
pub fn import_summary(tally: &ImportTally) -> String {
    // Two clauses, because the two counts are different facts and can both be
    // non-zero for the same book. One clause saying "metadata edits" for a book
    // that only came back with its highlights was simply untrue — the defect
    // that splitting `RestoreOutcome` exists to fix.
    let restored_note = if tally.restored > 0 {
        format!(" {} kept your earlier metadata edits.", tally.restored)
    } else {
        String::new()
    };
    let reading_note = if !tally.restored_books.is_empty() {
        format!(
            " {} came back with your notes and marks.",
            tally.restored_books.len()
        )
    } else {
        String::new()
    };
    format!(
        "Import done — {} added, {} already in library, {} failed.{restored_note}{reading_note}{}",
        tally.imported,
        tally.dupes,
        tally.errors,
        if tally.last_title.is_empty() {
            String::new()
        } else {
            format!(" Last: {}", tally.last_title)
        }
    )
}

/// Import `paths` on a worker thread, reporting per file.
pub fn spawn_import(
    catalog: Arc<Catalog>,
    paths: Vec<PathBuf>,
    on_step: impl Fn(usize, usize, String) + 'static,
    on_done: impl FnOnce(ImportTally) + 'static,
) {
    let total = paths.len();
    crate::tasks::spawn(
        "Importing EPUBs",
        move |reporter| {
            let mut tally = ImportTally::default();
            let mut failures: Vec<(String, String)> = Vec::new();
            // Every title this run brings in, so the finished row can expand
            // to name them all rather than just a count.
            let mut imported_titles: Vec<String> = Vec::new();
            for (i, path) in paths.iter().enumerate() {
                if reporter.cancelled() {
                    break;
                }
                match crate::epub::import_epub(&catalog, path) {
                    Ok(r) if r.duplicate => {
                        tally.dupes += 1;
                        tally.last_title = r.title.clone();
                    }
                    Ok(r) => {
                        tally.imported += 1;
                        imported_titles.push(r.title.clone());
                        if r.restored {
                            tally.restored += 1;
                        }
                        if r.reading_restored {
                            // The author is read here, on the worker, because
                            // the chooser table shows it and building that
                            // table is UI-thread work.
                            let authors = catalog
                                .get_book(r.book_id)
                                .ok()
                                .flatten()
                                .map(|b| b.authors)
                                .unwrap_or_default();
                            tally.restored_books.push(RestoredBook {
                                id: r.book_id,
                                title: r.title.clone(),
                                authors,
                            });
                        }
                        // A freshly imported scanned PDF gets the
                        // import-time whole-book OCR scan (2.20). Digital
                        // PDFs end that task immediately with "no scanned
                        // pages"; EPUBs and comics never start it.
                        if r.format == crate::models::BookFormat::Pdf {
                            if let Ok(Some(book)) = catalog.get_book(r.book_id) {
                                crate::pdf_ocr::enqueue_import_scan(
                                    catalog.clone(),
                                    r.book_id,
                                    r.title.clone(),
                                    book.file_path.clone(),
                                );
                            }
                        }
                        tally.last_title = r.title.clone();
                    }
                    Err(err) => {
                        tally.errors += 1;
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        failures.push((format!("Could not import {name}"), format!("{err:#}")));
                    }
                }
                reporter.step(i + 1, total, tally.last_title.clone());
            }
            // The toast for each failure disappears; the task manager's badge
            // does not. Say so here, where the count is known.
            if tally.errors > 0 {
                reporter.fail();
            }
            // The finished row answers "what did it actually do". A single
            // file names itself; a batch gives counts, because sixty titles
            // would be noise.
            reporter.summarize(if total == 1 {
                tally.last_title.clone()
            } else {
                format!(
                    "{} imported, {} duplicates, {} failed",
                    tally.imported, tally.dupes, tally.errors
                )
            });
            reporter.items(imported_titles);
            (tally, failures)
        },
        move |update| on_step(update.done, update.total, update.detail),
        move |(tally, failures)| {
            for (title, detail) in &failures {
                crate::notify::error(title, detail);
            }
            on_done(tally);
        },
    );
}

pub struct AllBooksModel {
    service: LibraryService,
    books: Vec<Book>,
    query: String,
    sort: SortKey,
    status: String,
    /// True while a background import is running; disables the button.
    importing: bool,
    selection_mode: bool,
    selected_books: HashSet<i64>,
    /// Monotonic counter for book-list queries. Each request stamps the value
    /// it was started with, and a reply that no longer matches has been
    /// superseded and is dropped.
    reload_gen: u64,
    search_mode: SearchMode,
    content_results: Vec<crate::content_index::BookContentSearchResult>,
    content_total_matches: usize,
    content_total_books: usize,
    content_duration_ms: u128,
    content_searching: bool,
    index_status: crate::content_index::ContentIndexStatus,
    indexing: bool,
    expanded_books: HashSet<i64>,
}

#[relm4::component(pub)]
impl Component for AllBooksModel {
    type Init = Arc<Catalog>;
    type Input = AllBooksMsg;
    type Output = AllBooksOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,
            set_hexpand: true,

            gtk::Label {
                set_label: "All books",
                add_css_class: "kalam-page-title",
                set_halign: gtk::Align::Start,
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                #[name = "search"]
                gtk::SearchEntry {
                    set_hexpand: true,
                    set_placeholder_text: Some("Search title, author, tag:fantasy, status:unread, rating:>3…"),
                    connect_search_changed[sender] => move |entry| {
                        sender.input(AllBooksMsg::SearchChanged(entry.text().to_string()));
                    },
                },

                #[name = "mode_box"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    add_css_class: "linked",

                    #[name = "mode_meta_btn"]
                    gtk::ToggleButton {
                        set_label: "Titles & Authors",
                        set_active: true,
                        set_tooltip_text: Some("Filter library by title, author, tag, reading status, or rating"),
                        connect_toggled[sender] => move |btn| {
                            if btn.is_active() {
                                sender.input(AllBooksMsg::SearchModeChanged(SearchMode::Metadata));
                            }
                        },
                    },

                    #[name = "mode_content_btn"]
                    gtk::ToggleButton {
                        set_label: "Book Content",
                        set_tooltip_text: Some("Deep search inside the full text of all books in your library"),
                        connect_toggled[sender] => move |btn| {
                            if btn.is_active() {
                                sender.input(AllBooksMsg::SearchModeChanged(SearchMode::Content));
                            }
                        },
                    },
                },

                #[name = "sort_box"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 4,
                },

                #[name = "select_btn"]
                gtk::Button {
                    #[watch]
                    set_label: if model.selection_mode {
                        "Exit Select"
                    } else {
                        "Select Mode"
                    },
                    add_css_class: "kalam-secondary-btn",
                    connect_clicked => AllBooksMsg::ToggleSelectionMode,
                },

                gtk::Button {
                    #[watch]
                    set_label: if model.importing {
                        "Importing…"
                    } else {
                        "+ Import EPUB"
                    },
                    add_css_class: "kalam-primary-btn",
                    #[watch]
                    set_sensitive: !model.importing,
                    connect_clicked => AllBooksMsg::PickFiles,
                },
            },

            #[name = "selection_bar"]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                #[watch]
                set_visible: model.selection_mode,

                gtk::Label {
                    #[watch]
                    set_label: &format!(
                        "{} book{} selected",
                        model.selected_books.len(),
                        if model.selected_books.len() == 1 { "" } else { "s" }
                    ),
                    add_css_class: "kalam-page-sub",
                },

                gtk::Button {
                    set_label: "Select All",
                    add_css_class: "kalam-secondary-btn",
                    connect_clicked => AllBooksMsg::SelectAll,
                },

                gtk::Button {
                    set_label: "Deselect All",
                    add_css_class: "kalam-secondary-btn",
                    connect_clicked => AllBooksMsg::DeselectAll,
                },

                gtk::Button {
                    set_label: "Bulk Edit…",
                    add_css_class: "kalam-primary-btn",
                    #[watch]
                    set_sensitive: !model.selected_books.is_empty(),
                    connect_clicked[sender] => move |btn| {
                        sender.input(AllBooksMsg::OpenBulkEditDialog(btn.clone().upcast()));
                    },
                },

                // Deleting is the one selection action that cannot be undone,
                // so it gets the danger style and a confirmation the others do
                // not. It was simply missing: selection mode shipped with edit
                // but no way to remove several books at once.
                gtk::Button {
                    set_label: "Delete",
                    add_css_class: "kalam-btn-danger",
                    #[watch]
                    set_sensitive: !model.selected_books.is_empty(),
                    connect_clicked[sender] => move |btn| {
                        sender.input(AllBooksMsg::OpenDeleteConfirm(btn.clone().upcast()));
                    },
                },
            },

            #[name = "status_label"]
            gtk::Label {
                #[watch]
                set_label: &model.status,
                add_css_class: "kalam-muted",
                set_halign: gtk::Align::Start,
            },

            #[name = "list"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                set_halign: gtk::Align::Start,
                set_hexpand: false,
                set_vexpand: false,
            },
        }
    }

    fn init(
        catalog: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let sort = SortKey::Added;
        let service = LibraryService::new(catalog);
        // Deliberately still synchronous, and the one blocking read left on
        // this page. At this point the page is not on screen, so there is no
        // visible UI to freeze and no existing list to preserve — making it
        // asynchronous would render an empty grid and then fill it, which is
        // a worse first impression than waiting a few milliseconds. Revisit
        // when a large library makes this measurable. Everything *after* the
        // page exists goes through `reload`, which is a worker.
        let snap = service.all_books(sort, "");
        let status = match snap.errors.first() {
            Some(err) => format!("Database error: {err}"),
            None => status_line(snap.books.len(), ""),
        };

        let mut model = AllBooksModel {
            service,
            books: snap.books,
            query: String::new(),
            sort,
            status,
            importing: false,
            selection_mode: false,
            selected_books: HashSet::new(),
            reload_gen: 0,
            search_mode: SearchMode::Metadata,
            content_results: Vec::new(),
            content_total_matches: 0,
            content_total_books: 0,
            content_duration_ms: 0,
            content_searching: false,
            index_status: crate::content_index::ContentIndexStatus::default(),
            indexing: false,
            expanded_books: HashSet::new(),
        };
        let widgets = view_output!();
        widgets
            .mode_content_btn
            .set_group(Some(&widgets.mode_meta_btn));
        model.check_index_status(&sender);

        for key in SortKey::ALL {
            let btn = gtk::ToggleButton::with_label(key.label());
            btn.add_css_class("kalam-secondary-btn");
            if *key == SortKey::Added {
                btn.set_active(true);
            }
            let k = *key;
            let s = sender.clone();
            btn.connect_toggled(move |b| {
                if b.is_active() {
                    s.input(AllBooksMsg::SortChanged(k));
                }
            });
            widgets.sort_box.append(&btn);
        }
        group_toggles(&widgets.sort_box);

        rebuild_list(
            &widgets.list,
            &model.books,
            &model.query,
            model.selection_mode,
            &model.selected_books,
            &sender,
        );

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
            AllBooksMsg::ImportStep { done, total, title } => {
                self.status = if title.is_empty() {
                    format!("Importing {done} of {total}…")
                } else {
                    format!("Importing {done} of {total} — {title}")
                };
                widgets.status_label.set_label(&self.status);
                return;
            }
            AllBooksMsg::ImportFinished(tally) => {
                self.importing = false;
                self.reload(&sender);

                if tally.imported > 0 {
                    crate::notify::success(
                        &format!(
                            "{} book{} imported",
                            tally.imported,
                            if tally.imported == 1 { "" } else { "s" }
                        ),
                        &tally.last_title,
                    );
                }

                self.status = import_summary(&tally);

                if !tally.restored_books.is_empty() {
                    let s = sender.input_sender().clone();
                    ask_about_restored(
                        root,
                        &self.service,
                        &tally.restored_books,
                        move || {
                            s.send(AllBooksMsg::Settled).ok();
                        },
                    );
                }
            }
            AllBooksMsg::Settled => {
                self.reload(&sender);
            }
            AllBooksMsg::BooksLoaded { gen, snap } => {
                // Drop a reply an older query produced. Typing quickly starts
                // several, and without this a slow early result can land after
                // a fast later one and put the wrong list on screen.
                if gen != self.reload_gen {
                    return;
                }
                match snap.errors.first() {
                    // Keep the list already on screen. Clearing it, as the
                    // synchronous version did, turns a failed read into
                    // something that looks like data loss.
                    Some(err) => self.status = format!("Database error: {err}"),
                    None => {
                        let n = snap.books.len();
                        self.books = snap.books;
                        if !self.status.starts_with("Import done") {
                            self.status = status_line(n, &self.query);
                        }
                    }
                }
            }
            AllBooksMsg::FocusSearch => {
                widgets.search.grab_focus();
                return;
            }
            AllBooksMsg::SetQuery(q) => {
                // Setting the entry text fires search-changed, which
                // arrives as SearchChanged and does the actual reload.
                widgets.search.set_text(&q);
                if self.query != q {
                    sender.input(AllBooksMsg::SearchChanged(q));
                }
            }
            AllBooksMsg::SearchChanged(q) => {
                self.query = q;
                self.status.clear();
                if self.search_mode == SearchMode::Content {
                    if self.query.trim().len() >= 2 {
                        self.search_content(&sender);
                    } else {
                        self.content_results.clear();
                        self.content_searching = false;
                    }
                } else {
                    self.reload(&sender);
                }
            }
            AllBooksMsg::SearchModeChanged(mode) => {
                if self.search_mode != mode {
                    self.search_mode = mode;
                    self.status.clear();
                    if mode == SearchMode::Content {
                        self.check_index_status(&sender);
                        if self.query.trim().len() >= 2 {
                            self.search_content(&sender);
                        } else {
                            self.content_results.clear();
                            self.content_searching = false;
                        }
                    } else {
                        self.reload(&sender);
                    }
                }
            }
            AllBooksMsg::ContentSearchLoaded { gen, snap } => {
                if gen != self.reload_gen {
                    return;
                }
                self.content_searching = false;
                match snap.errors.first() {
                    Some(err) => self.status = format!("Search error: {err}"),
                    None => {
                        self.content_results = snap.results;
                        self.content_total_matches = snap.total_matches;
                        self.content_total_books = snap.total_books;
                        self.content_duration_ms = snap.duration_ms;
                        if self.content_results.is_empty() {
                            self.status = format!("No matches found for “{}”", self.query.trim());
                        } else {
                            self.status = format!(
                                "Found {} match{} across {} book{} ({} ms)",
                                self.content_total_matches,
                                if self.content_total_matches == 1 {
                                    ""
                                } else {
                                    "es"
                                },
                                self.content_total_books,
                                if self.content_total_books == 1 {
                                    ""
                                } else {
                                    "s"
                                },
                                self.content_duration_ms,
                            );
                        }
                    }
                }
            }
            AllBooksMsg::IndexStatusLoaded(snap) => {
                if let Some(err) = snap.errors.first() {
                    self.status = format!("Index status error: {err}");
                }
                self.index_status = snap.status;
            }
            AllBooksMsg::TriggerIndexLibrary => {
                self.indexing = true;
                let catalog = self.service.catalog().clone();
                let done = sender.input_sender().clone();
                crate::tasks::spawn(
                    "Indexing library",
                    move |_reporter| LibraryService::new(catalog).index_unindexed_books(),
                    |_update| {},
                    move |res| {
                        let _ = done.send(AllBooksMsg::IndexFinished(res));
                    },
                );
            }
            AllBooksMsg::TriggerReindexLibrary => {
                self.indexing = true;
                let catalog = self.service.catalog().clone();
                let done = sender.input_sender().clone();
                crate::tasks::spawn(
                    "Reindexing library",
                    move |_reporter| LibraryService::new(catalog).reindex_all_books(),
                    |_update| {},
                    move |res| {
                        let _ = done.send(AllBooksMsg::IndexFinished(res));
                    },
                );
            }
            AllBooksMsg::IndexFinished(res) => {
                self.indexing = false;
                self.check_index_status(&sender);
                match res {
                    Ok(count) => {
                        crate::notify::success(
                            "Content search index updated",
                            &format!("{count} books indexed and ready for deep search."),
                        );
                        if self.query.trim().len() >= 2 {
                            self.search_content(&sender);
                        }
                    }
                    Err(err) => {
                        crate::notify::error("Indexing error", &err);
                    }
                }
            }
            AllBooksMsg::ToggleExpandBook(b_id) => {
                if self.expanded_books.contains(&b_id) {
                    self.expanded_books.remove(&b_id);
                } else {
                    self.expanded_books.insert(b_id);
                }
            }
            AllBooksMsg::SortChanged(sort) => {
                self.sort = sort;
                self.status.clear();
                self.reload(&sender);
            }
            AllBooksMsg::ToggleSelectionMode => {
                self.selection_mode = !self.selection_mode;
                if !self.selection_mode {
                    self.selected_books.clear();
                }
            }
            AllBooksMsg::ToggleSelectBook(id) => {
                if self.selected_books.contains(&id) {
                    self.selected_books.remove(&id);
                } else {
                    self.selected_books.insert(id);
                }
            }
            AllBooksMsg::SelectAll => {
                for b in &self.books {
                    self.selected_books.insert(b.id);
                }
            }
            AllBooksMsg::DeselectAll => {
                self.selected_books.clear();
            }
            AllBooksMsg::OpenBulkEditDialog(anchor) => {
                let content = gtk::Box::new(gtk::Orientation::Vertical, 12);

                let info = gtk::Label::new(Some(&format!(
                    "Editing metadata for {} selected book{}.",
                    self.selected_books.len(),
                    if self.selected_books.len() == 1 {
                        ""
                    } else {
                        "s"
                    }
                )));
                info.set_halign(gtk::Align::Start);
                info.add_css_class("kalam-muted");
                content.append(&info);

                // Add tags
                let add_tags_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let add_tags_check = gtk::CheckButton::with_label("Add tags:");
                let add_tags_entry = gtk::Entry::new();
                add_tags_entry.set_placeholder_text(Some("e.g. favourite, sci-fi"));
                add_tags_entry.set_hexpand(true);
                add_tags_box.append(&add_tags_check);
                add_tags_box.append(&add_tags_entry);
                content.append(&add_tags_box);

                let chk = add_tags_check.clone();
                add_tags_entry.connect_changed(move |e| {
                    chk.set_active(!e.text().trim().is_empty());
                });

                // Set/Replace tags
                let set_tags_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let set_tags_check = gtk::CheckButton::with_label("Set/Replace tags:");
                let set_tags_entry = gtk::Entry::new();
                set_tags_entry.set_placeholder_text(Some("e.g. classic, fiction"));
                set_tags_entry.set_hexpand(true);
                set_tags_box.append(&set_tags_check);
                set_tags_box.append(&set_tags_entry);
                content.append(&set_tags_box);

                let chk = set_tags_check.clone();
                set_tags_entry.connect_changed(move |e| {
                    chk.set_active(!e.text().trim().is_empty());
                });

                // Author
                let author_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let author_check = gtk::CheckButton::with_label("Update Author:");
                let author_entry = gtk::Entry::new();
                author_entry.set_placeholder_text(Some("e.g. Frank Herbert"));
                author_entry.set_hexpand(true);
                author_box.append(&author_check);
                author_box.append(&author_entry);
                content.append(&author_box);

                let chk = author_check.clone();
                author_entry.connect_changed(move |e| {
                    chk.set_active(!e.text().trim().is_empty());
                });

                // Publisher
                let publisher_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let publisher_check = gtk::CheckButton::with_label("Update Publisher:");
                let publisher_entry = gtk::Entry::new();
                publisher_entry.set_placeholder_text(Some("e.g. Chilton Books"));
                publisher_entry.set_hexpand(true);
                publisher_box.append(&publisher_check);
                publisher_box.append(&publisher_entry);
                content.append(&publisher_box);

                let chk = publisher_check.clone();
                publisher_entry.connect_changed(move |e| {
                    chk.set_active(!e.text().trim().is_empty());
                });

                // Reading status
                let status_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let status_check = gtk::CheckButton::with_label("Update Reading Status:");
                let status_combo = gtk::DropDown::from_strings(&["Unread", "Reading", "Finished"]);
                status_box.append(&status_check);
                status_box.append(&status_combo);
                content.append(&status_box);

                let chk = status_check.clone();
                status_combo.connect_selected_notify(move |_| {
                    chk.set_active(true);
                });

                // Buttons
                let btn_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                btn_box.set_halign(gtk::Align::End);
                let cancel = gtk::Button::with_label("Cancel");
                cancel.add_css_class("kalam-secondary-btn");
                let apply = gtk::Button::with_label("Apply Changes");
                apply.add_css_class("kalam-primary-btn");
                btn_box.append(&cancel);
                btn_box.append(&apply);
                content.append(&btn_box);

                let btn = apply.clone();
                add_tags_entry.connect_activate(move |_| {
                    btn.emit_clicked();
                });
                let btn = apply.clone();
                set_tags_entry.connect_activate(move |_| {
                    btn.emit_clicked();
                });
                let btn = apply.clone();
                author_entry.connect_activate(move |_| {
                    btn.emit_clicked();
                });
                let btn = apply.clone();
                publisher_entry.connect_activate(move |_| {
                    btn.emit_clicked();
                });

                let dialog = in_app_dialog::present(
                    &anchor,
                    "Bulk Edit Metadata",
                    in_app_dialog::DialogExit::UnsavedInput,
                    &content,
                );

                if let Some(dlg) = dialog {
                    let d = dlg.clone();
                    cancel.connect_clicked(move |_| d.close());

                    let d = dlg.clone();
                    let s = sender.clone();
                    apply.connect_clicked(move |_| {
                        let add_tags = if add_tags_check.is_active() {
                            let text = add_tags_entry.text();
                            let tags: Vec<String> = text
                                .split(',')
                                .map(|t| t.trim().to_string())
                                .filter(|t| !t.is_empty())
                                .collect();
                            if !tags.is_empty() {
                                Some(tags)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        let set_tags = if set_tags_check.is_active() {
                            let text = set_tags_entry.text();
                            let tags: Vec<String> = text
                                .split(',')
                                .map(|t| t.trim().to_string())
                                .filter(|t| !t.is_empty())
                                .collect();
                            Some(tags)
                        } else {
                            None
                        };

                        let author = if author_check.is_active() {
                            let text = author_entry.text().trim().to_string();
                            if !text.is_empty() {
                                Some(text)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        let publisher = if publisher_check.is_active() {
                            let text = publisher_entry.text().trim().to_string();
                            if !text.is_empty() {
                                Some(text)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        let reading_status = if status_check.is_active() {
                            let idx = status_combo.selected();
                            match idx {
                                0 => Some("Unread".to_string()),
                                1 => Some("Reading".to_string()),
                                2 => Some("Finished".to_string()),
                                _ => None,
                            }
                        } else {
                            None
                        };

                        let edit = BulkMetadataEdit {
                            add_tags,
                            set_tags,
                            author,
                            publisher,
                            reading_status,
                        };

                        s.input(AllBooksMsg::ApplyBulkEdit(edit));
                        d.close();
                    });
                }
            }
            AllBooksMsg::ApplyBulkEdit(edit) => {
                let ids: Vec<i64> = self.selected_books.iter().copied().collect();
                if let Err(err) = self.service.catalog().bulk_update_metadata(&ids, &edit) {
                    crate::notify::error("Bulk edit failed", &err.to_string());
                } else {
                    crate::notify::success(
                        "Bulk edit complete",
                        &format!("Updated metadata for {} books", ids.len()),
                    );
                    self.selected_books.clear();
                    self.selection_mode = false;
                    self.reload(&sender);
                }
            }
            AllBooksMsg::OpenDeleteConfirm(anchor) => {
                let count = self.selected_books.len();
                if count == 0 {
                    return;
                }
                // A comic selection deletes its whole series (`delete_book`
                // is series-scoped for comics), so the number the dialog
                // shows must count the chapters, not the cards.
                let ids: Vec<i64> = self.selected_books.iter().copied().collect();
                let catalog = self.service.catalog();
                let real_count = catalog.delete_scope_count(&ids).max(count);
                let rows = choice_rows(catalog, &ids);
                let s = sender.input_sender().clone();
                // Not the old two-button confirm: deleting a book can now keep
                // some of it and lose some of it, and one answer for the whole
                // selection would force the same choice on every book in it.
                let dialog = choose_kinds_table(
                    &anchor,
                    "Delete books",
                    &format!(
                        "Delete {real_count} book{}? The files and their library entries go. \
                         Anything still ticked is kept aside, and comes back if you import the \
                         same file again. Untick what should go for good.",
                        if real_count == 1 { "" } else { "s" }
                    ),
                    rows,
                    // "Cancel" is honest here and would not be on an import:
                    // closing this dialog leaves every book untouched.
                    ChooserButtons {
                        dismiss: "Cancel",
                        confirm: "Delete",
                        confirm_kind: PillButtonKind::Danger,
                    },
                    move |choices| {
                        s.send(AllBooksMsg::DeleteSelected(choices)).ok();
                    },
                );
                if dialog.is_none() {
                    crate::notify::error(
                        "Could not show the confirmation",
                        "Please try again once the page has finished loading.",
                    );
                }
            }
            AllBooksMsg::DeleteSelected(choices) => {
                if choices.is_empty() {
                    return;
                }
                let ids: Vec<i64> = choices.iter().map(|(id, _)| *id).collect();
                // Owned Arc so the worker can outlive this borrow.
                let catalog = self.service.catalog().clone();
                let count = ids.len();
                // What the toast reports: a comic selection removes every
                // chapter of its series, not just the card.
                let real_count = catalog.delete_scope_count(&ids).max(count);
                let s = sender.input_sender().clone();
                crate::tasks::spawn(
                    format!(
                        "Deleting {} book{}",
                        count,
                        if count == 1 { "" } else { "s" }
                    ),
                    move |reporter| {
                        let mut failed = 0usize;
                        for (i, id) in ids.iter().enumerate() {
                            if reporter.cancelled() {
                                break;
                            }
                            // Same courtesy as the single delete: drop the
                            // cached cover so re-importing the path cannot
                            // show the old one.
                            if let Ok(Some(book)) = catalog.get_book(*id) {
                                if let Some(path) = &book.cover_path {
                                    invalidate_cover_cache(path);
                                }
                            }
                            let keep = choices
                                .iter()
                                .find(|(cid, _)| cid == id)
                                .map(|(_, k)| *k)
                                .unwrap_or(ReadingKinds::ALL);
                            if catalog.delete_book_keeping(*id, keep).is_err() {
                                failed += 1;
                                reporter.fail();
                            }
                            reporter.step(i + 1, count, format!("{}/{count}", i + 1));
                        }
                        (real_count, failed)
                    },
                    |_| {},
                    move |(done, failed)| {
                        s.send(AllBooksMsg::DeleteDone { done, failed }).ok();
                    },
                );
            }
            AllBooksMsg::DeleteDone { done, failed } => {
                if failed == 0 {
                    crate::notify::success(
                        "Books removed",
                        &format!("{done} book{} deleted", if done == 1 { "" } else { "s" }),
                    );
                } else {
                    crate::notify::error(
                        "Delete incomplete",
                        &format!("{failed} of {done} could not be deleted"),
                    );
                }
                self.selected_books.clear();
                self.selection_mode = false;
                self.reload(&sender);
            }
            AllBooksMsg::PickFiles => {
                let dialog = gtk::FileDialog::builder()
                    .title("Import EPUB books")
                    .modal(true)
                    .build();

                let filter = gtk::FileFilter::new();
                filter.set_name(Some("Books, Comics, & PDFs (*.epub, *.cbz, *.cbr, *.pdf)"));
                filter.add_suffix("epub");
                filter.add_suffix("cbz");
                filter.add_suffix("cbr");
                filter.add_suffix("pdf");
                filter.add_suffix("pdf");
                filter.add_mime_type("application/epub+zip");
                filter.add_mime_type("application/x-cbz");
                filter.add_mime_type("application/x-cbr");
                filter.add_mime_type("application/pdf");
                let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
                filters.append(&filter);
                dialog.set_filters(Some(&filters));

                let s = sender.clone();
                let window = root.root().and_then(|r| r.downcast::<gtk::Window>().ok());
                let window = window.or_else(|| {
                    relm4::main_application()
                        .active_window()
                        .and_then(|w| w.downcast::<gtk::Window>().ok())
                });

                dialog.open_multiple(
                    window.as_ref(),
                    gtk::gio::Cancellable::NONE,
                    move |result| {
                        if let Ok(files) = result {
                            let mut paths = Vec::new();
                            let n = files.n_items();
                            for i in 0..n {
                                if let Some(obj) = files.item(i) {
                                    if let Ok(file) = obj.downcast::<gtk::gio::File>() {
                                        if let Some(p) = file.path() {
                                            paths.push(p);
                                        }
                                    }
                                }
                            }
                            if !paths.is_empty() {
                                s.input(AllBooksMsg::FilesChosen(paths));
                            }
                        }
                    },
                );
            }
            AllBooksMsg::FilesChosen(paths) => {
                let total = paths.len();
                self.importing = true;
                self.status = format!("Importing 1 of {total}…");

                let catalog = self.service.catalog().clone();
                let step_tx = sender.input_sender().clone();
                let done_tx = sender.input_sender().clone();
                spawn_import(
                    catalog,
                    paths,
                    move |done, total, title| {
                        let _ = step_tx.send(AllBooksMsg::ImportStep { done, total, title });
                    },
                    move |tally| {
                        let _ = done_tx.send(AllBooksMsg::ImportFinished(tally));
                    },
                );
            }
        }

        if self.search_mode == SearchMode::Content {
            widgets.sort_box.set_visible(false);
            widgets.select_btn.set_visible(false);
            widgets.selection_bar.set_visible(false);
            widgets.search.set_placeholder_text(Some(
                "Search inside book contents (phrases, quotes, names)…",
            ));
            widgets.mode_meta_btn.set_active(false);
            widgets.mode_content_btn.set_active(true);
            rebuild_content_search_list(&widgets.list, self, &sender);
        } else {
            widgets.sort_box.set_visible(true);
            widgets.select_btn.set_visible(true);
            widgets.selection_bar.set_visible(self.selection_mode);
            widgets.search.set_placeholder_text(Some(
                "Search title, author, tag:fantasy, status:unread, rating:>3…",
            ));
            widgets.mode_meta_btn.set_active(true);
            widgets.mode_content_btn.set_active(false);
            rebuild_list(
                &widgets.list,
                &self.books,
                &self.query,
                self.selection_mode,
                &self.selected_books,
                &sender,
            );
        }
        self.update_view(widgets, sender);
    }
}

impl AllBooksModel {
    /// Search book full-text contents on a worker thread.
    fn search_content(&mut self, sender: &ComponentSender<Self>) {
        self.reload_gen += 1;
        let gen = self.reload_gen;
        self.content_searching = true;
        let catalog = self.service.catalog().clone();
        let query = self.query.clone();
        let done = sender.input_sender().clone();
        crate::tasks::spawn(
            "Searching book contents",
            move |_reporter| LibraryService::new(catalog).search_content(&query),
            |_update| {},
            move |snap| {
                let _ = done.send(AllBooksMsg::ContentSearchLoaded { gen, snap });
            },
        );
    }

    /// Check content search index status on a worker thread.
    fn check_index_status(&mut self, sender: &ComponentSender<Self>) {
        let catalog = self.service.catalog().clone();
        let done = sender.input_sender().clone();
        crate::tasks::spawn(
            "Checking search index status",
            move |_reporter| LibraryService::new(catalog).index_status(),
            |_update| {},
            move |snap| {
                let _ = done.send(AllBooksMsg::IndexStatusLoaded(snap));
            },
        );
    }

    /// Ask for the book list on a worker thread.
    ///
    /// Roadmap 1.2b pilot. `all_books` is one of the queries whose cost grows
    /// with library size, and this is called on **every keystroke** of the
    /// search box, so it is the worst place on this page to block the UI
    /// thread. The grid keeps whatever it is already showing and swaps the
    /// list in when the reply lands — clearing it for the round trip would
    /// replace something instant with a flicker, which is the one way this
    /// change could make the app feel worse rather than merely safer.
    fn reload(&mut self, sender: &ComponentSender<Self>) {
        self.reload_gen += 1;
        let gen = self.reload_gen;
        let catalog = self.service.catalog().clone();
        let sort = self.sort;
        let query = self.query.clone();
        let done = sender.input_sender().clone();
        crate::tasks::spawn(
            "Loading books",
            move |_reporter| {
                // A fresh service rather than moving this page's own: `new`
                // only clones an `Arc`, the page's handle stays untouched, and
                // `AllBooksSnapshot` is `Send` — asserted by
                // `snapshots_are_send` — so the reply crosses back safely.
                LibraryService::new(catalog).all_books(sort, &query)
            },
            // Nothing to report: this is one query, not a sequence of steps.
            |_update| {},
            move |snap| {
                let _ = done.send(AllBooksMsg::BooksLoaded { gen, snap });
            },
        );
    }
}

fn status_line(n: usize, query: &str) -> String {
    if query.trim().is_empty() {
        if n == 0 {
            "No books yet — import an EPUB to get started.".into()
        } else {
            format!(
                "{n} book{} · click cover for float · Ctrl+click for full page",
                if n == 1 { "" } else { "s" }
            )
        }
    } else {
        format!("{n} result{}", if n == 1 { "" } else { "s" })
    }
}

fn group_toggles(box_: &gtk::Box) {
    let mut group_leader: Option<gtk::ToggleButton> = None;
    let mut child = box_.first_child();
    while let Some(w) = child {
        let next = w.next_sibling();
        if let Ok(btn) = w.downcast::<gtk::ToggleButton>() {
            if let Some(ref leader) = group_leader {
                btn.set_group(Some(leader));
            } else {
                group_leader = Some(btn);
            }
        }
        child = next;
    }
}

fn rebuild_list(
    list: &gtk::Box,
    books: &[Book],
    query: &str,
    selection_mode: bool,
    selected_books: &HashSet<i64>,
    sender: &ComponentSender<AllBooksModel>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if books.is_empty() {
        let query = query.trim();
        let message = if query.is_empty() {
            "Your library is empty.\nClick “+ Import EPUB” to add books.".to_string()
        } else {
            format!("No books match “{query}”.\nTry another search, or clear it to see everything.")
        };
        let empty = gtk::Label::new(Some(&message));
        empty.add_css_class("kalam-placeholder");
        empty.set_wrap(true);
        list.append(&empty);
        return;
    }

    let s = sender.clone();
    let s2 = sender.clone();

    if selection_mode {
        let sel_set = selected_books.clone();
        let grid = build_book_grid_selectable(
            books,
            move |id| sel_set.contains(&id),
            move |id| {
                s.input(AllBooksMsg::ToggleSelectBook(id));
            },
            move |id| {
                s2.input(AllBooksMsg::ToggleSelectBook(id));
            },
        );
        list.append(&grid);
    } else {
        let comics_map: std::collections::HashMap<i64, Option<String>> = books
            .iter()
            .filter(|b| {
                matches!(
                    b.format,
                    crate::models::BookFormat::Cbz | crate::models::BookFormat::Cbr
                )
            })
            .map(|b| (b.id, b.series.clone()))
            .collect();
        let cm_rc = std::rc::Rc::new(comics_map);
        let cm = cm_rc.clone();
        let grid = build_book_grid(
            books,
            move |id| {
                if let Some(Some(ser)) = cm.get(&id) {
                    s.output(AllBooksOut::ComicSeries {
                        series_name: ser.clone(),
                    })
                    .ok();
                    return;
                }
                s.output(AllBooksOut::OpenBook { book_id: id }).ok();
            },
            move |id| {
                s2.output(AllBooksOut::OpenBookDialog { book_id: id }).ok();
            },
        );
        list.append(&grid);
    }
}

/// Build one table row per selected card.
///
/// A comic is a single card on the grid but several chapters underneath, and
/// `delete_book_keeping` is series-scoped for comics — one row, one tick, and
/// the whole series follows it. The row says so, because "one book" on screen
/// is really twelve files being deleted.
fn choice_rows(catalog: &Catalog, ids: &[i64]) -> Vec<ChoiceRow> {
    ids.iter()
        .filter_map(|id| {
            let book = catalog.get_book(*id).ok().flatten()?;
            let covers = catalog.delete_scope_count(&[*id]);
            let authors = book.authors.trim();
            let detail = if covers > 1 {
                // Inside this branch the count is at least two, so the plural
                // is the only form that can ever be printed.
                let chapters = format!("{covers} chapters");
                if authors.is_empty() {
                    chapters
                } else {
                    format!("{authors} · {chapters}")
                }
            } else {
                authors.to_string()
            };
            Some(ChoiceRow {
                id: *id,
                title: book.title.clone(),
                detail,
                // Everything starts ticked. Deleting has always kept the
                // reading data, so the table must not change that by default.
                initial: ReadingKinds::ALL,
            })
        })
        .collect()
}

fn rebuild_content_search_list(
    list: &gtk::Box,
    model: &AllBooksModel,
    sender: &ComponentSender<AllBooksModel>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let query_trimmed = model.query.trim();
    if query_trimmed.len() < 2 {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
        card.add_css_class("kalam-card");
        card.set_margin_top(16);
        card.set_margin_bottom(16);
        card.set_margin_start(16);
        card.set_margin_end(16);

        let header_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let icon = gtk::Image::from_icon_name("system-search-symbolic");
        icon.set_pixel_size(32);
        icon.add_css_class("kalam-muted");
        header_box.append(&icon);

        let text_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        text_box.set_hexpand(true);
        let title_lbl = gtk::Label::new(Some("Deep Content Search"));
        title_lbl.add_css_class("kalam-card-title");
        title_lbl.set_halign(gtk::Align::Start);
        let desc_lbl = gtk::Label::new(Some(
            "Search across the entire text of your library. Find character names, quotes, phrases, and specific passages.",
        ));
        desc_lbl.add_css_class("kalam-card-meta");
        desc_lbl.set_halign(gtk::Align::Start);
        desc_lbl.set_wrap(true);
        text_box.append(&title_lbl);
        text_box.append(&desc_lbl);
        header_box.append(&text_box);
        card.append(&header_box);

        // Status & indexing bar
        let status_box = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        status_box.set_margin_top(8);
        status_box.set_valign(gtk::Align::Center);

        let st = &model.index_status;
        let status_text = if model.indexing {
            "Indexing text books in the background…".to_string()
        } else if st.indexed_books < st.total_books {
            let unindexed = st.total_books.saturating_sub(st.indexed_books);
            format!(
                "{} of {} text books indexed ({} not yet indexed)",
                st.indexed_books, st.total_books, unindexed
            )
        } else {
            format!(
                "Full-Text Index Ready: {} books indexed ({} words searchable)",
                st.indexed_books, st.total_words
            )
        };
        let st_lbl = gtk::Label::new(Some(&status_text));
        st_lbl.add_css_class("kalam-muted");
        st_lbl.set_halign(gtk::Align::Start);
        st_lbl.set_hexpand(true);
        status_box.append(&st_lbl);

        if model.indexing {
            let spinner = gtk::Spinner::new();
            spinner.start();
            status_box.append(&spinner);
        } else if st.indexed_books < st.total_books {
            let idx_btn = gtk::Button::with_label("Index Library Now");
            idx_btn.add_css_class("kalam-primary-btn");
            let s = sender.clone();
            idx_btn.connect_clicked(move |_| {
                s.input(AllBooksMsg::TriggerIndexLibrary);
            });
            status_box.append(&idx_btn);
        } else {
            let reindex_btn = gtk::Button::with_label("Reindex");
            reindex_btn.add_css_class("kalam-secondary-btn");
            let s = sender.clone();
            reindex_btn.connect_clicked(move |_| {
                s.input(AllBooksMsg::TriggerReindexLibrary);
            });
            status_box.append(&reindex_btn);
        }
        card.append(&status_box);
        list.append(&card);
        return;
    }

    if model.content_searching {
        let loading_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
        loading_box.set_margin_top(32);
        loading_box.set_halign(gtk::Align::Center);
        let spinner = gtk::Spinner::new();
        spinner.start();
        spinner.set_size_request(32, 32);
        let lbl = gtk::Label::new(Some(&format!(
            "Searching library books for “{}”…",
            query_trimmed
        )));
        lbl.add_css_class("kalam-muted");
        loading_box.append(&spinner);
        loading_box.append(&lbl);
        list.append(&loading_box);
        return;
    }

    if model.content_results.is_empty() {
        let empty_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        empty_box.set_margin_top(32);
        empty_box.set_halign(gtk::Align::Center);
        let icon = gtk::Image::from_icon_name("system-search-symbolic");
        icon.set_pixel_size(48);
        icon.add_css_class("kalam-muted");
        let lbl = gtk::Label::new(Some(&format!(
            "No passages found matching “{}”.",
            query_trimmed
        )));
        lbl.add_css_class("kalam-placeholder");
        let hint = gtk::Label::new(Some(
            "Try searching for single words, checking spelling, or indexing unindexed books.",
        ));
        hint.add_css_class("kalam-muted");
        empty_box.append(&icon);
        empty_box.append(&lbl);
        empty_box.append(&hint);
        list.append(&empty_box);
        return;
    }

    let summary_lbl = gtk::Label::new(Some(&format!(
        "Found {} match{} across {} book{} ({} ms)",
        model.content_total_matches,
        if model.content_total_matches == 1 {
            ""
        } else {
            "es"
        },
        model.content_total_books,
        if model.content_total_books == 1 {
            ""
        } else {
            "s"
        },
        model.content_duration_ms,
    )));
    summary_lbl.add_css_class("kalam-muted");
    summary_lbl.set_halign(gtk::Align::Start);
    summary_lbl.set_margin_bottom(12);
    list.append(&summary_lbl);

    for res in &model.content_results {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 10);
        card.add_css_class("kalam-card");
        card.set_margin_bottom(12);

        // Header row: Cover + Title/Author/Match count + Action buttons
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        header.set_valign(gtk::Align::Center);

        // Cover
        let pic = gtk::Picture::new();
        pic.set_can_shrink(true);
        pic.set_size_request(48, 70);
        if let Some(ref cover_path) = res.book.cover_path {
            if cover_path.exists() {
                pic.set_filename(Some(cover_path));
            }
        }
        header.append(&pic);

        // Info
        let info = gtk::Box::new(gtk::Orientation::Vertical, 4);
        info.set_hexpand(true);
        let title_lbl = gtk::Label::new(Some(&res.book.title));
        title_lbl.add_css_class("kalam-card-title");
        title_lbl.set_halign(gtk::Align::Start);
        title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);

        let author_lbl = gtk::Label::new(Some(&res.book.authors));
        author_lbl.add_css_class("kalam-card-meta");
        author_lbl.set_halign(gtk::Align::Start);

        let badge_lbl = gtk::Label::new(Some(&format!(
            "{} match{}",
            res.total_matches,
            if res.total_matches == 1 { "" } else { "es" },
        )));
        badge_lbl.add_css_class("kalam-card-badge");
        badge_lbl.set_halign(gtk::Align::Start);

        info.append(&title_lbl);
        info.append(&author_lbl);
        info.append(&badge_lbl);
        header.append(&info);

        // Actions: [ Read ] and [ Details ]
        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        actions.set_valign(gtk::Align::Center);

        let read_btn = gtk::Button::with_label("Read Book");
        read_btn.add_css_class("kalam-primary-btn");
        let b_id = res.book.id;
        let s = sender.clone();
        read_btn.connect_clicked(move |_| {
            s.output(AllBooksOut::OpenReader {
                book_id: b_id,
                chapter: None,
            })
            .ok();
        });
        actions.append(&read_btn);

        let details_btn = gtk::Button::with_label("Details");
        details_btn.add_css_class("kalam-secondary-btn");
        let s_det = sender.clone();
        details_btn.connect_clicked(move |_| {
            s_det.output(AllBooksOut::OpenBook { book_id: b_id }).ok();
        });
        actions.append(&details_btn);

        header.append(&actions);
        card.append(&header);

        // Snippets container
        let snippets_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        snippets_box.set_margin_top(8);

        let is_expanded = model.expanded_books.contains(&res.book.id);
        let visible_matches = if is_expanded {
            &res.matches[..]
        } else {
            let cap = res.matches.len().min(3);
            &res.matches[..cap]
        };

        for m in visible_matches {
            let snip_btn = gtk::Button::new();
            snip_btn.add_css_class("kalam-search-snippet-row");
            snip_btn.set_has_frame(false);

            let content_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
            let ch_lbl = gtk::Label::new(Some(&m.chapter_title));
            ch_lbl.add_css_class("kalam-search-snippet-chapter");
            ch_lbl.set_halign(gtk::Align::Start);

            let txt_lbl = gtk::Label::new(None);
            txt_lbl.set_use_markup(true);
            txt_lbl.set_markup(&m.snippet);
            txt_lbl.add_css_class("kalam-search-snippet-text");
            txt_lbl.set_halign(gtk::Align::Start);
            txt_lbl.set_xalign(0.0);
            txt_lbl.set_wrap(true);

            content_box.append(&ch_lbl);
            content_box.append(&txt_lbl);
            snip_btn.set_child(Some(&content_box));

            let s_snip = sender.clone();
            let ch_idx = m.chapter_index;
            snip_btn.connect_clicked(move |_| {
                s_snip
                    .output(AllBooksOut::OpenReader {
                        book_id: b_id,
                        chapter: Some(ch_idx),
                    })
                    .ok();
            });
            snippets_box.append(&snip_btn);
        }
        card.append(&snippets_box);

        if res.matches.len() > 3 {
            let toggle_btn = gtk::Button::new();
            toggle_btn.add_css_class("kalam-secondary-btn");
            toggle_btn.set_halign(gtk::Align::Start);
            toggle_btn.set_margin_top(4);
            if is_expanded {
                toggle_btn.set_label("Show fewer matches ▲");
            } else {
                let more = res.matches.len() - 3;
                toggle_btn.set_label(&format!(
                    "Show {more} more match{} in this book ▼",
                    if more == 1 { "" } else { "es" }
                ));
            }
            let s_exp = sender.clone();
            toggle_btn.connect_clicked(move |_| {
                s_exp.input(AllBooksMsg::ToggleExpandBook(b_id));
            });
            card.append(&toggle_btn);
        }

        list.append(&card);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_import_reads_naturally() {
        let tally = ImportTally {
            imported: 3,
            last_title: "Dune".into(),
            ..Default::default()
        };
        assert_eq!(
            import_summary(&tally),
            "Import done — 3 added, 0 already in library, 0 failed. Last: Dune"
        );
    }

    #[test]
    fn restored_edits_are_called_out_but_only_when_there_were_some() {
        // The note exists so a book that comes back with its old title does
        // not look like the import ignored the file.
        let none = ImportTally {
            imported: 1,
            ..Default::default()
        };
        assert!(!import_summary(&none).contains("kept your earlier"));

        let some = ImportTally {
            imported: 1,
            restored: 1,
            ..Default::default()
        };
        assert!(import_summary(&some).contains("1 kept your earlier metadata edits."));
    }

    /// Reading data gets its own clause, and never borrows the metadata one.
    ///
    /// This is the regression that splitting the restore outcome was for: the
    /// summary used to say "kept your earlier metadata edits" for any restore at
    /// all, so a book that came back with only its highlights was described as
    /// having kept edits nobody had made.
    #[test]
    fn a_restore_of_reading_data_does_not_claim_metadata_edits() {
        let reading_only = ImportTally {
            imported: 1,
            restored_books: vec![RestoredBook {
                id: 1,
                title: "Dune".to_string(),
                authors: "Frank Herbert".to_string(),
            }],
            ..Default::default()
        };
        let s = import_summary(&reading_only);
        assert!(
            s.contains("1 came back with your notes and marks."),
            "the reading clause is missing: {s}"
        );
        assert!(
            !s.contains("metadata edits"),
            "a highlights-only restore claimed metadata edits: {s}"
        );

        // And the two facts coexist without either swallowing the other.
        let both = ImportTally {
            imported: 2,
            restored: 1,
            restored_books: vec![
                RestoredBook {
                    id: 1,
                    title: "Dune".to_string(),
                    authors: "Frank Herbert".to_string(),
                },
                RestoredBook {
                    id: 2,
                    title: "Neuromancer".to_string(),
                    authors: "William Gibson".to_string(),
                },
            ],
            ..Default::default()
        };
        let s = import_summary(&both);
        assert!(s.contains("1 kept your earlier metadata edits."), "{s}");
        assert!(s.contains("2 came back with your notes and marks."), "{s}");
    }

    #[test]
    fn an_empty_title_leaves_off_the_last_clause() {
        // Every file failing means there is no title to report; the summary
        // should not trail off with a dangling "Last: ".
        let tally = ImportTally {
            errors: 2,
            ..Default::default()
        };
        let text = import_summary(&tally);
        assert!(text.ends_with("2 failed."), "got {text:?}");
        assert!(!text.contains("Last:"));
    }
}
