# Kalam

Kalam is a personal ebook library and reader for Linux. It is built with Rust,
GTK4 and Relm4.

## Features

- Import and organize EPUB, PDF and CBZ/CBR books.
- Read reflowable EPUBs with the native `kalam-reader` widget and PDFs with
  MuPDF. Fixed-layout EPUBs are not supported.
- Search across EPUB and PDF books while reading. Search the library by book
  text, title, author and other fields.
- Save highlights, quotes, bookmarks and reading progress.
- Look up selected words and phrases in offline dictionaries. Tapping a saved
  Word Memory term opens its word card; ordinary words are looked up by
  selecting them or using the reader's Dictionary action.
- Manage tags, shelves, reading lists, ratings, metadata and library folders.
- Save named whole-panel reading presets for comics and apply them like the
  equivalent manual Reading and Image changes.
- Use Open Library and Google Books when fetching book metadata, and
  MyAnimeList or Comic Vine when fetching comic-series metadata. The comic
  providers need credentials configured in Settings. Reading and library
  features do not require those services.

EPUB content editing is currently removed. The reader, library and navigation
no longer expose the editor or pending-edit review workflow. If content editing
is revisited later, its scope is XHTML/HTML and CSS edited directly; Markdown
support and Markdown conversion are out of scope.

Online browsing and downloads have placeholder UI and queue code, but there is
no registered content source. Those routes do not provide working source
browsing or downloads.

## Build and run

Kalam requires Linux, Rust, GTK 4.16 or newer, libadwaita 0.9, Relm4 0.11,
the native libraries needed by MuPDF and OCR.

On Arch Linux:

```bash
sudo pacman -S --needed rust gtk4 libadwaita base-devel pkgconf
```

Build and run the app:

```bash
cargo run
```

Run checks for every workspace member:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo build --workspace
```

The workspace has 11 members. A bare Cargo command selects the root app package;
use `--workspace` to include the Chapbook crates and tools. The CI workflow is
in [`.github/workflows/ci.yml`](.github/workflows/ci.yml). Its jobs and smoke
reports are described in [`docs/ci/README.md`](docs/ci/README.md).

## Where files live

The default library is `~/.local/share/kalam`. User settings and the list of
libraries are stored under `~/.config/kalam`.

```text
<library folder>/
  catalog.db
  library/Author - Title <short-id>/
    book.epub
    cover.jpg
    kalam.json
  cache/
  covers/
  authors/
  series-covers/

~/.local/share/kalam/dictionaries/
```

Book folders use a readable author/title name and a short id. The database id,
not the folder name, identifies each book. Kalam renames a folder when its
book's title or author changes.

`kalam.json` is a per-book backup snapshot containing metadata, tags, reading
position, annotations and any legacy EPUB edit history from older versions.
Reader progress is saved to the database; page turns do not rewrite the sidecar.
Metadata, annotation and bookmark changes refresh it. The current app no longer
creates or applies EPUB edit patches.

A library folder can be copied to another machine and opened from **Settings →
Storage → Libraries**. The cache can be rebuilt and does not need to be copied.

## Reader interactions

Select text and choose **Dictionary**, or use the `D` action, to look up a word
or phrase. Ordinary taps do not open dictionary lookup. Word Memory terms are
visually marked and can be tapped to open their saved word card.

`Ctrl+F` opens whole-book search in the EPUB reader. The dictionary card's
magnifier highlights occurrences of its word in the current chapter.

## Workspace map

```text
src/                         Kalam app
  pages/reader/              EPUB reader and reader UI
  pages/pdf_reader.rs        PDF reader UI
  db.rs, db/                 SQLite catalog and migrations
  service.rs                 screen data snapshots
  tasks.rs                   background tasks
  sidecar.rs                 per-book JSON snapshots
  epub_sanitizer.rs          general EPUB import sanitization
  paths.rs, folders.rs       library and readable book paths

crates/kalam-reader/         GTK widget embedded by Kalam
crates/chapbook-reader/      reading session and engine
crates/chapbook-epub/         EPUB container support
crates/chapbook-layout/       EPUB layout and pagination
crates/chapbook-core/         shared reading types and locators
crates/chapbook-paint/        display lists and image store
crates/chapbook-render-tinyskia/ CPU renderer
crates/chapbook-viewer-gtk/   reference GTK viewer
 tools/                       Chapbook CLI and reader demo
```

The app depends on local workspace crates. The root package repository is
[`alankarter1275/calibre-alt`](https://github.com/alankarter1275/calibre-alt);
the Chapbook workspace originated at
[`ophymx/chapbook`](https://github.com/ophymx/chapbook).

## Project documents

- [`ROADMAP.md`](ROADMAP.md) — current project plan and status.
- [`ARCH.md`](ARCH.md) — current architecture rules.
- [`UPSTREAM.md`](UPSTREAM.md) — Chapbook engine provenance.
- [`docs/WORKING.md`](docs/WORKING.md) — repository rules and guardrails.
- [`Issues.md`](Issues.md) — mirror of the tracked Linear issues.
- [`docs/ci/README.md`](docs/ci/README.md) — CI workflow and reports.
- [`docs/STABILITY.md`](docs/STABILITY.md) — stability tiers for local Chapbook crates.
- [`docs/pitfalls.md`](docs/pitfalls.md) — engineering lessons.

## Licensing

The app package does not declare a project-wide license. Bundled dictionary
resources have their own license and attribution notices in
[`resources/dictionaries/`](resources/dictionaries/). Keep those notices with
the resources when redistributing them.
