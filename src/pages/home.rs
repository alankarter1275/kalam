//! Home — the reader's front page (design: `home-spec.md`, design-system
//! §57–§66).
//!
//! The Home build, staged (`docs/build-decisions.md` B12, B21): two columns —
//! greeting, the quote placeholder (§63, B13) and the shelf heading on the
//! left; the Home book card with its pager (§60) and Updates (§62) on the
//! right — plus the search bar across the top (§57) and the state cards (§64)
//! covering everything not built yet.
//!
//! B21 makes this page the showcase: every element is drawn at the design's
//! size and shape, with invented content where no store exists yet, so the
//! composition can be judged whole. Two elements are the exception and stay
//! wired — the book card (real books in progress, real covers, real
//! navigation) and the import path behind the empty-library card's action,
//! which is the one piece of the old dashboard B12 kept.
//!
//! Still to come: §61's reading ruler in the shelf's 340px band (B22 — it
//! waits on a picture of the board, because the text gives every length but
//! not the arrangement), and real search behind §57's box, which today filters
//! an invented list rather than the library (B17).
//!
//! Same skeleton → worker → apply shape the dashboard used (7.1 step 2b):
//! `init` paints placeholders, a worker reads the snapshot and the
//! in-progress books, `Loaded` paints. Home never touches the network
//! (decision 110), and never queries the FTS index (B17, pitfalls §105).

use crate::db::Catalog;
use crate::models::Book;
use crate::pages::all_books::{ask_about_restored, import_summary, spawn_import, ImportTally};
use crate::rail_icons::{self, Face};
use crate::service::{HomeSnapshot, LibraryService};
use crate::widgets::book_row::cover_widget_deferred;
use crate::widgets::search_box::SearchGroup;
use crate::widgets::{search_bar, search_box, surface, updates};
use gtk::prelude::*;
use relm4::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;

/// One page of the Home book card, at the ratio the rest of the app already
/// uses for covers — `book_row::COVER_ASPECT` is 1.6, every shelf and grid
/// cover is 128 x 204, and the thumbnails on disk are 256 x 408. So 225 x 360,
/// and the card two of them side by side: 450 x 360 (B21).
///
/// §60 says 560 x 360, "two pages of 280", and 280 x 360 is 1:1.29. The cover
/// decode is `resize_exact` into a picture set to `Fill`, so holding that
/// number squeezed every cover on this page into a shape no cover in the
/// library actually has, while the same cover two clicks away was undistorted.
const COVER_W: i32 = 225;
const COVER_H: i32 = 360;
/// The card: its two equal pages side by side (§60, B21).
/// §64's state cards: the column-wide face and the narrower
/// "Nothing in progress" face, both 360 high -- the book card's
/// height, since every face stands in the book's place or the
/// column's.
const STATE_W: i32 = 600;
const STATE_W_NONE: i32 = 560;
const STATE_H: i32 = 360;

/// The reading page: wider than the cover by the owner's ruling
/// (2026-10-29, B35) -- "keep the cover side fixed, but make the
/// details side wider" -- where §474 and B21 had two equal pages.
const PAGE_W: i32 = 260;
const CARD_W: i32 = COVER_W + PAGE_W;
/// How many in-progress books the pager holds (§60: five dots).
const PAGER_MAX: usize = 5;
/// The greeting's second Display line (decision 186: "the greeting stacked on
/// two lines (Display)"). home-spec §7 names "Harvey" as the placeholder for
/// the user's own name, and B21 puts the design's own placeholder here until
/// the customization area in Settings exists (B14). The date the build showed
/// instead has no line in the design, and at Display 46 it would not have fit
/// the 470 column anyway.
const GREETING_NAME: &str = "Harvey";
/// Showcase text for the card's excerpt (B21). The real one comes from the
/// reader engine at the saved position (home-spec §7) and nothing hands it over
/// yet. A one-line apology cannot show whether Playfair 13 at 1.85 under a 96
/// fade reads correctly, and that is what this card is now being judged on.
const SHOWCASE_EXCERPT: &str = "The light in the hallway had gone out again, \
    and she stood at the top of the stairs listening to the house settle around \
    her. Somewhere below a door closed, and the sound came up through the floor \
    and into the soles of her feet.";
/// The placeholder quote (B13): §63's element drawn in full, so the real
/// source drops into this one constant when the owner settles it. Public
/// domain — Dickinson's couplet about what a book does.
const PLACEHOLDER_QUOTE: (&str, &str) = (
    "There is no Frigate like a Book\nTo take us Lands away",
    "Emily Dickinson",
);

/// §57's hint, shown in `text-2` while the field is empty.
const SEARCH_HINT: &str = "Search your library";
/// Decision 189 puts the last check at the right of §62's heading. Invented
/// until a feed exists that could have checked anything (B21).
const SHOWCASE_CHECKED: &str = "Checked 3 hours ago";
/// §62's rows, invented for the same reason: online sources are off until
/// Part 2 (B4), so the timeline is drawn at its real size and shape with
/// content a feed will later replace. Title, detail, time, then whether the
/// node is new (accent) or already seen (ring). Six rows: §62 shows four and
/// the rest is what the fade is for.
const SHOWCASE_UPDATES: &[(&str, &str, &str, bool)] = &[
    ("Wonder", "3 new chapters in the saved source", "2h", true),
    ("Piranesi", "The source listed a new edition", "5h", false),
    (
        "The Dispossessed",
        "Description updated at the source",
        "1d",
        false,
    ),
    (
        "Wolf Hall",
        "2 new chapters in the saved source",
        "2d",
        false,
    ),
    ("Earthsea", "A new edition appeared at the source", "3d", false),
    (
        "The Left Hand of Darkness",
        "Cover updated at the source",
        "4d",
        true,
    ),
    (
        "A Wizard of Earthsea",
        "New chapter in the saved source",
        "5d",
        false,
    ),
    ("Dune", "Synopsis updated at the source", "6d", false),
    ("The Tombs of Atuan", "New edition listed at the source", "1w", false),
    (
        "The Farthest Shore",
        "The source saved a new chapter",
        "1w",
        false,
    ),
];

/// Decision 198's groups, filled with invented rows (B21). Home never touches
/// the full-text index (B17, pitfalls §105), so the box under the search bar
/// filters this list instead of the library: group, main line, small line. The
/// two `InBooks` rows are decision 199's snippets, which the option menu's
/// switch turns on.
const SHOWCASE_SEARCH: &[(SearchGroup, &str, &str)] = &[
    (SearchGroup::Books, "Piranesi", "Susanna Clarke"),
    (
        SearchGroup::Books,
        "The Left Hand of Darkness",
        "Ursula K. Le Guin",
    ),
    (SearchGroup::Books, "Wolf Hall", "Hilary Mantel"),
    (SearchGroup::Books, "Giovanni's Room", "James Baldwin"),
    (
        SearchGroup::Books,
        "The Remains of the Day",
        "Kazuo Ishiguro",
    ),
    (SearchGroup::Books, "The Dispossessed", "Ursula K. Le Guin"),
    (SearchGroup::Authors, "Ursula K. Le Guin", "6 books"),
    (SearchGroup::Authors, "Hilary Mantel", "4 books"),
    (SearchGroup::Authors, "James Baldwin", "3 books"),
    (SearchGroup::Series, "Earthsea", "5 books"),
    (SearchGroup::Series, "The Broken Earth", "3 books"),
    (
        SearchGroup::Quotes,
        "There is no Frigate like a Book",
        "Emily Dickinson · Poems",
    ),
    (
        SearchGroup::Quotes,
        "All this happened, more or less.",
        "Kurt Vonnegut · Slaughterhouse-Five",
    ),
    (
        SearchGroup::InBooks,
        "…the tide came in through the door in the floor…",
        "Piranesi · Chapter 3",
    ),
    (
        SearchGroup::InBooks,
        "…she had left the door in the wall unlocked…",
        "The Left Hand of Darkness · Chapter 7",
    ),
];

/// Decision 197's recent searches, invented for the same reason. The real list
/// is saved when a result is opened or Enter is pressed, keeps the last twenty
/// and stays on this computer; none of that exists yet, though the delete
/// crosses and Clear really do edit this one.
const SHOWCASE_HISTORY: &[&str] = &[
    "piranesi",
    "le guin",
    "the door in the wall",
    "earthsea",
    "dickinson",
];

/// Navigation requests from Home. Variants are named for their destination
/// rather than sharing an `Open` prefix (clippy::enum_variant_names) -- the
/// same convention `LibraryOut` uses. The dashboard's `BookDialog`,
/// `AllBooks` and `ComicSeries` left with it (B12); a later stage adds back
/// whatever it emits, and `app.rs` grows an arm for it then.
#[derive(Debug)]
pub enum HomeOut {
    /// The cover click (§60): open the book's details page.
    Book { book_id: i64 },
    /// The play button (§60): resume in the reader at the saved position.
    /// No chapter rides along — `OpenReaderAt` with one *resets* the
    /// fraction inside the chapter, and resuming must not move the position
    /// the card was just showing. `None` opens at the stored spot.
    Read { book_id: i64 },
    /// The nothing-in-progress face's pill (§64): walk to the Library.
    OpenLibrary,
}

#[derive(Debug)]
pub enum HomeMsg {
    AddBooks,
    FilesChosen(Vec<PathBuf>),
    /// The whole import finished.
    ImportFinished(ImportTally),
    /// Re-read the snapshot — after an import answer changes what is in a
    /// book, since the card shows the position such an answer can reset.
    Refresh,
    /// The worker finished: snapshot plus card data (7.1 step 2b). The page
    /// painted its placeholders in `init` and fills from this. No library
    /// read happens on the UI thread.
    Loaded(Box<HomePayload>),
    /// A pager dot was clicked (§60): show this in-progress book.
    Switch(usize),
}

/// What the worker hands the apply half: the library snapshot plus the
/// per-book data §60's card needs (chapter line, excerpt), all read off
/// the UI thread.
#[derive(Debug)]
pub struct HomePayload {
    snap: HomeSnapshot,
    cards: Vec<CardData>,
}

/// One pager card's worth of §60. Both text fields are empty until the
/// chapter-head store lands (B17); the card's fallbacks cover them. The
/// chapter *index* stays on the worker side — "resume" deliberately
/// carries no chapter (see `HomeOut::Read`).
#[derive(Debug)]
struct CardData {
    book: Book,
    /// The saved position's chapter title; empty until B17's store lands.
    chapter_title: String,
    /// That chapter's first words; empty until B17's store lands.
    excerpt: String,
}

pub struct HomePageModel {
    service: LibraryService,
    importing: bool,
    forced_state: Option<String>,
    load_failed: bool,
    /// Which pager card the book card shows (§60).
    current: usize,
    /// Kept so a pager click repaints without another worker run.
    cards: Vec<CardData>,
    /// The empty-library state card (§64) needs the difference between
    /// "nothing in progress" and "nothing at all".
    library_empty: bool,
}

impl HomePageModel {
    /// Which of §64's faces the right column shows, if any: the
    /// lever's choice first, so a photograph can hold a face over a
    /// library that contradicts it, then the real conditions in the
    /// order they exclude each other -- a failed read with nothing
    /// to show, no books at all, books but none open (B36).
    fn state_face(&self) -> Option<&'static str> {
        match self.forced_state.as_deref() {
            Some("loading") | Some("empty") | Some("none") | Some("error") => {
                self.forced_state.as_deref().map(|s| match s {
                    "loading" => "loading",
                    "empty" => "empty",
                    "none" => "none",
                    _ => "error",
                })
            }
            _ => {
                if !self.cards.is_empty() {
                    None
                } else if self.load_failed {
                    Some("error")
                } else if self.library_empty {
                    Some("empty")
                } else {
                    Some("none")
                }
            }
        }
    }

}

#[relm4::component(pub)]
impl Component for HomePageModel {
    type Init = Arc<Catalog>;
    type Input = HomeMsg;
    type Output = HomeOut;
    type CommandOutput = ();

    view! {
        // An overlay, not a plain box: §57's result box hangs over the
        // greeting and the shelf from the search row, and since the B27
        // rebuild it is an ordinary child of this overlay rather than a
        // popover toplevel -- popup() on the popover died mid-call under
        // headless sway (run 42893860400: home_box:before2 logged,
        // home_box:after2 never, the unflushed buffer taking the line).
        #[root]
        gtk::Overlay {
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_hexpand: true,
                set_vexpand: true,

            // The two columns (home-spec §1), and the first thing in the
            // flow: the search row no longer stands ahead of them (§57's
            // bar hangs off the root overlay in init, the way its result
            // box does), so the columns' top margins measure from the
            // window's own top edge.
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_hexpand: true,
                set_vexpand: true,

                // Left column: 470 wide, 56 from the rail, its top below
                // the search row's band (28 above the bar plus the bar's
                // 40) even though the row itself now hangs off the
                // overlay: the greeting has always started there.
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_width_request: 470,
                    set_margin_start: 56,
                    set_margin_top: 68,
                    set_halign: gtk::Align::Start,
                    set_valign: gtk::Align::Start,

                    // The greeting, two Display lines (home-spec §1, decision
                    // 186). The second is the design's own placeholder name
                    // until the customization area in Settings exists (B14,
                    // B21); it is not the date, which the design has no line
                    // for and which the build was standing in with.
                    gtk::Label {
                        set_label: &greeting_line(),
                        add_css_class: "kalam-home-greeting",
                        set_halign: gtk::Align::Start,
                    },
                    gtk::Label {
                        set_label: GREETING_NAME,
                        add_css_class: "kalam-home-greeting-name",
                        set_halign: gtk::Align::Start,
                    },

                    // Quote (§63): Quote large behind a 1.5px accent rule,
                    // attribution 8 below. Placeholder until B13 settles —
                    // the swap is one constant, `PLACEHOLDER_QUOTE`.
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-quote-large",
                        set_margin_top: 28,
                        set_halign: gtk::Align::Start,

                        gtk::Label {
                            set_label: PLACEHOLDER_QUOTE.0,
                            add_css_class: "kalam-quote-large-text",
                            set_wrap: true,
                            set_max_width_chars: 36,
                            set_xalign: 0.0,
                        },
                        gtk::Label {
                            set_label: PLACEHOLDER_QUOTE.1,
                            add_css_class: "kalam-quote-large-source",
                            set_margin_top: 8,
                            set_halign: gtk::Align::Start,
                        },
                    },

                    // Your shelf (§61): the heading and Arrange-by chip now;
                    // the ruler itself comes later (B12), and the dashed card
                    // below says so instead of leaving a hole. The chip is
                    // inert until that stage wires §43's Arrange list.
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_margin_top: 24,

                        gtk::Label {
                            set_label: "Your shelf",
                            add_css_class: "kalam-home-heading",
                            set_hexpand: true,
                            set_halign: gtk::Align::Start,
                        },
                        gtk::Label {
                            set_label: "Arrange by · Most recent",
                            add_css_class: "kalam-chip",
                            set_valign: gtk::Align::Center,
                        },
                    },

                    #[name = "shelf_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_size_request: (-1, 340),
                        set_margin_top: 12,
                    },
                },

                // Right column: 600 wide, its right edge 32 from the
                // window's — the board's 56 page margin nudged 24 past
                // (home-spec §1). It keeps the board's 600 rather than
                // narrowing to the card's 450: the card is right-anchored, so
                // narrowing would not move it a pixel, and §64's state cards
                // are sized to the column (B21). Its top margin is the
                // card's own right gap, 54, by the owner's measure
                // (2026-10-29): the distance from the card's top edge to the
                // window's top edge should be the distance from the card's
                // right edge to the window's right edge, and the frames put
                // that at 54 everywhere -- the column's 32 margin plus the
                // 22 the pager dots take beside the card. §1's
                // 52-below-the-search-row put the top at 120; with the row
                // off the flow the column can keep §1's right anchor and
                // take the owner's top instead: 54, the card's own right
                // gap, so the two distances from the window's edges match
                // exactly. Updates rides inside and rises just as far.
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_width_request: 600,
                    set_hexpand: true,
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::Start,
                    set_margin_top: 54,
                    set_margin_end: 32,

                    #[name = "card_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 12,
                        set_halign: gtk::Align::End,
                    },

                    // Updates (§62, decision 189), 36 below the book card per
                    // home-spec §1. Drawn as a dummy: B21 supersedes B15's
                    // "nothing until there is a source", because online
                    // sources stay off until Part 2 (B4) while the element's
                    // size and shape can still be judged now. `init` fills
                    // this from `SHOWCASE_UPDATES`; a feed replaces those rows
                    // and nothing else about the element moves.
                    #[name = "updates_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_margin_top: 36,
                    },
                },
            },
            }
        }
    }

    fn init(
        catalog: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = HomePageModel {
            service: LibraryService::new(catalog),
            importing: false,
            // The CI photograph lever for §64's four faces: open Home
            // in one state however full the library is, the way
            // KALAM_HOME_BOOK opens a chosen book (B36). A normal
            // launch has no such variable and the real conditions
            // decide.
            forced_state: std::env::var("KALAM_HOME_STATE").ok(),
            load_failed: false,
            // A launch lever for the CI photographs and nothing else,
            // like KALAM_HOME_SEARCH: open the card at its third book so
            // a frame can prove the card is one size for every book
            // (B32). A normal launch has no such variable.
            current: std::env::var("KALAM_HOME_BOOK")
                .ok()
                .and_then(|v| v.parse::<usize>().ok())
                .unwrap_or(0),
            cards: Vec::new(),
            library_empty: false,
        };
        let widgets = view_output!();


        // §57's bar and box and §62's timeline (B21): all three come from
        // widget modules and go into the hosts the view reserves for them, the
        // way the shelf and the book card do. The bar and the box are wired to
        // each other — focus, typing, the count, the option menu — and their
        // rows are the invented ones below; the timeline's are too.
        // §57's search row hangs off the overlay at the board's own
        // numbers -- 28 above it, 56 from the rail -- start-anchored like
        // the result box under it, which has hung here since B27 and
        // sizes itself correctly. Keeping the row off the flow is what
        // lets the columns start at the window's top edge (B31).
        let search = search_bar::search_bar(SEARCH_HINT);
        let search_row = gtk::Box::new(gtk::Orientation::Vertical, 0);
        search_row.set_margin_top(28);
        search_row.set_margin_start(56);
        search_row.set_halign(gtk::Align::Start);
        search_row.set_valign(gtk::Align::Start);
        search_row.append(&search.root);
        root.add_overlay(&search_row);
        let box_pop = search_box::search_box(&search, &showcase_search(), &showcase_history());
        // §57 hangs the box off the bar and over the page: start-aligned at
        // the board's 56, its top the search row's 28 plus the bar's 40.
        // Hidden until focus or staging opens it; a hidden overlay child
        // takes no input and paints nothing.
        box_pop.set_halign(gtk::Align::Start);
        box_pop.set_valign(gtk::Align::Start);
        box_pop.set_margin_start(56);
        box_pop.set_margin_top(68);
        root.add_overlay(&box_pop);

        // Staging for the CI photographs, and nothing else: with
        // KALAM_HOME_SEARCH set, the field opens pre-filled on the first idle
        // tick so a frame can show the result box over real invented rows
        // without synthetic typing, which has never been trustworthy enough
        // under headless sway to stage a photograph with. A normal launch has
        // no such variable and this whole block is a no-op; the idle tick is
        // what makes focus land after the window is up rather than before it
        // exists.
        let staged_query = std::env::var("KALAM_HOME_SEARCH").unwrap_or_default();
        let focus_entry = search.entry.clone();
        let staged_pop = box_pop.clone();
        gtk::glib::idle_add_local_once(move || {
            if !staged_query.is_empty() {
                focus_entry.set_text(&staged_query);
                focus_entry.grab_focus();
                // Open here too, without waiting for any focus signal: the
                // probe round showed grab_focus sets the focus widget while
                // has-focus stays false under headless sway, so the enter
                // handler is the usual opener and this is the staging
                // launch's own belt-and-braces.
                staged_pop.set_visible(true);
                if let Some(root) = staged_pop.root() {
                    root.queue_draw();
                }
                crate::timing::note("home_focus:staged", 1);
                return;
            }
            // The first Home photographs disagreed with each other: one
            // launch showed the bar focused with no box under it, another
            // showed it at rest, and nothing in any log said which had
            // happened. GTK gives a new window's first focus to its first
            // focusable widget, which here is the field; a focused field with
            // no box is the one combination §57 does not describe, so on an
            // unstaged launch Home hands that first focus back and rests.
            if focus_entry.has_focus() {
                // Dropping focusable for one tick hands the focus back
                // without naming a new owner; restoring it in the same tick
                // leaves the field clickable and tab-reachable as before.
                // (WindowExt::set_focus would say the same in one call, but
                // two traits in scope both answer to that name and the
                // compiler refuses to choose -- E0034, run 42867562973.)
                focus_entry.set_focusable(false);
                focus_entry.set_focusable(true);
                crate::timing::note("home_focus:cleared", 1);
            } else {
                crate::timing::note("home_focus:resting", 1);
            }
        });
        // §64's loading face: skeletons for the book, the shelf and
        // Updates, in `border` fill with 6px rounds. The first snapshot
        // replaces all three; the KALAM_HOME_STATE=loading lever never
        // asks, so a photograph can hold this face (B36).
        let updates_skeleton = gtk::Box::new(gtk::Orientation::Vertical, 0);
        updates_skeleton.add_css_class("kalam-skeleton");
        updates_skeleton.set_size_request(-1, 224);
        widgets.updates_host.append(&updates_skeleton);

        // home-spec §7's keys, on B22's ruling: Home's field wins Ctrl+F while
        // Home is showing, and the shell's binding keeps its job everywhere
        // else. The shell's handler is a bubble-phase controller on the window
        // whose own comment gives page handlers first claim, so this
        // capture-phase controller claims Ctrl+F here and leaves the jump to
        // All Books intact on every other page. `/` does the same, except that
        // it types a slash once the field has focus.
        // home-spec §7's keys, on B22's ruling: Home's field wins Ctrl+F
        // while Home is showing, and the shell's binding keeps its job
        // everywhere else. The first photograph of the keys run proved the
        // obvious placement blind: a controller on the page root never sees
        // a key pressed while no widget has focus, because the event's
        // target is then the window itself and the path never walks the
        // page -- keys-home-search.png from run 42862494812 shows All Books
        // after a chord sent from Home, the shell having taken it untouched.
        // The claim therefore sits on the window in the capture phase, ahead
        // of the shell's bubble-phase handler, gated on Home being the mapped
        // page: true on map, false on unmap, so a cached-but-hidden Home
        // claims nothing and every other page reaches the shell as before.
        let home_shown = std::rc::Rc::new(std::cell::Cell::new(false));
        let attached = std::rc::Rc::new(std::cell::Cell::new(false));
        let entry_for_keys = search.entry.clone();
        // Clones before the closures, not inside them: a `move` closure
        // takes the Rc itself, and the unmap handler below still needs its
        // own handle (E0382, run 42881100543).
        let shown_for_map = home_shown.clone();
        let entry_for_map = entry_for_keys.clone();
        let box_for_click = box_pop.clone();
        // Attached on the first map, not in init: at init the page root is
        // not in any window yet, so root() is None there and a controller
        // asked for at init is a controller that never exists -- which is
        // what run 42871002844 proved, chord 6 navigating to All Books
        // through a build that believed it had claimed the chord.
        root.connect_map(move |w| {
            shown_for_map.set(true);
            if attached.get() {
                return;
            }
            attached.set(true);
            let Some(window) = w.root().and_then(|r| r.downcast::<gtk::Window>().ok()) else {
                return;
            };
            let entry = entry_for_map.clone();
            let shown = shown_for_map.clone();
            let box_for_keys = box_for_click.clone();
            let keys = gtk::EventControllerKey::new();
            keys.set_propagation_phase(gtk::PropagationPhase::Capture);
            keys.connect_key_pressed(move |_, keyval, _, modifiers| {
                if !shown.get() {
                    return gtk::glib::Propagation::Proceed;
                }
                // Escape closes the box the popover's autohide used to, and
                // stops there: with the box shut it is nobody's key here.
                if keyval == gtk::gdk::Key::Escape && box_for_keys.get_visible() {
                    box_for_keys.set_visible(false);
                    return gtk::glib::Propagation::Stop;
                }
                let ctrl = modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK);
                let find = ctrl && (keyval == gtk::gdk::Key::f || keyval == gtk::gdk::Key::F);
                if find {
                    // The witness the smoke probe waits on (B24): this page
                    // took the chord, so the shell's jump to All Books is not
                    // coming and no route line will follow. A `[timing]`
                    // label like every other witness the probe counts, and a
                    // no-op without KALAM_TIMING=1, so a normal launch writes
                    // nothing at all.
                    let _t = crate::timing::measure("home_search:ctrl_f");
                    entry.grab_focus();
                    return gtk::glib::Propagation::Stop;
                }
                if keyval == gtk::gdk::Key::slash && !ctrl && !entry.has_focus() {
                    let _t = crate::timing::measure("home_search:slash");
                    entry.grab_focus();
                    return gtk::glib::Propagation::Stop;
                }
                gtk::glib::Propagation::Proceed
            });
            window.add_controller(keys);

            // The outside click, by hand: capture phase on the window, so it
            // sees the press before whatever sits under the box does. A
            // press outside both the box and the bar shuts the box and is
            // then denied by this gesture, which is the gesture declining
            // the sequence -- the click carries on to its real target, the
            // way autohide let it.
            let click = gtk::GestureClick::new();
            click.set_propagation_phase(gtk::PropagationPhase::Capture);
            let panel_for_click = box_for_click.clone();
            let bar_for_click = search.root.clone();
            click.connect_pressed(move |gesture, _n_press, x, y| {
                if !panel_for_click.get_visible() {
                    return;
                }
                let Some(host) = gesture.widget() else {
                    return;
                };
                let inside = |w: &gtk::Widget| match w.compute_bounds(&host) {
                    Some(r) => {
                        // The click carries f64, the rect f32.
                        let (rx, ry) = (f64::from(r.x()), f64::from(r.y()));
                        let (rw, rh) = (f64::from(r.width()), f64::from(r.height()));
                        x >= rx && x <= rx + rw && y >= ry && y <= ry + rh
                    }
                    None => false,
                };
                let panel_w = panel_for_click.upcast_ref::<gtk::Widget>();
                let bar_w = bar_for_click.upcast_ref::<gtk::Widget>();
                if !inside(panel_w) && !inside(bar_w) {
                    panel_for_click.set_visible(false);
                    gesture.set_state(gtk::EventSequenceState::Denied);
                }
            });
            window.add_controller(click);
        });
        root.connect_unmap({
            let shown = home_shown.clone();
            move |_| shown.set(false)
        });

        // Stage-1 placeholders (B12): the shelf area says what is coming,
        // and the card host shows the dashed block the snapshot replaces a
        // moment later.
        let shelf_skeleton = gtk::Box::new(gtk::Orientation::Vertical, 0);
        shelf_skeleton.add_css_class("kalam-skeleton");
        shelf_skeleton.set_size_request(-1, 200);
        shelf_skeleton.set_vexpand(true);
        widgets.shelf_host.append(&shelf_skeleton);

        let skeleton = gtk::Box::new(gtk::Orientation::Vertical, 0);
        skeleton.add_css_class("kalam-skeleton");
        skeleton.set_size_request(CARD_W, COVER_H);
        widgets.card_host.append(&skeleton);

        if model.forced_state.as_deref() != Some("loading") {
            request_snapshot(&model.service, &sender);
        }

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            HomeMsg::Loaded(payload) => {
                let payload = *payload;
                // The same attribution pair the dashboard used: a timing
                // span for KALAM_TIMING=1 and an activity label so a stall
                // during this build says "while home_fill".
                let _t = crate::timing::measure("home_fill");
                let _a = crate::timing::activity("home_fill");
                for err in &payload.snap.errors {
                    crate::notify::error("Could not read the library", err);
                }
                self.cards = payload.cards;
                self.library_empty = payload.snap.stats.total_books == 0;
                self.load_failed = !payload.snap.errors.is_empty();
                if self.current >= self.cards.len() {
                    self.current = 0;
                }
                // Here and not in the worker that read them — see
                // `warm_pager_covers`.
                warm_pager_covers(&self.cards, self.current);
                paint_below(
                    widgets,
                    self.library_empty || self.forced_state.as_deref() == Some("empty"),
                    &crate::theme::current(self.service.catalog()),
                );
                paint_right(
                    widgets,
                    &self.cards,
                    self.current,
                    self.state_face(),
                    &self.service,
                    &sender,
                );
                // CI's photograph of a focus ring (B37): hand the
                // keyboard to the first button on the card, the way a
                // Tab walk would. A normal launch never sets this.
                // After paint_right, not before: paint rebuilds the
                // card, and a pill focused before the rebuild is a
                // pill thrown away with its ring (the first two
                // ringless frames).
                if std::env::var_os("KALAM_HOME_FOCUS").is_some() {
                    let host = widgets.card_host.clone();
                    focus_first_button(&host.upcast::<gtk::Widget>());
                }
            }
            HomeMsg::Switch(i) => {
                if i < self.cards.len() {
                    self.current = i;
                    paint_right(
                        widgets,
                        &self.cards,
                        self.current,
                        self.state_face(),
                        &self.service,
                        &sender,
                    );
                }
            }
            HomeMsg::Refresh => {
                request_snapshot(&self.service, &sender);
            }
            HomeMsg::AddBooks if !self.importing => {
                let dialog = gtk::FileDialog::builder()
                    .title("Import books")
                    .modal(true)
                    .build();

                let filter = gtk::FileFilter::new();
                filter.set_name(Some("Books, Comics, & PDFs (*.epub, *.cbz, *.cbr, *.pdf)"));
                filter.add_suffix("epub");
                filter.add_suffix("cbz");
                filter.add_suffix("cbr");
                filter.add_suffix("pdf");
                filter.add_mime_type("application/epub+zip");
                filter.add_mime_type("application/x-cbz");
                filter.add_mime_type("application/x-cbr");
                filter.add_mime_type("application/pdf");
                let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
                filters.append(&filter);
                dialog.set_filters(Some(&filters));

                let window = root.root().and_then(|r| r.downcast::<gtk::Window>().ok());
                let window = window.or_else(|| {
                    relm4::main_application()
                        .active_window()
                        .and_then(|w| w.downcast::<gtk::Window>().ok())
                });

                let s = sender.clone();
                dialog.open_multiple(
                    window.as_ref(),
                    gtk::gio::Cancellable::NONE,
                    move |result| {
                        let mut paths = Vec::new();
                        if let Ok(files) = result {
                            let n = files.n_items();
                            for i in 0..n {
                                if let Some(obj) = files.item(i) {
                                    if let Ok(file) = obj.downcast::<gtk::gio::File>() {
                                        if let Some(p) = file.path() {
                                            paths.push(p);
                                        }
                                    }
                                }
                            }
                            if !paths.is_empty() {
                                s.input(HomeMsg::FilesChosen(paths));
                            }
                        }
                    },
                );
            }
            // An import is already running; the chooser stays shut rather
            // than racing two workers over the same catalog.
            HomeMsg::AddBooks => {}
            HomeMsg::FilesChosen(paths) => {
                // Importing parses, hashes and copies each file. Doing that
                // inline froze the window, so it runs on a worker thread and
                // reports back per file (the All Books page does the same).
                self.importing = true;

                // Imports still go straight to the catalog: writes are not
                // part of the read-snapshot seam (they belong to the task
                // manager, A0 step 4).
                let catalog = self.service.catalog().clone();
                let done_tx = sender.input_sender().clone();
                spawn_import(
                    catalog,
                    paths,
                    // The design gives Home no in-flight status line, so the
                    // per-file reports go nowhere; the completion toast
                    // carries the outcome. Stage 4 decides whether a quiet
                    // progress element is worth proposing to the designer.
                    |_done, _total, _title| {},
                    move |tally| {
                        let _ = done_tx.send(HomeMsg::ImportFinished(tally));
                    },
                );
            }
            HomeMsg::ImportFinished(tally) => {
                self.importing = false;

                if tally.imported > 0 {
                    crate::notify::success(
                        &format!(
                            "{} book{} imported",
                            tally.imported,
                            if tally.imported == 1 { "" } else { "s" }
                        ),
                        &tally.last_title,
                    );
                } else {
                    // Nothing landed: skips and failures are the whole
                    // story, and the summary toast is the only place this
                    // page has to tell it.
                    crate::notify::error("Nothing was imported", &import_summary(&tally));
                }

                // New books are in the catalog now; refresh so the card and
                // the state cards reflect them. Same road as init: the
                // worker reads, the apply builds.
                request_snapshot(&self.service, &sender);

                // And ask about anything that came back with its marks. The
                // home page is a real import path, not a shortcut to one, so
                // skipping the question here would leave it silently asked
                // nowhere for books imported from this screen.
                if !tally.restored_books.is_empty() {
                    let s = sender.input_sender().clone();
                    ask_about_restored(root, &self.service, &tally.restored_books, move || {
                        s.send(HomeMsg::Refresh).ok();
                    });
                }
            }
        }

        // Overriding `update_with_view` replaces relm4's default
        // `update` + `update_view` pair. Nothing in this view is
        // `#[watch]`ed any more — everything is painted imperatively above
        // — but the call keeps the contract the dashboard's comment warned
        // about, in case a watch is added later.
        self.update_view(widgets, sender);
    }
}

/// Walk depth-first to the first button and give it the keyboard, so
/// a CI frame can show the focus ring §7 asks for (B37). The ring
/// itself rides a class toggled by the widget's own focus notify:
/// GTK's :focus states also ask for a focused window, which headless
/// CI never has, and the search bar's focus face already uses this
/// trick (B27).
fn focus_first_button(widget: &gtk::Widget) -> bool {
    if let Some(button) = widget.downcast_ref::<gtk::Button>() {
        button.add_css_class("kalam-focus-ring");
        button.connect_notify_local(Some("has-focus"), |widget, _| {
            let focused = widget.has_focus();
            if focused {
                widget.add_css_class("kalam-focus-ring");
            } else {
                widget.remove_css_class("kalam-focus-ring");
            }
        });
        button.grab_focus();
        return true;
    }
    let mut child = widget.first_child();
    while let Some(next) = child {
        if focus_first_button(&next) {
            return true;
        }
        child = next.next_sibling();
    }
    false
}

/// The shelf and Updates once the first snapshot lands: the dashed
/// stand-in for the ruler (§61 waits on the owner's board picture,
/// B12) and the Updates block (§62), replacing the loading skeletons.
/// The empty library hides Updates, as decision 194 draws it.
fn paint_below(
    widgets: &HomePageModelWidgets,
    library_empty: bool,
    theme: &crate::theme::Theme,
) {
    let shelf = &widgets.shelf_host;
    while let Some(child) = shelf.first_child() {
        shelf.remove(&child);
    }
    // The ruler itself (§61, B38): one drawn widget, or its empty
    // face when the library is. The dashed stand-in of B12 retires
    // here.
    let ruler = crate::widgets::shelf::shelf_ruler(library_empty, theme);
    ruler.set_vexpand(true);
    ruler.set_valign(gtk::Align::Start);
    shelf.append(&ruler);

    let host = &widgets.updates_host;
    host.set_visible(!library_empty);
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    if !library_empty {
        host.append(&updates::updates(&showcase_updates(), SHOWCASE_CHECKED));
    }
}

/// The right column's card: the book card with its pager (§60), or one
/// of §64's four state faces -- empty library, error, nothing in
/// progress, and the loading skeletons that init puts up and this
/// leaves alone only while the KALAM_HOME_STATE lever holds the page
/// before its first snapshot (B36).
fn paint_right(
    widgets: &HomePageModelWidgets,
    cards: &[CardData],
    current: usize,
    face: Option<&str>,
    service: &LibraryService,
    sender: &ComponentSender<HomePageModel>,
) {
    let host = &widgets.card_host;
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    // The host shrinks to its children and stands at the column's
    // end for the book and its pager; the narrower nothing-in-
    // progress card centres in the column instead, so the host
    // centres with it (B36).
    host.set_halign(if face == Some("none") {
        gtk::Align::Center
    } else {
        gtk::Align::End
    });

    if face == Some("loading") {
        let skeleton = gtk::Box::new(gtk::Orientation::Vertical, 0);
        skeleton.add_css_class("kalam-skeleton");
        skeleton.set_size_request(CARD_W, COVER_H);
        host.append(&skeleton);
        return;
    }

    let Some(face) = face else {
        host.append(&build_book_card(&cards[current], current, cards.len(), sender));
        host.append(&build_pager(cards, current, sender));
        return;
    };

    let card = match face {
        "empty" => {
            let add = sender.clone();
            let choose = sender.clone();
            let card = surface::home_state_card(
                STATE_W,
                STATE_H,
                "Your library is empty",
                "Add the books you are reading; the last one you opened \
                 will wait for you here.",
                vec![
                    (
                        "Add books",
                        crate::widgets::pill_button::PillButtonKind::Primary,
                        Box::new(move || add.input(HomeMsg::AddBooks)) as Box<dyn Fn()>,
                    ),
                    (
                        "Choose a folder",
                        crate::widgets::pill_button::PillButtonKind::Ghost,
                        Box::new(move || choose.input(HomeMsg::AddBooks)),
                    ),
                ],
            );
            card.set_halign(gtk::Align::Fill);
            card
        }
        "error" => {
            let retry_service = LibraryService::new(service.catalog().clone());
            let retry_sender = sender.clone();
            let card = surface::home_state_card(
                STATE_W,
                STATE_H,
                "Home could not load.",
                "Your books are not changed.",
                vec![(
                    "Retry",
                    crate::widgets::pill_button::PillButtonKind::Ghost,
                    Box::new(move || request_snapshot(&retry_service, &retry_sender))
                        as Box<dyn Fn()>,
                )],
            );
            card.set_halign(gtk::Align::Fill);
            card
        }
        _ => {
            let open = sender.clone();
            let card = surface::home_state_card(
                STATE_W_NONE,
                STATE_H,
                "Nothing in progress",
                "Open any book and it will wait for you here.",
                vec![(
                    "Open My Library",
                    crate::widgets::pill_button::PillButtonKind::Ghost,
                    Box::new(move || {
                        open.output(HomeOut::OpenLibrary).ok();
                    }) as Box<dyn Fn()>,
                )],
            );
            card.set_halign(gtk::Align::Center);
            card
        }
    };
    host.append(&card);
}

/// One card from §60: cover page left, reading page right, ribbon and play
/// button over it — two equal pages at the app's own cover ratio, so 450 x 360
/// rather than §60's 560 x 360 (B21). Cover click opens the book's page; play
/// resumes in the reader at the saved position.
/// How much gathered scroll one book costs. A trackpad swipe piles up
/// several units in its first few events, so four still answers at once
/// while a resting finger or a stray flick spends nothing (B32: the
/// owner asked for less sensitive, twice).
const SCROLL_STEP: f64 = 4.0;
/// How long the scroll stream must hold its breath before the detent
/// opens again. Long enough that a hesitant swipe stays one gesture.
const SCROLL_QUIET_MS: u64 = 350;

/// Push the detent's quiet moment further off: once the scroll stream has
/// been silent this long the lock opens, pending stroke resets, and the
/// next gesture may spend the next book.
fn arm_quiet(
    locked: &std::rc::Rc<std::cell::Cell<bool>>,
    pending: &std::rc::Rc<std::cell::Cell<f64>>,
    quiet: &std::rc::Rc<std::cell::RefCell<Option<gtk::glib::SourceId>>>,
) {
    if let Some(id) = quiet.borrow_mut().take() {
        id.remove();
    }
    let locked = locked.clone();
    let pending = pending.clone();
    // The closure moves what it captures, so it gets clones of its own
    // and the originals stay here to store the source id.
    let quiet_cb = quiet.clone();
    let id =
        gtk::glib::timeout_add_local_once(std::time::Duration::from_millis(SCROLL_QUIET_MS), move || {
        locked.set(false);
        pending.set(0.0);
        *quiet_cb.borrow_mut() = None;
    });
    *quiet.borrow_mut() = Some(id);
}

fn build_book_card(
    card: &CardData,
    current: usize,
    total: usize,
    sender: &ComponentSender<HomePageModel>,
) -> gtk::Widget {
    let CardData {
        book,
        chapter_title,
        excerpt,
    } = card;

    let overlay = gtk::Overlay::new();
    overlay.add_css_class("kalam-home-book-card");
    overlay.set_size_request(CARD_W, COVER_H);
    overlay.set_halign(gtk::Align::Start);

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);

    // Cover page.
    let art = cover_widget_deferred(book.cover_path.as_deref(), COVER_W, COVER_H);
    art.add_css_class("kalam-home-cover-art");
    // The frame arrives top-aligned for the library grid, where covers
    // of different heights share a row. In the card it fills: whatever
    // the body's height ends up being -- and a two-pixel seam between
    // the cover's bottom edge and the card's border survived three
    // attempts to explain it (B33, B35) -- a filling frame meets the
    // border exactly, and two pixels in 360 is not a ratio anyone sees.
    art.set_valign(gtk::Align::Fill);
    let cover = gtk::Button::builder().child(&art).build();
    cover.add_css_class("flat");
    cover.add_css_class("kalam-home-cover");
    cover.set_tooltip_text(Some(&book.title));
    let s = sender.clone();
    let id = book.id;
    cover.connect_clicked(move |_| {
        s.output(HomeOut::Book { book_id: id }).ok();
    });
    body.append(&cover);

    // Reading page: the chapter line in Label style, the chapter name in Card
    // title, the excerpt under a 96px fade to the card colour (§60). Step B's
    // chapter-head store does not exist yet, so the kicker says what the card
    // is and the Card-title line carries the book's own title — real data in
    // the slot the chapter name will take, rather than a hole (B21). Decision
    // 188's "no text beside the book" is about the space outside the card.
    // An overlay with a fixed slot for its main child and the whole
    // reading page hanging over it: a fill-aligned overlay child
    // contributes nothing to the overlay's measured size (the cover's
    // picture and the excerpt slot both prove it), so the page reports
    // exactly 225 x 360 however long the title, however the passage
    // wraps, and whatever fonts the machine has -- the owner's last
    // photograph came from a machine whose fallback fonts made twenty
    // capped characters wider than the text column, and the page's
    // preferred width lifted the card once more (B34). With the slot,
    // the card's own minimums are the only sizes left in the
    // negotiation, on every machine and every book.
    let page = gtk::Overlay::new();
    page.add_css_class("kalam-home-book-page");
    page.set_overflow(gtk::Overflow::Hidden);
    // The slot is the content box, not the page: the page's padding
    // (32 above and below, 24 each side, §474's built numbers) sits on
    // the overlay itself, so a slot of the full 225 x 360 made the page
    // 273 x 424 and the card followed (CI caught it: card 496 x 426).
    // 177 x 296 is the column and the height the flow inside has
    // always had; plus the padding, the page reports exactly one
    // cover page.
    let page_slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page_slot.set_size_request(PAGE_W - 48, COVER_H - 64);
    page.set_child(Some(&page_slot));
    let page_flow = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.add_overlay(&page_flow);
    page.set_vexpand(true);

    let line = gtk::Label::new(Some("Continue reading"));
    line.add_css_class("kalam-home-book-line");
    line.set_halign(gtk::Align::Start);
    // No ellipsis here: the kicker is this one fixed string and at §3's
    // Label size it fits the 177px text column, so an ellipsis on it can only
    // mean the column shrank — which is worth seeing as overflow, not hiding.
    page_flow.append(&line);

    let title: &str = if chapter_title.is_empty() {
        &book.title
    } else {
        chapter_title
    };
    let chapter = gtk::Label::new(Some(title));
    chapter.add_css_class("kalam-home-book-chapter");
    chapter.set_halign(gtk::Align::Start);
    chapter.set_ellipsize(gtk::pango::EllipsizeMode::End);
    // The cap is visual only now -- the page's slot holds the card's
    // width whatever the label prefers (B34) -- and at the wider page
    // of B35 the column is 212px, which is twenty-four characters of
    // the Card title face.
    chapter.set_max_width_chars(24);
    chapter.set_margin_top(6);
    page_flow.append(&chapter);

    let excerpt_box = gtk::Overlay::new();
    excerpt_box.set_margin_top(20);
    excerpt_box.set_vexpand(true);
    excerpt_box.set_overflow(gtk::Overflow::Hidden);
    // A fixed slot is the overlay's main child and the text hangs over
    // it: an overlay child aligned to fill contributes nothing to the
    // overlay's own measured size -- the cover's picture proves it,
    // real textures and all -- so however the passage wraps,
    // ellipsizes or refuses to, the page's natural size is this slot's
    // and the card's rectangle never moves (B33). The slot's height is
    // six lines at the rendered 36px pitch; where a machine's fonts
    // render shorter, the slot simply reserves the room.
    let slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
    slot.set_size_request(-1, 216);
    excerpt_box.set_child(Some(&slot));
    let text = gtk::Label::new(Some(if excerpt.is_empty() {
        SHOWCASE_EXCERPT
    } else {
        excerpt
    }));
    text.add_css_class("kalam-home-book-excerpt");
    text.set_wrap(true);
    // Six lines and no more, and this cap is about the card's bottom
    // edge, not the excerpt's: the page's natural height is however
    // many lines the passage wraps to, and the card -- whose 360 is a
    // floor, not a ceiling -- grew to match it, book by book, leaving
    // the cover page short of the bottom edge by the difference (23px
    // in the CI frames, a different number per book in the owner's).
    // At the rendered 36px line pitch six lines keep the page's
    // natural height inside the cover's 360, so the card is 360 high
    // for every book and both pages meet the bottom edge together.
    // The 96 fade still lands across the last lines, saying "there is
    // more" exactly as §60 wants.
    text.set_lines(6);
    // And ellipsized, because GTK only lets `lines` cap the label's
    // natural height when it also ellipsizes (the property's own
    // documentation: without wrap or ellipsize it does nothing, and
    // with wrap alone it caps the height request while the natural
    // height -- the one this card's size follows -- stays the whole
    // passage). The end ellipsis lands on the sixth line, under the
    // opaque tail of the fade, so the fade is what the eye reads.
    text.set_ellipsize(gtk::pango::EllipsizeMode::End);
    // No width cap on the text itself any more: as an overlay child
    // its measured size never reaches the page's, and the wrap it
    // draws follows the slot's width, which follows the page's 225.
    text.set_xalign(0.0);
    text.set_yalign(0.0);
    text.set_valign(gtk::Align::Start);
    text.set_halign(gtk::Align::Fill);
    excerpt_box.add_overlay(&text);
    let fade = gtk::Box::new(gtk::Orientation::Vertical, 0);
    fade.add_css_class("kalam-home-excerpt-fade");
    fade.set_size_request(-1, 96);
    fade.set_valign(gtk::Align::End);
    fade.set_can_target(false);
    excerpt_box.add_overlay(&fade);
    page_flow.append(&excerpt_box);

    body.append(&page);
    overlay.set_child(Some(&body));

    // Ribbon: 18×36 accent, 32 from the right, hanging from the top edge
    // (§60), and notched as decision 16 draws it -- a V cut into the
    // bottom, the shape that reads as a bookmark rather than a tab. CSS
    // has no clip path, so the notch is Cairo: a drawing area over the
    // card, reading the accent token from a probe like every other drawn
    // component does.
    let ribbon_probe = crate::widgets::paint::color_probe("kalam-ui-token-accent");
    let ribbon = gtk::DrawingArea::new();
    ribbon.set_size_request(18, 36);
    ribbon.set_halign(gtk::Align::End);
    ribbon.set_valign(gtk::Align::Start);
    ribbon.set_margin_end(32);
    ribbon.set_can_target(false);
    let probe_for_ribbon = ribbon_probe.clone();
    ribbon.set_draw_func(move |_area, cr, width, height| {
        let notch = 8.0f64;
        cr.move_to(0.0, 0.0);
        cr.line_to(width as f64, 0.0);
        cr.line_to(width as f64, height as f64);
        cr.line_to(width as f64 / 2.0, height as f64 - notch);
        cr.line_to(0.0, height as f64);
        cr.close_path();
        let accent = crate::widgets::paint::probe_color(&probe_for_ribbon);
        crate::widgets::paint::set_source(cr, accent, 1.0);
        let _ = cr.fill();
    });
    overlay.add_overlay(&ribbon);
    overlay.add_overlay(&ribbon_probe);

    // Play: the triangle is the page colour cut out of the accent disc —
    // the rail's active face (§56), reused for §60's button.
    let play = gtk::Button::new();
    play.add_css_class("kalam-home-play");
    play.set_size_request(40, 40);
    let icon = rail_icons::image_px("play", Face::CutOut, 16);
    rail_icons::remember_fixed(&icon, "play", Face::CutOut);
    play.set_child(Some(&icon));
    play.set_tooltip_text(Some("Resume reading"));
    play.set_halign(gtk::Align::End);
    play.set_valign(gtk::Align::End);
    play.set_margin_end(20);
    play.set_margin_bottom(20);
    let s = sender.clone();
    let id = book.id;
    play.connect_clicked(move |_| {
        s.output(HomeOut::Read { book_id: id }).ok();
    });
    overlay.add_overlay(&play);

    // Decision 188, as the owner re-ruled it and then re-ruled again once
    // their hand was on it: the card does not scroll, it stops. One gesture
    // spends exactly one book -- the first stroke past the threshold steps,
    // then a lock holds until the scroll stream has been quiet for a beat,
    // so a trackpad fling of any length lands on the neighbour and stops
    // there, and the next book waits for the next gesture. Down is next, up
    // previous, wrapping at both ends; the pager dots repaint through the
    // same Switch message.
    if total > 1 {
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        let locked = std::rc::Rc::new(std::cell::Cell::new(false));
        let pending = std::rc::Rc::new(std::cell::Cell::new(0.0f64));
        let quiet: std::rc::Rc<std::cell::RefCell<Option<gtk::glib::SourceId>>> =
            std::rc::Rc::new(std::cell::RefCell::new(None));
        // The closure outlives the borrow, so it carries a clone.
        let step_sender = sender.clone();
        scroll.connect_scroll(move |_ctrl, _dx, dy| {
            if dy == 0.0 {
                return gtk::glib::Propagation::Stop;
            }
            if locked.get() {
                // The same gesture still talking: push the quiet moment
                // further off, spend nothing.
                arm_quiet(&locked, &pending, &quiet);
                return gtk::glib::Propagation::Stop;
            }
            pending.set(pending.get() + dy);
            if pending.get().abs() < SCROLL_STEP {
                return gtk::glib::Propagation::Stop;
            }
            let step = if pending.get() > 0.0 { 1 } else { total - 1 };
            pending.set(0.0);
            locked.set(true);
            arm_quiet(&locked, &pending, &quiet);
            step_sender.input(HomeMsg::Switch((current + step) % total));
            gtk::glib::Propagation::Stop
        });
        overlay.add_controller(scroll);
    }

    overlay.upcast()
}

/// The pager column (§60): five dots, gap 10; the current one an 8×22
/// accent pill, the rest 8px circles at 45% of text-2. Dots are buttons, so
/// the pager is keyboard-reachable for free.
fn build_pager(
    cards: &[CardData],
    current: usize,
    sender: &ComponentSender<HomePageModel>,
) -> gtk::Widget {
    let pager = gtk::Box::new(gtk::Orientation::Vertical, 10);
    pager.set_valign(gtk::Align::Center);
    for (i, card) in cards.iter().enumerate().take(PAGER_MAX) {
        let dot = gtk::Button::new();
        dot.add_css_class("kalam-home-pager-dot");
        if i == current {
            dot.add_css_class("current");
            dot.set_size_request(8, 22);
        } else {
            dot.set_size_request(8, 8);
        }
        dot.set_tooltip_text(Some(&card.book.title));
        let s = sender.clone();
        dot.connect_clicked(move |_| {
            s.input(HomeMsg::Switch(i));
        });
        pager.append(&dot);
    }
    pager.upcast()
}

/// Reads the snapshot on a worker, then enriches it with what §60's card
/// paints, so the apply half touches no database (7.1 step 2b).
fn request_snapshot(service: &LibraryService, sender: &ComponentSender<HomePageModel>) {
    let service = LibraryService::new(service.catalog().clone());
    let s = sender.input_sender().clone();
    crate::tasks::spawn_internal(
        "Reading home",
        move |_reporter| {
            let snap = service.home();
            let cards = pager_cards(service.catalog());
            Box::new(HomePayload { snap, cards })
        },
        |_| {},
        move |payload| {
            let _ = s.send(HomeMsg::Loaded(payload));
        },
    );
}

/// The pager's books (§60): every book with a saved reading position, most
/// recently read first, up to the five dots. `reading_progress` is the
/// truth about "in progress" — the dashboard's fallback chain could end at
/// "newest book in the library", and a book nobody opened does not belong
/// on this card (B12).
///
/// The chapter name and excerpt stay empty for now (B17): the FTS index
/// cannot answer "one chapter of one book" without scanning every word of
/// every book — its book and chapter columns are stored *unindexed* — and
/// one such scan behind the catalog's single connection lock froze startup
/// on a real library. They return when chapter heads get a small keyed
/// store of their own; until then the card shows its quiet fallbacks.
fn pager_cards(catalog: &Catalog) -> Vec<CardData> {
    let cards: Vec<CardData> = catalog
        .books_in_progress(PAGER_MAX)
        .unwrap_or_default()
        .into_iter()
        .map(|(book, _chapter_index)| CardData {
            book,
            chapter_title: String::new(),
            excerpt: String::new(),
        })
        .collect();

    cards
}

/// Warm the covers the pager can land on, not just the visible one (§60).
///
/// Called from the apply half, never from the worker that read the cards:
/// warming starts a task, starting a task spawns onto the UI loop, and only
/// the UI thread may do that. From a worker, glib panics — and takes the
/// worker with it, so the snapshot it was building never arrives and the card
/// stays a skeleton for the life of the page (pitfalls §107).
fn warm_pager_covers(cards: &[CardData], visible_from: usize) {
    let books: Vec<Book> = cards.iter().map(|c| c.book.clone()).collect();
    if !books.is_empty() {
        crate::preload::warm_books(&books, visible_from, COVER_W, COVER_H);
    }
}

/// The local clock, or UTC when the system has no local time configured.
/// Every glib `DateTime` constructor is fallible, so the honest floor is
/// `None`: a machine that cannot read its own clock at all gets a neutral
/// greeting and no date line — not a panic, and not a wrong time of day.
fn local_now() -> Option<gtk::glib::DateTime> {
    gtk::glib::DateTime::now_local()
        .or_else(|_| gtk::glib::DateTime::now_utc())
        .ok()
}

/// The greeting's first line (home-spec §1). The second is `GREETING_NAME`.
/// Turn the showcase rows into the widget's entries. Kept beside the other
/// page helpers so the invented content sits in one place (B21).
fn showcase_updates() -> Vec<updates::UpdateEntry> {
    SHOWCASE_UPDATES
        .iter()
        .copied()
        .map(|(title, detail, time, fresh)| updates::UpdateEntry {
            title: title.to_string(),
            detail: detail.to_string(),
            time: time.to_string(),
            fresh,
        })
        .collect()
}

/// Turn the showcase rows into the box's items (B21).
fn showcase_search() -> Vec<search_box::SearchItem> {
    SHOWCASE_SEARCH
        .iter()
        .copied()
        .map(|(group, title, meta)| search_box::SearchItem {
            group,
            title: title.to_string(),
            meta: meta.to_string(),
        })
        .collect()
}

/// Turn the invented recent searches into the list the box edits.
fn showcase_history() -> Vec<String> {
    SHOWCASE_HISTORY
        .iter()
        .map(|query| query.to_string())
        .collect()
}

fn greeting_line() -> String {
    match local_now().map(|now| now.hour()) {
        Some(5..=11) => "Good morning".to_string(),
        Some(12..=17) => "Good afternoon".to_string(),
        Some(_) => "Good evening".to_string(),
        None => "Hello".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// B13: the placeholder must stay a real text/attribution pair, so
    /// §63's element never renders with an empty byline while the real
    /// source is being settled.
    #[test]
    fn placeholder_quote_has_text_and_attribution() {
        assert!(!PLACEHOLDER_QUOTE.0.is_empty());
        assert!(!PLACEHOLDER_QUOTE.1.is_empty());
    }

    /// The cover page keeps the cover ratio the rest of the app uses,
    /// and the card is that cover page plus the wider reading page of
    /// the owner's ruling -- B35: "keep the cover side fix, but make
    /// the details side wider" -- which supersedes B21's equal-pages
    /// pin. The ratio half stays: the numbers used to disagree with
    /// `book_row::COVER_ASPECT` and nothing said so.
    #[test]
    fn the_card_is_two_pages_at_the_cover_ratio() {
        let ratio = f64::from(COVER_H) / f64::from(COVER_W);
        assert!(
            (ratio - crate::widgets::book_row::COVER_ASPECT).abs() < 0.01,
            "a page must hold the app's cover ratio, got 1:{ratio:.3}"
        );
        assert_eq!(CARD_W, COVER_W + PAGE_W, "cover page plus reading page");
        assert_eq!((CARD_W, COVER_H), (485, 360), "the card B35 rules");
        assert_eq!(COVER_W, 225, "the cover side stays fixed");
    }

    /// §64 sizes the state faces against the column and the book: the
    /// empty and error cards span the 600 column, "Nothing in
    /// progress" stands a little narrower where the book would, and
    /// all three are exactly the book card's height (B36).
    #[test]
    fn state_cards_hold_section_64_sizes() {
        assert_eq!((STATE_W, STATE_H), (600, 360), "the column-wide faces");
        assert_eq!((STATE_W_NONE, STATE_H), (560, 360), "nothing in progress");
    }

    /// The greeting's second line is a name, not a date: decision 186 draws
    /// two Display lines and home-spec §7 names the placeholder. Pinned so a
    /// future edit does not quietly put the date back (B21).
    #[test]
    fn the_greeting_stands_on_two_display_lines() {
        assert!(!GREETING_NAME.is_empty());
        assert!(!GREETING_NAME.contains(','), "a name, not a formatted date");
    }
}
