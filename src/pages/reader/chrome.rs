//! Shell synchronizers, hover-open controllers, labels, tabs, and cover
//! host updates.

use super::mod_model::ReaderModel;
use super::settings_panel::reader_settings_pane_name;
use super::types::*;
use crate::widgets::book_row::cover_widget;
use gtk::prelude::*;
use relm4::ComponentSender;

pub(crate) fn sync_reader_stacks(model: &ReaderModel) {
    model
        .left_stack
        .set_visible_child_name(match model.left_tab {
            LeftSidebarTab::Toc => "toc",
            LeftSidebarTab::Settings => "settings",
        });
    model
        .right_stack
        .set_visible_child_name(match model.right_tab {
            RightSidebarTab::Highlights => "highlights",
            RightSidebarTab::Bookmarks => "bookmarks",
            RightSidebarTab::Words => "words",
        });
    model
        .settings_stack
        .set_visible_child_name(reader_settings_pane_name(model.settings_pane));
}

pub(crate) fn sync_reader_controls(model: &ReaderModel) {
    model.font_size_label.set_label(&model.font_px.to_string());
    model
        .line_height_label
        .set_label(&format!("{:.1}", model.line_height));
    model
        .column_width_label
        .set_label(&model.column_px.to_string());
    // A spread is a paged-mode thing: the toggle stays on record but does
    // nothing under continuous scroll, so the row greys out instead of
    // pretending (2.7, first field report).
    model
        .dual_page_switch
        .set_sensitive(!model.scrolled);
    // Arrow scrolling is active only in continuous scroll mode; in paged
    // mode Up/Down step chapters, so the scroll speed setting greys out.
    model
        .arrow_step_row
        .set_sensitive(model.scrolled);
    model
        .wheel_step_label
        .set_label(&model.wheel_step.to_string());
    model
        .arrow_step_label
        .set_label(&model.arrow_step.to_string());

    let bindings = model.keybinds.borrow();
    for (action, button) in &model.keybind_buttons {
        button.set_label(&bindings.binding(*action).label());
    }
    drop(bindings);

    for (pane, btn) in &model.settings_pane_buttons {
        toggle_active(btn, *pane == model.settings_pane);
    }
    for (filter, btn) in &model.highlight_filter_buttons {
        toggle_active(btn, *filter == model.highlight_filter);
    }
    for (scope, btn) in &model.word_scope_buttons {
        toggle_active(btn, *scope == model.word_scope);
    }
}

pub(crate) fn sync_sidebar_tabs(widgets: &super::ReaderModelWidgets, model: &ReaderModel) {
    toggle_active(&widgets.left_toc_tab, model.left_tab == LeftSidebarTab::Toc);
    toggle_active(
        &widgets.left_settings_tab,
        model.left_tab == LeftSidebarTab::Settings,
    );
    toggle_active(
        &widgets.right_highlights_tab,
        model.right_tab == RightSidebarTab::Highlights,
    );
    toggle_active(
        &widgets.right_bookmarks_tab,
        model.right_tab == RightSidebarTab::Bookmarks,
    );
    toggle_active(
        &widgets.right_words_tab,
        model.right_tab == RightSidebarTab::Words,
    );
}

pub(crate) fn toggle_active(widget: &impl IsA<gtk::Widget>, active: bool) {
    if active {
        widget.add_css_class("active");
    } else {
        widget.remove_css_class("active");
    }
}

pub(crate) fn rebuild_cover_host(host: &gtk::Box, cover_path: Option<&std::path::Path>) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }
    host.append(&cover_widget(cover_path, 48, 70));
}

pub(crate) fn update_sidebar_header(
    widgets: &super::ReaderModelWidgets,
    model: &ReaderModel,
    sender: &ComponentSender<ReaderModel>,
) {
    widgets.sidebar_book_title.set_label(&model.book_title);
    let tx = sender.input_sender().clone();
    crate::widgets::author_links::replace_author_links(
        &widgets.sidebar_book_author,
        &model.book_authors,
        "kalam-author-link-reader",
        std::rc::Rc::new(move |name| {
            let _ = tx.send(ReaderMsg::OpenAuthor(name));
        }),
    );
    widgets
        .sidebar_progress
        .set_fraction(model.progress_pct() as f64 / 100.0);
}

/// The bottom-right location text (decision 17): `NN%` at rest; on
/// hover it grows leftwards into `Chapter title · NN%` and brightens —
/// the one sighting of the chapter's name on the page.
pub(crate) fn update_chrome_labels(widgets: &super::ReaderModelWidgets, model: &ReaderModel) {
    let label = &widgets.location_label;
    if model.location_hover {
        label.add_css_class("hover");
    } else {
        label.remove_css_class("hover");
    }
    if model.chapter_count == 0 {
        label.set_label("—");
        return;
    }
    if model.location_hover {
        label.set_label(&format!(
            "{} · {}%",
            model.current_toc_title(),
            model.progress_pct()
        ));
    } else {
        label.set_label(&format!("{}%", model.progress_pct()));
    }
}

/// The bookmark ribbon (decision 16): outline when unmarked, accent-
/// filled when a mark sits in this chapter near this spot.
pub(crate) fn sync_ribbon(widgets: &super::ReaderModelWidgets, model: &ReaderModel) {
    let set = model.bookmark_here();
    widgets.ribbon_image.set_icon_name(Some(if set {
        "kalam-ribbon-filled-symbolic"
    } else {
        "kalam-ribbon-outline-symbolic"
    }));
    if set {
        widgets.ribbon_button.add_css_class("set");
    } else {
        widgets.ribbon_button.remove_css_class("set");
    }
    widgets.ribbon_button.set_tooltip_text(Some(if set {
        "Remove the bookmark here (B)"
    } else {
        "Bookmark this page (B)"
    }));
}

pub(crate) fn reader_sidebar_tab_content(icon: &str, label: &str) -> gtk::Box {
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 3);
    box_.set_halign(gtk::Align::Center);
    box_.set_hexpand(true);
    box_.append(&crate::icons::symbolic_with_classes(
        icon,
        17,
        &["kalam-inline-icon"],
    ));
    let label_widget = gtk::Label::new(Some(label));
    label_widget.set_halign(gtk::Align::Center);
    label_widget.set_width_chars(1);
    label_widget.set_max_width_chars(8);
    label_widget.set_ellipsize(gtk::pango::EllipsizeMode::End);
    box_.append(&label_widget);
    box_
}

pub(crate) fn overlay_child_box(overlay: &gtk::Overlay, index: usize) -> Option<gtk::Box> {
    let mut child = overlay.first_child()?;
    for _ in 0..index {
        child = child.next_sibling()?;
    }
    if let Ok(b) = child.clone().downcast::<gtk::Box>() {
        Some(b)
    } else if let Ok(r) = child.downcast::<gtk::Revealer>() {
        r.child().and_then(|w| w.downcast::<gtk::Box>().ok())
    } else {
        None
    }
}

/// Hover opens panels (decision 20 keeps the edge summon); closing is
/// deliberate now — Escape, a click on the page, the summon key again —
/// so there is no leave-to-close timer to wire any more.
pub(crate) fn connect_hover_open(
    widget: &impl IsA<gtk::Widget>,
    sender: &ComponentSender<ReaderModel>,
    open_msg: ReaderMsg,
) {
    let motion = gtk::EventControllerMotion::new();
    let tx = sender.input_sender().clone();
    motion.connect_enter(move |_, _, _| {
        let _ = tx.send(open_msg.clone());
    });
    widget.add_controller(motion);
}
