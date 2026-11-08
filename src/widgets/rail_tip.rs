//! The rail's drawn tooltip (design-system §56).
//!
//! §56 wants a 32-high card to the right of the hovered item — `card` fill,
//! 1px `border`, radius 10, the name at 13px and its shortcut at 11px in the
//! Meta style — appearing after a short delay. GTK's own tooltip cannot draw
//! that: it is text only, in the toolkit's bubble, so the rail draws its own
//! card in the shell's root overlay and positions it from the hovered item.
//!
//! An overlay child rather than a `gtk::Popover` on purpose. A popover is its
//! own surface and takes a grab, which would put the rail's keys (Alt+1 to
//! Alt+7, build-decisions B7) behind that grab for as long as a tip was
//! showing. The overlay card takes no grab and cannot be a pointer target, so
//! it never catches the mouse it is describing.
//!
//! The card replaces the rail's native tooltips, so the bell's background-work
//! count (build-decisions B5, which the tasks disc's badge used to carry)
//! rides along as a third line — see [`NotificationPanel::tasks_line`]. That is
//! the one place the rail's tip grows past §56's 32.

use crate::shortcuts::{ShortcutAction, ShortcutBindings};
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// A card's third line, fetched when the card shows rather than when the rail is
/// built. Named rather than spelled out at every use: clippy reads the long
/// form as a very complex type.
type ExtraLine = Rc<dyn Fn() -> Option<String>>;

/// §56: "after a short delay", which is a taste number rather than a specified
/// one. GTK's own tooltip waits 500 ms; this is a little quicker because the
/// card also carries the shortcut, which is worth reading. Deliberately not
/// pinned by a test — asserting on a constant only says the constant is itself.
const DELAY_MS: u64 = 400;

/// Gap between the item and the card.
const GAP: f64 = 8.0;

thread_local! {
    /// The card the shell draws. There is one rail per process, but the shell
    /// has to hide the tip from places that hold no handle on it — a key-driven
    /// navigation, for instance, moves the page out from under a hovering
    /// pointer — so [`hide`] reaches it module-wide. The same trade
    /// `rail_icons` makes with its registry of drawn glyphs.
    static CARD: RefCell<Option<RailTip>> = const { RefCell::new(None) };
}

/// Hide the rail's tip, wherever the shell is when it needs to.
pub fn hide() {
    CARD.with(|slot| {
        if let Some(tip) = slot.borrow().as_ref() {
            tip.hide();
        }
    });
}

/// Where the card sits for one item: to the right of it, centred on it
/// vertically (§56). Pure, so the arithmetic is pinned by a test rather than by
/// a screenshot.
fn margins(item_x: f64, item_y: f64, item_w: f64, item_h: f64, card_h: f64) -> (i32, i32) {
    let start = (item_x + item_w + GAP).round() as i32;
    // A two-line card is taller than the 44px item it describes; centring still
    // reads as belonging to it, and the top is clamped so the card cannot leave
    // the window.
    let top = (item_y + (item_h - card_h) / 2.0).round().max(0.0) as i32;
    (start, top)
}

#[derive(Clone)]
pub struct RailTip {
    card: gtk::Box,
    name: gtk::Label,
    key: gtk::Label,
    extra: gtk::Label,
    /// The item the card is positioned against, so a show can be skipped once
    /// the pointer has moved on.
    host: gtk::Overlay,
    /// Read at show time, so a rebind in Settings is on the card the next time
    /// the pointer arrives.
    bindings: Rc<RefCell<ShortcutBindings>>,
    /// A hover that has not become a tip yet.
    pending: Rc<RefCell<Option<gtk::glib::SourceId>>>,
}

impl RailTip {
    /// Build the card, hidden, inside `host`.
    pub fn new(host: &gtk::Overlay, bindings: Rc<RefCell<ShortcutBindings>>) -> Self {
        let name = gtk::Label::new(Some(""));
        name.add_css_class("kalam-rail-tip-name");

        let key = gtk::Label::new(Some(""));
        key.add_css_class("kalam-rail-tip-key");
        key.set_visible(false);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_vexpand(true);
        row.set_valign(gtk::Align::Center);
        row.append(&name);
        row.append(&key);

        let extra = gtk::Label::new(Some(""));
        extra.add_css_class("kalam-rail-tip-extra");
        extra.set_halign(gtk::Align::Start);
        extra.set_visible(false);

        let card = gtk::Box::new(gtk::Orientation::Vertical, 2);
        card.add_css_class("kalam-rail-tip");
        card.set_halign(gtk::Align::Start);
        card.set_valign(gtk::Align::Start);
        // A tip that catches the pointer hides itself the moment the pointer
        // reaches it.
        card.set_can_target(false);
        card.set_visible(false);
        card.append(&row);
        card.append(&extra);
        host.add_overlay(&card);

        let tip = Self {
            card,
            name,
            key,
            extra,
            host: host.clone(),
            bindings,
            pending: Rc::new(RefCell::new(None)),
        };
        CARD.with(|slot| *slot.borrow_mut() = Some(tip.clone()));
        tip
    }

    /// Give `button` a tip: `name`, with the shortcut for `action` beside it.
    pub fn attach(&self, button: &gtk::Button, name: &str, action: Option<ShortcutAction>) {
        self.attach_with_extra(button, name, action, None);
    }

    /// As [`RailTip::attach`], with a third line fetched when the card shows.
    /// The bell uses it for the background-work count (B5); `None` keeps the
    /// card at §56's single line.
    pub fn attach_with_extra(
        &self,
        button: &gtk::Button,
        name: &str,
        action: Option<ShortcutAction>,
        extra: Option<ExtraLine>,
    ) {
        let motion = gtk::EventControllerMotion::new();

        let tip = self.clone();
        let item = button.clone();
        let shown = name.to_string();
        let extra_in = extra.clone();
        motion.connect_enter(move |_, _, _| {
            tip.cancel();
            // A second name, not a shadow: the timeout closure moves what it
            // is given, and the line storing its id still needs the original.
            let shown_tip = tip.clone();
            let item = item.clone();
            let shown = shown.clone();
            let extra_in = extra_in.clone();
            let id = gtk::glib::timeout_add_local_once(
                std::time::Duration::from_millis(DELAY_MS),
                move || {
                    // Clear the slot first: the source has fired, and removing
                    // a spent id makes glib complain loudly.
                    *shown_tip.pending.borrow_mut() = None;
                    shown_tip.show_for(&item, &shown, action, extra_in.as_ref());
                },
            );
            *tip.pending.borrow_mut() = Some(id);
        });

        let tip = self.clone();
        motion.connect_leave(move |_| {
            tip.cancel();
            tip.hide();
        });

        // Clicking navigates, and the rail's tip has no business following the
        // pointer onto the page it just opened.
        let tip = self.clone();
        button.connect_clicked(move |_| {
            tip.cancel();
            tip.hide();
        });

        button.add_controller(motion);
    }

    /// Drop a hover that has not become a tip yet.
    fn cancel(&self) {
        if let Some(id) = self.pending.borrow_mut().take() {
            id.remove();
        }
    }

    /// Hide the card.
    pub fn hide(&self) {
        self.cancel();
        self.card.set_visible(false);
    }

    fn show_for(
        &self,
        button: &gtk::Button,
        name: &str,
        action: Option<ShortcutAction>,
        extra: Option<&ExtraLine>,
    ) {
        self.name.set_text(name);

        // The binding is read now, not when the rail was built: the shortcuts
        // pane can rebind any of these while the shell lives.
        let key = action
            .map(|action| self.bindings.borrow().binding(action).display_label())
            .unwrap_or_default();
        self.key.set_text(&key);
        self.key.set_visible(!key.is_empty());

        let extra_text = extra.and_then(|f| f());
        self.extra.set_text(extra_text.as_deref().unwrap_or(""));
        self.extra.set_visible(extra_text.is_some());

        // Measure rather than allocate: the card has no height worth reading
        // until it has been shown once, and a tip that jumps a line lower on
        // its second appearance is a tip nobody trusts.
        let (_, card_h, _, _) = self.card.measure(gtk::Orientation::Vertical, -1);
        // `compute_bounds`, not `translate_coordinates`, which GTK deprecated
        // in 4.12. It gives the item's whole rectangle in the overlay's
        // coordinates in one call, so the card is placed against the item as
        // laid out rather than as requested.
        if let Some(rect) = button.compute_bounds(&self.host) {
            let (start, top) = margins(
                rect.x() as f64,
                rect.y() as f64,
                rect.width() as f64,
                rect.height() as f64,
                card_h as f64,
            );
            self.card.set_margin_start(start);
            self.card.set_margin_top(top);
        }
        self.card.set_visible(true);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §56: the card sits to the right of the item, centred on it. The rail's
    /// items are 44 discs and the card is 32 high, so it lands 6 above the
    /// item's top edge.
    #[test]
    fn the_card_sits_beside_the_item_and_centred_on_it() {
        let (start, top) = margins(12.0, 100.0, 44.0, 44.0, 32.0);
        assert_eq!(start, 64, "12 + 44 wide + 8 gap");
        assert_eq!(top, 106, "100 + (44 - 32) / 2");
    }

    /// The bell's card carries a third line while work is running (B5), which
    /// makes it taller than the item. Centring still belongs to the item, and
    /// the top is clamped so the card cannot be pushed off the window.
    #[test]
    fn a_taller_card_stays_centred_and_on_screen() {
        let (_, top) = margins(12.0, 100.0, 44.0, 44.0, 48.0);
        assert_eq!(top, 98, "100 + (44 - 48) / 2");

        let (_, top) = margins(12.0, 0.0, 44.0, 44.0, 48.0);
        assert_eq!(top, 0, "clamped at the top of the window");
    }
}
