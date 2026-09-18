# Codebase Graph Index (Kalam / calibre-alt)

> ## ⚠️ Stale and machine-generated — do not trust, do not hand-edit
>
> This file was produced by [Graft](https://github.com/trailhq/Graft), not
> written by hand, and it describes the project **as it was before the reading
> engine was replaced**. In particular it still says the reader is WebKitGTK.
> It is not: the reader is now `crates/kalam-reader` over the vendored
> `chapbook-*` crates, and nothing in `src/` uses WebKit any more. It also
> predates the `crates/` and `tools/` trees entirely, which is now roughly a
> third of the repository.
>
> The absolute `file:///home/...` links below are Graft output too, and were
> never deliberately written. Hand-fixing them is pointless: the next
> `graft build` overwrites the file and they come back.
>
> **Regenerate rather than repair:** `npx @nanonets/graft` and then replace
> this file. Until then it is kept here for history only. Nothing else in the
> repository links to it.
>
> For an accurate map, read [`README.md`](../../README.md) ("Project layout"),
> [`docs/ARCHITECTURE.md`](../ARCHITECTURE.md) and
> [`docs/offline-roadmap.md`](../offline-roadmap.md).

---

> **AI Agent Executive Summary & Context Map**: Read this index first to understand Kalam's vision, current trajectory, and component architecture without reading full 300KB documentation files or running speculative file searches.

---

## 🎯 Executive Vision & Project Status
* **What Kalam Is**: A lightweight, personal, all-in-one ebook manager & reader for Linux built with **Rust**, **GTK4**, and **Relm4**. Designed to stay fast and responsive on modest hardware.
* **Core Capabilities**:
  * SQLite catalog (`~/.local/share/kalam/catalog.db`)
  * WebKitGTK EPUB reader (themes, font scaling, chapter scroll, TOC, highlight chips)
  * Offline dictionary engine (StarDict, TSV, CMU IPA pronunciation, POS sense dividers)
  * Library Hub (reading lists, smart rules, saved quotes, vocabulary Anki export)
* **Current Trajectory**: Dictionary track shipped (Phases 8–10), reader improvements complete, background preloading active. Full roadmap details in [`ROADMAP.md`](file:///home/kunalsingh/Projects/calibre-alt/ROADMAP.md).

---

## 1. Core Architecture & Entry Points
* **Root Application Shell**: [`src/main.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/main.rs), [`src/app.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/app.rs)
  * Initializes Relm4 application, GTK4 window, sidebar navigation stack, and SQLite database connection pool.
* **Data Models & Types**: [`src/models.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/models.rs)
  * Defines core structs: `Book`, `Author`, `Shelf`, `Tag`, `Highlight`, `Quote`, `SavedWord`, `ReadingProgress`.

---

## 2. Database Layer (`src/db/`)
* **SQLite Connection & Schema**: [`src/db.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/db.rs) (Database located at `~/.local/share/kalam/catalog.db`)
* **Annotations & Highlights**: [`src/db/annotations.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/db/annotations.rs)
* **Book & Series Metadata**: [`src/db/metadata.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/db/metadata.rs), [`src/db/series.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/db/series.rs)
* **Dictionaries & Vocabulary**: [`src/db/dictionaries.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/db/dictionaries.rs), [`src/db/pronunciation.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/db/pronunciation.rs), [`src/db/lookup_history.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/db/lookup_history.rs)
* **Shelves & Smart Rules**: [`src/db/shelves.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/db/shelves.rs)
* **Reading Stats & History**: [`src/db/stats.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/db/stats.rs), [`src/db/history.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/db/history.rs)

---

## 3. EPUB Parsing & Offline Dictionaries
* **EPUB Engine**: [`src/epub.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/epub.rs), [`src/epub_book.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/epub_book.rs), [`src/epub_writer.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/epub_writer.rs)
* **Offline Dictionary Core**: [`src/dict.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/dict.rs)
  * CMU Pronunciation IPA, StarDict indexer, TSV packs, and SQLite dictionary lookups.

---

## 4. UI Views & Pages (`src/pages/`)
* **Home Page**: [`src/pages/home.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/pages/home.rs)
* **All Books Grid**: [`src/pages/all_books.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/pages/all_books.rs)
* **Library & Shelves**: [`src/pages/library.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/pages/library.rs)
* **Reader Component**: [`src/pages/reader/chapter.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/pages/reader/chapter.rs) (WebKitGTK integration, chapter scrolling, TOC, highlighting, tap-to-lookup).
* **Analytics**: [`src/pages/analytics.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/pages/analytics.rs)
* **Lookup History**: [`src/pages/lookup_history.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/pages/lookup_history.rs)
* **Metadata Editor**: [`src/pages/metadata_editor.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/pages/metadata_editor.rs)

---

## 5. External API Metadata Providers
* **OpenLibrary**: [`src/metadata/openlibrary.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/metadata/openlibrary.rs)
* **Google Books**: [`src/metadata/google_books.rs`](file:///home/kunalsingh/Projects/calibre-alt/src/metadata/google_books.rs)
