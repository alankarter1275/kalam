# KAL-5 — EPUB content editing (deferred)

**Status: Removed from the current product scope on 2026-10-21.** The owner's
latest direction supersedes the earlier design and implementation plans.

The EPUB content editor has been removed for now, including reader inline
editing, the reader's Deep Editor/source entry point, the standalone editor,
its picker and navigation route, and the pending-edit review UI. There is no
app route for editing EPUB content. Do not continue editor implementation,
mockups or design work unless the owner explicitly reopens the feature.

If reopened, the scope is **HTML/XHTML and CSS edited directly**. Markdown
support and Markdown conversion are out of scope. This is not permission to
start that work now.

Keep unrelated book metadata editing and annotations. Existing EPUB patch
records and sidecar fields are retained only to preserve data written by older
Kalam versions; the current app does not create, render, apply or review them.

See `Issues.md` for the issue mirror and `docs/kalam/INTEGRATION.md` for the
current reader boundary.
