//! The editor's front door: pick a book to edit.
//!
//! The full editor (Phase 6) is per-book — it opens on one file, so every
//! earlier entry point knew the book already: the reader's TOC pencil for
//! the book being read, the book's own page. The sidebar entry has no book,
//! and that is the point: like Calibre's "Edit book", you arrive at the
//! editor first and choose the file from there.
//!
//! This page is that choice. Every EPUB in the library, one click, the
//! full editor opens on the book's own file. Non-EPUBs are not listed:
//! the editor's whole pipeline — patches, the bake, the source pane —
//! is EPUB-shaped, so a listed book it would refuse to open is a broken
//! promise, not a feature.

use crate::db::SortKey;
use crate::models::{Book, BookFormat};
use crate::service::LibraryService;
use crate::widgets::book_row::build_book_grid_open;
use gtk::prelude::*;
use relm4::prelude::*;

#[derive(Debug)]
pub enum EditorPickerOut {
    OpenEditor { book_id: i64 },
}

#[derive(Debug)]
pub enum EditorPickerMsg {
    SearchChanged(String),
}

pub struct EditorPickerModel {
    /// Every EPUB in the library, title-sorted, as loaded once at init.
    all: Vec<Book>,
    /// The slice of `all` the search box leaves on screen.
    shown: Vec<Book>,
    query: String,
    status: String,
}

#[relm4::component(pub)]
impl Component for EditorPickerModel {
    // A `LibraryService`, not the raw catalog handle: the ratchet in
    // tests/guardrails.rs holds pages to the service boundary, and this
    // page never needs the catalog past init — one snapshot read, then
    // it is pure in-memory filtering.
    type Init = LibraryService;
    type Input = EditorPickerMsg;
    type Output = EditorPickerOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,
            set_hexpand: true,

            gtk::Label {
                set_label: "Book editor",
                add_css_class: "kalam-page-title",
                set_halign: gtk::Align::Start,
            },

            gtk::Label {
                set_label: "Pick a book to edit — the editor opens on the book's own file.",
                add_css_class: "kalam-muted",
                set_halign: gtk::Align::Start,
            },

            gtk::SearchEntry {
                set_hexpand: true,
                set_placeholder_text: Some("Search title or author…"),
                connect_search_changed[sender] => move |entry| {
                    sender.input(EditorPickerMsg::SearchChanged(entry.text().to_string()));
                },
            },

            #[name = "status_label"]
            gtk::Label {
                #[watch]
                set_label: &model.status,
                add_css_class: "kalam-muted",
                set_halign: gtk::Align::Start,
            },

            #[name = "list"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                set_halign: gtk::Align::Start,
                set_hexpand: false,
                set_vexpand: false,
            },
        }
    }

    fn init(
        service: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Synchronous, like All Books' initial read: the page is not on
        // screen yet, so there is nothing to freeze, and the alternative —
        // an empty grid that fills in a beat later — is a worse first
        // impression of the editor's front door. The list is also smaller
        // than All Books' by construction: EPUBs only.
        let snap = service.all_books(SortKey::Title, "");
        let mut all: Vec<Book> = snap.books;
        all.retain(|b| b.format == BookFormat::Epub);
        let status = match snap.errors.first() {
            Some(err) => format!("Database error: {err}"),
            None => status_line(&all, ""),
        };

        let model = EditorPickerModel {
            shown: all.clone(),
            all,
            query: String::new(),
            status,
        };

        let widgets = view_output!();

        rebuild_list(&widgets.list, &model.shown, &model.query, &sender);

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
            EditorPickerMsg::SearchChanged(query) => {
                self.query = query;
                // Filter in place, not through the database: `all` is a
                // page-sized list of EPUBs already in memory, so a round
                // trip per keystroke would trade an instant reflow for a
                // flicker and nothing else.
                self.shown = self
                    .all
                    .iter()
                    .filter(|b| matches_query(b, &self.query))
                    .cloned()
                    .collect();
                self.status = status_line(&self.shown, &self.query);
                rebuild_list(&widgets.list, &self.shown, &self.query, &sender);
            }
        }
    }
}

/// Case-insensitive title or author match — the picker's whole query
/// language. Tags, status and rating filters belong to All Books, where
/// the full syntax lives; a picker that echoed it would be a second
/// implementation of the same search to keep in step.
fn matches_query(book: &Book, query: &str) -> bool {
    let q = query.trim().to_lowercase();
    if q.is_empty() {
        return true;
    }
    book.title.to_lowercase().contains(&q) || book.authors.to_lowercase().contains(&q)
}

fn status_line(books: &[Book], query: &str) -> String {
    let n = books.len();
    if n == 0 {
        if query.trim().is_empty() {
            "No EPUB books yet — import one from All Books to edit it.".into()
        } else {
            "No matches.".into()
        }
    } else {
        format!(
            "{n} EPUB book{} · click a cover to edit",
            if n == 1 { "" } else { "s" }
        )
    }
}

fn rebuild_list(
    list: &gtk::Box,
    books: &[Book],
    query: &str,
    sender: &ComponentSender<EditorPickerModel>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if books.is_empty() {
        let q = query.trim();
        let message = if q.is_empty() {
            "The library has no EPUB books yet.\nThe editor edits EPUB files — import one from All Books, then come back here.".to_string()
        } else {
            format!("No books match “{q}”.\nTry another search, or clear it to see every EPUB.")
        };
        let empty = gtk::Label::new(Some(&message));
        empty.add_css_class("kalam-placeholder");
        empty.set_wrap(true);
        list.append(&empty);
        return;
    }

    let s = sender.clone();
    list.append(&build_book_grid_open(books, move |book_id| {
        s.output(EditorPickerOut::OpenEditor { book_id }).ok();
    }));
}
