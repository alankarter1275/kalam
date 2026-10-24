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

use super::types::{FitMode, PageStyle, ReadingDirection};
use crate::db::Catalog;
use serde::{Deserialize, Serialize};

pub(crate) const MIN_ZOOM_PERCENT: u32 = 25;
pub(crate) const MAX_ZOOM_PERCENT: u32 = 300;

/// Everything shown by the comics Reading pane, as one value. Keeping
/// the whole pane in one record is the all-or-nothing rule: a series
/// either follows the global choices or carries this complete set.
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
        }
        .canonical();
        assert_eq!(settings.direction, ReadingDirection::Ltr);
        assert_eq!(settings.page_style, PageStyle::LongStrip);
        assert_eq!(settings.two_page_gap, 48);
        assert_eq!(settings.zoom_percent, MAX_ZOOM_PERCENT);

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
}
