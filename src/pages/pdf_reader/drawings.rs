//! PDF drawing tools (decisions 66–67, 69, 72): the bottom-centre
//! annotation toolbar, freehand pen/highlighter strokes, shapes,
//! text boxes, sticky notes, the eraser, and selecting a saved
//! object to recolour, reweight, move or delete.
//!
//! Everything drawn lives in source-page points against a stable
//! page identity. Display pixels exist only at the two edges of this
//! file — gestures coming in, Cairo going out — so zoom, spreads and
//! view rotation never change what a saved object means.

use super::*;
use crate::db::{PdfDrawing, PdfDrawingGeometry};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfDrawTool {
    #[default]
    Select,
    Highlighter,
    Pen,
    Eraser,
    Shape,
    TextBox,
    Sticky,
}

impl PdfDrawTool {
    pub const ALL: &'static [PdfDrawTool] = &[
        PdfDrawTool::Select,
        PdfDrawTool::Highlighter,
        PdfDrawTool::Pen,
        PdfDrawTool::Eraser,
        PdfDrawTool::Shape,
        PdfDrawTool::TextBox,
        PdfDrawTool::Sticky,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Highlighter => "Highlighter",
            Self::Pen => "Pen",
            Self::Eraser => "Eraser",
            Self::Shape => "Shape",
            Self::TextBox => "Text box",
            Self::Sticky => "Sticky note",
        }
    }

    pub fn icon_name(self) -> &'static str {
        match self {
            Self::Select => "kalam-draw-select-symbolic",
            Self::Highlighter => "kalam-highlight-symbolic",
            Self::Pen => "kalam-draw-pen-symbolic",
            Self::Eraser => "kalam-draw-eraser-symbolic",
            Self::Shape => "kalam-draw-shape-symbolic",
            Self::TextBox => "kalam-draw-text-symbolic",
            Self::Sticky => "kalam-draw-sticky-symbolic",
        }
    }

    pub fn is_select(self) -> bool {
        self == Self::Select
    }
}

/// The five highlight hues plus the fixed ink of decision 67.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfInkColor {
    Yellow,
    Green,
    Blue,
    Pink,
    Orange,
    #[default]
    Ink,
}

impl PdfInkColor {
    pub const HIGHLIGHTER: &'static [PdfInkColor] = &[
        PdfInkColor::Yellow,
        PdfInkColor::Green,
        PdfInkColor::Blue,
        PdfInkColor::Pink,
        PdfInkColor::Orange,
    ];

    pub const ALL: &'static [PdfInkColor] = &[
        PdfInkColor::Yellow,
        PdfInkColor::Green,
        PdfInkColor::Blue,
        PdfInkColor::Pink,
        PdfInkColor::Orange,
        PdfInkColor::Ink,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Yellow => "yellow",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Pink => "pink",
            Self::Orange => "orange",
            Self::Ink => "ink",
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "yellow" => Self::Yellow,
            "green" => Self::Green,
            "blue" => Self::Blue,
            "pink" | "rose" => Self::Pink,
            "orange" => Self::Orange,
            _ => Self::Ink,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Pink => "Pink",
            Self::Orange => "Orange",
            Self::Ink => "Ink",
        }
    }

    /// Full-strength RGB. The five hues are design-system §9's fixed
    /// palette; ink is decision 67's `#2A2420` in every theme.
    pub fn rgb(self) -> (f64, f64, f64) {
        match self {
            Self::Yellow => (232.0 / 255.0, 185.0 / 255.0, 58.0 / 255.0),
            Self::Green => (111.0 / 255.0, 179.0 / 255.0, 111.0 / 255.0),
            Self::Blue => (91.0 / 255.0, 155.0 / 255.0, 213.0 / 255.0),
            Self::Pink => (217.0 / 255.0, 123.0 / 255.0, 168.0 / 255.0),
            Self::Orange => (224.0 / 255.0, 138.0 / 255.0, 75.0 / 255.0),
            Self::Ink => (42.0 / 255.0, 36.0 / 255.0, 32.0 / 255.0),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfStrokeWidth {
    Thin,
    #[default]
    Medium,
    Thick,
}

impl PdfStrokeWidth {
    pub const ALL: &'static [PdfStrokeWidth] = &[
        PdfStrokeWidth::Thin,
        PdfStrokeWidth::Medium,
        PdfStrokeWidth::Thick,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Thin => "Thin",
            Self::Medium => "Medium",
            Self::Thick => "Thick",
        }
    }

    pub fn pts(self) -> f64 {
        match self {
            Self::Thin => 1.5,
            Self::Medium => 3.0,
            Self::Thick => 5.0,
        }
    }

    pub fn from_pts(pts: f64) -> Self {
        if pts <= 2.25 {
            Self::Thin
        } else if pts <= 4.0 {
            Self::Medium
        } else {
            Self::Thick
        }
    }

    pub fn from_pref(value: Option<&str>) -> Self {
        match value {
            Some("thin") => Self::Thin,
            Some("thick") => Self::Thick,
            _ => Self::Medium,
        }
    }

    pub fn pref_value(self) -> &'static str {
        match self {
            Self::Thin => "thin",
            Self::Medium => "medium",
            Self::Thick => "thick",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfShapeKind {
    #[default]
    Rectangle,
    Ellipse,
    Line,
    Arrow,
}

impl PdfShapeKind {
    pub const ALL: &'static [PdfShapeKind] = &[
        PdfShapeKind::Rectangle,
        PdfShapeKind::Ellipse,
        PdfShapeKind::Line,
        PdfShapeKind::Arrow,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Rectangle => "rectangle",
            Self::Ellipse => "ellipse",
            Self::Line => "line",
            Self::Arrow => "arrow",
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "ellipse" | "circle" | "oval" => Self::Ellipse,
            "line" => Self::Line,
            "arrow" => Self::Arrow,
            _ => Self::Rectangle,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Rectangle => "Rectangle",
            Self::Ellipse => "Ellipse",
            Self::Line => "Line",
            Self::Arrow => "Arrow",
        }
    }

    pub fn icon_name(self) -> &'static str {
        match self {
            Self::Rectangle => "kalam-shape-rectangle-symbolic",
            Self::Ellipse => "kalam-shape-ellipse-symbolic",
            Self::Line => "kalam-shape-line-symbolic",
            Self::Arrow => "kalam-shape-arrow-symbolic",
        }
    }

    pub fn can_fill(self) -> bool {
        matches!(self, Self::Rectangle | Self::Ellipse)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PdfTextSize {
    Small,
    #[default]
    Medium,
    Large,
}

impl PdfTextSize {
    pub const ALL: &'static [PdfTextSize] = &[
        PdfTextSize::Small,
        PdfTextSize::Medium,
        PdfTextSize::Large,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Small => "s",
            Self::Medium => "m",
            Self::Large => "l",
        }
    }

    pub fn from_name(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "s" | "small" => Self::Small,
            "l" | "large" => Self::Large,
            _ => Self::Medium,
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Self::Small => "S",
            Self::Medium => "M",
            Self::Large => "L",
        }
    }

    pub fn pts(self) -> f32 {
        match self {
            Self::Small => 10.0,
            Self::Medium => 14.0,
            Self::Large => 20.0,
        }
    }
}

/// The tools' remembered answers. Global rather than per book: the
/// colour in the pen is a property of the reader's hand, not the book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfDrawingSettings {
    pub highlighter_color: PdfInkColor,
    pub highlighter_opacity_percent: u8,
    pub pen_color: PdfInkColor,
    pub pen_width: PdfStrokeWidth,
    pub shape_kind: PdfShapeKind,
    pub shape_filled: bool,
    pub shape_color: PdfInkColor,
    pub shape_width: PdfStrokeWidth,
    pub text_size: PdfTextSize,
    pub text_color: PdfInkColor,
    pub text_paper_background: bool,
    pub sticky_color: PdfInkColor,
}

impl Default for PdfDrawingSettings {
    fn default() -> Self {
        Self {
            highlighter_color: PdfInkColor::Yellow,
            highlighter_opacity_percent: 60,
            pen_color: PdfInkColor::Ink,
            pen_width: PdfStrokeWidth::Medium,
            shape_kind: PdfShapeKind::Rectangle,
            shape_filled: true,
            shape_color: PdfInkColor::Blue,
            shape_width: PdfStrokeWidth::Medium,
            text_size: PdfTextSize::Medium,
            text_color: PdfInkColor::Ink,
            text_paper_background: true,
            sticky_color: PdfInkColor::Yellow,
        }
    }
}

impl PdfDrawingSettings {
    pub fn load(catalog: &Catalog) -> Self {
        let opacity = catalog.get_pref_i64("reader.pdf.draw.highlighter_opacity", 60);
        Self {
            highlighter_color: PdfInkColor::from_name(
                catalog
                    .get_pref("reader.pdf.draw.highlighter_color")
                    .as_deref()
                    .unwrap_or("yellow"),
            ),
            highlighter_opacity_percent: match opacity {
                40 => 40,
                80 => 80,
                _ => 60,
            },
            pen_color: PdfInkColor::from_name(
                catalog
                    .get_pref("reader.pdf.draw.pen_color")
                    .as_deref()
                    .unwrap_or("ink"),
            ),
            pen_width: PdfStrokeWidth::from_pref(
                catalog.get_pref("reader.pdf.draw.pen_width").as_deref(),
            ),
            shape_kind: PdfShapeKind::from_name(
                catalog
                    .get_pref("reader.pdf.draw.shape_kind")
                    .as_deref()
                    .unwrap_or("rectangle"),
            ),
            shape_filled: catalog.get_pref_i64("reader.pdf.draw.shape_filled", 1) != 0,
            shape_color: PdfInkColor::from_name(
                catalog
                    .get_pref("reader.pdf.draw.shape_color")
                    .as_deref()
                    .unwrap_or("blue"),
            ),
            shape_width: PdfStrokeWidth::from_pref(
                catalog.get_pref("reader.pdf.draw.shape_width").as_deref(),
            ),
            text_size: PdfTextSize::from_name(
                catalog
                    .get_pref("reader.pdf.draw.text_size")
                    .as_deref()
                    .unwrap_or("m"),
            ),
            text_color: PdfInkColor::from_name(
                catalog
                    .get_pref("reader.pdf.draw.text_color")
                    .as_deref()
                    .unwrap_or("ink"),
            ),
            text_paper_background: catalog
                .get_pref_i64("reader.pdf.draw.text_paper_background", 1)
                != 0,
            sticky_color: PdfInkColor::from_name(
                catalog
                    .get_pref("reader.pdf.draw.sticky_color")
                    .as_deref()
                    .unwrap_or("yellow"),
            ),
        }
    }

    pub fn persist(&self, catalog: &Catalog) {
        catalog.set_pref(
            "reader.pdf.draw.highlighter_color",
            self.highlighter_color.as_str(),
        );
        catalog.set_pref_i64(
            "reader.pdf.draw.highlighter_opacity",
            self.highlighter_opacity_percent as i64,
        );
        catalog.set_pref("reader.pdf.draw.pen_color", self.pen_color.as_str());
        catalog.set_pref("reader.pdf.draw.pen_width", self.pen_width.pref_value());
        catalog.set_pref("reader.pdf.draw.shape_kind", self.shape_kind.as_str());
        catalog.set_pref_i64(
            "reader.pdf.draw.shape_filled",
            i64::from(self.shape_filled),
        );
        catalog.set_pref("reader.pdf.draw.shape_color", self.shape_color.as_str());
        catalog.set_pref(
            "reader.pdf.draw.shape_width",
            self.shape_width.pref_value(),
        );
        catalog.set_pref("reader.pdf.draw.text_size", self.text_size.as_str());
        catalog.set_pref("reader.pdf.draw.text_color", self.text_color.as_str());
        catalog.set_pref_i64(
            "reader.pdf.draw.text_paper_background",
            i64::from(self.text_paper_background),
        );
        catalog.set_pref(
            "reader.pdf.draw.sticky_color",
            self.sticky_color.as_str(),
        );
    }

    pub fn highlighter_opacity(&self) -> f64 {
        self.highlighter_opacity_percent as f64 / 100.0
    }
}

/// A stroke being drawn right now, shared with the page painter so it
/// appears under the pointer before it is saved.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfActiveStroke {
    pub slot: PageSlot,
    pub page_id: String,
    pub tool: PdfDrawTool,
    pub color: PdfInkColor,
    pub opacity: f64,
    pub width_pts: f64,
    pub points: Vec<(f32, f32)>,
    pub source_width: f32,
    pub source_height: f32,
    pub rotation: i32,
}

/// A shape being dragged into being, shared with the page painter
/// on the same terms as the active stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfActiveShape {
    pub slot: PageSlot,
    pub page_id: String,
    pub kind: PdfShapeKind,
    pub start: (f32, f32),
    pub end: (f32, f32),
    pub filled: bool,
    pub color: PdfInkColor,
    pub opacity: f64,
    pub width_pts: f64,
    pub source_width: f32,
    pub source_height: f32,
    pub rotation: i32,
}

/// The inline editor a text box opens while it is being typed in.
/// The widget lives here, parented to its page overlay, until the
/// words are committed to a drawing row.
pub struct PdfDrawingTextEditor {
    pub slot: PageSlot,
    pub page_id: String,
    pub editing_id: Option<i64>,
    pub original: Option<Box<PdfDrawing>>,
    pub rect: (f32, f32, f32, f32),
    pub size: PdfTextSize,
    pub color: PdfInkColor,
    pub paper_background: bool,
    pub view: gtk::TextView,
}

/// The sticky card currently open: its buffer, and the row as it
/// stood when the card opened, so one card session is one undo step.
pub struct PdfStickyCardState {
    pub id: i64,
    pub buffer: gtk::TextBuffer,
    pub view: gtk::TextView,
    pub original: Option<Box<PdfDrawing>>,
}

/// One saved drawing prepared for a materialised page slot.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfDrawingPaint {
    pub slot: PageSlot,
    pub drawing: PdfDrawing,
    pub source_width: f32,
    pub source_height: f32,
    pub rotation: i32,
    pub selected: bool,
    pub paper_rgb: [f32; 3],
}

#[derive(Debug, Clone)]
pub enum PdfDrawingDragKind {
    Stroke,
    Shape,
    Place,
    Erase,
    Move {
        id: i64,
        original: Box<PdfDrawing>,
        last_source: (f32, f32),
    },
}

#[derive(Debug, Clone)]
pub struct PdfDrawingDragState {
    pub slot: PageSlot,
    pub start_x: f64,
    pub start_y: f64,
    pub kind: PdfDrawingDragKind,
}

/// A display point resolved against a page: its physical page, the
/// page's stable identity, the source-space point, and the display
/// size it was resolved against.
type SourcePointHit = (usize, String, (f32, f32), (f64, f64));

pub fn source_point_to_display(
    point: (f32, f32),
    source_width: f32,
    source_height: f32,
    display_width: f64,
    display_height: f64,
    rotation: i32,
) -> (f64, f64) {
    if source_width <= 0.0 || source_height <= 0.0 {
        return (0.0, 0.0);
    }
    let (x, y) = source_fraction_to_display(
        point.0 as f64 / source_width as f64,
        point.1 as f64 / source_height as f64,
        rotation,
    );
    (x * display_width, y * display_height)
}

fn display_scale(
    source_width: f32,
    source_height: f32,
    display_width: f64,
    display_height: f64,
    rotation: i32,
) -> f64 {
    let (shown_width, shown_height) = match normalize_pdf_rotation(rotation) {
        90 | 270 => (source_height as f64, source_width as f64),
        _ => (source_width as f64, source_height as f64),
    };
    if shown_width <= 0.0 || shown_height <= 0.0 {
        1.0
    } else {
        (display_width / shown_width).min(display_height / shown_height)
    }
}

/// `(x, y, width, height)` around an object, in source points,
/// including half its stroke width on every side.
pub fn drawing_bounds(geometry: &PdfDrawingGeometry, width_pts: f64) -> (f32, f32, f32, f32) {
    let pad = (width_pts.max(0.0) / 2.0) as f32;
    let (mut min_x, mut min_y, mut max_x, mut max_y) = match geometry {
        PdfDrawingGeometry::Stroke { points, .. } => {
            let mut bounds = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
            for &(x, y) in points {
                bounds.0 = bounds.0.min(x);
                bounds.1 = bounds.1.min(y);
                bounds.2 = bounds.2.max(x);
                bounds.3 = bounds.3.max(y);
            }
            if points.is_empty() {
                (0.0, 0.0, 0.0, 0.0)
            } else {
                bounds
            }
        }
        PdfDrawingGeometry::Shape { start, end, .. } => (
            start.0.min(end.0),
            start.1.min(end.1),
            start.0.max(end.0),
            start.1.max(end.1),
        ),
        PdfDrawingGeometry::Text { rect, .. } => (rect.0, rect.1, rect.0 + rect.2, rect.1 + rect.3),
        PdfDrawingGeometry::Sticky { point, .. } => (
            point.0 - 11.0,
            point.1 - 11.0,
            point.0 + 11.0,
            point.1 + 11.0,
        ),
    };
    min_x -= pad;
    min_y -= pad;
    max_x += pad;
    max_y += pad;
    (min_x, min_y, (max_x - min_x).max(1.0), (max_y - min_y).max(1.0))
}

fn point_segment_distance(point: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let ab = (b.0 - a.0, b.1 - a.1);
    let length_sq = ab.0 * ab.0 + ab.1 * ab.1;
    let t = if length_sq <= f32::EPSILON {
        0.0
    } else {
        (((point.0 - a.0) * ab.0 + (point.1 - a.1) * ab.1) / length_sq).clamp(0.0, 1.0)
    };
    let nearest = (a.0 + ab.0 * t, a.1 + ab.1 * t);
    ((point.0 - nearest.0).powi(2) + (point.1 - nearest.1).powi(2)).sqrt()
}

/// Whether a source-space point touches an object. `tolerance_pts`
/// is the pointer's forgiveness, converted from display pixels by
/// the caller; a stroke's own width always counts.
pub fn drawing_hit(
    geometry: &PdfDrawingGeometry,
    width_pts: f64,
    point: (f32, f32),
    tolerance_pts: f32,
) -> bool {
    let reach = (width_pts.max(0.0) / 2.0) as f32 + tolerance_pts.max(0.0);
    match geometry {
        PdfDrawingGeometry::Stroke { points, .. } => {
            if points.is_empty() {
                false
            } else if points.len() == 1 {
                point_segment_distance(point, points[0], points[0]) <= reach
            } else {
                points
                    .windows(2)
                    .any(|pair| point_segment_distance(point, pair[0], pair[1]) <= reach)
            }
        }
        PdfDrawingGeometry::Shape {
            shape,
            start,
            end,
            filled,
        } => {
            let (x, y, w, h) = drawing_bounds(geometry, 0.0);
            match shape.as_str() {
                "line" | "arrow" => point_segment_distance(point, *start, *end) <= reach,
                "ellipse" => {
                    let (cx, cy) = (x + w / 2.0, y + h / 2.0);
                    let (rx, ry) = ((w / 2.0).max(0.1), (h / 2.0).max(0.1));
                    let norm =
                        ((point.0 - cx) / rx).powi(2) + ((point.1 - cy) / ry).powi(2);
                    if *filled {
                        norm <= 1.0
                    } else {
                        (norm.sqrt() - 1.0).abs() * rx.min(ry) <= reach
                    }
                }
                _ if *filled => {
                    point.0 >= x - tolerance_pts
                        && point.0 <= x + w + tolerance_pts
                        && point.1 >= y - tolerance_pts
                        && point.1 <= y + h + tolerance_pts
                }
                _ => {
                    let corners = [
                        ((x, y), (x + w, y)),
                        ((x + w, y), (x + w, y + h)),
                        ((x + w, y + h), (x, y + h)),
                        ((x, y + h), (x, y)),
                    ];
                    corners
                        .iter()
                        .any(|(a, b)| point_segment_distance(point, *a, *b) <= reach)
                }
            }
        }
        PdfDrawingGeometry::Text { rect, .. } => {
            point.0 >= rect.0 - tolerance_pts
                && point.0 <= rect.0 + rect.2 + tolerance_pts
                && point.1 >= rect.1 - tolerance_pts
                && point.1 <= rect.1 + rect.3 + tolerance_pts
        }
        PdfDrawingGeometry::Sticky { point: pin, .. } => {
            point_segment_distance(point, *pin, *pin) <= reach.max(10.0)
        }
    }
}

/// Everything one stroke needs to reach Cairo: its source-space
/// points, the page and display they map between, and its ink.
struct StrokePaint<'a> {
    points: &'a [(f32, f32)],
    source_width: f32,
    source_height: f32,
    display_width: f64,
    display_height: f64,
    rotation: i32,
    color: PdfInkColor,
    opacity: f64,
    width_pts: f64,
}

fn paint_stroke_path(cr: &cairo::Context, paint: StrokePaint<'_>) {
    if paint.points.is_empty() {
        return;
    }
    let scale = display_scale(
        paint.source_width,
        paint.source_height,
        paint.display_width,
        paint.display_height,
        paint.rotation,
    );
    let line_width = (paint.width_pts.max(0.1) * scale).max(1.0);
    let (r, g, b) = paint.color.rgb();
    cr.set_source_rgba(r, g, b, paint.opacity.clamp(0.0, 1.0));
    cr.set_line_width(line_width);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    let mapped: Vec<(f64, f64)> = paint
        .points
        .iter()
        .map(|point| {
            source_point_to_display(
                *point,
                paint.source_width,
                paint.source_height,
                paint.display_width,
                paint.display_height,
                paint.rotation,
            )
        })
        .collect();
    if mapped.len() == 1 {
        cr.arc(mapped[0].0, mapped[0].1, line_width / 2.0, 0.0, std::f64::consts::TAU);
        let _ = cr.fill();
        return;
    }
    cr.move_to(mapped[0].0, mapped[0].1);
    for point in &mapped[1..] {
        cr.line_to(point.0, point.1);
    }
    let _ = cr.stroke();
}

struct ShapePaint {
    kind: PdfShapeKind,
    start: (f32, f32),
    end: (f32, f32),
    filled: bool,
    source_width: f32,
    source_height: f32,
    display_width: f64,
    display_height: f64,
    rotation: i32,
    color: PdfInkColor,
    opacity: f64,
    width_pts: f64,
}

fn paint_shape_path(cr: &cairo::Context, paint: ShapePaint) {
    let scale = display_scale(
        paint.source_width,
        paint.source_height,
        paint.display_width,
        paint.display_height,
        paint.rotation,
    );
    let line_width = (paint.width_pts.max(0.1) * scale).max(1.0);
    let a = source_point_to_display(
        paint.start,
        paint.source_width,
        paint.source_height,
        paint.display_width,
        paint.display_height,
        paint.rotation,
    );
    let b = source_point_to_display(
        paint.end,
        paint.source_width,
        paint.source_height,
        paint.display_width,
        paint.display_height,
        paint.rotation,
    );
    let (r, g, blue) = paint.color.rgb();
    // A filled shape carries its translucency in the fill alone:
    // the board's area highlight is a pale wash inside a line drawn
    // at full strength.
    let stroke_alpha = if paint.filled && paint.kind.can_fill() {
        1.0
    } else {
        paint.opacity.clamp(0.0, 1.0)
    };
    cr.set_line_width(line_width);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    match paint.kind {
        PdfShapeKind::Rectangle => {
            cr.rectangle(
                a.0.min(b.0),
                a.1.min(b.1),
                (a.0 - b.0).abs(),
                (a.1 - b.1).abs(),
            );
            if paint.filled {
                cr.set_source_rgba(r, g, blue, paint.opacity.clamp(0.0, 1.0));
                let _ = cr.fill_preserve();
            }
            cr.set_source_rgba(r, g, blue, stroke_alpha);
            let _ = cr.stroke();
        }
        PdfShapeKind::Ellipse => {
            let center = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            let radii = ((a.0 - b.0).abs() / 2.0, (a.1 - b.1).abs() / 2.0);
            if radii.0 <= 0.5 || radii.1 <= 0.5 {
                return;
            }
            let _ = cr.save();
            cr.translate(center.0, center.1);
            cr.scale(radii.0, radii.1);
            cr.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
            let _ = cr.restore();
            if paint.filled {
                cr.set_source_rgba(r, g, blue, paint.opacity.clamp(0.0, 1.0));
                let _ = cr.fill_preserve();
            }
            cr.set_source_rgba(r, g, blue, stroke_alpha);
            let _ = cr.stroke();
        }
        PdfShapeKind::Line => {
            cr.set_source_rgba(r, g, blue, stroke_alpha);
            cr.move_to(a.0, a.1);
            cr.line_to(b.0, b.1);
            let _ = cr.stroke();
        }
        PdfShapeKind::Arrow => {
            cr.set_source_rgba(r, g, blue, stroke_alpha);
            cr.move_to(a.0, a.1);
            cr.line_to(b.0, b.1);
            let _ = cr.stroke();
            let length = (b.0 - a.0).hypot(b.1 - a.1);
            if length > 2.0 {
                let head = (line_width * 4.0).clamp(9.0, length * 0.4);
                let angle = (b.1 - a.1).atan2(b.0 - a.0);
                let spread = 0.46;
                cr.move_to(b.0, b.1);
                cr.line_to(
                    b.0 - head * (angle - spread).cos(),
                    b.1 - head * (angle - spread).sin(),
                );
                cr.line_to(
                    b.0 - head * 0.72 * angle.cos(),
                    b.1 - head * 0.72 * angle.sin(),
                );
                cr.line_to(
                    b.0 - head * (angle + spread).cos(),
                    b.1 - head * (angle + spread).sin(),
                );
                cr.close_path();
                let _ = cr.fill();
            }
        }
    }
}

fn wrap_cairo_text(cr: &cairo::Context, text: &str, max_width: f64) -> Vec<String> {
    let mut lines = Vec::new();
    for paragraph in text.split('\n') {
        if paragraph.trim().is_empty() {
            lines.push(String::new());
            continue;
        }
        let mut line = String::new();
        for word in paragraph.split_whitespace() {
            let candidate = if line.is_empty() {
                word.to_string()
            } else {
                format!("{line} {word}")
            };
            let fits = cr
                .text_extents(&candidate)
                .map(|extents| extents.width() <= max_width)
                .unwrap_or(true);
            if fits || line.is_empty() {
                if !fits && line.is_empty() {
                    // One word wider than the box: break it by
                    // characters rather than painting past the edge.
                    let mut chunk = String::new();
                    for ch in word.chars() {
                        let next = format!("{chunk}{ch}");
                        let next_fits = cr
                            .text_extents(&next)
                            .map(|extents| extents.width() <= max_width)
                            .unwrap_or(true);
                        if next_fits || chunk.is_empty() {
                            chunk = next;
                        } else {
                            lines.push(std::mem::take(&mut chunk));
                            chunk.push(ch);
                        }
                    }
                    line = chunk;
                } else {
                    line = candidate;
                }
            } else {
                lines.push(std::mem::take(&mut line));
                line = word.to_string();
            }
        }
        lines.push(line);
    }
    lines
}

fn paint_text_box(
    cr: &cairo::Context,
    paint: &PdfDrawingPaint,
    display_width: f64,
    display_height: f64,
) {
    let PdfDrawingGeometry::Text {
        rect,
        text,
        size,
        paper_background,
    } = &paint.drawing.geometry
    else {
        return;
    };
    let (rect, text, size, paper_background) =
        (*rect, text.as_str(), size.as_str(), *paper_background);
    if text.trim().is_empty() || rect.2 <= 0.0 || rect.3 <= 0.0 {
        return;
    }
    let scale = display_scale(
        paint.source_width,
        paint.source_height,
        display_width,
        display_height,
        paint.rotation,
    );
    let origin = source_point_to_display(
        (rect.0, rect.1),
        paint.source_width,
        paint.source_height,
        display_width,
        display_height,
        paint.rotation,
    );
    let width = rect.2 as f64 * scale;
    let height = rect.3 as f64 * scale;
    let _ = cr.save();
    cr.translate(origin.0, origin.1);
    cr.rotate(match normalize_pdf_rotation(paint.rotation) {
        90 => std::f64::consts::FRAC_PI_2,
        180 => std::f64::consts::PI,
        270 => -std::f64::consts::FRAC_PI_2,
        _ => 0.0,
    });
    cr.rectangle(0.0, 0.0, width, height);
    cr.clip();
    if paper_background {
        let [r, g, b] = paint.paper_rgb;
        cr.set_source_rgba(r as f64, g as f64, b as f64, 1.0);
        cr.rectangle(0.0, 0.0, width, height);
        let _ = cr.fill();
    }
    let font_size = PdfTextSize::from_name(size).pts() as f64 * scale;
    cr.select_font_face(
        "Sans",
        cairo::FontSlant::Normal,
        cairo::FontWeight::Normal,
    );
    cr.set_font_size(font_size.max(1.0));
    let (r, g, b) = PdfInkColor::from_name(&paint.drawing.color).rgb();
    cr.set_source_rgba(r, g, b, paint.drawing.opacity.clamp(0.0, 1.0));
    let pad = 7.0 * scale;
    let lines = wrap_cairo_text(cr, text, (width - pad * 2.0).max(1.0));
    let ascent = cr
        .font_extents()
        .map(|extents| extents.ascent())
        .unwrap_or(font_size * 0.8);
    let mut baseline = pad + ascent;
    for line in lines {
        if baseline > height - 2.0 {
            break;
        }
        cr.move_to(pad, baseline);
        let _ = cr.show_text(&line);
        baseline += font_size * 1.35;
    }
    let _ = cr.restore();
}

fn rounded_rect_path(cr: &cairo::Context, x: f64, y: f64, w: f64, h: f64, radius: f64) {
    let radius = radius.min(w / 2.0).min(h / 2.0).max(0.0);
    cr.move_to(x + radius, y);
    cr.line_to(x + w - radius, y);
    cr.arc(x + w - radius, y + radius, radius, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.line_to(x + w, y + h - radius);
    cr.arc(x + w - radius, y + h - radius, radius, 0.0, std::f64::consts::FRAC_PI_2);
    cr.line_to(x + radius, y + h);
    cr.arc(x + radius, y + h - radius, radius, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    cr.line_to(x, y + radius);
    cr.arc(x + radius, y + radius, radius, std::f64::consts::PI, std::f64::consts::PI * 1.5);
    cr.close_path();
}

fn paint_sticky_pin(
    cr: &cairo::Context,
    paint: &PdfDrawingPaint,
    display_width: f64,
    display_height: f64,
    point: (f32, f32),
) {
    let scale = display_scale(
        paint.source_width,
        paint.source_height,
        display_width,
        display_height,
        paint.rotation,
    );
    let center = source_point_to_display(
        point,
        paint.source_width,
        paint.source_height,
        display_width,
        display_height,
        paint.rotation,
    );
    let radius = (10.0 * scale).max(8.0);
    cr.set_source_rgba(0.0, 0.0, 0.0, 0.16);
    cr.arc(center.0, center.1 + 1.0, radius, 0.0, std::f64::consts::TAU);
    let _ = cr.fill();
    let (r, g, b) = PdfInkColor::from_name(&paint.drawing.color).rgb();
    cr.set_source_rgba(r, g, b, paint.drawing.opacity.clamp(0.0, 1.0));
    cr.arc(center.0, center.1, radius, 0.0, std::f64::consts::TAU);
    let _ = cr.fill();
    // The little dark speech bubble of the board, drawn in the
    // fixed ink so it reads on every pin colour.
    let (ir, ig, ib) = PdfInkColor::Ink.rgb();
    cr.set_source_rgba(ir, ig, ib, 1.0);
    let unit = radius / 10.0;
    rounded_rect_path(
        cr,
        center.0 - 5.2 * unit,
        center.1 - 4.0 * unit,
        10.4 * unit,
        7.4 * unit,
        2.0 * unit,
    );
    let _ = cr.fill();
    cr.move_to(center.0 - 3.2 * unit, center.1 + 3.0 * unit);
    cr.line_to(center.0 - 0.8 * unit, center.1 + 3.0 * unit);
    cr.line_to(center.0 - 3.2 * unit, center.1 + 5.6 * unit);
    cr.close_path();
    let _ = cr.fill();
}

/// Paint one saved object, of whichever geometry kind it carries.
pub fn paint_drawing(
    cr: &cairo::Context,
    paint: &PdfDrawingPaint,
    display_width: f64,
    display_height: f64,
    accent_rgb: [f32; 3],
) {
    match &paint.drawing.geometry {
        PdfDrawingGeometry::Stroke { points, .. } => {
            paint_stroke_path(
                cr,
                StrokePaint {
                    points,
                    source_width: paint.source_width,
                    source_height: paint.source_height,
                    display_width,
                    display_height,
                    rotation: paint.rotation,
                    color: PdfInkColor::from_name(&paint.drawing.color),
                    opacity: paint.drawing.opacity,
                    width_pts: paint.drawing.width_pts,
                },
            );
        }
        PdfDrawingGeometry::Shape {
            shape,
            start,
            end,
            filled,
        } => {
            paint_shape_path(
                cr,
                ShapePaint {
                    kind: PdfShapeKind::from_name(shape),
                    start: *start,
                    end: *end,
                    filled: *filled,
                    source_width: paint.source_width,
                    source_height: paint.source_height,
                    display_width,
                    display_height,
                    rotation: paint.rotation,
                    color: PdfInkColor::from_name(&paint.drawing.color),
                    opacity: paint.drawing.opacity,
                    width_pts: paint.drawing.width_pts,
                },
            );
        }
        PdfDrawingGeometry::Text { .. } => {
            paint_text_box(cr, paint, display_width, display_height);
        }
        PdfDrawingGeometry::Sticky { point, .. } => {
            paint_sticky_pin(cr, paint, display_width, display_height, *point);
        }
    }
    if paint.selected {
        let (x, y, w, h) = drawing_bounds(&paint.drawing.geometry, paint.drawing.width_pts + 6.0);
        let a = source_point_to_display(
            (x, y),
            paint.source_width,
            paint.source_height,
            display_width,
            display_height,
            paint.rotation,
        );
        let b = source_point_to_display(
            (x + w, y + h),
            paint.source_width,
            paint.source_height,
            display_width,
            display_height,
            paint.rotation,
        );
        let [r, g, blue] = accent_rgb;
        cr.set_source_rgba(r as f64, g as f64, blue as f64, 0.95);
        cr.set_line_width(1.5);
        cr.set_dash(&[5.0, 4.0], 0.0);
        cr.rectangle(
            a.0.min(b.0),
            a.1.min(b.1),
            (a.0.max(b.0) - a.0.min(b.0)).max(1.0),
            (a.1.max(b.1) - a.1.min(b.1)).max(1.0),
        );
        let _ = cr.stroke();
        cr.set_dash(&[], 0.0);
    }
}

pub fn paint_active_stroke(
    cr: &cairo::Context,
    stroke: &PdfActiveStroke,
    display_width: f64,
    display_height: f64,
) {
    paint_stroke_path(
        cr,
        StrokePaint {
            points: &stroke.points,
            source_width: stroke.source_width,
            source_height: stroke.source_height,
            display_width,
            display_height,
            rotation: stroke.rotation,
            color: stroke.color,
            opacity: stroke.opacity,
            width_pts: stroke.width_pts,
        },
    );
}

pub fn paint_active_shape(
    cr: &cairo::Context,
    shape: &PdfActiveShape,
    display_width: f64,
    display_height: f64,
) {
    paint_shape_path(
        cr,
        ShapePaint {
            kind: shape.kind,
            start: shape.start,
            end: shape.end,
            filled: shape.filled,
            source_width: shape.source_width,
            source_height: shape.source_height,
            display_width,
            display_height,
            rotation: shape.rotation,
            color: shape.color,
            opacity: shape.opacity,
            width_pts: shape.width_pts,
        },
    );
}

fn set_button_active(button: &gtk::Button, active: bool) {
    if active {
        button.add_css_class("active");
    } else {
        button.remove_css_class("active");
    }
}

fn color_dot_button(color: PdfInkColor, tooltip: &str) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("kalam-pdf-draw-color");
    button.set_size_request(32, 32);
    button.set_valign(gtk::Align::Center);
    button.set_tooltip_text(Some(tooltip));
    button.update_property(&[gtk::accessible::Property::Label(tooltip)]);
    let dot = gtk::DrawingArea::new();
    dot.set_size_request(18, 18);
    dot.set_halign(gtk::Align::Center);
    dot.set_valign(gtk::Align::Center);
    dot.set_can_target(false);
    let (r, g, b) = color.rgb();
    dot.set_draw_func(move |_, cr, width, height| {
        cr.set_source_rgba(r, g, b, 1.0);
        cr.arc(
            width as f64 / 2.0,
            height as f64 / 2.0,
            (width.min(height) as f64 / 2.0).max(1.0),
            0.0,
            std::f64::consts::TAU,
        );
        let _ = cr.fill();
    });
    button.set_child(Some(&dot));
    button
}

fn shape_kind_button(kind: PdfShapeKind) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("kalam-pdf-draw-option");
    button.add_css_class("kalam-pdf-draw-shape-kind");
    button.set_size_request(38, 32);
    button.set_child(Some(&crate::icons::symbolic_with_classes(
        kind.icon_name(),
        18,
        &["kalam-inline-icon"],
    )));
    button.set_tooltip_text(Some(kind.label()));
    button.update_property(&[gtk::accessible::Property::Label(kind.label())]);
    button
}

fn shape_width_button(width: PdfStrokeWidth) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("kalam-pdf-draw-option");
    button.add_css_class("kalam-pdf-draw-shape-width");
    button.set_size_request(52, 32);
    button.set_tooltip_text(Some(&format!("{} line", width.label())));
    button.update_property(&[gtk::accessible::Property::Label(width.label())]);
    let sample = gtk::DrawingArea::new();
    sample.set_size_request(28, 12);
    sample.set_halign(gtk::Align::Center);
    sample.set_valign(gtk::Align::Center);
    sample.set_can_target(false);
    let pts = width.pts();
    sample.set_draw_func(move |_, cr, width, height| {
        cr.set_source_rgba(0.5, 0.5, 0.5, 1.0);
        cr.set_line_width(pts.max(1.0));
        cr.set_line_cap(cairo::LineCap::Round);
        cr.move_to(3.0, height as f64 / 2.0);
        cr.line_to(width as f64 - 3.0, height as f64 / 2.0);
        let _ = cr.stroke();
    });
    button.set_child(Some(&sample));
    button
}

pub struct PdfDrawingWidgets {
    pub root: gtk::Box,
    pub tool_buttons: Vec<(PdfDrawTool, gtk::Button)>,
    pub highlighter_options: gtk::Box,
    pub pen_options: gtk::Box,
    pub shape_options: gtk::Box,
    pub text_options: gtk::Box,
    pub highlighter_color_buttons: Vec<(PdfInkColor, gtk::Button)>,
    pub highlighter_opacity_buttons: Vec<(u8, gtk::Button)>,
    pub pen_color_buttons: Vec<(PdfInkColor, gtk::Button)>,
    pub pen_width_buttons: Vec<(PdfStrokeWidth, gtk::Button)>,
    pub shape_kind_buttons: Vec<(PdfShapeKind, gtk::Button)>,
    pub shape_fill_buttons: Vec<(bool, gtk::Button)>,
    pub shape_color_buttons: Vec<(PdfInkColor, gtk::Button)>,
    pub shape_width_buttons: Vec<(PdfStrokeWidth, gtk::Button)>,
    pub text_size_buttons: Vec<(PdfTextSize, gtk::Button)>,
    pub text_color_buttons: Vec<(PdfInkColor, gtk::Button)>,
    pub text_paper_switch: gtk::Switch,
}

fn option_card() -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("kalam-pdf-draw-options");
    card.set_size_request(252, -1);
    card.set_halign(gtk::Align::Center);
    card
}

pub fn build_drawing_toolbar(sender: &ComponentSender<PdfReaderModel>) -> PdfDrawingWidgets {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    root.set_halign(gtk::Align::Center);

    // Highlighter options: colour, then the three opacities.
    let highlighter_options = option_card();
    let highlighter_colors = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    highlighter_colors.set_halign(gtk::Align::Center);
    let mut highlighter_color_buttons = Vec::new();
    for color in PdfInkColor::HIGHLIGHTER {
        let button = color_dot_button(*color, &format!("Highlighter {}", color.label()));
        let tx = sender.input_sender().clone();
        let chosen = *color;
        button.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::SetHighlighterColor(chosen));
        });
        highlighter_colors.append(&button);
        highlighter_color_buttons.push((*color, button));
    }
    highlighter_options.append(&highlighter_colors);
    let opacities = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    opacities.set_halign(gtk::Align::Center);
    let mut highlighter_opacity_buttons = Vec::new();
    for percent in [40u8, 60, 80] {
        let button = gtk::Button::with_label(&format!("{percent}%"));
        button.add_css_class("kalam-pdf-draw-option");
        button.set_tooltip_text(Some(&format!("{percent}% opacity")));
        let tx = sender.input_sender().clone();
        button.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::SetHighlighterOpacity(percent));
        });
        opacities.append(&button);
        highlighter_opacity_buttons.push((percent, button));
    }
    highlighter_options.append(&opacities);
    root.append(&highlighter_options);

    // Pen options: colour (with ink), then the three thicknesses.
    let pen_options = option_card();
    let pen_colors = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    pen_colors.set_halign(gtk::Align::Center);
    let mut pen_color_buttons = Vec::new();
    for color in PdfInkColor::ALL {
        let button = color_dot_button(*color, &format!("Pen {}", color.label()));
        let tx = sender.input_sender().clone();
        let chosen = *color;
        button.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::SetPenColor(chosen));
        });
        pen_colors.append(&button);
        pen_color_buttons.push((*color, button));
    }
    pen_options.append(&pen_colors);
    let widths = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    widths.set_halign(gtk::Align::Center);
    let mut pen_width_buttons = Vec::new();
    for width in PdfStrokeWidth::ALL {
        let button = gtk::Button::with_label(width.label());
        button.add_css_class("kalam-pdf-draw-option");
        button.set_tooltip_text(Some(&format!("{} pen", width.label())));
        let tx = sender.input_sender().clone();
        let chosen = *width;
        button.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::SetPenWidth(chosen));
        });
        widths.append(&button);
        pen_width_buttons.push((*width, button));
    }
    pen_options.append(&widths);
    root.append(&pen_options);

    // Shape options: the four outlines, outline-or-filled, colour,
    // then the three line weights as drawn samples.
    let shape_options = option_card();
    let shape_kinds = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    shape_kinds.set_halign(gtk::Align::Center);
    let mut shape_kind_buttons = Vec::new();
    for kind in PdfShapeKind::ALL {
        let button = shape_kind_button(*kind);
        let tx = sender.input_sender().clone();
        let chosen = *kind;
        button.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::SetShapeKind(chosen));
        });
        shape_kinds.append(&button);
        shape_kind_buttons.push((*kind, button));
    }
    shape_options.append(&shape_kinds);
    let shape_fills = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    shape_fills.set_halign(gtk::Align::Center);
    let mut shape_fill_buttons = Vec::new();
    for (filled, label) in [(false, "Outline"), (true, "Filled")] {
        let button = gtk::Button::with_label(label);
        button.add_css_class("kalam-pdf-draw-option");
        button.set_tooltip_text(Some(label));
        let tx = sender.input_sender().clone();
        button.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::SetShapeFilled(filled));
        });
        shape_fills.append(&button);
        shape_fill_buttons.push((filled, button));
    }
    shape_options.append(&shape_fills);
    let shape_colors = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    shape_colors.set_halign(gtk::Align::Center);
    let mut shape_color_buttons = Vec::new();
    for color in PdfInkColor::ALL {
        let button = color_dot_button(*color, &format!("Shape {}", color.label()));
        let tx = sender.input_sender().clone();
        let chosen = *color;
        button.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::SetShapeColor(chosen));
        });
        shape_colors.append(&button);
        shape_color_buttons.push((*color, button));
    }
    shape_options.append(&shape_colors);
    let shape_widths = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    shape_widths.set_halign(gtk::Align::Center);
    let mut shape_width_buttons = Vec::new();
    for width in PdfStrokeWidth::ALL {
        let button = shape_width_button(*width);
        let tx = sender.input_sender().clone();
        let chosen = *width;
        button.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::SetShapeWidth(chosen));
        });
        shape_widths.append(&button);
        shape_width_buttons.push((*width, button));
    }
    shape_options.append(&shape_widths);
    root.append(&shape_options);

    // Text-box options: S/M/L, colour, and the paper ground.
    let text_options = option_card();
    let text_sizes = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    text_sizes.set_halign(gtk::Align::Center);
    let mut text_size_buttons = Vec::new();
    for size in PdfTextSize::ALL {
        let button = gtk::Button::with_label(size.label());
        button.add_css_class("kalam-pdf-draw-option");
        button.set_tooltip_text(Some(match size {
            PdfTextSize::Small => "Small text",
            PdfTextSize::Medium => "Medium text",
            PdfTextSize::Large => "Large text",
        }));
        let tx = sender.input_sender().clone();
        let chosen = *size;
        button.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::SetTextSize(chosen));
        });
        text_sizes.append(&button);
        text_size_buttons.push((*size, button));
    }
    text_options.append(&text_sizes);
    let text_colors = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    text_colors.set_halign(gtk::Align::Center);
    let mut text_color_buttons = Vec::new();
    for color in PdfInkColor::ALL {
        let button = color_dot_button(*color, &format!("Text {}", color.label()));
        let tx = sender.input_sender().clone();
        let chosen = *color;
        button.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::SetTextColor(chosen));
        });
        text_colors.append(&button);
        text_color_buttons.push((*color, button));
    }
    text_options.append(&text_colors);
    let paper_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let paper_label = gtk::Label::new(Some("Paper background"));
    paper_label.add_css_class("kalam-pdf-draw-switch-label");
    paper_label.set_halign(gtk::Align::Start);
    paper_label.set_hexpand(true);
    paper_row.append(&paper_label);
    let text_paper_switch = gtk::Switch::new();
    text_paper_switch.set_valign(gtk::Align::Center);
    text_paper_switch.update_property(&[gtk::accessible::Property::Label("Paper background")]);
    {
        let tx = sender.input_sender().clone();
        text_paper_switch.connect_active_notify(move |switch| {
            let _ = tx.send(PdfReaderMsg::SetTextPaperBackground(switch.is_active()));
        });
    }
    paper_row.append(&text_paper_switch);
    text_options.append(&paper_row);
    root.append(&text_options);

    // The toolbar pill itself.
    let toolbar = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    toolbar.add_css_class("kalam-pdf-draw-toolbar");
    toolbar.set_halign(gtk::Align::Center);
    let mut tool_buttons = Vec::new();
    for tool in PdfDrawTool::ALL {
        let button = gtk::Button::new();
        button.add_css_class("kalam-pdf-draw-tool");
        button.set_size_request(36, 36);
        button.set_valign(gtk::Align::Center);
        button.set_child(Some(&crate::icons::symbolic_with_classes(
            tool.icon_name(),
            18,
            &["kalam-inline-icon"],
        )));
        button.set_tooltip_text(Some(tool.label()));
        button.update_property(&[gtk::accessible::Property::Label(tool.label())]);
        let tx = sender.input_sender().clone();
        let chosen = *tool;
        button.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::SetDrawTool(chosen));
        });
        toolbar.append(&button);
        tool_buttons.push((*tool, button));
    }
    root.append(&toolbar);

    PdfDrawingWidgets {
        root,
        tool_buttons,
        highlighter_options,
        pen_options,
        shape_options,
        text_options,
        highlighter_color_buttons,
        highlighter_opacity_buttons,
        pen_color_buttons,
        pen_width_buttons,
        shape_kind_buttons,
        shape_fill_buttons,
        shape_color_buttons,
        shape_width_buttons,
        text_size_buttons,
        text_color_buttons,
        text_paper_switch,
    }
}

pub fn sync_drawing_toolbar(widgets: &PdfDrawingWidgets, model: &PdfReaderModel) {
    for (tool, button) in &widgets.tool_buttons {
        set_button_active(button, *tool == model.draw_tool);
    }
    widgets
        .highlighter_options
        .set_visible(model.draw_options_open && model.draw_tool == PdfDrawTool::Highlighter);
    widgets
        .pen_options
        .set_visible(model.draw_options_open && model.draw_tool == PdfDrawTool::Pen);
    widgets
        .shape_options
        .set_visible(model.draw_options_open && model.draw_tool == PdfDrawTool::Shape);
    widgets
        .text_options
        .set_visible(model.draw_options_open && model.draw_tool == PdfDrawTool::TextBox);
    for (color, button) in &widgets.highlighter_color_buttons {
        set_button_active(button, *color == model.draw_settings.highlighter_color);
    }
    for (percent, button) in &widgets.highlighter_opacity_buttons {
        set_button_active(
            button,
            *percent == model.draw_settings.highlighter_opacity_percent,
        );
    }
    for (color, button) in &widgets.pen_color_buttons {
        set_button_active(button, *color == model.draw_settings.pen_color);
    }
    for (width, button) in &widgets.pen_width_buttons {
        set_button_active(button, *width == model.draw_settings.pen_width);
    }
    for (kind, button) in &widgets.shape_kind_buttons {
        set_button_active(button, *kind == model.draw_settings.shape_kind);
    }
    for (filled, button) in &widgets.shape_fill_buttons {
        set_button_active(button, *filled == model.draw_settings.shape_filled);
        button.set_sensitive(model.draw_settings.shape_kind.can_fill());
    }
    for (color, button) in &widgets.shape_color_buttons {
        set_button_active(button, *color == model.draw_settings.shape_color);
    }
    for (width, button) in &widgets.shape_width_buttons {
        set_button_active(button, *width == model.draw_settings.shape_width);
    }
    for (size, button) in &widgets.text_size_buttons {
        set_button_active(button, *size == model.draw_settings.text_size);
    }
    for (color, button) in &widgets.text_color_buttons {
        set_button_active(button, *color == model.draw_settings.text_color);
    }
    if widgets.text_paper_switch.is_active() != model.draw_settings.text_paper_background {
        widgets
            .text_paper_switch
            .set_active(model.draw_settings.text_paper_background);
    }
}

fn build_sticky_card(
    drawing: &PdfDrawing,
    sender: &ComponentSender<PdfReaderModel>,
) -> (gtk::Box, PdfStickyCardState) {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("kalam-pdf-draw-popover");
    card.add_css_class("kalam-pdf-sticky-card");
    card.set_size_request(232, -1);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let current = PdfInkColor::from_name(&drawing.color);
    let mut dots: Vec<(PdfInkColor, gtk::Button)> = Vec::new();
    for color in PdfInkColor::HIGHLIGHTER {
        let button = color_dot_button(*color, &format!("Use {}", color.label()));
        set_button_active(&button, *color == current);
        header.append(&button);
        dots.push((*color, button));
    }
    let dots = std::rc::Rc::new(dots);
    for (color, button) in dots.iter() {
        let tx = sender.input_sender().clone();
        let all = dots.clone();
        let chosen = *color;
        button.connect_clicked(move |_| {
            for (other, dot) in all.iter() {
                set_button_active(dot, *other == chosen);
            }
            let _ = tx.send(PdfReaderMsg::SetSelectedDrawingColor(chosen));
        });
    }
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    header.append(&spacer);
    let delete = gtk::Button::new();
    delete.add_css_class("kalam-pdf-draw-sticky-delete");
    delete.set_child(Some(&crate::icons::symbolic_with_classes(
        "user-trash-symbolic",
        15,
        &["kalam-inline-icon"],
    )));
    delete.set_tooltip_text(Some("Delete this note (Ctrl+Z brings it back)"));
    delete.update_property(&[gtk::accessible::Property::Label("Delete")]);
    {
        let tx = sender.input_sender().clone();
        let id = drawing.id;
        delete.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::DeleteDrawing(id));
        });
    }
    header.append(&delete);
    card.append(&header);
    card.append(&gtk::Separator::new(gtk::Orientation::Horizontal));

    let text = match &drawing.geometry {
        PdfDrawingGeometry::Sticky { text, .. } => text.as_str(),
        _ => "",
    };
    let buffer = gtk::TextBuffer::new(None);
    buffer.set_text(text);
    buffer.set_enable_undo(true);
    let view = gtk::TextView::with_buffer(&buffer);
    view.add_css_class("kalam-pdf-sticky-editor");
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_accepts_tab(false);
    view.set_left_margin(2);
    view.set_right_margin(2);
    view.set_top_margin(2);
    view.set_bottom_margin(2);
    view.set_size_request(-1, 56);
    view.update_property(&[gtk::accessible::Property::Label("Note")]);
    card.append(&view);

    (
        card,
        PdfStickyCardState {
            id: drawing.id,
            buffer,
            view,
            original: Some(Box::new(drawing.clone())),
        },
    )
}

/// The small popover a selected drawing opens (decision 66): its
/// own options and a delete.
pub fn build_selected_drawing_popover(
    host: &gtk::Widget,
    anchor: gdk::Rectangle,
    position: gtk::PositionType,
    drawing: &PdfDrawing,
    sender: &ComponentSender<PdfReaderModel>,
) -> (gtk::Popover, Option<PdfStickyCardState>) {
    let popover = gtk::Popover::new();
    popover.set_parent(host);
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(position);
    popover.set_pointing_to(Some(&anchor));
    popover.add_css_class("kalam-pdf-draw-popover-shell");

    if matches!(drawing.geometry, PdfDrawingGeometry::Sticky { .. }) {
        let (card, state) = build_sticky_card(drawing, sender);
        popover.set_child(Some(&card));
        let tx = sender.input_sender().clone();
        popover.connect_closed(move |_| {
            let _ = tx.send(PdfReaderMsg::CloseDrawingPopover);
        });
        return (popover, Some(state));
    }

    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("kalam-pdf-draw-popover");
    card.set_size_request(252, -1);

    if let PdfDrawingGeometry::Shape {
        shape, filled, ..
    } = &drawing.geometry
    {
        let current_kind = PdfShapeKind::from_name(shape);
        let fills = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        fills.set_halign(gtk::Align::Center);
        let mut fill_buttons: Vec<(bool, gtk::Button)> = Vec::new();
        for (is_filled, label) in [(false, "Outline"), (true, "Filled")] {
            let button = gtk::Button::with_label(label);
            button.add_css_class("kalam-pdf-draw-option");
            set_button_active(&button, is_filled == *filled);
            button.set_sensitive(current_kind.can_fill());
            fills.append(&button);
            fill_buttons.push((is_filled, button));
        }
        let fill_buttons = std::rc::Rc::new(fill_buttons);
        for (is_filled, button) in fill_buttons.iter() {
            let tx = sender.input_sender().clone();
            let all = fill_buttons.clone();
            let chosen = *is_filled;
            button.connect_clicked(move |_| {
                for (other, fill) in all.iter() {
                    set_button_active(fill, *other == chosen);
                }
                let _ = tx.send(PdfReaderMsg::SetSelectedShapeFilled(chosen));
            });
        }

        let kinds = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        kinds.set_halign(gtk::Align::Center);
        let mut kind_buttons: Vec<(PdfShapeKind, gtk::Button)> = Vec::new();
        for kind in PdfShapeKind::ALL {
            let button = shape_kind_button(*kind);
            set_button_active(&button, *kind == current_kind);
            kinds.append(&button);
            kind_buttons.push((*kind, button));
        }
        let kind_buttons = std::rc::Rc::new(kind_buttons);
        for (kind, button) in kind_buttons.iter() {
            let tx = sender.input_sender().clone();
            let all = kind_buttons.clone();
            let fills = fill_buttons.clone();
            let chosen = *kind;
            button.connect_clicked(move |_| {
                for (other, kind_button) in all.iter() {
                    set_button_active(kind_button, *other == chosen);
                }
                for (_, fill) in fills.iter() {
                    fill.set_sensitive(chosen.can_fill());
                }
                let _ = tx.send(PdfReaderMsg::SetSelectedShapeKind(chosen));
            });
        }
        card.append(&kinds);
        card.append(&fills);
    }

    if let PdfDrawingGeometry::Text { size, .. } = &drawing.geometry {
        let current_size = PdfTextSize::from_name(size);
        let sizes = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        sizes.set_halign(gtk::Align::Center);
        let mut size_buttons: Vec<(PdfTextSize, gtk::Button)> = Vec::new();
        for text_size in PdfTextSize::ALL {
            let button = gtk::Button::with_label(text_size.label());
            button.add_css_class("kalam-pdf-draw-option");
            set_button_active(&button, *text_size == current_size);
            sizes.append(&button);
            size_buttons.push((*text_size, button));
        }
        let size_buttons = std::rc::Rc::new(size_buttons);
        for (text_size, button) in size_buttons.iter() {
            let tx = sender.input_sender().clone();
            let all = size_buttons.clone();
            let chosen = *text_size;
            button.connect_clicked(move |_| {
                for (other, size_button) in all.iter() {
                    set_button_active(size_button, *other == chosen);
                }
                let _ = tx.send(PdfReaderMsg::SetSelectedTextSize(chosen));
            });
        }
        card.append(&sizes);
    }

    let colors = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    colors.set_halign(gtk::Align::Center);
    let current = PdfInkColor::from_name(&drawing.color);
    let mut dots: Vec<(PdfInkColor, gtk::Button)> = Vec::new();
    let palette: &[PdfInkColor] = if drawing.geometry.kind() == "highlighter" {
        PdfInkColor::HIGHLIGHTER
    } else {
        PdfInkColor::ALL
    };
    for color in palette {
        let button = color_dot_button(*color, &format!("Use {}", color.label()));
        set_button_active(&button, *color == current);
        colors.append(&button);
        dots.push((*color, button));
    }
    let dots = std::rc::Rc::new(dots);
    for (color, button) in dots.iter() {
        let tx = sender.input_sender().clone();
        let all = dots.clone();
        let chosen = *color;
        button.connect_clicked(move |_| {
            for (other, dot) in all.iter() {
                set_button_active(dot, *other == chosen);
            }
            let _ = tx.send(PdfReaderMsg::SetSelectedDrawingColor(chosen));
        });
    }
    card.append(&colors);

    if matches!(
        drawing.geometry,
        PdfDrawingGeometry::Stroke { .. } | PdfDrawingGeometry::Shape { .. }
    ) {
        let widths = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        widths.set_halign(gtk::Align::Center);
        let is_highlighter = drawing.geometry.kind() == "highlighter";
        let choices: [(PdfStrokeWidth, f64); 3] = if is_highlighter {
            [
                (PdfStrokeWidth::Thin, 8.0),
                (PdfStrokeWidth::Medium, 14.0),
                (PdfStrokeWidth::Thick, 20.0),
            ]
        } else {
            [
                (PdfStrokeWidth::Thin, PdfStrokeWidth::Thin.pts()),
                (PdfStrokeWidth::Medium, PdfStrokeWidth::Medium.pts()),
                (PdfStrokeWidth::Thick, PdfStrokeWidth::Thick.pts()),
            ]
        };
        let current_index = if is_highlighter {
            if drawing.width_pts <= 11.0 {
                0
            } else if drawing.width_pts <= 17.0 {
                1
            } else {
                2
            }
        } else {
            match PdfStrokeWidth::from_pts(drawing.width_pts) {
                PdfStrokeWidth::Thin => 0,
                PdfStrokeWidth::Medium => 1,
                PdfStrokeWidth::Thick => 2,
            }
        };
        let mut width_buttons: Vec<gtk::Button> = Vec::new();
        for (index, (width, pts)) in choices.iter().enumerate() {
            let button = gtk::Button::with_label(width.label());
            button.add_css_class("kalam-pdf-draw-option");
            set_button_active(&button, index == current_index);
            widths.append(&button);
            width_buttons.push(button);
            let _ = pts;
        }
        let width_buttons = std::rc::Rc::new(width_buttons);
        for (index, (_, pts)) in choices.iter().enumerate() {
            let tx = sender.input_sender().clone();
            let all = width_buttons.clone();
            let chosen_pts = (*pts * 10.0).round() as i64;
            width_buttons[index].connect_clicked(move |_| {
                for (other, button) in all.iter().enumerate() {
                    set_button_active(button, other == index);
                }
                let _ = tx.send(PdfReaderMsg::SetSelectedDrawingWidthTenths(chosen_pts));
            });
        }
        card.append(&widths);
    }

    if let PdfDrawingGeometry::Text {
        paper_background, ..
    } = &drawing.geometry
    {
        let paper_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let paper_label = gtk::Label::new(Some("Paper background"));
        paper_label.add_css_class("kalam-pdf-draw-switch-label");
        paper_label.set_halign(gtk::Align::Start);
        paper_label.set_hexpand(true);
        paper_row.append(&paper_label);
        let paper_switch = gtk::Switch::new();
        paper_switch.set_active(*paper_background);
        paper_switch.set_valign(gtk::Align::Center);
        paper_switch.update_property(&[gtk::accessible::Property::Label("Paper background")]);
        {
            let tx = sender.input_sender().clone();
            paper_switch.connect_active_notify(move |switch| {
                let _ = tx.send(PdfReaderMsg::SetSelectedTextPaper(switch.is_active()));
            });
        }
        paper_row.append(&paper_switch);
        card.append(&paper_row);
    }

    let delete = gtk::Button::new();
    delete.add_css_class("kalam-pdf-draw-delete");
    delete.set_child(Some(&crate::icons::labelled(
        "user-trash-symbolic",
        14,
        "Delete",
        6,
    )));
    delete.set_halign(gtk::Align::Center);
    delete.set_tooltip_text(Some("Delete this drawing (Ctrl+Z brings it back)"));
    {
        let tx = sender.input_sender().clone();
        let id = drawing.id;
        delete.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::DeleteDrawing(id));
        });
    }
    card.append(&delete);

    popover.set_child(Some(&card));
    let tx = sender.input_sender().clone();
    popover.connect_closed(move |_| {
        let _ = tx.send(PdfReaderMsg::CloseDrawingPopover);
    });
    (popover, None)
}

impl PdfReaderModel {
    pub fn reload_drawings(&mut self) {
        self.page_identities = self
            .catalog
            .pdf_page_identities(self.book_id)
            .unwrap_or_default();
        self.drawings = self
            .catalog
            .pdf_drawings_for_book(self.book_id)
            .unwrap_or_default();
    }

    pub fn ensure_page_identities(&mut self) {
        match self
            .catalog
            .ensure_pdf_page_identities(self.book_id, self.total_pages)
        {
            Ok(pages) => self.page_identities = pages,
            Err(err) => log::warn!("PDF page identities unavailable: {err}"),
        }
        self.drawings = self
            .catalog
            .pdf_drawings_for_book(self.book_id)
            .unwrap_or_default();
    }

    pub fn page_id_for_position(&self, page: usize) -> Option<String> {
        self.page_identities
            .iter()
            .find(|identity| identity.position == page)
            .map(|identity| identity.page_id.clone())
    }

    fn drawing_target_dimensions(&self) -> (f64, f64) {
        if self.scroll_mode == PdfScrollMode::WrappedScrolling {
            (
                self.base_page_width * 0.6 * self.zoom_level,
                self.base_page_height * 0.6 * self.zoom_level,
            )
        } else {
            (
                self.base_page_width * self.zoom_level,
                self.base_page_height * self.zoom_level,
            )
        }
    }

    /// A display point as `(page, page identity, source point,
    /// target size)` — the one conversion every drawing gesture uses.
    fn source_point_for_display(
        &self,
        slot: PageSlot,
        x: f64,
        y: f64,
    ) -> Option<SourcePointHit> {
        let page = self.page_for_slot(slot)?;
        let page_id = self.page_id_for_position(page)?;
        let (target_w, target_h) = self.drawing_target_dimensions();
        if target_w <= 0.0 || target_h <= 0.0 {
            return None;
        }
        let point = display_point_to_source(
            x.clamp(0.0, target_w),
            y.clamp(0.0, target_h),
            target_w,
            target_h,
            self.source_page_width as f32,
            self.source_page_height as f32,
            self.rotation_degrees,
        );
        Some((page, page_id, point, (target_w, target_h)))
    }

    pub fn refresh_drawing_paints(&self) {
        let mut paints = Vec::new();
        let mut slots: Vec<PageSlot> = self.page_overlays.keys().copied().collect();
        slots.sort_by_key(|slot| format!("{slot:?}"));
        for slot in slots {
            let Some(page) = self.page_for_slot(slot) else {
                continue;
            };
            let Some(page_id) = self.page_id_for_position(page) else {
                continue;
            };
            for drawing in self.drawings.iter().filter(|d| d.page_id == page_id) {
                if self
                    .drawing_text_editor
                    .as_ref()
                    .and_then(|editor| editor.editing_id)
                    == Some(drawing.id)
                {
                    continue;
                }
                paints.push(PdfDrawingPaint {
                    slot,
                    drawing: drawing.clone(),
                    source_width: self.source_page_width as f32,
                    source_height: self.source_page_height as f32,
                    rotation: self.rotation_degrees,
                    selected: self.selected_drawing_id == Some(drawing.id),
                    paper_rgb: self.image_paper_rgb,
                });
            }
        }
        // Highlighters under pens, however they were interleaved.
        paints.sort_by_key(|paint| {
            (
                format!("{:?}", paint.slot),
                paint.drawing.geometry.kind() != "highlighter",
                paint.drawing.id,
            )
        });
        *self.drawing_paints.borrow_mut() = paints;
    }

    pub fn queue_drawing_slot(&self, slot: PageSlot) {
        if let Some(area) = self.page_draw_areas.get(&slot) {
            area.queue_draw();
        }
    }

    pub fn queue_all_drawing_areas(&self) {
        for area in self.page_draw_areas.values() {
            area.queue_draw();
        }
    }

    pub fn set_draw_tool(&mut self, tool: PdfDrawTool) {
        if self.drawing_text_editor.is_some() && tool != self.draw_tool {
            self.commit_drawing_text_editor();
        }
        let changed = self.draw_tool != tool;
        self.draw_tool = tool;
        self.draw_tool_shared.set(tool);
        self.draw_options_open = matches!(
            tool,
            PdfDrawTool::Highlighter
                | PdfDrawTool::Pen
                | PdfDrawTool::Shape
                | PdfDrawTool::TextBox
        );
        if !tool.is_select() {
            self.clear_selection();
            self.selected_drawing_id = None;
            self.dismiss_drawing_popover();
        }
        if changed {
            self.active_drawing.replace(None);
            self.active_shape.replace(None);
            if !matches!(
                self.drawing_drag.as_ref().map(|drag| &drag.kind),
                Some(PdfDrawingDragKind::Move { .. })
            ) {
                self.drawing_drag = None;
            }
        }
        let cursor = if tool.is_select() { "text" } else { "crosshair" };
        for overlay in self.page_overlays.values() {
            overlay.set_cursor_from_name(Some(cursor));
        }
        self.refresh_drawing_paints();
        self.queue_all_drawing_areas();
    }

    fn drawing_at(&self, slot: PageSlot, x: f64, y: f64) -> Option<PdfDrawing> {
        let (_, page_id, point, (target_w, target_h)) =
            self.source_point_for_display(slot, x, y)?;
        let tolerance = ((10.0 / target_w.max(1.0)) * self.source_page_width)
            .max((10.0 / target_h.max(1.0)) * self.source_page_height)
            as f32;
        self.drawings
            .iter()
            .rev()
            .find(|drawing| {
                drawing.page_id == page_id
                    && drawing_hit(&drawing.geometry, drawing.width_pts, point, tolerance)
            })
            .cloned()
    }

    fn display_rect_for_source_rect(
        &self,
        rect: (f32, f32, f32, f32),
    ) -> (i32, i32, i32, i32) {
        let (target_w, target_h) = self.drawing_target_dimensions();
        let a = source_point_to_display(
            (rect.0, rect.1),
            self.source_page_width as f32,
            self.source_page_height as f32,
            target_w,
            target_h,
            self.rotation_degrees,
        );
        let b = source_point_to_display(
            (rect.0 + rect.2, rect.1 + rect.3),
            self.source_page_width as f32,
            self.source_page_height as f32,
            target_w,
            target_h,
            self.rotation_degrees,
        );
        (
            a.0.min(b.0).round() as i32,
            a.1.min(b.1).round() as i32,
            (a.0.max(b.0) - a.0.min(b.0)).round().max(1.0) as i32,
            (a.1.max(b.1) - a.1.min(b.1)).round().max(1.0) as i32,
        )
    }

    fn clamp_text_rect(&self, rect: (f32, f32, f32, f32)) -> (f32, f32, f32, f32) {
        let page_w = (self.source_page_width as f32).max(1.0);
        let page_h = (self.source_page_height as f32).max(1.0);
        let width = rect.2.clamp(40.0, page_w);
        let height = rect.3.clamp(24.0, page_h);
        (
            rect.0.clamp(0.0, (page_w - width).max(0.0)),
            rect.1.clamp(0.0, (page_h - height).max(0.0)),
            width,
            height,
        )
    }

    pub fn text_rect_height(text: &str, size: PdfTextSize, width: f32, current: f32) -> f32 {
        let font = size.pts();
        let chars_per_line = (((width - 16.0) / (font * 0.52)).floor() as usize).max(1);
        let mut lines = 0usize;
        for paragraph in text.split('\n') {
            let count = paragraph.chars().count().max(1);
            lines += count.div_ceil(chars_per_line);
        }
        current.max(lines.max(1) as f32 * font * 1.35 + 16.0)
    }

    fn slot_for_drawing(&self, drawing: &PdfDrawing) -> Option<PageSlot> {
        let position = self
            .page_identities
            .iter()
            .find(|identity| identity.page_id == drawing.page_id)
            .map(|identity| identity.position)?;
        self.page_overlays
            .keys()
            .copied()
            .find(|slot| self.page_for_slot(*slot) == Some(position))
    }

    /// Write the sticky card's words back to its row. Called before
    /// the card closes and before any other change to the same row,
    /// so typing is never lost behind a colour click.
    pub fn commit_sticky_card_text(&mut self) {
        let Some((id, text)) = self.sticky_card.as_ref().map(|state| {
            (
                state.id,
                state
                    .buffer
                    .text(&state.buffer.start_iter(), &state.buffer.end_iter(), true)
                    .to_string(),
            )
        }) else {
            return;
        };
        let Some(current) = self
            .drawings
            .iter()
            .find(|drawing| drawing.id == id)
            .cloned()
        else {
            self.sticky_card = None;
            return;
        };
        let unchanged = matches!(
            &current.geometry,
            PdfDrawingGeometry::Sticky { text: saved, .. } if saved == &text
        );
        if unchanged {
            return;
        }
        let original = self
            .sticky_card
            .as_mut()
            .and_then(|state| state.original.take())
            .map(|drawing| *drawing)
            .unwrap_or_else(|| current.clone());
        let mut after = current;
        if let PdfDrawingGeometry::Sticky {
            text: saved, ..
        } = &mut after.geometry
        {
            *saved = text;
        }
        if crate::notify::report(
            self.catalog.update_pdf_drawing(&after),
            "Could not save the note",
        ) {
            self.remember_deleted(PdfDeletedMark::DrawingBefore(Box::new(original)));
        }
        self.reload_drawings();
        self.refresh_drawing_paints();
        self.queue_all_drawing_areas();
    }

    pub fn mutate_selected_drawing(
        &mut self,
        error: &str,
        change: impl FnOnce(&mut PdfDrawing),
    ) {
        self.commit_sticky_card_text();
        let Some(before) = self.selected_drawing_id.and_then(|id| {
            self.drawings
                .iter()
                .find(|drawing| drawing.id == id)
                .cloned()
        }) else {
            return;
        };
        let mut after = before.clone();
        change(&mut after);
        if after == before {
            return;
        }
        if crate::notify::report(self.catalog.update_pdf_drawing(&after), error) {
            self.remember_deleted(PdfDeletedMark::DrawingBefore(Box::new(before)));
        }
        self.reload_drawings();
        self.refresh_drawing_paints();
        self.queue_all_drawing_areas();
    }

    pub fn open_drawing_text_editor(
        &mut self,
        slot: PageSlot,
        existing: Option<PdfDrawing>,
        page_id: String,
        rect: (f32, f32, f32, f32),
        sender: &ComponentSender<Self>,
    ) {
        if self.drawing_text_editor.is_some() {
            self.commit_drawing_text_editor();
        }
        self.dismiss_drawing_popover();
        let Some(overlay) = self.page_overlays.get(&slot).cloned() else {
            return;
        };
        let rect = self.clamp_text_rect(rect);
        let (initial_text, size, color, paper_background, editing_id, original) =
            if let Some(drawing) = existing {
                let (text, size, paper) = match &drawing.geometry {
                    PdfDrawingGeometry::Text {
                        text,
                        size,
                        paper_background,
                        ..
                    } => (
                        text.clone(),
                        PdfTextSize::from_name(size),
                        *paper_background,
                    ),
                    _ => (String::new(), self.draw_settings.text_size, true),
                };
                (
                    text,
                    size,
                    PdfInkColor::from_name(&drawing.color),
                    paper,
                    Some(drawing.id),
                    Some(Box::new(drawing)),
                )
            } else {
                (
                    String::new(),
                    self.draw_settings.text_size,
                    self.draw_settings.text_color,
                    self.draw_settings.text_paper_background,
                    None,
                    None,
                )
            };
        let buffer = gtk::TextBuffer::new(None);
        buffer.set_text(&initial_text);
        buffer.set_enable_undo(true);
        let view = gtk::TextView::with_buffer(&buffer);
        view.add_css_class("kalam-pdf-text-editor");
        view.add_css_class(&format!("kalam-pdf-text-size-{}", size.as_str()));
        view.add_css_class(&format!("kalam-pdf-text-color-{}", color.as_str()));
        if !paper_background {
            view.add_css_class("kalam-pdf-text-editor-clear");
        }
        view.set_wrap_mode(gtk::WrapMode::WordChar);
        view.set_accepts_tab(false);
        view.set_left_margin(8);
        view.set_right_margin(8);
        view.set_top_margin(7);
        view.set_bottom_margin(7);
        view.update_property(&[gtk::accessible::Property::Label("Text box")]);
        let (display_x, display_y, display_w, display_h) =
            self.display_rect_for_source_rect(rect);
        view.set_halign(gtk::Align::Start);
        view.set_valign(gtk::Align::Start);
        view.set_margin_start(display_x);
        view.set_margin_top(display_y);
        view.set_size_request(display_w, display_h);
        let focus = gtk::EventControllerFocus::new();
        {
            let tx = sender.input_sender().clone();
            focus.connect_leave(move |_| {
                let _ = tx.send(PdfReaderMsg::CommitDrawingText);
            });
        }
        view.add_controller(focus);
        overlay.add_overlay(&view);
        self.selected_drawing_id = editing_id;
        self.drawing_text_editor = Some(PdfDrawingTextEditor {
            slot,
            page_id,
            editing_id,
            original,
            rect,
            size,
            color,
            paper_background,
            view: view.clone(),
        });
        self.refresh_drawing_paints();
        self.queue_drawing_slot(slot);
        glib::idle_add_local_once(move || {
            view.grab_focus();
            let buffer = view.buffer();
            buffer.place_cursor(&buffer.end_iter());
        });
    }

    pub fn commit_drawing_text_editor(&mut self) {
        let Some(editor) = self.drawing_text_editor.take() else {
            return;
        };
        let buffer = editor.view.buffer();
        let text = buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .trim()
            .to_string();
        if editor.view.parent().is_some() {
            editor.view.unparent();
        }
        let slot = editor.slot;
        if let Some(original) = editor.original {
            let original = *original;
            if text.is_empty() {
                self.delete_drawing_row(original);
            } else {
                let height = Self::text_rect_height(
                    &text,
                    editor.size,
                    editor.rect.2,
                    editor.rect.3,
                );
                let rect = self.clamp_text_rect((
                    editor.rect.0,
                    editor.rect.1,
                    editor.rect.2,
                    height,
                ));
                let mut after = original.clone();
                after.color = editor.color.as_str().to_string();
                after.geometry = PdfDrawingGeometry::Text {
                    rect,
                    text,
                    size: editor.size.as_str().to_string(),
                    paper_background: editor.paper_background,
                };
                if after != original {
                    if crate::notify::report(
                        self.catalog.update_pdf_drawing(&after),
                        "Could not save the text box",
                    ) {
                        self.remember_deleted(PdfDeletedMark::DrawingBefore(Box::new(
                            original,
                        )));
                    }
                    self.reload_drawings();
                }
                self.selected_drawing_id = editor.editing_id;
            }
        } else if !text.is_empty() {
            let height =
                Self::text_rect_height(&text, editor.size, editor.rect.2, editor.rect.3);
            let rect = self.clamp_text_rect((
                editor.rect.0,
                editor.rect.1,
                editor.rect.2,
                height,
            ));
            let drawing = PdfDrawing {
                id: 0,
                book_id: self.book_id,
                page_id: editor.page_id.clone(),
                color: editor.color.as_str().to_string(),
                opacity: 1.0,
                width_pts: 1.0,
                geometry: PdfDrawingGeometry::Text {
                    rect,
                    text,
                    size: editor.size.as_str().to_string(),
                    paper_background: editor.paper_background,
                },
                created_at: String::new(),
                updated_at: String::new(),
            };
            match self.catalog.insert_pdf_drawing(&drawing) {
                Ok(id) => {
                    self.selected_drawing_id = Some(id);
                    if let Ok(Some(saved)) = self.catalog.get_pdf_drawing(id) {
                        self.remember_deleted(PdfDeletedMark::DrawingCreated(Box::new(
                            saved,
                        )));
                    }
                }
                Err(err) => {
                    crate::notify::error("Could not save the text box", &err.to_string())
                }
            }
            self.reload_drawings();
        }
        self.refresh_drawing_paints();
        self.queue_drawing_slot(slot);
    }

    pub fn edit_drawing_text(&mut self, id: i64, sender: &ComponentSender<Self>) {
        let Some(drawing) = self
            .drawings
            .iter()
            .find(|drawing| drawing.id == id)
            .cloned()
        else {
            return;
        };
        let PdfDrawingGeometry::Text { rect, .. } = &drawing.geometry else {
            return;
        };
        let rect = *rect;
        let Some(slot) = self.slot_for_drawing(&drawing) else {
            return;
        };
        let page_id = drawing.page_id.clone();
        self.open_drawing_text_editor(slot, Some(drawing), page_id, rect, sender);
    }

    pub fn show_sticky_drawing(&mut self, id: i64, sender: &ComponentSender<Self>) {
        let Some(drawing) = self
            .drawings
            .iter()
            .find(|drawing| drawing.id == id)
            .cloned()
        else {
            return;
        };
        let Some(slot) = self.slot_for_drawing(&drawing) else {
            return;
        };
        self.selected_drawing_id = Some(id);
        self.refresh_drawing_paints();
        self.queue_drawing_slot(slot);
        self.show_selected_drawing_popover(slot, sender);
    }

    /// A double-click on a saved object: a text box opens for
    /// editing, anything else opens its options. Returns false when
    /// the click fell on bare page and belongs to word selection.
    pub fn open_drawing_object_at(
        &mut self,
        slot: PageSlot,
        x: f64,
        y: f64,
        sender: &ComponentSender<Self>,
    ) -> bool {
        let Some(drawing) = self.drawing_at(slot, x, y) else {
            return false;
        };
        self.clear_selection();
        self.selected_drawing_id = Some(drawing.id);
        if let PdfDrawingGeometry::Text { rect, .. } = &drawing.geometry {
            let rect = *rect;
            let page_id = drawing.page_id.clone();
            self.open_drawing_text_editor(slot, Some(drawing), page_id, rect, sender);
        } else {
            self.refresh_drawing_paints();
            self.queue_drawing_slot(slot);
            self.show_selected_drawing_popover(slot, sender);
        }
        true
    }

    fn place_text_or_sticky(
        &mut self,
        slot: PageSlot,
        x: f64,
        y: f64,
        sender: &ComponentSender<Self>,
    ) {
        let Some((_, page_id, point, _)) = self.source_point_for_display(slot, x, y)
        else {
            return;
        };
        if let Some(hit) = self.drawing_at(slot, x, y) {
            match (self.draw_tool, &hit.geometry) {
                (PdfDrawTool::TextBox, PdfDrawingGeometry::Text { rect, .. }) => {
                    let rect = *rect;
                    let page_id = hit.page_id.clone();
                    self.open_drawing_text_editor(slot, Some(hit), page_id, rect, sender);
                    return;
                }
                (PdfDrawTool::Sticky, PdfDrawingGeometry::Sticky { .. }) => {
                    self.selected_drawing_id = Some(hit.id);
                    self.refresh_drawing_paints();
                    self.queue_drawing_slot(slot);
                    self.show_selected_drawing_popover(slot, sender);
                    return;
                }
                _ => {}
            }
        }
        match self.draw_tool {
            PdfDrawTool::TextBox => {
                let size = self.draw_settings.text_size;
                let rect = (
                    point.0,
                    point.1,
                    190.0,
                    size.pts() * 1.35 + 16.0,
                );
                self.open_drawing_text_editor(slot, None, page_id, rect, sender);
            }
            PdfDrawTool::Sticky => {
                let drawing = PdfDrawing {
                    id: 0,
                    book_id: self.book_id,
                    page_id,
                    color: self.draw_settings.sticky_color.as_str().to_string(),
                    opacity: 1.0,
                    width_pts: 1.0,
                    geometry: PdfDrawingGeometry::Sticky {
                        point,
                        text: String::new(),
                    },
                    created_at: String::new(),
                    updated_at: String::new(),
                };
                match self.catalog.insert_pdf_drawing(&drawing) {
                    Ok(id) => {
                        self.selected_drawing_id = Some(id);
                        if let Ok(Some(saved)) = self.catalog.get_pdf_drawing(id) {
                            self.remember_deleted(PdfDeletedMark::DrawingCreated(Box::new(
                                saved,
                            )));
                        }
                    }
                    Err(err) => {
                        crate::notify::error("Could not save the note", &err.to_string())
                    }
                }
                self.reload_drawings();
                self.refresh_drawing_paints();
                self.queue_drawing_slot(slot);
                self.show_selected_drawing_popover(slot, sender);
            }
            _ => {}
        }
    }

    /// A Select press on a drawing: take hold of it (a drag moves it,
    /// a click opens its popover). Returns false on empty page, where
    /// the press belongs to text selection as before.
    pub fn begin_drawing_selection(&mut self, slot: PageSlot, x: f64, y: f64) -> bool {
        if self.drawing_text_editor.is_some() {
            self.commit_drawing_text_editor();
        }
        let Some(drawing) = self.drawing_at(slot, x, y) else {
            if self.selected_drawing_id.is_some() {
                self.selected_drawing_id = None;
                self.dismiss_drawing_popover();
                self.refresh_drawing_paints();
                self.queue_drawing_slot(slot);
            }
            return false;
        };
        let Some((_, _, source, _)) = self.source_point_for_display(slot, x, y) else {
            return false;
        };
        self.clear_selection();
        self.dismiss_drawing_popover();
        self.selected_drawing_id = Some(drawing.id);
        self.drawing_drag = Some(PdfDrawingDragState {
            slot,
            start_x: x,
            start_y: y,
            kind: PdfDrawingDragKind::Move {
                id: drawing.id,
                original: Box::new(drawing),
                last_source: source,
            },
        });
        self.refresh_drawing_paints();
        self.queue_drawing_slot(slot);
        true
    }

    pub fn update_drawing_move(&mut self, slot: PageSlot, dx: f64, dy: f64) -> bool {
        let Some(drag) = self.drawing_drag.as_mut() else {
            return false;
        };
        if drag.slot != slot {
            return false;
        }
        let (start_x, start_y) = (drag.start_x, drag.start_y);
        let (id, last_source) = match &drag.kind {
            PdfDrawingDragKind::Move {
                id, last_source, ..
            } => (*id, *last_source),
            _ => return false,
        };
        let Some((_, _, source, _)) =
            self.source_point_for_display(slot, start_x + dx, start_y + dy)
        else {
            return true;
        };
        let (move_dx, move_dy) = (source.0 - last_source.0, source.1 - last_source.1);
        if let PdfDrawingDragKind::Move { last_source, .. } =
            &mut self.drawing_drag.as_mut().expect("drag checked").kind
        {
            *last_source = source;
        }
        if let Some(drawing) = self.drawings.iter_mut().find(|drawing| drawing.id == id) {
            drawing.geometry.translate(move_dx, move_dy);
        }
        self.refresh_drawing_paints();
        self.queue_drawing_slot(slot);
        true
    }

    pub fn finish_drawing_move(
        &mut self,
        slot: PageSlot,
        dx: f64,
        dy: f64,
        sender: &ComponentSender<Self>,
    ) -> bool {
        let Some(drag) = self.drawing_drag.take() else {
            return false;
        };
        if drag.slot != slot
            || !matches!(drag.kind, PdfDrawingDragKind::Move { .. })
        {
            self.drawing_drag = Some(drag);
            return false;
        }
        let (id, original) = match drag.kind {
            PdfDrawingDragKind::Move { id, original, .. } => (id, *original),
            _ => unreachable!("move drag checked above"),
        };
        if dx.abs() > 3.0 || dy.abs() > 3.0 {
            if let Some(current) = self
                .drawings
                .iter()
                .find(|drawing| drawing.id == id)
                .cloned()
            {
                if crate::notify::report(
                    self.catalog.update_pdf_drawing(&current),
                    "Could not move the drawing",
                ) {
                    self.remember_deleted(PdfDeletedMark::DrawingBefore(Box::new(original)));
                }
                self.reload_drawings();
            }
        } else {
            // A click may have nudged the in-memory geometry by
            // less than a move counts as; put the stored row back.
            self.reload_drawings();
        }
        self.refresh_drawing_paints();
        self.queue_drawing_slot(slot);
        self.show_selected_drawing_popover(slot, sender);
        true
    }

    pub fn begin_active_drawing(&mut self, slot: PageSlot, x: f64, y: f64) {
        if self.drawing_text_editor.is_some() {
            self.commit_drawing_text_editor();
        }
        match self.draw_tool {
            PdfDrawTool::Select => {}
            PdfDrawTool::Eraser => {
                self.drawing_drag = Some(PdfDrawingDragState {
                    slot,
                    start_x: x,
                    start_y: y,
                    kind: PdfDrawingDragKind::Erase,
                });
                self.erase_at(slot, x, y);
            }
            PdfDrawTool::Highlighter | PdfDrawTool::Pen => {
                let Some((_, page_id, point, _)) =
                    self.source_point_for_display(slot, x, y)
                else {
                    return;
                };
                self.dismiss_drawing_popover();
                self.selected_drawing_id = None;
                let (color, opacity, width_pts) = if self.draw_tool == PdfDrawTool::Highlighter {
                    (
                        self.draw_settings.highlighter_color,
                        self.draw_settings.highlighter_opacity(),
                        14.0,
                    )
                } else {
                    (
                        self.draw_settings.pen_color,
                        1.0,
                        self.draw_settings.pen_width.pts(),
                    )
                };
                self.active_drawing.replace(Some(PdfActiveStroke {
                    slot,
                    page_id,
                    tool: self.draw_tool,
                    color,
                    opacity,
                    width_pts,
                    points: vec![point],
                    source_width: self.source_page_width as f32,
                    source_height: self.source_page_height as f32,
                    rotation: self.rotation_degrees,
                }));
                self.drawing_drag = Some(PdfDrawingDragState {
                    slot,
                    start_x: x,
                    start_y: y,
                    kind: PdfDrawingDragKind::Stroke,
                });
                self.queue_drawing_slot(slot);
            }
            PdfDrawTool::Shape => {
                let Some((_, page_id, point, _)) =
                    self.source_point_for_display(slot, x, y)
                else {
                    return;
                };
                self.dismiss_drawing_popover();
                self.selected_drawing_id = None;
                let filled =
                    self.draw_settings.shape_filled && self.draw_settings.shape_kind.can_fill();
                self.active_shape.replace(Some(PdfActiveShape {
                    slot,
                    page_id,
                    kind: self.draw_settings.shape_kind,
                    start: point,
                    end: point,
                    filled,
                    color: self.draw_settings.shape_color,
                    opacity: if filled { 0.35 } else { 1.0 },
                    width_pts: self.draw_settings.shape_width.pts(),
                    source_width: self.source_page_width as f32,
                    source_height: self.source_page_height as f32,
                    rotation: self.rotation_degrees,
                }));
                self.drawing_drag = Some(PdfDrawingDragState {
                    slot,
                    start_x: x,
                    start_y: y,
                    kind: PdfDrawingDragKind::Shape,
                });
                self.queue_drawing_slot(slot);
            }
            PdfDrawTool::TextBox | PdfDrawTool::Sticky => {
                if self.drawing_text_editor.is_some() {
                    self.commit_drawing_text_editor();
                }
                self.dismiss_drawing_popover();
                self.drawing_drag = Some(PdfDrawingDragState {
                    slot,
                    start_x: x,
                    start_y: y,
                    kind: PdfDrawingDragKind::Place,
                });
            }
        }
    }

    pub fn update_active_drawing(&mut self, slot: PageSlot, dx: f64, dy: f64) {
        let Some(drag) = self.drawing_drag.clone() else {
            return;
        };
        if drag.slot != slot {
            return;
        }
        let (x, y) = (drag.start_x + dx, drag.start_y + dy);
        match drag.kind {
            PdfDrawingDragKind::Erase => self.erase_at(slot, x, y),
            PdfDrawingDragKind::Stroke => {
                let Some((_, _, point, _)) = self.source_point_for_display(slot, x, y) else {
                    return;
                };
                if let Some(stroke) = self.active_drawing.borrow_mut().as_mut() {
                    if stroke.slot == slot {
                        let far_enough = stroke.points.last().is_none_or(|last| {
                            (point.0 - last.0).hypot(point.1 - last.1) >= 1.0
                        });
                        if far_enough {
                            stroke.points.push(point);
                        }
                    }
                }
                self.queue_drawing_slot(slot);
            }
            PdfDrawingDragKind::Shape => {
                let Some((_, _, point, _)) = self.source_point_for_display(slot, x, y) else {
                    return;
                };
                if let Some(shape) = self.active_shape.borrow_mut().as_mut() {
                    if shape.slot == slot {
                        shape.end = point;
                    }
                }
                self.queue_drawing_slot(slot);
            }
            PdfDrawingDragKind::Place | PdfDrawingDragKind::Move { .. } => {}
        }
    }

    pub fn finish_active_drawing(
        &mut self,
        slot: PageSlot,
        dx: f64,
        dy: f64,
        sender: &ComponentSender<Self>,
    ) {
        let Some(drag) = self.drawing_drag.take() else {
            return;
        };
        if drag.slot != slot {
            self.drawing_drag = Some(drag);
            return;
        }
        match &drag.kind {
            PdfDrawingDragKind::Shape => {
                if let Some((_, _, point, _)) = self
                    .source_point_for_display(slot, drag.start_x + dx, drag.start_y + dy)
                {
                    if let Some(shape) = self.active_shape.borrow_mut().as_mut() {
                        shape.end = point;
                    }
                }
                let Some(shape) = self.active_shape.replace(None) else {
                    return;
                };
                let enough = if shape.kind.can_fill() {
                    (shape.end.0 - shape.start.0).abs() >= 4.0
                        && (shape.end.1 - shape.start.1).abs() >= 4.0
                } else {
                    (shape.end.0 - shape.start.0).hypot(shape.end.1 - shape.start.1) >= 4.0
                };
                if !enough {
                    self.queue_drawing_slot(slot);
                    return;
                }
                let drawing = PdfDrawing {
                    id: 0,
                    book_id: self.book_id,
                    page_id: shape.page_id.clone(),
                    color: shape.color.as_str().to_string(),
                    opacity: shape.opacity,
                    width_pts: shape.width_pts,
                    geometry: PdfDrawingGeometry::Shape {
                        shape: shape.kind.as_str().to_string(),
                        start: shape.start,
                        end: shape.end,
                        filled: shape.filled,
                    },
                    created_at: String::new(),
                    updated_at: String::new(),
                };
                match self.catalog.insert_pdf_drawing(&drawing) {
                    Ok(id) => {
                        if let Ok(Some(saved)) = self.catalog.get_pdf_drawing(id) {
                            self.remember_deleted(PdfDeletedMark::DrawingCreated(Box::new(
                                saved,
                            )));
                        }
                    }
                    Err(err) => {
                        crate::notify::error("Could not save the shape", &err.to_string())
                    }
                }
                self.reload_drawings();
                self.refresh_drawing_paints();
                self.queue_drawing_slot(slot);
                return;
            }
            PdfDrawingDragKind::Place => {
                self.place_text_or_sticky(slot, drag.start_x, drag.start_y, sender);
                return;
            }
            PdfDrawingDragKind::Stroke => {}
            PdfDrawingDragKind::Erase | PdfDrawingDragKind::Move { .. } => return,
        }
        if let Some((_, _, point, _)) =
            self.source_point_for_display(slot, drag.start_x + dx, drag.start_y + dy)
        {
            if let Some(stroke) = self.active_drawing.borrow_mut().as_mut() {
                if stroke.points.last().is_none_or(|last| *last != point) {
                    stroke.points.push(point);
                }
            }
        }
        let Some(stroke) = self.active_drawing.replace(None) else {
            return;
        };
        if stroke.points.is_empty() {
            return;
        }
        let drawing = PdfDrawing {
            id: 0,
            book_id: self.book_id,
            page_id: stroke.page_id.clone(),
            color: stroke.color.as_str().to_string(),
            opacity: stroke.opacity,
            width_pts: stroke.width_pts,
            geometry: PdfDrawingGeometry::Stroke {
                tool: if stroke.tool == PdfDrawTool::Highlighter {
                    "highlighter".to_string()
                } else {
                    "pen".to_string()
                },
                points: stroke.points.clone(),
            },
            created_at: String::new(),
            updated_at: String::new(),
        };
        match self.catalog.insert_pdf_drawing(&drawing) {
            Ok(id) => {
                if let Ok(Some(saved)) = self.catalog.get_pdf_drawing(id) {
                    self.remember_deleted(PdfDeletedMark::DrawingCreated(Box::new(saved)));
                }
            }
            Err(err) => crate::notify::error("Could not save the drawing", &err.to_string()),
        }
        self.reload_drawings();
        self.refresh_drawing_paints();
        self.queue_drawing_slot(slot);
    }

    fn erase_at(&mut self, slot: PageSlot, x: f64, y: f64) {
        let Some(drawing) = self.drawing_at(slot, x, y) else {
            return;
        };
        self.delete_drawing_row(drawing);
        self.queue_drawing_slot(slot);
    }

    pub fn delete_drawing_row(&mut self, drawing: PdfDrawing) {
        if crate::notify::report(
            self.catalog.delete_pdf_drawing(drawing.id),
            "Could not delete the drawing",
        ) {
            if self.selected_drawing_id == Some(drawing.id) {
                self.selected_drawing_id = None;
                self.dismiss_drawing_popover();
            }
            self.remember_deleted(PdfDeletedMark::Drawing(Box::new(drawing)));
        }
        self.reload_drawings();
        self.refresh_drawing_paints();
        self.queue_all_drawing_areas();
    }

    pub fn dismiss_drawing_popover(&mut self) {
        self.commit_sticky_card_text();
        self.sticky_card = None;
        if let Some(popover) = self.drawing_popover.take() {
            popover.popdown();
            popover.unparent();
        }
    }

    /// Open the selected drawing's popover on whichever materialised
    /// slot currently carries its page.
    pub fn show_selected_drawing_popover_for_selection(
        &mut self,
        sender: &ComponentSender<Self>,
    ) {
        let Some(id) = self.selected_drawing_id else {
            return;
        };
        let Some(drawing) = self
            .drawings
            .iter()
            .find(|drawing| drawing.id == id)
            .cloned()
        else {
            return;
        };
        let Some(position) = self
            .page_identities
            .iter()
            .find(|identity| identity.page_id == drawing.page_id)
            .map(|identity| identity.position)
        else {
            return;
        };
        let Some(slot) = self
            .page_overlays
            .keys()
            .copied()
            .find(|slot| self.page_for_slot(*slot) == Some(position))
        else {
            return;
        };
        self.show_selected_drawing_popover(slot, sender);
    }

    pub fn show_selected_drawing_popover(
        &mut self,
        slot: PageSlot,
        sender: &ComponentSender<Self>,
    ) {
        self.dismiss_drawing_popover();
        let Some(id) = self.selected_drawing_id else {
            return;
        };
        let Some(drawing) = self
            .drawings
            .iter()
            .find(|drawing| drawing.id == id)
            .cloned()
        else {
            return;
        };
        let Some(overlay) = self.page_overlays.get(&slot).cloned() else {
            return;
        };
        if !overlay.is_mapped() {
            // A Wayland popup shown before its parent surface is
            // mapped can keep that surface from ever presenting.
            // Ask again once the page has had a frame to land in.
            let s = sender.clone();
            glib::timeout_add_local_once(std::time::Duration::from_millis(120), move || {
                let _ = s.input_sender().send(PdfReaderMsg::ShowDrawingPopover);
            });
            return;
        }
        let (target_w, target_h) = self.drawing_target_dimensions();
        let (x, y, w, h) = drawing_bounds(&drawing.geometry, drawing.width_pts);
        let a = source_point_to_display(
            (x, y),
            self.source_page_width as f32,
            self.source_page_height as f32,
            target_w,
            target_h,
            self.rotation_degrees,
        );
        let b = source_point_to_display(
            (x + w, y + h),
            self.source_page_width as f32,
            self.source_page_height as f32,
            target_w,
            target_h,
            self.rotation_degrees,
        );
        let anchor = gdk::Rectangle::new(
            a.0.min(b.0) as i32,
            a.1.min(b.1) as i32,
            (a.0.max(b.0) - a.0.min(b.0)).max(1.0) as i32,
            (a.1.max(b.1) - a.1.min(b.1)).max(1.0) as i32,
        );
        let position = if matches!(drawing.geometry, PdfDrawingGeometry::Sticky { .. }) {
            if (a.0.min(b.0) + a.0.max(b.0)) / 2.0 < target_w * 0.62 {
                gtk::PositionType::Right
            } else {
                gtk::PositionType::Left
            }
        } else if a.1.max(b.1) < target_h * 0.62 {
            gtk::PositionType::Bottom
        } else {
            gtk::PositionType::Top
        };
        let (popover, sticky_state) = build_selected_drawing_popover(
            overlay.upcast_ref(),
            anchor,
            position,
            &drawing,
            sender,
        );
        let sticky_view = sticky_state.as_ref().map(|state| state.view.clone());
        self.sticky_card = sticky_state;
        popover.popup();
        self.drawing_popover = Some(popover);
        if let Some(view) = sticky_view {
            glib::idle_add_local_once(move || {
                view.grab_focus();
                let buffer = view.buffer();
                buffer.place_cursor(&buffer.end_iter());
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn stroke() -> PdfDrawingGeometry {
        PdfDrawingGeometry::Stroke {
            tool: "pen".to_string(),
            points: vec![(10.0, 10.0), (110.0, 10.0), (110.0, 60.0)],
        }
    }

    #[test]
    fn ink_colors_round_trip_and_keep_the_fixed_palette() {
        for color in PdfInkColor::ALL {
            assert_eq!(PdfInkColor::from_name(color.as_str()), *color);
        }
        assert_eq!(
            PdfInkColor::Ink.rgb(),
            (42.0 / 255.0, 36.0 / 255.0, 32.0 / 255.0)
        );
        assert_eq!(PdfInkColor::from_name("unknown"), PdfInkColor::Ink);
    }

    #[test]
    fn stroke_hit_testing_uses_segments_and_width() {
        let geometry = stroke();
        assert!(drawing_hit(&geometry, 3.0, (60.0, 11.0), 0.0));
        assert!(drawing_hit(&geometry, 3.0, (112.0, 40.0), 2.0));
        assert!(!drawing_hit(&geometry, 3.0, (60.0, 30.0), 0.0));
    }

    #[test]
    fn stroke_bounds_include_half_the_width() {
        assert_eq!(
            drawing_bounds(&stroke(), 4.0),
            (8.0, 8.0, 104.0, 54.0)
        );
    }

    #[test]
    fn shape_and_text_choices_round_trip_their_stored_names() {
        for kind in PdfShapeKind::ALL {
            assert_eq!(PdfShapeKind::from_name(kind.as_str()), *kind);
        }
        for size in PdfTextSize::ALL {
            assert_eq!(PdfTextSize::from_name(size.as_str()), *size);
        }
        assert!(PdfShapeKind::Rectangle.can_fill());
        assert!(!PdfShapeKind::Arrow.can_fill());
    }

    #[test]
    fn shape_text_and_sticky_hit_testing_follow_their_geometry() {
        let filled = PdfDrawingGeometry::Shape {
            shape: "rectangle".to_string(),
            start: (10.0, 20.0),
            end: (110.0, 80.0),
            filled: true,
        };
        assert!(drawing_hit(&filled, 3.0, (60.0, 50.0), 0.0));
        let outline = PdfDrawingGeometry::Shape {
            shape: "rectangle".to_string(),
            start: (10.0, 20.0),
            end: (110.0, 80.0),
            filled: false,
        };
        assert!(!drawing_hit(&outline, 3.0, (60.0, 50.0), 0.0));
        assert!(drawing_hit(&outline, 3.0, (60.0, 21.0), 0.0));

        let text = PdfDrawingGeometry::Text {
            rect: (30.0, 40.0, 180.0, 42.0),
            text: "Compare with p. 24".to_string(),
            size: "m".to_string(),
            paper_background: true,
        };
        assert!(drawing_hit(&text, 1.0, (100.0, 60.0), 0.0));

        let sticky = PdfDrawingGeometry::Sticky {
            point: (100.0, 120.0),
            text: "Check this".to_string(),
        };
        assert!(drawing_hit(&sticky, 1.0, (108.0, 120.0), 0.0));
        assert_eq!(drawing_bounds(&sticky, 0.0), (89.0, 109.0, 22.0, 22.0));
    }

    #[test]
    fn source_points_map_through_quarter_turns() {
        assert_eq!(
            source_point_to_display((25.0, 50.0), 100.0, 200.0, 200.0, 100.0, 90),
            (150.0, 25.0)
        );
        assert_eq!(
            source_point_to_display((25.0, 50.0), 100.0, 200.0, 100.0, 200.0, 180),
            (75.0, 150.0)
        );
    }
}
