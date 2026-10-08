//! The edits review panel (Phase 6 step 5) — one book's pending edits,
//! before → after, with per-edit selection, per-edit discard, a collapsed
//! history of what earlier bakes changed, and the Apply button that bakes
//! the selection into the book's file.
//!
//! A plain widget panel in the app's float layer (the annotations/shelves
//! pattern), deliberately built as a shared component: the full editor
//! (step 8) hosts this same panel in its own chrome. All reads and the
//! small writes (delete one edit) happen inline — the accepted precedent
//! for panel-shaped work; the bake itself runs on a worker thread, and
//! its database follow-ups (re-hash, mark applied, reindex search) run
//! there with it, because reindexing a book is not a main-loop job.

use crate::db::{Catalog, PatchRecord};
use gtk::prelude::*;
use relm4::RelmWidgetExt;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

/// What the apply button needs to know: the rows as last laid out, and
/// whether a bake is in flight (a rebuild during a bake must not bring
/// the button back to life).
struct PanelState {
    pending: Vec<PatchRecord>,
    checks: HashMap<i64, gtk::CheckButton>,
    busy: bool,
}

/// Build the panel. `on_done` fires when the reader is finished with it
/// (the Done button) — the float layer closes on it.
pub fn build_edits_panel(
    catalog: Arc<Catalog>,
    book_id: i64,
    on_done: impl Fn() + 'static,
) -> gtk::Box {
    let on_done = Rc::new(on_done);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("kalam-edits-float");
    root.set_overflow(gtk::Overflow::Hidden);
    root.append(&crate::pages::book::panel_title("Pending edits"));

    let list_host = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list_host.set_margin_all(16);
    let scroll = gtk::ScrolledWindow::builder()
        .vexpand(true)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .child(&list_host)
        .build();
    root.append(&scroll);

    // Footer: Apply on the left, Done on the right.
    let footer = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    footer.set_margin_all(12);
    let apply = gtk::Button::with_label("Apply");
    apply.add_css_class("kalam-primary-btn");
    footer.append(&apply);
    let spacer = gtk::Box::new(gtk::Orientation::Vertical, 0);
    spacer.set_hexpand(true);
    footer.append(&spacer);
    let done = gtk::Button::with_label("Done");
    done.add_css_class("kalam-primary-btn");
    let done_fn = on_done.clone();
    done.connect_clicked(move |_| done_fn());
    footer.append(&done);
    root.append(&footer);

    let state = Rc::new(std::cell::RefCell::new(PanelState {
        pending: Vec::new(),
        checks: HashMap::new(),
        busy: false,
    }));

    // Self-referential refresh: every delete and every bake's landing
    // re-run it, so the list always tells the truth about the book.
    let holder: crate::pages::SelfRebuild = Rc::new(std::cell::RefCell::new(None));
    let closure: Rc<dyn Fn()> = Rc::new({
        let host = list_host.clone();
        let catalog = catalog.clone();
        let state = state.clone();
        let apply = apply.clone();
        let holder = holder.clone();
        move || {
            while let Some(child) = host.first_child() {
                host.remove(&child);
            }
            let all = match catalog.get_patches_for_book(book_id) {
                Ok(rows) => rows,
                Err(err) => {
                    crate::notify::error("Could not read this book's edits", &err.to_string());
                    Vec::new()
                }
            };
            let pending: Vec<PatchRecord> = all
                .iter()
                .filter(|p| p.status == "pending")
                .cloned()
                .collect();
            let applied: Vec<PatchRecord> = all
                .iter()
                .filter(|p| p.status == "applied")
                .cloned()
                .collect();

            let mut checks = HashMap::new();
            if all.is_empty() {
                let none = gtk::Label::new(Some(
                    "No edits for this book yet.\nSelect text while reading and press the pencil to fix a typo.",
                ));
                none.add_css_class("kalam-placeholder");
                none.set_wrap(true);
                host.append(&none);
            } else {
                if !pending.is_empty() {
                    let head = section_label(&format!(
                        "Pending — {}",
                        pending.len()
                    ));
                    host.append(&head);
                    for p in &pending {
                        let (row, check) = edit_row(p, &catalog, &holder);
                        host.append(&row);
                        checks.insert(p.id, check);
                    }
                } else {
                    let none = gtk::Label::new(Some("Nothing pending — every edit is in the book."));
                    none.add_css_class("kalam-placeholder");
                    none.set_wrap(true);
                    host.append(&none);
                }

                if !applied.is_empty() {
                    host.append(&history_section(&applied));
                }
            }
            // Every checkbox is live: unchecking one updates the Apply
            // button immediately, so the count it shows is never a lie.
            for check in checks.values() {
                let state = state.clone();
                let apply = apply.clone();
                check.connect_toggled(move |_| sync_apply_button(&state, &apply));
            }
            {
                let mut s = state.borrow_mut();
                s.pending = pending;
                s.checks = checks;
            }
            sync_apply_button(&state, &apply);
        }
    });
    *holder.borrow_mut() = Some(closure.clone());
    closure();

    // The bake: everything file-shaped on a worker, the verdict back on
    // the main loop, the panel telling the truth again either way.
    {
        let catalog = catalog.clone();
        let state = state.clone();
        // A differently-named clone: this closure must not capture the
        // very button it is connected to — the receiver `apply` is
        // borrowed by the connect call while a same-named capture would
        // have to move it (E0505). gtk::Button is refcounted; the clone
        // IS the same button.
        let apply_btn = apply.clone();
        let holder = holder.clone();
        apply.connect_clicked(move |_| {
            {
                let s = state.borrow();
                if s.busy {
                    return;
                }
            }
            let selected: Vec<PatchRecord> = {
                let s = state.borrow();
                s.pending
                    .iter()
                    .filter(|p| s.checks.get(&p.id).is_some_and(|c| c.is_active()))
                    .cloned()
                    .collect()
            };
            if selected.is_empty() {
                return;
            }
            // What the bake needs from the book row: the file to rewrite
            // and the hash the row must move from. The file's existence
            // is NOT checked here — ARCH.md: no disk on the UI thread —
            // and bake_epub's own missing-file error arrives through the
            // same verdict channel.
            let (path, old_hash) = match catalog.get_book(book_id) {
                Ok(Some(book)) => (book.file_path.clone(), book.file_hash.clone()),
                _ => {
                    crate::notify::error(
                        "Could not apply the edits",
                        "this book is no longer in the library",
                    );
                    return;
                }
            };

            {
                let mut s = state.borrow_mut();
                s.busy = true;
            }
            sync_apply_button(&state, &apply_btn);

            let (verdict_tx, verdict_rx) =
                async_channel::unbounded::<Result<crate::epub_sanitizer::BakeReport, String>>();
            let catalog = catalog.clone();
            std::thread::spawn(move || {
                let _t = crate::timing::measure("bake_edits");
                let out = bake_and_record(&catalog, book_id, &path, &old_hash, &selected);
                let _ = verdict_tx.send_blocking(out);
            });

            let state = state.clone();
            let apply_btn = apply_btn.clone();
            let holder = holder.clone();
            gtk::glib::spawn_future_local(async move {
                if let Ok(verdict) = verdict_rx.recv().await {
                    match verdict {
                        Ok(report) if report.failed.is_empty() => {
                            let n = report.applied.len();
                            let noun = if n == 1 { "edit" } else { "edits" };
                            crate::notify::success(
                                "Applied",
                                &format!("{n} {noun} written into the book."),
                            );
                        }
                        Ok(report) if report.applied.is_empty() => {
                            crate::notify::error(
                                "Nothing applied",
                                &report
                                    .failed
                                    .iter()
                                    .map(|(_, why)| why.as_str())
                                    .collect::<Vec<_>>()
                                    .join("; "),
                            );
                        }
                        Ok(report) => {
                            let n = report.applied.len();
                            let left = report.failed.len();
                            crate::notify::info(
                                "Partly applied",
                                &format!(
                                    "{n} applied; {left} stayed pending — {}.",
                                    report
                                        .failed
                                        .iter()
                                        .map(|(_, why)| why.as_str())
                                        .collect::<Vec<_>>()
                                        .join("; ")
                                ),
                            );
                        }
                        Err(err) => {
                            crate::notify::error("Could not apply the edits", &err);
                        }
                    }
                }
                {
                    let mut s = state.borrow_mut();
                    s.busy = false;
                }
                if let Some(refresh) = holder.borrow().clone() {
                    refresh();
                }
                sync_apply_button(&state, &apply_btn);
            });
        });
    }

    root
}

/// The bake and its database follow-ups, as one worker-thread unit: bake
/// the file, re-hash the book row (the remaster path's `rehash_book`,
/// which also moves any metadata overrides), mark what applied, and
/// reindex search so it matches what the reader will now show. Each
/// error string says exactly how far the pipeline got, because after the
/// first step they differ in what the reader needs to know.
fn bake_and_record(
    catalog: &Catalog,
    book_id: i64,
    path: &std::path::Path,
    old_hash: &str,
    selected: &[PatchRecord],
) -> Result<crate::epub_sanitizer::BakeReport, String> {
    let report = crate::epub_sanitizer::bake_epub(path, selected)
        .map_err(|e| format!("the book's file was not changed: {e:#}"))?;
    if report.applied.is_empty() {
        return Ok(report);
    }
    let new_hash = crate::db::hash_file(path)
        .map_err(|e| format!("the edits were written, but re-reading the book's hash failed: {e}"))?;
    catalog
        .rehash_book(book_id, old_hash, &new_hash)
        .map_err(|e| {
            format!("the edits were written, but the library record could not be updated: {e}")
        })?;
    for id in &report.applied {
        catalog.mark_patch_applied(*id).map_err(|e| {
            format!("the edits were written, but recording patch {id} as applied failed: {e}")
        })?;
    }
    // A spine op that landed changes the meaning of every chapter index
    // in the database. Carry the book's positions, highlights, saved
    // words and bookmarks across it (Phase 6 step 10); a remap failure
    // is reported like any other follow-up — the file is already right.
    let applied_spine = selected.iter().find(|p| {
        p.kind == "spine" && report.applied.contains(&p.id)
    });
    if let Some(op) = applied_spine {
        if let Some(order) = crate::epub_sanitizer::parse_spine_order(&op.replace_text) {
            catalog.remap_spine_indices(book_id, &order).map_err(|e| {
                format!(
                    "the edits were written, but re-anchoring the book's highlights \
                     and positions to the new chapter order failed: {e}"
                )
            })?;
        }
    }
    // An asset op that landed re-dressed the book: the library's jacket
    // is a cover file in the book's folder, extracted at import — stale
    // the moment the bake wrote new bytes into the book. Route the
    // staged image through the replace-cover service (fresh file, old
    // file and thumbnail cleaned, catalog pointed, new thumbnail
    // made), then the staging itself goes. Best-effort: the book's
    // cover entry is already right, and a jacket that lags one bake is
    // clutter, not corruption.
    for p in selected.iter().filter(|p| p.kind == "asset" && report.applied.contains(&p.id)) {
        if let Some(staged) = crate::epub_sanitizer::staged_asset_path(path, &p.replace_text) {
            let jacket = std::fs::read(&staged).ok().and_then(|bytes| {
                catalog
                    .get_book(book_id)
                    .ok()
                    .flatten()
                    .and_then(|book| crate::epub::replace_cover_bytes(catalog, &book, &bytes).ok())
            });
            if jacket.is_none() {
                log::warn!(
                    "bake: the cover file could not be updated — the book's own cover entry did"
                );
            }
            let _ = std::fs::remove_file(&staged);
        }
    }
    crate::content_index::index_book(catalog, book_id)
        .map_err(|e| format!("the edits were written, but the search index could not be updated: {e}"))?;
    Ok(report)
}

/// One pending edit: checkbox, before → after, meta, discard.
/// The row's two lines. A whole-file patch's replace text is the
/// entire entry — showing it would be a thousand-line row — so it
/// summarizes instead.
fn edit_row_lines(p: &crate::db::PatchRecord) -> (String, String) {
    if p.kind == "file" {
        (
            "Whole file".to_string(),
            format!("→ edited source ({} chars)", p.replace_text.chars().count()),
        )
    } else if p.kind == "spine" {
        // The stored order is the new reading order as old chapter
        // positions; what a reader wants to know is whether it moved
        // at all, and how far the book was reshuffled.
        let moved = crate::epub_sanitizer::parse_spine_order(&p.replace_text)
            .map(|order| {
                order
                    .iter()
                    .enumerate()
                    .filter(|(new, &old)| new != old)
                    .count()
            })
            .unwrap_or(0);
        (
            "Chapter order".to_string(),
            format!("→ {} of {} chapters reordered", moved, p.replace_text.split(',').count()),
        )
    } else if p.kind == "asset" {
        (
            "Cover".to_string(),
            format!(
                "→ replaced with {}",
                p.replace_text.rsplit('/').next().unwrap_or(&p.replace_text)
            ),
        )
    } else {
        (p.find_text.clone(), format!("→ {}", p.replace_text))
    }
}

/// The row's meta line. Text edits name their chapter; structural
/// ops are book-wide, so their line says what they are instead of a
/// chapter they do not have (their `chapter_index` is a placeholder
/// zero, never a position).
fn edit_row_meta(p: &PatchRecord, applied: bool) -> String {
    let structural = matches!(p.kind.as_str(), "spine" | "asset");
    let chapter = if structural {
        String::new()
    } else {
        format!("chapter {} · ", p.chapter_index + 1)
    };
    format!(
        "{} · {chapter}{}{}",
        source_name(&p.source),
        if applied { "applied " } else { "" },
        if applied {
            p.applied_at.as_deref().and_then(|s| s.get(..10)).unwrap_or("")
        } else {
            p.created_at.get(..10).unwrap_or("")
        }
    )
}

fn edit_row(
    p: &PatchRecord,
    catalog: &Arc<Catalog>,
    holder: &crate::pages::SelfRebuild,
) -> (gtk::Box, gtk::CheckButton) {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 9);
    row.set_margin_top(8);
    row.set_margin_bottom(8);

    let check = gtk::CheckButton::new();
    check.set_active(true);
    check.set_valign(gtk::Align::Start);
    row.append(&check);

    let info = gtk::Box::new(gtk::Orientation::Vertical, 2);
    info.set_hexpand(true);

    let (before_line, after_line) = edit_row_lines(p);
    let before = gtk::Label::new(Some(&before_line));
    before.add_css_class("k-edit-before");
    before.set_halign(gtk::Align::Start);
    before.set_xalign(0.0);
    before.set_wrap(true);
    info.append(&before);

    let after = gtk::Label::new(Some(&after_line));
    after.add_css_class("k-edit-after");
    after.set_halign(gtk::Align::Start);
    after.set_xalign(0.0);
    after.set_wrap(true);
    info.append(&after);

    let meta = gtk::Label::new(Some(&edit_row_meta(p, false)));
    meta.add_css_class("k-edit-meta");
    meta.set_halign(gtk::Align::Start);
    meta.set_xalign(0.0);
    info.append(&meta);
    row.append(&info);

    let del = gtk::Button::new();
    del.add_css_class("kalam-icon-btn");
    del.set_focus_on_click(false);
    del.set_tooltip_text(Some("Discard this edit"));
    del.set_child(Some(&crate::icons::symbolic("user-trash-symbolic", 15)));
    let id = p.id;
    let catalog = catalog.clone();
    let holder = holder.clone();
    let discarded = p.clone();
    del.connect_clicked(move |_| {
        if let Err(err) = catalog.delete_patch(id) {
            crate::notify::error("Could not discard the edit", &err.to_string());
        } else {
            crate::notify::compact("Edit discarded", "");
            // A discarded asset op leaves a staged file behind — delete
            // it off-thread (ARCH.md: no disk on the UI thread). Only
            // that op's own staging, resolved from the book's row like
            // the bake does.
            if discarded.kind == "asset" {
                let name = discarded.replace_text.clone();
                let book_id = discarded.book_id;
                // A click handler is `Fn` — it runs again — so the
                // thread gets its own clone; the captured handle is
                // never moved out of the closure that owns it.
                let catalog = catalog.clone();
                std::thread::spawn(move || {
                    if let Ok(Some(book)) = catalog.get_book(book_id) {
                        if let Some(staged) =
                            crate::epub_sanitizer::staged_asset_path(&book.file_path, &name)
                        {
                            let _ = std::fs::remove_file(staged);
                        }
                    }
                });
            }
        }
        if let Some(refresh) = holder.borrow().clone() {
            refresh();
        }
    });
    row.append(&del);

    (row, check)
}

/// The applied history, collapsed by default — what earlier bakes
/// changed, kept for the record, out of the way of the work.
fn history_section(applied: &[PatchRecord]) -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let revealer = gtk::Revealer::new();
    revealer.set_transition_type(gtk::RevealerTransitionType::SlideDown);
    revealer.set_reveal_child(false);

    let toggle = gtk::Button::with_label(&format!("Applied history — {}", applied.len()));
    toggle.add_css_class("kalam-secondary-btn");
    let revealer_ref = revealer.clone();
    toggle.connect_clicked(move |_| {
        revealer_ref.set_reveal_child(!revealer_ref.reveals_child());
    });
    section.append(&toggle);

    let rows = gtk::Box::new(gtk::Orientation::Vertical, 0);
    rows.set_margin_top(8);
    for p in applied {
        let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
        row.set_margin_top(6);
        row.set_margin_bottom(6);
        let (before_line, after_line) = edit_row_lines(p);
        let before = gtk::Label::new(Some(&before_line));
        before.add_css_class("k-edit-before");
        before.set_halign(gtk::Align::Start);
        before.set_xalign(0.0);
        before.set_wrap(true);
        row.append(&before);
        let after = gtk::Label::new(Some(&after_line));
        after.add_css_class("k-edit-after");
        after.set_halign(gtk::Align::Start);
        after.set_xalign(0.0);
        after.set_wrap(true);
        row.append(&after);
        let meta = gtk::Label::new(Some(&edit_row_meta(p, true)));
        meta.add_css_class("k-edit-meta");
        meta.set_halign(gtk::Align::Start);
        meta.set_xalign(0.0);
        row.append(&meta);
        rows.append(&row);
    }
    revealer.set_child(Some(&rows));
    section.append(&revealer);
    section
}

fn section_label(text: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("k-edits-section");
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    label
}

/// The patch source as the panel says it — "Fix typo" reads better than
/// "typo", and anything unknown is itself information.
fn source_name(source: &str) -> &str {
    match source {
        "typo" => "Fix typo",
        "proofread" => "Proofreading",
        "editor" => "Editor",
        "toc" => "TOC editor",
        "asset" => "Cover",
        other => other,
    }
}

/// Apply button label/sensitivity from the shared state (see the panel).
fn sync_apply_button(state: &Rc<std::cell::RefCell<PanelState>>, apply: &gtk::Button) {
    let s = state.borrow();
    if s.busy {
        apply.set_label("Applying…");
        apply.set_sensitive(false);
        return;
    }
    let checked = s
        .checks
        .values()
        .filter(|c| c.is_active())
        .count()
        .min(s.pending.len());
    if checked == 0 {
        apply.set_label("Apply");
        apply.set_sensitive(false);
    } else {
        let noun = if checked == 1 { "edit" } else { "edits" };
        apply.set_label(&format!("Apply {checked} {noun}"));
        apply.set_sensitive(true);
    }
}
