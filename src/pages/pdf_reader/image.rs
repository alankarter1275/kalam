//! Draw-time PDF page colour (decisions 54–55).
//!
//! MuPDF still rasterises the page exactly as it is in the file. The
//! Image pane's controls collapse into one affine transform that the
//! page widget applies while painting that texture, so dragging a
//! slider is only a repaint: no raster is regenerated, no texture is
//! rewritten, and the PDF itself is never touched. Trim margins is
//! the one control whose work happens earlier — it keeps using the
//! renderer's existing smart-crop analysis — so it lives in the same
//! settings value but is ignored by the colour transform.

use std::cell::{Cell, RefCell};

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

/// What the Image pane does to the page's own paper colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfPageColour {
    /// The page as the file drew it.
    #[default]
    Off,
    /// White paper dimmed toward the palette's paper token.
    Tint,
    /// A photographic negative of the page.
    Invert,
}

impl PdfPageColour {
    pub fn pref_value(self) -> &'static str {
        match self {
            Self::Off => "off",
            Self::Tint => "tint",
            Self::Invert => "invert",
        }
    }

    pub fn from_pref(value: Option<&str>) -> Option<Self> {
        match value {
            Some("off") => Some(Self::Off),
            Some("tint") => Some(Self::Tint),
            Some("invert") => Some(Self::Invert),
            _ => None,
        }
    }

    pub fn segment_index(self) -> usize {
        match self {
            Self::Off => 0,
            Self::Tint => 1,
            Self::Invert => 2,
        }
    }

    pub fn from_segment_index(index: usize) -> Self {
        match index {
            1 => Self::Tint,
            2 => Self::Invert,
            _ => Self::Off,
        }
    }
}

/// Everything the Image pane controls. Adjustments are centred on
/// zero, as in the comic reader's pane: 0 leaves that part of the
/// page alone and the ends of the range are deliberately strong.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfImageSettings {
    pub page_colour: PdfPageColour,
    pub tint_strength: i32,
    pub brightness: i32,
    pub contrast: i32,
    pub warmth: i32,
    pub grayscale: bool,
    pub trim_margins: bool,
}

impl Default for PdfImageSettings {
    fn default() -> Self {
        Self {
            page_colour: PdfPageColour::Off,
            tint_strength: Self::DEFAULT_TINT_STRENGTH,
            brightness: 0,
            contrast: 0,
            warmth: 0,
            grayscale: false,
            trim_margins: false,
        }
    }
}

impl PdfImageSettings {
    pub const MIN_ADJUSTMENT: i32 = -100;
    pub const MAX_ADJUSTMENT: i32 = 100;
    pub const DEFAULT_TINT_STRENGTH: i32 = 75;

    /// The settings a book with nothing stored opens with. Dark mode
    /// keeps the page pale rather than white (decision 55), so Tint
    /// is its starting answer; light mode's paper token is white
    /// already, and its pages start untouched.
    pub fn defaults_for_theme(dark: bool, trim_margins: bool) -> Self {
        Self {
            page_colour: if dark {
                PdfPageColour::Tint
            } else {
                PdfPageColour::Off
            },
            trim_margins,
            ..Self::default()
        }
    }

    pub fn canonical(mut self) -> Self {
        self.tint_strength = self.tint_strength.clamp(0, 100);
        self.brightness = self
            .brightness
            .clamp(Self::MIN_ADJUSTMENT, Self::MAX_ADJUSTMENT);
        self.contrast = self
            .contrast
            .clamp(Self::MIN_ADJUSTMENT, Self::MAX_ADJUSTMENT);
        self.warmth = self
            .warmth
            .clamp(Self::MIN_ADJUSTMENT, Self::MAX_ADJUSTMENT);
        self
    }
}

/// A `#rrggbb` theme token as linear-looking 0..1 channel values.
/// The colour matrix works in the texture's own sRGB byte space,
/// which is also the space the token was chosen in, so no transfer
/// function belongs here. Anything unrecognised is white, the paper
/// a tint would then harmlessly leave alone.
pub fn rgb_from_hex(value: &str) -> [f32; 3] {
    let Some(hex) = value.strip_prefix('#') else {
        return [1.0, 1.0, 1.0];
    };
    if hex.len() != 6 {
        return [1.0, 1.0, 1.0];
    }
    let channel = |part: &str| {
        u8::from_str_radix(part, 16)
            .map(|byte| byte as f32 / 255.0)
            .unwrap_or(1.0)
    };
    [
        channel(&hex[0..2]),
        channel(&hex[2..4]),
        channel(&hex[4..6]),
    ]
}

/// One affine colour transform in row-major order. Each matrix row
/// produces one output RGBA channel from the four input channels.
#[derive(Debug, Clone, Copy, PartialEq)]
struct ColorTransform {
    matrix: [[f32; 4]; 4],
    offset: [f32; 4],
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

    fn is_identity(&self) -> bool {
        *self == Self::identity()
    }

    fn row_major_floats(&self) -> [f32; 16] {
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

/// Compose the Image pane's controls in the order a reader would
/// expect: warmth first, contrast around middle grey with an
/// additive brightness shift, grayscale after those adjustments, and
/// the page-colour treatment last. Tint is a per-channel gain that
/// takes white to the palette's paper (scaled by its strength), so
/// black ink stays black and illustrations keep their hue while the
/// paper stops glaring. Alpha is never touched.
fn color_transform(settings: PdfImageSettings, paper_rgb: [f32; 3]) -> ColorTransform {
    let settings = settings.canonical();
    let mut transform = ColorTransform::identity();

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
        // Rec. 709 luma: the weights display pipelines use for sRGB
        // material, green-heavy enough that coloured print keeps
        // something close to its perceived weight.
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

    match settings.page_colour {
        PdfPageColour::Off => {}
        PdfPageColour::Tint => {
            let strength = settings.tint_strength as f32 / 100.0;
            for ((row, offset), paper) in transform.matrix[..3]
                .iter_mut()
                .zip(transform.offset[..3].iter_mut())
                .zip(paper_rgb.iter())
            {
                let gain = 1.0 - strength * (1.0 - paper.clamp(0.0, 1.0));
                for coeff in row {
                    *coeff *= gain;
                }
                *offset *= gain;
            }
        }
        PdfPageColour::Invert => {
            for channel in 0..3 {
                for coeff in &mut transform.matrix[channel] {
                    *coeff = -*coeff;
                }
                transform.offset[channel] = 1.0 - transform.offset[channel];
            }
        }
    }

    transform
}

glib::wrapper! {
    pub struct PdfPagePicture(ObjectSubclass<imp::PdfPagePicture>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl PdfPagePicture {
    pub fn new(settings: PdfImageSettings, paper_rgb: [f32; 3]) -> Self {
        let picture: Self = glib::Object::builder()
            .property("accessible-role", gtk::AccessibleRole::Img)
            .build();
        picture.imp().settings.set(settings.canonical());
        picture.imp().paper_rgb.set(paper_rgb);
        // The page overlay above this widget owns pointing; the
        // picture itself is only ever painted.
        picture.set_can_target(false);
        picture.set_can_focus(false);
        picture.set_focusable(false);
        picture.set_overflow(gtk::Overflow::Hidden);
        picture
    }

    pub fn set_texture(&self, texture: Option<&gdk::Texture>) {
        let next = texture.cloned();
        if self.imp().texture.replace(next.clone()) != next {
            self.queue_resize();
            self.queue_draw();
        }
    }

    /// Repaint with new colour settings. Only a queue-draw: the
    /// texture underneath is the same MuPDF raster throughout.
    pub fn set_image_settings(&self, settings: PdfImageSettings, paper_rgb: [f32; 3]) {
        let settings = settings.canonical();
        let settings_changed = self.imp().settings.replace(settings) != settings;
        let paper_changed = self.imp().paper_rgb.replace(paper_rgb) != paper_rgb;
        if settings_changed || paper_changed {
            self.queue_draw();
        }
    }
}

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct PdfPagePicture {
        pub(super) texture: RefCell<Option<gdk::Texture>>,
        pub(super) settings: Cell<PdfImageSettings>,
        pub(super) paper_rgb: Cell<[f32; 3]>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PdfPagePicture {
        const NAME: &'static str = "KalamPdfPagePicture";
        type Type = super::PdfPagePicture;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for PdfPagePicture {}

    impl WidgetImpl for PdfPagePicture {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let texture = self.texture.borrow();
            let Some(texture) = texture.as_ref() else {
                return (0, 0, -1, -1);
            };
            let source_w = texture.width().max(1) as f32;
            let source_h = texture.height().max(1) as f32;

            // The viewport builders size every page explicitly, and
            // the raster behind it is rendered at 1.5x that size.
            // Like the can-shrink GtkPicture this replaces, the
            // requested display size — not the texture's own — is
            // the natural size when one has been asked for.
            let (requested_w, requested_h) = self.obj().size_request();
            let requested = match orientation {
                gtk::Orientation::Horizontal => requested_w,
                _ => requested_h,
            };
            if requested > 0 {
                return (requested, requested, -1, -1);
            }

            // One pixel is a legal minimum and the page's own aspect
            // gives the natural size.
            let natural = match orientation {
                gtk::Orientation::Horizontal => {
                    if for_size > 0 {
                        for_size as f32 * source_w / source_h
                    } else {
                        source_w
                    }
                }
                _ => {
                    if for_size > 0 {
                        for_size as f32 * source_h / source_w
                    } else {
                        source_h
                    }
                }
            };
            (1, natural.round().max(1.0) as i32, -1, -1)
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            // The widget's own CSS ground first — white paper and
            // shadow under a loaded page, the placeholder surface
            // under an evicted one — then the page over it.
            self.parent_snapshot(snapshot);
            let Some(texture) = self.texture.borrow().clone() else {
                return;
            };
            let widget = self.obj();
            let allocated_w = widget.width() as f32;
            let allocated_h = widget.height() as f32;
            let texture_w = texture.width().max(1) as f32;
            let texture_h = texture.height().max(1) as f32;
            if allocated_w <= 0.0 || allocated_h <= 0.0 {
                return;
            }

            // Contain the page in the allocation, centred — the
            // GtkPicture content-fit this widget stands in for.
            let scale = (allocated_w / texture_w).min(allocated_h / texture_h);
            let dest_w = texture_w * scale;
            let dest_h = texture_h * scale;
            let destination = gtk::graphene::Rect::new(
                (allocated_w - dest_w) / 2.0,
                (allocated_h - dest_h) / 2.0,
                dest_w,
                dest_h,
            );

            snapshot.push_clip(&destination);
            let transform = color_transform(self.settings.get(), self.paper_rgb.get());
            let colour_pushed = !transform.is_identity();
            if colour_pushed {
                // GtkSnapshot applies transpose(matrix) to each
                // pixel. Our transform rows are already the output
                // channels, so hand GSK the transpose of that
                // row-major matrix.
                let matrix =
                    gtk::graphene::Matrix::from_float(transform.row_major_floats()).transpose();
                let offset = gtk::graphene::Vec4::new(
                    transform.offset[0],
                    transform.offset[1],
                    transform.offset[2],
                    transform.offset[3],
                );
                snapshot.push_color_matrix(&matrix, &offset);
            }
            snapshot.append_scaled_texture(
                &texture,
                gtk::gsk::ScalingFilter::Trilinear,
                &destination,
            );
            if colour_pushed {
                snapshot.pop();
            }
            snapshot.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAPER: [f32; 3] = [207.0 / 255.0, 198.0 / 255.0, 187.0 / 255.0];

    fn settings(page_colour: PdfPageColour) -> PdfImageSettings {
        PdfImageSettings {
            page_colour,
            ..PdfImageSettings::default()
        }
    }

    fn assert_pixel(actual: [f32; 4], expected: [f32; 3]) {
        for channel in 0..3 {
            assert!(
                (actual[channel] - expected[channel]).abs() < 0.0001,
                "channel {channel}: {actual:?} != {expected:?}"
            );
        }
        assert_eq!(actual[3], 1.0, "alpha is never touched: {actual:?}");
    }

    #[test]
    fn off_and_neutral_is_the_identity() {
        let transform = color_transform(PdfImageSettings::default(), PAPER);
        assert!(transform.is_identity());
        assert_pixel(
            transform.apply_clamped([0.25, 0.5, 0.75, 1.0]),
            [0.25, 0.5, 0.75],
        );
    }

    #[test]
    fn tint_takes_white_toward_paper_and_keeps_black() {
        let full = PdfImageSettings {
            tint_strength: 100,
            ..settings(PdfPageColour::Tint)
        };
        let transform = color_transform(full, PAPER);
        assert_pixel(transform.apply_clamped([1.0, 1.0, 1.0, 1.0]), PAPER);
        assert_pixel(
            transform.apply_clamped([0.0, 0.0, 0.0, 1.0]),
            [0.0, 0.0, 0.0],
        );

        // Half strength, and none, land where the slider says.
        let half = PdfImageSettings {
            tint_strength: 50,
            ..settings(PdfPageColour::Tint)
        };
        let white = color_transform(half, PAPER).apply_clamped([1.0, 1.0, 1.0, 1.0]);
        assert_pixel(
            white,
            [
                (1.0 + PAPER[0]) / 2.0,
                (1.0 + PAPER[1]) / 2.0,
                (1.0 + PAPER[2]) / 2.0,
            ],
        );
        let none = PdfImageSettings {
            tint_strength: 0,
            ..settings(PdfPageColour::Tint)
        };
        assert!(color_transform(none, PAPER).is_identity());
        // A palette whose paper is white tints to nothing at all.
        assert!(color_transform(full, [1.0, 1.0, 1.0]).is_identity());
    }

    #[test]
    fn invert_is_a_negative() {
        let transform = color_transform(settings(PdfPageColour::Invert), PAPER);
        assert_pixel(
            transform.apply_clamped([1.0, 1.0, 1.0, 1.0]),
            [0.0, 0.0, 0.0],
        );
        assert_pixel(
            transform.apply_clamped([0.0, 0.0, 0.0, 1.0]),
            [1.0, 1.0, 1.0],
        );
        assert_pixel(
            transform.apply_clamped([0.2, 0.4, 0.6, 1.0]),
            [0.8, 0.6, 0.4],
        );
    }

    #[test]
    fn adjustments_compose_like_the_comic_panes() {
        let bright = PdfImageSettings {
            brightness: 20,
            contrast: 100,
            ..PdfImageSettings::default()
        };
        // Middle grey is contrast's pivot, then gains 0.1 brightness.
        assert_pixel(
            color_transform(bright, PAPER).apply_clamped([0.5, 0.5, 0.5, 1.0]),
            [0.6, 0.6, 0.6],
        );

        let warm = PdfImageSettings {
            warmth: 100,
            ..PdfImageSettings::default()
        };
        assert_pixel(
            color_transform(warm, PAPER).apply_clamped([0.5, 0.5, 0.5, 1.0]),
            [0.61, 0.53, 0.39],
        );

        let grey = PdfImageSettings {
            grayscale: true,
            ..PdfImageSettings::default()
        };
        let luma = 0.2126 * 0.2 + 0.7152 * 0.4 + 0.0722 * 0.6;
        assert_pixel(
            color_transform(grey, PAPER).apply_clamped([0.2, 0.4, 0.6, 1.0]),
            [luma, luma, luma],
        );
    }

    #[test]
    fn defaults_and_canonical_clamps() {
        assert_eq!(
            PdfImageSettings::defaults_for_theme(true, false).page_colour,
            PdfPageColour::Tint
        );
        assert_eq!(
            PdfImageSettings::defaults_for_theme(false, true),
            PdfImageSettings {
                trim_margins: true,
                ..PdfImageSettings::default()
            }
        );
        let wild = PdfImageSettings {
            tint_strength: 140,
            brightness: -400,
            contrast: 400,
            warmth: 101,
            ..PdfImageSettings::default()
        }
        .canonical();
        assert_eq!(wild.tint_strength, 100);
        assert_eq!(wild.brightness, -100);
        assert_eq!(wild.contrast, 100);
        assert_eq!(wild.warmth, 100);
    }

    #[test]
    fn paper_hex_parses_and_falls_back_to_white() {
        assert_eq!(rgb_from_hex("#cfc6bb"), PAPER);
        assert_eq!(rgb_from_hex("not a colour"), [1.0, 1.0, 1.0]);
        assert_eq!(rgb_from_hex("#fff"), [1.0, 1.0, 1.0]);
    }
}
