//! History queries.
//!
//! Split out of a 3,400-line `db.rs` purely to make it navigable; these are
//! the same methods on the same `Catalog`, moved verbatim.

use super::*;
use std::collections::HashSet;

/// A closed session row, for the book page's timeline.
#[derive(Debug, Clone)]
pub struct SessionRow {
    #[allow(dead_code)] // sessions are aggregated for stats; the row id is never shown
    pub id: i64,
    #[allow(dead_code)] // stats group by book title, not id
    pub book_id: i64,
    pub started_at: String,
    #[allow(dead_code)] // only the duration is displayed, not the end stamp
    pub ended_at: Option<String>,
    pub seconds: i64,
}

/// A closed session with its book identity — the library dashboard merges
/// sessions into its history feed.
#[derive(Debug, Clone)]
pub struct LibrarySession {
    #[allow(dead_code)] // history rows display title/at; the row id is unused
    pub id: i64,
    #[allow(dead_code)] // joined to the book for display; the id itself is unread
    pub book_id: i64,
    pub started_at: String,
    pub seconds: i64,
    pub end_pct: i64,
    pub book_title: String,
    #[allow(dead_code)] // author line not rendered on history rows yet
    pub book_authors: String,
}

impl Catalog {
    // -----------------------------------------------------------------------
    // P4: History (append-only event log)
    // -----------------------------------------------------------------------

    pub fn log_event(&self, book_id: i64, kind: EventKind, detail: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO reading_events (book_id, kind, at, detail) VALUES (?1, ?2, ?3, ?4)",
            params![book_id, kind.as_str(), chrono_like_now(), detail],
        )?;
        Ok(())
    }

    /// Stamp `books.last_opened_at` and log an `opened` event — but collapse
    /// repeat opens within the same hour so flipping in and out of the reader
    /// doesn't flood History.
    pub fn mark_book_opened(&self, book_id: i64) -> Result<()> {
        let conn = self.conn();
        let now = chrono_like_now();
        conn.execute(
            "UPDATE books SET last_opened_at = ?2 WHERE id = ?1",
            params![book_id, now],
        )?;

        let recent: Option<String> = conn
            .query_row(
                "SELECT at FROM reading_events
                 WHERE book_id = ?1 AND kind = 'opened'
                 ORDER BY at DESC LIMIT 1",
                params![book_id],
                |r| r.get(0),
            )
            .optional()?;

        // Timestamps are ISO-8601 UTC, so a 13-char prefix is the same hour.
        let same_hour = recent
            .as_deref()
            .map(|prev| prev.len() >= 13 && now.len() >= 13 && prev[..13] == now[..13])
            .unwrap_or(false);

        if !same_hour {
            conn.execute(
                "INSERT INTO reading_events (book_id, kind, at, detail) VALUES (?1, 'opened', ?2, '')",
                params![book_id, now],
            )?;
        }
        Ok(())
    }

    /// Mark finished/unfinished by hand. Finishing also pins progress to 100%
    /// and drops the book off the reading list.
    pub fn set_book_finished(&self, book_id: i64, finished: bool) -> Result<()> {
        {
            let conn = self.conn();
            let now = chrono_like_now();
            if finished {
                conn.execute(
                    "UPDATE books SET finished_at = ?2, progress = 100 WHERE id = ?1",
                    params![book_id, now],
                )?;
            } else {
                conn.execute(
                    "UPDATE books SET finished_at = NULL WHERE id = ?1",
                    params![book_id],
                )?;
            }
        }
        if finished {
            self.remove_from_reading_list(book_id)?;
        }
        self.log_event(
            book_id,
            if finished {
                EventKind::Finished
            } else {
                EventKind::Unfinished
            },
            "",
        )
    }

    /// Auto-finish when progress crosses the threshold, once per book.
    /// Returns true if this call flipped the book to finished.
    pub fn auto_finish_if_complete(&self, book_id: i64, progress_pct: i64) -> Result<bool> {
        if progress_pct < 99 {
            return Ok(false);
        }
        let already: Option<String> = {
            let conn = self.conn();
            conn.query_row(
                "SELECT finished_at FROM books WHERE id = ?1",
                params![book_id],
                |r| r.get(0),
            )
            .optional()?
            .flatten()
        };
        if already.is_some() {
            return Ok(false);
        }
        {
            let conn = self.conn();
            conn.execute(
                "UPDATE books SET finished_at = ?2 WHERE id = ?1",
                params![book_id, chrono_like_now()],
            )?;
        }
        self.remove_from_reading_list(book_id)?;
        self.log_event(book_id, EventKind::Finished, "auto")?;
        Ok(true)
    }

    /// Which of `ids` are marked finished, in one query.
    ///
    /// Callers rendering a list (the series panel) used to call
    /// [`Catalog::book_finished_at`] once per row.
    pub fn finished_book_ids(&self, ids: &[i64]) -> Result<HashSet<i64>> {
        let mut out = HashSet::new();
        if ids.is_empty() {
            return Ok(out);
        }
        let conn = self.conn();
        for chunk in ids.chunks(500) {
            let holders = vec!["?"; chunk.len()].join(",");
            let sql =
                format!("SELECT id FROM books WHERE finished_at IS NOT NULL AND id IN ({holders})");
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(rusqlite::params_from_iter(chunk.iter()), |r| {
                r.get::<_, i64>(0)
            })?;
            for id in rows {
                out.insert(id?);
            }
        }
        Ok(out)
    }

    pub fn book_finished_at(&self, book_id: i64) -> Result<Option<String>> {
        let conn = self.conn();
        let v: Option<Option<String>> = conn
            .query_row(
                "SELECT finished_at FROM books WHERE id = ?1",
                params![book_id],
                |r| r.get(0),
            )
            .optional()?;
        Ok(v.flatten())
    }

    /// Newest-first history, optionally filtered by kind and free text.
    pub fn list_events(
        &self,
        kind: Option<EventKind>,
        query: &str,
        limit: usize,
    ) -> Result<Vec<ReadingEvent>> {
        let conn = self.conn();
        let q = query.trim();
        let like = format!("%{}%", escape_like(q));
        let kind_str = kind.map(|k| k.as_str().to_string());
        let mut stmt = conn.prepare_cached(
            "SELECT e.id, e.book_id, e.kind, e.at, e.detail, books.title, books.authors
             FROM reading_events e
             JOIN books ON books.id = e.book_id
             WHERE (?1 IS NULL OR e.kind = ?1)
               AND (?2 = '' OR books.title LIKE ?3 ESCAPE '\\'
                            OR books.authors LIKE ?3 ESCAPE '\\')
             ORDER BY e.at DESC, e.id DESC
             LIMIT ?4",
        )?;
        let rows = stmt.query_map(params![kind_str, q, like, limit as i64], |r| {
            let kind: String = r.get(2)?;
            Ok(ReadingEvent {
                id: r.get(0)?,
                book_id: r.get(1)?,
                kind: EventKind::from_str_lossy(&kind),
                at: r.get(3)?,
                detail: r.get(4)?,
                book_title: r.get(5)?,
                book_authors: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn clear_history(&self) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM reading_events", [])?;
        Ok(())
    }

    /// Books ordered by most recently opened — powers Home → Continue.
    pub fn recently_opened(&self, limit: usize) -> Result<Vec<Book>> {
        let conn = self.conn();
        let sql = format!(
            "SELECT {BOOK_COLUMNS}
             FROM books
             WHERE books.last_opened_at IS NOT NULL
               AND IFNULL(books.finished_at, '') = ''
             ORDER BY books.last_opened_at DESC
             LIMIT ?1"
        );
        let mut stmt = conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(params![limit as i64], row_to_book)?;
        let mut books = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        hydrate_books(&conn, &mut books)?;
        Ok(books)
    }

    // -----------------------------------------------------------------------
    // P4: Reading sessions (time tracking)
    // -----------------------------------------------------------------------

    /// Open a session row when the reader mounts; returns its id.
    pub fn start_reading_session(&self, book_id: i64, start_pct: i64) -> Result<i64> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO reading_sessions (book_id, started_at, ended_at, seconds, start_pct, end_pct)
             VALUES (?1, ?2, NULL, 0, ?3, ?3)",
            params![book_id, chrono_like_now(), start_pct],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Close sessions that were never closed, because the process died.
    ///
    /// `start_reading_session` writes a row with `ended_at = NULL, seconds =
    /// 0` and the reader's `shutdown()` fills it in. `shutdown()` does not run
    /// if the app is killed, panics, or the machine loses power — so the row
    /// stays open for ever. Nothing reaped these, so they accumulated for the
    /// life of the database.
    ///
    /// Two things went wrong because of that. The book page counted a session
    /// that contributed no time. Worse, the streak query filters
    /// `seconds > 0`, so a crash silently *broke a streak the user had
    /// actually earned* — the one number in the app people care about
    /// defending.
    ///
    /// Any row still open when the app starts belongs to a session that ended
    /// when the previous process did, so `ended_at` is set to `started_at`.
    /// `seconds` is deliberately left alone: the honest answer to "how long
    /// was that session" is whatever was last checkpointed, and inventing a
    /// duration would put fictional minutes into the statistics. Returns how
    /// many rows were closed.
    pub fn close_orphaned_sessions(&self) -> Result<usize> {
        let conn = self.conn();
        let n = conn.execute(
            "UPDATE reading_sessions
             SET ended_at = started_at
             WHERE ended_at IS NULL",
            [],
        )?;
        Ok(n)
    }

    /// Update a live session's elapsed time without closing it.
    ///
    /// Checkpointing while reading is what makes a crash cost seconds instead
    /// of the whole session. Called from the reader's progress tick, which the
    /// JS bridge already throttles to 1% of scroll movement, so this is a
    /// cheap single-row update at a human rate rather than per frame.
    ///
    /// `ended_at` stays NULL: the session is still open, and leaving it NULL
    /// is what lets [`close_orphaned_sessions`] recognise it after a crash.
    pub fn checkpoint_reading_session(
        &self,
        session_id: i64,
        seconds: i64,
        pct: i64,
    ) -> Result<()> {
        let conn = self.conn();
        let clamped = seconds.clamp(0, MAX_SESSION_SECONDS);
        conn.execute(
            "UPDATE reading_sessions SET seconds = ?2, end_pct = ?3 WHERE id = ?1",
            params![session_id, clamped, pct],
        )?;
        Ok(())
    }

    /// Close a session. Absurd durations are clamped so one forgotten window
    /// can't claim 14 hours read.
    ///
    /// Note on what the clamp is really for: `session_start.elapsed()` is an
    /// `Instant`, i.e. `CLOCK_MONOTONIC` on Linux, which does **not** advance
    /// while the machine is suspended. So this does not catch "laptop
    /// suspended with the reader open" (an earlier comment here claimed it
    /// did). What it catches is a window genuinely left open for hours while
    /// the machine is awake, which is the common case anyway.
    pub fn end_reading_session(&self, session_id: i64, seconds: i64, end_pct: i64) -> Result<()> {
        let conn = self.conn();
        let clamped = seconds.clamp(0, MAX_SESSION_SECONDS);
        conn.execute(
            "UPDATE reading_sessions SET ended_at = ?2, seconds = ?3, end_pct = ?4 WHERE id = ?1",
            params![session_id, chrono_like_now(), clamped, end_pct],
        )?;
        Ok(())
    }

    pub fn total_reading_seconds(&self, book_id: i64) -> Result<i64> {
        let conn = self.conn();
        let n: i64 = conn.query_row(
            "SELECT IFNULL(SUM(seconds), 0) FROM reading_sessions WHERE book_id = ?1",
            params![book_id],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    // -----------------------------------------------------------------------
    // P5.5: per-book stats for the book detail page
    // -----------------------------------------------------------------------

    /// How many reading sessions this book has, for the stats tile.
    pub fn count_sessions_for_book(&self, book_id: i64) -> Result<i64> {
        let conn = self.conn();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM reading_sessions WHERE book_id = ?1",
            params![book_id],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    /// Seconds read per **local** day for the last `days` days, oldest first.
    ///
    /// Missing days are filled with 0 so the chart always has `days` bars.
    /// Used by the "Last 7 days" mini chart on the book page.
    pub fn book_seconds_by_day(&self, book_id: i64, days: i64) -> Result<Vec<(String, i64)>> {
        let days = days.clamp(1, 30);
        let since = iso_days_ago(days - 1);
        let conn = self.conn();

        // Local day buckets -- see `crate::db::local_day_sql`. Not cached:
        // the SQL carries the timezone offset.
        let sql = format!(
            "SELECT {} AS day, SUM(seconds) AS total
             FROM reading_sessions
             WHERE book_id = ?1 AND started_at >= ?2
             GROUP BY day",
            crate::db::local_day_sql("started_at")
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params![book_id, since], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;

        let mut by_day: std::collections::HashMap<String, i64> = std::collections::HashMap::new();
        for row in rows {
            let (day, total) = row?;
            by_day.insert(day, total);
        }

        // Walk the window oldest → newest, filling gaps.
        let mut out = Vec::with_capacity(days as usize);
        for ago in (0..days).rev() {
            // The local day label, matching the buckets above.
            let key = crate::db::local_day_ago(ago);
            let total = by_day.remove(&key).unwrap_or(0);
            out.push((key, total));
        }
        Ok(out)
    }

    /// The `limit` most recent sessions for one book, newest first.
    pub fn book_recent_sessions(&self, book_id: i64, limit: usize) -> Result<Vec<SessionRow>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, book_id, started_at, ended_at, seconds
             FROM reading_sessions
             WHERE book_id = ?1 AND ended_at IS NOT NULL
             ORDER BY started_at DESC, id DESC
             LIMIT ?2",
        )?;
        let rows = stmt.query_map(params![book_id, limit as i64], |r| {
            Ok(SessionRow {
                id: r.get(0)?,
                book_id: r.get(1)?,
                started_at: r.get(2)?,
                ended_at: r.get(3)?,
                seconds: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Newest closed sessions across all books, with book identity — the
    /// library dashboard's history feed merges these with the event log.
    pub fn recent_sessions(&self, limit: usize) -> Result<Vec<LibrarySession>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT s.id, s.book_id, s.started_at, s.seconds, s.end_pct,
                    books.title, books.authors
             FROM reading_sessions s
             JOIN books ON books.id = s.book_id
             WHERE s.ended_at IS NOT NULL
             ORDER BY s.started_at DESC, s.id DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok(LibrarySession {
                id: r.get(0)?,
                book_id: r.get(1)?,
                started_at: r.get(2)?,
                seconds: r.get(3)?,
                end_pct: r.get(4)?,
                book_title: r.get(5)?,
                book_authors: r.get(6)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// When the book was first opened, if ever. Drives the "First opened"
    /// timeline entry.
    pub fn book_first_opened(&self, book_id: i64) -> Result<Option<String>> {
        let conn = self.conn();
        // `MIN(at)` over zero rows still returns one row, containing NULL, so
        // `.optional()` does not help here: the value itself must be nullable.
        // Reading it as a plain String made "never opened" a hard error, which
        // the callers used to hide with `.ok().flatten()`.
        let row: Option<String> = conn.query_row(
            "SELECT MIN(at) FROM reading_events
                 WHERE book_id = ?1 AND kind = 'opened'",
            params![book_id],
            |r| r.get(0),
        )?;
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::MAX_SESSION_SECONDS;
    use crate::db::Catalog;
    use crate::models::BookFormat;

    fn catalog_with_book() -> (Catalog, i64) {
        let cat = Catalog::open_in_memory().expect("in-memory catalog");
        let id = cat
            .insert_book(
                "uuid-session-test",
                "A Book",
                "An Author",
                None,
                "",
                BookFormat::Epub,
                "book.epub",
                "hash-session-test",
                None,
                &[],
            )
            .expect("insert book");
        (cat, id)
    }

    #[test]
    fn an_unclosed_session_is_reaped_at_startup() {
        // The crash case: `start_reading_session` wrote the row, the process
        // died, `end_reading_session` never ran.
        let (cat, book_id) = catalog_with_book();
        let session = cat.start_reading_session(book_id, 0).expect("start");

        let open_before: i64 = cat
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM reading_sessions WHERE ended_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(open_before, 1, "the session should start out open");

        let reaped = cat.close_orphaned_sessions().expect("reap");
        assert_eq!(reaped, 1);

        let (ended, started): (Option<String>, String) = cat
            .conn()
            .query_row(
                "SELECT ended_at, started_at FROM reading_sessions WHERE id = ?1",
                [session],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(
            ended.as_deref(),
            Some(started.as_str()),
            "a reaped session ends when it started -- we cannot know better, \
             and inventing a duration would put fictional minutes into the stats"
        );
    }

    #[test]
    fn reaping_leaves_properly_closed_sessions_alone() {
        let (cat, book_id) = catalog_with_book();
        let session = cat.start_reading_session(book_id, 0).expect("start");
        cat.end_reading_session(session, 120, 10).expect("end");

        assert_eq!(
            cat.close_orphaned_sessions().expect("reap"),
            0,
            "a closed session must not be touched"
        );
        assert_eq!(
            cat.total_reading_seconds(book_id).expect("total"),
            120,
            "reaping must not alter recorded time"
        );
    }

    #[test]
    fn a_checkpoint_preserves_time_across_a_crash() {
        // The whole point of checkpointing: the seconds survive even though
        // nothing ever called `end_reading_session`.
        let (cat, book_id) = catalog_with_book();
        let session = cat.start_reading_session(book_id, 0).expect("start");
        cat.checkpoint_reading_session(session, 95, 42)
            .expect("checkpoint");

        // Still open, so the reaper can still find it.
        let open: i64 = cat
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM reading_sessions WHERE ended_at IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(open, 1, "a checkpoint must not close the session");

        cat.close_orphaned_sessions().expect("reap");
        assert_eq!(
            cat.total_reading_seconds(book_id).expect("total"),
            95,
            "checkpointed time must survive the crash and the reap"
        );
    }

    #[test]
    fn a_checkpoint_is_clamped_like_a_close() {
        let (cat, book_id) = catalog_with_book();
        let session = cat.start_reading_session(book_id, 0).expect("start");
        // A week. Same absurd-duration guard as `end_reading_session`.
        cat.checkpoint_reading_session(session, 7 * 24 * 60 * 60, 50)
            .expect("checkpoint");
        assert_eq!(
            cat.total_reading_seconds(book_id).expect("total"),
            MAX_SESSION_SECONDS,
            "an absurd checkpoint must be clamped, not recorded"
        );
    }
}
