//! Root Relm4 application: slim sidebar + routed main content.

use crate::db::Catalog;
use crate::models::{LibrarySection, NavItem, Route};
use crate::shortcuts::ShortcutAction;
use crate::pages::{downloads::DownloadsModel, 
    all_books::{AllBooksModel, AllBooksMsg, AllBooksOut},
    analytics::AnalyticsModel,
    author::{AuthorPageModel, AuthorPageOut},
    authors::{AuthorsModel, AuthorsOut},
    book::{BookPageModel, BookPageOut},
    book_float::{BookFloatModel, BookFloatOut},
    comics::{ComicsModel, ComicsMsg, ComicsOut},
    browse::{BrowseModel, BrowseOut},
    remote_detail::{RemoteDetailModel, RemoteDetailOut, RemoteDetailInit},
    comics_reader::{ComicsReaderModel, ComicsReaderOut},
    component_preview::ComponentPreviewModel,
    history::{HistoryModel, HistoryOut},
    home::{HomeOut, HomePageModel},
    library::{LibraryOut, LibraryPageModel},
    lookup_history::LookupHistoryModel,
    placeholder::PlaceholderPageModel,
    pdf_reader::{PdfReaderInit, PdfReaderModel, PdfReaderOut},
    editor_picker::{EditorPickerModel, EditorPickerMsg, EditorPickerOut},
    epub_editor::{EpubEditorModel, EpubEditorOut},
    reader::{ReaderModel, ReaderOut},
    reading_list::{ReadingListModel, ReadingListOut},
    review::{ReviewModel, ReviewOut},
    saved_quotes::{SavedQuotesModel, SavedQuotesOut},
    saved_words::{SavedWordsModel, SavedWordsOut},
    series::{SeriesPageInit, SeriesPageModel, SeriesPageOut},
    settings::{SettingsMsg, SettingsPageModel, SettingsTab},
    shelf_detail::{ShelfDetailModel, ShelfDetailOut},
    shelves_grid::{ShelvesGridModel, ShelvesOut},
    tags::{TagBooksModel, TagBooksOut, TagsModel, TagsOut},
    task_manager::TaskManagerModel,
};
use gtk::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

#[derive(Debug)]
pub enum AppMsg {
    Navigate(NavItem),
    Push(Route),
    /// "Show books in library" (author page, decision 112): push the
    /// All Books section with the author search already typed in.
    ShowInLibrary {
        query: String,
    },
    Back,
    /// One app-wide shortcut from the decision-155 registry. The root
    /// key handler resolves the chord; this dispatch owns the route.
    Shortcut(crate::shortcuts::ShortcutAction),
    /// A shortcut-started import finished. Routed through the model so
    /// completion can refresh a listing without rebuilding a reader
    /// or resetting the Settings page that launched it.
    ShortcutImportFinished(crate::pages::all_books::ImportTally),
    OpenBookDialog {
        book_id: i64,
    },
    /// Close float and open full book page in the main column.
    FloatOpenFull {
        book_id: i64,
    },
    CloseBookDialog,
    /// Open the shelves checklist panel (in-app float).
    OpenShelvesFloat {
        book_id: i64,
        /// True when opened from the book float — closing returns there.
        from_book_float: bool,
    },
    /// Rebuild the page on screen if the catalog changed under it.
    RefreshCurrentPage,
    /// Open immersive reader for book_id.
    OpenReader {
        book_id: i64,
    },
    /// Open reader for book_id optionally positioned at chapter.
    OpenReaderAt {
        book_id: i64,
        chapter: Option<usize>,
    },
    /// Open a book directly in a floating bubble window.
    OpenBubble {
        book_id: i64,
    },
    /// Minimize an open reader into the floating bubble stack.
    MinimizeToBubble {
        book_id: i64,
    },
    BubbleCloseCurrent,
    BubbleSelect {
        book_id: i64,
    },
    BubbleClose {
        book_id: i64,
    },
    BubbleSetSplit {
        book_id: i64,
    },
    BubbleToggleSplit,
    BubbleCloseSplit,
    BubbleMinimizeWindow,
    BubbleCloseWindow,
    BubbleSetActivePane(crate::bubbles::ActivePane),
    /// Reload auto-import watch folder preferences and rebind monitors.
    ReloadWatchFolder,
}

enum PageSlot {
    Home(Controller<HomePageModel>),
    Library(Controller<LibraryPageModel>),
    AllBooks(Controller<AllBooksModel>),
    SavedQuotes(Controller<SavedQuotesModel>),
    SavedWords(Controller<SavedWordsModel>),
    Review(Controller<ReviewModel>),
    LookupHistory(Controller<LookupHistoryModel>),
    Shelves(Controller<ShelvesGridModel>),
    ShelfDetail(Controller<ShelfDetailModel>),
    ReadingList(Controller<ReadingListModel>),
    History(Controller<HistoryModel>),
    Tags(Controller<TagsModel>),
    TagBooks(Controller<TagBooksModel>),
    Analytics(Controller<AnalyticsModel>),
    TaskManager(Controller<TaskManagerModel>),
    Authors(Controller<AuthorsModel>),
    Author(Controller<AuthorPageModel>),
    Book(Controller<BookPageModel>),
    Series(Controller<SeriesPageModel>),
    Reader(Controller<ReaderModel>),
    PdfReader(Controller<PdfReaderModel>),
    Comics(Controller<ComicsModel>),
    ComicsReader(Controller<ComicsReaderModel>),
    RemoteBrowse(Controller<BrowseModel>),
    RemoteDetail(Controller<RemoteDetailModel>),
    Settings(Controller<SettingsPageModel>),
    ComponentPreview(Controller<ComponentPreviewModel>),
    Placeholder(Controller<PlaceholderPageModel>),
    Downloads(Controller<DownloadsModel>),
    EditorPicker(Controller<EditorPickerModel>),
    EpubEditor(Controller<EpubEditorModel>),
}

impl PageSlot {
    fn widget(&self) -> gtk::Widget {
        match self {
            PageSlot::Downloads(c) => c.widget().clone().upcast(),
            PageSlot::EditorPicker(c) => c.widget().clone().upcast(),
            PageSlot::EpubEditor(c) => c.widget().clone().upcast(),
            PageSlot::Home(c) => c.widget().clone().upcast(),
            PageSlot::Library(c) => c.widget().clone().upcast(),
            PageSlot::AllBooks(c) => c.widget().clone().upcast(),
            PageSlot::SavedQuotes(c) => c.widget().clone().upcast(),
            PageSlot::SavedWords(c) => c.widget().clone().upcast(),
            PageSlot::Review(c) => c.widget().clone().upcast(),
            PageSlot::LookupHistory(c) => c.widget().clone().upcast(),
            PageSlot::Shelves(c) => c.widget().clone().upcast(),
            PageSlot::ShelfDetail(c) => c.widget().clone().upcast(),
            PageSlot::ReadingList(c) => c.widget().clone().upcast(),
            PageSlot::History(c) => c.widget().clone().upcast(),
            PageSlot::Tags(c) => c.widget().clone().upcast(),
            PageSlot::TagBooks(c) => c.widget().clone().upcast(),
            PageSlot::Analytics(c) => c.widget().clone().upcast(),
            PageSlot::TaskManager(c) => c.widget().clone().upcast(),
            PageSlot::Authors(c) => c.widget().clone().upcast(),
            PageSlot::Author(c) => c.widget().clone().upcast(),
            PageSlot::Book(c) => c.widget().clone().upcast(),
            PageSlot::Series(c) => c.widget().clone().upcast(),
            PageSlot::Reader(c) => c.widget().clone().upcast(),
            PageSlot::PdfReader(c) => c.widget().clone().upcast(),
            PageSlot::Comics(c) => c.widget().clone().upcast(),
            PageSlot::ComicsReader(c) => c.widget().clone().upcast(),
            PageSlot::RemoteBrowse(c) => c.widget().clone().upcast(),
            PageSlot::RemoteDetail(c) => c.widget().clone().upcast(),
            PageSlot::Settings(c) => c.widget().clone().upcast(),
            PageSlot::ComponentPreview(c) => c.widget().clone().upcast(),
            PageSlot::Placeholder(c) => c.widget().clone().upcast(),
        }
    }
}

/// Which float is on screen — only one at a time. The controller is held
/// (never read) purely to keep the component alive until the float closes.
enum Floating {
    // The controller is stored (never read) purely to keep the float's
    // component alive until it closes — see the enum doc.
    #[allow(dead_code)]
    Book(Controller<BookFloatModel>),
    /// Shelves checklist — a plain widget panel, nothing to keep alive.
    /// `return_to` holds the book id when the panel was opened from the book
    /// float, so closing it hands control back to that float, not the page.
    Shelves {
        return_to: Option<i64>,
    },
}

pub struct AppModel {
    catalog: Arc<Catalog>,
    source_manager: Arc<crate::sources::SourceManager>,
    route: Route,
    history: Vec<Route>,
    sidebar_override: Option<NavItem>,
    page: Option<PageSlot>,
    floating: Option<Floating>,
    float_scrim: gtk::Box,
    float_host: gtk::Box,
    /// Pages kept alive between visits, keyed by route.
    ///
    /// Rebuilding a whole widget tree on every click was the second half of
    /// the UI lag: revisiting Home or Library reconstructed dozens of widgets
    /// and re-ran their queries. Cached pages are unparented rather than
    /// destroyed, so returning to one costs nothing.
    cache: Vec<(String, PageSlot)>,
    /// Catalog write counter at the time each cached page was built — the
    /// *page-cache* token, which ignores reading activity (see
    /// [`Catalog::page_cache_token`]). A cached page is only reused while
    /// this matches, so an import, delete or edit anywhere automatically
    /// forces a rebuild — no write path has to remember to invalidate.
    cache_token: i64,
    /// Set when navigating away from a reader. Reading no longer evicts the
    /// cache, so the page you return to would otherwise come back from the
    /// cache showing the reading position it had before you read — the
    /// continue card is too prominent for that. One forced rebuild of the
    /// landing page, and every *other* cached page survives the session.
    force_rebuild_next: bool,
    pub bubbles: crate::bubbles::BubbleManager,
    pub watch_folder: Option<crate::watch_folder::WatchFolderService>,
}

/// Cache key for a route, or `None` for pages that must always be rebuilt.
///
/// Reader is excluded deliberately: it owns the reading widget and a session,
/// and must be torn down on leave. Book and shelf pages are excluded because
/// their content changes as you edit metadata, ratings and membership.
fn cache_key(route: &Route) -> Option<String> {
    match route {
        Route::Module(NavItem::Home) => Some("home".into()),
        Route::Module(NavItem::Library) => Some("library".into()),
        Route::Module(NavItem::Shelves) | Route::ShelvesGrid => Some("shelves".into()),
        Route::Module(NavItem::Settings) => Some("settings".into()),
        // The authors index changes with every book edit and author
        // merge; rebuild it rather than serving a cached roster.
        Route::Module(NavItem::Authors) | Route::AuthorsIndex => None,
        // The picker carries each book's edited state; the workspace
        // owns buffers and undo history. Neither may be cache-served.
        Route::Module(NavItem::Editor) => None,
        Route::Module(item) => Some(format!("mod:{}", item.label())),
        Route::ComponentPreview => Some("components".into()),
        // Everything below reflects data the user is actively changing.
        Route::LibrarySection(_)
        | Route::ShelfDetail { .. }
        | Route::TagBooks { .. }
        | Route::AuthorPage { .. }
        | Route::BookPage { .. }
        | Route::EditorWorkspace { .. }
        | Route::SeriesPage { .. }
        | Route::ComicSeries { .. }
        | Route::Reader { .. }
        | Route::PdfReader { .. }
        | Route::ComicsReader { .. }
        | Route::RemoteDetail { .. }
        | Route::RemoteReader { .. }
        | Route::RemoteSearch { .. } => None,
    }
}

impl AppModel {
    fn sidebar_item(&self) -> NavItem {
        self.sidebar_override
            .unwrap_or_else(|| self.route.sidebar_item())
    }

    fn show_back_chip(&self) -> bool {
        !self.history.is_empty() && !self.route.owns_its_surface()
    }

    fn close_floating(&mut self) {
        while let Some(child) = self.float_host.first_child() {
            self.float_host.remove(&child);
        }
        self.float_host.set_visible(false);
        self.float_scrim.set_visible(false);
        self.floating = None;
    }

    fn open_floating(&mut self, book_id: i64, sender: &ComponentSender<Self>) {
        // The book dialog is one of the field-reported slow paths ("the
        // pause is when things OPEN: tapping a book, opening dialogs") —
        // its `init` reads `book_detail` inline. Measured and attributed
        // until the migration moves that read off the UI thread.
        let _span = crate::timing::measure("dialog_open:book_float");
        let _activity = crate::timing::activity("dialog_open:book_float");
        self.close_floating();

        let ctrl = BookFloatModel::builder()
            .launch((self.catalog.clone(), book_id))
            .forward(sender.input_sender(), move |out| match out {
                BookFloatOut::Close | BookFloatOut::Deleted { .. } => AppMsg::CloseBookDialog,
                BookFloatOut::OpenReader { book_id } => AppMsg::OpenReader { book_id },
                BookFloatOut::OpenBubble { book_id } => AppMsg::OpenBubble { book_id },
                BookFloatOut::OpenFullPage { book_id } => AppMsg::FloatOpenFull { book_id },
                BookFloatOut::OpenAuthor { name } => {
                    AppMsg::Push(Route::AuthorPage { author: name })
                }
                BookFloatOut::ShowShelves => AppMsg::OpenShelvesFloat {
                    book_id,
                    from_book_float: true,
                },
            });

        let float = ctrl.widget().clone();
        // 720x420 is a *floor*, not a size: GTK grows a widget past its size
        // request whenever the content needs more room, which is why the panel
        // used to change size from book to book. The content itself is now
        // bounded (see the tag row, title, author line and description section
        // in `book_float.rs`); pinning halign/valign to Center rather than Fill
        // keeps the panel at its requested size instead of stretching it.
        float.set_size_request(720, 420);
        float.set_hexpand(false);
        float.set_vexpand(false);
        float.set_halign(gtk::Align::Center);
        float.set_valign(gtk::Align::Center);
        self.float_host.append(&float);
        self.float_scrim.set_visible(true);
        self.float_host.set_visible(true);
        float.grab_focus();

        self.floating = Some(Floating::Book(ctrl));
    }

    fn open_shelves_floating(
        &mut self,
        book_id: i64,
        from_book_float: bool,
        sender: &ComponentSender<Self>,
    ) {
        let _span = crate::timing::measure("dialog_open:shelves_float");
        let _activity = crate::timing::activity("dialog_open:shelves_float");
        self.close_floating();

        let s = sender.clone();
        let panel =
            crate::pages::book::build_shelves_panel(self.catalog.clone(), book_id, move || {
                s.input(AppMsg::CloseBookDialog)
            });
        panel.set_size_request(380, 420);
        panel.set_hexpand(false);
        panel.set_vexpand(false);
        panel.set_halign(gtk::Align::Center);
        panel.set_valign(gtk::Align::Center);
        self.float_host.append(&panel);
        self.float_scrim.set_visible(true);
        self.float_host.set_visible(true);
        panel.grab_focus();

        self.floating = Some(Floating::Shelves {
            return_to: from_book_float.then_some(book_id),
        });
    }

    fn build_page(
        catalog: &Arc<Catalog>,
        source_manager: &Arc<crate::sources::SourceManager>,
        route: &Route,
        sender: &ComponentSender<Self>,
    ) -> PageSlot {
        // Roadmap 7.1 step 0: every route construction is measured (the
        // `route_open:` span, printed under KALAM_TIMING=1) and announced to
        // the always-on activity stack, so a UI-thread block while a page is
        // built is attributed to the page by name in the stall watchdog's
        // log. Both guards live for the whole match below. The page cache's
        // hit path never enters this function — a cached page is reparented,
        // not rebuilt — so these numbers are construction cost, nothing else.
        let label = route_label(route);
        let _span = crate::timing::measure(label);
        let _activity = crate::timing::activity(label);
        match route {
            Route::Module(NavItem::Home) => {
                let ctrl = HomePageModel::builder().launch(catalog.clone()).forward(
                    sender.input_sender(),
                    |out| match out {
                        HomeOut::Book { book_id } => AppMsg::Push(Route::BookPage { book_id }),
                        HomeOut::BookDialog { book_id } => AppMsg::OpenBookDialog { book_id },
                        HomeOut::AllBooks => {
                            AppMsg::Push(Route::LibrarySection(LibrarySection::AllBooks))
                        }
                        HomeOut::ComicSeries { series_name } => {
                            AppMsg::Push(Route::ComicSeries { series_name })
                        }
                        HomeOut::Search { query, .. } => {
                            // Open the full search page (All Books) with the query.
                            AppMsg::Push(Route::LibrarySection(LibrarySection::AllBooks))
                        }
                    },
                );
                PageSlot::Home(ctrl)
            }
            Route::Module(NavItem::Library) => {
                let ctrl = LibraryPageModel::builder().launch(catalog.clone()).forward(
                    sender.input_sender(),
                    |out| match out {
                        LibraryOut::Section(sec) => AppMsg::Push(Route::LibrarySection(sec)),
                        LibraryOut::Shelves => AppMsg::Push(Route::ShelvesGrid),
                        LibraryOut::Book { book_id } => AppMsg::Push(Route::BookPage { book_id }),
                        LibraryOut::BookDialog { book_id } => AppMsg::OpenBookDialog { book_id },
                        LibraryOut::Read { book_id } => AppMsg::OpenReader { book_id },
                        LibraryOut::ComicSeries { series_name } => {
                            AppMsg::Push(Route::ComicSeries { series_name })
                        }
                    },
                );
                PageSlot::Library(ctrl)
            }
            Route::LibrarySection(LibrarySection::AllBooks) => {
                let ctrl = AllBooksModel::builder().launch(catalog.clone()).forward(
                    sender.input_sender(),
                    |out| match out {
                        AllBooksOut::OpenBook { book_id } => {
                            AppMsg::Push(Route::BookPage { book_id })
                        }
                        AllBooksOut::OpenBookDialog { book_id } => {
                            AppMsg::OpenBookDialog { book_id }
                        }
                        AllBooksOut::OpenReader { book_id, chapter } => {
                            AppMsg::OpenReaderAt { book_id, chapter }
                        }
                        AllBooksOut::ComicSeries { series_name } => {
                            AppMsg::Push(Route::ComicSeries { series_name })
                        }
                        AllBooksOut::OpenShelves => AppMsg::Push(Route::ShelvesGrid),
                    },
                );
                PageSlot::AllBooks(ctrl)
            }
            Route::LibrarySection(LibrarySection::SavedQuotes) => {
                let cat = catalog.clone();
                let ctrl =
                    SavedQuotesModel::builder()
                        .launch(cat)
                        .forward(sender.input_sender(), |out| match out {
                            SavedQuotesOut::OpenBook { book_id } => {
                                AppMsg::Push(Route::BookPage { book_id })
                            }
                            SavedQuotesOut::JumpTo { book_id, .. } => {
                                AppMsg::Push(Route::BookPage { book_id })
                            }
                        });
                PageSlot::SavedQuotes(ctrl)
            }
            Route::LibrarySection(LibrarySection::SavedWords) => {
                let cat = catalog.clone();
                let ctrl =
                    SavedWordsModel::builder()
                        .launch(cat)
                        .forward(sender.input_sender(), |out| match out {
                            SavedWordsOut::OpenBook { book_id } => {
                                AppMsg::Push(Route::BookPage { book_id })
                            }
                        });
                PageSlot::SavedWords(ctrl)
            }
            Route::LibrarySection(LibrarySection::Review) => {
                let cat = catalog.clone();
                let ctrl = ReviewModel::builder()
                    .launch(crate::service::LibraryService::new(cat))
                    .forward(sender.input_sender(), |out| match out {
                        ReviewOut::OpenBook { book_id } => {
                            AppMsg::Push(Route::BookPage { book_id })
                        }
                    });
                PageSlot::Review(ctrl)
            }
            Route::LibrarySection(LibrarySection::LookupHistory) => {
                let ctrl = LookupHistoryModel::builder()
                    .launch(catalog.clone())
                    .detach();
                PageSlot::LookupHistory(ctrl)
            }
            Route::LibrarySection(LibrarySection::ReadingList) => {
                let ctrl = ReadingListModel::builder().launch(catalog.clone()).forward(
                    sender.input_sender(),
                    |out| match out {
                        ReadingListOut::OpenBook { book_id } => {
                            AppMsg::Push(Route::BookPage { book_id })
                        }
                        ReadingListOut::OpenReader { book_id } => AppMsg::OpenReader { book_id },
                        ReadingListOut::ComicSeries { series_name } => {
                            AppMsg::Push(Route::ComicSeries { series_name })
                        }
                    },
                );
                PageSlot::ReadingList(ctrl)
            }
            Route::LibrarySection(LibrarySection::History) => {
                let ctrl = HistoryModel::builder().launch(catalog.clone()).forward(
                    sender.input_sender(),
                    |out| match out {
                        HistoryOut::OpenBook { book_id } => {
                            AppMsg::Push(Route::BookPage { book_id })
                        }
                    },
                );
                PageSlot::History(ctrl)
            }
            Route::LibrarySection(LibrarySection::Tags) => {
                let ctrl = TagsModel::builder().launch(catalog.clone()).forward(
                    sender.input_sender(),
                    |out| match out {
                        TagsOut::OpenTag { tag } => AppMsg::Push(Route::TagBooks { tag }),
                    },
                );
                PageSlot::Tags(ctrl)
            }
            Route::LibrarySection(LibrarySection::Analytics) => {
                let ctrl = AnalyticsModel::builder().launch(catalog.clone()).detach();
                PageSlot::Analytics(ctrl)
            }
            Route::LibrarySection(LibrarySection::TaskManager) => {
                // Takes no catalog: this page reads the task registry, which
                // is process-wide, not per-library.
                let ctrl = TaskManagerModel::builder().launch(()).detach();
                PageSlot::TaskManager(ctrl)
            }
            Route::Module(NavItem::Shelves) | Route::ShelvesGrid => {
                let ctrl = ShelvesGridModel::builder().launch(catalog.clone()).forward(
                    sender.input_sender(),
                    |out| match out {
                        ShelvesOut::OpenShelf { shelf_id } => {
                            AppMsg::Push(Route::ShelfDetail { shelf_id })
                        }
                    },
                );
                PageSlot::Shelves(ctrl)
            }
            Route::ShelfDetail { shelf_id } => {
                let ctrl = ShelfDetailModel::builder()
                    .launch((catalog.clone(), *shelf_id))
                    .forward(sender.input_sender(), |out| match out {
                        ShelfDetailOut::OpenBook { book_id } => {
                            AppMsg::Push(Route::BookPage { book_id })
                        }
                        ShelfDetailOut::OpenBookDialog { book_id } => {
                            AppMsg::OpenBookDialog { book_id }
                        }
                        ShelfDetailOut::ComicSeries { series_name } => {
                            AppMsg::Push(Route::ComicSeries { series_name })
                        }
                    });
                PageSlot::ShelfDetail(ctrl)
            }
            Route::TagBooks { tag } => {
                let ctrl = TagBooksModel::builder()
                    .launch((catalog.clone(), tag.clone()))
                    .forward(sender.input_sender(), |out| match out {
                        TagBooksOut::OpenBook { book_id } => {
                            AppMsg::Push(Route::BookPage { book_id })
                        }
                        TagBooksOut::OpenBookDialog { book_id } => {
                            AppMsg::OpenBookDialog { book_id }
                        }
                        TagBooksOut::ComicSeries { series_name } => {
                            AppMsg::Push(Route::ComicSeries { series_name })
                        }
                    });
                PageSlot::TagBooks(ctrl)
            }
            Route::Module(NavItem::Authors) | Route::AuthorsIndex => {
                let ctrl = AuthorsModel::builder()
                    .launch(crate::service::LibraryService::new(catalog.clone()))
                    .forward(sender.input_sender(), |out| match out {
                        AuthorsOut::OpenAuthor { name } => {
                            AppMsg::Push(Route::AuthorPage { author: name })
                        }
                    });
                PageSlot::Authors(ctrl)
            }
            Route::AuthorPage { author } => {
                let ctrl = AuthorPageModel::builder()
                    .launch((catalog.clone(), author.clone()))
                    .forward(sender.input_sender(), |out| match out {
                        AuthorPageOut::OpenBook { book_id } => {
                            AppMsg::Push(Route::BookPage { book_id })
                        }
                        AuthorPageOut::ComicSeries { series_name } => {
                            AppMsg::Push(Route::ComicSeries { series_name })
                        }
                        AuthorPageOut::OpenSeries {
                            series,
                            first_author,
                        } => AppMsg::Push(Route::SeriesPage {
                            series,
                            first_author,
                        }),
                        AuthorPageOut::OpenShelf { shelf_id } => {
                            AppMsg::Push(Route::ShelfDetail { shelf_id })
                        }
                        AuthorPageOut::OpenReaderAt { book_id, chapter } => {
                            AppMsg::OpenReaderAt {
                                book_id,
                                chapter: Some(chapter),
                            }
                        }
                        AuthorPageOut::ShowInLibrary { query } => {
                            AppMsg::ShowInLibrary { query }
                        }
                    });
                PageSlot::Author(ctrl)
            }
            Route::BookPage { book_id } => {
                let id = *book_id;
                let cat_for_series = catalog.clone();
                let ctrl = BookPageModel::builder()
                    .launch((crate::service::LibraryService::new(catalog.clone()), id))
                    .forward(sender.input_sender(), move |out| match out {
                        BookPageOut::OpenReader => AppMsg::OpenReader { book_id: id },
                        BookPageOut::OpenAuthor { name } => {
                            AppMsg::Push(Route::AuthorPage { author: name })
                        }
                        BookPageOut::OpenBook { book_id } => AppMsg::FloatOpenFull { book_id },
                        BookPageOut::OpenSeries {
                            series,
                            first_author,
                        } => {
                            if let Ok(Some(s)) = cat_for_series.get_comic_series_by_title(&series) {
                                AppMsg::Push(Route::ComicSeries { series_name: s.title })
                            } else {
                                AppMsg::Push(Route::SeriesPage {
                                    series,
                                    first_author,
                                })
                            }
                        }
                        BookPageOut::Deleted { .. } => AppMsg::Back,
                        BookPageOut::EditBook => {
                            AppMsg::Push(Route::EditorWorkspace { book_id: id })
                        }
                    });
                PageSlot::Book(ctrl)
            }
            Route::SeriesPage {
                series,
                first_author,
            } => {
                let init = SeriesPageInit {
                    service: crate::service::LibraryService::new(catalog.clone()),
                    series: series.clone(),
                    first_author: first_author.clone(),
                };
                let ctrl = SeriesPageModel::builder()
                    .launch(init)
                    .forward(sender.input_sender(), |out| match out {
                        SeriesPageOut::OpenBook { book_id } => {
                            AppMsg::Push(Route::BookPage { book_id })
                        }
                        SeriesPageOut::OpenReader { book_id } => {
                            AppMsg::OpenReader { book_id }
                        }
                        SeriesPageOut::OpenAuthor { name } => {
                            AppMsg::Push(Route::AuthorPage { author: name })
                        }
                        SeriesPageOut::Back => AppMsg::Back,
                    });
                PageSlot::Series(ctrl)
            }
            Route::Reader { book_id } => {
                let id = *book_id;
                let ctrl = ReaderModel::builder()
                    .launch((catalog.clone(), id))
                    .forward(sender.input_sender(), |out| match out {
                        ReaderOut::Close => AppMsg::Back,
                        ReaderOut::MinimizeToBubble { book_id } => {
                            AppMsg::MinimizeToBubble { book_id }
                        }
                        ReaderOut::OpenAuthor { name } => {
                            AppMsg::Push(Route::AuthorPage { author: name })
                        }
                        // The book head's pencil (decision 20): out to
                        // the book's own page, the book left in place.
                        ReaderOut::OpenBook { book_id } => {
                            AppMsg::Push(Route::BookPage { book_id })
                        }
                    });
                PageSlot::Reader(ctrl)
            }
            Route::PdfReader { book_id } => {
                let init = PdfReaderInit {
                    book_id: *book_id,
                    catalog: catalog.clone(),
                };
                let ctrl = PdfReaderModel::builder()
                    .launch(init)
                    .forward(sender.input_sender(), |out| match out {
                        PdfReaderOut::Close => AppMsg::Back,
                        PdfReaderOut::MinimizeToBubble { book_id } => {
                            AppMsg::MinimizeToBubble { book_id }
                        }
                    });
                PageSlot::PdfReader(ctrl)
            }

            Route::RemoteReader { source_id, chapter_id, title } => {
                match crate::pages::comics_reader::providers::RemoteProvider::new(
                    source_manager.clone(),
                    source_id.clone(),
                    chapter_id.clone(),
                ) {
                    Ok(provider) => {
                        let init = crate::pages::comics_reader::types::ComicsReaderInit {
                            title: title.clone(),
                            provider: std::sync::Arc::new(provider),
                            catalog: Some(catalog.clone()),
                            book_id: None,
                            cover_path: None,
                        };
                        let ctrl = ComicsReaderModel::builder()
                            .launch(init)
                            .forward(sender.input_sender(), |out| match out {
                                ComicsReaderOut::Close => AppMsg::Back,
                                ComicsReaderOut::MinimizeToBubble { book_id } => {
                                    AppMsg::MinimizeToBubble { book_id }
                                }
                            });
                        PageSlot::ComicsReader(ctrl)
                    }
                    Err(err) => {
                        eprintln!("Failed to open remote reader: {}", err);
                        sender.input(AppMsg::Back);
                        let ctrl = PlaceholderPageModel::builder().launch(NavItem::RemoteBrowse).detach();
                        PageSlot::Placeholder(ctrl)
                    }
                }
            }

            Route::ComicsReader { book_id } => {
                let id = *book_id;
                // Use proper error handling — if the book row is missing or its
                // file has moved/been deleted, log and go back instead of aborting
                // the whole GTK process with a double-unwrap panic.
                let book_result = catalog.get_book(id);
                match book_result {
                    Ok(Some(book)) => {
                        match crate::pages::comics_reader::providers::LocalProvider::new(book.file_path.clone()) {
                            Ok(provider) => {
                                let init = crate::pages::comics_reader::types::ComicsReaderInit {
                                    title: book.title.clone(),
                                    provider: std::sync::Arc::new(provider),
                                    catalog: Some(catalog.clone()),
                                    book_id: Some(id),
                                    cover_path: book.cover_path.clone(),
                                };
                                let ctrl = ComicsReaderModel::builder()
                                    .launch(init)
                                    .forward(sender.input_sender(), |out| match out {
                                        ComicsReaderOut::Close => AppMsg::Back,
                                        ComicsReaderOut::MinimizeToBubble { book_id } => {
                                            AppMsg::MinimizeToBubble { book_id }
                                        }
                                    });
                                PageSlot::ComicsReader(ctrl)
                            }
                            Err(err) => {
                                eprintln!("kalam: failed to open comic file for book {id}: {err}");
                                sender.input(AppMsg::Back);
                                let ctrl = PlaceholderPageModel::builder().launch(NavItem::Comics).detach();
                                PageSlot::Placeholder(ctrl)
                            }
                        }
                    }
                    Ok(None) => {
                        eprintln!("kalam: comic book {id} not found in catalog");
                        sender.input(AppMsg::Back);
                        let ctrl = PlaceholderPageModel::builder().launch(NavItem::Comics).detach();
                        PageSlot::Placeholder(ctrl)
                    }
                    Err(err) => {
                        eprintln!("kalam: database error looking up comic book {id}: {err}");
                        sender.input(AppMsg::Back);
                        let ctrl = PlaceholderPageModel::builder().launch(NavItem::Comics).detach();
                        PageSlot::Placeholder(ctrl)
                    }
                }
            }

            Route::Module(NavItem::Comics) => {
                let ctrl = ComicsModel::builder()
                    .launch(catalog.clone())
                    .forward(sender.input_sender(), |out| match out {
                        ComicsOut::OpenComic { book_id } => AppMsg::OpenReader { book_id },
                        ComicsOut::OpenBookDialog { book_id } => AppMsg::OpenBookDialog { book_id },
                        ComicsOut::OpenRemoteManga { source_id, remote_id } => {
                            AppMsg::Push(Route::RemoteDetail { source_id, remote_id })
                        }
                    });
                PageSlot::Comics(ctrl)
            }
            Route::ComicSeries { series_name } => {
                let sname = series_name.clone();
                let ctrl = ComicsModel::builder()
                    .launch(catalog.clone())
                    .forward(sender.input_sender(), |out| match out {
                        ComicsOut::OpenComic { book_id } => AppMsg::OpenReader { book_id },
                        ComicsOut::OpenBookDialog { book_id } => AppMsg::OpenBookDialog { book_id },
                        ComicsOut::OpenRemoteManga { source_id, remote_id } => {
                            AppMsg::Push(Route::RemoteDetail { source_id, remote_id })
                        }
                    });
                let _ = ctrl.sender().send(ComicsMsg::OpenSeriesDrawer(sname));
                PageSlot::Comics(ctrl)
            }
            Route::Module(NavItem::Downloads) => {
                let ctrl = DownloadsModel::builder().launch(()).detach();
                PageSlot::Downloads(ctrl)
            }
            Route::Module(NavItem::Settings) => {
                let ctrl = SettingsPageModel::builder()
                    .launch(catalog.clone())
                    .detach();
                PageSlot::Settings(ctrl)
            }
            Route::ComponentPreview => {
                let ctrl = ComponentPreviewModel::builder().launch(()).detach();
                PageSlot::ComponentPreview(ctrl)
            }
            Route::Module(NavItem::RemoteBrowse) => {
                let init = crate::pages::browse::BrowseInit {
                    manager: source_manager.clone(),
                    source_id: "weebcentral".to_string(),
                    initial_query: None,
                };
                let ctrl = BrowseModel::builder()
                    .launch(init)
                    .forward(sender.input_sender(), |out| match out {
                        BrowseOut::OpenRemoteBook { source_id, remote_id } => {
                            AppMsg::Push(Route::RemoteDetail { source_id, remote_id })
                        }
                    });
                PageSlot::RemoteBrowse(ctrl)
            }
            Route::Module(NavItem::Fanfiction) => {
                let init = crate::pages::browse::BrowseInit {
                    manager: source_manager.clone(),
                    source_id: "ao3".to_string(),
                    initial_query: None,
                };
                let ctrl = BrowseModel::builder()
                    .launch(init)
                    .forward(sender.input_sender(), |out| match out {
                        BrowseOut::OpenRemoteBook { source_id, remote_id } => {
                            AppMsg::Push(Route::RemoteDetail { source_id, remote_id })
                        }
                    });
                PageSlot::RemoteBrowse(ctrl) // Reuse the same page slot for both!
            }
            Route::RemoteSearch { source_id, query } => {
                let init = crate::pages::browse::BrowseInit {
                    manager: source_manager.clone(),
                    source_id: source_id.clone(),
                    initial_query: Some(query.clone()),
                };
                let ctrl = BrowseModel::builder()
                    .launch(init)
                    .forward(sender.input_sender(), |out| match out {
                        BrowseOut::OpenRemoteBook { source_id, remote_id } => {
                            AppMsg::Push(Route::RemoteDetail { source_id, remote_id })
                        }
                    });
                PageSlot::RemoteBrowse(ctrl)
            }
            Route::RemoteDetail { source_id, remote_id } => {
                let init = RemoteDetailInit {
                    manager: source_manager.clone(),
                    catalog: catalog.clone(),
                    source_id: source_id.clone(),
                    remote_id: remote_id.clone(),
                };
                let ctrl = RemoteDetailModel::builder()
                    .launch(init)
                    .forward(sender.input_sender(), |out| match out {
                        RemoteDetailOut::Back => AppMsg::Back,
                        RemoteDetailOut::OpenReader { source_id, chapter_id, title } => {
                            AppMsg::Push(Route::RemoteReader { source_id, chapter_id, title })
                        }
                        RemoteDetailOut::OpenBook { book_id } => {
                            AppMsg::Push(Route::Reader { book_id })
                        }
                        RemoteDetailOut::OpenAuthor { source_id, author } => {
                            AppMsg::Push(Route::RemoteSearch { source_id, query: author })
                        }
                    });
                PageSlot::RemoteDetail(ctrl)
            }
            Route::Module(NavItem::Editor) => {
                let ctrl = EditorPickerModel::builder()
                    .launch(crate::service::LibraryService::new(catalog.clone()))
                    .forward(sender.input_sender(), |out| match out {
                        EditorPickerOut::OpenEditor { book_id } => {
                            AppMsg::Push(Route::EditorWorkspace { book_id })
                        }
                    });
                PageSlot::EditorPicker(ctrl)
            }
            Route::EditorWorkspace { book_id } => {
                let ctrl = EpubEditorModel::builder()
                    .launch((
                        crate::service::LibraryService::new(catalog.clone()),
                        *book_id,
                    ))
                    .forward(sender.input_sender(), |out| match out {
                        EpubEditorOut::Leave => AppMsg::Back,
                    });
                PageSlot::EpubEditor(ctrl)
            }
        }
    }

    fn swap_page(
        &mut self,
        content_host: &gtk::Box,
        route: Route,
        push_history: bool,
        sender: &ComponentSender<Self>,
    ) {
        if push_history {
            self.history.push(self.route.clone());
        } else {
            self.history.clear();
        }

        self.sidebar_override = match &route {
            Route::BookPage { .. } | Route::Reader { .. } => {
                Some(self.sidebar_item())
            }
            _ => None,
        };

        // Leaving a reader: the landing page is rebuilt fresh (its reading
        // position is stale by definition), everything else stays cached.
        self.force_rebuild_next = matches!(
            self.route,
            Route::Reader { .. }
                | Route::PdfReader { .. }
                | Route::ComicsReader { .. }
                | Route::EditorWorkspace { .. }
        );
        self.detach_current(content_host);

        self.route = route;
        let page = self.take_or_build(sender);
        content_host.append(&page.widget());
        self.page = Some(page);
        sync_content_classes(content_host, &self.route, self.show_back_chip());
    }

    /// Unparent the current page, parking it in the cache when its route is
    /// cacheable and dropping it otherwise.
    fn detach_current(&mut self, content_host: &gtk::Box) {
        while let Some(child) = content_host.first_child() {
            content_host.remove(&child);
        }

        // Only drop the controller *after* unparenting — otherwise GTK probes
        // a disposed widget and logs gtk_widget_is_ancestor criticals.
        let Some(page) = self.page.take() else {
            return;
        };
        if let Some(key) = cache_key(&self.route) {
            if !self.cache.iter().any(|(k, _)| *k == key) {
                self.cache.push((key, page));
            }
        }
    }

    /// Rebuild the current page if the catalog changed since it was built.
    ///
    /// Deleting a book only updated the database; whatever page was on screen
    /// kept its stale widgets until the next navigation, so a removed book
    /// lingered on Home until you switched tabs.
    fn refresh_if_stale(&mut self, content_host: &gtk::Box, sender: &ComponentSender<Self>) {
        let token = self.catalog.page_cache_token();
        if token == self.cache_token {
            return;
        }

        // Unparent before dropping, as everywhere else, or GTK complains about
        // a disposed widget. Nothing is cached here: every cached page was
        // built against the old catalog state.
        while let Some(child) = content_host.first_child() {
            content_host.remove(&child);
        }
        self.page = None;
        self.cache.clear();
        self.cache_token = token;

        // Rebuild the same route in place — no history push.
        let page = Self::build_page(&self.catalog, &self.source_manager, &self.route, sender);
        content_host.append(&page.widget());
        self.page = Some(page);
    }

    /// Reuse a cached page for the current route, or build a fresh one.
    fn take_or_build(&mut self, sender: &ComponentSender<Self>) -> PageSlot {
        // Any content write invalidates every cached page: a stale Home would
        // happily show a book you just deleted. Reading activity (progress
        // saves, session telemetry, mark-opened) deliberately does NOT — the
        // owner's field run measured this cache mostly dead because every
        // scroll evicted it; see `Catalog::page_cache_token`.
        let token = self.catalog.page_cache_token();
        if token != self.cache_token {
            self.cache.clear();
            self.cache_token = token;
        }

        // Leaving a reader forces one fresh build of the landing page (the
        // continue card must not show the position from before you read),
        // without touching the other cached pages.
        let forced = std::mem::take(&mut self.force_rebuild_next);

        if !forced {
            if let Some(key) = cache_key(&self.route) {
                if let Some(idx) = self.cache.iter().position(|(k, _)| *k == key) {
                    let (_, page) = self.cache.remove(idx);
                    crate::timing::note("page_cache_hit", 1);
                    return page;
                }
            }
        }
        // Instrumented because the hit rate decides whether this cache is
        // tuned further or deleted. Reading a book no longer evicts (the
        // token ignores activity writes) but still costs the landing page one
        // forced miss; under `KALAM_TIMING=1` these counters say whether the
        // cache is earning its keep. A forced miss is counted separately so
        // the two numbers stay honest.
        if forced {
            crate::timing::note("page_cache_forced_miss", 1);
        } else {
            crate::timing::note("page_cache_miss", 1);
        }
        Self::build_page(&self.catalog, &self.source_manager, &self.route, sender)
    }
}

#[relm4::component(pub)]
impl Component for AppModel {
    /// The catalog, opened once in `main()`.
    ///
    /// It used to be `()` and `init` opened its own handle — the third of
    /// three opens in one startup (`main` checked the database, then
    /// `startup_theme` opened it again to read the theme, then this one).
    /// Each open runs the whole of `migrate()`. Passing the handle in makes
    /// it one.
    type Init = Arc<Catalog>;
    type Input = AppMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        main_window = gtk::Window {
            add_css_class: "kalam-window",
            set_title: Some("Kalam"),
            set_default_width: 1100,
            set_default_height: 720,
            // No titlebar at all — not even the transparent strip with
            // window controls. The page runs edge to edge; Alt+F4 closes.
            set_decorated: false,

            // A0 step 4: tell background tasks to stop before the window goes.
            // Cancellation is cooperative, so this only sets a flag — short
            // tasks finish anyway, but a long one (the thumbnail backfill on a
            // big first launch) stops instead of decoding covers for a window
            // that no longer exists. `Proceed` so the close is not blocked.
            connect_close_request => move |_| {
                let pending = crate::tasks::running_count();
                crate::tasks::cancel_all();
                if pending > 0 {
                    crate::timing::note("tasks_cancelled_at_exit", pending);
                }
                gtk::glib::Propagation::Proceed
            },

            // One root overlay: main app under it, then a dimmed in-app book
            // panel, then toasts on top.
            #[name = "root_overlay"]
            gtk::Overlay {
                add_overlay = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::End,
                    add_css_class: "kalam-toast-host",
                    // Must not swallow clicks meant for the app beneath.
                    set_can_target: true,
                    // With no toasts up, this overlay child has no content and
                    // would be allocated 0x0 — an invalid rectangle as far as
                    // pixman is concerned. Hiding it until a toast exists keeps
                    // it out of the layout entirely.
                    set_visible: false,

                    #[name = "toast_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: 8,
                        set_halign: gtk::Align::End,
                        set_valign: gtk::Align::End,
                    },
                },

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_hexpand: true,
                    set_vexpand: true,
                    #[watch]
                    set_can_target: model.floating.is_none(),

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-sidebar",
                        set_hexpand: false,
                        set_vexpand: true,
                        #[watch]
                        set_visible: !model.route.is_reader(),

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            add_css_class: "kalam-sidebar-inner",
                            set_vexpand: true,

                            #[name = "brand"]
                            gtk::Box {
                                add_css_class: "kalam-brand",
                                set_halign: gtk::Align::Center,
                            },

                            // Equal expanding spacers above and below the nav pin
                            // it to the middle of the rail, with the logo held at
                            // the top and Settings at the bottom.
                            gtk::Box {
                                set_vexpand: true,
                                add_css_class: "kalam-nav-spacer",
                            },

                            #[name = "top_nav"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 0,
                                set_valign: gtk::Align::Center,
                            },

                            gtk::Box {
                                set_vexpand: true,
                                add_css_class: "kalam-nav-spacer",
                            },

                            #[name = "bottom_nav"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 0,
                                
                                // Hidden with the Downloads page: with no
                                // `Source` registered the queue is always
                                // empty, so this rail icon only ever opens a
                                // popover saying "No downloads tracked."
                                // The wiring below is kept so re-enabling it
                                // is one word. See `NavItem::ALL`.
                                #[name = "download_indicator"]
                                gtk::MenuButton {
                                    set_icon_name: "emblem-downloads-symbolic",
                                    add_css_class: "kalam-nav-btn",
                                    set_tooltip_text: Some("Active Downloads"),
                                    set_visible: false,
                                    #[name = "download_popover"]
                                    #[wrap(Some)]
                                    set_popover = &gtk::Popover {
                                        set_position: gtk::PositionType::Right,
                                        gtk::Box {
                                            set_orientation: gtk::Orientation::Vertical,
                                            set_margin_all: 12,
                                            set_spacing: 8,
                                            set_size_request: (260, -1),
                                            gtk::Label {
                                                set_label: "Downloads",
                                                add_css_class: "kalam-title-small",
                                                set_halign: gtk::Align::Start,
                                            },
                                            #[name = "download_popover_list"]
                                            gtk::Box {
                                                set_orientation: gtk::Orientation::Vertical,
                                                set_spacing: 6,
                                            }
                                        }
                                    }
                                }
                            },
                        },
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-main",
                        set_hexpand: true,
                        set_vexpand: true,

                        #[name = "content_overlay"]
                        gtk::Overlay {
                            add_overlay = &gtk::Box {
                                set_halign: gtk::Align::Start,
                                set_valign: gtk::Align::Start,
                                set_margin_top: 16,
                                set_margin_start: 16,
                                #[watch]
                                set_visible: model.show_back_chip(),

                                gtk::Button {
                                    add_css_class: "kalam-back-btn",
                                    add_css_class: "kalam-back-float",
                                    set_focus_on_click: false,
                                    set_child: Some(&crate::icons::labelled(
                                        "go-previous-symbolic",
                                        16,
                                        "Back",
                                        6,
                                    )),
                                    connect_clicked => AppMsg::Back,
                                },
                            },

                            #[wrap(Some)]
                            set_child = &gtk::ScrolledWindow {
                                set_hexpand: true,
                                set_vexpand: true,
                                // GTK4 dropped the global gtk-overlay-scrolling setting;
                                // it is per-widget now. Without this the scrollbar can be
                                // a permanent widget that takes layout space and is always
                                // painted, which no CSS can hide.
                                set_overlay_scrolling: true,
                                set_hscrollbar_policy: gtk::PolicyType::Never,
                                #[watch]
                                set_vscrollbar_policy: if model.route.owns_its_surface() {
                                    gtk::PolicyType::Never
                                } else {
                                    gtk::PolicyType::Automatic
                                },

                                #[name = "content_host"]
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    add_css_class: "kalam-content",
                                    set_hexpand: true,
                                    set_vexpand: true,
                                },
                            },
                        },
                    },
                },
            },
        }
    }

    fn init(
        catalog: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // The catalog arrives already open from `main()`, which is also where
        // a failure to open it is reported and exits. Opening it here as well
        // meant running `migrate()` — its full CREATE TABLE batch, eight
        // `PRAGMA table_info` probes and two `COUNT(*)`s over the dictionary
        // tables — for the third time in one startup, and gave first launch
        // two connections racing to build the merged dictionary store.
        // A0 step 3: give books imported before thumbnails existed a thumbnail
        // without re-importing. Off the UI thread so first paint is not delayed;
        // only missing files are generated, so it is cheap after the first pass.
        //
        // Cover-format fix (P8): the cover decoder was upgraded to sniff the actual
        // image format instead of trusting the file extension. Invalidate the skip
        // marker once so comics imported before the fix (which had cover.jpg files
        // that were actually WebP/PNG bytes) get their thumbnails regenerated.
        // After this pass completes the marker is re-set to the book count and the
        // next launch skips as normal.
        let source_manager = crate::sources::global_source_manager();
        let dl_mgr = std::sync::Arc::new(crate::downloads::DownloadManager::new(source_manager.clone(), catalog.clone()));
        let _ = crate::downloads::DOWNLOAD_MANAGER.set(dl_mgr);
        let initial_route = Route::Module(NavItem::Home);
        crate::timing::span("startup_first_page");
        let page = Self::build_page(&catalog, &source_manager, &initial_route, &sender);
        crate::timing::span_end("startup_first_page");

        let float_scrim = gtk::Box::new(gtk::Orientation::Vertical, 0);
        float_scrim.add_css_class("kalam-float-scrim");
        float_scrim.set_halign(gtk::Align::Fill);
        float_scrim.set_valign(gtk::Align::Fill);
        float_scrim.set_hexpand(true);
        float_scrim.set_vexpand(true);
        float_scrim.set_can_target(true);
        float_scrim.set_visible(false);

        let float_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
        float_host.add_css_class("kalam-float-stage");
        float_host.set_halign(gtk::Align::Center);
        float_host.set_valign(gtk::Align::Center);
        float_host.set_margin_top(24);
        float_host.set_margin_bottom(24);
        float_host.set_margin_start(24);
        float_host.set_margin_end(24);
        float_host.set_size_request(720, 420);
        float_host.set_overflow(gtk::Overflow::Visible);
        float_host.set_can_target(true);
        float_host.set_visible(false);

        let cache_token = catalog.page_cache_token();
        let watch_sender = sender.input_sender().clone();

        let bubbles = crate::bubbles::BubbleManager::new(catalog.clone(), sender.clone());
        let watch_folder = Some(crate::watch_folder::WatchFolderService::new(
            catalog.clone(),
            watch_sender,
        ));
        let model = AppModel {
            catalog,
            source_manager,
            route: initial_route,
            history: Vec::new(),
            sidebar_override: None,
            page: Some(page),
            floating: None,
            float_scrim: float_scrim.clone(),
            float_host: float_host.clone(),
            cache: Vec::new(),
            cache_token,
            force_rebuild_next: false,
            bubbles,
            watch_folder,
        };

        let widgets = view_output!();
        widgets.root_overlay.add_overlay(&float_scrim);
        widgets.root_overlay.add_overlay(&float_host);
        widgets.content_overlay.add_overlay(model.bubbles.scrim_widget());
        widgets.content_overlay.add_overlay(model.bubbles.window_widget());
        widgets.content_overlay.add_overlay(model.bubbles.bubble_widget());

        // The rail bell (decision 148): its panel and outside-click
        // catcher float over the whole window beside the rail, so
        // they live on the root overlay. The button joins the rail's
        // bottom stack below, once it exists.
        let notifications = crate::widgets::notification_panel::NotificationPanel::new();
        widgets.root_overlay.add_overlay(&notifications.catcher());
        widgets.root_overlay.add_overlay(&notifications.revealer());
        notifications.attach_escape(&root);
        crate::notify::on_history_changed(std::rc::Rc::new({
            let notifications = notifications.clone();
            move || notifications.refresh()
        }));
        notifications.refresh();

        // Clicking the dimmed area closes the float. The scrim already
        // swallowed those clicks so they could not reach the page behind it;
        // the handler was simply empty, which made the dim look interactive
        // and do nothing. Same behaviour as Esc, reachable with the mouse.
        let scrim_click = gtk::GestureClick::new();
        let s_scrim = sender.clone();
        scrim_click.connect_pressed(move |_, _, _, _| {
            s_scrim.input(AppMsg::CloseBookDialog);
        });
        float_scrim.add_controller(scrim_click);

        // Tab must not walk out of an open float into the page behind it.
        // Permanent, like the key handler below: the trap is inert whenever
        // `float_host` is hidden, so there is nothing to add or remove per
        // float. (`in_app_dialog.rs` attaches its own per dialog instead,
        // because those hosts are created and destroyed with the dialog.)
        root.add_controller(crate::widgets::focus_trap::controller(&float_host));

        let close_float_key = gtk::EventControllerKey::new();
        let s_key = sender.clone();
        let key_root = root.clone();
        close_float_key.connect_key_pressed(move |_, keyval, _, _| {
            use gtk::gdk::Key;
            if !float_host.is_visible() {
                return gtk::glib::Propagation::Proceed;
            }
            // Esc always closes. `q` is a convenience for the read-only
            // floats, but it must never fire while a text box has focus:
            // the tags panel has an entry, and typing "q" in it used to
            // dismiss the panel instead of typing the letter.
            // Spelled out because both `WidgetExt` and `GtkWindowExt` have a
            // `focus`, and a bare call is ambiguous. `gtk::Text` is the inner
            // widget of an Entry and is what actually holds focus.
            let focused = gtk::prelude::GtkWindowExt::focus(&key_root);
            let typing = focused
                .map(|w| w.is::<gtk::Text>() || w.is::<gtk::Entry>() || w.is::<gtk::SearchEntry>())
                .unwrap_or(false);
            let quit_key = keyval == Key::q || keyval == Key::Q;
            let close = keyval == Key::Escape || (!typing && quit_key);
            if close {
                s_key.input(AppMsg::CloseBookDialog);
                return gtk::glib::Propagation::Stop;
            }
            gtk::glib::Propagation::Proceed
        });
        root.add_controller(close_float_key);

        // The rebindable app shortcuts (decision 155). This remains a
        // bubble-phase handler on the window: page and reader handlers
        // get first claim on the keys they own (notably Ctrl+F inside a
        // book), and only an unclaimed press reaches the shell table.
        // The table is the same shared copy Settings edits, so a
        // successful recording changes this handler's next keypress.
        let app_shortcut_key = gtk::EventControllerKey::new();
        let shortcut_bindings = crate::shortcuts::shared(&model.catalog);
        let s_shortcuts = sender.clone();
        let key_root_shortcuts = root.clone();
        let notifications_shortcuts = notifications.clone();
        let float_host_shortcuts = model.float_host.clone();
        app_shortcut_key.connect_key_pressed(move |_, keyval, _, state| {
            use crate::shortcuts::ShortcutAction;

            let (action, binding) = {
                let table = shortcut_bindings.borrow();
                let Some(action) = table.action_for(keyval, state) else {
                    return gtk::glib::Propagation::Proceed;
                };
                (action, table.binding(action))
            };

            // There is no app-wide undo stack yet. Leaving the press
            // unclaimed also preserves the local Ctrl+Z undo that does
            // exist (for example, the Tag dialog's own history).
            if action == ShortcutAction::UndoDelete {
                return gtk::glib::Propagation::Proceed;
            }

            let focused = gtk::prelude::GtkWindowExt::focus(&key_root_shortcuts);
            let typing = focused
                .map(|w| {
                    w.is::<gtk::Editable>()
                        || w.is::<gtk::Text>()
                        || w.is::<gtk::TextView>()
                })
                .unwrap_or(false);
            if typing && !binding.fires_while_typing() {
                return gtk::glib::Propagation::Proceed;
            }

            match action {
                ShortcutAction::Notifications => {
                    notifications_shortcuts.toggle();
                    return gtk::glib::Propagation::Stop;
                }
                ShortcutAction::ClosePanel => {
                    if notifications_shortcuts.is_open() {
                        notifications_shortcuts.close();
                        return gtk::glib::Propagation::Stop;
                    }
                    if float_host_shortcuts.is_visible() {
                        s_shortcuts.input(AppMsg::CloseBookDialog);
                        return gtk::glib::Propagation::Stop;
                    }
                    // Reader and page panels own their close keys for
                    // now; do not swallow a press the shell cannot use.
                    return gtk::glib::Propagation::Proceed;
                }
                _ => {}
            }

            // A shell action replaces the bell panel rather than
            // navigating underneath its outside-click catcher.
            if notifications_shortcuts.is_open() {
                notifications_shortcuts.close();
            }
            s_shortcuts.input(AppMsg::Shortcut(action));
            gtk::glib::Propagation::Stop
        });
        root.add_controller(app_shortcut_key);

        // The rebindable app shortcuts (decision 155). This remains a
        // bubble-phase handler on the window: page and reader handlers
        // get first claim on the keys they own (notably Ctrl+F inside a
        // book), and only an unclaimed press reaches the shell table.
        // The table is the same shared copy Settings edits, so a
        // successful recording changes this handler's next keypress.
        let app_shortcut_key = gtk::EventControllerKey::new();
        let shortcut_bindings = crate::shortcuts::shared(&model.catalog);
        let s_shortcuts = sender.clone();
        let key_root_shortcuts = root.clone();
        let notifications_shortcuts = notifications.clone();
        let float_host_shortcuts = model.float_host.clone();
        app_shortcut_key.connect_key_pressed(move |_, keyval, _, state| {
            use crate::shortcuts::ShortcutAction;

            let (action, binding) = {
                let table = shortcut_bindings.borrow();
                let Some(action) = table.action_for(keyval, state) else {
                    return gtk::glib::Propagation::Proceed;
                };
                (action, table.binding(action))
            };

            // There is no app-wide undo stack yet. Leaving the press
            // unclaimed also preserves the local Ctrl+Z undo that does
            // exist (for example, the Tag dialog's own history).
            if action == ShortcutAction::UndoDelete {
                return gtk::glib::Propagation::Proceed;
            }

            let focused = gtk::prelude::GtkWindowExt::focus(&key_root_shortcuts);
            let typing = focused
                .map(|w| {
                    w.is::<gtk::Editable>()
                        || w.is::<gtk::Text>()
                        || w.is::<gtk::TextView>()
                })
                .unwrap_or(false);
            if typing && !binding.fires_while_typing() {
                return gtk::glib::Propagation::Proceed;
            }

            match action {
                ShortcutAction::Notifications => {
                    notifications_shortcuts.toggle();
                    return gtk::glib::Propagation::Stop;
                }
                ShortcutAction::ClosePanel => {
                    if notifications_shortcuts.is_open() {
                        notifications_shortcuts.close();
                        return gtk::glib::Propagation::Stop;
                    }
                    if float_host_shortcuts.is_visible() {
                        s_shortcuts.input(AppMsg::CloseBookDialog);
                        return gtk::glib::Propagation::Stop;
                    }
                    // Reader and page panels own their close keys for
                    // now; do not swallow a press the shell cannot use.
                    return gtk::glib::Propagation::Proceed;
                }
                _ => {}
            }

            // A shell action replaces the bell panel rather than
            // navigating underneath its outside-click catcher.
            if notifications_shortcuts.is_open() {
                notifications_shortcuts.close();
            }
            s_shortcuts.input(AppMsg::Shortcut(action));
            gtk::glib::Propagation::Stop
        });
        root.add_controller(app_shortcut_key);

        // From here on, any notify::* call lands on screen.
        crate::notify::attach(widgets.toast_host.clone());

        // CI screenshot states for the rail bell: no route reaches the
        // floating panel, and a fresh seeded launch has no history to
        // show. `notifications` seeds ordinary events (Today and
        // Earlier) and opens the actual panel; `notifications-empty`
        // clears any startup housekeeping entry and opens it empty;
        // `notifications-dot` seeds the same events but leaves the
        // panel closed, so the bell carries its unseen dot.
        if let Ok(state @ ("notifications" | "notifications-empty" | "notifications-dot")) =
            std::env::var("KALAM_DIALOG").as_deref()
        {
            let panel = notifications.clone();
            let with_events = state != "notifications-empty";
            let open_panel = state != "notifications-dot";
            gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(900), move || {
                if with_events {
                    crate::notify::seed_screenshot_history();
                } else {
                    crate::notify::clear_history();
                }
                if open_panel && !panel.is_open() {
                    panel.toggle();
                }
            });
        }

        // Any reading session still marked open belongs to a previous process
        // that died before it could close one. Reap them now, before anything
        // reads the statistics — an open row silently breaks the reading
        // streak it was part of. See `close_orphaned_sessions`.
        match model.catalog.close_orphaned_sessions() {
            Ok(0) => {}
            Ok(n) => crate::timing::note("sessions_reaped", n),
            Err(err) => eprintln!("kalam: could not close orphaned reading sessions: {err}"),
        }

        // Extracted-book caches are rebuilt on demand, so anything orphaned or
        // untouched for a fortnight is pure waste on a small disk.
        {
            // `catalog` was moved into the model above; use the model's handle.
            let uuids = model.catalog.all_uuids().unwrap_or_default();
            let freed = crate::paths::prune_reader_cache(&uuids, 14);
            if freed > 1024 * 1024 {
                crate::notify::info(
                    "Cleaned up reader cache",
                    &format!("Freed {}", crate::epub_metadata::human_size(freed)),
                );
            }
        }

        // The brand mark is rasterized in the accent colour (a texture
        // cannot inherit CSS), so it re-rasterizes on every theme install.
        // The listener lives as long as the process — so does the rail.
        let brand_image = brand_logo(crate::theme::current(&model.catalog).accent);
        widgets.brand.append(&brand_image);
        crate::theme::on_theme_applied(std::rc::Rc::new(
            move |theme: &crate::theme::Theme| {
                if let Some(texture) = brand_texture(theme.accent) {
                    brand_image.set_paintable(Some(&texture));
                }
            },
        ));

        let popover_list = widgets.download_popover_list.clone();
        widgets.download_popover.connect_visible_notify(move |popover| {
            if popover.is_visible() {
                while let Some(child) = popover_list.first_child() {
                    popover_list.remove(&child);
                }
                let jobs = crate::downloads::DOWNLOAD_MANAGER.get()
                    .map(|m| m.get_jobs())
                    .unwrap_or_default();
                if jobs.is_empty() {
                    let lbl = gtk::Label::new(Some("No downloads tracked."));
                    lbl.add_css_class("kalam-subtitle-muted");
                    popover_list.append(&lbl);
                } else {
                    for job in jobs.iter().take(6) {
                        let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
                        let title = gtk::Label::new(Some(&job.title));
                        title.set_halign(gtk::Align::Start);
                        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
                        title.add_css_class("kalam-subtitle-bold");
                        row.append(&title);

                        let status_str = match &job.status {
                            crate::downloads::JobStatus::Pending => "Queued".to_string(),
                            crate::downloads::JobStatus::Downloading { chapter_idx, total } => format!("Downloading {chapter_idx}/{total}"),
                            crate::downloads::JobStatus::Packaging => "Packaging EPUB...".to_string(),
                            crate::downloads::JobStatus::Done => "✓ Completed".to_string(),
                            crate::downloads::JobStatus::Cancelled => "Cancelled".to_string(),
                            crate::downloads::JobStatus::Failed(e) => format!("Failed: {e}"),
                        };
                        let status_lbl = gtk::Label::new(Some(&status_str));
                        status_lbl.set_halign(gtk::Align::Start);
                        status_lbl.add_css_class("kalam-badge");
                        row.append(&status_lbl);
                        popover_list.append(&row);
                    }
                }
            }
        });

        widgets.bottom_nav.append(&notifications.button());

        for item in NavItem::ALL {
            let btn = make_nav_button(*item, *item == NavItem::Home);
            let item_copy = *item;
            let s = sender.clone();
            btn.connect_clicked(move |_| s.input(AppMsg::Navigate(item_copy)));
            btn.set_widget_name(&format!("nav-{:?}", item));
            if item.is_bottom() {
                widgets.bottom_nav.append(&btn);
            } else {
                widgets.top_nav.append(&btn);
            }
        }

        if let Some(page) = &model.page {
            widgets.content_host.append(&page.widget());
        }
        sync_content_classes(&widgets.content_host, &model.route, model.show_back_chip());

        // A0 step 1: mark the first window as drawn (cold-start END). Realize is
        // the point GTK has produced the native window; it fires once. This is
        // a no-op unless KALAM_TIMING=1.
        let housekeeping_catalog = model.catalog.clone();
        widgets.main_window.connect_realize(move |window| {
            crate::timing::now("window_shown");
            // The frame probes hook the clock here, at realize, so the
            // very first frame is captured too (7.1 step 4). No-op and
            // costless unless KALAM_TIMING=1.
            crate::frames::install(window);
            // The splash has done its job the moment the real window draws,
            // and the disk-hungry housekeeping may now begin without having
            // raced the home screen's covers for the drive.
            crate::splash::close();
            start_housekeeping(&housekeeping_catalog);
        });

        // `KALAM_ROUTE=<name>` navigates to a page once the window is up.
        //
        // This exists for the CI screenshot job, which had no way to ask for a
        // page and so sent Tab/Tab/Return and hoped. That guesses the focus
        // order, and when the guess was wrong the run still reported success
        // while photographing Home three times -- a visual check that cannot
        // tell "I navigated" from "I did nothing" is worth nothing
        // (pitfalls §19). A named route removes the guess: the harness asks
        // for `library`, and if the app does not go there the screenshot is
        // wrong in a way somebody will see.
        //
        // Deliberately a diagnostic env var rather than a CLI flag or a
        // D-Bus verb, matching KALAM_TIMING / KALAM_NO_CSS / KALAM_NO_PRELOAD:
        // no argument parser, no new public surface to keep stable, and it
        // costs one `var_os` on a path that already reads three others. An
        // unknown name is reported and ignored rather than fatal -- a typo in
        // a CI script should not look like an application crash.
        if let Some(name) = std::env::var_os("KALAM_ROUTE") {
            let name = name.to_string_lossy().to_string();
            match series_route_for_book(&model.catalog, &name)
                .or_else(|| comic_series_route_for_book(&model.catalog, &name))
                .or_else(|| author_route_for_book(&model.catalog, &name))
                .or_else(|| route_by_name(&name))
            {
                Some(route) => {
                    eprintln!("kalam: KALAM_ROUTE={name} — navigating");
                    sender.input(AppMsg::Push(route));
                }
                None => {
                    eprintln!("kalam: KALAM_ROUTE={name} — unknown route, ignoring");
                    eprintln!(
                        "  known: {}",
                        known_route_names()
                            .into_iter()
                            .chain(ROUTE_ID_FORMS)
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }
            }
        }

        // Roadmap 1.12: `AppModel::init` is the last thing `main()` can reach,
        // and everything after it is GTK realizing the window — creating the
        // native surface and compiling shaders. Marking the boundary here
        // splits the tail of a cold start in two, which the first run of these
        // spans could not: 5.8 s sat inside `app.run` with no idea which half
        // of it was widget building and which was GTK.
        crate::timing::now("init_done");

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
            AppMsg::Navigate(item) => {
                let route = match item {
                    NavItem::Shelves => Route::ShelvesGrid,
                    NavItem::Authors => Route::AuthorsIndex,
                    other => Route::Module(other),
                };
                self.close_floating();
                self.swap_page(&widgets.content_host, route, false, &sender);
            }
            AppMsg::Push(route) => {
                self.close_floating();
                self.swap_page(&widgets.content_host, route, true, &sender);
            }
            AppMsg::ShowInLibrary { query } => {
                self.close_floating();
                self.swap_page(
                    &widgets.content_host,
                    Route::LibrarySection(LibrarySection::AllBooks),
                    true,
                    &sender,
                );
                if let Some(PageSlot::AllBooks(ctrl)) = &self.page {
                    let _ = ctrl.sender().send(AllBooksMsg::SetQuery(query));
                }
            }
            AppMsg::Back => {
                if let Some(prev) = self.history.pop() {
                    self.sidebar_override = match &prev {
                        Route::BookPage { .. } => self.sidebar_override,
                        _ => None,
                    };

                    // Reader close buttons send AppMsg::Back rather than a
                    // swap_page navigation, so the swap_page rule that forces
                    // a fresh build when leaving a reader never runs here.
                    // Without it the landing page comes straight from the
                    // cache with a reading position that predates the whole
                    // session (2026-10-05 field run #2: page_cache_hit right
                    // after reader close, no forced miss, ever). Same rule,
                    // same route list as swap_page.
                    self.force_rebuild_next = matches!(
                        self.route,
                        Route::Reader { .. }
                            | Route::PdfReader { .. }
                            | Route::ComicsReader { .. }
                            | Route::EditorWorkspace { .. }
                    );

                    self.detach_current(&widgets.content_host);

                    self.route = prev;
                    let page = self.take_or_build(&sender);
                    widgets.content_host.append(&page.widget());
                    self.page = Some(page);
                    // A cached picker must re-read which books were
                    // edited (its rows carry the edited state).
                    if let Some(PageSlot::EditorPicker(ctrl)) = &self.page {
                        let _ = ctrl.sender().send(EditorPickerMsg::Refresh);
                    }
                    sync_content_classes(&widgets.content_host, &self.route, self.show_back_chip());
                }
            }
            AppMsg::Shortcut(action) => {
                use crate::shortcuts::ShortcutAction;

                match action {
                    ShortcutAction::GoHome => {
                        self.close_floating();
                        self.swap_page(
                            &widgets.content_host,
                            Route::Module(NavItem::Home),
                            false,
                            &sender,
                        );
                    }
                    ShortcutAction::GoLibrary => {
                        self.close_floating();
                        self.swap_page(
                            &widgets.content_host,
                            Route::Module(NavItem::Library),
                            false,
                            &sender,
                        );
                    }
                    ShortcutAction::GoEditor => {
                        self.close_floating();
                        self.swap_page(
                            &widgets.content_host,
                            Route::Module(NavItem::Editor),
                            false,
                            &sender,
                        );
                    }
                    ShortcutAction::OpenSettings => {
                        self.close_floating();
                        self.swap_page(
                            &widgets.content_host,
                            Route::Module(NavItem::Settings),
                            false,
                            &sender,
                        );
                    }
                    ShortcutAction::Back => {
                        if self.floating.is_some() {
                            sender.input(AppMsg::CloseBookDialog);
                        } else {
                            sender.input(AppMsg::Back);
                        }
                    }
                    ShortcutAction::SearchLibrary => {
                        // In a book, Ctrl+F remains Find in book; the
                        // reader's handlers normally claim it before
                        // this shell dispatch can run.
                        if !self.route.is_reader() {
                            self.close_floating();
                            if self.route
                                != Route::LibrarySection(LibrarySection::AllBooks)
                            {
                                self.swap_page(
                                    &widgets.content_host,
                                    Route::LibrarySection(LibrarySection::AllBooks),
                                    true,
                                    &sender,
                                );
                            }
                            if let Some(PageSlot::AllBooks(ctrl)) = &self.page {
                                let _ = ctrl.sender().send(AllBooksMsg::FocusSearch);
                            }
                        }
                    }
                    ShortcutAction::ImportFiles => {
                        open_shortcut_import_files(root, &self.catalog, &sender);
                    }
                    ShortcutAction::ImportFolder => {
                        open_shortcut_import_folder(root, &self.catalog, &sender);
                    }
                    ShortcutAction::Fullscreen => {
                        if root.is_fullscreen() {
                            root.unfullscreen();
                        } else {
                            root.fullscreen();
                        }
                    }
                    ShortcutAction::ShowShortcuts => {
                        // The cheat-sheet overlay is not drawn yet; the
                        // editable pane is the same registry and is the
                        // useful interim surface for this binding.
                        self.close_floating();
                        if self.route != Route::Module(NavItem::Settings) {
                            self.swap_page(
                                &widgets.content_host,
                                Route::Module(NavItem::Settings),
                                false,
                                &sender,
                            );
                        }
                        if let Some(PageSlot::Settings(ctrl)) = &self.page {
                            let _ = ctrl
                                .sender()
                                .send(SettingsMsg::SelectTab(SettingsTab::Shortcuts));
                        }
                    }
                    ShortcutAction::Quit => root.close(),
                    // Notifications and ClosePanel are handled in the
                    // key handler, where their widgets live; UndoDelete
                    // is stored ahead of a global undo stack.
                    ShortcutAction::Notifications
                    | ShortcutAction::UndoDelete
                    | ShortcutAction::ClosePanel => {}
                }
            }
            AppMsg::ShortcutImportFinished(tally) => {
                if let Some(PageSlot::AllBooks(ctrl)) = &self.page {
                    // All Books already knows how to reload and report
                    // an import; hand it the tally rather than rebuild
                    // the page and lose its search state.
                    let _ = ctrl.sender().send(AllBooksMsg::ImportFinished(tally));
                } else {
                    if tally.imported > 0 {
                        crate::notify::success(
                            "Import finished",
                            &crate::pages::all_books::import_summary(&tally),
                        );
                    }
                    let refresh_listing = matches!(
                        self.route,
                        Route::Module(
                            NavItem::Home
                                | NavItem::Library
                                | NavItem::Shelves
                                | NavItem::Comics,
                        ) | Route::ShelvesGrid
                            | Route::LibrarySection(_)
                            | Route::ComicSeries { .. }
                    );
                    if refresh_listing {
                        self.refresh_if_stale(&widgets.content_host, &sender);
                    }
                }
            }
            AppMsg::OpenBookDialog { book_id } => {
                self.open_floating(book_id, &sender);
            }
            AppMsg::OpenShelvesFloat {
                book_id,
                from_book_float,
            } => self.open_shelves_floating(book_id, from_book_float, &sender),
            AppMsg::FloatOpenFull { book_id } => {
                self.close_floating();
                self.swap_page(
                    &widgets.content_host,
                    Route::BookPage { book_id },
                    true,
                    &sender,
                );
            }
            AppMsg::CloseBookDialog => {
                // A shelves panel opened from the book float hands control
                // back to that float — not straight down to the page.
                let return_to_book = match self.floating {
                    Some(Floating::Shelves {
                        return_to: Some(book_id),
                    }) => Some(book_id),
                    _ => None,
                };
                self.close_floating();
                if let Some(book_id) = return_to_book {
                    self.open_floating(book_id, &sender);
                }
                // The float can delete a book, so the page underneath may now
                // be showing something that no longer exists. Deferred to the
                // next main-loop turn: rebuilding here would dispose widgets
                // while GTK is still unwinding the float's close signal, which
                // is what produced the gtk_widget_is_ancestor criticals.
                let s = sender.clone();
                gtk::glib::idle_add_local_once(move || {
                    s.input(AppMsg::RefreshCurrentPage);
                });
            }
            AppMsg::RefreshCurrentPage => {
                self.refresh_if_stale(&widgets.content_host, &sender);
            }
            AppMsg::ReloadWatchFolder => {
                if let Some(wf) = self.watch_folder.as_mut() {
                    wf.reload();
                }
            }
            AppMsg::OpenReader { book_id } => {
                self.close_floating();
                let route = if let Ok(Some(book)) = self.catalog.get_book(book_id) {
                    if matches!(book.format, crate::models::BookFormat::Cbz | crate::models::BookFormat::Cbr) {
                        Route::ComicsReader { book_id }
                    } else if matches!(book.format, crate::models::BookFormat::Pdf) {
                        Route::PdfReader { book_id }
                    } else {
                        Route::Reader { book_id }
                    }
                } else {
                    Route::Reader { book_id }
                };
                self.swap_page(
                    &widgets.content_host,
                    route,
                    true,
                    &sender,
                );
            }
            AppMsg::OpenReaderAt { book_id, chapter } => {
                if let Some(ch) = chapter {
                    let _ = self.catalog.set_reading_progress(book_id, ch, 0.0, 0);
                }
                sender.input(AppMsg::OpenReader { book_id });
            }
            AppMsg::OpenBubble { book_id } => {
                self.close_floating();
                self.bubbles.open_bubble(book_id);
            }
            AppMsg::MinimizeToBubble { book_id } => {
                self.bubbles.minimize_book(book_id);
                if self.route.is_reader() {
                    sender.input(AppMsg::Back);
                }
            }
            AppMsg::BubbleCloseCurrent => {
                self.bubbles.close_window();
            }
            AppMsg::BubbleSelect { book_id } => {
                self.bubbles.select_bubble(book_id);
            }
            AppMsg::BubbleClose { book_id } => {
                self.bubbles.close_bubble(book_id);
            }
            AppMsg::BubbleSetSplit { book_id } => {
                self.bubbles.set_split_book(book_id);
            }
            AppMsg::BubbleToggleSplit => {
                self.bubbles.toggle_split();
            }
            AppMsg::BubbleCloseSplit => {
                self.bubbles.close_split();
            }
            AppMsg::BubbleMinimizeWindow => {
                self.bubbles.minimize_window();
            }
            AppMsg::BubbleCloseWindow => {
                self.bubbles.close_window();
            }
            AppMsg::BubbleSetActivePane(pane) => {
                self.bubbles.set_active_pane(pane);
            }
        }

        let active = self.sidebar_item();
        update_nav_styles(&widgets.top_nav, active);
        update_nav_styles(&widgets.bottom_nav, active);
        self.update_view(widgets, sender);
    }
}

/// The file half of the shell's Import EPUB shortcut. The native
/// chooser itself is undesigned and stays native; importing reuses
/// All Books' worker so duplicates, failures and task reporting do
/// not acquire a second implementation at the window level.
fn open_shortcut_import_files(
    root: &gtk::Window,
    catalog: &Arc<Catalog>,
    sender: &ComponentSender<AppModel>,
) {
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
    filter.add_mime_type("application/epub+zip");
    filter.add_mime_type("application/x-cbz");
    filter.add_mime_type("application/x-cbr");
    filter.add_mime_type("application/pdf");
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    dialog.set_filters(Some(&filters));

    let catalog = catalog.clone();
    let sender = sender.clone();
    dialog.open_multiple(
        Some(root),
        gtk::gio::Cancellable::NONE,
        move |result| {
            if let Ok(files) = result {
                let mut paths = Vec::new();
                for i in 0..files.n_items() {
                    if let Some(file) =
                        files.item(i).and_then(|obj| obj.downcast::<gtk::gio::File>().ok())
                    {
                        if let Some(path) = file.path() {
                            paths.push(path);
                        }
                    }
                }
                if !paths.is_empty() {
                    start_shortcut_import(catalog, paths, sender);
                }
            }
        },
    );
}

/// The folder half of the Import shortcut pair. Scanning the chosen
/// folder is disk work, so it runs as a task; the import starts only
/// when the supported-file list lands back on the main thread.
fn open_shortcut_import_folder(
    root: &gtk::Window,
    catalog: &Arc<Catalog>,
    sender: &ComponentSender<AppModel>,
) {
    let dialog = gtk::FileDialog::builder()
        .title("Import books from a folder")
        .modal(true)
        .build();

    let catalog = catalog.clone();
    let sender = sender.clone();
    dialog.select_folder(
        Some(root),
        gtk::gio::Cancellable::NONE,
        move |result| {
            if let Ok(folder) = result {
                if let Some(path) = folder.path() {
                    let scan_catalog = catalog.clone();
                    let scan_sender = sender.clone();
                    crate::tasks::spawn(
                        "Scanning folder for books",
                        move |_| collect_shortcut_import_files(path),
                        |_| {},
                        move |paths| {
                            if paths.is_empty() {
                                crate::notify::info(
                                    "No books found in that folder",
                                    "Choose a folder containing EPUB, PDF, CBZ or CBR files.",
                                );
                            } else {
                                start_shortcut_import(scan_catalog, paths, scan_sender);
                            }
                        },
                    );
                }
            }
        },
    );
}

fn start_shortcut_import(
    catalog: Arc<Catalog>,
    paths: Vec<std::path::PathBuf>,
    sender: ComponentSender<AppModel>,
) {
    crate::pages::all_books::spawn_import(
        catalog,
        paths,
        |_, _, _| {},
        move |tally| {
            sender.input(AppMsg::ShortcutImportFinished(tally));
        },
    );
}

fn collect_shortcut_import_files(root: std::path::PathBuf) -> Vec<std::path::PathBuf> {
    let mut pending = vec![root];
    let mut files = Vec::new();

    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(file_type) = entry.file_type() else {
                continue;
            };
            if file_type.is_dir() {
                pending.push(path);
            } else if file_type.is_file() {
                let extension = path
                    .extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| ext.to_ascii_lowercase())
                    .unwrap_or_default();
                if matches!(extension.as_str(), "epub" | "pdf" | "cbz" | "cbr") {
                    files.push(path);
                }
            }
        }
    }

    files.sort();
    files
}

/// The sidebar wordmark: the Kalam logo (calligraphic K, one SVG path,
/// materialized from `docs/design/logo-svg.md`).
///
/// Embedded with `include_str!` rather than read from disk so the binary
/// stays self-contained — there is no install step that would place an asset
/// directory next to it. The path fills with `currentColor`; a texture has no
/// CSS colour to inherit, so the current accent is substituted before
/// rasterizing. So the colour does not freeze as a startup snapshot, `init`
/// registers a [`crate::theme::on_theme_applied`] listener that re-rasterizes
/// the mark on every theme install. (A symbolic theme icon would recolour
/// for free, but `icons::init` is deferred to idle for its theme-rescan
/// cost, and the rail is on screen before that runs.)
fn brand_logo(accent: &str) -> gtk::Image {
    // design-system §5: the loop is a hairline, so check it at 38px. The
    // 68px rail is drawn around this size.
    const LOGO_PX: i32 = 38;

    let image = gtk::Image::new();
    if let Some(texture) = brand_texture(accent) {
        image.set_paintable(Some(&texture));
    }
    // gtk::Image, not gtk::Picture. A Picture's natural size is the texture's
    // own size, and both set_size_request and CSS min-width are *floors*, so a
    // 128px texture drew at 128px and stretched the whole rail. Image with
    // set_pixel_size is the one widget that treats the number as exact.
    image.set_pixel_size(LOGO_PX);
    image.set_halign(gtk::Align::Center);
    image.set_valign(gtk::Align::Center);
    image.add_css_class("kalam-brand-logo");
    image
}

/// Rasterize the logo in one accent colour. `None` only when the embedded
/// asset is corrupt; the app should still start, just without its mark.
/// Shared with the Settings About pane, which shows the same mark larger.
pub(crate) fn brand_texture(accent: &str) -> Option<gtk::gdk::Texture> {
    const LOGO_SVG: &str = include_str!("../assets/logo.svg");

    let svg = LOGO_SVG.replace("currentColor", accent);
    let bytes = gtk::glib::Bytes::from_owned(svg.into_bytes());
    gtk::gdk::Texture::from_bytes(&bytes).ok()
}

/// Map a rail entry to its global shortcut action, if any.
fn nav_shortcut(item: NavItem) -> Option<ShortcutAction> {
    match item {
        NavItem::Home => Some(ShortcutAction::GoHome),
        NavItem::Library => Some(ShortcutAction::GoLibrary),
        NavItem::Editor => Some(ShortcutAction::GoEditor),
        NavItem::Settings => Some(ShortcutAction::OpenSettings),
        _ => None,
    }
}

fn make_nav_button(item: NavItem, active: bool) -> gtk::Button {
    // Icon only. The rail is too narrow for a readable caption, and the
    // tooltip already carries the page name.
    let icon = crate::icons::symbolic_with_classes(item.icon(), 22, &["kalam-nav-icon"]);
    icon.set_halign(gtk::Align::Center);
    icon.set_valign(gtk::Align::Center);

    let btn = gtk::Button::new();
    btn.set_child(Some(&icon));
    btn.add_css_class("kalam-nav-btn");
    btn.set_halign(gtk::Align::Center);
    btn.set_hexpand(false);
    if active {
        btn.add_css_class("active");
    }
    let mut tip = item.label().to_string();
    if let Some(action) = nav_shortcut(item) {
        if let Some(label) = action.shortcut_label() {
            tip.push_str(&format!(" ({label})"));
        }
    }
    btn.set_tooltip_text(Some(&tip));
    btn.set_focus_on_click(false);
    btn
}

fn sync_content_classes(content_host: &gtk::Box, route: &Route, show_back_chip: bool) {
    if route.owns_its_surface() || matches!(route, Route::Module(NavItem::Settings)) {
        content_host.add_css_class("kalam-content-flush");
    } else {
        content_host.remove_css_class("kalam-content-flush");
    }

    if route.is_reader() {
        content_host.add_css_class("kalam-content-reader");
    } else {
        content_host.remove_css_class("kalam-content-reader");
    }

    if show_back_chip {
        content_host.add_css_class("kalam-content-with-back");
    } else {
        content_host.remove_css_class("kalam-content-with-back");
    }
}

/// Housekeeping that is useful but must not race the first paint for the
/// disk: the thumbnail backfill and the bundled-dictionary check. Started
/// only once the main window is on screen (`connect_realize`), so the home
/// screen's covers win the drive on a slow disk.
fn start_housekeeping(catalog: &std::sync::Arc<crate::db::Catalog>) {
    let backfill_catalog = catalog.clone();
    crate::thumbs::invalidate_backfill_marker(&backfill_catalog);
    crate::tasks::spawn_internal(
        "Rebuilding cover thumbnails",
        move |reporter| crate::thumbs::backfill_missing(&backfill_catalog, &reporter),
        // Only interesting under KALAM_TIMING=1: a first launch over a big
        // library can spend a while here, and without a progress line
        // there was no way to tell a slow backfill from a stalled one.
        |update| {
            if update.done == update.total || update.done % 50 == 0 {
                crate::timing::note("thumbs_backfilled", update.done);
            }
        },
        |generated| {
            if generated > 0 {
                crate::timing::note("thumbs_backfill_done", generated);
            }
        },
    );
    // A0 step 4 leftover, finished here: the first run decompresses and
    // imports ~6.8 MB of gzipped TSV packs, and it used to do that on this
    // thread — before the window existed. A new user waited on it with
    // nothing on screen to explain why.
    //
    // Off the seam now. Nothing on screen depends on it: the dictionary is
    // read when the user looks a word up in the reader, which cannot
    // happen before the window is even drawn. Later runs still early-out
    // on a pref, so this is a no-op after the first launch.
    //
    // Not merged into the thumbnail task above, deliberately: two
    // independent jobs sharing one worker means the slower one delays the
    // other for no reason, and a failure in one would be reported as a
    // failure of both.
    let dict_catalog = catalog.clone();
    crate::tasks::spawn_internal(
        "Checking dictionary packs",
        move |_reporter| {
            // No cancel check inside: the unit of work is a whole pack,
            // and abandoning one half-imported would leave the pref unset
            // and the rows partly written. It is bounded work that ends on
            // its own, so letting it finish is simpler and safer than
            // making it interruptible.
            crate::timing::span("startup_dicts");
            let result = crate::dict::install_bundled_dictionaries(&dict_catalog);
            crate::timing::span_end("startup_dicts");
            // Send back a String rather than the error: anyhow::Error is
            // not Send-safe to move across the seam here, and the message
            // is all the UI needs.
            result.err().map(|err| err.to_string())
        },
        |_update| {},
        |failed: Option<String>| {
            // Back on the main thread, so the toast is raised where the
            // notification system can actually display it (pitfalls §4e).
            if let Some(message) = failed {
                crate::notify::error("Could not install the bundled dictionaries", &message);
            }
        },
    );

    // Owner decision, 2026-09-29: book folders are named for their contents
    // ("Author - Title <short-id>") instead of bare uuids. This pass is the
    // one-time upgrade for an existing library and, after that, the keeper
    // that renames a folder when its book's metadata was edited elsewhere
    // or by an older version. Separate task, same reasoning as the two
    // above: renames are instant, so it never holds up the covers.
    let folder_catalog = catalog.clone();
    crate::tasks::spawn_internal(
        "Naming book folders",
        move |reporter| crate::folders::align_book_folders(&folder_catalog, &reporter),
        // Only interesting under KALAM_TIMING=1, and only when something
        // actually moved: a steady-state run renames nothing.
        |_update| {},
        |renamed: usize| {
            if renamed > 0 {
                crate::timing::note("folders_renamed", renamed);
            }
        },
    );

    // Item 2.22: a comic series is one folder on disk — `library/<Series>/`
    // holding the number-named chapter files and `covers/`. This pass is
    // the one-time upgrade for a library whose comics still live one
    // chapter per folder, and after that a no-op that costs one string
    // comparison per chapter. It runs after the book-folder pass above,
    // which skips comics precisely so the two never race.
    let comic_catalog = catalog.clone();
    crate::tasks::spawn_internal(
        "Organizing comic series folders",
        move |reporter| crate::comic_folders::migrate_comic_library(&comic_catalog, &reporter),
        |_update| {},
        |moved: usize| {
            if moved > 0 {
                crate::timing::note("comic_chapters_moved", moved);
            }
        },
    );

}

fn update_nav_styles(container: &gtk::Box, active: NavItem) {
    let mut child = container.first_child();
    while let Some(widget) = child {
        if let Ok(btn) = widget.clone().downcast::<gtk::Button>() {
            let name = btn.widget_name();
            if name == format!("nav-{:?}", active) {
                btn.add_css_class("active");
            } else {
                btn.remove_css_class("active");
            }
        }
        child = widget.next_sibling();
    }
}

/// The series route a `KALAM_ROUTE=series-book-<id>` value refers to.
///
/// A series route carries a name and an author, neither of which belongs
/// in a shell variable; the anchor book's id does, and the book supplies
/// both — exactly the identity its own Series link would open. Kept out
/// of `route_by_name` because that map is deliberately pure (its tests
/// resolve names without a catalog).
fn series_route_for_book(catalog: &Catalog, name: &str) -> Option<Route> {
    let normalized = name.trim().to_ascii_lowercase();
    let book_id = normalized
        .strip_prefix("series-book-")?
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)?;
    let book = catalog.get_book(book_id).ok().flatten()?;
    let series = book.series?.trim().to_string();
    if series.is_empty() {
        return None;
    }
    let first_author = book
        .authors
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    Some(Route::SeriesPage {
        series,
        first_author,
    })
}

/// The comic-series route a `KALAM_ROUTE=comic-series-book-<id>` value
/// refers to: the drawer for the series that chapter belongs to. The
/// relational series wins; an unmigrated chapter falls back to the
/// same series name its Comics grouping would infer.
fn comic_series_route_for_book(catalog: &Catalog, name: &str) -> Option<Route> {
    let normalized = name.trim().to_ascii_lowercase();
    let book_id = normalized
        .strip_prefix("comic-series-book-")?
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)?;
    if let Some((series, _chapter)) = catalog.get_comic_series_for_book(book_id).ok().flatten() {
        return Some(Route::ComicSeries {
            series_name: series.title,
        });
    }
    let book = catalog.get_book(book_id).ok().flatten()?;
    let series_name = book
        .series
        .as_deref()
        .map(str::trim)
        .filter(|series| !series.is_empty())
        .map(|series| crate::comics::sanitize_comic_series(series).0)
        .filter(|series| !series.trim().is_empty())
        .or_else(|| crate::comics::parse_comic_title(&book.title).series)?;
    Some(Route::ComicSeries { series_name })
}

/// The author route a `KALAM_ROUTE=author-book-<id>` value refers to: the
/// anchor book's first author, exactly what its own author link opens.
/// Same catalog-dependent reasoning as [`series_route_for_book`].
fn author_route_for_book(catalog: &Catalog, name: &str) -> Option<Route> {
    let normalized = name.trim().to_ascii_lowercase();
    let book_id = normalized
        .strip_prefix("author-book-")?
        .parse::<i64>()
        .ok()
        .filter(|id| *id > 0)?;
    let book = catalog.get_book(book_id).ok().flatten()?;
    let author = book
        .authors
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .to_string();
    if author.is_empty() {
        return None;
    }
    Some(Route::AuthorPage { author })
}

/// The route a `KALAM_ROUTE=<name>` value refers to, if any.
///
/// Names are the lowercase page names a person would say out loud, not the
/// `Debug` spelling of the enum: the CI harness and anyone debugging types
/// these by hand, and coupling them to Rust identifiers would silently break
/// every caller the next time a variant is renamed.
///
/// Most pages need no arguments; the two that do take the id after the name
/// (`book-<id>` and `read-<id>`), because a bare `book` cannot say which book
/// and guessing one would photograph some other page while looking right.
fn route_by_name(name: &str) -> Option<Route> {
    let name = name.trim().to_ascii_lowercase();
    // The PDF reader is its own route (MuPDF raster pages, unlike
    // the EPUB engine behind `read-`), so the screenshot loop
    // spells it out — and the same for comics below.
    if let Some(id) = name.strip_prefix("pdf-read-") {
        let book_id = id.parse::<i64>().ok().filter(|id| *id > 0)?;
        return Some(Route::PdfReader { book_id });
    }
    if let Some(id) = name.strip_prefix("comic-read-") {
        let book_id = id.parse::<i64>().ok().filter(|id| *id > 0)?;
        return Some(Route::ComicsReader { book_id });
    }
    if let Some(id) = name.strip_prefix("edit-book-") {
        let book_id = id.parse::<i64>().ok().filter(|id| *id > 0)?;
        return Some(Route::EditorWorkspace { book_id });
    }
    for (prefix, reader) in [("book-", false), ("read-", true)] {
        if let Some(id) = name.strip_prefix(prefix) {
            // A positive integer or nothing. `book-`, `book-abc` and `book--1`
            // are typos, and a route that guessed would hide them.
            let book_id = id.parse::<i64>().ok().filter(|id| *id > 0)?;
            return Some(if reader {
                Route::Reader { book_id }
            } else {
                Route::BookPage { book_id }
            });
        }
    }
    let route = match name.as_str() {
        "home" => Route::Module(NavItem::Home),
        "library" => Route::Module(NavItem::Library),
        "authors" => Route::AuthorsIndex,
        "editor" => Route::Module(NavItem::Editor),
        "downloads" => Route::Module(NavItem::Downloads),
        "comics" => Route::Module(NavItem::Comics),
        "browse" => Route::Module(NavItem::RemoteBrowse),
        "fanfiction" => Route::Module(NavItem::Fanfiction),
        "settings" => Route::Module(NavItem::Settings),
        "components" => Route::ComponentPreview,
        "shelves" => Route::ShelvesGrid,
        "all-books" | "allbooks" => Route::LibrarySection(LibrarySection::AllBooks),
        "reading-list" => Route::LibrarySection(LibrarySection::ReadingList),
        "history" => Route::LibrarySection(LibrarySection::History),
        "saved-quotes" => Route::LibrarySection(LibrarySection::SavedQuotes),
        "saved-words" => Route::LibrarySection(LibrarySection::SavedWords),
        "lookup-history" => Route::LibrarySection(LibrarySection::LookupHistory),
        "tags" => Route::LibrarySection(LibrarySection::Tags),
        "analytics" => Route::LibrarySection(LibrarySection::Analytics),
        "tasks" => Route::LibrarySection(LibrarySection::TaskManager),
        _ => return None,
    };
    Some(route)
}

/// Every name `route_by_name` accepts, for the error message. Kept next to it
/// so the two cannot drift; a test asserts each one resolves.
fn known_route_names() -> Vec<&'static str> {
    vec![
        "home",
        "library",
        "authors",
        "editor",
        "downloads",
        "comics",
        "browse",
        "fanfiction",
        "settings",
        "components",
        "shelves",
        "all-books",
        "reading-list",
        "history",
        "saved-quotes",
        "saved-words",
        "lookup-history",
        "tags",
        "analytics",
        "tasks",
    ]
}

/// The names that take an id, for the error message only. They are not in
/// `known_route_names` because that list is asserted to resolve as it stands.
const ROUTE_ID_FORMS: [&str; 8] = [
    "book-<id>",
    "edit-book-<id>",
    "read-<id>",
    "pdf-read-<id>",
    "comic-read-<id>",
    "series-book-<id>",
    "comic-series-book-<id>",
    "author-book-<id>",
];

/// A short static label per route, for the `route_open:` timing span and the
/// stall watchdog's activity stack (`timing::activity`). Every route gets
/// one because every construction is measured — including the readers,
/// which are the fast part; the watchdog observing them is a feature, not
/// noise (a freeze there is a finding too).
///
/// Exhaustive on purpose: a new route variant without a label is a compile
/// error, not a silently unattributed stall.
fn route_label(route: &Route) -> &'static str {
    match route {
        Route::Module(NavItem::Home) => "route_open:home",
        Route::Module(NavItem::Library) => "route_open:library",
        Route::Module(NavItem::Authors) => "route_open:authors",
        Route::Module(NavItem::Shelves) => "route_open:shelves",
        Route::Module(NavItem::Downloads) => "route_open:downloads",
        Route::Module(NavItem::Comics) => "route_open:comics",
        Route::Module(NavItem::Editor) => "route_open:editor",
        Route::EditorWorkspace { .. } => "route_open:editor_workspace",
        Route::Module(NavItem::RemoteBrowse) => "route_open:remote_browse",
        Route::Module(NavItem::Fanfiction) => "route_open:fanfiction",
        Route::Module(NavItem::Settings) => "route_open:settings",
        Route::ComponentPreview => "route_open:component_preview",
        Route::LibrarySection(LibrarySection::AllBooks) => "route_open:all_books",
        Route::LibrarySection(LibrarySection::ReadingList) => "route_open:reading_list",
        Route::LibrarySection(LibrarySection::History) => "route_open:history",
        Route::LibrarySection(LibrarySection::SavedQuotes) => "route_open:saved_quotes",
        Route::LibrarySection(LibrarySection::SavedWords) => "route_open:saved_words",
        Route::LibrarySection(LibrarySection::LookupHistory) => "route_open:lookup_history",
        Route::LibrarySection(LibrarySection::Tags) => "route_open:tags",
        Route::LibrarySection(LibrarySection::Analytics) => "route_open:analytics",
        Route::LibrarySection(LibrarySection::TaskManager) => "route_open:task_manager",
        Route::LibrarySection(LibrarySection::Review) => "route_open:review",
        Route::ShelvesGrid => "route_open:shelves_grid",
        Route::ShelfDetail { .. } => "route_open:shelf_detail",
        Route::TagBooks { .. } => "route_open:tag_books",
        Route::AuthorsIndex => "route_open:authors",
        Route::AuthorPage { .. } => "route_open:author",
        Route::BookPage { .. } => "route_open:book",
        Route::SeriesPage { .. } => "route_open:series",
        Route::RemoteDetail { .. } => "route_open:remote_detail",
        Route::RemoteReader { .. } => "route_open:remote_reader",
        Route::Reader { .. } => "route_open:reader",
        Route::ComicsReader { .. } => "route_open:comics_reader",
        Route::PdfReader { .. } => "route_open:pdf_reader",
        Route::RemoteSearch { .. } => "route_open:remote_search",
        Route::ComicSeries { .. } => "route_open:comic_series",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_advertised_route_name_resolves() {
        // The error message lists these, so a name that appears there and then
        // does not work is worse than no help at all.
        for name in known_route_names() {
            assert!(
                route_by_name(name).is_some(),
                "{name} is advertised but does not resolve"
            );
        }
    }

    #[test]
    fn every_route_has_a_distinct_label() {
        // The stall watchdog blames construction blocks by these labels; two
        // routes sharing one would mis-attribute a stall to the wrong screen,
        // and the id forms are included because they are real routes too.
        let mut labels: Vec<&'static str> = known_route_names()
            .into_iter()
            .filter_map(|name| route_by_name(name).as_ref().map(route_label))
            .collect();
        for name in ["book-1", "read-1", "pdf-read-1", "comic-read-1", "edit-book-1"] {
            labels.push(route_label(&route_by_name(name).expect("id form resolves")));
        }
        // `series-book-<id>` resolves through the catalog, so its route is
        // built by hand here.
        labels.push(route_label(&Route::SeriesPage {
            series: "Alpha".into(),
            first_author: "An Author".into(),
        }));
        let unique: std::collections::HashSet<&'static str> =
            labels.iter().copied().collect();
        assert_eq!(
            unique.len(),
            labels.len(),
            "route labels must be distinct, got {labels:?}"
        );
    }

    #[test]
    fn route_names_map_to_the_right_pages() {
        // Spot-check across all three Route shapes, so a copy-paste slip
        // between adjacent match arms is caught.
        assert_eq!(route_by_name("home"), Some(Route::Module(NavItem::Home)));
        assert_eq!(
            route_by_name("settings"),
            Some(Route::Module(NavItem::Settings))
        );
        assert_eq!(route_by_name("components"), Some(Route::ComponentPreview));
        assert_eq!(route_by_name("shelves"), Some(Route::ShelvesGrid));
        assert_eq!(
            route_by_name("all-books"),
            Some(Route::LibrarySection(LibrarySection::AllBooks))
        );
        assert_eq!(
            route_by_name("analytics"),
            Some(Route::LibrarySection(LibrarySection::Analytics))
        );
    }

    #[test]
    fn shelves_live_on_the_rail() {
        // Per §56, Shelves is a rail entry alongside Home/Library/Comics/Editor;
        // My Library remains the entry point for shelf detail pages.
        assert!(NavItem::ALL.contains(&NavItem::Shelves));
        assert_eq!(Route::ShelvesGrid.sidebar_item(), NavItem::Shelves);
        assert_eq!(
            Route::ShelfDetail { shelf_id: 1 }.sidebar_item(),
            NavItem::Shelves
        );
        assert_eq!(route_by_name("shelves"), Some(Route::ShelvesGrid));
    }

    #[test]
    fn route_names_tolerate_case_and_stray_whitespace() {
        // These arrive from a shell variable, where a trailing space or a
        // capital is a typo, not a different page.
        let expected = Some(Route::LibrarySection(LibrarySection::AllBooks));
        assert_eq!(route_by_name("All-Books"), expected);
        assert_eq!(route_by_name("  all-books  "), expected);
        assert_eq!(route_by_name("ALLBOOKS"), expected);
    }

    #[test]
    fn route_names_with_a_book_id_resolve_and_bad_ids_do_not() {
        // The screenshot job opens a real book with `read-1`. Book 1 always
        // exists: the CI seeder inserts with AUTOINCREMENT from 1.
        assert_eq!(route_by_name("read-1"), Some(Route::Reader { book_id: 1 }));
        assert_eq!(route_by_name("book-1"), Some(Route::BookPage { book_id: 1 }));
        assert_eq!(
            route_by_name("  READ-12 "),
            Some(Route::Reader { book_id: 12 })
        );
        assert_eq!(
            route_by_name("pdf-read-7"),
            Some(Route::PdfReader { book_id: 7 })
        );
        assert_eq!(
            route_by_name("comic-read-7"),
            Some(Route::ComicsReader { book_id: 7 })
        );
        for name in ["book", "reader", "book-", "read-", "book-abc", "book-1x", "read--1", "pdf-read-", "pdf-read-x", "pdf-7", "comic-read-", "comic-read-x", "comic-7"] {
            assert_eq!(route_by_name(name), None, "{name:?} must not resolve");
        }
    }

    #[test]
    fn unknown_route_names_are_rejected_not_guessed() {
        // An unrecognised name must be `None` so the caller can say so.
        // Silently falling back to Home is exactly the failure this whole
        // mechanism exists to remove: the screenshot job would photograph
        // Home and call it the library.
        for name in ["", "libary", "book", "reader", "nav-Home", "Module(Home)"] {
            assert_eq!(route_by_name(name), None, "{name:?} must not resolve");
        }
    }
}
