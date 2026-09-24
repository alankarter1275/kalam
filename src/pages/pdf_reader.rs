//! PDF Reader Component for Kalam.
//!
//! Native PDF reader built on MuPDF rasterization, offering:
//! - Decoupled Page Layout (Single Page vs Two-Page Spread) and Scroll Flow (Continuous vs Discrete).
//! - Dual-column continuous vertical scroll streams for facing spreads.
//! - Configurable page spread gap (0px, 4px, 8px, 12px, 16px) for seamless facing spreads.
//! - Autohiding edge hover navigation pills matching Kalam's EPUB reader chrome.
//! - Slide-in Zen-browser style left sidebar containing document outlines (TOC) and reader settings.
//! - Reading history, session time tracking, and catalog persistence for "Continue Reading" on Home.
//! - Crash-free, in-place zoom updates and robust pixel stride handling.

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use relm4::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::db::Catalog;
use crate::pdf::{PdfDocument, PdfTocEntry};

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

/// Page layout: Single page vs Two-page spread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfPageLayout {
    Single,
    TwoPage,
}

/// Scroll flow: Discrete (spread-at-a-time) vs Continuous vertical stream.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfScrollFlow {
    Discrete,
    Continuous,
}


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfSidebarTab {
    Toc,
    Settings,
}

pub struct PdfSettingsWidgets {
    pub layout_single_btn: gtk::Button,
    pub layout_two_page_btn: gtk::Button,
    pub flow_continuous_btn: gtk::Button,
    pub flow_discrete_btn: gtk::Button,
    pub cover_alone_switch: gtk::Switch,
    pub gap_0_btn: gtk::Button,
    pub gap_4_btn: gtk::Button,
    pub gap_8_btn: gtk::Button,
    pub gap_12_btn: gtk::Button,
    pub gap_16_btn: gtk::Button,
    pub spread_settings_box: gtk::Box,
    pub smart_crop_switch: gtk::Switch,
    pub zoom_label: gtk::Label,
}

#[derive(Debug)]
pub enum PdfReaderMsg {
    NextPage,
    PrevPage,
    ZoomIn,
    ZoomOut,
    ResetZoom,
    SetPageLayout(PdfPageLayout),
    SetScrollFlow(PdfScrollFlow),
    SetTwoPageGap(i32),
    ToggleContinuousMode,
    SetCoverAlone(bool),
    SetSmartCrop(bool),
    ToggleSmartCrop,
    SwitchSidebarTab(PdfSidebarTab),
    ToggleSidebar,
    CloseSidebar,
    ToggleControls,
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
    pub page_layout: PdfPageLayout,
    pub scroll_flow: PdfScrollFlow,
    pub two_page_gap: i32,
    pub cover_alone: bool,
    pub smart_crop: bool,
    pub sidebar_tab: PdfSidebarTab,
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
    pub session_id: Option<i64>,
    pub session_start: std::time::Instant,
    pub page_pictures: HashMap<usize, gtk::Picture>,
    pub paged_picture: Option<gtk::Picture>,
    pub paged_loading_box: Option<gtk::Box>,
    pub paged_label: Option<gtk::Label>,
    pub two_page_left_pic: Option<gtk::Picture>,
    pub two_page_right_pic: Option<gtk::Picture>,
    pub two_page_loading_box: Option<gtk::Box>,
    pub two_page_label: Option<gtk::Label>,
    pub left_stack: Option<gtk::Stack>,
    pub toc_list_box: Option<gtk::Box>,
    pub settings_widgets: Option<PdfSettingsWidgets>,
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

        let _ = init.catalog.mark_book_opened(init.book_id);
        let start_pct = match init.catalog.get_book(init.book_id) {
            Ok(Some(b)) => b.progress as i64,
            _ => 0,
        };
        let session_id = init.catalog.start_reading_session(init.book_id, start_pct).ok();
        let session_start = std::time::Instant::now();

        let layout_str = init.catalog.get_pref("reader.pdf.layout");
        let flow_str = init.catalog.get_pref("reader.pdf.flow");
        let view_mode_str = init.catalog.get_pref("reader.pdf.view_mode");

        let (page_layout, scroll_flow) = match (layout_str.as_deref(), flow_str.as_deref(), view_mode_str.as_deref()) {
            (Some("two_page"), Some("discrete"), _) => (PdfPageLayout::TwoPage, PdfScrollFlow::Discrete),
            (Some("two_page"), Some("continuous"), _) => (PdfPageLayout::TwoPage, PdfScrollFlow::Continuous),
            (Some("single"), Some("discrete"), _) => (PdfPageLayout::Single, PdfScrollFlow::Discrete),
            (Some("single"), Some("continuous"), _) => (PdfPageLayout::Single, PdfScrollFlow::Continuous),
            (Some("two_page"), _, _) => (PdfPageLayout::TwoPage, PdfScrollFlow::Continuous),
            (Some("single"), _, _) => (PdfPageLayout::Single, PdfScrollFlow::Continuous),
            (_, _, Some("two_page")) => (PdfPageLayout::TwoPage, PdfScrollFlow::Discrete),
            (_, _, Some("paged")) => (PdfPageLayout::Single, PdfScrollFlow::Discrete),
            (_, _, Some("continuous")) => (PdfPageLayout::Single, PdfScrollFlow::Continuous),
            _ => {
                let continuous_pref = init.catalog.get_pref_i64("reader.pdf.continuous", 1);
                if continuous_pref == 0 {
                    (PdfPageLayout::Single, PdfScrollFlow::Discrete)
                } else {
                    (PdfPageLayout::Single, PdfScrollFlow::Continuous)
                }
            }
        };

        let cover_alone = init.catalog.get_pref_i64("reader.pdf.cover_alone", 1) != 0;
        let two_page_gap = (init.catalog.get_pref_i64("reader.pdf.two_page_gap", 12) as i32).clamp(0, 48);

        let smart_crop_pref = init
            .catalog
            .get_pref(&format!("book.{}.pdf.smart_crop", init.book_id));
        let smart_crop = smart_crop_pref.as_deref() == Some("1");



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
            page_layout,
            scroll_flow,
            two_page_gap,
            cover_alone,
            smart_crop,
            sidebar_tab: PdfSidebarTab::Toc,
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
            session_id,
            session_start,
            page_pictures: HashMap::new(),
            paged_picture: None,
            paged_loading_box: None,
            paged_label: None,
            two_page_left_pic: None,
            two_page_right_pic: None,
            two_page_loading_box: None,
            two_page_label: None,
            left_stack: None,
            toc_list_box: None,
            settings_widgets: None,
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

                let scale = (1.5 * model.zoom_level).clamp(0.5, 3.5);
                if let Ok(rendered) = doc.render_page_rgba(model.current_page, scale, model.smart_crop) {
                    let bytes = glib::Bytes::from_owned(rendered.samples);
                    let texture = gdk::MemoryTexture::new(
                        rendered.width,
                        rendered.height,
                        gdk::MemoryFormat::R8g8b8a8,
                        &bytes,
                        rendered.stride,
                    );
                    model.textures.insert(
                        model.current_page,
                        (texture.upcast(), rendered.width, rendered.height),
                    );
                }
            } else {
                model.status_text = "Failed to open PDF document".to_string();
                model.is_loading = false;
            }
        } else {
            model.status_text = "PDF file not found".to_string();
            model.is_loading = false;
        }

        model
    }

    /// End active reading session in catalog.
    pub fn close_session(&self) {
        if let Some(sid) = self.session_id {
            let secs = self.session_start.elapsed().as_secs() as i64;
            let end_pct = (self.progress_fraction() * 100.0).round() as i64;
            let _ = self.catalog.end_reading_session(sid, secs, end_pct);
        }
    }

    /// Calculate facing page pair (left, right) for two-page mode.
    pub fn spread_for_page(&self, page: usize) -> (usize, Option<usize>) {
        if self.cover_alone {
            if page <= 1 {
                (1, None)
            } else {
                let left = if page.is_multiple_of(2) { page } else { page - 1 };
                let right = if left < self.total_pages {
                    Some(left + 1)
                } else {
                    None
                };
                (left, right)
            }
        } else {
            let left = if page % 2 == 1 { page } else { page - 1 };
            let right = if left < self.total_pages {
                Some(left + 1)
            } else {
                None
            };
            (left, right)
        }
    }

    /// Generate all facing page spreads for continuous dual-column scroll flow.
    pub fn all_spreads(&self) -> Vec<(usize, Option<usize>)> {
        let mut spreads = Vec::new();
        if self.total_pages == 0 {
            return spreads;
        }

        if self.cover_alone {
            spreads.push((1, None));
            let mut p = 2;
            while p <= self.total_pages {
                let right = if p < self.total_pages {
                    Some(p + 1)
                } else {
                    None
                };
                spreads.push((p, right));
                p += 2;
            }
        } else {
            let mut p = 1;
            while p <= self.total_pages {
                let right = if p < self.total_pages {
                    Some(p + 1)
                } else {
                    None
                };
                spreads.push((p, right));
                p += 2;
            }
        }
        spreads
    }

    /// Formatted indicator text for bottom pill (e.g. "2-3/120 (3%)").
    pub fn page_indicator_text(&self) -> String {
        if self.total_pages == 0 {
            return "0/0 (0%)".to_string();
        }
        if self.page_layout == PdfPageLayout::TwoPage {
            let (left, right) = self.spread_for_page(self.current_page);
            if let Some(r) = right {
                format!(
                    "{}-{}/{} ({:.0}%)",
                    left,
                    r,
                    self.total_pages,
                    (r as f64 / self.total_pages as f64) * 100.0
                )
            } else {
                format!(
                    "{}/{} ({:.0}%)",
                    left,
                    self.total_pages,
                    (left as f64 / self.total_pages as f64) * 100.0
                )
            }
        } else {
            format!(
                "{}/{} ({:.0}%)",
                self.current_page,
                self.total_pages,
                (self.current_page as f64 / self.total_pages as f64) * 100.0
            )
        }
    }

    /// Reading progress fraction (0.0 to 1.0).
    pub fn progress_fraction(&self) -> f64 {
        if self.total_pages == 0 {
            return 0.0;
        }
        if self.page_layout == PdfPageLayout::TwoPage {
            let (_, right) = self.spread_for_page(self.current_page);
            let p = right.unwrap_or(self.current_page);
            (p as f64 / self.total_pages as f64).clamp(0.0, 1.0)
        } else {
            (self.current_page as f64 / self.total_pages as f64).clamp(0.0, 1.0)
        }
    }

    /// Save current reading progress to catalog and sidecar.
    pub fn save_progress(&self) {
        if self.total_pages == 0 {
            return;
        }
        let fraction = self.progress_fraction();
        let _ = self.catalog.set_reading_progress(
            self.book_id,
            self.current_page,
            fraction,
            self.total_pages,
        );
        let _ = self.catalog.mark_book_opened(self.book_id);
        if let Some(sid) = self.session_id {
            let secs = self.session_start.elapsed().as_secs() as i64;
            let end_pct = (fraction * 100.0).round() as i64;
            let _ = self.catalog.checkpoint_reading_session(sid, secs, end_pct);
        }
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

    /// Schedule autohide of floating bottom navigation pill.
    pub fn schedule_bottom_hide(&mut self, sender: &ComponentSender<Self>) {
        self.bottom_hide_seq = self.bottom_hide_seq.wrapping_add(1);
        let seq = self.bottom_hide_seq;
        let tx = sender.input_sender().clone();
        glib::timeout_add_local_once(Duration::from_millis(2800), move || {
            let _ = tx.send(PdfReaderMsg::BottomHideTimerTick(seq));
        });
    }

    /// Schedule delayed close of Zen sidebar after pointer leaves.
    pub fn schedule_sidebar_close(&mut self, sender: &ComponentSender<Self>) {
        self.sidebar_close_seq = self.sidebar_close_seq.wrapping_add(1);
        let seq = self.sidebar_close_seq;
        let tx = sender.input_sender().clone();
        glib::timeout_add_local_once(Duration::from_millis(350), move || {
            let _ = tx.send(PdfReaderMsg::SidebarCloseTimerTick(seq));
        });
    }

    /// Asynchronously trigger rasterization of visible and surrounding pages.
    pub fn trigger_loads(&mut self, sender: &ComponentSender<Self>) {
        if self.total_pages == 0 {
            return;
        }

        let path = match self.file_path.as_ref() {
            Some(p) => p.clone(),
            None => return,
        };

        let mut load_order = Vec::new();
        match self.scroll_flow {
            PdfScrollFlow::Continuous => {
                let min_p = self.current_page.saturating_sub(6).max(1);
                let max_p = (self.current_page + 6).min(self.total_pages);
                load_order.push(self.current_page);
                for d in 1..=6 {
                    if self.current_page + d <= max_p {
                        load_order.push(self.current_page + d);
                    }
                    if self.current_page > d && self.current_page - d >= min_p {
                        load_order.push(self.current_page - d);
                    }
                }
            }
            PdfScrollFlow::Discrete => {
                match self.page_layout {
                    PdfPageLayout::Single => {
                        let cur = self.current_page;
                        load_order.push(cur);
                        if cur < self.total_pages {
                            load_order.push(cur + 1);
                        }
                        if cur > 1 {
                            load_order.push(cur - 1);
                        }
                        if cur + 2 <= self.total_pages {
                            load_order.push(cur + 2);
                        }
                    }
                    PdfPageLayout::TwoPage => {
                        let (left, right) = self.spread_for_page(self.current_page);
                        load_order.push(left);
                        if let Some(r) = right {
                            load_order.push(r);
                        }
                        if left + 2 <= self.total_pages {
                            load_order.push(left + 2);
                            if left + 3 <= self.total_pages {
                                load_order.push(left + 3);
                            }
                        }
                        if left >= 2 {
                            let (prev_left, _) = self.spread_for_page(left - 1);
                            load_order.push(prev_left);
                            if prev_left + 1 < left {
                                load_order.push(prev_left + 1);
                            }
                        }
                    }
                }
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

            crate::tasks::spawn_internal(
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

    /// Build the viewport container widget for current layout & scroll flow.
    pub fn build_viewport_widget(&mut self) -> gtk::Widget {
        match self.scroll_flow {
            PdfScrollFlow::Discrete => {
                match self.page_layout {
                    PdfPageLayout::Single => {
                        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
                        container.set_halign(gtk::Align::Center);
                        container.set_valign(gtk::Align::Center);
                        container.set_hexpand(true);
                        container.set_vexpand(true);
                        container.set_margin_top(48);
                        container.set_margin_bottom(72);
                        container.set_margin_start(16);
                        container.set_margin_end(16);

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
                            pic.set_size_request(*w, *h);
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

                        self.two_page_left_pic = None;
                        self.two_page_right_pic = None;
                        self.two_page_loading_box = None;
                        self.two_page_label = None;
                        self.page_pictures.clear();

                        container.upcast()
                    }
                    PdfPageLayout::TwoPage => {
                        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
                        container.set_halign(gtk::Align::Center);
                        container.set_valign(gtk::Align::Center);
                        container.set_hexpand(true);
                        container.set_vexpand(true);
                        container.set_margin_top(48);
                        container.set_margin_bottom(72);
                        container.set_margin_start(16);
                        container.set_margin_end(16);

                        let spread_box = gtk::Box::new(gtk::Orientation::Horizontal, self.two_page_gap);
                        spread_box.set_halign(gtk::Align::Center);
                        spread_box.set_valign(gtk::Align::Center);

                        let (left, right) = self.spread_for_page(self.current_page);

                        let left_pic = gtk::Picture::new();
                        left_pic.set_can_shrink(true);
                        left_pic.set_content_fit(gtk::ContentFit::Contain);
                        left_pic.set_halign(if right.is_some() { gtk::Align::End } else { gtk::Align::Center });
                        left_pic.set_valign(gtk::Align::Center);
                        left_pic.add_css_class("kalam-pdf-page-image");
                        left_pic.add_css_class("kalam-pdf-two-page");

                        let right_pic = gtk::Picture::new();
                        right_pic.set_can_shrink(true);
                        right_pic.set_content_fit(gtk::ContentFit::Contain);
                        right_pic.set_halign(gtk::Align::Start);
                        right_pic.set_valign(gtk::Align::Center);
                        right_pic.add_css_class("kalam-pdf-page-image");
                        right_pic.add_css_class("kalam-pdf-two-page");

                        let loading_box = gtk::Box::new(gtk::Orientation::Vertical, 12);
                        loading_box.set_halign(gtk::Align::Center);
                        loading_box.set_valign(gtk::Align::Center);
                        let spinner = gtk::Spinner::new();
                        spinner.start();
                        spinner.set_size_request(32, 32);
                        let label = gtk::Label::new(Some(&format!(
                            "Rendering page {}...",
                            if let Some(r) = right {
                                format!("{} - {}", left, r)
                            } else {
                                format!("{}", left)
                            }
                        )));
                        label.add_css_class("dim-label");
                        loading_box.append(&spinner);
                        loading_box.append(&label);

                        let left_loaded = self.textures.get(&left);
                        let right_loaded = right.and_then(|r| self.textures.get(&r));

                        let all_needed_loaded = if right.is_some() {
                            left_loaded.is_some() && right_loaded.is_some()
                        } else {
                            left_loaded.is_some()
                        };

                        if all_needed_loaded {
                            if let Some((tex, w, h)) = left_loaded {
                                left_pic.set_paintable(Some(tex));
                                left_pic.set_size_request(*w, *h);
                                left_pic.set_visible(true);
                            }
                            if right.is_some() {
                                if let Some((tex, w, h)) = right_loaded {
                                    right_pic.set_paintable(Some(tex));
                                    right_pic.set_size_request(*w, *h);
                                    right_pic.set_visible(true);
                                }
                            } else {
                                right_pic.set_visible(false);
                            }
                            loading_box.set_visible(false);
                        } else {
                            left_pic.set_visible(false);
                            right_pic.set_visible(false);
                            loading_box.set_visible(true);
                        }

                        spread_box.append(&left_pic);
                        spread_box.append(&right_pic);
                        container.append(&spread_box);
                        container.append(&loading_box);

                        self.two_page_left_pic = Some(left_pic);
                        self.two_page_right_pic = Some(right_pic);
                        self.two_page_loading_box = Some(loading_box);
                        self.two_page_label = Some(label);

                        self.paged_picture = None;
                        self.paged_loading_box = None;
                        self.paged_label = None;
                        self.page_pictures.clear();

                        container.upcast()
                    }
                }
            }
            PdfScrollFlow::Continuous => {
                let container = gtk::Box::new(gtk::Orientation::Vertical, 20);
                container.set_halign(gtk::Align::Center);
                container.set_valign(gtk::Align::Start);
                container.set_hexpand(true);
                container.set_vexpand(false);
                container.set_margin_top(56);
                container.set_margin_bottom(88);
                container.set_margin_start(16);
                container.set_margin_end(16);

                self.page_pictures.clear();
                self.paged_picture = None;
                self.paged_loading_box = None;
                self.paged_label = None;
                self.two_page_left_pic = None;
                self.two_page_right_pic = None;
                self.two_page_loading_box = None;
                self.two_page_label = None;

                match self.page_layout {
                    PdfPageLayout::Single => {
                        for p in 1..=self.total_pages {
                            let page_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
                            page_box.set_halign(gtk::Align::Center);

                            let pic = gtk::Picture::new();
                            pic.set_can_shrink(true);
                            pic.set_content_fit(gtk::ContentFit::Contain);
                            pic.set_halign(gtk::Align::Center);

                            if let Some((tex, w, h)) = self.textures.get(&p) {
                                pic.set_paintable(Some(tex));
                                pic.set_size_request(*w, *h);
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
                    }
                    PdfPageLayout::TwoPage => {
                        for (left, right) in self.all_spreads() {
                            let spread_row = gtk::Box::new(gtk::Orientation::Horizontal, self.two_page_gap);
                            spread_row.set_halign(gtk::Align::Center);
                            spread_row.set_valign(gtk::Align::Start);
                            spread_row.add_css_class("kalam-pdf-spread-row");

                            let left_pic = gtk::Picture::new();
                            left_pic.set_can_shrink(true);
                            left_pic.set_content_fit(gtk::ContentFit::Contain);
                            left_pic.set_halign(if right.is_some() { gtk::Align::End } else { gtk::Align::Center });
                            left_pic.add_css_class("kalam-pdf-two-page");

                            if let Some((tex, w, h)) = self.textures.get(&left) {
                                left_pic.set_paintable(Some(tex));
                                left_pic.set_size_request(*w, *h);
                                left_pic.add_css_class("kalam-pdf-page-image");
                            } else {
                                let est_w = (600.0 * self.zoom_level) as i32;
                                let est_h = (800.0 * self.zoom_level) as i32;
                                left_pic.set_size_request(est_w, est_h);
                                left_pic.add_css_class("kalam-pdf-placeholder");
                            }
                            spread_row.append(&left_pic);
                            self.page_pictures.insert(left, left_pic);

                            if let Some(r) = right {
                                let right_pic = gtk::Picture::new();
                                right_pic.set_can_shrink(true);
                                right_pic.set_content_fit(gtk::ContentFit::Contain);
                                right_pic.set_halign(gtk::Align::Start);
                                right_pic.add_css_class("kalam-pdf-two-page");

                                if let Some((tex, w, h)) = self.textures.get(&r) {
                                    right_pic.set_paintable(Some(tex));
                                    right_pic.set_size_request(*w, *h);
                                    right_pic.add_css_class("kalam-pdf-page-image");
                                } else {
                                    let est_w = (600.0 * self.zoom_level) as i32;
                                    let est_h = (800.0 * self.zoom_level) as i32;
                                    right_pic.set_size_request(est_w, est_h);
                                    right_pic.add_css_class("kalam-pdf-placeholder");
                                }
                                spread_row.append(&right_pic);
                                self.page_pictures.insert(r, right_pic);
                            }

                            container.append(&spread_row);
                        }
                    }
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
            pic.set_size_request(*w, *h);
            pic.set_visible(true);
            loading_box.set_visible(false);
            pic.queue_resize();
            pic.queue_draw();
        } else {
            pic.set_visible(false);
            loading_box.set_visible(true);
            label.set_label(&format!("Rendering page {}...", self.current_page));
        }
    }

    /// Fast, in-place update for TwoPage mode when page changes or renders.
    pub fn update_two_page_view(&self) {
        let (Some(left_pic), Some(right_pic), Some(loading_box), Some(label)) = (
            &self.two_page_left_pic,
            &self.two_page_right_pic,
            &self.two_page_loading_box,
            &self.two_page_label,
        ) else {
            return;
        };

        let (left, right) = self.spread_for_page(self.current_page);
        let left_loaded = self.textures.get(&left);
        let right_loaded = right.and_then(|r| self.textures.get(&r));

        let all_needed_loaded = if right.is_some() {
            left_loaded.is_some() && right_loaded.is_some()
        } else {
            left_loaded.is_some()
        };

        if all_needed_loaded {
            if let Some((tex, w, h)) = left_loaded {
                left_pic.set_paintable(Some(tex));
                left_pic.set_size_request(*w, *h);
                left_pic.set_halign(if right.is_some() { gtk::Align::End } else { gtk::Align::Center });
                left_pic.set_visible(true);
            }
            if right.is_some() {
                if let Some((tex, w, h)) = right_loaded {
                    right_pic.set_paintable(Some(tex));
                    right_pic.set_size_request(*w, *h);
                    right_pic.set_halign(gtk::Align::Start);
                    right_pic.set_visible(true);
                }
            } else {
                right_pic.set_visible(false);
            }
            loading_box.set_visible(false);
            left_pic.queue_resize();
            left_pic.queue_draw();
            right_pic.queue_resize();
            right_pic.queue_draw();
        } else {
            left_pic.set_visible(false);
            right_pic.set_visible(false);
            loading_box.set_visible(true);
            if let Some(r) = right {
                label.set_label(&format!("Rendering pages {}-{}...", left, r));
            } else {
                label.set_label(&format!("Rendering page {}...", left));
            }
        }
    }

    /// Smooth in-place zoom adjustment without widget recreation or SIGSEGV crashes.
    pub fn apply_zoom_change(&mut self, sender: &ComponentSender<Self>, scroll: &gtk::ScrolledWindow) {
        self.textures.clear();
        self.pending_loads.clear();

        match self.scroll_flow {
            PdfScrollFlow::Discrete => {
                match self.page_layout {
                    PdfPageLayout::Single => {
                        self.update_paged_view();
                    }
                    PdfPageLayout::TwoPage => {
                        self.update_two_page_view();
                    }
                }
            }
            PdfScrollFlow::Continuous => {
                let est_w = (600.0 * self.zoom_level) as i32;
                let est_h = (800.0 * self.zoom_level) as i32;
                for pic in self.page_pictures.values() {
                    pic.set_paintable(None::<&gdk::Paintable>);
                    pic.set_size_request(est_w, est_h);
                    pic.remove_css_class("kalam-pdf-page-image");
                    pic.add_css_class("kalam-pdf-placeholder");
                    pic.queue_resize();
                    pic.queue_draw();
                }
                scroll.queue_draw();
            }
        }

        self.trigger_loads(sender);
    }

    /// Scroll to current page ratio in continuous flow.
    pub fn scroll_to_current_page(&self, scroll: &gtk::ScrolledWindow) {
        if self.scroll_flow == PdfScrollFlow::Continuous && self.total_pages > 1 {
            let vadj = scroll.vadjustment();
            let max = (vadj.upper() - vadj.page_size()).max(0.0);
            if max > 0.0 {
                let ratio = (self.current_page.saturating_sub(1)) as f64 / (self.total_pages - 1) as f64;
                vadj.set_value((ratio * max).clamp(vadj.lower(), max));
            }
        }
    }

    /// Synchronize settings controls with active state.
    pub fn sync_settings_ui(&self, sw: &PdfSettingsWidgets) {
        toggle_active(&sw.layout_single_btn, self.page_layout == PdfPageLayout::Single);
        toggle_active(&sw.layout_two_page_btn, self.page_layout == PdfPageLayout::TwoPage);

        toggle_active(&sw.flow_continuous_btn, self.scroll_flow == PdfScrollFlow::Continuous);
        toggle_active(&sw.flow_discrete_btn, self.scroll_flow == PdfScrollFlow::Discrete);

        if sw.cover_alone_switch.is_active() != self.cover_alone {
            sw.cover_alone_switch.set_active(self.cover_alone);
        }
        sw.spread_settings_box.set_visible(self.page_layout == PdfPageLayout::TwoPage);

        toggle_active(&sw.gap_0_btn, self.two_page_gap == 0);
        toggle_active(&sw.gap_4_btn, self.two_page_gap == 4);
        toggle_active(&sw.gap_8_btn, self.two_page_gap == 8);
        toggle_active(&sw.gap_12_btn, self.two_page_gap == 12);
        toggle_active(&sw.gap_16_btn, self.two_page_gap == 16);

        if sw.smart_crop_switch.is_active() != self.smart_crop {
            sw.smart_crop_switch.set_active(self.smart_crop);
        }
        sw.zoom_label.set_label(&format!("{}%", (self.zoom_level * 100.0).round() as i32));
    }
}

impl Drop for PdfReaderModel {
    fn drop(&mut self) {
        self.close_session();
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

            // ── 3. Overlay: Invisible Left Edge Hover Strip ────────────
            add_overlay = &gtk::Box {
                add_css_class: "kalam-reader-hover-edge",
                add_css_class: "kalam-reader-hover-edge-left",
                set_width_request: 18,
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
                                set_label: &model.page_indicator_text(),
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
                    },
                },
            },

            // ── 6. Overlay: Slide-in Sidebar (TOC + Settings) ─────────
            add_overlay = &gtk::Revealer {
                add_css_class: "kalam-reader-sidebar-shell",
                add_css_class: "kalam-reader-sidebar-shell-left",
                #[watch]
                set_reveal_child: model.show_sidebar,
                set_transition_type: gtk::RevealerTransitionType::SlideRight,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Fill,
                set_margin_start: 8,
                set_margin_top: 8,
                set_margin_bottom: 8,

                #[name = "left_sidebar_box"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_size_request: (340, -1),
                    add_css_class: "kalam-reader-sidebar",
                    add_css_class: "kalam-reader-sidebar-left",

                    // Book Head (Cover + Title + Author + Progress)
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
                                set_max_width_chars: 24,
                                set_lines: 2,
                                set_xalign: 0.0,
                            },

                            gtk::Label {
                                add_css_class: "dim-label",
                                #[watch]
                                set_label: &model.author,
                                set_halign: gtk::Align::Start,
                                set_ellipsize: gtk::pango::EllipsizeMode::End,
                                set_max_width_chars: 24,
                                set_xalign: 0.0,
                            },

                            gtk::ProgressBar {
                                add_css_class: "kalam-reader-progress",
                                set_show_text: false,
                                #[watch]
                                set_fraction: model.progress_fraction(),
                            },
                        },
                    },

                    // Main Content Host (Stack populated in init)
                    #[name = "left_panel_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Vertical,
                        set_hexpand: true,
                        set_vexpand: true,
                    },

                    // Bottom Tabbar (TOC and Settings buttons)
                    gtk::Box {
                        add_css_class: "kalam-reader-tabbar",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 2,
                        set_homogeneous: true,

                        #[name = "tab_toc_btn"]
                        gtk::Button {
                            set_child: Some(&reader_sidebar_tab_content("view-list-bullet-symbolic", "TOC")),
                            add_css_class: "kalam-reader-tab",
                            set_hexpand: true,
                            connect_clicked => PdfReaderMsg::SwitchSidebarTab(PdfSidebarTab::Toc),
                        },

                        #[name = "tab_settings_btn"]
                        gtk::Button {
                            set_child: Some(&reader_sidebar_tab_content("preferences-system-symbolic", "Settings")),
                            add_css_class: "kalam-reader-tab",
                            set_hexpand: true,
                            connect_clicked => PdfReaderMsg::SwitchSidebarTab(PdfSidebarTab::Settings),
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

        // 2. Set up Left Sidebar Stack (TOC + Settings)
        let left_stack = gtk::Stack::new();
        left_stack.set_hexpand(true);
        left_stack.set_vexpand(true);
        left_stack.set_transition_type(gtk::StackTransitionType::Crossfade);

        let toc_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .build();
        toc_scroll.add_css_class("kalam-reader-panel-scroll");
        let toc_list_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        toc_list_box.set_hexpand(true);
        toc_list_box.set_vexpand(true);
        toc_scroll.set_child(Some(&toc_list_box));

        let settings_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .build();
        settings_scroll.add_css_class("kalam-reader-panel-scroll");

        let (settings_box, settings_widgets) = build_pdf_settings_panel(&model, &sender);
        settings_scroll.set_child(Some(&settings_box));

        left_stack.add_named(&toc_scroll, Some("toc"));
        left_stack.add_named(&settings_scroll, Some("settings"));
        widgets.left_panel_host.append(&left_stack);

        populate_toc_list(
            &toc_list_box,
            &model.toc_entries,
            model.current_page,
            model.total_pages,
            &sender,
        );

        model.left_stack = Some(left_stack);
        model.toc_list_box = Some(toc_list_box);
        model.settings_widgets = Some(settings_widgets);

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
            let left = x < 25.0;

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

        // 8. Scroll controller to autohide controls on scrolling or zoom on Ctrl+scroll
        let scroll_ctrl = gtk::EventControllerScroll::new(
            gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::HORIZONTAL,
        );
        let tx_sc = tx.clone();
        let tx_ctrl_zoom = tx.clone();
        scroll_ctrl.connect_scroll(move |controller, _dx, dy| {
            let state = controller.current_event_state();
            if state.contains(gdk::ModifierType::CONTROL_MASK) {
                if dy < -0.1 {
                    let _ = tx_ctrl_zoom.send(PdfReaderMsg::ZoomIn);
                } else if dy > 0.1 {
                    let _ = tx_ctrl_zoom.send(PdfReaderMsg::ZoomOut);
                }
                return gtk::glib::Propagation::Stop;
            }
            let _ = tx_sc.send(PdfReaderMsg::UserScrolled);
            gtk::glib::Propagation::Proceed
        });
        widgets.viewport_scroll.add_controller(scroll_ctrl);

        // 9. Track continuous scroll position changes to update current_page
        let vadj = widgets.viewport_scroll.vadjustment();
        let tx_vadj = tx.clone();
        vadj.connect_value_changed(move |_| {
            let _ = tx_vadj.send(PdfReaderMsg::UpdateScrollPage(0));
        });
        let tx_vadj_changed = tx.clone();
        vadj.connect_changed(move |_| {
            let _ = tx_vadj_changed.send(PdfReaderMsg::UpdateScrollPage(0));
        });

        // 10. Click / tap navigation: left quarter = prev, right quarter = next, center = toggle controls
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
                    let _ = tx_click.send(PdfReaderMsg::ToggleControls);
                }
            }
        });
        widgets.viewport_scroll.add_controller(click);

        // 11. Touchpad pinch-to-zoom gesture
        let zoom_gesture = gtk::GestureZoom::new();
        let tx_zoom = tx.clone();
        let last_scale = std::rc::Rc::new(std::cell::Cell::new(1.0f64));
        let ls_begin = last_scale.clone();
        zoom_gesture.connect_begin(move |_, _| {
            ls_begin.set(1.0);
        });
        let ls_change = last_scale.clone();
        zoom_gesture.connect_scale_changed(move |_, scale| {
            let prev = ls_change.get();
            let delta = scale - prev;
            if delta.abs() > 0.08 {
                ls_change.set(scale);
                if delta > 0.0 {
                    let _ = tx_zoom.send(PdfReaderMsg::ZoomIn);
                } else {
                    let _ = tx_zoom.send(PdfReaderMsg::ZoomOut);
                }
            }
        });
        widgets.viewport_scroll.add_controller(zoom_gesture);

        // Initial child and render triggering
        let child = model.build_viewport_widget();
        widgets.viewport_scroll.set_child(Some(&child));

        // Restore scroll position in continuous mode if resuming past page 1
        if model.scroll_flow == PdfScrollFlow::Continuous && model.current_page > 1 && model.total_pages > 1 {
            let cur = model.current_page;
            let total = model.total_pages;
            let vadj = widgets.viewport_scroll.vadjustment();
            glib::idle_add_local_once(move || {
                let max = (vadj.upper() - vadj.page_size()).max(0.0);
                if max > 0.0 {
                    let ratio = (cur - 1) as f64 / (total - 1) as f64;
                    vadj.set_value(ratio * max);
                }
            });
        }

        model.trigger_loads(&sender);

        // Schedule initial hide of back button and bottom pill after 2.5s
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
                match self.scroll_flow {
                    PdfScrollFlow::Discrete => {
                        match self.page_layout {
                            PdfPageLayout::TwoPage => {
                                let (left, _) = self.spread_for_page(self.current_page);
                                if self.cover_alone && left <= 1 {
                                    if self.total_pages >= 2 {
                                        self.current_page = 2;
                                    }
                                } else if left + 2 <= self.total_pages {
                                    self.current_page = left + 2;
                                } else if left < self.total_pages {
                                    self.current_page = left + 1;
                                }
                                self.save_progress();
                                self.trigger_loads(&sender);
                                self.update_two_page_view();
                            }
                            PdfPageLayout::Single => {
                                if self.current_page < self.total_pages {
                                    self.current_page += 1;
                                    self.save_progress();
                                    self.trigger_loads(&sender);
                                    self.update_paged_view();
                                }
                            }
                        }
                    }
                    PdfScrollFlow::Continuous => {
                        let step = if self.page_layout == PdfPageLayout::TwoPage { 2 } else { 1 };
                        if self.current_page < self.total_pages {
                            self.current_page = (self.current_page + step).min(self.total_pages);
                            self.save_progress();
                            self.trigger_loads(&sender);
                            self.scroll_to_current_page(&widgets.viewport_scroll);
                        }
                    }
                }
            }
            PdfReaderMsg::PrevPage => {
                match self.scroll_flow {
                    PdfScrollFlow::Discrete => {
                        match self.page_layout {
                            PdfPageLayout::TwoPage => {
                                let (left, _) = self.spread_for_page(self.current_page);
                                if self.cover_alone {
                                    if left <= 2 {
                                        self.current_page = 1;
                                    } else {
                                        self.current_page = left.saturating_sub(2).max(2);
                                    }
                                } else {
                                    self.current_page = left.saturating_sub(2).max(1);
                                }
                                self.save_progress();
                                self.trigger_loads(&sender);
                                self.update_two_page_view();
                            }
                            PdfPageLayout::Single => {
                                if self.current_page > 1 {
                                    self.current_page -= 1;
                                    self.save_progress();
                                    self.trigger_loads(&sender);
                                    self.update_paged_view();
                                }
                            }
                        }
                    }
                    PdfScrollFlow::Continuous => {
                        let step = if self.page_layout == PdfPageLayout::TwoPage { 2 } else { 1 };
                        if self.current_page > 1 {
                            self.current_page = self.current_page.saturating_sub(step).max(1);
                            self.save_progress();
                            self.trigger_loads(&sender);
                            self.scroll_to_current_page(&widgets.viewport_scroll);
                        }
                    }
                }
            }
            PdfReaderMsg::ZoomIn => {
                self.zoom_level = (self.zoom_level + 0.15).min(3.0);
                self.apply_zoom_change(&sender, &widgets.viewport_scroll);
            }
            PdfReaderMsg::ZoomOut => {
                self.zoom_level = (self.zoom_level - 0.15).max(0.4);
                self.apply_zoom_change(&sender, &widgets.viewport_scroll);
            }
            PdfReaderMsg::ResetZoom => {
                self.zoom_level = 1.0;
                self.apply_zoom_change(&sender, &widgets.viewport_scroll);
            }
            PdfReaderMsg::SetPageLayout(layout) => {
                if self.page_layout != layout {
                    self.page_layout = layout;
                    self.catalog.set_pref(
                        "reader.pdf.layout",
                        match self.page_layout {
                            PdfPageLayout::Single => "single",
                            PdfPageLayout::TwoPage => "two_page",
                        },
                    );
                    self.catalog.set_pref(
                        "reader.pdf.view_mode",
                        match (self.page_layout, self.scroll_flow) {
                            (PdfPageLayout::TwoPage, _) => "two_page",
                            (PdfPageLayout::Single, PdfScrollFlow::Continuous) => "continuous",
                            (PdfPageLayout::Single, PdfScrollFlow::Discrete) => "paged",
                        },
                    );
                    let child = self.build_viewport_widget();
                    widgets.viewport_scroll.set_child(Some(&child));
                    self.trigger_loads(&sender);
                    if self.scroll_flow == PdfScrollFlow::Continuous && self.current_page > 1 {
                        self.scroll_to_current_page(&widgets.viewport_scroll);
                    }
                }
            }
            PdfReaderMsg::SetScrollFlow(flow) => {
                if self.scroll_flow != flow {
                    self.scroll_flow = flow;
                    self.catalog.set_pref(
                        "reader.pdf.flow",
                        match self.scroll_flow {
                            PdfScrollFlow::Continuous => "continuous",
                            PdfScrollFlow::Discrete => "discrete",
                        },
                    );
                    self.catalog.set_pref_i64(
                        "reader.pdf.continuous",
                        if self.scroll_flow == PdfScrollFlow::Continuous { 1 } else { 0 },
                    );
                    self.catalog.set_pref(
                        "reader.pdf.view_mode",
                        match (self.page_layout, self.scroll_flow) {
                            (PdfPageLayout::TwoPage, _) => "two_page",
                            (PdfPageLayout::Single, PdfScrollFlow::Continuous) => "continuous",
                            (PdfPageLayout::Single, PdfScrollFlow::Discrete) => "paged",
                        },
                    );
                    let child = self.build_viewport_widget();
                    widgets.viewport_scroll.set_child(Some(&child));
                    self.trigger_loads(&sender);
                    if self.scroll_flow == PdfScrollFlow::Continuous && self.current_page > 1 {
                        self.scroll_to_current_page(&widgets.viewport_scroll);
                    }
                }
            }
            PdfReaderMsg::SetTwoPageGap(gap) => {
                let clamped = gap.clamp(0, 48);
                if self.two_page_gap != clamped {
                    self.two_page_gap = clamped;
                    self.catalog.set_pref_i64("reader.pdf.two_page_gap", clamped as i64);
                    if self.page_layout == PdfPageLayout::TwoPage {
                        let child = self.build_viewport_widget();
                        widgets.viewport_scroll.set_child(Some(&child));
                        self.trigger_loads(&sender);
                        if self.scroll_flow == PdfScrollFlow::Continuous && self.current_page > 1 {
                            self.scroll_to_current_page(&widgets.viewport_scroll);
                        }
                    }
                }
            }
            PdfReaderMsg::ToggleContinuousMode => {
                let next = match self.scroll_flow {
                    PdfScrollFlow::Continuous => PdfScrollFlow::Discrete,
                    PdfScrollFlow::Discrete => PdfScrollFlow::Continuous,
                };
                let _ = sender.input_sender().send(PdfReaderMsg::SetScrollFlow(next));
            }
            PdfReaderMsg::SetCoverAlone(alone) => {
                if self.cover_alone != alone {
                    self.cover_alone = alone;
                    self.catalog.set_pref_i64(
                        "reader.pdf.cover_alone",
                        if self.cover_alone { 1 } else { 0 },
                    );
                    if self.page_layout == PdfPageLayout::TwoPage {
                        let child = self.build_viewport_widget();
                        widgets.viewport_scroll.set_child(Some(&child));
                        self.trigger_loads(&sender);
                        if self.scroll_flow == PdfScrollFlow::Continuous && self.current_page > 1 {
                            self.scroll_to_current_page(&widgets.viewport_scroll);
                        }
                    }
                }
            }
            PdfReaderMsg::SetSmartCrop(crop) => {
                if self.smart_crop != crop {
                    self.smart_crop = crop;
                    self.catalog.set_pref(
                        &format!("book.{}.pdf.smart_crop", self.book_id),
                        if self.smart_crop { "1" } else { "0" },
                    );
                    self.apply_zoom_change(&sender, &widgets.viewport_scroll);
                }
            }
            PdfReaderMsg::ToggleSmartCrop => {
                let next = !self.smart_crop;
                let _ = sender.input_sender().send(PdfReaderMsg::SetSmartCrop(next));
            }
            PdfReaderMsg::SwitchSidebarTab(tab) => {
                self.sidebar_tab = tab;
                if tab == PdfSidebarTab::Toc {
                    if let Some(ref list) = self.toc_list_box {
                        populate_toc_list(
                            list,
                            &self.toc_entries,
                            self.current_page,
                            self.total_pages,
                            &sender,
                        );
                    }
                }
            }
            PdfReaderMsg::ToggleSidebar => {
                self.show_sidebar = !self.show_sidebar;
                if self.show_sidebar {
                    self.show_back_button = false;
                    if let Some(ref list) = self.toc_list_box {
                        populate_toc_list(
                            list,
                            &self.toc_entries,
                            self.current_page,
                            self.total_pages,
                            &sender,
                        );
                    }
                }
            }
            PdfReaderMsg::CloseSidebar => {
                self.show_sidebar = false;
                self.mouse_in_sidebar = false;
                self.mouse_in_left_edge = false;
            }
            PdfReaderMsg::ToggleControls => {
                if self.show_bottom_pill || self.show_back_button {
                    self.show_bottom_pill = false;
                    self.show_back_button = false;
                } else {
                    self.show_bottom_pill = true;
                    if !self.show_sidebar {
                        self.show_back_button = true;
                    }
                    self.schedule_bottom_hide(&sender);
                    self.schedule_back_hide(&sender);
                }
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
                    match self.scroll_flow {
                        PdfScrollFlow::Discrete => {
                            match self.page_layout {
                                PdfPageLayout::Single => self.update_paged_view(),
                                PdfPageLayout::TwoPage => self.update_two_page_view(),
                            }
                        }
                        PdfScrollFlow::Continuous => {
                            self.scroll_to_current_page(&widgets.viewport_scroll);
                        }
                    }
                }
            }
            PdfReaderMsg::EscapeKey => {
                if self.show_sidebar {
                    self.show_sidebar = false;
                    self.mouse_in_sidebar = false;
                    self.mouse_in_left_edge = false;
                } else {
                    self.save_progress();
                    self.close_session();
                    let _ = sender.output(PdfReaderOut::Close);
                }
            }
            PdfReaderMsg::Close => {
                self.save_progress();
                self.close_session();
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

                match self.scroll_flow {
                    PdfScrollFlow::Discrete => {
                        match self.page_layout {
                            PdfPageLayout::Single => {
                                if page == self.current_page {
                                    self.update_paged_view();
                                }
                            }
                            PdfPageLayout::TwoPage => {
                                let (left, right) = self.spread_for_page(self.current_page);
                                if page == left || Some(page) == right {
                                    self.update_two_page_view();
                                }
                            }
                        }
                    }
                    PdfScrollFlow::Continuous => {
                        if let Some(pic) = self.page_pictures.get(&page) {
                            pic.set_paintable(Some(&texture));
                            pic.set_size_request(width, height);
                            pic.remove_css_class("kalam-pdf-placeholder");
                            pic.add_css_class("kalam-pdf-page-image");
                            pic.queue_resize();
                            pic.queue_draw();
                            widgets.viewport_scroll.queue_draw();
                        }
                    }
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
                        if let Some(ref list) = self.toc_list_box {
                            populate_toc_list(
                                list,
                                &self.toc_entries,
                                self.current_page,
                                self.total_pages,
                                &sender,
                            );
                        }
                    }
                } else if !self.mouse_in_sidebar {
                    self.schedule_sidebar_close(&sender);
                }
            }
            PdfReaderMsg::SidebarHover(hovering) => {
                self.mouse_in_sidebar = hovering;
                if !hovering && !self.mouse_in_left_edge {
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
                if seq == self.sidebar_close_seq && !self.mouse_in_sidebar && !self.mouse_in_left_edge {
                    self.show_sidebar = false;
                }
            }
            PdfReaderMsg::UserScrolled => {
                if self.show_bottom_pill {
                    self.schedule_bottom_hide(&sender);
                }
                if self.show_back_button && !self.mouse_in_top_edge {
                    self.schedule_back_hide(&sender);
                }
            }
            PdfReaderMsg::ScrollDelta(dir) => {
                if self.scroll_flow == PdfScrollFlow::Continuous {
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
                if self.scroll_flow == PdfScrollFlow::Continuous && self.total_pages > 1 {
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

        if let Some(ref stack) = self.left_stack {
            stack.set_visible_child_name(match self.sidebar_tab {
                PdfSidebarTab::Toc => "toc",
                PdfSidebarTab::Settings => "settings",
            });
        }
        toggle_active(&widgets.tab_toc_btn, self.sidebar_tab == PdfSidebarTab::Toc);
        toggle_active(&widgets.tab_settings_btn, self.sidebar_tab == PdfSidebarTab::Settings);
        if let Some(ref sw) = self.settings_widgets {
            self.sync_settings_ui(sw);
        }

        self.update_view(widgets, sender);
    }
}

/// Helper function to populate the sidebar outline list matching the EPUB reader.
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
            lbl.add_css_class("kalam-reader-toc-title");
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

    for item in entries {
        let btn = gtk::Button::new();
        btn.add_css_class("kalam-reader-toc-item");
        if item.page == current_page {
            btn.add_css_class("active");
        }

        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let indent = (item.depth.saturating_sub(1) * 14) as i32;
        row_box.set_margin_start(indent);

        let title_lbl = gtk::Label::new(Some(&item.title));
        title_lbl.set_hexpand(true);
        title_lbl.set_halign(gtk::Align::Start);
        title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title_lbl.set_max_width_chars(28);
        title_lbl.add_css_class("kalam-reader-toc-title");

        let page_lbl = gtk::Label::new(Some(&format!("{}", item.page)));
        page_lbl.add_css_class("dim-label");

        row_box.append(&title_lbl);
        row_box.append(&page_lbl);
        btn.set_child(Some(&row_box));

        let target_page = item.page;
        let tx = sender.input_sender().clone();
        btn.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::JumpToToc(target_page));
        });

        container.append(&btn);
    }
}

/// Build Settings panel inside the slide-in left sidebar.
fn build_pdf_settings_panel(
    model: &PdfReaderModel,
    sender: &ComponentSender<PdfReaderModel>,
) -> (gtk::Box, PdfSettingsWidgets) {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 16);
    wrap.set_margin_start(16);
    wrap.set_margin_end(16);
    wrap.set_margin_top(16);
    wrap.set_margin_bottom(24);

    // 1. Layout & Flow Section
    let layout_section = reader_settings_section("Layout & Flow");

    let layout_title = gtk::Label::new(Some("Page Layout"));
    layout_title.add_css_class("kalam-reader-setting-name");
    layout_title.set_halign(gtk::Align::Start);
    layout_section.append(&layout_title);

    let layout_switcher = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    layout_switcher.add_css_class("kalam-reader-ui-preset-row");
    layout_switcher.set_homogeneous(true);

    let layout_single_btn = gtk::Button::with_label("Single Page");
    layout_single_btn.add_css_class("kalam-reader-filter-chip");
    layout_single_btn.add_css_class("kalam-reader-ui-preset");
    if model.page_layout == PdfPageLayout::Single {
        layout_single_btn.add_css_class("active");
    }
    let tx = sender.input_sender().clone();
    layout_single_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetPageLayout(PdfPageLayout::Single));
    });

    let layout_two_page_btn = gtk::Button::with_label("Two-Page Spread");
    layout_two_page_btn.add_css_class("kalam-reader-filter-chip");
    layout_two_page_btn.add_css_class("kalam-reader-ui-preset");
    if model.page_layout == PdfPageLayout::TwoPage {
        layout_two_page_btn.add_css_class("active");
    }
    let tx = sender.input_sender().clone();
    layout_two_page_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetPageLayout(PdfPageLayout::TwoPage));
    });

    layout_switcher.append(&layout_single_btn);
    layout_switcher.append(&layout_two_page_btn);
    layout_section.append(&layout_switcher);

    let flow_title = gtk::Label::new(Some("Scroll Flow"));
    flow_title.add_css_class("kalam-reader-setting-name");
    flow_title.set_halign(gtk::Align::Start);
    flow_title.set_margin_top(10);
    layout_section.append(&flow_title);

    let flow_switcher = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    flow_switcher.add_css_class("kalam-reader-ui-preset-row");
    flow_switcher.set_homogeneous(true);

    let flow_continuous_btn = gtk::Button::with_label("Continuous Stream");
    flow_continuous_btn.add_css_class("kalam-reader-filter-chip");
    flow_continuous_btn.add_css_class("kalam-reader-ui-preset");
    if model.scroll_flow == PdfScrollFlow::Continuous {
        flow_continuous_btn.add_css_class("active");
    }
    let tx = sender.input_sender().clone();
    flow_continuous_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetScrollFlow(PdfScrollFlow::Continuous));
    });

    let flow_discrete_btn = gtk::Button::with_label("Discrete (Paged)");
    flow_discrete_btn.add_css_class("kalam-reader-filter-chip");
    flow_discrete_btn.add_css_class("kalam-reader-ui-preset");
    if model.scroll_flow == PdfScrollFlow::Discrete {
        flow_discrete_btn.add_css_class("active");
    }
    let tx = sender.input_sender().clone();
    flow_discrete_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetScrollFlow(PdfScrollFlow::Discrete));
    });

    flow_switcher.append(&flow_continuous_btn);
    flow_switcher.append(&flow_discrete_btn);
    layout_section.append(&flow_switcher);

    // Spread Settings (Only active/visible when layout is TwoPage)
    let spread_settings_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    spread_settings_box.set_visible(model.page_layout == PdfPageLayout::TwoPage);

    let cover_alone_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    cover_alone_row.add_css_class("kalam-reader-setting-row");
    let cover_label = gtk::Label::new(Some("First page as single cover"));
    cover_label.add_css_class("kalam-reader-setting-name");
    cover_label.set_hexpand(true);
    cover_label.set_halign(gtk::Align::Start);
    cover_alone_row.append(&cover_label);

    let tx = sender.input_sender().clone();
    let cover_alone_switch = crate::pages::settings::toggle_switch(model.cover_alone, move |on| {
        let _ = tx.send(PdfReaderMsg::SetCoverAlone(on));
    });
    cover_alone_row.append(&cover_alone_switch);
    spread_settings_box.append(&cover_alone_row);

    // Spread Gap (Facing pages separation)
    let gap_title = gtk::Label::new(Some("Spread Gap"));
    gap_title.add_css_class("kalam-reader-setting-name");
    gap_title.set_halign(gtk::Align::Start);
    gap_title.set_margin_top(6);
    spread_settings_box.append(&gap_title);

    let gap_switcher = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    gap_switcher.add_css_class("kalam-reader-ui-preset-row");
    gap_switcher.set_homogeneous(true);

    let gap_0_btn = gtk::Button::with_label("0px");
    gap_0_btn.add_css_class("kalam-reader-filter-chip");
    if model.two_page_gap == 0 { gap_0_btn.add_css_class("active"); }
    let tx = sender.input_sender().clone();
    gap_0_btn.connect_clicked(move |_| { let _ = tx.send(PdfReaderMsg::SetTwoPageGap(0)); });

    let gap_4_btn = gtk::Button::with_label("4px");
    gap_4_btn.add_css_class("kalam-reader-filter-chip");
    if model.two_page_gap == 4 { gap_4_btn.add_css_class("active"); }
    let tx = sender.input_sender().clone();
    gap_4_btn.connect_clicked(move |_| { let _ = tx.send(PdfReaderMsg::SetTwoPageGap(4)); });

    let gap_8_btn = gtk::Button::with_label("8px");
    gap_8_btn.add_css_class("kalam-reader-filter-chip");
    if model.two_page_gap == 8 { gap_8_btn.add_css_class("active"); }
    let tx = sender.input_sender().clone();
    gap_8_btn.connect_clicked(move |_| { let _ = tx.send(PdfReaderMsg::SetTwoPageGap(8)); });

    let gap_12_btn = gtk::Button::with_label("12px");
    gap_12_btn.add_css_class("kalam-reader-filter-chip");
    if model.two_page_gap == 12 { gap_12_btn.add_css_class("active"); }
    let tx = sender.input_sender().clone();
    gap_12_btn.connect_clicked(move |_| { let _ = tx.send(PdfReaderMsg::SetTwoPageGap(12)); });

    let gap_16_btn = gtk::Button::with_label("16px");
    gap_16_btn.add_css_class("kalam-reader-filter-chip");
    if model.two_page_gap == 16 { gap_16_btn.add_css_class("active"); }
    let tx = sender.input_sender().clone();
    gap_16_btn.connect_clicked(move |_| { let _ = tx.send(PdfReaderMsg::SetTwoPageGap(16)); });

    gap_switcher.append(&gap_0_btn);
    gap_switcher.append(&gap_4_btn);
    gap_switcher.append(&gap_8_btn);
    gap_switcher.append(&gap_12_btn);
    gap_switcher.append(&gap_16_btn);
    spread_settings_box.append(&gap_switcher);

    let gap_hint = gtk::Label::new(Some("0px gap gives seamless cross-page spreads and illustrations."));
    gap_hint.add_css_class("kalam-reader-setting-hint");
    gap_hint.set_wrap(true);
    gap_hint.set_xalign(0.0);
    spread_settings_box.append(&gap_hint);

    layout_section.append(&spread_settings_box);
    wrap.append(&layout_section);

    let divider = gtk::Separator::new(gtk::Orientation::Horizontal);
    divider.add_css_class("kalam-section-divider");
    wrap.append(&divider);

    // 2. Display & Crop Section
    let crop_section = reader_settings_section("Display");

    let crop_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    crop_row.add_css_class("kalam-reader-setting-row");
    let crop_label = gtk::Label::new(Some("Smart Crop (Zathura style)"));
    crop_label.add_css_class("kalam-reader-setting-name");
    crop_label.set_hexpand(true);
    crop_label.set_halign(gtk::Align::Start);
    crop_row.append(&crop_label);

    let tx = sender.input_sender().clone();
    let smart_crop_switch = crate::pages::settings::toggle_switch(model.smart_crop, move |on| {
        let _ = tx.send(PdfReaderMsg::SetSmartCrop(on));
    });
    crop_row.append(&smart_crop_switch);
    crop_section.append(&crop_row);

    let crop_hint = gtk::Label::new(Some(
        "Auto-detects margins around ink content to maximize reading area on screen.",
    ));
    crop_hint.add_css_class("kalam-reader-setting-hint");
    crop_hint.set_wrap(true);
    crop_hint.set_xalign(0.0);
    crop_section.append(&crop_hint);

    wrap.append(&crop_section);

    let divider2 = gtk::Separator::new(gtk::Orientation::Horizontal);
    divider2.add_css_class("kalam-section-divider");
    wrap.append(&divider2);

    // 3. Zoom Section
    let zoom_section = reader_settings_section("Zoom");

    let zoom_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    zoom_row.add_css_class("kalam-reader-setting-row");
    let zoom_name = gtk::Label::new(Some("Magnification"));
    zoom_name.add_css_class("kalam-reader-setting-name");
    zoom_name.set_hexpand(true);
    zoom_name.set_halign(gtk::Align::Start);
    zoom_row.append(&zoom_name);

    let zoom_stepper = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    zoom_stepper.add_css_class("kalam-reader-stepper");

    let minus_btn = gtk::Button::new();
    minus_btn.add_css_class("kalam-reader-stepper-btn");
    minus_btn.set_size_request(30, 30);
    minus_btn.set_child(Some(&crate::icons::symbolic_with_classes(
        "list-remove-symbolic",
        14,
        &["kalam-inline-icon"],
    )));
    let tx = sender.input_sender().clone();
    minus_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::ZoomOut);
    });
    zoom_stepper.append(&minus_btn);

    let zoom_label = gtk::Label::new(Some(&format!(
        "{}%",
        (model.zoom_level * 100.0).round() as i32
    )));
    zoom_label.add_css_class("kalam-reader-stepper-value");
    zoom_stepper.append(&zoom_label);

    let plus_btn = gtk::Button::new();
    plus_btn.add_css_class("kalam-reader-stepper-btn");
    plus_btn.set_size_request(30, 30);
    plus_btn.set_child(Some(&crate::icons::symbolic_with_classes(
        "list-add-symbolic",
        14,
        &["kalam-inline-icon"],
    )));
    let tx = sender.input_sender().clone();
    plus_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::ZoomIn);
    });
    zoom_stepper.append(&plus_btn);

    let reset_btn = gtk::Button::with_label("100%");
    reset_btn.add_css_class("flat");
    let tx = sender.input_sender().clone();
    reset_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::ResetZoom);
    });
    zoom_stepper.append(&reset_btn);

    zoom_row.append(&zoom_stepper);
    zoom_section.append(&zoom_row);

    wrap.append(&zoom_section);

    let widgets = PdfSettingsWidgets {
        layout_single_btn,
        layout_two_page_btn,
        flow_continuous_btn,
        flow_discrete_btn,
        cover_alone_switch,
        gap_0_btn,
        gap_4_btn,
        gap_8_btn,
        gap_12_btn,
        gap_16_btn,
        spread_settings_box,
        smart_crop_switch,
        zoom_label,
    };

    (wrap, widgets)
}

fn reader_settings_section(label: &str) -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 8);
    section.add_css_class("kalam-reader-section");
    let heading = gtk::Label::new(Some(label));
    heading.add_css_class("kalam-reader-section-label");
    heading.set_halign(gtk::Align::Start);
    section.append(&heading);
    section
}

fn reader_sidebar_tab_content(icon: &str, label: &str) -> gtk::Box {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 3);
    box_.set_halign(gtk::Align::Center);
    box_.set_hexpand(true);
    box_.append(&crate::icons::symbolic_with_classes(
        icon,
        17,
        &["kalam-inline-icon"],
    ));
    let label_widget = gtk::Label::new(Some(label));
    label_widget.set_halign(gtk::Align::Center);
    box_.append(&label_widget);
    box_
}

fn toggle_active(button: &gtk::Button, active: bool) {
    if active {
        button.add_css_class("active");
    } else {
        button.remove_css_class("active");
    }
}
