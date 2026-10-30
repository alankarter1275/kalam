//! Domain models and navigation routes.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NavItem {
    Home,
    Library,
    Authors,
    Shelves,
    Downloads,
    Comics,
    Editor,
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
    ///   sources are a future track in `ROADMAP.md`; until one exists the
    ///   button advertises a feature the app cannot perform.
    /// * `RemoteBrowse` and `Fanfiction` — same reason, removed earlier.
    ///
    /// All three stay in the enum, in `label`/`icon`, in `placeholder_copy`
    /// and in the `KALAM_ROUTE` map, so putting one back is a one-line change
    /// here and nothing else.
    ///
    pub const ALL: &'static [NavItem] = &[
        NavItem::Home,
        NavItem::Library,
        NavItem::Authors,
        NavItem::Shelves,
        NavItem::Comics,
        NavItem::Editor,
        NavItem::Settings,
    ];

    pub fn label(self) -> &'static str {
        match self {
            NavItem::Home => "Home",
            NavItem::Library => "Library",
            NavItem::Authors => "Authors",
            NavItem::Shelves => "Shelves",
            NavItem::Downloads => "Downloads",
            NavItem::Comics => "Comics",
            NavItem::Editor => "Editor",
            NavItem::RemoteBrowse => "Browse",
            NavItem::Fanfiction => "Fanfic",
            NavItem::Settings => "Settings",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            NavItem::Home => "go-home-symbolic",
            NavItem::Library => "folder-documents-symbolic",
            NavItem::Authors => "avatar-default-symbolic",
            NavItem::Shelves => "view-grid-symbolic",
            NavItem::Downloads => "folder-download-symbolic",
            NavItem::Comics => "image-x-generic-symbolic",
            NavItem::Editor => "accessories-text-editor-symbolic",
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
    /// Every author in the library (decision 117).
    AuthorsIndex,
    AuthorPage {
        author: String,
    },
    BookPage {
        book_id: i64,
    },
    /// The deep EPUB editor workspace for one book (decisions 118–143).
    EditorWorkspace {
        book_id: i64,
    },
    /// The full book-series page. The first author is part of the route
    /// because it is part of the cached listing's identity: two authors can
    /// use the same series name without sharing a page or a fetch.
    SeriesPage {
        series: String,
        first_author: String,
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
    ComicSeries {
        series_name: String,
    },
    /// Hidden redesign component preview, reached only through
    /// `KALAM_ROUTE=components`.
    ComponentPreview,
}

impl Route {
    pub fn sidebar_item(&self) -> NavItem {
        match self {
            Route::Module(item) => *item,
            Route::ComponentPreview => NavItem::Settings,
            Route::ComicSeries { .. } => NavItem::Comics,
            Route::LibrarySection(_) => NavItem::Library,
            Route::ShelvesGrid | Route::ShelfDetail { .. } => NavItem::Shelves,
            Route::TagBooks { .. } => NavItem::Library,
            Route::AuthorsIndex | Route::AuthorPage { .. } => NavItem::Authors,
            Route::EditorWorkspace { .. } => NavItem::Editor,
            Route::BookPage { .. } | Route::SeriesPage { .. } | Route::Reader { .. } | Route::PdfReader { .. } | Route::ComicsReader { .. } | Route::RemoteReader { .. } => NavItem::Library,
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
        matches!(
            self,
            Route::Reader { .. }
                | Route::PdfReader { .. }
                | Route::ComicsReader { .. }
                | Route::RemoteReader { .. }
        )
    }

    /// Pages that fill the content area edge to edge and own their
    /// scrolling — the readers and the editor workspace. The shell's
    /// page scroller and back chip step aside for them (the editor
    /// has its own Back, because leaving it can mean discarding
    /// unsaved changes).
    pub fn owns_its_surface(&self) -> bool {
        self.is_reader() || matches!(self, Route::EditorWorkspace { .. })
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
        let (clean_series, ch_opt) = crate::comics::sanitize_comic_series(series);
        let display_series = if !clean_series.is_empty() {
            clean_series
        } else {
            series.to_string()
        };
        let idx = if self.series_index > 0.0 {
            self.series_index
        } else {
            ch_opt.unwrap_or(0.0)
        };
        if idx <= 0.0 {
            return Some(display_series);
        }
        // Whole numbers should not render as "3.0".
        if (idx.fract()).abs() < f32::EPSILON {
            Some(format!("{display_series} #{}", idx as i64))
        } else {
            Some(format!("{display_series} #{}", idx))
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

/// One role-bearing credit on a comic series. Manga sources speak in
/// Story/Art, western sources in Writer/Penciller/Inker and friends;
/// the page stays source-neutral by showing whatever roles it was
/// given (decision 46).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ComicCredit {
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub name: String,
}

/// The hand-editable fields of a comic series, by the names its
/// manual-field locks use. A field the reader changed in Edit details
/// is recorded here and a metadata refresh must never overwrite it
/// (decisions 46/48).
pub const COMIC_SERIES_FIELD_TITLE: &str = "title";
pub const COMIC_SERIES_FIELD_ORIGINAL_TITLE: &str = "original_title";
pub const COMIC_SERIES_FIELD_CREDITS: &str = "credits";
pub const COMIC_SERIES_FIELD_DESCRIPTION: &str = "description";
pub const COMIC_SERIES_FIELD_STATUS: &str = "status";
pub const COMIC_SERIES_FIELD_PUBLISHER: &str = "publisher";
pub const COMIC_SERIES_FIELD_SERIALIZATION: &str = "serialization";
pub const COMIC_SERIES_FIELD_PUBLISHED: &str = "published";
pub const COMIC_SERIES_FIELD_END_DATE: &str = "end_date";
pub const COMIC_SERIES_FIELD_MEDIA_TYPE: &str = "media_type";
pub const COMIC_SERIES_FIELD_GENRES: &str = "genres";
pub const COMIC_SERIES_FIELD_KNOWN_VOLUMES: &str = "known_volumes";
pub const COMIC_SERIES_FIELD_KNOWN_CHAPTERS: &str = "known_chapters";
pub const COMIC_SERIES_FIELD_COVER: &str = "cover";

/// A first-class Comic / Manga Series in the library catalog.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ComicSeries {
    pub id: i64,
    pub title: String,
    pub sort_title: String,
    /// The flat author line, kept beside `credits` for lists and
    /// search: unique credit names when roles are known, else the
    /// imported/typed summary.
    pub author: String,
    pub description: String,
    pub cover_book_id: Option<i64>,
    /// The cover to show: a fetched series cover when one was kept,
    /// else the cover chapter's own cover.
    pub cover_path: Option<PathBuf>,
    /// The fetched series cover alone, when metadata supplied one.
    pub metadata_cover_path: Option<PathBuf>,
    pub status: String,
    pub total_chapters: usize,
    pub completed_chapters: usize,
    pub unread_chapters: usize,
    pub created_at: String,
    pub updated_at: String,
    pub last_read_at: Option<String>,

    // Fetched/edited series metadata (decision 46). Empty and `None`
    // are “not known”, never a claim.
    pub original_title: String,
    pub credits: Vec<ComicCredit>,
    pub publisher: String,
    pub serialization: String,
    pub published: String,
    pub end_date: String,
    pub media_type: String,
    pub genres: Vec<String>,
    pub known_volumes: Option<i64>,
    /// Chapters (manga) or issues (western) the source knows about,
    /// as distinct from `total_chapters`, which counts this library.
    pub known_chapters: Option<i64>,
    /// Provenance of the last accepted fetch: source key
    /// (`myanimelist`, `comicvine`), its record id and page, and when
    /// Kalam took it. Empty until a fetch is accepted.
    pub metadata_source: String,
    pub metadata_source_id: String,
    pub metadata_source_url: String,
    pub metadata_fetched_at: String,
    /// `COMIC_SERIES_FIELD_*` names the reader edited by hand.
    pub manual_fields: Vec<String>,
}

impl ComicSeries {
    pub fn is_manual_field(&self, field: &str) -> bool {
        self.manual_fields.iter().any(|locked| locked == field)
    }

    /// “Story: A · Art: B” when roles are known, else the flat author.
    pub fn credits_summary(&self) -> String {
        if self.credits.is_empty() {
            return self.author.clone();
        }
        let mut parts: Vec<String> = Vec::new();
        for credit in &self.credits {
            let name = credit.name.trim();
            if name.is_empty() {
                continue;
            }
            let role = credit.role.trim();
            let part = if role.is_empty() {
                name.to_string()
            } else {
                format!("{role}: {name}")
            };
            if !parts.iter().any(|existing| existing == &part) {
                parts.push(part);
            }
        }
        if parts.is_empty() {
            self.author.clone()
        } else {
            parts.join(" · ")
        }
    }
}

/// An individual chapter within a `ComicSeries`, joined with its underlying `Book` row.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ComicChapter {
    pub id: i64,
    pub series_id: i64,
    pub book_id: i64,
    pub chapter_number: f32,
    pub volume_number: Option<f32>,
    pub chapter_title: String,
    pub created_at: String,
    pub book: Book,
}

impl ComicChapter {
    #[allow(dead_code)]
    pub fn display_number(&self) -> String {
        if (self.chapter_number.fract()).abs() < f32::EPSILON {
            format!("Ch. {}", self.chapter_number as i64)
        } else {
            format!("Ch. {}", self.chapter_number)
        }
    }
}
