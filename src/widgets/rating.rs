//! Custom-drawn five-star rating in half-star steps (§16).
//!
//! The older glyph picker in `charts.rs` remains in use until the book
//! details phase migrates its call sites. This is the drawn version the
//! design asks for: half stars are painted by clipping the star path, not
//! by relying on a font containing a half-star glyph.

use super::paint::{color_probe, probe_color, rounded_rect, set_source};
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

const STAR_COUNT: usize = 5;
const STAR_SIZE: f64 = 20.0;
const STAR_GAP: f64 = 3.0;
const STAR_PITCH: f64 = STAR_SIZE + STAR_GAP;
const CONTENT_WIDTH: i32 = 112; // 5 × 20px stars + 4 × 3px gaps.
const CONTENT_HEIGHT: i32 = 28;

fn normalize_half_stars(value: u8) -> u8 {
    value.min((STAR_COUNT * 2) as u8)
}

fn value_from_x(x: f64, width: i32) -> u8 {
    if width <= 0 || !x.is_finite() {
        return 0;
    }
    let scaled = (x.max(0.0) / width as f64) * CONTENT_WIDTH as f64;
    let star = (scaled / STAR_PITCH).floor().clamp(0.0, (STAR_COUNT - 1) as f64) as usize;
    let offset = scaled - star as f64 * STAR_PITCH;
    if offset < STAR_SIZE / 2.0 {
        (star * 2 + 1) as u8
    } else {
        (star * 2 + 2) as u8
    }
}

fn rating_text(value: u8) -> String {
    if value == 0 {
        "Not rated yet".to_owned()
    } else {
        format!("{:.1} / 5", value as f64 / 2.0)
    }
}

fn star_path(cr: &gtk::cairo::Context, centre_x: f64, centre_y: f64) {
    let outer = 9.5;
    let inner = 3.8;
    for i in 0..10 {
        let radius = if i % 2 == 0 { outer } else { inner };
        let angle = -std::f64::consts::FRAC_PI_2 + i as f64 * std::f64::consts::PI / 5.0;
        let x = centre_x + radius * angle.cos();
        let y = centre_y + radius * angle.sin();
        if i == 0 {
            cr.move_to(x, y);
        } else {
            cr.line_to(x, y);
        }
    }
    cr.close_path();
}

#[derive(Clone)]
pub struct Rating {
    root: gtk::Box,
    area: gtk::DrawingArea,
    label: gtk::Label,
    value: Rc<Cell<u8>>,
    hover: Rc<Cell<Option<u8>>>,
}

impl Rating {
    pub fn widget(&self) -> gtk::Box {
        self.root.clone()
    }

    pub fn value(&self) -> u8 {
        self.value.get()
    }

    /// Mirror external state without emitting the change callback.
    pub fn set_value(&self, value: u8) {
        let value = normalize_half_stars(value);
        self.value.set(value);
        self.hover.set(None);
        self.label.set_label(&rating_text(value));
        self.area.queue_draw();
    }
}

pub fn rating_control(initial: u8, on_change: impl Fn(u8) + 'static) -> Rating {
    let value = Rc::new(Cell::new(normalize_half_stars(initial)));
    let hover = Rc::new(Cell::new(None));
    let on_change = Rc::new(on_change);

    let area = gtk::DrawingArea::new();
    area.add_css_class("kalam-ui-rating");
    area.set_content_width(CONTENT_WIDTH);
    area.set_content_height(CONTENT_HEIGHT);
    area.set_size_request(CONTENT_WIDTH, CONTENT_HEIGHT);
    area.set_can_focus(true);
    area.set_focusable(true);
    area.set_cursor_from_name(Some("pointer"));
    area.set_tooltip_text(Some(
        "Rate in half-star steps. Click the current rating to clear it; arrow keys also work.",
    ));

    let accent_probe = color_probe("kalam-ui-token-accent");
    let border_probe = color_probe("kalam-ui-token-border");
    // Same-widget handles for the tree below; the originals move into the
    // draw closure.
    let accent_probe_in_tree = accent_probe.clone();
    let border_probe_in_tree = border_probe.clone();
    {
        let value = value.clone();
        let hover = hover.clone();
        area.set_draw_func(move |area, cr, width, height| {
            if width <= 0 || height <= 0 {
                return;
            }
            let accent = probe_color(&accent_probe);
            let border = probe_color(&border_probe);
            let shown = hover.get().unwrap_or_else(|| value.get());
            let fill_alpha = if hover.get().is_some() { 0.6 } else { 1.0 };

            let scale_x = width as f64 / CONTENT_WIDTH as f64;
            let centre_y = height as f64 / 2.0;

            for star in 0..STAR_COUNT {
                let full_value = (star * 2 + 2) as u8;
                let half_value = full_value - 1;
                let centre_x = (star as f64 * STAR_PITCH + STAR_SIZE / 2.0) * scale_x;

                set_source(cr, border, 1.0);
                star_path(cr, centre_x, centre_y);
                let _ = cr.fill();

                if shown >= full_value {
                    set_source(cr, accent, fill_alpha);
                    star_path(cr, centre_x, centre_y);
                    let _ = cr.fill();
                } else if shown == half_value {
                    // Paint the full star through a left-half clip.
                    let _ = cr.save();
                    let left = (star as f64 * STAR_PITCH) * scale_x;
                    cr.rectangle(left, 0.0, STAR_SIZE * scale_x / 2.0, height as f64);
                    cr.clip();
                    set_source(cr, accent, fill_alpha);
                    star_path(cr, centre_x, centre_y);
                    let _ = cr.fill();
                    let _ = cr.restore();
                }
            }

            if area.has_focus() {
                set_source(cr, accent, 1.0);
                cr.set_line_width(2.0);
                rounded_rect(cr, 1.0, 1.0, width as f64 - 2.0, height as f64 - 2.0, 6.0);
                let _ = cr.stroke();
            }
        });
    }

    let label = gtk::Label::new(Some(&rating_text(value.get())));
    label.add_css_class("kalam-ui-rating-label");
    label.set_valign(gtk::Align::Center);

    let click = gtk::GestureClick::new();
    click.set_button(1);
    {
        let value = value.clone();
        let hover = hover.clone();
        let on_change = on_change.clone();
        let label = label.clone();
        click.connect_released(move |gesture, _, x, _| {
            let Some(widget) = gesture.widget() else {
                return;
            };
            let picked = value_from_x(x, widget.width());
            let next = if picked == value.get() { 0 } else { picked };
            value.set(next);
            hover.set(None);
            label.set_label(&rating_text(next));
            widget.queue_draw();
            on_change(next);
        });
    }
    area.add_controller(click);

    let motion = gtk::EventControllerMotion::new();
    {
        let hover = hover.clone();
        motion.connect_motion(move |controller, x, _| {
            let Some(widget) = controller.widget() else {
                return;
            };
            let next = Some(value_from_x(x, widget.width()));
            if hover.replace(next) != next {
                widget.queue_draw();
            }
        });
    }
    {
        let hover = hover.clone();
        motion.connect_leave(move |controller| {
            hover.set(None);
            if let Some(widget) = controller.widget() {
                widget.queue_draw();
            }
        });
    }
    area.add_controller(motion);

    let keys = gtk::EventControllerKey::new();
    {
        let value = value.clone();
        let hover = hover.clone();
        let on_change = on_change.clone();
        let label = label.clone();
        keys.connect_key_pressed(move |controller, key, _, _| {
            let current = value.get() as i16;
            let next = match key {
                gtk::gdk::Key::Right | gtk::gdk::Key::Up => current + 1,
                gtk::gdk::Key::Left | gtk::gdk::Key::Down => current - 1,
                gtk::gdk::Key::Home | gtk::gdk::Key::Delete | gtk::gdk::Key::BackSpace => 0,
                gtk::gdk::Key::End => (STAR_COUNT * 2) as i16,
                _ => return gtk::glib::Propagation::Proceed,
            }
            .clamp(0, (STAR_COUNT * 2) as i16) as u8;

            if next != value.get() {
                value.set(next);
                hover.set(None);
                label.set_label(&rating_text(next));
                if let Some(widget) = controller.widget() {
                    widget.queue_draw();
                }
                on_change(next);
            }
            gtk::glib::Propagation::Stop
        });
    }
    area.add_controller(keys);

    let root = gtk::Box::new(gtk::Orientation::Horizontal, crate::style::spacing::XS);
    root.append(&area);
    root.append(&label);
    root.append(&accent_probe_in_tree);
    root.append(&border_probe_in_tree);

    Rating {
        root,
        area,
        label,
        value,
        hover,
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_half_stars, rating_text, value_from_x};

    #[test]
    fn pointer_position_maps_to_half_star_steps() {
        assert_eq!(value_from_x(1.0, 112), 1);
        assert_eq!(value_from_x(15.0, 112), 2);
        assert_eq!(value_from_x(24.0, 112), 3);
        assert_eq!(value_from_x(111.0, 112), 10);
        assert_eq!(value_from_x(f64::NAN, 112), 0);
    }

    #[test]
    fn rating_values_are_bounded_and_labelled() {
        assert_eq!(normalize_half_stars(99), 10);
        assert_eq!(rating_text(0), "Not rated yet");
        assert_eq!(rating_text(7), "3.5 / 5");
    }
}
