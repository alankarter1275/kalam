//! Reader UI preferences, dynamic CSS generation, and GTK style provider registration.

use super::types::*;
use crate::db::Catalog;
use crate::pages::reader::ReaderModel;
use gtk::prelude::*;

impl ReaderUiPrefs {
    pub(crate) fn load(catalog: &Catalog) -> Self {
        let mut prefs = Self {
            sidebar_gap: reader_ui_default(ReaderUiSetting::SidebarGap),
            left_sidebar_width: reader_ui_default(ReaderUiSetting::LeftSidebarWidth),
            right_sidebar_width: reader_ui_default(ReaderUiSetting::RightSidebarWidth),
            sidebar_radius: reader_ui_default(ReaderUiSetting::SidebarRadius),
            sidebar_padding: reader_ui_default(ReaderUiSetting::SidebarPadding),
            sidebar_shadow: reader_ui_default(ReaderUiSetting::SidebarShadow),
            bottom_pill_size: reader_ui_default(ReaderUiSetting::BottomPillSize),
            bottom_pill_gap: reader_ui_default(ReaderUiSetting::BottomPillGap),
            back_chip_size: reader_ui_default(ReaderUiSetting::BackChipSize),
            back_top_gap: reader_ui_default(ReaderUiSetting::BackTopGap),
            back_side_gap: reader_ui_default(ReaderUiSetting::BackSideGap),
            dim_strength: reader_ui_default(ReaderUiSetting::DimStrength),
        };
        for setting in [
            ReaderUiSetting::SidebarGap,
            ReaderUiSetting::LeftSidebarWidth,
            ReaderUiSetting::RightSidebarWidth,
            ReaderUiSetting::SidebarRadius,
            ReaderUiSetting::SidebarPadding,
            ReaderUiSetting::SidebarShadow,
            ReaderUiSetting::BottomPillSize,
            ReaderUiSetting::BottomPillGap,
            ReaderUiSetting::BackChipSize,
            ReaderUiSetting::BackTopGap,
            ReaderUiSetting::BackSideGap,
            ReaderUiSetting::DimStrength,
        ] {
            let saved =
                catalog.get_pref_i64(reader_ui_pref_key(setting), prefs.get(setting) as i64) as i32;
            let _ = prefs.set(setting, saved);
        }
        prefs
    }

    pub(crate) fn get(&self, setting: ReaderUiSetting) -> i32 {
        match setting {
            ReaderUiSetting::SidebarGap => self.sidebar_gap,
            ReaderUiSetting::LeftSidebarWidth => self.left_sidebar_width,
            ReaderUiSetting::RightSidebarWidth => self.right_sidebar_width,
            ReaderUiSetting::SidebarRadius => self.sidebar_radius,
            ReaderUiSetting::SidebarPadding => self.sidebar_padding,
            ReaderUiSetting::SidebarShadow => self.sidebar_shadow,
            ReaderUiSetting::BottomPillSize => self.bottom_pill_size,
            ReaderUiSetting::BottomPillGap => self.bottom_pill_gap,
            ReaderUiSetting::BackChipSize => self.back_chip_size,
            ReaderUiSetting::BackTopGap => self.back_top_gap,
            ReaderUiSetting::BackSideGap => self.back_side_gap,
            ReaderUiSetting::DimStrength => self.dim_strength,
        }
    }

    pub(crate) fn set(&mut self, setting: ReaderUiSetting, value: i32) -> bool {
        let next = clamp_reader_ui_value(setting, value);
        let slot = match setting {
            ReaderUiSetting::SidebarGap => &mut self.sidebar_gap,
            ReaderUiSetting::LeftSidebarWidth => &mut self.left_sidebar_width,
            ReaderUiSetting::RightSidebarWidth => &mut self.right_sidebar_width,
            ReaderUiSetting::SidebarRadius => &mut self.sidebar_radius,
            ReaderUiSetting::SidebarPadding => &mut self.sidebar_padding,
            ReaderUiSetting::SidebarShadow => &mut self.sidebar_shadow,
            ReaderUiSetting::BottomPillSize => &mut self.bottom_pill_size,
            ReaderUiSetting::BottomPillGap => &mut self.bottom_pill_gap,
            ReaderUiSetting::BackChipSize => &mut self.back_chip_size,
            ReaderUiSetting::BackTopGap => &mut self.back_top_gap,
            ReaderUiSetting::BackSideGap => &mut self.back_side_gap,
            ReaderUiSetting::DimStrength => &mut self.dim_strength,
        };
        if *slot == next {
            return false;
        }
        *slot = next;
        true
    }
}

pub(crate) fn reader_settings_pane_name(pane: ReaderSettingsPane) -> &'static str {
    match pane {
        ReaderSettingsPane::Reading => "reading",
        ReaderSettingsPane::Ui => "ui",
        ReaderSettingsPane::Shortcuts => "shortcuts",
    }
}

pub(crate) fn reader_ui_pref_key(setting: ReaderUiSetting) -> &'static str {
    match setting {
        ReaderUiSetting::SidebarGap => "reader.ui.sidebar_gap_px",
        ReaderUiSetting::LeftSidebarWidth => "reader.ui.left_sidebar_width_px",
        ReaderUiSetting::RightSidebarWidth => "reader.ui.right_sidebar_width_px",
        ReaderUiSetting::SidebarRadius => "reader.ui.sidebar_radius_px",
        ReaderUiSetting::SidebarPadding => "reader.ui.sidebar_padding_px",
        ReaderUiSetting::SidebarShadow => "reader.ui.sidebar_shadow_px",
        ReaderUiSetting::BottomPillSize => "reader.ui.bottom_pill_size_px",
        ReaderUiSetting::BottomPillGap => "reader.ui.bottom_pill_gap_px",
        ReaderUiSetting::BackChipSize => "reader.ui.back_chip_size_px",
        ReaderUiSetting::BackTopGap => "reader.ui.back_top_gap_px",
        ReaderUiSetting::BackSideGap => "reader.ui.back_side_gap_px",
        ReaderUiSetting::DimStrength => "reader.ui.dim_strength_pct",
    }
}

pub(crate) fn reader_ui_default(setting: ReaderUiSetting) -> i32 {
    match setting {
        ReaderUiSetting::SidebarGap => 8,
        ReaderUiSetting::LeftSidebarWidth => 248,
        ReaderUiSetting::RightSidebarWidth => 212,
        ReaderUiSetting::SidebarRadius => 20,
        ReaderUiSetting::SidebarPadding => 0,
        ReaderUiSetting::SidebarShadow => 22,
        ReaderUiSetting::BottomPillSize => 34,
        ReaderUiSetting::BottomPillGap => 18,
        ReaderUiSetting::BackChipSize => 32,
        ReaderUiSetting::BackTopGap => 14,
        ReaderUiSetting::BackSideGap => 16,
        ReaderUiSetting::DimStrength => 22,
    }
}

pub(crate) fn reader_ui_presets(setting: ReaderUiSetting) -> &'static [(&'static str, i32)] {
    match setting {
        ReaderUiSetting::SidebarGap => &UI_PRESETS_SIDEBAR_GAP,
        ReaderUiSetting::LeftSidebarWidth => &UI_PRESETS_LEFT_WIDTH,
        ReaderUiSetting::RightSidebarWidth => &UI_PRESETS_RIGHT_WIDTH,
        ReaderUiSetting::SidebarRadius => &UI_PRESETS_RADIUS,
        ReaderUiSetting::SidebarPadding => &UI_PRESETS_PADDING,
        ReaderUiSetting::SidebarShadow => &UI_PRESETS_SHADOW,
        ReaderUiSetting::BottomPillSize => &UI_PRESETS_PILL_SIZE,
        ReaderUiSetting::BottomPillGap => &UI_PRESETS_PILL_GAP,
        ReaderUiSetting::BackChipSize => &UI_PRESETS_BACK_SIZE,
        ReaderUiSetting::BackTopGap => &UI_PRESETS_BACK_TOP,
        ReaderUiSetting::BackSideGap => &UI_PRESETS_BACK_SIDE,
        ReaderUiSetting::DimStrength => &UI_PRESETS_DIM,
    }
}

pub(crate) fn clamp_reader_ui_value(setting: ReaderUiSetting, value: i32) -> i32 {
    match setting {
        ReaderUiSetting::SidebarGap => value.clamp(0, 36),
        ReaderUiSetting::LeftSidebarWidth => value.clamp(180, 340),
        ReaderUiSetting::RightSidebarWidth => value.clamp(160, 320),
        ReaderUiSetting::SidebarRadius => value.clamp(0, 36),
        ReaderUiSetting::SidebarPadding => value.clamp(0, 18),
        ReaderUiSetting::SidebarShadow => value.clamp(8, 40),
        ReaderUiSetting::BottomPillSize => value.clamp(26, 52),
        ReaderUiSetting::BottomPillGap => value.clamp(0, 40),
        ReaderUiSetting::BackChipSize => value.clamp(24, 48),
        ReaderUiSetting::BackTopGap => value.clamp(0, 40),
        ReaderUiSetting::BackSideGap => value.clamp(0, 40),
        ReaderUiSetting::DimStrength => value.clamp(0, 50),
    }
}

pub(crate) fn reader_ui_step(setting: ReaderUiSetting) -> i32 {
    match setting {
        ReaderUiSetting::SidebarGap => 2,
        ReaderUiSetting::LeftSidebarWidth => 4,
        ReaderUiSetting::RightSidebarWidth => 4,
        ReaderUiSetting::SidebarRadius => 2,
        ReaderUiSetting::SidebarPadding => 2,
        ReaderUiSetting::SidebarShadow => 2,
        ReaderUiSetting::BottomPillSize => 2,
        ReaderUiSetting::BottomPillGap => 2,
        ReaderUiSetting::BackChipSize => 2,
        ReaderUiSetting::BackTopGap => 2,
        ReaderUiSetting::BackSideGap => 2,
        ReaderUiSetting::DimStrength => 2,
    }
}

pub(crate) fn reader_ui_unit(setting: ReaderUiSetting) -> &'static str {
    match setting {
        ReaderUiSetting::DimStrength => "%",
        _ => "px",
    }
}

pub(crate) fn reader_ui_value_text(setting: ReaderUiSetting, value: i32) -> String {
    format!("{}{}", value, reader_ui_unit(setting))
}

pub(crate) fn parse_reader_ui_input(text: &str) -> Option<i32> {
    let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.is_empty() {
        None
    } else {
        digits.parse().ok()
    }
}

pub(crate) fn register_reader_ui_provider(provider: &gtk::CssProvider) {
    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

pub(crate) fn reader_ui_css(prefs: ReaderUiPrefs) -> String {
    let pad = prefs.sidebar_padding;
    let header_top = 22 + pad;
    let header_side = 18 + pad;
    let header_bottom = 16 + (pad / 2);
    let section_top = 14 + (pad / 2);
    let section_side = 16 + pad;
    let section_bottom = 8 + (pad / 2);
    let divider_side = 16 + pad;
    let tab_top = 10 + (pad / 2);
    let tab_side = 10 + (pad / 2);
    let tab_bottom = 12 + (pad / 2);
    let filter_top = 10 + (pad / 2);
    let filter_side = 12 + pad;
    let filter_bottom = 10 + (pad / 2);
    let toc_side = 16 + pad;
    let toc_top = 9 + (pad / 2);
    let list_side = 14 + pad;
    let list_top = 12 + (pad / 2);
    let shadow = prefs.sidebar_shadow;
    let shadow_y = (shadow / 2).clamp(6, 18);
    let shadow_blur = (shadow * 2 + 8).clamp(24, 88);
    let shadow_alpha = (0.20 + shadow as f32 / 100.0).clamp(0.22, 0.55);

    let pill = prefs.bottom_pill_size;
    let pill_pad_y = ((pill - 22) / 4).clamp(4, 8);
    let pill_pad_x = ((pill - 14) / 3).clamp(6, 12);
    let pill_info_pad = ((pill as f32 * 0.34).round() as i32).clamp(10, 18);
    let pill_icon = (pill / 2).clamp(14, 20);
    let pill_pages_font = ((pill as f32 * 0.44).round() as i32).clamp(12, 16);
    let pill_chapter_font = ((pill as f32 * 0.48).round() as i32).clamp(14, 18);

    let back = prefs.back_chip_size;
    let back_pad_y = ((back - 18) / 2).clamp(5, 10);
    let back_pad_left = (back_pad_y + 3).clamp(8, 14);
    let back_pad_right = (back_pad_y + 6).clamp(11, 17);
    let back_icon = (back / 2).clamp(14, 18);
    let dim_alpha = prefs.dim_strength as f32 / 100.0;

    format!(
        r#"
.kalam-reader-ui-live .kalam-reader-sidebar {{
    border-radius: {radius}px;
    box-shadow: 0 {shadow_y}px {shadow_blur}px alpha(#000, {shadow_alpha:.2});
}}

.kalam-reader-ui-live .kalam-reader-sidebar-left {{
    min-width: {left_width}px;
    border-radius: {radius}px;
}}

.kalam-reader-ui-live .kalam-reader-sidebar-right {{
    min-width: {right_width}px;
    border-radius: {radius}px;
}}

.kalam-reader-ui-live .kalam-reader-book-head {{
    padding: {header_top}px {header_side}px {header_bottom}px;
}}

.kalam-reader-ui-live .kalam-reader-section {{
    padding: {section_top}px {section_side}px {section_bottom}px;
}}

.kalam-reader-ui-live .kalam-reader-panel-divider {{
    margin-left: {divider_side}px;
    margin-right: {divider_side}px;
}}

.kalam-reader-ui-live .kalam-reader-tabbar {{
    padding: {tab_top}px {tab_side}px {tab_bottom}px;
}}

.kalam-reader-ui-live .kalam-reader-filter-row,
.kalam-reader-ui-live .kalam-reader-search-row {{
    padding: {filter_top}px {filter_side}px {filter_bottom}px;
}}

.kalam-reader-ui-live button.kalam-reader-toc-item {{
    padding: {toc_top}px {toc_side}px;
}}

.kalam-reader-ui-live .kalam-reader-annotation-wrap,
.kalam-reader-ui-live .kalam-reader-bookmark-row,
.kalam-reader-ui-live .kalam-reader-word-row {{
    padding: {list_top}px {list_side}px;
}}

.kalam-reader-ui-live button.kalam-reader-dim {{
    background: alpha(@kalam_bg, {dim_alpha:.2});
}}

.kalam-reader-ui-live .kalam-reader-bottom-pill {{
    padding: {pill_pad_y}px {pill_pad_x}px;
}}

.kalam-reader-ui-live button.kalam-reader-pill-nav {{
    min-width: {pill}px;
    min-height: {pill}px;
    padding: 0 {pill_pad_x}px;
}}

.kalam-reader-ui-live .kalam-reader-pill-nav image {{
    -gtk-icon-size: {pill_icon}px;
}}

.kalam-reader-ui-live .kalam-reader-pill-info {{
    padding: 0 {pill_info_pad}px;
}}

.kalam-reader-ui-live .kalam-reader-pill-pages {{
    font-size: {pill_pages_font}px;
}}

.kalam-reader-ui-live .kalam-reader-pill-chapter {{
    font-size: {pill_chapter_font}px;
}}

.kalam-reader-ui-live button.kalam-reader-back {{
    min-height: {back}px;
    padding: {back_pad_y}px {back_pad_right}px {back_pad_y}px {back_pad_left}px;
}}

.kalam-reader-ui-live .kalam-reader-back image {{
    -gtk-icon-size: {back_icon}px;
}}

.kalam-reader-ui-live .kalam-reader-annotation-card {{
    border: 1px solid transparent;
    border-radius: 10px;
}}

.kalam-reader-ui-live .kalam-reader-annotation-card-yellow {{
    background: alpha(#f4d35e, 0.08);
    border-color: alpha(#f4d35e, 0.16);
    border-left: 3px solid alpha(#f4d35e, 0.55);
}}

.kalam-reader-ui-live .kalam-reader-annotation-card-green {{
    background: alpha(#8acb9c, 0.08);
    border-color: alpha(#8acb9c, 0.16);
    border-left: 3px solid alpha(#8acb9c, 0.55);
}}

.kalam-reader-ui-live .kalam-reader-annotation-card-blue {{
    background: alpha(#8bb7f2, 0.08);
    border-color: alpha(#8bb7f2, 0.16);
    border-left: 3px solid alpha(#8bb7f2, 0.55);
}}

.kalam-reader-ui-live .kalam-reader-annotation-card-pink {{
    background: alpha(#e99bbd, 0.08);
    border-color: alpha(#e99bbd, 0.16);
    border-left: 3px solid alpha(#e99bbd, 0.55);
}}

.kalam-reader-ui-live .kalam-reader-annotation-card-orange {{
    background: alpha(#f2ae72, 0.08);
    border-color: alpha(#f2ae72, 0.16);
    border-left: 3px solid alpha(#f2ae72, 0.55);
}}

.kalam-reader-ui-live button.kalam-reader-filter-chip.kalam-reader-filter-yellow.active {{
    background: alpha(#f4d35e, 0.18);
    border-color: #f4d35e;
    color: #f4d35e;
}}

.kalam-reader-ui-live button.kalam-reader-filter-chip.kalam-reader-filter-green.active {{
    background: alpha(#8acb9c, 0.18);
    border-color: #8acb9c;
    color: #8acb9c;
}}

.kalam-reader-ui-live button.kalam-reader-filter-chip.kalam-reader-filter-blue.active {{
    background: alpha(#8bb7f2, 0.18);
    border-color: #8bb7f2;
    color: #8bb7f2;
}}

.kalam-reader-ui-live button.kalam-reader-filter-chip.kalam-reader-filter-pink.active {{
    background: alpha(#e99bbd, 0.18);
    border-color: #e99bbd;
    color: #e99bbd;
}}

.kalam-reader-ui-live button.kalam-reader-filter-chip.kalam-reader-filter-orange.active {{
    background: alpha(#f2ae72, 0.18);
    border-color: #f2ae72;
    color: #f2ae72;
}}

.kalam-reader-ui-live .kalam-reader-annotation-body {{
    padding: 11px 12px 10px 13px;
}}

.kalam-reader-ui-live .kalam-reader-annotation-card button.kalam-reader-list-hit:hover {{
    background: transparent;
    box-shadow: none;
}}

.kalam-reader-ui-live .kalam-reader-annotation-card button.kalam-btn-icon,
.kalam-reader-ui-live .kalam-reader-annotation-card menubutton.kalam-btn-icon {{
    min-width: 28px;
    min-height: 28px;
    padding: 0;
    background: transparent;
    border: none;
    color: @kalam_danger;
    opacity: 0;
}}

.kalam-reader-ui-live .kalam-reader-annotation-card:hover button.kalam-btn-icon,
.kalam-reader-ui-live .kalam-reader-annotation-card.open button.kalam-btn-icon,
.kalam-reader-ui-live .kalam-reader-annotation-card:hover menubutton.kalam-btn-icon,
.kalam-reader-ui-live .kalam-reader-annotation-card.open menubutton.kalam-btn-icon {{
    opacity: 1;
}}

.kalam-reader-ui-live .kalam-reader-annotation-note-preview {{
    background: transparent;
    border: none;
    border-top: 1px solid alpha(@kalam_border, 0.4);
    border-radius: 0;
    padding: 6px 13px 8px;
    color: @kalam_text_dim;
}}

.kalam-reader-ui-live .kalam-reader-annotation-note-preview:hover {{
    background: alpha(@kalam_surface_2, 0.4);
}}

.kalam-reader-ui-live label.kalam-reader-note-preview-text {{
    color: @kalam_text_dim;
    font-size: 0.72rem;
    font-style: italic;
    line-height: 1.4;
}}

.kalam-reader-ui-live .kalam-reader-annotation-note-wrap {{
    border-top: 1px solid alpha(@kalam_border, 0.4);
    padding: 8px 13px 10px;
}}

.kalam-reader-ui-live label.kalam-reader-note-label {{
    color: @kalam_text_dim;
    font-size: 0.68rem;
    font-weight: 700;
    letter-spacing: 0.07em;
}}

.kalam-reader-ui-live scrolledwindow.kalam-reader-note-scroll {{
    min-height: 52px;
    background: alpha(@kalam_bg, 0.2);
    border: 1px solid alpha(@kalam_border, 0.55);
    border-radius: 7px;
}}

.kalam-reader-ui-live textview.kalam-reader-note-view {{
    min-height: 52px;
    padding: 7px 10px;
    background: transparent;
    color: @kalam_text;
}}

.kalam-reader-ui-live textview.kalam-reader-note-view text {{
    background: transparent;
    color: @kalam_text;
}}

.kalam-reader-ui-live popover.kalam-reader-color-popover {{
    background: @kalam_surface;
    border: 1px solid @kalam_border;
    border-radius: 10px;
}}

.kalam-reader-ui-live box.kalam-reader-color-palette {{
    padding: 6px;
}}

.kalam-reader-ui-live button.kalam-reader-color-choice {{
    min-width: 92px;
    min-height: 30px;
    padding: 5px 10px;
    background: @kalam_surface_2;
    border: 1px solid @kalam_border;
    border-radius: 7px;
    color: @kalam_text;
}}

.kalam-reader-ui-live button.kalam-reader-color-choice:hover,
.kalam-reader-ui-live button.kalam-reader-color-choice.active {{
    background: alpha(@kalam_surface_2, 0.9);
    border-color: @kalam_accent;
}}

.kalam-reader-ui-live button.kalam-reader-color-choice-yellow {{
    color: #f4d35e;
}}

.kalam-reader-ui-live button.kalam-reader-color-choice-green {{
    color: #8acb9c;
}}

.kalam-reader-ui-live button.kalam-reader-color-choice-blue {{
    color: #8bb7f2;
}}

.kalam-reader-ui-live button.kalam-reader-color-choice-pink {{
    color: #e99bbd;
}}

.kalam-reader-ui-live button.kalam-reader-color-choice-orange {{
    color: #f2ae72;
}}

"#,
        radius = prefs.sidebar_radius,
        shadow_y = shadow_y,
        shadow_blur = shadow_blur,
        shadow_alpha = shadow_alpha,
        left_width = prefs.left_sidebar_width,
        right_width = prefs.right_sidebar_width,
        header_top = header_top,
        header_side = header_side,
        header_bottom = header_bottom,
        section_top = section_top,
        section_side = section_side,
        section_bottom = section_bottom,
        divider_side = divider_side,
        tab_top = tab_top,
        tab_side = tab_side,
        tab_bottom = tab_bottom,
        filter_top = filter_top,
        filter_side = filter_side,
        filter_bottom = filter_bottom,
        toc_top = toc_top,
        toc_side = toc_side,
        list_top = list_top,
        list_side = list_side,
        dim_alpha = dim_alpha,
        pill_pad_y = pill_pad_y,
        pill_pad_x = pill_pad_x,
        pill = pill,
        pill_icon = pill_icon,
        pill_info_pad = pill_info_pad,
        pill_pages_font = pill_pages_font,
        pill_chapter_font = pill_chapter_font,
        back = back,
        back_pad_y = back_pad_y,
        back_pad_right = back_pad_right,
        back_pad_left = back_pad_left,
        back_icon = back_icon,
    )
}

pub(crate) fn apply_reader_ui_prefs(model: &ReaderModel) {
    let gap = model.ui_prefs.sidebar_gap;
    if let Some(shell) = &model.left_sidebar_shell {
        shell.set_margin_start(gap);
        shell.set_margin_top(gap);
        shell.set_margin_bottom(gap);
    }
    if let Some(box_) = &model.left_sidebar_box {
        box_.set_size_request(model.ui_prefs.left_sidebar_width, -1);
    }
    if let Some(shell) = &model.right_sidebar_shell {
        shell.set_margin_end(gap);
        shell.set_margin_top(gap);
        shell.set_margin_bottom(gap);
    }
    if let Some(box_) = &model.right_sidebar_box {
        box_.set_size_request(model.ui_prefs.right_sidebar_width, -1);
    }
    if let Some(back_dock) = &model.back_dock {
        back_dock.set_margin_top(model.ui_prefs.back_top_gap);
        back_dock.set_margin_start(model.ui_prefs.back_side_gap);
    }
    if let Some(bottom_dock) = &model.bottom_dock {
        bottom_dock.set_margin_bottom(model.ui_prefs.bottom_pill_gap);
    }
    model
        .ui_css_provider
        .load_from_string(&reader_ui_css(model.ui_prefs));
}

pub(crate) fn update_reader_ui_setting(model: &mut ReaderModel, setting: ReaderUiSetting, value: i32) -> bool {
    if !model.ui_prefs.set(setting, value) {
        return false;
    }
    model.service.catalog().set_pref(
        reader_ui_pref_key(setting),
        &model.ui_prefs.get(setting).to_string(),
    );
    true
}
