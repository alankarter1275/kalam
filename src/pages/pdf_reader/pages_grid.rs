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
use gtk::glib::types::StaticType;
use gtk::glib::value::ToValue;
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

/// One cell of the grid as the reader describes it. `key` is the
/// page's stable identity (an inserted page's future one, while
/// an edit is staged): textures, failures and every mark are
/// keyed by it, so a staged reorder carries a page's thumbnail
/// with it instead of leaving it behind on a page number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfPagesGridEntry {
    pub key: String,
    /// The number strip under the thumbnail.
    pub label: String,
    /// The tooltip and accessible name.
    pub description: String,
    /// Where a plain (non-edit) click lands.
    pub jump_page: usize,
}

/// The marks over the entries: which keys are in the current
/// spread, which are selected for editing and which carry a
/// staged rotation.
#[derive(Debug, Clone, Default)]
pub struct PdfPagesGridMarks {
    pub current: HashSet<String>,
    pub selected: HashSet<String>,
    pub rotated: HashSet<String>,
    pub edit_mode: bool,
}

struct PageThumbCell {
    list_item: gtk::ListItem,
    button: gtk::Button,
    overlay: gtk::Overlay,
    picture: PdfPagePicture,
    status: gtk::Label,
    badge: gtk::Label,
    rotated_badge: gtk::Box,
    drop_bar: gtk::Box,
    num: gtk::Label,
    bound_key: Option<String>,
    click_modifiers: gdk::ModifierType,
    click_handler: Option<gtk::glib::SignalHandlerId>,
}

struct PagesGridState {
    entries: Vec<PdfPagesGridEntry>,
    textures: HashMap<String, gdk::Texture>,
    texture_order: VecDeque<String>,
    failed: HashSet<String>,
    current_keys: HashSet<String>,
    selected_keys: HashSet<String>,
    rotated_keys: HashSet<String>,
    dragging_keys: HashSet<String>,
    drop_index: Option<usize>,
    edit_mode: bool,
    columns: usize,
    aspect_height_for_width: f64,
    image_settings: PdfImageSettings,
    paper_rgb: [f32; 3],
    cells: Vec<PageThumbCell>,
}

impl PagesGridState {
    fn touch_thumbnail(&mut self, key: &str) {
        if self.textures.contains_key(key) {
            self.texture_order.retain(|cached| cached != key);
            self.texture_order.push_back(key.to_string());
        }
    }

    fn insert_thumbnail(&mut self, key: &str, texture: gdk::Texture) {
        self.failed.remove(key);
        self.textures.insert(key.to_string(), texture);
        self.touch_thumbnail(key);
        while self.textures.len() > MAX_CACHED_THUMBNAILS {
            let Some(oldest) = self.texture_order.pop_front() else {
                break;
            };
            if oldest != key {
                self.textures.remove(&oldest);
            }
        }
    }

    fn remove_thumbnail(&mut self, key: &str) {
        self.textures.remove(key);
        self.texture_order.retain(|cached| cached != key);
    }

    fn entry(&self, key: &str) -> Option<PdfPagesGridEntry> {
        self.entries.iter().find(|entry| entry.key == key).cloned()
    }

    fn cell_index(&self, list_item: &gtk::ListItem) -> Option<usize> {
        self.cells
            .iter()
            .position(|cell| cell.list_item == *list_item)
    }
}

struct PageCellPaint {
    texture: Option<gdk::Texture>,
    failed: bool,
    current: bool,
    selected: bool,
    rotated: bool,
    dragging: bool,
    drag_badge: Option<String>,
    drop_side: Option<gtk::Align>,
}

fn paint_cell(cell: &PageThumbCell, paint: &PageCellPaint, description: &str) {
    cell.picture.set_texture(paint.texture.as_ref());
    cell.status
        .set_label(if paint.failed { "Failed to load" } else { "" });
    if paint.failed {
        cell.button.add_css_class("failed");
    } else {
        cell.button.remove_css_class("failed");
    }
    if paint.current {
        cell.button.add_css_class("current");
    } else {
        cell.button.remove_css_class("current");
    }
    if paint.selected {
        cell.button.add_css_class("selected");
    } else {
        cell.button.remove_css_class("selected");
    }
    cell.badge.set_visible(paint.selected || paint.drag_badge.is_some());
    cell.badge.set_label(paint.drag_badge.as_deref().unwrap_or("✓"));
    cell.rotated_badge.set_visible(paint.rotated && !paint.dragging);
    cell.button.set_opacity(if paint.dragging { 0.35 } else { 1.0 });
    if let Some(side) = paint.drop_side {
        cell.drop_bar.set_halign(side);
        cell.drop_bar.set_visible(true);
    } else {
        cell.drop_bar.set_visible(false);
    }
    cell.button.set_tooltip_text(Some(description));
    cell.button
        .update_property(&[gtk::accessible::Property::Label(description)]);
}

/// Repaint one bound cell from the marks in force.
fn paint_bound_cell(state: &PagesGridState, cell: &PageThumbCell) {
    let Some(key) = cell.bound_key.as_deref() else {
        return;
    };
    let Some(position) = state.entries.iter().position(|entry| entry.key == key) else {
        return;
    };
    let entry = &state.entries[position];
    let drop_side = match state.drop_index {
        Some(index) if index == position => Some(gtk::Align::Start),
        Some(index) if index == position + 1 && position + 1 == state.entries.len() => {
            Some(gtk::Align::End)
        }
        _ => None,
    };
    let dragging = state.dragging_keys.contains(key);
    let drag_badge = if dragging && state.dragging_keys.len() > 1 {
        Some(state.dragging_keys.len().to_string())
    } else {
        None
    };
    paint_cell(
        cell,
        &PageCellPaint {
            texture: state.textures.get(key).cloned(),
            failed: state.failed.contains(key),
            current: state.current_keys.contains(key),
            selected: state.selected_keys.contains(key),
            rotated: state.rotated_keys.contains(key),
            dragging,
            drag_badge,
            drop_side,
        },
        &entry.description,
    );
}

/// Repaint every realised cell from the marks in force. Cells
/// are recycled, so this — not the bind — is what moves a tick,
/// a ring or the drop bar while the grid stands still.
fn refresh_cells(state: &PagesGridState) {
    for cell in &state.cells {
        paint_bound_cell(state, cell);
    }
}

fn clear_cell(cell: &PageThumbCell) {
    cell.picture.set_texture(None);
    cell.status.set_label("");
    cell.num.set_label("");
    cell.badge.set_visible(false);
    cell.rotated_badge.set_visible(false);
    cell.drop_bar.set_visible(false);
    cell.button.set_opacity(1.0);
    cell.button.remove_css_class("failed");
    cell.button.remove_css_class("current");
    cell.button.remove_css_class("selected");
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
            current_keys: HashSet::new(),
            selected_keys: HashSet::new(),
            rotated_keys: HashSet::new(),
            dragging_keys: HashSet::new(),
            drop_index: None,
            edit_mode: false,
            columns,
            aspect_height_for_width: std::f64::consts::SQRT_2,
            image_settings: image_settings.canonical(),
            paper_rgb,
            cells: Vec::new(),
        }));
        let factory = gtk::SignalListItemFactory::new();

        {
            let setup_state = state.clone();
            let setup_sender = sender.clone();
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

                // The edit-mode tick (decision 68: a tick badge,
                // never a checkbox). While a multi-page drag is
                // underway it shows the count instead.
                let badge = gtk::Label::new(Some("✓"));
                badge.add_css_class("kalam-pdf-page-thumb-badge");
                badge.set_halign(gtk::Align::Start);
                badge.set_valign(gtk::Align::Start);
                badge.set_margin_start(5);
                badge.set_margin_top(5);
                badge.set_size_request(20, 20);
                badge.set_can_target(false);
                badge.set_visible(false);
                overlay.add_overlay(&badge);

                // The staged rotation's mark, on a dark disc so
                // it reads over any page.
                let rotated_icon = gtk::Image::from_icon_name("object-rotate-right-symbolic");
                rotated_icon.set_pixel_size(11);
                rotated_icon.set_hexpand(true);
                rotated_icon.set_vexpand(true);
                rotated_icon.set_halign(gtk::Align::Center);
                rotated_icon.set_valign(gtk::Align::Center);
                let rotated_badge = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                rotated_badge.add_css_class("kalam-pdf-page-thumb-rotated");
                rotated_badge.set_halign(gtk::Align::End);
                rotated_badge.set_valign(gtk::Align::Start);
                rotated_badge.set_margin_end(5);
                rotated_badge.set_margin_top(5);
                rotated_badge.set_size_request(20, 20);
                rotated_badge.set_can_target(false);
                rotated_badge.append(&rotated_icon);
                rotated_badge.set_visible(false);
                overlay.add_overlay(&rotated_badge);

                // The drop slot: a 3px accent bar standing at an
                // insertion boundary while a drag hovers.
                let drop_bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                drop_bar.add_css_class("kalam-pdf-page-thumb-dropbar");
                drop_bar.set_halign(gtk::Align::Start);
                drop_bar.set_valign(gtk::Align::Fill);
                drop_bar.set_size_request(3, -1);
                drop_bar.set_can_target(false);
                drop_bar.set_visible(false);

                let num = gtk::Label::new(None);
                num.add_css_class("kalam-pdf-page-thumb-num");
                num.set_halign(gtk::Align::Center);
                num.set_valign(gtk::Align::Center);
                num.set_size_request(-1, NUMBER_HEIGHT);

                let content = gtk::Box::new(gtk::Orientation::Vertical, 0);
                content.add_css_class("kalam-pdf-page-thumb-content");
                content.append(&overlay);
                content.append(&num);

                // The bar belongs to the whole cell, number strip
                // included, so it stands over the assembled card
                // rather than only over the page image.
                let cell_overlay = gtk::Overlay::new();
                cell_overlay.set_hexpand(true);
                cell_overlay.set_vexpand(true);
                cell_overlay.set_child(Some(&content));
                cell_overlay.add_overlay(&drop_bar);

                let button = gtk::Button::new();
                button.set_child(Some(&cell_overlay));
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

                // The modifiers of the press behind the click:
                // a plain click selects, Ctrl toggles and Shift
                // takes a run, and `connect_clicked` alone cannot
                // say which it was.
                let press = gtk::GestureClick::new();
                press.set_button(1);
                {
                    let press_state = setup_state.clone();
                    let press_item = list_item.clone();
                    press.connect_pressed(move |gesture, _, _, _| {
                        let modifiers = gesture.current_event_state();
                        let mut state = press_state.borrow_mut();
                        if let Some(cell_idx) = state.cell_index(&press_item) {
                            state.cells[cell_idx].click_modifiers = modifiers;
                        }
                    });
                }
                button.add_controller(press);

                // A page drags its selection with it. The
                // payload is only the grabbed key — which page
                // that makes the moving group is the model's
                // decision, announced by PageEditDragStarted.
                let drag_source = gtk::DragSource::new();
                drag_source.set_actions(gdk::DragAction::MOVE);
                {
                    let prepare_state = setup_state.clone();
                    let prepare_item = list_item.clone();
                    drag_source.connect_prepare(move |_, _, _| {
                        let mut state = prepare_state.borrow_mut();
                        if !state.edit_mode {
                            return None;
                        }
                        let cell_idx = state.cell_index(&prepare_item)?;
                        let key = state.cells[cell_idx].bound_key.clone()?;
                        if !state.selected_keys.contains(&key) {
                            state.selected_keys.clear();
                            state.selected_keys.insert(key.clone());
                            refresh_cells(&state);
                        }
                        Some(gdk::ContentProvider::for_value(&key.to_value()))
                    });
                }
                {
                    let begin_state = setup_state.clone();
                    let begin_item = list_item.clone();
                    let begin_sender = setup_sender.clone();
                    let begin_button = button.clone();
                    drag_source.connect_drag_begin(move |source, _| {
                        let key = {
                            let mut state = begin_state.borrow_mut();
                            let Some(cell_idx) = state.cell_index(&begin_item) else {
                                return;
                            };
                            let Some(key) = state.cells[cell_idx].bound_key.clone() else {
                                return;
                            };
                            if state.selected_keys.is_empty() {
                                state.selected_keys.insert(key.clone());
                            }
                            state.dragging_keys.clone_from(&state.selected_keys);
                            refresh_cells(&state);
                            key
                        };
                        let paintable = gtk::WidgetPaintable::new(Some(&begin_button));
                        source.set_icon(
                            Some(&paintable),
                            begin_button.width() / 2,
                            begin_button.height() / 2,
                        );
                        let _ = begin_sender.send(PdfReaderMsg::PageEditDragStarted(key));
                    });
                }
                {
                    let end_state = setup_state.clone();
                    drag_source.connect_drag_end(move |_, _, _| {
                        let mut state = end_state.borrow_mut();
                        state.dragging_keys.clear();
                        state.drop_index = None;
                        refresh_cells(&state);
                    });
                }
                {
                    let cancel_state = setup_state.clone();
                    drag_source.connect_drag_cancel(move |_, _, _| {
                        let mut state = cancel_state.borrow_mut();
                        state.dragging_keys.clear();
                        state.drop_index = None;
                        refresh_cells(&state);
                        false
                    });
                }
                button.add_controller(drag_source);

                let drop_target = gtk::DropTarget::new(
                    String::static_type(),
                    gdk::DragAction::MOVE,
                );
                {
                    let hover_state = setup_state.clone();
                    let hover_item = list_item.clone();
                    let hover_button = button.clone();
                    let hover: Rc<dyn Fn(f64) -> gdk::DragAction> =
                        Rc::new(move |x: f64| -> gdk::DragAction {
                        let mut state = hover_state.borrow_mut();
                        if !state.edit_mode {
                            return gdk::DragAction::empty();
                        }
                        let Some(cell_idx) = state.cell_index(&hover_item) else {
                            return gdk::DragAction::empty();
                        };
                        let Some(key) = state.cells[cell_idx].bound_key.clone() else {
                            return gdk::DragAction::empty();
                        };
                        let Some(position) =
                            state.entries.iter().position(|entry| entry.key == key)
                        else {
                            return gdk::DragAction::empty();
                        };
                        let slot = if x >= f64::from(hover_button.width().max(1)) / 2.0 {
                            position + 1
                        } else {
                            position
                        };
                        state.drop_index = Some(slot);
                        refresh_cells(&state);
                        gdk::DragAction::MOVE
                        });
                    let enter = hover.clone();
                    drop_target.connect_enter(move |_, x, _| enter(x));
                    drop_target.connect_motion(move |_, x, _| hover(x));
                }
                {
                    let leave_state = setup_state.clone();
                    drop_target.connect_leave(move |_| {
                        let mut state = leave_state.borrow_mut();
                        state.drop_index = None;
                        refresh_cells(&state);
                    });
                }
                {
                    let drop_state = setup_state.clone();
                    let drop_item = list_item.clone();
                    let drop_sender = setup_sender.clone();
                    let drop_button = button.clone();
                    drop_target.connect_drop(move |_, value, x, _| {
                        let Ok(key) = value.get::<String>() else {
                            return false;
                        };
                        let mut state = drop_state.borrow_mut();
                        if !state.edit_mode {
                            return false;
                        }
                        let Some(cell_idx) = state.cell_index(&drop_item) else {
                            return false;
                        };
                        let Some(target_key) = state.cells[cell_idx].bound_key.clone()
                        else {
                            return false;
                        };
                        let Some(position) = state
                            .entries
                            .iter()
                            .position(|entry| entry.key == target_key)
                        else {
                            return false;
                        };
                        let target_index =
                            if x >= f64::from(drop_button.width().max(1)) / 2.0 {
                                position + 1
                            } else {
                                position
                            };
                        state.dragging_keys.clear();
                        state.drop_index = None;
                        refresh_cells(&state);
                        drop(state);
                        let _ = drop_sender.send(PdfReaderMsg::PageEditMove {
                            key,
                            target_index,
                        });
                        true
                    });
                }
                button.add_controller(drop_target);

                let cell = PageThumbCell {
                    list_item: list_item.clone(),
                    button,
                    overlay,
                    picture,
                    status,
                    badge,
                    rotated_badge,
                    drop_bar,
                    num,
                    bound_key: None,
                    click_modifiers: gdk::ModifierType::empty(),
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
                let entry = {
                    let Ok(entry_ref) = boxed.try_borrow::<PdfPagesGridEntry>() else {
                        return;
                    };
                    entry_ref.clone()
                };
                let key = entry.key.clone();

                {
                    let mut state = bind_state.borrow_mut();
                    let Some(cell_idx) = state.cell_index(list_item) else {
                        return;
                    };
                    if state.textures.contains_key(&key) {
                        state.touch_thumbnail(&key);
                    }

                    let cell = &mut state.cells[cell_idx];
                    if let Some(handler) = cell.click_handler.take() {
                        cell.button.disconnect(handler);
                    }
                    let tx = bind_sender.clone();
                    let click_state = bind_state.clone();
                    let click_item = list_item.clone();
                    let click_entry = entry.clone();
                    let handler = cell.button.connect_clicked(move |_| {
                        // In edit mode the click selects, with
                        // whatever modifiers its press carried;
                        // reading, it goes to the page.
                        let modifiers = {
                            let state = click_state.borrow();
                            state
                                .cell_index(&click_item)
                                .map(|cell_idx| state.cells[cell_idx].click_modifiers)
                                .unwrap_or_else(gdk::ModifierType::empty)
                        };
                        let editing = click_state.borrow().edit_mode;
                        if editing {
                            let _ = tx.send(PdfReaderMsg::PageEditThumbnailClicked {
                                key: click_entry.key.clone(),
                                control: modifiers.contains(gdk::ModifierType::CONTROL_MASK),
                                shift: modifiers.contains(gdk::ModifierType::SHIFT_MASK),
                            });
                        } else {
                            let _ = tx.send(PdfReaderMsg::JumpToPage(click_entry.jump_page));
                        }
                    });
                    cell.click_handler = Some(handler);
                    cell.bound_key = Some(entry.key.clone());
                    cell.click_modifiers = gdk::ModifierType::empty();
                    cell.num.set_label(&entry.label);
                }
                {
                    let state = bind_state.borrow();
                    if let Some(cell_idx) = state.cell_index(list_item) {
                        paint_bound_cell(&state, &state.cells[cell_idx]);
                    }
                }

                // Recycled into view is the lazy edge.
                let _ = bind_sender.send(PdfReaderMsg::PageThumbnailVisible(key));
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
                cell.bound_key = None;
                cell.click_modifiers = gdk::ModifierType::empty();
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

    /// Point the grid at this set of entries. A standing model
    /// is only replaced when the entries really change, so an
    /// ordinary panel refresh preserves both scroll and decoded
    /// thumbnails; when they do change (a staged reorder, a new
    /// book), thumbnails follow their key and only the marks of
    /// still-present pages survive.
    pub fn set_entries(
        &self,
        entries: Vec<PdfPagesGridEntry>,
        marks: PdfPagesGridMarks,
        aspect_height_for_width: f64,
        image_settings: PdfImageSettings,
        paper_rgb: [f32; 3],
    ) {
        self.set_image_settings(image_settings, paper_rgb);
        self.set_page_aspect(aspect_height_for_width);

        let valid: HashSet<String> = entries.iter().map(|entry| entry.key.clone()).collect();
        let current_keys: HashSet<String> = marks
            .current
            .into_iter()
            .filter(|key| valid.contains(key))
            .collect();
        let selected_keys: HashSet<String> = if marks.edit_mode {
            marks
                .selected
                .into_iter()
                .filter(|key| valid.contains(key))
                .collect()
        } else {
            HashSet::new()
        };
        let rotated_keys: HashSet<String> = if marks.edit_mode {
            marks
                .rotated
                .into_iter()
                .filter(|key| valid.contains(key))
                .collect()
        } else {
            HashSet::new()
        };

        enum EntriesChange {
            Standing,
            Replaced(Vec<BoxedAnyObject>),
        }
        let change = {
            let mut state = self.state.borrow_mut();
            // A page whose staged rotation flipped needs a fresh
            // thumbnail at its new angle; the old quarter-turn
            // picture must not linger under the new mark.
            let re_rotated: Vec<String> = state
                .rotated_keys
                .symmetric_difference(&rotated_keys)
                .cloned()
                .collect();
            for key in &re_rotated {
                state.remove_thumbnail(key);
                state.failed.remove(key);
            }
            state.current_keys = current_keys;
            state.selected_keys = selected_keys;
            state.rotated_keys = rotated_keys;
            state.edit_mode = marks.edit_mode;
            if !marks.edit_mode {
                state.dragging_keys.clear();
                state.drop_index = None;
            }
            if state.entries == entries {
                refresh_cells(&state);
                EntriesChange::Standing
            } else {
                state.textures.retain(|key, _| valid.contains(key));
                state.texture_order.retain(|key| valid.contains(key));
                state.failed.retain(|key| valid.contains(key));
                state.dragging_keys.retain(|key| valid.contains(key));
                if state
                    .drop_index
                    .is_some_and(|index| index > entries.len())
                {
                    state.drop_index = None;
                }
                state.entries = entries;
                EntriesChange::Replaced(
                    state
                        .entries
                        .iter()
                        .cloned()
                        .map(BoxedAnyObject::new)
                        .collect(),
                )
            }
        };

        if let EntriesChange::Replaced(objects) = change {
            let store = gtk::gio::ListStore::new::<BoxedAnyObject>();
            for object in &objects {
                store.append(object);
            }
            let selection = gtk::NoSelection::new(Some(store));
            self.view.set_model(Some(&selection));
            let state = self.state.borrow();
            refresh_cells(&state);
        }
    }

    pub fn set_current_keys(&self, current_keys: HashSet<String>) {
        let mut state = self.state.borrow_mut();
        let current_keys: HashSet<String> = current_keys
            .into_iter()
            .filter(|key| state.entries.iter().any(|entry| &entry.key == key))
            .collect();
        if state.current_keys == current_keys {
            return;
        }
        state.current_keys = current_keys;
        refresh_cells(&state);
    }

    /// Stage the drag marks from outside the controllers (the
    /// screenshot desk poses a drag this way).
    pub fn set_drag_state(&self, dragging_keys: HashSet<String>, drop_index: Option<usize>) {
        let mut state = self.state.borrow_mut();
        state.dragging_keys = dragging_keys
            .into_iter()
            .filter(|key| state.entries.iter().any(|entry| &entry.key == key))
            .collect();
        state.drop_index = drop_index.filter(|index| *index <= state.entries.len());
        refresh_cells(&state);
    }

    pub fn current_position(&self) -> Option<usize> {
        let state = self.state.borrow();
        state
            .entries
            .iter()
            .position(|entry| state.current_keys.contains(&entry.key))
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
            .position(|entry| entry.jump_page == page);
        if let Some(position) = position {
            self.view
                .scroll_to(position as u32, gtk::ListScrollFlags::empty(), None);
        }
    }

    pub fn scroll_to_key(&self, key: &str) {
        let position = self
            .state
            .borrow()
            .entries
            .iter()
            .position(|entry| entry.key == key);
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
        state.entries.get(position).map(|entry| entry.jump_page)
    }

    pub fn heading_anchor_page(&self, scroll_value: f64) -> Option<usize> {
        let state = self.state.borrow();
        let position = heading_anchor_position(
            state.entries.len(),
            state.columns,
            state.aspect_height_for_width,
            scroll_value,
        )?;
        state.entries.get(position).map(|entry| entry.jump_page)
    }

    pub fn bound_keys(&self) -> Vec<String> {
        self.state
            .borrow()
            .cells
            .iter()
            .filter_map(|cell| cell.bound_key.clone())
            .collect()
    }

    pub fn has_thumbnail_state(&self, key: &str) -> bool {
        let state = self.state.borrow();
        state.textures.contains_key(key) || state.failed.contains(key)
    }

    /// Forget one key's raster or failure without disturbing the
    /// rest of the cache — a staged quarter turn needs exactly
    /// that page rendered again at its new angle.
    pub fn clear_thumbnail(&self, key: &str) {
        let mut state = self.state.borrow_mut();
        state.remove_thumbnail(key);
        state.failed.remove(key);
        for cell in &state.cells {
            if cell.bound_key.as_deref() == Some(key) {
                paint_bound_cell(&state, cell);
            }
        }
    }

    pub fn update_thumbnail(&self, key: &str, texture: Option<gdk::Texture>) {
        let mut state = self.state.borrow_mut();
        if !state.entries.iter().any(|entry| entry.key == key) {
            return;
        }
        match texture {
            Some(texture) => state.insert_thumbnail(key, texture),
            None => {
                state.remove_thumbnail(key);
                state.failed.insert(key.to_string());
            }
        }
        for cell in &state.cells {
            if cell.bound_key.as_deref() == Some(key) {
                paint_bound_cell(&state, cell);
            }
        }
    }

    pub fn clear_thumbnails(&self) {
        let mut state = self.state.borrow_mut();
        state.textures.clear();
        state.texture_order.clear();
        state.failed.clear();
        refresh_cells(&state);
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
