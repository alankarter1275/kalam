//! Open Library — the default metadata source.
//!
//! Non-profit (Internet Archive), no API key, no registration. Coverage of
//! older and public-domain titles is excellent; recent commercial releases are
//! patchier, which is why Google Books sits alongside it.

use super::{agent, Candidate, CoverRef, FetchError, MetadataSource, SourceId};
use serde::Deserialize;

const SEARCH_URL: &str = "https://openlibrary.org/search.json";

pub struct OpenLibrary;

impl MetadataSource for OpenLibrary {
    fn id(&self) -> SourceId {
        SourceId::OpenLibrary
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<Candidate>, FetchError> {
        search(query, limit)
    }

    fn fetch_description(&self, detail_key: &str) -> Result<String, FetchError> {
        fetch_description(detail_key)
    }
}

// ---------------------------------------------------------------------------
// Wire format
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    docs: Vec<SearchDoc>,
}

#[derive(Debug, Deserialize)]
struct SearchDoc {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    author_name: Option<Vec<String>>,
    #[serde(default)]
    first_publish_year: Option<i64>,
    #[serde(default)]
    cover_i: Option<i64>,
    #[serde(default)]
    key: Option<String>,
    #[serde(default)]
    subject: Option<Vec<String>>,
    #[serde(default)]
    series: Option<Vec<String>>,
    #[serde(default)]
    publisher: Option<Vec<String>>,
    #[serde(default)]
    publish_date: Option<Vec<String>>,
}

/// `description` is maddeningly polymorphic: sometimes a string, sometimes
/// `{ "value": "…" }`.
#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum Description {
    Text(String),
    Object { value: String },
}

#[derive(Debug, Deserialize)]
struct WorkResponse {
    #[serde(default)]
    description: Option<Description>,
}

// ---------------------------------------------------------------------------
// Requests
// ---------------------------------------------------------------------------

/// Search by free text, or by title/author when both are known.
pub fn search(query: &str, limit: usize) -> Result<Vec<Candidate>, FetchError> {
    let query = query.trim();
    if query.is_empty() {
        return Ok(Vec::new());
    }

    let body = agent()
        .get(SEARCH_URL)
        .query("q", query)
        .query("limit", &limit.clamp(1, 20).to_string())
        // Ask only for the fields we use — the default payload is enormous.
        .query(
            "fields",
            concat!(
                "title,author_name,first_publish_year,cover_i,key,subject,",
                "series,publisher,publish_date"
            ),
        )
        .call()
        .map_err(|e| FetchError::Network(e.to_string()))?
        .into_string()
        .map_err(|e| FetchError::Network(e.to_string()))?;

    let parsed: SearchResponse =
        serde_json::from_str(&body).map_err(|e| FetchError::Parse(e.to_string()))?;

    Ok(parsed.docs.into_iter().map(doc_to_candidate).collect())
}

fn doc_to_candidate(doc: SearchDoc) -> Candidate {
    Candidate {
        title: doc.title.unwrap_or_default(),
        authors: doc.author_name.unwrap_or_default().join(", "),
        series: doc.series.and_then(|s| s.into_iter().next()),
        // Open Library subjects are long-tailed; keep a usable handful.
        tags: doc
            .subject
            .unwrap_or_default()
            .into_iter()
            .filter(|s| s.len() < 40)
            .take(8)
            .collect(),
        first_year: doc.first_publish_year,
        publisher: doc
            .publisher
            .unwrap_or_default()
            .into_iter()
            .next()
            .unwrap_or_default(),
        // Prefer a printed date; fall back to the first-publication year.
        published: doc
            .publish_date
            .unwrap_or_default()
            .into_iter()
            .next()
            .unwrap_or_else(|| {
                doc.first_publish_year
                    .map(|y| y.to_string())
                    .unwrap_or_default()
            }),
        cover: doc.cover_i.map(CoverRef::OpenLibraryId),
        // Open Library keeps descriptions on the work record, not the search
        // result, so they need a second request.
        description: String::new(),
        detail_key: doc.key,
        source: Some(SourceId::OpenLibrary),
        ..Default::default()
    }
}

/// Fetch a work's description. Absent descriptions are common and not an error.
pub fn fetch_description(work_key: &str) -> Result<String, FetchError> {
    let key = work_key.trim_start_matches('/');
    let url = format!("https://openlibrary.org/{key}.json");

    let body = agent()
        .get(&url)
        .call()
        .map_err(|e| FetchError::Network(e.to_string()))?
        .into_string()
        .map_err(|e| FetchError::Network(e.to_string()))?;

    let parsed: WorkResponse =
        serde_json::from_str(&body).map_err(|e| FetchError::Parse(e.to_string()))?;

    Ok(match parsed.description {
        Some(Description::Text(t)) => t,
        Some(Description::Object { value }) => value,
        None => String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_query_short_circuits() {
        assert!(search("   ", 5).unwrap().is_empty());
    }

    #[test]
    fn parses_search_payload() {
        let json = r#"{"docs":[
            {"title":"Dune","author_name":["Frank Herbert"],
             "first_publish_year":1965,"cover_i":123,"key":"/works/OL1W",
             "subject":["Science fiction","Desert"]}
        ]}"#;
        let parsed: SearchResponse = serde_json::from_str(json).unwrap();
        let c = doc_to_candidate(parsed.docs.into_iter().next().unwrap());
        assert_eq!(c.title, "Dune");
        assert_eq!(c.authors, "Frank Herbert");
        assert_eq!(c.first_year, Some(1965));
        assert_eq!(c.cover, Some(CoverRef::OpenLibraryId(123)));
        assert_eq!(c.tags.len(), 2);
        assert_eq!(c.source, Some(SourceId::OpenLibrary));
    }

    #[test]
    fn tolerates_missing_fields() {
        let parsed: SearchResponse = serde_json::from_str(r#"{"docs":[{}]}"#).unwrap();
        let c = doc_to_candidate(parsed.docs.into_iter().next().unwrap());
        assert!(c.title.is_empty());
        assert!(c.authors.is_empty());
        assert!(c.cover.is_none());
    }

    #[test]
    fn description_accepts_both_shapes() {
        let a: WorkResponse = serde_json::from_str(r#"{"description":"plain"}"#).unwrap();
        assert!(matches!(a.description, Some(Description::Text(_))));

        let b: WorkResponse =
            serde_json::from_str(r#"{"description":{"value":"nested"}}"#).unwrap();
        assert!(matches!(b.description, Some(Description::Object { .. })));

        let c: WorkResponse = serde_json::from_str("{}").unwrap();
        assert!(c.description.is_none());
    }

    #[test]
    fn long_subjects_are_dropped() {
        let long = "x".repeat(60);
        let json = format!(r#"{{"docs":[{{"subject":["short","{long}"]}}]}}"#);
        let parsed: SearchResponse = serde_json::from_str(&json).unwrap();
        let c = doc_to_candidate(parsed.docs.into_iter().next().unwrap());
        assert_eq!(c.tags, vec!["short"]);
    }

    #[test]
    fn published_date_falls_back_to_first_publish_year() {
        let json = r#"{"docs":[{"title":"Test","first_publish_year":1999}]}"#;
        let parsed: SearchResponse = serde_json::from_str(json).unwrap();
        let c = doc_to_candidate(parsed.docs.into_iter().next().unwrap());
        assert_eq!(c.published, "1999");
    }

    #[test]
    fn multiple_publishers_picks_first() {
        let json = r#"{"docs":[{"title":"Test","publisher":["First Pub","Second Pub"]}]}"#;
        let parsed: SearchResponse = serde_json::from_str(json).unwrap();
        let c = doc_to_candidate(parsed.docs.into_iter().next().unwrap());
        assert_eq!(c.publisher, "First Pub");
    }

    #[test]
    fn series_extracted_from_doc() {
        let json = r#"{"docs":[{"title":"Test","series":["Foundational Series"]}]}"#;
        let parsed: SearchResponse = serde_json::from_str(json).unwrap();
        let c = doc_to_candidate(parsed.docs.into_iter().next().unwrap());
        assert_eq!(c.series, Some("Foundational Series".to_string()));
    }
}
