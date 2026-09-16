//! Comics & Manga hub page: browse local CBZ/CBR comic archives and launch reader.

use crate::db::{Catalog, SortKey};
use crate::models::{Book, BookFormat};
use crate::pages::all_books::{import_summary, spawn_import, ImportTally};
use crate::service::LibraryService;
use crate::widgets::book_row::build_book_grid;
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

#[derive(Debug)]
pub enum ComicsMsg {
    #[allow(dead_code)]
    Refresh,
    SearchChanged(String),
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
    remote_manga: Vec<crate::sources::RemoteBookDetails>,
    filtered: Vec<Book>,
    search_query: String,
    status: String,
}

impl ComicsModel {
    pub fn new(catalog: Arc<Catalog>) -> Self {
        let service = LibraryService::new(catalog);
        let mut model = Self {
            service,
            comics: Vec::new(),
            remote_manga: Vec::new(),
            filtered: Vec::new(),
            search_query: String::new(),
            status: String::new(),
        };
        model.reload();
        model
    }

    fn reload(&mut self) {
        if let Ok(all_books) = self.service.catalog().list_books(SortKey::Added, "") {
            self.comics = all_books
                .into_iter()
                .filter(|b| matches!(b.format, BookFormat::Cbz | BookFormat::Cbr))
                .collect();
        } else {
            self.comics = Vec::new();
        }
        self.remote_manga = self.service.catalog().list_remote_books(Some("weebcentral")).unwrap_or_default();
        self.apply_filter();
        self.update_status();
    }

    fn apply_filter(&mut self) {
        if self.search_query.is_empty() {
            self.filtered = self.comics.clone();
        } else {
            let query = self.search_query.to_lowercase();
            self.filtered = self
                .comics
                .iter()
                .filter(|b| {
                    b.title.to_lowercase().contains(&query)
                        || b.authors.to_lowercase().contains(&query)
                })
                .cloned()
                .collect();
        }
    }

    fn update_status(&mut self) {
        let count = self.comics.len();
        let m_count = self.remote_manga.len();
        self.status = if count == 0 && m_count == 0 {
            "No comics or manga in library yet".to_string()
        } else {
            let mut parts = Vec::new();
            if count > 0 {
                parts.push(format!("{count} local comic{}", if count == 1 { "" } else { "s" }));
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

fn rebuild_comics_view(model: &ComicsModel, sender: &ComponentSender<ComicsModel>) -> gtk::Widget {
    if model.comics.is_empty() && model.remote_manga.is_empty() {
        return build_empty_state(sender).upcast();
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
                                if let Ok(tex) = gtk::gdk::Texture::from_bytes(&gtk::glib::Bytes::from(&bytes)) {
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

    if !model.filtered.is_empty() {
        if !model.remote_manga.is_empty() {
            let sec_header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            sec_header.set_margin_top(16);
            let title = gtk::Label::new(Some("Local Comic Archives"));
            title.add_css_class("kalam-title-medium");
            title.set_halign(gtk::Align::Start);
            sec_header.append(&title);
            root_box.append(&sec_header);
        }

        let s_click = sender.clone();
        let s_float = sender.clone();
        let grid = build_book_grid(
            &model.filtered,
            move |id| {
                s_click.input(ComicsMsg::OpenComic(id));
            },
            move |id| {
                s_float.input(ComicsMsg::OpenBookDialog(id));
            },
        );
        root_box.append(&grid);
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

                // Search Bar (visible if comics exist)
                #[watch]
                set_visible: !model.comics.is_empty() || !model.remote_manga.is_empty(),

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
                let s_progress = sender.clone();
                let s_done = sender.clone();
                spawn_import(
                    self.service.catalog().clone(),
                    paths,
                    move |done, total, title| {
                        s_progress.input(ComicsMsg::ImportStep { done, total, title });
                    },
                    move |tally| {
                        s_done.input(ComicsMsg::ImportFinished(tally));
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
