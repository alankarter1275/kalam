use crate::db::{Annotation, Catalog};
use crate::models::Book;
use crate::service::{LibraryService, QuotesSnapshot};
use gtk::prelude::*;
use relm4::prelude::*;
use std::sync::Arc;

#[derive(Debug)]
#[allow(dead_code)]
pub enum SavedQuotesOut {
    OpenBook { book_id: i64 },
    JumpTo { book_id: i64, chapter_index: usize },
}

#[derive(Debug)]
pub enum SavedQuotesMsg {
    SearchChanged(String),
    Delete(i64),
    Export,
    ExportAllData,
    Refresh,
    SaveNote { id: i64, note: String },
    /// A background quotes query finished. Carries the generation it was
    /// started with so a superseded reply cannot overwrite a newer one.
    Loaded { gen: u64, snap: QuotesSnapshot },
}

pub struct SavedQuotesModel {
    service: LibraryService,
    query: String,
    quotes: Vec<(Annotation, Option<Book>)>,
    status: String,
    /// Stamps each query so stale replies can be dropped. See
    /// [`SavedQuotesMsg::Loaded`].
    reload_gen: u64,
}

#[relm4::component(pub)]
impl Component for SavedQuotesModel {
    type Init = Arc<Catalog>;
    type Input = SavedQuotesMsg;
    type Output = SavedQuotesOut;
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

                gtk::Label {
                    set_label: "Saved quotes",
                    add_css_class: "kalam-page-title",
                    set_halign: gtk::Align::Start,
                    set_hexpand: true,
                },
                gtk::Button {
                    set_label: "Export Quotes",
                    add_css_class: "kalam-secondary-btn",
                    set_tooltip_text: Some("Export all quotes to ~/Quotes.md"),
                    connect_clicked => SavedQuotesMsg::Export,
                },
                gtk::Button {
                    set_label: "Export All Data",
                    add_css_class: "kalam-secondary-btn",
                    set_tooltip_text: Some("Export all vocabulary, quotes, and highlights to ~/Kalam-Export.md"),
                    connect_clicked => SavedQuotesMsg::ExportAllData,
                },
                gtk::Button {
                    set_child: Some(&crate::icons::symbolic_with_classes(
                        "view-refresh-symbolic",
                        16,
                        &["kalam-inline-icon"],
                    )),
                    add_css_class: "kalam-secondary-btn",
                    set_tooltip_text: Some("Refresh"),
                    connect_clicked => SavedQuotesMsg::Refresh,
                },
            },
            gtk::Box {
                set_orientation: gtk::Orientation::Horizontal,
                set_spacing: 6,
                #[name = "search_entry"]
                gtk::SearchEntry {
                    set_placeholder_text: Some("Search quotes…"),
                    set_hexpand: true,
                    connect_search_changed[sender] => move |e| {
                        sender.input(SavedQuotesMsg::SearchChanged(e.text().to_string()));
                    },
                },
            },
            #[name = "status_label"]
            gtk::Label {
                add_css_class: "kalam-muted",
                set_halign: gtk::Align::Start,
                set_wrap: true,
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
        // Synchronous first read, as on the other pages: the page is not on
        // screen yet, so there is nothing visible to freeze and no list to
        // preserve. Every later read goes through `reload`, which is a worker.
        let snap = service.quotes("");
        let status = match snap.errors.first() {
            Some(e) => format!("DB error: {e}"),
            None => status_line(snap.quotes.len(), ""),
        };
        let model = SavedQuotesModel {
            service,
            query: String::new(),
            quotes: snap.quotes,
            status,
            reload_gen: 0,
        };
        let widgets = view_output!();
        rebuild(&widgets.list_box, &model.quotes, &sender);
        widgets.status_label.set_label(&model.status);
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
            SavedQuotesMsg::SearchChanged(q) => {
                self.query = q;
                // The rebuild moved into `Loaded`: rebuilding here would draw
                // the list the query has not replaced yet.
                self.reload(&sender);
            }
            SavedQuotesMsg::Delete(id) => {
                crate::notify::outcome_info(
                    self.service.catalog().delete_annotation(id),
                    "Quote deleted",
                    "",
                    "Could not delete the quote",
                );
                self.reload(&sender);
            }
            SavedQuotesMsg::Export => {
                let exported = export_quotes_markdown(&self.quotes);
                let out_path = dirs::home_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("Quotes.md");
                let count = self.quotes.len();
                match std::fs::write(&out_path, exported) {
                    Ok(()) => {
                        self.status = format!("Exported to {}", out_path.display());
                        crate::notify::success(
                            &format!(
                                "{count} quote{} exported",
                                if count == 1 { "" } else { "s" }
                            ),
                            &out_path.display().to_string(),
                        );
                    }
                    Err(err) => {
                        self.status = format!("Export failed: {err}");
                        crate::notify::error("Could not export quotes", &err.to_string());
                    }
                }
                widgets.status_label.set_label(&self.status);
            }
            SavedQuotesMsg::ExportAllData => {
                let out_path = crate::paths::home_dir()
                    .unwrap_or_else(|| std::path::PathBuf::from("."))
                    .join("Kalam-Export.md");
                match self.service.catalog().export_reading_data_markdown(&out_path) {
                    Ok(count) => {
                        self.status = format!("Exported all data to {}", out_path.display());
                        crate::notify::success(
                            &format!(
                                "{count} item{} exported",
                                if count == 1 { "" } else { "s" }
                            ),
                            &out_path.display().to_string(),
                        );
                    }
                    Err(err) => {
                        self.status = format!("Export failed: {err}");
                        crate::notify::error("Could not export reading data", &err.to_string());
                    }
                }
                widgets.status_label.set_label(&self.status);
            }
            SavedQuotesMsg::SaveNote { id, note } => {
                crate::notify::outcome(
                    self.service
                        .catalog()
                        .update_annotation_note(id, note.trim()),
                    "Note saved",
                    "",
                    "Could not save your note",
                );
                self.reload(&sender);
            }
            SavedQuotesMsg::Refresh => self.reload(&sender),
            SavedQuotesMsg::Loaded { gen, snap } => {
                // Drop a reply an older query produced.
                if gen != self.reload_gen {
                    return;
                }
                // Failures are reported in the status line rather than a
                // toast, as this page has always done, in the wording it
                // already used. But a failed read no longer clears the list —
                // the synchronous version did, which made a read error look
                // like every quote had been deleted.
                self.status = match snap.errors.first() {
                    Some(e) => format!("DB error: {e}"),
                    None => {
                        let n = snap.quotes.len();
                        self.quotes = snap.quotes;
                        status_line(n, &self.query)
                    }
                };
                rebuild(&widgets.list_box, &self.quotes, &sender);
                widgets.status_label.set_label(&self.status);
            }
        }
        self.update_view(widgets, sender);
    }
}

impl SavedQuotesModel {
    /// Ask for the saved quotes on a worker thread (roadmap 1.2b).
    ///
    /// The list keeps what it is showing and swaps on arrival, so typing in
    /// the search box never blanks it while the query runs.
    fn reload(&mut self, sender: &ComponentSender<Self>) {
        self.reload_gen += 1;
        let gen = self.reload_gen;
        let catalog = self.service.catalog().clone();
        let query = self.query.clone();
        let done = sender.clone();
        crate::tasks::spawn(
            move |_reporter| LibraryService::new(catalog).quotes(&query),
            // One query, not a sequence of steps, so nothing to report.
            |_update| {},
            move |snap| done.input(SavedQuotesMsg::Loaded { gen, snap }),
        );
    }
}

/// The status line for a completed read. Shared by the synchronous first read
/// in `init` and by the asynchronous ones, so the two cannot drift apart.
fn status_line(n: usize, query: &str) -> String {
    if query.trim().is_empty() {
        if n == 0 {
            "No saved quotes yet — highlight or save quotes while reading.".into()
        } else {
            format!("{n} quote{} saved", if n == 1 { "" } else { "s" })
        }
    } else {
        format!(
            "{n} result{} for \"{}\"",
            if n == 1 { "" } else { "s" },
            query
        )
    }
}

fn rebuild(
    list: &gtk::Box,
    quotes: &[(Annotation, Option<Book>)],
    sender: &ComponentSender<SavedQuotesModel>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }
    if quotes.is_empty() {
        let l = gtk::Label::new(Some("Nothing to show here."));
        l.add_css_class("kalam-placeholder");
        list.append(&l);
        return;
    }
    for (anno, book_opt) in quotes {
        let row = gtk::Box::new(gtk::Orientation::Vertical, 6);
        row.add_css_class("kalam-quote-row");
        row.set_margin_bottom(8);
        row.set_margin_top(4);

        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.set_halign(gtk::Align::Fill);

        let title = if let Some(b) = book_opt {
            b.title.clone()
        } else {
            format!("Book #{}", anno.book_id)
        };
        let title_l = gtk::Label::new(Some(&format!("{} — ch {}", title, anno.chapter_index + 1)));
        title_l.add_css_class("kalam-muted");
        title_l.set_halign(gtk::Align::Start);
        title_l.set_hexpand(true);
        header.append(&title_l);

        let color_badge = gtk::Label::new(Some(&anno.color));
        color_badge.add_css_class("kalam-chip");
        color_badge.add_css_class(&format!("kalam-badge-{}", anno.color));
        header.append(&color_badge);

        let del_btn = gtk::Button::new();
        del_btn.set_child(Some(&crate::icons::symbolic_with_classes(
            "window-close-symbolic",
            16,
            &["kalam-inline-icon"],
        )));
        del_btn.add_css_class("kalam-secondary-btn");
        let id = anno.id;
        let s = sender.clone();
        del_btn.connect_clicked(move |_| s.input(SavedQuotesMsg::Delete(id)));
        header.append(&del_btn);

        row.append(&header);

        let quote_l = gtk::Label::new(Some(&anno.text_excerpt));
        quote_l.add_css_class("kalam-quote-text");
        quote_l.set_wrap(true);
        quote_l.set_xalign(0.0);
        quote_l.set_halign(gtk::Align::Start);
        row.append(&quote_l);

        // Your own thoughts on the quote — editable in place.
        let note_wrap = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        let note_entry = gtk::Entry::new();
        note_entry.set_placeholder_text(Some("Add your thoughts…"));
        note_entry.set_text(&anno.note);
        note_entry.set_hexpand(true);
        note_entry.add_css_class("kalam-note-entry");
        note_wrap.append(&note_entry);

        let save_note = gtk::Button::with_label("Save");
        save_note.add_css_class("kalam-mini-btn");
        {
            let entry = note_entry.clone();
            let s = sender.clone();
            save_note.connect_clicked(move |_| {
                s.input(SavedQuotesMsg::SaveNote {
                    id,
                    note: entry.text().to_string(),
                });
            });
        }
        note_wrap.append(&save_note);
        // Enter saves too.
        {
            let s = sender.clone();
            note_entry.connect_activate(move |e| {
                s.input(SavedQuotesMsg::SaveNote {
                    id,
                    note: e.text().to_string(),
                });
            });
        }
        row.append(&note_wrap);

        let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let jump_btn = gtk::Button::with_label("Open in book");
        jump_btn.add_css_class("kalam-secondary-btn");
        let bid = anno.book_id;
        let ch = anno.chapter_index as usize;
        let s2 = sender.clone();
        jump_btn.connect_clicked(move |_| {
            s2.output(SavedQuotesOut::JumpTo {
                book_id: bid,
                chapter_index: ch,
            })
            .ok();
            s2.output(SavedQuotesOut::OpenBook { book_id: bid }).ok();
        });
        actions.append(&jump_btn);
        row.append(&actions);

        let sep = gtk::Separator::new(gtk::Orientation::Horizontal);
        sep.set_margin_top(8);
        row.append(&sep);

        list.append(&row);
    }
}

/// Export every saved quote to `~/Quotes.md`. Shared between this page and
/// the Settings → Export card, so both always use the same format.
pub fn export_all_quotes_markdown(
    catalog: &Arc<Catalog>,
) -> Result<(usize, std::path::PathBuf), String> {
    let annos = catalog.list_all_quotes("").map_err(|e| format!("{e}"))?;
    // One batch read, not one `get_book` per quote.
    let ids: Vec<i64> = annos.iter().map(|a| a.book_id).collect();
    let books = catalog.books_by_ids(&ids).map_err(|e| format!("{e}"))?;
    let quotes: Vec<(Annotation, Option<Book>)> = annos
        .into_iter()
        .map(|a| {
            let book = books.get(&a.book_id).cloned();
            (a, book)
        })
        .collect();
    let markdown = export_quotes_markdown(&quotes);
    let out_path = dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("Quotes.md");
    std::fs::write(&out_path, markdown).map_err(|e| format!("{e}"))?;
    Ok((quotes.len(), out_path))
}

fn export_quotes_markdown(quotes: &[(Annotation, Option<Book>)]) -> String {
    let mut md = String::new();
    md.push_str("# Kalam — Saved Quotes\n\n");
    md.push_str(&format!("Exported {}\n\n", chrono_like_now()));
    for (anno, book) in quotes {
        let title = book
            .as_ref()
            .map(|b| b.title.as_str())
            .unwrap_or("Unknown Book");
        let authors = book
            .as_ref()
            .map(|b| b.authors_display())
            .unwrap_or("Unknown");
        md.push_str(&format!("## {title} — {authors}\n\n"));
        md.push_str(&format!(
            "> {}\n\n",
            anno.text_excerpt.replace('\n', "\n> ")
        ));
        if !anno.note.trim().is_empty() {
            md.push_str(&format!("**Note:** {}\n\n", anno.note));
        }
        md.push_str(&format!(
            "*Chapter {}, {} — {}{}*\n\n---\n\n",
            anno.chapter_index + 1,
            anno.color,
            anno.created_at,
            if anno.kind == "highlight" {
                " · highlight"
            } else {
                ""
            }
        ));
    }
    md
}

fn chrono_like_now() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{secs}")
}

mod dirs {
    use std::path::PathBuf;
    pub fn home_dir() -> Option<PathBuf> {
        std::env::var_os("HOME").map(PathBuf::from)
    }
}
