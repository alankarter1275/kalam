//! Shelves queries.
//!
//! Split out of a 3,400-line `db.rs` purely to make it navigable; these are
//! the same methods on the same `Catalog`, moved verbatim.

use super::*;

/// The display key for one book row in a collapsed count: registered comics
/// key by their series id, unregistered comics by the heuristic series key,
/// every other book by its own id. This mirrors `collapse_comic_chapters`
/// exactly, so a count computed here can never disagree with the cards the
/// collapsed grids show.
fn collapse_count_key(
    id: i64,
    format: &str,
    series: Option<&str>,
    title: &str,
    series_id: Option<i64>,
) -> String {
    if let Some(sid) = series_id {
        return format!("s{sid}");
    }
    if format == "CBZ" || format == "CBR" {
        return format!("h{}", heuristic_series_key(title, series));
    }
    format!("b{id}")
}

impl Catalog {
    // -----------------------------------------------------------------------
    // P4: Shelves
    // -----------------------------------------------------------------------

    /// All shelves ordered by position, each with a live book count.
    pub fn list_shelves(&self) -> Result<Vec<Shelf>> {
        let mut shelves = {
            let conn = self.conn();
            let mut stmt = conn.prepare_cached(
                "SELECT id, name, kind, description, rules, position, created_at, updated_at
                 FROM shelves
                 ORDER BY position ASC, name COLLATE NOCASE ASC",
            )?;
            let rows = stmt.query_map([], row_to_shelf)?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };

        // Manual counts come back in a single grouped query; only smart
        // shelves need their rules compiled and counted individually. Counts
        // are collapsed — one card per comic series, the same key the shelf
        // page displays — so a 700-chapter series never reads "701 books".
        let manual_counts: std::collections::HashMap<i64, usize> = {
            let conn = self.conn();
            let mut stmt = conn.prepare_cached(
                "SELECT sb.shelf_id, books.id, books.format, books.series, books.title,
                        cc.series_id
                 FROM shelf_books sb
                 JOIN books ON books.id = sb.book_id
                 LEFT JOIN comic_chapters cc ON cc.book_id = sb.book_id",
            )?;
            let rows = stmt.query_map(
                [],
                |r| {
                    Ok((
                        r.get::<_, i64>(0)?,
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, Option<String>>(3)?,
                        r.get::<_, String>(4)?,
                        r.get::<_, Option<i64>>(5)?,
                    ))
                },
            )?;
            let mut per_shelf: std::collections::HashMap<i64, std::collections::HashSet<String>> =
                std::collections::HashMap::new();
            for row in rows {
                let (shelf_id, id, format, series, title, series_id) = row?;
                per_shelf
                    .entry(shelf_id)
                    .or_default()
                    .insert(collapse_count_key(id, &format, series.as_deref(), &title, series_id));
            }
            per_shelf.into_iter().map(|(k, v)| (k, v.len())).collect()
        };

        for shelf in &mut shelves {
            shelf.book_count = match shelf.kind {
                ShelfKind::Manual => manual_counts.get(&shelf.id).copied().unwrap_or(0),
                ShelfKind::Smart => self.shelf_book_count(shelf).unwrap_or(0),
            };
        }
        Ok(shelves)
    }

    pub fn get_shelf(&self, id: i64) -> Result<Option<Shelf>> {
        let shelf = {
            let conn = self.conn();
            conn.query_row(
                "SELECT id, name, kind, description, rules, position, created_at, updated_at
                 FROM shelves WHERE id = ?1",
                params![id],
                row_to_shelf,
            )
            .optional()?
        };
        let Some(mut shelf) = shelf else {
            return Ok(None);
        };
        shelf.book_count = self.shelf_book_count(&shelf).unwrap_or(0);
        Ok(Some(shelf))
    }

    pub fn create_shelf(
        &self,
        name: &str,
        kind: ShelfKind,
        description: &str,
        rules: &str,
    ) -> Result<i64> {
        let conn = self.conn();
        let now = chrono_like_now();
        let next_pos: i64 = conn
            .query_row(
                "SELECT IFNULL(MAX(position), -1) + 1 FROM shelves",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        conn.execute(
            "INSERT INTO shelves (name, kind, description, rules, position, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?6)",
            params![
                name.trim(),
                kind.as_str(),
                description,
                rules,
                next_pos,
                now
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn update_shelf(&self, id: i64, name: &str, description: &str, rules: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE shelves SET name = ?2, description = ?3, rules = ?4, updated_at = ?5
             WHERE id = ?1",
            params![id, name.trim(), description, rules, chrono_like_now()],
        )?;
        Ok(())
    }

    pub fn delete_shelf(&self, id: i64) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM shelves WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// True when a shelf with this name already exists (case-insensitive).
    /// `except_id` lets the edit dialog ignore the shelf being renamed.
    pub fn shelf_name_taken(&self, name: &str, except_id: Option<i64>) -> Result<bool> {
        let conn = self.conn();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM shelves
             WHERE name = ?1 COLLATE NOCASE AND id <> IFNULL(?2, -1)",
            params![name.trim(), except_id],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    /// Books on a shelf: membership rows for manual, compiled rules for smart.
    pub fn shelf_books(&self, shelf: &Shelf, sort: SortKey, query: &str) -> Result<Vec<Book>> {
        match shelf.kind {
            ShelfKind::Manual => self.manual_shelf_books(shelf.id, sort, query),
            ShelfKind::Smart => self.smart_shelf_books(shelf, sort, query),
        }
    }

    fn manual_shelf_books(&self, shelf_id: i64, sort: SortKey, query: &str) -> Result<Vec<Book>> {
        let conn = self.conn();
        // Manual shelves keep hand-sorted order unless the user picks a sort.
        let order = match sort {
            SortKey::Title => "books.sort_title COLLATE NOCASE ASC",
            SortKey::Author => {
                "books.authors COLLATE NOCASE ASC, books.sort_title COLLATE NOCASE ASC"
            }
            SortKey::Added => "sb.position ASC, sb.added_at ASC",
        };
        let q = query.trim();
        let sql = format!(
            "SELECT {BOOK_COLUMNS}
             FROM books
             JOIN shelf_books sb ON sb.book_id = books.id
             WHERE sb.shelf_id = ?1
               AND (?2 = '' OR books.title LIKE ?3 ESCAPE '\\'
                            OR books.authors LIKE ?3 ESCAPE '\\')
             ORDER BY {order}"
        );
        let like = format!("%{}%", escape_like(q));
        let mut stmt = conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(params![shelf_id, q, like], row_to_book)?;
        let mut books = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        hydrate_books(&conn, &mut books)?;
        Ok(books)
    }

    fn smart_shelf_books(&self, shelf: &Shelf, sort: SortKey, query: &str) -> Result<Vec<Book>> {
        let (where_sql, rule_params) = shelf.rule_set().to_sql();
        let conn = self.conn();
        let order = match sort {
            SortKey::Title => "books.sort_title COLLATE NOCASE ASC",
            SortKey::Author => {
                "books.authors COLLATE NOCASE ASC, books.sort_title COLLATE NOCASE ASC"
            }
            SortKey::Added => "books.added_at DESC",
        };
        let q = query.trim();
        // Compiled rules use anonymous `?` placeholders, so the whole statement
        // must stay positional — mixing `?N` here would collide with them.
        let sql = format!(
            "SELECT {BOOK_COLUMNS}
             FROM books
             WHERE ({where_sql})
               AND (? = '' OR books.title LIKE ? ESCAPE '\\'
                           OR books.authors LIKE ? ESCAPE '\\')
             ORDER BY {order}"
        );

        let like = format!("%{}%", escape_like(q));
        // Bind order follows placeholder order in the SQL text: rules first.
        let mut bound: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(rule_params.len() + 3);
        for p in &rule_params {
            bound.push(p);
        }
        bound.push(&q);
        bound.push(&like);
        bound.push(&like);

        // Not cached: the WHERE clause is generated from this shelf's rules.
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(bound.as_slice(), row_to_book)?;
        let mut books = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        hydrate_books(&conn, &mut books)?;
        Ok(books)
    }

    /// The shelf badge count: what the collapsed shelf page displays, not the
    /// raw membership row count (a comic series is one card, not one per
    /// chapter).
    fn shelf_book_count(&self, shelf: &Shelf) -> Result<usize> {
        match shelf.kind {
            ShelfKind::Manual => {
                let sid = shelf.id;
                self.collapsed_count_where(
                    "books.id IN (SELECT book_id FROM shelf_books WHERE shelf_id = ?1)",
                    &[&sid as &dyn rusqlite::ToSql],
                )
            }
            ShelfKind::Smart => {
                let (where_sql, rule_params) = shelf.rule_set().to_sql();
                let bound: Vec<&dyn rusqlite::ToSql> = rule_params
                    .iter()
                    .map(|p| p as &dyn rusqlite::ToSql)
                    .collect();
                self.collapsed_count_where(&where_sql, bound.as_slice())
            }
        }
    }

    /// Count the cards a collapsed list would show for the books matching
    /// `where_sql` — one per book, one per comic series. Rule-derived SQL
    /// differs per call, so the statement is not cached.
    fn collapsed_count_where(
        &self,
        where_sql: &str,
        params: &[&dyn rusqlite::ToSql],
    ) -> Result<usize> {
        let conn = self.conn();
        let sql = format!(
            "SELECT books.id, books.format, books.series, books.title, cc.series_id
             FROM books
             LEFT JOIN comic_chapters cc ON cc.book_id = books.id
             WHERE {where_sql}"
        );
        let mut stmt = conn.prepare(&sql)?;
        let rows = stmt.query_map(params, |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, Option<i64>>(4)?,
            ))
        })?;
        let mut seen = std::collections::HashSet::new();
        for row in rows {
            let (id, format, series, title, series_id) = row?;
            seen.insert(collapse_count_key(id, &format, series.as_deref(), &title, series_id));
        }
        Ok(seen.len())
    }

    /// Count matches for an unsaved rule set — powers the live count in the
    /// editor. Collapsed, like every book count the UI shows.
    pub fn count_matching_rules(&self, rules: &crate::shelf_rules::RuleSet) -> Result<usize> {
        let (where_sql, rule_params) = rules.to_sql();
        let bound: Vec<&dyn rusqlite::ToSql> = rule_params
            .iter()
            .map(|p| p as &dyn rusqlite::ToSql)
            .collect();
        self.collapsed_count_where(&where_sql, bound.as_slice())
    }

    pub fn add_book_to_shelf(&self, shelf_id: i64, book_id: i64) -> Result<()> {
        let conn = self.conn();
        let next_pos: i64 = conn
            .query_row(
                "SELECT IFNULL(MAX(position), -1) + 1 FROM shelf_books WHERE shelf_id = ?1",
                params![shelf_id],
                |r| r.get(0),
            )
            .unwrap_or(0);
        conn.execute(
            "INSERT OR IGNORE INTO shelf_books (shelf_id, book_id, position, added_at)
             VALUES (?1, ?2, ?3, ?4)",
            params![shelf_id, book_id, next_pos, chrono_like_now()],
        )?;
        Ok(())
    }

    pub fn remove_book_from_shelf(&self, shelf_id: i64, book_id: i64) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM shelf_books WHERE shelf_id = ?1 AND book_id = ?2",
            params![shelf_id, book_id],
        )?;
        Ok(())
    }

    /// Every comic chapter that belongs to the same series as `book_id`
    /// (including it). A non-comic is its own peer. The collapsed grids show
    /// one card per series, so removal from a shelf or the reading list must
    /// clear every chapter at once — removing only the representative would
    /// leave the card standing on its remaining chapters.
    pub fn comic_series_peers(&self, book_id: i64) -> Result<Vec<i64>> {
        let conn = self.conn();

        // Registered series: peers are the chapters sharing this book's
        // comic_chapters row. Mixed registration (some chapters registered,
        // some not) does not occur — import registers every cbz/cbr it files.
        let registered: Vec<i64> = {
            let mut stmt = conn.prepare_cached(
                "SELECT cc2.book_id
                 FROM comic_chapters cc2
                 WHERE cc2.series_id =
                       (SELECT series_id FROM comic_chapters WHERE book_id = ?1)",
            )?;
            let rows = stmt.query_map(params![book_id], |r| r.get(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        if !registered.is_empty() {
            return Ok(registered);
        }

        // Unregistered comics: key them exactly the way
        // `collapse_comic_chapters` does — the series column, else the series
        // parsed out of the title, else the title itself.
        let (format, title, series): (String, String, Option<String>) = conn.query_row(
            "SELECT format, title, series FROM books WHERE id = ?1",
            params![book_id],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )?;
        if format != "CBZ" && format != "CBR" {
            return Ok(vec![book_id]);
        }
        let key = crate::db::heuristic_series_key(&title, series.as_deref());
        let mut stmt = conn.prepare_cached(
            "SELECT id, title, series FROM books
             WHERE format IN ('CBZ','CBR')
               AND id NOT IN (SELECT book_id FROM comic_chapters)",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((r.get::<_, i64>(0)?, r.get::<_, String>(1)?, r.get::<_, Option<String>>(2)?))
        })?;
        let mut peers = Vec::new();
        for row in rows {
            let (id, t, s) = row?;
            if crate::db::heuristic_series_key(&t, s.as_deref()) == key {
                peers.push(id);
            }
        }
        if peers.is_empty() {
            peers.push(book_id);
        }
        Ok(peers)
    }

    /// `comic_series_peers` for many books at once — the pickers called the
    /// single-book form once per collapsed row, which is exactly the N+1 the
    /// step-4 budgets exist to catch. Returns the peers of every requested
    /// id that belongs to a series (registered or heuristic); ids missing
    /// from the map are their own peer — callers fall back to `vec![id]`,
    /// matching the single-book form's "a non-comic is its own peer".
    ///
    /// Same shape as `books_by_ids`/`reading_progress_by_ids`: dedupe, chunk
    /// at 500, empty input issues nothing. Registered chapters resolve in one
    /// chunked query; the unregistered heuristic path runs at most twice more
    /// for the whole batch (the requested rows, then the one scan of all
    /// unregistered comics the single-book form did per book).
    pub fn comic_series_peers_by_ids(
        &self,
        book_ids: &[i64],
    ) -> Result<HashMap<i64, Vec<i64>>> {
        let mut out: HashMap<i64, Vec<i64>> = HashMap::new();
        if book_ids.is_empty() {
            return Ok(out);
        }
        let mut unique: Vec<i64> = book_ids.to_vec();
        unique.sort_unstable();
        unique.dedup();

        let conn = self.conn();

        // Registered: one chunked self-join — every requested chapter maps to
        // every chapter sharing its series (including itself).
        let mut unregistered: Vec<i64> = Vec::new();
        for chunk in unique.chunks(500) {
            let holders = vec!["?"; chunk.len()].join(",");
            let sql = format!(
                "SELECT cc1.book_id, cc2.book_id
                 FROM comic_chapters cc1
                 JOIN comic_chapters cc2 ON cc2.series_id = cc1.series_id
                 WHERE cc1.book_id IN ({holders})"
            );
            let mut stmt = conn.prepare(&sql)?;
            let params = rusqlite::params_from_iter(chunk.iter());
            let rows = stmt.query_map(params, |r| {
                Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (asked, peer) = row?;
                out.entry(asked).or_default().push(peer);
            }
        }
        for id in &unique {
            if !out.contains_key(id) {
                unregistered.push(*id);
            }
        }
        if unregistered.is_empty() {
            return Ok(out);
        }

        // Unregistered comics (and any non-comic that reached the batch):
        // key them exactly the way `comic_series_peers` does. Their own rows
        // first, so a non-comic can be answered without the scan.
        let holders = vec!["?"; unregistered.len()].join(",");
        let sql = format!(
            "SELECT id, format, title, series FROM books WHERE id IN ({holders})"
        );
        let mut stmt = conn.prepare(&sql)?;
        let params = rusqlite::params_from_iter(unregistered.iter());
        let rows = stmt.query_map(params, |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, Option<String>>(3)?,
            ))
        })?;
        let mut comic_keys: HashMap<i64, String> = HashMap::new();
        for row in rows {
            let (id, format, title, series) = row?;
            if format != "CBZ" && format != "CBR" {
                // A non-comic is its own peer; the single-book form returns
                // it directly, so record it and skip the heuristic scan.
                out.insert(id, vec![id]);
            } else {
                comic_keys.insert(
                    id,
                    crate::db::heuristic_series_key(&title, series.as_deref()),
                );
            }
        }
        if comic_keys.is_empty() {
            return Ok(out);
        }

        // One scan of all unregistered comics, grouped by heuristic key —
        // the same scan the single-book form ran once per book.
        let mut stmt = conn.prepare_cached(
            "SELECT id, title, series FROM books
             WHERE format IN ('CBZ','CBR')
               AND id NOT IN (SELECT book_id FROM comic_chapters)",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, Option<String>>(2)?,
            ))
        })?;
        let mut groups: HashMap<String, Vec<i64>> = HashMap::new();
        for row in rows {
            let (id, title, series) = row?;
            groups
                .entry(crate::db::heuristic_series_key(&title, series.as_deref()))
                .or_default()
                .push(id);
        }
        for (id, key) in comic_keys {
            // The requested book is itself in the scan's result set, so its
            // group always contains at least itself.
            if let Some(peers) = groups.remove(&key) {
                out.insert(id, peers);
            }
        }
        Ok(out)
    }

    /// Remove a comic series (every chapter) from a manual shelf. Callers pass
    /// the representative the collapsed list shows.
    pub fn remove_series_from_shelf(&self, shelf_id: i64, book_id: i64) -> Result<()> {
        let peers = self.comic_series_peers(book_id)?;
        let conn = self.conn();
        let holders = vec!["?"; peers.len()].join(",");
        let sql = format!(
            "DELETE FROM shelf_books
             WHERE shelf_id = ?1 AND book_id IN ({holders})"
        );
        let mut bound: Vec<&dyn rusqlite::ToSql> = Vec::with_capacity(peers.len() + 1);
        bound.push(&shelf_id);
        for p in &peers {
            bound.push(p);
        }
        conn.execute(&sql, bound.as_slice())?;
        Ok(())
    }

    /// Remove a comic series (every chapter) from the reading list.
    pub fn remove_series_from_reading_list(&self, book_id: i64) -> Result<()> {
        let peers = self.comic_series_peers(book_id)?;
        let conn = self.conn();
        let holders = vec!["?"; peers.len()].join(",");
        let sql = format!("DELETE FROM reading_list WHERE book_id IN ({holders})");
        conn.execute(&sql, rusqlite::params_from_iter(peers.iter()))?;
        Ok(())
    }

    /// Add every chapter of a comic series to a manual shelf in one
    /// transaction — hundreds of chapters, one commit, positions continuing
    /// the shelf's order. For a non-comic this is exactly
    /// `add_book_to_shelf`.
    pub fn add_series_to_shelf(&self, shelf_id: i64, book_id: i64) -> Result<()> {
        let peers = self.comic_series_peers(book_id)?;
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let base: i64 = tx.query_row(
            "SELECT IFNULL(MAX(position), -1) + 1 FROM shelf_books WHERE shelf_id = ?1",
            params![shelf_id],
            |r| r.get(0),
        )?;
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR IGNORE INTO shelf_books (shelf_id, book_id, position, added_at)
                 VALUES (?1, ?2, ?3, ?4)",
            )?;
            for (i, id) in peers.iter().enumerate() {
                stmt.execute(params![shelf_id, id, base + i as i64, chrono_like_now()])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Add every chapter of a comic series to the reading list in one
    /// transaction. For a non-comic this is exactly `add_to_reading_list`.
    pub fn add_series_to_reading_list(&self, book_id: i64) -> Result<()> {
        let peers = self.comic_series_peers(book_id)?;
        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let base: i64 = tx
            .query_row(
                "SELECT IFNULL(MAX(position), -1) + 1 FROM reading_list",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        {
            let mut stmt = tx.prepare_cached(
                "INSERT OR IGNORE INTO reading_list (book_id, position, note, added_at)
                 VALUES (?1, ?2, '', ?3)",
            )?;
            for (i, id) in peers.iter().enumerate() {
                stmt.execute(params![id, base + i as i64, chrono_like_now()])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    /// Is any of these books in the reading list? One query for the whole
    /// set — the collapsed picker asks once per series, and a series can be
    /// hundreds of chapters.
    pub fn any_in_reading_list(&self, book_ids: &[i64]) -> Result<bool> {
        if book_ids.is_empty() {
            return Ok(false);
        }
        let conn = self.conn();
        let holders = vec!["?"; book_ids.len()].join(",");
        let sql =
            format!("SELECT EXISTS(SELECT 1 FROM reading_list WHERE book_id IN ({holders}))");
        let found: i64 = conn.query_row(
            &sql,
            rusqlite::params_from_iter(book_ids.iter()),
            |r| r.get(0),
        )?;
        Ok(found != 0)
    }

    /// Every book id on the reading list — the membership half of the
    /// reading-list picker's read, in one query instead of one
    /// `any_in_reading_list` per row.
    pub fn reading_list_book_ids(&self) -> Result<Vec<i64>> {
        let conn = self.conn();
        let mut stmt =
            conn.prepare_cached("SELECT book_id FROM reading_list")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Every book id on one shelf — the membership half of the shelf
    /// picker's read, in one query (no book hydration, no join: the picker
    /// only needs the set).
    pub fn shelf_book_ids(&self, shelf_id: i64) -> Result<Vec<i64>> {
        let conn = self.conn();
        let mut stmt = conn
            .prepare_cached("SELECT book_id FROM shelf_books WHERE shelf_id = ?1")?;
        let rows = stmt.query_map(params![shelf_id], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Manual shelves this book belongs to (id, name) — for the book page chips.
    pub fn shelves_for_book(&self, book_id: i64) -> Result<Vec<(i64, String)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT s.id, s.name FROM shelves s
             JOIN shelf_books sb ON sb.shelf_id = s.id
             WHERE sb.book_id = ?1
             ORDER BY s.name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![book_id], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Move a manual shelf entry up/down by swapping positions with its neighbour.
    pub fn move_shelf_book(&self, shelf_id: i64, book_id: i64, delta: i64) -> Result<()> {
        let conn = self.conn();
        let ids: Vec<i64> = {
            let mut stmt = conn.prepare_cached(
                "SELECT book_id FROM shelf_books WHERE shelf_id = ?1
                 ORDER BY position ASC, added_at ASC",
            )?;
            let rows = stmt.query_map(params![shelf_id], |r| r.get(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        let Some(idx) = ids.iter().position(|id| *id == book_id) else {
            return Ok(());
        };
        let target = idx as i64 + delta;
        if target < 0 || target as usize >= ids.len() {
            return Ok(());
        }
        let mut reordered = ids.clone();
        reordered.swap(idx, target as usize);
        for (pos, id) in reordered.iter().enumerate() {
            conn.execute(
                "UPDATE shelf_books SET position = ?3 WHERE shelf_id = ?1 AND book_id = ?2",
                params![shelf_id, id, pos as i64],
            )?;
        }
        Ok(())
    }

    // -----------------------------------------------------------------------
    // P4: Reading list
    // -----------------------------------------------------------------------

    pub fn list_reading_list(&self) -> Result<Vec<ReadingListEntry>> {
        let conn = self.conn();
        let sql = format!(
            "SELECT {BOOK_COLUMNS}, rl.position, rl.note, rl.added_at
             FROM books
             JOIN reading_list rl ON rl.book_id = books.id
             ORDER BY rl.position ASC, rl.added_at ASC"
        );
        let mut stmt = conn.prepare_cached(&sql)?;
        let rows = stmt.query_map([], |row| {
            let book = row_to_book(row)?;
            Ok(ReadingListEntry {
                book,
                // BOOK_COLUMNS is 16 wide; the reading_list columns follow it.
                position: row.get(16)?,
                note: row.get(17)?,
                added_at: row.get(18)?,
            })
        })?;
        let mut entries = rows.collect::<std::result::Result<Vec<_>, _>>()?;
        let mut books: Vec<Book> = entries.iter().map(|e| e.book.clone()).collect();
        hydrate_books(&conn, &mut books)?;
        for (entry, book) in entries.iter_mut().zip(books) {
            entry.book = book;
        }
        Ok(entries)
    }

    pub fn is_in_reading_list(&self, book_id: i64) -> Result<bool> {
        let conn = self.conn();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM reading_list WHERE book_id = ?1",
            params![book_id],
            |r| r.get(0),
        )?;
        Ok(n > 0)
    }

    pub fn add_to_reading_list(&self, book_id: i64) -> Result<()> {
        let conn = self.conn();
        let next_pos: i64 = conn
            .query_row(
                "SELECT IFNULL(MAX(position), -1) + 1 FROM reading_list",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        conn.execute(
            "INSERT OR IGNORE INTO reading_list (book_id, position, note, added_at)
             VALUES (?1, ?2, '', ?3)",
            params![book_id, next_pos, chrono_like_now()],
        )?;
        Ok(())
    }

    pub fn remove_from_reading_list(&self, book_id: i64) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM reading_list WHERE book_id = ?1",
            params![book_id],
        )?;
        Ok(())
    }

    /// Move an entry up (`delta = -1`) or down (`delta = 1`).
    pub fn move_reading_list_entry(&self, book_id: i64, delta: i64) -> Result<()> {
        let conn = self.conn();
        let ids: Vec<i64> = {
            let mut stmt = conn
                .prepare("SELECT book_id FROM reading_list ORDER BY position ASC, added_at ASC")?;
            let rows = stmt.query_map([], |r| r.get(0))?;
            rows.collect::<std::result::Result<Vec<_>, _>>()?
        };
        let Some(idx) = ids.iter().position(|id| *id == book_id) else {
            return Ok(());
        };
        let target = idx as i64 + delta;
        if target < 0 || target as usize >= ids.len() {
            return Ok(());
        }
        let mut reordered = ids.clone();
        reordered.swap(idx, target as usize);
        for (pos, id) in reordered.iter().enumerate() {
            conn.execute(
                "UPDATE reading_list SET position = ?2 WHERE book_id = ?1",
                params![id, pos as i64],
            )?;
        }
        Ok(())
    }

    #[cfg(test)]
    pub fn set_reading_list_note(&self, book_id: i64, note: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE reading_list SET note = ?2 WHERE book_id = ?1",
            params![book_id, note],
        )?;
        Ok(())
    }
}
