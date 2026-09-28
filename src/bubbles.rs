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
    Epub(relm4::Controller<crate::pages::reader::ReaderModel>),
    Pdf(relm4::Controller<crate::pages::pdf_reader::PdfReaderModel>),
    Comic(relm4::Controller<crate::pages::comics_reader::ComicsReaderModel>),
}

impl BubbleReaderInstance {
    pub fn widget(&self) -> &gtk::Widget {
        match self {
            BubbleReaderInstance::Epub(ctrl) => ctrl.widget().upcast_ref(),
            BubbleReaderInstance::Pdf(ctrl) => ctrl.widget().upcast_ref(),
            BubbleReaderInstance::Comic(ctrl) => ctrl.widget().upcast_ref(),
        }
    }
}

pub struct BubbleManager {
    catalog: Arc<Catalog>,
    sender: ComponentSender<AppModel>,
    items: Vec<BubbleItem>,
    active_book_id: Option<i64>,
    split_book_id: Option<i64>,
    window_open: bool,
    active_pane: ActivePane,
    pos: Rc<RefCell<(f64, f64)>>,

    // Root UI container overlay
    root_overlay: gtk::Overlay,
    fixed_layer: gtk::Fixed,
    minimized_stack: gtk::Box,

    // Window widgets
    window_scrim: gtk::Box,
    window_card: gtk::Box,
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
        // By default, the root overlay does not catch clicks meant for the app underneath
        root_overlay.set_can_target(false);

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

        // Window scrim (dimmed backdrop behind floating inset window)
        let window_scrim = gtk::Box::new(gtk::Orientation::Vertical, 0);
        window_scrim.add_css_class("k-bubble-window-scrim");
        window_scrim.set_hexpand(true);
        window_scrim.set_vexpand(true);
        window_scrim.set_visible(false);
        window_scrim.set_can_target(true);

        let s_scrim = sender.clone();
        let scrim_click = gtk::GestureClick::new();
        scrim_click.connect_released(move |_, _, _, _| {
            s_scrim.input(AppMsg::BubbleMinimizeWindow);
        });
        window_scrim.add_controller(scrim_click);
        root_overlay.add_overlay(&window_scrim);

        // Android-style inset floating window card
        let window_card = gtk::Box::new(gtk::Orientation::Vertical, 0);
        window_card.add_css_class("k-bubble-window-card");
        window_card.set_hexpand(true);
        window_card.set_vexpand(true);
        window_card.set_margin_start(24);
        window_card.set_margin_end(24);
        window_card.set_margin_top(20);
        window_card.set_margin_bottom(20);
        window_card.set_visible(false);
        window_card.set_can_target(true);

        // Top Switcher Strip
        let top_bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        top_bar.add_css_class("k-bubble-topbar");

        let top_bubbles_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        top_bubbles_box.set_hexpand(true);
        top_bar.append(&top_bubbles_box);

        // Right-aligned window actions
        let actions_box = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        actions_box.set_halign(gtk::Align::End);

        let s_split = sender.clone();
        let split_btn = gtk::Button::from_icon_name("view-dual-symbolic");
        split_btn.add_css_class("flat");
        split_btn.set_tooltip_text(Some("Toggle Side-by-Side Split View"));
        split_btn.connect_clicked(move |_| {
            s_split.input(AppMsg::BubbleToggleSplit);
        });
        actions_box.append(&split_btn);

        let s_min = sender.clone();
        let min_btn = gtk::Button::from_icon_name("window-minimize-symbolic");
        min_btn.add_css_class("flat");
        min_btn.set_tooltip_text(Some("Minimize to Bubble"));
        min_btn.connect_clicked(move |_| {
            s_min.input(AppMsg::BubbleMinimizeWindow);
        });
        actions_box.append(&min_btn);

        let s_close = sender.clone();
        let close_btn = gtk::Button::from_icon_name("window-close-symbolic");
        close_btn.add_css_class("flat");
        close_btn.set_tooltip_text(Some("Close Reading Window"));
        close_btn.connect_clicked(move |_| {
            s_close.input(AppMsg::BubbleCloseWindow);
        });
        actions_box.append(&close_btn);

        top_bar.append(&actions_box);
        window_card.append(&top_bar);

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
        drop_zone_label.set_valign(gtk::Align::Center);
        drop_zone_label.set_vexpand(true);
        drop_zone_indicator.append(&drop_zone_label);

        reader_overlay.add_overlay(&drop_zone_indicator);
        window_card.append(&reader_overlay);
        root_overlay.add_overlay(&window_card);

        let pos = Rc::new(RefCell::new((720.0, 480.0)));

        // Setup Draggable gesture with ZERO edge snapping
        let drag = gtk::GestureDrag::new();
        let start_coord = Rc::new(RefCell::new((0.0, 0.0)));
        let start_coord_drag = start_coord.clone();
        let start_coord_end = start_coord;
        let pos_drag = pos.clone();
        let pos_end = pos.clone();
        let fixed_drag = fixed_layer.clone();
        let stack_drag = minimized_stack.clone();
        let s_click = sender.clone();

        drag.connect_drag_begin(move |_, x, y| {
            let current = *pos_drag.borrow();
            start_coord_drag.replace((current.0, current.1));
        });

        drag.connect_drag_update(move |_, offset_x, offset_y| {
            let (sx, sy) = *start_coord_drag.borrow();
            let new_x = (sx + offset_x).max(10.0f64);
            let new_y = (sy + offset_y).max(10.0f64);
            fixed_drag.move_(&stack_drag, new_x, new_y);
        });

        drag.connect_drag_end(move |_, offset_x, offset_y| {
            let (sx, sy) = *start_coord_end.borrow();
            let dist = offset_x.hypot(offset_y);
            if dist < 6.0f64 {
                // Short movement counts as a click: expand reading window!
                s_click.input(AppMsg::BubbleSelect { book_id: 0 }); // 0 means expand top/active
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
            split_book_id: None,
            window_open: false,
            active_pane: ActivePane::Left,
            pos,
            root_overlay,
            fixed_layer,
            minimized_stack,
            window_scrim,
            window_card,
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
        self.active_book_id = Some(book_id);
        self.window_open = true;
        self.refresh_ui();
    }

    pub fn minimize_book(&mut self, book_id: i64) {
        self.add_or_bring_to_front(book_id);
        self.active_book_id = Some(book_id);
        self.window_open = false;
        self.refresh_ui();
    }

    pub fn close_bubble(&mut self, book_id: i64) {
        self.items.retain(|item| item.book_id != book_id);
        if self.split_book_id == Some(book_id) {
            self.split_book_id = None;
            self.split_reader = None;
        }
        if self.active_book_id == Some(book_id) {
            self.active_book_id = self.items.first().map(|i| i.book_id);
            self.primary_reader = None;
        }
        if self.items.is_empty() {
            self.window_open = false;
            self.active_book_id = None;
            self.split_book_id = None;
            self.primary_reader = None;
            self.split_reader = None;
            unsafe { libc::malloc_trim(0); }
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

        self.add_or_bring_to_front(target_id);
        self.active_book_id = Some(target_id);
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
            self.window_card.set_visible(false);
            return;
        }

        if self.window_open {
            self.minimized_stack.set_visible(false);
            self.window_scrim.set_visible(true);
            self.window_card.set_visible(true);
            self.render_top_strip();
            self.render_readers();
        } else {
            self.window_scrim.set_visible(false);
            self.window_card.set_visible(false);
            self.minimized_stack.set_visible(true);
            self.render_minimized_stack();
        }
    }

    /// Renders the compact circular disc for the minimized bubble stack.
    /// The last active book is on top, displaying its cover art and progress ring.
    fn render_minimized_stack(&self) {
        while let Some(child) = self.minimized_stack.first_child() {
            self.minimized_stack.remove(&child);
        }

        let Some(top_item) = self.items.first() else { return };
        let top_id = top_item.book_id;

        let bubble_overlay = gtk::Overlay::new();
        bubble_overlay.add_css_class("k-bubble-mini-circle");
        bubble_overlay.set_size_request(60, 60);

        // Progress ring drawn on DrawingArea
        let progress_frac = (top_item.progress as f64 / 100.0).clamp(0.0, 1.0);
        let da = gtk::DrawingArea::new();
        da.set_content_width(60);
        da.set_content_height(60);
        da.set_draw_func(move |_area, cr, width, height| {
            let w = width as f64;
            let h = height as f64;
            let center_x = w / 2.0;
            let center_y = h / 2.0;
            let radius = (w.min(h) / 2.0) - 2.5;
            if radius <= 0.0 {
                return;
            }
            // Background track
            cr.set_line_width(2.5);
            cr.set_source_rgba(1.0, 1.0, 1.0, 0.20);
            cr.arc(center_x, center_y, radius, 0.0, 2.0 * std::f64::consts::PI);
            let _ = cr.stroke();

            // Progress arc
            if progress_frac > 0.001 {
                cr.set_line_width(3.0);
                cr.set_source_rgba(0.21, 0.52, 0.89, 0.95);
                let start_angle = -std::f64::consts::FRAC_PI_2;
                let end_angle = start_angle + progress_frac * 2.0 * std::f64::consts::PI;
                cr.arc(center_x, center_y, radius, start_angle, end_angle);
                let _ = cr.stroke();
            }
        });
        bubble_overlay.set_child(Some(&da));

        // Center circular cover
        let cover_box = gtk::Box::new(gtk::Orientation::Vertical, 0);
        cover_box.set_halign(gtk::Align::Center);
        cover_box.set_valign(gtk::Align::Center);
        cover_box.add_css_class("k-bubble-inner-cover");
        let cover = crate::widgets::book_row::cover_widget(top_item.cover_path.as_deref(), 50, 50);
        cover_box.append(&cover);
        bubble_overlay.add_overlay(&cover_box);

        // Stack count badge if more than 1 book
        if self.items.len() > 1 {
            let badge = gtk::Label::new(Some(&format!("{}", self.items.len())));
            badge.add_css_class("k-bubble-count-badge");
            badge.set_halign(gtk::Align::End);
            badge.set_valign(gtk::Align::End);
            badge.set_margin_end(2);
            badge.set_margin_bottom(2);
            bubble_overlay.add_overlay(&badge);
        }

        // Quick close [✕] button on hover (left-click closes directly)
        let close_btn = gtk::Button::from_icon_name("window-close-symbolic");
        close_btn.add_css_class("k-bubble-mini-close");
        close_btn.set_halign(gtk::Align::End);
        close_btn.set_valign(gtk::Align::Start);
        close_btn.set_visible(false);

        let s_close = self.sender.clone();
        close_btn.connect_clicked(move |_| {
            s_close.input(AppMsg::BubbleClose { book_id: top_id });
        });
        bubble_overlay.add_overlay(&close_btn);

        // Reveal close button on hover
        let close_ref = close_btn.clone();
        let hover = gtk::EventControllerMotion::new();
        hover.connect_enter(move |_, _, _| {
            close_ref.set_visible(true);
        });
        let close_leave = close_btn;
        hover.connect_leave(move |_| {
            close_leave.set_visible(false);
        });
        bubble_overlay.add_controller(hover);

        self.minimized_stack.append(&bubble_overlay);
    }

    /// Renders the top switcher strip inside the expanded window card:
    /// - Active book: Fixed-size horizontal pill displaying its book title and [✕].
    /// - Inactive books: Circular discs with cover and progress ring.
    /// - Hovering inactive circles gives rich popover with Title, Author, Progress, and Format.
    /// - Dragging an inactive circle into the reader splits the screen side-by-side (Zen Browser style).
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
                // Fixed-size horizontal pill showing title only
                let pill = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                pill.add_css_class("k-bubble-pill-active");
                pill.set_size_request(190, 36);
                pill.set_valign(gtk::Align::Center);

                let icon = gtk::Image::from_icon_name("book-open-symbolic");
                icon.set_pixel_size(14);
                pill.append(&icon);

                let title_lbl = gtk::Label::new(Some(&item.title));
                title_lbl.add_css_class("k-bubble-pill-title");
                title_lbl.set_ellipsize(gtk::pango::EllipsizeMode::End);
                title_lbl.set_hexpand(true);
                title_lbl.set_halign(gtk::Align::Start);
                pill.append(&title_lbl);

                let s_close = self.sender.clone();
                let close_pill_btn = gtk::Button::from_icon_name("window-close-symbolic");
                close_pill_btn.add_css_class("k-bubble-pill-close");
                close_pill_btn.set_tooltip_text(Some("Close book"));
                close_pill_btn.connect_clicked(move |_| {
                    s_close.input(AppMsg::BubbleClose { book_id: id });
                });
                pill.append(&close_pill_btn);

                self.top_bubbles_box.append(&pill);
            } else {
                // Inactive book: Circular disc
                let circle_overlay = gtk::Overlay::new();
                circle_overlay.add_css_class("k-bubble-circle-inactive");
                if is_split {
                    circle_overlay.add_css_class("k-bubble-circle-split");
                }
                circle_overlay.set_size_request(36, 36);
                circle_overlay.set_valign(gtk::Align::Center);

                // Progress ring
                let progress_frac = (item.progress as f64 / 100.0).clamp(0.0, 1.0);
                let da = gtk::DrawingArea::new();
                da.set_content_width(36);
                da.set_content_height(36);
                da.set_draw_func(move |_area, cr, width, height| {
                    let w = width as f64;
                    let h = height as f64;
                    let center_x = w / 2.0;
                    let center_y = h / 2.0;
                    let radius = (w.min(h) / 2.0) - 1.8;
                    if radius <= 0.0 {
                        return;
                    }
                    cr.set_line_width(2.0);
                    cr.set_source_rgba(1.0, 1.0, 1.0, 0.18);
                    cr.arc(center_x, center_y, radius, 0.0, 2.0 * std::f64::consts::PI);
                    let _ = cr.stroke();

                    if progress_frac > 0.001 {
                        cr.set_line_width(2.2);
                        cr.set_source_rgba(0.21, 0.52, 0.89, 0.95);
                        let start_angle = -std::f64::consts::FRAC_PI_2;
                        let end_angle = start_angle + progress_frac * 2.0 * std::f64::consts::PI;
                        cr.arc(center_x, center_y, radius, start_angle, end_angle);
                        let _ = cr.stroke();
                    }
                });
                circle_overlay.set_child(Some(&da));

                let cover = crate::widgets::book_row::cover_widget(item.cover_path.as_deref(), 30, 30);
                cover.set_halign(gtk::Align::Center);
                cover.set_valign(gtk::Align::Center);
                circle_overlay.add_overlay(&cover);

                // Left click to switch active book
                let s_sel = self.sender.clone();
                let click = gtk::GestureClick::new();
                click.set_button(1);
                click.connect_released(move |_, _, _, _| {
                    s_sel.input(AppMsg::BubbleSelect { book_id: id });
                });
                circle_overlay.add_controller(click);

                // Right click context menu for quick side-by-side split
                let s_ctx = self.sender.clone();
                let right_click = gtk::GestureClick::new();
                right_click.set_button(3);
                let pop_parent = circle_overlay.clone();
                right_click.connect_released(move |_, _, _, _| {
                    let popover = gtk::Popover::new();
                    popover.set_parent(&pop_parent);
                    let box_menu = gtk::Box::new(gtk::Orientation::Vertical, 4);

                    let split_action = gtk::Button::with_label("Split Side-by-Side");
                    split_action.add_css_class("flat");
                    let s_split = s_ctx.clone();
                    let pop_c = popover.clone();
                    split_action.connect_clicked(move |_| {
                        pop_c.popdown();
                        s_split.input(AppMsg::BubbleSetSplit { book_id: id });
                    });
                    box_menu.append(&split_action);

                    let close_action = gtk::Button::with_label("Close Bubble");
                    close_action.add_css_class("flat");
                    let s_close = s_ctx.clone();
                    let pop_c2 = popover.clone();
                    close_action.connect_clicked(move |_| {
                        pop_c2.popdown();
                        s_close.input(AppMsg::BubbleClose { book_id: id });
                    });
                    box_menu.append(&close_action);

                    popover.set_child(Some(&box_menu));
                    popover.popup();
                });
                circle_overlay.add_controller(right_click);

                // Rich Hover Tooltip with Title, Author, Progress, and Format
                let hover = gtk::EventControllerMotion::new();
                let title_h = item.title.clone();
                let author_h = item.author.clone();
                let format_h = item.format.as_str().to_uppercase();
                let progress_h = item.progress;
                let pop_anchor = circle_overlay.clone();
                hover.connect_enter(move |_, _, _| {
                    let tooltip_text = format!(
                        "{}\nby {}\n{} · {}% read",
                        title_h, author_h, format_h, progress_h
                    );
                    pop_anchor.set_tooltip_text(Some(&tooltip_text));
                });
                circle_overlay.add_controller(hover);

                // Zen Browser-Style Drag & Drop to Split
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

                self.top_bubbles_box.append(&circle_overlay);
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

            let left_reader = self.ensure_primary_reader(active_id);
            if let Some(r) = left_reader {
                left_pane.append(r.widget());
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

            let right_reader = self.ensure_split_reader(second_id);
            if let Some(r) = right_reader {
                right_pane.append(r.widget());
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
            let reader = self.ensure_primary_reader(active_id);
            if let Some(r) = reader {
                self.reader_content_box.append(r.widget());
            }
        }
    }

    fn ensure_primary_reader(&mut self, book_id: i64) -> Option<&BubbleReaderInstance> {
        let needs_rebuild = match &self.primary_reader {
            None => true,
            Some(_) => false,
        };
        if needs_rebuild {
            self.primary_reader = self.build_reader_instance(book_id);
        }
        self.primary_reader.as_ref()
    }

    fn ensure_split_reader(&mut self, book_id: i64) -> Option<&BubbleReaderInstance> {
        let needs_rebuild = match &self.split_reader {
            None => true,
            Some(_) => false,
        };
        if needs_rebuild {
            self.split_reader = self.build_reader_instance(book_id);
        }
        self.split_reader.as_ref()
    }

    /// Builds the appropriate reader controller based on book format (EPUB, PDF, or Comic).
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
                Some(BubbleReaderInstance::Epub(ctrl))
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
                Some(BubbleReaderInstance::Pdf(ctrl))
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
                Some(BubbleReaderInstance::Comic(ctrl))
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
}
