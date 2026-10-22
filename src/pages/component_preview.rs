//! Hidden preview route for the redesign component library.
//!
//! Open it with `KALAM_ROUTE=components`. It is deliberately absent from
//! the navigation rail: this is the GTK equivalent of the design set's
//! Components artboard, used to inspect every shared widget in both light
//! and dark modes before a page redesign depends on it.

use crate::style::{size, spacing};
use crate::widgets::{
    chip::{chip_label, count_badge, filter_chip, key_chip, ChipSize},
    pill_button::{
        boxed_ghost_button, ghost_button, icon_button, pill_button, primary_button,
        set_icon_button_selected, PillButtonKind,
    },
    progress::{linear_progress, segmented_progress, SegmentState},
    rating::rating_control,
    segmented::{segmented_control, SegmentedSize},
    setting_row::{slider_row, stepper_row, switch_row},
    surface::{
        add_inset_row, hairline, inset_card, outlined_section, quiet_notice, side_panel,
        side_panel_close_button, state_card, state_card_with_action,
    },
};
use gtk::prelude::*;
use relm4::prelude::*;

pub struct ComponentPreviewModel;

#[relm4::component(pub)]
impl SimpleComponent for ComponentPreviewModel {
    type Init = ();
    type Input = ();
    type Output = ();

    view! {
        #[root]
        gtk::ScrolledWindow {
            add_css_class: "kalam-component-preview",
            set_policy: (gtk::PolicyType::Never, gtk::PolicyType::External),
            set_hexpand: true,
            set_vexpand: true,

            #[name = "host"]
            gtk::Box {
                add_css_class: "kalam-component-preview-content",
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: spacing::LG,
                set_margin_start: spacing::XL,
                set_margin_end: spacing::XL,
                set_margin_top: spacing::LG,
                set_margin_bottom: spacing::XXL,
            },
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        _sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let model = ComponentPreviewModel;
        let widgets = view_output!();
        build_preview(&widgets.host);
        ComponentParts { model, widgets }
    }
}

fn section(host: &gtk::Box, title: &str) -> gtk::Box {
    let section = outlined_section(title);
    host.append(&section.widget());
    section.body()
}

fn row(spacing: i32) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, spacing);
    row.set_valign(gtk::Align::Center);
    row
}

fn build_preview(host: &gtk::Box) {
    let title = gtk::Label::new(Some("Components"));
    title.add_css_class("kalam-ui-display");
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    host.append(&title);

    let intro = gtk::Label::new(Some(
        "The shared Phase 2 building blocks. Switch palettes or Light/Dark/System in Settings, then return here to inspect the same widgets under every token set.",
    ));
    intro.add_css_class("kalam-ui-meta");
    intro.set_halign(gtk::Align::Start);
    intro.set_xalign(0.0);
    intro.set_wrap(true);
    host.append(&intro);

    let status = gtk::Label::new(Some("Interact with a control to see its callback here."));
    status.add_css_class("kalam-component-preview-status");
    status.set_halign(gtk::Align::Start);
    status.set_xalign(0.0);
    host.append(&status);

    build_buttons(host, &status);
    host.append(&hairline());
    build_segments_and_chips(host, &status);
    host.append(&hairline());
    build_setting_rows(host, &status);
    host.append(&hairline());
    build_progress_and_rating(host, &status);
    host.append(&hairline());
    build_surfaces(host, &status);
}

fn build_buttons(host: &gtk::Box, status: &gtk::Label) {
    let body = section(host, "Buttons");
    let buttons = row(spacing::SM);

    let status_for_primary = status.clone();
    buttons.append(&primary_button("Continue reading", move || {
        status_for_primary.set_label("Primary button clicked.");
    }));

    let status_for_ghost = status.clone();
    buttons.append(&ghost_button("Cancel", move || {
        status_for_ghost.set_label("Ghost button clicked.");
    }));

    let status_for_boxed = status.clone();
    buttons.append(&boxed_ghost_button("Back", move || {
        status_for_boxed.set_label("Boxed ghost button clicked.");
    }));

    let status_for_danger = status.clone();
    buttons.append(&pill_button("Remove", PillButtonKind::Danger, move || {
        status_for_danger.set_label("Destructive button clicked.");
    }));

    let disabled = primary_button("Unavailable", || {});
    disabled.set_sensitive(false);
    buttons.append(&disabled);
    body.append(&buttons);

    let icons = row(spacing::XS);
    let status_for_add = status.clone();
    icons.append(&icon_button(
        "list-add-symbolic",
        "Add",
        size::COMPACT,
        true,
        move || status_for_add.set_label("Add icon button clicked."),
    ));

    let status_for_bookmark = status.clone();
    let bookmark = icon_button(
        "bookmark-new-symbolic",
        "Toggle bookmark preview",
        size::TOUCH_TARGET,
        false,
        move || status_for_bookmark.set_label("Bookmark icon toggled."),
    );
    bookmark.connect_clicked(move |clicked| {
        let selected = !clicked.has_css_class("active");
        set_icon_button_selected(clicked, selected);
    });
    set_icon_button_selected(&bookmark, true);
    icons.append(&bookmark);

    let status_for_search = status.clone();
    icons.append(&icon_button(
        "edit-find-symbolic",
        "Search",
        36,
        false,
        move || status_for_search.set_label("Search icon button clicked."),
    ));
    body.append(&icons);
}

fn build_segments_and_chips(host: &gtk::Box, status: &gtk::Label) {
    let body = section(host, "Segmented controls and chips");

    let modes = ["Light", "Dark", "System"];
    let status_for_modes = status.clone();
    let mode_control = segmented_control(&modes, 2, SegmentedSize::Small, move |index| {
        status_for_modes.set_label(&format!("Small segmented control: {}.", modes[index]));
    });
    mode_control.set_selected(mode_control.selected());
    for (index, button) in mode_control.buttons().iter().enumerate() {
        button.set_tooltip_text(Some(&format!("Preview {}", modes[index])));
    }
    body.append(&mode_control.widget());

    let tabs = ["Contents", "Notes", "Settings"];
    let status_for_tabs = status.clone();
    let tab_control = segmented_control(&tabs, 0, SegmentedSize::Panel, move |index| {
        status_for_tabs.set_label(&format!("Panel tab: {}.", tabs[index]));
    });
    body.append(&tab_control.widget());

    let chips = row(spacing::XS);
    let status_for_unread = status.clone();
    chips.append(&filter_chip(
        "Unread",
        true,
        ChipSize::Standard,
        move |active| {
            status_for_unread.set_label(&format!("Unread filter: {active}."));
        },
    ));
    let status_for_favourites = status.clone();
    chips.append(&filter_chip(
        "Favourites",
        false,
        ChipSize::Standard,
        move |active| {
            status_for_favourites.set_label(&format!("Favourites filter: {active}."));
        },
    ));
    let status_for_search_chip = status.clone();
    chips.append(&filter_chip(
        "Search chip",
        false,
        ChipSize::Search,
        move |active| {
            status_for_search_chip.set_label(&format!("Search chip: {active}."));
        },
    ));
    chips.append(&chip_label("EPUB", true));
    chips.append(&chip_label("Series", false));
    chips.append(&key_chip("Ctrl K", false));
    chips.append(&key_chip("Recording", true));
    chips.append(&count_badge(3));
    body.append(&chips);
}

fn build_setting_rows(host: &gtk::Box, status: &gtk::Label) {
    let body = section(host, "Inset card and setting rows");
    let card = inset_card();

    let status_for_switch = status.clone();
    let switch = switch_row(
        "Follow system appearance",
        Some("Use the desktop's light or dark preference."),
        true,
        move |active| {
            status_for_switch.set_label(&format!("Switch row: {active}."));
        },
    );
    switch.switch().set_tooltip_text(Some("Preview switch"));
    add_inset_row(&card, &switch.widget());

    let status_for_slider = status.clone();
    let slider = slider_row(
        "Line height",
        Some("Reader body spacing."),
        1.4,
        2.1,
        0.05,
        1.75,
        2,
        "",
        move |value| {
            status_for_slider.set_label(&format!("Slider row: {value:.2}."));
        },
    );
    slider.scale().set_tooltip_text(Some("Preview slider"));
    add_inset_row(&card, &slider.widget());

    let status_for_stepper = status.clone();
    let stepper = stepper_row(
        "Reader font size",
        Some("Stored in pixels."),
        13,
        24,
        1,
        18,
        move |value| {
            status_for_stepper.set_label(&format!("Stepper row: {value} px."));
        },
    );
    let _initial_stepper_value = stepper.value();
    add_inset_row(&card, &stepper.widget());

    body.append(&card);
}

fn build_progress_and_rating(host: &gtk::Box, status: &gtk::Label) {
    let body = section(host, "Progress and rating");

    let progress_label = gtk::Label::new(Some("LINEAR"));
    progress_label.add_css_class("kalam-ui-label");
    progress_label.set_halign(gtk::Align::Start);
    body.append(&progress_label);
    body.append(&linear_progress(0.62));

    let segmented_label = gtk::Label::new(Some("SEGMENTED"));
    segmented_label.add_css_class("kalam-ui-label");
    segmented_label.set_halign(gtk::Align::Start);
    body.append(&segmented_label);
    body.append(&segmented_progress(
        &[
            SegmentState::Read,
            SegmentState::Read,
            SegmentState::InProgress(0.45),
            SegmentState::Unread,
            SegmentState::Missing,
        ],
        220,
    ));

    let status_for_rating = status.clone();
    let rating = rating_control(7, move |half_stars| {
        status_for_rating.set_label(&format!(
            "Rating: {:.1} / 5.",
            half_stars as f64 / 2.0
        ));
    });
    let initial_rating = rating.value();
    rating.set_value(initial_rating);
    body.append(&rating.widget());

    let quote = gtk::Label::new(Some(
        "“A reader lives a thousand lives before he dies.”",
    ));
    quote.add_css_class("kalam-ui-quote");
    quote.set_halign(gtk::Align::Start);
    quote.set_xalign(0.0);
    quote.set_wrap(true);
    body.append(&quote);
}

fn build_surfaces(host: &gtk::Box, status: &gtk::Label) {
    let body = section(host, "Panels and states");
    let layout = row(spacing::LG);
    layout.set_valign(gtk::Align::Start);

    let panel = side_panel(Some("Reader panel"));
    let status_for_close = status.clone();
    panel.header().append(&side_panel_close_button(move || {
        status_for_close.set_label("Side-panel close button clicked.");
    }));
    let panel_copy = gtk::Label::new(Some(
        "A 340px shared shell for contents, notes and reader settings.",
    ));
    panel_copy.add_css_class("kalam-ui-body");
    panel_copy.set_wrap(true);
    panel_copy.set_halign(gtk::Align::Start);
    panel_copy.set_xalign(0.0);
    panel.body().append(&panel_copy);
    panel.body().append(&chip_label("340 × radius 22", false));
    layout.append(&panel.widget());

    let states = gtk::Box::new(gtk::Orientation::Vertical, spacing::SM);
    states.set_hexpand(true);
    states.set_valign(gtk::Align::Start);
    states.append(&state_card(
        "Nothing here yet",
        "A blocking empty state uses the dashed state card.",
    ));
    let status_for_retry = status.clone();
    states.append(&state_card_with_action(
        "Could not load this panel",
        "The same card can carry one quiet recovery action.",
        "Try again",
        move || status_for_retry.set_label("State-card action clicked."),
    ));
    states.append(&quiet_notice(
        "No bibliography has been fetched for this author yet.",
    ));
    layout.append(&states);
    body.append(&layout);
}
