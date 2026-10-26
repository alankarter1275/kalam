//! Comic Vine — the western-comics source (decision 46).
//!
//! Comic Vine's searchable “volume” is the series-level record Kalam
//! needs: a run such as *Batman (1940)*, not one issue. Search returns
//! the basics; the volume detail endpoint (`/volume/4050-<id>/`, 4050
//! being Comic Vine's volume resource type) is what carries the full
//! person credits, so selecting a candidate fetches that one record
//! rather than detailing every search hit and burning the key's rate
//! limit on results nobody picked.
//!
//! Every request needs the reader's personal API key (Comic Vine →
//! API), passed as the `api_key` query parameter the API requires.

use super::{
    agent, Candidate, CandidateCredit, CoverRef, FetchError, MetadataSource, SourceId,
};
use serde::Deserialize;

const API_BASE: &str = "https://comicvine.gamespot.com/api";
const SEARCH_FIELDS: &str = concat!(
    "id,name,aliases,deck,description,image,publisher,start_year,",
    "count_of_issues,first_issue,last_issue,person_credits,",
    "concept_credits,api_detail_url,site_detail_url"
);
const DETAIL_FIELDS: &str = SEARCH_FIELDS;

pub struct ComicVine {
    /// The reader's personal Comic Vine API key; empty cannot search.
    pub api_key: String,
    /// Reports whether the key was rejected (status code 100), so
    /// Settings can show the rejected-credential state.
    pub on_key_state: Option<std::sync::Arc<dyn Fn(bool) + Send + Sync>>,
}

impl ComicVine {
    fn report_key_state(&self, rejected: bool) {
        if let Some(report) = &self.on_key_state {
            report(rejected);
        }
    }

    fn require_api_key(&self) -> Result<&str, FetchError> {
        let key = self.api_key.trim();
        if key.is_empty() {
            return Err(FetchError::Limited(
                "add your Comic Vine API key in Settings > Metadata sources.".into(),
            ));
        }
        Ok(key)
    }

    /// One GET with the key attached. `Ok(None)` is a 404.
    fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<Option<String>, FetchError> {
        let api_key = self.require_api_key()?;
        let mut req = agent()
            .get(url)
            .query("api_key", api_key)
            .query("format", "json");
        for (key, value) in query {
            req = req.query(key, value);
        }
        match req.call() {
            Ok(resp) => Ok(Some(
                resp.into_string()
                    .map_err(|e| FetchError::Network(e.to_string()))?,
            )),
            Err(ureq::Error::Status(code, resp)) => {
                let body = resp.into_string().unwrap_or_default();
                if envelope_status(&body) == Some(100) {
                    self.report_key_state(true);
                    return Err(FetchError::Limited(
                        "Comic Vine rejected the API key in Settings.".into(),
                    ));
                }
                match code {
                    404 => Ok(None),
                    401 => {
                        self.report_key_state(true);
                        Err(FetchError::Limited(
                            "Comic Vine rejected the API key in Settings.".into(),
                        ))
                    }
                    403 => Err(FetchError::Limited(
                        "Comic Vine refused the request. Check the API key, or try again later."
                            .into(),
                    )),
                    429 => Err(FetchError::Limited(
                        "Comic Vine is rate limiting this API key; try again later.".into(),
                    )),
                    _ => Err(FetchError::Network(format!("HTTP {code}"))),
                }
            }
            Err(err) => Err(FetchError::Network(err.to_string())),
        }
    }

    /// Validate the envelope Comic Vine wraps every answer in. Returns
    /// `Ok(false)` for “object not found”, which a refresh reports as
    /// *gone* rather than as a failure.
    fn check_envelope(&self, body: &str) -> Result<bool, FetchError> {
        let meta: EnvelopeMeta =
            serde_json::from_str(body).map_err(|e| FetchError::Parse(e.to_string()))?;
        match meta.status_code {
            1 => {
                self.report_key_state(false);
                Ok(true)
            }
            100 => {
                self.report_key_state(true);
                Err(FetchError::Limited(
                    "Comic Vine rejected the API key in Settings.".into(),
                ))
            }
            101 => Ok(false),
            _ => {
                let message = meta.error.trim();
                Err(FetchError::Limited(if message.is_empty() {
                    format!("Comic Vine returned status {}.", meta.status_code)
                } else {
                    message.to_string()
                }))
            }
        }
    }

    fn fetch_volume(&self, external_id: &str) -> Result<Option<Candidate>, FetchError> {
        let Some(id) = extract_volume_id(external_id) else {
            return Err(FetchError::Parse(format!(
                "“{external_id}” is not a Comic Vine volume id"
            )));
        };
        let url = format!("{API_BASE}/volume/4050-{id}/");
        let Some(body) = self.get(&url, &[("field_list", DETAIL_FIELDS)])? else {
            return Ok(None);
        };
        if !self.check_envelope(&body)? {
            return Ok(None);
        }
        parse_detail_body(&body).map(Some)
    }
}

impl MetadataSource for ComicVine {
    fn id(&self) -> SourceId {
        SourceId::ComicVine
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<Candidate>, FetchError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        self.require_api_key()?;
        // The search resource itself caps a page at ten results.
        let limit = limit.clamp(1, 10).to_string();
        let url = format!("{API_BASE}/search/");
        let Some(body) = self.get(
            &url,
            &[
                ("query", query),
                ("resources", "volume"),
                ("limit", &limit),
                ("field_list", SEARCH_FIELDS),
            ],
        )?
        else {
            return Ok(Vec::new());
        };
        if !self.check_envelope(&body)? {
            return Ok(Vec::new());
        }
        parse_search_body(&body)
    }

    fn fetch_description(&self, detail_key: &str) -> Result<String, FetchError> {
        let Some(id) = extract_volume_id(detail_key) else {
            return Ok(String::new());
        };
        Ok(self
            .fetch_volume(&id)?
            .map(|candidate| candidate.description)
            .unwrap_or_default())
    }

    fn fetch_candidate_details(
        &self,
        candidate: &Candidate,
    ) -> Result<Candidate, FetchError> {
        let id = candidate
            .external_id
            .as_deref()
            .and_then(extract_volume_id)
            .or_else(|| {
                candidate
                    .detail_key
                    .as_deref()
                    .and_then(extract_volume_id)
            });
        let Some(id) = id else {
            return Ok(candidate.clone());
        };
        match self.fetch_volume(&id)? {
            Some(detail) => Ok(merge_candidate(candidate.clone(), detail)),
            None => Ok(candidate.clone()),
        }
    }

    fn fetch_by_external_id(
        &self,
        external_id: &str,
    ) -> Result<Option<Candidate>, FetchError> {
        self.fetch_volume(external_id)
    }
}

/// Pull the numeric volume id out of a bare id, a `4050-<id>` resource
/// id, or an `api_detail_url`.
fn extract_volume_id(raw: &str) -> Option<String> {
    let key = raw.trim().trim_end_matches('/');
    let tail = key.rsplit('/').next().unwrap_or(key);
    let digits = tail.strip_prefix("4050-").unwrap_or(tail);
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit()))
        .then(|| digits.to_string())
}

fn envelope_status(body: &str) -> Option<i64> {
    serde_json::from_str::<EnvelopeMeta>(body)
        .ok()
        .map(|meta| meta.status_code)
}

/// Fill any gap in the detailed record from the search hit that led to
/// it; the detail payload wins everywhere it actually says something.
fn merge_candidate(base: Candidate, mut detail: Candidate) -> Candidate {
    if detail.title.is_empty() {
        detail.title = base.title;
    }
    if detail.authors.is_empty() {
        detail.authors = base.authors;
    }
    if detail.series.is_none() {
        detail.series = base.series;
    }
    if detail.tags.is_empty() {
        detail.tags = base.tags;
    }
    if detail.first_year.is_none() {
        detail.first_year = base.first_year;
    }
    if detail.publisher.is_empty() {
        detail.publisher = base.publisher;
    }
    if detail.published.is_empty() {
        detail.published = base.published;
    }
    if detail.cover.is_none() {
        detail.cover = base.cover;
    }
    if detail.description.is_empty() {
        detail.description = base.description;
    }
    if detail.detail_key.is_none() {
        detail.detail_key = base.detail_key;
    }
    if detail.credits.is_empty() {
        detail.credits = base.credits;
    }
    if detail.aliases.is_empty() {
        detail.aliases = base.aliases;
    }
    if detail.known_chapters.is_none() {
        detail.known_chapters = base.known_chapters;
    }
    if detail.external_id.is_none() {
        detail.external_id = base.external_id;
    }
    if detail.source_url.is_none() {
        detail.source_url = base.source_url;
    }
    detail
}

// ---------------------------------------------------------------------------
// Wire format
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct EnvelopeMeta {
    #[serde(default)]
    error: String,
    #[serde(default)]
    status_code: i64,
}

#[derive(Debug, Deserialize)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<Volume>,
}

#[derive(Debug, Deserialize)]
struct DetailResponse {
    results: Volume,
}

#[derive(Debug, Deserialize, Default)]
struct Volume {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    name: String,
    #[serde(default)]
    aliases: Option<String>,
    #[serde(default)]
    deck: Option<String>,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    image: Option<Image>,
    #[serde(default)]
    publisher: Option<NamedRef>,
    #[serde(default)]
    start_year: Option<serde_json::Value>,
    #[serde(default)]
    count_of_issues: Option<i64>,
    #[serde(default)]
    person_credits: Vec<PersonCredit>,
    #[serde(default)]
    concept_credits: Vec<NamedRef>,
    #[serde(default)]
    api_detail_url: Option<String>,
    #[serde(default)]
    site_detail_url: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct NamedRef {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct PersonCredit {
    #[serde(default)]
    name: String,
    #[serde(default)]
    role: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct Image {
    #[serde(default)]
    original_url: Option<String>,
    #[serde(default)]
    super_url: Option<String>,
    #[serde(default)]
    screen_url: Option<String>,
    #[serde(default)]
    medium_url: Option<String>,
    #[serde(default)]
    small_url: Option<String>,
    #[serde(default)]
    thumb_url: Option<String>,
}

fn parse_search_body(body: &str) -> Result<Vec<Candidate>, FetchError> {
    let parsed: SearchResponse =
        serde_json::from_str(body).map_err(|e| FetchError::Parse(e.to_string()))?;
    Ok(parsed
        .results
        .into_iter()
        .map(volume_to_candidate)
        .filter(|candidate| !candidate.title.trim().is_empty())
        .collect())
}

fn parse_detail_body(body: &str) -> Result<Candidate, FetchError> {
    let parsed: DetailResponse =
        serde_json::from_str(body).map_err(|e| FetchError::Parse(e.to_string()))?;
    Ok(volume_to_candidate(parsed.results))
}

fn volume_to_candidate(volume: Volume) -> Candidate {
    let title = volume.name.trim().to_string();

    let mut aliases: Vec<String> = Vec::new();
    if let Some(raw) = &volume.aliases {
        for alias in raw.lines() {
            let alias = alias.trim().to_string();
            if !alias.is_empty()
                && alias != title
                && !aliases
                    .iter()
                    .any(|existing| existing.eq_ignore_ascii_case(&alias))
            {
                aliases.push(alias);
            }
        }
    }

    let mut credits: Vec<CandidateCredit> = Vec::new();
    for person in &volume.person_credits {
        let name = person.name.trim();
        if name.is_empty() {
            continue;
        }
        let roles: Vec<String> = match &person.role {
            Some(raw) => raw
                .split(',')
                .map(credit_role)
                .filter(|role| !role.is_empty())
                .collect(),
            None => Vec::new(),
        };
        let roles = if roles.is_empty() {
            vec!["Creator".to_string()]
        } else {
            roles
        };
        for role in roles {
            let credit = CandidateCredit {
                role,
                name: name.to_string(),
            };
            if !credits.contains(&credit) {
                credits.push(credit);
            }
        }
        if credits.len() >= 40 {
            break;
        }
    }
    credits.truncate(40);

    let mut author_names: Vec<String> = Vec::new();
    for credit in &credits {
        if !author_names
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&credit.name))
        {
            author_names.push(credit.name.clone());
        }
    }

    let mut tags: Vec<String> = Vec::new();
    for concept in &volume.concept_credits {
        let name = concept.name.trim().to_string();
        if !name.is_empty()
            && name.len() < 40
            && !tags
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&name))
        {
            tags.push(name);
        }
    }
    tags.truncate(10);

    // `deck` is Comic Vine's own short summary; the full description is
    // a wiki page in HTML, useful only once cleaned and capped.
    let description = volume
        .deck
        .as_deref()
        .map(clean_html)
        .filter(|text| !text.is_empty())
        .or_else(|| volume.description.as_deref().map(clean_html))
        .unwrap_or_default();

    let published = match &volume.start_year {
        Some(serde_json::Value::String(year)) => year.trim().to_string(),
        Some(serde_json::Value::Number(year)) => year.to_string(),
        _ => String::new(),
    };
    let first_year = published
        .get(..4)
        .filter(|year| year.chars().all(|c| c.is_ascii_digit()))
        .and_then(|year| year.parse::<i64>().ok())
        .filter(|year| *year > 0);

    let external_id = (volume.id > 0).then(|| volume.id.to_string());
    let detail_key = volume
        .api_detail_url
        .clone()
        .filter(|url| !url.trim().is_empty())
        .or_else(|| external_id.clone());

    Candidate {
        series: Some(title.clone()),
        title,
        authors: author_names.join(", "),
        tags,
        first_year,
        publisher: volume
            .publisher
            .as_ref()
            .map(|publisher| publisher.name.trim().to_string())
            .unwrap_or_default(),
        published,
        cover: volume.image.as_ref().and_then(cover_ref),
        description,
        detail_key,
        source: Some(SourceId::ComicVine),
        credits,
        aliases,
        media_type: "Comic series".to_string(),
        known_chapters: volume.count_of_issues.filter(|n| *n > 0),
        source_url: volume
            .site_detail_url
            .clone()
            .filter(|url| !url.trim().is_empty()),
        external_id,
        ..Default::default()
    }
}

fn cover_ref(image: &Image) -> Option<CoverRef> {
    let pick = |values: &[&Option<String>]| {
        values
            .iter()
            .filter_map(|value| value.as_deref().map(str::trim))
            .find(|value| !value.is_empty())
            .map(str::to_string)
    };
    // Screen size is plenty for a series cover and avoids hauling the
    // multi-megabyte original through a thumbnail grid.
    let full = pick(&[
        &image.screen_url,
        &image.super_url,
        &image.original_url,
        &image.medium_url,
    ])?;
    let thumbnail = pick(&[&image.medium_url, &image.small_url, &image.thumb_url]);
    match thumbnail {
        Some(thumbnail) if thumbnail != full => Some(CoverRef::UrlPair { full, thumbnail }),
        _ => Some(CoverRef::Url(full)),
    }
}

fn credit_role(raw: &str) -> String {
    let cleaned = raw.trim().replace('_', " ");
    match cleaned.to_lowercase().as_str() {
        "" => String::new(),
        "writer" => "Writer".to_string(),
        "penciler" | "penciller" => "Penciller".to_string(),
        "inker" => "Inker".to_string(),
        "colorist" | "colourist" => "Colorist".to_string(),
        "letterer" => "Letterer".to_string(),
        "cover" | "cover artist" => "Cover Artist".to_string(),
        "artist" => "Artist".to_string(),
        "editor" => "Editor".to_string(),
        "creator" => "Creator".to_string(),
        _ => cleaned
            .split_whitespace()
            .map(|word| {
                let mut chars = word.chars();
                match chars.next() {
                    Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect::<Vec<_>>()
            .join(" "),
    }
}

/// Comic Vine descriptions are wiki HTML. Kalam stores plain text, so
/// tags become nothing (paragraph and line breaks become newlines),
/// the common entities are decoded, and the result is capped before a
/// decades-long series history can flood the catalog.
fn clean_html(raw: &str) -> String {
    let mut text = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '<' {
            text.push(ch);
            continue;
        }
        let mut tag = String::new();
        for next in chars.by_ref() {
            if next == '>' {
                break;
            }
            tag.push(next);
        }
        let name = tag
            .trim()
            .trim_start_matches('/')
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_lowercase();
        if name == "p" || name == "br" || name == "div" || name == "li" {
            text.push('\n');
        }
    }

    let text = text
        .replace("&nbsp;", " ")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&apos;", "'")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&amp;", "&");

    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.split_whitespace().collect::<Vec<_>>().join(" ");
        if line.is_empty() {
            if lines.last().is_some_and(|last: &String| !last.is_empty()) {
                lines.push(String::new());
            }
        } else {
            lines.push(line);
        }
    }
    while lines.last().is_some_and(|last: &String| last.is_empty()) {
        lines.pop();
    }
    lines.join("\n").chars().take(6000).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_volume_search_result_with_split_roles() {
        let body = r#"{"error":"OK","status_code":1,"results":[{
            "id":796,
            "name":"Batman",
            "aliases":"Batman Vol 1\nThe Dark Knight",
            "deck":"The original ongoing Batman series.",
            "image":{
                "original_url":"https://cdn.example/original.jpg",
                "screen_url":"https://cdn.example/screen.jpg",
                "medium_url":"https://cdn.example/medium.jpg"
            },
            "publisher":{"id":10,"name":"DC Comics"},
            "start_year":"1940",
            "count_of_issues":713,
            "person_credits":[
                {"id":1,"name":"Bill Finger","role":"writer"},
                {"id":2,"name":"Bob Kane","role":"penciler, cover"}
            ],
            "concept_credits":[{"id":3,"name":"Superhero"}],
            "api_detail_url":"https://comicvine.gamespot.com/api/volume/4050-796/",
            "site_detail_url":"https://comicvine.gamespot.com/batman/4050-796/"
        }]}"#;
        let out = parse_search_body(body).expect("search parses");
        assert_eq!(out.len(), 1);
        let c = &out[0];
        assert_eq!(c.title, "Batman");
        assert_eq!(c.publisher, "DC Comics");
        assert_eq!(c.first_year, Some(1940));
        assert_eq!(c.known_chapters, Some(713));
        assert_eq!(c.external_id.as_deref(), Some("796"));
        assert_eq!(c.aliases, vec!["Batman Vol 1", "The Dark Knight"]);
        assert_eq!(c.authors, "Bill Finger, Bob Kane");
        assert_eq!(
            c.credits,
            vec![
                CandidateCredit {
                    role: "Writer".into(),
                    name: "Bill Finger".into(),
                },
                CandidateCredit {
                    role: "Penciller".into(),
                    name: "Bob Kane".into(),
                },
                CandidateCredit {
                    role: "Cover Artist".into(),
                    name: "Bob Kane".into(),
                },
            ]
        );
        assert_eq!(
            c.cover,
            Some(CoverRef::UrlPair {
                full: "https://cdn.example/screen.jpg".into(),
                thumbnail: "https://cdn.example/medium.jpg".into(),
            })
        );
    }

    #[test]
    fn detail_results_are_an_object_and_html_is_cleaned() {
        let body = r#"{"error":"OK","status_code":1,"results":{
            "id":50171,
            "name":"Feynman",
            "deck":"",
            "description":"<p>A comic biography of <b>Richard Feynman</b> &amp; his work.</p><p>Second&nbsp;paragraph.</p>",
            "start_year":2011
        }}"#;
        let c = parse_detail_body(body).expect("detail parses");
        assert_eq!(c.title, "Feynman");
        assert_eq!(c.first_year, Some(2011));
        assert!(c.description.contains("Richard Feynman & his work."));
        assert!(c.description.contains("Second paragraph."));
        assert!(!c.description.contains('<'));
    }

    #[test]
    fn missing_api_key_is_an_actionable_error() {
        let source = ComicVine {
            api_key: String::new(),
            on_key_state: None,
        };
        let err = source.search("Batman", 5).unwrap_err();
        assert!(err.to_string().contains("API key"), "{err}");
    }

    #[test]
    fn volume_ids_are_extracted_from_every_spelling() {
        assert_eq!(extract_volume_id("796").as_deref(), Some("796"));
        assert_eq!(extract_volume_id("4050-796").as_deref(), Some("796"));
        assert_eq!(
            extract_volume_id("https://comicvine.gamespot.com/api/volume/4050-796/")
                .as_deref(),
            Some("796")
        );
        assert_eq!(extract_volume_id("batman"), None);
    }

    #[test]
    fn envelope_status_reads_the_wrapper() {
        assert_eq!(envelope_status(r#"{"error":"OK","status_code":1}"#), Some(1));
        assert_eq!(
            envelope_status(r#"{"error":"Invalid API Key","status_code":100}"#),
            Some(100)
        );
        assert_eq!(envelope_status("not json"), None);
    }
}
