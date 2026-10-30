//! Draw-time comic image processing (decision 41).
//!
//! Nothing here touches the decoded bytes that a page came from. The
//! colour controls collapse into one affine transform for GSK's colour
//! matrix node, and crop is a cached rectangle in unit page
//! coordinates that the page widget can clip to while painting.

use super::types::{ComicImageSettings, CropRect};

/// One affine colour transform in row-major order. Each matrix row
/// produces one output RGBA channel from the four input channels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ColorTransform {
    pub(crate) matrix: [[f32; 4]; 4],
    pub(crate) offset: [f32; 4],
}

impl ColorTransform {
    fn identity() -> Self {
        Self {
            matrix: [
                [1.0, 0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0, 0.0],
                [0.0, 0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0, 1.0],
            ],
            offset: [0.0; 4],
        }
    }

    pub(crate) fn is_identity(&self) -> bool {
        *self == Self::identity()
    }

    pub(crate) fn row_major_floats(&self) -> [f32; 16] {
        let mut values = [0.0; 16];
        for (row, values_row) in self.matrix.iter().enumerate() {
            values[row * 4..row * 4 + 4].copy_from_slice(values_row);
        }
        values
    }

    #[cfg(test)]
    fn apply_clamped(&self, pixel: [f32; 4]) -> [f32; 4] {
        let mut out = [0.0; 4];
        for (channel, value) in out.iter_mut().enumerate() {
            let sum = self.matrix[channel]
                .iter()
                .zip(pixel.iter())
                .map(|(coeff, input)| coeff * input)
                .sum::<f32>()
                + self.offset[channel];
            *value = sum.clamp(0.0, 1.0);
        }
        out
    }
}

/// Compose the Image pane's colour controls in the order a reader
/// would expect: warmth first, contrast around middle grey with an
/// additive brightness shift, grayscale after those adjustments, and
/// inversion last. Alpha is never touched.
pub(crate) fn color_transform(settings: ComicImageSettings) -> ColorTransform {
    let settings = settings.canonical();
    let mut transform = ColorTransform::identity();

    // Warmth is a channel-balance change. A quarter-turn of red/blue
    // at the end of the slider is visible without turning the whole
    // page into a colour cast; green moves just enough to keep warm
    // pages yellow rather than magenta.
    let warmth = settings.warmth as f32 / 100.0;
    let gains = [
        1.0 + 0.22 * warmth,
        1.0 + 0.06 * warmth,
        1.0 - 0.22 * warmth,
    ];
    for (channel, gain) in gains.iter().enumerate() {
        transform.matrix[channel][channel] = *gain;
    }

    let contrast = (100.0 + settings.contrast as f32) / 100.0;
    let brightness = settings.brightness as f32 / 200.0;
    let contrast_offset = 0.5 - 0.5 * contrast + brightness;
    for channel in 0..3 {
        for coeff in &mut transform.matrix[channel] {
            *coeff *= contrast;
        }
        transform.offset[channel] = transform.offset[channel] * contrast + contrast_offset;
    }

    if settings.grayscale {
        // Rec. 709 luma: the same weights display pipelines use for
        // sRGB material, and green-heavy enough that coloured lettering
        // retains something close to its perceived weight.
        let weights = [0.2126, 0.7152, 0.0722];
        let mut grey_row = [0.0; 4];
        let mut grey_offset = 0.0;
        for (channel, weight) in weights.iter().enumerate() {
            for (input, coeff) in transform.matrix[channel].iter().enumerate() {
                grey_row[input] += weight * coeff;
            }
            grey_offset += weight * transform.offset[channel];
        }
        for channel in 0..3 {
            transform.matrix[channel] = grey_row;
            transform.offset[channel] = grey_offset;
        }
    }

    if settings.invert_colours {
        for channel in 0..3 {
            for coeff in &mut transform.matrix[channel] {
                *coeff = -*coeff;
            }
            transform.offset[channel] = 1.0 - transform.offset[channel];
        }
    }

    transform
}

fn pixel_rgba(rgba: &[u8], stride: usize, x: usize, y: usize) -> Option<[f32; 4]> {
    let start = y.checked_mul(stride)?.checked_add(x.checked_mul(4)?)?;
    let bytes = rgba.get(start..start + 4)?;
    Some([
        bytes[0] as f32,
        bytes[1] as f32,
        bytes[2] as f32,
        bytes[3] as f32,
    ])
}

fn corner_background(
    rgba: &[u8],
    width: usize,
    height: usize,
    stride: usize,
    step_x: usize,
    step_y: usize,
) -> [f32; 4] {
    let patch = ((width.min(height) / 20).max(2)).min(width.min(height));
    let starts = [
        (0, 0),
        (width.saturating_sub(patch), 0),
        (0, height.saturating_sub(patch)),
        (
            width.saturating_sub(patch),
            height.saturating_sub(patch),
        ),
    ];
    let mut patches = Vec::with_capacity(4);

    for (start_x, start_y) in starts {
        let mut sum = [0.0f32; 4];
        let mut count = 0.0f32;
        for y in (start_y..(start_y + patch).min(height)).step_by(step_y) {
            for x in (start_x..(start_x + patch).min(width)).step_by(step_x) {
                if let Some(pixel) = pixel_rgba(rgba, stride, x, y) {
                    for channel in 0..4 {
                        sum[channel] += pixel[channel];
                    }
                    count += 1.0;
                }
            }
        }
        if count > 0.0 {
            patches.push([
                sum[0] / count,
                sum[1] / count,
                sum[2] / count,
                sum[3] / count,
            ]);
        }
    }

    if patches.is_empty() {
        return [255.0, 255.0, 255.0, 255.0];
    }

    // The middle of the four corner patches, channel by channel. One
    // corner covered by art cannot drag the background estimate with
    // it; a page whose art truly reaches every corner simply produces
    // content on every edge and therefore keeps the full crop.
    let mut background = [0.0; 4];
    for channel in 0..4 {
        let mut values: Vec<f32> = patches.iter().map(|patch| patch[channel]).collect();
        values.sort_by(f32::total_cmp);
        background[channel] = if values.len() == 4 {
            (values[1] + values[2]) / 2.0
        } else {
            values[values.len() / 2]
        };
    }
    background
}

fn is_content(pixel: [f32; 4], background: [f32; 4]) -> bool {
    if pixel[3] <= 16.0 {
        return false;
    }
    if background[3] <= 16.0 {
        return true;
    }

    let mut distance = 0.0f32;
    for channel in 0..3 {
        let delta = (pixel[channel] - background[channel]) / 255.0;
        distance += delta * delta;
    }
    let alpha_delta = ((pixel[3] - background[3]) / 255.0).abs();
    distance > 0.010 || alpha_delta > 0.25
}

/// Find the non-blank border box of one RGBA page. The scan is
/// subsampled and a sampled row/column needs more than an isolated
/// speck before it counts as content, so scanner dust and JPEG noise
/// in an otherwise blank margin do not preserve the whole margin.
/// When the page is blank, full-bleed, or too ambiguous to trust, the
/// result is the full page.
pub(crate) fn analyse_blank_border_crop(
    rgba: &[u8],
    width: u32,
    height: u32,
    stride: usize,
) -> CropRect {
    if width < 8 || height < 8 || stride < width as usize * 4 {
        return CropRect::FULL;
    }
    let width = width as usize;
    let height = height as usize;
    let Some(min_bytes) = stride
        .checked_mul(height.saturating_sub(1))
        .and_then(|rows| rows.checked_add(width * 4))
    else {
        return CropRect::FULL;
    };
    if rgba.len() < min_bytes {
        return CropRect::FULL;
    }

    // About 600 samples on the long side is plenty to locate panel
    // borders and keeps a multi-megapixel scan to a fraction of its
    // full pixel count on the decode thread.
    let step_x = (width.div_ceil(600)).max(1);
    let step_y = (height.div_ceil(600)).max(1);
    let xs: Vec<usize> = (0..width).step_by(step_x).collect();
    let ys: Vec<usize> = (0..height).step_by(step_y).collect();
    if xs.is_empty() || ys.is_empty() {
        return CropRect::FULL;
    }

    let background = corner_background(rgba, width, height, stride, step_x, step_y);
    let mut row_counts = vec![0usize; ys.len()];
    let mut col_counts = vec![0usize; xs.len()];

    for (row_idx, &y) in ys.iter().enumerate() {
        for (col_idx, &x) in xs.iter().enumerate() {
            let Some(pixel) = pixel_rgba(rgba, stride, x, y) else {
                continue;
            };
            if is_content(pixel, background) {
                row_counts[row_idx] += 1;
                col_counts[col_idx] += 1;
            }
        }
    }

    let row_threshold = (xs.len() / 500).max(2);
    let col_threshold = (ys.len() / 500).max(2);
    let Some(first_row) = row_counts.iter().position(|&n| n >= row_threshold) else {
        return CropRect::FULL;
    };
    let Some(last_row) = row_counts.iter().rposition(|&n| n >= row_threshold) else {
        return CropRect::FULL;
    };
    let Some(first_col) = col_counts.iter().position(|&n| n >= col_threshold) else {
        return CropRect::FULL;
    };
    let Some(last_col) = col_counts.iter().rposition(|&n| n >= col_threshold) else {
        return CropRect::FULL;
    };

    let mut min_x = xs[first_col];
    let mut min_y = ys[first_row];
    let mut max_x = (xs[last_col] + step_x - 1).min(width - 1);
    let mut max_y = (ys[last_row] + step_y - 1).min(height - 1);

    // Keep a breath of paper around the detected art. Anti-aliased
    // panel rules should never be shaved by the analysis grid.
    let pad_x = ((width as f32 * 0.006).round() as usize).clamp(2, 24);
    let pad_y = ((height as f32 * 0.006).round() as usize).clamp(2, 24);
    min_x = min_x.saturating_sub(pad_x);
    min_y = min_y.saturating_sub(pad_y);
    max_x = (max_x + pad_x).min(width - 1);
    max_y = (max_y + pad_y).min(height - 1);

    let crop_width = max_x + 1 - min_x;
    let crop_height = max_y + 1 - min_y;
    if crop_width < width / 10 || crop_height < height / 10 {
        return CropRect::FULL;
    }

    let crop = CropRect::new(
        min_x as f32 / width as f32,
        min_y as f32 / height as f32,
        crop_width as f32 / width as f32,
        crop_height as f32 / height as f32,
    );
    if crop.is_full() {
        CropRect::FULL
    } else {
        crop
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_pixel(actual: [f32; 4], expected: [f32; 4]) {
        for channel in 0..4 {
            assert!(
                (actual[channel] - expected[channel]).abs() < 0.0001,
                "channel {channel}: expected {}, got {}",
                expected[channel],
                actual[channel]
            );
        }
    }

    #[test]
    fn neutral_image_settings_are_the_identity() {
        let transform = color_transform(ComicImageSettings::default());
        assert!(transform.is_identity());
        assert_pixel(
            transform.apply_clamped([0.2, 0.4, 0.6, 0.8]),
            [0.2, 0.4, 0.6, 0.8],
        );
    }

    #[test]
    fn grayscale_uses_rec709_luma_and_leaves_alpha_alone() {
        let transform = color_transform(ComicImageSettings {
            grayscale: true,
            ..ComicImageSettings::default()
        });
        assert_pixel(
            transform.apply_clamped([1.0, 0.5, 0.0, 0.25]),
            [0.5702, 0.5702, 0.5702, 0.25],
        );
    }

    #[test]
    fn brightness_contrast_and_invert_compose_predictably() {
        let brightened = color_transform(ComicImageSettings {
            brightness: 20,
            contrast: 100,
            ..ComicImageSettings::default()
        });
        // Middle grey is contrast's pivot, then gains 0.1 brightness.
        assert_pixel(
            brightened.apply_clamped([0.5, 0.5, 0.5, 1.0]),
            [0.6, 0.6, 0.6, 1.0],
        );

        let inverted = color_transform(ComicImageSettings {
            invert_colours: true,
            ..ComicImageSettings::default()
        });
        assert_pixel(
            inverted.apply_clamped([0.0, 0.25, 1.0, 0.5]),
            [1.0, 0.75, 0.0, 0.5],
        );
    }

    #[test]
    fn warmth_moves_red_and_blue_in_opposite_directions() {
        let warm = color_transform(ComicImageSettings {
            warmth: 100,
            ..ComicImageSettings::default()
        })
        .apply_clamped([0.5, 0.5, 0.5, 1.0]);
        assert_pixel(warm, [0.61, 0.53, 0.39, 1.0]);
    }

    fn page(
        width: usize,
        height: usize,
        background: [u8; 4],
        rect: Option<(usize, usize, usize, usize, [u8; 4])>,
    ) -> Vec<u8> {
        let mut bytes = vec![0; width * height * 4];
        for y in 0..height {
            for x in 0..width {
                let mut pixel = background;
                if let Some((x0, y0, x1, y1, colour)) = rect {
                    if (x0..=x1).contains(&x) && (y0..=y1).contains(&y) {
                        pixel = colour;
                    }
                }
                let start = (y * width + x) * 4;
                bytes[start..start + 4].copy_from_slice(&pixel);
            }
        }
        bytes
    }

    #[test]
    fn crop_finds_art_inside_white_or_black_margins() {
        for (background, art) in [
            ([255, 255, 255, 255], [0, 0, 0, 255]),
            ([0, 0, 0, 255], [240, 240, 240, 255]),
        ] {
            let bytes = page(100, 80, background, Some((20, 15, 79, 64, art)));
            let crop = analyse_blank_border_crop(&bytes, 100, 80, 400);
            assert!(!crop.is_full(), "background {background:?} was not cropped");
            assert!(crop.x > 0.15 && crop.x < 0.20, "x was {}", crop.x);
            assert!(crop.y > 0.13 && crop.y < 0.19, "y was {}", crop.y);
            assert!(crop.width > 0.58 && crop.width < 0.66);
            assert!(crop.height > 0.58 && crop.height < 0.68);
        }
    }

    #[test]
    fn crop_treats_transparency_as_blank_and_ignores_a_speck() {
        let mut bytes = page(
            100,
            80,
            [0, 0, 0, 0],
            Some((30, 20, 69, 59, [200, 30, 20, 255])),
        );
        bytes[(2 * 100 + 2) * 4..(2 * 100 + 2) * 4 + 4]
            .copy_from_slice(&[0, 0, 0, 255]);
        let crop = analyse_blank_border_crop(&bytes, 100, 80, 400);
        assert!(crop.x > 0.25, "the speck preserved the margin: {crop:?}");
        assert!(crop.y > 0.18, "the speck preserved the margin: {crop:?}");
    }

    #[test]
    fn blank_and_full_bleed_pages_keep_the_full_frame() {
        let blank = page(100, 80, [250, 250, 250, 255], None);
        assert_eq!(
            analyse_blank_border_crop(&blank, 100, 80, 400),
            CropRect::FULL
        );

        let mut full = vec![0; 100 * 80 * 4];
        for y in 0..80 {
            for x in 0..100 {
                let start = (y * 100 + x) * 4;
                let colour = if (x + y) % 2 == 0 {
                    [20, 40, 200, 255]
                } else {
                    [220, 180, 30, 255]
                };
                full[start..start + 4].copy_from_slice(&colour);
            }
        }
        assert_eq!(
            analyse_blank_border_crop(&full, 100, 80, 400),
            CropRect::FULL
        );
    }
}
