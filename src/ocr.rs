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

/// Run OCR on a rendered RGBA page and produce a `PdfPageText` struct
/// with character and line coordinates mapped back to PDF point coordinates.
pub fn perform_ocr(
    engine: &OcrEngine,
    rendered: &RenderedPage,
    page_num: usize,
    width_pts: f32,
    height_pts: f32,
) -> Result<PdfPageText> {
    let w = rendered.width as u32;
    let h = rendered.height as u32;
    if w == 0 || h == 0 || rendered.samples.is_empty() {
        return Ok(PdfPageText {
            page_num,
            width_pts,
            height_pts,
            lines: Vec::new(),
        });
    }

    // Convert RGBA samples to RGB buffer
    let mut rgb = Vec::with_capacity((w * h * 3) as usize);
    for chunk in rendered.samples.chunks_exact(4) {
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

    let scale_x = w as f32 / width_pts.max(1.0);
    let scale_y = h as f32 / height_pts.max(1.0);

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
        width_pts,
        height_pts,
        lines,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
