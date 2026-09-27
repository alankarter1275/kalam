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
    let tag_lower = tag.to_lowercase();
    let close = format!("</{}>", tag_lower);

    let mut search_from = 0;
    while let Some(open_rel) = lower_xml[search_from..].find('<') {
        let actual_open = search_from + open_rel;
        let rest = &lower_xml[actual_open + 1..];
        if rest.starts_with(&tag_lower) {
            let after_tag = &rest[tag_lower.len()..];
            if let Some(c) = after_tag.chars().next() {
                if c == '>' || c.is_whitespace() || c == '/' {
                    if let Some(end_tag_bracket) = after_tag.find('>') {
                        let content_start = actual_open + 1 + tag_lower.len() + end_tag_bracket + 1;
                        if let Some(close_rel) = lower_xml[content_start..].find(&close) {
                            let content_end = content_start + close_rel;
                            let mut val = xml[content_start..content_end].trim();
                            if val.starts_with("<![CDATA[") && val.ends_with("]]>") {
                                val = val[9..val.len() - 3].trim();
                            }
                            if !val.is_empty() {
                                return Some(val.to_string());
                            }
                        }
                    }
                }
            }
        }
        search_from = actual_open + 1;
    }
    None
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

fn strip_bracket_tags(s: &str) -> String {
    let mut result = s.to_string();
    while let Some(start) = result.find('[') {
        if let Some(end) = result[start..].find(']') {
            result.replace_range(start..=start + end, " ");
        } else {
            break;
        }
    }
    while let Some(start) = result.find('{') {
        if let Some(end) = result[start..].find('}') {
            result.replace_range(start..=start + end, " ");
        } else {
            break;
        }
    }
    let mut cleaned = String::new();
    let mut i = 0;
    let bytes = result.as_bytes();
    while i < bytes.len() {
        if bytes[i] == b'(' {
            if let Some(close_rel) = result[i..].find(')') {
                let content = result[i + 1..i + close_rel].trim();
                let lower = content.to_lowercase();
                let is_meta = lower.contains("digital")
                    || lower.contains("official")
                    || lower.contains("colored")
                    || lower.contains("colour")
                    || lower.contains("scan")
                    || lower.contains("raw")
                    || lower.contains("web")
                    || lower.contains("cbr")
                    || lower.contains("cbz")
                    || lower.starts_with('v')
                    || lower.starts_with('c')
                    || content.chars().all(|c| c.is_ascii_digit())
                    || i + close_rel + 1 == bytes.len();
                if is_meta {
                    cleaned.push(' ');
                    i += close_rel + 1;
                    continue;
                }
            }
        }
        cleaned.push(bytes[i] as char);
        i += 1;
    }

    let normalized = cleaned.replace(['–', '—'], "-");
    let words: Vec<&str> = normalized.split_whitespace().collect();
    words.join(" ")
}

fn extract_leading_number(token: &str) -> Option<f32> {
    let trimmed = token.trim();
    let cleaned = trimmed
        .trim_start_matches(|c: char| !c.is_ascii_digit() && c != '.')
        .trim();
    let num_str: String = cleaned
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    if num_str.chars().any(|c| c.is_ascii_digit()) {
        num_str.parse::<f32>().ok()
    } else {
        None
    }
}

fn parse_comic_filename_internal(raw_stem: &str, parent_dir: Option<&Path>) -> ComicInfo {
    let trimmed = strip_bracket_tags(raw_stem);
    if trimmed.is_empty() {
        return ComicInfo::default();
    }

    // 1. Check if stem is purely a chapter/volume number (e.g. "01", "001", "Ch. 5", "c12")
    if let Some(num) = extract_leading_number(&trimmed) {
        let rest: String = trimmed
            .to_lowercase()
            .replace("chapter", "")
            .replace("chap", "")
            .replace("ch.", "")
            .replace("ch", "")
            .replace("volume", "")
            .replace("vol.", "")
            .replace("vol", "")
            .replace("v", "")
            .replace("c", "")
            .replace("#", "")
            .replace("-", "")
            .replace("_", "")
            .replace(" ", "");
        if rest.chars().all(|c| c.is_ascii_digit() || c == '.') {
            if let Some(parent) = parent_dir.and_then(|p| p.file_name()) {
                let parent_name = parent.to_string_lossy().trim().to_string();
                let lower = parent_name.to_lowercase();
                let is_uuid = parent_name.len() == 36
                    && parent_name.chars().filter(|&c| c == '-').count() == 4;
                if !parent_name.is_empty()
                    && lower != "comics"
                    && lower != "downloads"
                    && lower != "manga"
                    && lower != "desktop"
                    && lower != "books"
                    && lower != "library"
                    && !is_uuid
                {
                    return ComicInfo {
                        series: Some(parent_name),
                        number: Some(num),
                        title: None,
                        ..Default::default()
                    };
                }
            }
        }
    }

    // 2. Check for " - " delimiter (e.g. "Series - 01" or "Series - 01 - Title")
    let parts: Vec<&str> = trimmed.split(" - ").collect();
    if parts.len() >= 2 {
        let series_cand = parts[0].trim().to_string();
        if !series_cand.is_empty() && series_cand.chars().any(|c| c.is_alphabetic()) {
            if let Some(num) = extract_leading_number(parts[1]) {
                let title = if parts.len() >= 3 {
                    Some(parts[2..].join(" - "))
                } else {
                    None
                };
                return ComicInfo {
                    series: Some(series_cand),
                    number: Some(num),
                    title,
                    ..Default::default()
                };
            } else if parts.len() >= 3 {
                if let Some(num) = extract_leading_number(parts[2]) {
                    return ComicInfo {
                        series: Some(series_cand),
                        number: Some(num),
                        title: Some(parts[1].trim().to_string()),
                        ..Default::default()
                    };
                }
            }
        }
    }

    // 3. Colon delimiter: "Title: Subtitle"
    if let Some(colon_idx) = trimmed.find(':') {
        let left = trimmed[..colon_idx].trim();
        let right = trimmed[colon_idx + 1..].trim();
        let left_info = parse_comic_filename_internal(left, parent_dir);
        if left_info.series.is_some() && left_info.number.is_some() {
            return ComicInfo {
                series: left_info.series,
                number: left_info.number,
                title: if !right.is_empty() {
                    Some(right.to_string())
                } else {
                    None
                },
                ..Default::default()
            };
        }
    }

    // 4. Explicit chapter/volume keywords
    let lower_stem = trimmed.to_lowercase();
    let keywords = [
        "chapter ", "chapter",
        "chap. ", "chap.", "chap ",
        "ch. ", "ch.", "ch ",
        " c ", " c",
        "volume ", "volume",
        "vol. ", "vol.", "vol ",
        " v ", " v",
        "issue ", "issue",
        "#",
    ];
    for kw in &keywords {
        if let Some(idx) = lower_stem.rfind(kw) {
            let series_cand = trimmed[..idx]
                .trim_end_matches([' ', '-', '_', '.', ','])
                .trim();
            let num_cand = &trimmed[idx + kw.len()..];
            if !series_cand.is_empty() && series_cand.chars().any(|c| c.is_alphabetic()) {
                if let Some(num) = extract_leading_number(num_cand) {
                    let mut title = None;
                    if let Some(after_num_idx) = num_cand.find(|c: char| c.is_alphabetic()) {
                        let potential_title = num_cand[after_num_idx..]
                            .trim_start_matches([' ', '-', '_', ':', '.'])
                            .trim();
                        if !potential_title.is_empty() {
                            title = Some(potential_title.to_string());
                        }
                    }
                    return ComicInfo {
                        series: Some(series_cand.to_string()),
                        number: Some(num),
                        title,
                        ..Default::default()
                    };
                }
            }
        }
    }

    // 5. Ending with number preceded by separator (e.g. "Naruto 01", "Naruto 1", "Naruto_05", "Naruto-03", "One Piece 1050")
    let chars: Vec<char> = trimmed.chars().collect();
    if !chars.is_empty() {
        let mut end = chars.len();
        while end > 0 && chars[end - 1].is_whitespace() {
            end -= 1;
        }
        let mut start = end;
        let mut has_dot = false;
        while start > 0 {
            let c = chars[start - 1];
            if c.is_ascii_digit() {
                start -= 1;
            } else if c == '.' && !has_dot && start > 1 && chars[start - 2].is_ascii_digit() {
                has_dot = true;
                start -= 1;
            } else {
                break;
            }
        }
        if start < end {
            let num_str: String = chars[start..end].iter().collect();
            if let Ok(num) = num_str.parse::<f32>() {
                if start > 0 {
                    let sep = chars[start - 1];
                    if sep == ' ' || sep == '-' || sep == '_' || sep == '.' || sep == ',' {
                        let series_cand: String = chars[..start - 1].iter().collect();
                        let series_cand = series_cand
                            .trim_end_matches([' ', '-', '_', '.', ','])
                            .trim();
                        if !series_cand.is_empty() && series_cand.chars().any(|c| c.is_alphabetic()) {
                            return ComicInfo {
                                series: Some(series_cand.to_string()),
                                number: Some(num),
                                title: None,
                                ..Default::default()
                            };
                        }
                    }
                }
            }
        }
    }

    ComicInfo {
        series: None,
        number: None,
        title: Some(trimmed),
        ..Default::default()
    }
}

/// Parse comic series name, chapter number, and title from a title string.
pub fn parse_comic_title(title: &str) -> ComicInfo {
    parse_comic_filename_internal(title, None)
}

/// Parse comic series name, chapter number, and title using filename and directory heuristics.
pub fn parse_comic_filename(path: &Path) -> ComicInfo {
    let stem = path
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    parse_comic_filename_internal(&stem, path.parent())
}

/// Comprehensive comic info parser checking ComicInfo.xml first, then filename heuristics.
pub fn parse_comic_info(archive_path: &Path) -> ComicInfo {
    let mut info = read_comic_info_from_zip(archive_path)
        .ok()
        .flatten()
        .unwrap_or_default();
    if info.series.is_none() || info.number.is_none() {
        let from_filename = parse_comic_filename(archive_path);
        if info.series.is_none() {
            info.series = from_filename.series;
        }
        if info.number.is_none() {
            info.number = from_filename.number;
        }
        if info.title.is_none() {
            info.title = from_filename.title;
        }
    }
    info
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

        // Case 6: Space-separated number (e.g. Naruto 01 .. Naruto 05)
        let p6 = Path::new("Naruto 01.cbz");
        let i6 = parse_comic_filename(p6);
        assert_eq!(i6.series.as_deref(), Some("Naruto"));
        assert_eq!(i6.number, Some(1.0));

        let p6b = Path::new("Naruto 5.cbz");
        let i6b = parse_comic_filename(p6b);
        assert_eq!(i6b.series.as_deref(), Some("Naruto"));
        assert_eq!(i6b.number, Some(5.0));

        let p6c = Path::new("Naruto 001.cbz");
        let i6c = parse_comic_filename(p6c);
        assert_eq!(i6c.series.as_deref(), Some("Naruto"));
        assert_eq!(i6c.number, Some(1.0));

        // Case 7: Volume variants
        let p7a = Path::new("Naruto Vol. 1.cbz");
        let i7a = parse_comic_filename(p7a);
        assert_eq!(i7a.series.as_deref(), Some("Naruto"));
        assert_eq!(i7a.number, Some(1.0));

        let p7b = Path::new("Naruto v02.cbz");
        let i7b = parse_comic_filename(p7b);
        assert_eq!(i7b.series.as_deref(), Some("Naruto"));
        assert_eq!(i7b.number, Some(2.0));

        // Case 8: Dot, underscore, and hyphen separators
        let p8a = Path::new("Naruto.03.cbz");
        let i8a = parse_comic_filename(p8a);
        assert_eq!(i8a.series.as_deref(), Some("Naruto"));
        assert_eq!(i8a.number, Some(3.0));

        let p8b = Path::new("Naruto_04.cbz");
        let i8b = parse_comic_filename(p8b);
        assert_eq!(i8b.series.as_deref(), Some("Naruto"));
        assert_eq!(i8b.number, Some(4.0));

        // Case 9: Brackets + space-separated number
        let p9 = Path::new("[Official Scan] Naruto 05 [Digital].cbz");
        let i9 = parse_comic_filename(p9);
        assert_eq!(i9.series.as_deref(), Some("Naruto"));
        assert_eq!(i9.number, Some(5.0));

        // Case 10: Parsing from book title directly
        let t1 = parse_comic_title("Naruto 01");
        assert_eq!(t1.series.as_deref(), Some("Naruto"));
        assert_eq!(t1.number, Some(1.0));

        let t2 = parse_comic_title("Naruto - Ch. 3");
        assert_eq!(t2.series.as_deref(), Some("Naruto"));
        assert_eq!(t2.number, Some(3.0));
    }
}


