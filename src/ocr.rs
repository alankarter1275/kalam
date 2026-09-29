//! OCR (Optical Character Recognition) engine module.
//!
//! Provides a self-contained, pure-Rust OCR pipeline powered by `ocrs` and `rten`.
//! Supports extracting text and character-level bounding quads from rasterized images
//! and scanned PDF pages without requiring external system dependencies or network connectivity.

use anyhow::{anyhow, Result};
use ocrs::{ImageSource, OcrEngine, OcrEngineParams, TextItem};
use rten::Model;
use std::path::PathBuf;

use crate::pdf::{PdfPageText, PdfTextChar, PdfTextLine, RenderedPage};

/// Embedded Latin detection model bytes (DBNet architecture via rten).
pub const DETECTION_MODEL_BYTES: &[u8] = include_bytes!("../assets/models/ocr/text-detection.rten");

/// Embedded Latin recognition model bytes (CRNN architecture via rten).
pub const RECOGNITION_MODEL_BYTES: &[u8] = include_bytes!("../assets/models/ocr/text-recognition.rten");

/// Represents a single OCR recognition request for a PDF page.
#[derive(Debug, Clone)]
pub struct PdfOcrRequest {
    pub generation: u64,
    pub page: usize,
    pub path: PathBuf,
}

/// Represents a single OCR recognition request for a comic page.
#[derive(Debug, Clone)]
pub struct ComicOcrRequest {
    pub generation: u64,
    pub page: usize,
    pub raw_bytes: Vec<u8>,
    pub is_color_only: bool,
}

/// Check if an image contains color (as opposed to being grayscale / monochrome black & white).
/// Samples a grid of pixels across the image and checks color saturation (chroma).
pub fn is_image_color(img: &image::DynamicImage) -> bool {
    use image::GenericImageView;
    let (w, h) = img.dimensions();
    if w == 0 || h == 0 {
        return false;
    }

    match img.color() {
        image::ColorType::L8 | image::ColorType::La8 | image::ColorType::L16 | image::ColorType::La16 => {
            return false;
        }
        _ => {}
    }

    let step_x = (w / 40).max(1);
    let step_y = (h / 40).max(1);

    let mut total_samples = 0;
    let mut color_samples = 0;

    let mut y = step_y / 2;
    while y < h {
        let mut x = step_x / 2;
        while x < w {
            let pixel = img.get_pixel(x, y);
            let r = pixel[0] as i32;
            let g = pixel[1] as i32;
            let b = pixel[2] as i32;

            let max_c = r.max(g).max(b);
            let min_c = r.min(g).min(b);
            let chroma = max_c - min_c;

            // In monochrome / grayscale images (even with scanner tone or slight paper tint),
            // chroma is typically <= 15.
            // Vibrant colors in comics/manhwa/colored manga have chroma > 22.
            if chroma > 22 {
                color_samples += 1;
            }
            total_samples += 1;

            x += step_x;
        }
        y += step_y;
    }

    if total_samples == 0 {
        return false;
    }

    (color_samples as f32 / total_samples as f32) >= 0.015
}

/// Initialize the OCR engine.
///
/// Tries embedded static slices first (compiled directly into the binary).
/// If embedded slices are stubs (e.g. < 1000 bytes during bootstrap or unit testing),
/// falls back to looking for model files on disk (`assets/models/ocr/`, app data dir, `/usr/share/kalam/models/ocr/`).
/// Returns `None` gracefully if models cannot be located or loaded, allowing the rest of Kalam to run normally.
pub fn init_ocr_engine() -> Option<OcrEngine> {
    // Check if embedded model bytes are present (real models are > 1 MB)
    let detection_model = if DETECTION_MODEL_BYTES.len() > 1000 {
        match Model::load_static_slice(DETECTION_MODEL_BYTES) {
            Ok(m) => Some(m),
            Err(e) => {
                log::warn!("Embedded OCR text-detection model failed to load: {e}");
                None
            }
        }
    } else {
        None
    };

    let recognition_model = if RECOGNITION_MODEL_BYTES.len() > 1000 {
        match Model::load_static_slice(RECOGNITION_MODEL_BYTES) {
            Ok(m) => Some(m),
            Err(e) => {
                log::warn!("Embedded OCR text-recognition model failed to load: {e}");
                None
            }
        }
    } else {
        None
    };

    let (det, rec) = match (detection_model, recognition_model) {
        (Some(d), Some(r)) => (d, r),
        _ => {
            let possible_dirs = [
                PathBuf::from("assets/models/ocr"),
                crate::paths::data_dir().join("models/ocr"),
                PathBuf::from("/usr/share/kalam/models/ocr"),
            ];
            let mut found = None;
            for dir in &possible_dirs {
                let det_path = dir.join("text-detection.rten");
                let rec_path = dir.join("text-recognition.rten");
                if det_path.exists() && rec_path.exists() {
                    if let (Ok(d), Ok(r)) = (Model::load_file(&det_path), Model::load_file(&rec_path)) {
                        found = Some((d, r));
                        break;
                    }
                }
            }
            let Some((d, r)) = found else {
                log::info!("OCR models are not yet available; OCR on scanned pages will be disabled");
                return None;
            };
            (d, r)
        }
    };

    match OcrEngine::new(OcrEngineParams {
        detection_model: Some(det),
        recognition_model: Some(rec),
        ..Default::default()
    }) {
        Ok(engine) => {
            log::info!("OCR engine successfully initialized");
            Some(engine)
        }
        Err(e) => {
            log::warn!("Failed to initialize OcrEngine: {e}");
            None
        }
    }
}

/// Run OCR on a raw RGBA buffer and produce a `PdfPageText` struct
/// with character and line coordinates mapped to `dest_width` and `dest_height`.
pub fn perform_ocr_rgba(
    engine: &OcrEngine,
    samples: &[u8],
    w: u32,
    h: u32,
    page_num: usize,
    dest_width: f32,
    dest_height: f32,
) -> Result<PdfPageText> {
    if w == 0 || h == 0 || samples.is_empty() {
        return Ok(PdfPageText {
            page_num,
            width_pts: dest_width,
            height_pts: dest_height,
            lines: Vec::new(),
        });
    }

    // Convert RGBA samples to RGB buffer
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    let (chunks, _) = samples.as_chunks::<4>();
    for chunk in chunks {
        rgb.push(chunk[0]);
        rgb.push(chunk[1]);
        rgb.push(chunk[2]);
    }

    let img_source = ImageSource::from_bytes(&rgb, (w, h))
        .map_err(|e| anyhow!("Failed to create OCR image source: {:?}", e))?;
    let ocr_input = engine
        .prepare_input(img_source)
        .map_err(|e| anyhow!("Failed to prepare OCR input: {:?}", e))?;

    let word_rects = engine
        .detect_words(&ocr_input)
        .map_err(|e| anyhow!("OCR word detection failed: {:?}", e))?;

    let line_rects = engine.find_text_lines(&ocr_input, &word_rects);
    let recognized_lines = engine
        .recognize_text(&ocr_input, &line_rects)
        .map_err(|e| anyhow!("OCR text recognition failed: {:?}", e))?;

    let scale_x = w as f32 / dest_width.max(1.0);
    let scale_y = h as f32 / dest_height.max(1.0);

    let mut lines = Vec::new();

    for line_opt in recognized_lines {
        let Some(line) = line_opt else { continue };
        let chars_raw = line.chars();
        if chars_raw.is_empty() {
            continue;
        }

        let mut line_chars = Vec::with_capacity(chars_raw.len());
        let mut min_x = f32::MAX;
        let mut min_y = f32::MAX;
        let mut max_x = f32::MIN;
        let mut max_y = f32::MIN;

        for ch in chars_raw {
            let cx0 = ch.rect.left() as f32 / scale_x;
            let cy0 = ch.rect.top() as f32 / scale_y;
            let cx1 = ch.rect.right() as f32 / scale_x;
            let cy1 = ch.rect.bottom() as f32 / scale_y;

            min_x = min_x.min(cx0.min(cx1));
            min_y = min_y.min(cy0.min(cy1));
            max_x = max_x.max(cx0.max(cx1));
            max_y = max_y.max(cy0.max(cy1));

            line_chars.push(PdfTextChar {
                ch: ch.char,
                x0: cx0,
                y0: cy0,
                x1: cx1,
                y1: cy1,
            });
        }

        let line_text: String = line.to_string();

        lines.push(PdfTextLine {
            text: line_text,
            x0: min_x,
            y0: min_y,
            x1: max_x,
            y1: max_y,
            chars: line_chars,
        });
    }

    Ok(PdfPageText {
        page_num,
        width_pts: dest_width,
        height_pts: dest_height,
        lines,
    })
}

/// Run OCR on a rendered RGBA page and produce a `PdfPageText` struct
/// with character and line coordinates mapped back to PDF point coordinates.
pub fn perform_ocr(
    engine: &OcrEngine,
    rendered: &RenderedPage,
    page_num: usize,
    width_pts: f32,
    height_pts: f32,
) -> Result<PdfPageText> {
    perform_ocr_rgba(
        engine,
        &rendered.samples,
        rendered.width as u32,
        rendered.height as u32,
        page_num,
        width_pts,
        height_pts,
    )
}

/// Run OCR on encoded image bytes (JPEG, PNG, WebP, etc.).
pub fn perform_ocr_image_bytes(
    engine: &OcrEngine,
    raw_bytes: &[u8],
    page_num: usize,
    is_color_only: bool,
) -> Result<PdfPageText> {
    let img = image::load_from_memory(raw_bytes)
        .map_err(|e| anyhow!("Failed to decode image bytes for OCR: {e}"))?;
    let orig_w = img.width();
    let orig_h = img.height();

    // If auto-detection is enabled and page is black & white, skip OCR
    if is_color_only && !is_image_color(&img) {
        log::debug!("Skipping OCR on black & white comic page {}", page_num);
        return Ok(PdfPageText {
            page_num,
            width_pts: orig_w as f32,
            height_pts: orig_h as f32,
            lines: Vec::new(),
        });
    }

    // If page is very high resolution, resize to max 1800px on longest side for fast neural inference
    let (rgba, w, h) = if orig_w > 1800 || orig_h > 1800 {
        let resized = img.resize(1800, 1800, image::imageops::FilterType::Triangle);
        let rgba = resized.to_rgba8();
        let w = rgba.width();
        let h = rgba.height();
        (rgba, w, h)
    } else {
        let rgba = img.to_rgba8();
        (rgba, orig_w, orig_h)
    };

    perform_ocr_rgba(engine, &rgba, w, h, page_num, orig_w as f32, orig_h as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_image_color_detection() {
        use image::{Rgba, RgbaImage};

        // 1. Grayscale black & white image (e.g. standard manga page)
        let mut bw_img = RgbaImage::new(100, 100);
        for pixel in bw_img.pixels_mut() {
            *pixel = Rgba([245, 243, 240, 255]); // Slightly aged/scanned off-white paper
        }
        let dynamic_bw = image::DynamicImage::ImageRgba8(bw_img);
        assert!(!is_image_color(&dynamic_bw), "B&W manga page should not be detected as color");

        // 2. Colored image (e.g. colored Naruto / manhwa / comic)
        let mut color_img = RgbaImage::new(100, 100);
        for (x, _y, pixel) in color_img.enumerate_pixels_mut() {
            if x < 40 {
                *pixel = Rgba([240, 120, 30, 255]); // Orange clothes / chakra
            } else if x < 70 {
                *pixel = Rgba([100, 180, 240, 255]); // Blue sky
            } else {
                *pixel = Rgba([255, 255, 255, 255]); // Background
            }
        }
        let dynamic_color = image::DynamicImage::ImageRgba8(color_img);
        assert!(is_image_color(&dynamic_color), "Colored comic page should be detected as color");
    }

    #[test]
    fn test_ocr_engine_graceful_missing() {
        // If models are empty stubs, init_ocr_engine should return None gracefully without panicking
        let _engine = init_ocr_engine();
    }

    #[test]
    fn test_pdf_ocr_coordinate_mapping() {
        let dummy_chars = vec![
            PdfTextChar {
                ch: 'H',
                x0: 10.0,
                y0: 10.0,
                x1: 20.0,
                y1: 25.0,
            },
            PdfTextChar {
                ch: 'i',
                x0: 22.0,
                y0: 10.0,
                x1: 28.0,
                y1: 25.0,
            },
        ];
        let dummy_line = PdfTextLine {
            text: "Hi".to_string(),
            x0: 10.0,
            y0: 10.0,
            x1: 28.0,
            y1: 25.0,
            chars: dummy_chars,
        };
        let page_text = PdfPageText {
            page_num: 1,
            width_pts: 200.0,
            height_pts: 300.0,
            lines: vec![dummy_line],
        };
        assert!(!page_text.is_empty());
        assert_eq!(page_text.total_chars(), 2);
        let (sel, rects) = page_text.select_between((5.0, 5.0), (30.0, 30.0));
        assert_eq!(sel, "Hi");
        assert_eq!(rects.len(), 1);
    }

    #[test]
    fn test_perform_ocr_rgba_empty() {
        if let Some(engine) = init_ocr_engine() {
            let res = perform_ocr_rgba(&engine, &[], 0, 0, 1, 100.0, 100.0).unwrap();
            assert!(res.is_empty());
        }
    }

    #[test]
    fn test_perform_ocr_image_bytes_invalid() {
        if let Some(engine) = init_ocr_engine() {
            let res = perform_ocr_image_bytes(&engine, b"not an image", 0, false);
            assert!(res.is_err());
        }
    }

    /// Diagnostic probe for the scanned-PDF OCR selection path.
    ///
    /// A reader-reported issue: on the scanned-OCR test PDF, copy/selection
    /// on pages 2-4 skipped whole lines (page 1 was nearly perfect). This
    /// probe replays the reader's exact pipeline on the five fixture pages
    /// at the same resolution MuPDF's 2x page render produces, prints every
    /// recognized line with its geometry, then runs the reader's REAL
    /// `perform_ocr_rgba` + `select_between` on a drag across every single
    /// line - so the published report shows both whether the OCR data is
    /// complete and whether the selection logic can reach each line.
    ///
    /// Run explicitly (slow neural inference):
    ///   cargo test --release ocr_scan_probe -- --ignored --nocapture
    #[test]
    #[ignore = "slow neural inference; run explicitly with --ignored"]
    fn ocr_scan_probe_fixture_pages() {
        let Some(engine) = init_ocr_engine() else {
            panic!("OCR models unavailable; probe cannot run");
        };

        // The fixture pages are 1240x1754 px placed at 150 DPI, so the PDF
        // page is ~595.2 x 841.9 pt, matching MuPDF's page_dimensions.
        const W_PT: f32 = 595.2;
        const H_PT: f32 = 841.9;

        // (page, snippets that should appear in that page's recognized text)
        let expectations: &[(usize, &[&str])] = &[
            (
                1,
                &[
                    "LIGHTHOUSE KEEPER",
                    "Point Auburn",
                    "4,015",
                    "canvas bag",
                    "only the one book",
                ],
            ),
            (
                2,
                &[
                    "chart table",
                    "forty-one",
                    "Marianne",
                    "kayak",
                    "3 a.m. watch",
                    "lighthouse belongs to nobody",
                ],
            ),
            (
                3,
                &[
                    "Grandmother",
                    "night shift",
                    "lamplight",
                    "seen weather",
                    "passes here first",
                ],
            ),
            (
                4,
                &[
                    "pencil and one blank page",
                    "Why copy them",
                    "lighthouse of its own",
                    "considered this",
                    "empty line beneath",
                    "Arun Vaidya",
                    "Kestrel",
                    "To be continued",
                ],
            ),
            (
                5,
                &[
                    "ENGLISH FIRST PERIOD",
                    "PLEEEASE",
                    "HANDWRITING",
                    "DING-DONG",
                ],
            ),
        ];

        for (page, required) in expectations {
            let jpg = std::fs::read(format!("fixtures/ocr/scan-page{page}.jpg"))
                .expect("fixture image");
            let img = image::load_from_memory(&jpg).expect("decode fixture");
            // MuPDF renders the 595.2pt x 841.9pt page at 2x scale: ~1190x1685.
            let scaled = img.resize_exact(1190, 1685, image::imageops::FilterType::Lanczos3);
            let rgba = scaled.to_rgba8();
            let (w, h) = (rgba.width(), rgba.height());

            // --- 1. Raw pipeline diagnostics (mirrors perform_ocr_rgba) ---
            let mut rgb = Vec::with_capacity((w * h * 3) as usize);
            for px in rgba.pixels() {
                rgb.extend_from_slice(&[px[0], px[1], px[2]]);
            }
            let src = ImageSource::from_bytes(&rgb, (w, h)).expect("image source");
            let input = engine.prepare_input(src).expect("prepare input");
            let words = engine.detect_words(&input).expect("detect words");
            let line_rects = engine.find_text_lines(&input, &words);
            let recognized = engine.recognize_text(&input, &line_rects).expect("recognize");

            let sx = w as f32 / W_PT;
            let sy = h as f32 / H_PT;

            println!(
                "=== page {page}: {} word boxes, {} line groups, {} recognition results",
                words.len(),
                line_rects.len(),
                recognized.len()
            );

            let mut page_text_raw = String::new();
            let mut none_count = 0usize;
            for (i, line_opt) in recognized.iter().enumerate() {
                match line_opt {
                    Some(line) => {
                        let text = line.to_string();
                        let (mut x0, mut y0, mut x1, mut y1) =
                            (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
                        for ch in line.chars() {
                            let r = ch.rect;
                            x0 = x0.min(r.left() as f32);
                            x1 = x1.max(r.right() as f32);
                            y0 = y0.min(r.top() as f32);
                            y1 = y1.max(r.bottom() as f32);
                        }
                        println!(
                            "  ocr line {i:2} [x {:6.1}..{:6.1}  y {:6.1}..{:6.1} pt] {text}",
                            x0 / sx,
                            x1 / sx,
                            y0 / sy,
                            y1 / sy
                        );
                        page_text_raw.push_str(&text);
                        page_text_raw.push('\n');
                    }
                    None => {
                        none_count += 1;
                        println!("  ocr line {i:2}: <recognition returned NONE - dropped>");
                    }
                }
            }
            println!(
                "page {page}: {} lines recognized, {none_count} dropped as None",
                recognized.len() - none_count
            );

            let missing: Vec<&str> = required
                .iter()
                .copied()
                .filter(|s| !page_text_raw.contains(s))
                .collect();
            if missing.is_empty() {
                println!("page {page}: all expected snippets present in OCR output");
            } else {
                println!("page {page}: MISSING FROM OCR OUTPUT: {missing:?}");
            }

            // --- 2. The reader's real PdfPageText, via perform_ocr_rgba ---
            let page_text = perform_ocr_rgba(&engine, rgba.as_raw(), w, h, *page, W_PT, H_PT)
                .expect("perform_ocr_rgba");
            println!(
                "--- perform_ocr_rgba produced {} lines for page {page}",
                page_text.lines.len()
            );
            for (i, l) in page_text.lines.iter().enumerate() {
                let preview: String = l.text.chars().take(70).collect();
                println!(
                    "  pt  line {i:2} [x {:6.1}..{:6.1}  y {:6.1}..{:6.1} pt] {preview}",
                    l.x0, l.x1, l.y0, l.y1
                );
            }

            // --- 3. Selection simulation with the REAL select_between ---
            let (full_sel, full_rects) = page_text.select_between((30.0, 50.0), (560.0, 800.0));
            println!(
                "--- full-page drag: selected {} chars, {} highlight rects",
                full_sel.chars().count(),
                full_rects.len()
            );
            for (i, l) in page_text.lines.iter().enumerate() {
                let mid_y = ((l.y0 + l.y1) * 0.5).clamp(0.0, H_PT);
                let x_start = l.x0 + 2.0;
                let x_end = (l.x1 - 2.0).max(x_start + 1.0);
                let (sel, rects) = page_text.select_between((x_start, mid_y), (x_end, mid_y));
                let status = if rects.is_empty() || sel.is_empty() {
                    "UNSELECTABLE"
                } else {
                    "ok"
                };
                let preview: String = sel.chars().take(60).collect();
                println!("  drag line {i:2}: {status:12} -> {preview}");
            }
            println!("=== end page {page} ===");
        }
    }
}
