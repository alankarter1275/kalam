//! Open Library series lookup — the data source for the series page.
//!
//! Best-effort by design: OL's series field is derived from edition data, so
//! a listing can include a false member or miss a work. The page shows the
//! source and fetch time, and its refresh action is the escape hatch.
//!
//! Ordering is by first publish year, which is reading order for the great
//! majority of series. Works without a year sort last, alphabetically.

use super::{agent, CoverRef, FetchError};
use serde::Deserialize;
use std::path::PathBuf;

const SEARCH_URL: &str = "https://openlibrary.org/search.json";
const FIELDS: &str = "key,title,author_name,first_publish_year,cover_i";
const MAX_WORKS: usize = 30;

/// One work as OL knows it, before covers are downloaded.
#[derive(Debug, Clone)]
pub struct RemoteWork {
    pub title: String,
    /// OL work key (`/works/OL…`); empty when missing.
    pub key: String,
    /// First author, empty when unknown.
    pub author: String,
    /// First publish year, 0 when unknown.
    pub year: i64,
    /// Numeric cover id for `covers.openlibrary.org`, if any.
    pub cover_i: Option<i64>,
}

#[derive(Deserialize)]
struct SearchResponse {
    #[serde(default)]
    docs: Vec<SearchDoc>,
}

#[derive(Deserialize)]
struct SearchDoc {
    key: Option<String>,
    title: Option<String>,
    author_name: Option<Vec<String>>,
    first_publish_year: Option<i64>,
    cover_i: Option<i64>,
}

/// Search OL for the works in a series.
///
/// Tries the `series` field filter first, then a quoted free-text query as a
/// fallback, and returns deduplicated works ordered by first publish year.
/// An empty `Vec` means "OL knows nothing about this series" — distinct from
/// a network error, which is `Err`.
pub fn search_series(name: &str) -> Result<Vec<RemoteWork>, FetchError> {
    let name = name.trim();
    if name.is_empty() {
        return Ok(Vec::new());
    }

    let mut docs = query("series", name)?;
    if docs.is_empty() {
        let quoted = format!("series:\"{name}\"");
        docs = query("q", &quoted)?;
    }

    let mut works: Vec<RemoteWork> = docs.into_iter().filter_map(doc_to_work).collect();

    // Deduplicate by normalised title, preferring the doc that has a cover.
    works.sort_by(|a, b| {
        b.cover_i
            .is_some()
            .cmp(&a.cover_i.is_some())
            .then_with(|| a.title.cmp(&b.title))
    });
    let mut seen = std::collections::HashSet::new();
    works.retain(|w| {
        let key = normalise(&w.title);
        key.is_empty() || seen.insert(key)
    });

    // Publication order, then alphabetics; unknown years last.
    let year_key = |w: &RemoteWork| if w.year > 0 { w.year } else { i64::MAX };
    works.sort_by(|a, b| {
        year_key(a)
            .cmp(&year_key(b))
            .then_with(|| a.title.cmp(&b.title))
            .then_with(|| b.cover_i.is_some().cmp(&a.cover_i.is_some()))
    });
    works.truncate(MAX_WORKS);

    Ok(works)
}

fn query(param: &str, value: &str) -> Result<Vec<SearchDoc>, FetchError> {
    let body = agent()
        .get(SEARCH_URL)
        .query(param, value)
        .query("limit", "50")
        // Ask only for the fields we use — the default payload is enormous.
        .query("fields", FIELDS)
        .call()
        .map_err(|e| FetchError::Network(e.to_string()))?
        .into_string()
        .map_err(|e| FetchError::Network(e.to_string()))?;

    let parsed: SearchResponse =
        serde_json::from_str(&body).map_err(|e| FetchError::Parse(e.to_string()))?;
    Ok(parsed.docs)
}

fn doc_to_work(doc: SearchDoc) -> Option<RemoteWork> {
    let title = doc.title?.trim().to_string();
    if title.is_empty() {
        return None;
    }
    Some(RemoteWork {
        title,
        key: doc.key.unwrap_or_default(),
        author: doc
            .author_name
            .and_then(|a| a.into_iter().next())
            .unwrap_or_default(),
        year: doc.first_publish_year.unwrap_or(0),
        cover_i: doc.cover_i,
    })
}

/// Download a cover by OL id into the series-covers dir.
///
/// Idempotent: the file is named by cover id, so repeated fetches (and
/// refreshes) never re-download what is already on disk.
pub fn download_cover(cover_i: i64) -> Result<PathBuf, FetchError> {
    let dir = crate::paths::series_covers_dir();
    std::fs::create_dir_all(&dir).map_err(|e| FetchError::Network(e.to_string()))?;
    let dest = dir.join(format!("ol-{cover_i}.jpg"));
    if dest.is_file() {
        return Ok(dest);
    }
    let bytes = super::fetch_thumbnail(&CoverRef::OpenLibraryId(cover_i))?;
    std::fs::write(&dest, &bytes).map_err(|e| FetchError::Network(e.to_string()))?;
    Ok(dest)
}

fn normalise(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalise_removes_punctuation_and_case() {
        assert_eq!(
            normalise("The Fellowship of the Ring!"),
            "thefellowshipofthering"
        );
        assert_eq!(normalise("Harry Potter (Book 1)"), "harrypotterbook1");
    }

    #[test]
    fn doc_to_work_maps_fields_properly() {
        let doc = SearchDoc {
            key: Some("/works/OL123W".into()),
            title: Some("Dune".into()),
            author_name: Some(vec!["Frank Herbert".into(), "Other Author".into()]),
            first_publish_year: Some(1965),
            cover_i: Some(42),
        };
        let work = doc_to_work(doc).expect("should yield work");
        assert_eq!(work.title, "Dune");
        assert_eq!(work.key, "/works/OL123W");
        assert_eq!(work.author, "Frank Herbert");
        assert_eq!(work.year, 1965);
        assert_eq!(work.cover_i, Some(42));
    }

    #[test]
    fn doc_to_work_returns_none_for_missing_or_empty_title() {
        let doc_none = SearchDoc {
            key: None,
            title: None,
            author_name: None,
            first_publish_year: None,
            cover_i: None,
        };
        assert!(doc_to_work(doc_none).is_none());

        let doc_empty = SearchDoc {
            key: None,
            title: Some("   ".into()),
            author_name: None,
            first_publish_year: None,
            cover_i: None,
        };
        assert!(doc_to_work(doc_empty).is_none());
    }

    #[test]
    fn empty_search_query_returns_empty() {
        assert!(search_series("   ").unwrap().is_empty());
    }
}
