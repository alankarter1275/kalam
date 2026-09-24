//! PDF Reader Component for Kalam.
//!
//! Native PDF reader built on MuPDF rasterization, offering:
//! - Standard PDF viewer modes matching Firefox & Evince:
//!   - Scrolling: Page Scrolling, Vertical Scrolling, Horizontal Scrolling, Wrapped Scrolling
//!   - Spreads: No Spreads, Odd Spreads (Cover alone), Even Spreads (Side-by-side)
//!   - Configurable spread gap: 0px (Seamless for manga/diagrams), 4px, 8px, 12px, 16px
//! - Full Bookmarks support: view saved bookmarks in sidebar, one-click jump, delete, bottom pill toggle (B key)
//! - Top font cut-off protection with Top-aligned containers, safety margins, and dynamic containment
//! - Autohiding edge hover navigation pills matching Kalam's EPUB reader chrome
//! - Slide-in Zen-browser style left sidebar containing TOC, Bookmarks, and Settings
//! - Per-book preferences and global defaults persistence in catalog
//! - Reading history, session time tracking, and catalog persistence for "Continue Reading" on Home

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use relm4::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::db::{Catalog, ReadingBookmark};
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

/// Scrolling modes matching standard PDF viewers (Firefox / Evince):
/// - PageScrolling: Discrete flips, page-at-a-time
/// - VerticalScrolling: Continuous vertical stream
/// - HorizontalScrolling: Continuous horizontal stream
/// - WrappedScrolling: Multi-column wrapped grid flow
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfScrollMode {
    PageScrolling,
    VerticalScrolling,
    HorizontalScrolling,
    WrappedScrolling,
}

/// Spread modes matching standard PDF viewers:
/// - NoSpreads: Single page layout
/// - OddSpreads: Facing pages with cover alone on page 1 ([1], [2, 3], [4, 5]...)
/// - EvenSpreads: Facing pages starting on page 1 ([1, 2], [3, 4]...)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfSpreadMode {
    NoSpreads,
    OddSpreads,
    EvenSpreads,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfSidebarTab {
    Toc,
    Bookmarks,
    Settings,
}

pub struct PdfSettingsWidgets {
    pub scroll_page_btn: gtk::Button,
    pub scroll_vertical_btn: gtk::Button,
    pub scroll_horizontal_btn: gtk::Button,
    pub scroll_wrapped_btn: gtk::Button,
    pub spread_none_btn: gtk::Button,
    pub spread_odd_btn: gtk::Button,
    pub spread_even_btn: gtk::Button,
    pub gap_0_btn: gtk::Button,
    pub gap_4_btn: gtk::Button,
    pub gap_8_btn: gtk::Button,
    pub gap_12_btn: gtk::Button,
    pub gap_16_btn: gtk::Button,
    pub spread_gap_box: gtk::Box,
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
    FitToPage,
    FitToWidth,
    GoToFirstPage,
    GoToLastPage,
    SetScrollMode(PdfScrollMode),
    SetSpreadMode(PdfSpreadMode),
    SetSpreadGap(i32),
    ToggleBookmark,
    DeleteBookmark(i64),
    JumpToPage(usize),
    SetSmartCrop(bool),
    ToggleSmartCrop,
    SwitchSidebarTab(PdfSidebarTab),
    ToggleSidebar,
    ToggleControls,
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
    pub zoom_level: f64,
    pub scroll_mode: PdfScrollMode,
    pub spread_mode: PdfSpreadMode,
    pub two_page_gap: i32,
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
    pub bookmarks: Vec<ReadingBookmark>,
    pub textures: HashMap<usize, (gdk::Texture, i32, i32)>,
    pub pending_loads: HashSet<usize>,
    pub arrow_step: f32,
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
    pub bookmarks_list_box: Option<gtk::Box>,
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
        let session_id = init
            .catalog
            .start_reading_session(init.book_id, start_pct)
            .ok();
        let session_start = std::time::Instant::now();

        // 1. Scroll Mode: Check per-book setting first, fallback to global default, fallback to VerticalScrolling
        let book_scroll = init.catalog.get_pref(&format!("book.{}.pdf.scroll_mode", init.book_id));
        let global_scroll = init.catalog.get_pref("reader.pdf.scroll_mode");
        let scroll_mode = match book_scroll.as_deref().or(global_scroll.as_deref()) {
            Some("page") => PdfScrollMode::PageScrolling,
            Some("horizontal") => PdfScrollMode::HorizontalScrolling,
            Some("wrapped") => PdfScrollMode::WrappedScrolling,
            _ => PdfScrollMode::VerticalScrolling,
        };

        // 2. Spread Mode: Check per-book setting first, fallback to global default, fallback to NoSpreads
        let book_spread = init.catalog.get_pref(&format!("book.{}.pdf.spread_mode", init.book_id));
        let global_spread = init.catalog.get_pref("reader.pdf.spread_mode");
        let spread_mode = match book_spread.as_deref().or(global_spread.as_deref()) {
            Some("odd") => PdfSpreadMode::OddSpreads,
            Some("even") => PdfSpreadMode::EvenSpreads,
            _ => PdfSpreadMode::NoSpreads,
        };

        // 3. Zoom level: specific to this document
        let book_zoom = init.catalog.get_pref(&format!("book.{}.pdf.zoom", init.book_id));
        let zoom_level = book_zoom
            .and_then(|z| z.parse::<f64>().ok())
            .unwrap_or(1.0)
            .clamp(0.4, 3.0);

        // 4. Two-page spread gap: global default
        let two_page_gap = (init.catalog.get_pref_i64("reader.pdf.two_page_gap", 12) as i32).clamp(0, 48);

        // 5. Smart crop: per-book setting
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
            zoom_level,
            scroll_mode,
            spread_mode,
            two_page_gap,
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
            bookmarks: Vec::new(),
            textures: HashMap::new(),
            pending_loads: HashSet::new(),
            arrow_step,
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
            bookmarks_list_box: None,
            settings_widgets: None,
        };

        model.reload_bookmarks();

        if let Some(ref path) = file_path {
            if let Ok(doc) = PdfDocument::open(path) {
                model.total_pages = doc.page_count();
                if saved_page > model.total_pages {
                    model.current_page = 1;
                }
                model.toc_entries = doc.outlines().unwrap_or_default();
                model.is_loading = false;
                model.status_text.clear();

                let scale = ((1.5 * model.zoom_level).clamp(0.5, 3.5)) as f32;
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

    pub fn reload_bookmarks(&mut self) {
        self.bookmarks = self
            .catalog
            .list_reading_bookmarks(self.book_id)
            .unwrap_or_default();
    }

    pub fn progress_fraction(&self) -> f64 {
        if self.total_pages > 0 {
            (self.current_page as f64 / self.total_pages as f64).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

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

        if let Some(sid) = self.session_id {
            let elapsed = self.session_start.elapsed().as_secs() as i64;
            let end_pct = (fraction * 100.0).round() as i64;
            let _ = self.catalog.checkpoint_reading_session(sid, elapsed, end_pct);
        }
    }

    /// Calculate left and optional right pages for a given target page.
    pub fn spread_for_page(&self, page: usize) -> (usize, Option<usize>) {
        match self.spread_mode {
            PdfSpreadMode::NoSpreads => (page, None),
            PdfSpreadMode::OddSpreads => {
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
            }
            PdfSpreadMode::EvenSpreads => {
                let left = if !page.is_multiple_of(2) { page } else { page - 1 };
                let right = if left < self.total_pages {
                    Some(left + 1)
                } else {
                    None
                };
                (left, right)
            }
        }
    }

    /// Return list of (left, right) page spreads for streaming.
    pub fn all_spreads(&self) -> Vec<(usize, Option<usize>)> {
        match self.spread_mode {
            PdfSpreadMode::NoSpreads => {
                (1..=self.total_pages).map(|p| (p, None)).collect()
            }
            PdfSpreadMode::OddSpreads => {
                let mut list = vec![(1, None)];
                let mut p = 2;
                while p <= self.total_pages {
                    let right = if p < self.total_pages { Some(p + 1) } else { None };
                    list.push((p, right));
                    p += 2;
                }
                list
            }
            PdfSpreadMode::EvenSpreads => {
                let mut list = Vec::new();
                let mut p = 1;
                while p <= self.total_pages {
                    let right = if p < self.total_pages { Some(p + 1) } else { None };
                    list.push((p, right));
                    p += 2;
                }
                list
            }
        }
    }

    pub fn schedule_back_hide(&mut self, sender: &ComponentSender<Self>) {
        self.back_hide_seq = self.back_hide_seq.wrapping_add(1);
        let seq = self.back_hide_seq;
        let s = sender.clone();
        glib::timeout_add_local_once(Duration::from_millis(3000), move || {
            let _ = s.input_sender().send(PdfReaderMsg::BackHideTimerTick(seq));
        });
    }

    pub fn schedule_bottom_hide(&mut self, sender: &ComponentSender<Self>) {
        self.bottom_hide_seq = self.bottom_hide_seq.wrapping_add(1);
        let seq = self.bottom_hide_seq;
        let s = sender.clone();
        glib::timeout_add_local_once(Duration::from_millis(3500), move || {
            let _ = s.input_sender().send(PdfReaderMsg::BottomHideTimerTick(seq));
        });
    }

    pub fn schedule_sidebar_close(&mut self, sender: &ComponentSender<Self>) {
        self.sidebar_close_seq = self.sidebar_close_seq.wrapping_add(1);
        let seq = self.sidebar_close_seq;
        let s = sender.clone();
        glib::timeout_add_local_once(Duration::from_millis(350), move || {
            let _ = s.input_sender().send(PdfReaderMsg::SidebarCloseTimerTick(seq));
        });
    }

    pub fn trigger_loads(&mut self, sender: &ComponentSender<Self>) {
        let Some(ref path) = self.file_path else {
            return;
        };

        let mut load_order = Vec::new();

        match self.scroll_mode {
            PdfScrollMode::PageScrolling => {
                let (left, right) = self.spread_for_page(self.current_page);
                load_order.push(left);
                if let Some(r) = right {
                    load_order.push(r);
                }
                let next_page = if let Some(r) = right { r + 1 } else { left + 1 };
                if next_page <= self.total_pages {
                    let (next_left, next_right) = self.spread_for_page(next_page);
                    load_order.push(next_left);
                    if let Some(nr) = next_right {
                        load_order.push(nr);
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
            _ => {
                let cur = self.current_page;
                load_order.push(cur);

                if self.spread_mode != PdfSpreadMode::NoSpreads {
                    let (left, right) = self.spread_for_page(cur);
                    if left != cur {
                        load_order.push(left);
                    }
                    if let Some(r) = right {
                        if r != cur {
                            load_order.push(r);
                        }
                    }
                }

                // Window of 7 pages ahead and behind for uninterrupted smooth scrolling
                for delta in 1..=7 {
                    if cur + delta <= self.total_pages {
                        load_order.push(cur + delta);
                    }
                    if cur > delta {
                        load_order.push(cur - delta);
                    }
                }
            }
        }

        let scale = ((1.5 * self.zoom_level).clamp(0.5, 3.5)) as f32;
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

    /// Build the viewport container widget for current scroll mode & spread mode.
    pub fn build_viewport_widget(&mut self) -> gtk::Widget {
        self.page_pictures.clear();
        self.paged_picture = None;
        self.paged_loading_box = None;
        self.paged_label = None;
        self.two_page_left_pic = None;
        self.two_page_right_pic = None;
        self.two_page_loading_box = None;
        self.two_page_label = None;

        match self.scroll_mode {
            PdfScrollMode::PageScrolling => {
                match self.spread_mode {
                    PdfSpreadMode::NoSpreads => {
                        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
                        container.set_halign(gtk::Align::Center);
                        container.set_valign(gtk::Align::Start);
                        container.set_hexpand(true);
                        container.set_vexpand(true);
                        container.set_margin_top(28);
                        container.set_margin_bottom(72);
                        container.set_margin_start(16);
                        container.set_margin_end(16);

                        let pic = gtk::Picture::new();
                        pic.set_can_shrink(true);
                        pic.set_content_fit(gtk::ContentFit::Contain);
                        pic.set_halign(gtk::Align::Center);
                        pic.set_valign(gtk::Align::Start);
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
                            if self.zoom_level <= 1.05 {
                                pic.set_size_request(-1, -1);
                            } else {
                                let scaled_w = (*w as f64 * self.zoom_level) as i32;
                                let scaled_h = (*h as f64 * self.zoom_level) as i32;
                                pic.set_size_request(scaled_w, scaled_h);
                            }
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

                        container.upcast()
                    }
                    _ => {
                        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
                        container.set_halign(gtk::Align::Center);
                        container.set_valign(gtk::Align::Start);
                        container.set_hexpand(true);
                        container.set_vexpand(true);
                        container.set_margin_top(28);
                        container.set_margin_bottom(72);
                        container.set_margin_start(16);
                        container.set_margin_end(16);

                        let spread_box = gtk::Box::new(gtk::Orientation::Horizontal, self.two_page_gap);
                        spread_box.set_halign(gtk::Align::Center);
                        spread_box.set_valign(gtk::Align::Start);

                        let (left, right) = self.spread_for_page(self.current_page);

                        let left_pic = gtk::Picture::new();
                        left_pic.set_can_shrink(true);
                        left_pic.set_content_fit(gtk::ContentFit::Contain);
                        left_pic.set_halign(if right.is_some() { gtk::Align::End } else { gtk::Align::Center });
                        left_pic.set_valign(gtk::Align::Start);
                        left_pic.add_css_class("kalam-pdf-page-image");
                        left_pic.add_css_class("kalam-pdf-two-page");

                        let right_pic = gtk::Picture::new();
                        right_pic.set_can_shrink(true);
                        right_pic.set_content_fit(gtk::ContentFit::Contain);
                        right_pic.set_halign(gtk::Align::Start);
                        right_pic.set_valign(gtk::Align::Start);
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
                                if self.zoom_level <= 1.05 {
                                    left_pic.set_size_request(-1, -1);
                                } else {
                                    let scaled_w = (*w as f64 * self.zoom_level) as i32;
                                    let scaled_h = (*h as f64 * self.zoom_level) as i32;
                                    left_pic.set_size_request(scaled_w, scaled_h);
                                }
                                left_pic.set_visible(true);
                            }
                            if right.is_some() {
                                if let Some((tex, w, h)) = right_loaded {
                                    right_pic.set_paintable(Some(tex));
                                    if self.zoom_level <= 1.05 {
                                        right_pic.set_size_request(-1, -1);
                                    } else {
                                        let scaled_w = (*w as f64 * self.zoom_level) as i32;
                                        let scaled_h = (*h as f64 * self.zoom_level) as i32;
                                        right_pic.set_size_request(scaled_w, scaled_h);
                                    }
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

                        container.upcast()
                    }
                }
            }
            PdfScrollMode::VerticalScrolling => {
                let container = gtk::Box::new(gtk::Orientation::Vertical, 20);
                container.set_halign(gtk::Align::Center);
                container.set_valign(gtk::Align::Start);
                container.set_hexpand(true);
                container.set_vexpand(false);
                container.set_margin_top(36);
                container.set_margin_bottom(88);
                container.set_margin_start(16);
                container.set_margin_end(16);

                match self.spread_mode {
                    PdfSpreadMode::NoSpreads => {
                        for p in 1..=self.total_pages {
                            let page_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
                            page_box.set_halign(gtk::Align::Center);
                            page_box.set_valign(gtk::Align::Start);

                            let pic = gtk::Picture::new();
                            pic.set_can_shrink(true);
                            pic.set_content_fit(gtk::ContentFit::Contain);
                            pic.set_halign(gtk::Align::Center);
                            pic.set_valign(gtk::Align::Start);

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
                    _ => {
                        for (left, right) in self.all_spreads() {
                            let spread_row = gtk::Box::new(gtk::Orientation::Horizontal, self.two_page_gap);
                            spread_row.set_halign(gtk::Align::Center);
                            spread_row.set_valign(gtk::Align::Start);
                            spread_row.add_css_class("kalam-pdf-spread-row");

                            let left_pic = gtk::Picture::new();
                            left_pic.set_can_shrink(true);
                            left_pic.set_content_fit(gtk::ContentFit::Contain);
                            left_pic.set_halign(if right.is_some() { gtk::Align::End } else { gtk::Align::Center });
                            left_pic.set_valign(gtk::Align::Start);
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
                                right_pic.set_valign(gtk::Align::Start);
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
            PdfScrollMode::HorizontalScrolling => {
                let container = gtk::Box::new(gtk::Orientation::Horizontal, 24);
                container.set_valign(gtk::Align::Start);
                container.set_halign(gtk::Align::Start);
                container.set_vexpand(true);
                container.set_hexpand(false);
                container.set_margin_top(28);
                container.set_margin_bottom(64);
                container.set_margin_start(24);
                container.set_margin_end(64);

                match self.spread_mode {
                    PdfSpreadMode::NoSpreads => {
                        for p in 1..=self.total_pages {
                            let pic = gtk::Picture::new();
                            pic.set_can_shrink(true);
                            pic.set_content_fit(gtk::ContentFit::Contain);
                            pic.set_valign(gtk::Align::Start);

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

                            self.page_pictures.insert(p, pic.clone());
                            container.append(&pic);
                        }
                    }
                    _ => {
                        for (left, right) in self.all_spreads() {
                            let spread_box = gtk::Box::new(gtk::Orientation::Horizontal, self.two_page_gap);
                            spread_box.set_valign(gtk::Align::Start);
                            spread_box.add_css_class("kalam-pdf-spread-row");

                            let left_pic = gtk::Picture::new();
                            left_pic.set_can_shrink(true);
                            left_pic.set_content_fit(gtk::ContentFit::Contain);
                            left_pic.set_valign(gtk::Align::Start);
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
                            spread_box.append(&left_pic);
                            self.page_pictures.insert(left, left_pic);

                            if let Some(r) = right {
                                let right_pic = gtk::Picture::new();
                                right_pic.set_can_shrink(true);
                                right_pic.set_content_fit(gtk::ContentFit::Contain);
                                right_pic.set_valign(gtk::Align::Start);
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
                                spread_box.append(&right_pic);
                                self.page_pictures.insert(r, right_pic);
                            }

                            container.append(&spread_box);
                        }
                    }
                }

                container.upcast()
            }
            PdfScrollMode::WrappedScrolling => {
                let flow_box = gtk::FlowBox::new();
                flow_box.set_valign(gtk::Align::Start);
                flow_box.set_halign(gtk::Align::Center);
                flow_box.set_hexpand(true);
                flow_box.set_vexpand(true);
                flow_box.set_margin_top(36);
                flow_box.set_margin_bottom(88);
                flow_box.set_margin_start(16);
                flow_box.set_margin_end(16);
                flow_box.set_column_spacing(20);
                flow_box.set_row_spacing(20);
                flow_box.set_selection_mode(gtk::SelectionMode::None);

                for p in 1..=self.total_pages {
                    let pic = gtk::Picture::new();
                    pic.set_can_shrink(true);
                    pic.set_content_fit(gtk::ContentFit::Contain);
                    pic.set_valign(gtk::Align::Start);

                    if let Some((tex, w, h)) = self.textures.get(&p) {
                        pic.set_paintable(Some(tex));
                        let thumb_w = (*w as f64 * 0.6 * self.zoom_level) as i32;
                        let thumb_h = (*h as f64 * 0.6 * self.zoom_level) as i32;
                        pic.set_size_request(thumb_w, thumb_h);
                        pic.add_css_class("kalam-pdf-page-image");
                    } else {
                        let placeholder_w = (360.0 * self.zoom_level) as i32;
                        let placeholder_h = (480.0 * self.zoom_level) as i32;
                        pic.set_size_request(placeholder_w, placeholder_h);
                        pic.add_css_class("kalam-pdf-placeholder");
                    }

                    self.page_pictures.insert(p, pic.clone());
                    flow_box.append(&pic);
                }

                flow_box.upcast()
            }
        }
    }

    pub fn update_paged_view(&self) {
        let (Some(ref pic), Some(ref loading_box), Some(ref label)) =
            (&self.paged_picture, &self.paged_loading_box, &self.paged_label)
        else {
            return;
        };

        if let Some((tex, w, h)) = self.textures.get(&self.current_page) {
            pic.set_paintable(Some(tex));
            if self.zoom_level <= 1.05 {
                pic.set_size_request(-1, -1);
            } else {
                let scaled_w = (*w as f64 * self.zoom_level) as i32;
                let scaled_h = (*h as f64 * self.zoom_level) as i32;
                pic.set_size_request(scaled_w, scaled_h);
            }
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

    pub fn update_two_page_view(&self) {
        let (Some(ref left_pic), Some(ref right_pic), Some(ref loading_box), Some(ref label)) = (
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
                if self.zoom_level <= 1.05 {
                    left_pic.set_size_request(-1, -1);
                } else {
                    let scaled_w = (*w as f64 * self.zoom_level) as i32;
                    let scaled_h = (*h as f64 * self.zoom_level) as i32;
                    left_pic.set_size_request(scaled_w, scaled_h);
                }
                left_pic.set_halign(if right.is_some() { gtk::Align::End } else { gtk::Align::Center });
                left_pic.set_valign(gtk::Align::Start);
                left_pic.set_visible(true);
            }
            if right.is_some() {
                if let Some((tex, w, h)) = right_loaded {
                    right_pic.set_paintable(Some(tex));
                    if self.zoom_level <= 1.05 {
                        right_pic.set_size_request(-1, -1);
                    } else {
                        let scaled_w = (*w as f64 * self.zoom_level) as i32;
                        let scaled_h = (*h as f64 * self.zoom_level) as i32;
                        right_pic.set_size_request(scaled_w, scaled_h);
                    }
                    right_pic.set_halign(gtk::Align::Start);
                    right_pic.set_valign(gtk::Align::Start);
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

    /// Smooth in-place zoom adjustment without widget recreation or crashes.
    pub fn apply_zoom_change(&mut self, sender: &ComponentSender<Self>, scroll: &gtk::ScrolledWindow) {
        self.textures.clear();
        self.pending_loads.clear();

        match self.scroll_mode {
            PdfScrollMode::PageScrolling => {
                match self.spread_mode {
                    PdfSpreadMode::NoSpreads => self.update_paged_view(),
                    _ => self.update_two_page_view(),
                }
            }
            _ => {
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

    /// Scroll to current page ratio in continuous flow modes.
    pub fn scroll_to_current_page(&self, scroll: &gtk::ScrolledWindow) {
        if self.scroll_mode == PdfScrollMode::PageScrolling || self.total_pages <= 1 {
            return;
        }

        if self.scroll_mode == PdfScrollMode::HorizontalScrolling {
            let hadj = scroll.hadjustment();
            let max = (hadj.upper() - hadj.page_size()).max(0.0);
            if max > 0.0 {
                let ratio = (self.current_page.saturating_sub(1)) as f64 / (self.total_pages - 1) as f64;
                hadj.set_value((ratio * max).clamp(hadj.lower(), max));
            }
        } else {
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
        toggle_active(&sw.scroll_page_btn, self.scroll_mode == PdfScrollMode::PageScrolling);
        toggle_active(&sw.scroll_vertical_btn, self.scroll_mode == PdfScrollMode::VerticalScrolling);
        toggle_active(&sw.scroll_horizontal_btn, self.scroll_mode == PdfScrollMode::HorizontalScrolling);
        toggle_active(&sw.scroll_wrapped_btn, self.scroll_mode == PdfScrollMode::WrappedScrolling);

        toggle_active(&sw.spread_none_btn, self.spread_mode == PdfSpreadMode::NoSpreads);
        toggle_active(&sw.spread_odd_btn, self.spread_mode == PdfSpreadMode::OddSpreads);
        toggle_active(&sw.spread_even_btn, self.spread_mode == PdfSpreadMode::EvenSpreads);

        sw.spread_gap_box.set_visible(self.spread_mode != PdfSpreadMode::NoSpreads);
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

            // ── 1. Main Viewport Scroll ──────────────────────────────
            #[name = "viewport_scroll"]
            gtk::ScrolledWindow {
                set_hexpand: true,
                set_vexpand: true,
                set_hscrollbar_policy: gtk::PolicyType::Automatic,
                set_vscrollbar_policy: gtk::PolicyType::Always,
                add_css_class: "kalam-pdf-viewport",
            },

            // ── 2. Top Edge Trigger Strip ─────────────────────────────
            add_overlay = &gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_valign: gtk::Align::Start,
                set_hexpand: true,
                set_height_request: 45,
                add_css_class: "kalam-reader-edge-strip",
                add_css_class: "kalam-reader-edge-strip-top",
            },

            // ── 3. Bottom Edge Trigger Strip ──────────────────────────
            add_overlay = &gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_valign: gtk::Align::End,
                set_hexpand: true,
                set_height_request: 45,
                add_css_class: "kalam-reader-edge-strip",
                add_css_class: "kalam-reader-edge-strip-bottom",
            },

            // ── 4. Floating Top-Left Back Dock ────────────────────────
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_back_button && !model.show_sidebar,
                set_transition_type: gtk::RevealerTransitionType::SlideDown,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Start,
                set_margin_start: 24,
                set_margin_top: 18,

                #[name = "back_dock"]
                gtk::Box {
                    add_css_class: "kalam-reader-back-dock",
                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes(
                            "go-previous-symbolic",
                            15,
                            &["kalam-inline-icon"],
                        )),
                        set_label: "Library",
                        add_css_class: "kalam-reader-back-chip",
                        set_tooltip_text: Some("Back to Library (Esc / Backspace)"),
                        connect_clicked => PdfReaderMsg::Close,
                    },
                },
            },

            // ── 5. Floating Bottom Navigation Pill ────────────────────
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_bottom_pill,
                set_transition_type: gtk::RevealerTransitionType::SlideUp,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::End,
                set_margin_bottom: 24,

                #[name = "bottom_dock"]
                gtk::Box {
                    add_css_class: "kalam-reader-floating-pill",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 6,

                    // Sidebar Toggle
                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes(
                            "view-sidebar-start-symbolic",
                            15,
                            &["kalam-inline-icon"],
                        )),
                        add_css_class: "kalam-reader-pill-nav",
                        set_tooltip_text: Some("Toggle Outlines & Bookmarks (t)"),
                        connect_clicked => PdfReaderMsg::ToggleSidebar,
                    },

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-reader-pill-sep",
                    },

                    // Previous Page
                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes(
                            "go-previous-symbolic",
                            15,
                            &["kalam-inline-icon"],
                        )),
                        add_css_class: "kalam-reader-pill-nav",
                        set_tooltip_text: Some("Previous Page (Left / h)"),
                        connect_clicked => PdfReaderMsg::PrevPage,
                    },

                    // Page Indicator
                    gtk::Label {
                        add_css_class: "kalam-reader-pill-counter",
                        #[watch]
                        set_label: &format!("Page {} of {}", model.current_page, model.total_pages),
                    },

                    // Next Page
                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes(
                            "go-next-symbolic",
                            15,
                            &["kalam-inline-icon"],
                        )),
                        add_css_class: "kalam-reader-pill-nav",
                        set_tooltip_text: Some("Next Page (Right / Space / l)"),
                        connect_clicked => PdfReaderMsg::NextPage,
                    },

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-reader-pill-sep",
                    },

                    // Bookmark Toggle Button
                    #[name = "bottom_bookmark_btn"]
                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes(
                            "bookmark-new-symbolic",
                            15,
                            &["kalam-inline-icon"],
                        )),
                        add_css_class: "kalam-reader-pill-nav",
                        set_tooltip_text: Some("Bookmark Page (b)"),
                        connect_clicked => PdfReaderMsg::ToggleBookmark,
                    },

                    gtk::Separator {
                        set_orientation: gtk::Orientation::Vertical,
                        add_css_class: "kalam-reader-pill-sep",
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

            // ── 6. Overlay: Slide-in Sidebar (TOC + Bookmarks + Settings) ─────
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
                                set_ellipsize: gtk::pango::EllipsizeMode::End,
                                set_max_width_chars: 24,
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

                    // Bottom Tabbar (TOC, Bookmarks, and Settings buttons)
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

                        #[name = "tab_bookmarks_btn"]
                        gtk::Button {
                            set_child: Some(&reader_sidebar_tab_content("bookmark-new-symbolic", "Bookmarks")),
                            add_css_class: "kalam-reader-tab",
                            set_hexpand: true,
                            connect_clicked => PdfReaderMsg::SwitchSidebarTab(PdfSidebarTab::Bookmarks),
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

        // 2. Set up Left Sidebar Stack (TOC + Bookmarks + Settings)
        let left_stack = gtk::Stack::new();
        left_stack.set_hexpand(true);
        left_stack.set_vexpand(true);
        left_stack.set_transition_type(gtk::StackTransitionType::Crossfade);

        // Panel 1: Outlines (TOC)
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

        // Panel 2: Bookmarks
        let bookmarks_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .build();
        bookmarks_scroll.add_css_class("kalam-reader-panel-scroll");
        let bookmarks_list_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        bookmarks_list_box.set_hexpand(true);
        bookmarks_list_box.set_vexpand(true);
        bookmarks_scroll.set_child(Some(&bookmarks_list_box));

        // Panel 3: Settings
        let settings_scroll = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::Never)
            .vscrollbar_policy(gtk::PolicyType::Automatic)
            .hexpand(true)
            .vexpand(true)
            .build();
        settings_scroll.add_css_class("kalam-reader-panel-scroll");
        let (settings_box, settings_w) = build_pdf_settings_panel(&model, &sender);
        settings_scroll.set_child(Some(&settings_box));

        left_stack.add_named(&toc_scroll, Some("toc"));
        left_stack.add_named(&bookmarks_scroll, Some("bookmarks"));
        left_stack.add_named(&settings_scroll, Some("settings"));
        widgets.left_panel_host.append(&left_stack);

        populate_toc_list(
            &toc_list_box,
            &model.toc_entries,
            model.current_page,
            model.total_pages,
            &sender,
        );

        populate_bookmarks_list(&bookmarks_list_box, &model.bookmarks, &sender);

        model.left_stack = Some(left_stack);
        model.toc_list_box = Some(toc_list_box);
        model.bookmarks_list_box = Some(bookmarks_list_box);
        model.settings_widgets = Some(settings_w);

        // 3. Connect Keyboard Controller
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
                Key::b | Key::B => {
                    let _ = tx_key.send(PdfReaderMsg::ToggleBookmark);
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
                Key::Home => {
                    let _ = tx_key.send(PdfReaderMsg::GoToFirstPage);
                    gtk::glib::Propagation::Stop
                }
                Key::End => {
                    let _ = tx_key.send(PdfReaderMsg::GoToLastPage);
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

        // 10. Click controller on viewport to toggle controls or dismiss sidebar
        let click = gtk::GestureClick::new();
        let tx_clk = tx.clone();
        click.connect_released(move |_, _n_press, _x, _y| {
            let _ = tx_clk.send(PdfReaderMsg::ToggleControls);
        });
        widgets.viewport_scroll.add_controller(click);

        // 11. Pinch zoom gesture for touchpads and touchscreens
        let zoom_gesture = gtk::GestureZoom::new();
        let tx_zg = tx.clone();
        zoom_gesture.connect_scale_changed(move |_, scale_factor| {
            if scale_factor > 1.08 {
                let _ = tx_zg.send(PdfReaderMsg::ZoomIn);
            } else if scale_factor < 0.92 {
                let _ = tx_zg.send(PdfReaderMsg::ZoomOut);
            }
        });
        widgets.viewport_scroll.add_controller(zoom_gesture);

        // Initial child and render triggering
        let child = model.build_viewport_widget();
        widgets.viewport_scroll.set_child(Some(&child));

        // Restore scroll position in continuous mode if resuming past page 1
        if model.scroll_mode != PdfScrollMode::PageScrolling && model.current_page > 1 && model.total_pages > 1 {
            let cur = model.current_page;
            let total = model.total_pages;
            let vadj = widgets.viewport_scroll.vadjustment();
            glib::idle_add_local_once(move || {
                let max = (vadj.upper() - vadj.page_size()).max(0.0);
                if max > 0.0 {
                    let ratio = (cur.saturating_sub(1)) as f64 / (total - 1) as f64;
                    vadj.set_value((ratio * max).clamp(vadj.lower(), max));
                }
            });
        }

        model.schedule_back_hide(&sender);
        model.schedule_bottom_hide(&sender);
        model.trigger_loads(&sender);
        model.update_bookmark_icon_state(&widgets);

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
            PdfReaderMsg::Close => {
                self.save_progress();
                if let Some(sid) = self.session_id {
                    let elapsed = self.session_start.elapsed().as_secs() as i64;
                    let pct = if self.total_pages > 0 {
                        ((self.current_page as f64 / self.total_pages as f64) * 100.0).round() as i64
                    } else {
                        0
                    };
                    let _ = self.catalog.end_reading_session(sid, elapsed, pct);
                }
                let _ = sender.output_sender().send(PdfReaderOut::Close);
            }
            PdfReaderMsg::EscapeKey => {
                if self.show_sidebar {
                    self.show_sidebar = false;
                } else {
                    let _ = sender.input_sender().send(PdfReaderMsg::Close);
                }
            }
            PdfReaderMsg::NextPage => {
                if self.current_page < self.total_pages {
                    let step = match (self.spread_mode, self.scroll_mode) {
                        (PdfSpreadMode::OddSpreads | PdfSpreadMode::EvenSpreads, PdfScrollMode::PageScrolling) => {
                            let (_, right) = self.spread_for_page(self.current_page);
                            if right.is_some() { 2 } else { 1 }
                        }
                        _ => 1,
                    };
                    self.current_page = (self.current_page + step).min(self.total_pages);
                    self.save_progress();
                    self.trigger_loads(&sender);

                    match self.scroll_mode {
                        PdfScrollMode::PageScrolling => {
                            match self.spread_mode {
                                PdfSpreadMode::NoSpreads => self.update_paged_view(),
                                _ => self.update_two_page_view(),
                            }
                        }
                        _ => {
                            self.scroll_to_current_page(&widgets.viewport_scroll);
                        }
                    }
                    self.update_bookmark_icon_state(widgets);
                }
            }
            PdfReaderMsg::PrevPage => {
                if self.current_page > 1 {
                    let step = match (self.spread_mode, self.scroll_mode) {
                        (PdfSpreadMode::OddSpreads, PdfScrollMode::PageScrolling) => {
                            let (left, _) = self.spread_for_page(self.current_page);
                            if left > 1 && left == 2 { 1 } else { 2 }
                        }
                        (PdfSpreadMode::EvenSpreads, PdfScrollMode::PageScrolling) => 2,
                        _ => 1,
                    };
                    self.current_page = self.current_page.saturating_sub(step).max(1);
                    self.save_progress();
                    self.trigger_loads(&sender);

                    match self.scroll_mode {
                        PdfScrollMode::PageScrolling => {
                            match self.spread_mode {
                                PdfSpreadMode::NoSpreads => self.update_paged_view(),
                                _ => self.update_two_page_view(),
                            }
                        }
                        _ => {
                            self.scroll_to_current_page(&widgets.viewport_scroll);
                        }
                    }
                    self.update_bookmark_icon_state(widgets);
                }
            }
            PdfReaderMsg::GoToFirstPage => {
                if self.current_page != 1 {
                    self.current_page = 1;
                    self.save_progress();
                    self.trigger_loads(&sender);
                    match self.scroll_mode {
                        PdfScrollMode::PageScrolling => {
                            match self.spread_mode {
                                PdfSpreadMode::NoSpreads => self.update_paged_view(),
                                _ => self.update_two_page_view(),
                            }
                        }
                        _ => {
                            self.scroll_to_current_page(&widgets.viewport_scroll);
                        }
                    }
                    self.update_bookmark_icon_state(widgets);
                }
            }
            PdfReaderMsg::GoToLastPage => {
                if self.current_page != self.total_pages {
                    self.current_page = self.total_pages;
                    self.save_progress();
                    self.trigger_loads(&sender);
                    match self.scroll_mode {
                        PdfScrollMode::PageScrolling => {
                            match self.spread_mode {
                                PdfSpreadMode::NoSpreads => self.update_paged_view(),
                                _ => self.update_two_page_view(),
                            }
                        }
                        _ => {
                            self.scroll_to_current_page(&widgets.viewport_scroll);
                        }
                    }
                    self.update_bookmark_icon_state(widgets);
                }
            }
            PdfReaderMsg::ZoomIn => {
                self.zoom_level = (self.zoom_level + 0.15).min(3.0);
                self.catalog.set_pref(&format!("book.{}.pdf.zoom", self.book_id), &format!("{:.2}", self.zoom_level));
                self.apply_zoom_change(&sender, &widgets.viewport_scroll);
            }
            PdfReaderMsg::ZoomOut => {
                self.zoom_level = (self.zoom_level - 0.15).max(0.4);
                self.catalog.set_pref(&format!("book.{}.pdf.zoom", self.book_id), &format!("{:.2}", self.zoom_level));
                self.apply_zoom_change(&sender, &widgets.viewport_scroll);
            }
            PdfReaderMsg::ResetZoom => {
                self.zoom_level = 1.0;
                self.catalog.set_pref(&format!("book.{}.pdf.zoom", self.book_id), &format!("{:.2}", self.zoom_level));
                self.apply_zoom_change(&sender, &widgets.viewport_scroll);
            }
            PdfReaderMsg::FitToPage => {
                self.zoom_level = 1.0;
                self.catalog.set_pref(&format!("book.{}.pdf.zoom", self.book_id), &format!("{:.2}", self.zoom_level));
                self.apply_zoom_change(&sender, &widgets.viewport_scroll);
            }
            PdfReaderMsg::FitToWidth => {
                self.zoom_level = 1.35;
                self.catalog.set_pref(&format!("book.{}.pdf.zoom", self.book_id), &format!("{:.2}", self.zoom_level));
                self.apply_zoom_change(&sender, &widgets.viewport_scroll);
            }
            PdfReaderMsg::SetScrollMode(mode) => {
                if self.scroll_mode != mode {
                    self.scroll_mode = mode;
                    let mode_str = match self.scroll_mode {
                        PdfScrollMode::PageScrolling => "page",
                        PdfScrollMode::VerticalScrolling => "vertical",
                        PdfScrollMode::HorizontalScrolling => "horizontal",
                        PdfScrollMode::WrappedScrolling => "wrapped",
                    };
                    self.catalog.set_pref(&format!("book.{}.pdf.scroll_mode", self.book_id), mode_str);
                    self.catalog.set_pref("reader.pdf.scroll_mode", mode_str);

                    // Adjust scrollbar policies
                    if self.scroll_mode == PdfScrollMode::HorizontalScrolling {
                        widgets.viewport_scroll.set_hscrollbar_policy(gtk::PolicyType::Always);
                        widgets.viewport_scroll.set_vscrollbar_policy(gtk::PolicyType::Automatic);
                    } else {
                        widgets.viewport_scroll.set_hscrollbar_policy(gtk::PolicyType::Automatic);
                        widgets.viewport_scroll.set_vscrollbar_policy(gtk::PolicyType::Always);
                    }

                    let child = self.build_viewport_widget();
                    widgets.viewport_scroll.set_child(Some(&child));
                    self.trigger_loads(&sender);
                    if self.scroll_mode != PdfScrollMode::PageScrolling && self.current_page > 1 {
                        self.scroll_to_current_page(&widgets.viewport_scroll);
                    }
                }
            }
            PdfReaderMsg::SetSpreadMode(mode) => {
                if self.spread_mode != mode {
                    self.spread_mode = mode;
                    let mode_str = match self.spread_mode {
                        PdfSpreadMode::NoSpreads => "none",
                        PdfSpreadMode::OddSpreads => "odd",
                        PdfSpreadMode::EvenSpreads => "even",
                    };
                    self.catalog.set_pref(&format!("book.{}.pdf.spread_mode", self.book_id), mode_str);
                    self.catalog.set_pref("reader.pdf.spread_mode", mode_str);

                    let child = self.build_viewport_widget();
                    widgets.viewport_scroll.set_child(Some(&child));
                    self.trigger_loads(&sender);
                    if self.scroll_mode != PdfScrollMode::PageScrolling && self.current_page > 1 {
                        self.scroll_to_current_page(&widgets.viewport_scroll);
                    }
                }
            }
            PdfReaderMsg::SetSpreadGap(gap) => {
                let clamped = gap.clamp(0, 48);
                if self.two_page_gap != clamped {
                    self.two_page_gap = clamped;
                    self.catalog.set_pref_i64("reader.pdf.two_page_gap", clamped as i64);
                    if self.spread_mode != PdfSpreadMode::NoSpreads {
                        let child = self.build_viewport_widget();
                        widgets.viewport_scroll.set_child(Some(&child));
                        self.trigger_loads(&sender);
                        if self.scroll_mode != PdfScrollMode::PageScrolling && self.current_page > 1 {
                            self.scroll_to_current_page(&widgets.viewport_scroll);
                        }
                    }
                }
            }
            PdfReaderMsg::ToggleBookmark => {
                if let Some(mark) = self.bookmarks.iter().find(|b| b.chapter_index as usize == self.current_page) {
                    let _ = self.catalog.delete_reading_bookmark(mark.id);
                    crate::notify::compact("Bookmark removed", "");
                } else {
                    let _ = self.catalog.insert_reading_bookmark(
                        self.book_id,
                        self.current_page as i64,
                        self.progress_fraction(),
                        &format!("Page {}", self.current_page),
                    );
                    crate::notify::compact("Bookmark saved", &format!("Page {}", self.current_page));
                }
                self.reload_bookmarks();
                if let Some(ref list) = self.bookmarks_list_box {
                    populate_bookmarks_list(list, &self.bookmarks, &sender);
                }
                self.update_bookmark_icon_state(widgets);
            }
            PdfReaderMsg::DeleteBookmark(id) => {
                let _ = self.catalog.delete_reading_bookmark(id);
                self.reload_bookmarks();
                if let Some(ref list) = self.bookmarks_list_box {
                    populate_bookmarks_list(list, &self.bookmarks, &sender);
                }
                self.update_bookmark_icon_state(widgets);
            }
            PdfReaderMsg::JumpToPage(target_page) => {
                self.current_page = target_page.clamp(1, self.total_pages);
                self.save_progress();
                self.trigger_loads(&sender);

                match self.scroll_mode {
                    PdfScrollMode::PageScrolling => {
                        match self.spread_mode {
                            PdfSpreadMode::NoSpreads => self.update_paged_view(),
                            _ => self.update_two_page_view(),
                        }
                    }
                    _ => {
                        self.scroll_to_current_page(&widgets.viewport_scroll);
                    }
                }
                self.show_sidebar = false;
                self.update_bookmark_icon_state(widgets);
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
                let _ = sender.input_sender().send(PdfReaderMsg::SetSmartCrop(!self.smart_crop));
            }
            PdfReaderMsg::SwitchSidebarTab(tab) => {
                self.sidebar_tab = tab;
                if let Some(ref stack) = self.left_stack {
                    match tab {
                        PdfSidebarTab::Toc => {
                            stack.set_visible_child_name("toc");
                            toggle_active(&widgets.tab_toc_btn, true);
                            toggle_active(&widgets.tab_bookmarks_btn, false);
                            toggle_active(&widgets.tab_settings_btn, false);
                        }
                        PdfSidebarTab::Bookmarks => {
                            stack.set_visible_child_name("bookmarks");
                            toggle_active(&widgets.tab_toc_btn, false);
                            toggle_active(&widgets.tab_bookmarks_btn, true);
                            toggle_active(&widgets.tab_settings_btn, false);
                            if let Some(ref list) = self.bookmarks_list_box {
                                populate_bookmarks_list(list, &self.bookmarks, &sender);
                            }
                        }
                        PdfSidebarTab::Settings => {
                            stack.set_visible_child_name("settings");
                            toggle_active(&widgets.tab_toc_btn, false);
                            toggle_active(&widgets.tab_bookmarks_btn, false);
                            toggle_active(&widgets.tab_settings_btn, true);
                            if let Some(ref sw) = self.settings_widgets {
                                self.sync_settings_ui(sw);
                            }
                        }
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
                    if let Some(ref list) = self.bookmarks_list_box {
                        populate_bookmarks_list(list, &self.bookmarks, &sender);
                    }
                }
            }
            PdfReaderMsg::ToggleControls => {
                if self.show_sidebar {
                    self.show_sidebar = false;
                } else {
                    let next = !self.show_bottom_pill;
                    self.show_bottom_pill = next;
                    self.show_back_button = next;
                    if next {
                        self.schedule_back_hide(&sender);
                        self.schedule_bottom_hide(&sender);
                    }
                }
            }
            PdfReaderMsg::PageRendered {
                page,
                texture,
                width,
                height,
            } => {
                self.pending_loads.remove(&page);
                self.textures.insert(page, (texture.clone(), width, height));

                match self.scroll_mode {
                    PdfScrollMode::PageScrolling => {
                        match self.spread_mode {
                            PdfSpreadMode::NoSpreads => {
                                if page == self.current_page {
                                    self.update_paged_view();
                                }
                            }
                            _ => {
                                let (left, right) = self.spread_for_page(self.current_page);
                                if page == left || right == Some(page) {
                                    self.update_two_page_view();
                                }
                            }
                        }
                    }
                    _ => {
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
                        if let Some(ref list) = self.bookmarks_list_box {
                            populate_bookmarks_list(list, &self.bookmarks, &sender);
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
                if self.scroll_mode == PdfScrollMode::HorizontalScrolling {
                    let hadj = widgets.viewport_scroll.hadjustment();
                    let step = (self.arrow_step as f64) * dir;
                    let target = (hadj.value() + step)
                        .clamp(hadj.lower(), (hadj.upper() - hadj.page_size()).max(0.0));
                    hadj.set_value(target);
                } else if self.scroll_mode != PdfScrollMode::PageScrolling {
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
                if self.scroll_mode == PdfScrollMode::PageScrolling || self.total_pages <= 1 {
                    return;
                }

                let (pos, max) = if self.scroll_mode == PdfScrollMode::HorizontalScrolling {
                    let hadj = widgets.viewport_scroll.hadjustment();
                    (hadj.value(), (hadj.upper() - hadj.page_size()).max(1.0))
                } else {
                    let vadj = widgets.viewport_scroll.vadjustment();
                    (vadj.value(), (vadj.upper() - vadj.page_size()).max(1.0))
                };

                let ratio = (pos / max).clamp(0.0, 1.0);
                let target = 1 + (ratio * (self.total_pages - 1) as f64).round() as usize;
                if target != self.current_page && target <= self.total_pages {
                    self.current_page = target;
                    self.save_progress();
                    self.trigger_loads(&sender);
                    self.update_bookmark_icon_state(widgets);
                }
            }
        }

        if let Some(ref sw) = self.settings_widgets {
            self.sync_settings_ui(sw);
        }
    }
}

impl PdfReaderModel {
    pub fn update_bookmark_icon_state(&self, widgets: &PdfReaderModelWidgets) {
        let is_bookmarked = self
            .bookmarks
            .iter()
            .any(|b| b.chapter_index as usize == self.current_page);
        toggle_active(&widgets.bottom_bookmark_btn, is_bookmarked);
    }
}

fn populate_toc_list(
    list_box: &gtk::Box,
    entries: &[PdfTocEntry],
    current_page: usize,
    total_pages: usize,
    sender: &ComponentSender<PdfReaderModel>,
) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    if entries.is_empty() {
        let empty_label = gtk::Label::new(Some("No document outlines available."));
        empty_label.add_css_class("dim-label");
        empty_label.set_margin_top(24);
        empty_label.set_margin_start(16);
        empty_label.set_margin_end(16);
        list_box.append(&empty_label);
        return;
    }

    for item in entries {
        let page_target = item.page;
        if page_target == 0 || page_target > total_pages {
            continue;
        }

        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let indent = (item.depth.saturating_sub(1) * 14) as i32;
        row_box.set_margin_start(indent);

        let title_lbl = gtk::Label::new(Some(&item.title));
        title_lbl.set_halign(gtk::Align::Start);
        title_lbl.set_hexpand(true);
        title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        title_lbl.set_max_width_chars(24);
        row_box.append(&title_lbl);

        let page_lbl = gtk::Label::new(Some(&page_target.to_string()));
        page_lbl.add_css_class("dim-label");
        page_lbl.set_halign(gtk::Align::End);
        row_box.append(&page_lbl);

        let btn = gtk::Button::new();
        btn.set_child(Some(&row_box));
        btn.add_css_class("flat");
        btn.add_css_class("kalam-reader-toc-item");

        if current_page == page_target {
            btn.add_css_class("active");
        }

        let tx = sender.input_sender().clone();
        btn.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::JumpToPage(page_target));
        });

        list_box.append(&btn);
    }
}

fn populate_bookmarks_list(
    list_box: &gtk::Box,
    bookmarks: &[ReadingBookmark],
    sender: &ComponentSender<PdfReaderModel>,
) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    if bookmarks.is_empty() {
        let empty_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
        empty_box.set_margin_top(32);
        empty_box.set_margin_start(16);
        empty_box.set_margin_end(16);

        let icon = crate::icons::symbolic_with_classes("bookmark-new-symbolic", 32, &["dim-label"]);
        icon.set_halign(gtk::Align::Center);
        empty_box.append(&icon);

        let label = gtk::Label::new(Some("No bookmarks yet."));
        label.add_css_class("kalam-reader-section-label");
        label.set_halign(gtk::Align::Center);
        empty_box.append(&label);

        let hint = gtk::Label::new(Some("Press B or tap the bookmark button on the bottom bar to bookmark any page."));
        hint.add_css_class("dim-label");
        hint.set_wrap(true);
        hint.set_justify(gtk::Justification::Center);
        empty_box.append(&hint);

        list_box.append(&empty_box);
        return;
    }

    for mark in bookmarks {
        let page_num = mark.chapter_index as usize;
        let mark_id = mark.id;

        let row_btn = gtk::Button::new();
        row_btn.add_css_class("flat");
        row_btn.add_css_class("kalam-reader-bookmark-row");

        let h_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);

        let icon = crate::icons::symbolic_with_classes("bookmark-new-symbolic", 16, &["kalam-reader-bookmark-icon"]);
        h_box.append(&icon);

        let v_box = gtk::Box::new(gtk::Orientation::Vertical, 2);
        v_box.set_hexpand(true);

        let title_lbl = gtk::Label::new(Some(&mark.label));
        title_lbl.add_css_class("kalam-reader-bookmark-title");
        title_lbl.set_halign(gtk::Align::Start);
        v_box.append(&title_lbl);

        let progress_lbl = gtk::Label::new(Some(&format!("{:.0}% through book", mark.fraction * 100.0)));
        progress_lbl.add_css_class("dim-label");
        progress_lbl.set_halign(gtk::Align::Start);
        v_box.append(&progress_lbl);

        h_box.append(&v_box);

        let del_btn = gtk::Button::new();
        del_btn.set_child(Some(&crate::icons::symbolic_with_classes("edit-delete-symbolic", 14, &["dim-label"])));
        del_btn.add_css_class("flat");
        del_btn.set_tooltip_text(Some("Delete Bookmark"));
        let tx_del = sender.input_sender().clone();
        del_btn.connect_clicked(move |_| {
            let _ = tx_del.send(PdfReaderMsg::DeleteBookmark(mark_id));
        });
        h_box.append(&del_btn);

        row_btn.set_child(Some(&h_box));
        let tx_jump = sender.input_sender().clone();
        row_btn.connect_clicked(move |_| {
            let _ = tx_jump.send(PdfReaderMsg::JumpToPage(page_num));
        });

        list_box.append(&row_btn);
    }
}

fn build_pdf_settings_panel(
    model: &PdfReaderModel,
    sender: &ComponentSender<PdfReaderModel>,
) -> (gtk::Box, PdfSettingsWidgets) {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 16);
    wrap.set_margin_start(16);
    wrap.set_margin_end(16);
    wrap.set_margin_top(16);
    wrap.set_margin_bottom(24);

    // ── 1. Scrolling Section (Matching Photo) ────────────────
    let scroll_section = reader_settings_section("Scrolling");

    let scroll_box = gtk::Box::new(gtk::Orientation::Vertical, 6);

    let scroll_page_btn = gtk::Button::with_label("Page Scrolling");
    scroll_page_btn.add_css_class("kalam-reader-seg-btn");
    scroll_page_btn.set_halign(gtk::Align::Fill);
    let tx = sender.input_sender().clone();
    scroll_page_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetScrollMode(PdfScrollMode::PageScrolling));
    });
    scroll_box.append(&scroll_page_btn);

    let scroll_vertical_btn = gtk::Button::with_label("Vertical Scrolling");
    scroll_vertical_btn.add_css_class("kalam-reader-seg-btn");
    scroll_vertical_btn.set_halign(gtk::Align::Fill);
    let tx = sender.input_sender().clone();
    scroll_vertical_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetScrollMode(PdfScrollMode::VerticalScrolling));
    });
    scroll_box.append(&scroll_vertical_btn);

    let scroll_horizontal_btn = gtk::Button::with_label("Horizontal Scrolling");
    scroll_horizontal_btn.add_css_class("kalam-reader-seg-btn");
    scroll_horizontal_btn.set_halign(gtk::Align::Fill);
    let tx = sender.input_sender().clone();
    scroll_horizontal_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetScrollMode(PdfScrollMode::HorizontalScrolling));
    });
    scroll_box.append(&scroll_horizontal_btn);

    let scroll_wrapped_btn = gtk::Button::with_label("Wrapped Scrolling");
    scroll_wrapped_btn.add_css_class("kalam-reader-seg-btn");
    scroll_wrapped_btn.set_halign(gtk::Align::Fill);
    let tx = sender.input_sender().clone();
    scroll_wrapped_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetScrollMode(PdfScrollMode::WrappedScrolling));
    });
    scroll_box.append(&scroll_wrapped_btn);

    scroll_section.append(&scroll_box);
    wrap.append(&scroll_section);

    let divider = gtk::Separator::new(gtk::Orientation::Horizontal);
    divider.add_css_class("kalam-section-divider");
    wrap.append(&divider);

    // ── 2. Spreads Section (Matching Photo) ──────────────────
    let spread_section = reader_settings_section("Spreads");

    let spread_box = gtk::Box::new(gtk::Orientation::Vertical, 6);

    let spread_none_btn = gtk::Button::with_label("No Spreads");
    spread_none_btn.add_css_class("kalam-reader-seg-btn");
    spread_none_btn.set_halign(gtk::Align::Fill);
    let tx = sender.input_sender().clone();
    spread_none_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetSpreadMode(PdfSpreadMode::NoSpreads));
    });
    spread_box.append(&spread_none_btn);

    let spread_odd_btn = gtk::Button::with_label("Odd Spreads");
    spread_odd_btn.add_css_class("kalam-reader-seg-btn");
    spread_odd_btn.set_halign(gtk::Align::Fill);
    let tx = sender.input_sender().clone();
    spread_odd_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetSpreadMode(PdfSpreadMode::OddSpreads));
    });
    spread_box.append(&spread_odd_btn);

    let spread_even_btn = gtk::Button::with_label("Even Spreads");
    spread_even_btn.add_css_class("kalam-reader-seg-btn");
    spread_even_btn.set_halign(gtk::Align::Fill);
    let tx = sender.input_sender().clone();
    spread_even_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetSpreadMode(PdfSpreadMode::EvenSpreads));
    });
    spread_box.append(&spread_even_btn);

    spread_section.append(&spread_box);

    // Spread gap options box (visible only when Odd or Even Spreads active)
    let spread_gap_box = gtk::Box::new(gtk::Orientation::Vertical, 8);
    spread_gap_box.set_margin_top(8);

    let gap_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    gap_row.add_css_class("kalam-reader-setting-row");
    let gap_label = gtk::Label::new(Some("Spread Gap"));
    gap_label.add_css_class("kalam-reader-setting-name");
    gap_label.set_hexpand(true);
    gap_label.set_halign(gtk::Align::Start);
    gap_row.append(&gap_label);

    let gap_switcher = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    gap_switcher.add_css_class("linked");

    let gap_0_btn = gtk::Button::with_label("0px");
    gap_0_btn.add_css_class("kalam-reader-seg-btn");
    let tx = sender.input_sender().clone();
    gap_0_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetSpreadGap(0));
    });
    gap_switcher.append(&gap_0_btn);

    let gap_4_btn = gtk::Button::with_label("4px");
    gap_4_btn.add_css_class("kalam-reader-seg-btn");
    let tx = sender.input_sender().clone();
    gap_4_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetSpreadGap(4));
    });
    gap_switcher.append(&gap_4_btn);

    let gap_8_btn = gtk::Button::with_label("8px");
    gap_8_btn.add_css_class("kalam-reader-seg-btn");
    let tx = sender.input_sender().clone();
    gap_8_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetSpreadGap(8));
    });
    gap_switcher.append(&gap_8_btn);

    let gap_12_btn = gtk::Button::with_label("12px");
    gap_12_btn.add_css_class("kalam-reader-seg-btn");
    let tx = sender.input_sender().clone();
    gap_12_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetSpreadGap(12));
    });
    gap_switcher.append(&gap_12_btn);

    let gap_16_btn = gtk::Button::with_label("16px");
    gap_16_btn.add_css_class("kalam-reader-seg-btn");
    let tx = sender.input_sender().clone();
    gap_16_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::SetSpreadGap(16));
    });
    gap_switcher.append(&gap_16_btn);
    gap_row.append(&gap_switcher);
    spread_gap_box.append(&gap_row);

    let gap_hint = gtk::Label::new(Some("0px gives seamless cross-page spreads and illustrations."));
    gap_hint.add_css_class("kalam-reader-setting-hint");
    gap_hint.set_wrap(true);
    gap_hint.set_xalign(0.0);
    spread_gap_box.append(&gap_hint);

    spread_section.append(&spread_gap_box);
    wrap.append(&spread_section);

    let divider2 = gtk::Separator::new(gtk::Orientation::Horizontal);
    divider2.add_css_class("kalam-section-divider");
    wrap.append(&divider2);

    // ── 3. Zoom & View Sizing Section ─────────────────────────
    let zoom_section = reader_settings_section("Zoom & View Sizing");

    let fit_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    fit_row.add_css_class("kalam-reader-setting-row");
    let fit_label = gtk::Label::new(Some("Page Sizing"));
    fit_label.add_css_class("kalam-reader-setting-name");
    fit_label.set_hexpand(true);
    fit_label.set_halign(gtk::Align::Start);
    fit_row.append(&fit_label);

    let fit_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    fit_box.add_css_class("linked");

    let fit_page_btn = gtk::Button::with_label("Fit Page");
    fit_page_btn.add_css_class("kalam-reader-seg-btn");
    let tx = sender.input_sender().clone();
    fit_page_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::FitToPage);
    });
    fit_box.append(&fit_page_btn);

    let fit_width_btn = gtk::Button::with_label("Fit Width");
    fit_width_btn.add_css_class("kalam-reader-seg-btn");
    let tx = sender.input_sender().clone();
    fit_width_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::FitToWidth);
    });
    fit_box.append(&fit_width_btn);

    fit_row.append(&fit_box);
    zoom_section.append(&fit_row);

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

    let divider3 = gtk::Separator::new(gtk::Orientation::Horizontal);
    divider3.add_css_class("kalam-section-divider");
    wrap.append(&divider3);

    // ── 4. Navigation Section ─────────────────────────────────
    let nav_section = reader_settings_section("Document Navigation");

    let nav_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    nav_row.add_css_class("kalam-reader-setting-row");
    let nav_label = gtk::Label::new(Some("Jump to Page"));
    nav_label.add_css_class("kalam-reader-setting-name");
    nav_label.set_hexpand(true);
    nav_label.set_halign(gtk::Align::Start);
    nav_row.append(&nav_label);

    let nav_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    nav_box.add_css_class("linked");

    let first_page_btn = gtk::Button::with_label("First (1)");
    first_page_btn.add_css_class("kalam-reader-seg-btn");
    let tx = sender.input_sender().clone();
    first_page_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::GoToFirstPage);
    });
    nav_box.append(&first_page_btn);

    let last_page_btn = gtk::Button::with_label(&format!("Last ({})", model.total_pages));
    last_page_btn.add_css_class("kalam-reader-seg-btn");
    let tx = sender.input_sender().clone();
    last_page_btn.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::GoToLastPage);
    });
    nav_box.append(&last_page_btn);

    nav_row.append(&nav_box);
    nav_section.append(&nav_row);

    wrap.append(&nav_section);

    let divider4 = gtk::Separator::new(gtk::Orientation::Horizontal);
    divider4.add_css_class("kalam-section-divider");
    wrap.append(&divider4);

    // ── 5. Margins & Enhancements Section ─────────────────────
    let crop_section = reader_settings_section("Margins & Enhancements");

    let crop_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    crop_row.add_css_class("kalam-reader-setting-row");
    let crop_label = gtk::Label::new(Some("Smart Crop (Trim Margins)"));
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
        "Auto-detects margins around ink content to maximize reading area while preserving ink safety buffers.",
    ));
    crop_hint.add_css_class("kalam-reader-setting-hint");
    crop_hint.set_wrap(true);
    crop_hint.set_xalign(0.0);
    crop_section.append(&crop_hint);

    wrap.append(&crop_section);

    let divider5 = gtk::Separator::new(gtk::Orientation::Horizontal);
    divider5.add_css_class("kalam-section-divider");
    wrap.append(&divider5);

    // ── 6. Document Properties Section ────────────────────────
    let info_section = reader_settings_section("Document Properties");

    let title_prop = prop_row("Title", &model.title);
    info_section.append(&title_prop);

    let author_prop = prop_row("Author", &model.author);
    info_section.append(&author_prop);

    let pages_prop = prop_row("Total Pages", &format!("{} pages", model.total_pages));
    info_section.append(&pages_prop);

    let engine_prop = prop_row("Renderer", "MuPDF v0.8.0 (RGBA)");
    info_section.append(&engine_prop);

    wrap.append(&info_section);

    let widgets = PdfSettingsWidgets {
        scroll_page_btn,
        scroll_vertical_btn,
        scroll_horizontal_btn,
        scroll_wrapped_btn,
        spread_none_btn,
        spread_odd_btn,
        spread_even_btn,
        gap_0_btn,
        gap_4_btn,
        gap_8_btn,
        gap_12_btn,
        gap_16_btn,
        spread_gap_box,
        smart_crop_switch,
        zoom_label,
    };

    (wrap, widgets)
}

fn prop_row(label: &str, value: &str) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("kalam-reader-setting-row");
    let name_lbl = gtk::Label::new(Some(label));
    name_lbl.add_css_class("dim-label");
    name_lbl.set_halign(gtk::Align::Start);
    name_lbl.set_hexpand(true);
    row.append(&name_lbl);

    let val_lbl = gtk::Label::new(Some(value));
    val_lbl.set_halign(gtk::Align::End);
    val_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
    val_lbl.set_max_width_chars(22);
    row.append(&val_lbl);
    row
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
