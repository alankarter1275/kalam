//! EPUB edit patches (Phase 6) — queries.
//!
//! A patch is a non-destructive replacement rule: the edit lives here and in
//! the book's `kalam.json`, and the `.epub` on disk stays untouched until an
//! explicit apply. Every method that changes a patch refreshes the sidecar
//! before returning, the same invariant annotations follow — so the backup
//! can never silently fall behind the edits.

use super::*;

impl Catalog {
    // -----------------------------------------------------------------------
    // Phase 6: EPUB edit patches
    // -----------------------------------------------------------------------

    #[allow(clippy::too_many_arguments)]
    pub fn insert_patch(
        &self,
        book_id: i64,
        kind: &str,
        href: &str,
        chapter_index: i64,
        find_text: &str,
        replace_text: &str,
        context_before: &str,
        context_after: &str,
        source: &str,
    ) -> Result<i64> {
        let conn = self.conn();
        let now = chrono_like_now();
        conn.execute(
            "INSERT INTO patches
                (book_id, kind, href, chapter_index, find_text, replace_text,
                 context_before, context_after, source, status, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 'pending', ?10)",
            params![
                book_id,
                kind,
                href,
                chapter_index,
                find_text,
                replace_text,
                context_before,
                context_after,
                source,
                now
            ],
        )?;
        let id = conn.last_insert_rowid();
        // Drop the connection lock before refreshing the sidecar: that reads
        // the book, its annotations and its patches back, all of which take
        // the same lock.
        drop(conn);
        crate::sidecar::refresh_for_book(self, book_id);
        Ok(id)
    }

    /// All patches for a book, pending and applied, in creation order —
    /// the order the matcher must apply them in.
    pub fn get_patches_for_book(&self, book_id: i64) -> Result<Vec<PatchRecord>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, book_id, kind, href, chapter_index, find_text, replace_text,
                    context_before, context_after, source, status, created_at, applied_at
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
            })
        })?;
        Ok(rows.flatten().collect())
    }

    /// Only the pending patches, in creation order — what every render
    /// path wants. Applied patches are already in the file's bytes once a
    /// bake has written them; feeding them to the matcher again can only
    /// produce not-found flags. The sidecar keeps reading
    /// [`Self::get_patches_for_book`]: a backup documents everything that
    /// was ever done, not just what is still to do.
    pub fn get_pending_patches_for_book(&self, book_id: i64) -> Result<Vec<PatchRecord>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, book_id, kind, href, chapter_index, find_text, replace_text,
                    context_before, context_after, source, status, created_at, applied_at
             FROM patches WHERE book_id = ?1 AND status = 'pending' ORDER BY id ASC",
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
            })
        })?;
        Ok(rows.flatten().collect())
    }

    /// Mark one patch permanently applied. The row is kept — applied patches
    /// are history, documenting what a bake changed and when; the undo story
    /// is the `.epub.orig` backup, not this table.
    pub fn mark_patch_applied(&self, id: i64) -> Result<()> {
        let conn = self.conn();
        let book_id: i64 = conn.query_row(
            "SELECT book_id FROM patches WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        conn.execute(
            "UPDATE patches SET status = 'applied', applied_at = ?2 WHERE id = ?1",
            params![id, chrono_like_now()],
        )?;
        drop(conn);
        crate::sidecar::refresh_for_book(self, book_id);
        Ok(())
    }

    /// Discard one patch. For pending patches the owner chooses not to keep;
    /// applied history can also be removed once it no longer interests.
    pub fn delete_patch(&self, id: i64) -> Result<()> {
        let conn = self.conn();
        let book_id: i64 = conn.query_row(
            "SELECT book_id FROM patches WHERE id = ?1",
            params![id],
            |r| r.get(0),
        )?;
        conn.execute("DELETE FROM patches WHERE id = ?1", params![id])?;
        drop(conn);
        crate::sidecar::refresh_for_book(self, book_id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seed(cat: &Catalog, title: &str) -> i64 {
        cat.insert_book(
            &format!("uuid-{title}"),
            title,
            "An Author",
            None,
            "",
            crate::models::BookFormat::Epub,
            "book.epub",
            &format!("hash-{title}"),
            None,
            &[],
        )
        .expect("insert")
    }

    #[test]
    fn patches_list_in_creation_order_and_round_trip() {
        let cat = Catalog::open_in_memory().unwrap();
        let book = seed(&cat, "Dune");
        let a = cat
            .insert_patch(
                book, "text", "text/chapter1.xhtml", 0, "teh", "the", "fix ", " word", "typo",
            )
            .unwrap();
        let b = cat
            .insert_patch(
                book,
                "text",
                "text/chapter1.xhtml",
                0,
                "recieve",
                "receive",
                "",
                "",
                "proofread",
            )
            .unwrap();
        assert!(b > a, "ids follow creation order");

        let list = cat.get_patches_for_book(book).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].id, a, "creation order, not name or status order");
        assert_eq!(list[0].find_text, "teh");
        assert_eq!(list[0].replace_text, "the");
        assert_eq!(list[0].context_before, "fix ");
        assert_eq!(list[0].context_after, " word");
        assert_eq!(list[0].source, "typo");
        assert_eq!(list[0].status, "pending");
        assert_eq!(list[0].applied_at, None);
        assert_eq!(list[1].id, b);
        assert_eq!(list[1].source, "proofread");

        // Another book's patches stay out of this list.
        let other = seed(&cat, "Other");
        assert!(cat.get_patches_for_book(other).unwrap().is_empty());
    }

    #[test]
    fn marking_applied_keeps_the_patch_as_history() {
        let cat = Catalog::open_in_memory().unwrap();
        let book = seed(&cat, "Dune");
        let id = cat
            .insert_patch(book, "text", "text/chapter3.xhtml", 3, "a", "b", "", "", "editor")
            .unwrap();
        cat.mark_patch_applied(id).unwrap();

        let list = cat.get_patches_for_book(book).unwrap();
        assert_eq!(list.len(), 1, "applied patches are kept, not deleted");
        assert_eq!(list[0].status, "applied");
        assert!(
            list[0].applied_at.is_some(),
            "the bake time is recorded on the row"
        );
    }

    #[test]
    fn deleting_a_patch_removes_it() {
        let cat = Catalog::open_in_memory().unwrap();
        let book = seed(&cat, "Dune");
        let id = cat
            .insert_patch(book, "text", "text/chapter1.xhtml", 0, "x", "y", "", "", "typo")
            .unwrap();
        cat.delete_patch(id).unwrap();
        assert!(cat.get_patches_for_book(book).unwrap().is_empty());
    }

    #[test]
    fn patches_reach_the_sidecar_mirror() {
        // The content of the backup, built the same way refresh_for_book
        // builds it — the write itself needs a real library folder, which a
        // unit test has no business creating.
        let cat = Catalog::open_in_memory().unwrap();
        let book = seed(&cat, "Dune");
        cat.insert_patch(
            book, "text", "text/chapter1.xhtml", 0, "teh", "the", "fix ", " word", "typo",
        )
        .unwrap();

        let b = cat.get_book(book).unwrap().unwrap();
        let marks = cat.get_annotations_for_book(book).unwrap_or_default();
        let patches = cat.get_patches_for_book(book).unwrap_or_default();
        let side = crate::sidecar::Sidecar::from_parts(&b, None, &marks, &patches);
        assert_eq!(side.patches.len(), 1);
        assert_eq!(side.patches[0].find_text, "teh");
        assert_eq!(side.patches[0].replace_text, "the");
        assert_eq!(side.patches[0].href, "text/chapter1.xhtml");
        assert_eq!(side.patches[0].status, "pending");
    }
    #[test]
    fn pending_reads_leave_applied_history_behind() {
        let cat = Catalog::open_in_memory().unwrap();
        let book = seed(&cat, "Dune");
        let first = cat
            .insert_patch(book, "text", "c1.xhtml", 0, "teh", "the", "", "", "typo")
            .unwrap();
        let second = cat
            .insert_patch(book, "text", "c1.xhtml", 0, "recieve", "receive", "", "", "typo")
            .unwrap();

        // Before any bake: everything the matcher will see, in apply order.
        let pending = cat.get_pending_patches_for_book(book).unwrap();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].id, first, "creation order is the apply order");

        // A bake lands: the first is applied, the second stays pending.
        cat.mark_patch_applied(first).unwrap();

        let pending = cat.get_pending_patches_for_book(book).unwrap();
        assert_eq!(
            pending.iter().map(|p| p.id).collect::<Vec<_>>(),
            vec![second],
            "the render path must not re-feed applied patches to the matcher"
        );

        // The sidecar's read still sees the whole history.
        let all = cat.get_patches_for_book(book).unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0].status, "applied");
        assert_eq!(all[1].status, "pending");
    }

}
