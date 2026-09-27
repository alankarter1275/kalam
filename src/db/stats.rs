//! Stats queries.
//!
//! Split out of a 3,400-line `db.rs` purely to make it navigable; these are
//! the same methods on the same `Catalog`, moved verbatim.

use super::*;

impl Catalog {
    // -----------------------------------------------------------------------
    // P4: Tags browse
    // -----------------------------------------------------------------------

    /// (tag name, book count), most used first.
    pub fn list_tags_with_counts(&self) -> Result<Vec<(String, i64)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT t.name, COUNT(bt.book_id) AS n
             FROM tags t
             LEFT JOIN book_tags bt ON bt.tag_id = t.id
             GROUP BY t.id
             HAVING n > 0
             ORDER BY n DESC, t.name COLLATE NOCASE ASC",
        )?;
        let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn books_with_tag(&self, tag: &str, sort: SortKey) -> Result<Vec<Book>> {
        let conn = self.conn();
        let order = match sort {
            SortKey::Title => "books.sort_title COLLATE NOCASE ASC",
            SortKey::Author => "books.authors COLLATE NOCASE ASC",
            SortKey::Added => "books.added_at DESC",
        };
        let sql = format!(
            "SELECT {BOOK_COLUMNS}
             FROM books
             JOIN book_tags bt ON bt.book_id = books.id
             JOIN tags t ON t.id = bt.tag_id
             WHERE t.name = ?1 COLLATE NOCASE
             ORDER BY {order}"
        );
        let mut stmt = conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(params![tag], row_to_book)?;
        let mut books = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        hydrate_books(&conn, &mut books)?;
        Ok(books)
    }

    // -----------------------------------------------------------------------
    // P4: Analytics
    // -----------------------------------------------------------------------

    /// Write a consistent copy of the catalog to `dest`.
    ///
    /// Uses SQLite's own VACUUM INTO, so the result is a defragmented, valid
    /// database even while the app is running — unlike copying the file, which
    /// can catch a half-written WAL.
    pub fn backup_to(&self, dest: &Path) -> Result<u64> {
        if dest.exists() {
            fs::remove_file(dest)?;
        }
        let conn = self.conn();
        // The path is interpolated because VACUUM INTO does not take a bound
        // parameter; single quotes are escaped to keep it safe.
        let escaped = dest.to_string_lossy().replace('\'', "''");
        conn.execute_batch(&format!("VACUUM INTO '{escaped}'"))?;
        drop(conn);
        Ok(fs::metadata(dest).map(|m| m.len()).unwrap_or(0))
    }

    /// SQLite's write counter. Any INSERT/UPDATE/DELETE bumps it, so callers
    /// can cheaply tell whether the catalog changed since they last looked.
    ///
    /// Callers must treat this as an **opaque token compared for equality**,
    /// never as a monotonically increasing number. `sqlite3_total_changes` is
    /// a C `int`, so after ~2.1 billion row changes it wraps to negative and
    /// keeps counting. Equality still behaves correctly across a wrap except
    /// for the single unlucky value that repeats, which costs one stale page
    /// and nothing else. Ordering comparisons (`>`), by contrast, would go
    /// permanently wrong — hence the warning rather than a fix.
    ///
    /// Reaching that count needs millions of writes per session, which only
    /// the dictionary importer produces; a book library never will.
    pub fn change_token(&self) -> i64 {
        self.conn
            .lock()
            .map(|c| c.total_changes() as i64)
            .unwrap_or(0)
    }

    pub fn library_stats(&self) -> Result<LibraryStats> {
        // Cheap: total_changes() is an in-memory counter, not a query.
        let version = {
            let conn = self.conn();
            conn.total_changes() as i64
        };

        if let Ok(cache) = self.stats_cache.lock() {
            if let Some((cached_version, stats)) = cache.as_ref() {
                if *cached_version == version {
                    return Ok(stats.clone());
                }
            }
        }

        let stats = self.compute_library_stats()?;
        if let Ok(mut cache) = self.stats_cache.lock() {
            *cache = Some((version, stats.clone()));
        }
        Ok(stats)
    }

    fn compute_library_stats(&self) -> Result<LibraryStats> {
        let conn = self.conn();

        let one =
            |sql: &str| -> i64 { conn.query_row(sql, [], |r| r.get::<_, i64>(0)).unwrap_or(0) };

        let total_books = one(
            "SELECT (SELECT COUNT(*) FROM books WHERE id NOT IN (SELECT book_id FROM comic_chapters)) + (SELECT COUNT(*) FROM comic_series)",
        );
        let finished = one(
            "SELECT (
                SELECT COUNT(*) FROM books 
                WHERE id NOT IN (SELECT book_id FROM comic_chapters) 
                  AND (IFNULL(finished_at,'') <> '' OR progress >= 100)
             ) + (
                SELECT COUNT(*) FROM comic_series s
                WHERE (SELECT COUNT(*) FROM comic_chapters cc WHERE cc.series_id = s.id) > 0
                  AND (
                      SELECT COUNT(*) FROM comic_chapters cc 
                      JOIN books b ON b.id = cc.book_id 
                      WHERE cc.series_id = s.id AND (b.progress < 100 OR IFNULL(b.finished_at,'') = '')
                  ) = 0
             )",
        );
        let reading = one(
            "SELECT (
                SELECT COUNT(*) FROM books 
                WHERE id NOT IN (SELECT book_id FROM comic_chapters) 
                  AND progress > 0 AND progress < 100 AND IFNULL(finished_at,'') = ''
             ) + (
                SELECT COUNT(*) FROM comic_series s
                WHERE EXISTS (
                    SELECT 1 FROM comic_chapters cc 
                    JOIN books b ON b.id = cc.book_id 
                    WHERE cc.series_id = s.id AND (b.progress > 0 OR b.last_opened_at IS NOT NULL)
                )
                AND EXISTS (
                    SELECT 1 FROM comic_chapters cc 
                    JOIN books b ON b.id = cc.book_id 
                    WHERE cc.series_id = s.id AND b.progress < 100 AND IFNULL(b.finished_at,'') = ''
                )
             )",
        );
        let unread = total_books.saturating_sub(finished).saturating_sub(reading);

        let mut s = LibraryStats {
            total_books,
            finished,
            reading,
            unread,
            highlights: one("SELECT COUNT(*) FROM annotations WHERE kind = 'highlight'"),
            quotes: one("SELECT COUNT(*) FROM annotations WHERE kind = 'quote'"),
            saved_words: one("SELECT COUNT(*) FROM saved_words"),
            shelves: one("SELECT COUNT(*) FROM shelves"),
            reading_list: one("SELECT COUNT(*) FROM reading_list"),
            total_seconds: one("SELECT IFNULL(SUM(seconds), 0) FROM reading_sessions"),
            sessions: one("SELECT COUNT(*) FROM reading_sessions WHERE seconds > 0"),
            ..LibraryStats::default()
        };

        let cutoff_7 = iso_days_ago(7);
        let cutoff_30 = iso_days_ago(30);
        s.seconds_last_7 = conn
            .query_row(
                "SELECT IFNULL(SUM(seconds), 0) FROM reading_sessions WHERE started_at >= ?1",
                params![cutoff_7],
                |r| r.get(0),
            )
            .unwrap_or(0);
        s.seconds_last_30 = conn
            .query_row(
                "SELECT IFNULL(SUM(seconds), 0) FROM reading_sessions WHERE started_at >= ?1",
                params![cutoff_30],
                |r| r.get(0),
            )
            .unwrap_or(0);
        s.finished_last_30 = conn
            .query_row(
                "SELECT COUNT(*) FROM reading_events WHERE kind = 'finished' AND at >= ?1",
                params![cutoff_30],
                |r| r.get(0),
            )
            .unwrap_or(0);
        s.added_last_30 = conn
            .query_row(
                "SELECT COUNT(*) FROM books WHERE added_at >= ?1",
                params![cutoff_30],
                |r| r.get(0),
            )
            .unwrap_or(0);

        // Books added per month, last 6 calendar months present in the data.
        {
            let mut stmt = conn.prepare_cached(
                "SELECT substr(added_at, 1, 7) AS ym, COUNT(*)
                 FROM books
                 GROUP BY ym
                 ORDER BY ym DESC
                 LIMIT 6",
            )?;
            // Month granularity: the local-vs-UTC boundary only matters for
            // books added within hours of a month change, and a month label
            // being off by one in that window is not worth a per-row
            // conversion over the whole table.
            let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))?;
            let mut months = rows.collect::<std::result::Result<Vec<_>, _>>()?;
            months.reverse();
            s.added_by_month = months;
        }

        // Reading minutes per day for the last 14 days (zero-filled).
        {
            // Bucketed by the user's local day, not UTC's -- see
            // `local_day_sql`. Not `prepare_cached`: the SQL embeds the
            // timezone offset, so caching it under a fixed key would be
            // wrong if the offset ever differed.
            let sql = format!(
                "SELECT {} AS d, IFNULL(SUM(seconds), 0)
                 FROM reading_sessions
                 WHERE started_at >= ?1
                 GROUP BY d",
                crate::db::local_day_sql("started_at")
            );
            let mut stmt = conn.prepare(&sql)?;
            let cutoff_14 = iso_days_ago(13);
            let rows = stmt.query_map(params![cutoff_14], |r| {
                Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
            })?;
            let found = rows.collect::<std::result::Result<Vec<_>, _>>()?;
            let mut series = Vec::with_capacity(14);
            for back in (0..14).rev() {
                // The local day label, matching what the query bucketed.
                let key = crate::db::local_day_ago(back);
                let secs = found
                    .iter()
                    .find(|(d, _)| *d == key)
                    .map(|(_, v)| *v)
                    .unwrap_or(0);
                series.push((key, secs));
            }
            s.minutes_by_day = series;
        }

        // Average over *active* days only — dividing by 30 when you read on 3 of
        // them reports a demoralising and fairly meaningless number.
        {
            let active_days_sql = format!(
                "SELECT IFNULL(SUM(seconds), 0),
                        COUNT(DISTINCT {})
                 FROM reading_sessions
                 WHERE started_at >= ?1 AND seconds > 0",
                crate::db::local_day_sql("started_at")
            );
            let (total, days): (i64, i64) = conn
                .query_row(&active_days_sql, params![cutoff_30], |r| {
                    Ok((r.get(0)?, r.get(1)?))
                })
                .unwrap_or((0, 0));
            s.avg_minutes_per_active_day = if days > 0 { total / days / 60 } else { 0 };
        }

        {
            let mut stmt = conn.prepare_cached(
                "SELECT t.name, COUNT(bt.book_id) AS n
                 FROM tags t JOIN book_tags bt ON bt.tag_id = t.id
                 GROUP BY t.id ORDER BY n DESC, t.name COLLATE NOCASE LIMIT 8",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            s.top_tags = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        }

        {
            let mut stmt = conn.prepare_cached(
                "SELECT authors, COUNT(*) AS n FROM books
                 WHERE TRIM(authors) <> ''
                 GROUP BY authors COLLATE NOCASE
                 ORDER BY n DESC, authors COLLATE NOCASE LIMIT 8",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            s.top_authors = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        }

        {
            let mut stmt = conn.prepare_cached(
                "SELECT books.title, SUM(rs.seconds) AS n
                 FROM reading_sessions rs JOIN books ON books.id = rs.book_id
                 GROUP BY rs.book_id
                 HAVING n > 0
                 ORDER BY n DESC LIMIT 5",
            )?;
            let rows = stmt.query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?;
            s.most_read = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        }

        // Streaks over distinct days that have any reading session.
        {
            // Local days: a streak is about the user's calendar. Bucketing
            // in UTC merged two local nights into one day east of Greenwich
            // and broke streaks the user had genuinely earned.
            let sql = format!(
                "SELECT DISTINCT {} AS d FROM reading_sessions
                 WHERE seconds > 0 ORDER BY d DESC",
                crate::db::local_day_sql("started_at")
            );
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map([], |r| r.get::<_, String>(0))?;
            let days = rows.collect::<std::result::Result<Vec<_>, _>>()?;
            let (current, longest) = streaks(&days);
            s.current_streak_days = current;
            s.longest_streak_days = longest;
        }

        Ok(s)
    }
}
