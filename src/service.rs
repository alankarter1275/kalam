//! A0 step 2 — `LibraryService`: the seam between pages and the database.
//!
//! # What problem this solves
//!
//! Pages used to call `Catalog` directly, several times each, and swallow the
//! errors individually:
//!
//! ```ignore
//! let stats = catalog.library_stats().unwrap_or_default();
//! let books = catalog.recent_books(12).unwrap_or_default();
//! let tbr   = catalog.list_reading_list().unwrap_or_default();
//! ```
//!
//! Three problems with that. Every page decides for itself what a failed query
//! means (26 `unwrap_or_default()`s and 16 `.ok().flatten()`s across `pages/`),
//! so a broken database renders as an empty library rather than an error.
//! There is no single place to put caching. And moving queries off the UI
//! thread would mean editing every page.
//!
//! The service answers a page's whole data question in **one call returning one
//! owned snapshot**:
//!
//! ```ignore
//! let snap = service.home();   // one call, everything Home draws
//! ```
//!
//! # Why snapshots, not just wrapped getters
//!
//! This shape is the point of the step, not decoration. A snapshot is a plain
//! owned `Send` struct, so the same call can later run on a worker thread and
//! be handed back to the UI — without touching the page. Wrapping each getter
//! one-for-one would have moved the calls but kept N round trips per page, and
//! made each of them a separate future to sequence. `snapshots_are_send()`
//! below asserts the `Send` property at compile time so a future edit cannot
//! quietly break the promise by putting an `Rc` in a snapshot.
//!
//! # Error policy, in one place
//!
//! A read failure degrades to the empty value **and records the reason** in
//! `errors`, which pages surface. Previously the reason was dropped on the
//! floor. The service deliberately does not call `notify` itself: it must stay
//! callable from a worker thread, and `notify` is UI-thread-only (thread-local
//! toast host). Reporting is the caller's job.
//!
//! # Status
//!
//! Home, Analytics and Tags are converted. The remaining pages still hold an
//! `Arc<Catalog>`; both styles coexist on purpose, because `LibraryService`
//! borrows the same `Arc` rather than replacing it. Converting a page is:
//! add a snapshot method here, swap the page's field, delete its
//! `unwrap_or_default()`s. See the roadmap's A0 step 2 entry.

use crate::db::{
    Annotation, AuthorProfile, Catalog, Dictionary, DictLookup, EventKind, LibrarySession,
    LibraryStats, QuoteRef, ReadingBookmark, ReadingEvent, ReadingListEntry, SavedWord,
    SessionRow, Shelf, SortKey,
};
use crate::watch_folder::WatchFolderRule;
use crate::models::Book;
use std::sync::Arc;

/// Reads the catalog on behalf of the UI.
///
/// Cheap to clone (one `Arc` bump) and holds no state of its own, so a page can
/// keep one for its lifetime.
#[derive(Clone)]
pub struct LibraryService {
    catalog: Arc<Catalog>,
}

/// Collected read failures. Empty in the normal case, so the happy path costs
/// no allocation.
pub type Errors = Vec<String>;

/// Everything the Home page draws, fetched together.
#[derive(Debug, Default)]
pub struct HomeSnapshot {
    pub stats: LibraryStats,
    /// Recently added — bounded; Home shows a dozen covers, not the library.
    pub recent: Vec<Book>,
    /// "Continue reading", already resolved through Home's fallback chain
    /// (recently opened → in-progress → first book) so the page does not have
    /// to know the rule.
    pub continue_reading: Vec<Book>,
    pub reading_list: Vec<ReadingListEntry>,
    pub errors: Errors,
}

/// Everything the Analytics page draws.
#[derive(Debug, Default)]
pub struct AnalyticsSnapshot {
    pub stats: LibraryStats,
    pub goal: i64,
    pub finished_this_year: i64,
    /// Which of the last 7 days had any reading.
    pub week: [bool; 7],
    pub errors: Errors,
}

/// Everything the reader needs about a book when it opens.
///
/// Four reads that were each swallowed at startup, so opening a book against
/// a broken database silently dropped your highlights, bookmarks and saved
/// words — the reader looked fine and simply showed none of your work.
#[derive(Debug, Default)]
pub struct ReaderSnapshot {
    pub book: Option<Book>,
    pub annotations: Vec<Annotation>,
    pub bookmarks: Vec<ReadingBookmark>,
    pub saved_words: Vec<SavedWord>,
    pub errors: Errors,
}

/// Per-book reading stats.
///
/// Six reads that were each swallowed, so a database problem drew a page
/// saying you had never read the book. The redesigned book page removed
/// its stats/history card (decision 79) and now reads only the chapter
/// index and the started/finished dates; the remaining fields stay
/// because this is still the service's per-book stats read — the perf
/// harness and the service tests consume them.
#[derive(Debug, Default)]
pub struct BookStatsSnapshot {
    #[allow(dead_code)] // stats card removed by decision 79; perf/tests read it
    pub total_seconds: i64,
    #[allow(dead_code)] // as above
    pub session_count: i64,
    pub chapter_index: usize,
    #[allow(dead_code)] // as above
    pub seconds_by_day: Vec<(String, i64)>,
    #[allow(dead_code)] // as above
    pub recent_sessions: Vec<SessionRow>,
    pub finished_at: Option<String>,
    pub first_opened: Option<String>,
    pub errors: Errors,
}

/// One shelf and the books on it.
///
/// `shelf: None` means the shelf is gone, which the page already has wording
/// for; a failed read adds an `errors` row instead.
#[derive(Debug, Default)]
pub struct ShelfDetailSnapshot {
    pub shelf: Option<Shelf>,
    pub books: Vec<Book>,
    pub errors: Errors,
}

/// One book's float panel: the book plus the two flags the panel shows.
///
/// `book: None` means the book is genuinely gone, not that the read failed —
/// a failed read pushes a row into `errors` instead, so the panel can stop
/// showing "book not found" for what is really a database problem.
#[derive(Debug, Default)]
pub struct BookDetailSnapshot {
    pub book: Option<Book>,
    pub in_reading_list: bool,
    pub finished: bool,
    pub errors: Errors,
}

/// Everything the book page draws, in one call (roadmap 7.1 step 2a).
///
/// The page builds its skeleton on the UI thread and a worker fills it
/// when this snapshot arrives, so opening a book never touches the
/// database, the filesystem or the EPUB parser on the UI thread. It
/// composes the two existing per-card snapshots and adds the reads the
/// redesigned page needs: reading position, annotations, bookmarks,
/// this book's saved words, manual-shelf membership for the actions
/// submenu, and the next volume in the series. The file-size stat stays
/// with the page's worker — it is a filesystem question, not a catalog
/// one. (The old author profile / other-books reads left with the
/// author card the redesign retired; the author page has its own.)
#[derive(Debug, Default)]
pub struct BookPageSnapshot {
    pub detail: BookDetailSnapshot,
    /// Current chapter + scroll fraction; `None` until the book is opened.
    pub progress: Option<(usize, f64)>,
    pub stats: BookStatsSnapshot,
    pub annotations: Vec<Annotation>,
    /// Why the highlights list is empty when that read failed — reported
    /// with the highlights wording, not a generic database error.
    pub annotations_error: Option<String>,
    /// Reader bookmarks in this book — the Contents timeline's
    /// per-chapter counts and the Annotations section's bookmark rows.
    pub bookmarks: Vec<ReadingBookmark>,
    /// Words saved while reading this book (book scope only; a word
    /// saved in another volume is not this page's to show).
    pub saved_words: Vec<SavedWord>,
    /// Manual shelves, for the actions menu's "Add to shelf" checklist.
    pub shelves: Vec<Shelf>,
    /// Which of `shelves` this book is already on.
    pub shelf_ids: Vec<i64>,
    /// The next volume when it is in the library — the finished state's
    /// primary action opens it.
    pub next_in_series: Option<Book>,
    /// The next volume's title when the cached series listing names it
    /// but the library does not own it.
    pub next_series_title: Option<String>,
    /// 1-based position in the merged series listing, when known. The
    /// page prefers the book's own `series_index` for "Book N"; this is
    /// the fallback for books whose metadata never set one.
    pub series_position: Option<usize>,
    /// Length of the cached series listing ("of M"). `None` when no
    /// listing has been fetched — the owned-book count is never passed
    /// off as the series' total.
    pub series_total: Option<usize>,
    /// Not read by the service (it is DB-pure): the page's worker fills
    /// this with the file's `fs::metadata` size on its way back, so the
    /// stat also stays off the UI thread.
    pub file_size: Option<u64>,
}

/// One volume on the series page: a cached listing entry, an owned book
/// matched into it, or an owned book the listing never mentioned.
///
/// `book: None` is the page's dashed "Not in library" state. The position
/// is the volume's own metadata position where one exists (fractional
/// volumes keep their fraction); the listing's order supplies it where the
/// owned book's metadata never set one.
#[derive(Debug, Default)]
pub struct SeriesEntrySnapshot {
    pub title: String,
    pub position: f32,
    pub year: Option<i64>,
    pub book: Option<Book>,
    pub finished_at: Option<String>,
    /// 0-based current chapter from `reading_progress`, when the book has
    /// been opened. The chapter *count* is a file question, so the page's
    /// worker reads it separately, exactly as the book page does.
    pub chapter_index: Option<usize>,
}

/// Everything the series page draws, in one call.
///
/// The cached provider listing and the owned books are merged here — the
/// same join [`LibraryService::book_page`] reduces to one neighbour — so
/// the page renders one ordered row per volume and never has to know how
/// ownership was matched. `listed_total` is `None` until a listing has
/// been fetched: without one the owned count must never be passed off as
/// the series' total.
#[derive(Debug, Default)]
pub struct SeriesPageSnapshot {
    pub series_name: String,
    pub first_author: String,
    pub series_key: String,
    pub entries: Vec<SeriesEntrySnapshot>,
    pub listed_total: Option<usize>,
    /// The listing's provider id (`openlibrary`) and fetch stamp, for the
    /// source line. Both are `None` on the local-only listing.
    pub source: Option<String>,
    pub fetched_at: Option<String>,
    /// The remembered Strip/List choice for this series, and the pref key
    /// it lives under (the page writes the toggle back through it).
    pub view_list: bool,
    pub view_pref_key: String,
    pub errors: Errors,
}

/// The settings page, first half (7.1 step 2c): every pref- and table-shaped
/// read its six tabs need, in one DB-pure pass. Fast by design — a handful of
/// single-row reads — so the page can fill almost immediately.
///
/// The disk-flavored reads (the per-book sidecar survey, the libraries
/// registry file, the EPUB backup listing) are deliberately NOT here: they
/// are slow, only three rows want them, and the page reads them in its own
/// worker task (`SettingsFs`) so they never delay the fill — the same split
/// `BookPageSnapshot::file_size` made.
#[derive(Debug, Default)]
pub struct SettingsSnapshot {
    /// Installed dictionary packs, for the Dictionaries tab.
    pub dicts: Vec<Dictionary>,
    /// Why `dicts` is empty when its read failed — the old page notified
    /// with this wording inline; the apply now does, off the read.
    pub dicts_error: Option<String>,
    /// The active theme's id, for the Appearance picker's badges.
    pub theme_id: String,
    /// Watch-folder rules and the shelf list they route into (Book Files).
    pub watch_rules: Vec<WatchFolderRule>,
    pub shelves: Vec<Shelf>,
    /// Metadata source toggles and the Google Books key/country prefs
    /// (Metadata Sources).
    pub source_open_library: bool,
    pub source_google_books: bool,
    pub google_books_key: String,
    pub google_books_country: String,
    /// Google rejected the stored API key on its last keyed attempt; the
    /// Metadata sources pane shows the rejected-key state. Written by the
    /// fetcher via `enabled_sources`, cleared on key save or success.
    pub google_books_key_rejected: bool,
    /// EPUB polish-on-import and metadata writeback toggles (Book Files).
    pub clean_on_import: bool,
    pub writeback_enabled: bool,
    /// Reading defaults (the Reading pane): the typeface picker's list
    /// and the current global `reader.*` values, read with the reader's
    /// own defaults and clamps so the pane opens on what a book would
    /// actually get.
    pub typefaces: Vec<String>,
    pub reader_font_family: Option<String>,
    pub reader_font_px: i64,
    pub reader_line_height: f32,
    pub reader_column_px: i64,
    pub reader_justify: bool,
    pub reader_word_memory_scope: String,
    pub reader_ask_impression: bool,
}

/// One work on the author page: a cached bibliography entry, an owned book
/// matched into it by normalised title, or an owned book the bibliography
/// never mentioned. `book: None` is the dashed "Not in library" state.
#[derive(Debug, Default)]
pub struct AuthorEntrySnapshot {
    pub title: String,
    pub year: Option<i64>,
    /// Series position where one is known (bibliography position, else the
    /// owned book's `series_index`); 0 means unknown and sorts last.
    pub position: f32,
    pub book: Option<Book>,
}

/// One card on the author page: a series, or the standalone works.
/// Groups arrive ordered — standalone first, then series by first year —
/// with entries in series order (decision 106).
#[derive(Debug, Default)]
pub struct AuthorGroupSnapshot {
    pub title: String,
    /// `Some(series name)` for a series group, `None` for standalone.
    pub series: Option<String>,
    pub entries: Vec<AuthorEntrySnapshot>,
}

/// The pref prefix under which the author page remembers the shelf it
/// created (`author.shelf.<normalized name>` → `"<shelf id>"`, with a
/// `:linked` suffix when the name switch was on, so a future author
/// rename can follow it — decision 113).
pub const AUTHOR_SHELF_PREF_PREFIX: &str = "author.shelf.";

/// The author page: profile, owned books, the grouped bibliography
/// (cached works merged with owned books, the series page's join widened
/// to a whole author), the author's annotations/bookmarks/words, and the
/// shelf this page created for the author when one is live. The grouping
/// used to run on the UI thread in `init`; it is pure work over the books,
/// so the worker runs it here.
#[derive(Debug, Default)]
pub struct AuthorPageSnapshot {
    pub profile: Option<AuthorProfile>,
    pub owned_books: Vec<Book>,
    pub groups: Vec<AuthorGroupSnapshot>,
    /// Newest first (the order both the highlights row and the dialog use).
    pub annotations: Vec<Annotation>,
    pub bookmarks: Vec<ReadingBookmark>,
    pub words: Vec<SavedWord>,
    /// `(shelf id, shelf name)` for the author's shelf, if one was created
    /// from this page and still exists.
    pub shelf: Option<(i64, String)>,
    /// Owned books with a finished stamp — read state is "stamped or at
    /// 100%", the series page's rule, and the stamp lives outside `Book`.
    pub finished: std::collections::HashSet<i64>,
}

/// One author on the Authors index (decision 117): every spelling of
/// the name in the library folded into one entry, joined with the
/// cached profile when one exists. Book counts follow the author
/// page's rule — comic chapters collapse into their series first.
#[derive(Debug, Clone, Default)]
pub struct AuthorIndexEntry {
    pub name: String,
    pub sort_name: String,
    pub normalized_name: String,
    pub book_count: usize,
    pub read_count: usize,
    pub series_count: usize,
    /// Bibliography size when a fetch cached one; `None` until then.
    pub known_works: Option<i64>,
    pub bio: String,
    pub birth_year: Option<i64>,
    pub death_year: Option<i64>,
    pub top_subjects: Vec<String>,
    pub photo_path: Option<std::path::PathBuf>,
    /// Every spelling that should find this author in search.
    pub aliases: Vec<String>,
}

/// The Authors index page: one entry per author in the library,
/// ordered by sort name.
#[derive(Debug, Default)]
pub struct AuthorsSnapshot {
    pub entries: Vec<AuthorIndexEntry>,
    pub errors: Errors,
}

/// What the Edit author dialog asks to do with the portrait. A
/// replacement carries the image bytes; `edit_author` files them
/// into the authors directory before the catalog write.
#[derive(Debug, Clone, Default)]
pub enum AuthorPortraitEdit {
    #[default]
    Keep,
    Remove,
    Replace(Vec<u8>),
}

/// The Edit author form as one value (decision 112).
#[derive(Debug, Clone, Default)]
pub struct AuthorEditInput {
    pub name: String,
    pub sort_name: String,
    pub bio: String,
    pub birth_date: String,
    pub death_date: String,
    pub aliases: Vec<String>,
    pub portrait: AuthorPortraitEdit,
}

/// The vocabulary page: the visible word list plus its header counts.
///
/// The counts used to come from two extra `list_saved_words` calls whose
/// errors were swallowed with `.unwrap_or(0)`, so a failed read reported
/// "0 known" as though it were a fact.
#[derive(Debug, Default)]
pub struct WordsSnapshot {
    pub words: Vec<SavedWord>,
    pub total: i64,
    pub known: i64,
    pub errors: Errors,
}

/// Saved quotes, each already paired with its book.
///
/// The page used to call `get_book` once per quote, and `get_book` runs a
/// second query for tags — roughly `2N + 1` round trips on every keystroke in
/// the search box. The batch read collapses that to a constant three.
#[derive(Debug, Default)]
pub struct QuotesSnapshot {
    pub quotes: Vec<(Annotation, Option<Book>)>,
    pub errors: Errors,
}

/// Everything the dashboard's "Now reading" card needs, resolved off the
/// UI thread (7.1 step 2b): the EPUB spine is a file open + parse — a
/// 2.20 violation the old inline card carried — and the chapter index is
/// a database read.
#[derive(Debug)]
pub struct NowReading {
    pub book: Book,
    /// Spine entry titles, in order. Empty for non-EPUB books (and for
    /// EPUBs that fail to open — the card falls back to a bare percent).
    pub spine_titles: Vec<String>,
    /// 0-based index of the current chapter, already read on the worker.
    pub chapter_index: usize,
}

/// One merged history-feed row, fully resolved: the per-event progress and
/// format lookups the page used to do **while building its widgets** are
/// baked into `sub` here (7.1 step 2b).
#[derive(Debug, Default)]
pub struct DashboardFeedRow {
    pub at: String,
    pub title: String,
    pub sub: String,
    pub icon: &'static str,
    pub tint: &'static str,
    pub icon_tint: &'static str,
    pub book_id: i64,
}

/// The My Library dashboard. One read of everything the page shows, so a
/// broken database cannot render as a cheerful empty dashboard.
///
/// The page previously made **eight** independent reads and
/// `unwrap_or_default()`d every one of them.
#[derive(Debug, Default)]
pub struct DashboardSnapshot {
    pub stats: LibraryStats,
    /// Most recently opened books; the page picks "now reading" from these.
    pub recently_opened: Vec<Book>,
    /// The resolved "Now reading" card data (None when nothing qualifies).
    pub now_reading: Option<NowReading>,
    /// The merged, lookups-already-done history feed, truncated to the
    /// caller's limit.
    pub feed: Vec<DashboardFeedRow>,
    /// Fallback for the continue strip when nothing has been opened yet.
    pub recent: Vec<Book>,
    pub quotes: Vec<(Annotation, QuoteRef)>,
    pub words: Vec<SavedWord>,
    pub lookups: Vec<DictLookup>,
    pub goal: i64,
    pub finished_this_year: i64,
    pub errors: Errors,
}

/// The history feed: opened/finished events, newest first.
#[derive(Debug, Default)]
pub struct HistorySnapshot {
    pub events: Vec<ReadingEvent>,
    pub errors: Errors,
}

/// The dictionary lookup log.
#[derive(Debug, Default)]
pub struct LookupHistorySnapshot {
    pub lookups: Vec<DictLookup>,
    pub errors: Errors,
}

/// The All books grid: every book matching `query`, in `sort` order.
#[derive(Debug, Default)]
pub struct AllBooksSnapshot {
    pub books: Vec<Book>,
    pub errors: Errors,
}

/// The shelves grid: manual and smart collections with their live counts.
#[derive(Debug, Default)]
pub struct ShelvesSnapshot {
    pub shelves: Vec<Shelf>,
    pub errors: Errors,
}

/// The reading list: the ordered to-be-read queue.
#[derive(Debug, Default)]
pub struct ReadingListSnapshot {
    pub entries: Vec<ReadingListEntry>,
    pub errors: Errors,
}

/// One row of a book picker (reading list, shelf): everything the row's
/// CheckButton needs, assembled in one service read. Plain data so it can
/// cross the worker boundary like any snapshot.
#[derive(Debug, Default)]
pub struct PickerRow {
    pub id: i64,
    pub title: String,
    pub line: String,
    pub ticked: bool,
}

/// The tag cloud: every tag with how many books carry it.
#[derive(Debug, Default)]
pub struct TagsSnapshot {
    pub tags: Vec<(String, i64)>,
    pub errors: Errors,
}

/// Books carrying one tag.
#[derive(Debug, Default)]
pub struct TagBooksSnapshot {
    pub books: Vec<Book>,
    pub errors: Errors,
}

/// Library-wide full-text content search snapshot.
#[derive(Debug, Default)]
pub struct ContentSearchSnapshot {
    pub results: Vec<crate::content_index::BookContentSearchResult>,
    pub total_matches: usize,
    pub total_books: usize,
    pub duration_ms: u128,
    pub errors: Errors,
}

/// Library content search index status snapshot.
#[derive(Debug, Default)]
pub struct ContentIndexStatusSnapshot {
    pub status: crate::content_index::ContentIndexStatus,
    #[allow(dead_code)]
    pub errors: Errors,
}

impl LibraryService {
    pub fn new(catalog: Arc<Catalog>) -> Self {
        Self { catalog }
    }

    /// Escape hatch for code not yet converted (imports, writes, the reader's
    /// own session handling). Kept deliberately visible: every call site is a
    /// place the service does not cover yet.
    pub fn catalog(&self) -> &Arc<Catalog> {
        &self.catalog
    }

    // -- snapshots ---------------------------------------------------------

    /// Home: counts strip, continue row, reading-list peek, recently added.
    pub fn home(&self) -> HomeSnapshot {
        let _t = crate::timing::measure("service_home");
        let mut errors = Errors::new();
        let stats = take(self.catalog.library_stats(), "library stats", &mut errors);
        // Bounded on purpose: Home shows a dozen covers, not the whole library.
        let recent = take(self.catalog.recent_books(12), "recent books", &mut errors);
        let opened = take(
            self.catalog.recently_opened(4),
            "recently opened",
            &mut errors,
        );
        let reading_list = take(
            self.catalog.list_reading_list(),
            "reading list",
            &mut errors,
        );

        HomeSnapshot {
            continue_reading: continue_row(opened, &recent),
            stats,
            recent,
            reading_list,
            errors,
        }
    }

    /// Analytics: totals, streaks, goal progress, this week's activity.
    pub fn analytics(&self) -> AnalyticsSnapshot {
        let _t = crate::timing::measure("service_analytics");
        let mut errors = Errors::new();
        AnalyticsSnapshot {
            stats: take(self.catalog.library_stats(), "library stats", &mut errors),
            // These three already return plain values (they default internally),
            // so there is no error to collect from them.
            goal: self.catalog.reading_goal(),
            finished_this_year: self.catalog.finished_this_year(),
            week: self.catalog.week_activity(),
            errors,
        }
    }

    /// The tag cloud.
    /// Everything the reader loads when a book opens.
    pub fn reader(&self, book_id: i64) -> ReaderSnapshot {
        let _t = crate::timing::measure("service_reader");
        let mut errors = Errors::new();
        let cat = &self.catalog;
        ReaderSnapshot {
            book: take(cat.get_book(book_id), "book", &mut errors),
            annotations: take(
                cat.get_annotations_for_book(book_id),
                "highlights and notes",
                &mut errors,
            ),
            bookmarks: take(
                cat.list_reading_bookmarks(book_id),
                "bookmarks",
                &mut errors,
            ),
            saved_words: take(cat.list_saved_words("", None), "saved words", &mut errors),
            errors,
        }
    }

    /// The book page's stats strip and timeline, in one call.
    pub fn book_stats(&self, book_id: i64, days: i64, session_limit: usize) -> BookStatsSnapshot {
        let _t = crate::timing::measure("service_book_stats");
        let mut errors = Errors::new();
        let cat = &self.catalog;
        let progress: Option<(usize, f64)> = take(
            cat.get_reading_progress(book_id),
            "reading position",
            &mut errors,
        );
        BookStatsSnapshot {
            total_seconds: take(
                cat.total_reading_seconds(book_id),
                "total reading time",
                &mut errors,
            ),
            session_count: take(
                cat.count_sessions_for_book(book_id),
                "session count",
                &mut errors,
            ),
            chapter_index: progress.map(|(ci, _)| ci).unwrap_or(0),
            seconds_by_day: take(
                cat.book_seconds_by_day(book_id, days),
                "daily reading time",
                &mut errors,
            ),
            recent_sessions: take(
                cat.book_recent_sessions(book_id, session_limit),
                "recent sessions",
                &mut errors,
            ),
            finished_at: take(cat.book_finished_at(book_id), "finished date", &mut errors),
            first_opened: take(
                cat.book_first_opened(book_id),
                "first opened date",
                &mut errors,
            ),
            errors,
        }
    }

    /// One shelf plus its books, sorted and filtered.
    ///
    /// The books read is skipped when the shelf itself is missing, so a
    /// deleted shelf produces one clear outcome instead of two vague ones.
    pub fn shelf_detail(&self, shelf_id: i64, sort: SortKey, query: &str) -> ShelfDetailSnapshot {
        let _t = crate::timing::measure("service_shelf_detail");
        let mut errors = Errors::new();
        let shelf: Option<Shelf> = take(self.catalog.get_shelf(shelf_id), "shelf", &mut errors);
        let mut books = match &shelf {
            Some(s) => take(
                self.catalog.shelf_books(s, sort, query),
                "books on this shelf",
                &mut errors,
            ),
            None => Vec::new(),
        };
        // A shelf shows one card per comic series — never individual chapters.
        self.catalog.collapse_comic_chapters(&mut books);
        ShelfDetailSnapshot {
            shelf,
            books,
            errors,
        }
    }

    /// Everything the book float panel shows, in one call.
    pub fn book_detail(&self, book_id: i64) -> BookDetailSnapshot {
        let _t = crate::timing::measure("service_book_detail");
        let mut errors = Errors::new();
        BookDetailSnapshot {
            book: take(self.catalog.get_book(book_id), "book", &mut errors),
            in_reading_list: take(
                self.catalog.is_in_reading_list(book_id),
                "reading list state",
                &mut errors,
            ),
            finished: take(
                self.catalog.book_finished_at(book_id),
                "finished state",
                &mut errors,
            )
            .is_some(),
            errors,
        }
    }

    /// The whole book page in one call. See `BookPageSnapshot`.
    ///
    /// Every sub-read degrades exactly as it did when the page did it
    /// inline: a failed read yields the empty value plus an error row
    /// (or `annotations_error` with the highlights wording), never a
    /// panic and never a silent wrong screen.
    pub fn book_page(&self, book_id: i64) -> BookPageSnapshot {
        let _t = crate::timing::measure("service_book_page");
        let mut detail = self.book_detail(book_id);
        // One read for both consumers of the reading position: the hero
        // progress bar and the journey card queried it separately.
        let progress = self.catalog.get_reading_progress(book_id).ok().flatten();
        let stats = self.book_stats(book_id, 7, 3);
        let (annotations, annotations_error) =
            match self.catalog.get_annotations_for_book(book_id) {
                Ok(rows) => (rows, None),
                Err(err) => (Vec::new(), Some(err.to_string())),
            };
        let bookmarks = take(
            self.catalog.list_reading_bookmarks(book_id),
            "bookmarks",
            &mut detail.errors,
        );
        let saved_words = take(
            self.catalog.get_saved_words_for_scope(book_id, "book"),
            "saved words",
            &mut detail.errors,
        );
        let shelves: Vec<Shelf> = take(
            self.catalog.list_shelves(),
            "shelves",
            &mut detail.errors,
        )
        .into_iter()
        .filter(|s| s.kind == crate::db::ShelfKind::Manual)
        .collect();
        let shelf_ids: Vec<i64> = take(
            self.catalog.shelves_for_book(book_id),
            "shelf membership",
            &mut detail.errors,
        )
        .into_iter()
        .map(|(id, _name)| id)
        .collect();

        // Next in series: the cached provider listing joined with the
        // owned books by normalised title — the same merge the series
        // page renders, reduced to the one neighbour this page needs.
        // Without a cached listing the owned books alone still answer
        // "what do I read next", just not "of M".
        let (next_in_series, next_series_title, series_position, series_total) =
            self.next_in_series(book_id, detail.book.as_ref(), &mut detail.errors);

        BookPageSnapshot {
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
            // The service is DB-pure; the page's worker fills this.
            file_size: None,
        }
    }

    /// Resolve the volume after `book_id` in its series. Returns
    /// `(owned next, unowned next title, position, total)`.
    #[allow(clippy::type_complexity)]
    fn next_in_series(
        &self,
        book_id: i64,
        book: Option<&Book>,
        errors: &mut Errors,
    ) -> (Option<Book>, Option<String>, Option<usize>, Option<usize>) {
        let Some(book) = book else {
            return (None, None, None, None);
        };
        let series = book.series.as_deref().unwrap_or("").trim();
        if series.is_empty() {
            return (None, None, None, None);
        }
        let first_author = book
            .authors
            .split(',')
            .next()
            .unwrap_or("")
            .trim()
            .to_string();
        let key = crate::db::series_key(series, &first_author);
        let works = take(
            self.catalog.get_cached_series(&key),
            "series listing",
            errors,
        )
        .map(|entry| entry.works)
        .unwrap_or_default();
        let owned = take(
            self.catalog.books_in_series(series),
            "books in series",
            errors,
        );

        let normalise = |s: &str| {
            s.to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        };

        // One ordered row per volume: provider order first, owned books
        // matched into their slots, unmatched owned books appended in
        // series order (they are truth the provider never mentioned).
        let mut rows: Vec<(String, Option<Book>)> = works
            .iter()
            .map(|w| (w.title.clone(), None))
            .collect();
        let mut extras: Vec<(String, Option<Book>)> = Vec::new();
        for owned_book in owned {
            let wanted = normalise(&owned_book.title);
            let slot = rows
                .iter()
                .position(|(title, local)| local.is_none() && normalise(title) == wanted);
            match slot {
                Some(i) => rows[i].1 = Some(owned_book),
                None => extras.push((owned_book.title.clone(), Some(owned_book))),
            }
        }
        let has_listing = !rows.is_empty();
        rows.extend(extras);

        let Some(index) = rows
            .iter()
            .position(|(_, local)| local.as_ref().is_some_and(|b| b.id == book_id))
        else {
            return (None, None, None, None);
        };
        let position = Some(index + 1);
        // "of M" only when a fetched listing backs the number.
        let total = has_listing.then_some(rows.len());
        match rows.into_iter().nth(index + 1) {
            Some((_, Some(next))) => (Some(next), None, position, total),
            Some((title, None)) => (None, Some(title), position, total),
            None => (None, None, position, total),
        }
    }

    /// The whole series page in one call. See `SeriesPageSnapshot`.
    ///
    /// The merge is the retired float's, kept honest in the same places:
    /// provider order first, owned books matched in by normalised title,
    /// unmatched owned books appended (they are truth the provider never
    /// mentioned) — with one change the full page needs: an owned book's
    /// own `series_index` outranks its slot in the provider order, so
    /// "Book 2.5" stays Book 2.5 and sorts between 2 and 3.
    pub fn series_page(&self, series: &str, first_author: &str) -> SeriesPageSnapshot {
        let _t = crate::timing::measure("service_series_page");
        let mut errors = Errors::new();
        let series_name = series.trim().to_string();
        let owned = take(
            self.catalog.books_in_series(&series_name),
            "books in series",
            &mut errors,
        );

        let first_author = if first_author.trim().is_empty() {
            owned
                .first()
                .map(|b| {
                    b.authors
                        .split(',')
                        .next()
                        .unwrap_or("")
                        .trim()
                        .to_string()
                })
                .unwrap_or_default()
        } else {
            first_author.trim().to_string()
        };
        let key = crate::db::series_key(&series_name, &first_author);
        let cached = take(
            self.catalog.get_cached_series(&key),
            "series listing",
            &mut errors,
        );
        let (works, source, fetched_at) = match cached {
            Some(entry) => (entry.works, Some(entry.source), Some(entry.fetched_at)),
            None => (Vec::new(), None, None),
        };
        // A series opened with no author and no owned books can still take
        // its byline from the listing itself.
        let first_author = if first_author.is_empty() {
            works
                .iter()
                .find_map(|w| {
                    let author = w.author.trim();
                    (!author.is_empty()).then(|| author.to_string())
                })
                .unwrap_or_default()
        } else {
            first_author
        };

        let normalise = |s: &str| {
            s.to_lowercase()
                .chars()
                .filter(|c| c.is_alphanumeric())
                .collect::<String>()
        };

        let mut entries: Vec<SeriesEntrySnapshot> = works
            .iter()
            .enumerate()
            .map(|(i, work)| SeriesEntrySnapshot {
                title: work.title.clone(),
                position: (i + 1) as f32,
                year: (work.year > 0).then_some(work.year),
                book: None,
                finished_at: None,
                chapter_index: None,
            })
            .collect();

        let mut extras: Vec<Book> = Vec::new();
        for book in owned {
            let wanted = normalise(&book.title);
            let slot = entries
                .iter()
                .position(|e| e.book.is_none() && normalise(&e.title) == wanted);
            match slot {
                Some(i) => {
                    entries[i].title = book.title.clone();
                    if book.series_index > 0.0 {
                        entries[i].position = book.series_index;
                    }
                    if entries[i].year.is_none() {
                        entries[i].year = published_year(&book.published);
                    }
                    entries[i].book = Some(book);
                }
                None => extras.push(book),
            }
        }

        // Unindexed owned books keep the reading order `books_in_series`
        // returned, after everything whose position is actually known.
        let mut next_unindexed = entries
            .iter()
            .map(|e| e.position)
            .fold(0.0f32, f32::max);
        next_unindexed = extras
            .iter()
            .filter(|b| b.series_index > 0.0)
            .fold(next_unindexed, |max, b| max.max(b.series_index));
        for book in extras {
            let position = if book.series_index > 0.0 {
                book.series_index
            } else {
                next_unindexed += 1.0;
                next_unindexed
            };
            entries.push(SeriesEntrySnapshot {
                title: book.title.clone(),
                position,
                year: published_year(&book.published),
                book: Some(book),
                finished_at: None,
                chapter_index: None,
            });
        }
        entries.sort_by(|a, b| a.position.total_cmp(&b.position));

        let listed_total = (!works.is_empty()).then_some(entries.len());

        let owned_ids: Vec<i64> = entries
            .iter()
            .filter_map(|e| e.book.as_ref().map(|b| b.id))
            .collect();
        let finished_dates = take(
            self.catalog.finished_dates_by_ids(&owned_ids),
            "finished dates",
            &mut errors,
        );
        let positions = take(
            self.catalog.reading_progress_by_ids(&owned_ids),
            "reading positions",
            &mut errors,
        );
        for entry in &mut entries {
            if let Some(book) = &entry.book {
                entry.finished_at = finished_dates.get(&book.id).cloned();
                entry.chapter_index = positions.get(&book.id).map(|(chapter, _)| *chapter);
            }
        }

        let view_pref_key = format!("series.view.{key}");
        let view_list = self.catalog.get_pref(&view_pref_key).as_deref() == Some("list");

        SeriesPageSnapshot {
            series_name,
            first_author,
            series_key: key,
            entries,
            listed_total,
            source,
            fetched_at,
            view_list,
            view_pref_key,
            errors,
        }
    }

    /// The settings page's DB-pure reads, one pass (7.1 step 2c). See
    /// `SettingsSnapshot` for why the disk-flavored reads are the page's.
    pub fn settings(&self) -> SettingsSnapshot {
        let _t = crate::timing::measure("service_settings");
        let (dicts, dicts_error) = match self.catalog.list_dictionaries() {
            Ok(rows) => (rows, None),
            Err(err) => (Vec::new(), Some(err.to_string())),
        };
        let (source_open_library, source_google_books) = (
            crate::metadata::source_enabled(&self.catalog, crate::metadata::SourceId::OpenLibrary),
            crate::metadata::source_enabled(&self.catalog, crate::metadata::SourceId::GoogleBooks),
        );
        SettingsSnapshot {
            dicts,
            dicts_error,
            theme_id: crate::theme::current_choice(&self.catalog).id(),
            watch_rules: crate::watch_folder::load_watch_rules(&self.catalog),
            shelves: self.catalog.list_shelves().unwrap_or_default(),
            source_open_library,
            source_google_books,
            google_books_key: self
                .catalog
                .get_pref("meta.googlebooks.key")
                .unwrap_or_default(),
            google_books_country: self
                .catalog
                .get_pref("meta.googlebooks.country")
                .unwrap_or_else(crate::metadata::google_books::detect_country),
            google_books_key_rejected: self
                .catalog
                .get_pref("meta.googlebooks.key_rejected")
                .map(|v| v == "1")
                .unwrap_or(false),
            clean_on_import: crate::epub_sanitizer::clean_on_import_enabled(&self.catalog),
            writeback_enabled: crate::epub_metadata::write_enabled(&self.catalog),
            // The Reading pane's data. The typeface list builds a font
            // system just to enumerate it (bundled faces plus the user's
            // fonts folder) — worker-only work, which is where this runs.
            typefaces: kalam_reader::available_font_families(Some(crate::paths::fonts_dir())),
            reader_font_family: self.catalog.get_pref("reader.font_family"),
            reader_font_px: self.catalog.get_pref_i64("reader.font_px", 17).clamp(13, 24),
            reader_line_height: self
                .catalog
                .get_pref("reader.line_height")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(1.8)
                .clamp(1.3, 2.5),
            reader_column_px: self
                .catalog
                .get_pref_i64("reader.column_px", 620)
                .clamp(400, 860),
            reader_justify: self.catalog.get_pref_i64("reader.justify", 0) != 0,
            reader_word_memory_scope: self
                .catalog
                .get_pref("reader.word_memory_scope")
                .unwrap_or_else(|| "library".into()),
            reader_ask_impression: self.catalog.get_pref_i64("reader.ask_impression", 1) != 0,
        }
    }

    /// The author page's whole data question in one call (7.1 step 2c):
    /// the profile if cached, the owned books, their bibliography
    /// grouping, and the reader's own marks across those books. Nothing
    /// here fetches: the bibliography is shown as cached (decision 110).
    pub fn author_page(&self, author_name: &str) -> AuthorPageSnapshot {
        let _t = crate::timing::measure("service_author");
        let mut owned_books = crate::author::owned_books_for_author(&self.catalog, author_name);
        // One card per comic series, never per chapter — the author's shelf
        // obeys the same rule as every other list.
        self.catalog.collapse_comic_chapters(&mut owned_books);
        let mut profile = self
            .catalog
            .get_author_profile_by_name(author_name)
            .ok()
            .flatten();
        // The photo path is a blind join in the row mapper; whether the
        // download still exists is settled here, on the worker, so the
        // page never stats a file to draw the portrait (7.7).
        if let Some(profile) = &mut profile {
            if profile.photo_path.as_ref().is_some_and(|p| !p.is_file()) {
                profile.photo_path = None;
            }
        }
        let groups = author_groups(profile.as_ref(), &owned_books);

        let ids: Vec<i64> = owned_books.iter().map(|book| book.id).collect();
        let finished = self.catalog.finished_book_ids(&ids).unwrap_or_default();
        let annotations = self
            .catalog
            .annotations_for_books(&ids)
            .unwrap_or_default();
        let bookmarks = self
            .catalog
            .reading_bookmarks_for_books(&ids)
            .unwrap_or_default();
        let words = self.catalog.saved_words_for_books(&ids).unwrap_or_default();

        let shelf = self
            .catalog
            .get_pref(&format!(
                "{AUTHOR_SHELF_PREF_PREFIX}{}",
                crate::author::normalize_author_name(author_name)
            ))
            .and_then(|raw| raw.split(':').next()?.parse::<i64>().ok())
            .and_then(|id| self.catalog.get_shelf(id).ok().flatten())
            .map(|shelf| (shelf.id, shelf.name));

        AuthorPageSnapshot {
            profile,
            owned_books,
            groups,
            annotations,
            bookmarks,
            words,
            shelf,
            finished,
        }
    }

    /// The Authors index (decision 117): every author named by a
    /// book in the library, spellings folded by normalized name and
    /// by cached-profile aliases, with the counts the cards show.
    /// Runs on the page's worker; portrait existence is settled here
    /// so the fill never stats a file.
    pub fn authors(&self) -> AuthorsSnapshot {
        let _t = crate::timing::measure("service_authors");
        let mut errors = Errors::new();
        let mut books = take(
            self.catalog.list_books(SortKey::Title, ""),
            "books",
            &mut errors,
        );
        // One entry per comic series, never per chapter — the same
        // rule the author page's owned count obeys.
        self.catalog.collapse_comic_chapters(&mut books);
        let ids: Vec<i64> = books.iter().map(|b| b.id).collect();
        let finished = take(
            self.catalog.finished_book_ids(&ids),
            "finished books",
            &mut errors,
        );
        let profiles = take(
            self.catalog.all_author_profiles(),
            "author profiles",
            &mut errors,
        );
        let mut entries = aggregate_author_index(&books, &finished, &profiles);
        for entry in &mut entries {
            if entry.photo_path.as_ref().is_some_and(|p| !p.is_file()) {
                entry.photo_path = None;
            }
        }
        AuthorsSnapshot { entries, errors }
    }

    /// Save an Edit author form (decision 112). A replacement
    /// portrait is filed into the authors directory first; the
    /// catalog then renames the identity and locks the changed
    /// fields in one transaction.
    pub fn edit_author(
        &self,
        current_name: &str,
        input: AuthorEditInput,
    ) -> std::result::Result<crate::db::AuthorEditOutcome, String> {
        let photo = match input.portrait {
            AuthorPortraitEdit::Keep => crate::db::AuthorPhotoEdit::Keep,
            AuthorPortraitEdit::Remove => crate::db::AuthorPhotoEdit::Remove,
            AuthorPortraitEdit::Replace(bytes) => {
                if bytes.is_empty() {
                    return Err("That image file is empty.".into());
                }
                let file = store_author_portrait(&input.name, &bytes)?;
                crate::db::AuthorPhotoEdit::Replace(file)
            }
        };
        self.catalog
            .edit_author(
                current_name,
                &crate::db::AuthorEdit {
                    name: input.name,
                    sort_name: input.sort_name,
                    bio: input.bio,
                    birth_date: input.birth_date,
                    death_date: input.death_date,
                    aliases: input.aliases,
                    photo,
                },
            )
            .map_err(|e| e.to_string())
    }

    /// Merge one author into another (decision 112); see
    /// [`Catalog::merge_authors`] for the semantics.
    pub fn merge_authors(
        &self,
        source_name: &str,
        target_name: &str,
    ) -> std::result::Result<crate::db::AuthorEditOutcome, String> {
        self.catalog
            .merge_authors(source_name, target_name)
            .map_err(|e| e.to_string())
    }

    /// Vocabulary matching `query` and `known`, plus the header counts.
    ///
    /// The counts deliberately cover the whole table, not just the page's
    /// 500-row display cap, so the header stays true for large vocabularies.
    pub fn words(&self, query: &str, known: Option<bool>) -> WordsSnapshot {
        let _t = crate::timing::measure("service_words");
        let mut errors = Errors::new();
        let words = take(
            self.catalog.list_saved_words(query, known),
            "saved words",
            &mut errors,
        );
        let (total, known_count) = take(
            self.catalog.saved_word_counts(),
            "vocabulary counts",
            &mut errors,
        );
        WordsSnapshot {
            words,
            total,
            known: known_count,
            errors,
        }
    }

    /// Saved quotes matching `query`, each paired with its book.
    ///
    /// A book that no longer exists yields `None` rather than an error row —
    /// that is a missing book, not a failed read, and the page already has
    /// wording for it.
    pub fn quotes(&self, query: &str) -> QuotesSnapshot {
        let _t = crate::timing::measure("service_quotes");
        let mut errors = Errors::new();
        let annos: Vec<Annotation> = take(
            self.catalog.list_all_quotes(query),
            "saved quotes",
            &mut errors,
        );
        let ids: Vec<i64> = annos.iter().map(|a| a.book_id).collect();
        let books = take(self.catalog.books_by_ids(&ids), "quote books", &mut errors);
        let quotes = annos
            .into_iter()
            .map(|a| {
                let book = books.get(&a.book_id).cloned();
                (a, book)
            })
            .collect();
        QuotesSnapshot { quotes, errors }
    }

    /// My Library dashboard: every strip on the page, in one call.
    ///
    /// `feed_limit` is doubled internally the way the page does it: the feed
    /// merges events with sessions and then trims, so both sides need slack.
    pub fn dashboard(&self, feed_limit: usize) -> DashboardSnapshot {
        let _t = crate::timing::measure("service_dashboard");
        let mut errors = Errors::new();
        let cat = &self.catalog;
        let stats = take(cat.library_stats(), "library stats", &mut errors);
        let recently_opened = take(cat.recently_opened(6), "recently opened", &mut errors);
        // "Now reading": the most recently opened book that isn't finished.
        // Resolved here — on the worker — so the page never opens an EPUB
        // or reads progress while building widgets (7.1 step 2b).
        let now_reading = recently_opened
            .iter()
            .find(|b| b.progress < 100)
            .map(|book| {
                let spine_titles = if book.format == crate::models::BookFormat::Epub {
                    crate::epub_book::OpenBook::open(
                        &book.file_path,
                        &crate::paths::reader_cache_dir(&book.uuid),
                    )
                    .map(|ob| ob.spine.iter().map(|sp| sp.title.clone()).collect())
                    .unwrap_or_default()
                } else {
                    Vec::new()
                };
                let chapter_index = cat
                    .get_reading_progress(book.id)
                    .ok()
                    .flatten()
                    .map(|(idx, _)| idx)
                    .unwrap_or(0);
                NowReading {
                    book: book.clone(),
                    spine_titles,
                    chapter_index,
                }
            });
        let recent = take(cat.recent_books(60), "recent books", &mut errors);
        let quotes = take(cat.recent_quotes(2), "recent quotes", &mut errors);
        let words = take(cat.list_saved_words("", None), "saved words", &mut errors);
        let lookups = take(cat.list_dict_lookups("", 3), "recent lookups", &mut errors);
        let events = take(
            cat.list_events(None, "", feed_limit * 2),
            "reading history",
            &mut errors,
        );
        let sessions = take(
            cat.recent_sessions(feed_limit * 2),
            "reading sessions",
            &mut errors,
        );
        let feed = dashboard_feed(cat, &events, &sessions, feed_limit);
        DashboardSnapshot {
            stats,
            recently_opened,
            now_reading,
            feed,
            recent,
            quotes,
            words,
            lookups,
            // Infallible by construction (they fall back to a default inside
            // the catalog), so they contribute no error rows.
            goal: cat.reading_goal(),
            finished_this_year: cat.finished_this_year(),
            errors,
        }
    }

    /// History page: reading events, optionally filtered by kind and text.
    pub fn history(&self, kind: Option<EventKind>, query: &str, limit: usize) -> HistorySnapshot {
        let _t = crate::timing::measure("service_history");
        let mut errors = Errors::new();
        HistorySnapshot {
            events: take(
                self.catalog.list_events(kind, query, limit),
                "reading history",
                &mut errors,
            ),
            errors,
        }
    }

    /// Lookup History page: the dictionary lookup log.
    pub fn lookup_history(&self, query: &str, limit: usize) -> LookupHistorySnapshot {
        let _t = crate::timing::measure("service_lookup_history");
        let mut errors = Errors::new();
        LookupHistorySnapshot {
            lookups: take(
                self.catalog.list_dict_lookups(query, limit),
                "lookup history",
                &mut errors,
            ),
            errors,
        }
    }

    /// All books page: the whole library, filtered by `query` and sorted.
    /// When comic series exist, individual chapters are collapsed into their parent
    /// series so that every comic is displayed as one item.
    pub fn all_books(&self, sort: SortKey, query: &str) -> AllBooksSnapshot {
        let _t = crate::timing::measure("service_all_books");
        let mut errors = Errors::new();
        let mut books = take(self.catalog.list_books(sort, query), "books", &mut errors);
        self.catalog.collapse_comic_chapters(&mut books);
        AllBooksSnapshot {
            books,
            errors,
        }
    }

    /// Deep content search across full book text in the library.
    pub fn search_content(&self, query: &str) -> ContentSearchSnapshot {
        let _t = crate::timing::measure("service_search_content");
        let start = std::time::Instant::now();
        let mut errors = Errors::new();
        let results = take(
            self.catalog.search_book_contents(query),
            "search_content",
            &mut errors,
        );
        let total_matches: usize = results.iter().map(|r| r.total_matches).sum();
        let total_books = results.len();
        let duration_ms = start.elapsed().as_millis();
        ContentSearchSnapshot {
            results,
            total_matches,
            total_books,
            duration_ms,
            errors,
        }
    }

    /// Read the library content index status.
    pub fn index_status(&self) -> ContentIndexStatusSnapshot {
        let mut errors = Errors::new();
        let status = take(
            self.catalog.get_content_index_status(),
            "index_status",
            &mut errors,
        );
        ContentIndexStatusSnapshot { status, errors }
    }

    /// Index all unindexed books into the full-text search index.
    pub fn index_unindexed_books(&self) -> Result<usize, String> {
        self.catalog
            .index_all_unindexed_books()
            .map_err(|e| e.to_string())
    }

    /// Rebuild the library full-text search index from scratch.
    pub fn reindex_all_books(&self) -> Result<usize, String> {
        self.catalog
            .reindex_all_books()
            .map_err(|e| e.to_string())
    }

    /// Shelves page: every shelf, ordered as stored.
    pub fn shelves(&self) -> ShelvesSnapshot {
        let _t = crate::timing::measure("service_shelves");
        let mut errors = Errors::new();
        ShelvesSnapshot {
            shelves: take(self.catalog.list_shelves(), "shelves", &mut errors),
            errors,
        }
    }

    /// Reading list page: the ordered queue.
    pub fn reading_list(&self) -> ReadingListSnapshot {
        let _t = crate::timing::measure("service_reading_list");
        let mut errors = Errors::new();
        let mut entries = take(
            self.catalog.list_reading_list(),
            "reading list",
            &mut errors,
        );
        // One row per comic series, never per chapter: collapse the books,
        // then keep exactly the entries whose book survived. The surviving
        // entry keeps its own position and note; its book is the retitled
        // representative.
        {
            let mut books: Vec<Book> = entries.iter().map(|e| e.book.clone()).collect();
            self.catalog.collapse_comic_chapters(&mut books);
            let mut survived: std::collections::HashMap<i64, Book> =
                books.into_iter().map(|b| (b.id, b)).collect();
            entries.retain_mut(|e| match survived.remove(&e.book.id) {
                Some(b) => {
                    e.book = b;
                    true
                }
                None => false,
            });
        }
        ReadingListSnapshot {
            entries,
            errors,
        }
    }

    /// The "Add to reading list" picker's rows: every book in the library
    /// (matching `query`), comics collapsed to one row per series — the
    /// owner's rule holds in pickers too — each pre-checked when any of its
    /// chapters is already queued. Formerly assembled inline by the page,
    /// one peers lookup and one membership query per row; now it is a
    /// service read with a step-4 budget on it.
    pub fn reading_list_picker(&self, query: &str) -> Vec<PickerRow> {
        let _t = crate::timing::measure("picker_read");
        let membership: std::collections::HashSet<i64> = self
            .catalog
            .reading_list_book_ids()
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.picker_rows(query, membership)
    }

    /// The shelf's "Add books" picker: the same rows, ticked when any of
    /// the book's (or series') chapters is already on the shelf.
    pub fn shelf_picker(&self, shelf_id: i64, query: &str) -> Vec<PickerRow> {
        let _t = crate::timing::measure("picker_read");
        let membership: std::collections::HashSet<i64> = self
            .catalog
            .shelf_book_ids(shelf_id)
            .unwrap_or_default()
            .into_iter()
            .collect();
        self.picker_rows(query, membership)
    }

    /// The shared half of both pickers: list, collapse comics to one row
    /// per series, then resolve ticked state from one batched peers map and
    /// the membership set — nothing per row. A row whose peers cannot be
    /// resolved (an unknown book) degrades to itself, exactly what the
    /// pages' `unwrap_or_else(|_| vec![book.id])` did.
    fn picker_rows(
        &self,
        query: &str,
        membership: std::collections::HashSet<i64>,
    ) -> Vec<PickerRow> {
        let mut books = self
            .catalog
            .list_books(SortKey::Title, query)
            .unwrap_or_default();
        // One row per comic series, never per chapter.
        self.catalog.collapse_comic_chapters(&mut books);
        let comic_ids: Vec<i64> = books
            .iter()
            .filter(|b| {
                matches!(
                    b.format,
                    crate::models::BookFormat::Cbz | crate::models::BookFormat::Cbr
                )
            })
            .map(|b| b.id)
            .collect();
        let peers = self
            .catalog
            .comic_series_peers_by_ids(&comic_ids)
            .unwrap_or_default();
        books
            .into_iter()
            .map(|book| {
                let own = std::iter::once(book.id);
                let row_peers: &[i64] = peers.get(&book.id).map_or(&[], Vec::as_slice);
                let ticked = own
                    .chain(row_peers.iter().copied())
                    .any(|id| membership.contains(&id));
                PickerRow {
                    id: book.id,
                    title: book.title.clone(),
                    line: format!("{} — {}", book.title, book.authors_display()),
                    ticked,
                }
            })
            .collect()
    }

    pub fn tags(&self) -> TagsSnapshot {
        let _t = crate::timing::measure("service_tags");
        let mut errors = Errors::new();
        TagsSnapshot {
            tags: take(self.catalog.list_tags_with_counts(), "tags", &mut errors),
            errors,
        }
    }

    /// Books carrying `tag`, in `sort` order.
    pub fn tag_books(&self, tag: &str, sort: SortKey) -> TagBooksSnapshot {
        let _t = crate::timing::measure("service_tag_books");
        let mut errors = Errors::new();
        let mut books = take(
            self.catalog.books_with_tag(tag, sort),
            "books for tag",
            &mut errors,
        );
        // One card per comic series, never per chapter.
        self.catalog.collapse_comic_chapters(&mut books);
        TagBooksSnapshot {
            books,
            errors,
        }
    }
}

/// Unwrap a query result, or record why it failed and fall back to the empty
/// value. This is the one place the "a failed read shows as empty" policy
/// lives; it used to be repeated, silently, in every page.
#[derive(Default)]
struct AuthorAccum {
    /// Display spellings seen on books, with counts — the most-used
    /// spelling names the entry when no profile canonicalizes it.
    candidates: std::collections::HashMap<String, usize>,
    books: std::collections::HashSet<i64>,
    read: std::collections::HashSet<i64>,
    series: std::collections::HashSet<String>,
    spellings: Vec<String>,
    profile: Option<usize>,
}

/// Fold the library's books into one index entry per author
/// (decision 117). Pure work over the reads `authors()` made.
fn aggregate_author_index(
    books: &[Book],
    finished: &std::collections::HashSet<i64>,
    profiles: &[AuthorProfile],
) -> Vec<AuthorIndexEntry> {
    use std::collections::HashMap;

    let mut profile_by_key: HashMap<String, usize> = HashMap::new();
    for (index, profile) in profiles.iter().enumerate() {
        let mut claim = |key: String| {
            if !key.is_empty() {
                profile_by_key.entry(key).or_insert(index);
            }
        };
        claim(profile.normalized_name.trim().to_string());
        claim(crate::author::normalize_author_name(&profile.canonical_name));
        claim(crate::author::normalize_author_name(&profile.sort_name));
        for alias in &profile.aliases {
            claim(crate::author::normalize_author_name(alias));
        }
    }

    let mut by_key: HashMap<String, AuthorAccum> = HashMap::new();
    for book in books {
        if book.authors.trim().is_empty() {
            continue;
        }
        for raw_name in crate::author::split_author_names(&book.authors) {
            let key = crate::author::normalize_author_name(&raw_name);
            if key.is_empty() {
                continue;
            }
            let (identity, profile_index) = match profile_by_key.get(&key) {
                Some(&index) => {
                    let profile = &profiles[index];
                    let identity = if profile.normalized_name.trim().is_empty() {
                        crate::author::normalize_author_name(&profile.canonical_name)
                    } else {
                        profile.normalized_name.trim().to_string()
                    };
                    (identity, Some(index))
                }
                None => (key, None),
            };
            let acc = by_key.entry(identity).or_default();
            if profile_index.is_some() {
                acc.profile = profile_index;
            }
            *acc.candidates
                .entry(crate::author::display_author_name(&raw_name))
                .or_insert(0) += 1;
            acc.spellings.push(raw_name);
            acc.books.insert(book.id);
            if finished.contains(&book.id) || book.progress >= 100 {
                acc.read.insert(book.id);
            }
            if let Some(series) = book.series.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
                acc.series
                    .insert(series.to_lowercase().split_whitespace().collect::<Vec<_>>().join(" "));
            }
        }
    }

    let mut entries: Vec<AuthorIndexEntry> = by_key
        .into_iter()
        .map(|(normalized_name, acc)| {
            let profile = acc.profile.map(|index| &profiles[index]);
            let name = profile
                .map(|p| p.canonical_name.trim().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| {
                    let mut candidates: Vec<(String, usize)> =
                        acc.candidates.iter().map(|(n, c)| (n.clone(), *c)).collect();
                    candidates
                        .sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
                    candidates
                        .into_iter()
                        .next()
                        .map(|(n, _)| n)
                        .unwrap_or_default()
                });
            let sort_name = profile
                .map(|p| p.sort_name.trim().to_string())
                .filter(|n| !n.is_empty())
                .unwrap_or_else(|| crate::author::sort_author_name(&name));

            let mut aliases: Vec<String> = Vec::new();
            let mut seen = std::collections::HashSet::new();
            let mut push_alias = |alias: &str| {
                let alias = alias.trim();
                if !alias.is_empty()
                    && seen.insert(crate::author::normalize_author_name(alias))
                {
                    aliases.push(alias.to_string());
                }
            };
            for spelling in &acc.spellings {
                push_alias(spelling);
            }
            if let Some(profile) = profile {
                for alias in &profile.aliases {
                    push_alias(alias);
                }
            }

            AuthorIndexEntry {
                name,
                sort_name,
                normalized_name,
                book_count: acc.books.len(),
                read_count: acc.read.len(),
                series_count: acc.series.len(),
                known_works: profile
                    .filter(|p| !p.works.is_empty())
                    .map(|p| p.work_count.max(p.works.len() as i64)),
                bio: profile.map(|p| p.bio.clone()).unwrap_or_default(),
                birth_year: profile.and_then(|p| published_year(&p.birth_date)),
                death_year: profile.and_then(|p| published_year(&p.death_date)),
                top_subjects: profile
                    .map(|p| p.top_subjects.clone())
                    .unwrap_or_default(),
                photo_path: profile.and_then(|p| p.photo_path.clone()),
                aliases,
            }
        })
        .collect();
    entries.sort_by(|a, b| {
        a.sort_name
            .to_lowercase()
            .cmp(&b.sort_name.to_lowercase())
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
    entries
}

/// File a hand-picked portrait into the authors directory and
/// return its file name. Content-hashed, so picking the same image
/// twice is one file and a changed image never reads back stale
/// through the cover cache.
fn store_author_portrait(name: &str, bytes: &[u8]) -> std::result::Result<String, String> {
    use std::hash::{Hash, Hasher};

    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    bytes.hash(&mut hasher);
    let ext = if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        "png"
    } else if bytes.starts_with(&[0xFF, 0xD8]) {
        "jpg"
    } else if bytes.starts_with(b"GIF8") {
        "gif"
    } else if bytes.len() >= 12 && &bytes[0..4] == b"RIFF" && &bytes[8..12] == b"WEBP" {
        "webp"
    } else {
        "img"
    };
    let mut stem: String = crate::author::normalize_author_name(name)
        .chars()
        .take(32)
        .collect();
    if stem.is_empty() {
        stem = "author".to_string();
    }
    let file_name = format!("custom-{stem}-{:016x}.{ext}", hasher.finish());
    let dir = crate::paths::authors_dir();
    std::fs::create_dir_all(&dir).map_err(|e| format!("Could not create the authors folder: {e}"))?;
    let path = dir.join(&file_name);
    if path.is_file() {
        return Ok(file_name);
    }
    let tmp = dir.join(format!(".{file_name}.{}.tmp", std::process::id()));
    std::fs::write(&tmp, bytes).map_err(|e| format!("Could not save the portrait: {e}"))?;
    match std::fs::rename(&tmp, &path) {
        Ok(()) => Ok(file_name),
        Err(err) => {
            let _ = std::fs::remove_file(&tmp);
            if path.is_file() {
                Ok(file_name)
            } else {
                Err(format!("Could not save the portrait: {err}"))
            }
        }
    }
}

/// The first plausible four-digit year in a free-form published string
/// (`1969`, `1969-06-01`, `June 1969`). The catalog stores whatever the
/// file's metadata said, so a series row that wants a year has to dig it
/// out rather than assume a shape.
fn published_year(published: &str) -> Option<i64> {
    let bytes = published.as_bytes();
    let last = bytes.len().checked_sub(4)?;
    for i in 0..=last {
        if bytes[i].is_ascii_digit()
            && bytes[i + 1].is_ascii_digit()
            && bytes[i + 2].is_ascii_digit()
            && bytes[i + 3].is_ascii_digit()
        {
            let year: i64 = published[i..i + 4].parse().ok()?;
            if (1000..=2999).contains(&year) {
                return Some(year);
            }
        }
    }
    None
}

/// Merge a cached bibliography with the owned books into the author
/// page's cards (decisions 106, 110): bibliography works first (duplicate
/// editions collapse by normalised title), owned books matched in by
/// normalised title, unmatched owned books grouped by the series their
/// file metadata names, else standalone. Without a bibliography the
/// owned books alone still group the same way — the page just shows no
/// totals and no missing works.
fn author_groups(
    profile: Option<&AuthorProfile>,
    owned_books: &[Book],
) -> Vec<AuthorGroupSnapshot> {
    use std::collections::HashMap;

    let normalise = |s: &str| {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    };

    // Groups keyed by normalised series name; "" is standalone. Built in
    // encounter order, reordered at the end.
    let mut groups: Vec<AuthorGroupSnapshot> = Vec::new();
    let mut group_index: HashMap<String, usize> = HashMap::new();
    // Normalised title → (group, entry) for bibliography works, so owned
    // books land in their work's slot.
    let mut work_slots: HashMap<String, (usize, usize)> = HashMap::new();

    fn group_for<'g>(
        groups: &'g mut Vec<AuthorGroupSnapshot>,
        group_index: &mut HashMap<String, usize>,
        key: &str,
        series: Option<String>,
    ) -> &'g mut AuthorGroupSnapshot {
        let index = *group_index.entry(key.to_string()).or_insert_with(|| {
            groups.push(AuthorGroupSnapshot {
                title: series
                    .clone()
                    .unwrap_or_else(|| "Standalone works".to_string()),
                series,
                entries: Vec::new(),
            });
            groups.len() - 1
        });
        &mut groups[index]
    }

    if let Some(profile) = profile {
        for work in &profile.works {
            let title = work.title.trim();
            if title.is_empty() {
                continue;
            }
            let title_key = normalise(title);
            // Duplicate editions/translations of one work collapse.
            if work_slots.contains_key(&title_key) {
                continue;
            }
            let series_name = work.series_name.trim();
            let (key, series) = if series_name.is_empty() {
                (String::new(), None)
            } else {
                (normalise(series_name), Some(series_name.to_string()))
            };
            let group = group_for(&mut groups, &mut group_index, &key, series);
            let group_pos = group_index[&key];
            // Open Library positions arrive as text ("1", "2.5", sometimes
            // prose); anything unparseable is simply unknown.
            let position = work.series_position.trim().parse::<f32>().unwrap_or(0.0);
            group.entries.push(AuthorEntrySnapshot {
                title: title.to_string(),
                year: work.first_publish_year,
                position,
                book: None,
            });
            work_slots.insert(title_key, (group_pos, group.entries.len() - 1));
        }
    }

    for book in owned_books {
        let title_key = normalise(&book.title);
        if let Some(&(group_pos, entry_pos)) = work_slots.get(&title_key) {
            let entry = &mut groups[group_pos].entries[entry_pos];
            if entry.book.is_none() {
                entry.title = book.title.clone();
                if entry.year.is_none() {
                    entry.year = published_year(&book.published);
                }
                if book.series_index > 0.0 {
                    entry.position = book.series_index;
                }
                entry.book = Some(book.clone());
                continue;
            }
        }
        let series_name = book.series.as_deref().unwrap_or("").trim();
        let (key, series) = if series_name.is_empty() {
            (String::new(), None)
        } else {
            (normalise(series_name), Some(series_name.to_string()))
        };
        let group = group_for(&mut groups, &mut group_index, &key, series);
        group.entries.push(AuthorEntrySnapshot {
            title: book.title.clone(),
            year: published_year(&book.published),
            position: if book.series_index > 0.0 {
                book.series_index
            } else {
                0.0
            },
            book: Some(book.clone()),
        });
    }

    groups.retain(|group| !group.entries.is_empty());
    for group in &mut groups {
        let is_series = group.series.is_some();
        group.entries.sort_by(|a, b| {
            if is_series {
                let pos = |e: &AuthorEntrySnapshot| {
                    if e.position > 0.0 { e.position } else { f32::MAX }
                };
                pos(a)
                    .partial_cmp(&pos(b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            } else {
                std::cmp::Ordering::Equal
            }
            .then_with(|| {
                a.year
                    .unwrap_or(i64::MAX)
                    .cmp(&b.year.unwrap_or(i64::MAX))
            })
            .then_with(|| a.title.cmp(&b.title))
        });
    }
    // Standalone first, then series by first year (decision 106).
    groups.sort_by(|a, b| {
        let rank = |g: &AuthorGroupSnapshot| {
            (
                g.series.is_some(),
                g.entries
                    .iter()
                    .filter_map(|e| e.year)
                    .min()
                    .unwrap_or(i64::MAX),
            )
        };
        rank(a).cmp(&rank(b)).then_with(|| a.title.cmp(&b.title))
    });
    groups
}

fn take<T: Default>(result: crate::db::Result<T>, what: &str, errors: &mut Errors) -> T {
    match result {
        Ok(value) => value,
        Err(err) => {
            errors.push(format!("{what}: {err}"));
            T::default()
        }
    }
}

/// Home's "continue reading" rule, extracted so it is testable and so the page
/// does not carry query logic: prefer genuinely recently-opened books, else any
/// book in progress, else the newest book so the row is never empty in a
/// freshly-imported library.
fn continue_row(recently_opened: Vec<Book>, recent: &[Book]) -> Vec<Book> {
    if !recently_opened.is_empty() {
        return recently_opened;
    }
    let in_progress: Vec<Book> = recent
        .iter()
        .filter(|b| b.progress > 0 && b.progress < 100)
        .take(4)
        .cloned()
        .collect();
    if !in_progress.is_empty() {
        return in_progress;
    }
    recent.first().cloned().into_iter().collect()
}

/// The merged history feed (events + reading sessions), fully resolved on
/// the worker: the per-event progress and format lookups the page used to
/// do while building are baked into `sub` here (7.1 step 2b).
fn dashboard_feed(
    cat: &Catalog,
    events: &[ReadingEvent],
    sessions: &[LibrarySession],
    feed_limit: usize,
) -> Vec<DashboardFeedRow> {
    // Batched up front. The feed used to call `get_reading_progress` once
    // per Opened event and `get_book` once per Imported event — bounded by
    // the feed limit, but "small × one-query-per-row" is exactly the shape
    // the step-4 perf budgets exist to catch, and the dashboard budget
    // caught this on its first run (2026-10-04).
    let progress_by_id: std::collections::HashMap<i64, (usize, f64)> = cat
        .reading_progress_by_ids(
            &events
                .iter()
                .filter(|e| e.kind == EventKind::Opened)
                .map(|e| e.book_id)
                .collect::<Vec<_>>(),
        )
        .unwrap_or_default();
    let books_by_id = cat
        .books_by_ids(
            &events
                .iter()
                .filter(|e| e.kind == EventKind::Imported)
                .map(|e| e.book_id)
                .collect::<Vec<_>>(),
        )
        .unwrap_or_default();

    let mut items: Vec<DashboardFeedRow> = Vec::new();

    for e in events {
        let sub = match e.kind {
            // Opened events carry no detail -- show where the book
            // currently sits instead.
            EventKind::Opened => progress_by_id
                .get(&e.book_id)
                .map(|(_, frac)| format!("Resumed at {}%", (frac * 100.0).round() as i64))
                .unwrap_or_default(),
            EventKind::Finished => {
                if e.detail == "auto" {
                    format!("{} · auto-finished", e.book_authors)
                } else {
                    e.book_authors.clone()
                }
            }
            EventKind::Unfinished => e.book_authors.clone(),
            EventKind::Imported => {
                let format_label = books_by_id
                    .get(&e.book_id)
                    .map(|b| b.format.as_str().to_string())
                    .unwrap_or_default();
                if format_label.is_empty() {
                    e.book_authors.clone()
                } else {
                    format!("{} · {}", e.book_authors, format_label)
                }
            }
        };
        items.push(DashboardFeedRow {
            at: e.at.clone(),
            title: format!("{} {}", e.kind.label(), e.book_title),
            sub,
            icon: e.kind.icon(),
            tint: match e.kind {
                EventKind::Finished => "kalam-hist-tint-success",
                EventKind::Imported => "kalam-hist-tint-warning",
                _ => "kalam-hist-tint-accent",
            },
            icon_tint: match e.kind {
                EventKind::Finished => "kalam-event-finished",
                EventKind::Imported => "kalam-event-imported",
                _ => "kalam-event-opened",
            },
            book_id: e.book_id,
        });
    }

    for s in sessions {
        if s.seconds < 30 {
            continue; // ignore flip-in-and-out sessions
        }
        let mins = (s.seconds / 60).max(1);
        let sub = if (1..100).contains(&s.end_pct) {
            format!("{mins} min session · reached {}%", s.end_pct)
        } else {
            format!("{mins} min session")
        };
        items.push(DashboardFeedRow {
            at: s.started_at.clone(),
            title: format!("Read {}", s.book_title),
            sub,
            icon: "media-playback-start-symbolic",
            tint: "kalam-hist-tint-accent",
            icon_tint: "kalam-event-opened",
            book_id: s.book_id,
        });
    }

    // ISO-8601 UTC strings compare chronologically.
    items.sort_by(|a, b| b.at.cmp(&a.at));
    items.truncate(feed_limit);
    items
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::ShelfKind;
    use crate::models::BookFormat;

    /// The reason snapshots exist: they must be able to cross a thread
    /// boundary, so that moving a query onto a worker is a change *here* and
    /// not in every page. Asserted at compile time — putting an `Rc` or a GTK
    /// widget in a snapshot will fail this rather than being noticed later.
    #[test]
    fn snapshots_are_send() {
        fn assert_send<T: Send>() {}
        assert_send::<HomeSnapshot>();
        assert_send::<AnalyticsSnapshot>();
        assert_send::<TagsSnapshot>();
        assert_send::<TagBooksSnapshot>();
        assert_send::<DashboardSnapshot>();
        assert_send::<QuotesSnapshot>();
        assert_send::<WordsSnapshot>();
        assert_send::<BookDetailSnapshot>();
        assert_send::<ShelfDetailSnapshot>();
        assert_send::<BookStatsSnapshot>();
        // The book page snapshot crosses a worker boundary (7.1 step 2a).
        assert_send::<BookPageSnapshot>();
        // The series page reads on the same worker pattern.
        assert_send::<SeriesPageSnapshot>();
        // Settings and the author page join it in 7.1 step 2c.
        assert_send::<SettingsSnapshot>();
        assert_send::<AuthorPageSnapshot>();
        assert_send::<ReaderSnapshot>();
        // Added when `all_books` became the 1.2b pilot: it now crosses a
        // thread boundary, so the property has to be compile-checked rather
        // than assumed.
        assert_send::<AllBooksSnapshot>();
        assert_send::<ContentSearchSnapshot>();
        assert_send::<ContentIndexStatusSnapshot>();
        // History joins them in the same 1.2b pass. Deliberately *not* the
        // snapshots for pages that stay synchronous (shelves, reading list,
        // analytics, reader, book detail) — asserting `Send` on a type that
        // never crosses a thread proves nothing and reads as coverage it
        // isn't.
        assert_send::<HistorySnapshot>();
        // Picker rows cross the worker boundary with the picker's read
        // (step 4, the comic-axis increment).
        assert_send::<PickerRow>();
        // The service itself must be Send too, or it cannot be moved onto the
        // worker that would run those queries.
        assert_send::<LibraryService>();
    }

    /// Roadmap 7.1 step 2a: the book page snapshot is one fixed set of
    /// statements. A book with progress, highlights and same-author
    /// siblings must cost exactly the same as a bare one — the
    /// count-shaped budget in the proven `perf.rs` pattern (an integer,
    /// identical on every machine, cannot flake).
    #[test]
    fn book_page_statement_count_is_fixed() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let bare = seed(svc.catalog(), "Bare", &[]);
        let busy = seed(svc.catalog(), "Busy", &[]);
        // Same author as `bare` (seed always uses "An Author"), plus
        // progress and highlights: everything the snapshot reads, present
        // for one book and absent for the other.
        svc.catalog()
            .set_reading_progress(busy, 3, 0.5, 10)
            .expect("seed progress");
        for i in 0..5 {
            svc.catalog()
                .insert_annotation(
                    busy,
                    "highlight",
                    i,
                    "p",
                    0,
                    "p",
                    9,
                    "amber",
                    "solid",
                    &format!("excerpt {i}"),
                    "",
                )
                .expect("seed annotation");
        }

        let n_bare = svc.catalog().count_queries(|| svc.book_page(bare));
        let n_busy = svc.catalog().count_queries(|| svc.book_page(busy));
        assert_eq!(
            n_bare, n_busy,
            "book_page must be a fixed set of statements: {n_bare} for a bare book vs {n_busy} for one with progress and highlights"
        );

        // And the snapshot carries the seeded state.
        let snap = svc.book_page(busy);
        assert_eq!(
            snap.detail.book.as_ref().map(|b| b.title.as_str()),
            Some("Busy")
        );
        assert_eq!(snap.progress.map(|(c, _)| c), Some(3));
        assert_eq!(snap.annotations.len(), 5);
        // The redesign's reads ride along empty rather than erroring:
        // no bookmarks or saved words were seeded, and neither book is
        // in a series, so there is no next volume to offer.
        assert!(snap.bookmarks.is_empty());
        assert!(snap.saved_words.is_empty());
        assert!(snap.next_in_series.is_none());
    }

    /// 7.1 step 2c: the settings snapshot is one fixed set of statements.
    /// A settings page with dictionaries and shelves installed must cost
    /// exactly what a factory-fresh one does — same count-shaped budget as
    /// `book_page_statement_count_is_fixed`, and the reason is the same: the
    /// number is identical on every machine and cannot flake.
    ///
    /// Only the table-shaped reads are seeded. The pref-shaped ones are
    /// deliberately left at their defaults: setting them changes *which*
    /// statements fire, not how many rows they return — the watch-rules
    /// reader takes a second legacy-pref look when no rules are saved, and
    /// global prefs (theme, sources, the Google key) fall through the
    /// prefs.json file first. Seeding rows is what tests the property:
    /// neither list grows with the library.
    #[test]
    fn settings_statement_count_is_fixed() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let n_fresh = svc.catalog().count_queries(|| svc.settings());
        assert!(n_fresh > 0);

        let cat = svc.catalog();
        cat.insert_dictionary("Test Dict", Some("en"), 100)
            .expect("seed dictionary");
        cat.create_shelf("A shelf", ShelfKind::Manual, "", "")
            .expect("seed shelf");

        let n_full = svc.catalog().count_queries(|| svc.settings());
        assert_eq!(
            n_fresh, n_full,
            "settings must be a fixed set of statements: {n_fresh} fresh vs {n_full} with a dictionary and a shelf installed"
        );

        // And the snapshot carries the seeded rows plus the pref defaults.
        let snap = svc.settings();
        assert_eq!(snap.dicts.len(), 1);
        assert_eq!(snap.dicts[0].name, "Test Dict");
        assert_eq!(snap.shelves.len(), 1);
        assert_eq!(snap.shelves[0].name, "A shelf");
        assert_eq!(snap.theme_id, crate::theme::DEFAULT.id);
        assert!(snap.source_open_library);
        assert!(snap.source_google_books);
        assert!(snap.clean_on_import);
        assert!(snap.writeback_enabled);
        assert_eq!(snap.google_books_key, "");
        assert!(snap.watch_rules.is_empty());
    }

    /// 7.1 step 2c: the author page snapshot's cost does not grow with the
    /// library — one book by an author costs exactly what three do.
    ///
    /// Deliberately not empty-vs-busy (pitfalls §51's second face): an
    /// author with no books skips `hydrate_books`' batched tag query
    /// entirely, so the empty case is *structurally* cheaper — one batched
    /// statement fewer, the good direction. The property worth pinning is
    /// that the count is flat in the number of books.
    #[test]
    fn author_page_statement_count_is_fixed() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));

        seed(svc.catalog(), "First", &[]);
        let n_one = svc.catalog().count_queries(|| svc.author_page("An Author"));

        seed(svc.catalog(), "Second", &[]);
        seed(svc.catalog(), "Third", &[]);
        let n_three = svc.catalog().count_queries(|| svc.author_page("An Author"));

        assert_eq!(
            n_one, n_three,
            "author_page must not grow with the book count: {n_one} statements for one book vs {n_three} for three"
        );

        // And the snapshot carries the books.
        let snap = svc.author_page("An Author");
        assert_eq!(snap.owned_books.len(), 3);
        // AuthorProfile is not PartialEq, so the emptiness is asked, not
        // compared.
        assert!(snap.profile.is_none());
    }

    #[test]
    fn author_page_groups_bibliography_with_owned_books() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let cat = svc.catalog();

        // Owned: series books 1 and 3 of "Alpha", one standalone the
        // bibliography never mentions.
        let one = seed(cat, "Alpha One", &[]);
        let three = seed(cat, "Alpha Three", &[]);
        seed(cat, "Loose Ends", &[]);
        cat.set_book_series(one, Some("Alpha"), 1.0).unwrap();
        cat.set_book_series(three, Some("Alpha"), 3.0).unwrap();

        let work = |title: &str, series: &str, position: &str, year: i64| crate::db::AuthorWork {
            title: title.into(),
            series_name: series.into(),
            series_position: position.into(),
            first_publish_year: Some(year),
            ..crate::db::AuthorWork::default()
        };
        let profile = AuthorProfile {
            canonical_name: "An Author".into(),
            normalized_name: crate::author::normalize_author_name("An Author"),
            works: vec![
                work("Alpha One", "Alpha", "1", 2001),
                work("Alpha Two", "Alpha", "2", 2003),
                // A duplicate edition of the same work collapses.
                work("Alpha Two", "Alpha", "2", 2004),
                work("Alpha Three", "Alpha", "3", 2005),
                work("Solo Flight", "", "", 1999),
            ],
            ..AuthorProfile::default()
        };
        cat.upsert_author_profile(&profile).unwrap();

        let snap = svc.author_page("An Author");
        // Standalone first, then the series (decision 106).
        assert_eq!(snap.groups.len(), 2);
        let standalone = &snap.groups[0];
        assert_eq!(standalone.title, "Standalone works");
        assert!(standalone.series.is_none());
        // Year order: the bibliography's Solo Flight (missing), then the
        // unlisted owned book.
        assert_eq!(standalone.entries.len(), 2);
        assert_eq!(standalone.entries[0].title, "Solo Flight");
        assert!(standalone.entries[0].book.is_none());
        assert_eq!(standalone.entries[1].title, "Loose Ends");
        assert!(standalone.entries[1].book.is_some());

        let alpha = &snap.groups[1];
        assert_eq!(alpha.series.as_deref(), Some("Alpha"));
        assert_eq!(alpha.entries.len(), 3, "duplicate edition collapsed");
        assert_eq!(
            alpha
                .entries
                .iter()
                .map(|e| e.book.is_some())
                .collect::<Vec<_>>(),
            vec![true, false, true],
            "position order with the middle book missing"
        );
        assert_eq!(alpha.entries[1].year, Some(2003));
        assert_eq!(alpha.entries[2].position, 3.0);
    }

    #[test]
    fn authors_index_folds_spellings_and_counts_books() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let cat = svc.catalog();
        let seed_author = |uuid: &str, title: &str, authors: &str, series: Option<&str>| {
            cat.insert_book(
                uuid,
                title,
                authors,
                series,
                "",
                BookFormat::Epub,
                &format!("{uuid}.epub"),
                &format!("hash-{uuid}"),
                None,
                &[],
            )
            .unwrap()
        };

        let first = seed_author(
            "idx-1",
            "A Wizard of Earthsea",
            "Le Guin, Ursula K.",
            Some("Earthsea"),
        );
        seed_author(
            "idx-2",
            "The Tombs of Atuan",
            "Ursula K. Le Guin",
            Some("Earthsea"),
        );
        seed_author("idx-3", "The Dispossessed", "Ursula Le Guin", None);
        seed_author(
            "idx-4",
            "A Shared Book",
            "Ursula K. Le Guin; Another Author",
            None,
        );
        cat.set_book_finished(first, true).unwrap();

        let profile = AuthorProfile {
            canonical_name: "Ursula K. Le Guin".into(),
            sort_name: "Le Guin, Ursula K.".into(),
            normalized_name: crate::author::normalize_author_name("Ursula K. Le Guin"),
            birth_date: "21 October 1929".into(),
            death_date: "22 January 2018".into(),
            work_count: 12,
            works: vec![
                crate::db::AuthorWork {
                    title: "A Wizard of Earthsea".into(),
                    ..crate::db::AuthorWork::default()
                },
                crate::db::AuthorWork {
                    title: "The Tombs of Atuan".into(),
                    ..crate::db::AuthorWork::default()
                },
            ],
            aliases: vec!["Ursula Le Guin".into()],
            ..AuthorProfile::default()
        };
        cat.upsert_author_profile(&profile).unwrap();
        // A cached profile with no books left is not an author *in
        // the library* and stays off the index.
        let ghost = AuthorProfile {
            canonical_name: "Ghost Author".into(),
            normalized_name: crate::author::normalize_author_name("Ghost Author"),
            ..AuthorProfile::default()
        };
        cat.upsert_author_profile(&ghost).unwrap();

        let snap = svc.authors();
        assert!(snap.errors.is_empty(), "{:?}", snap.errors);
        assert_eq!(snap.entries.len(), 2);
        assert_eq!(snap.entries[0].name, "Another Author");
        assert_eq!(snap.entries[0].book_count, 1);

        let le_guin = &snap.entries[1];
        assert_eq!(le_guin.name, "Ursula K. Le Guin");
        assert_eq!(le_guin.book_count, 4);
        assert_eq!(le_guin.read_count, 1);
        assert_eq!(le_guin.series_count, 1);
        assert_eq!(le_guin.known_works, Some(12));
        assert_eq!(le_guin.birth_year, Some(1929));
        assert_eq!(le_guin.death_year, Some(2018));
        assert!(le_guin.aliases.iter().any(|a| a == "Ursula Le Guin"));
    }

    #[test]
    fn series_page_merges_the_listing_with_owned_books() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let cat = svc.catalog();
        let one = seed(cat, "Alpha One", &[]);
        let two = seed(cat, "Alpha Two", &[]);
        cat.set_book_series(one, Some("Alpha"), 1.0).unwrap();
        // A fractional owned position outranks the listing's slot for the
        // same work: the listing puts this third, the book says 2.5.
        cat.set_book_series(two, Some("Alpha"), 2.5).unwrap();
        cat.set_book_finished(one, true).unwrap();
        cat.set_reading_progress(two, 8, 0.5, 10).unwrap();
        {
            let conn = cat.conn();
            conn.execute(
                "UPDATE books SET published = 'June 2001', progress = 42 WHERE id = ?1",
                rusqlite::params![two],
            )
            .unwrap();
        }

        let works = vec![
            crate::db::SeriesWork {
                title: "Alpha One".into(),
                key: String::new(),
                cover_path: String::new(),
                year: 1998,
                author: "An Author".into(),
            },
            crate::db::SeriesWork {
                title: "Alpha Missing".into(),
                key: String::new(),
                cover_path: String::new(),
                year: 2000,
                author: "An Author".into(),
            },
            crate::db::SeriesWork {
                title: "Alpha Two".into(),
                key: String::new(),
                cover_path: String::new(),
                year: 0,
                author: "An Author".into(),
            },
        ];
        cat.upsert_series_cache(
            &crate::db::series_key("Alpha", "An Author"),
            "openlibrary",
            &works,
        )
        .unwrap();

        let snap = svc.series_page("Alpha", "An Author");
        assert!(snap.errors.is_empty(), "errors: {:?}", snap.errors);
        assert_eq!(snap.listed_total, Some(3));
        let titles: Vec<&str> = snap.entries.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["Alpha One", "Alpha Missing", "Alpha Two"]);
        assert_eq!(snap.entries[2].position, 2.5);
        assert!(snap.entries[0].finished_at.is_some());
        assert!(snap.entries[1].book.is_none(), "the unowned work stays a gap");
        assert_eq!(snap.entries[2].chapter_index, Some(8));
        assert_eq!(snap.entries[2].year, Some(2001));
        assert!(!snap.view_list);

        cat.set_pref(&snap.view_pref_key, "list");
        assert!(svc.series_page("Alpha", "An Author").view_list);
    }

    fn seed(cat: &Catalog, title: &str, tags: &[&str]) -> i64 {
        let tags: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
        cat.insert_book(
            &format!("uuid-{title}"),
            title,
            "An Author",
            None,
            "",
            BookFormat::Epub,
            "book.epub",
            &format!("hash-{title}"),
            None,
            &tags,
        )
        .expect("seed book")
    }

    /// The pickers' one rule, pinned in the service layer: comics appear as
    /// one row per series — never one per chapter — and a series row is
    /// ticked when any of its chapters is on the list/shelf. Covers both
    /// registration kinds (relational chapters and the heuristic fallback).
    #[test]
    fn pickers_collapse_comics_and_tick_through_any_chapter() {
        let cat = Catalog::open_in_memory().unwrap();
        let epub = seed(&cat, "Plain", &[]);
        // Registered series: two chapters filed under one comic_series.
        let c1 = seed(&cat, "Horimiya - c001", &[]);
        let c2 = seed(&cat, "Horimiya - c002", &[]);
        // Mark them as comics in place (the seed helper files epubs).
        set_comic_format(&cat, c1, Some("Horimiya"));
        set_comic_format(&cat, c2, Some("Horimiya"));
        let series_id = cat
            .get_or_create_comic_series("Horimiya", Some("HERO"), None)
            .unwrap();
        cat.add_comic_chapter(series_id, c1, 1.0, None, "Page 1").unwrap();
        cat.add_comic_chapter(series_id, c2, 2.0, None, "Page 2").unwrap();
        // Unregistered pair: heuristic peers via the series column.
        let h1 = seed(&cat, "Naruto - c001", &[]);
        let h2 = seed(&cat, "Naruto - c002", &[]);
        set_comic_format(&cat, h1, Some("Naruto"));
        set_comic_format(&cat, h2, Some("Naruto"));

        let svc = LibraryService::new(Arc::new(cat));

        // One chapter queued is enough to tick the whole series row.
        svc.catalog().add_to_reading_list(c2).unwrap();
        let rows = svc.reading_list_picker("");
        assert_eq!(
            rows.len(),
            3,
            "epub + one row per comic series, never per chapter: {rows:?}"
        );
        let horimiya = rows.iter().find(|r| r.title == "Horimiya").unwrap();
        assert!(horimiya.ticked, "a queued chapter ticks the series row");
        assert!(
            !rows.iter().any(|r| r.title.contains("c00")),
            "no chapter titles may appear as rows"
        );
        assert!(!rows.iter().find(|r| r.id == epub).unwrap().ticked);

        // The same rule on shelves, through the other picker.
        let shelf_id = cat_shelf(&svc);
        svc.catalog().add_book_to_shelf(shelf_id, c1).unwrap();
        let rows = svc.shelf_picker(shelf_id, "");
        assert_eq!(rows.len(), 3);
        assert!(
            rows.iter().find(|r| r.title == "Horimiya").unwrap().ticked,
            "a chapter on the shelf ticks the series row"
        );
    }

    /// `seed` files epubs; a comic needs the format and series columns set.
    fn set_comic_format(cat: &Catalog, id: i64, series: Option<&str>) {
        let conn = cat.conn();
        conn.execute(
            "UPDATE books SET format = 'CBZ', series = ?2 WHERE id = ?1",
            rusqlite::params![id, series],
        )
        .expect("set comic format");
    }

    fn cat_shelf(svc: &LibraryService) -> i64 {
        svc.catalog()
            .create_shelf("Picker", ShelfKind::Manual, "", "")
            .expect("create shelf")
    }

    /// A `Book` with only the fields the continue-row rule reads. Building one
    /// by hand keeps those tests free of the database entirely.
    fn book(title: &str, progress: u8) -> Book {
        Book {
            id: 0,
            uuid: String::new(),
            title: title.into(),
            authors: String::new(),
            series: None,
            description: String::new(),
            format: BookFormat::Epub,
            file_name: String::new(),
            file_hash: String::new(),
            cover_name: None,
            added_at: String::new(),
            progress,
            rating: 0,
            publisher: String::new(),
            published: String::new(),
            series_index: 0.0,
            tags: Vec::new(),
            cover_path: None,
            file_path: std::path::PathBuf::new(),
        }
    }

    #[test]
    fn home_reports_counts_and_recent_books() {
        let cat = Catalog::open_in_memory().unwrap();
        seed(&cat, "Dune", &["scifi"]);
        seed(&cat, "Emma", &["classic"]);
        let svc = LibraryService::new(Arc::new(cat));

        let snap = svc.home();
        assert!(
            snap.errors.is_empty(),
            "unexpected errors: {:?}",
            snap.errors
        );
        assert_eq!(snap.stats.total_books, 2);
        assert_eq!(snap.recent.len(), 2);
    }

    #[test]
    fn history_and_lookup_history_return_rows_without_errors() {
        // Both pages used `unwrap_or_default()`, so a broken read looked like
        // "nothing has happened yet" -- indistinguishable from a new install.
        let cat = Catalog::open_in_memory().unwrap();
        let book = seed(&cat, "Dune", &[]);
        cat.log_event(book, EventKind::Opened, "")
            .expect("log event");
        cat.log_dict_lookup("melange", Some(book), None, None, true)
            .expect("log lookup");
        let svc = LibraryService::new(Arc::new(cat));

        let hist = svc.history(None, "", 50);
        assert_eq!(hist.events.len(), 1);
        assert!(hist.errors.is_empty());

        let looks = svc.lookup_history("", 50);
        assert_eq!(looks.lookups.len(), 1);
        assert!(looks.errors.is_empty());
    }

    #[test]
    fn reader_returns_the_book_with_its_marks() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let id = seed(svc.catalog(), "Dune", &[]);
        svc.catalog()
            .insert_annotation(id, "highlight", 0, "/1", 0, "/1", 9, "", "solid", "the spice", "")
            .expect("insert annotation");
        svc.catalog()
            .insert_saved_word("melange", "a spice", None, Some(id), None, None)
            .expect("insert saved word");

        let snap = svc.reader(id);
        assert!(snap.errors.is_empty(), "{:?}", snap.errors);
        assert_eq!(snap.book.map(|b| b.title), Some("Dune".to_string()));
        assert_eq!(snap.annotations.len(), 1);
        assert_eq!(snap.saved_words.len(), 1);
        assert!(snap.bookmarks.is_empty());
    }

    #[test]
    fn opening_a_book_with_no_marks_is_not_an_error() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let id = seed(svc.catalog(), "Emma", &[]);

        let snap = svc.reader(id);
        assert!(
            snap.errors.is_empty(),
            "a fresh book is not a failed read: {:?}",
            snap.errors
        );
        assert!(snap.annotations.is_empty());
        assert!(snap.bookmarks.is_empty());
        assert!(snap.saved_words.is_empty());
    }

    #[test]
    fn book_stats_are_zero_for_an_unread_book_without_erroring() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let id = seed(svc.catalog(), "Dune", &[]);

        let snap = svc.book_stats(id, 7, 3);
        assert!(
            snap.errors.is_empty(),
            "a never-opened book is not a failed read: {:?}",
            snap.errors
        );
        assert_eq!(snap.total_seconds, 0);
        assert_eq!(snap.session_count, 0);
        assert_eq!(snap.chapter_index, 0);
        assert!(snap.recent_sessions.is_empty());
        assert!(snap.finished_at.is_none());
        assert!(snap.first_opened.is_none());
    }

    #[test]
    fn book_stats_pick_up_a_finished_date() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let id = seed(svc.catalog(), "Emma", &[]);
        svc.catalog()
            .set_book_finished(id, true)
            .expect("mark finished");

        let snap = svc.book_stats(id, 7, 3);
        assert!(snap.errors.is_empty(), "{:?}", snap.errors);
        assert!(snap.finished_at.is_some(), "finished date must come back");
    }

    #[test]
    fn shelf_detail_returns_the_shelf_and_its_books() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let book = seed(svc.catalog(), "Dune", &[]);
        let shelf_id = svc
            .catalog()
            .create_shelf("To read", ShelfKind::Manual, "", "")
            .expect("create shelf");
        svc.catalog()
            .add_book_to_shelf(shelf_id, book)
            .expect("add to shelf");

        let snap = svc.shelf_detail(shelf_id, SortKey::Added, "");
        assert!(snap.errors.is_empty(), "{:?}", snap.errors);
        assert_eq!(snap.shelf.map(|s| s.name), Some("To read".to_string()));
        assert_eq!(snap.books.len(), 1);
        assert_eq!(snap.books[0].title, "Dune");
    }

    #[test]
    fn a_missing_shelf_yields_no_books_and_no_error() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let snap = svc.shelf_detail(9999, SortKey::Added, "");
        assert!(snap.shelf.is_none());
        assert!(snap.books.is_empty());
        assert!(
            snap.errors.is_empty(),
            "a deleted shelf is not a failed read: {:?}",
            snap.errors
        );
    }

    #[test]
    fn book_detail_separates_a_missing_book_from_a_failed_read() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let id = seed(svc.catalog(), "Dune", &["scifi"]);

        let snap = svc.book_detail(id);
        assert!(snap.errors.is_empty(), "{:?}", snap.errors);
        assert_eq!(snap.book.map(|b| b.title), Some("Dune".to_string()));
        assert!(!snap.in_reading_list);
        assert!(!snap.finished);

        // A book that does not exist is `None` with no error: the panel says
        // so honestly, and a real DB failure stays distinguishable from it.
        let gone = svc.book_detail(9999);
        assert!(gone.book.is_none());
        assert!(
            gone.errors.is_empty(),
            "a missing book is not a failed read: {:?}",
            gone.errors
        );
    }

    #[test]
    fn book_detail_reflects_reading_list_and_finished_flags() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let id = seed(svc.catalog(), "Emma", &[]);

        svc.catalog()
            .add_to_reading_list(id)
            .expect("add to reading list");
        let queued = svc.book_detail(id);
        assert!(queued.errors.is_empty(), "{:?}", queued.errors);
        assert!(queued.in_reading_list);
        assert!(!queued.finished);

        // Finishing a book deliberately drops it off the reading list, so the
        // two flags are not independent — the snapshot must show that, not a
        // stale "still queued".
        svc.catalog()
            .set_book_finished(id, true)
            .expect("mark finished");
        let done = svc.book_detail(id);
        assert!(done.errors.is_empty(), "{:?}", done.errors);
        assert!(done.finished);
        assert!(
            !done.in_reading_list,
            "set_book_finished removes the book from the reading list"
        );
    }

    #[test]
    fn word_counts_are_read_not_guessed() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let mut ids = Vec::new();
        for w in ["ephemeral", "petrichor", "susurrus"] {
            ids.push(
                svc.catalog()
                    .insert_saved_word(w, "a definition", None, None, None, None)
                    .expect("insert saved word"),
            );
        }
        svc.catalog()
            .set_saved_word_known(ids[0], true)
            .expect("mark known");

        let snap = svc.words("", None);
        assert!(snap.errors.is_empty(), "{:?}", snap.errors);
        assert_eq!(snap.words.len(), 3);
        assert_eq!(snap.total, 3);
        // The old code swallowed this read and showed 0 on failure.
        assert_eq!(snap.known, 1);

        // Filtering narrows the list but the header counts stay whole-table,
        // which is what "3 of 1 known" style wording needs.
        let known_only = svc.words("", Some(true));
        assert_eq!(known_only.words.len(), 1);
        assert_eq!(known_only.total, 3);
        assert_eq!(known_only.known, 1);
        assert!(known_only.errors.is_empty());
    }

    #[test]
    fn an_empty_vocabulary_is_not_an_error() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let snap = svc.words("", None);
        assert!(snap.words.is_empty());
        assert_eq!(snap.total, 0);
        assert_eq!(snap.known, 0);
        assert!(snap.errors.is_empty(), "{:?}", snap.errors);
    }

    #[test]
    fn quotes_pair_each_annotation_with_its_book_in_one_batch() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let id = seed(svc.catalog(), "Dune", &["scifi"]);
        for excerpt in ["the spice must flow", "fear is the mind-killer"] {
            svc.catalog()
                .insert_annotation(id, "quote", 0, "/1", 0, "/1", 5, "", "solid", excerpt, "")
                .expect("insert annotation");
        }

        let snap = svc.quotes("");
        assert!(snap.errors.is_empty(), "{:?}", snap.errors);
        assert_eq!(snap.quotes.len(), 2);
        // Every quote resolved to its book: this is what the per-quote
        // `get_book` loop used to do with 2N + 1 queries.
        for (_, book) in &snap.quotes {
            assert_eq!(book.as_ref().map(|b| b.title.as_str()), Some("Dune"));
        }

        // Search narrows the same way the page expects.
        let hit = svc.quotes("spice");
        assert_eq!(hit.quotes.len(), 1);
        assert!(hit.errors.is_empty());

        // A search matching nothing is not a failure.
        let miss = svc.quotes("zzzz-no-such-text");
        assert!(miss.quotes.is_empty());
        assert!(miss.errors.is_empty(), "no matches is not an error");
    }

    #[test]
    fn there_are_no_quotes_at_all_is_not_an_error() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let snap = svc.quotes("");
        assert!(snap.quotes.is_empty());
        assert!(
            snap.errors.is_empty(),
            "an empty quote list is not a failure: {:?}",
            snap.errors
        );
    }

    #[test]
    fn dashboard_reads_every_strip_without_errors() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let id = seed(svc.catalog(), "Dune", &["scifi"]);
        svc.catalog()
            .log_event(id, EventKind::Opened, "")
            .expect("log event");

        let snap = svc.dashboard(4);
        assert!(
            snap.errors.is_empty(),
            "healthy dashboard reported errors: {:?}",
            snap.errors
        );
        assert_eq!(snap.stats.total_books, 1);
        // The raw events/sessions left the snapshot when the feed moved
        // in (2b): the dashboard's contract is the merged feed itself.
        assert_eq!(snap.feed.len(), 1);
        // The goal counters are infallible, so they always have a value.
        assert!(snap.goal >= 0);
        assert!(snap.finished_this_year >= 0);
    }

    #[test]
    fn an_empty_dashboard_is_not_an_error() {
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));
        let snap = svc.dashboard(4);
        assert!(snap.errors.is_empty(), "{:?}", snap.errors);
        assert_eq!(snap.stats.total_books, 0);
        assert!(snap.recently_opened.is_empty());
        assert!(snap.feed.is_empty());
    }

    #[test]
    fn an_empty_log_is_not_an_error() {
        // A fresh install has no history; that must stay silent, not toast.
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));

        assert!(svc.history(None, "", 50).events.is_empty());
        assert!(svc.history(None, "", 50).errors.is_empty());
        assert!(svc.lookup_history("", 50).lookups.is_empty());
        assert!(svc.lookup_history("", 50).errors.is_empty());
    }

    #[test]
    fn all_books_respects_the_search_query() {
        // The page distinguishes "no books at all" from "nothing matched", so
        // the snapshot has to actually filter rather than always return all.
        let cat = Catalog::open_in_memory().unwrap();
        seed(&cat, "Dune", &[]);
        seed(&cat, "Emma", &[]);
        let svc = LibraryService::new(Arc::new(cat));

        let all = svc.all_books(SortKey::Title, "");
        assert_eq!(all.books.len(), 2);
        assert!(all.errors.is_empty());

        let hit = svc.all_books(SortKey::Title, "Dune");
        assert_eq!(hit.books.len(), 1);
        assert_eq!(hit.books[0].title, "Dune");

        let miss = svc.all_books(SortKey::Title, "zzzz");
        assert!(miss.books.is_empty());
        assert!(miss.errors.is_empty(), "no match is not a failure");
    }

    #[test]
    fn advanced_search_syntax_filters_correctly() {
        let cat = Catalog::open_in_memory().unwrap();
        let id1 = cat
            .insert_book(
                "uuid-dune",
                "Dune",
                "Frank Herbert",
                Some("Dune Chronicles"),
                "Sci-fi classic",
                BookFormat::Epub,
                "dune.epub",
                "hash-dune",
                None,
                &["sci-fi".into(), "classic".into()],
            )
            .unwrap();
        cat.set_book_rating(id1, 8).unwrap();
        cat.set_book_finished(id1, true).unwrap();

        let id2 = cat
            .insert_book(
                "uuid-mistborn",
                "Mistborn",
                "Brandon Sanderson",
                Some("Mistborn"),
                "Epic fantasy",
                BookFormat::Epub,
                "mistborn.epub",
                "hash-mistborn",
                None,
                &["fantasy".into()],
            )
            .unwrap();
        cat.set_book_rating(id2, 10).unwrap();

        let svc = LibraryService::new(Arc::new(cat));

        // Filter by tag
        let res = svc.all_books(SortKey::Title, "tag:fantasy");
        assert_eq!(res.books.len(), 1);
        assert_eq!(res.books[0].title, "Mistborn");

        // Filter by author
        let res = svc.all_books(SortKey::Title, "author:Herbert");
        assert_eq!(res.books.len(), 1);
        assert_eq!(res.books[0].title, "Dune");

        // Filter by status
        let res = svc.all_books(SortKey::Title, "status:finished");
        assert_eq!(res.books.len(), 1);
        assert_eq!(res.books[0].title, "Dune");

        let res = svc.all_books(SortKey::Title, "status:unread");
        assert_eq!(res.books.len(), 1);
        assert_eq!(res.books[0].title, "Mistborn");

        // Filter by rating
        let res = svc.all_books(SortKey::Title, "rating:>4");
        assert_eq!(res.books.len(), 1);
        assert_eq!(res.books[0].title, "Mistborn");

        // Combined filter
        let res = svc.all_books(SortKey::Title, "tag:fantasy author:Sanderson status:unread rating:>=5");
        assert_eq!(res.books.len(), 1);
        assert_eq!(res.books[0].title, "Mistborn");
    }

    #[test]
    fn shelves_are_returned_and_an_empty_grid_is_not_an_error() {
        // Same defect as the reading list: `unwrap_or_default()` made a broken
        // read look like "you have no shelves yet".
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));

        let snap = svc.shelves();
        assert!(snap.shelves.is_empty());
        assert!(snap.errors.is_empty(), "no shelves is not a failure");

        svc.catalog()
            .create_shelf("To read", ShelfKind::Manual, "", "")
            .expect("create shelf");
        let snap = svc.shelves();
        assert_eq!(snap.shelves.len(), 1);
        assert_eq!(snap.shelves[0].name, "To read");
        assert!(snap.errors.is_empty());
    }

    #[test]
    fn shelf_detail_shows_a_comic_series_as_one_card() {
        // The owner's shelf report (2026-10-03): shelves show comics as
        // series, never individual chapters. The snapshot is what the page
        // renders, so the collapse is asserted here.
        let cat = Catalog::open_in_memory().unwrap();
        let svc = LibraryService::new(Arc::new(cat));

        let reg = svc
            .catalog()
            .insert_book(
                "reg-1", "Dune", "Frank Herbert", None, "", BookFormat::Epub,
                "dune.epub", "h-dune", None, &[],
            )
            .unwrap();
        let c1 = svc
            .catalog()
            .insert_book(
                "c-1", "Horimiya - c001", "HERO", None, "", BookFormat::Cbz,
                "horimiya_01.cbz", "h-c1", None, &[],
            )
            .unwrap();
        let c2 = svc
            .catalog()
            .insert_book(
                "c-2", "Horimiya - c002", "HERO", None, "", BookFormat::Cbz,
                "horimiya_02.cbz", "h-c2", None, &[],
            )
            .unwrap();
        let s_id = svc
            .catalog()
            .get_or_create_comic_series("Horimiya", Some("HERO"), None)
            .unwrap();
        svc.catalog().add_comic_chapter(s_id, c1, 1.0, None, "Page 1").unwrap();
        svc.catalog().add_comic_chapter(s_id, c2, 2.0, None, "Page 2").unwrap();

        let shelf_id = svc
            .catalog()
            .create_shelf("Comics", ShelfKind::Manual, "", "")
            .unwrap();
        svc.catalog().add_book_to_shelf(shelf_id, reg).unwrap();
        svc.catalog().add_book_to_shelf(shelf_id, c1).unwrap();
        svc.catalog().add_book_to_shelf(shelf_id, c2).unwrap();

        let snap = svc.shelf_detail(shelf_id, SortKey::Added, "");
        assert!(snap.errors.is_empty());
        assert_eq!(snap.books.len(), 2, "one epub card + one series card");
        let comic = snap
            .books
            .iter()
            .find(|b| b.series.as_deref() == Some("Horimiya"))
            .expect("the series card is present");
        assert_eq!(comic.title, "Horimiya");
    }

    #[test]
    fn reading_list_returns_the_queue_in_order() {
        // The page used to render `unwrap_or_default()`, so a failed read and a
        // genuinely empty queue looked identical. The snapshot separates them:
        // entries carry the order, errors carry the reason.
        let cat = Catalog::open_in_memory().unwrap();
        let first = seed(&cat, "Dune", &[]);
        let second = seed(&cat, "Emma", &[]);
        cat.add_to_reading_list(first).expect("queue first");
        cat.add_to_reading_list(second).expect("queue second");
        let svc = LibraryService::new(Arc::new(cat));

        let snap = svc.reading_list();
        assert!(
            snap.errors.is_empty(),
            "unexpected errors: {:?}",
            snap.errors
        );
        assert_eq!(snap.entries.len(), 2);
        assert_eq!(snap.entries[0].book.title, "Dune");
    }

    #[test]
    fn reading_list_is_empty_without_an_error_when_nothing_is_queued() {
        // An empty queue is not a failure, and must not raise a toast.
        let cat = Catalog::open_in_memory().unwrap();
        seed(&cat, "Dune", &[]);
        let svc = LibraryService::new(Arc::new(cat));

        let snap = svc.reading_list();
        assert!(snap.entries.is_empty());
        assert!(snap.errors.is_empty(), "empty is not an error");
    }

    #[test]
    fn continue_row_falls_back_to_the_newest_book() {
        // Nothing opened and nothing part-read: the row would be empty, which
        // reads as "this app is broken" in a freshly imported library.
        let cat = Catalog::open_in_memory().unwrap();
        seed(&cat, "Dune", &[]);
        let svc = LibraryService::new(Arc::new(cat));

        let snap = svc.home();
        assert_eq!(snap.continue_reading.len(), 1);
        assert_eq!(snap.continue_reading[0].title, "Dune");
    }

    #[test]
    fn continue_row_prefers_in_progress_over_newest() {
        // `recent` is newest-first, so the untouched book leads the list; the
        // rule must still pick the one actually being read.
        let row = continue_row(Vec::new(), &[book("Untouched", 0), book("Half read", 42)]);
        assert_eq!(row.len(), 1);
        assert_eq!(row[0].title, "Half read");
    }

    #[test]
    fn continue_row_ignores_finished_books() {
        // 100% is done, not "continue". Without the upper bound the row would
        // keep offering a book the user has already finished.
        let row = continue_row(Vec::new(), &[book("Finished", 100)]);
        assert_eq!(row.len(), 1, "falls back to the newest book");
        assert_eq!(
            row[0].title, "Finished",
            "the fallback is allowed to show it, but not as 'in progress'"
        );

        let row = continue_row(Vec::new(), &[book("Finished", 100), book("Reading", 10)]);
        assert_eq!(row.len(), 1);
        assert_eq!(row[0].title, "Reading");
    }

    #[test]
    fn continue_row_keeps_recently_opened_when_present() {
        let row = continue_row(
            vec![book("Opened last night", 5)],
            &[book("Newer import", 0)],
        );
        assert_eq!(row.len(), 1);
        assert_eq!(row[0].title, "Opened last night");
    }

    #[test]
    fn tags_snapshot_counts_books_per_tag() {
        let cat = Catalog::open_in_memory().unwrap();
        seed(&cat, "Dune", &["scifi"]);
        seed(&cat, "Neuromancer", &["scifi"]);
        seed(&cat, "Emma", &["classic"]);
        let svc = LibraryService::new(Arc::new(cat));

        let snap = svc.tags();
        assert!(snap.errors.is_empty());
        let scifi = snap
            .tags
            .iter()
            .find(|(name, _)| name == "scifi")
            .expect("scifi tag present");
        assert_eq!(scifi.1, 2);
    }

    #[test]
    fn tag_books_returns_only_that_tag() {
        let cat = Catalog::open_in_memory().unwrap();
        seed(&cat, "Dune", &["scifi"]);
        seed(&cat, "Emma", &["classic"]);
        let svc = LibraryService::new(Arc::new(cat));

        let snap = svc.tag_books("scifi", SortKey::Title);
        assert!(snap.errors.is_empty());
        assert_eq!(snap.books.len(), 1);
        assert_eq!(snap.books[0].title, "Dune");
    }

    #[test]
    fn analytics_snapshot_reads_the_goal_back() {
        let cat = Catalog::open_in_memory().unwrap();
        cat.set_reading_goal(24);
        let svc = LibraryService::new(Arc::new(cat));

        let snap = svc.analytics();
        assert!(snap.errors.is_empty());
        assert_eq!(snap.goal, 24);
    }

    #[test]
    fn a_failed_read_is_recorded_rather_than_silently_empty() {
        // The whole point of the error policy: the page can say "database
        // error", where before every page turned this into an empty list.
        let mut errors = Errors::new();
        let value: Vec<Book> = take(
            Err(crate::db::DbError::Sqlite(
                rusqlite::Error::QueryReturnedNoRows,
            )),
            "recent books",
            &mut errors,
        );
        assert!(value.is_empty());
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].starts_with("recent books:"),
            "the message must name the query: {:?}",
            errors[0]
        );
    }
}
