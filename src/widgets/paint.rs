//! Small Cairo helpers shared by the custom-drawn components.
//!
//! Colours still come from CSS, but not through `StyleContext`: GTK has
//! deprecated its named-colour lookup and this workspace builds with
//! warnings denied. A hidden label carrying a token class is the GTK4-safe
//! probe — drawing reads that label's resolved foreground colour with
//! `widget.color()`, the same mechanism `charts.rs` already uses.

use gtk::prelude::*;

/// Build an invisible label whose foreground colour resolves a CSS token.
/// The caller must put the probe in the same widget tree as the drawing it
/// feeds; otherwise GTK has no reason to style it.
pub fn color_probe(css_class: &str) -> gtk::Label {
    let probe = gtk::Label::new(None);
    probe.add_css_class(css_class);
    probe.set_visible(false);
    probe
}

/// Read a probe's resolved foreground colour.
pub fn probe_color(probe: &gtk::Label) -> gtk::gdk::RGBA {
    probe.color()
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
pub fn rounded_rect(
    cr: &gtk::cairo::Context,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    radius: f64,
) {
    let width = width.max(0.0);
    let height = height.max(0.0);
    if width == 0.0 || height == 0.0 {
        return;
    }
    let radius = radius.clamp(0.0, width.min(height) / 2.0);
    let right = x + width;
    let bottom = y + height;

    cr.new_sub_path();
    cr.arc(
        right - radius,
        y + radius,
        radius,
        -std::f64::consts::FRAC_PI_2,
        0.0,
    );
    cr.arc(
        right - radius,
        bottom - radius,
        radius,
        0.0,
        std::f64::consts::FRAC_PI_2,
    );
    cr.arc(
        x + radius,
        bottom - radius,
        radius,
        std::f64::consts::FRAC_PI_2,
        std::f64::consts::PI,
    );
    cr.arc(
        x + radius,
        y + radius,
        radius,
        std::f64::consts::PI,
        std::f64::consts::PI * 1.5,
    );
    cr.close_path();
}
