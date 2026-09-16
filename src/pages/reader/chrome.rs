//! Shell synchronizers, hover zone controllers, labels, tabs, and cover host updates.

use super::mod_model::ReaderModel;
use super::types::*;
use super::ui_prefs::{reader_settings_pane_name, reader_ui_value_text};
use crate::epub_book::ReadingTheme;
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

    for (pane, btn) in &model.settings_pane_buttons {
        toggle_active(btn, *pane == model.settings_pane);
    }
    for (theme, btn) in &model.theme_dots {
        toggle_active(btn, *theme == model.theme);
    }
    for (setting, controls) in &model.ui_controls {
        let value = model.ui_prefs.get(*setting);
        *controls.last_value.borrow_mut() = value;
        controls
            .value_entry
            .set_tooltip_text(Some(&reader_ui_value_text(*setting, value)));
        let desired_text = value.to_string();
        if !controls.value_entry.has_focus() && controls.value_entry.text().as_str() != desired_text
        {
            controls.value_entry.set_text(&desired_text);
        }
        let mut matched_preset = false;
        for (preset_value, btn) in &controls.preset_buttons {
            let active = *preset_value == value;
            toggle_active(btn, active);
            matched_preset |= active;
        }
        toggle_active(&controls.custom_button, !matched_preset);
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

pub(crate) fn update_chrome_labels(widgets: &super::ReaderModelWidgets, model: &ReaderModel) {
    if model.chapter_count == 0 {
        widgets.progress_label.set_label("—");
        widgets.pill_chapter_label.set_label(&model.book_title);
        return;
    }
    widgets.progress_label.set_label(&format!(
        "{} / {}",
        model.chapter + 1,
        model.chapter_count
    ));
    widgets
        .pill_chapter_label
        .set_label(model.current_chapter_title());
}

pub(crate) fn sync_reader_stage_theme(stage: &gtk::Box, theme: ReadingTheme) {
    for class_name in [
        "kalam-reader-paper-light",
        "kalam-reader-paper-sepia",
        "kalam-reader-paper-dark",
        "kalam-reader-paper-ink",
    ] {
        stage.remove_css_class(class_name);
    }
    stage.add_css_class(match theme {
        ReadingTheme::Light => "kalam-reader-paper-light",
        ReadingTheme::Sepia => "kalam-reader-paper-sepia",
        ReadingTheme::Dark => "kalam-reader-paper-dark",
        ReadingTheme::Ink => "kalam-reader-paper-ink",
    });
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
    child.downcast::<gtk::Box>().ok()
}

pub(crate) fn connect_hover_zone(
    widget: &impl IsA<gtk::Widget>,
    sender: &ComponentSender<ReaderModel>,
    open_msg: ReaderMsg,
    close_msg: ReaderMsg,
) {
    let motion = gtk::EventControllerMotion::new();
    let tx = sender.input_sender().clone();
    motion.connect_enter(move |_, _, _| {
        let _ = tx.send(open_msg.clone());
    });
    let tx = sender.input_sender().clone();
    motion.connect_leave(move |_| {
        let _ = tx.send(close_msg.clone());
    });
    widget.add_controller(motion);
}
