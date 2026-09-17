//! Right sidebar panel builders for Highlights, Bookmarks, and Words.

use super::mod_model::ReaderModel;
use super::types::*;
use gtk::prelude::*;
use relm4::prelude::*;
use relm4::RelmWidgetExt;

pub(crate) fn build_highlights_panel(
    sender: &ComponentSender<ReaderModel>,
    _active: HighlightFilter,
    list: &gtk::Box,
) -> (gtk::Box, Vec<(HighlightFilter, gtk::Button)>) {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    search_row.add_css_class("kalam-reader-search-row");
    search_row.set_margin_all(12);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search highlights and notes…"));
    search.set_tooltip_text(Some("Search saved highlight text and notes"));
    search.set_hexpand(true);
    search.add_css_class("kalam-reader-search");
    let s = sender.clone();
    search.connect_search_changed(move |entry| {
        s.input(ReaderMsg::AnnotationSearchChanged(entry.text().to_string()));
    });
    search_row.append(&search);
    wrap.append(&search_row);

    let chips = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    chips.add_css_class("kalam-reader-filter-row");
    chips.set_margin_top(10);
    chips.set_margin_start(12);
    chips.set_margin_end(12);
    chips.set_margin_bottom(10);

    let mut buttons = Vec::new();
    for (label, filter, class_name) in [
        ("All", HighlightFilter::All, None),
        (
            "Yellow",
            HighlightFilter::Yellow,
            Some("kalam-reader-filter-yellow"),
        ),
        (
            "Green",
            HighlightFilter::Green,
            Some("kalam-reader-filter-green"),
        ),
        (
            "Blue",
            HighlightFilter::Blue,
            Some("kalam-reader-filter-blue"),
        ),
        (
            "Pink",
            HighlightFilter::Pink,
            Some("kalam-reader-filter-pink"),
        ),
        (
            "Orange",
            HighlightFilter::Orange,
            Some("kalam-reader-filter-orange"),
        ),
        (
            "Underline",
            HighlightFilter::Underline,
            Some("kalam-reader-filter-underline"),
        ),
        ("Quotes", HighlightFilter::Quotes, None),
    ] {
        let btn = gtk::Button::with_label(label);
        btn.add_css_class("kalam-reader-filter-chip");
        if let Some(class_name) = class_name {
            btn.add_css_class(class_name);
        }
        let s = sender.clone();
        btn.connect_clicked(move |_| s.input(ReaderMsg::SetHighlightFilter(filter)));
        chips.append(&btn);
        buttons.push((filter, btn));
    }
    wrap.append(&chips);

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .child(list)
        .build();
    scroll.add_css_class("kalam-reader-panel-scroll");
    wrap.append(&scroll);

    (wrap, buttons)
}

pub(crate) fn build_bookmarks_panel(sender: &ComponentSender<ReaderModel>, list: &gtk::Box) -> gtk::Box {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_margin_all(12);
    let add_btn = gtk::Button::new();
    add_btn.set_child(Some(&crate::icons::labelled(
        "bookmark-new-symbolic",
        16,
        "Add current place",
        6,
    )));
    add_btn.add_css_class("kalam-btn-tonal");
    let s = sender.clone();
    add_btn.connect_clicked(move |_| s.input(ReaderMsg::AddBookmark));
    actions.append(&add_btn);
    wrap.append(&actions);

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .child(list)
        .build();
    scroll.add_css_class("kalam-reader-panel-scroll");
    wrap.append(&scroll);
    wrap
}

pub(crate) fn build_words_panel(
    sender: &ComponentSender<ReaderModel>,
    _scope: WordScope,
    list: &gtk::Box,
) -> (gtk::Box, Vec<(WordScope, gtk::Button)>, gtk::SearchEntry) {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    search_row.add_css_class("kalam-reader-search-row");
    search_row.set_margin_all(12);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Look up a word…"));
    search.set_hexpand(true);
    search.add_css_class("kalam-reader-search");
    let s = sender.clone();
    search.connect_search_changed(move |entry| {
        s.input(ReaderMsg::DictSearch(entry.text().to_string()));
    });
    search_row.append(&search);
    wrap.append(&search_row);

    let chips = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    chips.add_css_class("kalam-reader-filter-row");
    chips.set_margin_start(12);
    chips.set_margin_end(12);
    chips.set_margin_bottom(10);
    let mut buttons = Vec::new();
    for (label, scope) in [
        ("Chapter", WordScope::Chapter),
        ("This book", WordScope::Book),
        ("All", WordScope::All),
    ] {
        let btn = gtk::Button::with_label(label);
        btn.add_css_class("kalam-reader-filter-chip");
        let s = sender.clone();
        btn.connect_clicked(move |_| s.input(ReaderMsg::SetWordScope(scope)));
        chips.append(&btn);
        buttons.push((scope, btn));
    }
    wrap.append(&chips);

    let scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vscrollbar_policy(gtk::PolicyType::Automatic)
        .hexpand(true)
        .vexpand(true)
        .child(list)
        .build();
    scroll.add_css_class("kalam-reader-panel-scroll");
    wrap.append(&scroll);

    (wrap, buttons, search)
}
