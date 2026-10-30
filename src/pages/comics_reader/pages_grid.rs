//! The Pages tab's whole-series thumbnail grid (decision 38).
//!
//! `GtkGridView` owns the window: it creates cells only for the rows
//! around the viewport and recycles them as the reader scrolls. A cell
//! therefore has two separate lives — the recycled widget built once
//! in `setup`, and the series page it is currently bound to. Binding a
//! page is also the moment its thumbnail becomes worth decoding, so a
//! thousand-page series never asks for a thousand decodes just because
//! the Pages tab opened.

use super::types::{ComicsReaderMsg, SeriesPageRef};
use gtk::gdk;
use gtk::glib::BoxedAnyObject;
use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

const COLUMNS: usize = 3;
const CELL_WIDTH: i32 = 86;
const CELL_HEIGHT: i32 = 112;
const CELL_GAP: i32 = 12;
const ROW_PITCH: f64 = (CELL_HEIGHT + CELL_GAP) as f64;
/// Enough decoded thumbnails for forty-eight rows around the reader.
/// The bound is what keeps a long scroll through a whole series from
/// turning the lazy grid into a second, unbounded texture cache.
const MAX_CACHED_THUMBNAILS: usize = 144;

/// One item in the Pages grid's list model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SeriesPageEntry {
    pub page: SeriesPageRef,
    pub book_id: Option<i64>,
}

impl SeriesPageEntry {
    pub const fn new(page: SeriesPageRef, book_id: Option<i64>) -> Self {
        Self { page, book_id }
    }
}

/// Flatten a series' chapters into grid order. Chapters whose page
/// count could not be listed are skipped rather than invented; the
/// chapter actually open always uses its live provider count, so the
/// current page can always be ringed and scrolled to.
pub fn build_series_page_entries(
    book_ids: &[i64],
    chapter_page_counts: &[Option<usize>],
    current_chapter_idx: Option<usize>,
    current_total_pages: usize,
) -> Vec<SeriesPageEntry> {
    let mut entries = Vec::new();
    for (chapter_idx, book_id) in book_ids.iter().copied().enumerate() {
        let count = if current_chapter_idx == Some(chapter_idx) {
            current_total_pages
        } else {
            chapter_page_counts
                .get(chapter_idx)
                .copied()
                .flatten()
                .unwrap_or(0)
        };
        entries.extend((0..count).map(move |page_idx| {
            SeriesPageEntry::new(
                SeriesPageRef::new(Some(chapter_idx), page_idx),
                Some(book_id),
            )
        }));
    }

    let current_chapter_present = entries
        .iter()
        .any(|entry| entry.page.chapter_idx == current_chapter_idx);
    if current_total_pages > 0 && !current_chapter_present {
        // A standalone or otherwise unplaced chapter: its own pages
        // are the whole grid, exactly as the reader can honestly show.
        let book_id = current_chapter_idx.and_then(|idx| book_ids.get(idx).copied());
        return (0..current_total_pages)
            .map(|page_idx| {
                SeriesPageEntry::new(
                    SeriesPageRef::new(current_chapter_idx, page_idx),
                    book_id,
                )
            })
            .collect();
    }
    entries
}

/// The grid position at the top-left of the first visible row.
pub fn first_visible_position(entry_count: usize, scroll_value: f64) -> Option<usize> {
    if entry_count == 0 {
        return None;
    }
    let row = (scroll_value.max(0.0) / ROW_PITCH).floor() as usize;
    Some((row * COLUMNS).min(entry_count - 1))
}

/// Open with one row of context above the current page where the
/// series has one, matching the drawn Pages tab rather than pinning
/// the current thumbnail to the very top.
pub fn scroll_target_for_position(position: usize) -> f64 {
    (position / COLUMNS).saturating_sub(1) as f64 * ROW_PITCH
}

/// The opening scroll after the viewport's real limit is known. Near
/// the end of a short series, clamping the row-aligned target to the
/// adjustment maximum would leave the first row cut in half; prefer
/// the last whole-row offset when it still shows the current cell in
/// full, and only use the hard maximum when the current page is in
/// the final row and needs that last partial-row travel.
pub fn opening_scroll_target_for_position(
    position: usize,
    max_scroll: f64,
    viewport_height: f64,
) -> f64 {
    let max_scroll = max_scroll.max(0.0);
    let desired = scroll_target_for_position(position);
    if desired <= max_scroll {
        return desired;
    }

    let aligned_max = (max_scroll / ROW_PITCH).floor() * ROW_PITCH;
    let current_row = position / COLUMNS;
    let current_bottom =
        current_row as f64 * ROW_PITCH + (CELL_GAP / 2 + CELL_HEIGHT) as f64;
    if aligned_max + viewport_height.max(0.0) >= current_bottom {
        aligned_max
    } else {
        max_scroll
    }
}

struct PageThumbCell {
    list_item: gtk::ListItem,
    button: gtk::Button,
    picture: gtk::Picture,
    status: gtk::Label,
    num: gtk::Label,
    bound_page: Option<SeriesPageRef>,
    click_handler: Option<gtk::glib::SignalHandlerId>,
}

#[derive(Default)]
struct PagesGridState {
    entries: Vec<SeriesPageEntry>,
    textures: HashMap<SeriesPageRef, gdk::Texture>,
    texture_order: VecDeque<SeriesPageRef>,
    failed: HashSet<SeriesPageRef>,
    current: Option<SeriesPageRef>,
    cells: Vec<PageThumbCell>,
}

impl PagesGridState {
    fn touch_thumbnail(&mut self, page: SeriesPageRef) {
        if self.textures.contains_key(&page) {
            self.texture_order.retain(|cached| *cached != page);
            self.texture_order.push_back(page);
        }
    }

    fn insert_thumbnail(&mut self, page: SeriesPageRef, texture: gdk::Texture) {
        self.failed.remove(&page);
        self.textures.insert(page, texture);
        self.touch_thumbnail(page);
        while self.textures.len() > MAX_CACHED_THUMBNAILS {
            let Some(oldest) = self.texture_order.pop_front() else {
                break;
            };
            if oldest != page {
                self.textures.remove(&oldest);
            }
        }
    }

    fn remove_thumbnail(&mut self, page: SeriesPageRef) {
        self.textures.remove(&page);
        self.texture_order.retain(|cached| *cached != page);
    }
}

fn page_description(entry: SeriesPageEntry) -> String {
    match entry.page.chapter_idx {
        Some(chapter_idx) => format!(
            "Chapter {} · Page {}",
            chapter_idx + 1,
            entry.page.page_idx + 1
        ),
        None => format!("Page {}", entry.page.page_idx + 1),
    }
}

fn paint_cell(
    cell: &PageThumbCell,
    texture: Option<&gdk::Texture>,
    failed: bool,
    current: bool,
) {
    match texture {
        Some(texture) => cell.picture.set_paintable(Some(texture)),
        None => cell.picture.set_paintable(None::<&gdk::Texture>),
    }
    cell.status
        .set_label(if failed { "Failed to load" } else { "" });
    if failed {
        cell.button.add_css_class("failed");
    } else {
        cell.button.remove_css_class("failed");
    }
    if current {
        cell.button.add_css_class("current");
    } else {
        cell.button.remove_css_class("current");
    }
}

fn clear_cell(cell: &PageThumbCell) {
    cell.picture.set_paintable(None::<&gdk::Texture>);
    cell.status.set_label("");
    cell.num.set_label("");
    cell.button.remove_css_class("failed");
    cell.button.remove_css_class("current");
}

/// The windowed, lazy Pages grid and the small amount of state its
/// recycled cells need to be repainted when a decode lands.
#[derive(Clone)]
pub struct SeriesPagesGrid {
    view: gtk::GridView,
    state: Rc<RefCell<PagesGridState>>,
}

impl SeriesPagesGrid {
    pub fn new(sender: relm4::Sender<ComicsReaderMsg>) -> Self {
        let state = Rc::new(RefCell::new(PagesGridState::default()));
        let factory = gtk::SignalListItemFactory::new();

        {
            let setup_state = state.clone();
            factory.connect_setup(move |_, object| {
                let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
                    return;
                };
                // The button inside is the interactive element; the
                // list item itself must not also claim activation,
                // selection or focus.
                list_item.set_activatable(false);
                list_item.set_selectable(false);
                list_item.set_focusable(false);

                let overlay = gtk::Overlay::new();
                overlay.set_size_request(CELL_WIDTH, CELL_HEIGHT);
                overlay.set_hexpand(true);
                overlay.set_vexpand(true);

                let picture = gtk::Picture::new();
                picture.set_content_fit(gtk::ContentFit::Cover);
                picture.set_can_shrink(true);
                picture.set_hexpand(true);
                picture.set_vexpand(true);
                picture.set_size_request(CELL_WIDTH, CELL_HEIGHT);
                overlay.set_child(Some(&picture));

                let status = gtk::Label::new(None);
                status.add_css_class("kalam-comics-page-status");
                status.set_halign(gtk::Align::Center);
                status.set_valign(gtk::Align::Center);
                status.set_wrap(true);
                status.set_justify(gtk::Justification::Center);
                overlay.add_overlay(&status);

                let num = gtk::Label::new(None);
                num.add_css_class("kalam-comics-page-num");
                num.set_halign(gtk::Align::Center);
                num.set_valign(gtk::Align::End);
                num.set_margin_bottom(6);
                overlay.add_overlay(&num);

                let button = gtk::Button::new();
                button.set_child(Some(&overlay));
                button.add_css_class("kalam-comics-page-thumb");
                button.set_hexpand(true);
                button.set_vexpand(true);
                // Half the 12px gap on every side: neighbouring
                // cells meet at a full gap, and the current page's
                // offset outline has room to stand clear of them.
                button.set_margin_start(CELL_GAP / 2);
                button.set_margin_end(CELL_GAP / 2);
                button.set_margin_top(CELL_GAP / 2);
                button.set_margin_bottom(CELL_GAP / 2);
                list_item.set_child(Some(&button));

                setup_state.borrow_mut().cells.push(PageThumbCell {
                    list_item: list_item.clone(),
                    button,
                    picture,
                    status,
                    num,
                    bound_page: None,
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
                    let Ok(entry_ref) = boxed.try_borrow::<SeriesPageEntry>() else {
                        return;
                    };
                    *entry_ref
                };

                {
                    let mut state = bind_state.borrow_mut();
                    let Some(cell_idx) = state
                        .cells
                        .iter()
                        .position(|cell| cell.list_item == *list_item)
                    else {
                        return;
                    };
                    let texture = state.textures.get(&entry.page).cloned();
                    if texture.is_some() {
                        state.touch_thumbnail(entry.page);
                    }
                    let failed = state.failed.contains(&entry.page);
                    let current = state.current == Some(entry.page);

                    let cell = &mut state.cells[cell_idx];
                    if let Some(handler) = cell.click_handler.take() {
                        cell.button.disconnect(handler);
                    }
                    let tx = bind_sender.clone();
                    let page = entry.page;
                    let handler = cell.button.connect_clicked(move |_| {
                        let _ = tx.send(ComicsReaderMsg::OpenSeriesPage(page));
                    });
                    cell.click_handler = Some(handler);
                    cell.bound_page = Some(entry.page);
                    cell.num
                        .set_label(&format!("{}", entry.page.page_idx + 1));
                    let description = page_description(entry);
                    cell.button.set_tooltip_text(Some(&description));
                    cell.button.update_property(&[gtk::accessible::Property::Label(
                        &description,
                    )]);
                    paint_cell(cell, texture.as_ref(), failed, current);
                }

                // Recycled into view is the lazy edge: ask for this
                // page's thumbnail now, and no earlier.
                let _ = bind_sender.send(ComicsReaderMsg::PageThumbnailVisible(entry.page));
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
                cell.bound_page = None;
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
        let view = gtk::GridView::new(Some(selection), Some(factory));
        view.add_css_class("kalam-comics-pages-grid");
        view.set_min_columns(COLUMNS as u32);
        view.set_max_columns(COLUMNS as u32);
        view.set_enable_rubberband(false);
        view.set_single_click_activate(false);
        view.set_hexpand(true);
        view.set_vexpand(true);
        // Together with each cell's half-gap margins: 20px panel
        // insets left and right, 12px below the sticky heading.
        view.set_margin_start(14);
        view.set_margin_end(14);
        view.set_margin_top(6);
        view.set_margin_bottom(6);

        Self { view, state }
    }

    pub fn widget(&self) -> gtk::GridView {
        self.view.clone()
    }

    pub fn entry_count(&self) -> usize {
        self.state.borrow().entries.len()
    }

    pub fn book_id_for_page(&self, page: SeriesPageRef) -> Option<i64> {
        self.state
            .borrow()
            .entries
            .iter()
            .find(|entry| entry.page == page)
            .and_then(|entry| entry.book_id)
    }

    /// Replace the list model only when the series shape has really
    /// changed. Rebuilding on every panel refresh would throw away
    /// the reader's scroll position and rebind the standing window.
    pub fn set_entries(
        &self,
        entries: Vec<SeriesPageEntry>,
        current: Option<SeriesPageRef>,
    ) {
        let changed = self.state.borrow().entries != entries;
        if !changed {
            self.set_current(current);
            return;
        }

        let objects = {
            let mut state = self.state.borrow_mut();
            let valid: HashSet<SeriesPageRef> =
                entries.iter().map(|entry| entry.page).collect();
            state
                .textures
                .retain(|page, _| valid.contains(page));
            state.texture_order.retain(|page| valid.contains(page));
            state.failed.retain(|page| valid.contains(page));
            state.entries = entries.clone();
            state.current = current.filter(|page| valid.contains(page));
            entries
                .iter()
                .copied()
                .map(BoxedAnyObject::new)
                .collect::<Vec<_>>()
        };

        let store = gtk::gio::ListStore::new::<BoxedAnyObject>();
        for object in &objects {
            store.append(object);
        }
        let selection = gtk::NoSelection::new(Some(store));
        self.view.set_model(Some(&selection));
        self.set_current(current);
    }

    pub fn set_current(&self, current: Option<SeriesPageRef>) {
        let mut state = self.state.borrow_mut();
        let current = current.filter(|page| {
            state
                .entries
                .iter()
                .any(|entry| entry.page == *page)
        });
        state.current = current;
        for cell in &state.cells {
            if cell.bound_page.is_some() && cell.bound_page == current {
                cell.button.add_css_class("current");
            } else {
                cell.button.remove_css_class("current");
            }
        }
    }

    pub fn current_position(&self) -> Option<usize> {
        let state = self.state.borrow();
        let current = state.current?;
        state
            .entries
            .iter()
            .position(|entry| entry.page == current)
    }

    pub fn scroll_to_current(&self) {
        if let Some(position) = self.current_position() {
            self.view.scroll_to(
                position as u32,
                gtk::ListScrollFlags::empty(),
                None,
            );
        }
    }

    pub fn opening_scroll_target(
        &self,
        max_scroll: f64,
        viewport_height: f64,
    ) -> Option<f64> {
        self.current_position().map(|position| {
            opening_scroll_target_for_position(position, max_scroll, viewport_height)
        })
    }

    pub fn first_visible_page(&self, scroll_value: f64) -> Option<SeriesPageRef> {
        let state = self.state.borrow();
        let position = first_visible_position(state.entries.len(), scroll_value)?;
        state.entries.get(position).map(|entry| entry.page)
    }

    pub fn has_thumbnail_state(&self, page: SeriesPageRef) -> bool {
        let state = self.state.borrow();
        state.textures.contains_key(&page) || state.failed.contains(&page)
    }

    pub fn update_thumbnail(&self, page: SeriesPageRef, texture: Option<gdk::Texture>) {
        let mut state = self.state.borrow_mut();
        if !state.entries.iter().any(|entry| entry.page == page) {
            return;
        }
        let current = state.current == Some(page);
        match texture {
            Some(texture) => {
                state.insert_thumbnail(page, texture.clone());
                for cell in &state.cells {
                    if cell.bound_page == Some(page) {
                        paint_cell(cell, Some(&texture), false, current);
                    }
                }
            }
            None => {
                state.remove_thumbnail(page);
                state.failed.insert(page);
                for cell in &state.cells {
                    if cell.bound_page == Some(page) {
                        paint_cell(cell, None, true, current);
                    }
                }
            }
        }
    }

    pub fn clear_thumbnails(&self) {
        let mut state = self.state.borrow_mut();
        state.textures.clear();
        state.texture_order.clear();
        state.failed.clear();
        let current = state.current;
        for cell in &state.cells {
            if cell.bound_page.is_some() {
                paint_cell(cell, None, false, cell.bound_page == current);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entries_cover_known_chapters_and_restart_page_numbers() {
        let entries = build_series_page_entries(
            &[10, 20, 30],
            &[Some(2), None, Some(3)],
            Some(2),
            3,
        );

        assert_eq!(entries.len(), 5);
        assert_eq!(
            entries[0],
            SeriesPageEntry::new(SeriesPageRef::new(Some(0), 0), Some(10))
        );
        assert_eq!(
            entries[1],
            SeriesPageEntry::new(SeriesPageRef::new(Some(0), 1), Some(10))
        );
        // The unreadable middle chapter is skipped; chapter 3's
        // first thumbnail is still its own page 1.
        assert_eq!(
            entries[2],
            SeriesPageEntry::new(SeriesPageRef::new(Some(2), 0), Some(30))
        );
        assert_eq!(
            entries[4],
            SeriesPageEntry::new(SeriesPageRef::new(Some(2), 2), Some(30))
        );
    }

    #[test]
    fn standalone_chapter_entries_use_the_live_page_count() {
        let entries = build_series_page_entries(&[], &[], None, 2);
        assert_eq!(
            entries,
            vec![
                SeriesPageEntry::new(SeriesPageRef::new(None, 0), None),
                SeriesPageEntry::new(SeriesPageRef::new(None, 1), None),
            ]
        );
    }

    #[test]
    fn first_visible_row_and_opening_scroll_use_the_fixed_cell_pitch() {
        assert_eq!(first_visible_position(24, 0.0), Some(0));
        assert_eq!(first_visible_position(24, ROW_PITCH - 1.0), Some(0));
        assert_eq!(first_visible_position(24, ROW_PITCH), Some(3));
        assert_eq!(first_visible_position(24, 100_000.0), Some(23));
        assert_eq!(first_visible_position(0, 0.0), None);

        assert_eq!(scroll_target_for_position(0), 0.0);
        assert_eq!(scroll_target_for_position(3), 0.0);
        assert_eq!(scroll_target_for_position(6), ROW_PITCH);

        // A short series whose maximum falls mid-row still opens on
        // a whole row when the current thumbnail remains in full.
        assert_eq!(
            opening_scroll_target_for_position(15, 187.0, 805.0),
            ROW_PITCH
        );
        // In the final row, keeping the current page fully visible
        // wins over row alignment.
        assert_eq!(
            opening_scroll_target_for_position(21, 187.0, 805.0),
            187.0
        );
    }
}
