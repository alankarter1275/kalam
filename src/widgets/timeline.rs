//! Series-status timeline from design-system §15.
//!
//! The gutter is painted rather than assembled from boxes because the three
//! node states have different fills/rings and the 2px line changes colour
//! exactly between the read run and the upcoming run. The row content on
//! the right remains ordinary labels, so title and date copy wrap/ellipsize
//! with GTK's text machinery instead of Cairo's.

use super::paint::{color_probe, probe_color, set_source};
use gtk::prelude::*;

const ROW_HEIGHT: i32 = 64;
const NODE_DIAMETER: f64 = 24.0;

/// Where an entry sits in a series' reading sequence.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TimelineState {
    Read,
    Current,
    Upcoming,
}

/// Plain data for one timeline row. Page code decides what `right` means
/// (usually a date, or the current percentage); the component only applies
/// the approved type and colour roles.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimelineEntry {
    pub title: String,
    pub subtitle: Option<String>,
    pub right: Option<String>,
    pub state: TimelineState,
    pub bookmark_count: u32,
}

impl TimelineEntry {
    pub fn new(title: impl Into<String>, state: TimelineState) -> Self {
        Self {
            title: title.into(),
            subtitle: None,
            right: None,
            state,
            bookmark_count: 0,
        }
    }

    pub fn with_subtitle(mut self, subtitle: impl Into<String>) -> Self {
        self.subtitle = Some(subtitle.into());
        self
    }

    pub fn with_right(mut self, right: impl Into<String>) -> Self {
        self.right = Some(right.into());
        self
    }

    pub fn with_bookmarks(mut self, bookmark_count: u32) -> Self {
        self.bookmark_count = bookmark_count;
        self
    }
}

/// The two half-segments a row paints. `None` means "no segment" at the
/// first/last row; `Some(true)` is the accent link between read/current
/// nodes, and `Some(false)` is the border-toned upcoming link.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TimelineLinks {
    pub upper: Option<bool>,
    pub lower: Option<bool>,
}

/// Build a complete vertical timeline, deriving every gutter link from the
/// neighbouring entries.
pub fn timeline(entries: &[TimelineEntry]) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("kalam-ui-timeline");

    for (index, entry) in entries.iter().enumerate() {
        let upper = (index > 0).then(|| link_is_accent(entries[index - 1].state, entry.state));
        let lower = entries
            .get(index + 1)
            .map(|next| link_is_accent(entry.state, next.state));
        root.append(&timeline_row(entry, TimelineLinks { upper, lower }));
    }
    root
}

/// Build one 64px row with an explicitly supplied gutter. Series and
/// contents pages with filtered/reordered rows use this when the neighbours
/// in the data are not the neighbours on screen.
pub fn timeline_row(entry: &TimelineEntry, links: TimelineLinks) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    row.add_css_class("kalam-ui-timeline-row");
    row.set_size_request(-1, ROW_HEIGHT);
    row.set_valign(gtk::Align::Center);
    row.append(&timeline_gutter(entry.state, links));

    let copy = gtk::Box::new(gtk::Orientation::Vertical, 2);
    copy.set_hexpand(true);
    copy.set_valign(gtk::Align::Center);

    let title = gtk::Label::new(Some(&entry.title));
    title.add_css_class("kalam-ui-timeline-title");
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    copy.append(&title);

    if let Some(subtitle) = &entry.subtitle {
        let subtitle_label = gtk::Label::new(Some(subtitle));
        subtitle_label.add_css_class("kalam-ui-timeline-subtitle");
        subtitle_label.set_halign(gtk::Align::Start);
        subtitle_label.set_xalign(0.0);
        subtitle_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        copy.append(&subtitle_label);
    }
    row.append(&copy);

    if entry.bookmark_count > 0 {
        row.append(&bookmark_indicator(entry.bookmark_count));
    }

    if let Some(right) = &entry.right {
        let right_label = gtk::Label::new(Some(right));
        right_label.add_css_class("kalam-ui-timeline-right");
        if entry.state == TimelineState::Current {
            right_label.add_css_class("current");
        }
        right_label.set_halign(gtk::Align::End);
        right_label.set_valign(gtk::Align::Center);
        right_label.set_xalign(1.0);
        row.append(&right_label);
    }

    row
}

fn bookmark_indicator(count: u32) -> gtk::Box {
    let indicator = gtk::Box::new(
        gtk::Orientation::Horizontal,
        crate::style::spacing::XXS,
    );
    indicator.add_css_class("kalam-ui-timeline-bookmarks");
    indicator.set_valign(gtk::Align::Center);
    indicator.set_tooltip_text(Some(&format!(
        "{count} bookmark{}",
        if count == 1 { "" } else { "s" }
    )));

    let icon = gtk::Image::from_icon_name("bookmark-new-symbolic");
    icon.set_pixel_size(14);
    indicator.append(&icon);

    if count > 1 {
        let label = gtk::Label::new(Some(&count.to_string()));
        label.add_css_class("kalam-ui-timeline-bookmarks");
        indicator.append(&label);
    }
    indicator
}

fn timeline_gutter(state: TimelineState, links: TimelineLinks) -> gtk::Box {
    let accent_probe = color_probe("kalam-ui-token-accent");
    let fill_probe = color_probe("kalam-ui-token-accent-fill");
    let border_probe = color_probe("kalam-ui-token-border");
    let muted_probe = color_probe("kalam-ui-token-text-dim");
    let background_probe = color_probe("kalam-ui-token-bg");

    let accent_in_tree = accent_probe.clone();
    let fill_in_tree = fill_probe.clone();
    let border_in_tree = border_probe.clone();
    let muted_in_tree = muted_probe.clone();
    let background_in_tree = background_probe.clone();

    let area = gtk::DrawingArea::new();
    area.add_css_class("kalam-ui-timeline-gutter");
    area.set_content_width(NODE_DIAMETER as i32);
    area.set_content_height(ROW_HEIGHT);
    area.set_size_request(NODE_DIAMETER as i32, ROW_HEIGHT);
    area.set_draw_func(move |_area, cr, width, height| {
        if width <= 0 || height <= 0 {
            return;
        }

        let accent = probe_color(&accent_probe);
        let fill = probe_color(&fill_probe);
        let border = probe_color(&border_probe);
        let muted = probe_color(&muted_probe);
        let background = probe_color(&background_probe);
        let centre_x = width as f64 / 2.0;
        let centre_y = height as f64 / 2.0;
        let radius = NODE_DIAMETER / 2.0;

        if let Some(accent_link) = links.upper {
            draw_link(
                cr,
                centre_x,
                0.0,
                centre_y - radius,
                if accent_link { accent } else { border },
            );
        }
        if let Some(accent_link) = links.lower {
            draw_link(
                cr,
                centre_x,
                centre_y + radius,
                height as f64,
                if accent_link { accent } else { border },
            );
        }

        match state {
            TimelineState::Read => {
                set_source(cr, fill, 1.0);
                cr.arc(
                    centre_x,
                    centre_y,
                    radius,
                    0.0,
                    std::f64::consts::TAU,
                );
                let _ = cr.fill();

                cr.set_source_rgb(1.0, 1.0, 1.0);
                cr.set_line_width(2.0);
                cr.set_line_cap(gtk::cairo::LineCap::Round);
                cr.set_line_join(gtk::cairo::LineJoin::Round);
                cr.move_to(centre_x - 5.0, centre_y);
                cr.line_to(centre_x - 1.5, centre_y + 4.0);
                cr.line_to(centre_x + 6.0, centre_y - 4.0);
                let _ = cr.stroke();
            }
            TimelineState::Current => {
                set_source(cr, background, 1.0);
                cr.arc(
                    centre_x,
                    centre_y,
                    radius - 1.0,
                    0.0,
                    std::f64::consts::TAU,
                );
                let _ = cr.fill();

                set_source(cr, accent, 1.0);
                cr.set_line_width(2.0);
                cr.arc(
                    centre_x,
                    centre_y,
                    radius - 1.0,
                    0.0,
                    std::f64::consts::TAU,
                );
                let _ = cr.stroke();

                // A simple almond-and-pupil eye. Painting it avoids
                // depending on a symbolic eye glyph being present in every
                // icon theme and keeps it optically centred in the ring.
                cr.set_line_width(1.6);
                cr.move_to(centre_x - 6.0, centre_y);
                cr.curve_to(
                    centre_x - 3.0,
                    centre_y - 4.0,
                    centre_x + 3.0,
                    centre_y - 4.0,
                    centre_x + 6.0,
                    centre_y,
                );
                cr.curve_to(
                    centre_x + 3.0,
                    centre_y + 4.0,
                    centre_x - 3.0,
                    centre_y + 4.0,
                    centre_x - 6.0,
                    centre_y,
                );
                let _ = cr.stroke();
                cr.arc(centre_x, centre_y, 2.0, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
            }
            TimelineState::Upcoming => {
                set_source(cr, background, 1.0);
                cr.arc(
                    centre_x,
                    centre_y,
                    radius - 0.75,
                    0.0,
                    std::f64::consts::TAU,
                );
                let _ = cr.fill();

                set_source(cr, muted, 0.5);
                cr.set_line_width(1.5);
                cr.arc(
                    centre_x,
                    centre_y,
                    radius - 0.75,
                    0.0,
                    std::f64::consts::TAU,
                );
                let _ = cr.stroke();
            }
        }
    });

    let gutter = gtk::Box::new(gtk::Orientation::Vertical, 0);
    gutter.set_size_request(NODE_DIAMETER as i32, ROW_HEIGHT);
    gutter.set_valign(gtk::Align::Center);
    gutter.append(&area);
    gutter.append(&accent_in_tree);
    gutter.append(&fill_in_tree);
    gutter.append(&border_in_tree);
    gutter.append(&muted_in_tree);
    gutter.append(&background_in_tree);
    gutter
}

fn draw_link(
    cr: &gtk::cairo::Context,
    centre_x: f64,
    from_y: f64,
    to_y: f64,
    colour: gtk::gdk::RGBA,
) {
    if to_y <= from_y {
        return;
    }
    set_source(cr, colour, 1.0);
    cr.set_line_width(2.0);
    cr.move_to(centre_x, from_y);
    cr.line_to(centre_x, to_y);
    let _ = cr.stroke();
}

fn link_is_accent(before: TimelineState, after: TimelineState) -> bool {
    matches!(
        (before, after),
        (TimelineState::Read, TimelineState::Read)
            | (TimelineState::Read, TimelineState::Current)
    )
}

#[cfg(test)]
mod tests {
    use super::{link_is_accent, TimelineState};

    #[test]
    fn accent_links_cover_the_read_run_into_the_current_node() {
        assert!(link_is_accent(TimelineState::Read, TimelineState::Read));
        assert!(link_is_accent(
            TimelineState::Read,
            TimelineState::Current
        ));
        assert!(!link_is_accent(
            TimelineState::Current,
            TimelineState::Upcoming
        ));
        assert!(!link_is_accent(
            TimelineState::Upcoming,
            TimelineState::Upcoming
        ));
    }
}
