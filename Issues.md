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

Owner's verdict, verbatim:

> "making epub editor...usable. currently it's completely useless,
> everything about it is kind of a mess"

Same day, on the rail's new picker page:

> "it looks horrendous"

**Context.** Phase 6 shipped all 10 steps, each CI-green (602 tests,
screenshot passes): inline selection editing in the reader, the patch
matcher/serializer, review panel + bake, proofreading mode, the full
editor page + the reader's TOC pencil, raw source mode, structural ops
(spine reorder, staged cover replacement), and the rail Editor entry +
picker page. Every piece is verified by tests and CI screenshots — and
the whole is still a mess to use. **The gap is UX, not machinery.**

**First step: gather the owner's concrete complaints.** The
assessment is total but not yet itemized. Before touching code, walk
the flows screen-by-screen with the owner and write the list: entry
points and discoverability, the edit → review → bake loop, the editor
page's chrome, the picker page's look, performance feel, and trust
(what the bake does to the file, `.orig` backup). Note: the agent
cannot view screenshots in its environment — complaints need to
arrive as words.

**Pointers.**

- `plan.md` — Phase 6 step log (settled design + per-step outcomes)
- `docs/pitfalls.md` §72–§96 — the phase's lessons
- `src/pages/epub_editor.rs` (~1.9k lines), `src/pages/editor_picker.rs`,
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
directly into the file. kalam today keeps PDF highlights DB-side
(overlay, file untouched). Recommended: the Phase 6 pattern — stage
edits DB-side for instant display and undo, then a
review-and-**bake-into-the-file** step (`.orig` backup, same as the
EPUB editor).

**Sizing — a phase, not a step.** Suggested order:

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
- Precedent for staged edits + bake: the EPUB editor (`plan.md`
  Phase 6, `src/epub_patches.rs`, `src/epub.rs`)
