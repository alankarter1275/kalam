//! Comic reading settings and the per-series override (decisions 39,
//! 40 and 147).
//!
//! The global values live under the established `reader.comic.*` keys,
//! so they are shared by every library. A series that keeps its own
//! settings has one JSON record in that library's database; its
//! presence is the "Settings for this series" switch. Older Kalam
//! versions wrote direction, page style and fit separately per chapter
//! book (`book.<id>.comic.*`); those values seed the series record the
//! first time a chapter from the series is opened with this model.

use super::types::{
    ComicBackground, ComicImageSettings, FitMode, PageStyle, ReadingDirection,
};
use crate::db::Catalog;
use serde::{Deserialize, Serialize};

pub(crate) const MIN_ZOOM_PERCENT: u32 = 25;
pub(crate) const MAX_ZOOM_PERCENT: u32 = 300;

/// The global saved-presets record. The `reader.*` prefix is load-bearing:
/// library switching treats those keys as app-wide, so a reader's named
/// setups follow them from one library to the next.
pub(crate) const COMIC_PRESETS_PREF: &str = "reader.comic.presets.v1";
pub(crate) const MAX_PRESET_NAME_CHARS: usize = 60;

/// Everything shown by the comics Reading and Image panes, as one
/// value. Keeping the whole right panel in one record is the
/// all-or-nothing rule (decision 40): a series either follows the
/// global choices or carries this complete set.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub(crate) struct ComicReadingSettings {
    pub(crate) direction: ReadingDirection,
    pub(crate) page_style: PageStyle,
    pub(crate) fit_mode: FitMode,
    /// The facing-page seam, retained from the earlier reader even
    /// though the redesigned pane only asks about it in Spread.
    pub(crate) two_page_gap: i32,
    /// Whether Long strip puts space between successive pages.
    pub(crate) strip_gap: bool,
    /// Zoom as a percentage of the size the selected fit produces:
    /// 100 is the fit itself, 200 is twice that size.
    pub(crate) zoom_percent: u32,
    pub(crate) auto_next_chapter: bool,
    pub(crate) mark_read_on_advance: bool,
    /// The Image pane rides in the same record: one switch covers the
    /// whole panel, so an Image edit is a series edit too.
    pub(crate) image: ComicImageSettings,
}

impl Default for ComicReadingSettings {
    fn default() -> Self {
        Self {
            direction: ReadingDirection::Ltr,
            page_style: PageStyle::Single,
            fit_mode: FitMode::Width,
            two_page_gap: 0,
            strip_gap: true,
            zoom_percent: 100,
            auto_next_chapter: true,
            mark_read_on_advance: true,
            image: ComicImageSettings::default(),
        }
    }
}

/// The database-local key for one series' record. A real comic-series
/// row is the stable identity. Series known only from chapter books'
/// shared series name anchor to their lowest chapter-book id, which
/// every chapter's open computes from the same set; a stand-alone
/// chapter anchors to itself.
pub(crate) fn series_pref_key(
    series_id: Option<i64>,
    chapter_book_ids: &[i64],
    book_id: Option<i64>,
) -> Option<String> {
    if let Some(series_id) = series_id {
        return Some(format!("comic.series.reading.{series_id}"));
    }
    if let Some(anchor) = chapter_book_ids.iter().min() {
        return Some(format!("comic.series.reading.first.{anchor}"));
    }
    book_id.map(|id| format!("comic.series.reading.book.{id}"))
}

fn direction_from_pref(value: Option<&str>) -> ReadingDirection {
    match value {
        Some("rtl") => ReadingDirection::Rtl,
        Some("webtoon") => ReadingDirection::Webtoon,
        _ => ReadingDirection::Ltr,
    }
}

fn direction_pref(direction: ReadingDirection) -> &'static str {
    match direction {
        ReadingDirection::Ltr => "ltr",
        ReadingDirection::Rtl => "rtl",
        ReadingDirection::Webtoon => "webtoon",
    }
}

fn page_style_from_pref(value: Option<&str>) -> PageStyle {
    match value {
        Some("double") => PageStyle::Double,
        Some("strip") => PageStyle::LongStrip,
        // Fade is not one of the designed page styles (decision 39);
        // a stored "fade" opens as Single, like anything unrecognised.
        _ => PageStyle::Single,
    }
}

fn page_style_pref(style: PageStyle) -> &'static str {
    match style {
        PageStyle::Single | PageStyle::Fade => "single",
        PageStyle::Double => "double",
        PageStyle::LongStrip => "strip",
    }
}

fn fit_from_pref(value: Option<&str>) -> FitMode {
    match value {
        Some("height") => FitMode::Height,
        Some("screen") => FitMode::Screen,
        Some("original") => FitMode::Original,
        _ => FitMode::Width,
    }
}

fn fit_pref(fit: FitMode) -> &'static str {
    match fit {
        FitMode::Width => "width",
        FitMode::Height => "height",
        FitMode::Screen => "screen",
        FitMode::Original => "original",
    }
}

fn background_from_pref(value: Option<&str>) -> ComicBackground {
    match value {
        Some("black") => ComicBackground::Black,
        Some("grey") => ComicBackground::Grey,
        Some("white") => ComicBackground::White,
        _ => ComicBackground::Theme,
    }
}

fn background_pref(background: ComicBackground) -> &'static str {
    match background {
        ComicBackground::Theme => "theme",
        ComicBackground::Black => "black",
        ComicBackground::Grey => "grey",
        ComicBackground::White => "white",
    }
}

impl ComicReadingSettings {
    /// Clamp numeric values and fold the two historical spellings the
    /// redesigned pane no longer offers: Fade becomes Single, and a
    /// Webtoon *direction* becomes the Long strip page style reading
    /// left to right.
    pub(crate) fn canonical(mut self) -> Self {
        self.two_page_gap = self.two_page_gap.clamp(0, 48);
        self.zoom_percent = self
            .zoom_percent
            .clamp(MIN_ZOOM_PERCENT, MAX_ZOOM_PERCENT);
        self.image = self.image.canonical();
        if self.page_style == PageStyle::Fade {
            self.page_style = PageStyle::Single;
        }
        if self.direction == ReadingDirection::Webtoon {
            self.direction = ReadingDirection::Ltr;
            self.page_style = PageStyle::LongStrip;
        }
        self
    }

    pub(crate) fn load_globals(catalog: &Catalog) -> Self {
        Self {
            direction: direction_from_pref(
                catalog.get_pref("reader.comic.direction").as_deref(),
            ),
            page_style: page_style_from_pref(
                catalog.get_pref("reader.comic.page_style").as_deref(),
            ),
            fit_mode: fit_from_pref(catalog.get_pref("reader.comic.fit_mode").as_deref()),
            two_page_gap: catalog
                .get_pref_i64("reader.comic.two_page_gap", 0)
                .clamp(0, 48) as i32,
            strip_gap: catalog.get_pref_i64("reader.comic.strip_gap", 1) != 0,
            zoom_percent: catalog
                .get_pref_i64("reader.comic.zoom_percent", 100)
                .clamp(MIN_ZOOM_PERCENT as i64, MAX_ZOOM_PERCENT as i64) as u32,
            auto_next_chapter: catalog
                .get_pref_i64("reader.comic.auto_next_chapter", 1)
                != 0,
            mark_read_on_advance: catalog
                .get_pref_i64("reader.comic.mark_read_on_advance", 1)
                != 0,
            image: ComicImageSettings {
                background: background_from_pref(
                    catalog
                        .get_pref("reader.comic.image.background")
                        .as_deref(),
                ),
                brightness: catalog.get_pref_i64("reader.comic.image.brightness", 0) as i32,
                contrast: catalog.get_pref_i64("reader.comic.image.contrast", 0) as i32,
                warmth: catalog.get_pref_i64("reader.comic.image.warmth", 0) as i32,
                grayscale: catalog.get_pref_i64("reader.comic.image.grayscale", 0) != 0,
                invert_colours: catalog.get_pref_i64("reader.comic.image.invert", 0) != 0,
                crop_blank_borders: catalog.get_pref_i64("reader.comic.image.crop", 1) != 0,
            },
        }
        .canonical()
    }

    pub(crate) fn write_globals(&self, catalog: &Catalog) {
        let settings = self.clone().canonical();
        catalog.set_pref("reader.comic.direction", direction_pref(settings.direction));
        catalog.set_pref(
            "reader.comic.page_style",
            page_style_pref(settings.page_style),
        );
        catalog.set_pref("reader.comic.fit_mode", fit_pref(settings.fit_mode));
        catalog.set_pref_i64("reader.comic.two_page_gap", settings.two_page_gap as i64);
        catalog.set_pref_i64(
            "reader.comic.strip_gap",
            i64::from(settings.strip_gap),
        );
        catalog.set_pref_i64("reader.comic.zoom_percent", settings.zoom_percent as i64);
        catalog.set_pref_i64(
            "reader.comic.auto_next_chapter",
            i64::from(settings.auto_next_chapter),
        );
        catalog.set_pref_i64(
            "reader.comic.mark_read_on_advance",
            i64::from(settings.mark_read_on_advance),
        );
        catalog.set_pref(
            "reader.comic.image.background",
            background_pref(settings.image.background),
        );
        catalog.set_pref_i64(
            "reader.comic.image.brightness",
            settings.image.brightness as i64,
        );
        catalog.set_pref_i64(
            "reader.comic.image.contrast",
            settings.image.contrast as i64,
        );
        catalog.set_pref_i64("reader.comic.image.warmth", settings.image.warmth as i64);
        catalog.set_pref_i64(
            "reader.comic.image.grayscale",
            i64::from(settings.image.grayscale),
        );
        catalog.set_pref_i64(
            "reader.comic.image.invert",
            i64::from(settings.image.invert_colours),
        );
        catalog.set_pref_i64(
            "reader.comic.image.crop",
            i64::from(settings.image.crop_blank_borders),
        );
    }

    pub(crate) fn load_series(catalog: &Catalog, key: &str) -> Option<Self> {
        let raw = catalog.get_pref(key)?;
        serde_json::from_str::<Self>(&raw)
            .ok()
            .map(Self::canonical)
    }

    pub(crate) fn save_series(&self, catalog: &Catalog, key: &str) {
        if let Ok(raw) = serde_json::to_string(&self.clone().canonical()) {
            catalog.set_pref(key, &raw);
        }
    }

    pub(crate) fn clear_series(catalog: &Catalog, key: &str) {
        catalog.delete_pref(key);
    }

    /// The pre-redesign per-book values for one chapter, over the
    /// standing globals. `None` when the chapter never had any.
    pub(crate) fn legacy_book(
        catalog: &Catalog,
        book_id: i64,
        globals: &Self,
    ) -> Option<Self> {
        let direction = catalog.get_pref(&format!("book.{book_id}.comic.direction"));
        let page_style = catalog.get_pref(&format!("book.{book_id}.comic.page_style"));
        let fit_mode = catalog.get_pref(&format!("book.{book_id}.comic.fit_mode"));
        if direction.is_none() && page_style.is_none() && fit_mode.is_none() {
            return None;
        }
        let mut settings = globals.clone();
        if direction.is_some() {
            settings.direction = direction_from_pref(direction.as_deref());
        }
        if page_style.is_some() {
            settings.page_style = page_style_from_pref(page_style.as_deref());
        }
        if fit_mode.is_some() {
            settings.fit_mode = fit_from_pref(fit_mode.as_deref());
        }
        Some(settings.canonical())
    }

    pub(crate) fn clear_legacy_book(catalog: &Catalog, book_id: i64) {
        for field in ["direction", "page_style", "fit_mode"] {
            catalog.delete_pref(&format!("book.{book_id}.comic.{field}"));
        }
    }
}

/// One named, user-saved comics setup. A preset is deliberately the whole
/// Reading + Image record, not a partial overlay: applying “Manga” should
/// not leave last night's warmth or crop choice hiding inside it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct ComicSettingsPreset {
    pub(crate) name: String,
    pub(crate) settings: ComicReadingSettings,
}

impl ComicSettingsPreset {
    pub(crate) fn summary(&self) -> String {
        self.settings.summary()
    }
}

impl ComicReadingSettings {
    /// The short shape shown under a preset's name. It names the choices a
    /// reader uses to tell presets apart without turning the row into a
    /// dump of every switch in the record.
    pub(crate) fn summary(&self) -> String {
        let settings = self.clone().canonical();
        let fit = match settings.fit_mode {
            FitMode::Width => "Width",
            FitMode::Height => "Height",
            FitMode::Screen => "Screen",
            FitMode::Original => "Original",
        };
        let mut parts = vec![
            settings.direction.short_label().to_string(),
            settings.page_style.label().to_string(),
            fit.to_string(),
            format!("{}%", settings.zoom_percent),
        ];
        if settings.image != ComicImageSettings::default() {
            parts.push("Image adjusted".to_string());
        }
        parts.join(" · ")
    }
}

/// Names are display labels, so whitespace is presentation rather than
/// identity: trim the ends and collapse runs inside before comparing or
/// storing a name.
pub(crate) fn normalise_preset_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn preset_name_key(name: &str) -> String {
    normalise_preset_name(name).to_lowercase()
}

fn sort_presets(presets: &mut [ComicSettingsPreset]) {
    presets.sort_by(|a, b| {
        a.name
            .to_lowercase()
            .cmp(&b.name.to_lowercase())
            .then_with(|| a.name.cmp(&b.name))
    });
}

/// Add a preset, or replace the one whose name differs only by case or
/// spacing. Returns whether an existing preset was replaced, so the UI can
/// say so instead of silently pretending a second copy was made.
pub(crate) fn upsert_preset(
    presets: &mut Vec<ComicSettingsPreset>,
    name: &str,
    settings: ComicReadingSettings,
) -> bool {
    let name = normalise_preset_name(name);
    if name.is_empty() {
        return false;
    }
    let key = preset_name_key(&name);
    let settings = settings.canonical();
    if let Some(existing) = presets
        .iter_mut()
        .find(|preset| preset_name_key(&preset.name) == key)
    {
        existing.name = name;
        existing.settings = settings;
        sort_presets(presets);
        return true;
    }
    presets.push(ComicSettingsPreset { name, settings });
    sort_presets(presets);
    false
}

pub(crate) fn remove_preset(presets: &mut Vec<ComicSettingsPreset>, name: &str) -> bool {
    let key = preset_name_key(name);
    let before = presets.len();
    presets.retain(|preset| preset_name_key(&preset.name) != key);
    presets.len() != before
}

fn canonical_preset_list(raw: Vec<ComicSettingsPreset>) -> Vec<ComicSettingsPreset> {
    let mut out: Vec<ComicSettingsPreset> = Vec::new();
    for preset in raw {
        let name = normalise_preset_name(&preset.name);
        if name.is_empty() {
            continue;
        }
        let key = preset_name_key(&name);
        if out.iter().any(|p| preset_name_key(&p.name) == key) {
            continue;
        }
        out.push(ComicSettingsPreset {
            name,
            settings: preset.settings.canonical(),
        });
    }
    sort_presets(&mut out);
    out
}

pub(crate) fn load_presets(catalog: &Catalog) -> Vec<ComicSettingsPreset> {
    let Some(raw) = catalog.get_pref(COMIC_PRESETS_PREF) else {
        return Vec::new();
    };
    serde_json::from_str::<Vec<ComicSettingsPreset>>(&raw)
        .map(canonical_preset_list)
        .unwrap_or_default()
}

pub(crate) fn store_presets(catalog: &Catalog, presets: &[ComicSettingsPreset]) {
    let presets = canonical_preset_list(presets.to_vec());
    if let Ok(raw) = serde_json::to_string(&presets) {
        catalog.set_pref(COMIC_PRESETS_PREF, &raw);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn series_keys_prefer_the_series_row_then_a_stable_chapter_anchor() {
        assert_eq!(
            series_pref_key(Some(9), &[4, 5], Some(5)).as_deref(),
            Some("comic.series.reading.9")
        );
        assert_eq!(
            series_pref_key(None, &[12, 7, 20], Some(12)).as_deref(),
            Some("comic.series.reading.first.7")
        );
        assert_eq!(
            series_pref_key(None, &[], Some(3)).as_deref(),
            Some("comic.series.reading.book.3")
        );
        assert_eq!(series_pref_key(None, &[], None), None);
    }

    #[test]
    fn globals_round_trip_and_canonicalise_legacy_spellings() {
        let catalog = Catalog::open_in_memory().expect("memory catalog");
        let settings = ComicReadingSettings {
            direction: ReadingDirection::Webtoon,
            page_style: PageStyle::Fade,
            fit_mode: FitMode::Screen,
            two_page_gap: 99,
            strip_gap: false,
            zoom_percent: 999,
            auto_next_chapter: false,
            mark_read_on_advance: false,
            image: ComicImageSettings {
                background: ComicBackground::Grey,
                brightness: 140,
                contrast: -25,
                warmth: 30,
                grayscale: true,
                invert_colours: false,
                crop_blank_borders: false,
            },
        }
        .canonical();
        assert_eq!(settings.direction, ReadingDirection::Ltr);
        assert_eq!(settings.page_style, PageStyle::LongStrip);
        assert_eq!(settings.two_page_gap, 48);
        assert_eq!(settings.zoom_percent, MAX_ZOOM_PERCENT);
        assert_eq!(settings.image.brightness, ComicImageSettings::MAX_ADJUSTMENT);

        settings.write_globals(&catalog);
        assert_eq!(ComicReadingSettings::load_globals(&catalog), settings);
    }

    #[test]
    fn a_series_record_is_present_or_absent_not_half_there() {
        let catalog = Catalog::open_in_memory().expect("memory catalog");
        let key = "comic.series.reading.42";
        assert!(ComicReadingSettings::load_series(&catalog, key).is_none());

        let settings = ComicReadingSettings {
            direction: ReadingDirection::Rtl,
            zoom_percent: 45,
            ..ComicReadingSettings::default()
        };
        settings.save_series(&catalog, key);
        assert_eq!(
            ComicReadingSettings::load_series(&catalog, key),
            Some(settings)
        );

        ComicReadingSettings::clear_series(&catalog, key);
        assert!(ComicReadingSettings::load_series(&catalog, key).is_none());
        catalog.set_pref(key, "{not json");
        assert!(ComicReadingSettings::load_series(&catalog, key).is_none());
    }

    #[test]
    fn legacy_per_book_values_overlay_only_the_fields_they_stored() {
        let catalog = Catalog::open_in_memory().expect("memory catalog");
        let globals = ComicReadingSettings {
            fit_mode: FitMode::Height,
            zoom_percent: 125,
            ..ComicReadingSettings::default()
        };
        assert!(ComicReadingSettings::legacy_book(&catalog, 8, &globals).is_none());

        catalog.set_pref("book.8.comic.direction", "rtl");
        catalog.set_pref("book.8.comic.page_style", "double");
        let migrated =
            ComicReadingSettings::legacy_book(&catalog, 8, &globals).expect("legacy values");
        assert_eq!(migrated.direction, ReadingDirection::Rtl);
        assert_eq!(migrated.page_style, PageStyle::Double);
        assert_eq!(migrated.fit_mode, FitMode::Height);
        assert_eq!(migrated.zoom_percent, 125);

        ComicReadingSettings::clear_legacy_book(&catalog, 8);
        assert!(ComicReadingSettings::legacy_book(&catalog, 8, &globals).is_none());
    }

    #[test]
    fn saved_presets_round_trip_through_the_global_pref() {
        let catalog = Catalog::open_in_memory().expect("memory catalog");
        let manga = ComicReadingSettings {
            direction: ReadingDirection::Rtl,
            page_style: PageStyle::Double,
            zoom_percent: 125,
            image: ComicImageSettings {
                brightness: 12,
                ..ComicImageSettings::default()
            },
            ..ComicReadingSettings::default()
        };
        let mut presets = Vec::new();
        assert!(!upsert_preset(&mut presets, "  Manga   night ", manga.clone()));
        assert!(!upsert_preset(
            &mut presets,
            "Western",
            ComicReadingSettings::default()
        ));
        store_presets(&catalog, &presets);

        let loaded = load_presets(&catalog);
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].name, "Manga night");
        assert_eq!(loaded[0].settings, manga);
        assert_eq!(loaded[1].name, "Western");
        assert!(loaded[0].summary().contains("RTL · Spread"));
        assert!(loaded[0].summary().contains("Image adjusted"));
    }

    #[test]
    fn preset_names_overwrite_case_insensitively_and_remove_the_same_way() {
        let mut presets = Vec::new();
        assert!(!upsert_preset(
            &mut presets,
            "Manga",
            ComicReadingSettings::default()
        ));
        let replacement = ComicReadingSettings {
            zoom_percent: 80,
            ..ComicReadingSettings::default()
        };
        assert!(upsert_preset(&mut presets, "manga", replacement.clone()));
        assert_eq!(presets.len(), 1);
        assert_eq!(presets[0].name, "manga");
        assert_eq!(presets[0].settings, replacement);

        assert!(remove_preset(&mut presets, "MANGA"));
        assert!(presets.is_empty());
        assert!(!remove_preset(&mut presets, "MANGA"));
    }

    #[test]
    fn malformed_and_duplicate_preset_records_are_ignored_safely() {
        let catalog = Catalog::open_in_memory().expect("memory catalog");
        catalog.set_pref(COMIC_PRESETS_PREF, "{not json");
        assert!(load_presets(&catalog).is_empty());

        catalog.set_pref(
            COMIC_PRESETS_PREF,
            r#"[
                {"name":"  ", "settings":{}},
                {"name":"One", "settings":{"direction":"Webtoon", "zoom_percent":999}},
                {"name":"one", "settings":{}}
            ]"#,
        );
        let loaded = load_presets(&catalog);
        assert_eq!(loaded.len(), 1);
        assert_eq!(loaded[0].name, "One");
        assert_eq!(loaded[0].settings.page_style, PageStyle::LongStrip);
        assert_eq!(loaded[0].settings.zoom_percent, MAX_ZOOM_PERCENT);
    }
}
