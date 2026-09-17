//! Tag management and bulk metadata operations.

use super::*;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct BulkMetadataEdit {
    /// If Some, add these tags to all selected books.
    pub add_tags: Option<Vec<String>>,
    /// If Some, set/replace tags for all selected books with these tags.
    pub set_tags: Option<Vec<String>>,
    /// If Some, update author for all selected books.
    pub author: Option<String>,
    /// If Some, update publisher for all selected books.
    pub publisher: Option<String>,
    /// If Some, update reading status ("Unread", "Reading", "Read"/"Finished").
    pub reading_status: Option<String>,
}

impl Catalog {
    /// Rename a tag across all books in the catalog and refresh sidecar backups.
    pub fn rename_tag(&self, old_name: &str, new_name: &str) -> Result<()> {
        let old_name = old_name.trim();
        let new_name = new_name.trim();
        if old_name.is_empty() || new_name.is_empty() || old_name == new_name {
            return Ok(());
        }

        let mut conn = self.conn();
        let old_tag_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                params![old_name],
                |r| r.get(0),
            )
            .optional()?;

        let Some(old_id) = old_tag_id else {
            return Ok(());
        };

        // Find affected books
        let mut stmt = conn.prepare("SELECT DISTINCT book_id FROM book_tags WHERE tag_id = ?1")?;
        let book_ids: Vec<i64> = stmt
            .query_map(params![old_id], |r| r.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);

        let new_tag_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                params![new_name],
                |r| r.get(0),
            )
            .optional()?;

        let tx = conn.transaction()?;

        if let Some(new_id) = new_tag_id {
            if old_id != new_id {
                // Target tag already exists under another ID: move books and drop old tag
                tx.execute(
                    "UPDATE OR IGNORE book_tags SET tag_id = ?1 WHERE tag_id = ?2",
                    params![new_id, old_id],
                )?;
                tx.execute("DELETE FROM book_tags WHERE tag_id = ?1", params![old_id])?;
                tx.execute("DELETE FROM tags WHERE id = ?1", params![old_id])?;
            } else {
                // Same ID (casing change e.g. "scifi" -> "SciFi")
                tx.execute(
                    "UPDATE tags SET name = ?1 WHERE id = ?2",
                    params![new_name, old_id],
                )?;
            }
        } else {
            // New tag does not exist: rename tag in place
            tx.execute(
                "UPDATE tags SET name = ?1 WHERE id = ?2",
                params![new_name, old_id],
            )?;
        }

        tx.execute(
            "DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM book_tags)",
            [],
        )?;
        tx.commit()?;
        drop(conn);

        for book_id in book_ids {
            let _ = self.remember_overrides(book_id);
            crate::sidecar::refresh_for_book(self, book_id);
        }

        Ok(())
    }

    /// Move all books under `source_tag` into `target_tag` and remove `source_tag`.
    pub fn merge_tags(&self, source_tag: &str, target_tag: &str) -> Result<()> {
        let source_tag = source_tag.trim();
        let target_tag = target_tag.trim();
        if source_tag.is_empty() || target_tag.is_empty() {
            return Ok(());
        }
        if source_tag.eq_ignore_ascii_case(target_tag) {
            return self.rename_tag(source_tag, target_tag);
        }

        let mut conn = self.conn();
        let source_tag_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                params![source_tag],
                |r| r.get(0),
            )
            .optional()?;

        let Some(source_id) = source_tag_id else {
            return Ok(());
        };

        let mut stmt = conn.prepare("SELECT DISTINCT book_id FROM book_tags WHERE tag_id = ?1")?;
        let book_ids: Vec<i64> = stmt
            .query_map(params![source_id], |r| r.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);

        let tx = conn.transaction()?;
        tx.execute(
            "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
            params![target_tag],
        )?;
        let target_id: i64 = tx.query_row(
            "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
            params![target_tag],
            |r| r.get(0),
        )?;

        if source_id != target_id {
            tx.execute(
                "UPDATE OR IGNORE book_tags SET tag_id = ?1 WHERE tag_id = ?2",
                params![target_id, source_id],
            )?;
            tx.execute("DELETE FROM book_tags WHERE tag_id = ?1", params![source_id])?;
            tx.execute("DELETE FROM tags WHERE id = ?1", params![source_id])?;
        }

        tx.execute(
            "DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM book_tags)",
            [],
        )?;
        tx.commit()?;
        drop(conn);

        for book_id in book_ids {
            let _ = self.remember_overrides(book_id);
            crate::sidecar::refresh_for_book(self, book_id);
        }

        Ok(())
    }

    /// Disassociate `tag_name` from all books and delete it from `tags` table.
    pub fn delete_tag(&self, tag_name: &str) -> Result<()> {
        let tag_name = tag_name.trim();
        if tag_name.is_empty() {
            return Ok(());
        }

        let mut conn = self.conn();
        let tag_id: Option<i64> = conn
            .query_row(
                "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                params![tag_name],
                |r| r.get(0),
            )
            .optional()?;

        let Some(id) = tag_id else {
            return Ok(());
        };

        let mut stmt = conn.prepare("SELECT DISTINCT book_id FROM book_tags WHERE tag_id = ?1")?;
        let book_ids: Vec<i64> = stmt
            .query_map(params![id], |r| r.get(0))?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        drop(stmt);

        let tx = conn.transaction()?;
        tx.execute("DELETE FROM book_tags WHERE tag_id = ?1", params![id])?;
        tx.execute("DELETE FROM tags WHERE id = ?1", params![id])?;
        tx.execute(
            "DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM book_tags)",
            [],
        )?;
        tx.commit()?;
        drop(conn);

        for book_id in book_ids {
            let _ = self.remember_overrides(book_id);
            crate::sidecar::refresh_for_book(self, book_id);
        }

        Ok(())
    }

    /// Bulk edit metadata fields across selected books in a single database transaction.
    pub fn bulk_update_metadata(&self, book_ids: &[i64], edit: &BulkMetadataEdit) -> Result<()> {
        if book_ids.is_empty() {
            return Ok(());
        }

        let mut conn = self.conn();
        let tx = conn.transaction()?;
        let now = chrono_like_now();

        for &book_id in book_ids {
            if let Some(ref author) = edit.author {
                let author = author.trim();
                if !author.is_empty() {
                    tx.execute(
                        "UPDATE books SET authors = ?2 WHERE id = ?1",
                        params![book_id, author],
                    )?;
                }
            }

            if let Some(ref publisher) = edit.publisher {
                let publisher = publisher.trim();
                if !publisher.is_empty() {
                    tx.execute(
                        "UPDATE books SET publisher = ?2 WHERE id = ?1",
                        params![book_id, publisher],
                    )?;
                }
            }

            if let Some(ref status) = edit.reading_status {
                match status.to_lowercase().as_str() {
                    "unread" => {
                        tx.execute(
                            "UPDATE books SET progress = 0, finished_at = NULL WHERE id = ?1",
                            params![book_id],
                        )?;
                    }
                    "reading" => {
                        tx.execute(
                            "UPDATE books SET progress = CASE WHEN progress = 0 OR progress >= 100 THEN 1 ELSE progress END, finished_at = NULL WHERE id = ?1",
                            params![book_id],
                        )?;
                    }
                    "read" | "finished" => {
                        tx.execute(
                            "UPDATE books SET progress = 100, finished_at = ?2 WHERE id = ?1",
                            params![book_id, now],
                        )?;
                        tx.execute(
                            "DELETE FROM reading_list WHERE book_id = ?1",
                            params![book_id],
                        )?;
                    }
                    _ => {}
                }
            }

            if let Some(ref tags) = edit.set_tags {
                tx.execute("DELETE FROM book_tags WHERE book_id = ?1", params![book_id])?;
                for tag in tags {
                    let tag = tag.trim();
                    if tag.is_empty() {
                        continue;
                    }
                    tx.execute(
                        "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
                        params![tag],
                    )?;
                    let tag_id: i64 = tx.query_row(
                        "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                        params![tag],
                        |r| r.get(0),
                    )?;
                    tx.execute(
                        "INSERT OR IGNORE INTO book_tags (book_id, tag_id) VALUES (?1, ?2)",
                        params![book_id, tag_id],
                    )?;
                }
            }

            if let Some(ref tags) = edit.add_tags {
                for tag in tags {
                    let tag = tag.trim();
                    if tag.is_empty() {
                        continue;
                    }
                    tx.execute(
                        "INSERT OR IGNORE INTO tags (name) VALUES (?1)",
                        params![tag],
                    )?;
                    let tag_id: i64 = tx.query_row(
                        "SELECT id FROM tags WHERE name = ?1 COLLATE NOCASE",
                        params![tag],
                        |r| r.get(0),
                    )?;
                    tx.execute(
                        "INSERT OR IGNORE INTO book_tags (book_id, tag_id) VALUES (?1, ?2)",
                        params![book_id, tag_id],
                    )?;
                }
            }
        }

        tx.execute(
            "DELETE FROM tags WHERE id NOT IN (SELECT tag_id FROM book_tags)",
            [],
        )?;
        tx.commit()?;
        drop(conn);

        for &book_id in book_ids {
            let _ = self.remember_overrides(book_id);
            crate::sidecar::refresh_for_book(self, book_id);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BookFormat;

    fn insert_test_book(cat: &Catalog, title: &str, author: &str, tags: &[&str]) -> i64 {
        let uuid = format!("uuid-{}", title.to_lowercase().replace(' ', "-"));
        let tags_vec: Vec<String> = tags.iter().map(|s| s.to_string()).collect();
        cat.insert_book(
            &uuid,
            title,
            author,
            None,
            "",
            BookFormat::Epub,
            "book.epub",
            &format!("hash-{}", uuid),
            None,
            &tags_vec,
        )
        .expect("insert book")
    }

    #[test]
    fn test_rename_tag() {
        let cat = Catalog::open_in_memory().unwrap();
        let b1 = insert_test_book(&cat, "Book 1", "Author A", &["sci-fi", "classic"]);
        let b2 = insert_test_book(&cat, "Book 2", "Author B", &["sci-fi"]);

        cat.rename_tag("sci-fi", "Science Fiction").unwrap();

        let book1 = cat.get_book(b1).unwrap().unwrap();
        assert!(book1.tags.contains(&"Science Fiction".to_string()));
        assert!(book1.tags.contains(&"classic".to_string()));

        let book2 = cat.get_book(b2).unwrap().unwrap();
        assert_eq!(book2.tags, vec!["Science Fiction"]);
    }

    #[test]
    fn test_merge_tags() {
        let cat = Catalog::open_in_memory().unwrap();
        let b1 = insert_test_book(&cat, "Book 1", "Author A", &["fantasy"]);
        let b2 = insert_test_book(&cat, "Book 2", "Author B", &["magic"]);

        cat.merge_tags("fantasy", "magic").unwrap();

        let book1 = cat.get_book(b1).unwrap().unwrap();
        assert_eq!(book1.tags, vec!["magic"]);

        let book2 = cat.get_book(b2).unwrap().unwrap();
        assert_eq!(book2.tags, vec!["magic"]);

        let tag_counts = cat.list_tags_with_counts().unwrap();
        assert_eq!(tag_counts.len(), 1);
        assert_eq!(tag_counts[0].0, "magic");
        assert_eq!(tag_counts[0].1, 2);
    }

    #[test]
    fn test_delete_tag() {
        let cat = Catalog::open_in_memory().unwrap();
        let b1 = insert_test_book(&cat, "Book 1", "Author A", &["temp", "keep"]);

        cat.delete_tag("temp").unwrap();

        let book1 = cat.get_book(b1).unwrap().unwrap();
        assert_eq!(book1.tags, vec!["keep"]);

        let tag_counts = cat.list_tags_with_counts().unwrap();
        assert_eq!(tag_counts.len(), 1);
        assert_eq!(tag_counts[0].0, "keep");
    }

    #[test]
    fn test_bulk_update_metadata() {
        let cat = Catalog::open_in_memory().unwrap();
        let b1 = insert_test_book(&cat, "Book 1", "Author A", &["old"]);
        let b2 = insert_test_book(&cat, "Book 2", "Author A", &["old"]);

        let edit = BulkMetadataEdit {
            add_tags: Some(vec!["new_tag".into()]),
            author: Some("Updated Author".into()),
            publisher: Some("Test Press".into()),
            reading_status: Some("Finished".into()),
            ..Default::default()
        };

        cat.bulk_update_metadata(&[b1, b2], &edit).unwrap();

        let book1 = cat.get_book(b1).unwrap().unwrap();
        assert_eq!(book1.authors, "Updated Author");
        assert_eq!(book1.publisher, "Test Press");
        assert_eq!(book1.progress, 100);
        assert!(book1.tags.contains(&"old".to_string()));
        assert!(book1.tags.contains(&"new_tag".to_string()));

        let book2 = cat.get_book(b2).unwrap().unwrap();
        assert_eq!(book2.authors, "Updated Author");
        assert_eq!(book2.publisher, "Test Press");
        assert_eq!(book2.progress, 100);
        assert!(book2.tags.contains(&"old".to_string()));
        assert!(book2.tags.contains(&"new_tag".to_string()));
    }

    #[test]
    fn test_rename_tag_case_change() {
        let cat = Catalog::open_in_memory().unwrap();
        let b1 = insert_test_book(&cat, "Book 1", "Author A", &["scifi"]);
        cat.rename_tag("scifi", "SciFi").unwrap();
        let book1 = cat.get_book(b1).unwrap().unwrap();
        assert_eq!(book1.tags, vec!["SciFi"]);
    }

    #[test]
    fn test_bulk_update_set_tags() {
        let cat = Catalog::open_in_memory().unwrap();
        let b1 = insert_test_book(&cat, "Book 1", "Author A", &["old_tag1", "old_tag2"]);

        let edit = BulkMetadataEdit {
            set_tags: Some(vec!["replaced_tag".into()]),
            reading_status: Some("Reading".into()),
            ..Default::default()
        };

        cat.bulk_update_metadata(&[b1], &edit).unwrap();

        let book1 = cat.get_book(b1).unwrap().unwrap();
        assert_eq!(book1.tags, vec!["replaced_tag"]);
        assert_eq!(book1.progress, 1);
    }
}
