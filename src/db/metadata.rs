//! Metadata queries.
//!
//! Split out of a 3,400-line `db.rs` purely to make it navigable; these are
//! the same methods on the same `Catalog`, moved verbatim.

use super::*;

impl Catalog {
    // -----------------------------------------------------------------------
    // P5: metadata editing
    // -----------------------------------------------------------------------

    /// Overwrite the editable metadata fields. Tags are replaced wholesale,
    /// which matches what the edit dialog presents (one comma-separated box).
    #[allow(clippy::too_many_arguments)]
    pub fn update_book_metadata(
        &self,
        book_id: i64,
        title: &str,
        authors: &str,
        series: Option<&str>,
        series_index: f32,
        publisher: &str,
        published: &str,
        description: &str,
        tags: &[String],
    ) -> Result<()> {
        let conn = self.conn();
        let title = title.trim();
        // sort_title mirrors insert_book so ordering stays consistent.
        conn.execute(
            "UPDATE books
             SET title = ?2, sort_title = ?3, authors = ?4, series = ?5,
                 series_index = ?6, publisher = ?7, published = ?8, description = ?9
             WHERE id = ?1",
            params![
                book_id,
                title,
                title.to_lowercase(),
                authors.trim(),
                series.map(|s| s.trim()).filter(|s| !s.is_empty()),
                series_index.max(0.0) as f64,
                publisher.trim(),
                published.trim(),
                description,
            ],
        )?;

        conn.execute("DELETE FROM book_tags WHERE book_id = ?1", params![book_id])?;
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
                params![book_id, tag_id],
            )?;
        }

        drop(conn);

        self.prune_orphan_tags()?;

        // Remember the result so removing and re-importing this file does not
        // silently discard the edit.
        self.remember_overrides(book_id)?;

        // Keep the per-book kalam.json in step. Best-effort: a stale backup
        // deserves a log line, never a failed edit.
        crate::sidecar::refresh_for_book(self, book_id);
        Ok(())
    }

    /// Add one tag from the book page's tag chip without touching the other
    /// metadata fields.
    pub fn add_book_tag(&self, book_id: i64, tag: &str) -> Result<()> {
        let tag = tag.trim();
        if tag.is_empty() {
            return Ok(());
        }
        let conn = self.conn();
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
            params![book_id, tag_id],
        )?;
        drop(conn);

        self.prune_orphan_tags()?;
        self.remember_overrides(book_id)?;
        crate::sidecar::refresh_for_book(self, book_id);
        Ok(())
    }

    /// Remove one tag by name (case-insensitive) from the book page's chip.
    pub fn remove_book_tag(&self, book_id: i64, tag: &str) -> Result<()> {
        let tag = tag.trim();
        if tag.is_empty() {
            return Ok(());
        }
        let conn = self.conn();
        conn.execute(
            "DELETE FROM book_tags
             WHERE book_id = ?1 AND tag_id IN
                   (SELECT id FROM tags WHERE name = ?2 COLLATE NOCASE)",
            params![book_id, tag],
        )?;
        drop(conn);

        self.prune_orphan_tags()?;
        self.remember_overrides(book_id)?;
        crate::sidecar::refresh_for_book(self, book_id);
        Ok(())
    }

    /// Drop tags no book references — orphaned ones would clutter the tag
    /// browser.
    pub(super) fn prune_orphan_tags(&self) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM book_tags)",
            [],
        )?;
        Ok(())
    }

    /// Remember the current metadata against the file's hash, so it can be
    /// restored if the book is removed and imported again.
    // Called from delete_book in the parent module.
    pub(super) fn remember_overrides(&self, book_id: i64) -> Result<()> {
        // A restore is not an edit: writing the freshly-imported state back
        // over the saved copy is exactly what we are trying to avoid.
        if self.restoring.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }
        let Some(book) = self.get_book(book_id)? else {
            return Ok(());
        };
        if book.file_hash.trim().is_empty() {
            return Ok(());
        }

        // The cover lives in library/<uuid>/, which is deleted along with the
        // book, so remembering only its name would leave a dangling pointer.
        // Keep a copy outside that directory, named after the file hash.
        let stashed_cover = book.cover_path.as_ref().and_then(|src| {
            if !src.is_file() {
                return None;
            }
            let ext = src
                .extension()
                .and_then(|e| e.to_str())
                .unwrap_or("jpg")
                .to_ascii_lowercase();
            let dest_name = format!("{}.{ext}", book.file_hash);
            let dest = crate::paths::override_covers_dir().join(&dest_name);
            let _ = fs::create_dir_all(crate::paths::override_covers_dir());
            match fs::copy(src, &dest) {
                Ok(_) => Some(dest_name),
                Err(_) => None,
            }
        });

        let conn = self.conn();
        conn.execute(
            "INSERT INTO metadata_overrides
                (file_hash, title, authors, series, series_index, publisher,
                 published, description, tags, rating, cover_name, updated_at)
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12)
             ON CONFLICT(file_hash) DO UPDATE SET
                title = excluded.title,
                authors = excluded.authors,
                series = excluded.series,
                series_index = excluded.series_index,
                publisher = excluded.publisher,
                published = excluded.published,
                description = excluded.description,
                tags = excluded.tags,
                rating = excluded.rating,
                cover_name = excluded.cover_name,
                updated_at = excluded.updated_at",
            params![
                book.file_hash,
                book.title,
                book.authors,
                book.series,
                book.series_index as f64,
                book.publisher,
                book.published,
                book.description,
                book.tags.join(", "),
                book.rating as i64,
                stashed_cover,
                chrono_like_now(),
            ],
        )?;
        Ok(())
    }

    /// Re-apply remembered edits to a freshly imported book. Returns true when
    /// something was restored.
    pub fn restore_overrides(&self, book_id: i64, file_hash: &str) -> Result<bool> {
        // A named struct rather than a ten-element tuple: the tuple needed a
        // type annotation that tripped clippy::type_complexity, and this is
        // easier to read besides.
        struct Saved {
            title: String,
            authors: String,
            series: Option<String>,
            series_index: f64,
            publisher: String,
            published: String,
            description: String,
            tags: String,
            rating: i64,
            cover_name: Option<String>,
        }

        let saved: Option<Saved> = {
            let conn = self.conn();
            conn.query_row(
                "SELECT IFNULL(title,''), IFNULL(authors,''), series,
                        IFNULL(series_index,0), IFNULL(publisher,''),
                        IFNULL(published,''), IFNULL(description,''),
                        IFNULL(tags,''), IFNULL(rating,0), cover_name
                 FROM metadata_overrides WHERE file_hash = ?1",
                params![file_hash],
                |r| {
                    Ok(Saved {
                        title: r.get(0)?,
                        authors: r.get(1)?,
                        series: r.get(2)?,
                        series_index: r.get(3)?,
                        publisher: r.get(4)?,
                        published: r.get(5)?,
                        description: r.get(6)?,
                        tags: r.get(7)?,
                        rating: r.get(8)?,
                        cover_name: r.get(9)?,
                    })
                },
            )
            .optional()?
        };

        let Some(saved) = saved else {
            return Ok(false);
        };

        // Suppress re-stashing for the duration; the Drop impl clears the flag
        // even if a step below fails.
        struct RestoreGuard<'a>(&'a std::sync::atomic::AtomicBool);
        impl Drop for RestoreGuard<'_> {
            fn drop(&mut self) {
                self.0.store(false, std::sync::atomic::Ordering::Relaxed);
            }
        }
        self.restoring
            .store(true, std::sync::atomic::Ordering::Relaxed);
        let _guard = RestoreGuard(&self.restoring);

        let tags: Vec<String> = saved
            .tags
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();

        self.update_book_metadata(
            book_id,
            &saved.title,
            &saved.authors,
            saved.series.as_deref(),
            saved.series_index as f32,
            &saved.publisher,
            &saved.published,
            &saved.description,
            &tags,
        )?;

        if saved.rating > 0 {
            self.set_book_rating(book_id, saved.rating.clamp(0, 10) as u8)?;
        }

        // Copy the stashed cover back into the new book's directory. Failing
        // here is not fatal — the text metadata is already restored, and the
        // book keeps whatever cover the EPUB supplied.
        if let (Some(stashed), Some(book)) = (saved.cover_name, self.get_book(book_id)?) {
            let src = crate::paths::override_covers_dir().join(&stashed);
            if src.is_file() {
                let ext = src
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or("jpg")
                    .to_ascii_lowercase();
                let dest_name = format!("cover-restored.{ext}");
                let dest = book_dir(&book.uuid).join(&dest_name);
                if fs::create_dir_all(book_dir(&book.uuid)).is_ok() && fs::copy(&src, &dest).is_ok()
                {
                    self.set_cover_name_quiet(book_id, Some(&dest_name))?;
                }
            }
        }
        Ok(true)
    }

    /// Forget remembered edits for a file — used by "import fresh".
    #[cfg(test)]
    pub fn forget_overrides(&self, file_hash: &str) -> Result<()> {
        // Drop the stashed cover too, otherwise the covers directory grows
        // forever with images nothing references.
        let stashed: Option<String> = {
            let conn = self.conn();
            conn.query_row(
                "SELECT IFNULL(cover_name, '') FROM metadata_overrides WHERE file_hash = ?1",
                params![file_hash],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .filter(|n| !n.is_empty())
        };
        if let Some(name) = stashed {
            let path = crate::paths::override_covers_dir().join(name);
            let _ = fs::remove_file(path);
        }

        let conn = self.conn();
        conn.execute(
            "DELETE FROM metadata_overrides WHERE file_hash = ?1",
            params![file_hash],
        )?;
        Ok(())
    }

    /// Re-point a book (and its remembered edits) at a new content hash.
    ///
    /// Writing metadata into the EPUB changes the file's bytes, so the stored
    /// hash goes stale. Left alone that would break duplicate detection on
    /// re-import and orphan the override row, so both move together.
    pub fn rehash_book(&self, book_id: i64, old_hash: &str, new_hash: &str) -> Result<()> {
        if old_hash == new_hash {
            return Ok(());
        }
        let conn = self.conn();
        conn.execute(
            "UPDATE books SET file_hash = ?2 WHERE id = ?1",
            params![book_id, new_hash],
        )?;
        // Drop any override already filed under the new hash, then move ours.
        conn.execute(
            "DELETE FROM metadata_overrides WHERE file_hash = ?1",
            params![new_hash],
        )?;
        conn.execute(
            "UPDATE metadata_overrides SET file_hash = ?2 WHERE file_hash = ?1",
            params![old_hash, new_hash],
        )?;
        Ok(())
    }

    /// Point the book at a new cover file inside its own directory.
    pub fn set_cover_name(&self, book_id: i64, cover_name: Option<&str>) -> Result<()> {
        {
            let conn = self.conn();
            conn.execute(
                "UPDATE books SET cover_name = ?2 WHERE id = ?1",
                params![book_id, cover_name],
            )?;
        }
        // Re-stash: a replaced cover is an edit, and the remembered copy would
        // otherwise still be the jacket from before the swap.
        self.remember_overrides(book_id)?;
        Ok(())
    }

    /// Like `set_cover_name`, but does not touch the remembered override.
    /// Used when *restoring* a cover, where re-stashing would be circular.
    fn set_cover_name_quiet(&self, book_id: i64, cover_name: Option<&str>) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE books SET cover_name = ?2 WHERE id = ?1",
            params![book_id, cover_name],
        )?;
        Ok(())
    }

    // -----------------------------------------------------------------------
    // P4.2: ratings & reading goals
    // -----------------------------------------------------------------------

    /// Ratings are stored as 0..=10 half-stars (7 == 3.5 stars); 0 == unrated.
    pub fn set_book_rating(&self, book_id: i64, half_stars: u8) -> Result<()> {
        {
            let conn = self.conn();
            conn.execute(
                "UPDATE books SET rating = ?2 WHERE id = ?1",
                params![book_id, half_stars.min(10) as i64],
            )?;
        }
        self.remember_overrides(book_id)?;
        Ok(())
    }

    /// Yearly target, e.g. "read 24 books this year". 0 disables the goal.
    pub fn reading_goal(&self) -> i64 {
        self.get_pref_i64("goal.books_per_year", 0)
    }

    pub fn set_reading_goal(&self, books: i64) {
        self.set_pref("goal.books_per_year", &books.max(0).to_string());
    }

    /// Books finished since 1 January of the current year.
    pub fn finished_this_year(&self) -> i64 {
        // The user's current year, not UTC's -- on 1 January this is the
        // difference between "0 books this year" and the right answer.
        let local_today = crate::db::local_today();
        let year = &local_today[..4];
        let conn = match self.conn.lock() {
            Ok(c) => c,
            Err(_) => return 0,
        };
        conn.query_row(
            "SELECT COUNT(*) FROM books
             WHERE finished_at IS NOT NULL AND substr(finished_at, 1, 4) = ?1",
            params![year],
            |r| r.get(0),
        )
        .unwrap_or(0)
    }

    /// Which of the last 7 days had any reading — powers the streak strip.
    /// Index 0 is six days ago, index 6 is today.
    pub fn week_activity(&self) -> [bool; 7] {
        let mut out = [false; 7];
        let Ok(conn) = self.conn.lock() else {
            return out;
        };
        let cutoff = iso_days_ago(6);
        let sql = format!(
            "SELECT DISTINCT {} FROM reading_sessions
             WHERE started_at >= ?1 AND seconds > 0",
            crate::db::local_day_sql("started_at")
        );
        let Ok(mut stmt) = conn.prepare(&sql) else {
            return out;
        };
        let Ok(rows) = stmt.query_map(params![cutoff], |r| r.get::<_, String>(0)) else {
            return out;
        };
        let days: Vec<String> = rows.flatten().collect();
        for (i, slot) in out.iter_mut().enumerate() {
            let key = crate::db::local_day_ago(6 - i as i64);
            *slot = days.contains(&key);
        }
        out
    }
}
