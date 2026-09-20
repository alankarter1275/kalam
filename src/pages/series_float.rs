//! Floating series panel — the full listing for a book's series.
//!
//! Opened from the book page's Series row (only books in a series show it).
//! The listing comes from Open Library, fetched once per series and cached in
//! `series_cache`; covers land in `series-covers/`. The ⟳ button is the only
//! way to re-fetch — nothing refreshes automatically.
//!
//! Works you own are matched by title and get a live status badge; clicking
//! one opens its book page. Works you don't own are display-only.

use crate::db::{series_key, Catalog, SeriesWork};
use crate::models::Book;
use crate::service::LibraryService;
use crate::widgets::book_row::cover_widget;
use gtk::prelude::*;
use relm4::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug)]
pub enum SeriesFloatOut {
    OpenBook { book_id: i64 },
}

#[derive(Debug)]
pub enum SeriesFloatMsg {
    /// Manual re-fetch (the ⟳ button, or the retry button after an error).
    Refresh,
    /// A background fetch finished. `works` is `Some` on success — possibly
    /// empty, which renders a "no listing" state without caching the absence.
    Fetched {
        works: Option<Vec<SeriesWork>>,
        error: Option<String>,
    },
}

/// One rendered row: a remote work, possibly matched to a local book.
#[derive(Debug, Clone)]
struct SeriesRow {
    title: String,
    cover: Option<PathBuf>,
    year: i64,
    local: Option<Book>,
}

#[derive(Debug, Clone)]
enum SeriesState {
    Loading,
    Ready {
        rows: Vec<SeriesRow>,
        fetched_at: String,
    },
    Error {
        detail: String,
    },
}

pub struct SeriesFloatModel {
    catalog: Arc<Catalog>,
    #[allow(dead_code)] // the float queries the service for the series grid
    service: LibraryService,
    series_name: String,
    series_key: String,
    state: SeriesState,
    fetching: bool,
}

/// Result of the worker thread, posted back to the main context.
enum FetchResult {
    Ok(Vec<SeriesWork>),
    Err(String),
}

#[relm4::component(pub)]
impl Component for SeriesFloatModel {
    type Init = (Arc<Catalog>, String, String);
    type Input = SeriesFloatMsg;
    type Output = SeriesFloatOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            add_css_class: "kalam-series-float",
            set_overflow: gtk::Overflow::Hidden,
            set_hexpand: true,
            set_vexpand: true,

            // Header: title and refresh. Dismissal is backdrop-click or Esc.
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 10,
                set_margin_top: 16,
                set_margin_start: 16,
                set_margin_end: 12,

                #[name = "series_title"]
                gtk::Label {
                    add_css_class: "kalam-series-float-title",
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                },

                #[name = "refresh_btn"]
                gtk::Button {
                    add_css_class: "kalam-icon-btn",
                    set_focus_on_click: false,
                    set_tooltip_text: Some("Refresh from Open Library"),
                    set_child: Some(&crate::icons::symbolic("view-refresh-symbolic", 16)),
                    connect_clicked => SeriesFloatMsg::Refresh,
                },

                // No close button by user request (2026-09-03): the backdrop
                // and Esc both dismiss this, and it is a read-only listing, so
                // a misclick outside costs nothing.
            },

            // Swapped between loading / list / error.
            #[name = "host"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                add_css_class: "kalam-series-host",
                set_hexpand: true,
                set_vexpand: true,
            },

            #[name = "footer"]
            gtk::Label {
                add_css_class: "kalam-series-footer",
                set_halign: gtk::Align::Start,
                set_margin_top: 8,
                set_margin_start: 16,
                set_margin_end: 16,
                set_margin_bottom: 10,
            },
        }
    }

    fn init(
        (catalog, series_name, first_author): Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let key = series_key(&series_name, &first_author);
        let service = LibraryService::new(catalog.clone());
        let mut model = SeriesFloatModel {
            catalog,
            service,
            series_name: series_name.clone(),
            series_key: key,
            state: SeriesState::Loading,
            fetching: true,
        };
        let widgets = view_output!();
        widgets.series_title.set_label(&model.series_name);

        // A cache hit renders immediately; a miss goes to Open Library.
        match model.catalog.get_cached_series(&model.series_key) {
            Ok(Some(entry)) => {
                let rows = merge_rows(&model.catalog, &model.series_name, &entry.works);
                model.fetching = false;
                model.state = SeriesState::Ready {
                    rows,
                    fetched_at: entry.fetched_at,
                };
            }
            Ok(None) => start_fetch(
                &model.catalog,
                &model.series_name,
                &model.series_key,
                sender.clone(),
            ),
            Err(err) => {
                let rows = local_series_rows(&model.catalog, &model.series_name);
                model.fetching = false;
                if !rows.is_empty() {
                    model.state = SeriesState::Ready {
                        rows,
                        fetched_at: "Local library (offline)".into(),
                    };
                } else {
                    model.state = SeriesState::Error {
                        detail: err.to_string(),
                    };
                }
            }
        }
        render(&model, &widgets, &sender);
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
            SeriesFloatMsg::Refresh => {
                if self.fetching {
                    return;
                }
                self.fetching = true;
                self.state = SeriesState::Loading;
                start_fetch(
                    &self.catalog,
                    &self.series_name,
                    &self.series_key,
                    sender.clone(),
                );
                render(self, widgets, &sender);
            }
            SeriesFloatMsg::Fetched { works, error } => {
                self.fetching = false;
                match works {
                    Some(works) => {
                        let fetched_at = crate::db::chrono_like_now();
                        if works.is_empty() {
                            let rows = local_series_rows(&self.catalog, &self.series_name);
                            if !rows.is_empty() {
                                self.state = SeriesState::Ready {
                                    rows,
                                    fetched_at: "Local library".into(),
                                };
                            } else {
                                self.state = SeriesState::Ready {
                                    rows: Vec::new(),
                                    fetched_at,
                                };
                            }
                        } else {
                            self.state = SeriesState::Ready {
                                rows: merge_rows(&self.catalog, &self.series_name, &works),
                                fetched_at,
                            };
                        }
                    }
                    None => {
                        let rows = local_series_rows(&self.catalog, &self.series_name);
                        if !rows.is_empty() {
                            self.state = SeriesState::Ready {
                                rows,
                                fetched_at: "Local library (offline)".into(),
                            };
                        } else {
                            self.state = SeriesState::Error {
                                detail: error.unwrap_or_else(|| "Unknown error".into()),
                            };
                        }
                    }
                }
                render(self, widgets, &sender);
            }
        }
    }
}

/// Run the fetch on a worker thread and post the result back to GTK.
///
/// Takes the sender by value: the delivery closure must be `'static`, so it
/// keeps its own clone.
///
/// A0 step 4: this used to hand-roll the worker, the `async_channel` and the
/// local future — the exact three-part dance `tasks::spawn` now owns. Worth
/// noting what the old version got wrong, because the seam makes it
/// unrepresentable: `send()` on an `async_channel::Sender` returns a *future*,
/// and nothing polls it on a plain worker thread, so `let _ = tx.send(..)`
/// dropped every result on the floor and the receiver only woke when the
/// sender dropped — surfacing successful fetches as "Fetch worker ended
/// unexpectedly". `Reporter` and the result channel use `send_blocking`
/// internally, so a caller cannot make that mistake here again.
fn start_fetch(
    catalog: &Arc<Catalog>,
    series_name: &str,
    series_key: &str,
    sender: ComponentSender<SeriesFloatModel>,
) {
    let name = series_name.to_string();
    let key = series_key.to_string();
    let cat = catalog.clone();

    crate::tasks::spawn(
        "Fetching series covers",
        move |_reporter| fetch_and_cache(&cat, &name, &key),
        |_update| {},
        move |result| {
            let msg = match result {
                FetchResult::Ok(works) => SeriesFloatMsg::Fetched {
                    works: Some(works),
                    error: None,
                },
                FetchResult::Err(detail) => SeriesFloatMsg::Fetched {
                    works: None,
                    error: Some(detail),
                },
            };
            sender.input(msg);
        },
    );
}

/// Search, download covers, and cache the listing. Runs off the main thread.
fn fetch_and_cache(catalog: &Arc<Catalog>, series_name: &str, series_key: &str) -> FetchResult {
    let remote = match crate::metadata::series::search_series(series_name) {
        Ok(works) => works,
        Err(err) => return FetchResult::Err(err.to_string()),
    };

    let works: Vec<SeriesWork> = remote
        .into_iter()
        .map(|w| {
            // A failed cover download must not kill the whole listing.
            let cover_path = w
                .cover_i
                .map(crate::metadata::series::download_cover)
                .transpose()
                .ok()
                .flatten()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default();
            SeriesWork {
                title: w.title,
                key: w.key,
                cover_path,
                year: w.year,
                author: w.author,
            }
        })
        .collect();

    // Only a real listing is worth caching.
    if !works.is_empty() {
        if let Err(err) = catalog.upsert_series_cache(series_key, "openlibrary", &works) {
            return FetchResult::Err(format!("Could not cache the listing: {err}"));
        }
    }
    FetchResult::Ok(works)
}

/// Join remote works with the books you own in this series.
///
/// Matching is by normalised title; an OL work and a local book that never
/// appear in OL's listing still sit side by side (local extras append at the
/// end), so the card is always a superset of the truth you can verify.
fn merge_rows(catalog: &Arc<Catalog>, series_name: &str, remote: &[SeriesWork]) -> Vec<SeriesRow> {
    let mut rows: Vec<SeriesRow> = remote
        .iter()
        .map(|w| SeriesRow {
            title: w.title.clone(),
            cover: (!w.cover_path.is_empty()).then(|| PathBuf::from(&w.cover_path)),
            year: w.year,
            local: None,
        })
        .collect();

    // A failed read here would silently hide books you actually own, making
    // the panel claim the series is entirely unowned.
    let owned = match catalog.books_in_series(series_name) {
        Ok(books) => books,
        Err(err) => {
            crate::notify::error(
                "Could not check your copies of this series",
                &err.to_string(),
            );
            Vec::new()
        }
    };
    let mut extras: Vec<SeriesRow> = Vec::new();
    for book in owned {
        let key = normalise(&book.title);
        let slot = rows
            .iter()
            .position(|r| r.local.is_none() && normalise(&r.title) == key);
        match slot {
            Some(i) => rows[i].local = Some(book),
            None => extras.push(SeriesRow {
                title: book.title.clone(),
                cover: book.cover_path.clone(),
                year: 0,
                local: Some(book),
            }),
        }
    }
    rows.extend(extras);
    rows
}

fn normalise(s: &str) -> String {
    s.to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// Rebuild the host + footer for the current state.
fn render(
    model: &SeriesFloatModel,
    widgets: &SeriesFloatModelWidgets,
    sender: &ComponentSender<SeriesFloatModel>,
) {
    let host = &widgets.host;
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }

    match &model.state {
        SeriesState::Loading => {
            let wrap = gtk::Box::new(gtk::Orientation::Vertical, 12);
            wrap.set_valign(gtk::Align::Center);
            wrap.set_halign(gtk::Align::Center);
            wrap.set_vexpand(true);
            wrap.set_hexpand(true);

            let spinner = gtk::Spinner::new();
            spinner.set_spinning(true);
            spinner.add_css_class("kalam-series-spinner");
            wrap.append(&spinner);

            let label = gtk::Label::new(Some("Fetching the series from Open Library…"));
            label.add_css_class("kalam-muted");
            wrap.append(&label);

            host.append(&wrap);
            widgets.footer.set_visible(false);
            widgets.refresh_btn.set_sensitive(false);
        }
        SeriesState::Error { detail } => {
            let wrap = gtk::Box::new(gtk::Orientation::Vertical, 12);
            wrap.set_valign(gtk::Align::Center);
            wrap.set_halign(gtk::Align::Center);
            wrap.set_vexpand(true);
            wrap.set_hexpand(true);

            let label = gtk::Label::new(Some(&format!("Couldn't fetch the series.\n\n{detail}")));
            label.add_css_class("kalam-series-error");
            label.set_wrap(true);
            label.set_justify(gtk::Justification::Center);
            wrap.append(&label);

            let retry = gtk::Button::with_label("Try again");
            retry.add_css_class("kalam-secondary-btn");
            retry.set_halign(gtk::Align::Center);
            let retry_sender = sender.clone();
            retry.connect_clicked(move |_| {
                retry_sender.input(SeriesFloatMsg::Refresh);
            });
            wrap.append(&retry);

            host.append(&wrap);
            widgets.footer.set_visible(false);
            widgets.refresh_btn.set_sensitive(false);
        }
        SeriesState::Ready { rows, fetched_at } => {
            widgets.refresh_btn.set_sensitive(true);

            if rows.is_empty() {
                let label = gtk::Label::new(Some(
                    "Open Library doesn't list this series yet — small or \
                     self-published series often aren't there.",
                ));
                label.add_css_class("kalam-series-error");
                label.set_wrap(true);
                label.set_justify(gtk::Justification::Center);
                let wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);
                wrap.set_valign(gtk::Align::Center);
                wrap.set_halign(gtk::Align::Center);
                wrap.set_vexpand(true);
                wrap.set_hexpand(true);
                wrap.append(&label);
                host.append(&wrap);
                widgets.footer.set_visible(false);
                return;
            }

            let scroll = gtk::ScrolledWindow::builder()
                .hscrollbar_policy(gtk::PolicyType::Never)
                .vscrollbar_policy(gtk::PolicyType::Automatic)
                .overlay_scrolling(true)
                .vexpand(true)
                .build();
            scroll.set_margin_start(8);
            scroll.set_margin_end(8);

            let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
            list.add_css_class("kalam-series-list");
            // One query for the whole list instead of `book_finished_at` per
            // row. A failed read leaves the set empty, which reads as "not
            // finished" — the same thing the per-row `.ok()` used to do, but
            // now it costs one query instead of N.
            let owned_ids: Vec<i64> = rows
                .iter()
                .filter_map(|r| r.local.as_ref().map(|b| b.id))
                .collect();
            let finished_ids = model
                .catalog
                .finished_book_ids(&owned_ids)
                .unwrap_or_default();
            for (pos, row) in rows.iter().enumerate() {
                list.append(&build_series_row(row, pos, &finished_ids, sender));
            }
            scroll.set_child(Some(&list));
            host.append(&scroll);

            // Honest footer: where this came from, and when.
            widgets.footer.set_visible(true);
            if fetched_at.starts_with("Local library") {
                widgets.footer.set_label(fetched_at);
            } else {
                widgets.footer.set_label(&format!(
                    "Open Library · fetched {}",
                    pretty_fetched(fetched_at)
                ));
            }
        }
    }
}

fn local_series_rows(catalog: &Arc<Catalog>, series_name: &str) -> Vec<SeriesRow> {
    let local_books = catalog.detect_local_series(series_name).unwrap_or_default();
    local_books
        .into_iter()
        .map(|b| SeriesRow {
            title: b.title.clone(),
            cover: b.cover_path.clone(),
            year: 0,
            local: Some(b),
        })
        .collect()
}

fn build_series_row(
    row: &SeriesRow,
    pos: usize,
    finished_ids: &std::collections::HashSet<i64>,
    sender: &ComponentSender<SeriesFloatModel>,
) -> gtk::Box {
    let outer = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    outer.add_css_class("kalam-series-row");
    outer.set_margin_start(8);
    outer.set_margin_end(8);

    let cover = cover_widget(row.cover.as_deref(), 34, 50);
    cover.add_css_class("kalam-series-mini");
    outer.append(&cover);

    let info = gtk::Box::new(gtk::Orientation::Vertical, 1);
    info.set_hexpand(true);
    info.set_halign(gtk::Align::Start);
    info.set_valign(gtk::Align::Center);

    let num = if row.year > 0 {
        format!("Book {} · {}", pos + 1, row.year)
    } else {
        format!("Book {}", pos + 1)
    };
    let num = gtk::Label::new(Some(&num));
    num.add_css_class("kalam-series-num");
    num.set_halign(gtk::Align::Start);
    info.append(&num);

    let name = gtk::Label::new(Some(&row.title));
    name.add_css_class("kalam-series-name");
    name.set_halign(gtk::Align::Start);
    name.set_hexpand(true);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    info.append(&name);
    outer.append(&info);

    // Status badge: live for books you own, muted for the rest.
    let (text, class): (String, &str) = match &row.local {
        Some(book) => {
            let finished = finished_ids.contains(&book.id);
            if finished || book.progress >= 100 {
                ("Read".into(), "kalam-badge-series-read")
            } else if book.progress > 0 {
                (
                    format!("{}% read", book.progress),
                    "kalam-badge-series-reading",
                )
            } else {
                ("Not started".into(), "kalam-badge-series-unread")
            }
        }
        None => ("Not in library".into(), "kalam-badge-series-unowned"),
    };
    let badge = gtk::Label::new(Some(&text));
    badge.add_css_class(class);
    badge.set_valign(gtk::Align::Center);
    outer.append(&badge);

    if let Some(book) = &row.local {
        let id = book.id;
        let sender = sender.clone();
        outer.set_cursor_from_name(Some("pointer"));
        outer.set_tooltip_text(Some("Open book page"));
        let click = gtk::GestureClick::new();
        click.set_button(1);
        click.connect_released(move |_, _, _, _| {
            sender.output(SeriesFloatOut::OpenBook { book_id: id }).ok();
        });
        outer.add_controller(click);
    }
    outer
}

/// `2026-08-29T14:22:00Z` → `29 Aug 2026` (today/yesterday get friendly names).
fn pretty_fetched(iso: &str) -> String {
    if iso.len() >= 10 {
        crate::pages::history::pretty_day(&iso[..10])
    } else {
        iso.to_string()
    }
}
