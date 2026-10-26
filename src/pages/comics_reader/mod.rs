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
pub mod reading_settings;
pub mod image_processing;
pub mod page_picture;
pub mod pages_grid;

pub use types::*;

use reading_settings::ComicReadingSettings;

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

/// The live widgets for one saved-preset row. The row's settings ride
/// along so the ordinary settings sync can keep the “Current” marker
/// truthful after any individual control is nudged.
struct PresetRowWidgets {
    name: String,
    settings: ComicReadingSettings,
    apply_button: gtk::Button,
    current_label: gtk::Label,
}

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
    pub strip_gap: bool,
    /// Zoom as a percentage of the selected fit's size; 100 is the
    /// fit itself. `pre_zoom_fit` is the fit a double-click to
    /// original size came from, so the next one can go back.
    pub zoom_percent: u32,
    pub pre_zoom_fit: Option<FitMode>,
    pub auto_next_chapter: bool,
    pub mark_read_on_advance: bool,
    /// The Image pane's effective choices (decision 41), carried in
    /// the same panel-wide settings record as the Reading fields.
    pub image_settings: ComicImageSettings,
    /// The per-series reading-settings record (decisions 40/147):
    /// where it lives, and whether this series currently keeps one.
    /// The fields above are always the effective choices, wherever
    /// they came from.
    pub series_id: Option<i64>,
    pub series_settings_key: Option<String>,
    pub series_override_on: bool,
    pub set_series_override_switch: Option<Rc<dyn Fn(bool)>>,
    pub set_strip_gap_switch: Option<Rc<dyn Fn(bool)>>,
    pub strip_gap_row: Option<gtk::Box>,
    pub set_auto_next_switch: Option<Rc<dyn Fn(bool)>>,
    pub set_mark_read_switch: Option<Rc<dyn Fn(bool)>>,
    pub zoom_slider: Option<crate::widgets::setting_row::SliderRow>,
    pub use_defaults_btn: Option<gtk::Button>,
    /// Named whole-panel setups (`reader.comic.presets.v1`). They are
    /// loaded once when the reader opens and rebuilt only when the list
    /// itself changes; the Current marker is then kept in step by the
    /// normal settings sync.
    pub saved_presets: Vec<reading_settings::ComicSettingsPreset>,
    pub preset_sender: Option<relm4::Sender<ComicsReaderMsg>>,
    pub preset_list_host: Option<gtk::Box>,
    pub preset_empty_label: Option<gtk::Label>,
    pub preset_rows: Vec<PresetRowWidgets>,
    pub preset_form: Option<gtk::Box>,
    pub preset_name_entry: Option<gtk::Entry>,
    pub preset_form_note: Option<gtk::Label>,
    pub preset_save_button: Option<gtk::Button>,
    pub preset_name_draft: String,
    pub preset_form_open: bool,
    pub preset_form_error: String,
    /// Image-pane handles, in `ComicBackground::ALL` order for the
    /// colour dots. The guarded setters and slider rows mirror the
    /// Reading pane's pattern.
    pub image_background_buttons: Vec<gtk::Button>,
    pub brightness_slider: Option<crate::widgets::setting_row::SliderRow>,
    pub contrast_slider: Option<crate::widgets::setting_row::SliderRow>,
    pub warmth_slider: Option<crate::widgets::setting_row::SliderRow>,
    pub set_grayscale_switch: Option<Rc<dyn Fn(bool)>>,
    pub set_invert_switch: Option<Rc<dyn Fn(bool)>>,
    pub set_crop_switch: Option<Rc<dyn Fn(bool)>>,
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
    /// Cached blank-border analysis for the current chapter, keyed
    /// like `textures`. The rectangles are tiny and deliberately
    /// outlive the bounded full-size texture window: returning to a
    /// page should not analyse it a second time in this chapter.
    pub page_crop_boxes: HashMap<usize, CropRect>,
    pub pending_loads: HashSet<usize>,
    pub session_id: Option<i64>,
    pub session_start: std::time::Instant,
    pub last_viewport_width: i32,
    pub last_viewport_height: i32,
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
    /// The Pages tab's windowed whole-series grid. It owns its
    /// thumbnail textures and failure states, keyed by chapter as
    /// well as page so chapter 1 page 4 and chapter 2 page 4 never
    /// share a cell or a decode.
    pub page_grid: Option<pages_grid::SeriesPagesGrid>,
    pub thumb_pending: HashSet<SeriesPageRef>,
    /// Providers opened only when a thumbnail from another chapter
    /// first scrolls into the grid. The chapter being read always
    /// uses the live `provider` above, so remote chapters keep
    /// working without a local file.
    pub thumb_providers: HashMap<usize, Arc<dyn provider::ImageProvider>>,
    /// Bumped when the thumbnail caches are emptied. A chapter switch
    /// deliberately does not bump it: series thumbnails remain true
    /// whichever chapter is open.
    pub thumb_load_gen: u64,
    /// Bumped whenever the reading provider changes, so full-page
    /// decodes finishing after a chapter switch are recognised as
    /// belonging to the chapter that was left and dropped.
    pub load_gen: u64,
}

#[allow(dead_code)]
enum DecodedPage {
    Rgba {
        width: i32,
        height: i32,
        bytes: Vec<u8>,
        /// Full reading pages carry their blank-border analysis;
        /// thumbnails reuse this transport without spending the scan.
        crop: Option<CropRect>,
    },
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

/// Auto next chapter (decision 39): a next-page ask on the last page
/// goes straight into the next chapter instead of stopping at the
/// chapter-end card.
fn should_auto_advance(auto_next: bool, is_last_page: bool, has_next_chapter: bool) -> bool {
    auto_next && is_last_page && has_next_chapter
}

/// Mark read on chapter advance (decision 39): only a forward move
/// through the series marks the chapter being left; going back to
/// re-read an earlier chapter changes nothing behind the reader.
fn should_mark_read_on_advance(
    enabled: bool,
    current_idx: Option<usize>,
    target_idx: Option<usize>,
) -> bool {
    enabled
        && matches!(
            (current_idx, target_idx),
            (Some(current), Some(target)) if target > current
        )
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

        let (series_name, series_chapters, series_chapter_meta, authors, series_id) =
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
                    (Some(series.title), books, meta, authors, Some(series.id))
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
                        (Some(s.clone()), chapters, meta, authors, None)
                    } else {
                        (None, Vec::new(), Vec::new(), authors, None)
                    }
                } else {
                    (None, Vec::new(), Vec::new(), String::new(), None)
                }
            } else {
                (None, Vec::new(), Vec::new(), String::new(), None)
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

        // Reading settings (decisions 39/40/147): the global comic
        // defaults, replaced wholesale by this series' own record
        // when it keeps one. Legacy per-book direction/style/fit
        // values seed that record on first open — the current
        // chapter's choices win, then the first chapter that has any.
        let chapter_book_ids: Vec<i64> = series_chapters.iter().map(|b| b.id).collect();
        let series_settings_key = reading_settings::series_pref_key(
            series_id,
            &chapter_book_ids,
            init.book_id,
        );
        let (settings, series_override_on) = if let Some(ref catalog) = init.catalog {
            let globals = ComicReadingSettings::load_globals(catalog);
            let mut series_settings = series_settings_key
                .as_deref()
                .and_then(|key| ComicReadingSettings::load_series(catalog, key));
            if series_settings.is_none() {
                if let Some(key) = &series_settings_key {
                    let mut candidates: Vec<i64> = Vec::new();
                    if let Some(book_id) = init.book_id {
                        candidates.push(book_id);
                    }
                    candidates.extend(
                        chapter_book_ids
                            .iter()
                            .copied()
                            .filter(|id| Some(*id) != init.book_id),
                    );
                    if let Some(migrated) = candidates.iter().find_map(|id| {
                        ComicReadingSettings::legacy_book(catalog, *id, &globals)
                    }) {
                        migrated.save_series(catalog, key);
                        for id in &candidates {
                            ComicReadingSettings::clear_legacy_book(catalog, *id);
                        }
                        series_settings = Some(migrated);
                    }
                }
            }
            let override_on = series_settings.is_some();
            (series_settings.unwrap_or(globals), override_on)
        } else {
            (ComicReadingSettings::default(), false)
        };
        let saved_presets = init
            .catalog
            .as_ref()
            .map(|catalog| reading_settings::load_presets(catalog))
            .unwrap_or_default();

        Self {
            title: init.title,
            provider: init.provider,
            catalog: init.catalog,
            book_id: init.book_id,
            cover_path: init.cover_path,
            current_page: saved_page,
            total_pages,
            direction: settings.direction,
            page_style: settings.page_style,
            fit_mode: settings.fit_mode,
            two_page_gap: settings.two_page_gap,
            strip_gap: settings.strip_gap,
            zoom_percent: settings.zoom_percent,
            pre_zoom_fit: None,
            auto_next_chapter: settings.auto_next_chapter,
            mark_read_on_advance: settings.mark_read_on_advance,
            image_settings: settings.image,
            series_id,
            series_settings_key,
            series_override_on,
            set_series_override_switch: None,
            set_strip_gap_switch: None,
            strip_gap_row: None,
            set_auto_next_switch: None,
            set_mark_read_switch: None,
            zoom_slider: None,
            use_defaults_btn: None,
            saved_presets,
            preset_sender: None,
            preset_list_host: None,
            preset_empty_label: None,
            preset_rows: Vec::new(),
            preset_form: None,
            preset_name_entry: None,
            preset_form_note: None,
            preset_save_button: None,
            preset_name_draft: String::new(),
            preset_form_open: false,
            preset_form_error: String::new(),
            image_background_buttons: Vec::new(),
            brightness_slider: None,
            contrast_slider: None,
            warmth_slider: None,
            set_grayscale_switch: None,
            set_invert_switch: None,
            set_crop_switch: None,
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
            page_crop_boxes: HashMap::new(),
            pending_loads: HashSet::new(),
            session_id,
            session_start: std::time::Instant::now(),
            last_viewport_width: 0,
            last_viewport_height: 0,
            authors,
            series_name,
            series_chapters,
            series_chapter_meta,
            current_chapter_idx,
            at_chapter_end: false,
            chapter_page_counts,
            chapter_page_offsets,
            series_total_pages,
            page_grid: None,
            thumb_pending: HashSet::new(),
            thumb_providers: HashMap::new(),
            thumb_load_gen: 0,
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

    /// The effective Reading-pane choices as one settings value.
    pub(crate) fn current_reading_settings(&self) -> ComicReadingSettings {
        ComicReadingSettings {
            direction: self.direction,
            page_style: self.page_style,
            fit_mode: self.fit_mode,
            two_page_gap: self.two_page_gap,
            strip_gap: self.strip_gap,
            zoom_percent: self.zoom_percent,
            auto_next_chapter: self.auto_next_chapter,
            mark_read_on_advance: self.mark_read_on_advance,
            image: self.image_settings,
        }
        .canonical()
    }

    /// Record a Reading-pane edit. Editing while the series follows
    /// the defaults turns its own record on (decision 147); the
    /// reader never writes the global comic prefs from an edit —
    /// only "Use these settings as my defaults" does that.
    pub fn store_series_setting(&mut self) {
        let settings = self.current_reading_settings();
        // Keep the live fields canonical too (a stored Webtoon
        // direction becomes Long strip, zooms clamp).
        self.direction = settings.direction;
        self.page_style = settings.page_style;
        self.fit_mode = settings.fit_mode;
        self.two_page_gap = settings.two_page_gap;
        self.zoom_percent = settings.zoom_percent;
        self.image_settings = settings.image;
        if let (Some(ref catalog), Some(key)) = (&self.catalog, &self.series_settings_key) {
            settings.save_series(catalog, key);
        }
        self.series_override_on = true;
    }

    /// Take a settings value wholesale into the live fields. The
    /// caller rebuilds the viewport and syncs the pane.
    pub(crate) fn apply_reading_settings(&mut self, settings: &ComicReadingSettings) {
        let settings = settings.clone().canonical();
        self.direction = settings.direction;
        self.page_style = settings.page_style;
        self.fit_mode = settings.fit_mode;
        self.two_page_gap = settings.two_page_gap;
        self.strip_gap = settings.strip_gap;
        self.zoom_percent = settings.zoom_percent;
        self.pre_zoom_fit = None;
        self.auto_next_chapter = settings.auto_next_chapter;
        self.mark_read_on_advance = settings.mark_read_on_advance;
        self.image_settings = settings.image;
    }

    fn active_preset_name(&self) -> Option<&str> {
        let current = self.current_reading_settings();
        self.saved_presets
            .iter()
            .find(|preset| preset.settings == current)
            .map(|preset| preset.name.as_str())
    }

    fn persist_presets(&self) {
        if let Some(catalog) = &self.catalog {
            reading_settings::store_presets(catalog, &self.saved_presets);
        }
    }

    fn rebuild_preset_rows(&mut self) {
        let (Some(host), Some(sender)) = (&self.preset_list_host, &self.preset_sender)
        else {
            return;
        };
        while let Some(child) = host.first_child() {
            host.remove(&child);
        }
        self.preset_rows.clear();

        for preset in self.saved_presets.clone() {
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
            row.add_css_class("kalam-comics-preset-row");

            let apply = gtk::Button::new();
            apply.add_css_class("kalam-comics-preset-apply");
            apply.set_hexpand(true);
            apply.set_halign(gtk::Align::Fill);
            apply.set_tooltip_text(Some(&format!("Apply “{}”", preset.name)));
            apply.update_property(&[gtk::accessible::Property::Label(
                &format!("Apply preset {}", preset.name),
            )]);

            let apply_content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let text = gtk::Box::new(gtk::Orientation::Vertical, 1);
            text.set_hexpand(true);
            let name = gtk::Label::new(Some(&preset.name));
            name.add_css_class("kalam-comics-preset-name");
            name.set_halign(gtk::Align::Start);
            name.set_xalign(0.0);
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            text.append(&name);
            let summary = gtk::Label::new(Some(&preset.summary()));
            summary.add_css_class("kalam-comics-preset-summary");
            summary.set_halign(gtk::Align::Start);
            summary.set_xalign(0.0);
            summary.set_ellipsize(gtk::pango::EllipsizeMode::End);
            text.append(&summary);
            apply_content.append(&text);

            let current = gtk::Label::new(Some("Current"));
            current.add_css_class("kalam-comics-preset-current");
            current.set_valign(gtk::Align::Center);
            current.set_visible(false);
            apply_content.append(&current);
            apply.set_child(Some(&apply_content));

            {
                let tx = sender.clone();
                let name = preset.name.clone();
                apply.connect_clicked(move |_| {
                    let _ = tx.send(ComicsReaderMsg::ApplyPreset(name.clone()));
                });
            }
            row.append(&apply);

            let delete = gtk::Button::from_icon_name("user-trash-symbolic");
            delete.add_css_class("kalam-btn-icon");
            delete.add_css_class("danger");
            delete.set_valign(gtk::Align::Center);
            delete.set_tooltip_text(Some(&format!("Delete “{}”", preset.name)));
            delete.update_property(&[gtk::accessible::Property::Label(
                &format!("Delete preset {}", preset.name),
            )]);
            {
                let tx = sender.clone();
                let name = preset.name.clone();
                delete.connect_clicked(move |_| {
                    let _ = tx.send(ComicsReaderMsg::DeletePreset(name.clone()));
                });
            }
            row.append(&delete);
            host.append(&row);

            self.preset_rows.push(PresetRowWidgets {
                name: preset.name,
                settings: preset.settings,
                apply_button: apply,
                current_label: current,
            });
        }

        if let Some(empty) = &self.preset_empty_label {
            empty.set_visible(self.saved_presets.is_empty());
        }
    }

    fn sync_preset_ui(&self) {
        let current = self.current_reading_settings();
        for row in &self.preset_rows {
            let active = row.settings == current;
            toggle_active(&row.apply_button, active);
            row.current_label.set_visible(active);
        }
        if let Some(empty) = &self.preset_empty_label {
            empty.set_visible(self.saved_presets.is_empty());
        }
        if let Some(form) = &self.preset_form {
            form.set_visible(self.preset_form_open);
        }

        let draft = reading_settings::normalise_preset_name(&self.preset_name_draft);
        let replacing = (!draft.is_empty()).then(|| {
            self.saved_presets
                .iter()
                .find(|preset| preset.name.to_lowercase() == draft.to_lowercase())
        });
        let replacing = replacing.flatten();
        if let Some(note) = &self.preset_form_note {
            if !self.preset_form_error.is_empty() {
                note.set_label(&self.preset_form_error);
                note.add_css_class("kalam-comics-preset-error");
            } else if let Some(existing) = replacing {
                note.set_label(&format!(
                    "This will replace the existing “{}” preset.",
                    existing.name
                ));
                note.remove_css_class("kalam-comics-preset-error");
            } else {
                note.set_label("Presets keep Reading and Image settings together.");
                note.remove_css_class("kalam-comics-preset-error");
            }
        }
        if let Some(button) = &self.preset_save_button {
            let valid = !draft.is_empty()
                && draft.chars().count() <= reading_settings::MAX_PRESET_NAME_CHARS;
            button.set_sensitive(valid);
            button.set_label(if replacing.is_some() {
                "Replace preset"
            } else {
                "Save preset"
            });
        }
    }

    pub fn settings_differ_from_defaults(&self) -> bool {
        let globals = match &self.catalog {
            Some(catalog) => ComicReadingSettings::load_globals(catalog),
            None => ComicReadingSettings::default(),
        };
        self.current_reading_settings() != globals
    }

    /// The crop to paint for one page. Analysis is cached whether or
    /// not the switch is on, so turning Crop on never waits for a
    /// second scan of a page this chapter has already decoded.
    pub(crate) fn effective_crop(&self, page_idx: usize) -> CropRect {
        if self.image_settings.crop_blank_borders {
            self.page_crop_boxes
                .get(&page_idx)
                .copied()
                .unwrap_or(CropRect::FULL)
        } else {
            CropRect::FULL
        }
    }

    pub(crate) fn effective_page_size(
        &self,
        page_idx: usize,
        texture: &gdk::Texture,
    ) -> (i32, i32) {
        self.effective_crop(page_idx)
            .pixel_size(texture.width(), texture.height())
    }

    /// Mark the chapter being left read, at its last page, whatever
    /// page the reader happened to advance from (a spread's second
    /// page has been read too).
    pub fn mark_current_chapter_read(&self) {
        if self.total_pages == 0 {
            return;
        }
        if let (Some(ref catalog), Some(book_id)) = (&self.catalog, self.book_id) {
            let _ = catalog.set_reading_progress(
                book_id,
                self.total_pages - 1,
                1.0,
                self.total_pages,
            );
            if let Some(sid) = self.session_id {
                let elapsed = self.session_start.elapsed().as_secs() as i64;
                let _ = catalog.checkpoint_reading_session(sid, elapsed, 100);
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
            let known_crop = self.page_crop_boxes.get(&idx).copied();
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
                        // Crop's one analysis pass (spec §5) rides
                        // this existing off-main-thread decode. Its
                        // result travels back with the pixels and is
                        // cached for the rest of the chapter.
                        let crop = known_crop.unwrap_or_else(|| {
                            image_processing::analyse_blank_border_crop(
                                rgba.as_raw(),
                                rgba.width(),
                                rgba.height(),
                                rgba.width() as usize * 4,
                            )
                        });
                        Some(DecodedPage::Rgba {
                            width,
                            height,
                            bytes: rgba.into_raw(),
                            crop: Some(crop),
                        })
                    } else {
                        Some(DecodedPage::RawBytes(b))
                    }
                },
                |_| {},
                move |decoded_opt| {
                    let (tex_opt, crop) = match decoded_opt {
                        Some(DecodedPage::Rgba {
                            width,
                            height,
                            bytes,
                            crop,
                        }) => {
                            let stride = (width * 4) as usize;
                            let gbytes = glib::Bytes::from_owned(bytes);
                            let mem_tex = gdk::MemoryTexture::new(
                                width,
                                height,
                                gdk::MemoryFormat::R8g8b8a8,
                                &gbytes,
                                stride,
                            );
                            (Some(mem_tex.upcast::<gdk::Texture>()), crop)
                        }
                        Some(DecodedPage::RawBytes(b)) => (
                            gdk::Texture::from_bytes(&glib::Bytes::from(&b)).ok(),
                            None,
                        ),
                        None => (None, None),
                    };
                    let _ = s.send(types::ComicsReaderMsg::PageLoaded(
                        gen, idx, tex_opt, crop,
                    ));
                },
            );
        }
    }

    /// The page being read, in the whole-series grid's own terms.
    pub fn current_series_page_ref(&self) -> SeriesPageRef {
        SeriesPageRef::new(self.current_chapter_idx, self.current_page)
    }

    /// Every countable page in the series, in grid order. Page
    /// numbers restart at each chapter because the grid numbers —
    /// and its thumbnail jumps — are chapter-local (decision 38).
    pub fn page_grid_entries(&self) -> Vec<pages_grid::SeriesPageEntry> {
        let book_ids: Vec<i64> = self.series_chapters.iter().map(|book| book.id).collect();
        pages_grid::build_series_page_entries(
            &book_ids,
            &self.chapter_page_counts,
            self.current_chapter_idx,
            self.total_pages,
        )
    }

    pub fn sync_page_grid_entries(&self) {
        if let Some(grid) = &self.page_grid {
            grid.set_entries(
                self.page_grid_entries(),
                Some(self.current_series_page_ref()),
            );
        }
    }

    fn thumbnail_provider_for(
        &mut self,
        page: SeriesPageRef,
    ) -> Option<Arc<dyn provider::ImageProvider>> {
        match page.chapter_idx {
            None => Some(self.provider.clone()),
            Some(chapter_idx) if Some(chapter_idx) == self.current_chapter_idx => {
                Some(self.provider.clone())
            }
            Some(chapter_idx) => {
                if let Some(provider) = self.thumb_providers.get(&chapter_idx) {
                    return Some(provider.clone());
                }
                let book = self.series_chapters.get(chapter_idx)?;
                let provider: Arc<dyn provider::ImageProvider> = Arc::new(
                    providers::LocalProvider::new(book.file_path.clone()).ok()?,
                );
                self.thumb_providers.insert(chapter_idx, provider.clone());
                Some(provider)
            }
        }
    }

    /// Decode one small copy when the windowed Pages grid binds its
    /// cell. Separate from [`Self::trigger_loads`]: the reading
    /// window keeps only a few full-size textures alive, while the
    /// grid's provider may be any chapter in the series.
    pub fn request_page_thumbnail(
        &mut self,
        page: SeriesPageRef,
        sender: &relm4::ComponentSender<Self>,
    ) {
        let Some(grid) = self.page_grid.clone() else {
            return;
        };
        if grid.has_thumbnail_state(page) || !self.thumb_pending.insert(page) {
            return;
        }

        let Some(provider) = self.thumbnail_provider_for(page) else {
            self.thumb_pending.remove(&page);
            grid.update_thumbnail(page, None);
            return;
        };

        let gen = self.thumb_load_gen;
        let idx = page.page_idx;
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Loading comic thumbnail",
            move |_| -> Option<DecodedPage> {
                let b = provider.fetch_page(idx).ok()?;
                let img = image::load_from_memory(&b).ok()?;
                let rgba = img.thumbnail(240, 360).to_rgba8();
                let width = rgba.width() as i32;
                let height = rgba.height() as i32;
                Some(DecodedPage::Rgba {
                    width,
                    height,
                    bytes: rgba.into_raw(),
                    crop: None,
                })
            },
            |_| {},
            move |decoded_opt| {
                let tex_opt = match decoded_opt {
                    Some(DecodedPage::Rgba {
                        width,
                        height,
                        bytes,
                        ..
                    }) => {
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
                let _ = s.send(types::ComicsReaderMsg::ThumbnailLoaded(gen, page, tex_opt));
            },
        );
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
        // The series override and the switches/slider built in init
        // (decision 147): the guarded setters keep a sync from
        // echoing back as a user edit.
        if let Some(set) = &self.set_series_override_switch {
            set(self.series_override_on);
        }
        if let Some(set) = &self.set_strip_gap_switch {
            set(self.strip_gap);
        }
        if let Some(row) = &self.strip_gap_row {
            row.set_visible(
                self.page_style == PageStyle::LongStrip
                    || self.direction == ReadingDirection::Webtoon,
            );
        }
        if let Some(set) = &self.set_auto_next_switch {
            set(self.auto_next_chapter);
        }
        if let Some(set) = &self.set_mark_read_switch {
            set(self.mark_read_on_advance);
        }
        if let Some(slider) = &self.zoom_slider {
            let scale = slider.scale();
            if (scale.value() - self.zoom_percent as f64).abs() > f64::EPSILON {
                scale.set_value(self.zoom_percent as f64);
            }
        }
        if let Some(btn) = &self.use_defaults_btn {
            let differs = self.settings_differ_from_defaults();
            btn.set_sensitive(differs);
            btn.set_label(if differs {
                "Use these settings as my defaults"
            } else {
                "Matches your defaults"
            });
        }
        self.sync_preset_ui();

        // The Image pane and the stage it paints (decision 41).
        for class in ["comic-bg-black", "comic-bg-grey", "comic-bg-white"] {
            widgets.viewport_box.remove_css_class(class);
        }
        match self.image_settings.background {
            ComicBackground::Theme => {}
            ComicBackground::Black => widgets.viewport_box.add_css_class("comic-bg-black"),
            ComicBackground::Grey => widgets.viewport_box.add_css_class("comic-bg-grey"),
            ComicBackground::White => widgets.viewport_box.add_css_class("comic-bg-white"),
        }
        for (idx, button) in self.image_background_buttons.iter().enumerate() {
            toggle_active(
                button,
                ComicBackground::ALL.get(idx).copied()
                    == Some(self.image_settings.background),
            );
        }
        for (slider, value) in [
            (&self.brightness_slider, self.image_settings.brightness),
            (&self.contrast_slider, self.image_settings.contrast),
            (&self.warmth_slider, self.image_settings.warmth),
        ] {
            if let Some(slider) = slider {
                let scale = slider.scale();
                if (scale.value() - value as f64).abs() > f64::EPSILON {
                    scale.set_value(value as f64);
                }
            }
        }
        if let Some(set) = &self.set_grayscale_switch {
            set(self.image_settings.grayscale);
        }
        if let Some(set) = &self.set_invert_switch {
            set(self.image_settings.invert_colours);
        }
        if let Some(set) = &self.set_crop_switch {
            set(self.image_settings.crop_blank_borders);
        }

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
                self.sync_page_grid_entries();
                sync_pages_current(self);
                widgets.pages_empty_label.set_visible(
                    self.page_grid
                        .as_ref()
                        .is_some_and(|grid| grid.entry_count() == 0),
                );
                // Open scrolled to the current page, wherever in
                // the series it sits. GridView first makes GTK
                // realise that window; the idle adjustment then puts
                // one row of context above it, using the fixed
                // 112 + 12 cell pitch.
                if let Some(grid) = &self.page_grid {
                    grid.scroll_to_current();
                    let grid = grid.clone();
                    let vadj = widgets.pages_scroll.vadjustment();
                    glib::idle_add_local_once(move || {
                        let max = (vadj.upper() - vadj.page_size()).max(0.0);
                        if let Some(target) =
                            grid.opening_scroll_target(max, vadj.page_size())
                        {
                            vadj.set_value(target);
                        }
                    });
                }
                update_pages_heading(widgets, self);
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
                ComicRightTab::Image => 1,
                ComicRightTab::Shortcuts => 2,
            });
        }
        widgets.right_stack.set_visible_child_name(match self.right_tab {
            ComicRightTab::Reading => "reading",
            ComicRightTab::Image => "image",
            ComicRightTab::Shortcuts => "shortcuts",
        });
        self.sync_settings_ui(widgets);
    }

    pub fn cleanup_memory(&mut self) {
        self.textures.clear();
        self.page_crop_boxes.clear();
        self.pending_loads.clear();
        self.thumb_pending.clear();
        self.thumb_providers.clear();
        if let Some(grid) = &self.page_grid {
            grid.clear_thumbnails();
        }
        self.thumb_load_gen = self.thumb_load_gen.wrapping_add(1);
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

/// The reader's zoom as a plain multiplier of the selected fit.
fn zoom_factor(model: &ComicsReaderModel) -> f64 {
    model.zoom_percent.clamp(
        reading_settings::MIN_ZOOM_PERCENT,
        reading_settings::MAX_ZOOM_PERCENT,
    ) as f64
        / 100.0
}

/// A texture's size at an absolute scale, never below a pixel.
fn zoomed_size(texture_w: i32, texture_h: i32, scale: f64) -> (i32, i32) {
    (
        ((texture_w as f64) * scale).round().max(1.0) as i32,
        ((texture_h as f64) * scale).round().max(1.0) as i32,
    )
}

fn update_viewport_policies(
    scrolled_window: &gtk::ScrolledWindow,
    fit_mode: FitMode,
    is_webtoon: bool,
    zoom_percent: u32,
) {
    if is_webtoon {
        // A zoomed-in strip is wider than the window; let it pan.
        scrolled_window.set_hscrollbar_policy(if zoom_percent > 100 {
            gtk::PolicyType::Automatic
        } else {
            gtk::PolicyType::Never
        });
        scrolled_window.set_vscrollbar_policy(gtk::PolicyType::Always);
    } else if zoom_percent != 100 {
        // Zoomed away from the fit, the page can overflow either
        // axis; both directions pan (decision 42).
        scrolled_window.set_hscrollbar_policy(gtk::PolicyType::Automatic);
        scrolled_window.set_vscrollbar_policy(gtk::PolicyType::Automatic);
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
    pic: page_picture::ComicPagePicture,
    target_w: i32,
    target_h: i32,
    _sender: Option<&ComponentSender<ComicsReaderModel>>,
) -> gtk::Widget {
    if target_w > 0 && target_h > 0 {
        pic.set_size_request(target_w, target_h);
    }
    pic.upcast()
}

/// Push new colour settings into every page already on the stage.
/// Recursing the live tree is what lets a slider be a repaint rather
/// than a viewport rebuild, so Long strip keeps its scroll position
/// while the reader is still dragging.
fn update_page_pictures(widget: &gtk::Widget, settings: ComicImageSettings) {
    if let Ok(picture) = widget.clone().downcast::<page_picture::ComicPagePicture>() {
        picture.set_image_settings(settings);
    }
    let mut child = widget.first_child();
    while let Some(current) = child {
        update_page_pictures(&current, settings);
        child = current.next_sibling();
    }
}

fn rebuild_viewport_widget(
    model: &ComicsReaderModel,
    viewport_width: i32,
    viewport_height: i32,
    sender: Option<&ComponentSender<ComicsReaderModel>>,
) -> gtk::Widget {
    let win_w = if viewport_width > 100 { viewport_width } else { 850 };
    let win_h = if viewport_height > 100 { viewport_height } else { 700 };
    let zoom = zoom_factor(model);
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
        let avail_w = (((win_w - 24).max(300) as f64) * zoom).round() as i32;
        // "Gap between pages" (decision 39): the strip breathes, or
        // runs as one continuous column.
        let vbox = gtk::Box::new(
            gtk::Orientation::Vertical,
            if model.strip_gap { 16 } else { 0 },
        );
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
                let (tw, th) = model.effective_page_size(idx, texture);
                let target_h = if tw > 0 {
                    ((avail_w as f64) * (th as f64 / tw as f64)).round() as i32
                } else {
                    -1
                };

                let pic = page_picture::ComicPagePicture::new(
                    texture,
                    model.image_settings,
                    model.effective_crop(idx),
                );
                pic.set_size_request(avail_w, target_h);
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
                    let (tw, th) = model.effective_page_size(idx, texture);
                    let pic = page_picture::ComicPagePicture::new(
                        texture,
                        model.image_settings,
                        model.effective_crop(idx),
                    );
                    pic.set_halign(if is_left { gtk::Align::End } else { gtk::Align::Start });

                    let (target_w, target_h) = match model.fit_mode {
                        FitMode::Screen => {
                            if (zoom - 1.0).abs() > f64::EPSILON && tw > 0 && th > 0 {
                                let scale = (((win_w - 32 - model.two_page_gap).max(400) / 2) as f64 / tw as f64)
                                    .min(win_h as f64 / th as f64)
                                    * zoom;
                                let (target_w, target_h) = zoomed_size(tw, th, scale);
                                pic.set_size_request(target_w, target_h);
                                pic.set_valign(gtk::Align::Center);
                                (target_w, target_h)
                            } else {
                                pic.set_hexpand(true);
                                pic.set_vexpand(true);
                                pic.set_valign(gtk::Align::Center);
                                pic.set_size_request(-1, -1);
                                (-1, -1)
                            }
                        }
                        FitMode::Height => {
                            if (zoom - 1.0).abs() > f64::EPSILON && tw > 0 && th > 0 {
                                let (target_w, target_h) =
                                    zoomed_size(tw, th, win_h as f64 / th as f64 * zoom);
                                pic.set_size_request(target_w, target_h);
                                pic.set_valign(gtk::Align::Center);
                                (target_w, target_h)
                            } else {
                                pic.set_hexpand(false);
                                pic.set_vexpand(true);
                                pic.set_valign(gtk::Align::Center);
                                pic.set_size_request(-1, -1);
                                (-1, -1)
                            }
                        }
                        FitMode::Width => {
                            let avail_w = ((((win_w - 32 - model.two_page_gap).max(400) / 2) as f64) * zoom).round() as i32;
                            let target_h = if tw > 0 {
                                ((avail_w as f64) * (th as f64 / tw as f64)).round() as i32
                            } else {
                                -1
                            };
                            pic.set_size_request(avail_w, target_h);
                            pic.set_valign(gtk::Align::Start);
                            (avail_w, target_h)
                        }
                        FitMode::Original => {
                            let (target_w, target_h) = zoomed_size(tw, th, zoom);
                            pic.set_size_request(target_w, target_h);
                            pic.set_valign(gtk::Align::Center);
                            (target_w, target_h)
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
        let (tw, th) = model.effective_page_size(model.current_page, texture);
        let pic = page_picture::ComicPagePicture::new(
            texture,
            model.image_settings,
            model.effective_crop(model.current_page),
        );
        pic.set_halign(gtk::Align::Center);

        let (target_w, target_h) = match model.fit_mode {
            FitMode::Screen => {
                if (zoom - 1.0).abs() > f64::EPSILON && tw > 0 && th > 0 {
                    let scale = ((win_w - 24).max(300) as f64 / tw as f64)
                        .min(win_h as f64 / th as f64)
                        * zoom;
                    let (target_w, target_h) = zoomed_size(tw, th, scale);
                    pic.set_size_request(target_w, target_h);
                    pic.set_valign(gtk::Align::Center);
                    (target_w, target_h)
                } else {
                    pic.set_hexpand(true);
                    pic.set_vexpand(true);
                    pic.set_valign(gtk::Align::Center);
                    pic.set_size_request(-1, -1);
                    (-1, -1)
                }
            }
            FitMode::Height => {
                if (zoom - 1.0).abs() > f64::EPSILON && tw > 0 && th > 0 {
                    let (target_w, target_h) =
                        zoomed_size(tw, th, win_h as f64 / th as f64 * zoom);
                    pic.set_size_request(target_w, target_h);
                    pic.set_valign(gtk::Align::Center);
                    (target_w, target_h)
                } else {
                    pic.set_hexpand(false);
                    pic.set_vexpand(true);
                    pic.set_valign(gtk::Align::Center);
                    pic.set_size_request(-1, -1);
                    (-1, -1)
                }
            }
            FitMode::Width => {
                let avail_w = (((win_w - 24).max(300) as f64) * zoom).round() as i32;
                let target_h = if tw > 0 {
                    ((avail_w as f64) * (th as f64 / tw as f64)).round() as i32
                } else {
                    -1
                };
                pic.set_size_request(avail_w, target_h);
                pic.set_valign(gtk::Align::Start);
                (avail_w, target_h)
            }
            FitMode::Original => {
                let (target_w, target_h) = zoomed_size(tw, th, zoom);
                pic.set_size_request(target_w, target_h);
                pic.set_valign(gtk::Align::Center);
                (target_w, target_h)
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

/// The Pages tab's sticky heading: the chapter on show at the top
/// of the grid, its page count and reading state (`36 pages ·
/// reading`, `· read` once finished, `· not started` before it).
/// Scrolling the whole-series grid into another chapter moves this
/// heading even though the chapter being read has not changed.
fn update_pages_heading(widgets: &ComicsReaderModelWidgets, model: &ComicsReaderModel) {
    let visible_chapter_idx = model
        .page_grid
        .as_ref()
        .and_then(|grid| {
            grid.first_visible_page(widgets.pages_scroll.vadjustment().value())
        })
        .map(|page| page.chapter_idx)
        .unwrap_or(model.current_chapter_idx);

    let (chapter, page_count, state) = if let Some((idx, book)) = visible_chapter_idx
        .and_then(|idx| model.series_chapters.get(idx).map(|book| (idx, book)))
    {
        // The heading spells the word out (`CHAPTER 2`, in the
        // section label's uppercase) where the chapter rows
        // abbreviate it to `Ch. 2`.
        let number = chapter_display(model, idx, book).0;
        let chapter = match number.strip_prefix("Ch. ") {
            Some(n) => format!("Chapter {n}"),
            None => number,
        };
        let page_count = if Some(idx) == model.current_chapter_idx {
            model.total_pages
        } else {
            model
                .chapter_page_counts
                .get(idx)
                .copied()
                .flatten()
                .unwrap_or(0)
        };
        let state = if Some(idx) == model.current_chapter_idx {
            if model.total_pages > 0 && model.current_page + 1 >= model.total_pages {
                "read"
            } else {
                "reading"
            }
        } else if book.progress >= 100 {
            "read"
        } else if book.progress > 0 {
            "reading"
        } else {
            "not started"
        };
        (chapter, page_count, state)
    } else {
        let state = if model.total_pages > 0 && model.current_page + 1 >= model.total_pages {
            "read"
        } else {
            "reading"
        };
        ("Chapter".to_string(), model.total_pages, state)
    };

    widgets.pages_chapter_label.set_label(&chapter);
    widgets
        .pages_meta_label
        .set_label(&format!("{page_count} pages · {state}"));
}

/// Mark the page being read in a standing Pages grid, without
/// rebuilding it or moving its scroll position.
fn sync_pages_current(model: &ComicsReaderModel) {
    if let Some(grid) = &model.page_grid {
        grid.set_current(Some(model.current_series_page_ref()));
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
            ("Zoom in", &["+"][..]),
            ("Zoom out", &["−"][..]),
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

/// A Reading-pane switch row plus the silent setter that keeps it in
/// step with the model. The guard is what stops a model sync (an
/// edit below turning the series record on, a reset turning it off)
/// from arriving back here dressed as another user flip.
fn guarded_switch_row(
    label: &str,
    initial: bool,
    sender: &ComponentSender<ComicsReaderModel>,
    make_msg: Rc<dyn Fn(bool) -> ComicsReaderMsg>,
) -> (
    crate::widgets::setting_row::SwitchRow,
    Rc<dyn Fn(bool)>,
) {
    let guard = Rc::new(std::cell::Cell::new(false));
    let tx = sender.input_sender().clone();
    let guard_cb = guard.clone();
    let row = crate::widgets::setting_row::switch_row(label, None, initial, move |on| {
        if !guard_cb.get() {
            let _ = tx.send(make_msg(on));
        }
    });
    let switch = row.switch();
    let set: Rc<dyn Fn(bool)> = Rc::new(move |on| {
        guard.set(true);
        switch.set_active(on);
        guard.set(false);
    });
    (row, set)
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
                            // Fill + xalign, not Start: an ellipsizing
                            // label aligned Start is allocated its
                            // minimum (one glyph and the ellipsis).
                            set_xalign: 0.0,
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

                                // "Settings for this series"
                                // (decision 147), built in init.
                                #[name = "series_settings_host"]
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_margin_start: 16,
                                    set_margin_end: 16,
                                },

                                // Saved presets sit above the
                                // individual controls because one
                                // preset replaces the whole record
                                // those controls edit. Init fills this
                                // host with the current list and the
                                // inline save form.
                                append = &reader_settings_section("Saved presets"),
                                #[name = "presets_host"]
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_spacing: 8,
                                    set_margin_start: 16,
                                    set_margin_end: 16,
                                },

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

                                // "Gap between pages" for Long
                                // strip (decision 39), built in init
                                // and only shown in that style.
                                #[name = "strip_gap_host"]
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_margin_start: 16,
                                    set_margin_end: 16,
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



                                // Section: DIRECTION AND ZOOM
                                append = &reader_settings_section("Direction and zoom"),
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_spacing: 0,
                                    set_margin_start: 16,
                                    set_margin_end: 16,
                                    add_css_class: "kalam-comics-inset-card",

                                    gtk::Box {
                                        set_orientation: gtk::Orientation::Horizontal,
                                        set_spacing: 8,
                                        add_css_class: "kalam-comics-direction-row",

                                        gtk::Label {
                                            set_label: "Direction",
                                            set_hexpand: true,
                                            set_xalign: 0.0,
                                            set_valign: gtk::Align::Center,
                                        },

                                        gtk::Box {
                                            set_orientation: gtk::Orientation::Horizontal,
                                            set_spacing: 4,
                                            set_homogeneous: true,
                                            set_size_request: (132, -1),
                                            set_valign: gtk::Align::Center,

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
                                    },

                                    #[name = "zoom_host"]
                                    gtk::Box {
                                        set_orientation: gtk::Orientation::Vertical,
                                    },
                                },

                                // Section: BEHAVIOUR
                                append = &reader_settings_section("Behaviour"),
                                #[name = "behaviour_host"]
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_spacing: 0,
                                    set_margin_start: 16,
                                    set_margin_end: 16,
                                    add_css_class: "kalam-comics-inset-card",
                                },

                                #[name = "defaults_host"]
                                gtk::Box {
                                    set_orientation: gtk::Orientation::Vertical,
                                    set_margin_start: 16,
                                    set_margin_end: 16,
                                },                            },
                        },

                        // Pages View: the whole series in one
                        // windowed grid. Init installs the GridView
                        // as the scrolled window's direct child so
                        // GTK only creates the visible cells.
                        #[name = "pages_overlay"]
                        gtk::Overlay {
                            set_hexpand: true,
                            set_vexpand: true,

                            #[name = "pages_scroll"]
                            gtk::ScrolledWindow {
                                set_hexpand: true,
                                set_vexpand: true,
                                set_hscrollbar_policy: gtk::PolicyType::Never,
                            },

                            // The grid runs under a 72px fade to
                            // the panel card at the bottom.
                            add_overlay = &gtk::Box {
                                add_css_class: "kalam-comics-pages-fade",
                                set_orientation: gtk::Orientation::Vertical,
                                set_valign: gtk::Align::End,
                                set_size_request: (-1, 72),
                                set_can_target: false,
                            },

                            #[name = "pages_empty_label"]
                            add_overlay = &gtk::Label {
                                set_label: "No pages in this chapter",
                                add_css_class: "dim-label",
                                set_halign: gtk::Align::Center,
                                set_valign: gtk::Align::Start,
                                set_margin_top: 24,
                                set_can_target: false,
                                set_visible: false,
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

                        // Image View, populated in init alongside
                        // the Reading pane's guarded controls.
                        #[name = "image_scroll"]
                        gtk::ScrolledWindow {
                            set_hexpand: true,
                            set_vexpand: true,
                            set_hscrollbar_policy: gtk::PolicyType::Never,

                            #[name = "image_box"]
                            gtk::Box {
                                set_orientation: gtk::Orientation::Vertical,
                                set_spacing: 0,
                                set_margin_start: 16,
                                set_margin_end: 16,
                                set_margin_bottom: 20,
                            },
                        },

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
        widgets.left_stack.page(&widgets.pages_overlay).set_name("pages");
        widgets.right_stack.page(&widgets.image_scroll).set_name("image");
        widgets.right_stack.page(&widgets.shortcuts_scroll).set_name("shortcuts");
        widgets.left_stack.set_visible_child_name("chapters");
        widgets.right_stack.set_visible_child_name("reading");

        // The whole-series Pages grid (decision 38). It is the
        // scrolled window's direct child, so GridView can window
        // its cells against the real viewport; thumbnails are only
        // requested as those cells bind.
        {
            let page_grid =
                pages_grid::SeriesPagesGrid::new(sender.input_sender().clone());
            page_grid.set_entries(
                model.page_grid_entries(),
                Some(model.current_series_page_ref()),
            );
            widgets.pages_scroll.set_child(Some(&page_grid.widget()));
            widgets
                .pages_empty_label
                .set_visible(page_grid.entry_count() == 0);
            model.page_grid = Some(page_grid);

            let tx = sender.input_sender().clone();
            widgets
                .pages_scroll
                .vadjustment()
                .connect_value_changed(move |adj| {
                    let _ = tx.send(ComicsReaderMsg::PagesGridScrolled(adj.value()));
                });
        }

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
                &["Reading", "Image", "Shortcuts"],
                &widgets.right_tabs_host,
                Rc::new(move |idx| {
                    let _ = tx.send(ComicsReaderMsg::SetRightTab(match idx {
                        1 => ComicRightTab::Image,
                        2 => ComicRightTab::Shortcuts,
                        _ => ComicRightTab::Reading,
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

        // The Reading pane's switches, zoom slider and defaults pill
        // (decisions 39/147). They are built here rather than in the
        // view macro because each switch needs its guarded setter,
        // the same shape the EPUB Reading pane uses.
        {
            let (row, set) = guarded_switch_row(
                "Settings for this series",
                model.series_override_on,
                &sender,
                Rc::new(ComicsReaderMsg::SetSeriesOverride),
            );
            row.widget().add_css_class("kalam-comics-inset-card");
            widgets.series_settings_host.append(&row.widget());
            model.set_series_override_switch = Some(set);

            let (row, set) = guarded_switch_row(
                "Gap between pages",
                model.strip_gap,
                &sender,
                Rc::new(ComicsReaderMsg::SetStripGap),
            );
            row.widget().add_css_class("kalam-comics-inset-card");
            // The host is what sync shows and hides, so the pane
            // keeps no empty spacing for the row in other styles.
            model.strip_gap_row = Some(widgets.strip_gap_host.clone());
            widgets.strip_gap_host.append(&row.widget());
            model.set_strip_gap_switch = Some(set);

            let (row, set) = guarded_switch_row(
                "Auto next chapter",
                model.auto_next_chapter,
                &sender,
                Rc::new(ComicsReaderMsg::SetAutoNextChapter),
            );
            row.widget()
                .add_css_class("kalam-comics-setting-divider");
            widgets.behaviour_host.append(&row.widget());
            model.set_auto_next_switch = Some(set);

            let (row, set) = guarded_switch_row(
                "Mark read on chapter advance",
                model.mark_read_on_advance,
                &sender,
                Rc::new(ComicsReaderMsg::SetMarkReadOnAdvance),
            );
            widgets.behaviour_host.append(&row.widget());
            model.set_mark_read_switch = Some(set);

            let tx = sender.input_sender().clone();
            let zoom = crate::widgets::setting_row::slider_row(
                "Zoom",
                None,
                reading_settings::MIN_ZOOM_PERCENT as f64,
                reading_settings::MAX_ZOOM_PERCENT as f64,
                5.0,
                model.zoom_percent as f64,
                0,
                "%",
                move |value| {
                    let _ = tx.send(ComicsReaderMsg::SetZoomPercent(value.round() as u32));
                },
            );
            widgets.zoom_host.append(&zoom.widget());
            model.zoom_slider = Some(zoom);

            let tx = sender.input_sender().clone();
            let defaults_btn = crate::widgets::pill_button::boxed_ghost_button(
                "Matches your defaults",
                move || {
                    let _ = tx.send(ComicsReaderMsg::UseSettingsAsDefaults);
                },
            );
            defaults_btn.set_hexpand(true);
            defaults_btn.set_sensitive(false);
            widgets.defaults_host.append(&defaults_btn);
            model.use_defaults_btn = Some(defaults_btn);
        }

        // Saved presets. The dynamic rows are rebuilt only when a
        // preset is saved or deleted; the save form itself is built
        // once and shown or hidden by the ordinary settings sync.
        {
            model.preset_sender = Some(sender.input_sender().clone());

            let list_host = gtk::Box::new(gtk::Orientation::Vertical, 6);
            widgets.presets_host.append(&list_host);
            model.preset_list_host = Some(list_host);

            let empty = gtk::Label::new(Some(
                "No saved presets yet. Set the reader up the way you like, then save it here.",
            ));
            empty.add_css_class("kalam-comics-preset-empty");
            empty.set_halign(gtk::Align::Start);
            empty.set_xalign(0.0);
            empty.set_wrap(true);
            widgets.presets_host.append(&empty);
            model.preset_empty_label = Some(empty);

            let tx = sender.input_sender().clone();
            let save_current = crate::widgets::pill_button::boxed_ghost_button(
                "Save current settings…",
                move || {
                    let _ = tx.send(ComicsReaderMsg::ShowSavePresetForm);
                },
            );
            save_current.set_hexpand(true);
            widgets.presets_host.append(&save_current);

            let form = gtk::Box::new(gtk::Orientation::Vertical, 8);
            form.add_css_class("kalam-comics-inset-card");
            form.add_css_class("kalam-comics-preset-form");
            form.set_visible(false);

            let caption = gtk::Label::new(Some("PRESET NAME"));
            caption.add_css_class("kalam-ui-field-label");
            caption.set_halign(gtk::Align::Start);
            caption.set_xalign(0.0);
            form.append(&caption);

            let entry = gtk::Entry::new();
            entry.add_css_class("kalam-ui-field");
            entry.set_placeholder_text(Some("Manga, Western, Night reading…"));
            entry.set_max_length(reading_settings::MAX_PRESET_NAME_CHARS as i32);
            entry.set_hexpand(true);
            entry.update_property(&[gtk::accessible::Property::Label("Preset name")]);
            {
                let tx = sender.input_sender().clone();
                entry.connect_changed(move |entry| {
                    let _ = tx.send(ComicsReaderMsg::PresetNameChanged(
                        entry.text().to_string(),
                    ));
                });
                let tx = sender.input_sender().clone();
                entry.connect_activate(move |_| {
                    let _ = tx.send(ComicsReaderMsg::SaveCurrentPreset);
                });
            }
            form.append(&entry);
            model.preset_name_entry = Some(entry);

            let note = gtk::Label::new(Some(
                "Presets keep Reading and Image settings together.",
            ));
            note.add_css_class("kalam-comics-preset-note");
            note.set_halign(gtk::Align::Start);
            note.set_xalign(0.0);
            note.set_wrap(true);
            form.append(&note);
            model.preset_form_note = Some(note);

            let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            actions.set_halign(gtk::Align::End);
            let tx = sender.input_sender().clone();
            actions.append(&crate::widgets::pill_button::boxed_ghost_button(
                "Cancel",
                move || {
                    let _ = tx.send(ComicsReaderMsg::CancelSavePreset);
                },
            ));
            let tx = sender.input_sender().clone();
            let save = crate::widgets::pill_button::primary_button(
                "Save preset",
                move || {
                    let _ = tx.send(ComicsReaderMsg::SaveCurrentPreset);
                },
            );
            save.set_sensitive(false);
            actions.append(&save);
            model.preset_save_button = Some(save);
            form.append(&actions);

            widgets.presets_host.append(&form);
            model.preset_form = Some(form);
            model.rebuild_preset_rows();
        }

        // The Image pane (decision 41), in board order. It is built
        // with the Reading pane's guarded rows because edits here
        // are edits to the same panel-wide series record.
        {
            let image_host = widgets.image_box.clone();
            let add_image_section = |label: &str| {
                let heading = gtk::Label::new(Some(label));
                heading.add_css_class("kalam-reader-section-label");
                heading.set_halign(gtk::Align::Start);
                heading.set_margin_top(14);
                heading.set_margin_bottom(8);
                image_host.append(&heading);
            };

            add_image_section("Background");
            let background_card = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            background_card.add_css_class("kalam-comics-inset-card");
            background_card.set_size_request(-1, 48);
            background_card.set_valign(gtk::Align::Center);
            let background_label = gtk::Label::new(Some("Colour"));
            background_label.add_css_class("kalam-ui-body");
            background_label.set_hexpand(true);
            background_label.set_xalign(0.0);
            background_label.set_valign(gtk::Align::Center);
            background_card.append(&background_label);

            let dots = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            dots.set_valign(gtk::Align::Center);
            for background in ComicBackground::ALL {
                let button = gtk::Button::new();
                button.add_css_class("kalam-comics-colour-dot");
                button.add_css_class(match background {
                    ComicBackground::Theme => "bg-theme",
                    ComicBackground::Black => "bg-black",
                    ComicBackground::Grey => "bg-grey",
                    ComicBackground::White => "bg-white",
                });
                button.set_size_request(24, 24);
                button.set_valign(gtk::Align::Center);
                let accessible_label = match background {
                    ComicBackground::Theme => "Theme background",
                    ComicBackground::Black => "Black background",
                    ComicBackground::Grey => "Grey background",
                    ComicBackground::White => "White background",
                };
                button.set_tooltip_text(Some(accessible_label));
                button.update_property(&[gtk::accessible::Property::Label(
                    accessible_label,
                )]);
                let tx = sender.input_sender().clone();
                button.connect_clicked(move |_| {
                    let _ = tx.send(ComicsReaderMsg::SetImageBackground(background));
                });
                dots.append(&button);
                model.image_background_buttons.push(button);
            }
            background_card.append(&dots);
            widgets.image_box.append(&background_card);

            add_image_section("Adjustments");
            let adjustments_card = gtk::Box::new(gtk::Orientation::Vertical, 0);
            adjustments_card.add_css_class("kalam-comics-inset-card");
            let make_adjustment = |label: &str,
                                   initial: i32,
                                   make_msg: Rc<dyn Fn(i32) -> ComicsReaderMsg>| {
                let tx = sender.input_sender().clone();
                crate::widgets::setting_row::slider_row(
                    label,
                    None,
                    ComicImageSettings::MIN_ADJUSTMENT as f64,
                    ComicImageSettings::MAX_ADJUSTMENT as f64,
                    1.0,
                    initial as f64,
                    0,
                    "",
                    move |value| {
                        let _ = tx.send(make_msg(value.round() as i32));
                    },
                )
            };

            let brightness = make_adjustment(
                "Brightness",
                model.image_settings.brightness,
                Rc::new(ComicsReaderMsg::SetBrightness),
            );
            brightness
                .widget()
                .add_css_class("kalam-comics-setting-divider");
            adjustments_card.append(&brightness.widget());
            model.brightness_slider = Some(brightness);

            let contrast = make_adjustment(
                "Contrast",
                model.image_settings.contrast,
                Rc::new(ComicsReaderMsg::SetContrast),
            );
            contrast
                .widget()
                .add_css_class("kalam-comics-setting-divider");
            adjustments_card.append(&contrast.widget());
            model.contrast_slider = Some(contrast);

            let warmth = make_adjustment(
                "Warmth",
                model.image_settings.warmth,
                Rc::new(ComicsReaderMsg::SetWarmth),
            );
            adjustments_card.append(&warmth.widget());
            model.warmth_slider = Some(warmth);
            widgets.image_box.append(&adjustments_card);

            add_image_section("Filters");
            let filters_card = gtk::Box::new(gtk::Orientation::Vertical, 0);
            filters_card.add_css_class("kalam-comics-inset-card");

            let (row, set) = guarded_switch_row(
                "Grayscale",
                model.image_settings.grayscale,
                &sender,
                Rc::new(ComicsReaderMsg::SetGrayscale),
            );
            row.widget()
                .add_css_class("kalam-comics-setting-divider");
            filters_card.append(&row.widget());
            model.set_grayscale_switch = Some(set);

            let (row, set) = guarded_switch_row(
                "Invert colours",
                model.image_settings.invert_colours,
                &sender,
                Rc::new(ComicsReaderMsg::SetInvertColours),
            );
            filters_card.append(&row.widget());
            model.set_invert_switch = Some(set);
            widgets.image_box.append(&filters_card);

            add_image_section("Crop");
            let (row, set) = guarded_switch_row(
                "Crop blank borders",
                model.image_settings.crop_blank_borders,
                &sender,
                Rc::new(ComicsReaderMsg::SetCropBlankBorders),
            );
            row.widget().add_css_class("kalam-comics-inset-card");
            widgets.image_box.append(&row.widget());
            model.set_crop_switch = Some(set);

            let tx = sender.input_sender().clone();
            let reset_btn = crate::widgets::pill_button::boxed_ghost_button(
                "Reset image settings",
                move || {
                    let _ = tx.send(ComicsReaderMsg::ResetImageSettings);
                },
            );
            reset_btn.set_halign(gtk::Align::Start);
            reset_btn.set_margin_top(16);
            widgets.image_box.append(&reset_btn);
        }

        build_shortcuts_panel(&widgets.shortcuts_box);
        populate_chapters_list(&widgets.chapters_list_box, &model, &sender);
        model.sync_page_grid_entries();
        sync_pages_current(&model);
        update_pages_heading(&widgets, &model);
        model.update_bookmark_icon_state(&widgets);
        model.sync_settings_ui(&widgets);

        let is_webtoon = model.direction == ReadingDirection::Webtoon || model.page_style == PageStyle::LongStrip;
        update_viewport_policies(&widgets.viewport_box, model.fit_mode, is_webtoon, model.zoom_percent);
        let child = rebuild_viewport_widget(&model, 850, 700, Some(&sender));
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
        // Added before the drag gesture: controllers run in addition
        // order, so the click has counted the press by the time the
        // drag decides whether to stand down for a double-click.
        let multi_click = Rc::new(std::cell::Cell::new(0_i32));
        let multi_click_cb = multi_click.clone();
        click.connect_pressed(move |gesture, n_press, _, _| {
            root_for_focus.grab_focus();
            multi_click_cb.set(n_press);
            if n_press == 2 {
                // Double-click toggles fit and 100% (decision 42).
                let _ = tx_click.send(ComicsReaderMsg::ToggleZoom100);
                gesture.set_state(gtk::EventSequenceState::Claimed);
            } else {
                // Not claimed: the same press may become a pan drag.
                let _ = tx_click.send(ComicsReaderMsg::ClosePanels);
            }
        });
        widgets.viewport_box.add_controller(click);

        // Drag to pan (decision 42): the adjustments follow the
        // pointer, in the opposite direction of the drag, from
        // wherever the page stood when the drag began.
        {
            let drag = gtk::GestureDrag::new();
            let start = Rc::new(std::cell::Cell::new((0.0, 0.0)));
            let hadj = widgets.viewport_box.hadjustment();
            let vadj = widgets.viewport_box.vadjustment();
            {
                let start = start.clone();
                let hadj = hadj.clone();
                let vadj = vadj.clone();
                let multi_click = multi_click.clone();
                drag.connect_drag_begin(move |_, _, _| {
                    if multi_click.get() < 2 {
                        start.set((hadj.value(), vadj.value()));
                    }
                });
            }
            drag.connect_drag_update(move |gesture, offset_x, offset_y| {
                if multi_click.get() >= 2 {
                    return;
                }
                let (start_x, start_y) = start.get();
                let max_x = (hadj.upper() - hadj.page_size()).max(0.0);
                let max_y = (vadj.upper() - vadj.page_size()).max(0.0);
                if max_x > 0.0 || max_y > 0.0 {
                    hadj.set_value((start_x - offset_x).clamp(0.0, max_x));
                    vadj.set_value((start_y - offset_y).clamp(0.0, max_y));
                    gesture.set_state(gtk::EventSequenceState::Claimed);
                }
            });
            widgets.viewport_box.add_controller(drag);
        }

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
        let zoom_scroll_accum = Rc::new(std::cell::Cell::new(0.0_f64));
        scroll_ctrl.connect_scroll(move |controller, _dx, dy| {
            // Ctrl-scroll zooms (decision 42's scroll-to-zoom, with
            // the plain wheel left to the page/stack scrolling it
            // already drives — a long strip reads by scrolling).
            // Smooth wheels report many small deltas, so they are
            // gathered into one step, as the PDF reader gathers them.
            if controller
                .current_event_state()
                .contains(gdk::ModifierType::CONTROL_MASK)
            {
                let accumulated = zoom_scroll_accum.get() + dy;
                if accumulated <= -0.8 {
                    zoom_scroll_accum.set(0.0);
                    let _ = s_sc.input_sender().send(ComicsReaderMsg::ZoomBy(10));
                } else if accumulated >= 0.8 {
                    zoom_scroll_accum.set(0.0);
                    let _ = s_sc.input_sender().send(ComicsReaderMsg::ZoomBy(-10));
                } else {
                    zoom_scroll_accum.set(accumulated);
                }
                return gtk::glib::Propagation::Stop;
            }
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
                gdk::Key::plus | gdk::Key::equal | gdk::Key::KP_Add => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::ZoomBy(10));
                    glib::Propagation::Stop
                }
                gdk::Key::minus | gdk::Key::KP_Subtract => {
                    let _ = s_key.input_sender().send(ComicsReaderMsg::ZoomBy(-10));
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
            Ok("reading-strip") => {
                // The Long-strip Reading board: real edits, so the
                // frame also proves the series record turning itself
                // on and the strip-only gap row appearing.
                sender.input(ComicsReaderMsg::SetPageStyle(PageStyle::LongStrip));
                sender.input(ComicsReaderMsg::SetDirection(ReadingDirection::Rtl));
                sender.input(ComicsReaderMsg::SetZoomPercent(45));
                sender.input(ComicsReaderMsg::OpenRightPanel);
            }
            Ok("presets") => {
                // Save two real whole-panel setups, then apply the
                // second one back: the frame proves persistence, the
                // summary text and the Current marker together.
                sender.input(ComicsReaderMsg::SetDirection(ReadingDirection::Ltr));
                sender.input(ComicsReaderMsg::SetPageStyle(PageStyle::Single));
                sender.input(ComicsReaderMsg::SetFitMode(FitMode::Screen));
                sender.input(ComicsReaderMsg::ShowSavePresetForm);
                sender.input(ComicsReaderMsg::PresetNameChanged(
                    "Western".to_string(),
                ));
                sender.input(ComicsReaderMsg::SaveCurrentPreset);

                sender.input(ComicsReaderMsg::SetDirection(ReadingDirection::Rtl));
                sender.input(ComicsReaderMsg::SetPageStyle(PageStyle::LongStrip));
                sender.input(ComicsReaderMsg::SetFitMode(FitMode::Width));
                sender.input(ComicsReaderMsg::SetZoomPercent(45));
                sender.input(ComicsReaderMsg::SetImageBackground(
                    ComicBackground::Black,
                ));
                sender.input(ComicsReaderMsg::SetBrightness(10));
                sender.input(ComicsReaderMsg::ShowSavePresetForm);
                sender.input(ComicsReaderMsg::PresetNameChanged(
                    "Manga night".to_string(),
                ));
                sender.input(ComicsReaderMsg::SaveCurrentPreset);
                sender.input(ComicsReaderMsg::ApplyPreset("Western".to_string()));
                sender.input(ComicsReaderMsg::ApplyPreset(
                    "Manga night".to_string(),
                ));
                sender.input(ComicsReaderMsg::OpenRightPanel);
            }
            Ok("image") => {
                sender.input(ComicsReaderMsg::SetRightTab(ComicRightTab::Image));
                sender.input(ComicsReaderMsg::OpenRightPanel);
            }
            Ok("image-adjusted") => {
                // Real Image edits: the frame has to prove the stage
                // colour, the GSK colour matrix and crop all reached
                // the painted page, not only that the pane exists.
                sender.input(ComicsReaderMsg::SetImageBackground(
                    ComicBackground::White,
                ));
                sender.input(ComicsReaderMsg::SetBrightness(15));
                sender.input(ComicsReaderMsg::SetContrast(25));
                sender.input(ComicsReaderMsg::SetWarmth(45));
                sender.input(ComicsReaderMsg::SetCropBlankBorders(true));
                sender.input(ComicsReaderMsg::SetRightTab(ComicRightTab::Image));
                sender.input(ComicsReaderMsg::OpenRightPanel);
            }
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
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
                    widgets.viewport_box.set_child(Some(&child));
                    update_viewport_policies(&widgets.viewport_box, self.fit_mode, false, self.zoom_percent);
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
                    // Auto next chapter (decision 39) skips the
                    // chapter-end card; without it the card is the
                    // deliberate stop before the next chapter.
                    if should_auto_advance(
                        self.auto_next_chapter,
                        is_last_page,
                        self.next_chapter().is_some(),
                    ) {
                        let _ = sender.input_sender().send(ComicsReaderMsg::NextChapter);
                        return;
                    }
                    // A strip has no end card to stop at; with
                    // auto-advance off it simply rests at the last page.
                    if !is_webtoon
                        && (self.next_chapter().is_some() || self.series_chapters.len() > 1)
                    {
                        self.at_chapter_end = true;
                        let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
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
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
                    widgets.viewport_box.set_child(Some(&child));
                    update_viewport_policies(&widgets.viewport_box, self.fit_mode, false, self.zoom_percent);
                }
            }
            ComicsReaderMsg::PrevPage => {
                self.hide_chrome_on_interaction();
                if self.at_chapter_end {
                    self.at_chapter_end = false;
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
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
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
                    widgets.viewport_box.set_child(Some(&child));
                    update_viewport_policies(&widgets.viewport_box, self.fit_mode, false, self.zoom_percent);
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
            ComicsReaderMsg::PageLoaded(gen, idx, ref maybe_tex, crop) => {
                if gen != self.load_gen {
                    // Asked for by the chapter that was left; its
                    // page numbers mean nothing here.
                    return;
                }
                self.pending_loads.remove(&idx);
                if let Some(tex) = maybe_tex.clone() {
                    self.textures.insert(idx, tex.clone());
                    if let Some(crop) = crop {
                        self.page_crop_boxes.insert(idx, crop);
                    }
                    let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;

                    if is_webtoon {
                        let mut in_place_applied = false;
                        if let Some(vbox) = get_webtoon_vbox(&widgets.viewport_box) {
                            let vp_w = widgets.viewport_box.width();
                            let win_w = if vp_w > 100 { vp_w } else { 850 };
                            let avail_w = (((win_w - 24).max(300) as f64) * zoom_factor(self)).round() as i32;
                            let (tw, th) = self.effective_page_size(idx, &tex);
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
                                        let pic = page_picture::ComicPagePicture::new(
                                            &tex,
                                            self.image_settings,
                                            self.effective_crop(idx),
                                        );
                                        pic.set_size_request(avail_w, target_h);
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
                            let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
                            widgets.viewport_box.set_child(Some(&child));
                            update_viewport_policies(&widgets.viewport_box, self.fit_mode, true, self.zoom_percent);
                        }
                    } else {
                        let in_view = if self.page_style == PageStyle::Double {
                            idx == self.current_page || idx == self.current_page + 1
                        } else {
                            idx == self.current_page
                        };
                        if in_view {
                            let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
                            widgets.viewport_box.set_child(Some(&child));
                            update_viewport_policies(&widgets.viewport_box, self.fit_mode, false, self.zoom_percent);
                        }
                    }
                }
            }
            ComicsReaderMsg::ThumbnailLoaded(gen, page, maybe_tex) => {
                if gen != self.thumb_load_gen {
                    return;
                }
                self.thumb_pending.remove(&page);
                if let Some(grid) = self.page_grid.clone() {
                    grid.update_thumbnail(page, maybe_tex);
                }
            }
            ComicsReaderMsg::PageThumbnailVisible(page) => {
                self.request_page_thumbnail(page, &sender);
            }
            ComicsReaderMsg::OpenSeriesPage(page) => {
                if page.chapter_idx.is_none()
                    || page.chapter_idx == self.current_chapter_idx
                {
                    let _ = sender
                        .input_sender()
                        .send(ComicsReaderMsg::SetPage(page.page_idx));
                } else if let Some(book_id) = self
                    .page_grid
                    .as_ref()
                    .and_then(|grid| grid.book_id_for_page(page))
                {
                    let _ = sender.input_sender().send(
                        ComicsReaderMsg::SwitchChapterAt(book_id, Some(page.page_idx)),
                    );
                }
            }
            ComicsReaderMsg::PagesGridScrolled(_) => {
                if self.left_tab == ComicLeftTab::Pages {
                    update_pages_heading(widgets, self);
                }
            }
            ComicsReaderMsg::ToggleDirection => {
                let next = self.direction.next();
                let _ = sender.input_sender().send(ComicsReaderMsg::SetDirection(next));
            }
            ComicsReaderMsg::SetDirection(dir) => {
                // A Webtoon direction is the old spelling of Long
                // strip; the pane itself only offers LTR/RTL.
                let (dir, style) = if dir == ReadingDirection::Webtoon {
                    (ReadingDirection::Ltr, PageStyle::LongStrip)
                } else {
                    (dir, self.page_style)
                };
                if self.direction != dir || self.page_style != style {
                    self.direction = dir;
                    self.page_style = style;
                    self.store_series_setting();
                    self.trigger_osd(dir.label(), &sender);
                    self.trigger_loads(&sender);
                    let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                    update_viewport_policies(&widgets.viewport_box, self.fit_mode, is_webtoon, self.zoom_percent);
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
                    widgets.viewport_box.set_child(Some(&child));
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::SetPageStyle(style) => {
                if self.page_style != style {
                    self.page_style = style;
                    self.store_series_setting();
                    self.trigger_osd(style.label(), &sender);
                    self.trigger_loads(&sender);
                    widgets.page_indicator_label.set_label(&self.page_indicator_label());
                    let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                    update_viewport_policies(&widgets.viewport_box, self.fit_mode, is_webtoon, self.zoom_percent);
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
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
                // Choosing a fit means that fit exactly: any zoom
                // away from it is forgotten.
                if self.fit_mode != fit || self.zoom_percent != 100 {
                    self.fit_mode = fit;
                    self.zoom_percent = 100;
                    self.pre_zoom_fit = None;
                    self.store_series_setting();
                    self.trigger_osd(fit.label(), &sender);
                    let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                    update_viewport_policies(&widgets.viewport_box, self.fit_mode, is_webtoon, self.zoom_percent);
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
                    widgets.viewport_box.set_child(Some(&child));
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::SetSpreadGap(gap) => {
                let clamped = gap.clamp(0, 48);
                if self.two_page_gap != clamped {
                    self.two_page_gap = clamped;
                    self.store_series_setting();
                    if self.page_style == PageStyle::Double {
                        let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
                        widgets.viewport_box.set_child(Some(&child));
                    }
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::SetStripGap(on) => {
                if self.strip_gap != on {
                    self.strip_gap = on;
                    self.store_series_setting();
                    let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                    if is_webtoon {
                        let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
                        widgets.viewport_box.set_child(Some(&child));
                        update_viewport_policies(&widgets.viewport_box, self.fit_mode, true, self.zoom_percent);
                    }
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::SetZoomPercent(percent) => {
                let clamped = percent.clamp(
                    reading_settings::MIN_ZOOM_PERCENT,
                    reading_settings::MAX_ZOOM_PERCENT,
                );
                if self.zoom_percent != clamped {
                    self.zoom_percent = clamped;
                    self.store_series_setting();
                    self.trigger_osd(&format!("{}%", self.zoom_percent), &sender);
                    let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                    update_viewport_policies(&widgets.viewport_box, self.fit_mode, is_webtoon, self.zoom_percent);
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
                    widgets.viewport_box.set_child(Some(&child));
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::ZoomBy(delta) => {
                let next = (self.zoom_percent as i32 + delta).clamp(
                    reading_settings::MIN_ZOOM_PERCENT as i32,
                    reading_settings::MAX_ZOOM_PERCENT as i32,
                ) as u32;
                let _ = sender
                    .input_sender()
                    .send(ComicsReaderMsg::SetZoomPercent(next));
            }
            ComicsReaderMsg::ToggleZoom100 => {
                // Fit ↔ 100% (decision 42): a zoomed page first
                // returns to its fit; a fitted page goes to original
                // size, remembering the fit it came from.
                if self.zoom_percent != 100 {
                    self.zoom_percent = 100;
                } else if self.fit_mode != FitMode::Original {
                    self.pre_zoom_fit = Some(self.fit_mode);
                    self.fit_mode = FitMode::Original;
                } else {
                    self.fit_mode = self.pre_zoom_fit.take().unwrap_or(FitMode::Screen);
                }
                self.store_series_setting();
                self.trigger_osd(
                    if self.fit_mode == FitMode::Original {
                        "100%"
                    } else {
                        self.fit_mode.label()
                    },
                    &sender,
                );
                let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                update_viewport_policies(&widgets.viewport_box, self.fit_mode, is_webtoon, self.zoom_percent);
                let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
                widgets.viewport_box.set_child(Some(&child));
                self.sync_settings_ui(widgets);
            }
            ComicsReaderMsg::SetAutoNextChapter(on) => {
                if self.auto_next_chapter != on {
                    self.auto_next_chapter = on;
                    self.store_series_setting();
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::SetMarkReadOnAdvance(on) => {
                if self.mark_read_on_advance != on {
                    self.mark_read_on_advance = on;
                    self.store_series_setting();
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::SetImageBackground(background) => {
                if self.image_settings.background != background {
                    self.image_settings.background = background;
                    self.store_series_setting();
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::SetBrightness(value) => {
                let value = value.clamp(
                    ComicImageSettings::MIN_ADJUSTMENT,
                    ComicImageSettings::MAX_ADJUSTMENT,
                );
                if self.image_settings.brightness != value {
                    self.image_settings.brightness = value;
                    self.store_series_setting();
                    if let Some(child) = widgets.viewport_box.child() {
                        update_page_pictures(&child, self.image_settings);
                    }
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::SetContrast(value) => {
                let value = value.clamp(
                    ComicImageSettings::MIN_ADJUSTMENT,
                    ComicImageSettings::MAX_ADJUSTMENT,
                );
                if self.image_settings.contrast != value {
                    self.image_settings.contrast = value;
                    self.store_series_setting();
                    if let Some(child) = widgets.viewport_box.child() {
                        update_page_pictures(&child, self.image_settings);
                    }
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::SetWarmth(value) => {
                let value = value.clamp(
                    ComicImageSettings::MIN_ADJUSTMENT,
                    ComicImageSettings::MAX_ADJUSTMENT,
                );
                if self.image_settings.warmth != value {
                    self.image_settings.warmth = value;
                    self.store_series_setting();
                    if let Some(child) = widgets.viewport_box.child() {
                        update_page_pictures(&child, self.image_settings);
                    }
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::SetGrayscale(on) => {
                if self.image_settings.grayscale != on {
                    self.image_settings.grayscale = on;
                    self.store_series_setting();
                    if let Some(child) = widgets.viewport_box.child() {
                        update_page_pictures(&child, self.image_settings);
                    }
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::SetInvertColours(on) => {
                if self.image_settings.invert_colours != on {
                    self.image_settings.invert_colours = on;
                    self.store_series_setting();
                    if let Some(child) = widgets.viewport_box.child() {
                        update_page_pictures(&child, self.image_settings);
                    }
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::SetCropBlankBorders(on) => {
                if self.image_settings.crop_blank_borders != on {
                    self.image_settings.crop_blank_borders = on;
                    self.store_series_setting();
                    // Crop changes the page's effective aspect, not
                    // just its paint: the fit sizes have to be laid
                    // out again around the cropped rectangle.
                    let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                    update_viewport_policies(&widgets.viewport_box, self.fit_mode, is_webtoon, self.zoom_percent);
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
                    widgets.viewport_box.set_child(Some(&child));
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::ResetImageSettings => {
                let defaults = ComicImageSettings::default();
                if self.image_settings != defaults {
                    let crop_changed =
                        self.image_settings.crop_blank_borders != defaults.crop_blank_borders;
                    self.image_settings = defaults;
                    self.store_series_setting();
                    if crop_changed {
                        let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                        update_viewport_policies(&widgets.viewport_box, self.fit_mode, is_webtoon, self.zoom_percent);
                        let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
                        widgets.viewport_box.set_child(Some(&child));
                    } else if let Some(child) = widgets.viewport_box.child() {
                        update_page_pictures(&child, self.image_settings);
                    }
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::SetSeriesOverride(on) => {
                if on {
                    self.store_series_setting();
                    self.sync_settings_ui(widgets);
                } else {
                    // Reset to my defaults is this same gesture
                    // (decision 147): forget the record and take the
                    // global comic choices wholesale.
                    if let (Some(ref catalog), Some(key)) =
                        (&self.catalog, &self.series_settings_key)
                    {
                        ComicReadingSettings::clear_series(catalog, key);
                    }
                    self.series_override_on = false;
                    let globals = match &self.catalog {
                        Some(catalog) => ComicReadingSettings::load_globals(catalog),
                        None => ComicReadingSettings::default(),
                    };
                    self.apply_reading_settings(&globals);
                    self.trigger_loads(&sender);
                    widgets.page_indicator_label.set_label(&self.page_indicator_label());
                    let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                    update_viewport_policies(&widgets.viewport_box, self.fit_mode, is_webtoon, self.zoom_percent);
                    let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
                    widgets.viewport_box.set_child(Some(&child));
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::UseSettingsAsDefaults => {
                let current = self.current_reading_settings();
                if let Some(ref catalog) = self.catalog {
                    current.write_globals(catalog);
                    // The series record now says nothing the
                    // defaults don't, so it can go: the series just
                    // follows them again.
                    if self.series_override_on {
                        if let Some(key) = &self.series_settings_key {
                            ComicReadingSettings::clear_series(catalog, key);
                        }
                        self.series_override_on = false;
                    }
                }
                self.trigger_osd("Defaults updated", &sender);
                self.sync_settings_ui(widgets);
            }
            ComicsReaderMsg::ShowSavePresetForm => {
                self.preset_form_open = true;
                self.preset_form_error.clear();
                if let Some(active) = self.active_preset_name().map(str::to_string) {
                    self.preset_name_draft = active;
                }
                if let Some(entry) = &self.preset_name_entry {
                    if entry.text().as_str() != self.preset_name_draft {
                        entry.set_text(&self.preset_name_draft);
                    }
                    entry.grab_focus();
                    entry.select_region(0, -1);
                }
                self.sync_settings_ui(widgets);
            }
            ComicsReaderMsg::CancelSavePreset => {
                self.preset_form_open = false;
                self.preset_form_error.clear();
                self.sync_settings_ui(widgets);
            }
            ComicsReaderMsg::PresetNameChanged(name) => {
                self.preset_name_draft = name;
                self.preset_form_error.clear();
                self.sync_settings_ui(widgets);
            }
            ComicsReaderMsg::SaveCurrentPreset => {
                let name = reading_settings::normalise_preset_name(&self.preset_name_draft);
                if name.is_empty() {
                    self.preset_form_open = true;
                    self.preset_form_error = "Give this preset a name.".to_string();
                    if let Some(entry) = &self.preset_name_entry {
                        entry.grab_focus();
                    }
                    self.sync_settings_ui(widgets);
                } else if name.chars().count()
                    > reading_settings::MAX_PRESET_NAME_CHARS
                {
                    self.preset_form_open = true;
                    self.preset_form_error = format!(
                        "Keep the name to {} characters or fewer.",
                        reading_settings::MAX_PRESET_NAME_CHARS
                    );
                    if let Some(entry) = &self.preset_name_entry {
                        entry.grab_focus();
                    }
                    self.sync_settings_ui(widgets);
                } else {
                    let settings = self.current_reading_settings();
                    let replaced =
                        reading_settings::upsert_preset(&mut self.saved_presets, &name, settings);
                    self.persist_presets();
                    self.preset_name_draft = name.clone();
                    self.preset_form_open = false;
                    self.preset_form_error.clear();
                    if let Some(entry) = &self.preset_name_entry {
                        if entry.text().as_str() != name {
                            entry.set_text(&name);
                        }
                    }
                    self.rebuild_preset_rows();
                    let osd = if replaced {
                        format!("Preset “{name}” replaced")
                    } else {
                        format!("Preset “{name}” saved")
                    };
                    self.trigger_osd(&osd, &sender);
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::ApplyPreset(name) => {
                let Some(preset) = self
                    .saved_presets
                    .iter()
                    .find(|preset| preset.name.to_lowercase() == name.to_lowercase())
                    .cloned()
                else {
                    return;
                };
                if self.current_reading_settings() == preset.settings {
                    self.trigger_osd(&format!("“{}” is already in use", preset.name), &sender);
                    self.sync_settings_ui(widgets);
                } else {
                    self.apply_reading_settings(&preset.settings);
                    self.store_series_setting();
                    self.trigger_osd(&format!("Preset “{}”", preset.name), &sender);
                    self.trigger_loads(&sender);
                    widgets
                        .page_indicator_label
                        .set_label(&self.page_indicator_label());
                    let is_webtoon = self.direction == ReadingDirection::Webtoon
                        || self.page_style == PageStyle::LongStrip;
                    update_viewport_policies(
                        &widgets.viewport_box,
                        self.fit_mode,
                        is_webtoon,
                        self.zoom_percent,
                    );
                    let child = rebuild_viewport_widget(
                        self,
                        widgets.viewport_box.width(),
                        widgets.viewport_box.height(),
                        Some(&sender),
                    );
                    widgets.viewport_box.set_child(Some(&child));
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::DeletePreset(name) => {
                if reading_settings::remove_preset(&mut self.saved_presets, &name) {
                    self.persist_presets();
                    self.rebuild_preset_rows();
                    self.trigger_osd(&format!("Preset “{name}” deleted"), &sender);
                    self.sync_settings_ui(widgets);
                }
            }
            ComicsReaderMsg::ViewportResized => {
                let curr_w = widgets.viewport_box.width();
                let curr_h = widgets.viewport_box.height();
                let width_changed = (curr_w - self.last_viewport_width).abs() > 16;
                let height_changed = (curr_h - self.last_viewport_height).abs() > 16;
                if width_changed || height_changed {
                    self.last_viewport_width = curr_w;
                    self.last_viewport_height = curr_h;
                    // Width-sized pages follow the width; height and
                    // screen fits (and any zoom away from a fit) are
                    // sized from both dimensions.
                    let size_dependent = self.fit_mode == FitMode::Width
                        || self.fit_mode == FitMode::Height
                        || self.fit_mode == FitMode::Screen
                        || self.zoom_percent != 100;
                    if size_dependent {
                        let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                        let child = rebuild_viewport_widget(self, curr_w, curr_h, Some(&sender));
                        widgets.viewport_box.set_child(Some(&child));
                        update_viewport_policies(&widgets.viewport_box, self.fit_mode, is_webtoon, self.zoom_percent);
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
                // Decide now, write only once the next chapter has
                // actually opened: a missing file must not mark the
                // chapter still being read as finished.
                let mut mark_read_on_leave = false;
                if let (Some(_), Some(curr_bid)) = (&self.catalog, self.book_id) {
                    if curr_bid == new_bid {
                        if let Some(page) = target_page {
                            let _ = sender.input_sender().send(ComicsReaderMsg::SetPage(page));
                        }
                        return;
                    }
                    let target_idx = self
                        .series_chapters
                        .iter()
                        .position(|b| b.id == new_bid);
                    mark_read_on_leave = should_mark_read_on_advance(
                        self.mark_read_on_advance,
                        self.current_chapter_idx,
                        target_idx,
                    );
                }
                if let Some(catalog) = self.catalog.clone() {
                    if let Ok(Some(new_book)) = catalog.get_book(new_bid) {
                        if let Ok(new_prov) = crate::pages::comics_reader::providers::LocalProvider::new(new_book.file_path.clone()) {
                            if mark_read_on_leave {
                                self.mark_current_chapter_read();
                            } else {
                                self.save_progress();
                            }
                            self.book_id = Some(new_bid);
                            self.title = new_book.title.clone();
                            self.cover_path = new_book.cover_path.clone();
                            if !new_book.authors.trim().is_empty() {
                                self.authors = new_book.authors.clone();
                            }
                            self.provider = std::sync::Arc::new(new_prov);
                            self.total_pages = self.provider.page_count();
                            self.textures.clear();
                            self.page_crop_boxes.clear();
                            self.pending_loads.clear();
                            // Pages thumbnails and their pending
                            // decodes are series-wide: moving between
                            // chapters changes the ring, not the truth
                            // of a thumbnail already decoded.
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
                            self.sync_page_grid_entries();
                            sync_pages_current(self);
                            widgets.pages_empty_label.set_visible(
                                self.page_grid
                                    .as_ref()
                                    .is_some_and(|grid| grid.entry_count() == 0),
                            );
                            update_pages_heading(widgets, self);

                            self.trigger_loads(&sender);
                            let child = rebuild_viewport_widget(self, widgets.viewport_box.width(), widgets.viewport_box.height(), Some(&sender));
                            widgets.viewport_box.set_child(Some(&child));
                            let is_webtoon = self.direction == ReadingDirection::Webtoon || self.page_style == PageStyle::LongStrip;
                            update_viewport_policies(&widgets.viewport_box, self.fit_mode, is_webtoon, self.zoom_percent);
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
    fn test_reading_defaults_and_behaviour_gates() {
        let dummy = Arc::new(DummyProvider { count: 10 });
        let mut model = ComicsReaderModel::new(types::ComicsReaderInit {
            title: "Test Reading Defaults".to_string(),
            provider: dummy,
            catalog: None,
            book_id: None,
            cover_path: None,
        });

        // The built-in defaults behind the global prefs.
        assert!(!model.series_override_on);
        assert!(model.strip_gap);
        assert_eq!(model.zoom_percent, 100);
        assert!(model.auto_next_chapter);
        assert!(model.mark_read_on_advance);
        assert!(!model.settings_differ_from_defaults());

        model.zoom_percent = 45;
        assert!(model.settings_differ_from_defaults());
        assert_eq!(zoomed_size(800, 1200, zoom_factor(&model)), (360, 540));

        assert!(should_auto_advance(true, true, true));
        assert!(!should_auto_advance(false, true, true));
        assert!(!should_auto_advance(true, false, true));
        assert!(!should_auto_advance(true, true, false));

        assert!(should_mark_read_on_advance(true, Some(0), Some(1)));
        assert!(!should_mark_read_on_advance(true, Some(1), Some(0)));
        assert!(!should_mark_read_on_advance(false, Some(0), Some(1)));
        assert!(!should_mark_read_on_advance(true, None, Some(1)));
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
