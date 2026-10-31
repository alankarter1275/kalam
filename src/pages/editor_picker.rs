//! Editor picker (decision 142, design-system §40): the rail's
//! Editor entry lands here — "Edit a book", an underlined search,
//! then every EPUB in the library with its edited state. Opening a
//! row opens the workspace; a book's ⋯ menu skips this page entirely
//! (decision 86).
//!
//! The *Edits waiting* group belongs to the in-reader editor's typo
//! queue, which is built after the deep editor; until then the page
//! shows the one group that exists.

use crate::models::Book;
use crate::service::{EditorPickerEntry, LibraryService};
use crate::widgets::cover_state::{book_cover, CoverAvailability};
use gtk::prelude::*;
use relm4::prelude::*;

#[derive(Debug)]
pub enum EditorPickerOut {
    OpenEditor { book_id: i64 },
}

#[derive(Debug)]
pub enum EditorPickerMsg {
    Loaded {
        gen: u64,
        entries: Vec<EditorPickerEntry>,
    },
    SearchChanged(String),
    Refresh,
}

pub struct EditorPickerModel {
    service: LibraryService,
    entries: Vec<EditorPickerEntry>,
    loaded: bool,
    query: String,
    load_gen: u64,
}

#[relm4::component(pub)]
impl Component for EditorPickerModel {
    type Init = LibraryService;
    type Input = EditorPickerMsg;
    type Output = EditorPickerOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 0,
            add_css_class: "kalam-authorspage-inner",
            set_hexpand: true,
            set_vexpand: false,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 8,

                gtk::Label {
                    set_label: "EDITOR",
                    add_css_class: "kalam-authorpage-eyebrow",
                    set_halign: gtk::Align::Start,
                },
                gtk::Label {
                    set_label: "Edit a book",
                    add_css_class: "kalam-authorpage-title",
                    set_halign: gtk::Align::Start,
                },
            },

            #[name = "controls_host"]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 16,
                set_margin_top: 24,
                set_valign: gtk::Align::Center,
            },

            #[name = "content_host"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                set_margin_top: 28,
            },
        }
    }

    fn init(
        service: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let mut model = EditorPickerModel {
            service,
            entries: Vec::new(),
            loaded: false,
            query: String::new(),
            load_gen: 0,
        };
        let widgets = view_output!();

        // Underlined search (decision 45 via §40): 320 x 40.
        let search = gtk::SearchEntry::new();
        search.add_css_class("kalam-editorpicker-search");
        search.set_placeholder_text(Some("Search books…"));
        search.set_size_request(320, 40);
        search.set_hexpand(false);
        let s = sender.clone();
        search.connect_search_changed(move |entry| {
            s.input(EditorPickerMsg::SearchChanged(entry.text().to_string()));
        });
        widgets.controls_host.append(&search);

        model.rebuild(&widgets, &sender);
        model.request_entries(&sender);
        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        message: Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match message {
            EditorPickerMsg::Loaded { gen, entries } => {
                if gen != self.load_gen {
                    return;
                }
                self.entries = entries;
                self.loaded = true;
                self.rebuild(widgets, &sender);
            }
            EditorPickerMsg::SearchChanged(query) => {
                self.query = query;
                self.rebuild(widgets, &sender);
            }
            EditorPickerMsg::Refresh => {
                self.request_entries(&sender);
            }
        }
    }
}

impl EditorPickerModel {
    fn request_entries(&mut self, sender: &ComponentSender<Self>) {
        self.load_gen += 1;
        let gen = self.load_gen;
        let service = self.service.clone();
        let s = sender.input_sender().clone();
        crate::tasks::spawn_internal(
            "Listing books to edit",
            move |_reporter| service.editor_picker_entries(),
            |_| {},
            move |entries| {
                let _ = s.send(EditorPickerMsg::Loaded { gen, entries });
            },
        );
    }

    fn rebuild(&self, widgets: &EditorPickerModelWidgets, sender: &ComponentSender<Self>) {
        clear_box(&widgets.content_host);
        if !self.loaded {
            return;
        }
        if self.entries.is_empty() {
            widgets.content_host.append(&state_card(
                "No books to edit",
                "Add an EPUB to your library and it will be listed here for editing.",
            ));
            return;
        }

        let query = self.query.trim().to_ascii_lowercase();
        let visible: Vec<&EditorPickerEntry> = self
            .entries
            .iter()
            .filter(|entry| {
                query.is_empty()
                    || entry.book.title.to_ascii_lowercase().contains(&query)
                    || entry.book.authors.to_ascii_lowercase().contains(&query)
            })
            .collect();
        if visible.is_empty() {
            let label = gtk::Label::new(Some("No books match your search."));
            label.add_css_class("kalam-ui-meta");
            label.set_halign(gtk::Align::Start);
            label.set_margin_top(8);
            widgets.content_host.append(&label);
            return;
        }

        let heading = gtk::Label::new(Some("All EPUBs"));
        heading.add_css_class("kalam-editorpicker-group");
        heading.set_halign(gtk::Align::Start);
        heading.set_margin_bottom(4);
        widgets.content_host.append(&heading);

        for entry in visible {
            widgets.content_host.append(&picker_row(entry, sender));
        }
    }
}

/// One 72-high picker row (§40): 40 x 56 cover, title over author,
/// the edited state at the right.
fn picker_row(
    entry: &EditorPickerEntry,
    sender: &ComponentSender<EditorPickerModel>,
) -> gtk::Button {
    let book: &Book = &entry.book;
    let row = gtk::Button::new();
    row.add_css_class("kalam-editorpicker-row");
    row.set_size_request(-1, 72);
    row.set_tooltip_text(Some("Open in the editor"));

    let content = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    content.set_valign(gtk::Align::Center);
    content.append(&book_cover(
        book,
        40,
        56,
        false,
        CoverAvailability::Available,
        true,
    ));

    let titles = gtk::Box::new(gtk::Orientation::Vertical, 2);
    titles.set_valign(gtk::Align::Center);
    let title = gtk::Label::new(Some(&book.title));
    title.add_css_class("kalam-editorpicker-book-title");
    title.set_halign(gtk::Align::Start);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    titles.append(&title);
    if !book.authors.is_empty() {
        let author = gtk::Label::new(Some(&book.authors));
        author.add_css_class("kalam-ui-meta");
        author.set_halign(gtk::Align::Start);
        author.set_ellipsize(gtk::pango::EllipsizeMode::End);
        titles.append(&author);
    }
    content.append(&titles);

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    content.append(&spacer);

    let state = match &entry.last_edited_at {
        Some(stamp) => edited_label(stamp),
        None => "Not edited".to_string(),
    };
    let state_label = gtk::Label::new(Some(&state));
    state_label.add_css_class("kalam-ui-meta");
    state_label.set_valign(gtk::Align::Center);
    content.append(&state_label);

    row.set_child(Some(&content));
    let book_id = book.id;
    let s = sender.clone();
    row.connect_clicked(move |_| {
        let _ = s.output(EditorPickerOut::OpenEditor { book_id });
    });
    row
}

/// "2026-10-02T…" → "Edited 2 Oct" (decision 142).
fn edited_label(iso: &str) -> String {
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let month: usize = iso.get(5..7).and_then(|m| m.parse().ok()).unwrap_or(0);
    let day: u32 = iso.get(8..10).and_then(|d| d.parse().ok()).unwrap_or(0);
    if (1..=12).contains(&month) && day > 0 {
        format!("Edited {day} {}", MONTHS[month - 1])
    } else {
        "Edited".to_string()
    }
}

fn state_card(title: &str, body: &str) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    card.add_css_class("kalam-ui-state-card");
    card.set_halign(gtk::Align::Start);
    let title_label = gtk::Label::new(Some(title));
    title_label.add_css_class("kalam-editorpicker-book-title");
    title_label.set_halign(gtk::Align::Start);
    card.append(&title_label);
    let body_label = gtk::Label::new(Some(body));
    body_label.add_css_class("kalam-ui-meta");
    body_label.set_halign(gtk::Align::Start);
    card.append(&body_label);
    card
}

fn clear_box(host: &gtk::Box) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
}
