//! PDF drawing tools (decisions 66–67, 69, 72): the bottom-centre
//! annotation toolbar, freehand pen/highlighter strokes, the eraser,
//! and selecting a saved stroke to recolour, reweight, move or delete.
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
}

impl PdfDrawTool {
    pub const ALL: &'static [PdfDrawTool] = &[
        PdfDrawTool::Select,
        PdfDrawTool::Highlighter,
        PdfDrawTool::Pen,
        PdfDrawTool::Eraser,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Select => "Select",
            Self::Highlighter => "Highlighter",
            Self::Pen => "Pen",
            Self::Eraser => "Eraser",
        }
    }

    pub fn icon_name(self) -> &'static str {
        match self {
            Self::Select => "kalam-draw-select-symbolic",
            Self::Highlighter => "kalam-highlight-symbolic",
            Self::Pen => "kalam-draw-pen-symbolic",
            Self::Eraser => "kalam-draw-eraser-symbolic",
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

/// The tools' remembered answers. Global rather than per book: the
/// colour in the pen is a property of the reader's hand, not the book.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfDrawingSettings {
    pub highlighter_color: PdfInkColor,
    pub highlighter_opacity_percent: u8,
    pub pen_color: PdfInkColor,
    pub pen_width: PdfStrokeWidth,
}

impl Default for PdfDrawingSettings {
    fn default() -> Self {
        Self {
            highlighter_color: PdfInkColor::Yellow,
            highlighter_opacity_percent: 60,
            pen_color: PdfInkColor::Ink,
            pen_width: PdfStrokeWidth::Medium,
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

/// One saved drawing prepared for a materialised page slot.
#[derive(Debug, Clone, PartialEq)]
pub struct PdfDrawingPaint {
    pub slot: PageSlot,
    pub drawing: PdfDrawing,
    pub source_width: f32,
    pub source_height: f32,
    pub rotation: i32,
    pub selected: bool,
}

#[derive(Debug, Clone)]
pub enum PdfDrawingDragKind {
    Stroke,
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
        PdfDrawingGeometry::Sticky { point, .. } => (point.0, point.1, point.0, point.1),
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

fn paint_stroke_path(
    cr: &cairo::Context,
    points: &[(f32, f32)],
    source_width: f32,
    source_height: f32,
    display_width: f64,
    display_height: f64,
    rotation: i32,
    color: PdfInkColor,
    opacity: f64,
    width_pts: f64,
) {
    if points.is_empty() {
        return;
    }
    let scale = display_scale(
        source_width,
        source_height,
        display_width,
        display_height,
        rotation,
    );
    let line_width = (width_pts.max(0.1) * scale).max(1.0);
    let (r, g, b) = color.rgb();
    cr.set_source_rgba(r, g, b, opacity.clamp(0.0, 1.0));
    cr.set_line_width(line_width);
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    let mapped: Vec<(f64, f64)> = points
        .iter()
        .map(|point| {
            source_point_to_display(
                *point,
                source_width,
                source_height,
                display_width,
                display_height,
                rotation,
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

/// Paint one saved object. Only strokes exist in this tranche; the
/// other geometry kinds paint with their own tools when they land.
pub fn paint_drawing(
    cr: &cairo::Context,
    paint: &PdfDrawingPaint,
    display_width: f64,
    display_height: f64,
    accent_rgb: [f32; 3],
) {
    if let PdfDrawingGeometry::Stroke { points, .. } = &paint.drawing.geometry {
        paint_stroke_path(
            cr,
            points,
            paint.source_width,
            paint.source_height,
            display_width,
            display_height,
            paint.rotation,
            PdfInkColor::from_name(&paint.drawing.color),
            paint.drawing.opacity,
            paint.drawing.width_pts,
        );
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
        &stroke.points,
        stroke.source_width,
        stroke.source_height,
        display_width,
        display_height,
        stroke.rotation,
        stroke.color,
        stroke.opacity,
        stroke.width_pts,
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

pub struct PdfDrawingWidgets {
    pub root: gtk::Box,
    pub tool_buttons: Vec<(PdfDrawTool, gtk::Button)>,
    pub highlighter_options: gtk::Box,
    pub pen_options: gtk::Box,
    pub highlighter_color_buttons: Vec<(PdfInkColor, gtk::Button)>,
    pub highlighter_opacity_buttons: Vec<(u8, gtk::Button)>,
    pub pen_color_buttons: Vec<(PdfInkColor, gtk::Button)>,
    pub pen_width_buttons: Vec<(PdfStrokeWidth, gtk::Button)>,
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
        highlighter_color_buttons,
        highlighter_opacity_buttons,
        pen_color_buttons,
        pen_width_buttons,
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
}

/// The small popover a selected drawing opens (decision 66):
/// colour, thickness, delete.
pub fn build_selected_drawing_popover(
    host: &gtk::Widget,
    anchor: gdk::Rectangle,
    below: bool,
    drawing: &PdfDrawing,
    sender: &ComponentSender<PdfReaderModel>,
) -> gtk::Popover {
    let popover = gtk::Popover::new();
    popover.set_parent(host);
    popover.set_autohide(true);
    popover.set_has_arrow(false);
    popover.set_position(if below {
        gtk::PositionType::Bottom
    } else {
        gtk::PositionType::Top
    });
    popover.set_pointing_to(Some(&anchor));
    popover.add_css_class("kalam-pdf-draw-popover-shell");

    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("kalam-pdf-draw-popover");
    card.set_size_request(252, -1);

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

    if matches!(drawing.geometry, PdfDrawingGeometry::Stroke { .. }) {
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
    popover
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
    ) -> Option<(usize, String, (f32, f32), (f64, f64))> {
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
                paints.push(PdfDrawingPaint {
                    slot,
                    drawing: drawing.clone(),
                    source_width: self.source_page_width as f32,
                    source_height: self.source_page_height as f32,
                    rotation: self.rotation_degrees,
                    selected: self.selected_drawing_id == Some(drawing.id),
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
        let changed = self.draw_tool != tool;
        self.draw_tool = tool;
        self.draw_tool_shared.set(tool);
        self.draw_options_open = matches!(tool, PdfDrawTool::Highlighter | PdfDrawTool::Pen);
        if !tool.is_select() {
            self.clear_selection();
            self.selected_drawing_id = None;
            self.dismiss_drawing_popover();
        }
        if changed {
            self.active_drawing.replace(None);
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

    /// A Select press on a drawing: take hold of it (a drag moves it,
    /// a click opens its popover). Returns false on empty page, where
    /// the press belongs to text selection as before.
    pub fn begin_drawing_selection(&mut self, slot: PageSlot, x: f64, y: f64) -> bool {
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
            PdfDrawingDragKind::Move { .. } => {}
        }
    }

    pub fn finish_active_drawing(&mut self, slot: PageSlot, dx: f64, dy: f64) {
        let Some(drag) = self.drawing_drag.take() else {
            return;
        };
        if drag.slot != slot {
            self.drawing_drag = Some(drag);
            return;
        }
        if !matches!(drag.kind, PdfDrawingDragKind::Stroke) {
            return;
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
        if let Some(popover) = self.drawing_popover.take() {
            popover.popdown();
            popover.unparent();
        }
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
        let below = a.1.max(b.1) < target_h * 0.62;
        let popover = build_selected_drawing_popover(
            overlay.upcast_ref(),
            anchor,
            below,
            &drawing,
            sender,
        );
        popover.popup();
        self.drawing_popover = Some(popover);
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
        assert!(drawing_hit(&geometry, 3.0, (60.0, 12.0), 0.0));
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
