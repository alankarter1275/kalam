//! Annotations queries.
//!
//! Split out of a 3,400-line `db.rs` purely to make it navigable; these are
//! the same methods on the same `Catalog`, moved verbatim.

use super::*;

impl Catalog {
    // -----------------------------------------------------------------------
    // P3: Annotations
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub fn insert_annotation(
        &self,
        book_id: i64,
        kind: &str,
        chapter_index: i64,
        start_path: &str,
        start_offset: i64,
        end_path: &str,
        end_offset: i64,
        color: &str,
        text_excerpt: &str,
        note: &str,
    ) -> Result<i64> {
        let conn = self.conn();
        let now = chrono_like_now();
        conn.execute(
            "INSERT INTO annotations
                (book_id, kind, chapter_index, start_path, start_offset, end_path, end_offset,
                 color, text_excerpt, note, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?11)",
            params![
                book_id,
                kind,
                chapter_index,
                start_path,
                start_offset,
                end_path,
                end_offset,
                color,
                text_excerpt,
                note,
                now
            ],
        )?;
        let id = conn.last_insert_rowid();
        // Drop the connection lock before refreshing the sidecar: that reads
        // the book and its annotations back, which takes the same lock.
        drop(conn);
        crate::sidecar::refresh_for_book(self, book_id);
        Ok(id)
    }

    pub fn get_annotations_for_book(&self, book_id: i64) -> Result<Vec<Annotation>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, book_id, kind, chapter_index, start_path, start_offset, end_path, end_offset,
                    color, text_excerpt, note, cfi, created_at, updated_at
             FROM annotations WHERE book_id = ?1 ORDER BY chapter_index ASC, created_at ASC",
        )?;
        let rows = stmt.query_map(params![book_id], row_to_annotation)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn get_annotations_for_chapter(
        &self,
        book_id: i64,
        chapter_index: i64,
    ) -> Result<Vec<Annotation>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, book_id, kind, chapter_index, start_path, start_offset, end_path, end_offset,
                    color, text_excerpt, note, cfi, created_at, updated_at
             FROM annotations
             WHERE book_id = ?1 AND chapter_index = ?2
             ORDER BY created_at ASC",
        )?;
        let rows = stmt.query_map(params![book_id, chapter_index], row_to_annotation)?;
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// A few recent quotes with their book titles, in one query.
    /// The dashboard previously fetched 500 rows and then a book per card.
    /// Recent quotes/highlights with their book's identity (title, authors,
    /// cover path) — the library dashboard renders a card per quote.
    pub fn recent_quotes(&self, limit: usize) -> Result<Vec<(Annotation, QuoteRef)>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT a.id, a.book_id, a.kind, a.chapter_index, a.start_path, a.start_offset,
                    a.end_path, a.end_offset, a.color, a.text_excerpt, a.note, a.cfi,
                    a.created_at, a.updated_at, books.title, books.authors,
                    books.uuid, books.cover_name
             FROM annotations a
             JOIN books ON books.id = a.book_id
             WHERE a.kind IN ('quote','highlight') AND TRIM(a.text_excerpt) <> ''
             ORDER BY a.created_at DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], |r| {
            let cover_name: Option<String> = r.get(17)?;
            let uuid: String = r.get(16)?;
            let ref_ = QuoteRef {
                title: r.get(14)?,
                author: r.get(15)?,
                cover_path: cover_name.map(|name| book_dir(&uuid).join(name)),
            };
            Ok((row_to_annotation(r)?, ref_))
        })?;
        Ok(rows.collect::<std::result::Result<Vec<_>, _>>()?)
    }

    /// Total saved quotes, for counts that do not need the rows themselves.
    #[allow(dead_code)]
    pub fn count_quotes(&self) -> Result<i64> {
        let conn = self.conn();
        let n: i64 = conn.query_row(
            "SELECT COUNT(*) FROM annotations WHERE kind IN ('quote','highlight')",
            [],
            |r| r.get(0),
        )?;
        Ok(n)
    }

    pub fn list_all_quotes(&self, query: &str) -> Result<Vec<Annotation>> {
        let conn = self.conn();
        let q = query.trim();
        let mut stmt = if q.is_empty() {
            conn.prepare_cached(
                "SELECT id, book_id, kind, chapter_index, start_path, start_offset, end_path, end_offset,
                        color, text_excerpt, note, cfi, created_at, updated_at
                 FROM annotations WHERE kind IN ('quote','highlight')
                 ORDER BY created_at DESC LIMIT 500",
            )?
        } else {
            conn.prepare_cached(
                "SELECT id, book_id, kind, chapter_index, start_path, start_offset, end_path, end_offset,
                        color, text_excerpt, note, cfi, created_at, updated_at
                 FROM annotations
                 WHERE kind IN ('quote','highlight')
                   AND (text_excerpt LIKE ?1 ESCAPE '\\' OR note LIKE ?1 ESCAPE '\\')
                 ORDER BY created_at DESC LIMIT 500",
            )?
        };
        let like = format!("%{}%", escape_like(q));
        let rows = if q.is_empty() {
            stmt.query_map([], row_to_annotation)?
        } else {
            stmt.query_map(params![like], row_to_annotation)?
        };
        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    pub fn delete_annotation(&self, id: i64) -> Result<()> {
        let conn = self.conn();
        // Which book this belonged to, read *before* the delete removes the
        // row that says so.
        let book_id: Option<i64> = conn
            .query_row(
                "SELECT book_id FROM annotations WHERE id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()?;
        conn.execute("DELETE FROM annotations WHERE id = ?1", params![id])?;
        drop(conn);
        if let Some(book_id) = book_id {
            crate::sidecar::refresh_for_book(self, book_id);
        }
        Ok(())
    }

    pub fn update_annotation_note(&self, id: i64, note: &str) -> Result<()> {
        let conn = self.conn();
        let now = chrono_like_now();
        conn.execute(
            "UPDATE annotations SET note = ?1, updated_at = ?2 WHERE id = ?3",
            params![note, now, id],
        )?;
        drop(conn);
        self.refresh_sidecar_for_annotation(id);
        Ok(())
    }

    pub fn update_annotation_color(&self, id: i64, color: &str) -> Result<()> {
        let conn = self.conn();
        let now = chrono_like_now();
        conn.execute(
            "UPDATE annotations SET color = ?1, updated_at = ?2 WHERE id = ?3",
            params![color, now, id],
        )?;
        drop(conn);
        self.refresh_sidecar_for_annotation(id);
        Ok(())
    }

    /// Store the engine's locator JSON for a highlight (the `cfi` column,
    /// which the WebKit reader never used).
    pub fn update_annotation_cfi(&self, id: i64, cfi: &str) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE annotations SET cfi = ?1, updated_at = ?2 WHERE id = ?3",
            params![cfi, chrono_like_now(), id],
        )?;
        drop(conn);
        self.refresh_sidecar_for_annotation(id);
        Ok(())
    }

    /// Refresh the backup for whichever book owns this annotation.
    ///
    /// Best-effort, like every other sidecar write: an annotation that has
    /// already gone simply has nothing to refresh.
    fn refresh_sidecar_for_annotation(&self, annotation_id: i64) {
        let book_id: Option<i64> = self
            .conn()
            .query_row(
                "SELECT book_id FROM annotations WHERE id = ?1",
                params![annotation_id],
                |r| r.get(0),
            )
            .optional()
            .ok()
            .flatten();
        if let Some(book_id) = book_id {
            crate::sidecar::refresh_for_book(self, book_id);
        }
    }

    // -----------------------------------------------------------------------
    // P3: Saved words
    // -----------------------------------------------------------------------

    pub fn insert_saved_word(
        &self,
        word: &str,
        definition: &str,
        dict_name: Option<&str>,
        book_id: Option<i64>,
        chapter_index: Option<i64>,
        context_text: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn();
        let now = chrono_like_now();
        conn.execute(
            "INSERT INTO saved_words (word, definition, dict_name, book_id, chapter_index, context_text, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![
                word,
                definition,
                dict_name,
                book_id,
                chapter_index,
                context_text,
                now
            ],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Saved words, newest first. `query` filters by word/definition;
    /// `known` (Phase 7) restricts to `Some(true)` known / `Some(false)`
    /// to-review words, or `None` for all.
    /// `(total, known)` counts for the vocabulary page header.
    ///
    /// The page used to get these by calling [`Catalog::list_saved_words`]
    /// twice and taking `.len()`, which loads up to 500 full rows — definition
    /// text and all — purely to count them. It also swallowed both reads, so a
    /// failure silently reported **zero known words**, which reads as real
    /// data rather than as a problem.
    ///
    /// One query, and it counts past the 500-row display cap, so the header is
    /// now accurate for large vocabularies.
    pub fn saved_word_counts(&self) -> Result<(i64, i64)> {
        let conn = self.conn();
        let row = conn.query_row(
            "SELECT COUNT(*), COALESCE(SUM(CASE WHEN known THEN 1 ELSE 0 END), 0)
             FROM saved_words",
            [],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )?;
        Ok(row)
    }

    pub fn list_saved_words(&self, query: &str, known: Option<bool>) -> Result<Vec<SavedWord>> {
        let conn = self.conn();
        let q = query.trim();
        if q.is_empty() {
            let mut stmt = match known {
                None => conn.prepare_cached(
                    "SELECT id, word, definition, dict_name, book_id, chapter_index, context_text, created_at, known
                     FROM saved_words ORDER BY created_at DESC LIMIT 500",
                )?,
                Some(_k) => conn.prepare_cached(
                    "SELECT id, word, definition, dict_name, book_id, chapter_index, context_text, created_at, known
                     FROM saved_words WHERE known = ?1 ORDER BY created_at DESC LIMIT 500",
                )?,
            };
            let rows = match known {
                None => stmt.query_map([], row_to_saved_word)?,
                Some(k) => stmt.query_map(params![k as i64], row_to_saved_word)?,
            };
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(Into::into)
        } else {
            let like = format!("%{}%", escape_like(q));
            let mut stmt = match known {
                None => conn.prepare_cached(
                    "SELECT id, word, definition, dict_name, book_id, chapter_index, context_text, created_at, known
                     FROM saved_words
                     WHERE word LIKE ?1 ESCAPE '\\' OR definition LIKE ?1 ESCAPE '\\'
                     ORDER BY created_at DESC LIMIT 500",
                )?,
                Some(_k) => conn.prepare_cached(
                    "SELECT id, word, definition, dict_name, book_id, chapter_index, context_text, created_at, known
                     FROM saved_words
                     WHERE (word LIKE ?1 ESCAPE '\\' OR definition LIKE ?1 ESCAPE '\\')
                       AND known = ?2
                     ORDER BY created_at DESC LIMIT 500",
                )?,
            };
            let rows = match known {
                None => stmt.query_map(params![like], row_to_saved_word)?,
                Some(k) => stmt.query_map(params![like, k as i64], row_to_saved_word)?,
            };
            rows.collect::<std::result::Result<Vec<_>, _>>()
                .map_err(Into::into)
        }
    }

    /// Phase 7: mark a saved word as known (or back to to-review).
    pub fn set_saved_word_known(&self, id: i64, known: bool) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE saved_words SET known = ?1 WHERE id = ?2",
            params![known as i64, id],
        )?;
        Ok(())
    }

    pub fn delete_saved_word(&self, id: i64) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM saved_words WHERE id = ?1", params![id])?;
        Ok(())
    }

    /// Fetch all saved words without display caps for data export.
    pub fn list_all_saved_words_unlimited(&self) -> Result<Vec<SavedWord>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, word, definition, dict_name, book_id, chapter_index, context_text, created_at, known
             FROM saved_words ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_saved_word)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Fetch all annotations (split into quotes and highlights) without display caps for data export.
    pub fn list_all_annotations_unlimited(&self) -> Result<(Vec<Annotation>, Vec<Annotation>)> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, book_id, kind, chapter_index, start_path, start_offset, end_path, end_offset,
                    color, text_excerpt, note, cfi, created_at, updated_at
             FROM annotations ORDER BY created_at DESC",
        )?;
        let rows = stmt.query_map([], row_to_annotation)?;
        let mut quotes = Vec::new();
        let mut highlights = Vec::new();
        for r in rows {
            let anno = r?;
            if anno.kind == "quote" {
                quotes.push(anno);
            } else {
                highlights.push(anno);
            }
        }
        Ok((quotes, highlights))
    }

    // -----------------------------------------------------------------------
    // Reader bookmarks
    // -----------------------------------------------------------------------

    pub fn insert_reading_bookmark(
        &self,
        book_id: i64,
        chapter_index: i64,
        fraction: f64,
        label: &str,
    ) -> Result<i64> {
        let conn = self.conn();
        let now = chrono_like_now();
        conn.execute(
            "INSERT INTO reading_bookmarks (book_id, chapter_index, fraction, label, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![book_id, chapter_index, fraction.clamp(0.0, 1.0), label, now],
        )?;
        Ok(conn.last_insert_rowid())
    }

    pub fn list_reading_bookmarks(&self, book_id: i64) -> Result<Vec<ReadingBookmark>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, book_id, chapter_index, fraction, label, created_at
             FROM reading_bookmarks
             WHERE book_id = ?1
             ORDER BY chapter_index ASC, fraction ASC, created_at DESC",
        )?;
        let rows = stmt.query_map(params![book_id], row_to_reading_bookmark)?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    pub fn delete_reading_bookmark(&self, id: i64) -> Result<()> {
        let conn = self.conn();
        conn.execute("DELETE FROM reading_bookmarks WHERE id = ?1", params![id])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BookFormat;

    fn catalog_with_book() -> (Catalog, i64) {
        let cat = Catalog::open_in_memory().expect("in-memory catalog");
        let id = cat
            .insert_book(
                "uuid-anno-test",
                "Annotation Book",
                "Author",
                None,
                "",
                BookFormat::Epub,
                "book.epub",
                "hash-anno-test",
                None,
                &[],
            )
            .expect("insert book");
        (cat, id)
    }

    #[test]
    fn insert_and_update_annotation_cfi_persists_locators() {
        let (cat, book_id) = catalog_with_book();
        let id = cat
            .insert_annotation(
                book_id,
                "highlight",
                2,
                "",
                0,
                "",
                0,
                "yellow",
                "Sample text excerpt",
                "Test note",
            )
            .expect("insert annotation");

        let cfi_json = r#"{"kalam_locator":1,"start":{"spine_href":"ch2.xhtml","spine_index":2,"char_offset":10,"locator_version":1,"quote":{"prefix":"","exact":"Sample","suffix":""},"spine_fraction":0.1,"book_progression":0.05},"end":{"spine_href":"ch2.xhtml","spine_index":2,"char_offset":20,"locator_version":1,"quote":{"prefix":"","exact":"text","suffix":""},"spine_fraction":0.12,"book_progression":0.06}}"#;

        cat.update_annotation_cfi(id, cfi_json).expect("update cfi");

        let annotations = cat.get_annotations_for_book(book_id).expect("get annotations");
        assert_eq!(annotations.len(), 1);
        assert_eq!(annotations[0].cfi.as_deref(), Some(cfi_json));
        assert_eq!(annotations[0].kind, "highlight");
        assert_eq!(annotations[0].chapter_index, 2);
    }

    #[test]
    fn reading_bookmarks_crud() {
        let (cat, book_id) = catalog_with_book();
        let mark_id = cat
            .insert_reading_bookmark(book_id, 1, 0.45, "Chapter 2 - Middle")
            .expect("insert bookmark");

        let marks = cat.list_reading_bookmarks(book_id).expect("list bookmarks");
        assert_eq!(marks.len(), 1);
        assert_eq!(marks[0].id, mark_id);
        assert_eq!(marks[0].chapter_index, 1);
        assert!((marks[0].fraction - 0.45).abs() < f64::EPSILON);
        assert_eq!(marks[0].label, "Chapter 2 - Middle");

        cat.delete_reading_bookmark(mark_id).expect("delete bookmark");
        let marks2 = cat.list_reading_bookmarks(book_id).expect("list bookmarks");
        assert!(marks2.is_empty());
    }
}
