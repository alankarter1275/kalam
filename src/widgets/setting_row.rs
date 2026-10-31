//! Switch, slider and stepper rows for panels and dialogs (§§9–10).

use super::pill_button::icon_button;
use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

fn setting_copy(label: &str, description: Option<&str>) -> gtk::Box {
    let copy = gtk::Box::new(gtk::Orientation::Vertical, crate::style::spacing::XXS);
    copy.add_css_class("kalam-ui-setting-copy");
    copy.set_hexpand(true);
    copy.set_valign(gtk::Align::Center);

    let title = gtk::Label::new(Some(label));
    title.add_css_class("kalam-ui-body");
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    copy.append(&title);

    if let Some(description) = description {
        let meta = gtk::Label::new(Some(description));
        meta.add_css_class("kalam-ui-meta");
        meta.set_halign(gtk::Align::Start);
        meta.set_xalign(0.0);
        meta.set_wrap(true);
        copy.append(&meta);
    }
    copy
}

#[derive(Clone)]
pub struct SwitchRow {
    root: gtk::Box,
    switch: gtk::Switch,
}

impl SwitchRow {
    pub fn widget(&self) -> gtk::Box {
        self.root.clone()
    }

    pub fn switch(&self) -> gtk::Switch {
        self.switch.clone()
    }
}

pub fn switch_row(
    label: &str,
    description: Option<&str>,
    initial: bool,
    on_change: impl Fn(bool) + 'static,
) -> SwitchRow {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, crate::style::spacing::SM);
    root.add_css_class("kalam-ui-setting-row");
    root.append(&setting_copy(label, description));

    let switch = gtk::Switch::new();
    switch.add_css_class("kalam-ui-switch");
    switch.set_active(initial);
    switch.set_valign(gtk::Align::Center);
    // Connected after the initial value, so constructing a row never looks
    // like a user change.
    switch.connect_active_notify(move |switch| on_change(switch.is_active()));
    root.append(&switch);

    SwitchRow { root, switch }
}

fn format_slider_value(value: f64, decimals: usize, suffix: &str) -> String {
    format!("{value:.decimals$}{suffix}", decimals = decimals)
}

#[derive(Clone)]
pub struct SliderRow {
    root: gtk::Box,
    scale: gtk::Scale,
    suppress_change: Rc<Cell<bool>>,
}

impl SliderRow {
    pub fn widget(&self) -> gtk::Box {
        self.root.clone()
    }

    pub fn scale(&self) -> gtk::Scale {
        self.scale.clone()
    }

    /// Mirror external state without emitting the change callback.
    ///
    /// A plain `scale.set_value` during a model sync is not harmless:
    /// the callback's message is queued *behind* any edits already in
    /// flight, so two rapid external changes can leave stale 100% and
    /// 45% messages taking turns undoing each other forever. Syncing
    /// through here keeps a repaint a repaint.
    pub fn set_value_silently(&self, value: f64) {
        if (self.scale.value() - value).abs() <= f64::EPSILON {
            return;
        }
        self.suppress_change.set(true);
        self.scale.set_value(value);
        self.suppress_change.set(false);
    }
}

#[allow(clippy::too_many_arguments)]
pub fn slider_row(
    label: &str,
    description: Option<&str>,
    min: f64,
    max: f64,
    step: f64,
    initial: f64,
    decimals: usize,
    suffix: &str,
    on_change: impl Fn(f64) + 'static,
) -> SliderRow {
    let root = gtk::Box::new(gtk::Orientation::Vertical, crate::style::spacing::XS);
    root.add_css_class("kalam-ui-setting-row");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, crate::style::spacing::SM);
    header.append(&setting_copy(label, description));
    let value = gtk::Label::new(Some(&format_slider_value(initial, decimals, suffix)));
    value.add_css_class("kalam-ui-meta");
    value.set_valign(gtk::Align::Center);
    header.append(&value);
    root.append(&header);

    let scale = gtk::Scale::with_range(
        gtk::Orientation::Horizontal,
        min,
        max,
        step.max(f64::EPSILON),
    );
    scale.add_css_class("kalam-ui-slider");
    scale.set_value(initial.clamp(min.min(max), min.max(max)));
    scale.set_draw_value(false);
    scale.set_digits(decimals as i32);
    scale.set_hexpand(true);

    let suffix = suffix.to_owned();
    let suppress_change = Rc::new(Cell::new(false));
    let suppress_cb = suppress_change.clone();
    scale.connect_value_changed(move |scale| {
        let current = scale.value();
        value.set_label(&format_slider_value(current, decimals, &suffix));
        if !suppress_cb.get() {
            on_change(current);
        }
    });
    root.append(&scale);

    SliderRow {
        root,
        scale,
        suppress_change,
    }
}

#[derive(Clone)]
pub struct StepperRow {
    root: gtk::Box,
    value_label: gtk::Label,
    minus: gtk::Button,
    plus: gtk::Button,
    value: Rc<Cell<i32>>,
    min: i32,
    max: i32,
}

impl StepperRow {
    pub fn widget(&self) -> gtk::Box {
        self.root.clone()
    }

    pub fn value(&self) -> i32 {
        self.value.get()
    }

    /// Mirror external state without emitting the change callback.
    pub fn set_value(&self, value: i32) {
        let value = value.clamp(self.min, self.max);
        self.value.set(value);
        self.value_label.set_label(&value.to_string());
        self.minus.set_sensitive(value > self.min);
        self.plus.set_sensitive(value < self.max);
    }
}

fn refresh_stepper_siblings(clicked: &gtk::Button, value: i32, min: i32, max: i32) {
    let Some(parent) = clicked.parent() else {
        return;
    };
    let mut child = parent.first_child();
    while let Some(widget) = child {
        match widget.widget_name().as_str() {
            "kalam-ui-stepper-value" => {
                if let Ok(label) = widget.clone().downcast::<gtk::Label>() {
                    label.set_label(&value.to_string());
                }
            }
            "kalam-ui-stepper-minus" => widget.set_sensitive(value > min),
            "kalam-ui-stepper-plus" => widget.set_sensitive(value < max),
            _ => {}
        }
        child = widget.next_sibling();
    }
}

pub fn stepper_row(
    label: &str,
    description: Option<&str>,
    min: i32,
    max: i32,
    step: i32,
    initial: i32,
    on_change: impl Fn(i32) + 'static,
) -> StepperRow {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, crate::style::spacing::SM);
    root.add_css_class("kalam-ui-setting-row");
    root.append(&setting_copy(label, description));

    let value = Rc::new(Cell::new(initial.clamp(min, max)));
    let on_change = Rc::new(on_change);
    let step = step.max(1);

    let controls = gtk::Box::new(gtk::Orientation::Horizontal, crate::style::spacing::XS);
    controls.set_valign(gtk::Align::Center);

    let make_step = |delta: i32, icon: &str, label: &str, widget_name: &str| {
        let value = value.clone();
        let on_change = on_change.clone();
        let button = icon_button(icon, label, crate::style::size::COMPACT, true, move || {
            let next = (value.get() + delta).clamp(min, max);
            // The click callback below also runs and refreshes siblings from
            // the shared cell. Emitting here keeps this helper independent
            // of that widget traversal.
            if next != value.get() {
                value.set(next);
                on_change(next);
            }
        });
        button.set_widget_name(widget_name);
        button
    };

    let minus = make_step(
        -step,
        "list-remove-symbolic",
        "Decrease",
        "kalam-ui-stepper-minus",
    );
    let plus = make_step(
        step,
        "list-add-symbolic",
        "Increase",
        "kalam-ui-stepper-plus",
    );

    // `icon_button` owns the primary click callback; this second handler
    // only mirrors the shared value into the neighbouring widgets.
    for button in [&minus, &plus] {
        let value = value.clone();
        button.connect_clicked(move |clicked| {
            refresh_stepper_siblings(clicked, value.get(), min, max);
        });
    }

    let value_label = gtk::Label::new(Some(&value.get().to_string()));
    value_label.add_css_class("kalam-ui-stepper-value");
    value_label.set_widget_name("kalam-ui-stepper-value");
    value_label.set_valign(gtk::Align::Center);

    controls.append(&minus);
    controls.append(&value_label);
    controls.append(&plus);
    root.append(&controls);

    let row = StepperRow {
        root,
        value_label,
        minus,
        plus,
        value: value.clone(),
        min,
        max,
    };
    row.set_value(value.get());
    row
}

#[cfg(test)]
mod tests {
    use super::format_slider_value;

    #[test]
    fn slider_values_keep_the_requested_precision_and_suffix() {
        assert_eq!(format_slider_value(1.75, 2, ""), "1.75");
        assert_eq!(format_slider_value(18.0, 0, " px"), "18 px");
    }
}
