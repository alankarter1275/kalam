# Kalam and Chapbook workspace guide

This guide describes the current workspace. The source tree, manifests and
tests are authoritative if a detail changes.

## Repository layout

The root package is the Kalam GTK application. The same Cargo workspace also
contains the local Chapbook engine crates, the `kalam-reader` GTK widget and
two tools. `Cargo.toml` lists the current members.

Useful paths:

- `src/app.rs` — application model, routing and app-level actions.
- `src/pages/reader/` — EPUB reader page and its `kalam-reader` integration.
- `src/pages/pdf_reader.rs`, `src/pdf.rs` — PDF UI and MuPDF adapter.
- `src/pages/comics_reader/`, `src/comics.rs` — CBZ/CBR reader and archive
  handling.
- `src/db.rs`, `src/db/`, `src/service.rs` — catalog, migrations and page
  snapshots.
- `crates/kalam-reader/` — GTK EPUB reader widget.
- `crates/chapbook-*` — local EPUB, layout, reader and renderer crates.
- `docs/WORKING.md`, `ARCH.md`, `ROADMAP.md` — current repository rules,
  architecture and plan.

`docs/kalam/PLAN.md`, `RESEARCH.md`, `RESTRICTIONS.md` and `UPSTREAM.md` are
historical engine-planning and provenance records. Use the current code and
[`INTEGRATION.md`](INTEGRATION.md) for the app's reader behavior.

## Current reader integration

Kalam embeds `kalam_reader::ReaderView` from `src/pages/reader/engine.rs`.
The page connects reader events to application messages and owns the surrounding
chrome, database writes, dictionary cards, highlights and bookmarks.

- `ReaderView::search()` searches across the book, not only the current
  chapter.
- Ordinary dictionary lookup is initiated by selecting text or choosing the
  Dictionary action. `connect_word_tap()` is used for saved Word Memory terms,
  not generic word lookup.
- Reading progress is stored as chapter index plus a fraction in SQLite.
- Highlight positions use the reader's locator data when available. Older
  annotation rows can remain visible in the annotations list without being
  placeable on the current page.
- EPUB content editing and its pending-edit review workflow are removed. Old
  patch records remain only as compatibility data in the database and sidecar;
  current code does not apply or render them.

See [`INTEGRATION.md`](INTEGRATION.md) for callback and persistence details.

## Content sources

`src/sources/` contains the `Source` interface and an empty manager. No source
implementation is registered, so remote browsing and downloads are not
available. The source-manager comments and future design must agree with the
current Part 2 decision in `ROADMAP.md`; do not treat the TOML scraper sample as
a working source.

## Build and checks

Use the repository-root commands:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo build --workspace
```

The CI workflow is `.github/workflows/ci.yml`; its actual checks and smoke-test
limits are described in [`docs/ci/README.md`](../ci/README.md). Local build
availability depends on the machine's Rust toolchain and GTK dependencies.

## EPUB content editing

The former EPUB editor has been removed from the reader, library and app
navigation. Do not implement or mock up an editor unless the owner reopens the
work. If reopened, edit HTML/XHTML and CSS directly; do not add Markdown
support or Markdown conversion. Book metadata editing and annotations are
separate features and remain in scope.
