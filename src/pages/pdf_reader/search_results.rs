//! The PDF search results list (decision 61).
//!
//! `GtkListView` keeps this as lazy as the Pages grid: the model
//! can hold every match the document service found, while GTK
//! only materialises the rows around the panel's viewport. The
//! model in `pdf_reader.rs` decides which results the standing
//! Chapter / Book scope can see and groups them; this module
//! only paints those entries and reports a click back as the
//! raw result index it was handed.

use super::PdfReaderMsg;
use gtk::glib::BoxedAnyObject;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;

/// The two token colours a snippet needs (design-system §9):
/// `match` is the palette text at a whisper, `match-current`
/// the accent at 38%. They travel as literals because Pango
/// markup cannot read GTK's `@define-color` names; the caller
/// derives them from the theme in force, never invents them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PdfSearchSnippetPaint {
    pub match_colour: String,
    pub match_alpha_percent: u32,
    pub current_colour: String,
}

impl Default for PdfSearchSnippetPaint {
    fn default() -> Self {
        Self {
            match_colour: "#e8ddd5".to_string(),
            match_alpha_percent: 10,
            current_colour: "#d4552a".to_string(),
        }
    }
}

/// One row in the results model: a chapter heading, a match, or
/// the one quiet line that stands in for an empty scope.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PdfSearchResultsEntry {
    Heading(String),
    Result {
        raw_index: usize,
        page: usize,
        snippet: String,
    },
    Message(String),
}

pub fn plain_snippet(snippet: &str) -> String {
    snippet.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn chars_match(left: char, right: char) -> bool {
    left == right || left.to_lowercase().eq(right.to_lowercase())
}

fn match_ranges(snippet: &[char], query: &[char]) -> Vec<(usize, usize)> {
    if query.is_empty() || snippet.len() < query.len() {
        return Vec::new();
    }
    let mut ranges = Vec::new();
    let mut start = 0;
    while start + query.len() <= snippet.len() {
        if snippet[start..start + query.len()]
            .iter()
            .zip(query)
            .all(|(candidate, wanted)| chars_match(*candidate, *wanted))
        {
            ranges.push((start, start + query.len()));
            start += query.len();
        } else {
            start += 1;
        }
    }
    ranges
}

/// Pango markup for one snippet: the text escaped, every match
/// bold on its tint. The current result takes the accent tint;
/// its row carries the accent bar besides (decision 61).
pub fn format_search_snippet(
    snippet: &str,
    query: &str,
    current: bool,
    paint: &PdfSearchSnippetPaint,
) -> String {
    let plain = plain_snippet(snippet);
    let chars: Vec<char> = plain.chars().collect();
    let query_chars: Vec<char> = plain_snippet(query).chars().collect();
    let ranges = match_ranges(&chars, &query_chars);
    if ranges.is_empty() {
        return gtk::glib::markup_escape_text(&plain).to_string();
    }

    let (colour, alpha) = if current {
        (paint.current_colour.as_str(), 38)
    } else {
        (
            paint.match_colour.as_str(),
            paint.match_alpha_percent,
        )
    };

    let mut markup = String::new();
    let mut cursor = 0;
    for (start, end) in ranges {
        if cursor < start {
            let text: String = chars[cursor..start].iter().collect();
            markup.push_str(&gtk::glib::markup_escape_text(&text));
        }
        let text: String = chars[start..end].iter().collect();
        markup.push_str(&format!(
            "<span weight=\"bold\" background=\"{colour}\" background_alpha=\"{alpha}%\">{}</span>",
            gtk::glib::markup_escape_text(&text)
        ));
        cursor = end;
    }
    if cursor < chars.len() {
        let text: String = chars[cursor..].iter().collect();
        markup.push_str(&gtk::glib::markup_escape_text(&text));
    }
    markup
}

struct SearchResultCell {
    list_item: gtk::ListItem,
    heading: gtk::Label,
    message: gtk::Label,
    button: gtk::Button,
    bar: gtk::Box,
    page: gtk::Label,
    snippet: gtk::Label,
    bound_entry: Option<PdfSearchResultsEntry>,
    click_handler: Option<gtk::glib::SignalHandlerId>,
}

struct SearchResultsState {
    entries: Vec<PdfSearchResultsEntry>,
    query: String,
    current_raw_index: Option<usize>,
    paint: PdfSearchSnippetPaint,
    cells: Vec<SearchResultCell>,
}

fn paint_cell(cell: &SearchResultCell, state: &SearchResultsState) {
    let Some(entry) = &cell.bound_entry else {
        cell.heading.set_visible(false);
        cell.message.set_visible(false);
        cell.button.set_visible(false);
        return;
    };

    match entry {
        PdfSearchResultsEntry::Heading(title) => {
            cell.heading.set_label(title);
            cell.heading.set_visible(true);
            cell.message.set_visible(false);
            cell.button.set_visible(false);
        }
        PdfSearchResultsEntry::Message(text) => {
            cell.message.set_label(text);
            cell.heading.set_visible(false);
            cell.message.set_visible(true);
            cell.button.set_visible(false);
        }
        PdfSearchResultsEntry::Result {
            raw_index,
            page,
            snippet,
        } => {
            let current = state.current_raw_index == Some(*raw_index);
            cell.heading.set_visible(false);
            cell.message.set_visible(false);
            cell.button.set_visible(true);
            cell.page.set_label(&format!("p. {page}"));
            cell.snippet.set_markup(&format_search_snippet(
                snippet,
                &state.query,
                current,
                &state.paint,
            ));
            if current {
                cell.button.add_css_class("current");
                cell.bar.add_css_class("current");
            } else {
                cell.button.remove_css_class("current");
                cell.bar.remove_css_class("current");
            }
            let description = format!(
                "{}Page {page} search result: {}",
                if current { "Current result. " } else { "" },
                plain_snippet(snippet)
            );
            cell.button.set_tooltip_text(Some(&description));
            cell.button
                .update_property(&[gtk::accessible::Property::Label(&description)]);
        }
    }
}

fn clear_cell(cell: &SearchResultCell) {
    cell.heading.set_label("");
    cell.message.set_label("");
    cell.page.set_label("");
    cell.snippet.set_label("");
    cell.button.remove_css_class("current");
    cell.bar.remove_css_class("current");
}

/// The standing lazy list. Rows are recycled by GTK; only the
/// entry list, the query, the current raw index and the bound
/// cells live here.
#[derive(Clone)]
pub struct PdfSearchResultsList {
    view: gtk::ListView,
    state: Rc<RefCell<SearchResultsState>>,
}

impl PdfSearchResultsList {
    pub fn new(sender: relm4::Sender<PdfReaderMsg>) -> Self {
        let state = Rc::new(RefCell::new(SearchResultsState {
            entries: Vec::new(),
            query: String::new(),
            current_raw_index: None,
            paint: PdfSearchSnippetPaint::default(),
            cells: Vec::new(),
        }));
        let factory = gtk::SignalListItemFactory::new();

        {
            let setup_state = state.clone();
            factory.connect_setup(move |_, object| {
                let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                list_item.set_activatable(false);
                list_item.set_selectable(false);
                list_item.set_focusable(false);

                let heading = gtk::Label::new(None);
                heading.add_css_class("kalam-pdf-search-results-heading");
                heading.set_halign(gtk::Align::Fill);
                heading.set_xalign(0.0);
                heading.set_hexpand(true);
                heading.set_ellipsize(gtk::pango::EllipsizeMode::End);
                heading.set_width_chars(1);

                let message = gtk::Label::new(None);
                message.add_css_class("kalam-pdf-search-results-message");
                message.set_halign(gtk::Align::Fill);
                message.set_xalign(0.0);
                message.set_hexpand(true);
                message.set_wrap(true);

                let bar = gtk::Box::new(gtk::Orientation::Vertical, 0);
                bar.add_css_class("kalam-pdf-search-result-bar");
                bar.set_size_request(3, -1);
                bar.set_valign(gtk::Align::Fill);
                bar.set_vexpand(true);

                let page = gtk::Label::new(None);
                page.add_css_class("kalam-pdf-search-result-page");
                page.set_width_request(44);
                page.set_margin_start(17);
                page.set_valign(gtk::Align::Start);
                page.set_xalign(0.0);

                let snippet = gtk::Label::new(None);
                snippet.add_css_class("kalam-pdf-search-result-snippet");
                snippet.set_use_markup(true);
                snippet.set_wrap(true);
                snippet.set_wrap_mode(gtk::pango::WrapMode::WordChar);
                snippet.set_lines(3);
                snippet.set_ellipsize(gtk::pango::EllipsizeMode::End);
                snippet.set_halign(gtk::Align::Fill);
                snippet.set_xalign(0.0);
                snippet.set_hexpand(true);
                snippet.set_margin_start(10);

                let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                row.append(&bar);
                row.append(&page);
                row.append(&snippet);

                let button = gtk::Button::new();
                button.add_css_class("kalam-pdf-search-result-row");
                button.set_child(Some(&row));
                button.set_hexpand(true);

                let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
                root.add_css_class("kalam-pdf-search-result-cell");
                root.set_hexpand(true);
                root.append(&heading);
                root.append(&message);
                root.append(&button);
                list_item.set_child(Some(&root));

                setup_state.borrow_mut().cells.push(SearchResultCell {
                    list_item: list_item.clone(),
                    heading,
                    message,
                    button,
                    bar,
                    page,
                    snippet,
                    bound_entry: None,
                    click_handler: None,
                });
            });
        }

        {
            let bind_state = state.clone();
            let bind_sender = sender.clone();
            factory.connect_bind(move |_, object| {
                let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                let Some(boxed) = list_item.item().and_downcast::<BoxedAnyObject>() else {
                    return;
                };
                let entry = {
                    let Ok(entry_ref) = boxed.try_borrow::<PdfSearchResultsEntry>() else {
                        return;
                    };
                    entry_ref.clone()
                };

                let mut state = bind_state.borrow_mut();
                let Some(cell_idx) = state
                    .cells
                    .iter()
                    .position(|cell| cell.list_item == *list_item)
                else {
                    return;
                };
                let click_target = match &entry {
                    PdfSearchResultsEntry::Result { raw_index, .. } => Some(*raw_index),
                    _ => None,
                };
                {
                    let cell = &mut state.cells[cell_idx];
                    if let Some(handler) = cell.click_handler.take() {
                        cell.button.disconnect(handler);
                    }
                    if let Some(raw_index) = click_target {
                        let tx = bind_sender.clone();
                        let handler = cell.button.connect_clicked(move |_| {
                            let _ = tx.send(PdfReaderMsg::JumpToSearchResult(raw_index));
                        });
                        cell.click_handler = Some(handler);
                    }
                    cell.bound_entry = Some(entry);
                }
                let cell = &state.cells[cell_idx];
                paint_cell(cell, &state);
            });
        }

        {
            let unbind_state = state.clone();
            factory.connect_unbind(move |_, object| {
                let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                let mut state = unbind_state.borrow_mut();
                let Some(cell) = state
                    .cells
                    .iter_mut()
                    .find(|cell| cell.list_item == *list_item)
                else {
                    return;
                };
                if let Some(handler) = cell.click_handler.take() {
                    cell.button.disconnect(handler);
                }
                cell.bound_entry = None;
                clear_cell(cell);
            });
        }

        {
            let teardown_state = state.clone();
            factory.connect_teardown(move |_, object| {
                let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                let removed = {
                    let mut state = teardown_state.borrow_mut();
                    let Some(cell_idx) = state
                        .cells
                        .iter()
                        .position(|cell| cell.list_item == *list_item)
                    else {
                        return;
                    };
                    state.cells.remove(cell_idx)
                };
                if let Some(handler) = removed.click_handler {
                    removed.button.disconnect(handler);
                }
            });
        }

        let empty_store = gtk::gio::ListStore::new::<BoxedAnyObject>();
        let selection = gtk::NoSelection::new(Some(empty_store));
        let view = gtk::ListView::new(Some(selection), Some(factory));
        view.add_css_class("kalam-pdf-search-results-list");
        view.set_show_separators(false);
        view.set_single_click_activate(false);
        view.set_hexpand(true);
        view.set_vexpand(true);

        Self { view, state }
    }

    pub fn widget(&self) -> gtk::ListView {
        self.view.clone()
    }

    /// Replace the model only when the grouped entries, query or
    /// tint actually moved; a Next / Previous step is just a new
    /// current row on the standing list.
    pub fn set_results(
        &self,
        entries: Vec<PdfSearchResultsEntry>,
        query: String,
        current_raw_index: Option<usize>,
        paint: PdfSearchSnippetPaint,
    ) {
        let objects = {
            let mut state = self.state.borrow_mut();
            if state.entries == entries && state.query == query && state.paint == paint {
                drop(state);
                self.set_current(current_raw_index);
                return;
            }
            state.entries = entries;
            state.query = query;
            state.current_raw_index = current_raw_index;
            state.paint = paint;
            state
                .entries
                .iter()
                .cloned()
                .map(BoxedAnyObject::new)
                .collect::<Vec<_>>()
        };

        let store = gtk::gio::ListStore::new::<BoxedAnyObject>();
        for object in &objects {
            store.append(object);
        }
        let selection = gtk::NoSelection::new(Some(store));
        self.view.set_model(Some(&selection));
    }

    pub fn set_current(&self, current_raw_index: Option<usize>) {
        let mut state = self.state.borrow_mut();
        if state.current_raw_index == current_raw_index {
            return;
        }
        state.current_raw_index = current_raw_index;
        for cell in &state.cells {
            if matches!(
                cell.bound_entry,
                Some(PdfSearchResultsEntry::Result { .. })
            ) {
                paint_cell(cell, &state);
            }
        }
    }

    pub fn scroll_to_current(&self) {
        let position = {
            let state = self.state.borrow();
            let Some(current) = state.current_raw_index else {
                return;
            };
            let Some(position) = state.entries.iter().position(|entry| {
                matches!(
                    entry,
                    PdfSearchResultsEntry::Result { raw_index, .. }
                        if *raw_index == current
                )
            }) else {
                return;
            };
            position
        };
        let view = self.view.clone();
        gtk::glib::idle_add_local_once(move || {
            view.scroll_to(position as u32, gtk::ListScrollFlags::empty(), None);
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paint() -> PdfSearchSnippetPaint {
        PdfSearchSnippetPaint {
            match_colour: "#111111".to_string(),
            match_alpha_percent: 10,
            current_colour: "#aa0000".to_string(),
        }
    }

    #[test]
    fn snippet_markup_bolds_every_match_on_its_tint() {
        let markup = format_search_snippet(
            "…shield Volcanoes and a volcano…",
            "volcano",
            false,
            &paint(),
        );
        assert_eq!(markup.matches("background=\"#111111\"").count(), 2);
        assert!(markup.contains("background_alpha=\"10%\""));
        assert!(markup.contains(">Volcano</span>es"));
        assert!(markup.contains(">volcano</span>"));
    }

    #[test]
    fn current_snippet_markup_uses_the_accent_tint() {
        let markup = format_search_snippet("a volcano", "volcano", true, &paint());
        assert!(markup.contains("background=\"#aa0000\""));
        assert!(markup.contains("background_alpha=\"38%\""));
    }

    #[test]
    fn snippet_markup_escapes_the_page_text() {
        let markup = format_search_snippet("fish & <chips>", "chips", false, &paint());
        assert!(markup.contains("fish &amp; &lt;"));
        assert!(markup.contains(">chips</span>&gt;"));
    }
}
