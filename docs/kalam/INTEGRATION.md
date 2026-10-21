# Kalam reader integration

Kalam embeds `kalam-reader::ReaderView` for reflowable EPUB reading. Fixed-layout
EPUBs are rejected by the current reader. The widget handles layout, rendering,
pagination, selection and reader-level events. The app owns navigation chrome,
catalog persistence, dictionary UI, annotations and bookmarks. EPUB content
editing is currently removed.

## Main integration points

| Concern | Current implementation |
|---|---|
| Open an EPUB | `src/pages/reader/engine.rs::open_engine` builds preferences and `ReaderOptions`, then calls `ReaderView::open`. |
| Connect events | `src/pages/reader/engine.rs::wire` forwards reader callbacks as `ReaderMsg` values. |
| Host widget | `src/pages/reader/mod.rs` embeds `view.widget()` in the reader overlay and owns app chrome. |
| Close a book | The reader page calls `ReaderView::close()` when replacing or shutting down the reader view. |
| Search | `ReaderView::search()` searches across the book; the reader page presents results from `Ctrl+F`. |
| TOC | `ReaderView::toc()` returns nested publisher entries; the app renders them depth-first and falls back to spine titles only if no entry has a usable spine target. Sidebar clicks navigate to the entry's spine index with `goto_chapter`; the app does not call `goto_toc()`. |
| Engine API | [`crates/kalam-reader/README.md`](../../crates/kalam-reader/README.md) describes the public widget surface. |

`ReaderView::open` constructs GTK objects and currently runs synchronously from
the reader-page initialization path. Do not move it to a worker without
redesigning the GTK construction boundary. The widget's chapter loading and
rendering use its own reader machinery.

## Reader events and host behavior

| Widget event or API | Kalam behavior |
|---|---|
| `connect_position` | Stores the current chapter index and fraction in the SQLite reading-progress row. |
| `connect_selection` | Drives the selection chip and actions. Ordinary dictionary lookup starts from an explicit Dictionary action, not a generic word tap. |
| `connect_selection_moved` | Keeps the host's selection-action anchor current as layout and scrolling move the selected text. |
| `connect_word_tap` / `connect_word_hover` | Handles terms previously supplied through `set_word_memory`; these are Word Memory interactions. |
| `connect_highlight_tap` | Opens the app's actions for an existing annotation. |
| `connect_note` | Shows an in-place footnote/note card and lets the reader follow the link from that card. |
| `connect_external_link` | Launches supported HTTP and HTTPS links through GTK. |
| `connect_image_tap` | Opens the image lightbox. |
| `search` | Provides whole-book find. |
| `goto_chapter` | The app's table-of-contents sidebar navigates by spine index; `goto_toc` is available in the widget API but is not used by this app path. |

Callbacks run on the GTK thread. They forward messages or make short UI
updates; disk, database and archive operations should not be added to a reader
callback.

## Progress and annotations

Reading position is stored as `(chapter_index, fraction)` in SQLite. The engine
can also produce a layered text locator for durable position restoration, but
page-turn progress writes do not refresh `kalam.json` on every turn. The
sidecar is a backup snapshot refreshed by supported metadata, annotation and
bookmark changes. Legacy edit-patch records are preserved only for compatibility
with data written by older versions; they are not applied or reviewed.

Kalam stores annotations in its database. `LibraryService::reader(book_id)`
loads every annotation for the book into the reader snapshot. New engine
highlights use locator ranges serialized in the annotation row's `cfi` field;
the widget receives the host row id when painting them. Rows without a usable
current locator remain available in annotation lists, but the reader cannot
paint them at a text position.

## Bubbles and split reading

The bubble strip can retain lightweight entries for several books. It mounts one
primary reader and, when split mode is active, one second reader side by side.
Only those two reader widgets are active at once.

## EPUB content editing

The reader's inline-edit controls, Deep Editor/source entry point, standalone
editor, picker/navigation entry and pending-edit review UI have been removed.
There is no in-app route to edit EPUB content. If editing is reopened later, the
scope is HTML/XHTML and CSS edited directly; Markdown support or conversion is
out of scope.

## Scope boundary

The reader widget handles EPUBs only. Kalam's PDF path uses MuPDF, and its
CBZ/CBR path uses the comics reader modules. Remote source browsing and
platform downloads have no registered source implementation; see Part 2 of
[`ROADMAP.md`](../../ROADMAP.md).
