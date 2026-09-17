//! Cover cards for library grids (click → float, Ctrl+click → full page).
//!
//! Every card is the **same pixel width** so long titles cannot break the grid.
//! Cover is 1.6:1 portrait; title + author sit below and ellipsize inside that width.

use crate::models::Book;
use gtk::gdk::ModifierType;
use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::Path;
use std::rc::Rc;

/// Cache key: the original cover path plus the size it was decoded at.
type CoverKey = (String, i32, i32);

/// How many decoded covers to keep.
///
/// Each entry is roughly `w * h * 4` bytes — about 102 KB at the grid's
/// 128×204 — so this is a memory budget more than a count: 300 covers is
/// ~30 MB, which is a fair share of a 4 GB machine and several screens' worth
/// of scrolling.
///
/// The number matters more than it used to. Before A0 step 5's fix, the
/// preloader stopped after 24 covers, so nothing ever approached this bound in
/// practice. Now that every cover in a library gets queued, a 2,000-book
/// library really would try to hold 2,000 textures without it.
const COVER_CACHE_MAX: usize = 300;

/// Decoded cover textures, with least-recently-used eviction.
///
/// Without a cache every navigation re-reads and re-scales each cover PNG from
/// disk, which is what made scrolling and page switches feel heavy. GDK
/// textures are reference-counted and live on the GPU, so re-using them is
/// both faster and lighter than holding pixbufs.
///
/// The eviction is the interesting part. The obvious bound — "if it is too
/// big, empty it" — is wrong in the one case that matters: it throws away the
/// covers currently on screen along with everything else, so crossing the
/// limit makes the whole visible grid decode again. Dropping only the
/// least-recently-used entry keeps what the user is looking at.
///
/// Generic over the value purely so the eviction logic is testable: a
/// `gdk::Texture` cannot be constructed without an initialised GTK display,
/// and CI has none. The tests below exercise this with plain integers, which
/// is the whole of the interesting behaviour.
struct CoverCache<T> {
    map: HashMap<CoverKey, T>,
    /// Keys oldest-first. A `Vec` rather than a linked list on purpose: at 300
    /// entries the shuffle is a few hundred pointer moves and happens once per
    /// cover, which is nothing next to decoding a PNG.
    order: Vec<CoverKey>,
}

// Hand-written rather than derived: `#[derive(Default)]` would demand
// `T: Default`, which a texture is not.
impl<T> Default for CoverCache<T> {
    fn default() -> Self {
        Self {
            map: HashMap::new(),
            order: Vec::new(),
        }
    }
}

impl<T: Clone> CoverCache<T> {
    fn get(&mut self, key: &CoverKey) -> Option<T> {
        let texture = self.map.get(key)?.clone();
        self.touch(key);
        Some(texture)
    }

    fn contains(&self, key: &CoverKey) -> bool {
        self.map.contains_key(key)
    }

    /// Mark a key as most recently used.
    fn touch(&mut self, key: &CoverKey) {
        if let Some(i) = self.order.iter().position(|k| k == key) {
            let k = self.order.remove(i);
            self.order.push(k);
        }
    }

    fn insert(&mut self, key: CoverKey, texture: T) {
        if self.map.insert(key.clone(), texture).is_none() {
            self.order.push(key);
        } else {
            self.touch(&key);
        }
        while self.order.len() > COVER_CACHE_MAX {
            // remove(0) is O(n) but n is 300 and this runs once per insert
            // past the bound; clarity wins over a VecDeque here.
            let oldest = self.order.remove(0);
            self.map.remove(&oldest);
        }
    }

    fn retain_paths(&mut self, drop_path: &str) {
        self.map.retain(|(p, _, _), _| p != drop_path);
        self.order.retain(|(p, _, _)| p != drop_path);
    }
}

thread_local! {
    static COVER_CACHE: RefCell<CoverCache<gtk::gdk::Texture>> =
        RefCell::new(CoverCache::default());
}

/// A cover frame showing a placeholder, waiting for its texture.
struct PendingFrame {
    key: (String, i32, i32),
    /// Weak: the page can be destroyed before the decode finishes.
    frame: gtk::glib::WeakRef<gtk::Box>,
}

thread_local! {
    /// Frames waiting on a decode. UI-owned and only ever touched on the main
    /// thread, which is exactly what `thread_local!` is for
    /// (`docs/pitfalls.md` §4e).
    static PENDING_FRAMES: RefCell<Vec<PendingFrame>> = const { RefCell::new(Vec::new()) };
}

/// Drop cached textures for a cover that changed or was deleted.
pub fn invalidate_cover_cache(path: &Path) {
    let key = path.to_string_lossy().to_string();
    COVER_CACHE.with(|c| c.borrow_mut().retain_paths(&key));
}

/// Whether a cover is already decoded at this size.
///
/// Lets the preloader (A0 step 5) skip work the grid has already done, so
/// re-running it after a small scroll costs almost nothing.
pub fn is_cover_cached(path: &Path, w: i32, h: i32) -> bool {
    let key = (path.to_string_lossy().to_string(), w, h);
    // Deliberately does not touch the LRU order: this is the preloader asking
    // "do I need to decode this?", not the user viewing a cover. Counting it
    // as a use would let a background sweep reorder the cache away from what
    // is actually on screen.
    COVER_CACHE.with(|c| c.borrow().contains(&key))
}

/// Store a cover decoded off the UI thread by the preloader.
///
/// The worker cannot build the texture — `gdk::Texture` is a GObject and
/// belongs to the main thread — so it sends raw RGBA and the wrap happens
/// here. That wrap is a pointer copy, not a decode, which is the whole point:
/// the expensive part already happened on the worker.
///
/// Ignores anything already cached so a preload can never replace a texture
/// the grid is currently showing.
pub fn cache_decoded_cover(decoded: &crate::preload::DecodedCover) {
    let (w, h) = (decoded.width, decoded.height);
    let expected = (w as usize) * (h as usize) * 4;
    // A mismatch here would be a garbled image or a crash inside GDK rather
    // than a visible bug, so refuse instead of trusting the buffer.
    if w <= 0 || h <= 0 || decoded.rgba.len() != expected {
        return;
    }
    let key = (decoded.cover.to_string_lossy().to_string(), w, h);
    let inserted = COVER_CACHE.with(|c| {
        let mut cache = c.borrow_mut();
        if cache.contains(&key) {
            return None;
        }
        let bytes = gtk::glib::Bytes::from(&decoded.rgba[..]);
        let texture: gtk::gdk::Texture = gtk::gdk::MemoryTexture::new(
            w,
            h,
            gtk::gdk::MemoryFormat::R8g8b8a8,
            &bytes,
            (w as usize) * 4,
        )
        .upcast();
        cache.insert(key.clone(), texture.clone());
        Some((key, texture))
    });

    // Outside the cache borrow: the swap re-enters nothing, but holding a
    // RefCell borrow across widget work is how re-entrancy panics start.
    if let Some((key, texture)) = inserted {
        swap_in_cover(&key, &texture);
    }
}

/// Cover width (px). Height = width × 1.6 (standard ebook portrait).
pub const COVER_W: i32 = 128;
pub const COVER_ASPECT: f64 = 1.6;
pub const COVER_H: i32 = ((COVER_W as f64) * COVER_ASPECT) as i32; // 204

/// Full card width — locked; labels cannot grow past this.
pub const CARD_W: i32 = COVER_W;
/// Space reserved under the cover for title (2 lines) + author.
const TITLE_AREA_H: i32 = 36;
const AUTHOR_AREA_H: i32 = 18;
const GAP: i32 = 6;
pub const CARD_H: i32 = COVER_H + GAP + TITLE_AREA_H + AUTHOR_AREA_H;

/// Columns in the library grid (uniform cells).
const GRID_COLS: i32 = 6;
const COL_SPACING: u32 = 16;
const ROW_SPACING: u32 = 20;

/// One bookshelf card: fixed cover + title + author underneath.
pub fn build_book_card(
    book: &Book,
    on_full: impl Fn() + 'static,
    on_float: impl Fn() + 'static,
) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, GAP);
    card.add_css_class("kalam-book-card");
    card.set_hexpand(false);
    card.set_vexpand(false);
    card.set_halign(gtk::Align::Start);
    card.set_valign(gtk::Align::Start);
    // Lock both axes so long titles never widen the cell.
    card.set_size_request(CARD_W, CARD_H);
    card.set_overflow(gtk::Overflow::Hidden);

    // Deferred: a grid builds hundreds of these at once, so decoding here
    // would be the stall the preloader exists to remove.
    let cover = cover_widget_deferred(book.cover_path.as_deref(), COVER_W, COVER_H);
    cover.add_css_class("kalam-book-card-cover");
    cover.set_halign(gtk::Align::Center);

    let click = gtk::GestureClick::new();
    click.set_button(1);
    click.connect_released(move |gesture, _n, _x, _y| {
        let state = gesture.current_event_state();
        if state.contains(ModifierType::CONTROL_MASK) {
            on_full();
        } else {
            on_float();
        }
    });
    card.add_controller(click);
    card.set_cursor_from_name(Some("pointer"));
    // Full title/author available on hover even when ellipsized.
    card.set_tooltip_text(Some(&format!(
        "{}\n{}\n\nClick: float · Ctrl+click: full page",
        book.title,
        book.authors_display()
    )));

    // Text column clamped to COVER_W — labels ellipsize inside, never expand card.
    let text_col = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text_col.set_size_request(COVER_W, TITLE_AREA_H + AUTHOR_AREA_H);
    text_col.set_hexpand(false);
    text_col.set_vexpand(false);
    text_col.set_halign(gtk::Align::Center);
    text_col.set_overflow(gtk::Overflow::Hidden);

    let title = gtk::Label::new(Some(&book.title));
    title.add_css_class("kalam-book-card-title");
    title.set_halign(gtk::Align::Center);
    title.set_justify(gtk::Justification::Center);
    title.set_xalign(0.5);
    // Single-line ellipsis is the most reliable way to keep fixed width in GTK.
    // Two lines with wrap still often grow the parent; we use 2 lines capped in pixels.
    title.set_wrap(true);
    title.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    title.set_lines(2);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_hexpand(false);
    title.set_vexpand(false);
    title.set_size_request(COVER_W, TITLE_AREA_H);
    // Critical: natural width must not exceed cover width.
    title.set_width_chars(1);
    title.set_max_width_chars(1);

    let author = gtk::Label::new(Some(book.authors_display()));
    author.add_css_class("kalam-book-card-author");
    author.set_halign(gtk::Align::Center);
    author.set_xalign(0.5);
    author.set_ellipsize(gtk::pango::EllipsizeMode::End);
    author.set_single_line_mode(true);
    author.set_hexpand(false);
    author.set_vexpand(false);
    author.set_size_request(COVER_W, AUTHOR_AREA_H);
    author.set_width_chars(1);
    author.set_max_width_chars(1);

    text_col.append(&title);
    text_col.append(&author);

    card.append(&cover);
    card.append(&text_col);
    card
}

// ---------------------------------------------------------------------------
// A0 step 6 — build only the cards you can see
//
// The old grid built one card per book. At 2,000 books that is 2,000 cards
// before the page can appear (396 ms, measured in CI) and ~232 MB of memory,
// nearly all of it cover images held alive by cards that are scrolled far off
// screen.
//
// This builds ~30 instead: the rows the window is showing, plus a margin above
// and below. The grid is still a plain GtkGrid inside the same page, in the
// same scroller, so nothing about the layout changes -- no fixed header, no
// second scrollbar, the header still scrolls away with the page. Empty rows
// are held open by a spacer of the exact height the missing cards would have
// occupied, so the scrollbar is the same size and in the same place as before.
//
// On by default since 2026-09-04, after the user confirmed it on a real
// screen. `KALAM_NO_WINDOWED_GRID=1` restores the old build-every-card grid.
// ---------------------------------------------------------------------------

/// Is the windowed grid switched on? Default: yes.
///
/// Was opt-in via `KALAM_WINDOWED_GRID=1` while it had never been seen on a
/// real screen. The user checked it on 2026-09-04 — books all present, page
/// the right length, scrollbar steady — so it is now the default and
/// `KALAM_NO_WINDOWED_GRID=1` is the way back to building every card.
///
/// Same opt-*out* shape as `KALAM_NO_PRELOAD` and `KALAM_NO_WEBVIEW_POOL`,
/// deliberately: an escape hatch is only useful if it works the way the other
/// escape hatches in this codebase already work.
pub fn windowed_grid_enabled() -> bool {
    windowed_grid_enabled_for(std::env::var_os("KALAM_NO_WINDOWED_GRID").as_deref())
}

/// The rule as a pure function, so it can be tested without touching
/// process-wide environment state mid-run.
fn windowed_grid_enabled_for(disable: Option<&std::ffi::OsStr>) -> bool {
    match disable {
        None => true,
        // Unset-but-present and an explicit "0" both mean "leave it on", so a
        // stray `KALAM_NO_WINDOWED_GRID=` in a shell profile cannot silently
        // switch it off. Matches `preload_enabled_for` exactly.
        Some(v) => v.is_empty() || v == "0",
    }
}

/// How many rows of cards a library needs.
fn row_count(books: usize) -> i32 {
    if books == 0 {
        return 0;
    }
    ((books as i32) + GRID_COLS - 1) / GRID_COLS
}

/// Height in pixels of one row, including the gap beneath it.
const ROW_PITCH: i32 = CARD_H + ROW_SPACING as i32;

/// Extra rows built above and below the visible ones.
///
/// Scrolling can outrun the fill: GTK hands us the new scroll position and we
/// build cards in response, so a fast flick can reach rows that do not exist
/// yet and show blank space. A margin means we have already built what the
/// user is about to reach. Three rows is ~850 px of runway in each direction,
/// at a cost of ~36 extra cards.
const OVERSCAN_ROWS: i32 = 3;

/// Which rows to build for a given scroll position.
///
/// Returns `(first_row, last_row)` inclusive. Split out from the widget code
/// because it is the part that can be wrong in an interesting way, and it is
/// pure arithmetic -- no display needed, so CI can test it.
///
/// `scroll_top` is how far the page is scrolled down, `viewport_h` the visible
/// height, both in pixels.
fn visible_rows(scroll_top: f64, viewport_h: f64, total_rows: i32) -> (i32, i32) {
    if total_rows <= 0 {
        return (0, -1);
    }

    // A viewport height of 0 means GTK has not laid out yet. Guessing "no rows
    // are visible" would leave the page blank until the first scroll, so build
    // the top of the grid and let the first real measurement correct it.
    let viewport_h = if viewport_h <= 1.0 {
        1000.0
    } else {
        viewport_h
    };
    let scroll_top = scroll_top.max(0.0);

    let first = (scroll_top / ROW_PITCH as f64).floor() as i32 - OVERSCAN_ROWS;
    let last = ((scroll_top + viewport_h) / ROW_PITCH as f64).ceil() as i32 + OVERSCAN_ROWS;

    (first.max(0), last.min(total_rows - 1))
}

/// Total pixel height of the whole grid, from the book count alone.
///
/// Deliberately independent of which cards are mounted. The first attempt let
/// a `GtkGrid` derive its own height from its children, and since the whole
/// point is to remove most of those children, the height changed as the user
/// scrolled -- that was the jumping scrollbar.
fn grid_height(books: usize) -> i32 {
    let rows = row_count(books);
    if rows <= 0 {
        return 0;
    }
    rows * ROW_PITCH - ROW_SPACING as i32
}

/// Where card `index` sits, in pixels from the grid's top-left.
fn card_position(index: usize) -> (i32, i32) {
    let col = (index as i32) % GRID_COLS;
    let row = (index as i32) / GRID_COLS;
    (col * (CARD_W + COL_SPACING as i32), row * ROW_PITCH)
}

/// The windowed grid: cards placed at fixed pixel positions, and only the
/// visible ones exist.
///
/// **Why not a `GtkGrid`.** The first attempt used one, with a tall spacer
/// widget in the last row to hold the full height open. Every part of that was
/// wrong, and all three bugs the user hit came from it:
///
///  - A grid row is as tall as its tallest child, so a 6,532 px spacer made
///    the last *row* 6,532 px tall. The page scrolled about twice as far as it
///    should, and the extra was blank.
///  - The spacer sat in column 0 of the last row, so the book belonging in
///    that cell had nowhere to go. Books went missing.
///  - Rows holding no mounted cards collapsed to zero height. As cards mounted
///    and unmounted during a scroll the row heights kept changing, so the
///    total height changed under the scrollbar — that is the jumping.
///
/// The root mistake was letting the container decide the geometry while
/// removing the children it derives that geometry from. `GtkFixed` does not
/// do that: every card is placed at an exact x/y, and the overall size is set
/// once from the book count. Positions therefore do not depend on which cards
/// happen to be mounted, which is precisely the property this needs.
///
/// Layout maths matches the old grid exactly — same `GRID_COLS`, same
/// spacings, same card size — so the result is pixel-identical to what the
/// user sees today.
fn build_windowed_grid(
    books: &[Book],
    on_full: impl Fn(i64) + Clone + 'static,
    on_float: impl Fn(i64) + Clone + 'static,
) -> gtk::Box {
    let grid = gtk::Fixed::new();
    grid.set_halign(gtk::Align::Start);
    grid.set_valign(gtk::Align::Start);
    grid.set_hexpand(false);
    grid.set_vexpand(false);
    grid.add_css_class("kalam-book-grid");

    let shell = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    shell.set_halign(gtk::Align::Fill);
    shell.set_valign(gtk::Align::Start);
    shell.set_hexpand(true);
    shell.set_vexpand(false);
    shell.add_css_class("kalam-book-grid-shell");

    crate::timing::span("grid_build");

    let total_rows = row_count(books.len());

    // Ask for the exact size the full grid would occupy, once, from the book
    // count alone. Nothing here depends on which cards are mounted, so the
    // scrollbar cannot change length while scrolling.
    if total_rows > 0 {
        let full_w = GRID_COLS * CARD_W + (GRID_COLS - 1) * COL_SPACING as i32;
        grid.set_size_request(full_w, grid_height(books.len()));
    }

    // Cards currently in the grid, by book index, so a scroll can tell what to
    // add and what to remove without rebuilding everything.
    let mounted: Rc<RefCell<HashMap<usize, gtk::Box>>> = Rc::new(RefCell::new(HashMap::new()));

    let books_owned: Rc<Vec<Book>> = Rc::new(books.to_vec());

    // One closure does all the work: given a scroll position, make the grid
    // hold exactly the cards for the visible rows.
    let sync: Rc<dyn Fn(f64, f64)> = {
        let grid = grid.clone();
        let mounted = mounted.clone();
        let books = books_owned.clone();
        let on_full = on_full.clone();
        let on_float = on_float.clone();
        Rc::new(move |scroll_top: f64, viewport_h: f64| {
            let (first, last) = visible_rows(scroll_top, viewport_h, total_rows);
            if last < first {
                return;
            }

            let want_from = (first as usize) * GRID_COLS as usize;
            let want_to = (((last + 1) as usize) * GRID_COLS as usize).min(books.len());

            let mut mounted = mounted.borrow_mut();

            // Remove cards that scrolled out. This is what frees the cover
            // images: dropping the card drops the only remaining hold on the
            // texture once the cache has evicted it.
            mounted.retain(|&i, cell| {
                if i >= want_from && i < want_to {
                    return true;
                }
                grid.remove(cell);
                false
            });

            // Add the ones that scrolled in.
            let mut added = Vec::new();
            for i in want_from..want_to {
                if mounted.contains_key(&i) {
                    continue;
                }
                let book = &books[i];
                let id = book.id;
                let f1 = on_full.clone();
                let f2 = on_float.clone();
                let card = build_book_card(book, move || f1(id), move || f2(id));

                let cell = gtk::Box::new(gtk::Orientation::Vertical, 0);
                cell.set_size_request(CARD_W, CARD_H);
                cell.set_hexpand(false);
                cell.set_vexpand(false);
                cell.set_halign(gtk::Align::Start);
                cell.append(&card);

                let (x, y) = card_position(i);
                grid.put(&cell, x as f64, y as f64);
                mounted.insert(i, cell);
                added.push(i);
            }

            // Decode covers for what we just mounted, nearest first. Only the
            // new arrivals -- re-warming the whole library on every scroll
            // step is the cost this change exists to remove.
            if let Some(&start) = added.first() {
                let slice_end = (want_to).min(books.len());
                crate::preload::warm_books(&books[start..slice_end], 0, COVER_W, COVER_H);
            }
        })
    };

    // First fill. The real viewport height is unknown until GTK lays out, so
    // `visible_rows` assumes a sensible window; the scroll handler corrects it
    // as soon as there is a real measurement.
    sync(0.0, 0.0);

    shell.append(&grid);
    crate::timing::span_end("grid_build");
    crate::timing::note("grid_cards", mounted.borrow().len());
    crate::timing::note("grid_cards_total", books.len());

    drop_dead_pending_frames();

    // Follow the page's own scrollbar. The grid is several levels below the
    // ScrolledWindow, so we walk up to find it rather than requiring the pages
    // to pass it in -- that keeps this a drop-in replacement for the old grid
    // and means the three call sites do not change at all.
    let sync_for_map = sync.clone();
    shell.connect_map(move |shell| {
        let Some(scroller) = enclosing_scroller(shell) else {
            return;
        };
        let adj = scroller.vadjustment();

        // Sync once now that there are real measurements...
        sync_for_map(adj.value(), adj.page_size());

        // ...and on every scroll after that.
        let s = sync_for_map.clone();
        adj.connect_value_changed(move |adj| {
            s(adj.value(), adj.page_size());
        });

        // The viewport height changes when the window is resized, which
        // changes how many rows are visible.
        let s2 = sync_for_map.clone();
        adj.connect_page_size_notify(move |adj| {
            s2(adj.value(), adj.page_size());
        });
    });

    shell
}

/// Walk up the widget tree to the `ScrolledWindow` this grid lives in.
///
/// The grid sits four levels deep (scroller > content host > page > list box >
/// grid), and the depth differs between the three pages that use it. Walking
/// up is what lets this stay a drop-in replacement.
fn enclosing_scroller(widget: &impl IsA<gtk::Widget>) -> Option<gtk::ScrolledWindow> {
    let mut node = widget.as_ref().parent();
    while let Some(w) = node {
        if let Ok(scroller) = w.clone().downcast::<gtk::ScrolledWindow>() {
            return Some(scroller);
        }
        node = w.parent();
    }
    None
}

/// Uniform grid of fixed-size cards (same cell width for every book).
pub fn build_book_grid(
    books: &[Book],
    on_full: impl Fn(i64) + Clone + 'static,
    on_float: impl Fn(i64) + Clone + 'static,
) -> gtk::Box {
    // A0 step 6: build only the rows on screen. Same layout, same scrollbar --
    // see `build_windowed_grid`. `KALAM_NO_WINDOWED_GRID=1` restores the old
    // build-every-card behaviour.
    if windowed_grid_enabled() {
        return build_windowed_grid(books, on_full, on_float);
    }

    // GtkGrid with homogeneous columns = true grid view.
    let grid = gtk::Grid::new();
    grid.set_column_spacing(COL_SPACING);
    grid.set_row_spacing(ROW_SPACING);
    grid.set_column_homogeneous(true);
    grid.set_row_homogeneous(false);
    grid.set_halign(gtk::Align::Start);
    grid.set_valign(gtk::Align::Start);
    grid.set_hexpand(true);
    grid.set_vexpand(false);
    grid.add_css_class("kalam-book-grid");

    // Shell keeps the whole grid from being stretched by the parent but allows fill.
    let shell = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    shell.set_halign(gtk::Align::Fill);
    shell.set_valign(gtk::Align::Start);
    shell.set_hexpand(true);
    shell.set_vexpand(false);
    shell.add_css_class("kalam-book-grid-shell");

    // A0 step 5: the number that says whether deferring the covers worked.
    // Before, this loop decoded every cover before it could return.
    crate::timing::span("grid_build");

    for (i, book) in books.iter().enumerate() {
        let id = book.id;
        let f1 = on_full.clone();
        let f2 = on_float.clone();
        let card = build_book_card(book, move || f1(id), move || f2(id));

        // Cell wrapper enforces CARD_W so Grid homogeneous cells stay equal.
        let cell = gtk::Box::new(gtk::Orientation::Vertical, 0);
        cell.set_size_request(CARD_W, CARD_H);
        cell.set_hexpand(false);
        cell.set_vexpand(false);
        cell.set_halign(gtk::Align::Start);
        cell.append(&card);

        let col = (i as i32) % GRID_COLS;
        let row = (i as i32) / GRID_COLS;
        grid.attach(&cell, col, row, 1, 1);
    }

    shell.append(&grid);
    crate::timing::span_end("grid_build");
    crate::timing::note("grid_cards", books.len());

    // Frames from pages that have since been destroyed. Covers that never
    // decode -- a missing or corrupt file -- are never swapped, so without
    // this their entries would accumulate for the life of the process.
    drop_dead_pending_frames();

    // Every card above is showing a placeholder. Start decoding, nearest
    // first, so the top of the grid fills in while the user is still looking
    // at it. This calls back into `cache_decoded_cover`, which swaps each
    // image into its frame as it lands.
    crate::preload::warm_books(books, 0, COVER_W, COVER_H);

    shell
}

pub fn build_book_card_selectable(
    book: &Book,
    is_selected: bool,
    on_full: impl Fn() + 'static,
    on_float: impl Fn() + 'static,
) -> gtk::Box {
    let card = build_book_card(book, on_full, on_float);
    if is_selected {
        card.add_css_class("kalam-card-selected");
        let check = gtk::Label::new(Some("✓ Selected"));
        check.add_css_class("kalam-tag-count");
        check.set_halign(gtk::Align::Center);
        check.set_valign(gtk::Align::Start);
        check.set_margin_top(4);

        let overlay = gtk::Overlay::new();
        overlay.set_child(Some(&card));
        overlay.add_overlay(&check);

        let outer = gtk::Box::new(gtk::Orientation::Vertical, 0);
        outer.set_size_request(CARD_W, CARD_H);
        outer.append(&overlay);
        return outer;
    }
    card
}

fn build_windowed_grid_selectable(
    books: &[Book],
    is_selected: impl Fn(i64) -> bool + Clone + 'static,
    on_full: impl Fn(i64) + Clone + 'static,
    on_float: impl Fn(i64) + Clone + 'static,
) -> gtk::Box {
    let grid = gtk::Fixed::new();
    grid.set_halign(gtk::Align::Start);
    grid.set_valign(gtk::Align::Start);
    grid.set_hexpand(false);
    grid.set_vexpand(false);
    grid.add_css_class("kalam-book-grid");

    let shell = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    shell.set_halign(gtk::Align::Fill);
    shell.set_valign(gtk::Align::Start);
    shell.set_hexpand(true);
    shell.set_vexpand(false);
    shell.add_css_class("kalam-book-grid-shell");

    crate::timing::span("grid_build");

    let total_rows = row_count(books.len());

    if total_rows > 0 {
        let full_w = GRID_COLS * CARD_W + (GRID_COLS - 1) * COL_SPACING as i32;
        grid.set_size_request(full_w, grid_height(books.len()));
    }

    let mounted: Rc<RefCell<HashMap<usize, gtk::Box>>> = Rc::new(RefCell::new(HashMap::new()));
    let books_owned: Rc<Vec<Book>> = Rc::new(books.to_vec());

    let sync: Rc<dyn Fn(f64, f64)> = {
        let grid = grid.clone();
        let mounted = mounted.clone();
        let books = books_owned.clone();
        let is_selected = is_selected.clone();
        let on_full = on_full.clone();
        let on_float = on_float.clone();
        Rc::new(move |scroll_top: f64, viewport_h: f64| {
            let (first, last) = visible_rows(scroll_top, viewport_h, total_rows);
            if last < first {
                return;
            }

            let want_from = (first as usize) * GRID_COLS as usize;
            let want_to = (((last + 1) as usize) * GRID_COLS as usize).min(books.len());

            let mut mounted = mounted.borrow_mut();

            mounted.retain(|&i, cell| {
                if i >= want_from && i < want_to {
                    return true;
                }
                grid.remove(cell);
                false
            });

            let mut added = Vec::new();
            for i in want_from..want_to {
                if mounted.contains_key(&i) {
                    continue;
                }
                let book = &books[i];
                let id = book.id;
                let f1 = on_full.clone();
                let f2 = on_float.clone();
                let sel = is_selected(id);
                let card = build_book_card_selectable(book, sel, move || f1(id), move || f2(id));

                let cell = gtk::Box::new(gtk::Orientation::Vertical, 0);
                cell.set_size_request(CARD_W, CARD_H);
                cell.set_hexpand(false);
                cell.set_vexpand(false);
                cell.set_halign(gtk::Align::Start);
                cell.append(&card);

                let (x, y) = card_position(i);
                grid.put(&cell, x as f64, y as f64);
                mounted.insert(i, cell);
                added.push(i);
            }

            if let Some(&start) = added.first() {
                let slice_end = (want_to).min(books.len());
                crate::preload::warm_books(&books[start..slice_end], 0, COVER_W, COVER_H);
            }
        })
    };

    sync(0.0, 0.0);

    shell.append(&grid);
    crate::timing::span_end("grid_build");
    crate::timing::note("grid_cards", mounted.borrow().len());
    crate::timing::note("grid_cards_total", books.len());

    drop_dead_pending_frames();

    let sync_for_map = sync.clone();
    shell.connect_map(move |shell| {
        let Some(scroller) = enclosing_scroller(shell) else {
            return;
        };
        let adj = scroller.vadjustment();

        sync_for_map(adj.value(), adj.page_size());

        let s = sync_for_map.clone();
        adj.connect_value_changed(move |adj| {
            s(adj.value(), adj.page_size());
        });

        let s2 = sync_for_map.clone();
        adj.connect_page_size_notify(move |adj| {
            s2(adj.value(), adj.page_size());
        });
    });

    shell
}

pub fn build_book_grid_selectable(
    books: &[Book],
    is_selected: impl Fn(i64) -> bool + Clone + 'static,
    on_full: impl Fn(i64) + Clone + 'static,
    on_float: impl Fn(i64) + Clone + 'static,
) -> gtk::Box {
    if windowed_grid_enabled() {
        return build_windowed_grid_selectable(books, is_selected, on_full, on_float);
    }

    let grid = gtk::Grid::new();
    grid.set_column_spacing(COL_SPACING);
    grid.set_row_spacing(ROW_SPACING);
    grid.set_column_homogeneous(true);
    grid.set_row_homogeneous(false);
    grid.set_halign(gtk::Align::Start);
    grid.set_valign(gtk::Align::Start);
    grid.set_hexpand(true);
    grid.set_vexpand(false);
    grid.add_css_class("kalam-book-grid");

    let shell = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    shell.set_halign(gtk::Align::Fill);
    shell.set_valign(gtk::Align::Start);
    shell.set_hexpand(true);
    shell.set_vexpand(false);
    shell.add_css_class("kalam-book-grid-shell");

    crate::timing::span("grid_build");

    for (i, book) in books.iter().enumerate() {
        let id = book.id;
        let f1 = on_full.clone();
        let f2 = on_float.clone();
        let card = build_book_card_selectable(book, is_selected(id), move || f1(id), move || f2(id));

        let cell = gtk::Box::new(gtk::Orientation::Vertical, 0);
        cell.set_size_request(CARD_W, CARD_H);
        cell.set_hexpand(false);
        cell.set_vexpand(false);
        cell.set_halign(gtk::Align::Start);
        cell.append(&card);

        let col = (i as i32) % GRID_COLS;
        let row = (i as i32) / GRID_COLS;
        grid.attach(&cell, col, row, 1, 1);
    }

    shell.append(&grid);
    crate::timing::span_end("grid_build");
    crate::timing::note("grid_cards", books.len());

    drop_dead_pending_frames();
    crate::preload::warm_books(books, 0, COVER_W, COVER_H);

    shell
}

/// Fixed `w`×`h` cover (always the same size — with or without an image).
///
/// Decodes on the spot. Right for the one-or-two covers on a detail page,
/// where a placeholder that fills in a moment later would just look like a
/// flicker. Grids want [`cover_widget_deferred`] instead.
pub fn cover_widget(path: Option<&Path>, w: i32, h: i32) -> gtk::Widget {
    let frame = new_cover_frame(w, h);

    if let Some(path) = path {
        if path.is_file() {
            if let Some(picture) = scaled_cover_picture(path, w, h) {
                frame.append(&picture);
                return frame.upcast();
            }
        }
    }

    frame.append(&placeholder_for(w, h));
    frame.upcast()
}

/// Like [`cover_widget`], but never decodes on the UI thread.
///
/// A0 step 5, and the half of step 3 that was deferred to here: an uncached
/// cover gets a placeholder **immediately** and the frame is recorded, so
/// [`swap_in_cover`] can fill it once a worker has decoded the image.
///
/// This is what makes a big grid cheap. A 400-book page used to decode 400
/// covers on the UI thread before it could show anything; now it shows
/// straight away and the images arrive as they are ready.
pub fn cover_widget_deferred(path: Option<&Path>, w: i32, h: i32) -> gtk::Widget {
    // A/B escape hatch: with the preloader off nothing would ever fill these
    // frames, so fall all the way back to the old synchronous behaviour rather
    // than leaving a grid of permanent placeholders.
    if !crate::preload::preload_enabled() {
        return cover_widget(path, w, h);
    }

    let frame = new_cover_frame(w, h);

    if let Some(path) = path {
        if path.is_file() {
            let key = (path.to_string_lossy().to_string(), w, h);
            // Already decoded: use it now, no placeholder flash.
            if let Some(texture) = COVER_CACHE.with(|c| c.borrow_mut().get(&key)) {
                frame.append(&build_picture(&texture, w, h));
                return frame.upcast();
            }
            frame.append(&placeholder_for(w, h));
            PENDING_FRAMES.with(|p| {
                p.borrow_mut().push(PendingFrame {
                    key,
                    frame: frame.downgrade(),
                });
            });
            return frame.upcast();
        }
    }

    frame.append(&placeholder_for(w, h));
    frame.upcast()
}

/// Forget frames whose widgets have been destroyed.
fn drop_dead_pending_frames() {
    PENDING_FRAMES.with(|p| {
        p.borrow_mut()
            .retain(|entry| entry.frame.upgrade().is_some());
    });
}

fn new_cover_frame(w: i32, h: i32) -> gtk::Box {
    let frame = gtk::Box::new(gtk::Orientation::Vertical, 0);
    frame.add_css_class("kalam-cover-frame");
    frame.set_size_request(w, h);
    frame.set_hexpand(false);
    frame.set_vexpand(false);
    frame.set_halign(gtk::Align::Center);
    frame.set_valign(gtk::Align::Start);
    frame.set_overflow(gtk::Overflow::Hidden);
    frame
}

fn placeholder_for(w: i32, h: i32) -> gtk::Box {
    let placeholder = gtk::Box::new(gtk::Orientation::Vertical, 0);
    placeholder.add_css_class("kalam-cover-placeholder");
    placeholder.set_size_request(w, h);
    placeholder.set_hexpand(false);
    placeholder.set_vexpand(false);
    placeholder
}

/// Replace the placeholder in every frame waiting on this cover.
///
/// Frames are held **weakly**: a page can be torn down long before its covers
/// finish decoding, and a strong reference here would both leak the widget and
/// let a preload write into a dead page. Dead entries are dropped on the way
/// past, which is what keeps the list from growing across navigations —
/// covers that never decode (a missing or corrupt file) are only reaped this
/// way, since nothing else will ever come back for them.
fn swap_in_cover(key: &(String, i32, i32), texture: &gtk::gdk::Texture) {
    PENDING_FRAMES.with(|p| {
        let mut pending = p.borrow_mut();
        pending.retain(|entry| {
            let Some(frame) = entry.frame.upgrade() else {
                return false; // page is gone
            };
            if &entry.key != key {
                return true; // waiting on a different cover
            }
            while let Some(child) = frame.first_child() {
                frame.remove(&child);
            }
            frame.append(&build_picture(texture, key.1, key.2));
            false
        });
    });
}

fn scaled_cover_picture(path: &Path, w: i32, h: i32) -> Option<gtk::Picture> {
    // Cache key stays the *original* cover path so invalidation on a cover
    // change/replacement keeps working exactly as before.
    let key = (path.to_string_lossy().to_string(), w, h);

    // Serve from cache when we have already decoded this cover at this size.
    if let Some(texture) = COVER_CACHE.with(|c| c.borrow_mut().get(&key)) {
        return Some(build_picture(&texture, w, h));
    }

    // Once imported, we generate a tiny thumbnail (cache/thumbs/<uuid>.png) so
    // the grid does not decode the full 1000×1500+ cover on the UI thread. Use
    // it whenever it exists and this slot is small enough to fit without
    // upscaling; large slots (book page, author photo) still decode the cover.
    // Keep the thumbnail PathBuf alive for the whole call so the borrow below
    // outlives it (an ephemeral Option<PathBuf> would drop before decode).
    let thumb = thumb_for_slot(path, w, h).filter(|p| p.is_file());
    let decode_path: &Path = thumb.as_deref().unwrap_or(path);

    let texture = decode_cover(decode_path, w, h)?;
    COVER_CACHE.with(|c| c.borrow_mut().insert(key, texture.clone()));
    Some(build_picture(&texture, w, h))
}

/// The thumbnail path to decode for a cover slot, or `None` when this slot is
/// too large to use the thumbnail (never upscale it).
fn thumb_for_slot(cover: &Path, w: i32, h: i32) -> Option<std::path::PathBuf> {
    if w > crate::thumbs::THUMB_W as i32 || h > crate::thumbs::THUMB_H as i32 {
        return None;
    }
    crate::paths::thumbnail_for_cover(cover)
}

fn decode_cover(path: &Path, w: i32, h: i32) -> Option<gtk::gdk::Texture> {
    use gdk_pixbuf::{InterpType, Pixbuf};

    let pixbuf = Pixbuf::from_file_at_scale(path, w, h, false).ok()?;
    let pixbuf = if pixbuf.width() != w || pixbuf.height() != h {
        pixbuf.scale_simple(w, h, InterpType::Bilinear)?
    } else {
        pixbuf
    };
    Some(gtk::gdk::Texture::for_pixbuf(&pixbuf))
}

fn build_picture(texture: &gtk::gdk::Texture, w: i32, h: i32) -> gtk::Picture {
    let picture = gtk::Picture::for_paintable(texture);
    picture.set_content_fit(gtk::ContentFit::Fill);
    picture.set_can_shrink(true);
    picture.set_size_request(w, h);
    picture.set_hexpand(false);
    picture.set_vexpand(false);
    picture.set_halign(gtk::Align::Fill);
    picture.set_valign(gtk::Align::Fill);
    picture.add_css_class("kalam-cover-img");
    picture
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(name: &str) -> CoverKey {
        (name.to_string(), COVER_W, COVER_H)
    }

    #[test]
    fn a_hit_keeps_an_entry_alive_and_evicts_the_untouched_one() {
        // The property that matters: the covers on screen must survive an
        // overflow. The previous bound cleared the whole cache, so crossing it
        // made the visible grid decode itself again -- a stall exactly when
        // the cache was supposed to be helping.
        let mut cache: CoverCache<u32> = CoverCache::default();
        for i in 0..COVER_CACHE_MAX {
            cache.insert(key(&format!("cover-{i}")), i as u32);
        }
        assert_eq!(cache.map.len(), COVER_CACHE_MAX, "full, not over");

        // Look at the oldest entry, so it is no longer the oldest.
        assert_eq!(cache.get(&key("cover-0")), Some(0));

        cache.insert(key("new-one"), 999);

        assert_eq!(cache.map.len(), COVER_CACHE_MAX, "bounded");
        assert_eq!(
            cache.get(&key("cover-0")),
            Some(0),
            "the recently used entry survived"
        );
        assert!(
            !cache.contains(&key("cover-1")),
            "the least recently used entry was dropped instead"
        );
    }

    #[test]
    fn the_cache_never_grows_past_its_bound() {
        // Before A0 step 5's fix the preloader stopped at 24 covers, so this
        // bound was never approached. Now every cover in a library is queued,
        // and a 2,000-book library would hold 2,000 textures without this.
        let mut cache: CoverCache<u32> = CoverCache::default();
        for i in 0..2000 {
            cache.insert(key(&format!("cover-{i}")), i as u32);
        }
        assert_eq!(cache.map.len(), COVER_CACHE_MAX);
        assert_eq!(
            cache.order.len(),
            COVER_CACHE_MAX,
            "the order list must be trimmed too, or it leaks on its own"
        );
        assert!(cache.contains(&key("cover-1999")), "kept the newest");
        assert!(!cache.contains(&key("cover-0")), "dropped the oldest");
    }

    #[test]
    fn re_inserting_the_same_key_does_not_grow_the_order_list() {
        // A duplicate insert that pushed to `order` again would slowly poison
        // eviction: the list would fill with stale copies of live keys.
        let mut cache: CoverCache<u32> = CoverCache::default();
        for _ in 0..10 {
            cache.insert(key("same"), 1);
        }
        assert_eq!(cache.map.len(), 1);
        assert_eq!(cache.order.len(), 1, "no duplicate bookkeeping");
    }

    #[test]
    fn invalidating_a_path_clears_every_size_and_its_bookkeeping() {
        // A replaced cover must not linger at any size, and the order list has
        // to forget it too -- a key left there would evict a live entry later.
        let mut cache: CoverCache<u32> = CoverCache::default();
        cache.insert(("/covers/a.png".into(), 128, 204), 1);
        cache.insert(("/covers/a.png".into(), 256, 408), 2);
        cache.insert(("/covers/b.png".into(), 128, 204), 3);

        cache.retain_paths("/covers/a.png");

        assert_eq!(cache.map.len(), 1, "both sizes of a.png are gone");
        assert_eq!(cache.order.len(), 1, "order list pruned as well");
        assert!(cache.contains(&("/covers/b.png".into(), 128, 204)));
    }

    #[test]
    fn a_probe_does_not_count_as_a_use() {
        // `is_cover_cached` is the preloader asking whether it needs to
        // decode. If that counted as a use, a background sweep over a whole
        // library would reorder the cache away from what is on screen.
        let mut cache: CoverCache<u32> = CoverCache::default();
        cache.insert(key("first"), 1);
        cache.insert(key("second"), 2);

        assert!(cache.contains(&key("first")));

        assert_eq!(
            cache.order.first(),
            Some(&key("first")),
            "still the oldest -- contains() must not touch the order"
        );
    }

    // -- A0 step 6: which rows the windowed grid builds ---------------------
    //
    // Pure arithmetic, so CI can check it without a display. This is the part
    // that can be wrong in an interesting way: too few rows and the user sees
    // blank space, too many and the change saves nothing.

    /// Cards built for a row range, for readability in the assertions below.
    fn cards_in(range: (i32, i32)) -> i32 {
        let (first, last) = range;
        if last < first {
            return 0;
        }
        (last - first + 1) * GRID_COLS
    }

    #[test]
    fn a_full_library_builds_a_screenful_not_the_whole_thing() {
        // The entire point of the change. 2,000 books is 334 rows; at the top
        // of a 1000px window we must build a few dozen cards, not 2,000.
        let total = row_count(2000);
        assert_eq!(total, 334, "334 rows of 6 covers 2,000 books");

        let built = cards_in(visible_rows(0.0, 1000.0, total));
        assert!(
            built < 100,
            "built {built} cards at the top of a 2,000-book library -- the \
             windowing is not working"
        );
        assert!(
            built >= 30,
            "built only {built} cards for a 1000px window; a screenful is ~30 \
             plus overscan, so this would show blank rows"
        );
    }

    #[test]
    fn the_visible_window_covers_the_whole_viewport() {
        // Every row the user can actually see must be built, or there are
        // holes in the grid. Checked across the full scroll range rather than
        // at one position, because an off-by-one only shows at some offsets.
        let total = row_count(2000);
        let viewport = 1000.0;
        let mut top = 0.0;
        while top < (total * ROW_PITCH) as f64 {
            let (first, last) = visible_rows(top, viewport, total);

            let first_visible = (top / ROW_PITCH as f64).floor() as i32;
            let last_visible =
                (((top + viewport) / ROW_PITCH as f64).ceil() as i32 - 1).min(total - 1);

            assert!(
                first <= first_visible,
                "at scroll {top}: built from row {first} but row {first_visible} is on screen"
            );
            assert!(
                last >= last_visible,
                "at scroll {top}: built to row {last} but row {last_visible} is on screen"
            );
            top += 97.0; // deliberately not a multiple of the row pitch
        }
    }

    #[test]
    fn scrolling_never_skips_a_row() {
        // Consecutive scroll positions must overlap or touch. A gap means a
        // fast scroll could land on rows nobody ever built.
        let total = row_count(2000);
        let mut previous: Option<(i32, i32)> = None;
        let mut top = 0.0;
        while top < (total * ROW_PITCH) as f64 {
            let current = visible_rows(top, 1000.0, total);
            if let Some((_, prev_last)) = previous {
                assert!(
                    current.0 <= prev_last + 1,
                    "jumped from row {prev_last} to {} -- rows in between are never built",
                    current.0
                );
            }
            previous = Some(current);
            top += 137.0;
        }
    }

    #[test]
    fn the_edges_of_the_scroll_range_stay_in_bounds() {
        let total = row_count(2000);

        // Overscroll bounce can hand us a negative offset.
        assert_eq!(visible_rows(-500.0, 1000.0, total).0, 0);

        // At the bottom, never past the last row.
        let (_, last) = visible_rows((total * ROW_PITCH) as f64, 1000.0, total);
        assert!(last < total, "built row {last}, past the end at {total}");

        // Scrolled far past the end: no rows, and crucially no panic and no
        // reversed range that would be read as "build everything".
        let (first, last) = visible_rows(9_999_999.0, 1000.0, total);
        assert!(last < first, "expected an empty range, got {first}..{last}");
    }

    #[test]
    fn a_library_smaller_than_one_screen_still_works() {
        // The small-library case is the one the user actually has.
        assert_eq!(row_count(0), 0);
        let (first, last) = visible_rows(0.0, 1000.0, row_count(0));
        assert!(last < first, "empty library must build nothing");

        // 3 books is one row, and we must not build rows that do not exist.
        let total = row_count(3);
        assert_eq!(total, 1);
        assert_eq!(visible_rows(0.0, 1000.0, total), (0, 0));

        // 139 books -- the user's real library.
        let total = row_count(139);
        assert_eq!(total, 24);
        let (_, last) = visible_rows(0.0, 1000.0, total);
        assert!(last <= 23, "built row {last} of a 24-row library");
    }

    #[test]
    fn an_unmeasured_viewport_still_fills_the_top() {
        // GTK reports a page size of 0 before the first layout. Treating that
        // as "nothing is visible" would leave the page blank until the user
        // scrolled -- which looks exactly like a broken page.
        let total = row_count(2000);
        let built = cards_in(visible_rows(0.0, 0.0, total));
        assert!(
            built > 0,
            "an unmeasured viewport built nothing; the page would open blank"
        );
    }

    #[test]
    fn the_windowed_grid_is_on_unless_explicitly_switched_off() {
        use std::ffi::OsStr;
        // On by default since the user checked it on a real screen.
        assert!(windowed_grid_enabled_for(None), "must default to on");
        // A stray `KALAM_NO_WINDOWED_GRID=` in a shell profile must not
        // silently disable it -- same rule as KALAM_NO_PRELOAD.
        assert!(windowed_grid_enabled_for(Some(OsStr::new(""))));
        assert!(windowed_grid_enabled_for(Some(OsStr::new("0"))));
        // ...and the escape hatch works.
        assert!(!windowed_grid_enabled_for(Some(OsStr::new("1"))));
    }

    // -- The three bugs the user found on 2026-09-04 ------------------------
    //
    // All three came from using a GtkGrid row as a spacer. These pin the
    // geometry so that mistake cannot come back in another form.

    /// Height the plain (non-windowed) grid occupies, worked out the way
    /// GtkGrid does it: N rows of cards with N-1 gaps between them. The
    /// windowed grid must match this exactly, or the scrollbar is a lie.
    fn plain_grid_height(books: usize) -> i32 {
        let rows = row_count(books);
        if rows <= 0 {
            return 0;
        }
        rows * CARD_H + (rows - 1) * ROW_SPACING as i32
    }

    #[test]
    fn the_page_is_exactly_as_long_as_the_books_need() {
        // Bug 1: the page scrolled about twice as far as it should, and
        // everything past the books was blank. The spacer was 6,532 px and it
        // sat *inside* the last row, so the row became 6,532 px tall on top of
        // the rows above it.
        for books in [1, 6, 7, 144, 139, 2000] {
            assert_eq!(
                grid_height(books),
                plain_grid_height(books),
                "{books} books: windowed grid is {} px but the normal grid is \
                 {} px. A grid that is too long scrolls into blank space; too \
                 short and the last books are unreachable.",
                grid_height(books),
                plain_grid_height(books)
            );
        }
    }

    #[test]
    fn a_small_library_does_not_reserve_room_for_a_big_one() {
        // The user's actual complaint: 144 books, but the page scrolled as if
        // there were far more. Height must follow the real book count.
        let small = grid_height(144);
        let big = grid_height(2000);
        assert!(
            small < big / 10,
            "144 books reserved {small} px and 2,000 books {big} px -- a small \
             library is holding open space it does not need"
        );
        assert_eq!(small, 6796, "24 rows of cards and 23 gaps");
    }

    #[test]
    fn every_book_fits_inside_the_page() {
        // Bug 2: books went missing, because the spacer occupied column 0 of
        // the last row and the book belonging there had nowhere to go. Check
        // every card lands inside the reserved area.
        for books in [1, 6, 7, 144, 2000] {
            let height = grid_height(books);
            for i in 0..books {
                let (x, y) = card_position(i);
                assert!(
                    y + CARD_H <= height,
                    "{books} books: book {i} ends at {} px but the page is only \
                     {height} px tall -- it would be cut off or missing",
                    y + CARD_H
                );
                assert!(x >= 0 && y >= 0, "book {i} placed off-page at {x},{y}");
            }
        }
    }

    #[test]
    fn no_two_books_land_on_the_same_spot() {
        // The other half of bug 2. If two cards share a position one hides the
        // other, which also reads as a missing book.
        let mut seen = std::collections::HashSet::new();
        for i in 0..2000 {
            let pos = card_position(i);
            assert!(
                seen.insert(pos),
                "book {i} placed on top of another at {pos:?}"
            );
        }
    }

    #[test]
    fn the_page_length_never_changes_while_scrolling() {
        // Bug 3: the scrollbar and covers jumped. Rows with no mounted cards
        // collapsed to nothing, so the total height changed as cards came and
        // went. The height must depend only on the book count -- never on how
        // many cards happen to be on screen.
        let books = 2000;
        let expected = grid_height(books);
        let total = row_count(books);
        let mut top = 0.0;
        while top < (total * ROW_PITCH) as f64 {
            // Whatever is mounted at this scroll offset...
            let (first, last) = visible_rows(top, 1000.0, total);
            assert!(first >= 0 && last < total);
            // ...the page is still exactly as tall as it was.
            assert_eq!(
                grid_height(books),
                expected,
                "page height changed at scroll offset {top}"
            );
            top += 211.0;
        }
    }

    #[test]
    fn cards_line_up_in_the_same_columns_as_before() {
        // The layout must be pixel-identical to the grid the user already has,
        // since this is meant to be invisible.
        assert_eq!(card_position(0), (0, 0));
        // Second card: one card width plus one column gap to the right.
        assert_eq!(card_position(1), (CARD_W + COL_SPACING as i32, 0));
        // First card of the second row: back to x=0, down one row pitch.
        assert_eq!(card_position(GRID_COLS as usize), (0, ROW_PITCH));
        // Last column, then wrap.
        let last_col = (GRID_COLS - 1) as usize;
        assert_eq!(card_position(last_col).1, 0, "row 0 must not wrap early");
        assert_eq!(
            card_position(last_col + 1).0,
            0,
            "column {GRID_COLS} must wrap to the next row"
        );
    }

    #[test]
    fn an_empty_library_reserves_no_space() {
        assert_eq!(grid_height(0), 0, "an empty library must not scroll at all");
    }
}
