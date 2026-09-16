//! Root Relm4 application: slim sidebar + routed main content.

use crate::db::Catalog;
use crate::models::{LibrarySection, NavItem, Route};
use crate::pages::{downloads::DownloadsModel, 
    all_books::{AllBooksModel, AllBooksOut},
    analytics::AnalyticsModel,
    author::{AuthorPageModel, AuthorPageOut},
    book::{BookPageModel, BookPageOut},
    book_float::{BookFloatModel, BookFloatOut},
    comics::{ComicsModel, ComicsOut},
    browse::{BrowseModel, BrowseOut},
    remote_detail::{RemoteDetailModel, RemoteDetailOut, RemoteDetailInit},
    comics_reader::{ComicsReaderModel, ComicsReaderOut},
    history::{HistoryModel, HistoryOut},
    home::{HomeOut, HomePageModel},
    library::{LibraryOut, LibraryPageModel},
    lookup_history::LookupHistoryModel,
    placeholder::PlaceholderPageModel,
    reader::{ReaderModel, ReaderOut},
    reading_list::{ReadingListModel, ReadingListOut},
    saved_quotes::{SavedQuotesModel, SavedQuotesOut},
    saved_words::{SavedWordsModel, SavedWordsOut},
    series_float::{SeriesFloatModel, SeriesFloatOut},
    settings::SettingsPageModel,
    shelf_detail::{ShelfDetailModel, ShelfDetailOut},
    shelves_grid::{ShelvesGridModel, ShelvesOut},
    tags::{TagBooksModel, TagBooksOut, TagsModel, TagsOut},
};
use gtk::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

#[derive(Debug)]
pub enum AppMsg {
    Navigate(NavItem),
    Push(Route),
    Back,
    OpenBookDialog {
        book_id: i64,
    },
    /// Close float and open full book page in the main column.
    FloatOpenFull {
        book_id: i64,
    },
    CloseBookDialog,
    /// Open the series float from the book page's Series link.
    OpenSeriesFloat {
        series: String,
        first_author: String,
    },
    /// Open the highlights & quotes panel (in-app float) from the book page.
    OpenAnnotationsFloat {
        book_id: i64,
    },
    /// Open the shelves checklist panel (in-app float).
    OpenShelvesFloat {
        book_id: i64,
        /// True when opened from the book float — closing returns there.
        from_book_float: bool,
    },
    /// Open the tags panel (in-app float) from the book page.
    OpenTagsFloat {
        book_id: i64,
    },
    /// Rebuild the page on screen if the catalog changed under it.
    RefreshCurrentPage,
    /// Open immersive reader for book_id.
    OpenReader {
        book_id: i64,
    },
}

enum PageSlot {
    Home(Controller<HomePageModel>),
    Library(Controller<LibraryPageModel>),
    AllBooks(Controller<AllBooksModel>),
    SavedQuotes(Controller<SavedQuotesModel>),
    SavedWords(Controller<SavedWordsModel>),
    LookupHistory(Controller<LookupHistoryModel>),
    Shelves(Controller<ShelvesGridModel>),
    ShelfDetail(Controller<ShelfDetailModel>),
    ReadingList(Controller<ReadingListModel>),
    History(Controller<HistoryModel>),
    Tags(Controller<TagsModel>),
    TagBooks(Controller<TagBooksModel>),
    Analytics(Controller<AnalyticsModel>),
    Author(Controller<AuthorPageModel>),
    Book(Controller<BookPageModel>),
    Reader(Controller<ReaderModel>),
    Comics(Controller<ComicsModel>),
    ComicsReader(Controller<ComicsReaderModel>),
    RemoteBrowse(Controller<BrowseModel>),
    RemoteDetail(Controller<RemoteDetailModel>),
    Settings(Controller<SettingsPageModel>),
    Placeholder(Controller<PlaceholderPageModel>),
    Downloads(Controller<DownloadsModel>),
}

impl PageSlot {
    fn widget(&self) -> gtk::Widget {
        match self {
            PageSlot::Downloads(c) => c.widget().clone().upcast(),
            PageSlot::Home(c) => c.widget().clone().upcast(),
            PageSlot::Library(c) => c.widget().clone().upcast(),
            PageSlot::AllBooks(c) => c.widget().clone().upcast(),
            PageSlot::SavedQuotes(c) => c.widget().clone().upcast(),
            PageSlot::SavedWords(c) => c.widget().clone().upcast(),
            PageSlot::LookupHistory(c) => c.widget().clone().upcast(),
            PageSlot::Shelves(c) => c.widget().clone().upcast(),
            PageSlot::ShelfDetail(c) => c.widget().clone().upcast(),
            PageSlot::ReadingList(c) => c.widget().clone().upcast(),
            PageSlot::History(c) => c.widget().clone().upcast(),
            PageSlot::Tags(c) => c.widget().clone().upcast(),
            PageSlot::TagBooks(c) => c.widget().clone().upcast(),
            PageSlot::Analytics(c) => c.widget().clone().upcast(),
            PageSlot::Author(c) => c.widget().clone().upcast(),
            PageSlot::Book(c) => c.widget().clone().upcast(),
            PageSlot::Reader(c) => c.widget().clone().upcast(),
            PageSlot::Comics(c) => c.widget().clone().upcast(),
            PageSlot::ComicsReader(c) => c.widget().clone().upcast(),
            PageSlot::RemoteBrowse(c) => c.widget().clone().upcast(),
            PageSlot::RemoteDetail(c) => c.widget().clone().upcast(),
            PageSlot::Settings(c) => c.widget().clone().upcast(),
            PageSlot::Placeholder(c) => c.widget().clone().upcast(),
        }
    }
}

/// Which float is on screen — only one at a time. The controller is held
/// (never read) purely to keep the component alive until the float closes.
#[allow(dead_code)]
enum Floating {
    Book(Controller<BookFloatModel>),
    Series(Controller<SeriesFloatModel>),
    /// Highlights & quotes — a plain widget panel, nothing to keep alive.
    Annotations,
    /// Shelves checklist — a plain widget panel, nothing to keep alive.
    /// `return_to` holds the book id when the panel was opened from the book
    /// float, so closing it hands control back to that float, not the page.
    Shelves {
        return_to: Option<i64>,
    },
    /// Tags panel — a plain widget panel, nothing to keep alive.
    Tags,
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
    /// Catalog write counter at the time each cached page was built. A cached
    /// page is only reused while this matches, so an import, delete or edit
    /// anywhere automatically forces a rebuild — no write path has to
    /// remember to invalidate.
    cache_token: i64,
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
        Route::Module(item) => Some(format!("mod:{}", item.label())),
        // Everything below reflects data the user is actively changing.
        Route::LibrarySection(_)
        | Route::ShelfDetail { .. }
        | Route::TagBooks { .. }
        | Route::AuthorPage { .. }
        | Route::BookPage { .. }
        | Route::Reader { .. }
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
        !self.history.is_empty() && !self.route.is_reader()
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
        self.close_floating();

        let ctrl = BookFloatModel::builder()
            .launch((self.catalog.clone(), book_id))
            .forward(sender.input_sender(), move |out| match out {
                BookFloatOut::Close | BookFloatOut::Deleted { .. } => AppMsg::CloseBookDialog,
                BookFloatOut::OpenReader { book_id } => AppMsg::OpenReader { book_id },
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

    /// The series float, opened from the book page's Series link.
    fn open_series_floating(
        &mut self,
        series: String,
        first_author: String,
        sender: &ComponentSender<Self>,
    ) {
        self.close_floating();

        let ctrl = SeriesFloatModel::builder()
            .launch((self.catalog.clone(), series, first_author))
            .forward(sender.input_sender(), |out| match out {
                SeriesFloatOut::OpenBook { book_id } => AppMsg::FloatOpenFull { book_id },
            });

        let float = ctrl.widget().clone();
        float.set_size_request(560, 560);
        float.set_hexpand(false);
        float.set_vexpand(false);
        float.set_halign(gtk::Align::Center);
        float.set_valign(gtk::Align::Center);
        self.float_host.append(&float);
        self.float_scrim.set_visible(true);
        self.float_host.set_visible(true);
        float.grab_focus();

        self.floating = Some(Floating::Series(ctrl));
    }

    /// The highlights & quotes panel, opened from the book page. Like the
    /// other floats it lives in the in-app float layer — never a separate
    /// window — so the compositor can't move it to another workspace.
    fn open_annotations_floating(&mut self, book_id: i64, sender: &ComponentSender<Self>) {
        self.close_floating();

        let s = sender.clone();
        let panel =
            crate::pages::book::build_annotations_panel(self.catalog.clone(), book_id, move || {
                s.input(AppMsg::CloseBookDialog)
            });
        panel.set_size_request(460, 480);
        panel.set_hexpand(false);
        panel.set_vexpand(false);
        panel.set_halign(gtk::Align::Center);
        panel.set_valign(gtk::Align::Center);
        self.float_host.append(&panel);
        self.float_scrim.set_visible(true);
        self.float_host.set_visible(true);
        panel.grab_focus();

        self.floating = Some(Floating::Annotations);
    }

    fn open_shelves_floating(
        &mut self,
        book_id: i64,
        from_book_float: bool,
        sender: &ComponentSender<Self>,
    ) {
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

    fn open_tags_floating(&mut self, book_id: i64, sender: &ComponentSender<Self>) {
        self.close_floating();

        let s = sender.clone();
        let panel =
            crate::pages::book::build_tags_panel(self.catalog.clone(), book_id, move || {
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

        self.floating = Some(Floating::Tags);
    }

    fn build_page(
        catalog: &Arc<Catalog>,
        source_manager: &Arc<crate::sources::SourceManager>,
        route: &Route,
        sender: &ComponentSender<Self>,
    ) -> PageSlot {
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
                    },
                );
                PageSlot::Home(ctrl)
            }
            Route::Module(NavItem::Library) => {
                let ctrl = LibraryPageModel::builder().launch(catalog.clone()).forward(
                    sender.input_sender(),
                    |out| match out {
                        LibraryOut::Section(sec) => AppMsg::Push(Route::LibrarySection(sec)),
                        LibraryOut::Book { book_id } => AppMsg::Push(Route::BookPage { book_id }),
                        LibraryOut::BookDialog { book_id } => AppMsg::OpenBookDialog { book_id },
                        LibraryOut::Read { book_id } => AppMsg::OpenReader { book_id },
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
                    });
                PageSlot::TagBooks(ctrl)
            }
            Route::AuthorPage { author } => {
                let ctrl = AuthorPageModel::builder()
                    .launch((catalog.clone(), author.clone()))
                    .forward(sender.input_sender(), |out| match out {
                        AuthorPageOut::OpenBook { book_id } => {
                            AppMsg::Push(Route::BookPage { book_id })
                        }
                        AuthorPageOut::OpenBookDialog { book_id } => {
                            AppMsg::OpenBookDialog { book_id }
                        }
                    });
                PageSlot::Author(ctrl)
            }
            Route::BookPage { book_id } => {
                let id = *book_id;
                let ctrl = BookPageModel::builder()
                    .launch((catalog.clone(), id))
                    .forward(sender.input_sender(), move |out| match out {
                        BookPageOut::OpenReader => AppMsg::OpenReader { book_id: id },
                        BookPageOut::OpenAuthor { name } => {
                            AppMsg::Push(Route::AuthorPage { author: name })
                        }
                        BookPageOut::OpenBook { book_id } => AppMsg::FloatOpenFull { book_id },
                        BookPageOut::OpenSeries {
                            series,
                            first_author,
                        } => AppMsg::OpenSeriesFloat {
                            series,
                            first_author,
                        },
                        BookPageOut::ViewHighlights => AppMsg::OpenAnnotationsFloat { book_id: id },
                        BookPageOut::ShowShelves => AppMsg::OpenShelvesFloat {
                            book_id: id,
                            from_book_float: false,
                        },
                        BookPageOut::ShowTags => AppMsg::OpenTagsFloat { book_id: id },
                        BookPageOut::Deleted { .. } => AppMsg::Back,
                    });
                PageSlot::Book(ctrl)
            }
            Route::Reader { book_id } => {
                let id = *book_id;
                let ctrl = ReaderModel::builder()
                    .launch((catalog.clone(), id))
                    .forward(sender.input_sender(), |out| match out {
                        ReaderOut::Close => AppMsg::Back,
                        ReaderOut::OpenAuthor { name } => {
                            AppMsg::Push(Route::AuthorPage { author: name })
                        }
                    });
                PageSlot::Reader(ctrl)
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
                        };
                        let ctrl = ComicsReaderModel::builder()
                            .launch(init)
                            .forward(sender.input_sender(), |out| match out {
                                ComicsReaderOut::Close => AppMsg::Back,
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
                let book = catalog.get_book(id).unwrap().unwrap();
                let provider = std::sync::Arc::new(
                    crate::pages::comics_reader::providers::LocalProvider::new(book.file_path.clone()).unwrap()
                );
                let init = crate::pages::comics_reader::types::ComicsReaderInit {
                    title: book.title.clone(),
                    provider,
                };
                let ctrl = ComicsReaderModel::builder()
                    .launch(init)
                    .forward(sender.input_sender(), |out| match out {
                        ComicsReaderOut::Close => AppMsg::Back,
                    });
                PageSlot::ComicsReader(ctrl)
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
            Route::BookPage { .. } | Route::Reader { .. } => Some(self.sidebar_item()),
            _ => None,
        };

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
        let token = self.catalog.change_token();
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
        // Any catalog write invalidates every cached page: a stale Home would
        // happily show a book you just deleted.
        let token = self.catalog.change_token();
        if token != self.cache_token {
            self.cache.clear();
            self.cache_token = token;
        }

        if let Some(key) = cache_key(&self.route) {
            if let Some(idx) = self.cache.iter().position(|(k, _)| *k == key) {
                let (_, page) = self.cache.remove(idx);
                crate::timing::note("page_cache_hit", 1);
                return page;
            }
        }
        // Instrumented because the hit rate is in doubt and should be
        // measured before this cache is either tuned or deleted. The token is
        // `total_changes()`, which *every* write bumps — including the
        // reading-progress save on each 1% of scroll — so ten minutes in the
        // reader is expected to evict everything on the way back out. Under
        // `KALAM_TIMING=1` these two counters say whether the cache is
        // earning its keep or is dead weight.
        crate::timing::note("page_cache_miss", 1);
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
                                
                                #[name = "download_indicator"]
                                gtk::MenuButton {
                                    set_icon_name: "emblem-downloads-symbolic",
                                    add_css_class: "kalam-nav-btn",
                                    set_tooltip_text: Some("Active Downloads"),
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
                                set_vscrollbar_policy: if model.route.is_reader() {
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
        let backfill_catalog = catalog.clone();
        crate::thumbs::invalidate_backfill_marker(&backfill_catalog);
        crate::tasks::spawn(
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
        crate::tasks::spawn(
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

        let cache_token = catalog.change_token();
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
        };

        let widgets = view_output!();
        widgets.root_overlay.add_overlay(&float_scrim);
        widgets.root_overlay.add_overlay(&float_host);

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

        // From here on, any notify::* call lands on screen.
        crate::notify::attach(widgets.toast_host.clone());

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
                    &format!("Freed {}", crate::epub_write::human_size(freed)),
                );
            }
        }

        widgets.brand.append(&brand_logo());

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
        widgets
            .main_window
            .connect_realize(|_| crate::timing::now("window_shown"));

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
            match route_by_name(&name) {
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
            AppMsg::Navigate(item) => {
                let route = match item {
                    NavItem::Shelves => Route::ShelvesGrid,
                    other => Route::Module(other),
                };
                self.close_floating();
                self.swap_page(&widgets.content_host, route, false, &sender);
            }
            AppMsg::Push(route) => {
                self.close_floating();
                self.swap_page(&widgets.content_host, route, true, &sender);
            }
            AppMsg::Back => {
                if let Some(prev) = self.history.pop() {
                    self.sidebar_override = match &prev {
                        Route::BookPage { .. } => self.sidebar_override,
                        _ => None,
                    };

                    self.detach_current(&widgets.content_host);

                    self.route = prev;
                    let page = self.take_or_build(&sender);
                    widgets.content_host.append(&page.widget());
                    self.page = Some(page);
                    sync_content_classes(&widgets.content_host, &self.route, self.show_back_chip());
                }
            }
            AppMsg::OpenBookDialog { book_id } => {
                self.open_floating(book_id, &sender);
            }
            AppMsg::OpenSeriesFloat {
                series,
                first_author,
            } => {
                self.open_series_floating(series, first_author, &sender);
            }
            AppMsg::OpenAnnotationsFloat { book_id } => {
                self.open_annotations_floating(book_id, &sender);
            }
            AppMsg::OpenShelvesFloat {
                book_id,
                from_book_float,
            } => self.open_shelves_floating(book_id, from_book_float, &sender),
            AppMsg::OpenTagsFloat { book_id } => self.open_tags_floating(book_id, &sender),
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
            AppMsg::OpenReader { book_id } => {
                self.close_floating();
                let route = if let Ok(Some(book)) = self.catalog.get_book(book_id) {
                    if matches!(book.format, crate::models::BookFormat::Cbz | crate::models::BookFormat::Cbr) {
                        Route::ComicsReader { book_id }
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
        }

        let active = self.sidebar_item();
        update_nav_styles(&widgets.top_nav, active);
        update_nav_styles(&widgets.bottom_nav, active);
        self.update_view(widgets, sender);
    }
}

/// The sidebar wordmark: the Kalam logo.
///
/// Embedded with `include_bytes!` rather than read from disk so the binary
/// stays self-contained — there is no install step that would place an asset
/// directory next to it.
fn brand_logo() -> gtk::Image {
    const LOGO: &[u8] = include_bytes!("../assets/logo.png");
    // Matches the nav glyphs, a shade larger so the mark still leads the rail.
    const LOGO_PX: i32 = 24;

    let bytes = gtk::glib::Bytes::from_static(LOGO);
    let image = match gtk::gdk::Texture::from_bytes(&bytes) {
        Ok(texture) => gtk::Image::from_paintable(Some(&texture)),
        // A corrupt asset should not stop the app from starting.
        Err(_) => gtk::Image::new(),
    };
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

fn make_nav_button(item: NavItem, active: bool) -> gtk::Button {
    // Icon only. The rail is too narrow for a readable caption, and the
    // tooltip already carries the page name.
    let icon = crate::icons::symbolic_with_classes(item.icon(), 18, &["kalam-nav-icon"]);
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
    btn.set_tooltip_text(Some(item.label()));
    btn.set_focus_on_click(false);
    btn
}

fn sync_content_classes(content_host: &gtk::Box, route: &Route, show_back_chip: bool) {
    if route.is_reader() || matches!(route, Route::Module(NavItem::Settings)) {
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
        "downloads" => Route::Module(NavItem::Downloads),
        "comics" => Route::Module(NavItem::Comics),
        "browse" => Route::Module(NavItem::RemoteBrowse),
        "fanfiction" => Route::Module(NavItem::Fanfiction),
        "settings" => Route::Module(NavItem::Settings),
        "shelves" => Route::ShelvesGrid,
        "all-books" | "allbooks" => Route::LibrarySection(LibrarySection::AllBooks),
        "reading-list" => Route::LibrarySection(LibrarySection::ReadingList),
        "history" => Route::LibrarySection(LibrarySection::History),
        "saved-quotes" => Route::LibrarySection(LibrarySection::SavedQuotes),
        "saved-words" => Route::LibrarySection(LibrarySection::SavedWords),
        "lookup-history" => Route::LibrarySection(LibrarySection::LookupHistory),
        "tags" => Route::LibrarySection(LibrarySection::Tags),
        "analytics" => Route::LibrarySection(LibrarySection::Analytics),
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
        "downloads",
        "comics",
        "browse",
        "fanfiction",
        "settings",
        "shelves",
        "all-books",
        "reading-list",
        "history",
        "saved-quotes",
        "saved-words",
        "lookup-history",
        "tags",
        "analytics",
    ]
}

/// The two names that take an id, for the error message only. They are not in
/// `known_route_names` because that list is asserted to resolve as it stands.
const ROUTE_ID_FORMS: [&str; 2] = ["book-<id>", "read-<id>"];

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
    fn route_names_map_to_the_right_pages() {
        // Spot-check across all three Route shapes, so a copy-paste slip
        // between adjacent match arms is caught.
        assert_eq!(route_by_name("home"), Some(Route::Module(NavItem::Home)));
        assert_eq!(
            route_by_name("settings"),
            Some(Route::Module(NavItem::Settings))
        );
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
        for name in ["book", "reader", "book-", "read-", "book-abc", "book-1x", "read--1"] {
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
