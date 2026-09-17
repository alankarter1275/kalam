//! Moku-style comics reader page component.
//!
//! Provides an immersive image-pager reader for CBZ/CBR comic archives with
//! LTR/RTL/Webtoon reading modes, page scrubbing, slide-out settings panel,
//! floating minimal chrome, and bounded memory caching.

pub mod types;
pub mod provider;
pub mod providers;

pub use types::*;

use gtk::gdk;
use gtk::prelude::*;
use relm4::prelude::*;
use std::collections::HashMap;
use std::sync::Arc;

pub struct ComicsReaderModel {
    pub title: String,
    pub provider: Arc<dyn provider::ImageProvider>,
    pub current_page: usize,
    pub direction: ReadingDirection,
    pub page_style: PageStyle,
    pub fit_mode: FitMode,
    pub show_chrome: bool,
    pub show_settings: bool,
    pub textures: HashMap<usize, gdk::Texture>,
}

impl ComicsReaderModel {
    pub fn new(init: types::ComicsReaderInit) -> Self {
        let model = Self {
            title: init.title,
            provider: init.provider,
            current_page: 0,
            direction: ReadingDirection::Ltr,
            page_style: PageStyle::LongStrip,
            fit_mode: FitMode::Width,
            show_chrome: true,
            show_settings: false,
            textures: HashMap::new(),
        };

        // Note: we can't spawn tasks directly from new without the sender,
        // so preload_nearby_pages must be called from `init` trait method,
        // or we handle loading synchronously if it's local.
        // For now we just return the model and let `init` trigger the fetch.
        model
    }

    /// Ask the provider for images in the window `curr ± 2` (or wider for webtoon/double).
    fn get_missing_pages(&mut self) -> Vec<usize> {
        let total = self.provider.page_count();
        if total == 0 {
            return Vec::new();
        }
        let curr = self.current_page;
        let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;

        let (min_keep, max_keep) = if is_webtoon {
            (curr.saturating_sub(4), (curr + 6).min(total.saturating_sub(1)))
        } else if self.page_style == PageStyle::Double {
            (curr.saturating_sub(2), (curr + 3).min(total.saturating_sub(1)))
        } else {
            (curr.saturating_sub(2), (curr + 2).min(total.saturating_sub(1)))
        };

        // Retain textures inside active preloading range to bound memory
        self.textures.retain(|&idx, _| idx >= min_keep && idx <= max_keep);

        let mut missing = Vec::new();
        for idx in min_keep..=max_keep {
            if !self.textures.contains_key(&idx) {
                missing.push(idx);
            }
        }
        missing
    }

    pub fn set_page(&mut self, idx: usize) {
        let total = self.provider.page_count();
        if total == 0 {
            return;
        }
        let clamped = idx.clamp(0, total - 1);
        self.current_page = clamped;
    }

    pub fn trigger_loads(&mut self, sender: &relm4::ComponentSender<Self>) {
        let missing = self.get_missing_pages();
        let provider = self.provider.clone();

        for idx in missing {
            let s = sender.clone();
            let prov = provider.clone();
            crate::tasks::spawn(
                move |_| -> Option<gdk::Texture> {
                    let b = prov.fetch_page(idx).ok()?;
                    if let Ok(img) = image::load_from_memory(&b) {
                        let rgba = img.to_rgba8();
                        let width = rgba.width() as i32;
                        let height = rgba.height() as i32;
                        let stride = (width * 4) as usize;
                        let gbytes = glib::Bytes::from(&rgba.into_raw());
                        let mem_tex = gdk::MemoryTexture::new(
                            width,
                            height,
                            gdk::MemoryFormat::R8g8b8a8,
                            &gbytes,
                            stride,
                        );
                        Some(mem_tex.upcast::<gdk::Texture>())
                    } else if let Ok(tex) = gdk::Texture::from_bytes(&glib::Bytes::from(&b)) {
                        Some(tex)
                    } else {
                        None
                    }
                },
                |_| {},
                move |tex_opt| {
                    s.input(types::ComicsReaderMsg::PageLoaded(idx, tex_opt));
                },
            );
        }
    }

    pub fn next_page(&mut self) {
        let step = if self.page_style == PageStyle::Double { 2 } else { 1 };
        let total = self.provider.page_count();
        if total > 0 && self.current_page + step < total {
            self.set_page(self.current_page + step);
        } else if total > 0 {
            self.set_page(total - 1);
        }
    }

    pub fn prev_page(&mut self) {
        let step = if self.page_style == PageStyle::Double { 2 } else { 1 };
        self.set_page(self.current_page.saturating_sub(step));
    }
}

fn rebuild_viewport_widget(model: &ComicsReaderModel) -> gtk::Widget {
    let fit_content_fit = match model.fit_mode {
        FitMode::Width => gtk::ContentFit::Fill,
        FitMode::Height => gtk::ContentFit::Contain,
        FitMode::Screen => gtk::ContentFit::Cover,
        FitMode::Original => gtk::ContentFit::ScaleDown,
    };

    let is_webtoon = model.direction == ReadingDirection::Webtoon || model.page_style == PageStyle::LongStrip;

    if is_webtoon {
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 4);
        vbox.set_halign(gtk::Align::Center);
        vbox.set_valign(gtk::Align::Start);
        vbox.set_hexpand(true);
        vbox.add_css_class("kalam-webtoon-container");

        let total = model.provider.page_count();
        for idx in 0..total {
            let item_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            item_box.set_halign(gtk::Align::Center);
            item_box.set_valign(gtk::Align::Start);
            item_box.set_hexpand(true);
            item_box.add_css_class("kalam-webtoon-page-item");

            if let Some(texture) = model.textures.get(&idx) {
                let pic = gtk::Picture::for_paintable(texture);
                pic.set_can_shrink(true);
                pic.set_content_fit(fit_content_fit);
                pic.set_halign(gtk::Align::Center);
                item_box.append(&pic);
            } else {
                let placeholder = gtk::Box::new(gtk::Orientation::Vertical, 0);
                placeholder.set_size_request(400, 600);
                placeholder.set_halign(gtk::Align::Center);
                placeholder.set_valign(gtk::Align::Center);

                let spinner = gtk::Spinner::new();
                spinner.set_spinning(true);
                spinner.set_size_request(32, 32);
                spinner.set_halign(gtk::Align::Center);
                spinner.set_valign(gtk::Align::Center);
                placeholder.append(&spinner);

                item_box.append(&placeholder);
            }
            vbox.append(&item_box);
        }
        return vbox.upcast();
    }

    if model.page_style == PageStyle::Double {
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        hbox.set_halign(gtk::Align::Center);
        hbox.set_valign(gtk::Align::Center);
        hbox.set_hexpand(true);
        hbox.set_vexpand(true);

        let page_a = model.current_page;
        let page_b = if model.current_page + 1 < model.provider.page_count() {
            Some(model.current_page + 1)
        } else {
            None
        };

        // For RTL (manga): Page A is on the RIGHT, Page B is on the LEFT.
        // For LTR: Page A is on the LEFT, Page B is on the RIGHT.
        let (left_idx, right_idx) = match model.direction {
            ReadingDirection::Rtl => (page_b, Some(page_a)),
            _ => (Some(page_a), page_b),
        };

        let create_page_widget = |opt_idx: Option<usize>| -> gtk::Widget {
            let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
            container.set_halign(gtk::Align::Center);
            container.set_valign(gtk::Align::Center);
            container.set_hexpand(true);
            container.set_vexpand(true);

            if let Some(idx) = opt_idx {
                if let Some(texture) = model.textures.get(&idx) {
                    let pic = gtk::Picture::for_paintable(texture);
                    pic.set_can_shrink(true);
                    pic.set_content_fit(fit_content_fit);
                    pic.set_halign(gtk::Align::Center);
                    pic.set_valign(gtk::Align::Center);
                    container.append(&pic);
                } else {
                    let spinner = gtk::Spinner::new();
                    spinner.set_spinning(true);
                    spinner.set_size_request(48, 48);
                    container.append(&spinner);
                }
            }
            container.upcast()
        };

        hbox.append(&create_page_widget(left_idx));
        hbox.append(&create_page_widget(right_idx));

        return hbox.upcast();
    }

    // Single page mode
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    vbox.set_halign(gtk::Align::Center);
    vbox.set_valign(gtk::Align::Center);
    vbox.set_hexpand(true);
    vbox.set_vexpand(true);

    if let Some(texture) = model.textures.get(&model.current_page) {
        let pic = gtk::Picture::for_paintable(texture);
        pic.set_can_shrink(true);
        pic.set_content_fit(fit_content_fit);
        pic.set_halign(gtk::Align::Center);
        pic.set_valign(gtk::Align::Center);
        vbox.append(&pic);
    } else {
        let spinner = gtk::Spinner::new();
        spinner.set_spinning(true);
        spinner.set_size_request(48, 48);
        vbox.append(&spinner);
    }

    vbox.upcast()
}

#[relm4::component(pub)]
impl Component for ComicsReaderModel {
    type Init = types::ComicsReaderInit;
    type Input = ComicsReaderMsg;
    type Output = ComicsReaderOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Overlay {
            add_css_class: "kalam-comics-overlay",
            set_hexpand: true,
            set_vexpand: true,

            // ── 1. Base Layer: Comic Image Viewport ─────────────────────
            #[name = "viewport_box"]
            gtk::ScrolledWindow {
                add_css_class: "kalam-comics-viewport",
                set_hexpand: true,
                set_vexpand: true,
            },

            // ── 2. Overlay: Dim Backdrop for Settings Drawer ───────────
            add_overlay = &gtk::Button {
                add_css_class: "kalam-reader-dim",
                #[watch]
                set_visible: model.show_settings,
                set_hexpand: true,
                set_vexpand: true,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Fill,
                connect_clicked => ComicsReaderMsg::CloseSettings,
            },

            // ── 3. Overlay: Floating Minimal Top Bar (Moku-style) ───────
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_chrome,
                set_transition_type: gtk::RevealerTransitionType::SlideDown,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Start,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    add_css_class: "kalam-comics-topbar",
                    set_spacing: 12,

                    // Clean close button
                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes("window-close-symbolic", 16, &["kalam-inline-icon"])),
                        add_css_class: "kalam-comics-icon-btn",
                        set_tooltip_text: Some("Close reader (Esc)"),
                        connect_clicked => ComicsReaderMsg::Close,
                    },

                    // Breadcrumb title: Title / Page X of Y
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        set_hexpand: true,
                        set_valign: gtk::Align::Center,

                        gtk::Label {
                            #[watch]
                            set_label: &model.title,
                            add_css_class: "kalam-comics-title",
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                            set_halign: gtk::Align::Start,
                        },

                        gtk::Label {
                            set_label: "/",
                            add_css_class: "kalam-comics-separator",
                        },

                        gtk::Label {
                            #[watch]
                            set_label: &format!(
                                "Page {} of {}",
                                model.current_page + 1,
                                model.provider.page_count().max(1)
                            ),
                            add_css_class: "kalam-comics-page-count",
                        },
                    },

                    // Right controls: Fit Mode, Direction Badge, Settings Gear
                    gtk::Button {
                        #[watch]
                        set_child: Some(&crate::icons::symbolic_with_classes(model.fit_mode.icon(), 16, &["kalam-inline-icon"])),
                        add_css_class: "kalam-comics-icon-btn",
                        #[watch]
                        set_tooltip_text: Some(&format!("Fit Mode: {} (F)", model.fit_mode.label())),
                        connect_clicked => ComicsReaderMsg::ToggleFitMode,
                    },

                    gtk::Button {
                        #[watch]
                        set_label: model.direction.short_label(),
                        add_css_class: "kalam-comics-badge-btn",
                        #[watch]
                        set_tooltip_text: Some(&format!("Direction: {}", model.direction.label())),
                        connect_clicked => ComicsReaderMsg::ToggleDirection,
                    },

                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes("emblem-system-symbolic", 16, &["kalam-inline-icon"])),
                        add_css_class: "kalam-comics-icon-btn",
                        set_tooltip_text: Some("Reader Settings"),
                        connect_clicked => ComicsReaderMsg::ToggleSettings,
                    },
                },
            },

            // ── 4. Overlay: Edge-to-Edge Bottom Scrubber Bar (Moku-style) ──
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_chrome,
                set_transition_type: gtk::RevealerTransitionType::SlideUp,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::End,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    add_css_class: "kalam-comics-bottombar",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 12,
                    set_valign: gtk::Align::Center,

                    // Prev page button
                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes("go-previous-symbolic", 15, &["kalam-inline-icon"])),
                        add_css_class: "kalam-comics-icon-btn",
                        set_tooltip_text: Some("Previous page (Left arrow)"),
                        connect_clicked => ComicsReaderMsg::PrevPage,
                    },

                    // Full-width Scrubber Slider
                    gtk::Scale {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_hexpand: true,
                        set_draw_value: false,
                        add_css_class: "kalam-comics-slider",
                        #[watch]
                        set_range: (0.0, (model.provider.page_count().saturating_sub(1)) as f64),
                        #[watch]
                        set_value: model.current_page as f64,
                        connect_value_changed[sender] => move |scale| {
                            let val = scale.value().round() as usize;
                            sender.input(ComicsReaderMsg::SetPage(val));
                        },
                    },

                    // Page indicator pill
                    gtk::Label {
                        #[watch]
                        set_label: &format!("{}/{}", model.current_page + 1, model.provider.page_count().max(1)),
                        add_css_class: "kalam-comics-pill-label",
                        set_valign: gtk::Align::Center,
                    },

                    // Next page button
                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes("go-next-symbolic", 15, &["kalam-inline-icon"])),
                        add_css_class: "kalam-comics-icon-btn",
                        set_tooltip_text: Some("Next page (Right arrow / Space)"),
                        connect_clicked => ComicsReaderMsg::NextPage,
                    },
                },
            },

            // ── 5. Overlay: Slide-out Reader Settings Drawer ───────────
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_settings,
                set_transition_type: gtk::RevealerTransitionType::SlideLeft,
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Fill,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    add_css_class: "kalam-comics-settings-drawer",
                    set_width_request: 320,

                    // Header
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        add_css_class: "kalam-comics-drawer-header",
                        set_spacing: 8,

                        gtk::Label {
                            set_label: "Reader Settings",
                            add_css_class: "kalam-comics-drawer-title",
                            set_hexpand: true,
                            set_halign: gtk::Align::Start,
                        },

                        gtk::Button {
                            set_child: Some(&crate::icons::symbolic_with_classes("window-close-symbolic", 15, &["kalam-inline-icon"])),
                            add_css_class: "kalam-comics-icon-btn",
                            set_tooltip_text: Some("Close Settings"),
                            connect_clicked => ComicsReaderMsg::CloseSettings,
                        },
                    },

                    // Scrollable Drawer Body
                    gtk::ScrolledWindow {
                        set_hexpand: true,
                        set_vexpand: true,

                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 20,
                            add_css_class: "kalam-comics-drawer-body",

                            // Section: PAGE STYLE
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 8,

                                gtk::Label {
                                    set_label: "PAGE STYLE",
                                    add_css_class: "kalam-comics-section-label",
                                    set_halign: gtk::Align::Start,
                                },

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 4,
                                    set_homogeneous: true,

                                    gtk::Button {
                                        #[wrap(Some)]
                                        set_child = &gtk::Box {
                                            set_orientation: gtk::Orientation::Vertical,
                                            set_spacing: 4,
                                            set_halign: gtk::Align::Center,
                                            set_valign: gtk::Align::Center,
                                            gtk::Image { set_icon_name: Some("view-paged-symbolic") },
                                            gtk::Label { set_label: "Single", add_css_class: "kalam-comics-seg-label" },
                                        },
                                        add_css_class: "kalam-comics-seg-btn",
                                        #[watch]
                                        set_css_classes: if model.page_style == PageStyle::Single {
                                            &["kalam-comics-seg-btn", "active"]
                                        } else {
                                            &["kalam-comics-seg-btn"]
                                        },
                                        connect_clicked => ComicsReaderMsg::SetPageStyle(PageStyle::Single),
                                    },

                                    gtk::Button {
                                        #[wrap(Some)]
                                        set_child = &gtk::Box {
                                            set_orientation: gtk::Orientation::Vertical,
                                            set_spacing: 4,
                                            set_halign: gtk::Align::Center,
                                            set_valign: gtk::Align::Center,
                                            gtk::Image { set_icon_name: Some("view-dual-symbolic") },
                                            gtk::Label { set_label: "Double", add_css_class: "kalam-comics-seg-label" },
                                        },
                                        add_css_class: "kalam-comics-seg-btn",
                                        #[watch]
                                        set_css_classes: if model.page_style == PageStyle::Double {
                                            &["kalam-comics-seg-btn", "active"]
                                        } else {
                                            &["kalam-comics-seg-btn"]
                                        },
                                        connect_clicked => ComicsReaderMsg::SetPageStyle(PageStyle::Double),
                                    },

                                    gtk::Button {
                                        #[wrap(Some)]
                                        set_child = &gtk::Box {
                                            set_orientation: gtk::Orientation::Vertical,
                                            set_spacing: 4,
                                            set_halign: gtk::Align::Center,
                                            set_valign: gtk::Align::Center,
                                            gtk::Image { set_icon_name: Some("media-playlist-consecutive-symbolic") },
                                            gtk::Label { set_label: "Fade", add_css_class: "kalam-comics-seg-label" },
                                        },
                                        add_css_class: "kalam-comics-seg-btn",
                                        #[watch]
                                        set_css_classes: if model.page_style == PageStyle::Fade {
                                            &["kalam-comics-seg-btn", "active"]
                                        } else {
                                            &["kalam-comics-seg-btn"]
                                        },
                                        connect_clicked => ComicsReaderMsg::SetPageStyle(PageStyle::Fade),
                                    },

                                    gtk::Button {
                                        #[wrap(Some)]
                                        set_child = &gtk::Box {
                                            set_orientation: gtk::Orientation::Vertical,
                                            set_spacing: 4,
                                            set_halign: gtk::Align::Center,
                                            set_valign: gtk::Align::Center,
                                            gtk::Image { set_icon_name: Some("format-justify-fill-symbolic") },
                                            gtk::Label { set_label: "Long Strip", add_css_class: "kalam-comics-seg-label" },
                                        },
                                        add_css_class: "kalam-comics-seg-btn",
                                        #[watch]
                                        set_css_classes: if model.page_style == PageStyle::LongStrip {
                                            &["kalam-comics-seg-btn", "active"]
                                        } else {
                                            &["kalam-comics-seg-btn"]
                                        },
                                        connect_clicked => ComicsReaderMsg::SetPageStyle(PageStyle::LongStrip),
                                    },
                                },
                            },

                            // Section: READING DIRECTION
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 8,

                                gtk::Label {
                                    set_label: "READING DIRECTION",
                                    add_css_class: "kalam-comics-section-label",
                                    set_halign: gtk::Align::Start,
                                },

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 6,
                                    set_homogeneous: true,

                                    gtk::Button {
                                        #[wrap(Some)]
                                        set_child = &gtk::Box {
                                            set_orientation: gtk::Orientation::Horizontal,
                                            set_spacing: 6,
                                            set_halign: gtk::Align::Center,
                                            set_valign: gtk::Align::Center,
                                            gtk::Label { set_label: "Left to Right", add_css_class: "kalam-comics-seg-label" },
                                            gtk::Image { set_icon_name: Some("go-next-symbolic"), set_icon_size: gtk::IconSize::Inherit },
                                        },
                                        add_css_class: "kalam-comics-seg-btn",
                                        #[watch]
                                        set_css_classes: if model.direction == ReadingDirection::Ltr {
                                            &["kalam-comics-seg-btn", "active"]
                                        } else {
                                            &["kalam-comics-seg-btn"]
                                        },
                                        set_tooltip_text: Some("Left to Right"),
                                        connect_clicked => ComicsReaderMsg::SetDirection(ReadingDirection::Ltr),
                                    },

                                    gtk::Button {
                                        #[wrap(Some)]
                                        set_child = &gtk::Box {
                                            set_orientation: gtk::Orientation::Horizontal,
                                            set_spacing: 6,
                                            set_halign: gtk::Align::Center,
                                            set_valign: gtk::Align::Center,
                                            gtk::Image { set_icon_name: Some("go-previous-symbolic"), set_icon_size: gtk::IconSize::Inherit },
                                            gtk::Label { set_label: "Right to Left", add_css_class: "kalam-comics-seg-label" },
                                        },
                                        add_css_class: "kalam-comics-seg-btn",
                                        #[watch]
                                        set_css_classes: if model.direction == ReadingDirection::Rtl {
                                            &["kalam-comics-seg-btn", "active"]
                                        } else {
                                            &["kalam-comics-seg-btn"]
                                        },
                                        set_tooltip_text: Some("Right to Left (Manga)"),
                                        connect_clicked => ComicsReaderMsg::SetDirection(ReadingDirection::Rtl),
                                    },
                                },
                            },

                            // Section: FIT MODE
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 8,

                                gtk::Label {
                                    set_label: "FIT MODE",
                                    add_css_class: "kalam-comics-section-label",
                                    set_halign: gtk::Align::Start,
                                },

                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 4,
                                    set_homogeneous: true,

                                    gtk::Button {
                                        #[wrap(Some)]
                                        set_child = &gtk::Box {
                                            set_orientation: gtk::Orientation::Vertical,
                                            set_spacing: 4,
                                            set_halign: gtk::Align::Center,
                                            set_valign: gtk::Align::Center,
                                            gtk::Image { set_icon_name: Some("zoom-fit-width-symbolic") },
                                            gtk::Label { set_label: "Fit Width", add_css_class: "kalam-comics-seg-label" },
                                        },
                                        add_css_class: "kalam-comics-seg-btn",
                                        #[watch]
                                        set_css_classes: if model.fit_mode == FitMode::Width {
                                            &["kalam-comics-seg-btn", "active"]
                                        } else {
                                            &["kalam-comics-seg-btn"]
                                        },
                                        connect_clicked => ComicsReaderMsg::SetFitMode(FitMode::Width),
                                    },

                                    gtk::Button {
                                        #[wrap(Some)]
                                        set_child = &gtk::Box {
                                            set_orientation: gtk::Orientation::Vertical,
                                            set_spacing: 4,
                                            set_halign: gtk::Align::Center,
                                            set_valign: gtk::Align::Center,
                                            gtk::Image { set_icon_name: Some("zoom-fit-height-symbolic") },
                                            gtk::Label { set_label: "Fit Height", add_css_class: "kalam-comics-seg-label" },
                                        },
                                        add_css_class: "kalam-comics-seg-btn",
                                        #[watch]
                                        set_css_classes: if model.fit_mode == FitMode::Height {
                                            &["kalam-comics-seg-btn", "active"]
                                        } else {
                                            &["kalam-comics-seg-btn"]
                                        },
                                        connect_clicked => ComicsReaderMsg::SetFitMode(FitMode::Height),
                                    },

                                    gtk::Button {
                                        #[wrap(Some)]
                                        set_child = &gtk::Box {
                                            set_orientation: gtk::Orientation::Vertical,
                                            set_spacing: 4,
                                            set_halign: gtk::Align::Center,
                                            set_valign: gtk::Align::Center,
                                            gtk::Image { set_icon_name: Some("zoom-fit-best-symbolic") },
                                            gtk::Label { set_label: "Fit Screen", add_css_class: "kalam-comics-seg-label" },
                                        },
                                        add_css_class: "kalam-comics-seg-btn",
                                        #[watch]
                                        set_css_classes: if model.fit_mode == FitMode::Screen {
                                            &["kalam-comics-seg-btn", "active"]
                                        } else {
                                            &["kalam-comics-seg-btn"]
                                        },
                                        connect_clicked => ComicsReaderMsg::SetFitMode(FitMode::Screen),
                                    },

                                    gtk::Button {
                                        #[wrap(Some)]
                                        set_child = &gtk::Box {
                                            set_orientation: gtk::Orientation::Vertical,
                                            set_spacing: 4,
                                            set_halign: gtk::Align::Center,
                                            set_valign: gtk::Align::Center,
                                            gtk::Image { set_icon_name: Some("zoom-original-symbolic") },
                                            gtk::Label { set_label: "Original", add_css_class: "kalam-comics-seg-label" },
                                        },
                                        add_css_class: "kalam-comics-seg-btn",
                                        #[watch]
                                        set_css_classes: if model.fit_mode == FitMode::Original {
                                            &["kalam-comics-seg-btn", "active"]
                                        } else {
                                            &["kalam-comics-seg-btn"]
                                        },
                                        connect_clicked => ComicsReaderMsg::SetFitMode(FitMode::Original),
                                    },
                                },
                            },

                            // Section: SHORTCUTS
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 8,
                                add_css_class: "kalam-comics-shortcuts-box",

                                gtk::Label {
                                    set_label: "KEYBOARD SHORTCUTS",
                                    add_css_class: "kalam-comics-section-label",
                                    set_halign: gtk::Align::Start,
                                },

                                gtk::Label {
                                    set_label: "← / →       Turn page\nSpace       Next page\nClick image Toggle chrome\nF           Toggle fit mode\nEsc         Close",
                                    add_css_class: "kalam-comics-shortcut-text",
                                    set_halign: gtk::Align::Start,
                                },
                            },
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
        let mut model = ComicsReaderModel::new(init);
        model.trigger_loads(&sender);
        let widgets = view_output!();

        let child = rebuild_viewport_widget(&model);
        widgets.viewport_box.set_child(Some(&child));

        // Tap/click on the comic viewport: left = prev/next, right = next/prev, center = toggle chrome
        let click = gtk::GestureClick::new();
        let s = sender.clone();
        click.connect_released(move |gesture, _, x, _| {
            if let Some(widget) = gesture.widget() {
                let width = widget.width() as f64;
                if width > 0.0 {
                    let ratio = x / width;
                    s.input(ComicsReaderMsg::TapAtRatio(ratio));
                } else {
                    s.input(ComicsReaderMsg::ToggleChrome);
                }
            }
        });
        widgets.viewport_box.add_controller(click);

        // Webtoon scroll position listener
        let vadj = widgets.viewport_box.vadjustment();
        let s_scroll = sender.clone();
        let page_count = model.provider.page_count();
        vadj.connect_value_changed(move |adj| {
            let max = (adj.upper() - adj.page_size()).max(1.0);
            if max > 0.0 && page_count > 0 {
                let ratio = (adj.value() / max).clamp(0.0, 1.0);
                let page = ((page_count - 1) as f64 * ratio).round() as usize;
                s_scroll.input(ComicsReaderMsg::UpdateScrollPage(page));
            }
        });

        // Keyboard navigation controller
        let key_controller = gtk::EventControllerKey::new();
        let s = sender.clone();
        key_controller.connect_key_pressed(move |_, key, _, _| {
            match key {
                gdk::Key::Escape => {
                    s.input(ComicsReaderMsg::Close);
                    glib::Propagation::Stop
                }
                gdk::Key::Left => {
                    s.input(ComicsReaderMsg::KeyLeft);
                    glib::Propagation::Stop
                }
                gdk::Key::Right => {
                    s.input(ComicsReaderMsg::KeyRight);
                    glib::Propagation::Stop
                }
                gdk::Key::Page_Up | gdk::Key::Up => {
                    s.input(ComicsReaderMsg::PrevPage);
                    glib::Propagation::Stop
                }
                gdk::Key::Page_Down | gdk::Key::Down | gdk::Key::space => {
                    s.input(ComicsReaderMsg::NextPage);
                    glib::Propagation::Stop
                }
                gdk::Key::f | gdk::Key::F => {
                    s.input(ComicsReaderMsg::ToggleFitMode);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        root.add_controller(key_controller);

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        let mut loaded_page_idx = None;

        match msg {
            ComicsReaderMsg::SetPage(idx) => {
                self.set_page(idx);
                self.trigger_loads(&sender);
            }
            ComicsReaderMsg::UpdateScrollPage(idx) => {
                let total = self.provider.page_count();
                if total > 0 {
                    let clamped = idx.clamp(0, total - 1);
                    if self.current_page != clamped {
                        self.current_page = clamped;
                        self.trigger_loads(&sender);
                    }
                }
            }
            ComicsReaderMsg::NextPage => {
                self.next_page();
                self.trigger_loads(&sender);
            }
            ComicsReaderMsg::PrevPage => {
                self.prev_page();
                self.trigger_loads(&sender);
            }
            ComicsReaderMsg::KeyLeft => {
                if self.direction == ReadingDirection::Rtl {
                    self.next_page();
                } else {
                    self.prev_page();
                }
                self.trigger_loads(&sender);
            }
            ComicsReaderMsg::KeyRight => {
                if self.direction == ReadingDirection::Rtl {
                    self.prev_page();
                } else {
                    self.next_page();
                }
                self.trigger_loads(&sender);
            }
            ComicsReaderMsg::TapAtRatio(ratio) => {
                if ratio < 0.35 {
                    if self.direction == ReadingDirection::Rtl {
                        self.next_page();
                    } else {
                        self.prev_page();
                    }
                    self.trigger_loads(&sender);
                } else if ratio > 0.65 {
                    if self.direction == ReadingDirection::Rtl {
                        self.prev_page();
                    } else {
                        self.next_page();
                    }
                    self.trigger_loads(&sender);
                } else {
                    self.show_chrome = !self.show_chrome;
                }
            }
            ComicsReaderMsg::PageLoaded(idx, maybe_tex) => {
                if let Some(tex) = maybe_tex {
                    self.textures.insert(idx, tex);
                    loaded_page_idx = Some(idx);
                }
            }
            ComicsReaderMsg::ToggleDirection => {
                self.direction = self.direction.next();
            }
            ComicsReaderMsg::SetDirection(dir) => {
                self.direction = dir;
            }
            ComicsReaderMsg::SetPageStyle(style) => {
                self.page_style = style;
            }
            ComicsReaderMsg::ToggleFitMode => {
                self.fit_mode = self.fit_mode.next();
            }
            ComicsReaderMsg::SetFitMode(fit) => {
                self.fit_mode = fit;
            }
            ComicsReaderMsg::ToggleChrome => {
                self.show_chrome = !self.show_chrome;
            }
            ComicsReaderMsg::ToggleSettings => {
                self.show_settings = !self.show_settings;
            }
            ComicsReaderMsg::CloseSettings => {
                self.show_settings = false;
            }
            ComicsReaderMsg::Close => {
                if self.show_settings {
                    self.show_settings = false;
                } else {
                    let _ = sender.output(ComicsReaderOut::Close);
                }
            }
        }

        let fit_content_fit = match self.fit_mode {
            FitMode::Width => gtk::ContentFit::Fill,
            FitMode::Height => gtk::ContentFit::Contain,
            FitMode::Screen => gtk::ContentFit::Cover,
            FitMode::Original => gtk::ContentFit::ScaleDown,
        };
        let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;

        let mut updated_in_place = false;
        if is_webtoon {
            if let Some(child_widget) = widgets.viewport_box.child() {
                if child_widget.has_css_class("kalam-webtoon-container") {
                    if let Ok(vbox) = child_widget.downcast::<gtk::Box>() {
                        if let Some(idx) = loaded_page_idx {
                            if let Some(tex) = self.textures.get(&idx) {
                                let mut curr = vbox.first_child();
                                let mut i = 0;
                                while let Some(item) = curr {
                                    if i == idx {
                                        if let Ok(item_box) = item.downcast::<gtk::Box>() {
                                            while let Some(old) = item_box.first_child() {
                                                item_box.remove(&old);
                                            }
                                            let pic = gtk::Picture::for_paintable(tex);
                                            pic.set_can_shrink(true);
                                            pic.set_content_fit(fit_content_fit);
                                            pic.set_halign(gtk::Align::Center);
                                            item_box.append(&pic);
                                        }
                                        break;
                                    }
                                    curr = item.next_sibling();
                                    i += 1;
                                }
                            }
                        }
                        updated_in_place = true;
                    }
                }
            }
        }

        if !updated_in_place {
            let child = rebuild_viewport_widget(self);
            widgets.viewport_box.set_child(Some(&child));
        }

        self.update_view(widgets, sender);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    struct DummyProvider {
        count: usize,
    }

    impl provider::ImageProvider for DummyProvider {
        fn page_count(&self) -> usize {
            self.count
        }

        fn fetch_page(&self, _idx: usize) -> anyhow::Result<Vec<u8>> {
            Ok(vec![0u8; 16])
        }
    }

    #[test]
    fn test_missing_pages_preloading_bounds() {
        let dummy = Arc::new(DummyProvider { count: 10 });
        let mut model = ComicsReaderModel::new(types::ComicsReaderInit {
            title: "Test Manga".to_string(),
            provider: dummy,
        });

        model.page_style = PageStyle::Single;
        model.direction = ReadingDirection::Ltr;
        model.current_page = 5;

        let missing = model.get_missing_pages();
        assert_eq!(missing, vec![3, 4, 5, 6, 7]);
    }

    #[test]
    fn test_double_page_step_and_navigation() {
        let dummy = Arc::new(DummyProvider { count: 10 });
        let mut model = ComicsReaderModel::new(types::ComicsReaderInit {
            title: "Test Manga".to_string(),
            provider: dummy,
        });

        model.page_style = PageStyle::Double;
        model.current_page = 0;

        model.next_page();
        assert_eq!(model.current_page, 2);

        model.next_page();
        assert_eq!(model.current_page, 4);

        model.prev_page();
        assert_eq!(model.current_page, 2);
    }

    #[test]
    fn test_reading_direction_cycle() {
        let dir = ReadingDirection::Ltr;
        assert_eq!(dir.next(), ReadingDirection::Rtl);
        assert_eq!(ReadingDirection::Rtl.next(), ReadingDirection::Webtoon);
        assert_eq!(ReadingDirection::Webtoon.next(), ReadingDirection::Ltr);
    }

    #[test]
    fn test_webtoon_mode_preloading_range() {
        let dummy = Arc::new(DummyProvider { count: 20 });
        let mut model = ComicsReaderModel::new(types::ComicsReaderInit {
            title: "Test Webtoon".to_string(),
            provider: dummy,
        });

        model.direction = ReadingDirection::Webtoon;
        model.current_page = 5;

        let missing = model.get_missing_pages();
        assert_eq!(missing, (1..=11).collect::<Vec<_>>());
    }
}

