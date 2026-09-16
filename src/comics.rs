//! Comic archive helper for reading `.cbz` and `.cbr` zip archives.
//!
//! CBZ files are ZIP archives containing page images (PNG, JPEG, WebP, GIF, AVIF).
//! Page list is naturally sorted so `page10.jpg` comes after `page2.jpg`.

use anyhow::{anyhow, Context, Result};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

/// Check if a filename extension corresponds to a supported comic image format.
pub fn is_image_filename(filename: &str) -> bool {
    let lower = filename.to_lowercase();
    lower.ends_with(".png")
        || lower.ends_with(".jpg")
        || lower.ends_with(".jpeg")
        || lower.ends_with(".webp")
        || lower.ends_with(".gif")
        || lower.ends_with(".avif")
}

/// Key for natural sorting: chunks of text and numbers, so "page2" < "page10".
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
enum SortChunk {
    Num(u64),
    Str(String),
}

fn natural_sort_key(s: &str) -> Vec<SortChunk> {
    let mut chunks = Vec::new();
    let mut curr_num = String::new();
    let mut curr_str = String::new();

    for c in s.chars() {
        if c.is_ascii_digit() {
            if !curr_str.is_empty() {
                chunks.push(SortChunk::Str(curr_str.to_lowercase()));
                curr_str.clear();
            }
            curr_num.push(c);
        } else {
            if !curr_num.is_empty() {
                if let Ok(n) = curr_num.parse::<u64>() {
                    chunks.push(SortChunk::Num(n));
                }
                curr_num.clear();
            }
            curr_str.push(c);
        }
    }
    if !curr_str.is_empty() {
        chunks.push(SortChunk::Str(curr_str.to_lowercase()));
    }
    if !curr_num.is_empty() {
        if let Ok(n) = curr_num.parse::<u64>() {
            chunks.push(SortChunk::Num(n));
        }
    }
    chunks
}

/// Natural sort helper for a list of string file names.
pub fn sort_comic_pages(pages: &mut [String]) {
    pages.sort_by_key(|page| natural_sort_key(page));
}

/// List all image pages in a comic archive, ordered naturally.
pub fn list_comic_pages(archive_path: &Path) -> Result<Vec<String>> {
    let file = File::open(archive_path)
        .with_context(|| format!("Could not open comic archive at {:?}", archive_path))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("Failed to read ZIP archive from {:?}", archive_path))?;

    let mut pages = Vec::new();
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name();
        // Ignore OS metadata files like __MACOSX/
        if name.contains("__MACOSX") || entry.is_dir() {
            continue;
        }
        if is_image_filename(name) {
            pages.push(name.to_string());
        }
    }

    sort_comic_pages(&mut pages);
    if pages.is_empty() {
        return Err(anyhow!("No image pages found in comic archive"));
    }
    Ok(pages)
}

/// Extract raw bytes of a single page from a comic archive.
pub fn extract_comic_page(archive_path: &Path, entry_name: &str) -> Result<Vec<u8>> {
    let file = File::open(archive_path)
        .with_context(|| format!("Could not open comic archive at {:?}", archive_path))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("Failed to read ZIP archive from {:?}", archive_path))?;

    let mut entry = archive
        .by_name(entry_name)
        .with_context(|| format!("Page '{entry_name}' not found in archive"))?;

    let mut buffer = Vec::with_capacity(entry.size() as usize);
    entry.read_to_end(&mut buffer)?;
    Ok(buffer)
}

/// Extract the first image page as the cover thumbnail.
#[allow(dead_code)]
pub fn extract_comic_cover(archive_path: &Path) -> Result<Vec<u8>> {
    let pages = list_comic_pages(archive_path)?;
    let first_page = pages
        .first()
        .ok_or_else(|| anyhow!("Comic archive has no image pages"))?;
    extract_comic_page(archive_path, first_page)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn image_filename_filtering() {
        assert!(is_image_filename("page01.JPG"));
        assert!(is_image_filename("01_cover.png"));
        assert!(is_image_filename("chapter/05.webp"));
        assert!(!is_image_filename("metadata.xml"));
        assert!(!is_image_filename(".DS_Store"));
    }

    #[test]
    fn natural_sorting_orders_numbers_correctly() {
        let mut pages = vec![
            "page10.jpg".to_string(),
            "page2.jpg".to_string(),
            "page1.jpg".to_string(),
            "page20.jpg".to_string(),
        ];
        sort_comic_pages(&mut pages);
        assert_eq!(
            pages,
            vec![
                "page1.jpg".to_string(),
                "page2.jpg".to_string(),
                "page10.jpg".to_string(),
                "page20.jpg".to_string(),
            ]
        );
    }

    #[test]
    fn natural_sorting_handles_nested_paths() {
        let mut pages = vec![
            "ch2/p1.png".to_string(),
            "ch1/p10.png".to_string(),
            "ch1/p2.png".to_string(),
        ];
        sort_comic_pages(&mut pages);
        assert_eq!(
            pages,
            vec![
                "ch1/p2.png".to_string(),
                "ch1/p10.png".to_string(),
                "ch2/p1.png".to_string(),
            ]
        );
    }
}
