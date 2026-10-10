//! P5 — pluggable metadata sources.
//!
//! Every provider implements [`MetadataSource`] and returns the same
//! [`Candidate`] shape, so the edit dialog can merge results from several
//! services into one list without knowing who produced what.
//!
//! Sources are synchronous by design: callers run them on worker threads and
//! post results back to the GTK main context. Nothing here touches the catalog
//! — a lookup only ever *offers* values, which the user then reviews.

pub mod google_books;
pub mod openlibrary;
pub mod series;

use std::time::Duration;

/// Sent on every request; Open Library asks that clients identify themselves.
pub const USER_AGENT: &str = concat!(
    "Kalam/",
    env!("CARGO_PKG_VERSION"),
    " (personal ebook manager; +https://github.com/alankarter1275/calibre-alt)"
);

/// Network calls are best-effort; the UI shows the message and moves on.
#[derive(Debug)]
pub enum FetchError {
    Network(String),
    Parse(String),
    /// Provider refused us — quota, key or rate limit. Worth wording kindly.
    Limited(String),
}

impl std::fmt::Display for FetchError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FetchError::Network(m) => write!(f, "{}", humanise_network(m)),
            FetchError::Parse(m) => write!(f, "Could not read the response: {m}"),
            FetchError::Limited(m) => write!(f, "{m}"),
        }
    }
}

/// ureq embeds the full request URL and a DNS trace in its error text, which
/// is unreadable in a narrow panel. Collapse the common cases to one line.
fn humanise_network(raw: &str) -> String {
    let lower = raw.to_lowercase();
    if lower.contains("name resolution") || lower.contains("dns") {
        return "no internet connection (DNS lookup failed).".into();
    }
    if lower.contains("timed out") || lower.contains("timeout") {
        return "the request timed out.".into();
    }
    if lower.contains("connection refused") || lower.contains("connect") {
        return "could not reach the server.".into();
    }
    if lower.contains("certificate") || lower.contains("tls") {
        return "the secure connection failed.".into();
    }
    // Unknown shape: keep it, but never let a URL run away with the layout.
    let trimmed: String = raw.chars().take(110).collect();
    if raw.chars().count() > 110 {
        format!("{trimmed}…")
    } else {
        trimmed
    }
}

/// Which service a candidate came from, so results can be badged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceId {
    OpenLibrary,
    GoogleBooks,
}

impl SourceId {
    pub const ALL: &'static [SourceId] = &[SourceId::OpenLibrary, SourceId::GoogleBooks];

    /// Stable key for preferences.
    pub fn key(self) -> &'static str {
        match self {
            SourceId::OpenLibrary => "openlibrary",
            SourceId::GoogleBooks => "googlebooks",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            SourceId::OpenLibrary => "Open Library",
            SourceId::GoogleBooks => "Google Books",
        }
    }

    /// Short badge shown beside each result row.
    pub fn badge(self) -> &'static str {
        match self {
            SourceId::OpenLibrary => "OL",
            SourceId::GoogleBooks => "GB",
        }
    }

    pub fn css_class(self) -> &'static str {
        match self {
            SourceId::OpenLibrary => "kalam-badge-ol",
            SourceId::GoogleBooks => "kalam-badge-gb",
        }
    }
}

/// Where a cover can be fetched from. Providers differ: Open Library serves
/// covers by numeric id, Google Books hands back a direct URL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoverRef {
    OpenLibraryId(i64),
    Url(String),
}

/// One candidate result, flattened into just what the edit dialog needs.
#[derive(Debug, Clone, Default)]
pub struct Candidate {
    pub title: String,
    pub authors: String,
    pub series: Option<String>,
    pub tags: Vec<String>,
    pub first_year: Option<i64>,
    pub publisher: String,
    /// Full publication date when the provider has one, else the year.
    pub published: String,
    pub cover: Option<CoverRef>,
    /// Some providers return the description inline; others need a second
    /// request keyed by this handle.
    pub description: String,
    pub detail_key: Option<String>,
    pub source: Option<SourceId>,
}

impl Candidate {
    /// One-line summary for the results list.
    pub fn summary(&self) -> String {
        let mut parts = Vec::new();
        if !self.authors.is_empty() {
            parts.push(self.authors.clone());
        }
        if !self.published.is_empty() {
            parts.push(self.published.clone());
        } else if let Some(year) = self.first_year {
            parts.push(year.to_string());
        }
        if !self.publisher.is_empty() {
            parts.push(self.publisher.clone());
        }
        if self.cover.is_some() {
            parts.push("has cover".into());
        }
        parts.join(" · ")
    }

    /// Key used to spot the same book arriving from two providers.
    fn dedup_key(&self) -> String {
        let title = self
            .title
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>();
        // First author only: providers disagree on ordering and on how many
        // contributors to list.
        let author = self
            .authors
            .split(',')
            .next()
            .unwrap_or_default()
            .to_lowercase()
            .chars()
            .filter(|c| c.is_alphanumeric())
            .collect::<String>();
        format!("{title}|{author}")
    }

    /// How complete this candidate is, used to pick a winner when two
    /// providers return the same book.
    fn richness(&self) -> u32 {
        let mut score = 0;
        if self.cover.is_some() {
            score += 3;
        }
        if !self.description.trim().is_empty() || self.detail_key.is_some() {
            score += 2;
        }
        if !self.publisher.is_empty() {
            score += 1;
        }
        if !self.published.is_empty() {
            score += 1;
        }
        if !self.tags.is_empty() {
            score += 1;
        }
        if self.series.is_some() {
            score += 1;
        }
        score
    }
}

/// A metadata provider.
pub trait MetadataSource: Send + Sync {
    fn id(&self) -> SourceId;

    /// Free-text search, usually "title author".
    fn search(&self, query: &str, limit: usize) -> Result<Vec<Candidate>, FetchError>;

    /// Fetch a long description for a candidate, if the provider needs a
    /// second call. Default: nothing more to fetch.
    fn fetch_description(&self, _detail_key: &str) -> Result<String, FetchError> {
        Ok(String::new())
    }
}

/// Build the provider list, honouring which sources are enabled.
pub fn enabled_sources(catalog: &crate::db::Catalog) -> Vec<Box<dyn MetadataSource>> {
    let mut out: Vec<Box<dyn MetadataSource>> = Vec::new();
    for id in SourceId::ALL {
        if !source_enabled(catalog, *id) {
            continue;
        }
        match id {
            SourceId::OpenLibrary => out.push(Box::new(openlibrary::OpenLibrary)),
            SourceId::GoogleBooks => out.push(Box::new(google_books::GoogleBooks {
                api_key: catalog.get_pref("meta.googlebooks.key").unwrap_or_default(),
                country: catalog
                    .get_pref("meta.googlebooks.country")
                    .filter(|c| !c.trim().is_empty())
                    .unwrap_or_else(google_books::detect_country),
            })),
        }
    }
    out
}

/// Sources are on unless explicitly turned off.
pub fn source_enabled(catalog: &crate::db::Catalog, id: SourceId) -> bool {
    catalog
        .get_pref(&format!("meta.{}.enabled", id.key()))
        .map(|v| v != "0")
        .unwrap_or(true)
}

pub fn set_source_enabled(catalog: &crate::db::Catalog, id: SourceId, on: bool) {
    catalog.set_pref(
        &format!("meta.{}.enabled", id.key()),
        if on { "1" } else { "0" },
    );
}

/// Query every enabled source in parallel and merge the results.
///
/// Returns the merged list plus any per-source failures, so the UI can say
/// "Google Books is rate limited" while still showing Open Library's hits.
pub fn search_all(
    sources: Vec<Box<dyn MetadataSource>>,
    query: &str,
    limit: usize,
) -> (Vec<Candidate>, Vec<(SourceId, String)>) {
    if sources.is_empty() {
        return (Vec::new(), Vec::new());
    }

    // One thread per source: two sequential HTTP round trips would otherwise
    // double the wait for no reason.
    //
    // Roadmap 1.3: this stays a parallel map rather than becoming a
    // `tasks::spawn` task, deliberately. It is called synchronously and its
    // whole contract is *returning* the merged results, so turning it into a
    // background task would change what it means. What it does need is the
    // same honesty about failure as everywhere else.
    let mut handles = Vec::new();
    for source in sources {
        let query = query.to_string();
        // Take the id before spawning. If the worker panics, `join` hands
        // back nothing but the panic payload and the provider's name would be
        // lost with it — and the UI is supposed to say *which* service failed.
        let id = source.id();
        handles.push((id, std::thread::spawn(move || source.search(&query, limit))));
    }

    let mut all = Vec::new();
    let mut errors = Vec::new();
    for (id, handle) in handles {
        match handle.join() {
            Ok(Ok(list)) => all.extend(list),
            Ok(Err(err)) => errors.push((id, err.to_string())),
            // A panic, not a failed lookup — reported rather than dropped.
            // Silently swallowing it makes a crashed provider look like a
            // provider that found nothing, and "Open Library had no match" is
            // a different claim from "Open Library crashed".
            Err(_) => errors.push((id, "the lookup thread panicked".to_string())),
        }
    }

    (merge(all), errors)
}

/// Collapse duplicates, keeping the richest version of each book.
///
/// Ordering is by richness so the most complete match is easiest to reach,
/// which matters more than preserving provider order.
fn merge(mut candidates: Vec<Candidate>) -> Vec<Candidate> {
    // Descending richness: sort_by_key on Reverse avoids the manual cmp.
    candidates.sort_by_key(|c| std::cmp::Reverse(c.richness()));

    let mut seen: Vec<String> = Vec::new();
    let mut out = Vec::new();
    for candidate in candidates {
        let key = candidate.dedup_key();
        // A title with no author is too weak a key to dedup on — two unrelated
        // authorless results would otherwise collapse into one.
        let has_author = !key.ends_with('|');
        if has_author {
            if seen.contains(&key) {
                continue;
            }
            seen.push(key);
        }
        out.push(candidate);
    }
    out
}

pub(crate) fn agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_connect(Duration::from_secs(10))
        .timeout_read(Duration::from_secs(20))
        .user_agent(USER_AGENT)
        .build()
}

/// Download cover bytes for a candidate.
pub fn fetch_cover(cover: &CoverRef) -> Result<Vec<u8>, FetchError> {
    let url = match cover {
        CoverRef::OpenLibraryId(id) => format!("https://covers.openlibrary.org/b/id/{id}-L.jpg"),
        CoverRef::Url(url) => url.clone(),
    };
    download_image(&url)
}

/// Small thumbnail for the preview grid, when the provider offers one.
pub fn fetch_thumbnail(cover: &CoverRef) -> Result<Vec<u8>, FetchError> {
    let url = match cover {
        CoverRef::OpenLibraryId(id) => format!("https://covers.openlibrary.org/b/id/{id}-M.jpg"),
        CoverRef::Url(url) => url.clone(),
    };
    download_image(&url)
}

fn download_image(url: &str) -> Result<Vec<u8>, FetchError> {
    use std::io::Read;

    let resp = agent()
        .get(url)
        .call()
        .map_err(|e| FetchError::Network(e.to_string()))?;

    let mut bytes = Vec::new();
    resp.into_reader()
        .take(8 * 1024 * 1024)
        .read_to_end(&mut bytes)
        .map_err(|e| FetchError::Network(e.to_string()))?;

    if bytes.len() < 512 {
        // Open Library serves a 1x1 placeholder when a cover is missing.
        return Err(FetchError::Network("no cover available".into()));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn candidate(title: &str, author: &str, source: SourceId) -> Candidate {
        Candidate {
            title: title.into(),
            authors: author.into(),
            source: Some(source),
            ..Default::default()
        }
    }

    #[test]
    fn duplicates_across_sources_collapse() {
        let mut a = candidate("Dune", "Frank Herbert", SourceId::OpenLibrary);
        let mut b = candidate("dune", "Frank Herbert", SourceId::GoogleBooks);
        // Make the Google copy richer so it should win.
        b.cover = Some(CoverRef::Url("http://x/c.jpg".into()));
        b.publisher = "Ace".into();
        a.tags = vec!["scifi".into()];

        let merged = merge(vec![a, b]);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].source, Some(SourceId::GoogleBooks));
    }

    #[test]
    fn different_books_are_kept() {
        let merged = merge(vec![
            candidate("Dune", "Frank Herbert", SourceId::OpenLibrary),
            candidate("Emma", "Jane Austen", SourceId::GoogleBooks),
        ]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn punctuation_and_case_do_not_defeat_dedup() {
        let merged = merge(vec![
            candidate("The Hobbit", "J.R.R. Tolkien", SourceId::OpenLibrary),
            candidate("the hobbit!", "JRR Tolkien", SourceId::GoogleBooks),
        ]);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn authorless_results_are_never_merged_together() {
        // Two unrelated books with no author must not collapse into one.
        let merged = merge(vec![
            candidate("Untitled", "", SourceId::OpenLibrary),
            candidate("Something Else", "", SourceId::GoogleBooks),
        ]);
        assert_eq!(merged.len(), 2);
    }

    #[test]
    fn richness_prefers_covers_and_descriptions() {
        let bare = candidate("A", "B", SourceId::OpenLibrary);
        let mut rich = candidate("A", "B", SourceId::GoogleBooks);
        rich.cover = Some(CoverRef::OpenLibraryId(1));
        rich.description = "text".into();
        assert!(rich.richness() > bare.richness());
    }

    #[test]
    fn network_errors_are_collapsed_to_one_line() {
        // The real message ureq produced in the field, URL and all.
        let raw = "https://openlibrary.org/search.json?q=Nyxia%3A+The+Nyxia+Triad&fields=title \
                   Dns Failed: resolve dns name 'openlibrary.org:443': failed to lookup address \
                   information: Temporary failure in name resolution";
        let shown = FetchError::Network(raw.into()).to_string();
        assert!(shown.contains("no internet"), "{shown}");
        assert!(!shown.contains("openlibrary.org/search.json"), "{shown}");
        assert!(shown.chars().count() < 80, "too long: {shown}");
    }

    #[test]
    fn unknown_network_errors_are_still_truncated() {
        let raw = "x".repeat(500);
        let shown = FetchError::Network(raw).to_string();
        assert!(shown.chars().count() <= 111, "{}", shown.chars().count());
    }

    #[test]
    fn timeouts_and_refusals_get_their_own_wording() {
        assert!(FetchError::Network("operation timed out".into())
            .to_string()
            .contains("timed out"));
        assert!(FetchError::Network("connection refused".into())
            .to_string()
            .contains("could not reach"));
    }

    #[test]
    fn empty_source_list_yields_nothing() {
        let (found, errors) = search_all(Vec::new(), "dune", 5);
        assert!(found.is_empty());
        assert!(errors.is_empty());
    }
}
