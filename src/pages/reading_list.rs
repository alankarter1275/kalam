//! P4 — Reading list: an ordered to-be-read queue.

use crate::db::{Catalog, ReadingListEntry, SortKey};
use crate::service::LibraryService;
use crate::widgets::book_row::cover_widget_deferred;
use crate::widgets::in_app_dialog;
use gtk::prelude::*;
use relm4::prelude::*;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Debug)]
pub enum ReadingListOut {
    OpenBook { book_id: i64 },
    OpenReader { book_id: i64 },
    /// A comic series row — chapters are only ever seen on the series page.
    /// (The Read button stays chapter-precise: it resumes exactly where the
    /// reader left that series.)
    ComicSeries { series_name: String },
}

#[derive(Debug)]
pub enum ReadingListMsg {
    Move { book_id: i64, delta: i64 },
    Remove(i64),
    AddBooks,
    Refresh,
}

pub struct ReadingListModel {
    service: LibraryService,
    entries: Vec<ReadingListEntry>,
}

#[relm4::component(pub)]
impl Component for ReadingListModel {
    type Init = Arc<Catalog>;
    type Input = ReadingListMsg;
    type Output = ReadingListOut;
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
                        set_label: "Reading list",
                        add_css_class: "kalam-page-title",
                        set_halign: gtk::Align::Start,
                    },
                    gtk::Label {
                        #[watch]
                        set_label: &status_line(model.entries.len()),
                        add_css_class: "kalam-page-sub",
                        set_halign: gtk::Align::Start,
                    },
                },

                gtk::Button {
                    set_label: "+ Add books",
                    add_css_class: "kalam-primary-btn",
                    set_valign: gtk::Align::Center,
                    connect_clicked => ReadingListMsg::AddBooks,
                },
            },

            gtk::ScrolledWindow {
                set_vexpand: true,
                set_hexpand: true,
                set_hscrollbar_policy: gtk::PolicyType::Never,

                #[name = "list"]
                gtk::Box {
                    set_orientation: gtk::Orientation::Vertical,
                    set_spacing: 8,
                },
            },
        }
    }

    fn init(
        catalog: Self::Init,
        _root: Self::Root,
        sender: ComponentSender<Self>,
    ) -> ComponentParts<Self> {
        let service = LibraryService::new(catalog);
        let snap = service.reading_list();
        report_errors(&snap.errors);
        let model = ReadingListModel {
            service,
            entries: snap.entries,
        };
        let widgets = view_output!();
        rebuild(&widgets.list, &model.entries, &sender);
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
            ReadingListMsg::Move { book_id, delta } => {
                // The list visibly reorders itself, so success needs no toast.
                crate::notify::report(
                    self.service
                        .catalog()
                        .move_reading_list_entry(book_id, delta),
                    "Could not reorder the reading list",
                );
                self.reload();
            }
            ReadingListMsg::Remove(book_id) => {
                let entry = self.entries.iter().find(|e| e.book.id == book_id);
                let title = entry.map(|e| e.book.title.clone()).unwrap_or_default();
                // The list is collapsed, so a comic row is the whole series —
                // Remove clears every chapter of it, not just the
                // representative.
                let is_comic = entry
                    .map(|e| {
                        matches!(
                            e.book.format,
                            crate::models::BookFormat::Cbz | crate::models::BookFormat::Cbr
                        )
                    })
                    .unwrap_or(false);
                let result = if is_comic {
                    self.service
                        .catalog()
                        .remove_series_from_reading_list(book_id)
                } else {
                    self.service.catalog().remove_from_reading_list(book_id)
                };
                crate::notify::outcome_info(
                    result,
                    "Removed from reading list",
                    &title,
                    "Could not update the reading list",
                );
                self.reload();
            }
            ReadingListMsg::AddBooks => {
                let s = sender.clone();
                open_picker(root, self.service.catalog().clone(), move || {
                    s.input(ReadingListMsg::Refresh)
                });
            }
            ReadingListMsg::Refresh => self.reload(),
        }
        rebuild(&widgets.list, &self.entries, &sender);
        self.update_view(widgets, sender);
    }
}

impl ReadingListModel {
    fn reload(&mut self) {
        let snap = self.service.reading_list();
        report_errors(&snap.errors);
        self.entries = snap.entries;
    }
}

/// Surface read failures instead of rendering them as an empty queue. The
/// service collects them; deciding what the user sees stays with the UI.
fn report_errors(errors: &[String]) {
    for err in errors {
        crate::notify::error("Could not read the reading list", err);
    }
}

fn status_line(n: usize) -> String {
    if n == 0 {
        "Books you plan to read next, in the order you want them.".into()
    } else {
        format!("{n} book{} queued up", if n == 1 { "" } else { "s" })
    }
}

fn rebuild(
    list: &gtk::Box,
    entries: &[ReadingListEntry],
    sender: &ComponentSender<ReadingListModel>,
) {
    while let Some(child) = list.first_child() {
        list.remove(&child);
    }

    if entries.is_empty() {
        let empty = gtk::Label::new(Some(concat!(
            "Your reading list is empty.\n\n",
            "Add books here from the book page, the float panel, or “+ Add books” above.",
        )));
        empty.add_css_class("kalam-placeholder");
        empty.set_wrap(true);
        empty.set_halign(gtk::Align::Start);
        list.append(&empty);
        return;
    }

    let last = entries.len().saturating_sub(1);
    crate::timing::span("reading_list_build");
    for (i, entry) in entries.iter().enumerate() {
        list.append(&build_row(i, last, entry, sender));
    }
    crate::timing::span_end("reading_list_build");
    crate::timing::note("reading_list_rows", entries.len());

    // The warming half of the deferred-cover contract: placeholders fill
    // only if this page queues the decode at the exact (path, 44, 70) key.
    // After the first warm the cache serves every rebuild instantly, so the
    // reorder/remove paths stay cheap too.
    let books: Vec<crate::models::Book> = entries.iter().map(|e| e.book.clone()).collect();
    crate::preload::warm_books(&books, 0, 44, 70);
}

fn build_row(
    index: usize,
    last: usize,
    entry: &ReadingListEntry,
    sender: &ComponentSender<ReadingListModel>,
) -> gtk::Box {
    let book = &entry.book;
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("kalam-list-row");

    let ordinal = gtk::Label::new(Some(&format!("{}", index + 1)));
    ordinal.add_css_class("kalam-list-ordinal");
    ordinal.set_valign(gtk::Align::Center);
    ordinal.set_width_chars(2);
    row.append(&ordinal);

    // Deferred, not decode-on-the-spot: this is a list surface, and the
    // owner's log measured the page opening in 322 ms with a 0.7 ms read —
    // the covers were the cost. Rows get placeholders instantly; the decode
    // is queued by `rebuild` (the other half of the contract, pitfalls §53).
    let cover = cover_widget_deferred(book.cover_path.as_deref(), 44, 70);
    cover.set_valign(gtk::Align::Center);
    row.append(&cover);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
    text.set_hexpand(true);
    text.set_valign(gtk::Align::Center);

    let title = gtk::Label::new(Some(&book.title));
    title.add_css_class("kalam-card-title");
    title.set_halign(gtk::Align::Start);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    title.set_xalign(0.0);
    text.append(&title);

    let meta = gtk::Label::new(Some(&format!(
        "{} · {}",
        book.authors_display(),
        if book.progress > 0 {
            format!("{}% read", book.progress)
        } else {
            "not started".to_string()
        }
    )));
    meta.add_css_class("kalam-card-meta");
    meta.set_halign(gtk::Align::Start);
    meta.set_ellipsize(gtk::pango::EllipsizeMode::End);
    meta.set_xalign(0.0);
    text.append(&meta);
    row.append(&text);

    let book_id = book.id;

    let up = gtk::Button::new();
    up.set_child(Some(&crate::icons::symbolic_with_classes(
        "go-up-symbolic",
        16,
        &["kalam-inline-icon"],
    )));
    up.add_css_class("kalam-mini-btn");
    up.set_valign(gtk::Align::Center);
    up.set_sensitive(index > 0);
    {
        let s = sender.clone();
        up.connect_clicked(move |_| s.input(ReadingListMsg::Move { book_id, delta: -1 }));
    }
    row.append(&up);

    let down = gtk::Button::new();
    down.set_child(Some(&crate::icons::symbolic_with_classes(
        "go-down-symbolic",
        16,
        &["kalam-inline-icon"],
    )));
    down.add_css_class("kalam-mini-btn");
    down.set_valign(gtk::Align::Center);
    down.set_sensitive(index < last);
    {
        let s = sender.clone();
        down.connect_clicked(move |_| s.input(ReadingListMsg::Move { book_id, delta: 1 }));
    }
    row.append(&down);

    let read = gtk::Button::with_label("Read");
    read.add_css_class("kalam-mini-btn");
    read.set_valign(gtk::Align::Center);
    {
        let s = sender.clone();
        read.connect_clicked(move |_| {
            s.output(ReadingListOut::OpenReader { book_id }).ok();
        });
    }
    row.append(&read);

    let remove = gtk::Button::new();
    remove.set_child(Some(&crate::icons::symbolic_with_classes(
        "window-close-symbolic",
        16,
        &["kalam-inline-icon"],
    )));
    remove.add_css_class("kalam-mini-btn");
    remove.add_css_class("kalam-mini-btn-danger");
    remove.set_valign(gtk::Align::Center);
    remove.set_tooltip_text(Some("Remove from reading list"));
    {
        let s = sender.clone();
        remove.connect_clicked(move |_| s.input(ReadingListMsg::Remove(book_id)));
    }
    row.append(&remove);

    // Clicking the row opens the book page — or the series page for a comic,
    // the same rule every list obeys.
    let click = gtk::GestureClick::new();
    click.set_button(1);
    let comic_series = if matches!(
        book.format,
        crate::models::BookFormat::Cbz | crate::models::BookFormat::Cbr
    ) {
        book.series.clone()
    } else {
        None
    };
    {
        let s = sender.clone();
        click.connect_released(move |_, _, _, _| {
            if let Some(ser) = comic_series.as_ref() {
                s.output(ReadingListOut::ComicSeries {
                    series_name: ser.clone(),
                })
                .ok();
                return;
            }
            s.output(ReadingListOut::OpenBook { book_id }).ok();
        });
    }
    row.add_controller(click);
    row.set_cursor_from_name(Some("pointer"));

    row
}

/// Library checklist for bulk-queueing books.
///
/// A1: drawn inside the window rather than as a `gtk::Window`, which Sway
/// treated as an ordinary tile. Exit is
/// [`in_app_dialog::DialogExit::OwnButtons`]: ticking a box writes straight to
/// the DB, so there is nothing unsaved and "Done" is just "I am finished
/// looking". Backdrop-click and Esc mean the same thing here.
fn open_picker(anchor: &gtk::Box, catalog: Arc<Catalog>, on_changed: impl Fn() + 'static) {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 10);
    // Fit the window, whatever its width (same rule as the shelf picker).
    let win_w = anchor
        .root()
        .map(|r| r.upcast_ref::<gtk::Widget>().width())
        .unwrap_or(0);
    let want = if win_w > 0 {
        520.min((win_w - 120).max(320))
    } else {
        520
    };
    root.set_size_request(want, 520);
    // Same data-not-screenshots verification as the shelf picker.
    if win_w > 0 {
        crate::timing::note("picker_window", win_w as usize);
    }

    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search library…"));
    root.append(&search);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list)
        .build();
    root.append(&scroll);

    let on_changed = Rc::new(on_changed);

    // One picker row, assembled off the UI thread: everything a row's
    // CheckButton needs, so the only work left on arrival is widgets.
    struct PickerRow {
        id: i64,
        title: String,
        line: String,
        ticked: bool,
    }

    // Skeleton first. The owner's log measured the open blocking the UI
    // thread for 351 ms (2026-10-04): the whole-library read, the
    // reading-list membership, the per-comic peers lookup and every row's
    // widget were all built before the panel could appear. The panel now
    // shows immediately with "Loading…"; the read runs on a worker and the
    // rows swap in on arrival.
    let loading = gtk::Label::new(Some("Loading…"));
    loading.add_css_class("kalam-muted");
    loading.set_halign(gtk::Align::Start);
    list.append(&loading);

    // A keystroke supersedes the read in flight; its answer is dropped on
    // arrival (the history page's generation rule — the list keeps what it
    // is showing and swaps, it never blanks while a read runs).
    let gen = Rc::new(std::cell::Cell::new(0u64));
    // The width diagnostics fire once, after the first rows arrive — the
    // skeleton's width says nothing about cut-off rows.
    let measured = Rc::new(std::cell::Cell::new(false));

    let fill = {
        let catalog = catalog.clone();
        let list = list.clone();
        let on_changed = on_changed.clone();
        let root = root.clone();
        let gen = gen.clone();
        let measured = measured.clone();
        Rc::new(move |query: &str| {
            gen.set(gen.get() + 1);
            let my_gen = gen.get();
            let read_catalog = catalog.clone();
            let done_catalog = catalog.clone();
            let done_list = list.clone();
            let done_on_changed = on_changed.clone();
            let done_root = root.clone();
            let done_gen = gen.clone();
            let done_measured = measured.clone();
            let query = query.to_string();
            crate::tasks::spawn(
                "Loading books",
                move |_reporter| {
                    let _read = crate::timing::measure("picker_read");
                    let mut books = read_catalog
                        .list_books(SortKey::Title, &query)
                        .unwrap_or_default();
                    // One row per comic series, never per chapter — the
                    // owner's rule holds in pickers too.
                    read_catalog.collapse_comic_chapters(&mut books);
                    books
                        .into_iter()
                        .map(|book| {
                            // A collapsed comic row is the whole series:
                            // ticked when any chapter is queued; tick/untick
                            // add/remove every chapter. Regular books check
                            // themselves. The lookup is off the UI thread
                            // now, so it costs the dialog nothing.
                            let peers = if matches!(
                                book.format,
                                crate::models::BookFormat::Cbz | crate::models::BookFormat::Cbr
                            ) {
                                read_catalog
                                    .comic_series_peers(book.id)
                                    .unwrap_or_else(|_| vec![book.id])
                            } else {
                                vec![book.id]
                            };
                            PickerRow {
                                id: book.id,
                                title: book.title.clone(),
                                line: format!("{} — {}", book.title, book.authors_display()),
                                ticked: read_catalog
                                    .any_in_reading_list(&peers)
                                    .unwrap_or(false),
                            }
                        })
                        .collect::<Vec<_>>()
                },
                |_| {},
                move |rows| {
                    // A newer fill was asked for while this read ran; keep
                    // what is on screen and drop this answer.
                    if my_gen != done_gen.get() {
                        return;
                    }
                    let _fill = crate::timing::measure("picker_fill");
                    crate::timing::note("picker_rows", rows.len());
                    while let Some(child) = done_list.first_child() {
                        done_list.remove(&child);
                    }
                    if rows.is_empty() {
                        let empty = gtk::Label::new(Some("No books match."));
                        empty.add_css_class("kalam-muted");
                        empty.set_halign(gtk::Align::Start);
                        done_list.append(&empty);
                    } else {
                        for row in rows {
                            // Own label, not the button's internal one — see
                            // the shelf picker (picker_width 1906 on a 1350
                            // window, 2026-10-03).
                            let check = gtk::CheckButton::new();
                            let label = gtk::Label::new(Some(&row.line));
                            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
                            label.set_max_width_chars(44);
                            label.set_xalign(0.0);
                            label.set_hexpand(true);
                            check.set_child(Some(&label));
                            check.set_hexpand(true);
                            check.set_halign(gtk::Align::Fill);
                            check.add_css_class("kalam-picker-row");
                            check.set_active(row.ticked);

                            let catalog = done_catalog.clone();
                            let on_changed = done_on_changed.clone();
                            let book_id = row.id;
                            let book_title = row.title.clone();
                            check.connect_toggled(move |c| {
                                if c.is_active() {
                                    crate::notify::outcome(
                                        catalog.add_series_to_reading_list(book_id),
                                        "Added to reading list",
                                        &book_title,
                                        "Could not update the reading list",
                                    );
                                } else {
                                    crate::notify::outcome_info(
                                        catalog.remove_series_from_reading_list(book_id),
                                        "Removed from reading list",
                                        &book_title,
                                        "Could not update the reading list",
                                    );
                                }
                                on_changed();
                            });
                            done_list.append(&check);
                        }
                    }
                    // Width diagnostics, once, with rows on screen: natural
                    // vs allocated names the layer if the panel is ever cut
                    // off again (natural huge = the rows; natural small,
                    // width huge = the allocation).
                    if !done_measured.get() {
                        done_measured.set(true);
                        let r = done_root.clone();
                        gtk::glib::idle_add_local_once(move || {
                            let (_, nat, _, _) = r.measure(gtk::Orientation::Horizontal, -1);
                            crate::timing::note("picker_natural", nat as usize);
                            crate::timing::note("picker_width", r.width() as usize);
                        });
                    }
                },
            );
        })
    };

    {
        let fill = fill.clone();
        search.connect_search_changed(move |e| fill(&e.text()));
    }

    let done = gtk::Button::with_label("Done");
    done.add_css_class("kalam-primary-btn");
    done.set_halign(gtk::Align::End);
    root.append(&done);

    let Some(dialog) = in_app_dialog::present(
        anchor,
        "Add to reading list",
        in_app_dialog::DialogExit::OwnButtons,
        &root,
    ) else {
        crate::notify::error(
            "Could not open the picker",
            "Please try again once the page has finished loading.",
        );
        return;
    };
    done.connect_clicked(move |_| dialog.close());

    // The first read starts only once the panel is on screen — the skeleton
    // it replaces is the point of the fix.
    fill("");
}
