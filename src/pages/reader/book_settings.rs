//! Per-book reading settings (decision 147): the Reading pane's global
//! `reader.*` defaults are what every book opens with, and a book can
//! keep its own instead. The override is all-or-nothing (decision 40):
//! one JSON value per book whose *presence* is the override switch.
//!
//! Stored under `book.reading.<id>` — deliberately not `reader.*`,
//! which `libraries::is_global_pref` classifies as an app setting in
//! the shared prefs file. Book ids only exist inside one library, so
//! the override belongs in that library's database next to the book.

use super::engine;
use super::mod_model::ReaderModel;
use crate::db::Catalog;
use gtk::prelude::WidgetExt;
use serde::{Deserialize, Serialize};

/// Everything the reader's Reading pane edits, in one value. The
/// Dictionary section's history toggle is *not* here: it is a
/// library-shared privacy pref (`dict_history_enabled`), not a
/// reading-appearance default, so the pane edits it globally as
/// before.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct BookReadingSettings {
    /// A theme id (`<palette>-light|dark`) when the book's page does
    /// not follow the app theme (decision 145); `None` follows.
    pub(crate) theme: Option<String>,
    pub(crate) font_family: Option<String>,
    pub(crate) font_px: u32,
    pub(crate) line_height: f32,
    pub(crate) column_px: u32,
    pub(crate) justify: bool,
    pub(crate) publisher_styles: bool,
    pub(crate) scrolled: bool,
    pub(crate) dual_page: bool,
    pub(crate) wheel_step: i32,
    pub(crate) arrow_step: i32,
    pub(crate) autohide_cursor: bool,
    pub(crate) word_memory_scope: String,
}

fn pref_key(book_id: i64) -> String {
    format!("book.reading.{book_id}")
}

impl BookReadingSettings {
    /// The standing defaults: the same global `reader.*` prefs the
    /// Settings Reading pane edits, with the same fallbacks the reader
    /// has always opened with.
    pub(crate) fn load_globals(catalog: &Catalog) -> Self {
        Self {
            theme: None,
            // An empty stored family is the "Default" choice spelled
            // out (the old reader wrote "" for it): canonicalize to
            // None here so a written-and-read-back value compares
            // equal to itself in `differs_from`.
            font_family: catalog
                .get_pref("reader.font_family")
                .filter(|f| !f.trim().is_empty()),
            font_px: catalog.get_pref_i64("reader.font_px", 17).clamp(13, 24) as u32,
            line_height: catalog
                .get_pref("reader.line_height")
                .and_then(|v| v.parse::<f32>().ok())
                .unwrap_or(1.8)
                .clamp(1.3, 2.5),
            column_px: catalog
                .get_pref_i64("reader.column_px", 620)
                .clamp(400, 860) as u32,
            justify: catalog.get_pref_i64("reader.justify", 0) != 0,
            publisher_styles: catalog.get_pref_i64("reader.publisher_styles", 1) != 0,
            scrolled: catalog.get_pref_i64(engine::PREF_SCROLLED, 0) != 0,
            dual_page: catalog.get_pref_i64("reader.dual_page", 1) != 0,
            wheel_step: catalog.get_pref_i64("reader.wheel_step", 92).clamp(20, 400) as i32,
            arrow_step: catalog.get_pref_i64("reader.arrow_step", 45).clamp(10, 200) as i32,
            autohide_cursor: catalog.get_pref_i64("reader.autohide_cursor", 1) != 0,
            word_memory_scope: catalog
                .get_pref("reader.word_memory_scope")
                .unwrap_or_else(|| "library".into()),
        }
    }

    /// Write these values over the global defaults ("Use these
    /// settings as my defaults"). Only what the reader's Reading pane
    /// shows is copied (decision 147); the theme is the app's own
    /// choice and is never written from here.
    pub(crate) fn write_globals(&self, catalog: &Catalog) {
        catalog.set_pref(
            "reader.font_family",
            self.font_family.as_deref().unwrap_or(""),
        );
        catalog.set_pref("reader.font_px", &self.font_px.to_string());
        catalog.set_pref("reader.line_height", &format!("{:.1}", self.line_height));
        catalog.set_pref("reader.column_px", &self.column_px.to_string());
        catalog.set_pref("reader.justify", if self.justify { "1" } else { "0" });
        catalog.set_pref(
            "reader.publisher_styles",
            if self.publisher_styles { "1" } else { "0" },
        );
        catalog.set_pref(engine::PREF_SCROLLED, if self.scrolled { "1" } else { "0" });
        catalog.set_pref(
            "reader.dual_page",
            if self.dual_page { "1" } else { "0" },
        );
        catalog.set_pref("reader.wheel_step", &self.wheel_step.to_string());
        catalog.set_pref("reader.arrow_step", &self.arrow_step.to_string());
        catalog.set_pref(
            "reader.autohide_cursor",
            if self.autohide_cursor { "1" } else { "0" },
        );
        catalog.set_pref("reader.word_memory_scope", &self.word_memory_scope);
    }

    /// The book's override, if it keeps one. An unparsable value reads
    /// as no override rather than failing the open — the book just
    /// follows the defaults again.
    pub(crate) fn load_book(catalog: &Catalog, book_id: i64) -> Option<Self> {
        let raw = catalog.get_pref(&pref_key(book_id))?;
        serde_json::from_str(&raw).ok()
    }

    pub(crate) fn save_book(&self, catalog: &Catalog, book_id: i64) {
        if let Ok(raw) = serde_json::to_string(self) {
            catalog.set_pref(&pref_key(book_id), &raw);
        }
    }

    pub(crate) fn clear_book(catalog: &Catalog, book_id: i64) {
        catalog.delete_pref(&pref_key(book_id));
    }

    /// Whether these settings would paint a different page from the
    /// standing defaults — the "Use as defaults" pill's whole test.
    /// The theme is excluded: it has no global counterpart to copy to,
    /// and a book keeping its own theme still matches the defaults in
    /// everything the pill can act on.
    pub(crate) fn differs_from(&self, globals: &Self) -> bool {
        self.font_family != globals.font_family
            || self.font_px != globals.font_px
            || (self.line_height - globals.line_height).abs() > f32::EPSILON
            || self.column_px != globals.column_px
            || self.justify != globals.justify
            || self.publisher_styles != globals.publisher_styles
            || self.scrolled != globals.scrolled
            || self.dual_page != globals.dual_page
            || self.wheel_step != globals.wheel_step
            || self.arrow_step != globals.arrow_step
            || self.autohide_cursor != globals.autohide_cursor
            || self.word_memory_scope != globals.word_memory_scope
    }
}

impl ReaderModel {
    /// The reader's standing choices as one settings value — the
    /// model fields *are* the effective state, override or not.
    pub(crate) fn current_settings(&self) -> BookReadingSettings {
        BookReadingSettings {
            theme: self.theme_override.clone(),
            font_family: self.font_family.clone(),
            font_px: self.font_px,
            line_height: self.line_height,
            column_px: self.column_px,
            justify: self.justify,
            publisher_styles: self.publisher_styles,
            scrolled: self.scrolled,
            dual_page: self.dual_page,
            wheel_step: self.wheel_step,
            arrow_step: self.arrow_step,
            autohide_cursor: self.autohide_cursor,
            word_memory_scope: self.word_memory_scope.clone(),
        }
    }

    /// Record a Reading-pane edit. Editing while the book follows the
    /// defaults turns its override on (decision 147): the current
    /// effective state — the edit already applied to the fields —
    /// becomes the book's own, and the global prefs are never written
    /// from the reader.
    pub(crate) fn store_book_setting(&mut self) {
        let settings = self.current_settings();
        settings.save_book(self.service.catalog(), self.book_id);
        self.book_override = Some(settings);
    }

    /// Take a settings value wholesale: the fields, the engine, and
    /// the resolved palette. Used when the override switch turns off
    /// and the defaults come back.
    pub(crate) fn apply_settings(&mut self, settings: &BookReadingSettings) {
        self.theme_override = settings.theme.clone();
        self.font_family = settings.font_family.clone();
        self.font_px = settings.font_px;
        self.line_height = settings.line_height;
        self.column_px = settings.column_px;
        self.justify = settings.justify;
        self.publisher_styles = settings.publisher_styles;
        self.scrolled = settings.scrolled;
        self.dual_page = settings.dual_page;
        self.wheel_step = settings.wheel_step;
        self.arrow_step = settings.arrow_step;
        self.autohide_cursor = settings.autohide_cursor;
        if self.word_memory_scope != settings.word_memory_scope {
            self.word_memory_scope = settings.word_memory_scope.clone();
            self.reload_saved_words();
        }
        self.refresh_palette();
        self.with_view(|v| {
            v.set_font_family(settings.font_family.clone());
            v.set_font_px(settings.font_px as f32);
            v.set_line_height(settings.line_height);
            v.set_column_px(settings.column_px as f32);
            v.set_justify(settings.justify);
            v.set_publisher_styles(settings.publisher_styles);
            v.set_mode(engine::mode_from_pref(settings.scrolled as i64));
            v.set_dual_page(settings.dual_page);
            v.set_wheel_step(settings.wheel_step as f32);
            v.set_arrow_step(settings.arrow_step as f32);
            v.set_autohide_cursor(settings.autohide_cursor);
        });
        if let Some(bar) = &self.strip_scrollbar {
            bar.set_visible(settings.scrolled);
        }
    }

    /// Resolve the page palette: the book's own theme when it keeps
    /// one, the app theme otherwise (decisions 19/145), and hand it
    /// to the engine. An unknown stored id falls back to the app
    /// theme rather than painting in somebody else's colours.
    pub(crate) fn refresh_palette(&mut self) {
        // `choice_by_id` is total (unknown ids become the default
        // choice), so a stored id only counts when it round-trips to
        // the theme it names; anything else follows the app theme.
        let theme = self
            .theme_override
            .as_deref()
            .map(|id| (id, crate::theme::choice_by_id(id).resolve()))
            .filter(|(id, theme)| theme.id == *id)
            .map(|(_, theme)| theme)
            .unwrap_or_else(|| crate::theme::current(self.service.catalog()));
        self.palette = engine::reader_palette(&theme);
        let palette = self.palette;
        self.with_view(|v| v.set_palette(palette));
    }

    /// The "Use these settings as my defaults" pill's enable test.
    pub(crate) fn settings_differ_from_defaults(&self) -> bool {
        let globals = BookReadingSettings::load_globals(self.service.catalog());
        self.current_settings().differs_from(&globals)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn globals_round_trip_through_the_prefs() {
        let catalog = Catalog::open_in_memory().expect("memory catalog");
        let mut settings = BookReadingSettings::load_globals(&catalog);
        settings.font_px = 21;
        settings.justify = true;
        settings.word_memory_scope = "book".into();
        // The theme is not a global default: writing globals must not
        // invent one.
        settings.theme = Some("plum-dark".into());
        settings.write_globals(&catalog);
        let back = BookReadingSettings::load_globals(&catalog);
        assert_eq!(back.font_px, 21);
        assert!(back.justify);
        assert_eq!(back.word_memory_scope, "book");
        assert_eq!(back.theme, None);
        assert!(!settings.differs_from(&back));
    }

    #[test]
    fn a_book_override_is_present_or_absent_not_half_there() {
        let catalog = Catalog::open_in_memory().expect("memory catalog");
        assert!(BookReadingSettings::load_book(&catalog, 7).is_none());
        let mut settings = BookReadingSettings::load_globals(&catalog);
        settings.theme = Some("teal-dark".into());
        settings.save_book(&catalog, 7);
        let back = BookReadingSettings::load_book(&catalog, 7).expect("saved override");
        assert_eq!(back, settings);
        BookReadingSettings::clear_book(&catalog, 7);
        assert!(BookReadingSettings::load_book(&catalog, 7).is_none());
    }

    #[test]
    fn corrupt_override_reads_as_no_override() {
        let catalog = Catalog::open_in_memory().expect("memory catalog");
        catalog.set_pref("book.reading.3", "{not json");
        assert!(BookReadingSettings::load_book(&catalog, 3).is_none());
    }
}
