# Kalam architecture

This document describes the architecture in the current source tree. Where a
prose rule and the code disagree, verify the behavior and update the rule or
code in the same change. The application source and tests are authoritative.

## Application layers

```text
GTK / Relm4 application (`src/app.rs`, `src/pages/`)
        │ messages, owned snapshots
        ▼
Library service (`src/service.rs`) ── background tasks (`src/tasks.rs`)
        │ catalog queries and updates       │ filesystem / parsing / OCR
        ▼                                    ▼
SQLite catalog (`src/db.rs`, `src/db/`)   EPUB, PDF, image and metadata code
```

- `AppModel` owns navigation and connects page events to application
  operations. Page modules contain GTK widgets, page models and view logic.
- `LibraryService` groups catalog reads into owned, screen-sized snapshots.
  Pages should use those snapshots rather than holding `Arc<Catalog>`; the
  current code has exceptions, tracked by a regression ratchet.
- `Catalog` owns SQLite access and migrations. The database is the source of
  truth for books, metadata, annotations, bookmarks, Word Memory entries and
  reading progress.
- `tasks.rs` runs blocking or potentially expensive work away from the GTK
  thread and delivers owned results back as application messages. Workers must
  not manipulate GTK objects.

The separation is an active direction, not a claim that every page operation
is already asynchronous. Consult the page and service implementation before
changing the threading model.

## Reading engines

- Reflowable EPUB reading uses the `kalam-reader` GTK widget, backed by the
  local Chapbook crates in the workspace. Fixed-layout EPUBs are not supported.
- The widget exposes whole-book `search()` and reader interaction callbacks.
  Kalam uses explicit selection and Dictionary actions for ordinary lookup;
  tap callbacks are connected to Word Memory terms.
- PDF reading is implemented in `src/pdf.rs` and `src/pages/pdf_reader.rs`
  using MuPDF. OCR support lives in `src/ocr.rs` and `src/pdf_ocr.rs`.
- CBZ/CBR import, ordering and page display are implemented in the comics
  modules and `src/pages/comics_reader/`.

The workspace's `crates/chapbook-*` libraries and `crates/kalam-reader` are
local workspace members; see the root `Cargo.toml` for the current member list.

## EPUB content editing

The EPUB content editor was removed and deferred on 2026-10-21. The owner later
explicitly reopened the feature and directed implementation, so it is built and
reachable: `NavItem::Editor` in the navigation rail (decision 102),
`Route::EditorWorkspace`, `src/pages/epub_editor.rs`, `editor_picker.rs`,
`epub_structure.rs`, `epub_tools.rs`, `text_diff.rs`, `font_subset.rs`,
`db/editor.rs`, and a `sourceview5` dependency. Content scope is HTML/XHTML and
CSS edited directly; Markdown support and conversion remain out of scope.
**Further editor work is deferred** until the UI redesign is complete; redesign
work must not remove or stub the editor out. This section was not updated when
the feature was reopened, which is why it contradicted the shipped code.
Book metadata editing and annotations are unrelated and remain available.
Legacy patch data is retained only for compatibility with older databases and
sidecars; the app does not create, apply, render or review it.

## Library storage

- The SQLite catalog is authoritative. The default library is under
  `~/.local/share/kalam`; users can configure other library folders.
- Book folders use readable author/title names with a short identifier.
  Database ids identify books; the folder name may change when metadata does.
- `kalam.json` is a per-book backup snapshot for metadata, tags, annotations,
  reading position and edit patches. It is refreshed after metadata,
  annotation, bookmark and patch changes, but not on every reader progress
  update.
- Covers, author images, series covers and rebuildable caches are separate
  library resources. See `src/paths.rs`, `src/folders.rs` and `src/sidecar.rs`
  for exact path and sidecar behavior.

## Current boundaries

- Keep GTK objects on the GTK thread. Use tasks for work that may block it.
- Keep SQL in the catalog layer and prefer `LibraryService` snapshots in
  pages. `tests/guardrails.rs` tracks current exceptions; it is a ratchet, not
  a claim that the boundary is complete.
- Add tests when changing catalog migrations, reader event handling, sidecar
  contents, EPUB patches or file operations.
- There are no registered implementations of the content-source interface in
  `src/sources/`; the interface and manager are scaffolding, not working online
  browsing or download support.

See [`docs/WORKING.md`](docs/WORKING.md) for repository rules,
[`ROADMAP.md`](ROADMAP.md) for current project status, and
[`docs/ci/README.md`](docs/ci/README.md) for CI coverage.
