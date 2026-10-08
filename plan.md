# Plan — Phase 6: the EPUB editor

**Status: PLANNING. The design is settled with the owner (2026-10-08,
recorded in ROADMAP's Phase 6 section — the record of authority). The
technical research below is next. Nothing is built until the owner says go
(his 2026-09-29 rule).**

The previous plan (the 7.1 app-wide async migration) is superseded by this
one per the workflow; its outcomes are in the ROADMAP changelog rows through
2026-10-08 and its full text, including the leftover field findings, is
preserved in git history.

---

## Where the design came from

The owner's first instruction was to re-read the recorded prior discussion
before planning from scratch. Findings (2026-10-08): the prior thinking is
a spec, not a discussion — one line in docs/conversation.md's Sept-8 redux
("Inline EPUB Editing: non-destructive sidecar patches in `kalam.json`"),
the full four-layer spec in the old offline-roadmap Module 1 (Sept 18,
preserved in git history at `4b64e0c^:docs/offline-roadmap.md`), distilled
into ROADMAP Phase 6. The "how" was never discussed anywhere. Three
discussion turns with the owner then settled the design.

## The settled design (summary; full text in ROADMAP Phase 6)

Two surfaces kept (in-reader `[Fix Typo]` popover + proofreading pencil
toggle, and the full-screen editor). Markdown as presentation, never
storage: the default surface is Obsidian-Live-Preview style — the chapter
rendered by our own engine, click a paragraph to edit in place, changes
visible as you type, no Markdown round-trips. Raw HTML/CSS mode with one
Preview button toggling a debounced side-by-side pane rendered on the
service thread. Every edit, from every mode, is a pending patch stored in
the single `kalam.json` sidecar (already read at open) with a database
mirror, the same arrangement as annotations; nothing touches the `.epub`
until an explicit apply. Pending patches apply virtually at render.
Review panel on demand with a badge (no auto-prompt): before → after,
apply-all or apply-selected, baking through the sanitizer's safety net.
Applied patches kept as collapsed history; the undo story is the
`.epub.orig` backup. Patches apply in creation order; a non-matching
original is flagged, never silently skipped. Structural edits are a second
pending-op kind; replaced binary assets stage as files in the book folder.

## Research checklist (next planning turns — cite files and lines here)

- [ ] Re-read docs/pitfalls.md end to end (the phase-start rule). Entries
      already known to touch this work: §29 (Pango/emoji), §31,
      §32/§34/§35 (selection gestures — the popover and edit mode sit on
      the same surface), §38 (verify file-format writes in viewers other
      than the writer), §42, §50.
- [ ] Engine DOM access: how the engine exposes a parsed chapter
      (`crates/chapbook-layout/src/dom/`, `crates/chapbook-core`) and
      where an edit surface hooks in (`src/pages/reader/engine.rs` service
      thread); what in-place paragraph editing means against that DOM.
- [ ] Write-back stability: the core property — parse → serialize must be
      byte-identical for unedited content, and an edit must touch only its
      own span. Design the fixture tests that prove it before building
      anything user-visible.
- [ ] Patch store: the `kalam.json` schema extension in `src/sidecar.rs`,
      the database mirror and its migration (the next schema version
      after the current v17), and the "every write refreshes the sidecar"
      invariant (docs/WORKING.md §2).
- [ ] Virtual application at render: where pending patches are applied
      (engine layer vs app layer), and the Phase 6 verification item —
      `LayeredLocator` quote anchors surviving edits (fixture with
      annotations, then edits, then re-anchor checks).
- [ ] Bake path: reuse of the sanitizer's repack (mimetype-first
      uncompressed, `.epub.orig` backup, container re-verify) in
      `src/epub_sanitizer.rs`; how a partial apply (selected patches)
      composes with the creation-order rule.
- [ ] Review panel: one component used by the book details page and the
      editor chrome; before → after rendering from the patch records.
- [ ] Raw-mode editor widget: GtkSourceView (a new dependency — needs the
      README "what it enables" note per the anti-bloat rules), HTML/CSS
      syntax highlighting, the debounced preview bridge to the service
      thread.
- [ ] Structural ops: the model for split/merge/reorder/TOC edits as
      pending operations; staged asset files in the book folder.
- [ ] Done-when fixtures: fix a typo mid-paragraph, restructure a bad
      TOC, produce a clean `.epub` that opens correctly in another reader
      (§38's rule: verified outside our own stack).

## Draft build order (to sharpen after research; small first, big last)

1. Patch store (sidecar + db + migration) — no UI.
2. Virtual application at render + annotation-survival verification.
3. `[Fix Typo]` popover in the reader — the first visible slice.
4. Review panel + apply/bake on the book details page.
5. Proofreading edit mode (the pencil toggle).
6. Full editor: default rendered surface with in-place editing.
7. Raw HTML/CSS mode + side-by-side preview.
8. File tree, asset manager, visual TOC editor, structural ops.

## Open questions for the owner

None blocking research — the UX questions are settled. New questions land
here as the research turns them up, per the workflow.
