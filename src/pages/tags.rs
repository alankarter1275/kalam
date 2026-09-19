//! P4 — Tags: browse the library by tag, then drill into one tag's books.

use crate::db::{Catalog, SortKey};
use crate::models::Book;
use crate::service::{LibraryService, TagBooksSnapshot, TagsSnapshot};
use crate::widgets::book_row::build_book_grid;
use gtk::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

// ---------------------------------------------------------------------------
// Tag cloud
// ---------------------------------------------------------------------------

use crate::widgets::in_app_dialog;

#[derive(Debug)]
pub enum TagsOut {
    OpenTag { tag: String },
}

#[derive(Debug)]
pub enum TagsMsg {
    SearchChanged(String),
    RenameTag { old_name: String, new_name: String },
    MergeTag { source_tag: String, target_tag: String },
    DeleteTag { tag_name: String },
    /// A background tag-cloud query finished. Carries the generation it was
    /// started with so a superseded reply cannot overwrite a newer one.
    Loaded { gen: u64, snap: TagsSnapshot },
}

pub struct TagsModel {
    catalog: Arc<Catalog>,
    tags: Vec<(String, i64)>,
    query: String,
    /// Stamps each query so stale replies can be dropped. See
    /// [`TagsMsg::Loaded`].
    reload_gen: u64,
}

#[relm4::component(pub)]
impl Component for TagsModel {
    type Init = Arc<Catalog>;
    type Input = TagsMsg;
    type Output = TagsOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,
            set_hexpand: true,

            gtk::Label {
                set_label: "Tags",
                add_css_class: "kalam-page-title",
                set_halign: gtk::Align::Start,
            },
            gtk::Label {
                #[watch]
                set_label: &status_line(model.visible().len()),
                add_css_class: "kalam-page-sub",
                set_halign: gtk::Align::Start,
            },

            gtk::SearchEntry {
                set_hexpand: true,
                set_placeholder_text: Some("Filter tags…"),
                connect_search_changed[sender] => move |e| {
                    sender.input(TagsMsg::SearchChanged(e.text().to_string()));
                },
            },

            gtk::ScrolledWindow {
                set_vexpand: true,
                set_hexpand: true,
                set_hscrollbar_policy: gtk::PolicyType::Never,

                #[name = "cloud"]
                gtk::FlowBox {
                    set_valign: gtk::Align::Start,
                    set_max_children_per_line: 6,
                    set_min_children_per_line: 2,
                    set_selection_mode: gtk::SelectionMode::None,
                    set_column_spacing: 8,
                    set_row_spacing: 8,
                },
            },
        }
    }

    fn init(
        catalog: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        // Synchronous first read, as on the other pages: the page is not on
        // screen yet, so there is nothing visible to freeze and no cloud to
        // preserve. Every later read goes through `reload`, a worker.
        let snap = LibraryService::new(catalog.clone()).tags();
        report_errors(&snap.errors);
        let model = TagsModel {
            catalog,
            tags: snap.tags,
            query: String::new(),
            reload_gen: 0,
        };
        let widgets = view_output!();
        rebuild(&widgets.cloud, &model.visible(), &sender);
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
            TagsMsg::SearchChanged(q) => self.query = q,
            TagsMsg::RenameTag { old_name, new_name } => {
                if let Err(err) = self.catalog.rename_tag(&old_name, &new_name) {
                    crate::notify::error("Could not rename tag", &err.to_string());
                } else {
                    crate::notify::success("Tag renamed", &format!("Renamed #{old_name} to #{new_name}"));
                }
                self.reload(&sender);
            }
            TagsMsg::MergeTag { source_tag, target_tag } => {
                if let Err(err) = self.catalog.merge_tags(&source_tag, &target_tag) {
                    crate::notify::error("Could not merge tags", &err.to_string());
                } else {
                    crate::notify::success("Tags merged", &format!("Merged #{source_tag} into #{target_tag}"));
                }
                self.reload(&sender);
            }
            TagsMsg::DeleteTag { tag_name } => {
                if let Err(err) = self.catalog.delete_tag(&tag_name) {
                    crate::notify::error("Could not delete tag", &err.to_string());
                } else {
                    crate::notify::success("Tag deleted", &format!("Deleted tag #{tag_name}"));
                }
                self.reload(&sender);
            }
            TagsMsg::Loaded { gen, snap } => {
                // Drop a reply an older query produced.
                if gen != self.reload_gen {
                    return;
                }
                if snap.errors.is_empty() {
                    self.tags = snap.tags;
                } else {
                    // Keep the cloud on screen; a failed read is not a
                    // library with no tags.
                    report_errors(&snap.errors);
                }
            }
        }
        rebuild(&widgets.cloud, &self.visible(), &sender);
        self.update_view(widgets, sender);
    }
}

impl TagsModel {
    /// Re-read the tag cloud on a worker thread (roadmap 1.2b).
    ///
    /// The write that prompted this still happens on the UI thread — moving
    /// writes off it is a separate and larger change — but the re-read is the
    /// part that counts every tag across the whole library, and so the part
    /// that grows with the collection.
    fn reload(&mut self, sender: &ComponentSender<Self>) {
        self.reload_gen += 1;
        let gen = self.reload_gen;
        let catalog = self.catalog.clone();
        let done = sender.clone();
        crate::tasks::spawn(
            move |_reporter| LibraryService::new(catalog).tags(),
            // One query, not a sequence of steps, so nothing to report.
            |_update| {},
            move |snap| done.input(TagsMsg::Loaded { gen, snap }),
        );
    }

    fn visible(&self) -> Vec<(String, i64)> {
        let q = self.query.trim().to_lowercase();
        if q.is_empty() {
            return self.tags.clone();
        }
        self.tags
            .iter()
            .filter(|(name, _)| name.to_lowercase().contains(&q))
            .cloned()
            .collect()
    }
}

fn status_line(n: usize) -> String {
    if n == 0 {
        "No tags yet — tags come from EPUB metadata on import.".into()
    } else {
        format!("{n} tag{} in your library", if n == 1 { "" } else { "s" })
    }
}

fn rebuild(cloud: &gtk::FlowBox, tags: &[(String, i64)], sender: &ComponentSender<TagsModel>) {
    while let Some(child) = cloud.first_child() {
        cloud.remove(&child);
    }

    if tags.is_empty() {
        let empty = gtk::Label::new(Some("No tags match."));
        empty.add_css_class("kalam-placeholder");
        empty.set_halign(gtk::Align::Start);
        cloud.insert(&empty, -1);
        return;
    }

    let max = tags.iter().map(|(_, n)| *n).max().unwrap_or(1).max(1);

    for (name, count) in tags {
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 2);

        let btn = gtk::Button::new();
        btn.add_css_class("kalam-tag-chip");
        let weight = (*count * 3) / max;
        btn.add_css_class(match weight {
            0 => "kalam-tag-w1",
            1 => "kalam-tag-w2",
            2 => "kalam-tag-w3",
            _ => "kalam-tag-w4",
        });

        let inner = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let label = gtk::Label::new(Some(name));
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        inner.append(&label);
        let badge = gtk::Label::new(Some(&count.to_string()));
        badge.add_css_class("kalam-tag-count");
        inner.append(&badge);
        btn.set_child(Some(&inner));

        let tag = name.clone();
        let s = sender.clone();
        btn.connect_clicked(move |_| {
            s.output(TagsOut::OpenTag { tag: tag.clone() }).ok();
        });
        container.append(&btn);

        // Action menu button
        let menu_btn = gtk::MenuButton::new();
        menu_btn.set_icon_name("view-more-symbolic");
        menu_btn.add_css_class("kalam-secondary-btn");
        menu_btn.set_tooltip_text(Some("Tag actions"));

        let popover = gtk::Popover::new();
        let pop_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
        pop_box.set_margin_top(6);
        pop_box.set_margin_bottom(6);
        pop_box.set_margin_start(6);
        pop_box.set_margin_end(6);

        // 1. Rename
        let rename_btn = gtk::Button::with_label("Rename");
        rename_btn.add_css_class("kalam-secondary-btn");
        let name_clone = name.clone();
        let sender_clone = sender.clone();
        let popover_clone = popover.clone();
        rename_btn.connect_clicked(move |anchor| {
            popover_clone.popdown();
            let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
            let entry = gtk::Entry::new();
            entry.set_text(&name_clone);
            entry.set_placeholder_text(Some("New tag name"));
            content.append(&entry);

            let btn_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            btn_box.set_halign(gtk::Align::End);
            let cancel = gtk::Button::with_label("Cancel");
            cancel.add_css_class("kalam-secondary-btn");
            let save = gtk::Button::with_label("Rename");
            save.add_css_class("kalam-primary-btn");
            btn_box.append(&cancel);
            btn_box.append(&save);
            content.append(&btn_box);

            let save_btn = save.clone();
            entry.connect_activate(move |_| {
                save_btn.emit_clicked();
            });

            let dialog = in_app_dialog::present(
                anchor,
                &format!("Rename tag “{name_clone}”"),
                in_app_dialog::DialogExit::UnsavedInput,
                &content,
            );

            if let Some(dlg) = dialog {
                let d = dlg.clone();
                cancel.connect_clicked(move |_| d.close());
                let d = dlg.clone();
                let old = name_clone.clone();
                let s = sender_clone.clone();
                save.connect_clicked(move |_| {
                    let new_name = entry.text().trim().to_string();
                    if !new_name.is_empty() && new_name != old {
                        s.input(TagsMsg::RenameTag { old_name: old.clone(), new_name });
                    }
                    d.close();
                });
            }
        });
        pop_box.append(&rename_btn);

        // 2. Merge
        let merge_btn = gtk::Button::with_label("Merge into…");
        merge_btn.add_css_class("kalam-secondary-btn");
        let name_clone = name.clone();
        let sender_clone = sender.clone();
        let popover_clone = popover.clone();
        merge_btn.connect_clicked(move |anchor| {
            popover_clone.popdown();
            let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
            let label = gtk::Label::new(Some(&format!("Move all books from “{name_clone}” into another tag:")));
            label.set_halign(gtk::Align::Start);
            content.append(&label);

            let entry = gtk::Entry::new();
            entry.set_placeholder_text(Some("Target tag name"));
            content.append(&entry);

            let btn_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            btn_box.set_halign(gtk::Align::End);
            let cancel = gtk::Button::with_label("Cancel");
            cancel.add_css_class("kalam-secondary-btn");
            let merge = gtk::Button::with_label("Merge Tags");
            merge.add_css_class("kalam-primary-btn");
            btn_box.append(&cancel);
            btn_box.append(&merge);
            content.append(&btn_box);

            let merge_btn = merge.clone();
            entry.connect_activate(move |_| {
                merge_btn.emit_clicked();
            });

            let dialog = in_app_dialog::present(
                anchor,
                &format!("Merge tag “{name_clone}”"),
                in_app_dialog::DialogExit::UnsavedInput,
                &content,
            );

            if let Some(dlg) = dialog {
                let d = dlg.clone();
                cancel.connect_clicked(move |_| d.close());
                let d = dlg.clone();
                let src = name_clone.clone();
                let s = sender_clone.clone();
                merge.connect_clicked(move |_| {
                    let target_tag = entry.text().trim().to_string();
                    if !target_tag.is_empty() && target_tag != src {
                        s.input(TagsMsg::MergeTag { source_tag: src.clone(), target_tag });
                    }
                    d.close();
                });
            }
        });
        pop_box.append(&merge_btn);

        // 3. Delete
        let delete_btn = gtk::Button::with_label("Delete Tag");
        delete_btn.add_css_class("kalam-secondary-btn");
        delete_btn.add_css_class("destructive-action");
        let name_clone = name.clone();
        let sender_clone = sender.clone();
        let popover_clone = popover.clone();
        delete_btn.connect_clicked(move |anchor| {
            popover_clone.popdown();
            let content = gtk::Box::new(gtk::Orientation::Vertical, 10);
            let label = gtk::Label::new(Some(&format!(
                "Are you sure you want to delete tag “{name_clone}”? It will be removed from all books."
            )));
            label.set_wrap(true);
            label.set_halign(gtk::Align::Start);
            content.append(&label);

            let btn_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            btn_box.set_halign(gtk::Align::End);
            let cancel = gtk::Button::with_label("Cancel");
            cancel.add_css_class("kalam-secondary-btn");
            let del = gtk::Button::with_label("Delete");
            del.add_css_class("destructive-action");
            btn_box.append(&cancel);
            btn_box.append(&del);
            content.append(&btn_box);

            let dialog = in_app_dialog::present(
                anchor,
                &format!("Delete tag “{name_clone}”?"),
                in_app_dialog::DialogExit::OwnButtons,
                &content,
            );

            if let Some(dlg) = dialog {
                let d = dlg.clone();
                cancel.connect_clicked(move |_| d.close());
                let d = dlg.clone();
                let tag_name = name_clone.clone();
                let s = sender_clone.clone();
                del.connect_clicked(move |_| {
                    s.input(TagsMsg::DeleteTag { tag_name: tag_name.clone() });
                    d.close();
                });
            }
        });
        pop_box.append(&delete_btn);

        popover.set_child(Some(&pop_box));
        menu_btn.set_popover(Some(&popover));
        container.append(&menu_btn);

        cloud.insert(&container, -1);
    }
}

// ---------------------------------------------------------------------------
// Books for one tag
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum TagBooksOut {
    OpenBook { book_id: i64 },
    OpenBookDialog { book_id: i64 },
}

#[derive(Debug)]
pub enum TagBooksMsg {
    SortChanged(SortKey),
    /// A background tag-books query finished. Carries the generation it was
    /// started with so a superseded reply cannot overwrite a newer one.
    Loaded { gen: u64, snap: TagBooksSnapshot },
}

pub struct TagBooksModel {
    service: LibraryService,
    tag: String,
    books: Vec<Book>,
    sort: SortKey,
    /// Stamps each query so stale replies can be dropped. See
    /// [`TagBooksMsg::Loaded`].
    reload_gen: u64,
}

#[relm4::component(pub)]
impl Component for TagBooksModel {
    type Init = (Arc<Catalog>, String);
    type Input = TagBooksMsg;
    type Output = TagBooksOut;
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

                    #[name = "title"]
                    gtk::Label {
                        add_css_class: "kalam-page-title",
                        set_halign: gtk::Align::Start,
                    },
                    gtk::Label {
                        #[watch]
                        set_label: &format!(
                            "{} book{} tagged",
                            model.books.len(),
                            if model.books.len() == 1 { "" } else { "s" }
                        ),
                        add_css_class: "kalam-page-sub",
                        set_halign: gtk::Align::Start,
                    },
                },

                #[name = "sort_box"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 4,
                    set_valign: gtk::Align::Center,
                },
            },

            gtk::ScrolledWindow {
                set_vexpand: true,
                set_hexpand: true,
                set_hscrollbar_policy: gtk::PolicyType::Never,

                #[name = "list"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                },
            },
        }
    }

    fn init(
        (catalog, tag): Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let sort = SortKey::Title;
        let service = LibraryService::new(catalog);
        // Synchronous first read, as on the other pages: the page is not on
        // screen yet, so there is nothing visible to freeze and no grid to
        // preserve. The sort change goes through `reload`, which is a worker.
        let snap = service.tag_books(&tag, sort);
        report_errors(&snap.errors);
        let model = TagBooksModel {
            service,
            tag,
            books: snap.books,
            sort,
            reload_gen: 0,
        };
        let widgets = view_output!();
        widgets.title.set_label(&format!("#{}", model.tag));

        for key in SortKey::ALL {
            let btn = gtk::ToggleButton::with_label(key.label());
            btn.add_css_class("kalam-secondary-btn");
            if *key == SortKey::Title {
                btn.set_active(true);
            }
            let k = *key;
            let s = sender.clone();
            btn.connect_toggled(move |b| {
                if b.is_active() {
                    s.input(TagBooksMsg::SortChanged(k));
                }
            });
            widgets.sort_box.append(&btn);
        }
        group_toggles(&widgets.sort_box);

        rebuild_books(&widgets.list, &model.books, &sender);
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
            TagBooksMsg::SortChanged(sort) => {
                self.sort = sort;
                self.reload(&sender);
            }
            TagBooksMsg::Loaded { gen, snap } => {
                // Drop a reply an older query produced — clicking through the
                // sort buttons quickly starts several.
                if gen != self.reload_gen {
                    return;
                }
                if snap.errors.is_empty() {
                    self.books = snap.books;
                } else {
                    // Keep the grid on screen rather than blanking it.
                    report_errors(&snap.errors);
                }
            }
        }
        rebuild_books(&widgets.list, &self.books, &sender);
        self.update_view(widgets, sender);
    }
}

impl TagBooksModel {
    /// Ask for one tag's books on a worker thread (roadmap 1.2b).
    ///
    /// The grid keeps what it is showing and swaps on arrival, so clicking
    /// through the sort buttons never blanks it while the query runs.
    fn reload(&mut self, sender: &ComponentSender<Self>) {
        self.reload_gen += 1;
        let gen = self.reload_gen;
        let catalog = self.service.catalog().clone();
        let tag = self.tag.clone();
        let sort = self.sort;
        let done = sender.clone();
        crate::tasks::spawn(
            move |_reporter| LibraryService::new(catalog).tag_books(&tag, sort),
            // One query, not a sequence of steps, so nothing to report.
            |_update| {},
            move |snap| done.input(TagBooksMsg::Loaded { gen, snap }),
        );
    }
}

/// Surface read failures instead of rendering them as an empty page. The
/// service collects them; deciding what the user sees stays with the UI.
fn report_errors(errors: &[String]) {
    for err in errors {
        crate::notify::error("Could not read the library", err);
    }
}

fn group_toggles(box_: &gtk::Box) {
    let mut leader: Option<gtk::ToggleButton> = None;
    let mut child = box_.first_child();
    while let Some(w) = child {
        let next = w.next_sibling();
        if let Ok(btn) = w.downcast::<gtk::ToggleButton>() {
            match &leader {
                Some(l) => btn.set_group(Some(l)),
                None => leader = Some(btn),
            }
        }
        child = next;
    }
}

fn rebuild_books(list: &gtk::Box, books: &[Book], sender: &ComponentSender<TagBooksModel>) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if books.is_empty() {
        let empty = gtk::Label::new(Some("No books carry this tag any more."));
        empty.add_css_class("kalam-placeholder");
        empty.set_halign(gtk::Align::Start);
        list.append(&empty);
        return;
    }

    let s1 = sender.clone();
    let s2 = sender.clone();
    let grid = build_book_grid(
        books,
        move |id| {
            s1.output(TagBooksOut::OpenBook { book_id: id }).ok();
        },
        move |id| {
            s2.output(TagBooksOut::OpenBookDialog { book_id: id }).ok();
        },
    );
    list.append(&grid);
}
