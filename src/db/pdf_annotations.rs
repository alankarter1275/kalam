//! PDF page identities and drawing-object queries.
//!
//! Drawings name the page they live on by stable identity, never by
//! page number (decisions 66, 69 and 72). SQLite is the source of
//! truth; every mutation refreshes the book's sidecar snapshot, like
//! the older annotation paths.

use super::*;
use std::collections::HashSet;

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
    fn drawings_need_a_real_page_identity() {
        let (cat, book_id) = catalog_with_pdf();
        // No identities yet: the foreign key refuses an object that
        // names a page Kalam never gave an identity to.
        assert!(cat.insert_pdf_drawing(&a_stroke(book_id, "page-1")).is_err());
    }
}
