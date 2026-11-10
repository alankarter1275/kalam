//! The search bar (design-system §57).
//!
//! §57 draws two things: this bar, and the result box that hangs under it.
//! Only the bar ships here — the box has nothing to put in its groups until
//! search itself exists (B17), and the showcase is staged so the composition
//! can be judged a piece at a time (B21). The bar is not inert while it
//! waits: its underline answers focus exactly as §57 describes.
//!
//! It lives with the widgets rather than in the Home page because §58 hangs
//! the AO3 search off the same bar — one drawing of the element, two pages.
//!
//! The builder hands back the bar and the three pieces `super::search_box`
//! drives: the field the box drops open on, the "N results" count decision 197
//! puts at the bar's right end, and the picker that carries decision 200's
//! menu.
//!
//! home-spec §7's keys — `/` and Ctrl+F into the field — are the page's rather
//! than the bar's, because they have to work while the focus is somewhere else
//! on Home. Ctrl+F is also the shell's `SearchLibrary` binding, which jumps to
//! All Books (decision 155); B22 splits it by page, so Home claims the key in a
//! capture-phase controller of its own and the shell's binding still owns it
//! everywhere else.
use crate::rail_icons::{image_px, remember_fixed, Face};
use gtk::prelude::*;

/// §57: 520 wide, 40 high. home-spec §1 repeats both numbers and adds the 28
/// above the bar and the 56 from the rail — margins the *page* owns, not the
/// bar's, so they stay where the bar is placed.
pub const BAR_WIDTH: i32 = 520;
pub const BAR_HEIGHT: i32 = 40;
/// §57: magnifier 18, sliders icon 20, the picker itself 32 square.
const MAGNIFIER_PX: i32 = 18;
const SLIDERS_PX: i32 = 20;
const PICKER_PX: i32 = 32;

/// The bar, and the three pieces the box under it drives.
pub struct SearchBar {
    pub root: gtk::Box,
    pub entry: gtk::Entry,
    pub count: gtk::Label,
    pub options: gtk::Button,
}

/// Build the bar. `hint` is the placeholder §57 shows in `text-2` while the
/// field is empty.
pub fn search_bar(hint: &str) -> SearchBar {
    // The 10 between the magnifier, the field and the picker is the app's
    // usual gap: §57 names the sizes of the three but no space between them.
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    root.add_css_class("kalam-search-bar");
    root.set_size_request(BAR_WIDTH, BAR_HEIGHT);
    root.set_valign(gtk::Align::Center);

    let magnifier = image_px("search", Face::Idle, MAGNIFIER_PX);
    remember_fixed(&magnifier, "search", Face::Idle);
    root.append(&magnifier);

    let entry = gtk::Entry::new();
    entry.add_css_class("kalam-search-entry");
    entry.set_placeholder_text(Some(hint));
    entry.set_halign(gtk::Align::Fill);
    entry.set_hexpand(true);
    entry.set_valign(gtk::Align::Center);
    // §57's underline belongs to the bar, so the entry brings none of GTK's
    // own frame or background.
    entry.set_has_frame(false);
    // §57: the underline turns accent when the field has focus. GTK has no
    // `:focus-within` selector, so the class is switched here rather than
    // left to the stylesheet.
    entry.connect_has_focus_notify({
        let root = root.clone();
        move |entry| {
            if entry.has_focus() {
                root.add_css_class("focused");
            } else {
                root.remove_css_class("focused");
            }
        }
    });
    root.append(&entry);

    // "N results" (§57) in Meta at the right of the bar. Nothing to count
    // yet, so it stays hidden rather than saying "0 results" over an empty
    // field.
    let count = gtk::Label::new(None);
    count.add_css_class("kalam-search-count");
    count.add_css_class("kalam-muted");
    count.set_valign(gtk::Align::Center);
    count.set_visible(false);
    root.append(&count);

    // The option picker (§57): a bare 32-square button, radius 8, no outline,
    // `border` fill on hover. §17's menu attaches when the box does; the
    // tooltip is the one home-spec §7 asks every icon button to carry.
    let options = gtk::Button::new();
    options.add_css_class("kalam-search-options");
    options.set_size_request(PICKER_PX, PICKER_PX);
    options.set_valign(gtk::Align::Center);
    options.set_tooltip_text(Some("Search options"));
    let sliders = image_px("sliders-horizontal", Face::Idle, SLIDERS_PX);
    remember_fixed(&sliders, "sliders-horizontal", Face::Idle);
    options.set_child(Some(&sliders));
    root.append(&options);

    SearchBar {
        root,
        entry,
        count,
        options,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §57's bar is 520 x 40, with an 18 magnifier, a 20 sliders icon and a
    /// 32-square picker. home-spec §1 repeats the bar's two numbers, so they
    /// are pinned where both readings of the design can be checked at once.
    #[test]
    fn the_bar_carries_the_design_numbers() {
        assert_eq!((BAR_WIDTH, BAR_HEIGHT), (520, 40), "§57 and home-spec §1");
        assert_eq!(MAGNIFIER_PX, 18, "§57's magnifier");
        assert_eq!(SLIDERS_PX, 20, "§57's sliders icon");
        assert_eq!(PICKER_PX, 32, "§57's bare picker button");
    }
}
