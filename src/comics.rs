//! Comic archive helper for reading `.cbz` and `.cbr` zip archives.
//!
//! CBZ files are ZIP archives containing page images (PNG, JPEG, WebP, GIF, AVIF).
//! Page list is naturally sorted so `page10.jpg` comes after `page2.jpg`.

use anyhow::{anyhow, Context, Result};
use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

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
pub fn extract_comic_cover(archive_path: &Path) -> Result<Vec<u8>> {
    let pages = list_comic_pages(archive_path)?;
    let first_page = pages
        .first()
        .ok_or_else(|| anyhow!("Comic archive has no image pages"))?;
    extract_comic_page(archive_path, first_page)
}

/// Structured metadata for a comic issue or manga chapter.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ComicInfo {
    pub series: Option<String>,
    pub number: Option<f32>,
    pub volume: Option<i32>,
    pub title: Option<String>,
    pub writer: Option<String>,
    pub summary: Option<String>,
}

fn extract_xml_tag(xml: &str, tag: &str) -> Option<String> {
    let lower_xml = xml.to_lowercase();
    let open = format!("<{}>", tag.to_lowercase());
    let close = format!("</{}>", tag.to_lowercase());
    let start_idx = lower_xml.find(&open)? + open.len();
    let end_idx = lower_xml[start_idx..].find(&close)? + start_idx;
    let val = xml[start_idx..end_idx].trim();
    if val.is_empty() {
        None
    } else {
        Some(val.to_string())
    }
}

/// Parse an XML string adhering to the ComicRack ComicInfo.xml schema.
pub fn parse_comic_info_xml(xml: &str) -> ComicInfo {
    let series = extract_xml_tag(xml, "Series");
    let number = extract_xml_tag(xml, "Number").and_then(|s| s.parse::<f32>().ok());
    let volume = extract_xml_tag(xml, "Volume").and_then(|s| s.parse::<i32>().ok());
    let title = extract_xml_tag(xml, "Title");
    let writer = extract_xml_tag(xml, "Writer");
    let summary = extract_xml_tag(xml, "Summary");

    ComicInfo {
        series,
        number,
        volume,
        title,
        writer,
        summary,
    }
}

/// Read `ComicInfo.xml` from a CBZ zip archive if present.
pub fn read_comic_info_from_zip(archive_path: &Path) -> Result<Option<ComicInfo>> {
    let file = File::open(archive_path)?;
    let mut archive = ZipArchive::new(file)?;
    for i in 0..archive.len() {
        let entry = archive.by_index(i)?;
        let name = entry.name().to_lowercase();
        if name == "comicinfo.xml" || name.ends_with("/comicinfo.xml") {
            let mut buf = String::new();
            let mut reader = std::io::BufReader::new(entry);
            reader.read_to_string(&mut buf)?;
            return Ok(Some(parse_comic_info_xml(&buf)));
        }
    }
    Ok(None)
}

/// Parse comic series name, chapter number, and title using filename and directory heuristics.
pub fn parse_comic_filename(path: &Path) -> ComicInfo {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    if stem.is_empty() {
        return ComicInfo::default();
    }

    fn strip_outer_brackets(s: &str) -> String {
        let mut result = s.to_string();
        while let Some(start) = result.find('[') {
            if let Some(end) = result[start..].find(']') {
                result.replace_range(start..=start + end, "");
            } else {
                break;
            }
        }
        while let Some(start) = result.find('(') {
            if let Some(end) = result[start..].find(')') {
                result.replace_range(start..=start + end, "");
            } else {
                break;
            }
        }
        result.trim().to_string()
    }

    fn extract_chapter_number(token: &str) -> Option<f32> {
        let cleaned = token
            .trim_start_matches(|c: char| !c.is_ascii_digit() && c != '.')
            .trim();
        let num_str: String = cleaned
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        num_str.parse::<f32>().ok()
    }

    let trimmed_stem = strip_outer_brackets(&stem);

    // Case 1: Stem is just a number or chapter token (e.g. "001", "Ch. 5", "c12") -> use parent directory
    if let Some(num) = extract_chapter_number(&trimmed_stem) {
        let rest: String = trimmed_stem
            .to_lowercase()
            .replace("chapter", "")
            .replace("ch.", "")
            .replace("ch", "")
            .replace("vol.", "")
            .replace("vol", "")
            .replace("v", "")
            .replace("#", "")
            .chars()
            .filter(|c| !c.is_ascii_digit() && *c != '.' && !c.is_whitespace() && *c != '-' && *c != '_')
            .collect();
        if rest.is_empty() {
            if let Some(parent) = path.parent().and_then(|p| p.file_name()) {
                let parent_name = parent.to_string_lossy().trim().to_string();
                let lower = parent_name.to_lowercase();
                if !parent_name.is_empty()
                    && lower != "comics"
                    && lower != "downloads"
                    && lower != "manga"
                    && lower != "desktop"
                {
                    return ComicInfo {
                        series: Some(parent_name),
                        number: Some(num),
                        ..Default::default()
                    };
                }
            }
        }
    }

    // Case 2: "Series - Chapter Part - Title Part"
    let parts: Vec<&str> = trimmed_stem.split(" - ").collect();
    if parts.len() >= 2 {
        let series = parts[0].trim().to_string();
        let mut chapter_num = extract_chapter_number(parts[1]);
        let mut title = if parts.len() >= 3 {
            Some(parts[2].trim().to_string())
        } else {
            None
        };
        if chapter_num.is_none() && parts.len() >= 3 {
            chapter_num = extract_chapter_number(parts[2]);
            title = Some(parts[1].trim().to_string());
        }
        if !series.is_empty() {
            return ComicInfo {
                series: Some(series),
                number: chapter_num,
                title,
                ..Default::default()
            };
        }
    }

    // Case 3: "Series #12"
    if let Some(hash_idx) = trimmed_stem.find('#') {
        let series = trimmed_stem[..hash_idx].trim().to_string();
        let num = extract_chapter_number(&trimmed_stem[hash_idx + 1..]);
        if !series.is_empty() {
            return ComicInfo {
                series: Some(series),
                number: num,
                ..Default::default()
            };
        }
    }

    // Case 4: "Series Ch 12" / "Series Chapter 12" / "Series c12"
    for prefix in &["chapter ", "chapter", "ch. ", "ch.", "ch ", " c"] {
        if let Some(idx) = trimmed_stem.to_lowercase().rfind(prefix) {
            let series = trimmed_stem[..idx].trim_end_matches(&[' ', '-', '_'][..]).trim().to_string();
            let num = extract_chapter_number(&trimmed_stem[idx + prefix.len()..]);
            if !series.is_empty() && num.is_some() {
                return ComicInfo {
                    series: Some(series),
                    number: num,
                    ..Default::default()
                };
            }
        }
    }

    // Case 5: "Series_01" or "Series-01"
    if let Some(sep_idx) = trimmed_stem.rfind(&['_', '-'][..]) {
        let potential_num = extract_chapter_number(&trimmed_stem[sep_idx + 1..]);
        if let Some(num) = potential_num {
            let series = trimmed_stem[..sep_idx].trim().to_string();
            if !series.is_empty() {
                return ComicInfo {
                    series: Some(series),
                    number: Some(num),
                    ..Default::default()
                };
            }
        }
    }

    ComicInfo {
        series: None,
        number: None,
        title: Some(trimmed_stem),
        ..Default::default()
    }
}

/// Comprehensive comic info parser checking ComicInfo.xml first, then filename heuristics.
pub fn parse_comic_info(archive_path: &Path) -> ComicInfo {
    if let Ok(Some(info)) = read_comic_info_from_zip(archive_path) {
        if info.series.is_some() || info.title.is_some() {
            return info;
        }
    }
    parse_comic_filename(archive_path)
}

/// Upscale all image pages in a CBZ archive using Lanczos3 resampling filter.
///
/// Unpacks the CBZ file, rescales each page image by `scale_factor` (e.g. 2.0x) using `image::imageops::FilterType::Lanczos3`
/// to preserve ink sharpness and smooth screentones, and repacks into `output_path`.
pub fn remaster_comic_cbz<F>(
    input_path: &Path,
    output_path: &Path,
    scale_factor: f32,
    progress_cb: F,
) -> Result<()>
where
    F: Fn(usize, usize),
{
    let file = File::open(input_path)
        .with_context(|| format!("Could not open source comic archive at {:?}", input_path))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("Failed to read ZIP archive from {:?}", input_path))?;

    let total_entries = archive.len();
    let dest_file = File::create(output_path)
        .with_context(|| format!("Could not create output comic archive at {:?}", output_path))?;
    let mut zip_writer = ZipWriter::new(dest_file);

    let deflated_options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for i in 0..total_entries {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        if entry.is_dir() {
            progress_cb(i + 1, total_entries);
            continue;
        }

        let mut buffer = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buffer)?;

        if is_image_filename(&name) && !name.contains("__MACOSX") {
            if let Ok(img) = image::load_from_memory(&buffer) {
                let nwidth = (img.width() as f32 * scale_factor).round() as u32;
                let nheight = (img.height() as f32 * scale_factor).round() as u32;
                let nwidth = nwidth.max(1);
                let nheight = nheight.max(1);

                let resized = img.resize(nwidth, nheight, image::imageops::FilterType::Lanczos3);

                let mut encoded_bytes = Vec::new();
                let lower = name.to_lowercase();
                let format = if lower.ends_with(".jpg") || lower.ends_with(".jpeg") {
                    image::ImageFormat::Jpeg
                } else if lower.ends_with(".webp") {
                    image::ImageFormat::WebP
                } else if lower.ends_with(".gif") {
                    image::ImageFormat::Gif
                } else {
                    image::ImageFormat::Png
                };

                let encode_res = if format == image::ImageFormat::Jpeg {
                    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded_bytes, 92);
                    // encode_image already yields ImageError; the old
                    // `.map_err(ImageError::from)` converted it to itself.
                    encoder.encode_image(&resized)
                } else {
                    resized.write_to(&mut std::io::Cursor::new(&mut encoded_bytes), format)
                };

                if encode_res.is_ok() {
                    zip_writer.start_file(&name, deflated_options)?;
                    zip_writer.write_all(&encoded_bytes)?;
                } else {
                    // Fallback to PNG encoding if original format encoder (e.g. WebP) is unavailable in image crate
                    let mut png_bytes = Vec::new();
                    if resized.write_to(&mut std::io::Cursor::new(&mut png_bytes), image::ImageFormat::Png).is_ok() {
                        zip_writer.start_file(&name, deflated_options)?;
                        zip_writer.write_all(&png_bytes)?;
                    } else {
                        zip_writer.start_file(&name, deflated_options)?;
                        zip_writer.write_all(&buffer)?;
                    }
                }
            } else {
                zip_writer.start_file(&name, deflated_options)?;
                zip_writer.write_all(&buffer)?;
            }
        } else {
            zip_writer.start_file(&name, deflated_options)?;
            zip_writer.write_all(&buffer)?;
        }

        progress_cb(i + 1, total_entries);
    }

    zip_writer.finish()?;
    Ok(())
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

    #[test]
    fn test_remaster_comic_cbz() -> Result<()> {
        let tmp_dir = std::env::temp_dir();
        let uid = uuid::Uuid::new_v4().to_string();
        let input_cbz = tmp_dir.join(format!("test_in_{uid}.cbz"));
        let output_cbz = tmp_dir.join(format!("test_out_{uid}.cbz"));

        // Create a dummy CBZ with a 10x10 image
        let img = image::RgbImage::new(10, 10);
        let mut img_bytes = Vec::new();
        img.write_to(&mut std::io::Cursor::new(&mut img_bytes), image::ImageFormat::Png)?;

        {
            let file = File::create(&input_cbz)?;
            let mut zip = ZipWriter::new(file);
            let options = SimpleFileOptions::default();
            zip.start_file("page01.png", options)?;
            zip.write_all(&img_bytes)?;
            zip.finish()?;
        }

        // Remaster 2.0x
        remaster_comic_cbz(&input_cbz, &output_cbz, 2.0, |_, _| {})?;

        // Verify output archive
        let out_file = File::open(&output_cbz)?;
        let mut out_zip = ZipArchive::new(out_file)?;
        assert_eq!(out_zip.len(), 1);

        let mut entry = out_zip.by_index(0)?;
        let mut out_bytes = Vec::new();
        entry.read_to_end(&mut out_bytes)?;

        let remastered_img = image::load_from_memory(&out_bytes)?;
        assert_eq!(remastered_img.width(), 20);
        assert_eq!(remastered_img.height(), 20);

        let _ = std::fs::remove_file(&input_cbz);
        let _ = std::fs::remove_file(&output_cbz);

        Ok(())
    }

    #[test]
    fn test_parse_comic_info_xml() {
        let xml = r#"<?xml version="1.0" encoding="utf-8"?>
<ComicInfo>
  <Series>Berserk</Series>
  <Number>12.5</Number>
  <Volume>2</Volume>
  <Title>The Golden Age</Title>
  <Writer>Kentaro Miura</Writer>
  <Summary>Guts battles on.</Summary>
</ComicInfo>"#;
        let info = parse_comic_info_xml(xml);
        assert_eq!(info.series.as_deref(), Some("Berserk"));
        assert_eq!(info.number, Some(12.5));
        assert_eq!(info.volume, Some(2));
        assert_eq!(info.title.as_deref(), Some("The Golden Age"));
        assert_eq!(info.writer.as_deref(), Some("Kentaro Miura"));
        assert_eq!(info.summary.as_deref(), Some("Guts battles on."));
    }

    #[test]
    fn test_parse_comic_filename_patterns() {
        // Case 1: Series - Chapter - Title
        let p1 = Path::new("/comics/Berserk - c001 - The Black Swordsman.cbz");
        let i1 = parse_comic_filename(p1);
        assert_eq!(i1.series.as_deref(), Some("Berserk"));
        assert_eq!(i1.number, Some(1.0));
        assert_eq!(i1.title.as_deref(), Some("The Black Swordsman"));

        // Case 2: Brackets + Chapter
        let p2 = Path::new("[ScanGroup] Chainsaw Man - Chapter 12 [Digital].cbz");
        let i2 = parse_comic_filename(p2);
        assert_eq!(i2.series.as_deref(), Some("Chainsaw Man"));
        assert_eq!(i2.number, Some(12.0));

        // Case 3: Hash Issue
        let p3 = Path::new("Batman #04 (2016).cbr");
        let i3 = parse_comic_filename(p3);
        assert_eq!(i3.series.as_deref(), Some("Batman"));
        assert_eq!(i3.number, Some(4.0));

        // Case 4: Parent directory
        let p4 = Path::new("/home/user/Comics/Solo Leveling/005.cbz");
        let i4 = parse_comic_filename(p4);
        assert_eq!(i4.series.as_deref(), Some("Solo Leveling"));
        assert_eq!(i4.number, Some(5.0));

        // Case 5: Fractional chapter
        let p5 = Path::new("Spy x Family - c009.5.cbz");
        let i5 = parse_comic_filename(p5);
        assert_eq!(i5.series.as_deref(), Some("Spy x Family"));
        assert_eq!(i5.number, Some(9.5));
    }
}


