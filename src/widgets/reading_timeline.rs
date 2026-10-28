//! Reading timeline widget (§51).
//!
//! Cairo-drawn reading timeline: axis, lanes, session segments, Today line,
//! hit testing for hover and click, keyboard focus with Tab and Enter,
//! horizontal scroll by drag or scroll events.
//!
//! The open card is a Gtk.Revealer below the canvas; clicking a session
//! reveals it with that book's details.

use crate::db::{Catalog, TimelineLane, TimelineSession};
use crate::widgets::paint::{color_probe, probe_color, rounded_rect, set_source};
use gtk::prelude::*;
use relm4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

const LANE_HEIGHT: i32 = 56;
const LANE_GAP: i32 = 16;
const AXIS_WIDTH: i32 = 170;
const SEGMENT_HEIGHT: f64 = 7.0;
const LINE_THICKNESS: f64 = 1.5;
const TODAY_LINE_THICKNESS: f64 = 1.0;
const MIN_TIMELINE_WIDTH: i32 = 900;
const DEFAULT_HEIGHT: i32 = 320;
const SEGMENT_MIN_WIDTH: f64 = 6.0;
const SEGMENT_MAX_WIDTH: f64 = 40.0;
const HEADER_SPACE: f64 = 40.0; // space below axis labels before lanes start

const HIGHLIGHT_CSS_CLASSES: [&str; 5] = [
    "kalam_hl_yellow",
    "kalam_hl_green",
    "kalam_hl_blue",
    "kalam_hl_pink",
    "kalam_hl_orange",
];

/// Mutable drawing state shared between the Cairo closure and event handlers.
struct TimelineDrawState {
    scroll_offset: f64,
    hovered_book: Option<i64>,
}

#[derive(Debug)]
pub enum ReadingTimelineOut {
    OpenBook { book_id: i64 },
}

#[derive(Debug)]
pub enum ReadingTimelineMsg {
    SessionClicked { book_id: i64 },
}

pub struct ReadingTimelineModel {
    catalog: Arc<Catalog>,
    lanes: Vec<TimelineLane>,
    day_min: i64,
    day_max: i64,
    today_day: i64,
    x_scale: f64,
    open_book: Option<i64>,
    draw_state: Rc<RefCell<TimelineDrawState>>,
}

#[relm4::component(pub)]
impl Component for ReadingTimelineModel {
    type Init = Arc<Catalog>;
    type Input = ReadingTimelineMsg;
    type Output = ReadingTimelineOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            set_hexpand: true,
            set_vexpand: false,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_margin_top: 8,
                set_margin_bottom: 6,
                set_margin_start: 16,
                set_margin_end: 16,

                gtk::Label {
                    set_label: "Reading timeline";
                    add_css_class: "kalam-section-label";
                    set_halign: gtk::Align::Start;
                    set_hexpand: true;
                },
                gtk::Label {
                    set_label: "Show all →";
                    add_css_class: "kalam-show-all";
                    set_halign: gtk::Align::End;
                },
            },

            gtk::ScrolledWindow {
                set_hscrollbar_policy: gtk::PolicyType::Never;
                set_vscrollbar_policy: gtk::PolicyType::Never;
                set_hexpand: true;

                #[name = "canvas"]
                gtk::DrawingArea {
                    add_css_class: "kalam-reading-timeline";
                    set_hexpand: true;
                    set_vexpand: false;
                    set_content_height: DEFAULT_HEIGHT;
                    set_width_request: MIN_TIMELINE_WIDTH;
                    set_focusable: true;
                    set_cursor_from_name: Some("pointer");
                },
            },

            #[name = "card_revealer"]
            gtk::Revealer {
                set_reveal_child: false;
                set_transition_type: gtk::RevealerTransitionType::SlideDown;
                set_transition_duration: 200;
                set_margin_top: 6;
                set_margin_start: 16;
                set_margin_end: 16;
                set_hexpand: true;
            },
        }
    }

    fn init(
        catalog: Self::Init,
        _root: &Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let lanes = catalog.timeline_lanes(200).unwrap_or_default();

        let (day_min, day_max) = compute_day_range(&lanes);
        let today_day = today_day_num();
        let x_scale = if day_max > day_min {
            (MIN_TIMELINE_WIDTH - AXIS_WIDTH) as f64 / (day_max - day_min) as f64
        } else {
            1.0
        };

        let draw_state = Rc::new(RefCell::new(TimelineDrawState {
            scroll_offset: 0.0,
            hovered_book: None,
        }));

        let model = ReadingTimelineModel {
            catalog,
            lanes,
            day_min,
            day_max,
            today_day,
            x_scale,
            open_book: None,
            draw_state: draw_state.clone(),
        };
        let widgets = view_output!();

        setup_canvas(&widgets.canvas, &model, draw_state);
        setup_interactions(&widgets.canvas, &model, &sender, draw_state);

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            ReadingTimelineMsg::SessionClicked { book_id } => {
                self.open_book = Some(book_id);
                widgets.card_revealer.set_reveal_child(true);
                if let Some(lane) = self.lanes.iter().find(|l| l.book_id == book_id) {
                    let card = build_card(lane);
                    widgets.card_revealer.set_child(Some(&card));
                }
                sender.output(ReadingTimelineOut::OpenBook { book_id }).ok();
            }
        }
        self.update_view(widgets, sender);
    }
}

/// Set up the Cairo draw function on the canvas.
fn setup_canvas(
    canvas: &gtk::DrawingArea,
    model: &ReadingTimelineModel,
    draw_state: Rc<RefCell<TimelineDrawState>>,
) {
    // Colour probes must live in the same widget tree as the drawing.
    let accent_probe = color_probe("kalam-ui-token-accent");
    let border_probe = color_probe("kalam-ui-token-border");
    let text_probe = color_probe("kalam-ui-token-text");
    let text_dim_probe = color_probe("kalam-ui-token-text-dim");
    let bg_probe = color_probe("kalam-ui-token-bg");
    canvas.add_overlay(&accent_probe);
    canvas.add_overlay(&border_probe);
    canvas.add_overlay(&text_probe);
    canvas.add_overlay(&text_dim_probe);
    canvas.add_overlay(&bg_probe);

    // One probe per highlight colour for the five timeline line colours.
    let hl_probes: Vec<gtk::Label> = HIGHLIGHT_CSS_CLASSES
        .iter()
        .map(|cls| color_probe(cls))
        .collect();
    for probe in &hl_probes {
        canvas.add_overlay(probe);
    }

    let lanes = model.lanes.clone();
    let day_min = model.day_min;
    let day_max = model.day_max;
    let today_day = model.today_day;
    let x_scale = model.x_scale;
    let open_book = model.open_book;

    canvas.set_draw_func(move |_area, cr, width, height| {
        if width <= 0.0 || height <= 0.0 {
            return;
        }

        let accent = probe_color(&accent_probe);
        let border = probe_color(&border_probe);
        let text = probe_color(&text_probe);
        let text_dim = probe_color(&text_dim_probe);
        let bg = probe_color(&bg_probe);

        let hl_colours: Vec<gtk::gdk::RGBA> = hl_probes.iter().map(probe_color).collect();

        let state = draw_state.borrow();
        let offset = state.scroll_offset;
        let hovered_book = state.hovered_book;
        drop(state);

        draw_timeline(
            cr,
            width,
            height,
            &lanes,
            day_min,
            day_max,
            today_day,
            x_scale,
            offset,
            open_book,
            hovered_book,
            &accent,
            &border,
            &text,
            &text_dim,
            &bg,
            &hl_colours,
        );
    });
}

/// Attach gesture and key controllers for scroll, click and keyboard.
fn setup_interactions(
    canvas: &gtk::DrawingArea,
    model: &ReadingTimelineModel,
    sender: &ComponentSender<ReadingTimelineModel>,
    draw_state: Rc<RefCell<TimelineDrawState>>,
) {
    let lanes = model.lanes.clone();
    let day_min = model.day_min;
    let day_max = model.day_max;
    let x_scale = model.x_scale;

    // Click on a session → open its card.
    let click = gtk::GestureClick::new();
    click.set_button(1);
    let s = sender.clone();
    let ds = draw_state.clone();
    click.connect_released(move |gesture, _n_press, x, y| {
        let book_id = hit_test_lane(x, y, &lanes, day_min, day_max, x_scale);
        if book_id != 0 {
            ds.borrow_mut().hovered_book = Some(book_id);
            let widget = gesture.widget().unwrap();
            widget.queue_draw();
            s.input(ReadingTimelineMsg::SessionClicked { book_id }).ok();
        }
    });
    canvas.add_controller(click);

    // Motion for hover tracking.
    let motion = gtk::EventControllerMotion::new();
    let ds = draw_state.clone();
    motion.connect_motion(move |_controller, x, y| {
        let book_id = hit_test_lane(x, y, &lanes, day_min, day_max, x_scale);
        let mut state = ds.borrow_mut();
        if state.hovered_book != Some(book_id) {
            state.hovered_book = if book_id != 0 { Some(book_id) } else { None };
            drop(state);
            let widget = _controller.widget().unwrap();
            widget.queue_draw();
        }
    });
    canvas.add_controller(motion);

    // Horizontal drag to scroll.
    let drag = gtk::GestureDrag::new();
    let ds = draw_state.clone();
    drag.connect_update(move |_gesture, offset_x, _offset_y| {
        let mut state = ds.borrow_mut();
        state.scroll_offset += offset_x;
        let max_scroll = ((MIN_TIMELINE_WIDTH as f64) * 0.5).max(0.0);
        state.scroll_offset = state.scroll_offset.clamp(-max_scroll, 0.0);
        drop(state);
        let widget = _gesture.widget().unwrap();
        widget.queue_draw();
        _gesture.set_point(0, 0.0, 0.0);
    });
    canvas.add_controller(drag);

    // Keyboard focus: Enter activates the hovered session.
    let key = gtk::EventControllerKey::new();
    let s = sender.clone();
    let ds = draw_state.clone();
    key.connect_key_pressed(move |_controller, keyval, _keycode, _state| {
        use gdk::enums::key::*;
        let activated = matches!(keyval, Return | KP_Enter);
        if activated {
            let state = ds.borrow();
            if let Some(book_id) = state.hovered_book {
                drop(state);
                s.input(ReadingTimelineMsg::SessionClicked { book_id }).ok();
            } else {
                drop(state);
            }
        }
        gtk::Inhibit(false)
    });
    canvas.add_controller(key);
}

/// Convert a pointer coordinate to a book id (hit testing).
///
/// Maps the click point to a lane (by y) and then to a session on that lane
/// (by x). Returns `0` (invalid book id) when nothing is hit.
fn hit_test_lane(
    x: f64,
    y: f64,
    lanes: &[TimelineLane],
    day_min: i64,
    day_max: i64,
    x_scale: f64,
) -> i64 {
    let axis_x = AXIS_WIDTH as f64;
    let day_span = (day_max - day_min).max(1) as f64;

    for lane in lanes.iter() {
        let lane_index = lanes.iter().position(|l| l.book_id == lane.book_id).unwrap_or(0);
        let lane_y = (lane_index as f64) * (LANE_HEIGHT + LANE_GAP) as f64
            + LANE_GAP as f64
            + HEADER_SPACE;

        // Check if the click is within this lane's vertical band.
        if y < lane_y - 20.0 || y > lane_y + 20.0 {
            continue;
        }

        // Check each session segment on this lane.
        for session in &lane.sessions {
            if let Some(day) = date_to_day_num(&session.started_at[..10]) {
                let seg_x = axis_x + (day - day_min) as f64 * x_scale;
                let seg_width = (session.seconds as f64 / 86400.0 * x_scale)
                    .clamp(SEGMENT_MIN_WIDTH, SEGMENT_MAX_WIDTH);
                if (x - seg_x).abs() <= seg_width / 2.0 + 4.0 {
                    return lane.book_id;
                }
            }
        }
    }
    0
}

/// Compute the min and max day numbers across all sessions.
fn compute_day_range(lanes: &[TimelineLane]) -> (i64, i64) {
    let mut min: Option<i64> = None;
    let mut max: Option<i64> = None;
    for lane in lanes {
        for session in &lane.sessions {
            if let Some(day) = date_to_day_num(&session.started_at[..10]) {
                min = min.map(|m| m.min(day)).or(Some(day));
                max = max.map(|m| m.max(day)).or(Some(day));
            }
        }
    }
    match (min, max) {
        (Some(lo), Some(hi)) => (lo, hi),
        _ => (0, 1),
    }
}

/// Today's date as a day number since the Unix epoch.
fn today_day_num() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    (secs / 86400) as i64
}

/// Convert "YYYY-MM-DD" to days since 1970-01-01 (Howard Hinnant's algorithm).
fn date_to_day_num(date: &str) -> Option<i64> {
    let y: i64 = date.get(0..4)?.parse().ok()?;
    let m: i64 = date.get(5..7)?.parse().ok()?;
    let d: i64 = date.get(8..10)?.parse().ok()?;

    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400; // [0, 399]
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1; // [0, 365]
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy; // [0, 146096]
    Some(era * 146097 + doe - 719468)
}

/// Convert a day number to a short label ("Oct 28", "Today", etc.).
fn day_num_to_label(day: i64) -> String {
    let today = today_day_num();
    if day == today {
        return "Today".into();
    }
    let (_y, m, d) = civil_from_days(day);
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun",
        "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    format!("{} {}", MONTHS.get(m as usize - 1).copied().unwrap_or(""), d)
}

/// Inverse of date_to_day_num: days since epoch → (year, month, day).
fn civil_from_days(z: i64) -> (i32, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097; // [0, 146096]
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
    let mp = (5 * doy + 2) / 153; // [0, 11]
    let d = doy - (153 * mp + 2) / 5 + 1; // [1, 31]
    let m = if mp < 10 { mp + 3 } else { mp - 9 }; // [1, 12]
    (y as i32, m as u32, d as u32)
}

/// Draw the complete timeline on the Cairo context.
fn draw_timeline(
    cr: &gtk::cairo::Context,
    width: f64,
    height: f64,
    lanes: &[TimelineLane],
    day_min: i64,
    day_max: i64,
    today_day: i64,
    x_scale: f64,
    scroll_offset: f64,
    open_book: Option<i64>,
    hovered_book: Option<i64>,
    accent: &gtk::gdk::RGBA,
    border: &gtk::gdk::RGBA,
    text: &gtk::gdk::RGBA,
    text_dim: &gtk::gdk::RGBA,
    bg: &gtk::gdk::RGBA,
    hl_colours: &[gtk::gdk::RGBA],
) {
    // Background.
    set_source(cr, bg.clone(), 1.0);
    cr.paint();

    if lanes.is_empty() {
        return;
    }

    let axis_x = AXIS_WIDTH as f64 + scroll_offset;
    let plot_width = width - axis_x - scroll_offset;
    let day_span = (day_max - day_min).max(1) as f64;

    // ── Vertical date axis labels ─────────────────────────────────────
    cr.set_font_size(11.0);
    set_source(cr, text_dim.clone(), 1.0);

    // Draw day ticks at regular intervals.
    let tick_count = 8;
    for i in 0..=tick_count {
        let frac = i as f64 / tick_count as f64;
        let day = day_min as f64 + frac * day_span;
        let x = axis_x + frac * plot_width;
        let label = day_num_to_label(day as i64);

        cr.move_to(x, 18.0);
        cr.show_text(&label);
    }

    // ── Today vertical line ───────────────────────────────────────────
    if today_day >= day_min && today_day <= day_max {
        let today_x = axis_x + (today_day - day_min) as f64 * x_scale;
        set_source(cr, accent.clone(), 1.0);
        cr.set_line_width(TODAY_LINE_THICKNESS);
        cr.move_to(today_x, 30.0);
        cr.line_to(today_x, height);
        cr.stroke();
    }

    // ── Lanes ─────────────────────────────────────────────────────────
    for (lane_index, lane) in lanes.iter().enumerate() {
        let lane_y = (lane_index as f64) * (LANE_HEIGHT + LANE_GAP) as f64
            + LANE_GAP as f64
            + HEADER_SPACE;
        let hl_colour = &hl_colours[lane_index % hl_colours.len()];
        let is_open = open_book == Some(lane.book_id);
        let is_hovered = hovered_book == Some(lane.book_id);

        // Lane label (book title + authors) on the axis side.
        cr.set_font_size(12.0);
        set_source(cr, text.clone(), 1.0);
        cr.move_to(8.0, lane_y + SEGMENT_HEIGHT / 2.0 + 4.0);
        let title = if lane.title.len() > 28 {
            format!("{}…", &lane.title[..28])
        } else {
            lane.title.clone()
        };
        cr.show_text(&title);

        cr.set_font_size(10.0);
        set_source(cr, text_dim.clone(), 1.0);
        cr.move_to(8.0, lane_y + SEGMENT_HEIGHT / 2.0 + 15.0);
        let authors = if lane.authors.len() > 24 {
            format!("{}…", &lane.authors[..24])
        } else {
            lane.authors.clone()
        };
        cr.show_text(&authors);

        // Lane background when open or hovered.
        if is_open || is_hovered {
            let hover_bg = if is_open { accent } else { border };
            set_source(cr, hover_bg.clone(), if is_open { 0.15 } else { 0.08 });
            cr.rectangle(0.0, lane_y - 22.0, width, LANE_HEIGHT as f64);
            cr.fill();
        }

        // Lane line.
        let line_alpha = if is_open || is_hovered { 0.8 } else { 0.4 };
        set_source(cr, border.clone(), line_alpha);
        cr.set_line_width(LINE_THICKNESS);
        cr.move_to(axis_x, lane_y);
        cr.line_to(axis_x + plot_width, lane_y);
        cr.stroke();

        // Session segments on this lane.
        for session in &lane.sessions {
            if let Some(day) = date_to_day_num(&session.started_at[..10]) {
                let seg_x = axis_x + (day - day_min) as f64 * x_scale;
                let seg_width = (session.seconds as f64 / 86400.0 * x_scale)
                    .clamp(SEGMENT_MIN_WIDTH, SEGMENT_MAX_WIDTH);
                let seg_y = lane_y - SEGMENT_HEIGHT / 2.0;

                // Rounded rect for the segment.
                rounded_rect(
                    cr,
                    seg_x - seg_width / 2.0,
                    seg_y,
                    seg_width,
                    SEGMENT_HEIGHT,
                    SEGMENT_HEIGHT / 2.0,
                );
                let seg_alpha = if is_open || is_hovered { 0.9 } else { 0.6 };
                set_source(cr, hl_colour.clone(), seg_alpha);
                cr.fill();

                // Outline.
                set_source(cr, hl_colour.clone(), 1.0);
                cr.set_line_width(1.0);
                cr.stroke();
            }
        }

        // Progress indicator at end of lane.
        if lane.progress > 0 {
            let prog_x = axis_x + day_span * x_scale;
            set_source(cr, accent.clone(), 1.0);
            cr.set_line_width(2.0);
            cr.move_to(prog_x, lane_y - 8.0);
            cr.line_to(prog_x, lane_y + 8.0);
            cr.stroke();
        }
    }
}

/// Build the open card for a lane.
fn build_card(lane: &TimelineLane) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("kalam-timeline-card");
    card.set_margin_top(12);
    card.set_margin_bottom(12);
    card.set_margin_start(16);
    card.set_margin_end(16);
    card.set_hexpand(true);

    let title = gtk::Label::new(Some(&lane.title));
    title.add_css_class("kalam-timeline-card-title");
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    card.append(&title);

    if !lane.authors.is_empty() {
        let authors = gtk::Label::new(Some(&lane.authors));
        authors.add_css_class("kalam-timeline-card-meta");
        authors.set_halign(gtk::Align::Start);
        authors.set_xalign(0.0);
        authors.set_ellipsize(gtk::pango::EllipsizeMode::End);
        card.append(&authors);
    }

    let progress = gtk::Label::new(Some(&format!("Progress: {}%", lane.progress)));
    progress.add_css_class("kalam-timeline-card-meta");
    progress.set_halign(gtk::Align::Start);
    progress.set_xalign(0.0);
    card.append(&progress);

    if let Some(finished) = &lane.finished_at {
        let finished_label = gtk::Label::new(Some(&format!("Finished: {}", finished)));
        finished_label.add_css_class("kalam-timeline-card-meta");
        finished_label.set_halign(gtk::Align::Start);
        finished_label.set_xalign(0.0);
        card.append(&finished_label);
    }

    let sessions_label = gtk::Label::new(Some(&format!("Reading sessions: {}", lane.sessions.len())));
    sessions_label.add_css_class("kalam-timeline-card-meta");
    sessions_label.set_halign(gtk::Align::Start);
    sessions_label.set_xalign(0.0);
    card.append(&sessions_label);

    // Most recent session detail.
    if let Some(most_recent) = &lane.most_recent {
        let recent = gtk::Label::new(Some(&format!(
            "Last read: {} ({}s, {}%)",
            most_recent.started_at, most_recent.seconds, most_recent.end_pct
        )));
        recent.add_css_class("kalam-timeline-card-meta");
        recent.set_halign(gtk::Align::Start);
        recent.set_xalign(0.0);
        card.append(&recent);
    }

    card
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_to_day_num_round_trip() {
        let date = "2026-10-28";
        let day = date_to_day_num(date).expect("valid date");
        let (y, m, d) = civil_from_days(day);
        assert_eq!(y, 2026);
        assert_eq!(m, 10);
        assert_eq!(d, 28);
    }

    #[test]
    fn date_to_day_num_epoch() {
        assert_eq!(date_to_day_num("1970-01-01"), Some(0));
        assert_eq!(date_to_day_num("1970-01-02"), Some(1));
    }

    #[test]
    fn compute_day_range_empty() {
        let lanes: Vec<TimelineLane> = vec![];
        assert_eq!(compute_day_range(&lanes), (0, 1));
    }

    #[test]
    fn compute_day_range_single_session() {
        let lanes = vec![TimelineLane {
            book_id: 1,
            title: "Test".into(),
            authors: "Author".into(),
            progress: 50,
            finished_at: None,
            last_opened_at: None,
            sessions: vec![TimelineSession {
                id: 1,
                started_at: "2026-10-28T10:00:00Z".into(),
                seconds: 3600,
                end_pct: 25,
            }],
            most_recent: None,
        }];
        let (lo, hi) = compute_day_range(&lanes);
        let expected = date_to_day_num("2026-10-28").unwrap();
        assert_eq!(lo, expected);
        assert_eq!(hi, expected);
    }

    #[test]
    fn hit_test_lane_misses_outside_lanes() {
        let lanes = vec![TimelineLane {
            book_id: 1,
            title: "Test".into(),
            authors: "Author".into(),
            progress: 50,
            finished_at: None,
            last_opened_at: None,
            sessions: vec![TimelineSession {
                id: 1,
                started_at: "2026-10-28T10:00:00Z".into(),
                seconds: 3600,
                end_pct: 25,
            }],
            most_recent: None,
        }];
        let day = date_to_day_num("2026-10-28").unwrap();
        // Click far above all lanes.
        assert_eq!(hit_test_lane(100.0, 0.0, &lanes, day, day, 1.0), 0);
        // Click far below all lanes.
        assert_eq!(hit_test_lane(100.0, 10000.0, &lanes, day, day, 1.0), 0);
    }
}
