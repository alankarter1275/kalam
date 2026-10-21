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

**Backlog · priority High · filed 2026-10-09**
<https://linear.app/kalam-alan/issue/KAL-5/epub-editor-usability-overhaul-completely-useless-everything-about-it>

Owner's original verdict, verbatim:

> "making epub editor...usable. currently it's completely useless,
> everything about it is kind of a mess"

The earlier KAL-5 design and implementation are superseded by the owner's
2026-10-21 direction: remove the EPUB content editor for now rather than
continue its redesign. Reader inline editing, its Deep Editor/source entry
point, the standalone editor, picker/navigation entry, and pending-edit review
UI have been removed. There is no editor route through the reader, library or
app navigation. Do not continue implementation or mockups unless the owner
reopens the work.

If reopened later, editing is limited to HTML/XHTML and CSS edited directly.
Markdown support and Markdown conversion are out of scope. This decision does
not affect book metadata editing or annotations.

Legacy patch rows and sidecar fields remain only so older saved data is not
silently erased; the current app does not create, render, apply, or review these
patches.

**Pointers.**

- `plan.md` — the current deferral and future scope boundary
- `docs/kalam/INTEGRATION.md` — current reader integration and removed editor
- `src/pages/metadata_editor.rs` — unrelated book metadata editor, retained

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
