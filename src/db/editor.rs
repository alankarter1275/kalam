//! Deep-editor snapshot rows (decision 135).
//!
//! A snapshot is a whole-EPUB copy under
//! `paths::editor_snapshots_dir(uuid)` taken before every save, tool,
//! restore and import. This table is what History lists (decision 134)
//! and what the editor picker reads its "Edited 2 Oct" state from
//! (decision 142). The files are the source of truth for bytes; a row
//! whose file is gone (deleted by hand, a disk cleanup) must be
//! reported as missing by callers rather than trusted.

use super::*;

/// One row of `editor_snapshots`.
#[derive(Debug, Clone)]
pub struct EditorSnapshot {
    pub id: i64,
    pub book_id: i64,
    /// What was about to happen: "Before save", "Before Replace all",
    /// "Original file" (decision 134).
    pub label: String,
    /// File name inside the book's snapshot folder.
    pub file_name: String,
    /// Files the operation changed (for save: the buffers written).
    pub files_changed: i64,
    pub created_at: String,
}

/// One entry of the editor's History list (decision 134): a
/// snapshot row, or — `snapshot_id: None` — the untouched
/// original file the book was added with (the `.orig` backup).
/// `missing` marks a row whose file is gone; History shows it
/// but cannot restore or compare it.
#[derive(Debug, Clone)]
pub struct EditorVersion {
    pub snapshot_id: Option<i64>,
    pub label: String,
    /// ISO-8601 UTC, as stored (`created_at` for snapshots, the
    /// book's `added_at` for the original).
    pub created_at: String,
    /// Files the operation changed; `None` for the original.
    pub files_changed: Option<i64>,
    pub missing: bool,
}

impl Catalog {
    pub fn add_editor_snapshot(
        &self,
        book_id: i64,
        label: &str,
        file_name: &str,
        files_changed: i64,
    ) -> Result<i64> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO editor_snapshots
                 (book_id, label, file_name, files_changed, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![book_id, label, file_name, files_changed, chrono_like_now()],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Newest first — History's order (decision 134).
    pub fn editor_snapshots(&self, book_id: i64) -> Vec<EditorSnapshot> {
        let conn = self.conn();
        let Ok(mut stmt) = conn.prepare(
            "SELECT id, book_id, label, file_name, files_changed, created_at
             FROM editor_snapshots WHERE book_id = ?1 ORDER BY id DESC",
        ) else {
            return Vec::new();
        };
        let Ok(rows) = stmt.query_map(params![book_id], |r| {
            Ok(EditorSnapshot {
                id: r.get(0)?,
                book_id: r.get(1)?,
                label: r.get(2)?,
                file_name: r.get(3)?,
                files_changed: r.get(4)?,
                created_at: r.get(5)?,
            })
        }) else {
            return Vec::new();
        };
        rows.flatten().collect()
    }

    /// The picker's "Edited …" state: when the newest snapshot was taken.
    pub fn last_editor_snapshot_at(&self, book_id: i64) -> Option<String> {
        let conn = self.conn();
        conn.query_row(
            "SELECT created_at FROM editor_snapshots
             WHERE book_id = ?1 ORDER BY id DESC LIMIT 1",
            params![book_id],
            |r| r.get(0),
        )
        .optional()
        .ok()
        .flatten()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshots_round_trip_newest_first() {
        let catalog = Catalog::open_in_memory().expect("in-memory catalog");
        // A book row for the foreign key: same minimal insert the other
        // db tests use would need the full column list, so lean on a
        // direct insert with defaults where the schema allows.
        let book_id = {
            let conn = catalog.conn();
            conn.execute(
                "INSERT INTO books (uuid, title, sort_title, authors, format, file_name, file_hash, added_at)
                 VALUES ('u-1', 'T', 'T', 'A', 'EPUB', 't.epub', 'h', '2026-01-01')",
                [],
            )
            .expect("insert book");
            conn.last_insert_rowid()
        };
        catalog
            .add_editor_snapshot(book_id, "Original file", "1-original.epub", 0)
            .expect("first snapshot");
        catalog
            .add_editor_snapshot(book_id, "Before save", "2-before-save.epub", 3)
            .expect("second snapshot");

        let rows = catalog.editor_snapshots(book_id);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].label, "Before save");
        assert_eq!(rows[0].files_changed, 3);
        assert_eq!(rows[1].label, "Original file");
        assert!(catalog.last_editor_snapshot_at(book_id).is_some());
        assert!(catalog.last_editor_snapshot_at(book_id + 99).is_none());
    }
}
