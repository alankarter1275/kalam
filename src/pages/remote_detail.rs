use crate::sources::{RemoteBookDetails, RemoteChapter, SourceManager};
use gtk::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

#[derive(Debug)]
pub enum RemoteDetailOut {
    Back,
    OpenReader {
        source_id: String,
        chapter_id: String,
        title: String,
    },
    OpenBook {
        book_id: i64,
    },
    OpenAuthor {
        source_id: String,
        author: String,
    },
}

#[derive(Debug)]
pub enum RemoteDetailMsg {
    LoadInfo,
    InfoSuccess(RemoteBookDetails, Vec<RemoteChapter>),
    InfoFailed(String),
    AddToLibrary,
    ReadChapter { chapter_id: String, title: String },
    ChapterReady(i64),
    ChapterFailed(String),
    ClickedAuthor,
    OpenExternalUrl(String),
}

pub struct RemoteDetailInit {
    pub manager: Arc<SourceManager>,
    pub catalog: Arc<crate::db::Catalog>,
    pub source_id: String,
    pub remote_id: String,
}

pub struct RemoteDetailModel {
    manager: Arc<SourceManager>,
    catalog: Arc<crate::db::Catalog>,
    source_id: String,
    remote_id: String,
    details: Option<RemoteBookDetails>,
    chapters: Vec<RemoteChapter>,
    status: String,
    is_loading: bool,
    is_in_library: bool,
}

impl RemoteDetailModel {
    pub fn new(init: RemoteDetailInit) -> Self {
        Self {
            manager: init.manager,
            catalog: init.catalog,
            source_id: init.source_id,
            remote_id: init.remote_id,
            details: None,
            chapters: Vec::new(),
            status: "Loading manga details…".to_string(),
            is_loading: true,
            is_in_library: false,
        }
    }
}

#[relm4::component(pub)]
impl Component for RemoteDetailModel {
    type Init = RemoteDetailInit;
    type Input = RemoteDetailMsg;
    type Output = RemoteDetailOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 16,
            add_css_class: "kalam-page-container",

            // ── Top Navigation Bar ──────────────────────────────────────────
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,
                add_css_class: "kalam-page-header",

                gtk::Button {
                    set_icon_name: "go-previous-symbolic",
                    add_css_class: "flat",
                    set_tooltip_text: Some("Back to Browse"),
                    connect_clicked[sender] => move |_| {
                        let _ = sender.output(RemoteDetailOut::Back);
                    },
                },

                gtk::Label {
                    #[watch]
                    set_label: if let Some(d) = &model.details { &d.title } else { "Manga Details" },
                    add_css_class: "kalam-title-large",
                    set_halign: gtk::Align::Start,
                    set_hexpand: true,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                },

                gtk::Button {
                    #[watch]
                    set_label: if model.is_in_library { "✓ In Library" } else { "+ Add to Library" },
                    add_css_class: "kalam-btn-filled",
                    #[watch]
                    set_sensitive: !model.is_loading && model.details.is_some() && !model.is_in_library,
                    connect_clicked => RemoteDetailMsg::AddToLibrary,
                }
            },

            // ── Status Banner (while loading or on error) ───────────────────
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                #[watch]
                set_visible: model.is_loading,

                gtk::Spinner {
                    #[watch]
                    set_spinning: model.is_loading,
                },

                gtk::Label {
                    #[watch]
                    set_label: &model.status,
                    add_css_class: "kalam-subtitle-muted",
                },
            },

            // ── Manga Info Card ─────────────────────────────────────────────
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 20,
                add_css_class: "kalam-card",
                #[watch]
                set_visible: model.details.is_some(),

                // Cover Image
                #[name = "cover_pic"]
                gtk::Picture {
                    set_content_fit: gtk::ContentFit::Cover,
                    set_size_request: (160, 230),
                    add_css_class: "kalam-book-card",
                },

                // Metadata Details
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,
                    set_hexpand: true,

                    gtk::Label {
                        #[watch]
                        set_label: if let Some(d) = &model.details { &d.title } else { "" },
                        add_css_class: "kalam-title-medium",
                        set_halign: gtk::Align::Start,
                        set_wrap: true,
                    },

                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 12,

                        gtk::Button {
                            #[watch]
                            set_label: if let Some(d) = &model.details {
                                if d.author.is_empty() { "Unknown Author" } else { &d.author }
                            } else {
                                ""
                            },
                            add_css_class: "kalam-btn-subtle",
                            connect_clicked => RemoteDetailMsg::ClickedAuthor,
                        },

                        gtk::Label {
                            #[watch]
                            set_label: if let Some(d) = &model.details {
                                &d.status
                            } else {
                                ""
                            },
                            add_css_class: "kalam-badge",
                            set_halign: gtk::Align::Start,
                        },
                    },

                    gtk::ScrolledWindow {
                        set_vexpand: true,
                        set_max_content_height: 120,
                        set_propagate_natural_height: true,

                        gtk::Label {
                            #[watch]
                            set_label: if let Some(d) = &model.details { &d.description } else { "" },
                            set_wrap: true,
                            set_halign: gtk::Align::Start,
                            set_valign: gtk::Align::Start,
                            add_css_class: "kalam-body",
                        },
                    },
                }
            },

            // ── Chapters Section ────────────────────────────────────────────
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                #[watch]
                set_visible: !model.chapters.is_empty(),

                gtk::Label {
                    #[watch]
                    set_label: &format!("Chapters ({})", model.chapters.len()),
                    add_css_class: "kalam-title-medium",
                    set_halign: gtk::Align::Start,
                },
            },

            gtk::ScrolledWindow {
                set_hexpand: true,
                set_vexpand: true,
                #[watch]
                set_visible: !model.chapters.is_empty(),

                #[name = "chapters_box"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 6,
                }
            }
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = RemoteDetailModel::new(init);
        let widgets = view_output!();

        sender.input(RemoteDetailMsg::LoadInfo);

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
            RemoteDetailMsg::LoadInfo => {
                self.is_loading = true;
                self.status = "Loading manga details and chapters…".to_string();

                if let Some(source) = self.manager.get(&self.source_id) {
                    let s = sender.input_sender().clone();
                    let r_id = self.remote_id.clone();

                    crate::tasks::spawn(
                        move |_reporter| {
                            let details = source.get_details(&r_id)?;
                            let chapters = source.get_chapters(&r_id)?;
                            Ok::<_, anyhow::Error>((details, chapters))
                        },
                        |_| {},
                        move |res| match res {
                            Ok((d, c)) => { let _ = s.send(RemoteDetailMsg::InfoSuccess(d, c)); }
                            Err(e)     => { let _ = s.send(RemoteDetailMsg::InfoFailed(e.to_string())); }
                        },
                    );
                } else {
                    self.is_loading = false;
                    self.status = "Source not found.".to_string();
                }
            }

            RemoteDetailMsg::InfoSuccess(details, chapters) => {
                self.is_loading = false;

                // Load cover image in background
                if let (Some(url), Some(source)) = (&details.cover_url, self.manager.get(&self.source_id)) {
                    let u = url.clone();
                    let pic = widgets.cover_pic.clone();
                    crate::tasks::spawn(
                        move |_| source.fetch_image(&u).ok(),
                        |_| {},
                        move |bytes| {
                            if let Some(b) = bytes {
                                let stream = gtk::gio::MemoryInputStream::from_bytes(&gtk::glib::Bytes::from(&b));
                                if let Ok(pixbuf) = gtk::gdk_pixbuf::Pixbuf::from_stream_at_scale(&stream, 160, 230, true, None::<&gtk::gio::Cancellable>) {
                                    pic.set_paintable(Some(&gtk::gdk::Texture::for_pixbuf(&pixbuf)));
                                }
                            }
                        },
                    );
                }

                self.details = Some(details);
                self.chapters = chapters;

                // Clear previous chapters list
                while let Some(child) = widgets.chapters_box.first_child() {
                    widgets.chapters_box.remove(&child);
                }

                let manga_title = self.details.as_ref().map(|d| d.title.clone()).unwrap_or_default();

                // Render styled chapter rows
                for ch in &self.chapters {
                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
                    row.add_css_class("kalam-card");
                    row.set_margin_start(4);
                    row.set_margin_end(4);

                    // Chapter number badge
                    let num_lbl = gtk::Label::new(Some(&format!("Ch. {}", ch.number)));
                    num_lbl.add_css_class("kalam-body-emphasis");
                    num_lbl.set_size_request(80, -1);
                    num_lbl.set_halign(gtk::Align::Start);
                    row.append(&num_lbl);

                    // Chapter Title (or dash if untitled)
                    let title_text = if ch.title.is_empty() {
                        "Untitled".to_string()
                    } else {
                        ch.title.clone()
                    };
                    let title_lbl = gtk::Label::new(Some(&title_text));
                    title_lbl.set_hexpand(true);
                    title_lbl.set_halign(gtk::Align::Start);
                    title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
                    title_lbl.add_css_class("kalam-body");
                    row.append(&title_lbl);

                    // Action button: Internal Reader vs External Web Link
                    if let Some(ref ext_url) = ch.url {
                        let badge = gtk::Label::new(Some("MangaPlus"));
                        badge.add_css_class("kalam-badge");
                        row.append(&badge);

                        let btn = gtk::Button::with_label("Open Web ↗");
                        btn.add_css_class("kalam-btn-tonal");
                        let url_clone = ext_url.clone();
                        let s = sender.clone();
                        btn.connect_clicked(move |_| {
                            s.input(RemoteDetailMsg::OpenExternalUrl(url_clone.clone()));
                        });
                        row.append(&btn);
                    } else {
                        let btn = gtk::Button::with_label("Read");
                        btn.add_css_class("kalam-btn-filled");
                        let ch_id = ch.chapter_id.clone();
                        let reader_title = format!("{} - Ch. {}", manga_title, ch.number);
                        let s = sender.clone();
                        btn.connect_clicked(move |_| {
                            s.input(RemoteDetailMsg::ReadChapter {
                                chapter_id: ch_id.clone(),
                                title: reader_title.clone(),
                            });
                        });
                        row.append(&btn);
                    }

                    widgets.chapters_box.append(&row);
                }
            }

            RemoteDetailMsg::InfoFailed(err) => {
                self.is_loading = false;
                self.status = format!("Failed to load: {}", err);
            }

            RemoteDetailMsg::AddToLibrary => {
                if let Some(details) = &self.details {
                    if self.source_id == "royalroad" {
                        if let Some(dl_mgr) = crate::downloads::DOWNLOAD_MANAGER.get() {
                            dl_mgr.queue_download(&details.title, &self.source_id, &self.remote_id);
                            self.is_in_library = true;
                            self.status = "Queued for background download!".to_string();
                        }
                    } else {
                        let res1 = self.catalog.add_remote_book(details, &self.source_id);
                        let res2 = self.catalog.add_remote_chapters(&self.remote_id, &self.source_id, &self.chapters);
    
                        if let Err(e) = res1 {
                            self.status = format!("Error adding to library: {}", e);
                        } else if let Err(e) = res2 {
                            self.status = format!("Error saving chapters: {}", e);
                        } else {
                            self.is_in_library = true;
                            self.status = "Added to library!".to_string();
                        }
                    }
                }
            }

            RemoteDetailMsg::ReadChapter { chapter_id, title } => {
                if self.source_id == "royalroad" {
                    if let Some(details) = &self.details {
                        if let Ok(books) = self.catalog.list_books(crate::db::SortKey::Added, "") {
                            if let Some(b) = books.into_iter().find(|b| b.title.eq_ignore_ascii_case(&details.title)) {
                                let cache_dir = crate::paths::reader_cache_dir(&b.uuid);
                                let is_stale = match crate::epub_book::OpenBook::open(&b.file_path, &cache_dir) {
                                    Ok(open) => open.chapter_count() < self.chapters.len(),
                                    Err(_) => true,
                                };

                                if is_stale {
                                    let _ = self.catalog.delete_book(b.id);
                                    let _ = std::fs::remove_dir_all(&cache_dir);
                                } else {
                                    let idx = self.chapters.iter().position(|c| c.chapter_id == chapter_id).unwrap_or(0);
                                    let _ = self.catalog.set_reading_progress(b.id, idx, 0.0, self.chapters.len());

                                    // If this chapter in cache is a placeholder, fetch it before opening
                                    if let Ok(open) = crate::epub_book::OpenBook::open(&b.file_path, &cache_dir) {
                                        if let Some(item) = open.spine.get(idx) {
                                            let is_placeholder = open.chapter_body(idx)
                                                .map(|body| body.contains("kalam-remote-placeholder"))
                                                .unwrap_or(false);
                                            if is_placeholder {
                                                self.is_loading = true;
                                                self.status = format!("Loading {}…", title);
                                                let s = sender.input_sender().clone();
                                                let source_mgr = self.manager.clone();
                                                let source_id = self.source_id.clone();
                                                let chap_id = chapter_id.clone();
                                                let chap_title = title.clone();
                                                let path = item.path.clone();
                                                let book_id = b.id;

                                                crate::tasks::spawn(
                                                    move |_| -> anyhow::Result<i64> {
                                                        let source = source_mgr.get(&source_id).ok_or_else(|| anyhow::anyhow!("Source not found"))?;
                                                        let content = source.get_chapter_content(&chap_id)?;
                                                        let html = match content {
                                                            crate::sources::ChapterContent::Html(h) => h,
                                                            _ => return Err(anyhow::anyhow!("Expected HTML chapter")),
                                                        };
                                                        let xhtml = format!(
                                                            r#"<?xml version="1.0" encoding="UTF-8"?>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>{}</title></head>
<body>
<h1>{}</h1>
{}
</body>
</html>"#,
                                                            quick_xml::escape::escape(&chap_title),
                                                            quick_xml::escape::escape(&chap_title),
                                                            html
                                                        );
                                                        let _ = std::fs::write(&path, &xhtml);
                                                        Ok(book_id)
                                                    },
                                                    |_| {},
                                                    move |res| match res {
                                                        Ok(book_id) => { let _ = s.send(RemoteDetailMsg::ChapterReady(book_id)); }
                                                        Err(e) => { let _ = s.send(RemoteDetailMsg::ChapterFailed(e.to_string())); }
                                                    },
                                                );
                                                return;
                                            }
                                        }
                                    }

                                    let _ = sender.output(RemoteDetailOut::OpenBook { book_id: b.id });
                                    return;
                                }
                            }
                        }

                        // Not in library yet (or was stale and cleaned up): fetch target chapter HTML and generate full-skeleton EPUB
                        self.is_loading = true;
                        self.status = format!("Loading {}…", title);
                        let s = sender.input_sender().clone();
                        let source_mgr = self.manager.clone();
                        let source_id = self.source_id.clone();
                        let chap_id = chapter_id.clone();
                        let b_title = details.title.clone();
                        let b_author = details.author.clone();
                        let cover_url = details.cover_url.clone();
                        let catalog = self.catalog.clone();
                        let chapters = self.chapters.clone();
                        let target_idx = chapters.iter().position(|c| c.chapter_id == chap_id).unwrap_or(0);

                        crate::tasks::spawn(
                            move |_| -> anyhow::Result<i64> {
                                let source = source_mgr.get(&source_id).ok_or_else(|| anyhow::anyhow!("Source not found"))?;
                                let content = source.get_chapter_content(&chap_id)?;
                                let target_html = match content {
                                    crate::sources::ChapterContent::Html(h) => h,
                                    _ => return Err(anyhow::anyhow!("Expected HTML chapter")),
                                };
                                let cover_bytes = if let Some(u) = cover_url {
                                    source.fetch_image(&u).ok()
                                } else {
                                    None
                                };
                                let out_dir = std::env::temp_dir().join("kalam_downloads");
                                std::fs::create_dir_all(&out_dir)?;
                                let temp_file = out_dir.join(format!("{}.epub", uuid::Uuid::new_v4()));

                                let mut web_chapters = Vec::with_capacity(chapters.len());
                                for (i, c) in chapters.iter().enumerate() {
                                    let html = if i == target_idx {
                                        target_html.clone()
                                    } else {
                                        format!(
                                            r#"<div class="kalam-remote-placeholder" data-source-id="{}" data-chapter-id="{}"><p>Loading chapter…</p></div>"#,
                                            quick_xml::escape::escape(&source_id),
                                            quick_xml::escape::escape(&c.chapter_id)
                                        )
                                    };
                                    web_chapters.push(crate::epub_writer::WebChapter {
                                        title: c.title.clone(),
                                        html_content: html,
                                    });
                                }

                                crate::epub_writer::generate_epub(&temp_file, &b_title, &b_author, cover_bytes.as_deref(), web_chapters)?;
                                let res = crate::epub::import_epub(&catalog, &temp_file)?;
                                let _ = std::fs::remove_file(&temp_file);
                                let _ = catalog.set_reading_progress(res.book_id, target_idx, 0.0, chapters.len());
                                Ok(res.book_id)
                            },
                            |_| {},
                            move |res| match res {
                                Ok(book_id) => { let _ = s.send(RemoteDetailMsg::ChapterReady(book_id)); }
                                Err(e) => { let _ = s.send(RemoteDetailMsg::ChapterFailed(e.to_string())); }
                            },
                        );
                        return;
                    }
                }

                // Default / Manga: stream images via ComicsReader
                let _ = sender.output(RemoteDetailOut::OpenReader {
                    source_id: self.source_id.clone(),
                    chapter_id,
                    title,
                });
            }

            RemoteDetailMsg::ChapterReady(book_id) => {
                self.is_loading = false;
                self.status.clear();
                let _ = sender.output(RemoteDetailOut::OpenBook { book_id });
            }

            RemoteDetailMsg::ChapterFailed(err) => {
                self.is_loading = false;
                self.status = format!("Failed to open chapter: {err}");
            }

            RemoteDetailMsg::ClickedAuthor => {
                if let Some(d) = &self.details {
                    if !d.author.is_empty() {
                        let _ = sender.output(RemoteDetailOut::OpenAuthor {
                            source_id: self.source_id.clone(),
                            author: d.author.clone(),
                        });
                    }
                }
            }

            RemoteDetailMsg::OpenExternalUrl(url) => {
                // Launch external chapter in system default web browser
                let _ = gtk::glib::spawn_command_line_async(format!("xdg-open '{}'", url));
            }
        }

        self.update_view(widgets, sender);
    }
}
