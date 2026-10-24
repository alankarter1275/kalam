//! Icon-only reader selection chip from design-system §13.
//!
//! The EPUB and PDF readers currently build different selection surfaces.
//! This is their shared shell: five one-tap highlight colours, then Note and
//! Define. It returns domain actions and knows nothing about a selection's
//! locator, which reader produced it, or where the reader will place it.

use super::paint::{color_probe, probe_color, set_source};
use super::pill_button::icon_button;
use crate::db::HighlightColor;
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SelectionChipAction {
    Highlight(HighlightColor),
    Note,
    Define,
}

#[derive(Clone)]
pub struct SelectionChip {
    root: gtk::Box,
    selected: Rc<Cell<HighlightColor>>,
    colors: Vec<HighlightColor>,
    dots: Rc<RefCell<Vec<gtk::DrawingArea>>>,
}

impl SelectionChip {
    pub fn widget(&self) -> gtk::Box {
        self.root.clone()
    }

    pub fn selected_color(&self) -> HighlightColor {
        self.selected.get()
    }

    /// Mirror a reader's remembered colour without emitting an action.
    /// Colours this chip was not built with are ignored.
    pub fn set_selected_color(&self, color: HighlightColor) {
        if !self.colors.contains(&color) {
            return;
        }
        self.selected.set(color);
        redraw_dots(&self.dots);
    }
}

/// Build the approved five-colour chip.
pub fn selection_chip(
    on_action: impl Fn(SelectionChipAction) + 'static,
) -> SelectionChip {
    selection_chip_with_colors(
        HighlightColor::SOFT_FIVE,
        HighlightColor::Yellow,
        on_action,
    )
}

/// Build a chip with a caller-selected colour set. An empty colour slice
/// still yields the standard five: a selection surface with no highlight
/// action would be a dead end rather than a useful configuration.
pub fn selection_chip_with_colors(
    colors: &[HighlightColor],
    initial: HighlightColor,
    on_action: impl Fn(SelectionChipAction) + 'static,
) -> SelectionChip {
    let colors = if colors.is_empty() {
        HighlightColor::SOFT_FIVE.to_vec()
    } else {
        colors.to_vec()
    };
    let initial = if colors.contains(&initial) {
        initial
    } else {
        colors[0]
    };

    let selected = Rc::new(Cell::new(initial));
    let dots = Rc::new(RefCell::new(Vec::new()));
    let on_action: Rc<dyn Fn(SelectionChipAction)> = Rc::new(on_action);

    let root = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    root.add_css_class("kalam-ui-selection-chip");
    // Content-sized like the segmented controls; the chip must hug its
    // swatches and actions, not stretch to its parent's width.
    root.set_halign(gtk::Align::Start);
    root.set_valign(gtk::Align::Center);
    root.set_size_request(-1, crate::style::size::TOUCH_TARGET);
    root.set_valign(gtk::Align::Center);

    let ring_probe = color_probe("kalam-ui-token-text");
    let ring_probe_in_tree = ring_probe.clone();

    for color in &colors {
        let color = *color;
        let dot = gtk::DrawingArea::new();
        dot.add_css_class("kalam-ui-selection-dot");
        dot.set_content_width(20);
        dot.set_content_height(20);
        dot.set_size_request(20, 20);
        dot.set_halign(gtk::Align::Center);
        dot.set_valign(gtk::Align::Center);

        {
            let selected = selected.clone();
            let ring_probe = ring_probe.clone();
            dot.set_draw_func(move |_area, cr, width, height| {
                if width <= 0 || height <= 0 {
                    return;
                }
                let centre_x = width as f64 / 2.0;
                let centre_y = height as f64 / 2.0;
                let radius = width.min(height) as f64 / 2.0 - 1.0;
                let (red, green, blue, alpha) = highlight_rgba(color);
                cr.set_source_rgba(red, green, blue, alpha);
                cr.arc(centre_x, centre_y, radius, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();

                if selected.get() == color {
                    set_source(cr, probe_color(&ring_probe), 1.0);
                    cr.set_line_width(2.0);
                    cr.arc(
                        centre_x,
                        centre_y,
                        radius - 1.0,
                        0.0,
                        std::f64::consts::TAU,
                    );
                    let _ = cr.stroke();
                }
            });
        }
        dots.borrow_mut().push(dot.clone());

        let button = gtk::Button::new();
        button.add_css_class("kalam-ui-selection-color");
        button.set_size_request(
            crate::style::size::COMPACT,
            crate::style::size::COMPACT,
        );
        button.set_child(Some(&dot));
        button.set_tooltip_text(Some(&format!(
            "Highlight in {}",
            highlight_label(color)
        )));
        button.set_can_focus(true);

        let selected_for_click = selected.clone();
        let dots_for_click = dots.clone();
        let on_action_for_click = on_action.clone();
        button.connect_clicked(move |_| {
            selected_for_click.set(color);
            redraw_dots(&dots_for_click);
            on_action_for_click(SelectionChipAction::Highlight(color));
        });
        root.append(&button);
    }

    let divider = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    divider.add_css_class("kalam-ui-selection-divider");
    divider.set_size_request(1, 24);
    divider.set_valign(gtk::Align::Center);
    root.append(&divider);

    let on_action_for_note = on_action.clone();
    root.append(&icon_button(
        "document-edit-symbolic",
        "Add note",
        crate::style::size::COMPACT,
        false,
        move || on_action_for_note(SelectionChipAction::Note),
    ));

    root.append(&icon_button(
        "accessories-dictionary-symbolic",
        "Define",
        crate::style::size::COMPACT,
        false,
        move || on_action(SelectionChipAction::Define),
    ));

    root.append(&ring_probe_in_tree);

    SelectionChip {
        root,
        selected,
        colors,
        dots,
    }
}

fn redraw_dots(dots: &Rc<RefCell<Vec<gtk::DrawingArea>>>) {
    for dot in dots.borrow().iter() {
        dot.queue_draw();
    }
}

fn highlight_label(color: HighlightColor) -> &'static str {
    match color {
        HighlightColor::Yellow => "Yellow",
        HighlightColor::Green => "Green",
        HighlightColor::Blue => "Blue",
        HighlightColor::Pink => "Pink",
        HighlightColor::Orange => "Orange",
        HighlightColor::Underline => "Underline blue",
    }
}

/// RGBA values carry design decision 27's five hues — the same hues
/// `kalam_reader::HighlightColor::css` paints — at full strength, the
/// way the Notes tab's filter dots carry them: a swatch names its
/// colour, where the band on the page is the translucent thing. They
/// are fixed annotation colours, not theme tokens, and painting them
/// here keeps five one-off hex literals out of the application
/// stylesheet's ratchet.
fn highlight_rgba(color: HighlightColor) -> (f64, f64, f64, f64) {
    match color {
        HighlightColor::Yellow => (232.0 / 255.0, 185.0 / 255.0, 58.0 / 255.0, 1.0),
        HighlightColor::Green => (111.0 / 255.0, 179.0 / 255.0, 111.0 / 255.0, 1.0),
        HighlightColor::Blue => (91.0 / 255.0, 155.0 / 255.0, 213.0 / 255.0, 1.0),
        HighlightColor::Pink => (217.0 / 255.0, 123.0 / 255.0, 168.0 / 255.0, 1.0),
        HighlightColor::Orange => (224.0 / 255.0, 138.0 / 255.0, 75.0 / 255.0, 1.0),
        HighlightColor::Underline => (59.0 / 255.0, 130.0 / 255.0, 246.0 / 255.0, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use super::{highlight_label, highlight_rgba};
    use crate::db::HighlightColor;

    #[test]
    fn painted_dots_match_the_reader_highlight_palette() {
        // Decision 27's #E8B93A / #D97BA8, full strength like the Notes
        // tab's filter dots.
        assert_eq!(
            highlight_rgba(HighlightColor::Yellow),
            (232.0 / 255.0, 185.0 / 255.0, 58.0 / 255.0, 1.0)
        );
        assert_eq!(
            highlight_rgba(HighlightColor::Pink),
            (217.0 / 255.0, 123.0 / 255.0, 168.0 / 255.0, 1.0)
        );
        assert_eq!(highlight_label(HighlightColor::Blue), "Blue");
    }
}
