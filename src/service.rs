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
    Annotation, Catalog, DictLookup, EventKind, LibrarySession, LibraryStats, QuoteRef,
    ReadingBookmark, ReadingEvent, ReadingListEntry, SavedWord, SessionRow, Shelf, SortKey,
};
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

/// The book page's reading-stats and timeline panel.
///
/// Six reads that were each swallowed, so a database problem drew a page
/// saying you had never read the book.
#[derive(Debug, Default)]
pub struct BookStatsSnapshot {
    pub total_seconds: i64,
    pub session_count: i64,
    pub chapter_index: usize,
    pub seconds_by_day: Vec<(String, i64)>,
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
    /// Fallback for the continue strip when nothing has been opened yet.
    pub recent: Vec<Book>,
    pub quotes: Vec<(Annotation, QuoteRef)>,
    pub words: Vec<SavedWord>,
    pub lookups: Vec<DictLookup>,
    pub events: Vec<ReadingEvent>,
    pub sessions: Vec<LibrarySession>,
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
        let mut errors = Errors::new();
        let shelf: Option<Shelf> = take(self.catalog.get_shelf(shelf_id), "shelf", &mut errors);
        let books = match &shelf {
            Some(s) => take(
                self.catalog.shelf_books(s, sort, query),
                "books on this shelf",
                &mut errors,
            ),
            None => Vec::new(),
        };
        ShelfDetailSnapshot {
            shelf,
            books,
            errors,
        }
    }

    /// Everything the book float panel shows, in one call.
    pub fn book_detail(&self, book_id: i64) -> BookDetailSnapshot {
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

    /// Vocabulary matching `query` and `known`, plus the header counts.
    ///
    /// The counts deliberately cover the whole table, not just the page's
    /// 500-row display cap, so the header stays true for large vocabularies.
    pub fn words(&self, query: &str, known: Option<bool>) -> WordsSnapshot {
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
        let mut errors = Errors::new();
        let cat = &self.catalog;
        DashboardSnapshot {
            stats: take(cat.library_stats(), "library stats", &mut errors),
            recently_opened: take(cat.recently_opened(6), "recently opened", &mut errors),
            recent: take(cat.recent_books(60), "recent books", &mut errors),
            quotes: take(cat.recent_quotes(2), "recent quotes", &mut errors),
            words: take(cat.list_saved_words("", None), "saved words", &mut errors),
            lookups: take(cat.list_dict_lookups("", 3), "recent lookups", &mut errors),
            events: take(
                cat.list_events(None, "", feed_limit * 2),
                "reading history",
                &mut errors,
            ),
            sessions: take(
                cat.recent_sessions(feed_limit * 2),
                "reading sessions",
                &mut errors,
            ),
            // Infallible by construction (they fall back to a default inside
            // the catalog), so they contribute no error rows.
            goal: cat.reading_goal(),
            finished_this_year: cat.finished_this_year(),
            errors,
        }
    }

    /// History page: reading events, optionally filtered by kind and text.
    pub fn history(&self, kind: Option<EventKind>, query: &str, limit: usize) -> HistorySnapshot {
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
    pub fn all_books(&self, sort: SortKey, query: &str) -> AllBooksSnapshot {
        let mut errors = Errors::new();
        AllBooksSnapshot {
            books: take(self.catalog.list_books(sort, query), "books", &mut errors),
            errors,
        }
    }

    /// Shelves page: every shelf, ordered as stored.
    pub fn shelves(&self) -> ShelvesSnapshot {
        let mut errors = Errors::new();
        ShelvesSnapshot {
            shelves: take(self.catalog.list_shelves(), "shelves", &mut errors),
            errors,
        }
    }

    /// Reading list page: the ordered queue.
    pub fn reading_list(&self) -> ReadingListSnapshot {
        let mut errors = Errors::new();
        ReadingListSnapshot {
            entries: take(
                self.catalog.list_reading_list(),
                "reading list",
                &mut errors,
            ),
            errors,
        }
    }

    pub fn tags(&self) -> TagsSnapshot {
        let mut errors = Errors::new();
        TagsSnapshot {
            tags: take(self.catalog.list_tags_with_counts(), "tags", &mut errors),
            errors,
        }
    }

    /// Books carrying `tag`, in `sort` order.
    pub fn tag_books(&self, tag: &str, sort: SortKey) -> TagBooksSnapshot {
        let mut errors = Errors::new();
        TagBooksSnapshot {
            books: take(
                self.catalog.books_with_tag(tag, sort),
                "books for tag",
                &mut errors,
            ),
            errors,
        }
    }
}

/// Unwrap a query result, or record why it failed and fall back to the empty
/// value. This is the one place the "a failed read shows as empty" policy
/// lives; it used to be repeated, silently, in every page.
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
        assert_send::<ReaderSnapshot>();
        // The service itself must be Send too, or it cannot be moved onto the
        // worker that would run those queries.
        assert_send::<LibraryService>();
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
            .insert_annotation(id, "highlight", 0, "/1", 0, "/1", 9, "", "the spice", "")
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
                .insert_annotation(id, "quote", 0, "/1", 0, "/1", 5, "", excerpt, "")
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
        assert_eq!(snap.events.len(), 1);
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
        assert!(snap.events.is_empty());
        assert!(snap.sessions.is_empty());
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
