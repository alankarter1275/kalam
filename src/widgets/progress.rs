//! 3px linear and segmented progress indicators (§§10, 19).

use super::paint::{color_probe, probe_color, rounded_rect, set_source};
use gtk::prelude::*;

const SEGMENT_GAP: f64 = 4.0;
const SEGMENT_HEIGHT: f64 = 3.0;

fn normalized_fraction(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn segment_width(total_width: f64, count: usize) -> f64 {
    if count == 0 || total_width <= 0.0 {
        return 0.0;
    }
    let gaps = SEGMENT_GAP * count.saturating_sub(1) as f64;
    ((total_width - gaps) / count as f64).max(0.0)
}

/// The single 3px progress bar. GTK's widget supplies accessibility and
/// text-direction behaviour; the component class supplies the shape.
pub fn linear_progress(fraction: f64) -> gtk::ProgressBar {
    let bar = gtk::ProgressBar::new();
    bar.add_css_class("kalam-ui-progress");
    bar.set_fraction(normalized_fraction(fraction));
    bar.set_hexpand(true);
    bar
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SegmentState {
    Read,
    InProgress(f64),
    Unread,
    Missing,
}

/// One segment per item — series books, author works, and similar sets.
/// `width` is a request; the drawing itself always uses the real allocation.
///
/// The returned box wraps the drawing area plus its hidden colour probes
/// (see `paint.rs`); treat it as one widget.
pub fn segmented_progress(segments: &[SegmentState], width: i32) -> gtk::Box {
    let accent_probe = color_probe("kalam-ui-token-accent");
    let border_probe = color_probe("kalam-ui-token-border");
    let muted_probe = color_probe("kalam-ui-token-text-dim");

    let area = gtk::DrawingArea::new();
    area.add_css_class("kalam-ui-segmented-progress");
    area.set_content_width(width.max(1));
    area.set_content_height(7);
    area.set_size_request(width.max(1), 7);
    area.set_valign(gtk::Align::Center);
    area.set_hexpand(true);

    // Handles to the same probe widgets for the tree below; the originals
    // move into the draw closure. A GTK clone is another reference to the
    // same widget, not a second label.
    let accent_probe_in_tree = accent_probe.clone();
    let border_probe_in_tree = border_probe.clone();
    let muted_probe_in_tree = muted_probe.clone();

    let states = segments.to_vec();
    area.set_draw_func(move |_area, cr, width, height| {
        if states.is_empty() || width <= 0 || height <= 0 {
            return;
        }
        let accent = probe_color(&accent_probe);
        let border = probe_color(&border_probe);
        let muted = probe_color(&muted_probe);
        let segment_w = segment_width(width as f64, states.len());
        if segment_w <= 0.0 {
            return;
        }
        let y = ((height as f64 - SEGMENT_HEIGHT) / 2.0).max(0.0);

        for (index, state) in states.iter().enumerate() {
            let x = index as f64 * (segment_w + SEGMENT_GAP);
            match state {
                SegmentState::Read => {
                    set_source(cr, accent, 1.0);
                    rounded_rect(cr, x, y, segment_w, SEGMENT_HEIGHT, SEGMENT_HEIGHT / 2.0);
                    let _ = cr.fill();
                }
                SegmentState::InProgress(fraction) => {
                    set_source(cr, border, 1.0);
                    rounded_rect(cr, x, y, segment_w, SEGMENT_HEIGHT, SEGMENT_HEIGHT / 2.0);
                    let _ = cr.fill();

                    let filled = segment_w * normalized_fraction(*fraction);
                    if filled > 0.0 {
                        set_source(cr, accent, 1.0);
                        rounded_rect(cr, x, y, filled, SEGMENT_HEIGHT, SEGMENT_HEIGHT / 2.0);
                        let _ = cr.fill();
                    }
                }
                SegmentState::Unread => {
                    set_source(cr, border, 1.0);
                    rounded_rect(cr, x, y, segment_w, SEGMENT_HEIGHT, SEGMENT_HEIGHT / 2.0);
                    let _ = cr.fill();
                }
                SegmentState::Missing => {
                    set_source(cr, muted, 1.0);
                    cr.set_line_width(1.0);
                    cr.set_dash(&[3.0, 2.0], 0.0);
                    cr.move_to(x, y + SEGMENT_HEIGHT / 2.0);
                    cr.line_to(x + segment_w, y + SEGMENT_HEIGHT / 2.0);
                    let _ = cr.stroke();
                    cr.set_dash(&[], 0.0);
                }
            }
        }
    });

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.set_size_request(width.max(1), 7);
    root.set_halign(gtk::Align::Start);
    root.set_valign(gtk::Align::Center);
    root.append(&area);
    root.append(&accent_probe_in_tree);
    root.append(&border_probe_in_tree);
    root.append(&muted_probe_in_tree);
    root
}

#[cfg(test)]
mod tests {
    use super::{normalized_fraction, segment_width};

    #[test]
    fn fractions_are_clamped_and_non_finite_values_are_empty() {
        assert_eq!(normalized_fraction(-1.0), 0.0);
        assert_eq!(normalized_fraction(0.25), 0.25);
        assert_eq!(normalized_fraction(2.0), 1.0);
        assert_eq!(normalized_fraction(f64::NAN), 0.0);
        assert_eq!(normalized_fraction(f64::INFINITY), 0.0);
    }

    #[test]
    fn segment_width_accounts_for_gaps_without_going_negative() {
        assert_eq!(segment_width(220.0, 5), 40.8);
        assert_eq!(segment_width(4.0, 5), 0.0);
        assert_eq!(segment_width(220.0, 0), 0.0);
    }
}
