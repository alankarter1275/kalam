//! §57's result box, and decision 200's option menu.
//!
//! The box hangs under the bar and drops open when the field takes focus
//! (decision 197): with nothing typed it shows the recent searches, and while
//! typing it shows decision 198's groups in their fixed order, each with its
//! top three until "See all N" expands it in place.
//!
//! What is real and what is invented (B21):
//!
//! * Real: the box's shape and §57's numbers; opening on focus, closing on
//!   Escape or a click outside; the group order, the top three and the
//!   expand-in-place ruling; the highlighted first row; the marked match;
//!   "N results" at the bar's right end, capped at 999+; the delete crosses
//!   and Clear on the recent list; the option menu with its dimmed online
//!   sites and the "Search inside books" switch that turns the snippet group
//!   on and off; and the skeleton lines that stand in for the search itself.
//! * Invented: the rows. Home never touches the full-text index (B17, pitfalls
//!   §105), so there is nothing to search yet — the box filters the short list
//!   the page hands it by plain case-insensitive substring. Decision 198's
//!   real matching (word starts, ignored accents, quoted phrases, forgiven
//!   typos, the `tag:` prefixes, results arriving in stages) belongs to the
//!   stage that builds the index. Nothing here navigates either: a dummy row
//!   has no book behind it, so rows are not buttons and Enter does nothing.
//! * Not in the design, and marked where they are used: how long the fake
//!   full-text search takes, the height the list scrolls at, the sizes of the
//!   clock and the cross on a history row, the no-match card's two lines of
//!   copy, and the name of the snippet group.
use super::menu::{menu_popover, MenuPopover, MenuWidth};
use super::paint::{color_probe, probe_color};
use super::search_bar::SearchBar;
use super::setting_row::switch_row;
use super::surface;
use crate::rail_icons::{image_px, remember_fixed, Face};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

/// §57: the box is the bar's width, rows are 52 for books and series, 48 for
/// authors and 44 for history.
const BOX_WIDTH: i32 = 520;
const BOOK_ROW: i32 = 52;
const AUTHOR_ROW: i32 = 48;
/// The two natural-height rows, pinned: a quote or snippet row is two
/// lines of prose with padding, a heading one line with its own.
const PROSE_ROW: i32 = 56;
const HEADING_ROW: i32 = 34;

const HISTORY_ROW: i32 = 44;
/// Not in the design: the list scrolls from this height instead of growing
/// past it, so a wide match cannot push the box off an 800-high board.
const SCROLL_MAX: i32 = 320;
/// Fit-and-stop, as ruled: rows join while their fixed heights fit §57's
/// 320, and the chain is pinned to the committed sum. The probe dumps that
/// led here read a ghost row below the outline; the photograph comparison
/// (B29) showed the ghost is the page's own quote block under the opaque
/// box -- the overlay doing exactly its job -- and the settled grant exact.

/// Decision 198: three rows per group until "See all N" expands it.
const TOP_N: usize = 3;
/// Decision 198: "N results" caps at 999+.
const COUNT_CAP: usize = 999;
/// Not in the design: how long the invented full-text search takes.
const INSIDE_DELAY_MS: u64 = 700;
/// §57 sizes the bar's magnifier and sliders but says nothing about the clock
/// and the cross on a history row; 16 sits a step under the 18 magnifier.
const ROW_ICON_PX: i32 = 16;
/// Not in the design: the skeleton lines' widths, three of them, longest
/// first, the way §64's skeletons read as a paragraph.
const SKELETON_WIDTHS: [i32; 3] = [380, 300, 220];

/// Decision 198's groups, in the order it fixes. A group is shown only when
/// something in it matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SearchGroup {
    Books,
    Authors,
    Series,
    Quotes,
    /// Decision 199's snippets, which join the list only while the "Search
    /// inside books" switch is on. The name is the build's own: §57 calls
    /// them snippets and the switch says "inside books".
    InBooks,
}

impl SearchGroup {
    /// Every group in decision 198's order, snippets last.
    pub const ALL: [SearchGroup; 5] = [
        Self::Books,
        Self::Authors,
        Self::Series,
        Self::Quotes,
        Self::InBooks,
    ];

    /// The heading's name. §57's heading reads "Name · count"; the count is
    /// added where the heading is built.
    fn heading(self) -> &'static str {
        match self {
            Self::Books => "Books",
            Self::Authors => "Authors",
            Self::Series => "Series",
            Self::Quotes => "Saved quotes",
            Self::InBooks => "Inside books",
        }
    }

    /// §57's row height, or `None` for the two groups whose rows are two
    /// lines of prose — a quote and a snippet — where a fixed height would
    /// clip them and the design names none.
    fn row_height(self) -> Option<i32> {
        match self {
            Self::Books | Self::Series => Some(BOOK_ROW),
            Self::Authors => Some(AUTHOR_ROW),
            Self::Quotes | Self::InBooks => None,
        }
    }

    /// The main line's style: Body for a name, Quote for a passage (§57).
    fn title_class(self) -> &'static str {
        match self {
            Self::Quotes | Self::InBooks => "kalam-ui-quote",
            _ => "kalam-ui-body",
        }
    }

    /// The small line under it: Meta for an author or a count, and §57's 11px
    /// Caption for a passage's book and chapter.
    fn meta_class(self) -> &'static str {
        match self {
            Self::Quotes | Self::InBooks => "kalam-ui-caption",
            _ => "kalam-ui-meta",
        }
    }

    /// Which of the expand-in-place flags this group owns.
    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|group| *group == self)
            .unwrap_or(0)
    }
}

/// One row of the dummy list: a group, the main line, and the small line
/// under it.
#[derive(Clone, Debug)]
pub struct SearchItem {
    pub group: SearchGroup,
    pub title: String,
    pub meta: String,
}

impl SearchItem {
    /// Plain case-insensitive substring matching over the row's two lines.
    /// Decision 198's real rules live with the index, not here.
    fn matches(&self, query: &str) -> bool {
        match_range(query, &self.title).is_some() || match_range(query, &self.meta).is_some()
    }
}

/// Hang the box under `bar` and wire it to the bar's field, count and picker.
/// `items` is the dummy library the box filters; `history` is the dummy recent
/// list (decision 197), which the crosses and Clear really do edit.
pub fn search_box(
    bar: &SearchBar,
    items: &[SearchItem],
    history: &[String],
) -> gtk::Box {
    // Not a GtkPopover, and run 42893860400 is the reason: popup() on the
    // popover died mid-call under headless sway (home_box:before2 logged,
    // home_box:after2 never, the unflushed log buffer taking the line with
    // it), in the same environment where the reader's annotation popover
    // photographs fine. §57's box is a panel under the bar in every respect
    // the design cares about, so it is now exactly that: an ordinary child
    // of the page overlay, shown and hidden, with Escape and the outside
    // click closed by hand where `autohide` used to do it. Being in the
    // page rather than a toplevel also keeps the field's keystrokes in the
    // page, which was the other half of what the popover's non-modality
    // bought.
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    panel.set_visible(false);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.add_css_class("kalam-search-list");

    // §24: no scrollbar anywhere; the list simply ends at the outline.
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&list));
    scroll.set_hexpand(true);
    scroll.set_vexpand(true);
    scroll.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroll.set_vscrollbar_policy(gtk::PolicyType::External);
    // And the scroll clips: the settled layout of this chain has, in every
    // probe run, ended a row shorter than the render-time measure, and
    // whatever the settled number turns out to be, nothing below it may
    // draw past the outline the settled layout has closed.
    scroll.set_overflow(gtk::Overflow::Hidden);
    // No propagated natural height: run 6992780's frame showed the box
    // closing its outline after the second group while the third group's
    // rows painted on below it -- the propagation left the viewport one
    // layout behind. The scroll's height is instead a plain number, set
    // from the list's own measure at the end of every render.
    scroll.set_max_content_height(SCROLL_MAX);

    // §57's shape rides the shell, not the panel: run 42907346209's probe
    // gave the panel its full 520 x 262 and still no frame showed the
    // border on the right or the bottom, because this shell's own width
    // request is a floor -- it overflows a bordered parent by exactly the
    // border's two pixels and paints over it, GTK boxes not clipping. On
    // the shell the border has nothing left to overflow it, and the
    // request shrinks by the border so the outside stays §57's 520.
    let inner = gtk::Box::new(gtk::Orientation::Vertical, 0);
    inner.add_css_class("kalam-search-box-inner");
    // No overlay of its own inside the box: the history state photographs
    // clean and the grouped state leaked a row past every clip, and the one
    // widget between them is this chain's nested overlay, whose fade child
    // never painted at all. The scroll sits in the shell directly; §57's
    // fade goes without inside the box, recorded in B28.
    inner.append(&scroll);
    let shell = gtk::Box::new(gtk::Orientation::Vertical, 0);
    shell.add_css_class("kalam-search-box");
    shell.set_size_request(BOX_WIDTH, -1);
    // Clip, so a descendant's larger natural size ends at the shell's
    // edge instead of painting over the outline's inset.
    shell.set_overflow(gtk::Overflow::Hidden);
    shell.append(&inner);
    panel.append(&shell);

    let state = Results {
        entry: bar.entry.clone(),
        count: bar.count.clone(),
        list: list.clone(),
        scroll: scroll.clone(),
        shell: shell.clone(),
        items: Rc::new(items.to_vec()),
        history: Rc::new(RefCell::new(history.to_vec())),
        expanded: Rc::new(RefCell::new(vec![false; SearchGroup::ALL.len()])),
        inside_books: Rc::new(Cell::new(false)),
        searching: Rc::new(Cell::new(false)),
        budget: Rc::new(Cell::new(0)),
        // A drawing reads its colours from labels in its own tree
        // (`super::paint`), and the match tint needs two: the tint itself and
        // the page colour it is blended over.
        tint_probe: color_probe("kalam-ui-token-search-match"),
        bg_probe: color_probe("kalam-ui-token-bg"),
    };
    state.list.append(&state.tint_probe);
    state.list.append(&state.bg_probe);
    state.render();

    // Decision 197: the box drops open when the field takes focus (§57's
    // accent underline is the bar's own wiring). Nothing pops it down on
    // focus-out — Escape and a click outside are the ways out, and closing on
    // focus-out would fight the popover for the field.
    // An EventControllerFocus, not notify::has-focus: three photograph
    // rounds showed a focused field whose notify handlers left no trace in
    // any log -- not even the bar's own class switch, the "ring" in the
    // frames being GTK's built-in entry outline rather than the styled
    // state. The focus controller's enter is the mechanism the toolkit
    // guarantees for exactly this.
    let focus_open = panel.clone();
    let focus_ctrl = gtk::EventControllerFocus::new();
    focus_ctrl.connect_enter(move |_| {
        // No has_focus() guard, and the probe round (run 42890371025) is
        // why: under headless sway the window never holds seat-level
        // focus, so GTK sets the focus *widget* -- draws the outline,
        // fires enter -- while has-focus stays false, and seven rounds of
        // handlers guarded on a property that never arrived. enter IS the
        // gain; open on it, unconditionally.
        focus_open.set_visible(true);
        // A whole-window repaint once the box is in, so the photographed
        // frame is one layout rather than a tear between two.
        if let Some(root) = focus_open.root() {
            root.queue_draw();
        }
    });
    bar.entry.add_controller(focus_ctrl);

    let typed = state.clone();

    bar.entry.connect_changed(move |_| typed.render());

    // Decision 200's option menu on the picker. §57 gives the picker the
    // `border` fill while its menu is open, so the open state takes the class
    // the hover rule already uses.
    let switched = state.clone();
    let menu = option_menu(move |on| switched.set_inside_books(on));
    let picker_menu = menu.popover();
    picker_menu.set_parent(&bar.options);
    let open_menu = picker_menu.clone();
    let picker_open = bar.options.clone();
    bar.options.connect_clicked(move |_| {
        open_menu.popup();
        // §57 fills the picker with `border` while its menu is open. A GTK4
        // popover has no `show` signal to hang that on, so the class rides on
        // the click that opens it and comes off with `closed`.
        picker_open.add_css_class("open");
    });
    let picker_shut = bar.options.clone();
    picker_menu.connect_closed(move |_| picker_shut.remove_css_class("open"));

    panel
}

/// Decision 200: §17's menu at 260 wide — "Search in" with Library ticked and
/// the four online sites dimmed under their own caption, a divider, then the
/// "Search inside books" switch with §57's Meta caption. 260 is §17's library
/// width, which `MenuWidth` calls `Tools`.
fn option_menu(on_inside_books: impl Fn(bool) + 'static) -> MenuPopover {
    let menu = menu_popover(MenuWidth::Tools);
    menu.add_caption("Search in");
    // Library is the only search that exists, so the tick cannot move. The
    // row stays a checklist row because §17 draws one and decision 200 asks
    // for it ticked.
    menu.add_check_item("Library", true, |_| {});
    menu.add_caption("Online · not available yet");
    for site in ["AO3", "Fanfiction.net", "Royal Road", "MangaDex"] {
        menu.add_unavailable_item(site);
    }
    menu.add_separator();
    // Decision 199: off by default, on until turned off, and off again at
    // every restart — which holding it in a `Cell` does by itself, since
    // nothing writes it down.
    let row = switch_row(
        "Search inside books",
        Some("Looks through the full text. Slower."),
        false,
        on_inside_books,
    );
    menu.list().append(&row.widget());
    menu
}

/// The box's contents and the state they are drawn from. Cloning it clones
/// GTK handles and `Rc`s, so every callback drives the same box.
#[derive(Clone)]
struct Results {
    entry: gtk::Entry,
    count: gtk::Label,
    list: gtk::Box,
    scroll: gtk::ScrolledWindow,
    shell: gtk::Box,
    items: Rc<Vec<SearchItem>>,
    history: Rc<RefCell<Vec<String>>>,
    expanded: Rc<RefCell<Vec<bool>>>,
    inside_books: Rc<Cell<bool>>,
    searching: Rc<Cell<bool>>,
    /// Height committed to the list so far in this render, for fit-and-stop.
    budget: Rc<Cell<i32>>,
    tint_probe: gtk::Label,
    bg_probe: gtk::Label,
}

impl Results {
    /// Draw the box for what is in the field now.
    fn render(&self) {
        while let Some(child) = self.list.first_child() {
            child.unparent();
        }
        self.budget.set(0);
        let query = self.entry.text().trim().to_string();
        if query.is_empty() {
            self.count.set_visible(false);
            self.render_history();
        } else {
            let groups = self.groups(&query);
            let total: usize = groups.iter().map(|(_, hits)| hits.len()).sum();
            self.count.set_label(&results_caption(total));
            self.count.set_visible(true);
            if self.searching.get() {
                self.render_searching(&query, &groups);
            } else if total == 0 {
                self.render_no_match(&query);
            } else {
                self.render_groups(&query, &groups);
            }
        }
        // The probes went out with the rest of the list, and a drawing can
        // only read a colour from a label that is in the tree.
        self.list.append(&self.tint_probe);
        self.list.append(&self.bg_probe);
        // And the whole chain pinned to the committed sum, exactly: no
        // measure, no negotiation, every link the same number the rows
        // were added against.
        let used = self.budget.get();
        self.list.set_height_request(used);
        self.scroll.set_height_request(used);
        self.shell.set_height_request(used + 2);
    }

    /// Commit `h` more pixels of list, if the cap still holds.
    fn take(&self, h: i32) -> bool {
        let used = self.budget.get();
        if used + h > SCROLL_MAX {
            return false;
        }
        self.budget.set(used + h);
        true
    }

    /// Decision 198's groups in order, each only when it has a match, with the
    /// indices of the rows that matched. The snippet group joins them only
    /// while the switch is on (decision 199).
    fn groups(&self, query: &str) -> Vec<(SearchGroup, Vec<usize>)> {
        let mut out = Vec::new();
        for group in SearchGroup::ALL {
            if group == SearchGroup::InBooks && !self.inside_books.get() {
                continue;
            }
            let hits: Vec<usize> = self
                .items
                .iter()
                .enumerate()
                .filter(|(_, item)| item.group == group && item.matches(query))
                .map(|(at, _)| at)
                .collect();
            if !hits.is_empty() {
                out.push((group, hits));
            }
        }
        out
    }

    fn is_expanded(&self, group: SearchGroup) -> bool {
        self.expanded.borrow()[group.index()]
    }

    fn set_expanded(&self, group: SearchGroup, expanded: bool) {
        self.expanded.borrow_mut()[group.index()] = expanded;
        self.render();
    }

    /// Decision 197: with nothing typed, the recent searches — each with a
    /// delete cross, and a Clear. The list itself is the page's invented one
    /// (B21); the real one is saved when a result is opened or Enter is
    /// pressed, keeps twenty and stays on this computer, none of which exists
    /// yet. Deleting a row here does remove it, from the dummy list.
    fn render_history(&self) {
        let queries = self.history.borrow().clone();
        let clear = self.link_button("Clear", {
            let state = self.clone();
            move || state.clear_history()
        });
        if !self.take(HEADING_ROW) {
            return;
        }
        self.list
            .append(&self.heading_row("Recent searches", Some(clear)));
        if queries.is_empty() {
            if self.take(40) {
                let none = gtk::Label::new(Some("Searches you open will be listed here."));
                none.add_css_class("kalam-ui-meta");
                none.add_css_class("kalam-search-none");
                none.set_halign(gtk::Align::Start);
                none.set_xalign(0.0);
                self.list.append(&none);
            }
            return;
        }
        for (index, query) in queries.iter().enumerate() {
            if !self.take(HISTORY_ROW) {
                break;
            }
            self.list.append(&self.history_row(query, index));
        }
    }

    /// The groups, each with its heading and either its top three or all of
    /// them, and decision 198's highlight on the very first row.
    fn render_groups(&self, query: &str, groups: &[(SearchGroup, Vec<usize>)]) {
        let mut first = true;
        for (group, hits) in groups {
            // By value: the "See all N" closure below is `'static`, and the
            // slice it is built from is not.
            let group = *group;
            let total = hits.len();
            let expanded = self.is_expanded(group);
            let shown = if expanded || total <= TOP_N {
                hits.as_slice()
            } else {
                &hits[..TOP_N]
            };
            let right = (total > TOP_N).then(|| {
                let label = if expanded {
                    "Show less".to_string()
                } else {
                    format!("See all {total} ›")
                };
                let state = self.clone();
                self.link_button(&label, move || state.set_expanded(group, !expanded))
            });
            // A group joins whole or not at all: the heading and its first
            // row are checked together, so a heading never ends up dangling
            // over the cap line.
            let first_h = shown
                .first()
                .map(|at| self.items[*at].group.row_height().unwrap_or(PROSE_ROW))
                .unwrap_or(0);
            if !self.take(HEADING_ROW + first_h) {
                break;
            }
            let heading = format!("{} · {}", group.heading(), total);
            self.list.append(&self.heading_row(&heading, right));
            let mut committed = true;
            for at in shown {
                if !committed && !self.take(self.items[*at].group.row_height().unwrap_or(PROSE_ROW))
                {
                    break;
                }
                committed = false;
                let row = self.result_row(&self.items[*at], query, first);
                first = false;
                self.list.append(&row);
            }
        }
    }

    /// Decision 199: while the full text is being searched the box shows
    /// skeleton lines and says so. The groups already found stay on screen —
    /// decision 198 has results arriving in stages — and the snippet group is
    /// the one still coming. §64 gives a skeleton the `border` fill and
    /// radius 6; the widths are the build's own.
    fn render_searching(&self, query: &str, groups: &[(SearchGroup, Vec<usize>)]) {
        let ready: Vec<(SearchGroup, Vec<usize>)> = groups
            .iter()
            .filter(|(group, _)| *group != SearchGroup::InBooks)
            .cloned()
            .collect();
        self.render_groups(query, &ready);

        if self.take(40) {
            let note = gtk::Label::new(Some("Searching inside your books…"));
            note.add_css_class("kalam-ui-meta");
            note.add_css_class("kalam-search-note");
            note.set_halign(gtk::Align::Start);
            note.set_xalign(0.0);
            self.list.append(&note);
        }
        for width in SKELETON_WIDTHS {
            // 12 of skeleton and its 8 of margin.
            if !self.take(20) {
                break;
            }
            let line = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            line.add_css_class("kalam-search-skeleton");
            line.set_size_request(width, 12);
            line.set_halign(gtk::Align::Start);
            self.list.append(&line);
        }
    }

    /// §57: the no-match message is §10's dashed state card, inside the box.
    /// Neither line of copy is in the design, so both are the build's own —
    /// the shelf's caption is the other place this page had to write one.
    fn render_no_match(&self, query: &str) {
        let card = surface::state_card(
            &format!("Nothing matches “{query}”"),
            "Search looks at titles, authors, series and saved quotes. Try a \
             shorter word — or turn on Search inside books to look through \
             the full text.",
        );
        card.add_css_class("kalam-search-nomatch");
        // The card is the whole content of this state, and its own height
        // is the one natural one left in the box; the take keeps the sum
        // honest for the pin below.
        self.take(140);
        self.list.append(&card);
    }

    /// §57's group heading: Label style, "Name · count", 14 20 4 padding, with
    /// the accent link at the right.
    fn heading_row(&self, title: &str, right: Option<gtk::Button>) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("kalam-search-heading");
        // Fixed like the rows, for the same reason: the heading is the
        // other natural-height child in the list.
        row.set_size_request(-1, HEADING_ROW);
        let label = gtk::Label::new(Some(title));
        label.add_css_class("kalam-ui-label");
        label.set_hexpand(true);
        label.set_halign(gtk::Align::Start);
        label.set_valign(gtk::Align::Center);
        label.set_xalign(0.0);
        row.append(&label);
        if let Some(right) = right {
            row.append(&right);
        }
        row
    }

    /// The accent link in a heading's right slot: §57's "See all N ›" at 12,
    /// decision 198's "Show less" in the same place, and the recent list's
    /// Clear.
    fn link_button(&self, label: &str, on_click: impl Fn() + 'static) -> gtk::Button {
        let button = gtk::Button::with_label(label);
        button.add_css_class("kalam-search-link");
        button.set_halign(gtk::Align::End);
        button.set_valign(gtk::Align::Center);
        button.connect_clicked(move |_| on_click());
        button
    }

    /// One result row: the main line with its match marked, and the small line
    /// under it. It is a box and not a button, because a dummy row has no book
    /// to open — decision 198's Enter and click both belong to the stage that
    /// wires real results.
    fn result_row(&self, item: &SearchItem, query: &str, first: bool) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Vertical, 0);
        row.add_css_class("kalam-search-row");
        if first {
            row.add_css_class("highlighted");
        }
        // A fixed height for every row, prose included: the settled-layout
        // dump showed the chain losing exactly one prose row's height four
        // hundred milliseconds after render, with no second render -- a
        // natural-height child collapsing once its label layout settles --
        // and a row whose height is a request cannot collapse.
        let height = item.group.row_height().unwrap_or(PROSE_ROW);
        row.set_size_request(-1, height);
        if item.group.row_height().is_none() {
            row.add_css_class("prose");
        }

        let tint = probe_color(&self.tint_probe);
        let bg = probe_color(&self.bg_probe);

        let copy = gtk::Box::new(gtk::Orientation::Vertical, 2);
        copy.set_hexpand(true);
        copy.set_vexpand(true);
        copy.set_valign(gtk::Align::Center);

        let title = marked_label(&item.title, query, item.group.title_class(), tint, bg);
        title.set_halign(gtk::Align::Start);
        title.set_xalign(0.0);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        copy.append(&title);

        if !item.meta.is_empty() {
            let meta = marked_label(&item.meta, query, item.group.meta_class(), tint, bg);
            meta.set_halign(gtk::Align::Start);
            meta.set_xalign(0.0);
            meta.set_ellipsize(gtk::pango::EllipsizeMode::End);
            copy.append(&meta);
        }
        row.append(&copy);
        row
    }

    /// §57's history row: a clock icon, the query, and a delete cross, 44
    /// high. The cross removes the row from the dummy list.
    fn history_row(&self, query: &str, index: usize) -> gtk::Box {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.add_css_class("kalam-search-row");
        row.add_css_class("history");
        row.set_size_request(-1, HISTORY_ROW);

        let clock = image_px("clock", Face::Idle, ROW_ICON_PX);
        remember_fixed(&clock, "clock", Face::Idle);
        row.append(&clock);

        let label = gtk::Label::new(Some(query));
        label.add_css_class("kalam-ui-body");
        label.set_hexpand(true);
        label.set_halign(gtk::Align::Start);
        label.set_valign(gtk::Align::Center);
        label.set_xalign(0.0);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row.append(&label);

        let cross = gtk::Button::new();
        cross.add_css_class("kalam-search-delete");
        cross.set_valign(gtk::Align::Center);
        cross.set_tooltip_text(Some("Remove this search"));
        let glyph = image_px("x", Face::Idle, ROW_ICON_PX);
        remember_fixed(&glyph, "x", Face::Idle);
        cross.set_child(Some(&glyph));
        let state = self.clone();
        cross.connect_clicked(move |_| state.delete_history(index));
        row.append(&cross);
        row
    }

    fn delete_history(&self, index: usize) {
        if index < self.history.borrow().len() {
            self.history.borrow_mut().remove(index);
        }
        self.render();
    }

    fn clear_history(&self) {
        self.history.borrow_mut().clear();
        self.render();
    }

    /// Decision 199: the switch turns the snippet group on and off, and while
    /// it "searches" the box shows skeleton lines. The wait is invented —
    /// there is no full-text index to wait for (B17).
    fn set_inside_books(&self, on: bool) {
        self.inside_books.set(on);
        let typing = !self.entry.text().trim().is_empty();
        if on && typing {
            self.searching.set(true);
            self.render();
            let done = self.clone();
            gtk::glib::timeout_add_local_once(Duration::from_millis(INSIDE_DELAY_MS), move || {
                done.searching.set(false);
                done.render();
            });
        } else {
            self.searching.set(false);
            self.render();
        }
    }
}

/// A label whose text carries §57's marked match: bold on the `match` tint.
/// The tint is blended over the page colour here because pango's background
/// attribute takes an opaque colour, while the token is text at 8 to 10%.
fn marked_label(
    text: &str,
    query: &str,
    class: &str,
    tint: gtk::gdk::RGBA,
    bg: gtk::gdk::RGBA,
) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class(class);
    if let Some((start, end)) = match_range(query, text) {
        let list = gtk::pango::AttrList::new();
        let mut weight = gtk::pango::AttrInt::new_weight(gtk::pango::Weight::Bold);
        weight.set_start_index(start);
        weight.set_end_index(end);
        list.insert(weight);
        let (red, green, blue) = match_tint(tint, bg);
        let mut background = gtk::pango::AttrColor::new_background(red, green, blue);
        background.set_start_index(start);
        background.set_end_index(end);
        list.insert(background);
        label.set_attributes(Some(&list));
    }
    label
}

/// Blend the `match` token over the page colour and hand back the opaque
/// 16-bit triplet pango wants.
fn match_tint(tint: gtk::gdk::RGBA, bg: gtk::gdk::RGBA) -> (u16, u16, u16) {
    let alpha = f64::from(tint.alpha());
    let blend = |channel: f32, base: f32| -> u16 {
        let mixed = f64::from(channel) * alpha + f64::from(base) * (1.0 - alpha);
        (mixed * 65535.0).round().clamp(0.0, 65535.0) as u16
    };
    (
        blend(tint.red(), bg.red()),
        blend(tint.green(), bg.green()),
        blend(tint.blue(), bg.blue()),
    )
}

/// The byte range of `query`'s first match in `text`, ignoring case, or `None`.
/// Pango wants byte offsets into the string the label shows, so the search
/// walks the lowercased characters while keeping each one's offset in the
/// original — a capital that lowercases to two characters (İ) would otherwise
/// shift every range behind it.
fn match_range(query: &str, text: &str) -> Option<(u32, u32)> {
    let needle: Vec<char> = query.chars().flat_map(|c| c.to_lowercase()).collect();
    if needle.is_empty() {
        return None;
    }
    let hay: Vec<(usize, char)> = text
        .char_indices()
        .flat_map(|(at, c)| c.to_lowercase().map(move |lower| (at, lower)))
        .collect();
    if hay.len() < needle.len() {
        return None;
    }
    for start in 0..=hay.len() - needle.len() {
        let window = &hay[start..start + needle.len()];
        if window.iter().map(|(_, c)| *c).eq(needle.iter().copied()) {
            let last = window[window.len() - 1].0;
            let end = text
                .char_indices()
                .map(|(at, _)| at)
                .find(|at| *at > last)
                .unwrap_or(text.len());
            return Some((window[0].0 as u32, end as u32));
        }
    }
    None
}

/// Decision 197 puts "N results" at the bar's right end and decision 198 caps
/// the count at 999+. The singular is the build's own English.
fn results_caption(total: usize) -> String {
    if total > COUNT_CAP {
        format!("{COUNT_CAP}+ results")
    } else if total == 1 {
        "1 result".to_string()
    } else {
        format!("{total} results")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// §57's row heights and the box's width, pinned: a change here is a
    /// change of design, not a refactor.
    #[test]
    fn the_box_carries_the_design_numbers() {
        assert_eq!(BOX_WIDTH, 520, "§57: the box is the bar's width");
        assert_eq!(SearchGroup::Books.row_height(), Some(52), "books");
        assert_eq!(SearchGroup::Series.row_height(), Some(52), "series");
        assert_eq!(SearchGroup::Authors.row_height(), Some(48), "authors");
        assert_eq!(HISTORY_ROW, 44, "history");
        // The fade's assertion went with the fade: B28 records why the box
        // no longer carries one.
        assert_eq!(TOP_N, 3, "decision 198's top three");
    }

    /// Decision 198's order, with decision 199's snippets last: the groups are
    /// drawn in this order and only when they match.
    #[test]
    fn the_groups_keep_their_fixed_order() {
        let order: Vec<&str> = SearchGroup::ALL.iter().map(|g| g.heading()).collect();
        assert_eq!(
            order,
            vec!["Books", "Authors", "Series", "Saved quotes", "Inside books"]
        );
    }

    /// The marked match is a byte range in the label's own text, so the search
    /// has to survive a lowercase form that is a different length.
    #[test]
    fn the_match_range_points_at_the_original_text() {
        assert_eq!(match_range("pir", "Piranesi"), Some((0, 3)));
        assert_eq!(match_range("ANESI", "Piranesi"), Some((3, 8)));
        assert_eq!(match_range("le guin", "Ursula K. Le Guin"), Some((10, 17)));
        assert_eq!(match_range("xyz", "Piranesi"), None);
        assert_eq!(match_range("", "Piranesi"), None);
        // İ lowercases to two characters, so the bytes behind it shift; the
        // range still has to name "stanbul" in the original string.
        assert_eq!(match_range("stanbul", "İstanbul"), Some((2, 9)));
    }

    /// Decision 198 caps the count; the build's own English makes one result
    /// singular.
    #[test]
    fn the_count_reads_as_the_decisions_say() {
        assert_eq!(results_caption(1), "1 result");
        assert_eq!(results_caption(7), "7 results");
        assert_eq!(results_caption(COUNT_CAP), "999 results");
        assert_eq!(results_caption(COUNT_CAP + 1), "999+ results");
    }
}
