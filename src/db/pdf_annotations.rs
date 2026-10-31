//! PDF page identities and drawing-object queries.
//!
//! Drawings name the page they live on by stable identity, never by
//! page number (decisions 66, 69 and 72). SQLite is the source of
//! truth; every mutation refreshes the book's sidecar snapshot, like
//! the older annotation paths.

use super::*;
use std::collections::{HashMap, HashSet};

type DrawingRow = (i64, i64, String, String, f64, f64, String, String, String);

fn drawing_from_row(row: DrawingRow) -> Result<PdfDrawing> {
    let (id, book_id, page_id, color, opacity, width_pts, geometry, created_at, updated_at) = row;
    let geometry = serde_json::from_str(&geometry)
        .map_err(|err| DbError::PdfAnnotation(format!("drawing {id} geometry: {err}")))?;
    Ok(PdfDrawing {
        id,
        book_id,
        page_id,
        color,
        opacity,
        width_pts,
        geometry,
        created_at,
        updated_at,
    })
}

fn encode_geometry(drawing: &PdfDrawing) -> Result<String> {
    serde_json::to_string(&drawing.geometry)
        .map_err(|err| DbError::PdfAnnotation(format!("encode drawing geometry: {err}")))
}

/// The new home of a deleted reading position: the nearest page
/// that remains (decision 69), measured in the old order — that
/// is where "nearest" meant something while the reader stood
/// there. Equidistant neighbours settle forward, the way reading
/// goes.
fn nearest_remaining_new_page(
    old_page: usize,
    survivor_old_to_new: &HashMap<usize, usize>,
) -> Option<usize> {
    survivor_old_to_new
        .iter()
        .min_by_key(|(old, _)| {
            let old = **old;
            (old.abs_diff(old_page), old < old_page)
        })
        .map(|(_, new)| *new)
}

fn page_edit_backup_path(target_file: &Path) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let name = format!(
        ".{}-kalam-page-edit-backup-{}-{stamp}",
        target_file
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "book.pdf".to_string()),
        std::process::id()
    );
    target_file
        .parent()
        .map(|parent| parent.join(name.clone()))
        .unwrap_or_else(|| PathBuf::from(name))
}

fn sync_parent_dir(path: &Path) {
    if let Some(parent) = path.parent() {
        if let Ok(dir) = std::fs::File::open(parent) {
            let _ = dir.sync_all();
        }
    }
}

impl Catalog {
    /// This book's PDF page identities, in current page order.
    pub fn pdf_page_identities(&self, book_id: i64) -> Result<Vec<PdfPageIdentity>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT page_id, position FROM pdf_page_identities
             WHERE book_id = ?1 ORDER BY position ASC",
        )?;
        let rows = stmt.query_map(params![book_id], |row| {
            Ok(PdfPageIdentity {
                page_id: row.get(0)?,
                position: row.get::<_, i64>(1)?.max(1) as usize,
            })
        })?;
        rows.collect::<std::result::Result<Vec<_>, _>>()
            .map_err(Into::into)
    }

    /// Give every current page of this book a stable identity.
    ///
    /// A book that has never been edited gets `page-1`, `page-2`, … in
    /// physical order — the identity is only a name, and from then on
    /// the stored position, not the name, says where the page sits.
    /// Pages a changed file gained are named on the same terms, with a
    /// suffix if an edited book already spends the plain name on a
    /// page that moved.
    pub fn ensure_pdf_page_identities(
        &self,
        book_id: i64,
        total_pages: usize,
    ) -> Result<Vec<PdfPageIdentity>> {
        if total_pages == 0 {
            return self.pdf_page_identities(book_id);
        }
        let existing = self.pdf_page_identities(book_id)?;
        let positions: HashSet<usize> = existing.iter().map(|page| page.position).collect();
        let mut ids: HashSet<String> = existing.iter().map(|page| page.page_id.clone()).collect();
        let mut added = false;
        {
            let conn = self.conn();
            for position in 1..=total_pages {
                if positions.contains(&position) {
                    continue;
                }
                let mut page_id = format!("page-{position}");
                let mut suffix = 2;
                while ids.contains(&page_id) {
                    page_id = format!("page-{position}-{suffix}");
                    suffix += 1;
                }
                conn.execute(
                    "INSERT INTO pdf_page_identities (book_id, page_id, position)
                     VALUES (?1, ?2, ?3)",
                    params![book_id, page_id, position as i64],
                )?;
                ids.insert(page_id);
                added = true;
            }
        }
        // Refresh after the connection lock is dropped: the sidecar
        // reads the book back through the same catalog.
        if added {
            crate::sidecar::refresh_for_book(self, book_id);
        }
        Ok(self
            .pdf_page_identities(book_id)?
            .into_iter()
            .filter(|page| page.position <= total_pages)
            .collect())
    }

    /// Store one new drawing object. The row's `id` and stamps are
    /// assigned here; everything else is stored as given.
    pub fn insert_pdf_drawing(&self, drawing: &PdfDrawing) -> Result<i64> {
        let geometry = encode_geometry(drawing)?;
        let conn = self.conn();
        let now = chrono_like_now();
        conn.execute(
            "INSERT INTO pdf_drawings
                (book_id, page_id, kind, color, opacity, width_pts, geometry,
                 created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?8)",
            params![
                drawing.book_id,
                drawing.page_id,
                drawing.geometry.kind(),
                drawing.color,
                drawing.opacity.clamp(0.0, 1.0),
                drawing.width_pts.max(0.1),
                geometry,
                now
            ],
        )?;
        let id = conn.last_insert_rowid();
        drop(conn);
        crate::sidecar::refresh_for_book(self, drawing.book_id);
        Ok(id)
    }

    /// Every drawing stored for this book, oldest first — the order
    /// they were drawn, which is also their paint order.
    pub fn pdf_drawings_for_book(&self, book_id: i64) -> Result<Vec<PdfDrawing>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, book_id, page_id, color, opacity, width_pts, geometry,
                    created_at, updated_at
             FROM pdf_drawings WHERE book_id = ?1 ORDER BY id ASC",
        )?;
        let rows = stmt.query_map(params![book_id], |row| {
            Ok((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
                row.get(6)?,
                row.get(7)?,
                row.get(8)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            out.push(drawing_from_row(row?)?);
        }
        Ok(out)
    }

    pub fn get_pdf_drawing(&self, id: i64) -> Result<Option<PdfDrawing>> {
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(
            "SELECT id, book_id, page_id, color, opacity, width_pts, geometry,
                    created_at, updated_at
             FROM pdf_drawings WHERE id = ?1",
        )?;
        let row = stmt
            .query_row(params![id], |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                    row.get(6)?,
                    row.get(7)?,
                    row.get(8)?,
                ))
            })
            .optional()?;
        row.map(drawing_from_row).transpose()
    }

    /// Write back a moved, recoloured or reworded drawing.
    pub fn update_pdf_drawing(&self, drawing: &PdfDrawing) -> Result<()> {
        let geometry = encode_geometry(drawing)?;
        let conn = self.conn();
        conn.execute(
            "UPDATE pdf_drawings
             SET page_id = ?1, kind = ?2, color = ?3, opacity = ?4,
                 width_pts = ?5, geometry = ?6, updated_at = ?7
             WHERE id = ?8",
            params![
                drawing.page_id,
                drawing.geometry.kind(),
                drawing.color,
                drawing.opacity.clamp(0.0, 1.0),
                drawing.width_pts.max(0.1),
                geometry,
                chrono_like_now(),
                drawing.id
            ],
        )?;
        drop(conn);
        crate::sidecar::refresh_for_book(self, drawing.book_id);
        Ok(())
    }

    pub fn delete_pdf_drawing(&self, id: i64) -> Result<()> {
        let conn = self.conn();
        let book_id: Option<i64> = conn
            .query_row(
                "SELECT book_id FROM pdf_drawings WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .optional()?;
        conn.execute("DELETE FROM pdf_drawings WHERE id = ?1", params![id])?;
        drop(conn);
        if let Some(book_id) = book_id {
            crate::sidecar::refresh_for_book(self, book_id);
        }
        Ok(())
    }

    /// Put a deleted drawing back exactly as it was — same id, same
    /// stamps (decision 30's undo).
    pub fn restore_pdf_drawing(&self, drawing: &PdfDrawing) -> Result<()> {
        let geometry = encode_geometry(drawing)?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO pdf_drawings
                (id, book_id, page_id, kind, color, opacity, width_pts,
                 geometry, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                drawing.id,
                drawing.book_id,
                drawing.page_id,
                drawing.geometry.kind(),
                drawing.color,
                drawing.opacity.clamp(0.0, 1.0),
                drawing.width_pts.max(0.1),
                geometry,
                drawing.created_at,
                drawing.updated_at
            ],
        )?;
        drop(conn);
        crate::sidecar::refresh_for_book(self, drawing.book_id);
        Ok(())
    }

    /// Count the reader's own work on `pages` — (page identity,
    /// physical page) pairs, because the two annotation families
    /// still name a page differently: drawings by identity
    /// (decision 72), highlights and bookmarks by the physical
    /// page they were made on. Grouped counts for the whole book
    /// are filtered here rather than asked page by page, so
    /// selecting every page of a long book stays three queries.
    pub fn pdf_page_bound_counts(
        &self,
        book_id: i64,
        pages: &[(String, usize)],
    ) -> Result<PdfPageBoundCounts> {
        let mut counts = PdfPageBoundCounts::default();
        if pages.is_empty() {
            return Ok(counts);
        }
        let selected_pages: HashSet<i64> =
            pages.iter().map(|(_, page)| *page as i64).collect();
        let selected_ids: HashSet<&str> =
            pages.iter().map(|(page_id, _)| page_id.as_str()).collect();
        let conn = self.conn();

        {
            let mut stmt = conn.prepare_cached(
                "SELECT chapter_index, kind, COUNT(*) FROM annotations
                 WHERE book_id = ?1 GROUP BY chapter_index, kind",
            )?;
            let rows = stmt.query_map(params![book_id], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, i64>(2)?,
                ))
            })?;
            for row in rows {
                let (page, kind, count) = row?;
                if !selected_pages.contains(&page) || count <= 0 {
                    continue;
                }
                let count = count as usize;
                match kind.as_str() {
                    "highlight" => counts.highlights += count,
                    "quote" => counts.quotes += count,
                    _ => counts.notes += count,
                }
            }
        }
        {
            let mut stmt = conn.prepare_cached(
                "SELECT chapter_index, COUNT(*) FROM reading_bookmarks
                 WHERE book_id = ?1 GROUP BY chapter_index",
            )?;
            let rows = stmt.query_map(params![book_id], |row| {
                Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (page, count) = row?;
                if selected_pages.contains(&page) && count > 0 {
                    counts.bookmarks += count as usize;
                }
            }
        }
        {
            let mut stmt = conn.prepare_cached(
                "SELECT page_id, COUNT(*) FROM pdf_drawings
                 WHERE book_id = ?1 GROUP BY page_id",
            )?;
            let rows = stmt.query_map(params![book_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?;
            for row in rows {
                let (page_id, count) = row?;
                if selected_ids.contains(page_id.as_str()) && count > 0 {
                    counts.drawings += count as usize;
                }
            }
        }
        Ok(counts)
    }

    /// Land a PDF page edit: the rewritten file and every
    /// page-bound row move in one step (decision 70).
    ///
    /// `edited_file` is the finished replacement, already written
    /// and verified beside `target_file`. The database work runs
    /// in one transaction; the file swap happens *inside* it,
    /// after the rows and before the commit, with the old file
    /// kept as a same-folder backup until the commit lands. A
    /// failed swap rolls the transaction back untouched; a failed
    /// commit puts the old file back. Only after both halves have
    /// landed is the sidecar — `kalam.json`, the backup copy of
    /// this same state — refreshed from the new truth.
    ///
    /// What moves, by decision 69: drawings follow their page
    /// identity (and turn with it), highlights and bookmarks
    /// follow their physical page, a deleted page's are deleted,
    /// the reading position follows its page or takes the nearest
    /// remaining one, and saved words — the book's, not a page's —
    /// are left exactly as they are. The OCR page cache is dropped
    /// outright: it is keyed by page number, and the pages it
    /// describes have just moved house.
    pub fn apply_pdf_page_edit(
        &self,
        book_id: i64,
        rewrite: &PdfPageEditRewrite,
        edited_file: &Path,
        target_file: &Path,
        new_hash: &str,
    ) -> Result<()> {
        if rewrite.ordered_page_ids.is_empty() {
            return Err(DbError::PdfAnnotation(
                "a page edit cannot leave a book with no pages".to_string(),
            ));
        }
        if edited_file == target_file
            || edited_file.parent() != target_file.parent()
            || !edited_file.is_file()
            || !target_file.is_file()
        {
            return Err(DbError::PdfAnnotation(
                "the edited PDF is not ready beside the book's copy".to_string(),
            ));
        }
        let backup_file = page_edit_backup_path(target_file);
        if backup_file.exists() {
            return Err(DbError::PdfAnnotation(
                "a page-edit backup is already in the way".to_string(),
            ));
        }

        let outcome = (|| -> Result<()> {
            let mut conn = self.conn();
            let tx = conn.transaction()?;

            let old_hash: String = tx.query_row(
                "SELECT file_hash FROM books WHERE id = ?1",
                params![book_id],
                |row| row.get(0),
            )?;

            let current: Vec<PdfPageIdentity> = {
                let mut stmt = tx.prepare(
                    "SELECT page_id, position FROM pdf_page_identities
                     WHERE book_id = ?1 ORDER BY position ASC",
                )?;
                let rows = stmt.query_map(params![book_id], |row| {
                    Ok(PdfPageIdentity {
                        page_id: row.get(0)?,
                        position: row.get::<_, i64>(1)?.max(0) as usize,
                    })
                })?;
                rows.collect::<std::result::Result<Vec<_>, _>>()?
            };
            if current
                .iter()
                .enumerate()
                .any(|(index, page)| page.position != index + 1)
            {
                return Err(DbError::PdfAnnotation(
                    "the book's page identities are not in step with its file"
                        .to_string(),
                ));
            }

            let new_position_by_id: HashMap<&str, usize> = rewrite
                .ordered_page_ids
                .iter()
                .enumerate()
                .map(|(index, page_id)| (page_id.as_str(), index + 1))
                .collect();
            if new_position_by_id.len() != rewrite.ordered_page_ids.len()
                || rewrite
                    .ordered_page_ids
                    .iter()
                    .any(|page_id| page_id.trim().is_empty())
            {
                return Err(DbError::PdfAnnotation(
                    "the edit names a page twice, or not at all".to_string(),
                ));
            }
            let new_total = rewrite.ordered_page_ids.len();
            let current_ids: HashSet<&str> =
                current.iter().map(|page| page.page_id.as_str()).collect();
            let mut survivor_old_to_new: HashMap<usize, usize> = HashMap::new();
            let mut deleted_pages: Vec<usize> = Vec::new();
            let mut deleted_ids: Vec<String> = Vec::new();
            for page in &current {
                match new_position_by_id.get(page.page_id.as_str()) {
                    Some(new_position) => {
                        survivor_old_to_new.insert(page.position, *new_position);
                    }
                    None => {
                        deleted_pages.push(page.position);
                        deleted_ids.push(page.page_id.clone());
                    }
                }
            }

            // Drawings on surviving pages turn with them, before
            // the identity rewrite below cascades the deleted
            // pages' drawings away.
            let rotation_by_id: HashMap<&str, &PdfPageRotationRewrite> = rewrite
                .rotations
                .iter()
                .filter(|rotation| {
                    rotation.clockwise_degrees.rem_euclid(360) != 0
                        && new_position_by_id.contains_key(rotation.page_id.as_str())
                })
                .map(|rotation| (rotation.page_id.as_str(), rotation))
                .collect();
            if !rotation_by_id.is_empty() {
                let drawings: Vec<PdfDrawing> = {
                    let mut stmt = tx.prepare(
                        "SELECT id, book_id, page_id, color, opacity, width_pts,
                                geometry, created_at, updated_at
                         FROM pdf_drawings WHERE book_id = ?1 ORDER BY id ASC",
                    )?;
                    let rows = stmt.query_map(params![book_id], |row| {
                        Ok((
                            row.get(0)?,
                            row.get(1)?,
                            row.get(2)?,
                            row.get(3)?,
                            row.get(4)?,
                            row.get(5)?,
                            row.get(6)?,
                            row.get(7)?,
                            row.get(8)?,
                        ))
                    })?;
                    rows.map(|row| row.map_err(Into::into).and_then(drawing_from_row))
                        .collect::<Result<Vec<_>>>()?
                };
                let now = chrono_like_now();
                for mut drawing in drawings {
                    let Some(rotation) = rotation_by_id.get(drawing.page_id.as_str())
                    else {
                        continue;
                    };
                    drawing.geometry.rotate_clockwise(
                        rotation.clockwise_degrees,
                        rotation.page_width,
                        rotation.page_height,
                    );
                    tx.execute(
                        "UPDATE pdf_drawings SET geometry = ?1, updated_at = ?2
                         WHERE id = ?3",
                        params![encode_geometry(&drawing)?, now, drawing.id],
                    )?;
                }
            }

            // Highlights and bookmarks name a physical page. A
            // deleted page's go first; the rest move through a
            // far-negative holding encoding so a swap performed as
            // two updates cannot undo itself.
            const HOLDING_OFFSET: i64 = 1_000_000;
            for table in ["annotations", "reading_bookmarks"] {
                for old_page in &deleted_pages {
                    tx.execute(
                        &format!(
                            "DELETE FROM {table}
                             WHERE book_id = ?1 AND chapter_index = ?2"
                        ),
                        params![book_id, *old_page as i64],
                    )?;
                }
                for (old_page, new_page) in &survivor_old_to_new {
                    if old_page != new_page {
                        tx.execute(
                            &format!(
                                "UPDATE {table} SET chapter_index = ?1
                                 WHERE book_id = ?2 AND chapter_index = ?3"
                            ),
                            params![
                                -(HOLDING_OFFSET + *new_page as i64),
                                book_id,
                                *old_page as i64
                            ],
                        )?;
                    }
                }
                tx.execute(
                    &format!(
                        "UPDATE {table}
                         SET chapter_index = -chapter_index - {HOLDING_OFFSET}
                         WHERE book_id = ?1 AND chapter_index <= -{HOLDING_OFFSET}"
                    ),
                    params![book_id],
                )?;
            }

            if let Some((old_page, _)) = tx
                .query_row(
                    "SELECT chapter_index, fraction FROM reading_progress
                     WHERE book_id = ?1",
                    params![book_id],
                    |row| Ok((row.get::<_, i64>(0)?.max(1) as usize, row.get::<_, f64>(1)?)),
                )
                .optional()?
            {
                let new_page = survivor_old_to_new
                    .get(&old_page)
                    .copied()
                    .or_else(|| {
                        nearest_remaining_new_page(old_page, &survivor_old_to_new)
                    })
                    .unwrap_or(1)
                    .clamp(1, new_total);
                let fraction = new_page as f64 / new_total as f64;
                let progress = (((new_page as f64 + fraction) / new_total as f64)
                    * 100.0)
                    .round()
                    .clamp(0.0, 100.0) as i64;
                tx.execute(
                    "UPDATE reading_progress
                     SET chapter_index = ?1, fraction = ?2, updated_at = ?3
                     WHERE book_id = ?4",
                    params![new_page as i64, fraction, chrono_like_now(), book_id],
                )?;
                tx.execute(
                    "UPDATE books SET progress = ?1 WHERE id = ?2",
                    params![progress, book_id],
                )?;
            }

            tx.execute(
                "DELETE FROM page_ocr_cache WHERE book_id = ?1",
                params![book_id],
            )?;

            // Identities: deleted names go (their drawings cascade
            // with them), every page of the new order parks at the
            // negative of its new position, then one flip lands
            // them all without a uniqueness collision mid-swap.
            for page_id in &deleted_ids {
                tx.execute(
                    "DELETE FROM pdf_page_identities
                     WHERE book_id = ?1 AND page_id = ?2",
                    params![book_id, page_id],
                )?;
            }
            for (index, page_id) in rewrite.ordered_page_ids.iter().enumerate() {
                let parked = -((index + 1) as i64);
                if current_ids.contains(page_id.as_str()) {
                    tx.execute(
                        "UPDATE pdf_page_identities SET position = ?1
                         WHERE book_id = ?2 AND page_id = ?3",
                        params![parked, book_id, page_id],
                    )?;
                } else {
                    tx.execute(
                        "INSERT INTO pdf_page_identities (book_id, page_id, position)
                         VALUES (?1, ?2, ?3)",
                        params![book_id, page_id, parked],
                    )?;
                }
            }
            tx.execute(
                "UPDATE pdf_page_identities SET position = -position
                 WHERE book_id = ?1 AND position < 0",
                params![book_id],
            )?;

            if old_hash != new_hash {
                tx.execute(
                    "UPDATE books SET file_hash = ?1 WHERE id = ?2",
                    params![new_hash, book_id],
                )?;
                tx.execute(
                    "DELETE FROM metadata_overrides WHERE file_hash = ?1",
                    params![new_hash],
                )?;
                tx.execute(
                    "UPDATE metadata_overrides SET file_hash = ?1
                     WHERE file_hash = ?2",
                    params![new_hash, old_hash],
                )?;
            }

            // The swap itself, inside the transaction: from here a
            // failure either restores the old file or rolls the
            // rows back, so the file and the catalog cannot be
            // left telling different stories.
            std::fs::rename(target_file, &backup_file)?;
            if let Err(error) = std::fs::rename(edited_file, target_file) {
                let _ = std::fs::rename(&backup_file, target_file);
                return Err(error.into());
            }
            if let Err(error) = tx.commit() {
                let _ = std::fs::remove_file(target_file);
                let _ = std::fs::rename(&backup_file, target_file);
                return Err(error.into());
            }
            Ok(())
        })();

        match outcome {
            Ok(()) => {
                let _ = std::fs::remove_file(&backup_file);
                sync_parent_dir(target_file);
                crate::sidecar::refresh_for_book(self, book_id);
                Ok(())
            }
            Err(error) => {
                // Last-resort restoration if the swap itself was
                // interrupted between its two renames.
                if backup_file.exists() && !target_file.exists() {
                    let _ = std::fs::rename(&backup_file, target_file);
                }
                Err(error)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BookFormat;

    fn catalog_with_pdf() -> (Catalog, i64) {
        let cat = Catalog::open_in_memory().expect("in-memory catalog");
        let id = cat
            .insert_book(
                "uuid-pdf-draw-test",
                "Drawing Book",
                "Author",
                None,
                "",
                BookFormat::Pdf,
                "book.pdf",
                "hash-pdf-draw-test",
                None,
                &[],
            )
            .expect("insert book");
        (cat, id)
    }

    fn a_stroke(book_id: i64, page_id: &str) -> PdfDrawing {
        PdfDrawing {
            id: 0,
            book_id,
            page_id: page_id.to_string(),
            color: "ink".to_string(),
            opacity: 1.0,
            width_pts: 3.0,
            geometry: PdfDrawingGeometry::Stroke {
                tool: "pen".to_string(),
                points: vec![(10.0, 20.0), (30.0, 40.0)],
            },
            created_at: String::new(),
            updated_at: String::new(),
        }
    }

    #[test]
    fn page_identities_are_stable_and_positions_fill_in() {
        let (cat, book_id) = catalog_with_pdf();
        let pages = cat
            .ensure_pdf_page_identities(book_id, 3)
            .expect("ensure identities");
        assert_eq!(pages.len(), 3);
        assert_eq!(pages[0].page_id, "page-1");
        assert_eq!(pages[2].position, 3);

        // A second call names nothing new; a longer file only appends.
        let again = cat
            .ensure_pdf_page_identities(book_id, 3)
            .expect("ensure again");
        assert_eq!(again, pages);
        let longer = cat
            .ensure_pdf_page_identities(book_id, 4)
            .expect("ensure longer");
        assert_eq!(longer.len(), 4);
        assert_eq!(longer[3].page_id, "page-4");
        assert_eq!(&longer[..3], &pages[..]);
    }

    #[test]
    fn drawings_round_trip_update_delete_and_restore() {
        let (cat, book_id) = catalog_with_pdf();
        let pages = cat
            .ensure_pdf_page_identities(book_id, 2)
            .expect("ensure identities");
        let id = cat
            .insert_pdf_drawing(&a_stroke(book_id, &pages[1].page_id))
            .expect("insert drawing");

        let stored = cat
            .get_pdf_drawing(id)
            .expect("get drawing")
            .expect("drawing stored");
        assert_eq!(stored.page_id, "page-2");
        assert_eq!(stored.color, "ink");
        match &stored.geometry {
            PdfDrawingGeometry::Stroke { tool, points } => {
                assert_eq!(tool, "pen");
                assert_eq!(points, &vec![(10.0, 20.0), (30.0, 40.0)]);
            }
            other => panic!("expected a stroke, got {other:?}"),
        }

        let mut moved = stored.clone();
        moved.color = "blue".to_string();
        moved.geometry.translate(5.0, -5.0);
        cat.update_pdf_drawing(&moved).expect("update drawing");
        let after = cat.pdf_drawings_for_book(book_id).expect("list drawings");
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].color, "blue");
        match &after[0].geometry {
            PdfDrawingGeometry::Stroke { points, .. } => {
                assert_eq!(points[0], (15.0, 15.0));
            }
            other => panic!("expected a stroke, got {other:?}"),
        }

        cat.delete_pdf_drawing(id).expect("delete drawing");
        assert!(cat
            .pdf_drawings_for_book(book_id)
            .expect("list drawings")
            .is_empty());
        cat.restore_pdf_drawing(&after[0]).expect("restore drawing");
        let back = cat
            .get_pdf_drawing(id)
            .expect("get drawing")
            .expect("drawing restored");
        assert_eq!(back.id, id);
        assert_eq!(back.created_at, after[0].created_at);
    }

    #[test]
    fn geometry_turns_with_its_page() {
        let mut stroke = PdfDrawingGeometry::Stroke {
            tool: "pen".to_string(),
            points: vec![(25.0, 50.0)],
        };
        stroke.rotate_clockwise(90, 100.0, 200.0);
        match &stroke {
            PdfDrawingGeometry::Stroke { points, .. } => {
                assert_eq!(points, &vec![(150.0, 25.0)]);
            }
            other => panic!("expected a stroke, got {other:?}"),
        }

        let mut text = PdfDrawingGeometry::Text {
            rect: (10.0, 20.0, 30.0, 40.0),
            text: "note".to_string(),
            size: "m".to_string(),
            paper_background: true,
        };
        text.rotate_clockwise(270, 100.0, 200.0);
        match &text {
            PdfDrawingGeometry::Text { rect, .. } => {
                assert_eq!(*rect, (20.0, 60.0, 40.0, 30.0));
            }
            other => panic!("expected text, got {other:?}"),
        }
    }

    #[test]
    fn applying_a_page_edit_moves_everything_with_its_page() {
        let (cat, book_id) = catalog_with_pdf();
        let pages = cat
            .ensure_pdf_page_identities(book_id, 3)
            .expect("ensure identities");

        // Highlights on all three pages, bookmarks and drawings
        // on the second and third, a saved word and an OCR page
        // on the second, and the reader standing on the second.
        for page in 1..=3 {
            cat.insert_annotation(
                book_id,
                "highlight",
                page,
                "",
                0,
                "",
                0,
                "yellow",
                "solid",
                &format!("words on page {page}"),
                "",
            )
            .expect("insert highlight");
        }
        for page in [2, 3] {
            cat.insert_reading_bookmark(book_id, page, 0.0, "")
                .expect("insert bookmark");
            let mut drawing = a_stroke(book_id, &pages[(page - 1) as usize].page_id);
            drawing.geometry = PdfDrawingGeometry::Stroke {
                tool: "pen".to_string(),
                points: vec![(25.0, 50.0)],
            };
            cat.insert_pdf_drawing(&drawing).expect("insert drawing");
        }
        cat.insert_saved_word(
            "serendipity",
            "a happy accident",
            None,
            Some(book_id),
            Some(2),
            None,
        )
        .expect("insert saved word");
        cat.set_reading_progress(book_id, 2, 0.5, 3)
            .expect("save progress");
        cat.save_page_ocr(
            book_id,
            2,
            "fingerprint",
            &crate::pdf::PdfPageText {
                page_num: 2,
                width_pts: 100.0,
                height_pts: 200.0,
                lines: Vec::new(),
            },
        )
        .expect("cache an OCR page");

        let counts = cat
            .pdf_page_bound_counts(
                book_id,
                &[
                    ("page-2".to_string(), 2),
                    ("page-3".to_string(), 3),
                ],
            )
            .expect("count page-bound work");
        assert_eq!(
            counts,
            PdfPageBoundCounts {
                highlights: 2,
                quotes: 0,
                notes: 0,
                drawings: 2,
                bookmarks: 2,
            }
        );

        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "kalam-page-edit-db-test-{}-{stamp}",
            std::process::id()
        ));
        std::fs::create_dir_all(&dir).expect("create the fixture folder");
        let target = dir.join("book.pdf");
        let edited = dir.join("book.pdf.kalam-editing");
        std::fs::write(&target, b"old pdf").expect("write the old copy");
        std::fs::write(&edited, b"new pdf").expect("write the edited copy");

        // New order: page 3 (turned a quarter clockwise), an
        // inserted page, then page 1. Page 2 is deleted.
        let outcome = cat.apply_pdf_page_edit(
            book_id,
            &PdfPageEditRewrite {
                ordered_page_ids: vec![
                    "page-3".to_string(),
                    "inserted-1".to_string(),
                    "page-1".to_string(),
                ],
                rotations: vec![PdfPageRotationRewrite {
                    page_id: "page-3".to_string(),
                    page_width: 100.0,
                    page_height: 200.0,
                    clockwise_degrees: 90,
                }],
            },
            &edited,
            &target,
            "hash-after-edit",
        );
        let file_now = std::fs::read(&target).unwrap_or_default();
        let leftovers: Vec<String> = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .filter_map(|entry| entry.ok())
                    .map(|entry| entry.file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        let _ = std::fs::remove_dir_all(&dir);
        outcome.expect("apply the page edit");
        assert_eq!(file_now, b"new pdf", "the managed copy was replaced");
        assert_eq!(
            leftovers,
            vec!["book.pdf".to_string()],
            "neither the staging file nor the backup is left behind"
        );

        let identities = cat
            .pdf_page_identities(book_id)
            .expect("read identities back");
        assert_eq!(
            identities
                .iter()
                .map(|page| (page.page_id.as_str(), page.position))
                .collect::<Vec<_>>(),
            vec![("page-3", 1), ("inserted-1", 2), ("page-1", 3)]
        );

        let annotations = cat
            .get_annotations_for_book(book_id)
            .expect("read highlights back");
        assert_eq!(
            annotations
                .iter()
                .map(|row| (row.chapter_index, row.text_excerpt.as_str()))
                .collect::<Vec<_>>(),
            vec![(1, "words on page 3"), (3, "words on page 1")],
            "highlights followed their pages; page 2's is gone"
        );
        let bookmarks = cat
            .list_reading_bookmarks(book_id)
            .expect("read bookmarks back");
        assert_eq!(
            bookmarks
                .iter()
                .map(|mark| mark.chapter_index)
                .collect::<Vec<_>>(),
            vec![1]
        );
        let drawings = cat
            .pdf_drawings_for_book(book_id)
            .expect("read drawings back");
        assert_eq!(drawings.len(), 1, "the deleted page's drawing is gone");
        assert_eq!(drawings[0].page_id, "page-3");
        match &drawings[0].geometry {
            PdfDrawingGeometry::Stroke { points, .. } => {
                assert_eq!(points, &vec![(150.0, 25.0)], "the stroke turned too");
            }
            other => panic!("expected a stroke, got {other:?}"),
        }

        assert_eq!(
            cat.get_reading_progress(book_id).expect("read progress"),
            Some((1, 1.0 / 3.0)),
            "a deleted reading page settles on the nearest remaining one"
        );
        let words = cat
            .saved_words_for_books(&[book_id])
            .expect("read saved words");
        assert_eq!(words.len(), 1, "saved words belong to the book");
        assert_eq!(words[0].chapter_index, Some(2), "and are left untouched");
        assert!(
            cat.load_page_ocr(book_id, 2, "fingerprint")
                .expect("read the OCR cache")
                .is_none(),
            "page-numbered OCR cannot survive the pages moving"
        );
        assert_eq!(
            cat.get_book(book_id)
                .expect("read the book")
                .expect("book present")
                .file_hash,
            "hash-after-edit"
        );
    }

    #[test]
    fn drawings_need_a_real_page_identity() {
        let (cat, book_id) = catalog_with_pdf();
        // No identities yet: the foreign key refuses an object that
        // names a page Kalam never gave an identity to.
        assert!(cat.insert_pdf_drawing(&a_stroke(book_id, "page-1")).is_err());
    }
}
