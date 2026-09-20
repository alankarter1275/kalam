//! Domain models and navigation routes.

use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NavItem {
    Home,
    Library,
    Shelves,
    Downloads,
    Comics,
    RemoteBrowse,
    Fanfiction,
    Settings,
}

impl NavItem {
    /// What the sidebar actually shows.
    ///
    /// Deliberately missing, and not an oversight:
    ///
    /// * `Downloads` — the queue page works, but nothing ever registers a
    ///   `Source`, so it can only ever read "0 downloads tracked". Online
    ///   sources are Part 2 (`docs/offline-roadmap.md`); until one exists the
    ///   button advertises a feature the app cannot perform.
    /// * `RemoteBrowse` and `Fanfiction` — same reason, removed earlier.
    ///
    /// All three stay in the enum, in `label`/`icon`, in `placeholder_copy`
    /// and in the `KALAM_ROUTE` map, so putting one back is a one-line change
    /// here and nothing else.
    pub const ALL: &'static [NavItem] = &[
        NavItem::Home,
        NavItem::Library,
        NavItem::Shelves,
        NavItem::Comics,
        NavItem::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NavItem::Home => "Home",
            NavItem::Library => "Library",
            NavItem::Shelves => "Shelves",
            NavItem::Downloads => "Downloads",
            NavItem::Comics => "Comics",
            NavItem::RemoteBrowse => "Browse",
            NavItem::Fanfiction => "Fanfic",
            NavItem::Settings => "Settings",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            NavItem::Home => "go-home-symbolic",
            NavItem::Library => "folder-documents-symbolic",
            NavItem::Shelves => "view-grid-symbolic",
            NavItem::Downloads => "folder-download-symbolic",
            NavItem::Comics => "image-x-generic-symbolic",
            NavItem::RemoteBrowse => "network-workgroup-symbolic",
            NavItem::Fanfiction => "document-edit-symbolic",
            NavItem::Settings => "emblem-system-symbolic",
        }
    }

    pub fn is_bottom(self) -> bool {
        matches!(self, NavItem::Settings)
    }
}

/// Where we are inside the main content stack.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Route {
    Module(NavItem),
    LibrarySection(LibrarySection),
    ShelvesGrid,
    ShelfDetail {
        shelf_id: i64,
    },
    /// Books carrying one tag (P4 tag browse).
    TagBooks {
        tag: String,
    },
    AuthorPage {
        author: String,
    },
    BookPage {
        book_id: i64,
    },
    RemoteDetail {
        source_id: String,
        remote_id: String,
    },
    RemoteReader {
        source_id: String,
        chapter_id: String,
        title: String,
    },
    /// Immersive EPUB reader.
    Reader {
        book_id: i64,
    },
    /// Immersive Comics reader (P8).
    ComicsReader {
        book_id: i64,
    },
    /// Immersive PDF reader with smart crop and text reflow.
    PdfReader {
        book_id: i64,
    },
    RemoteSearch {
        source_id: String,
        query: String,
    },
}

impl Route {
    pub fn sidebar_item(&self) -> NavItem {
        match self {
            Route::Module(item) => *item,
            Route::LibrarySection(_) => NavItem::Library,
            Route::ShelvesGrid | Route::ShelfDetail { .. } => NavItem::Shelves,
            Route::TagBooks { .. } | Route::AuthorPage { .. } => NavItem::Library,
            Route::BookPage { .. } | Route::Reader { .. } | Route::PdfReader { .. } | Route::ComicsReader { .. } | Route::RemoteReader { .. } => NavItem::Library,
            Route::RemoteDetail { .. } => NavItem::RemoteBrowse,
            Route::RemoteSearch { source_id, .. } => {
                if source_id == "royalroad" {
                    NavItem::Fanfiction
                } else {
                    NavItem::RemoteBrowse
                }
            }
        }
    }

    pub fn is_reader(&self) -> bool {
        matches!(self, Route::Reader { .. } | Route::PdfReader { .. } | Route::ComicsReader { .. } | Route::RemoteReader { .. })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LibrarySection {
    AllBooks,
    ReadingList,
    History,
    SavedQuotes,
    SavedWords,
    /// Phase 10: the dictionary lookup log.
    LookupHistory,
    Tags,
    Analytics,
    /// Roadmap 1.14: what is running in the background, and how to stop it.
    TaskManager,
    /// Roadmap #5: spaced-repetition review of saved words.
    Review,
}

impl LibrarySection {
    /// Kept for the section pickers that will return with the definitive
    /// layout; the dashboard now routes via content sections instead of a
    /// generated tile grid.
    #[allow(dead_code)] // the full section list renders once the library gains section tabs
    pub const ALL: &'static [LibrarySection] = &[
        LibrarySection::AllBooks,
        LibrarySection::ReadingList,
        LibrarySection::History,
        LibrarySection::SavedQuotes,
        LibrarySection::SavedWords,
        LibrarySection::LookupHistory,
        LibrarySection::Tags,
        LibrarySection::Analytics,
        LibrarySection::TaskManager,
        LibrarySection::Review,
    ];

    #[allow(dead_code)] // section icons for those tabs
    pub fn icon(self) -> &'static str {
        match self {
            LibrarySection::AllBooks => "folder-documents-symbolic",
            LibrarySection::ReadingList => "view-list-symbolic",
            LibrarySection::History => "document-open-recent-symbolic",
            LibrarySection::SavedQuotes => "insert-text-symbolic",
            LibrarySection::SavedWords => "accessories-dictionary-symbolic",
            LibrarySection::LookupHistory => "edit-find-symbolic",
            LibrarySection::Tags => "tag-symbolic",
            LibrarySection::Analytics => "view-bar-symbolic",
            LibrarySection::TaskManager => "system-run-symbolic",
            LibrarySection::Review => "media-playlist-repeat-symbolic",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookFormat {
    Epub,
    Pdf,
    Cbz,
    Cbr,
    Other,
}

impl BookFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            BookFormat::Epub => "EPUB",
            BookFormat::Pdf => "PDF",
            BookFormat::Cbz => "CBZ",
            BookFormat::Cbr => "CBR",
            BookFormat::Other => "OTHER",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_uppercase().as_str() {
            "EPUB" => BookFormat::Epub,
            "PDF" => BookFormat::Pdf,
            "CBZ" => BookFormat::Cbz,
            "CBR" => BookFormat::Cbr,
            _ => BookFormat::Other,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Book {
    pub id: i64,
    pub uuid: String,
    pub title: String,
    /// Comma-separated for P1 simplicity.
    pub authors: String,
    pub series: Option<String>,
    pub description: String,
    pub format: BookFormat,
    pub file_name: String,
    pub file_hash: String,
    pub cover_name: Option<String>,
    pub added_at: String,
    pub progress: u8,
    /// 0..=10 half-stars; 0 means unrated.
    pub rating: u8,
    pub publisher: String,
    /// Free text as printed on the book, e.g. "February 15, 2012".
    pub published: String,
    /// Position within `series`; 0 means unset. Fractional for novellas.
    pub series_index: f32,
    pub tags: Vec<String>,
    pub cover_path: Option<PathBuf>,
    pub file_path: PathBuf,
}

impl Book {
    /// `3.5` for 7 half-stars — `None` when unrated.
    pub fn rating_stars(&self) -> Option<f32> {
        if self.rating == 0 {
            None
        } else {
            Some(self.rating as f32 / 2.0)
        }
    }

    /// "Lord of the Rings #3", or just the series when no index is set.
    pub fn series_display(&self) -> Option<String> {
        let series = self.series.as_deref()?.trim();
        if series.is_empty() {
            return None;
        }
        if self.series_index <= 0.0 {
            return Some(series.to_string());
        }
        // Whole numbers should not render as "3.0".
        if (self.series_index.fract()).abs() < f32::EPSILON {
            Some(format!("{series} #{}", self.series_index as i64))
        } else {
            Some(format!("{series} #{}", self.series_index))
        }
    }

    pub fn authors_display(&self) -> &str {
        if self.authors.trim().is_empty() {
            "Unknown"
        } else {
            self.authors.as_str()
        }
    }
}
