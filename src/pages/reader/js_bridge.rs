//! Dictionary lookups and the popover that shows them.
//!
//! This file used to be the whole WebKit bridge: it built highlight
//! JSON, injected it with `eval_js`, and decoded the JS payloads the
//! page sent back. The engine replaced all of that — highlighting is a
//! method call now, and taps arrive as `ReaderMsg`s — so what is left
//! is what was never about WebKit at all: the dictionary query and the
//! GTK popover that draws an entry.

use super::engine;
use super::mod_model::ReaderModel;
use crate::db::{DictEntry, EntryData, PhraseLookup};
use gtk::prelude::*;
use relm4::ComponentSender;

impl ReaderModel {
    pub(crate) fn lookup_dict(&self, query: &str, limit: usize) -> Vec<DictEntry> {
        crate::timing::span("dict_lookup");
        let results = if query.split_whitespace().count() > 1 {
            match self
                .service
                .catalog()
                .search_phrase(query, limit)
                .unwrap_or(PhraseLookup::Empty)
            {
                PhraseLookup::Phrase(hits) => hits,
                PhraseLookup::Breakdown(parts) => parts
                    .iter()
                    .filter_map(|(_, hits)| hits.first().cloned())
                    .take(limit)
                    .collect(),
                PhraseLookup::Empty => Vec::new(),
            }
        } else {
            match self.service.catalog().search_dict(query, limit) {
                Ok(hits) => hits,
                Err(err) => {
                    crate::notify::error("Dictionary search failed", &err.to_string());
                    Vec::new()
                }
            }
        };
        let _ = self.service.catalog().log_dict_lookup(
            query,
            Some(self.book_id),
            Some(self.chapter as i64),
            self.dict_context.as_deref(),
            !results.is_empty(),
        );
        crate::timing::span_end("dict_lookup");
        results
    }
}

impl ReaderModel {
    /// Show an entry in the dictionary popover, pointed at `rect`.
    ///
    /// `saved` is Kalam's answer to "is this word already in the Words
    /// sidebar", `hint` the sense the sentence around the tap suggests
    /// (`None` when the hint preference is off).
    pub(crate) fn show_dict(
        &mut self,
        data: &EntryData,
        saved: bool,
        hint: Option<usize>,
        rect: gtk::gdk::Rectangle,
        sender: &ComponentSender<ReaderModel>,
    ) {
        // The popover being replaced reports itself closed as it goes;
        // ClearDict has to know that report is not the user's.
        if self.dict_popover.is_some() {
            self.dict_suppress_clear = self.dict_suppress_clear.saturating_add(1);
        }
        engine::dismiss(self.dict_popover.take());
        let Some(view) = &self.view else { return };
        let pronunciation = crate::db::pronunciation_for(&data.word).map(|p| format!("/{p}"));
        let card = engine::DictCard::from_entry(data, pronunciation, saved, hint);
        let popover = engine::build_dict_popover(view.widget().upcast_ref(), &rect, &card, sender);
        popover.popup();
        self.dict_anchor = Some(rect);
        self.dict_popover = Some(popover);
    }
}

pub(crate) fn truncate_def(s: &str, n: usize) -> String {
    let mut chars = s.chars();
    let head: String = chars.by_ref().take(n).collect();
    if chars.next().is_none() {
        head
    } else {
        format!("{head}\u{2026}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_def_keeps_short_text_untouched() {
        assert_eq!(truncate_def("short", 180), "short");
        assert_eq!(truncate_def("abcde", 5), "abcde");
    }

    #[test]
    fn truncate_def_cuts_long_text_and_marks_it() {
        assert_eq!(truncate_def("abcdef", 5), "abcde…");
    }

    #[test]
    fn truncate_def_does_not_panic_on_multibyte_text() {
        let cases = [
            "café — a small restaurant serving coffee",
            "\u{2018}bank\u{2019} the side of a river",
            "/ˈbæŋk/ pronunciation of the headword",
            "銀行 — a financial institution",
            "ααααααααααααααααααααααααααααα",
        ];
        for case in cases {
            for n in 0..12 {
                let out = truncate_def(case, n);
                let expected: String = case.chars().take(n).collect();
                assert!(
                    out.starts_with(&expected),
                    "truncate_def({case:?}, {n}) = {out:?} lost the prefix"
                );
                assert!(out.chars().count() <= n + 1, "cut {n} produced {out:?}");
            }
        }
    }

    #[test]
    fn truncate_def_counts_characters_not_bytes() {
        assert_eq!(truncate_def("ααααα", 5), "ααααα");
        assert_eq!(truncate_def("αααααα", 5), "ααααα…");
    }
}
