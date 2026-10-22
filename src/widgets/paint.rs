//! Small Cairo helpers shared by the custom-drawn components.
//!
//! Colours are looked up from the widget's resolved CSS rather than passed
//! in as literals. That keeps the palette decision in `theme.rs`: a drawing
//! asks for the `@define-color` name it needs (`kalam_accent`,
//! `kalam_border`, …) and automatically follows palette and mode changes.

use gtk::prelude::*;

/// Return the CSS colour registered under `name`, or `area.color()` when the
/// name is missing. The fallback is a visible, theme-derived colour rather
/// than black, so a renamed token degrades instead of disappearing.
pub fn token_color(area: &gtk::DrawingArea, name: &str) -> gtk::gdk::RGBA {
    area.style_context().lookup_color(name).unwrap_or_else(|| area.color())
}

/// Set a Cairo source from a GDK colour, replacing its alpha.
pub fn set_source(cr: &gtk::cairo::Context, colour: gtk::gdk::RGBA, alpha: f64) {
    cr.set_source_rgba(
        colour.red() as f64,
        colour.green() as f64,
        colour.blue() as f64,
        alpha.clamp(0.0, 1.0),
    );
}

/// Add a rounded-rectangle path. Negative dimensions are clamped away so a
/// draw callback during an intermediate tiny allocation cannot hand Cairo an
/// inverted rectangle.
pub fn rounded_rect(cr: &gtk::cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    let width = width.max(0.0);
    let height = height.max(0.0);
    if width == 0.0 || height == 0.0 {
        return;
    }
    let radius = radius.clamp(0.0, width.min(height) / 2.0);
    let right = x + width;
    let bottom = y + height;

    cr.new_sub_path();
    cr.arc(right - radius, y + radius, radius, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.arc(right - radius, bottom - radius, radius, 0.0, std::f64::consts::FRAC_PI_2);
    cr.arc(x + radius, bottom - radius, radius, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    cr.arc(x + radius, y + radius, radius, std::f64::consts::PI, std::f64::consts::PI * 1.5);
    cr.close_path();
}
