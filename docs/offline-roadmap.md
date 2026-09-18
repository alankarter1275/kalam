# Kalam Offline Master Plan (Part 1)

This document is the single source of truth for **Part 1** of Kalam's development. 

Part 1 focuses entirely on building a **blazing fast, fully capable, robust offline ebook reader, editor, and library management system**. All online components (web plugins, online scrapers, MangaDex/AO3/FicHub clients) are strictly deferred to **Part 2**. The final UI design overhaul is scheduled at the very end of Part 1 once all offline features and performance foundations are complete.

---

## Read this before picking up work (added 2026-09-18)

The six modules below are a **list of everything Part 1 wants**, not a queue.
Left unmarked, every session picks a different starting point and the work
comes out in fragments. So: check the status first, then take the next item
from [the order](#suggested-order), not the next item in the document.

Status was verified against the code on 2026-09-18, not copied from an older
claim. Re-check before trusting it.

### Already built

The comics reader (CBZ/CBR, ~2,000 lines), the metadata editor with Open
Library and Google Books, author pages, series cover stacks, smart shelves
with a real rule engine, ratings, streaks and analytics, saved words and
quotes with Markdown/CSV/Anki export, the multi-library switcher, the
background task manager (`src/tasks.rs`), cover preloaders and persistent
thumbnails (`src/preload.rs`, `src/thumbs.rs`), the offline dictionary system,
the `kalam.json` sidecar beside every book, find-in-chapter, the reading
location pill, the bookmarks panel, and the teardrop selection handles
(`crates/kalam-reader/src/handles.rs`).

### Partly built — finish, don't restart

- **Text layout controls.** The engine hyphenates and justifies, but there is
  no user-facing toggle for either.
- **In-book search.** Exists as find-in-chapter; the roadmap wants a
  library-wide match list with counters and next/previous.
- **PDF.** See the warning below — this one is further from done than it looks.

### Not started

Full-text search across the library (the `tantivy` crate is not a
dependency), the full EPUB editor, sidecar typo patches, folder-watch
auto-import, the duplicate finder, "Abandoned" as a reading status, custom
keybindings, footnote popovers, jump-back history, dual-page view, a
user-supplied fonts folder, and the Material 3 redesign.

### ⚠️ The PDF reader is not doing what the roadmap assumes

`src/pdf.rs` has no real page rasterizer. `render_page_image_uncropped` looks
for an embedded image in the page and, if there is one, returns it — so
**scanned PDFs work**, and smart crop trims their margins correctly. When
there is no embedded image it falls back to `render_text_to_canvas`, which
draws a black bar for each line of text. Page mode is the default
(`reflow_mode: false`), so **opening an ordinary text PDF shows a page of
black rectangles.**

Reflow mode does work — it extracts and shows the real text. So the smallest
honest fix is to default to reflow when a page has no embedded image, and say
so on screen. That is a small change and it removes a visibly broken state.

Real page rendering is a separate, deliberate decision — it means taking on a
rendering library (`lopdf` parses PDFs but cannot rasterize them, and writing
a rasterizer is not a realistic option). The candidates and the trade-offs are
recorded under **"Which PDF engine?"** in `docs/conversation.md`.

---

## Open items — recorded 2026-09-18, nothing actioned yet

Everything below was found during the code review and **deliberately not
fixed**. It is written down so it does not have to be rediscovered, and so a
session that picks up Part 1 knows what is already known-broken. Sorted by how
much it matters, not by how easy it is.

### Blocking or near-blocking

- **Engine migration is unfinished and the damage list does not exist.**
  Replacing WebKit with `kalam-reader` broke things built on top of it and the
  repairs are still in progress. There is no written list of what is broken.
  Until there is, "the migration is done" is not a claim anyone can check, and
  every new session rediscovers the same breakage. **Creating that list is
  step 1 of the order above.**
- **CI never covered the engine — fixed, unverified.** `--workspace` was added
  to the clippy and test steps on 2026-09-18. Before that, one test binary ran
  (`unittests src/main.rs`) and three workspace members were never even
  compiled. The first run with the flag may fail; a failure there is
  information, not a regression. **Needs a push to confirm.**
- **PDF default mode is visibly broken** for text PDFs (black bars). See
  above.
- **PDF engine undecided.** See
  [Which PDF engine?](./conversation.md#20-which-pdf-engine-2026-09-18) in
  `docs/conversation.md` for the comparison. Recommendation on rendering
  quality alone: **MuPDF**, narrowly ahead of PDFium, both clearly ahead of
  Poppler.

### Contradictions between documents — settle before Part 2

- **The plugin substrate has three different answers.** `ARCH.md` says Lua.
  `docs/conversation.md` records "Lua pivot reversed: Pure Rust" with fragile
  selectors in a TOML file, and *then* "WebAssembly plugin ecosystem replacing
  Lua". The scaffolding that exists (`wit/kalam.wit`, `plugins/ao3`) is Wasm.
  `src/sources/scrapers/mangaball.toml` is the TOML approach and nothing reads
  it. Pick one and delete the other two sets of notes.
- **`docs/ci/github-actions-ci.yml` claims to be the canonical copy of the
  workflow** (README says so) but has drifted ~130 lines from
  `.github/workflows/ci.yml`. One of them should be deleted.

### App-level rules that do not exist yet

`crates/kalam-reader` inherits strict boundaries from upstream
(`docs/kalam/archive/RESTRICTIONS.md`: no C++ stylo, no WebKit, no JS bridges)
which is a large part of why that code stayed clean. **`src/` has no
equivalent.** `docs/WORKING.md` has four invariants and only one of them (zero
`unwrap`) is checkable. See the discussion in `docs/conversation.md` under
**"Guidelines for the app half"**.

### Small, safe, and worth doing in one sweep

- **Three unused dependencies** in the root `Cargo.toml`: `toml` (zero uses),
  `urlencoding` (zero uses), `scraper` (used only by the disabled
  `src/plugins/mod.rs`). They cost build time for nothing.
- **No logger is installed.** `log = "0.4"` is only in `[workspace.dependencies]`.
  The engine emits `log::` calls that go nowhere, and the app has 83
  `eprintln!` sites in non-test code — all invisible when launched from the
  `.desktop` file, which is the normal way to launch it.
- **Icon installed at the wrong size.** `Makefile` puts a 128×128
  `assets/logo.png` into `icons/hicolor/512x512/apps/`, so the system upscales
  it. A 2048×2048 source already exists at `docs/design/logo_transparent.png`.
- **No file association.** `resources/app.kalam.Kalam.desktop` has no
  `MimeType` and `Exec=kalam` has no `%f`, and `main.rs` never reads `argv`.
  So "Open with Kalam" on an `.epub` cannot work.
- **`src/plugins/mod.rs` cannot compile.** It imports `wasmtime`, which is in
  neither `Cargo.toml` nor `Cargo.lock`. It survives only because `main.rs`
  has `// mod plugins;` commented out. Delete it or fix it; do not leave it.
- **`epub_write.rs` and `epub_writer.rs`** are unrelated files with
  near-identical names (OPF metadata writeback vs building an EPUB from
  fetched HTML), and the generic `human_size()` helper lives in the former and
  is imported from `app.rs` and `settings.rs`.
- **`src/downloads.rs` breaks the repo's own zero-`unwrap` rule** — six
  `lock().unwrap()` calls on a mutex touched from a worker thread, which is
  exactly the cascade-panic that `src/tasks.rs:52` documents and guards
  against. It also writes to `std::env::temp_dir()` instead of going through
  `src/paths.rs`, and has no rate limiting. Part 2 code, but it should use
  `src/tasks.rs` when it is picked up.
- **`paths.rs::home_dir()` reads only `$HOME`**, falling back to
  `PathBuf::from(".")`. With `HOME` unset the app creates a `kalam/` folder in
  the current directory.
- **90 `#[allow(dead_code)]` attributes, 5 of them module-wide**
  (`src/dict.rs`, `src/shelf_rules.rs`, `src/db/pronunciation.rs`,
  `src/pages/reader/engine.rs`, `src/plugins/mod.rs`). CI's `-D warnings` was
  added specifically to catch dead code; these suppress it wholesale.
- **`ROADMAP.md` is 2,669 lines** and the README tells every new session to
  read it. Needs trimming to a dated log.
- **No `LICENSE` file at the repository root**, despite
  `license = "GPL-3.0-or-later"` in `Cargo.toml`. Low priority for a personal
  project, but the repo is public.

### Upstream relationship — needs a decision

`docs/kalam/UPSTREAM.md` describes a monthly routine of cherry-picking fixes
from the original chapbook repository. The engine crates are now ordinary
workspace members that Kalam edits directly. **Editing in place and pulling
from upstream at the same time produces conflicts.** Either stop tracking
upstream, or keep a clean boundary between "files we edit" and "files we
don't". The routine also assumes the merged upstream history is present; this
could not be verified here because the working checkout is shallow.


### Suggested order

Deliberately front-loads finishing the engine swap, because the editor, the
sanitizer and the search index all sit on top of it and would be built twice
otherwise.

1. **Finish and verify the reading-engine migration.** Keep a written list of
   what the WebKit replacement broke; the migration is done when that list is
   empty. Nothing below should start before this.
2. **Fix the PDF default mode** (small; stops a visibly broken screen).
3. **Make `LibraryService` asynchronous.** It was designed for this and is
   currently synchronous, so this is finishing a design, not starting one. It
   unblocks the smoothness everything else is judged on.
4. **Reading-engine completeness**: text layout toggles, footnotes, jump-back
   history, dual-page view, custom fonts folder.
5. **Library-wide full-text search** (`tantivy`). Largest single new
   subsystem; do it after the service layer is async so indexing can run in
   the background.
6. **Ingestion**: the sanitizer pipeline, then folder-watch auto-import.
7. **The EPUB editor.** Last of the engine work and the biggest item — it
   depends on 1, 4 and 6.
8. **Library management odds and ends**: duplicate finder, "Abandoned"
   status, inline metadata editing, bulk edits.
9. **Performance budgets.**
10. **Material 3 redesign.** Deliberately last, exactly as this document
    already says: every step above adds screens.

---

## Module 1: The Reading Engines & EPUB Editor

### 1. EPUB & Reflowable Text Engine (Kalam Engine)
* **Core Reading Experience**:
  * Crisp typography with user-selectable fonts, font sizes, line heights, margins, and themes (Light, Sepia, Dark, OLED/Ink Pure Black).
  * Dual-page view in widescreen mode and continuous vertical scrolling option.
  * Table of Contents (TOC) hierarchy navigation, reading progress tracking, and bottom-right reading location percentage pill.
  * Ribbon bookmark button in top-right header with bookmark management panel.
  * **"Jump Back" History (Breadcrumbs)**: Instant return button whenever jumping to footnotes, TOC headers, or search matches.
  * **Auto-Hiding Mouse Cursor**: Mouse pointer disappears after 2 seconds of inactivity so it never blocks words.
  * **Custom Keybindings & Mouse Wheel Tuning**: Fully configurable shortcut keys and adjustable mouse wheel scroll step sensitivity.
  * **Custom Fonts Folder**: Drop `.ttf` / `.otf` fonts into Kalam's font directory without needing root OS installation.
  * **Text Justification & Hyphenation Toggle**: Easily toggle between left-aligned (ragged right) and fully justified text with smart hyphenation.
* **Text Selection & Tooling**:
  * Fluid mouse/touch selection with custom teardrop drag handles.
  * Double-click to select word; triple-click to select paragraph.
  * Action Toolbar:
    * Highlight with multi-color palette (Yellow, Green, Blue, Pink, Orange) + Underline style.
    * `[Fix Typo]` inline action popover: type the correction, hit Enter, and the fix is immediately saved as a sidecar patch without interrupting reading flow.
    * Quote, Dictionary lookup, and Copy actions.
* **Non-Destructive Sidecar Patches & Permanent EPUB Baking**:
  * **Sidecar Patches (Default)**: While reading, typo fixes and notes are saved as non-destructive XPath/DOM replacement rules in the database and `kalam.json`. Original `.epub` files on disk remain untouched.
  * **"Apply Patches to EPUB" (Permanent Bake)**: An explicit button in book tools allowing users to permanently write all accumulated sidecar typo fixes directly into the `.epub` archive and re-verify its container structure.
* **Proofreading Edit Mode (In-Reader)**:
  * Dedicated pencil toggle in reader chrome allowing one-click paragraph editing inline with hover highlights.
* **Obsidian-Style Full EPUB Editor (Calibre Power, Modern Zen UI)**:
  * A dedicated full-screen book editor route for comprehensive book crafting and remodeling:
    * **Live Preview Surface (Default)**: Obsidian-like WYSIWYG editor where HTML markup is presented as clean, intuitive Markdown-style formatting (`# Headings`, `**bold**`, `*italics*`, `> blockquotes`, images).
    * **Raw HTML / CSS Code Mode Toggle**: One-click switch for power users to edit raw HTML tags, class attributes, and stylesheets with syntax highlighting.
    * **Book File Tree & Asset Manager**: Left sidebar showing all XHTML chapters, stylesheets (`.css`), embedded fonts, and images.
    * **Table of Contents (TOC) & Chapter Editor**: Visually rename chapters, nest sub-chapters, reorder sections, or split/merge chapters.
* **Quote-Anchored Locators & Robust CFI**:
  * Locators dynamically re-anchor to the exact text snippet regardless of reflow, font resizing, or window dimension changes.
  * Clicking an annotation, quote, or bookmark in the sidebar jumps directly to the exact highlighted phrase.
* **Footnotes & Cross-References**:
  * Clicking superscript links like `[1]` opens an instant footnote popover or jumps to the reference note with a quick back navigation trigger.

### 2. PDF Engine (Zathura-Style Smart Engine)
* **High-Performance Rendering**:
  * Crisp PDF rendering backed by Google's PDFium engine (`pdfium-render`).
* **Smart Margin Crop**:
  * Automatic ink-boundary detection: detects the actual text and illustration bounding boxes on each page and automatically crops/zooms to remove empty whitespace margins, maximizing screen real estate (Zathura-style).
* **Text Reflow Mode**:
  * Heuristic text layer extraction guessing paragraph boundaries based on line spacing/indentation, converting text-heavy PDFs into reflowable text with customizable fonts, themes, and font sizes.
* **PDF Navigation & Bookmarks**:
  * PDF outline / Table of Contents tree, page jump box, and page bookmarking.

### 3. Comics & Manga Engine (CBZ/CBR)
* **Reading Modes**:
  * Single page, Left-to-Right (Western), Right-to-Left (Manga), and Webtoon continuous vertical scroll mode.
* **Manga Night / Dark Mode (Color Inversion)**:
  * One-tap dark mode inversion for black-and-white manga pages (white page backgrounds become dark, black ink lines become crisp white) for strain-free reading in dark rooms.
* **Double-Page Spreads**:
  * Automatic dual-page spread detection in landscape/widescreen mode.
* **Smart Page Splitting & Stitching**:
  * Automatic splitting of dual-page scans into single pages when reading in portrait/narrow window mode.
* **Manga Zoom Loupe (Magnifying Glass)**:
  * Smooth right-click/press magnifying loupe to inspect small speech bubble text or detailed artwork without full-page zoom.
* **Image Tuning Filters**:
  * Instant contrast, sharpness, and brightness adjustments to clean up faded manga scans and enhance dark artwork.
* **Zero-Latency Preloading**:
  * Viewport memory caching with background preloading of adjacent pages (`page ± 2`) for instant page turns.
* **Comic Remaster Tool**:
  * Background worker to upscale and clean up low-resolution vintage scans using CPU Lanczos3 scaling (`image` crate), preserving crisp black ink lines while smoothing screentones, repacked cleanly into an enhanced archive.

---

## Module 2: Under-the-Hood Architecture & Performance (Yazi-Style)

* **Asynchronous `LibraryService` Layer**:
  * UI components never run blocking SQLite queries on the main thread.
  * Requests are routed through an async `LibraryService` actor pattern, ensuring 60 FPS UI responsiveness.
* **Task Manager (`tasks.rs`)**:
  * Centralized queue for long-running jobs (importing books, batch metadata fetching, full-text index rebuilding, comic remastering, EPUB patch baking) with progress bars and cancellation support.
* **Smart Preloaders & Memory Safety**:
  * Preloading next chapter in the background while the user reads.
  * Async cover texture generation and memory-bounded thumbnail caching (surviving restarts via persisted ~200px thumbnails).
* **Bubble Memory & Floating Window Host**:
  * Lightweight overlay host allowing book cards, quick notes, and dictionary popups to float above active views without reloading the page or leaking memory.

---

## Module 3: Content Sanitizer & Deep Content Search

* **EPUB Ingestion Sanitizer Pipeline**:
  * Automatic background cleaning of imported EPUBs upon drag-and-drop:
    * Unzips EPUB archive.
    * Strips toxic hardcoded CSS (e.g. forced 8px fonts, fixed margins, forced color overrides).
    * Repairs broken XML syntax.
    * Auto-generates Table of Contents from `<h1>`/`<h2>` headings if the manifest lacks one.
    * Pre-extracts cover images into the thumbnail cache.
    * Repacks pristine archive.
* **Folder-Watch Auto-Import ("Drop Folder")**:
  * Background directory watcher monitoring a configured folder (e.g. `~/Downloads/Books`) to silently import and sanitize new files into the library.
* **Library-Wide Deep Content Search (Tantivy FTS)**:
  * Blazing fast offline full-text search engine (powered by Rust's `tantivy`) indexing the complete text of all books.
  * Instant (10–20ms) queries across tens of thousands of books for character names, quotes, or themes.
* **In-Book Text Search (`Ctrl+F`)**:
  * Live in-book search with highlighted match positions, match counters, and rapid next/previous traversal.

---

## Module 4: Offline Library Management & Metadata Editors

* **Interactive Inline Metadata Editing (`book.rs`)**:
  * Seamless inline editing directly on the Book Details page: clicking Title, Author, or series wraps labels in a `gtk::Stack` overlapping with a `gtk::Entry` box to save instantly on Enter.
  * Tag FlowBox features a permanent `[ + ]` pill to open a mini inline entry to quickly append tags.
* **Dedicated Metadata Editor & Online Fetcher Hub**:
  * Full-screen editing route for detailed book metadata:
    * **Integrated Online Fetching**: Search and fetch accurate metadata and high-res cover art from OpenLibrary and Google Books.
    * **Edition & Cover Comparison**: Side-by-side comparison of fetched editions before applying.
    * **Comprehensive Field Editing**: Title, authors, series name, series index, publisher, publication date, language, ISBN, synopsis/description, and tags.
* **Author Pages**:
  * Dedicated author hub displaying author biographies, personal notes, and all associated books grouped by series and release date.
* **Series Management ("Cover Stacks")**:
  * Multi-book series visually collapse into stacked cover cards (showing Book 1 with a subtle stacked-paper effect behind it) in the library grid to prevent clutter.
  * Clicking a stack expands all books in sequential reading order.
* **Custom & Smart Shelves**:
  * **Manual Shelves**: Curated collections and reading playlists.
  * **Smart Shelves**: Dynamic playlists generated from live search queries (e.g., `tag:Sci-Fi status:Unread rating:>4`) pinned to the sidebar.
* **Reading Statuses**:
  * Built-in primary reading states for all books: **Currently Reading**, **Want to Read**, **Finished**, and **Abandoned / Dropped**.
* **Personal Star Ratings & Book Reviews**:
  * 5-star rating widget and private notes/review markdown editor on the Book Details page.
* **Reading Streaks & Offline Reading Stats**:
  * Dashboard statistics tracking total books finished, weekly reading hours, and daily reading streaks.
* **Bulk Metadata & Tag Editing**:
  * Multi-select books to batch-assign tags, authors, shelves, or reading statuses in a single click.
* **Duplicate Book Finder**:
  * One-click tool to identify duplicate files across the catalog by hash or Title/Author matching for easy library hygiene.

---

## Module 5: Annotations, Vocabulary & Data Export

* **Vocabulary / Saved Words Hub**:
  * Dedicated hub for reviewed and saved dictionary lookups with pronunciation, definitions, parts of speech, and reading context.
* **Saved Quotes Hub**:
  * Centralized collection page for managing, categorizing, and reviewing saved book quotes.
* **Markdown Data Export**:
  * One-click export of reading progress, highlights, notes, and quotes to a structured Markdown document (`~/Kalam-Export.md`) for note-taking apps like Obsidian.
* **Multi-Library Switcher**:
  * Seamless switching between isolated library directories (e.g., Personal, Work, Archive) with independent catalogs.

---

## Module 6: Performance Budgets & Final UI System (Part 1 Closure)

* **Performance Bottleneck Fixes**:
  * Resolve opening latency on floating book cards and detail views to ensure instant rendering.
* **Material 3 Design System (Stitch MCP)**:
  * Systematic visual overhaul using Stitch MCP tokens and structured layouts across all views (Home, Library, Details, Reader, Settings, Author Hub) once all core offline features are verified.
