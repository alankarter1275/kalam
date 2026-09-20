//! Lookup history (Phase 10) — append-only log of every dictionary lookup,
//! separate from the deliberate saves in Saved Words.
//!
//! Misses are logged too (`found = 0`): a word the user looked up that has
//! no definition is the best signal about gaps in the installed packs.
//! Repeats of the same word in the same book within the same hour collapse
//! into one row (same ISO-hour trick as `mark_book_opened`), so flipping
//! back to a word does not flood the log.

use super::*;

/// One logged dictionary lookup.
#[derive(Debug, Clone)]
pub struct DictLookup {
    #[allow(dead_code)] // lookup rows list the word; the row id is never shown
    pub id: i64,
    pub word: String,
    #[allow(dead_code)] // the jump uses the word; the originating book is not surfaced
    pub book_id: Option<i64>,
    #[allow(dead_code)] // as above
    pub chapter_index: Option<i64>,
    pub context_text: String,
    pub found: bool,
    pub at: String,
}

impl Catalog {
    /// Log one lookup unless the `dict_history_enabled` pref is off.
    pub fn log_dict_lookup(
        &self,
        word: &str,
        book_id: Option<i64>,
        chapter_index: Option<i64>,
        context_text: Option<&str>,
        found: bool,
    ) -> Result<()> {
        if self.get_pref_i64("dict_history_enabled", 1) == 0 {
            return Ok(());
        }
        let word = word.trim();
        if word.is_empty() {
            return Ok(());
        }
        let conn = self.conn();
        let now = chrono_like_now();

        // Collapse repeats: same word, same book, within the same hour.
        // (Sidebar lookups carry no book, so `IFNULL(book_id, -1)` keeps
        // those collapsing among themselves.)
        let recent: Option<String> = conn
            .query_row(
                "SELECT at FROM dict_lookups
                 WHERE word = ?1 COLLATE NOCASE
                   AND IFNULL(book_id, -1) = IFNULL(?2, -1)
                 ORDER BY at DESC LIMIT 1",
                params![word, book_id],
                |r| r.get(0),
            )
            .optional()?;
        let same_hour = recent
            .as_deref()
            .map(|prev| prev.len() >= 13 && now.len() >= 13 && prev[..13] == now[..13])
            .unwrap_or(false);
        if same_hour {
            return Ok(());
        }

        conn.execute(
            "INSERT INTO dict_lookups (word, book_id, chapter_index, context_text, found, at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                word,
                book_id,
                chapter_index,
                context_text.unwrap_or(""),
                found as i64,
                now,
            ],
        )?;
        Ok(())
    }

    /// Newest-first lookup history, optionally filtered to words matching
    /// `query` (case-insensitive substring).
    pub fn list_dict_lookups(&self, query: &str, limit: usize) -> Result<Vec<DictLookup>> {
        let conn = self.conn();
        let q = query.trim();
        let mut stmt = if q.is_empty() {
            conn.prepare_cached(
                "SELECT id, word, book_id, chapter_index, context_text, found, at
                 FROM dict_lookups ORDER BY at DESC, id DESC LIMIT ?1",
            )?
        } else {
            conn.prepare_cached(
                "SELECT id, word, book_id, chapter_index, context_text, found, at
                 FROM dict_lookups
                 WHERE word LIKE ?1 ESCAPE '\\'
                 ORDER BY at DESC, id DESC LIMIT ?2",
            )?
        };
        let like = format!("%{}%", escape_like(q));
        let rows = if q.is_empty() {
            stmt.query_map(params![limit as i64], row_to_dict_lookup)?
        } else {
            stmt.query_map(params![like, limit as i64], row_to_dict_lookup)?
        };
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    pub fn clear_dict_lookups(&self) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM dict_lookups", [])?;
        Ok(())
    }

    /// Words looked up more than once, most-frequent first — the proposed
    /// study set for the vocabulary review.
    pub fn repeat_lookup_words(&self, limit: usize) -> Result<Vec<(String, i64)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT word, COUNT(*) AS n FROM dict_lookups
             GROUP BY word COLLATE NOCASE
             HAVING n > 1
             ORDER BY n DESC, word COLLATE NOCASE ASC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }
}

fn row_to_dict_lookup(row: &rusqlite::Row<'_>) -> rusqlite::Result<DictLookup> {
    Ok(DictLookup {
        id: row.get(0)?,
        word: row.get(1)?,
        book_id: row.get(2)?,
        chapter_index: row.get(3)?,
        context_text: row.get(4)?,
        found: row.get::<_, i64>(5)? != 0,
        at: row.get(6)?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_history_logs_hits_misses_and_collapses_repeats() {
        use crate::models::BookFormat;
        let cat = Catalog::open_in_memory().unwrap();
        // Real book ids: dict_lookups.book_id has a foreign key, and the
        // catalog runs with PRAGMA foreign_keys = ON.
        let b1 = cat
            .insert_book(
                "uuid-b1",
                "Book One",
                "Author",
                None,
                "",
                BookFormat::Epub,
                "b1.epub",
                "hash-b1",
                None,
                &[],
            )
            .unwrap();
        let b2 = cat
            .insert_book(
                "uuid-b2",
                "Book Two",
                "Author",
                None,
                "",
                BookFormat::Epub,
                "b2.epub",
                "hash-b2",
                None,
                &[],
            )
            .unwrap();

        // Pref defaults to on.
        assert_eq!(cat.get_pref_i64("dict_history_enabled", 1), 1);

        cat.log_dict_lookup("serendipity", Some(b1), Some(2), Some("a sentence"), true)
            .unwrap();
        // Same word, same book, same hour → collapsed.
        cat.log_dict_lookup("serendipity", Some(b1), Some(2), Some("again"), true)
            .unwrap();
        // Same word, different book → separate row.
        cat.log_dict_lookup("serendipity", Some(b2), None, None, true)
            .unwrap();
        // A miss is logged with found = 0.
        cat.log_dict_lookup("zzzqqq", Some(b1), None, None, false)
            .unwrap();
        // Sidebar lookups without a book collapse among themselves.
        cat.log_dict_lookup("zzzqqq", None, None, None, false)
            .unwrap();

        let rows = cat.list_dict_lookups("", 100).unwrap();
        assert_eq!(rows.len(), 4);
        let miss = rows.iter().find(|r| r.word == "zzzqqq").unwrap();
        assert!(!miss.found);
        assert_eq!(rows.iter().filter(|r| r.word == "serendipity").count(), 2);

        // Word filter.
        let filtered = cat.list_dict_lookups("seren", 100).unwrap();
        assert_eq!(filtered.len(), 2);

        // Repeat study set: serendipity × 2 (grouped across books) and
        // zzzqqq × 2 (a book row plus a no-book row are separate), both
        // with equal frequency, listed most-frequent-first then A–Z.
        let repeats = cat.repeat_lookup_words(10).unwrap();
        assert_eq!(
            repeats,
            vec![("serendipity".to_string(), 2), ("zzzqqq".to_string(), 2)]
        );

        // Disabling the pref stops all writes.
        cat.set_pref("dict_history_enabled", "0");
        cat.log_dict_lookup("fresh", None, None, None, true)
            .unwrap();
        assert_eq!(cat.list_dict_lookups("", 100).unwrap().len(), 4);

        // Clear empties the table.
        cat.clear_dict_lookups().unwrap();
        assert!(cat.list_dict_lookups("", 100).unwrap().is_empty());
        assert!(cat.repeat_lookup_words(10).unwrap().is_empty());
    }
}
