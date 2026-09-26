use super::provider::ImageProvider;
use crate::comics::{extract_comic_page, list_comic_pages};
use crate::sources::{ChapterContent, SourceManager};
use anyhow::{anyhow, Result};
use std::path::PathBuf;
use std::sync::Arc;

pub struct LocalProvider {
    file_path: PathBuf,
    pages: Vec<String>,
}

impl LocalProvider {
    pub fn new(file_path: PathBuf) -> Result<Self> {
        let pages = list_comic_pages(&file_path).unwrap_or_default();
        if pages.is_empty() {
            return Err(anyhow!("No pages found in local comic"));
        }
        Ok(Self { file_path, pages })
    }
}

impl ImageProvider for LocalProvider {
    fn page_count(&self) -> usize {
        self.pages.len()
    }

    fn fetch_page(&self, idx: usize) -> Result<Vec<u8>> {
        let page_name = self.pages.get(idx).ok_or_else(|| anyhow!("Page index out of bounds"))?;
        extract_comic_page(&self.file_path, page_name)
    }
}

pub struct RemoteProvider {
    manager: Arc<SourceManager>,
    source_id: String,
    image_urls: Vec<String>,
}

impl RemoteProvider {
    pub fn new(manager: Arc<SourceManager>, source_id: String, chapter_id: String) -> Result<Self> {
        let source = manager.get(&source_id).ok_or_else(|| anyhow!("Source not found"))?;
        let content = source.get_chapter_content(&chapter_id)?;
        
        match content {
            ChapterContent::Images(urls) => {
                Ok(Self {
                    manager,
                    source_id,
                    image_urls: urls,
                })
            }
            ChapterContent::Html(_) => {
                Err(anyhow!("Expected images for comic reader, got HTML"))
            }
        }
    }
}

impl ImageProvider for RemoteProvider {
    fn page_count(&self) -> usize {
        self.image_urls.len()
    }

    fn fetch_page(&self, idx: usize) -> Result<Vec<u8>> {
        let url = self.image_urls.get(idx).ok_or_else(|| anyhow!("Page index out of bounds"))?;
        let source = self.manager.get(&self.source_id).ok_or_else(|| anyhow!("Source not found"))?;
        source.fetch_image(url)
    }
}
