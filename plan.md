# Plan — 2.18 + 2.20 (PDF half): instant, never-blocking PDF reading

**Status: DRAFT — planning phase. Open questions for the owner at the
bottom. No implementation starts until they are answered and this plan is
approved.**

Per the working agreement: research below is done and cited; the pitfalls
file has been consulted (§23, §27, §28, §29 — see the end); all owner
questions are asked here, during planning, not during implementation.

---

## Goal

The PDF reader feels instant: it opens immediately, the page on screen
loads first and a few pages around it follow (forward-biased), selection
works on the first click on every page (scanned or not), memory stays
bounded, and no part of the flow touches the UI thread with disk, database
or parsing work. Owner's principle, confirmed verbatim: **"the UI thread
never touches disk, database, or parsing."**

Roadmap items: **2.18** (PDF selection parity with the EPUB reader, which
also closes 2.10) and the **PDF half of 2.20**. The app-wide half of 2.20
(the whole app feels slow except the readers) is deliberately NOT in this
plan — it starts with measurement and gets its own plan after this one.

## What is true today (verified, with evidence)

1. **Opening a PDF blocks the UI thread.** `PdfReaderModel::new`
   (`src/pages/pdf_reader.rs:533`–) synchronously calls `PdfDocument::open`,
   `doc.outlines()` and renders the initial 1–2 pages
   (`doc.render_page_rgba`) inside the click that opened the book. Big
   scans pay all of it before the first frame.
2. **First click on a scanned page is a silent no-op.**
   `ensure_page_text` (`:986`) finds no vector text and no cached OCR,
   fires background OCR and returns `None`; the handlers
   `SelectionWordAt` (`:3723`), `SelectionLineAt` (`:3791`) and
   `SelectionDragBegin` (`:3599`) return without feedback. This is the
   owner's "double click doesn't work in pdfs" report.
3. **OCR is already a background worker** (channel + thread, spawned at
   `:2725`), and results are persisted in `page_ocr_cache` (schema v16),
   keyed by `"{size}:{mtime}"` (`ocr_file_fingerprint`, `:4143`) — each
   page is recognized once, ever; a changed file re-scans.
4. **OCR only ever runs while the reader is open** (`ensure_page_text` on
   open, `:2842`). Nothing at import time warms the cache.
5. **The image cache is bounded and windowed** — eviction of pages outside
   the keep-window plus `malloc_trim` (`:788`–`810`); prefetch window in
   `trigger_loads` (`:856`–`940`): paged mode = current spread + 1 forward
   + 1 back; continuous modes = current + 3 forward + 3 back.
6. **The text-layer cache (`page_text_cache`, `:369`) is unbounded** during
   a session — it grows with every visited page.
7. **Vector text extraction runs on the UI thread** inside
   `ensure_page_text`, on open and on every selection click.
8. **Handles differ between readers.** The PDF reader draws thin bars +
   circles in its own draw function (`:1195` area); the EPUB reader uses
   the engine's teardrop handles (`crates/kalam-reader/src/handles.rs`).
   The toolbar chip itself is already shared CSS (`k-sel-*`).
9. **The import pipeline runs on the task system** (background,
   cancellable, visible in the task manager); PDFs import through the
   `import_epub` PDF branch (`src/epub.rs:68`–), which already opens the
   document for the cover — a hook point for import-time OCR exists there.

## Design

1. **Instant open.** Model construction no longer opens the document. The
   reader window appears immediately with a page frame + the existing
   loading placeholder. One background job (via the existing render
   workers) opens the document, reads page count + outline + dimensions,
   renders the first visible spread, and publishes results via messages
   the UI applies. Closing the reader cancels it. Nothing in
   `PdfReaderModel::new` touches the file.
2. **One shared OCR queue.** Import enqueues OCR for image-only PDFs at
   low priority (visible in the task manager, progress, cancellable). The
   reader, when open, promotes its visible window to the front of the
   queue. Deduplicated by (book, page) through the existing
   `page_ocr_cache`. Only pages whose vector text is empty are enqueued.
3. **Text pre-warm, forward-biased.** After images are requested for the
   visible window, text for the same window is ensured on a worker
   (vector extraction off-thread; scanned pages via the queue). Jumping
   cancels pending text work like it cancels renders.
4. **Bounded text cache.** Text lives under the same keep-window as
   images (text is ~20–60 KB/page, so the window may be wider — e.g.
   2× the image window — but it must have a cap).
5. **First-click selection.** A click on a page whose text is not ready
   stores a pending intent (page, point, kind) and signals it; when the
   text arrives the intent executes automatically. With import-time OCR
   and pre-warm this path is rare, but it exists so the first click is
   never a dead click.
6. **Handle parity.** The PDF draw function renders the same teardrop
   handles the EPUB engine uses (same shape, size, color), so both readers
   look and feel identical. Plain text only in any new UI string — no
   emoji (pitfalls §29), no novelty icons (owner: professional only).
7. **UI-thread discipline.** Document open, outline, render, text
   extraction, OCR cache reads/writes and progress saves all run on
   workers / the task system. Verified by code review against the plan,
   and by new timing spans (`pdf_open_*`) that must show ~0 ms of UI
   blocking.

## Steps (each step leaves CI green, committed separately)

1. Move document open + outline + first render off the UI thread
   (skeleton-first open). Add `pdf_open_*` timing spans.
2. Off-thread vector text extraction + pre-warm window + bounded text
   cache.
3. Pending-selection intent + feedback (per the owner's answer to Q2).
4. Import-time OCR queue with task-manager entry + reader promotion.
5. Teardrop handle parity with the EPUB reader.
6. Owner field test (see below), fixes, then: record outcome in ROADMAP
   (2.18 + 2.10 close, 2.20 PDF half), clear this file.

## Test plan

- **CI/unit:** queue priority + dedup; keep-window eviction of text cache;
  pending-intent executes on text arrival; existing 870 tests stay green
  (no test count drop expected beyond intentional changes); perf budgets
  unaffected; the existing PDF OCR probe stays green.
- **Owner, on device:** open a large scanned PDF — window appears
  immediately, first page paints without a freeze; double-click a word on
  a page never opened before — it selects (or signals and then selects);
  handles look identical to the EPUB reader; scroll a 300-page scan —
  memory stays flat (watch `htop`); import a scanned PDF — task manager
  shows the OCR job with progress, cancellable.

## Pitfalls consulted

- §29 — no color emoji in any new string (the feedback signal is text).
- §28 — shared code care: `k-sel-*` CSS and `PdfPageText` methods are
  shared with the EPUB reader; check every helper this plan touches for
  other users before changing it.
- §23 / moved-base fault — pull --rebase before every push; after any
  sandbox reset, fetch + `reset --mixed origin/<branch>` before trusting
  grep results.
- §27 — this plan's "What is true today" was verified against the code in
  this session, not quoted from memory or old docs.

## Open questions for the owner (planning phase)

1. **Import-time OCR cost.** A 300-page scanned book takes minutes of
   background CPU at import (each page ≈ seconds of neural inference).
   It is background, visible and cancellable in the task manager, and the
   book opens instantly either way. Option A: OCR the whole book at
   import. Option B: OCR only the first ~10 pages at import, the rest
   during first reading. Which do you prefer?
2. **Feedback on a not-yet-recognized page.** If you click a page whose
   text is not ready, should the app show a small brief "Recognizing
   page…" message before the selection lands — or stay silent and simply
   select the moment it is ready?
3. **The whole-app slowness (drives the NEXT plan, not this one).** You
   said everything except the readers feels slow. A rough order helps:
   which feels slowest — Home, the Library grid, a book's page, author
   pages, dialogs, or sidebar navigation? And is it opening things, or
   scrolling, or both?
