use gtk::prelude::*;
use relm4::prelude::*;
use crate::downloads::{DownloadJob, JobStatus, DOWNLOAD_MANAGER};

#[derive(Debug)]
pub enum DownloadsMsg {
    Tick,
}

pub struct DownloadsModel {
    jobs: Vec<DownloadJob>,
}

#[relm4::component(pub)]
impl Component for DownloadsModel {
    type Init = ();
    type Input = DownloadsMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 16,
            add_css_class: "kalam-page-container",

            // ── Header ───────────────────────────────────────────
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 12,
                add_css_class: "kalam-page-header",

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 4,
                    set_hexpand: true,

                    gtk::Label {
                        set_label: "Downloads Hub",
                        add_css_class: "kalam-title-large",
                        set_halign: gtk::Align::Start,
                    },
                    gtk::Label {
                        #[watch]
                        set_label: &format!("{} downloads tracked", model.jobs.len()),
                        add_css_class: "kalam-subtitle-muted",
                        set_halign: gtk::Align::Start,
                    },
                },
            },

            // ── Empty State ──────────────────────────────────────
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_halign: gtk::Align::Center,
                set_valign: gtk::Align::Center,
                set_spacing: 12,
                set_vexpand: true,
                #[watch]
                set_visible: model.jobs.is_empty(),

                gtk::Image {
                    set_icon_name: Some("folder-download-symbolic"),
                    set_pixel_size: 64,
                    add_css_class: "kalam-subtitle-muted",
                },
                gtk::Label {
                    set_label: "No downloads yet",
                    add_css_class: "kalam-title-medium",
                },
                gtk::Label {
                    set_label: "Novels and stories queued for download will appear here.",
                    add_css_class: "kalam-subtitle-muted",
                }
            },

            // ── Jobs List ────────────────────────────────────────
            gtk::ScrolledWindow {
                set_hexpand: true,
                set_vexpand: true,
                #[watch]
                set_visible: !model.jobs.is_empty(),

                #[name = "jobs_box"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 12,
                    set_margin_start: 8,
                    set_margin_end: 8,
                    set_margin_top: 8,
                    set_margin_bottom: 16,
                }
            }
        }
    }

    fn init(_init: Self::Init, root: Self::Root, sender: ComponentSender<Self>) -> ComponentParts<Self> {
        let initial_jobs = if let Some(mgr) = DOWNLOAD_MANAGER.get() {
            mgr.get_jobs()
        } else {
            Vec::new()
        };

        let model = Self { jobs: initial_jobs };
        let widgets = view_output!();

        // 800ms polling loop to refresh download statuses
        let s = sender.input_sender().clone();
        gtk::glib::timeout_add_local(std::time::Duration::from_millis(800), move || {
            if s.send(DownloadsMsg::Tick).is_err() {
                gtk::glib::ControlFlow::Break
            } else {
                gtk::glib::ControlFlow::Continue
            }
        });

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        _root: &Self::Root,
    ) {
        match msg {
            DownloadsMsg::Tick => {
                if let Some(mgr) = DOWNLOAD_MANAGER.get() {
                    self.jobs = mgr.get_jobs();
                }
            }
        }
        render_jobs(self, widgets);
        self.update_view(widgets, sender);
    }
}

fn render_jobs(model: &DownloadsModel, widgets: &mut DownloadsModelWidgets) {
    // Rebuild job items
    while let Some(child) = widgets.jobs_box.first_child() {
        widgets.jobs_box.remove(&child);
    }

    for job in &model.jobs {
            let row = gtk::Box::new(gtk::Orientation::Vertical, 6);
            row.add_css_class("kalam-card");
            row.set_margin_bottom(4);

            let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            let title = gtk::Label::new(Some(&job.title));
            title.add_css_class("kalam-title-small");
            title.set_halign(gtk::Align::Start);
            title.set_hexpand(true);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            header.append(&title);

            let (status_text, frac, badge_class) = match &job.status {
                JobStatus::Pending => ("Queued".to_string(), 0.0, "kalam-badge"),
                JobStatus::Downloading { chapter_idx, total } => {
                    let f = if *total > 0 { *chapter_idx as f64 / *total as f64 } else { 0.0 };
                    (format!("Downloading ({chapter_idx}/{total})"), f, "kalam-badge")
                }
                JobStatus::Packaging => ("Packaging EPUB...".to_string(), 0.95, "kalam-badge"),
                JobStatus::Done => ("✓ Completed".to_string(), 1.0, "kalam-badge"),
                JobStatus::Cancelled => ("Cancelled".to_string(), 0.0, "kalam-badge"),
                JobStatus::Failed(err) => (format!("Failed: {err}"), 0.0, "kalam-badge"),
            };

            let badge = gtk::Label::new(Some(&status_text));
            badge.add_css_class(badge_class);
            badge.set_halign(gtk::Align::End);
            header.append(&badge);
            row.append(&header);

            let bar = gtk::ProgressBar::new();
            bar.set_fraction(frac);
            if matches!(job.status, JobStatus::Done) {
                bar.add_css_class("success");
            }
            row.append(&bar);

            widgets.jobs_box.append(&row);
        }
    }
