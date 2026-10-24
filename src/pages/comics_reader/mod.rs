//! Comic and manga reader component for Kalam.
//!
//! An immersive image-pager for CBZ/CBR archives and remote manga,
//! carrying the EPUB reader's pure-page chrome (decisions 35–43, 50):
//!
//! - The page alone at rest. The Back pill with its Bubble slide-out
//!   drops in when the pointer rests in the top-left zone; the
//!   bookmark ribbon hangs at the top-right, always there; the plain
//!   location text (`12 / 180`, series-wide) sits bottom-right. No
//!   bottom pill, no dim layer.
//! - Two floating panels, 340 wide and 16 from the edges: Chapters and
//!   Pages on the left, Reading and Shortcuts on the right, summoned
//!   from the 15px edge strips or by `T` / `S` and put away by a click
//!   on the page or Esc (which exits from a bare page).
//! - A top-centre page-jump field (`G`) counting by series page or by
//!   chapter, with the accent out-of-range state.
//! - Reading progress checkpointing, bookmark persistence, and memory
//!   release on close.

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
use std::rc::Rc;
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
    pub left_tab: ComicLeftTab,
    pub right_tab: ComicRightTab,
    pub left_panel_open: bool,
    pub right_panel_open: bool,
    pub show_back_button: bool,
    /// Whether the pointer currently rests in the Back unit's
    /// top-left zone, and the generation counter its reveal/hide
    /// timers are keyed on (decision 15's timings, inherited).
    pub back_hover: bool,
    pub back_gen: u64,
    pub back_unit: Option<crate::widgets::slide_out_pill::SlideOutPill>,
    pub left_tabs: Option<crate::widgets::segmented::SegmentedControl>,
    pub right_tabs: Option<crate::widgets::segmented::SegmentedControl>,
    pub jump_open: bool,
    pub jump_mode: JumpMode,
    pub jump_text: String,
    pub jump_mode_control: Option<crate::widgets::segmented::SegmentedControl>,
    pub show_zoom_osd: bool,
    pub zoom_osd_seq: u64,
    pub osd_text: String,
    pub bookmarks: Vec<ReadingBookmark>,
    pub textures: HashMap<usize, gdk::Texture>,
    pub pending_loads: HashSet<usize>,
    pub session_id: Option<i64>,
    pub session_start: std::time::Instant,
    pub last_viewport_width: i32,
    pub authors: String,
    #[allow(dead_code)]
    pub series_name: Option<String>,
    pub series_chapters: Vec<crate::models::Book>,
    pub series_chapter_meta: Vec<ChapterMeta>,
    pub current_chapter_idx: Option<usize>,
    pub at_chapter_end: bool,
    /// Per-chapter page counts in series order (`None` where the
    /// archive could not be listed), with the running offsets and the
    /// series total derived from them. These are what make the
    /// location text and the page-jump field series-wide; where any
    /// count is missing the reader falls back to chapter-relative
    /// numbers rather than inventing a total.
    pub chapter_page_counts: Vec<Option<usize>>,
    pub chapter_page_offsets: Vec<Option<usize>>,
    pub series_total_pages: Option<usize>,
    /// Pages-tab thumbnails, keyed by page within the current chapter.
    /// Decoded small and kept apart from the reading window's
    /// full-size textures, which are deliberately bounded.
    pub thumb_textures: HashMap<usize, gdk::Texture>,
    pub thumb_pending: HashSet<usize>,
    pub thumb_failed: HashSet<usize>,
    pub page_thumb_buttons: Vec<gtk::Button>,
    pub page_thumb_pictures: Vec<gtk::Picture>,
    pub page_thumb_status: Vec<gtk::Label>,
    /// Bumped whenever the provider changes, so page and thumbnail
    /// decodes finishing after a chapter switch are recognised as
    /// belonging to the chapter that was left and dropped.
    pub load_gen: u64,
}

#[allow(dead_code)]
enum DecodedPage {
    Rgba { width: i32, height: i32, bytes: Vec<u8> },
    RawBytes(Vec<u8>),
}

/// Running page offsets per chapter plus the series total, from the
/// per-chapter counts. An unknown count poisons every offset after it
/// (and the total): a series page number the reader cannot actually
/// place is worse than falling back to chapter-relative numbers.
fn page_offsets_and_total(counts: &[Option<usize>]) -> (Vec<Option<usize>>, Option<usize>) {
    let mut offsets = Vec::with_capacity(counts.len());
    let mut running: Option<usize> = Some(0);
    for count in counts {
        offsets.push(running);
        running = match (running, count) {
            (Some(done), Some(n)) => Some(done + n),
            _ => None,
        };
    }
    (offsets, running.filter(|total| *total > 0))
}

/// `3` for a whole chapter number, `10.5` for a fractional one — the
/// shortest rendering, never `3.0`.
fn format_chapter_number(number: f32) -> String {
    if (number.fract()).abs() < f32::EPSILON {
        format!("{}", number as i64)
    } else {
        format!("{number}")
    }
}

impl ComicsReaderModel {
    pub fn new(init: types::ComicsReaderInit) -> Self {
        let total_pages = init.provider.page_count();

        let (series_name, series_chapters, series_chapter_meta, authors) =
            if let (Some(ref catalog), Some(book_id)) = (&init.catalog, init.book_id) {
                if let Ok(Some((series, _ch))) = catalog.get_comic_series_for_book(book_id) {
                    let chs = catalog.chapters_for_series(series.id).unwrap_or_default();
                    let authors = if !series.author.trim().is_empty() {
                        series.author.clone()
                    } else {
                        chs.first().map(|c| c.book.authors.clone()).unwrap_or_default()
                    };
                    let (mut books, mut meta) = (Vec::new(), Vec::new());
                    for c in chs {
                        meta.push(ChapterMeta {
                            number: Some(c.chapter_number),
                            volume: c.volume_number,
                            title: c.chapter_title.clone(),
                        });
                        books.push(c.book);
                    }
                    (Some(series.title), books, meta, authors)
                } else if let Ok(Some(book)) = catalog.get_book(book_id) {
                    let authors = book.authors.clone();
                    let s_name = book
                        .series
                        .as_deref()
                        .map(|s| crate::comics::sanitize_comic_series(s).0)
                        .filter(|s| !s.is_empty())
                        .or_else(|| crate::comics::parse_comic_title(&book.title).series);
                    if let Some(ref s) = s_name {
                        let chapters = catalog.books_in_series(s).unwrap_or_default();
                        let meta = chapters
                            .iter()
                            .map(|b| {
                                let info = crate::comics::parse_comic_title(&b.title);
                                ChapterMeta {
                                    number: (b.series_index > 0.0)
                                        .then_some(b.series_index)
                                        .or(info.number),
                                    volume: info.volume.map(|v| v as f32),
                                    title: info.title.unwrap_or_default(),
                                }
                            })
                            .collect();
                        (Some(s.clone()), chapters, meta, authors)
                    } else {
                        (None, Vec::new(), Vec::new(), authors)
                    }
                } else {
                    (None, Vec::new(), Vec::new(), String::new())
                }
            } else {
                (None, Vec::new(), Vec::new(), String::new())
            };

        let current_chapter_idx = init.book_id.and_then(|bid| {
            series_chapters.iter().position(|b| b.id == bid)
        });

        // The series-wide page accounting (decision 35): listing an
        // archive's entries reads only its central directory, so the
        // counts are cheap enough to take up front. The current
        // chapter's count comes from the provider that is already
        // open; a chapter whose file is gone simply has no count, and
        // every total downstream of it stays unknown.
        let chapter_page_counts: Vec<Option<usize>> = series_chapters
            .iter()
            .map(|b| {
                if Some(b.id) == init.book_id {
                    (total_pages > 0).then_some(total_pages)
                } else {
                    crate::comics::list_comic_pages(&b.file_path)
                        .ok()
                        .map(|pages| pages.len())
                        .filter(|n| *n > 0)
                }
            })
            .collect();
        let (chapter_page_offsets, series_total_pages) =
            page_offsets_and_total(&chapter_page_counts);

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
                // Fade is not one of the designed page styles
                // (decision 39 skipped it); a stored "fade" opens as
                // Single rather than as a mode with no tile.
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
            left_tab: ComicLeftTab::Chapters,
            right_tab: ComicRightTab::Reading,
            left_panel_open: false,
            right_panel_open: false,
            // The page opens bare (decision 35): no chrome stands
            // until the pointer asks for it.
            show_back_button: false,
            back_hover: false,
            back_gen: 0,
            back_unit: None,
            left_tabs: None,
            right_tabs: None,
            jump_open: false,
            jump_mode: JumpMode::Page,
            jump_text: String::new(),
            jump_mode_control: None,
            show_zoom_osd: false,
            zoom_osd_seq: 0,
            osd_text: String::new(),
            bookmarks,
            textures: HashMap::new(),
            pending_loads: HashSet::new(),
            session_id,
            session_start: std::time::Instant::now(),
            last_viewport_width: 0,
            authors,
            series_name,
            series_chapters,
            series_chapter_meta,
            current_chapter_idx,
            at_chapter_end: false,
            chapter_page_counts,
            chapter_page_offsets,
            series_total_pages,
            thumb_textures: HashMap::new(),
            thumb_pending: HashSet::new(),
            thumb_failed: HashSet::new(),
            page_thumb_buttons: Vec::new(),
            page_thumb_pictures: Vec::new(),
            page_thumb_status: Vec::new(),
            load_gen: 0,
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
        let gen = self.load_gen;

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
                    let _ = s.send(types::ComicsReaderMsg::PageLoaded(gen, idx, tex_opt));
                },
            );
        }
    }

    /// Decode small copies of every page in the current chapter for
    /// the Pages grid. Separate from [`Self::trigger_loads`]: the
    /// reading window keeps only a few full-size textures alive,
    /// while the grid wants the whole chapter at thumbnail size.
    pub fn trigger_thumbnail_loads(&mut self, sender: &relm4::ComponentSender<Self>) {
        let provider = self.provider.clone();
        let gen = self.load_gen;
        for idx in 0..self.total_pages {
            if self.thumb_textures.contains_key(&idx)
                || self.thumb_pending.contains(&idx)
                || self.thumb_failed.contains(&idx)
                || self.textures.contains_key(&idx)
            {
                continue;
            }
            self.thumb_pending.insert(idx);
            let s = sender.input_sender().clone();
            let prov = provider.clone();
            crate::tasks::spawn_internal(
                "Loading comic thumbnail",
                move |_| -> Option<DecodedPage> {
                    let b = prov.fetch_page(idx).ok()?;
                    let img = image::load_from_memory(&b).ok()?;
                    let rgba = img.thumbnail(240, 360).to_rgba8();
                    let width = rgba.width() as i32;
                    let height = rgba.height() as i32;
                    Some(DecodedPage::Rgba {
                        width,
                        height,
                        bytes: rgba.into_raw(),
                    })
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
                    let _ = s.send(types::ComicsReaderMsg::ThumbnailLoaded(gen, idx, tex_opt));
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

    /// The current page's 1-based number across the whole series, and
    /// the series total — both only when every chapter up to here
    /// could be counted.
    pub fn series_page_position(&self) -> Option<(usize, usize)> {
        let idx = self.current_chapter_idx?;
        let offset = self.chapter_page_offsets.get(idx).copied().flatten()?;
        let total = self.series_total_pages?;
        Some((offset + self.current_page + 1, total))
    }

    /// The bottom-right location text (decision 35): plain, series-
    /// wide where the counts allow (`12 / 180`, a spread `12–13 /
    /// 180`), chapter-relative where they do not (`3 / 74`).
    pub fn page_indicator_label(&self) -> String {
        if self.total_pages == 0 {
            return "0 / 0".to_string();
        }
        let spread = self.page_style == PageStyle::Double
            && self.current_page + 1 < self.total_pages;
        if let Some((first, total)) = self.series_page_position() {
            if spread {
                format!("{}–{} / {}", first, first + 1, total)
            } else {
                format!("{} / {}", first, total)
            }
        } else if spread {
            format!(
                "{}–{} / {}",
                self.current_page + 1,
                self.current_page + 2,
                self.total_pages
            )
        } else {
            format!("{} / {}", self.current_page + 1, self.total_pages)
        }
    }

    /// The largest number the jump field accepts in its current mode.
    pub fn jump_max(&self) -> usize {
        match self.jump_mode {
            JumpMode::Page => self.series_total_pages.unwrap_or(self.total_pages),
            JumpMode::Chapter => self.series_chapters.len().max(1),
        }
    }

    /// The number the jump field opens on for its current mode: where
    /// the reader already is, in that mode's units.
    pub fn jump_current(&self) -> usize {
        match self.jump_mode {
            JumpMode::Page => self
                .series_page_position()
                .map(|(page, _)| page)
                .unwrap_or_else(|| self.current_page + 1),
            JumpMode::Chapter => self.current_chapter_idx.map(|i| i + 1).unwrap_or(1),
        }
    }

    /// Whether the jump field's text names a place that exists. Empty
    /// is not yet an error — the field only turns accent once a
    /// number has been offered and refused (decision 50).
    pub fn jump_out_of_range(&self) -> bool {
        let text = self.jump_text.trim();
        if text.is_empty() {
            return false;
        }
        match text.parse::<usize>() {
            Ok(n) => n == 0 || n > self.jump_max(),
            Err(_) => true,
        }
    }

    pub fn jump_hint(&self) -> String {
        let max = self.jump_max();
        if self.jump_out_of_range() {
            match self.jump_mode {
                JumpMode::Page => format!("Pages 1–{max}"),
                JumpMode::Chapter => format!("Chapters 1–{max}"),
            }
        } else {
            format!("of {max}")
        }
    }

    /// Resolve a 0-based series-wide page to (chapter index, page
    /// within the chapter). `None` when the counts cannot place it.
    pub fn series_page_target(&self, global_page: usize) -> Option<(usize, usize)> {
        for (idx, (offset, count)) in self
            .chapter_page_offsets
            .iter()
            .zip(self.chapter_page_counts.iter())
            .enumerate()
        {
            let (offset, count) = ((*offset)?, (*count)?);
            if global_page >= offset && global_page < offset + count {
                return Some((idx, global_page - offset));
            }
        }
        None
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

    /// A page turn or a scroll puts the Back unit away, unless the
    /// pointer is resting in its zone — the page goes back to being
    /// only a page. The panels are not chrome in this sense: they
    /// stand until they are put away (decision 20, inherited).
    pub fn hide_chrome_on_interaction(&mut self) {
        if self.show_back_button && !self.back_hover {
            self.show_back_button = false;
            self.back_gen = self.back_gen.wrapping_add(1);
            if let Some(unit) = &self.back_unit {
                unit.hide();
            }
        }
    }

    /// The bookmark ribbon (decision 16): outline when unmarked,
    /// accent-filled when a mark sits on this page.
    pub fn update_bookmark_icon_state(&self, widgets: &ComicsReaderModelWidgets) {
        let is_bookmarked = self
            .bookmarks
            .iter()
            .any(|b| b.chapter_index as usize == self.current_page);
        widgets.ribbon_image.set_icon_name(Some(if is_bookmarked {
            "kalam-ribbon-filled-symbolic"
        } else {
            "kalam-ribbon-outline-symbolic"
        }));
        if is_bookmarked {
            widgets.ribbon_button.add_css_class("set");
        } else {
            widgets.ribbon_button.remove_css_class("set");
        }
        widgets.ribbon_button.set_tooltip_text(Some(if is_bookmarked {
            "Remove the bookmark here (B)"
        } else {
            "Bookmark this page (B)"
        }));
    }

    pub fn sync_settings_ui(&self, widgets: &ComicsReaderModelWidgets) {
        toggle_active(&widgets.style_single_btn, self.page_style == PageStyle::Single);
        toggle_active(&widgets.style_double_btn, self.page_style == PageStyle::Double);
        toggle_active(&widgets.style_strip_btn, self.page_style == PageStyle::LongStrip);

        toggle_active(&widgets.dir_ltr_btn, self.direction == ReadingDirection::Ltr);
        toggle_active(&widgets.dir_rtl_btn, self.direction == ReadingDirection::Rtl);

        // The spread gap only exists in Spread (decision 39 puts a
        // gap row under the page style it belongs to): hide the row
        // and the section heading directly before it together.
        widgets.gap_box.set_visible(self.page_style == PageStyle::Double);
        if let Some(heading) = widgets.gap_box.prev_sibling() {
            heading.set_visible(self.page_style == PageStyle::Double);
        }
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

    /// Bring the left panel's stack and tab pill into line with
    /// `left_tab`, (re)building the tab that is about to show. The
    /// Pages grid also starts its thumbnail decodes and opens
    /// scrolled to the page being read.
    pub fn refresh_left_panel(
        &mut self,
        widgets: &ComicsReaderModelWidgets,
        sender: &ComponentSender<Self>,
    ) {
        if let Some(tabs) = &self.left_tabs {
            tabs.set_selected(match self.left_tab {
                ComicLeftTab::Chapters => 0,
                ComicLeftTab::Pages => 1,
            });
        }
        match self.left_tab {
            ComicLeftTab::Chapters => {
                widgets.left_stack.set_visible_child_name("chapters");
                populate_chapters_list(&widgets.chapters_list_box, self, sender);
            }
            ComicLeftTab::Pages => {
                widgets.left_stack.set_visible_child_name("pages");
                populate_pages_grid(&widgets.pages_box, self, sender);
                update_pages_heading(widgets, self);
                self.trigger_thumbnail_loads(sender);
                // Open scrolled to the current page: rows are a
                // fixed 112 high with a 12 gap, so the target is
                // known before the thumbnails land.
                let vadj = widgets.pages_scroll.vadjustment();
                let row = (self.current_page / 3) as f64;
                glib::idle_add_local_once(move || {
                    let max = (vadj.upper() - vadj.page_size()).max(0.0);
                    vadj.set_value((row * 124.0 - 12.0).clamp(0.0, max));
                });
            }
        }
    }

    pub fn refresh_right_panel(
        &mut self,
        widgets: &ComicsReaderModelWidgets,
        _sender: &ComponentSender<Self>,
    ) {
        if let Some(tabs) = &self.right_tabs {
            tabs.set_selected(match self.right_tab {
                ComicRightTab::Reading => 0,
                ComicRightTab::Shortcuts => 1,
            });
        }
        widgets.right_stack.set_visible_child_name(match self.right_tab {
            ComicRightTab::Reading => "reading",
            ComicRightTab::Shortcuts => "shortcuts",
        });
        self.sync_settings_ui(widgets);
    }

    pub fn cleanup_memory(&mut self) {
        self.textures.clear();
        self.pending_loads.clear();
        self.thumb_textures.clear();
        self.thumb_pending.clear();
        self.thumb_failed.clear();
        self.load_gen = self.load_gen.wrapping_add(1);

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

#[allow(clippy::too_many_arguments)]
fn wrap_comic_page(
    _model: &ComicsReaderModel,
    _page_idx: usize,
    pic: gtk::Picture,
    target_w: i32,
    target_h: i32,
    _sender: Option<&ComponentSender<ComicsReaderModel>>,
) -> gtk::Widget {
    if target_w > 0 && target_h > 0 {
        pic.set_size_request(target_w, target_h);
    }
    pic.into()
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
            let done_lbl = gtk::Label::new(Some("You've reached the end of the series!"));
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
                let wrapped = wrap_comic_page(
                    model,
                    idx,
                    pic,
                    avail_w,
                    target_h,
                    sender,
                );
                item_box.append(&wrapped);
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

                    let (target_w, target_h) = match model.fit_mode {
                        FitMode::Screen => {
                            pic.set_can_shrink(true);
                            pic.set_content_fit(gtk::ContentFit::Contain);
                            pic.set_hexpand(true);
                            pic.set_vexpand(true);
                            pic.set_valign(gtk::Align::Center);
                            pic.set_size_request(-1, -1);
                            (-1, -1)
                        }
                        FitMode::Height => {
                            pic.set_can_shrink(true);
                            pic.set_content_fit(gtk::ContentFit::Contain);
                            pic.set_hexpand(false);
                            pic.set_vexpand(true);
                            pic.set_valign(gtk::Align::Center);
                            pic.set_size_request(-1, -1);
                            (-1, -1)
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
                            (avail_w, target_h)
                        }
                        FitMode::Original => {
                            pic.set_size_request(tw, th);
                            pic.set_can_shrink(false);
                            pic.set_content_fit(gtk::ContentFit::Contain);
                            pic.set_valign(gtk::Align::Center);
                            (tw, th)
                        }
                    };
                    let wrapped = wrap_comic_page(
                        model,
                        idx,
                        pic,
                        target_w,
                        target_h,
                        sender,
                    );
                    container.append(&wrapped);
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

        let (target_w, target_h) = match model.fit_mode {
            FitMode::Screen => {
                pic.set_can_shrink(true);
                pic.set_content_fit(gtk::ContentFit::Contain);
                pic.set_hexpand(true);
                pic.set_vexpand(true);
                pic.set_valign(gtk::Align::Center);
                pic.set_size_request(-1, -1);
                (-1, -1)
            }
            FitMode::Height => {
                pic.set_can_shrink(true);
                pic.set_content_fit(gtk::ContentFit::Contain);
                pic.set_hexpand(false);
                pic.set_vexpand(true);
                pic.set_valign(gtk::Align::Center);
                pic.set_size_request(-1, -1);
                (-1, -1)
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
                (avail_w, target_h)
            }
            FitMode::Original => {
                pic.set_size_request(tw, th);
                pic.set_can_shrink(false);
                pic.set_content_fit(gtk::ContentFit::Contain);
                pic.set_valign(gtk::Align::Center);
                (tw, th)
            }
        };
        let wrapped = wrap_comic_page(
            model,
            model.current_page,
            pic,
            target_w,
            target_h,
            sender,
        );
        vbox.append(&wrapped);
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

/// One chapter's display pieces: its `Ch. N` number, its own title
/// (never the series name repeated), and its volume if it has one.
fn chapter_display(
    model: &ComicsReaderModel,
    idx: usize,
    book: &crate::models::Book,
) -> (String, String, Option<f32>) {
    let meta = model.series_chapter_meta.get(idx);
    let info = crate::comics::parse_comic_title(&book.title);
    let number = meta
        .and_then(|m| m.number)
        .or_else(|| (book.series_index > 0.0).then_some(book.series_index))
        .or(info.number)
        .unwrap_or((idx + 1) as f32);
    let volume = meta.and_then(|m| m.volume).or_else(|| info.volume.map(|v| v as f32));
    let series = model.series_name.as_deref().unwrap_or("");
    let usable = |candidate: &str| {
        let candidate = candidate.trim();
        (!candidate.is_empty() && candidate != series && candidate != book.title)
            .then(|| candidate.to_string())
    };
    let title = meta
        .and_then(|m| usable(&m.title))
        .or_else(|| info.title.as_deref().and_then(usable))
        .unwrap_or_else(|| format!("Chapter {}", format_chapter_number(number)));
    (format!("Ch. {}", format_chapter_number(number)), title, volume)
}

/// The Chapters tab (decision 38): the series head — cover, title,
/// author, counts — then the chapters under their volume headings.
/// The chapter being read carries the accent bar and its page
/// position; read and missing chapters sit at 55%.
fn populate_chapters_list(
    list_box: &gtk::Box,
    model: &ComicsReaderModel,
    sender: &ComponentSender<ComicsReaderModel>,
) {
    while let Some(child) = list_box.first_child() {
        list_box.remove(&child);
    }

    let head = gtk::Box::new(gtk::Orientation::Horizontal, 14);
    head.add_css_class("kalam-comics-chapters-head");
    head.append(&crate::widgets::book_row::cover_widget(
        model.cover_path.as_deref(),
        64,
        96,
    ));
    let head_copy = gtk::Box::new(gtk::Orientation::Vertical, 3);
    head_copy.set_valign(gtk::Align::Center);
    head_copy.set_hexpand(true);
    let title_lbl = gtk::Label::new(Some(
        model.series_name.as_deref().unwrap_or(&model.title),
    ));
    title_lbl.add_css_class("kalam-comics-series-title");
    title_lbl.set_halign(gtk::Align::Start);
    title_lbl.set_wrap(true);
    title_lbl.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    title_lbl.set_lines(2);
    title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
    head_copy.append(&title_lbl);
    if !model.authors.trim().is_empty() {
        let author_lbl = gtk::Label::new(Some(model.authors.trim()));
        author_lbl.add_css_class("kalam-comics-series-meta");
        author_lbl.set_halign(gtk::Align::Start);
        author_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        head_copy.append(&author_lbl);
    }
    let chapters = &model.series_chapters;
    let counts = if chapters.is_empty() {
        format!("{} pages", model.total_pages)
    } else if let Some(total) = model.series_total_pages {
        format!("{} chapters · {} pages", chapters.len(), total)
    } else {
        format!("{} chapters", chapters.len())
    };
    let counts_lbl = gtk::Label::new(Some(&counts));
    counts_lbl.add_css_class("kalam-comics-series-meta");
    counts_lbl.set_halign(gtk::Align::Start);
    head_copy.append(&counts_lbl);
    head.append(&head_copy);
    list_box.append(&head);

    if chapters.is_empty() {
        let empty = gtk::Label::new(Some("No chapters in series"));
        empty.add_css_class("dim-label");
        empty.set_margin_top(24);
        empty.set_halign(gtk::Align::Center);
        list_box.append(&empty);
        return;
    }

    let mut last_volume: Option<f32> = None;
    for (idx, ch) in chapters.iter().enumerate() {
        let (number_label, chapter_title, volume) = chapter_display(model, idx, ch);
        if volume.is_some() && volume != last_volume {
            last_volume = volume;
            let heading = gtk::Label::new(Some(&format!(
                "Volume {}",
                format_chapter_number(volume.unwrap_or(1.0))
            )));
            heading.add_css_class("kalam-comics-volume-heading");
            heading.set_halign(gtk::Align::Start);
            list_box.append(&heading);
        } else if volume.is_some() {
            last_volume = volume;
        }

        let is_current = Some(ch.id) == model.book_id;
        let missing = !ch.file_path.exists();
        let read = ch.progress >= 100 && !is_current;

        let row_box = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row_box.set_valign(gtk::Align::Center);
        let bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
        bar.add_css_class("kalam-comics-current-bar");
        if is_current {
            bar.add_css_class("current");
        }
        bar.set_size_request(3, 26);
        bar.set_valign(gtk::Align::Center);
        row_box.append(&bar);

        let num_lbl = gtk::Label::new(Some(&number_label));
        num_lbl.add_css_class("kalam-comics-chapter-num");
        num_lbl.set_halign(gtk::Align::Start);
        row_box.append(&num_lbl);

        let title_lbl = gtk::Label::new(Some(&chapter_title));
        title_lbl.add_css_class("kalam-comics-chapter-title");
        if is_current {
            title_lbl.add_css_class("current");
        }
        title_lbl.set_halign(gtk::Align::Start);
        title_lbl.set_hexpand(true);
        title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
        row_box.append(&title_lbl);

        let state = if missing {
            Some("Missing".to_string())
        } else if is_current {
            model
                .chapter_page_counts
                .get(idx)
                .copied()
                .flatten()
                .map(|total| format!("{} / {}", model.current_page + 1, total))
        } else if read {
            Some("Read".to_string())
        } else {
            None
        };
        if let Some(state) = state {
            let state_lbl = gtk::Label::new(Some(&state));
            state_lbl.add_css_class("kalam-comics-chapter-state");
            row_box.append(&state_lbl);
        }

        let btn = gtk::Button::new();
        btn.set_child(Some(&row_box));
        btn.add_css_class("kalam-comics-chapter-row");
        btn.set_hexpand(true);
        btn.set_size_request(-1, 44);
        if read || missing {
            btn.set_opacity(0.55);
        }
        let bid = ch.id;
        let s = sender.clone();
        btn.connect_clicked(move |_| {
            s.input(ComicsReaderMsg::SwitchChapterAt(bid, None));
        });
        list_box.append(&btn);
    }
}

/// The Pages tab (decision 38): the current chapter as a 3-column
/// grid of 112-high thumbnails, the page number over the bottom, the
/// page being read ringed in the accent. A page whose thumbnail has
/// not arrived is a skeleton in the border colour; one that failed
/// is the dashed card. The widget handles are kept on the model so a
/// decode landing — or a page turn — updates one cell instead of
/// rebuilding the grid under the reader's scroll position.
fn populate_pages_grid(
    grid_host: &gtk::Box,
    model: &mut ComicsReaderModel,
    sender: &ComponentSender<ComicsReaderModel>,
) {
    while let Some(child) = grid_host.first_child() {
        grid_host.remove(&child);
    }
    model.page_thumb_buttons.clear();
    model.page_thumb_pictures.clear();
    model.page_thumb_status.clear();

    if model.total_pages == 0 {
        let empty = gtk::Label::new(Some("No pages in this chapter"));
        empty.add_css_class("dim-label");
        empty.set_margin_top(24);
        empty.set_halign(gtk::Align::Center);
        grid_host.append(&empty);
        return;
    }

    let grid = gtk::Grid::new();
    grid.set_column_spacing(12);
    grid.set_row_spacing(12);
    grid.set_column_homogeneous(true);
    grid.set_hexpand(true);

    for idx in 0..model.total_pages {
        let failed = model.thumb_failed.contains(&idx);
        let texture = model
            .thumb_textures
            .get(&idx)
            .or_else(|| model.textures.get(&idx));

        let overlay = gtk::Overlay::new();
        overlay.set_size_request(86, 112);
        overlay.set_hexpand(true);
        overlay.set_vexpand(false);

        let pic = gtk::Picture::new();
        if let Some(tex) = texture {
            pic.set_paintable(Some(tex));
        }
        pic.set_content_fit(gtk::ContentFit::Cover);
        pic.set_can_shrink(true);
        pic.set_hexpand(true);
        pic.set_vexpand(true);
        pic.set_size_request(86, 112);
        overlay.set_child(Some(&pic));

        let status = gtk::Label::new(Some(if failed { "Failed to load" } else { "" }));
        status.add_css_class("kalam-comics-page-status");
        status.set_halign(gtk::Align::Center);
        status.set_valign(gtk::Align::Center);
        status.set_wrap(true);
        status.set_justify(gtk::Justification::Center);
        overlay.add_overlay(&status);

        let num = gtk::Label::new(Some(&format!("{}", idx + 1)));
        num.add_css_class("kalam-comics-page-num");
        num.set_halign(gtk::Align::Center);
        num.set_valign(gtk::Align::End);
        num.set_margin_bottom(6);
        overlay.add_overlay(&num);

        let btn = gtk::Button::new();
        btn.set_child(Some(&overlay));
        btn.add_css_class("kalam-comics-page-thumb");
        if idx == model.current_page {
            btn.add_css_class("current");
        }
        if failed {
            btn.add_css_class("failed");
        }
        btn.set_tooltip_text(Some(&format!("Page {}", idx + 1)));
        let s = sender.clone();
        btn.connect_clicked(move |_| {
            s.input(ComicsReaderMsg::SetPage(idx));
        });

        grid.attach(&btn, (idx % 3) as i32, (idx / 3) as i32, 1, 1);
        model.page_thumb_buttons.push(btn);
        model.page_thumb_pictures.push(pic);
        model.page_thumb_status.push(status);
    }
    grid_host.append(&grid);
}

/// The Pages tab's sticky heading: the chapter on show and how far
/// through it the reader is (`36 pages · reading`, `· read` once
/// finished).
fn update_pages_heading(widgets: &ComicsReaderModelWidgets, model: &ComicsReaderModel) {
    let chapter = model
        .current_chapter_idx
        .and_then(|idx| {
            model
                .series_chapters
                .get(idx)
                .map(|book| chapter_display(model, idx, book).0)
        })
        .unwrap_or_else(|| "Chapter".to_string());
    widgets.pages_chapter_label.set_label(&chapter);
    let state = if model.total_pages > 0 && model.current_page + 1 >= model.total_pages {
        "read"
    } else {
        "reading"
    };
    widgets
        .pages_meta_label
        .set_label(&format!("{} pages · {}", model.total_pages, state));
}

/// Mark the page being read in a standing Pages grid, without
/// rebuilding it.
fn sync_pages_current(model: &ComicsReaderModel) {
    for (idx, btn) in model.page_thumb_buttons.iter().enumerate() {
        if idx == model.current_page {
            btn.add_css_class("current");
        } else {
            btn.remove_css_class("current");
        }
    }
}

fn shortcut_row(label: &str, keys: &[&str]) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("kalam-comics-shortcut-row");
    row.set_size_request(-1, 48);
    let lbl = gtk::Label::new(Some(label));
    lbl.set_halign(gtk::Align::Start);
    lbl.set_hexpand(true);
    lbl.set_valign(gtk::Align::Center);
    row.append(&lbl);
    for key in keys {
        let chip = gtk::Label::new(Some(key));
        chip.add_css_class("kalam-ui-key-chip");
        chip.set_valign(gtk::Align::Center);
        row.append(&chip);
    }
    row
}

fn shortcut_group(host: &gtk::Box, heading: &str, rows: &[(&str, &[&str])]) {
    let heading_lbl = gtk::Label::new(Some(heading));
    heading_lbl.add_css_class("kalam-reader-section-label");
    heading_lbl.set_halign(gtk::Align::Start);
    heading_lbl.set_margin_top(14);
    heading_lbl.set_margin_bottom(8);
    host.append(&heading_lbl);
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("kalam-comics-inset-card");
    for (label, keys) in rows {
        card.append(&shortcut_row(label, keys));
    }
    host.append(&card);
}

/// The Shortcuts pane (decision 50): the reader's keys in their
/// three groups. Recording a new key and the conflict refusal ride
/// on the shared keybind store and join here with it; until then
/// the pane is the honest reference for the keys the reader answers.
fn build_shortcuts_panel(host: &gtk::Box) {
    shortcut_group(
        host,
        "Navigation",
        &[
            ("Next page", &["→", "Space"][..]),
            ("Previous page", &["←"][..]),
            ("Go to page", &["G"][..]),
            ("Toggle bookmark", &["B"][..]),
        ],
    );
    shortcut_group(
        host,
        "View",
        &[
            ("Toggle spread", &["D"][..]),
            ("Fit mode", &["F"][..]),
        ],
    );
    shortcut_group(
        host,
        "Panels",
        &[
            ("Chapters and Pages", &["T"][..]),
            ("Reading settings", &["S"][..]),
            ("Exit book", &["Esc"][..]),
        ],
    );
}

/// The Reading pane's icon tiles (decision 39): the glyph over its
/// name, in a 68-high bordered tile.
fn comic_tile_content(icon: &str, label: &str) -> gtk::Box {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 6);
    box_.set_halign(gtk::Align::Center);
    box_.set_valign(gtk::Align::Center);
    box_.append(&crate::icons::symbolic_with_classes(
        icon,
        20,
        &["kalam-inline-icon"],
    ));
    let label_widget = gtk::Label::new(Some(label));
    label_widget.set_halign(gtk::Align::Center);
    box_.append(&label_widget);
    box_
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

            // ── 2. The 15px edge strips that summon the panels ──────
            // No dim layer anywhere (decision 35): a panel is put away
            // by a click on the page, which is spent doing it.
            #[name = "left_edge_box"]
            add_overlay = &gtk::Box {
                add_css_class: "kalam-reader-hover-edge",
                add_css_class: "kalam-reader-hover-edge-left",
                set_width_request: 15,
                set_hexpand: false,
                set_vexpand: true,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Fill,
            },

            #[name = "right_edge_box"]
            add_overlay = &gtk::Box {
                add_css_class: "kalam-reader-hover-edge",
                add_css_class: "kalam-reader-hover-edge-right",
                set_width_request: 15,
                set_hexpand: false,
                set_vexpand: true,
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Fill,
            },

            // ── 3. The Back pill with its Bubble slide-out ───────────
            // Hidden until the pointer rests in the top-left zone
            // (decision 15, inherited by 35). The unit itself is
            // built in init (the shared slide-out pill) and appended
            // into `back_host`.
            #[name = "back_revealer"]
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_back_button && !model.left_panel_open,
                set_transition_type: gtk::RevealerTransitionType::SlideDown,
                set_transition_duration: 220,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Start,

                #[name = "back_host"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_margin_start: 18,
                    set_margin_top: 14,
                },
            },

            // ── 4. The bookmark ribbon ────────────────────────────────
            // Always there, hanging from the top edge 48 from the
            // right (decision 16); the icon and the `.set` class
            // carry the state.
            #[name = "ribbon_button"]
            add_overlay = &gtk::Button {
                add_css_class: "kalam-reader-ribbon",
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Start,
                set_margin_end: 48,
                connect_clicked => ComicsReaderMsg::ToggleBookmark,

                #[name = "ribbon_image"]
                gtk::Image {
                    set_icon_name: Some("kalam-ribbon-outline-symbolic"),
                    set_pixel_size: 26,
                },
            },

            // ── 5. The location text ──────────────────────────────────
            // `12 / 180` bottom-right, always visible, no pill under
            // it (decision 35). Clicking it opens the jump field.
            #[name = "page_indicator_label"]
            add_overlay = &gtk::Label {
                add_css_class: "kalam-reader-location",
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::End,
                set_margin_end: 24,
                set_margin_bottom: 16,
                #[watch]
                set_label: &model.page_indicator_label(),
            },

            // ── 6. The page-jump field ────────────────────────────────
            // A 52-high pill at the top centre (decision 50), in the
            // search bar's shape: label, the underlined number, the
            // `of N` hint (accent `Pages 1–N` out of range), the
            // Page/Chapter switch, and a close. The switch is built
            // in init into `jump_mode_host`.
            #[name = "jump_revealer"]
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.jump_open,
                set_transition_type: gtk::RevealerTransitionType::SlideDown,
                set_transition_duration: 180,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Start,
                set_margin_top: 16,

                #[wrap(Some)]
                set_child = &gtk::Box {
                    add_css_class: "kalam-comics-jump",
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 10,
                    set_size_request: (480, -1),

                    gtk::Label {
                        add_css_class: "kalam-comics-jump-label",
                        set_label: "Go to",
                        set_valign: gtk::Align::Center,
                    },

                    #[name = "jump_entry"]
                    gtk::Entry {
                        add_css_class: "kalam-comics-jump-entry",
                        set_width_chars: 5,
                        set_max_width_chars: 7,
                        set_valign: gtk::Align::Center,
                        set_input_purpose: gtk::InputPurpose::Digits,
                        #[watch]
                        set_css_classes: if model.jump_out_of_range() {
                            &["kalam-comics-jump-entry", "error"]
                        } else {
                            &["kalam-comics-jump-entry"]
                        },
                    },

                    #[name = "jump_hint"]
                    gtk::Label {
                        add_css_class: "kalam-comics-jump-hint",
                        set_valign: gtk::Align::Center,
                        set_hexpand: true,
                        set_halign: gtk::Align::End,
                        #[watch]
                        set_label: &model.jump_hint(),
                        #[watch]
                        set_css_classes: if model.jump_out_of_range() {
                            &["kalam-comics-jump-hint", "error"]
                        } else {
                            &["kalam-comics-jump-hint"]
                        },
                    },

                    #[name = "jump_mode_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_valign: gtk::Align::Center,
                    },

                    gtk::Button {
                        set_child: Some(&crate::icons::symbolic_with_classes(
                            "window-close-symbolic",
                            14,
                            &["kalam-inline-icon"],
                        )),
                        add_css_class: "kalam-comics-jump-close",
                        set_valign: gtk::Align::Center,
                        set_tooltip_text: Some("Close (Esc)"),
                        connect_clicked => ComicsReaderMsg::CloseJump,
                    },
                },
            },

            // ── 7. The transient readout ──────────────────────────────
            // Bottom centre, only while it has something to say (the
            // zoom readout's home, decision 42; fit and direction
            // changes borrowed it while zoom is still to come).
            add_overlay = &gtk::Revealer {
                #[watch]
                set_reveal_child: model.show_zoom_osd,
                set_transition_type: gtk::RevealerTransitionType::Crossfade,
                set_transition_duration: 250,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::End,
                set_can_target: false,
                set_margin_bottom: 24,

                #[wrap(Some)]
                set_child = &gtk::Label {
                    add_css_class: "kalam-zoom-osd",
                    #[watch]
                    set_label: &model.osd_text,
                },
            },

            // ── 8. The left panel: Chapters and Pages ─────────────────
            // Floating 16 from every edge (decision 35); the tab pill
            // rides at the top (decision 37), built in init into
            // `left_tabs_host`. The series head lives inside the
            // Chapters tab now, not over the whole panel.
            #[name = "left_revealer"]
            add_overlay = &gtk::Revealer {
                add_css_class: "kalam-reader-sidebar-shell",
                add_css_class: "kalam-reader-sidebar-shell-left",
                #[watch]
                set_reveal_child: model.left_panel_open,
                set_transition_type: gtk::RevealerTransitionType::SlideRight,
                set_transition_duration: 220,
                set_halign: gtk::Align::Start,
                set_valign: gtk::Align::Fill,
                set_margin_start: 16,
                set_margin_top: 16,
                set_margin_bottom: 16,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_size_request: (340, -1),
                    set_vexpand: true,
                    add_css_class: "kalam-reader-sidebar",
                    add_css_class: "kalam-reader-sidebar-left",
                    add_css_class: "kalam-comics-panel",

                    #[name = "left_tabs_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_margin_start: 20,
                        set_margin_end: 20,
                        set_margin_top: 16,
                    },

                    // The Pages tab's sticky chapter heading: it sits
                    // outside the scrolling grid and names the chapter
                    // the grid is showing.
                    gtk::Box {
                        add_css_class: "kalam-comics-pages-heading",
                        set_orientation: gtk::Orientation::Horizontal,
                        set_spacing: 8,
                        set_margin_start: 20,
                        set_margin_end: 20,
                        set_margin_top: 14,
                        #[watch]
                        set_visible: model.left_tab == ComicLeftTab::Pages,

                        #[name = "pages_chapter_label"]
                        gtk::Label {
                            add_css_class: "kalam-reader-section-label",
                            set_halign: gtk::Align::Start,
                            set_hexpand: true,
                            set_ellipsize: gtk::pango::EllipsizeMode::End,
                        },

                        #[name = "pages_meta_label"]
                        gtk::Label {
                            add_css_class: "kalam-comics-pages-meta",
                            set_halign: gtk::Align::End,
                        },
                    },

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
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 0,
                                set_margin_start: 20,
                                set_margin_end: 20,
                                set_margin_top: 16,
                                set_margin_bottom: 20,
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

                                // Section: PAGE STYLE
                                append = &reader_settings_section("Page style"),
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 4,
                                    set_margin_start: 16,
                                    set_margin_end: 16,
                                    set_homogeneous: true,

                                    #[name = "style_single_btn"]
                                    gtk::Button {
                                        set_child: Some(&comic_tile_content(
                                            "view-paged-symbolic",
                                            "Single",
                                        )),
                                        add_css_class: "kalam-reader-seg-btn",
                                        add_css_class: "kalam-comics-tile",
                                        set_tooltip_text: Some("Single page display"),
                                        connect_clicked => ComicsReaderMsg::SetPageStyle(PageStyle::Single),
                                    },

                                    #[name = "style_double_btn"]
                                    gtk::Button {
                                        set_child: Some(&comic_tile_content(
                                            "view-dual-symbolic",
                                            "Spread",
                                        )),
                                        add_css_class: "kalam-reader-seg-btn",
                                        add_css_class: "kalam-comics-tile",
                                        set_tooltip_text: Some("Two facing pages"),
                                        connect_clicked => ComicsReaderMsg::SetPageStyle(PageStyle::Double),
                                    },

                                    #[name = "style_strip_btn"]
                                    gtk::Button {
                                        set_child: Some(&comic_tile_content(
                                            "kalam-pdf-page-scroll-symbolic",
                                            "Long strip",
                                        )),
                                        add_css_class: "kalam-reader-seg-btn",
                                        add_css_class: "kalam-comics-tile",
                                        set_tooltip_text: Some("Continuous vertical strip"),
                                        connect_clicked => ComicsReaderMsg::SetPageStyle(PageStyle::LongStrip),
                                    },
                                },

                                // Section: READING DIRECTION
                                append = &reader_settings_section("Direction"),
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 4,
                                    set_margin_start: 16,
                                    set_margin_end: 16,
                                    set_homogeneous: true,

                                    #[name = "dir_ltr_btn"]
                                    gtk::Button {
                                        set_label: "LTR",
                                        add_css_class: "kalam-reader-seg-btn",
                                        add_css_class: "kalam-comics-direction-btn",
                                        set_tooltip_text: Some("Left to right (Western comics)"),
                                        connect_clicked => ComicsReaderMsg::SetDirection(ReadingDirection::Ltr),
                                    },

                                    #[name = "dir_rtl_btn"]
                                    gtk::Button {
                                        set_label: "RTL",
                                        add_css_class: "kalam-reader-seg-btn",
                                        add_css_class: "kalam-comics-direction-btn",
                                        set_tooltip_text: Some("Right to left (Manga)"),
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
                                append = &reader_settings_section("Fit"),
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Horizontal,
                                    set_spacing: 4,
                                    set_margin_start: 16,
                                    set_margin_end: 16,
                                    set_homogeneous: true,

                                    #[name = "fit_width_btn"]
                                    gtk::Button {
                                        set_child: Some(&comic_tile_content(
                                            "zoom-fit-best-symbolic",
                                            "Width",
                                        )),
                                        add_css_class: "kalam-reader-seg-btn",
                                        add_css_class: "kalam-comics-tile",
                                        set_tooltip_text: Some("Fit image to viewport width"),
                                        connect_clicked => ComicsReaderMsg::SetFitMode(FitMode::Width),
                                    },

                                    #[name = "fit_height_btn"]
                                    gtk::Button {
                                        set_child: Some(&comic_tile_content(
                                            "view-fullscreen-symbolic",
                                            "Height",
                                        )),
                                        add_css_class: "kalam-reader-seg-btn",
                                        add_css_class: "kalam-comics-tile",
                                        set_tooltip_text: Some("Fit image to viewport height"),
                                        connect_clicked => ComicsReaderMsg::SetFitMode(FitMode::Height),
                                    },

                                    #[name = "fit_screen_btn"]
                                    gtk::Button {
                                        set_child: Some(&comic_tile_content(
                                            "zoom-fit-best-symbolic",
                                            "Screen",
                                        )),
                                        add_css_class: "kalam-reader-seg-btn",
                                        add_css_class: "kalam-comics-tile",
                                        set_tooltip_text: Some("Fit entire image inside screen"),
                                        connect_clicked => ComicsReaderMsg::SetFitMode(FitMode::Screen),
                                    },

                                    #[name = "fit_orig_btn"]
                                    gtk::Button {
                                        set_child: Some(&comic_tile_content(
                                            "zoom-original-symbolic",
                                            "Original",
                                        )),
                                        add_css_class: "kalam-reader-seg-btn",
                                        add_css_class: "kalam-comics-tile",
                                        set_tooltip_text: Some("Display at 100% original size"),
                                        connect_clicked => ComicsReaderMsg::SetFitMode(FitMode::Original),
                                    },
                                },

                            },
                        },

                        // Pages View: the current chapter's grid,
                        // populated by `populate_pages_grid`.
                        #[name = "pages_scroll"]
                        gtk::ScrolledWindow {
                            set_hexpand: true,
                            set_vexpand: true,
                            set_hscrollbar_policy: gtk::PolicyType::Never,

                            #[name = "pages_box"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 0,
                                set_margin_start: 20,
                                set_margin_end: 20,
                                set_margin_top: 12,
                                set_margin_bottom: 20,
                            },
                        },
                    },
                },
            },

            // ── 9. The right panel: Reading and Shortcuts ─────────────
            // The Reading pane's widget tree is declared above (it
            // predates the split); init reparents its scroll window
            // into this stack, so the named controls and their sync
            // code survive the move untouched.
            #[name = "right_revealer"]
            add_overlay = &gtk::Revealer {
                add_css_class: "kalam-reader-sidebar-shell",
                add_css_class: "kalam-reader-sidebar-shell-right",
                #[watch]
                set_reveal_child: model.right_panel_open,
                set_transition_type: gtk::RevealerTransitionType::SlideLeft,
                set_transition_duration: 220,
                set_halign: gtk::Align::End,
                set_valign: gtk::Align::Fill,
                set_margin_end: 16,
                set_margin_top: 16,
                set_margin_bottom: 16,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_size_request: (340, -1),
                    set_vexpand: true,
                    add_css_class: "kalam-reader-sidebar",
                    add_css_class: "kalam-reader-sidebar-right",
                    add_css_class: "kalam-comics-panel",

                    #[name = "right_tabs_host"]
                    gtk::Box {
                        set_orientation: gtk::Orientation::Horizontal,
                        set_margin_start: 16,
                        set_margin_end: 16,
                        set_margin_top: 16,
                    },

                    #[name = "right_stack"]
                    gtk::Stack {
                        set_hexpand: true,
                        set_vexpand: true,
                        set_transition_type: gtk::StackTransitionType::Crossfade,

                        // Shortcuts View, populated in init by
                        // `build_shortcuts_panel`.
                        #[name = "shortcuts_scroll"]
                        gtk::ScrolledWindow {
                            set_hexpand: true,
                            set_vexpand: true,
                            set_hscrollbar_policy: gtk::PolicyType::Never,

                            #[name = "shortcuts_box"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 0,
                                set_margin_start: 16,
                                set_margin_end: 16,
                                set_margin_bottom: 20,
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

        // The Reading pane's scroll window is declared in the left
        // panel (its tree predates the split); move it into the
        // right panel's stack before anything syncs it.
        widgets.left_stack.remove(&widgets.settings_scroll);
        widgets
            .right_stack
            .add_named(&widgets.settings_scroll, Some("reading"));

        // Explicitly register stack page names so GTK child lookup succeeds without warnings
        widgets.left_stack.page(&widgets.chapters_scroll).set_name("chapters");
        widgets.left_stack.page(&widgets.pages_scroll).set_name("pages");
        widgets.right_stack.page(&widgets.shortcuts_scroll).set_name("shortcuts");
        widgets.left_stack.set_visible_child_name("chapters");
        widgets.right_stack.set_visible_child_name("reading");

        // The panel tab pills (decision 37): the shared segmented
        // control at panel size, stretched across the panel with
        // each tab taking an equal share.
        let make_tabs = |labels: &[&str],
                         host: &gtk::Box,
                         on_change: Rc<dyn Fn(usize) + 'static>|
         -> crate::widgets::segmented::SegmentedControl {
            let control = crate::widgets::segmented::segmented_control(
                labels,
                0,
                crate::widgets::segmented::SegmentedSize::Panel,
                move |idx| on_change(idx),
            );
            let root = control.widget();
            root.set_hexpand(true);
            root.set_halign(gtk::Align::Fill);
            for button in control.buttons() {
                button.set_hexpand(true);
            }
            host.append(&root);
            control
        };
        {
            let tx = sender.input_sender().clone();
            model.left_tabs = Some(make_tabs(
                &["Chapters", "Pages"],
                &widgets.left_tabs_host,
                Rc::new(move |idx| {
                    let _ = tx.send(ComicsReaderMsg::SetLeftTab(if idx == 1 {
                        ComicLeftTab::Pages
                    } else {
                        ComicLeftTab::Chapters
                    }));
                }),
            ));
            let tx = sender.input_sender().clone();
            model.right_tabs = Some(make_tabs(
                &["Reading", "Shortcuts"],
                &widgets.right_tabs_host,
                Rc::new(move |idx| {
                    let _ = tx.send(ComicsReaderMsg::SetRightTab(if idx == 1 {
                        ComicRightTab::Shortcuts
                    } else {
                        ComicRightTab::Reading
                    }));
                }),
            ));
            let tx = sender.input_sender().clone();
            model.jump_mode_control = Some(crate::widgets::segmented::segmented_control(
                &["Page", "Chapter"],
                0,
                crate::widgets::segmented::SegmentedSize::Small,
                move |idx| {
                    let _ = tx.send(ComicsReaderMsg::SetJumpMode(if idx == 1 {
                        JumpMode::Chapter
                    } else {
                        JumpMode::Page
                    }));
                },
            ));
            widgets
                .jump_mode_host
                .append(&model.jump_mode_control.as_ref().expect("just built").widget());
        }

        // The Back+Bubble unit (decision 15): the shell's own Back
        // pill, with the shared slide-out carrying an icon-only
        // Bubble behind it. Focus entering the pill reveals the
        // unit too, so keyboard readers meet it by tabbing.
        {
            let back_button = gtk::Button::new();
            back_button.set_child(Some(&crate::icons::labelled(
                "go-previous-symbolic",
                16,
                "Back",
                6,
            )));
            back_button.add_css_class("kalam-back-btn");
            back_button.add_css_class("kalam-back-float");
            back_button.set_can_focus(true);
            let tx = sender.input_sender().clone();
            back_button.connect_clicked(move |_| {
                let _ = tx.send(ComicsReaderMsg::Close);
            });
            let tx = sender.input_sender().clone();
            back_button.connect_has_focus_notify(move |btn| {
                if btn.has_focus() {
                    let _ = tx.send(ComicsReaderMsg::ShowBackChrome);
                }
            });

            let bubble_button = gtk::Button::new();
            bubble_button.set_child(Some(&crate::icons::symbolic_with_classes(
                "view-dual-symbolic",
                16,
                &["kalam-inline-icon"],
            )));
            bubble_button.set_tooltip_text(Some("Minimize to Bubble"));
            bubble_button.update_property(&[gtk::accessible::Property::Label(
                "Minimize to Bubble",
            )]);
            bubble_button.set_size_request(44, 44);

            let tx = sender.input_sender().clone();
            let unit = crate::widgets::slide_out_pill::slide_out_pill_with_button(
                &back_button,
                bubble_button,
                move || {
                    let _ = tx.send(ComicsReaderMsg::MinimizeToBubble);
                },
            );
            widgets.back_host.append(&unit.widget());
            model.back_unit = Some(unit);
        }

        // Reduced motion turns the panel slides into fades (decision
        // 15, system §9) — decided once at open, like the EPUB side.
        if crate::appearance::reduce_motion() {
            widgets
                .back_revealer
                .set_transition_type(gtk::RevealerTransitionType::Crossfade);
            widgets
                .left_revealer
                .set_transition_type(gtk::RevealerTransitionType::Crossfade);
            widgets
                .right_revealer
                .set_transition_type(gtk::RevealerTransitionType::Crossfade);
            widgets
                .jump_revealer
                .set_transition_type(gtk::RevealerTransitionType::Crossfade);
        }

        // The jump field's entry: every keystroke re-validates
        // (the accent range state is live), Enter jumps.
        {
            let tx = sender.input_sender().clone();
            widgets.jump_entry.connect_changed(move |entry| {
                let _ = tx.send(ComicsReaderMsg::JumpTextChanged(entry.text().to_string()));
            });
            let tx = sender.input_sender().clone();
            widgets.jump_entry.connect_activate(move |_| {
                let _ = tx.send(ComicsReaderMsg::SubmitJump);
            });
        }

        build_shortcuts_panel(&widgets.shortcuts_box);
        populate_chapters_list(&widgets.chapters_list_box, &model, &sender);
        populate_pages_grid(&widgets.pages_box, &mut model, &sender);
        update_pages_heading(&widgets, &model);
        model.update_bookmark_icon_state(&widgets);
        model.sync_settings_ui(&widgets);

        let is_webtoon = model.direction == ReadingDirection::Webtoon || model.page_style == PageStyle::LongStrip;
        update_viewport_policies(&widgets.viewport_box, model.fit_mode, is_webtoon);
        let child = rebuild_viewport_widget(&model, 850, Some(&sender));
        widgets.viewport_box.set_child(Some(&child));

        // 1. The Back unit's rest zone (decision 15): the top-left
        // 180 × 68. Enter/leave transitions are reported once each;
        // the 250 ms rest and the 300 ms grace live in the update arms.
        {
            let root_motion = gtk::EventControllerMotion::new();
            let tx = sender.input_sender().clone();
            let was_in_zone = Rc::new(std::cell::Cell::new(false));
            let was_in_zone_move = was_in_zone.clone();
            root_motion.connect_motion(move |_, x, y| {
                let in_zone = x < 180.0 && y < 68.0;
                if in_zone != was_in_zone_move.get() {
                    was_in_zone_move.set(in_zone);
                    let _ = tx.send(ComicsReaderMsg::BackZoneHover(in_zone));
                }
            });
            let tx_leave = sender.input_sender().clone();
            root_motion.connect_leave(move |_| {
                if was_in_zone.get() {
                    was_in_zone.set(false);
                    let _ = tx_leave.send(ComicsReaderMsg::BackZoneHover(false));
                }
            });
            root.add_controller(root_motion);
        }

        // 2. The edge strips summon their panels (decision 35).
        {
            let left_motion = gtk::EventControllerMotion::new();
            let tx = sender.input_sender().clone();
            left_motion.connect_enter(move |_, _, _| {
                let _ = tx.send(ComicsReaderMsg::OpenLeftPanel);
            });
            widgets.left_edge_box.add_controller(left_motion);

            let right_motion = gtk::EventControllerMotion::new();
            let tx = sender.input_sender().clone();
            right_motion.connect_enter(move |_, _, _| {
                let _ = tx.send(ComicsReaderMsg::OpenRightPanel);
            });
            widgets.right_edge_box.add_controller(right_motion);
        }

        // 3. Clicking the location text opens the jump field: it is
        // the one piece of page furniture that names a place, so it
        // is where naming a different place starts.
        {
            let click = gtk::GestureClick::new();
            let tx = sender.input_sender().clone();
            click.connect_pressed(move |gesture, _, _, _| {
                let _ = tx.send(ComicsReaderMsg::OpenJump(None));
                gesture.set_state(gtk::EventSequenceState::Claimed);
            });
            widgets.page_indicator_label.add_controller(click);
        }

        // 4. Viewport click: grab keyboard focus, and put a standing
        // panel away — the click is spent doing it (decision 20),
        // so it does not also fall through to anything else. The
        // panels are overlay siblings of the viewport, so a click
        // that lands here was never on a panel.
        let click = gtk::GestureClick::new();
        let root_for_focus = root.clone();
        let tx_click = sender.input_sender().clone();
        click.connect_pressed(move |gesture, _, _, _| {
            root_for_focus.grab_focus();
            let _ = tx_click.send(ComicsReaderMsg::ClosePanels);
            gesture.set_state(gtk::EventSequenceState::Claimed);
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
        key_controller.connect_key_pressed(move |controller, keyval, _, _| {
            // The jump field is a text entry standing on this same
            // root: while it (or any entry) holds focus, its keys are
            // its own — BackSpace edits, digits type — and only Esc
            // is answered here, as the layered close.
            let is_typing = controller
                .widget()
                .and_then(|w| w.root())
                .and_then(|r| r.focus())
                .is_some_and(|f| {
                    f.is::<gtk::Editable>() || f.is::<gtk::Text>() || f.is::<gtk::TextView>()
                });
            if keyval == gdk::Key::Escape {
                let _ = s_key.input_sender().send(ComicsReaderMsg::Escape);
                return glib::Propagation::Stop;
            }
            if is_typing {
                return glib::Propagation::Proceed;
            }
            match keyval {
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
                gdk::Key::d | gdk::Key::D => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::ToggleSpread);
                    glib::Propagation::Stop
                }
                gdk::Key::g | gdk::Key::G => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::OpenJump(None));
                    glib::Propagation::Stop
                }
                gdk::Key::t | gdk::Key::T => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::ToggleLeftPanel);
                    glib::Propagation::Stop
                }
                gdk::Key::s | gdk::Key::S => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::ToggleRightPanel);
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

        // Screenshot loop: the page opens bare, so a proof frame of
        // the summoned chrome has to ask for it (the EPUB loop's
        // `KALAM_READER_ACTION`, in this reader's own spelling).
        match std::env::var("KALAM_COMIC_ACTION").as_deref() {
            Ok("chrome") => {
                sender.input(ComicsReaderMsg::ShowBackChrome);
                if let Some(unit) = &model.back_unit {
                    unit.reveal();
                }
            }
            Ok("chapters") => sender.input(ComicsReaderMsg::OpenLeftPanel),
            Ok("pages") => {
                sender.input(ComicsReaderMsg::SetLeftTab(ComicLeftTab::Pages));
                sender.input(ComicsReaderMsg::OpenLeftPanel);
            }
            Ok("reading") => sender.input(ComicsReaderMsg::OpenRightPanel),
            Ok("shortcuts") => {
                sender.input(ComicsReaderMsg::SetRightTab(ComicRightTab::Shortcuts));
                sender.input(ComicsReaderMsg::OpenRightPanel);
            }
            Ok("jump") => sender.input(ComicsReaderMsg::OpenJump(None)),
            Ok("jump-range") => {
                sender.input(ComicsReaderMsg::OpenJump(Some("240".to_string())))
            }
            _ => {}
        }

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
            ComicsReaderMsg::SetPage(idx) => {
                self.hide_chrome_on_interaction();
                self.set_page(idx);
                self.at_chapter_end = false;
                self.save_progress();
                self.trigger_loads(&sender);
                self.update_bookmark_icon_state(widgets);
                sync_pages_current(self);
                update_pages_heading(widgets, self);
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
                    sync_pages_current(self);
                    update_pages_heading(widgets, self);
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
                sync_pages_current(self);
                update_pages_heading(widgets, self);
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
                sync_pages_current(self);
                update_pages_heading(widgets, self);
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
            ComicsReaderMsg::PageLoaded(gen, idx, ref maybe_tex) => {
                if gen != self.load_gen {
                    // Asked for by the chapter that was left; its
                    // page numbers mean nothing here.
                    return;
                }
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
                                        let wrapped = wrap_comic_page(
                                            self,
                                            idx,
                                            pic,
                                            avail_w,
                                            target_h,
                                            Some(&sender),
                                        );
                                        item_box.append(&wrapped);
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
            ComicsReaderMsg::ThumbnailLoaded(gen, idx, maybe_tex) => {
                if gen != self.load_gen {
                    return;
                }
                self.thumb_pending.remove(&idx);
                match maybe_tex {
                    Some(tex) => {
                        if let Some(pic) = self.page_thumb_pictures.get(idx) {
                            pic.set_paintable(Some(&tex));
                        }
                        self.thumb_textures.insert(idx, tex);
                    }
                    None => {
                        self.thumb_failed.insert(idx);
                        if let Some(btn) = self.page_thumb_buttons.get(idx) {
                            btn.add_css_class("failed");
                        }
                        if let Some(status) = self.page_thumb_status.get(idx) {
                            status.set_label("Failed to load");
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
            ComicsReaderMsg::ToggleSpread => {
                let next = if self.page_style == PageStyle::Double {
                    PageStyle::Single
                } else {
                    PageStyle::Double
                };
                let _ = sender.input_sender().send(ComicsReaderMsg::SetPageStyle(next));
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
                    let _ = sender
                        .input_sender()
                        .send(ComicsReaderMsg::SwitchChapterAt(next_ch.id, None));
                }
            }
            ComicsReaderMsg::PrevChapter => {
                if let Some(prev_ch) = self.prev_chapter().cloned() {
                    let _ = sender
                        .input_sender()
                        .send(ComicsReaderMsg::SwitchChapterAt(prev_ch.id, None));
                }
            }
            ComicsReaderMsg::SwitchChapterAt(new_bid, target_page) => {
                if let (Some(_), Some(curr_bid)) = (&self.catalog, self.book_id) {
                    if curr_bid == new_bid {
                        if let Some(page) = target_page {
                            let _ = sender.input_sender().send(ComicsReaderMsg::SetPage(page));
                        }
                        return;
                    }
                    self.save_progress();
                }
                if let Some(catalog) = self.catalog.clone() {
                    if let Ok(Some(new_book)) = catalog.get_book(new_bid) {
                        if let Ok(new_prov) = crate::pages::comics_reader::providers::LocalProvider::new(new_book.file_path.clone()) {
                            self.book_id = Some(new_bid);
                            self.title = new_book.title.clone();
                            self.cover_path = new_book.cover_path.clone();
                            if !new_book.authors.trim().is_empty() {
                                self.authors = new_book.authors.clone();
                            }
                            self.provider = std::sync::Arc::new(new_prov);
                            self.total_pages = self.provider.page_count();
                            self.textures.clear();
                            self.pending_loads.clear();
                            self.thumb_textures.clear();
                            self.thumb_pending.clear();
                            self.thumb_failed.clear();
                            self.load_gen = self.load_gen.wrapping_add(1);
                            self.at_chapter_end = false;

                            self.current_page = match target_page {
                                Some(page) if self.total_pages > 0 => {
                                    page.min(self.total_pages - 1)
                                }
                                Some(_) => 0,
                                None => match catalog.get_reading_progress(new_bid) {
                                    Ok(Some((page, _))) if page < self.total_pages => page,
                                    _ => 0,
                                },
                            };

                            self.current_chapter_idx = self.series_chapters.iter().position(|b| b.id == new_bid);
                            // The chapter just opened has a provider-
                            // true count now, whatever the up-front
                            // listing said; keep the series accounting
                            // honest with it.
                            if let Some(idx) = self.current_chapter_idx {
                                if let Some(slot) = self.chapter_page_counts.get_mut(idx) {
                                    *slot = (self.total_pages > 0).then_some(self.total_pages);
                                    let (offsets, total) =
                                        page_offsets_and_total(&self.chapter_page_counts);
                                    self.chapter_page_offsets = offsets;
                                    self.series_total_pages = total;
                                }
                            }

                            self.reload_bookmarks();
                            self.update_bookmark_icon_state(widgets);
                            populate_chapters_list(&widgets.chapters_list_box, self, &sender);
                            populate_pages_grid(&widgets.pages_box, self, &sender);
                            update_pages_heading(widgets, self);
                            if self.left_panel_open && self.left_tab == ComicLeftTab::Pages {
                                self.trigger_thumbnail_loads(&sender);
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
                        }
                    }
                }
            }
            ComicsReaderMsg::OpenLeftPanel => {
                let was_open = self.left_panel_open;
                self.left_panel_open = true;
                self.show_back_button = false;
                self.back_gen = self.back_gen.wrapping_add(1);
                widgets.left_revealer.set_reveal_child(true);
                if !was_open {
                    self.refresh_left_panel(widgets, &sender);
                }
            }
            ComicsReaderMsg::OpenRightPanel => {
                let was_open = self.right_panel_open;
                self.right_panel_open = true;
                widgets.right_revealer.set_reveal_child(true);
                if !was_open {
                    self.refresh_right_panel(widgets, &sender);
                }
            }
            ComicsReaderMsg::ToggleLeftPanel => {
                if self.left_panel_open {
                    self.left_panel_open = false;
                    widgets.left_revealer.set_reveal_child(false);
                } else {
                    let _ = sender.input_sender().send(ComicsReaderMsg::OpenLeftPanel);
                }
            }
            ComicsReaderMsg::ToggleRightPanel => {
                if self.right_panel_open {
                    self.right_panel_open = false;
                    widgets.right_revealer.set_reveal_child(false);
                } else {
                    let _ = sender.input_sender().send(ComicsReaderMsg::OpenRightPanel);
                }
            }
            ComicsReaderMsg::ClosePanels => {
                if self.left_panel_open || self.right_panel_open {
                    self.left_panel_open = false;
                    self.right_panel_open = false;
                    widgets.left_revealer.set_reveal_child(false);
                    widgets.right_revealer.set_reveal_child(false);
                }
            }
            ComicsReaderMsg::SetLeftTab(tab) => {
                self.left_tab = tab;
                self.refresh_left_panel(widgets, &sender);
            }
            ComicsReaderMsg::SetRightTab(tab) => {
                self.right_tab = tab;
                self.refresh_right_panel(widgets, &sender);
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
                    } else {
                        let fract = self.progress_fraction();
                        let title = format!("Page {}", self.current_page + 1);
                        let _ = catalog.insert_reading_bookmark(bid, page, fract, &title);
                    }
                    self.reload_bookmarks();
                    self.update_bookmark_icon_state(widgets);
                }
            }
            ComicsReaderMsg::ShowBackChrome => {
                self.back_hover = true;
                self.back_gen = self.back_gen.wrapping_add(1);
                self.show_back_button = true;
            }
            ComicsReaderMsg::BackZoneHover(hovering) => {
                if !hovering && !self.back_hover {
                    // Already accounted away (a focus reveal never
                    // entered the zone) — nothing to schedule.
                    return;
                }
                self.back_hover = hovering;
                self.back_gen = self.back_gen.wrapping_add(1);
                let gen = self.back_gen;
                let tx = sender.input_sender().clone();
                if hovering {
                    // The 250 ms rest before the unit drops in.
                    glib::timeout_add_local_once(Duration::from_millis(250), move || {
                        let _ = tx.send(ComicsReaderMsg::BackRevealDue(gen));
                    });
                } else {
                    if let Some(unit) = &self.back_unit {
                        unit.hide();
                    }
                    // The 300 ms grace, then the whole unit goes.
                    glib::timeout_add_local_once(Duration::from_millis(300), move || {
                        let _ = tx.send(ComicsReaderMsg::BackHideDue(gen));
                    });
                }
            }
            ComicsReaderMsg::BackRevealDue(gen) => {
                if gen == self.back_gen && self.back_hover && !self.left_panel_open {
                    self.show_back_button = true;
                }
            }
            ComicsReaderMsg::BackHideDue(gen) => {
                if gen == self.back_gen && !self.back_hover {
                    self.show_back_button = false;
                }
            }
            ComicsReaderMsg::OpenJump(text) => {
                self.jump_open = true;
                self.jump_text = text.unwrap_or_else(|| self.jump_current().to_string());
                widgets.jump_entry.set_text(&self.jump_text);
                widgets.jump_entry.grab_focus();
                widgets.jump_entry.select_region(0, -1);
            }
            ComicsReaderMsg::CloseJump => {
                self.jump_open = false;
                root.grab_focus();
            }
            ComicsReaderMsg::SetJumpMode(mode) => {
                if self.jump_mode != mode {
                    self.jump_mode = mode;
                    self.jump_text = self.jump_current().to_string();
                    widgets.jump_entry.set_text(&self.jump_text);
                    widgets.jump_entry.select_region(0, -1);
                }
                if let Some(control) = &self.jump_mode_control {
                    control.set_selected(match mode {
                        JumpMode::Page => 0,
                        JumpMode::Chapter => 1,
                    });
                }
            }
            ComicsReaderMsg::JumpTextChanged(text) => {
                self.jump_text = text;
                if widgets.jump_entry.text().as_str() != self.jump_text {
                    widgets.jump_entry.set_text(&self.jump_text);
                }
            }
            ComicsReaderMsg::SubmitJump => {
                if self.jump_out_of_range() {
                    return;
                }
                let Ok(number) = self.jump_text.trim().parse::<usize>() else {
                    return;
                };
                if number == 0 || number > self.jump_max() {
                    return;
                }
                self.jump_open = false;
                root.grab_focus();
                self.left_panel_open = false;
                self.right_panel_open = false;
                widgets.left_revealer.set_reveal_child(false);
                widgets.right_revealer.set_reveal_child(false);
                match self.jump_mode {
                    JumpMode::Page => {
                        if let Some((chapter_idx, page)) = self.series_page_target(number - 1) {
                            if Some(chapter_idx) == self.current_chapter_idx {
                                let _ = sender.input_sender().send(ComicsReaderMsg::SetPage(page));
                            } else if let Some(book) = self.series_chapters.get(chapter_idx) {
                                let _ = sender.input_sender().send(
                                    ComicsReaderMsg::SwitchChapterAt(book.id, Some(page)),
                                );
                            }
                        } else {
                            let _ = sender
                                .input_sender()
                                .send(ComicsReaderMsg::SetPage(number - 1));
                        }
                    }
                    JumpMode::Chapter => {
                        if let Some(book) = self.series_chapters.get(number - 1) {
                            let _ = sender.input_sender().send(
                                ComicsReaderMsg::SwitchChapterAt(book.id, Some(0)),
                            );
                        } else {
                            let _ = sender.input_sender().send(ComicsReaderMsg::SetPage(0));
                        }
                    }
                }
            }
            ComicsReaderMsg::UserScrolled => {
                self.hide_chrome_on_interaction();
            }
            ComicsReaderMsg::HideOsd(seq) => {
                if seq == self.zoom_osd_seq {
                    self.show_zoom_osd = false;
                }
            }
            ComicsReaderMsg::Escape => {
                // One layered Escape (decisions 14, 35): the jump
                // field first, then a standing panel, and only from
                // a bare page does it leave the book.
                if self.jump_open {
                    self.jump_open = false;
                    root.grab_focus();
                } else if self.left_panel_open || self.right_panel_open {
                    self.left_panel_open = false;
                    self.right_panel_open = false;
                    widgets.left_revealer.set_reveal_child(false);
                    widgets.right_revealer.set_reveal_child(false);
                } else {
                    self.cleanup_memory();
                    let _ = sender.output(ComicsReaderOut::Close);
                }
            }
            ComicsReaderMsg::Close => {
                self.cleanup_memory();
                let _ = sender.output(ComicsReaderOut::Close);
            }
            ComicsReaderMsg::MinimizeToBubble => {
                let id = self.book_id.unwrap_or(0);
                self.cleanup_memory();
                let _ = sender.output(ComicsReaderOut::MinimizeToBubble { book_id: id });
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
    fn test_panels_default_to_chapters_and_reading_closed() {
        let dummy = Arc::new(DummyProvider { count: 10 });
        let model = ComicsReaderModel::new(types::ComicsReaderInit {
            title: "Test Manga".to_string(),
            provider: dummy,
            catalog: None,
            book_id: None,
            cover_path: None,
        });
        assert_eq!(model.left_tab, ComicLeftTab::Chapters);
        assert_eq!(model.right_tab, ComicRightTab::Reading);
        assert!(!model.left_panel_open);
        assert!(!model.right_panel_open);
        // The page opens bare (decision 35).
        assert!(!model.show_back_button);
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
        assert_eq!(model.page_indicator_label(), "1 / 20");

        model.page_style = PageStyle::Double;
        assert_eq!(model.page_indicator_label(), "1–2 / 20");

        model.current_page = 19;
        assert_eq!(model.page_indicator_label(), "20 / 20");
    }

    #[test]
    fn test_series_page_offsets_location_and_jump_targets() {
        let dummy = Arc::new(DummyProvider { count: 12 });
        let mut model = ComicsReaderModel::new(types::ComicsReaderInit {
            title: "Test Series".to_string(),
            provider: dummy,
            catalog: None,
            book_id: Some(2),
            cover_path: None,
        });
        model.chapter_page_counts = vec![Some(10), Some(12)];
        let (offsets, total) = page_offsets_and_total(&model.chapter_page_counts);
        model.chapter_page_offsets = offsets;
        model.series_total_pages = total;
        model.current_chapter_idx = Some(1);
        model.page_style = PageStyle::Single;
        model.current_page = 3;

        assert_eq!(model.series_total_pages, Some(22));
        assert_eq!(model.page_indicator_label(), "14 / 22");
        model.page_style = PageStyle::Double;
        assert_eq!(model.page_indicator_label(), "14–15 / 22");
        assert_eq!(model.series_page_target(13), Some((1, 3)));
        assert_eq!(model.series_page_target(9), Some((0, 9)));
        assert_eq!(model.series_page_target(22), None);
        assert_eq!(model.jump_max(), 22);
        assert_eq!(model.jump_current(), 14);
    }

    #[test]
    fn test_page_offsets_poisoned_by_an_unknown_count() {
        let (offsets, total) = page_offsets_and_total(&[Some(10), None, Some(5)]);
        assert_eq!(offsets, vec![Some(0), Some(10), None]);
        assert_eq!(total, None);
    }

    #[test]
    fn test_jump_out_of_range_state() {
        let dummy = Arc::new(DummyProvider { count: 20 });
        let mut model = ComicsReaderModel::new(types::ComicsReaderInit {
            title: "Test Jump".to_string(),
            provider: dummy,
            catalog: None,
            book_id: None,
            cover_path: None,
        });
        model.jump_text = String::new();
        assert!(!model.jump_out_of_range());
        assert_eq!(model.jump_hint(), "of 20");

        model.jump_text = "240".to_string();
        assert!(model.jump_out_of_range());
        assert_eq!(model.jump_hint(), "Pages 1–20");

        model.jump_text = "0".to_string();
        assert!(model.jump_out_of_range());

        model.jump_text = "7".to_string();
        assert!(!model.jump_out_of_range());
        assert_eq!(model.jump_hint(), "of 20");
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
        assert_eq!(model.page_indicator_label(), "1 / 10");
        assert!((model.progress_fraction() - 0.1).abs() < 1e-4);

        model.next_page();
        assert_eq!(model.current_page, 1);
        assert_eq!(model.page_indicator_label(), "2 / 10");
        assert!((model.progress_fraction() - 0.2).abs() < 1e-4);

        model.set_page(9);
        assert_eq!(model.current_page, 9);
        assert_eq!(model.page_indicator_label(), "10 / 10");
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
        // The location stays a plain page position even on the
        // chapter-end card (decision 35); the card itself carries
        // the "completed" news.
        assert_eq!(model.page_indicator_label(), "5 / 5");
    }

}
