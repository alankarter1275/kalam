//! Legacy EPUB edit records retained for database and sidecar compatibility.
//!
//! The EPUB content editor is removed. These rows are no longer created,
//! applied, reviewed, or rendered; retaining the reader here lets ordinary
//! sidecar refreshes preserve patch history left by an older Kalam version.

use super::*;

impl Catalog {
    /// Read legacy patch records in creation order so sidecar refreshes do not
    /// erase history written by older versions. This is not used to modify or
    /// render EPUB content.
    pub fn get_patches_for_book(&self, book_id: i64) -> Result<Vec<PatchRecord>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, book_id, kind, href, chapter_index, find_text, replace_text,
                    context_before, context_after, source, status, created_at, applied_at,
                    before_hash
             FROM patches WHERE book_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![book_id], |r| {
            Ok(PatchRecord {
                id: r.get(0)?,
                book_id: r.get(1)?,
                kind: r.get(2)?,
                href: r.get(3)?,
                chapter_index: r.get(4)?,
                find_text: r.get(5)?,
                replace_text: r.get(6)?,
                context_before: r.get(7)?,
                context_after: r.get(8)?,
                source: r.get(9)?,
                status: r.get(10)?,
                created_at: r.get(11)?,
                applied_at: r.get(12)?,
                before_hash: r.get(13)?,
            })
        })?;
        Ok(rows.flatten().collect())
    }
}
