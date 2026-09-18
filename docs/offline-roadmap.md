# Kalam Offline Master Plan (Part 1)

This document is the single source of truth for **Part 1** of Kalam's development. 

Part 1 focuses entirely on building a **blazing fast, fully capable, robust offline ebook reader, editor, and library management system**. All online components (web plugins, online scrapers, MangaDex/AO3/FicHub clients) are strictly deferred to **Part 2**. The final UI design overhaul is scheduled at the very end of Part 1 once all offline features and performance foundations are complete.

---

## Module 1: The Reading Engines & EPUB Editor

### 1. EPUB & Reflowable Text Engine (Kalam Engine)
* **Core Reading Experience**:
  * Crisp typography with user-selectable fonts, font sizes, line heights, margins, and themes (Light, Sepia, Dark, OLED/Ink).
  * Dual-page view in widescreen mode and continuous vertical scrolling option.
  * Table of Contents (TOC) hierarchy navigation, reading progress tracking, and bottom-right reading location percentage pill.
  * Ribbon bookmark button in top-right header with bookmark management panel.
  * **"Jump Back" History (Breadcrumbs)**: If jumping to a footnote, Table of Contents, or search match, a floating button lets you instantly return to your previous reading position.
  * **Auto-Hiding Mouse Cursor**: Pointer hides after 2 seconds of inactivity during reading so it never blocks words.
  * **Custom Keybindings & Mouse Wheel Tuning**: Fully configurable shortcut keys and adjustable mouse wheel scroll step sensitivity.
* **Text Selection & Tooling**:
  * Fluid mouse/touch selection with custom teardrop drag handles.
  * Double-click to select word; triple-click to select paragraph.
  * Action Toolbar (Highlight with multi-color palette + Underline style, Quote, Dictionary lookup, Copy).
* **Non-Destructive Inline EPUB Editing**:
  * Fix typos and formatting errors directly inside books without altering or corrupting the original `.epub` file on disk.
  * Edits are stored as XPath replacement rules in the database and sidecar `kalam.json`.
* **Proofreading Edit Mode**:
  * Dedicated pencil toggle in reader chrome allowing one-click paragraph editing inline.
* **Quote-Anchored Locators & Robust CFI**:
  * Locators dynamically re-anchor to the exact text snippet regardless of reflow, font resizing, or window dimension changes.
  * Clicking an annotation, quote, or bookmark in the sidebar jumps directly to the exact highlighted phrase.
* **Footnotes & Cross-References**:
  * Clicking superscript links like `[1]` opens an instant footnote popover or jumps to the reference note with a quick back navigation trigger.

### 2. PDF Engine (Zathura-Style Smart Engine)
* **High-Performance Rendering**:
  * Crisp PDF rendering backed by Google's PDFium engine.
* **Smart Margin Crop**:
  * Automatic ink-boundary detection: detects the actual text and illustration bounding box on each page and crops out empty whitespace margins for maximum screen usage.
* **Text Reflow Mode**:
  * Heuristic text layer extraction converting text-heavy PDFs into reflowable text with customizable fonts, themes, and font sizes.
* **PDF Navigation & Bookmarks**:
  * PDF outline / Table of Contents tree, page jump box, and page bookmarking.

### 3. Comics & Manga Engine (CBZ/CBR)
* **Reading Modes**:
  * Single page, Left-to-Right (Western), Right-to-Left (Manga), and Webtoon continuous vertical scroll mode.
* **Double-Page Spreads**:
  * Automatic dual-page spread detection in landscape/widescreen mode.
* **Smart Page Splitting & Stitching**:
  * Automatic splitting of dual-page scans into single pages when reading in portrait/narrow window mode.
* **Image Tuning Filters**:
  * Instant contrast, sharpness, and brightness adjustments to clean up faded manga scans and enhance dark artwork.
* **Zero-Latency Preloading**:
  * Viewport memory caching with background preloading of adjacent pages (`page ± 2`) for instant page turns.
* **Comic Remaster Tool**:
  * Background worker to upscale and clean up low-resolution vintage scans using Lanczos3 scaling without blocking the UI.

---

## Module 2: Under-the-Hood Architecture & Performance (Yazi-Style)

* **Asynchronous `LibraryService` Layer**:
  * UI components never run blocking SQLite queries on the main thread.
  * Requests are routed through an async `LibraryService` actor pattern, ensuring 60 FPS UI responsiveness.
* **Task Manager (`tasks.rs`)**:
  * Centralized queue for long-running jobs (importing books, batch metadata fetching, full-text index rebuilding, comic remastering) with progress bars and cancellation support.
* **Smart Preloaders & Memory Safety**:
  * Preloading next chapter in the background while the user reads.
  * Async cover texture generation and memory-bounded thumbnail caching.
* **Bubble Memory & Floating Window Host**:
  * Lightweight overlay host allowing book cards, quick notes, and dictionary popups to float above active views without reloading the page or leaking memory.

---

## Module 3: Content Sanitizer & Deep Content Search

* **EPUB Ingestion Sanitizer**:
  * Automatic background cleaning of imported EPUBs:
    * Strips toxic hardcoded CSS (e.g. forced 8px fonts, fixed margins).
    * Repairs broken XML syntax.
    * Auto-generates Table of Contents from `<h1>`/`<h2>` headings if the manifest lacks one.
    * Pre-extracts cover images into the thumbnail cache.
* **Library-Wide Deep Content Search (Tantivy FTS)**:
  * Blazing fast offline full-text search engine indexing the complete text of all books.
  * Instant (10–20ms) queries across tens of thousands of books for character names, quotes, or themes.
* **In-Book Text Search (`Ctrl+F`)**:
  * Live in-book search with highlighted match positions, match counters, and rapid next/previous traversal.

---

## Module 4: Offline Library Management & Metadata Editors

* **Inline Metadata Editing**:
  * Seamless inline editing directly on the Book Details page: clicking a title, author name, or tag switches it to an inline entry and immediately saves changes.
* **Dedicated Metadata Editor**:
  * Full-screen editing route for detailed book metadata: series index, publisher, publication date, custom cover assignment, and tag management.
* **Author Pages**:
  * Dedicated author hub displaying author biographies, personal notes, and all associated books grouped by series and release date.
* **Series Management ("Cover Stacks")**:
  * Multi-book series visually collapse into stacked cover cards in the library grid to prevent clutter.
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
