use crate::db::{Catalog, SavedWord};
use crate::service::LibraryService;
use gtk::prelude::*;
use relm4::prelude::*;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug)]
pub enum SavedWordsOut {
    #[allow(dead_code)]
    OpenBook { book_id: i64 },
}

#[derive(Debug)]
pub enum SavedWordsMsg {
    SearchChanged(String),
    Delete(i64),
    ToggleKnown(i64),
    FilterAll,
    FilterToReview,
    FilterKnown,
    /// Phase 10: propose repeat-looked-up words as a study set.
    SuggestFromHistory,
    /// Phase 10: a suggested word chip was clicked — search for it.
    SearchWord(String),
    ExportCsv,
    ExportAnki,
    ExportMarkdown,
    Refresh,
}

pub struct SavedWordsModel {
    service: LibraryService,
    query: String,
    words: Vec<SavedWord>,
    status: String,
    /// Phase 7 review scope: None = all, Some(false) = to review,
    /// Some(true) = known.
    filter: Option<bool>,
}

#[relm4::component(pub)]
impl Component for SavedWordsModel {
    type Init = Arc<Catalog>;
    type Input = SavedWordsMsg;
    type Output = SavedWordsOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,
            set_hexpand: true,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 10,
                gtk::Label {
                    set_label: "Saved words",
                    add_css_class: "kalam-page-title",
                    set_halign: gtk::Align::Start,
                    set_hexpand: true,
                },
                gtk::Button {
                    set_child: Some(&crate::icons::symbolic_with_classes(
                        "view-refresh-symbolic",
                        16,
                        &["kalam-inline-icon"],
                    )),
                    add_css_class: "kalam-secondary-btn",
                    connect_clicked => SavedWordsMsg::Refresh,
                },
            },
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 6,
                #[name = "search_entry"]
                gtk::SearchEntry {
                    set_placeholder_text: Some("Search words…"),
                    set_hexpand: true,
                    connect_search_changed[sender] => move |e| {
                        sender.input(SavedWordsMsg::SearchChanged(e.text().to_string()));
                    },
                },
            },
            // Phase 7: review scope + exports.
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 6,
                #[name = "filter_box"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 6,
                    #[name = "filter_all"]
                    gtk::ToggleButton {
                        set_label: "All",
                        add_css_class: "kalam-secondary-btn",
                    },
                    #[name = "filter_review"]
                    gtk::ToggleButton {
                        set_label: "To review",
                        add_css_class: "kalam-secondary-btn",
                    },
                    #[name = "filter_known"]
                    gtk::ToggleButton {
                        set_label: "Known",
                        add_css_class: "kalam-secondary-btn",
                    },
                },
                gtk::Box {
                    set_hexpand: true,
                },
                gtk::Button {
                    set_label: "Suggest from history",
                    add_css_class: "kalam-btn-outlined",
                    set_halign: gtk::Align::Center,
                    connect_clicked => SavedWordsMsg::SuggestFromHistory,
                },
                gtk::Button {
                    set_label: "Export CSV",
                    add_css_class: "kalam-btn-outlined",
                    set_halign: gtk::Align::Center,
                    connect_clicked => SavedWordsMsg::ExportCsv,
                },
                gtk::Button {
                    set_label: "Export Anki",
                    add_css_class: "kalam-btn-outlined",
                    set_halign: gtk::Align::Center,
                    connect_clicked => SavedWordsMsg::ExportAnki,
                },
                gtk::Button {
                    set_label: "Export Markdown",
                    add_css_class: "kalam-btn-outlined",
                    set_halign: gtk::Align::Center,
                    connect_clicked => SavedWordsMsg::ExportMarkdown,
                },
            },
            // Phase 10: repeat-lookup study set, filled by
            // SavedWordsMsg::SuggestFromHistory.
            #[name = "suggest_box"]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 6,
                set_visible: false,
            },
            #[name = "status_label"]
            gtk::Label {
                add_css_class: "kalam-muted",
                set_halign: gtk::Align::Start,
                set_wrap: true,
            },
            #[name = "scroll"]
            gtk::ScrolledWindow {
                set_vexpand: true,
                set_hexpand: true,
                #[name = "list_box"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 10,
                    set_margin_top: 6,
                }
            },
        }
    }

    fn init(
        catalog: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = SavedWordsModel {
            service: LibraryService::new(catalog),
            query: String::new(),
            words: Vec::new(),
            status: String::new(),
            filter: None,
        };
        let widgets = view_output!();
        group_toggles(&widgets.filter_box);
        widgets.filter_all.set_active(true);
        let s = sender.clone();
        widgets.filter_all.connect_toggled(move |b| {
            if b.is_active() {
                s.input(SavedWordsMsg::FilterAll);
            }
        });
        let s = sender.clone();
        widgets.filter_review.connect_toggled(move |b| {
            if b.is_active() {
                s.input(SavedWordsMsg::FilterToReview);
            }
        });
        let s = sender.clone();
        widgets.filter_known.connect_toggled(move |b| {
            if b.is_active() {
                s.input(SavedWordsMsg::FilterKnown);
            }
        });
        let mut model = model;
        model.reload();
        rebuild(&widgets.list_box, &model.words, &sender);
        widgets.status_label.set_label(&model.status);
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
            SavedWordsMsg::SearchChanged(q) => {
                self.query = q;
                self.reload();
                rebuild(&widgets.list_box, &self.words, &sender);
                widgets.status_label.set_label(&self.status);
            }
            SavedWordsMsg::Delete(id) => {
                let word = self
                    .words
                    .iter()
                    .find(|w| w.id == id)
                    .map(|w| w.word.clone())
                    .unwrap_or_default();
                crate::notify::outcome_info(
                    self.service.catalog().delete_saved_word(id),
                    "Word deleted",
                    &word,
                    "Could not delete the word",
                );
                self.reload();
                rebuild(&widgets.list_box, &self.words, &sender);
                widgets.status_label.set_label(&self.status);
            }
            SavedWordsMsg::ToggleKnown(id) => {
                let word = self
                    .words
                    .iter()
                    .find(|w| w.id == id)
                    .map(|w| (w.word.clone(), !w.known))
                    .unwrap_or((String::new(), true));
                crate::notify::outcome_info(
                    self.service.catalog().set_saved_word_known(id, word.1),
                    if word.1 {
                        "Marked as known"
                    } else {
                        "Back to review"
                    },
                    &word.0,
                    "Could not update the word",
                );
                self.reload();
                rebuild(&widgets.list_box, &self.words, &sender);
                widgets.status_label.set_label(&self.status);
            }
            SavedWordsMsg::FilterAll => {
                self.filter = None;
                widgets.filter_all.set_active(true);
                widgets.filter_review.set_active(false);
                widgets.filter_known.set_active(false);
                self.reload();
                rebuild(&widgets.list_box, &self.words, &sender);
                widgets.status_label.set_label(&self.status);
            }
            SavedWordsMsg::FilterToReview => {
                self.filter = Some(false);
                widgets.filter_all.set_active(false);
                widgets.filter_review.set_active(true);
                widgets.filter_known.set_active(false);
                self.reload();
                rebuild(&widgets.list_box, &self.words, &sender);
                widgets.status_label.set_label(&self.status);
            }
            SavedWordsMsg::FilterKnown => {
                self.filter = Some(true);
                widgets.filter_all.set_active(false);
                widgets.filter_review.set_active(false);
                widgets.filter_known.set_active(true);
                self.reload();
                rebuild(&widgets.list_box, &self.words, &sender);
                widgets.status_label.set_label(&self.status);
            }
            SavedWordsMsg::SuggestFromHistory => {
                let repeats = self
                    .service
                    .catalog()
                    .repeat_lookup_words(20)
                    .unwrap_or_default();
                if repeats.is_empty() {
                    crate::notify::info(
                        "No repeat lookups yet",
                        "Words you look up more than once in the reader will show up here.",
                    );
                }
                populate_suggestions(&widgets.suggest_box, &repeats, &sender);
            }
            SavedWordsMsg::SearchWord(word) => {
                self.query = word.clone();
                widgets.search_entry.set_text(&word);
                self.reload();
                rebuild(&widgets.list_box, &self.words, &sender);
                widgets.status_label.set_label(&self.status);
            }
            SavedWordsMsg::ExportCsv => match export_saved_words_csv(self.service.catalog()) {
                Ok((n, path)) => {
                    crate::notify::compact(
                        &format!("{n} word{} exported", if n == 1 { "" } else { "s" }),
                        &path.display().to_string(),
                    );
                }
                Err(e) => crate::notify::error("Could not export words", &e),
            },
            SavedWordsMsg::ExportAnki => match export_saved_words_anki(self.service.catalog()) {
                Ok((n, path)) => {
                    crate::notify::compact(
                        &format!(
                            "{n} word{} exported for Anki",
                            if n == 1 { "" } else { "s" }
                        ),
                        &path.display().to_string(),
                    );
                }
                Err(e) => crate::notify::error("Could not export words", &e),
            },
            SavedWordsMsg::ExportMarkdown => {
                let out_path = crate::paths::home_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("Kalam-Export.md");
                match self.service.catalog().export_reading_data_markdown(&out_path) {
                    Ok(n) => {
                        crate::notify::success(
                            &format!("{n} item{} exported", if n == 1 { "" } else { "s" }),
                            &out_path.display().to_string(),
                        );
                    }
                    Err(e) => crate::notify::error("Could not export reading data", &e.to_string()),
                }
            }
            SavedWordsMsg::Refresh => {
                self.reload();
                rebuild(&widgets.list_box, &self.words, &sender);
                widgets.status_label.set_label(&self.status);
            }
        }
        self.update_view(widgets, sender);
    }
}

impl SavedWordsModel {
    fn reload(&mut self) {
        let snap = self.service.words(&self.query, self.filter);
        // This page reports failures in its status line, as it always has.
        if let Some(e) = snap.errors.first() {
            self.words.clear();
            self.status = format!("DB error: {e}");
            return;
        }
        let n = snap.words.len();
        let total = snap.total;
        let known = snap.known;
        self.words = snap.words;
        let scope = match self.filter {
            None => format!("{n} of {total} word{}", if total == 1 { "" } else { "s" }),
            Some(false) => format!(
                "{n} to review · {total} word{} saved",
                if total == 1 { "" } else { "s" }
            ),
            Some(true) => format!(
                "{n} known · {total} word{} saved",
                if total == 1 { "" } else { "s" }
            ),
        };
        self.status = if self.query.trim().is_empty() {
            if total == 0 {
                "No saved words yet — lookup a word in the reader (D or chip Aa) and save it."
                    .into()
            } else {
                format!("{scope} · {known} known")
            }
        } else {
            format!(
                "{n} result{} for \"{}\"",
                if n == 1 { "" } else { "s" },
                self.query
            )
        };
    }
}

/// Radio-group the filter toggles (same pattern as the History page).
fn group_toggles(box_: &gtk::Box) {
    let mut leader: Option<gtk::ToggleButton> = None;
    let mut child = box_.first_child();
    while let Some(w) = child {
        let next = w.next_sibling();
        if let Ok(btn) = w.downcast::<gtk::ToggleButton>() {
            match &leader {
                Some(l) => btn.set_group(Some(l)),
                None => leader = Some(btn),
            }
        }
        child = next;
    }
}

fn rebuild(list: &gtk::Box, words: &[SavedWord], sender: &ComponentSender<SavedWordsModel>) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    if words.is_empty() {
        let l = gtk::Label::new(Some("No saved words."));
        l.add_css_class("kalam-placeholder");
        list.append(&l);
        return;
    }
    for w in words {
        let row = gtk::Box::new(gtk::Orientation::Vertical, 6);
        row.add_css_class("kalam-word-row");
        row.set_margin_bottom(8);
        if w.known {
            row.add_css_class("kalam-word-row-known");
        }

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let word_l = gtk::Label::new(Some(&w.word));
        word_l.add_css_class("kalam-word-title");
        word_l.set_halign(gtk::Align::Start);
        word_l.set_hexpand(true);
        header.append(&word_l);

        if let Some(dict) = &w.dict_name {
            let badge = gtk::Label::new(Some(dict.as_str()));
            badge.add_css_class("kalam-chip");
            header.append(&badge);
        }

        // Phase 7: mark known / back to review.
        let known_btn = gtk::Button::new();
        known_btn.set_child(Some(&crate::icons::symbolic_with_classes(
            "object-select-symbolic",
            16,
            &["kalam-inline-icon"],
        )));
        known_btn.add_css_class("kalam-secondary-btn");
        if w.known {
            known_btn.add_css_class("kalam-known-btn-active");
            known_btn.set_tooltip_text(Some("Mark as to review"));
        } else {
            known_btn.set_tooltip_text(Some("Mark as known"));
        }
        let id = w.id;
        let s = sender.clone();
        known_btn.connect_clicked(move |_| s.input(SavedWordsMsg::ToggleKnown(id)));
        header.append(&known_btn);

        let del = gtk::Button::new();
        del.set_child(Some(&crate::icons::symbolic_with_classes(
            "window-close-symbolic",
            16,
            &["kalam-inline-icon"],
        )));
        del.add_css_class("kalam-secondary-btn");
        let id = w.id;
        let s = sender.clone();
        del.connect_clicked(move |_| s.input(SavedWordsMsg::Delete(id)));
        header.append(&del);

        row.append(&header);

        let def_l = gtk::Label::new(Some(&w.definition));
        def_l.add_css_class("kalam-muted");
        def_l.set_wrap(true);
        def_l.set_xalign(0.0);
        row.append(&def_l);

        if let Some(ctx) = &w.context_text {
            if !ctx.trim().is_empty() {
                let ctx_l = gtk::Label::new(Some(&format!("Context: {}", ctx)));
                ctx_l.add_css_class("kalam-placeholder");
                ctx_l.set_wrap(true);
                ctx_l.set_xalign(0.0);
                row.append(&ctx_l);
            }
        }

        let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
        sep.set_margin_top(6);
        row.append(&sep);

        list.append(&row);
    }
}

// ---------------------------------------------------------------------------
// Phase 7 exports — mirror the ~/Quotes.md pattern (fixed home path, toast
// with the resulting path). Pure string builders are unit-tested.
// ---------------------------------------------------------------------------

/// Quote one CSV field, and defuse it if a spreadsheet would treat it as a
/// formula.
///
/// Excel and LibreOffice evaluate any cell whose text begins with `=`, `+`,
/// `-` or `@`. Dictionary content hits this constantly and innocently:
/// suffix headwords like `-ness`, and definitions written as `- to do X`.
/// The result ranges from a cell showing `#NAME?` instead of the definition
/// to, with a crafted pack, a formula the spreadsheet offers to execute.
///
/// The standard defusal is a leading apostrophe, which spreadsheets consume
/// as "treat the rest as text" and other CSV readers see as one stray
/// character. Prefixing forces quoting too, so the apostrophe cannot be
/// mistaken for part of the delimiter structure.
fn csv_field(value: &str) -> String {
    let dangerous = value.starts_with(['=', '+', '-', '@']);
    let needs_quoting =
        dangerous || value.contains(',') || value.contains('"') || value.contains('\n');
    let escaped = value.replace('"', "\"\"");
    if !needs_quoting {
        return escaped;
    }
    if dangerous {
        format!("\"'{escaped}\"")
    } else {
        format!("\"{escaped}\"")
    }
}

fn saved_words_csv(words: &[SavedWord]) -> String {
    let mut out = String::new();
    out.push_str("word,definition,context,dictionary,known,created_at\n");
    for w in words {
        let context = w.context_text.as_deref().unwrap_or("").replace('\n', " ");
        out.push_str(&format!(
            "{},{},{},{},{},{}\n",
            csv_field(&w.word),
            csv_field(&w.definition.replace('\n', " ")),
            csv_field(&context),
            csv_field(w.dict_name.as_deref().unwrap_or("")),
            if w.known { "known" } else { "to review" },
            w.created_at,
        ));
    }
    out
}

fn saved_words_anki_tsv(words: &[SavedWord]) -> String {
    // Anki's default import splits on tabs: front = word, back = definition,
    // extra = context. Newlines are kept so the back field can wrap.
    let mut out = String::new();
    out.push_str("#separator:tab\n#html:false\n");
    out.push_str("#columns:word\tdefinition\tcontext\n");
    for w in words {
        let context = w.context_text.as_deref().unwrap_or("");
        out.push_str(&format!(
            "{}\t{}\t{}\n",
            w.word.replace(['\t', '\n'], " "),
            w.definition.replace('\t', " "),
            context.replace('\t', " "),
        ));
    }
    out
}



/// Export every saved word to `~/SavedWords.csv`. Shared shape with the
/// ~/Quotes.md export: fixed home path, count + path returned for a toast.
pub fn export_saved_words_csv(catalog: &Arc<Catalog>) -> Result<(usize, PathBuf), String> {
    let words = catalog
        .list_saved_words("", None)
        .map_err(|e| format!("{e}"))?;
    let csv = saved_words_csv(&words);
    let out_path = crate::paths::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("SavedWords.csv");
    std::fs::write(&out_path, csv).map_err(|e| format!("{e}"))?;
    Ok((words.len(), out_path))
}

/// Export every saved word to `~/SavedWords-Anki.txt` (tab-separated,
/// Anki-importable: word / definition / context).
pub fn export_saved_words_anki(catalog: &Arc<Catalog>) -> Result<(usize, PathBuf), String> {
    let words = catalog
        .list_saved_words("", None)
        .map_err(|e| format!("{e}"))?;
    let tsv = saved_words_anki_tsv(&words);
    let out_path = crate::paths::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("SavedWords-Anki.txt");
    std::fs::write(&out_path, tsv).map_err(|e| format!("{e}"))?;
    Ok((words.len(), out_path))
}

/// Fill the repeat-lookup study-set chip row. Each chip is `word ×N`;
/// clicking one searches the saved-words list for that word.
fn populate_suggestions(
    suggest_box: &gtk::Box,
    repeats: &[(String, i64)],
    sender: &ComponentSender<SavedWordsModel>,
) {
    while let Some(child) = suggest_box.first_child() {
        suggest_box.remove(&child);
    }
    if repeats.is_empty() {
        suggest_box.set_visible(false);
        return;
    }
    let intro = gtk::Label::new(Some("Repeat lookups:"));
    intro.add_css_class("kalam-muted");
    intro.set_valign(gtk::Align::Center);
    suggest_box.append(&intro);
    for (word, n) in repeats {
        let chip = gtk::Button::with_label(&format!("{word} ×{n}"));
        chip.add_css_class("kalam-btn-outlined");
        chip.add_css_class("kalam-btn-sm");
        chip.set_valign(gtk::Align::Center);
        chip.set_tooltip_text(Some(&format!(
            "Looked up {n} times — click to find it in your saved words"
        )));
        let w = word.clone();
        let s = sender.clone();
        chip.connect_clicked(move |_| s.input(SavedWordsMsg::SearchWord(w.clone())));
        suggest_box.append(&chip);
    }
    suggest_box.set_visible(true);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(id: i64, w: &str, def: &str, ctx: Option<&str>, known: bool) -> SavedWord {
        SavedWord {
            id,
            word: w.to_string(),
            definition: def.to_string(),
            dict_name: Some("WordNet".to_string()),
            book_id: None,
            chapter_index: None,
            context_text: ctx.map(str::to_string),
            created_at: "2026-09-01T00:00:00Z".to_string(),
            known,
        }
    }

    #[test]
    fn csv_escapes_commas_quotes_and_newlines() {
        let words = vec![
            word(
                1,
                "serendipity",
                "a happy accident",
                Some("luck, chance"),
                false,
            ),
            word(2, "quoted\"word", "line one\nline two", Some("plain"), true),
        ];
        let csv = saved_words_csv(&words);
        assert!(csv.starts_with("word,definition,context,dictionary,known,created_at\n"));
        assert!(csv.contains("\"luck, chance\""));
        assert!(csv.contains("\"quoted\"\"word\""));
        // Definition newlines collapse to spaces so each row is one line.
        assert!(csv.contains(",line one line two,"));
        assert!(csv.contains(",known,"));
        assert!(csv.contains(",to review,"));
    }

    #[test]
    fn csv_defuses_spreadsheet_formulas() {
        // Suffix headwords and dash-led definitions are ordinary dictionary
        // content, and both begin with a character that Excel and
        // LibreOffice treat as the start of a formula.
        for danger in ["-ness", "=1+1", "+foo", "@bar", "-- to remove"] {
            let out = csv_field(danger);
            assert!(
                out.starts_with("\"'"),
                "{danger:?} must be quoted and apostrophe-prefixed, got {out}"
            );
        }
    }

    #[test]
    fn csv_leaves_ordinary_fields_alone() {
        // The defusal must not fire on normal text, or every cell in the
        // export grows a stray apostrophe.
        assert_eq!(csv_field("bank"), "bank");
        assert_eq!(csv_field("a river bank"), "a river bank");
        // A dash *inside* the value is not a leading dash.
        assert_eq!(csv_field("well-being"), "well-being");
    }

    #[test]
    fn anki_tsv_is_tab_separated_with_header() {
        let words = vec![
            word(
                1,
                "serendipity",
                "a happy accident",
                Some("luck, chance"),
                false,
            ),
            word(2, "wander", "to walk aimlessly", None, true),
        ];
        let tsv = saved_words_anki_tsv(&words);
        assert!(tsv.starts_with("#separator:tab\n#html:false\n"));
        assert!(tsv.contains("#columns:word\tdefinition\tcontext\n"));
        assert!(tsv.contains("serendipity\ta happy accident\tluck, chance\n"));
        assert!(tsv.contains("wander\tto walk aimlessly\t\n"));
        // Tabs/newlines are stripped from every field so Anki sees 3 columns.
        for line in tsv.lines().skip(3) {
            assert_eq!(line.split('\t').count(), 3, "line: {line}");
        }
    }
}
