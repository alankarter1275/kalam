# KAL-5 — EPUB editor usability overhaul

**Status: Implementation in progress; authorized 2026-10-21.** The first
permanent-save slice is drafted in the working tree and has not yet been built
or tested. The approved product direction is no longer awaiting authorization;
the remaining work is to finish, validate, measure, and phase the experience.

## Current editor behavior

Before KAL-5, the shipped editor staged guarded paragraph and chapter-source
changes as patches; it also staged chapter ordering and cover replacement.
Pending text patches can appear in the reader. Applying selected patches
rewrites the EPUB, preserves the first `.orig` backup, updates the book hash,
and reindexes search. The editor has no general archive file tree or
arbitrary-file editor.

**Implementation in progress:** in-reader selection/paragraph fixes, full-editor
paragraph edits, and raw-source Save now target direct EPUB writes in the local
worktree. The writer uses entry and archive hash guards, validates the changed
resource and repacked package, makes backups, atomically replaces the archive,
and refreshes catalog/search/sidecar state. This code is not yet compiled or
tested. Chapter ordering and cover replacement are temporarily unavailable
rather than allowed to create new patches; the legacy patch panel remains for
review/apply/discard of existing pending edits. The picker still uses its
existing title-sorted cover grid, and the editor still lacks a general archive
file tree.

The entry points are the **Editor** sidebar picker and the per-book pencil in
the reader sidebar. The current side-by-side source preview is a debounced
raster from Kalam's native renderer, not WebKit.

## Owner's direction — 2026-10-21

The owner wants a modern, Calibre-capable editor and a book-like editing
experience, with implementation details delegated to the agent.

- **In-reader editing is for small fixes.** Select text, choose Edit, and get a
  caret at that text. Keep the reader page and its layout as-is. Save makes the
  edit permanent in the EPUB; do not create new pending patches for this flow.
- **Deep editor layout.** A toggleable file tree on the left, editor in the
  middle, and a toggleable preview on the right.
- **Markdown-style editing.** The Light mode should feel like Obsidian Live
  Preview. Deep mode edits XHTML/CSS source and can show the rendered result in
  the same right-hand preview.
- **Advanced tools.** Aim for most of Calibre's useful EPUB tools, delivered in
  phases rather than blocking the usable core editor.
- The user is comfortable with the EPUB being permanently changed. Their
  performance concern is UI responsiveness, delay and flicker—not retaining a
  patch queue.

This supersedes the earlier hybrid proposal that kept new in-reader fixes as
patches. Existing pending patches from the shipped editor still need a safe
one-time review/apply/discard path; they must not be silently dropped.

## Recommended behavior

### In-reader quick edits

1. Select text and choose **Edit** from the existing selection controls.
2. Put a text-editing layer exactly over the selected words, styled to match
   the book. The reader remains visible and does not turn into a separate
   editor page.
3. Show a compact floating formatting toolbar only while editing, anchored
   near the selection/caret and moved above or below it to avoid covering the
   text. Provide common actions such as bold, italic, underline, link, Save and
   Cancel; keyboard shortcuts can supplement it. The toolbar should not change
   the page layout.
4. Save once on the user's command, not on each keystroke. The edit writes
   directly into the EPUB and then refreshes the visible passage at the same
   reading position. On failure, leave the edit open and the text intact.

The reader is painted by Kalam's reading widget rather than being a normal text
field. The caret and toolbar therefore need to be a carefully aligned overlay;
they can look in-place, but are not literally part of the page-painting canvas.

### Deep editor and Markdown-style Light mode

- Keep the file tree and preview independently collapsible/resizable. Show a
  clear book name, selected resource, editing mode, dirty/saved state, and
  Save action so it is always obvious where an edit will go.
- In **Light mode**, use a Markdown-like editing surface: common prose markup
  is formatted as the user types, with Obsidian-style markers visible near the
  cursor. The right pane shows the actual EPUB rendering through Kalam's native
  renderer. This is a Markdown-like control surface, not a `.md` file stored in
  the book.
- Do not convert an entire chapter through a generic Markdown converter.
  Preserve the original XHTML and update only edited supported content. Start
  with a safe subset (paragraphs, headings, emphasis, simple lists and links);
  show unsupported XHTML as raw markup and keep it untouched unless the user
  explicitly edits that raw part.
- In **Deep mode**, use a syntax-highlighted XHTML/CSS editor. The same
  right-hand Preview toggle shows a rendered chapter. When a stylesheet is
  selected, let the user pick which chapter to preview.
- Refresh preview after a short quiet period, discard older renders when a
  newer edit is underway, and show when the preview is stale or failed.

### Direct saves, backups and validation

- Both the in-reader editor and deep editor write directly to the EPUB; new
  edits do not enter the pending-patch queue.
- Preserve the previously selected safety rules: validate the touched
  XHTML/CSS and the repacked ZIP/OPF, and refuse stale or invalid writes while
  keeping the user's buffer. Offer reload or save-a-copy for a conflict; no
  silent force-save.
- Back up before direct writes. The earlier choice was a timestamped backup
  for every direct Save. Applying that to every one-line in-reader edit would
  copy the whole EPUB each time and can consume substantial storage. Agent
  recommendation: keep timestamped backups per deep-editor Save, but make one
  pristine backup before the first in-reader edit and reuse it for subsequent
  quick fixes. Use this as the initial policy; revisit only if measurements or
  recovery testing show a concrete reason to change it.
- Write to a temporary archive and replace the EPUB only after validation.
  Update the catalog hash and search index after success. Archive work,
  backups, validation and indexing run off the GTK thread.
- The shipped patch review remains only as a migration path for legacy pending
  edits. Resolve those before starting direct editing; do not apply or discard
  them automatically.

## Performance assessment

The archive write can take noticeable wall-clock time for a large book or slow
drive, especially when a full backup is made. It does not have to freeze the
UI: backup, archive rewrite, validation and reindexing can run on a worker while
the current page stays visible with a small **Saving…** status. Kalam's current
baker raw-copies untouched ZIP entries, so it need not decompress and
recompress every asset, but it still reads/writes the archive and the edited
entry.

There is a separate refresh risk. `ReaderView::open` synchronously opens the
ZIP/package and loads fonts, and the current reader reload path calls it on the
GTK thread. Reusing that path unchanged after every small save could cause a
brief pause or flicker. The editor must not clear the page while saving; the
post-save refresh should preserve the locator and swap in the updated passage
only when it is ready. Measure this on real books before promising a timing.

## Phased scope

**Core first:** reader-like in-place quick edits, clearer picker, file tree,
Light/Deep modes, native preview, direct saving, backup/validation, and safe
legacy-patch handling.

**Follow-up tools:** add/delete/rename/merge/split archive files; visual TOC;
full Check Book with cross-file/link/resource checks; saved searches; and
formatting, punctuation and font tools. Keep the advanced list in scope for the
program, but schedule it after the core editor is usable.

## Product feedback and validation

No further design approval blocks implementation. The owner's original
first-use complaint remains broad—navigation felt unusable and capabilities
were missing—and no concrete screen/action examples have been supplied. After
the core path is buildable, test it end-to-end on real books and use the owner's
first-use feedback to prioritize the next interaction improvements. Report any
trade-off that changes the approved feel rather than silently changing scope.
