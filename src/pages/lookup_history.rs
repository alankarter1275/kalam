//! Phase 10 — Lookup history: the append-only dictionary lookup log,
//! grouped by day like the History page. The off switch lives in reader
//! settings (ReaderMsg::SetDictHistory); Clear empties the table here.

use crate::db::{Catalog, DictLookup};
use crate::pages::history::pretty_day;
use crate::service::LibraryService;
use gtk::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

const PAGE_LIMIT: usize = 500;

#[derive(Debug)]
pub enum LookupHistoryMsg {
    SearchChanged(String),
    Clear,
    Refresh,
}

pub struct LookupHistoryModel {
    service: LibraryService,
    lookups: Vec<DictLookup>,
    query: String,
}

#[relm4::component(pub)]
impl Component for LookupHistoryModel {
    type Init = Arc<Catalog>;
    type Input = LookupHistoryMsg;
    type Output = ();
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,
            set_hexpand: true,

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 10,

                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_hexpand: true,

                    gtk::Label {
                        set_label: "Lookup history",
                        add_css_class: "kalam-page-title",
                        set_halign: gtk::Align::Start,
                    },
                    gtk::Label {
                        #[watch]
                        set_label: &status_line(model.lookups.len()),
                        add_css_class: "kalam-page-sub",
                        set_halign: gtk::Align::Start,
                    },
                },
                gtk::Button {
                    set_child: Some(&crate::icons::symbolic_with_classes(
                        "view-refresh-symbolic",
                        16,
                        &["kalam-inline-icon"],
                    )),
                    add_css_class: "kalam-secondary-btn",
                    connect_clicked => LookupHistoryMsg::Refresh,
                },
                gtk::Button {
                    set_label: "Clear history",
                    add_css_class: "kalam-btn-danger",
                    connect_clicked => LookupHistoryMsg::Clear,
                },
            },

            gtk::SearchEntry {
                set_placeholder_text: Some("Search looked-up words…"),
                set_hexpand: true,
                connect_search_changed[sender] => move |e| {
                    sender.input(LookupHistoryMsg::SearchChanged(e.text().to_string()));
                },
            },

            #[name = "scroll"]
            gtk::ScrolledWindow {
                set_vexpand: true,
                set_hexpand: true,
                #[name = "list_box"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 10,
                    set_margin_top: 6,
                }
            },
        }
    }

    fn init(
        catalog: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let service = LibraryService::new(catalog);
        let snap = service.lookup_history("", PAGE_LIMIT);
        report_errors(&snap.errors);
        let model = LookupHistoryModel {
            service,
            lookups: snap.lookups,
            query: String::new(),
        };
        let widgets = view_output!();
        populate_list(&widgets.list_box, &model.lookups);
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
            LookupHistoryMsg::SearchChanged(query) => {
                self.query = query;
                let snap = self.service.lookup_history(&self.query, PAGE_LIMIT);
                report_errors(&snap.errors);
                self.lookups = snap.lookups;
                populate_list(&widgets.list_box, &self.lookups);
            }
            LookupHistoryMsg::Clear => {
                crate::notify::outcome_info(
                    self.service.catalog().clear_dict_lookups(),
                    "Lookup history cleared",
                    "",
                    "Could not clear the lookup history",
                );
                self.lookups.clear();
                populate_list(&widgets.list_box, &self.lookups);
            }
            LookupHistoryMsg::Refresh => {
                let snap = self.service.lookup_history(&self.query, PAGE_LIMIT);
                report_errors(&snap.errors);
                self.lookups = snap.lookups;
                populate_list(&widgets.list_box, &self.lookups);
            }
        }

        // Refreshes the `#[watch]` status line; overriding `update_with_view`
        // means relm4 no longer calls this for us.
        self.update_view(widgets, sender);
    }
}

fn status_line(count: usize) -> String {
    match count {
        0 => {
            "No lookups recorded yet. Look up a word in the reader with the D key or a tap.".into()
        }
        1 => "1 lookup recorded".into(),
        n => format!("{n} lookups recorded"),
    }
}

/// Surface read failures instead of rendering them as an empty log. The
/// service collects them; deciding what the user sees stays with the UI.
fn report_errors(errors: &[String]) {
    for err in errors {
        crate::notify::error("Could not read your lookup history", err);
    }
}

fn populate_list(list: &gtk::Box, lookups: &[DictLookup]) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    if lookups.is_empty() {
        let empty = gtk::Label::new(Some("Nothing here yet."));
        empty.add_css_class("kalam-placeholder");
        empty.set_halign(gtk::Align::Start);
        list.append(&empty);
        return;
    }

    // Group by calendar day so the log reads like a diary (History pattern).
    let mut current_day = String::new();
    for lookup in lookups {
        let day = lookup.at.get(..10).unwrap_or("").to_string();
        if day != current_day {
            let header = gtk::Label::new(Some(&pretty_day(&day)));
            header.add_css_class("kalam-section-label");
            header.set_halign(gtk::Align::Start);
            header.set_margin_top(if current_day.is_empty() { 0 } else { 10 });
            list.append(&header);
            current_day = day;
        }
        list.append(&build_row(lookup));
    }
}

fn build_row(lookup: &DictLookup) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.add_css_class("kalam-list-row");

    let icon = crate::icons::symbolic_with_classes(
        if lookup.found {
            "accessories-dictionary-symbolic"
        } else {
            "dialog-question-symbolic"
        },
        16,
        &[
            "kalam-event-icon",
            if lookup.found {
                "kalam-event-opened"
            } else {
                "kalam-event-imported"
            },
        ],
    );
    icon.set_valign(gtk::Align::Center);
    row.append(&icon);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);

    let title_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some(&lookup.word));
    title.add_css_class("kalam-card-title");
    title.set_halign(gtk::Align::Start);
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title_row.append(&title);
    if !lookup.found {
        let miss = gtk::Label::new(Some("no definition"));
        miss.add_css_class("kalam-chip");
        miss.add_css_class("kalam-chip-neutral");
        miss.set_valign(gtk::Align::Center);
        title_row.append(&miss);
    }
    text.append(&title_row);

    let context = lookup.context_text.trim();
    let context_line = if context.is_empty() {
        "no context".to_string()
    } else {
        format!("“{}”", context)
    };
    let meta = gtk::Label::new(Some(&context_line));
    meta.add_css_class("kalam-card-meta");
    meta.set_halign(gtk::Align::Start);
    meta.set_xalign(0.0);
    meta.set_ellipsize(gtk::pango::EllipsizeMode::End);
    text.append(&meta);
    row.append(&text);

    let time = gtk::Label::new(Some(lookup.at.get(11..16).unwrap_or("")));
    time.add_css_class("kalam-muted");
    time.set_valign(gtk::Align::Center);
    row.append(&time);

    row
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lookup_count_line_explains_itself_when_empty() {
        // The empty case tells the reader which key does the thing, rather than
        // just saying there is nothing to show.
        let empty = status_line(0);
        assert_eq!(
            empty,
            "No lookups recorded yet. Look up a word in the reader with the D key or a tap."
        );
        assert_eq!(status_line(1), "1 lookup recorded");
        assert_eq!(status_line(2), "2 lookups recorded");
        assert_eq!(status_line(57), "57 lookups recorded");
    }
}
