//! Hover/focus slide-out pill from design-system §23 and decision 15.
//!
//! GTK cannot reproduce the web sketch with one negative margin and a
//! z-index: sibling controls must move aside, while the primary pill must
//! stay painted above the secondary pill's 24px tuck. The overlay therefore
//! has three layers: an expanding sizing row, the secondary pill aligned to
//! its end, and finally the primary aligned to its start. The revealer in
//! the sizing row is what pushes following controls; the visible pill only
//! rides the row's changing allocation.

use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

const HEIGHT: i32 = 44;
const TUCK_PX: i32 = 24;
const HOVER_DELAY_MS: u64 = 150;
const LEAVE_GRACE_MS: u64 = 300;
const TRANSITION_MS: u32 = 200;

#[derive(Clone)]
pub struct SlideOutPill {
    root: gtk::Overlay,
    revealer: gtk::Revealer,
    secondary: gtk::Button,
    revealed: Rc<Cell<bool>>,
    generation: Rc<Cell<u64>>,
}

impl SlideOutPill {
    pub fn widget(&self) -> gtk::Overlay {
        self.root.clone()
    }

    pub fn secondary_button(&self) -> gtk::Button {
        self.secondary.clone()
    }

    pub fn is_revealed(&self) -> bool {
        self.revealed.get()
    }

    pub fn reveal(&self) {
        next_generation(&self.generation);
        reveal_now(&self.revealer, &self.secondary, &self.revealed);
    }

    pub fn hide(&self) {
        let generation = next_generation(&self.generation);
        hide_now(
            &self.revealer,
            &self.secondary,
            &self.revealed,
            &self.generation,
            generation,
        );
    }

    pub fn set_secondary_sensitive(&self, sensitive: bool) {
        self.secondary.set_sensitive(sensitive);
    }
}

/// Pair an existing primary widget with a text pill that slides out to its
/// right. `primary` must not already have a parent.
pub fn slide_out_pill(
    primary: &impl IsA<gtk::Widget>,
    secondary_label: &str,
    on_secondary: impl Fn() + 'static,
) -> SlideOutPill {
    let primary_widget: gtk::Widget = primary.as_ref().clone();
    primary_widget.add_css_class("kalam-ui-slideout-primary");
    primary_widget.set_halign(gtk::Align::Start);
    primary_widget.set_valign(gtk::Align::Center);

    let secondary = gtk::Button::with_label(secondary_label);
    secondary.add_css_class("kalam-ui-slideout-pill");
    secondary.set_size_request(-1, HEIGHT);
    secondary.set_halign(gtk::Align::End);
    secondary.set_valign(gtk::Align::Center);
    secondary.set_can_focus(true);
    secondary.set_visible(false);
    secondary.connect_clicked(move |_| on_secondary());

    let primary_width = natural_width(&primary_widget);
    let secondary_width = natural_width(secondary.upcast_ref::<gtk::Widget>());
    let exposed_width = exposed_width(secondary_width);

    let primary_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    primary_spacer.set_size_request(primary_width, HEIGHT);

    let exposed_spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    exposed_spacer.set_size_request(exposed_width, HEIGHT);

    let revealer = gtk::Revealer::new();
    revealer.set_transition_type(gtk::RevealerTransitionType::SlideRight);
    revealer.set_transition_duration(TRANSITION_MS);
    revealer.set_reveal_child(false);
    revealer.set_child(Some(&exposed_spacer));

    let sizing_row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    sizing_row.set_size_request(primary_width, HEIGHT);
    sizing_row.append(&primary_spacer);
    sizing_row.append(&revealer);

    let root = gtk::Overlay::new();
    root.add_css_class("kalam-ui-slideout");
    root.set_overflow(gtk::Overflow::Hidden);
    root.set_halign(gtk::Align::Start);
    root.set_valign(gtk::Align::Center);
    root.set_child(Some(&sizing_row));
    root.add_overlay(&secondary);
    // Added last, therefore painted last: this is the tuck's "primary on
    // top" edge. The sizing row already reserved the primary's width.
    root.add_overlay(&primary_widget);

    let revealed = Rc::new(Cell::new(false));
    let hovered = Rc::new(Cell::new(false));
    let focused = Rc::new(Cell::new(false));
    let generation = Rc::new(Cell::new(0_u64));

    let motion = gtk::EventControllerMotion::new();
    {
        let hovered = hovered.clone();
        let focused = focused.clone();
        let revealed = revealed.clone();
        let generation = generation.clone();
        let revealer = revealer.clone();
        let secondary = secondary.clone();
        motion.connect_enter(move |_, _, _| {
            hovered.set(true);
            schedule_reveal(
                &hovered,
                &focused,
                &revealed,
                &generation,
                &revealer,
                &secondary,
            );
        });
    }
    {
        let hovered = hovered.clone();
        let focused = focused.clone();
        let revealed = revealed.clone();
        let generation = generation.clone();
        let revealer = revealer.clone();
        let secondary = secondary.clone();
        motion.connect_leave(move |_| {
            hovered.set(false);
            schedule_hide(
                &hovered,
                &focused,
                &revealed,
                &generation,
                &revealer,
                &secondary,
            );
        });
    }
    root.add_controller(motion);

    let focus = gtk::EventControllerFocus::new();
    {
        let hovered = hovered.clone();
        let focused = focused.clone();
        let revealed = revealed.clone();
        let generation = generation.clone();
        let revealer = revealer.clone();
        let secondary = secondary.clone();
        focus.connect_enter(move |_| {
            focused.set(true);
            schedule_reveal(
                &hovered,
                &focused,
                &revealed,
                &generation,
                &revealer,
                &secondary,
            );
        });
    }
    {
        let hovered = hovered.clone();
        let focused = focused.clone();
        let revealed = revealed.clone();
        let generation = generation.clone();
        let revealer = revealer.clone();
        let secondary = secondary.clone();
        focus.connect_leave(move |_| {
            focused.set(false);
            schedule_hide(
                &hovered,
                &focused,
                &revealed,
                &generation,
                &revealer,
                &secondary,
            );
        });
    }
    root.add_controller(focus);

    SlideOutPill {
        root,
        revealer,
        secondary,
        revealed,
        generation,
    }
}

fn natural_width(widget: &gtk::Widget) -> i32 {
    let (_, natural, _, _) = widget.measure(gtk::Orientation::Horizontal, -1);
    natural.max(1)
}

fn exposed_width(secondary_width: i32) -> i32 {
    (secondary_width - TUCK_PX).max(0)
}

fn next_generation(generation: &Rc<Cell<u64>>) -> u64 {
    let next = generation.get().wrapping_add(1);
    generation.set(next);
    next
}

fn schedule_reveal(
    hovered: &Rc<Cell<bool>>,
    focused: &Rc<Cell<bool>>,
    revealed: &Rc<Cell<bool>>,
    generation: &Rc<Cell<u64>>,
    revealer: &gtk::Revealer,
    secondary: &gtk::Button,
) {
    let scheduled_generation = next_generation(generation);
    if revealed.get() {
        return;
    }

    let hovered = hovered.clone();
    let focused = focused.clone();
    let revealed = revealed.clone();
    let generation = generation.clone();
    let revealer = revealer.clone();
    let secondary = secondary.clone();
    gtk::glib::timeout_add_local_once(Duration::from_millis(HOVER_DELAY_MS), move || {
        if generation.get() == scheduled_generation && (hovered.get() || focused.get()) {
            reveal_now(&revealer, &secondary, &revealed);
        }
    });
}

fn schedule_hide(
    hovered: &Rc<Cell<bool>>,
    focused: &Rc<Cell<bool>>,
    revealed: &Rc<Cell<bool>>,
    generation: &Rc<Cell<u64>>,
    revealer: &gtk::Revealer,
    secondary: &gtk::Button,
) {
    let scheduled_generation = next_generation(generation);
    if !revealed.get() {
        return;
    }

    let hovered = hovered.clone();
    let focused = focused.clone();
    let revealed = revealed.clone();
    let generation = generation.clone();
    let revealer = revealer.clone();
    let secondary = secondary.clone();
    gtk::glib::timeout_add_local_once(Duration::from_millis(LEAVE_GRACE_MS), move || {
        if generation.get() == scheduled_generation && !hovered.get() && !focused.get() {
            hide_now(
                &revealer,
                &secondary,
                &revealed,
                &generation,
                scheduled_generation,
            );
        }
    });
}

fn reveal_now(revealer: &gtk::Revealer, secondary: &gtk::Button, revealed: &Rc<Cell<bool>>) {
    revealed.set(true);
    secondary.set_visible(true);
    revealer.set_reveal_child(true);
}

fn hide_now(
    revealer: &gtk::Revealer,
    secondary: &gtk::Button,
    revealed: &Rc<Cell<bool>>,
    generation: &Rc<Cell<u64>>,
    scheduled_generation: u64,
) {
    revealed.set(false);
    revealer.set_reveal_child(false);

    // Keep the pill painted while the sizing row contracts. Making it
    // invisible only after the same 200ms transition prevents a one-frame
    // transparent gap at the right edge.
    let generation = generation.clone();
    let secondary = secondary.clone();
    gtk::glib::timeout_add_local_once(Duration::from_millis(TRANSITION_MS as u64), move || {
        if generation.get() == scheduled_generation {
            secondary.set_visible(false);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{exposed_width, TUCK_PX};

    #[test]
    fn only_the_secondary_pill_beyond_the_tuck_expands_the_row() {
        assert_eq!(exposed_width(148), 148 - TUCK_PX);
        assert_eq!(exposed_width(TUCK_PX), 0);
        assert_eq!(exposed_width(0), 0);
    }
}
