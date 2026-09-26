//! PDF Engine Integration via MuPDF.
//!
//! Provides true MuPDF document parsing, high-fidelity page rasterization,
//! vector outline / TOC extraction, Zathura-style smart cropping, and text extraction.

use anyhow::{anyhow, Result};
use image::{DynamicImage, GenericImageView, RgbaImage};
use mupdf::{Colorspace, Document, Matrix, Outline, Rect, TextBlockContent, TextExtractOptions, TextPageFlags};
use std::path::{Path, PathBuf};

/// Bounding rectangle in PDF points: (x0, y0, x1, y1).
pub type PdfTextRect = (f32, f32, f32, f32);

/// Extracted text selection result: (selected_text, highlight_rects).
pub type PdfSelectionResult = (String, Vec<PdfTextRect>);

/// Single extracted character with bounding box in PDF points (72 DPI).
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct PdfTextChar {
    pub ch: char,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

/// Single extracted text line with bounding box and character stream in PDF points.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct PdfTextLine {
    pub text: String,
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    pub chars: Vec<PdfTextChar>,
}

/// Extracted page text containing line hierarchy and document point dimensions.
#[allow(dead_code)]
#[derive(Debug, Clone, PartialEq)]
pub struct PdfPageText {
    pub page_num: usize,
    pub width_pts: f32,
    pub height_pts: f32,
    pub lines: Vec<PdfTextLine>,
}

impl PdfPageText {
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.lines.is_empty() || self.lines.iter().all(|l| l.chars.is_empty())
    }

    #[allow(dead_code)]
    pub fn total_chars(&self) -> usize {
        self.lines.iter().map(|l| l.chars.len()).sum()
    }

    /// Helper to find the index of the line closest to a given document point.
    pub fn find_closest_line_index(&self, p: (f32, f32)) -> Option<usize> {
        if self.lines.is_empty() {
            return None;
        }

        let mut best_idx = None;
        let mut min_dist = f32::MAX;

        for (idx, line) in self.lines.iter().enumerate() {
            let top = line.y0.min(line.y1);
            let bottom = line.y0.max(line.y1);
            let left = line.x0.min(line.x1);
            let right = line.x0.max(line.x1);

            let dy = if p.1 < top {
                top - p.1
            } else if p.1 > bottom {
                p.1 - bottom
            } else {
                0.0
            };

            let dx = if p.0 < left {
                left - p.0
            } else if p.0 > right {
                p.0 - right
            } else {
                0.0
            };

            // Heavily weight dy so lines on the same vertical level take strong precedence,
            // while dx disambiguates columns cleanly.
            let dist = dy * 4.0 + dx;
            if dist < min_dist {
                min_dist = dist;
                best_idx = Some(idx);
            }
        }

        best_idx
    }

    /// Select text between two points in document point coordinates.
    /// Employs column-aware segmentation matching standard Adobe Acrobat / Foxit behavior:
    /// prevents drag selection in one column/box from bleeding into adjacent columns,
    /// sidebars, or callouts located at the same vertical height.
    #[allow(dead_code)]
    pub fn select_between(
        &self,
        p0: (f32, f32),
        p1: (f32, f32),
    ) -> PdfSelectionResult {
        if self.lines.is_empty() {
            return (String::new(), Vec::new());
        }

        let idx0 = match self.find_closest_line_index(p0) {
            Some(i) => i,
            None => return (String::new(), Vec::new()),
        };
        let idx1 = match self.find_closest_line_index(p1) {
            Some(i) => i,
            None => return (String::new(), Vec::new()),
        };

        let (start_idx, end_idx, start_pt, end_pt) = if idx0 < idx1 {
            (idx0, idx1, p0, p1)
        } else if idx1 < idx0 {
            (idx1, idx0, p1, p0)
        } else {
            // Same line: order points horizontally
            if p0.0 <= p1.0 {
                (idx0, idx0, p0, p1)
            } else {
                (idx0, idx0, p1, p0)
            }
        };

        let start_line = &self.lines[start_idx];
        let end_line = &self.lines[end_idx];

        let sl_x0 = start_line.x0.min(start_line.x1);
        let sl_x1 = start_line.x0.max(start_line.x1);
        let el_x0 = end_line.x0.min(end_line.x1);
        let el_x1 = end_line.x0.max(end_line.x1);

        let sl_center = (sl_x0 + sl_x1) * 0.5;
        let el_center = (el_x0 + el_x1) * 0.5;

        // Check whether start and end lines are within the same column or content block.
        // They share a column if their horizontal spans overlap or their horizontal centers align closely.
        let horiz_overlap = sl_x0 < el_x1 + 12.0 && el_x0 < sl_x1 + 12.0;
        let center_close = (sl_center - el_center).abs() < (sl_x1 - sl_x0).max(el_x1 - el_x0) * 0.6;
        let is_same_column = horiz_overlap || center_close;

        let mut col_min_x = sl_x0.min(el_x0) - 20.0;
        let mut col_max_x = sl_x1.max(el_x1) + 20.0;

        let mut selected_lines = Vec::new();
        let mut highlight_rects = Vec::new();

        for line_idx in start_idx..=end_idx {
            let line = &self.lines[line_idx];
            let line_x0 = line.x0.min(line.x1);
            let line_x1 = line.x0.max(line.x1);
            let line_center = (line_x0 + line_x1) * 0.5;

            // When selecting within a single column, skip any interleaved or parallel
            // lines belonging to adjacent columns or sidebars.
            if is_same_column && (line_center < col_min_x || line_center > col_max_x) {
                continue;
            }

            if is_same_column {
                col_min_x = col_min_x.min(line_x0 - 20.0);
                col_max_x = col_max_x.max(line_x1 + 20.0);
            }

            let is_first_line = line_idx == start_idx;
            let is_last_line = line_idx == end_idx;

            let mut line_chars = Vec::new();
            let mut line_rect_x0 = f32::MAX;
            let mut line_rect_x1 = f32::MIN;
            let mut line_rect_y0 = line.y0.min(line.y1);
            let mut line_rect_y1 = line.y0.max(line.y1);

            for ch in &line.chars {
                let cx0 = ch.x0.min(ch.x1);
                let cx1 = ch.x0.max(ch.x1);
                let cy0 = ch.y0.min(ch.y1);
                let cy1 = ch.y0.max(ch.y1);
                let mid_x = (cx0 + cx1) * 0.5;

                let selected = match (is_first_line, is_last_line) {
                    (true, true) => mid_x >= start_pt.0.min(end_pt.0) && mid_x <= start_pt.0.max(end_pt.0),
                    (true, false) => mid_x >= start_pt.0,
                    (false, true) => mid_x <= end_pt.0,
                    (false, false) => true,
                };

                if selected {
                    line_chars.push(ch.ch);
                    line_rect_x0 = line_rect_x0.min(cx0);
                    line_rect_x1 = line_rect_x1.max(cx1);
                    line_rect_y0 = line_rect_y0.min(cy0);
                    line_rect_y1 = line_rect_y1.max(cy1);
                }
            }

            if line.chars.is_empty() {
                let selected = match (is_first_line, is_last_line) {
                    (true, true) => line_x0 <= end_pt.0 && line_x1 >= start_pt.0,
                    (true, false) => line_x1 >= start_pt.0,
                    (false, true) => line_x0 <= end_pt.0,
                    (false, false) => true,
                };
                if selected {
                    selected_lines.push(line.text.clone());
                    highlight_rects.push((line_x0, line.y0.min(line.y1), line_x1, line.y0.max(line.y1)));
                }
            } else if !line_chars.is_empty() {
                let line_str: String = line_chars.into_iter().collect();
                selected_lines.push(line_str);
                highlight_rects.push((line_rect_x0, line_rect_y0, line_rect_x1, line_rect_y1));
            }
        }

        let result_text = selected_lines.join(" ").trim().to_string();
        (result_text, highlight_rects)
    }

    /// Select text inside an exact rectangular bounding box (for Alt+Drag block marquee selection).
    /// Returns:
    /// - Selected text string (lines separated by newlines)
    /// - List of highlight bounding boxes `(x0, y0, x1, y1)` in document points
    #[allow(dead_code)]
    pub fn select_rect(
        &self,
        p0: (f32, f32),
        p1: (f32, f32),
    ) -> PdfSelectionResult {
        if self.lines.is_empty() {
            return (String::new(), Vec::new());
        }

        let min_x = p0.0.min(p1.0);
        let max_x = p0.0.max(p1.0);
        let min_y = p0.1.min(p1.1);
        let max_y = p0.1.max(p1.1);

        let mut selected_lines = Vec::new();
        let mut highlight_rects = Vec::new();

        for line in &self.lines {
            let line_top = line.y0.min(line.y1);
            let line_bottom = line.y0.max(line.y1);

            // Skip lines outside vertical box bounds (with 1pt tolerance)
            if line_bottom < min_y - 1.0 || line_top > max_y + 1.0 {
                continue;
            }

            let mut line_selected_chars = Vec::new();
            let mut line_min_x = f32::MAX;
            let mut line_max_x = f32::MIN;
            let mut line_min_y = f32::MAX;
            let mut line_max_y = f32::MIN;

            for ch in &line.chars {
                let ch_min_x = ch.x0.min(ch.x1);
                let ch_max_x = ch.x0.max(ch.x1);
                let ch_min_y = ch.y0.min(ch.y1);
                let ch_max_y = ch.y0.max(ch.y1);
                let ch_mid_x = (ch_min_x + ch_max_x) * 0.5;
                let ch_mid_y = (ch_min_y + ch_max_y) * 0.5;

                if ch_mid_x >= min_x && ch_mid_x <= max_x && ch_mid_y >= min_y && ch_mid_y <= max_y {
                    line_selected_chars.push(ch.ch);
                    line_min_x = line_min_x.min(ch_min_x);
                    line_max_x = line_max_x.max(ch_max_x);
                    line_min_y = line_min_y.min(ch_min_y);
                    line_max_y = line_max_y.max(ch_max_y);
                }
            }

            if line.chars.is_empty() {
                let lx0 = line.x0.min(line.x1);
                let lx1 = line.x0.max(line.x1);
                if lx1 >= min_x && lx0 <= max_x {
                    selected_lines.push(line.text.clone());
                    highlight_rects.push((lx0.max(min_x), line_top.max(min_y), lx1.min(max_x), line_bottom.min(max_y)));
                }
            } else if !line_selected_chars.is_empty() {
                let line_str: String = line_selected_chars.into_iter().collect();
                selected_lines.push(line_str);
                highlight_rects.push((line_min_x, line_min_y, line_max_x, line_max_y));
            }
        }

        (selected_lines.join("\n").trim().to_string(), highlight_rects)
    }

    /// Find word under point in document coordinates (PDF points).
    /// Returns word text and bounding box (x0, y0, x1, y1).
    #[allow(dead_code)]
    pub fn word_at(&self, p: (f32, f32)) -> Option<PdfSelectionResult> {
        for line in &self.lines {
            let line_top = line.y0.min(line.y1) - 4.0;
            let line_bottom = line.y0.max(line.y1) + 4.0;
            if p.1 < line_top || p.1 > line_bottom {
                continue;
            }

            if let Some(char_idx) = line.chars.iter().position(|c| {
                let min_x = c.x0.min(c.x1) - 2.0;
                let max_x = c.x0.max(c.x1) + 2.0;
                p.0 >= min_x && p.0 <= max_x
            }) {
                if line.chars[char_idx].ch.is_whitespace() {
                    continue;
                }

                let mut start_idx = char_idx;
                while start_idx > 0 && !line.chars[start_idx - 1].ch.is_whitespace() {
                    start_idx -= 1;
                }

                let mut end_idx = char_idx;
                while end_idx + 1 < line.chars.len() && !line.chars[end_idx + 1].ch.is_whitespace() {
                    end_idx += 1;
                }

                let mut word_text = String::new();
                let mut min_x = f32::MAX;
                let mut max_x = f32::MIN;
                let mut min_y = line.y0.min(line.y1);
                let mut max_y = line.y0.max(line.y1);

                for c in &line.chars[start_idx..=end_idx] {
                    word_text.push(c.ch);
                    min_x = min_x.min(c.x0.min(c.x1));
                    max_x = max_x.max(c.x0.max(c.x1));
                    min_y = min_y.min(c.y0.min(c.y1));
                    max_y = max_y.max(c.y0.max(c.y1));
                }

                let trimmed = word_text.trim_matches(|c: char| c.is_ascii_punctuation() && c != '\'' && c != '-');
                if !trimmed.is_empty() {
                    return Some((trimmed.to_string(), vec![(min_x, min_y, max_x, max_y)]));
                }
            }
        }
        None
    }

    /// Find entire line under point in document coordinates (PDF points).
    #[allow(dead_code)]
    pub fn line_at(&self, p: (f32, f32)) -> Option<PdfSelectionResult> {
        for line in &self.lines {
            let line_top = line.y0.min(line.y1) - 4.0;
            let line_bottom = line.y0.max(line.y1) + 4.0;
            if p.1 >= line_top && p.1 <= line_bottom {
                let trimmed = line.text.trim().to_string();
                if !trimmed.is_empty() {
                    return Some((
                        trimmed,
                        vec![(
                            line.x0.min(line.x1),
                            line.y0.min(line.y1),
                            line.x0.max(line.x1),
                            line.y0.max(line.y1),
                        )],
                    ));
                }
            }
        }
        None
    }
}

/// Ink bounding box representing content boundaries in normalized (0.0 .. 1.0) coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InkBoundingBox {
    pub min_x: f32, // 0.0 .. 1.0
    pub min_y: f32, // 0.0 .. 1.0
    pub max_x: f32, // 0.0 .. 1.0
    pub max_y: f32, // 0.0 .. 1.0
}

impl Default for InkBoundingBox {
    fn default() -> Self {
        Self {
            min_x: 0.0,
            min_y: 0.0,
            max_x: 1.0,
            max_y: 1.0,
        }
    }
}

impl InkBoundingBox {
    pub fn new(min_x: f32, min_y: f32, max_x: f32, max_y: f32) -> Self {
        Self {
            min_x: min_x.clamp(0.0, 1.0),
            min_y: min_y.clamp(0.0, 1.0),
            max_x: max_x.clamp(0.0, 1.0),
            max_y: max_y.clamp(0.0, 1.0),
        }
    }

    pub fn width_fraction(&self) -> f32 {
        (self.max_x - self.min_x).max(0.01)
    }

    pub fn height_fraction(&self) -> f32 {
        (self.max_y - self.min_y).max(0.01)
    }
}

/// Flattened table of contents item extracted from document outline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfTocEntry {
    pub title: String,
    pub page: usize, // 1-based page number
    pub depth: usize,
}

/// Raw RGBA rasterized page buffer.
#[derive(Clone, Debug)]
pub struct RenderedPage {
    pub page_num: usize,
    pub width: i32,
    pub height: i32,
    pub stride: usize,
    pub samples: Vec<u8>,
}

/// MuPDF Document handle.
pub struct PdfDocument {
    #[allow(dead_code)]
    pub path: PathBuf,
    doc: Document,
    page_count: usize,
}

impl PdfDocument {
    /// Load a PDF document from a file path using MuPDF.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let path_str = path_buf.to_string_lossy();
        let doc = Document::open(&*path_str)
            .map_err(|e| anyhow!("Failed to load PDF document {:?}: {}", path_buf, e))?;

        let count = doc
            .page_count()
            .map_err(|e| anyhow!("Failed to count pages in {:?}: {}", path_buf, e))?;

        if count <= 0 {
            return Err(anyhow!("PDF document has no pages"));
        }

        Ok(Self {
            path: path_buf,
            doc,
            page_count: count as usize,
        })
    }

    /// Return total page count.
    pub fn page_count(&self) -> usize {
        self.page_count
    }

    /// Return page dimensions (width, height in points) at 72 DPI.
    #[allow(dead_code)]
    pub fn page_dimensions(&self, page_num: usize) -> Result<(f32, f32)> {
        if page_num == 0 || page_num > self.page_count {
            return Err(anyhow!("Page number {} out of range", page_num));
        }

        let page = self
            .doc
            .load_page((page_num - 1) as i32)
            .map_err(|e| anyhow!("Failed to load page {}: {}", page_num, e))?;

        let rect = page
            .bounds()
            .map_err(|e| anyhow!("Failed to read page {} bounds: {}", page_num, e))?;

        Ok((rect.width().abs(), rect.height().abs()))
    }

    /// Extract raw text layer from a PDF page (1-indexed) using MuPDF.
    #[allow(dead_code)]
    pub fn extract_raw_text(&self, page_num: usize) -> Result<String> {
        if page_num == 0 || page_num > self.page_count {
            return Err(anyhow!("Page number {} out of range", page_num));
        }

        let page = self
            .doc
            .load_page((page_num - 1) as i32)
            .map_err(|e| anyhow!("Failed to load page {}: {}", page_num, e))?;

        let text = page
            .text(TextExtractOptions::default())
            .map_err(|e| anyhow!("Failed to extract text from page {}: {}", page_num, e))?;

        Ok(text)
    }

    /// Extract structured text with character and line bounding boxes from a page (1-indexed).
    #[allow(dead_code)]
    pub fn extract_page_text(&self, page_num: usize) -> Result<PdfPageText> {
        if page_num == 0 || page_num > self.page_count {
            return Err(anyhow!("Page number {} out of range", page_num));
        }

        let page = self
            .doc
            .load_page((page_num - 1) as i32)
            .map_err(|e| anyhow!("Failed to load page {}: {}", page_num, e))?;

        let bounds = page
            .bounds()
            .unwrap_or_else(|_| Rect::new(0.0, 0.0, 612.0, 792.0));

        let text_page = page
            .to_text_page(TextPageFlags::empty())
            .map_err(|e| anyhow!("Failed to extract structured text: {}", e))?;

        let structured = text_page.structured();
        let mut lines = Vec::new();

        for block in structured.blocks {
            if let TextBlockContent::Text { lines: block_lines } = block.content {
                for line in block_lines {
                    let chars: Vec<PdfTextChar> = line
                        .chars
                        .into_iter()
                        .map(|c| {
                            let x0 = c.quad.ul.x.min(c.quad.ur.x).min(c.quad.ll.x).min(c.quad.lr.x);
                            let y0 = c.quad.ul.y.min(c.quad.ur.y).min(c.quad.ll.y).min(c.quad.lr.y);
                            let x1 = c.quad.ul.x.max(c.quad.ur.x).max(c.quad.ll.x).max(c.quad.lr.x);
                            let y1 = c.quad.ul.y.max(c.quad.ur.y).max(c.quad.ll.y).max(c.quad.lr.y);
                            PdfTextChar {
                                ch: c.ch,
                                x0,
                                y0,
                                x1,
                                y1,
                            }
                        })
                        .collect();

                    lines.push(PdfTextLine {
                        text: line.text,
                        x0: line.bounds.x0,
                        y0: line.bounds.y0,
                        x1: line.bounds.x1,
                        y1: line.bounds.y1,
                        chars,
                    });
                }
            }
        }

        Ok(PdfPageText {
            page_num,
            width_pts: bounds.width(),
            height_pts: bounds.height(),
            lines,
        })
    }

    /// Extract hierarchical table of contents (outlines) from the PDF document.
    pub fn outlines(&self) -> Result<Vec<PdfTocEntry>> {
        let mut entries = Vec::new();
        if let Ok(roots) = self.doc.outlines() {
            for root in &roots {
                collect_outline(root, 0, &mut entries);
            }
        }
        Ok(entries)
    }

    /// Whether this page carries visual content.
    /// With MuPDF rendering every page into crisp native visuals, every page has a picture.
    #[allow(dead_code)]
    pub fn page_has_picture(&self, _page_num: usize) -> bool {
        true
    }

    /// Rasterize a PDF page to raw RGBA bytes with optional Smart Crop applied.
    pub fn render_page_rgba(
        &self,
        page_num: usize,
        scale: f32,
        smart_crop: bool,
    ) -> Result<RenderedPage> {
        if page_num == 0 || page_num > self.page_count {
            return Err(anyhow!("Page number {} out of range", page_num));
        }

        let page = self
            .doc
            .load_page((page_num - 1) as i32)
            .map_err(|e| anyhow!("Failed to load page {}: {}", page_num, e))?;

        let scale_clamped = scale.clamp(0.2, 4.0);
        let matrix = Matrix::new_scale(scale_clamped, scale_clamped);
        let pixmap = page
            .to_pixmap(&matrix, &Colorspace::device_rgb(), true, true)
            .map_err(|e| anyhow!("Failed to rasterize page {}: {}", page_num, e))?;

        let width = pixmap.width();
        let height = pixmap.height();
        if width == 0 || height == 0 {
            return Err(anyhow!("Rendered page {page_num} has empty dimensions"));
        }

        let w_usize = width as usize;
        let h_usize = height as usize;
        let raw_samples = pixmap.samples();
        let raw_stride = pixmap.stride();

        let tight_raw = if raw_stride == (w_usize * 4) as isize && raw_samples.len() >= w_usize * h_usize * 4 {
            raw_samples[..w_usize * h_usize * 4].to_vec()
        } else if raw_stride > 0 {
            let row_stride = raw_stride as usize;
            let mut buf = Vec::with_capacity(w_usize * h_usize * 4);
            for y in 0..h_usize {
                let start = y * row_stride;
                let end = start + w_usize * 4;
                if end <= raw_samples.len() {
                    buf.extend_from_slice(&raw_samples[start..end]);
                } else if start < raw_samples.len() {
                    buf.extend_from_slice(&raw_samples[start..]);
                    buf.resize(buf.len() + (end - raw_samples.len()), 255);
                } else {
                    buf.resize(buf.len() + w_usize * 4, 255);
                }
            }
            buf
        } else {
            let row_stride = raw_stride.unsigned_abs();
            let mut buf = Vec::with_capacity(w_usize * h_usize * 4);
            for y in 0..h_usize {
                let start = (h_usize - 1 - y) * row_stride;
                let end = start + w_usize * 4;
                if end <= raw_samples.len() {
                    buf.extend_from_slice(&raw_samples[start..end]);
                } else if start < raw_samples.len() {
                    buf.extend_from_slice(&raw_samples[start..]);
                    buf.resize(buf.len() + (end - raw_samples.len()), 255);
                } else {
                    buf.resize(buf.len() + w_usize * 4, 255);
                }
            }
            buf
        };

        if smart_crop {
            if let Some(rgba) = RgbaImage::from_raw(width, height, tight_raw.clone()) {
                let dyn_img = DynamicImage::ImageRgba8(rgba);
                let ink_box = Self::calculate_ink_box_for_image(&dyn_img);
                let x = (ink_box.min_x * width as f32) as u32;
                let y = (ink_box.min_y * height as f32) as u32;
                let crop_w = (ink_box.width_fraction() * width as f32) as u32;
                let crop_h = (ink_box.height_fraction() * height as f32) as u32;

                if crop_w > 0 && crop_h > 0 && (crop_w < width || crop_h < height) {
                    let cropped = dyn_img.crop_imm(x, y, crop_w, crop_h).to_rgba8();
                    let c_w = cropped.width() as i32;
                    let c_h = cropped.height() as i32;
                    let c_stride = (c_w * 4) as usize;
                    return Ok(RenderedPage {
                        page_num,
                        width: c_w,
                        height: c_h,
                        stride: c_stride,
                        samples: cropped.into_raw(),
                    });
                }
            }
        }

        let w = width as i32;
        let h = height as i32;
        let stride = (w * 4) as usize;

        Ok(RenderedPage {
            page_num,
            width: w,
            height: h,
            stride,
            samples: tight_raw,
        })
    }

    /// Render uncropped raw page image (1-indexed) as a DynamicImage.
    #[allow(dead_code)]
    pub fn render_page_image(&self, page_num: usize, smart_crop: bool) -> Result<DynamicImage> {
        let rendered = self.render_page_rgba(page_num, 1.5, smart_crop)?;
        let rgba = RgbaImage::from_raw(
            rendered.width as u32,
            rendered.height as u32,
            rendered.samples,
        )
        .ok_or_else(|| anyhow!("Failed to build RGBA image from rendered page samples"))?;
        Ok(DynamicImage::ImageRgba8(rgba))
    }

    /// Calculate Zathura-Style Smart Crop ink bounding box for a page image.
    ///
    /// Scans pixel contents against the sampled page background color to find min/max ink coordinates,
    /// with generous reading margin padding so fonts, ascenders, descenders, and page numbers are never clipped.
    pub fn calculate_ink_box_for_image(img: &DynamicImage) -> InkBoundingBox {
        let (width, height) = img.dimensions();
        if width == 0 || height == 0 {
            return InkBoundingBox::default();
        }

        let rgba = img.to_rgba8();

        // Sample corner pixels to determine background paper luminance (handles white, cream, off-white)
        let sample_pts = [
            (2.min(width.saturating_sub(1)), 2.min(height.saturating_sub(1))),
            (width.saturating_sub(3), 2.min(height.saturating_sub(1))),
            (2.min(width.saturating_sub(1)), height.saturating_sub(3)),
            (width.saturating_sub(3), height.saturating_sub(3)),
        ];
        let mut bg_lum_sum = 0.0f32;
        let mut bg_count = 0;
        for (sx, sy) in sample_pts {
            let p = rgba.get_pixel(sx, sy);
            if p[3] > 20 {
                let l = 0.299 * p[0] as f32 + 0.587 * p[1] as f32 + 0.114 * p[2] as f32;
                bg_lum_sum += l;
                bg_count += 1;
            }
        }
        let bg_lum = if bg_count > 0 {
            bg_lum_sum / bg_count as f32
        } else {
            255.0
        };

        let mut min_x = width;
        let mut min_y = height;
        let mut max_x = 0;
        let mut max_y = 0;
        let mut found_ink = false;

        for y in 0..height {
            for x in 0..width {
                let pixel = rgba.get_pixel(x, y);
                let r = pixel[0] as f32;
                let g = pixel[1] as f32;
                let b = pixel[2] as f32;
                let a = pixel[3] as u32;

                let lum = 0.299 * r + 0.587 * g + 0.114 * b;

                // Ink check: detect any pixel that noticeably departs from background paper color,
                // or any anti-aliased glyph stroke. Sensitive enough to capture light grey headers,
                // footnote markers, subtle section titles, and subpixel font edges.
                let diff = (bg_lum - lum).abs();
                if (a > 20 && diff > 10.0) || (a > 20 && lum < 242.0) {
                    found_ink = true;
                    if x < min_x {
                        min_x = x;
                    }
                    if x > max_x {
                        max_x = x;
                    }
                    if y < min_y {
                        min_y = y;
                    }
                    if y > max_y {
                        max_y = y;
                    }
                }
            }
        }

        if !found_ink || min_x >= max_x || min_y >= max_y {
            return InkBoundingBox::default();
        }

        // Add 6% margin padding (min 56px) so text, ascenders, descenders, headers, and page numbers
        // always preserve generous breathing room and are never cut off.
        let pad_x = ((width as f32) * 0.06).max(56.0) as u32;
        let pad_y = ((height as f32) * 0.06).max(56.0) as u32;

        let crop_min_x = min_x.saturating_sub(pad_x) as f32 / width as f32;
        let crop_min_y = min_y.saturating_sub(pad_y) as f32 / height as f32;
        let crop_max_x = (max_x + pad_x).min(width) as f32 / width as f32;
        let crop_max_y = (max_y + pad_y).min(height) as f32 / height as f32;

        InkBoundingBox::new(crop_min_x, crop_min_y, crop_max_x, crop_max_y)
    }

    /// Run heuristic paragraph boundary detection over extracted raw text lines.
    #[allow(dead_code)]
    pub fn reflow_text(raw_text: &str) -> Vec<String> {
        if raw_text.trim().is_empty() {
            return Vec::new();
        }

        let lines: Vec<&str> = raw_text.lines().map(|l| l.trim()).collect();
        let mut paragraphs: Vec<String> = Vec::new();
        let mut current_para = String::new();

        for line in lines {
            if line.is_empty() {
                if !current_para.is_empty() {
                    paragraphs.push(current_para.trim().to_string());
                    current_para.clear();
                }
                continue;
            }

            // Skip page numbers or standalone headers like "Page 1"
            if is_header_or_footer(line) {
                continue;
            }

            if current_para.is_empty() {
                current_para.push_str(line);
            } else if current_para.ends_with('-') && !current_para.ends_with(" -") {
                current_para.pop(); // remove hyphenation
                current_para.push_str(line);
            } else {
                // Line continuation in same paragraph
                current_para.push(' ');
                current_para.push_str(line);
            }
        }

        if !current_para.is_empty() {
            paragraphs.push(current_para.trim().to_string());
        }

        paragraphs
    }
}

/// Helper to determine if a TOC entry title represents a top-level structural division (Chapter, Part, etc.).
pub fn is_top_level_title(title: &str) -> bool {
    let lower = title.trim().to_lowercase();
    lower.starts_with("chapter")
        || lower.starts_with("part")
        || lower.starts_with("book")
        || lower.starts_with("act")
        || lower.starts_with("canto")
        || lower.starts_with("preface")
        || lower.starts_with("prologue")
        || lower.starts_with("epilogue")
        || lower.starts_with("introduction")
        || lower.starts_with("appendix")
        || lower.starts_with("contents")
        || lower.starts_with("table of contents")
        || lower.starts_with("conclusion")
        || lower.starts_with("glossary")
        || lower.starts_with("bibliography")
        || lower.starts_with("index")
}

/// Recursive helper to collect table of contents outline items.
fn collect_outline(item: &Outline, depth: usize, out: &mut Vec<PdfTocEntry>) {
    let title = item.title.trim();
    if !title.is_empty() {
        let is_top = is_top_level_title(title);
        let effective_depth = if is_top { 0 } else { depth };
        let page_1_based = item
            .dest
            .as_ref()
            .map(|d| (d.loc.page_number + 1) as usize)
            .unwrap_or(1);
        out.push(PdfTocEntry {
            title: title.to_string(),
            page: page_1_based,
            depth: effective_depth,
        });
        let child_depth = if is_top { 1 } else { effective_depth + 1 };
        for child in &item.down {
            collect_outline(child, child_depth, out);
        }
    } else {
        for child in &item.down {
            collect_outline(child, depth, out);
        }
    }
}

/// Helper function to check if a text line is a header/footer or page number.
#[allow(dead_code)]
fn is_header_or_footer(line: &str) -> bool {
    let lower = line.to_lowercase();
    if lower.starts_with("page ") || lower.starts_with("chapter ") {
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() <= 3 && parts.get(1).is_some_and(|p| p.chars().all(|c| c.is_numeric())) {
            return true;
        }
    }
    line.chars().all(|c| c.is_numeric() || c == '-') && line.len() <= 5
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;

    #[test]
    fn test_ink_bounding_box_defaults() {
        let box_default = InkBoundingBox::default();
        assert_eq!(box_default.min_x, 0.0);
        assert_eq!(box_default.max_x, 1.0);
        assert_eq!(box_default.width_fraction(), 1.0);
    }

    #[test]
    fn test_calculate_ink_box_for_blank_and_content_image() {
        let blank_img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(
            100,
            100,
            Rgba([255, 255, 255, 255]),
        ));
        let ink_box = PdfDocument::calculate_ink_box_for_image(&blank_img);
        assert_eq!(ink_box, InkBoundingBox::default());

        let mut content_img =
            RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
        // Draw ink rect from x=20..80, y=30..70
        for y in 30..70 {
            for x in 20..80 {
                content_img.put_pixel(x, y, Rgba([0, 0, 0, 255]));
            }
        }
        let dyn_img = DynamicImage::ImageRgba8(content_img);
        let ink_box = PdfDocument::calculate_ink_box_for_image(&dyn_img);

        assert!(ink_box.min_x <= 0.20);
        assert!(ink_box.max_x >= 0.79);
        assert!(ink_box.min_y <= 0.30);
        assert!(ink_box.max_y >= 0.69);
    }

    #[test]
    fn reflow_joins_the_lines_of_one_paragraph() {
        assert_eq!(
            PdfDocument::reflow_text("A line that ends\nand another that finishes it."),
            vec!["A line that ends and another that finishes it.".to_string()]
        );
    }

    #[test]
    fn reflow_splits_on_a_blank_line() {
        assert_eq!(
            PdfDocument::reflow_text("First paragraph\nstill first\n\nSecond paragraph"),
            vec![
                "First paragraph still first".to_string(),
                "Second paragraph".to_string()
            ]
        );
    }

    #[test]
    fn reflow_mends_a_hyphen_broken_word() {
        assert_eq!(
            PdfDocument::reflow_text("The author was being delib-\nerate about it."),
            vec!["The author was being deliberate about it.".to_string()]
        );
        // A dash at the end of a sentence is punctuation, not a break.
        assert_eq!(
            PdfDocument::reflow_text("He said -\nand meant it."),
            vec!["He said - and meant it.".to_string()]
        );
    }

    #[test]
    fn reflow_drops_page_furniture() {
        assert_eq!(
            PdfDocument::reflow_text("Chapter 3\nThe real text.\n42"),
            vec!["The real text.".to_string()]
        );
    }

    #[test]
    fn reflow_of_nothing_is_nothing() {
        assert!(PdfDocument::reflow_text("   ").is_empty());
    }

    #[test]
    fn test_reflow_text_heuristics() {
        let raw = "This is a sentence that is split across two-\nlines due to PDF formatting.\n\nThis is a second paragraph.\nIt continues here.";
        let reflowed = PdfDocument::reflow_text(raw);

        assert_eq!(reflowed.len(), 2);
        assert!(reflowed[0].contains("across twolines due to PDF formatting."));
        assert_eq!(reflowed[1], "This is a second paragraph. It continues here.");
    }

    #[test]
    fn test_pdf_page_text_select_between() {
        let page_text = PdfPageText {
            page_num: 1,
            width_pts: 600.0,
            height_pts: 800.0,
            lines: vec![
                PdfTextLine {
                    text: "Hello world".to_string(),
                    x0: 50.0,
                    y0: 100.0,
                    x1: 150.0,
                    y1: 120.0,
                    chars: vec![
                        PdfTextChar { ch: 'H', x0: 50.0, y0: 100.0, x1: 60.0, y1: 120.0 },
                        PdfTextChar { ch: 'e', x0: 60.0, y0: 100.0, x1: 70.0, y1: 120.0 },
                        PdfTextChar { ch: 'l', x0: 70.0, y0: 100.0, x1: 75.0, y1: 120.0 },
                        PdfTextChar { ch: 'l', x0: 75.0, y0: 100.0, x1: 80.0, y1: 120.0 },
                        PdfTextChar { ch: 'o', x0: 80.0, y0: 100.0, x1: 90.0, y1: 120.0 },
                        PdfTextChar { ch: ' ', x0: 90.0, y0: 100.0, x1: 95.0, y1: 120.0 },
                        PdfTextChar { ch: 'w', x0: 95.0, y0: 100.0, x1: 110.0, y1: 120.0 },
                        PdfTextChar { ch: 'o', x0: 110.0, y0: 100.0, x1: 120.0, y1: 120.0 },
                        PdfTextChar { ch: 'r', x0: 120.0, y0: 100.0, x1: 130.0, y1: 120.0 },
                        PdfTextChar { ch: 'l', x0: 130.0, y0: 100.0, x1: 135.0, y1: 120.0 },
                        PdfTextChar { ch: 'd', x0: 135.0, y0: 100.0, x1: 145.0, y1: 120.0 },
                    ],
                },
            ],
        };

        let (text, rects) = page_text.select_between((50.0, 105.0), (90.0, 105.0));
        assert_eq!(text, "Hello");
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].0, 50.0);
        assert_eq!(rects[0].2, 90.0);
    }

    #[test]
    fn test_pdf_page_text_word_at() {
        let page_text = PdfPageText {
            page_num: 1,
            width_pts: 600.0,
            height_pts: 800.0,
            lines: vec![
                PdfTextLine {
                    text: "Quick brown fox".to_string(),
                    x0: 50.0,
                    y0: 100.0,
                    x1: 200.0,
                    y1: 120.0,
                    chars: vec![
                        PdfTextChar { ch: 'Q', x0: 50.0, y0: 100.0, x1: 60.0, y1: 120.0 },
                        PdfTextChar { ch: 'u', x0: 60.0, y0: 100.0, x1: 70.0, y1: 120.0 },
                        PdfTextChar { ch: 'i', x0: 70.0, y0: 100.0, x1: 75.0, y1: 120.0 },
                        PdfTextChar { ch: 'c', x0: 75.0, y0: 100.0, x1: 85.0, y1: 120.0 },
                        PdfTextChar { ch: 'k', x0: 85.0, y0: 100.0, x1: 95.0, y1: 120.0 },
                        PdfTextChar { ch: ' ', x0: 95.0, y0: 100.0, x1: 100.0, y1: 120.0 },
                        PdfTextChar { ch: 'b', x0: 100.0, y0: 100.0, x1: 110.0, y1: 120.0 },
                        PdfTextChar { ch: 'r', x0: 110.0, y0: 100.0, x1: 120.0, y1: 120.0 },
                        PdfTextChar { ch: 'o', x0: 120.0, y0: 100.0, x1: 130.0, y1: 120.0 },
                        PdfTextChar { ch: 'w', x0: 130.0, y0: 100.0, x1: 145.0, y1: 120.0 },
                        PdfTextChar { ch: 'n', x0: 145.0, y0: 100.0, x1: 155.0, y1: 120.0 },
                    ],
                },
            ],
        };

        let result = page_text.word_at((125.0, 110.0));
        assert!(result.is_some());
        let (word, rects) = result.unwrap();
        assert_eq!(word, "brown");
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].0, 100.0);
        assert_eq!(rects[0].2, 155.0);
    }

    #[test]
    fn test_pdf_page_text_line_at() {
        let page_text = PdfPageText {
            page_num: 1,
            width_pts: 600.0,
            height_pts: 800.0,
            lines: vec![
                PdfTextLine {
                    text: "Single complete line.".to_string(),
                    x0: 50.0,
                    y0: 100.0,
                    x1: 200.0,
                    y1: 120.0,
                    chars: vec![],
                },
            ],
        };

        let result = page_text.line_at((100.0, 110.0));
        assert!(result.is_some());
        let (line, rects) = result.unwrap();
        assert_eq!(line, "Single complete line.");
        assert_eq!(rects.len(), 1);
        assert_eq!(rects[0].0, 50.0);
        assert_eq!(rects[0].2, 200.0);
    }

    #[test]
    fn test_pdf_page_text_column_aware_selection() {
        // Multi-column page: Column 1 on left (x=50..250), STARFACT sidebar on right (x=350..500)
        let page_text = PdfPageText {
            page_num: 1,
            width_pts: 600.0,
            height_pts: 800.0,
            lines: vec![
                PdfTextLine {
                    text: "Aristotle wrote books.".to_string(),
                    x0: 50.0,
                    y0: 100.0,
                    x1: 220.0,
                    y1: 118.0,
                    chars: "Aristotle wrote books."
                        .chars()
                        .enumerate()
                        .map(|(i, ch)| PdfTextChar {
                            ch,
                            x0: 50.0 + (i as f32 * 7.5),
                            y0: 100.0,
                            x1: 50.0 + ((i + 1) as f32 * 7.5),
                            y1: 118.0,
                        })
                        .collect(),
                },
                PdfTextLine {
                    text: "STARFACT: Stars shine.".to_string(),
                    x0: 350.0,
                    y0: 100.0,
                    x1: 490.0,
                    y1: 118.0,
                    chars: "STARFACT: Stars shine."
                        .chars()
                        .enumerate()
                        .map(|(i, ch)| PdfTextChar {
                            ch,
                            x0: 350.0 + (i as f32 * 6.5),
                            y0: 100.0,
                            x1: 350.0 + ((i + 1) as f32 * 6.5),
                            y1: 118.0,
                        })
                        .collect(),
                },
                PdfTextLine {
                    text: "He studied natural philosophy.".to_string(),
                    x0: 50.0,
                    y0: 125.0,
                    x1: 245.0,
                    y1: 143.0,
                    chars: "He studied natural philosophy."
                        .chars()
                        .enumerate()
                        .map(|(i, ch)| PdfTextChar {
                            ch,
                            x0: 50.0 + (i as f32 * 6.5),
                            y0: 125.0,
                            x1: 50.0 + ((i + 1) as f32 * 6.5),
                            y1: 143.0,
                        })
                        .collect(),
                },
            ],
        };

        // Drag down Column 1 across lines 1 and 3
        let (text, rects) = page_text.select_between((50.0, 105.0), (250.0, 135.0));
        assert!(text.contains("Aristotle wrote books."));
        assert!(text.contains("He studied natural philosophy."));
        // Critically: STARFACT sidebar at the same vertical height MUST NOT be included!
        assert!(!text.contains("STARFACT"));
        assert_eq!(rects.len(), 2);
    }

    #[test]
    fn test_pdf_page_text_select_rect_block_mode() {
        let page_text = PdfPageText {
            page_num: 1,
            width_pts: 600.0,
            height_pts: 800.0,
            lines: vec![
                PdfTextLine {
                    text: "Col 1 line 1".to_string(),
                    x0: 50.0,
                    y0: 100.0,
                    x1: 150.0,
                    y1: 120.0,
                    chars: vec![
                        PdfTextChar { ch: 'C', x0: 50.0, y0: 100.0, x1: 60.0, y1: 120.0 },
                        PdfTextChar { ch: '1', x0: 60.0, y0: 100.0, x1: 70.0, y1: 120.0 },
                    ],
                },
                PdfTextLine {
                    text: "Box line 1".to_string(),
                    x0: 300.0,
                    y0: 100.0,
                    x1: 400.0,
                    y1: 120.0,
                    chars: vec![
                        PdfTextChar { ch: 'B', x0: 300.0, y0: 100.0, x1: 310.0, y1: 120.0 },
                        PdfTextChar { ch: '1', x0: 310.0, y0: 100.0, x1: 320.0, y1: 120.0 },
                    ],
                },
                PdfTextLine {
                    text: "Box line 2".to_string(),
                    x0: 300.0,
                    y0: 130.0,
                    x1: 400.0,
                    y1: 150.0,
                    chars: vec![
                        PdfTextChar { ch: 'B', x0: 300.0, y0: 130.0, x1: 310.0, y1: 150.0 },
                        PdfTextChar { ch: '2', x0: 310.0, y0: 130.0, x1: 320.0, y1: 150.0 },
                    ],
                },
            ],
        };

        // Select exact rectangle over the box on the right
        let (text, rects) = page_text.select_rect((295.0, 95.0), (410.0, 155.0));
        assert_eq!(text, "B1\nB2");
        assert_eq!(rects.len(), 2);
        assert!(!text.contains("C1"));
    }
}
