//! MyAnimeList — the manga source (decision 46), via the official API v2.
//!
//! Public catalog reads authenticate with the reader's own Client ID in
//! the `X-MAL-CLIENT-ID` header (MyAnimeList → Settings → API); there is
//! no keyless official search, so a missing ID is reported as the
//! actionable error it is rather than as a silent empty result. Search
//! asks for the full field set up front, so a candidate already carries
//! its synopsis, credits and serialization — no second call is needed
//! unless a linked series is being refreshed by id.

use super::{
    agent, Candidate, CandidateCredit, CoverRef, FetchError, MetadataSource, SourceId,
};
use serde::de::DeserializeOwned;
use serde::Deserialize;

const API_BASE: &str = "https://api.myanimelist.net/v2/manga";
const FIELDS: &str = concat!(
    "id,title,main_picture,alternative_titles,start_date,end_date,synopsis,",
    "mean,rank,popularity,nsfw,media_type,status,genres,num_volumes,",
    "num_chapters,authors{first_name,last_name},pictures,background,",
    "serialization{name}"
);

pub struct MyAnimeList {
    /// The reader's MyAnimeList API Client ID; empty cannot search.
    pub client_id: String,
    /// Reports whether the Client ID was rejected, so Settings can show
    /// the same rejected-credential state as Google Books.
    pub on_key_state: Option<std::sync::Arc<dyn Fn(bool) + Send + Sync>>,
}

impl MyAnimeList {
    fn report_key_state(&self, rejected: bool) {
        if let Some(report) = &self.on_key_state {
            report(rejected);
        }
    }

    fn require_client_id(&self) -> Result<&str, FetchError> {
        let id = self.client_id.trim();
        if id.is_empty() {
            return Err(FetchError::Limited(
                "add your MyAnimeList Client ID in Settings > Metadata sources.".into(),
            ));
        }
        Ok(id)
    }

    /// One authenticated GET. `Ok(None)` is a 404: a refresh for a record
    /// MyAnimeList no longer has is an answer, not a failure.
    fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<Option<String>, FetchError> {
        let client_id = self.require_client_id()?;
        let mut req = agent().get(url).set("X-MAL-CLIENT-ID", client_id);
        for (key, value) in query {
            req = req.query(key, value);
        }
        match req.call() {
            Ok(resp) => {
                let body = resp
                    .into_string()
                    .map_err(|e| FetchError::Network(e.to_string()))?;
                self.report_key_state(false);
                Ok(Some(body))
            }
            Err(ureq::Error::Status(code, resp)) => {
                let detail = resp
                    .into_string()
                    .ok()
                    .and_then(|body| serde_json::from_str::<ErrorResponse>(&body).ok())
                    .map(|e| e.message.trim().to_string())
                    .unwrap_or_default();
                match code {
                    404 => Ok(None),
                    401 => {
                        self.report_key_state(true);
                        Err(FetchError::Limited(
                            "MyAnimeList rejected the Client ID in Settings.".into(),
                        ))
                    }
                    403 => Err(FetchError::Limited(
                        "MyAnimeList refused the request. Check the Client ID, or try again later."
                            .into(),
                    )),
                    429 => Err(FetchError::Limited(
                        "MyAnimeList is rate limiting requests; try again in a moment.".into(),
                    )),
                    400 if !detail.is_empty() => Err(FetchError::Limited(detail)),
                    _ => Err(FetchError::Network(format!("HTTP {code}"))),
                }
            }
            Err(err) => Err(FetchError::Network(err.to_string())),
        }
    }

    fn fetch_manga(&self, external_id: &str) -> Result<Option<Candidate>, FetchError> {
        let id = external_id.trim();
        if id.is_empty() || !id.chars().all(|c| c.is_ascii_digit()) {
            return Err(FetchError::Parse(format!(
                "“{external_id}” is not a MyAnimeList manga id"
            )));
        }
        let url = format!("{API_BASE}/{id}");
        let Some(body) = self.get(&url, &[("fields", FIELDS), ("nsfw", "true")])? else {
            return Ok(None);
        };
        parse_detail_body(&body).map(Some)
    }
}

impl MetadataSource for MyAnimeList {
    fn id(&self) -> SourceId {
        SourceId::MyAnimeList
    }

    fn search(&self, query: &str, limit: usize) -> Result<Vec<Candidate>, FetchError> {
        let query = query.trim();
        if query.is_empty() {
            return Ok(Vec::new());
        }
        self.require_client_id()?;
        let limit = limit.clamp(1, 100).to_string();
        let Some(body) = self.get(
            API_BASE,
            &[
                ("q", query),
                ("limit", &limit),
                ("fields", FIELDS),
                // A metadata lookup is explicit: hiding adult entries
                // would make some owned series unfindable.
                ("nsfw", "true"),
            ],
        )?
        else {
            return Ok(Vec::new());
        };
        parse_search_body(&body)
    }

    fn fetch_description(&self, detail_key: &str) -> Result<String, FetchError> {
        let Some(id) = extract_manga_id(detail_key) else {
            return Ok(String::new());
        };
        Ok(self
            .fetch_manga(&id)?
            .map(|candidate| candidate.description)
            .unwrap_or_default())
    }

    fn fetch_by_external_id(
        &self,
        external_id: &str,
    ) -> Result<Option<Candidate>, FetchError> {
        self.fetch_manga(external_id)
    }
}

fn extract_manga_id(detail_key: &str) -> Option<String> {
    let key = detail_key.trim().trim_end_matches('/');
    let candidate = key.rsplit('/').next().unwrap_or(key);
    (!candidate.is_empty() && candidate.chars().all(|c| c.is_ascii_digit()))
        .then(|| candidate.to_string())
}

// ---------------------------------------------------------------------------
// Wire format
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
struct ErrorResponse {
    #[serde(default)]
    message: String,
}

#[derive(Debug, Deserialize)]
struct ListResponse {
    #[serde(default)]
    data: Vec<ListEntry>,
}

#[derive(Debug, Deserialize)]
struct ListEntry {
    node: Manga,
}

#[derive(Debug, Deserialize)]
struct Manga {
    #[serde(default)]
    id: i64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    main_picture: Option<Picture>,
    #[serde(default)]
    alternative_titles: Option<AlternativeTitles>,
    #[serde(default)]
    start_date: String,
    #[serde(default)]
    end_date: String,
    #[serde(default)]
    synopsis: String,
    #[serde(default)]
    mean: f64,
    #[serde(default)]
    media_type: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    genres: Vec<Named>,
    #[serde(default)]
    num_volumes: i64,
    #[serde(default)]
    num_chapters: i64,
    #[serde(default)]
    authors: Vec<AuthorEntry>,
    #[serde(default)]
    pictures: Vec<Picture>,
    #[serde(default)]
    serialization: Vec<SerializationEntry>,
}

#[derive(Debug, Deserialize)]
struct Picture {
    #[serde(default)]
    medium: String,
    #[serde(default)]
    large: String,
}

#[derive(Debug, Deserialize)]
struct AlternativeTitles {
    #[serde(default)]
    synonyms: Vec<String>,
    #[serde(default)]
    en: String,
    #[serde(default)]
    ja: String,
}

#[derive(Debug, Deserialize, Default)]
struct Named {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct AuthorEntry {
    #[serde(default)]
    node: Person,
    #[serde(default)]
    role: String,
}

#[derive(Debug, Deserialize, Default)]
struct Person {
    #[serde(default)]
    first_name: String,
    #[serde(default)]
    last_name: String,
}

#[derive(Debug, Deserialize)]
struct SerializationEntry {
    #[serde(default)]
    node: Named,
}

/// MAL answers with JSON objects. Without this shape check Serde's
/// sequence form plus the fields' defaults would accept `[]` as an
/// empty result, quietly turning a changed/error payload into “no
/// matches” instead of the parse failure it is.
fn parse_object<T: DeserializeOwned>(body: &str) -> Result<T, FetchError> {
    let value: serde_json::Value =
        serde_json::from_str(body).map_err(|e| FetchError::Parse(e.to_string()))?;
    if !value.is_object() {
        return Err(FetchError::Parse(
            "MyAnimeList returned a response in an unexpected shape.".to_string(),
        ));
    }
    serde_json::from_value(value).map_err(|e| FetchError::Parse(e.to_string()))
}

fn parse_search_body(body: &str) -> Result<Vec<Candidate>, FetchError> {
    let parsed: ListResponse = parse_object(body)?;
    Ok(parsed
        .data
        .into_iter()
        .map(|entry| manga_to_candidate(entry.node))
        .filter(|candidate| !candidate.title.trim().is_empty())
        .collect())
}

fn parse_detail_body(body: &str) -> Result<Candidate, FetchError> {
    let parsed: Manga = parse_object(body)?;
    Ok(manga_to_candidate(parsed))
}

fn manga_to_candidate(manga: Manga) -> Candidate {
    let alternative = manga.alternative_titles;
    let (english, japanese, synonyms) = match alternative {
        Some(alt) => (alt.en.trim().to_string(), alt.ja.trim().to_string(), alt.synonyms),
        None => (String::new(), String::new(), Vec::new()),
    };

    // The node title is the romaji/main title; an English title, when
    // MyAnimeList has one, is the name on the reader's shelf.
    let title = if !english.is_empty() {
        english
    } else {
        manga.title.trim().to_string()
    };
    let original_title = if !japanese.is_empty() && japanese != title {
        japanese
    } else if manga.title.trim() != title {
        manga.title.trim().to_string()
    } else {
        String::new()
    };

    let mut aliases: Vec<String> = Vec::new();
    for alias in synonyms {
        let alias = alias.trim().to_string();
        if !alias.is_empty()
            && alias != title
            && alias != original_title
            && !aliases
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&alias))
        {
            aliases.push(alias);
        }
    }

    let credits: Vec<CandidateCredit> = manga
        .authors
        .into_iter()
        .filter_map(|entry| {
            let name = [entry.node.first_name.trim(), entry.node.last_name.trim()]
                .into_iter()
                .filter(|part| !part.is_empty())
                .collect::<Vec<_>>()
                .join(" ");
            (!name.is_empty()).then(|| CandidateCredit {
                role: credit_role(&entry.role),
                name,
            })
        })
        .collect();

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
    for genre in manga.genres {
        let name = genre.name.trim().to_string();
        if !name.is_empty()
            && !tags
                .iter()
                .any(|existing| existing.eq_ignore_ascii_case(&name))
        {
            tags.push(name);
        }
    }
    tags.truncate(12);

    let serialization = manga
        .serialization
        .into_iter()
        .map(|entry| entry.node.name.trim().to_string())
        .filter(|name| !name.is_empty())
        .collect::<Vec<_>>()
        .join(", ");

    let published = manga.start_date.trim().to_string();
    let first_year = published
        .get(..4)
        .filter(|year| year.chars().all(|c| c.is_ascii_digit()))
        .and_then(|year| year.parse::<i64>().ok())
        .filter(|year| *year > 0);

    let cover = manga
        .main_picture
        .as_ref()
        .and_then(cover_ref)
        .or_else(|| manga.pictures.first().and_then(cover_ref));

    let external_id = (manga.id > 0).then(|| manga.id.to_string());

    Candidate {
        series: Some(title.clone()),
        title,
        authors: author_names.join(", "),
        tags,
        first_year,
        publisher: String::new(),
        published,
        cover,
        description: clean_synopsis(&manga.synopsis),
        detail_key: external_id.clone(),
        source: Some(SourceId::MyAnimeList),
        credits,
        aliases,
        original_title,
        serialization,
        status: status_label(&manga.status),
        media_type: media_type_label(&manga.media_type),
        end_date: manga.end_date.trim().to_string(),
        known_volumes: (manga.num_volumes > 0).then_some(manga.num_volumes),
        known_chapters: (manga.num_chapters > 0).then_some(manga.num_chapters),
        rating: (manga.mean > 0.0).then_some(manga.mean),
        source_url: external_id
            .as_ref()
            .map(|id| format!("https://myanimelist.net/manga/{id}")),
        external_id,
    }
}

fn cover_ref(picture: &Picture) -> Option<CoverRef> {
    let large = picture.large.trim();
    let medium = picture.medium.trim();
    match (large.is_empty(), medium.is_empty()) {
        (false, false) if large != medium => Some(CoverRef::UrlPair {
            full: large.to_string(),
            thumbnail: medium.to_string(),
        }),
        (false, _) => Some(CoverRef::Url(large.to_string())),
        (true, false) => Some(CoverRef::Url(medium.to_string())),
        (true, true) => None,
    }
}

fn clean_synopsis(raw: &str) -> String {
    raw.trim()
        .trim_end_matches("[Written by MAL Rewrite]")
        .trim()
        .to_string()
}

fn credit_role(raw: &str) -> String {
    let cleaned = raw.trim().replace('_', " ");
    match cleaned.to_lowercase().as_str() {
        "" => "Author".to_string(),
        "story & art" | "story and art" => "Story & Art".to_string(),
        "story" => "Story".to_string(),
        "art" => "Art".to_string(),
        "original creator" => "Original Creator".to_string(),
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

fn status_label(raw: &str) -> String {
    match raw.trim().to_lowercase().as_str() {
        "currently_publishing" | "publishing" => "Ongoing".to_string(),
        "finished" => "Completed".to_string(),
        "not_yet_published" | "not_published" => "Not yet published".to_string(),
        "on_hiatus" => "Hiatus".to_string(),
        "discontinued" => "Cancelled".to_string(),
        "" => String::new(),
        other => other.replace('_', " "),
    }
}

fn media_type_label(raw: &str) -> String {
    match raw.trim().to_lowercase().as_str() {
        "manga" => "Manga".to_string(),
        "novel" => "Novel".to_string(),
        "one_shot" => "One-shot".to_string(),
        "doujinshi" => "Doujinshi".to_string(),
        "manhwa" => "Manhwa".to_string(),
        "manhua" => "Manhua".to_string(),
        "oel" => "OEL".to_string(),
        "" => String::new(),
        other => other.replace('_', " "),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_search_result_with_credits_and_serialization() {
        let body = r#"{"data":[{"node":{
            "id":2,
            "title":"Berserk",
            "main_picture":{"medium":"https://cdn.example/m.jpg","large":"https://cdn.example/l.jpg"},
            "alternative_titles":{"synonyms":["Berserk: The Prototype"],"en":"Berserk","ja":"ベルセルク"},
            "start_date":"1989-08-25",
            "synopsis":"Guts travels a dark medieval world. [Written by MAL Rewrite]",
            "mean":9.47,
            "media_type":"manga",
            "status":"currently_publishing",
            "genres":[{"id":1,"name":"Action"},{"id":2,"name":"Adventure"}],
            "num_volumes":42,
            "num_chapters":380,
            "authors":[{"node":{"id":1868,"first_name":"Kentaro","last_name":"Miura"},"role":"Story & Art"}],
            "serialization":[{"node":{"id":2,"name":"Young Animal"},"role":"Serialization"}]
        }}]}"#;
        let out = parse_search_body(body).expect("search parses");
        assert_eq!(out.len(), 1);
        let c = &out[0];
        assert_eq!(c.title, "Berserk");
        assert_eq!(c.original_title, "ベルセルク");
        assert_eq!(c.authors, "Kentaro Miura");
        assert_eq!(c.credits[0].role, "Story & Art");
        assert_eq!(c.serialization, "Young Animal");
        assert_eq!(c.status, "Ongoing");
        assert_eq!(c.media_type, "Manga");
        assert_eq!(c.first_year, Some(1989));
        assert_eq!(c.known_volumes, Some(42));
        assert_eq!(c.known_chapters, Some(380));
        assert_eq!(c.external_id.as_deref(), Some("2"));
        assert!(c.description.ends_with("world."));
        assert_eq!(
            c.cover,
            Some(CoverRef::UrlPair {
                full: "https://cdn.example/l.jpg".into(),
                thumbnail: "https://cdn.example/m.jpg".into(),
            })
        );
    }

    #[test]
    fn english_title_wins_and_unknown_counts_stay_unknown() {
        let body = r#"{"data":[{"node":{
            "id":13,
            "title":"One Piece",
            "alternative_titles":{"en":"One Piece","ja":"ONE PIECE"},
            "num_volumes":0,
            "num_chapters":0
        }}]}"#;
        let out = parse_search_body(body).expect("search parses");
        assert_eq!(out[0].title, "One Piece");
        assert_eq!(out[0].known_volumes, None);
        assert_eq!(out[0].known_chapters, None);
    }

    #[test]
    fn missing_client_id_is_an_actionable_error_not_an_empty_result() {
        let source = MyAnimeList {
            client_id: String::new(),
            on_key_state: None,
        };
        let err = source.search("Berserk", 5).unwrap_err();
        assert!(err.to_string().contains("Client ID"), "{err}");
    }

    #[test]
    fn detail_keys_accept_ids_and_urls() {
        assert_eq!(extract_manga_id("2").as_deref(), Some("2"));
        assert_eq!(
            extract_manga_id("https://myanimelist.net/manga/2/").as_deref(),
            Some("2")
        );
        assert_eq!(extract_manga_id("not-an-id"), None);
    }

    #[test]
    fn malformed_payload_is_a_parse_error() {
        assert!(matches!(
            parse_search_body("[]"),
            Err(FetchError::Parse(_))
        ));
        assert!(matches!(
            parse_detail_body("null"),
            Err(FetchError::Parse(_))
        ));
    }
}
