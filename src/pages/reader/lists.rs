//! Sidebar list rebuilders (TOC, Highlights, Notes, Bookmarks, Words) and note editing.

use super::chapter::chapter_label;
use super::chrome::connect_hover_zone;
use super::js_bridge::truncate_def;
use super::mod_model::ReaderModel;
use super::types::*;
use crate::db::{Annotation, HighlightColor, SavedWord};
use gtk::prelude::*;
use relm4::ComponentSender;

impl ReaderModel {
    pub(crate) fn reload_annotations(&mut self) {
        let chapter = self
            .service
            .catalog()
            .get_annotations_for_chapter(self.book_id, self.chapter as i64);
        let book = self
            .service
            .catalog()
            .get_annotations_for_book(self.book_id);
        match (chapter, book) {
            (Ok(ch), Ok(all)) => {
                self.chapter_annotations = ch;
                self.all_book_annotations = all;
            }
            (Err(err), _) | (_, Err(err)) => {
                crate::notify::error("Could not read your highlights", &err.to_string())
            }
        }
    }

    pub(crate) fn reload_bookmarks(&mut self) {
        match self.service.catalog().list_reading_bookmarks(self.book_id) {
            Ok(rows) => self.bookmarks = rows,
            Err(err) => crate::notify::error("Could not read your bookmarks", &err.to_string()),
        }
    }

    pub(crate) fn reload_saved_words(&mut self) {
        match self.service.catalog().list_saved_words("", None) {
            Ok(rows) => self.saved_words = rows,
            Err(err) => crate::notify::error("Could not read your saved words", &err.to_string()),
        }
    }

    /// The book's table of contents as the engine reported it at open.
    pub(crate) fn toc_entries(&self) -> Vec<kalam_reader::TocEntry> {
        self.view.as_ref().map(|view| view.toc()).unwrap_or_default()
    }

    /// Which entry the TOC scroll should centre on, as (position, total)
    /// in the list `rebuild_toc` builds from the same entries.
    pub(crate) fn toc_display_position(&self) -> Option<(usize, usize)> {
        let entries = self.toc_entries();
        let rows = toc_rows(&entries, &self.chapter_titles);
        // The last row at or before the reading position: the same rule
        // toc_active_spine_index applies, but over *rows* -- a nested part
        // heading is a row with no spine index, so the two are not the same
        // numbering and the scroll has to count every row the list drew.
        let position = rows
            .iter()
            .enumerate()
            .filter(|(_, row)| row.spine_index.is_some_and(|idx| idx <= self.chapter))
            .map(|(i, _)| i)
            .next_back()
            .unwrap_or(0);
        Some((position, rows.len()))
    }

    pub(crate) fn position_toc_scroll(&self) {
        let Some((current, total)) = self.toc_display_position() else {
            return;
        };
        let adj = self.toc_scroll.vadjustment();
        let max = (adj.upper() - adj.page_size()).max(0.0);
        if max <= 0.0 || total <= 1 {
            adj.set_value(0.0);
            return;
        }
        let content_span = adj.upper().max(adj.page_size());
        let target_center = content_span * ((current as f64 + 0.5) / total as f64);
        let target = (target_center - (adj.page_size() / 2.0)).clamp(0.0, max);
        adj.set_value(target);
    }

    pub(crate) fn close_sidebars(&mut self) {
        self.left_sidebar_open = false;
        self.right_sidebar_open = false;
        self.cancel_left_close();
        self.cancel_right_close();
    }

    pub(crate) fn flush_annotation_note_draft(&mut self) {
        let Some((id, note)) = self.annotation_note_draft.take() else {
            return;
        };
        if !self.persist_annotation_note(id, &note) {
            self.annotation_note_draft = Some((id, note));
        }
    }

    pub(crate) fn close_annotation_editor(&mut self) {
        self.flush_annotation_note_draft();
        self.editing_annotation = None;
    }

    pub(crate) fn persist_annotation_note(&mut self, id: i64, note: &str) -> bool {
        let normalized = note.trim();
        if !self
            .all_book_annotations
            .iter()
            .any(|annotation| annotation.id == id)
        {
            return false;
        }
        if self
            .all_book_annotations
            .iter()
            .find(|annotation| annotation.id == id)
            .is_some_and(|annotation| annotation.note == normalized)
        {
            return true;
        }
        if let Err(err) = self
            .service
            .catalog()
            .update_annotation_note(id, normalized)
        {
            crate::notify::error("Could not save your note", &err.to_string());
            return false;
        }

        let saved_note = normalized.to_string();
        for annotation in &mut self.all_book_annotations {
            if annotation.id == id {
                annotation.note = saved_note.clone();
            }
        }
        for annotation in &mut self.chapter_annotations {
            if annotation.id == id {
                annotation.note = saved_note.clone();
            }
        }
        true
    }

    pub(crate) fn filtered_annotations(&self) -> Vec<&Annotation> {
        let query = self.annotation_search_query.trim().to_lowercase();
        self.all_book_annotations
            .iter()
            .filter(|anno| {
                let matches_filter = match self.highlight_filter {
                    HighlightFilter::All => anno.kind == "highlight" || anno.kind == "quote",
                    HighlightFilter::Yellow => {
                        anno.kind == "highlight" && anno.color.eq_ignore_ascii_case("yellow")
                    }
                    HighlightFilter::Green => {
                        anno.kind == "highlight" && anno.color.eq_ignore_ascii_case("green")
                    }
                    HighlightFilter::Blue => {
                        anno.kind == "highlight" && anno.color.eq_ignore_ascii_case("blue")
                    }
                    HighlightFilter::Pink => {
                        anno.kind == "highlight" && anno.color.eq_ignore_ascii_case("pink")
                    }
                    HighlightFilter::Orange => {
                        anno.kind == "highlight" && anno.color.eq_ignore_ascii_case("orange")
                    }
                    HighlightFilter::Quotes => anno.kind == "quote",
                };
                if !matches_filter || query.is_empty() {
                    return matches_filter;
                }
                anno.text_excerpt.to_lowercase().contains(&query)
                    || anno.note.to_lowercase().contains(&query)
            })
            .collect()
    }

    pub(crate) fn filtered_saved_words(&self) -> Vec<&SavedWord> {
        self.saved_words
            .iter()
            .filter(|word| match self.word_scope {
                WordScope::Chapter => {
                    word.book_id == Some(self.book_id)
                        && word.chapter_index == Some(self.chapter as i64)
                }
                WordScope::Book => word.book_id == Some(self.book_id),
                WordScope::All => true,
            })
            .collect()
    }
}

/// One row of the table of contents as the sidebar draws it.
///
/// `spine_index` is what a click jumps to; a row can be a nesting heading
/// with nowhere to go (a "Part One" whose target is only its children),
/// which is why it is optional and why the list is not simply a list of
/// spine indices.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TocRow {
    pub(crate) label: String,
    pub(crate) spine_index: Option<usize>,
    pub(crate) depth: usize,
}

/// The rows the TOC list shows, depth first: a part, then its chapters,
/// then the next part. When the engine's TOC has no usable target at all,
/// the book's spine titles are the fallback — the list must never be empty
/// on a book that has chapters.
pub(crate) fn toc_rows(entries: &[kalam_reader::TocEntry], titles: &[String]) -> Vec<TocRow> {
    let mut rows = Vec::new();
    collect_toc_rows(entries, 0, &mut rows);
    if rows.iter().all(|row| row.spine_index.is_none()) {
        rows = titles
            .iter()
            .enumerate()
            .map(|(idx, title)| TocRow {
                label: title.clone(),
                spine_index: Some(idx),
                depth: 0,
            })
            .collect();
    }
    rows
}

fn collect_toc_rows(entries: &[kalam_reader::TocEntry], depth: usize, out: &mut Vec<TocRow>) {
    for entry in entries {
        out.push(TocRow {
            label: entry.label.clone(),
            spine_index: entry.spine_index,
            depth,
        });
        // Recursing unconditionally, not just for entries with a target:
        // a part heading usually has no spine index of its own, and its
        // chapters are exactly what used to go missing.
        collect_toc_rows(&entry.children, depth + 1, out);
    }
}

pub(crate) fn rebuild_toc(
    list: &gtk::Box,
    entries: &[kalam_reader::TocEntry],
    titles: &[String],
    current: usize,
    sender: &ComponentSender<ReaderModel>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    let rows = toc_rows(entries, titles);
    let active_spine = toc_active_spine_index(entries, titles.len(), current);
    for row in &rows {
        match row.spine_index {
            Some(idx) => append_toc_btn(list, row, idx, active_spine, sender),
            None => append_toc_heading(list, row),
        }
    }
}

/// A nesting heading: not a button, because there is nothing to jump to.
fn append_toc_heading(list: &gtk::Box, row: &TocRow) {
    let label = gtk::Label::new(Some(&row.label));
    label.add_css_class("kalam-reader-toc-part");
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    label.set_margin_start(16 + row.depth as i32 * 16);
    label.set_margin_end(16);
    list.append(&label);
}

/// Every spine index the table of contents points at, in display order
/// (nested entries included — they are listed in the same depth-first
/// order [`toc_rows`] draws, so the two cannot drift).
fn toc_spine_indices(entries: &[kalam_reader::TocEntry]) -> Vec<usize> {
    let mut out = Vec::new();
    for entry in entries {
        if let Some(idx) = entry.spine_index {
            out.push(idx);
        }
        out.extend(toc_spine_indices(&entry.children));
    }
    out
}

fn append_toc_btn(
    list: &gtk::Box,
    row: &TocRow,
    idx: usize,
    active_spine: Option<usize>,
    sender: &ComponentSender<ReaderModel>,
) {
    let btn = gtk::Button::new();
    btn.add_css_class("kalam-reader-toc-item");
    if Some(idx) == active_spine {
        btn.add_css_class("active");
    }
    // 16 px per nesting level, matching the sidebar's own 16 px padding so
    // a top-level row and a chapter of the same book do not look switched.
    btn.set_margin_start(row.depth as i32 * 16);
    let title = gtk::Label::new(Some(&row.label));
    title.set_halign(gtk::Align::Start);
    title.set_hexpand(true);
    title.set_wrap(true);
    title.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_lines(2);
    title.set_xalign(0.0);
    btn.set_child(Some(&title));
    let s = sender.clone();
    btn.connect_clicked(move |_| s.input(ReaderMsg::TocSelect(idx)));
    list.append(&btn);
}

fn toc_active_spine_index(
    entries: &[kalam_reader::TocEntry],
    chapter_count: usize,
    current: usize,
) -> Option<usize> {
    let visible = toc_spine_indices(entries);
    if visible.is_empty() {
        if chapter_count == 0 {
            None
        } else {
            Some(current.min(chapter_count.saturating_sub(1)))
        }
    } else {
        Some(
            visible
                .iter()
                .copied()
                .rfind(|idx| *idx <= current)
                .unwrap_or(visible[0]),
        )
    }
}

fn append_reader_empty(list: &gtk::Box, text: &str) {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);
    wrap.set_hexpand(true);
    wrap.set_vexpand(true);

    let top_spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    top_spacer.set_vexpand(true);
    wrap.append(&top_spacer);

    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-reader-empty");
    label.set_wrap(true);
    label.set_halign(gtk::Align::Center);
    label.set_justify(gtk::Justification::Center);
    label.set_xalign(0.5);
    wrap.append(&label);

    let bottom_spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bottom_spacer.set_vexpand(true);
    wrap.append(&bottom_spacer);

    list.append(&wrap);
}

fn annotation_note_text(buffer: &gtk::TextBuffer) -> String {
    let start = buffer.start_iter();
    let end = buffer.end_iter();
    buffer.text(&start, &end, false).to_string()
}

fn annotation_recolor_button(
    annotation_id: i64,
    current_color: HighlightColor,
    sender: &ComponentSender<ReaderModel>,
) -> gtk::MenuButton {
    let recolor = gtk::MenuButton::new();
    recolor.add_css_class("kalam-btn-icon");
    recolor.set_can_focus(false);
    recolor.set_always_show_arrow(false);
    recolor.set_tooltip_text(Some("Change highlight color"));
    recolor.set_child(Some(&crate::icons::symbolic_with_classes(
        "applications-graphics-symbolic",
        16,
        &["kalam-inline-icon"],
    )));

    let popover = gtk::Popover::new();
    popover.add_css_class("kalam-reader-color-popover");
    popover.set_has_arrow(false);
    popover.set_position(gtk::PositionType::Left);
    let palette = gtk::Box::new(gtk::Orientation::Vertical, 4);
    palette.add_css_class("kalam-reader-color-palette");

    for color in HighlightColor::ALL.iter().copied() {
        let choice = gtk::Button::with_label(highlight_color_label(color));
        choice.add_css_class("kalam-reader-color-choice");
        choice.add_css_class(highlight_color_choice_class(color));
        if color == current_color {
            choice.add_css_class("active");
        }
        choice.set_hexpand(true);
        choice.set_halign(gtk::Align::Fill);
        choice.set_tooltip_text(Some(&format!(
            "Use {} highlight",
            highlight_color_label(color)
        )));
        let s = sender.clone();
        let popover_to_close = popover.clone();
        choice.connect_clicked(move |_| {
            s.input(ReaderMsg::RecolorAnnotation(annotation_id, color));
            popover_to_close.popdown();
        });
        palette.append(&choice);
    }

    popover.set_child(Some(&palette));
    recolor.set_popover(Some(&popover));

    let tx = sender.input_sender().clone();
    recolor.connect_active_notify(move |button| {
        if button.is_active() {
            let _ = tx.send(ReaderMsg::OpenRightSidebar);
        }
    });
    connect_hover_zone(
        &popover,
        sender,
        ReaderMsg::OpenRightSidebar,
        ReaderMsg::ScheduleCloseRight,
    );
    recolor
}

fn highlight_color_label(color: HighlightColor) -> &'static str {
    match color {
        HighlightColor::Yellow => "Yellow",
        HighlightColor::Green => "Green",
        HighlightColor::Blue => "Blue",
        HighlightColor::Pink => "Pink",
        HighlightColor::Orange => "Orange",
    }
}

fn highlight_color_choice_class(color: HighlightColor) -> &'static str {
    match color {
        HighlightColor::Yellow => "kalam-reader-color-choice-yellow",
        HighlightColor::Green => "kalam-reader-color-choice-green",
        HighlightColor::Blue => "kalam-reader-color-choice-blue",
        HighlightColor::Pink => "kalam-reader-color-choice-pink",
        HighlightColor::Orange => "kalam-reader-color-choice-orange",
    }
}

pub(crate) fn rebuild_highlights_list(model: &ReaderModel, sender: &ComponentSender<ReaderModel>) {
    while let Some(child) = model.highlights_list.first_child() {
        model.highlights_list.remove(&child);
    }
    let annos = model.filtered_annotations();
    if annos.is_empty() {
        let empty_message = if model.annotation_search_query.trim().is_empty() {
            "No highlights yet in this view."
        } else {
            "No matching highlights or notes."
        };
        append_reader_empty(&model.highlights_list, empty_message);
        return;
    }

    for anno in annos.into_iter().take(150) {
        let annotation_id = anno.id;
        let selected = model.editing_annotation == Some(annotation_id);
        let outer = gtk::Box::new(gtk::Orientation::Vertical, 8);
        outer.add_css_class("kalam-reader-annotation-wrap");

        let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        card.add_css_class("kalam-reader-annotation-card");
        card.add_css_class(color_card_class(&anno.color));
        if selected {
            card.add_css_class("open");
        }
        card.set_hexpand(true);

        let body = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        body.add_css_class("kalam-reader-annotation-body");
        body.set_hexpand(true);

        let jump = gtk::Button::new();
        jump.add_css_class("kalam-reader-list-hit");
        jump.set_hexpand(true);
        jump.set_halign(gtk::Align::Fill);
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.set_hexpand(true);

        let text_col = gtk::Box::new(gtk::Orientation::Vertical, 4);
        text_col.set_hexpand(true);
        let text = gtk::Label::new(Some(&anno.text_excerpt));
        text.add_css_class("kalam-reader-annotation-text");
        text.set_wrap(true);
        text.set_xalign(0.0);
        text.set_halign(gtk::Align::Start);
        text_col.append(&text);
        let meta = gtk::Label::new(Some(&chapter_label(model, anno.chapter_index as usize)));
        meta.add_css_class("kalam-reader-annotation-meta");
        meta.set_halign(gtk::Align::Start);
        meta.set_xalign(0.0);
        text_col.append(&meta);
        row.append(&text_col);
        jump.set_child(Some(&row));
        let s = sender.clone();
        jump.connect_clicked(move |_| s.input(ReaderMsg::ToggleAnnotation(annotation_id)));
        body.append(&jump);

        if anno.kind == "highlight" {
            let recolor = annotation_recolor_button(
                annotation_id,
                HighlightColor::from_str_lossy(&anno.color),
                sender,
            );
            body.append(&recolor);
        }

        let delete = gtk::Button::new();
        delete.add_css_class("kalam-btn-icon");
        delete.add_css_class("danger");
        delete.set_focus_on_click(false);
        delete.set_tooltip_text(Some("Delete highlight"));
        delete.set_child(Some(&crate::icons::symbolic_with_classes(
            "user-trash-symbolic",
            16,
            &["kalam-inline-icon"],
        )));
        let s = sender.clone();
        delete.connect_clicked(move |_| s.input(ReaderMsg::DeleteAnnotation(annotation_id)));
        body.append(&delete);
        card.append(&body);
        let mut note_view_to_focus = None;

        if selected {
            let note_wrap = gtk::Box::new(gtk::Orientation::Vertical, 5);
            note_wrap.add_css_class("kalam-reader-annotation-note-wrap");
            note_wrap.set_hexpand(true);

            let note_label = gtk::Label::new(Some("Note"));
            note_label.add_css_class("kalam-reader-note-label");
            note_label.set_halign(gtk::Align::Start);
            note_label.set_xalign(0.0);
            note_wrap.append(&note_label);

            let note_view = gtk::TextView::new();
            note_view.add_css_class("kalam-reader-note-view");
            note_view.set_wrap_mode(gtk::WrapMode::WordChar);
            note_view.set_hexpand(true);
            note_view.set_vexpand(false);
            let note_buffer = note_view.buffer();
            note_buffer.set_text(&anno.note);

            let note_scroll = gtk::ScrolledWindow::builder()
                .min_content_height(52)
                .max_content_height(112)
                .hscrollbar_policy(gtk::PolicyType::Never)
                .vscrollbar_policy(gtk::PolicyType::Automatic)
                .hexpand(true)
                .child(&note_view)
                .build();
            note_scroll.add_css_class("kalam-reader-note-scroll");
            note_wrap.append(&note_scroll);

            let tx = sender.input_sender().clone();
            note_buffer.connect_changed(move |buffer| {
                let _ = tx.send(ReaderMsg::AnnotationNoteChanged(
                    annotation_id,
                    annotation_note_text(buffer),
                ));
            });

            let tx = sender.input_sender().clone();
            let buffer_for_focus = note_buffer.clone();
            note_view.connect_has_focus_notify(move |view| {
                if view.has_focus() {
                    return;
                }
                let _ = tx.send(ReaderMsg::SaveAnnotationNote(
                    annotation_id,
                    annotation_note_text(&buffer_for_focus),
                ));
            });
            card.append(&note_wrap);
            note_view_to_focus = Some(note_view);
        } else if !anno.note.trim().is_empty() {
            let preview = gtk::Button::new();
            preview.add_css_class("kalam-reader-annotation-note-preview");
            preview.set_hexpand(true);
            preview.set_halign(gtk::Align::Fill);
            let preview_text = gtk::Label::new(Some(&format!("Note: {}", anno.note.trim())));
            preview_text.add_css_class("kalam-reader-note-preview-text");
            preview_text.set_wrap(true);
            preview_text.set_halign(gtk::Align::Start);
            preview_text.set_xalign(0.0);
            preview.set_child(Some(&preview_text));
            let s = sender.clone();
            preview.connect_clicked(move |_| s.input(ReaderMsg::ToggleAnnotation(annotation_id)));
            card.append(&preview);
        }

        outer.append(&card);
        model.highlights_list.append(&outer);
        if let Some(note_view) = note_view_to_focus {
            note_view.grab_focus();
        }
    }
}

fn color_card_class(color: &str) -> &'static str {
    match HighlightColor::from_str_lossy(color) {
        HighlightColor::Yellow => "kalam-reader-annotation-card-yellow",
        HighlightColor::Green => "kalam-reader-annotation-card-green",
        HighlightColor::Blue => "kalam-reader-annotation-card-blue",
        HighlightColor::Pink => "kalam-reader-annotation-card-pink",
        HighlightColor::Orange => "kalam-reader-annotation-card-orange",
    }
}

pub(crate) fn rebuild_bookmarks_list(model: &ReaderModel, sender: &ComponentSender<ReaderModel>) {
    while let Some(child) = model.bookmarks_list.first_child() {
        model.bookmarks_list.remove(&child);
    }
    if model.bookmarks.is_empty() {
        append_reader_empty(
            &model.bookmarks_list,
            "No marks yet. Use Add current place or press M.",
        );
        return;
    }

    for mark in model.bookmarks.iter().take(150) {
        let outer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        outer.add_css_class("kalam-reader-bookmark-row");

        let jump = gtk::Button::new();
        jump.add_css_class("kalam-reader-list-hit");
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.append(&crate::icons::symbolic_with_classes(
            "bookmark-new-symbolic",
            16,
            &["kalam-reader-bookmark-icon"],
        ));
        let text_col = gtk::Box::new(gtk::Orientation::Vertical, 4);
        text_col.set_hexpand(true);
        let title = gtk::Label::new(Some(if mark.label.trim().is_empty() {
            "Reading mark"
        } else {
            &mark.label
        }));
        title.add_css_class("kalam-reader-bookmark-title");
        title.set_halign(gtk::Align::Start);
        title.set_xalign(0.0);
        text_col.append(&title);
        let meta = gtk::Label::new(Some(&format!(
            "{} · {}%",
            chapter_label(model, mark.chapter_index as usize),
            (mark.fraction * 100.0).round() as i64
        )));
        meta.add_css_class("kalam-reader-annotation-meta");
        meta.set_halign(gtk::Align::Start);
        meta.set_xalign(0.0);
        text_col.append(&meta);
        row.append(&text_col);
        jump.set_child(Some(&row));
        let ch = mark.chapter_index as usize;
        let frac = mark.fraction;
        let s = sender.clone();
        jump.connect_clicked(move |_| s.input(ReaderMsg::JumpToLocation(ch, frac)));
        outer.append(&jump);

        let delete = gtk::Button::new();
        delete.add_css_class("kalam-btn-icon");
        delete.add_css_class("danger");
        delete.set_child(Some(&crate::icons::symbolic_with_classes(
            "user-trash-symbolic",
            16,
            &["kalam-inline-icon"],
        )));
        let id = mark.id;
        let s = sender.clone();
        delete.connect_clicked(move |_| s.input(ReaderMsg::DeleteBookmark(id)));
        outer.append(&delete);

        model.bookmarks_list.append(&outer);
    }
}

pub(crate) fn rebuild_words_list(model: &ReaderModel, sender: &ComponentSender<ReaderModel>) {
    while let Some(child) = model.words_list.first_child() {
        model.words_list.remove(&child);
    }

    if !model.dict_query.trim().is_empty() {
        if model.dict_results.is_empty() {
            append_reader_empty(
                &model.words_list,
                "No matches. Import dictionaries in Settings if needed.",
            );
            return;
        }
        for entry in model.dict_results.iter().take(40) {
            let btn = gtk::Button::new();
            btn.add_css_class("kalam-reader-word-row");
            let body = gtk::Box::new(gtk::Orientation::Vertical, 4);
            let title = gtk::Label::new(Some(&entry.word));
            title.add_css_class("kalam-reader-word-name");
            title.set_halign(gtk::Align::Start);
            title.set_xalign(0.0);
            body.append(&title);
            let def = gtk::Label::new(Some(&truncate_def(&entry.definition, 180)));
            def.add_css_class("kalam-reader-word-def");
            def.set_wrap(true);
            def.set_xalign(0.0);
            def.set_halign(gtk::Align::Start);
            body.append(&def);
            let meta = gtk::Label::new(Some("Dictionary match"));
            meta.add_css_class("kalam-reader-word-meta");
            meta.set_halign(gtk::Align::Start);
            meta.set_xalign(0.0);
            body.append(&meta);
            btn.set_child(Some(&body));
            let word = entry.word.clone();
            let s = sender.clone();
            btn.connect_clicked(move |_| s.input(ReaderMsg::DictSearchSelect(word.clone())));
            model.words_list.append(&btn);
        }
        return;
    }

    let words = model.filtered_saved_words();
    if words.is_empty() {
        append_reader_empty(&model.words_list, "No saved words in this view yet.");
        return;
    }

    for word in words.into_iter().take(150) {
        let btn = gtk::Button::new();
        btn.add_css_class("kalam-reader-word-row");
        let body = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let title = gtk::Label::new(Some(&word.word));
        title.add_css_class("kalam-reader-word-name");
        title.set_halign(gtk::Align::Start);
        title.set_xalign(0.0);
        body.append(&title);
        let def = gtk::Label::new(Some(&truncate_def(&word.definition, 180)));
        def.add_css_class("kalam-reader-word-def");
        def.set_wrap(true);
        def.set_xalign(0.0);
        def.set_halign(gtk::Align::Start);
        body.append(&def);
        let meta = gtk::Label::new(Some(&saved_word_meta(model, word)));
        meta.add_css_class("kalam-reader-word-meta");
        meta.set_halign(gtk::Align::Start);
        meta.set_xalign(0.0);
        body.append(&meta);
        btn.set_child(Some(&body));
        let text = word.word.clone();
        let s = sender.clone();
        btn.connect_clicked(move |_| s.input(ReaderMsg::DictSearchSelect(text.clone())));
        model.words_list.append(&btn);
    }
}

fn saved_word_meta(model: &ReaderModel, word: &SavedWord) -> String {
    match word.chapter_index {
        Some(ch) => format!("{} · saved word", chapter_label(model, ch as usize)),
        None => "Saved word".into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(label: &str, spine: Option<usize>, children: Vec<kalam_reader::TocEntry>) -> kalam_reader::TocEntry {
        kalam_reader::TocEntry {
            label: label.to_string(),
            href: None,
            fragment: None,
            spine_index: spine,
            children,
        }
    }

    #[test]
    fn toc_rows_walk_parts_and_their_chapters_in_order() {
        // Part One -> ch0, ch1; Part Two -> ch2. The headings have no spine
        // index of their own, which is the shape the old code dropped: it
        // listed top-level entries only, so a Part -> Chapter book showed
        // two rows and both of them were the parts.
        let entries = vec![
            entry("Part One", None, vec![entry("One", Some(0), vec![]), entry("Two", Some(1), vec![])]),
            entry("Part Two", None, vec![entry("Three", Some(2), vec![])]),
        ];
        let rows = toc_rows(&entries, &["fallback 0".into(), "fallback 1".into(), "fallback 2".into()]);
        let seen: Vec<(&str, Option<usize>, usize)> = rows
            .iter()
            .map(|row| (row.label.as_str(), row.spine_index, row.depth))
            .collect();
        assert_eq!(
            seen,
            vec![
                ("Part One", None, 0),
                ("One", Some(0), 1),
                ("Two", Some(1), 1),
                ("Part Two", None, 0),
                ("Three", Some(2), 1),
            ]
        );
    }

    #[test]
    fn toc_rows_fall_back_to_chapter_titles_only_when_nothing_has_a_target() {
        let titles = vec!["Chapter 1".to_string(), "Chapter 2".to_string()];
        // No entries at all.
        let rows = toc_rows(&[], &titles);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1].label, "Chapter 2");
        assert_eq!(rows[1].spine_index, Some(1));
        assert_eq!(rows[1].depth, 0);
        // Entries that exist but point nowhere are still useless: the
        // fallback is what a reader can actually click.
        let entries = vec![entry("Contents", None, vec![])];
        let rows = toc_rows(&entries, &titles);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].label, "Chapter 1");
    }

    #[test]
    fn toc_rows_keep_a_nested_entrys_target_for_the_click() {
        // Three levels, because an EPUB can nest them and depth is what the
        // indent is drawn from.
        let entries = vec![entry(
            "One",
            Some(0),
            vec![entry("One A", Some(1), vec![entry("One A i", Some(2), vec![])])],
        )];
        let rows = toc_rows(&entries, &[]);
        let depths: Vec<usize> = rows.iter().map(|row| row.depth).collect();
        assert_eq!(depths, vec![0, 1, 2]);
        let spines: Vec<Option<usize>> = rows.iter().map(|row| row.spine_index).collect();
        assert_eq!(spines, vec![Some(0), Some(1), Some(2)]);
    }
}
