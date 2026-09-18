# Kalam — architecture notes (living doc)

## Product

Personal Linux-only ebook manager + reader. One process. No plugin host, no content server.

**Name:** Kalam  
**Stack:** Rust · GTK4 · Relm4 · `kalam-reader` (native engine, no WebKit) · SQLite · `lopdf` for PDF  

## Navigation (P0)

```text
Sidebar
  Home
  Library  → hub tiles → section lists → Book page
  Shelves  → 2-col grid → Shelf detail → Book page
  Comics   → local CBZ/CBR grid → comics reader
  Settings (pinned to the bottom of the rail)
```

`Downloads`, `Browse` and `Fanfic` still exist as `NavItem`s, routes and
placeholder copy, but are **not** in `NavItem::ALL`, so the sidebar does not
show them. They belong to Part 2 and nothing implements a `Source` yet; see
`docs/offline-roadmap.md` and the comment on `NavItem::ALL`.

Book can open as:

1. **Page** in the main column (stack + Back), or  
2. **Float** — separate non-modal window with the same book page.

## Shelves UX (agreed)

1. Sidebar **Shelves** → grid (2 columns) of all shelves  
2. Each card: name, book count, smart/manual, rule/description  
3. Click card → shelf detail (list of books)  
4. Click book → book page (or Float)

Smart shelf ≈ Calibre virtual library (saved rules).  
Manual shelf ≈ pinned book ids.  
Rule engine is **not** implemented in P0 (sample data only).

## EPUB reader

**Native, not WebKit.** The reader is `crates/kalam-reader`'s `ReaderView`, a
`gtk::DrawingArea` that lays out and paints the book itself via the vendored
`chapbook-*` crates. There is no web process, no injected JavaScript and no
JS bridge. The engine replaced WebKitGTK; `docs/kalam/INTEGRATION.md` records
the wiring.

- Reading modes: **Paged** (one page at a time) and **Scrolled** (the whole
  book as one column, chapters laid out on demand with estimated heights that
  self-correct). Switching modes keeps your place.
- Preferences are four numbers plus a theme — `font_px`, `line_height`,
  `column_px` and one of four `KalamTheme`s — applied live with no reload.
- Position is a `LayeredLocator`, serialised field by field into the
  previously-unused `annotations.cfi` column (`locator_to_json` /
  `locator_from_json` in `src/pages/reader/engine.rs`). It survives relayout,
  font changes and window resizes.
- Everything the old JS did is now a method call, and everything the JS sent
  back arrives through the callbacks `engine::wire` installs:
  `connect_position`, `connect_selection`, `connect_word`,
  `connect_external_link`, `connect_image_tap`.
- The selection chip and the dictionary popover are real GTK popovers built
  in `engine.rs`, not HTML drawn inside a page.

The page is still split into sub-modules under `src/pages/reader/`: `mod.rs`,
`types.rs`, `mod_model.rs`, `chapter.rs` (chapter identity only — the engine
owns the rest), `session.rs` (telemetry + progress checkpoint), `engine.rs`
(all engine wiring), `js_bridge.rs` (**misnamed leftover** — it is now the
dictionary query and its popover, with no JS in it; rename when convenient),
`ui_prefs.rs`, `settings_panel.rs`, `panels.rs`, `lists.rs`, `chrome.rs`.

Dictionary flow is unchanged from P3: Settings → import StarDict / SQLite /
TSV → `dict_entries` → exact → prefix → substring → popover anchored at the
selection.

## Shelves engine (P4)

Two kinds share one table, separated by `kind`:

- **Manual** — rows in `shelf_books`, hand-ordered by `position`
- **Smart** — a JSON rule document in `shelves.rules`, compiled at query time

Rule documents are deliberately **flat**: a list of `{field, op, value}` plus a
single `match: all | any`. `shelf_rules::RuleSet::to_sql()` turns that into a
parameterised `WHERE` fragment over `books`; tag rules become
`EXISTS (SELECT 1 FROM book_tags …)`, negations wrap in `NOT`. Unknown fields or
blank values are skipped rather than failing, and an empty rule set compiles to
`0 = 1` so a half-built shelf matches nothing instead of the whole library.

Nested boolean groups were considered and deferred — the JSON can gain a
`groups` key later without a schema migration.

## History & time tracking (P4)

- `reading_events` is append-only: `opened | finished | unfinished | imported`.
  Repeat opens inside the same hour are collapsed so flipping in and out of the
  reader doesn't flood the log.
- `reading_sessions` gets one row per reader mount, closed in `shutdown()`.
  Durations are clamped to 6h — a suspended laptop must not claim a marathon.
- Auto-finish fires once at ≥99% progress; `finished_at` guards re-firing.

## Data dirs (actual)

```text
~/.local/share/kalam/
  catalog.db                 # + P4: shelves, shelf_books, reading_list,
                             #   reading_events, reading_sessions
  library/<uuid>/            # book.epub + cover.*
  dictionaries/              # (placeholder dir, actual entries in catalog.db)
  cache/reader/<uuid>/       # extracted EPUB — used by the book/library pages
                             #   to list chapters; the engine reads the .epub
                             #   itself and does not need this
  cache/thumbs/<uuid>.png    # persistent cover thumbnails (A0 step 3)
  covers/<file_hash>.<ext>   # stashed covers for metadata restore — survives
                             #   book deletion, hence not under library/<uuid>/
  authors/                   # cached author photos
  series-covers/             # cached series float covers
~/Quotes.md                  # exported quotes Markdown
~/SavedWords.csv             # exported vocabulary (RFC-4180)
~/SavedWords-Anki.txt        # exported vocabulary (Anki TSV)
~/.config/kalam/config.toml  (future)
```

## Service layer (A0 step 2)

Pages do not talk to `Catalog` directly any more (three converted so far —
Home, Analytics, Tags; the rest migrate incrementally). They hold a
`LibraryService` and ask it one question:

```rust
let snap = service.home();   // stats + recent + continue row + reading list
```

Three properties matter, and each is load-bearing:

1. **One call, one owned snapshot.** Not wrapped getters. A snapshot is a
   plain `Send` struct, so the same call can later run on a worker thread and
   be handed back to the UI *without touching the page* — the whole point of
   the step. `snapshots_are_send()` asserts this at compile time.
2. **One error policy.** A failed read degrades to the empty value **and**
   records the reason; the page surfaces it. Previously each page decided for
   itself, so a broken database looked like an empty library.
3. **The service never calls `notify`.** Toasts are thread-local to the UI
   thread; the service has to stay callable from a worker. Reporting belongs
   to the caller.

Query logic lives in the service, not in widget-building code — Home's
"continue reading" fallback chain (recently opened → in progress → newest) is
there, and unit-tested.

Writes still go straight to `Catalog`. They belong to the task manager
(A0 step 4), not to this read seam.

## Source seam (A0 step 8 — designed, not yet code)

> **SET ASIDE — this is Part 2 work (2026-09-18).** The owner's instruction:
> the plugin system belongs to Part 2, so it is not being decided or built now.
> Everything below stays as the *design record*, and one thing in it is already
> known to be stale: **the Lua choice below was superseded.** The current
> decision is **WebAssembly**, recorded in `docs/conversation.md` §22. It is
> not being implemented, and the reasoning that would make it safe to implement
> (feature-gate `wasmtime`, do not ship the host before a second plugin exists,
> keep the TOML selector config for selector-only fixes) is written there.
>
> Do not start on this section during Part 1. The parts that are *not* about
> plugins — one `Source` trait for fiction and manga, rate limits enforced by
> the host, a source being a pure function with no filesystem or widgets — are
> still the intended shape whenever Part 2 does pick this up.

Where books come from that are not the user's disk: AO3, FanFiction.net, Royal
Road, MangaDex, Komga. Full design in
[`docs/source-seam.md`](./docs/archive/source-seam.md); the essentials:

- **One `Source` trait for fiction and manga**, not two. They differ only in
  the final step, which is a two-variant `Content` enum (`Text` / `Images`).
  Everything else — search, pagination, chapter lists, rate limits, the
  download queue, the follow scheduler — is shared and must not be duplicated.
- **Scraped sources are Lua plugins; API-backed ones are built-in Rust.** The
  split is *does this break when someone else changes their website* — AO3,
  FFN and scraped manga rot, so they get a fix loop measured in seconds
  (edit a selector, restart); MangaDex, Open Library and Google Books have
  documented APIs, so they are compiled in and type-checked. Same for add-on
  metadata providers: two built in, the long tail in Lua, which is Calibre's
  model. `docs/source-seam.md` §9a has the reasoning and the evidence.
- **`SourceFactory` is `Send`; `Source` is not.** The factory crosses to a
  worker thread and builds the live source there. This is what makes Lua
  possible at all: `mlua`'s VM is `!Send`, so it is built on the worker and
  born and dies on one thread, and `mlua`'s `send` feature (a reentrant mutex
  on every VM access) stays off.
- **A source is a pure function from a query to structured data.** No
  filesystem, no catalog, no widgets. The host decides what to store — the
  same discipline that keeps `LibraryService` worker-callable. For Lua this is
  *enforced* by the sandbox; a built-in Rust source follows it voluntarily.
- **Rate limits are declared by the source and enforced by the host**, so one
  careless source cannot get Kalam's User-Agent blocked.

Lands as code with AO3 in P7, its first implementation and first caller — a
trait with no implementation would fail `-D warnings` in a binary crate.

## Reader engine (replaces the old "Reader WebView" section)

There is no WebView pool any more, and `src/webview_pool.rs` is gone. The
`KALAM_NO_WEBVIEW_POOL=1` switch listed in the README's switch table is a
leftover and does nothing.

What replaced it: one `ReaderView` per reader page, created in
`ReaderModel::init` and torn down in `shutdown()`. There is no cross-book
reuse to manage, because there is no ~400 ms web-process spawn to avoid — the
widget is constructed in-process.

What *is* worth keeping from the old design is the underlying problem, which
still applies in a different form: the engine's per-chapter layout is cached
inside the session (`Session`'s layout and image caches, see
`crates/chapbook-reader/src/cache.rs`), and `ReaderView::cache_bytes()`
reports it. Closing a book drops the session and its cache with it.

## Theming & CSS

- **`src/theme.rs`** owns every colour. One `Theme` struct per palette, 13 dark
  themes grouped standard/darker per family. Adding one is a single entry.
- **`src/style.rs`** owns *shape only* — spacing, radii, type scale, borders. It
  refers to colours by `@kalam_*` name; a literal hex there is a bug unless it
  is deliberately theme-independent (highlight markers, reader paper swatches,
  reader stage).
- `theme::apply()` prepends the `@define-color` block and re-parses, so a theme
  switch restyles in place with no widget rebuilt.
- Reader *page* theming (Light/Sepia/Dark paper) is separate from app chrome, on
  purpose: a sepia page inside a dark app is a legitimate combination.

> ⚠️ **Before editing `style.rs`, read its module header.** It documents five
> GTK behaviours that are non-obvious and cost about a dozen debugging rounds on
> a single scrollbar — provider priority vs specificity, why `opacity` below 1
> triggers pixman errors, how `margin`/`border`/`padding` are subtracted from
> allocations, what `scrolledwindow:hover` actually matches, and the
> `KALAM_NO_CSS=1` diagnostic.

## Out of scope

- Z-Library / unauthorized shadow libraries  
- Calibre-style multi-app suite  
- Content server, fetch news  

## Phase map

| Phase | Deliverable |
|-------|-------------|
| P0 | Shell + nav + sample shelves/books ✅ |
| P1 | SQLite + EPUB import + covers ✅ |
| P2 | Reader (chapter scroll, fonts, progress) ✅ |
| P3 | Highlights, quotes, offline dictionary ✅ |
| P4 | Shelves engine, lists, history, tags, analytics ✅ |
| P5 | Metadata edit, cover replace, Open Library fetch ✅ |
| P5.5 | UI overhaul (colour system, 13 themes, Settings v2, book page) — in progress |
| A0 | Architecture & performance track ← **you are here** (steps 1–5 done, 6 closed on evidence, 8 designed; 7 open) |
| P6+ | Downloads, sources (AO3, FF), comics, PDF, tools |

`ROADMAP.md` is the authoritative plan; this table is a summary. See its
"Current trajectory" section for the locked order.
