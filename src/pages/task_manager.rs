//! 1.14 — the task manager: what is running, how far along, and how to stop it.
//!
//! Every background task in Kalam already goes through [`crate::tasks`], which
//! records each one with a label and its latest progress. This page does no
//! work of its own — it is a window onto that registry.
//!
//! **Why a page and not a toast.** A toast says "something happened" and is
//! gone. A task manager answers the three questions a long job raises and a
//! toast cannot: *is it still going? how far along is it? can I stop it?*
//! Until now the only answer to the third one was "close the window", which
//! called `cancel_all()` and took every task with it.
//!
//! **Why it polls.** The registry is a `Mutex<Vec<..>>` that the main thread
//! reads and the workers never touch. A tick every half second costs a few
//! microseconds; pushing updates instead would mean every task holding a
//! reference to a widget, coupling the workers to the UI for no benefit. The
//! tick also stops on its own when the page closes, because the sender it
//! sends to is dropped with it.

use crate::tasks::{self, FinishedTask, TaskInfo};
use gtk::prelude::*;
use relm4::prelude::*;
use std::rc::Rc;

/// How often the list re-reads the registry.
///
/// Half a second is quick enough that a progress bar visibly moves and slow
/// enough that a long download is not redrawing constantly. A fast query —
/// the 5 ms page loads from roadmap 1.2b — is usually added and removed
/// between two ticks, so it simply never appears. That is the desired
/// behaviour: this page is for work you are waiting on, not for noise.
const TICK: std::time::Duration = std::time::Duration::from_millis(500);

#[derive(Debug)]
pub enum TaskManagerMsg {
    /// Re-read the registry.
    Tick,
    /// Ask one task to stop.
    Cancel(u64),
    /// Empty the finished list.
    ClearFinished,
}

pub struct TaskManagerModel {
    running: Vec<TaskInfo>,
    recent: Vec<FinishedTask>,
    // Widget handles kept on the model so `update` can reach them. relm4 hands
    // `update` the root but not the widget struct, and these are refcounted
    // clones, not ownership — the same reason `library.rs` passes `&widgets`
    // around rather than storing it.
    status: gtk::Label,
    running_list: gtk::ListBox,
    recent_header: gtk::Box,
    recent_list: gtk::ListBox,
    /// Held for the page's lifetime, not used.
    ///
    /// Whether dropping a `SourceId` detaches its source is not something this
    /// codebase has ever settled: `reader/mod.rs` stores all four of its timers
    /// and removes them by hand, while `downloads.rs` drops one — and that page
    /// is hidden, so nobody has ever seen its 800 ms poll run. Rather than
    /// inherit that ambiguity in the one loop the whole panel depends on, keep
    /// the handle. A tick that stopped silently would leave this page frozen on
    /// whatever it last read, which is indistinguishable from "nothing running".
    _tick: gtk::glib::SourceId,
}

#[relm4::component(pub)]
impl Component for TaskManagerModel {
    type Init = ();
    type Input = TaskManagerMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        gtk::ScrolledWindow {
            set_hscrollbar_policy: gtk::PolicyType::Never,
            set_vexpand: true,

            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 14,
                set_margin_all: 24,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_hexpand: true,

                    gtk::Label {
                        set_label: "Tasks",
                        add_css_class: "kalam-page-title",
                        set_halign: gtk::Align::Start,
                    },
                    #[name = "status"]
                    gtk::Label {
                        add_css_class: "kalam-page-sub",
                        set_halign: gtk::Align::Start,
                    },
                },

                gtk::Label {
                    set_label: "Running",
                    add_css_class: "kalam-section-label",
                    set_halign: gtk::Align::Start,
                },
                #[name = "running_list"]
                gtk::ListBox {
                    add_css_class: "kalam-card",
                    set_selection_mode: gtk::SelectionMode::None,
                },

                #[name = "recent_header"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                },
                #[name = "recent_list"]
                gtk::ListBox {
                    add_css_class: "kalam-card",
                    set_selection_mode: gtk::SelectionMode::None,
                }
            }
        }
    }

    fn init(
        _init: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let widgets = view_output!();

        // Started before the model exists because the model holds its handle.
        // A repeating tick on the GLib main loop — no tokio, per the standing
        // rule in `tasks.rs`.
        let tick_sender = sender.input_sender().clone();
        let tick = gtk::glib::timeout_add_local(TICK, move || {
            match tick_sender.send(TaskManagerMsg::Tick) {
                Ok(()) => gtk::glib::ControlFlow::Continue,
                // Fails once the page is gone, which is what ends the loop.
                Err(_) => gtk::glib::ControlFlow::Break,
            }
        });

        let model = TaskManagerModel {
            running: tasks::tasks(),
            recent: tasks::recent(),
            status: widgets.status.clone(),
            running_list: widgets.running_list.clone(),
            recent_header: widgets.recent_header.clone(),
            recent_list: widgets.recent_list.clone(),
            _tick: tick,
        };

        // The "Recently finished" header is built here rather than in `view!`
        // because its clear button needs the sender.
        let header_label = gtk::Label::new(Some("Recently finished"));
        header_label.add_css_class("kalam-section-label");
        header_label.set_halign(gtk::Align::Start);
        model.recent_header.append(&header_label);
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        model.recent_header.append(&spacer);
        let clear = gtk::Button::with_label("Clear");
        clear.add_css_class("kalam-secondary-btn");
        {
            let s = sender.input_sender().clone();
            clear.connect_clicked(move |_| {
                s.send(TaskManagerMsg::ClearFinished).ok();
            });
        }
        model.recent_header.append(&clear);

        model.render(&sender);

        ComponentParts { model, widgets }
    }

    fn update(&mut self, msg: Self::Input, sender: ComponentSender<Self>, _root: &Self::Root) {
        match msg {
            TaskManagerMsg::Tick => {
                self.running = tasks::tasks();
                self.recent = tasks::recent();
                self.render(&sender);
            }
            TaskManagerMsg::Cancel(id) => {
                // Cooperative: this sets a flag and the worker notices at its
                // next check, so the row will not vanish instantly. Reading
                // the registry again now still matters, because a task that
                // had already finished is gone from it.
                tasks::cancel(id);
                self.running = tasks::tasks();
                self.render(&sender);
            }
            TaskManagerMsg::ClearFinished => {
                tasks::clear_finished();
                self.recent = tasks::recent();
                self.render(&sender);
            }
        }
    }
}

impl TaskManagerModel {
    /// Rebuild both lists and the status line from the model.
    ///
    /// Clear-and-refill rather than diffing: there are single digits of rows,
    /// and rebuilding is what `library.rs` does for the dashboard. A task list
    /// redrawing twice a second is not something anyone reads closely enough
    /// to notice.
    fn render(&self, sender: &ComponentSender<Self>) {
        self.status.set_label(&status_line(&self.running, &self.recent));
        let s = sender.input_sender().clone();
        let on_cancel: Rc<dyn Fn(u64)> =
            Rc::new(move |id| {
                s.send(TaskManagerMsg::Cancel(id)).ok();
            });

        clear_list(&self.running_list);
        if self.running.is_empty() {
            self.running_list.append(&empty_row("Nothing is running."));
        }
        for task in &self.running {
            let content = running_row(task, &on_cancel);
            self.running_list.append(&as_list_row(&content));
        }

        clear_list(&self.recent_list);
        if self.recent.is_empty() {
            self.recent_list.append(&empty_row("Nothing has finished yet."));
        }
        for task in &self.recent {
            self.recent_list.append(&recent_row(task));
        }
    }
}

/// What the sidebar button shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Badge {
    /// Nothing running, nothing went wrong.
    Idle,
    /// This many tasks running.
    Running(usize),
    /// Nothing running, but something recent failed and has not been
    /// acknowledged. The toast that reported it is long gone by now.
    Failed,
}

/// Read the registry the way the sidebar badge needs it.
pub(crate) fn badge() -> Badge {
    let running = tasks::tasks().len();
    if running > 0 {
        return Badge::Running(running);
    }
    if tasks::recent().iter().any(|t| t.failed) {
        return Badge::Failed;
    }
    Badge::Idle
}

/// Swap the sidebar button's face to match [`Badge`].
///
/// The mark *is* the button's child rather than an overlay, because there is
/// nothing to overlay: idle shows a tick, busy shows a count, failure shows a
/// warning. Three mutually exclusive states, one slot.
///
/// Deliberately uses no new CSS. `resources/style.css` is under a ratchet
/// (`tests/guardrails.rs`, 184 hex colours) and a badge does not justify
/// spending any of that budget; the count inherits the nav button's own font.
pub(crate) fn apply_badge(btn: &gtk::Button, state: Badge) {
    match state {
        Badge::Idle => {
            let icon = crate::icons::symbolic_with_classes(
                "object-select-symbolic",
                18,
                &["kalam-nav-icon"],
            );
            icon.set_halign(gtk::Align::Center);
            icon.set_valign(gtk::Align::Center);
            btn.set_child(Some(&icon));
            btn.set_tooltip_text(Some("Tasks — nothing running"));
        }
        Badge::Running(n) => {
            let label = gtk::Label::new(Some(&n.to_string()));
            label.set_halign(gtk::Align::Center);
            label.set_valign(gtk::Align::Center);
            btn.set_child(Some(&label));
            let noun = if n == 1 { "task" } else { "tasks" };
            btn.set_tooltip_text(Some(&format!("{n} {noun} running")));
        }
        Badge::Failed => {
            let icon = crate::icons::symbolic_with_classes(
                "dialog-warning-symbolic",
                18,
                &["kalam-nav-icon"],
            );
            icon.set_halign(gtk::Align::Center);
            icon.set_valign(gtk::Align::Center);
            btn.set_child(Some(&icon));
            btn.set_tooltip_text(Some("Tasks — something failed"));
        }
    }
}

/// Build the panel the `w` key opens.
///
/// Returns the panel and the box its rows go into, separately, so the caller
/// can refill that box on its own timer. The dialog deliberately has **no
/// timer of its own**: `AppModel` already ticks once to keep the sidebar badge
/// honest, and one timer in one place is easier to reason about than two —
/// especially after the task *page* turned out to depend on a handle nobody
/// was keeping alive.
pub(crate) fn build_tasks_dialog(on_cancel: Rc<dyn Fn(u64)>) -> (gtk::Box, gtk::Box) {
    let panel = gtk::Box::new(gtk::Orientation::Vertical, 10);
    panel.add_css_class("kalam-float-panel");
    panel.set_margin_all(16);

    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some("Running tasks"));
    title.add_css_class("kalam-title-small");
    title.set_halign(gtk::Align::Start);
    title.set_hexpand(true);
    head.append(&title);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
    panel.append(&head);
    panel.append(&list);
    fill_tasks_dialog(&list, &on_cancel);
    (panel, list)
}

/// Refill the `w` dialog's list. Called on every app tick while it is open.
pub(crate) fn fill_tasks_dialog(list: &gtk::Box, on_cancel: &Rc<dyn Fn(u64)>) {
    clear_box(list);
    let running = tasks::tasks();
    if running.is_empty() {
        list.append(&empty_label("Nothing is running."));
        return;
    }
    for task in &running {
        list.append(&running_row(task, on_cancel));
    }
}

// Two of them because `remove` lives in `ListBoxExt` for one and `BoxExt` for
// the other, and there is no shared trait carrying it.
fn clear_list(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn clear_box(list: &gtk::Box) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn empty_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-page-sub");
    label.set_margin_all(14);
    label.set_halign(gtk::Align::Start);
    label
}

fn empty_row(text: &str) -> gtk::ListBoxRow {
    as_list_row(&empty_label(text))
}

/// One running task: a name, a progress bar when the task can count, and a
/// way to stop it.
///
/// Takes a callback rather than a `ComponentSender` because two different
/// hosts build these rows — the full page and the `w` dialog — and only one of
/// them is a component.
/// Builds the *content*, not a `ListBoxRow`: the page wraps it in a row, the
/// `w` dialog appends it straight to a plain box, and a `ListBoxRow` outside a
/// `ListBox` is a widget that renders as nothing.
pub(crate) fn running_row(task: &TaskInfo, on_cancel: &Rc<dyn Fn(u64)>) -> gtk::Box {
    let col = gtk::Box::new(gtk::Orientation::Vertical, 4);
    col.set_margin_all(12);

    // Title row: label on the left, cancel on the right.
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some(&task.label));
    title.add_css_class("kalam-hist-title");
    title.set_halign(gtk::Align::Start);
    title.set_hexpand(true);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    top.append(&title);

    let cancel = gtk::Button::with_label("Cancel");
    cancel.add_css_class("kalam-secondary-btn");
    {
        let cb = on_cancel.clone();
        let id = task.id;
        cancel.connect_clicked(move |_| cb(id));
    }
    top.append(&cancel);
    col.append(&top);

    // Only show a bar when the task reported a total. An empty bar for a task
    // that cannot know its length reads as "stuck", which is worse than
    // showing nothing.
    if task.total > 0 {
        let bar = gtk::ProgressBar::new();
        let frac = task.done as f64 / task.total as f64;
        bar.set_fraction(frac.clamp(0.0, 1.0));
        bar.set_text(Some(&format!("{} of {}", task.done, task.total)));
        bar.set_show_text(true);
        col.append(&bar);
    }

    if !task.detail.is_empty() {
        let detail = gtk::Label::new(Some(&task.detail));
        detail.add_css_class("kalam-page-sub");
        detail.set_halign(gtk::Align::Start);
        detail.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        detail.set_max_width_chars(48);
        col.append(&detail);
    }

    col
}

/// Wrap row content for a `ListBox`.
fn as_list_row(child: &impl IsA<gtk::Widget>) -> gtk::ListBoxRow {
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(child));
    row
}

fn recent_row(task: &FinishedTask) -> gtk::ListBoxRow {
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    line.set_margin_all(12);

    let label = gtk::Label::new(Some(&task.label));
    label.set_halign(gtk::Align::Start);
    label.set_hexpand(true);
    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    line.append(&label);

    // "Cancelled" is worth saying out loud. A task the user stopped and a task
    // that finished look the same otherwise, and the difference is whether
    // they need to start it again.
    let state = gtk::Label::new(Some(if task.cancelled {
        "Cancelled"
    } else {
        "Finished"
    }));
    state.add_css_class("kalam-page-sub");
    line.append(&state);

    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&line));
    row
}

fn status_line(running: &[TaskInfo], recent: &[FinishedTask]) -> String {
    match running.len() {
        0 => "Nothing running in the background.".to_string(),
        1 => format!("1 task running. {} finished recently.", recent.len()),
        n => format!("{n} tasks running. {} finished recently.", recent.len()),
    }
}
