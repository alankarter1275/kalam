use crate::db::{BulkMetadataEdit, Catalog, SortKey};
use crate::models::Book;
use crate::service::{AllBooksSnapshot, LibraryService};
use crate::widgets::book_row::{build_book_grid, build_book_grid_selectable};
use crate::widgets::in_app_dialog;
use gtk::prelude::*;
use relm4::prelude::*;
use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Debug)]
pub enum AllBooksOut {
    OpenBook { book_id: i64 },
    OpenBookDialog { book_id: i64 },
}

#[derive(Debug)]
pub enum AllBooksMsg {
    SearchChanged(String),
    SortChanged(SortKey),
    PickFiles,
    FilesChosen(Vec<PathBuf>),
    /// One file finished importing: 1-based index, total, and its title.
    ImportStep {
        done: usize,
        total: usize,
        title: String,
    },
    /// The whole import finished.
    ImportFinished(ImportTally),
    /// A background book-list query finished.
    ///
    /// Carries the generation it was started with, so a slow reply from an
    /// older query cannot land after a newer one and put the wrong list on
    /// screen — which typing quickly into the search box makes easy to hit.
    BooksLoaded { gen: u64, snap: AllBooksSnapshot },
    ToggleSelectionMode,
    ToggleSelectBook(i64),
    SelectAll,
    DeselectAll,
    OpenBulkEditDialog(gtk::Widget),
    ApplyBulkEdit(BulkMetadataEdit),
}

#[derive(Debug, Default)]
pub struct ImportTally {
    pub imported: usize,
    pub dupes: usize,
    pub errors: usize,
    pub restored: usize,
    pub last_title: String,
}

/// The one-line summary shown after an import finishes.
pub fn import_summary(tally: &ImportTally) -> String {
    let restored_note = if tally.restored > 0 {
        format!(" {} kept your earlier metadata edits.", tally.restored)
    } else {
        String::new()
    };
    format!(
        "Import done — {} added, {} already in library, {} failed.{restored_note}{}",
        tally.imported,
        tally.dupes,
        tally.errors,
        if tally.last_title.is_empty() {
            String::new()
        } else {
            format!(" Last: {}", tally.last_title)
        }
    )
}

/// Import `paths` on a worker thread, reporting per file.
pub fn spawn_import(
    catalog: Arc<Catalog>,
    paths: Vec<PathBuf>,
    on_step: impl Fn(usize, usize, String) + 'static,
    on_done: impl FnOnce(ImportTally) + 'static,
) {
    let total = paths.len();
    crate::tasks::spawn(
        move |reporter| {
            let mut tally = ImportTally::default();
            let mut failures: Vec<(String, String)> = Vec::new();
            for (i, path) in paths.iter().enumerate() {
                if reporter.cancelled() {
                    break;
                }
                match crate::epub::import_epub(&catalog, path) {
                    Ok(r) if r.duplicate => {
                        tally.dupes += 1;
                        tally.last_title = r.title.clone();
                    }
                    Ok(r) => {
                        tally.imported += 1;
                        if r.restored {
                            tally.restored += 1;
                        }
                        tally.last_title = r.title.clone();
                    }
                    Err(err) => {
                        tally.errors += 1;
                        let name = path
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_default();
                        failures.push((format!("Could not import {name}"), format!("{err:#}")));
                    }
                }
                reporter.step(i + 1, total, tally.last_title.clone());
            }
            (tally, failures)
        },
        move |update| on_step(update.done, update.total, update.detail),
        move |(tally, failures)| {
            for (title, detail) in &failures {
                crate::notify::error(title, detail);
            }
            on_done(tally);
        },
    );
}

pub struct AllBooksModel {
    service: LibraryService,
    books: Vec<Book>,
    query: String,
    sort: SortKey,
    status: String,
    /// True while a background import is running; disables the button.
    importing: bool,
    selection_mode: bool,
    selected_books: HashSet<i64>,
    /// Monotonic counter for book-list queries. Each request stamps the value
    /// it was started with, and a reply that no longer matches has been
    /// superseded and is dropped.
    reload_gen: u64,
}

#[relm4::component(pub)]
impl Component for AllBooksModel {
    type Init = Arc<Catalog>;
    type Input = AllBooksMsg;
    type Output = AllBooksOut;
    type CommandOutput = ();

    view! {
        #[root]
        gtk::Box {
            set_orientation: gtk::Orientation::Vertical,
            set_spacing: 12,
            set_hexpand: true,

            gtk::Label {
                set_label: "All books",
                add_css_class: "kalam-page-title",
                set_halign: gtk::Align::Start,
            },

            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,

                #[name = "search"]
                gtk::SearchEntry {
                    set_hexpand: true,
                    set_placeholder_text: Some("Search title, author, tag:fantasy, status:unread, rating:>3…"),
                    connect_search_changed[sender] => move |entry| {
                        sender.input(AllBooksMsg::SearchChanged(entry.text().to_string()));
                    },
                },

                #[name = "sort_box"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Horizontal,
                    set_spacing: 4,
                },

                gtk::Button {
                    #[watch]
                    set_label: if model.selection_mode {
                        "Exit Select"
                    } else {
                        "Select Mode"
                    },
                    add_css_class: "kalam-secondary-btn",
                    connect_clicked => AllBooksMsg::ToggleSelectionMode,
                },

                gtk::Button {
                    #[watch]
                    set_label: if model.importing {
                        "Importing…"
                    } else {
                        "+ Import EPUB"
                    },
                    add_css_class: "kalam-primary-btn",
                    #[watch]
                    set_sensitive: !model.importing,
                    connect_clicked => AllBooksMsg::PickFiles,
                },
            },

            #[name = "selection_bar"]
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 8,
                #[watch]
                set_visible: model.selection_mode,

                gtk::Label {
                    #[watch]
                    set_label: &format!(
                        "{} book{} selected",
                        model.selected_books.len(),
                        if model.selected_books.len() == 1 { "" } else { "s" }
                    ),
                    add_css_class: "kalam-page-sub",
                },

                gtk::Button {
                    set_label: "Select All",
                    add_css_class: "kalam-secondary-btn",
                    connect_clicked => AllBooksMsg::SelectAll,
                },

                gtk::Button {
                    set_label: "Deselect All",
                    add_css_class: "kalam-secondary-btn",
                    connect_clicked => AllBooksMsg::DeselectAll,
                },

                gtk::Button {
                    set_label: "Bulk Edit…",
                    add_css_class: "kalam-primary-btn",
                    #[watch]
                    set_sensitive: !model.selected_books.is_empty(),
                    connect_clicked[sender] => move |btn| {
                        sender.input(AllBooksMsg::OpenBulkEditDialog(btn.clone().upcast()));
                    },
                },
            },

            #[name = "status_label"]
            gtk::Label {
                #[watch]
                set_label: &model.status,
                add_css_class: "kalam-muted",
                set_halign: gtk::Align::Start,
            },

            #[name = "list"]
            gtk::Box {
                set_orientation: gtk::Orientation::Vertical,
                set_spacing: 0,
                set_halign: gtk::Align::Start,
                set_hexpand: false,
                set_vexpand: false,
            },
        }
    }

    fn init(
        catalog: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let sort = SortKey::Added;
        let service = LibraryService::new(catalog);
        // Deliberately still synchronous, and the one blocking read left on
        // this page. At this point the page is not on screen, so there is no
        // visible UI to freeze and no existing list to preserve — making it
        // asynchronous would render an empty grid and then fill it, which is
        // a worse first impression than waiting a few milliseconds. Revisit
        // when a large library makes this measurable. Everything *after* the
        // page exists goes through `reload`, which is a worker.
        let snap = service.all_books(sort, "");
        let status = match snap.errors.first() {
            Some(err) => format!("Database error: {err}"),
            None => status_line(snap.books.len(), ""),
        };

        let model = AllBooksModel {
            service,
            books: snap.books,
            query: String::new(),
            sort,
            status,
            importing: false,
            selection_mode: false,
            selected_books: HashSet::new(),
            reload_gen: 0,
        };
        let widgets = view_output!();

        for key in SortKey::ALL {
            let btn = gtk::ToggleButton::with_label(key.label());
            btn.add_css_class("kalam-secondary-btn");
            if *key == SortKey::Added {
                btn.set_active(true);
            }
            let k = *key;
            let s = sender.clone();
            btn.connect_toggled(move |b| {
                if b.is_active() {
                    s.input(AllBooksMsg::SortChanged(k));
                }
            });
            widgets.sort_box.append(&btn);
        }
        group_toggles(&widgets.sort_box);

        rebuild_list(
            &widgets.list,
            &model.books,
            &model.query,
            model.selection_mode,
            &model.selected_books,
            &sender,
        );

        ComponentParts { model, widgets }
    }

    fn update_with_view(
        &mut self,
        widgets: &mut Self::Widgets,
        msg: Self::Input,
        sender: ComponentSender<Self>,
        root: &Self::Root,
    ) {
        match msg {
            AllBooksMsg::ImportStep { done, total, title } => {
                self.status = if title.is_empty() {
                    format!("Importing {done} of {total}…")
                } else {
                    format!("Importing {done} of {total} — {title}")
                };
                widgets.status_label.set_label(&self.status);
                return;
            }
            AllBooksMsg::ImportFinished(tally) => {
                self.importing = false;
                self.reload(&sender);

                if tally.imported > 0 {
                    crate::notify::success(
                        &format!(
                            "{} book{} imported",
                            tally.imported,
                            if tally.imported == 1 { "" } else { "s" }
                        ),
                        &tally.last_title,
                    );
                }

                self.status = import_summary(&tally);
            }
            AllBooksMsg::BooksLoaded { gen, snap } => {
                // Drop a reply an older query produced. Typing quickly starts
                // several, and without this a slow early result can land after
                // a fast later one and put the wrong list on screen.
                if gen != self.reload_gen {
                    return;
                }
                match snap.errors.first() {
                    // Keep the list already on screen. Clearing it, as the
                    // synchronous version did, turns a failed read into
                    // something that looks like data loss.
                    Some(err) => self.status = format!("Database error: {err}"),
                    None => {
                        let n = snap.books.len();
                        self.books = snap.books;
                        if !self.status.starts_with("Import done") {
                            self.status = status_line(n, &self.query);
                        }
                    }
                }
            }
            AllBooksMsg::SearchChanged(q) => {
                self.query = q;
                self.status.clear();
                self.reload(&sender);
            }
            AllBooksMsg::SortChanged(sort) => {
                self.sort = sort;
                self.status.clear();
                self.reload(&sender);
            }
            AllBooksMsg::ToggleSelectionMode => {
                self.selection_mode = !self.selection_mode;
                if !self.selection_mode {
                    self.selected_books.clear();
                }
            }
            AllBooksMsg::ToggleSelectBook(id) => {
                if self.selected_books.contains(&id) {
                    self.selected_books.remove(&id);
                } else {
                    self.selected_books.insert(id);
                }
            }
            AllBooksMsg::SelectAll => {
                for b in &self.books {
                    self.selected_books.insert(b.id);
                }
            }
            AllBooksMsg::DeselectAll => {
                self.selected_books.clear();
            }
            AllBooksMsg::OpenBulkEditDialog(anchor) => {
                let content = gtk::Box::new(gtk::Orientation::Vertical, 12);

                let info = gtk::Label::new(Some(&format!(
                    "Editing metadata for {} selected book{}.",
                    self.selected_books.len(),
                    if self.selected_books.len() == 1 { "" } else { "s" }
                )));
                info.set_halign(gtk::Align::Start);
                info.add_css_class("kalam-muted");
                content.append(&info);

                // Add tags
                let add_tags_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let add_tags_check = gtk::CheckButton::with_label("Add tags:");
                let add_tags_entry = gtk::Entry::new();
                add_tags_entry.set_placeholder_text(Some("e.g. favourite, sci-fi"));
                add_tags_entry.set_hexpand(true);
                add_tags_box.append(&add_tags_check);
                add_tags_box.append(&add_tags_entry);
                content.append(&add_tags_box);

                let chk = add_tags_check.clone();
                add_tags_entry.connect_changed(move |e| {
                    chk.set_active(!e.text().trim().is_empty());
                });

                // Set/Replace tags
                let set_tags_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let set_tags_check = gtk::CheckButton::with_label("Set/Replace tags:");
                let set_tags_entry = gtk::Entry::new();
                set_tags_entry.set_placeholder_text(Some("e.g. classic, fiction"));
                set_tags_entry.set_hexpand(true);
                set_tags_box.append(&set_tags_check);
                set_tags_box.append(&set_tags_entry);
                content.append(&set_tags_box);

                let chk = set_tags_check.clone();
                set_tags_entry.connect_changed(move |e| {
                    chk.set_active(!e.text().trim().is_empty());
                });

                // Author
                let author_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let author_check = gtk::CheckButton::with_label("Update Author:");
                let author_entry = gtk::Entry::new();
                author_entry.set_placeholder_text(Some("e.g. Frank Herbert"));
                author_entry.set_hexpand(true);
                author_box.append(&author_check);
                author_box.append(&author_entry);
                content.append(&author_box);

                let chk = author_check.clone();
                author_entry.connect_changed(move |e| {
                    chk.set_active(!e.text().trim().is_empty());
                });

                // Publisher
                let publisher_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let publisher_check = gtk::CheckButton::with_label("Update Publisher:");
                let publisher_entry = gtk::Entry::new();
                publisher_entry.set_placeholder_text(Some("e.g. Chilton Books"));
                publisher_entry.set_hexpand(true);
                publisher_box.append(&publisher_check);
                publisher_box.append(&publisher_entry);
                content.append(&publisher_box);

                let chk = publisher_check.clone();
                publisher_entry.connect_changed(move |e| {
                    chk.set_active(!e.text().trim().is_empty());
                });

                // Reading status
                let status_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                let status_check = gtk::CheckButton::with_label("Update Reading Status:");
                let status_combo = gtk::DropDown::from_strings(&["Unread", "Reading", "Finished"]);
                status_box.append(&status_check);
                status_box.append(&status_combo);
                content.append(&status_box);

                let chk = status_check.clone();
                status_combo.connect_selected_notify(move |_| {
                    chk.set_active(true);
                });

                // Buttons
                let btn_box = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                btn_box.set_halign(gtk::Align::End);
                let cancel = gtk::Button::with_label("Cancel");
                cancel.add_css_class("kalam-secondary-btn");
                let apply = gtk::Button::with_label("Apply Changes");
                apply.add_css_class("kalam-primary-btn");
                btn_box.append(&cancel);
                btn_box.append(&apply);
                content.append(&btn_box);

                let btn = apply.clone();
                add_tags_entry.connect_activate(move |_| {
                    btn.emit_clicked();
                });
                let btn = apply.clone();
                set_tags_entry.connect_activate(move |_| {
                    btn.emit_clicked();
                });
                let btn = apply.clone();
                author_entry.connect_activate(move |_| {
                    btn.emit_clicked();
                });
                let btn = apply.clone();
                publisher_entry.connect_activate(move |_| {
                    btn.emit_clicked();
                });

                let dialog = in_app_dialog::present(
                    &anchor,
                    "Bulk Edit Metadata",
                    in_app_dialog::DialogExit::UnsavedInput,
                    &content,
                );

                if let Some(dlg) = dialog {
                    let d = dlg.clone();
                    cancel.connect_clicked(move |_| d.close());

                    let d = dlg.clone();
                    let s = sender.clone();
                    apply.connect_clicked(move |_| {
                        let add_tags = if add_tags_check.is_active() {
                            let text = add_tags_entry.text();
                            let tags: Vec<String> = text
                                .split(',')
                                .map(|t| t.trim().to_string())
                                .filter(|t| !t.is_empty())
                                .collect();
                            if !tags.is_empty() {
                                Some(tags)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        let set_tags = if set_tags_check.is_active() {
                            let text = set_tags_entry.text();
                            let tags: Vec<String> = text
                                .split(',')
                                .map(|t| t.trim().to_string())
                                .filter(|t| !t.is_empty())
                                .collect();
                            Some(tags)
                        } else {
                            None
                        };

                        let author = if author_check.is_active() {
                            let text = author_entry.text().trim().to_string();
                            if !text.is_empty() {
                                Some(text)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        let publisher = if publisher_check.is_active() {
                            let text = publisher_entry.text().trim().to_string();
                            if !text.is_empty() {
                                Some(text)
                            } else {
                                None
                            }
                        } else {
                            None
                        };

                        let reading_status = if status_check.is_active() {
                            let idx = status_combo.selected();
                            match idx {
                                0 => Some("Unread".to_string()),
                                1 => Some("Reading".to_string()),
                                2 => Some("Finished".to_string()),
                                _ => None,
                            }
                        } else {
                            None
                        };

                        let edit = BulkMetadataEdit {
                            add_tags,
                            set_tags,
                            author,
                            publisher,
                            reading_status,
                        };

                        s.input(AllBooksMsg::ApplyBulkEdit(edit));
                        d.close();
                    });
                }
            }
            AllBooksMsg::ApplyBulkEdit(edit) => {
                let ids: Vec<i64> = self.selected_books.iter().copied().collect();
                if let Err(err) = self.service.catalog().bulk_update_metadata(&ids, &edit) {
                    crate::notify::error("Bulk edit failed", &err.to_string());
                } else {
                    crate::notify::success(
                        "Bulk edit complete",
                        &format!("Updated metadata for {} books", ids.len()),
                    );
                    self.selected_books.clear();
                    self.selection_mode = false;
                    self.reload(&sender);
                }
            }
            AllBooksMsg::PickFiles => {
                let dialog = gtk::FileDialog::builder()
                    .title("Import EPUB books")
                    .modal(true)
                    .build();

                let filter = gtk::FileFilter::new();
                filter.set_name(Some("Books, Comics, & PDFs (*.epub, *.cbz, *.cbr, *.pdf)"));
                filter.add_suffix("epub");
                filter.add_suffix("cbz");
                filter.add_suffix("cbr");
                filter.add_suffix("pdf");
                filter.add_suffix("pdf");
                filter.add_mime_type("application/epub+zip");
                filter.add_mime_type("application/x-cbz");
                filter.add_mime_type("application/x-cbr");
                filter.add_mime_type("application/pdf");
                let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
                filters.append(&filter);
                dialog.set_filters(Some(&filters));

                let s = sender.clone();
                let window = root.root().and_then(|r| r.downcast::<gtk::Window>().ok());
                let window = window.or_else(|| {
                    relm4::main_application()
                        .active_window()
                        .and_then(|w| w.downcast::<gtk::Window>().ok())
                });

                dialog.open_multiple(
                    window.as_ref(),
                    gtk::gio::Cancellable::NONE,
                    move |result| {
                        if let Ok(files) = result {
                            let mut paths = Vec::new();
                            let n = files.n_items();
                            for i in 0..n {
                                if let Some(obj) = files.item(i) {
                                    if let Ok(file) = obj.downcast::<gtk::gio::File>() {
                                        if let Some(p) = file.path() {
                                            paths.push(p);
                                        }
                                    }
                                }
                            }
                            if !paths.is_empty() {
                                s.input(AllBooksMsg::FilesChosen(paths));
                            }
                        }
                    },
                );
            }
            AllBooksMsg::FilesChosen(paths) => {
                let total = paths.len();
                self.importing = true;
                self.status = format!("Importing 1 of {total}…");

                let catalog = self.service.catalog().clone();
                let step_sender = sender.clone();
                let done_sender = sender.clone();
                spawn_import(
                    catalog,
                    paths,
                    move |done, total, title| {
                        step_sender.input(AllBooksMsg::ImportStep { done, total, title });
                    },
                    move |tally| {
                        done_sender.input(AllBooksMsg::ImportFinished(tally));
                    },
                );
            }
        }

        rebuild_list(
            &widgets.list,
            &self.books,
            &self.query,
            self.selection_mode,
            &self.selected_books,
            &sender,
        );
        self.update_view(widgets, sender);
    }
}

impl AllBooksModel {
    /// Ask for the book list on a worker thread.
    ///
    /// Roadmap 1.2b pilot. `all_books` is one of the queries whose cost grows
    /// with library size, and this is called on **every keystroke** of the
    /// search box, so it is the worst place on this page to block the UI
    /// thread. The grid keeps whatever it is already showing and swaps the
    /// list in when the reply lands — clearing it for the round trip would
    /// replace something instant with a flicker, which is the one way this
    /// change could make the app feel worse rather than merely safer.
    fn reload(&mut self, sender: &ComponentSender<Self>) {
        self.reload_gen += 1;
        let gen = self.reload_gen;
        let catalog = self.service.catalog().clone();
        let sort = self.sort;
        let query = self.query.clone();
        let done = sender.clone();
        crate::tasks::spawn(
            move |_reporter| {
                // A fresh service rather than moving this page's own: `new`
                // only clones an `Arc`, the page's handle stays untouched, and
                // `AllBooksSnapshot` is `Send` — asserted by
                // `snapshots_are_send` — so the reply crosses back safely.
                LibraryService::new(catalog).all_books(sort, &query)
            },
            // Nothing to report: this is one query, not a sequence of steps.
            |_update| {},
            move |snap| done.input(AllBooksMsg::BooksLoaded { gen, snap }),
        );
    }
}

fn status_line(n: usize, query: &str) -> String {
    if query.trim().is_empty() {
        if n == 0 {
            "No books yet — import an EPUB to get started.".into()
        } else {
            format!(
                "{n} book{} · click cover for float · Ctrl+click for full page",
                if n == 1 { "" } else { "s" }
            )
        }
    } else {
        format!("{n} result{}", if n == 1 { "" } else { "s" })
    }
}

fn group_toggles(box_: &gtk::Box) {
    let mut group_leader: Option<gtk::ToggleButton> = None;
    let mut child = box_.first_child();
    while let Some(w) = child {
        let next = w.next_sibling();
        if let Ok(btn) = w.downcast::<gtk::ToggleButton>() {
            if let Some(ref leader) = group_leader {
                btn.set_group(Some(leader));
            } else {
                group_leader = Some(btn);
            }
        }
        child = next;
    }
}

fn rebuild_list(
    list: &gtk::Box,
    books: &[Book],
    query: &str,
    selection_mode: bool,
    selected_books: &HashSet<i64>,
    sender: &ComponentSender<AllBooksModel>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if books.is_empty() {
        let query = query.trim();
        let message = if query.is_empty() {
            "Your library is empty.\nClick “+ Import EPUB” to add books.".to_string()
        } else {
            format!("No books match “{query}”.\nTry another search, or clear it to see everything.")
        };
        let empty = gtk::Label::new(Some(&message));
        empty.add_css_class("kalam-placeholder");
        empty.set_wrap(true);
        list.append(&empty);
        return;
    }

    let s = sender.clone();
    let s2 = sender.clone();

    if selection_mode {
        let sel_set = selected_books.clone();
        let grid = build_book_grid_selectable(
            books,
            move |id| sel_set.contains(&id),
            move |id| {
                s.input(AllBooksMsg::ToggleSelectBook(id));
            },
            move |id| {
                s2.input(AllBooksMsg::ToggleSelectBook(id));
            },
        );
        list.append(&grid);
    } else {
        let grid = build_book_grid(
            books,
            move |id| {
                s.output(AllBooksOut::OpenBook { book_id: id }).ok();
            },
            move |id| {
                s2.output(AllBooksOut::OpenBookDialog { book_id: id }).ok();
            },
        );
        list.append(&grid);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_clean_import_reads_naturally() {
        let tally = ImportTally {
            imported: 3,
            last_title: "Dune".into(),
            ..Default::default()
        };
        assert_eq!(
            import_summary(&tally),
            "Import done — 3 added, 0 already in library, 0 failed. Last: Dune"
        );
    }

    #[test]
    fn restored_edits_are_called_out_but_only_when_there_were_some() {
        // The note exists so a book that comes back with its old title does
        // not look like the import ignored the file.
        let none = ImportTally {
            imported: 1,
            ..Default::default()
        };
        assert!(!import_summary(&none).contains("kept your earlier"));

        let some = ImportTally {
            imported: 1,
            restored: 1,
            ..Default::default()
        };
        assert!(import_summary(&some).contains("1 kept your earlier metadata edits."));
    }

    #[test]
    fn an_empty_title_leaves_off_the_last_clause() {
        // Every file failing means there is no title to report; the summary
        // should not trail off with a dangling "Last: ".
        let tally = ImportTally {
            errors: 2,
            ..Default::default()
        };
        let text = import_summary(&tally);
        assert!(text.ends_with("2 failed."), "got {text:?}");
        assert!(!text.contains("Last:"));
    }
}
