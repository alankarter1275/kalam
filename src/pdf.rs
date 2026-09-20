//! PDF Engine Integration and Reflow Engine.
//!
//! Provides PDF document parsing via `lopdf`, page text extraction, Zathura-style smart cropping
//! (ink bounding box calculation to strip blank margins), and heuristic text reflow.

use anyhow::{anyhow, Result};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use lopdf::Document;
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

/// PDF Document handle.
pub struct PdfDocument {
    #[allow(dead_code)] // kept for re-open/title; not read on every render path
    pub path: PathBuf,
    doc: Document,
    page_numbers: Vec<u32>,
}

impl PdfDocument {
    /// Load a PDF document from a file path.
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path_buf = path.as_ref().to_path_buf();
        let doc = Document::load(&path_buf)
            .map_err(|e| anyhow!("Failed to load PDF document {:?}: {}", path_buf, e))?;

        let mut page_numbers: Vec<u32> = doc.get_pages().keys().cloned().collect();
        page_numbers.sort_unstable();

        if page_numbers.is_empty() {
            return Err(anyhow!("PDF document has no pages"));
        }

        Ok(Self {
            path: path_buf,
            doc,
            page_numbers,
        })
    }

    /// Return total page count.
    pub fn page_count(&self) -> usize {
        self.page_numbers.len()
    }

    /// Extract raw text layer from a PDF page (1-indexed).
    pub fn extract_raw_text(&self, page_num: usize) -> Result<String> {
        if page_num == 0 || page_num > self.page_numbers.len() {
            return Err(anyhow!("Page number {} out of range", page_num));
        }

        let pdf_page_num = self.page_numbers[page_num - 1];
        let text = self
            .doc
            .extract_text(&[pdf_page_num])
            .unwrap_or_else(|_| String::new());

        Ok(text)
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

    /// Render uncropped raw page image (1-indexed).
    fn render_page_image_uncropped(&self, page_num: usize) -> Result<DynamicImage> {
        if page_num == 0 || page_num > self.page_numbers.len() {
            return Err(anyhow!("Page number {} out of range", page_num));
        }

        let pdf_page_num = self.page_numbers[page_num - 1];

        // Try extracting raster image XObject from the PDF page resources
        if let Ok(page_id) = self.doc.get_pages().get(&pdf_page_num).copied().ok_or_else(|| anyhow!("Page ID not found")) {
            if let Ok(resources) = self.doc.get_page_resources(page_id) {
                if let Some(resources_dict) = resources.0 {
                    if let Ok(xobjects) = resources_dict.get(b"XObject").and_then(|o| o.as_dict()) {
                        for (_, obj) in xobjects.iter() {
                            let stream_res = if let Ok(ref_id) = obj.as_reference() {
                                self.doc.get_object(ref_id).and_then(|o| o.as_stream())
                            } else {
                                obj.as_stream()
                            };

                            if let Ok(stream) = stream_res {
                                if let Ok(subtype) = stream.dict.get(b"Subtype").and_then(|s| s.as_name()) {
                                    if subtype == b"Image" {
                                        // Try decompressed stream content first, then fallback to raw stream content (e.g. for JPEG DCTDecode streams)
                                        let bytes_candidates = vec![
                                            stream.decompressed_content().ok(),
                                            Some(stream.content.clone()),
                                        ];

                                        for bytes in bytes_candidates.into_iter().flatten() {
                                            if let Ok(img) = image::load_from_memory(&bytes) {
                                                return Ok(img);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Synthetic page canvas fallback: render extracted text onto a clean page canvas
        let text = self.extract_raw_text(page_num)?;
        let canvas = render_text_to_canvas(&text, 800, 1050);
        Ok(canvas)
    }

    /// Render PDF page image with optional Smart Crop applied.
    pub fn render_page_image(&self, page_num: usize, smart_crop: bool) -> Result<DynamicImage> {
        let img = self.render_page_image_uncropped(page_num)?;

        if smart_crop {
            let ink_box = Self::calculate_ink_box_for_image(&img);
            let (width, height) = img.dimensions();

            let x = (ink_box.min_x * width as f32) as u32;
            let y = (ink_box.min_y * height as f32) as u32;
            let crop_w = (ink_box.width_fraction() * width as f32) as u32;
            let crop_h = (ink_box.height_fraction() * height as f32) as u32;

            if crop_w > 0 && crop_h > 0 && (crop_w < width || crop_h < height) {
                return Ok(img.crop_imm(x, y, crop_w, crop_h));
            }
        }

        Ok(img)
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

/// Helper function to render text lines onto a synthetic GTK page canvas image (for preview / page mode).
fn render_text_to_canvas(text: &str, width: u32, height: u32) -> DynamicImage {
    let mut img = RgbaImage::from_pixel(width, height, Rgba([255, 255, 255, 255]));

    let margin = 40;
    let lines: Vec<&str> = text.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    let mut current_y = margin + 30;

    for line in lines {
        if current_y + 20 > height - margin {
            break;
        }
        let line_len = line.len().min(70);
        let line_w = ((line_len as u32) * 9).min(width - 2 * margin - 20);

        // Draw ink pixels representing the text line (no margin border line to avoid breaking smart crop)
        for dy in 0..8 {
            for dx in 0..line_w {
                let px = margin + 20 + dx;
                let py = current_y + dy;
                if px < width - margin && py < height - margin {
                    img.put_pixel(px, py, Rgba([40, 40, 40, 255]));
                }
            }
        }
        current_y += 18;
    }

    DynamicImage::ImageRgba8(img)
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
        let blank_img = DynamicImage::ImageRgba8(RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255])));
        let ink_box = PdfDocument::calculate_ink_box_for_image(&blank_img);
        assert_eq!(ink_box, InkBoundingBox::default());

        let mut content_img = RgbaImage::from_pixel(100, 100, Rgba([255, 255, 255, 255]));
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
    fn test_synthetic_canvas_smart_crop() {
        let canvas = render_text_to_canvas("Sample header line\nSecond line of content", 800, 1050);
        let ink_box = PdfDocument::calculate_ink_box_for_image(&canvas);

        // Verify smart crop calculated bounds specifically around the text block, ignoring blank margins
        assert!(ink_box.min_x >= 0.05); // margin at left
        assert!(ink_box.max_y <= 0.50); // text only covers top half of canvas
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

