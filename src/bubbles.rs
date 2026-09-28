//! In-App Draggable Bubbles and Zen-Style Side-by-Side Split Reader.
//!
//! Provides Android-style floating chat-head bubbles for multiple open books
//! and an inset floating reading window with side-by-side split screen capability.

use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use gtk::prelude::*;
use relm4::prelude::*;

use crate::app::{AppModel, AppMsg};
use crate::db::Catalog;
use crate::models::BookFormat;

/// A lightweight representation of a book held in a bubble.
/// Rule: "A bubble holds no book. It is a bookmark, not a reader."
#[derive(Clone, Debug, PartialEq)]
pub struct BubbleItem {
    pub book_id: i64,
    pub title: String,
    pub author: String,
    pub format: BookFormat,
    pub progress: u8,
    pub cover_path: Option<PathBuf>,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ActivePane {
    Left,
    Right,
}

/// Active reader instance held only while being read.
pub enum BubbleReaderInstance {
    Epub {
        book_id: i64,
        ctrl: relm4::Controller<crate::pages::reader::ReaderModel>,
    },
    Pdf {
        book_id: i64,
        ctrl: relm4::Controller<crate::pages::pdf_reader::PdfReaderModel>,
    },
    Comic {
        book_id: i64,
        ctrl: relm4::Controller<crate::pages::comics_reader::ComicsReaderModel>,
    },
}

impl BubbleReaderInstance {
    pub fn book_id(&self) -> i64 {
        match self {
            BubbleReaderInstance::Epub { book_id, .. } => *book_id,
            BubbleReaderInstance::Pdf { book_id, .. } => *book_id,
            BubbleReaderInstance::Comic { book_id, .. } => *book_id,
        }
    }

    pub fn widget(&self) -> &gtk::Widget {
        match self {
            BubbleReaderInstance::Epub { ctrl, .. } => ctrl.widget().upcast_ref(),
            BubbleReaderInstance::Pdf { ctrl, .. } => ctrl.widget().upcast_ref(),
            BubbleReaderInstance::Comic { ctrl, .. } => ctrl.widget().upcast_ref(),
        }
    }
}

pub struct BubbleManager {
    catalog: Arc<Catalog>,
    sender: ComponentSender<AppModel>,
    items: Vec<BubbleItem>,
    active_book_id: Option<i64>,
    previous_active_book_id: Option<i64>,
    split_book_id: Option<i64>,
    window_open: bool,
    active_pane: ActivePane,
    _pos: Rc<RefCell<(f64, f64)>>,

    // Root UI container overlay mounted in kalam-main content_overlay
    root_overlay: gtk::Overlay,
    _fixed_layer: gtk::Fixed,
    minimized_stack: gtk::Box,

    // Window widgets
    window_scrim: gtk::Box,
    window_container: gtk::Box,
    top_bubbles_box: gtk::Box,
    reader_content_box: gtk::Box,
    drop_zone_indicator: gtk::Box,
    drop_zone_label: gtk::Label,

    // Active reader controllers
    primary_reader: Option<BubbleReaderInstance>,
    split_reader: Option<BubbleReaderInstance>,
}

impl BubbleManager {
    pub fn new(catalog: Arc<Catalog>, sender: ComponentSender<AppModel>) -> Self {
        let root_overlay = gtk::Overlay::new();
        root_overlay.set_hexpand(true);
        root_overlay.set_vexpand(true);
        root_overlay.set_halign(gtk::Align::Fill);
        root_overlay.set_valign(gtk::Align::Fill);
        root_overlay.set_can_target(true);

        // Base stage ensures root_overlay has full allocated size of content_overlay
        let base_stage = gtk::Box::new(gtk::Orientation::Vertical, 0);
        base_stage.set_hexpand(true);
        base_stage.set_vexpand(true);
        base_stage.set_can_target(false);
        root_overlay.set_child(Some(&base_stage));

        // Fixed layer for freely draggable minimized bubbles with zero edge snapping
        let fixed_layer = gtk::Fixed::new();
        fixed_layer.set_hexpand(true);
        fixed_layer.set_vexpand(true);
        fixed_layer.set_can_target(false);

        let minimized_stack = gtk::Box::new(gtk::Orientation::Vertical, 0);
        minimized_stack.add_css_class("k-bubble-minimized-host");
        minimized_stack.set_size_request(60, 60);
        minimized_stack.set_visible(false);
        minimized_stack.set_can_target(true);

        fixed_layer.put(&minimized_stack, 720.0, 480.0);
        root_overlay.add_overlay(&fixed_layer);

        // Window scrim (dimmed backdrop behind floating window, clicking outside minimizes)
        let window_scrim = gtk::Box::new(gtk::Orientation::Vertical, 0);
        window_scrim.add_css_class("k-bubble-window-scrim");
        window_scrim.set_hexpand(true);
        window_scrim.set_vexpand(true);
        window_scrim.set_halign(gtk::Align::Fill);
        window_scrim.set_valign(gtk::Align::Fill);
        window_scrim.set_visible(false);
        window_scrim.set_can_target(true);

        let s_scrim = sender.clone();
        let scrim_click = gtk::GestureClick::new();
        scrim_click.connect_released(move |_, _, _, _| {
            s_scrim.input(AppMsg::BubbleMinimizeWindow);
        });
        window_scrim.add_controller(scrim_click);
        root_overlay.add_overlay(&window_scrim);

        // Floating Window Container (occupies ~80% content width/height centered in content area)
        let window_container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        window_container.set_hexpand(true);
        window_container.set_vexpand(true);
        window_container.set_halign(gtk::Align::Fill);
        window_container.set_valign(gtk::Align::Fill);
        window_container.set_margin_start(54);
        window_container.set_margin_end(54);
        window_container.set_margin_top(28);
        window_container.set_margin_bottom(28);
        window_container.set_visible(false);
        window_container.set_can_target(true);

        // Top Floating Strip (Free-floating row, ZERO background bar!)
        let top_floating_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        top_floating_row.add_css_class("k-bubble-floating-strip");
        top_floating_row.set_halign(gtk::Align::Start);
        top_floating_row.set_valign(gtk::Align::Center);
        top_floating_row.set_margin_bottom(8);

        let top_bubbles_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        top_bubbles_box.set_halign(gtk::Align::Start);
        top_floating_row.append(&top_bubbles_box);
        window_container.append(&top_floating_row);

        // Reading Window Card
        let window_card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        window_card.add_css_class("k-bubble-window-card");
        window_card.set_hexpand(true);
        window_card.set_vexpand(true);

        // Reader Area Overlay (hosts reader content and translucent drag-to-split drop indicator)
        let reader_overlay = gtk::Overlay::new();
        reader_overlay.set_hexpand(true);
        reader_overlay.set_vexpand(true);

        let reader_content_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        reader_content_box.set_hexpand(true);
        reader_content_box.set_vexpand(true);
        reader_overlay.set_child(Some(&reader_content_box));

        // Translucent Zen-style drop indicator
        let drop_zone_indicator = gtk::Box::new(gtk::Orientation::Vertical, 0);
        drop_zone_indicator.add_css_class("k-bubble-drop-zone");
        drop_zone_indicator.set_visible(false);
        drop_zone_indicator.set_can_target(false);
        drop_zone_indicator.set_valign(gtk::Align::Fill);
        drop_zone_indicator.set_halign(gtk::Align::End);
        drop_zone_indicator.set_hexpand(false);
        drop_zone_indicator.set_vexpand(true);

        let drop_zone_label = gtk::Label::new(Some("Drop to split side-by-side"));
        drop_zone_label.add_css_class("k-bubble-drop-label");
        drop_zone_label.set_justify(gtk::Justification::Center);
        drop_zone_label.set_valign(gtk::Align::Center);
        drop_zone_label.set_vexpand(true);
        drop_zone_indicator.append(&drop_zone_label);

        reader_overlay.add_overlay(&drop_zone_indicator);
        window_card.append(&reader_overlay);
        window_container.append(&window_card);

        root_overlay.add_overlay(&window_container);

        let pos = Rc::new(RefCell::new((720.0, 480.0)));

        // Setup Draggable gesture with ZERO edge snapping
        let drag = gtk::GestureDrag::new();
        let start_coord = Rc::new(RefCell::new((0.0, 0.0)));
        let start_coord_begin = start_coord.clone();
        let start_coord_update = start_coord.clone();
        let start_coord_end = start_coord;
        let pos_drag = pos.clone();
        let pos_end = pos.clone();
        let fixed_drag = fixed_layer.clone();
        let stack_drag = minimized_stack.clone();
        let s_click = sender.clone();

        drag.connect_drag_begin(move |_, _x, _y| {
            let current = *pos_drag.borrow();
            start_coord_begin.replace((current.0, current.1));
        });

        drag.connect_drag_update(move |_, offset_x, offset_y| {
            let (sx, sy) = *start_coord_update.borrow();
            let new_x = (sx + offset_x).max(10.0f64);
            let new_y = (sy + offset_y).max(10.0f64);
            fixed_drag.move_(&stack_drag, new_x, new_y);
        });

        drag.connect_drag_end(move |_, offset_x, offset_y| {
            let (sx, sy) = *start_coord_end.borrow();
            let dist = offset_x.hypot(offset_y);
            if dist < 6.0f64 {
                // Short movement counts as a click: expand reading window
                s_click.input(AppMsg::BubbleSelect { book_id: 0 });
            } else {
                // Free drop: stays right where dropped, ZERO snapping!
                let final_x = (sx + offset_x).max(10.0f64);
                let final_y = (sy + offset_y).max(10.0f64);
                pos_end.replace((final_x, final_y));
            }
        });

        minimized_stack.add_controller(drag);

        Self {
            catalog,
            sender,
            items: Vec::new(),
            active_book_id: None,
            previous_active_book_id: None,
            split_book_id: None,
            window_open: false,
            active_pane: ActivePane::Left,
            _pos: pos,
            root_overlay,
            _fixed_layer: fixed_layer,
            minimized_stack,
            window_scrim,
            window_container,
            top_bubbles_box,
            reader_content_box,
            drop_zone_indicator,
            drop_zone_label,
            primary_reader: None,
            split_reader: None,
        }
    }

    pub fn widget(&self) -> &gtk::Widget {
        self.root_overlay.upcast_ref()
    }

    pub fn open_bubble(&mut self, book_id: i64) {
        self.add_or_bring_to_front(book_id);
        if self.active_book_id != Some(book_id) {
            self.previous_active_book_id = self.active_book_id;
        }
        self.active_book_id = Some(book_id);
        self.window_open = true;
        self.refresh_ui();
    }

    pub fn minimize_book(&mut self, book_id: i64) {
        self.add_or_bring_to_front(book_id);
        if self.active_book_id != Some(book_id) {
            self.previous_active_book_id = self.active_book_id;
        }
        self.active_book_id = Some(book_id);
        self.window_open = false;
        self.refresh_ui();
    }

    pub fn close_bubble(&mut self, book_id: i64) {
        let closed_idx = self.items.iter().position(|item| item.book_id == book_id);
        self.items.retain(|item| item.book_id != book_id);

        if self.primary_reader.as_ref().map(|r| r.book_id()) == Some(book_id) {
            self.primary_reader = None;
        }
        if self.split_reader.as_ref().map(|r| r.book_id()) == Some(book_id) {
            self.split_reader = None;
            self.split_book_id = None;
        }

        if self.items.is_empty() {
            self.active_book_id = None;
            self.previous_active_book_id = None;
            self.split_book_id = None;
            self.window_open = false;
            self.primary_reader = None;
            self.split_reader = None;
            unsafe { libc::malloc_trim(0); }
            self.refresh_ui();
            return;
        }

        if self.active_book_id == Some(book_id) {
            // First choice: last active book if still in items
            let candidate = if let Some(prev) = self.previous_active_book_id {
                if self.items.iter().any(|i| i.book_id == prev) {
                    Some(prev)
                } else {
                    None
                }
            } else {
                None
            };

            // Second choice: book right beside it
            let next_active = candidate.or_else(|| {
                let idx = closed_idx.unwrap_or(0).min(self.items.len().saturating_sub(1));
                self.items.get(idx).map(|i| i.book_id)
            });

            self.active_book_id = next_active;
            self.primary_reader = None;
        }

        self.refresh_ui();
    }

    pub fn select_bubble(&mut self, book_id: i64) {
        let target_id = if book_id == 0 {
            self.active_book_id.or_else(|| self.items.first().map(|i| i.book_id))
        } else {
            Some(book_id)
        };
        let Some(target_id) = target_id else { return };

        if self.active_book_id != Some(target_id) {
            self.previous_active_book_id = self.active_book_id;
        }
        self.add_or_bring_to_front(target_id);
        self.active_book_id = Some(target_id);
        if self.split_book_id == Some(target_id) {
            self.split_book_id = None;
            self.split_reader = None;
        }
        self.window_open = true;
        self.refresh_ui();
    }

    pub fn set_split_book(&mut self, book_id: i64) {
        if self.active_book_id == Some(book_id) {
            return;
        }
        self.split_book_id = Some(book_id);
        self.window_open = true;
        self.refresh_ui();
    }

    pub fn close_split(&mut self) {
        self.split_book_id = None;
        self.split_reader = None;
        unsafe { libc::malloc_trim(0); }
        self.refresh_ui();
    }

    pub fn toggle_split(&mut self) {
        if self.split_book_id.is_some() {
            self.close_split();
        } else if let Some(candidate) = self.items.iter().find(|i| Some(i.book_id) != self.active_book_id) {
            self.set_split_book(candidate.book_id);
        }
    }

    pub fn minimize_window(&mut self) {
        self.window_open = false;
        self.refresh_ui();
    }

    pub fn close_window(&mut self) {
        self.window_open = false;
        self.refresh_ui();
    }

    pub fn set_active_pane(&mut self, pane: ActivePane) {
        self.active_pane = pane;
    }

    fn add_or_bring_to_front(&mut self, book_id: i64) {
        if let Some(pos) = self.items.iter().position(|i| i.book_id == book_id) {
            let item = self.items.remove(pos);
            self.items.insert(0, item);
        } else if let Some(item) = self.load_bubble_item(book_id) {
            self.items.insert(0, item);
        }
    }

    fn load_bubble_item(&self, book_id: i64) -> Option<BubbleItem> {
        let book = self.catalog.get_book(book_id).ok()??;
        let author = book.authors_display().to_string();
        Some(BubbleItem {
            book_id,
            title: book.title,
            author,
            format: book.format,
            progress: book.progress,
            cover_path: book.cover_path,
        })
    }

    /// Refresh all bubble widgets according to current state.
    pub fn refresh_ui(&mut self) {
        if self.items.is_empty() {
            self.minimized_stack.set_visible(false);
            self.window_scrim.set_visible(false);
            self.window_container.set_visible(false);
            return;
        }

        if self.window_open {
            self.render_top_strip();
            self.render_readers();
            self.minimized_stack.set_visible(false);
            self.window_scrim.set_visible(true);
            self.window_container.set_visible(true);
        } else {
            self.render_minimized_stack();
            self.minimized_stack.set_visible(true);
            self.window_scrim.set_visible(false);
            self.window_container.set_visible(false);
        }
    }

    /// Renders the collapsed floating bubble stack.
    /// - Shows the most recently active book's cover art and progress ring on the top circle.
    /// - Free-floating: stays anywhere dropped without edge snapping.
    /// - Reveals a hover [✕] button to dismiss with a click.
    fn render_minimized_stack(&self) {
        while let Some(child) = self.minimized_stack.first_child() {
            self.minimized_stack.remove(&child);
        }

        let Some(top_item) = self.items.first() else { return };
        let top_id = top_item.book_id;

        let root_circle = gtk::Overlay::new();
        root_circle.add_css_class("k-bubble-mini-circle");
        root_circle.set_size_request(56, 56);
        root_circle.set_valign(gtk::Align::Center);
        root_circle.set_halign(gtk::Align::Center);

        // Circular Progress Ring
        let progress_frac = (top_item.progress as f64 / 100.0).clamp(0.0, 1.0);
        let da = gtk::DrawingArea::new();
        da.set_draw_func(move |_, cr, w, h| {
            let center_x = w as f64 / 2.0;
            let center_y = h as f64 / 2.0;
            let radius = (center_x.min(center_y) - 3.0).max(2.0);
            cr.set_line_width(3.0);
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.15);
            let _ = cr.arc(center_x, center_y, radius, 0.0, 2.0 * std::f64::consts::PI);
            let _ = cr.stroke();
            if progress_frac > 0.0 {
                cr.set_source_rgba(0.208, 0.518, 0.894, 0.95);
                let start_angle = -std::f64::consts::FRAC_PI_2;
                let end_angle = start_angle + (2.0 * std::f64::consts::PI * progress_frac);
                let _ = cr.arc(center_x, center_y, radius, start_angle, end_angle);
                let _ = cr.stroke();
            }
        });
        root_circle.set_child(Some(&da));

        // Cover thumbnail inside the circular ring
        let cover_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        cover_box.add_css_class("k-bubble-inner-cover");
        cover_box.set_halign(gtk::Align::Center);
        cover_box.set_valign(gtk::Align::Center);

        if let Some(cover_path) = &top_item.cover_path {
            if cover_path.exists() {
                let pic = gtk::Picture::for_filename(cover_path);
                pic.set_content_fit(gtk::ContentFit::Cover);
                pic.set_size_request(42, 42);
                cover_box.append(&pic);
            } else {
                let icon = gtk::Image::from_icon_name("book-open-symbolic");
                icon.set_pixel_size(24);
                cover_box.append(&icon);
            }
        } else {
            let icon = gtk::Image::from_icon_name("book-open-symbolic");
            icon.set_pixel_size(24);
            cover_box.append(&icon);
        }
        root_circle.add_overlay(&cover_box);

        // Multiple books badge count
        if self.items.len() > 1 {
            let count_lbl = gtk::Label::new(Some(&format!("{}", self.items.len())));
            count_lbl.add_css_class("k-bubble-count-badge");
            count_lbl.set_halign(gtk::Align::End);
            count_lbl.set_valign(gtk::Align::End);
            count_lbl.set_margin_end(2);
            count_lbl.set_margin_bottom(2);
            root_circle.add_overlay(&count_lbl);
        }

        // Left-click [✕] button revealed on hover for desktop convenience
        let close_btn = gtk::Button::from_icon_name("window-close-symbolic");
        close_btn.add_css_class("k-bubble-mini-close");
        close_btn.set_tooltip_text(Some("Dismiss Bubble"));
        close_btn.set_halign(gtk::Align::Start);
        close_btn.set_valign(gtk::Align::Start);
        close_btn.set_visible(false);

        let s_close = self.sender.clone();
        close_btn.connect_clicked(move |_| {
            s_close.input(AppMsg::BubbleClose { book_id: top_id });
        });
        root_circle.add_overlay(&close_btn);

        let hover = gtk::EventControllerMotion::new();
        let close_ref = close_btn.clone();
        hover.connect_enter(move |_, _, _| {
            close_ref.set_visible(true);
        });
        let close_ref2 = close_btn;
        hover.connect_leave(move |_| {
            close_ref2.set_visible(false);
        });
        root_circle.add_controller(hover);

        self.minimized_stack.append(&root_circle);
    }

    /// Renders the floating icons row above the reading card.
    /// - Active book: Compact fixed pill (~165px wide) with cover morphing into [✕] on hover.
    /// - Inactive books: Circular discs with cover and progress ring.
    /// - Hovering inactive circles slides out an action drawer with [Split Screen] and [Close].
    fn render_top_strip(&self) {
        while let Some(child) = self.top_bubbles_box.first_child() {
            self.top_bubbles_box.remove(&child);
        }

        let active_id = self.active_book_id;
        let split_id = self.split_book_id;

        for item in &self.items {
            let is_active = Some(item.book_id) == active_id;
            let is_split = Some(item.book_id) == split_id;
            let id = item.book_id;

            if is_active {
                // Compact fixed-size horizontal pill (165px wide)
                let pill = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                pill.add_css_class("k-bubble-pill-active");
                pill.set_size_request(165, 34);
                pill.set_valign(gtk::Align::Center);

                let icon_stack = gtk::Stack::new();
                icon_stack.set_transition_type(gtk::StackTransitionType::Crossfade);
                icon_stack.set_transition_duration(150);

                let cover_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
                cover_box.set_size_request(22, 22);
                cover_box.set_valign(gtk::Align::Center);
                cover_box.set_halign(gtk::Align::Center);
                cover_box.add_css_class("k-bubble-cover-disc");

                if let Some(cp) = &item.cover_path {
                    if cp.exists() {
                        let pic = gtk::Picture::for_filename(cp);
                        pic.set_content_fit(gtk::ContentFit::Cover);
                        pic.set_size_request(22, 22);
                        cover_box.append(&pic);
                    } else {
                        let ic = gtk::Image::from_icon_name("book-open-symbolic");
                        ic.set_pixel_size(14);
                        cover_box.append(&ic);
                    }
                } else {
                    let ic = gtk::Image::from_icon_name("book-open-symbolic");
                    ic.set_pixel_size(14);
                    cover_box.append(&ic);
                }
                icon_stack.add_named(&cover_box, Some("cover"));

                // [✕] Close button revealed in place of cover on hover
                let close_btn = gtk::Button::from_icon_name("window-close-symbolic");
                close_btn.add_css_class("k-bubble-mini-close-btn");
                close_btn.set_tooltip_text(Some("Close book"));
                let s_close = self.sender.clone();
                close_btn.connect_clicked(move |_| {
                    s_close.input(AppMsg::BubbleClose { book_id: id });
                });
                icon_stack.add_named(&close_btn, Some("close"));
                icon_stack.set_visible_child_name("cover");

                let hover = gtk::EventControllerMotion::new();
                let stack_h = icon_stack.clone();
                hover.connect_enter(move |_, _, _| {
                    stack_h.set_visible_child_name("close");
                });
                let stack_l = icon_stack.clone();
                hover.connect_leave(move |_| {
                    stack_l.set_visible_child_name("cover");
                });
                pill.add_controller(hover);

                let title_lbl = gtk::Label::new(Some(&item.title));
                title_lbl.add_css_class("k-bubble-pill-title");
                title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
                title_lbl.set_hexpand(true);
                title_lbl.set_halign(gtk::Align::Start);

                pill.append(&icon_stack);
                pill.append(&title_lbl);

                self.top_bubbles_box.append(&pill);
            } else {
                // Inactive book: Circular disc with slide-out hover drawer
                let container = gtk::Box::new(gtk::Orientation::Horizontal, 4);
                container.add_css_class("k-bubble-inactive-container");
                container.set_valign(gtk::Align::Center);

                let circle_overlay = gtk::Overlay::new();
                circle_overlay.add_css_class("k-bubble-circle-inactive");
                if is_split {
                    circle_overlay.add_css_class("k-bubble-circle-split");
                }
                circle_overlay.set_size_request(34, 34);
                circle_overlay.set_valign(gtk::Align::Center);
                circle_overlay.set_cursor_from_name(Some("pointer"));

                // Progress ring
                let progress_frac = (item.progress as f64 / 100.0).clamp(0.0, 1.0);
                let da = gtk::DrawingArea::new();
                da.set_draw_func(move |_, cr, w, h| {
                    let center_x = w as f64 / 2.0;
                    let center_y = h as f64 / 2.0;
                    let radius = (center_x.min(center_y) - 2.0).max(2.0);
                    cr.set_line_width(2.5);
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.15);
                    let _ = cr.arc(center_x, center_y, radius, 0.0, 2.0 * std::f64::consts::PI);
                    let _ = cr.stroke();
                    if progress_frac > 0.0 {
                        cr.set_source_rgba(0.208, 0.518, 0.894, 0.95);
                        let start_angle = -std::f64::consts::FRAC_PI_2;
                        let end_angle = start_angle + (2.0 * std::f64::consts::PI * progress_frac);
                        let _ = cr.arc(center_x, center_y, radius, start_angle, end_angle);
                        let _ = cr.stroke();
                    }
                });
                circle_overlay.set_child(Some(&da));

                let inner_cover = gtk::Box::new(gtk::Orientation::Vertical, 0);
                inner_cover.add_css_class("k-bubble-inner-cover");
                inner_cover.set_halign(gtk::Align::Center);
                inner_cover.set_valign(gtk::Align::Center);
                if let Some(cp) = &item.cover_path {
                    if cp.exists() {
                        let pic = gtk::Picture::for_filename(cp);
                        pic.set_content_fit(gtk::ContentFit::Cover);
                        pic.set_size_request(24, 24);
                        inner_cover.append(&pic);
                    } else {
                        let ic = gtk::Image::from_icon_name("book-open-symbolic");
                        ic.set_pixel_size(14);
                        inner_cover.append(&ic);
                    }
                } else {
                    let ic = gtk::Image::from_icon_name("book-open-symbolic");
                    ic.set_pixel_size(14);
                    inner_cover.append(&ic);
                }
                circle_overlay.add_overlay(&inner_cover);

                // Clicking the circle directly activates this book
                let s_select = self.sender.clone();
                let click = gtk::GestureClick::new();
                click.connect_released(move |_, _, _, _| {
                    s_select.input(AppMsg::BubbleSelect { book_id: id });
                });
                circle_overlay.add_controller(click);

                // Hover Drawer with [Split Screen] and [Close]
                let drawer = gtk::Box::new(gtk::Orientation::Horizontal, 2);
                drawer.add_css_class("k-bubble-drawer");
                drawer.set_valign(gtk::Align::Center);
                drawer.set_visible(false);

                // Split button
                let split_btn = gtk::Button::from_icon_name("view-dual-symbolic");
                split_btn.add_css_class("k-bubble-drawer-btn");
                split_btn.set_tooltip_text(Some("Split Side-by-Side"));
                let s_split = self.sender.clone();
                split_btn.connect_clicked(move |_| {
                    s_split.input(AppMsg::BubbleSetSplit { book_id: id });
                });
                drawer.append(&split_btn);

                // Close button
                let close_btn = gtk::Button::from_icon_name("window-close-symbolic");
                close_btn.add_css_class("k-bubble-drawer-btn");
                close_btn.set_tooltip_text(Some("Close book"));
                let s_close = self.sender.clone();
                close_btn.connect_clicked(move |_| {
                    s_close.input(AppMsg::BubbleClose { book_id: id });
                });
                drawer.append(&close_btn);

                let hover = gtk::EventControllerMotion::new();
                let d_h = drawer.clone();
                hover.connect_enter(move |_, _, _| {
                    d_h.set_visible(true);
                });
                let d_l = drawer.clone();
                hover.connect_leave(move |_| {
                    d_l.set_visible(false);
                });
                container.add_controller(hover);

                // Drag & Drop to split side-by-side
                let drag = gtk::GestureDrag::new();
                let drop_zone = self.drop_zone_indicator.clone();
                let drop_lbl = self.drop_zone_label.clone();
                let s_drag = self.sender.clone();
                let drag_title = item.title.clone();

                drag.connect_drag_update(move |_, _, offset_y| {
                    if offset_y > 20.0 {
                        drop_zone.set_visible(true);
                        drop_lbl.set_label(&format!("Drop to read side-by-side:\n{}", drag_title));
                    } else {
                        drop_zone.set_visible(false);
                    }
                });

                let drop_zone_end = self.drop_zone_indicator.clone();
                drag.connect_drag_end(move |_, _, offset_y| {
                    drop_zone_end.set_visible(false);
                    if offset_y > 35.0 {
                        s_drag.input(AppMsg::BubbleSetSplit { book_id: id });
                    }
                });
                circle_overlay.add_controller(drag);

                container.append(&circle_overlay);
                container.append(&drawer);
                self.top_bubbles_box.append(&container);
            }
        }
    }

    /// Renders the reading surface: either a single reader or side-by-side split view.
    fn render_readers(&mut self) {
        while let Some(child) = self.reader_content_box.first_child() {
            self.reader_content_box.remove(&child);
        }

        let Some(active_id) = self.active_book_id else { return };
        let split_id = self.split_book_id;

        if let Some(second_id) = split_id {
            // Side-by-Side Split View (Two Books at Once)
            let left_widget = self.ensure_primary_reader(active_id).map(|r| r.widget().clone());
            let right_widget = self.ensure_split_reader(second_id).map(|r| r.widget().clone());

            let split_box = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            split_box.set_hexpand(true);
            split_box.set_vexpand(true);

            // Left Pane (Active Book)
            let left_pane = gtk::Box::new(gtk::Orientation::Vertical, 0);
            left_pane.set_hexpand(true);
            left_pane.set_vexpand(true);
            if self.active_pane == ActivePane::Left {
                left_pane.add_css_class("k-bubble-pane-focused");
            }

            if let Some(w) = left_widget {
                left_pane.append(&w);
            }

            // Click gesture to set Left Pane as active for fair reading-time tracking
            let s_pane_l = self.sender.clone();
            let click_l = gtk::GestureClick::new();
            click_l.connect_released(move |_, _, _, _| {
                s_pane_l.input(AppMsg::BubbleSetActivePane(ActivePane::Left));
            });
            left_pane.add_controller(click_l);
            split_box.append(&left_pane);

            // Thin vertical divider between panes
            let divider = gtk::Separator::new(gtk::Orientation::Vertical);
            divider.add_css_class("k-bubble-split-divider");
            split_box.append(&divider);

            // Right Pane (Split Book)
            let right_pane = gtk::Box::new(gtk::Orientation::Vertical, 0);
            right_pane.set_hexpand(true);
            right_pane.set_vexpand(true);
            if self.active_pane == ActivePane::Right {
                right_pane.add_css_class("k-bubble-pane-focused");
            }

            if let Some(w) = right_widget {
                right_pane.append(&w);
            }

            // Click gesture to set Right Pane as active for fair reading-time tracking
            let s_pane_r = self.sender.clone();
            let click_r = gtk::GestureClick::new();
            click_r.connect_released(move |_, _, _, _| {
                s_pane_r.input(AppMsg::BubbleSetActivePane(ActivePane::Right));
            });
            right_pane.add_controller(click_r);
            split_box.append(&right_pane);

            self.reader_content_box.append(&split_box);
        } else {
            // Single Book View
            let widget = self.ensure_primary_reader(active_id).map(|r| r.widget().clone());
            if let Some(w) = widget {
                self.reader_content_box.append(&w);
            }
        }
    }

    fn ensure_primary_reader(&mut self, book_id: i64) -> Option<&BubbleReaderInstance> {
        let needs_build = match &self.primary_reader {
            Some(r) => r.book_id() != book_id,
            None => true,
        };
        if needs_build {
            self.primary_reader = self.build_reader_instance(book_id);
        }
        self.primary_reader.as_ref()
    }

    fn ensure_split_reader(&mut self, book_id: i64) -> Option<&BubbleReaderInstance> {
        let needs_build = match &self.split_reader {
            Some(r) => r.book_id() != book_id,
            None => true,
        };
        if needs_build {
            self.split_reader = self.build_reader_instance(book_id);
        }
        self.split_reader.as_ref()
    }

    fn build_reader_instance(&self, book_id: i64) -> Option<BubbleReaderInstance> {
        let book = self.catalog.get_book(book_id).ok()??;
        match book.format {
            BookFormat::Epub => {
                let ctrl = crate::pages::reader::ReaderModel::builder()
                    .launch((self.catalog.clone(), book_id))
                    .forward(self.sender.input_sender(), move |out| match out {
                        crate::pages::reader::ReaderOut::Close => AppMsg::BubbleMinimizeWindow,
                        crate::pages::reader::ReaderOut::MinimizeToBubble { book_id } => {
                            AppMsg::MinimizeToBubble { book_id }
                        }
                        crate::pages::reader::ReaderOut::OpenAuthor { name } => {
                            AppMsg::Push(crate::models::Route::AuthorPage { author: name })
                        }
                    });
                Some(BubbleReaderInstance::Epub { book_id, ctrl })
            }
            BookFormat::Pdf => {
                let init = crate::pages::pdf_reader::PdfReaderInit {
                    book_id,
                    catalog: self.catalog.clone(),
                };
                let ctrl = crate::pages::pdf_reader::PdfReaderModel::builder()
                    .launch(init)
                    .forward(self.sender.input_sender(), move |out| match out {
                        crate::pages::pdf_reader::PdfReaderOut::Close => AppMsg::BubbleMinimizeWindow,
                        crate::pages::pdf_reader::PdfReaderOut::MinimizeToBubble { book_id } => {
                            AppMsg::MinimizeToBubble { book_id }
                        }
                    });
                Some(BubbleReaderInstance::Pdf { book_id, ctrl })
            }
            BookFormat::Cbz | BookFormat::Cbr => {
                let provider = crate::pages::comics_reader::providers::LocalProvider::new(book.file_path.clone()).ok()?;
                let init = crate::pages::comics_reader::types::ComicsReaderInit {
                    title: book.title.clone(),
                    provider: std::sync::Arc::new(provider),
                    catalog: Some(self.catalog.clone()),
                    book_id: Some(book_id),
                    cover_path: book.cover_path.clone(),
                };
                let ctrl = crate::pages::comics_reader::ComicsReaderModel::builder()
                    .launch(init)
                    .forward(self.sender.input_sender(), move |out| match out {
                        crate::pages::comics_reader::types::ComicsReaderOut::Close => AppMsg::BubbleMinimizeWindow,
                        crate::pages::comics_reader::types::ComicsReaderOut::MinimizeToBubble { book_id } => {
                            AppMsg::MinimizeToBubble { book_id }
                        }
                    });
                Some(BubbleReaderInstance::Comic { book_id, ctrl })
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bubble_item_equality_and_ordering() {
        let b1 = BubbleItem {
            book_id: 1,
            title: "Book One".to_string(),
            author: "Author A".to_string(),
            format: BookFormat::Epub,
            progress: 45,
            cover_path: None,
        };
        let b2 = BubbleItem {
            book_id: 2,
            title: "Book Two".to_string(),
            author: "Author B".to_string(),
            format: BookFormat::Pdf,
            progress: 80,
            cover_path: None,
        };

        let mut items = vec![b1.clone(), b2.clone()];
        assert_eq!(items.first(), Some(&b1));

        // Reordering brings selected item to front
        let removed = items.remove(1);
        items.insert(0, removed);
        assert_eq!(items.first(), Some(&b2));
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn test_active_pane_state() {
        let pane = ActivePane::Left;
        assert_eq!(pane, ActivePane::Left);
        let pane_r = ActivePane::Right;
        assert_ne!(pane, pane_r);
    }

    #[test]
    fn test_smart_fallback_selection_on_close() {
        let b1 = BubbleItem {
            book_id: 1,
            title: "Book One".to_string(),
            author: "Author A".to_string(),
            format: BookFormat::Epub,
            progress: 45,
            cover_path: None,
        };
        let b2 = BubbleItem {
            book_id: 2,
            title: "Book Two".to_string(),
            author: "Author B".to_string(),
            format: BookFormat::Pdf,
            progress: 80,
            cover_path: None,
        };
        let b3 = BubbleItem {
            book_id: 3,
            title: "Book Three".to_string(),
            author: "Author C".to_string(),
            format: BookFormat::Cbz,
            progress: 10,
            cover_path: None,
        };

        let mut items = vec![b1, b2, b3];
        let closed_id = 2;
        let closed_idx = items.iter().position(|i| i.book_id == closed_id);
        items.retain(|i| i.book_id != closed_id);

        let previous_active = Some(1);
        let next_active = previous_active
            .filter(|&prev| items.iter().any(|i| i.book_id == prev))
            .or_else(|| {
                let idx = closed_idx.unwrap_or(0).min(items.len().saturating_sub(1));
                items.get(idx).map(|i| i.book_id)
            });

        assert_eq!(next_active, Some(1));
    }
}
