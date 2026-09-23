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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfViewMode {
    Paged,
    Continuous,
}

#[derive(Debug)]
pub enum PdfReaderMsg {
    NextPage,
    PrevPage,
    SetPage(usize),
    ZoomIn,
    ZoomOut,
    ResetZoom,
    ToggleContinuousMode,
    ToggleSmartCrop,
    ToggleSidebar,
    CloseSidebar,
    JumpToToc(usize),
    Close,
    PageRendered {
        page: usize,
        texture: gdk::Texture,
        width: i32,
        height: i32,
    },
    PointerMoved,
    HideChromeTimerTick,
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
    pub file_path: Option<PathBuf>,
    pub current_page: usize,
    pub total_pages: usize,
    pub zoom_level: f32,
    pub view_mode: PdfViewMode,
    pub smart_crop: bool,
    pub show_chrome: bool,
    pub show_sidebar: bool,
    pub toc_entries: Vec<PdfTocEntry>,
    pub textures: HashMap<usize, (gdk::Texture, i32, i32)>,
    pub pending_loads: HashSet<usize>,
    pub arrow_step: f32,
    #[allow(dead_code)]
    pub is_loading: bool,
    pub status_text: String,
    pub chrome_hide_timer: Option<glib::SourceId>,
    pub mouse_in_controls: bool,
}

impl PdfReaderModel {
    pub fn new(init: PdfReaderInit) -> Self {
        let (title, file_path) = match init.catalog.get_book(init.book_id) {
            Ok(Some(book)) => (book.title, Some(book.file_path)),
            _ => ("PDF Document".to_string(), None),
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
            file_path: file_path.clone(),
            current_page: saved_page,
            total_pages: 1,
            zoom_level: 1.0,
            view_mode: PdfViewMode::Paged,
            smart_crop: true,
            show_chrome: true,
            show_sidebar: false,
            toc_entries: Vec::new(),
            textures: HashMap::new(),
            pending_loads: HashSet::new(),
            arrow_step: arrow_step.clamp(10.0, 200.0),
            is_loading: true,
            status_text: "Opening PDF...".to_string(),
            chrome_hide_timer: None,
            mouse_in_controls: false,
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

    /// Schedule autohide of floating controls after 3.5s of inactivity.
    pub fn schedule_chrome_hide(&mut self, sender: ComponentSender<Self>) {
        if let Some(timer) = self.chrome_hide_timer.take() {
            timer.remove();
        }
        let s = sender.clone();
        let source_id = glib::timeout_add_local_once(Duration::from_millis(3500), move || {
            s.input(PdfReaderMsg::HideChromeTimerTick);
        });
        self.chrome_hide_timer = Some(source_id);
    }

    /// Evict distant page textures and trigger background renders for current ± 2 pages.
    pub fn trigger_loads(&mut self, sender: &ComponentSender<Self>) {
        let Some(ref path) = self.file_path else {
            return;
        };

        // Bounded memory eviction: keep only pages within current_page - 2 ..= current_page + 2
        let cur = self.current_page as isize;
        self.textures.retain(|&p, _| (p as isize - cur).abs() <= 2);

        // Determine which pages need rendering: current ± 2 pages
        let min_page = (self.current_page.saturating_sub(2)).max(1);
        let max_page = (self.current_page + 2).min(self.total_pages);

        // Always render current_page first, then immediate neighbors
        let mut load_order = vec![self.current_page];
        for d in 1..=2 {
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
                        s.input(PdfReaderMsg::PageRendered {
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

            // ── 3. Overlay: Floating Back Chip (Top-Left) ──────────────
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_chrome,
                set_transition_type: gtk::RevealerTransitionType::SlideDown,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Start,

                #[name = "back_dock"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                    add_css_class: "kalam-reader-back-dock",
                    set_margin_start: 18,
                    set_margin_top: 18,

                    gtk::Button {
                        set_icon_name: "go-previous-symbolic",
                        add_css_class: "kalam-reader-back",
                        set_tooltip_text: Some("Back to Library (Esc / Backspace)"),
                        connect_clicked => PdfReaderMsg::Close,
                    },

                    gtk::Label {
                        #[watch]
                        set_label: &model.title,
                        set_ellipsize: gtk::pango::EllipsizeMode::End,
                        set_max_width_chars: 40,
                        add_css_class: "kalam-reader-back-title",
                    },
                },
            },

            // ── 4. Overlay: Floating Bottom Pill (Bottom-Center) ───────
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_chrome,
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
                            set_label: "✂️",
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

            // ── 5. Overlay: Slide-in Table of Contents Sidebar ─────────
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_sidebar,
                set_transition_type: gtk::RevealerTransitionType::SlideRight,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Fill,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_width_request: 320,
                    add_css_class: "kalam-reader-sidebar",
                    add_css_class: "kalam-reader-sidebar-left",

                    // Sidebar Header
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        add_css_class: "kalam-reader-sidebar-head",
                        set_margin_all: 12,

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

                    // Sidebar Outlines List
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

        // Keyboard navigation controller
        let key = gtk::EventControllerKey::new();
        let s_key = sender.clone();
        key.connect_key_pressed(move |_, keyval, _keycode, _state| {
            use gtk::gdk::Key;
            match keyval {
                Key::Escape | Key::BackSpace | Key::q | Key::Q => {
                    s_key.input(PdfReaderMsg::Close);
                    gtk::glib::Propagation::Stop
                }
                Key::Left | Key::Page_Up | Key::h | Key::H => {
                    s_key.input(PdfReaderMsg::PrevPage);
                    gtk::glib::Propagation::Stop
                }
                Key::Right | Key::Page_Down | Key::space | Key::l | Key::L => {
                    s_key.input(PdfReaderMsg::NextPage);
                    gtk::glib::Propagation::Stop
                }
                Key::Up | Key::k | Key::K => {
                    s_key.input(PdfReaderMsg::ScrollDelta(-1.0));
                    gtk::glib::Propagation::Stop
                }
                Key::Down | Key::j | Key::J => {
                    s_key.input(PdfReaderMsg::ScrollDelta(1.0));
                    gtk::glib::Propagation::Stop
                }
                Key::plus | Key::equal | Key::KP_Add => {
                    s_key.input(PdfReaderMsg::ZoomIn);
                    gtk::glib::Propagation::Stop
                }
                Key::minus | Key::KP_Subtract => {
                    s_key.input(PdfReaderMsg::ZoomOut);
                    gtk::glib::Propagation::Stop
                }
                Key::_0 | Key::KP_0 => {
                    s_key.input(PdfReaderMsg::ResetZoom);
                    gtk::glib::Propagation::Stop
                }
                Key::m | Key::M => {
                    s_key.input(PdfReaderMsg::ToggleContinuousMode);
                    gtk::glib::Propagation::Stop
                }
                Key::c | Key::C => {
                    s_key.input(PdfReaderMsg::ToggleSmartCrop);
                    gtk::glib::Propagation::Stop
                }
                Key::t | Key::T => {
                    s_key.input(PdfReaderMsg::ToggleSidebar);
                    gtk::glib::Propagation::Stop
                }
                _ => gtk::glib::Propagation::Proceed,
            }
        });
        root.add_controller(key);
        root.set_can_focus(true);
        root.grab_focus();

        // Pointer motion controller to reveal controls on movement
        let motion = gtk::EventControllerMotion::new();
        let s_motion = sender.clone();
        motion.connect_motion(move |_, _, _| {
            s_motion.input(PdfReaderMsg::PointerMoved);
        });
        root.add_controller(motion);

        // Track hover state on bottom dock to avoid autohiding while interacting
        let bottom_motion = gtk::EventControllerMotion::new();
        let s_bm = sender.clone();
        bottom_motion.connect_enter(move |_, _, _| {
            s_bm.input(PdfReaderMsg::PointerMoved);
        });
        widgets.bottom_dock.add_controller(bottom_motion);

        // Track continuous scroll position changes to update current_page
        let vadj = widgets.viewport_scroll.vadjustment();
        let s_vadj = sender.clone();
        vadj.connect_value_changed(move |adj| {
            let max = (adj.upper() - adj.page_size()).max(1.0);
            let ratio = (adj.value() / max).clamp(0.0, 1.0);
            s_vadj.input(PdfReaderMsg::UpdateScrollPage(ratio as usize));
        });

        // Click / tap navigation: left quarter = prev, right quarter = next, center = reveal controls
        let click = gtk::GestureClick::new();
        let s_click = sender.clone();
        click.connect_released(move |gesture, _, x, _| {
            let Some(widget) = gesture.widget() else {
                return;
            };
            let width = widget.width() as f64;
            if width > 0.0 {
                let fraction = x / width;
                if fraction < 0.22 {
                    s_click.input(PdfReaderMsg::PrevPage);
                } else if fraction > 0.78 {
                    s_click.input(PdfReaderMsg::NextPage);
                } else {
                    s_click.input(PdfReaderMsg::PointerMoved);
                }
            }
        });
        widgets.viewport_scroll.add_controller(click);

        // Initial child and render triggering
        let child = rebuild_viewport_widget(&model);
        widgets.viewport_scroll.set_child(Some(&child));

        model.trigger_loads(&sender);
        model.schedule_chrome_hide(sender.clone());

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
                    let child = rebuild_viewport_widget(self);
                    widgets.viewport_scroll.set_child(Some(&child));
                }
            }
            PdfReaderMsg::PrevPage => {
                if self.current_page > 1 {
                    self.current_page -= 1;
                    self.save_progress();
                    self.trigger_loads(&sender);
                    let child = rebuild_viewport_widget(self);
                    widgets.viewport_scroll.set_child(Some(&child));
                }
            }
            PdfReaderMsg::SetPage(page) => {
                let clamped = page.clamp(1, self.total_pages);
                if clamped != self.current_page {
                    self.current_page = clamped;
                    self.save_progress();
                    self.trigger_loads(&sender);
                    let child = rebuild_viewport_widget(self);
                    widgets.viewport_scroll.set_child(Some(&child));
                }
            }
            PdfReaderMsg::ZoomIn => {
                self.zoom_level = (self.zoom_level + 0.2).min(3.0);
                self.textures.clear();
                self.pending_loads.clear();
                self.trigger_loads(&sender);
                let child = rebuild_viewport_widget(self);
                widgets.viewport_scroll.set_child(Some(&child));
            }
            PdfReaderMsg::ZoomOut => {
                self.zoom_level = (self.zoom_level - 0.2).max(0.4);
                self.textures.clear();
                self.pending_loads.clear();
                self.trigger_loads(&sender);
                let child = rebuild_viewport_widget(self);
                widgets.viewport_scroll.set_child(Some(&child));
            }
            PdfReaderMsg::ResetZoom => {
                self.zoom_level = 1.0;
                self.textures.clear();
                self.pending_loads.clear();
                self.trigger_loads(&sender);
                let child = rebuild_viewport_widget(self);
                widgets.viewport_scroll.set_child(Some(&child));
            }
            PdfReaderMsg::ToggleContinuousMode => {
                self.view_mode = match self.view_mode {
                    PdfViewMode::Paged => PdfViewMode::Continuous,
                    PdfViewMode::Continuous => PdfViewMode::Paged,
                };
                let child = rebuild_viewport_widget(self);
                widgets.viewport_scroll.set_child(Some(&child));
            }
            PdfReaderMsg::ToggleSmartCrop => {
                self.smart_crop = !self.smart_crop;
                self.textures.clear();
                self.pending_loads.clear();
                self.trigger_loads(&sender);
                let child = rebuild_viewport_widget(self);
                widgets.viewport_scroll.set_child(Some(&child));
            }
            PdfReaderMsg::ToggleSidebar => {
                self.show_sidebar = !self.show_sidebar;
                if self.show_sidebar {
                    self.show_chrome = true;
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
            }
            PdfReaderMsg::JumpToToc(page) => {
                self.show_sidebar = false;
                let clamped = page.clamp(1, self.total_pages);
                if clamped != self.current_page {
                    self.current_page = clamped;
                    self.save_progress();
                    self.trigger_loads(&sender);
                    let child = rebuild_viewport_widget(self);
                    widgets.viewport_scroll.set_child(Some(&child));
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
                self.textures.insert(page, (texture, width, height));
                if page == self.current_page || self.view_mode == PdfViewMode::Continuous {
                    let child = rebuild_viewport_widget(self);
                    widgets.viewport_scroll.set_child(Some(&child));
                }
            }
            PdfReaderMsg::PointerMoved => {
                self.show_chrome = true;
                self.schedule_chrome_hide(sender.clone());
            }
            PdfReaderMsg::HideChromeTimerTick => {
                if !self.mouse_in_controls && !self.show_sidebar {
                    self.show_chrome = false;
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
                    sender.input(PdfReaderMsg::NextPage);
                } else {
                    sender.input(PdfReaderMsg::PrevPage);
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

/// Rebuild the viewport widget depending on Paged or Continuous mode.
fn rebuild_viewport_widget(model: &PdfReaderModel) -> gtk::Widget {
    let container = gtk::Box::new(gtk::Orientation::Vertical, 0);

    match model.view_mode {
        PdfViewMode::Paged => {
            container.set_orientation(gtk::Orientation::Vertical);
            container.set_halign(gtk::Align::Center);
            container.set_valign(gtk::Align::Center);
            container.set_hexpand(true);
            container.set_vexpand(true);
            container.set_margin_all(16);

            if let Some((tex, w, h)) = model.textures.get(&model.current_page) {
                let pic = gtk::Picture::for_paintable(tex);
                pic.set_can_shrink(true);
                pic.set_content_fit(gtk::ContentFit::Contain);
                let target_w = ((*w as f32) * model.zoom_level) as i32;
                let target_h = ((*h as f32) * model.zoom_level) as i32;
                pic.set_size_request(target_w, target_h);
                pic.set_halign(gtk::Align::Center);
                pic.set_valign(gtk::Align::Center);
                pic.add_css_class("kalam-pdf-page-image");
                container.append(&pic);
            } else {
                let loading_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
                loading_box.set_halign(gtk::Align::Center);
                loading_box.set_valign(gtk::Align::Center);

                if !model.status_text.is_empty() {
                    let err_lbl = gtk::Label::new(Some(&model.status_text));
                    err_lbl.add_css_class("dim-label");
                    loading_box.append(&err_lbl);
                } else {
                    let spinner = gtk::Spinner::new();
                    spinner.start();
                    spinner.set_size_request(32, 32);
                    loading_box.append(&spinner);

                    let label = gtk::Label::new(Some(&format!(
                        "Rendering page {}...",
                        model.current_page
                    )));
                    label.add_css_class("dim-label");
                    loading_box.append(&label);
                }

                container.append(&loading_box);
            }
        }
        PdfViewMode::Continuous => {
            container.set_orientation(gtk::Orientation::Vertical);
            container.set_spacing(20);
            container.set_halign(gtk::Align::Center);
            container.set_valign(gtk::Align::Start);
            container.set_hexpand(true);
            container.set_vexpand(false);
            container.set_margin_top(24);
            container.set_margin_bottom(80);

            for p in 1..=model.total_pages {
                let page_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
                page_box.set_halign(gtk::Align::Center);

                if let Some((tex, w, h)) = model.textures.get(&p) {
                    let pic = gtk::Picture::for_paintable(tex);
                    pic.set_can_shrink(true);
                    pic.set_content_fit(gtk::ContentFit::Contain);
                    let target_w = ((*w as f32) * model.zoom_level) as i32;
                    let target_h = ((*h as f32) * model.zoom_level) as i32;
                    pic.set_size_request(target_w, target_h);
                    pic.set_halign(gtk::Align::Center);
                    pic.add_css_class("kalam-pdf-page-image");
                    page_box.append(&pic);
                } else {
                    let placeholder = gtk::Box::new(gtk::Orientation::Vertical, 8);
                    placeholder.set_size_request(
                        (600.0 * model.zoom_level) as i32,
                        (800.0 * model.zoom_level) as i32,
                    );
                    placeholder.set_halign(gtk::Align::Center);
                    placeholder.set_valign(gtk::Align::Center);
                    placeholder.add_css_class("kalam-pdf-placeholder");

                    let lbl = gtk::Label::new(Some(&format!("Page {}", p)));
                    lbl.add_css_class("dim-label");
                    lbl.set_valign(gtk::Align::Center);
                    placeholder.append(&lbl);

                    page_box.append(&placeholder);
                }

                container.append(&page_box);
            }
        }
    }

    container.upcast()
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

            let s = sender.clone();
            btn.connect_clicked(move |_| {
                s.input(PdfReaderMsg::JumpToToc(p));
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

        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row_box.set_margin_start((entry.depth as i32 * 14).min(70));

        let title_lbl = gtk::Label::new(Some(&entry.title));
        title_lbl.set_hexpand(true);
        title_lbl.set_halign(gtk::Align::Start);
        title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row_box.append(&title_lbl);

        let page_lbl = gtk::Label::new(Some(&format!("{}", entry.page)));
        page_lbl.add_css_class("dim-label");
        page_lbl.set_halign(gtk::Align::End);
        row_box.append(&page_lbl);

        btn.set_child(Some(&row_box));

        let p = entry.page;
        let s = sender.clone();
        btn.connect_clicked(move |_| {
            s.input(PdfReaderMsg::JumpToToc(p));
        });

        container.append(&btn);
    }
}
