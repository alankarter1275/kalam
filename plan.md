# KAL-5 — EPUB content editing (built; further work deferred)

**Status: Reopened by the owner and built. Further work is deferred until the
UI redesign is complete.**

The editor was removed from the product scope on 2026-10-21. The owner then
explicitly reopened the feature and directed implementation to start, so that
removal no longer describes the product. The editor is live: `NavItem::Editor`
in the navigation rail (decision 102), `Route::EditorWorkspace`,
`src/pages/epub_editor.rs`, `editor_picker.rs`, `epub_structure.rs`,
`epub_tools.rs`, `text_diff.rs`, `font_subset.rs` and `db/editor.rs`, with
`sourceview5` as a dependency.

The content scope is unchanged from what was specified: **HTML/XHTML and CSS
edited directly**. Markdown support and Markdown conversion are out of scope.

What is deferred is *further* work. The owner's direction is to finish the UI
redesign first and then come back to the editor's known issues as their own
pass. Redesign work must not remove or stub the editor out, and no new editor
feature should be started without asking.

This file was not updated when the feature was reopened. That is why it, along
with `ARCH.md`, `ROADMAP.md` and `docs/redesign-audit.md`, contradicted the
shipped code.

Keep unrelated book metadata editing and annotations. Existing EPUB patch
records and sidecar fields are retained only to preserve data written by older
Kalam versions; the current app does not create, render, apply or review them.

See `Issues.md` for the issue mirror and `docs/kalam/INTEGRATION.md` for the
current reader boundary.
