//! Google Books — broad coverage, especially for recent and non-English titles
//! where Open Library thins out.
//!
//! Works without an API key, but keyless requests draw on a quota shared by
//! every anonymous client worldwide, so `429` happens unpredictably. A free
//! personal key (Google Cloud console → enable the Books API) lifts that to a
//! private allowance, and can be set in Settings. Descriptions and thumbnails
//! arrive inline, so no second request is needed.

use super::{agent, Candidate, CoverRef, FetchError, MetadataSource, SourceId};
use serde::Deserialize;

const SEARCH_URL: &str = "https://www.googleapis.com/books/v1/volumes";

pub struct GoogleBooks {
    /// Empty means "no key" — still works, just shares the global quota.
    pub api_key: String,
    /// ISO country code sent with every request; see the note in `search`.
    pub country: String,
    /// Reports whether the API key was rejected (`true`) or accepted
    /// (`false`) on a keyed attempt, so Settings can show the rejected-key
    /// state. Only keyed attempts report; quota and network failures say
    /// nothing about the key.
    pub on_key_state: Option<std::sync::Arc<dyn Fn(bool) + Send + Sync>>,
}

impl GoogleBooks {
    fn report_key_state(&self, rejected: bool) {
        if let Some(report) = &self.on_key_state {
            report(rejected);
        }
    }
}

/// Google only serves results for countries it has rights in, so the fallback
/// has to be somewhere with broad coverage rather than blank.
pub const DEFAULT_COUNTRY: &str = "US";

/// Best-effort guess from the environment, so most users never touch Settings.
pub fn detect_country() -> String {
    for var in ["LC_ALL", "LC_MESSAGES", "LANG"] {
        if let Ok(value) = std::env::var(var) {
            // en_IN.UTF-8 -> IN
            if let Some(region) = value.split('.').next().and_then(|s| s.split('_').nth(1)) {
                let region = region.trim();
                if region.len() == 2 && region.chars().all(|c| c.is_ascii_alphabetic()) {
                    return region.to_ascii_uppercase();
                }
            }
        }
    }
    DEFAULT_COUNTRY.to_string()
}

impl MetadataSource for GoogleBooks {
    fn id(&self) -> SourceId {
        SourceId::GoogleBooks
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<Candidate>, FetchError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }

        // Built fresh per attempt: ureq::Request is consumed by call() and is
        // not Clone, so a retry needs its own instance.
        let build = || {
            let mut req = agent()
                .get(SEARCH_URL)
                .query("q", query)
                .query("maxResults", &limit.clamp(1, 40).to_string())
                .query("printType", "books")
                // Google refuses outright when it cannot geolocate the caller's
                // IP ("Cannot determine user location for geographically
                // restricted operation") — common on VPNs and some ISPs. An
                // explicit country sidesteps the lookup.
                .query("country", &self.country);
            if !self.api_key.trim().is_empty() {
                req = req.query("key", self.api_key.trim());
            }
            req
        };

        // One retry on 429: the anonymous quota is shared globally, so a refusal
        // is often a momentary burst rather than a hard block.
        let mut attempt = build().call();
        if matches!(&attempt, Err(ureq::Error::Status(429, _))) {
            std::thread::sleep(std::time::Duration::from_millis(700));
            attempt = build().call();
        }

        let body = match attempt {
            Ok(resp) => resp
                .into_string()
                .map_err(|e| FetchError::Network(e.to_string()))?,
            Err(ureq::Error::Status(code, resp)) => {
                // Google puts a useful sentence in the body; a bare status
                // code leaves the user with nothing to act on.
                let detail = resp
                    .into_string()
                    .ok()
                    .and_then(|b| {
                        serde_json::from_str::<ErrorResponse>(&b)
                            .ok()
                            .map(|e| e.error.message)
                    })
                    .unwrap_or_default();

                return Err(match code {
                    429 if self.api_key.trim().is_empty() => FetchError::Limited(
                        "the shared quota is exhausted. Add a free API key in \
                         Settings > Metadata sources for your own allowance."
                            .into(),
                    ),
                    429 => FetchError::Limited("your API key hit its daily limit.".into()),
                    400 if !self.api_key.trim().is_empty() => {
                        self.report_key_state(true);
                        FetchError::Limited("the API key in Settings was rejected.".into())
                    }
                    403 if detail.contains("location") => {
                        FetchError::Limited(format!("{detail} Set your country in Settings."))
                    }
                    _ if !detail.is_empty() => FetchError::Limited(detail),
                    _ => FetchError::Network(format!("HTTP {code}")),
                });
            }
            Err(err) => return Err(FetchError::Network(err.to_string())),
        };

        let parsed: VolumesResponse =
            serde_json::from_str(&body).map_err(|e| FetchError::Parse(e.to_string()))?;

        // A keyed request Google answered clears any rejected-key state: the
        // key in Settings works, whatever an earlier attempt hit.
        if !self.api_key.trim().is_empty() {
            self.report_key_state(false);
        }

        Ok(parsed
            .items
            .unwrap_or_default()
            .into_iter()
            .map(volume_to_candidate)
            .collect())
    }
}

// ---------------------------------------------------------------------------
// Wire format
// ---------------------------------------------------------------------------

/// Google's error envelope: `{"error": {"message": "...", "code": 403}}`.
#[derive(Debug, Deserialize)]
struct ErrorResponse {
    error: ErrorBody,
}

#[derive(Debug, Deserialize)]
struct ErrorBody {
    #[serde(default)]
    message: String,
}

#[derive(Debug, Deserialize)]
struct VolumesResponse {
    #[serde(default)]
    items: Option<Vec<Volume>>,
}

#[derive(Debug, Deserialize)]
struct Volume {
    #[serde(rename = "volumeInfo", default)]
    volume_info: Option<VolumeInfo>,
}

#[derive(Debug, Deserialize)]
struct VolumeInfo {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    subtitle: Option<String>,
    #[serde(default)]
    authors: Option<Vec<String>>,
    #[serde(default)]
    publisher: Option<String>,
    /// `2012`, `2012-02` or `2012-02-15` — Google is inconsistent.
    #[serde(rename = "publishedDate", default)]
    published_date: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    categories: Option<Vec<String>>,
    #[serde(rename = "imageLinks", default)]
    image_links: Option<ImageLinks>,
}

#[derive(Debug, Deserialize)]
struct ImageLinks {
    #[serde(default)]
    thumbnail: Option<String>,
    #[serde(rename = "smallThumbnail", default)]
    small_thumbnail: Option<String>,
}

fn volume_to_candidate(volume: Volume) -> Candidate {
    let info = volume.volume_info.unwrap_or(VolumeInfo {
        title: None,
        subtitle: None,
        authors: None,
        publisher: None,
        published_date: None,
        description: None,
        categories: None,
        image_links: None,
    });

    // Google splits "Title: Subtitle" across two fields; rejoin so the value
    // matches what is printed on the book.
    let title = match (info.title, info.subtitle) {
        (Some(t), Some(s)) if !s.trim().is_empty() => format!("{t}: {s}"),
        (Some(t), _) => t,
        (None, _) => String::new(),
    };

    let published = info.published_date.unwrap_or_default();
    let first_year = published
        .get(..4)
        .and_then(|y| y.parse::<i64>().ok())
        .filter(|y| *y > 0);

    let cover = info.image_links.and_then(|links| {
        links
            .thumbnail
            .or(links.small_thumbnail)
            // Thumbnails are served over plain HTTP in some responses, and the
            // zoom parameter caps them small; ask for a larger image instead.
            .map(|url| {
                url.replace("http://", "https://")
                    .replace("&edge=curl", "")
                    .replace("zoom=1", "zoom=2")
            })
            .map(CoverRef::Url)
    });

    Candidate {
        title,
        authors: info.authors.unwrap_or_default().join(", "),
        // Google Books has no series concept in this payload.
        series: None,
        tags: info
            .categories
            .unwrap_or_default()
            .into_iter()
            // Categories arrive as "Fiction / Science Fiction / Space Opera";
            // the trailing segment is the useful one.
            .flat_map(|c| {
                c.split('/')
                    .map(|part| part.trim().to_string())
                    .collect::<Vec<_>>()
            })
            .filter(|c| !c.is_empty() && c.len() < 40)
            .take(8)
            .collect(),
        first_year,
        publisher: info.publisher.unwrap_or_default(),
        published,
        cover,
        // Descriptions come back inline, so no follow-up request.
        description: info.description.unwrap_or_default(),
        detail_key: None,
        source: Some(SourceId::GoogleBooks),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(json: &str) -> Vec<Candidate> {
        let parsed: VolumesResponse = serde_json::from_str(json).unwrap();
        parsed
            .items
            .unwrap_or_default()
            .into_iter()
            .map(volume_to_candidate)
            .collect()
    }

    #[test]
    fn parses_a_volume() {
        let out = parse(
            r#"{"items":[{"volumeInfo":{
                "title":"Dune","authors":["Frank Herbert"],
                "publisher":"Ace","publishedDate":"1965-06-01",
                "description":"Desert planet.",
                "categories":["Fiction / Science Fiction / Space Opera"],
                "imageLinks":{"thumbnail":"http://books.google.com/x?zoom=1&edge=curl"}
            }}]}"#,
        );
        assert_eq!(out.len(), 1);
        let c = &out[0];
        assert_eq!(c.title, "Dune");
        assert_eq!(c.authors, "Frank Herbert");
        assert_eq!(c.publisher, "Ace");
        assert_eq!(c.first_year, Some(1965));
        assert_eq!(c.description, "Desert planet.");
        assert_eq!(c.source, Some(SourceId::GoogleBooks));
        // Categories are split on '/' and trimmed.
        assert!(c.tags.contains(&"Space Opera".to_string()));
    }

    #[test]
    fn cover_urls_are_upgraded() {
        let out = parse(
            r#"{"items":[{"volumeInfo":{"title":"A",
                "imageLinks":{"thumbnail":"http://x/y?zoom=1&edge=curl"}}}]}"#,
        );
        match &out[0].cover {
            Some(CoverRef::Url(url)) => {
                assert!(url.starts_with("https://"), "{url}");
                assert!(!url.contains("edge=curl"), "{url}");
                assert!(url.contains("zoom=2"), "{url}");
            }
            other => panic!("expected a URL cover, got {other:?}"),
        }
    }

    #[test]
    fn subtitle_is_rejoined() {
        let out = parse(r#"{"items":[{"volumeInfo":{"title":"Dune","subtitle":"Book One"}}]}"#);
        assert_eq!(out[0].title, "Dune: Book One");
    }

    #[test]
    fn missing_fields_are_tolerated() {
        let out = parse(r#"{"items":[{}]}"#);
        assert_eq!(out.len(), 1);
        assert!(out[0].title.is_empty());
        assert!(out[0].cover.is_none());
    }

    #[test]
    fn empty_payload_is_not_an_error() {
        assert!(parse(r#"{}"#).is_empty());
    }

    #[test]
    fn country_falls_back_when_locale_is_unhelpful() {
        // Nothing parseable in the environment must still give a usable code.
        assert_eq!(DEFAULT_COUNTRY.len(), 2);
        let detected = detect_country();
        assert_eq!(detected.len(), 2, "got {detected}");
        assert!(detected.chars().all(|c| c.is_ascii_uppercase()));
    }

    #[test]
    fn error_envelope_parses() {
        let body = r#"{"error":{"code":403,
            "message":"Cannot determine user location for geographically restricted operation."}}"#;
        let parsed: ErrorResponse = serde_json::from_str(body).unwrap();
        assert!(parsed.error.message.contains("location"));
    }

    #[test]
    fn partial_dates_still_yield_a_year() {
        let out = parse(r#"{"items":[{"volumeInfo":{"title":"A","publishedDate":"2012"}}]}"#);
        assert_eq!(out[0].first_year, Some(2012));
    }

    #[test]
    fn multiple_authors_joined_with_comma() {
        let out = parse(
            r#"{"items":[{"volumeInfo":{"title":"Test","authors":["Author One","Author Two"]}}]}"#,
        );
        assert_eq!(out[0].authors, "Author One, Author Two");
    }

    #[test]
    fn small_thumbnail_fallback_works() {
        let out = parse(
            r#"{"items":[{"volumeInfo":{"title":"Test","imageLinks":{"smallThumbnail":"http://example.com/small.jpg"}}}]}"#,
        );
        match &out[0].cover {
            Some(CoverRef::Url(url)) => assert_eq!(url, "https://example.com/small.jpg"),
            other => panic!("expected CoverRef::Url, got {other:?}"),
        }
    }

    #[test]
    fn category_parsing_and_tag_filtering() {
        let out = parse(
            r#"{"items":[{"volumeInfo":{"title":"Test","categories":["Fiction / Sci-Fi", "This is an extremely long category name that should be filtered out because it exceeds forty characters in total length"]}}]}"#,
        );
        assert_eq!(out[0].tags, vec!["Fiction", "Sci-Fi"]);
    }

    #[test]
    fn invalid_published_date_returns_none_year() {
        let out = parse(r#"{"items":[{"volumeInfo":{"title":"Test","publishedDate":"invalid"}}]}"#);
        assert_eq!(out[0].first_year, None);
    }
}
