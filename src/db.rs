//! SQLite catalog access — P1 books + P2 progress + P3 annotations & dictionary.

use crate::models::{Book, BookFormat};
use crate::paths::{book_dir, catalog_db, ensure_data_dirs};
use rusqlite::{params, Connection, OptionalExtension};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use thiserror::Error;

mod annotations;
mod authors;
mod dictionaries;
mod history;
mod lookup_history;
mod metadata;
mod prefs;
mod pronunciation;
mod series;
mod shelves;
mod stats;

pub use dictionaries::{
    likely_sense_index, EntryData, PhraseLookup, BUNDLED_ANTONYMS_NAME, BUNDLED_IDIOMS_NAME,
    BUNDLED_SYNONYMS_NAME, BUNDLED_WORDNET_NAME,
};
pub use history::{LibrarySession, SessionRow};
pub use lookup_history::DictLookup;
pub use pronunciation::pronunciation_for;
pub use series::{series_key, SeriesWork};

#[derive(Debug, Error)]
pub enum DbError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, DbError>;

/// Bumped whenever `migrate()` learns new tables/columns.
/// v3 = P3 annotations & dictionary · v4 = P4 shelves, lists, history, sessions
/// · v5 = ratings + reading goals · v6 = publisher/published/series index
/// · v7 = remembered metadata edits, keyed by file hash · v8 = reader bookmarks
/// · v9 = cached author profiles and aliases · v10 = series cache (Open Library)
/// · v11 = dictionary headword key (fold_key) + idx_dict_entries_key
/// · v12 = dictionary priority + combined_words merged store
/// · v13 = saved_words.known (review status for vocabulary tools)
/// · v14 = dict_lookups (append-only lookup history, Phase 10)
pub const SCHEMA_VERSION: i64 = 14;

/// Process-wide DB handle (GTK app is single-threaded for UI; imports run sync on UI for P1).
pub struct Catalog {
    conn: Mutex<Connection>,
    /// `library_stats()` runs ~20 aggregates and is called on Home, the
    /// Library dashboard and Analytics. The result is memoised against
    /// SQLite's total_changes() counter: any write anywhere bumps it, so the
    /// cache cannot go stale and no write path has to remember to clear it.
    stats_cache: Mutex<Option<(i64, LibraryStats)>>,
    /// Set while restoring remembered metadata.
    ///
    /// restore_overrides applies the saved values through the normal edit
    /// path, which re-stashes as it goes — and at that moment the book still
    /// has its freshly-imported cover, so the stash was being overwritten with
    /// the EPUB default before the real cover could be copied back.
    restoring: std::sync::atomic::AtomicBool,
}

// ---------------------------------------------------------------------------
// Domain structs for P3
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct Annotation {
    pub id: i64,
    pub book_id: i64,
    pub kind: String, // highlight | quote | note
    pub chapter_index: i64,
    pub start_path: String,
    pub start_offset: i64,
    pub end_path: String,
    pub end_offset: i64,
    pub color: String,
    pub text_excerpt: String,
    pub note: String,
    pub cfi: Option<String>,
    pub created_at: String,
    #[allow(dead_code)]
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct SavedWord {
    pub id: i64,
    pub word: String,
    pub definition: String,
    pub dict_name: Option<String>,
    pub book_id: Option<i64>,
    pub chapter_index: Option<i64>,
    pub context_text: Option<String>,
    pub created_at: String,
    /// Phase 7: review status — true once the word is marked as known.
    pub known: bool,
}

/// Book identity attached to a saved quote — the library dashboard renders
/// cover, book and author per quote card.
#[derive(Debug, Clone)]
pub struct QuoteRef {
    pub title: String,
    pub author: String,
    pub cover_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct ReadingBookmark {
    pub id: i64,
    #[allow(dead_code)]
    pub book_id: i64,
    pub chapter_index: i64,
    pub fraction: f64,
    pub label: String,
    #[allow(dead_code)]
    pub created_at: String,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct AuthorWork {
    pub title: String,
    pub first_publish_year: Option<i64>,
    pub subjects: Vec<String>,
    pub cover_id: Option<i64>,
    #[serde(default)]
    pub cover_file: Option<String>,
    pub work_key: String,
    #[serde(default)]
    pub series_key: String,
    #[serde(default)]
    pub series_name: String,
    #[serde(default)]
    pub series_position: String,
    #[serde(default)]
    pub rating_average: Option<f32>,
    #[serde(default)]
    pub rating_count: i64,
}

#[derive(Debug, Clone, Default)]
pub struct AuthorProfile {
    #[allow(dead_code)]
    pub id: i64,
    pub canonical_name: String,
    pub sort_name: String,
    pub normalized_name: String,
    pub bio: String,
    pub birth_date: String,
    pub death_date: String,
    pub top_work: String,
    pub top_subjects: Vec<String>,
    pub openlibrary_key: String,
    pub photo_file: Option<String>,
    pub photo_path: Option<PathBuf>,
    pub work_count: i64,
    pub works: Vec<AuthorWork>,
    pub aliases: Vec<String>,
    pub fetched_at: String,
    pub source_url: String,
}

#[derive(Debug, Clone)]
pub struct Dictionary {
    #[allow(dead_code)]
    pub id: i64,
    pub name: String,
    pub lang: Option<String>,
    pub entry_count: i64,
    #[allow(dead_code)]
    pub added_at: String,
    /// Merged-store priority (schema v12): lower numbers speak first.
    #[allow(dead_code)]
    pub priority: i64,
}

#[derive(Debug, Clone)]
pub struct DictEntry {
    #[allow(dead_code)]
    pub id: i64,
    #[allow(dead_code)]
    pub dict_id: i64,
    pub word: String,
    pub definition: String,
}

// ---------------------------------------------------------------------------
// Domain structs for P4
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShelfKind {
    Manual,
    Smart,
}

impl ShelfKind {
    pub fn as_str(self) -> &'static str {
        match self {
            ShelfKind::Manual => "manual",
            ShelfKind::Smart => "smart",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            ShelfKind::Manual => "Manual",
            ShelfKind::Smart => "Smart",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "smart" => ShelfKind::Smart,
            _ => ShelfKind::Manual,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Shelf {
    pub id: i64,
    pub name: String,
    pub kind: ShelfKind,
    pub description: String,
    /// JSON rule document; empty for manual shelves.
    pub rules: String,
    #[allow(dead_code)]
    pub position: i64,
    #[allow(dead_code)]
    pub created_at: String,
    #[allow(dead_code)]
    pub updated_at: String,
    /// Live count, filled in by `list_shelves`.
    pub book_count: usize,
}

impl Shelf {
    pub fn rule_set(&self) -> crate::shelf_rules::RuleSet {
        crate::shelf_rules::RuleSet::parse(&self.rules)
    }

    /// Card subtitle: rule summary for smart shelves, description otherwise.
    pub fn summary(&self) -> String {
        match self.kind {
            ShelfKind::Smart => self.rule_set().describe(),
            ShelfKind::Manual => {
                if self.description.trim().is_empty() {
                    "Hand-picked books".to_string()
                } else {
                    self.description.clone()
                }
            }
        }
    }
}

/// A row in the ordered to-be-read list.
#[derive(Debug, Clone)]
pub struct ReadingListEntry {
    pub book: Book,
    #[allow(dead_code)]
    pub position: i64,
    #[allow(dead_code)]
    pub note: String,
    #[allow(dead_code)]
    pub added_at: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EventKind {
    Opened,
    Finished,
    Unfinished,
    Imported,
}

impl EventKind {
    pub fn as_str(self) -> &'static str {
        match self {
            EventKind::Opened => "opened",
            EventKind::Finished => "finished",
            EventKind::Unfinished => "unfinished",
            EventKind::Imported => "imported",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            EventKind::Opened => "Opened",
            EventKind::Finished => "Finished",
            EventKind::Unfinished => "Marked unread",
            EventKind::Imported => "Imported",
        }
    }

    pub fn icon(self) -> &'static str {
        match self {
            EventKind::Opened => "document-open-recent-symbolic",
            EventKind::Finished => "object-select-symbolic",
            EventKind::Unfinished => "view-refresh-symbolic",
            EventKind::Imported => "list-add-symbolic",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "finished" => EventKind::Finished,
            "unfinished" => EventKind::Unfinished,
            "imported" => EventKind::Imported,
            _ => EventKind::Opened,
        }
    }
}

/// A history row joined with its book title for display.
#[derive(Debug, Clone)]
pub struct ReadingEvent {
    #[allow(dead_code)]
    pub id: i64,
    pub book_id: i64,
    pub kind: EventKind,
    pub at: String,
    pub detail: String,
    pub book_title: String,
    pub book_authors: String,
}

/// Aggregated numbers for the Analytics page.
#[derive(Debug, Clone, Default)]
pub struct LibraryStats {
    pub total_books: i64,
    pub finished: i64,
    pub reading: i64,
    pub unread: i64,
    pub highlights: i64,
    pub quotes: i64,
    pub saved_words: i64,
    pub shelves: i64,
    pub reading_list: i64,
    pub total_seconds: i64,
    pub seconds_last_7: i64,
    pub seconds_last_30: i64,
    pub sessions: i64,
    pub finished_last_30: i64,
    pub added_last_30: i64,
    pub current_streak_days: i64,
    pub longest_streak_days: i64,
    /// Mean minutes per day across days that had any reading (last 30 days).
    pub avg_minutes_per_active_day: i64,
    /// (label, count) — newest month last.
    pub added_by_month: Vec<(String, i64)>,
    /// (label, seconds) — last 14 days, oldest first.
    pub minutes_by_day: Vec<(String, i64)>,
    pub top_tags: Vec<(String, i64)>,
    pub top_authors: Vec<(String, i64)>,
    pub most_read: Vec<(String, i64)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HighlightColor {
    Yellow,
    Green,
    Blue,
    Pink,
    Orange,
}

impl HighlightColor {
    pub fn as_str(self) -> &'static str {
        match self {
            HighlightColor::Yellow => "yellow",
            HighlightColor::Green => "green",
            HighlightColor::Blue => "blue",
            HighlightColor::Pink => "pink",
            HighlightColor::Orange => "orange",
        }
    }

    pub fn from_str_lossy(s: &str) -> Self {
        match s.to_ascii_lowercase().as_str() {
            "green" => HighlightColor::Green,
            "blue" => HighlightColor::Blue,
            "pink" | "rose" => HighlightColor::Pink,
            "orange" => HighlightColor::Orange,
            _ => HighlightColor::Yellow,
        }
    }

    pub const ALL: &'static [HighlightColor] = &[
        HighlightColor::Yellow,
        HighlightColor::Green,
        HighlightColor::Blue,
        HighlightColor::Pink,
        HighlightColor::Orange,
    ];
}

// ---------------------------------------------------------------------------
// A0 step 7 — query counting (test-only)
//
// Why counting statements rather than timing them: the CI runner's wall-clock
// swings ~60% between identical runs (`startup_dicts`, byte-identical work,
// measured 2774 ms and 3740 ms on consecutive runs), so any time threshold
// loose enough not to flake is too loose to catch a real regression. A query
// count is an integer, is identical on every machine, and catches the failure
// that has actually bitten this repo three times (§16, §18): a loop that
// issues one query per row.
// ---------------------------------------------------------------------------

#[cfg(test)]
thread_local! {
    /// Statements executed on this thread while a counter is installed.
    ///
    /// Thread-local rather than a global counter because `cargo test` runs
    /// tests in parallel: a shared counter would make every query-count test
    /// depend on what the others happened to be doing, which is exactly the
    /// kind of flake this is meant to replace.
    static QUERY_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
impl Catalog {





    /// Count the SQL statements one call issues.
    ///
    /// `Connection::trace` takes a bare `fn(&str)`, not a closure, so the
    /// counter has to live outside it — hence the thread-local above rather
    /// than something captured. It also needs `&mut Connection`, which the
    /// `MutexGuard` provides through `DerefMut`.
    ///
    /// The hook is installed for the duration of `f` and removed afterwards,
    /// so a test that forgets to stop counting cannot affect a later one.
    pub(crate) fn count_queries<T>(&self, f: impl FnOnce() -> T) -> usize {
        fn bump(_sql: &str) {
            QUERY_COUNT.with(|c| c.set(c.get() + 1));
        }

        QUERY_COUNT.with(|c| c.set(0));
        // Each guard is scoped and dropped before `f` runs: `f` takes the same
        // lock, and holding it across the call would deadlock.
        {
            let mut conn = self.conn();
            conn.trace(Some(bump));
        }

        let _ = f();

        let n = QUERY_COUNT.with(|c| c.get());
        {
            let mut conn = self.conn();
            conn.trace(None);
        }
        n
    }
}

impl Catalog {
    /// Take the connection lock, recovering from poisoning.
    ///
    /// A panic on any thread while holding this lock used to make every later
    /// `expect("db lock")` abort the whole app. SQLite itself is unharmed by
    /// the panic, so carrying on with the data is strictly better than dying.
    pub(crate) fn conn(&self) -> std::sync::MutexGuard<'_, Connection> {
        self.conn.lock().unwrap_or_else(|poisoned| {
            eprintln!("kalam: recovered a poisoned database lock");
            poisoned.into_inner()
        })
    }

    pub fn open() -> Result<Self> {
        ensure_data_dirs()?;
        let path = catalog_db();
        let conn = Connection::open(&path)?;
        conn.execute_batch(
            "
            PRAGMA foreign_keys = ON;
            PRAGMA journal_mode = WAL;
            -- WAL already survives crashes; full fsync per commit is the
            -- single biggest cost on spinning disks and cheap SSDs.
            PRAGMA synchronous = NORMAL;
            -- 64 MB page cache and memory temp tables: the catalog is small
            -- enough to sit in RAM, which removes most read latency.
            PRAGMA cache_size = -64000;
            PRAGMA temp_store = MEMORY;
            PRAGMA mmap_size = 268435456;
            ",
        )?;
        let cat = Self {
            conn: Mutex::new(conn),
            stats_cache: Mutex::new(None),
            restoring: std::sync::atomic::AtomicBool::new(false),
        };
        cat.migrate()?;
        Ok(cat)
    }

    /// In-memory catalog with the full schema applied — tests only.
    #[cfg(test)]
    pub fn open_in_memory() -> Result<Self> {
        let conn = Connection::open_in_memory()?;
        conn.execute_batch("PRAGMA foreign_keys = ON;")?;
        let cat = Self {
            conn: Mutex::new(conn),
            stats_cache: Mutex::new(None),
            restoring: std::sync::atomic::AtomicBool::new(false),
        };
        cat.migrate()?;
        Ok(cat)
    }

    fn migrate(&self) -> Result<()> {
        let conn = self.conn();
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS schema_version (
                version INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS books (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                uuid          TEXT    NOT NULL UNIQUE,
                title         TEXT    NOT NULL,
                sort_title    TEXT    NOT NULL,
                authors       TEXT    NOT NULL DEFAULT '',
                series        TEXT,
                description   TEXT    NOT NULL DEFAULT '',
                format        TEXT    NOT NULL,
                file_name     TEXT    NOT NULL,
                file_hash     TEXT    NOT NULL,
                cover_name    TEXT,
                added_at      TEXT    NOT NULL,
                progress      INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS tags (
                id   INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE COLLATE NOCASE
            );

            CREATE TABLE IF NOT EXISTS book_tags (
                book_id INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                tag_id  INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
                PRIMARY KEY (book_id, tag_id)
            );

            CREATE INDEX IF NOT EXISTS idx_books_title ON books(sort_title);
            CREATE INDEX IF NOT EXISTS idx_books_added ON books(added_at);
            CREATE INDEX IF NOT EXISTS idx_books_hash ON books(file_hash);

            CREATE TABLE IF NOT EXISTS reading_progress (
                book_id       INTEGER PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
                chapter_index INTEGER NOT NULL DEFAULT 0,
                fraction      REAL    NOT NULL DEFAULT 0.0,
                updated_at    TEXT    NOT NULL
            );

            -- P3 tables
            CREATE TABLE IF NOT EXISTS annotations (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                book_id       INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                kind          TEXT    NOT NULL,
                chapter_index INTEGER NOT NULL,
                start_path    TEXT    NOT NULL,
                start_offset  INTEGER NOT NULL,
                end_path      TEXT    NOT NULL,
                end_offset    INTEGER NOT NULL,
                color         TEXT    NOT NULL DEFAULT 'yellow',
                text_excerpt  TEXT    NOT NULL DEFAULT '',
                note          TEXT    NOT NULL DEFAULT '',
                cfi           TEXT,
                created_at    TEXT    NOT NULL,
                updated_at    TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_annotations_book ON annotations(book_id);
            CREATE INDEX IF NOT EXISTS idx_annotations_book_chapter ON annotations(book_id, chapter_index);
            CREATE INDEX IF NOT EXISTS idx_annotations_kind ON annotations(kind);

            CREATE TABLE IF NOT EXISTS saved_words (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                word          TEXT    NOT NULL,
                definition    TEXT    NOT NULL,
                dict_name     TEXT,
                book_id       INTEGER REFERENCES books(id) ON DELETE SET NULL,
                chapter_index INTEGER,
                context_text  TEXT,
                created_at    TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_saved_words_word ON saved_words(word COLLATE NOCASE);

            CREATE TABLE IF NOT EXISTS reading_bookmarks (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                book_id       INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                chapter_index INTEGER NOT NULL,
                fraction      REAL    NOT NULL DEFAULT 0.0,
                label         TEXT    NOT NULL DEFAULT '',
                created_at    TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_reading_bookmarks_book
                ON reading_bookmarks(book_id, chapter_index, created_at DESC);

            CREATE TABLE IF NOT EXISTS dictionaries (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT    NOT NULL UNIQUE,
                lang        TEXT,
                entry_count INTEGER NOT NULL DEFAULT 0,
                added_at    TEXT    NOT NULL
            );

            CREATE TABLE IF NOT EXISTS dict_entries (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                dict_id    INTEGER NOT NULL REFERENCES dictionaries(id) ON DELETE CASCADE,
                word       TEXT    NOT NULL COLLATE NOCASE,
                definition TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_dict_entries_word ON dict_entries(word COLLATE NOCASE);
            CREATE INDEX IF NOT EXISTS idx_dict_entries_dict ON dict_entries(dict_id);

            -- v12: merged dictionary store. One row per headword key; the
            -- dictionary that speaks for a word is the one with the lowest
            -- priority (ties: lowest id). `senses` is a JSON array of that
            -- dictionary's definitions for the word. The reader searches
            -- only this table — never the raw entries — so a word always
            -- appears once.
            CREATE TABLE IF NOT EXISTS combined_words (
                key     TEXT    PRIMARY KEY COLLATE NOCASE,
                word    TEXT    NOT NULL,
                dict_id INTEGER NOT NULL REFERENCES dictionaries(id) ON DELETE CASCADE,
                senses  TEXT    NOT NULL
            );

            -- P4 tables
            CREATE TABLE IF NOT EXISTS shelves (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                name        TEXT    NOT NULL UNIQUE COLLATE NOCASE,
                kind        TEXT    NOT NULL DEFAULT 'manual',  -- manual | smart
                description TEXT    NOT NULL DEFAULT '',
                rules       TEXT    NOT NULL DEFAULT '',        -- JSON for smart shelves
                position    INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT    NOT NULL,
                updated_at  TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_shelves_position ON shelves(position);

            CREATE TABLE IF NOT EXISTS shelf_books (
                shelf_id INTEGER NOT NULL REFERENCES shelves(id) ON DELETE CASCADE,
                book_id  INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                position INTEGER NOT NULL DEFAULT 0,
                added_at TEXT    NOT NULL,
                PRIMARY KEY (shelf_id, book_id)
            );
            CREATE INDEX IF NOT EXISTS idx_shelf_books_shelf ON shelf_books(shelf_id, position);
            CREATE INDEX IF NOT EXISTS idx_shelf_books_book ON shelf_books(book_id);

            CREATE TABLE IF NOT EXISTS reading_list (
                book_id  INTEGER PRIMARY KEY REFERENCES books(id) ON DELETE CASCADE,
                position INTEGER NOT NULL DEFAULT 0,
                note     TEXT    NOT NULL DEFAULT '',
                added_at TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_reading_list_position ON reading_list(position);

            -- Append-only history log: opened | finished | unfinished | imported
            CREATE TABLE IF NOT EXISTS reading_events (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                book_id    INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                kind       TEXT    NOT NULL,
                at         TEXT    NOT NULL,
                detail     TEXT    NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_reading_events_book ON reading_events(book_id);
            CREATE INDEX IF NOT EXISTS idx_reading_events_at ON reading_events(at DESC);
            CREATE INDEX IF NOT EXISTS idx_reading_events_kind ON reading_events(kind);

            -- v14: append-only dictionary lookup history (Phase 10).
            -- book_id/chapter_index are nullable: the sidebar search logs
            -- lookups that are not tied to a book. `found` = the lookup
            -- resolved to senses (misses are the signal for pack gaps).
            CREATE TABLE IF NOT EXISTS dict_lookups (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                word          TEXT    NOT NULL,
                book_id       INTEGER REFERENCES books(id) ON DELETE SET NULL,
                chapter_index INTEGER,
                context_text  TEXT    NOT NULL DEFAULT '',
                found         INTEGER NOT NULL DEFAULT 1,
                at            TEXT    NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_dict_lookups_word ON dict_lookups(word COLLATE NOCASE);
            CREATE INDEX IF NOT EXISTS idx_dict_lookups_at ON dict_lookups(at DESC);

            -- One row per reader visit; closed out when the reader shuts down.
            CREATE TABLE IF NOT EXISTS reading_sessions (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                book_id     INTEGER NOT NULL REFERENCES books(id) ON DELETE CASCADE,
                started_at  TEXT    NOT NULL,
                ended_at    TEXT,
                seconds     INTEGER NOT NULL DEFAULT 0,
                start_pct   INTEGER NOT NULL DEFAULT 0,
                end_pct     INTEGER NOT NULL DEFAULT 0
            );
            CREATE INDEX IF NOT EXISTS idx_reading_sessions_book ON reading_sessions(book_id);
            CREATE INDEX IF NOT EXISTS idx_reading_sessions_started ON reading_sessions(started_at DESC);

            -- P4.1: tiny key/value store for UI preferences that must survive
            -- restarts (reader theme, font size, ...).
            CREATE TABLE IF NOT EXISTS app_prefs (
                key   TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            -- v7: metadata you edited by hand, remembered against the file's
            -- content hash so it survives removing and re-importing the book.
            -- Deliberately NOT cascaded from books: the whole point is that it
            -- outlives the catalog row.
            CREATE TABLE IF NOT EXISTS metadata_overrides (
                file_hash    TEXT PRIMARY KEY,
                title        TEXT,
                authors      TEXT,
                series       TEXT,
                series_index REAL,
                publisher    TEXT,
                published    TEXT,
                description  TEXT,
                tags         TEXT,
                rating       INTEGER,
                cover_name   TEXT,
                updated_at   TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS author_profiles (
                id                INTEGER PRIMARY KEY AUTOINCREMENT,
                canonical_name    TEXT NOT NULL,
                sort_name         TEXT NOT NULL DEFAULT '',
                normalized_name   TEXT NOT NULL UNIQUE,
                bio               TEXT NOT NULL DEFAULT '',
                birth_date        TEXT NOT NULL DEFAULT '',
                death_date        TEXT NOT NULL DEFAULT '',
                top_work          TEXT NOT NULL DEFAULT '',
                top_subjects_json TEXT NOT NULL DEFAULT '[]',
                openlibrary_key   TEXT NOT NULL DEFAULT '',
                photo_file        TEXT,
                work_count        INTEGER NOT NULL DEFAULT 0,
                works_json        TEXT NOT NULL DEFAULT '[]',
                fetched_at        TEXT NOT NULL DEFAULT '',
                source_url        TEXT NOT NULL DEFAULT ''
            );
            CREATE INDEX IF NOT EXISTS idx_author_profiles_name
                ON author_profiles(canonical_name COLLATE NOCASE);

            CREATE TABLE IF NOT EXISTS author_aliases (
                author_id         INTEGER NOT NULL REFERENCES author_profiles(id) ON DELETE CASCADE,
                alias             TEXT NOT NULL,
                normalized_alias  TEXT NOT NULL UNIQUE,
                PRIMARY KEY (author_id, normalized_alias)
            );
            CREATE INDEX IF NOT EXISTS idx_author_aliases_author
                ON author_aliases(author_id);

            -- v10: series listings fetched from Open Library, one row per
            -- series so the book page's series float caches first-time only.
            CREATE TABLE IF NOT EXISTS series_cache (
                series_key TEXT PRIMARY KEY,
                source     TEXT NOT NULL,
                fetched_at TEXT NOT NULL,
                works_json TEXT NOT NULL
            );
            -- v11: remote content platform (Manga/Fiction)
            CREATE TABLE IF NOT EXISTS remote_books (
                id             TEXT PRIMARY KEY, -- uuid
                source_id      TEXT NOT NULL,    -- e.g. "mangadex"
                remote_id      TEXT NOT NULL,    -- unique ID from the source
                title          TEXT NOT NULL,
                author         TEXT NOT NULL,
                description    TEXT NOT NULL,
                cover_url      TEXT,
                added_at       TEXT NOT NULL,
                last_update_at TEXT,
                status         TEXT NOT NULL DEFAULT 'Ongoing'
            );
            CREATE UNIQUE INDEX IF NOT EXISTS idx_remote_books_source_remote 
                ON remote_books(source_id, remote_id);

            CREATE TABLE IF NOT EXISTS remote_chapters (
                id             TEXT PRIMARY KEY, -- uuid
                book_id        TEXT NOT NULL REFERENCES remote_books(id) ON DELETE CASCADE,
                chapter_id     TEXT NOT NULL,    -- remote chapter ID
                title          TEXT NOT NULL,
                number         REAL NOT NULL,    -- FLOAT
                volume         REAL,             -- FLOAT
                url            TEXT,
                read_status    TEXT NOT NULL DEFAULT 'Unread', -- 'Unread', 'Reading', 'Read'
                fetched_at     TEXT NOT NULL,
                downloaded_path TEXT             -- Future proofing for local CBZ downloads
            );
            CREATE INDEX IF NOT EXISTS idx_remote_chapters_book 
                ON remote_chapters(book_id);
            CREATE UNIQUE INDEX IF NOT EXISTS idx_remote_chapters_book_chapter
                ON remote_chapters(book_id, chapter_id);
            "#,
        )?;

        // books.last_opened_at / finished_at were added in v4; ALTER is the only
        // way to extend an existing table created by v1–v3.
        add_column_if_missing(&conn, "books", "last_opened_at", "TEXT")?;
        add_column_if_missing(&conn, "books", "finished_at", "TEXT")?;
        // v5: half-star ratings stored as 0..=10 (i.e. tenths of the 5-star
        // scale x2) so "3.5 stars" is an integer 7 and needs no float compare.
        add_column_if_missing(&conn, "books", "rating", "INTEGER NOT NULL DEFAULT 0")?;
        // v6: publication details. `series_index` is REAL because half-numbers
        // ("book 2.5") are common in series, and 0 means "not set".
        add_column_if_missing(&conn, "books", "publisher", "TEXT NOT NULL DEFAULT ''")?;
        add_column_if_missing(&conn, "books", "published", "TEXT NOT NULL DEFAULT ''")?;
        add_column_if_missing(&conn, "books", "series_index", "REAL NOT NULL DEFAULT 0")?;

        // Indexed *after* the ALTERs above, since these columns do not exist in
        // the CREATE TABLE that older databases were built from.
        // Stats and the dashboard filter on them on every visit.
        conn.execute_batch(
            "
            CREATE INDEX IF NOT EXISTS idx_books_progress ON books(progress);
            CREATE INDEX IF NOT EXISTS idx_books_last_opened ON books(last_opened_at DESC);
            CREATE INDEX IF NOT EXISTS idx_books_finished ON books(finished_at);
            ",
        )?;

        // v11: precomputed dictionary headword key (fold_key — lowercased,
        // diacritics stripped, whitespace collapsed) so exact/prefix lookups
        // hit an index instead of a case-insensitive LIKE scan over word.
        // Added after the CREATE TABLE because older databases were built
        // without it, exactly like the books columns above.
        add_column_if_missing(&conn, "dict_entries", "key", "TEXT")?;
        conn.execute_batch(
            "
            CREATE INDEX IF NOT EXISTS idx_dict_entries_key ON dict_entries(key COLLATE NOCASE);
            ",
        )?;

        // Backfill the key column for rows imported before v11. Computed in
        // Rust (SQLite cannot strip diacritics), batched in small write
        // transactions; guarded by `key IS NULL` so it runs at most once.
        // The read guard is dropped first so the write transaction can take
        // the connection.
        drop(conn);
        self.backfill_dict_entry_keys()?;
        let mut conn = self.conn();

        // v12: dictionary priority (lower = shown first; imports default 100)
        // and the merged store. Databases that already have entries get the
        // store built once here; later rebuilds happen whenever the
        // dictionary set changes (import / remove / bundled install).
        add_column_if_missing(
            &conn,
            "dictionaries",
            "priority",
            "INTEGER NOT NULL DEFAULT 100",
        )?;
        let has_entries: i64 =
            conn.query_row("SELECT COUNT(*) FROM dict_entries", [], |r| r.get(0))?;
        let combined_empty: i64 =
            conn.query_row("SELECT COUNT(*) FROM combined_words", [], |r| r.get(0))?;
        if has_entries > 0 && combined_empty == 0 {
            drop(conn);
            self.rebuild_combined_dictionary()?;
            conn = self.conn();
        }

        // v13: saved_words.known — Phase 7 review status. Existing rows
        // default to 0 (unknown), so nothing needs a backfill.
        add_column_if_missing(&conn, "saved_words", "known", "INTEGER NOT NULL DEFAULT 0")?;

        let version: Option<i64> = conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |r| {
                r.get(0)
            })
            .optional()?;
        if version.is_none() {
            conn.execute(
                "INSERT INTO schema_version (version) VALUES (?1)",
                params![SCHEMA_VERSION],
            )?;
        } else if let Some(v) = version {
            if v < SCHEMA_VERSION {
                conn.execute(
                    "UPDATE schema_version SET version = ?1",
                    params![SCHEMA_VERSION],
                )?;
            }
        }
        Ok(())
    }

    /// Every book's uuid — used to spot orphaned reader caches.
    pub fn all_uuids(&self) -> Result<Vec<String>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached("SELECT uuid FROM books")?;
        let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
        Ok(rows.flatten().collect())
    }

    #[allow(dead_code)]
    pub fn count_books(&self) -> Result<usize> {
        let conn = self.conn();
        let n: i64 = conn.query_row("SELECT COUNT(*) FROM books", [], |r| r.get(0))?;
        Ok(n as usize)
    }

    /// Just `(uuid, cover_name)` for books that have a cover.
    ///
    /// The thumbnail backfill needs exactly these two columns, but was calling
    /// [`Catalog::list_books`], which selects all 21 fields of every book *and*
    /// runs a second query joining `tags` — none of which it reads. On a
    /// 2,000-book library that is two full table scans and 2,000 `Book`
    /// structs built and dropped, on every launch, to answer a question about
    /// files on disk.
    ///
    /// Books with no cover are filtered in SQL rather than in the loop: they
    /// can never have a thumbnail, so carrying them out of the database only
    /// to skip them is pure waste.
    pub fn books_with_covers(&self) -> Result<Vec<(String, String)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT uuid, cover_name FROM books \
             WHERE cover_name IS NOT NULL AND cover_name <> ''",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        Ok(rows.flatten().collect())
    }

    pub fn list_books(&self, sort: SortKey, query: &str) -> Result<Vec<Book>> {
        let conn = self.conn();
        let order = match sort {
            SortKey::Title => "sort_title COLLATE NOCASE ASC",
            SortKey::Author => "authors COLLATE NOCASE ASC, sort_title COLLATE NOCASE ASC",
            SortKey::Added => "added_at DESC",
        };

        let q = query.trim();
        let mut books = if q.is_empty() {
            let sql = format!("SELECT {BOOK_COLUMNS} FROM books ORDER BY {order}");
            let mut stmt = conn.prepare_cached(&sql)?;
            let rows = stmt.query_map([], row_to_book)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        } else {
            let like = format!("%{}%", escape_like(q));
            let sql = format!(
                "SELECT {BOOK_COLUMNS}
                 FROM books
                 WHERE books.title LIKE ?1 ESCAPE '\\'
                    OR books.authors LIKE ?1 ESCAPE '\\'
                    OR IFNULL(books.series,'') LIKE ?1 ESCAPE '\\'
                 ORDER BY {order}"
            );
            let mut stmt = conn.prepare_cached(&sql)?;
            let rows = stmt.query_map(params![like], row_to_book)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };

        hydrate_books(&conn, &mut books)?;
        Ok(books)
    }

    /// Newest books, capped. Pages that show a handful of covers were calling
    /// `list_books` and loading the entire library to display six of them.
    pub fn recent_books(&self, limit: usize) -> Result<Vec<Book>> {
        let conn = self.conn();
        let sql = format!("SELECT {BOOK_COLUMNS} FROM books ORDER BY books.added_at DESC LIMIT ?1");
        let mut stmt = conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(params![limit as i64], row_to_book)?;
        let mut books = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        hydrate_books(&conn, &mut books)?;
        Ok(books)
    }

    pub fn add_remote_book(&self, book: &crate::sources::RemoteBookDetails, source_id: &str) -> anyhow::Result<()> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "INSERT OR REPLACE INTO remote_books (id, source_id, remote_id, title, author, description, cover_url, added_at, status)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'), ?8)"
        )?;
        
        let id_str = format!("{}-{}", source_id, book.remote_id);
        
        stmt.execute(rusqlite::params![
            id_str,
            source_id,
            book.remote_id,
            book.title,
            book.author,
            book.description,
            book.cover_url,
            book.status,
        ])?;
        Ok(())
    }

    pub fn list_remote_books(&self, source_id_filter: Option<&str>) -> anyhow::Result<Vec<crate::sources::RemoteBookDetails>> {
        let conn = self.conn.lock().unwrap();
        let mut sql = "SELECT source_id, remote_id, title, author, description, cover_url, status FROM remote_books".to_string();
        if let Some(src) = source_id_filter {
            sql.push_str(&format!(" WHERE source_id = '{src}'"));
        }
        sql.push_str(" ORDER BY added_at DESC");
        
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map([], |row| {
            Ok(crate::sources::RemoteBookDetails {
                remote_id: row.get(1)?,
                title: row.get(2)?,
                author: row.get(3)?,
                description: row.get(4)?,
                cover_url: row.get(5)?,
                status: row.get(6)?,
                tags: Vec::new(),
            })
        })?;
        
        let mut res = Vec::new();
        for r in rows {
            res.push(r?);
        }
        Ok(res)
    }

    pub fn add_remote_chapters(&self, book_remote_id: &str, source_id: &str, chapters: &[crate::sources::RemoteChapter]) -> anyhow::Result<()> {
        let book_id_str = format!("{}-{}", source_id, book_remote_id);
        
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn.prepare(
            "INSERT OR REPLACE INTO remote_chapters (id, book_id, chapter_id, title, number, volume, url, fetched_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, datetime('now'))"
        )?;

        for ch in chapters {
            let ch_id_str = format!("{}-{}", book_id_str, ch.chapter_id);
            stmt.execute(rusqlite::params![
                ch_id_str,
                book_id_str,
                ch.chapter_id,
                ch.title,
                ch.number,
                ch.volume,
                ch.url,
            ])?;
        }
        Ok(())
    }

    pub fn get_book(&self, id: i64) -> Result<Option<Book>> {
        let conn = self.conn();
        let mut book = conn
            .query_row(
                &format!("SELECT {BOOK_COLUMNS} FROM books WHERE books.id = ?1"),
                params![id],
                row_to_book,
            )
            .optional()?;
        if let Some(ref mut b) = book {
            b.tags = tags_for_book(&conn, b.id)?;
            b.cover_path = b
                .cover_name
                .as_ref()
                .map(|name| book_dir(&b.uuid).join(name));
            b.file_path = book_dir(&b.uuid).join(&b.file_name);
        }
        Ok(book)
    }

    /// Fetch many books at once, keyed by id.
    ///
    /// Callers that enrich a list (saved quotes, the dashboard feed) used to
    /// call [`Catalog::get_book`] in a loop. That is `2N + 1` queries, because
    /// `get_book` runs a second query for tags — 500 quotes meant roughly
    /// 1,001 round trips on every keystroke in the search box. This does it in
    /// two queries total regardless of `N`.
    ///
    /// Ids that do not exist are simply absent from the map, which lets the
    /// caller keep distinguishing "no such book" from "the read failed".
    pub fn books_by_ids(&self, ids: &[i64]) -> Result<HashMap<i64, Book>> {
        let mut out: HashMap<i64, Book> = HashMap::new();
        if ids.is_empty() {
            return Ok(out);
        }
        // De-duplicate: the same book usually owns many quotes.
        let mut unique: Vec<i64> = ids.to_vec();
        unique.sort_unstable();
        unique.dedup();

        let conn = self.conn();
        // SQLite caps host parameters (999 by default), so chunk defensively.
        for chunk in unique.chunks(500) {
            let holders = vec!["?"; chunk.len()].join(",");
            let sql = format!("SELECT {BOOK_COLUMNS} FROM books WHERE books.id IN ({holders})");
            let mut stmt = conn.prepare(&sql)?;
            let params = rusqlite::params_from_iter(chunk.iter());
            let rows = stmt.query_map(params, row_to_book)?;
            for b in rows {
                let mut b = b?;
                b.cover_path = b
                    .cover_name
                    .as_ref()
                    .map(|name| book_dir(&b.uuid).join(name));
                b.file_path = book_dir(&b.uuid).join(&b.file_name);
                out.insert(b.id, b);
            }
        }

        // One tags query for the whole batch instead of one per book.
        for chunk in unique.chunks(500) {
            let holders = vec!["?"; chunk.len()].join(",");
            let sql = format!(
                "SELECT bt.book_id, t.name FROM tags t
                 JOIN book_tags bt ON bt.tag_id = t.id
                 WHERE bt.book_id IN ({holders})
                 ORDER BY t.name COLLATE NOCASE"
            );
            let mut stmt = conn.prepare(&sql)?;
            let params = rusqlite::params_from_iter(chunk.iter());
            let rows = stmt.query_map(params, |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (book_id, tag) = row?;
                if let Some(b) = out.get_mut(&book_id) {
                    b.tags.push(tag);
                }
            }
        }
        Ok(out)
    }

    pub fn find_by_hash(&self, hash: &str) -> Result<Option<i64>> {
        let conn = self.conn();
        let id = conn
            .query_row(
                "SELECT id FROM books WHERE file_hash = ?1 LIMIT 1",
                params![hash],
                |r| r.get(0),
            )
            .optional()?;
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn insert_book(
        &self,
        uuid: &str,
        title: &str,
        authors: &str,
        series: Option<&str>,
        description: &str,
        format: BookFormat,
        file_name: &str,
        file_hash: &str,
        cover_name: Option<&str>,
        tags: &[String],
    ) -> Result<i64> {
        let conn = self.conn();
        let added = chrono_like_now();
        let sort_title = title.to_lowercase();
        conn.execute(
            "INSERT INTO books
                (uuid, title, sort_title, authors, series, description, format,
                 file_name, file_hash, cover_name, added_at, progress)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,0)",
            params![
                uuid,
                title,
                sort_title,
                authors,
                series,
                description,
                format.as_str(),
                file_name,
                file_hash,
                cover_name,
                added,
            ],
        )?;
        let id = conn.last_insert_rowid();
        for tag in tags {
            let tag = tag.trim();
            if tag.is_empty() {
                continue;
            }
            conn.execute(
                "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
                params![tag],
            )?;
            let tag_id: i64 = conn.query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                params![tag],
                |r| r.get(0),
            )?;
            conn.execute(
                "INSERT OR IGNORE INTO book_tags (book_id, tag_id) VALUES (?1, ?2)",
                params![id, tag_id],
            )?;
        }
        Ok(id)
    }

    pub fn delete_book(&self, id: i64) -> Result<()> {
        let book = match self.get_book(id)? {
            Some(b) => b,
            None => return Ok(()),
        };
        // Capture any hand-edited metadata first: the row is about to go, and
        // re-importing the same file should not lose your work.
        let _ = self.remember_overrides(id);
        {
            let conn = self.conn();
            conn.execute("DELETE FROM books WHERE id = ?1", params![id])?;
        }
        let dir = book_dir(&book.uuid);
        if dir.exists() {
            let _ = fs::remove_dir_all(&dir);
        }
        // A0 step 3: drop the cover thumbnail so the cache cannot grow with
        // deleted books.
        crate::thumbs::remove_thumbnail(&book.uuid);
        // The startup backfill skips itself when the book count matches its
        // last complete run. A delete followed by an import nets to the same
        // count, which would wrongly skip the new book, so forget the marker
        // here — the cost of an unnecessary pass is far lower than the cost of
        // a book permanently without a thumbnail.
        crate::thumbs::invalidate_backfill_marker(self);
        Ok(())
    }

    /// Detailed reading position (chapter + in-chapter fraction).
    pub fn get_reading_progress(&self, book_id: i64) -> Result<Option<(usize, f64)>> {
        let conn = self.conn();
        let row = conn
            .query_row(
                "SELECT chapter_index, fraction FROM reading_progress WHERE book_id = ?1",
                params![book_id],
                |r| Ok((r.get::<_, i64>(0)? as usize, r.get::<_, f64>(1)?)),
            )
            .optional()?;
        Ok(row)
    }

    /// Save position and update the books.progress percent (0–100).
    pub fn set_reading_progress(
        &self,
        book_id: i64,
        chapter_index: usize,
        fraction: f64,
        chapter_count: usize,
    ) -> Result<()> {
        let conn = self.conn();
        let frac = fraction.clamp(0.0, 1.0);
        let now = chrono_like_now();
        conn.execute(
            "INSERT INTO reading_progress (book_id, chapter_index, fraction, updated_at)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(book_id) DO UPDATE SET
               chapter_index = excluded.chapter_index,
               fraction = excluded.fraction,
               updated_at = excluded.updated_at",
            params![book_id, chapter_index as i64, frac, now],
        )?;
        // Overall percent across chapters.
        let overall = if chapter_count == 0 {
            0.0
        } else {
            ((chapter_index as f64) + frac) / (chapter_count as f64) * 100.0
        };
        let pct = overall.round().clamp(0.0, 100.0) as i64;
        conn.execute(
            "UPDATE books SET progress = ?1 WHERE id = ?2",
            params![pct, book_id],
        )?;
        drop(conn);

        // Deliberately NOT refreshing the sidecar here.
        //
        // This runs on every page turn. Rewriting a JSON file that often would
        // be pointless disk traffic, and the thing it would protect — your
        // exact place in a book — is the least valuable field in the backup
        // and the one most quickly re-found. `checkpoint_session` in the
        // reader already persists progress to the database on leaving a book,
        // and the sidecar picks the position up on the next real edit.
        //
        // Worth stating rather than leaving as an omission: everything else
        // that changes a book does refresh it, so a reader glancing at those
        // call sites would otherwise read this one as a bug.
        Ok(())
    }
}

/// Shared projection so every book query returns the same column order.
const BOOK_COLUMNS: &str = "books.id, books.uuid, books.title, books.authors, books.series, \
     books.description, books.format, books.file_name, books.file_hash, books.cover_name, \
     books.added_at, books.progress, books.rating, books.publisher, books.published, \
     books.series_index";

/// Sessions longer than this are almost certainly an idle window.
const MAX_SESSION_SECONDS: i64 = 6 * 60 * 60;

/// Fill tags + resolved paths for a freshly queried batch of books.
fn hydrate_books(conn: &Connection, books: &mut [Book]) -> Result<()> {
    if books.is_empty() {
        return Ok(());
    }

    // One query for every book's tags, rather than one query per book. With a
    // few hundred books the old loop was the dominant cost of opening any page
    // that showed a list.
    // Bound parameters, not interpolated ids. The values are `i64`s straight
    // from the database, so nothing here was injectable — but this was the
    // one dynamic `IN (...)` in the codebase built by string concatenation
    // while `books_by_ids` and `finished_ids_among` next door both use
    // placeholders. An inconsistency in a security-relevant pattern is how
    // the next one gets written against user input, so it now matches them.
    let holders = vec!["?"; books.len()].join(",");
    let sql = format!(
        "SELECT bt.book_id, t.name FROM tags t
         JOIN book_tags bt ON bt.tag_id = t.id
         WHERE bt.book_id IN ({holders})
         ORDER BY t.name COLLATE NOCASE"
    );
    let ids: Vec<i64> = books.iter().map(|b| b.id).collect();

    let mut by_book: std::collections::HashMap<i64, Vec<String>> = std::collections::HashMap::new();
    {
        // Plain prepare: the placeholder count varies with the page size, so
        // caching would grow the statement cache without bound.
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(rusqlite::params_from_iter(ids.iter()), |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?))
        })?;
        for row in rows {
            let (book_id, tag) = row?;
            by_book.entry(book_id).or_default().push(tag);
        }
    }

    for book in books.iter_mut() {
        book.tags = by_book.remove(&book.id).unwrap_or_default();
        book.cover_path = book
            .cover_name
            .as_ref()
            .map(|name| book_dir(&book.uuid).join(name));
        book.file_path = book_dir(&book.uuid).join(&book.file_name);
    }
    Ok(())
}

fn row_to_shelf(row: &rusqlite::Row<'_>) -> rusqlite::Result<Shelf> {
    let kind: String = row.get(2)?;
    Ok(Shelf {
        id: row.get(0)?,
        name: row.get(1)?,
        kind: ShelfKind::from_str_lossy(&kind),
        description: row.get(3)?,
        rules: row.get(4)?,
        position: row.get(5)?,
        created_at: row.get(6)?,
        updated_at: row.get(7)?,
        book_count: 0,
    })
}

/// `ALTER TABLE ... ADD COLUMN` guarded by a PRAGMA lookup, so migrations stay
/// idempotent on databases created by older versions.
fn add_column_if_missing(conn: &Connection, table: &str, column: &str, ty: &str) -> Result<()> {
    let mut stmt = conn.prepare_cached(&format!("PRAGMA table_info({table})"))?;
    let existing: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    if !existing.iter().any(|c| c.eq_ignore_ascii_case(column)) {
        conn.execute_batch(&format!("ALTER TABLE {table} ADD COLUMN {column} {ty}"))?;
    }
    Ok(())
}

/// Longest and current run of consecutive days, given day keys (`YYYY-MM-DD`)
/// sorted newest first.
fn streaks(days_desc: &[String]) -> (i64, i64) {
    if days_desc.is_empty() {
        return (0, 0);
    }
    let nums: Vec<i64> = days_desc.iter().filter_map(|d| days_from_iso(d)).collect();
    if nums.is_empty() {
        return (0, 0);
    }

    // The user's today, not UTC's. `days_desc` is now bucketed by local day
    // (see `local_day_sql`), so comparing it against a UTC "today" would be
    // an off-by-one-day for anyone east or west of Greenwich for part of the
    // clock — including the 05:30 window that broke streaks at UTC+05:30.
    let today = days_from_iso(&local_today()).unwrap_or(nums[0]);
    // A streak is "current" if the newest day is today or yesterday.
    let mut current = 0;
    if today - nums[0] <= 1 {
        current = 1;
        for pair in nums.windows(2) {
            if pair[0] - pair[1] == 1 {
                current += 1;
            } else {
                break;
            }
        }
    }

    let mut longest = 1;
    let mut run = 1;
    for pair in nums.windows(2) {
        if pair[0] - pair[1] == 1 {
            run += 1;
            longest = longest.max(run);
        } else {
            run = 1;
        }
    }
    (current, longest)
}

fn days_from_iso(s: &str) -> Option<i64> {
    let bytes = s.as_bytes();
    if bytes.len() < 10 {
        return None;
    }
    let y: i64 = s[0..4].parse().ok()?;
    let m: i64 = s[5..7].parse().ok()?;
    let d: i64 = s[8..10].parse().ok()?;
    Some(days_from_civil(y, m, d))
}

/// Howard Hinnant's `days_from_civil` — inverse of `civil_from_days` below.
fn days_from_civil(y: i64, m: i64, d: i64) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

/// Whole days since the Unix epoch, for weekday arithmetic.
pub fn days_since_epoch() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| (d.as_secs() / 86400) as i64)
        .unwrap_or(0)
}

/// Seconds to add to UTC to get the user's local wall-clock time.
///
/// # Why this exists
///
/// Every timestamp is *stored* as ISO-8601 UTC, which is right and does not
/// change. But every day-bucket used to be computed as the UTC date, and a
/// day-bucket is a **wall-clock** question: "did I read today?" means the
/// user's today, not Greenwich's.
///
/// At UTC+05:30 — the timezone this project is developed in — reading between
/// midnight and 05:30 local was filed under the *previous* day. That shifted
/// the 14-day chart, made "last 7 days" a window offset by five and a half
/// hours, and, worst of all, broke reading streaks: a late-night reader could
/// read every single night and still see the streak reset, because two
/// consecutive local days collapsed into one UTC day.
///
/// # Why SQLite computes it
///
/// The project deliberately has no `chrono`/`time` dependency, and reading
/// `/etc/localtime` by hand is a TZif parser nobody should write. SQLite is
/// already linked, already knows the OS timezone, and answers this in one
/// query. Computed once per process: the offset can technically change under
/// a running process (DST), but a reader app that is wrong for one evening
/// after a DST shift and right again on restart is an acceptable trade for
/// not carrying a timezone database.
pub fn local_offset_seconds() -> i64 {
    static OFFSET: std::sync::OnceLock<i64> = std::sync::OnceLock::new();
    *OFFSET.get_or_init(|| {
        // A scratch in-memory connection: this must work before (and
        // independently of) the catalog being open.
        let Ok(conn) = Connection::open_in_memory() else {
            return 0;
        };
        conn.query_row(
            "SELECT CAST(strftime('%s', 'now', 'localtime') AS INTEGER)
                  - CAST(strftime('%s', 'now') AS INTEGER)",
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0)
    })
}

/// SQL fragment: the **local** calendar day (`YYYY-MM-DD`) of a stored UTC
/// timestamp column.
///
/// Use this anywhere a query groups or filters by day. Writing
/// `substr(col, 1, 10)` instead takes the UTC date and reintroduces the bug
/// described on [`local_offset_seconds`].
///
/// `datetime(col, 'localtime')` is not used here: it would make the result
/// depend on the timezone of whatever process runs the query, whereas the
/// offset above is resolved once and shared with the Rust-side day maths, so
/// SQL and Rust always agree on where a day boundary is.
pub fn local_day_sql(column: &str) -> String {
    let offset = local_offset_seconds();
    format!("substr(datetime({column}, '{offset:+} seconds'), 1, 10)")
}

/// ISO-8601 **UTC** timestamp for the instant of local midnight, `days` ago.
///
/// The value is still UTC because that is what the stored columns are
/// compared against; the *boundary* it names is the user's midnight, not
/// Greenwich's. That is the whole distinction: "the last 7 days" should start
/// at 00:00 in the user's kitchen.
pub fn iso_days_ago(days: i64) -> String {
    iso_days_ago_at(days, now_unix(), local_offset_seconds())
}

/// [`iso_days_ago`] with the clock and the timezone injected.
///
/// Split out so the day maths is testable without depending on the machine's
/// timezone — `docs/pitfalls.md` §19: a test run in UTC would pass on every
/// version of this function, including the broken one.
fn iso_days_ago_at(days: i64, now: i64, offset: i64) -> String {
    let then = (now - days.max(0) * 86400).max(0);
    // Snap to midnight *in local time*: shift into local, truncate the
    // day there, then shift back to UTC for storage comparison.
    let local = then + offset;
    let local_midnight = local - local.rem_euclid(86400);
    let utc_midnight = (local_midnight - offset).max(0);
    format_unix_utc(utc_midnight as u64)
}

/// Seconds since the Unix epoch, or 0 if the clock is before it.
fn now_unix() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// The local calendar date `days` ago, as `YYYY-MM-DD`.
///
/// The label counterpart to [`iso_days_ago`]: that returns a UTC *instant* to
/// compare a column against, this returns the *day string* a
/// [`local_day_sql`] bucket will carry. Chart code needs the second one, and
/// using `iso_days_ago(n)[..10]` for it is wrong — that is the UTC date of a
/// local midnight, which is the previous day for any positive offset.
pub fn local_day_ago(days: i64) -> String {
    local_day_ago_at(days, now_unix(), local_offset_seconds())
}

/// [`local_day_ago`] with the clock and timezone injected, for tests.
fn local_day_ago_at(days: i64, now: i64, offset: i64) -> String {
    let local = (now + offset - days.max(0) * 86400).max(0);
    format_unix_utc(local as u64)[..10].to_string()
}

/// Today's local calendar date as `YYYY-MM-DD`.
///
/// The counterpart to [`local_day_sql`] on the Rust side, so streak maths and
/// chart labels agree with what the queries bucketed.
pub fn local_today() -> String {
    local_day_ago_at(0, now_unix(), local_offset_seconds())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SortKey {
    Title,
    Author,
    Added,
}

impl SortKey {
    pub fn label(self) -> &'static str {
        match self {
            SortKey::Title => "Title",
            SortKey::Author => "Author",
            SortKey::Added => "Added",
        }
    }

    pub const ALL: &'static [SortKey] = &[SortKey::Title, SortKey::Author, SortKey::Added];
}

fn row_to_book(row: &rusqlite::Row<'_>) -> rusqlite::Result<Book> {
    let format_str: String = row.get(6)?;
    Ok(Book {
        id: row.get(0)?,
        uuid: row.get(1)?,
        title: row.get(2)?,
        authors: row.get(3)?,
        series: row.get(4)?,
        description: row.get(5)?,
        format: BookFormat::from_str_lossy(&format_str),
        file_name: row.get(7)?,
        file_hash: row.get(8)?,
        cover_name: row.get(9)?,
        added_at: row.get(10)?,
        progress: row.get::<_, i64>(11)? as u8,
        // Older rows predate the column; treat a read failure as unrated.
        rating: row.get::<_, i64>(12).unwrap_or(0) as u8,
        publisher: row.get::<_, String>(13).unwrap_or_default(),
        published: row.get::<_, String>(14).unwrap_or_default(),
        series_index: row.get::<_, f64>(15).unwrap_or(0.0) as f32,
        tags: Vec::new(),
        cover_path: None,
        file_path: PathBuf::new(),
    })
}

fn row_to_annotation(row: &rusqlite::Row<'_>) -> rusqlite::Result<Annotation> {
    Ok(Annotation {
        id: row.get(0)?,
        book_id: row.get(1)?,
        kind: row.get(2)?,
        chapter_index: row.get(3)?,
        start_path: row.get(4)?,
        start_offset: row.get(5)?,
        end_path: row.get(6)?,
        end_offset: row.get(7)?,
        color: row.get(8)?,
        text_excerpt: row.get(9)?,
        note: row.get(10)?,
        cfi: row.get(11)?,
        created_at: row.get(12)?,
        updated_at: row.get(13)?,
    })
}

fn row_to_saved_word(row: &rusqlite::Row<'_>) -> rusqlite::Result<SavedWord> {
    Ok(SavedWord {
        id: row.get(0)?,
        word: row.get(1)?,
        definition: row.get(2)?,
        dict_name: row.get(3)?,
        book_id: row.get(4)?,
        chapter_index: row.get(5)?,
        context_text: row.get(6)?,
        created_at: row.get(7)?,
        known: row.get::<_, i64>(8)? != 0,
    })
}

fn row_to_reading_bookmark(row: &rusqlite::Row<'_>) -> rusqlite::Result<ReadingBookmark> {
    Ok(ReadingBookmark {
        id: row.get(0)?,
        book_id: row.get(1)?,
        chapter_index: row.get(2)?,
        fraction: row.get(3)?,
        label: row.get(4)?,
        created_at: row.get(5)?,
    })
}

fn tags_for_book(conn: &Connection, book_id: i64) -> Result<Vec<String>> {
    let mut stmt = conn.prepare_cached(
        "SELECT t.name FROM tags t
         JOIN book_tags bt ON bt.tag_id = t.id
         WHERE bt.book_id = ?1
         ORDER BY t.name COLLATE NOCASE",
    )?;
    let rows = stmt.query_map(params![book_id], |r| r.get(0))?;
    let mut tags = Vec::new();
    for t in rows {
        tags.push(t?);
    }
    Ok(tags)
}

fn escape_like(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('%', "\\%")
        .replace('_', "\\_")
}

/// Current time in the same ISO-8601 UTC format every stored timestamp uses.
pub fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format_unix_utc(secs)
}

fn format_unix_utc(secs: u64) -> String {
    let days = (secs / 86400) as i64;
    let tod = secs % 86400;
    let (y, m, d) = civil_from_days(days);
    let hh = tod / 3600;
    let mm = (tod % 3600) / 60;
    let ss = tod % 60;
    format!("{y:04}-{m:02}-{d:02}T{hh:02}:{mm:02}:{ss:02}Z")
}

fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y as i32, m as u32, d as u32)
}

pub fn hash_file(path: &Path) -> Result<String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 64 * 1024];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shelf_rules::{MatchMode, Rule, RuleField, RuleOp, RuleSet};

    fn seed(cat: &Catalog, title: &str, authors: &str, tags: &[&str]) -> i64 {
        let uuid = format!("uuid-{title}");
        let tags: Vec<String> = tags.iter().map(|t| t.to_string()).collect();
        cat.insert_book(
            &uuid,
            title,
            authors,
            None,
            "",
            BookFormat::Epub,
            "book.epub",
            &format!("hash-{title}"),
            None,
            &tags,
        )
        .expect("insert")
    }

    #[test]
    fn migrations_land_on_current_version() {
        let cat = Catalog::open_in_memory().unwrap();
        let conn = cat.conn.lock().unwrap();
        let v: i64 = conn
            .query_row("SELECT version FROM schema_version LIMIT 1", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(v, SCHEMA_VERSION);
    }

    #[test]
    fn saved_words_known_flag_round_trips_and_filters() {
        let cat = Catalog::open_in_memory().unwrap();
        let a = cat
            .insert_saved_word(
                "serendipity",
                "a happy accident",
                Some("WordNet"),
                None,
                None,
                Some("luck, chance"),
            )
            .unwrap();
        let b = cat
            .insert_saved_word(
                "wander",
                "to walk aimlessly",
                Some("WordNet"),
                None,
                None,
                None,
            )
            .unwrap();

        // Fresh rows are unknown (to review).
        let all = cat.list_saved_words("", None).unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().all(|w| !w.known));
        assert_eq!(cat.list_saved_words("", Some(false)).unwrap().len(), 2);
        assert!(cat.list_saved_words("", Some(true)).unwrap().is_empty());

        // Mark one known; the flag round-trips and the filters split.
        cat.set_saved_word_known(a, true).unwrap();
        let known = cat.list_saved_words("", Some(true)).unwrap();
        assert_eq!(known.len(), 1);
        assert_eq!(known[0].id, a);
        assert!(known[0].known);
        let review = cat.list_saved_words("", Some(false)).unwrap();
        assert_eq!(review.len(), 1);
        assert_eq!(review[0].id, b);

        // Toggling back works too.
        cat.set_saved_word_known(a, false).unwrap();
        assert_eq!(cat.list_saved_words("", Some(true)).unwrap().len(), 0);
        assert_eq!(cat.list_saved_words("", Some(false)).unwrap().len(), 2);

        // Search still combines with the filter.
        cat.set_saved_word_known(b, true).unwrap();
        let hits = cat.list_saved_words("wan", Some(true)).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].word, "wander");
        assert!(cat
            .list_saved_words("serendipity", Some(true))
            .unwrap()
            .is_empty());
    }

    #[test]
    fn manual_shelf_membership_round_trips() {
        let cat = Catalog::open_in_memory().unwrap();
        let a = seed(&cat, "Dune", "Frank Herbert", &["scifi"]);
        let b = seed(&cat, "Emma", "Jane Austen", &["classic"]);
        let shelf_id = cat
            .create_shelf("Favourites", ShelfKind::Manual, "", "")
            .unwrap();

        cat.add_book_to_shelf(shelf_id, a).unwrap();
        cat.add_book_to_shelf(shelf_id, b).unwrap();
        // Duplicate add must not create a second row.
        cat.add_book_to_shelf(shelf_id, a).unwrap();

        let shelf = cat.get_shelf(shelf_id).unwrap().unwrap();
        assert_eq!(shelf.book_count, 2);

        cat.remove_book_from_shelf(shelf_id, b).unwrap();
        let shelf = cat.get_shelf(shelf_id).unwrap().unwrap();
        assert_eq!(shelf.book_count, 1);
        assert_eq!(cat.shelves_for_book(a).unwrap().len(), 1);
    }

    #[test]
    fn smart_shelf_filters_by_tag_and_progress() {
        let cat = Catalog::open_in_memory().unwrap();
        let dune = seed(&cat, "Dune", "Frank Herbert", &["scifi", "classic"]);
        seed(&cat, "Emma", "Jane Austen", &["classic"]);
        seed(&cat, "Neuromancer", "William Gibson", &["scifi"]);

        // Dune is half read; the others are untouched.
        cat.set_reading_progress(dune, 5, 0.5, 10).unwrap();

        let mut rules = RuleSet::default();
        rules
            .rules
            .push(Rule::new(RuleField::Tag, RuleOp::Is, "scifi"));
        rules
            .rules
            .push(Rule::new(RuleField::Progress, RuleOp::Is, "unread"));

        let id = cat
            .create_shelf("Unread scifi", ShelfKind::Smart, "", &rules.to_json())
            .unwrap();
        let shelf = cat.get_shelf(id).unwrap().unwrap();
        let books = cat.shelf_books(&shelf, SortKey::Title, "").unwrap();
        assert_eq!(books.len(), 1);
        assert_eq!(books[0].title, "Neuromancer");

        // Same rules with ANY should widen the result.
        let mut any = rules.clone();
        any.set_mode(MatchMode::Any);
        assert_eq!(cat.count_matching_rules(&any).unwrap(), 3);
    }

    #[test]
    fn smart_shelf_search_binds_alongside_rules() {
        let cat = Catalog::open_in_memory().unwrap();
        seed(&cat, "Dune", "Frank Herbert", &["scifi"]);
        seed(&cat, "Neuromancer", "William Gibson", &["scifi"]);

        let mut rules = RuleSet::default();
        rules
            .rules
            .push(Rule::new(RuleField::Tag, RuleOp::Is, "scifi"));
        let id = cat
            .create_shelf("Scifi", ShelfKind::Smart, "", &rules.to_json())
            .unwrap();
        let shelf = cat.get_shelf(id).unwrap().unwrap();

        // Regression: rule params and the search term share one statement.
        let hits = cat.shelf_books(&shelf, SortKey::Title, "neuro").unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].title, "Neuromancer");
    }

    #[test]
    fn empty_smart_shelf_matches_nothing() {
        let cat = Catalog::open_in_memory().unwrap();
        seed(&cat, "Dune", "Frank Herbert", &["scifi"]);
        let id = cat.create_shelf("Empty", ShelfKind::Smart, "", "").unwrap();
        let shelf = cat.get_shelf(id).unwrap().unwrap();
        assert_eq!(shelf.book_count, 0);
    }

    #[test]
    fn reading_list_reorders() {
        let cat = Catalog::open_in_memory().unwrap();
        let a = seed(&cat, "A", "x", &[]);
        let b = seed(&cat, "B", "x", &[]);
        let c = seed(&cat, "C", "x", &[]);
        for id in [a, b, c] {
            cat.add_to_reading_list(id).unwrap();
        }

        let titles: Vec<String> = cat
            .list_reading_list()
            .unwrap()
            .iter()
            .map(|e| e.book.title.clone())
            .collect();
        assert_eq!(titles, vec!["A", "B", "C"]);

        cat.move_reading_list_entry(c, -1).unwrap();
        let titles: Vec<String> = cat
            .list_reading_list()
            .unwrap()
            .iter()
            .map(|e| e.book.title.clone())
            .collect();
        assert_eq!(titles, vec!["A", "C", "B"]);

        // Moving past the edge is a no-op, not an error.
        cat.move_reading_list_entry(a, -1).unwrap();
        assert_eq!(cat.list_reading_list().unwrap().len(), 3);
    }

    #[test]
    fn reading_list_note_and_meta_are_not_book_fields() {
        // Regression: list_reading_list once read position/note/added_at from
        // indices 12..14, which are actually progress/rating/publisher. Give
        // the book non-default values so the wrong reads would be visible.
        let cat = Catalog::open_in_memory().unwrap();
        let a = seed(&cat, "A", "x", &[]);
        cat.conn()
            .execute(
                "UPDATE books SET progress = 42, rating = 4, publisher = 'TestPub' WHERE id = ?1",
                params![a],
            )
            .unwrap();
        cat.add_to_reading_list(a).unwrap();
        cat.set_reading_list_note(a, "my note").unwrap();

        let entries = cat.list_reading_list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].position, 0);
        assert_eq!(entries[0].note, "my note");
        assert!(!entries[0].added_at.is_empty());
        assert_ne!(entries[0].added_at, "TestPub");
        // The book row itself still hydrates its real values.
        assert_eq!(entries[0].book.progress, 42);
        assert_eq!(entries[0].book.rating, 4);
        assert_eq!(entries[0].book.publisher, "TestPub");
    }

    #[test]
    fn finishing_a_book_clears_it_from_the_reading_list() {
        let cat = Catalog::open_in_memory().unwrap();
        let a = seed(&cat, "A", "x", &[]);
        cat.add_to_reading_list(a).unwrap();
        assert!(cat.is_in_reading_list(a).unwrap());

        cat.set_book_finished(a, true).unwrap();
        assert!(!cat.is_in_reading_list(a).unwrap());
        assert!(cat.book_finished_at(a).unwrap().is_some());

        cat.set_book_finished(a, false).unwrap();
        assert!(cat.book_finished_at(a).unwrap().is_none());
    }

    #[test]
    fn auto_finish_fires_once() {
        let cat = Catalog::open_in_memory().unwrap();
        let a = seed(&cat, "A", "x", &[]);
        assert!(!cat.auto_finish_if_complete(a, 50).unwrap());
        assert!(cat.auto_finish_if_complete(a, 100).unwrap());
        assert!(!cat.auto_finish_if_complete(a, 100).unwrap());
    }

    #[test]
    fn opening_twice_in_an_hour_logs_one_event() {
        let cat = Catalog::open_in_memory().unwrap();
        let a = seed(&cat, "A", "x", &[]);
        cat.mark_book_opened(a).unwrap();
        cat.mark_book_opened(a).unwrap();
        let events = cat.list_events(Some(EventKind::Opened), "", 50).unwrap();
        assert_eq!(events.len(), 1);
    }

    #[test]
    fn sessions_clamp_absurd_durations() {
        let cat = Catalog::open_in_memory().unwrap();
        let a = seed(&cat, "A", "x", &[]);
        let sid = cat.start_reading_session(a, 0).unwrap();
        cat.end_reading_session(sid, 99_999_999, 10).unwrap();
        assert_eq!(cat.total_reading_seconds(a).unwrap(), MAX_SESSION_SECONDS);
    }

    #[test]
    fn tags_browse_counts_books() {
        let cat = Catalog::open_in_memory().unwrap();
        seed(&cat, "Dune", "Frank Herbert", &["scifi", "classic"]);
        seed(&cat, "Emma", "Jane Austen", &["classic"]);

        let tags = cat.list_tags_with_counts().unwrap();
        assert_eq!(tags[0], ("classic".to_string(), 2));
        assert_eq!(
            cat.books_with_tag("scifi", SortKey::Title).unwrap().len(),
            1
        );
    }

    #[test]
    fn stats_reflect_library_state() {
        let cat = Catalog::open_in_memory().unwrap();
        let a = seed(&cat, "A", "x", &["t"]);
        seed(&cat, "B", "y", &[]);
        cat.set_reading_progress(a, 1, 0.5, 10).unwrap();

        let stats = cat.library_stats().unwrap();
        assert_eq!(stats.total_books, 2);
        assert_eq!(stats.reading, 1);
        assert_eq!(stats.unread, 1);
        assert_eq!(stats.minutes_by_day.len(), 14);
    }

    #[test]
    fn edits_survive_delete_and_reimport() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = seed(&cat, "Nyxia", "S. Reintgen", &["scifi"]);
        let hash = cat.get_book(id).unwrap().unwrap().file_hash;

        cat.update_book_metadata(
            id,
            "Nyxia Uprising",
            "Scott Reintgen",
            Some("The Nyxia Triad"),
            3.0,
            "Random House",
            "2019",
            "Edited by hand.",
            &["scifi".into(), "ya".into()],
        )
        .unwrap();
        cat.set_book_rating(id, 9).unwrap();

        // Removing the book must not discard the edits.
        cat.delete_book(id).unwrap();
        assert!(cat.get_book(id).unwrap().is_none());

        // Re-import: same file, so the same hash.
        let new_id = cat
            .insert_book(
                "uuid-again",
                "Nyxia",
                "S. Reintgen",
                None,
                "",
                BookFormat::Epub,
                "book.epub",
                &hash,
                None,
                &[],
            )
            .unwrap();
        assert!(cat.restore_overrides(new_id, &hash).unwrap());

        let restored = cat.get_book(new_id).unwrap().unwrap();
        assert_eq!(restored.title, "Nyxia Uprising");
        assert_eq!(restored.authors, "Scott Reintgen");
        assert_eq!(restored.series.as_deref(), Some("The Nyxia Triad"));
        assert_eq!(restored.series_index, 3.0);
        assert_eq!(restored.publisher, "Random House");
        assert_eq!(restored.rating, 9);
        assert_eq!(restored.tags.len(), 2);
    }

    #[test]
    fn a_restored_book_keeps_its_cover() {
        // NOTE: this test writes real files under the data dir (book cover +
        // the stashed override copy). The seeded title must stay unique so
        // its uuid/hash paths cannot collide with the other cover tests,
        // which run in parallel.
        let cat = Catalog::open_in_memory().unwrap();
        let id = seed(&cat, "CoverRestored", "x", &[]);
        let book = cat.get_book(id).unwrap().unwrap();
        let hash = book.file_hash.clone();

        // Put a real file where the cover is expected.
        let dir = crate::paths::book_dir(&book.uuid);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("cover.png"), b"not-really-a-png-but-long-enough").unwrap();
        cat.set_cover_name(id, Some("cover.png")).unwrap();

        cat.delete_book(id).unwrap();

        let new_id = cat
            .insert_book(
                "uuid-restored-cover",
                "A",
                "x",
                None,
                "",
                BookFormat::Epub,
                "book.epub",
                &hash,
                None,
                &[],
            )
            .unwrap();
        assert!(cat.restore_overrides(new_id, &hash).unwrap());

        let restored = cat.get_book(new_id).unwrap().unwrap();
        assert!(restored.cover_name.is_some(), "cover was not restored");
        assert!(
            restored.cover_path.as_ref().is_some_and(|p| p.is_file()),
            "cover file missing on disk"
        );

        // Tidy up so the test does not leave files in the real data dir.
        let _ = std::fs::remove_dir_all(crate::paths::book_dir(&restored.uuid));
        let _ = cat.forget_overrides(&hash);
    }

    #[test]
    fn replacing_a_cover_updates_the_remembered_copy() {
        // The bug: editing metadata stashed the cover, then the new cover was
        // written afterwards, so the override kept the *previous* jacket and a
        // re-import restored the wrong image.
        // Unique seeded title: this test writes real files (book dir + stashed
        // override cover) and must not collide with the other cover tests.
        let cat = Catalog::open_in_memory().unwrap();
        let id = seed(&cat, "CoverSwap", "x", &[]);
        let book = cat.get_book(id).unwrap().unwrap();
        let hash = book.file_hash.clone();
        let dir = crate::paths::book_dir(&book.uuid);
        std::fs::create_dir_all(&dir).unwrap();

        std::fs::write(dir.join("old.png"), vec![b'o'; 600]).unwrap();
        cat.set_cover_name(id, Some("old.png")).unwrap();

        // Swap in a different cover, as the metadata editor does.
        std::fs::write(dir.join("new.png"), vec![b'n'; 600]).unwrap();
        cat.set_cover_name(id, Some("new.png")).unwrap();

        cat.delete_book(id).unwrap();
        let new_id = cat
            .insert_book(
                "uuid-cover-swap",
                "A",
                "x",
                None,
                "",
                BookFormat::Epub,
                "book.epub",
                &hash,
                None,
                &[],
            )
            .unwrap();
        assert!(cat.restore_overrides(new_id, &hash).unwrap());

        let restored = cat.get_book(new_id).unwrap().unwrap();
        let bytes = std::fs::read(restored.cover_path.as_ref().unwrap()).unwrap();
        assert_eq!(bytes[0], b'n', "restored the pre-swap cover");

        let _ = std::fs::remove_dir_all(crate::paths::book_dir(&restored.uuid));
        let _ = std::fs::remove_dir_all(dir);
        let _ = cat.forget_overrides(&hash);
    }

    #[test]
    fn restoring_does_not_clobber_the_stashed_cover() {
        // Regression: restore_overrides applies metadata through the normal
        // edit path, which re-stashes. At that moment the book still has the
        // freshly-imported cover, so the saved image was overwritten with the
        // EPUB default a moment before it was due to be copied back.
        // Unique seeded title: this test writes real files (book dir + stashed
        // override cover) and must not collide with the other cover tests.
        let cat = Catalog::open_in_memory().unwrap();
        let id = seed(&cat, "CoverClobber", "x", &[]);
        let book = cat.get_book(id).unwrap().unwrap();
        let hash = book.file_hash.clone();
        let dir = crate::paths::book_dir(&book.uuid);
        std::fs::create_dir_all(&dir).unwrap();

        // A distinctive custom cover.
        std::fs::write(dir.join("custom.png"), vec![b'C'; 800]).unwrap();
        cat.set_cover_name(id, Some("custom.png")).unwrap();
        cat.delete_book(id).unwrap();

        // Re-import: the new book arrives with a *different* cover on disk,
        // standing in for whatever the EPUB supplies.
        let new_id = cat
            .insert_book(
                "uuid-clobber",
                "A",
                "x",
                None,
                "",
                BookFormat::Epub,
                "book.epub",
                &hash,
                Some("epub-default.png"),
                &[],
            )
            .unwrap();
        let fresh_dir = crate::paths::book_dir("uuid-clobber");
        std::fs::create_dir_all(&fresh_dir).unwrap();
        std::fs::write(fresh_dir.join("epub-default.png"), vec![b'E'; 800]).unwrap();

        assert!(cat.restore_overrides(new_id, &hash).unwrap());

        let restored = cat.get_book(new_id).unwrap().unwrap();
        let bytes = std::fs::read(restored.cover_path.as_ref().unwrap()).unwrap();
        assert_eq!(
            bytes[0], b'C',
            "restored the EPUB default instead of the saved cover"
        );

        let _ = std::fs::remove_dir_all(&fresh_dir);
        let _ = std::fs::remove_dir_all(&dir);
        let _ = cat.forget_overrides(&hash);
    }

    #[test]
    fn a_different_file_gets_no_overrides() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = seed(&cat, "A", "x", &[]);
        cat.update_book_metadata(id, "Edited", "x", None, 0.0, "", "", "", &[])
            .unwrap();

        // A book whose bytes differ has a different hash and must be untouched.
        let other = seed(&cat, "B", "y", &[]);
        let other_hash = cat.get_book(other).unwrap().unwrap().file_hash;
        assert!(!cat.restore_overrides(other, &other_hash).unwrap());
        assert_eq!(cat.get_book(other).unwrap().unwrap().title, "B");
    }

    #[test]
    fn forgetting_overrides_gives_a_clean_import() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = seed(&cat, "A", "x", &[]);
        let hash = cat.get_book(id).unwrap().unwrap().file_hash;
        cat.update_book_metadata(id, "Edited", "x", None, 0.0, "", "", "", &[])
            .unwrap();

        cat.forget_overrides(&hash).unwrap();
        assert!(!cat.restore_overrides(id, &hash).unwrap());
    }

    #[test]
    fn stats_cache_refreshes_after_a_write() {
        let cat = Catalog::open_in_memory().unwrap();
        seed(&cat, "A", "x", &[]);
        assert_eq!(cat.library_stats().unwrap().total_books, 1);

        // A second call must hit the cache and still be correct.
        assert_eq!(cat.library_stats().unwrap().total_books, 1);

        // Any write bumps total_changes(), so the next read recomputes.
        seed(&cat, "B", "y", &[]);
        assert_eq!(cat.library_stats().unwrap().total_books, 2);
    }

    #[test]
    fn recent_books_is_bounded_and_newest_first() {
        let cat = Catalog::open_in_memory().unwrap();
        for name in ["A", "B", "C"] {
            seed(&cat, name, "x", &["t"]);
        }
        let books = cat.recent_books(2).unwrap();
        assert_eq!(books.len(), 2);
        // Tags must still be hydrated by the batched lookup.
        assert_eq!(books[0].tags, vec!["t".to_string()]);
    }

    #[test]
    fn batched_tag_hydration_matches_per_book() {
        let cat = Catalog::open_in_memory().unwrap();
        seed(&cat, "Dune", "Herbert", &["scifi", "classic"]);
        seed(&cat, "Emma", "Austen", &["classic"]);
        seed(&cat, "Bare", "Nobody", &[]);

        let books = cat.list_books(SortKey::Title, "").unwrap();
        let find = |t: &str| books.iter().find(|b| b.title == t).unwrap().tags.clone();
        assert_eq!(find("Dune").len(), 2);
        assert_eq!(find("Emma"), vec!["classic".to_string()]);
        // A book with no tags must come back empty, not missing.
        assert!(find("Bare").is_empty());
    }

    #[test]
    fn recent_quotes_carries_its_book_title() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = seed(&cat, "Dune", "Herbert", &[]);
        cat.insert_annotation(
            id,
            "quote",
            0,
            "p",
            0,
            "p",
            9,
            "yellow",
            "Fear is the mind-killer",
            "",
        )
        .unwrap();

        let quotes = cat.recent_quotes(5).unwrap();
        assert_eq!(quotes.len(), 1);
        assert_eq!(quotes[0].1.title, "Dune");
        assert_eq!(cat.count_quotes().unwrap(), 1);
    }

    #[test]
    fn reading_bookmarks_round_trip() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = seed(&cat, "Dune", "Herbert", &[]);
        let mark = cat
            .insert_reading_bookmark(id, 2, 0.35, "The doors of stone")
            .unwrap();
        let all = cat.list_reading_bookmarks(id).unwrap();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].id, mark);
        assert_eq!(all[0].chapter_index, 2);
        assert!((all[0].fraction - 0.35).abs() < f64::EPSILON);
        assert_eq!(all[0].label, "The doors of stone");
        cat.delete_reading_bookmark(mark).unwrap();
        assert!(cat.list_reading_bookmarks(id).unwrap().is_empty());
    }

    #[test]
    fn metadata_edit_round_trips_new_fields() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = seed(&cat, "Dune", "Frank Herbert", &["scifi"]);

        cat.update_book_metadata(
            id,
            "Dune Messiah",
            "Frank Herbert",
            Some("Dune"),
            2.5,
            "Ace",
            "1969",
            "Sequel.",
            &["scifi".into(), "classic".into()],
        )
        .unwrap();

        let b = cat.get_book(id).unwrap().unwrap();
        assert_eq!(b.title, "Dune Messiah");
        assert_eq!(b.series.as_deref(), Some("Dune"));
        assert_eq!(b.series_index, 2.5);
        assert_eq!(b.publisher, "Ace");
        assert_eq!(b.published, "1969");
        assert_eq!(b.tags.len(), 2);
        // Fractional indexes must not render as "2.5.0".
        assert_eq!(b.series_display().as_deref(), Some("Dune #2.5"));
    }

    #[test]
    fn whole_series_numbers_drop_the_decimal() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = seed(&cat, "A", "x", &[]);
        cat.update_book_metadata(id, "A", "x", Some("Trilogy"), 3.0, "", "", "", &[])
            .unwrap();
        let b = cat.get_book(id).unwrap().unwrap();
        assert_eq!(b.series_display().as_deref(), Some("Trilogy #3"));
    }

    #[test]
    fn editing_tags_prunes_orphans() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = seed(&cat, "A", "x", &["temporary"]);
        cat.update_book_metadata(id, "A", "x", None, 0.0, "", "", "", &["kept".into()])
            .unwrap();
        let tags = cat.list_tags_with_counts().unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].0, "kept");
    }

    #[test]
    fn shelf_name_collisions_are_detected() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = cat
            .create_shelf("Favourites", ShelfKind::Manual, "", "")
            .unwrap();
        assert!(cat.shelf_name_taken("favourites", None).unwrap());
        // The shelf being edited doesn't collide with itself.
        assert!(!cat.shelf_name_taken("Favourites", Some(id)).unwrap());
        assert!(!cat.shelf_name_taken("Other", None).unwrap());
    }

    #[test]
    fn streak_helpers_handle_gaps() {
        assert_eq!(streaks(&[]), (0, 0));
        let days = vec![
            "2026-01-10".to_string(),
            "2026-01-09".to_string(),
            "2026-01-05".to_string(),
        ];
        // Not adjacent to today, so current is 0 but the run of 2 is longest.
        let (_, longest) = streaks(&days);
        assert_eq!(longest, 2);
    }

    #[test]
    fn book_first_opened_is_none_not_an_error_for_a_never_opened_book() {
        // `MIN(at)` over zero rows returns one row containing NULL, so this
        // used to be a hard error that every caller hid with `.ok().flatten()`
        // -- which is exactly how it stayed unnoticed.
        let cat = Catalog::open_in_memory().unwrap();
        let id = seed(&cat, "Dune", "Herbert", &[]);

        let first = cat
            .book_first_opened(id)
            .expect("never opened is not a failure");
        assert!(first.is_none());

        cat.log_event(id, EventKind::Opened, "").unwrap();
        assert!(cat.book_first_opened(id).unwrap().is_some());
    }

    #[test]
    fn finished_book_ids_batches_the_finished_flag() {
        let cat = Catalog::open_in_memory().unwrap();
        let a = seed(&cat, "Dune", "Herbert", &[]);
        let b = seed(&cat, "Emma", "Austen", &[]);
        let c = seed(&cat, "Ulysses", "Joyce", &[]);
        cat.set_book_finished(a, true).unwrap();
        cat.set_book_finished(c, true).unwrap();

        let done = cat.finished_book_ids(&[a, b, c]).unwrap();
        assert!(done.contains(&a));
        assert!(!done.contains(&b), "unfinished book must not appear");
        assert!(done.contains(&c));
        assert_eq!(done.len(), 2);

        // It agrees with the per-row call it replaces.
        for id in [a, b, c] {
            let single = cat.book_finished_at(id).unwrap().is_some();
            assert_eq!(single, done.contains(&id), "book {id}");
        }

        assert!(cat.finished_book_ids(&[]).unwrap().is_empty());
    }

    #[test]
    fn books_by_ids_batches_and_keeps_tags() {
        let cat = Catalog::open_in_memory().unwrap();
        let a = seed(&cat, "Dune", "Herbert", &["scifi", "classic"]);
        let b = seed(&cat, "Emma", "Austen", &[]);

        // Duplicate and unknown ids are both tolerated: the same book usually
        // owns many quotes, and a deleted book must not fail the whole read.
        let map = cat.books_by_ids(&[a, b, a, 9999]).unwrap();
        assert_eq!(map.len(), 2);
        assert_eq!(map[&a].title, "Dune");
        assert_eq!(map[&b].title, "Emma");
        assert!(!map.contains_key(&9999));

        // Tags survive the batch path (they come from a second query).
        let mut tags = map[&a].tags.clone();
        tags.sort();
        assert_eq!(tags, vec!["classic".to_string(), "scifi".to_string()]);
        assert!(map[&b].tags.is_empty());

        // It agrees with the one-at-a-time path it replaces.
        let single = cat.get_book(a).unwrap().unwrap();
        assert_eq!(single.title, map[&a].title);
        assert_eq!(single.file_path, map[&a].file_path);
    }

    #[test]
    fn books_by_ids_on_no_ids_is_not_an_error() {
        let cat = Catalog::open_in_memory().unwrap();
        assert!(cat.books_by_ids(&[]).unwrap().is_empty());
    }

    #[test]
    fn iso_days_ago_is_midnight_aligned_in_utc() {
        // With no offset, local midnight IS UTC midnight.
        let s = iso_days_ago_at(3, 1_760_000_000, 0);
        assert!(s.ends_with("T00:00:00Z"), "{s}");
    }

    #[test]
    fn iso_days_ago_snaps_to_local_midnight_not_utc_midnight() {
        // The bug this fixes. At UTC+05:30 the user's midnight is 18:30Z the
        // previous day, so a day window must START there -- snapping to
        // 00:00Z would include five and a half hours of the user's previous
        // day and exclude the same from today.
        let ist = 5 * 3600 + 1800;
        let s = iso_days_ago_at(0, 1_760_000_000, ist);
        assert!(
            s.ends_with("T18:30:00Z"),
            "expected local midnight expressed in UTC, got {s}"
        );

        // West of Greenwich the boundary moves the other way: UTC-05:00
        // means local midnight is 05:00Z the same day.
        let est = -5 * 3600;
        let s = iso_days_ago_at(0, 1_760_000_000, est);
        assert!(s.ends_with("T05:00:00Z"), "{s}");
    }

    #[test]
    fn a_local_day_label_is_the_users_date_not_utcs() {
        // 2025-10-09T08:53:20Z. At UTC+05:30 that is already 14:23 local on
        // the 9th -- same date here.
        let now = 1_760_000_000;
        let ist = 5 * 3600 + 1800;
        assert_eq!(local_day_ago_at(0, now, ist), "2025-10-09");

        // The case that mattered: 20:00Z on the 9th is 01:30 on the *10th*
        // in Kolkata. Bucketing by UTC filed that reading under the 9th and
        // is exactly how a late-night reader lost a streak.
        let late = 1_760_040_000; // 2025-10-09T20:00:00Z
        assert_eq!(
            local_day_ago_at(0, late, ist),
            "2025-10-10",
            "reading after local midnight belongs to the new local day"
        );
        assert_eq!(
            local_day_ago_at(0, late, 0),
            "2025-10-09",
            "and in UTC it would have been filed a day earlier -- the bug"
        );
    }

    #[test]
    fn local_day_labels_walk_backwards_one_day_at_a_time() {
        let now = 1_760_000_000;
        let ist = 5 * 3600 + 1800;
        assert_eq!(local_day_ago_at(1, now, ist), "2025-10-08");
        assert_eq!(local_day_ago_at(2, now, ist), "2025-10-07");
        // Across a month boundary.
        assert_eq!(local_day_ago_at(9, now, ist), "2025-09-30");
    }

    #[test]
    fn local_day_sql_produces_a_shifted_date_expression() {
        // The SQL text is generated, so assert its shape: a wrong sign here
        // would move every chart bar by a day and be invisible in review.
        let sql = local_day_sql("started_at");
        assert!(sql.starts_with("substr(datetime(started_at,"), "{sql}");
        assert!(sql.ends_with("), 1, 10)"), "{sql}");
        // The offset must carry an explicit sign for SQLite's modifier syntax.
        assert!(sql.contains('+') || sql.contains('-'), "{sql}");
    }

    #[test]
    fn local_day_sql_and_rust_agree_on_the_same_instant() {
        // The invariant that matters: the SQL bucket and the Rust label must
        // name the same day, or the zero-fill loop silently misses every bar.
        let conn = Connection::open_in_memory().unwrap();
        let offset = local_offset_seconds();
        let stamp = chrono_like_now();

        let sql = format!("SELECT {}", local_day_sql("?1"));
        let from_sql: String = conn.query_row(&sql, params![stamp], |r| r.get(0)).unwrap();
        let from_rust = local_day_ago_at(0, now_unix(), offset);
        assert_eq!(
            from_sql, from_rust,
            "SQL bucketing and Rust labelling disagree about today"
        );
    }

    #[test]
    fn streaks_use_local_days() {
        // Three consecutive local days, newest first, as `local_day_sql`
        // would return them.
        let days = vec![local_day_ago(0), local_day_ago(1), local_day_ago(2)];
        let (current, longest) = streaks(&days);
        assert_eq!(current, 3, "three consecutive days read is a 3-day streak");
        assert_eq!(longest, 3);
    }

    #[test]
    fn hinnant_date_math_roundtrips_accurately() {
        let test_cases = [
            (1970, 1, 1, 0i64),
            (1969, 12, 31, -1i64),
            (2000, 1, 1, 10957i64),
            (2024, 2, 29, 19782i64), // Leap day
            (1900, 2, 28, -25509i64), // Non-leap year century
            (2000, 2, 29, 11016i64), // Leap year century
        ];

        for &(y, m, d, expected_days) in &test_cases {
            let calculated_days = days_from_civil(y as i64, m as i64, d as i64);
            assert_eq!(
                calculated_days, expected_days,
                "days_from_civil failed for {y:04}-{m:02}-{d:02}"
            );

            let (cy, cm, cd) = civil_from_days(calculated_days);
            assert_eq!(
                (cy, cm, cd),
                (y, m, d),
                "civil_from_days failed for days={calculated_days}"
            );
        }
    }

    #[test]
    fn format_unix_utc_formats_timestamps() {
        assert_eq!(format_unix_utc(0), "1970-01-01T00:00:00Z");
        assert_eq!(format_unix_utc(1704067200), "2024-01-01T00:00:00Z");
        assert_eq!(format_unix_utc(1709210999), "2024-02-29T12:49:59Z");
    }

    #[test]
    fn days_from_iso_handles_valid_and_invalid_inputs() {
        assert_eq!(days_from_iso("1970-01-01"), Some(0));
        assert_eq!(days_from_iso("2024-02-29"), Some(19782));
        assert_eq!(days_from_iso("invalid"), None);
        assert_eq!(days_from_iso("2024-01"), None);
    }
}
