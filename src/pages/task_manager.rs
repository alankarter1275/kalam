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
                }

                gtk::Label {
                    set_label: "Running",
                    add_css_class: "kalam-section-label",
                    set_halign: gtk::Align::Start,
                },
                #[name = "running_list"]
                gtk::ListBox {
                    add_css_class: "kalam-card",
                    set_selection_mode: gtk::SelectionMode::None,
                }

                #[name = "recent_header"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 8,
                }
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

        let mut model = TaskManagerModel {
            running: tasks::tasks(),
            recent: tasks::recent(),
            status: widgets.status.clone(),
            running_list: widgets.running_list.clone(),
            recent_header: widgets.recent_header.clone(),
            recent_list: widgets.recent_list.clone(),
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

        // A repeating tick on the GLib main loop. No tokio, per the standing
        // rule in `tasks.rs`: relm4 is already the actor framework, and a
        // second scheduler would only fight the first.
        let tick_sender = sender.input_sender().clone();
        gtk::glib::timeout_add_local(TICK, move || {
            match tick_sender.send(TaskManagerMsg::Tick) {
                Ok(()) => gtk::glib::ControlFlow::Continue,
                // Fails once the page is gone, which is what ends the loop.
                Err(_) => gtk::glib::ControlFlow::Break,
            }
        });

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

        clear(&self.running_list);
        if self.running.is_empty() {
            self.running_list.append(&empty_row("Nothing is running."));
        }
        for task in &self.running {
            self.running_list.append(&running_row(task, sender));
        }

        clear(&self.recent_list);
        if self.recent.is_empty() {
            self.recent_list.append(&empty_row("Nothing has finished yet."));
        }
        for task in &self.recent {
            self.recent_list.append(&recent_row(task));
        }
    }
}

fn clear(list: &gtk::ListBox) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
}

fn empty_row(text: &str) -> gtk::ListBoxRow {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-page-sub");
    label.set_margin_all(14);
    label.set_halign(gtk::Align::Start);
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&label));
    row
}

/// One running task: a name, a progress bar when the task can count, and a
/// way to stop it.
fn running_row(task: &TaskInfo, sender: &ComponentSender<TaskManagerModel>) -> gtk::ListBoxRow {
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
        let s = sender.input_sender().clone();
        let id = task.id;
        cancel.connect_clicked(move |_| {
            s.send(TaskManagerMsg::Cancel(id)).ok();
        });
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

    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&col));
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
