//! The left panel's Notes tab (decision 20): one list for the book's
//! highlights, bookmarks and saved words behind four filter chips,
//! with the standing filter's own controls underneath — the colour
//! dots and quote filter for Highlights, the place button for
//! Bookmarks, the lookup field and scope switch for Words
//! (decision 22).

use super::mod_model::ReaderModel;
use super::types::*;
use gtk::prelude::*;
use relm4::prelude::*;

pub(crate) struct NotesPanelControls {
    pub(crate) root: gtk::Box,
    pub(crate) filter_buttons: Vec<(NotesFilter, gtk::Button)>,
    pub(crate) highlight_filter_buttons: Vec<(HighlightFilter, gtk::Button)>,
    pub(crate) word_scope_buttons: Vec<(WordScope, gtk::Button)>,
    pub(crate) highlights_controls: gtk::Box,
    pub(crate) bookmarks_controls: gtk::Box,
    pub(crate) words_controls: gtk::Box,
}

pub(crate) fn build_notes_panel(
    sender: &ComponentSender<ReaderModel>,
    list: &gtk::Box,
) -> NotesPanelControls {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);

    // The four chips.
    let chips = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    chips.add_css_class("kalam-reader-filter-row");
    let mut filter_buttons = Vec::new();
    for (label, filter) in [
        ("All", NotesFilter::All),
        ("Highlights", NotesFilter::Highlights),
        ("Bookmarks", NotesFilter::Bookmarks),
        ("Words", NotesFilter::Words),
    ] {
        let btn = gtk::Button::with_label(label);
        btn.add_css_class("kalam-reader-filter-chip");
        let s = sender.clone();
        btn.connect_clicked(move |_| s.input(ReaderMsg::SetNotesFilter(filter)));
        chips.append(&btn);
        filter_buttons.push((filter, btn));
    }
    wrap.append(&chips);

    // Highlights' own row: search the text, filter by colour and kind.
    let highlights_controls = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let hl_search_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    hl_search_row.set_margin_start(12);
    hl_search_row.set_margin_end(12);
    hl_search_row.set_margin_bottom(10);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search highlights and notes…"));
    search.set_tooltip_text(Some("Search saved highlight text and notes"));
    search.set_hexpand(true);
    search.add_css_class("kalam-reader-search");
    let s = sender.clone();
    search.connect_search_changed(move |entry| {
        s.input(ReaderMsg::AnnotationSearchChanged(entry.text().to_string()));
    });
    hl_search_row.append(&search);
    let export = gtk::Button::new();
    export.set_child(Some(&crate::icons::labelled(
        "document-send-symbolic",
        16,
        "Export",
        6,
    )));
    export.add_css_class("kalam-btn-outlined");
    export.set_tooltip_text(Some("Export highlights & notes to ~/Highlights.md"));
    let s = sender.clone();
    export.connect_clicked(move |_| s.input(ReaderMsg::ExportAnnotations));
    hl_search_row.append(&export);
    highlights_controls.append(&hl_search_row);

    let kind_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    kind_row.set_margin_start(12);
    kind_row.set_margin_end(12);
    kind_row.set_margin_bottom(10);
    kind_row.set_valign(gtk::Align::Center);
    let mut highlight_filter_buttons = Vec::new();
    let all_btn = gtk::Button::with_label("All");
    all_btn.add_css_class("kalam-reader-filter-chip");
    let s = sender.clone();
    all_btn.connect_clicked(move |_| s.input(ReaderMsg::SetHighlightFilter(HighlightFilter::All)));
    kind_row.append(&all_btn);
    highlight_filter_buttons.push((HighlightFilter::All, all_btn));
    // Five colour dots; the ringed one is the standing filter.
    for (filter, class_name, name) in [
        (HighlightFilter::Yellow, "kalam-reader-dot-yellow", "Yellow"),
        (HighlightFilter::Green, "kalam-reader-dot-green", "Green"),
        (HighlightFilter::Blue, "kalam-reader-dot-blue", "Blue"),
        (HighlightFilter::Pink, "kalam-reader-dot-pink", "Pink"),
        (HighlightFilter::Orange, "kalam-reader-dot-orange", "Orange"),
    ] {
        let dot = gtk::Button::new();
        dot.add_css_class("kalam-reader-color-dot");
        dot.add_css_class(class_name);
        dot.set_tooltip_text(Some(name));
        dot.update_property(&[gtk::accessible::Property::Label(name)]);
        dot.set_valign(gtk::Align::Center);
        let s = sender.clone();
        dot.connect_clicked(move |_| s.input(ReaderMsg::SetHighlightFilter(filter)));
        kind_row.append(&dot);
        highlight_filter_buttons.push((filter, dot));
    }
    for (label, filter) in [
        ("Underline", HighlightFilter::Underline),
        ("Quotes", HighlightFilter::Quotes),
    ] {
        let btn = gtk::Button::with_label(label);
        btn.add_css_class("kalam-reader-filter-chip");
        let s = sender.clone();
        btn.connect_clicked(move |_| s.input(ReaderMsg::SetHighlightFilter(filter)));
        kind_row.append(&btn);
        highlight_filter_buttons.push((filter, btn));
    }
    highlights_controls.append(&kind_row);
    wrap.append(&highlights_controls);

    // Bookmarks' own row: mark the place you are standing on. The
    // ribbon is the primary gesture now; this is its labelled twin.
    let bookmarks_controls = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_margin_start(12);
    actions.set_margin_end(12);
    actions.set_margin_bottom(10);
    let add_btn = gtk::Button::new();
    add_btn.set_child(Some(&crate::icons::labelled(
        "bookmark-new-symbolic",
        16,
        "Add current place",
        6,
    )));
    add_btn.add_css_class("kalam-btn-tonal");
    let s = sender.clone();
    add_btn.connect_clicked(move |_| s.input(ReaderMsg::ToggleBookmark));
    actions.append(&add_btn);
    bookmarks_controls.append(&actions);
    wrap.append(&bookmarks_controls);

    // Words' own rows (decision 22): look a word up, choose whose
    // saved words are listed.
    let words_controls = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let word_search_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    word_search_row.set_margin_start(12);
    word_search_row.set_margin_end(12);
    word_search_row.set_margin_bottom(10);
    let word_search = gtk::SearchEntry::new();
    word_search.set_placeholder_text(Some("Look up a word…"));
    word_search.set_hexpand(true);
    word_search.add_css_class("kalam-reader-search");
    let s = sender.clone();
    word_search.connect_search_changed(move |entry| {
        s.input(ReaderMsg::DictSearch(entry.text().to_string()));
    });
    word_search_row.append(&word_search);
    words_controls.append(&word_search_row);

    let scope_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    scope_row.set_margin_start(12);
    scope_row.set_margin_end(12);
    scope_row.set_margin_bottom(10);
    let mut word_scope_buttons = Vec::new();
    for (label, scope) in [
        ("Chapter", WordScope::Chapter),
        ("This book", WordScope::Book),
        ("All books", WordScope::All),
    ] {
        let btn = gtk::Button::with_label(label);
        btn.add_css_class("kalam-reader-filter-chip");
        let s = sender.clone();
        btn.connect_clicked(move |_| s.input(ReaderMsg::SetWordScope(scope)));
        scope_row.append(&btn);
        word_scope_buttons.push((scope, btn));
    }
    words_controls.append(&scope_row);
    wrap.append(&words_controls);

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .child(list)
        .build();
    scroll.add_css_class("kalam-reader-panel-scroll");
    wrap.append(&scroll);

    NotesPanelControls {
        root: wrap,
        filter_buttons,
        highlight_filter_buttons,
        word_scope_buttons,
        highlights_controls,
        bookmarks_controls,
        words_controls,
    }
}
