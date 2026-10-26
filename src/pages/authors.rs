//! Authors index (decision 117): every author in the library, one
//! card each, opening the author page.
//!
//! The page was left undesigned, so it follows the author page's
//! language: plain `bg`, the accent eyebrow and Playfair Display title,
//! outlined radius-14 cards with the 64px circular portrait (or the
//! neutral initials disc), Meta counts underneath. The grouping —
//! spelling variants folded together, comic chapters collapsed —
//! lives in `LibraryService::authors`; this file only renders it,
//! filters it, and sorts it.

use crate::author::{initials, normalize_author_name};
use crate::service::{AuthorIndexEntry, AuthorsSnapshot, LibraryService};
use crate::widgets::book_row::{cache_decoded_cover, cover_widget};
use crate::widgets::segmented::{segmented_control, SegmentedSize};
use gtk::prelude::*;
use relm4::prelude::*;

const PORTRAIT: i32 = 64;

#[derive(Debug)]
pub enum AuthorsOut {
    OpenAuthor { name: String },
}

#[derive(Debug)]
pub enum AuthorsMsg {
    Refresh,
    Loaded {
        gen: u64,
        load: Box<AuthorsLoad>,
    },
    SearchChanged(String),
    SortChanged(usize),
}

/// The snapshot plus the portraits its worker decoded — the author
/// page's recipe, so the fill's cover lookups hit the cache instead
/// of decoding on the UI thread.
#[derive(Debug)]
pub struct AuthorsLoad {
    snap: AuthorsSnapshot,
    covers: Vec<crate::preload::DecodedCover>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AuthorSort {
    Name,
    MostBooks,
    MostRead,
}

pub struct AuthorsModel {
    service: LibraryService,
    snapshot: Option<AuthorsSnapshot>,
    query: String,
    sort: AuthorSort,
    load_gen: u64,
}

#[relm4::component(pub)]
impl Component for AuthorsModel {
    type Init = LibraryService;
    type Input = AuthorsMsg;
    type Output = AuthorsOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "kalam-authorspage-inner",
            set_hexpand: true,
            set_vexpand: false,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,

                gtk::Label {
                    set_label: "LIBRARY",
                    add_css_class: "kalam-authorpage-eyebrow",
                    set_halign: gtk::Align::Start,
                },
                gtk::Label {
                    set_label: "Authors",
                    add_css_class: "kalam-authorpage-title",
                    set_halign: gtk::Align::Start,
                },
                gtk::Label {
                    #[watch]
                    set_label: &model.status_line(),
                    add_css_class: "kalam-ui-meta",
                    set_halign: gtk::Align::Start,
                },
            },

            #[name = "controls_host"]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 16,
                set_margin_top: 24,
                set_valign: gtk::Align::Center,
            },

            #[name = "content_host"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                set_margin_top: 24,
            },
        }
    }

    fn init(
        service: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = AuthorsModel {
            service,
            snapshot: None,
            query: String::new(),
            sort: AuthorSort::Name,
            load_gen: 0,
        };
        let widgets = view_output!();

        let search = gtk::SearchEntry::new();
        search.add_css_class("kalam-authors-search");
        search.set_placeholder_text(Some("Search authors…"));
        search.set_size_request(360, 44);
        search.set_hexpand(false);
        let s = sender.clone();
        search.connect_search_changed(move |entry| {
            s.input(AuthorsMsg::SearchChanged(entry.text().to_string()));
        });
        widgets.controls_host.append(&search);

        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        widgets.controls_host.append(&spacer);

        let s = sender.clone();
        let sort = segmented_control(
            &["Name", "Most books", "Most read"],
            0,
            SegmentedSize::Small,
            move |index| {
                s.input(AuthorsMsg::SortChanged(index));
            },
        );
        widgets.controls_host.append(&sort.widget());

        model.rebuild(&widgets, &sender);
        model.request_snapshot(&sender);
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
            AuthorsMsg::Refresh => {
                self.request_snapshot(&sender);
                return;
            }
            AuthorsMsg::Loaded { gen, load } => {
                if gen != self.load_gen {
                    return;
                }
                for decoded in &load.covers {
                    cache_decoded_cover(decoded);
                }
                if let Some(error) = load.snap.errors.first() {
                    crate::notify::error("Some authors may be missing", error);
                }
                self.snapshot = Some(load.snap);
            }
            AuthorsMsg::SearchChanged(query) => self.query = query,
            AuthorsMsg::SortChanged(index) => {
                self.sort = match index {
                    1 => AuthorSort::MostBooks,
                    2 => AuthorSort::MostRead,
                    _ => AuthorSort::Name,
                };
            }
        }
        self.rebuild(widgets, &sender);
        self.update_view(widgets, sender);
    }
}

impl AuthorsModel {
    fn request_snapshot(&mut self, sender: &ComponentSender<Self>) {
        self.load_gen += 1;
        let gen = self.load_gen;
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Reading authors",
            move |_reporter| {
                let snap = service.authors();
                let covers = snap
                    .entries
                    .iter()
                    .filter_map(|entry| entry.photo_path.as_deref())
                    .filter_map(|path| {
                        crate::preload::decode_for_cache(path, PORTRAIT, PORTRAIT)
                    })
                    .collect();
                AuthorsLoad { snap, covers }
            },
            |_| {},
            move |load| {
                let _ = s.send(AuthorsMsg::Loaded {
                    gen,
                    load: Box::new(load),
                });
            },
        );
    }

    fn status_line(&self) -> String {
        let Some(snap) = &self.snapshot else {
            return "Loading authors…".to_string();
        };
        let total = snap.entries.len();
        if total == 0 {
            return "No authors yet".to_string();
        }
        if self.query.trim().is_empty() {
            format!(
                "{total} author{} in your library",
                if total == 1 { "" } else { "s" }
            )
        } else {
            format!("{} of {total} authors", self.visible_entries().len())
        }
    }

    fn visible_entries(&self) -> Vec<&AuthorIndexEntry> {
        let Some(snap) = &self.snapshot else {
            return Vec::new();
        };
        let query = normalize_author_name(self.query.trim());
        let mut entries: Vec<&AuthorIndexEntry> = snap
            .entries
            .iter()
            .filter(|entry| {
                query.is_empty()
                    || normalize_author_name(&entry.name).contains(&query)
                    || normalize_author_name(&entry.sort_name).contains(&query)
                    || entry
                        .aliases
                        .iter()
                        .any(|alias| normalize_author_name(alias).contains(&query))
            })
            .collect();
        let by_name = |a: &AuthorIndexEntry, b: &AuthorIndexEntry| {
            a.sort_name
                .to_lowercase()
                .cmp(&b.sort_name.to_lowercase())
                .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
        };
        match self.sort {
            AuthorSort::Name => entries.sort_by(|a, b| by_name(a, b)),
            AuthorSort::MostBooks => entries.sort_by(|a, b| {
                b.book_count.cmp(&a.book_count).then_with(|| by_name(a, b))
            }),
            AuthorSort::MostRead => entries.sort_by(|a, b| {
                b.read_count
                    .cmp(&a.read_count)
                    .then_with(|| b.book_count.cmp(&a.book_count))
                    .then_with(|| by_name(a, b))
            }),
        }
        entries
    }

    fn rebuild(&self, widgets: &AuthorsModelWidgets, sender: &ComponentSender<Self>) {
        let _t = crate::timing::measure("authors_fill");
        let _a = crate::timing::activity("authors_fill");
        let host = &widgets.content_host;
        while let Some(child) = host.first_child() {
            host.remove(&child);
        }

        let Some(snap) = &self.snapshot else {
            host.append(&empty_label("Loading authors…"));
            return;
        };
        if snap.entries.is_empty() {
            host.append(&empty_label(if snap.errors.is_empty() {
                "No authors yet — import a book and its author will appear here."
            } else {
                "Authors could not be loaded."
            }));
            return;
        }
        let visible = self.visible_entries();
        if visible.is_empty() {
            host.append(&empty_label(&format!(
                "No authors match “{}”.",
                self.query.trim()
            )));
            return;
        }

        if self.sort == AuthorSort::Name {
            let mut current_letter: Option<String> = None;
            let mut flow: Option<gtk::FlowBox> = None;
            for entry in visible {
                let letter = group_letter(entry);
                if current_letter.as_deref() != Some(letter.as_str()) {
                    let heading = gtk::Label::new(Some(&letter));
                    heading.add_css_class("kalam-authors-group");
                    heading.set_halign(gtk::Align::Start);
                    if current_letter.is_some() {
                        heading.set_margin_top(28);
                    }
                    host.append(&heading);
                    let new_flow = author_flow();
                    new_flow.set_margin_top(12);
                    host.append(&new_flow);
                    current_letter = Some(letter);
                    flow = Some(new_flow);
                }
                if let Some(flow) = &flow {
                    flow.insert(&author_card(entry, sender), -1);
                }
            }
        } else {
            let flow = author_flow();
            for entry in visible {
                flow.insert(&author_card(entry, sender), -1);
            }
            host.append(&flow);
        }
    }
}

fn group_letter(entry: &AuthorIndexEntry) -> String {
    entry
        .sort_name
        .chars()
        .find(|c| c.is_alphabetic())
        .map(|c| c.to_uppercase().collect::<String>())
        .unwrap_or_else(|| "#".to_string())
}

fn author_flow() -> gtk::FlowBox {
    let flow = gtk::FlowBox::new();
    flow.set_valign(gtk::Align::Start);
    flow.set_homogeneous(true);
    flow.set_min_children_per_line(1);
    flow.set_max_children_per_line(3);
    flow.set_column_spacing(16);
    flow.set_row_spacing(16);
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_activate_on_single_click(false);
    flow
}

fn empty_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-ui-meta");
    label.set_halign(gtk::Align::Start);
    label.set_wrap(true);
    label.set_xalign(0.0);
    label
}

fn lifespan(entry: &AuthorIndexEntry) -> Option<String> {
    match (entry.birth_year, entry.death_year) {
        (Some(b), Some(d)) => Some(format!("{b}–{d}")),
        (Some(b), None) => Some(format!("Born {b}")),
        (None, Some(d)) => Some(format!("Died {d}")),
        (None, None) => None,
    }
}

fn detail_line(entry: &AuthorIndexEntry) -> String {
    let bio = entry
        .bio
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if !bio.is_empty() {
        return bio;
    }
    if !entry.top_subjects.is_empty() {
        return entry
            .top_subjects
            .iter()
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .take(3)
            .collect::<Vec<_>>()
            .join(" · ");
    }
    let name_key = normalize_author_name(&entry.name);
    let sort_key = normalize_author_name(&entry.sort_name);
    entry
        .aliases
        .iter()
        .find(|alias| {
            let key = normalize_author_name(alias);
            key != name_key && key != sort_key
        })
        .map(|alias| format!("Also as {}", crate::author::display_author_name(alias)))
        .unwrap_or_default()
}

fn portrait(entry: &AuthorIndexEntry) -> gtk::Widget {
    match entry.photo_path.as_deref() {
        Some(path) => {
            let widget = cover_widget(Some(path), PORTRAIT, PORTRAIT);
            widget.add_css_class("kalam-authorpage-portrait");
            widget.set_size_request(PORTRAIT, PORTRAIT);
            widget
        }
        None => {
            let disc = gtk::Box::new(gtk::Orientation::Vertical, 0);
            disc.add_css_class("kalam-authorpage-portrait-initials");
            disc.set_size_request(PORTRAIT, PORTRAIT);
            disc.set_halign(gtk::Align::Start);
            let label = gtk::Label::new(Some(&initials(&entry.name)));
            label.add_css_class("kalam-authorpage-initials-text");
            label.set_halign(gtk::Align::Center);
            label.set_valign(gtk::Align::Center);
            label.set_vexpand(true);
            disc.append(&label);
            disc.upcast()
        }
    }
}

fn author_card(entry: &AuthorIndexEntry, sender: &ComponentSender<AuthorsModel>) -> gtk::Button {
    let button = gtk::Button::new();
    button.add_css_class("kalam-authors-card");
    button.set_size_request(-1, 124);
    button.set_hexpand(true);
    button.set_valign(gtk::Align::Start);
    button.set_can_focus(true);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    row.set_valign(gtk::Align::Center);
    let portrait = portrait(entry);
    portrait.set_valign(gtk::Align::Center);
    row.append(&portrait);

    let info = gtk::Box::new(gtk::Orientation::Vertical, 3);
    info.set_hexpand(true);
    info.set_valign(gtk::Align::Center);

    let name = gtk::Label::new(Some(&entry.name));
    name.add_css_class("kalam-ui-card-title");
    name.set_halign(gtk::Align::Start);
    name.set_xalign(0.0);
    name.set_wrap(true);
    name.set_lines(2);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    info.append(&name);

    if let Some(years) = lifespan(entry) {
        let years_label = gtk::Label::new(Some(&years));
        years_label.add_css_class("kalam-ui-meta");
        years_label.set_halign(gtk::Align::Start);
        info.append(&years_label);
    }

    let mut meta = format!(
        "{} in library · {} read",
        entry.book_count, entry.read_count
    );
    if entry.series_count > 0 {
        meta.push_str(&format!(
            " · {} series",
            entry.series_count
        ));
    }
    if let Some(known) = entry.known_works {
        if known > entry.book_count as i64 {
            meta.push_str(&format!(" · {known} works known"));
        }
    }
    let meta_label = gtk::Label::new(Some(&meta));
    meta_label.add_css_class("kalam-ui-meta");
    meta_label.set_halign(gtk::Align::Start);
    meta_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    info.append(&meta_label);

    let detail = detail_line(entry);
    if !detail.is_empty() {
        let detail_label = gtk::Label::new(Some(&detail));
        detail_label.add_css_class("kalam-ui-meta");
        detail_label.set_halign(gtk::Align::Start);
        detail_label.set_xalign(0.0);
        detail_label.set_wrap(true);
        detail_label.set_lines(2);
        detail_label.set_width_chars(1);
        detail_label.set_max_width_chars(44);
        detail_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        info.append(&detail_label);
    }
    row.append(&info);

    let chevron = gtk::Image::from_icon_name("go-next-symbolic");
    chevron.set_pixel_size(18);
    chevron.add_css_class("kalam-authors-chevron");
    chevron.set_valign(gtk::Align::Center);
    row.append(&chevron);

    button.set_child(Some(&row));
    let accessible = format!(
        "Open {}, {} book{} in library",
        entry.name,
        entry.book_count,
        if entry.book_count == 1 { "" } else { "s" }
    );
    button.set_tooltip_text(Some(&accessible));
    button.update_property(&[gtk::accessible::Property::Label(&accessible)]);
    let name = entry.name.clone();
    let s = sender.clone();
    button.connect_clicked(move |_| {
        let _ = s.output(AuthorsOut::OpenAuthor { name: name.clone() });
    });
    button
}
