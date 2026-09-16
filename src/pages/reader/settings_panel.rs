//! Left sidebar Settings panel builder and UI setting stepper rows.

use super::mod_model::ReaderModel;
use super::types::*;
use super::ui_prefs::{
    parse_reader_ui_input, reader_settings_pane_name, reader_ui_presets,
    reader_ui_step, reader_ui_unit,
};
use crate::epub_book::ReadingTheme;
use gtk::prelude::*;
use relm4::prelude::*;
use relm4::RelmWidgetExt;
use std::cell::RefCell;
use std::rc::Rc;

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_reader_settings_panel(
    sender: &ComponentSender<ReaderModel>,
    theme: ReadingTheme,
    font_px: u32,
    line_height: f32,
    column_px: u32,
    scrolled: bool,
    ui_prefs: ReaderUiPrefs,
    dict_sense_hint: bool,
    dict_history_enabled: bool,
) -> ReaderSettingsControls {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let switcher = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    switcher.add_css_class("kalam-reader-settings-switcher");
    switcher.set_margin_all(12);
    switcher.set_homogeneous(true);
    let mut pane_buttons = Vec::new();
    for (label, pane) in [
        ("Reading", ReaderSettingsPane::Reading),
        ("UI", ReaderSettingsPane::Ui),
    ] {
        let btn = gtk::Button::with_label(label);
        btn.add_css_class("kalam-reader-settings-switch");
        let tx = sender.input_sender().clone();
        btn.connect_clicked(move |_| {
            let _ = tx.send(ReaderMsg::SwitchSettingsPane(pane));
        });
        switcher.append(&btn);
        pane_buttons.push((pane, btn));
    }
    wrap.append(&switcher);

    let stack = gtk::Stack::new();
    stack.set_hexpand(true);
    stack.set_vexpand(true);
    stack.set_transition_type(gtk::StackTransitionType::Crossfade);

    let reading_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let theme_section = reader_settings_section("Reading theme");
    let dots_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let mut dots = Vec::new();
    for (label, value, class_name) in [
        ("Sepia", ReadingTheme::Sepia, "kalam-reader-theme-dot-sepia"),
        ("Light", ReadingTheme::Light, "kalam-reader-theme-dot-light"),
        ("Dark", ReadingTheme::Dark, "kalam-reader-theme-dot-dark"),
        ("Ink", ReadingTheme::Ink, "kalam-reader-theme-dot-ink"),
    ] {
        let btn = gtk::Button::new();
        btn.add_css_class("kalam-reader-theme-dot");
        btn.add_css_class(class_name);
        btn.set_tooltip_text(Some(label));
        let tx = sender.input_sender().clone();
        btn.connect_clicked(move |_| {
            let _ = tx.send(ReaderMsg::Theme(value));
        });
        dots_row.append(&btn);
        dots.push((value, btn));
    }
    theme_section.append(&dots_row);
    reading_page.append(&theme_section);
    reading_page.append(&reader_panel_divider());

    let type_section = reader_settings_section("Type");
    let font_size_label = gtk::Label::new(Some(&font_px.to_string()));
    font_size_label.add_css_class("kalam-reader-stepper-value");
    type_section.append(&reader_stepper_row(
        "Font size",
        &font_size_label,
        sender,
        ReaderMsg::FontDelta(-1),
        ReaderMsg::FontDelta(1),
    ));
    let line_height_label = gtk::Label::new(Some(&format!("{line_height:.1}")));
    line_height_label.add_css_class("kalam-reader-stepper-value");
    type_section.append(&reader_stepper_row(
        "Line height",
        &line_height_label,
        sender,
        ReaderMsg::LineHeightDelta(-1),
        ReaderMsg::LineHeightDelta(1),
    ));
    reading_page.append(&type_section);
    reading_page.append(&reader_panel_divider());

    let width_section = reader_settings_section("Column width");
    let column_width_label = gtk::Label::new(Some(&column_px.to_string()));
    column_width_label.add_css_class("kalam-reader-stepper-value");
    width_section.append(&reader_stepper_row(
        "Width",
        &column_width_label,
        sender,
        ReaderMsg::ColumnWidthDelta(-20),
        ReaderMsg::ColumnWidthDelta(20),
    ));
    reading_page.append(&width_section);
    reading_page.append(&reader_panel_divider());

    // "Layout": one page at a time, or one long strip. Same control the
    // `reader.scrolled` preference holds, so the key and the switch agree.
    let mode_section = reader_settings_section("Layout");
    let mode_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    mode_row.add_css_class("kalam-reader-setting-row");
    let mode_label = gtk::Label::new(Some("Continuous scroll"));
    mode_label.add_css_class("kalam-reader-setting-name");
    mode_label.set_hexpand(true);
    mode_label.set_halign(gtk::Align::Start);
    mode_row.append(&mode_label);
    let mode_tx = sender.input_sender().clone();
    let mode_switch = crate::pages::settings::toggle_switch(scrolled, move |on| {
        let _ = mode_tx.send(ReaderMsg::SetScrolled(on));
    });
    mode_row.append(&mode_switch);
    mode_section.append(&mode_row);
    reading_page.append(&mode_section);
    reading_page.append(&reader_panel_divider());

    let dict_section = reader_settings_section("Dictionary");
    let hint_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    hint_row.add_css_class("kalam-reader-setting-row");
    let hint_label = gtk::Label::new(Some("Sense hint"));
    hint_label.add_css_class("kalam-reader-setting-name");
    hint_label.set_hexpand(true);
    hint_label.set_halign(gtk::Align::Start);
    hint_row.append(&hint_label);
    let hint_tx = sender.input_sender().clone();
    let hint_switch = crate::pages::settings::toggle_switch(dict_sense_hint, move |on| {
        let _ = hint_tx.send(ReaderMsg::SetDictSenseHint(on));
    });
    hint_row.append(&hint_switch);
    dict_section.append(&hint_row);

    let history_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    history_row.add_css_class("kalam-reader-setting-row");
    let history_label = gtk::Label::new(Some("Lookup history"));
    history_label.add_css_class("kalam-reader-setting-name");
    history_label.set_hexpand(true);
    history_label.set_halign(gtk::Align::Start);
    history_row.append(&history_label);
    let history_tx = sender.input_sender().clone();
    let history_switch = crate::pages::settings::toggle_switch(dict_history_enabled, move |on| {
        let _ = history_tx.send(ReaderMsg::SetDictHistory(on));
    });
    history_row.append(&history_switch);
    dict_section.append(&history_row);
    reading_page.append(&dict_section);
    stack.add_named(
        &reading_page,
        Some(reader_settings_pane_name(ReaderSettingsPane::Reading)),
    );

    let ui_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let mut ui_controls = Vec::new();

    let sidebar_section = reader_settings_section("Sidebars");
    for (title, setting) in [
        ("Edge gap", ReaderUiSetting::SidebarGap),
        ("Left width", ReaderUiSetting::LeftSidebarWidth),
        ("Right width", ReaderUiSetting::RightSidebarWidth),
        ("Corner radius", ReaderUiSetting::SidebarRadius),
        ("Inner padding", ReaderUiSetting::SidebarPadding),
        ("Shadow depth", ReaderUiSetting::SidebarShadow),
    ] {
        let (row, controls) =
            reader_ui_setting_block(title, setting, ui_prefs.get(setting), sender);
        sidebar_section.append(&row);
        ui_controls.push((setting, controls));
    }
    ui_page.append(&sidebar_section);
    ui_page.append(&reader_panel_divider());

    let controls_section = reader_settings_section("Floating controls");
    for (title, setting) in [
        ("Bottom pill size", ReaderUiSetting::BottomPillSize),
        ("Bottom gap", ReaderUiSetting::BottomPillGap),
        ("Back chip size", ReaderUiSetting::BackChipSize),
        ("Back top gap", ReaderUiSetting::BackTopGap),
        ("Back side gap", ReaderUiSetting::BackSideGap),
    ] {
        let (row, controls) =
            reader_ui_setting_block(title, setting, ui_prefs.get(setting), sender);
        controls_section.append(&row);
        ui_controls.push((setting, controls));
    }
    ui_page.append(&controls_section);
    ui_page.append(&reader_panel_divider());

    let overlay_section = reader_settings_section("Overlay");
    let (dim_row, dim_controls) = reader_ui_setting_block(
        "Background dim",
        ReaderUiSetting::DimStrength,
        ui_prefs.get(ReaderUiSetting::DimStrength),
        sender,
    );
    overlay_section.append(&dim_row);
    ui_controls.push((ReaderUiSetting::DimStrength, dim_controls));
    ui_page.append(&overlay_section);
    stack.add_named(
        &ui_page,
        Some(reader_settings_pane_name(ReaderSettingsPane::Ui)),
    );

    if theme == ReadingTheme::Sepia {
        for (_, dot) in &dots {
            dot.remove_css_class("active");
        }
    }

    wrap.append(&stack);

    ReaderSettingsControls {
        root: wrap,
        settings_stack: stack,
        pane_buttons,
        font_size_label,
        line_height_label,
        column_width_label,
        theme_dots: dots,
        ui_controls,
    }
}

pub(crate) fn reader_ui_setting_block(
    label: &str,
    setting: ReaderUiSetting,
    value: i32,
    sender: &ComponentSender<ReaderModel>,
) -> (gtk::Box, ReaderUiSettingControls) {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 8);
    wrap.add_css_class("kalam-reader-ui-setting");

    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    head.add_css_class("kalam-reader-ui-setting-head");
    let label_widget = gtk::Label::new(Some(label));
    label_widget.add_css_class("kalam-reader-ui-setting-label");
    label_widget.set_hexpand(true);
    label_widget.set_halign(gtk::Align::Start);
    head.append(&label_widget);
    wrap.append(&head);

    let presets = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    presets.add_css_class("kalam-reader-ui-preset-row");
    presets.set_homogeneous(true);
    let preset_size_group = gtk::SizeGroup::new(gtk::SizeGroupMode::Horizontal);
    let mut preset_buttons = Vec::new();
    for &(preset_label, preset_value) in reader_ui_presets(setting) {
        let btn = gtk::Button::with_label(preset_label);
        btn.add_css_class("kalam-reader-filter-chip");
        btn.add_css_class("kalam-reader-ui-preset");
        preset_size_group.add_widget(&btn);
        let tx = sender.input_sender().clone();
        btn.connect_clicked(move |_| {
            let _ = tx.send(ReaderMsg::SetUiSetting(setting, preset_value));
        });
        presets.append(&btn);
        preset_buttons.push((preset_value, btn));
    }
    wrap.append(&presets);

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    controls.add_css_class("kalam-reader-ui-control-row");

    let value_entry = gtk::Entry::new();
    value_entry.add_css_class("kalam-reader-ui-value-entry");
    value_entry.set_input_purpose(gtk::InputPurpose::Digits);
    value_entry.set_width_chars(4);
    value_entry.set_max_length(4);
    value_entry.set_text(&value.to_string());
    let last_value = Rc::new(RefCell::new(value));

    let custom_button = gtk::Button::with_label("Custom");
    custom_button.add_css_class("kalam-reader-filter-chip");
    custom_button.add_css_class("kalam-reader-ui-preset");
    custom_button.add_css_class("kalam-reader-ui-preset-custom");
    custom_button.set_focus_on_click(false);
    preset_size_group.add_widget(&custom_button);
    let value_entry_for_focus = value_entry.clone();
    custom_button.connect_clicked(move |_| {
        value_entry_for_focus.grab_focus();
    });
    controls.append(&custom_button);

    let minus = gtk::Button::new();
    minus.add_css_class("kalam-reader-stepper-btn");
    minus.set_size_request(30, 30);
    minus.set_halign(gtk::Align::Center);
    minus.set_valign(gtk::Align::Center);
    minus.set_child(Some(&crate::icons::symbolic_with_classes(
        "list-remove-symbolic",
        14,
        &["kalam-inline-icon"],
    )));
    let tx = sender.input_sender().clone();
    minus.connect_clicked(move |_| {
        let _ = tx.send(ReaderMsg::AdjustUiSetting(
            setting,
            -reader_ui_step(setting),
        ));
    });
    controls.append(&minus);

    let value_inline = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    value_inline.append(&value_entry);
    let unit_label = gtk::Label::new(Some(reader_ui_unit(setting)));
    unit_label.add_css_class("kalam-reader-ui-unit");
    value_inline.append(&unit_label);
    controls.append(&value_inline);

    let plus = gtk::Button::new();
    plus.add_css_class("kalam-reader-stepper-btn");
    plus.set_size_request(30, 30);
    plus.set_halign(gtk::Align::Center);
    plus.set_valign(gtk::Align::Center);
    plus.set_child(Some(&crate::icons::symbolic_with_classes(
        "list-add-symbolic",
        14,
        &["kalam-inline-icon"],
    )));
    let tx = sender.input_sender().clone();
    plus.connect_clicked(move |_| {
        let _ = tx.send(ReaderMsg::AdjustUiSetting(setting, reader_ui_step(setting)));
    });
    controls.append(&plus);
    wrap.append(&controls);

    let tx = sender.input_sender().clone();
    let last_value_for_activate = last_value.clone();
    value_entry.connect_activate(move |entry| {
        if let Some(next) = parse_reader_ui_input(entry.text().as_str()) {
            let _ = tx.send(ReaderMsg::SetUiSetting(setting, next));
        } else {
            entry.set_text(&last_value_for_activate.borrow().to_string());
        }
    });

    let tx = sender.input_sender().clone();
    let last_value_for_focus = last_value.clone();
    value_entry.connect_has_focus_notify(move |entry| {
        if entry.has_focus() {
            return;
        }
        if let Some(next) = parse_reader_ui_input(entry.text().as_str()) {
            let _ = tx.send(ReaderMsg::SetUiSetting(setting, next));
        } else {
            entry.set_text(&last_value_for_focus.borrow().to_string());
        }
    });

    (
        wrap,
        ReaderUiSettingControls {
            value_entry,
            last_value,
            preset_buttons,
            custom_button,
        },
    )
}

pub(crate) fn reader_settings_section(label: &str) -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 10);
    section.add_css_class("kalam-reader-section");
    let heading = gtk::Label::new(Some(label));
    heading.add_css_class("kalam-reader-section-label");
    heading.set_halign(gtk::Align::Start);
    section.append(&heading);
    section
}

pub(crate) fn reader_stepper_row(
    label: &str,
    value_label: &gtk::Label,
    sender: &ComponentSender<ReaderModel>,
    minus_msg: ReaderMsg,
    plus_msg: ReaderMsg,
) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("kalam-reader-setting-row");

    let label_widget = gtk::Label::new(Some(label));
    label_widget.add_css_class("kalam-reader-setting-name");
    label_widget.set_hexpand(true);
    label_widget.set_halign(gtk::Align::Start);
    row.append(&label_widget);

    let stepper = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    stepper.add_css_class("kalam-reader-stepper");

    let minus = gtk::Button::new();
    minus.add_css_class("kalam-reader-stepper-btn");
    minus.set_size_request(30, 30);
    minus.set_halign(gtk::Align::Center);
    minus.set_valign(gtk::Align::Center);
    minus.set_child(Some(&crate::icons::symbolic_with_classes(
        "list-remove-symbolic",
        14,
        &["kalam-inline-icon"],
    )));
    let s = sender.clone();
    minus.connect_clicked(move |_| s.input(minus_msg.clone()));
    stepper.append(&minus);

    stepper.append(value_label);

    let plus = gtk::Button::new();
    plus.add_css_class("kalam-reader-stepper-btn");
    plus.set_size_request(30, 30);
    plus.set_halign(gtk::Align::Center);
    plus.set_valign(gtk::Align::Center);
    plus.set_child(Some(&crate::icons::symbolic_with_classes(
        "list-add-symbolic",
        14,
        &["kalam-inline-icon"],
    )));
    let s = sender.clone();
    plus.connect_clicked(move |_| s.input(plus_msg.clone()));
    stepper.append(&plus);

    row.append(&stepper);
    row
}

pub(crate) fn reader_panel_divider() -> gtk::Separator {
    let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
    sep.add_css_class("kalam-reader-panel-divider");
    sep
}
