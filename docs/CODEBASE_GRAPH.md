# Codebase Graph Index (Kalam / calibre-alt)

> **AI Agent Context Map**: Use this compact node index to locate components, database modules, GTK views, and EPUB parsers without reading full documentation files or running speculative file searches.

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
