//! The Pages tab's lazy thumbnail grid (decision 59).
//!
//! `GtkGridView` windows the book exactly as it does in the comics
//! reader: cells exist only around the viewport, and binding a cell
//! is the first moment its low-resolution page is worth rendering.
//! The raster itself stays on the PDF reader's thumbnail worker;
//! this module owns only the recycled cells, their small LRU cache,
//! the current-spread outline, and the arithmetic a sticky heading
//! needs to follow the grid.

use super::image::{PdfImageSettings, PdfPagePicture};
use super::PdfReaderMsg;
use gtk::gdk;
use gtk::glib::BoxedAnyObject;
use gtk::prelude::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet, VecDeque};
use std::rc::Rc;

/// The raster width asked of MuPDF. It is comfortably above the
/// two-column cell's 144px display width without turning a screen
/// of thumbnails into full-size page renders.
pub const THUMBNAIL_TARGET_WIDTH: u32 = 256;

const GRID_MARGIN: i32 = 14;
const GRID_TOP_MARGIN: i32 = 6;
const CELL_GAP: i32 = 12;
const NUMBER_HEIGHT: i32 = 22;
/// Enough small textures for a long look around the current part
/// of a book, bounded so scrolling a thousand pages cannot quietly
/// build a second unbounded page cache.
const MAX_CACHED_THUMBNAILS: usize = 120;

fn sanitized_columns(columns: usize) -> usize {
    if columns == 2 { 2 } else { 3 }
}

fn sanitized_aspect(aspect: f64) -> f64 {
    if aspect.is_finite() && aspect > 0.0 {
        aspect.clamp(0.25, 3.0)
    } else {
        std::f64::consts::SQRT_2
    }
}

/// One thumbnail cell's displayed size: the whole page contained in
/// its own aspect, plus the board's page-number strip underneath.
/// The fixed widths are not arbitrary: with the panel's 340px, the
/// grid's 14px side margins and each cell's half of a 12px gap,
/// 92px makes three columns and 144px makes two, exactly.
pub fn thumbnail_cell_size(columns: usize, aspect_height_for_width: f64) -> (i32, i32) {
    let width = if sanitized_columns(columns) == 2 {
        144
    } else {
        92
    };
    let image_height = (width as f64 * sanitized_aspect(aspect_height_for_width))
        .round()
        .clamp(48.0, 280.0) as i32;
    (width, image_height + NUMBER_HEIGHT)
}

pub fn thumbnail_row_pitch(columns: usize, aspect_height_for_width: f64) -> f64 {
    thumbnail_cell_size(columns, aspect_height_for_width).1 as f64 + CELL_GAP as f64
}

/// The list position at the top-left of the first row still in view.
pub fn first_visible_position(
    entry_count: usize,
    columns: usize,
    aspect_height_for_width: f64,
    scroll_value: f64,
) -> Option<usize> {
    if entry_count == 0 {
        return None;
    }
    let columns = sanitized_columns(columns);
    let pitch = thumbnail_row_pitch(columns, aspect_height_for_width);
    let row = ((scroll_value.max(0.0) - GRID_TOP_MARGIN as f64).max(0.0) / pitch).floor() as usize;
    Some((row * columns).min(entry_count - 1))
}

/// The position that names a visible row for the sticky heading.
/// A row can straddle a chapter boundary: in three columns its
/// middle page gives the row to the section holding most of it,
/// and in two columns the right-hand page settles the tie in
/// favour of the section taking over the grid.
pub fn heading_anchor_position(
    entry_count: usize,
    columns: usize,
    aspect_height_for_width: f64,
    scroll_value: f64,
) -> Option<usize> {
    let first = first_visible_position(
        entry_count,
        columns,
        aspect_height_for_width,
        scroll_value,
    )?;
    Some((first + sanitized_columns(columns) / 2).min(entry_count - 1))
}

/// Open with one row of context above the current spread where the
/// book has one, as the comics Pages grid does.
pub fn scroll_target_for_position(
    position: usize,
    columns: usize,
    aspect_height_for_width: f64,
) -> f64 {
    let columns = sanitized_columns(columns);
    let row = (position / columns).saturating_sub(1);
    GRID_TOP_MARGIN as f64 + row as f64 * thumbnail_row_pitch(columns, aspect_height_for_width)
}

/// The opening target after the real scroll limit is known. A
/// short book's maximum can fall halfway through a row; keep the
/// row whole when the current thumbnail remains fully visible,
/// and spend the partial travel only in the final row.
pub fn opening_scroll_target_for_position(
    position: usize,
    columns: usize,
    aspect_height_for_width: f64,
    max_scroll: f64,
    viewport_height: f64,
) -> f64 {
    let max_scroll = max_scroll.max(0.0);
    let desired = scroll_target_for_position(position, columns, aspect_height_for_width);
    if desired <= max_scroll {
        return desired;
    }

    let columns = sanitized_columns(columns);
    let pitch = thumbnail_row_pitch(columns, aspect_height_for_width);
    let aligned_max = GRID_TOP_MARGIN as f64
        + (((max_scroll - GRID_TOP_MARGIN as f64).max(0.0) / pitch).floor() * pitch);
    let current_row = position / columns;
    let cell_height = thumbnail_cell_size(columns, aspect_height_for_width).1 as f64;
    let current_bottom = GRID_TOP_MARGIN as f64
        + current_row as f64 * pitch
        + (CELL_GAP / 2) as f64
        + cell_height;
    if aligned_max + viewport_height.max(0.0) >= current_bottom {
        aligned_max
    } else {
        max_scroll
    }
}

/// MuPDF's render scale for a thumbnail of `target_width` pixels.
/// Page dimensions arrive in the file's own orientation, so a
/// quarter-turned view measures its displayed width against the
/// source height.
pub fn thumbnail_render_scale(
    page_width: f64,
    page_height: f64,
    rotation_degrees: i32,
    target_width: u32,
) -> f32 {
    let quarter_turn = matches!(rotation_degrees.rem_euclid(360), 90 | 270);
    let effective_width = if quarter_turn {
        page_height
    } else {
        page_width
    };
    if !effective_width.is_finite() || effective_width <= 0.0 || target_width == 0 {
        return 0.5;
    }
    (target_width as f64 / effective_width).clamp(0.05, 4.0) as f32
}

struct PageThumbCell {
    list_item: gtk::ListItem,
    button: gtk::Button,
    overlay: gtk::Overlay,
    picture: PdfPagePicture,
    status: gtk::Label,
    num: gtk::Label,
    bound_page: Option<usize>,
    click_handler: Option<gtk::glib::SignalHandlerId>,
}

struct PagesGridState {
    entries: Vec<usize>,
    textures: HashMap<usize, gdk::Texture>,
    texture_order: VecDeque<usize>,
    failed: HashSet<usize>,
    current_pages: HashSet<usize>,
    columns: usize,
    aspect_height_for_width: f64,
    image_settings: PdfImageSettings,
    paper_rgb: [f32; 3],
    cells: Vec<PageThumbCell>,
}

impl PagesGridState {
    fn touch_thumbnail(&mut self, page: usize) {
        if self.textures.contains_key(&page) {
            self.texture_order.retain(|cached| *cached != page);
            self.texture_order.push_back(page);
        }
    }

    fn insert_thumbnail(&mut self, page: usize, texture: gdk::Texture) {
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

    fn remove_thumbnail(&mut self, page: usize) {
        self.textures.remove(&page);
        self.texture_order.retain(|cached| *cached != page);
    }
}

fn page_description(page: usize, total_pages: usize, current: bool) -> String {
    if current {
        format!("Page {page} of {total_pages}, in the current spread")
    } else {
        format!("Page {page} of {total_pages}")
    }
}

fn paint_cell(
    cell: &PageThumbCell,
    texture: Option<&gdk::Texture>,
    failed: bool,
    current: bool,
) {
    cell.picture.set_texture(texture);
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
    cell.picture.set_texture(None);
    cell.status.set_label("");
    cell.num.set_label("");
    cell.button.remove_css_class("failed");
    cell.button.remove_css_class("current");
}

fn apply_cell_size(cell: &PageThumbCell, columns: usize, aspect_height_for_width: f64) {
    let (width, total_height) = thumbnail_cell_size(columns, aspect_height_for_width);
    let image_height = total_height - NUMBER_HEIGHT;
    cell.overlay.set_size_request(width, image_height);
    cell.picture.set_size_request(width, image_height);
    cell.button.set_size_request(width, total_height);
}

/// The PDF Pages grid's standing GTK side. It is deliberately a
/// sibling of the comics grid rather than a shared abstraction:
/// these cells contain whole PDF pages in the book's aspect and
/// paint them through the PDF Image pane's colour transform, so
/// only the recycling pattern is really shared.
#[derive(Clone)]
pub struct PdfPagesGrid {
    view: gtk::GridView,
    state: Rc<RefCell<PagesGridState>>,
}

impl PdfPagesGrid {
    pub fn new(
        sender: relm4::Sender<PdfReaderMsg>,
        columns: usize,
        image_settings: PdfImageSettings,
        paper_rgb: [f32; 3],
    ) -> Self {
        let columns = sanitized_columns(columns);
        let state = Rc::new(RefCell::new(PagesGridState {
            entries: Vec::new(),
            textures: HashMap::new(),
            texture_order: VecDeque::new(),
            failed: HashSet::new(),
            current_pages: HashSet::new(),
            columns,
            aspect_height_for_width: std::f64::consts::SQRT_2,
            image_settings: image_settings.canonical(),
            paper_rgb,
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

                let (settings, paper_rgb) = {
                    let state = setup_state.borrow();
                    (state.image_settings, state.paper_rgb)
                };

                let overlay = gtk::Overlay::new();
                overlay.set_hexpand(true);
                overlay.set_vexpand(false);

                let picture = PdfPagePicture::new(settings, paper_rgb);
                picture.add_css_class("kalam-pdf-page-thumb-image");
                picture.set_hexpand(true);
                picture.set_vexpand(true);
                overlay.set_child(Some(&picture));

                let status = gtk::Label::new(None);
                status.add_css_class("kalam-pdf-page-thumb-status");
                status.set_halign(gtk::Align::Center);
                status.set_valign(gtk::Align::Center);
                status.set_wrap(true);
                status.set_justify(gtk::Justification::Center);
                overlay.add_overlay(&status);

                let num = gtk::Label::new(None);
                num.add_css_class("kalam-pdf-page-thumb-num");
                num.set_halign(gtk::Align::Center);
                num.set_valign(gtk::Align::Center);
                num.set_size_request(-1, NUMBER_HEIGHT);

                let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
                content.add_css_class("kalam-pdf-page-thumb-content");
                content.append(&overlay);
                content.append(&num);

                let button = gtk::Button::new();
                button.set_child(Some(&content));
                button.add_css_class("kalam-pdf-page-thumb");
                button.set_hexpand(true);
                button.set_vexpand(true);
                // Half the gap on every side, so adjacent cells
                // meet at the designed 12px and an offset outline
                // has air around the current spread.
                button.set_margin_start(CELL_GAP / 2);
                button.set_margin_end(CELL_GAP / 2);
                button.set_margin_top(CELL_GAP / 2);
                button.set_margin_bottom(CELL_GAP / 2);
                list_item.set_child(Some(&button));

                let cell = PageThumbCell {
                    list_item: list_item.clone(),
                    button,
                    overlay,
                    picture,
                    status,
                    num,
                    bound_page: None,
                    click_handler: None,
                };
                let mut state = setup_state.borrow_mut();
                apply_cell_size(&cell, state.columns, state.aspect_height_for_width);
                state.cells.push(cell);
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
                let page = {
                    let Ok(page_ref) = boxed.try_borrow::<usize>() else {
                        return;
                    };
                    *page_ref
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
                    let texture = state.textures.get(&page).cloned();
                    if texture.is_some() {
                        state.touch_thumbnail(page);
                    }
                    let failed = state.failed.contains(&page);
                    let current = state.current_pages.contains(&page);
                    let total_pages = state.entries.len();

                    let cell = &mut state.cells[cell_idx];
                    if let Some(handler) = cell.click_handler.take() {
                        cell.button.disconnect(handler);
                    }
                    let tx = bind_sender.clone();
                    let handler = cell.button.connect_clicked(move |_| {
                        let _ = tx.send(PdfReaderMsg::JumpToPage(page));
                    });
                    cell.click_handler = Some(handler);
                    cell.bound_page = Some(page);
                    cell.num.set_label(&page.to_string());
                    let description = page_description(page, total_pages, current);
                    cell.button.set_tooltip_text(Some(&description));
                    cell.button.update_property(&[gtk::accessible::Property::Label(
                        &description,
                    )]);
                    paint_cell(cell, texture.as_ref(), failed, current);
                }

                // Recycled into view is the lazy edge.
                let _ = bind_sender.send(PdfReaderMsg::PageThumbnailVisible(page));
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
        view.add_css_class("kalam-pdf-pages-grid");
        view.set_min_columns(columns as u32);
        view.set_max_columns(columns as u32);
        view.set_enable_rubberband(false);
        view.set_single_click_activate(false);
        view.set_hexpand(true);
        view.set_vexpand(true);
        view.set_margin_start(GRID_MARGIN);
        view.set_margin_end(GRID_MARGIN);
        view.set_margin_top(GRID_TOP_MARGIN);
        view.set_margin_bottom(GRID_TOP_MARGIN);

        Self { view, state }
    }

    pub fn widget(&self) -> gtk::GridView {
        self.view.clone()
    }

    pub fn set_columns(&self, columns: usize) -> bool {
        let columns = sanitized_columns(columns);
        {
            let mut state = self.state.borrow_mut();
            if state.columns == columns {
                return false;
            }
            state.columns = columns;
            let aspect = state.aspect_height_for_width;
            for cell in &state.cells {
                apply_cell_size(cell, columns, aspect);
            }
        }
        self.view.set_min_columns(columns as u32);
        self.view.set_max_columns(columns as u32);
        self.view.queue_resize();
        true
    }

    pub fn set_image_settings(&self, settings: PdfImageSettings, paper_rgb: [f32; 3]) {
        let settings = settings.canonical();
        let mut state = self.state.borrow_mut();
        if state.image_settings == settings && state.paper_rgb == paper_rgb {
            return;
        }
        state.image_settings = settings;
        state.paper_rgb = paper_rgb;
        for cell in &state.cells {
            cell.picture.set_image_settings(settings, paper_rgb);
        }
    }

    pub fn set_page_aspect(&self, aspect_height_for_width: f64) {
        let aspect = sanitized_aspect(aspect_height_for_width);
        let mut state = self.state.borrow_mut();
        if (state.aspect_height_for_width - aspect).abs() <= 0.001 {
            return;
        }
        state.aspect_height_for_width = aspect;
        let columns = state.columns;
        for cell in &state.cells {
            apply_cell_size(cell, columns, aspect);
        }
        drop(state);
        self.view.queue_resize();
    }

    /// Point the grid at this document. A standing model is only
    /// replaced when the page count really changes, so an ordinary
    /// panel refresh preserves both scroll and decoded thumbnails.
    pub fn set_document(
        &self,
        total_pages: usize,
        current_pages: HashSet<usize>,
        aspect_height_for_width: f64,
        image_settings: PdfImageSettings,
        paper_rgb: [f32; 3],
    ) {
        self.set_image_settings(image_settings, paper_rgb);
        self.set_page_aspect(aspect_height_for_width);

        let entries: Vec<usize> = if total_pages == 0 {
            Vec::new()
        } else {
            (1..=total_pages).collect()
        };
        let changed = self.state.borrow().entries != entries;
        if !changed {
            self.set_current_pages(current_pages);
            return;
        }

        let objects = {
            let mut state = self.state.borrow_mut();
            let valid: HashSet<usize> = entries.iter().copied().collect();
            state.textures.retain(|page, _| valid.contains(page));
            state.texture_order.retain(|page| valid.contains(page));
            state.failed.retain(|page| valid.contains(page));
            state.current_pages = current_pages
                .into_iter()
                .filter(|page| valid.contains(page))
                .collect();
            state.entries = entries.clone();
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
        self.refresh_current_cells();
    }

    fn refresh_current_cells(&self) {
        let state = self.state.borrow();
        let total_pages = state.entries.len();
        for cell in &state.cells {
            let Some(page) = cell.bound_page else {
                continue;
            };
            let current = state.current_pages.contains(&page);
            if current {
                cell.button.add_css_class("current");
            } else {
                cell.button.remove_css_class("current");
            }
            let description = page_description(page, total_pages, current);
            cell.button.set_tooltip_text(Some(&description));
            cell.button
                .update_property(&[gtk::accessible::Property::Label(&description)]);
        }
    }

    pub fn set_current_pages(&self, current_pages: HashSet<usize>) {
        {
            let mut state = self.state.borrow_mut();
            let current_pages: HashSet<usize> = current_pages
                .into_iter()
                .filter(|page| state.entries.contains(page))
                .collect();
            if state.current_pages == current_pages {
                return;
            }
            state.current_pages = current_pages;
        }
        self.refresh_current_cells();
    }

    pub fn current_position(&self) -> Option<usize> {
        let state = self.state.borrow();
        state
            .entries
            .iter()
            .position(|page| state.current_pages.contains(page))
    }

    pub fn scroll_to_current(&self) {
        if let Some(position) = self.current_position() {
            self.view
                .scroll_to(position as u32, gtk::ListScrollFlags::empty(), None);
        }
    }

    pub fn scroll_to_page(&self, page: usize) {
        let position = self
            .state
            .borrow()
            .entries
            .iter()
            .position(|entry| *entry == page);
        if let Some(position) = position {
            self.view
                .scroll_to(position as u32, gtk::ListScrollFlags::empty(), None);
        }
    }

    pub fn opening_scroll_target(
        &self,
        max_scroll: f64,
        viewport_height: f64,
    ) -> Option<f64> {
        let position = self.current_position()?;
        let state = self.state.borrow();
        Some(opening_scroll_target_for_position(
            position,
            state.columns,
            state.aspect_height_for_width,
            max_scroll,
            viewport_height,
        ))
    }

    pub fn first_visible_page(&self, scroll_value: f64) -> Option<usize> {
        let state = self.state.borrow();
        let position = first_visible_position(
            state.entries.len(),
            state.columns,
            state.aspect_height_for_width,
            scroll_value,
        )?;
        state.entries.get(position).copied()
    }

    pub fn heading_anchor_page(&self, scroll_value: f64) -> Option<usize> {
        let state = self.state.borrow();
        let position = heading_anchor_position(
            state.entries.len(),
            state.columns,
            state.aspect_height_for_width,
            scroll_value,
        )?;
        state.entries.get(position).copied()
    }

    pub fn bound_pages(&self) -> Vec<usize> {
        self.state
            .borrow()
            .cells
            .iter()
            .filter_map(|cell| cell.bound_page)
            .collect()
    }

    pub fn has_thumbnail_state(&self, page: usize) -> bool {
        let state = self.state.borrow();
        state.textures.contains_key(&page) || state.failed.contains(&page)
    }

    pub fn update_thumbnail(&self, page: usize, texture: Option<gdk::Texture>) {
        let mut state = self.state.borrow_mut();
        if !state.entries.contains(&page) {
            return;
        }
        let current = state.current_pages.contains(&page);
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
        let current_pages = state.current_pages.clone();
        for cell in &state.cells {
            if let Some(page) = cell.bound_page {
                paint_cell(cell, None, false, current_pages.contains(&page));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cell_sizes_keep_the_whole_page_and_the_number_strip() {
        assert_eq!(thumbnail_cell_size(3, 595.0 / 420.0), (92, 152));
        assert_eq!(thumbnail_cell_size(2, 595.0 / 420.0), (144, 226));
        // Unsupported column counts fall back to the board's three.
        assert_eq!(thumbnail_cell_size(9, 1.0), (92, 114));
        // Invalid aspects fall back to the A-series proportion.
        assert_eq!(thumbnail_cell_size(3, f64::NAN).0, 92);
    }

    #[test]
    fn first_visible_and_opening_scroll_follow_the_row_pitch() {
        let aspect = 595.0 / 420.0;
        let pitch = thumbnail_row_pitch(3, aspect);
        assert_eq!(first_visible_position(24, 3, aspect, 0.0), Some(0));
        assert_eq!(
            first_visible_position(24, 3, aspect, GRID_TOP_MARGIN as f64 + pitch),
            Some(3)
        );
        assert_eq!(first_visible_position(24, 3, aspect, 100_000.0), Some(23));
        assert_eq!(first_visible_position(0, 3, aspect, 0.0), None);

        // A boundary row is named by its middle page in three
        // columns and by its right-hand page in two.
        assert_eq!(heading_anchor_position(24, 3, aspect, 0.0), Some(1));
        assert_eq!(heading_anchor_position(24, 2, aspect, 0.0), Some(1));
        assert_eq!(
            heading_anchor_position(24, 3, aspect, GRID_TOP_MARGIN as f64 + pitch),
            Some(4)
        );
        assert_eq!(heading_anchor_position(24, 3, aspect, 100_000.0), Some(23));

        assert_eq!(
            scroll_target_for_position(0, 3, aspect),
            GRID_TOP_MARGIN as f64
        );
        assert_eq!(
            scroll_target_for_position(6, 3, aspect),
            GRID_TOP_MARGIN as f64 + pitch
        );
    }

    #[test]
    fn thumbnail_scale_measures_displayed_width_after_a_quarter_turn() {
        assert!((thumbnail_render_scale(420.0, 595.0, 0, 256) - 256.0 / 420.0).abs() < 0.0001);
        assert!((thumbnail_render_scale(420.0, 595.0, 90, 256) - 256.0 / 595.0).abs() < 0.0001);
        assert!((thumbnail_render_scale(420.0, 595.0, -90, 256) - 256.0 / 595.0).abs() < 0.0001);
        assert_eq!(thumbnail_render_scale(0.0, 595.0, 0, 256), 0.5);
        assert_eq!(thumbnail_render_scale(420.0, 595.0, 0, 0), 0.5);
    }
}
