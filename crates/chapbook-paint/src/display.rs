//! The paint-neutral display list: dumb draw ops in page coordinates
//! (CSS px), executed by any backend without re-shaping or style access.

use chapbook_core::{PanelRect, Point, Rect, Rgba, Rotation, Size, UpdateClass};

use crate::images::{ImageLook, ImageStore};
use crate::page::{FragmentKind, Glyph, Page};

#[derive(Debug, Clone)]
pub struct DisplayList {
    /// Full page size in CSS px; backends scale to device pixels.
    pub size: Size,
    pub ops: Vec<DisplayOp>,
}

impl DisplayList {
    /// Where on the page a panel should diffuse quantization error:
    /// wherever an image is, and nowhere else.
    ///
    /// The list is the only thing that knows the difference between a
    /// photograph and a paragraph by the time pixels exist, so the answer
    /// has to come from here. Hand the result to
    /// `mezzotint::encode::quantize_for` as its diffusion regions.
    ///
    /// Device pixels in page orientation, rounded outward — the space a
    /// rasterized page is in before [`rotate`](crate::rotate) turns it,
    /// which is also where quantization happens.
    pub fn dither_regions(&self, scale: f32) -> Vec<PanelRect> {
        self.ops
            .iter()
            .filter_map(|op| match op {
                DisplayOp::Image { dest, .. } => {
                    Some(crate::panel_rect(*dest, self.size, scale, Rotation::None))
                }
                _ => None,
            })
            .filter(|rect| !rect.is_empty())
            .collect()
    }
}

#[derive(Debug, Clone)]
pub enum DisplayOp {
    FillRect {
        rect: Rect,
        color: Rgba,
    },
    /// kalam: a selection band — a fill with rounded corners, composited
    /// with `blend` so the tint sits *behind* the ink rather than over the
    /// page: multiply on a light page darkens paper and leaves black text
    /// black; screen on a dark page lightens paper and leaves light text
    /// light. A plain `FillRect` is the right op for everything else, and
    /// a backend without blend modes may paint this one as a `FillRect`.
    Band {
        rect: Rect,
        color: Rgba,
        radius: f32,
        blend: Blend,
    },
    /// Positioned glyphs sharing one face/size/weight/color. `origin` is the
    /// baseline origin in page coordinates; glyph offsets are relative to it.
    GlyphRun {
        font: cosmic_text::fontdb::ID,
        font_size: f32,
        font_weight: u16,
        color: Rgba,
        origin: Point,
        glyphs: Vec<Glyph>,
    },
    /// A raster image scaled into `dest`, resolved via the `ImageStore`.
    ///
    /// kalam: `treatment` says how the pixels meet the page — as they
    /// are, multiplied into light paper, or inverted onto a dark ground.
    /// Chosen by [`build_display_list`]; a backend applies it as it draws.
    Image {
        resource: u64,
        dest: Rect,
        treatment: ImageTreatment,
    },
}

/// kalam: how a [`DisplayOp::Image`] is painted onto the page ground.
///
/// The old WebKit reader gave every image `mix-blend-mode: multiply` on
/// its light themes and `filter: invert(1)` + `mix-blend-mode: screen` on
/// its night themes, and excepted covers and photographs from both by
/// their class names. The engine keeps the two effects and makes the
/// exception from the pixels instead ([`ImageLook`]) — a class name is a
/// guess about the pixels, and the pixels are right here.
///
/// | ground | paper | picture / unknown |
/// |---|---|---|
/// | light | `Multiply` (`Plain` over pure white, where it is the same) | `Plain` |
/// | dark | `Invert` | `Plain` |
///
/// Image-book pages (comics, PDF) are not paper in the sense meant here
/// even when their pixels say so; a shell reviving that pipeline should
/// pin them to `Plain`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ImageTreatment {
    /// Paint the pixels as decoded, over the page: a picture on any
    /// ground (a negative is worse than a bright photograph; a tinted
    /// photograph is worse than a true one), and anything the store does
    /// not know.
    #[default]
    Plain,
    /// Paint as decoded, composited with [`Blend::Multiply`]: paper on a
    /// light ground. White takes the paper's tint and ink stays ink, so
    /// the plate loses its rectangle. Over pure white it is source-over.
    Multiply,
    /// Invert each colour channel (alpha untouched), then composite with
    /// [`Blend::Screen`]: paper on a dark ground. Black ink becomes
    /// light, the white ground becomes black, and screen makes black
    /// take the page colour — the plate reads like the text around it.
    Invert,
}

impl ImageTreatment {
    /// The treatment for an image of `look` on a page ground of
    /// `background` — the table above, with "dark" meaning a Rec. 601
    /// luma below 128 ([`is_dark_ground`], the selection band's test).
    ///
    /// Over pure white, multiply *is* source-over (`s + d·(1 − sa)` with
    /// `d = 1`), so paper on a white page is `Plain`: the same pixels for
    /// less work, and the light render goldens are untouched.
    pub fn for_look(look: ImageLook, background: Rgba) -> ImageTreatment {
        match (look, is_dark_ground(background)) {
            (ImageLook::Picture, _) => ImageTreatment::Plain,
            (ImageLook::Paper, false) if background == Rgba::WHITE => ImageTreatment::Plain,
            (ImageLook::Paper, false) => ImageTreatment::Multiply,
            (ImageLook::Paper, true) => ImageTreatment::Invert,
        }
    }
}

/// kalam: is this page ground dark — Rec. 601 luma below 128, the usual
/// test. One definition for everything that turns on it: the selection
/// band's blend, an image's treatment, and whether a session stores its
/// paper images as negatives.
pub fn is_dark_ground(bg: Rgba) -> bool {
    let luma = 0.299 * f32::from(bg.r) + 0.587 * f32::from(bg.g) + 0.114 * f32::from(bg.b);
    luma < 128.0
}

/// What changed since the previous frame — the signal an e-ink shell needs
/// to choose between a full flash and a fast partial refresh, and any
/// backend needs to decide how much work to redo. Only the engine can know
/// this, so it travels with the frame.
///
/// Ordered by how much of the page the change disturbs: when several things
/// happen before a frame is taken, the strongest one describes it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum FrameIntent {
    /// Nothing changed; the same content, painted again.
    #[default]
    Repaint,
    /// The selection moved — typically a few lines.
    Selection,
    /// A stored highlight appeared or went away.
    Annotation,
    /// A background load landed: a comic page, a PDF rasterization.
    ContentArrived,
    /// A page turn within the same unit.
    PageTurn,
    /// A different spine unit.
    UnitChange,
    /// Everything reflowed: font size, theme, page metrics.
    Relayout,
}

impl FrameIntent {
    /// What a panel has to do to show this change — the join between what
    /// the engine knows (the kind of change) and what a device knows (the
    /// waveform it calls that).
    ///
    /// Each row is the *least* disruptive update that still renders the
    /// change faithfully; `mezzotint::RefreshPolicy` decides separately
    /// when to spend a flash the content did not ask for.
    ///
    /// `Relayout` is the one row that asks for more than it strictly
    /// needs. Every pixel changed anyway, so the flash costs nothing the
    /// user was not already going to see — and it pays off the ghosting
    /// debt for free, at the moment it is cheapest.
    ///
    /// `None` for [`FrameIntent::Repaint`]: nothing changed, so there is
    /// nothing for a panel to do. That is the absence of an update rather
    /// than a kind of one, which is why it is an `Option` here instead of
    /// a variant every backend would have to write a dead match arm for.
    pub fn update_class(self) -> Option<UpdateClass> {
        match self {
            FrameIntent::Repaint => None,
            FrameIntent::Selection => Some(UpdateClass::Monochrome),
            FrameIntent::Annotation => Some(UpdateClass::Fast),
            FrameIntent::ContentArrived | FrameIntent::PageTurn | FrameIntent::UnitChange => {
                Some(UpdateClass::Quality)
            }
            FrameIntent::Relayout => Some(UpdateClass::Flash),
        }
    }
}

/// A page's display ops plus what a backend needs to decide how to put them
/// on screen.
#[derive(Debug, Clone)]
pub struct Frame {
    pub list: DisplayList,
    pub intent: FrameIntent,
    /// The region the change disturbs, when the producer can state it more
    /// cheaply than the backend can repaint. `None` means "assume the whole
    /// page" — always correct, just not always minimal.
    pub damage: Option<Rect>,
}

/// kalam: how a [`DisplayOp::Band`] composites onto what is under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Blend {
    /// Source-over: the plain translucent fill every other op uses.
    #[default]
    Normal,
    /// Darkens; a no-op over black ink. For light pages.
    Multiply,
    /// Lightens; a no-op over white ink. For dark pages.
    Screen,
}

/// A range to highlight: locator range plus fill color, painted per line
/// under the text. Stored highlights and the transient selection are the
/// same op — only the color differs.
///
/// kalam: and the blend. Stored highlights keep `Blend::Normal` (their
/// colours were chosen as translucent fills); the live selection asks for
/// the page's blend so its tint reads like a marker over the paper.
#[derive(Debug, Clone, Copy)]
pub struct Selection {
    pub start: u32,
    pub end: u32,
    pub color: Rgba,
    pub blend: Blend,
}

/// kalam: corner radius of a selection band, CSS px — the old reader's
/// `.kalam-selection-band { border-radius: 2px }`.
pub const BAND_RADIUS: f32 = 2.0;

/// Flatten a laid-out page into draw ops. `background` becomes the first op
/// (a full-page fill), so themes (night mode) are a color choice here, not a
/// renderer concern. Each of `selections` paints its highlight rect
/// immediately before each line it touches — over earlier backgrounds,
/// under the text — in slice order, so a later one covers an earlier one.
///
/// kalam: `images` is consulted for each image's [`ImageLook`], which with
/// `background` fixes the op's [`ImageTreatment`]; an image the store does
/// not hold (or an empty store) paints plain, as before.
pub fn build_display_list(
    page: &Page,
    background: Rgba,
    selections: &[Selection],
    images: &ImageStore,
) -> DisplayList {
    let mut ops = vec![DisplayOp::FillRect {
        rect: Rect::new(0.0, 0.0, page.size.w, page.size.h),
        color: background,
    }];

    for fragment in &page.fragments {
        match &fragment.kind {
            // Hidden text paints only its selection highlight — the pixels
            // are in the raster underneath.
            FragmentKind::HiddenText(line) => {
                for sel in selections {
                    push_selection_rect(&mut ops, fragment, line, *sel);
                }
            }
            FragmentKind::Line(line) => {
                for sel in selections {
                    push_selection_rect(&mut ops, fragment, line, *sel);
                }
                let origin = Point::new(
                    fragment.rect.origin.x,
                    fragment.rect.origin.y + line.baseline,
                );
                // Decorations paint under the glyphs, like browsers do.
                for deco in &line.decorations {
                    ops.push(DisplayOp::FillRect {
                        rect: Rect::new(
                            fragment.rect.origin.x + deco.x,
                            fragment.rect.origin.y + deco.y,
                            deco.width,
                            deco.thickness,
                        ),
                        color: deco.color,
                    });
                }
                for run in &line.runs {
                    ops.push(DisplayOp::GlyphRun {
                        font: run.font,
                        font_size: run.font_size,
                        font_weight: run.font_weight,
                        color: run.color,
                        origin,
                        glyphs: run.glyphs.clone(),
                    });
                }
            }
            FragmentKind::Rule { color } => {
                ops.push(DisplayOp::FillRect {
                    rect: fragment.rect,
                    color: *color,
                });
            }
            FragmentKind::Image { resource } => {
                let treatment = match images.look(*resource) {
                    Some(look) => ImageTreatment::for_look(look, background),
                    None => ImageTreatment::Plain,
                };
                ops.push(DisplayOp::Image {
                    resource: *resource,
                    dest: fragment.rect,
                    treatment,
                });
            }
            FragmentKind::Box(decoration) => {
                push_box_decoration(&mut ops, &fragment.rect, decoration);
            }
        }
    }

    DisplayList {
        size: page.size,
        ops,
    }
}

/// The per-line selection highlight: the union of the selected glyphs'
/// extents, painted before the line's own ops.
///
/// kalam: the fill is the line's *band* — glyph box plus 2 px, see
/// `LineFragment::band_extent` — not the whole line box, with rounded
/// corners and the selection's blend. `Page::rects_for_range` answers
/// with the same rects, so geometry a shell reads matches what it sees.
fn push_selection_rect(
    ops: &mut Vec<DisplayOp>,
    fragment: &crate::page::Fragment,
    line: &crate::page::LineFragment,
    sel: Selection,
) {
    // One fill per visually contiguous piece — see
    // `LineFragment::selected_spans` for why a bidi line can need two.
    // Allocates per line per selection, which a drag event pays; the
    // alternative is threading a scratch buffer through the whole walk for
    // a vector that is almost always one element long.
    let mut spans = Vec::new();
    line.selected_spans(sel.start, sel.end, &mut spans);
    if spans.is_empty() {
        return;
    }
    let (top, bottom) = line.band_extent(fragment.rect.size.h);
    for (from, to) in spans {
        ops.push(DisplayOp::Band {
            rect: Rect::new(
                fragment.rect.origin.x + from,
                fragment.rect.origin.y + top,
                to - from,
                bottom - top,
            ),
            color: sel.color,
            radius: BAND_RADIUS,
            blend: sel.blend,
        });
    }
}

/// Background fill + solid border edges. Horizontal edges paint only on the
/// slices that carry them; vertical edges paint on every slice.
fn push_box_decoration(
    ops: &mut Vec<DisplayOp>,
    rect: &Rect,
    decoration: &crate::page::BoxDecoration,
) {
    if let Some(background) = decoration.background {
        if !background.is_transparent() {
            ops.push(DisplayOp::FillRect {
                rect: *rect,
                color: background,
            });
        }
    }
    let w = &decoration.border_widths;
    let color = decoration.border_color;
    if color.is_transparent() {
        return;
    }
    let (x, y) = (rect.origin.x, rect.origin.y);
    let (bw, bh) = (rect.size.w, rect.size.h);
    if decoration.first_slice && w.top > 0.0 {
        ops.push(DisplayOp::FillRect {
            rect: Rect::new(x, y, bw, w.top),
            color,
        });
    }
    if decoration.last_slice && w.bottom > 0.0 {
        ops.push(DisplayOp::FillRect {
            rect: Rect::new(x, y + bh - w.bottom, bw, w.bottom),
            color,
        });
    }
    if w.left > 0.0 {
        ops.push(DisplayOp::FillRect {
            rect: Rect::new(x, y, w.left, bh),
            color,
        });
    }
    if w.right > 0.0 {
        ops.push(DisplayOp::FillRect {
            rect: Rect::new(x + bw - w.right, y, w.right, bh),
            color,
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::page::Fragment;

    /// A page holding one image fragment keyed `resource`.
    fn image_page(resource: u64) -> Page {
        Page {
            size: Size::new(600.0, 800.0),
            content: Rect::new(40.0, 40.0, 520.0, 720.0),
            fragments: vec![Fragment {
                rect: Rect::new(100.0, 100.0, 64.0, 48.0),
                kind: FragmentKind::Image { resource },
                tag: resource,
            }],
        }
    }

    /// A store holding a white plate with a black rule (paper) under 1
    /// and a saturated red square (picture) under 2.
    fn store() -> ImageStore {
        let mut store = ImageStore::default();
        let mut plate = [255u8, 255, 255, 255].repeat(64 * 40);
        plate.extend([0u8, 0, 0, 255].repeat(64 * 8));
        store.insert(1, 64, 48, plate);
        store.insert(2, 64, 48, [200u8, 60, 60, 255].repeat(64 * 48));
        store
    }

    fn treatment_of(list: &DisplayList) -> ImageTreatment {
        list.ops
            .iter()
            .find_map(|op| match op {
                DisplayOp::Image { treatment, .. } => Some(*treatment),
                _ => None,
            })
            .expect("an image op")
    }

    const DARK: Rgba = Rgba::new(0x1b, 0x1e, 0x24, 255);
    const SEPIA: Rgba = Rgba::new(246, 240, 226, 255);

    #[test]
    fn paper_inverts_on_a_dark_ground_and_multiplies_on_a_light_one() {
        let store = store();
        let dark = build_display_list(&image_page(1), DARK, &[], &store);
        assert_eq!(treatment_of(&dark), ImageTreatment::Invert);
        // Kalam's Light paper and the engine's Sepia are light grounds
        // with a tint to take.
        for ground in [Rgba::new(0xfa, 0xf8, 0xf5, 255), SEPIA] {
            let light = build_display_list(&image_page(1), ground, &[], &store);
            let treatment = treatment_of(&light);
            assert_eq!(treatment, ImageTreatment::Multiply, "{ground:?}");
        }
        // Pure white has no tint to take: plain, which paints the same.
        let white = build_display_list(&image_page(1), Rgba::WHITE, &[], &store);
        assert_eq!(treatment_of(&white), ImageTreatment::Plain);
    }

    #[test]
    fn pictures_stay_plain_on_every_ground() {
        let store = store();
        for ground in [DARK, Rgba::WHITE, SEPIA] {
            let list = build_display_list(&image_page(2), ground, &[], &store);
            assert_eq!(treatment_of(&list), ImageTreatment::Plain, "{ground:?}");
        }
    }

    #[test]
    fn unknown_images_are_plain_on_every_ground() {
        // Not in the store (undecoded, or an empty store): plain, and the
        // op is still emitted for a backend that can resolve it.
        let store = store();
        for ground in [DARK, Rgba::WHITE] {
            let unknown = build_display_list(&image_page(9), ground, &[], &store);
            assert_eq!(treatment_of(&unknown), ImageTreatment::Plain);
            let empty = build_display_list(&image_page(1), ground, &[], &ImageStore::default());
            assert_eq!(treatment_of(&empty), ImageTreatment::Plain);
        }
    }

    #[test]
    fn the_ground_test_is_luma_not_any_one_channel() {
        // Kalam's Ink theme and the engine's own Dark ground are dark.
        assert!(is_dark_ground(Rgba::new(0x0d, 0x0d, 0x0d, 255)));
        assert!(is_dark_ground(Rgba::new(18, 18, 18, 255)));
        // A saturated blue is dark; a saturated yellow is not.
        assert!(is_dark_ground(Rgba::new(0, 0, 255, 255)));
        assert!(!is_dark_ground(Rgba::new(255, 255, 0, 255)));
        assert!(!is_dark_ground(Rgba::WHITE));
    }
}
