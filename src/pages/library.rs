//! My Library — a single at-a-glance dashboard (v5 mockup:
//! `docs/files/kalam_my_library_v5.html`).
//!
//! Top row: the book currently being read (cover, Read button, progress and
//! a four-chapter timeline centered on the current chapter) beside a 2×2
//! stats grid — sparklines for the headline figures and the yearly goal.
//! Below: continue reading (with hover play buttons), saved quotes, a
//! merged history feed (events + sessions) and a vocabulary strip.
//! Section headers double as the way in — click a header to drill down.
//! Sections only render when they have content, though, so the pages that
//! have no section here (or whose section is empty) are reached from the
//! quick-links row under the title instead.

use crate::db::{Catalog, EventKind};
use crate::models::{Book, BookFormat, LibrarySection};
use crate::pages::history::pretty_day;
use crate::service::{DashboardSnapshot, LibraryService};
use crate::widgets::book_row::cover_widget;
use crate::widgets::charts::{monthly_series, sparkline};
use gtk::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

/// Navigation requests from the dashboard. Variants are named for their
/// destination rather than sharing an `Open` prefix (clippy::enum_variant_names).
#[derive(Debug)]
pub enum LibraryOut {
    Section(LibrarySection),
    Book {
        book_id: i64,
    },
    BookDialog {
        book_id: i64,
    },
    /// Resume reading a book (the continue-reading play buttons).
    Read {
        book_id: i64,
    },
}

pub struct LibraryPageModel {
    service: LibraryService,
}

#[relm4::component(pub)]
impl SimpleComponent for LibraryPageModel {
    type Init = Arc<Catalog>;
    type Input = ();
    type Output = LibraryOut;

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 18,
            set_hexpand: true,

            #[name = "body"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 22,
                set_hexpand: true,
            },
        }
    }

    fn init(
        catalog: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = LibraryPageModel {
            service: LibraryService::new(catalog),
        };
        let widgets = view_output!();
        let snap = model.service.dashboard(FEED_LIMIT);
        report_errors(&snap.errors);
        build_dashboard(&widgets.body, &snap, &model.service, &sender);
        ComponentParts { model, widgets }
    }
}

/// How many feed rows the dashboard shows.
const FEED_LIMIT: usize = 4;

/// Surface read failures instead of rendering them as an empty dashboard.
/// The service collects them; deciding what the user sees stays with the UI.
fn report_errors(errors: &[String]) {
    for err in errors {
        crate::notify::error("Could not read your library", err);
    }
}

/// `snap` carries everything the page reads in one go; `service` is still
/// needed for the per-row lookups the feed does (progress, book format) and
/// for the cover/chapter work the cards do.
fn build_dashboard(
    body: &gtk::Box,
    snap: &DashboardSnapshot,
    service: &LibraryService,
    sender: &ComponentSender<LibraryPageModel>,
) {
    let catalog = service.catalog();
    let stats = &snap.stats;

    // ── header ──────────────────────────────────────────────────────────
    let head = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let title = gtk::Label::new(Some("My Library"));
    title.add_css_class("kalam-page-title");
    title.set_halign(gtk::Align::Start);
    head.append(&title);

    let sub = gtk::Label::new(Some(&format!(
        "{} book{} · {} reading · {} finished · {} unread",
        stats.total_books,
        if stats.total_books == 1 { "" } else { "s" },
        stats.reading,
        stats.finished,
        stats.unread
    )));
    sub.add_css_class("kalam-page-sub");
    sub.set_halign(gtk::Align::Start);
    head.append(&sub);
    body.append(&head);
    body.append(&quick_links(sender));

    if stats.total_books == 0 {
        // The "All books" quick link above is the way in (that page owns the
        // importer), so there is no duplicate section header here.
        let empty = gtk::Label::new(Some(concat!(
            "Your library is empty.\n\n",
            "Import an EPUB from All books to get started — this page fills up ",
            "with your quotes, history and reading progress as you go."
        )));
        empty.add_css_class("kalam-placeholder");
        empty.set_wrap(true);
        empty.set_halign(gtk::Align::Start);
        body.append(&empty);
        return;
    }

    // ── top row: now reading | 2×2 stats grid ───────────────────────────
    let top_row = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    top_row.set_hexpand(true);

    // Now reading: the most recently opened book that isn't finished yet.
    let now = snap.recently_opened.iter().find(|b| b.progress < 100);
    if let Some(book) = now {
        top_row.append(&now_reading_card(book, catalog, sender));
    }

    let grid = gtk::Grid::new();
    grid.set_column_spacing(10);
    grid.set_row_spacing(10);
    grid.set_column_homogeneous(true);
    grid.set_hexpand(true);

    let daily: Vec<i64> = stats.minutes_by_day.iter().map(|(_, s)| *s / 60).collect();
    let cards: [(String, String, String, Vec<i64>, &str); 3] = [
        (
            "Avg time / day".to_string(),
            stats.avg_minutes_per_active_day.to_string(),
            "min".to_string(),
            daily.clone(),
            "kalam-spark-green",
        ),
        (
            "Books completed".to_string(),
            stats.finished.to_string(),
            if stats.finished == 1 {
                "book".to_string()
            } else {
                "books".to_string()
            },
            monthly_series(stats),
            "kalam-spark-red",
        ),
        (
            "Current streak".to_string(),
            stats.current_streak_days.to_string(),
            if stats.current_streak_days == 1 {
                "day".to_string()
            } else {
                "days".to_string()
            },
            daily,
            "kalam-spark-blue",
        ),
    ];
    for (i, (label, value, unit, series, spark_class)) in cards.iter().enumerate() {
        let card = stat_card(label, value, unit, series, spark_class);
        grid.attach(&card, (i % 2) as i32, (i / 2) as i32, 1, 1);
    }
    grid.attach(&goal_card(snap), 1, 1, 1, 1);
    top_row.append(&grid);
    body.append(&top_row);

    // ── continue reading ────────────────────────────────────────────────
    let mut continuing: Vec<Book> = snap.recently_opened.clone();
    if continuing.is_empty() {
        // Bounded: this used to load every book to show at most six.
        continuing = snap
            .recent
            .iter()
            .filter(|b| b.progress > 0 && b.progress < 100)
            .take(6)
            .cloned()
            .collect();
    }
    if !continuing.is_empty() {
        body.append(&section(
            "CONTINUE READING",
            LibrarySection::History,
            sender,
            continue_strip(&continuing, sender).upcast::<gtk::Widget>(),
        ));
    }

    // ── saved quotes ────────────────────────────────────────────────────
    let quotes = &snap.quotes;
    if !quotes.is_empty() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.set_homogeneous(true);
        for (anno, qref) in quotes.iter() {
            row.append(&quote_card(anno, qref));
        }
        body.append(&section(
            "SAVED QUOTES",
            LibrarySection::SavedQuotes,
            sender,
            row.upcast::<gtk::Widget>(),
        ));
    }

    // ── history (events + sessions, merged) ─────────────────────────────
    let feed = history_feed(snap, catalog);
    if !feed.is_empty() {
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for item in &feed {
            list.append(&history_row(item, sender));
        }
        body.append(&section(
            "HISTORY",
            LibrarySection::History,
            sender,
            list.upcast::<gtk::Widget>(),
        ));
    }

    // ── vocabulary ──────────────────────────────────────────────────────
    let words: Vec<_> = snap.words.iter().take(4).collect();
    if !words.is_empty() {
        let flow = gtk::FlowBox::builder()
            .selection_mode(gtk::SelectionMode::None)
            .max_children_per_line(4)
            .column_spacing(10)
            .row_spacing(10)
            .halign(gtk::Align::Start)
            .build();
        for w in words {
            let pill = gtk::Box::new(gtk::Orientation::Vertical, 3);
            pill.add_css_class("kalam-vocab-pill");
            let word = gtk::Label::new(Some(&w.word));
            word.add_css_class("kalam-vocab-word");
            word.set_halign(gtk::Align::Start);
            pill.append(&word);
            if !w.definition.trim().is_empty() {
                let def = gtk::Label::new(Some(&w.definition));
                def.add_css_class("kalam-vocab-def");
                def.set_wrap(true);
                def.set_xalign(0.0);
                def.set_max_width_chars(28);
                pill.append(&def);
            }
            let s = sender.clone();
            let click = gtk::GestureClick::new();
            click.set_button(1);
            click.connect_released(move |_, _, _, _| {
                s.output(LibraryOut::Section(LibrarySection::SavedWords))
                    .ok();
            });
            pill.add_controller(click);
            pill.set_cursor_from_name(Some("pointer"));
            flow.insert(&pill, -1);
        }
        body.append(&section(
            "VOCABULARY",
            LibrarySection::SavedWords,
            sender,
            flow.upcast::<gtk::Widget>(),
        ));
    }

    // ── lookup history (Phase 10) ────────────────────────────────────────
    let lookups = &snap.lookups;
    if !lookups.is_empty() {
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for lookup in lookups {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
            row.add_css_class("kalam-list-row");
            let word = gtk::Label::new(Some(&lookup.word));
            word.add_css_class("kalam-card-title");
            word.set_halign(gtk::Align::Start);
            word.set_xalign(0.0);
            word.set_ellipsize(gtk::pango::EllipsizeMode::End);
            row.append(&word);
            if !lookup.found {
                let miss = gtk::Label::new(Some("no definition"));
                miss.add_css_class("kalam-chip");
                miss.add_css_class("kalam-chip-neutral");
                miss.set_valign(gtk::Align::Center);
                row.append(&miss);
            }
            let time = gtk::Label::new(Some(lookup.at.get(11..16).unwrap_or("")));
            time.add_css_class("kalam-muted");
            time.set_valign(gtk::Align::Center);
            row.append(&time);
            let s = sender.clone();
            let click = gtk::GestureClick::new();
            click.set_button(1);
            click.connect_released(move |_, _, _, _| {
                s.output(LibraryOut::Section(LibrarySection::LookupHistory))
                    .ok();
            });
            row.add_controller(click);
            row.set_cursor_from_name(Some("pointer"));
            list.append(&row);
        }
        body.append(&section(
            "LOOKUP HISTORY",
            LibrarySection::LookupHistory,
            sender,
            list.upcast::<gtk::Widget>(),
        ));
    }
}

// ---------------------------------------------------------------------------
// Top row
// ---------------------------------------------------------------------------

/// The "Now reading" card: cover with a hard offset shadow, identity, a Read
/// button, the progress bar with chapter location, and a four-row chapter
/// timeline centered on the current chapter.
fn now_reading_card(
    book: &Book,
    catalog: &Arc<Catalog>,
    sender: &ComponentSender<LibraryPageModel>,
) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("kalam-now-reading");
    card.set_size_request(260, -1);

    let eyebrow = gtk::Label::new(Some("Now reading"));
    eyebrow.add_css_class("kalam-nr-eyebrow");
    eyebrow.set_halign(gtk::Align::Start);
    card.append(&eyebrow);

    // Cover + meta (title, author, Read button pinned to the bottom).
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 14);

    let cover = cover_widget(book.cover_path.as_deref(), 72, 104);
    cover.add_css_class("kalam-nr-cover");
    let cover_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    cover_box.append(&cover);
    let id = book.id;
    let s_cover = sender.clone();
    let click_cover = gtk::GestureClick::new();
    click_cover.set_button(1);
    click_cover.connect_released(move |_, _, _, _| {
        s_cover.output(LibraryOut::BookDialog { book_id: id }).ok();
    });
    cover_box.add_controller(click_cover);
    cover_box.set_cursor_from_name(Some("pointer"));
    top.append(&cover_box);

    let meta = gtk::Box::new(gtk::Orientation::Vertical, 2);
    meta.set_hexpand(true);
    let title = gtk::Label::new(Some(&book.title));
    title.add_css_class("kalam-nr-title");
    title.set_wrap(true);
    title.set_xalign(0.0);
    title.set_halign(gtk::Align::Start);
    meta.append(&title);
    let author = gtk::Label::new(Some(book.authors_display()));
    author.add_css_class("kalam-nr-author");
    author.set_halign(gtk::Align::Start);
    author.set_ellipsize(gtk::pango::EllipsizeMode::End);
    meta.append(&author);

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_vexpand(true);
    meta.append(&spacer);

    let read = gtk::Button::with_label("Read");
    read.add_css_class("kalam-nr-read");
    read.set_focus_on_click(false);
    read.set_hexpand(true);
    let id_read = book.id;
    let s_read = sender.clone();
    read.connect_clicked(move |_| {
        s_read.output(LibraryOut::Read { book_id: id_read }).ok();
    });
    meta.append(&read);
    top.append(&meta);
    card.append(&top);

    // Spine + current chapter (EPUBs only — the mockup's timeline needs it).
    let chapters = if book.format == BookFormat::Epub {
        crate::epub_book::OpenBook::open(
            &book.file_path,
            &crate::paths::reader_cache_dir(&book.uuid),
        )
        .map(|ob| ob.spine.iter().map(|s| s.title.clone()).collect::<Vec<_>>())
        .unwrap_or_default()
    } else {
        Vec::new()
    };
    let (chapter_index, _frac) = catalog
        .get_reading_progress(book.id)
        .ok()
        .flatten()
        .unwrap_or((0, 0.0));
    let current = chapter_index.min(chapters.len().saturating_sub(1));

    let bar = gtk::ProgressBar::new();
    bar.add_css_class("kalam-nr-prog");
    bar.set_fraction((book.progress as f64 / 100.0).clamp(0.0, 1.0));
    card.append(&bar);

    // The current spine entry's own title — a plain "Chapter N" would count
    // front-matter spine entries too and drift from the timeline below.
    let loc = if chapters.is_empty() {
        format!("{}% complete", book.progress)
    } else {
        format!("{}% complete · {}", book.progress, chapters[current])
    };
    let loc_label = gtk::Label::new(Some(&loc));
    loc_label.add_css_class("kalam-nr-prog-label");
    loc_label.set_halign(gtk::Align::Start);
    loc_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    card.append(&loc_label);

    // Four rows, centered on the current chapter (one behind, two ahead).
    if !chapters.is_empty() {
        let start = current.saturating_sub(1);
        let rows: Vec<(usize, String)> = chapters
            .iter()
            .enumerate()
            .skip(start)
            .take(4)
            .map(|(i, t)| (i, t.clone()))
            .collect();
        let tl = gtk::Box::new(gtk::Orientation::Vertical, 0);
        tl.add_css_class("kalam-nr-timeline");
        for (pos, (i, chapter_title)) in rows.iter().enumerate() {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);

            let col = gtk::Box::new(gtk::Orientation::Vertical, 0);
            let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            let dot_class = if *i < current {
                "kalam-nr-dot-done"
            } else if *i == current {
                "kalam-nr-dot-cur"
            } else {
                "kalam-nr-dot-fut"
            };
            dot.add_css_class(dot_class);
            if *i < current {
                // A symbolic image, not a text glyph: labels sit high in the
                // line box and the check would drift off-centre.
                let check = crate::icons::symbolic_with_classes(
                    "object-select-symbolic",
                    10,
                    &["kalam-nr-dot-check"],
                );
                check.set_halign(gtk::Align::Center);
                check.set_valign(gtk::Align::Center);
                dot.append(&check);
            }
            col.append(&dot);
            if pos + 1 < rows.len() {
                let line = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                line.add_css_class("kalam-nr-line");
                if *i < current {
                    line.add_css_class("kalam-nr-line-done");
                }
                line.set_vexpand(true);
                line.set_halign(gtk::Align::Center);
                col.append(&line);
            }
            row.append(&col);

            let name = gtk::Label::new(Some(chapter_title));
            let name_class = if *i < current {
                "kalam-nr-ch-done"
            } else if *i == current {
                "kalam-nr-ch-cur"
            } else {
                "kalam-nr-ch-fut"
            };
            name.add_css_class(name_class);
            name.set_hexpand(true);
            name.set_halign(gtk::Align::Start);
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            name.set_margin_bottom(if pos + 1 < rows.len() { 8 } else { 0 });
            row.append(&name);
            tl.append(&row);
        }
        card.append(&tl);
    }
    card
}

/// Headline figure with an inline sparkline (mockup's 2×2 stats grid).
fn stat_card(label: &str, value: &str, unit: &str, series: &[i64], spark_class: &str) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    card.add_css_class("kalam-lib-stat-card");
    card.set_hexpand(true);
    card.set_halign(gtk::Align::Fill);

    let l = gtk::Label::new(Some(label));
    l.add_css_class("kalam-lib-stat-label");
    l.set_halign(gtk::Align::Start);
    card.append(&l);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let v = gtk::Label::new(Some(value));
    v.add_css_class("kalam-lib-stat-val");
    row.append(&v);
    let u = gtk::Label::new(Some(unit));
    u.add_css_class("kalam-lib-stat-unit");
    u.set_valign(gtk::Align::End);
    u.set_margin_bottom(2);
    row.append(&u);
    card.append(&row);

    let spark = sparkline(series, spark_class);
    spark.set_vexpand(true);
    card.append(&spark);
    card
}

/// The yearly goal card: finished-this-year over the configured goal, with
/// a gold progress bar.
fn goal_card(snap: &DashboardSnapshot) -> gtk::Box {
    let goal = snap.goal;
    let done = snap.finished_this_year;
    let year = crate::db::iso_days_ago(0)
        .get(..4)
        .unwrap_or("")
        .to_string();

    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    card.add_css_class("kalam-lib-stat-card");
    card.set_hexpand(true);
    card.set_halign(gtk::Align::Fill);

    let l = gtk::Label::new(Some(&format!("{year} goal")));
    l.add_css_class("kalam-lib-stat-label");
    l.set_halign(gtk::Align::Start);
    card.append(&l);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let v = gtk::Label::new(Some(&done.to_string()));
    v.add_css_class("kalam-lib-stat-val");
    row.append(&v);
    let u = gtk::Label::new(Some(&format!("/ {goal}")));
    u.add_css_class("kalam-lib-stat-unit");
    u.set_valign(gtk::Align::End);
    u.set_margin_bottom(2);
    row.append(&u);
    card.append(&row);

    let bar = gtk::ProgressBar::new();
    bar.add_css_class("kalam-goal-bar");
    let frac = if goal > 0 {
        (done as f64 / goal as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };
    bar.set_fraction(frac);
    bar.set_vexpand(true);
    bar.set_valign(gtk::Align::End);
    card.append(&bar);
    card
}

// ---------------------------------------------------------------------------
// Continue reading
// ---------------------------------------------------------------------------

/// Horizontal, scrollable row of book cards. Hovering a card reveals a
/// circular play button (resume in the reader); clicking the card opens the
/// book float.
fn continue_strip(
    books: &[Book],
    sender: &ComponentSender<LibraryPageModel>,
) -> gtk::ScrolledWindow {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.set_halign(gtk::Align::Start);
    for book in books {
        row.append(&continue_card(book, sender));
    }

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::External)
        .vscrollbar_policy(gtk::PolicyType::Never)
        .child(&row)
        .build();
    scroll.add_css_class("kalam-lib-scroll");
    scroll.set_hexpand(true);
    scroll
}

fn continue_card(book: &Book, sender: &ComponentSender<LibraryPageModel>) -> gtk::Box {
    let cell = gtk::Box::new(gtk::Orientation::Vertical, 6);
    cell.add_css_class("kalam-lib-card");
    cell.set_size_request(120, -1);

    let overlay = gtk::Overlay::new();
    overlay.set_size_request(120, 170);
    let cover = cover_widget(book.cover_path.as_deref(), 120, 170);
    // Card-click gesture lives on the cover itself: the play button sits
    // above the cover in the overlay, so its clicks never reach it.
    let id = book.id;
    let s = sender.clone();
    let click = gtk::GestureClick::new();
    click.set_button(1);
    click.connect_released(move |_, _, _, _| {
        s.output(LibraryOut::BookDialog { book_id: id }).ok();
    });
    cover.add_controller(click);
    cover.set_cursor_from_name(Some("pointer"));
    overlay.set_child(Some(&cover));

    let play = gtk::Button::new();
    play.add_css_class("kalam-lib-play");
    play.set_focus_on_click(false);
    play.set_halign(gtk::Align::End);
    play.set_valign(gtk::Align::End);
    play.set_margin_bottom(8);
    play.set_margin_end(8);
    play.set_tooltip_text(Some("Resume reading"));
    play.set_child(Some(&crate::icons::symbolic(
        "media-playback-start-symbolic",
        14,
    )));
    let id_play = book.id;
    let s_play = sender.clone();
    play.connect_clicked(move |_| {
        s_play.output(LibraryOut::Read { book_id: id_play }).ok();
    });

    overlay.add_overlay(&play);
    cell.append(&overlay);

    let bar = gtk::ProgressBar::new();
    bar.add_css_class("kalam-mini-progress");
    bar.set_fraction((book.progress as f64 / 100.0).clamp(0.0, 1.0));
    cell.append(&bar);

    let title = gtk::Label::new(Some(&book.title));
    title.add_css_class("kalam-lib-title");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_halign(gtk::Align::Start);
    cell.append(&title);

    let author = gtk::Label::new(Some(book.authors_display()));
    author.add_css_class("kalam-lib-author");
    author.set_ellipsize(gtk::pango::EllipsizeMode::End);
    author.set_halign(gtk::Align::Start);
    cell.append(&author);

    let pct = gtk::Label::new(Some(&format!("{}%", book.progress)));
    pct.add_css_class("kalam-lib-pct");
    pct.set_halign(gtk::Align::Start);
    cell.append(&pct);

    cell
}

// ---------------------------------------------------------------------------
// Saved quotes
// ---------------------------------------------------------------------------

fn quote_card(anno: &crate::db::Annotation, qref: &crate::db::QuoteRef) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    card.add_css_class("kalam-q-card");

    let cover = cover_widget(qref.cover_path.as_deref(), 48, 68);
    cover.add_css_class("kalam-q-cover");
    card.append(&cover);

    let body_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    body_box.set_hexpand(true);
    body_box.set_valign(gtk::Align::Center);

    let b = gtk::Label::new(Some(&qref.title));
    b.add_css_class("kalam-q-book");
    b.set_halign(gtk::Align::Start);
    b.set_ellipsize(gtk::pango::EllipsizeMode::End);
    body_box.append(&b);

    let text = anno.text_excerpt.trim();
    let shown = if text.chars().count() > 180 {
        let cut: String = text.chars().take(180).collect();
        format!("{cut}…")
    } else {
        text.to_string()
    };
    let q = gtk::Label::new(Some(&shown));
    q.add_css_class("kalam-q-text");
    q.set_wrap(true);
    q.set_xalign(0.0);
    q.set_halign(gtk::Align::Start);
    body_box.append(&q);

    let first_author = qref.author.split(',').next().unwrap_or("").trim();
    let meta = gtk::Label::new(Some(&format!(
        "{} · {}",
        first_author,
        relative_days(anno.created_at.get(..10).unwrap_or("")),
    )));
    meta.add_css_class("kalam-q-meta");
    meta.set_halign(gtk::Align::Start);
    body_box.append(&meta);

    card.append(&body_box);
    card
}

/// "today" / "yesterday" / "2 days ago" / "Aug 27" — quote meta lines.
fn relative_days(day: &str) -> String {
    if day.len() < 10 {
        return day.to_string();
    }
    if crate::db::iso_days_ago(0).starts_with(day) {
        return "today".into();
    }
    if crate::db::iso_days_ago(1).starts_with(day) {
        return "yesterday".into();
    }
    for d in 2..=90 {
        if crate::db::iso_days_ago(d).starts_with(day) {
            return format!("{d} days ago");
        }
    }
    pretty_day(day).to_string()
}

// ---------------------------------------------------------------------------
// History feed (events + sessions)
// ---------------------------------------------------------------------------

struct FeedItem {
    at: String,
    title: String,
    sub: String,
    icon: &'static str,
    /// Badge background tint class.
    tint: &'static str,
    /// Icon colour class (the pre-existing `kalam-event-*` set).
    icon_tint: &'static str,
    book_id: i64,
}

/// Newest-first mix of the event log and reading sessions, capped at
/// [`FEED_LIMIT`]. Both inputs come from the snapshot; `catalog` remains only
/// for the per-event detail lookups (progress, format).
fn history_feed(snap: &DashboardSnapshot, catalog: &Arc<Catalog>) -> Vec<FeedItem> {
    let mut items: Vec<FeedItem> = Vec::new();

    for e in &snap.events {
        let sub = match e.kind {
            // Opened events carry no detail — show where the book
            // currently sits instead.
            EventKind::Opened => catalog
                .get_reading_progress(e.book_id)
                .ok()
                .flatten()
                .map(|(_, frac)| format!("Resumed at {}%", (frac * 100.0).round() as i64))
                .unwrap_or_default(),
            EventKind::Finished => {
                if e.detail == "auto" {
                    format!("{} · auto-finished", e.book_authors)
                } else {
                    e.book_authors.clone()
                }
            }
            EventKind::Unfinished => e.book_authors.clone(),
            EventKind::Imported => {
                let format_label = catalog
                    .get_book(e.book_id)
                    .ok()
                    .flatten()
                    .map(|b| b.format.as_str().to_string())
                    .unwrap_or_default();
                if format_label.is_empty() {
                    e.book_authors.clone()
                } else {
                    format!("{} · {}", e.book_authors, format_label)
                }
            }
        };
        items.push(FeedItem {
            at: e.at.clone(),
            title: format!("{} {}", e.kind.label(), e.book_title),
            sub,
            icon: e.kind.icon(),
            tint: match e.kind {
                EventKind::Finished => "kalam-hist-tint-success",
                EventKind::Imported => "kalam-hist-tint-warning",
                _ => "kalam-hist-tint-accent",
            },
            icon_tint: match e.kind {
                EventKind::Finished => "kalam-event-finished",
                EventKind::Imported => "kalam-event-imported",
                _ => "kalam-event-opened",
            },
            book_id: e.book_id,
        });
    }

    for s in &snap.sessions {
        if s.seconds < 30 {
            continue; // ignore flip-in-and-out sessions
        }
        let mins = (s.seconds / 60).max(1);
        let sub = if (1..100).contains(&s.end_pct) {
            format!("{mins} min session · reached {}%", s.end_pct)
        } else {
            format!("{mins} min session")
        };
        items.push(FeedItem {
            at: s.started_at.clone(),
            title: format!("Read {}", s.book_title),
            sub,
            icon: "media-playback-start-symbolic",
            tint: "kalam-hist-tint-accent",
            icon_tint: "kalam-event-opened",
            book_id: s.book_id,
        });
    }

    // ISO-8601 UTC strings compare chronologically.
    items.sort_by(|a, b| b.at.cmp(&a.at));
    items.truncate(FEED_LIMIT);
    items
}

fn history_row(item: &FeedItem, sender: &ComponentSender<LibraryPageModel>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("kalam-hist-row");

    let badge = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    badge.add_css_class("kalam-hist-badge");
    badge.add_css_class(item.tint);
    let icon =
        crate::icons::symbolic_with_classes(item.icon, 17, &["kalam-event-icon", item.icon_tint]);
    icon.set_halign(gtk::Align::Center);
    icon.set_valign(gtk::Align::Center);
    badge.append(&icon);
    row.append(&badge);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    let title = gtk::Label::new(Some(&item.title));
    title.add_css_class("kalam-hist-title");
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&title);
    if !item.sub.is_empty() {
        let sub = gtk::Label::new(Some(&item.sub));
        sub.add_css_class("kalam-hist-sub");
        sub.set_halign(gtk::Align::Start);
        sub.set_xalign(0.0);
        sub.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&sub);
    }
    row.append(&text);

    let time = gtk::Label::new(Some(&feed_time(&item.at)));
    time.add_css_class("kalam-hist-time");
    time.set_valign(gtk::Align::Center);
    row.append(&time);

    let book_id = item.book_id;
    let s = sender.clone();
    let click = gtk::GestureClick::new();
    click.set_button(1);
    click.connect_released(move |_, _, _, _| {
        s.output(LibraryOut::Book { book_id }).ok();
    });
    row.add_controller(click);
    row.set_cursor_from_name(Some("pointer"));
    row
}

/// "Today · 18:59" or "Aug 27 · 21:30".
fn feed_time(at: &str) -> String {
    let day = at.get(..10).unwrap_or("");
    let clock = at.get(11..16).unwrap_or("");
    if clock.is_empty() {
        return pretty_day(day).to_string();
    }
    format!("{} · {}", pretty_day(day), clock)
}

// ---------------------------------------------------------------------------
// Section header
// ---------------------------------------------------------------------------

/// Section with a clickable header that routes to the full page, showing a
/// "Show all →" affordance on the right (whole header is the affordance).
/// Quick links to the library pages that this dashboard does not give a
/// section of its own.
///
/// These were unreachable. `AllBooks` was linked only from the empty-library
/// placeholder below, inside its `return` branch — so the one moment you could
/// open the full grid from here was while you owned no books, and importing
/// your first book made the link vanish. `ReadingList`, `Tags` and `Analytics`
/// have complete pages wired into the router in `app.rs`, but nothing in the
/// UI ever pushed those routes, so they could not be opened at all.
///
/// Content sections (history, quotes, vocabulary) keep their own clickable
/// headers; this row deliberately does not duplicate them.
fn quick_links(sender: &ComponentSender<LibraryPageModel>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_halign(gtk::Align::Start);

    let targets = [
        (
            "All books",
            LibrarySection::AllBooks,
            "Browse, search and sort every book in your library",
        ),
        (
            "Reading list",
            LibrarySection::ReadingList,
            "Your ordered to-read queue",
        ),
        (
            "Tags",
            LibrarySection::Tags,
            "Browse your tags and the books under each one",
        ),
        (
            "Analytics",
            LibrarySection::Analytics,
            "Reading stats, charts and streaks",
        ),
        (
            "Tasks",
            LibrarySection::TaskManager,
            "What is running in the background, and how to stop it",
        ),
        (
            "Review",
            LibrarySection::Review,
            "Spaced-repetition review of your saved words",
        ),
    ];

    for (label, target, tip) in targets {
        let btn = gtk::Button::with_label(label);
        btn.add_css_class("kalam-secondary-btn");
        btn.set_tooltip_text(Some(tip));
        let s = sender.clone();
        btn.connect_clicked(move |_| {
            s.output(LibraryOut::Section(target)).ok();
        });
        row.append(&btn);
    }

    row
}

fn section(
    title: &str,
    target: LibrarySection,
    sender: &ComponentSender<LibraryPageModel>,
    content: gtk::Widget,
) -> gtk::Box {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 10);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.add_css_class("kalam-section-header");

    let label = gtk::Label::new(Some(title));
    label.add_css_class("kalam-section-label");
    label.set_halign(gtk::Align::Start);
    header.append(&label);

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    header.append(&spacer);

    let show_all = gtk::Label::new(Some("Show all \u{2192}"));
    show_all.add_css_class("kalam-show-all");
    header.append(&show_all);

    let click = gtk::GestureClick::new();
    click.set_button(1);
    let s = sender.clone();
    click.connect_released(move |_, _, _, _| {
        s.output(LibraryOut::Section(target)).ok();
    });
    header.add_controller(click);
    header.set_cursor_from_name(Some("pointer"));
    header.set_tooltip_text(Some(&format!("Open {} page", title.to_lowercase())));

    wrap.append(&header);
    wrap.append(&content);
    wrap
}
