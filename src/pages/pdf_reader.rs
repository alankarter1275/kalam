//! PDF Reader Page with MuPDF rasterization, unified chrome, outline sidebar, and bounded caching.

use crate::db::Catalog;
use crate::pdf::{PdfDocument, PdfTocEntry};
use gtk::gdk;
use gtk::prelude::*;
use relm4::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone)]
pub struct PdfReaderInit {
    pub catalog: Arc<Catalog>,
    pub book_id: i64,
}

impl std::fmt::Debug for PdfReaderInit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PdfReaderInit")
            .field("book_id", &self.book_id)
            .finish()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfViewMode {
    Paged,
    Continuous,
}

#[derive(Debug)]
pub enum PdfReaderMsg {
    NextPage,
    PrevPage,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    ToggleContinuousMode,
    ToggleSmartCrop,
    ToggleSidebar,
    CloseSidebar,
    JumpToToc(usize),
    EscapeKey,
    Close,
    PageRendered {
        page: usize,
        texture: gdk::Texture,
        width: i32,
        height: i32,
    },
    TopEdgeHover(bool),
    BottomEdgeHover(bool),
    LeftEdgeHover(bool),
    SidebarHover(bool),
    BackHideTimerTick(u64),
    BottomHideTimerTick(u64),
    SidebarCloseTimerTick(u64),
    UserScrolled,
    ScrollDelta(f64),
    UpdateScrollPage(usize),
}

#[derive(Debug)]
pub enum PdfReaderOut {
    Close,
}

pub struct PdfReaderModel {
    pub book_id: i64,
    pub catalog: Arc<Catalog>,
    pub title: String,
    pub author: String,
    pub cover_path: Option<PathBuf>,
    pub file_path: Option<PathBuf>,
    pub current_page: usize,
    pub total_pages: usize,
    pub zoom_level: f32,
    pub view_mode: PdfViewMode,
    pub smart_crop: bool,
    pub show_back_button: bool,
    pub show_bottom_pill: bool,
    pub show_sidebar: bool,
    pub mouse_in_top_edge: bool,
    pub mouse_in_bottom_edge: bool,
    pub mouse_in_left_edge: bool,
    pub mouse_in_sidebar: bool,
    pub back_hide_seq: u64,
    pub bottom_hide_seq: u64,
    pub sidebar_close_seq: u64,
    pub toc_entries: Vec<PdfTocEntry>,
    pub textures: HashMap<usize, (gdk::Texture, i32, i32)>,
    pub pending_loads: HashSet<usize>,
    pub arrow_step: f32,
    #[allow(dead_code)]
    pub is_loading: bool,
    pub status_text: String,
    pub page_pictures: HashMap<usize, gtk::Picture>,
    pub paged_picture: Option<gtk::Picture>,
    pub paged_loading_box: Option<gtk::Box>,
    pub paged_label: Option<gtk::Label>,
}

impl PdfReaderModel {
    pub fn new(init: PdfReaderInit) -> Self {
        let (title, author, cover_path, file_path) = match init.catalog.get_book(init.book_id) {
            Ok(Some(book)) => (
                book.title,
                book.authors,
                book.cover_path,
                Some(book.file_path),
            ),
            _ => ("PDF Document".to_string(), String::new(), None, None),
        };

        let saved_page = match init.catalog.get_reading_progress(init.book_id) {
            Ok(Some((page, _))) if page >= 1 => page,
            _ => 1,
        };

        let arrow_step = init.catalog.get_pref_i64("reader.arrow_step", 45) as f32;

        let mut model = Self {
            book_id: init.book_id,
            catalog: init.catalog,
            title,
            author,
            cover_path,
            file_path: file_path.clone(),
            current_page: saved_page,
            total_pages: 1,
            zoom_level: 1.0,
            view_mode: PdfViewMode::Paged,
            smart_crop: true,
            show_back_button: true,
            show_bottom_pill: true,
            show_sidebar: false,
            mouse_in_top_edge: false,
            mouse_in_bottom_edge: false,
            mouse_in_left_edge: false,
            mouse_in_sidebar: false,
            back_hide_seq: 0,
            bottom_hide_seq: 0,
            sidebar_close_seq: 0,
            toc_entries: Vec::new(),
            textures: HashMap::new(),
            pending_loads: HashSet::new(),
            arrow_step: arrow_step.clamp(10.0, 200.0),
            is_loading: true,
            status_text: "Opening PDF...".to_string(),
            page_pictures: HashMap::new(),
            paged_picture: None,
            paged_loading_box: None,
            paged_label: None,
        };

        if let Some(ref path) = file_path {
            if let Ok(doc) = PdfDocument::open(path) {
                model.total_pages = doc.page_count();
                if saved_page > model.total_pages {
                    model.current_page = 1;
                }
                model.toc_entries = doc.outlines().unwrap_or_default();
                model.is_loading = false;
                model.status_text.clear();
            } else {
                model.status_text = "Failed to open PDF document".to_string();
                model.is_loading = false;
            }
        } else {
            model.status_text = "Book file not found".to_string();
            model.is_loading = false;
        }

        model
    }

    /// Save current reading progress to catalog and sidecar.
    pub fn save_progress(&self) {
        if self.total_pages == 0 {
            return;
        }
        let fraction = self.current_page as f64 / self.total_pages as f64;
        let _ = self.catalog.set_reading_progress(
            self.book_id,
            self.current_page,
            fraction,
            self.total_pages,
        );
        crate::sidecar::refresh_for_book(&self.catalog, self.book_id);
    }

    /// Schedule autohide of floating back button.
    pub fn schedule_back_hide(&mut self, sender: &ComponentSender<Self>) {
        self.back_hide_seq = self.back_hide_seq.wrapping_add(1);
        let seq = self.back_hide_seq;
        let tx = sender.input_sender().clone();
        glib::timeout_add_local_once(Duration::from_millis(2500), move || {
            let _ = tx.send(PdfReaderMsg::BackHideTimerTick(seq));
        });
    }

    /// Schedule autohide of floating bottom pill.
    pub fn schedule_bottom_hide(&mut self, sender: &ComponentSender<Self>) {
        self.bottom_hide_seq = self.bottom_hide_seq.wrapping_add(1);
        let seq = self.bottom_hide_seq;
        let tx = sender.input_sender().clone();
        glib::timeout_add_local_once(Duration::from_millis(3000), move || {
            let _ = tx.send(PdfReaderMsg::BottomHideTimerTick(seq));
        });
    }

    /// Schedule close of left sidebar (350ms debounce like Zen browser).
    pub fn schedule_sidebar_close(&mut self, sender: &ComponentSender<Self>) {
        self.sidebar_close_seq = self.sidebar_close_seq.wrapping_add(1);
        let seq = self.sidebar_close_seq;
        let tx = sender.input_sender().clone();
        glib::timeout_add_local_once(Duration::from_millis(350), move || {
            let _ = tx.send(PdfReaderMsg::SidebarCloseTimerTick(seq));
        });
    }

    /// Evict distant page textures and trigger background renders for current ± 4 pages.
    pub fn trigger_loads(&mut self, sender: &ComponentSender<Self>) {
        let Some(ref path) = self.file_path else {
            return;
        };

        // Generous memory cache: retain current_page ± 8 pages in memory (up to 17 pages, ~80-100 MB)
        let cur = self.current_page as isize;
        let mut evicted = Vec::new();
        self.textures.retain(|&p, _| {
            let keep = (p as isize - cur).abs() <= 8;
            if !keep {
                evicted.push(p);
            }
            keep
        });

        // Clear paintable for evicted pages so GPU/cairo memory is freed
        for p in evicted {
            if let Some(pic) = self.page_pictures.get(&p) {
                pic.set_paintable(None::<&gdk::Texture>);
                pic.add_css_class("kalam-pdf-placeholder");
            }
        }

        // Determine which pages need pre-rendering: current ± 4 pages
        let min_page = (self.current_page.saturating_sub(4)).max(1);
        let max_page = (self.current_page + 4).min(self.total_pages);

        // Prioritize current_page, then neighbors in load order
        let mut load_order = vec![self.current_page];
        for d in 1..=4 {
            if self.current_page + d <= max_page {
                load_order.push(self.current_page + d);
            }
            if self.current_page > d && self.current_page - d >= min_page {
                load_order.push(self.current_page - d);
            }
        }

        let scale = (1.5 * self.zoom_level).clamp(0.5, 3.5);
        let smart_crop = self.smart_crop;

        for page in load_order {
            if self.textures.contains_key(&page) || self.pending_loads.contains(&page) {
                continue;
            }
            self.pending_loads.insert(page);
            let s = sender.clone();
            let p_buf = path.clone();

            crate::tasks::spawn(
                "Rendering PDF page",
                move |_| -> anyhow::Result<(usize, i32, i32, usize, Vec<u8>)> {
                    let doc = PdfDocument::open(&p_buf)?;
                    let rendered = doc.render_page_rgba(page, scale, smart_crop)?;
                    Ok((
                        rendered.page_num,
                        rendered.width,
                        rendered.height,
                        rendered.stride,
                        rendered.samples,
                    ))
                },
                |_| {},
                move |res| {
                    if let Ok((p, w, h, stride, samples)) = res {
                        let bytes = glib::Bytes::from_owned(samples);
                        let texture = gdk::MemoryTexture::new(
                            w,
                            h,
                            gdk::MemoryFormat::R8g8b8a8,
                            &bytes,
                            stride,
                        );
                        let _ = s.input_sender().send(PdfReaderMsg::PageRendered {
                            page: p,
                            texture: texture.upcast(),
                            width: w,
                            height: h,
                        });
                    }
                },
            );
        }
    }

    /// Build the viewport container widget for current view mode (Paged vs Continuous).
    pub fn build_viewport_widget(&mut self) -> gtk::Widget {
        match self.view_mode {
            PdfViewMode::Paged => {
                let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
                container.set_halign(gtk::Align::Center);
                container.set_valign(gtk::Align::Center);
                container.set_hexpand(true);
                container.set_vexpand(true);
                container.set_margin_all(16);

                let pic = gtk::Picture::new();
                pic.set_can_shrink(true);
                pic.set_content_fit(gtk::ContentFit::Contain);
                pic.set_halign(gtk::Align::Center);
                pic.set_valign(gtk::Align::Center);
                pic.add_css_class("kalam-pdf-page-image");

                let loading_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
                loading_box.set_halign(gtk::Align::Center);
                loading_box.set_valign(gtk::Align::Center);

                let spinner = gtk::Spinner::new();
                spinner.start();
                spinner.set_size_request(32, 32);
                loading_box.append(&spinner);

                let label = gtk::Label::new(Some(&format!(
                    "Rendering page {}...",
                    self.current_page
                )));
                label.add_css_class("dim-label");
                loading_box.append(&label);

                if let Some((tex, w, h)) = self.textures.get(&self.current_page) {
                    pic.set_paintable(Some(tex));
                    let target_w = ((*w as f32) * self.zoom_level) as i32;
                    let target_h = ((*h as f32) * self.zoom_level) as i32;
                    pic.set_size_request(target_w, target_h);
                    pic.set_visible(true);
                    loading_box.set_visible(false);
                } else {
                    pic.set_visible(false);
                    loading_box.set_visible(true);
                }

                container.append(&pic);
                container.append(&loading_box);

                self.paged_picture = Some(pic);
                self.paged_loading_box = Some(loading_box);
                self.paged_label = Some(label);
                self.page_pictures.clear();

                container.upcast()
            }
            PdfViewMode::Continuous => {
                let container = gtk::Box::new(gtk::Orientation::Vertical, 20);
                container.set_halign(gtk::Align::Center);
                container.set_valign(gtk::Align::Start);
                container.set_hexpand(true);
                container.set_vexpand(false);
                container.set_margin_top(24);
                container.set_margin_bottom(80);

                self.page_pictures.clear();
                self.paged_picture = None;
                self.paged_loading_box = None;
                self.paged_label = None;

                for p in 1..=self.total_pages {
                    let page_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
                    page_box.set_halign(gtk::Align::Center);

                    let pic = gtk::Picture::new();
                    pic.set_can_shrink(true);
                    pic.set_content_fit(gtk::ContentFit::Contain);
                    pic.set_halign(gtk::Align::Center);

                    if let Some((tex, w, h)) = self.textures.get(&p) {
                        pic.set_paintable(Some(tex));
                        let target_w = ((*w as f32) * self.zoom_level) as i32;
                        let target_h = ((*h as f32) * self.zoom_level) as i32;
                        pic.set_size_request(target_w, target_h);
                        pic.add_css_class("kalam-pdf-page-image");
                    } else {
                        let placeholder_w = (600.0 * self.zoom_level) as i32;
                        let placeholder_h = (800.0 * self.zoom_level) as i32;
                        pic.set_size_request(placeholder_w, placeholder_h);
                        pic.add_css_class("kalam-pdf-placeholder");
                    }

                    page_box.append(&pic);
                    self.page_pictures.insert(p, pic);

                    container.append(&page_box);
                }

                container.upcast()
            }
        }
    }

    /// Fast, in-place update for Paged mode when page changes or renders.
    pub fn update_paged_view(&self) {
        let (Some(pic), Some(loading_box), Some(label)) = (
            &self.paged_picture,
            &self.paged_loading_box,
            &self.paged_label,
        ) else {
            return;
        };

        if let Some((tex, w, h)) = self.textures.get(&self.current_page) {
            pic.set_paintable(Some(tex));
            let target_w = ((*w as f32) * self.zoom_level) as i32;
            let target_h = ((*h as f32) * self.zoom_level) as i32;
            pic.set_size_request(target_w, target_h);
            pic.set_visible(true);
            loading_box.set_visible(false);
        } else {
            pic.set_visible(false);
            loading_box.set_visible(true);
            label.set_label(&format!("Rendering page {}...", self.current_page));
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
        gtk::Overlay {
            add_css_class: "kalam-reader-overlay",
            set_hexpand: true,
            set_vexpand: true,

            // ── 1. Base Layer: Document Viewport ───────────────────────
            #[name = "viewport_scroll"]
            gtk::ScrolledWindow {
                set_hexpand: true,
                set_vexpand: true,
                add_css_class: "kalam-reader-viewport",
            },

            // ── 2. Overlay: Dim Backdrop for Left Sidebar ──────────────
            add_overlay = &gtk::Button {
                add_css_class: "kalam-reader-dim",
                #[watch]
                set_visible: model.show_sidebar,
                set_hexpand: true,
                set_vexpand: true,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Fill,
                connect_clicked => PdfReaderMsg::CloseSidebar,
            },

            // ── 3. Overlay: Left Edge Hover Strip (Zen Browser style)
            add_overlay = &gtk::Box {
                add_css_class: "kalam-reader-hover-edge",
                add_css_class: "kalam-reader-hover-edge-left",
                set_width_request: 20,
                set_hexpand: false,
                set_vexpand: true,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Fill,
            },

            // ── 4. Overlay: Floating Back Button (Top-Left) ────────────
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_back_button && !model.show_sidebar,
                set_transition_type: gtk::RevealerTransitionType::SlideDown,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Start,

                #[name = "back_dock"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    add_css_class: "kalam-reader-back-dock",
                    set_margin_start: 18,
                    set_margin_top: 18,

                    gtk::Button {
                        set_child: Some(&crate::icons::labelled("go-previous-symbolic", 16, "Library", 6)),
                        add_css_class: "kalam-reader-back",
                        set_tooltip_text: Some("Back to Library (Esc / Backspace)"),
                        connect_clicked => PdfReaderMsg::Close,
                    },
                },
            },

            // ── 5. Overlay: Floating Bottom Pill (Bottom-Center) ───────
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_bottom_pill,
                set_transition_type: gtk::RevealerTransitionType::SlideUp,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::End,

                #[name = "bottom_dock"]
                gtk::Box {
                    add_css_class: "kalam-reader-bottom-dock",
                    set_margin_bottom: 20,

                    gtk::Box {
                        add_css_class: "kalam-reader-bottom-pill",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 4,
                        set_valign: gtk::Align::Center,

                        // Previous Page
                        gtk::Button {
                            set_icon_name: "go-previous-symbolic",
                            add_css_class: "kalam-reader-pill-nav",
                            set_tooltip_text: Some("Previous Page (Left / Page Up)"),
                            connect_clicked => PdfReaderMsg::PrevPage,
                        },

                        // Page info & progress
                        gtk::Box {
                            add_css_class: "kalam-reader-pill-info",
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 4,
                            set_valign: gtk::Align::Center,

                            gtk::Label {
                                add_css_class: "kalam-reader-pill-pages",
                                #[watch]
                                set_label: &format!(
                                    "{}/{} ({:.0}%)",
                                    model.current_page,
                                    model.total_pages,
                                    if model.total_pages > 0 {
                                        (model.current_page as f64 / model.total_pages as f64) * 100.0
                                    } else {
                                        0.0
                                    }
                                ),
                            },
                        },

                        // Next Page
                        gtk::Button {
                            set_icon_name: "go-next-symbolic",
                            add_css_class: "kalam-reader-pill-nav",
                            set_tooltip_text: Some("Next Page (Right / Page Down / Space)"),
                            connect_clicked => PdfReaderMsg::NextPage,
                        },

                        gtk::Separator {
                            set_orientation: gtk::Orientation::Vertical,
                            set_margin_start: 4,
                            set_margin_end: 4,
                        },

                        // Zoom Out
                        gtk::Button {
                            set_label: "-",
                            add_css_class: "kalam-reader-pill-nav",
                            set_tooltip_text: Some("Zoom Out (-)"),
                            connect_clicked => PdfReaderMsg::ZoomOut,
                        },

                        // Zoom Percentage / Reset
                        gtk::Button {
                            #[watch]
                            set_label: &format!("{}%", (model.zoom_level * 100.0).round() as i32),
                            add_css_class: "flat",
                            set_tooltip_text: Some("Reset Zoom to 100% (0)"),
                            connect_clicked => PdfReaderMsg::ResetZoom,
                        },

                        // Zoom In
                        gtk::Button {
                            set_label: "+",
                            add_css_class: "kalam-reader-pill-nav",
                            set_tooltip_text: Some("Zoom In (+)"),
                            connect_clicked => PdfReaderMsg::ZoomIn,
                        },

                        gtk::Separator {
                            set_orientation: gtk::Orientation::Vertical,
                            set_margin_start: 4,
                            set_margin_end: 4,
                        },

                        // Mode toggle: Paged vs Continuous
                        gtk::Button {
                            #[watch]
                            set_icon_name: if model.view_mode == PdfViewMode::Paged {
                                "view-continuous-symbolic"
                            } else {
                                "view-paged-symbolic"
                            },
                            add_css_class: "kalam-reader-pill-nav",
                            #[watch]
                            set_tooltip_text: Some(if model.view_mode == PdfViewMode::Paged {
                                "Continuous Vertical Scroll (M)"
                            } else {
                                "Paged View (M)"
                            }),
                            connect_clicked => PdfReaderMsg::ToggleContinuousMode,
                        },

                        // Smart Crop Toggle
                        gtk::Button {
                            set_icon_name: "edit-cut-symbolic",
                            add_css_class: "kalam-reader-pill-nav",
                            #[watch]
                            set_tooltip_text: Some(if model.smart_crop {
                                "Smart Crop: ON (C)"
                            } else {
                                "Smart Crop: OFF (C)"
                            }),
                            connect_clicked => PdfReaderMsg::ToggleSmartCrop,
                        },

                        // Table of Contents Sidebar Toggle
                        gtk::Button {
                            set_icon_name: "sidebar-show-symbolic",
                            add_css_class: "kalam-reader-pill-nav",
                            set_tooltip_text: Some("Table of Contents / Outlines (T)"),
                            connect_clicked => PdfReaderMsg::ToggleSidebar,
                        },
                    },
                },
            },

            // ── 6. Overlay: Slide-in Table of Contents Sidebar ─────────
            add_overlay = &gtk::Revealer {
                add_css_class: "kalam-reader-sidebar-shell",
                add_css_class: "kalam-reader-sidebar-shell-left",
                #[watch]
                set_reveal_child: model.show_sidebar,
                set_transition_type: gtk::RevealerTransitionType::SlideRight,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Fill,

                #[name = "left_sidebar_box"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_width_request: 320,
                    add_css_class: "kalam-reader-sidebar",
                    add_css_class: "kalam-reader-sidebar-left",

                    // 1. Book Head (Cover + Title + Author + Progress)
                    gtk::Box {
                        add_css_class: "kalam-reader-book-head",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 12,

                        #[name = "cover_host"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            add_css_class: "kalam-reader-cover-slot",
                            set_width_request: 48,
                        },

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 4,
                            set_hexpand: true,

                            gtk::Label {
                                add_css_class: "kalam-reader-book-title",
                                add_css_class: "kalam-title-serif",
                                #[watch]
                                set_label: &model.title,
                                set_halign: gtk::Align::Start,
                                set_wrap: true,
                                set_wrap_mode: gtk::pango::WrapMode::WordChar,
                                set_ellipsize: gtk::pango::EllipsizeMode::End,
                                set_lines: 2,
                                set_xalign: 0.0,
                            },

                            gtk::Label {
                                add_css_class: "dim-label",
                                #[watch]
                                set_label: &model.author,
                                set_halign: gtk::Align::Start,
                                set_ellipsize: gtk::pango::EllipsizeMode::End,
                                set_xalign: 0.0,
                            },

                            gtk::ProgressBar {
                                add_css_class: "kalam-reader-progress",
                                set_show_text: false,
                                #[watch]
                                set_fraction: if model.total_pages > 0 {
                                    model.current_page as f64 / model.total_pages as f64
                                } else {
                                    0.0
                                },
                            },
                        },
                    },

                    // 2. TOC Header
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        add_css_class: "kalam-reader-sidebar-head",
                        set_margin_start: 14,
                        set_margin_end: 14,
                        set_margin_top: 10,
                        set_margin_bottom: 6,

                        gtk::Label {
                            set_label: "Table of Contents",
                            add_css_class: "kalam-reader-sidebar-title",
                            set_hexpand: true,
                            set_halign: gtk::Align::Start,
                        },

                        gtk::Button {
                            set_icon_name: "window-close-symbolic",
                            set_tooltip_text: Some("Close (Esc)"),
                            add_css_class: "flat",
                            connect_clicked => PdfReaderMsg::CloseSidebar,
                        },
                    },

                    // 3. Outlines List
                    gtk::ScrolledWindow {
                        set_hexpand: true,
                        set_vexpand: true,

                        #[name = "toc_list_box"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 2,
                            set_margin_all: 8,
                        },
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
        let mut model = Self::new(init);
        let widgets = view_output!();

        // 1. Populate Cover in Sidebar Book Head
        let cover_w = crate::widgets::book_row::cover_widget(model.cover_path.as_deref(), 48, 70);
        widgets.cover_host.append(&cover_w);

        // 2. Populate Sidebar TOC
        populate_toc_list(
            &widgets.toc_list_box,
            &model.toc_entries,
            model.current_page,
            model.total_pages,
            &sender,
        );

        // 3. Keyboard navigation controller
        let tx = sender.input_sender().clone();
        let key = gtk::EventControllerKey::new();
        let tx_key = tx.clone();
        key.connect_key_pressed(move |_, keyval, _keycode, _state| {
            use gtk::gdk::Key;
            match keyval {
                Key::Escape => {
                    let _ = tx_key.send(PdfReaderMsg::EscapeKey);
                    gtk::glib::Propagation::Stop
                }
                Key::BackSpace | Key::q | Key::Q => {
                    let _ = tx_key.send(PdfReaderMsg::Close);
                    gtk::glib::Propagation::Stop
                }
                Key::Left | Key::Page_Up | Key::h | Key::H => {
                    let _ = tx_key.send(PdfReaderMsg::PrevPage);
                    gtk::glib::Propagation::Stop
                }
                Key::Right | Key::Page_Down | Key::space | Key::l | Key::L => {
                    let _ = tx_key.send(PdfReaderMsg::NextPage);
                    gtk::glib::Propagation::Stop
                }
                Key::Up | Key::k | Key::K => {
                    let _ = tx_key.send(PdfReaderMsg::ScrollDelta(-1.0));
                    gtk::glib::Propagation::Stop
                }
                Key::Down | Key::j | Key::J => {
                    let _ = tx_key.send(PdfReaderMsg::ScrollDelta(1.0));
                    gtk::glib::Propagation::Stop
                }
                Key::plus | Key::equal | Key::KP_Add => {
                    let _ = tx_key.send(PdfReaderMsg::ZoomIn);
                    gtk::glib::Propagation::Stop
                }
                Key::minus | Key::KP_Subtract => {
                    let _ = tx_key.send(PdfReaderMsg::ZoomOut);
                    gtk::glib::Propagation::Stop
                }
                Key::_0 | Key::KP_0 => {
                    let _ = tx_key.send(PdfReaderMsg::ResetZoom);
                    gtk::glib::Propagation::Stop
                }
                Key::m | Key::M => {
                    let _ = tx_key.send(PdfReaderMsg::ToggleContinuousMode);
                    gtk::glib::Propagation::Stop
                }
                Key::c | Key::C => {
                    let _ = tx_key.send(PdfReaderMsg::ToggleSmartCrop);
                    gtk::glib::Propagation::Stop
                }
                Key::t | Key::T => {
                    let _ = tx_key.send(PdfReaderMsg::ToggleSidebar);
                    gtk::glib::Propagation::Stop
                }
                _ => gtk::glib::Propagation::Proceed,
            }
        });
        root.add_controller(key);
        root.set_can_focus(true);
        root.grab_focus();

        // 4. Edge hover motion controller on root (Top, Bottom, Left)
        let root_motion = gtk::EventControllerMotion::new();
        let tx_rm = tx.clone();
        let root_clone = root.clone();
        let was_top = std::rc::Rc::new(std::cell::Cell::new(false));
        let was_bottom = std::rc::Rc::new(std::cell::Cell::new(false));
        let was_left = std::rc::Rc::new(std::cell::Cell::new(false));
        let was_top_clone = was_top.clone();
        let was_bottom_clone = was_bottom.clone();
        let was_left_clone = was_left.clone();

        root_motion.connect_motion(move |_, x, y| {
            let h = root_clone.height() as f64;
            let top = y < 50.0;
            let bottom = y > (h - 60.0) && h > 60.0;
            let left = x < 20.0;

            if top != was_top_clone.get() {
                was_top_clone.set(top);
                let _ = tx_rm.send(PdfReaderMsg::TopEdgeHover(top));
            }
            if bottom != was_bottom_clone.get() {
                was_bottom_clone.set(bottom);
                let _ = tx_rm.send(PdfReaderMsg::BottomEdgeHover(bottom));
            }
            if left != was_left_clone.get() {
                was_left_clone.set(left);
                let _ = tx_rm.send(PdfReaderMsg::LeftEdgeHover(left));
            }
        });

        let tx_leave = tx.clone();
        root_motion.connect_leave(move |_| {
            if was_top.get() {
                was_top.set(false);
                let _ = tx_leave.send(PdfReaderMsg::TopEdgeHover(false));
            }
            if was_bottom.get() {
                was_bottom.set(false);
                let _ = tx_leave.send(PdfReaderMsg::BottomEdgeHover(false));
            }
            if was_left.get() {
                was_left.set(false);
                let _ = tx_leave.send(PdfReaderMsg::LeftEdgeHover(false));
            }
        });
        root.add_controller(root_motion);

        // 5. Hover state on floating back dock
        let back_motion = gtk::EventControllerMotion::new();
        let tx_bd = tx.clone();
        back_motion.connect_enter(move |_, _, _| {
            let _ = tx_bd.send(PdfReaderMsg::TopEdgeHover(true));
        });
        let tx_bdl = tx.clone();
        back_motion.connect_leave(move |_| {
            let _ = tx_bdl.send(PdfReaderMsg::TopEdgeHover(false));
        });
        widgets.back_dock.add_controller(back_motion);

        // 6. Hover state on floating bottom dock
        let bottom_motion = gtk::EventControllerMotion::new();
        let tx_bm = tx.clone();
        bottom_motion.connect_enter(move |_, _, _| {
            let _ = tx_bm.send(PdfReaderMsg::BottomEdgeHover(true));
        });
        let tx_bml = tx.clone();
        bottom_motion.connect_leave(move |_| {
            let _ = tx_bml.send(PdfReaderMsg::BottomEdgeHover(false));
        });
        widgets.bottom_dock.add_controller(bottom_motion);

        // 7. Hover state on Left Sidebar
        let sidebar_motion = gtk::EventControllerMotion::new();
        let tx_sm = tx.clone();
        sidebar_motion.connect_enter(move |_, _, _| {
            let _ = tx_sm.send(PdfReaderMsg::SidebarHover(true));
        });
        let tx_sml = tx.clone();
        sidebar_motion.connect_leave(move |_| {
            let _ = tx_sml.send(PdfReaderMsg::SidebarHover(false));
        });
        widgets.left_sidebar_box.add_controller(sidebar_motion);

        // 8. Scroll controller to autohide controls on scrolling
        let scroll_ctrl = gtk::EventControllerScroll::new(
            gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::HORIZONTAL,
        );
        let tx_sc = tx.clone();
        scroll_ctrl.connect_scroll(move |_, _, _| {
            let _ = tx_sc.send(PdfReaderMsg::UserScrolled);
            gtk::glib::Propagation::Proceed
        });
        widgets.viewport_scroll.add_controller(scroll_ctrl);

        // 9. Track continuous scroll position changes to update current_page
        let vadj = widgets.viewport_scroll.vadjustment();
        let tx_vadj = tx.clone();
        vadj.connect_value_changed(move |adj| {
            let max = (adj.upper() - adj.page_size()).max(1.0);
            let ratio = (adj.value() / max).clamp(0.0, 1.0);
            let _ = tx_vadj.send(PdfReaderMsg::UpdateScrollPage(ratio as usize));
        });

        // 11. Click / tap navigation: left quarter = prev, right quarter = next, center = toggle bottom pill
        let click = gtk::GestureClick::new();
        let tx_click = tx.clone();
        click.connect_released(move |gesture, _, x, _| {
            let Some(widget) = gesture.widget() else {
                return;
            };
            let width = widget.width() as f64;
            if width > 0.0 {
                let fraction = x / width;
                if fraction < 0.22 {
                    let _ = tx_click.send(PdfReaderMsg::PrevPage);
                } else if fraction > 0.78 {
                    let _ = tx_click.send(PdfReaderMsg::NextPage);
                } else {
                    let _ = tx_click.send(PdfReaderMsg::BottomEdgeHover(true));
                }
            }
        });
        widgets.viewport_scroll.add_controller(click);

        // Initial child and render triggering
        let child = model.build_viewport_widget();
        widgets.viewport_scroll.set_child(Some(&child));

        model.trigger_loads(&sender);

        // Schedule initial hide of back button and bottom pill after 3.5s
        model.schedule_back_hide(&sender);
        model.schedule_bottom_hide(&sender);

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
            PdfReaderMsg::NextPage => {
                if self.current_page < self.total_pages {
                    self.current_page += 1;
                    self.save_progress();
                    self.trigger_loads(&sender);
                    if self.view_mode == PdfViewMode::Paged {
                        self.update_paged_view();
                    }
                }
            }
            PdfReaderMsg::PrevPage => {
                if self.current_page > 1 {
                    self.current_page -= 1;
                    self.save_progress();
                    self.trigger_loads(&sender);
                    if self.view_mode == PdfViewMode::Paged {
                        self.update_paged_view();
                    }
                }
            }
            PdfReaderMsg::ZoomIn => {
                self.zoom_level = (self.zoom_level + 0.2).min(3.0);
                self.textures.clear();
                self.pending_loads.clear();
                self.trigger_loads(&sender);
                let child = self.build_viewport_widget();
                widgets.viewport_scroll.set_child(Some(&child));
            }
            PdfReaderMsg::ZoomOut => {
                self.zoom_level = (self.zoom_level - 0.2).max(0.4);
                self.textures.clear();
                self.pending_loads.clear();
                self.trigger_loads(&sender);
                let child = self.build_viewport_widget();
                widgets.viewport_scroll.set_child(Some(&child));
            }
            PdfReaderMsg::ResetZoom => {
                self.zoom_level = 1.0;
                self.textures.clear();
                self.pending_loads.clear();
                self.trigger_loads(&sender);
                let child = self.build_viewport_widget();
                widgets.viewport_scroll.set_child(Some(&child));
            }
            PdfReaderMsg::ToggleContinuousMode => {
                self.view_mode = match self.view_mode {
                    PdfViewMode::Paged => PdfViewMode::Continuous,
                    PdfViewMode::Continuous => PdfViewMode::Paged,
                };
                let child = self.build_viewport_widget();
                widgets.viewport_scroll.set_child(Some(&child));
                self.trigger_loads(&sender);
            }
            PdfReaderMsg::ToggleSmartCrop => {
                self.smart_crop = !self.smart_crop;
                self.textures.clear();
                self.pending_loads.clear();
                self.trigger_loads(&sender);
                let child = self.build_viewport_widget();
                widgets.viewport_scroll.set_child(Some(&child));
            }
            PdfReaderMsg::ToggleSidebar => {
                self.show_sidebar = !self.show_sidebar;
                if self.show_sidebar {
                    self.show_back_button = false;
                    populate_toc_list(
                        &widgets.toc_list_box,
                        &self.toc_entries,
                        self.current_page,
                        self.total_pages,
                        &sender,
                    );
                }
            }
            PdfReaderMsg::CloseSidebar => {
                self.show_sidebar = false;
                self.mouse_in_sidebar = false;
                self.mouse_in_left_edge = false;
            }
            PdfReaderMsg::JumpToToc(page) => {
                self.show_sidebar = false;
                self.mouse_in_sidebar = false;
                self.mouse_in_left_edge = false;
                let clamped = page.clamp(1, self.total_pages);
                if clamped != self.current_page {
                    self.current_page = clamped;
                    self.save_progress();
                    self.trigger_loads(&sender);
                    if self.view_mode == PdfViewMode::Paged {
                        self.update_paged_view();
                    } else {
                        let vadj = widgets.viewport_scroll.vadjustment();
                        let target_y = if self.total_pages > 1 && vadj.upper() > 0.0 {
                            (clamped.saturating_sub(1) as f64) * (vadj.upper() / self.total_pages as f64)
                        } else {
                            0.0
                        };
                        vadj.set_value(
                            target_y.clamp(vadj.lower(), (vadj.upper() - vadj.page_size()).max(0.0)),
                        );
                    }
                }
            }
            PdfReaderMsg::EscapeKey => {
                if self.show_sidebar {
                    self.show_sidebar = false;
                    self.mouse_in_sidebar = false;
                    self.mouse_in_left_edge = false;
                } else {
                    let _ = sender.output(PdfReaderOut::Close);
                }
            }
            PdfReaderMsg::Close => {
                let _ = sender.output(PdfReaderOut::Close);
            }
            PdfReaderMsg::PageRendered {
                page,
                texture,
                width,
                height,
            } => {
                self.pending_loads.remove(&page);
                self.textures.insert(page, (texture.clone(), width, height));

                // Smooth in-place picture updates without demolishing or rebuilding widgets
                if self.view_mode == PdfViewMode::Paged {
                    if page == self.current_page {
                        self.update_paged_view();
                    }
                } else if let Some(pic) = self.page_pictures.get(&page) {
                    let target_w = ((width as f32) * self.zoom_level) as i32;
                    let target_h = ((height as f32) * self.zoom_level) as i32;
                    pic.set_paintable(Some(&texture));
                    pic.set_size_request(target_w, target_h);
                    pic.remove_css_class("kalam-pdf-placeholder");
                    pic.add_css_class("kalam-pdf-page-image");
                }
            }
            PdfReaderMsg::TopEdgeHover(hovering) => {
                self.mouse_in_top_edge = hovering;
                if hovering {
                    if !self.show_sidebar {
                        self.show_back_button = true;
                    }
                } else if self.show_back_button {
                    self.schedule_back_hide(&sender);
                }
            }
            PdfReaderMsg::BottomEdgeHover(hovering) => {
                self.mouse_in_bottom_edge = hovering;
                if hovering {
                    self.show_bottom_pill = true;
                } else if self.show_bottom_pill {
                    self.schedule_bottom_hide(&sender);
                }
            }
            PdfReaderMsg::LeftEdgeHover(hovering) => {
                self.mouse_in_left_edge = hovering;
                if hovering {
                    let was_hidden = !self.show_sidebar;
                    self.show_sidebar = true;
                    self.show_back_button = false;
                    if was_hidden {
                        populate_toc_list(
                            &widgets.toc_list_box,
                            &self.toc_entries,
                            self.current_page,
                            self.total_pages,
                            &sender,
                        );
                    }
                } else if !self.mouse_in_sidebar && self.show_sidebar {
                    self.schedule_sidebar_close(&sender);
                }
            }
            PdfReaderMsg::SidebarHover(hovering) => {
                self.mouse_in_sidebar = hovering;
                if hovering {
                    self.show_sidebar = true;
                    self.show_back_button = false;
                } else if !self.mouse_in_left_edge && self.show_sidebar {
                    self.schedule_sidebar_close(&sender);
                }
            }
            PdfReaderMsg::BackHideTimerTick(seq) => {
                if seq == self.back_hide_seq && !self.mouse_in_top_edge {
                    self.show_back_button = false;
                }
            }
            PdfReaderMsg::BottomHideTimerTick(seq) => {
                if seq == self.bottom_hide_seq && !self.mouse_in_bottom_edge {
                    self.show_bottom_pill = false;
                }
            }
            PdfReaderMsg::SidebarCloseTimerTick(seq) => {
                if seq == self.sidebar_close_seq && !self.mouse_in_left_edge && !self.mouse_in_sidebar {
                    self.show_sidebar = false;
                }
            }
            PdfReaderMsg::UserScrolled => {
                if !self.mouse_in_top_edge {
                    self.show_back_button = false;
                }
                if !self.mouse_in_bottom_edge {
                    self.show_bottom_pill = false;
                }
                if !self.mouse_in_left_edge && !self.mouse_in_sidebar {
                    self.show_sidebar = false;
                }
            }
            PdfReaderMsg::ScrollDelta(dir) => {
                if self.view_mode == PdfViewMode::Continuous {
                    let vadj = widgets.viewport_scroll.vadjustment();
                    let step = (self.arrow_step as f64) * dir;
                    let target = (vadj.value() + step)
                        .clamp(vadj.lower(), (vadj.upper() - vadj.page_size()).max(0.0));
                    vadj.set_value(target);
                } else if dir > 0.0 {
                    let _ = sender.input_sender().send(PdfReaderMsg::NextPage);
                } else {
                    let _ = sender.input_sender().send(PdfReaderMsg::PrevPage);
                }
            }
            PdfReaderMsg::UpdateScrollPage(_ratio_page) => {
                if self.view_mode == PdfViewMode::Continuous && self.total_pages > 1 {
                    let vadj = widgets.viewport_scroll.vadjustment();
                    let max = (vadj.upper() - vadj.page_size()).max(1.0);
                    let ratio = (vadj.value() / max).clamp(0.0, 1.0);
                    let target = 1 + (ratio * (self.total_pages - 1) as f64).round() as usize;
                    if target != self.current_page {
                        self.current_page = target;
                        self.save_progress();
                        self.trigger_loads(&sender);
                    }
                }
            }
        }

        self.update_view(widgets, sender);
    }
}

/// Helper function to populate the sidebar outline list.
fn populate_toc_list(
    container: &gtk::Box,
    entries: &[PdfTocEntry],
    current_page: usize,
    total_pages: usize,
    sender: &ComponentSender<PdfReaderModel>,
) {
    while let Some(child) = container.first_child() {
        container.remove(&child);
    }

    if entries.is_empty() {
        let empty_lbl = gtk::Label::new(Some("No document outlines found."));
        empty_lbl.add_css_class("dim-label");
        empty_lbl.set_margin_all(16);
        container.append(&empty_lbl);

        for p in 1..=total_pages.min(100) {
            let btn = gtk::Button::new();
            btn.add_css_class("kalam-reader-toc-item");
            if p == current_page {
                btn.add_css_class("active");
            }
            let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let lbl = gtk::Label::new(Some(&format!("Page {}", p)));
            lbl.set_hexpand(true);
            lbl.set_halign(gtk::Align::Start);
            row_box.append(&lbl);
            btn.set_child(Some(&row_box));

            let tx = sender.input_sender().clone();
            btn.connect_clicked(move |_| {
                let _ = tx.send(PdfReaderMsg::JumpToToc(p));
            });
            container.append(&btn);
        }
        return;
    }

    for entry in entries {
        let btn = gtk::Button::new();
        btn.add_css_class("kalam-reader-toc-item");
        if entry.page == current_page {
            btn.add_css_class("active");
        }

        btn.set_margin_start(entry.depth as i32 * 14);

        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let title_lbl = gtk::Label::new(Some(&entry.title));
        title_lbl.set_halign(gtk::Align::Start);
        title_lbl.set_hexpand(true);
        title_lbl.set_wrap(true);
        title_lbl.set_wrap_mode(gtk::pango::WrapMode::WordChar);
        title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title_lbl.set_lines(2);
        title_lbl.set_xalign(0.0);
        row_box.append(&title_lbl);

        let page_lbl = gtk::Label::new(Some(&format!("{}", entry.page)));
        page_lbl.add_css_class("dim-label");
        page_lbl.set_halign(gtk::Align::End);
        row_box.append(&page_lbl);

        btn.set_child(Some(&row_box));

        let p = entry.page;
        let tx = sender.input_sender().clone();
        btn.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::JumpToToc(p));
        });

        container.append(&btn);
    }
}
