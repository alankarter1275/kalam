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
    #[allow(dead_code)]
    pub series_key: String,
    #[allow(dead_code)]
    pub source: String,
    pub fetched_at: String,
    pub works: Vec<SeriesWork>,
}

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
}
