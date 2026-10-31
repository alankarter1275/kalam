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
            LeftSidebarTab::Contents => "contents",
            LeftSidebarTab::Notes => "notes",
        });
    model
        .settings_stack
        .set_visible_child_name(reader_settings_pane_name(model.settings_pane));
}

/// The Reading pane's preview card: the current page's own words set
/// in the standing choices (spec §3). Same Pango rendering as the
/// Settings preview, in the panel's narrower column.
pub(crate) fn refresh_reading_preview(model: &ReaderModel) {
    let family = model
        .font_family
        .clone()
        .unwrap_or_else(|| kalam_reader::BODY_FONT.to_string());
    model.preview_body.set_markup(&format!(
        "<span font_family=\"{}\" size=\"{}\" line_height=\"{:.1}\">{}</span>",
        gtk::glib::markup_escape_text(&family),
        model.font_px * 750,
        model.line_height,
        gtk::glib::markup_escape_text(&model.preview_text),
    ));
    model.preview_body.set_justify(if model.justify {
        gtk::Justification::Fill
    } else {
        gtk::Justification::Left
    });
    // The card's text column, capped the way a narrow window caps
    // the book: 340 panel − panel padding − card padding.
    let width = (model.column_px as i32).min(276);
    model
        .preview_body
        .set_max_width_chars(((width * 2) / (model.font_px as i32).max(1)).clamp(20, 120));
    model.preview_column.set_size_request(width, -1);
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
    model.dual_page_switch.set_sensitive(!model.scrolled);
    // Arrow scrolling is active only in continuous scroll mode; in paged
    // mode Up/Down step chapters, so the scroll speed setting greys out.
    model.arrow_step_row.set_sensitive(model.scrolled);
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
    for (filter, btn) in &model.notes_filter_buttons {
        toggle_active(btn, *filter == model.notes_filter);
    }
    model
        .notes_highlights_controls
        .set_visible(model.notes_filter == NotesFilter::Highlights);
    model
        .notes_bookmarks_controls
        .set_visible(model.notes_filter == NotesFilter::Bookmarks);
    model
        .notes_words_controls
        .set_visible(model.notes_filter == NotesFilter::Words);

    // The per-book override (decision 147): the switch, the book's
    // theme choice, and whether anything differs from the defaults
    // for the pill to copy over.
    (model.set_book_override_switch)(model.book_override.is_some());
    (model.set_book_theme_choice)(model.theme_override.as_deref());
    let differs = model.settings_differ_from_defaults();
    model.use_defaults_btn.set_sensitive(differs);
    model.use_defaults_btn.set_label(if differs {
        "Use these settings as my defaults"
    } else {
        "Matches your defaults"
    });
    refresh_reading_preview(model);
}

pub(crate) fn sync_sidebar_tabs(widgets: &super::ReaderModelWidgets, model: &ReaderModel) {
    toggle_active(
        &widgets.left_contents_tab,
        model.left_tab == LeftSidebarTab::Contents,
    );
    toggle_active(
        &widgets.left_notes_tab,
        model.left_tab == LeftSidebarTab::Notes,
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
    // No progress bar in the head (decision 20): the percentage
    // already lives on the page.
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
