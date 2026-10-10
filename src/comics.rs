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

fn strip_bracket_tags(s: &str) -> String {
    let mut result = s.replace(['–', '—'], "-");
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
    let mut rem = result.as_str();
    while let Some(open_idx) = rem.find('(') {
        cleaned.push_str(&rem[..open_idx]);
        if let Some(close_idx) = rem[open_idx..].find(')') {
            let content = rem[open_idx + 1..open_idx + close_idx].trim();
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
                || open_idx + close_idx + 1 == rem.len();
            if is_meta {
                cleaned.push(' ');
            } else {
                cleaned.push_str(&rem[open_idx..=open_idx + close_idx]);
            }
            rem = &rem[open_idx + close_idx + 1..];
        } else {
            rem = &rem[open_idx..];
            break;
        }
    }
    cleaned.push_str(rem);

    let words: Vec<&str> = cleaned.split_whitespace().collect();
    words.join(" ")
}

/// A chapter/volume marker: a short numeric token, optionally preceded
/// by a marker word or prefix ("Ch. 5", "c12", "vol2", "#12").
///
/// It must be a **complete word** — digits, at most one '.', one range
/// dash ("12-13" keeps its first half), at most five of them (the app's
/// own ceiling: chapter stems are four digits, five only past 9999) —
/// with nothing alphanumeric after the digits. The old scan skipped
/// *any* characters until it found digits, so the keyword `" c"`
/// matched the start of a uuid-padded series name and read
/// `"d272d85-…"` as chapter 272, truncating the title. CI caught it as
/// a 1-in-16 flake (the uuid has to start `c` + digit); a user's
/// "The Chronicles 1950" would have hit the same truncation for real.
fn marker_number(token: &str) -> Option<f32> {
    let mut rest = token.trim_start();
    // An optional marker word ahead of the digits: "Ch. 5", "Vol. 2".
    if let Some(space) = rest.find(char::is_whitespace) {
        let head = rest[..space].trim_end_matches(['.', '-', ',']);
        if !head.is_empty()
            && head.len() <= 7
            && head.chars().all(|c| c.is_alphabetic() || c == '#')
        {
            rest = rest[space..].trim_start();
        }
    }
    let word = rest.split_whitespace().next()?;
    // A short alphabetic prefix fused to the number: "c12", "vol2",
    // "chapter12". Longer than any marker word and it is a title word.
    let prefix_end = word
        .char_indices()
        .find(|(_, c)| !c.is_alphabetic())
        .map(|(i, _)| i)
        .unwrap_or(word.len());
    if prefix_end > 7 {
        return None;
    }
    let body = &word[prefix_end..];
    let mut digits = 0;
    for c in body.chars() {
        if c.is_ascii_digit() {
            digits += 1;
        } else if c == '.' || c == '-' {
            // A number can hold one dot and range dashes.
        } else {
            // A letter after the digits: a word that merely contains
            // digits, not a marker.
            return None;
        }
    }
    if digits == 0 || digits > 5 {
        return None;
    }
    let lead: String = body
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    lead.parse::<f32>().ok()
}

/// Clean and sanitize a comic series name by removing extraneous chapter/issue suffixes
/// (e.g. "Naruto – Digital Colored Comics - Ch. 2" -> ("Naruto - Digital Colored Comics", Some(2.0)))
pub fn sanitize_comic_series(raw: &str) -> (String, Option<f32>) {
    let stripped = strip_bracket_tags(raw);
    let normalized = stripped.replace(['–', '—'], "-");
    let trimmed = normalized.trim();
    if trimmed.is_empty() {
        return (String::new(), None);
    }

    // 1. Check for trailing " - Ch. X", " - Chapter X", " - c0X", " - #X", " - Vol. X"
    if let Some(dash_idx) = trimmed.rfind(" - ") {
        let after_dash = trimmed[dash_idx + 3..].trim();
        if let Some(num) = marker_number(after_dash) {
            let prefix = trimmed[..dash_idx].trim();
            if !prefix.is_empty() && prefix.chars().any(|c| c.is_alphabetic()) {
                return (prefix.to_string(), Some(num));
            }
        }
    }

    // 2. Check for explicit keywords: "chapter", "ch.", "ch ", "vol.", "vol ", "issue ", "#"
    let lower = trimmed.to_lowercase();
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
        if let Some(idx) = lower.rfind(kw) {
            let after = &trimmed[idx + kw.len()..];
            if let Some(num) = marker_number(after) {
                let prefix = trimmed[..idx]
                    .trim_end_matches([' ', '-', '_', '.', ','])
                    .trim();
                if !prefix.is_empty() && prefix.chars().any(|c| c.is_alphabetic()) {
                    return (prefix.to_string(), Some(num));
                }
            }
        }
    }

    // 3. Check for trailing digits preceded by separator (e.g. "Naruto 01", "Naruto_05")
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
            // Five digits is the ceiling a chapter number can reach
            // (stems are four, five only past 9999). A longer digit run
            // — a date, a serial, a uuid's all-digit tail group — is
            // not a chapter marker, and treating one as a marker
            // truncates the series title.
            let digit_count = num_str.chars().filter(|c| c.is_ascii_digit()).count();
            let num = if digit_count <= 5 {
                num_str.parse::<f32>().ok()
            } else {
                None
            };
            if let Some(num) = num {
                if start > 0 {
                    let sep = chars[start - 1];
                    if sep == ' ' || sep == '-' || sep == '_' || sep == '.' || sep == ',' {
                        let prefix: String = chars[..start - 1].iter().collect();
                        let prefix = prefix
                            .trim_end_matches([' ', '-', '_', '.', ','])
                            .trim();
                        if !prefix.is_empty() && prefix.chars().any(|c| c.is_alphabetic()) {
                            return (prefix.to_string(), Some(num));
                        }
                    }
                }
            }
        }
    }

    (trimmed.to_string(), None)
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
    let raw_series = extract_xml_tag(xml, "Series");
    let (series, series_num) = if let Some(ref s) = raw_series {
        let (clean_s, num) = sanitize_comic_series(s);
        (Some(clean_s), num)
    } else {
        (None, None)
    };
    let number = extract_xml_tag(xml, "Number")
        .and_then(|s| s.parse::<f32>().ok())
        .or(series_num);
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

fn parse_comic_filename_internal(raw_stem: &str, parent_dir: Option<&Path>) -> ComicInfo {
    let trimmed = strip_bracket_tags(raw_stem);
    if trimmed.is_empty() {
        return ComicInfo::default();
    }

    // 1. Check if stem is purely a chapter/volume number (e.g. "01", "001", "Ch. 5", "c12")
    if let Some(num) = marker_number(&trimmed) {
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
            if let Some(num) = marker_number(parts[1]) {
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
                if let Some(num) = marker_number(parts[2]) {
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
                if let Some(num) = marker_number(num_cand) {
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
            // Five digits is the ceiling a chapter number can reach
            // (stems are four, five only past 9999). A longer digit run
            // — a date, a serial, a uuid's all-digit tail group — is
            // not a chapter marker, and treating one as a marker
            // truncates the series title.
            let digit_count = num_str.chars().filter(|c| c.is_ascii_digit()).count();
            let num = if digit_count <= 5 {
                num_str.parse::<f32>().ok()
            } else {
                None
            };
            if let Some(num) = num {
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

/// Output format for remastered pages.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RemasterFormat {
    /// Re-encode as JPEG at the given quality (1–100). Smaller files; a
    /// little loss, which at quality 80+ is invisible on line art.
    Jpeg(u8),
    /// Re-encode as lossless PNG. Bigger files, pixel-perfect.
    Png,
}

/// Which pages of an archive a remaster touches.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum RemasterPages {
    /// Every image page.
    All,
    /// Only image pages narrower than this many pixels — the low-resolution
    /// ones, where upscaling actually buys something. Wider pages are
    /// copied through byte-for-byte.
    BelowWidth(u32),
    /// A 1-based inclusive range in the reader's page order, so page 1 is
    /// the first page the reader shows.
    Range { from: usize, to: usize },
}

/// Everything the caller chooses for a remaster. Nothing is hidden: the
/// owner's standing rule is that every option is the user's to decide.
#[derive(Debug, Clone, Copy)]
pub struct RemasterOptions {
    pub scale: f32,
    pub pages: RemasterPages,
    pub format: RemasterFormat,
}

/// What a finished remaster did, for the completion message.
#[derive(Debug, Default, PartialEq)]
pub struct RemasterReport {
    /// Image pages in the archive.
    pub pages_total: usize,
    /// Pages actually upscaled and re-encoded.
    pub pages_remastered: usize,
    /// Pages left alone for being at least the minimum width already.
    pub skipped_wide: usize,
    /// Pages left alone for being outside the chosen range.
    pub skipped_range: usize,
}

/// Upscale selected pages of a comic archive with the Lanczos3 filter.
///
/// Unpacks the archive, rescales each selected page image by `scale` using
/// `image::imageops::FilterType::Lanczos3` (the right resampler for
/// anti-aliased line art: it keeps ink edges sharp and screentones smooth),
/// re-encodes in the chosen format, and repacks every entry — untouched
/// pages pass through byte-for-byte, so nothing else in the archive
/// (reading order, `ComicInfo.xml`, folder structure) changes.
///
/// `progress_cb(done, total)` is called once per archive entry and returns
/// `false` to cancel; a cancelled remaster removes its partial output file
/// and returns an error, leaving the original untouched.
pub fn remaster_comic_archive<F>(
    input_path: &Path,
    output_path: &Path,
    opts: &RemasterOptions,
    mut progress_cb: F,
) -> Result<RemasterReport>
where
    F: FnMut(usize, usize) -> bool,
{
    let file = File::open(input_path)
        .with_context(|| format!("Could not open source comic archive at {:?}", input_path))?;
    let mut archive = ZipArchive::new(file)
        .with_context(|| format!("Failed to read ZIP archive from {:?}", input_path))?;

    let total_entries = archive.len();

    // Page numbers in the reader's order, not the archive's storage order:
    // a "page range" must mean the same pages the reader shows.
    let mut image_names: Vec<String> = Vec::new();
    for i in 0..total_entries {
        let entry = archive.by_index(i)?;
        let name = entry.name();
        if !entry.is_dir() && is_image_filename(name) && !name.contains("__MACOSX") {
            image_names.push(name.to_string());
        }
    }
    sort_comic_pages(&mut image_names);
    let page_numbers: std::collections::HashMap<String, usize> = image_names
        .iter()
        .enumerate()
        .map(|(i, n)| (n.clone(), i + 1))
        .collect();

    let mut report = RemasterReport {
        pages_total: image_names.len(),
        ..RemasterReport::default()
    };

    let dest_file = File::create(output_path)
        .with_context(|| format!("Could not create output comic archive at {:?}", output_path))?;
    let mut zip_writer = ZipWriter::new(dest_file);

    let deflated_options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);

    for i in 0..total_entries {
        let mut entry = archive.by_index(i)?;
        let name = entry.name().to_string();

        if entry.is_dir() {
            if !progress_cb(i + 1, total_entries) {
                drop(zip_writer);
                let _ = std::fs::remove_file(output_path);
                return Err(anyhow!("cancelled"));
            }
            continue;
        }

        let mut buffer = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut buffer)?;

        match remaster_entry(&name, &buffer, opts, &page_numbers, &mut report) {
            EntryOutput::Untouched => {
                zip_writer.start_file(&name, deflated_options)?;
                zip_writer.write_all(&buffer)?;
            }
            EntryOutput::Reencoded(bytes) => {
                zip_writer.start_file(&name, deflated_options)?;
                zip_writer.write_all(&bytes)?;
            }
        }

        if !progress_cb(i + 1, total_entries) {
            drop(zip_writer);
            let _ = std::fs::remove_file(output_path);
            return Err(anyhow!("cancelled"));
        }
    }

    zip_writer.finish()?;
    Ok(report)
}

/// What [`remaster_comic_archive`] does with one entry's bytes.
enum EntryOutput {
    /// Copy the original bytes through unchanged.
    Untouched,
    /// Write these re-encoded bytes instead.
    Reencoded(Vec<u8>),
}

/// Decide and encode one archive entry: upscaled and re-encoded when the
/// options select it, the untouched original bytes otherwise.
fn remaster_entry(
    name: &str,
    original: &[u8],
    opts: &RemasterOptions,
    page_numbers: &std::collections::HashMap<String, usize>,
    report: &mut RemasterReport,
) -> EntryOutput {
    if !is_image_filename(name) || name.contains("__MACOSX") {
        return EntryOutput::Untouched;
    }
    // A file with an image name that does not decode (corrupt, or a format
    // this build cannot read) must survive a remaster: pass it through.
    let Ok(img) = image::load_from_memory(original) else {
        return EntryOutput::Untouched;
    };

    let selected = match opts.pages {
        RemasterPages::All => true,
        RemasterPages::BelowWidth(limit) => img.width() < limit,
        RemasterPages::Range { from, to } => {
            let (lo, hi) = (from.min(to), from.max(to));
            page_numbers.get(name).is_some_and(|&n| n >= lo && n <= hi)
        }
    };
    if !selected {
        if matches!(opts.pages, RemasterPages::BelowWidth(_)) {
            report.skipped_wide += 1;
        } else {
            report.skipped_range += 1;
        }
        return EntryOutput::Untouched;
    }

    let nwidth = ((img.width() as f32 * opts.scale).round() as u32).max(1);
    let nheight = ((img.height() as f32 * opts.scale).round() as u32).max(1);
    let resized = img.resize(nwidth, nheight, image::imageops::FilterType::Lanczos3);

    // JPEG cannot carry an alpha channel; flattening onto white would tint
    // transparent manga scans, and dropping alpha to black would darken
    // them. RGB conversion keeps the pixels' own colours.
    let flattened = if matches!(opts.format, RemasterFormat::Jpeg(_)) && resized.color().has_alpha()
    {
        Some(image::DynamicImage::ImageRgb8(resized.to_rgb8()))
    } else {
        None
    };
    let to_encode = flattened.as_ref().unwrap_or(&resized);

    let mut encoded = Vec::new();
    let encode_res = match opts.format {
        RemasterFormat::Jpeg(quality) => {
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, quality)
                .encode_image(to_encode)
        }
        RemasterFormat::Png => {
            resized.write_to(&mut std::io::Cursor::new(&mut encoded), image::ImageFormat::Png)
        }
    };
    if encode_res.is_ok() {
        report.pages_remastered += 1;
        return EntryOutput::Reencoded(encoded);
    }
    // The chosen encoder refused (a format this build cannot write): fall
    // back to PNG, then to the untouched original. A remaster must never
    // lose a page.
    let mut png = Vec::new();
    if resized
        .write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png)
        .is_ok()
    {
        report.pages_remastered += 1;
        return EntryOutput::Reencoded(png);
    }
    EntryOutput::Untouched
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build a throwaway CBZ in the system temp dir with the given pages.
    /// Returns (input path, output path).
    ///
    /// Like every helper in this repo, this sits above the first `#[test]`:
    /// the panic guardrail suppresses `mod tests` only down to the first
    /// nested test item, so a helper with `.expect()` between test
    /// functions would be miscounted as production code (pitfalls §26).
    fn scratch_cbz(
        tag: &str,
        pages: &[(&str, image::DynamicImage)],
    ) -> (std::path::PathBuf, std::path::PathBuf) {
        let uid = uuid::Uuid::new_v4().to_string();
        let input = std::env::temp_dir().join(format!("kalam-remaster-{tag}-in-{uid}.cbz"));
        let output = std::env::temp_dir().join(format!("kalam-remaster-{tag}-out-{uid}.cbz"));

        let file = File::create(&input).expect("create input cbz");
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default();
        for (name, img) in pages {
            let mut bytes = Vec::new();
            img.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
                .expect("encode fixture page");
            zip.start_file(*name, options).expect("add fixture page");
            zip.write_all(&bytes).expect("write fixture page");
        }
        zip.finish().expect("finish fixture cbz");
        (input, output)
    }

    /// Decode every entry of a CBZ by name, so tests can check which pages
    /// were upscaled and which were left alone.
    fn decoded_entries(path: &std::path::Path) -> Vec<(String, image::DynamicImage)> {
        let file = File::open(path).expect("open output cbz");
        let mut zip = ZipArchive::new(file).expect("read output cbz");
        let mut out = Vec::new();
        for i in 0..zip.len() {
            let mut entry = zip.by_index(i).expect("output entry");
            let name = entry.name().to_string();
            let mut bytes = Vec::new();
            entry.read_to_end(&mut bytes).expect("read output entry");
            let img = image::load_from_memory(&bytes).expect("decode output page");
            out.push((name, img));
        }
        out
    }

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
    fn remaster_upscales_every_page_and_reports() -> Result<()> {
        let page = image::DynamicImage::ImageRgb8(image::RgbImage::new(10, 10));
        let (input, output) = scratch_cbz("all", &[("page01.png", page)]);

        let report = remaster_comic_archive(
            &input,
            &output,
            &RemasterOptions {
                scale: 2.0,
                pages: RemasterPages::All,
                format: RemasterFormat::Jpeg(90),
            },
            |_, _| true,
        )?;

        let entries = decoded_entries(&output);
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].1.width(), 20);
        assert_eq!(entries[0].1.height(), 20);
        assert_eq!(report.pages_total, 1);
        assert_eq!(report.pages_remastered, 1);
        assert_eq!(report.skipped_wide, 0);

        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);
        Ok(())
    }

    #[test]
    fn remaster_leaves_wide_pages_alone_below_a_width_limit() -> Result<()> {
        let (input, output) = scratch_cbz(
            "wide",
            &[
                ("a_small.png", image::DynamicImage::ImageRgb8(image::RgbImage::new(60, 10))),
                ("b_big.png", image::DynamicImage::ImageRgb8(image::RgbImage::new(300, 10))),
            ],
        );

        let report = remaster_comic_archive(
            &input,
            &output,
            &RemasterOptions {
                scale: 2.0,
                pages: RemasterPages::BelowWidth(100),
                format: RemasterFormat::Png,
            },
            |_, _| true,
        )?;

        for (name, img) in decoded_entries(&output) {
            if name == "a_small.png" {
                assert_eq!(img.width(), 120, "small page must be upscaled");
            } else {
                assert_eq!(img.width(), 300, "wide page must be untouched");
            }
        }
        assert_eq!(report.pages_total, 2);
        assert_eq!(report.pages_remastered, 1);
        assert_eq!(report.skipped_wide, 1);

        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);
        Ok(())
    }

    #[test]
    fn remaster_page_ranges_use_reading_order_not_storage_order() -> Result<()> {
        // Stored deliberately out of order: lexicographic order would make
        // z10.png the second entry, natural order (what the reader shows)
        // makes it the third.
        let (input, output) = scratch_cbz(
            "range",
            &[
                ("z10.png", image::DynamicImage::ImageRgb8(image::RgbImage::new(10, 10))),
                ("z1.png", image::DynamicImage::ImageRgb8(image::RgbImage::new(10, 10))),
                ("z2.png", image::DynamicImage::ImageRgb8(image::RgbImage::new(10, 10))),
            ],
        );

        let report = remaster_comic_archive(
            &input,
            &output,
            &RemasterOptions {
                scale: 2.0,
                pages: RemasterPages::Range { from: 2, to: 3 },
                format: RemasterFormat::Png,
            },
            |_, _| true,
        )?;

        // Reader order is z1 (1), z2 (2), z10 (3): the range 2–3 selects
        // z2 and z10; z1 must come through untouched.
        for (name, img) in decoded_entries(&output) {
            match name.as_str() {
                "z1.png" => assert_eq!(img.width(), 10, "page 1 is outside the range"),
                "z2.png" | "z10.png" => assert_eq!(img.width(), 20, "{name} is inside the range"),
                other => panic!("unexpected entry {other}"),
            }
        }
        assert_eq!(report.pages_total, 3);
        assert_eq!(report.pages_remastered, 2);
        assert_eq!(report.skipped_range, 1);

        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);
        Ok(())
    }

    #[test]
    fn a_cancelled_remaster_removes_its_partial_output() -> Result<()> {
        let (input, output) = scratch_cbz(
            "cancel",
            &[("a.png", image::DynamicImage::ImageRgb8(image::RgbImage::new(10, 10)))],
        );

        let result = remaster_comic_archive(
            &input,
            &output,
            &RemasterOptions {
                scale: 2.0,
                pages: RemasterPages::All,
                format: RemasterFormat::Png,
            },
            |_, _| false,
        );

        assert!(result.is_err(), "cancelling must surface an error");
        assert!(!output.exists(), "partial output must be removed");
        // And the original is still readable.
        assert!(File::open(&input).is_ok());

        let _ = std::fs::remove_file(&input);
        Ok(())
    }

    #[test]
    fn jpeg_output_never_carries_an_alpha_channel() -> Result<()> {
        let (input, output) = scratch_cbz(
            "alpha",
            &[("a.png", image::DynamicImage::ImageRgba8(image::RgbaImage::new(10, 10)))],
        );

        remaster_comic_archive(
            &input,
            &output,
            &RemasterOptions {
                scale: 2.0,
                pages: RemasterPages::All,
                format: RemasterFormat::Jpeg(85),
            },
            |_, _| true,
        )?;

        let entries = decoded_entries(&output);
        assert_eq!(entries.len(), 1);
        assert!(!entries[0].1.color().has_alpha(), "JPEG has no alpha channel");

        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);
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

    /// A uuid padded onto a series name — what the import tests do to
    /// stay unique — must not be read as a chapter marker. The old
    /// lenient scan matched the keyword " c" against the start of
    /// "cd272d85-…" and invented chapter 272, truncating the title to
    /// "Import Series" (the CI flake of 2026-10-02, pitfalls §47: it
    /// only fired when the random uuid began "c" + digit, about one run
    /// in sixteen).
    #[test]
    fn a_uuid_padded_series_name_is_not_a_chapter_marker() {
        for series in [
            // " c" + digits: the exact CI failure shape.
            "Import Series cd272d85-3d0b-486e-a077-4aba134cf1c4",
            // " c" + digit + dash: the word check must reject it too.
            "Import Series c272-3d0b-486e-a077-4aba134cf1c4",
            // " v" is a keyword as well.
            "Import Series vd272d85-3d0b-486e-a077-4aba134cf1c4",
            // An all-digit tail group trips the trailing-digit rule
            // unless the digit cap stops it (about one run in 200).
            "Import Series 9d272d85-3d0b-486e-a077-123456789012",
            // No c/v prefix, letters through the tail: the plain case.
            "Import Series 9d272d85-3d0b-486e-a077-4aba134cf1c4",
        ] {
            let (clean, num) = sanitize_comic_series(series);
            assert_eq!(clean, series, "the series name must survive intact");
            assert_eq!(num, None, "no chapter number may be invented from {series}");
        }
    }

    /// The marker forms the heuristics exist for keep parsing after the
    /// strict word check replaced the lenient scan.
    #[test]
    fn chapter_marker_forms_still_parse() {
        let eq = |raw: &str, name: &str, num: f32| {
            let (clean, parsed) = sanitize_comic_series(raw);
            assert_eq!(clean, name, "series from {raw:?}");
            assert_eq!(parsed, Some(num), "number from {raw:?}");
        };
        eq("Naruto c12", "Naruto", 12.0);
        eq("Naruto c12-13", "Naruto", 12.0);
        eq("Naruto v2", "Naruto", 2.0);
        eq("Berserk chapter 356", "Berserk", 356.0);
        eq("Naruto 0102", "Naruto", 102.0);
        eq(
            "Naruto – Digital Colored Comics - Ch. 2",
            "Naruto - Digital Colored Comics",
            2.0,
        );

        // Fused prefix forms resolve through the filename parser, where
        // the parent folder supplies the series name.
        let info = parse_comic_filename(Path::new("/tmp/Naruto/c12.cbz"));
        assert_eq!(info.series.as_deref(), Some("Naruto"));
        assert_eq!(info.number, Some(12.0));
        let info = parse_comic_filename(Path::new("/tmp/Naruto/vol2.cbz"));
        assert_eq!(info.series.as_deref(), Some("Naruto"));
        assert_eq!(info.number, Some(2.0));
    }
}


