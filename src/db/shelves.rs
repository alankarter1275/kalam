//! Shelves queries.
//!
//! Split out of a 3,400-line `db.rs` purely to make it navigable; these are
//! the same methods on the same `Catalog`, moved verbatim.

use super::*;

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
        // shelves need their rules compiled and counted individually.
        let manual_counts: std::collections::HashMap<i64, usize> = {
            let conn = self.conn();
            let mut stmt = conn
                .prepare_cached("SELECT shelf_id, COUNT(*) FROM shelf_books GROUP BY shelf_id")?;
            let rows = stmt.query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)))?;
            rows.filter_map(|r| r.ok())
                .map(|(id, n)| (id, n as usize))
                .collect()
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

    fn shelf_book_count(&self, shelf: &Shelf) -> Result<usize> {
        let conn = self.conn();
        let n: i64 = match shelf.kind {
            ShelfKind::Manual => conn.query_row(
                "SELECT COUNT(*) FROM shelf_books WHERE shelf_id = ?1",
                params![shelf.id],
                |r| r.get(0),
            )?,
            ShelfKind::Smart => {
                let (where_sql, rule_params) = shelf.rule_set().to_sql();
                // Rule-derived SQL differs per shelf, so it is not cached.
                let sql = format!("SELECT COUNT(*) FROM books WHERE {where_sql}");
                let bound: Vec<&dyn rusqlite::ToSql> = rule_params
                    .iter()
                    .map(|p| p as &dyn rusqlite::ToSql)
                    .collect();
                conn.query_row(&sql, bound.as_slice(), |r| r.get(0))?
            }
        };
        Ok(n as usize)
    }

    /// Count matches for an unsaved rule set — powers the live count in the editor.
    pub fn count_matching_rules(&self, rules: &crate::shelf_rules::RuleSet) -> Result<usize> {
        let (where_sql, rule_params) = rules.to_sql();
        let conn = self.conn();
        let sql = format!("SELECT COUNT(*) FROM books WHERE {where_sql}");
        let bound: Vec<&dyn rusqlite::ToSql> = rule_params
            .iter()
            .map(|p| p as &dyn rusqlite::ToSql)
            .collect();
        let n: i64 = conn.query_row(&sql, bound.as_slice(), |r| r.get(0))?;
        Ok(n as usize)
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
