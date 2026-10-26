//! PDF Pages Edit mode (decisions 68–71).
//!
//! The staged edit itself is plain data: a baseline order, the
//! order the reader has made of it, a selection, and the undo
//! snapshots between. Nothing here touches the file or the
//! catalog until Apply, which is the whole point of staging —
//! leaving without applying must be able to forget every bit of
//! it. When Apply does come, the PDF is rewritten beside the
//! book's managed copy, verified, hashed, and handed to the
//! catalog's one transaction with the file swap inside it.

use super::*;
use crate::db::{PdfPageBoundCounts, PdfPageEditRewrite, PdfPageRotationRewrite};
use crate::pdf::{PdfPageEditEntry, PdfPageEditSource};
use crate::widgets::dialog::{dialog_shell, present_dialog, DialogSize};
use crate::widgets::in_app_dialog::{DialogExit, InAppDialog};
use crate::widgets::pill_button::{boxed_ghost_button, pill_button, PillButtonKind};
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

pub fn normalize_edit_rotation(degrees: i32) -> i32 {
    degrees.rem_euclid(360)
}

/// Where one staged page comes from. An existing page keeps the
/// stable identity the catalog gave it (decision 72); an inserted
/// page carries the identity it will be given on Apply, minted
/// here so selection, thumbnails and undo can all name it from
/// the moment it appears.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfPageEditItemSource {
    Existing {
        page_id: String,
        page: usize,
    },
    Inserted {
        page_id: String,
        path: PathBuf,
        page: usize,
        page_count: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfPageEditItem {
    pub source: PdfPageEditItemSource,
    /// The staged clockwise quarter turn, on top of the source
    /// page's own `/Rotate`.
    pub rotation_clockwise: i32,
}

impl PdfPageEditItem {
    pub fn key(&self) -> &str {
        match &self.source {
            PdfPageEditItemSource::Existing { page_id, .. }
            | PdfPageEditItemSource::Inserted { page_id, .. } => page_id,
        }
    }

    pub fn existing_page(&self) -> Option<usize> {
        match &self.source {
            PdfPageEditItemSource::Existing { page, .. } => Some(*page),
            PdfPageEditItemSource::Inserted { .. } => None,
        }
    }

    pub fn is_inserted(&self) -> bool {
        matches!(self.source, PdfPageEditItemSource::Inserted { .. })
    }

    fn edit_entry(&self) -> PdfPageEditEntry {
        PdfPageEditEntry {
            source: match &self.source {
                PdfPageEditItemSource::Existing { page, .. } => {
                    PdfPageEditSource::Existing { page: *page }
                }
                PdfPageEditItemSource::Inserted { path, page, .. } => {
                    PdfPageEditSource::Inserted {
                        path: path.clone(),
                        page: *page,
                    }
                }
            },
            rotation_clockwise: self.rotation_clockwise,
        }
    }

    fn thumbnail_source(&self, book_path: &Path) -> PdfThumbnailSource {
        match &self.source {
            PdfPageEditItemSource::Existing { page, .. } => PdfThumbnailSource {
                path: book_path.to_path_buf(),
                page: *page,
                extra_rotation: self.rotation_clockwise,
            },
            PdfPageEditItemSource::Inserted { path, page, .. } => PdfThumbnailSource {
                path: path.clone(),
                page: *page,
                extra_rotation: self.rotation_clockwise,
            },
        }
    }

    fn description(&self, position: usize, total: usize) -> String {
        match &self.source {
            PdfPageEditItemSource::Existing { page, .. } => {
                format!("Book page {page}, at position {position} of {total}")
            }
            PdfPageEditItemSource::Inserted {
                path,
                page,
                page_count,
                ..
            } => {
                let name = path
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "another PDF".to_string());
                format!(
                    "Page {page} of {page_count} from {name}, at position {position} of {total}"
                )
            }
        }
    }
}

/// The file and page a thumbnail key is rendered from, plus the
/// staged turn to lay over the reader's view rotation. Resolved
/// when the grid is pointed at a (possibly staged) order, so the
/// thumbnail worker never has to know an edit exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PdfThumbnailSource {
    pub path: PathBuf,
    pub page: usize,
    pub extra_rotation: i32,
}

/// What the reader was trying to do when an unfinished edit got
/// in the way. The unsaved-changes answer (decision 70) either
/// clears it, discards into it, or applies into it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PdfPageEditLeaveIntent {
    ExitMode,
    CloseReader,
    MinimizeToBubble,
    SwitchLeftTab(PdfLeftTab),
}

/// What a finished Apply hands back to the reader: the book's new
/// shape, and where the page it stood on has gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PdfPageEditAppliedInfo {
    pub total_pages: usize,
    pub current_page: usize,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PdfPageEditSummary {
    pub deleted: usize,
    pub inserted: usize,
    pub rotated: usize,
    pub moved: usize,
}

#[derive(Clone)]
struct PdfPageEditSnapshot {
    items: Vec<PdfPageEditItem>,
    selected: HashSet<String>,
    anchor: Option<String>,
}

#[derive(Clone, Default)]
pub struct PdfPageEditState {
    pub active: bool,
    pub busy: bool,
    baseline: Vec<PdfPageEditItem>,
    items: Vec<PdfPageEditItem>,
    selected: HashSet<String>,
    anchor: Option<String>,
    undo: Vec<PdfPageEditSnapshot>,
    insert_counter: u64,
}

impl PdfPageEditState {
    pub fn start(
        &mut self,
        identities: &[PdfPageIdentity],
        total_pages: usize,
        current_page: usize,
    ) -> bool {
        if self.active || self.busy || total_pages == 0 || identities.len() != total_pages {
            return false;
        }
        let mut ordered: Vec<&PdfPageIdentity> = identities.iter().collect();
        ordered.sort_by_key(|page| page.position);
        if ordered
            .iter()
            .enumerate()
            .any(|(index, page)| page.position != index + 1)
        {
            return false;
        }
        let items: Vec<PdfPageEditItem> = ordered
            .iter()
            .map(|page| PdfPageEditItem {
                source: PdfPageEditItemSource::Existing {
                    page_id: page.page_id.clone(),
                    page: page.position,
                },
                rotation_clockwise: 0,
            })
            .collect();
        self.baseline = items.clone();
        self.items = items;
        self.selected.clear();
        self.undo.clear();
        self.anchor = None;
        // Standing on a page is the likeliest reason to be here:
        // it starts selected, ticked, where the reader can see it.
        if let Some(item) = self
            .items
            .iter()
            .find(|item| item.existing_page() == Some(current_page.clamp(1, total_pages)))
        {
            self.selected.insert(item.key().to_string());
            self.anchor = Some(item.key().to_string());
        }
        self.active = true;
        true
    }

    pub fn reset(&mut self) {
        self.active = false;
        self.busy = false;
        self.baseline.clear();
        self.items.clear();
        self.selected.clear();
        self.anchor = None;
        self.undo.clear();
    }

    pub fn items(&self) -> &[PdfPageEditItem] {
        &self.items
    }

    pub fn baseline(&self) -> &[PdfPageEditItem] {
        &self.baseline
    }

    pub fn selected(&self) -> &HashSet<String> {
        &self.selected
    }

    pub fn selected_keys_in_order(&self) -> Vec<String> {
        self.items
            .iter()
            .filter(|item| self.selected.contains(item.key()))
            .map(|item| item.key().to_string())
            .collect()
    }

    pub fn has_changes(&self) -> bool {
        self.active && self.items != self.baseline
    }

    fn push_undo(&mut self) {
        const KEPT: usize = 100;
        self.undo.push(PdfPageEditSnapshot {
            items: self.items.clone(),
            selected: self.selected.clone(),
            anchor: self.anchor.clone(),
        });
        if self.undo.len() > KEPT {
            self.undo.remove(0);
        }
    }

    pub fn undo(&mut self) -> bool {
        if !self.active || self.busy {
            return false;
        }
        let Some(snapshot) = self.undo.pop() else {
            return false;
        };
        self.items = snapshot.items;
        self.selected = snapshot.selected;
        self.anchor = snapshot.anchor;
        true
    }

    fn position_of(&self, key: &str) -> Option<usize> {
        self.items.iter().position(|item| item.key() == key)
    }

    /// A thumbnail click, with the modifiers it arrived with
    /// (decision 68): plain replaces, Ctrl toggles, Shift extends
    /// the selection over the run from the anchor.
    pub fn click(&mut self, key: &str, control: bool, shift: bool) -> bool {
        if !self.active || self.busy {
            return false;
        }
        let Some(position) = self.position_of(key) else {
            return false;
        };
        let before = self.selected.clone();
        if shift {
            let anchor = self
                .anchor
                .as_deref()
                .and_then(|anchor| self.position_of(anchor))
                .unwrap_or(position);
            let (from, to) = (anchor.min(position), anchor.max(position));
            for item in &self.items[from..=to] {
                self.selected.insert(item.key().to_string());
            }
            if self.anchor.is_none() {
                self.anchor = Some(key.to_string());
            }
        } else if control {
            if !self.selected.remove(key) {
                self.selected.insert(key.to_string());
            }
            self.anchor = Some(key.to_string());
        } else {
            self.selected.clear();
            self.selected.insert(key.to_string());
            self.anchor = Some(key.to_string());
        }
        self.selected != before
    }

    pub fn select_all(&mut self) -> bool {
        if !self.active || self.busy {
            return false;
        }
        let before = self.selected.len();
        self.selected = self
            .items
            .iter()
            .map(|item| item.key().to_string())
            .collect();
        self.selected.len() != before
    }

    /// Begin dragging `key`: an unselected page drags alone, a
    /// selected one carries the whole selection with it.
    pub fn drag_keys(&mut self, key: &str) -> Vec<String> {
        if !self.active || self.busy || self.position_of(key).is_none() {
            return Vec::new();
        }
        if !self.selected.contains(key) {
            self.selected.clear();
            self.selected.insert(key.to_string());
            self.anchor = Some(key.to_string());
        }
        self.selected_keys_in_order()
    }

    /// Drop the selection at `insertion_index`, a slot among the
    /// *current* items (0 is before the first, `len` after the
    /// last). Selected pages keep their relative order.
    pub fn move_selected_to(&mut self, insertion_index: usize) -> bool {
        if !self.active || self.busy || self.selected.is_empty() {
            return false;
        }
        let target = moved_items(&self.items, &self.selected, insertion_index);
        if target == self.items {
            return false;
        }
        self.push_undo();
        self.items = target;
        true
    }

    pub fn rotate_selected(&mut self, clockwise_degrees: i32) -> bool {
        if !self.active || self.busy || self.selected.is_empty() {
            return false;
        }
        let delta = normalize_edit_rotation(clockwise_degrees);
        if delta == 0 {
            return false;
        }
        self.push_undo();
        for item in &mut self.items {
            if self.selected.contains(item.key()) {
                item.rotation_clockwise =
                    normalize_edit_rotation(item.rotation_clockwise + delta);
            }
        }
        true
    }

    /// Stage the deletion of the selection. The caller has
    /// already asked where asking was owed; this only refuses the
    /// one deletion no book survives — all of it.
    pub fn delete_selected(&mut self) -> bool {
        if !self.active
            || self.busy
            || self.selected.is_empty()
            || self.selected.len() >= self.items.len()
        {
            return false;
        }
        self.push_undo();
        let selected = self.selected.clone();
        self.items.retain(|item| !selected.contains(item.key()));
        self.selected.clear();
        self.anchor = None;
        true
    }

    /// The slot new pages land in: after the last selected page,
    /// or after the page being read when nothing is selected.
    pub fn insertion_index(&self, current_page: usize) -> usize {
        let after = |index: usize| (index + 1).min(self.items.len());
        if let Some(index) = self
            .items
            .iter()
            .rposition(|item| self.selected.contains(item.key()))
        {
            return after(index);
        }
        if let Some(index) = self
            .items
            .iter()
            .position(|item| item.existing_page() == Some(current_page))
        {
            return after(index);
        }
        self.items.len()
    }

    /// Stage every page of another PDF at the insertion slot, in
    /// its own order, and leave them selected — they are what the
    /// reader just did, and the likeliest next thing to act on.
    pub fn insert_pdf(&mut self, path: PathBuf, page_count: usize, current_page: usize) -> bool {
        if !self.active || self.busy || page_count == 0 {
            return false;
        }
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let mut inserted = Vec::with_capacity(page_count);
        for page in 1..=page_count {
            self.insert_counter += 1;
            inserted.push(PdfPageEditItem {
                source: PdfPageEditItemSource::Inserted {
                    page_id: format!(
                        "inserted-{}-{stamp}-{}-{page}",
                        std::process::id(),
                        self.insert_counter
                    ),
                    path: path.clone(),
                    page,
                    page_count,
                },
                rotation_clockwise: 0,
            });
        }
        let slot = self.insertion_index(current_page);
        self.push_undo();
        self.selected = inserted
            .iter()
            .map(|item| item.key().to_string())
            .collect();
        self.anchor = inserted.first().map(|item| item.key().to_string());
        for (offset, item) in inserted.into_iter().enumerate() {
            self.items.insert(slot + offset, item);
        }
        true
    }

    /// The selected existing pages as the catalog counts work on
    /// them: identity for the drawings, physical page for the
    /// highlights and bookmarks.
    pub fn selected_existing_pages(&self) -> Vec<(String, usize)> {
        self.items
            .iter()
            .filter(|item| self.selected.contains(item.key()))
            .filter_map(|item| {
                item.existing_page()
                    .map(|page| (item.key().to_string(), page))
            })
            .collect()
    }

    /// The selection's places in the staged order — the numbers
    /// under the thumbnails the reader is looking at.
    pub fn selected_positions(&self) -> Vec<usize> {
        self.items
            .iter()
            .enumerate()
            .filter(|(_, item)| self.selected.contains(item.key()))
            .map(|(index, _)| index + 1)
            .collect()
    }

    pub fn summary(&self) -> PdfPageEditSummary {
        let baseline_keys: HashSet<&str> =
            self.baseline.iter().map(|item| item.key()).collect();
        let item_keys: HashSet<&str> = self.items.iter().map(|item| item.key()).collect();
        let baseline_index: HashMap<&str, usize> = self
            .baseline
            .iter()
            .enumerate()
            .map(|(index, item)| (item.key(), index))
            .collect();
        // Pages that merely shuffled along to make room were not
        // themselves moved: the moved count is what is left after
        // the longest run that kept its old relative order.
        let survivor_order: Vec<usize> = self
            .items
            .iter()
            .filter_map(|item| baseline_index.get(item.key()).copied())
            .collect();
        PdfPageEditSummary {
            deleted: baseline_keys.difference(&item_keys).count(),
            inserted: self.items.iter().filter(|item| item.is_inserted()).count(),
            rotated: self
                .items
                .iter()
                .filter(|item| normalize_edit_rotation(item.rotation_clockwise) != 0)
                .count(),
            moved: survivor_order.len() - longest_increasing_run(&survivor_order),
        }
    }

    /// Where an old physical page ends up: its own new place when
    /// it survives, the nearest surviving page's when it does not
    /// (decision 69), and the first page when nothing of the old
    /// book remains.
    pub fn mapped_current_page(&self, old_page: usize) -> usize {
        let mut best: Option<(usize, bool, usize)> = None;
        for (index, item) in self.items.iter().enumerate() {
            let Some(page) = item.existing_page() else {
                continue;
            };
            if page == old_page {
                return index + 1;
            }
            let candidate = (page.abs_diff(old_page), page < old_page, index + 1);
            if best.is_none_or(|current| candidate < current) {
                best = Some(candidate);
            }
        }
        best.map(|(_, _, page)| page).unwrap_or(1)
    }

    pub fn edit_entries(&self) -> Vec<PdfPageEditEntry> {
        self.items.iter().map(|item| item.edit_entry()).collect()
    }

    pub fn selected_edit_entries(&self) -> Vec<PdfPageEditEntry> {
        self.items
            .iter()
            .filter(|item| self.selected.contains(item.key()))
            .map(|item| item.edit_entry())
            .collect()
    }

    pub fn ordered_page_ids(&self) -> Vec<String> {
        self.items
            .iter()
            .map(|item| item.key().to_string())
            .collect()
    }
}

/// The sequence after a drop, computed without touching the
/// state so a no-op drop never spends an undo snapshot.
pub fn moved_items(
    items: &[PdfPageEditItem],
    selected: &HashSet<String>,
    insertion_index: usize,
) -> Vec<PdfPageEditItem> {
    let slot = insertion_index.min(items.len());
    let removed_before_slot = items[..slot]
        .iter()
        .filter(|item| selected.contains(item.key()))
        .count();
    let moving: Vec<PdfPageEditItem> = items
        .iter()
        .filter(|item| selected.contains(item.key()))
        .cloned()
        .collect();
    let mut rest: Vec<PdfPageEditItem> = items
        .iter()
        .filter(|item| !selected.contains(item.key()))
        .cloned()
        .collect();
    let at = (slot - removed_before_slot).min(rest.len());
    for (offset, item) in moving.into_iter().enumerate() {
        rest.insert(at + offset, item);
    }
    rest
}

fn longest_increasing_run(values: &[usize]) -> usize {
    let mut tails: Vec<usize> = Vec::new();
    for value in values {
        match tails.binary_search(value) {
            Ok(_) => {}
            Err(position) => {
                if position == tails.len() {
                    tails.push(*value);
                } else {
                    tails[position] = *value;
                }
            }
        }
    }
    tails.len()
}

fn page_word(count: usize) -> &'static str {
    if count == 1 {
        "page"
    } else {
        "pages"
    }
}

/// "1", "1 and 2", "1, 2 and 3" — the join the dialogs' copy uses.
fn join_words(parts: &[String]) -> String {
    match parts {
        [] => String::new(),
        [only] => only.clone(),
        [first, second] => format!("{first} and {second}"),
        _ => {
            let (last, head) = parts.split_last().expect("non-empty");
            format!("{} and {last}", head.join(", "))
        }
    }
}

/// The unsaved-changes sentence (the board's "You deleted 2
/// pages, rotated 1 and moved 3."), followed by the reassurance
/// that nothing is written yet.
pub fn page_edit_summary_sentence(summary: PdfPageEditSummary) -> String {
    let mut phrases: Vec<String> = Vec::new();
    let phrase = |verb: &str, count: usize, phrases: &mut Vec<String>| {
        if count == 0 {
            return;
        }
        if phrases.is_empty() {
            phrases.push(format!("{verb} {count} {}", page_word(count)));
        } else {
            phrases.push(format!("{verb} {count}"));
        }
    };
    phrase("deleted", summary.deleted, &mut phrases);
    phrase("inserted", summary.inserted, &mut phrases);
    phrase("rotated", summary.rotated, &mut phrases);
    phrase("moved", summary.moved, &mut phrases);
    if phrases.is_empty() {
        return "Nothing is written to your copy of the book until you apply them."
            .to_string();
    }
    format!(
        "You {}. Nothing is written to your copy of the book until you apply them.",
        join_words(&phrases)
    )
}

/// The delete prompt's words: its title, and a body naming the
/// pages and what sits on them (decision 69's counts).
pub fn page_edit_delete_copy(
    selected_positions: &[usize],
    counts: PdfPageBoundCounts,
) -> (String, String) {
    let page_count = selected_positions.len();
    let title = format!("Delete {page_count} {}?", page_word(page_count));
    let names = if page_count <= 3 {
        join_words(
            &selected_positions
                .iter()
                .map(|page| page.to_string())
                .collect::<Vec<_>>(),
        )
    } else {
        format!(
            "{}, {} and {} more",
            selected_positions[0],
            selected_positions[1],
            page_count - 2
        )
    };
    let subject = if page_count == 1 {
        format!("Page {names} has")
    } else {
        format!("Pages {names} have")
    };
    let mut parts: Vec<String> = Vec::new();
    let mut part = |count: usize, singular: &str, plural: &str| {
        if count > 0 {
            parts.push(format!(
                "{count} {}",
                if count == 1 { singular } else { plural }
            ));
        }
    };
    part(counts.highlights, "highlight", "highlights");
    part(counts.quotes, "quote", "quotes");
    part(counts.notes, "note", "notes");
    part(counts.drawings, "drawing", "drawings");
    part(counts.bookmarks, "bookmark", "bookmarks");
    let total = counts.highlights
        + counts.quotes
        + counts.notes
        + counts.drawings
        + counts.bookmarks;
    let pronoun = if total == 1 { "It" } else { "They" };
    let body = format!(
        "{subject} {}. {pronoun} will be removed too. \
         You can undo this until you apply your changes.",
        join_words(&parts)
    );
    (title, body)
}

fn close_dialog_from_slot(slot: &Rc<RefCell<Option<InAppDialog>>>) {
    if let Some(dialog) = slot.borrow().clone() {
        dialog.close();
    }
}

fn page_edit_dialog_shell(message: &str) -> crate::widgets::dialog::DialogShell {
    let shell = dialog_shell(DialogSize::Form, None);
    let label = gtk::Label::new(Some(message));
    label.add_css_class("kalam-ui-dialog-message");
    label.set_halign(gtk::Align::Start);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_width_chars(1);
    label.set_max_width_chars(((DialogSize::Form.width() - 48) / 9).max(1));
    shell.body().append(&label);
    // The boards spread these footers: the way back on the left,
    // the way forward on the right.
    shell.footer().set_halign(gtk::Align::Fill);
    shell
}

fn footer_spacer() -> gtk::Box {
    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    spacer
}

/// The delete confirmation: a ghost Cancel on the left, the
/// filled Delete on the right (decision 75).
pub fn show_page_edit_delete_dialog(
    anchor: &impl IsA<gtk::Widget>,
    title: &str,
    message: &str,
    sender: &ComponentSender<PdfReaderModel>,
) -> Option<InAppDialog> {
    let shell = page_edit_dialog_shell(message);
    let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));

    let cancel_slot = slot.clone();
    shell.footer().append(&boxed_ghost_button("Cancel", move || {
        close_dialog_from_slot(&cancel_slot);
    }));
    shell.footer().append(&footer_spacer());
    let confirm_slot = slot.clone();
    let tx = sender.input_sender().clone();
    shell
        .footer()
        .append(&pill_button("Delete", PillButtonKind::Primary, move || {
            close_dialog_from_slot(&confirm_slot);
            let _ = tx.send(PdfReaderMsg::ConfirmPageEditDelete);
        }));

    let dialog = present_dialog(anchor, title, DialogExit::OwnButtons, &shell)?;
    *slot.borrow_mut() = Some(dialog.clone());
    Some(dialog)
}

/// The leaving question: Discard on the left, Keep editing and
/// the filled Apply together on the right (decision 70).
pub fn show_page_edit_unsaved_dialog(
    anchor: &impl IsA<gtk::Widget>,
    message: &str,
    sender: &ComponentSender<PdfReaderModel>,
) -> Option<InAppDialog> {
    let shell = page_edit_dialog_shell(message);
    let slot: Rc<RefCell<Option<InAppDialog>>> = Rc::new(RefCell::new(None));

    let discard_slot = slot.clone();
    let tx = sender.input_sender().clone();
    shell
        .footer()
        .append(&boxed_ghost_button("Discard", move || {
            close_dialog_from_slot(&discard_slot);
            let _ = tx.send(PdfReaderMsg::PageEditLeaveDiscard);
        }));
    shell.footer().append(&footer_spacer());
    let keep_slot = slot.clone();
    let tx = sender.input_sender().clone();
    shell
        .footer()
        .append(&boxed_ghost_button("Keep editing", move || {
            close_dialog_from_slot(&keep_slot);
            let _ = tx.send(PdfReaderMsg::PageEditLeaveKeep);
        }));
    let apply_slot = slot.clone();
    let tx = sender.input_sender().clone();
    shell
        .footer()
        .append(&pill_button("Apply", PillButtonKind::Primary, move || {
            close_dialog_from_slot(&apply_slot);
            let _ = tx.send(PdfReaderMsg::PageEditLeaveApply);
        }));

    let dialog = present_dialog(
        anchor,
        "Apply your page changes?",
        DialogExit::OwnButtons,
        &shell,
    )?;
    *slot.borrow_mut() = Some(dialog.clone());
    Some(dialog)
}

fn temp_sibling_path(target: &Path, purpose: &str) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0);
    let name = format!(
        ".{}-kalam-{purpose}-{}-{stamp}",
        target
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| "pages.pdf".to_string()),
        std::process::id()
    );
    target
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(|parent| parent.join(&name))
        .unwrap_or_else(|| PathBuf::from(name))
}

fn write_page_edit_temp(
    book_path: &Path,
    shared_password: &PdfSessionPassword,
    state: &PdfPageEditState,
) -> std::result::Result<(PathBuf, crate::pdf::PdfPageEditWriteInfo), String> {
    let temp = temp_sibling_path(book_path, "editing");
    let outcome = (|| -> std::result::Result<crate::pdf::PdfPageEditWriteInfo, String> {
        let mut doc = pdf_open_document(book_path, shared_password)
            .map_err(|error| error.log_message().to_string())?;
        let mut file = std::fs::File::create(&temp)
            .map_err(|error| format!("Could not start the edited copy: {error}"))?;
        let info = doc
            .write_edited_pages_to(&state.edit_entries(), &mut file)
            .map_err(|error| error.to_string())?;
        file.sync_all()
            .map_err(|error| format!("Could not finish the edited copy: {error}"))?;
        drop(file);
        drop(doc);
        // The replacement has to open before anything is allowed
        // to stand in the book's place.
        let check = pdf_open_document(&temp, shared_password)
            .map_err(|error| error.log_message().to_string())?;
        if check.page_count() != info.page_count {
            return Err("The edited copy does not have the pages it should.".to_string());
        }
        Ok(info)
    })();
    match outcome {
        Ok(info) => Ok((temp, info)),
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            Err(error)
        }
    }
}

/// Apply on its own thread: rewrite, verify, hash, then the
/// catalog's transaction — with its file swap inside — and the
/// sidecar refresh it finishes with.
fn apply_page_edit(
    catalog: Arc<Catalog>,
    book_id: i64,
    book_path: PathBuf,
    shared_password: PdfSessionPassword,
    state: PdfPageEditState,
    old_current_page: usize,
) -> std::result::Result<PdfPageEditAppliedInfo, String> {
    let (temp, info) = write_page_edit_temp(&book_path, &shared_password, &state)?;
    let outcome = (|| -> std::result::Result<PdfPageEditAppliedInfo, String> {
        let new_hash = crate::db::hash_file(&temp)
            .map_err(|error| format!("Could not read the edited copy back: {error}"))?;
        let rotations = state
            .items()
            .iter()
            .filter(|item| normalize_edit_rotation(item.rotation_clockwise) != 0)
            .filter_map(|item| {
                let page = item.existing_page()?;
                let (width, height) = info.existing_page_dimensions.get(&page)?;
                Some(PdfPageRotationRewrite {
                    page_id: item.key().to_string(),
                    page_width: *width,
                    page_height: *height,
                    clockwise_degrees: item.rotation_clockwise,
                })
            })
            .collect();
        catalog
            .apply_pdf_page_edit(
                book_id,
                &PdfPageEditRewrite {
                    ordered_page_ids: state.ordered_page_ids(),
                    rotations,
                },
                &temp,
                &book_path,
                &new_hash,
            )
            .map_err(|error| error.to_string())?;
        Ok(PdfPageEditAppliedInfo {
            total_pages: info.page_count,
            current_page: state.mapped_current_page(old_current_page),
        })
    })();
    if outcome.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    outcome
}

/// Extract on its own thread: the selected pages into a verified
/// temp file in the destination's folder, then one rename onto
/// the name the reader chose.
fn extract_page_edit(
    book_path: PathBuf,
    shared_password: PdfSessionPassword,
    entries: Vec<PdfPageEditEntry>,
    destination: PathBuf,
) -> std::result::Result<PathBuf, String> {
    if destination == book_path {
        return Err(
            "Extracted pages need their own file, not the book's copy.".to_string(),
        );
    }
    let temp = temp_sibling_path(&destination, "extracting");
    let outcome = (|| -> std::result::Result<(), String> {
        let doc = pdf_open_document(&book_path, &shared_password)
            .map_err(|error| error.log_message().to_string())?;
        let mut file = std::fs::File::create(&temp)
            .map_err(|error| format!("Could not start the extracted copy: {error}"))?;
        let written = doc
            .write_extracted_pages_to(&entries, &mut file)
            .map_err(|error| error.to_string())?;
        file.sync_all()
            .map_err(|error| format!("Could not finish the extracted copy: {error}"))?;
        drop(file);
        drop(doc);
        let check = pdf_open_document(&temp, &shared_password)
            .map_err(|error| error.log_message().to_string())?;
        if check.page_count() != written {
            return Err(
                "The extracted copy does not have the pages it should.".to_string()
            );
        }
        std::fs::rename(&temp, &destination)
            .map_err(|error| format!("Could not save the extracted copy: {error}"))?;
        Ok(())
    })();
    match outcome {
        Ok(()) => Ok(destination),
        Err(error) => {
            let _ = std::fs::remove_file(&temp);
            Err(error)
        }
    }
}

fn inspect_insert_pdf(path: &Path) -> std::result::Result<usize, String> {
    let doc = PdfDocument::open(path).map_err(|error| {
        if error.to_string().contains("password") {
            "That PDF is password protected, so its pages cannot be inserted."
                .to_string()
        } else {
            format!("That file could not be opened as a PDF: {error}")
        }
    })?;
    // A MuPDF-readable file that is not a PDF has no pages to
    // graft; asking for a rotation is the cheapest way to learn
    // that before the pages are staged.
    doc.page_rotation(1)
        .map_err(|_| "That file is not a PDF Kalam can take pages from.".to_string())?;
    Ok(doc.page_count())
}

fn pdf_file_filter() -> gtk::gio::ListStore {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("PDF documents (*.pdf)"));
    filter.add_suffix("pdf");
    filter.add_mime_type("application/pdf");
    let filters = gtk::gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    filters
}

fn dialog_window(anchor: &gtk::Widget) -> Option<gtk::Window> {
    anchor
        .root()
        .and_then(|root| root.downcast::<gtk::Window>().ok())
        .or_else(|| {
            relm4::main_application()
                .active_window()
                .and_then(|window| window.downcast::<gtk::Window>().ok())
        })
}

fn extract_initial_name(title: &str, selected: &[usize]) -> String {
    let mut stem: String = title
        .chars()
        .map(|ch| {
            if ch == '/' || ch == '\\' || ch.is_control() {
                '-'
            } else {
                ch
            }
        })
        .collect();
    stem = stem.trim().trim_matches('.').trim().to_string();
    if stem.is_empty() {
        stem = "Extracted pages".to_string();
    }
    match selected {
        [page] => format!("{stem} - page {page}.pdf"),
        pages => format!("{stem} - {} pages.pdf", pages.len()),
    }
}

impl PdfReaderModel {
    pub fn page_edit_current_key(&self) -> Option<String> {
        self.page_id_for_position(self.current_page)
    }

    pub fn start_page_edit(&mut self) -> bool {
        if !self.doc_ready || self.open_state != PdfOpenState::Ready {
            return false;
        }
        self.ensure_page_identities();
        let started =
            self.page_edit
                .start(&self.page_identities, self.total_pages, self.current_page);
        if started {
            self.sync_pages_grid_document();
        } else if !self.page_edit.active {
            crate::notify::error(
                "These pages cannot be edited yet",
                "Kalam could not line the book's pages up with their saved work.",
            );
        }
        started
    }

    pub fn exit_page_edit(&mut self) {
        self.page_edit.reset();
        self.page_edit_leave_intent = None;
        self.page_edit_dialog = None;
        self.sync_pages_grid_document();
    }

    /// The Pages header in both its shapes: the chapter heading
    /// with its pencil, and the edit bar that replaces it.
    pub fn sync_page_edit_header(&self) {
        let Some(pages) = &self.pages_widgets else {
            return;
        };
        let editing = self.page_edit.active;
        pages.normal_heading.set_visible(!editing);
        pages.edit_heading.set_visible(editing);
        pages.edit_button.set_sensitive(
            self.doc_ready && self.open_state == PdfOpenState::Ready && !self.page_edit.busy,
        );
        if !editing {
            return;
        }
        let selected = self.page_edit.selected().len();
        pages
            .edit_selected_label
            .set_label(&format!("{selected} selected"));
        let busy = self.page_edit.busy;
        pages
            .edit_apply_button
            .set_sensitive(self.page_edit.has_changes() && !busy);
        let has_selection = selected > 0 && !busy;
        pages.edit_rotate_left_button.set_sensitive(has_selection);
        pages.edit_rotate_right_button.set_sensitive(has_selection);
        pages.edit_extract_button.set_sensitive(has_selection);
        pages.edit_delete_button.set_sensitive(
            has_selection && selected < self.page_edit.items().len(),
        );
        pages.edit_insert_button.set_sensitive(!busy);
    }

    /// The grid's entries for the order in hand — the file's, or
    /// the staged edit's — with the thumbnail source for each.
    /// The source map is rebuilt alongside: it is the only place
    /// a key is ever turned back into a file and page.
    pub fn page_edit_grid_entries(
        &mut self,
    ) -> (
        Vec<pages_grid::PdfPagesGridEntry>,
        HashSet<String>,
        HashSet<String>,
        HashSet<String>,
    ) {
        self.page_thumbnail_sources.clear();
        if !self.doc_ready {
            return (Vec::new(), HashSet::new(), HashSet::new(), HashSet::new());
        }
        let Some(book_path) = self.file_path.clone() else {
            return (Vec::new(), HashSet::new(), HashSet::new(), HashSet::new());
        };

        if self.page_edit.active {
            let items = self.page_edit.items().to_vec();
            let total = items.len();
            let mut entries = Vec::with_capacity(total);
            let mut rotated = HashSet::new();
            for (index, item) in items.iter().enumerate() {
                let position = index + 1;
                if normalize_edit_rotation(item.rotation_clockwise) != 0 {
                    rotated.insert(item.key().to_string());
                }
                self.page_thumbnail_sources.insert(
                    item.key().to_string(),
                    item.thumbnail_source(&book_path),
                );
                entries.push(pages_grid::PdfPagesGridEntry {
                    key: item.key().to_string(),
                    label: position.to_string(),
                    description: item.description(position, total),
                    jump_page: item.existing_page().unwrap_or(position),
                });
            }
            return (
                entries,
                HashSet::new(),
                self.page_edit.selected().clone(),
                rotated,
            );
        }

        let identities: HashMap<usize, String> = self
            .page_identities
            .iter()
            .map(|page| (page.position, page.page_id.clone()))
            .collect();
        let current_keys: HashSet<String> = self
            .pages_current_pages()
            .iter()
            .filter_map(|page| identities.get(page).cloned())
            .collect();
        let mut entries = Vec::with_capacity(self.total_pages);
        for page in 1..=self.total_pages {
            let key = identities
                .get(&page)
                .cloned()
                .unwrap_or_else(|| format!("physical-{page}"));
            self.page_thumbnail_sources.insert(
                key.clone(),
                PdfThumbnailSource {
                    path: book_path.clone(),
                    page,
                    extra_rotation: 0,
                },
            );
            entries.push(pages_grid::PdfPagesGridEntry {
                key,
                label: page.to_string(),
                description: format!("Page {page} of {}", self.total_pages),
                jump_page: page,
            });
        }
        (entries, current_keys, HashSet::new(), HashSet::new())
    }

    /// Ask the thumbnail worker for one key the grid has just
    /// bound. Existing, failed and already-pending keys stop
    /// here, as they always have.
    pub fn request_page_thumbnail_key(&mut self, key: &str) {
        if !self.doc_ready {
            return;
        }
        let Some(grid) = self.pages_grid.clone() else {
            return;
        };
        if grid.has_thumbnail_state(key) || !self.thumbnail_pending.insert(key.to_string()) {
            return;
        }
        let Some(source) = self.page_thumbnail_sources.get(key).cloned() else {
            self.thumbnail_pending.remove(key);
            grid.update_thumbnail(key, None);
            return;
        };
        let Some(tx) = self.thumbnail_tx.clone() else {
            self.thumbnail_pending.remove(key);
            grid.update_thumbnail(key, None);
            return;
        };
        let _ = tx.send_blocking(PdfThumbnailRequest {
            generation: self.thumbnail_generation,
            revision: self.document_revision,
            key: key.to_string(),
            page: source.page,
            target_width: pages_grid::THUMBNAIL_TARGET_WIDTH,
            rotation: normalize_edit_rotation(
                self.rotation_degrees + source.extra_rotation,
            ),
            path: source.path,
        });
    }

    pub fn request_bound_thumbnails(&mut self, keys: &HashSet<String>) {
        let Some(grid) = self.pages_grid.clone() else {
            return;
        };
        for key in grid.bound_keys() {
            if keys.contains(&key) {
                self.request_page_thumbnail_key(&key);
            }
        }
    }

    /// Delete, asking only where the pages carry work of the
    /// reader's (decision 69).
    pub fn request_page_edit_delete(&mut self, sender: &ComponentSender<Self>) {
        if !self.page_edit.active
            || self.page_edit.busy
            || self.page_edit.selected().is_empty()
        {
            return;
        }
        if self.page_edit.selected().len() >= self.page_edit.items().len() {
            crate::notify::error(
                "A book needs at least one page",
                "Leave one page undeleted, or insert another first.",
            );
            return;
        }
        let counts = match self.catalog.pdf_page_bound_counts(
            self.book_id,
            &self.page_edit.selected_existing_pages(),
        ) {
            Ok(counts) => counts,
            Err(error) => {
                crate::notify::error("Could not check those pages", &error.to_string());
                return;
            }
        };
        if counts.is_empty() {
            if self.page_edit.delete_selected() {
                self.sync_pages_grid_document();
            }
            return;
        }
        let (title, body) =
            page_edit_delete_copy(&self.page_edit.selected_positions(), counts);
        let Some(anchor) = self
            .pages_widgets
            .as_ref()
            .map(|pages| pages.scroll.clone().upcast::<gtk::Widget>())
        else {
            return;
        };
        self.page_edit_dialog = show_page_edit_delete_dialog(&anchor, &title, &body, sender);
    }

    pub fn show_page_edit_unsaved_dialog(&mut self, sender: &ComponentSender<Self>) {
        let Some(anchor) = self
            .pages_widgets
            .as_ref()
            .map(|pages| pages.scroll.clone().upcast::<gtk::Widget>())
        else {
            return;
        };
        let message = page_edit_summary_sentence(self.page_edit.summary());
        self.page_edit_dialog = show_page_edit_unsaved_dialog(&anchor, &message, sender);
    }

    /// Stand in the way of leaving, when leaving would lose a
    /// staged edit. `true` means the caller must not proceed:
    /// either the question is on screen, or a write is in flight
    /// and the intent is being kept for its answer.
    pub fn page_edit_blocks_leave(
        &mut self,
        intent: PdfPageEditLeaveIntent,
        sender: &ComponentSender<Self>,
    ) -> bool {
        if !self.page_edit.active {
            return false;
        }
        if self.page_edit.busy {
            if intent != PdfPageEditLeaveIntent::ExitMode {
                self.page_edit_leave_intent = Some(intent);
            }
            return true;
        }
        if !self.page_edit.has_changes() {
            self.page_edit.reset();
            self.page_edit_leave_intent = None;
            self.sync_pages_grid_document();
            return false;
        }
        self.page_edit_leave_intent = Some(intent);
        self.show_page_edit_unsaved_dialog(sender);
        true
    }

    pub fn perform_page_edit_leave(
        &mut self,
        intent: PdfPageEditLeaveIntent,
        sender: &ComponentSender<Self>,
    ) {
        match intent {
            PdfPageEditLeaveIntent::ExitMode => self.sync_pages_grid_document(),
            PdfPageEditLeaveIntent::CloseReader => {
                let _ = sender.input_sender().send(PdfReaderMsg::Close);
            }
            PdfPageEditLeaveIntent::MinimizeToBubble => {
                let _ = sender.input_sender().send(PdfReaderMsg::MinimizeToBubble);
            }
            PdfPageEditLeaveIntent::SwitchLeftTab(tab) => {
                let _ = sender.input_sender().send(PdfReaderMsg::SetLeftTab(tab));
                // A keyboard summon (T/H) expects the panel, not
                // just its tab changed behind a closed revealer.
                let _ = sender.input_sender().send(PdfReaderMsg::OpenLeftPanel);
            }
        }
    }

    /// After a write finishes, take up a leave that was waiting
    /// on it — asking now if the edit is still unapplied.
    pub fn resume_page_edit_leave(&mut self, sender: &ComponentSender<Self>) {
        let Some(intent) = self.page_edit_leave_intent else {
            return;
        };
        if self.page_edit.busy {
            return;
        }
        self.page_edit_leave_intent = None;
        if self.page_edit.active && self.page_edit.has_changes() {
            self.page_edit_leave_intent = Some(intent);
            self.show_page_edit_unsaved_dialog(sender);
        } else {
            if self.page_edit.active {
                self.page_edit.reset();
                self.sync_pages_grid_document();
            }
            self.perform_page_edit_leave(intent, sender);
        }
    }

    pub fn choose_insert_pdf(&self, sender: &ComponentSender<Self>) {
        let Some(anchor) = self
            .pages_widgets
            .as_ref()
            .map(|pages| pages.scroll.clone().upcast::<gtk::Widget>())
        else {
            return;
        };
        let dialog = gtk::FileDialog::builder()
            .title("Insert pages from a PDF")
            .modal(true)
            .build();
        dialog.set_filters(Some(&pdf_file_filter()));
        let window = dialog_window(&anchor);
        let tx = sender.input_sender().clone();
        dialog.open(window.as_ref(), gtk::gio::Cancellable::NONE, move |res| {
            if let Ok(file) = res {
                if let Some(path) = file.path() {
                    let _ = tx.send(PdfReaderMsg::InsertPagesChosen(path));
                }
            }
        });
    }

    pub fn choose_extract_pdf(&self, sender: &ComponentSender<Self>) {
        let Some(anchor) = self
            .pages_widgets
            .as_ref()
            .map(|pages| pages.scroll.clone().upcast::<gtk::Widget>())
        else {
            return;
        };
        let dialog = gtk::FileDialog::builder()
            .title("Save extracted pages")
            .modal(true)
            .initial_name(extract_initial_name(
                &self.title,
                &self.page_edit.selected_positions(),
            ))
            .build();
        dialog.set_filters(Some(&pdf_file_filter()));
        let window = dialog_window(&anchor);
        let tx = sender.input_sender().clone();
        dialog.save(window.as_ref(), gtk::gio::Cancellable::NONE, move |res| {
            if let Ok(file) = res {
                if let Some(path) = file.path() {
                    let _ = tx.send(PdfReaderMsg::ExtractPagesChosen(path));
                }
            }
        });
    }

    pub fn begin_insert_inspection(
        &mut self,
        path: PathBuf,
        sender: &ComponentSender<Self>,
    ) {
        if !self.page_edit.active || self.page_edit.busy {
            return;
        }
        self.page_edit.busy = true;
        self.sync_page_edit_header();
        let tx = sender.input_sender().clone();
        let spawned = std::thread::Builder::new()
            .name("kalam-pdf-insert-inspect".to_string())
            .spawn(move || {
                let result = inspect_insert_pdf(&path);
                let _ = tx.send(PdfReaderMsg::InsertPagesReady { path, result });
            });
        if spawned.is_err() {
            self.page_edit.busy = false;
            self.sync_page_edit_header();
            crate::notify::error(
                "Pages could not be inserted",
                "Kalam could not start checking that PDF.",
            );
            self.resume_page_edit_leave(sender);
        }
    }

    pub fn begin_extract_pages(
        &mut self,
        destination: PathBuf,
        sender: &ComponentSender<Self>,
    ) {
        if !self.page_edit.active || self.page_edit.busy {
            return;
        }
        let entries = self.page_edit.selected_edit_entries();
        let Some(book_path) = self.file_path.clone() else {
            return;
        };
        if entries.is_empty() {
            return;
        }
        self.page_edit.busy = true;
        self.sync_page_edit_header();
        let password = self.session_password.clone();
        let tx = sender.input_sender().clone();
        let spawned = std::thread::Builder::new()
            .name("kalam-pdf-extract".to_string())
            .spawn(move || {
                let result =
                    extract_page_edit(book_path, password, entries, destination);
                let _ = tx.send(PdfReaderMsg::ExtractPagesDone(result));
            });
        if spawned.is_err() {
            self.page_edit.busy = false;
            self.sync_page_edit_header();
            crate::notify::error(
                "Pages could not be extracted",
                "Kalam could not start writing the extracted copy.",
            );
            self.resume_page_edit_leave(sender);
        }
    }

    pub fn begin_apply_page_edit(&mut self, sender: &ComponentSender<Self>) {
        if !self.page_edit.active || self.page_edit.busy || !self.page_edit.has_changes() {
            return;
        }
        let Some(book_path) = self.file_path.clone() else {
            crate::notify::error(
                "The book's copy could not be found",
                "Your staged changes are still here; nothing was written.",
            );
            return;
        };
        self.page_edit.busy = true;
        self.sync_page_edit_header();
        let catalog = self.catalog.clone();
        let book_id = self.book_id;
        let password = self.session_password.clone();
        let state = self.page_edit.clone();
        let old_current_page = self.current_page;
        let tx = sender.input_sender().clone();
        let spawned = std::thread::Builder::new()
            .name("kalam-pdf-page-apply".to_string())
            .spawn(move || {
                let result = apply_page_edit(
                    catalog,
                    book_id,
                    book_path,
                    password,
                    state,
                    old_current_page,
                );
                let _ = tx.send(PdfReaderMsg::PageEditApplied(result));
            });
        if spawned.is_err() {
            self.page_edit.busy = false;
            self.sync_page_edit_header();
            crate::notify::error(
                "The edit could not be started",
                "Your staged changes are still here; nothing was written.",
            );
        }
    }

    /// The book changed under the reader: retire every raster,
    /// text layer and search answer the old file produced, move
    /// to the mapped page, and let the document service describe
    /// the new book as it would a fresh open.
    pub fn finish_page_edit_apply(
        &mut self,
        info: PdfPageEditAppliedInfo,
        widgets: &PdfReaderModelWidgets,
        sender: &ComponentSender<Self>,
    ) {
        self.page_edit.reset();
        self.page_edit_dialog = None;
        self.document_revision = self.document_revision.wrapping_add(1);

        self.commit_drawing_text_editor();
        self.commit_sticky_card_text();
        self.dismiss_drawing_popover();
        self.selected_drawing_id = None;
        self.set_draw_tool(PdfDrawTool::Select);
        self.clear_selection();
        if self.search_active || self.search_results_open {
            self.search_active = false;
            self.clear_search_state(widgets, sender);
        } else {
            self.search_results.clear();
            self.search_pending = false;
            self.refresh_search_highlights();
        }

        self.invalidate_renders();
        self.textures.clear();
        self.page_text_cache.clear();
        self.text_in_progress.clear();
        self.ocr_in_progress.clear();
        self.ocr_started_fp.clear();
        self.thumbnail_generation = self.thumbnail_generation.wrapping_add(1);
        if let Some(generation) = &self.thumbnail_active_generation {
            generation.store(self.thumbnail_generation, Ordering::Relaxed);
        }
        self.thumbnail_pending.clear();
        if let Some(grid) = &self.pages_grid {
            grid.clear_thumbnails();
        }

        self.total_pages = info.total_pages.max(1);
        self.current_page = info.current_page.clamp(1, self.total_pages);
        self.toc_entries.clear();
        self.page_labels.clear();
        self.collapsed_toc_entries.clear();
        self.doc_ready = false;
        self.is_loading = true;
        self.reload_bookmarks();
        self.reload_annotations();
        self.reload_saved_words();
        self.reload_drawings();
        self.refresh_drawing_paints();
        self.sync_pages_grid_document();

        if let (Some(path), Some(tx)) = (self.file_path.clone(), self.doc_tx.as_ref()) {
            let _ = tx.send_blocking(PdfDocRequest::Info {
                path,
                page: self.current_page,
                revision: self.document_revision,
            });
        }
        crate::notify::success(
            "Pages updated",
            &format!(
                "This copy of the book now has {} {}.",
                info.total_pages,
                page_word(info.total_pages)
            ),
        );
        self.resume_page_edit_leave(sender);
    }

    /// The screenshot loop's staged edit: the reading spread and
    /// its neighbours selected, one later page already turned.
    pub fn stage_page_edit_for_screenshot(&mut self) {
        if !self.start_page_edit() {
            return;
        }
        let first = self.current_page.max(1);
        let wanted: Vec<usize> = (first..=self.total_pages).take(3).collect();
        let keys: Vec<String> = wanted
            .iter()
            .filter_map(|page| self.page_id_for_position(*page))
            .collect();
        if let Some(first_key) = keys.first() {
            self.page_edit.click(first_key, false, false);
        }
        if let Some(last_key) = keys.last() {
            self.page_edit.click(last_key, false, true);
        }
        // The board's turned page, two rows down.
        if let Some(key) = self.page_id_for_position(first + 4) {
            self.page_edit.click(&key, false, false);
            self.page_edit.rotate_selected(90);
            if let Some(first_key) = keys.first() {
                self.page_edit.click(first_key, false, false);
            }
            if let Some(last_key) = keys.last() {
                self.page_edit.click(last_key, false, true);
            }
        }
        self.sync_pages_grid_document();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edit_state(pages: usize, current: usize) -> PdfPageEditState {
        let identities: Vec<PdfPageIdentity> = (1..=pages)
            .map(|page| PdfPageIdentity {
                page_id: format!("page-{page}"),
                position: page,
            })
            .collect();
        let mut state = PdfPageEditState::default();
        assert!(state.start(&identities, pages, current));
        state
    }

    fn pages_of(state: &PdfPageEditState) -> Vec<usize> {
        state
            .items()
            .iter()
            .filter_map(|item| item.existing_page())
            .collect()
    }

    #[test]
    fn selection_replaces_toggles_and_extends() {
        let mut state = edit_state(6, 1);
        state.click("page-2", false, false);
        state.click("page-4", false, true);
        assert_eq!(state.selected_keys_in_order(), vec!["page-2", "page-3", "page-4"]);
        state.click("page-3", true, false);
        assert_eq!(state.selected_keys_in_order(), vec!["page-2", "page-4"]);
        state.select_all();
        assert_eq!(state.selected().len(), 6);
    }

    #[test]
    fn moves_carry_the_selection_in_order_and_undo_restores_it() {
        let mut state = edit_state(6, 1);
        state.click("page-2", false, false);
        state.click("page-3", true, false);
        // Dropping before old position 6 (slot 5) puts 2 and 3
        // after 5, still in their own order.
        assert!(state.move_selected_to(5));
        assert_eq!(pages_of(&state), vec![1, 4, 5, 2, 3, 6]);
        assert_eq!(state.summary().moved, 2);
        assert!(state.undo());
        assert_eq!(pages_of(&state), vec![1, 2, 3, 4, 5, 6]);
        assert!(!state.has_changes());
    }

    #[test]
    fn rotate_and_delete_stage_until_undone() {
        let mut state = edit_state(4, 1);
        state.click("page-2", false, false);
        assert!(state.rotate_selected(-90));
        assert_eq!(state.items()[1].rotation_clockwise, 270);
        assert!(state.rotate_selected(90));
        assert!(!state.has_changes(), "turning back is no longer a change");
        assert!(state.delete_selected());
        assert_eq!(pages_of(&state), vec![1, 3, 4]);
        assert_eq!(state.summary().deleted, 1);
        assert!(state.undo());
        assert_eq!(pages_of(&state), vec![1, 2, 3, 4]);
    }

    #[test]
    fn the_last_page_cannot_be_deleted_and_current_page_maps_to_the_nearest() {
        let mut state = edit_state(2, 1);
        state.select_all();
        assert!(!state.delete_selected());
        state.click("page-1", false, false);
        assert!(state.delete_selected());
        assert_eq!(state.mapped_current_page(1), 1);
    }

    #[test]
    fn summary_and_delete_copy_match_the_boards() {
        let sentence = page_edit_summary_sentence(PdfPageEditSummary {
            deleted: 2,
            inserted: 0,
            rotated: 1,
            moved: 3,
        });
        assert_eq!(
            sentence,
            "You deleted 2 pages, rotated 1 and moved 3. \
             Nothing is written to your copy of the book until you apply them."
        );
        let (title, body) = page_edit_delete_copy(
            &[12, 13],
            PdfPageBoundCounts {
                highlights: 3,
                bookmarks: 1,
                ..Default::default()
            },
        );
        assert_eq!(title, "Delete 2 pages?");
        assert_eq!(
            body,
            "Pages 12 and 13 have 3 highlights and 1 bookmark. \
             They will be removed too. You can undo this until you apply your changes."
        );
    }

    #[test]
    fn inserted_pages_land_after_the_selection_with_fresh_identities() {
        let mut state = edit_state(3, 1);
        state.click("page-2", false, false);
        assert!(state.insert_pdf(PathBuf::from("/tmp/other.pdf"), 2, 1));
        assert_eq!(state.items().len(), 5);
        assert!(state.items()[2].is_inserted());
        assert!(state.items()[3].is_inserted());
        assert_ne!(state.items()[2].key(), state.items()[3].key());
        assert_eq!(state.summary().inserted, 2);
        assert_eq!(state.selected().len(), 2, "the new pages arrive selected");
    }
}
