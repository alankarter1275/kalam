# Issues — the tracked work

Kalam's backlog lives in Linear (workspace `Kalam-alan`, team `KAL`).
This file mirrors it in the repo so the plan is readable without a
Linear login, and so `git log` carries the record with the code.

The record's rules:

- **Linear is the tracker of record** — statuses, priorities and
  discussion live there. This file is the mirror, not the master.
- Every issue filed for kalam gets a section here the same day. When
  a status or priority changes in Linear, the line here changes with
  it, in the same commit as any related work.
- KAL-1 through KAL-4 ("Get familiar with Linear", "Connect your
  tools", "Import your data", "Set up your teams", filed 2026-10-07)
  are Linear's own onboarding samples, not project issues — they are
  deliberately not mirrored below.

---

## KAL-5 — EPUB editor usability overhaul

**In Progress · priority High · filed 2026-10-09**
<https://linear.app/kalam-alan/issue/KAL-5/epub-editor-usability-overhaul-completely-useless-everything-about-it>

Owner's verdict, verbatim:

> "making epub editor...usable. currently it's completely useless,
> everything about it is kind of a mess"

Same day, on the rail's new picker page:

> "it looks horrendous"

**Baseline and implementation status.** Before KAL-5, paragraph and
chapter-source edits, chapter ordering, and cover replacement could create
pending patches. The patch panel applies selected legacy edits by rewriting the
EPUB and refreshing its hash/index. The first direct-save slice is now drafted
in the working tree: reader selection/paragraph fixes, deep-editor paragraph
edits, and raw-source Save write into the EPUB rather than creating patches.
It has not yet been compiled or tested. Chapter ordering and cover replacement
are temporarily unavailable rather than creating new patches; the legacy panel
remains the safe review/apply/discard path for existing pending edits. The
editor still lacks a general archive file tree.

**Owner's direction (2026-10-21).** Both in-reader quick edits and deep-editor
changes should save directly into the EPUB; do not create new pending patches.
In-reader edits should remain book-like, with an inline caret and a small
contextual formatting toolbar. The deep editor should have a toggleable file
tree, a central editor, and a toggleable native-render preview. Light mode
should feel like Obsidian Live Preview while preserving XHTML safely; Deep mode
edits XHTML/CSS source. Build the core first, then schedule most advanced
Calibre-like tools (file operations, visual TOC, Check Book, saved searches and
formatting tools).

**Safety direction:** Review legacy pending patches before direct editing;
validate touched XHTML/CSS and the repacked ZIP/OPF; refuse stale or invalid
writes while preserving the user's buffer. The owner previously selected
per-save timestamped backups for direct saves. The agent recommends keeping
those for deep-editor saves, but making one pristine backup before the first
in-reader edit and reusing it for quick fixes, to avoid copying the whole EPUB
for every small correction. Archive writes run off the GTK thread, but the
reader refresh still opens the view synchronously; its delay/flicker needs
engineering work and measurement. The implementation is authorized and in
progress; see `plan.md` for the full behavior and remaining phases.

**Pointers.**

- `plan.md` — current KAL-5 plan, implementation state, and remaining work
- `docs/pitfalls.md` §72–§96 — implementation lessons
- `docs/manual-test-checklist.md` — current editor checks
- `src/pages/epub_editor.rs`, `src/pages/editor_picker.rs`,
  `src/pages/edits_panel.rs`
- `resources/style.css` (`kalam-*` classes)
- Standing: the §38 field check (baked EPUB opens correctly in
  another reader) is still open on the owner's side

**Definition of done.** The owner uses the editor on a real book
end-to-end without calling it useless — and can say what it does in
one sentence.

---

## KAL-6 — PDF tools: annotations, ink, page surgery — browser-parity (Zen/Edge) and beyond

**Backlog · priority Medium · filed 2026-10-09**
<https://linear.app/kalam-alan/issue/KAL-6/pdf-tools-annotations-ink-page-surgery-browser-parity-zenedge-and>

Bring kalam's PDF reader to browser-parity (Zen / Edge) and beyond:
annotations, ink, page surgery.

**Feasibility — confirmed 2026-10-09, no new dependency.** Audited
against **mupdf 0.8** source — the exact engine already in
`Cargo.toml`, currently used read-only. The editing machinery is
already compiled into the binary.

Possible (all verified in the crate's API):

| Feature | Backing API |
|---|---|
| Text highlight / underline / strikethrough / squiggly | real PDF annotations with quad points — kalam already extracts per-char text boxes (selection) and has the color/style drawer UI |
| Pen / ink | ink annotations with full stroke lists |
| Sticky notes + typed text on page | text annots (author, color, opacity, popups), free-text |
| Reorder pages | `move_page` |
| Delete pages | `delete_pages` (ranges) |
| Add/merge files | `insert_pdf` at any position |
| Rotate | `set_rotation` |
| Extract pages | `select_pages` + save |
| Form filling | `PdfWidget` API |
| True redaction (remove content under blackout) | `apply_redaction` — beyond what browsers offer |
| Dark mode | render-side |

Smart crop is already shipped. All of it saves via `Document::save`
(full or incremental).

Not possible / off-character:

- **Read aloud** — a whole TTS subsystem, not a PDF feature; offline
  voices are robotic.
- **Cloud AI summarize/translate** (Edge-style) — out of character
  for an offline reader.
- **In-place editing of existing page text like a word processor** —
  no engine does this; browsers don't either. Drawing *new* text over
  the page is possible (`src/shape/` in the crate).

**Architecture decision needed.** Browsers write annotations
directly into the file. Kalam keeps PDF highlights in the database and
paints them over the page; it does not write them into the PDF. One option
is to stage edits for review, then offer an explicit background bake with a
pristine `.orig` backup, following the current EPUB patch workflow.

**Suggested implementation order:**

1. Highlights + notes written into the file (selection UI and drawer
   already exist)
2. Ink pen (stroke capture layer on the page)
3. Page surgery — reorder/delete/rotate/merge (engine calls are
   trivial; the thumbnail-rail UI is the work)

**Pointers.**

- `src/pdf.rs` — kalam's read-only wrapper today;
  `src/pages/pdf_reader.rs` (~5.9k lines) — the reader UI
- mupdf-rs 0.8: `src/pdf/annotation.rs`, `src/pdf/document.rs`
  (`save` / `insert_pdf` / `move_page` / `delete_pages` /
  `select_pages`), `src/shape/` (draw-onto-page), forms via
  `PdfWidget`
- EPUB patch and bake implementation: `src/epub_patches.rs`,
  `src/epub_sanitizer.rs`, `src/epub.rs`
