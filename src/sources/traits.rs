use std::collections::HashMap;

/// What a chapter contains.
/// This enum is the crucial seam that unifies Manga (P9) and Fiction (P7).
#[derive(Debug, Clone)]
pub enum ChapterContent {
    /// Manga returns a list of image URLs to stream.
    #[allow(dead_code)]
    Images(Vec<String>),
    /// Fiction returns an HTML string.
    #[allow(dead_code)]
    Html(String),
}

/// Metadata for a remote book discovered via search or browse.
#[derive(Debug, Clone, Default)]
pub struct RemoteBookCard {
    pub remote_id: String,
    pub title: String,
    pub author: String,
    pub cover_url: Option<String>,
}

/// Detailed metadata for a remote book.
#[derive(Debug, Clone, Default)]
pub struct RemoteBookDetails {
    pub remote_id: String,
    pub title: String,
    pub author: String,
    pub description: String,
    pub cover_url: Option<String>,
    #[allow(dead_code)]
    pub tags: Vec<String>,
    pub status: String,
}

/// Metadata for a remote chapter.
#[derive(Debug, Clone, Default)]
pub struct RemoteChapter {
    pub chapter_id: String,
    pub title: String,
    pub number: f32, // Float to support 12.5 etc.
    pub volume: Option<f32>,
    pub url: Option<String>,
}

/// Dynamic filter type definition for Tachiyomi-style UI generation.
#[derive(Debug, Clone)]
pub enum FilterType {
    #[allow(dead_code)]
    Text { placeholder: String },
    #[allow(dead_code)]
    Checkbox,
    #[allow(dead_code)]
    Select { options: Vec<(String, String)> },
    #[allow(dead_code)]
    Sort { options: Vec<(String, String)> },
}

/// A dynamic filter metadata struct supplied by a Source.
#[derive(Debug, Clone)]
pub struct FilterDefinition {
    pub id: String,
    pub name: String,
    pub filter_type: FilterType,
    pub default_value: String,
}

/// A paginated page of search results.
#[derive(Debug, Clone)]
pub struct SearchPage {
    pub results: Vec<RemoteBookCard>,
    pub has_more: bool,
}

/// The unified Source trait.
/// All methods are synchronous; they should be called from `crate::tasks::spawn` workers.
pub trait Source: Send + Sync {
    /// Internal unique ID of this source (e.g., "mangadex").
    fn id(&self) -> &str;
    
    /// Human-readable name of this source.
    #[allow(dead_code)]
    fn name(&self) -> &str;
    
    /// Base URL for this source, if applicable.
    #[allow(dead_code)]
    // Will be utilized by Wasm plugins in Phase 4.
    fn base_url(&self) -> &str;

    /// Declare available dynamic search filters.
    fn get_filter_definitions(&self) -> Vec<FilterDefinition> {
        Vec::new()
    }

    /// Search for books.
    fn search(
        &self,
        query: &str,
        page: u32,
        filters: &HashMap<String, String>,
    ) -> anyhow::Result<SearchPage>;

    /// Fetch detailed metadata for a book.
    fn get_details(&self, remote_id: &str) -> anyhow::Result<RemoteBookDetails>;

    /// Fetch the list of chapters for a book.
    fn get_chapters(&self, remote_id: &str) -> anyhow::Result<Vec<RemoteChapter>>;

    /// Fetch the content of a specific chapter.
    fn get_chapter_content(&self, chapter_id: &str) -> anyhow::Result<ChapterContent>;

    /// Fetch raw bytes for a specific URL, optionally injecting referer/auth headers.
    /// This is used by the ComicsReader's ImageProvider to stream images.
    fn fetch_image(&self, url: &str) -> anyhow::Result<Vec<u8>>;
}
