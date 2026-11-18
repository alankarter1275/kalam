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

/// The Arrange by choices (decision 193). Date finished is the
/// default and the only one that draws the ruler; the others stack
/// the bars in their order, and Colour stacks them by hue.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arrange {
    Date,
    Colour,
    Rating,
    Length,
    Author,
}

impl Arrange {
    pub fn from_id(id: &str) -> Self {
        match id {
            "colour" => Self::Colour,
            "rating" => Self::Rating,
            "length" => Self::Length,
            "author" => Self::Author,
            _ => Self::Date,
        }
    }
}

/// The hue of a cover colour, for the rainbow stack.
fn hue(c: (f64, f64, f64)) -> f64 {
    let (r, g, b) = c;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    if max - min < 1e-6 {
        return 0.0;
    }
    let h = if max == r {
        (g - b) / (max - min)
    } else if max == g {
        2.0 + (b - r) / (max - min)
    } else {
        4.0 + (r - g) / (max - min)
    };
    (h * 60.0 + 360.0) % 360.0
}

/// The top-to-bottom order of the books under an arrangement.
pub fn order_books(books: &[ShelfBook], dates: &[i64], arrange: Arrange) -> Vec<usize> {
    let mut idx: Vec<usize> = (0..books.len()).collect();
    match arrange {
        Arrange::Date => idx.sort_by_key(|i| dates[*i]),
        Arrange::Colour => idx.sort_by(|a, b| {
            hue(books[*a].colour).partial_cmp(&hue(books[*b].colour)).unwrap_or(std::cmp::Ordering::Equal)
        }),
        Arrange::Rating => idx.sort_by(|a, b| {
            books[*b].rating.partial_cmp(&books[*a].rating).unwrap_or(std::cmp::Ordering::Equal)
        }),
        Arrange::Length => idx.sort_by_key(|i| std::cmp::Reverse(books[*i].pages)),
        Arrange::Author => idx.sort_by_key(|i| books[*i].author),
    }
    idx
}

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
    // `author` and `rating` wait for R2's hover card (B38); the
    // drawing itself needs neither, and clippy counts unread fields
    // in a binary crate.
    pub title: &'static str,
    #[allow(dead_code)]
    pub author: &'static str,
    pub days_ago: i64,
    pub pages: i32,
    pub colour: (f64, f64, f64),
    pub rating: f64,
    /// Invented days from first opened to finished, for the hover
    /// card's Took stat (decision 192).
    pub took_days: i32,
    /// An invented impression line, for the hover card's Quote-style
    /// note (B38).
    pub note: &'static str,
}

/// The showcase shelf (B38): fourteen finished books across fifteen
/// months, crowded in some weeks and empty in others, so the elastic
/// ruler has both cases to show.
pub fn showcase_shelf() -> Vec<ShelfBook> {
    use ShelfBook as B;
    vec![
        B { title: "The Left Hand of Darkness", author: "Ursula K. Le Guin", days_ago: 452, pages: 304, colour: (0.42, 0.50, 0.62), rating: 4.5, took_days: 148, note: "The north kept getting colder as she read it." },
        B { title: "Piranesi", author: "Susanna Clarke", days_ago: 418, pages: 245, colour: (0.55, 0.47, 0.42), rating: 4.0, took_days: 96, note: "A house of staircases; she still draws them." },
        B { title: "Wolf Hall", author: "Hilary Mantel", days_ago: 372, pages: 653, colour: (0.36, 0.30, 0.26), rating: 4.5, took_days: 210, note: "Cromwell's quiet, and the court's noise." },
        B { title: "Kindred", author: "Octavia E. Butler", days_ago: 330, pages: 287, colour: (0.62, 0.36, 0.30), rating: 5.0, took_days: 61, note: "Time would not sit still for her either." },
        B { title: "Blindsight", author: "Peter Watts", days_ago: 291, pages: 384, colour: (0.30, 0.42, 0.44), rating: 4.0, took_days: 44, note: "The scramblebirds are still in her head." },
        B { title: "A Wizard of Earthsea", author: "Ursula K. Le Guin", days_ago: 244, pages: 183, colour: (0.47, 0.52, 0.38), rating: 4.5, took_days: 30, note: "The first word of the whole shelf." },
        B { title: "The Dispossessed", author: "Ursula K. Le Guin", days_ago: 198, pages: 387, colour: (0.52, 0.40, 0.50), rating: 5.0, took_days: 77, note: "Anarchy, and the cost of holding it." },
        B { title: "Solaris", author: "Stanisław Lem", days_ago: 151, pages: 204, colour: (0.35, 0.45, 0.55), rating: 4.0, took_days: 58, note: "The ocean thought back at them." },
        B { title: "Wayward Pines", author: "Blake Crouch", days_ago: 96, pages: 320, colour: (0.60, 0.30, 0.26), rating: 3.5, took_days: 25, note: "Read in one night, lights on." },
        B { title: "Pines", author: "Blake Crouch", days_ago: 88, pages: 320, colour: (0.28, 0.38, 0.32), rating: 4.0, took_days: 19, note: "Read in one night, lights on." },
        B { title: "Ubik", author: "Philip K. Dick", days_ago: 61, pages: 272, colour: (0.58, 0.44, 0.36), rating: 4.5, took_days: 41, note: "Every ending felt like a door." },
        B { title: "Dune", author: "Frank Herbert", days_ago: 34, pages: 688, colour: (0.66, 0.50, 0.30), rating: 5.0, took_days: 132, note: "The desert, and the worm, and the wait." },
        B { title: "The Tombs of Atuan", author: "Ursula K. Le Guin", days_ago: 21, pages: 163, colour: (0.44, 0.36, 0.44), rating: 4.0, took_days: 24, note: "The tombs were quieter than the living." },
        B { title: "The Farthest Shore", author: "Ursula K. Le Guin", days_ago: 9, pages: 259, colour: (0.32, 0.44, 0.50), rating: 4.5, took_days: 33, note: "Dragonflight, at last, and earned." },
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

fn ymd(day: i64) -> (i32, i32, i32) {
    gtk::glib::DateTime::from_unix_utc(day * 86_400)
        .map(|dt| (dt.year(), dt.month(), dt.day_of_month()))
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

type Rgb = (f64, f64, f64);

/// The resolved theme tokens a drawn widget paints with.
struct Palette {
    accent: Rgb,
    dim: Rgb,
    bg: Rgb,
    text: Rgb,
}

/// Where the content stands right now: the elastic anchors, the pan,
/// the zoom and the widget's height, passed together so the draw
/// helpers stay under clippy's argument count.
struct View<'a> {
    anchors: &'a [(i64, f64)],
    pan: f64,
    zoom: Zoom,
    height: i32,
}

/// What the pointer, the keyboard and the month filter are doing to
/// the drawing this frame.
#[derive(Clone)]
struct Fx {
    hover: Option<usize>,
    selected: Option<usize>,
    filter: Option<(i32, i32)>,
    /// Per book: dimmed to a quarter by the month filter (193).
    dimmed: Vec<bool>,
}

struct ShelfState {
    zoom: Zoom,
    /// Downward shift of the content from its opened-at-bottom rest;
    /// 0 shows Today at the foot, negative is impossible, and the
    /// ceiling brings the oldest anchor to the top edge.
    pan: f64,
    readout: Option<gtk::glib::SourceId>,
    /// The bar the pointer rests on (§61: it slides out 14).
    hover: Option<usize>,
    /// The bar the keyboard stands on (Left, Right, Enter).
    selected: Option<usize>,
    /// A month the shelf is filtered to (decision 193).
    filter: Option<(i32, i32)>,
    arrange: Arrange,
    hide_grace: Option<gtk::glib::SourceId>,
}

/// Where things were drawn, kept between frames so the pointer and
/// the keyboard can hit-test what the eye sees.
#[derive(Default)]
struct Geom {
    /// x, y, width, thickness and book index, in draw order.
    bars: Vec<(f64, f64, f64, f64, usize)>,
    /// x, y, width, height, year, month per month label.
    labels: Vec<(f64, f64, f64, f64, i32, i32)>,
}

/// The shelf block: heading, the drawn ruler with its zoom readout
/// hanging over it, and the caption line (home-spec §1). The empty
/// library draws the ruler's own empty face instead -- base line,
/// Today ring and one Quote-style line (§64, decision 193).
pub fn shelf_ruler(
    empty: bool,
    theme: &crate::theme::Theme,
    filter_pill: &gtk::Button,
    arrange: Arrange,
) -> gtk::Box {
    // The filter is a date idea; other arrangements have no months
    // to filter (decision 193).
    let empty = empty || std::env::var_os("KALAM_SHELF_EMPTY").is_some();
    let pal = Palette {
        accent: hex(theme.accent),
        dim: hex(theme.text_dim),
        bg: hex(theme.bg),
        text: hex(theme.text),
    };
    // The heading row (with the Arrange by chip and the filter pill)
    // lives in the page's view; the ruler is the drawn area, the
    // caption and nothing else.
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);

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
    let filter = if arrange == Arrange::Date {
        std::env::var("KALAM_SHELF_FILTER").ok().and_then(|v| {
            let (y, m) = v.split_once('-')?;
            Some((y.parse::<i32>().ok()?, m.parse::<i32>().ok()?))
        })
    } else {
        None
    };
    let state = std::rc::Rc::new(std::cell::RefCell::new(ShelfState {
        zoom,
        pan: 0.0,
        readout: None,
        hover: None,
        selected: None,
        filter,
        arrange,
        hide_grace: None,
    }));
    let geom = std::rc::Rc::new(std::cell::RefCell::new(Geom::default()));

    let draw_books = books.clone();
    let draw_dates = dates.clone();
    let draw_state = state.clone();
    let draw_geom = geom.clone();
    area.set_draw_func(move |area, cr, width, height| {
        let st = draw_state.borrow();
        let widget = area.upcast_ref::<gtk::Widget>();
        let anchors = anchors_for(st.zoom, &draw_dates, today, height as f64);
        let pan = st.pan;

        // The base line: the axis itself, 2px, dim at half strength.
        cr.set_line_width(2.0);
        cr.set_source_rgba(pal.dim.0, pal.dim.1, pal.dim.2, 0.5);
        cr.move_to(AXIS_X, 0.0);
        cr.line_to(AXIS_X, height as f64);
        cr.stroke().ok();

        if empty {
            let y = height as f64 - 18.0;
            cr.set_line_width(2.0);
            cr.set_source_rgba(pal.accent.0, pal.accent.1, pal.accent.2, 1.0);
            cr.arc(AXIS_X, y, 5.0, 0.0, std::f64::consts::TAU);
            cr.stroke().ok();
            let layout =
                widget.create_pango_layout(Some("Your first finished book stands here."));
            set_font(&layout, "Playfair Display", 18.0, false);
            let (w, _) = layout.size();
            cr.move_to(
                ((width as f64 - f64::from(w) / gtk::pango::SCALE as f64) / 2.0).max(AXIS_X + 20.0),
                y - 40.0,
            );
            cr.set_source_rgba(pal.dim.0, pal.dim.1, pal.dim.2, 1.0);
            pangocairo::functions::update_layout(cr, &layout);
            pangocairo::functions::show_layout(cr, &layout);
            draw_fade(cr, width, &pal.bg);
            return;
        }

        let view = View { anchors: &anchors, pan, zoom: st.zoom, height };
        let mut geom = draw_geom.borrow_mut();
        geom.bars.clear();
        geom.labels.clear();
        let dimmed: Vec<bool> = draw_dates
            .iter()
            .map(|d| match st.filter {
                Some((fy, fm)) => {
                    let (y, m, _) = ymd(*d);
                    (y, m) != (fy, fm)
                }
                None => false,
            })
            .collect();
        let fx = Fx {
            hover: st.hover,
            selected: st.selected,
            filter: st.filter,
            dimmed,
        };
        // The arrangement orders the bars top to bottom; only the
        // date arrangement owns the ruler (decision 193).
        let order = order_books(&draw_books, &draw_dates, st.arrange);
        let dated = st.arrange == Arrange::Date;
        let ys: Vec<f64> = (0..draw_books.len())
            .map(|i| {
                if dated {
                    map_date(&anchors, draw_dates[i], pan)
                } else {
                    let rank = order.iter().position(|o| *o == i).unwrap_or(0);
                    24.0 + rank as f64 * 20.0 - pan
                }
            })
            .collect();
        if dated {
            draw_ticks(cr, widget, &draw_dates, &view, &pal, &mut geom, &fx);
        }
        draw_bars(cr, widget, &draw_books, &ys, &view, &mut geom, &fx);
        if dated {
            draw_dots(cr, &ys, &view, &pal);
            draw_today(cr, widget, &view, &pal);
        }
        draw_fade(cr, width, &pal.bg);
    });

    // Plain scroll travels through time; Ctrl+scroll changes the
    // scale (decision 191). Both stop here: the shelf is its own
    // little world inside the page.
    let scroll_state = state.clone();
    let scroll_area = area.clone();
    let scroll_dates = dates.clone();
    // One label, two owners: the scroll closure takes a clone and the
    // pill below keeps the original.
    let readout_label = readout_label();
    let scroll_label = readout_label.clone();
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
    readout.append(&readout_label);
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
    if filter.is_some() {
        apply_filter(filter, filter_pill, &caption);
    }

    // ---- R2: the hover card, the month filter, the keyboard.
    let popover = gtk::Popover::new();
    popover.set_has_arrow(false);
    popover.add_css_class("kalam-shelf-popover");
    popover.set_parent(&area);

    let card_box = gtk::Box::new(gtk::Orientation::Vertical, crate::style::spacing::XS);
    card_box.add_css_class("kalam-shelf-card");
    card_box.set_size_request(278, -1);

    let cover_colour = std::rc::Rc::new(std::cell::Cell::new((0.5, 0.5, 0.5)));
    let cover = gtk::DrawingArea::new();
    cover.set_size_request(56, 84);
    cover.set_halign(gtk::Align::Start);
    let cover_cc = cover_colour.clone();
    cover.set_draw_func(move |_w, cr, width, height| {
        let (r, g, b) = cover_cc.get();
        cr.set_source_rgba(r, g, b, 1.0);
        round_rect(cr, 0.0, 0.0, width as f64, height as f64, 6.0);
        cr.fill().ok();
    });
    card_box.append(&cover);

    let title_l = gtk::Label::new(None);
    title_l.add_css_class("kalam-ui-card-title");
    title_l.set_halign(gtk::Align::Start);
    title_l.set_wrap(true);
    card_box.append(&title_l);
    let author_l = gtk::Label::new(None);
    author_l.add_css_class("kalam-ui-meta");
    author_l.set_halign(gtk::Align::Start);
    card_box.append(&author_l);
    let stars_l = gtk::Label::new(None);
    stars_l.add_css_class("kalam-shelf-stars");
    stars_l.set_halign(gtk::Align::Start);
    card_box.append(&stars_l);
    let date_l = gtk::Label::new(None);
    date_l.add_css_class("kalam-ui-meta");
    date_l.set_halign(gtk::Align::Start);
    card_box.append(&date_l);

    let stats = gtk::Box::new(gtk::Orientation::Horizontal, crate::style::spacing::LG);
    stats.set_margin_top(crate::style::spacing::XS);
    let took_l = stat_pair(&stats, "Took");
    let pages_l = stat_pair(&stats, "Pages");
    card_box.append(&stats);

    let note_l = gtk::Label::new(None);
    note_l.add_css_class("kalam-ui-quote");
    note_l.set_halign(gtk::Align::Start);
    note_l.set_xalign(0.0);
    note_l.set_wrap(true);
    note_l.set_margin_top(crate::style::spacing::XS);
    card_box.append(&note_l);
    popover.set_child(Some(&card_box));

    let fill_card: std::rc::Rc<dyn Fn(usize)> = {
        let (title_l, author_l, stars_l, date_l, note_l) =
            (title_l.clone(), author_l.clone(), stars_l.clone(), date_l.clone(), note_l.clone());
        let (took_l, pages_l) = (took_l.clone(), pages_l.clone());
        let cc = cover_colour.clone();
        let cover = cover.clone();
        let books = books.clone();
        let dates = dates.clone();
        std::rc::Rc::new(move |i: usize| {
            let Some(b) = books.get(i) else { return };
            title_l.set_text(b.title);
            author_l.set_text(b.author);
            let stars = "★".repeat(b.rating.round() as usize);
            stars_l.set_text(&format!("{stars}  {}", b.rating));
            let (y, m, d) = ymd(dates[i]);
            date_l.set_text(&format!(
                "Finished {} {} {}",
                d,
                MONTHS[(m - 1) as usize % 12],
                y
            ));
            took_l.set_text(&format!("{} days", b.took_days));
            pages_l.set_text(&b.pages.to_string());
            note_l.set_text(b.note);
            cc.set(b.colour);
            cover.queue_draw();
        })
    };

    let show_at: std::rc::Rc<dyn Fn(usize)> = {
        let popover = popover.clone();
        let geom = geom.clone();
        let pop_area = area.clone();
        std::rc::Rc::new(move |i: usize| {
            // A popover needs a toplevel to hang its surface from,
            // and a bar to point at: before the window maps there is
            // no surface, and before the first draw there is no bar
            // geometry. Asking anyway wedged the whole frame in CI
            // (B38, first two hover photographs).
            if !pop_area.is_mapped() {
                return;
            }
            let rect = {
                let g = geom.borrow();
                g.bars
                    .iter()
                    .copied()
                    .find(|r| r.4 == i && r.2 > 0.0)
            };
            let Some((x, y, len, thick, _)) = rect else {
                return;
            };
            popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(
                (x + len + 30.0) as i32,
                y as i32,
                2,
                thick.max(4.0) as i32,
            )));
            popover.popup();
        })
    };

    let cancel_hide: std::rc::Rc<dyn Fn()> = {
        let state = state.clone();
        std::rc::Rc::new(move || {
            if let Some(id) = state.borrow_mut().hide_grace.take() {
                id.remove();
            }
        })
    };
    let hide_later: std::rc::Rc<dyn Fn()> = {
        let popover = popover.clone();
        let state = state.clone();
        std::rc::Rc::new(move || {
            if let Some(id) = state.borrow_mut().hide_grace.take() {
                id.remove();
            }
            let st = state.clone();
            let pop = popover.clone();
            let id = gtk::glib::timeout_add_local_once(
                std::time::Duration::from_millis(300),
                move || {
                    pop.popdown();
                    st.borrow_mut().hide_grace = None;
                },
            );
            state.borrow_mut().hide_grace = Some(id);
        })
    };

    // Hover: the bar slides, the leader runs, the card stands to the
    // right (§61). Leave gives a 300 ms grace before the card goes.
    let motion = gtk::EventControllerMotion::new();
    let m_state = state.clone();
    let m_area = area.clone();
    let m_geom = geom.clone();
    let m_fill = fill_card.clone();
    let m_show = show_at.clone();
    let m_cancel = cancel_hide.clone();
    motion.connect_motion(move |_ctrl, x, y| {
        let idx = hit_bar(&m_geom.borrow(), x, y);
        let changed = {
            let mut st = m_state.borrow_mut();
            let changed = st.hover != idx;
            st.hover = idx;
            changed
        };
        if changed {
            m_area.queue_draw();
        }
        if let Some(i) = idx {
            m_cancel();
            m_fill(i);
            m_show(i);
        }
    });
    let l_state = state.clone();
    let l_area = area.clone();
    let l_hide = hide_later.clone();
    motion.connect_leave(move |_ctrl| {
        l_state.borrow_mut().hover = None;
        l_area.queue_draw();
        l_hide();
    });
    area.add_controller(motion);

    // Click: a month label filters the shelf to it; a bar takes the
    // keyboard's place and opens the card (decisions 191, 193).
    let click = gtk::GestureClick::new();
    let c_state = state.clone();
    let c_area = area.clone();
    let c_geom = geom.clone();
    let c_pill = filter_pill.clone();
    let c_caption = caption.clone();
    let c_fill = fill_card.clone();
    let c_show = show_at.clone();
    let c_cancel = cancel_hide.clone();
    click.connect_pressed(move |_g, _n, x, y| {
        let label = {
            let g = c_geom.borrow();
            g.labels
                .iter()
                .find(|r| x >= r.0 && x <= r.0 + r.2 && y >= r.1 && y <= r.1 + r.3)
                .map(|r| (r.4, r.5))
        };
        if let Some((yy, mm)) = label {
            let next = {
                let mut st = c_state.borrow_mut();
                st.filter = if st.filter == Some((yy, mm)) { None } else { Some((yy, mm)) };
                st.filter
            };
            apply_filter(next, &c_pill, &c_caption);
            c_area.queue_draw();
            return;
        }
        if let Some(i) = hit_bar(&c_geom.borrow(), x, y) {
            c_state.borrow_mut().selected = Some(i);
            c_area.queue_draw();
            c_cancel();
            c_fill(i);
            c_show(i);
        }
    });
    area.add_controller(click);

    // The pill clears what the label set.
    let p_state = state.clone();
    let p_area = area.clone();
    let p_pill = filter_pill.clone();
    let p_caption = caption.clone();
    filter_pill.connect_clicked(move |_| {
        p_state.borrow_mut().filter = None;
        apply_filter(None, &p_pill, &p_caption);
        p_area.queue_draw();
    });

    // Keyboard: Left and Right walk the bars, Enter opens the card,
    // Escape closes it (home-spec §6).
    // The ruler area cannot hold focus, so typed keys never reach a
    // controller on it. Hang the controller on the root instead: it
    // sees every key the focused widget does not consume (the search
    // entry swallows its own typing; arrow keys and enter on a bare
    // window land here).
    let k_state = state.clone();
    let k_area = area.clone();
    let k_fill = fill_card.clone();
    let k_show = show_at.clone();
    let k_cancel = cancel_hide.clone();
    let k_pop = popover.clone();
    let book_len = books.len();
    area.connect_realize(move |a| {
        let Some(root) = a.root() else { return };
        let keys = gtk::EventControllerKey::new();
        let k_state = k_state.clone();
        let k_area = k_area.clone();
        let k_fill = k_fill.clone();
        let k_show = k_show.clone();
        let k_cancel = k_cancel.clone();
        let k_pop = k_pop.clone();
        keys.connect_key_pressed(move |_c, key, _code, _mod| {
        use gtk::gdk::Key;
        match key {
            Key::Right | Key::Down | Key::Left | Key::Up => {
                let mut st = k_state.borrow_mut();
                st.selected = Some(match (st.selected, key) {
                    (Some(i), Key::Right) | (Some(i), Key::Down) => (i + 1).min(book_len - 1),
                    (Some(i), _) => i.saturating_sub(1),
                    (None, _) => 0,
                });
                drop(st);
                k_area.queue_draw();
                gtk::glib::Propagation::Stop
            }
            Key::Return | Key::KP_Enter => {
                let sel = k_state.borrow().selected;
                if let Some(i) = sel {
                    k_cancel();
                    k_fill(i);
                    k_show(i);
                }
                gtk::glib::Propagation::Stop
            }
            Key::Escape => {
                k_pop.popdown();
                gtk::glib::Propagation::Stop
            }
            _ => gtk::glib::Propagation::Proceed,
        }
        });
        root.add_controller(keys);
    });

    // CI's photograph of the keyboard path (B38): the lever does
    // what three Downs and an Enter would -- select a bar and open
    // its card -- because the harness sends no arrows of its own.
    if std::env::var_os("KALAM_SHELF_KEYS").is_some() {
        let s_state = state.clone();
        let s_area = area.clone();
        let s_fill = fill_card.clone();
        let s_show = show_at.clone();
        area.connect_map(move |_| {
            let s_state = s_state.clone();
            let s_area = s_area.clone();
            let s_fill = s_fill.clone();
            let s_show = s_show.clone();
            gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
                s_state.borrow_mut().selected = Some(3);
                s_area.queue_draw();
                s_fill(3);
                s_show(3);
            });
        });
    }

    // CI's photograph of the hover card (B38): hold it open on one
    // book, the way a resting pointer would -- but only once the
    // widget is inside a mapped window, or the popover has no
    // surface to open onto.
    if let Ok(i) = std::env::var("KALAM_SHELF_HOVER").map_err(|_| ())
        .and_then(|v| v.parse::<usize>().map_err(|_| ()))
    {
        let h_state = state.clone();
        let h_area = area.clone();
        let h_fill = fill_card.clone();
        let h_show = show_at.clone();
        area.connect_map(move |_| {
            // One beat after the first draw, so the bars exist and
            // the map cascade is over before the popover opens.
            let h_state = h_state.clone();
            let h_area = h_area.clone();
            let h_fill = h_fill.clone();
            let h_show = h_show.clone();
            gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(300), move || {
                h_state.borrow_mut().hover = Some(i);
                h_area.queue_draw();
                h_fill(i);
                h_show(i);
            });
        });
    }

    root
}

/// One Label-over-13px stat for the hover card (§61); returns the
/// value label for the filler to write.
fn stat_pair(host: &gtk::Box, name: &str) -> gtk::Label {
    let col = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let label = gtk::Label::new(Some(name));
    label.add_css_class("kalam-shelf-stat-label");
    label.set_halign(gtk::Align::Start);
    col.append(&label);
    let value = gtk::Label::new(None);
    value.add_css_class("kalam-shelf-stat-value");
    value.set_halign(gtk::Align::Start);
    col.append(&value);
    host.append(&col);
    value
}

/// Which bar the pointer is over, if any.
fn hit_bar(geom: &Geom, x: f64, y: f64) -> Option<usize> {
    geom.bars.iter().find_map(|&(bx, by, len, thick, i)| {
        (len > 0.0 && x >= AXIS_X && x <= bx + len + 4.0 && y >= by - 2.0 && y <= by + thick + 2.0)
            .then_some(i)
    })
}

/// The month-filter pill and the caption follow the filter
/// (decision 193: the pill with a cross, the caption changes).
fn apply_filter(f: Option<(i32, i32)>, pill: &gtk::Button, caption: &gtk::Label) {
    match f {
        Some((y, m)) => {
            let name = MONTHS[(m - 1) as usize % 12];
            pill.set_label(&format!("{name} '{} ×", y % 100));
            pill.set_visible(true);
            caption.set_text(&format!("Finished in {name} {y}."));
        }
        None => {
            pill.set_visible(false);
            caption.set_text("Finished books, newest at the bottom.");
        }
    }
}

fn round_rect(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    let r = r.min(w / 2.0).min(h / 2.0);
    cr.move_to(x + r, y);
    cr.line_to(x + w - r, y);
    cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.line_to(x + w, y + h - r);
    cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
    cr.line_to(x + r, y + h);
    cr.arc(x + r, y + h - r, r, std::f64::consts::FRAC_PI_2, std::f64::consts::PI);
    cr.line_to(x, y + r);
    cr.arc(x + r, y + r, r, std::f64::consts::PI, 3.0 * std::f64::consts::FRAC_PI_2);
    cr.close_path();
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

/// A theme token as paint: the resolved theme's hex strings, parsed.
/// Drawn widgets cannot read CSS tokens, and this is the same hand-
/// off charts.rs already uses (B38).
fn hex(s: &str) -> (f64, f64, f64) {
    let bytes = s.trim_start_matches('#');
    let v = u32::from_str_radix(bytes.get(0..6).unwrap_or("888888"), 16)
        .unwrap_or(0x88_88_88);
    (
        ((v >> 16) & 255) as f64 / 255.0,
        ((v >> 8) & 255) as f64 / 255.0,
        (v & 255) as f64 / 255.0,
    )
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
    dates: &[i64],
    view: &View<'_>,
    pal: &Palette,
    geom: &mut Geom,
    fx: &Fx,
) {
    let View { anchors, pan, zoom, height } = *view;
    let (dim, text) = (pal.dim, pal.text);
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
                set_font(&layout, "Playfair Display", 12.0, true);
                cr.move_to(AXIS_X + 56.0, y - 7.0);
                cr.set_source_rgba(text.0, text.1, text.2, 1.0);
                pangocairo::functions::update_layout(cr, &layout);
                pangocairo::functions::show_layout(cr, &layout);
                // §61: Years adds a Meta "N books" under each year.
                draw_year_count(cr, widget, dates, year, y, dim);
            }
        }
        Zoom::Months | Zoom::Weeks => {
            let (yy0, mm0, _) = ymd(y0);
            let (yy1, mm1, _) = ymd(y1);
            let mut year = yy0;
            let mut month = mm0;
            while (year, month) <= (yy1, mm1) {
                let day = day_number(year, month, 1);
                let y = map_date(anchors, day, pan);
                if y > -20.0 && y < height as f64 + 20.0 {
                    cr.set_line_width(1.0);
                    cr.set_source_rgba(dim.0, dim.1, dim.2, 0.7);
                    cr.move_to(AXIS_X, y);
                    cr.line_to(AXIS_X + 48.0, y);
                    cr.stroke().ok();
                    let name = MONTHS[(month - 1) as usize % 12];
                    let filtered = fx.filter == Some((year, month));
                    let layout = widget.create_pango_layout(Some(name));
                    set_font(&layout, "DM Sans", 11.0, filtered);
                    cr.move_to(AXIS_X + 56.0, y - 6.0);
                    if filtered {
                        cr.set_source_rgba(pal.accent.0, pal.accent.1, pal.accent.2, 1.0);
                    } else {
                        cr.set_source_rgba(dim.0, dim.1, dim.2, 1.0);
                    }
                    pangocairo::functions::update_layout(cr, &layout);
                    pangocairo::functions::show_layout(cr, &layout);
                    let (lw, lh) = layout.size();
                    geom.labels.push((
                        AXIS_X + 52.0,
                        y - 8.0,
                        f64::from(lw) / gtk::pango::SCALE as f64 + 8.0,
                        f64::from(lh) / gtk::pango::SCALE as f64 + 4.0,
                        year,
                        month,
                    ));
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
                            set_font(&layout, "DM Sans", 11.0, false);
                            cr.move_to(AXIS_X + 18.0, y - 6.0);
                            cr.set_source_rgba(dim.0, dim.1, dim.2, 0.8);
                            pangocairo::functions::update_layout(cr, &layout);
                            pangocairo::functions::show_layout(cr, &layout);
                        }
                    }
                    day += 1;
                }
            }
        }
    }
}

/// The Meta "N books" line under a Years label (§61).
fn draw_year_count(
    cr: &gtk::cairo::Context,
    widget: &gtk::Widget,
    dates: &[i64],
    year: i32,
    y: f64,
    dim: Rgb,
) {
    let n = dates.iter().filter(|d| ymd(**d).0 == year).count();
    let text = format!("{n} book{}", if n == 1 { "" } else { "s" });
    let layout = widget.create_pango_layout(Some(&text));
    set_font(&layout, "DM Sans", 11.0, false);
    cr.move_to(AXIS_X + 56.0, y + 8.0);
    cr.set_source_rgba(dim.0, dim.1, dim.2, 1.0);
    pangocairo::functions::update_layout(cr, &layout);
    pangocairo::functions::show_layout(cr, &layout);
}

fn draw_bars(
    cr: &gtk::cairo::Context,
    widget: &gtk::Widget,
    books: &[ShelfBook],
    ys: &[f64],
    view: &View<'_>,
    geom: &mut Geom,
    fx: &Fx,
) {
    for (i, book) in books.iter().enumerate() {
        let y = ys[i];
        if y < -30.0 || y > view.height as f64 + 30.0 {
            geom.bars.push((0.0, 0.0, 0.0, 0.0, i));
            continue;
        }
        let (thick, len) = bar_metrics(view.zoom, book.pages);
        let lifted = fx.hover == Some(i) || fx.selected == Some(i);
        let slide = if lifted { 14.0 } else { 0.0 };
        let x = AXIS_X + 2.0 + slide;
        let top = y - thick / 2.0;
        // A filtered month keeps its colour; the rest stand at a
        // quarter strength (decision 193).
        let alpha = if fx.dimmed[i] { 0.25 } else { 1.0 };
        let (r, g, b) = book.colour;
        cr.set_source_rgba(r, g, b, alpha);
        rounded_bar(cr, x, top, len, thick);
        cr.fill().ok();
        cr.set_source_rgba(1.0, 1.0, 1.0, 0.2 * alpha);
        cr.rectangle(x + 8.0, top, 2.0, thick);
        cr.rectangle(x + len - 6.0, top, 2.0, thick);
        cr.fill().ok();
        if lifted {
            // The hairline leader at 40% text-2, from the slid bar's
            // end toward the card (§61).
            cr.set_line_width(1.0);
            cr.set_source_rgba(0.45, 0.42, 0.40, 0.4);
            cr.move_to(x + len + 4.0, y);
            cr.line_to(x + len + 28.0, y);
            cr.stroke().ok();
        }
        if fx.selected == Some(i) {
            // The keyboard's ring: 2px accent at 2px offset (§7).
            cr.set_line_width(2.0);
            cr.set_source_rgba(0.78, 0.32, 0.16, 1.0);
            cr.rectangle(x - 2.0, top - 2.0, len + 4.0, thick + 4.0);
            cr.stroke().ok();
        }
        if view.zoom == Zoom::Weeks {
            let layout = widget.create_pango_layout(Some(book.title));
            set_font(&layout, "DM Sans", 11.0, false);
            layout.set_width(((len - 24.0).max(20.0) as i32) * gtk::pango::SCALE);
            layout.set_ellipsize(gtk::pango::EllipsizeMode::End);
            cr.move_to(x + 12.0, top + thick / 2.0 - 6.0);
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.92 * alpha);
            pangocairo::functions::update_layout(cr, &layout);
            pangocairo::functions::show_layout(cr, &layout);
        }
        geom.bars.push((x, top, len, thick, i));
    }
}

fn rounded_bar(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64) {
    // radius 0 3 3 0: square at the axis, round at the far end.
    let r = 3.0f64.min(h / 2.0);
    cr.move_to(x, y);
    cr.line_to(x + w - r, y);
    cr.arc(x + w - r, y + r, r, -std::f64::consts::FRAC_PI_2, 0.0);
    cr.line_to(x + w, y + h - r);
    cr.arc(x + w - r, y + h - r, r, 0.0, std::f64::consts::FRAC_PI_2);
    cr.line_to(x, y + h);
    cr.close_path();
}

fn draw_dots(cr: &gtk::cairo::Context, ys: &[f64], view: &View<'_>, pal: &Palette) {
    let accent = pal.accent;
    let r = if view.zoom == Zoom::Years { 2.0 } else { 3.5 };
    for y in ys {
        cr.set_source_rgba(accent.0, accent.1, accent.2, 1.0);
        cr.arc(AXIS_X, *y, r, 0.0, std::f64::consts::TAU);
        cr.fill().ok();
    }
}

fn draw_today(
    cr: &gtk::cairo::Context,
    widget: &gtk::Widget,
    view: &View<'_>,
    pal: &Palette,
) {
    let View { anchors, pan, height, .. } = *view;
    let (accent, bg) = (pal.accent, pal.bg);
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
    set_font(&layout, "DM Sans", 11.0, false);
    cr.move_to(AXIS_X + 12.0, y - 6.0);
    cr.set_source_rgba(accent.0, accent.1, accent.2, 1.0);
    pangocairo::functions::update_layout(cr, &layout);
    pangocairo::functions::show_layout(cr, &layout);
}

fn set_font(layout: &gtk::pango::Layout, family: &str, px: f64, bold: bool) {
    let desc = gtk::pango::FontDescription::from_string(&format!(
        "{family}{bold} {size}px",
        bold = if bold { " Bold" } else { "" },
        size = px as i32
    ));
    layout.set_font_description(Some(&desc));
}

/// Day number of a calendar date, for tick generation.
fn day_number(year: i32, month: i32, day: i32) -> i64 {
    gtk::glib::DateTime::from_utc(year, month, day, 0, 0, 0.0)
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

    /// The arrangements reorder without losing or doubling a book,
    /// and the colour stack really runs round the wheel (193).
    #[test]
    fn arrangements_are_permutations() {
        let books = showcase_shelf();
        let dates: Vec<i64> = books.iter().map(|b| 1000 - b.days_ago).collect();
        for arrange in [
            Arrange::Date,
            Arrange::Colour,
            Arrange::Rating,
            Arrange::Length,
            Arrange::Author,
        ] {
            let mut order = order_books(&books, &dates, arrange);
            order.sort_unstable();
            let expect: Vec<usize> = (0..books.len()).collect();
            assert_eq!(order, expect, "arrangement {arrange:?} lost a book");
        }
        let colour = order_books(&books, &dates, Arrange::Colour);
        let hues: Vec<f64> = colour.iter().map(|i| hue(books[*i].colour)).collect();
        let mut sorted = hues.clone();
        sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert_eq!(hues, sorted, "the rainbow stack is not in hue order");
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
