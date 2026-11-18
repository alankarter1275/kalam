//! Your shelf and its elastic timeline ruler (design-system §61,
//! decisions 190 to 193): one custom-drawn widget for the base line,
//! the bars, the dots, the ticks and the Today ring, with a single
//! piecewise date-to-position function shared by every part of it.
//!
//! The shelf is showcase content (B21): the catalog keeps no finish
//! dates, page counts or cover colours yet, so the books below are
//! invented the way the Updates rows are, and the ruler draws them
//! exactly as the design draws its own.

use gtk::prelude::*;

// The scroll controller's modifier state lives in gdk, which this
// module does not otherwise name.
use gtk::gdk;

/// The three zooms (decision 191). Months is the default; there is
/// no switch, only Ctrl+scroll.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Zoom {
    Weeks,
    Months,
    Years,
}

/// One invented finished book: title, author, days before today it
/// was finished, page count, cover colour, rating.
#[derive(Debug, Clone)]
pub struct ShelfBook {
    pub title: &'static str,
    pub author: &'static str,
    pub days_ago: i64,
    pub pages: i32,
    pub colour: (f64, f64, f64),
    pub rating: f64,
}

/// The showcase shelf (B38): fourteen finished books across fifteen
/// months, crowded in some weeks and empty in others, so the elastic
/// ruler has both cases to show.
pub fn showcase_shelf() -> Vec<ShelfBook> {
    use ShelfBook as B;
    vec![
        B { title: "The Left Hand of Darkness", author: "Ursula K. Le Guin", days_ago: 452, pages: 304, colour: (0.42, 0.50, 0.62), rating: 4.5 },
        B { title: "Piranesi", author: "Susanna Clarke", days_ago: 418, pages: 245, colour: (0.55, 0.47, 0.42), rating: 4.0 },
        B { title: "Wolf Hall", author: "Hilary Mantel", days_ago: 372, pages: 653, colour: (0.36, 0.30, 0.26), rating: 4.5 },
        B { title: "Kindred", author: "Octavia E. Butler", days_ago: 330, pages: 287, colour: (0.62, 0.36, 0.30), rating: 5.0 },
        B { title: "Blindsight", author: "Peter Watts", days_ago: 291, pages: 384, colour: (0.30, 0.42, 0.44), rating: 4.0 },
        B { title: "A Wizard of Earthsea", author: "Ursula K. Le Guin", days_ago: 244, pages: 183, colour: (0.47, 0.52, 0.38), rating: 4.5 },
        B { title: "The Dispossessed", author: "Ursula K. Le Guin", days_ago: 198, pages: 387, colour: (0.52, 0.40, 0.50), rating: 5.0 },
        B { title: "Solaris", author: "Stanisław Lem", days_ago: 151, pages: 204, colour: (0.35, 0.45, 0.55), rating: 4.0 },
        B { title: "Wayward Pines", author: "Blake Crouch", days_ago: 96, pages: 320, colour: (0.60, 0.30, 0.26), rating: 3.5 },
        B { title: "Pines", author: "Blake Crouch", days_ago: 88, pages: 320, colour: (0.28, 0.38, 0.32), rating: 4.0 },
        B { title: "Ubik", author: "Philip K. Dick", days_ago: 61, pages: 272, colour: (0.58, 0.44, 0.36), rating: 4.5 },
        B { title: "Dune", author: "Frank Herbert", days_ago: 34, pages: 688, colour: (0.66, 0.50, 0.30), rating: 5.0 },
        B { title: "The Tombs of Atuan", author: "Ursula K. Le Guin", days_ago: 21, pages: 163, colour: (0.44, 0.36, 0.44), rating: 4.0 },
        B { title: "The Farthest Shore", author: "Ursula K. Le Guin", days_ago: 9, pages: 259, colour: (0.32, 0.44, 0.50), rating: 4.5 },
    ]
}

/// The drawn area's height (home-spec §1: an area 340 high).
pub const AREA_H: i32 = 340;
/// The axis stands this many pixels in from the column's left edge.
const AXIS_X: f64 = 10.0;
/// Pixels per day at each zoom, before the elastic stretch.
const PX_PER_DAY: f64 = 1.7;
const PX_PER_DAY_WEEKS: f64 = 7.0;
const PX_PER_DAY_YEARS: f64 = 0.55;
/// Minimum distance between bar centres at each zoom (§61).
const MIN_GAP: f64 = 17.0;
const MIN_GAP_WEEKS: f64 = 18.0;
const MIN_GAP_YEARS: f64 = 6.0;
/// How long the zoom readout pill stays up after the last zoom.
const READOUT_MS: u64 = 900;

const MONTHS: [&str; 12] = [
    "January", "February", "March", "April", "May", "June", "July",
    "August", "September", "October", "November", "December",
];

/// Day number (unix days) of a date; the ruler's only clock.
fn day_of(dt: &gtk::glib::DateTime) -> i64 {
    dt.to_unix() / 86_400
}

fn today_day() -> i64 {
    gtk::glib::DateTime::now_utc()
        .map(|dt| day_of(&dt))
        .unwrap_or(0)
}

fn ymd(day: i64) -> (i32, u32, u32) {
    gtk::glib::DateTime::from_unix_utc(day * 86_400)
        .map(|dt| (dt.year(), dt.month() as u32, dt.day_of_month() as u32))
        .unwrap_or((1970, 1, 1))
}

/// The elastic date-to-position map (decision 191): dates fall where
/// the scale puts them, except where books crowd -- then each book
/// claims its minimum gap and the ruler stretches between anchors.
/// One piecewise-linear function, shared by bars, dots and ticks.
///
/// Returns the anchors, oldest first (a month of air above the
/// oldest book, then the books, then today pinned at the foot).
pub fn layout_books(
    dates: &[i64],
    today: i64,
    px_per_day: f64,
    min_gap: f64,
    area_h: f64,
) -> Vec<(i64, f64)> {
    let today_y = area_h - 18.0;
    let mut sorted: Vec<i64> = dates.to_vec();
    sorted.sort_unstable();
    let ideal: Vec<(i64, f64)> = sorted
        .iter()
        .map(|d| (*d, today_y - (today - d) as f64 * px_per_day))
        .collect();
    // Walk newest to oldest, pushing crowded books up off each other.
    let mut anchored: Vec<(i64, f64)> = Vec::with_capacity(ideal.len());
    for (d, y) in ideal.iter().rev() {
        let y = match anchored.last() {
            Some((_, prev)) => y.min(prev - min_gap),
            None => *y,
        };
        anchored.push((*d, y));
    }
    anchored.reverse();
    let mut anchors = Vec::with_capacity(anchored.len() + 2);
    let top = anchored
        .first()
        .map(|(d, y)| (d - 30, y - 30.0 * px_per_day))
        .unwrap_or((today - 30, today_y - 30.0 * px_per_day));
    anchors.push(top);
    anchors.extend(anchored);
    anchors.push((today, today_y));
    anchors
}

/// How far the content may pan up from its opened-at-bottom rest:
/// enough to bring the top anchor to the widget's top edge.
fn max_pan(anchors: &[(i64, f64)]) -> f64 {
    anchors.first().map(|a| (-a.1).max(0.0)).unwrap_or(0.0)
}

/// Position of any date under the piecewise map; `pan` shifts the
/// content window down from its rest (0 = today at the foot).
fn map_date(anchors: &[(i64, f64)], day: i64, pan: f64) -> f64 {
    let y = match (anchors.first(), anchors.last()) {
        (Some(first), _) if day <= first.0 => first.1,
        (_, Some(last)) if day >= last.0 => last.1,
        (Some(_), Some(_)) => {
            let mut y = anchors[0].1;
            for pair in anchors.windows(2) {
                if day >= pair[0].0 && day <= pair[1].0 {
                    let span = (pair[1].0 - pair[0].0).max(1) as f64;
                    let t = (day - pair[0].0) as f64 / span;
                    y = pair[0].1 + (pair[1].1 - pair[0].1) * t;
                    break;
                }
            }
            y
        }
        _ => 0.0,
    };
    y + pan
}

/// Bar thickness and length from the page count (§61: 8 to 12 thick
/// and 50 to 150 long in Months, 16 thick and 110 to 140 long in
/// Weeks with the title inside, a thin 4 in Years).
fn bar_metrics(zoom: Zoom, pages: i32) -> (f64, f64) {
    let t = (pages.clamp(150, 800) - 150) as f64 / 650.0;
    match zoom {
        Zoom::Months => (8.0 + t * 4.0, 50.0 + t * 100.0),
        Zoom::Weeks => (16.0, 110.0 + t * 30.0),
        Zoom::Years => (4.0, 50.0 + t * 100.0),
    }
}

struct ShelfState {
    zoom: Zoom,
    /// Downward shift of the content from its opened-at-bottom rest;
    /// 0 shows Today at the foot, negative is impossible, and the
    /// ceiling brings the oldest anchor to the top edge.
    pan: f64,
    readout: Option<gtk::glib::SourceId>,
}

/// The shelf block: heading, the drawn ruler with its zoom readout
/// hanging over it, and the caption line (home-spec §1). The empty
/// library draws the ruler's own empty face instead -- base line,
/// Today ring and one Quote-style line (§64, decision 193).
pub fn shelf_ruler(empty: bool) -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let head = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let title = gtk::Label::new(Some("Your shelf"));
    title.add_css_class("kalam-home-heading");
    title.set_halign(gtk::Align::Start);
    head.append(&title);
    // R3 hangs the Arrange by chip here.
    let chip_host = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    chip_host.set_widget_name("arrange_host");
    chip_host.set_hexpand(true);
    chip_host.set_halign(gtk::Align::End);
    head.append(&chip_host);
    root.append(&head);

    let overlay = gtk::Overlay::new();
    overlay.set_size_request(-1, AREA_H);
    overlay.set_margin_top(crate::style::spacing::SM);

    let area = gtk::DrawingArea::new();
    area.set_size_request(-1, AREA_H);
    area.set_hexpand(true);
    area.set_focusable(!empty);

    let books = showcase_shelf();
    let today = today_day();
    let dates: Vec<i64> = books.iter().map(|b| today - b.days_ago).collect();

    let zoom = if empty {
        Zoom::Months
    } else {
        match std::env::var("KALAM_SHELF_ZOOM").as_deref() {
            Ok("weeks") => Zoom::Weeks,
            Ok("years") => Zoom::Years,
            _ => Zoom::Months,
        }
    };
    let state = std::rc::Rc::new(std::cell::RefCell::new(ShelfState {
        zoom,
        pan: 0.0,
        readout: None,
    }));

    let draw_books = books.clone();
    let draw_dates = dates.clone();
    let draw_state = state.clone();
    area.set_draw_func(move |area, cr, width, height| {
        let st = draw_state.borrow();
        let widget = area.upcast_ref::<gtk::Widget>();
        let accent = color(widget, "kalam_accent", (0.78, 0.32, 0.16));
        let dim = color(widget, "kalam_text_dim", (0.45, 0.42, 0.40));
        let bg = color(widget, "kalam_bg", (0.97, 0.95, 0.93));
        let text = color(widget, "kalam_text", (0.15, 0.13, 0.12));
        let anchors = anchors_for(st.zoom, &draw_dates, today, height as f64);
        let pan = st.pan;

        // The base line: the axis itself, 2px, dim at half strength.
        cr.set_line_width(2.0);
        cr.set_source_rgba(dim.0, dim.1, dim.2, 0.5);
        cr.move_to(AXIS_X, 0.0);
        cr.line_to(AXIS_X, height as f64);
        cr.stroke().ok();

        if empty {
            let y = height as f64 - 18.0;
            cr.set_line_width(2.0);
            cr.set_source_rgba(accent.0, accent.1, accent.2, 1.0);
            cr.arc(AXIS_X, y, 5.0, 0.0, std::f64::consts::TAU);
            cr.stroke().ok();
            let layout =
                widget.create_pango_layout(Some("Your first finished book stands here."));
            set_font(layout, "Playfair Display", 18.0, false);
            let (w, _) = layout.size();
            cr.move_to(
                ((width as f64 - f64::from(w) / gtk::pango::SCALE as f64) / 2.0).max(AXIS_X + 20.0),
                y - 40.0,
            );
            cr.set_source_rgba(dim.0, dim.1, dim.2, 1.0);
            cr.update_layout(&layout);
            cr.show_layout(&layout);
            draw_fade(cr, width, &bg);
            return;
        }

        draw_ticks(cr, widget, &anchors, pan, st.zoom, height, &dim, &text);
        draw_bars(cr, widget, &draw_books, &draw_dates, &anchors, pan, st.zoom, height);
        draw_dots(cr, &draw_dates, &anchors, pan, st.zoom, &accent);
        draw_today(cr, widget, &anchors, pan, height, &accent, &bg);
        draw_fade(cr, width, &bg);
    });

    // Plain scroll travels through time; Ctrl+scroll changes the
    // scale (decision 191). Both stop here: the shelf is its own
    // little world inside the page.
    let scroll_state = state.clone();
    let scroll_area = area.clone();
    let scroll_dates = dates.clone();
    let scroll_label = readout_label();
    let controller =
        gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    controller.connect_scroll(move |ctrl, _dx, dy| {
        let mut st = scroll_state.borrow_mut();
        if ctrl.current_event_state().contains(gdk::ModifierType::CONTROL_MASK) {
            let next = match st.zoom {
                Zoom::Weeks if dy > 0.0 => Zoom::Months,
                Zoom::Months if dy < 0.0 => Zoom::Weeks,
                Zoom::Months if dy > 0.0 => Zoom::Years,
                Zoom::Years if dy < 0.0 => Zoom::Months,
                same => same,
            };
            if next != st.zoom {
                st.zoom = next;
                st.pan = 0.0;
                drop(st);
                show_readout(&scroll_label, &scroll_state, next);
                scroll_area.queue_draw();
            }
            return gtk::glib::Propagation::Stop;
        }
        let anchors = anchors_for(st.zoom, &scroll_dates, today, AREA_H as f64);
        // Wheel down walks toward today (the rest), wheel up into the
        // past, like a list that reads bottom-first.
        st.pan = (st.pan - dy * 18.0).clamp(0.0, max_pan(&anchors));
        drop(st);
        scroll_area.queue_draw();
        gtk::glib::Propagation::Stop
    });
    area.add_controller(controller);

    overlay.set_child(Some(&area));

    // The zoom readout pill (§61): shown only while zooming.
    let readout = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    readout.add_css_class("kalam-shelf-readout");
    readout.set_halign(gtk::Align::End);
    readout.set_valign(gtk::Align::Start);
    readout.set_visible(false);
    readout.append(&scroll_label);
    overlay.add_overlay(&readout);

    root.append(&overlay);

    let caption = gtk::Label::new(Some(if empty {
        "Your first finished book stands here."
    } else {
        "Finished books, newest at the bottom."
    }));
    caption.add_css_class("kalam-ui-meta");
    caption.set_visible(!empty);
    caption.set_halign(gtk::Align::Start);
    caption.set_margin_top(crate::style::spacing::XS);
    root.append(&caption);

    root
}

fn readout_label() -> gtk::Label {
    let label = gtk::Label::new(Some("Months"));
    label.set_widget_name("readout_label");
    label
}

fn anchors_for(zoom: Zoom, dates: &[i64], today: i64, area_h: f64) -> Vec<(i64, f64)> {
    let (ppd, gap) = match zoom {
        Zoom::Weeks => (PX_PER_DAY_WEEKS, MIN_GAP_WEEKS),
        Zoom::Months => (PX_PER_DAY, MIN_GAP),
        Zoom::Years => (PX_PER_DAY_YEARS, MIN_GAP_YEARS),
    };
    layout_books(dates, today, ppd, gap, area_h)
}

fn color(w: &gtk::Widget, name: &str, fall: (f64, f64, f64)) -> (f64, f64, f64) {
    w.lookup_color(name)
        .map(|c| (c.red() as f64, c.green() as f64, c.blue() as f64))
        .unwrap_or(fall)
}

fn draw_fade(cr: &gtk::cairo::Context, width: i32, bg: &(f64, f64, f64)) {
    // The top 40px melt into the page (§61).
    for step in 0..40 {
        let y = step as f64;
        cr.set_source_rgba(bg.0, bg.1, bg.2, 1.0 - step as f64 / 40.0);
        cr.set_line_width(1.0);
        cr.move_to(0.0, y);
        cr.line_to(width as f64, y);
        cr.stroke().ok();
    }
}

fn draw_ticks(
    cr: &gtk::cairo::Context,
    widget: &gtk::Widget,
    anchors: &[(i64, f64)],
    pan: f64,
    zoom: Zoom,
    height: i32,
    dim: &(f64, f64, f64),
    text: &(f64, f64, f64),
) {
    let (y0, y1) = match (anchors.first(), anchors.last()) {
        (Some(a), Some(b)) => (a.0.min(b.0), a.0.max(b.0)),
        _ => return,
    };
    match zoom {
        Zoom::Years => {
            let (yy0, _, _) = ymd(y0);
            let (yy1, _, _) = ymd(y1);
            for year in yy0..=yy1 {
                let day = day_number(year, 1, 1);
                let y = map_date(anchors, day, pan);
                if y < -20.0 || y > height as f64 + 20.0 {
                    continue;
                }
                cr.set_line_width(1.0);
                cr.set_source_rgba(dim.0, dim.1, dim.2, 1.0);
                cr.move_to(AXIS_X, y);
                cr.line_to(AXIS_X + 44.0, y);
                cr.stroke().ok();
                let layout = widget.create_pango_layout(Some(&year.to_string()));
                set_font(layout, "Playfair Display", 12.0, true);
                cr.move_to(AXIS_X + 56.0, y - 7.0);
                cr.set_source_rgba(text.0, text.1, text.2, 1.0);
                cr.update_layout(&layout);
                cr.show_layout(&layout);
            }
        }
        Zoom::Months | Zoom::Weeks => {
            let (yy0, mm0, _) = ymd(y0);
            let (yy1, mm1, _) = ymd(y1);
            let mut year = yy0;
            let mut month = mm0 as i32;
            while (year, month) <= (yy1, mm1 as i32) {
                let day = day_number(year, month as u32, 1);
                let y = map_date(anchors, day, pan);
                if y > -20.0 && y < height as f64 + 20.0 {
                    cr.set_line_width(1.0);
                    cr.set_source_rgba(dim.0, dim.1, dim.2, 0.7);
                    cr.move_to(AXIS_X, y);
                    cr.line_to(AXIS_X + 48.0, y);
                    cr.stroke().ok();
                    let name = MONTHS[(month - 1) as usize % 12];
                    let layout = widget.create_pango_layout(Some(name));
                    set_font(layout, "DM Sans", 11.0, false);
                    cr.move_to(AXIS_X + 56.0, y - 6.0);
                    cr.set_source_rgba(dim.0, dim.1, dim.2, 1.0);
                    cr.update_layout(&layout);
                    cr.show_layout(&layout);
                }
                month += 1;
                if month > 12 {
                    month = 1;
                    year += 1;
                }
            }
            if zoom == Zoom::Weeks {
                let mut day = y0;
                while day <= y1 {
                    let y = map_date(anchors, day, pan);
                    if y > -10.0 && y < height as f64 + 10.0 {
                        let (_, _, dom) = ymd(day);
                        let weekday = weekday_of(day);
                        let len = if weekday == 0 { 12.0 } else { 8.0 };
                        cr.set_line_width(1.0);
                        cr.set_source_rgba(
                            dim.0,
                            dim.1,
                            dim.2,
                            if weekday == 0 { 0.30 } else { 0.25 },
                        );
                        cr.move_to(AXIS_X, y);
                        cr.line_to(AXIS_X + len, y);
                        cr.stroke().ok();
                        if weekday == 0 {
                            let layout =
                                widget.create_pango_layout(Some(&dom.to_string()));
                            set_font(layout, "DM Sans", 11.0, false);
                            cr.move_to(AXIS_X + 18.0, y - 6.0);
                            cr.set_source_rgba(dim.0, dim.1, dim.2, 0.8);
                            cr.update_layout(&layout);
                            cr.show_layout(&layout);
                        }
                    }
                    day += 1;
                }
            }
        }
    }
}

fn draw_bars(
    cr: &gtk::cairo::Context,
    widget: &gtk::Widget,
    books: &[ShelfBook],
    dates: &[i64],
    anchors: &[(i64, f64)],
    pan: f64,
    zoom: Zoom,
    height: i32,
) {
    for (book, day) in books.iter().zip(dates.iter()) {
        let y = map_date(anchors, *day, pan);
        if y < -30.0 || y > height as f64 + 30.0 {
            continue;
        }
        let (thick, len) = bar_metrics(zoom, book.pages);
        let top = y - thick / 2.0;
        let (r, g, b) = book.colour;
        cr.set_source_rgba(r, g, b, 1.0);
        rounded_bar(cr, AXIS_X + 2.0, top, len, thick);
        cr.fill().ok();
        // The thin white band near each end (content, §61).
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.2);
        cr.rectangle(AXIS_X + 2.0 + 8.0, top, 2.0, thick);
        cr.rectangle(AXIS_X + 2.0 + len - 6.0, top, 2.0, thick);
        cr.fill().ok();
        if zoom == Zoom::Weeks {
            // The title inside the bar at 11px, ellipsised to the bar.
            let layout = widget.create_pango_layout(Some(book.title));
            set_font(layout, "DM Sans", 11.0, false);
            layout.set_width(((len - 24.0).max(20.0) as i32) * gtk::pango::SCALE);
            layout.set_ellipsize(gtk::pango::EllipsizeMode::End);
            cr.move_to(AXIS_X + 14.0, top + thick / 2.0 - 6.0);
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.92);
            cr.update_layout(&layout);
            cr.show_layout(&layout);
        }
    }
}

fn rounded_bar(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64) {
    // radius 0 3 3 0: square at the axis, round at the far end.
    let r = 3.0.min(h / 2.0);
    cr.move_to(x, y);
    cr.line_to(x + w - r, y);
    cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.line_to(x + w, y + h - r);
    cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
    cr.line_to(x, y + h);
    cr.close_path();
}

fn draw_dots(
    cr: &gtk::cairo::Context,
    dates: &[i64],
    anchors: &[(i64, f64)],
    pan: f64,
    zoom: Zoom,
    accent: &(f64, f64, f64),
) {
    let r = if zoom == Zoom::Years { 2.0 } else { 3.5 };
    for day in dates {
        let y = map_date(anchors, *day, pan);
        cr.set_source_rgba(accent.0, accent.1, accent.2, 1.0);
        cr.arc(AXIS_X, y, r, 0.0, std::f64::consts::TAU);
        cr.fill().ok();
    }
}

fn draw_today(
    cr: &gtk::cairo::Context,
    widget: &gtk::Widget,
    anchors: &[(i64, f64)],
    pan: f64,
    height: i32,
    accent: &(f64, f64, f64),
    bg: &(f64, f64, f64),
) {
    let today = today_day();
    let y = map_date(anchors, today, pan);
    if y < -20.0 || y > height as f64 + 20.0 {
        return;
    }
    cr.set_source_rgba(bg.0, bg.1, bg.2, 1.0);
    cr.arc(AXIS_X, y, 5.0, 0.0, std::f64::consts::TAU);
    cr.fill().ok();
    cr.set_line_width(2.0);
    cr.set_source_rgba(accent.0, accent.1, accent.2, 1.0);
    cr.arc(AXIS_X, y, 5.0, 0.0, std::f64::consts::TAU);
    cr.stroke().ok();
    let layout = widget.create_pango_layout(Some("Today"));
    set_font(layout, "DM Sans", 11.0, false);
    cr.move_to(AXIS_X + 12.0, y - 6.0);
    cr.set_source_rgba(accent.0, accent.1, accent.2, 1.0);
    cr.update_layout(&layout);
    cr.show_layout(&layout);
}

fn set_font(layout: gtk::pango::Layout, family: &str, px: f64, bold: bool) {
    let desc = gtk::pango::FontDescription::from_string(&format!(
        "{family}{bold} {size}px",
        bold = if bold { " Bold" } else { "" },
        size = px as i32
    ));
    layout.set_font_description(Some(&desc));
}

/// Day number of a calendar date, for tick generation.
fn day_number(year: i32, month: u32, day: u32) -> i64 {
    gtk::glib::DateTime::from_ymd(year, month, day, 0, 0, 0.0)
        .map(|dt| day_of(&dt))
        .unwrap_or(0)
}

/// 0 = Monday, matching the design's Monday labels.
fn weekday_of(day: i64) -> i64 {
    // Unix day 0 was a Thursday.
    (day + 3) % 7
}

fn show_readout(
    label: &gtk::Label,
    state: &std::rc::Rc<std::cell::RefCell<ShelfState>>,
    zoom: Zoom,
) {
    let name = match zoom {
        Zoom::Weeks => "Weeks",
        Zoom::Months => "Months",
        Zoom::Years => "Years",
    };
    label.set_text(name);
    if let Some(pill) = label.parent() {
        pill.set_visible(true);
    }
    let old = state.borrow_mut().readout.take();
    if let Some(id) = old {
        id.remove();
    }
    let st = state.clone();
    let label = label.clone();
    let id = gtk::glib::timeout_add_local_once(
        std::time::Duration::from_millis(READOUT_MS),
        move || {
            if let Some(pill) = label.parent() {
                pill.set_visible(false);
            }
            st.borrow_mut().readout = None;
        },
    );
    state.borrow_mut().readout = Some(id);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The elastic map's whole promise: crowded books still stand
    /// their minimum gap apart, and uncrowded ones keep the scale.
    #[test]
    fn crowded_books_keep_their_gap() {
        let today = 1000;
        // Three books on the same day, then a quiet month.
        let dates = [999, 999, 1000, 960];
        let anchors = layout_books(&dates, today, PX_PER_DAY, MIN_GAP, 340.0);
        let ys: Vec<f64> = anchors[1..anchors.len() - 1].iter().map(|a| a.1).collect();
        for pair in ys.windows(2) {
            assert!(
                (pair[0] - pair[1]).abs() >= MIN_GAP - 0.001,
                "bars collided at {} and {}",
                pair[0],
                pair[1]
            );
        }
        // The quiet book keeps its proportional place: forty days up
        // from today at 1.7 px per day.
        let quiet = anchors.iter().find(|a| a.0 == 960).unwrap().1;
        let today_y = 340.0 - 18.0;
        assert!((quiet - (today_y - 40.0 * PX_PER_DAY)).abs() < 0.001);
    }

    /// Today is at the bottom, and the map ends there (decision 191).
    #[test]
    fn today_stands_at_the_bottom() {
        let today = 5000;
        let dates = [4990, 4970];
        let anchors = layout_books(&dates, today, PX_PER_DAY, MIN_GAP, 340.0);
        let last = anchors.last().unwrap();
        assert_eq!(last.0, today);
        assert!((last.1 - (340.0 - 18.0)).abs() < 0.001);
    }
}
