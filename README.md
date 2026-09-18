# Kalam

**Kalam** is a lightweight, personal, all-in-one ebook manager and reader for Linux.
Built with **Rust**, **GTK4**, and **Relm4**. Designed to stay fast on modest hardware.

> Reader-improvements track complete — merged dictionary store, popup redesign,
> POS + likely-sense hint, offline IPA pronunciation, find in chapter,
> vocabulary review + CSV/Anki export. Dictionary track Phases 8–10
> shipped (POS dividers, priority reorder, lookup history).
>
> **Tap-a-word-to-open-dictionary is not a feature and never will be.** It
> shipped once under WebKit, and the owner removed it deliberately. Look a word
> up by selecting it. Do not reintroduce a tap-to-look-up.

> **For AI agents / new chats — read this first.**
> **Current plan: [`docs/offline-roadmap.md`](./docs/offline-roadmap.md)** —
> the single source of truth for **Part 1** (everything offline: engines,
> editor, library, search, performance). Online sources, scrapers and plugins
> are all **Part 2** and deliberately unbuilt.
> Older plan and history: [`ROADMAP.md`](./ROADMAP.md) (its **"Read this
> first"** block and **"Current trajectory"** section) — large and partly
> stale, treat as a log.
> Design decisions and the Part 2 discussion:
> [`docs/conversation.md`](./docs/conversation.md).
> **Known pitfalls: [`docs/pitfalls.md`](./docs/pitfalls.md)** — mistakes already
> made here and how they were fixed; read it before writing code, and add to it
> when you get something wrong.
> **Keep all of these updated in the same commit as your code** — a change that
> leaves the plan stale is not done.


## Working agreement

- **CI (GitHub Actions)** compiles, clippys, runs the unit tests and builds
  debug+release on every push — you don’t need to build between commits.
- **Backend review pass done** — full line-by-line sweep of the data layer:
  importers hardened (read-only SQLite packs, identifier quoting, rollback,
  catalog.db self-import guard), one latent bug fixed (reading-list column
  offsets), 9 new unit tests. No other defects.
- **CI runs the test suite on every push** (`cargo test --workspace
  --all-targets`, live in `.github/workflows/ci.yml`). On failure the
  diagnostics are published to `ci-logs/test-latest.txt`.
  - `--workspace` is load-bearing and was missing until 2026-09-18. This
    workspace's root is itself a package (`kalam`), so with no
    `default-members` key Cargo defaults to that one package: a bare
    `cargo test` ran the app's ~374 unit tests and silently skipped the ~420
    under `crates/*/tests` and `tools/*/tests`. Same for clippy. If you add a
    member, `--workspace` already covers it.
  - Counts drift; check with
    `grep -rc '#\[test\]' src crates tools --include='*.rs'` rather than
    trusting a number written here.
  - Note that `ci-logs/` is a snapshot, not live: the committed logs predate
    the engine becoming a path member and still reference it as a git
    dependency.
- Workflow changes are made by the agent directly in
  `.github/workflows/ci.yml` — see `docs/ci/README.md`.
- **Your Arch machine** is only needed at **phase boundaries** (smoke-test + design feedback).
- Full plan: [`ROADMAP.md`](./ROADMAP.md) · architecture notes: [`ARCH.md`](./ARCH.md) ·
  design decisions: [`docs/conversation.md`](./docs/conversation.md) ·
  known pitfalls: [`docs/pitfalls.md`](./docs/pitfalls.md) ·
  stability policy: [`docs/STABILITY.md`](./docs/STABILITY.md) ·
  how to smoke-test the A0 changes and read the timing output:
  [`docs/testing-a0.md`](./docs/archive/testing-a0.md) ·
  **A0 steps 4+5 (background tasks + preloaders):
  [`docs/testing-a0-step5.md`](./docs/archive/testing-a0-step5.md)**

## What works now

- Slim sidebar shell + cover-card library grid
- **SQLite catalog** at `~/.local/share/kalam/catalog.db`
- **Import EPUB** (Home → “+ Add books”, or My Library → All books → “+ Import EPUB”)
- **All books** grid (search, sort, cover cards) — from Home → “All books” or the
  My Library quick links
- **Reading list**, **Tags** and **Analytics** — from the My Library quick links
- **EPUB reader** (`crates/kalam-reader`, a native GTK4 widget — no WebKit, no
  JavaScript): paged and continuous-scroll reading, TOC, four themes, font
  size, line height, column width, progress restore
- **Highlights & quotes**: select text → floating chip (yellow/green/blue/pink/orange), save quote (❝), copy
- **Dictionary**: offline packs (StarDict .ifo/.idx/.dict[.dz], SQLite .db, TSV), lookup via chip, tap, or `D` shortcut, a popup with numbered senses + POS, synonym/antonym chips, idiom cards, bookmark & copy, offline IPA pronunciation (`bank` → `/ˈbæŋk/`) from the bundled CMU Pronouncing Dictionary, keyboard support (↑/↓ focus a sense, Enter saves it), and **Find in chapter**
- **Annotations list**: reader bottom pill ✎ shows highlights/quotes for current book, jump & delete
- **Library hub**: My Library → Saved quotes (real data) → export to Markdown (`~/Quotes.md`), Saved words (real data) → vocabulary review (mark known / to review, All/To review/Known filter) → export CSV (`~/SavedWords.csv`) or Anki TSV (`~/SavedWords-Anki.txt`)
- **Settings**: dictionary packs import (+ Import dictionary), list & remove, data paths
- **Shelves**: manual collections + **smart shelves** with a rule builder
  (tag / author / series / format / progress / title / added · is · is not ·
  contains · date windows · All-or-Any), live match count, 2-column grid
- **Reading list**: ordered TBR with ↑/↓ reorder and a bulk picker
- **History**: every open / finish / import, grouped by day and filterable
- **Reading time**: sessions recorded per reader visit (clamped at 6h)
- **Tags**: usage-weighted tag cloud → per-tag book grid
- **Analytics**: counts, time read (7d/30d/all), streaks, 14-day bar chart,
  books-added-per-month, most read, top tags & authors
- **Book page**: add to reading list, Mark finished / unread, shelf checklist,
  half-star rating, **Edit metadata**
- **Metadata editor**: title/authors/series/tags/description, replace cover
  from disk, and **Open Library** search that stages results for review
- **Ratings & goals**: half-star ratings, yearly reading goal, daily streak
- Float detail panel (Suwayomi-style); Read opens the viewer
- AO3 / comics / downloads still placeholders

## Phase overview

| Phase | Feature |
|-------|---------|
| P0 | Shell + nav ✅ |
| P1 | SQLite library, EPUB import, covers ✅ |
| P2 | EPUB reader (incl. P2.1 chrome restyle) ✅ |
| P3 | Highlights, quotes, offline dictionary ✅ |
| P4 | Shelves engine, lists, history, tags, analytics ✅ |
| **P5** | **Metadata edit, cover replace, Open Library fetch** ✅ |
| Reader track | Annotation workflow + hybrid anchoring; dictionary overhaul (merged store, popup redesign, likely-sense hint, IPA pronunciation, find in chapter) + vocabulary review (known flag, CSV/Anki export) ✅ · Phases 8–10 shipped: POS grouping dividers, dictionary priority reorder UI, lookup history |
| Backend review | Full sweep of `db.rs` + `db/*`: importers hardened, reading-list column bug fixed, 9 new tests ✅ |
| A0 (architecture) | ✅ **done** except the plugin seam. Measured (`perf.rs` / `timing.rs`) · `LibraryService` seam · cover thumbnails · task manager · preloaders · **windowed book grid** (2,000 books: 502 MB → 247 MB, 434 ms → 12 ms) · perf budgets in CI that assert **query counts**, not milliseconds. The plugin-host seam is designed in `docs/archive/source-seam.md` and lands with its first implementation (Part 2) |
| **P6.5** | **Libraries** ✅ — choose the folder, keep several, switch between them (restarts), copy one to another machine and it opens. App settings stay shared; dictionaries are not duplicated per library. Every book folder keeps a `kalam.json` backup of its details, tags, highlights and reading position |
| P8 / P9 | Comics and manga — **next**. Image pager, then the same `Source` trait with MangaDex |
| P6 + P7 | Downloads hub **and** the fiction client, **built together** (combined 2026-09-04): a queue with nothing to download is a shell. Browsing client for AO3 / Royal Road / Literotica / FFN — browse, filter, author pages, read online, download, auto-update |
| P10–P12 | PDF, tools, Lua plugins — see ROADMAP |
| UI overhaul (P5.5) | Colour system, 13 themes, Settings v2, book page, series float ✅ · the remaining screens are **deliberately last** (moved 2026-09-04): every phase above adds screens, so restyling now means restyling again later |

## Switches

Every risky change ships behind one of these, so a problem can be turned off
without waiting for a fix. All are off-by-default in the sense that the app
does the right thing with none of them set.

| Variable | Effect |
|---|---|
| `KALAM_TIMING=1` | print cold-start / book-open / chapter-turn timings |
| `KALAM_NO_WINDOWED_GRID=1` | build every book card again, not just the visible ones |
| `KALAM_NO_PRELOAD=1` | decode covers synchronously, as before A0 step 5 |
| `KALAM_NO_CSS=1` | run with stock GTK styling — tells you whether a visual bug is ours |
| `KALAM_ROUTE=<page>` | open straight to a page (`all-books`, `settings`, `read-1`, …); used by the CI screenshots |

`KALAM_NO_WEBVIEW_POOL=1` used to be in this table. It is dead — nothing reads
it any more, because the WebKit view pool it controlled went with the reader
it belonged to. `KALAM_ROUTE` can still reach the hidden Part 2 pages
(`downloads`, `browse`), which is how CI gets at them now that the sidebar
does not list them.

## Where your files live

```
~/.config/kalam/
  libraries.json     which libraries exist and which is open
  prefs.json         app settings shared by every library

<library folder>/    default ~/.local/share/kalam
  catalog.db         books, shelves, highlights, reading progress
  library/<uuid>/
    book.epub
    cover.jpg
    kalam.json       backup copy of this book's details, tags and highlights
  cache/             thumbnails and unpacked EPUBs — safe to delete

~/.local/share/kalam/dictionaries/   shared by every library, never duplicated
```

A library folder is self-contained: copy it to another machine, point Kalam at
it in **Settings → Storage → Libraries**, and it opens. The `cache/` folder is
rebuilt as needed and does not need to travel.

## Requirements (Arch Linux)

```bash
sudo pacman -S --needed rust gtk4 libadwaita base-devel pkgconf
```

**GTK 4.16 or newer.** Kalam itself asks gtk4-rs for 4.12, but
`crates/kalam-reader` asks for 4.16 (for the `AccessibleText` interface and
its extents/offset geometry), Cargo unifies the two, and the build then
requires system GTK ≥ 4.16. This is also why CI pins `ubuntu-26.04` rather
than `-latest`.

WebKitGTK is **not** a dependency and has not been since the reader was
replaced by `kalam-reader`. If a build error mentions webkit, something is
stale.

## Build & run

```bash
cd kalam   # or your clone path
cargo run
```

Release build (what you’ll use day to day):

```bash
cargo run --release
```

`cargo build`/`cargo run` build just the app. To build, lint or test the whole
workspace — the reading engine crates and the two tools included — use
`make check` / `make test`, or pass `--workspace` yourself. See the note in
the [Working agreement](#working-agreement) for why that flag matters.

## Test online with GitHub Codespaces

If running Kalam on your own machine is a pain, you can test it in GitHub Codespaces.

### First time setup

1. Open this repo on GitHub.
2. Click **Code**.
3. Open the **Codespaces** tab.
4. Click **Create codespace** on the branch you want to test.
5. Wait for the setup to finish.

### Start Kalam in the browser

In the Codespaces terminal, run:

```bash
./scripts/run-kalam-codespace.sh
```

Then:

1. Open the **Ports** tab in Codespaces.
2. Find port **6080**.
3. Open it in the browser.
4. You should see a simple Linux desktop.
5. Kalam should open there.

### If you want to start it by hand

```bash
./scripts/codespaces-desktop.sh
source ~/.cache/kalam-codespace/env.sh
cargo run
```

### Notes

- The first start can take a few minutes.
- Port **6080** is the browser view for the app.
- If you only see a file list in the browser, run `./scripts/codespaces-desktop.sh` again and refresh the page.
- If the desktop opens but Kalam is not running yet, go back to the terminal and run `cargo run`.
- You can also use `cargo run --release` if you want the faster build.

## Click-through demo (P4)

1. **Shelves → + Smart shelf** → name it, add rules (e.g. `Tag is fantasy`
   **and** `Progress is Unread`) → watch the **live match count** → Create
2. Click the card → the shelf lists exactly those books
3. **Shelves → + Shelf** (manual) → open it → **+ Add books** → tick a few →
   expand **Manage shelf order** → reorder with ↑/↓ or remove
4. **Book page → Shelves…** → tick/untick manual shelves → chips update
5. **Book page → + Reading list** → **Library → Reading list** → reorder, Read
6. **Read** a book for a minute, leave → **Library → History** shows "Opened"
7. **Library → Analytics** → time read, streaks, 14-day chart, top tags
8. **Library → Tags** → click a tag → grid of books with that tag
9. **Book page → Mark finished** → drops off the reading list, logged in History
10. **Home** → counts strip, Continue row, Up next peek

## Project layout

The repository is **two projects in one workspace**: the application (`src/`)
and the reading engine it embeds (`crates/`, vendored from
[ophymx/chapbook](https://github.com/ophymx/chapbook) — see
[`docs/kalam/UPSTREAM.md`](./docs/kalam/UPSTREAM.md)). Roughly a third of the
code is the engine. Both are workspace members, which is why the `--workspace`
flag on `cargo test`/`cargo clippy` matters so much.

```text
src/                       THE APPLICATION (~51k lines)
  main.rs          entry, library check, catalog open, theme
  app.rs           shell, sidebar, routing, page cache
  db.rs            SQLite catalog + schema migrations
  db/              db.rs split: annotations, authors, dictionaries, history,
                   lookup_history, metadata, prefs, pronunciation, search,
                   series, shelves, stats, tags
  models.rs        routes, NavItem, books/shelves
  service.rs       LibraryService — pages ask, it answers (A0 step 2)
  tasks.rs         background tasks with progress + cancellation (A0 step 4)
  preload.rs       background cover/asset preloaders (A0 step 5)
  thumbs.rs        persistent cover thumbnails (A0 step 3)
  paths.rs         XDG paths — per-library vs shared (P6.5)
  libraries.rs     which library is open, the registry, global prefs (P6.5)
  sidecar.rs       kalam.json backup beside every book (P6.5)
  notify.rs        toast notifications + history
  perf.rs          query-count budgets (gate CI) + timing probes (manual)
  timing.rs        in-app timing harness (KALAM_TIMING=1)
  theme.rs         every colour — 13 dark themes
  style.rs         global CSS — shape only (spacing, radii, type scale)

  # reading & formats
  epub.rs          EPUB OPF metadata + cover extract/replace
  epub_book.rs     open an on-disk EPUB: spine, TOC, chapter text, safe unzip
  epub_write.rs    metadata writeback into the EPUB's OPF
  epub_writer.rs   build a new EPUB from fetched HTML chapters (Part 2)
  dict.rs          StarDict / SQLite / TSV dictionary import & search
  shelf_rules.rs   smart-shelf rule documents → SQL
  comics.rs        CBZ/CBR archive reading
  pdf.rs           PDF parsing via lopdf, text extraction, smart crop
  author.rs        author profile fetch + normalisation
  export.rs        quotes / vocabulary export (Markdown, CSV, Anki TSV)
  metadata/        Open Library + Google Books fetch, series lookup
  icons.rs         symbolic icon helpers

  # Part 2 scaffolding — present but not wired up, see below
  downloads.rs     download queue
  sources/         the Source trait + an empty SourceManager
  plugins/         Wasm plugin host — disabled (`// mod plugins;` in main.rs)

  pages/           Home, Library, AllBooks, Shelves (+ editor/detail),
                   ReadingList, History, Tags, Analytics, Book (+ float),
                   Author, Series float, Reader, Comics (+ comics_reader/),
                   PDF reader, MetadataEditor, SavedQuotes, SavedWords,
                   LookupHistory, Downloads, Browse, Settings
  widgets/         book row/card, charts, author links, focus trap, dialogs

crates/                    THE READING ENGINE (vendored, ~24k lines)
  chapbook-core/           formats, locators, geometry, fonts, credentials
  chapbook-epub/           EPUB container, OPF, font obfuscation
  chapbook-layout/         the layout engine: DOM → stylo cascade → box tree
                           → cosmic-text inline layout → pagination
  chapbook-paint/          page/panel/display lists, image store
  chapbook-render-tinyskia/  rasterizer
  chapbook-reader/         the reading session: cache, nav, selection,
                           highlights, scroll, zoom, conformance suite
  chapbook-viewer-gtk/     standalone GTK4 reference viewer (dev harness)
  kalam-reader/            the widget Kalam actually embeds — wraps
                           chapbook-reader, adds Kalam's themes/fonts/prefs

tools/
  chapbook-cli/            layout/render/text dump CLI + snapshot & golden tests
  kalam-reader-demo/       run the reader widget on its own, no Kalam

fixtures/          test EPUBs (with sources), fonts, golden render PNGs
resources/         app CSS, .desktop file, bundled dictionaries
wit/               Wasm interface for the (disabled) plugin host
```

### Part 2 scaffolding

`downloads.rs`, `sources/`, `plugins/` and `wit/` are the frame of the online
feature set: a queue, a `Source` trait, a Wasm plugin host. **Nothing
implements `Source`, so `SourceManager` is always empty**, and the plugin host
is commented out of `main.rs` (it references `wasmtime`, which is not a
dependency). The Downloads sidebar entry and the rail indicator are hidden for
that reason — see the comment on `NavItem::ALL` in `src/models.rs`. This is
all Part 2; [`docs/offline-roadmap.md`](./docs/offline-roadmap.md) is Part 1.

### Backend layout (post split)

The 3,400-line `db.rs` was split so each area is navigable. All methods live
on the same `Catalog`:

| File | Covers |
|------|--------|
| `db.rs` | schema/migrations, book CRUD, progress, row mappers, helpers (`escape_like`, `chrono_like_now`, streaks) |
| `db/dictionaries.rs` | merged store, search/lookup chain, sense parsing |
| `db/annotations.rs` | highlights, quotes, saved words, reading bookmarks |
| `db/history.rs` | event log, reading sessions |
| `db/metadata.rs` | metadata edits, overrides/restore, covers, ratings, goals |
| `db/shelves.rs` | shelves, reading list |
| `db/stats.rs` | analytics, backup |
| `db/authors.rs` / `db/series.rs` | author profiles, series cache |
| `db/prefs.rs` / `db/pronunciation.rs` | app prefs, IPA pronunciation |
| `db/search.rs` | search query parser (`tag:` / `author:` / `status:` / `rating:`) → SQL |
| `db/tags.rs` | tag CRUD and bulk metadata edits |
| `db/lookup_history.rs` | append-only dictionary lookup log (misses included) |

## Data

The layout is in **[Where your files live](#where-your-files-live)** above.
Full detail:

```text
<library folder>/              default ~/.local/share/kalam, one per library
  catalog.db                   books, shelves, highlights, progress
  library/<uuid>/
    book.epub · cover.jpg
    kalam.json                 backup of this book's details and highlights
  cache/reader/<uuid>/         extracted EPUB for the reader (throwaway)
  cache/thumbs/<uuid>.png      persistent cover thumbnails
  covers/                      stashed covers for metadata restore, by file hash
  authors/                     cached author photos
  series-covers/               cached series float covers

~/.local/share/kalam/
  dictionaries/                shared by every library, never duplicated

~/.config/kalam/               about this machine, not about books
  libraries.json               which libraries exist, and which is open
  prefs.json                   app settings shared by every library

~/Quotes.md            (export target — saved quotes, Markdown)
~/SavedWords.csv       (export target — vocabulary, RFC-4180)
~/SavedWords-Anki.txt  (export target — vocabulary, Anki TSV)
```

## Offline dictionaries

Kalam does not fetch dictionary data from the internet. You add a pack once,
then lookups work offline.

### Import a pack

1. Download a dictionary in one of the supported formats.
2. Open **Settings → Dictionaries → Import dictionary**.
3. For a StarDict pack, select its `.ifo`, `.idx`, or `.dict`/`.dict.dz`
   file. Keep all three files together; Kalam finds the matching files beside
   the one you select.
4. For a SQLite pack, select the `.db` file. It should have a table with
   `word` and `definition` columns.
5. For a text pack, select a `.tsv` or `.txt` file with one entry per line:
   `word<TAB>definition`.

The imported entries are copied into Kalam's catalog database, so the original
pack can be moved afterwards. Imported packs appear in the same Settings page
and can be removed there. On Linux, the dictionary data directory is
`~/.local/share/kalam/dictionaries`.

In the reader, select a word or complete phrase and choose **Dictionary** (or
press `D`). Kalam keeps the phrase, removes surrounding punctuation, and
lemmatizes the term before looking it up: WordNet's irregular exception lists
resolve `went` → `go`, `mice` → `mouse`, `better` → `good` and `running` →
`run`, with regular suffix rules as the fallback. Lookups are case- and
diacritic-insensitive — `Run`, `RUN` and `rún` all find `run` — because every
headword is indexed under a normalized key. Selecting a phrase looks for the
phrase itself first (so `run out of steam` resolves to the idiom entry), then
falls back to per-word results (`odd mixture` yields `odd` and `mixture`).

All installed dictionaries are combined into one merged store: each word
appears exactly once, provided by the highest-priority dictionary that has it
(WordNet first, then the other bundled packs, then any dictionary you
import), with all of that dictionary's senses listed. A word shared by two
dictionaries is never shown twice, and the other dictionary's version stays
hidden. The store rebuilds automatically whenever you import or remove a
dictionary. The popup shows up to five matching words; each has **Save word**
and **Copy** buttons.

The popup also shows the word's pronunciation as a compact IPA
transcription — `bank` → `/ˈbæŋk/`, `run` → `/ˈrʌn/` — from the bundled CMU
Pronouncing Dictionary 0.7a (BSD-style licence; provenance and checksums in
`resources/dictionaries/cmudict-0.7a.NOTICE.txt`). It is fully offline: the
packed dictionary is compiled into the binary, ARPABET phonemes are
converted to IPA with stress marks, and lookups resolve through the same
lemmatization as definitions, so `running` finds `run`. Words absent from
the dictionary (and multi-word phrases) simply show no transcription.

You can also tap any word in the book to look it up — no selection needed.
A plain click resolves the word under the caret and opens the popup for it
(with the surrounding sentence as context); a double-click still selects a
word for highlighting. With the popup open, ↑/↓ move a focus ring across
the senses and Enter saves the word with the focused sense's definition.
The magnifier button in the popup header highlights every occurrence of
the headword in the current chapter (Esc clears the highlights) — a
chapter-scoped stand-in until an in-book search exists.

### Reliable download sources

- [FreeDict downloads](https://freedict.org/downloads/) provides StarDict
  archives, SHA-512 checksums, many language pairs, and a direct link to the
  source dictionary. FreeDict says that each dictionary has its own licence;
  check the licence in the pack before redistributing it.
- [Princeton WordNet downloads](https://wordnet.princeton.edu/download) is the
  official source for the English WordNet database. It requires its licence
  notice and acknowledgement. WordNet normally needs conversion to StarDict,
  SQLite, or TSV before Kalam can import it.
- [Wiktionary dumps](https://dumps.wikimedia.org/) are broad but are released
  under CC BY-SA/GFDL terms. A redistributed extract needs attribution and
  must follow the share-alike and source-copy requirements, so Wiktionary is
  not silently bundled by Kalam.

Avoid download sites that only say “free” without naming the copyright holder
and licence. Oxford, Collins, Longman, and similar commercial dictionaries are
not safe to bundle without a separate redistribution licence.

### Bundled dictionaries

The base app includes four small English-only starter packs. **English WordNet
2025** has about 127,000 headwords and is about 4.2 MB compressed. **English
Idioms and Expressions** adds 1,024 phrase-to-meaning entries and is about
16 KB compressed. **English Synonyms (WordNet 3.0)** covers 110,000+ words
with their synset companions (about 1.7 MB compressed), and **English
Antonyms (WordNet 3.0)** adds 6,600+ antonym pairs (about 50 KB compressed).
All four packs are installed and enabled on the first run (new packs also
appear automatically on the first launch after this update), work without a
download, and appear separately in **Settings → Dictionaries**. If you remove
a pack, Kalam remembers that choice and does not silently add it back.

The WordNet pack is a format conversion of the [Open English Wordnet 2025
Edition](https://github.com/globalwordnet/english-wordnet/releases/tag/2025-edition),
which is derived from Princeton WordNet. Kalam also bundles the Princeton
WordNet 3.0 morphological exception lists (`noun.exc`, `verb.exc`, `adj.exc`,
`adv.exc`, gzipped) to resolve irregular lookup forms such as `went` → `go`,
and the English Synonyms and English Antonyms packs, which are derived from
the Princeton WordNet 3.0 synset and antonym-pointer data; their source,
checksums and licence note are kept beside the packed lists
(`wordnet-3.0-exc.NOTICE.txt`, `wordnet-3.0-synonyms.NOTICE.txt`).
The idiom pack is a format conversion
of [`baiango/english_idioms`](https://github.com/baiango/english_idioms), using
commit `d47bfb40a3f76d0f08ba1867016c383d3c21c596`. Its upstream repository
releases the data under The Unlicense. Its README says the list was collected
with ChatGPT and may contain grammatical, factual, or literal-versus-figurative
errors, so Kalam presents it as a supplemental phrase source rather than an
authoritative dictionary. Selecting a complete phrase such as `break a leg`
or `piece of cake` can now find that phrase in this pack. It does not make every
ordinary word combination meaningful automatically: `odd mixture`, for example,
remains two WordNet word entries unless a dictionary contains that exact phrase.

The bundled source revisions, checksums, attribution, and complete licence
notices are kept beside the generated packs in `resources/dictionaries/`.
Keep the applicable notices with any redistribution. The WordNet files must
remain because that data has both Open English WordNet and underlying Princeton
WordNet terms; the idiom pack has its own upstream Unlicense notice. “Personal
use” does not by itself remove licence obligations when data is committed to a
public repository or shipped in an application. No proprietary or
unclear-licence dictionary data is included.

Additional language packs can be added later after choosing the languages and
checking each pack's licence and size. The existing import flow remains the
way to add those packs now.

## License

GPL-3.0-or-later (aligned with typical GTK app norms; adjust if you prefer).
