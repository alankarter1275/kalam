//! Chapter identity and navigation: the engine owns everything else.
//!
//! The WebKit reader built an HTML string per chapter, warmed the next
//! file, and injected chapter bodies into a live DOM. The engine opens
//! the EPUB itself and lays chapters out on demand, so what is left here
//! is the two things the page still owns: the chapter's *name* (from the
//! table of contents, captured at open) and the jump that the TOC,
//! bookmarks and the position callback all go through.

use super::engine;
use super::mod_model::ReaderModel;

impl ReaderModel {
    /// The title of the chapter on screen, for the pill and the sidebar
    /// header. Falls back the same way the engine's divider rule does.
    pub(crate) fn current_chapter_title(&self) -> &str {
        self.chapter_titles
            .get(self.chapter)
            .map(String::as_str)
            .unwrap_or("Reading")
    }

    /// Jump to a chapter, or to a fraction of the way through it. The
    /// engine reports where it landed through the position callback,
    /// which saves progress and reloads the sidebars.
    pub(crate) fn go_chapter(&mut self, idx: usize, frac: f64) {
        if idx >= self.chapter_count {
            return;
        }
        self.flush_annotation_note_draft();
        self.editing_annotation = None;
        engine::dismiss(self.selection_chip.take());
        engine::dismiss(self.dict_popover.take());
        self.with_view(|v| {
            v.goto_chapter(idx, frac);
        });
        // The position callback sets chapter/fraction, saves progress and
        // reloads the sidebars once the page is on screen.
    }
}

pub(crate) fn chapter_label(model: &ReaderModel, chapter_index: usize) -> String {
    model
        .chapter_titles
        .get(chapter_index)
        .cloned()
        .unwrap_or_else(|| format!("Ch {}", chapter_index + 1))
}
