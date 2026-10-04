//! Series cache — remote series listings for the book page's series float.
//!
//! A series spans books you may not own, and the local catalog can only know
//! about the ones in the library. So the complete listing is fetched from
//! Open Library once per series, cached here together with the downloaded
//! covers, and only refreshed when the user asks (the ⟳ button in the float).
//!
//! The cache key includes the first author because two series can share a
//! name. The fetch itself is by series name only, so a key collision means a
//! slightly redundant fetch at worst — never wrong data.

use super::*;

/// One work in a cached series listing.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SeriesWork {
    pub title: String,
    /// Open Library work key; empty when the provider gave none.
    #[serde(default)]
    pub key: String,
    /// Local path of the downloaded cover; empty when there was none.
    #[serde(default)]
    pub cover_path: String,
    /// First publish year per the provider; 0 when unknown.
    #[serde(default)]
    pub year: i64,
    /// First author per the provider; empty when unknown.
    #[serde(default)]
    pub author: String,
}

/// A decoded cache row.
#[derive(Debug, Clone)]
pub struct SeriesCacheEntry {
    #[allow(dead_code)] // series grouping reads name/cover; the provenance key is not surfaced
    pub series_key: String,
    #[allow(dead_code)] // as above
    pub source: String,
    pub fetched_at: String,
    pub works: Vec<SeriesWork>,
}

type RawChapterRow = (i64, i64, i64, f32, Option<f32>, String, String);

/// Normalised key for a series: what the listing is *about*, not how one
/// book's OPF spells it. A leading article is dropped from the series name
/// so "The Kingkiller Chronicle" and "Kingkiller Chronicle" share one cache
/// entry; the author part is only case/punctuation-normalised, never
/// article-stripped ("A. A. Milne" stays under the a's).
pub fn series_key(series: &str, first_author: &str) -> String {
    let norm = |s: &str| {
        s.to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>()
    };
    format!(
        "{}|{}",
        norm(strip_leading_article(series)),
        norm(first_author)
    )
}

/// Drop a leading "the"/"a"/"an" token, when there is more than one word.
fn strip_leading_article(series: &str) -> &str {
    let trimmed = series.trim_start();
    let mut words = trimmed.splitn(2, char::is_whitespace);
    match (words.next(), words.next()) {
        (Some(first), Some(rest))
            if matches!(first.to_ascii_lowercase().as_str(), "the" | "a" | "an") =>
        {
            rest.trim_start()
        }
        _ => trimmed,
    }
}

/// The dedup key for a comic that has no relational `comic_chapters` row:
/// the series column, else the series parsed out of the title, else the title
/// — lowercased. This is the exact rule `collapse_comic_chapters` applies, so
/// removal and counting helpers must use it too or they would disagree with
/// what the grids display.
pub(crate) fn heuristic_series_key(title: &str, series: Option<&str>) -> String {
    let name = series
        .map(str::to_string)
        .or_else(|| crate::comics::parse_comic_title(title).series)
        .unwrap_or_else(|| title.to_string());
    name.to_lowercase()
}

impl Catalog {
    // -----------------------------------------------------------------------
    // P5.5: series cache (v10)
    // -----------------------------------------------------------------------

    /// Cached listing for `series_key`, or `None` on first sight (a corrupted
    /// row also counts as a miss: the next open simply re-fetches).
    pub fn get_cached_series(&self, series_key: &str) -> Result<Option<SeriesCacheEntry>> {
        let conn = self.conn();
        let row: Option<(String, String, String, String)> = conn
            .query_row(
                "SELECT series_key, source, fetched_at, works_json
                 FROM series_cache WHERE series_key = ?1",
                params![series_key],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()?;

        let Some((series_key, source, fetched_at, works_json)) = row else {
            return Ok(None);
        };
        let Ok(works) = serde_json::from_str::<Vec<SeriesWork>>(&works_json) else {
            return Ok(None);
        };
        Ok(Some(SeriesCacheEntry {
            series_key,
            source,
            fetched_at,
            works,
        }))
    }

    /// Store (or overwrite) the cached listing for a series.
    pub fn upsert_series_cache(
        &self,
        series_key: &str,
        source: &str,
        works: &[SeriesWork],
    ) -> Result<()> {
        let json = serde_json::to_string(works)
            .map_err(|e| DbError::Io(std::io::Error::other(format!("encode series cache: {e}"))))?;
        let conn = self.conn();
        conn.execute(
            "INSERT INTO series_cache (series_key, source, fetched_at, works_json)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(series_key) DO UPDATE SET
                source = excluded.source,
                fetched_at = excluded.fetched_at,
                works_json = excluded.works_json",
            params![series_key, source, chrono_like_now(), json],
        )?;
        Ok(())
    }

    /// Books in the library that belong to `series`, ordered by position.
    ///
    /// Books with a real `series_index` come first (in order); the rest are
    /// grouped after them alphabetically, so a series whose OPFs never set an
    /// index still lists sensibly.
    pub fn books_in_series(&self, series: &str) -> Result<Vec<Book>> {
        let series = series.trim();
        if series.is_empty() {
            return Ok(Vec::new());
        }
        let conn = self.conn();
        let mut stmt = conn.prepare_cached(&format!(
            "SELECT {BOOK_COLUMNS} FROM books
             WHERE IFNULL(books.series,'') COLLATE NOCASE = ?1
             ORDER BY (books.series_index > 0) DESC, books.series_index ASC,
                      books.sort_title ASC, books.id ASC"
        ))?;
        let mut books = stmt
            .query_map(params![series], row_to_book)?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        hydrate_books(&conn, &mut books)?;
        Ok(books)
    }

    /// Auto-detect books belonging to `series` in local catalog,
    /// ordered by `series_index` (reading order).
    pub fn detect_local_series(&self, series: &str) -> Result<Vec<Book>> {
        self.books_in_series(series)
    }

    /// Update the series name and series index for a book.
    pub fn set_book_series(&self, book_id: i64, series: Option<&str>, series_index: f32) -> Result<()> {
        let conn = self.conn();
        conn.execute(
            "UPDATE books SET series = ?1, series_index = ?2 WHERE id = ?3",
            params![
                series.map(|s| s.trim()).filter(|s| !s.is_empty()),
                series_index.max(0.0) as f64,
                book_id,
            ],
        )?;
        Ok(())
    }

    /// Auto-detect and populate series metadata for comic archives (CBZ/CBR) in the library.
    #[allow(dead_code)]
    pub fn backfill_comic_series(&self) -> Result<usize> {
        let books = self.list_books(SortKey::Added, "")?;
        let mut updated = 0;
        for book in books {
            if !matches!(book.format, BookFormat::Cbz | BookFormat::Cbr) {
                continue;
            }
            let current_series = book.series.as_deref().unwrap_or("").trim().to_string();
            let current_idx = book.series_index;

            let (cleaned_series, detected_idx) = if !current_series.is_empty() {
                crate::comics::sanitize_comic_series(&current_series)
            } else {
                (String::new(), None)
            };

            let needs_clean = !current_series.is_empty()
                && (cleaned_series != current_series || (current_idx <= 0.0 && detected_idx.is_some()));
            let needs_detect = current_series.is_empty();

            if needs_clean {
                let final_idx = if let Some(idx) = detected_idx {
                    idx
                } else if current_idx > 0.0 {
                    current_idx
                } else {
                    crate::comics::parse_comic_title(&book.title)
                        .number
                        .unwrap_or(0.0)
                };
                self.set_book_series(book.id, Some(&cleaned_series), final_idx)?;
                updated += 1;
            } else if needs_detect {
                let mut meta = crate::comics::parse_comic_info(&book.file_path);
                if meta.series.is_none() {
                    let title_meta = crate::comics::parse_comic_title(&book.title);
                    if title_meta.series.is_some() {
                        meta.series = title_meta.series;
                        if meta.number.is_none() {
                            meta.number = title_meta.number;
                        }
                    }
                } else if meta.number.is_none() {
                    let title_meta = crate::comics::parse_comic_title(&book.title);
                    if title_meta.number.is_some() {
                        meta.number = title_meta.number;
                    }
                }
                if let Some(ref s) = meta.series {
                    let (clean_s, ch_opt) = crate::comics::sanitize_comic_series(s);
                    let idx = meta.number.or(ch_opt).unwrap_or(0.0);
                    self.set_book_series(book.id, Some(&clean_s), idx)?;
                    updated += 1;
                }
            }
        }
        Ok(updated)
    }

    /// Get or create a comic series entry by title (case-insensitive).
    pub fn get_or_create_comic_series(
        &self,
        title: &str,
        author: Option<&str>,
        description: Option<&str>,
    ) -> Result<i64> {
        let conn = self.conn();
        let title_clean = title.trim();
        let existing: Option<i64> = conn
            .query_row(
                "SELECT id FROM comic_series WHERE title = ?1 COLLATE NOCASE",
                params![title_clean],
                |r| r.get(0),
            )
            .optional()?;

        if let Some(id) = existing {
            return Ok(id);
        }

        let sort_title = title_clean.to_lowercase();
        let author_str = author.unwrap_or("Unknown").trim();
        let desc_str = description.unwrap_or("").trim();

        conn.execute(
            "INSERT INTO comic_series (title, sort_title, author, description, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'), datetime('now'))",
            params![title_clean, sort_title, author_str, desc_str],
        )?;

        Ok(conn.last_insert_rowid())
    }

    /// The on-disk folder name for a comic series, creating the folder if it
    /// does not exist yet (item 2.22: `library/<Series>/` holds the chapter
    /// files and `covers/`).
    ///
    /// The name is derived from the series title once and then stays stable —
    /// it is not re-derived on every start, because it is embedded in every
    /// chapter's stored `file_name`, and renaming would mean touching them
    /// all. A chapter that has already been placed tells us the name; only
    /// the first placement computes it.
    ///
    /// A clash — another folder already using the sanitized title, whether
    /// ours or a foreign one — is resolved with a `#<series id>` suffix. The
    /// suffix deliberately starts with `#`: the library folder scan resolves
    /// folders by their trailing id token, and a bare number there could be
    /// mistaken for one, while `#3` never is.
    pub fn ensure_series_folder(&self, series_id: i64) -> Result<String> {
        let name = self.series_folder_name_for(series_id)?;
        std::fs::create_dir_all(
            crate::paths::library_dir().join(&name).join("covers"),
        )
        .map_err(DbError::Io)?;
        Ok(name)
    }

    /// The series folder's name, without creating anything. The placement
    /// pass resolves first and creates only when a file is actually about
    /// to move, so a series whose files are all missing leaves no empty
    /// folder behind.
    pub fn series_folder_name_for(&self, series_id: i64) -> Result<String> {
        // Already placed? Any migrated chapter's file_name starts with the
        // folder ("Naruto …/0102.cbz").
        let placed: Option<String> = {
            let conn = self.conn();
            conn.query_row(
                "SELECT b.file_name FROM comic_chapters c
                 JOIN books b ON b.id = c.book_id
                 WHERE c.series_id = ?1 AND b.file_name LIKE '%/%'
                 ORDER BY c.chapter_number ASC LIMIT 1",
                params![series_id],
                |r| r.get::<_, String>(0),
            )
            .optional()?
            .and_then(|name| name.split('/').next().map(|s| s.to_string()))
        };
        if let Some(folder) = placed {
            return Ok(folder);
        }

        let title: String = {
            let conn = self.conn();
            conn.query_row(
                "SELECT title FROM comic_series WHERE id = ?1",
                params![series_id],
                |r| r.get(0),
            )
            .optional()?
            .unwrap_or_else(|| "Untitled Series".to_string())
        };

        let base = crate::paths::series_folder_name(&title);
        let lib = crate::paths::library_dir();
        let mut name = base.clone();
        if lib.join(&name).exists() {
            name = format!("{base} #{series_id}");
            let mut n = 1;
            while lib.join(&name).exists() && n < 100 {
                n += 1;
                name = format!("{base} #{series_id}-{n}");
            }
            if lib.join(&name).exists() {
                // Every name we are willing to print is taken. The bare id is
                // not a title a person would pick for a folder, so it is free.
                name = format!("series-{series_id}");
            }
        }
        Ok(name)
    }

    /// Add a chapter to a comic series, mapping it to a book row.
    pub fn add_comic_chapter(
        &self,
        series_id: i64,
        book_id: i64,
        chapter_number: f32,
        volume_number: Option<f32>,
        chapter_title: &str,
    ) -> Result<i64> {
        let conn = self.conn();
        conn.execute(
            "INSERT INTO comic_chapters (series_id, book_id, chapter_number, volume_number, chapter_title, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, datetime('now'))
             ON CONFLICT(book_id) DO UPDATE SET
                 series_id = excluded.series_id,
                 chapter_number = excluded.chapter_number,
                 volume_number = excluded.volume_number,
                 chapter_title = excluded.chapter_title",
            params![
                series_id,
                book_id,
                chapter_number as f64,
                volume_number.map(|v| v as f64),
                chapter_title.trim(),
            ],
        )?;
        let chapter_id = conn.last_insert_rowid();

        // Ensure series has a cover_book_id (prefer chapter 1.0 or first imported)
        conn.execute(
            "UPDATE comic_series
             SET cover_book_id = ?1, updated_at = datetime('now')
             WHERE id = ?2 AND (cover_book_id IS NULL OR ?3 <= 1.0)",
            params![book_id, series_id, chapter_number as f64],
        )?;

        Ok(chapter_id)
    }

    /// Get a single comic series by its ID, with total chapter counts and cover resolution.
    pub fn get_comic_series(&self, series_id: i64) -> Result<Option<ComicSeries>> {
        let conn = self.conn();
        let sql = "
            SELECT 
                s.id,
                s.title,
                s.sort_title,
                s.author,
                s.description,
                s.cover_book_id,
                COALESCE(cover_book.uuid, first_b.uuid) AS cover_uuid,
                COALESCE(cover_book.cover_name, first_b.cover_name) AS cover_name,
                s.status,
                COUNT(c.id) AS total_chapters,
                SUM(CASE WHEN b.progress >= 100 THEN 1 ELSE 0 END) AS completed_chapters,
                SUM(CASE WHEN b.progress = 0 THEN 1 ELSE 0 END) AS unread_chapters,
                s.created_at,
                s.updated_at,
                s.last_read_at
            FROM comic_series s
            LEFT JOIN books cover_book ON cover_book.id = s.cover_book_id
            LEFT JOIN comic_chapters c ON c.series_id = s.id
            LEFT JOIN books b ON b.id = c.book_id
            LEFT JOIN comic_chapters first_c ON first_c.series_id = s.id AND first_c.chapter_number = (
                SELECT MIN(sub_c.chapter_number) FROM comic_chapters sub_c WHERE sub_c.series_id = s.id
            )
            LEFT JOIN books first_b ON first_b.id = first_c.book_id
            WHERE s.id = ?1
            GROUP BY s.id
        ";

        let mut stmt = conn.prepare_cached(sql)?;
        let series = stmt
            .query_row(params![series_id], |r| {
                let cover_uuid: Option<String> = r.get(6)?;
                let cover_name: Option<String> = r.get(7)?;
                let cover_path =
                    cover_uuid.and_then(|u| cover_name.map(|n| resolve_library_file(&u, &n)));

                Ok(ComicSeries {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    sort_title: r.get(2)?,
                    author: r.get(3)?,
                    description: r.get(4)?,
                    cover_book_id: r.get(5)?,
                    cover_path,
                    status: r.get(8)?,
                    total_chapters: r.get::<_, i64>(9)? as usize,
                    completed_chapters: r.get::<_, i64>(10)? as usize,
                    unread_chapters: r.get::<_, i64>(11)? as usize,
                    created_at: r.get(12)?,
                    updated_at: r.get(13)?,
                    last_read_at: r.get(14)?,
                })
            })
            .optional()?;

        Ok(series)
    }

    /// Look up a comic series by title (case-insensitive).
    pub fn get_comic_series_by_title(&self, title: &str) -> Result<Option<ComicSeries>> {
        let series_id: Option<i64> = {
            let conn = self.conn();
            let mut stmt = conn.prepare_cached(
                "SELECT id FROM comic_series WHERE title = ?1 COLLATE NOCASE LIMIT 1",
            )?;
            stmt.query_row(params![title.trim()], |r| r.get(0)).optional()?
        };

        if let Some(id) = series_id {
            self.get_comic_series(id)
        } else {
            Ok(None)
        }
    }

    /// List all comic series matching an optional search query, ordered by title.
    pub fn list_comic_series(&self, query: &str) -> Result<Vec<ComicSeries>> {
        let conn = self.conn();
        let q = query.trim().to_lowercase();
        let has_filter = !q.is_empty();
        let like_pattern = format!("%{q}%");

        let sql = "
            SELECT 
                s.id,
                s.title,
                s.sort_title,
                s.author,
                s.description,
                s.cover_book_id,
                COALESCE(cover_book.uuid, first_b.uuid) AS cover_uuid,
                COALESCE(cover_book.cover_name, first_b.cover_name) AS cover_name,
                s.status,
                COUNT(c.id) AS total_chapters,
                SUM(CASE WHEN b.progress >= 100 THEN 1 ELSE 0 END) AS completed_chapters,
                SUM(CASE WHEN b.progress = 0 THEN 1 ELSE 0 END) AS unread_chapters,
                s.created_at,
                s.updated_at,
                s.last_read_at
            FROM comic_series s
            LEFT JOIN books cover_book ON cover_book.id = s.cover_book_id
            LEFT JOIN comic_chapters c ON c.series_id = s.id
            LEFT JOIN books b ON b.id = c.book_id
            LEFT JOIN comic_chapters first_c ON first_c.series_id = s.id AND first_c.chapter_number = (
                SELECT MIN(sub_c.chapter_number) FROM comic_chapters sub_c WHERE sub_c.series_id = s.id
            )
            LEFT JOIN books first_b ON first_b.id = first_c.book_id
            WHERE (?1 = 0 OR s.title LIKE ?2 OR s.author LIKE ?2)
            GROUP BY s.id
            ORDER BY s.sort_title COLLATE NOCASE ASC
        ";

        let mut stmt = conn.prepare_cached(sql)?;
        let rows = stmt.query_map(
            params![if has_filter { 1 } else { 0 }, like_pattern],
            |r| {
                let cover_uuid: Option<String> = r.get(6)?;
                let cover_name: Option<String> = r.get(7)?;
                let cover_path =
                    cover_uuid.and_then(|u| cover_name.map(|n| resolve_library_file(&u, &n)));

                Ok(ComicSeries {
                    id: r.get(0)?,
                    title: r.get(1)?,
                    sort_title: r.get(2)?,
                    author: r.get(3)?,
                    description: r.get(4)?,
                    cover_book_id: r.get(5)?,
                    cover_path,
                    status: r.get(8)?,
                    total_chapters: r.get::<_, i64>(9)? as usize,
                    completed_chapters: r.get::<_, i64>(10)? as usize,
                    unread_chapters: r.get::<_, i64>(11)? as usize,
                    created_at: r.get(12)?,
                    updated_at: r.get(13)?,
                    last_read_at: r.get(14)?,
                })
            },
        )?;

        let mut list = Vec::new();
        for r in rows {
            list.push(r?);
        }
        Ok(list)
    }

    /// List all chapters belonging to a comic series in ascending numerical order.
    pub fn chapters_for_series(&self, series_id: i64) -> Result<Vec<ComicChapter>> {
        let (raw_chapters, book_ids) = {
            let conn = self.conn();
            let sql = "
                SELECT id, series_id, book_id, chapter_number, volume_number, chapter_title, created_at
                FROM comic_chapters
                WHERE series_id = ?1
                ORDER BY chapter_number ASC
            ";
            let mut stmt = conn.prepare_cached(sql)?;
            let rows = stmt.query_map(params![series_id], |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    r.get::<_, i64>(1)?,
                    r.get::<_, i64>(2)?,
                    r.get::<_, f64>(3)? as f32,
                    r.get::<_, Option<f64>>(4)?.map(|v| v as f32),
                    r.get::<_, String>(5)?,
                    r.get::<_, String>(6)?,
                ))
            })?;

            let mut raw = Vec::new();
            let mut ids = Vec::new();
            for r in rows {
                let row_val = r?;
                ids.push(row_val.2);
                raw.push(row_val);
            }
            (raw, ids)
        };

        let mut books_map = self.books_by_ids(&book_ids)?;

        let mut chapters = Vec::new();
        for (id, s_id, book_id, ch_num, vol_num, title, created_at) in raw_chapters {
            if let Some(book) = books_map.remove(&book_id) {
                chapters.push(ComicChapter {
                    id,
                    series_id: s_id,
                    book_id,
                    chapter_number: ch_num,
                    volume_number: vol_num,
                    chapter_title: title,
                    created_at,
                    book,
                });
            }
        }

        Ok(chapters)
    }

    /// Find the comic series and chapter entry for a given book_id.
    pub fn get_comic_series_for_book(
        &self,
        book_id: i64,
    ) -> Result<Option<(ComicSeries, ComicChapter)>> {
        let row: Option<RawChapterRow> = {
            let conn = self.conn();
            conn.query_row(
                "SELECT id, series_id, book_id, chapter_number, volume_number, chapter_title, created_at
                 FROM comic_chapters
                 WHERE book_id = ?1",
                params![book_id],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get::<_, f64>(3)? as f32,
                        r.get::<_, Option<f64>>(4)?.map(|v| v as f32),
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .optional()?
        };

        let Some((c_id, series_id, b_id, ch_num, vol_num, ch_title, created_at)) = row else {
            return Ok(None);
        };

        let Some(series) = self.get_comic_series(series_id)? else {
            return Ok(None);
        };

        let mut books = self.books_by_ids(&[b_id])?;
        let Some(book) = books.remove(&b_id) else {
            return Ok(None);
        };

        let chapter = ComicChapter {
            id: c_id,
            series_id,
            book_id: b_id,
            chapter_number: ch_num,
            volume_number: vol_num,
            chapter_title: ch_title,
            created_at,
            book,
        };

        Ok(Some((series, chapter)))
    }

    /// Retrieve the next chapter in the series by chapter number.
    #[allow(dead_code)]
    pub fn next_comic_chapter(
        &self,
        series_id: i64,
        current_chapter_number: f32,
    ) -> Result<Option<ComicChapter>> {
        let row: Option<RawChapterRow> = {
            let conn = self.conn();
            conn.query_row(
                "SELECT id, series_id, book_id, chapter_number, volume_number, chapter_title, created_at
                 FROM comic_chapters
                 WHERE series_id = ?1 AND chapter_number > ?2
                 ORDER BY chapter_number ASC
                 LIMIT 1",
                params![series_id, current_chapter_number as f64],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get::<_, f64>(3)? as f32,
                        r.get::<_, Option<f64>>(4)?.map(|v| v as f32),
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .optional()?
        };

        let Some((id, s_id, book_id, ch_num, vol_num, title, created_at)) = row else {
            return Ok(None);
        };

        let mut books = self.books_by_ids(&[book_id])?;
        let Some(book) = books.remove(&book_id) else {
            return Ok(None);
        };

        Ok(Some(ComicChapter {
            id,
            series_id: s_id,
            book_id,
            chapter_number: ch_num,
            volume_number: vol_num,
            chapter_title: title,
            created_at,
            book,
        }))
    }

    /// Retrieve the previous chapter in the series by chapter number.
    #[allow(dead_code)]
    pub fn prev_comic_chapter(
        &self,
        series_id: i64,
        current_chapter_number: f32,
    ) -> Result<Option<ComicChapter>> {
        let row: Option<RawChapterRow> = {
            let conn = self.conn();
            conn.query_row(
                "SELECT id, series_id, book_id, chapter_number, volume_number, chapter_title, created_at
                 FROM comic_chapters
                 WHERE series_id = ?1 AND chapter_number < ?2
                 ORDER BY chapter_number DESC
                 LIMIT 1",
                params![series_id, current_chapter_number as f64],
                |r| {
                    Ok((
                        r.get(0)?,
                        r.get(1)?,
                        r.get(2)?,
                        r.get::<_, f64>(3)? as f32,
                        r.get::<_, Option<f64>>(4)?.map(|v| v as f32),
                        r.get(5)?,
                        r.get(6)?,
                    ))
                },
            )
            .optional()?
        };

        let Some((id, s_id, book_id, ch_num, vol_num, title, created_at)) = row else {
            return Ok(None);
        };

        let mut books = self.books_by_ids(&[book_id])?;
        let Some(book) = books.remove(&book_id) else {
            return Ok(None);
        };

        Ok(Some(ComicChapter {
            id,
            series_id: s_id,
            book_id,
            chapter_number: ch_num,
            volume_number: vol_num,
            chapter_title: title,
            created_at,
            book,
        }))
    }

    /// One-time migration: migrate legacy CBZ/CBR books into `comic_series` and `comic_chapters`.
    pub fn migrate_comic_series_and_chapters(&self) -> Result<usize> {
        let unmigrated_ids: Vec<i64> = {
            let conn = self.conn();
            let mut stmt = conn.prepare(
                "SELECT id FROM books 
                 WHERE (format = 'CBZ' OR format = 'CBR')
                   AND id NOT IN (SELECT book_id FROM comic_chapters)
                 ORDER BY id ASC",
            )?;
            let rows = stmt.query_map([], |r| r.get::<_, i64>(0))?;
            let mut ids = Vec::new();
            for r in rows {
                ids.push(r?);
            }
            ids
        };

        if unmigrated_ids.is_empty() {
            return Ok(0);
        }

        let books_map = self.books_by_ids(&unmigrated_ids)?;
        let mut migrated = 0;

        for id in unmigrated_ids {
            let Some(book) = books_map.get(&id) else {
                continue;
            };

            // Determine canonical series title and chapter number
            let raw_series = book.series.as_deref().unwrap_or("").trim();
            let (series_title, detected_num) = if !raw_series.is_empty() {
                crate::comics::sanitize_comic_series(raw_series)
            } else {
                let meta = crate::comics::parse_comic_title(&book.title);
                (meta.series.unwrap_or_else(|| book.title.clone()), meta.number)
            };

            let series_title = if series_title.trim().is_empty() {
                book.title.clone()
            } else {
                series_title
            };

            let ch_num = if book.series_index > 0.0 {
                book.series_index
            } else {
                detected_num.unwrap_or(1.0)
            };

            let series_id = self.get_or_create_comic_series(
                &series_title,
                Some(&book.authors),
                Some(&book.description),
            )?;
            let _ = self.add_comic_chapter(series_id, book.id, ch_num, None, &book.title)?;
            migrated += 1;
        }

        Ok(migrated)
    }

    /// Collapse multiple chapters of the same comic series into a single representative item.
    ///
    /// For any comic books in `books` that belong to a `comic_series`, only the representative
    /// book (cover book or first chapter) is kept, with its title updated to the series title
    /// and author updated to the series author. Non-comic books are left untouched.
    pub fn collapse_comic_chapters(&self, books: &mut Vec<Book>) {
        if books.is_empty() {
            return;
        }

        let comic_ids: Vec<i64> = books
            .iter()
            .filter(|b| matches!(b.format, BookFormat::Cbz | BookFormat::Cbr))
            .map(|b| b.id)
            .collect();

        if comic_ids.is_empty() {
            return;
        }

        let conn = self.conn();
        let holders = vec!["?"; comic_ids.len()].join(",");
        let sql = format!(
            "SELECT c.book_id, s.id, s.title, s.author, s.cover_book_id
             FROM comic_chapters c
             JOIN comic_series s ON s.id = c.series_id
             WHERE c.book_id IN ({holders})"
        );

        let mut book_to_series: std::collections::HashMap<i64, (i64, String, String, Option<i64>)> =
            std::collections::HashMap::new();

        if let Ok(mut stmt) = conn.prepare(&sql) {
            if let Ok(rows) = stmt.query_map(rusqlite::params_from_iter(comic_ids.iter()), |r| {
                Ok((
                    r.get::<_, i64>(0)?,
                    (
                        r.get::<_, i64>(1)?,
                        r.get::<_, String>(2)?,
                        r.get::<_, String>(3)?,
                        r.get::<_, Option<i64>>(4)?,
                    ),
                ))
            }) {
                for row in rows.flatten() {
                    book_to_series.insert(row.0, row.1);
                }
            }
        }

        let mut seen_series: std::collections::HashSet<i64> = std::collections::HashSet::new();
        let mut seen_heuristic_series: std::collections::HashSet<String> =
            std::collections::HashSet::new();

        let mut i = 0;
        while i < books.len() {
            let b = &mut books[i];
            if !matches!(b.format, BookFormat::Cbz | BookFormat::Cbr) {
                i += 1;
                continue;
            }

            if let Some((s_id, s_title, s_author, _cover_id)) = book_to_series.get(&b.id) {
                if seen_series.contains(s_id) {
                    books.remove(i);
                    continue;
                }
                seen_series.insert(*s_id);
                b.title = s_title.clone();
                if !s_author.is_empty() {
                    b.authors = s_author.clone();
                }
                b.series = Some(s_title.clone());
                i += 1;
            } else {
                // Heuristic fallback for comics not yet in relational comic_chapters
                let key = heuristic_series_key(&b.title, b.series.as_deref());
                if seen_heuristic_series.contains(&key) {
                    books.remove(i);
                    continue;
                }
                seen_heuristic_series.insert(key);
                let series_name = b
                    .series
                    .clone()
                    .or_else(|| crate::comics::parse_comic_title(&b.title).series)
                    .unwrap_or_else(|| b.title.clone());
                b.title = series_name.clone();
                b.series = Some(series_name);
                i += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::BookFormat;

    fn seed_series(cat: &Catalog, title: &str, series: Option<&str>, index: f32) -> i64 {
        let uuid = format!("uuid-{}", title.to_lowercase());
        let id = cat
            .insert_book(
                &uuid,
                title,
                "Some Author",
                series,
                "",
                BookFormat::Epub,
                "book.epub",
                &format!("hash-{}", title.to_lowercase()),
                None,
                &[],
            )
            .expect("insert");
        // insert_book does not take a series index; set it directly so the
        // ordering test exercises real series_index values.
        cat.conn()
            .execute(
                "UPDATE books SET series_index = ?1 WHERE id = ?2",
                params![index, id],
            )
            .expect("set series index");
        id
    }

    #[test]
    fn series_cache_round_trips() {
        let cat = Catalog::open_in_memory().unwrap();
        assert!(cat.get_cached_series("k|a").unwrap().is_none());

        let works = vec![
            SeriesWork {
                title: "The Name of the Wind".into(),
                key: "/works/OL1".into(),
                cover_path: "/tmp/ol-1.jpg".into(),
                year: 2007,
                author: "Patrick Rothfuss".into(),
            },
            SeriesWork {
                title: "The Wise Man's Fear".into(),
                key: "/works/OL2".into(),
                cover_path: String::new(),
                year: 2011,
                author: "Patrick Rothfuss".into(),
            },
        ];
        cat.upsert_series_cache("kingkiller|rothfuss", "openlibrary", &works)
            .unwrap();

        let entry = cat
            .get_cached_series("kingkiller|rothfuss")
            .unwrap()
            .unwrap();
        assert_eq!(entry.source, "openlibrary");
        assert_eq!(entry.works.len(), 2);
        assert_eq!(entry.works[1].title, "The Wise Man's Fear");

        // Overwrite: a refresh replaces the listing wholesale.
        let fewer = vec![works[0].clone()];
        cat.upsert_series_cache("kingkiller|rothfuss", "openlibrary", &fewer)
            .unwrap();
        let entry = cat
            .get_cached_series("kingkiller|rothfuss")
            .unwrap()
            .unwrap();
        assert_eq!(entry.works.len(), 1);
    }

    #[test]
    fn series_key_normalises_spelling() {
        assert_eq!(
            series_key("The  Kingkiller Chronicle", "Patrick Rothfuss"),
            series_key("kingkiller-chronicle", "PATRICK  ROTHFUSS"),
        );
        assert_ne!(
            series_key("Narnia", "Lewis"),
            series_key("Narnia", "Tolkien"),
        );
    }

    #[test]
    fn series_key_strips_articles_from_series_not_authors() {
        // Leading articles in the series name collapse to one key...
        assert_eq!(
            series_key("The Wheel of Time", "Jordan"),
            series_key("wheel of time", "Jordan"),
        );
        assert_eq!(
            series_key("A Game of Thrones", "Martin"),
            series_key("game of thrones", "Martin"),
        );
        // ...but author initials must not be eaten ("A. A. Milne" != "Milne").
        assert_ne!(
            series_key("Narnia", "A. A. Milne"),
            series_key("Narnia", "Milne"),
        );
        // A lone article is not a series name to strip.
        assert_eq!(series_key("The", "Author"), "the|author");
    }

    #[test]
    fn books_in_series_orders_by_index_then_title() {
        let cat = Catalog::open_in_memory().unwrap();
        // Unindexed books must land after indexed ones, alphabetically.
        seed_series(&cat, "Untitled Entry", Some("Chronicle"), 0.0);
        seed_series(&cat, "Second", Some("Chronicle"), 2.0);
        seed_series(&cat, "First", Some("Chronicle"), 1.0);
        seed_series(&cat, "Other Series", Some("Nope"), 1.0);

        let books = cat.books_in_series("Chronicle").unwrap();
        let titles: Vec<&str> = books.iter().map(|b| b.title.as_str()).collect();
        assert_eq!(titles, vec!["First", "Second", "Untitled Entry"]);
    }

    #[test]
    fn detect_local_series_returns_books_in_reading_order() {
        let cat = Catalog::open_in_memory().unwrap();
        seed_series(&cat, "Book Three", Some("Foundation"), 3.0);
        seed_series(&cat, "Book One", Some("Foundation"), 1.0);
        seed_series(&cat, "Book Two", Some("Foundation"), 2.0);

        let books = cat.detect_local_series("Foundation").unwrap();
        let titles: Vec<&str> = books.iter().map(|b| b.title.as_str()).collect();
        assert_eq!(titles, vec!["Book One", "Book Two", "Book Three"]);
    }

    #[test]
    fn test_comic_series_and_chapters_relational_hierarchy() {
        let cat = Catalog::open_in_memory().unwrap();
        let b1 = cat
            .insert_book(
                "u1",
                "Naruto - Ch. 1",
                "Masashi Kishimoto",
                None,
                "",
                BookFormat::Cbz,
                "b1.cbz",
                "h1",
                None,
                &[],
            )
            .unwrap();
        let b2 = cat
            .insert_book(
                "u2",
                "Naruto - Ch. 2",
                "Masashi Kishimoto",
                None,
                "",
                BookFormat::Cbz,
                "b2.cbz",
                "h2",
                None,
                &[],
            )
            .unwrap();

        let s_id = cat
            .get_or_create_comic_series(
                "Naruto",
                Some("Masashi Kishimoto"),
                Some("Ninja story"),
            )
            .unwrap();
        cat.add_comic_chapter(s_id, b1, 1.0, None, "Chapter 1")
            .unwrap();
        cat.add_comic_chapter(s_id, b2, 2.0, None, "Chapter 2")
            .unwrap();

        let series_list = cat.list_comic_series("").unwrap();
        assert_eq!(series_list.len(), 1);
        assert_eq!(series_list[0].title, "Naruto");
        assert_eq!(series_list[0].total_chapters, 2);

        let chapters = cat.chapters_for_series(s_id).unwrap();
        assert_eq!(chapters.len(), 2);
        assert_eq!(chapters[0].chapter_number, 1.0);
        assert_eq!(chapters[1].chapter_number, 2.0);

        let next = cat.next_comic_chapter(s_id, 1.0).unwrap().unwrap();
        assert_eq!(next.chapter_number, 2.0);

        let prev = cat.prev_comic_chapter(s_id, 2.0).unwrap().unwrap();
        assert_eq!(prev.chapter_number, 1.0);

        let (found_series, found_ch) = cat.get_comic_series_for_book(b2).unwrap().unwrap();
        assert_eq!(found_series.title, "Naruto");
        assert_eq!(found_ch.chapter_number, 2.0);
    }

    #[test]
    fn test_comic_series_recent_books_recently_opened_and_stats() {
        let cat = Catalog::open_in_memory().unwrap();

        // Seed 2 regular books
        let reg1 = cat
            .insert_book(
                "reg-1",
                "Dune",
                "Frank Herbert",
                None,
                "",
                BookFormat::Epub,
                "dune.epub",
                "h-dune",
                None,
                &[],
            )
            .unwrap();
        let _reg2 = cat
            .insert_book(
                "reg-2",
                "Foundation",
                "Isaac Asimov",
                None,
                "",
                BookFormat::Epub,
                "foundation.epub",
                "h-found",
                None,
                &[],
            )
            .unwrap();

        // Seed 3 chapters of a comic series
        let c1 = cat
            .insert_book(
                "c-1",
                "Horimiya - c001",
                "HERO",
                None,
                "",
                BookFormat::Cbz,
                "horimiya_01.cbz",
                "h-c1",
                None,
                &[],
            )
            .unwrap();
        let c2 = cat
            .insert_book(
                "c-2",
                "Horimiya - c002",
                "HERO",
                None,
                "",
                BookFormat::Cbz,
                "horimiya_02.cbz",
                "h-c2",
                None,
                &[],
            )
            .unwrap();
        let c3 = cat
            .insert_book(
                "c-3",
                "Horimiya - c003",
                "HERO",
                None,
                "",
                BookFormat::Cbz,
                "horimiya_03.cbz",
                "h-c3",
                None,
                &[],
            )
            .unwrap();

        let s_id = cat
            .get_or_create_comic_series("Horimiya", Some("HERO, Daisuke Hagiwara"), None)
            .unwrap();
        cat.add_comic_chapter(s_id, c1, 1.0, None, "Page 1").unwrap();
        cat.add_comic_chapter(s_id, c2, 2.0, None, "Page 2").unwrap();
        cat.add_comic_chapter(s_id, c3, 3.0, None, "Page 3").unwrap();

        // 1. get_comic_series_by_title lookup
        let found = cat.get_comic_series_by_title("horimiya").unwrap().unwrap();
        assert_eq!(found.id, s_id);
        assert_eq!(found.title, "Horimiya");
        assert_eq!(found.total_chapters, 3);

        // 2. recent_books must collapse chapters: 2 regular books + 1 comic series = 3 items total
        let recent = cat.recent_books(10).unwrap();
        assert_eq!(recent.len(), 3);
        let series_item = recent.iter().find(|b| b.title == "Horimiya");
        assert!(series_item.is_some(), "Comic series should be present as 'Horimiya'");
        assert_eq!(series_item.unwrap().authors, "HERO, Daisuke Hagiwara");
        assert_eq!(series_item.unwrap().series.as_deref(), Some("Horimiya"));

        // 3. library_stats must treat the series as 1 item, not 3 discrete books
        let stats = cat.library_stats().unwrap();
        assert_eq!(stats.total_books, 3); // 2 regular + 1 comic series

        // 4. recently_opened must deduplicate chapters of the same series
        cat.conn()
            .execute("UPDATE books SET last_opened_at = '2026-01-01T10:00:00Z' WHERE id = ?1", params![c1])
            .unwrap();
        cat.conn()
            .execute("UPDATE books SET last_opened_at = '2026-01-01T10:05:00Z' WHERE id = ?1", params![c2])
            .unwrap();
        cat.conn()
            .execute("UPDATE books SET last_opened_at = '2026-01-01T10:10:00Z' WHERE id = ?1", params![reg1])
            .unwrap();

        let opened = cat.recently_opened(5).unwrap();
        assert_eq!(opened.len(), 2); // 1 regular book (reg1) + 1 comic series (Horimiya)
        let comic_opened = opened.iter().find(|b| b.series.as_deref() == Some("Horimiya"));
        assert!(comic_opened.is_some());
        // Most recently opened chapter of Horimiya was c2
        assert_eq!(comic_opened.unwrap().id, c2);

        // 5. collapse_comic_chapters must collapse all chapters into 1 item
        let mut all_books = cat.list_books(crate::db::SortKey::Added, "").unwrap();
        assert_eq!(all_books.len(), 5); // 2 regular + 3 comic chapters
        cat.collapse_comic_chapters(&mut all_books);
        assert_eq!(all_books.len(), 3); // 2 regular + 1 comic series
        let series_b = all_books.iter().find(|b| b.title == "Horimiya");
        assert!(series_b.is_some());
        assert_eq!(series_b.unwrap().authors, "HERO, Daisuke Hagiwara");
    }

    #[test]
    fn shelves_and_reading_list_show_comics_as_series() {
        // The owner's rule, stated as a test: no list surface ever shows a
        // comic's individual chapters. Counts, removal and peers must all
        // agree with the collapsed cards.
        let cat = Catalog::open_in_memory().unwrap();

        let reg = cat
            .insert_book(
                "reg-1",
                "Dune",
                "Frank Herbert",
                None,
                "",
                BookFormat::Epub,
                "dune.epub",
                "h-dune",
                None,
                &[],
            )
            .unwrap();
        let c1 = cat
            .insert_book(
                "c-1",
                "Horimiya - c001",
                "HERO",
                None,
                "",
                BookFormat::Cbz,
                "horimiya_01.cbz",
                "h-c1",
                None,
                &[],
            )
            .unwrap();
        let c2 = cat
            .insert_book(
                "c-2",
                "Horimiya - c002",
                "HERO",
                None,
                "",
                BookFormat::Cbz,
                "horimiya_02.cbz",
                "h-c2",
                None,
                &[],
            )
            .unwrap();
        let s_id = cat
            .get_or_create_comic_series("Horimiya", Some("HERO"), None)
            .unwrap();
        cat.add_comic_chapter(s_id, c1, 1.0, None, "Page 1").unwrap();
        cat.add_comic_chapter(s_id, c2, 2.0, None, "Page 2").unwrap();

        // A manual shelf holding the epub and both chapters.
        let shelf_id = cat
            .create_shelf("Mixed", ShelfKind::Manual, "", "")
            .unwrap();
        cat.add_book_to_shelf(shelf_id, reg).unwrap();
        cat.add_book_to_shelf(shelf_id, c1).unwrap();
        cat.add_book_to_shelf(shelf_id, c2).unwrap();

        // The badge counts cards, not membership rows: 1 epub + 1 series = 2.
        let shelves = cat.list_shelves().unwrap();
        assert_eq!(shelves[0].book_count, 2, "collapsed badge count");

        // The shelf's book list collapses to one series card.
        let shelf = cat.get_shelf(shelf_id).unwrap().unwrap();
        let mut books = cat.shelf_books(&shelf, crate::db::SortKey::Added, "").unwrap();
        cat.collapse_comic_chapters(&mut books);
        assert_eq!(books.len(), 2);
        let comic = books
            .iter()
            .find(|b| b.series.as_deref() == Some("Horimiya"))
            .expect("series card present");
        assert_eq!(comic.title, "Horimiya");

        // Peers: the representative resolves to every chapter of its series.
        let peers = cat.comic_series_peers(comic.id).unwrap();
        assert!(peers.contains(&c1) && peers.contains(&c2) && peers.len() == 2);
        // A non-comic is its own peer.
        assert_eq!(cat.comic_series_peers(reg).unwrap(), vec![reg]);

        // Series-scoped remove clears the whole series from the shelf.
        cat.remove_series_from_shelf(shelf_id, comic.id).unwrap();
        let shelves = cat.list_shelves().unwrap();
        assert_eq!(shelves[0].book_count, 1, "only the epub remains");

        // Heuristic peers: unregistered comics keyed by their series column.
        let h1 = cat
            .insert_book(
                "h-1",
                "Naruto - c001",
                "Kishimoto",
                Some("Naruto"),
                "",
                BookFormat::Cbz,
                "naruto_01.cbz",
                "h-n1",
                None,
                &[],
            )
            .unwrap();
        let h2 = cat
            .insert_book(
                "h-2",
                "Naruto - c002",
                "Kishimoto",
                Some("Naruto"),
                "",
                BookFormat::Cbz,
                "naruto_02.cbz",
                "h-n2",
                None,
                &[],
            )
            .unwrap();
        let peers = cat.comic_series_peers(h1).unwrap();
        assert!(peers.contains(&h1) && peers.contains(&h2) && peers.len() == 2);

        // The reading list: both chapters queued, the series remove clears
        // them together and leaves the epub.
        cat.add_to_reading_list(reg).unwrap();
        cat.add_to_reading_list(c1).unwrap();
        cat.add_to_reading_list(c2).unwrap();
        cat.remove_series_from_reading_list(c1).unwrap();
        let entries = cat.list_reading_list().unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].book.id, reg);
    }

    #[test]
    fn pickers_add_and_remove_a_series_as_one() {
        // The "+ Add books" pickers show one row per series; ticking it must
        // put every chapter on the shelf / reading list, unticking must take
        // them all off — and a regular book still behaves as a single row.
        let cat = Catalog::open_in_memory().unwrap();

        let reg = cat
            .insert_book(
                "reg-1",
                "Dune",
                "Frank Herbert",
                None,
                "",
                BookFormat::Epub,
                "dune.epub",
                "h-dune",
                None,
                &[],
            )
            .unwrap();
        let c1 = cat
            .insert_book(
                "c-1",
                "Horimiya - c001",
                "HERO",
                None,
                "",
                BookFormat::Cbz,
                "horimiya_01.cbz",
                "h-c1",
                None,
                &[],
            )
            .unwrap();
        let c2 = cat
            .insert_book(
                "c-2",
                "Horimiya - c002",
                "HERO",
                None,
                "",
                BookFormat::Cbz,
                "horimiya_02.cbz",
                "h-c2",
                None,
                &[],
            )
            .unwrap();
        let s_id = cat
            .get_or_create_comic_series("Horimiya", Some("HERO"), None)
            .unwrap();
        cat.add_comic_chapter(s_id, c1, 1.0, None, "Page 1").unwrap();
        cat.add_comic_chapter(s_id, c2, 2.0, None, "Page 2").unwrap();

        // Shelf: tick the series row → both chapters are members, the badge
        // still counts one card, positions run on from the shelf's order.
        let shelf_id = cat
            .create_shelf("Mixed", ShelfKind::Manual, "", "")
            .unwrap();
        cat.add_series_to_shelf(shelf_id, reg).unwrap();
        cat.add_series_to_shelf(shelf_id, c1).unwrap();
        let shelf = cat.get_shelf(shelf_id).unwrap().unwrap();
        let members = cat.shelf_books(&shelf, crate::db::SortKey::Title, "").unwrap();
        assert_eq!(members.len(), 3, "epub + both chapters");
        assert_eq!(cat.list_shelves().unwrap()[0].book_count, 2, "two cards");

        // Untick the series row → every chapter leaves, the epub stays.
        cat.remove_series_from_shelf(shelf_id, c1).unwrap();
        let shelf = cat.get_shelf(shelf_id).unwrap().unwrap();
        let members = cat.shelf_books(&shelf, crate::db::SortKey::Title, "").unwrap();
        assert_eq!(members.len(), 1);
        assert_eq!(members[0].id, reg);

        // Reading list: any_in_reading_list sees a series through any one of
        // its chapters; the series add queues them all in order.
        assert!(!cat.any_in_reading_list(&[c1, c2]).unwrap());
        cat.add_series_to_reading_list(c1).unwrap();
        assert!(cat.any_in_reading_list(&[c1, c2]).unwrap());
        let entries = cat.list_reading_list().unwrap();
        assert_eq!(entries.len(), 2, "both chapters queued");
        let ids: Vec<i64> = entries.iter().map(|e| e.book.id).collect();
        assert!(ids.contains(&c1) && ids.contains(&c2));
        // And the series remove clears them both (asserted in the test
        // above; here just the count).
        cat.remove_series_from_reading_list(c2).unwrap();
        assert!(cat.list_reading_list().unwrap().is_empty());
    }

    #[test]
    fn comic_series_peers_by_ids_matches_the_single_book_form() {
        // The batched form the pickers use must answer exactly what
        // `comic_series_peers` answers, for every kind of book at once:
        // registered chapters, unregistered heuristic pairs, and non-comics
        // (their own peer). Unknown ids stay out of the map — callers fall
        // back to `vec![id]`.
        let cat = Catalog::open_in_memory().unwrap();

        let reg = cat
            .insert_book(
                "reg-1",
                "Dune",
                "Frank Herbert",
                None,
                "",
                BookFormat::Epub,
                "dune.epub",
                "h-dune",
                None,
                &[],
            )
            .unwrap();
        let c1 = cat
            .insert_book(
                "c-1",
                "Horimiya - c001",
                "HERO",
                None,
                "",
                BookFormat::Cbz,
                "horimiya_01.cbz",
                "h-c1",
                None,
                &[],
            )
            .unwrap();
        let c2 = cat
            .insert_book(
                "c-2",
                "Horimiya - c002",
                "HERO",
                None,
                "",
                BookFormat::Cbz,
                "horimiya_02.cbz",
                "h-c2",
                None,
                &[],
            )
            .unwrap();
        let s_id = cat
            .get_or_create_comic_series("Horimiya", Some("HERO"), None)
            .unwrap();
        cat.add_comic_chapter(s_id, c1, 1.0, None, "Page 1").unwrap();
        cat.add_comic_chapter(s_id, c2, 2.0, None, "Page 2").unwrap();
        let h1 = cat
            .insert_book(
                "h-1",
                "Naruto - c001",
                "Kishimoto",
                Some("Naruto"),
                "",
                BookFormat::Cbz,
                "naruto_01.cbz",
                "h-n1",
                None,
                &[],
            )
            .unwrap();
        let h2 = cat
            .insert_book(
                "h-2",
                "Naruto - c002",
                "Kishimoto",
                Some("Naruto"),
                "",
                BookFormat::Cbz,
                "naruto_02.cbz",
                "h-n2",
                None,
                &[],
            )
            .unwrap();

        let mut asked = |map: &std::collections::HashMap<i64, Vec<i64>>, id: i64| {
            let mut got = map.get(&id).cloned().unwrap_or_default();
            got.sort_unstable();
            got
        };

        let map = cat
            .comic_series_peers_by_ids(&[c2, c1, reg, h1, h2, 999_999])
            .unwrap();
        // Registered: every chapter sees every chapter of its series.
        assert_eq!(asked(&map, c1), vec![c1, c2]);
        assert_eq!(asked(&map, c2), vec![c1, c2]);
        // A non-comic is its own peer.
        assert_eq!(asked(&map, reg), vec![reg]);
        // Unregistered heuristic pair, both directions.
        assert_eq!(asked(&map, h1), vec![h1, h2]);
        assert_eq!(asked(&map, h2), vec![h1, h2]);
        // An unknown id is simply absent — the caller's fallback owns it.
        assert!(!map.contains_key(&999_999));

        // Each answer is the single-book form's answer, verbatim.
        for id in [c1, c2, reg, h1, h2] {
            let mut single = cat.comic_series_peers(id).unwrap();
            single.sort_unstable();
            assert_eq!(asked(&map, id), single, "id {id}");
        }

        // Empty input is not an error and issues nothing — the shared
        // contract of the batched readers (`books_by_ids` and friends).
        let empty: Vec<i64> = Vec::new();
        let stmts = cat.count_queries(|| cat.comic_series_peers_by_ids(&empty).unwrap());
        assert_eq!(stmts, 0, "empty batch must not query");
        assert!(cat.comic_series_peers_by_ids(&empty).unwrap().is_empty());
    }
}
