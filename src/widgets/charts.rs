//! Shared cairo-drawn charts.
//!
//! Kept out of any one page so the Library dashboard and the Analytics page
//! render identical visuals from a single implementation. Colours are read
//! back from CSS via `widget.color()`, so palette decisions stay in style.rs.

use crate::db::LibraryStats;
use gtk::prelude::*;

/// Books-added series, padded so a sparkline always has two points to draw.
pub fn monthly_series(stats: &LibraryStats) -> Vec<i64> {
    let v: Vec<i64> = stats.added_by_month.iter().map(|(_, n)| *n).collect();
    if v.len() < 2 {
        vec![0, v.first().copied().unwrap_or(0)]
    } else {
        v
    }
}

pub fn sparkline(series: &[i64], css_class: &str) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.add_css_class("kalam-sparkline");
    area.add_css_class(css_class);
    area.set_content_width(84);
    area.set_content_height(30);
    area.set_valign(gtk::Align::Center);

    let data: Vec<f64> = series.iter().map(|v| *v as f64).collect();
    area.set_draw_func(move |area, cr, w, h| {
        if data.len() < 2 {
            return;
        }
        // Colour comes from CSS so themes stay in one place.
        let colour = area.color();
        let max = data.iter().cloned().fold(0.0_f64, f64::max).max(1.0);
        let step = (w as f64) / ((data.len() - 1) as f64);
        let pad = 3.0;
        let usable = (h as f64) - pad * 2.0;

        let point = |i: usize, v: f64| -> (f64, f64) {
            let x = (i as f64) * step;
            let y = pad + usable - (v / max) * usable;
            (x, y)
        };

        cr.set_line_width(1.8);
        cr.set_source_rgba(
            colour.red() as f64,
            colour.green() as f64,
            colour.blue() as f64,
            1.0,
        );
        let (x0, y0) = point(0, data[0]);
        cr.move_to(x0, y0);
        for (i, v) in data.iter().enumerate().skip(1) {
            let (x, y) = point(i, *v);
            cr.line_to(x, y);
        }
        let _ = cr.stroke();

        // End dot on the newest point, like the mockup's sparklines.
        if let Some(&last) = data.last() {
            let (lx, ly) = point(data.len() - 1, last);
            cr.arc(lx, ly, 2.5, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        }
    });
    area
}

/// Full-width line chart with a soft fill under the curve.
pub fn line_chart(series: &[i64], labels: &[String]) -> gtk::Box {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 4);

    let area = gtk::DrawingArea::new();
    area.add_css_class("kalam-linechart");
    area.set_content_height(150);
    area.set_hexpand(true);

    let data: Vec<f64> = series.iter().map(|v| *v as f64).collect();
    area.set_draw_func(move |area, cr, w, h| {
        if data.len() < 2 {
            return;
        }
        let colour = area.color();
        let (r, g, b) = (
            colour.red() as f64,
            colour.green() as f64,
            colour.blue() as f64,
        );
        let max = data.iter().cloned().fold(0.0_f64, f64::max).max(1.0);
        let pad_x = 6.0;
        let pad_y = 10.0;
        let usable_w = (w as f64) - pad_x * 2.0;
        let usable_h = (h as f64) - pad_y * 2.0;
        let step = usable_w / ((data.len() - 1) as f64);

        let point = |i: usize, v: f64| -> (f64, f64) {
            (
                pad_x + (i as f64) * step,
                pad_y + usable_h - (v / max) * usable_h,
            )
        };

        // Faint horizontal guides.
        cr.set_line_width(1.0);
        cr.set_source_rgba(r, g, b, 0.10);
        for k in 0..=3 {
            let y = pad_y + usable_h * (k as f64 / 3.0);
            cr.move_to(pad_x, y);
            cr.line_to(pad_x + usable_w, y);
        }
        let _ = cr.stroke();

        // Fill under the curve.
        let (x0, y0) = point(0, data[0]);
        cr.move_to(x0, pad_y + usable_h);
        cr.line_to(x0, y0);
        for (i, v) in data.iter().enumerate().skip(1) {
            let (x, y) = point(i, *v);
            cr.line_to(x, y);
        }
        cr.line_to(pad_x + usable_w, pad_y + usable_h);
        cr.close_path();
        cr.set_source_rgba(r, g, b, 0.16);
        let _ = cr.fill();

        // The curve itself.
        cr.set_line_width(2.2);
        cr.set_source_rgba(r, g, b, 1.0);
        cr.move_to(x0, y0);
        for (i, v) in data.iter().enumerate().skip(1) {
            let (x, y) = point(i, *v);
            cr.line_to(x, y);
        }
        let _ = cr.stroke();

        // Emphasise the most recent point.
        if let Some(last) = data.last() {
            let (x, y) = point(data.len() - 1, *last);
            cr.arc(x, y, 3.5, 0.0, std::f64::consts::PI * 2.0);
            let _ = cr.fill();
        }
    });
    wrap.append(&area);

    // Sparse x-axis labels — every third day keeps them readable.
    let axis = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    axis.set_homogeneous(true);
    for (i, label) in labels.iter().enumerate() {
        let text = if i % 3 == 0 || i == labels.len() - 1 {
            label.as_str()
        } else {
            ""
        };
        let l = gtk::Label::new(Some(text));
        l.add_css_class("kalam-bar-label");
        axis.append(&l);
    }
    wrap.append(&axis);
    wrap
}

/// Read-only star display, e.g. `★★★½☆` plus the numeric value.
pub fn stars_label(half_stars: u8) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);

    let full = half_stars / 2;
    let half = half_stars % 2;
    let mut glyphs = String::new();
    for i in 0..5u8 {
        if i < full {
            glyphs.push('★');
        } else if i == full && half == 1 {
            glyphs.push('⯨');
        } else {
            glyphs.push('☆');
        }
    }

    let stars = gtk::Label::new(Some(&glyphs));
    stars.add_css_class("kalam-stars");
    row.append(&stars);

    if half_stars > 0 {
        let value = gtk::Label::new(Some(&format!("{:.1}/5", half_stars as f32 / 2.0)));
        value.add_css_class("kalam-stars-value");
        row.append(&value);
    }
    row
}

/// Interactive 0–5 star picker in half-star steps. `on_pick` receives
/// half-stars (0..=10); clicking the current value again clears the rating.
pub fn star_picker(current: u8, on_pick: impl Fn(u8) + 'static) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    row.add_css_class("kalam-star-picker");
    let on_pick = std::rc::Rc::new(on_pick);

    // Each star is a plain Label, *not* a Button: a Button owns an internal
    // click gesture that claims the event sequence, so a GestureClick added on
    // top of one never reports a release and the picker looks dead.
    // Left half of a star = half rating, right half = whole.
    for star in 1..=5u8 {
        let full_value = star * 2;
        let half_value = full_value - 1;

        let glyph = if current >= full_value {
            "\u{2605}" // ★
        } else if current == half_value {
            "\u{2bea}" // ⯪ half star
        } else {
            "\u{2606}" // ☆
        };

        let label = gtk::Label::new(Some(glyph));
        label.add_css_class("kalam-star-glyph");
        if current >= half_value {
            label.add_css_class("kalam-star-on");
        }
        label.set_cursor_from_name(Some("pointer"));
        label.set_tooltip_text(Some(
            "Click the left half of a star for a half rating, the right half for a whole one",
        ));

        let click = gtk::GestureClick::new();
        click.set_button(1);
        {
            let on_pick = on_pick.clone();
            let label_for_width = label.clone();
            click.connect_released(move |_, _, x, _| {
                let width = label_for_width.width().max(1) as f64;
                let value = if x < width / 2.0 {
                    half_value
                } else {
                    full_value
                };
                // Clicking the active value clears it, so a rating is undoable.
                on_pick(if current == value { 0 } else { value });
            });
        }
        label.add_controller(click);

        // Labels do not prelight on their own, so drive the hover class here.
        let motion = gtk::EventControllerMotion::new();
        {
            let label_hover = label.clone();
            motion.connect_enter(move |_, _, _| {
                label_hover.add_css_class("kalam-star-hover");
            });
        }
        {
            let label_hover = label.clone();
            motion.connect_leave(move |_| {
                label_hover.remove_css_class("kalam-star-hover");
            });
        }
        label.add_controller(motion);

        row.append(&label);
    }

    // Explicit clear, since discovering "click the same star again" is unlikely.
    let clear = gtk::Button::new();
    clear.set_child(Some(&crate::icons::symbolic_with_classes(
        "window-close-symbolic",
        14,
        &["kalam-inline-icon"],
    )));
    clear.add_css_class("kalam-star-clear");
    clear.set_tooltip_text(Some("Clear rating"));
    clear.set_visible(current > 0);
    {
        let on_pick = on_pick.clone();
        clear.connect_clicked(move |_| on_pick(0));
    }
    row.append(&clear);

    row
}

/// Seven-day streak strip: weekday initials with a flame on active days.
pub fn streak_strip(active: [bool; 7]) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.add_css_class("kalam-streak-strip");
    row.set_homogeneous(true);

    // `active` runs oldest → newest, so the last cell is today.
    let today_idx = 6;
    let labels = weekday_initials();

    for (i, on) in active.iter().enumerate() {
        let cell = gtk::Box::new(gtk::Orientation::Vertical, 4);
        cell.add_css_class("kalam-streak-day");
        if i == today_idx {
            cell.add_css_class("kalam-streak-today");
        }

        let name = gtk::Label::new(Some(labels[i]));
        name.add_css_class("kalam-streak-label");
        cell.append(&name);

        // A plain filled dot, not a color-emoji glyph: the old fire emoji
        // here sent Pango to Noto Color Emoji, whose bitmap font cairo
        // cannot scale (Pango-WARNING "failed to create cairo scaled
        // font", 2026-10-01).
        let flame = gtk::Label::new(Some("●"));
        flame.add_css_class("kalam-streak-flame");
        if !on {
            flame.add_css_class("kalam-streak-off");
        }
        cell.append(&flame);
        row.append(&cell);
    }
    row
}

/// Weekday initials for the last seven days, oldest first.
fn weekday_initials() -> [&'static str; 7] {
    const NAMES: [&str; 7] = ["M", "T", "W", "T", "F", "S", "S"];
    // 1970-01-01 was a Thursday, so day-of-week = (days + 3) mod 7 with Monday 0.
    let today = crate::db::days_since_epoch();
    let mut out = [""; 7];
    for (i, slot) in out.iter_mut().enumerate() {
        let day = today - (6 - i as i64);
        *slot = NAMES[(((day + 3) % 7 + 7) % 7) as usize];
    }
    out
}
