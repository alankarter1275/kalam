//! Modernized Comic and Manga Reader component for Kalam.
//!
//! Provides an immersive image-pager reader for CBZ/CBR comic archives and remote manga with:
//! - Unified floating chrome matching EPUB and PDF readers:
//!   - Floating top-left dock (`[← Library]` + `[Bookmark]` with live active state)
//!   - Floating bottom-center pill (`[Prev]`, `Page X of Y`, `[Next]`)
//!   - VLC-style HUD in top-right corner for zoom and fit mode feedback
//!   - Persistent hover detection zones on root and active pill hit-testing
//!   - Chrome reveals on hovering dock positions; autohides on scroll, page turns, or after 3.5 seconds
//! - Zen-style slide-in left sidebar:
//!   - Header with cover thumbnail, title, and reading progress bar
//!   - Expanding content area: Settings (default) and Bookmarks
//!   - Pinned bottom tab bar for fast switching between Bookmarks and Settings
//!   - Categorized settings panel: Page Style (Single, Double, Webtoon), Reading Direction (LTR, RTL/Manga),
//!     Spread Gap (0px seamless to 16px), Fit Mode, and Shortcuts reference
//! - Reading progress checkpointing, bookmark persistence, and memory release on close.

pub mod types;
pub mod provider;
pub mod providers;

pub use types::*;

use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use relm4::prelude::*;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use crate::db::ReadingBookmark;

pub struct ComicsReaderModel {
    pub title: String,
    pub provider: Arc<dyn provider::ImageProvider>,
    pub catalog: Option<Arc<crate::db::Catalog>>,
    pub book_id: Option<i64>,
    pub cover_path: Option<PathBuf>,
    pub current_page: usize,
    pub total_pages: usize,
    pub direction: ReadingDirection,
    pub page_style: PageStyle,
    pub fit_mode: FitMode,
    pub two_page_gap: i32,
    pub sidebar_tab: ComicSidebarTab,
    pub show_back_button: bool,
    pub show_bottom_pill: bool,
    pub show_sidebar: bool,
    pub sidebar_pinned: bool,
    pub mouse_in_top_edge: bool,
    pub mouse_in_bottom_edge: bool,
    pub mouse_in_left_edge: bool,
    pub mouse_in_sidebar: bool,
    pub back_hide_seq: u64,
    pub bottom_hide_seq: u64,
    pub sidebar_close_seq: u64,
    pub show_zoom_osd: bool,
    pub zoom_osd_seq: u64,
    pub osd_text: String,
    pub bookmarks: Vec<ReadingBookmark>,
    pub textures: HashMap<usize, gdk::Texture>,
    pub pending_loads: HashSet<usize>,
    pub session_id: Option<i64>,
    pub session_start: std::time::Instant,
    pub last_viewport_width: i32,
    pub bookmarks_list_box: Option<gtk::Box>,
    #[allow(dead_code)]
    pub series_name: Option<String>,
    pub series_chapters: Vec<crate::models::Book>,
    pub current_chapter_idx: Option<usize>,
    pub at_chapter_end: bool,
    pub chapters_list_box: Option<gtk::ListBox>,
}

#[allow(dead_code)]
enum DecodedPage {
    Rgba { width: i32, height: i32, bytes: Vec<u8> },
    RawBytes(Vec<u8>),
}

impl ComicsReaderModel {
    pub fn new(init: types::ComicsReaderInit) -> Self {
        let total_pages = init.provider.page_count();

        let (series_name, series_chapters) = if let (Some(ref catalog), Some(book_id)) = (&init.catalog, init.book_id) {
            if let Ok(Some(book)) = catalog.get_book(book_id) {
                let s_name = book.series.or_else(|| {
                    crate::comics::parse_comic_title(&book.title).series
                });
                if let Some(ref s) = s_name {
                    let mut chapters = catalog.books_in_series(s).unwrap_or_default();
                    if chapters.len() <= 1 {
                        let _ = catalog.backfill_comic_series();
                        chapters = catalog.books_in_series(s).unwrap_or_default();
                    }
                    (Some(s.clone()), chapters)
                } else {
                    (None, Vec::new())
                }
            } else {
                (None, Vec::new())
            }
        } else {
            (None, Vec::new())
        };

        let current_chapter_idx = init.book_id.and_then(|bid| {
            series_chapters.iter().position(|b| b.id == bid)
        });

        let saved_page = if let (Some(ref catalog), Some(book_id)) = (&init.catalog, init.book_id) {
            match catalog.get_reading_progress(book_id) {
                Ok(Some((page, _))) if page < total_pages => page,
                _ => 0,
            }
        } else {
            0
        };

        let bookmarks = if let (Some(ref catalog), Some(book_id)) = (&init.catalog, init.book_id) {
            let _ = catalog.mark_book_opened(book_id);
            catalog.list_reading_bookmarks(book_id).unwrap_or_default()
        } else {
            Vec::new()
        };

        let session_id = if let (Some(ref catalog), Some(book_id)) = (&init.catalog, init.book_id) {
            let start_pct = match catalog.get_book(book_id) {
                Ok(Some(b)) => b.progress as i64,
                _ => 0,
            };
            catalog.start_reading_session(book_id, start_pct).ok()
        } else {
            None
        };

        // Preference resolution: per-book -> global default -> built-in default
        let (direction, page_style, fit_mode, two_page_gap) = if let Some(ref catalog) = init.catalog {
            let bid = init.book_id;
            let dir_pref = bid
                .and_then(|id| catalog.get_pref(&format!("book.{id}.comic.direction")))
                .or_else(|| catalog.get_pref("reader.comic.direction"));
            let dir = match dir_pref.as_deref() {
                Some("rtl") => ReadingDirection::Rtl,
                Some("webtoon") => ReadingDirection::Webtoon,
                _ => ReadingDirection::Ltr,
            };

            let style_pref = bid
                .and_then(|id| catalog.get_pref(&format!("book.{id}.comic.page_style")))
                .or_else(|| catalog.get_pref("reader.comic.page_style"));
            let style = match style_pref.as_deref() {
                Some("double") => PageStyle::Double,
                Some("fade") => PageStyle::Fade,
                Some("strip") => PageStyle::LongStrip,
                _ => PageStyle::Single,
            };

            let fit_pref = bid
                .and_then(|id| catalog.get_pref(&format!("book.{id}.comic.fit_mode")))
                .or_else(|| catalog.get_pref("reader.comic.fit_mode"));
            let fit = match fit_pref.as_deref() {
                Some("height") => FitMode::Height,
                Some("screen") => FitMode::Screen,
                Some("original") => FitMode::Original,
                _ => FitMode::Width,
            };

            let gap = catalog.get_pref_i64("reader.comic.two_page_gap", 0).clamp(0, 48) as i32;

            (dir, style, fit, gap)
        } else {
            (
                ReadingDirection::Ltr,
                PageStyle::Single,
                FitMode::Width,
                0,
            )
        };

        Self {
            title: init.title,
            provider: init.provider,
            catalog: init.catalog,
            book_id: init.book_id,
            cover_path: init.cover_path,
            current_page: saved_page,
            total_pages,
            direction,
            page_style,
            fit_mode,
            two_page_gap,
            sidebar_tab: ComicSidebarTab::Settings,
            show_back_button: true,
            show_bottom_pill: true,
            show_sidebar: false,
            sidebar_pinned: false,
            mouse_in_top_edge: false,
            mouse_in_bottom_edge: false,
            mouse_in_left_edge: false,
            mouse_in_sidebar: false,
            back_hide_seq: 0,
            bottom_hide_seq: 0,
            sidebar_close_seq: 0,
            show_zoom_osd: false,
            zoom_osd_seq: 0,
            osd_text: String::new(),
            bookmarks,
            textures: HashMap::new(),
            pending_loads: HashSet::new(),
            session_id,
            session_start: std::time::Instant::now(),
            last_viewport_width: 0,
            bookmarks_list_box: None,
            series_name,
            series_chapters,
            current_chapter_idx,
            at_chapter_end: false,
            chapters_list_box: None,
        }
    }

    pub fn next_chapter(&self) -> Option<&crate::models::Book> {
        let idx = self.current_chapter_idx?;
        self.series_chapters.get(idx + 1)
    }

    pub fn prev_chapter(&self) -> Option<&crate::models::Book> {
        let idx = self.current_chapter_idx?;
        if idx > 0 {
            self.series_chapters.get(idx - 1)
        } else {
            None
        }
    }

    pub fn reload_bookmarks(&mut self) {
        if let (Some(ref catalog), Some(book_id)) = (&self.catalog, self.book_id) {
            self.bookmarks = catalog.list_reading_bookmarks(book_id).unwrap_or_default();
        }
    }

    pub fn progress_fraction(&self) -> f64 {
        if self.total_pages > 0 {
            (self.current_page + 1) as f64 / self.total_pages as f64
        } else {
            0.0
        }
    }

    pub fn save_progress(&self) {
        if let (Some(ref catalog), Some(book_id)) = (&self.catalog, self.book_id) {
            let fraction = self.progress_fraction();
            let _ = catalog.set_reading_progress(
                book_id,
                self.current_page,
                fraction,
                self.total_pages,
            );

            if let Some(sid) = self.session_id {
                let elapsed = self.session_start.elapsed().as_secs() as i64;
                let end_pct = (fraction * 100.0).round() as i64;
                let _ = catalog.checkpoint_reading_session(sid, elapsed, end_pct);
            }
        }
    }

    /// Ask the provider for images in the window `curr ± 2` (or wider for webtoon/double).
    pub fn get_missing_pages(&mut self) -> Vec<usize> {
        let total = self.total_pages;
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

        // Retain textures and pending loads inside active preloading range to bound memory
        self.textures.retain(|&idx, _| idx >= min_keep && idx <= max_keep);
        self.pending_loads.retain(|&idx| idx >= min_keep && idx <= max_keep);

        let mut missing = Vec::new();
        for idx in min_keep..=max_keep {
            if !self.textures.contains_key(&idx) && !self.pending_loads.contains(&idx) {
                missing.push(idx);
            }
        }
        missing
    }

    pub fn set_page(&mut self, idx: usize) {
        if self.total_pages == 0 {
            return;
        }
        let clamped = idx.clamp(0, self.total_pages - 1);
        self.current_page = clamped;
    }

    pub fn trigger_loads(&mut self, sender: &relm4::ComponentSender<Self>) {
        let missing = self.get_missing_pages();
        let provider = self.provider.clone();

        for idx in missing {
            self.pending_loads.insert(idx);
            let s = sender.input_sender().clone();
            let prov = provider.clone();
            crate::tasks::spawn_internal(
                "Loading comic page",
                move |_| -> Option<DecodedPage> {
                    let b = match prov.fetch_page(idx) {
                        Ok(bytes) => bytes,
                        Err(err) => {
                            eprintln!("kalam: comic reader failed to fetch page {idx}: {err}");
                            return None;
                        }
                    };
                    if let Ok(img) = image::load_from_memory(&b) {
                        let rgba = img.to_rgba8();
                        let width = rgba.width() as i32;
                        let height = rgba.height() as i32;
                        Some(DecodedPage::Rgba {
                            width,
                            height,
                            bytes: rgba.into_raw(),
                        })
                    } else {
                        Some(DecodedPage::RawBytes(b))
                    }
                },
                |_| {},
                move |decoded_opt| {
                    let tex_opt = match decoded_opt {
                        Some(DecodedPage::Rgba { width, height, bytes }) => {
                            let stride = (width * 4) as usize;
                            let gbytes = glib::Bytes::from_owned(bytes);
                            let mem_tex = gdk::MemoryTexture::new(
                                width,
                                height,
                                gdk::MemoryFormat::R8g8b8a8,
                                &gbytes,
                                stride,
                            );
                            Some(mem_tex.upcast::<gdk::Texture>())
                        }
                        Some(DecodedPage::RawBytes(b)) => {
                            gdk::Texture::from_bytes(&glib::Bytes::from(&b)).ok()
                        }
                        None => None,
                    };
                    let _ = s.send(types::ComicsReaderMsg::PageLoaded(idx, tex_opt));
                },
            );
        }
    }

    pub fn next_page(&mut self) {
        if self.total_pages == 0 {
            return;
        }

        if self.page_style == PageStyle::Double {
            let max_start = if self.total_pages > 1 {
                if self.total_pages.is_multiple_of(2) {
                    self.total_pages - 2
                } else {
                    self.total_pages - 1
                }
            } else {
                0
            };
            let next = self.current_page + 2;
            if next <= max_start {
                self.set_page(next);
            } else {
                self.set_page(max_start);
            }
        } else if self.current_page + 1 < self.total_pages {
            self.set_page(self.current_page + 1);
        }
    }

    pub fn prev_page(&mut self) {
        let step = if self.page_style == PageStyle::Double { 2 } else { 1 };
        self.set_page(self.current_page.saturating_sub(step));
    }

    pub fn page_indicator_label(&self) -> String {
        if self.at_chapter_end {
            if let Some(next) = self.next_chapter() {
                return format!("Completed · Next: {}", next.title);
            } else {
                return "Series Completed 🎉".to_string();
            }
        }
        if self.total_pages == 0 {
            return "Page 0 of 0".to_string();
        }
        if self.page_style == PageStyle::Double {
            if self.current_page + 1 < self.total_pages {
                format!(
                    "Pages {}-{} of {}",
                    self.current_page + 1,
                    self.current_page + 2,
                    self.total_pages
                )
            } else {
                format!("Page {} of {}", self.current_page + 1, self.total_pages)
            }
        } else {
            format!("Page {} of {}", self.current_page + 1, self.total_pages)
        }
    }

    pub fn trigger_osd(&mut self, text: &str, sender: &ComponentSender<Self>) {
        self.osd_text = text.to_string();
        self.show_zoom_osd = true;
        self.zoom_osd_seq = self.zoom_osd_seq.wrapping_add(1);
        let seq = self.zoom_osd_seq;
        let s = sender.clone();
        glib::timeout_add_local_once(Duration::from_millis(1000), move || {
            let _ = s.input_sender().send(ComicsReaderMsg::HideOsd(seq));
        });
    }

    pub fn schedule_back_hide(&mut self, sender: &ComponentSender<Self>) {
        self.back_hide_seq = self.back_hide_seq.wrapping_add(1);
        let seq = self.back_hide_seq;
        let s = sender.clone();
        glib::timeout_add_local_once(Duration::from_millis(3500), move || {
            let _ = s.input_sender().send(ComicsReaderMsg::BackHideTimerTick(seq));
        });
    }

    pub fn schedule_bottom_hide(&mut self, sender: &ComponentSender<Self>) {
        self.bottom_hide_seq = self.bottom_hide_seq.wrapping_add(1);
        let seq = self.bottom_hide_seq;
        let s = sender.clone();
        glib::timeout_add_local_once(Duration::from_millis(3500), move || {
            let _ = s.input_sender().send(ComicsReaderMsg::BottomHideTimerTick(seq));
        });
    }

    pub fn schedule_sidebar_close(&mut self, sender: &ComponentSender<Self>) {
        self.sidebar_close_seq = self.sidebar_close_seq.wrapping_add(1);
        let seq = self.sidebar_close_seq;
        let s = sender.clone();
        glib::timeout_add_local_once(Duration::from_millis(700), move || {
            let _ = s.input_sender().send(ComicsReaderMsg::SidebarCloseTimerTick(seq));
        });
    }

    pub fn hide_chrome_on_interaction(&mut self) {
        if self.show_back_button && !self.mouse_in_top_edge {
            self.show_back_button = false;
            self.back_hide_seq = self.back_hide_seq.wrapping_add(1);
        }
        if self.show_bottom_pill && !self.mouse_in_bottom_edge {
            self.show_bottom_pill = false;
            self.bottom_hide_seq = self.bottom_hide_seq.wrapping_add(1);
        }
    }

    pub fn update_bookmark_icon_state(&self, widgets: &ComicsReaderModelWidgets) {
        let is_bookmarked = self
            .bookmarks
            .iter()
            .any(|b| b.chapter_index as usize == self.current_page);
        toggle_active(&widgets.top_bookmark_btn, is_bookmarked);
    }

    pub fn sync_settings_ui(&self, widgets: &ComicsReaderModelWidgets) {
        toggle_active(&widgets.style_single_btn, self.page_style == PageStyle::Single);
        toggle_active(&widgets.style_double_btn, self.page_style == PageStyle::Double);
        toggle_active(&widgets.style_strip_btn, self.page_style == PageStyle::LongStrip);

        toggle_active(&widgets.dir_ltr_btn, self.direction == ReadingDirection::Ltr);
        toggle_active(&widgets.dir_rtl_btn, self.direction == ReadingDirection::Rtl);

        widgets.gap_box.set_sensitive(self.page_style == PageStyle::Double);
        toggle_active(&widgets.gap_0_btn, self.two_page_gap == 0);
        toggle_active(&widgets.gap_4_btn, self.two_page_gap == 4);
        toggle_active(&widgets.gap_8_btn, self.two_page_gap == 8);
        toggle_active(&widgets.gap_12_btn, self.two_page_gap == 12);
        toggle_active(&widgets.gap_16_btn, self.two_page_gap == 16);

        toggle_active(&widgets.fit_width_btn, self.fit_mode == FitMode::Width);
        toggle_active(&widgets.fit_height_btn, self.fit_mode == FitMode::Height);
        toggle_active(&widgets.fit_screen_btn, self.fit_mode == FitMode::Screen);
        toggle_active(&widgets.fit_orig_btn, self.fit_mode == FitMode::Original);
    }

    pub fn cleanup_memory(&mut self) {
        self.textures.clear();
        self.pending_loads.clear();

        #[cfg(target_os = "linux")]
        unsafe {
            libc::malloc_trim(0);
        }
    }
}

impl Drop for ComicsReaderModel {
    fn drop(&mut self) {
        if let (Some(ref catalog), Some(sid)) = (&self.catalog, self.session_id) {
            let elapsed = self.session_start.elapsed().as_secs() as i64;
            let pct = if self.total_pages > 0 {
                (((self.current_page + 1) as f64 / self.total_pages as f64) * 100.0).round() as i64
            } else {
                0
            };
            let _ = catalog.end_reading_session(sid, elapsed, pct);
        }
        self.cleanup_memory();
    }
}

fn get_webtoon_vbox(scrolled_window: &gtk::ScrolledWindow) -> Option<gtk::Box> {
    let child = scrolled_window.child()?;
    if child.has_css_class("kalam-webtoon-container") {
        child.downcast::<gtk::Box>().ok()
    } else if let Ok(vp) = child.downcast::<gtk::Viewport>() {
        vp.child().and_then(|c| c.downcast::<gtk::Box>().ok())
    } else {
        None
    }
}

fn update_viewport_policies(
    scrolled_window: &gtk::ScrolledWindow,
    fit_mode: FitMode,
    is_webtoon: bool,
) {
    if is_webtoon {
        scrolled_window.set_hscrollbar_policy(gtk::PolicyType::Never);
        scrolled_window.set_vscrollbar_policy(gtk::PolicyType::Always);
    } else {
        match fit_mode {
            FitMode::Screen => {
                scrolled_window.set_hscrollbar_policy(gtk::PolicyType::Never);
                scrolled_window.set_vscrollbar_policy(gtk::PolicyType::Never);
            }
            FitMode::Width => {
                scrolled_window.set_hscrollbar_policy(gtk::PolicyType::Never);
                scrolled_window.set_vscrollbar_policy(gtk::PolicyType::Automatic);
            }
            FitMode::Height => {
                scrolled_window.set_hscrollbar_policy(gtk::PolicyType::Automatic);
                scrolled_window.set_vscrollbar_policy(gtk::PolicyType::Never);
            }
            FitMode::Original => {
                scrolled_window.set_hscrollbar_policy(gtk::PolicyType::Automatic);
                scrolled_window.set_vscrollbar_policy(gtk::PolicyType::Automatic);
            }
        }
    }
}

fn rebuild_viewport_widget(
    model: &ComicsReaderModel,
    viewport_width: i32,
    sender: Option<&ComponentSender<ComicsReaderModel>>,
) -> gtk::Widget {
    let win_w = if viewport_width > 100 { viewport_width } else { 850 };
    let is_webtoon = model.direction == ReadingDirection::Webtoon || model.page_style == PageStyle::LongStrip;

    if !is_webtoon && model.at_chapter_end {
        let card_box = gtk::Box::new(gtk::Orientation::Vertical, 16);
        card_box.set_halign(gtk::Align::Center);
        card_box.set_valign(gtk::Align::Center);
        card_box.set_hexpand(true);
        card_box.set_vexpand(true);
        card_box.set_margin_top(48);
        card_box.set_margin_bottom(48);
        card_box.add_css_class("kalam-card");
        card_box.set_size_request(400, -1);

        let icon = gtk::Image::from_icon_name("emblem-ok-symbolic");
        icon.set_pixel_size(48);
        card_box.append(&icon);

        let title_lbl = gtk::Label::new(Some(&format!("{} Completed!", model.title)));
        title_lbl.add_css_class("kalam-title-medium");
        title_lbl.set_wrap(true);
        title_lbl.set_justify(gtk::Justification::Center);
        card_box.append(&title_lbl);

        if let Some(next_ch) = model.next_chapter() {
            let next_lbl = gtk::Label::new(Some(&format!("Next Up: {}", next_ch.title)));
            next_lbl.add_css_class("kalam-subtitle-muted");
            next_lbl.set_wrap(true);
            next_lbl.set_justify(gtk::Justification::Center);
            card_box.append(&next_lbl);

            let next_btn = gtk::Button::with_label("Read Next Chapter (Space / →)");
            next_btn.add_css_class("kalam-btn-filled");
            next_btn.set_halign(gtk::Align::Center);
            if let Some(s) = sender {
                let s_clone = s.clone();
                next_btn.connect_clicked(move |_| {
                    s_clone.input(ComicsReaderMsg::NextChapter);
                });
            }
            card_box.append(&next_btn);
        } else {
            let done_lbl = gtk::Label::new(Some("🎉 You've reached the end of the series!"));
            done_lbl.add_css_class("kalam-subtitle-muted");
            card_box.append(&done_lbl);
        }

        let back_btn = gtk::Button::with_label("Back to Library (Esc)");
        back_btn.add_css_class("kalam-btn-subtle");
        back_btn.set_halign(gtk::Align::Center);
        if let Some(s) = sender {
            let s_clone = s.clone();
            back_btn.connect_clicked(move |_| {
                s_clone.input(ComicsReaderMsg::Close);
            });
        }
        card_box.append(&back_btn);

        return card_box.upcast();
    }

    if is_webtoon {
        let avail_w = (win_w - 24).max(300);
        let vbox = gtk::Box::new(gtk::Orientation::Vertical, 4);
        vbox.set_halign(gtk::Align::Center);
        vbox.set_valign(gtk::Align::Start);
        vbox.set_hexpand(true);
        vbox.add_css_class("kalam-webtoon-container");

        let total = model.total_pages;
        for idx in 0..total {
            let item_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
            item_box.set_halign(gtk::Align::Center);
            item_box.set_valign(gtk::Align::Start);
            item_box.set_hexpand(true);
            item_box.add_css_class("kalam-webtoon-page-item");

            if let Some(texture) = model.textures.get(&idx) {
                let tw = texture.width();
                let th = texture.height();
                let target_h = if tw > 0 {
                    ((avail_w as f64) * (th as f64 / tw as f64)).round() as i32
                } else {
                    -1
                };

                let pic = gtk::Picture::for_paintable(texture);
                pic.set_size_request(avail_w, target_h);
                pic.set_can_shrink(false);
                pic.set_content_fit(gtk::ContentFit::Contain);
                pic.set_halign(gtk::Align::Center);
                item_box.append(&pic);
            } else {
                let placeholder = gtk::Box::new(gtk::Orientation::Vertical, 0);
                placeholder.set_size_request(avail_w, (avail_w as f64 * 1.4).round() as i32);
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
        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, model.two_page_gap);
        hbox.set_halign(gtk::Align::Center);
        hbox.set_hexpand(true);
        if model.fit_mode == FitMode::Width {
            hbox.set_valign(gtk::Align::Start);
            hbox.set_vexpand(false);
        } else {
            hbox.set_valign(gtk::Align::Center);
            hbox.set_vexpand(true);
        }

        let page_a = model.current_page;
        let page_b = if model.current_page + 1 < model.total_pages {
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

        let create_page_widget = |opt_idx: Option<usize>, is_left: bool| -> gtk::Widget {
            let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
            container.set_hexpand(true);
            container.set_halign(if is_left { gtk::Align::End } else { gtk::Align::Start });
            if model.fit_mode == FitMode::Width {
                container.set_valign(gtk::Align::Start);
                container.set_vexpand(false);
            } else {
                container.set_valign(gtk::Align::Center);
                container.set_vexpand(true);
            }

            if let Some(idx) = opt_idx {
                if let Some(texture) = model.textures.get(&idx) {
                    let tw = texture.width();
                    let th = texture.height();
                    let pic = gtk::Picture::for_paintable(texture);
                    pic.set_halign(if is_left { gtk::Align::End } else { gtk::Align::Start });

                    match model.fit_mode {
                        FitMode::Screen => {
                            pic.set_can_shrink(true);
                            pic.set_content_fit(gtk::ContentFit::Contain);
                            pic.set_hexpand(true);
                            pic.set_vexpand(true);
                            pic.set_valign(gtk::Align::Center);
                            pic.set_size_request(-1, -1);
                        }
                        FitMode::Height => {
                            pic.set_can_shrink(true);
                            pic.set_content_fit(gtk::ContentFit::Contain);
                            pic.set_hexpand(false);
                            pic.set_vexpand(true);
                            pic.set_valign(gtk::Align::Center);
                            pic.set_size_request(-1, -1);
                        }
                        FitMode::Width => {
                            let avail_w = (win_w - 32 - model.two_page_gap).max(400) / 2;
                            let target_h = if tw > 0 {
                                ((avail_w as f64) * (th as f64 / tw as f64)).round() as i32
                            } else {
                                -1
                            };
                            pic.set_size_request(avail_w, target_h);
                            pic.set_can_shrink(false);
                            pic.set_content_fit(gtk::ContentFit::Contain);
                            pic.set_valign(gtk::Align::Start);
                        }
                        FitMode::Original => {
                            pic.set_size_request(tw, th);
                            pic.set_can_shrink(false);
                            pic.set_content_fit(gtk::ContentFit::Contain);
                            pic.set_valign(gtk::Align::Center);
                        }
                    }
                    container.append(&pic);
                } else {
                    let placeholder = gtk::Box::new(gtk::Orientation::Vertical, 0);
                    placeholder.set_size_request(320, 480);
                    placeholder.set_halign(gtk::Align::Center);
                    placeholder.set_valign(gtk::Align::Center);

                    let spinner = gtk::Spinner::new();
                    spinner.set_spinning(true);
                    spinner.set_size_request(36, 36);
                    spinner.set_halign(gtk::Align::Center);
                    spinner.set_valign(gtk::Align::Center);
                    placeholder.append(&spinner);
                    container.append(&placeholder);
                }
            } else {
                container.set_halign(gtk::Align::Center);
            }
            container.upcast()
        };

        hbox.append(&create_page_widget(left_idx, true));
        hbox.append(&create_page_widget(right_idx, false));

        return hbox.upcast();
    }

    // Single page mode
    let vbox = gtk::Box::new(gtk::Orientation::Vertical, 0);
    vbox.set_halign(gtk::Align::Center);
    vbox.set_hexpand(true);
    if model.fit_mode == FitMode::Width {
        vbox.set_valign(gtk::Align::Start);
        vbox.set_vexpand(false);
    } else {
        vbox.set_valign(gtk::Align::Center);
        vbox.set_vexpand(true);
    }

    if let Some(texture) = model.textures.get(&model.current_page) {
        let tw = texture.width();
        let th = texture.height();
        let pic = gtk::Picture::for_paintable(texture);
        pic.set_halign(gtk::Align::Center);

        match model.fit_mode {
            FitMode::Screen => {
                pic.set_can_shrink(true);
                pic.set_content_fit(gtk::ContentFit::Contain);
                pic.set_hexpand(true);
                pic.set_vexpand(true);
                pic.set_valign(gtk::Align::Center);
                pic.set_size_request(-1, -1);
            }
            FitMode::Height => {
                pic.set_can_shrink(true);
                pic.set_content_fit(gtk::ContentFit::Contain);
                pic.set_hexpand(false);
                pic.set_vexpand(true);
                pic.set_valign(gtk::Align::Center);
                pic.set_size_request(-1, -1);
            }
            FitMode::Width => {
                let avail_w = (win_w - 24).max(300);
                let target_h = if tw > 0 {
                    ((avail_w as f64) * (th as f64 / tw as f64)).round() as i32
                } else {
                    -1
                };
                pic.set_size_request(avail_w, target_h);
                pic.set_can_shrink(false);
                pic.set_content_fit(gtk::ContentFit::Contain);
                pic.set_valign(gtk::Align::Start);
            }
            FitMode::Original => {
                pic.set_size_request(tw, th);
                pic.set_can_shrink(false);
                pic.set_content_fit(gtk::ContentFit::Contain);
                pic.set_valign(gtk::Align::Center);
            }
        }
        vbox.append(&pic);
    } else {
        let spinner = gtk::Spinner::new();
        spinner.set_spinning(true);
        spinner.set_size_request(48, 48);
        spinner.set_halign(gtk::Align::Center);
        spinner.set_valign(gtk::Align::Center);
        vbox.append(&spinner);
    }

    vbox.upcast()
}

fn populate_chapters_list(
    list_box: &gtk::ListBox,
    chapters: &[crate::models::Book],
    current_bid: Option<i64>,
    sender: &ComponentSender<ComicsReaderModel>,
) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }
    if chapters.is_empty() {
        let empty = gtk::Label::new(Some("No chapters in series"));
        empty.add_css_class("dim-label");
        empty.set_margin_top(24);
        list_box.append(&empty);
        return;
    }

    for ch in chapters {
        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row_box.set_margin_start(16);
        row_box.set_margin_end(16);
        row_box.set_margin_top(8);
        row_box.set_margin_bottom(8);
        row_box.add_css_class("kalam-reader-chapter-row");
        if Some(ch.id) == current_bid {
            row_box.add_css_class("active");
        }

        let label_text = if ch.series_index > 0.0 {
            format!("Chapter {}", ch.series_index)
        } else {
            ch.title.clone()
        };

        let title_lbl = gtk::Label::new(Some(&label_text));
        title_lbl.add_css_class("kalam-title-small");
        title_lbl.set_halign(gtk::Align::Start);
        title_lbl.set_hexpand(true);
        title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row_box.append(&title_lbl);

        if ch.progress >= 100 {
            let check = gtk::Image::from_icon_name("emblem-ok-symbolic");
            check.set_pixel_size(14);
            check.add_css_class("dim-label");
            row_box.append(&check);
        } else if ch.progress > 0 {
            let pct = gtk::Label::new(Some(&format!("{}%", ch.progress)));
            pct.add_css_class("dim-label");
            row_box.append(&pct);
        }

        let btn = gtk::Button::new();
        btn.set_child(Some(&row_box));
        btn.add_css_class("kalam-btn-subtle");
        btn.set_hexpand(true);
        let bid = ch.id;
        let s = sender.clone();
        btn.connect_clicked(move |_| {
            s.input(ComicsReaderMsg::SwitchChapter(bid));
        });

        list_box.append(&btn);
    }
}

fn populate_bookmarks_list(
    list: &gtk::Box,
    bookmarks: &[ReadingBookmark],
    sender: &ComponentSender<ComicsReaderModel>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if bookmarks.is_empty() {
        let empty_lbl = gtk::Label::new(Some("No bookmarks saved yet.\nPress 'b' to bookmark the current page."));
        empty_lbl.add_css_class("kalam-reader-empty");
        empty_lbl.set_halign(gtk::Align::Center);
        empty_lbl.set_valign(gtk::Align::Center);
        empty_lbl.set_justify(gtk::Justification::Center);
        list.append(&empty_lbl);
        return;
    }

    for mark in bookmarks {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("kalam-reader-bookmark-row");
        row.set_hexpand(true);

        let jump_btn = gtk::Button::new();
        jump_btn.add_css_class("kalam-reader-list-hit");
        jump_btn.set_hexpand(true);

        let hbox = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        hbox.append(&crate::icons::symbolic_with_classes(
            "bookmark-new-symbolic",
            15,
            &["kalam-reader-bookmark-icon"],
        ));

        let lbl = gtk::Label::new(Some(&format!("Page {}", mark.chapter_index + 1)));
        lbl.add_css_class("kalam-reader-bookmark-title");
        lbl.set_halign(gtk::Align::Start);
        lbl.set_hexpand(true);
        hbox.append(&lbl);

        jump_btn.set_child(Some(&hbox));
        let page_target = mark.chapter_index as usize;
        let s_jump = sender.clone();
        jump_btn.connect_clicked(move |_| {
            let _ = s_jump.input_sender().send(ComicsReaderMsg::SetPage(page_target));
        });
        row.append(&jump_btn);

        let del_btn = gtk::Button::new();
        del_btn.set_child(Some(&crate::icons::symbolic_with_classes(
            "user-trash-symbolic",
            14,
            &["kalam-inline-icon"],
        )));
        del_btn.add_css_class("kalam-reader-pill-nav");
        del_btn.set_tooltip_text(Some("Delete Bookmark"));
        let mark_id = mark.id;
        let s_del = sender.clone();
        del_btn.connect_clicked(move |_| {
            let _ = s_del.input_sender().send(ComicsReaderMsg::DeleteBookmark(mark_id));
        });
        row.append(&del_btn);

        list.append(&row);
    }
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
    let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    box_.set_halign(gtk::Align::Center);
    box_.set_valign(gtk::Align::Center);
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

#[relm4::component(pub)]
impl Component for ComicsReaderModel {
    type Init = types::ComicsReaderInit;
    type Input = ComicsReaderMsg;
    type Output = ComicsReaderOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Overlay {
            add_css_class: "kalam-reader-overlay",
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

            // ── 2. Overlay: Dim Backdrop for Dismissing Left Sidebar ──
            #[name = "dim_backdrop"]
            add_overlay = &gtk::Button {
                add_css_class: "kalam-reader-dim",
                #[watch]
                set_visible: model.show_sidebar,
                set_hexpand: true,
                set_vexpand: true,
                set_halign: gtk::Align::Fill,
                set_valign: gtk::Align::Fill,
                connect_clicked => ComicsReaderMsg::CloseSidebar,
            },

            // ── 3. Overlay: Invisible Left Edge Hover Strip ───────────
            #[name = "left_edge_box"]
            add_overlay = &gtk::Box {
                add_css_class: "kalam-reader-hover-edge",
                add_css_class: "kalam-reader-hover-edge-left",
                set_width_request: 24,
                set_hexpand: false,
                set_vexpand: true,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Fill,
            },

            // ── 4. Floating Top-Left Back Dock ────────────────────────
            #[name = "top_back_revealer"]
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_back_button && !model.show_sidebar,
                set_transition_type: gtk::RevealerTransitionType::SlideDown,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Start,

                #[name = "back_dock"]
                gtk::Box {
                    add_css_class: "kalam-reader-back-dock",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 6,
                    set_margin_start: 24,
                    set_margin_top: 18,

                    gtk::Button {
                        set_child: Some(&crate::icons::labelled(
                            "go-previous-symbolic",
                            16,
                            "Library",
                            6,
                        )),
                        add_css_class: "kalam-reader-back",
                        set_tooltip_text: Some("Back to Library (Esc / Backspace)"),
                        connect_clicked => ComicsReaderMsg::Close,
                    },

                    #[name = "top_bookmark_btn"]
                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes(
                            "bookmark-new-symbolic",
                            16,
                            &["kalam-inline-icon"],
                        )),
                        add_css_class: "kalam-reader-back",
                        set_tooltip_text: Some("Bookmark Page (b)"),
                        connect_clicked => ComicsReaderMsg::ToggleBookmark,
                    },
                },
            },

            // ── 5. Floating Bottom Navigation Pill ────────────────────
            #[name = "bottom_pill_revealer"]
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_bottom_pill,
                set_transition_type: gtk::RevealerTransitionType::SlideUp,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::End,

                #[name = "bottom_dock"]
                gtk::Box {
                    add_css_class: "kalam-reader-bottom-dock",
                    set_margin_bottom: 24,

                    gtk::Box {
                        add_css_class: "kalam-reader-bottom-pill",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 4,
                        set_valign: gtk::Align::Center,

                        // Previous Page
                        gtk::Button {
                            set_child: Some(&crate::icons::symbolic_with_classes(
                                "go-previous-symbolic",
                                15,
                                &["kalam-inline-icon"],
                            )),
                            add_css_class: "kalam-reader-pill-nav",
                            set_tooltip_text: Some("Previous Page (Left / h)"),
                            connect_clicked => ComicsReaderMsg::PrevPage,
                        },

                        // Page Indicator Box
                        gtk::Box {
                            add_css_class: "kalam-reader-pill-info",
                            set_orientation: gtk::Orientation::Horizontal,
                            set_spacing: 6,
                            set_valign: gtk::Align::Center,

                            #[name = "page_indicator_label"]
                            gtk::Label {
                                add_css_class: "kalam-reader-pill-pages",
                                #[watch]
                                set_label: &model.page_indicator_label(),
                            },
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
                            connect_clicked => ComicsReaderMsg::NextPage,
                        },
                    },
                },
            },

            // ── 6. VLC-Style Top-Right HUD ─────────────────────────────
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_zoom_osd,
                set_transition_type: gtk::RevealerTransitionType::Crossfade,
                set_transition_duration: 250,
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Start,
                set_can_target: false,
                set_margin_top: 24,
                set_margin_end: 28,

                #[wrap(Some)]
                set_child = &gtk::Label {
                    add_css_class: "kalam-zoom-osd",
                    #[watch]
                    set_label: &model.osd_text,
                },
            },

            // ── 7. Zen-Style Left Sidebar (Settings + Bookmarks) ───────
            #[name = "sidebar_revealer"]
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
                    set_size_request: (340, -1),
                    set_margin_start: 8,
                    set_margin_top: 8,
                    set_margin_bottom: 8,
                    set_vexpand: true,
                    add_css_class: "kalam-reader-sidebar",
                    add_css_class: "kalam-reader-sidebar-left",

                    // 1. Header (Cover + Title + Progress + Close Button)
                    gtk::Box {
                        add_css_class: "kalam-reader-book-head",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 12,
                        set_halign: gtk::Align::Fill,

                        // Cover slot
                        #[name = "cover_host"]
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            add_css_class: "kalam-reader-cover-slot",
                            set_width_request: 48,
                        },

                        // Title & Progress info
                        gtk::Box {
                            set_orientation: gtk::Orientation::Vertical,
                            set_spacing: 4,
                            set_hexpand: true,
                            set_valign: gtk::Align::Center,

                            gtk::Label {
                                add_css_class: "kalam-reader-book-title",
                                #[watch]
                                set_label: &model.title,
                                set_halign: gtk::Align::Start,
                                set_wrap: true,
                                set_wrap_mode: gtk::pango::WrapMode::WordChar,
                                set_lines: 2,
                                set_ellipsize: gtk::pango::EllipsizeMode::End,
                            },

                            gtk::Label {
                                add_css_class: "kalam-reader-book-author",
                                #[watch]
                                set_label: &format!("Page {} of {}", model.current_page + 1, model.total_pages),
                                set_halign: gtk::Align::Start,
                            },

                            gtk::ProgressBar {
                                add_css_class: "kalam-reader-progress",
                                #[watch]
                                set_fraction: model.progress_fraction(),
                            },
                        },
                    },

                    // 2. Tab Content Stack (Middle, expanding)
                    #[name = "left_stack"]
                    gtk::Stack {
                        set_hexpand: true,
                        set_vexpand: true,
                        set_transition_type: gtk::StackTransitionType::Crossfade,

                        // Chapters View
                        #[name = "chapters_scroll"]
                        gtk::ScrolledWindow {
                            set_hexpand: true,
                            set_vexpand: true,
                            set_hscrollbar_policy: gtk::PolicyType::Never,

                            #[name = "chapters_list_box"]
                            gtk::ListBox {
                                set_selection_mode: gtk::SelectionMode::None,
                                add_css_class: "kalam-reader-chapter-list",
                            },
                        },

                        // Settings View (Default)
                        #[name = "settings_scroll"]
                        gtk::ScrolledWindow {
                            set_hexpand: true,
                            set_vexpand: true,
                            set_hscrollbar_policy: gtk::PolicyType::Never,

                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 12,
                                set_margin_top: 8,
                                set_margin_bottom: 24,

                                // Section: PAGE STYLE / FLOW
                                append = &reader_settings_section("Page Style / Flow"),
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 4,
                                    set_margin_start: 16,
                                    set_margin_end: 16,
                                    set_homogeneous: true,

                                    #[name = "style_single_btn"]
                                    gtk::Button {
                                        set_child: Some(&crate::icons::labelled(
                                            "view-paged-symbolic",
                                            16,
                                            "Single",
                                            6,
                                        )),
                                        add_css_class: "kalam-reader-seg-btn",
                                        set_tooltip_text: Some("Single page display"),
                                        connect_clicked => ComicsReaderMsg::SetPageStyle(PageStyle::Single),
                                    },

                                    #[name = "style_double_btn"]
                                    gtk::Button {
                                        set_child: Some(&crate::icons::labelled(
                                            "view-dual-symbolic",
                                            16,
                                            "Double",
                                            6,
                                        )),
                                        add_css_class: "kalam-reader-seg-btn",
                                        set_tooltip_text: Some("Two facing pages"),
                                        connect_clicked => ComicsReaderMsg::SetPageStyle(PageStyle::Double),
                                    },

                                    #[name = "style_strip_btn"]
                                    gtk::Button {
                                        set_child: Some(&crate::icons::labelled(
                                            "format-justify-fill-symbolic",
                                            16,
                                            "Webtoon",
                                            6,
                                        )),
                                        add_css_class: "kalam-reader-seg-btn",
                                        set_tooltip_text: Some("Continuous vertical strip"),
                                        connect_clicked => ComicsReaderMsg::SetPageStyle(PageStyle::LongStrip),
                                    },
                                },

                                // Section: READING DIRECTION
                                append = &reader_settings_section("Reading Direction"),
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 4,
                                    set_margin_start: 16,
                                    set_margin_end: 16,
                                    set_homogeneous: true,

                                    #[name = "dir_ltr_btn"]
                                    gtk::Button {
                                        set_child: Some(&crate::icons::labelled(
                                            "go-next-symbolic",
                                            16,
                                            "Left → Right",
                                            6,
                                        )),
                                        add_css_class: "kalam-reader-seg-btn",
                                        set_tooltip_text: Some("Left to Right (Western comics)"),
                                        connect_clicked => ComicsReaderMsg::SetDirection(ReadingDirection::Ltr),
                                    },

                                    #[name = "dir_rtl_btn"]
                                    gtk::Button {
                                        set_child: Some(&crate::icons::labelled(
                                            "go-previous-symbolic",
                                            16,
                                            "Right → Left",
                                            6,
                                        )),
                                        add_css_class: "kalam-reader-seg-btn",
                                        set_tooltip_text: Some("Right to Left (Manga)"),
                                        connect_clicked => ComicsReaderMsg::SetDirection(ReadingDirection::Rtl),
                                    },
                                },

                                // Section: SPREAD GAP
                                append = &reader_settings_section("Spread Gap"),
                                #[name = "gap_box"]
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 4,
                                    set_margin_start: 16,
                                    set_margin_end: 16,
                                    set_homogeneous: true,
                                    add_css_class: "kalam-reader-spread-gap-box",

                                    #[name = "gap_0_btn"]
                                    gtk::Button {
                                        set_label: "0px",
                                        add_css_class: "kalam-reader-seg-btn",
                                        set_tooltip_text: Some("Seamless (No gap between facing pages)"),
                                        connect_clicked => ComicsReaderMsg::SetSpreadGap(0),
                                    },

                                    #[name = "gap_4_btn"]
                                    gtk::Button {
                                        set_label: "4px",
                                        add_css_class: "kalam-reader-seg-btn",
                                        set_tooltip_text: Some("4px spread gap"),
                                        connect_clicked => ComicsReaderMsg::SetSpreadGap(4),
                                    },

                                    #[name = "gap_8_btn"]
                                    gtk::Button {
                                        set_label: "8px",
                                        add_css_class: "kalam-reader-seg-btn",
                                        set_tooltip_text: Some("8px spread gap"),
                                        connect_clicked => ComicsReaderMsg::SetSpreadGap(8),
                                    },

                                    #[name = "gap_12_btn"]
                                    gtk::Button {
                                        set_label: "12px",
                                        add_css_class: "kalam-reader-seg-btn",
                                        set_tooltip_text: Some("12px spread gap"),
                                        connect_clicked => ComicsReaderMsg::SetSpreadGap(12),
                                    },

                                    #[name = "gap_16_btn"]
                                    gtk::Button {
                                        set_label: "16px",
                                        add_css_class: "kalam-reader-seg-btn",
                                        set_tooltip_text: Some("16px spread gap"),
                                        connect_clicked => ComicsReaderMsg::SetSpreadGap(16),
                                    },
                                },

                                // Section: FIT MODE
                                append = &reader_settings_section("Fit Mode"),
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 4,
                                    set_margin_start: 16,
                                    set_margin_end: 16,
                                    set_homogeneous: true,

                                    #[name = "fit_width_btn"]
                                    gtk::Button {
                                        set_child: Some(&crate::icons::labelled(
                                            "zoom-fit-best-symbolic",
                                            16,
                                            "Width",
                                            4,
                                        )),
                                        add_css_class: "kalam-reader-seg-btn",
                                        set_tooltip_text: Some("Fit image to viewport width"),
                                        connect_clicked => ComicsReaderMsg::SetFitMode(FitMode::Width),
                                    },

                                    #[name = "fit_height_btn"]
                                    gtk::Button {
                                        set_child: Some(&crate::icons::labelled(
                                            "view-fullscreen-symbolic",
                                            16,
                                            "Height",
                                            4,
                                        )),
                                        add_css_class: "kalam-reader-seg-btn",
                                        set_tooltip_text: Some("Fit image to viewport height"),
                                        connect_clicked => ComicsReaderMsg::SetFitMode(FitMode::Height),
                                    },

                                    #[name = "fit_screen_btn"]
                                    gtk::Button {
                                        set_child: Some(&crate::icons::labelled(
                                            "zoom-fit-best-symbolic",
                                            16,
                                            "Screen",
                                            4,
                                        )),
                                        add_css_class: "kalam-reader-seg-btn",
                                        set_tooltip_text: Some("Fit entire image inside screen"),
                                        connect_clicked => ComicsReaderMsg::SetFitMode(FitMode::Screen),
                                    },

                                    #[name = "fit_orig_btn"]
                                    gtk::Button {
                                        set_child: Some(&crate::icons::labelled(
                                            "zoom-original-symbolic",
                                            16,
                                            "1:1",
                                            4,
                                        )),
                                        add_css_class: "kalam-reader-seg-btn",
                                        set_tooltip_text: Some("Display at 100% original size"),
                                        connect_clicked => ComicsReaderMsg::SetFitMode(FitMode::Original),
                                    },
                                },

                                // Section: SHORTCUTS
                                append = &reader_settings_section("Shortcuts"),
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_spacing: 6,
                                    set_margin_start: 16,
                                    set_margin_end: 16,

                                    gtk::Label {
                                        set_label: "← / →, h / l      Turn page",
                                        add_css_class: "dim-label",
                                        set_halign: gtk::Align::Start,
                                    },
                                    gtk::Label {
                                        set_label: "Space / PgDn      Next page",
                                        add_css_class: "dim-label",
                                        set_halign: gtk::Align::Start,
                                    },
                                    gtk::Label {
                                        set_label: "PgUp              Previous page",
                                        add_css_class: "dim-label",
                                        set_halign: gtk::Align::Start,
                                    },
                                    gtk::Label {
                                        set_label: "Click sides       Turn page",
                                        add_css_class: "dim-label",
                                        set_halign: gtk::Align::Start,
                                    },
                                    gtk::Label {
                                        set_label: "Click center      Toggle chrome",
                                        add_css_class: "dim-label",
                                        set_halign: gtk::Align::Start,
                                    },
                                    gtk::Label {
                                        set_label: "b                 Bookmark page",
                                        add_css_class: "dim-label",
                                        set_halign: gtk::Align::Start,
                                    },
                                    gtk::Label {
                                        set_label: "f                 Cycle fit mode",
                                        add_css_class: "dim-label",
                                        set_halign: gtk::Align::Start,
                                    },
                                    gtk::Label {
                                        set_label: "t / s             Toggle sidebar",
                                        add_css_class: "dim-label",
                                        set_halign: gtk::Align::Start,
                                    },
                                    gtk::Label {
                                        set_label: "Esc / Backspace   Back to Library",
                                        add_css_class: "dim-label",
                                        set_halign: gtk::Align::Start,
                                    },
                                },
                            },
                        },

                        // Bookmarks Tab View
                        #[name = "bookmarks_scroll"]
                        gtk::ScrolledWindow {
                            set_hexpand: true,
                            set_vexpand: true,
                            set_hscrollbar_policy: gtk::PolicyType::Never,

                            #[name = "bookmarks_list_box"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 0,
                            },
                        },
                    },

                    // 3. Tab Bar (Pinned to bottom)
                    gtk::Box {
                        add_css_class: "kalam-reader-tabbar",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 4,
                        set_homogeneous: true,

                        #[name = "tab_chapters_btn"]
                        gtk::Button {
                            add_css_class: "kalam-reader-tab",
                            #[watch]
                            set_visible: model.series_chapters.len() > 1,
                            #[watch]
                            set_css_classes: if model.sidebar_tab == ComicSidebarTab::Chapters {
                                &["kalam-reader-tab", "active"]
                            } else {
                                &["kalam-reader-tab"]
                            },
                            set_child: Some(&reader_sidebar_tab_content("view-list-bullet-symbolic", "Chapters")),
                            connect_clicked => ComicsReaderMsg::SetSidebarTab(ComicSidebarTab::Chapters),
                        },

                        #[name = "tab_settings_btn"]
                        gtk::Button {
                            add_css_class: "kalam-reader-tab",
                            #[watch]
                            set_css_classes: if model.sidebar_tab == ComicSidebarTab::Settings {
                                &["kalam-reader-tab", "active"]
                            } else {
                                &["kalam-reader-tab"]
                            },
                            set_child: Some(&reader_sidebar_tab_content("emblem-system-symbolic", "Settings")),
                            connect_clicked => ComicsReaderMsg::SetSidebarTab(ComicSidebarTab::Settings),
                        },

                        #[name = "tab_bookmarks_btn"]
                        gtk::Button {
                            add_css_class: "kalam-reader-tab",
                            #[watch]
                            set_css_classes: if model.sidebar_tab == ComicSidebarTab::Bookmarks {
                                &["kalam-reader-tab", "active"]
                            } else {
                                &["kalam-reader-tab"]
                            },
                            set_child: Some(&reader_sidebar_tab_content("bookmark-new-symbolic", "Bookmarks")),
                            connect_clicked => ComicsReaderMsg::SetSidebarTab(ComicSidebarTab::Bookmarks),
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

        let cover_w = crate::widgets::book_row::cover_widget(model.cover_path.as_deref(), 48, 70);
        widgets.cover_host.append(&cover_w);

        model.bookmarks_list_box = Some(widgets.bookmarks_list_box.clone());

        populate_bookmarks_list(&widgets.bookmarks_list_box, &model.bookmarks, &sender);
        model.update_bookmark_icon_state(&widgets);
        model.sync_settings_ui(&widgets);

        // Explicitly register stack page names so GTK child lookup succeeds without warnings
        widgets.left_stack.page(&widgets.chapters_scroll).set_name("chapters");
        widgets.left_stack.page(&widgets.settings_scroll).set_name("settings");
        widgets.left_stack.page(&widgets.bookmarks_scroll).set_name("bookmarks");
        widgets.left_stack.set_visible_child_name("settings");

        model.chapters_list_box = Some(widgets.chapters_list_box.clone());
        populate_chapters_list(&widgets.chapters_list_box, &model.series_chapters, model.book_id, &sender);

        model.schedule_back_hide(&sender);
        model.schedule_bottom_hide(&sender);

        let is_webtoon = model.direction == ReadingDirection::Webtoon || model.page_style == PageStyle::LongStrip;
        update_viewport_policies(&widgets.viewport_box, model.fit_mode, is_webtoon);
        let child = rebuild_viewport_widget(&model, 850, Some(&sender));
        widgets.viewport_box.set_child(Some(&child));

        // 1. Motion controller on root (Edge hover zones & sidebar hover tracking)
        let root_motion = gtk::EventControllerMotion::new();
        let tx_rm = sender.clone();
        let root_clone = root.clone();
        let was_top = std::rc::Rc::new(std::cell::Cell::new(false));
        let was_bottom = std::rc::Rc::new(std::cell::Cell::new(false));
        let was_left_edge = std::rc::Rc::new(std::cell::Cell::new(false));
        let was_sidebar_zone = std::rc::Rc::new(std::cell::Cell::new(false));
        let was_top_clone = was_top.clone();
        let was_bottom_clone = was_bottom.clone();
        let was_left_edge_clone = was_left_edge.clone();
        let was_sidebar_zone_clone = was_sidebar_zone.clone();

        root_motion.connect_motion(move |_, x, y| {
            let w = root_clone.width() as f64;
            let h = root_clone.height() as f64;
            // Only trigger top dock when hovering over top-left area where the pill lives
            let top = x < 240.0 && y < 75.0;
            // Only trigger bottom dock when hovering over bottom-center area where the pill lives
            let bottom = y > (h - 75.0) && h > 75.0 && (x - (w / 2.0)).abs() < 180.0;
            // Left edge zone for revealing sidebar
            let left_edge = x < 24.0;
            // Entire sidebar width region (sidebar is 340px wide + margins)
            let in_sidebar = x < 360.0;

            if top != was_top_clone.get() {
                was_top_clone.set(top);
                let _ = tx_rm.input_sender().send(ComicsReaderMsg::TopEdgeHover(top));
            }
            if bottom != was_bottom_clone.get() {
                was_bottom_clone.set(bottom);
                let _ = tx_rm.input_sender().send(ComicsReaderMsg::BottomEdgeHover(bottom));
            }
            if left_edge != was_left_edge_clone.get() {
                was_left_edge_clone.set(left_edge);
                let _ = tx_rm.input_sender().send(ComicsReaderMsg::LeftEdgeHover(left_edge));
            }
            if in_sidebar != was_sidebar_zone_clone.get() {
                was_sidebar_zone_clone.set(in_sidebar);
                let _ = tx_rm.input_sender().send(ComicsReaderMsg::SidebarHover(in_sidebar));
            }
        });

        let tx_leave = sender.clone();
        let was_top_leave = was_top.clone();
        let was_bottom_leave = was_bottom.clone();
        let was_left_edge_leave = was_left_edge.clone();
        let was_sidebar_zone_leave = was_sidebar_zone.clone();
        root_motion.connect_leave(move |_| {
            if was_top_leave.get() {
                was_top_leave.set(false);
                let _ = tx_leave.input_sender().send(ComicsReaderMsg::TopEdgeHover(false));
            }
            if was_bottom_leave.get() {
                was_bottom_leave.set(false);
                let _ = tx_leave.input_sender().send(ComicsReaderMsg::BottomEdgeHover(false));
            }
            if was_left_edge_leave.get() {
                was_left_edge_leave.set(false);
                let _ = tx_leave.input_sender().send(ComicsReaderMsg::LeftEdgeHover(false));
            }
            if was_sidebar_zone_leave.get() {
                was_sidebar_zone_leave.set(false);
                let _ = tx_leave.input_sender().send(ComicsReaderMsg::SidebarHover(false));
            }
        });
        root.add_controller(root_motion);

        // 2. Motion controllers on floating docks (so hovering them preserves visibility)
        let top_motion = gtk::EventControllerMotion::new();
        let s_tm = sender.clone();
        top_motion.connect_enter(move |_, _, _| {
            let _ = s_tm.input_sender().send(ComicsReaderMsg::TopEdgeHover(true));
        });
        let s_tml = sender.clone();
        top_motion.connect_leave(move |_| {
            let _ = s_tml.input_sender().send(ComicsReaderMsg::TopEdgeHover(false));
        });
        widgets.back_dock.add_controller(top_motion);

        let bottom_motion = gtk::EventControllerMotion::new();
        let s_bm = sender.clone();
        bottom_motion.connect_enter(move |_, _, _| {
            let _ = s_bm.input_sender().send(ComicsReaderMsg::BottomEdgeHover(true));
        });
        let s_bml = sender.clone();
        bottom_motion.connect_leave(move |_| {
            let _ = s_bml.input_sender().send(ComicsReaderMsg::BottomEdgeHover(false));
        });
        widgets.bottom_dock.add_controller(bottom_motion);

        // 3. Motion controller on left sidebar
        let sidebar_motion = gtk::EventControllerMotion::new();
        let s_sm = sender.clone();
        sidebar_motion.connect_enter(move |_, _, _| {
            let _ = s_sm.input_sender().send(ComicsReaderMsg::SidebarHover(true));
        });
        let s_smm = sender.clone();
        sidebar_motion.connect_motion(move |_, _, _| {
            let _ = s_smm.input_sender().send(ComicsReaderMsg::SidebarHover(true));
        });
        widgets.left_sidebar_box.add_controller(sidebar_motion);

        // Hover controller on dedicated left edge strip
        let left_edge_motion = gtk::EventControllerMotion::new();
        let s_lem = sender.clone();
        left_edge_motion.connect_enter(move |_, _, _| {
            let _ = s_lem.input_sender().send(ComicsReaderMsg::LeftEdgeHover(true));
        });
        widgets.left_edge_box.add_controller(left_edge_motion);

        let left_edge_click = gtk::GestureClick::new();
        let s_lec = sender.clone();
        left_edge_click.connect_pressed(move |_, _, _, _| {
            let _ = s_lec.input_sender().send(ComicsReaderMsg::ToggleSidebar);
        });
        widgets.left_edge_box.add_controller(left_edge_click);

        // 4. Viewport click: grab keyboard focus (no chrome toggle or page change on click)
        let click = gtk::GestureClick::new();
        let root_for_focus = root.clone();
        click.connect_pressed(move |_, _, _, _| {
            root_for_focus.grab_focus();
        });
        widgets.viewport_box.add_controller(click);

        // 5. Continuous scroll listener
        let vadj = widgets.viewport_box.vadjustment();
        let s_scroll = sender.clone();
        vadj.connect_value_changed(move |adj| {
            let _ = s_scroll.input_sender().send(ComicsReaderMsg::UserScrolled);
            let max = (adj.upper() - adj.page_size()).max(1.0);
            if max > 0.0 {
                let ratio = (adj.value() / max).clamp(0.0, 1.0);
                let _ = s_scroll.input_sender().send(ComicsReaderMsg::UpdateScrollPage(ratio));
            }
        });

        let scroll_ctrl = gtk::EventControllerScroll::new(
            gtk::EventControllerScrollFlags::VERTICAL | gtk::EventControllerScrollFlags::HORIZONTAL,
        );
        let s_sc = sender.clone();
        scroll_ctrl.connect_scroll(move |_, _, _| {
            let _ = s_sc.input_sender().send(ComicsReaderMsg::UserScrolled);
            gtk::glib::Propagation::Proceed
        });
        widgets.viewport_box.add_controller(scroll_ctrl);

        // 6. Viewport resize notification (for Fit Width)
        let s_hadj = sender.clone();
        widgets.viewport_box.hadjustment().connect_page_size_notify(move |_| {
            let _ = s_hadj.input_sender().send(ComicsReaderMsg::ViewportResized);
        });
        let s_vadj_size = sender.clone();
        widgets.viewport_box.vadjustment().connect_page_size_notify(move |_| {
            let _ = s_vadj_size.input_sender().send(ComicsReaderMsg::ViewportResized);
        });

        // 7. Keyboard shortcuts on root with Capture propagation phase
        let key_controller = gtk::EventControllerKey::new();
        key_controller.set_propagation_phase(gtk::PropagationPhase::Capture);
        let s_key = sender.clone();
        key_controller.connect_key_pressed(move |_, keyval, _, _| {
            match keyval {
                gdk::Key::Escape => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::Close);
                    glib::Propagation::Stop
                }
                gdk::Key::BackSpace | gdk::Key::q | gdk::Key::Q => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::Close);
                    glib::Propagation::Stop
                }
                gdk::Key::Left | gdk::Key::Page_Up | gdk::Key::h | gdk::Key::H => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::KeyLeft);
                    glib::Propagation::Stop
                }
                gdk::Key::Right | gdk::Key::Page_Down | gdk::Key::space | gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::l | gdk::Key::L => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::KeyRight);
                    glib::Propagation::Stop
                }
                gdk::Key::Up | gdk::Key::k | gdk::Key::K => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::ScrollDelta(-1.0));
                    glib::Propagation::Stop
                }
                gdk::Key::Down | gdk::Key::j | gdk::Key::J => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::ScrollDelta(1.0));
                    glib::Propagation::Stop
                }
                gdk::Key::Home => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::SetPage(0));
                    glib::Propagation::Stop
                }
                gdk::Key::End => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::SetPage(usize::MAX));
                    glib::Propagation::Stop
                }
                gdk::Key::b | gdk::Key::B => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::ToggleBookmark);
                    glib::Propagation::Stop
                }
                gdk::Key::f | gdk::Key::F => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::ToggleFitMode);
                    glib::Propagation::Stop
                }
                gdk::Key::t | gdk::Key::T | gdk::Key::s | gdk::Key::S => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::ToggleSidebar);
                    glib::Propagation::Stop
                }
                _ => glib::Propagation::Proceed,
            }
        });
        root.add_controller(key_controller);

        root.set_can_focus(true);
        root.set_focusable(true);
        root.grab_focus();
        root.connect_map(|r| {
            r.grab_focus();
        });
        gtk::glib::idle_add_local_once({
            let root = root.clone();
            move || {
                root.grab_focus();
            }
        });
        widgets.viewport_box.set_can_focus(false);
        widgets.viewport_box.set_focusable(false);

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
            ComicsReaderMsg::SetPage(idx) => {
                self.hide_chrome_on_interaction();
                self.set_page(idx);
                self.save_progress();
                self.trigger_loads(&sender);
                self.update_bookmark_icon_state(widgets);
                widgets.page_indicator_label.set_label(&self.page_indicator_label());
                let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                if is_webtoon {
                    let vadj = widgets.viewport_box.vadjustment();
                    let max = (vadj.upper() - vadj.page_size()).max(0.0);
                    if self.total_pages > 1 && max > 0.0 {
                        let ratio = self.current_page as f64 / (self.total_pages - 1) as f64;
                        vadj.set_value(ratio * max);
                    }
                } else {
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), Some(&sender));
                    widgets.viewport_box.set_child(Some(&child));
                    update_viewport_policies(&widgets.viewport_box, self.fit_mode, false);
                }
            }
            ComicsReaderMsg::UpdateScrollPage(ratio) => {
                let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                if !is_webtoon || self.total_pages <= 1 {
                    return;
                }
                let target_page = ((self.total_pages - 1) as f64 * ratio).round() as usize;
                let clamped = target_page.clamp(0, self.total_pages - 1);
                if self.current_page != clamped {
                    self.current_page = clamped;
                    self.save_progress();
                    self.trigger_loads(&sender);
                    self.update_bookmark_icon_state(widgets);
                    widgets.page_indicator_label.set_label(&self.page_indicator_label());
                }
            }
            ComicsReaderMsg::ScrollDelta(dir) => {
                self.hide_chrome_on_interaction();
                let vadj = widgets.viewport_box.vadjustment();
                let step = 80.0 * dir;
                let target = (vadj.value() + step)
                    .clamp(vadj.lower(), (vadj.upper() - vadj.page_size()).max(0.0));
                vadj.set_value(target);
            }
            ComicsReaderMsg::NextPage => {
                self.hide_chrome_on_interaction();
                let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                if self.at_chapter_end {
                    let _ = sender.input_sender().send(ComicsReaderMsg::NextChapter);
                    return;
                }
                let is_last_page = if self.page_style == PageStyle::Double {
                    self.current_page + 2 >= self.total_pages
                } else {
                    self.current_page + 1 >= self.total_pages
                };

                if is_last_page {
                    if is_webtoon {
                        if self.next_chapter().is_some() {
                            let _ = sender.input_sender().send(ComicsReaderMsg::NextChapter);
                            return;
                        }
                    } else if self.next_chapter().is_some() || self.series_chapters.len() > 1 {
                        self.at_chapter_end = true;
                        let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), Some(&sender));
                        widgets.viewport_box.set_child(Some(&child));
                        widgets.page_indicator_label.set_label(&self.page_indicator_label());
                        return;
                    }
                }

                self.next_page();
                self.save_progress();
                self.trigger_loads(&sender);
                self.update_bookmark_icon_state(widgets);
                widgets.page_indicator_label.set_label(&self.page_indicator_label());
                if is_webtoon {
                    let vadj = widgets.viewport_box.vadjustment();
                    let max = (vadj.upper() - vadj.page_size()).max(0.0);
                    if self.total_pages > 1 && max > 0.0 {
                        let ratio = self.current_page as f64 / (self.total_pages - 1) as f64;
                        vadj.set_value(ratio * max);
                    }
                } else {
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), Some(&sender));
                    widgets.viewport_box.set_child(Some(&child));
                    update_viewport_policies(&widgets.viewport_box, self.fit_mode, false);
                }
            }
            ComicsReaderMsg::PrevPage => {
                self.hide_chrome_on_interaction();
                if self.at_chapter_end {
                    self.at_chapter_end = false;
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), Some(&sender));
                    widgets.viewport_box.set_child(Some(&child));
                    widgets.page_indicator_label.set_label(&self.page_indicator_label());
                    return;
                }
                self.prev_page();
                self.save_progress();
                self.trigger_loads(&sender);
                self.update_bookmark_icon_state(widgets);
                widgets.page_indicator_label.set_label(&self.page_indicator_label());
                let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                if is_webtoon {
                    let vadj = widgets.viewport_box.vadjustment();
                    let max = (vadj.upper() - vadj.page_size()).max(0.0);
                    if self.total_pages > 1 && max > 0.0 {
                        let ratio = self.current_page as f64 / (self.total_pages - 1) as f64;
                        vadj.set_value(ratio * max);
                    }
                } else {
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), Some(&sender));
                    widgets.viewport_box.set_child(Some(&child));
                    update_viewport_policies(&widgets.viewport_box, self.fit_mode, false);
                }
            }
            ComicsReaderMsg::KeyLeft => {
                self.hide_chrome_on_interaction();
                if self.direction == ReadingDirection::Rtl {
                    let _ = sender.input_sender().send(ComicsReaderMsg::NextPage);
                } else {
                    let _ = sender.input_sender().send(ComicsReaderMsg::PrevPage);
                }
            }
            ComicsReaderMsg::KeyRight => {
                self.hide_chrome_on_interaction();
                if self.direction == ReadingDirection::Rtl {
                    let _ = sender.input_sender().send(ComicsReaderMsg::PrevPage);
                } else {
                    let _ = sender.input_sender().send(ComicsReaderMsg::NextPage);
                }
            }
            ComicsReaderMsg::PageLoaded(idx, ref maybe_tex) => {
                self.pending_loads.remove(&idx);
                if let Some(tex) = maybe_tex.clone() {
                    self.textures.insert(idx, tex.clone());
                    let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;

                    if is_webtoon {
                        let mut in_place_applied = false;
                        if let Some(vbox) = get_webtoon_vbox(&widgets.viewport_box) {
                            let vp_w = widgets.viewport_box.width();
                            let win_w = if vp_w > 100 { vp_w } else { 850 };
                            let avail_w = (win_w - 24).max(300);
                            let tw = tex.width();
                            let th = tex.height();
                            let target_h = if tw > 0 {
                                ((avail_w as f64) * (th as f64 / tw as f64)).round() as i32
                            } else {
                                -1
                            };

                            let mut curr = vbox.first_child();
                            let mut i = 0;
                            while let Some(item) = curr {
                                if i == idx {
                                    if let Ok(item_box) = item.downcast::<gtk::Box>() {
                                        while let Some(old) = item_box.first_child() {
                                            item_box.remove(&old);
                                        }
                                        let pic = gtk::Picture::for_paintable(&tex);
                                        pic.set_size_request(avail_w, target_h);
                                        pic.set_can_shrink(false);
                                        pic.set_content_fit(gtk::ContentFit::Contain);
                                        pic.set_halign(gtk::Align::Center);
                                        item_box.append(&pic);
                                        in_place_applied = true;
                                    }
                                    break;
                                }
                                curr = item.next_sibling();
                                i += 1;
                            }
                        }
                        if !in_place_applied {
                            let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), Some(&sender));
                            widgets.viewport_box.set_child(Some(&child));
                            update_viewport_policies(&widgets.viewport_box, self.fit_mode, true);
                        }
                    } else {
                        let in_view = if self.page_style == PageStyle::Double {
                            idx == self.current_page || idx == self.current_page + 1
                        } else {
                            idx == self.current_page
                        };
                        if in_view {
                            let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), Some(&sender));
                            widgets.viewport_box.set_child(Some(&child));
                            update_viewport_policies(&widgets.viewport_box, self.fit_mode, false);
                        }
                    }
                }
            }
            ComicsReaderMsg::ToggleDirection => {
                let next = self.direction.next();
                let _ = sender.input_sender().send(ComicsReaderMsg::SetDirection(next));
            }
            ComicsReaderMsg::SetDirection(dir) => {
                if self.direction != dir {
                    self.direction = dir;
                    if let Some(ref catalog) = self.catalog {
                        let dir_str = match dir {
                            ReadingDirection::Ltr => "ltr",
                            ReadingDirection::Rtl => "rtl",
                            ReadingDirection::Webtoon => "webtoon",
                        };
                        if let Some(bid) = self.book_id {
                            catalog.set_pref(&format!("book.{bid}.comic.direction"), dir_str);
                        }
                        catalog.set_pref("reader.comic.direction", dir_str);
                    }
                    self.trigger_osd(dir.label(), &sender);
                    let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                    update_viewport_policies(&widgets.viewport_box, self.fit_mode, is_webtoon);
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), Some(&sender));
                    widgets.viewport_box.set_child(Some(&child));
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::SetPageStyle(style) => {
                if self.page_style != style {
                    self.page_style = style;
                    if let Some(ref catalog) = self.catalog {
                        let style_str = match style {
                            PageStyle::Single => "single",
                            PageStyle::Double => "double",
                            PageStyle::Fade => "fade",
                            PageStyle::LongStrip => "strip",
                        };
                        if let Some(bid) = self.book_id {
                            catalog.set_pref(&format!("book.{bid}.comic.page_style"), style_str);
                        }
                        catalog.set_pref("reader.comic.page_style", style_str);
                    }
                    self.trigger_osd(style.label(), &sender);
                    self.trigger_loads(&sender);
                    widgets.page_indicator_label.set_label(&self.page_indicator_label());
                    let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                    update_viewport_policies(&widgets.viewport_box, self.fit_mode, is_webtoon);
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), Some(&sender));
                    widgets.viewport_box.set_child(Some(&child));
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::ToggleFitMode => {
                let next = self.fit_mode.next();
                let _ = sender.input_sender().send(ComicsReaderMsg::SetFitMode(next));
            }
            ComicsReaderMsg::SetFitMode(fit) => {
                if self.fit_mode != fit {
                    self.fit_mode = fit;
                    if let Some(ref catalog) = self.catalog {
                        let fit_str = match fit {
                            FitMode::Width => "width",
                            FitMode::Height => "height",
                            FitMode::Screen => "screen",
                            FitMode::Original => "original",
                        };
                        if let Some(bid) = self.book_id {
                            catalog.set_pref(&format!("book.{bid}.comic.fit_mode"), fit_str);
                        }
                        catalog.set_pref("reader.comic.fit_mode", fit_str);
                    }
                    self.trigger_osd(fit.label(), &sender);
                    let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                    update_viewport_policies(&widgets.viewport_box, self.fit_mode, is_webtoon);
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), Some(&sender));
                    widgets.viewport_box.set_child(Some(&child));
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::SetSpreadGap(gap) => {
                let clamped = gap.clamp(0, 48);
                if self.two_page_gap != clamped {
                    self.two_page_gap = clamped;
                    if let Some(ref catalog) = self.catalog {
                        catalog.set_pref_i64("reader.comic.two_page_gap", clamped as i64);
                    }
                    if self.page_style == PageStyle::Double {
                        let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), Some(&sender));
                        widgets.viewport_box.set_child(Some(&child));
                    }
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::ViewportResized => {
                let curr_w = widgets.viewport_box.width();
                if (curr_w - self.last_viewport_width).abs() > 16 {
                    self.last_viewport_width = curr_w;
                    if self.fit_mode == FitMode::Width {
                        let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                        let child = rebuild_viewport_widget(self, curr_w, Some(&sender));
                        widgets.viewport_box.set_child(Some(&child));
                        update_viewport_policies(&widgets.viewport_box, self.fit_mode, is_webtoon);
                    }
                }
            }
            ComicsReaderMsg::NextChapter => {
                if let Some(next_ch) = self.next_chapter().cloned() {
                    let _ = sender.input_sender().send(ComicsReaderMsg::SwitchChapter(next_ch.id));
                }
            }
            ComicsReaderMsg::PrevChapter => {
                if let Some(prev_ch) = self.prev_chapter().cloned() {
                    let _ = sender.input_sender().send(ComicsReaderMsg::SwitchChapter(prev_ch.id));
                }
            }
            ComicsReaderMsg::SwitchChapter(new_bid) => {
                if let (Some(_), Some(curr_bid)) = (&self.catalog, self.book_id) {
                    if curr_bid == new_bid {
                        return;
                    }
                    self.save_progress();
                }
                if let (Some(ref catalog), Some(book_id)) = (&self.catalog, Some(new_bid)) {
                    if let Ok(Some(new_book)) = catalog.get_book(book_id) {
                        if let Ok(new_prov) = crate::pages::comics_reader::providers::LocalProvider::new(new_book.file_path.clone()) {
                            self.book_id = Some(new_bid);
                            self.title = new_book.title.clone();
                            self.cover_path = new_book.cover_path.clone();
                            self.provider = std::sync::Arc::new(new_prov);
                            self.total_pages = self.provider.page_count();
                            self.textures.clear();
                            self.pending_loads.clear();
                            self.at_chapter_end = false;

                            let saved_page = match catalog.get_reading_progress(new_bid) {
                                Ok(Some((page, _))) if page < self.total_pages => page,
                                _ => 0,
                            };
                            self.current_page = saved_page;

                            self.current_chapter_idx = self.series_chapters.iter().position(|b| b.id == new_bid);

                            self.reload_bookmarks();
                            self.update_bookmark_icon_state(widgets);

                            while let Some(child) = widgets.cover_host.first_child() {
                                widgets.cover_host.remove(&child);
                            }
                            let cover_w = crate::widgets::book_row::cover_widget(self.cover_path.as_deref(), 48, 70);
                            widgets.cover_host.append(&cover_w);

                            if let Some(ref lb) = self.chapters_list_box {
                                populate_chapters_list(lb, &self.series_chapters, self.book_id, &sender);
                            }

                            self.trigger_loads(&sender);
                            let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), Some(&sender));
                            widgets.viewport_box.set_child(Some(&child));
                            let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                            update_viewport_policies(&widgets.viewport_box, self.fit_mode, is_webtoon);
                            if is_webtoon {
                                widgets.viewport_box.vadjustment().set_value(0.0);
                            }
                            widgets.page_indicator_label.set_label(&self.page_indicator_label());
                            self.trigger_osd(&self.title.clone(), &sender);
                        }
                    }
                }
            }
            ComicsReaderMsg::ToggleSidebar => {
                self.show_sidebar = !self.show_sidebar;
                self.sidebar_pinned = self.show_sidebar;
                widgets.sidebar_revealer.set_reveal_child(self.show_sidebar);
                widgets.dim_backdrop.set_visible(self.show_sidebar);
                if self.show_sidebar {
                    self.mouse_in_sidebar = true;
                    self.show_back_button = false;
                    widgets.top_back_revealer.set_reveal_child(false);
                    self.sync_settings_ui(widgets);
                    let tab_name = match self.sidebar_tab {
                        ComicSidebarTab::Chapters => "chapters",
                        ComicSidebarTab::Settings => "settings",
                        ComicSidebarTab::Bookmarks => "bookmarks",
                    };
                    widgets.left_stack.set_visible_child_name(tab_name);
                    if self.sidebar_tab == ComicSidebarTab::Bookmarks {
                        populate_bookmarks_list(&widgets.bookmarks_list_box, &self.bookmarks, &sender);
                    } else if self.sidebar_tab == ComicSidebarTab::Chapters {
                        if let Some(ref lb) = self.chapters_list_box {
                            populate_chapters_list(lb, &self.series_chapters, self.book_id, &sender);
                        }
                    }
                }
            }
            ComicsReaderMsg::CloseSidebar => {
                self.show_sidebar = false;
                self.sidebar_pinned = false;
                self.mouse_in_sidebar = false;
                self.mouse_in_left_edge = false;
                widgets.sidebar_revealer.set_reveal_child(false);
                widgets.dim_backdrop.set_visible(false);
            }
            ComicsReaderMsg::SetSidebarTab(tab) => {
                self.sidebar_tab = tab;
                let tab_name = match tab {
                    ComicSidebarTab::Chapters => "chapters",
                    ComicSidebarTab::Settings => "settings",
                    ComicSidebarTab::Bookmarks => "bookmarks",
                };
                widgets.left_stack.set_visible_child_name(tab_name);
                if tab == ComicSidebarTab::Bookmarks {
                    populate_bookmarks_list(&widgets.bookmarks_list_box, &self.bookmarks, &sender);
                } else if tab == ComicSidebarTab::Chapters {
                    if let Some(ref lb) = self.chapters_list_box {
                        populate_chapters_list(lb, &self.series_chapters, self.book_id, &sender);
                    }
                }
            }
            ComicsReaderMsg::ToggleBookmark => {
                if let (Some(ref catalog), Some(bid)) = (&self.catalog, self.book_id) {
                    let page = self.current_page as i64;
                    let existing = self
                        .bookmarks
                        .iter()
                        .find(|b| b.chapter_index == page)
                        .map(|b| b.id);
                    if let Some(id) = existing {
                        let _ = catalog.delete_reading_bookmark(id);
                        self.trigger_osd("Bookmark Removed", &sender);
                    } else {
                        let fract = self.progress_fraction();
                        let title = format!("Page {}", self.current_page + 1);
                        let _ = catalog.insert_reading_bookmark(bid, page, fract, &title);
                        self.trigger_osd("Bookmark Added", &sender);
                    }
                    self.reload_bookmarks();
                    self.update_bookmark_icon_state(widgets);
                    if self.sidebar_tab == ComicSidebarTab::Bookmarks {
                        populate_bookmarks_list(&widgets.bookmarks_list_box, &self.bookmarks, &sender);
                    }
                }
            }
            ComicsReaderMsg::DeleteBookmark(id) => {
                if let Some(ref catalog) = self.catalog {
                    let _ = catalog.delete_reading_bookmark(id);
                    self.reload_bookmarks();
                    self.update_bookmark_icon_state(widgets);
                    if self.sidebar_tab == ComicSidebarTab::Bookmarks {
                        populate_bookmarks_list(&widgets.bookmarks_list_box, &self.bookmarks, &sender);
                    }
                }
            }
            ComicsReaderMsg::TopEdgeHover(inside) => {
                self.mouse_in_top_edge = inside;
                if inside {
                    if !self.show_sidebar {
                        self.show_back_button = true;
                        widgets.top_back_revealer.set_reveal_child(true);
                    }
                    self.back_hide_seq = self.back_hide_seq.wrapping_add(1);
                } else if self.show_back_button {
                    self.schedule_back_hide(&sender);
                }
            }
            ComicsReaderMsg::BottomEdgeHover(inside) => {
                self.mouse_in_bottom_edge = inside;
                if inside {
                    self.show_bottom_pill = true;
                    widgets.bottom_pill_revealer.set_reveal_child(true);
                    self.bottom_hide_seq = self.bottom_hide_seq.wrapping_add(1);
                } else if self.show_bottom_pill {
                    self.schedule_bottom_hide(&sender);
                }
            }
            ComicsReaderMsg::LeftEdgeHover(inside) => {
                self.mouse_in_left_edge = inside;
                if inside {
                    let was_hidden = !self.show_sidebar;
                    self.show_sidebar = true;
                    self.mouse_in_sidebar = true;
                    self.sidebar_close_seq = self.sidebar_close_seq.wrapping_add(1);
                    self.show_back_button = false;
                    widgets.top_back_revealer.set_reveal_child(false);
                    widgets.sidebar_revealer.set_reveal_child(true);
                    widgets.dim_backdrop.set_visible(true);
                    if was_hidden {
                        self.sync_settings_ui(widgets);
                        let tab_name = match self.sidebar_tab {
                            ComicSidebarTab::Chapters => "chapters",
                            ComicSidebarTab::Settings => "settings",
                            ComicSidebarTab::Bookmarks => "bookmarks",
                        };
                        widgets.left_stack.set_visible_child_name(tab_name);
                        if self.sidebar_tab == ComicSidebarTab::Bookmarks {
                            populate_bookmarks_list(&widgets.bookmarks_list_box, &self.bookmarks, &sender);
                        } else if self.sidebar_tab == ComicSidebarTab::Chapters {
                            if let Some(ref lb) = self.chapters_list_box {
                                populate_chapters_list(lb, &self.series_chapters, self.book_id, &sender);
                            }
                        }
                    }
                } else if !self.mouse_in_sidebar && !self.sidebar_pinned {
                    self.schedule_sidebar_close(&sender);
                }
            }
            ComicsReaderMsg::SidebarHover(inside) => {
                self.mouse_in_sidebar = inside;
                if inside {
                    self.sidebar_close_seq = self.sidebar_close_seq.wrapping_add(1);
                } else if !self.mouse_in_left_edge && !self.sidebar_pinned {
                    self.schedule_sidebar_close(&sender);
                }
            }
            ComicsReaderMsg::BackHideTimerTick(seq) => {
                if seq == self.back_hide_seq && !self.mouse_in_top_edge {
                    self.show_back_button = false;
                    widgets.top_back_revealer.set_reveal_child(false);
                }
            }
            ComicsReaderMsg::BottomHideTimerTick(seq) => {
                if seq == self.bottom_hide_seq && !self.mouse_in_bottom_edge {
                    self.show_bottom_pill = false;
                    widgets.bottom_pill_revealer.set_reveal_child(false);
                }
            }
            ComicsReaderMsg::SidebarCloseTimerTick(seq) => {
                if !self.sidebar_pinned
                    && seq == self.sidebar_close_seq
                    && !self.mouse_in_sidebar
                    && !self.mouse_in_left_edge
                {
                    self.show_sidebar = false;
                    widgets.sidebar_revealer.set_reveal_child(false);
                    widgets.dim_backdrop.set_visible(false);
                }
            }
            ComicsReaderMsg::UserScrolled => {
                self.hide_chrome_on_interaction();
                if !self.show_back_button {
                    widgets.top_back_revealer.set_reveal_child(false);
                }
                if !self.show_bottom_pill {
                    widgets.bottom_pill_revealer.set_reveal_child(false);
                }
            }
            ComicsReaderMsg::HideOsd(seq) => {
                if seq == self.zoom_osd_seq {
                    self.show_zoom_osd = false;
                }
            }
            ComicsReaderMsg::ToggleSettings => {
                let _ = sender.input_sender().send(ComicsReaderMsg::ToggleSidebar);
            }
            ComicsReaderMsg::CloseSettings => {
                self.show_sidebar = false;
                widgets.sidebar_revealer.set_reveal_child(false);
                widgets.dim_backdrop.set_visible(false);
            }
            ComicsReaderMsg::Close => {
                if self.show_sidebar {
                    self.show_sidebar = false;
                    self.sidebar_pinned = false;
                    widgets.sidebar_revealer.set_reveal_child(false);
                    widgets.dim_backdrop.set_visible(false);
                } else {
                    let _ = sender.output(ComicsReaderOut::Close);
                }
            }
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
            catalog: None,
            book_id: None,
            cover_path: None,
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
            catalog: None,
            book_id: None,
            cover_path: None,
        });

        model.page_style = PageStyle::Double;
        model.current_page = 0;

        model.next_page();
        assert_eq!(model.current_page, 2);

        model.next_page();
        assert_eq!(model.current_page, 4);

        model.next_page();
        assert_eq!(model.current_page, 6);

        model.next_page();
        assert_eq!(model.current_page, 8);

        // Next page at boundary (page 8 of 10) must remain at 8 so even/odd pairs remain aligned
        model.next_page();
        assert_eq!(model.current_page, 8);

        model.prev_page();
        assert_eq!(model.current_page, 6);
    }

    #[test]
    fn test_pending_loads_deduplication() {
        let dummy = Arc::new(DummyProvider { count: 10 });
        let mut model = ComicsReaderModel::new(types::ComicsReaderInit {
            title: "Test Preload".to_string(),
            provider: dummy,
            catalog: None,
            book_id: None,
            cover_path: None,
        });
        model.page_style = PageStyle::Single;
        model.current_page = 0;

        let missing_first = model.get_missing_pages();
        assert_eq!(missing_first, vec![0, 1, 2]);

        // Simulate marking page 1 as pending load
        model.pending_loads.insert(1);

        let missing_second = model.get_missing_pages();
        assert_eq!(missing_second, vec![0, 2]);
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
            catalog: None,
            book_id: None,
            cover_path: None,
        });

        model.direction = ReadingDirection::Webtoon;
        model.current_page = 5;

        let missing = model.get_missing_pages();
        assert_eq!(missing, (1..=11).collect::<Vec<_>>());
    }

    #[test]
    fn test_fit_mode_cycle() {
        let fit = FitMode::Width;
        assert_eq!(fit.next(), FitMode::Height);
        assert_eq!(FitMode::Height.next(), FitMode::Screen);
        assert_eq!(FitMode::Screen.next(), FitMode::Original);
        assert_eq!(FitMode::Original.next(), FitMode::Width);
    }

    #[test]
    fn test_sidebar_default_is_settings() {
        let dummy = Arc::new(DummyProvider { count: 10 });
        let model = ComicsReaderModel::new(types::ComicsReaderInit {
            title: "Test Manga".to_string(),
            provider: dummy,
            catalog: None,
            book_id: None,
            cover_path: None,
        });
        assert_eq!(model.sidebar_tab, ComicSidebarTab::Settings);
    }

    #[test]
    fn test_page_indicator_label() {
        let dummy = Arc::new(DummyProvider { count: 20 });
        let mut model = ComicsReaderModel::new(types::ComicsReaderInit {
            title: "Test Indicator".to_string(),
            provider: dummy,
            catalog: None,
            book_id: None,
            cover_path: None,
        });

        model.page_style = PageStyle::Single;
        model.current_page = 0;
        assert_eq!(model.page_indicator_label(), "Page 1 of 20");

        model.page_style = PageStyle::Double;
        assert_eq!(model.page_indicator_label(), "Pages 1-2 of 20");

        model.current_page = 19;
        assert_eq!(model.page_indicator_label(), "Page 20 of 20");
    }

    #[test]
    fn test_comic_spread_gap() {
        let dummy = Arc::new(DummyProvider { count: 10 });
        let mut model = ComicsReaderModel::new(types::ComicsReaderInit {
            title: "Test Gap".to_string(),
            provider: dummy,
            catalog: None,
            book_id: None,
            cover_path: None,
        });

        assert_eq!(model.two_page_gap, 0);
        model.two_page_gap = 8;
        assert_eq!(model.two_page_gap, 8);
    }

    #[test]
    fn test_touchpad_scroll_guarded_in_paged_modes() {
        let dummy = Arc::new(DummyProvider { count: 20 });
        let mut model = ComicsReaderModel::new(types::ComicsReaderInit {
            title: "Test Scroll".to_string(),
            provider: dummy,
            catalog: None,
            book_id: None,
            cover_path: None,
        });

        model.page_style = PageStyle::Single;
        model.direction = ReadingDirection::Ltr;
        model.current_page = 0;

        let is_webtoon = model.direction == ReadingDirection::Webtoon || model.page_style == PageStyle::LongStrip;
        assert!(!is_webtoon, "Single page mode must not be treated as webtoon");

        model.page_style = PageStyle::Double;
        let is_webtoon_double = model.direction == ReadingDirection::Webtoon || model.page_style == PageStyle::LongStrip;
        assert!(!is_webtoon_double, "Double page mode must not be treated as webtoon");

        // Webtoon mode
        model.direction = ReadingDirection::Webtoon;
        let is_webtoon_active = model.direction == ReadingDirection::Webtoon || model.page_style == PageStyle::LongStrip;
        assert!(is_webtoon_active);
        let ratio = 0.5;
        let target = ((model.total_pages - 1) as f64 * ratio).round() as usize;
        assert_eq!(target, 10);
    }

    #[test]
    fn test_live_page_indicator_and_progress_updates() {
        let dummy = Arc::new(DummyProvider { count: 10 });
        let mut model = ComicsReaderModel::new(types::ComicsReaderInit {
            title: "Test Manga".to_string(),
            provider: dummy,
            catalog: None,
            book_id: None,
            cover_path: None,
        });

        assert_eq!(model.current_page, 0);
        assert_eq!(model.page_indicator_label(), "Page 1 of 10");
        assert!((model.progress_fraction() - 0.1).abs() < 1e-4);

        model.next_page();
        assert_eq!(model.current_page, 1);
        assert_eq!(model.page_indicator_label(), "Page 2 of 10");
        assert!((model.progress_fraction() - 0.2).abs() < 1e-4);

        model.set_page(9);
        assert_eq!(model.current_page, 9);
        assert_eq!(model.page_indicator_label(), "Page 10 of 10");
        assert!((model.progress_fraction() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_series_chapters_navigation_and_end_card() {
        let dummy = Arc::new(DummyProvider { count: 5 });
        let mut model = ComicsReaderModel::new(types::ComicsReaderInit {
            title: "Berserk - Chapter 1".to_string(),
            provider: dummy,
            catalog: None,
            book_id: Some(1),
            cover_path: None,
        });

        let ch1 = crate::models::Book {
            id: 1,
            uuid: "uuid-1".to_string(),
            title: "Chapter 1".to_string(),
            authors: "Kentaro Miura".to_string(),
            series: Some("Berserk".to_string()),
            description: String::new(),
            format: crate::models::BookFormat::Cbz,
            file_name: "ch1.cbz".to_string(),
            file_hash: "h1".to_string(),
            cover_name: None,
            added_at: String::new(),
            progress: 100,
            rating: 0,
            publisher: String::new(),
            published: String::new(),
            series_index: 1.0,
            tags: Vec::new(),
            cover_path: None,
            file_path: std::path::PathBuf::from("/comics/ch1.cbz"),
        };
        let ch2 = crate::models::Book {
            id: 2,
            uuid: "uuid-2".to_string(),
            title: "Chapter 2".to_string(),
            authors: "Kentaro Miura".to_string(),
            series: Some("Berserk".to_string()),
            description: String::new(),
            format: crate::models::BookFormat::Cbz,
            file_name: "ch2.cbz".to_string(),
            file_hash: "h2".to_string(),
            cover_name: None,
            added_at: String::new(),
            progress: 0,
            rating: 0,
            publisher: String::new(),
            published: String::new(),
            series_index: 2.0,
            tags: Vec::new(),
            cover_path: None,
            file_path: std::path::PathBuf::from("/comics/ch2.cbz"),
        };

        model.series_chapters = vec![ch1, ch2];
        model.current_chapter_idx = Some(0);
        model.page_style = PageStyle::Single;
        model.direction = ReadingDirection::Ltr;
        model.current_page = 4; // last page of 5

        assert_eq!(model.next_chapter().map(|b| b.id), Some(2));
        assert_eq!(model.prev_chapter().map(|b| b.id), None);

        model.at_chapter_end = true;
        assert!(model.page_indicator_label().contains("Completed · Next: Chapter 2"));
    }
}
