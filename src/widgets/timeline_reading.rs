//! §51 Reading timeline — Cairo-drawn lanes of reading activity.
//!
//! One horizontal line per book, laid out across a month axis. Colours cycle
//! through the five highlight colours. Clicking a line opens its card below
//! the axis. Drawn with Cairo so thin lines and rounded caps render crisply;
//! labels and buttons are ordinary GTK widgets.

use crate::db::{Catalog, TimelineLane};
use crate::widgets::paint::{color_probe, probe_color, set_source};
use gtk::cairo;
use gtk::prelude::*;
use relm4::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// The five highlight colours (annotation palette), cycled across lanes.
const LANE_COLOURS: [(f64, f64, f64); 5] = [
    (0.91, 0.75, 0.48), // yellow
    (0.60, 0.76, 0.47), // green
    (0.38, 0.69, 0.94), // blue
    (0.78, 0.47, 0.87), // pink
    (0.82, 0.60, 0.40), // orange
];

/// Geometry constants (§51).
const AXIS_HEIGHT: i32 = 30;      // axis row height
const FIRST_LANE_Y: i32 = 34;     // first lane y position
const LANE_GAP: i32 = 16;         // gap between lanes
const LANE_HEIGHT: f64 = 3.0;     // line thickness
const OPEN_LANE_HEIGHT: f64 = 5.0; // line thickness when open
const TODAY_LINE_WIDTH: f64 = 1.5; // today marker width
const CURRENT_DOT_RADIUS: f64 = 3.0; // current-read dot radius
const CONNECTOR_WIDTH: f64 = 1.5;  // connector below open line
const PAD_X: f64 = 40.0;          // left padding for axis
const PAD_RIGHT: f64 = 20.0;      // right padding
const MONTH_LABEL_PAD: i32 = 6;   // padding above axis for month labels

/// Messages from the timeline widget.
#[derive(Debug)]
pub enum TimelineReadingMsg {
    LaneSelected(i64),
    ExpandedToggled,
}

/// Pre-computed draw data for one lane.
#[derive(Debug, Clone)]
pub struct DrawLane {
    pub book_id: i64,
    pub title: String,
    pub authors: String,
    pub progress: u8,
    pub finished_at: Option<String>,
    /// Normalised x positions (0.0..1.0) of this lane's sessions, oldest first.
    pub x_positions: Vec<f64>,
    pub is_current: bool,
}

/// The timeline widget: header + Cairo drawing area.
pub struct TimelineReading {
    area: gtk::DrawingArea,
    accent_probe: gtk::Label,
    text_dim_probe: gtk::Label,
    border_probe: gtk::Label,
    lanes: RefCell<Vec<DrawLane>>,
    /// Normalised x positions of month tick starts (0.0..1.0) with labels.
    month_ticks: RefCell<Vec<(f64, String)>>,
    /// Today's normalised x position (0.0..1.0), if within range.
    today_norm: RefCell<Option<f64>>,
    /// Axis range as (earliest_iso, latest_iso) strings for normalisation.
    axis_range: RefCell<Option<(String, String)>>,
    open_lane: RefCell<Option<usize>>,
    hover_lane: RefCell<Option<usize>>,
    sender: RefCell<Option<ComponentSender<TimelineReadingModel>>>,
}

#[relm4::component(pub)]
impl Component for TimelineReadingModel {
    type Init = (Arc<Catalog>, Vec<TimelineLane>);
    type Input = TimelineReadingMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            set_hexpand: true,
            set_vexpand: false,

            // ── header (§51) ──────────────────────────────────────────
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Center,
                set_margin_top: 8,
                set_margin_bottom: 4,

                gtk::Label {
                    set_label: "Reading timeline",
                    add_css_class: "kalam-section-caption",
                    set_halign: gtk::Align::Start,
                    set_valign: gtk::Align::Center,
                },

                gtk::Box {
                    set_hexpand: true,
                },

                #[name = "all_impressions_btn"]
                gtk::Button {
                    set_label: "All impressions ›",
                    add_css_class: "kalam-timeline-link",
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::Center,
                    connect_clicked => TimelineReadingMsg::ExpandedToggled,
                },

                #[name = "expand_btn"]
                gtk::Button {
                    set_child: Some(&crate::icons::symbolic("pan-down-symbolic", 14)),
                    add_css_class: "kalam-timeline-ghost",
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::Center,
                    set_tooltip_text: Some("Expand timeline"),
                    connect_clicked => TimelineReadingMsg::ExpandedToggled,
                },
            },

            // ── axis + lanes ──────────────────────────────────────────
            #[name = "area"]
            gtk::DrawingArea {
                add_css_class: "kalam-timeline",
                set_content_height: AXIS_HEIGHT + FIRST_LANE_Y + LANE_GAP * 10,
                set_hexpand: true,
                set_vexpand: false,
                set_draw_func(move |_area, cr, width, height| {
                    draw_timeline(
                        cr, width, height,
                        &model.lanes, &model.month_ticks, &model.today_norm,
                        &model.axis_range,
                        model.open_lane.borrow(), model.hover_lane.borrow(),
                        &model.accent_probe, &model.text_dim_probe, &model.border_probe,
                    );
                }),

                gtk::EventControllerMotion {
                    connect_move(move |_area, x, y| {
                        let lanes = model.lanes.borrow();
                        let hovered = hit_test_lane(x as f64, y as f64, &lanes, AXIS_HEIGHT as f64);
                        let mut hover = model.hover_lane.borrow_mut();
                        if *hover != hovered {
                            *hover = hovered;
                            let _ = widgets.area.queue_draw();
                        }
                        gtk::glib::Propagation::Proceed
                    }),
                    connect_leave(move |_| {
                        let mut hover = model.hover_lane.borrow_mut();
                        if hover.is_some() {
                            *hover = None;
                            let _ = widgets.area.queue_draw();
                        }
                        gtk::glib::Propagation::Proceed
                    }),
                },

                gtk::GestureClick {
                    connect_released(move |_, x, y, _| {
                        let lanes = model.lanes.borrow();
                        let clicked = hit_test_lane(x as f64, y as f64, &lanes, AXIS_HEIGHT as f64);
                        if let Some(idx) = clicked {
                            let book_id = lanes[idx].book_id;
                            let mut open = model.open_lane.borrow_mut();
                            *open = if *open == Some(idx) { None } else { Some(idx) };
                            drop(open);
                            let _ = widgets.area.queue_draw();
                            if let Some(s) = model.sender.borrow().as_ref() {
                                s.input(TimelineReadingMsg::LaneSelected(book_id));
                            }
                        }
                        gtk::glib::Propagation::Stop
                    }),
                },
            },
        }
    }

    fn init(
        (catalog, lanes): (Arc<Catalog>, Vec<TimelineLane>),
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let accent_probe = color_probe("kalam-timeline-accent");
        let text_dim_probe = color_probe("kalam-timeline-text-dim");
        let border_probe = color_probe("kalam-timeline-border");

        let (draw_lanes, month_ticks, today_norm, axis_range) =
            compute_lane_geometry(&lanes);

        let model = TimelineReadingModel {
            area: gtk::DrawingArea::new(),
            accent_probe,
            text_dim_probe,
            border_probe,
            lanes: RefCell::new(draw_lanes),
            month_ticks: RefCell::new(month_ticks),
            today_norm: RefCell::new(today_norm),
            axis_range: RefCell::new(axis_range),
            open_lane: RefCell::new(None),
            hover_lane: RefCell::new(None),
            sender: RefCell::new(Some(sender)),
        };
        let widgets = view_output!();

        // Probes must be in the widget tree for GTK to resolve their colours.
        widgets.root().append(&accent_probe);
        widgets.root().append(&text_dim_probe);
        widgets.root().append(&border_probe);

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        _widgets: &mut Self::Widgets,
        msg: Self::Input,
        _sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            TimelineReadingMsg::LaneSelected(_) => {}
            TimelineReadingMsg::ExpandedToggled => {}
        }
        self.update_view(_widgets, _sender);
    }
}

/// Convert an ISO date string to a day number for linear mapping.
fn iso_to_days(d: &str) -> f64 {
    let parts: Vec<&str> = d.split('-').collect();
    if parts.len() >= 3 {
        let y: f64 = parts[0].parse().unwrap_or(1970.0);
        let m: f64 = parts[1].parse().unwrap_or(1.0);
        let day: f64 = parts[2].parse().unwrap_or(1.0);
        (y - 1970.0) * 365.25 + (m - 1.0) * 30.44 + day
    } else {
        0.0
    }
}

/// Compute lane geometry, month ticks, today position and axis range.
fn compute_lane_geometry(
    lanes: &[TimelineLane],
) -> (
    Vec<DrawLane>,
    Vec<(f64, String)>,
    Option<f64>,
    Option<(String, String)>,
) {
    let mut all_dates: Vec<String> = Vec::new();
    for lane in lanes {
        for s in &lane.sessions {
            all_dates.push(s.started_at.clone());
        }
    }

    if all_dates.is_empty() {
        return (Vec::new(), Vec::new(), None, None);
    }

    all_dates.sort();
    all_dates.dedup();
    let earliest = all_dates.first().cloned().unwrap_or_default();
    let latest = all_dates.last().cloned().unwrap_or_default();

    let earliest_days = iso_to_days(&earliest);
    let latest_days = iso_to_days(&latest);
    let range_days = (latest_days - earliest_days).max(1.0);

    // Today as days.
    let today_str = crate::db::chrono_like_now();
    let today = today_str.get(0..10).unwrap_or(&today_str);
    let today_days = iso_to_days(today);

    // Month ticks: first of each month between earliest and latest.
    let mut month_ticks: Vec<(f64, String)> = Vec::new();
    let e_parts: Vec<&str> = earliest.split('-').collect();
    if e_parts.len() >= 3 {
        let mut y: i32 = e_parts[0].parse().unwrap_or(1970);
        let mut m: i32 = e_parts[1].parse().unwrap_or(1);
        loop {
            let label = format!("{:04}-{:02}", y, m);
            let tick_days = iso_to_days(&format!("{:04}-{:02}-01", y, m));
            if tick_days > latest_days {
                break;
            }
            let norm = (tick_days - earliest_days) / range_days;
            // Month initial: first letter of month name.
            let initial = match m {
                1 => "J", 2 => "F", 3 => "M", 4 => "A", 5 => "M", 6 => "J",
                7 => "J", 8 => "A", 9 => "S", 10 => "O", 11 => "N", 12 => "D",
                _ => " ",
            }.to_string();
            month_ticks.push((norm, initial));
            if m == 12 {
                m = 1;
                y += 1;
            } else {
                m += 1;
            }
        }
    }

    let today_norm = if today_days >= earliest_days && today_days <= latest_days {
        Some((today_days - earliest_days) / range_days)
    } else {
        None
    };

    let mut draw_lanes: Vec<DrawLane> = Vec::new();
    for lane in lanes {
        let mut x_positions: Vec<f64> = Vec::new();
        for s in &lane.sessions {
            let days = iso_to_days(&s.started_at);
            let norm = (days - earliest_days) / range_days;
            x_positions.push(norm.clamp(0.0, 1.0));
        }
        x_positions.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        x_positions.dedup_by(|a, b| (a * 1000.0) as i64 == (b * 1000.0) as i64);

        let is_current = lane.finished_at.is_none() && lane.progress > 0 && lane.progress < 100;

        draw_lanes.push(DrawLane {
            book_id: lane.book_id,
            title: lane.title.clone(),
            authors: lane.authors.clone(),
            progress: lane.progress,
            finished_at: lane.finished_at.clone(),
            x_positions,
            is_current,
        });
    }

    (
        draw_lanes,
        month_ticks,
        today_norm,
        Some((earliest, latest)),
    )
}

/// Draw the axis, month ticks, lanes, today marker.
#[allow(clippy::too_many_arguments)]
fn draw_timeline(
    cr: &cairo::Context,
    width: i32,
    height: i32,
    lanes: &RefCell<Vec<DrawLane>>,
    month_ticks: &RefCell<Vec<(f64, String)>>,
    today_norm: &RefCell<Option<f64>>,
    axis_range: &RefCell<Option<(String, String)>>,
    open_lane: Option<usize>,
    hover_lane: Option<usize>,
    accent_probe: &gtk::Label,
    text_dim_probe: &gtk::Label,
    border_probe: &gtk::Label,
) {
    if width <= 0 || height <= 0 {
        return;
    }

    let lanes = lanes.borrow();
    let month_ticks = month_ticks.borrow();
    let today_norm = today_norm.borrow();
    let axis_range = axis_range.borrow();

    let accent = probe_color(accent_probe);
    let text_dim = probe_color(text_dim_probe);
    let border = probe_color(border_probe);

    let usable_w = (width as f64) - PAD_X - PAD_RIGHT;
    let axis_y = AXIS_HEIGHT as f64;

    if let Some((earliest, latest)) = axis_range.as_ref() {
        let _ = (earliest, latest);
    }

    // ── axis hairline ──
    cr.set_line_width(1.0);
    set_source(cr, border, 1.0);
    cr.move_to(PAD_X, axis_y);
    cr.line_to(PAD_X + usable_w, axis_y);
    let _ = cr.stroke();

    // ── month ticks + labels ──
    cr.set_line_width(1.0);
    set_source(cr, text_dim, 1.0);
    for (norm, initial) in month_ticks.iter() {
        let x = PAD_X + norm * usable_w;
        cr.move_to(x, axis_y);
        cr.line_to(x, axis_y + 4.0);
        let _ = cr.stroke();
        cr.set_font_size(11.0);
        cr.move_to(x - 3.0, axis_y - MONTH_LABEL_PAD as f64);
        let _ = cr.show_text(initial);
    }

    // ── today marker ──
    if let Some(tn) = *today_norm {
        let x = PAD_X + tn * usable_w;
        cr.set_line_width(TODAY_LINE_WIDTH);
        set_source(cr, accent, 1.0);
        cr.move_to(x, axis_y + 2.0);
        cr.line_to(x, axis_y + 8.0);
        let _ = cr.stroke();
        cr.set_font_size(11.0);
        cr.move_to(x - 12.0, axis_y - 6.0);
        let _ = cr.show_text("Today");
    }

    // ── lanes ──
    for (i, lane) in lanes.iter().enumerate() {
        let y = FIRST_LANE_Y as f64 + (i as f64) * LANE_GAP as f64;
        let is_open = open_lane == Some(i);
        let is_hovered = hover_lane == Some(i);
        let _ = is_hovered;

        let (r, g, b) = LANE_COLOURS[i % LANE_COLOURS.len()];
        let line_h = if is_open { OPEN_LANE_HEIGHT } else { LANE_HEIGHT };

        if lane.x_positions.len() >= 2 {
            cr.set_line_width(line_h);
            set_source(
                cr,
                gtk::gdk::RGBA {
                    red: r,
                    green: g,
                    blue: b,
                    alpha: 1.0,
                },
                1.0,
            );
            cr.set_line_cap(cairo::LineCap::Round);
            let first_x = PAD_X + lane.x_positions[0] * usable_w;
            let last_x = PAD_X + lane.x_positions.last().copied().unwrap_or(0.0) * usable_w;
            cr.move_to(first_x, y);
            cr.line_to(last_x, y);
            let _ = cr.stroke();
        } else if let Some(&xn) = lane.x_positions.first() {
            cr.set_line_width(1.0);
            set_source(
                cr,
                gtk::gdk::RGBA {
                    red: r,
                    green: g,
                    blue: b,
                    alpha: 1.0,
                },
                1.0,
            );
            let x = PAD_X + xn * usable_w;
            cr.arc(x, y, LANE_HEIGHT, 0.0, std::f64::consts::TAU);
            let _ = cr.fill();
        }

        // Current-read dot at line's end.
        if lane.is_current {
            if let Some(&xn) = lane.x_positions.last() {
                let x = PAD_X + xn * usable_w;
                set_source(cr, accent, 1.0);
                cr.arc(x, y, CURRENT_DOT_RADIUS, 0.0, std::f64::consts::TAU);
                let _ = cr.fill();
            }
        }

        // Open-line connector below the lane.
        if is_open {
            cr.set_line_width(CONNECTOR_WIDTH);
            set_source(
                cr,
                gtk::gdk::RGBA {
                    red: r,
                    green: g,
                    blue: b,
                    alpha: 1.0,
                },
                1.0,
            );
            if let Some(&xn) = lane.x_positions.last() {
                let x = PAD_X + xn * usable_w;
                cr.move_to(x, y + line_h / 2.0);
                cr.line_to(x, y + 10.0);
                let _ = cr.stroke();
            }
        }
    }
}

/// Hit-test which lane (if any) is at (x, y).
fn hit_test_lane(x: f64, y: f64, lanes: &[DrawLane], axis_y: f64) -> Option<usize> {
    if y < axis_y {
        return None;
    }
    let lane_idx = ((y - FIRST_LANE_Y as f64) / LANE_GAP as f64).floor() as usize;
    if lane_idx < lanes.len() {
        Some(lane_idx)
    } else {
        None
    }
}
