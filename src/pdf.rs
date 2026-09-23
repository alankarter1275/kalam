//! PDF Engine Integration via MuPDF.
//!
//! Provides true MuPDF document parsing, high-fidelity page rasterization,
//! vector outline / TOC extraction, Zathura-style smart cropping, and text extraction.

use anyhow::{anyhow, Result};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use mupdf::{Colorspace, Document, Matrix, Outline, TextExtractOptions};
use std::path::{Path, PathBuf};

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
        let doc = Document::open(&path_buf)
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

    /// Extract hierarchical table of contents (outlines) from the PDF document.
    pub fn outlines(&self) -> Result<Vec<PdfTocEntry>> {
        let mut entries = Vec::new();
        if let Ok(Some(root)) = self.doc.outlines() {
            collect_outline(&root, 0, &mut entries);
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
        let raw = pixmap.samples().to_vec();

        if smart_crop {
            if let Some(rgba) = RgbaImage::from_raw(width, height, raw.clone()) {
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
        let stride = pixmap.stride() as usize;

        Ok(RenderedPage {
            page_num,
            width: w,
            height: h,
            stride,
            samples: raw,
        })
    }

    /// Render uncropped raw page image (1-indexed) as a DynamicImage.
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
    /// Scans pixel contents to find min/max ink coordinates, cropping out blank white margins.
    pub fn calculate_ink_box_for_image(img: &DynamicImage) -> InkBoundingBox {
        let (width, height) = img.dimensions();
        if width == 0 || height == 0 {
            return InkBoundingBox::default();
        }

        let mut min_x = width;
        let mut min_y = height;
        let mut max_x = 0;
        let mut max_y = 0;
        let mut found_ink = false;

        let rgba = img.to_rgba8();
        for y in 0..height {
            for x in 0..width {
                let pixel = rgba.get_pixel(x, y);
                // Check if pixel is "ink" (using luminance threshold < 225 out of 255)
                let r = pixel[0] as f32;
                let g = pixel[1] as f32;
                let b = pixel[2] as f32;
                let a = pixel[3] as u32;

                let lum = 0.299 * r + 0.587 * g + 0.114 * b;

                if a > 30 && lum < 225.0 {
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

        // Add 1% padding margin so content isn't clipped flush to ink edges
        let pad_x = ((width as f32) * 0.01).max(2.0) as u32;
        let pad_y = ((height as f32) * 0.01).max(2.0) as u32;

        let crop_min_x = min_x.saturating_sub(pad_x) as f32 / width as f32;
        let crop_min_y = min_y.saturating_sub(pad_y) as f32 / height as f32;
        let crop_max_x = (max_x + pad_x).min(width) as f32 / width as f32;
        let crop_max_y = (max_y + pad_y).min(height) as f32 / height as f32;

        InkBoundingBox::new(crop_min_x, crop_min_y, crop_max_x, crop_max_y)
    }

    /// Run heuristic paragraph boundary detection over extracted raw text lines.
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

/// Recursive helper to collect table of contents outline items.
fn collect_outline(item: &Outline, depth: usize, out: &mut Vec<PdfTocEntry>) {
    let title = item.title.trim();
    if !title.is_empty() {
        let page_1_based = item.page.map(|p| (p + 1) as usize).unwrap_or(1);
        out.push(PdfTocEntry {
            title: title.to_string(),
            page: page_1_based,
            depth,
        });
        for child in &item.down {
            collect_outline(child, depth + 1, out);
        }
    } else {
        for child in &item.down {
            collect_outline(child, depth, out);
        }
    }
}

/// Helper function to check if a text line is a header/footer or page number.
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
}
