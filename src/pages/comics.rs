//! Comics & Manga hub page: browse local CBZ/CBR comic archives and launch reader.

use crate::db::{Catalog, SortKey};
use crate::models::{Book, BookFormat};
use crate::pages::all_books::{import_summary, spawn_import, ImportTally};
use crate::service::LibraryService;
use crate::widgets::book_row::{build_book_grid, cover_widget, cover_widget_deferred};
use gtk::prelude::*;
use relm4::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug)]
// The three are "open something" on purpose; the shared prefix is the
// point, not an accident.
#[allow(clippy::enum_variant_names)]
pub enum ComicsOut {
    OpenComic { book_id: i64 },
    OpenBookDialog { book_id: i64 },
    OpenRemoteManga { source_id: String, remote_id: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ComicViewMode {
    #[default]
    Series,
    AllFiles,
}

#[derive(Debug, Clone)]
pub struct ComicSeriesGroup {
    pub series_name: String,
    pub authors: String,
    pub cover_path: Option<PathBuf>,
    pub chapters: Vec<Book>,
    pub total_chapters: usize,
    pub unread_count: usize,
    pub next_to_read: Option<Book>,
}

#[derive(Debug)]
#[allow(clippy::enum_variant_names)]
pub enum ComicsMsg {
    #[allow(dead_code)] // reserved refresh action for the comics grid
    Refresh,
    SearchChanged(String),
    ToggleViewMode,
    OpenSeriesDrawer(String),
    CloseSeriesDrawer,
    PickFiles,
    FilesChosen(Vec<PathBuf>),
    ImportStep {
        done: usize,
        total: usize,
        title: String,
    },
    ImportFinished(ImportTally),
    OpenComic(i64),
    OpenBookDialog(i64),
    OpenRemoteManga(String),
}

pub struct ComicsModel {
    service: LibraryService,
    comics: Vec<Book>,
    series_groups: Vec<ComicSeriesGroup>,
    standalone_comics: Vec<Book>,
    remote_manga: Vec<crate::sources::RemoteBookDetails>,
    filtered_series: Vec<ComicSeriesGroup>,
    filtered_standalone: Vec<Book>,
    filtered_all: Vec<Book>,
    search_query: String,
    status: String,
    view_mode: ComicViewMode,
    active_drawer_series: Option<String>,
}

pub fn group_comics_into_series(comics: &[Book]) -> (Vec<ComicSeriesGroup>, Vec<Book>) {
    let mut series_map: std::collections::HashMap<String, Vec<Book>> =
        std::collections::HashMap::new();
    let mut standalone = Vec::new();

    for book in comics {
        if let Some(ref s) = book.series {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                series_map
                    .entry(trimmed.to_string())
                    .or_default()
                    .push(book.clone());
                continue;
            }
        }
        standalone.push(book.clone());
    }

    let mut groups = Vec::new();
    for (series_name, mut chapters) in series_map {
        chapters.sort_by(|a, b| {
            if a.series_index > 0.0 && b.series_index > 0.0 {
                a.series_index
                    .partial_cmp(&b.series_index)
                    .unwrap_or(std::cmp::Ordering::Equal)
            } else if a.series_index > 0.0 {
                std::cmp::Ordering::Less
            } else if b.series_index > 0.0 {
                std::cmp::Ordering::Greater
            } else {
                a.title.cmp(&b.title)
            }
        });

        let total_chapters = chapters.len();
        let unread_count = chapters.iter().filter(|c| c.progress == 0).count();
        let next_to_read = chapters
            .iter()
            .find(|c| c.progress < 100)
            .or_else(|| chapters.first())
            .cloned();
        let cover_path = chapters.iter().find_map(|c| c.cover_path.clone());
        let authors = chapters
            .first()
            .map(|c| c.authors_display().to_string())
            .unwrap_or_else(|| "Unknown".to_string());

        groups.push(ComicSeriesGroup {
            series_name,
            authors,
            cover_path,
            chapters,
            total_chapters,
            unread_count,
            next_to_read,
        });
    }

    groups.sort_by(|a, b| a.series_name.to_lowercase().cmp(&b.series_name.to_lowercase()));
    standalone.sort_by(|a, b| a.title.cmp(&b.title));

    (groups, standalone)
}

impl ComicsModel {
    pub fn new(catalog: Arc<Catalog>) -> Self {
        let service = LibraryService::new(catalog);
        let mut model = Self {
            service,
            comics: Vec::new(),
            series_groups: Vec::new(),
            standalone_comics: Vec::new(),
            remote_manga: Vec::new(),
            filtered_series: Vec::new(),
            filtered_standalone: Vec::new(),
            filtered_all: Vec::new(),
            search_query: String::new(),
            status: String::new(),
            view_mode: ComicViewMode::Series,
            active_drawer_series: None,
        };
        model.reload();
        model
    }

    fn reload(&mut self) {
        let _ = self.service.catalog().backfill_comic_series();

        if let Ok(all_books) = self.service.catalog().list_books(SortKey::Added, "") {
            self.comics = all_books
                .into_iter()
                .filter(|b| matches!(b.format, BookFormat::Cbz | BookFormat::Cbr))
                .collect();
        } else {
            self.comics = Vec::new();
        }
        self.remote_manga = self
            .service
            .catalog()
            .list_remote_books(Some("weebcentral"))
            .unwrap_or_default();

        let (groups, standalone) = group_comics_into_series(&self.comics);
        self.series_groups = groups;
        self.standalone_comics = standalone;

        if let Some(ref active) = self.active_drawer_series {
            if !self.series_groups.iter().any(|g| &g.series_name == active) {
                self.active_drawer_series = None;
            }
        }

        self.apply_filter();
        self.update_status();
    }

    fn apply_filter(&mut self) {
        if self.search_query.is_empty() {
            self.filtered_series = self.series_groups.clone();
            self.filtered_standalone = self.standalone_comics.clone();
            self.filtered_all = self.comics.clone();
        } else {
            let query = self.search_query.to_lowercase();
            self.filtered_series = self
                .series_groups
                .iter()
                .filter(|g| {
                    g.series_name.to_lowercase().contains(&query)
                        || g.authors.to_lowercase().contains(&query)
                        || g.chapters
                            .iter()
                            .any(|c| c.title.to_lowercase().contains(&query))
                })
                .cloned()
                .collect();
            self.filtered_standalone = self
                .standalone_comics
                .iter()
                .filter(|b| {
                    b.title.to_lowercase().contains(&query)
                        || b.authors.to_lowercase().contains(&query)
                })
                .cloned()
                .collect();
            self.filtered_all = self
                .comics
                .iter()
                .filter(|b| {
                    b.title.to_lowercase().contains(&query)
                        || b.authors.to_lowercase().contains(&query)
                        || b.series
                            .as_deref()
                            .unwrap_or("")
                            .to_lowercase()
                            .contains(&query)
                })
                .cloned()
                .collect();
        }
    }

    fn update_status(&mut self) {
        let s_count = self.series_groups.len();
        let c_count = self.comics.len();
        let m_count = self.remote_manga.len();
        self.status = if c_count == 0 && m_count == 0 {
            "No comics or manga in library yet".to_string()
        } else {
            let mut parts = Vec::new();
            if s_count > 0 {
                let standalone_count = self.standalone_comics.len();
                if standalone_count > 0 {
                    parts.push(format!(
                        "{s_count} series, {standalone_count} standalone ({c_count} files total)"
                    ));
                } else {
                    parts.push(format!("{s_count} series ({c_count} chapters)"));
                }
            } else if c_count > 0 {
                parts.push(format!(
                    "{c_count} local comic{}",
                    if c_count == 1 { "" } else { "s" }
                ));
            }
            if m_count > 0 {
                parts.push(format!("{m_count} saved manga"));
            }
            parts.join(", ")
        };
    }
}

fn build_empty_state(sender: &ComponentSender<ComicsModel>) -> gtk::Box {
    let empty_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
    empty_box.set_halign(gtk::Align::Center);
    empty_box.set_valign(gtk::Align::Center);
    empty_box.set_margin_top(48);
    empty_box.set_margin_bottom(48);

    let icon = gtk::Image::from_icon_name("image-x-generic-symbolic");
    icon.set_pixel_size(64);
    icon.add_css_class("kalam-subtitle-muted");
    empty_box.append(&icon);

    let title = gtk::Label::new(Some("No comics or manga in your library"));
    title.add_css_class("kalam-title-medium");
    empty_box.append(&title);

    let subtitle = gtk::Label::new(Some(
        "Import local CBZ or CBR archive files to read comics with the Moku viewer, or browse online manga.",
    ));
    subtitle.add_css_class("kalam-subtitle-muted");
    empty_box.append(&subtitle);

    let btn = gtk::Button::with_label("Import Comics");
    btn.add_css_class("kalam-btn-filled");
    btn.set_halign(gtk::Align::Center);
    btn.connect_clicked({
        let s = sender.clone();
        move |_| s.input(ComicsMsg::PickFiles)
    });
    empty_box.append(&btn);

    empty_box
}

fn build_series_card(
    group: &ComicSeriesGroup,
    sender: &ComponentSender<ComicsModel>,
) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.set_size_request(150, -1);
    card.add_css_class("kalam-card");

    let top_click_area = gtk::Box::new(gtk::Orientation::Vertical, 6);
    top_click_area.set_cursor_from_name(Some("pointer"));
    top_click_area.set_tooltip_text(Some("Click to view all chapters"));

    let cover = cover_widget_deferred(group.cover_path.as_deref(), 150, 210);
    cover.add_css_class("kalam-book-card-cover");
    top_click_area.append(&cover);

    let title_lbl = gtk::Label::new(Some(&group.series_name));
    title_lbl.set_wrap(true);
    title_lbl.set_max_width_chars(18);
    title_lbl.set_lines(2);
    title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title_lbl.add_css_class("kalam-title-small");
    title_lbl.set_halign(gtk::Align::Start);
    top_click_area.append(&title_lbl);

    let sub_text = if group.unread_count == 0 && group.total_chapters > 0 {
        format!("{} ch · All read ✓", group.total_chapters)
    } else if group.unread_count < group.total_chapters {
        format!("{} ch · {} unread", group.total_chapters, group.unread_count)
    } else {
        format!(
            "{} chapter{}",
            group.total_chapters,
            if group.total_chapters == 1 { "" } else { "s" }
        )
    };
    let sub_lbl = gtk::Label::new(Some(&sub_text));
    sub_lbl.add_css_class("kalam-subtitle-muted");
    sub_lbl.set_halign(gtk::Align::Start);
    top_click_area.append(&sub_lbl);

    let s_drawer = sender.clone();
    let sname = group.series_name.clone();
    let click = gtk::GestureClick::new();
    click.connect_released(move |_, _, _, _| {
        s_drawer.input(ComicsMsg::OpenSeriesDrawer(sname.clone()));
    });
    top_click_area.add_controller(click);

    card.append(&top_click_area);

    if let Some(ref next) = group.next_to_read {
        let resume_lbl = if group.unread_count == 0 && group.total_chapters > 0 {
            "Ch. 1".to_string()
        } else if next.progress > 0 {
            if next.series_index > 0.0 {
                format!("Resume #{}", next.series_index)
            } else {
                "Resume".to_string()
            }
        } else if next.series_index > 0.0 {
            format!("Start #{}", next.series_index)
        } else {
            "Start".to_string()
        };
        let resume_btn = gtk::Button::with_label(&resume_lbl);
        resume_btn.add_css_class("kalam-btn-subtle");
        resume_btn.set_tooltip_text(Some(&format!("Open {}", next.title)));
        let next_id = next.id;
        let s_open = sender.clone();
        resume_btn.connect_clicked(move |_| {
            s_open.input(ComicsMsg::OpenComic(next_id));
        });
        card.append(&resume_btn);
    }

    card
}

fn build_series_drawer_view(
    group: &ComicSeriesGroup,
    sender: &ComponentSender<ComicsModel>,
) -> gtk::Box {
    let root_box = gtk::Box::new(gtk::Orientation::Vertical, 16);
    root_box.set_margin_start(16);
    root_box.set_margin_end(16);
    root_box.set_margin_top(8);
    root_box.set_margin_bottom(24);

    // Top navigation row: Back button
    let top_nav = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let back_btn = gtk::Button::with_label("← All Series");
    back_btn.add_css_class("kalam-btn-subtle");
    let s_back = sender.clone();
    back_btn.connect_clicked(move |_| {
        s_back.input(ComicsMsg::CloseSeriesDrawer);
    });
    top_nav.append(&back_btn);
    root_box.append(&top_nav);

    // Hero banner card
    let hero = gtk::Box::new(gtk::Orientation::Horizontal, 20);
    hero.add_css_class("kalam-card");

    let cover = cover_widget(group.cover_path.as_deref(), 120, 180);
    cover.add_css_class("kalam-book-card-cover");
    hero.append(&cover);

    let info_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    info_box.set_hexpand(true);
    info_box.set_valign(gtk::Align::Center);

    let title_lbl = gtk::Label::new(Some(&group.series_name));
    title_lbl.add_css_class("kalam-title-large");
    title_lbl.set_halign(gtk::Align::Start);
    title_lbl.set_wrap(true);
    info_box.append(&title_lbl);

    let author_lbl = gtk::Label::new(Some(&group.authors));
    author_lbl.add_css_class("kalam-subtitle-muted");
    author_lbl.set_halign(gtk::Align::Start);
    info_box.append(&author_lbl);

    let completed_count = group.total_chapters.saturating_sub(group.unread_count);
    let stats_text = format!(
        "{} chapter{} · {} completed · {} unread",
        group.total_chapters,
        if group.total_chapters == 1 { "" } else { "s" },
        completed_count,
        group.unread_count
    );
    let stats_lbl = gtk::Label::new(Some(&stats_text));
    stats_lbl.add_css_class("kalam-card-meta");
    stats_lbl.set_halign(gtk::Align::Start);
    info_box.append(&stats_lbl);

    let pbar = gtk::ProgressBar::new();
    let fraction = if group.total_chapters > 0 {
        completed_count as f64 / group.total_chapters as f64
    } else {
        0.0
    };
    pbar.set_fraction(fraction);
    pbar.set_hexpand(true);
    info_box.append(&pbar);

    if let Some(ref next) = group.next_to_read {
        let resume_lbl = if group.unread_count == 0 && group.total_chapters > 0 {
            "Restart Series (Ch. 1)".to_string()
        } else if next.progress > 0 {
            if next.series_index > 0.0 {
                format!("Resume Chapter {}", next.series_index)
            } else {
                format!("Resume: {}", next.title)
            }
        } else if next.series_index > 0.0 {
            format!("Start Reading Chapter {}", next.series_index)
        } else {
            format!("Start Reading: {}", next.title)
        };
        let resume_btn = gtk::Button::with_label(&resume_lbl);
        resume_btn.add_css_class("kalam-btn-filled");
        resume_btn.set_halign(gtk::Align::Start);
        let next_id = next.id;
        let s_open = sender.clone();
        resume_btn.connect_clicked(move |_| {
            s_open.input(ComicsMsg::OpenComic(next_id));
        });
        info_box.append(&resume_btn);
    }

    hero.append(&info_box);
    root_box.append(&hero);

    // Chapters section header
    let sec_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    sec_header.set_margin_top(12);
    let sec_title = gtk::Label::new(Some("Chapters"));
    sec_title.add_css_class("kalam-title-medium");
    sec_title.set_halign(gtk::Align::Start);
    sec_header.append(&sec_title);
    root_box.append(&sec_header);

    // Chapter list container
    let list_box = gtk::ListBox::new();
    list_box.set_selection_mode(gtk::SelectionMode::None);

    for ch in &group.chapters {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row_box.add_css_class("kalam-list-row");
        row_box.set_margin_bottom(6);

        let main_click_area = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        main_click_area.set_hexpand(true);
        main_click_area.set_cursor_from_name(Some("pointer"));

        let num_str = if ch.series_index > 0.0 {
            format!("#{}", ch.series_index)
        } else {
            "—".to_string()
        };
        let num_lbl = gtk::Label::new(Some(&num_str));
        num_lbl.add_css_class("kalam-series-num");
        num_lbl.set_size_request(45, -1);
        num_lbl.set_halign(gtk::Align::Start);
        num_lbl.set_valign(gtk::Align::Center);
        main_click_area.append(&num_lbl);

        let title_lbl = gtk::Label::new(Some(&ch.title));
        title_lbl.add_css_class("kalam-series-name");
        title_lbl.set_hexpand(true);
        title_lbl.set_halign(gtk::Align::Start);
        title_lbl.set_valign(gtk::Align::Center);
        title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        main_click_area.append(&title_lbl);

        let badge = if ch.progress >= 100 {
            let b = gtk::Label::new(Some("Read"));
            b.add_css_class("kalam-badge-series-read");
            b
        } else if ch.progress > 0 {
            let b = gtk::Label::new(Some(&format!("{}%", ch.progress)));
            b.add_css_class("kalam-badge-series-reading");
            b
        } else {
            let b = gtk::Label::new(Some("Unread"));
            b.add_css_class("kalam-badge-series-unread");
            b
        };
        badge.set_valign(gtk::Align::Center);
        main_click_area.append(&badge);

        let fmt_lbl = gtk::Label::new(Some(&ch.format.to_string().to_uppercase()));
        fmt_lbl.add_css_class("kalam-card-meta");
        fmt_lbl.set_valign(gtk::Align::Center);
        main_click_area.append(&fmt_lbl);

        let ch_id = ch.id;
        let s_click = sender.clone();
        let click = gtk::GestureClick::new();
        click.connect_released(move |_, _, _, _| {
            s_click.input(ComicsMsg::OpenComic(ch_id));
        });
        main_click_area.add_controller(click);

        row_box.append(&main_click_area);

        let info_btn = gtk::Button::from_icon_name("document-properties-symbolic");
        info_btn.add_css_class("flat");
        info_btn.set_tooltip_text(Some("Book Details"));
        info_btn.set_valign(gtk::Align::Center);
        let s_info = sender.clone();
        info_btn.connect_clicked(move |_| {
            s_info.input(ComicsMsg::OpenBookDialog(ch_id));
        });
        row_box.append(&info_btn);

        list_box.append(&row_box);
    }

    root_box.append(&list_box);
    root_box
}

fn rebuild_comics_view(model: &ComicsModel, sender: &ComponentSender<ComicsModel>) -> gtk::Widget {
    if model.comics.is_empty() && model.remote_manga.is_empty() {
        return build_empty_state(sender).upcast();
    }

    if let Some(ref sname) = model.active_drawer_series {
        if let Some(group) = model.series_groups.iter().find(|g| &g.series_name == sname) {
            return build_series_drawer_view(group, sender).upcast();
        }
    }

    let root_box = gtk::Box::new(gtk::Orientation::Vertical, 20);
    root_box.set_margin_start(16);
    root_box.set_margin_end(16);
    root_box.set_margin_top(12);
    root_box.set_margin_bottom(24);

    if !model.remote_manga.is_empty() {
        let sec_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title = gtk::Label::new(Some("Saved Online Manga"));
        title.add_css_class("kalam-title-medium");
        title.set_halign(gtk::Align::Start);
        sec_header.append(&title);
        root_box.append(&sec_header);

        let flow = gtk::FlowBox::new();
        flow.set_selection_mode(gtk::SelectionMode::None);
        flow.set_valign(gtk::Align::Start);
        flow.set_max_children_per_line(6);
        flow.set_min_children_per_line(2);
        flow.set_row_spacing(16);
        flow.set_column_spacing(16);

        for m in &model.remote_manga {
            let card = gtk::Box::new(gtk::Orientation::Vertical, 6);
            card.set_size_request(150, -1);
            card.add_css_class("kalam-card");
            card.set_cursor_from_name(Some("pointer"));

            let pic = gtk::Picture::new();
            pic.set_size_request(150, 210);
            pic.set_content_fit(gtk::ContentFit::Cover);
            pic.add_css_class("kalam-book-card");
            card.append(&pic);

            if let Some(cover_url) = &m.cover_url {
                let url = cover_url.clone();
                let pic_weak = pic.downgrade();
                crate::tasks::spawn(
                    "Loading comic cover",
                    move |_| -> anyhow::Result<Vec<u8>> {
                        let mut reader = ureq::get(&url)
                            .set("User-Agent", "Mozilla/5.0")
                            .call()?
                            .into_reader();
                        let mut buf = Vec::new();
                        std::io::Read::read_to_end(&mut reader, &mut buf)?;
                        Ok(buf)
                    },
                    |_| {},
                    move |res| {
                        if let Ok(bytes) = res {
                            if let Some(pic) = pic_weak.upgrade() {
                                if let Ok(tex) =
                                    gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from(&bytes))
                                {
                                    pic.set_paintable(Some(&tex));
                                }
                            }
                        }
                    },
                );
            }

            let title_lbl = gtk::Label::new(Some(&m.title));
            title_lbl.set_wrap(true);
            title_lbl.set_max_width_chars(18);
            title_lbl.set_lines(2);
            title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
            title_lbl.add_css_class("kalam-title-small");
            title_lbl.set_halign(gtk::Align::Start);
            card.append(&title_lbl);

            let s = sender.clone();
            let rid = m.remote_id.clone();
            let click = gtk::GestureClick::new();
            click.connect_released(move |_, _, _, _| {
                s.input(ComicsMsg::OpenRemoteManga(rid.clone()));
            });
            card.add_controller(click);

            flow.append(&card);
        }
        root_box.append(&flow);
    }

    match model.view_mode {
        ComicViewMode::Series => {
            if !model.filtered_series.is_empty() {
                let sec_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                if !model.remote_manga.is_empty() {
                    sec_header.set_margin_top(16);
                }
                let title = gtk::Label::new(Some("Comic & Manga Series"));
                title.add_css_class("kalam-title-medium");
                title.set_halign(gtk::Align::Start);
                sec_header.append(&title);
                root_box.append(&sec_header);

                let cover_paths: Vec<PathBuf> = model
                    .filtered_series
                    .iter()
                    .filter_map(|g| g.cover_path.clone())
                    .collect();
                crate::preload::warm_covers(cover_paths, 150, 210);

                let flow = gtk::FlowBox::new();
                flow.set_selection_mode(gtk::SelectionMode::None);
                flow.set_valign(gtk::Align::Start);
                flow.set_max_children_per_line(6);
                flow.set_min_children_per_line(2);
                flow.set_row_spacing(16);
                flow.set_column_spacing(16);

                for group in &model.filtered_series {
                    let card = build_series_card(group, sender);
                    flow.append(&card);
                }
                root_box.append(&flow);
            }

            if !model.filtered_standalone.is_empty() {
                let sec_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                if !model.remote_manga.is_empty() || !model.filtered_series.is_empty() {
                    sec_header.set_margin_top(16);
                }
                let title = gtk::Label::new(Some("Standalone Comics"));
                title.add_css_class("kalam-title-medium");
                title.set_halign(gtk::Align::Start);
                sec_header.append(&title);
                root_box.append(&sec_header);

                let s_click = sender.clone();
                let s_float = sender.clone();
                let grid = build_book_grid(
                    &model.filtered_standalone,
                    move |id| {
                        s_click.input(ComicsMsg::OpenComic(id));
                    },
                    move |id| {
                        s_float.input(ComicsMsg::OpenBookDialog(id));
                    },
                );
                root_box.append(&grid);
            }
        }
        ComicViewMode::AllFiles => {
            if !model.filtered_all.is_empty() {
                if !model.remote_manga.is_empty() {
                    let sec_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    sec_header.set_margin_top(16);
                    let title = gtk::Label::new(Some("All Comic Archives"));
                    title.add_css_class("kalam-title-medium");
                    title.set_halign(gtk::Align::Start);
                    sec_header.append(&title);
                    root_box.append(&sec_header);
                }

                let s_click = sender.clone();
                let s_float = sender.clone();
                let grid = build_book_grid(
                    &model.filtered_all,
                    move |id| {
                        s_click.input(ComicsMsg::OpenComic(id));
                    },
                    move |id| {
                        s_float.input(ComicsMsg::OpenBookDialog(id));
                    },
                );
                root_box.append(&grid);
            }
        }
    }

    if !model.search_query.is_empty() {
        let has_results = match model.view_mode {
            ComicViewMode::Series => {
                !model.filtered_series.is_empty() || !model.filtered_standalone.is_empty()
            }
            ComicViewMode::AllFiles => !model.filtered_all.is_empty(),
        };
        if !has_results && model.remote_manga.is_empty() {
            let empty_search = gtk::Box::new(gtk::Orientation::Vertical, 8);
            empty_search.set_margin_top(48);
            empty_search.set_halign(gtk::Align::Center);
            let lbl = gtk::Label::new(Some(&format!(
                "No comics found matching \"{}\"",
                model.search_query
            )));
            lbl.add_css_class("kalam-subtitle-muted");
            empty_search.append(&lbl);
            root_box.append(&empty_search);
        }
    }

    root_box.upcast()
}

#[relm4::component(pub)]
impl Component for ComicsModel {
    type Init = Arc<Catalog>;
    type Input = ComicsMsg;
    type Output = ComicsOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 16,
            add_css_class: "kalam-page-container",

            // Header Section
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,
                add_css_class: "kalam-page-header",

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 4,
                    set_hexpand: true,

                    gtk::Label {
                        set_label: "Comics & Manga",
                        add_css_class: "kalam-title-large",
                        set_halign: gtk::Align::Start,
                    },
                    gtk::Label {
                        #[watch]
                        set_label: &model.status,
                        add_css_class: "kalam-subtitle-muted",
                        set_halign: gtk::Align::Start,
                    },
                },

                // View Mode Toggle (Series / All Files)
                gtk::Button {
                    #[watch]
                    set_visible: !model.comics.is_empty() && model.active_drawer_series.is_none(),
                    #[watch]
                    set_label: match model.view_mode {
                        ComicViewMode::Series => "View: Series",
                        ComicViewMode::AllFiles => "View: All Files",
                    },
                    add_css_class: "kalam-btn-subtle",
                    #[watch]
                    set_tooltip_text: Some(match model.view_mode {
                        ComicViewMode::Series => "Switch to All Files view",
                        ComicViewMode::AllFiles => "Switch to Series grouping view",
                    }),
                    connect_clicked => ComicsMsg::ToggleViewMode,
                },

                gtk::Button {
                    set_label: "+ Import Comics",
                    add_css_class: "kalam-btn-filled",
                    connect_clicked => ComicsMsg::PickFiles,
                },
            },

            // Main Content Area
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 12,
                set_hexpand: true,
                set_vexpand: true,

                // Search Bar (visible if comics exist and not inside series drawer)
                #[watch]
                set_visible: (!model.comics.is_empty() || !model.remote_manga.is_empty()) && model.active_drawer_series.is_none(),

                gtk::SearchEntry {
                    set_placeholder_text: Some("Search comics by title or author…"),
                    connect_search_changed[sender] => move |entry| {
                        sender.input(ComicsMsg::SearchChanged(entry.text().to_string()));
                    },
                },

                // Comics Display Container
                #[name = "scrolled_window"]
                gtk::ScrolledWindow {
                    set_hexpand: true,
                    set_vexpand: true,
                },
            },
        }
    }

    fn init(
        catalog: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ComicsModel::new(catalog);
        let widgets = view_output!();

        let child = rebuild_comics_view(&model, &sender);
        widgets.scrolled_window.set_child(Some(&child));

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
            ComicsMsg::Refresh => {
                self.reload();
            }
            ComicsMsg::SearchChanged(q) => {
                self.search_query = q;
                self.apply_filter();
            }
            ComicsMsg::ToggleViewMode => {
                self.view_mode = match self.view_mode {
                    ComicViewMode::Series => ComicViewMode::AllFiles,
                    ComicViewMode::AllFiles => ComicViewMode::Series,
                };
                self.active_drawer_series = None;
                widgets.scrolled_window.vadjustment().set_value(0.0);
            }
            ComicsMsg::OpenSeriesDrawer(sname) => {
                self.active_drawer_series = Some(sname);
                widgets.scrolled_window.vadjustment().set_value(0.0);
            }
            ComicsMsg::CloseSeriesDrawer => {
                self.active_drawer_series = None;
                widgets.scrolled_window.vadjustment().set_value(0.0);
            }
            ComicsMsg::PickFiles => {
                let dialog = gtk::FileDialog::builder()
                    .title("Import Comic Archives (CBZ/CBR)")
                    .modal(true)
                    .build();

                let filter = gtk::FileFilter::new();
                filter.set_name(Some("Comic Archives (*.cbz, *.cbr)"));
                filter.add_suffix("cbz");
                filter.add_suffix("cbr");
                filter.add_mime_type("application/x-cbz");
                filter.add_mime_type("application/x-cbr");
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
                    None::<&gtk::gio::Cancellable>,
                    move |res| {
                        if let Ok(files) = res {
                            let mut paths = Vec::new();
                            for i in 0..files.n_items() {
                                if let Some(file) =
                                    files.item(i).and_then(|o| o.downcast::<gtk::gio::File>().ok())
                                {
                                    if let Some(p) = file.path() {
                                        paths.push(p);
                                    }
                                }
                            }
                            s.input(ComicsMsg::FilesChosen(paths));
                        }
                    },
                );
            }
            ComicsMsg::FilesChosen(paths) => {
                let total = paths.len();
                if total == 0 {
                    return;
                }
                let s_progress = sender.input_sender().clone();
                let s_done = sender.input_sender().clone();
                spawn_import(
                    self.service.catalog().clone(),
                    paths,
                    move |done, total, title| {
                        let _ = s_progress.send(ComicsMsg::ImportStep { done, total, title });
                    },
                    move |tally| {
                        let _ = s_done.send(ComicsMsg::ImportFinished(tally));
                    },
                );
            }
            ComicsMsg::ImportStep { done, total, title } => {
                self.status = format!("Importing comic {done}/{total}: {title}…");
            }
            ComicsMsg::ImportFinished(tally) => {
                self.reload();
                self.status = import_summary(&tally);
            }
            ComicsMsg::OpenComic(id) => {
                let _ = sender.output(ComicsOut::OpenComic { book_id: id });
            }
            ComicsMsg::OpenBookDialog(id) => {
                let _ = sender.output(ComicsOut::OpenBookDialog { book_id: id });
            }
            ComicsMsg::OpenRemoteManga(remote_id) => {
                let _ = sender.output(ComicsOut::OpenRemoteManga {
                    source_id: "weebcentral".to_string(),
                    remote_id,
                });
            }
        }

        let child = rebuild_comics_view(self, &sender);
        widgets.scrolled_window.set_child(Some(&child));

        self.update_view(widgets, sender);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn make_test_book(
        id: i64,
        title: &str,
        series: Option<&str>,
        series_index: f32,
        progress: u8,
    ) -> Book {
        Book {
            id,
            uuid: format!("uuid-{}", id),
            title: title.to_string(),
            authors: "Test Author".to_string(),
            series: series.map(|s| s.to_string()),
            description: String::new(),
            format: BookFormat::Cbz,
            file_name: format!("{}.cbz", id),
            file_hash: format!("hash_{}", id),
            cover_name: None,
            added_at: String::new(),
            progress,
            rating: 0,
            publisher: String::new(),
            published: String::new(),
            series_index,
            tags: Vec::new(),
            cover_path: Some(PathBuf::from(format!("/tmp/{}.jpg", id))),
            file_path: PathBuf::from(format!("/tmp/{}.cbz", id)),
        }
    }

    #[test]
    fn test_group_comics_into_series() {
        let books = vec![
            make_test_book(1, "One Piece 02", Some("One Piece"), 2.0, 100),
            make_test_book(2, "One Piece 01", Some("One Piece"), 1.0, 100),
            make_test_book(3, "One Piece 03", Some("One Piece"), 3.0, 45),
            make_test_book(4, "One Piece 04", Some("One Piece"), 4.0, 0),
            make_test_book(5, "Berserk 01", Some("Berserk"), 1.0, 0),
            make_test_book(6, "Standalone Comic", None, 0.0, 10),
        ];

        let (groups, standalone) = group_comics_into_series(&books);

        // Should have 2 series: Berserk and One Piece (alphabetical)
        assert_eq!(groups.len(), 2);
        assert_eq!(groups[0].series_name, "Berserk");
        assert_eq!(groups[1].series_name, "One Piece");

        // Berserk
        assert_eq!(groups[0].total_chapters, 1);
        assert_eq!(groups[0].unread_count, 1);
        assert_eq!(groups[0].next_to_read.as_ref().unwrap().id, 5);

        // One Piece chapters sorted: 1, 2, 3, 4
        assert_eq!(groups[1].total_chapters, 4);
        assert_eq!(groups[1].unread_count, 1); // only Ch 4 is 0%
        let op_ch_ids: Vec<i64> = groups[1].chapters.iter().map(|c| c.id).collect();
        assert_eq!(op_ch_ids, vec![2, 1, 3, 4]);
        // next_to_read should be Ch 3 (progress is 45 < 100)
        assert_eq!(groups[1].next_to_read.as_ref().unwrap().id, 3);

        // Standalone
        assert_eq!(standalone.len(), 1);
        assert_eq!(standalone[0].id, 6);
    }

    #[test]
    fn test_series_all_completed_next_to_read_loops_to_first() {
        let books = vec![
            make_test_book(1, "Naruto 01", Some("Naruto"), 1.0, 100),
            make_test_book(2, "Naruto 02", Some("Naruto"), 2.0, 100),
        ];

        let (groups, _) = group_comics_into_series(&books);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].unread_count, 0);
        // When all completed, next_to_read is first chapter
        assert_eq!(groups[0].next_to_read.as_ref().unwrap().id, 1);
    }
}
