//! Segmented controls and side-panel tab pills (design-system §§10, 22).

use gtk::prelude::*;
use std::cell::Cell;
use std::rc::Rc;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SegmentedSize {
    /// 28px options inside a pill container (§22).
    Small,
    /// 36px options for reader and dialog panels (§10).
    Panel,
}

/// Handle returned by [`segmented_control`]. Cloning it clones widget
/// handles and shares the selection cell, matching normal GTK ownership.
#[derive(Clone)]
pub struct SegmentedControl {
    root: gtk::Box,
    buttons: Vec<gtk::Button>,
    selected: Rc<Cell<usize>>,
}

impl SegmentedControl {
    pub fn widget(&self) -> gtk::Box {
        self.root.clone()
    }

    pub fn selected(&self) -> usize {
        self.selected.get()
    }

    pub fn buttons(&self) -> &[gtk::Button] {
        &self.buttons
    }

    /// Change the visible selection without emitting the change callback.
    /// Callers that mirror external state need this distinction to avoid a
    /// feedback loop.
    pub fn set_selected(&self, index: usize) {
        if self.buttons.is_empty() {
            return;
        }
        let index = index.min(self.buttons.len() - 1);
        self.selected.set(index);
        for (i, button) in self.buttons.iter().enumerate() {
            if i == index {
                button.add_css_class("active");
            } else {
                button.remove_css_class("active");
            }
        }
    }
}

pub fn segmented_control(
    labels: &[&str],
    selected: usize,
    size: SegmentedSize,
    on_change: impl Fn(usize) + 'static,
) -> SegmentedControl {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, crate::style::spacing::XXS);
    root.add_css_class("kalam-ui-segmented");
    if size == SegmentedSize::Panel {
        root.add_css_class("panel");
    }
    root.set_halign(gtk::Align::Start);
    root.set_valign(gtk::Align::Center);

    let state = Rc::new(Cell::new(0));
    let on_change = Rc::new(on_change);
    let mut buttons = Vec::with_capacity(labels.len());

    for (index, label) in labels.iter().enumerate() {
        let button = gtk::Button::with_label(label);
        button.add_css_class("kalam-ui-segment");
        button.set_can_focus(true);
        {
            let state = state.clone();
            let on_change = on_change.clone();
            button.connect_clicked(move |clicked| {
                if state.replace(index) == index {
                    return;
                }
                // Walk siblings instead of capturing the button list: a
                // closure holding every button while owned by one of those
                // buttons would create a reference cycle and leak the row.
                if let Some(parent) = clicked.parent() {
                    let mut child = parent.first_child();
                    let mut i = 0;
                    while let Some(widget) = child {
                        if let Ok(other) = widget.clone().downcast::<gtk::Button>() {
                            if i == index {
                                other.add_css_class("active");
                            } else {
                                other.remove_css_class("active");
                            }
                            i += 1;
                        }
                        child = widget.next_sibling();
                    }
                }
                on_change(index);
            });
        }
        root.append(&button);
        buttons.push(button);
    }

    let control = SegmentedControl {
        root,
        buttons,
        selected: state,
    };
    control.set_selected(selected);
    control
}
