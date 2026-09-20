use crate::author::{
    self, display_author_name, initials, status_counts, works_not_in_library, SeriesProgress,
};
use crate::db::{AuthorProfile, AuthorWork, Catalog};
use crate::models::Book;
use crate::service::LibraryService;
use crate::widgets::{
    book_row::{build_book_card, cover_widget},
    charts::stars_label,
};
use gtk::prelude::*;
use relm4::prelude::*;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug)]
pub enum AuthorPageOut {
    OpenBook { book_id: i64 },
    OpenBookDialog { book_id: i64 },
}

#[derive(Debug)]
pub enum AuthorPageMsg {
    Refresh,
    // Boxed: AuthorProfile is ~376 bytes, so an unboxed variant made every
    // AuthorPageMsg that large -- including the far more frequent Refresh.
    Fetched(Box<Result<AuthorProfile, String>>),
}

pub struct AuthorPageModel {
    catalog: Arc<Catalog>,
    #[allow(dead_code)] // queried once the author saved-quotes section lands
    service: LibraryService,
    requested_name: String,
    profile: Option<AuthorProfile>,
    owned_books: Vec<Book>,
    series: Vec<SeriesProgress>,
    loading: bool,
    error: Option<String>,
}

#[derive(Debug, Clone)]
struct WorkGroup {
    title: String,
    work_count: usize,
    first_year: Option<i64>,
    last_year: Option<i64>,
    cover_path: Option<PathBuf>,
    rating_average: Option<f32>,
    rating_count: i64,
    is_series: bool,
}

#[derive(Default)]
struct WorkGroupBuilder {
    title: String,
    work_count: usize,
    years: Vec<i64>,
    cover_path: Option<PathBuf>,
    rating_weight: f64,
    rating_count: i64,
    is_series: bool,
}

#[derive(Default)]
struct OnlineSeriesSummary {
    known_count: usize,
    first_year: Option<i64>,
    last_year: Option<i64>,
}

#[relm4::component(pub)]
impl Component for AuthorPageModel {
    type Init = (Arc<Catalog>, String);
    type Input = AuthorPageMsg;
    type Output = AuthorPageOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 22,
            set_hexpand: true,
            set_vexpand: false,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 24,
                set_hexpand: true,

                #[name = "hero_main"]
                gtk::Box {
                    add_css_class: "kalam-author-hero",
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,
                    set_hexpand: true,
                },

                #[name = "hero_side"]
                gtk::Box {
                    add_css_class: "kalam-author-side",
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 14,
                    set_hexpand: false,
                    set_halign: gtk::Align::End,
                },
            },

            gtk::Label {
                set_label: "SERIES TRACKER",
                add_css_class: "kalam-section-label",
                set_halign: gtk::Align::Start,
            },

            #[name = "series_host"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
            },

            gtk::Label {
                set_label: "YOUR BOOKS",
                add_css_class: "kalam-section-label",
                set_halign: gtk::Align::Start,
            },

            #[name = "owned_host"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
            },

            gtk::Label {
                set_label: "MORE BY THIS AUTHOR",
                add_css_class: "kalam-section-label",
                set_halign: gtk::Align::Start,
            },

            #[name = "works_host"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,
                set_margin_bottom: 12,
            },
        }
    }

    fn init(
        (catalog, author_name): Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let owned_books = author::owned_books_for_author(&catalog, &author_name);
        let profile = catalog
            .get_author_profile_by_name(&author_name)
            .ok()
            .flatten();
        let series = author::series_progress(&owned_books);
        let loading = profile.is_none();

        let service = LibraryService::new(catalog.clone());
        let model = AuthorPageModel {
            catalog,
            service,
            requested_name: author_name,
            profile,
            owned_books,
            series,
            loading,
            error: None,
        };
        let mut widgets = view_output!();
        fill_author_page(&mut widgets, &model, &sender);
        if model.profile.is_none() {
            spawn_author_fetch(
                model.catalog.clone(),
                model.requested_name.clone(),
                model.owned_books.clone(),
                &sender,
            );
        }
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
            AuthorPageMsg::Refresh => {
                self.loading = true;
                self.error = None;
                spawn_author_fetch(
                    self.catalog.clone(),
                    self.requested_name.clone(),
                    self.owned_books.clone(),
                    &sender,
                );
            }
            AuthorPageMsg::Fetched(result) => {
                self.loading = false;
                match *result {
                    Ok(profile) => {
                        self.profile = Some(profile);
                        self.error = None;
                    }
                    Err(err) => {
                        self.error = Some(err);
                    }
                }
                self.owned_books =
                    author::owned_books_for_author(&self.catalog, &self.requested_name);
                self.series = author::series_progress(&self.owned_books);
            }
        }

        fill_author_page(widgets, self, &sender);
        self.update_view(widgets, sender);
    }
}

fn spawn_author_fetch(
    catalog: Arc<Catalog>,
    author_name: String,
    owned_books: Vec<Book>,
    sender: &ComponentSender<AuthorPageModel>,
) {
    // A0 step 4. The old version posted straight into the input sender from
    // the worker, which happens to be safe (relm4 senders are `Send`) but left
    // the page with no way to be told to stop. Going through the seam means
    // this fetch is registered and gets cancelled with everything else when
    // the window closes.
    let tx = sender.input_sender().clone();
    crate::tasks::spawn(
        "Fetching author photo",
        move |_reporter| author::fetch_and_cache_author(&catalog, &author_name, &owned_books),
        |_update| {},
        move |result| {
            let _ = tx.send(AuthorPageMsg::Fetched(Box::new(result)));
        },
    );
}

fn fill_author_page(
    widgets: &mut AuthorPageModelWidgets,
    model: &AuthorPageModel,
    sender: &ComponentSender<AuthorPageModel>,
) {
    rebuild_hero(&widgets.hero_main, &widgets.hero_side, model, sender);
    rebuild_series(&widgets.series_host, model);
    rebuild_owned_books(&widgets.owned_host, &model.owned_books, sender);
    rebuild_works(&widgets.works_host, model);
}

fn rebuild_hero(
    main_host: &gtk::Box,
    side_host: &gtk::Box,
    model: &AuthorPageModel,
    sender: &ComponentSender<AuthorPageModel>,
) {
    clear_box(main_host);
    clear_box(side_host);

    let display_name = model
        .profile
        .as_ref()
        .map(|profile| profile.canonical_name.clone())
        .unwrap_or_else(|| display_author_name(&model.requested_name));

    let title = gtk::Label::new(Some(&display_name));
    title.add_css_class("kalam-author-name");
    title.set_halign(gtk::Align::Start);
    title.set_wrap(true);
    title.set_xalign(0.0);
    main_host.append(&title);

    let facts = gtk::Box::new(gtk::Orientation::Vertical, 8);
    facts.add_css_class("kalam-author-facts");
    for (label, value) in hero_fact_rows(model, &display_name) {
        facts.append(&fact_row(&label, &value));
    }
    main_host.append(&facts);

    let bio = model
        .profile
        .as_ref()
        .map(|profile| profile.bio.trim())
        .filter(|bio| !bio.is_empty())
        .unwrap_or("No bio has been loaded yet for this author.");
    let bio_label = gtk::Label::new(Some(bio));
    bio_label.add_css_class("kalam-author-bio");
    bio_label.set_halign(gtk::Align::Start);
    bio_label.set_wrap(true);
    bio_label.set_xalign(0.0);
    main_host.append(&bio_label);

    if let Some(err) = &model.error {
        let error = gtk::Label::new(Some(err));
        error.add_css_class("kalam-error-text");
        error.set_halign(gtk::Align::Start);
        error.set_wrap(true);
        error.set_xalign(0.0);
        main_host.append(&error);
    }

    side_host.append(&author_photo_overlay(model, sender));
    side_host.append(&author_stats(model));
}

fn rebuild_series(host: &gtk::Box, model: &AuthorPageModel) {
    clear_box(host);
    if model.series.is_empty() {
        host.append(&simple_note(
            "No series showed up from the books you already own by this author.",
        ));
        return;
    }

    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .column_spacing(14)
        .row_spacing(14)
        .halign(gtk::Align::Start)
        .build();

    for entry in &model.series {
        flow.insert(&series_card(entry, model.profile.as_ref()), -1);
    }

    host.append(&flow);
}

fn rebuild_owned_books(host: &gtk::Box, books: &[Book], sender: &ComponentSender<AuthorPageModel>) {
    clear_box(host);
    if books.is_empty() {
        host.append(&simple_note(
            "No books by this author are in your library yet.",
        ));
        return;
    }

    let mut books = books.to_vec();
    books.sort_by(|a, b| {
        a.series
            .as_deref()
            .unwrap_or("")
            .cmp(b.series.as_deref().unwrap_or(""))
            .then_with(|| {
                a.series_index
                    .partial_cmp(&b.series_index)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.title.cmp(&b.title))
    });

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 24);
    row.add_css_class("kalam-author-books-strip");
    row.set_halign(gtk::Align::Start);

    for book in &books {
        let book_id = book.id;
        let full_sender = sender.clone();
        let float_sender = sender.clone();
        row.append(&build_book_card(
            book,
            move || {
                full_sender.output(AuthorPageOut::OpenBook { book_id }).ok();
            },
            move || {
                float_sender
                    .output(AuthorPageOut::OpenBookDialog { book_id })
                    .ok();
            },
        ));
    }

    // These cards defer their cover decode, so nothing fills them without
    // this. (`build_book_grid` does it for pages that use the grid; this page
    // builds its strip by hand.)
    crate::preload::warm_books(
        &books,
        0,
        crate::widgets::book_row::COVER_W,
        crate::widgets::book_row::COVER_H,
    );

    let rail = gtk::ScrolledWindow::new();
    rail.add_css_class("kalam-author-books-rail");
    rail.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    rail.set_hexpand(true);
    rail.set_vexpand(false);
    rail.set_child(Some(&row));
    host.append(&rail);
}

fn rebuild_works(host: &gtk::Box, model: &AuthorPageModel) {
    clear_box(host);
    if model.profile.is_none() {
        let text = if model.loading {
            "Loading more books by this author…"
        } else {
            "No online author data has been loaded yet."
        };
        host.append(&simple_note(text));
        return;
    }

    let groups = more_work_groups(model);
    if groups.is_empty() {
        host.append(&simple_note(
            "No extra books are ready to show yet, or you already own the fetched ones.",
        ));
        return;
    }

    let flow = gtk::FlowBox::builder()
        .selection_mode(gtk::SelectionMode::None)
        .column_spacing(16)
        .row_spacing(16)
        .halign(gtk::Align::Start)
        .build();

    for group in groups {
        flow.insert(&work_group_card(&group), -1);
    }

    host.append(&flow);
}

fn hero_fact_rows(model: &AuthorPageModel, display_name: &str) -> Vec<(String, String)> {
    let mut rows = vec![("Name".to_string(), display_name.to_string())];

    if let Some(profile) = &model.profile {
        if !profile.birth_date.trim().is_empty() {
            rows.push((
                "Date of birth".to_string(),
                profile.birth_date.trim().to_string(),
            ));
        }
        if !profile.death_date.trim().is_empty() {
            rows.push((
                "Date of death".to_string(),
                profile.death_date.trim().to_string(),
            ));
        }
        if profile.work_count > 0 {
            rows.push(("Works found".to_string(), profile.work_count.to_string()));
        }
        let aliases = profile
            .aliases
            .iter()
            .filter(|name| normalize_key(name) != normalize_key(display_name))
            .take(3)
            .cloned()
            .collect::<Vec<_>>();
        if !aliases.is_empty() {
            rows.push(("Also written as".to_string(), aliases.join(" · ")));
        }
        if !profile.fetched_at.trim().is_empty() {
            rows.push((
                "Last updated".to_string(),
                profile
                    .fetched_at
                    .split('T')
                    .next()
                    .unwrap_or(profile.fetched_at.as_str())
                    .to_string(),
            ));
        }
    }

    if model.loading {
        rows.push(("Status".to_string(), "Loading author info…".to_string()));
    }

    rows
}

fn fact_row(label: &str, value: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("kalam-author-fact-row");

    let key = gtk::Label::new(Some(label));
    key.add_css_class("kalam-author-fact-key");
    key.set_halign(gtk::Align::Start);
    key.set_valign(gtk::Align::Start);
    key.set_xalign(0.0);
    row.append(&key);

    let val = gtk::Label::new(Some(value));
    val.add_css_class("kalam-author-fact-val");
    val.set_halign(gtk::Align::Start);
    val.set_wrap(true);
    val.set_xalign(0.0);
    row.append(&val);

    row
}

fn author_photo_overlay(
    model: &AuthorPageModel,
    sender: &ComponentSender<AuthorPageModel>,
) -> gtk::Overlay {
    let overlay = gtk::Overlay::new();
    overlay.add_css_class("kalam-author-photo-wrap");
    overlay.set_halign(gtk::Align::Center);
    overlay.set_child(Some(&author_photo(model)));

    let refresh = gtk::Button::new();
    refresh.add_css_class("kalam-author-photo-refresh");
    refresh.set_tooltip_text(Some("Refresh author info"));
    refresh.set_halign(gtk::Align::End);
    refresh.set_valign(gtk::Align::End);
    refresh.set_sensitive(!model.loading);
    let icon = gtk::Image::from_icon_name("view-refresh-symbolic");
    refresh.set_child(Some(&icon));
    let tx = sender.input_sender().clone();
    refresh.connect_clicked(move |_| {
        let _ = tx.send(AuthorPageMsg::Refresh);
    });
    overlay.add_overlay(&refresh);

    overlay
}

fn author_stats(model: &AuthorPageModel) -> gtk::Grid {
    let (owned, finished, reading) = status_counts(&model.owned_books);
    let unread = owned.saturating_sub(finished + reading);

    let grid = gtk::Grid::new();
    grid.add_css_class("kalam-author-stats-grid");
    grid.set_row_spacing(10);
    grid.set_column_spacing(10);
    grid.set_halign(gtk::Align::Center);

    for (idx, (label, value, css)) in [
        ("Owned", owned.to_string(), "kalam-author-stat-owned"),
        (
            "Finished",
            finished.to_string(),
            "kalam-author-stat-finished",
        ),
        ("Reading", reading.to_string(), "kalam-author-stat-reading"),
        ("Unread", unread.to_string(), "kalam-author-stat-unread"),
    ]
    .into_iter()
    .enumerate()
    {
        let disc = stat_disc(label, &value, css);
        grid.attach(&disc, (idx % 2) as i32, (idx / 2) as i32, 1, 1);
    }

    grid
}

fn stat_disc(label: &str, value: &str, css: &str) -> gtk::Box {
    let disc = gtk::Box::new(gtk::Orientation::Vertical, 2);
    disc.add_css_class("kalam-author-stat-disc");
    disc.add_css_class(css);
    disc.set_halign(gtk::Align::Center);
    disc.set_valign(gtk::Align::Center);
    disc.set_size_request(92, 92);

    let value_label = gtk::Label::new(Some(value));
    value_label.add_css_class("kalam-author-stat-disc-value");
    value_label.set_halign(gtk::Align::Center);
    disc.append(&value_label);

    let label_widget = gtk::Label::new(Some(label));
    label_widget.add_css_class("kalam-author-stat-disc-label");
    label_widget.set_halign(gtk::Align::Center);
    disc.append(&label_widget);

    disc
}

fn author_photo(model: &AuthorPageModel) -> gtk::Widget {
    if let Some(profile) = &model.profile {
        if let Some(path) = profile.photo_path.as_deref() {
            if path.is_file() {
                let photo = cover_widget(Some(path), 220, 220);
                photo.remove_css_class("kalam-cover-frame");
                photo.add_css_class("kalam-author-photo-shell");
                photo.add_css_class("kalam-author-photo");
                photo.set_overflow(gtk::Overflow::Hidden);
                photo.set_size_request(220, 220);
                return photo;
            }
        }
    }

    let placeholder = gtk::Box::new(gtk::Orientation::Vertical, 0);
    placeholder.add_css_class("kalam-author-photo-placeholder");
    placeholder.set_size_request(220, 220);
    placeholder.set_halign(gtk::Align::Center);
    placeholder.set_valign(gtk::Align::Start);

    let label = gtk::Label::new(Some(&initials(
        model
            .profile
            .as_ref()
            .map(|profile| profile.canonical_name.as_str())
            .unwrap_or(model.requested_name.as_str()),
    )));
    label.add_css_class("kalam-author-photo-initials");
    label.set_halign(gtk::Align::Center);
    label.set_valign(gtk::Align::Center);
    placeholder.append(&label);
    placeholder.upcast()
}

fn series_card(entry: &SeriesProgress, profile: Option<&AuthorProfile>) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    card.add_css_class("kalam-author-series-card");
    card.set_size_request(480, -1);
    card.set_halign(gtk::Align::Start);

    let first_book = entry.owned.first();
    let cover = cover_widget(
        first_book.and_then(|book| book.cover_path.as_deref()),
        72,
        112,
    );
    card.append(&cover);

    let body = gtk::Box::new(gtk::Orientation::Vertical, 6);
    body.set_hexpand(true);

    let title = gtk::Label::new(Some(&entry.name));
    title.add_css_class("kalam-author-series-title");
    title.set_halign(gtk::Align::Start);
    title.set_wrap(true);
    title.set_xalign(0.0);
    body.append(&title);

    let summary = online_series_summary(profile, &entry.name).unwrap_or_default();
    let known_count = summary.known_count.max(entry.owned.len());
    let counts = gtk::Label::new(Some(&format!(
        "{} book{} · you own {}",
        known_count,
        if known_count == 1 { "" } else { "s" },
        entry.owned.len()
    )));
    counts.add_css_class("kalam-author-series-meta");
    counts.set_halign(gtk::Align::Start);
    counts.set_xalign(0.0);
    body.append(&counts);

    if let Some(years) = series_year_text(entry, &summary) {
        let years_label = gtk::Label::new(Some(&years));
        years_label.add_css_class("kalam-author-series-meta");
        years_label.set_halign(gtk::Align::Start);
        years_label.set_xalign(0.0);
        body.append(&years_label);
    }

    let status_text = if entry.missing.is_empty() {
        "Your shelf has no missing numbered books so far.".to_string()
    } else {
        format!("Missing from your shelf: {}", entry.missing.join(" · "))
    };
    let status = gtk::Label::new(Some(&status_text));
    status.add_css_class("kalam-author-series-status");
    status.set_halign(gtk::Align::Start);
    status.set_wrap(true);
    status.set_xalign(0.0);
    body.append(&status);

    card.append(&body);
    card
}

fn online_series_summary(
    profile: Option<&AuthorProfile>,
    series_name: &str,
) -> Option<OnlineSeriesSummary> {
    let profile = profile?;
    let target = normalize_key(series_name);
    if target.is_empty() {
        return None;
    }

    let mut years = Vec::new();
    let mut count = 0usize;
    for work in &profile.works {
        if normalize_key(&work.series_name) == target {
            count += 1;
            if let Some(year) = work.first_publish_year {
                years.push(year);
            }
        }
    }
    if count == 0 {
        return None;
    }

    years.sort_unstable();
    Some(OnlineSeriesSummary {
        known_count: count,
        first_year: years.first().copied(),
        last_year: years.last().copied(),
    })
}

fn series_year_text(entry: &SeriesProgress, summary: &OnlineSeriesSummary) -> Option<String> {
    if let Some(text) = year_range_text(summary.first_year, summary.last_year) {
        return Some(format!("Release: {text}"));
    }

    let mut years = entry.owned.iter().filter_map(book_year).collect::<Vec<_>>();
    years.sort_unstable();
    year_range_text(years.first().copied(), years.last().copied())
        .map(|text| format!("Release: {text}"))
}

fn book_year(book: &Book) -> Option<i64> {
    let digits = book
        .published
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect::<String>();
    if digits.len() < 4 {
        None
    } else {
        digits[..4].parse().ok()
    }
}

fn more_work_groups(model: &AuthorPageModel) -> Vec<WorkGroup> {
    let Some(profile) = &model.profile else {
        return Vec::new();
    };

    let works = works_not_in_library(profile, &model.owned_books);
    let mut grouped: BTreeMap<String, WorkGroupBuilder> = BTreeMap::new();

    for work in works {
        let series_name = work.series_name.trim();
        let (key, title, is_series) = if series_name.is_empty() {
            (
                format!(
                    "work:{}",
                    if work.work_key.trim().is_empty() {
                        normalize_key(&work.title)
                    } else {
                        normalize_key(&work.work_key)
                    }
                ),
                work.title.clone(),
                false,
            )
        } else {
            (
                format!("series:{}", normalize_key(series_name)),
                series_name.to_string(),
                true,
            )
        };

        let entry = grouped.entry(key).or_insert_with(|| WorkGroupBuilder {
            title,
            is_series,
            ..WorkGroupBuilder::default()
        });
        entry.work_count += 1;
        if entry.cover_path.is_none() {
            entry.cover_path = work_cover_path(&work);
        }
        if let Some(year) = work.first_publish_year {
            entry.years.push(year);
        }
        if let Some(avg) = work.rating_average {
            if work.rating_count > 0 {
                let count = work.rating_count as f64;
                entry.rating_weight += (avg as f64) * count;
                entry.rating_count += work.rating_count;
            }
        }
    }

    let mut groups = grouped
        .into_values()
        .map(|mut entry| {
            entry.years.sort_unstable();
            WorkGroup {
                title: entry.title,
                work_count: entry.work_count,
                first_year: entry.years.first().copied(),
                last_year: entry.years.last().copied(),
                cover_path: entry.cover_path,
                rating_average: if entry.rating_count > 0 {
                    Some((entry.rating_weight / entry.rating_count as f64) as f32)
                } else {
                    None
                },
                rating_count: entry.rating_count,
                is_series: entry.is_series,
            }
        })
        .collect::<Vec<_>>();

    groups.sort_by(|a, b| {
        a.first_year
            .unwrap_or(i64::MAX)
            .cmp(&b.first_year.unwrap_or(i64::MAX))
            .then_with(|| a.title.cmp(&b.title))
    });
    groups.truncate(18);
    groups
}

fn work_group_card(group: &WorkGroup) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("kalam-author-discovery-card");
    card.set_size_request(210, -1);
    card.set_halign(gtk::Align::Start);

    card.append(&work_group_cover(group));

    let title = gtk::Label::new(Some(&group.title));
    title.add_css_class("kalam-author-discovery-title");
    title.set_halign(gtk::Align::Start);
    title.set_wrap(true);
    title.set_xalign(0.0);
    card.append(&title);

    let kind = if group.is_series {
        format!(
            "Series · {} book{}",
            group.work_count,
            if group.work_count == 1 { "" } else { "s" }
        )
    } else {
        "Standalone".to_string()
    };
    let meta = gtk::Label::new(Some(&kind));
    meta.add_css_class("kalam-author-discovery-meta");
    meta.set_halign(gtk::Align::Start);
    meta.set_xalign(0.0);
    card.append(&meta);

    if let Some(years) = year_range_text(group.first_year, group.last_year) {
        let years_label = gtk::Label::new(Some(&years));
        years_label.add_css_class("kalam-author-discovery-meta");
        years_label.set_halign(gtk::Align::Start);
        years_label.set_xalign(0.0);
        card.append(&years_label);
    }

    if let Some(avg) = group.rating_average {
        let rating_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        rating_box.set_halign(gtk::Align::Start);
        rating_box.append(&stars_label(rating_half_stars(avg)));
        if group.rating_count > 0 {
            let count = gtk::Label::new(Some(&format!("{} ratings", group.rating_count)));
            count.add_css_class("kalam-author-discovery-meta");
            count.set_halign(gtk::Align::Start);
            rating_box.append(&count);
        }
        card.append(&rating_box);
    }

    card
}

fn work_group_cover(group: &WorkGroup) -> gtk::Widget {
    cover_widget(group.cover_path.as_deref(), 136, 204)
}

fn work_cover_path(work: &AuthorWork) -> Option<PathBuf> {
    let file = work.cover_file.as_deref()?;
    let path = crate::paths::authors_dir().join(file);
    path.is_file().then_some(path)
}

fn rating_half_stars(value: f32) -> u8 {
    (value * 2.0).round().clamp(0.0, 10.0) as u8
}

fn year_range_text(first: Option<i64>, last: Option<i64>) -> Option<String> {
    match (first, last) {
        (Some(a), Some(b)) if a == b => Some(a.to_string()),
        (Some(a), Some(b)) => Some(format!("{a}–{b}")),
        (Some(a), None) => Some(a.to_string()),
        (None, Some(b)) => Some(b.to_string()),
        (None, None) => None,
    }
}

fn simple_note(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-author-empty");
    label.set_halign(gtk::Align::Start);
    label.set_wrap(true);
    label.set_xalign(0.0);
    label
}

fn normalize_key(text: &str) -> String {
    text.to_lowercase()
        .chars()
        .filter(|ch| ch.is_alphanumeric())
        .collect()
}

fn clear_box(host: &gtk::Box) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
}
