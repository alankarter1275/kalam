use gtk::gdk;
use anyhow::Result;

#[allow(dead_code)] // the tiled provider's load states; wired with the provider
pub enum ImageState {
    Loading,
    Ready(gdk::Texture),
    Failed,
}

pub trait ImageProvider: Send + Sync {
    /// Returns the number of pages.
    fn page_count(&self) -> usize;
    
    /// Returns true if a page exists.
    #[allow(dead_code)] // bounds-check for the provider's windowed decode
    fn has_page(&self, idx: usize) -> bool {
        idx < self.page_count()
    }
    
    /// Optionally blocks to fetch the image bytes for a given page index.
    fn fetch_page(&self, idx: usize) -> Result<Vec<u8>>;
}
