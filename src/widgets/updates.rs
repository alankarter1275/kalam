//! The Updates timeline on Home (design-system §62, decision 189).
//!
//! Not `super::timeline`, which is §15's series-status list: 64-high rows, 24
//! nodes, an accent run through the part already read. §62 is a different
//! element — 56-high rows, a 2px `border` line, 10 nodes that are either
//! filled `accent` (new) or a 1.5px `text-2` ring on `bg` (seen), four rows
//! visible under a 48 fade, scrolling with no bar of its own.
//!
//! The gutter is painted per row the way §15's is, so the line stays
//! continuous while the rows scroll: each row draws its own 2px segment and
//! its own node.
//!
//! The content is a dummy (B21 supersedes B15's "draw nothing until there is
//! a source"). Online sources are off until Part 2 (B4), so Home draws the
//! element at its real size and shape with invented rows; the feed replaces
//! the rows and nothing else about the element moves.
use super::paint::{color_probe, probe_color, set_source};
use gtk::prelude::*;

/// §62: rows 56 high, nodes 10 across, four rows visible, a 48 fade below
/// them.
const ROW_HEIGHT: i32 = 56;
const NODE_DIAMETER: f64 = 10.0;
const VISIBLE_ROWS: i32 = 4;
const FADE_HEIGHT: i32 = 48;
/// Room for the 2px line with the node centred on it.
const GUTTER_WIDTH: i32 = 14;

/// One row of §62.
pub struct UpdateEntry {
    /// The title, drawn at 13 with weight 600.
    pub title: String,
    /// The Meta line under the title.
    pub detail: String,
    /// The time at the row's right.
    pub time: String,
    /// `true` fills the node in accent (new); `false` draws the 1.5px ring on
    /// `bg` (seen).
    pub fresh: bool,
}

/// Build the block: §62's Section heading with decision 189's "Checked N
/// hours ago" at the right, then the rows and the fade.
pub fn updates(entries: &[UpdateEntry], checked: &str) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 8);
    root.add_css_class("kalam-updates");

    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let title = gtk::Label::new(Some("Updates"));
    title.add_css_class("kalam-home-heading");
    title.set_halign(gtk::Align::Start);
    let meta = gtk::Label::new(Some(checked));
    meta.add_css_class("kalam-muted");
    meta.set_hexpand(true);
    meta.set_halign(gtk::Align::End);
    heading.append(&title);
    heading.append(&meta);
    root.append(&heading);

    let rows = gtk::Box::new(gtk::Orientation::Vertical, 0);
    rows.add_css_class("kalam-updates-list");
    for entry in entries {
        rows.append(&row(entry));
    }

    // §62: it scrolls without a visible bar. `External` is GTK's word for "no
    // scrollbar here", and the fade is what tells the eye the list continues.
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&rows));
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroll.set_vscrollbar_policy(gtk::PolicyType::External);

    let stack = gtk::Overlay::new();
    stack.set_child(Some(&scroll));
    stack.set_height_request(VISIBLE_ROWS * ROW_HEIGHT);
    let fade = gtk::Box::new(gtk::Orientation::Vertical, 0);
    fade.add_css_class("kalam-updates-fade");
    fade.set_height_request(FADE_HEIGHT);
    fade.set_valign(gtk::Align::End);
    // The fade is paint, not a target: it must not swallow the rows' hover or
    // their clicks.
    fade.set_can_target(false);
    stack.add_overlay(&fade);
    root.append(&stack);
    root
}

/// One 56-high row: the painted gutter, the title and its Meta line, then the
/// time at the right.
fn row(entry: &UpdateEntry) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("kalam-updates-row");
    row.set_size_request(-1, ROW_HEIGHT);
    row.append(&gutter(entry.fresh));

    let copy = gtk::Box::new(gtk::Orientation::Vertical, 2);
    copy.set_hexpand(true);
    copy.set_valign(gtk::Align::Center);

    let title = gtk::Label::new(Some(entry.title.as_str()));
    title.add_css_class("kalam-updates-title");
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let detail = gtk::Label::new(Some(entry.detail.as_str()));
    detail.add_css_class("kalam-muted");
    detail.set_halign(gtk::Align::Start);
    detail.set_xalign(0.0);
    detail.set_ellipsize(gtk::pango::EllipsizeMode::End);
    copy.append(&title);
    copy.append(&detail);
    row.append(&copy);

    let time = gtk::Label::new(Some(entry.time.as_str()));
    time.add_css_class("kalam-muted");
    time.set_valign(gtk::Align::Center);
    row.append(&time);
    row
}

/// The 2px line and this row's node, painted rather than assembled from
/// boxes: the node is either a filled disc or a ring, and the line has to run
/// the full row height so neighbouring rows read as one line.
fn gutter(fresh: bool) -> gtk::Box {
    let accent_probe = color_probe("kalam-ui-token-accent");
    let line_probe = color_probe("kalam-ui-token-border");
    let ring_probe = color_probe("kalam-ui-token-text-dim");
    let bg_probe = color_probe("kalam-ui-token-bg");
    let accent = accent_probe.clone();
    let line = line_probe.clone();
    let ring = ring_probe.clone();
    let bg = bg_probe.clone();

    let area = gtk::DrawingArea::new();
    area.set_content_width(GUTTER_WIDTH);
    area.set_content_height(ROW_HEIGHT);
    area.set_size_request(GUTTER_WIDTH, ROW_HEIGHT);
    area.set_draw_func(move |_area, cr, width, height| {
        if width <= 0 || height <= 0 {
            return;
        }
        let centre_x = width as f64 / 2.0;
        let centre_y = height as f64 / 2.0;
        let radius = NODE_DIAMETER / 2.0;
        let turn = std::f64::consts::TAU;

        // The line runs the whole row, so stacked rows join into one 2px line.
        set_source(cr, probe_color(&line), 1.0);
        cr.rectangle(centre_x - 1.0, 0.0, 2.0, height as f64);
        let _ = cr.fill();

        if fresh {
            set_source(cr, probe_color(&accent), 1.0);
            cr.arc(centre_x, centre_y, radius, 0.0, turn);
            let _ = cr.fill();
        } else {
            // Seen: a 1.5px `text-2` ring on `bg`, so the line does not show
            // through the middle of the node.
            set_source(cr, probe_color(&bg), 1.0);
            cr.arc(centre_x, centre_y, radius, 0.0, turn);
            let _ = cr.fill();
            set_source(cr, probe_color(&ring), 1.0);
            cr.set_line_width(1.5);
            cr.arc(centre_x, centre_y, radius - 0.75, 0.0, turn);
            let _ = cr.stroke();
        }
    });

    // The probes ride along because a drawing reads its colours from labels
    // in the same tree (`super::paint`); they are invisible and cost no room.
    let holder = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    holder.append(&area);
    holder.append(&accent_probe);
    holder.append(&line_probe);
    holder.append(&ring_probe);
    holder.append(&bg_probe);
    holder
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §62's numbers, pinned: 56-high rows, four of them visible, a 10 node
    /// and a 48 fade. Changing one of these is a change of design, not a
    /// refactor, so it should not happen quietly.
    #[test]
    fn the_block_is_four_rows_under_a_fade() {
        let visible = VISIBLE_ROWS * ROW_HEIGHT;
        assert_eq!(visible, 224, "four 56-high rows");
        assert_eq!(FADE_HEIGHT, 48, "§62's fade");
        // The node is painted inside its gutter, so §62's 10 has to stay
        // narrower than the gutter or the paint would spill under the copy.
        assert!(NODE_DIAMETER < f64::from(GUTTER_WIDTH), "§62's 10px node");
    }
}
