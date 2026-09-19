//! P5 — edit metadata, replace the cover, fetch from Open Library.
//!
//! Built with direct widget handling rather than as a Relm4 component,
//! matching `shelf_editor`: the Open Library results list is built and
//! rebuilt dynamically, which is far simpler that way.
//!
//! A1: shown as an in-app dialog, not a `gtk::Window`. It holds unsaved edits,
//! so backdrop-click is deliberately disabled — Cancel and Esc are the ways
//! out. See `crate::widgets::in_app_dialog`.
//!
//! Network work runs on a worker thread and reports back through a channel on
//! the main context, so the dialog never blocks the UI. Fetched values are
//! staged into the form for review — nothing is written to the catalog until
//! you press Save.

use crate::db::Catalog;
use crate::metadata::{self, Candidate, CoverRef};
use crate::widgets::book_row::cover_widget;
use crate::widgets::charts::star_picker;
use crate::widgets::in_app_dialog;
use gtk::prelude::*;
use relm4::RelmWidgetExt;
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::Arc;

/// Width of the slide-out search panel, and how much the window grows to
/// accommodate it so the form itself never gets squeezed.
const PANEL_WIDTH: i32 = 330;
const BASE_WIDTH: i32 = 780;
/// Total dialog width: form plus room for the search panel. Fixed for the
/// lifetime of the dialog so revealing the panel cannot push it off-screen or
/// leave it visually off-centre.
const DIALOG_WIDTH: i32 = BASE_WIDTH + PANEL_WIDTH;
/// Cover thumbnails keep the standard 1:1.6 book ratio so the grid is even.
const THUMB_W: i32 = 128;
const THUMB_H: i32 = (THUMB_W as f32 * 1.6) as i32;

/// What the worker thread sends back to the UI.
enum FetchMsg {
    /// Merged candidates, plus any per-source failures worth mentioning.
    Results(Vec<Candidate>, Vec<(metadata::SourceId, String)>),
    Failed(String),
    /// Thumbnails to choose between: (cover id, JPEG bytes).
    CoverChoices(Vec<(CoverRef, Vec<u8>)>),
    CoverReady(Vec<u8>),
    CoverFailed(String),
}

/// Open the editor for `book_id`. `on_saved` runs after a successful write.
pub fn open_metadata_editor(
    anchor: &impl IsA<gtk::Widget>,
    catalog: Arc<Catalog>,
    book_id: i64,
    on_saved: impl Fn() + 'static,
) {
    open_editor_inner(anchor.as_ref(), catalog, book_id, Rc::new(on_saved));
}

fn open_editor_inner(
    anchor: &gtk::Widget,
    catalog: Arc<Catalog>,
    book_id: i64,
    on_saved: Rc<dyn Fn()>,
) {
    // Silently returning here meant the editor just never appeared: no
    // dialog, no message, nothing to click. Say which of the two it was.
    let book = match catalog.get_book(book_id) {
        Ok(Some(book)) => book,
        Ok(None) => {
            crate::notify::error(
                "Cannot edit this book",
                "It is no longer in your library — it may have been deleted.",
            );
            return;
        }
        Err(err) => {
            crate::notify::error("Could not open the metadata editor", &err.to_string());
            return;
        }
    };

    // The app window: needed to measure the display, and later as the parent
    // for the cover file chooser, which is a real portal dialog.
    let app_window: Option<gtk::Window> =
        anchor.root().and_then(|r| r.downcast::<gtk::Window>().ok());

    // Clamp to the display so the form fits on small laptop screens. As an
    // in-app dialog it can never exceed the window, but a form wider than the
    // window would still be clipped, so the clamp stays.
    let max_width = app_window
        .as_ref()
        .and_then(|w| w.surface())
        .and_then(|s| gtk::gdk::Display::default().and_then(|d| d.monitor_at_surface(&s)))
        .map(|m| m.geometry().width() - 80)
        .unwrap_or(DIALOG_WIDTH);

    // Outer shell holds the scroller and an always-visible action bar.
    let shell = gtk::Box::new(gtk::Orientation::Vertical, 0);
    // Width only: the height comes from the content, capped by the scroller
    // below so the action bar is always reachable on a short laptop screen.
    shell.set_size_request(DIALOG_WIDTH.min(max_width), -1);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 12);
    root.set_margin_all(18);

    // ── two reflowable columns ──────────────────────────────────────────
    // When the search panel opens, `side` is emptied into `fields` so the form
    // becomes a single column and nothing is squeezed off-screen.
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 20);

    let fields = gtk::Box::new(gtk::Orientation::Vertical, 8);
    fields.set_hexpand(true);

    // Search icons live on the field they act on, Calibre-style.
    let (title_entry, title_search) = labelled_entry_with_search(
        &fields,
        "TITLE",
        &book.title,
        "Search Open Library by title",
    );
    // No search icon here: it sent exactly the same title+author query as the
    // title icon, so it was a second button for one action.
    let authors_entry = labelled_entry(&fields, "AUTHORS", &book.authors);

    // Series and its position sit on one row, as in Calibre.
    fields.append(&section_label("SERIES"));
    let series_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let series_entry = gtk::Entry::new();
    series_entry.set_text(book.series.as_deref().unwrap_or_default());
    series_entry.set_hexpand(true);
    series_row.append(&series_entry);

    // Fractional steps because novellas are routinely "#2.5".
    let series_index = gtk::SpinButton::with_range(0.0, 999.0, 0.5);
    series_index.set_digits(1);
    series_index.set_value(book.series_index as f64);
    series_index.set_tooltip_text(Some("Position in the series — 0 means unset"));
    series_row.append(&series_index);
    fields.append(&series_row);

    let tags_entry = labelled_entry(
        &fields,
        "TAGS / GENRE (COMMA SEPARATED)",
        &book.tags.join(", "),
    );

    // ── movable block: rating, publisher, published ─────────────────────
    // Lives in `side` normally, moves under SERIES when the panel opens.
    let pub_block = gtk::Box::new(gtk::Orientation::Vertical, 8);

    pub_block.append(&section_label("RATING"));
    let rating_host = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let rating_value = Rc::new(std::cell::Cell::new(book.rating));
    {
        let rating_host_inner = rating_host.clone();
        let rating_value = rating_value.clone();
        // Rebuilt on each pick so the filled glyphs follow the click.
        let rebuild: crate::pages::SelfRebuild = Rc::new(RefCell::new(None));
        let rebuild_ref = rebuild.clone();
        let f: Rc<dyn Fn()> = Rc::new(move || {
            while let Some(c) = rating_host_inner.first_child() {
                rating_host_inner.remove(&c);
            }
            let rv = rating_value.clone();
            let again = rebuild_ref.borrow().clone();
            rating_host_inner.append(&star_picker(rating_value.get(), move |v| {
                rv.set(v);
                if let Some(f) = &again {
                    f();
                }
            }));
        });
        *rebuild.borrow_mut() = Some(f.clone());
        f();
    }
    pub_block.append(&rating_host);

    let publisher_entry = labelled_entry(&pub_block, "PUBLISHER", &book.publisher);
    let published_entry = labelled_entry(&pub_block, "PUBLISHED", &book.published);

    // ── movable block: description ──────────────────────────────────────
    let desc_block = gtk::Box::new(gtk::Orientation::Vertical, 8);
    desc_block.append(&section_label("DESCRIPTION"));
    let desc_view = gtk::TextView::new();
    desc_view.set_wrap_mode(gtk::WrapMode::WordChar);
    desc_view.add_css_class("kalam-desc-view");
    desc_view
        .buffer()
        .set_text(&crate::epub::strip_html(&book.description));
    let desc_scroll = gtk::ScrolledWindow::builder()
        .min_content_height(150)
        .max_content_height(150)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&desc_view)
        .build();
    desc_scroll.add_css_class("kalam-desc-scroll");
    desc_block.append(&desc_scroll);

    // ── movable block: cover ────────────────────────────────────────────
    let cover_block = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let cover_head = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let cover_label = section_label("COVER");
    cover_label.set_hexpand(true);
    cover_head.append(&cover_label);
    let cover_search = icon_button("system-search-symbolic", "Search Open Library for a cover");
    cover_head.append(&cover_search);
    cover_block.append(&cover_head);

    let cover_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
    cover_host.append(&cover_widget(book.cover_path.as_deref(), 150, 240));
    cover_host.set_halign(gtk::Align::Start);
    cover_block.append(&cover_host);

    let pick_cover = gtk::Button::with_label("From file…");
    pick_cover.add_css_class("kalam-secondary-btn");
    pick_cover.set_halign(gtk::Align::Start);
    cover_block.append(&pick_cover);

    // Default arrangement: form left, publication + cover right.
    fields.append(&desc_block);
    top.append(&fields);

    let side = gtk::Box::new(gtk::Orientation::Vertical, 8);
    side.set_valign(gtk::Align::Start);
    side.set_size_request(230, -1);
    side.add_css_class("kalam-metadata-side");
    side.append(&pub_block);
    side.append(&cover_block);
    top.append(&side);
    root.append(&top);

    // ── Open Library: a slide-out panel, not an inline section ──────────
    // Built here but revealed only on demand, so the form stays uncluttered.
    let search_entry = gtk::Entry::new();
    search_entry.set_hexpand(true);
    search_entry.set_placeholder_text(Some("Title and author…"));
    // Seed with what we already know so one click usually suffices.
    search_entry.set_text(format!("{} {}", book.title, book.authors).trim());

    let search_btn = gtk::Button::with_label("Search");
    search_btn.add_css_class("kalam-primary-btn");

    // Search progress lives in the panel; form-level messages use `status`,
    // which sits in the action bar and is always visible.
    let search_status = gtk::Label::new(Some("Type a title and press Search."));
    search_status.add_css_class("kalam-muted");
    search_status.set_halign(gtk::Align::Start);
    search_status.set_wrap(true);
    search_status.set_xalign(0.0);
    // A raw network error carries the whole request URL. Without a cap the
    // label's natural width is that URL, which stretched the panel across the
    // dialog; without a line limit it pushed the results out of view.
    search_status.set_width_chars(1);
    search_status.set_max_width_chars(44);
    search_status.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    search_status.set_lines(4);
    search_status.set_ellipsize(gtk::pango::EllipsizeMode::End);

    let results = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let results_scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&results)
        .build();

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 10);
    panel.add_css_class("kalam-search-panel");
    panel.set_size_request(PANEL_WIDTH, -1);

    let panel_head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let panel_title = gtk::Label::new(Some("FIND METADATA"));
    panel_title.add_css_class("kalam-detail-section-title");
    panel_title.set_halign(gtk::Align::Start);
    panel_title.set_hexpand(true);
    panel_head.append(&panel_title);
    let panel_close = gtk::Button::new();
    panel_close.set_child(Some(&crate::icons::symbolic_with_classes(
        "window-close-symbolic",
        16,
        &["kalam-inline-icon"],
    )));
    panel_close.add_css_class("kalam-rule-remove");
    panel_close.set_tooltip_text(Some("Close search"));
    panel_head.append(&panel_close);
    panel.append(&panel_head);

    let search_row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    search_row.append(&search_entry);
    search_row.append(&search_btn);
    panel.append(&search_row);
    panel.append(&search_status);
    panel.append(&results_scroll);

    let hint = gtk::Label::new(Some(
        "Clicking a result fills the form on the left. Nothing is saved until you press Save.",
    ));
    hint.add_css_class("kalam-muted");
    hint.set_wrap(true);
    hint.set_xalign(0.0);
    panel.append(&hint);

    // Revealer gives the slide-out; the window widens to match.
    let revealer = gtk::Revealer::new();
    revealer.set_child(Some(&panel));
    revealer.set_transition_type(gtk::RevealerTransitionType::SlideLeft);
    revealer.set_transition_duration(180);
    revealer.set_reveal_child(false);
    revealer.set_hexpand(false);

    // ── actions: pinned outside the scroller so Save is always reachable ─
    // `max_content_height` is what keeps the form from growing taller than the
    // screen. A `gtk::Window` was bounded by its own default height; an in-app
    // panel is sized by its content, so without a cap a long description would
    // push the action bar out of view.
    let content_scroll = gtk::ScrolledWindow::builder()
        .hscrollbar_policy(gtk::PolicyType::Never)
        .max_content_height(460)
        .propagate_natural_height(true)
        .vexpand(true)
        .hexpand(true)
        .child(&root)
        .build();

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.set_vexpand(true);
    body.append(&content_scroll);
    body.append(&revealer);
    shell.append(&body);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    actions.add_css_class("kalam-dialog-actions");

    // Form-level feedback (save errors, cover picked) sits in the pinned bar
    // so it is visible regardless of scroll position.
    let status = gtk::Label::new(None);
    status.add_css_class("kalam-muted");
    status.set_halign(gtk::Align::Start);
    status.set_ellipsize(gtk::pango::EllipsizeMode::End);
    status.set_max_width_chars(48);

    // Walk the library without closing the dialog — the point of a bulk
    // clean-up pass. Both save first, so nothing is silently discarded.
    let neighbours = catalog
        .list_books(crate::db::SortKey::Title, "")
        .unwrap_or_default();
    let position = neighbours.iter().position(|b| b.id == book_id);

    let prev_btn = gtk::Button::new();
    prev_btn.set_child(Some(&crate::icons::labelled(
        "go-previous-symbolic",
        16,
        "Previous",
        6,
    )));
    prev_btn.add_css_class("kalam-secondary-btn");
    let next_btn = gtk::Button::new();
    next_btn.set_child(Some(&crate::icons::labelled(
        "go-next-symbolic",
        16,
        "Next",
        6,
    )));
    next_btn.add_css_class("kalam-secondary-btn");
    prev_btn.set_sensitive(matches!(position, Some(i) if i > 0));
    next_btn.set_sensitive(matches!(position, Some(i) if i + 1 < neighbours.len()));
    if position.is_some() {
        prev_btn.set_tooltip_text(Some("Save and edit the previous book"));
        next_btn.set_tooltip_text(Some("Save and edit the next book"));
    }
    actions.append(&prev_btn);
    actions.append(&next_btn);

    actions.append(&status);

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    actions.append(&spacer);

    let cancel = gtk::Button::with_label("Cancel");
    cancel.add_css_class("kalam-secondary-btn");
    let save = gtk::Button::with_label("Save");
    save.add_css_class("kalam-primary-btn");
    actions.append(&cancel);
    actions.append(&save);
    shell.append(&actions);

    // A1: an in-app dialog, so Sway cannot tile it away from the app. Unsaved
    // edits mean backdrop-click is disabled; Cancel and Esc remain.
    let Some(dialog) = in_app_dialog::present(
        anchor,
        "Edit metadata",
        in_app_dialog::DialogExit::UnsavedInput,
        &shell,
    ) else {
        crate::notify::error(
            "Could not open the metadata editor",
            "Please try again once the page has finished loading.",
        );
        return;
    };

    // Cover bytes fetched from Open Library, written only on Save.
    let pending_cover: Rc<RefCell<Option<Vec<u8>>>> = Rc::new(RefCell::new(None));

    // ── open/close the panel, reflowing the form as it goes ─────────────
    // The dialog is a fixed width that already allows for the panel, so
    // revealing it fills space that was always reserved: the window never
    // resizes, never drifts off-centre, and nothing can be clipped.
    //
    // Opening still moves the publication block under Series and the cover
    // below the description, so the form reads as one column while the panel
    // has the right-hand side.
    let panel_open = Rc::new(std::cell::Cell::new(false));
    let set_panel: Rc<dyn Fn(bool)> = {
        let revealer = revealer.clone();
        let search_entry = search_entry.clone();
        let top = top.clone();
        let fields = fields.clone();
        let side = side.clone();
        let pub_block = pub_block.clone();
        let cover_block = cover_block.clone();
        let desc_block = desc_block.clone();
        let panel_open = panel_open.clone();

        Rc::new(move |open: bool| {
            if panel_open.get() == open {
                return;
            }
            panel_open.set(open);

            if open {
                // Single column: publication under series, cover last.
                side.remove(&pub_block);
                side.remove(&cover_block);
                top.remove(&side);
                fields.remove(&desc_block);
                fields.append(&pub_block);
                fields.append(&desc_block);
                fields.append(&cover_block);
            } else {
                fields.remove(&pub_block);
                fields.remove(&cover_block);
                side.append(&pub_block);
                side.append(&cover_block);
                top.append(&side);
            }

            revealer.set_reveal_child(open);

            if open {
                search_entry.grab_focus();
            }
        })
    };

    // ── worker channel ──────────────────────────────────────────────────
    let (tx, rx) = async_channel::unbounded::<FetchMsg>();

    {
        let status = status.clone();
        let search_status = search_status.clone();
        let results = results.clone();
        let title_entry = title_entry.clone();
        let authors_entry = authors_entry.clone();
        let series_entry = series_entry.clone();
        let tags_entry = tags_entry.clone();
        let publisher_entry = publisher_entry.clone();
        let published_entry = published_entry.clone();
        let desc_view = desc_view.clone();
        let cover_host = cover_host.clone();
        let pending_cover = pending_cover.clone();
        let tx_inner = tx.clone();

        gtk::glib::spawn_future_local(async move {
            while let Ok(msg) = rx.recv().await {
                match msg {
                    FetchMsg::Results(list, errors) => {
                        rebuild_results(
                            &results,
                            &list,
                            &search_status,
                            &title_entry,
                            &authors_entry,
                            &series_entry,
                            &tags_entry,
                            &publisher_entry,
                            &published_entry,
                            &desc_view,
                            &tx_inner,
                        );
                        // Partial failures are worth naming: results that look
                        // thin may just be one provider being unavailable.
                        // Name the actual failure: "unavailable" gives the
                        // user nothing to act on.
                        let note = if errors.is_empty() {
                            String::new()
                        } else {
                            format!(
                                "  ({})",
                                errors
                                    .iter()
                                    .map(|(id, e)| format!("{}: {e}", id.label()))
                                    .collect::<Vec<_>>()
                                    .join("; ")
                            )
                        };
                        search_status.set_label(&if list.is_empty() {
                            format!("No matches. Try a different title or author.{note}")
                        } else {
                            format!(
                                "{} match{} — “Use this” fills the form for review.{note}",
                                list.len(),
                                if list.len() == 1 { "" } else { "es" }
                            )
                        });
                    }
                    FetchMsg::Failed(err) => {
                        search_status.set_label(&format!("Lookup failed. {err}"));
                    }
                    FetchMsg::CoverChoices(choices) => {
                        // Show thumbnails; the chosen one is fetched at full
                        // size and previewed before anything is written.
                        while let Some(c) = results.first_child() {
                            results.remove(&c);
                        }
                        let grid = gtk::FlowBox::builder()
                            .selection_mode(gtk::SelectionMode::None)
                            .min_children_per_line(2)
                            .max_children_per_line(2)
                            .homogeneous(true)
                            .column_spacing(10)
                            .row_spacing(10)
                            .halign(gtk::Align::Start)
                            .build();
                        for (cover_ref, bytes) in choices {
                            let Some(texture) = texture_from_bytes(&bytes) else {
                                continue;
                            };
                            let pic = gtk::Picture::for_paintable(&texture);
                            // Contain, not Cover: show the whole jacket rather
                            // than cropping to fill the cell.
                            pic.set_content_fit(gtk::ContentFit::Contain);
                            pic.set_can_shrink(true);
                            pic.set_size_request(THUMB_W, THUMB_H);

                            // Fixed-size wrapper stops the FlowBox stretching
                            // cells to different widths.
                            let cell = gtk::Box::new(gtk::Orientation::Vertical, 0);
                            cell.set_size_request(THUMB_W, THUMB_H);
                            cell.set_halign(gtk::Align::Center);
                            cell.set_valign(gtk::Align::Center);
                            cell.append(&pic);

                            let btn = gtk::Button::new();
                            btn.set_child(Some(&cell));
                            btn.add_css_class("kalam-cover-choice");
                            btn.set_halign(gtk::Align::Center);
                            btn.set_tooltip_text(Some("Use this cover"));

                            let tx2 = tx_inner.clone();
                            let ss = search_status.clone();
                            btn.connect_clicked(move |_| {
                                ss.set_label("Fetching full-size cover…");
                                let tx2 = tx2.clone();
                                let cover_ref = cover_ref.clone();
                                crate::tasks::spawn(
                                    move |_reporter| match metadata::fetch_cover(&cover_ref) {
                                        Ok(b) => FetchMsg::CoverReady(b),
                                        Err(e) => FetchMsg::CoverFailed(e.to_string()),
                                    },
                                    |_update| {},
                                    move |msg| {
                                        let _ = tx2.send_blocking(msg);
                                    },
                                );
                            });
                            grid.insert(&btn, -1);
                        }
                        results.append(&grid);
                        search_status.set_label("Pick a cover to preview it.");
                    }
                    FetchMsg::CoverReady(bytes) => {
                        // Preview immediately; the file is written on Save.
                        if let Some(texture) = texture_from_bytes(&bytes) {
                            while let Some(c) = cover_host.first_child() {
                                cover_host.remove(&c);
                            }
                            let pic = gtk::Picture::for_paintable(&texture);
                            pic.set_size_request(150, 240);
                            pic.set_content_fit(gtk::ContentFit::Fill);
                            cover_host.append(&pic);
                        }
                        *pending_cover.borrow_mut() = Some(bytes);
                        status.set_label("Cover downloaded — press Save to keep it.");
                    }
                    FetchMsg::CoverFailed(err) => {
                        status.set_label(&format!("Could not download that cover. {err}"));
                    }
                }
            }
        });
    }

    // ── search ──────────────────────────────────────────────────────────
    {
        let entry = search_entry.clone();
        let search_status = search_status.clone();
        let tx = tx.clone();
        let catalog = catalog.clone();
        // Rc so both the button and Enter can trigger the same logic; a plain
        // move closure capturing widgets is not Clone.
        let run: Rc<dyn Fn()> = Rc::new(move || {
            let query = entry.text().to_string();
            if query.trim().is_empty() {
                search_status.set_label("Type something to search for.");
                return;
            }
            // Rebuilt per search so toggling a source in Settings takes effect
            // immediately.
            let sources = metadata::enabled_sources(&catalog);
            if sources.is_empty() {
                search_status.set_label("No metadata sources are enabled — see Settings.");
                return;
            }
            search_status.set_label("Searching…");
            let tx = tx.clone();
            // Blocking HTTP on a worker thread keeps the dialog responsive.
            // Through the seam (A0 step 4) so it is cancelled at shutdown
            // rather than left holding an open socket.
            crate::tasks::spawn(
                move |_reporter| {
                    let (list, errors) = metadata::search_all(sources, &query, 10);
                    if list.is_empty() && !errors.is_empty() {
                        // Every source failed — surface why.
                        FetchMsg::Failed(
                            errors
                                .iter()
                                .map(|(id, e)| format!("{}: {e}", id.label()))
                                .collect::<Vec<_>>()
                                .join("  ·  "),
                        )
                    } else {
                        FetchMsg::Results(list, errors)
                    }
                },
                |_update| {},
                move |msg| {
                    let _ = tx.send_blocking(msg);
                },
            );
        });
        let run2 = run.clone();
        search_btn.connect_clicked(move |_| run());
        search_entry.connect_activate(move |_| run2());
    }

    // ── cover search: previews in the panel, pick before committing ─────
    {
        let title_entry_c = title_entry.clone();
        let authors_entry_c = authors_entry.clone();
        let search_status = search_status.clone();
        let search_entry_c = search_entry.clone();
        let set_panel = set_panel.clone();
        let tx = tx.clone();
        let catalog_c = catalog.clone();
        cover_search.connect_clicked(move |_| {
            let query = format!(
                "{} {}",
                title_entry_c.text().trim(),
                authors_entry_c.text().trim()
            )
            .trim()
            .to_string();
            if query.is_empty() {
                return;
            }
            search_entry_c.set_text(&query);
            set_panel(true);
            search_status.set_label("Looking for covers…");

            let tx = tx.clone();
            let sources = metadata::enabled_sources(&catalog_c);
            crate::tasks::spawn(
                move |reporter| {
                    let (list, errors) = metadata::search_all(sources, &query, 12);
                    // Fetch small thumbnails so the grid appears quickly; the
                    // full-size image is only pulled once one is picked.
                    //
                    // Six sequential HTTP gets is the longest wait in this
                    // dialog, so this is the one loop here worth making
                    // cancellable: closing the window mid-search stops it
                    // instead of downloading covers nobody will see.
                    let mut found = Vec::new();
                    for cover in list.iter().filter_map(|c| c.cover.clone()).take(6) {
                        if reporter.cancelled() {
                            break;
                        }
                        if let Ok(bytes) = metadata::fetch_thumbnail(&cover) {
                            found.push((cover, bytes));
                        }
                    }
                    if !found.is_empty() {
                        FetchMsg::CoverChoices(found)
                    } else if !errors.is_empty() {
                        FetchMsg::CoverFailed(
                            errors
                                .iter()
                                .map(|(id, e)| format!("{}: {e}", id.label()))
                                .collect::<Vec<_>>()
                                .join("  ·  "),
                        )
                    } else {
                        FetchMsg::CoverFailed("no covers found for that title".into())
                    }
                },
                |_update| {},
                move |msg| {
                    let _ = tx.send_blocking(msg);
                },
            );
        });
    }

    // ── replace cover from disk ─────────────────────────────────────────
    {
        let app_window = app_window.clone();
        let cover_host = cover_host.clone();
        let pending_cover = pending_cover.clone();
        let status = status.clone();
        pick_cover.connect_clicked(move |_| {
            let dialog = gtk::FileDialog::builder()
                .title("Choose a cover image")
                .modal(true)
                .build();
            let filter = gtk::FileFilter::new();
            filter.set_name(Some("Images"));
            for suffix in ["png", "jpg", "jpeg", "webp", "gif"] {
                filter.add_suffix(suffix);
            }
            let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
            filters.append(&filter);
            dialog.set_filters(Some(&filters));

            let cover_host = cover_host.clone();
            let pending_cover = pending_cover.clone();
            let status = status.clone();
            dialog.open(
                app_window.as_ref(),
                gtk::gio::Cancellable::NONE,
                move |res| {
                    let Ok(file) = res else { return };
                    let Some(path) = file.path() else { return };
                    match std::fs::read(&path) {
                        Ok(bytes) if !bytes.is_empty() => {
                            if let Some(texture) = texture_from_bytes(&bytes) {
                                while let Some(c) = cover_host.first_child() {
                                    cover_host.remove(&c);
                                }
                                let pic = gtk::Picture::for_paintable(&texture);
                                pic.set_size_request(150, 240);
                                pic.set_content_fit(gtk::ContentFit::Fill);
                                cover_host.append(&pic);
                            }
                            *pending_cover.borrow_mut() = Some(bytes);
                            status.set_label("Cover selected — press Save to keep it.");
                        }
                        Ok(_) => status.set_label("That image file is empty."),
                        Err(err) => status.set_label(&format!("Could not read that file: {err}")),
                    }
                },
            );
        });
    }

    {
        let dialog = dialog.clone();
        cancel.connect_clicked(move |_| dialog.close());
    }

    // ── save ────────────────────────────────────────────────────────────
    // Shared by Save and by Previous/Next, which save before moving on.
    // Returns false when the form is invalid, so navigation can abort.
    let commit: Rc<dyn Fn() -> bool> = {
        let catalog = catalog.clone();
        let status = status.clone();
        let pending_cover = pending_cover.clone();
        let on_saved = on_saved.clone();
        let title_entry = title_entry.clone();
        let authors_entry = authors_entry.clone();
        let series_entry = series_entry.clone();
        let series_index = series_index.clone();
        let publisher_entry = publisher_entry.clone();
        let published_entry = published_entry.clone();
        let tags_entry = tags_entry.clone();
        let desc_view = desc_view.clone();
        let rating_value = rating_value.clone();

        Rc::new(move || {
            let title = title_entry.text().trim().to_string();
            if title.is_empty() {
                status.set_label("A book needs a title.");
                return false;
            }

            let buffer = desc_view.buffer();
            let description = buffer
                .text(&buffer.start_iter(), &buffer.end_iter(), false)
                .to_string();
            let tags: Vec<String> = tags_entry
                .text()
                .split(',')
                .map(|t| t.trim().to_string())
                .filter(|t| !t.is_empty())
                .collect();
            let series = series_entry.text().trim().to_string();

            if let Err(err) = catalog.update_book_metadata(
                book_id,
                &title,
                authors_entry.text().trim(),
                if series.is_empty() {
                    None
                } else {
                    Some(series.as_str())
                },
                series_index.value() as f32,
                publisher_entry.text().trim(),
                published_entry.text().trim(),
                &description,
                &tags,
            ) {
                crate::notify::error("Could not save metadata", &err.to_string());
                status.set_label(&format!("Could not save: {err}"));
                return false;
            }

            crate::notify::report(
                catalog.set_book_rating(book_id, rating_value.get()),
                "Could not save the rating",
            );

            // Re-read so the cover swap sees the current row.
            if let Some(bytes) = pending_cover.borrow_mut().take() {
                if let Ok(Some(fresh)) = catalog.get_book(book_id) {
                    if let Err(err) = crate::epub::replace_cover_bytes(&catalog, &fresh, &bytes) {
                        crate::notify::error("Cover could not be saved", &err.to_string());
                        status.set_label(&format!("Metadata saved, but the cover failed: {err}"));
                        on_saved();
                        return false;
                    }
                }
            }

            // Push the result into the EPUB so other readers see it too.
            // Failing here is not fatal: the edit is already in the catalog.
            if crate::epub_metadata::write_enabled(&catalog) {
                match crate::epub_metadata::sync_book_to_file(&catalog, book_id) {
                    Ok(report) if report.wrote_metadata || report.wrote_cover => {
                        status.set_label("Saved, and written into the book file.");
                    }
                    Ok(_) => {}
                    Err(err) => {
                        // The dialog closes right after this, so the status
                        // line alone would vanish before it was read.
                        crate::notify::error(
                            "Metadata saved, but the book file was not updated",
                            &err.to_string(),
                        );
                        status.set_label(&format!(
                            "Saved in Kalam, but the file was not updated: {err}"
                        ));
                        on_saved();
                        return true;
                    }
                }
            }

            // Saving from this dialog was silent whenever everything worked,
            // which made a successful edit indistinguishable from a no-op.
            crate::notify::success("Metadata saved", &title);

            on_saved();
            true
        })
    };

    // The title icon opens the panel seeded with title + author; Open Library
    // matches far better on the pair than on either alone.
    {
        let set_panel = set_panel.clone();
        let search_entry = search_entry.clone();
        let title_entry = title_entry.clone();
        let authors_entry = authors_entry.clone();
        title_search.connect_clicked(move |_| {
            let query = format!(
                "{} {}",
                title_entry.text().trim(),
                authors_entry.text().trim()
            );
            search_entry.set_text(query.trim());
            set_panel(true);
        });
    }

    {
        let set_panel = set_panel.clone();
        panel_close.connect_clicked(move |_| set_panel(false));
    }

    {
        let dialog = dialog.clone();
        let commit = commit.clone();
        save.connect_clicked(move |_| {
            if commit() {
                dialog.close();
            }
        });
    }

    // Previous / Next: commit, close, reopen on the neighbour.
    for (btn, delta) in [(&prev_btn, -1_i64), (&next_btn, 1_i64)] {
        let dialog = dialog.clone();
        let anchor = anchor.clone();
        let catalog = catalog.clone();
        let commit = commit.clone();
        let on_saved = on_saved.clone();
        let neighbours: Vec<i64> = neighbours.iter().map(|b| b.id).collect();
        btn.connect_clicked(move |_| {
            if !commit() {
                return;
            }
            let Some(idx) = neighbours.iter().position(|id| *id == book_id) else {
                return;
            };
            let target = idx as i64 + delta;
            if target < 0 || target as usize >= neighbours.len() {
                return;
            }
            let next_id = neighbours[target as usize];
            // Close this panel before reopening on the neighbour, or two
            // dialogs would stack on the same overlay.
            dialog.close();
            open_editor_inner(&anchor, catalog.clone(), next_id, on_saved.clone());
        });
    }

    // Esc is handled by the dialog helper, for every in-app dialog alike.
}

#[allow(clippy::too_many_arguments)]
fn rebuild_results(
    host: &gtk::Box,
    candidates: &[Candidate],
    status: &gtk::Label,
    title_entry: &gtk::Entry,
    authors_entry: &gtk::Entry,
    series_entry: &gtk::Entry,
    tags_entry: &gtk::Entry,
    publisher_entry: &gtk::Entry,
    published_entry: &gtk::Entry,
    desc_view: &gtk::TextView,
    tx: &async_channel::Sender<FetchMsg>,
) {
    while let Some(child) = host.first_child() {
        host.remove(&child);
    }

    for candidate in candidates {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
        row.add_css_class("kalam-list-row");

        // Origin badge, so you can tell which provider is claiming what.
        if let Some(source) = candidate.source {
            let badge = gtk::Label::new(Some(source.badge()));
            badge.add_css_class("kalam-card-badge");
            badge.add_css_class(source.css_class());
            badge.set_valign(gtk::Align::Center);
            badge.set_tooltip_text(Some(source.label()));
            row.append(&badge);
        }

        let text = gtk::Box::new(gtk::Orientation::Vertical, 2);
        text.set_hexpand(true);

        let title = gtk::Label::new(Some(&candidate.title));
        title.add_css_class("kalam-card-title");
        title.set_halign(gtk::Align::Start);
        title.set_xalign(0.0);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&title);

        let meta = gtk::Label::new(Some(&candidate.summary()));
        meta.add_css_class("kalam-card-meta");
        meta.set_halign(gtk::Align::Start);
        meta.set_xalign(0.0);
        meta.set_ellipsize(gtk::pango::EllipsizeMode::End);
        text.append(&meta);
        row.append(&text);

        let use_btn = gtk::Button::with_label("Use this");
        use_btn.add_css_class("kalam-mini-btn");
        use_btn.set_valign(gtk::Align::Center);
        {
            let c = candidate.clone();
            let title_entry = title_entry.clone();
            let authors_entry = authors_entry.clone();
            let series_entry = series_entry.clone();
            let tags_entry = tags_entry.clone();
            let publisher_entry = publisher_entry.clone();
            let published_entry = published_entry.clone();
            let desc_view = desc_view.clone();
            let status = status.clone();
            let tx = tx.clone();
            use_btn.connect_clicked(move |_| {
                // Only overwrite fields the result actually has, so a sparse
                // match cannot blank out good local data.
                if !c.title.is_empty() {
                    title_entry.set_text(&c.title);
                }
                if !c.authors.is_empty() {
                    authors_entry.set_text(&c.authors);
                }
                if let Some(series) = &c.series {
                    series_entry.set_text(series);
                }
                if !c.tags.is_empty() {
                    tags_entry.set_text(&c.tags.join(", "));
                }
                if !c.publisher.is_empty() {
                    publisher_entry.set_text(&c.publisher);
                }
                if !c.published.is_empty() {
                    published_entry.set_text(&c.published);
                }
                let origin = c.source.map(|s| s.label()).unwrap_or("the source");
                status.set_label(&format!("Filled from {origin} — review, then Save."));

                // Google Books returns descriptions inline; Open Library needs
                // a second request keyed by the work id.
                if !c.description.trim().is_empty() {
                    desc_view.buffer().set_text(&c.description);
                } else if let (Some(key), Some(id)) = (c.detail_key.clone(), c.source) {
                    let desc_view = desc_view.clone();
                    let status2 = status.clone();
                    // Was a worker + its own channel + a local future, three
                    // pieces to say "fetch this and set a label". One call now.
                    crate::tasks::spawn(
                        move |_reporter| {
                            let source: Box<dyn metadata::MetadataSource> = match id {
                                metadata::SourceId::OpenLibrary => {
                                    Box::new(metadata::openlibrary::OpenLibrary)
                                }
                                metadata::SourceId::GoogleBooks => {
                                    Box::new(metadata::google_books::GoogleBooks {
                                        api_key: String::new(),
                                        country: metadata::google_books::detect_country(),
                                    })
                                }
                            };
                            source.fetch_description(&key).unwrap_or_default()
                        },
                        |_update| {},
                        move |text| {
                            if !text.trim().is_empty() {
                                desc_view.buffer().set_text(&text);
                                status2.set_label("Description filled — review, then Save.");
                            }
                        },
                    );
                }

                if let Some(cover_ref) = c.cover.clone() {
                    let tx = tx.clone();
                    crate::tasks::spawn(
                        move |_reporter| match metadata::fetch_cover(&cover_ref) {
                            Ok(bytes) => FetchMsg::CoverReady(bytes),
                            Err(err) => FetchMsg::CoverFailed(err.to_string()),
                        },
                        |_update| {},
                        move |msg| {
                            let _ = tx.send_blocking(msg);
                        },
                    );
                }
            });
        }
        row.append(&use_btn);
        host.append(&row);
    }
}

fn section_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("kalam-detail-section-title");
    label.set_halign(gtk::Align::Start);
    label
}

/// Small flat button carrying one symbolic icon.
fn icon_button(icon_name: &str, tooltip: &str) -> gtk::Button {
    let btn = gtk::Button::new();
    btn.set_child(Some(&crate::icons::symbolic_with_classes(
        icon_name,
        16,
        &["kalam-inline-icon"],
    )));
    btn.add_css_class("kalam-icon-btn");
    btn.set_tooltip_text(Some(tooltip));
    btn.set_valign(gtk::Align::Center);
    btn
}

/// Entry with a search icon on its right, sharing one row.
fn labelled_entry_with_search(
    parent: &gtk::Box,
    label: &str,
    value: &str,
    tooltip: &str,
) -> (gtk::Entry, gtk::Button) {
    parent.append(&section_label(label));
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let entry = gtk::Entry::new();
    entry.set_text(value);
    entry.set_hexpand(true);
    row.append(&entry);
    let btn = icon_button("system-search-symbolic", tooltip);
    row.append(&btn);
    parent.append(&row);
    (entry, btn)
}

fn labelled_entry(parent: &gtk::Box, label: &str, value: &str) -> gtk::Entry {
    parent.append(&section_label(label));
    let entry = gtk::Entry::new();
    entry.set_text(value);
    entry.set_hexpand(true);
    parent.append(&entry);
    entry
}

fn texture_from_bytes(bytes: &[u8]) -> Option<gtk::gdk::Texture> {
    let glib_bytes = gtk::glib::Bytes::from(bytes);
    gtk::gdk::Texture::from_bytes(&glib_bytes).ok()
}
