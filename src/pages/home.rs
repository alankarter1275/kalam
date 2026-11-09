//! Home — the reader's front page (design: `home-spec.md`, design-system
//! §57–§66).
//!
//! Stage 1 of the Home build (`docs/build-decisions.md` B12): two columns —
//! greeting, the quote placeholder (§63, B13) and the shelf heading on the
//! left; the Home book card with its pager (§60) on the right — plus the
//! state cards (§64) covering everything not built yet. The reading ruler is
//! stage 2, the search bar stage 3 (its 520×40 row is reserved from day one,
//! so the bar drops in without moving anything), and Updates is not drawn
//! until something feeds it (§62's own rule, B15). The old dashboard left
//! the day this landed, per B12 — but its import machinery stayed, because
//! the empty-library state card's "Add books" action is a real import path.
//!
//! Same skeleton → worker → apply shape the dashboard used (7.1 step 2b):
//! `init` paints placeholders, a worker reads the snapshot *and* the card
//! excerpts, `Loaded` paints. Home never touches the network (decision 110).

use crate::db::Catalog;
use crate::models::Book;
use crate::pages::all_books::{ask_about_restored, import_summary, spawn_import, ImportTally};
use crate::rail_icons::{self, Face};
use crate::service::{HomeSnapshot, LibraryService};
use crate::widgets::book_row::cover_widget_deferred;
use crate::widgets::surface;
use gtk::prelude::*;
use relm4::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;

/// The Home book card's cover page (§60): the left 280 of its 560 width.
const COVER_W: i32 = 280;
const COVER_H: i32 = 360;
/// How many in-progress books the pager holds (§60: five dots).
const PAGER_MAX: usize = 5;
/// Excerpt length in words. The card fades out at 96px (§60), so more text
/// than this is never visible; the cap keeps the label cheap to lay out.
const EXCERPT_WORDS: usize = 70;

/// The placeholder quote (B13): §63's element drawn in full, so the real
/// source drops into this one constant when the owner settles it. Public
/// domain — Dickinson's couplet about what a book does.
const PLACEHOLDER_QUOTE: (&str, &str) = (
    "There is no Frigate like a Book\nTo take us Lands away",
    "Emily Dickinson",
);

/// Navigation requests from Home. Variants are named for their destination
/// rather than sharing an `Open` prefix (clippy::enum_variant_names) -- the
/// same convention `LibraryOut` uses. The dashboard's `BookDialog`,
/// `AllBooks` and `ComicSeries` left with it (B12); a later stage adds back
/// whatever it emits, and `app.rs` grows an arm for it then.
#[derive(Debug)]
pub enum HomeOut {
    /// The cover click (§60): open the book's details page.
    Book {
        book_id: i64,
    },
    /// The play button (§60): resume in the reader at the saved position.
    /// No chapter rides along — `OpenReaderAt` with one *resets* the
    /// fraction inside the chapter, and resuming must not move the position
    /// the card was just showing. `None` opens at the stored spot.
    Read {
        book_id: i64,
    },
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

/// One pager card's worth of §60. The chapter index the excerpt came from
/// stays in the worker: nothing on this side of the message needs it —
/// "resume" deliberately carries no chapter (see `HomeOut::Read`).
#[derive(Debug)]
struct CardData {
    book: Book,
    /// The saved position's chapter title, from the FTS index; empty until
    /// the book is indexed.
    chapter_title: String,
    /// That chapter's first words; empty until indexed, and the card says so.
    excerpt: String,
}

pub struct HomePageModel {
    service: LibraryService,
    importing: bool,
    /// Which pager card the book card shows (§60).
    current: usize,
    /// Kept so a pager click repaints without another worker run.
    cards: Vec<CardData>,
    /// The empty-library state card (§64) needs the difference between
    /// "nothing in progress" and "nothing at all".
    library_empty: bool,
}

#[relm4::component(pub)]
impl Component for HomePageModel {
    type Init = Arc<Catalog>;
    type Input = HomeMsg;
    type Output = HomeOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_hexpand: true,
            set_vexpand: true,

            // The search row (§57): 28px above it, 520×40 — empty until
            // stage 3 lands the underlined bar and its result box (B12).
            // Reserving the row now is the ruling: the bar drops in later
            // without moving anything below it.
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_size_request: (520, 40),
                set_margin_top: 28,
                set_margin_start: 56,
                set_halign: gtk::Align::Start,
            },

            // The two columns (home-spec §1).
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_hexpand: true,
                set_vexpand: true,

                // Left column: 470 wide, 56 from the rail.
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_width_request: 470,
                    set_margin_start: 56,
                    set_halign: gtk::Align::Start,
                    set_valign: gtk::Align::Start,

                    // The greeting, two Display lines (home-spec §1). The
                    // second line is the date, not a name, until the
                    // customization area in Settings exists (B14).
                    gtk::Label {
                        set_label: &greeting_line(),
                        add_css_class: "kalam-home-greeting",
                        set_halign: gtk::Align::Start,
                    },
                    gtk::Label {
                        set_label: &date_line(),
                        add_css_class: "kalam-home-greeting-date",
                        set_halign: gtk::Align::Start,
                        set_margin_top: 6,
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
                    // the ruler itself is stage 2 (B12), and the dashed card
                    // below says so instead of leaving a hole. The chip is
                    // inert until stage 2 wires the Arrange sheet.
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

                // Right column: 600 wide, its top 52 below the search row.
                // The board nudges the column 24 past the page's right
                // margin; the shell has no negative margin, so it stops 24
                // short of the edge instead — stage 4 checks the two against
                // the board.
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_width_request: 600,
                    set_hexpand: true,
                    set_halign: gtk::Align::End,
                    set_valign: gtk::Align::Start,
                    set_margin_top: 52,
                    set_margin_end: 24,

                    #[name = "card_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 12,
                        set_halign: gtk::Align::End,
                    },

                    // No Updates block: §62's timeline is not drawn until
                    // something feeds it (B15). Online sources are off
                    // until Part 2 (B4); a local-events feed is an open
                    // option, not a promise.
                },
            },
        }
    }

    fn init(
        catalog: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = HomePageModel {
            service: LibraryService::new(catalog),
            importing: false,
            current: 0,
            cards: Vec::new(),
            library_empty: false,
        };
        let widgets = view_output!();

        // Stage-1 placeholders (B12): the shelf area says what is coming,
        // and the card host shows the dashed block the snapshot replaces a
        // moment later.
        let shelf_card = surface::state_card(
            "Your shelf",
            "Weeks, months and years of your reading, drawn as lines on a \
             ruler — that arrives with the next stage of this build.",
        );
        shelf_card.set_valign(gtk::Align::Center);
        shelf_card.set_vexpand(true);
        widgets.shelf_host.append(&shelf_card);

        let skeleton = gtk::Box::new(gtk::Orientation::Vertical, 0);
        skeleton.add_css_class("kalam-placeholder");
        skeleton.set_size_request(560, 360);
        widgets.card_host.append(&skeleton);

        request_snapshot(&model.service, &sender);

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
                if self.current >= self.cards.len() {
                    self.current = 0;
                }
                paint_right(
                    widgets,
                    &self.cards,
                    self.current,
                    self.library_empty,
                    &sender,
                );
            }
            HomeMsg::Switch(i) => {
                if i < self.cards.len() {
                    self.current = i;
                    paint_right(
                        widgets,
                        &self.cards,
                        self.current,
                        self.library_empty,
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

/// The right column's card: the book card with its pager (§60), or the
/// state card (§64) that covers it until there is something to show.
fn paint_right(
    widgets: &HomePageModelWidgets,
    cards: &[CardData],
    current: usize,
    library_empty: bool,
    sender: &ComponentSender<HomePageModel>,
) {
    let host = &widgets.card_host;
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }

    if cards.is_empty() {
        let card = if library_empty {
            let s = sender.clone();
            surface::state_card_with_action(
                "Your library is empty",
                "Add the books you are reading; the last one you opened will \
                 wait for you here.",
                "Add books",
                move || {
                    s.input(HomeMsg::AddBooks);
                },
            )
        } else {
            // §64's drawn empty-shelf variant stands on the ruler's
            // baseline, which is stage 2; the dashed card covers the gap
            // until then (B12).
            surface::state_card(
                "Nothing in progress",
                "Open any book and it will wait for you here.",
            )
        };
        card.set_size_request(560, 360);
        card.set_valign(gtk::Align::Center);
        host.append(&card);
        return;
    }

    host.append(&build_book_card(&cards[current], sender));
    host.append(&build_pager(cards, current, sender));
}

/// One card from §60: cover page left, reading page right, ribbon and play
/// button over it, 560×360. Cover click opens the book's page; play resumes
/// in the reader at the saved position.
fn build_book_card(card: &CardData, sender: &ComponentSender<HomePageModel>) -> gtk::Widget {
    let CardData {
        book,
        chapter_title,
        excerpt,
    } = card;

    let overlay = gtk::Overlay::new();
    overlay.add_css_class("kalam-home-book-card");
    overlay.set_size_request(560, 360);
    overlay.set_halign(gtk::Align::Start);

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);

    // Cover page.
    let art = cover_widget_deferred(book.cover_path.as_deref(), COVER_W, COVER_H);
    art.add_css_class("kalam-home-cover-art");
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

    // Reading page: chapter line (Label), chapter name (Card title),
    // excerpt under a 96px fade to the card colour (§60).
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.add_css_class("kalam-home-book-page");
    page.set_hexpand(true);
    page.set_vexpand(true);

    let line = gtk::Label::new(Some(&book.title));
    line.add_css_class("kalam-home-book-line");
    line.set_halign(gtk::Align::Start);
    line.set_ellipsize(gtk::pango::EllipsizeMode::End);
    page.append(&line);

    let title: &str = if chapter_title.is_empty() {
        "Where you left off"
    } else {
        chapter_title
    };
    let chapter = gtk::Label::new(Some(title));
    chapter.add_css_class("kalam-home-book-chapter");
    chapter.set_halign(gtk::Align::Start);
    chapter.set_ellipsize(gtk::pango::EllipsizeMode::End);
    chapter.set_margin_top(6);
    page.append(&chapter);

    let excerpt_box = gtk::Overlay::new();
    excerpt_box.set_margin_top(20);
    excerpt_box.set_vexpand(true);
    let text = gtk::Label::new(Some(if excerpt.is_empty() {
        "The excerpt appears once this book finishes indexing."
    } else {
        excerpt
    }));
    text.add_css_class("kalam-home-book-excerpt");
    if excerpt.is_empty() {
        // Not an error — indexing runs in the background after import.
        text.add_css_class("kalam-muted");
    }
    text.set_wrap(true);
    text.set_xalign(0.0);
    text.set_yalign(0.0);
    text.set_valign(gtk::Align::Start);
    excerpt_box.set_child(Some(&text));
    let fade = gtk::Box::new(gtk::Orientation::Vertical, 0);
    fade.add_css_class("kalam-home-excerpt-fade");
    fade.set_size_request(-1, 96);
    fade.set_valign(gtk::Align::End);
    fade.set_can_target(false);
    excerpt_box.add_overlay(&fade);
    page.append(&excerpt_box);

    body.append(&page);
    overlay.set_child(Some(&body));

    // Ribbon: 18×36 accent, 32 from the right (§60). The board's notched
    // bottom would need a clip path, which GTK CSS does not have; the
    // rounded rectangle is the honest approximation.
    let ribbon = gtk::Box::new(gtk::Orientation::Vertical, 0);
    ribbon.add_css_class("kalam-home-ribbon");
    ribbon.set_size_request(18, 36);
    ribbon.set_halign(gtk::Align::End);
    ribbon.set_valign(gtk::Align::Start);
    ribbon.set_margin_end(32);
    ribbon.set_can_target(false);
    overlay.add_overlay(&ribbon);

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
/// on this card (B12). Chapter title and excerpt come from the FTS index
/// (v15), so no book file is opened and no read happens on the UI thread.
fn pager_cards(catalog: &Catalog) -> Vec<CardData> {
    let cards: Vec<CardData> = catalog
        .books_in_progress(PAGER_MAX)
        .unwrap_or_default()
        .into_iter()
        .map(|(book, chapter_index)| {
            let (chapter_title, excerpt) =
                crate::content_index::chapter_text(catalog, book.id, chapter_index)
                    .ok()
                    .flatten()
                    .map(|(title, content)| (title, first_words(&content, EXCERPT_WORDS)))
                    .unwrap_or_default();
            CardData {
                book,
                chapter_title,
                excerpt,
            }
        })
        .collect();

    // The card shows covers, and covers decode lazily; warm every book the
    // pager can land on, not just the visible one (the dashboard warmed its
    // strips the same way).
    let books: Vec<Book> = cards.iter().map(|c| c.book.clone()).collect();
    if !books.is_empty() {
        crate::preload::warm_books(&books, 0, COVER_W, COVER_H);
    }
    cards
}

/// The excerpt: the first `words` words of the chapter, whitespace
/// collapsed — the FTS content column carries the raw text, newlines and
/// all, and the card shows prose, not a transcript.
fn first_words(content: &str, words: usize) -> String {
    let mut out = String::new();
    for word in content.split_whitespace().take(words) {
        if !out.is_empty() {
            out.push(' ');
        }
        out.push_str(word);
    }
    out
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

/// The greeting's first line (home-spec §1). No name on the second line
/// until the customization area in Settings exists (B14).
fn greeting_line() -> String {
    match local_now().map(|now| now.hour()) {
        Some(5..=11) => "Good morning".to_string(),
        Some(12..=17) => "Good afternoon".to_string(),
        Some(_) => "Good evening".to_string(),
        None => "Hello".to_string(),
    }
}

/// The greeting's second line: today's date standing in for the name (B14),
/// e.g. "Saturday, 08 November". Empty only when the clock is unreadable.
fn date_line() -> String {
    local_now()
        .and_then(|now| now.format("%A, %d %B").ok())
        .map(|s| s.to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The excerpt collapses the FTS text's newlines and stops at the word
    /// budget — the card fades out at 96px (§60), so anything longer is
    /// dead weight in the label's layout.
    #[test]
    fn excerpt_collapses_whitespace_and_stops_at_the_budget() {
        let raw = "One two\n\nthree   four\nfive six seven";
        assert_eq!(first_words(raw, 4), "One two three four");
        assert_eq!(first_words(raw, 100), "One two three four five six seven");
        assert_eq!(first_words("", 10), "");
    }

    /// B13: the placeholder must stay a real text/attribution pair, so
    /// §63's element never renders with an empty byline while the real
    /// source is being settled.
    #[test]
    fn placeholder_quote_has_text_and_attribution() {
        assert!(!PLACEHOLDER_QUOTE.0.is_empty());
        assert!(!PLACEHOLDER_QUOTE.1.is_empty());
    }

    /// The date line must not be empty wherever a clock reads at all — the
    /// UTC fallback exists for machines with no local timezone configured,
    /// and the greeting's second line has nothing else to stand on (B14).
    /// Only an unreadable clock may produce "".
    #[test]
    fn date_line_is_never_empty() {
        if gtk::glib::DateTime::now_utc().is_ok() {
            assert!(!date_line().is_empty());
        }
    }
}
