//! The PDF reader's Notes tab (decision 60): one list for the
//! book's highlights, bookmarks and saved words behind the four
//! filter chips. The Highlights filter carries its colour dots and
//! search button, Words its saved-word search and scope switch, and
//! Bookmarks keep the page marks this tab used to hold alone.

use super::{
    current_toc_index, pages_heading_for_page, pdf_jump_chapter_pages, toggle_active,
    PdfNotesFilter, PdfReaderModel, PdfReaderMsg, PdfWordScope,
};
use crate::db::{Annotation, HighlightColor, ReadingBookmark, SavedWord};
use gtk::prelude::*;
use relm4::prelude::*;

const NOTES_ROW_CAP: usize = 250;

pub struct PdfNotesWidgets {
    list: gtk::Box,
    filter_buttons: Vec<(PdfNotesFilter, gtk::Button)>,
    highlights_controls: gtk::Box,
    highlight_colour_buttons: Vec<(HighlightColor, gtk::Button)>,
    highlight_search_button: gtk::Button,
    highlight_search_entry: gtk::SearchEntry,
    words_controls: gtk::Box,
    word_search_entry: gtk::SearchEntry,
    word_scope_control: crate::widgets::segmented::SegmentedControl,
}

impl PdfNotesWidgets {
    pub fn focus_highlight_search(&self) {
        self.highlight_search_entry.grab_focus();
    }
}

pub(super) fn build_pdf_notes_pane(
    sender: &ComponentSender<PdfReaderModel>,
    word_scope: PdfWordScope,
) -> (gtk::Box, PdfNotesWidgets) {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("kalam-pdf-notes-pane");
    root.set_hexpand(true);
    root.set_vexpand(true);

    let chips = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    chips.set_margin_start(20);
    chips.set_margin_end(20);
    chips.set_margin_top(14);
    let mut filter_buttons = Vec::new();
    for (label, filter) in [
        ("All", PdfNotesFilter::All),
        ("Highlights", PdfNotesFilter::Highlights),
        ("Bookmarks", PdfNotesFilter::Bookmarks),
        ("Words", PdfNotesFilter::Words),
    ] {
        let button = gtk::Button::with_label(label);
        button.add_css_class("kalam-reader-filter-chip");
        let tx = sender.input_sender().clone();
        button.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::SetNotesFilter(filter));
        });
        chips.append(&button);
        filter_buttons.push((filter, button));
    }
    root.append(&chips);

    // Highlights: five colour dots and the round search button. A
    // standing colour is a filter; pressing it again lets every
    // colour back in.
    let highlights_controls = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let colour_row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    colour_row.set_margin_start(20);
    colour_row.set_margin_end(20);
    colour_row.set_margin_top(14);
    let mut highlight_colour_buttons = Vec::new();
    for (colour, class_name, name) in [
        (HighlightColor::Yellow, "kalam-reader-dot-yellow", "Yellow"),
        (HighlightColor::Green, "kalam-reader-dot-green", "Green"),
        (HighlightColor::Blue, "kalam-reader-dot-blue", "Blue"),
        (HighlightColor::Pink, "kalam-reader-dot-pink", "Pink"),
        (HighlightColor::Orange, "kalam-reader-dot-orange", "Orange"),
    ] {
        let dot = gtk::Button::new();
        dot.add_css_class("kalam-reader-color-dot");
        dot.add_css_class(class_name);
        dot.set_tooltip_text(Some(name));
        dot.update_property(&[gtk::accessible::Property::Label(name)]);
        dot.set_valign(gtk::Align::Center);
        let tx = sender.input_sender().clone();
        dot.connect_clicked(move |_| {
            let _ = tx.send(PdfReaderMsg::ToggleHighlightColourFilter(colour));
        });
        colour_row.append(&dot);
        highlight_colour_buttons.push((colour, dot));
    }
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    colour_row.append(&spacer);

    let highlight_search_button = gtk::Button::new();
    highlight_search_button.add_css_class("kalam-pdf-notes-search-toggle");
    highlight_search_button.set_child(Some(&crate::icons::symbolic_with_classes(
        "system-search-symbolic",
        16,
        &["kalam-inline-icon"],
    )));
    highlight_search_button.set_tooltip_text(Some("Search highlights"));
    highlight_search_button
        .update_property(&[gtk::accessible::Property::Label("Search highlights")]);
    let tx = sender.input_sender().clone();
    highlight_search_button.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::ToggleHighlightSearch);
    });
    colour_row.append(&highlight_search_button);
    highlights_controls.append(&colour_row);

    let highlight_search_entry = gtk::SearchEntry::new();
    highlight_search_entry.set_placeholder_text(Some("Search highlights and notes…"));
    highlight_search_entry.add_css_class("kalam-reader-search");
    highlight_search_entry.set_margin_start(20);
    highlight_search_entry.set_margin_end(20);
    highlight_search_entry.set_margin_top(10);
    let tx = sender.input_sender().clone();
    highlight_search_entry.connect_search_changed(move |entry| {
        let _ = tx.send(PdfReaderMsg::HighlightSearchChanged(
            entry.text().to_string(),
        ));
    });
    highlights_controls.append(&highlight_search_entry);
    root.append(&highlights_controls);

    // Words: search the saved words, then choose whose they are.
    let words_controls = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let word_search_entry = gtk::SearchEntry::new();
    word_search_entry.set_placeholder_text(Some("Search saved words"));
    word_search_entry.add_css_class("kalam-reader-search");
    word_search_entry.set_margin_start(20);
    word_search_entry.set_margin_end(20);
    word_search_entry.set_margin_top(14);
    let tx = sender.input_sender().clone();
    word_search_entry.connect_search_changed(move |entry| {
        let _ = tx.send(PdfReaderMsg::WordSearchChanged(entry.text().to_string()));
    });
    words_controls.append(&word_search_entry);

    let tx = sender.input_sender().clone();
    let word_scope_control = crate::widgets::segmented::segmented_control(
        &["Chapter", "Book", "All books"],
        word_scope.toggle_index(),
        crate::widgets::segmented::SegmentedSize::Panel,
        move |index| {
            let _ = tx.send(PdfReaderMsg::SetWordScope(PdfWordScope::from_toggle_index(
                index,
            )));
        },
    );
    let scope_host = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    scope_host.set_margin_start(20);
    scope_host.set_margin_end(20);
    scope_host.set_margin_top(10);
    let scope_root = word_scope_control.widget();
    scope_root.set_hexpand(true);
    scope_root.set_halign(gtk::Align::Fill);
    for button in word_scope_control.buttons() {
        button.set_hexpand(true);
    }
    scope_host.append(&scope_root);
    words_controls.append(&scope_host);
    root.append(&words_controls);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.add_css_class("kalam-pdf-notes-list");
    list.set_hexpand(true);
    list.set_vexpand(true);
    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .child(&list)
        .build();
    scroll.add_css_class("kalam-reader-panel-scroll");
    root.append(&scroll);

    (
        root,
        PdfNotesWidgets {
            list,
            filter_buttons,
            highlights_controls,
            highlight_colour_buttons,
            highlight_search_button,
            highlight_search_entry,
            words_controls,
            word_search_entry,
            word_scope_control,
        },
    )
}

pub(super) fn populate_notes_panel(
    model: &PdfReaderModel,
    sender: &ComponentSender<PdfReaderModel>,
) {
    let Some(widgets) = &model.notes_widgets else {
        return;
    };
    sync_notes_controls(model, widgets);
    while let Some(child) = widgets.list.first_child() {
        widgets.list.remove(&child);
    }

    match model.notes_filter {
        PdfNotesFilter::Highlights => populate_highlights(model, widgets, sender),
        PdfNotesFilter::Bookmarks => populate_bookmarks(model, widgets, sender),
        PdfNotesFilter::Words => populate_words(model, widgets, sender),
        PdfNotesFilter::All => populate_all(model, widgets, sender),
    }
}

fn sync_notes_controls(model: &PdfReaderModel, widgets: &PdfNotesWidgets) {
    for (filter, button) in &widgets.filter_buttons {
        toggle_active(button, *filter == model.notes_filter);
    }
    let has_highlights = model
        .annotations
        .iter()
        .any(|anno| anno.kind == "highlight" || anno.kind == "quote");
    widgets
        .highlights_controls
        .set_visible(model.notes_filter == PdfNotesFilter::Highlights && has_highlights);
    for (colour, button) in &widgets.highlight_colour_buttons {
        toggle_active(button, model.highlight_colour_filter == Some(*colour));
    }
    toggle_active(
        &widgets.highlight_search_button,
        model.highlight_search_open || !model.highlight_search_query.trim().is_empty(),
    );
    widgets
        .highlight_search_entry
        .set_visible(model.highlight_search_open);
    if widgets.highlight_search_entry.text().as_str() != model.highlight_search_query.as_str() {
        widgets
            .highlight_search_entry
            .set_text(&model.highlight_search_query);
    }

    widgets
        .words_controls
        .set_visible(model.notes_filter == PdfNotesFilter::Words && !model.saved_words.is_empty());
    if widgets.word_search_entry.text().as_str() != model.word_search_query.as_str() {
        widgets.word_search_entry.set_text(&model.word_search_query);
    }
    widgets
        .word_scope_control
        .set_selected(model.word_scope.toggle_index());
}

enum NotesItem<'a> {
    Annotation(&'a Annotation),
    Bookmark(&'a ReadingBookmark),
    Word(&'a SavedWord),
}

fn populate_highlights(
    model: &PdfReaderModel,
    widgets: &PdfNotesWidgets,
    sender: &ComponentSender<PdfReaderModel>,
) {
    let has_highlights = model
        .annotations
        .iter()
        .any(|anno| anno.kind == "highlight" || anno.kind == "quote");
    if !has_highlights {
        widgets.list.append(&notes_empty(
            "kalam-highlight-symbolic",
            "No highlights yet",
            "Select some text and pick a colour to highlight it.",
        ));
        return;
    }

    let mut annos: Vec<&Annotation> = model
        .annotations
        .iter()
        .filter(|anno| {
            annotation_matches(
                anno,
                model.highlight_colour_filter,
                &model.highlight_search_query,
            )
        })
        .collect();
    annos.sort_by(|a, b| {
        a.chapter_index
            .cmp(&b.chapter_index)
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });
    if annos.is_empty() {
        widgets.list.append(&notes_empty(
            "system-search-symbolic",
            "No matching highlights",
            "Try another colour or search.",
        ));
        return;
    }

    let mut standing_heading: Option<String> = None;
    for anno in annos.into_iter().take(NOTES_ROW_CAP) {
        let heading = section_heading(model, annotation_page(anno));
        if standing_heading.as_deref() != Some(heading.as_str()) {
            widgets.list.append(&notes_heading(&heading));
            standing_heading = Some(heading);
        }
        widgets.list.append(&annotation_row(anno, sender));
    }
}

fn populate_bookmarks(
    model: &PdfReaderModel,
    widgets: &PdfNotesWidgets,
    sender: &ComponentSender<PdfReaderModel>,
) {
    if model.bookmarks.is_empty() {
        widgets.list.append(&notes_empty(
            "kalam-ribbon-outline-symbolic",
            "No bookmarks yet",
            "Press B to bookmark the page you’re on.",
        ));
        return;
    }

    let mut marks: Vec<&ReadingBookmark> = model.bookmarks.iter().collect();
    marks.sort_by(|a, b| {
        a.chapter_index
            .cmp(&b.chapter_index)
            .then_with(|| a.fraction.total_cmp(&b.fraction))
            .then_with(|| a.created_at.cmp(&b.created_at))
            .then_with(|| a.id.cmp(&b.id))
    });

    let mut standing_heading: Option<String> = None;
    let mut group_has_row = false;
    for mark in marks.into_iter().take(NOTES_ROW_CAP) {
        let page = mark.chapter_index.max(1) as usize;
        let heading =
            pages_heading_for_page(&model.toc_entries, model.total_pages, page, &model.title).title;
        if standing_heading.as_deref() != Some(heading.as_str()) {
            widgets.list.append(&notes_heading(&heading));
            standing_heading = Some(heading);
            group_has_row = false;
        }
        if group_has_row {
            widgets.list.append(&notes_separator());
        }
        widgets.list.append(&bookmark_row(model, mark, sender));
        group_has_row = true;
    }
}

fn populate_words(
    model: &PdfReaderModel,
    widgets: &PdfNotesWidgets,
    sender: &ComponentSender<PdfReaderModel>,
) {
    if model.saved_words.is_empty() {
        widgets.list.append(&notes_empty(
            "accessories-dictionary-symbolic",
            "No saved words yet",
            "Select a word, press Define, then save it to keep it here.",
        ));
        return;
    }

    let chapter = pages_heading_for_page(
        &model.toc_entries,
        model.total_pages,
        model.current_page,
        &model.title,
    );
    let mut words: Vec<&SavedWord> = model
        .saved_words
        .iter()
        .filter(|word| {
            word_in_scope(
                word,
                model.book_id,
                model.word_scope,
                chapter.start_page,
                chapter.end_page,
            ) && word_matches_query(word, &model.word_search_query)
        })
        .collect();
    if model.word_scope != PdfWordScope::All {
        // This book's words read in the order the book meets them;
        // All books keeps the saved list's newest-first order,
        // there being no one book to order by.
        words.sort_by(|a, b| {
            a.chapter_index
                .unwrap_or(i64::MAX)
                .cmp(&b.chapter_index.unwrap_or(i64::MAX))
                .then_with(|| a.created_at.cmp(&b.created_at))
                .then_with(|| a.word.to_lowercase().cmp(&b.word.to_lowercase()))
        });
    }
    if words.is_empty() {
        widgets.list.append(&notes_empty(
            "accessories-dictionary-symbolic",
            "No saved words here",
            "Try another scope or search.",
        ));
        return;
    }
    for (index, word) in words.into_iter().take(NOTES_ROW_CAP).enumerate() {
        if index > 0 {
            widgets.list.append(&notes_word_separator());
        }
        widgets.list.append(&saved_word_row(word, sender));
    }
}

fn populate_all(
    model: &PdfReaderModel,
    widgets: &PdfNotesWidgets,
    sender: &ComponentSender<PdfReaderModel>,
) {
    // Book order across all three kinds: page first, then the
    // creation stamp all three share.
    let mut items: Vec<(i64, &str, u8, NotesItem<'_>)> = Vec::new();
    for anno in model
        .annotations
        .iter()
        .filter(|anno| anno.kind == "highlight" || anno.kind == "quote")
    {
        items.push((
            anno.chapter_index,
            anno.created_at.as_str(),
            0,
            NotesItem::Annotation(anno),
        ));
    }
    for mark in &model.bookmarks {
        items.push((
            mark.chapter_index,
            mark.created_at.as_str(),
            1,
            NotesItem::Bookmark(mark),
        ));
    }
    for word in model
        .saved_words
        .iter()
        .filter(|word| word.book_id == Some(model.book_id))
    {
        items.push((
            word.chapter_index.unwrap_or(i64::MAX),
            word.created_at.as_str(),
            2,
            NotesItem::Word(word),
        ));
    }
    if items.is_empty() {
        widgets.list.append(&notes_empty(
            "kalam-highlight-symbolic",
            "Nothing kept yet",
            "Highlights, bookmarks and saved words from this book gather here.",
        ));
        return;
    }
    items.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then_with(|| a.1.cmp(b.1))
            .then_with(|| a.2.cmp(&b.2))
    });

    let mut standing_heading: Option<String> = None;
    for (page, _, _, item) in items.into_iter().take(NOTES_ROW_CAP) {
        let heading = if page == i64::MAX {
            "Saved words".to_string()
        } else {
            section_heading(model, page.max(1) as usize)
        };
        if standing_heading.as_deref() != Some(heading.as_str()) {
            widgets.list.append(&notes_heading(&heading));
            standing_heading = Some(heading);
        }
        match item {
            NotesItem::Annotation(anno) => widgets.list.append(&annotation_row(anno, sender)),
            NotesItem::Bookmark(mark) => widgets.list.append(&bookmark_row(model, mark, sender)),
            NotesItem::Word(word) => widgets.list.append(&saved_word_row(word, sender)),
        }
    }
}

fn annotation_page(anno: &Annotation) -> usize {
    anno.chapter_index.max(1) as usize
}

fn annotation_row(anno: &Annotation, sender: &ComponentSender<PdfReaderModel>) -> gtk::Box {
    let outer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    outer.add_css_class("kalam-pdf-notes-highlight-row");

    let jump = gtk::Button::new();
    jump.add_css_class("kalam-pdf-notes-row-hit");
    jump.set_hexpand(true);
    jump.set_halign(gtk::Align::Fill);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.set_hexpand(true);

    let bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bar.add_css_class("kalam-reader-annotation-bar");
    bar.add_css_class(colour_bar_class(&anno.color));
    bar.set_size_request(3, -1);
    bar.set_valign(gtk::Align::Fill);
    row.append(&bar);

    let text_col = gtk::Box::new(gtk::Orientation::Vertical, 4);
    text_col.set_hexpand(true);
    let excerpt = if anno.text_excerpt.trim().is_empty() {
        "Highlight"
    } else {
        anno.text_excerpt.trim()
    };
    let text = gtk::Label::new(Some(excerpt));
    text.add_css_class("kalam-pdf-notes-excerpt");
    text.set_wrap(true);
    text.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    text.set_lines(-1);
    text.set_halign(gtk::Align::Fill);
    text.set_xalign(0.0);
    text_col.append(&text);

    let meta = gtk::Label::new(Some(&annotation_meta(anno)));
    meta.add_css_class("kalam-pdf-notes-meta");
    meta.set_halign(gtk::Align::Fill);
    meta.set_xalign(0.0);
    text_col.append(&meta);
    row.append(&text_col);
    jump.set_child(Some(&row));
    let page = annotation_page(anno);
    let tx = sender.input_sender().clone();
    jump.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::JumpToPage(page));
    });
    outer.append(&jump);
    outer.append(&delete_button(
        "Delete highlight",
        sender,
        PdfReaderMsg::DeleteAnnotation(anno.id),
    ));
    outer
}

fn bookmark_row(
    model: &PdfReaderModel,
    mark: &ReadingBookmark,
    sender: &ComponentSender<PdfReaderModel>,
) -> gtk::Box {
    let outer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    outer.add_css_class("kalam-pdf-notes-bookmark-row");

    let jump = gtk::Button::new();
    jump.add_css_class("kalam-pdf-notes-row-hit");
    jump.set_hexpand(true);
    jump.set_halign(gtk::Align::Fill);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.set_hexpand(true);
    let icon = crate::icons::symbolic_with_classes(
        "kalam-ribbon-filled-symbolic",
        16,
        &["kalam-pdf-notes-bookmark-icon"],
    );
    icon.set_valign(gtk::Align::Start);
    row.append(&icon);

    let page = mark.chapter_index.max(1) as usize;
    let text_col = gtk::Box::new(gtk::Orientation::Vertical, 3);
    text_col.set_hexpand(true);
    let title = gtk::Label::new(Some(&bookmark_title(
        page,
        section_title(model, page).as_deref(),
        &mark.label,
    )));
    title.add_css_class("kalam-reader-bookmark-title");
    title.set_wrap(true);
    title.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    title.set_lines(-1);
    title.set_halign(gtk::Align::Fill);
    title.set_xalign(0.0);
    text_col.append(&title);

    let meta_text = match chapter_number(model, page) {
        Some(number) => format!("Chapter {number}"),
        None => format!("{:.0}% through book", mark.fraction * 100.0),
    };
    let meta = gtk::Label::new(Some(&meta_text));
    meta.add_css_class("kalam-pdf-notes-meta");
    meta.set_halign(gtk::Align::Fill);
    meta.set_xalign(0.0);
    text_col.append(&meta);
    row.append(&text_col);
    jump.set_child(Some(&row));
    let tx = sender.input_sender().clone();
    jump.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::JumpToPage(page));
    });
    outer.append(&jump);
    outer.append(&delete_button(
        "Delete bookmark",
        sender,
        PdfReaderMsg::DeleteBookmark(mark.id),
    ));
    outer
}

fn saved_word_row(word: &SavedWord, sender: &ComponentSender<PdfReaderModel>) -> gtk::Box {
    let outer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    outer.add_css_class("kalam-pdf-notes-word-row-wrap");

    let lookup = gtk::Button::new();
    lookup.add_css_class("kalam-pdf-notes-row-hit");
    lookup.set_hexpand(true);
    lookup.set_halign(gtk::Align::Fill);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 3);
    body.set_hexpand(true);
    let title = gtk::Label::new(Some(&word.word));
    title.add_css_class("kalam-reader-word-name");
    title.add_css_class("kalam-pdf-notes-word-name");
    title.set_halign(gtk::Align::Fill);
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    body.append(&title);

    let meaning = gtk::Label::new(Some(&one_line_meaning(&word.definition)));
    meaning.add_css_class("kalam-pdf-notes-word-meaning");
    meaning.set_halign(gtk::Align::Fill);
    meaning.set_xalign(0.0);
    meaning.set_ellipsize(gtk::pango::EllipsizeMode::End);
    body.append(&meaning);
    lookup.set_child(Some(&body));
    let text = word.word.clone();
    let tx = sender.input_sender().clone();
    lookup.connect_clicked(move |_| {
        let _ = tx.send(PdfReaderMsg::LookUpWord(text.clone()));
    });
    outer.append(&lookup);
    outer.append(&delete_button(
        "Forget this word",
        sender,
        PdfReaderMsg::DeleteSavedWord(word.id),
    ));
    outer
}

fn delete_button(
    label: &str,
    sender: &ComponentSender<PdfReaderModel>,
    message: PdfReaderMsg,
) -> gtk::Button {
    let delete = gtk::Button::new();
    delete.add_css_class("kalam-btn-icon");
    delete.add_css_class("danger");
    delete.set_valign(gtk::Align::Center);
    delete.set_tooltip_text(Some(label));
    delete.update_property(&[gtk::accessible::Property::Label(label)]);
    delete.set_child(Some(&crate::icons::symbolic_with_classes(
        "user-trash-symbolic",
        16,
        &["kalam-inline-icon"],
    )));
    let (kind, id) = match message {
        PdfReaderMsg::DeleteAnnotation(id) => (0, id),
        PdfReaderMsg::DeleteBookmark(id) => (1, id),
        PdfReaderMsg::DeleteSavedWord(id) => (2, id),
        _ => (u8::MAX, 0),
    };
    let tx = sender.input_sender().clone();
    delete.connect_clicked(move |_| {
        let message = match kind {
            0 => PdfReaderMsg::DeleteAnnotation(id),
            1 => PdfReaderMsg::DeleteBookmark(id),
            2 => PdfReaderMsg::DeleteSavedWord(id),
            _ => return,
        };
        let _ = tx.send(message);
    });
    delete
}

fn notes_heading(title: &str) -> gtk::Label {
    let heading = gtk::Label::new(Some(title));
    heading.add_css_class("kalam-reader-notes-heading");
    heading.add_css_class("kalam-pdf-notes-heading");
    heading.set_halign(gtk::Align::Fill);
    heading.set_xalign(0.0);
    heading.set_ellipsize(gtk::pango::EllipsizeMode::End);
    heading
}

fn notes_separator() -> gtk::Separator {
    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    separator.add_css_class("kalam-pdf-notes-separator");
    separator.set_margin_start(20);
    separator.set_margin_end(20);
    separator.set_margin_top(8);
    separator
}

fn notes_word_separator() -> gtk::Separator {
    let separator = gtk::Separator::new(gtk::Orientation::Horizontal);
    separator.add_css_class("kalam-pdf-notes-separator");
    separator.set_margin_start(20);
    separator.set_margin_end(20);
    separator
}

fn notes_empty(icon_name: &str, title: &str, body: &str) -> gtk::Box {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 8);
    wrap.add_css_class("kalam-pdf-notes-empty");
    wrap.set_hexpand(true);
    wrap.set_vexpand(true);
    wrap.set_margin_start(56);
    wrap.set_margin_end(56);

    let top_spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    top_spacer.set_vexpand(true);
    wrap.append(&top_spacer);

    let icon = crate::icons::symbolic_with_classes(icon_name, 32, &["kalam-pdf-notes-empty-icon"]);
    icon.set_halign(gtk::Align::Center);
    wrap.append(&icon);

    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("kalam-pdf-notes-empty-title");
    title_label.set_halign(gtk::Align::Center);
    wrap.append(&title_label);

    let body_label = gtk::Label::new(Some(body));
    body_label.add_css_class("kalam-pdf-notes-empty-body");
    body_label.set_wrap(true);
    body_label.set_justify(gtk::Justification::Center);
    body_label.set_halign(gtk::Align::Fill);
    body_label.set_xalign(0.5);
    wrap.append(&body_label);

    let bottom_spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    bottom_spacer.set_vexpand(true);
    wrap.append(&bottom_spacer);
    wrap
}

/// The deepest outline title a page sits in, when the outline
/// names one. Headings fall back to the top-level chapter (or the
/// book); a bookmark's title keeps its stored label instead.
fn section_title(model: &PdfReaderModel, page: usize) -> Option<String> {
    current_toc_index(&model.toc_entries, page, model.total_pages)
        .map(|index| model.toc_entries[index].title.trim().to_string())
        .filter(|title| !title.is_empty())
}

fn section_heading(model: &PdfReaderModel, page: usize) -> String {
    section_title(model, page).unwrap_or_else(|| {
        pages_heading_for_page(&model.toc_entries, model.total_pages, page, &model.title).title
    })
}

fn chapter_number(model: &PdfReaderModel, page: usize) -> Option<usize> {
    let chapters = pdf_jump_chapter_pages(&model.toc_entries, model.total_pages);
    chapters
        .iter()
        .rposition(|start| *start <= page)
        .map(|index| index + 1)
}

fn colour_bar_class(colour: &str) -> &'static str {
    match HighlightColor::from_str_lossy(colour) {
        HighlightColor::Yellow => "kalam-reader-bar-yellow",
        HighlightColor::Green => "kalam-reader-bar-green",
        HighlightColor::Blue => "kalam-reader-bar-blue",
        HighlightColor::Pink => "kalam-reader-bar-pink",
        HighlightColor::Orange => "kalam-reader-bar-orange",
        HighlightColor::Underline => "kalam-reader-bar-underline",
    }
}

pub(super) fn annotation_matches(
    anno: &Annotation,
    colour: Option<HighlightColor>,
    query: &str,
) -> bool {
    let matches_kind = match colour {
        None => anno.kind == "highlight" || anno.kind == "quote",
        Some(colour) => {
            anno.kind == "highlight" && HighlightColor::from_str_lossy(&anno.color) == colour
        }
    };
    if !matches_kind {
        return false;
    }
    let query = query.trim().to_lowercase();
    query.is_empty()
        || anno.text_excerpt.to_lowercase().contains(&query)
        || anno.note.to_lowercase().contains(&query)
}

pub(super) fn word_in_scope(
    word: &SavedWord,
    book_id: i64,
    scope: PdfWordScope,
    chapter_start: usize,
    chapter_end: usize,
) -> bool {
    match scope {
        PdfWordScope::Chapter => {
            word.book_id == Some(book_id)
                && word
                    .chapter_index
                    .is_some_and(|page| page >= chapter_start as i64 && page <= chapter_end as i64)
        }
        PdfWordScope::Book => word.book_id == Some(book_id),
        PdfWordScope::All => true,
    }
}

pub(super) fn word_matches_query(word: &SavedWord, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || word.word.to_lowercase().contains(&query)
        || word.definition.to_lowercase().contains(&query)
        || word
            .context_text
            .as_deref()
            .is_some_and(|context| context.to_lowercase().contains(&query))
}

pub(super) fn annotation_meta(anno: &Annotation) -> String {
    let mut meta = format!("p. {}", annotation_page(anno));
    if anno.kind == "quote" {
        meta.push_str(" · quote");
    }
    if !anno.note.trim().is_empty() {
        meta.push_str(" · note");
    }
    meta
}

pub(super) fn bookmark_title(page: usize, section: Option<&str>, stored_label: &str) -> String {
    if let Some(section) = section.filter(|section| !section.trim().is_empty()) {
        return format!("Page {page} · {}", section.trim());
    }
    let stored = stored_label.trim();
    if stored.is_empty() || stored == format!("Page {page}") {
        format!("Page {page}")
    } else {
        stored.to_string()
    }
}

pub(super) fn one_line_meaning(definition: &str) -> String {
    let meaning = definition.split_whitespace().collect::<Vec<_>>().join(" ");
    if meaning.is_empty() {
        "Saved word".to_string()
    } else {
        meaning
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn annotation(kind: &str, colour: &str, excerpt: &str, note: &str) -> Annotation {
        Annotation {
            id: 7,
            book_id: 21,
            kind: kind.to_string(),
            chapter_index: 4,
            start_path: String::new(),
            start_offset: 0,
            end_path: String::new(),
            end_offset: 0,
            color: colour.to_string(),
            style: "solid".to_string(),
            text_excerpt: excerpt.to_string(),
            note: note.to_string(),
            cfi: None,
            created_at: "2026-10-26T00:00:00Z".to_string(),
            updated_at: "2026-10-26T00:00:00Z".to_string(),
        }
    }

    fn saved_word(book_id: Option<i64>, page: Option<i64>) -> SavedWord {
        SavedWord {
            id: 3,
            word: "tectonic".to_string(),
            definition: "relating to the Earth’s crust".to_string(),
            dict_name: None,
            book_id,
            chapter_index: page,
            context_text: Some("tectonic plates".to_string()),
            created_at: "2026-10-26T00:00:00Z".to_string(),
            known: false,
        }
    }

    #[test]
    fn highlights_filter_by_colour_kind_and_text() {
        let yellow = annotation("highlight", "yellow", "Plates meet", "check this");
        assert!(annotation_matches(&yellow, None, ""));
        assert!(annotation_matches(
            &yellow,
            Some(HighlightColor::Yellow),
            "CHECK"
        ));
        assert!(!annotation_matches(&yellow, Some(HighlightColor::Blue), ""));
        assert!(!annotation_matches(&yellow, None, "ocean"));

        let quote = annotation("quote", "yellow", "Plates meet", "");
        assert!(annotation_matches(&quote, None, ""));
        assert!(!annotation_matches(
            &quote,
            Some(HighlightColor::Yellow),
            ""
        ));
    }

    #[test]
    fn words_follow_chapter_book_and_all_scopes() {
        let word = saved_word(Some(21), Some(4));
        assert!(word_in_scope(&word, 21, PdfWordScope::Chapter, 2, 7));
        assert!(!word_in_scope(&word, 21, PdfWordScope::Chapter, 8, 9));
        assert!(word_in_scope(&word, 21, PdfWordScope::Book, 8, 9));
        assert!(word_in_scope(&word, 99, PdfWordScope::All, 8, 9));
        assert!(word_matches_query(&word, "CRUST"));
        assert!(word_matches_query(&word, "plates"));
        assert!(!word_matches_query(&word, "magma"));
    }

    #[test]
    fn row_text_matches_the_notes_board() {
        let anno = annotation("highlight", "green", "Heat drives motion", "remember");
        assert_eq!(annotation_meta(&anno), "p. 4 · note");
        let quote = annotation("quote", "yellow", "Heat drives motion", "");
        assert_eq!(annotation_meta(&quote), "p. 4 · quote");
        assert_eq!(
            bookmark_title(4, Some("Plate tectonics"), "Where the plates meet"),
            "Page 4 · Plate tectonics"
        );
        assert_eq!(bookmark_title(4, None, "Page 4"), "Page 4");
        assert_eq!(
            one_line_meaning("molten rock\n beneath   the surface"),
            "molten rock beneath the surface"
        );
    }
}
