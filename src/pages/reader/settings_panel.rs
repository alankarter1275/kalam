//! The reader's Settings panel builder (Reading and Shortcuts panes).

use super::keybinds::{KeyBinding, KeyBindings, ReaderAction};
use super::mod_model::ReaderModel;
use super::types::*;
use gtk::prelude::*;
use relm4::prelude::*;
use relm4::RelmWidgetExt;
use std::cell::RefCell;
use std::rc::Rc;

/// The stack child name each settings pane is filed under.
pub(crate) fn reader_settings_pane_name(pane: ReaderSettingsPane) -> &'static str {
    match pane {
        ReaderSettingsPane::Reading => "reading",
        ReaderSettingsPane::Shortcuts => "shortcuts",
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn build_reader_settings_panel(
    sender: &ComponentSender<ReaderModel>,
    font_px: u32,
    line_height: f32,
    column_px: u32,
    scrolled: bool,
    book_override_on: bool,
    book_theme: Option<String>,
    preview_heading: String,
    preview_text: String,
    dict_history_enabled: bool,
    font_family: Option<String>,
    families: Vec<String>,
    justify: bool,
    publisher_styles: bool,
    dual_page: bool,
    autohide_cursor: bool,
    wheel_step: i32,
    arrow_step: i32,
    keybinds: &Rc<RefCell<KeyBindings>>,
    word_memory_scope: &str,
) -> ReaderSettingsControls {
    let wrap = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let switcher = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    switcher.add_css_class("kalam-reader-settings-switcher");
    switcher.set_margin_all(12);
    switcher.set_homogeneous(true);
    let mut pane_buttons = Vec::new();
    for (label, pane) in [
        ("Reading", ReaderSettingsPane::Reading),
        ("Shortcuts", ReaderSettingsPane::Shortcuts),
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

    // This book: the per-book override (decision 147). Off, the book
    // reads with the app defaults; on, everything below is this
    // book's own — including a theme of its own (decision 145).
    // Editing anything below while off turns it on, so these two
    // controls answer to the model either way.
    let book_card = reader_settings_card("book-open-symbolic", "This book");
    let (book_row, set_book_override_switch) =
        book_override_row(book_override_on, sender);
    book_card.append(&book_row);
    let book_hint = gtk::Label::new(Some(
        "Settings changed here apply to this book alone.",
    ));
    book_hint.add_css_class("kalam-reader-setting-hint");
    book_hint.set_wrap(true);
    book_hint.set_xalign(0.0);
    book_card.append(&book_hint);
    let (theme_row, set_book_theme_choice) = book_theme_row(book_theme, sender);
    book_card.append(&theme_row);
    reading_page.append(&book_card);

    // The preview card: this book's own words, set in the standing
    // choices — the same sheet the Settings page draws, narrower.
    // The markup is refreshed from chrome's control sync.
    let preview_card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    preview_card.add_css_class("kalam-settings-preview");
    let preview_column = gtk::Box::new(gtk::Orientation::Vertical, 10);
    preview_column.set_halign(gtk::Align::Center);
    let preview_heading_label = gtk::Label::new(Some(&preview_heading));
    preview_heading_label.add_css_class("kalam-ui-section-title");
    preview_heading_label.set_halign(gtk::Align::Start);
    preview_heading_label.set_xalign(0.0);
    preview_column.append(&preview_heading_label);
    let preview_body = gtk::Label::new(None);
    preview_body.set_wrap(true);
    // The Settings preview's sizing lesson: a wrap label's natural
    // width is its one-line width, which would force the whole 340
    // panel open. Ask for one character and cap with max-width below.
    preview_body.set_width_chars(1);
    preview_body.set_use_markup(true);
    preview_body.set_xalign(0.0);
    {
        let family = font_family
            .clone()
            .unwrap_or_else(|| kalam_reader::BODY_FONT.to_string());
        preview_body.set_markup(&format!(
            "<span font_family=\"{}\" size=\"{}\" line_height=\"{:.1}\">{}</span>",
            gtk::glib::markup_escape_text(&family),
            font_px * 750,
            line_height,
            gtk::glib::markup_escape_text(&preview_text),
        ));
        preview_body.set_justify(if justify {
            gtk::Justification::Fill
        } else {
            gtk::Justification::Left
        });
        let width = (column_px as i32).min(276);
        preview_body
            .set_max_width_chars(((width * 2) / (font_px as i32).max(1)).clamp(20, 120));
        preview_column.set_size_request(width, -1);
    }
    preview_column.append(&preview_body);
    preview_card.append(&preview_column);
    reading_page.append(&preview_card);

    // 1. Typography: font choice, sizing, spacing, alignment, styles
    let type_section = reader_settings_card("insert-text-symbolic", "Typography");
    if !families.is_empty() {
        let default_label = "Default";
        let current = font_family.clone().unwrap_or_default();
        let mut display: Vec<String> = vec![default_label.to_string()];
        display.extend(families.iter().cloned());
        let selected = display.iter().position(|f| *f == current).unwrap_or(0) as u32;

        let strings: Vec<&str> = display.iter().map(|s| s.as_str()).collect();
        let drop = gtk::DropDown::builder()
            .model(&gtk::StringList::new(&strings))
            .build();
        drop.set_valign(gtk::Align::Center);

        let ready = Rc::new(std::cell::Cell::new(false));
        let tx = sender.input_sender().clone();
        let choices = display.clone();
        let ready_for_cb = ready.clone();
        drop.connect_selected_item_notify(move |d| {
            if !ready_for_cb.get() {
                return;
            }
            let chosen = choices
                .get(d.selected() as usize)
                .cloned()
                .unwrap_or_default();
            let value = if chosen == default_label {
                String::new()
            } else {
                chosen
            };
            let _ = tx.send(ReaderMsg::SetFontFamily(value));
        });
        drop.set_selected(selected);
        ready.set(true);

        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.add_css_class("kalam-reader-setting-row");
        let label = gtk::Label::new(Some("Typeface"));
        label.add_css_class("kalam-reader-setting-name");
        label.set_hexpand(true);
        label.set_halign(gtk::Align::Start);
        row.append(&label);
        row.append(&drop);
        type_section.append(&row);
    }

    let fonts_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    fonts_row.add_css_class("kalam-reader-setting-row");
    let fonts_label = gtk::Label::new(Some("Fonts folder"));
    fonts_label.add_css_class("kalam-reader-setting-name");
    fonts_label.set_hexpand(true);
    fonts_label.set_halign(gtk::Align::Start);
    fonts_row.append(&fonts_label);
    let fonts_tx = sender.input_sender().clone();
    let fonts_open = gtk::Button::with_label("Open");
    fonts_open.add_css_class("flat");
    fonts_open.set_tooltip_text(Some("Drop .ttf or .otf files in here"));
    fonts_open.connect_clicked(move |_| {
        let _ = fonts_tx.send(ReaderMsg::OpenFontsFolder);
    });
    fonts_row.append(&fonts_open);
    type_section.append(&fonts_row);

    let fonts_hint =
        gtk::Label::new(Some("New faces join the picker the next time you open the book."));
    fonts_hint.add_css_class("kalam-reader-setting-hint");
    fonts_hint.set_wrap(true);
    fonts_hint.set_xalign(0.0);
    type_section.append(&fonts_hint);

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

    let input_tx = sender.input_sender();
    setting_toggle(&type_section, "Justify", justify, input_tx, ReaderMsg::SetJustify);
    setting_toggle(
        &type_section,
        "Publisher styles",
        publisher_styles,
        input_tx,
        ReaderMsg::SetPublisherStyles,
    );
    reading_page.append(&type_section);

    // 3. Layout: column width, continuous strip vs paged spread
    let layout_section = reader_settings_card("view-paged-symbolic", "Layout");
    let column_width_label = gtk::Label::new(Some(&column_px.to_string()));
    column_width_label.add_css_class("kalam-reader-stepper-value");
    layout_section.append(&reader_stepper_row(
        "Column width",
        &column_width_label,
        sender,
        ReaderMsg::ColumnWidthDelta(-20),
        ReaderMsg::ColumnWidthDelta(20),
    ));

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
    layout_section.append(&mode_row);

    let dual_page_switch = setting_toggle(
        &layout_section,
        "Two pages side by side",
        dual_page,
        input_tx,
        ReaderMsg::SetDualPage,
    );
    dual_page_switch.set_sensitive(!scrolled);
    reading_page.append(&layout_section);

    // 4. Navigation & Scrolling: speeds, pointer behaviour
    let nav_section = reader_settings_card("go-next-symbolic", "Navigation & Scrolling");

    let wheel_step_label = gtk::Label::new(Some(&wheel_step.to_string()));
    wheel_step_label.add_css_class("kalam-reader-stepper-value");
    nav_section.append(&reader_stepper_row(
        "Wheel scroll speed",
        &wheel_step_label,
        sender,
        ReaderMsg::WheelStepDelta(-8),
        ReaderMsg::WheelStepDelta(8),
    ));

    let arrow_step_label = gtk::Label::new(Some(&arrow_step.to_string()));
    arrow_step_label.add_css_class("kalam-reader-stepper-value");
    let arrow_step_row = reader_stepper_row(
        "Arrow scroll speed",
        &arrow_step_label,
        sender,
        ReaderMsg::ArrowStepDelta(-5),
        ReaderMsg::ArrowStepDelta(5),
    );
    arrow_step_row.set_sensitive(scrolled);
    nav_section.append(&arrow_step_row);

    setting_toggle(
        &nav_section,
        "Hide pointer while reading",
        autohide_cursor,
        input_tx,
        ReaderMsg::SetAutohideCursor,
    );
    let pointer_hint = gtk::Label::new(Some(
        "The pointer gets out of the way after two seconds still; move it to bring it back.",
    ));
    pointer_hint.add_css_class("kalam-reader-setting-hint");
    pointer_hint.set_wrap(true);
    pointer_hint.set_xalign(0.0);
    nav_section.append(&pointer_hint);

    reading_page.append(&nav_section);

    // 5. Dictionary. No Sense hint toggle (decision 33): the badge
    // is going; its pref and handler stay dormant.
    let dict_section = reader_settings_card("accessories-dictionary-symbolic", "Dictionary");
    setting_toggle(
        &dict_section,
        "Lookup history",
        dict_history_enabled,
        input_tx,
        ReaderMsg::SetDictHistory,
    );

    // Word Memory Scope setting
    let wm_box = gtk::Box::new(gtk::Orientation::Vertical, 6);
    wm_box.set_margin_start(16);
    wm_box.set_margin_end(16);
    wm_box.set_margin_top(8);
    wm_box.set_margin_bottom(8);

    let wm_label = gtk::Label::new(Some("Word memory (vocabulary dotted underline)"));
    wm_label.set_xalign(0.0);
    wm_label.add_css_class("kalam-reader-setting-title");
    wm_box.append(&wm_label);

    let wm_seg = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    wm_seg.set_homogeneous(true);

    let scopes = [
        ("library", "Library"),
        ("series", "Series"),
        ("book", "Book"),
        ("off", "Off"),
    ];
    let mut wm_buttons = Vec::new();

    for (scope_val, scope_label) in scopes {
        let btn = gtk::Button::with_label(scope_label);
        btn.add_css_class("kalam-reader-seg-btn");
        if word_memory_scope == scope_val {
            btn.add_css_class("active");
        }
        wm_seg.append(&btn);
        wm_buttons.push((scope_val, btn));
    }

    let wm_buttons_rc = std::rc::Rc::new(wm_buttons);
    for (scope_val, btn) in wm_buttons_rc.iter() {
        let tx = input_tx.clone();
        let val = scope_val.to_string();
        let all_btns = wm_buttons_rc.clone();
        btn.connect_clicked(move |_| {
            for (s_val, b) in all_btns.iter() {
                if *s_val == val {
                    b.add_css_class("active");
                } else {
                    b.remove_css_class("active");
                }
            }
            let _ = tx.send(ReaderMsg::SetWordMemoryScope(val.clone()));
        });
    }
    wm_box.append(&wm_seg);
    dict_section.append(&wm_box);

    reading_page.append(&dict_section);

    // The pill that promotes this book's standing choices to the app
    // defaults (decision 147); greyed out, reading "Matches your
    // defaults", while there is nothing to promote.
    let use_defaults_btn = {
        let tx = sender.input_sender().clone();
        crate::widgets::pill_button::boxed_ghost_button(
            "Matches your defaults",
            move || {
                let _ = tx.send(ReaderMsg::UseSettingsAsDefaults);
            },
        )
    };
    use_defaults_btn.add_css_class("kalam-reader-use-defaults");
    use_defaults_btn.set_sensitive(false);
    reading_page.append(&use_defaults_btn);

    stack.add_named(
        &reading_page,
        Some(reader_settings_pane_name(ReaderSettingsPane::Reading)),
    );

    // Roadmap 2.9, third cut: the reader's own keys, in a tab of their
    // own — twelve rows deep they were burying the reading settings, and
    // the tab layout is how the panel already says "a different kind of
    // setting".
    let shortcuts_page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let key_section = reader_settings_card("emblem-system-symbolic", "Keyboard");
    let mut keybind_buttons = Vec::new();
    for action in ReaderAction::ALL {
        let binding = keybinds.borrow().binding(action);
        let button = keybind_row(&key_section, action, binding, sender);
        keybind_buttons.push((action, button));
    }
    // The conflict answer (decision 24): a taken key refuses and names
    // its owner here. Hidden until there is something to say.
    let keybind_hint = gtk::Label::new(None);
    keybind_hint.add_css_class("kalam-reader-keybind-conflict");
    keybind_hint.set_wrap(true);
    keybind_hint.set_xalign(0.0);
    keybind_hint.set_visible(false);
    key_section.append(&keybind_hint);
    let key_hint = gtk::Label::new(Some(
        "Click a key, then press the one you want; Escape cancels. A key does one \
         thing — one already taken refuses and names its owner.",
    ));
    key_hint.add_css_class("kalam-reader-setting-hint");
    key_hint.set_wrap(true);
    key_hint.set_xalign(0.0);
    key_section.append(&key_hint);

    let reset_keys = gtk::Button::with_label("Reset to defaults");
    reset_keys.add_css_class("flat");
    let reset_tx = sender.input_sender().clone();
    reset_keys.connect_clicked(move |_| {
        let _ = reset_tx.send(ReaderMsg::ResetKeyBindings);
    });
    key_section.append(&reset_keys);
    shortcuts_page.append(&key_section);
    stack.add_named(
        &shortcuts_page,
        Some(reader_settings_pane_name(ReaderSettingsPane::Shortcuts)),
    );

    wrap.append(&stack);

    ReaderSettingsControls {
        root: wrap,
        settings_stack: stack,
        pane_buttons,
        font_size_label,
        line_height_label,
        column_width_label,
        wheel_step_label,
        arrow_step_label,
        arrow_step_row,
        keybind_buttons,
        dual_page_switch,
        set_book_override_switch,
        set_book_theme_choice,
        use_defaults_btn,
        keybind_hint,
        preview_column,
        preview_body,
    }
}

/// One editable key: its name, and a button that shows the key and takes a
/// new one when clicked.
///
/// The capture is a key controller on the button itself, armed by the click
/// and disarmed by whatever key arrives — so there is no modal to dismiss
/// and no way to leave the panel listening.
fn keybind_row(
    section: &gtk::Box,
    action: ReaderAction,
    binding: KeyBinding,
    sender: &ComponentSender<ReaderModel>,
) -> gtk::Button {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("kalam-reader-setting-row");
    let label = gtk::Label::new(Some(action.label()));
    label.add_css_class("kalam-reader-setting-name");
    label.set_hexpand(true);
    label.set_halign(gtk::Align::Start);
    row.append(&label);

    let button = gtk::Button::with_label(&binding.label());
    button.add_css_class("kalam-reader-keycap");
    button.set_tooltip_text(Some("Click, then press the key you want"));
    row.append(&button);
    section.append(&row);

    // What the button showed before capture, so Escape can put it back.
    let shown = Rc::new(RefCell::new(binding.label()));
    let armed = Rc::new(std::cell::Cell::new(false));

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    {
        let armed = armed.clone();
        let shown = shown.clone();
        let button = button.clone();
        let tx = sender.input_sender().clone();
        keys.connect_key_pressed(move |_, keyval, _, state| {
            use gtk::glib::Propagation;
            if !armed.get() {
                return Propagation::Proceed;
            }
            armed.set(false);
            if keyval == gtk::gdk::Key::Escape {
                let back = shown.borrow().clone();
                button.set_label(&back);
                return Propagation::Stop;
            }
            let ctrl = state.contains(gtk::gdk::ModifierType::CONTROL_MASK);
            let Some(binding) = KeyBinding::capture(keyval, ctrl) else {
                return Propagation::Stop;
            };
            *shown.borrow_mut() = binding.label();
            button.set_label(&shown.borrow());
            let _ = tx.send(ReaderMsg::SetKeyBinding(action, keyval, ctrl));
            Propagation::Stop
        });
    }
    button.add_controller(keys);

    {
        // A name of its own: the closure cannot move the button it is
        // being attached to.
        let armed = armed.clone();
        let target = button.clone();
        button.connect_clicked(move |_| {
            armed.set(true);
            target.set_label("Press a key…");
            target.grab_focus();
        });
    }

    button
}

/// One settings section as an inset card (spec §3): the app's fill
/// under the standing card, a header icon beside the Label-style
/// name, then the rows.
pub(crate) fn reader_settings_card(icon_name: &'static str, label: &str) -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 10);
    section.add_css_class("kalam-reader-section");
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    head.append(&crate::icons::symbolic_with_classes(
        icon_name,
        16,
        &["kalam-inline-icon"],
    ));
    let heading = gtk::Label::new(Some(label));
    heading.add_css_class("kalam-reader-section-label");
    heading.set_halign(gtk::Align::Start);
    head.append(&heading);
    section.append(&head);
    section
}

/// The This-book switch row (decision 147) and the setter that keeps
/// it in step when an edit below creates the override on its own or
/// resetting deletes it — the guard cell keeps that from echoing
/// back as a user flip.
fn book_override_row(
    on: bool,
    sender: &ComponentSender<ReaderModel>,
) -> (gtk::Box, Rc<dyn Fn(bool)>) {
    use std::cell::Cell;
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("kalam-reader-setting-row");
    let label = gtk::Label::new(Some("Use different settings for this book"));
    label.add_css_class("kalam-reader-setting-name");
    label.set_hexpand(true);
    label.set_halign(gtk::Align::Start);
    label.set_wrap(true);
    row.append(&label);

    let switching = Rc::new(Cell::new(false));
    let sw = gtk::Switch::new();
    sw.set_active(on);
    sw.set_valign(gtk::Align::Center);
    sw.add_css_class("kalam-switch");
    let tx = sender.input_sender().clone();
    let switching_cb = switching.clone();
    sw.connect_active_notify(move |sw| {
        if !switching_cb.get() {
            let _ = tx.send(ReaderMsg::SetBookOverride(sw.is_active()));
        }
    });
    row.append(&sw);

    let set: Rc<dyn Fn(bool)> = {
        let switching = switching.clone();
        let sw = sw.clone();
        Rc::new(move |on| {
            switching.set(true);
            sw.set_active(on);
            switching.set(false);
        })
    };
    (row, set)
}

/// The book's own theme (decision 145): the app's, or any palette in
/// either mode. Only the page follows it — the chrome stays the
/// app's. Comes back with the silent setter the model syncs through:
/// the custom dropdown's `set_selected` never fires its change
/// callback, so a sync cannot echo back as a user choice.
fn book_theme_row(
    current: Option<String>,
    sender: &ComponentSender<ReaderModel>,
) -> (gtk::Box, BookThemeSetter) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("kalam-reader-setting-row");
    let label = gtk::Label::new(Some("Theme"));
    label.add_css_class("kalam-reader-setting-name");
    label.set_hexpand(true);
    label.set_halign(gtk::Align::Start);
    row.append(&label);

    let mut ids: Vec<Option<String>> = vec![None];
    let mut display: Vec<String> = vec!["App theme".to_string()];
    for palette in crate::theme::Palette::ALL.iter().copied() {
        for mode in [crate::theme::ModePref::Light, crate::theme::ModePref::Dark] {
            let choice = crate::theme::ThemeChoice { palette, mode };
            ids.push(Some(choice.id()));
            display.push(choice.label());
        }
    }
    let selected = ids.iter().position(|id| *id == current).unwrap_or(0);

    let options: Vec<crate::widgets::dropdown::DropdownOption> = display
        .into_iter()
        .map(crate::widgets::dropdown::DropdownOption::new)
        .collect();
    let tx = sender.input_sender().clone();
    let choices = ids.clone();
    // The handle must outlive the row (the silent setter drives it),
    // so it rides inside an Rc the setter closure keeps.
    let drop = Rc::new(crate::widgets::dropdown::dropdown(
        "Theme",
        options,
        selected,
        None,
        move |index| {
            let chosen = choices.get(index).cloned().unwrap_or(None);
            let _ = tx.send(ReaderMsg::SetBookTheme(chosen));
        },
        || {},
    ));
    row.append(&drop.widget());

    let set: BookThemeSetter = Rc::new(move |id: Option<&str>| {
        let idx = ids
            .iter()
            .position(|entry| entry.as_deref() == id)
            .unwrap_or(0);
        drop.set_selected(idx);
    });
    (row, set)
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

fn setting_toggle(
    section: &gtk::Box,
    label: &str,
    on: bool,
    tx: &relm4::Sender<ReaderMsg>,
    make_msg: fn(bool) -> ReaderMsg,
) -> gtk::Switch {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("kalam-reader-setting-row");
    let name = gtk::Label::new(Some(label));
    name.add_css_class("kalam-reader-setting-name");
    name.set_hexpand(true);
    name.set_halign(gtk::Align::Start);
    row.append(&name);
    let tx = tx.clone();
    let switch = crate::pages::settings::toggle_switch(on, move |v| {
        let _ = tx.send(make_msg(v));
    });
    row.append(&switch);
    section.append(&row);
    switch
}
