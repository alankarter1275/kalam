//! Reading data that outlives its book.
//!
//! `delete_book_single` removes the `books` row, and fifteen tables hang off it
//! with `ON DELETE CASCADE`. The catalog runs with `PRAGMA foreign_keys = ON` in
//! both `Catalog::open` and `Catalog::open_in_memory`, so the cascade really
//! fires: your place in the book, your bookmarks and your highlights are gone,
//! and none of them can be derived from the file again.
//!
//! `metadata_overrides` already rescues hand-edited metadata across a delete and
//! re-import. This is the same idea for the data that cannot be rebuilt, keyed by
//! the same content fingerprint and sharing the same lifecycle.
//!
//! # Where the hooks sit
//!
//! Capture is in `delete_book_single`, beside `remember_overrides` and before the
//! cascade runs. Restore is reached through `restore_overrides`, which
//! `import_epub` calls — and that one function accepts `.epub`, `.cbz`, `.cbr`
//! and `.pdf`, and is the only production caller of `insert_book`, so a single
//! hook covers the whole import surface. Comic series need nothing special:
//! `delete_comic_series` deletes chapter by chapter through
//! `delete_book_single`, so each chapter is stashed against its own hash.
//!
//! # Deliberately not stashed
//!
//! - `book_content_fts`, `book_search_index_status`, `page_ocr_cache` — rebuilt
//!   by re-indexing. That costs time, not data.
//! - `comic_chapters` — rebuilt by the import itself.
//! - `book_tags` — already carried inside `metadata_overrides`.
//! - `saved_words` — `ON DELETE SET NULL`, so the row already survives; and since
//!   schema v23 a word is one dictionary entry that belongs to no book.
//! - `editor_snapshots` — the rows name files under
//!   `paths::editor_snapshots_dir(uuid)`, and a re-import gets a fresh uuid, so
//!   restored rows would name files in a directory that does not exist. The files
//!   themselves outlive the delete (they sit under the data dir, not
//!   `library/<uuid>/`), but nothing can address them again without moving them
//!   too. That is editor work, and the editor is out of scope here.
//! - `patches` — no production writer anywhere in the tree. The table is legacy,
//!   kept for sidecar compatibility, so there is nothing to rescue.
//! - `reading_events`, `reading_sessions` — reading history and statistics. Left
//!   out on purpose: they are records *about* a book you chose to remove rather
//!   than work you authored inside it. Adding them is a small extension of the
//!   same document if that judgement turns out to be wrong.

use super::pdf_annotations::encode_geometry;
use super::*;
use serde::{Deserialize, Serialize};

/// Bumped when the shape below changes, so an older payload is skipped whole
/// rather than half-applied onto a book.
const STASH_VERSION: u32 = 1;

/// Where the book sat. Kept verbatim rather than rebuilt through
/// `add_book_to_shelf` / `add_to_reading_list`, because both of those compute a
/// fresh position and drop the note — and the note is the part worth keeping.
#[derive(Serialize, Deserialize)]
struct ShelfSlot {
    shelf_id: i64,
    position: i64,
    added_at: String,
}

#[derive(Serialize, Deserialize)]
struct ListSlot {
    position: i64,
    note: String,
    added_at: String,
}

/// Read straight from the row rather than through `get_reading_progress`, which
/// returns only the chapter and fraction and would lose the "last read" stamp
/// that ordering depends on.
#[derive(Serialize, Deserialize)]
struct Progress {
    chapter_index: i64,
    fraction: f64,
    updated_at: String,
}

/// One JSON document per deleted book.
///
/// `#[serde(default)]` so a field added in a later version reads as empty
/// instead of failing the whole payload — the same forward-compatibility the
/// `Sidecar` struct relies on.
#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct Stash {
    version: u32,
    progress: Option<Progress>,
    bookmarks: Vec<ReadingBookmark>,
    annotations: Vec<Annotation>,
    page_identities: Vec<PdfPageIdentity>,
    drawings: Vec<PdfDrawing>,
    shelves: Vec<ShelfSlot>,
    reading_list: Option<ListSlot>,
}

impl Stash {
    fn is_empty(&self) -> bool {
        self.progress.is_none()
            && self.bookmarks.is_empty()
            && self.annotations.is_empty()
            && self.page_identities.is_empty()
            && self.drawings.is_empty()
            && self.shelves.is_empty()
            && self.reading_list.is_none()
    }
}

impl Catalog {
    /// Capture the reading data that is about to cascade away.
    ///
    /// Called from `delete_book_single` only, and that is deliberate. The
    /// metadata stash is refreshed on every edit because the edit is what needs
    /// remembering; this one only matters at the single moment the rows are about
    /// to be destroyed. Hooking it into `remember_overrides` would serialise
    /// every highlight in the book on every title or tag edit, for nothing.
    pub(super) fn remember_reading(&self, book_id: i64) -> Result<()> {
        self.remember_reading_kinds(book_id, ReadingKinds::ALL)
    }

    /// Capture only the parts `keep` names.
    ///
    /// Shelf and reading-list placement is captured whatever `keep` says. It is
    /// never offered as a choice, because it is where the book lives rather than
    /// work done inside it — see `ReadingKinds`.
    pub(super) fn remember_reading_kinds(&self, book_id: i64, keep: ReadingKinds) -> Result<()> {
        // A restore is not an edit — the same guard `remember_overrides` keeps.
        if self.restoring.load(std::sync::atomic::Ordering::Relaxed) {
            return Ok(());
        }
        let Some(origin) = self.origin_hash(book_id)? else {
            return Ok(());
        };
        if origin.trim().is_empty() {
            return Ok(());
        }

        // Annotations are one table told apart only by its `kind` column, so
        // the split between highlights and quotes happens here rather than in
        // two queries.
        let annotations = if keep.highlights && keep.quotes {
            self.get_annotations_for_book(book_id)?
        } else {
            self.get_annotations_for_book(book_id)?
                .into_iter()
                .filter(|a| keep.covers_annotation(&a.kind))
                .collect()
        };
        let stash = Stash {
            version: STASH_VERSION,
            progress: if keep.position {
                self.progress_row(book_id)?
            } else {
                None
            },
            bookmarks: if keep.bookmarks {
                self.list_reading_bookmarks(book_id)?
            } else {
                Vec::new()
            },
            annotations,
            // Identities are only worth keeping with the ink that hangs off
            // them; on their own they are rebuilt by `ensure_pdf_page_identities`.
            page_identities: if keep.ink {
                self.pdf_page_identities(book_id)?
            } else {
                Vec::new()
            },
            drawings: if keep.ink {
                self.pdf_drawings_for_book(book_id)?
            } else {
                Vec::new()
            },
            shelves: self.shelf_slots(book_id)?,
            reading_list: self.reading_list_slot(book_id)?,
        };
        if stash.is_empty() {
            // Nothing to remember. An empty document would restore to nothing
            // and linger in the table forever.
            return Ok(());
        }

        // A serialisation failure is not worth failing a delete over: the book is
        // going either way, and the caller discards this result just as it
        // discards `remember_overrides`'.
        let Ok(payload) = serde_json::to_string(&stash) else {
            return Ok(());
        };
        let conn = self.conn();
        conn.execute(
            "INSERT INTO reading_overrides (file_hash, payload, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(file_hash) DO UPDATE SET
                payload = excluded.payload,
                updated_at = excluded.updated_at",
            params![origin, payload, chrono_like_now()],
        )?;
        Ok(())
    }

    /// Put the stashed reading data back onto a freshly imported book.
    ///
    /// Called from `restore_overrides` while its `RestoreGuard` is held, so
    /// nothing written here is mistaken for an edit and re-stashed.
    ///
    /// Every insert is idempotent — `OR IGNORE` where the row owns its id,
    /// `OR REPLACE` where `book_id` is the key — so restoring twice cannot
    /// duplicate a book's highlights. Original row ids are kept: `annotations`,
    /// `reading_bookmarks` and `pdf_drawings` are all AUTOINCREMENT, which means
    /// SQLite never reissues an id, so the ones the cascade freed stay free.
    ///
    /// The stash is not pruned afterwards, matching `metadata_overrides`: each
    /// delete rewrites it, so it stays current, and a second re-import of the
    /// same file restores just as well as the first.
    pub(super) fn restore_reading(&self, book_id: i64) -> Result<bool> {
        self.restore_reading_kinds(book_id, ReadingKinds::ALL)
    }

    /// Put back only the parts `keep` names. Placement always comes back.
    ///
    /// Filtering here rather than at capture is what makes "ask me later"
    /// possible: the whole document is stored once, and a reader who defers the
    /// question can still be offered all of it when they open the book.
    pub(super) fn restore_reading_kinds(&self, book_id: i64, keep: ReadingKinds) -> Result<bool> {
        let Some(origin) = self.origin_hash(book_id)? else {
            return Ok(false);
        };
        let payload: Option<String> = {
            let conn = self.conn();
            conn.query_row(
                "SELECT payload FROM reading_overrides WHERE file_hash = ?1",
                params![origin],
                |r| r.get(0),
            )
            .optional()?
        };
        let Some(payload) = payload else {
            return Ok(false);
        };
        // An unreadable or out-of-date payload is skipped whole, and never fails
        // the import. Losing the stash is bad; refusing to open a book the reader
        // just imported is worse.
        let Ok(stash) = serde_json::from_str::<Stash>(&payload) else {
            return Ok(false);
        };
        if stash.version != STASH_VERSION {
            return Ok(false);
        }

        {
            let conn = self.conn();

            if let Some(p) = stash.progress.as_ref().filter(|_| keep.position) {
                conn.execute(
                    "INSERT OR REPLACE INTO reading_progress
                         (book_id, chapter_index, fraction, updated_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![book_id, p.chapter_index, p.fraction, p.updated_at],
                )?;
            }

            // Identities strictly before drawings: `pdf_drawings` carries a
            // composite foreign key to `pdf_page_identities(book_id, page_id)`,
            // so a drawing written first is rejected outright and the whole
            // restore would abort with it.
            for pi in stash.page_identities.iter().filter(|_| keep.ink) {
                conn.execute(
                    "INSERT OR IGNORE INTO pdf_page_identities
                         (book_id, page_id, position)
                     VALUES (?1, ?2, ?3)",
                    params![book_id, pi.page_id, pi.position as i64],
                )?;
            }
            for d in stash.drawings.iter().filter(|_| keep.ink) {
                conn.execute(
                    "INSERT OR IGNORE INTO pdf_drawings
                         (id, book_id, page_id, kind, color, opacity, width_pts,
                          geometry, created_at, updated_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
                    params![
                        d.id,
                        book_id,
                        d.page_id,
                        d.geometry.kind(),
                        d.color,
                        d.opacity.clamp(0.0, 1.0),
                        d.width_pts.max(0.1),
                        encode_geometry(d)?,
                        d.created_at,
                        d.updated_at
                    ],
                )?;
            }

            for a in stash
                .annotations
                .iter()
                .filter(|a| keep.covers_annotation(&a.kind))
            {
                conn.execute(
                    "INSERT OR IGNORE INTO annotations
                         (id, book_id, kind, chapter_index, start_path, start_offset,
                          end_path, end_offset, color, style, text_excerpt, note, cfi,
                          created_at, updated_at)
                     VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11,?12,?13,?14,?15)",
                    params![
                        a.id,
                        book_id,
                        a.kind,
                        a.chapter_index,
                        a.start_path,
                        a.start_offset,
                        a.end_path,
                        a.end_offset,
                        a.color,
                        a.style,
                        a.text_excerpt,
                        a.note,
                        a.cfi,
                        a.created_at,
                        a.updated_at
                    ],
                )?;
            }

            for b in stash.bookmarks.iter().filter(|_| keep.bookmarks) {
                conn.execute(
                    "INSERT OR IGNORE INTO reading_bookmarks
                         (id, book_id, chapter_index, fraction, label, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                    params![
                        b.id,
                        book_id,
                        b.chapter_index,
                        b.fraction.clamp(0.0, 1.0),
                        b.label,
                        b.created_at
                    ],
                )?;
            }

            for s in &stash.shelves {
                // The shelf itself may have been deleted since the book was.
                // `OR IGNORE` does not suppress a foreign-key violation, so ask
                // first rather than let one missing shelf abort the restore.
                let alive: i64 = conn
                    .query_row(
                        "SELECT COUNT(*) FROM shelves WHERE id = ?1",
                        params![s.shelf_id],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                if alive > 0 {
                    conn.execute(
                        "INSERT OR IGNORE INTO shelf_books
                             (shelf_id, book_id, position, added_at)
                         VALUES (?1, ?2, ?3, ?4)",
                        params![s.shelf_id, book_id, s.position, s.added_at],
                    )?;
                }
            }

            if let Some(l) = &stash.reading_list {
                conn.execute(
                    "INSERT OR REPLACE INTO reading_list
                         (book_id, position, note, added_at)
                     VALUES (?1, ?2, ?3, ?4)",
                    params![book_id, l.position, l.note, l.added_at],
                )?;
            }
        }

        // One refresh, not one per row. `insert_annotation`,
        // `insert_reading_bookmark` and `insert_pdf_drawing` each rewrite the
        // sidecar, so going through them would write the whole book's JSON once
        // per restored row — hundreds of writes for a well-marked book.
        crate::sidecar::refresh_for_book(self, book_id);
        Ok(true)
    }

    /// Remove the chosen kinds from a book that is still in the library.
    ///
    /// This is the other half of "start fresh". Because a re-import restores
    /// first and asks afterwards, declining means taking rows back out of a
    /// live book rather than never writing them — and the same operation serves
    /// the book page's clear action, where there is no import involved at all.
    ///
    /// The highlights/quotes split mirrors `ReadingKinds::covers_annotation`
    /// exactly: `quote` rows are quotes, every other `kind` is a highlight. The
    /// two must not drift, or a clear would remove marks the chooser said it was
    /// keeping.
    ///
    /// This touches the live rows only. A rescue copy already filed against the
    /// file's fingerprint is untouched — the book page's clear action has to call
    /// [`forget_reading_stash`] as well, or the marks it just removed would come
    /// back on the next delete-and-reimport.
    #[allow(dead_code)] // wired up by the clear action and the import dialog
    pub fn clear_reading_kinds(&self, book_id: i64, kinds: ReadingKinds) -> Result<ClearTally> {
        let conn = self.conn();
        let mut tally = ClearTally::default();

        if kinds.position {
            let n = conn.execute(
                "DELETE FROM reading_progress WHERE book_id = ?1",
                params![book_id],
            )?;
            if n > 0 {
                tally.position = true;
                // `books.progress` is a denormalised percent for the grid card,
                // written by `set_reading_progress_from`. Clearing the position
                // row without it leaves a card advertising a place in a book
                // that no longer has one. Same pairing as the reset in
                // `db::tags`, including `finished_at`.
                conn.execute(
                    "UPDATE books SET progress = 0, finished_at = NULL WHERE id = ?1",
                    params![book_id],
                )?;
            }
        }
        if kinds.bookmarks {
            tally.bookmarks = conn.execute(
                "DELETE FROM reading_bookmarks WHERE book_id = ?1",
                params![book_id],
            )?;
        }
        if kinds.highlights {
            tally.highlights = conn.execute(
                "DELETE FROM annotations WHERE book_id = ?1 AND kind <> 'quote'",
                params![book_id],
            )?;
        }
        if kinds.quotes {
            tally.quotes = conn.execute(
                "DELETE FROM annotations WHERE book_id = ?1 AND kind = 'quote'",
                params![book_id],
            )?;
        }
        if kinds.ink {
            // Page identities stay: they are structural, rebuilt on demand by
            // `ensure_pdf_page_identities`, and carry no mark of the reader.
            tally.ink = conn.execute(
                "DELETE FROM pdf_drawings WHERE book_id = ?1",
                params![book_id],
            )?;
        }
        drop(conn);

        if !tally.is_empty() {
            // One refresh, for the same reason the restore does one: the
            // per-row helpers would each rewrite the whole sidecar.
            crate::sidecar::refresh_for_book(self, book_id);
        }
        Ok(tally)
    }

    /// Drop the rescue copy filed against this book's fingerprint.
    ///
    /// Returns whether a row was there. Called by the book page's clear action,
    /// so that clearing is permanent — without it the marks would sit in the
    /// stash and return on the next delete-and-reimport, which is exactly what
    /// "clear" promised not to do.
    #[allow(dead_code)] // wired up by the clear action and the import dialog
    pub fn forget_reading_stash(&self, book_id: i64) -> Result<bool> {
        let Some(origin) = self.origin_hash(book_id)? else {
            return Ok(false);
        };
        let conn = self.conn();
        let n = conn.execute(
            "DELETE FROM reading_overrides WHERE file_hash = ?1",
            params![origin],
        )?;
        Ok(n > 0)
    }

    /// Whether a book is still waiting for the reader to say what to keep.
    ///
    /// Set when a re-import brought reading data back and the question went
    /// unanswered — either the reader chose "ask me when I open it", or the
    /// import ran with no window to ask from, which a watched folder and a
    /// download both do.
    pub fn set_restore_pending(&self, book_id: i64, pending: bool) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE books SET restore_pending = ?1 WHERE id = ?2",
            params![i64::from(pending), book_id],
        )?;
        Ok(())
    }

    #[allow(dead_code)] // wired up by the book page and the reader
    pub fn restore_pending(&self, book_id: i64) -> Result<bool> {
        let conn = self.conn();
        let flag: i64 = conn
            .query_row(
                "SELECT restore_pending FROM books WHERE id = ?1",
                params![book_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or(0);
        Ok(flag != 0)
    }

    fn progress_row(&self, book_id: i64) -> Result<Option<Progress>> {
        let conn = self.conn();
        let row = conn
            .query_row(
                "SELECT chapter_index, fraction, updated_at
                 FROM reading_progress WHERE book_id = ?1",
                params![book_id],
                |r| {
                    Ok(Progress {
                        chapter_index: r.get(0)?,
                        fraction: r.get(1)?,
                        updated_at: r.get(2)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }

    fn shelf_slots(&self, book_id: i64) -> Result<Vec<ShelfSlot>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT shelf_id, position, added_at FROM shelf_books WHERE book_id = ?1",
        )?;
        let rows = stmt.query_map(params![book_id], |r| {
            Ok(ShelfSlot {
                shelf_id: r.get(0)?,
                position: r.get(1)?,
                added_at: r.get(2)?,
            })
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(row?);
        }
        Ok(out)
    }

    fn reading_list_slot(&self, book_id: i64) -> Result<Option<ListSlot>> {
        let conn = self.conn();
        let row = conn
            .query_row(
                "SELECT position, note, added_at FROM reading_list WHERE book_id = ?1",
                params![book_id],
                |r| {
                    Ok(ListSlot {
                        position: r.get(0)?,
                        note: r.get(1)?,
                        added_at: r.get(2)?,
                    })
                },
            )
            .optional()?;
        Ok(row)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::{PdfDrawing, PdfDrawingGeometry, ShelfKind};
    use crate::models::BookFormat;

    fn book(cat: &Catalog, title: &str) -> i64 {
        cat.insert_book(
            &format!("uuid-{title}"),
            title,
            "Author",
            None,
            "",
            BookFormat::Epub,
            "book.epub",
            &format!("hash-{title}"),
            None,
            &[],
        )
        .expect("insert book")
    }

    /// The stash document round-trips every part it claims to carry. Without
    /// this, a field silently dropped by a typo in the SQL would only show up as
    /// missing data in the app, and the restore tests below would still pass
    /// because they assert against whatever survived.
    #[test]
    fn the_stash_document_carries_every_part() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = book(&cat, "StashShape");

        cat.set_reading_progress(id, 4, 0.25, 12).unwrap();
        cat.insert_reading_bookmark(id, 4, 0.25, "marked").unwrap();
        cat.insert_annotation(
            id,
            "highlight",
            4,
            "p",
            1,
            "p",
            9,
            "yellow",
            "solid",
            "excerpt",
            "note",
        )
        .unwrap();
        cat.add_to_reading_list(id).unwrap();
        cat.set_reading_list_note(id, "because").unwrap();
        let shelf = cat.create_shelf("Kept", ShelfKind::Manual, "", "").unwrap();
        cat.add_book_to_shelf(shelf, id).unwrap();

        cat.remember_reading(id).unwrap();

        let payload: String = {
            let conn = cat.conn();
            conn.query_row(
                "SELECT payload FROM reading_overrides WHERE file_hash = ?1",
                params!["hash-StashShape"],
                |r| r.get(0),
            )
            .unwrap()
        };
        let stash: Stash = serde_json::from_str(&payload).unwrap();
        assert_eq!(stash.version, STASH_VERSION);
        let p = stash.progress.expect("progress stashed");
        assert_eq!((p.chapter_index, p.fraction), (4, 0.25));
        assert_eq!(stash.bookmarks.len(), 1);
        assert_eq!(stash.bookmarks[0].label, "marked");
        assert_eq!(stash.annotations.len(), 1);
        assert_eq!(stash.annotations[0].note, "note");
        assert_eq!(stash.shelves.len(), 1);
        assert_eq!(stash.shelves[0].shelf_id, shelf);
        let l = stash.reading_list.expect("reading list stashed");
        assert_eq!(l.note, "because", "the note is the part worth keeping");
    }

    /// A book with nothing worth remembering writes no row at all.
    #[test]
    fn an_unmarked_book_stashes_nothing() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = book(&cat, "Pristine");
        cat.remember_reading(id).unwrap();
        let n: i64 = {
            let conn = cat.conn();
            conn.query_row("SELECT COUNT(*) FROM reading_overrides", [], |r| r.get(0))
                .unwrap()
        };
        assert_eq!(n, 0, "an empty document would restore to nothing");
    }

    /// The composite foreign key makes ordering load-bearing: identities must be
    /// in place before the drawings that point at them, or the drawing insert is
    /// rejected and takes the whole restore with it.
    #[test]
    fn drawings_come_back_because_their_page_identities_come_first() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = book(&cat, "Inked");
        let identities = cat.ensure_pdf_page_identities(id, 3).unwrap();
        let page_id = identities[0].page_id.clone();
        cat.insert_pdf_drawing(&PdfDrawing {
            id: 0,
            book_id: id,
            page_id: page_id.clone(),
            color: "ink".into(),
            opacity: 1.0,
            width_pts: 3.0,
            geometry: PdfDrawingGeometry::Sticky {
                point: (10.0, 20.0),
                text: "remember this".into(),
            },
            created_at: String::new(),
            updated_at: String::new(),
        })
        .unwrap();
        assert_eq!(cat.pdf_drawings_for_book(id).unwrap().len(), 1);

        cat.remember_reading(id).unwrap();
        cat.delete_book(id).unwrap();
        assert!(cat.pdf_drawings_for_book(id).unwrap().is_empty());

        let again = book(&cat, "InkedAgain");
        // Same content hash, so the stash matches; different id, so everything
        // has to be re-keyed onto it.
        {
            let conn = cat.conn();
            conn.execute(
                "UPDATE books SET file_hash = 'hash-Inked' WHERE id = ?1",
                params![again],
            )
            .unwrap();
        }
        assert!(cat.restore_reading(again).unwrap());

        let drawings = cat.pdf_drawings_for_book(again).unwrap();
        assert_eq!(drawings.len(), 1, "the ink survived the delete");
        assert_eq!(drawings[0].book_id, again, "re-keyed onto the new book");
        assert_eq!(drawings[0].page_id, page_id);
        match &drawings[0].geometry {
            PdfDrawingGeometry::Sticky { text, .. } => {
                assert_eq!(text.as_str(), "remember this")
            }
            other => panic!("geometry did not round-trip: {other:?}"),
        }
    }

    /// Restoring twice must not duplicate anything. The import path calls
    /// `restore_overrides` once, but a stash that is not pruned can be restored
    /// again after a second delete-and-import cycle, and the ids it carries are
    /// the ids already in the table.
    #[test]
    fn restoring_twice_changes_nothing() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = book(&cat, "Twice");
        cat.set_reading_progress(id, 2, 0.5, 9).unwrap();
        cat.insert_reading_bookmark(id, 2, 0.5, "here").unwrap();
        cat.insert_annotation(
            id,
            "highlight",
            2,
            "",
            0,
            "",
            4,
            "green",
            "solid",
            "text",
            "",
        )
        .unwrap();
        cat.remember_reading(id).unwrap();
        cat.delete_book(id).unwrap();

        let again = book(&cat, "TwiceAgain");
        {
            let conn = cat.conn();
            conn.execute(
                "UPDATE books SET file_hash = 'hash-Twice' WHERE id = ?1",
                params![again],
            )
            .unwrap();
        }
        assert!(cat.restore_reading(again).unwrap());
        assert!(cat.restore_reading(again).unwrap(), "still reports a stash");

        assert_eq!(cat.list_reading_bookmarks(again).unwrap().len(), 1);
        assert_eq!(cat.get_annotations_for_book(again).unwrap().len(), 1);
        assert_eq!(cat.get_reading_progress(again).unwrap(), Some((2, 0.5)));
    }

    /// A shelf deleted after the book was must not abort the restore. The
    /// foreign key would reject the membership row, and `OR IGNORE` does not
    /// suppress foreign-key violations.
    #[test]
    fn a_missing_shelf_does_not_take_the_rest_of_the_restore_with_it() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = book(&cat, "Shelved");
        let shelf = cat
            .create_shelf("Temporary", ShelfKind::Manual, "", "")
            .unwrap();
        cat.add_book_to_shelf(shelf, id).unwrap();
        cat.set_reading_progress(id, 1, 0.1, 5).unwrap();
        cat.remember_reading(id).unwrap();
        cat.delete_book(id).unwrap();
        cat.delete_shelf(shelf).unwrap();

        let again = book(&cat, "ShelvedAgain");
        {
            let conn = cat.conn();
            conn.execute(
                "UPDATE books SET file_hash = 'hash-Shelved' WHERE id = ?1",
                params![again],
            )
            .unwrap();
        }
        assert!(cat.restore_reading(again).unwrap());
        assert_eq!(
            cat.get_reading_progress(again).unwrap(),
            Some((1, 0.1)),
            "the rest of the restore still landed"
        );
        assert!(
            cat.shelf_book_ids(shelf).unwrap_or_default().is_empty(),
            "the dead shelf gained nothing"
        );
    }

    /// A reader who keeps only some of it gets only some of it back. This is the
    /// whole point of `ReadingKinds`: the chooser's ticks have to survive the
    /// trip through the stash rather than being flattened into all-or-nothing.
    #[test]
    fn a_partial_choice_captures_and_restores_only_what_was_chosen() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = book(&cat, "Partial");
        cat.set_reading_progress(id, 4, 0.25, 12).unwrap();
        cat.insert_reading_bookmark(id, 4, 0.25, "kept").unwrap();
        cat.insert_annotation(
            id, "highlight", 4, "", 0, "", 9, "yellow", "solid", "kept", "",
        )
        .unwrap();
        cat.insert_annotation(
            id, "quote", 4, "", 0, "", 9, "yellow", "solid", "dropped", "",
        )
        .unwrap();

        // Keep the place and the highlight; let the quote and the bookmark go.
        //
        // This goes through the delete entry point rather than calling
        // `remember_reading_kinds` and then `delete_book`, and that ordering is
        // the whole test: `delete_book` stashes with `ALL`, so capturing first
        // and deleting second would silently overwrite the choice with a full
        // capture and everything would come back. The UI only ever has the one
        // call, so this is the path that has to work.
        cat.delete_book_keeping(
            id,
            ReadingKinds {
                position: true,
                bookmarks: false,
                highlights: true,
                quotes: false,
                ink: false,
            },
        )
        .unwrap();

        let again = book(&cat, "PartialAgain");
        {
            let conn = cat.conn();
            conn.execute(
                "UPDATE books SET file_hash = 'hash-Partial' WHERE id = ?1",
                params![again],
            )
            .unwrap();
        }
        assert!(cat.restore_reading(again).unwrap());

        assert_eq!(cat.get_reading_progress(again).unwrap(), Some((4, 0.25)));
        assert!(
            cat.list_reading_bookmarks(again).unwrap().is_empty(),
            "the bookmark was not chosen"
        );
        let marks = cat.get_annotations_for_book(again).unwrap();
        assert_eq!(marks.len(), 1, "only the chosen kind came back");
        assert_eq!(marks[0].kind, "highlight");
        assert_eq!(marks[0].text_excerpt, "kept");
    }

    /// Clearing takes rows out of a book that is still in the library, which is
    /// what "start fresh" means once a re-import has already restored them. The
    /// counts are what let the confirmation say something true.
    #[test]
    fn clearing_removes_the_chosen_kinds_and_counts_them() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = book(&cat, "Cleared");
        cat.set_reading_progress(id, 4, 0.25, 12).unwrap();
        cat.insert_reading_bookmark(id, 4, 0.25, "one").unwrap();
        cat.insert_reading_bookmark(id, 5, 0.5, "two").unwrap();
        cat.insert_annotation(
            id, "highlight", 4, "", 0, "", 9, "yellow", "solid", "h", "",
        )
        .unwrap();
        cat.insert_annotation(id, "quote", 4, "", 0, "", 9, "yellow", "solid", "q", "")
            .unwrap();

        let tally = cat
            .clear_reading_kinds(
                id,
                ReadingKinds {
                    position: true,
                    bookmarks: true,
                    highlights: true,
                    quotes: false,
                    ink: false,
                },
            )
            .unwrap();

        assert!(tally.position);
        assert_eq!(tally.bookmarks, 2);
        assert_eq!(tally.highlights, 1);
        assert_eq!(tally.quotes, 0, "quotes were not chosen, so none were removed");
        assert_eq!(tally.total(), 4);

        assert_eq!(cat.get_reading_progress(id).unwrap(), None);
        assert!(cat.list_reading_bookmarks(id).unwrap().is_empty());
        let left = cat.get_annotations_for_book(id).unwrap();
        assert_eq!(left.len(), 1, "the unchosen quote survives");
        assert_eq!(left[0].kind, "quote");
    }

    /// `books.progress` is a second copy of the position, denormalised onto the
    /// book row so the grid card can show a percentage without a join. Clearing
    /// the position row without it leaves a card advertising a place in a book
    /// that no longer has one.
    #[test]
    fn clearing_the_position_also_clears_the_percentage_on_the_card() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = book(&cat, "Percent");
        cat.set_reading_progress(id, 6, 0.5, 12).unwrap();
        assert!(
            cat.get_book(id).unwrap().unwrap().progress > 0,
            "the test needs a non-zero percentage to prove it was cleared"
        );

        cat.clear_reading_kinds(
            id,
            ReadingKinds {
                position: true,
                ..ReadingKinds::NONE
            },
        )
        .unwrap();

        assert_eq!(cat.get_reading_progress(id).unwrap(), None);
        assert_eq!(
            cat.get_book(id).unwrap().unwrap().progress,
            0,
            "the card still showed a percentage for a position that is gone"
        );
    }

    /// The trap this exists to close: clearing the marks but leaving the rescue
    /// copy means the next restore brings them straight back, which is exactly
    /// what "clear" promised not to do.
    #[test]
    fn a_clear_is_only_permanent_once_the_stash_is_forgotten_too() {
        let cat = Catalog::open_in_memory().unwrap();
        let id = book(&cat, "Forgotten");
        cat.insert_annotation(
            id, "highlight", 4, "", 0, "", 9, "yellow", "solid", "doomed", "",
        )
        .unwrap();
        cat.remember_reading(id).unwrap();
        let highlights = ReadingKinds {
            highlights: true,
            ..ReadingKinds::NONE
        };

        // Clearing the live rows alone is not enough: the rescue copy still
        // holds them, and any restore puts them straight back.
        cat.clear_reading_kinds(id, highlights).unwrap();
        assert!(cat.get_annotations_for_book(id).unwrap().is_empty());
        assert!(
            cat.restore_reading(id).unwrap(),
            "the copy survived the clear, so the marks came back"
        );
        assert_eq!(cat.get_annotations_for_book(id).unwrap().len(), 1);

        // Forgetting the copy is what makes "clear" mean clear.
        cat.clear_reading_kinds(id, highlights).unwrap();
        assert!(cat.forget_reading_stash(id).unwrap(), "a copy was there");
        assert!(
            !cat.restore_reading(id).unwrap(),
            "the forgotten marks came back anyway"
        );
        assert!(cat.get_annotations_for_book(id).unwrap().is_empty());
        assert!(
            !cat.forget_reading_stash(id).unwrap(),
            "forgetting twice should report nothing left to forget"
        );
    }

    /// A restore that brought reading data back leaves the book owing the reader
    /// a question. One that brought back only metadata does not, because there
    /// is nothing to choose about — the edits are simply re-applied.
    #[test]
    fn only_a_reading_restore_flags_the_book_as_owing_an_answer() {
        let cat = Catalog::open_in_memory().unwrap();

        // Metadata only: no question to ask.
        let meta = book(&cat, "MetaOnly");
        cat.update_book_metadata(
            meta,
            "Renamed By Hand",
            "Author",
            None,
            0.0,
            "",
            "",
            "",
            &[],
        )
        .unwrap();
        cat.delete_book(meta).unwrap();
        let meta_again = book(&cat, "MetaOnlyAgain");
        {
            let conn = cat.conn();
            conn.execute(
                "UPDATE books SET file_hash = 'hash-MetaOnly' WHERE id = ?1",
                params![meta_again],
            )
            .unwrap();
        }
        let outcome = cat.restore_overrides(meta_again).unwrap();
        assert!(outcome.metadata);
        assert!(!outcome.reading);
        assert!(
            !cat.restore_pending(meta_again).unwrap(),
            "a metadata-only restore owes the reader nothing"
        );

        // Reading data: the question is outstanding until answered.
        let read = book(&cat, "ReadOnly");
        cat.insert_reading_bookmark(read, 1, 0.5, "mark").unwrap();
        cat.delete_book(read).unwrap();
        let read_again = book(&cat, "ReadOnlyAgain");
        {
            let conn = cat.conn();
            conn.execute(
                "UPDATE books SET file_hash = 'hash-ReadOnly' WHERE id = ?1",
                params![read_again],
            )
            .unwrap();
        }
        // `outcome.metadata` is deliberately not asserted here. Deleting a book
        // remembers its metadata unconditionally, edited or not, so a re-import
        // reports a metadata restore for every book that was ever deleted. The
        // two halves are therefore not mutually exclusive, and the pending flag
        // has to key off `reading` alone -- which is what this asserts.
        let outcome = cat.restore_overrides(read_again).unwrap();
        assert!(outcome.reading);
        assert!(
            cat.restore_pending(read_again).unwrap(),
            "the reader has not said whether to keep it yet"
        );

        // Answering clears the flag, so the question is asked once.
        cat.set_restore_pending(read_again, false).unwrap();
        assert!(!cat.restore_pending(read_again).unwrap());
    }
}
