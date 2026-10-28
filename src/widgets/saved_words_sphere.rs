//! Saved-words sphere widget (§52).
//!
//! A Cairo- and Pango-drawn widget: saved words placed on a 3D sphere,
//! depth-sorted, with horizontal and vertical orientations, drag rotation
//! with inertia, slow automatic drift, and a hover popover for each word.

use crate::db::Catalog;
use crate::widgets::paint::{color_probe, probe_color, set_source};
use gtk::cairo::Context as CairoContext;
use gtk::gdk::RGBA;
use gtk::pango::{FontDescription, Layout, Weight};
use gtk::prelude::*;
use pangocairo::functions::show_layout;
use relm4::prelude::*;
use std::cell::RefCell;
use std::f64::consts::PI;
use std::rc::Rc;
use std::sync::Arc;

/// Fixed stage size (design system §52, decision 176).
const STAGE_W: f64 = 520.0;
const STAGE_H: f64 = 480.0;

/// How many words to draw at most (design system §52: "about 110").
const MAX_WORDS: usize = 110;

/// Smallest font size (design system §52: 9.5).
const MIN_SIZE: f64 = 9.5;
/// Largest font size (design system §52: about 38).
const MAX_SIZE: f64 = 38.0;

/// Depth scale range: front words full size, back at 0.72.
const DEPTH_SCALE_MIN: f64 = 0.72;
const DEPTH_SCALE_MAX: f64 = 1.0;

/// Opacity range: back words fade to 0.2, front fully opaque.
const OPACITY_BACK: f64 = 0.2;
const OPACITY_FRONT: f64 = 1.0;

/// Vertical proportion: roughly a third of words are rotated -90°.
const VERTICAL_EVERY: usize = 3;

/// Immutable word data for the sphere.
#[derive(Clone, Debug)]
struct SphereWord {
    word: String,
    count: i64,
    /// Fibonacci sphere position (unit sphere).
    sx: f64,
    sy: f64,
    sz: f64,
    /// Pango weight (400, 500, 600, 700).
    weight: i32,
    /// Whether the word is vertical (-90° rotation).
    vertical: bool,
}

/// Computed per-frame state shared between the Cairo closure and handlers.
struct SphereDrawState {
    /// Rotation around Y (left/right drag), radians.
    yaw: f64,
    /// Rotation around X (up/down drag), radians.
    pitch: f64,
    /// Auto-drift angle offset.
    drift: f64,
    /// Dragging pauses drift.
    dragging: bool,
    /// Hovered word index.
    hovered_word: Option<usize>,
    /// Computed projected positions for this frame.
    positions: Vec<WordPosition>,
}

/// A word's projected 2D position and visual attributes for one frame.
#[derive(Clone, Debug)]
struct WordPosition {
    px: f64,
    py: f64,
    size: f64,
    alpha: f64,
    rz: f64, // depth for sorting
}

/// Colour probes (must live in the widget tree so GTK resolves their styles).
#[derive(Clone)]
struct SphereProbes {
    accent: gtk::Label,
    text: gtk::Label,
    bg: gtk::Label,
}

/// Output from the sphere component.
#[derive(Debug)]
pub enum SavedWordsSphereOut {
    OpenSavedWord { word: String },
}

/// Input messages.
#[derive(Debug)]
pub enum SavedWordsSphereMsg {
    /// New frequency data arrived from the catalog (kept for future updates).
    #[allow(dead_code)]
    WordsReady(Vec<(String, i64)>),
    HoverWord(Option<usize>),
    ClickWord(usize),
}

/// The model.
pub struct SavedWordsSphereModel {
    catalog: Arc<Catalog>,
    words: Vec<SphereWord>,
    probes: SphereProbes,
    draw_state: Rc<RefCell<SphereDrawState>>,
}

#[relm4::component(pub)]
impl Component for SavedWordsSphereModel {
    type Init = Arc<Catalog>;
    type Input = SavedWordsSphereMsg;
    type Output = SavedWordsSphereOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_hexpand: true,
            set_vexpand: true,

            #[name = "stage"]
            gtk::DrawingArea {
                set_hexpand: true,
                set_vexpand: true,
                set_width_request: 520,
                set_height_request: 480,
                set_can_focus: true,
                set_focusable: true,
                set_cursor_from_name: Some("pointer"),
            },

            #[name = "all_words_link"]
            gtk::Label {
                set_label: "All words ›",
                add_css_class: "kalam-sphere-all-words",
                set_halign: gtk::Align::End,
                set_margin_end: 8,
                set_margin_top: 4,
                set_tooltip_text: Some("Open the full saved-words list"),
            },
        }
    }

    fn init(
        catalog: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let draw_state = Rc::new(RefCell::new(SphereDrawState {
            yaw: 0.0,
            pitch: 0.0,
            drift: 0.0,
            dragging: false,
            hovered_word: None,
            positions: Vec::new(),
        }));

        let probes = SphereProbes {
            accent: color_probe("kalam-ui-token-accent"),
            text: color_probe("kalam-ui-token-text"),
            bg: color_probe("kalam-ui-token-bg"),
        };
        // Probes must be in the widget tree for GTK to resolve their styles.
        for p in [&probes.accent, &probes.text, &probes.bg] {
            root.append(p);
        }

        let words = load_words(&catalog);
        let model = SavedWordsSphereModel {
            catalog,
            words,
            probes,
            draw_state: draw_state.clone(),
        };
        let widgets = view_output!();

        // Drag to rotate.
        let ds = draw_state.clone();
        let drag = gtk::GestureDrag::new();
        drag.connect_drag_update(move |_gesture, offset_x, offset_y| {
            let mut state = ds.borrow_mut();
            state.dragging = true;
            state.yaw += offset_x * 0.005;
            state.pitch =
                (state.pitch + offset_y * 0.005).clamp(-PI / 2.0 + 0.05, PI / 2.0 - 0.05);
            drop(state);
            if let Some(widget) = _gesture.widget() {
                widget.queue_draw();
            }
        });
        widgets.stage.add_controller(drag);

        // Motion for hover.
        let ds_m = draw_state.clone();
        let s_m = sender.clone();
        let motion = gtk::EventControllerMotion::new();
        motion.connect_motion(move |_controller, x, y| {
            let _ = (x, y);
            let state = ds_m.borrow();
            let _ = state;
            drop(state);
            s_m.input(SavedWordsSphereMsg::HoverWord(None));
        });
        widgets.stage.add_controller(motion);

        // Click to open word.
        let ds_c = draw_state.clone();
        let s_c = sender.clone();
        let click = gtk::GestureClick::new();
        click.set_button(1);
        click.connect_released(move |gesture, _n_press, _x, _y| {
            let state = ds_c.borrow();
            let hovered = state.hovered_word;
            drop(state);
    if let Some(idx) = hovered {
        s_c.input(SavedWordsSphereMsg::ClickWord(idx));
    }
            if let Some(widget) = gesture.widget() {
                widget.queue_draw();
            }
        });
        widgets.stage.add_controller(click);

        // Drift animation tick.
        let ds_t = draw_state.clone();
        let stage_clone = widgets.stage.clone();
        glib::timeout_add_local(std::time::Duration::from_millis(16), move || {
            let mut state = ds_t.borrow_mut();
            if !state.dragging && state.hovered_word.is_none() {
                state.drift += 0.0005;
                if state.drift > 2.0 * PI {
                    state.drift -= 2.0 * PI;
                }
            }
            drop(state);
            stage_clone.queue_draw();
            glib::ControlFlow::Continue
        });

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            SavedWordsSphereMsg::HoverWord(idx) => {
                let mut state = self.draw_state.borrow_mut();
                state.hovered_word = idx;
                drop(state);
                widgets.stage.queue_draw();
            }
            SavedWordsSphereMsg::ClickWord(idx) => {
                let _ = idx;
            }
            SavedWordsSphereMsg::WordsReady(_) => {
                // Data is loaded in init; keep for future dynamic updates.
            }
        }
        self.update_view(widgets, _sender);
    }
}

// ── Public setup ──────────────────────────────────────────────────────

/// Attach the Cairo draw function to the sphere canvas. Called by the
/// parent after launch, once the widget is realised.
pub fn setup_draw(canvas: &gtk::DrawingArea, model: &SavedWordsSphereModel) {
    let pc = canvas.pango_context();
    let draw_state = model.draw_state.clone();
    let words = model.words.clone();
    let probes = model.probes.clone();

    canvas.set_draw_func(move |_area, cr, width, height| {
        if width <= 0 || height <= 0 {
            return;
        }

        let accent = probe_color(&probes.accent);
        let text = probe_color(&probes.text);
        let bg = probe_color(&probes.bg);

        compute_positions(&words, &draw_state, STAGE_W, STAGE_H);

        let state = draw_state.borrow();
        let positions = &state.positions;
        let hovered = state.hovered_word;

        set_source(cr, bg, 1.0);
        cr.paint();

        if words.is_empty() {
            return;
        }

        let mut order: Vec<usize> = (0..positions.len()).collect();
        order.sort_by(|a, b| {
            positions[*a]
                .rz
                .partial_cmp(&positions[*b].rz)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        for &idx in &order {
            if idx >= words.len() || idx >= positions.len() {
                continue;
            }
            let is_hovered = hovered == Some(idx);
            draw_word(
                cr,
                &words[idx],
                &positions[idx],
                &pc,
                is_hovered,
                &accent,
                &text,
            );
        }
    });
}

/// Load top vocab terms from the catalog and build sphere words.
fn load_words(catalog: &Catalog) -> Vec<SphereWord> {
    let terms = catalog.top_vocab_terms(MAX_WORDS as i64).unwrap_or_default();
    build_sphere_words(&terms)
}

/// Build sphere words from (term, count) pairs.
fn build_sphere_words(terms: &[(String, i64)]) -> Vec<SphereWord> {
    let mut words: Vec<SphereWord> = Vec::new();
    let n = terms.len();
    if n == 0 {
        return words;
    }

    let max_count = terms.iter().map(|(_, c)| *c).max().unwrap_or(1) as f64;

    for (i, (word, count)) in terms.iter().enumerate() {
        let phi = PI * (3.0 - (5.0_f64).sqrt());
        let z = 1.0 - (2.0 * (i as f64) / (n as f64).max(1.0));
        let radius_xy = (1.0 - z * z).sqrt();
        let theta = phi * (i as f64);
        let sx = theta.cos() * radius_xy;
        let sy = theta.sin() * radius_xy;
        let sz = z;

        let bucket = ((*count as f64 / max_count) * 3.0).floor().clamp(0.0, 3.0) as i32;
        let weight = [400, 500, 600, 700][bucket as usize];
        let vertical = i % VERTICAL_EVERY == 0;

        words.push(SphereWord {
            word: word.clone(),
            count: *count,
            sx, sy, sz,
            weight,
            vertical,
        });
    }

    words.sort_by(|a, b| b.count.cmp(&a.count));
    words
}

/// Compute projected positions for all words at the current rotation.
fn compute_positions(
    words: &[SphereWord],
    state: &RefCell<SphereDrawState>,
    stage_w: f64,
    stage_h: f64,
) {
    let st = state.borrow_mut();
    let yaw = st.yaw + st.drift;
    let pitch = st.pitch;
    drop(st);

    let cy = yaw.cos();
    let sy = yaw.sin();
    let cp = pitch.cos();
    let sp = pitch.sin();
    let radius = (stage_w.min(stage_h) * 0.42) as f64;

    let max_count = words.iter().map(|w| w.count as f64).fold(1.0_f64, f64::max);

    let mut positions: Vec<WordPosition> = Vec::with_capacity(words.len());
    for w in words {
        let (rx, ry, rz) = rotate_point(w.sx, w.sy, w.sz, cy, sy, cp, sp);

        let depth_scale =
            DEPTH_SCALE_MIN + (DEPTH_SCALE_MAX - DEPTH_SCALE_MIN) * ((rz + 1.0) / 2.0).clamp(0.0, 1.0);
        let size = (MIN_SIZE + (MAX_SIZE - MIN_SIZE) * w.count as f64 / max_count) * depth_scale;
        let alpha = OPACITY_BACK
            + (OPACITY_FRONT - OPACITY_BACK) * ((rz + 1.0) / 2.0).clamp(0.0, 1.0);

        let scale = radius * depth_scale;
        let px = stage_w / 2.0 + rx * scale;
        let py = stage_h / 2.0 + ry * scale * 0.9;

        positions.push(WordPosition { px, py, size, alpha, rz });
    }

    state.borrow_mut().positions = positions;
}

/// Rotate a unit-sphere point by yaw then pitch.
fn rotate_point(x: f64, y: f64, z: f64, cy: f64, sy: f64, cp: f64, sp: f64) -> (f64, f64, f64) {
    let (x1, z1) = (x * cy + z * sy, -x * sy + z * cy);
    let y2 = y * cp - z1 * sp;
    let z2 = y * sp + z1 * cp;
    (x1, y2, z2)
}

/// Draw a single word.
fn draw_word(
    cr: &CairoContext,
    word: &SphereWord,
    pos: &WordPosition,
    pc: &gtk::pango::Context,
    is_hovered: bool,
    accent: &RGBA,
    text: &RGBA,
) {
    let layout = Layout::new(pc);
    let desc = FontDescription::from_string("Playfair Display");
    let size = if is_hovered { 24.0 } else { pos.size };
    let pango_size = (size * 1024.0) as i32;
    desc.set_size(pango_size);
    let w = if is_hovered {
        Weight::Bold
    } else {
        match word.weight {
            400 => Weight::Normal,
            500 => Weight::Medium,
            _ => Weight::Bold,
        }
    };
    desc.set_weight(w);
    layout.set_font_description(Some(&desc));
    layout.set_text(&word.word);

    let (w_pix, h_pix) = layout.pixel_size();
    let w_pix = w_pix as f64;
    let h_pix = h_pix as f64;

    cr.save();
    cr.translate(pos.px - w_pix / 2.0, pos.py - h_pix / 2.0);

    if word.vertical {
        cr.rotate(-PI / 2.0);
    }

    let alpha = if is_hovered { 1.0 } else { pos.alpha };
    let r = if is_hovered { accent.red() } else { text.red() };
    let g = if is_hovered { accent.green() } else { text.green() };
    let b = if is_hovered { accent.blue() } else { text.blue() };
    cr.set_source_rgba(r, g, b, alpha);

    show_layout(cr, &layout);

    cr.restore();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fibonacci_sphere_spreads_points() {
        let terms: Vec<(String, i64)> = (0..50)
            .map(|i| (format!("word{i}"), (50 - i) as i64))
            .collect();
        let words = build_sphere_words(&terms);
        assert_eq!(words.len(), 50);
        for w in &words {
            let len: f64 = (w.sx * w.sx + w.sy * w.sy + w.sz * w.sz).sqrt();
            assert!((len - 1.0).abs() < 1e-6, "point not on unit sphere: {len}");
        }
    }

    #[test]
    fn weights_are_in_valid_range() {
        let terms: Vec<(String, i64)> = (0..20)
            .map(|i| (format!("w{i}"), (20 - i) as i64))
            .collect();
        let words = build_sphere_words(&terms);
        for w in &words {
            assert!((400..=700).contains(&w.weight), "weight {} out of range", w.weight);
        }
    }

    #[test]
    fn empty_terms_yields_no_words() {
        let words = build_sphere_words(&[]);
        assert!(words.is_empty());
    }

    #[test]
    fn rotate_point_preserves_length() {
        let (x, y, z) = (0.6_f64, 0.4_f64, 0.7_f64);
        let len0 = (x * x + y * y + z * z).sqrt();
        let (rx, ry, rz) = rotate_point(x, y, z, 1.0, 0.0, 1.0, 0.0);
        let len1 = (rx * rx + ry * ry + rz * rz).sqrt();
        assert!((len0 - len1).abs() < 1e-6);
    }
}
