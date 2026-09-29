//! Bubble-aware comic OCR (ROADMAP 2.14).
//!
//! Instead of running one OCR pass over a whole comic page and hoping, find
//! each speech balloon first, recognize the text inside that clean crop, and
//! map the lines back to page coordinates. Balloons give clean crops (no art
//! bleeding through the text), the reading order falls out of the balloon
//! geometry, and text outside balloons (sound effects, captions) is left
//! alone by choice rather than recognized by accident — the owner's decision,
//! 2026-09-29: balloons only.
//!
//! The detector is classical computer vision, no neural net and no new
//! dependencies: threshold, connected components, then four tests that a
//! balloon passes and art does not. Every constant below was tuned against
//! four fixture pages (`fixtures/ocr/comic/`) with known ground truth, where
//! the prototype found 15/16 balloons (the 16th is deliberately clipped flat
//! by the page edge — a documented limitation) with zero false positives,
//! IoU 0.89–0.98, margins: fill 0.69–0.96 (floor 0.35), ink 0.008–0.090
//! (floor 0.003), perimeter darkness 0.83–1.00 (floor 0.55).

use anyhow::Result;
use ocrs::{ImageSource, OcrEngine, TextItem};
use serde::{Deserialize, Serialize};

use crate::pdf::{PdfPageText, PdfTextLine};

// --- detector constants (validated on the fixtures; see module docs) ------
/// A pixel is "white" (balloon fill, paper) at this luminance or above.
const WHITE_T: u8 = 200;
/// A pixel is "dark" (outline, glyph) at this luminance or below.
const DARK_T: u8 = 128;
/// Smallest balloon: 0.2% of the page (above screentone white gaps).
const MIN_AREA_FRAC: f32 = 0.002;
/// Largest balloon: half the page (below panel interiors, which the
/// containment test removes anyway).
const MAX_AREA_FRAC: f32 = 0.50;
/// Component area / bounding-box area. An ellipse fills π/4 ≈ 0.785, a
/// scalloped thought cloud ~0.7, free-form white art regions fall below.
const MIN_FILL: f32 = 0.35;
/// Bounding-box aspect limits. 8.0 admits long flat caption boxes.
const MIN_ASPECT: f32 = 0.20;
const MAX_ASPECT: f32 = 8.0;
/// Fraction of the component's boundary neighbors that must be dark (a
/// balloon has an outline). Darkness is tested with a 1px tolerance, because
/// JPEG ringing paints a light halo right against a black outline — without
/// it, a quality-82 scan loses a third of its outline signal.
const MIN_PERIM_DARK: f32 = 0.55;
/// Dark pixels the component must ENCLOSE (glyphs live in holes inside the
/// balloon's white). Empty white shapes — highlights, shirts — have none.
/// 0.3% keeps a single short word in a big balloon ("Agreed.") at 0.8%.
const MIN_INK: f32 = 0.003;
const MAX_INK: f32 = 0.60;
/// Two balloons share a reading band when their vertical spans overlap by at
/// least this fraction of the shorter one; bands read top-to-bottom, within a
/// band left-to-right (or right-to-left for manga).
const BAND_OVERLAP: f32 = 0.50;
/// Padding added around a detected balloon before cropping for recognition.
const CROP_PAD: u32 = 6;
/// Pages larger than this on the long side are downscaled for detection and
/// recognition (same cap as the whole-page path in `ocr.rs`).
const WORK_LIMIT: u32 = 1800;

/// One detected speech balloon: its rect in page pixels plus the contiguous
/// range of lines (in [`ComicPageOcr::lines`]) that were recognized inside it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComicBalloon {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
    /// First line of this balloon in the flattened line array.
    pub first_line: u32,
    pub line_count: u32,
}

/// Balloon-aware OCR result for one comic page. `lines` is stored in a
/// canonical (left-to-right) order; [`ComicPageOcr::flatten_for`] rebuilds
/// the reading order for the reader's current direction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComicPageOcr {
    pub page_num: usize,
    /// Original page pixel width (what selection coordinates use).
    pub width: f32,
    pub height: f32,
    pub balloons: Vec<ComicBalloon>,
    /// All recognized lines, concatenated per balloon in canonical order.
    /// When `balloons` is empty this holds a whole-page fallback pass.
    pub lines: Vec<PdfTextLine>,
}

/// What the reader keeps per page: the direction-independent OCR result and
/// the flattened view for the current reading direction.
#[derive(Debug, Clone)]
pub struct ComicPageTexts {
    pub ocr: ComicPageOcr,
    /// Lines in reading order for the direction this view was built with.
    pub flat: PdfPageText,
    /// Balloons with line ranges remapped into `flat.lines`.
    pub flat_balloons: Vec<ComicBalloon>,
}

impl ComicPageOcr {
    /// An empty result (page skipped, or nothing recognized).
    pub fn empty(page_num: usize, width: f32, height: f32) -> Self {
        Self {
            page_num,
            width,
            height,
            balloons: Vec::new(),
            lines: Vec::new(),
        }
    }

    /// Balloon indices in banded reading order. Bands are groups whose
    /// vertical spans overlap by at least `BAND_OVERLAP` of the shorter
    /// balloon; bands read top-to-bottom, within a band left-to-right (or
    /// right-to-left when `rtl`).
    pub fn ordered_balloons(&self, rtl: bool) -> Vec<usize> {
        let mut remaining: Vec<usize> = (0..self.balloons.len()).collect();
        remaining.sort_by(|&a, &b| {
            self.balloons[a]
                .y0
                .total_cmp(&self.balloons[b].y0)
                .then_with(|| self.balloons[a].x0.total_cmp(&self.balloons[b].x0))
        });
        let mut ordered = Vec::with_capacity(self.balloons.len());
        while !remaining.is_empty() {
            // The band of the topmost remaining balloon.
            let ref_i = remaining[0];
            let ref_balloon = &self.balloons[ref_i];
            let mut band = vec![ref_i];
            let mut rest = Vec::new();
            for &i in &remaining[1..] {
                let b = &self.balloons[i];
                let overlap = ref_balloon.y1.min(b.y1) - ref_balloon.y0.max(b.y0);
                let shorter = (ref_balloon.y1 - ref_balloon.y0).min(b.y1 - b.y0);
                if shorter > 0.0 && overlap / shorter >= BAND_OVERLAP {
                    band.push(i);
                } else {
                    rest.push(i);
                }
            }
            remaining = rest;
            band.sort_by(|&a, &b| {
                let xa = self.balloons[a].x0;
                let xb = self.balloons[b].x0;
                if rtl {
                    xb.total_cmp(&xa)
                } else {
                    xa.total_cmp(&xb)
                }
            });
            ordered.extend(band);
        }
        ordered
    }

    /// Build the reader-facing view for a reading direction: lines
    /// re-concatenated in reading order, balloons remapped to match. When no
    /// balloons were detected, the whole-page fallback lines pass through
    /// unchanged (no balloon owns them).
    pub fn flatten_for(&self, rtl: bool) -> ComicPageTexts {
        if self.balloons.is_empty() {
            return ComicPageTexts {
                ocr: self.clone(),
                flat: PdfPageText {
                    page_num: self.page_num,
                    width_pts: self.width,
                    height_pts: self.height,
                    lines: self.lines.clone(),
                },
                flat_balloons: Vec::new(),
            };
        }
        let order = self.ordered_balloons(rtl);
        let mut lines: Vec<PdfTextLine> = Vec::with_capacity(self.lines.len());
        let mut flat_balloons = Vec::with_capacity(order.len());
        for &i in &order {
            let b = &self.balloons[i];
            let first = b.first_line as usize;
            let last = (first + b.line_count as usize).min(self.lines.len());
            let new_first = lines.len() as u32;
            lines.extend(self.lines[first..last].iter().cloned());
            flat_balloons.push(ComicBalloon {
                x0: b.x0,
                y0: b.y0,
                x1: b.x1,
                y1: b.y1,
                first_line: new_first,
                line_count: b.line_count,
            });
        }
        ComicPageTexts {
            ocr: self.clone(),
            flat: PdfPageText {
                page_num: self.page_num,
                width_pts: self.width,
                height_pts: self.height,
                lines,
            },
            flat_balloons,
        }
    }

}

/// Run bubble-aware OCR on one comic page's encoded bytes.
///
/// `is_color_only` keeps the existing meaning: when true, a black & white
/// page returns an empty result without paying for inference. The result is
/// direction-independent; call [`ComicPageOcr::flatten_for`] with the
/// reader's direction when it arrives.
pub fn bubble_ocr_page(
    engine: &OcrEngine,
    raw_bytes: &[u8],
    page_num: usize,
    is_color_only: bool,
) -> Result<ComicPageOcr> {
    let img = image::load_from_memory(raw_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to decode comic page for OCR: {e}"))?;
    let orig_w = img.width();
    let orig_h = img.height();

    if is_color_only && !crate::ocr::is_image_color(&img) {
        return Ok(ComicPageOcr::empty(page_num, orig_w as f32, orig_h as f32));
    }

    // Working resolution for both detection and recognition.
    let work = if orig_w > WORK_LIMIT || orig_h > WORK_LIMIT {
        img.resize(WORK_LIMIT, WORK_LIMIT, image::imageops::FilterType::Triangle)
    } else {
        img
    };
    let work_w = work.width();
    let work_h = work.height();
    let scale_x = orig_w as f32 / work_w.max(1) as f32;
    let scale_y = orig_h as f32 / work_h.max(1) as f32;

    let gray = image::imageops::grayscale(&work);
    let balloons = detect_balloons(gray.as_raw(), work_w as usize, work_h as usize);

    if balloons.is_empty() {
        // No balloons detected (full-bleed art, an unusual style, or a
        // detection miss). Fall back to the whole-page pipeline rather than
        // leaving the page with nothing: graceful degradation, not a silent
        // regression from the pre-2.14 reader.
        let pt = crate::ocr::perform_ocr_image_bytes(engine, raw_bytes, page_num, false)?;
        return Ok(ComicPageOcr {
            page_num,
            width: pt.width_pts,
            height: pt.height_pts,
            balloons: Vec::new(),
            lines: pt.lines,
        });
    }

    let work_rgb = work.to_rgb8();
    let mut all_lines: Vec<PdfTextLine> = Vec::new();
    let mut out_balloons: Vec<ComicBalloon> = Vec::with_capacity(balloons.len());

    for (bx0, by0, bx1, by1) in balloons {
        let cx0 = bx0.saturating_sub(CROP_PAD);
        let cy0 = by0.saturating_sub(CROP_PAD);
        let cx1 = (bx1 + CROP_PAD).min(work_w);
        let cy1 = (by1 + CROP_PAD).min(work_h);
        let cw = cx1.saturating_sub(cx0);
        let ch = cy1.saturating_sub(cy0);
        if cw == 0 || ch == 0 {
            continue;
        }

        // Crop to an RGB buffer for the recognizer.
        let mut rgb = Vec::with_capacity((cw * ch * 3) as usize);
        for y in cy0..cy1 {
            for x in cx0..cx1 {
                let px = work_rgb.get_pixel(x, y).0; // [u8; 3]
                rgb.push(px[0]);
                rgb.push(px[1]);
                rgb.push(px[2]);
            }
        }

        let mut lines = recognize_lines(
            engine,
            &rgb,
            cw,
            ch,
            &CropGeom {
                ox: cx0 as f32,
                oy: cy0 as f32,
                scale_x,
                scale_y,
            },
        )?;
        // Lines in crop-detection order; reading order within a balloon is
        // strictly vertical.
        lines.sort_by(|a, b| a.y0.total_cmp(&b.y0).then_with(|| a.x0.total_cmp(&b.x0)));

        let first_line = all_lines.len() as u32;
        let line_count = lines.len() as u32;
        all_lines.extend(lines);
        out_balloons.push(ComicBalloon {
            x0: bx0 as f32 * scale_x,
            y0: by0 as f32 * scale_y,
            x1: bx1 as f32 * scale_x,
            y1: by1 as f32 * scale_y,
            first_line,
            line_count,
        });
    }

    // Balloons with no recognized text are dropped (the final arbiter: an
    // outlined white region with no words is art, not a balloon).
    let kept: Vec<ComicBalloon> = out_balloons
        .into_iter()
        .filter(|b| b.line_count > 0)
        .collect();

    Ok(ComicPageOcr {
        page_num,
        width: orig_w as f32,
        height: orig_h as f32,
        balloons: kept,
        lines: all_lines,
    })
}

/// Where a crop sits on the working image and how it scales back to
/// original-page pixels.
struct CropGeom {
    ox: f32,
    oy: f32,
    scale_x: f32,
    scale_y: f32,
}

/// Recognize the text lines in one RGB region, mapping coordinates to
/// original-page pixels via the crop geometry.
fn recognize_lines(
    engine: &OcrEngine,
    rgb: &[u8],
    w: u32,
    h: u32,
    geom: &CropGeom,
) -> Result<Vec<PdfTextLine>> {
    if w == 0 || h == 0 || rgb.is_empty() {
        return Ok(Vec::new());
    }
    let src = ImageSource::from_bytes(rgb, (w, h))
        .map_err(|e| anyhow::anyhow!("Failed to create OCR image source: {e:?}"))?;
    let input = engine
        .prepare_input(src)
        .map_err(|e| anyhow::anyhow!("Failed to prepare OCR input: {e:?}"))?;
    let words = engine
        .detect_words(&input)
        .map_err(|e| anyhow::anyhow!("OCR word detection failed: {e:?}"))?;
    let line_rects = engine.find_text_lines(&input, &words);
    let recognized = engine
        .recognize_text(&input, &line_rects)
        .map_err(|e| anyhow::anyhow!("OCR text recognition failed: {e:?}"))?;

    let mut lines = Vec::new();
    for line_opt in recognized {
        let Some(line) = line_opt else { continue };
        let raw = line.chars();
        if raw.is_empty() {
            continue;
        }
        let mut chars = Vec::with_capacity(raw.len());
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for ch in raw {
            let cx0 = geom.ox + ch.rect.left() as f32;
            let cy0 = geom.oy + ch.rect.top() as f32;
            let cx1 = geom.ox + ch.rect.right() as f32;
            let cy1 = geom.oy + ch.rect.bottom() as f32;
            x0 = x0.min(cx0.min(cx1));
            y0 = y0.min(cy0.min(cy1));
            x1 = x1.max(cx0.max(cx1));
            y1 = y1.max(cy0.max(cy1));
            chars.push(crate::pdf::PdfTextChar {
                ch: ch.char,
                x0: cx0 * geom.scale_x,
                y0: cy0 * geom.scale_y,
                x1: cx1 * geom.scale_x,
                y1: cy1 * geom.scale_y,
            });
        }
        lines.push(PdfTextLine {
            text: line.to_string(),
            x0: x0 * geom.scale_x,
            y0: y0 * geom.scale_y,
            x1: x1 * geom.scale_x,
            y1: y1 * geom.scale_y,
            chars,
        });
    }
    Ok(lines)
}

/// A connected white region found during labeling.
struct Component {
    count: usize,
    /// Inclusive min / exclusive max bounds.
    x0: usize,
    y0: usize,
    x1: usize,
    y1: usize,
}

/// Detect speech balloons on a page's luminance buffer (row-major, `w * h`
/// bytes): threshold, connected components, then the four balloon tests (see
/// the module docs for the measured margins). Returns rects in working-pixel
/// coordinates, in no particular order.
pub fn detect_balloons(lum: &[u8], w: usize, h: usize) -> Vec<(u32, u32, u32, u32)> {
    if w == 0 || h == 0 || lum.len() < w * h {
        return Vec::new();
    }
    let total = w * h;
    let at = |x: usize, y: usize| lum[y * w + x];

    // Connected components over the white mask (8-connectivity).
    let mut labels: Vec<i32> = vec![-1; total];
    let mut comps: Vec<Component> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    for start in 0..total {
        if labels[start] != -1 || at(start % w, start / w) < WHITE_T {
            continue;
        }
        let cid = comps.len() as i32;
        labels[start] = cid;
        stack.clear();
        stack.push(start);
        let (mut x0, mut y0, mut x1, mut y1) = (usize::MAX, usize::MAX, 0usize, 0usize);
        let mut count = 0usize;
        while let Some(idx) = stack.pop() {
            count += 1;
            let (px, py) = (idx % w, idx / w);
            x0 = x0.min(px);
            x1 = x1.max(px + 1);
            y0 = y0.min(py);
            y1 = y1.max(py + 1);
            for dy in -1i32..=1 {
                for dx in -1i32..=1 {
                    if dx == 0 && dy == 0 {
                        continue;
                    }
                    let nx = px as i32 + dx;
                    let ny = py as i32 + dy;
                    if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
                        continue;
                    }
                    let n = ny as usize * w + nx as usize;
                    if labels[n] == -1 && at(nx as usize, ny as usize) >= WHITE_T {
                        labels[n] = cid;
                        stack.push(n);
                    }
                }
            }
        }
        comps.push(Component {
            count,
            x0,
            y0,
            x1,
            y1,
        });
    }

    // Geometry pre-filters, then the ink and outline tests, per component.
    let mut candidates: Vec<(usize, usize, usize, usize)> = Vec::new();
    for (cid, c) in comps.iter().enumerate() {
        // Page background is white and border-connected; balloons are
        // enclosed. (A balloon clipped flat by the page edge merges with the
        // background and is lost — documented v1 limitation.)
        if c.x0 == 0 || c.y0 == 0 || c.x1 == w || c.y1 == h {
            continue;
        }
        let frac = c.count as f32 / total as f32;
        if frac < MIN_AREA_FRAC || frac > MAX_AREA_FRAC {
            continue;
        }
        let bw = c.x1 - c.x0;
        let bh = c.y1 - c.y0;
        if bw == 0 || bh == 0 {
            continue;
        }
        let fill = c.count as f32 / (bw * bh) as f32;
        if fill < MIN_FILL {
            continue;
        }
        let aspect = bw as f32 / bh as f32;
        if aspect < MIN_ASPECT || aspect > MAX_ASPECT {
            continue;
        }

        // Hole flood over the bbox: non-member pixels reachable from the
        // bbox border are outside; the rest are holes (the glyphs).
        let mut reach = vec![false; bw * bh];
        let mut flood: Vec<usize> = Vec::new();
        let is_member = |lx: usize, ly: usize| labels[(c.y0 + ly) * w + c.x0 + lx] == cid as i32;
        let push = |flood: &mut Vec<usize>, reach: &mut Vec<bool>, lx: usize, ly: usize| {
            if !is_member(lx, ly) && !reach[ly * bw + lx] {
                reach[ly * bw + lx] = true;
                flood.push(ly * bw + lx);
            }
        };
        for lx in 0..bw {
            push(&mut flood, &mut reach, lx, 0);
            push(&mut flood, &mut reach, lx, bh - 1);
        }
        for ly in 0..bh {
            push(&mut flood, &mut reach, 0, ly);
            push(&mut flood, &mut reach, bw - 1, ly);
        }
        while let Some(idx) = flood.pop() {
            let (lx, ly) = (idx % bw, idx / bw);
            if lx > 0 {
                push(&mut flood, &mut reach, lx - 1, ly);
            }
            if lx + 1 < bw {
                push(&mut flood, &mut reach, lx + 1, ly);
            }
            if ly > 0 {
                push(&mut flood, &mut reach, lx, ly - 1);
            }
            if ly + 1 < bh {
                push(&mut flood, &mut reach, lx, ly + 1);
            }
        }

        let mut ink_dark = 0usize;
        let mut border_positions = 0usize;
        let mut border_dark = 0usize;
        for ly in 0..bh {
            for lx in 0..bw {
                if is_member(lx, ly) {
                    continue;
                }
                if !reach[ly * bw + lx] {
                    // A hole: dark pixels here are glyph ink.
                    if at(c.x0 + lx, c.y0 + ly) <= DARK_T {
                        ink_dark += 1;
                    }
                }
            }
        }
        for ly in 0..bh {
            for lx in 0..bw {
                if !is_member(lx, ly) {
                    continue;
                }
                // Boundary: 4-neighbors that are not part of this component.
                // Darkness is tested with a 1px ring (JPEG tolerance).
                for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
                    let gx = c.x0 as i32 + lx as i32 + dx;
                    let gy = c.y0 as i32 + ly as i32 + dy;
                    if gx < 0 || gy < 0 || gx >= w as i32 || gy >= h as i32 {
                        continue;
                    }
                    let (gx, gy) = (gx as usize, gy as usize);
                    if labels[gy * w + gx] == cid as i32 {
                        continue;
                    }
                    border_positions += 1;
                    if pixel_dark_with_ring(&at, w, h, gx, gy) {
                        border_dark += 1;
                    }
                }
            }
        }

        let ink = ink_dark as f32 / c.count as f32;
        if ink < MIN_INK || ink > MAX_INK {
            continue;
        }
        if border_positions == 0 || (border_dark as f32 / border_positions as f32) < MIN_PERIM_DARK
        {
            continue;
        }

        candidates.push((c.x0, c.y0, c.x1, c.y1));
    }

    // Containment pruning: a balloon never contains another balloon. A
    // candidate fully containing a smaller candidate (center inside too) is a
    // panel or other background region.
    let mut pruned = Vec::new();
    for (i, &big) in candidates.iter().enumerate() {
        let contains = candidates.iter().enumerate().any(|(j, &small)| {
            if i == j {
                return false;
            }
            let (bx0, by0, bx1, by1) = big;
            let (sx0, sy0, sx1, sy1) = small;
            let cx = (sx0 + sx1) as f32 / 2.0;
            let cy = (sy0 + sy1) as f32 / 2.0;
            bx0 <= sx0
                && sx1 <= bx1
                && by0 <= sy0
                && sy1 <= by1
                && cx >= bx0 as f32
                && cx <= bx1 as f32
                && cy >= by0 as f32
                && cy <= by1 as f32
        });
        if !contains {
            pruned.push(big);
        }
    }

    pruned
        .into_iter()
        .map(|(x0, y0, x1, y1)| (x0 as u32, y0 as u32, x1 as u32, y1 as u32))
        .collect()
}

/// Dark at this pixel, or dark within 1px of it (absorbs JPEG ringing).
fn pixel_dark_with_ring(
    at: &dyn Fn(usize, usize) -> u8,
    w: usize,
    h: usize,
    x: usize,
    y: usize,
) -> bool {
    if at(x, y) <= DARK_T {
        return true;
    }
    for (dx, dy) in [(-1i32, 0i32), (1, 0), (0, -1), (0, 1)] {
        let nx = x as i32 + dx;
        let ny = y as i32 + dy;
        if nx < 0 || ny < 0 || nx >= w as i32 || ny >= h as i32 {
            continue;
        }
        if at(nx as usize, ny as usize) <= DARK_T {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Ground truth fixture descriptor (fixtures/ocr/comic/comic-pages.json).
    #[derive(Debug, Deserialize)]
    struct GtPage {
        file: String,
        balloons: Vec<GtBalloon>,
    }

    #[derive(Debug, Deserialize)]
    struct GtBalloon {
        rect: [f32; 4],
        #[serde(default)]
        expect_miss: bool,
        text: Vec<String>,
    }

    fn iou(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> f32 {
        let ix = (a.2.min(b.2) - a.0.max(b.0)).max(0.0);
        let iy = (a.3.min(b.3) - a.1.max(b.1)).max(0.0);
        let inter = ix * iy;
        let union = (a.2 - a.0) * (a.3 - a.1) + (b.2 - b.0) * (b.3 - b.1) - inter;
        if union <= 0.0 {
            0.0
        } else {
            inter / union
        }
    }

    fn centers_mutually_inside(a: (f32, f32, f32, f32), b: (f32, f32, f32, f32)) -> bool {
        let acx = (a.0 + a.2) / 2.0;
        let acy = (a.1 + a.3) / 2.0;
        let bcx = (b.0 + b.2) / 2.0;
        let bcy = (b.1 + b.3) / 2.0;
        b.0 <= acx
            && acx <= b.2
            && b.1 <= acy
            && acy <= b.3
            && a.0 <= bcx
            && bcx <= a.2
            && a.1 <= bcy
            && bcy <= a.3
    }

    #[test]
    fn detector_matches_fixture_ground_truth() -> Result<()> {
        let pages: Vec<GtPage> = {
            let raw = std::fs::read_to_string("fixtures/ocr/comic/comic-pages.json")?;
            #[derive(Deserialize)]
            struct All {
                pages: Vec<GtPage>,
            }
            let all: All = serde_json::from_str(&raw)?;
            all.pages
        };
        assert!(!pages.is_empty(), "fixture ground truth is missing");

        for page in &pages {
            let img = image::open(format!("fixtures/ocr/comic/{}", page.file))?;
            let gray = image::imageops::grayscale(&img);
            let found = detect_balloons(
                gray.as_raw(),
                gray.width() as usize,
                gray.height() as usize,
            );
            let found_f: Vec<(f32, f32, f32, f32)> = found
                .iter()
                .map(|&(x0, y0, x1, y1)| (x0 as f32, y0 as f32, x1 as f32, y1 as f32))
                .collect();

            let mut matched: Vec<Option<(f32, f32, f32, f32)>> = vec![None; page.balloons.len()];
            for f in &found_f {
                for (i, gt) in page.balloons.iter().enumerate() {
                    let g = (gt.rect[0], gt.rect[1], gt.rect[2], gt.rect[3]);
                    if matched[i].is_none() && centers_mutually_inside(g, *f) {
                        matched[i] = Some(*f);
                    }
                }
            }

            for (i, gt) in page.balloons.iter().enumerate() {
                if gt.expect_miss {
                    // The clipped-by-the-edge balloon merges with the page
                    // background; v1 must not report it as a balloon.
                    assert!(
                        matched[i].is_none(),
                        "{}: edge-clipped balloon leaked into the results",
                        page.file
                    );
                    continue;
                }
                let Some(f) = matched[i] else {
                    panic!("{}: balloon {i} {:?} not detected", page.file, gt.rect);
                };
                let g = (gt.rect[0], gt.rect[1], gt.rect[2], gt.rect[3]);
                assert!(
                    iou(g, f) >= 0.5,
                    "{}: balloon {i} iou {:.2} too low: gt {g:?} vs {f:?}",
                    page.file,
                    iou(g, f)
                );
            }
            // Every detection must belong to some ground-truth balloon: the
            // fixtures contain decoys (screentone, outline-less highlights,
            // panel interiors) that must produce zero false positives.
            for f in &found_f {
                let belongs = page.balloons.iter().any(|gt| {
                    let g = (gt.rect[0], gt.rect[1], gt.rect[2], gt.rect[3]);
                    centers_mutually_inside(g, *f)
                });
                assert!(belongs, "{}: unexpected extra balloon {f:?}", page.file);
            }
        }
        Ok(())
    }

    #[test]
    fn reading_order_bands_left_to_right_and_right_to_left() {
        let page = ComicPageOcr {
            page_num: 0,
            width: 1200.0,
            height: 1800.0,
            balloons: vec![
                ComicBalloon {
                    x0: 90.0,
                    y0: 140.0,
                    x1: 540.0,
                    y1: 430.0,
                    first_line: 0,
                    line_count: 1,
                },
                ComicBalloon {
                    x0: 660.0,
                    y0: 170.0,
                    x1: 1110.0,
                    y1: 460.0,
                    first_line: 1,
                    line_count: 1,
                },
                ComicBalloon {
                    x0: 90.0,
                    y0: 620.0,
                    x1: 540.0,
                    y1: 900.0,
                    first_line: 2,
                    line_count: 1,
                },
                ComicBalloon {
                    x0: 660.0,
                    y0: 820.0,
                    x1: 1110.0,
                    y1: 1150.0,
                    first_line: 3,
                    line_count: 1,
                },
            ],
            lines: Vec::new(),
        };
        assert_eq!(page.ordered_balloons(false), vec![0, 1, 2, 3]);
        // Manga: the right balloon of the top band reads first.
        assert_eq!(page.ordered_balloons(true), vec![1, 0, 2, 3]);
    }

    #[test]
    fn flatten_reorders_lines_and_remaps_balloons() {
        let page = ComicPageOcr {
            page_num: 7,
            width: 800.0,
            height: 600.0,
            balloons: vec![
                ComicBalloon {
                    x0: 400.0,
                    y0: 10.0,
                    x1: 700.0,
                    y1: 100.0,
                    first_line: 2,
                    line_count: 1,
                },
                ComicBalloon {
                    x0: 50.0,
                    y0: 10.0,
                    x1: 350.0,
                    y1: 100.0,
                    first_line: 0,
                    line_count: 2,
                },
            ],
            lines: vec![
                line("left one", 50.0, 10.0),
                line("left two", 50.0, 40.0),
                line("right one", 400.0, 10.0),
            ],
        };
        let ltr = page.flatten_for(false);
        assert_eq!(ltr.flat.lines.len(), 3);
        assert_eq!(ltr.flat.lines[0].text, "left one");
        assert_eq!(ltr.flat.lines[2].text, "right one");
        // Balloon ranges remapped into the flattened array.
        assert_eq!(ltr.flat_balloons[0].first_line, 0);
        assert_eq!(ltr.flat_balloons[0].line_count, 2);
        assert_eq!(ltr.flat_balloons[1].first_line, 2);

        let rtl = page.flatten_for(true);
        assert_eq!(rtl.flat.lines[0].text, "right one");
        assert_eq!(rtl.flat.lines[1].text, "left one");
        assert_eq!(rtl.flat_balloons[0].first_line, 0);
        assert_eq!(rtl.flat_balloons[1].first_line, 1);
        assert_eq!(rtl.flat_balloons[1].line_count, 2);

        // Alt+click hit-testing, exactly as the reader does it: the first
        // balloon (in reading order) whose rect contains the point.
        let hit = |x: f32, y: f32| {
            ltr.flat_balloons
                .iter()
                .position(|b| x >= b.x0 && x <= b.x1 && y >= b.y0 && y <= b.y1)
        };
        assert_eq!(hit(60.0, 20.0), Some(0));
        assert_eq!(hit(500.0, 50.0), Some(1));
        assert_eq!(hit(10.0, 500.0), None);
    }

    #[test]
    fn whole_page_fallback_lines_pass_through_flatten() {
        let page = ComicPageOcr {
            page_num: 1,
            width: 100.0,
            height: 100.0,
            balloons: Vec::new(),
            lines: vec![line("sfx", 5.0, 5.0)],
        };
        let view = page.flatten_for(false);
        assert_eq!(view.flat.lines.len(), 1);
        assert_eq!(view.flat.lines[0].text, "sfx");
        assert!(view.flat_balloons.is_empty());
    }

    fn line(text: &str, x: f32, y: f32) -> PdfTextLine {
        PdfTextLine {
            text: text.to_string(),
            x0: x,
            y0: y,
            x1: x + 40.0,
            y1: y + 12.0,
            chars: Vec::new(),
        }
    }

    /// Full-pipeline probe over the comic fixtures (slow neural inference).
    ///
    /// Runs the real `bubble_ocr_page` on every fixture page and checks each
    /// balloon's expected text against the lines recognized INSIDE that
    /// balloon (not just somewhere on the page) — this is the test that
    /// proves the crop → recognize → map-back chain, and it publishes its
    /// per-balloon output in CI:
    ///   cargo test --release comic_bubble_probe -- --ignored --nocapture
    #[test]
    #[ignore = "slow neural inference; run explicitly with --ignored"]
    fn comic_bubble_probe_fixture_pages() -> Result<()> {
        let Some(engine) = crate::ocr::init_ocr_engine() else {
            panic!("OCR models unavailable; probe cannot run");
        };
        let pages: Vec<GtPage> = {
            let raw = std::fs::read_to_string("fixtures/ocr/comic/comic-pages.json")?;
            #[derive(Deserialize)]
            struct All {
                pages: Vec<GtPage>,
            }
            let all: All = serde_json::from_str(&raw)?;
            all.pages
        };

        for page in &pages {
            let bytes = std::fs::read(format!("fixtures/ocr/comic/{}", page.file))?;
            let ocr = bubble_ocr_page(&engine, &bytes, 0, false)?;
            println!(
                "=== {}: {} balloons, {} lines",
                page.file,
                ocr.balloons.len(),
                ocr.lines.len()
            );
            assert!(
                !ocr.balloons.is_empty(),
                "{}: no balloons detected (fallback path taken)",
                page.file
            );

            for (i, gt) in page.balloons.iter().enumerate() {
                if gt.expect_miss {
                    continue;
                }
                // Detection order is scan order, not ground-truth order:
                // match balloons by geometry (mutual center containment).
                let gt_rect = (gt.rect[0], gt.rect[1], gt.rect[2], gt.rect[3]);
                let Some(b) = ocr
                    .balloons
                    .iter()
                    .find(|b| centers_mutually_inside(gt_rect, (b.x0, b.y0, b.x1, b.y1)))
                else {
                    panic!("{}: balloon {i} {:?} not detected", page.file, gt.rect);
                };
                let first = b.first_line as usize;
                let last = first + b.line_count as usize;
                let balloon_text: String = ocr.lines[first..last]
                    .iter()
                    .map(|l| l.text.as_str())
                    .collect::<Vec<_>>()
                    .join(" | ");
                println!(
                    "  balloon {i} [{:.0},{:.0} - {:.0},{:.0}]: {balloon_text}",
                    b.x0,
                    b.y0,
                    b.x1,
                    b.y1
                );
                let lower = balloon_text.to_lowercase();
                for want in &gt.text {
                    let want_words: Vec<&str> = want.split_whitespace().collect();
                    if want_words.is_empty() {
                        continue;
                    }
                    // Every word of the expected line should appear in the
                    // balloon's recognized text (OCR may merge or split
                    // lines, so a contiguous substring match is too strict).
                    let missing: Vec<&str> = want_words
                        .into_iter()
                        .filter(|wd| {
                            let w = wd.trim_matches(|c: char| !c.is_alphanumeric());
                            !w.is_empty() && !lower.contains(&w.to_lowercase())
                        })
                        .collect();
                    assert!(
                        missing.is_empty(),
                        "{page} balloon {i}: words {missing:?} missing from {balloon_text:?}",
                        page = page.file
                    );
                }
            }
        }
        Ok(())
    }
}
