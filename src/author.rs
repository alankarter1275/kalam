use crate::db::{Annotation, AuthorProfile, AuthorWork, Catalog};
use crate::metadata::{self, CoverRef};
use crate::models::Book;
use crate::paths::authors_dir;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;

#[derive(Debug, Clone)]
#[allow(dead_code)] // payload for the author page's saved-quotes section, not built yet
pub struct AuthorQuote {
    pub book_title: String,
    pub excerpt: String,
    pub created_at: String,
}

#[derive(Debug, Clone)]
pub struct SeriesProgress {
    pub name: String,
    pub owned: Vec<Book>,
    pub missing: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct AuthorSearchResponse {
    #[serde(default)]
    docs: Vec<AuthorSearchDoc>,
}

#[derive(Debug, Deserialize)]
struct AuthorSearchDoc {
    #[serde(default)]
    key: String,
    #[serde(default)]
    name: String,
    #[serde(default)]
    alternate_names: Vec<String>,
    #[serde(default)]
    top_subjects: Vec<String>,
    #[serde(default)]
    top_work: String,
    #[serde(default)]
    birth_date: String,
    #[serde(default)]
    death_date: String,
    #[serde(default)]
    work_count: i64,
}

#[derive(Debug, Deserialize)]
struct AuthorResponse {
    #[serde(default)]
    name: String,
    #[serde(default)]
    personal_name: String,
    #[serde(default)]
    birth_date: String,
    #[serde(default)]
    death_date: String,
    #[serde(default)]
    alternate_names: Vec<String>,
    #[serde(default)]
    bio: Option<OpenLibraryText>,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum OpenLibraryText {
    Text(String),
    Object { value: String },
}

#[derive(Debug, Deserialize)]
struct AuthorWorksResponse {
    #[serde(default)]
    entries: Vec<AuthorWorkDoc>,
}

#[derive(Debug, Deserialize)]
struct AuthorWorkDoc {
    #[serde(default)]
    key: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    subjects: Vec<String>,
    #[serde(default)]
    covers: Vec<i64>,
    #[serde(default)]
    first_publish_year: Option<i64>,
    #[serde(default)]
    first_publish_date: String,
}

#[derive(Debug, Default, Deserialize)]
struct WorkDetailsResponse {
    #[serde(default)]
    series: Vec<WorkSeriesRef>,
}

#[derive(Debug, Default, Deserialize)]
struct WorkSeriesRef {
    #[serde(default)]
    series: WorkKeyRef,
    #[serde(default)]
    position: String,
}

#[derive(Debug, Default, Deserialize)]
struct WorkKeyRef {
    #[serde(default)]
    key: String,
}

#[derive(Debug, Default, Deserialize)]
struct SeriesResponse {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Default, Deserialize)]
struct RatingsResponse {
    #[serde(default)]
    summary: RatingsSummary,
}

#[derive(Debug, Default, Deserialize)]
struct RatingsSummary {
    #[serde(default)]
    average: f32,
    #[serde(default)]
    count: i64,
}

pub fn split_author_names(text: &str) -> Vec<String> {
    let text = collapse_ws(text);
    if text.is_empty() {
        return Vec::new();
    }

    for sep in [";", " & ", " and ", " / ", "\n"] {
        if text.contains(sep) {
            return text
                .split(sep)
                .map(collapse_ws)
                .filter(|part| !part.is_empty())
                .collect();
        }
    }

    vec![text]
}

pub fn display_author_name(name: &str) -> String {
    let name = collapse_ws(name);
    if !looks_like_sort_name(&name) {
        return name;
    }
    let mut parts = name.splitn(2, ',').map(collapse_ws);
    let family = parts.next().unwrap_or_default();
    let rest = parts.next().unwrap_or_default();
    if family.is_empty() || rest.is_empty() {
        name
    } else {
        format!("{rest} {family}")
    }
}

pub fn sort_author_name(name: &str) -> String {
    let name = collapse_ws(name);
    if name.is_empty() || looks_like_sort_name(&name) {
        return name;
    }
    let parts: Vec<_> = name.split_whitespace().collect();
    if parts.len() < 2 {
        return name;
    }
    let family = parts.last().copied().unwrap_or_default();
    let rest = parts[..parts.len() - 1].join(" ");
    format!("{family}, {rest}")
}

pub fn normalize_author_name(name: &str) -> String {
    normalize_bits(&display_author_name(name))
}

pub fn owned_books_for_author(catalog: &Catalog, author_name: &str) -> Vec<Book> {
    catalog.books_for_author(author_name).unwrap_or_default()
}

#[allow(dead_code)] // feeds the future author saved-quotes section
pub fn saved_quotes_for_books(catalog: &Catalog, books: &[Book], limit: usize) -> Vec<AuthorQuote> {
    let mut quotes = Vec::new();
    for book in books {
        if let Ok(rows) = catalog.get_annotations_for_book(book.id) {
            for ann in rows {
                if !is_saved_quote(&ann) {
                    continue;
                }
                quotes.push(AuthorQuote {
                    book_title: book.title.clone(),
                    excerpt: ann.text_excerpt.trim().to_string(),
                    created_at: ann.created_at,
                });
            }
        }
    }
    quotes.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    quotes.truncate(limit);
    quotes
}

pub fn series_progress(books: &[Book]) -> Vec<SeriesProgress> {
    let mut grouped: BTreeMap<String, Vec<Book>> = BTreeMap::new();
    for book in books {
        let Some(series) = book.series.clone() else {
            continue;
        };
        let series = series.trim();
        if series.is_empty() {
            continue;
        }
        grouped
            .entry(series.to_string())
            .or_default()
            .push(book.clone());
    }

    let mut out = Vec::new();
    for (name, mut owned) in grouped {
        owned.sort_by(|a, b| {
            a.series_index
                .partial_cmp(&b.series_index)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.title.cmp(&b.title))
        });
        out.push(SeriesProgress {
            missing: missing_series_slots(&owned),
            name,
            owned,
        });
    }
    out
}

pub fn fetch_and_cache_author(
    catalog: &Catalog,
    requested_name: &str,
    owned_books: &[Book],
) -> Result<AuthorProfile, String> {
    let requested_display = display_author_name(requested_name);
    let requested_key = normalize_author_name(&requested_display);
    if requested_key.is_empty() {
        return Err("This author name is empty.".into());
    }

    let existing = catalog
        .get_author_profile_by_name(requested_name)
        .map_err(|e| e.to_string())?;

    let doc = match search_author(&requested_display, owned_books) {
        Ok(Some(doc)) => doc,
        Ok(None) => {
            if let Some(profile) = existing {
                return Ok(profile);
            }
            return Err(format!("Could not find online info for {}.", requested_display));
        }
        Err(err) => {
            if let Some(profile) = existing {
                return Ok(profile);
            }
            return Err(err);
        }
    };

    let mut profile = match fetch_author_profile(catalog, &doc, existing.as_ref()) {
        Ok(prof) => prof,
        Err(err) => {
            if let Some(existing_prof) = existing {
                return Ok(existing_prof);
            }
            return Err(err);
        }
    };
    profile.normalized_name = requested_key;
    if profile.canonical_name.trim().is_empty() {
        profile.canonical_name = requested_display.clone();
    }
    if profile.sort_name.trim().is_empty() {
        profile.sort_name = sort_author_name(&profile.canonical_name);
    }

    let mut aliases = profile.aliases.clone();
    aliases.push(requested_name.to_string());
    aliases.push(requested_display);
    if let Some(current) = existing.as_ref() {
        aliases.extend(current.aliases.iter().cloned());
        if profile.photo_file.is_none() {
            profile.photo_file = current.photo_file.clone();
            profile.photo_path = current.photo_path.clone();
        }
        if profile.bio.trim().is_empty() && !current.bio.trim().is_empty() {
            profile.bio = current.bio.clone();
        }
    }
    profile.aliases = dedup_names(aliases);

    if let Ok(Some(photo_file)) = fetch_author_photo(&profile.openlibrary_key) {
        profile.photo_file = Some(photo_file.clone());
        profile.photo_path = Some(authors_dir().join(photo_file));
    }

    catalog
        .upsert_author_profile(&profile)
        .map_err(|e| e.to_string())?;
    catalog
        .get_author_profile_by_name(requested_name)
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "Saved the author info, but could not load it back.".into())
}

pub fn works_not_in_library(profile: &AuthorProfile, owned_books: &[Book]) -> Vec<AuthorWork> {
    let owned_titles: HashSet<String> = owned_books
        .iter()
        .map(|book| normalize_title(&book.title))
        .collect();
    let mut works = profile
        .works
        .iter()
        .filter(|work| !work.title.trim().is_empty())
        .filter(|work| !owned_titles.contains(&normalize_title(&work.title)))
        .cloned()
        .collect::<Vec<_>>();
    works.sort_by(|a, b| {
        a.first_publish_year
            .unwrap_or(i64::MAX)
            .cmp(&b.first_publish_year.unwrap_or(i64::MAX))
            .then_with(|| a.title.cmp(&b.title))
    });
    works
}

pub fn initials(name: &str) -> String {
    let display = display_author_name(name);
    let mut out = String::new();
    for part in display.split_whitespace().take(2) {
        if let Some(ch) = part.chars().find(|c| c.is_alphabetic()) {
            out.push(ch.to_ascii_uppercase());
        }
    }
    if out.is_empty() {
        "?".into()
    } else {
        out
    }
}

#[allow(dead_code)] // helper for that saved-quotes section
pub fn line_text(parts: &[String]) -> String {
    parts
        .iter()
        .filter(|part| !part.trim().is_empty())
        .cloned()
        .collect::<Vec<_>>()
        .join(" · ")
}

pub fn status_counts(books: &[Book]) -> (usize, usize, usize) {
    let finished = books.iter().filter(|book| book.progress >= 100).count();
    let reading = books
        .iter()
        .filter(|book| book.progress > 0 && book.progress < 100)
        .count();
    (books.len(), finished, reading)
}

#[allow(dead_code)] // as above
fn is_saved_quote(ann: &Annotation) -> bool {
    matches!(ann.kind.as_str(), "quote" | "highlight") && !ann.text_excerpt.trim().is_empty()
}

fn missing_series_slots(books: &[Book]) -> Vec<String> {
    let mut nums: Vec<i32> = books
        .iter()
        .filter_map(|book| {
            let idx = book.series_index;
            if idx > 0.0 && idx.fract().abs() < f32::EPSILON {
                Some(idx.round() as i32)
            } else {
                None
            }
        })
        .collect();
    nums.sort_unstable();
    nums.dedup();

    if nums.is_empty() {
        return Vec::new();
    }

    let mut out = Vec::new();
    if let Some(first) = nums.first().copied() {
        for n in 1..first {
            out.push(format!("Missing #{n}"));
        }
    }
    for pair in nums.windows(2) {
        let a = pair[0];
        let b = pair[1];
        if b - a > 1 {
            for n in (a + 1)..b {
                out.push(format!("Missing #{n}"));
            }
        }
    }
    out
}

fn search_author(name: &str, owned_books: &[Book]) -> Result<Option<AuthorSearchDoc>, String> {
    let body = metadata::agent()
        .get("https://openlibrary.org/search/authors.json")
        .query("q", name)
        .query("limit", "12")
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;

    let parsed: AuthorSearchResponse = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    Ok(pick_best_author(parsed.docs, name, owned_books))
}

fn pick_best_author(
    docs: Vec<AuthorSearchDoc>,
    requested_name: &str,
    owned_books: &[Book],
) -> Option<AuthorSearchDoc> {
    let target = normalize_author_name(requested_name);
    let owned_titles: HashSet<String> = owned_books
        .iter()
        .map(|book| normalize_title(&book.title))
        .collect();

    docs.into_iter().max_by_key(|doc| {
        let mut score = 0_i64;
        if normalize_author_name(&doc.name) == target {
            score += 500;
        }
        if doc
            .alternate_names
            .iter()
            .any(|name| normalize_author_name(name) == target)
        {
            score += 380;
        }
        if !doc.top_work.trim().is_empty() && owned_titles.contains(&normalize_title(&doc.top_work))
        {
            score += 260;
        }
        score += doc.work_count.min(60);
        if !doc.birth_date.trim().is_empty() {
            score += 10;
        }
        if !doc.key.trim().is_empty() {
            score += 10;
        }
        score
    })
}

fn fetch_author_profile(
    catalog: &Catalog,
    doc: &AuthorSearchDoc,
    existing: Option<&AuthorProfile>,
) -> Result<AuthorProfile, String> {
    let author_key = author_path(&doc.key);
    let body = metadata::agent()
        .get(&format!("https://openlibrary.org{author_key}.json"))
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    let parsed: AuthorResponse = serde_json::from_str(&body).map_err(|e| e.to_string())?;

    let works_body = metadata::agent()
        .get(&format!("https://openlibrary.org{author_key}/works.json"))
        .query("limit", "40")
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    let works_parsed: AuthorWorksResponse =
        serde_json::from_str(&works_body).map_err(|e| e.to_string())?;

    let canonical_name = if !parsed.name.trim().is_empty() {
        parsed.name.trim().to_string()
    } else if !parsed.personal_name.trim().is_empty() {
        parsed.personal_name.trim().to_string()
    } else {
        display_author_name(&doc.name)
    };
    let bio = match parsed.bio {
        Some(OpenLibraryText::Text(text)) => text,
        Some(OpenLibraryText::Object { value }) => value,
        None => String::new(),
    };

    let existing_works_by_key: HashMap<String, AuthorWork> = existing
        .map(|p| {
            p.works
                .iter()
                .filter(|w| !w.work_key.trim().is_empty())
                .map(|w| (w.work_key.trim().to_string(), w.clone()))
                .collect()
        })
        .unwrap_or_default();
    let existing_works_by_title: HashMap<String, AuthorWork> = existing
        .map(|p| {
            p.works
                .iter()
                .filter(|w| !w.title.trim().is_empty())
                .map(|w| (normalize_title(&w.title), w.clone()))
                .collect()
        })
        .unwrap_or_default();

    let mut works = works_parsed
        .entries
        .into_iter()
        .filter(|entry| !entry.title.trim().is_empty())
        .map(|entry| {
            let key = entry.key.trim();
            let title = entry.title.trim();
            let prev = existing_works_by_key
                .get(key)
                .cloned()
                .or_else(|| existing_works_by_title.get(&normalize_title(title)).cloned());

            AuthorWork {
                title: title.to_string(),
                first_publish_year: entry
                    .first_publish_year
                    .or_else(|| extract_year(&entry.first_publish_date))
                    .or_else(|| prev.as_ref().and_then(|p| p.first_publish_year)),
                subjects: entry
                    .subjects
                    .into_iter()
                    .filter(|subject| subject.len() <= 32)
                    .take(4)
                    .collect(),
                cover_id: entry.covers.into_iter().next().or_else(|| prev.as_ref().and_then(|p| p.cover_id)),
                cover_file: prev.as_ref().and_then(|p| p.cover_file.clone()),
                work_key: entry.key,
                series_key: prev.as_ref().map(|p| p.series_key.clone()).unwrap_or_default(),
                series_name: prev.as_ref().map(|p| p.series_name.clone()).unwrap_or_default(),
                series_position: prev.as_ref().map(|p| p.series_position.clone()).unwrap_or_default(),
                rating_average: prev.as_ref().and_then(|p| p.rating_average),
                rating_count: prev.as_ref().map(|p| p.rating_count).unwrap_or(0),
            }
        })
        .collect::<Vec<_>>();

    let mut series_names = HashMap::new();
    let mut remote_budget = 5usize;
    let mut rate_limited = false;
    for work in &mut works {
        enrich_author_work(
            catalog,
            work,
            &mut series_names,
            &mut remote_budget,
            &mut rate_limited,
        );
    }

    let mut aliases = parsed.alternate_names;
    aliases.push(canonical_name.clone());
    aliases.push(sort_author_name(&canonical_name));
    aliases.push(display_author_name(&doc.name));
    let top_subjects = doc.top_subjects.iter().take(8).cloned().collect();
    let source_url = format!("https://openlibrary.org{author_key}");

    Ok(AuthorProfile {
        id: 0,
        canonical_name: canonical_name.clone(),
        sort_name: sort_author_name(&canonical_name),
        normalized_name: normalize_author_name(&canonical_name),
        bio: bio.trim().to_string(),
        birth_date: if parsed.birth_date.trim().is_empty() {
            doc.birth_date.trim().to_string()
        } else {
            parsed.birth_date.trim().to_string()
        },
        death_date: if parsed.death_date.trim().is_empty() {
            doc.death_date.trim().to_string()
        } else {
            parsed.death_date.trim().to_string()
        },
        top_work: doc.top_work.trim().to_string(),
        top_subjects,
        openlibrary_key: author_key,
        photo_file: None,
        photo_path: None,
        work_count: doc.work_count.max(works.len() as i64),
        works,
        aliases: dedup_names(aliases),
        fetched_at: String::new(),
        source_url,
    })
}

fn enrich_author_work(
    catalog: &Catalog,
    work: &mut AuthorWork,
    series_names: &mut HashMap<String, String>,
    remote_budget: &mut usize,
    rate_limited: &mut bool,
) {
    if let Some(cover_id) = work.cover_id {
        if work.cover_file.is_none() {
            if let Ok(photo_file) = fetch_work_cover(&work.work_key, cover_id, rate_limited) {
                work.cover_file = photo_file;
            }
        }
    }

    if work.series_name.trim().is_empty() {
        if let Some(name) = series_name_from_subjects(&work.subjects) {
            work.series_name = name;
        }
    }

    if work.series_name.trim().is_empty() && !work.series_key.trim().is_empty() {
        let name = load_series_name(catalog, series_names, &work.series_key, rate_limited);
        if !name.is_empty() {
            work.series_name = name;
        }
    }

    if !work.work_key.trim().is_empty() && !*rate_limited && *remote_budget > 0 {
        if work.series_name.trim().is_empty() {
            *remote_budget = remote_budget.saturating_sub(1);
            match fetch_work_details(&work.work_key) {
                Ok(details) => {
                    if let Some(series) = details.series.into_iter().next() {
                        let series_key = series_path(&series.series.key);
                        if !series_key.is_empty() {
                            work.series_key = series_key.clone();
                            work.series_position = collapse_ws(&series.position);
                            let name = load_series_name(catalog, series_names, &series_key, rate_limited);
                            if !name.is_empty() {
                                work.series_name = name;
                            }
                        }
                    }
                }
                Err(err) => {
                    if err.contains("429") || err.contains("Limited") || err.contains("Too Many Requests") {
                        *rate_limited = true;
                    }
                }
            }
        }

        if !*rate_limited && work.rating_average.is_none() {
            match fetch_work_rating(&work.work_key) {
                Ok((average, count)) => {
                    if count > 0 {
                        work.rating_average = Some(average.clamp(0.0, 5.0));
                        work.rating_count = count;
                    }
                }
                Err(err) => {
                    if err.contains("429") || err.contains("Limited") || err.contains("Too Many Requests") {
                        *rate_limited = true;
                    }
                }
            }
        }
    }
}

fn fetch_work_details(work_key: &str) -> Result<WorkDetailsResponse, String> {
    let body = metadata::agent()
        .get(&format!(
            "https://openlibrary.org{}.json",
            work_path(work_key)
        ))
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    serde_json::from_str(&body).map_err(|e| e.to_string())
}

fn fetch_work_rating(work_key: &str) -> Result<(f32, i64), String> {
    let body = metadata::agent()
        .get(&format!(
            "https://openlibrary.org{}/ratings.json",
            work_path(work_key)
        ))
        .call()
        .map_err(|e| e.to_string())?
        .into_string()
        .map_err(|e| e.to_string())?;
    let parsed: RatingsResponse = serde_json::from_str(&body).map_err(|e| e.to_string())?;
    Ok((parsed.summary.average, parsed.summary.count))
}

fn fetch_work_cover(
    work_key: &str,
    cover_id: i64,
    rate_limited: &mut bool,
) -> Result<Option<String>, String> {
    if cover_id <= 0 || *rate_limited {
        return Ok(None);
    }
    let stem = work_key.rsplit('/').next().unwrap_or("work").trim();
    let file_name = format!("{stem}-{cover_id}.jpg");
    let path = authors_dir().join(&file_name);
    if path.is_file() {
        return Ok(Some(file_name));
    }

    let bytes = match metadata::fetch_cover(&CoverRef::OpenLibraryId(cover_id)) {
        Ok(bytes) if !bytes.is_empty() => bytes,
        Ok(_) => return Ok(None),
        Err(err) => {
            let err_str = err.to_string();
            if err_str.contains("429") || err_str.contains("Limited") || err_str.contains("Too Many Requests") {
                *rate_limited = true;
            }
            return Ok(None);
        }
    };
    fs::create_dir_all(authors_dir()).map_err(|e| e.to_string())?;
    fs::write(path, bytes).map_err(|e| e.to_string())?;
    Ok(Some(file_name))
}

fn load_series_name(
    catalog: &Catalog,
    cache: &mut HashMap<String, String>,
    series_key: &str,
    rate_limited: &mut bool,
) -> String {
    if let Some(name) = cache.get(series_key) {
        return name.clone();
    }

    if let Ok(Some(entry)) = catalog.get_cached_series(series_key) {
        if let Some(first_work) = entry.works.first() {
            if !first_work.title.trim().is_empty() {
                let name = collapse_ws(&first_work.title);
                cache.insert(series_key.to_string(), name.clone());
                return name;
            }
        }
    }

    if *rate_limited {
        return String::new();
    }

    let result = metadata::agent()
        .get(&format!(
            "https://openlibrary.org{}.json",
            series_path(series_key)
        ))
        .call()
        .map_err(|e| e.to_string())
        .and_then(|resp| resp.into_string().map_err(|e| e.to_string()))
        .and_then(|body| serde_json::from_str::<SeriesResponse>(&body).map_err(|e| e.to_string()));

    let name = match result {
        Ok(series) => collapse_ws(&series.name),
        Err(err) => {
            if err.contains("429") || err.contains("Limited") || err.contains("Too Many Requests") {
                *rate_limited = true;
            }
            String::new()
        }
    };

    if !name.is_empty() {
        cache.insert(series_key.to_string(), name.clone());
    }
    name
}

fn series_name_from_subjects(subjects: &[String]) -> Option<String> {
    subjects
        .iter()
        .find_map(|subject| subject.strip_prefix("series:"))
        .map(|name| collapse_ws(&name.replace('_', " ")))
        .filter(|name| !name.is_empty())
}

fn fetch_author_photo(author_key: &str) -> Result<Option<String>, String> {
    let olid = author_key.rsplit('/').next().unwrap_or(author_key).trim();
    if olid.is_empty() {
        return Ok(None);
    }
    let file_name = format!("{olid}.jpg");
    let path = authors_dir().join(&file_name);
    if path.is_file() {
        return Ok(Some(file_name));
    }
    let cover = CoverRef::Url(format!(
        "https://covers.openlibrary.org/a/olid/{olid}-L.jpg"
    ));
    let bytes = match metadata::fetch_cover(&cover) {
        Ok(bytes) if !bytes.is_empty() => bytes,
        Ok(_) => return Ok(None),
        Err(_) => return Ok(None),
    };
    fs::create_dir_all(authors_dir()).map_err(|e| e.to_string())?;
    fs::write(&path, bytes).map_err(|e| e.to_string())?;
    Ok(Some(file_name))
}

fn author_path(key: &str) -> String {
    let key = key.trim();
    if key.starts_with("/authors/") {
        key.to_string()
    } else if key.starts_with("OL") {
        format!("/authors/{key}")
    } else {
        format!("/authors/{}", key.trim_start_matches('/'))
    }
}

fn work_path(key: &str) -> String {
    let key = key.trim();
    if key.starts_with("/works/") {
        key.to_string()
    } else if key.starts_with("OL") {
        format!("/works/{key}")
    } else {
        format!("/works/{}", key.trim_start_matches('/'))
    }
}

fn series_path(key: &str) -> String {
    let key = key.trim();
    if key.starts_with("/series/") {
        key.to_string()
    } else if key.starts_with("OL") {
        format!("/series/{key}")
    } else {
        format!("/series/{}", key.trim_start_matches('/'))
    }
}

fn dedup_names(names: Vec<String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for name in names {
        let display = display_author_name(&name);
        let key = normalize_author_name(&display);
        if key.is_empty() || !seen.insert(key) {
            continue;
        }
        out.push(display);
    }
    out
}

fn looks_like_sort_name(name: &str) -> bool {
    let mut parts = name
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty());
    let Some(first) = parts.next() else {
        return false;
    };
    let Some(second) = parts.next() else {
        return false;
    };
    parts.next().is_none()
        && !first.is_empty()
        && !second.is_empty()
        && second.split_whitespace().count() <= 6
}

fn extract_year(text: &str) -> Option<i64> {
    let digits = text
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.len() < 4 {
        None
    } else {
        digits[..4].parse().ok()
    }
}

fn collapse_ws(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_bits(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

fn normalize_title(title: &str) -> String {
    normalize_bits(title)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_sort_name_to_same_key() {
        assert_eq!(
            normalize_author_name("Martin, George R. R."),
            normalize_author_name("George R. R. Martin")
        );
    }

    #[test]
    fn splits_common_multi_author_formats() {
        assert_eq!(
            split_author_names("Alice Smith & Bob Jones"),
            vec!["Alice Smith", "Bob Jones"]
        );
        assert_eq!(
            split_author_names("Alice Smith; Bob Jones"),
            vec!["Alice Smith", "Bob Jones"]
        );
    }
}
