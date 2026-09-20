//! PDF Reader Page with Zathura-Style Smart Crop & Text Reflow.

use crate::db::Catalog;
use crate::epub_book::ReadingTheme;
use crate::pdf::PdfDocument;
use gtk::gdk;
use gtk::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

#[derive(Clone)]
pub struct PdfReaderInit {
    pub book_id: i64,
    pub catalog: Arc<Catalog>,
}

impl std::fmt::Debug for PdfReaderInit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfReaderInit")
            .field("book_id", &self.book_id)
            .finish()
    }
}

#[derive(Debug)]
pub enum PdfReaderMsg {
    NextPage,
    PrevPage,
    SetPage(usize),
    ZoomIn,
    ZoomOut,
    ResetZoom,
    ToggleSmartCrop,
    ToggleReflow,
    SetTheme(ReadingTheme),
    DecreaseFontSize,
    IncreaseFontSize,
    SetLineHeight(f32),
    Close,
}

#[derive(Debug)]
pub enum PdfReaderOut {
    Close,
}

pub struct PdfReaderModel {
    pub book_id: i64,
    pub catalog: Arc<Catalog>,
    pub title: String,
    pub pdf_doc: Option<Arc<PdfDocument>>,
    pub current_page: usize,
    pub total_pages: usize,
    pub zoom_level: f32,
    pub smart_crop: bool,
    pub reflow_mode: bool,
    /// Reflow was chosen for this page because it has no picture of its
    /// own — the thing the on-screen notice explains (roadmap 2.11).
    pub auto_reflow: bool,
    /// The reader has used the reflow toggle this session, so the
    /// per-page default no longer overrules them.
    pub user_chose_reflow: bool,
    pub font_size: u32,
    pub reading_theme: ReadingTheme,
    pub line_height: f32,
    pub reflowed_paragraphs: Vec<String>,
    pub current_texture: Option<gdk::Texture>,
    pub texture_width: i32,
    pub texture_height: i32,
    pub is_loading: bool,
    pub status_text: String,
}

impl PdfReaderModel {
    pub fn new(init: PdfReaderInit) -> Self {
        let (title, file_path) = match init.catalog.get_book(init.book_id) {
            Ok(Some(book)) => (book.title, Some(book.file_path)),
            _ => ("PDF Reader".to_string(), None),
        };

        let saved_page = match init.catalog.get_reading_progress(init.book_id) {
            Ok(Some((page, _))) if page >= 1 => page,
            _ => 1,
        };

        let mut model = Self {
            book_id: init.book_id,
            catalog: init.catalog,
            title,
            pdf_doc: None,
            current_page: saved_page,
            total_pages: 1,
            zoom_level: 1.0,
            smart_crop: true,
            reflow_mode: false,
            auto_reflow: false,
            user_chose_reflow: false,
            font_size: 16,
            reading_theme: ReadingTheme::Ink,
            line_height: 1.5,
            reflowed_paragraphs: Vec::new(),
            current_texture: None,
            texture_width: 600,
            texture_height: 800,
            is_loading: true,
            status_text: "Loading PDF...".to_string(),
        };

        if let Some(path) = file_path {
            let opened = {
                let _open = crate::timing::measure("pdf_open");
                PdfDocument::open(&path)
            };
            if let Ok(doc) = opened {
                model.total_pages = doc.page_count();
                if saved_page > model.total_pages {
                    model.current_page = 1;
                }
                let doc_arc = Arc::new(doc);
                model.pdf_doc = Some(doc_arc);
                model.is_loading = false;
                model.status_text.clear();

                // Initial render of page
                model.render_current_page();
            } else {
                model.status_text = "Failed to load PDF file".to_string();
                model.is_loading = false;
            }
        } else {
            model.status_text = "Book not found in catalog".to_string();
            model.is_loading = false;
        }

        model
    }

    fn render_current_page(&mut self) {
        let Some(ref pdf_doc) = self.pdf_doc else {
            return;
        };

        let fraction = if self.total_pages > 0 {
            self.current_page as f64 / self.total_pages as f64
        } else {
            0.0
        };
        let _ = self.catalog.set_reading_progress(
            self.book_id,
            self.current_page,
            fraction,
            self.total_pages,
        );
        // refresh_for_book returns (); the old `let _ =` discarded nothing.
        crate::sidecar::refresh_for_book(&self.catalog, self.book_id);

        // Roadmap 2.11: a page with a picture of its own is shown as that
        // picture. A page without one has nothing to render — the old
        // fallback drew a dark grey bar for every line of text — so its
        // text is reflowed instead, and the toolbar says so.
        let has_picture = self
            .pdf_doc
            .as_ref()
            .is_some_and(|doc| doc.page_has_picture(self.current_page));
        if !self.user_chose_reflow {
            self.reflow_mode = !has_picture;
        }
        self.auto_reflow = self.reflow_mode && !has_picture;

        // Two paths, two very different costs: one reads a page's text,
        // the other builds a full-page bitmap. KALAM_TIMING=1 says which
        // one ran and what it took.
        let _page_timer = crate::timing::measure(if self.reflow_mode {
            "pdf_page_reflow"
        } else {
            "pdf_page_image"
        });

        if self.reflow_mode {
            // Extract and reflow text
            if let Ok(raw_text) = pdf_doc.extract_raw_text(self.current_page) {
                self.reflowed_paragraphs = PdfDocument::reflow_text(&raw_text);
            } else {
                self.reflowed_paragraphs = vec!["[No readable text extracted from page]".to_string()];
            }
        } else {
            // Render page image with smart crop
            if let Ok(img) = pdf_doc.render_page_image(self.current_page, self.smart_crop) {
                let rgba_img = img.to_rgba8();
                let (w, h) = (rgba_img.width() as i32, rgba_img.height() as i32);
                self.texture_width = w;
                self.texture_height = h;

                let bytes = glib::Bytes::from(&rgba_img.into_raw());
                let texture = gdk::MemoryTexture::new(
                    w,
                    h,
                    gdk::MemoryFormat::R8g8b8a8,
                    &bytes,
                    (w * 4) as usize,
                );
                self.current_texture = Some(texture.upcast());
            }
        }
    }
}

#[relm4::component(pub)]
impl Component for PdfReaderModel {
    type Init = PdfReaderInit;
    type Input = PdfReaderMsg;
    type Output = PdfReaderOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_hexpand: true,
            set_vexpand: true,
            add_css_class: "kalam-pdf-reader",

            // ── Top Header Control Bar ─────────────────────────────────
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                set_margin_all: 8,
                add_css_class: "kalam-pdf-header",

                gtk::Button {
                    set_icon_name: "go-previous-symbolic",
                    set_tooltip_text: Some("Back"),
                    connect_clicked => PdfReaderMsg::Close,
                },

                gtk::Label {
                    #[watch]
                    set_label: &model.title,
                    add_css_class: "title-4",
                    set_hexpand: true,
                    set_halign: gtk::Align::Start,
                    set_ellipsize: gtk::pango::EllipsizeMode::End,
                },

                // Page Navigation Controls
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 4,
                    add_css_class: "linked",

                    gtk::Button {
                        set_icon_name: "go-previous-symbolic",
                        set_tooltip_text: Some("Previous Page"),
                        connect_clicked => PdfReaderMsg::PrevPage,
                    },

                    gtk::SpinButton {
                        set_range: (1.0, model.total_pages.max(1) as f64),
                        #[watch]
                        set_value: model.current_page as f64,
                        set_numeric: true,
                        set_width_chars: 4,
                        set_tooltip_text: Some("Jump to page"),
                        connect_value_changed[sender] => move |spin| {
                            let page = spin.value() as usize;
                            sender.input(PdfReaderMsg::SetPage(page));
                        },
                    },

                    gtk::Label {
                        #[watch]
                        set_label: &format!("/ {}", model.total_pages),
                        set_margin_start: 2,
                        set_margin_end: 6,
                    },

                    gtk::Button {
                        set_icon_name: "go-next-symbolic",
                        set_tooltip_text: Some("Next Page"),
                        connect_clicked => PdfReaderMsg::NextPage,
                    },
                },

                // Zoom Controls
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 2,
                    add_css_class: "linked",

                    gtk::Button {
                        set_label: "-",
                        set_tooltip_text: Some("Zoom Out"),
                        connect_clicked => PdfReaderMsg::ZoomOut,
                    },

                    gtk::Button {
                        #[watch]
                        set_label: &format!("{}%", (model.zoom_level * 100.0) as u32),
                        set_tooltip_text: Some("Reset Zoom"),
                        connect_clicked => PdfReaderMsg::ResetZoom,
                    },

                    gtk::Button {
                        set_label: "+",
                        set_tooltip_text: Some("Zoom In"),
                        connect_clicked => PdfReaderMsg::ZoomIn,
                    },
                },

                // Toggles: Smart Crop & Text Reflow
                gtk::ToggleButton {
                    set_label: "✂️ Smart Crop",
                    set_tooltip_text: Some("Toggle Zathura-style margins auto-crop"),
                    #[watch]
                    set_active: model.smart_crop,
                    connect_clicked => PdfReaderMsg::ToggleSmartCrop,
                },

                gtk::ToggleButton {
                    set_label: "📄 Text Reflow",
                    set_tooltip_text: Some("Toggle adaptive text reflow engine"),
                    #[watch]
                    set_active: model.reflow_mode,
                    connect_clicked => PdfReaderMsg::ToggleReflow,
                },
            },

            // ── Secondary Toolbar (Active when Reflow Mode is ON) ───────
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,
                set_margin_start: 12,
                set_margin_end: 12,
                set_margin_bottom: 8,
                add_css_class: "kalam-reflow-toolbar",
                #[watch]
                set_visible: model.reflow_mode,

                gtk::Label {
                    // Roadmap 2.11: reflow is a fallback, not a feature the
                    // reader turned on, so it explains itself.
                    #[watch]
                    set_label: if model.auto_reflow {
                        "Reflowed text — this page has no image to show. Turn Text Reflow off for the raw page."
                    } else {
                        ""
                    },
                    #[watch]
                    set_visible: model.auto_reflow,
                    add_css_class: "dim-label",
                },

                gtk::Label {
                    set_label: "Theme:",
                    add_css_class: "dim-label",
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 4,
                    add_css_class: "linked",

                    gtk::Button {
                        set_label: "Ink",
                        connect_clicked => PdfReaderMsg::SetTheme(ReadingTheme::Ink),
                    },
                    gtk::Button {
                        set_label: "Sepia",
                        connect_clicked => PdfReaderMsg::SetTheme(ReadingTheme::Sepia),
                    },
                    gtk::Button {
                        set_label: "Dark",
                        connect_clicked => PdfReaderMsg::SetTheme(ReadingTheme::Dark),
                    },
                },

                gtk::Label {
                    set_label: "Font Size:",
                    add_css_class: "dim-label",
                    set_margin_start: 12,
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 4,
                    add_css_class: "linked",

                    gtk::Button {
                        set_label: "A-",
                        connect_clicked => PdfReaderMsg::DecreaseFontSize,
                    },
                    gtk::Label {
                        #[watch]
                        set_label: &format!("{}px", model.font_size),
                        set_margin_start: 4,
                        set_margin_end: 4,
                    },
                    gtk::Button {
                        set_label: "A+",
                        connect_clicked => PdfReaderMsg::IncreaseFontSize,
                    },
                },

                gtk::Label {
                    set_label: "Line Height:",
                    add_css_class: "dim-label",
                    set_margin_start: 12,
                },

                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 4,
                    add_css_class: "linked",

                    gtk::Button {
                        set_label: "1.2",
                        connect_clicked => PdfReaderMsg::SetLineHeight(1.2),
                    },
                    gtk::Button {
                        set_label: "1.5",
                        connect_clicked => PdfReaderMsg::SetLineHeight(1.5),
                    },
                    gtk::Button {
                        set_label: "1.8",
                        connect_clicked => PdfReaderMsg::SetLineHeight(1.8),
                    },
                },
            },

            // ── Main Viewport Area ─────────────────────────────────────
            gtk::ScrolledWindow {
                set_hexpand: true,
                set_vexpand: true,
                add_css_class: "kalam-pdf-viewport",

                // Option 1: Page Image Mode (Reflow OFF)
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_halign: gtk::Align::Center,
                    set_valign: gtk::Align::Center,
                    set_hexpand: true,
                    set_vexpand: true,
                    #[watch]
                    set_visible: !model.reflow_mode,

                    gtk::Label {
                        #[watch]
                        set_label: &model.status_text,
                        #[watch]
                        set_visible: !model.status_text.is_empty(),
                    },

                    #[name = "pdf_picture"]
                    gtk::Picture {
                        set_can_shrink: true,
                        set_content_fit: gtk::ContentFit::Contain,
                        #[watch]
                        set_paintable: model.current_texture.as_ref(),
                        #[watch]
                        set_visible: model.current_texture.is_some(),
                        #[watch]
                        set_size_request: (
                            (model.texture_width as f32 * model.zoom_level) as i32,
                            (model.texture_height as f32 * model.zoom_level) as i32,
                        ),
                    },
                },

                // Option 2: Text Reflow Mode (Reflow ON)
                #[name = "reflow_box"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 16,
                    set_margin_all: 24,
                    set_hexpand: true,
                    set_halign: gtk::Align::Center,
                    set_size_request: (720, -1),
                    #[watch]
                    set_visible: model.reflow_mode,

                    gtk::Label {
                        #[watch]
                        set_label: if model.reflowed_paragraphs.is_empty() {
                            "No reflowed text available for this page."
                        } else {
                            ""
                        },
                        #[watch]
                        set_visible: model.reflowed_paragraphs.is_empty(),
                    },

                    // Paragraphs rendering container
                    #[name = "reflow_container"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_spacing: (16.0 * model.line_height) as i32,
                    },
                },
            },
        }
    }

    fn init(
        init: Self::Init,
        root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = Self::new(init);
        let widgets = view_output!();

        // Keyboard shortcuts controller
        let key = gtk::EventControllerKey::new();
        let s = sender.clone();
        key.connect_key_pressed(move |_, keyval, _keycode, _state| {
            use gtk::gdk::Key;
            match keyval {
                Key::Left | Key::Page_Up | Key::BackSpace | Key::k | Key::K => {
                    s.input(PdfReaderMsg::PrevPage);
                    gtk::glib::Propagation::Stop
                }
                Key::Right | Key::Page_Down | Key::space | Key::j | Key::J => {
                    s.input(PdfReaderMsg::NextPage);
                    gtk::glib::Propagation::Stop
                }
                Key::plus | Key::equal | Key::KP_Add => {
                    s.input(PdfReaderMsg::ZoomIn);
                    gtk::glib::Propagation::Stop
                }
                Key::minus | Key::KP_Subtract => {
                    s.input(PdfReaderMsg::ZoomOut);
                    gtk::glib::Propagation::Stop
                }
                Key::r | Key::R => {
                    s.input(PdfReaderMsg::ToggleReflow);
                    gtk::glib::Propagation::Stop
                }
                Key::c | Key::C => {
                    s.input(PdfReaderMsg::ToggleSmartCrop);
                    gtk::glib::Propagation::Stop
                }
                Key::Escape | Key::q | Key::Q => {
                    s.input(PdfReaderMsg::Close);
                    gtk::glib::Propagation::Stop
                }
                _ => gtk::glib::Propagation::Proceed,
            }
        });
        root.add_controller(key);
        root.set_can_focus(true);
        root.grab_focus();

        // Update reflow container paragraphs dynamically when building view
        populate_reflow_paragraphs(&widgets.reflow_container, &model);

        ComponentParts { model, widgets }
    }

    fn update(
        &mut self,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            PdfReaderMsg::NextPage => {
                if self.current_page < self.total_pages {
                    self.current_page += 1;
                    self.render_current_page();
                }
            }
            PdfReaderMsg::PrevPage => {
                if self.current_page > 1 {
                    self.current_page -= 1;
                    self.render_current_page();
                }
            }
            PdfReaderMsg::SetPage(page) => {
                let clamped = page.clamp(1, self.total_pages);
                if clamped != self.current_page {
                    self.current_page = clamped;
                    self.render_current_page();
                }
            }
            PdfReaderMsg::ZoomIn => {
                self.zoom_level = (self.zoom_level + 0.25).min(3.0);
            }
            PdfReaderMsg::ZoomOut => {
                self.zoom_level = (self.zoom_level - 0.25).max(0.5);
            }
            PdfReaderMsg::ResetZoom => {
                self.zoom_level = 1.0;
            }
            PdfReaderMsg::ToggleSmartCrop => {
                self.smart_crop = !self.smart_crop;
                self.render_current_page();
            }
            PdfReaderMsg::ToggleReflow => {
                self.reflow_mode = !self.reflow_mode;
                self.user_chose_reflow = true;
                self.render_current_page();
            }
            PdfReaderMsg::SetTheme(theme) => {
                self.reading_theme = theme;
            }
            PdfReaderMsg::DecreaseFontSize => {
                self.font_size = self.font_size.saturating_sub(2).max(12);
            }
            PdfReaderMsg::IncreaseFontSize => {
                self.font_size = (self.font_size + 2).min(36);
            }
            PdfReaderMsg::SetLineHeight(height) => {
                self.line_height = height;
            }
            PdfReaderMsg::Close => {
                let _ = sender.output(PdfReaderOut::Close);
            }
        }
    }

    fn post_view(&self, widgets: &Self::Widgets) {
        if self.reflow_mode {
            let container = &widgets.reflow_box;
            container.remove_css_class("kalam-theme-light");
            container.remove_css_class("kalam-theme-sepia");
            container.remove_css_class("kalam-theme-dark");
            container.remove_css_class("kalam-theme-ink");

            let theme_class = match self.reading_theme {
                ReadingTheme::Light => "kalam-theme-light",
                ReadingTheme::Sepia => "kalam-theme-sepia",
                ReadingTheme::Dark => "kalam-theme-dark",
                ReadingTheme::Ink => "kalam-theme-ink",
            };
            container.add_css_class(theme_class);

            populate_reflow_paragraphs(&widgets.reflow_container, self);
        }
    }
}

/// Helper function to populate the reflow container with GTK labels formatted with font size & markup.
fn populate_reflow_paragraphs(container: &gtk::Box, model: &PdfReaderModel) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    for para in &model.reflowed_paragraphs {
        let label = gtk::Label::new(None);
        let escaped = glib::markup_escape_text(para);
        let size_pt = model.font_size * 1024;
        let markup = format!("<span size='{size_pt}'>{escaped}</span>");
        label.set_markup(&markup);
        label.set_wrap(true);
        label.set_wrap_mode(gtk::pango::WrapMode::Word);
        label.set_selectable(true);
        label.set_xalign(0.0);
        label.set_margin_bottom((12.0 * model.line_height) as i32);

        container.append(&label);
    }
}
