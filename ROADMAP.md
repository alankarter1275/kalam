# Kalam — Roadmap

**This is the only roadmap.** It replaced both the old P0–P12 phase plan and
`docs/offline-roadmap.md` on 2026-09-18. The old phase write-ups are archived
in [`docs/archive/roadmap-phases-p0-p12.md`](./docs/archive/roadmap-phases-p0-p12.md),
kept for their lessons, not for planning.

Part 1 is a **fast, capable, offline** ebook reader, editor and library. Every
online piece — plugins, scrapers, MangaDex/AO3/FicHub clients — is Part 2.

---

## ⚠️ Read this first — every chat, every agent

If you are an AI agent starting work on this repo, read ALL of this before
touching code:

1. **`README.md`** — what the app is, current status table.
2. **This file** — the plan, the order, what is really shipped.
3. **`docs/conversation.md`** — the living design log: every accepted and
   rejected idea, with reasons. **Do not re-litigate settled decisions** — if
   you think one is wrong, raise it in chat first.
4. **[`docs/pitfalls.md`](./docs/pitfalls.md)** — mistakes already made here and
   how they were fixed. Every entry cost a CI cycle or shipped a bug once
   already. Skim it before you write code, and **read the relevant section in
   full** before you touch GTK sizing, scrollbars, keyboard shortcuts, error
   paths, or anything that deletes a call site.
5. **[`docs/kalam/RESTRICTIONS.md`](./docs/kalam/RESTRICTIONS.md)** — the live
   guardrails for the reading engine.

**Hard rules for every change you make:**

- **Write and speak in plain, simple English.** Standing instruction from the
  owner, given 2026-09-04: *"I can't understand anything you said. I need you to
  use easy and simple language to explain things."* It applies to chat replies
  first, and to docs and comments too.

  What that means in practice:

  - **Explain the thing before naming it.** Not "the `PENDING_FRAMES` weak-ref
    map breaks under recycling" — instead "each cover picture remembers which
    book it belongs to. If we start reusing pictures for different books, a
    slow-loading cover can land on the wrong book."
  - **No unexplained jargon.** *Virtualization*, *refcount*, *GObject*, *N+1*,
    *LRU* mean nothing on their own. Say it in ordinary words, or give a
    one-line explanation the first time.
  - **Short sentences. One idea each.** Long sentences with three clauses and
    two dashes are the main problem, not the vocabulary.
  - **Lead with the answer**, then the reasoning.
  - **Numbers need meaning.** Not "507 MB peak RSS at 2,000 books" — "it uses
    507 MB of memory, which is a lot on a 4 GB machine."
  - **When asking the owner to choose, make the options concrete**: what
    changes on screen, what could break, how long it takes.

  Being simple is not the same as leaving things out. Keep the honesty, the
  caveats and the numbers — just say them in words that do not need a glossary.
  This rule is about *communication only*. It does not lower the bar for the
  engineering, the testing, or the docs.

- **Keep the docs current — in the same commit as the code.** A change that
  leaves this file stale is **not done**.
- **Commit work regularly after major changes or completed batches.** Standing
  instruction, 2026-09-05. Do not leave large diffs accumulating.
- **When you get something wrong, write it down.** If a mistake cost a CI
  failure, produced a user-visible bug, or was only caught by re-reading an
  existing warning, add it to `docs/pitfalls.md` in the same commit as the fix:
  what went wrong, *why*, and what to do instead.
- **Never skip the changelog.** This file ends with a changelog table — append a
  dated row for every phase shipped or decision locked. It is how a new chat
  catches up in one glance.
- **CI is the gate.** There is no local Rust toolchain in the sandbox: push and
  watch GitHub Actions (`gh run list`). Never force-push. The agent edits
  `.github/workflows/ci.yml` directly and pushes it — the owner granted the
  GitHub App the `workflows` permission on 2026-09-18. See `docs/ci/README.md`.
- **You cannot see the screen.** The owner is the QA loop for anything visual:
  ask for error text (not screenshots — you cannot view them), and have them run
  the app on Arch at phase boundaries.
- **Branch:** each Arena session is pinned to its own `arena/<id>-kalam` branch.
  Work only on the branch the current session names, and push only to it. Do not
  copy the branch id out of this file — it changes every session.
- **A status claim needs evidence.** See the next section. A ✅ with nothing
  behind it is how this project spent a year believing the downloads hub was
  finished.

---

## Working agreement

| Who | Does |
|-----|------|
| **Agent** | Implements the phase, pushes code and workflow files, keeps CI green, updates this doc |
| **CI (GitHub Actions)** | `fmt` · `clippy` · `test` · `cargo build` on every push, plus a non-blocking GUI smoke test |
| **You** | (1) Paste failed CI step logs only when `ci-logs/` does not have them. (2) Run the app on Arch **once per completed phase** for UX feedback |

You do **not** need to build between small commits. Only at phase boundaries.

### Definition of Done (every phase)

1. CI green on the branch
2. Feature list for that phase implemented
3. README / ARCH / **this file** updated if behaviour or UX targets changed
4. You ran it on Arch (or explicitly deferred) and listed change requests

### Parked for later discussion

Raised, deliberately not decided, and **not** dropped.

- ~~**Multiple books/readers open at once**~~ **Resolved — it is item 2.12.**
  Raised 2026-09-04 as "discuss before designing", then discussed and designed
  on 2026-09-19: the bubbles. Design and recorded decisions are in
  [Bubbles](#bubbles--multiple-books-open-at-once).

### Non-goals (whole project)

- Z-Library / unauthorized shadow libraries
- Calibre multi-app suite, content server, fetch news
- Windows / macOS
- **Tap-a-word-to-open-dictionary.** Shipped once under WebKit, removed
  deliberately on 2026-09-18. Look a word up by selecting it. Do not
  reintroduce it — `ReaderView::connect_word` and `TappedWord` were deleted
  from the engine so it cannot come back by accident.

---

## North star

> One native Linux app: a fast personal library for a few thousand books,
> excellent EPUB reading, annotation and editing, honest metadata — no bloat,
> and nothing that needs the network to work.

**Stack:** Rust · GTK4 · Relm4 · custom CSS · SQLite · **`kalam-reader`** (the
native engine — no WebKit) · **MuPDF** (`mupdf = { version = "0.8", default-features = false, features = ["base14-fonts"] }`) for native PDF rasterization and vector outlines · `tantivy` for full-text search (planned)

---

## What is really shipped

Verified against the code on 2026-09-18, not copied from a status mark. Every
line cites the evidence it rests on. **When a claim has no evidence to cite,
that is the finding** — which is exactly how the downloads hub stayed "done" for
a year.

| Area | Status | Evidence |
|---|---|---|
| App shell, routing, sidebar | shipped | `src/app.rs`; sidebar is `NavItem::ALL` in `src/models.rs` |
| Library core: import, grid, search, sort | shipped | `src/db.rs`, `src/pages/library.rs` |
| EPUB reading | shipped, engine replaced | `crates/kalam-reader/`, `src/pages/reader/` |
| Annotations, highlights, quotes | shipped | `src/db/annotations.rs` |
| Offline dictionary | shipped (data layer) | `src/db/dictionaries.rs` 1,975 lines, `src/dict.rs` 1,074 |
| Pronunciation (IPA) | shipped | `src/db/pronunciation.rs`, used by `src/export.rs` |
| Vocabulary tools, CSV/Anki export | shipped | `src/pages/saved_words.rs` |
| Lookup history | shipped | `src/db/lookup_history.rs`, routed in `src/app.rs` |
| Dictionary priority reorder | shipped | `move_dictionary_priority`, wired at `src/pages/settings.rs:498` |
| Shelves, smart shelves, reading list | shipped | `src/db/shelves.rs`, `src/shelf_rules.rs` |
| Analytics, streaks, reading time | shipped | `src/db/stats.rs` |
| Metadata editor, Open Library, Google Books | shipped | `src/pages/metadata_editor.rs`, `src/metadata/` |
| Multi-library switcher, `kalam.json` sidecars | shipped | `src/libraries.rs`, `src/sidecar.rs` |
| Comics reader (CBZ/CBR) | shipped | `src/comics.rs` + `src/pages/comics_reader/`, 1,685 lines |
| In-app dialogs | shipped | `src/widgets/in_app_dialog.rs`; no `gtk::Window::builder` left in `src/` |
| Background tasks, preloaders, thumbnails | shipped | `src/tasks.rs`, `src/preload.rs`, `src/thumbs.rs` |
| Windowed book grid | shipped, default | 502 MB → 247 MB at 2,000 books |
| Perf budgets (query counts) | shipped, gating CI | `src/perf.rs`, six budgets, not `#[ignore]`d |
| Find in chapter | shipped | `ReaderView::search` wired at `src/pages/reader/mod.rs:1161` |
| **Downloads hub / online sources** | **NOT shipped** | Zero `impl Source` in the repo. `SourceManager::new()` builds an empty list. `Downloads`, `RemoteBrowse` and `Fanfiction` are deliberately absent from `NavItem::ALL`, so the sidebar cannot reach them. `src/models.rs` documents why. **Part 2.** |
| **PDF reader & engine** | **shipped** | Rebuilt from scratch with MuPDF (`mupdf = { version = "0.8", default-features = false, features = ["base14-fonts"] }`). Crisp 8.7ms rasterization, unified EPUB-matching chrome (floating back chip, bottom pill with page jump and zoom, autohiding controls, slide-in TOC sidebar), continuous vertical scroll and paged modes, bounded memory cache (<80 MB). Replaced heuristic `lopdf` viewer. |
| **Wasm plugins** | **NOT shipped** | `// mod plugins;` is commented out in `src/main.rs`; `wasmtime` is in neither `Cargo.toml` nor `Cargo.lock`. **Part 2.** |
| Full-text library search | not started | `tantivy` is not a dependency |
| EPUB editor | not started | — |

**Test baseline (run `35399375314`):** 52 test binaries, **750 tests passed, 0
failed**, 6 ignored, no panics. Before `--workspace` was added on 2026-09-18, one
test binary ran.

---

## Part 1 — the offline library

Part 1 is split into **eight phases**. Unlike the old plan, these *are* a queue:
work top to bottom, and take the next unfinished item from the phase you are in.

**Why this order.** The question was whether to do all the invisible
groundwork first so the backend is solid before anything visible gets built.
The answer is **yes for the groundwork that later work sits on, but not all of
it up front**, and the phases alternate on purpose.

Three reasons:

1. **Some invisible work is genuinely load-bearing.** Making the database
   layer asynchronous is a prerequisite for the search index, folder-watch
   import, EPUB patch baking and bulk edits. Building any of those first means
   rebuilding it. So Phase 1 exists, and it is invisible.
2. **Doing *all* the invisible work first is a trap.** Six months of
   groundwork with nothing on screen is hard to judge and easy to get wrong —
   you cannot tell whether an abstraction is right until something real is
   built on it. So the phases alternate: invisible, visible, invisible,
   visible.
3. **Cheap fixes should not wait for a phase of their own.** Dead
   dependencies, a mis-sized icon, a missing file association — these are
   minutes each, so they are batched into Phase 1 rather than given a phase.

Each phase below ends with **"Done when"**. A phase is not finished when the
code is written; it is finished when that line is true.

### Every work item has a permanent number

Items are numbered `1.1`, `2.4`, and so on. **The numbers never change.**
Finished items keep their number and are struck through; new items are
appended at the end of their phase with the next free number. Renumbering
would break every reference anyone has made to an item.

Two rules follow from that, and they are the point of the numbering:

- **Work goes in number order.** Take the lowest unfinished number in the
  current phase. If an item is blocked, say so and move to the next one —
  do not quietly skip ahead to something more interesting.
- **Nothing is built that is not on this list.** When something new comes up
  mid-phase — a defect, a good idea, a "while we're in here" — it gets a
  number and a place *before* it gets written. This is the rule that stops
  the plan drifting, and it exists because that is exactly what happened
  before: work was done because it felt urgent at the time, and a year later
  nobody could say what was finished.

The exception is a defect in code being touched *right now*, where leaving it
would be worse than fixing it. Fix it, then give it a number in the changelog
row for that commit so the record still accounts for it.

---

### Phase 1 — Foundations

> Mostly invisible. Everything after this sits on it.

**Goal:** make the ground solid enough that later phases are built once, not
twice.

**Why first:** the database layer is synchronous today, which is the single
thing that blocks the search index, background import, EPUB patch baking and
bulk editing. Fixing it later means rewriting all four.

**Work:**

- ~~**1.1 — Write the engine damage list.**~~ **Done, 2026-09-19.** The audit
  is the numbered list that follows in Phase 2 — items 2.1 through 2.11 came
  out of it. What it found, in short: three things broken today (text PDFs,
  the dictionary's keyboard, its tests), four engine features with no control
  for them, and five documents that describe an app that no longer exists.
  **One gap the audit cannot close:** it was done by reading the code, not by
  running the app. Anything that only misbehaves when clicked is still
  unfound, and will be numbered as it turns up.
- ~~**1.2 — Move blocking SQLite off the UI thread.**~~ **Done, 2026-09-19.**
  *(1.2a measured; 1.2b closed then **reopened** the same day, then built.)* *Renamed 2026-09-19. The
  original wording — "make `LibraryService` asynchronous" — was wrong, and
  following it literally would have contradicted a decision already recorded in
  the code.* `src/tasks.rs` has a section headed **"No tokio"**: `thread::spawn`
  + `async-channel` + the GLib main loop, because relm4 is already the actor
  framework and a second runtime is a second scheduler fighting the first.
  Making the service `async fn` would require exactly that second runtime.
  **What the code was actually designed for** is the opposite shape, and it is
  already built: the methods stay synchronous, they return plain owned `Send`
  snapshots, and the *caller* runs them on a worker via `tasks::spawn`.
  `service.rs` says so — "the same call can later run on a worker thread and be
  handed back to the UI, without touching the page" — and
  `snapshots_are_send()` asserts `Send` on the service and all 12 snapshots so
  a future edit cannot quietly break the promise.

  **The gap is that nobody is using it.** Measured this turn: **92** service
  call sites across `src/pages/`, of which **2** are near a `tasks::spawn`.
  They sit in `init()` and `reload()` — both UI-thread paths. The work is
  routing those calls, not rewriting the service.

  Split in two, because converting all 92 blind would be the shared-font-system
  mistake again — a large change with a loading state on every page, justified
  by no measurement. There is **no** timing data for any service call today.
  - ~~**1.2a — Measure what a service call costs.**~~ **Done, 2026-09-19.** All
    16 snapshot methods instrumented via `timing::measure`, measured on the
    owner's Arch machine. Results below.
  - ~~**1.2b — Make the pages stop doing database work.**~~ **Done,
    2026-09-19.** *Reopened 2026-09-19 after being wrongly closed the same
    day, then built and confirmed by the owner.* It was originally closed on
    the grounds that every query is fast — and that reasoning was wrong in two
    ways, both worth keeping on record.

    **First, it answered a question that was never asked.** The Yazi philosophy
    in `docs/conversation.md` §3 is *"don't make the UI fast — make it never
    wait"*, and its first rule is *"the UI never does work."* That is a
    statement about architecture, not about milliseconds. Closing the item
    because the queries are cheap today mistook a measurement for a decision.

    **Second, the measurement was taken at the wrong scale.** The library is
    **150 books** (`grid_cards_total 150`); Phase 7's target is **2,000**.
    `all_books()` returning 150 rows in 4.7 ms says nothing about 2,000 rows,
    and the queries that scale with library size — `all_books`, `dashboard`,
    `history`, `words`, `quotes`, `tag_books` — are exactly the ones that will
    stop being cheap. Measuring at 7.5% of the design scale and concluding
    "not needed" does not survive the roadmap's own goal.

    The measurements stay valid and are kept below; what changes is the
    conclusion drawn from them.

    | Query | ms at 150 books | Scales with library? |
    |---|---|---|
    | `dashboard` | **10.8** | yes |
    | `home` | 8.4, 2.6 | yes |
    | `all_books` | 4.7 | **yes — the worst case at scale** |
    | `history` | 2.6 | yes |
    | `tag_books` | 1.5, 0.9 | yes |
    | `tags` | 1.4, 1.2, 1.1 | yes |
    | `quotes` | 0.9 | yes |
    | `book_detail` | 0.7 | no |
    | `shelves` | 0.5 | no |
    | `reading_list` | 0.5 | no |
    | `analytics` | 0.3 | no |
    | `reader` | 0.3 | no |

    **Three design questions must be answered before converting anything,**
    because converting 92 call sites without them would make the app feel
    worse, not better:
    - **No flash of empty state.** A page whose query round-trips must keep
      showing what it already has and refresh in place, not clear and refill.
      Yazi gets away with loading states because its operations are genuinely
      slow; a 1 ms query behind a blank frame is a regression. Concretely:
      today a page asks and blocks for ~1 ms, which is invisible. Moving the
      question to a worker creates a moment with no answer, and clearing the
      view for that moment would replace something instant with a flicker.
      The page keeps its current snapshot and swaps it when the reply lands.
    - **Convert by scaling risk, not uniformly.** Seven of the measured
      queries grow with library size — `all_books`, `dashboard`, `home`,
      `history`, `tag_books`, `tags`, `quotes`. Five do not: `shelves`,
      `reading_list`, `analytics`, `reader`, `book_detail` are bounded by
      shelves, or by one book, and cost the same at 5,000 books as at 150.
      Moving those buys nothing and costs a loading state. **The exception is
      written here so it reads as a decision and not as unfinished work.**
      Three queries were never measured (`words`, `lookup_history`,
      `book_stats`); `words` and `lookup_history` grow with vocabulary rather
      than library size and should be measured before being classified, not
      assumed. `shelf_detail` is bounded by one shelf.
    - ~~**A cache belongs in the service layer.**~~ **Deferred, 2026-09-19.**
      Refresh-in-place already removes the visible delay a cache would hide,
      which makes the cache an optimisation rather than a requirement. Against
      that, a cache carries a corruption-flavoured failure mode: getting
      invalidation wrong means showing books that were deleted. `service.rs`'s
      own doc does name "no single place to put caching" as one of the three
      problems it was written to solve, so this is a real gap — but it is
      revisited only if the pilot or a later measurement shows a page that
      feels slow without it. Same rule that retired the shared font system and
      the Cairo renderer.

    **Plan: one page as a pilot, then roll out.** Converting 92 call sites
    before the pattern is proven is how a mistake gets repeated 92 times. The
    pilot is **All Books** — the query that scales worst (`all_books`, 4.7 ms
    at 150 books, 4 call sites in `src/pages/all_books.rs`).
    1. `all_books()` runs on a worker via `tasks::spawn`.
    2. The page keeps rendering its current list while the query runs.
    3. The new snapshot replaces it on arrival; a failed read keeps the old
       list and surfaces the error rather than blanking the grid.
    Then the owner runs it. **The success criterion is that it feels
    identical** — this change is about never blocking, not about being faster,
    and a pilot that feels slower has failed even if the code is correct. Only
    then do the other seven.

    The `Send` guarantee is already load-bearing here: `snapshots_are_send()`
    asserts `Send` on the service and all 12 snapshots, which is exactly what
    handing them across a thread requires.

    **Note on page ownership.** Each page builds its own `LibraryService`
    (`LibraryService::new(catalog)` in `all_books.rs:261`), so nothing is
    shared between pages today. That is fine for the pilot and is one more
    reason the cache is deferred: a useful cache would have to be shared
    process-wide, which is a larger change than the pilot needs.

    **Pilot built, 2026-09-19 — awaiting the owner's verdict.** `reload()` now
    hands the query to `tasks::spawn` and returns immediately; the grid keeps
    what it is showing and swaps the list on arrival. Three details that were
    not in the original plan and came out of writing it:
    - **Replies carry a generation counter and stale ones are dropped.**
      `SearchChanged` fires on *every keystroke*, so typing quickly starts
      several queries; without this a slow early result could land after a
      fast later one and put the wrong list on screen. This is the bug the
      design review did not anticipate and only surfaced when the call sites
      were read.
    - **A failed read now keeps the list and reports the error.** The
      synchronous version called `books.clear()`, which turned a failed read
      into something that looks like data loss.
    - **`AllBooksSnapshot` was added to `snapshots_are_send()`.** It now
      crosses a thread, so the property is compile-checked; it was not in the
      list before. Worth checking for each of the remaining seven.

    `init()` keeps its synchronous read deliberately — the page is not on
    screen yet, so there is nothing visible to freeze and no list to preserve,
    and making it asynchronous would draw an empty grid and then fill it. It
    is the one blocking read left on this page and is marked for revisit once
    a large library makes it measurable.

    **What CI proved and what it did not.** Run `35432889032` is green —
    clippy, 762 tests, both builds — and `snapshots_are_send` ran with the new
    type in it, so the snapshot genuinely can cross a thread. **But no test
    exercises the new path**: the async reply needs a GTK main loop, so what
    is verified is that it compiles and that the types are right, not that the
    list refreshes. Only running the app shows that.

    **Rolled out, 2026-09-19 — owner confirmed the pilot, CI `35434414790`
    green (762 tests, 0 clippy findings).** The same shape now covers every
    search- and sort-driven reload: **history** (search, filter, clear,
    refresh), **saved quotes** (search, delete, save note, refresh), the **tag
    cloud** (re-read after rename, merge, delete) and **one tag's books** (sort
    change). Four more pages, all confirmed by reading their call sites rather
    than assumed.

    Reading them turned up three things the pilot had not shown:
    - **Two more pages blanked on a read error.** `saved_quotes` called
      `quotes.clear()`; `history` assigned `snap.events` unconditionally, which
      is empty on error, so the feed cleared the same way. Same bug, two more
      copies — which suggests checking for it in the remaining seven rather
      than assuming `all_books` was the only one.
    - **`saved_quotes` rebuilt its list inline right after `reload()`.** With
      the read now asynchronous that would have drawn the list the query had
      not replaced yet, so the rebuild moved into the `Loaded` handler. A side
      effect worth noting: `SaveNote` reloaded without ever rebuilding, so the
      list did not update after saving a note until the next message arrived.
      It does now — a latent bug the conversion happened to fix.
    - **The tag cloud's re-reads follow a write, and the write is still
      synchronous.** Moving writes off the UI thread is a separate and larger
      change; only the re-read moved, which is the part that counts every tag
      across the library and so the part that grows.

    `HistorySnapshot` joined `snapshots_are_send()`. The snapshots for pages
    that stay synchronous deliberately did **not** — asserting `Send` on a type
    that never crosses a thread proves nothing and would read as coverage it
    is not.

    **Still synchronous by design: every page's first read in `init`.** Each
    carries a comment saying so. The page is not on screen yet, so there is
    nothing visible to freeze and no list to preserve, and making it
    asynchronous would draw an empty view and then fill it. Revisit when a
    large library makes those reads measurable. `dashboard` (`library.rs:76`)
    has no reload path at all — it is read once at init and never refreshed —
    so it is entirely covered by this exception, not partially converted.

    **Not converted, and not missing:** `analytics`, `reading_list` and
    `shelf_detail` each keep their own synchronous `reload()` — those are three
    of the bounded queries in the table above. `saved_words` calls `words()`,
    the unmeasured query, and should be timed before it is classified.
    `comics` has a `reload()` too but reads through `catalog()` directly
    rather than a service snapshot, so it is outside this change either way.
- ~~**1.3 — Route background work through `src/tasks.rs`.**~~ **Done, 2026-09-19
  — the migration had largely already happened.** Audited rather than assumed:
  there are **27 `tasks::spawn` call sites** across imports, metadata fetching,
  thumbnails, dictionary install and downloads-adjacent work, so the pattern is
  established. What was actually left was three bare `std::thread::spawn`
  calls, and only one of them was a defect:
  - **`src/downloads.rs` — converted, and this was the real work.** One bare
    `thread::spawn` plus **six `lock().unwrap()`** calls on the job map. A
    worker panicking while holding that lock poisons it, and `.unwrap()` would
    then fail on *every* later read — including the UI thread drawing the
    download list, so one crashed download took the page down with it. Now
    routed through `tasks::spawn` behind a poison-tolerant `locked()` helper
    (the same recovery `tasks.rs` uses), with the cancellation flag checked
    **between chapters** so a cancelled download never leaves a half-built
    EPUB. Added `JobStatus::Cancelled` — distinct from `Failed`, because
    stopping on purpose is not the same claim as something going wrong — and
    its two UI match arms (`app.rs`, `pages/downloads.rs`). Two tests: the
    poison case that used to panic, and the ordinary path to prove `locked`
    is not only a workaround.
  - **`src/metadata/mod.rs:273` — deliberately left as a parallel map.** It
    spawns one thread per provider and joins them all inside a synchronous
    call whose whole contract is *returning* merged results; making it a
    background task would change what it means. Recorded here as a principled
    exception so it does not read as drift. It did get one real fix: a
    **panicking** provider was swallowed by `Err(_) => {}`, so a crash looked
    identical to "no matches". The source id is now taken *before* spawning —
    `join()` returns nothing but the panic payload, so the provider's name
    would otherwise be lost with it — and reported as an error.
  - **`src/notify.rs:529` — not production.** It is inside a test that
    asserts a worker's notification crosses to the main thread. Left alone.

  Two of the subsystems 1.3 named do not exist yet: **index rebuilds**
  (`tantivy` full-text search is unshipped) and **patch baking** (the EPUB
  editor is Phase 6). They inherit this pattern when they are built.
  **The cancel *button* is not part of this item** — cancellation now works
  and `cancel_all()` reaches downloads, but there is still no UI to trigger
  one. That is 1.14.
- ~~**1.4 — Install a logger.**~~ **Done, 2026-09-19** (`src/logging.rs`). The engine
  makes 17 `log::` calls that were discarded because no logger was installed;
  they now go to the terminal and to `~/.local/share/kalam/kalam.log` when
  `RUST_LOG` is set, and nowhere at all when it is not. The app's own 83
  `eprintln!` sites are a separate, larger job and are still invisible.
- ~~**1.5 — Measure what opening a book costs.**~~ **Done, 2026-09-19.** Measured on
  the owner's Arch machine across four books. Results and what they settle are
  in [Bubbles](#bubbles--multiple-books-open-at-once).
- ~~**1.6 — Share one font system across sessions.**~~ **Decided against, 2026-09-19 —
  not needed.** The measurement says the font scan is **1–5 ms** out of a 32–78
  ms open. Sharing it would save milliseconds and add a real complexity cost.
  Left as-is on purpose; see the table in
  [Bubbles](#bubbles--multiple-books-open-at-once) before revisiting.
- ~~**1.7 — Fix `paths.rs::home_dir()`.**~~ **Done, 2026-09-19.** It read only
  `$HOME` and fell back to `PathBuf::from(".")`, so with `HOME` unset the app
  created a `kalam/` folder in whatever directory it was started from.
  Now: `$HOME` when set and non-empty, otherwise `getpwuid_r`. An empty `$HOME`
  counts as unset, because `PathBuf::from("")` joins to a relative path and
  would reproduce the bug in a different shape.
  - **Three copies, not one.** `pages/saved_quotes.rs` and `pages/saved_words.rs`
    each had a private `mod dirs` shim containing the identical one-liner —
    named as though it were the `dirs` crate, which is not a dependency. Both
    deleted and repointed, so the bug has one home instead of three.
  - **`getpwuid_r`, not `getpwuid`.** The plain form fills a static buffer, and
    `home_dir` became reachable from worker threads when 1.2b moved page
    queries onto background tasks — concurrent calls would race over it. Worth
    noting that 1.2b is what made this matter.
  - **The signature was checked against docs.rs rather than remembered**, and
    that mattered: glibc's `getpwuid_r` takes **five** arguments, the first
    draft used the four-argument BSD form, and CI rejected it. Same lesson as
    the `From<Cow<str>>` error in the follow-up commit — both were assumed
    APIs, both caught by the build.
  - **The `"."` fallback stays** — refusing to start would be worse — but
    `legacy_data_dir` now logs an error naming the directory. Silent is what
    let a library end up somewhere nobody could find.
  - **Four tests, all confirmed running by name** (766 passed, up from 762).
    They drive `resolve_home` directly rather than mutating the environment,
    which is process-wide and would race every other test in the binary. They
    deliberately do not assert the resolved value or that it is absolute —
    both depend on the machine running them.
  - `libc` promoted to a direct dependency; already in the tree transitively,
    so no change to build time or binary size.
- **1.8 — Batch of small, safe fixes.** *Closed 2026-09-19: seven done, one
  deliberately skipped, one moved to 1.16. The roadmap assumed "a few minutes"
  each; that held for most and was wrong for two.*
  - ~~**1.8a** Delete three unused dependencies from the root `Cargo.toml`:
    `toml`, `urlencoding`, `scraper`.~~ **Done.** Confirmed zero uses across
    `src/`, `crates/` *and* `tools/` before removing — the earlier mistake of
    grepping only `crates/` is what broke `chapbook-cli` once already.
  - ~~**1.8b** Delete or fix `src/plugins/mod.rs`.~~ **Done — archived, not
    deleted.** Moved to `docs/archive/plugins-mod.rs`. It imported `wasmtime`,
    which is in neither manifest nor lock, and survived only because
    `// mod plugins;` was commented out in `main.rs`: dead code sitting in the
    build tree looking live. Archived because WASM plugins are Part 2 and the
    design in it is worth keeping somewhere obvious; the commented `mod` line
    is gone since it now points at nothing.
  - ~~**1.8c** Rename one of `src/epub_write.rs` / `src/epub_writer.rs`.~~
    **Done — `epub_write.rs` → `epub_metadata.rs`.** It writes metadata back
    into an existing EPUB; `epub_writer.rs` builds new ones. This one was
    renamed rather than the other because the new name says what the file
    does, and all 17 call sites share one mechanical `crate::epub_write::`
    prefix. 15 of the module's own tests confirmed still running by name.
  - ~~**1.8d** Install the icon at the right size.~~ **Done.**
    `assets/logo-512.png` generated from the 2048 px master and the `Makefile`
    installs that. `logo.png` stays at 128 **deliberately**: it is also
    embedded in the About dialog via `include_bytes!` (`app.rs:1503`), where
    512 would be wasted bytes in the binary.
  - **1.8e — Deliberately not done, 2026-09-19.** Make "Open with Kalam" work:
    add `MimeType` to the `.desktop` file, add `%f` to `Exec`, parse `argv` in
    `main.rs`. None of the three exist. **Owner's decision: skip it.** The
    owner works from yazi rather than double-clicking, and the entry point is
    therefore `Enter` → `xdg-open` (yazi's own default Linux opener is
    `xdg-open "$1"`) → MIME lookup → the `.desktop` file → `Exec` → `argv`.
    Same chain, so the fix would still have applied — it is simply not worth
    doing for this workflow.
    **Recorded because it is larger than those three bullet points.**
    `Catalog::open()` runs at `main.rs:137`, *before* `app.run()` at `:179`,
    and Kalam has no single-instance guard of its own. If GTK's application-ID
    registration gives single-instance semantics — **unchecked against
    relm4's source** — then a second launch opened the database, handed off to
    the running instance, and exited, dropping the file. Reaching an
    already-open window means handling GTK's file-open signal inside the
    application lifecycle, not reading `argv` in `main()`.
    **Owner's stated preference for whenever this is revisited:** do not import
    the book, just open it — but reading history and progress should still be
    recorded. **That collides with the schema as it stands:**
    `reading_events.book_id` is `INTEGER NOT NULL REFERENCES books(id) ON
    DELETE CASCADE` (`db.rs:692`), so no history can exist for a book with no
    row in `books`. Two ways out, both worth deciding before building:
    - Make `book_id` nullable — a schema bump from `SCHEMA_VERSION` 14, and
      there is precedent: the v14 dictionary-lookup table made `book_id`
      nullable for exactly this reason, because sidebar searches log lookups
      not tied to a book.
    - Create a lightweight row in `books` pointing at the original file rather
      than a copied library file. History, progress and quotes then work
      unchanged, but the grid has to decide whether to show it.
  - ~~**1.8f** Add a `LICENSE` file at the repository root.~~ **Done, but not
    as written.** The item assumed `Cargo.toml` said `GPL-3.0-or-later` and
    the file was merely missing. Checking found **two** declarations: the app
    said GPL-3.0-or-later while the seven `chapbook-*` engine crates said MIT
    OR Apache-2.0. **Owner's decision: a personal project with no license at
    all**, which under copyright default means all rights reserved. So all 12
    license lines across 11 manifests were removed and **no** `LICENSE` file
    added. The dependency direction was checked first and is clean — GPL code
    depends on permissive code, never the reverse — so the old arrangement was
    legally coherent, just accidental.
  - **1.8g → moved to its own item, see 1.16.** The count is 85, not 90, with
    5 module-wide. Working through them means deciding for each whether the
    code should be deleted, wired up, or genuinely kept — a real pass, not a
    few minutes. Split out rather than rushed alongside the mechanical fixes.
  - ~~**1.8h** Rename `src/pages/reader/js_bridge.rs`.~~ **Done —
    `dictionary_popover.rs`.** The file's own doc comment already explained
    that the WebKit bridge was gone and what remained is the dictionary query
    and the popover. Both `use` sites updated.
  - ~~**1.8i** Fix the doc comment in `src/timing.rs`, which still describes
    handing chapters to WebKit.~~ **Done.**
- ~~**1.9 — Write the app-level guardrails.**~~ **Done, 2026-09-19.**
  `crates/kalam-reader` inherits strict boundaries from upstream, which is a
  large part of why that code stayed clean; `src/` had no equivalent, and what
  did exist was prose in `docs/WORKING.md`. Built as **three ratchet tests** in
  `tests/guardrails.rs` (a ratchet asserts a violation count never *rises*),
  per the recommendation in `docs/conversation.md` §21.
  | Rule | Ratchet |
  |---|---|
  | No `unwrap()`/`expect()` in production code | **12** |
  | Pages hold `Arc<Catalog>` rather than asking `LibraryService` | **58** occurrences, 22 files |
  | Literal hex colours in `resources/style.css` | **184** |
  - **Every number was measured fresh.** §21's table (2026-09-18) is wrong in
    all three: it said 6 unwraps "all in `downloads.rs`" — that file was
    rewritten and now has none in production, and the real 12 sit mostly in
    `perf.rs` (4, a headless harness) and `db/dictionaries.rs` (3); it said
    "many" for `Arc<Catalog>`; and it said "unknown, small" for hex colours
    when the stylesheet has 184 and `theme.rs` duplicates 169 of them. That
    duplication is the real finding and is not fixed by a ratchet, only
    stopped from growing.
  - **The first measurement was itself wrong and was caught before shipping.**
    Cutting each file at its first `#[cfg(test)]` both over-counted `perf.rs`,
    whose tests are top-level `#[test]` functions rather than a module, and
    silently skipped most of `db.rs`, which applies `#[cfg(test)]` to a single
    function at `:524`. The scanner now walks brace depth and suppresses
    exactly the annotated item.
  - **A second scanner bug survived to CI and was caught by the scanner's own
    test.** A single-line item (`pub fn helper() { x.unwrap(); }`) opens and
    closes on one line, so suppression was set and cleared before the line was
    counted. The ratchet number is unchanged at 12 — no real file has that
    shape — so the bug was latent, and would have started lying the day someone
    wrote a one-line test helper above production code. **This is the direct
    payoff of testing the scanner rather than trusting it.**
  - `docs/WORKING.md` gains the anti-bloat rules §21 recommended, and its §1
    now says plainly that the no-`unwrap` rule is broken in 12 places and
    enforced as a ratchet, rather than stating an absolute nobody checks.
  - **Env-var ceiling recorded as five `KALAM_*` switches** (plus `RUST_LOG`),
    with a note that it already slipped by one since that count was first
    written down — which is the argument for writing the ceiling down at all.
- ~~**1.10 — Decide the upstream relationship.**~~ **Decided 2026-09-19: Kalam
  no longer tracks upstream.** `docs/kalam/UPSTREAM.md` described cherry-picking
  fixes monthly from `ophymx/chapbook`, but the engine crates are ordinary
  workspace members that Kalam edits in place.
  **The evidence for stopping, measured rather than assumed.** Kalam has
  modified **74 inherited files** — the engine's own library and database layer
  removed, one file deleted outright, a theme system added that upstream does
  not have. `UPSTREAM.md` itself already singled out
  `chapbook-reader/src/open.rs` as "the file most likely to conflict on a
  cherry-pick." Against that cost, the routine **ran exactly once** (2026-09-10,
  range `ab14cb7..7ace24a`, PRs #32–#36, 42 files) and took **nothing**:
  upstream was working on phone and Windows shells, FFI/JNI, OPDS bindings and
  Swift/.NET surfaces. Real upkeep, zero yield.
  **What changed.** `UPSTREAM.md` rewritten from a 198-line monthly procedure
  into a 136-line provenance note. The 74-row table of inherited-file edits is
  **kept**, reframed: it no longer predicts merge conflicts, but it does record
  how this engine differs from the one imported, which is worth having when
  debugging. Nothing is lost — the upstream history was merged with
  `--allow-unrelated-histories`, so a specific fix can still be found and
  brought over by hand; there is just no standing obligation to look.
  **Four other documents stated the routine as still live and were corrected:**
  `PLAN.md` (its companion-doc list, the §3 premise, and step 7 "Monthly"),
  `docs/kalam/WORKING.md` (the pending-decision bullet, which also pointed at an
  "Upstream relationship" item in `docs/offline-roadmap.md` that does not exist),
  and `docs/kalam/RESTRICTIONS.md`. That last one is worth noting: it told
  readers to keep dead format variants byte-identical *so cherry-picks stay
  cheap*, which is no longer a reason. It now says they could be deleted, but
  as part of **1.16** rather than as a drive-by, since removing them means
  touching the `#[cfg]` sites that reference them.
  `docs/kalam/RESEARCH.md` was deliberately **left alone** — it is a dated log
  of what was believed when each decision was made, and rewriting it would make
  it less useful.
  **Owner's other decision:** the five changes marked "candidate to send
  upstream" — real bugs in the original project, one being a reading setting
  the author persisted but never read — stay recorded in `UPSTREAM.md` rather
  than becoming issues or pull requests.
- ~~**1.11 — Correct the documents that describe an app that no longer
  exists.**~~ **Done, 2026-09-19.** These were not cosmetic: a README that
  overstates what is finished is how this project came to believe the downloads
  hub was shipped when it is not.
  - ~~**README claims P6 and P7 were "built together".**~~ **Corrected.** The
    precise position, verified rather than assumed: there is **no**
    `impl Source` anywhere and `SourceManager::new()` builds an empty `Vec`, so
    the browse route wired to `source_id: "ao3"` resolves to nothing. But the
    plumbing is genuinely real — a job queue with progress and cancellation
    (`src/downloads.rs`, made properly asynchronous in 1.3), a downloads page,
    and a live route. Saying "does not exist" would have been as wrong as
    saying "built"; the row now says the machinery exists and the content
    sources do not. Part 2.
  - ~~**README claims "P8 / P9 Comics and manga — next".**~~ **Corrected.**
    Comics shipped — 1,685 lines across `src/comics.rs` and
    `src/pages/comics_reader/` — and the separate P9 manga track was folded
    into it. The "What works now" list also called comics a placeholder in the
    same document that is otherwise accurate about the reader, so both places
    were fixed.
  - **README's WebKit claims were already correct and were left alone.** The
    item said the status table "describes a WebKit app throughout", but
    checking found four WebKit mentions, all of them accurate — the reader
    bullet says "no WebKit, no JavaScript", and the build section explains that
    WebKitGTK has not been a dependency since the swap. **The roadmap entry was
    stale, not the README.** Worth recording because the reflex would have been
    to "fix" text that was right.
  - ~~**`docs/conversation.md` §20 recommends Poppler while §22 chose
    MuPDF.**~~ **Corrected by annotation, not rewriting.** §20's reasoning was
    sound on the criterion it was given ("lightweight") and §22 changed the
    criterion to rendering quality. A superseded note points at §22 and says
    why; the original argument stays, because deleting it would hide that the
    decision was a change of criterion rather than a correction.
  - **Also fixed, found while in there:** the repository has **two** files
    named `WORKING.md` — `docs/WORKING.md` (62 lines, the invariants and the
    1.9 ratchets) and `docs/kalam/WORKING.md` (321 lines, the agent-facing
    guide). They do not overlap in content, but the names collide and it is
    easy to edit the wrong one; 1.9 added rules to the shorter of the two.
    Each now opens with a pointer to the other.
- ~~**1.12 — Attribute the unmeasured part of cold start.**~~ **Done, 2026-09-19** —
  every millisecond now has an owner. *Added 2026-09-19
  from the 1.2a run, which measured this by accident.* The log reports
  **`window_shown` = 9,166.6 ms** — nine seconds from process start to the
  window appearing. The three startup spans account for only **2,410 ms** of
  it: `startup_db_open` **2,114.9**, `startup_first_page` 294.4,
  `startup_dicts` 0.6. **6,757 ms — 74% of the wait — is not attributed to
  anything.**
  This is the worst number in the whole log and the one the user feels first,
  and there is no way to fix it while it is invisible. Candidates, none of
  them yet confirmed: GTK and Adwaita initialisation, `icons::init()`, the
  library registry checks, `theme::current` (an uninstrumented database read)
  and `theme::apply`, all the widget building in `AppModel::init` outside the
  `startup_first_page` span, and GTK's first realize — which compiles shaders
  on Intel integrated graphics and can cost seconds on its own.
  The work is to instrument those, not to fix them: spans around each block in
  `main()` plus a `pre_run` marker, which splits the gap into "before
  `app.run`" and "inside `AppModel::init` and realize" in one run. Fixes follow
  in Phase 7 once the log says where the time is.
  Separately worth noting from the same log: `grid_build` took **260.2 ms** to
  build 48 of 150 book cards on All Books, against 7.1 ms for 2,000 cards in
  the benchmark. That discrepancy is Phase 7's problem, not this item's.

  **Second run, 2026-09-19 — the `main()` half is now fully attributed, and the
  instrumentation checked itself:** the six spans sum to 5,063.4 ms against a
  `pre_run` marker of 5,063.7 ms, so **0.3 ms** of everything before `app.run`
  is unaccounted. On a 10,883.9 ms cold start:

  | Where | ms | Share |
  |---|---|---|
  | **inside `app.run`, unattributed** | **5,289.6** | **48.6%** |
  | `startup_db_open` | 2,049.0 | 18.8% |
  | `startup_gtk_init` | 1,826.1 | 16.8% |
  | `startup_icons` | 859.4 | 7.9% |
  | `startup_first_page` | 527.4 | 4.8% |
  | `startup_theme` | 145.1 | 1.3% |
  | `startup_libraries` | 119.3 | 1.1% |
  | `startup_style` | 64.5 | 0.6% |
  | `startup_dicts` | 0.7 | — |

  Two things stand out. **`startup_icons` is 859.4 ms for four tiny SVG files**
  that are not even rewritten after the first run — so the cost is in the GTK
  calls, and `IconTheme::add_search_path` (which rescans the theme) is the
  likely one. **`startup_db_open` is 2.0 s** and is now the largest single
  *named* cost; `Catalog::open` runs `migrate()` on every start.
  Neither is fixed yet, because neither is confirmed at the line level.

  **Remaining: the 5,289.6 ms inside `app.run`**, which is 48.6% of the whole
  start and still belongs to nothing. `AppModel::init` is 306 lines building
  the whole widget tree, and after it GTK realizes the window. An
  `init_done` marker now sits at the end of `init`, so the next run splits that
  into "building the widgets" and "GTK creating the surface and compiling
  shaders" — the latter can genuinely cost seconds on Intel integrated
  graphics, and telling those apart is the difference between something we can
  fix and something we cannot.

  **Third round, 2026-09-19 — item complete, and the headline number was
  wrong.** The owner ran it four times back to back and the first was an
  outlier: **the 8.9–10.9 s figures were a cold OS page cache on a spinning
  HDD, not a code defect.** Three warm runs agree within 14 ms:

  | | warm 1 | warm 2 | warm 3 |
  |---|---|---|---|
  | `window_shown` | 1,357.8 | 1,366.6 | 1,372.1 |
  | `init_done` | 1,246.0 | 1,255.0 | 1,262.2 |
  | `pre_run` | 682.0 | 690.8 | 687.9 |

  Warm start is **~1.36 s**, and it is now fully accounted for — the six
  `main()` spans sum to within **0.3 ms** of `pre_run` on every run:

  | Where | ms | Share | Fixable? |
  |---|---|---|---|
  | `AppModel::init` (widget tree) | 564–574 | 41% | yes — needs one more round of spans |
  | **`icons_theme`** | **493–505** | **36%** | **yes — see 1.13** |
  | GTK realize | 110–112 | 8% | probably not |
  | `startup_gtk_init` | 101 | 7% | no |
  | `startup_style` | 50–57 | 4% | no |
  | `startup_theme` | 25–27 | 2% | no |
  | `startup_db_open` | **8.9–9.5** | <1% | nothing to fix |
  | `startup_first_page` | 7.9–8.6 | <1% | nothing to fix |
  | `startup_libraries` | 0.2 | — | — |

  **Two of the earlier alarms were cold-cache artifacts and are withdrawn.**
  `startup_db_open` is 2,049–2,311 ms cold and **9 ms warm** — `migrate()` is
  not slow, reading the file from a cold disk is. `startup_first_page` is
  527 ms cold and 8 ms warm. The cold figures are still worth knowing (first
  launch after a reboot really does take ~9 s) but there is no code to change.

  **`icons_write` measures 0.1 ms**, which confirms the 859/496 ms was never
  about writing four small SVGs. It is the GTK icon-theme rescan, as suspected.

  **Done when:** `KALAM_TIMING=1` on the Arch machine accounts for essentially
  all of the gap between `timing::start()` and `window_shown`. **Met** — 0.3 ms
  unaccounted before `app.run`, and `init_done` splits the rest.
- ~~**1.13 — Take the 496 ms icon-theme rescan off the startup path.**~~ **Done,
  2026-09-19 — warm start is 44% faster.** Follows
  directly from 1.12, which measured it at **493–505 ms warm — 36% of the whole
  start** — with `icons_write` at 0.1 ms proving the cost is GTK's icon theme,
  not the files. `IconTheme::add_search_path` makes GTK rescan every icon
  theme on the system to pick up four small SVGs.
  Two findings make this safe to move:
  - **Two of the four icons are dead.** `kalam-dictionary-symbolic` and
    `kalam-copy-symbolic` have **zero** references anywhere outside `icons.rs`;
    they are written to disk on every machine and never drawn. The other two
    are used only by the reader's selection toolbar
    (`src/pages/reader/engine.rs:312` and `:347`).
  - **Nothing at startup needs them.** No icon is drawn before the reader is
    opened, so the rescan is pure wait on the critical path.
  The fix: make `icons::init()` idempotent behind a `OnceLock`, schedule it as
  an idle callback once the window is up, and have the toolbar builder call it
  too. The idle callback runs within a few milliseconds of the window being
  drawn — long before anyone can open a book and select text — so in practice
  the cost moves off the path entirely; and if it somehow has not run, the
  toolbar forces it and behaviour is exactly what it is today. Delete the two
  dead icons while in there.
  **Done when:** `window_shown` drops by roughly 490 ms warm, and the highlight
  and quote buttons still show their own icons rather than a fallback glyph.

  **First result, 2026-09-19 (cold run — warm confirmation still pending).**
  The ordering is right: `icons_write` and `icons_theme` now print **after**
  `window_shown`, so the rescan is off the pre-paint path as designed. And the
  effect was larger than predicted:

  | | before | after |
  |---|---|---|
  | `icons_theme` | 788.0 cold / 493–505 warm | **5.9** |
  | `icons_write` | 0.1 | 22.3 |

  **`icons_theme` fell from 493–505 ms to 5.9 ms — it did not merely move, it
  became cheap.** The likely reason, offered as an inference rather than
  something measured: called during startup, `add_search_path` arrived before
  GTK had loaded any icon theme, so it triggered a full load of every theme on
  the system; called after the window is up, the theme is already loaded and
  adding a path is a cheap invalidation. If that is right, the 496 ms was never
  inherent to having two custom icons — it was the cost of doing it early.
  `window_shown` on this run was 9,910.6 ms, but that is **cold** and cold runs
  have ranged 8,871–10,884 ms, so it supports no conclusion.

  **Warm confirmation, 2026-09-19 — four runs, and the owner confirmed the
  highlight and quote buttons still show their own icons.**

  | | before (3 runs) | after (4 runs) | change |
  |---|---|---|---|
  | `pre_run` — all of `main()` | 686.9 | 224.6 | **−462.2** |
  | `AppModel::init` | 567.5 | 424.9 | **−142.6** |
  | GTK realize | 111.1 | 112.5 | +1.4 |
  | **`window_shown`** | **1,365.5** | **762.1** | **−603.4 (44% faster)** |

  Per-run `window_shown` after: 718.1, 719.8, 762.8, 847.6 ms — spread 129.5 ms,
  wider than the 14 ms before, and `startup_gtk_init` alone ranged 100–191 ms
  across those runs, so the extra variance looks like GTK initialisation
  rather than anything this change did.
  **The 142.6 ms saved inside `AppModel::init` was not predicted and is not
  explained.** The plausible reading, offered as an inference: `add_search_path`
  invalidates the icon theme, so every `from_icon_name` during widget building
  afterwards had to resolve against a theme marked dirty; with the call moved
  out, those lookups hit a clean cache. Not measured — if `AppModel::init` is
  ever optimised, this is worth confirming rather than assuming.

  **Renderer test, 2026-09-19 — `GSK_RENDERER=cairo` rejected.** Half the cold
  penalty was suspected to be GTK compiling shaders, so the software renderer
  was measured against the GL one, three runs each:

  | | run 1 | run 2 | run 3 |
  |---|---|---|---|
  | Cairo | 727.3 | 612.9 | 612.9 |
  | GL (default) | **3,302.9** | 710.4 | 711.6 |

  **Steady state: Cairo ~613 ms against GL's ~711 ms.** The ranges overlap and
  the gap is ~100 ms, which is not worth giving up GPU acceleration for — so
  the renderer stays GL. **Rejected, not deferred.**
  The more useful number is GL's *first* run at **3,302.9 ms** against 711 ms
  after: roughly **2.6 s of one-time graphics setup** that later runs do not
  pay. That is consistent with the shader hypothesis, offered as an inference,
  and it matters for **1.15** — whatever warms the cache at login should warm
  the graphics stack too, not just the files.
  Also worth recording: the earlier single Cairo reading of 4,447.3 ms was not
  comparable to anything. It was one run, of unknown cache state, and drawing
  a conclusion from it would have been the same mistake as the 2 s database
  open.

- **1.14 — Give the background work a visible task manager.** *Added
  2026-09-19.* The machinery was already there — **28 `tasks::spawn` call
  sites**, not the 27 this entry claimed — but the registry stored only an id
  and a cancel flag, so there was nothing to look at. Entries now carry a
  **label**, the latest progress, and whether the work failed; there is a
  bounded list of the last 20 finished tasks; and `spawn`/`spawn_stream` take a
  label as their first argument, so all 28 call sites name the operation in
  plain words. The three that act on a named thing use it (`Downloading
  {title}`, `Remastering {title}`). The API is `cancel(id)`, `tasks()`,
  `recent()`, `clear_finished()`, `Reporter::fail()`.
  **Two halves, and the first one shipped alone.** The page came first
  (`src/pages/task_manager.rs`) and was marked done on the strength of a green
  CI run. The owner then found it was **two clicks away** — My library → Tasks —
  and that importing a sub-5 MB EPUB finishes in a fraction of a second, so
  there is no way to be watching the page when it happens. A task manager you
  have to navigate to is not a task manager. **This bullet stays open until the
  second half is in**, which is why the reachability work was folded back into
  it rather than tracked as its own number: splitting it is what let the first
  half be called finished.
  The second half:
  1. **A sidebar button** in `bottom_nav`, above Settings, following the shape
     of the hidden `download_indicator`.
  2. **Its face is the badge** — a tick when idle, the running count when busy,
     a warning when something recent failed. One slot, three states, no
     overlay. The warning matters because the toast that reports a failure is
     gone in seconds; `Reporter::fail()` is how a worker says so, and only
     tasks that can tell use it — import and dictionary install so far, which
     is a gap rather than a lie.
  3. **`w` toggles a floating dialog** showing only what is running, with
     progress and a cancel button — narrower than the page on purpose.
     Clicking the sidebar button opens the **full page**.
  **`w` is scoped away from the reader** by doing nothing at all: the reader
  binds `w` to its Words tab, its key controller runs first because key events
  travel up from the focused widget, and it returns `Stop`. The owner chose to
  leave that binding alone rather than move a key they already use. **The
  consequence is recorded, not hidden: the reader hides the sidebar, so while
  reading there is no way to reach the task manager at all.**
  **Pause was asked about and is not being built.** Cancellation here is
  cooperative — a worker checks a flag between units of work. A pause flag is
  the same mechanism, but these tasks are mid-HTTP-request or holding a
  database handle when you would press it, and blocking there means holding a
  socket or a lock indefinitely. Possible, and a bad idea.
  **A progress bar is drawn only when the task reported a total** — an empty
  bar reads as "stuck", which is worse than no bar. Fast queries from 1.2b are
  added and removed between ticks and never appear; this page is for work you
  wait on.
  **Open defect, not yet explained.** The owner imported books and nothing
  appeared. The registry is proven sound by a test that drives a real `spawn`
  end to end, and the page is proven to render by the headless-sway smoke run,
  so the break is in the link between them. The page's refresh timer was
  discarding its `SourceId`; that is fixed, but this entry does not claim the
  bug is fixed with it. The badge and the dialog both read the registry off
  the app-wide tick, so if the page is still wrong the badge will say so from
  the home screen.
  **Done when:** the badge is visible from anywhere outside the reader and is
  right within a second of a task starting or finishing; `w` opens and closes
  the dialog; clicking the button opens the page; and an import actually shows
  up somewhere the owner can see it.
- ~~**1.15 — Warm the file cache at login so the first launch is a warm one.**~~
  **Done, 2026-09-20.** *Added 2026-09-19, owner-approved.* Cold start is ~9.9 s against ~762 ms
  warm, and the whole difference is reading from a spinning disk — no code
  change makes that faster. But the owner's own runs prove the fix: cold
  10,884 ms, then 762 ms on the very next launch with nothing recompiled.
  Something that reads Kalam's binary, its shared libraries and the catalog
  database into the page cache shortly after login turns the first click into
  a warm one. It does not reduce the work; it moves it to a moment nobody is
  waiting through.
  **Test before automating:** run the warm-up by hand after a reboot, before
  opening Kalam, and confirm the launch is ~760 ms. Only then make it a
  systemd user unit or autostart entry.
  **Honest caveat to record now:** with 4 GB of RAM, opening a browser first
  may evict what was just warmed, so this will not hold every time. It is a
  mitigation, not a fix — the fix is faster storage.
  **Done when:** a launch immediately after a reboot measures close to the
  warm figure, and it survives a reboot without the owner doing anything.
  **2026-09-20:** `scripts/warm-cache.sh` written — warms the binary, its
  `ldd` dependencies, the GTK/graphics stack (resolved through `ldconfig`,
  not hardcoded paths), `catalog.db`, and the fontconfig cache; `--covers`
  additionally warms the thumbnails. Uses `vmtouch` when present, `cat` to
  /dev/null otherwise, so it has no hard dependency. The manual test comes
  first per this bullet: run it after a reboot, launch with `KALAM_TIMING=1`,
  and confirm `window_shown` is near ~760 ms. The systemd user unit is the
  step *after* that holds, not before.
  **Manual test passed, 2026-09-20.** Owner rebooted, ran `warm-cache.sh`,
  launched with nothing else open: **`window_shown` = 1136 ms**, against the
  ~9,900 ms cold start — the gap is gone. `pre_run` (676.8 ms) is now fully
  attributed (its five spans sum to 676.6 ms), the largest being
  `startup_db_open` at 291.3 ms; the rest of the launch is `startup_first_page`
  232.5 ms and ~119 ms of GTK realize. The 1136 ms is above the older ~762 ms
  warm figure because the catalogue has grown (`db_open` is now the biggest
  single span), not because the warm-up fell short. `scripts/kalam-warm.service`
  is the systemd user unit that runs the script at login, so it survives a
  reboot without the owner doing anything — the second half of "done when".
  **Service enabled and confirmed by the owner** (`systemctl --user status
  kalam-warm.service`: `enabled`, ran at login, `status=0/SUCCESS`, "warmed
  185 files"). `inactive (dead)` after the run is the correct state for a
  `Type=oneshot` unit.

- ~~**1.16 — Work through the `#[allow(dead_code)]` suppressions.**~~ **Done, 2026-09-20.** *Split out
  of 1.8g, 2026-09-19.* **85** of them, **5** module-wide (`#!`), which each
  hide an entire module rather than one item.
  **Why it is its own item and not a few minutes.** CI runs clippy with
  `-D warnings` specifically so dead code fails the build, and every one of
  these switches that check off locally. Removing a suppression is not a
  deletion — it forces a decision about the code underneath, and the three
  answers are different work: delete it, wire it up because it was meant to be
  used, or keep it and say why. Some will turn out to be genuinely
  forward-looking (the engine's suspend/release hooks); some will be leftovers
  from the WebKit removal, like `js_bridge` was.
  **Order:** the 5 module-wide ones first, since each of those hides the most,
  then the rest. Expect real deletions to follow, which is the point.
  **Done when:** the count is materially lower, every survivor has a written
  reason rather than a bare attribute, and clippy's dead-code check is
  actually running over the code again.
  **Progress, 2026-09-20:** all **4** module-wide `#![allow(dead_code)]` are
  gone (the census at start was 4, not 5 — one had already been resolved).
  Un-hiding them exposed only three dead items: `pronunciation.rs` and the
  reading `engine.rs` proved fully used. Deleted `dict::strip_dict_html`
  (leftover HTML stripper) and `shelf_rules::MatchMode::label` (no caller).
  `dict::import_dictionary` turned out to be test-only, so it is now
  `#[cfg(test)]` rather than dead-on-bin. Module-wide count: 4 → 0.
  **Batch 2, 2026-09-20 (item-level):** item count 80 → 72. Deleted
  zero-caller fns `db::count_books`, `dictionaries::insert_dict_entry`,
  `pdf::calculate_ink_box` (helpers verified used, no cascade). Removed a
  *stale* allow on `comics::extract_comic_cover` (it is called by `epub.rs`).
  Made test-only fns `#[cfg(test)]`: `count_quotes`, `dict_entry_count`,
  `forget_overrides`, `set_reading_list_note`. The remaining ~72 are mostly
  data-model struct fields (keep-with-reason) plus a few forward-looking
  clusters (author saved-quotes, reader engine, sources) for later passes.
  **Batch 3, 2026-09-20:** de-staled `sources::all()` (used by `browse`).
  Removing the allows on `Floating` and `SavedQuotesOut` exposed never-read
  *payloads* inside otherwise-used enums — a lesson recorded: an enum being
  used does not mean every variant field is read. Resolved with reasoned
  field-level allows: `Floating`'s controllers are stored purely to keep the
  float components alive, and `JumpTo.chapter_index` is part of the output
  contract whose reader-scroll is not yet wired. Count now ~72; the remainder
  is the data-model field sweep.
  **Completed, 2026-09-20.** The field sweep finished: every surviving
  `#[allow(dead_code)]` in the tree now carries a written reason. Tallies:
  module-wide 4 → 0; deleted dead items `count_books`, `insert_dict_entry`,
  `calculate_ink_box`, `strip_dict_html`, `MatchMode::label`,
  `DictSearchResult`; test-only fns moved to `#[cfg(test)]`
  (`import_dictionary`, `count_quotes`, `dict_entry_count`,
  `forget_overrides`, `set_reading_list_note`); stale allows removed where the
  item is actually used (`extract_comic_cover`, `sources::all`, and the
  enum-level ones re-scoped to reasoned field allows). Clippy's dead-code
  check runs over every module again.
- ~~**1.18 — The comic upscaler offers itself to books it cannot work on.**~~
  **Done, 2026-09-19.** Found by the owner: the Remaster button was drawn
  unconditionally in `book.rs` and `book_float.rs`, so it appeared on EPUBs and
  PDFs too. The guard existed but sat at the *click* — pressing it on an EPUB
  produced a "Not a comic" toast. A control that can only fail should not be
  drawn. Visibility now follows the format, in both the page and the float,
  including when the book has been removed underneath you. The toast guard
  stays as a backstop.
- **1.19 — Bulk delete from the all-books selection.** *Added and done
  2026-09-19, owner-directed.* Selection mode already let you pick many books
  and bulk-*edit* them, but offered no way to remove several at once — the
  obvious pairing was simply missing. A Delete button (danger-styled) now joins
  the selection bar. It confirms first, because it is the one selection action
  that cannot be undone, then deletes on a worker via `tasks::spawn` — so the
  job shows up in the task manager with progress and is cancellable, and the
  grid cannot freeze while sixty books' data dirs are removed. Files are left
  where they are; the library row, reading history and thumbnail go, matching
  the single-book delete. The owner then asked for *every* delete to be a
  task, so the single-book delete (book page) now runs on the same seam too —
  it is far too fast to cancel, but it belongs in the history like any other
  operation, and the page no longer blocks on removing the directory.

**Done when:** no UI thread blocks on SQLite; background work reports progress
and can be cancelled; the app writes a log file that survives a `.desktop`
launch; the damage list exists and is either empty or fully accounted for in a
later phase; `cargo build` is clean of dead dependencies; and the README's
status table survives a line-by-line check against the code.

---

### Phase 2 — Reading quality

> Visible. The reader is the part you touch most, and it is where the engine
> swap did the most damage.

**Goal:** make daily reading complete again, and finish the dictionary.

**Work:**

The order below is deliberate. The dictionary comes first because it is the
oldest thing that is broken and the one that turns a daily action — looking up
a word — into a dead end. Bubbles come last: they are the largest item, and
they need 1.2's asynchronous service layer underneath them.

- **2.1 — Dictionary: arrow-key sense-walk.** The lookup itself works — select a
  word, press `d`, and `LookUpSelection` at `src/pages/reader/mod.rs:1517`
  opens the card. What is gone is moving between meanings with ↑/↓ and pressing
  Enter to save the one you are on. There is no `k-def-focus` and no
  focused-sense concept anywhere in `src/`; this is new GTK work.
- **2.2 — Dictionary: rebuild the tests.** The old 55-check jsdom harness is gone and
  cannot be rebuilt as JavaScript. Rewrite the equivalent coverage as Rust
  tests against the engine.
  - **Done 2026-09-21:** dictionary coverage now lives in Rust — `src/db/dictionaries.rs`
    (search, lemmatization, phrase lookup, pos parsing), `src/db/pronunciation.rs`
    (IPA), plus engine-side `DictCard::from_entry` tests in
    `src/pages/reader/engine.rs` (pos middot join, sense mapping + hint index,
    syn/ant/idiom/saved passthrough). All pass in CI.
- **2.3 — Font picker.** The engine already supports it and already knows what
  to offer: `ReadingSettings::font_family` takes a family,
  `Session::set_font_family` applies one (`chapbook-reader/src/nav.rs:115`),
  and `Session::font_families` returns the installed faces to offer
  (`chapbook-reader/src/lib.rs:672`, covered by tests in `session.rs` and
  `settings.rs`). **None of it is referenced anywhere in `src/`** — the
  capability exists and no control reaches it. Add the picker to the settings
  panel.
  - **Done 2026-09-21:** a "Typeface" dropdown now sits in the reader's Type
    settings, listing the engine's installed faces with a "Default" row. The
    choice persists to `reader.font_family`, rides `KalamPrefs.font_family`
    into the engine, and applies live (blank → bundled default).
- **2.4 — Text layout toggles.** Three engine settings with no user-facing
  switch, each verified by grep against `src/`:
  - **Justify.** `ReadingSettings::justify` exists but is never set. The nine
    `justify` hits in `src/` are unrelated to it: eight are GTK label
    alignment (`set_justify`) and one is an icon name.
  - **Hyphenation.** The engine hyphenates (`chapbook-layout/src/hyphenate.rs`)
    with no way to turn it off.
  - **Publisher styles.** `ReadingSettings::publisher_styles` exists, zero
    references in `src/`. The underlying engine bug was fixed upstream, so
    this is only the missing switch.
  - **Column width** is already exposed; these three join it.
  - **Done 2026-09-21:** a "Text" section in the reader settings now carries
    all three switches. `justify` and `publisher_styles` ride
    `ReadingSettings` straight into the engine; hyphenation has no engine
    field, so turning it off appends `hyphens: manual !important` to the skin
    CSS, which stops the layout's soft-hyphen pass
    (`chapbook-layout/src/boxtree.rs:245` only hyphenates when
    `frag.hyphens_auto`). They persist to `reader.justify` (off by default),
    `reader.hyphenate` and `reader.publisher_styles` (on by default) and apply
    live.
- **2.5 — Footnotes.** Clicking `[1]` opens an instant popover or jumps to the note.
  The engine resolves internal links; nothing in `src/` handles them.
  - **Done 2026-09-21:** tapping a footnote reference shows the note where the
    reader already is. `Session::peek_link` resolves an internal href and
    returns the note's text without moving (`chapbook-layout`'s new
    `extract_text_at` gives one subtree); `looks_like_note` keeps TOC entries
    and cross-references navigating as before — a note is an `li`/`aside`/`dd`
    or a short `p`/`div`, a section or heading is a destination. The shell
    offers the note through `View::connect_note` and answers with a card in
    the dictionary card's styling plus a "Go to the note" button, which uses
    the new `View::follow_link`.
- **2.6 — "Jump back" history.** An instant return after jumping to a footnote, TOC
  entry or search match. **The engine already keeps the trail**:
  `Session::back` / `can_go_back` (`chapbook-reader/src/nav.rs`) pop a
  64-deep stack that `jump` pushes on. What is missing is only a control —
  `View::follow_link` landed with 2.5 and is the shape to copy.
  - **Done 2026-09-21:** `Session::back_depth` exposes the trail's length, and
    the reader watches it grow on each position report — which catches jumps
    the engine made on its own, like a tap on a link in the page, that no
    message would otherwise announce. A third button appears in the dock that
    already holds Library and Bookmark, and Backspace does the same. The offer
    withdraws on the next ordinary page turn, so it reads as "undo that jump"
    rather than a permanent history menu.
- **2.7 — Dual-page toggle, plus continuous vertical scroll.** The engine already
  shows facing pages above 900 px (`view.rs:911`) — **automatically, with no
  way to switch it off.** Add the toggle rather than the behaviour.
  - **Done 2026-09-21:** the spread was four separate copies of
    `width > 900` across `kalam-reader/src/view.rs`; they are now one
    predicate, `spread_at()`, which also consults `View::dual_page`. The
    switch is "Two pages side by side" in the Layout section, persisted to
    `reader.dual_page` and **on by default**, so nobody's reading changes
    until they ask for it to. Continuous vertical scroll — the other half of
    this item — already shipped earlier as `reader.scrolled`.
- **2.8 — Custom fonts folder** — drop `.ttf`/`.otf` in without a system install.
  - **Done 2026-09-21:** `~/.local/share/kalam/fonts`, scanned recursively
    when a book opens (`ReaderOptions::fonts_dir` → `Faces::Dir`) and offered
    by the 2.3 typeface picker. It sits under the shared root rather than per
    library, for the reason `dictionaries_dir` gives: a typeface belongs to
    the installation. The folder loads *after* the bundled faces, so a name
    Literata or Noto Sans already answers keeps the face that shipped, and
    anything dropped in simply joins the picker. Settings → Reading → Type
    has a "Fonts folder" row with an Open button, and says plainly that new
    faces appear on the next book open — faces are read at open, not per
    page turn.
- **2.9 — Auto-hiding mouse cursor** after 2s, and configurable keybindings and
  mouse-wheel sensitivity.
  - **Cursor and wheel done 2026-09-21:** the pointer gets out of the way two
    seconds after it last moved, and any movement brings it back — a movement
    re-arms one timer rather than stacking them, and leaving the page cancels
    it. On by default, switchable in Settings → Reading → Pointer. Scroll
    speed is a setting (`reader.wheel_step`, 20–400 px per notch) instead of
    the compile-time constant it was; paged mode is untouched, because the
    wheel never turned pages there.
  - **Keybindings done 2026-09-21:** twelve of Kalam's actions — look up,
    next/previous chapter, larger/smaller text, contents, settings,
    highlights, saved words, bookmark, jump back, search — live in a table
    (`src/pages/reader/keybinds.rs`) that Settings → Reading → Keyboard
    edits: click a key, press the one you want, Escape cancels, and "Reset
    to defaults" puts them all back. A key is held by the name the toolkit
    gives it (`d`, `plus`, `BackSpace`), which round-trips through
    `reader.key.<action>` and makes Shift+D the same key as d. One action
    per key: binding a key another action holds leaves that action
    *unbound* — recorded, showing "None", never firing — because dropping
    it from the table let the shipped default sneak straight back. `=`
    still makes text larger as the unshifted twin of `+`, but an exact
    claim on `=` wins, since the twin is only a fallback. `m` duplicated
    `b` for bookmarks and is gone. The engine's own keys (arrows, space,
    Page Up/Down) stay fixed: they are the universal reading keys and live
    in a different table in a different crate.
- **2.10 — Selection toolbar.** Double-click selects a word, triple-click a paragraph.
  The teardrop handles exist (`crates/kalam-reader/src/handles.rs`). The
  toolbar needs highlight in five colours plus underline, quote, dictionary
  lookup and copy.
- ~~**2.11 — Rebuild PDF reader from scratch.**~~ **Done, 2026-09-24.** With user
  approval, completely tore down the old heuristic `lopdf` viewer in `src/pdf.rs`
  and `src/pages/pdf_reader.rs`. MuPDF (`mupdf` crate v0.8 with `base14-fonts`)
  is now the native rasterization engine, compiling cleanly on CI. The PDF reader
  chrome is fully unified with Kalam's EPUB reader design:
  - Top edge hover reveals floating back button pill ("Library") without title text to prevent overlaps. Autohides cleanly after 2.5s with SlideDown transition.
  - Bottom edge hover reveals floating bottom pill with page numbers, prev/next, zoom, view modes, and smart crop. Autohides reliably after 2.5s with SlideUp transition; center-tap toggles controls cleanly without getting stuck.
  - EPUB reader back button and bottom pill also upgraded to smooth Revealer `SlideDown` and `SlideUp` transitions.
  - Touchpad pinch-to-zoom gesture (`gtk::GestureZoom`) and Ctrl+Scroll for fluid zooming.
  - Continuous vertical scroll mode is viewer default and persists across restarts (`reader.pdf.continuous`). Pre-renders the active starting page synchronously (~10ms) on open so the viewer displays the page immediately with zero blank placeholder flicker.
  - Background rendering invalidates viewport snapshot caches with `queue_draw` and `queue_resize`, eliminating delayed rendering.
  - Restores continuous vertical scroll offset accurately when resuming past page 1.
  - Enabled MuPDF `system-fonts` feature backed by `font-kit` and fontconfig so non-embedded PDF fonts resolve cleanly without glyph truncation or substitution mismatch.
  - Smart crop auto-detects background paper luminance with a 5.5% (min 44px) breathing margin to protect thin serifs, headers, and footnotes.
  - Left edge hover (Zen Browser style 20px edge) smoothly slides in the left TOC outlines sidebar, auto-closing on exit with 350ms debounce.
  - Left sidebar header matches EPUB reader (`kalam-reader-book-head`) featuring book cover thumbnail, serif title, author, progress bar, and direct panel scroll list (removed artificial header and TOC bottom bar button).
  - Arrow key scrolling in continuous mode obeys user-configured arrow scroll speed (`reader.arrow_step`).
  - Generous bounded memory caching (active page ± 8 pages, ~85-100 MB) with ± 4 page prefetching for smooth rapid scrolling.
- **2.12 — Bubbles.** Several books open at once as stacked circles over any screen in
  the app; tap one to read it in a floating window. Full design, including what
  has to be measured first, is in
  [Bubbles](#bubbles--multiple-books-open-at-once). This is the largest single
  item in the phase. The measurement it was gated on is **done** (1.5); what it
  still needs is 1.2's asynchronous service layer, which is why it sits last.
  Per-book cache budgets come with it — `set_cache_budget` already exists, only
  the call is missing.

**Round 4 (2026-09-22) — settings recategorization and arrow scroll speed.**
Field report: T1 typeface dropdown and keyboard arrow navigation confirmed
working on device ("done. looks fine"). In response to user feedback,
the left sidebar's Reading settings have been recategorized from 7
fragmented sections into 5 clean, logically sorted groups: Theme (palette
dots), Typography (Typeface dropdown, Fonts folder, Font size, Line
height, Justify, Publisher styles), Layout (Column width, Continuous
scroll, Two pages), Navigation & Scrolling (Wheel scroll speed, Arrow
scroll speed, Pointer autohide), and Dictionary (Sense hint, Lookup
history). The new `reader.arrow_step` preference (range 10–200, step 5,
default 45) scales both single-tap arrow steps and the 20 ms continuous
glide speed; in paged mode it is greyed out (since vertical arrows step
chapters there), mirroring the existing continuous-mode greying of
two-page spreads.

**Round 3 (2026-09-22) — shipped, awaiting the report.** The typeface
dropdown's fourth fix is the first one aimed at the right wall: the
Settings panel hangs in the **left** sidebar (`left_stack`), and the three
earlier holds all guarded the **right** sidebar's close timer;
`ForceCloseLeft` now mirrors the right side's popover-tree hold. Arrow
keys gained the reader's spec: scrolled mode — vertical arrows scroll and
a held key glides on a 20 ms timer; horizontal arrows step chapters.
Paged mode — vertical arrows step chapters, horizontal keep turning
pages. A backward step from mid-chapter first rewinds to that chapter's
start, and an opposite arrow immediately after a step is answered by the
session trail ("take me back"); any navigation in between clears that
memo. The PDF rebuild from scratch is queued next.

**Field report, round 2 (2026-09-21, evening).** Hyphenation is **retired
outright** — the reader's call ("I really don't like it"): toggle, pref,
message and model field all gone; the engine keeps the capability, the app
never asks. Two round-1 fixes didn't take: the sidebar-vs-dropdown hold
(first try watched window `is_active`, which a popup doesn't reliably flip —
replaced with a walk for a visible `gtk::Popover` inside the sidebar, which
works on every platform), and hide-cursor-on-scroll (touchpads drip
sub-pixel motions through a two-finger gesture and every motion re-showed
the pointer — a 160 ms grace after each scroll event absorbs the drip).
Smooth scroll steps are also capped at 240 px per event, because a high
speed setting made a two-finger flick teleport whole screens. The PDF
import and the rest of round 2 passed.

**Field report, round 1 (2026-09-21).** The whole 2.x batch went through its
first real run on the device; the report and every fix are itemized in
`docs/manual-test-checklist.md` ("Round 2"). Caught only there: the importer
never accepted `.pdf` at all (fixed — a PDF now imports like a comic, page
one as cover); the hyphenation toggle never *asked* for hyphenation, so it
showed nothing in either position (fixed — "on" claims `hyphens: auto`, and
a test pins it); touchpad smooth-scroll ignored the scroll-speed setting
(fixed — surface deltas scale by step/default); jump-back's Backspace never
arrived through the focus-dependent capture path (fixed — window-global
shortcut, typing-guarded, and the on-screen undo button retired as clutter);
the settings sidebar's leave-timer killed dropdown popups mid-pick (fixed —
hold while a popup owns the session); the two-page toggle now greys in
continuous scroll; the fonts folder copies its path to the clipboard when no
file manager exists; keyboard shortcuts moved to their own settings tab.

**What already survived.** The dictionary overhaul shipped ten phases under
WebKit; most of it is SQLite and is intact — the headword index, WordNet
lemmatization, phrase decomposition, the merged store, the popup UI and part-of-
speech dividers, the Lesk likely-sense hint, pronunciation, vocabulary tools,
priority reorder and lookup history. Only the JavaScript-driven interaction
layer died, and find-in-chapter came across intact. So this is one rebuild and
one test rewrite, not a redo.

**Deliberately not here:** tap-a-word-to-open-dictionary. Removed on
2026-09-18 and the engine API deleted so it cannot return by accident.

**Done when:** a full read of a real EPUB — with a footnote, a looked-up word
saved to vocabulary using the keyboard, a font changed from the picker, and a
jump back from the TOC — needs no workaround.

---

### Phase 3 — Ingestion

> Mostly invisible, but the result is visible: fewer broken books.

**Goal:** books imported into Kalam arrive clean.

**Why after Phase 1:** the sanitizer runs at import time and must not freeze
the interface, so it needs the async service layer.

**Work:**

- **The EPUB ingestion sanitizer.** On import: unzip, strip toxic hardcoded
  CSS (forced 8px fonts, fixed margins, forced colour overrides), repair broken
  XML, generate a TOC from `<h1>`/`<h2>` when the manifest lacks one,
  pre-extract the cover, repack. `extract_zip` is already hardened against zip
  attacks with four named tests, so the entry point is safe.
- **Folder-watch auto-import.** A background watcher on a configured folder
  (`~/Downloads/Books`) that silently imports and sanitizes new files.

**Done when:** dropping a messy real-world EPUB into the watch folder produces
a book that opens correctly, with a cover and a table of contents, without
touching the interface.

---

### Phase 4 — Search

> The largest single new subsystem in Part 1.

**Goal:** find things across the whole library, and within a book.

**Why after Phase 3:** indexing every book takes minutes and must run in the
background, and it should index *clean* books — otherwise the index has to be
rebuilt after Phase 3 changes them.

**Work:**

- **Library-wide full-text search** with `tantivy`. 10–20 ms queries across
  the whole library for character names, quotes or themes. `tantivy` is not
  currently a dependency.
- **Extend in-book search.** Find-in-chapter already works —
  `ReaderView::search` is wired at `src/pages/reader/mod.rs:1161` behind
  Ctrl+F. This adds live match counting and next/previous across the whole
  book.

**Done when:** a search for a character's name returns the right books in
under 100 ms on a library of a few thousand books, and rebuilding the index
does not block the interface.

---

### Phase 5 — Library management

> Visible. Browsing and organising.

**Goal:** the library page can answer questions about your books.

**Work:**

- **Inline metadata editing** on the book page: click Title/Author/series, a
  `gtk::Stack` swaps in an entry, Enter saves. A permanent `[ + ]` pill on the
  tag row for quick adds.
- **Metadata editor and fetcher hub.** Full-screen route with Open Library and
  Google Books, side-by-side edition and cover comparison before applying, and
  every field: title, authors, series, series index, publisher, date, language,
  ISBN, synopsis, tags. The fetchers exist in `src/metadata/`; the hub does
  not.
- **Author pages** with biography, personal notes, and books grouped by series
  and release date.
- **Series cover stacks** in the grid — Book 1 with a stacked-paper effect,
  expanding to the full reading order.
- **Manual and smart shelves.** Smart shelves are dynamic queries pinned to the
  sidebar; `src/shelf_rules.rs` already has the rule engine.
- **Reading statuses**: Currently Reading, Want to Read, Finished, and
  **Abandoned** — the last does not exist yet, and matters because abandoned
  books should stop counting toward streaks.
- **Ratings and private reviews.** 5 stars plus a Markdown notes editor.
- **Bulk editing.** Multi-select to batch-assign tags, authors, shelves or
  status. Needs Phase 1's async layer or it will freeze on a large selection.
- **Duplicate finder** by hash or title/author.

**Done when:** you can find, fix and organise a messy 500-book library without
leaving the app or editing files by hand.

---

### Phase 6 — The EPUB editor

> The biggest item in Part 1, and the reason everything above comes first.

**Goal:** edit EPUBs without another application.

**Why last of the engine work:** it depends on Phase 1 (async), Phase 2
(engine completeness) and Phase 3 (the sanitizer, since editing a toxic file
means fighting the publisher's CSS).

**Work:**

- **Sidecar patches.** Typo fixes and notes are saved as non-destructive
  replacement rules in the database and `kalam.json`; the `.epub` on disk is
  untouched. An explicit **"Apply patches to EPUB"** writes them into the
  archive and re-verifies the container.
- **`[Fix Typo]`** inline popover that saves a sidecar patch without
  interrupting reading.
- **Proofreading edit mode.** A pencil toggle in the reader chrome for
  one-click inline paragraph editing with hover highlights.
- **Full EPUB editor.** A dedicated full-screen route: an Obsidian-like
  WYSIWYG surface presenting HTML as clean Markdown-style formatting; a raw
  HTML/CSS mode with syntax highlighting; a file tree and asset manager for
  chapters, stylesheets, fonts and images; and a visual TOC editor to rename,
  nest, reorder, split and merge chapters.
- **Quote-anchored locators.** Already the engine's model (`LayeredLocator` in
  `crates/chapbook-core`), so annotations re-anchor to their text regardless of
  reflow, font size or window size. Verify it holds after edits.

**Done when:** you can fix a typo mid-paragraph, restructure a bad table of
contents, and produce a clean `.epub` that opens correctly in another reader.

---

### Phase 7 — Performance budgets

**Goal:** keep it fast as the app grows.

**Why here:** there is a point in measuring only once the features exist to
measure. The ratchet is already in place — `src/perf.rs` has six query budgets
that gate CI, and they are not `#[ignore]`d.

**Work:**

Numbered from 2026-09-19, when 1.12 and 1.13 handed this phase two measured
findings. Nothing here has been started, so numbering the pre-existing items
breaks no reference.

- **7.1 — Fix the remaining bottlenecks**, notably opening latency on floating
  book cards and detail views.
- **7.2 — Floating window host.** Book cards, quick notes and dictionary popups
  float above the active view without reloading the page or leaking memory.
- **7.3 — Extend the perf budgets** to the new subsystems from Phases 3–6.
- **7.4 — Defer the chapter character count.** Opening a book walks every chapter to
  count its characters, so a locator can become a progress percentage —
  measured at 30–147 ms depending on the book, paid before the first page
  draws. It is only needed when something asks for a percentage, so it could be
  built on first use instead. Low value on its own; it belongs here rather than
  in a phase of its own.
- **7.5 — Break down `AppModel::init`.** *From 1.13, 2026-09-19.* After the icon
  rescan moved off the startup path, warm start is **762 ms** and
  `AppModel::init` is **424.9 ms of it — 56%**. It is 306 lines building the
  whole widget tree in one go, and nothing inside it is measured yet, so the
  first step is spans around its major blocks, not optimisation. Everything
  else in a warm start is now small: `startup_gtk_init` 136.5 ms (not ours),
  GTK realize 112.5 ms (probably not fixable), `startup_style` 52.5 ms,
  `startup_db_open` 9.0 ms, `startup_first_page` 8.0 ms.
  Note the unexplained 142.6 ms that 1.13 removed from this function as a side
  effect — recorded there, and worth confirming rather than assuming if this is
  ever touched.
- **7.6 — Explain the `grid_build` discrepancy.** *From 1.12, 2026-09-19.*
  `grid_build` measured **260.2 ms** to build 48 of 150 book cards on All
  Books, while the benchmark builds 2,000 cards in 7.1 ms. Either the benchmark
  is not measuring what the app does, or the app is doing 35× more work per
  card than it should. Worth resolving before trusting either number.

**Done when:** a 2,000-book library opens in under a second, scrolling never
drops a frame on a mid-range machine, and every subsystem added in Phases 3–6
has a budget.

**Reference point:** windowing the book grid took peak memory from 502 MB to
247 MB at 2,000 books. Building every card at once cost 352 MB and 139.7 ms
per card. See `ci-logs/scale-2000-comparison.txt`.

---

### Phase 8 — Material 3 redesign

**Deliberately last.** Every phase above adds screens, so restyling now means
restyling again later. Home, Library, Details, Reader, Settings and the author
hub, once.

---

## Bubbles — multiple books open at once

**A vital part of Part 1.** Engine work in Phase 1, the bubbles themselves in
Phase 2. This section is the design, recorded so it does not evaporate the way
the original entry did.

The only earlier record of this was one line in `docs/conversation.md`:
*"Bubble Memory: Single WebKit process SPA for multiple open books. In-app
`gtk::Overlay` floating chat head outside reader."* Half of that is dead — there
is no WebKit process any more. The other half is what this section expands.

### What it is

Android-style bubbles. Open several books and they sit as **stacked circles on
top of whatever you are doing** — library, settings, analytics, anywhere in the
app. Tap one and it expands into a floating reading window while the others line
up along the top. You can open a book *straight into* a bubble from the library,
and you can minimize the book you are reading *back into* a bubble.

In-app only. A separate always-on-top window would fight a tiling compositor,
and `ARCH.md` records that Kalam already hit exactly that problem with dialogs —
which is why the in-app float layer exists at all.

### Two readers, and the full one loses nothing

This is the constraint that keeps the feature buildable.

| | **Full reader** | **Bubble reader** |
|---|---|---|
| Where | The page, as today | A floating window |
| Left sidebar | TOC **and Settings** | TOC only |
| Right sidebar | **Highlights, Bookmarks, Words** | none |
| Floating pills | **all of them** — selection chip, highlight colours, dictionary popup, quote/copy | none |
| Annotations, search, bookmarks | **all there** | none |
| Status | **unchanged; nothing is removed** | new, thin |

The full reader is 5,845 lines across 12 files with five sidebar tabs. The
bubble reader keeps one of those five.

Worth being honest about *why* that helps, because the obvious reason is not the
real one. **It is not a memory saving** — memory lives in the engine's book
object (fonts, parsed book, layout caches), not in the sidebars, which are
ordinary widgets over database rows. The saving is complexity: a small floating
window cannot fit the full chrome anyway, and a bubble reader that wanted the
full feature set would mean maintaining 5,845 lines twice, forever. Instead the
reading surface becomes one component with two shells around it.

### The memory design

The engine already has the machinery, in `crates/chapbook-reader/src/cache.rs`:

| Engine call | What it does |
|---|---|
| `Session::suspend()` | Drops every cache except the page on screen; the session stays usable |
| `Session::release_caches()` | The same, callable directly |
| `Session::set_cache_budget(bytes)` | A memory cap, changeable at runtime, evicting immediately |
| `Session::cache_bytes()` | What is actually held |

**Which budget is in force is worth being precise about, because there are
two.** The *engine's* `DEFAULT_CACHE_BUDGET` is 192 MB, sized for comics on a
phone, and its comment records that before the budget existed the answer was
*"everything, forever"* — measured at **676 MB after forty comic pages**. But
`kalam-reader` overrides it with its own 32 MB, chosen explicitly for a machine
with 4 GB in total, and that is what Kalam actually runs at. Confirmed by log
rather than by reading: `opening ... with a 32 MB cache budget`.

**Per-book budgets are the one piece of this that does not exist yet.** There is
one budget for one open book. Bubbles need it to vary by state — the book being
read gets the full 32 MB, a warm one gets a couple, a bubble gets nothing
because it holds no book. `set_cache_budget` already does the work; what is
missing is Kalam calling it as books change state. That is a Phase 2 task, and
it is small.

So three states, and only one of them is expensive:

| State | Holds | Cost | How many |
|---|---|---|---|
| **Bubble** | Book id, cover thumbnail, position — all already database rows | Kilobytes | As many as you like |
| **Warm** | Book opened, budget trimmed, suspended | The fixed open cost | One or two |
| **Reading** | Full cache budget, drawing | Up to its budget | One |

**The key rule: a bubble holds no book.** It is a bookmark, not a reader.

### Measured — and the engine needs no change

Measured 2026-09-19 on the owner's Arch machine, `RUST_LOG=info`, four books.
Every one reported **8 font faces**.

| Book | Open | of which font scan | Chapters | Char count | First frame |
|---|---|---|---|---|---|
| Wonder | 78 ms | **3 ms** | 140 | 117 ms | 174 ms |
| Nyxia | 55 ms | **5 ms** | 50 | 140 ms | 183 ms |
| Immortals of Meluha | 32 ms | **1 ms** | 41 | 32 ms | 53 ms |
| The Dragonet Prophecy | 70 ms | **1 ms** | 14 | 61 ms | 123 ms |

**The font scan is 1–5 ms.** The candidate fix — sharing one font system across
sessions — was expected to be the centrepiece. **It is not worth building.**
It would save a few milliseconds and cost real complexity.

What the numbers actually say:

- **Opening a book is cheap: 32–78 ms.** Adding the once-per-book chapter
  character count (`host_position`, 32–140 ms, cached for the life of the
  session) puts a book ready-to-read at roughly **100–220 ms**. Fast enough to
  do on demand, slow enough that a bubble must not hold an open book.
  **That confirms the design rule rather than changing it.**
- **First paint is 53–183 ms.** That is the latency a tap on a bubble would
  show. Acceptable; not instant.
- **Chapter layout varies enormously: 4 ms to 603 ms.** The slow cases are
  image-heavy — one chapter laid out 141 pages with 194 images in 547 ms,
  another took 603 ms with 487 images. Text-only chapters are 4–10 ms.
- **Memory is the real constraint, not time.** Observed single-chapter cache
  sizes: **34 MB, 37 MB, 13 MB, 12 MB**. The default budget is 192 MB *per
  session*, so several warm books is several hundred megabytes. Bubbles must
  call `set_cache_budget` down hard on every book that is not being read, and
  `suspend()` the rest.

So the engine work reduces to: **use the cache controls that already exist, and
add nothing.** Phase 1 keeps no engine item for bubbles.

### Also found: a warning that buried the log

`xml5ever` warns, once per document parsed, that it does not implement
`stop_parsing` for XML5 — **hundreds of times across a single book.** It is a
third-party crate (0.39.0), the statement is true, and nothing is wrong. But it
drowned the timings the logger exists to show.

Held to `error` in `src/logging.rs`. `RUST_LOG=info,xml5ever=warn` brings it
back; a suppression that cannot be lifted would be a trap.

### Recorded decisions

- Bubbles live **inside the Kalam window**, not as OS windows.
- One expanded at a time, others lined up — Android's behaviour.
- Bubble face: **the book cover with a thin progress ring** around it.
- The bubble reader is reading surface plus a **TOC-only sidebar**.
- The full reader **keeps every feature it has today**.

---

## Deferred — PDF

**You said PDFs are rare, so this is not on the critical path.** Recorded so it
is not lost.

The PDF import reader is further from done than it looks. `src/pdf.rs` has no page
rasterizer — only `lopdf`, which reads text. `render_page_image_uncropped`
looks for a picture embedded in the page: **scanned PDFs work**, and smart
crop trims them correctly. With no embedded picture it falls back to
`render_text_to_canvas`, which draws **a dark grey bar for each line of text** —
the code comment says so directly. Page mode is the default
(`reflow_mode: false` at `src/pages/pdf_reader.rs:89`), so **opening an
ordinary text PDF shows a page of grey stripes.**

Two options, in order of cost:

1. ~~**Cheap:** default to the existing reflow mode when a page has no embedded
   picture, and say so on screen. A few hours. Makes text PDFs readable.~~
   **Moved into Part 1 on 2026-09-19 — it is item 2.11.** Reason: PDF bubbles
   are wanted, and they cannot exist while text PDFs render as grey stripes.
   This was the tension between "PDFs are rare" and "PDFs get bubbles", and
   the resolution is to pay the few hours and leave real rendering deferred.
2. **Real:** add a rendering library. **The decision was MuPDF** — 8.7 ms/page
   against Poppler's 14.6, and it won the one rigorous eight-engine fidelity
   study. The `mupdf` crate compiles the library from vendored source with
   `default-features = false` and `base14-fonts`. Reasoning in `docs/conversation.md` §22.
   **Shipped on 2026-09-24:** completely tore down the heuristic `lopdf` viewer in `src/pdf.rs`
   and `src/pages/pdf_reader.rs`. Rebuilt the PDF reader from scratch with native MuPDF
   rasterization, unified reader chrome matching EPUB (floating back chip, floating bottom pill
   with page scrub and zoom, autohiding controls, slide-in TOC outline sidebar), continuous vertical
   scroll and paged modes, and bounded background memory caching (<80 MB).

**Comics and manga** are a separate track and are already shipped (1,685
lines). Not deferred. Wishlist items from the old plan, if ever: automatic
double-page spread detection, a magnifying loupe, contrast/sharpness tuning
for faded scans, and a remaster tool for upscaling low-resolution scans.

---

## Decisions still needed

Raised, deliberately not decided, and **not** dropped.

- ~~**Multiple books open at once**~~ **Resolved — item 2.12.** Raised
  2026-09-04, designed 2026-09-19 as the bubbles; see
  [Bubbles](#bubbles--multiple-books-open-at-once). The reading-time question
  it raised is still open and travels with 2.12: two open books must not both
  count reading time.
- **Chapter-level → page-level cache eviction.** *A tripwire, not a task.*
  Eviction drops whole chapters, so a 141-page chapter is held as one lump
  whether you are looking at page 1 or page 141. That was a real problem when
  such a chapter cached 34 MB against a 32 MB budget; after images started
  decoding to display size the same chapter is **13 MB and fits on its own**.
  **Do not build this until a single chapter again exceeds the budget.** To
  check: `RUST_LOG=info kalam`, open an illustrated book, and look for a
  `laid out unit N (... KB)` line above 32,768.
- **Upstream relationship** — see Phase 1. Needs an answer before Phase 6.
- **Toolchain pinning.** `.github/workflows/ci.yml` uses
  `dtolnay/rust-toolchain@stable` unpinned, so a new Rust release can break a
  build that was green yesterday. Pinning costs flexibility; not pinning costs
  predictability.

---
## Part 2 — online (stub)

Everything networked lives here and **nothing in Part 1 may depend on it.**

- **Plugin substrate.** The decision is **WebAssembly**, superseding the Lua
  design still described in `ARCH.md`'s Source-seam section (that section is
  banner-marked as set aside). Three conditions make it worth doing: put
  `wasmtime` behind a Cargo feature that is off by default; do not ship the host
  before a *second* plugin exists; keep a plain TOML selector config alongside,
  because most scraper breakage is a changed CSS selector and a selector fix
  that needs a wasm rebuild is not a fast fix loop. Reasoning in
  [`docs/conversation.md`](./docs/conversation.md) §22.
- **Online sources.** AO3, MangaDex, Royal Road, Literotica, FFN. The old
  design and its evidence are in the archive; the `Source` trait has never been
  implemented, so treat it as unbuilt.
- **Downloads hub.** The pages exist and are unreachable. When this starts, use
  `src/tasks.rs` — `src/downloads.rs` currently uses bare `thread::spawn` and six
  `lock().unwrap()` calls, which is the cascade-panic that `src/tasks.rs:52`
  exists to prevent.
- **Literotica account sync** — raised, never decided.

---

## Lessons worth keeping

The detail is in [`docs/archive/roadmap-phases-p0-p12.md`](./docs/archive/roadmap-phases-p0-p12.md)
and [`docs/pitfalls.md`](./docs/pitfalls.md). Four that change how you work:

- **A0 step 6 — three confident estimates, all wrong.** Grid virtualization was
  ruled out on measurements taken from a test harness that never actually
  reached the page it was measuring. Every estimate reasoned about Home-page
  numbers. The tell — `grid_build` absent from every report — was visible each
  time. **Check what the measurement actually measured.**
- **A check that has never failed is not known to work.** Verify by sabotage:
  break the thing on purpose and confirm the check goes red.
- **When you fix the thing a check was watching, re-derive what the check
  proves.** Changing a mechanism and carrying the old oracle across gives you a
  test that passes on broken code, or fails on working code.
- **Answering a question the owner did not ask.** When an answer settles one
  variable, change only that variable. If a second decision seems to follow, say
  so and ask.

---

## Schema

`SCHEMA_VERSION` = **14** (`src/db.rs`). Migrations run on open and are
additive; there is no downgrade path, so copy
`~/.local/share/kalam/catalog.db` before testing a build that bumps it.
Full table list in the archive.

---

## CI

| Check | When | Blocks? |
|---|---|---|
| `cargo fmt --all` | every push | no — report only, publishes the diff |
| `cargo clippy --workspace --all-targets -- -D warnings` | every push | **yes** |
| `cargo test --workspace --all-targets` | every push | **yes** |
| `cargo build` and `cargo build --release` | every push | **yes** |
| GUI smoke test (headless sway, opens a book) | every push | no — `continue-on-error` |
| Visual judgement | your Arch machine, phase end | you |

**`--workspace` is load-bearing.** Without it Cargo selects only the root
package, because the workspace has 11 members and no `default-members`. Details
and the design reasoning are in [`docs/ci/README.md`](./docs/ci/README.md).

---

## Changelog of plan decisions

Append a dated row for every phase shipped, every architectural decision locked,
and every scope change. Newest rows go at the bottom, so the history reads
top-to-bottom like a journal.

| Date | Event |
|------|-------|
| 2026-07-24 | Name: Kalam; Linux only; Relm4+GTK4; no Z-Library |
| 2026-07-24 | EPUB engine: WebKit; chapter-wise scroll |
| 2026-07-24 | Custom UI; slim sidebar IA |
| 2026-07-24 | Shelves: grid → detail → book page or float |
| 2026-07-24 | CI compile; Arch at phase boundaries |
| 2026-07-24 | Roadmap P0–P11 |
| 2026-07-25 | P0 signed off; P1 library core |
| 2026-07-25 | Cover cards: 1.6:1 cover, title/author below; click=float, Ctrl+click=page |
| 2026-07-25 | Float: Suwayomi-style panel; later no open/close morph animations |
| 2026-07-26 | P2 EPUB reader; restyle to tablet-book floating chrome |
| 2026-07-26 | Two readers locked: text vs comics |
| 2026-07-26 | Text reader look: sepia default, no blue body/links, no underlines |
| 2026-07-26 | Comics reader look: Moku-like black stage, top meta + bottom scrub (P8) |
| 2026-07-26 | P0–P2 treated complete; **next = P3** |
| 2026-07-26 | P3 annotations & dictionary shipped: highlights (5 colors), quotes, offline dict packs (StarDict/SQLite/TSV), Saved quotes/words real data, export Markdown, annotations list, dictionary popup, Settings import |
| 2026-07-27 | P5 shipped: metadata editor, cover replacement, Open Library lookup (staged for review, never auto-applied) |
| 2026-07-27 | Ratings (half-star), yearly reading goals and quote notes added from the reference designs; social elements deliberately skipped |
| 2026-07-27 | P4 shipped: shelves engine (manual + flat-rule smart shelves), reading list, event-log history, reading-time sessions, tags browse, analytics with streaks |
| 2026-07-27 | Smart shelves locked as flat rules + All/Any; nested groups deferred and kept JSON-compatible |
| 2026-07-28 | Performance pass shipped: query batching + stats memoisation, page cache, imports off the UI thread |
| 2026-07-28 | Hardening pass shipped: toast notifications, `VACUUM INTO` backup, reader-cache pruning, poison-safe locks, `db.rs` split |
| 2026-07-28 | Metadata overrides keyed on `file_hash` (schema v7) so edits and covers survive delete → re-import |
| 2026-07-28 | EPUB writeback fixed for single-line OPFs; toast accent restyled to the reference; every user action now confirms |
| 2026-07-28 | P5.5 opened: UI overhaul, one window at a time, mockup before code. Colour system + 13 dark themes + slim sidebar shipped |
| 2026-07-28 | P5.5 Settings window redesigned: 220px 6-tab navigation rail + rounded cards layout |
| 2026-08-26 | P5.5 button hierarchy + chip + serif-title classes adopted from user style pass (`d545d28`) |
| 2026-08-26 | Settings v2 shipped: grouped nav, section cards, family theme picker, pill switches, export card — mockup-first, CI green |
| 2026-08-30 | Reader-improvements track recorded: annotation workflow and hybrid anchoring are complete; recoloring existing highlights is shipped, with text search next, followed by dictionary improvements and only later reader architecture changes |
| 2026-08-31 | Reader annotation search shipped across saved highlight text and notes; color/type filters remain available, and annotation design polish is deferred until feature work is complete |
| 2026-08-31 | Dictionary lookup keeps the selected phrase intact and normalizes surrounding punctuation plus common simple inflections |
| 2026-08-31 | Dictionary popup displays up to five matching results with separate save/copy actions; the clearly licensed English WordNet 2025 starter pack is bundled and enabled on first run |
| 2026-08-31 | Bundled the separate English Idioms and Expressions pack (1,024 phrase-to-meaning entries) with its upstream Unlicense notice, source revision, checksum, and first-run removal marker |
| 2026-08-31 | Dictionary overhaul plan extended with deferred Phase 5.5 POS grouping and transparent, optional Lesk sense hints; all matched senses remain visible |
| 2026-08-31 | Temporary selection handles now support pointer/touch dragging without changing native selection or saving annotations implicitly; triple-click rendering remains deferred |
| 2026-08-31 | Fresh text selections now paint their custom selection bands live during mouse/touch drag; the toolbar and handles still wait for release |
| 2026-09-01 | Dictionary overhaul Phase 1 shipped: precomputed `fold_key` headword index (schema v11). Exact/prefix lookups use `idx_dict_entries_key`; `Run`/`run`/`rún` all resolve to `run`; existing databases backfill in place with no reimport |
| 2026-09-01 | Dictionary overhaul Phase 2 shipped: real lemmatization from the bundled Princeton WordNet 3.0 exception lists (`noun.exc`/`verb.exc`/`adj.exc`/`adv.exc`, gzipped, with NOTICE + checksums). `went→go`, `mice→mouse`, `better→good`, `running→run` resolve to headwords; suffix rules remain the fallback |
| 2026-09-01 | Fixed dormant test failures/warnings found by the first real `cargo test` run: `series_key` now strips leading series articles (The/A/An) so article variants share one series-cache key, while author names are never article-stripped; the series ordering test helper now stores the series index it was passed, making the indexed-vs-unindexed ordering assertion real instead of vacuous |
| 2026-09-01 | Fixed the flaky cover-override tests: all three cover tests seeded the same book title, so in parallel `cargo test` runs they raced on the same real filesystem paths (shared stashed cover `covers/hash-A.png` and book dir). Each test now seeds a unique title, isolating its uuid/hash paths |
| 2026-09-01 | Dictionary overhaul Phase 3 shipped: `search_phrase` in the catalog (full phrase → longest contained phrase headword via sliding window → per-token breakdown with lemmatization), `PhraseLookup` enum, and the reader's dict-lookup bridge routes multi-token queries through it. `odd mixture` now yields cards for `odd` and `mixture`; `run out of steam today` resolves to the `run out of steam` entry |
| 2026-09-01 | Phase 3 follow-up: the sidebar Words search box still used the single-word path, so phrases typed there dead-ended with "No matches". All lookup paths now share one phrase-aware pipeline (`lookup_dict`) — sidebar search and selection popup behave identically |
| 2026-09-01 | Two more bundled English packs, on by default: **English Synonyms (WordNet 3.0)** (110,365 words, synset companions) and **English Antonyms (WordNet 3.0)** (6,621 antonym pairs), both derived from the already-licensed Princeton WordNet 3.0 data (NOTICE + checksums beside the packs). New packs auto-install on next launch via their own first-run prefs, so existing installs gain them without re-import |
| 2026-09-01 | Dictionary overhaul Phase 4 shipped (user-designed): merged dictionary store. One word = one entry, from the highest-priority dictionary that has it (WordNet 10, Idioms 20, Synonyms 30, Antonyms 40, imports 100) — priority wins even when another dictionary has more senses, and the losing dictionaries' copies are never shown. Schema v12: `dictionaries.priority` + `combined_words` (one row per headword key, senses as deduped JSON). Auto-rebuilt on import/remove/bundled install and once at migration. The old Phase 4/5 plan of per-dictionary tabs is obsolete and removed |
| 2026-09-01 | Dictionary overhaul Phase 5 shipped: popup redesign — the fake "1 RESULT / No definition / Total dict entries" state is gone, one clean entry per word with numbered senses, phrase misses render clickable breakdown chips, imported HTML definitions render formatted instead of flattened, a single action bar (Save/Copy/Search-in-book) replaces per-result button pairs, and the popup stays dark on every paper theme | 
| 2026-09-01 | Dictionary overhaul Phase 5.5 shipped (partial): Lesk "likely here" hint — `likely_sense_index` (pure fn, unit-tested: stopword-filtered overlap, headword excluded, tiny stemmer) marks the best-matching sense with an accent pill, the bridge now sends the full sentence context, a `dict_sense_hint` pref toggles the hint only, and senses are never hidden. The header POS pill shipped earlier; **POS grouping dividers remain deferred** |
| 2026-09-01 | Dictionary overhaul Phase 6 shipped: tap-to-look-up (240 ms tap delay, caret-word resolution, sentence context) + popup keyboard (↑/↓ focus ring with wrap, Enter saves the focused sense; ←/→ deliberately unbound) + **Find in chapter** (`kalamSearchInBook`, temporary accent hits, count toast, Esc clears). jsdom harness at `docs/files/test_kalam_dict_preview.js` grew to 55 checks |
| 2026-09-01 | Dictionary overhaul Phase 7 shipped: vocabulary review — schema v13 `saved_words.known`, Saved Words page gains All/To review/Known filter + mark-known check buttons, and exports **CSV** (`~/SavedWords.csv`, RFC-4180) and **Anki TSV** (`~/SavedWords-Anki.txt`, `#separator:tab`). The dictionary overhaul is complete |
| 2026-09-01 | Backend review pass (focused + full sweep) shipped: dict importers hardened — SQLite packs are opened read-only (never modify the pack, no `-wal`/`-journal` sidecars), pack table/column identifiers are double-quoted against crafted names, failed imports roll back the dictionary row instead of leaving a half-import, and importing Kalam's own `catalog.db` as a pack is rejected via canonical path comparison. Fixed a latent bug: `list_reading_list` read `progress/rating/publisher` as `position/note/added_at` (column offset vs the 16-column `BOOK_COLUMNS`). 9 new unit tests; the rest of the sweep (shelves, history, metadata, stats, authors, series, annotations, prefs, pronunciation, `shelf_rules.rs`, schema/FKs) found no defects |
| 2026-09-01 | CI workflow gains a `cargo test` step: compiles and runs all unit tests headless (in-memory SQLite), publishes failures to `ci-logs/test-latest.txt` and fails the run. The full workflow is staged at `docs/ci/github-actions-ci.yml` — the Arena App cannot push `.github/workflows/` changes, so the user installs it manually with their own account |
| 2026-09-01 | The first real `cargo test` run on CI (user installed the workflow, `bc6473f`) caught exactly one failure: `quote_ident_escapes_embedded_quotes` — my test's expected string had one extra escaped quote (three quotes after the word instead of the correct two that SQLite identifier quoting produces). Function correct, test literal wrong; fixed (`1aae8f1`). All 154 unit tests now pass on every push |
| 2026-09-01 | Dictionary track Phases 8–10 recorded in the roadmap (user-authored): Phase 8 — POS grouping dividers (finishes P5.5, rendering-only, flat-index invariant for hint + keyboard nav); Phase 9 — Settings dictionary priority reorder UI; Phase 10 — lookup history (optional, schema v14, logs misses, privacy toggle + clear ship with the feature). Verified against the code before recording: `priority` is write-only today (missing from `Dictionary` and `list_dictionaries()`, which also orders by name — plumbing required); import/removal already call `rebuild_combined_dictionary()`; `lookup_dict` (reader.rs:1880) is the single lookup funnel to log in; `pos` already reaches the popup per-sense. Track numbering is local, distinct from global P8/P9/P10 (comics/PDF) |
| 2026-09-01 | Dictionary track Phase 8 shipped: POS grouping dividers in the dictionary popup (`be11c07`) — senses grouped under noun/verb/adjective/adverb dividers (remaining POS first-appearance order; unlabelled senses last with no divider). Rendering-only over the flat senses array: `Sense.number`, the Lesk hint index and the ↑/↓ keyboard walk keep indexing the flat list; a group straddling the "Show N more" fold keeps one divider at its true start; the header POS chip is dropped when 2+ groups exist. CI green |
| 2026-09-01 | Dictionary track Phase 9 shipped: Settings dictionary priority reorder UI (`d12c905`) — the v12 `priority` column was write-only; `Dictionary` now carries it, `list_dictionaries()` selects it and orders `priority ASC, name ASC`, and Dictionaries rows get ↑/↓ buttons that renumber all packs compactly (0, 1, 2, …), trigger `rebuild_combined_dictionary()`, and refresh; the top row shows a quiet "speaks first" chip; ↑/↓ disabled at the ends; re-imports keep user-set priority (upsert doesn't touch the column). CI green |
| 2026-09-01 | Dictionary track Phase 10 shipped: lookup history — schema v14 `dict_lookups` (FK to books `ON DELETE SET NULL`, NOCASE word index), logging in the `lookup_dict` funnel (both the popup selection/tap path and sidebar lookups; per-keystroke search prefixes not logged), misses logged with `found = 0`, same-hour same-book collapse, pref `dict_history_enabled` (default 1) with a reader Settings toggle, Lookup History page (day-grouped, searchable, miss chip, Clear), `repeat_lookup_words()` feeding "Suggest from history" chips on Saved Words plus a "Last 3 lookups" dashboard card on Library. Three CI iterations fixed a relm4 5-arg `update_with_view` + `#[watch]` label lifetime (`68373fa`), a FK violation in the new test (seed real books, `6b4e528`), and the repeat-set expectation (miss word logged across two books, `0b188a3`). 156 unit tests green |
| 2026-09-02 | Design conversation recorded in `docs/conversation.md`: (1) performance — stay Rust, architecture is the bottleneck (no language rewrite); (2) second AI's analysis reviewed and verified (grid rebuild, sync decode, in-memory cover cache, 3,638-line CSS — all real; fix order adjusted: thumbnails → async decode → virtualize if numbers say so); (3) Yazi philosophy adopted — service layer + task manager + preloaders + thin UI (relm4 + async-channel already give half the skeleton); (4) roadmap scope reviewed — P6–P11 stand; custom text-renderer question deferred to a dedicated discussion; (5) **Plugin API non-goal reversed** — plugins wanted, leaning Lua, design TBD after the architecture track. Roadmap: schema version corrected to 14, phase map gains A0 (architecture track) + P12 (plugins), non-goals updated, next-steps gains the A0 entry |
| 2026-09-02 | Scope confirmed as a **content platform** (user): fiction sources (AO3/FFN/Webnovel/Royal Road…) with tag search, downloads, offline reading, follow + **auto-updater** (FFN-app-class) and manga sources (Suwayomi-class browse/read). Roadmap: P7 expanded (fiction platform, auto-updater replaces manual check), P9 expanded (manga platform; **no Suwayomi server rewrite** — adapter concept natively, optional Suwayomi-server client adapter later), P12 = source-adapter plugins (Lua). Renderer question framed in `docs/conversation.md` §7: WebKit = full browser engine (power vs weight); custom renderer = we draw text (1–3 person-years, but sources feed it clean content so it never needs to be a browser); **leaning hybrid** — custom for reading, WebKit for browse/fallback; sequencing: product (sources) first on WebKit, renderer as A0 crown after |
| 2026-09-02 | Scope sharpened: **no browse mode** — Kalam never renders arbitrary websites; sources return structured data via plugins. Renderer decision resolved direction: WebKit = EPUB engine only (lazy), custom renderer for source fiction + comics; EPUB normalization (crengine-style) is Path B crown. Tachiyomi/Suwayomi fully explained: extension = 200–500-line Kotlin adapter for one site; Kotlin/JVM can't run in Rust; we reimplement the adapter pattern natively (MangaDex/Komga/OPDS need no scraping); Suwayomi-server bridge is a cheap optional plugin. PDF not forgotten — P10 MuPDF, fixed-layout, never WebKit (AGPL license note). Borrow list recorded in `docs/conversation.md` §8: FanFicFare (fiction adapters), Tachiyomi extensions (pattern), MangaDex API, Komga/Kavita, KOReader + crengine (renderer), cosmic-text/swash/vello (Rust text stack), lol_html/ammonia (sanitizing), Yazi, Foliate — all license-compatible with GPL-3.0-or-later |
| 2026-09-02 | Renderer direction sharpened (user push-back): custom renderer is the **endgame for all reflowable text** — cosmic-text based (Rust text layout, NOT an EPUB engine; we build normalization/pagination/painting), fiction first (clean content), EPUB via a lol_html normalization pipeline after; WebKit demoted to fallback for exotic EPUBs (may be cut). crengine (C++ EPUB engine) kept as a legitimate shortcut if EPUB-before-custom-engine is wanted, at the cost of C++ in the stack + less dict/theme/annotation control. Manga architecture confirmed: Tachiyomi-shaped `Source` adapter API with **Lua plugins we write**; **no Kotlin-extension bridge** (they are Android APKs — wrong shape for desktop; pattern + scraping logic port instead; MangaDex/Komga/OPDS need no scraping). PDF stays MuPDF (P10); comics = image decode + GTK pager (no engine). Engine map recorded in `docs/conversation.md` §8 |
| 2026-09-02 | Renderer timing decided: **not now, not at the end — start right after sources, grow alongside.** Order: A0 architecture → P7 fiction sources → renderer vertical slice (alongside) → EPUB normalization → PDF/comics. crengine deep-dive: GPL-2.0 (KOReader fork AGPL-3.0) vs our GPL-3.0-or-later — license mismatch; C++ codebase with no Rust bindings (FFI wrapper burden); partial CSS 2.1 (no float/border/etc. — what real EPUBs use); dict/annotation integration is the same fight against a foreign engine. Verdict: crengine only as a separate dynamically-linked fallback bridge; **cosmic-text + our own normalizer remains the recommendation** (hard part is ours either way) — recorded in `docs/conversation.md` §10 |
| 2026-09-02 | Roadmap restructured as the single handoff document for future chats: new "⚠️ Read this first" block (agent instructions: read README/ROADMAP/conversation.md, keep docs current in the same commit, never skip the changelog, CI is the gate, user is the QA loop); new "Documentation discipline" rules in the Working agreement; new **"Current trajectory (locked)"** section — agreed order A0 → P6 → P7 → renderer vertical slice (alongside P7) → P8/P9 → EPUB normalization → P10/P11/P12, plus all locked decisions in one place; detailed **A0 section** (measure → LibraryService → thumbnails/async decode → task manager → preloaders → virtualization-if-numbers-earn-it → perf-budget CI test → plugin-host seam); detailed **P12 section** (Lua via mlua, source-adapter API, no Kotlin bridge, no Suwayomi rewrite); phase map + next steps rewritten to match |
| 2026-09-02 | **chapbook discovered** (`ophymx/chapbook`, Apache-2.0): a week-old project that IS our custom-renderer plan — stylo (Firefox's CSS engine) + cosmic-text + tiny-skia/vello, no webview, pagination-first, GTK4 viewer, quote-anchored `LayeredLocator` positions, PDF via hayro. Recorded in `docs/conversation.md` §11: added as a **candidate renderer foundation** (re-evaluate at vertical-slice time, not a dependency yet), **quote-anchored locators adopted for annotations** (fixes the P7 auto-updater anchor risk), **hayro added to the P10 shortlist** (AGPL-free PDF), stylo noted as the preferred EPUB-cascade option vs hand-rolled normalization. WebKit fallback kept (chapbook's own stated limit: a subset of publisher EPUBs) |\n| 2026-09-02 | A0 step 1 (measure first) started: headless data-layer measurement harness added as `src/perf.rs` (`#[ignore]`d, seeds 2,000 books; times `list_books` ×3 sorts, search, `recent_books`, `library_stats`, tags). Run with `cargo test --release perf -- --ignored --nocapture`. CI compiles it but skips it, so CI stays fast. No behaviour change |
| 2026-09-02 | A0 step 1, GUI half: in-app timing harness added as `src/timing.rs`, gated behind `KALAM_TIMING=1`. Prints cold start (`window_shown` via window `realize`), `book_open` (EPUB parse), `chapter_load`→`chapter_done` (WebKit render), and `dict_lookup` milliseconds to the terminal. No-op when the env var is absent, so zero overhead in normal use. `src/perf.rs` counts timings. The data-layer baseline is in: all list-page queries stay under ~20 ms for 2,000 books, confirming the DB layer is well-tuned and the UI side (sync cover decode + full grid rebuild + ephemeral cover cache) is where A0's later steps focus. Run with `KALAM_TIMING=1 cargo run --release` |
| 2026-09-02 | **Held A0 step 6 (grid virtualization).** Measured evidence: the data layer is <20 ms for 2,000 books and there is no measured grid lag, so virtualization would add risk for no measured win. Recorded in the A0 status block. |
| 2026-09-02 | Home gains an **"+ Add books"** button (header, right of the title) that opens the same EPUB picker and background import as **My Library → All books** — parse, hash, copy, per-file progress, and an "Import done — N added / M already in library / K failed" summary. It disables itself while importing and refreshes Home in place on completion so the counts, Continue and Recently-added cards update. Home was converted from a relm4 `SimpleComponent` to a full `Component` so the import worker can report progress and the button/label state can update; it reuses `ImportProgress`/`ImportTally` from All Books. The My Library dashboard keeps its own path, which heads to All books for import |
| 2026-09-02 | A0 step 3 (thumbnails) shipped and CI-green: `src/thumbs.rs` generates a persistent 256×408 thumbnail (`cache/thumbs/<uuid>.png`) at import and on cover replacement; the grid prefers it when the slot is small enough (never upscales it); covered by 5 headless unit tests. New dep `image` (default-features off; only png/jpeg/gif/webp) because gdk-pixbuf in this toolchain cannot encode PNG. Thumbnails are removed on book delete. `src/perf.rs` gains a cover-decode probe (full cover vs thumbnail). **Existing books:** `backfill_missing` runs on a background thread at startup so a library imported before this change gains thumbnails without re-importing (only missing files are generated). Several CI iterations fixed the real bugs — image's `std` feature does not exist (default-features off is fine; `image::open`/`save_buffer` need no feature), `DynamicImage` has `width()/height()` not `dimensions()`, `paths.rs` needed `Path` imported, a temporary `Option<PathBuf>` was dropped while borrowed, and `thumbs.rs`/tests used `PathBuf` without importing it and passed a `String` to a `&str` parameter. The true async *swap-in* is deferred to the task manager (step 4) where it architecturally belongs — the thumbnail-decode win is already captured synchronously. Part of A; see the A0 later steps |
| 2026-09-02 | **WebView reuse shipped** (the cheap A0 win step 1 measured): the reader called `webkit6::WebView::new()` in `init()`, so every book open spawned a WebKit process (~400 ms, vs ~3.5 ms to revisit a warm one). `src/webview_pool.rs` parks exactly one view between readers — `acquire()` in `init()`, `release()` in `shutdown()`. Only the widget is pooled, deliberately **not** the whole reader page: caching the page would also keep the reading session counting while the user browsed the library and defer the progress write, so the reader's lifecycle is unchanged. Handler discipline is the subtle part — a recycled view still carries the previous reader's handlers, each holding a dropped component's `Sender`, so sizing / context-menu suppression / the `"kalam"` script-message *registration* are permanent and live in the pool (WebKit rejects a second registration of that name on one manager), while every handler capturing a `ComponentSender` is recorded as a `SignalHandlerId` and disconnected in `shutdown()` before parking. Cost: the WebKit process (~100–200 MB) stays resident after the first book instead of being released on leave; the page is blanked on release so the book's DOM is still freed. `KALAM_NO_WEBVIEW_POOL=1` restores the old behaviour for A/B measurement with `KALAM_TIMING=1`. **Needs an Arch smoke-test:** book A → leave → book B → back to A, checking highlights, dictionary popup, tap-to-look-up and progress restore on the 2nd/3rd open |
| 2026-09-02 | **A0 step 2 started: `LibraryService`** (`src/service.rs`) — the seam between pages and the database. Pages made several direct `Catalog` calls each and swallowed the errors individually (26 `unwrap_or_default()`, 16 `.ok().flatten()` across `pages/`), so a broken database rendered as an empty library, there was nowhere to put caching, and moving queries off the UI thread meant editing every page. The service answers a page's whole data question in **one call returning one owned snapshot**. Snapshots rather than one-for-one wrapped getters is the substance of the step: a snapshot is a plain owned `Send` struct, so the same call can later run on a worker and be handed back to the UI without touching the page — asserted at compile time by `snapshots_are_send()`. Error policy now lives in one place (degrade to empty **and** record the reason; pages surface it as a toast — the service never calls `notify`, which is UI-thread-only, so it stays worker-callable). Converted Home (4 reads → 1), Analytics (4 → 1), Tags cloud + tag-books; Home's "continue reading" fallback chain moved into the service and gained tests, including the previously untested rule that a 100%-finished book is not offered as "continue". Other pages keep their `Arc<Catalog>` and migrate incrementally — the service borrows the same `Arc`. Writes (import) still go straight to the catalog: those belong to step 4. 9 new unit tests |
| 2026-09-02 | Docs accuracy pass: README file map listed `openlibrary.rs` at the top level (moved to `src/metadata/`) and omitted `author`/`notify`/`thumbs`/`perf`/`timing`/`icons`/`paths`; README + ARCH data-dir blocks said `override-covers/` where the code creates `covers/` (`paths.rs:43`) and omitted `cache/thumbs/` and `authors/`; ARCH phase table said "P4 ← you are here", three phases stale; ROADMAP pinned agents to a branch id from two sessions ago in two places (now states the per-session rule instead of naming one) and carried a verbatim duplicate of the "Reader chrome restyle (P2.1)" section; test count 156 → 163 (165 `#[test]`s, 2 `#[ignore]`d perf probes) in both files; `ci-logs/` held failures from runs that were since fixed, which reads as if the branch is red — cleared with a note, CI overwrites them on the next real failure |
| 2026-09-02 | Home gains an **"All books"** button (header, left of "+ Add books") routing to the existing `Route::LibrarySection(AllBooks)` — the full grid was previously reachable only from the My Library dashboard's empty-library placeholder — see the correction row below, this claim was wrong. Browsing is secondary-styled, importing keeps the primary emphasis. Also fixed a real defect found while wiring it: when a search matched nothing, All books rendered "Your library is empty. Click + Import EPUB" — wrong for anyone with books, and it hid the actual fix. `rebuild_list` now takes the query and distinguishes the two empty cases, naming the failed term and pointing at clearing the search. Added `docs/testing-a0.md`: the Arch smoke-test recipe (the 5 checks that catch a stale WebView handler on the 2nd/3rd book open), what each `KALAM_TIMING=1` label measures, and the A/B procedure against `KALAM_NO_WEBVIEW_POOL=1` that quantifies the ~400 ms saving. Corrected `timing.rs`'s own module docs, which advertised a `chapter_done` line that is never printed — `span_end` prints under the opening label, so `chapter_load` is a single line covering load→rendered. Test count corrected again: README/ROADMAP claimed 163, the tree now has 175 `#[test]`s minus the 2 `#[ignore]`d perf probes = **173** that CI runs (the `service.rs`, `webview_pool.rs` and `thumbs.rs` tests landed after the last recount) |
| 2026-09-02 | **Correction + dead-UI fix, prompted by the user disputing the previous row.** I had written that All books was "reachable through the My Library dashboard"; that was wrong. `library.rs` linked `AllBooks` from exactly one place — line 114, inside the `if stats.total_books == 0 { … return; }` placeholder — so the only moment the full grid was reachable was while the library was empty, and importing your first book removed the link. Auditing the other sections found worse: `ReadingList`, `Tags` and `Analytics` have complete pages, `PageSlot` variants and `Route::LibrarySection` arms in `app.rs`, but **nothing anywhere in the UI ever emitted those routes** — three finished pages that could not be opened at all. (`LibrarySection::ALL`/`icon()` are `#[allow(dead_code)]`, a leftover of the tile grid that the v5 dashboard replaced; the dashboard routes via content sections, and sections only render when they have content, so pages with no section were orphaned.) Added a `quick_links` row under the My Library title — All books / Reading list / Tags / Analytics — and dropped the now-duplicate "ALL BOOKS" section from the empty branch. Lesson recorded: "a route exists in `app.rs`" is not evidence the user can get there; reachability means grepping for who *emits* the route |
| 2026-09-02 | First real `KALAM_TIMING=1` run on the user's library (Arch, release build). **The WebView pool is confirmed working**: chapter turns settle at 37–48 ms and later book opens at 2.8–215 ms, with no ~400 ms WebKit re-spawn after the first book — the tail the pool was built to remove. The first book of a session still pays WebKit startup (`chapter_load` 2772.9 then 697.2 ms), which is expected and unavoidable without pre-warming. **But cold start came back at 8018.7 ms against a ~0.9 s baseline**, which nothing in A0 explains. Rather than guess, split `window_shown` into `startup_db_open` / `startup_dicts` / `startup_first_page` so the next run attributes it; prime suspect is the first-run bundled-dictionary import, which decompresses and inserts ~6.8 MB of gzipped TSV **on the UI thread** before first paint (`install_bundled_dictionaries`, `app.rs` init) and early-outs on a pref afterwards — i.e. probably once-per-install, not per-launch. Also removed `LibraryService::change_token`, a passthrough I added in step 2 that nothing ever called (the app cache uses `self.catalog.change_token()` directly): it was `pub`, so only the binary's `dead_code` warning caught it, and **CI could not have** — the clippy step has `continue-on-error: true` and no `-D warnings`, so warnings never fail a run. Noted as a gap in the CI gate |
| 2026-09-02 | Cold-start scare **resolved: there was no regression, and my diagnosis was wrong.** Six consecutive `KALAM_TIMING=1` runs decayed 6290 → 1596 → 965 → 852 → 945 → 831 ms, settling at **~898 ms — the documented ~0.9 s baseline**. The 8 s was a first-run-after-build artefact (OS page cache warming on a freshly-linked binary and its GTK/WebKit/ICU libraries), not app work. I had named the bundled-dictionary import as prime suspect; the spans measured `startup_dicts` at **0.1–0.2 ms on every run including the first**, so the pref early-out was already working and the suspect was innocent — had I "fixed" it on the hypothesis I would have rewritten correct code and left the real distribution unmeasured. Useful result from the breakdown: of a steady ~898 ms, only **~138 ms (16%)** is instrumented app work, and **~754 ms (84%) is toolkit startup** (GTK/libadwaita/WebKit/CSS/first layout) before or around `AppModel::init`. `startup_first_page` (~137 ms) is the only app-side target worth attacking; the DB open (~6 ms) and dictionary check (~0.1 ms) have nothing left in them. Recorded as the floor for A0 step 5. **CI gate tightened** in the staged workflow: clippy now runs `-- -D warnings`, so a `dead_code` warning like the unused `LibraryService::change_token` fails the run instead of passing green (handed to the user to install — the agent cannot push `.github/workflows/`) |
| 2026-09-02 | `-D warnings` installed by the user (`163c56c`) and **it immediately paid for itself**: the first run failed with **30 warnings that had been invisible**, one of them a real bug. `src/pages/series_float.rs` used `let _ = tx.send(result)` on an `async_channel::Sender` from a plain worker thread — `send()` there returns a *future*, nothing polled it, so the fetched series listing was silently dropped and the receiver only woke when `tx` fell out of scope, reporting "Fetch worker ended unexpectedly" on every **successful** fetch. Fixed with `send_blocking` (the pattern `metadata_editor.rs` already used correctly); it was the only `async_channel` send in the tree. Also fixed: a `HomeOut` variant-prefix regression I introduced (renamed `OpenBook`/`OpenBookDialog`/`OpenAllBooks` → `Book`/`BookDialog`/`AllBooks`, matching the convention `LibraryOut` documents), an unused test helper in `thumbs.rs`, `epub_write.rs` had 7 real functions sitting *after* its `#[cfg(test)] mod tests` (moved the module to EOF), plus needless borrows, redundant closures, `contains` over `iter().any`, `sort_by_key`, an unnecessary `to_string`, redundant `i32` casts and two redundant re-bindings. Three judgement calls kept the code and justified the suppression in a comment instead: the Lesk stemmer's identical-bodied suffix rules (distinct linguistic rules that will diverge), `build_reader_settings_panel`'s 8 arguments (a struct existing only for one call site), and `AuthorPageMsg::Fetched` was **boxed** rather than suppressed since ~376 bytes on every message including the frequent `Refresh` is a genuine cost. `Rc<RefCell<Option<Rc<dyn Fn()>>>>` appeared in three pages and is now the `pages::SelfRebuild` alias |
| 2026-09-02 | A0 step 2 continues: **Reading list converted** (4th page). It read `list_reading_list().unwrap_or_default()` in two places, so a failed database read rendered as the friendly *"Books you plan to read next"* placeholder — the same empty-state lie as the All books search bug, and the same class as the series-fetch bug the strict clippy gate just exposed: **code written never to complain**. Now `service.reading_list()` returns one owned snapshot and the page reports failures as a toast. Writes (reorder, remove, bulk add) still go through `service.catalog()`, the deliberate escape hatch — they belong to step 4. 2 new tests, including one asserting an empty queue raises **no** error, so the fix cannot regress into a toast on every visit |
| 2026-09-02 | A0 step 2 continues: **Shelves grid converted** (5th page) — `list_shelves().unwrap_or_default()` in both `init()` and `Refresh`, so a failed read rendered as "no shelves yet". Now one `service.shelves()` snapshot with the failure surfaced as a toast; the writes (create, edit, delete) keep going through `service.catalog()` until step 4. 1 new test covering both halves: a real shelf comes back, and an empty grid produces **no** error. **Saved quotes was examined and deliberately left alone** — it already does a full `match` on `list_all_quotes` and shows `"DB error: {e}"`, so it does not have the defect step 2 removes; converting it would be churn for its own sake. Being on the list of unconverted pages is not the same as being broken, and the count is not the goal |
| 2026-09-02 | A0 step 2 continues: **All books converted** (6th page), and it turned out to be *half* honest already — `reload()` matched on `list_books` and showed `"Database error: .."`, but `init()` used `unwrap_or_default()`, so **the first paint of the page claimed an empty library where a refresh on the same broken database would have told the truth**. Exactly the inconsistency a single seam removes. Now both paths go through `service.all_books(sort, query)`; the error still lands in the status line rather than a toast, because this page has always reported failures there and changing that would be a UI change smuggled into a refactor. The import-summary line still outranks the generic count. 1 new test pinning the three cases the page's own copy distinguishes: all books, a matching search, and a search matching nothing (which is **not** an error) |
| 2026-09-02 | **A1 (in-app dialogs) logged and scheduled** after the user reported that dialogs open as separate windows, which a tiling compositor (Sway) treats as ordinary top-levels — movable to another workspace, tiled beside the app, left behind on a workspace switch. Audit found the app already half-converted: an in-app float layer exists in `app.rs` (`float_host` + `float_scrim` on a `gtk::Overlay`) with a comment explicitly stating floats must never be separate windows "so the compositor can't move it to another workspace", and **5 dialogs already use it** (book float, series float, annotations, shelves, tags). **5 remain as `gtk::Window`**: metadata editor, shelf editor, the two book pickers, delete-shelf confirm. The 4 `FileDialog`s stay native on purpose — they are portal-backed and the user wants their real file manager. User's hard requirement recorded: **every in-app dialog needs a visible close/✕ button**, since without a title bar there is no compositor-provided escape; the existing five already comply (✕ on the floats, Done on the panels, global Esc/Q in `app.rs`). Ordered cheapest-first, metadata editor last |
| 2026-09-02 | A0 step 2 continues: **History and Lookup History converted** (7th and 8th pages). Both called `unwrap_or_default()` on every read — `history.rs` in `init()` and `reload()`, `lookup_history.rs` in `init()` and *both* refresh paths — so a failed read rendered as "Opened and finished books show up here as you read", i.e. indistinguishable from a fresh install. Both now take one snapshot per read and toast the failure; the clear-history writes still go through `service.catalog()` until step 4. 2 new tests: rows come back for a seeded event and lookup, and an empty log produces **no** error. **`saved_words.rs` was examined and deliberately skipped** — like `saved_quotes.rs` it already `match`es its main read and surfaces the error, so there is nothing for step 2 to remove there. Third page now audited-and-left-alone; the unconverted list is not a to-do list, it is a list of pages that have not been *checked* |
| 2026-09-02 | A0 step 2 continues: **My Library converted** (9th page) and it was the worst one — the dashboard made **eight** independent catalog reads and `unwrap_or_default()`d **every single one**, so a broken database rendered as a cheerful, fully laid-out, completely empty dashboard: no stats, no continue-reading strip, no quotes, no vocabulary, no history, and a reading goal of zero. The landing page of the app was the least honest page in it. All eight now come from one `service.dashboard(FEED_LIMIT)` snapshot and failures are toasted. Two structural fixes came with it: the free functions `build_dashboard`/`goal_card`/`history_feed` took `&Arc<Catalog>` and read the database themselves, which is exactly what blocks step 4 — they now take the snapshot instead; and `history_feed` held **an N+1**, calling `get_reading_progress` or `get_book` once per event, which is the same defect found in `saved_quotes.rs`. `LibrarySession` had to be exported from `crate::db` because no caller had ever been able to name the type. `reading_goal()`/`finished_this_year()` return bare `i64` and cannot fail, so they add no error rows. 2 new tests: a seeded dashboard reports no errors, and an empty one is **not** an error |
| 2026-09-02 | **Correction to the two rows above.** They record `saved_quotes.rs` and `saved_words.rs` as "examined and deliberately left alone" because each already `match`es its main read. When challenged to justify that, the reasoning did not survive: checking only the headline query is not a sufficient screening test. `saved_quotes.rs` calls `get_book` **per quote**, and `get_book` internally runs a second tags query — about **2N+1 queries on every page open and every keystroke in the search box** (500 quotes is roughly 1,001 queries). That is the same class of bug this project already fixed once for the Library dashboard (~99 reads down to ~8) and then reintroduced elsewhere unnoticed; there is still no batch `books_by_ids` in `src/db*` to fix it with. `saved_words.rs` swallows two *counting* queries with `.unwrap_or(0)`, so a failure silently reports **zero known words**. Both also keep scattered DB calls in UI code, which is the thing step 4 cannot move off the UI thread. The screening test is therefore: main read, **secondary/enriching reads, N+1 patterns, and off-thread readiness**. Both pages are back on the list. "It reports its main error" is not a reason to skip a page |
| 2026-09-02 | A0 step 2 continues: **Saved quotes converted** (10th page) — the page named in the correction row above. Its main read was always honest, but it called `get_book` **once per quote** and `get_book` runs a second query for tags, so a 500-quote library did about **1,001 round trips on every page open and every keystroke in the search box**. Added `Catalog::books_by_ids`, which does the whole batch in **two** queries (books, then all their tags at once) regardless of how many quotes there are, de-duplicates ids because one book usually owns many quotes, and chunks at 500 to stay under SQLite's host-parameter cap. Missing ids are simply absent from the map, so "this book was deleted" stays distinguishable from "the read failed" and the page keeps rendering *Unknown Book*. The same N+1 in the shared `export_all_quotes_markdown` (used by Settings → Export too) is fixed the same way. The page keeps reporting errors in its status line, since that is what it has always done. 4 new tests (187 total): the batch agrees with `get_book` including tags and file paths, duplicate and unknown ids are tolerated, an empty id list is not an error, and search hits, misses and an empty library are all error-free |
| 2026-09-02 | A0 step 2 continues: **Vocabulary (saved words) converted** (11th page), the second page named in the correction row. Its list read was honest, but the two header counts were fetched by calling `list_saved_words` twice more and taking `.len()` — loading up to 500 full rows, definitions and all, purely to count them — and both were swallowed with `.unwrap_or(...)`, so a failed read reported **zero known words** as though it were a fact. Added `Catalog::saved_word_counts()`, one `COUNT(*)` + `SUM(CASE WHEN known ...)` query, and routed the page through `service.words(query, filter)`. Two side benefits: the page went from **three** queries per reload to **two**, and the header counts now cover the whole table instead of stopping at the 500-row display cap, so they are correct for large vocabularies. Errors stay in the status line, matching the page's existing behaviour. 2 new tests (189 total): counts are read rather than guessed and survive filtering, and an empty vocabulary is **not** an error |
| 2026-09-02 | A0 step 2 continues: **Book float and Series float converted** (12th and 13th pages). The book float made the same three reads twice — once in `init()`, once in `reload_state()` — and swallowed all six, so a database failure rendered as **"this book was deleted"**: the panel could not tell a missing book from a broken read. Now one `service.book_detail(id)` snapshot serves both paths, and `book: None` means genuinely gone while a real failure goes to `errors` and a toast. The series float held **another N+1**: `book_finished_at` once per row, so a 20-book series meant 20 extra queries every time the panel opened. Added `Catalog::finished_book_ids(&[i64])` — one query, chunked at 500 — and the panel now computes the whole set before the row loop. Its `books_in_series` read was also swallowed, which would have quietly claimed **you own none of the series** on a failure; it now reports. That is the **third** N+1 found since the screening test was tightened (saved quotes, the dashboard's history feed, and this one), which is the strongest evidence yet that "its main read is honest" was never a safe way to skip a page. 3 new tests (192 total) |
| 2026-09-02 | A0 step 2 continues: **Shelf detail converted** (14th page), and a stale-header bug fell out of it. `Refresh` re-read the shelf row and then called `reload()`, which re-read only the books — two separate reads of the same thing, both swallowed, and the header could end up describing a shelf that had already been renamed. One `service.shelf_detail(id, sort, query)` snapshot now returns the shelf and its books together, so they cannot disagree, and a missing shelf skips the books read entirely instead of producing two vague outcomes. Failures toast. The book-picker at the bottom of the file still takes a raw `Arc<Catalog>` **on purpose** — it is a separate `gtk::Window` and therefore an **A1** target; converting its plumbing now would only have to be redone when it becomes an in-app dialog. 2 new tests (194 total). **A test I wrote was wrong and CI caught it**: it asserted a book could be both finished and on the reading list, but `set_book_finished(true)` deliberately drops the book off the list. The test now pins that real coupling instead of my assumption about it |
| 2026-09-02 | A0 step 2 continues: **Book page converted** (15th page) — the second-largest file on the list, 31 swallow sites. Two clusters did the damage. The header repeated the same three reads across **four** call sites, and two of those re-read the book row *and then* called `reload_state()`, which re-read the other two — so opening the page and pressing anything did overlapping work, all of it swallowed, and a failure looked like a deleted book. It now reuses the very same `service.book_detail(id)` snapshot the book float uses; one seam, two pages. The stats/timeline card made **six** more swallowed reads scattered through a 150-line function (total time, session count, reading position, seconds-by-day, recent sessions, finished date, first opened) and drew "you have never read this book" on any failure; those are now one `service.book_stats(id, days, limit)` call at the top of the function. `SessionRow` had to be exported from `crate::db`. The remaining `unwrap_or`s in this file are string and slice defaults (`.get(..10).unwrap_or("")`), not swallowed DB errors. 2 new tests (196 total) |
| 2026-09-02 | A0 step 2 continues: **Reader converted** (16th page) — the largest file in the project. Its raw count of 35 "swallows" was misleading: most are `get_pref_i64(key, default)`, which returns a default **by design** and is step-4 work, not error swallowing. The real defect was four reads at book-open — the book row, its highlights, its bookmarks and your saved words — each `unwrap_or_default()`d, so **opening a book against a broken database silently showed none of your work**: the reader looked completely normal and simply had no highlights. Those are now one `service.reader(book_id)` snapshot. The three `reload_*` helpers also swallowed, and worse, they *overwrote* the live lists with empty ones on failure; they now report and **keep what is on screen**, because a blank list reads as "you never highlighted anything". Also removed a redundant `get_book` at session start that re-read the book row the snapshot had just fetched. Fixes the clippy failure from `a4ebfd2`: two **multi-line** `model\n.catalog` chains in `book.rs` that a single-line grep missed — the sweep is now a regex over `(self|model)\s*\n\s*\.catalog` and the whole repo is clean. 2 new tests (198 total) |
| 2026-09-02 | **A0 step 2 complete.** Final pass over the last four files, none of which needed a snapshot but three of which were lying anyway. **Metadata editor** and **Shelf editor** both did `let Ok(Some(x)) = ... else { return }` / `_ => return`, so on a failed read the dialog **simply never appeared** — no window, no message, nothing to click; they now say whether the thing was deleted or the read failed. Shelf editor's duplicate-name check was `.unwrap_or(false)`, i.e. a failed check assumed the name was free. **Settings** rendered a failed `list_dictionaries()` as "no dictionaries installed", which is exactly what a successful uninstall looks like. The reader's dictionary search made a broken index indistinguishable from "that word isn't in the dictionary". **`author.rs` genuinely needed nothing** — it makes no database reads at all, it only passes the `Arc` to its children, and its lone `unwrap_or_default()` is on a local helper; that is what a real skip looks like, stated precisely, versus the earlier hand-waving. Remaining `unwrap_or`s across the pages are now only `get_pref(key, default)` calls (defaults by design, step 4) and string/slice defaults like `.get(..10).unwrap_or("")`. **Score for the whole step: 16 pages converted, 5 N+1 query storms removed, 3 new batch queries (`books_by_ids`, `saved_word_counts`, `finished_book_ids`), 198 tests** |
| 2026-09-02 | **A real bug the step-2 pass uncovered, not just a refactor.** The new `book_stats` tests failed in CI, and the cause was a genuine defect in `Catalog::book_first_opened`: `SELECT MIN(at) ...` over zero rows still returns **one row containing NULL**, so `.optional()` does not help — the *value* has to be nullable. Reading it as a plain `String` made "this book has never been opened" a **hard error**. It went unnoticed for as long as it existed precisely because every caller wrote `.ok().flatten()`,which turned the error into `None` and produced the right screen by accident. This is the clearest possible demonstration of why step 2 was worth doing: the swallow was not just hiding hypothetical future failures, it was hiding a live bug in the query underneath it. Fixed by reading into `Option<String>`; audited the other aggregates and they all already use `IFNULL`. 1 regression test (199 total) |
| 2026-09-02 | Wrote `docs/testing-a0-step2.md` answering "do I need to test anything before A1?". Short answer recorded: **no new parameters, no required testing** — CI covers compile, clippy `-D warnings` and 199 tests. Two things CI genuinely cannot check are written down: (1) the **corrupt-database test**, which is the only way to see what step 2 actually fixed — `XDG_DATA_HOME=/tmp/kalam-test` gives a throwaway library so the real one is never touched, then overwrite `catalog.db` with garbage and confirm the app now *says* something instead of drawing a cheerful empty library; and (2) a two-minute pass opening each page to confirm the happy path still looks right, since CI has no display. Also noted the three user-visible effects of step 2 on a healthy database: the `book_first_opened` bug fix, Saved quotes no longer running ~1,001 queries per keystroke, and failures now toasting. Added a pre-A1 question for the user: the five already-in-app dialogs are the pattern A1 will copy five more times, so it is worth deciding now whether they are the standard to match |
| 2026-09-03 | **Crash fixed: a corrupt catalog aborted the process with a core dump.** The user ran the corrupt-database test from `docs/testing-a0-step2.md` and it did not toast — it died. Cause was a fallback in `AppModel::init` that "handled" a failed `Catalog::open()` by **calling the same function again and `.expect()`ing it**, which is a guaranteed panic; and because `init()` runs inside a GTK signal callback, that panic **cannot unwind**, so it escalated to `panic in a function that cannot unwind` → abort → core dump, printing a raw backtrace instead of saying what was wrong. Fixed in two places: `main()` now opens the catalog **before** `app.run()` and, on failure, prints the error, the database path, and the exact `mv` command to move the broken file aside (noting book files live elsewhere and are safe), then exits 1; and the `init()` arm no longer retries — it reports and exits cleanly, since reaching it means the database broke between the pre-flight check and startup. **This is the second real bug the step-2 pass has surfaced**, and again the pattern is the same: the error path had never been executed, so nobody noticed it was nonsense |
| 2026-09-03 | **Backdrop-click now closes in-app dialogs, and the A1 close-button rule is revised.** The user asked for click-outside-to-close and questioned whether the ✕ could then go away. Implementation turned out to be two lines: the scrim already had a `GestureClick` whose handler was **empty** — it existed only to stop clicks reaching the page behind it, so the dimmed area looked interactive and did nothing. It now sends `CloseBookDialog`, the same message Esc sends. Z-order was already correct (scrim added to the overlay before `float_host`), so clicks *inside* the dialog are unaffected. On the button question the user made the sharper point that **the dialogs exist for different reasons and should not all be dismissed identically**; the roadmap now carries a table mapping dialog *kind* to affordance — **‹ Back** for detours you navigated into, **✕** for transient overlays, **Cancel + Save** for forms with unsaved input, **Cancel / Delete** for confirmations. Recorded decision on dropping the button entirely: **no** — backdrop-click and Esc are both invisible affordances, so a dialog whose only exits are invisible is still a trap; keep one visible control, but the right one rather than a reflexive ✕ |
| 2026-09-03 | **A1 done: all five remaining `gtk::Window` dialogs now draw inside the app.** `gtk::Window::builder` no longer appears anywhere in `src/`. One new helper, `src/widgets/in_app_dialog.rs`, walks up from any widget to the app's root `gtk::Overlay` and adds its own scrim + centred panel; it deliberately does **not** reuse the `AppMsg` float layer, because these dialogs are plain functions taking an `on_confirm: impl Fn()` closure and a closure cannot travel through a `#[derive(Debug)]` message enum. `DialogExit` ended up with two variants, not the table's four: both converted kinds already bring their own named buttons, so the header adds none — `OwnButtons` (pickers, delete confirmation) closes on a backdrop click, `UnsavedInput` (metadata editor, shelf editor) **does not**, because silently discarding a half-typed description over a slightly-off click is a bad trade. The ‹ Back and ✕ rows still describe the book/series floats, which have their own headers in `app.rs`. The interesting part was **what a `gtk::Window` had quietly been doing for free**: (1) `close()` destroys the widget tree and so breaks the reference loop between a widget and the callback capturing it — removing an overlay child does not, so `teardown()` empties the host too, otherwise every dialog ever opened would leak; (2) a window has a default height, while a panel centred in an overlay is sized by its content, so the metadata form and the smart-shelf rule list needed `max_content_height` + `propagate_natural_height` or a long description would push Save off a 768px screen; (3) the cover `gtk::FileDialog` is portal-backed and needs a genuine top-level parent, now resolved from the anchor's root. Also deleted three copies of a `window_of()` helper that existed only to parent these dialogs, and two hand-rolled Esc handlers now that the helper gives Esc to all of them. 1 new test |
| 2026-09-03 | **Audited the five dialogs that were already in-app; they did not match the five new ones, or each other.** Four fixes. (1) The book float's ✕ was tooltipped "Close (Q)" and the series float's "Close (Esc)" — same layer, same keys, two different lessons. (2) A real bug: the global float key handler in `app.rs` closed on `q` whenever a float was visible **without checking whether a text box had focus**, so typing the letter `q` into the tags panel's entry dismissed the panel instead of typing. Esc now always closes; `q` is ignored while an entry has focus. (3) On the user's instruction the ✕ came off **both** floats with **no ‹ Back replacement** — they are read-only detours, so a stray backdrop click costs nothing ("I won't lose anything if I accidentally misclicked"). This deliberately waives the "keep one visible control" rule for exactly the case the rule was never meant to cover: there is nothing to lose. Removing the button made `SeriesFloatMsg::Close` and `SeriesFloatOut::Close` unreachable, so they were deleted, along with four now-dead `.kalam-float-close` CSS rules. (4) The annotations, shelves and tags panels had **no title bar at all** while the new dialogs do, and used three byte-identical copies of a 14px-corner shell against the dialogs' 20px; they now share a `panel_title()` helper using the dialog header's own markup and CSS class, and one merged CSS rule. Also corrected an earlier misreading of my own: the series float opens from the **book page**, not the book float, so closing it to the page was always correct |
| 2026-09-03 | Follow-up: the `q`-while-typing guard failed clippy with `E0034: multiple applicable items in scope — multiple \`focus\` found`. Both `WidgetExt` and `GtkWindowExt` define `focus`, and with `gtk::prelude::*` in scope on a `gtk::Window` a bare `.focus()` is ambiguous. Fixed by naming the trait: `gtk::prelude::GtkWindowExt::focus(&key_root)`. Also reordered the focused-widget test to check `gtk::Text` first, since that is the inner widget of a `gtk::Entry` and the one that actually holds focus |
| 2026-09-03 | **Book float: fixed the panel changing size from book to book.** The user reported the floating book details resizing and guessed the tags were behind it — correct, and there were three more causes of the same fault. (1) **Tags** were a `gtk::FlowBox` with `max_children_per_line: 8` holding up to 12 chips, so a book with 9+ tags **wrapped to a second row** and made the panel taller. Per the user's instruction they are now a **single horizontally-sliding line** — a `ScrolledWindow` with `hscrollbar_policy: Automatic`, `vscrollbar_policy: Never` and a fixed 34px height — never two lines. The `.take(12)` cap went with it: the row scrolls, so every tag can be shown. (2) **Title and series** labels had `set_wrap: true`, so a long title took two or three lines; both are now one ellipsised line with the full text in a tooltip. (3) **Authors** are filled by the shared `replace_author_links`, which builds a wrapping FlowBox — right for the book page and reader, wrong here, so the host's height is pinned at the float's own call site rather than changing the helper for everyone. (4) The **biggest** one: the description section's "no Read more" branch left the section completely unbounded (`height_request(-1)`, natural height, no max), so a short blurb gave a short panel and a nearly-long-enough one gave a tall panel; it now reserves the same `DESC_SECTION_HEIGHT` as every other branch. Root cause behind all four: `set_size_request(720, 420)` is a **floor**, not a size — GTK grows a widget past its request whenever content needs the room, so any unbounded child could resize the panel |
| 2026-09-03 | Book float, two follow-ups from the user, both caused by the previous fix. (1) **The action buttons shifted up on a book with no tags.** `fill()` hid the tag scroller when `book.tags` was empty, and a hidden widget occupies no space, so the buttons moved depending on whether the book happened to be tagged. The row is now shown **unconditionally** — it is empty and invisible either way, and its fixed height is precisely what holds the buttons still. (2) **Removed the visible scrollbar from the tag row.** `hscrollbar_policy` changed from `Automatic` to **`External`**: the row still slides by wheel, touchpad and drag, but GTK draws and allocates no bar. `Automatic` was also its own small version of the original bug — it reserved bar space only for heavily-tagged books. `TAGS_ROW_H` dropped 34px → 26px now that no bar has to fit. A belt-and-braces CSS rule hides any scrollbar a theme might still paint, and it does so by making the slider's **background transparent, not with `opacity: 0`** — the warning block at the top of `style.rs` records that opacity on a collapsed scrollbar renders through a zero-sized offscreen surface and trips `pixman_region32_init_rect: Invalid rectangle`. Nearly repeated that exact bug |
| 2026-09-03 | **`docs/pitfalls.md` added** at the user's request — every mistake made in this session (and the earlier A0 ones), each written as *what went wrong → why → what to do instead*, so the next agent does not rediscover them at the cost of a CI cycle or a shipped bug. 15 sections, all verified against the code as it stands. The substantial ones: **error paths that have never executed** (the corrupt-catalog fallback that retried the call that had just failed and `.expect()`ed it — a guaranteed panic that could not unwind out of a GTK callback; and `SELECT MIN(x)` over zero rows returning one NULL row, which every caller's `.ok().flatten()` turned into the right screen by accident); **what a `gtk::Window` was doing for free** (destroying the widget tree, which is what breaks the widget-holds-callback-holds-widget cycle; bounding height; being a real top-level for portal dialogs); **`set_size_request` is a floor, not a size** — the root of the book float resizing, with four separate unbounded children; **hiding a widget removes its space**, which is how fixing that introduced the shifted action buttons; **never `opacity` on a scrollbar** (walked into twice now, caught the second time only by re-reading `style.rs`'s own warning); `PolicyType::External` to hide a scrollbar without losing scrolling; `.focus()` being ambiguous between `WidgetExt` and `GtkWindowExt`; a letter shortcut on the window root firing while you type; dead code failing `-D warnings` after you delete a call site; the empty-state-lie family; and the CI/no-local-toolchain workflow traps (rustfmt races, stale `clippy-latest.txt`, HEAD drift, `$PIPESTATUS`, byte-vs-char line lengths, heredoc asserts). Wired into both "read this first" blocks (README + ROADMAP) as a **numbered step 4**, with a new hard rule — *when you get something wrong, record it in the same commit as the fix* — plus in-code pointers from `style.rs`'s scrollbar block, `in_app_dialog.rs` and `book_float.rs` to the relevant section |
| 2026-09-03 | **Action buttons in the book float still moved when a book had no tags — fixed properly this time, by anchoring rather than by reserving.** The previous attempt kept the tag row permanently visible so it would hold its space; the user reported the shift was still there. Two real causes. (1) The tag scroller had `vscrollbar_policy: Never`, and `Never` is a promise to GTK that the content is fully visible in that direction — so GTK propagates the child's **whole** minimum height and `min/max_content_height` cannot shrink it. Any chip taller than `TAGS_ROW_H` therefore still grew the row. Changed to `External`, which keeps scrolling but lets the row be exactly the height requested, plus a matching `height_request` so request/min/max all agree. (2) More fundamentally, **nothing in the float body expanded**, so leftover height pooled *below* the buttons and their position tracked whatever sat above them. Added a `vexpand: true` spacer immediately before the action row and pinned the row `valign: End`: the slack now collects above the buttons instead, and since the float is a fixed 420px the buttons land in the same place for every book — tags or none, long description or short. Lesson for `docs/pitfalls.md`: reserving space for one variable child only fixes that child; **anchoring the thing that must not move is what actually fixes it** |
| 2026-09-03 | Book float spacing tweak: the anchoring fix left an unpleasant gap between the tag row and the action buttons. Cause was placement — the `vexpand` spacer sat **between** them, so it anchored the buttons but pushed the tags up and away. Moved the spacer **above** the tag row (and flipped the row's `valign` from `Start` to `End`), so the slack now collects above the pair and tags + buttons travel down together, keeping the body's natural 10px spacing between them. Both stay at a fixed position; only the gap closes |
| 2026-09-03 | Book float, three-part polish. (1) **Gap between tags and action buttons closed** — `.kalam-float-actions` padding-top 8px → 0 and the tag scroller's bottom margin 2px → 0, leaving just the body's own 10px. (2) **Description given the spare room** — the blank `vexpand` spacer is gone; `desc_section` now carries the only `vexpand` in the body, so it both pins the tags/buttons to the bottom *and* absorbs the leftover height instead of wasting it. `read_more_btn` gains `valign: End` so "Read more"/"Show less" sits at the bottom of the section rather than tight under the text, and the description scroller no longer caps its own height (`max_content_height` removed in all branches; `DESC_PREVIEW_HEIGHT` deleted as it became unused, which would have failed `-D warnings`). (3) **Long titles no longer widen the panel** — the title now wraps to two lines (`set_lines(2)` + `WrapMode::WordChar`) and, crucially, gains `max_width_chars`. Ellipsising alone never fixed this: a label reports its **full** string as its natural width regardless of ellipsize, and the float's 720px is a floor, so the panel grew to fit. The same cap was missing on `series_val` and on the four left-column fact labels (publisher, published, format, progress location), all of which could widen the cover column — capped and ellipsised, with `Overflow::Hidden` on the column as a backstop. New pitfalls §4c (ellipsize ≠ width cap) and §4d (give slack to something useful, not a blank spacer) |
| 2026-09-03 | **A1 polish finished: keyboard focus is now contained inside in-app dialogs.** New `src/widgets/focus_trap.rs`. A `gtk::Window` is a focus scope — Tab cycles inside it and stops at its edge — but Kalam's dialogs are panels in a `gtk::Overlay`, and an overlay is not a focus scope. The page underneath stayed in the same widget tree and stayed focusable, so Tab walked straight out of a "modal" dialog into the sidebar and the page behind it: you could reach a button you could not see and press Enter on it. The scrim's `can_target` had been blocking the mouse and nothing was blocking the keyboard. The trap is one capture-phase key controller on the window root that owns Tab/Shift+Tab while the panel is visible, advances focus with `child_focus` and wraps at the ends by clearing the root focus and searching again; every other key passes through untouched, Esc included. Capture phase for the same reason the Esc handler needed it — in bubble phase GTK's own focus move has already run. Wired into **both** dialog systems: `in_app_dialog.rs` attaches one per dialog and removes it in `teardown()` (an orphaned controller would keep swallowing Tab for a panel that is gone), while `app.rs` attaches one permanently to `float_host`, which is safe because the trap is inert whenever the panel is hidden. Key matching is split into a pure `tab_direction()` so it is unit-testable without a display — 3 new tests, including one asserting Esc/q/Return are *not* swallowed. Also reconciled `DialogExit`'s doc comment with reality: it claimed the ‹ Back and ✕ variants would arrive when the float headers were revisited, but the floats were deliberately given no visible exit at all, so such a variant would be uncallable dead code and fail `-D warnings`; the floats are also Relm4 components hosted by `app.rs`, not users of the helper. Two variants is the finished set |
| 2026-09-03 | **Book float: "Read more" / "Show less" removed entirely; the description is now always shown in full and scrolls.** The toggle was working backwards — clicking "Read more" made the description *shrink to one line* instead of expanding. Cause: `fill()` runs on every update and called `widgets.desc_scroll.set_vexpand(false)`, silently overriding the `vexpand: true` set in the view. `desc_section` still expanded, but the scroller inside it did not, so it sat at its own minimum; the collapsed branch hid that because `PolicyType::Never` forces a scroller to show its content at full height, and switching to `Automatic` on expand released it back down to nothing. Rather than repair a control that fought the panel's fixed size, it is gone: one scroller, `vexpand` set the same way in the view and in `fill()`, `Automatic` always, full text always. Deleted with it — `BookFloatMsg::ToggleDescription`, the `desc_expanded` model field and its update arm, the `read_more_btn` widget, `description_preview()`, the now-orphaned `truncate_text()`, the `DESC_PREVIEW_CHARS` / `DESC_EXPANDED_HEIGHT` / `READ_MORE_HEIGHT` consts, and the `.kalam-float-read-more` CSS. Every one of those would have been a `-D warnings` failure if left behind, which is the usual tax on removing a feature from a binary crate. `DESC_SECTION_HEIGHT` is now a plain 182px floor. New pitfall §3b: setting a property in both the view and the per-update fill function means the fill function always wins |
| 2026-09-03 | **A0 step 4 started: `src/tasks.rs`, the background-task seam.** Kalam had four ways of doing slow work — bare `thread::spawn`, `thread::spawn` + `async_channel`, relm4 `spawn_command`, and "do it on the UI thread and hope". The new API is one shape: `tasks::spawn(work, on_progress, on_done)`. `work` is `Send`, receives a `Reporter` (progress + a cooperative cancel flag) and has **no** way to reach the UI; `on_progress` and `on_done` are deliberately *not* `Send` and run on the main thread, so they can touch widgets and raise toasts. The compiler therefore refuses to let a worker hold a widget, which is otherwise a bug you find by crashing. Progress is drained to exhaustion before the result is delivered, so a completion toast can never overtake the last progress line. `thread::spawn` + `async-channel` + GLib, no tokio, per the locked decision. **Two real bugs fixed on the way.** (1) `notify::push` is `thread_local!` and builds GTK widgets, but worker threads were calling `notify::error` — the import loop reports a bad file that way — so the toast went into the worker's own empty `HISTORY`/`PENDING` and was **never shown**: silent failure, the exact thing `notify` exists to prevent. `push` now detects a non-main thread and bounces via `MainContext::invoke` (free on the main thread, which is the common path). (2) Dictionary import ran `import_dictionary` inside the file-chooser callback **on the UI thread**, so parsing a StarDict/TSV pack of a few hundred thousand entries froze the window — with the "Importing…" toast painted just before the freeze, making it look like a hang. Now a task. The thumbnail backfill also moved onto the seam and gained a cancellation check, so quitting during a big first launch no longer leaves a thread decoding covers for a window that is gone. `cancel_all()` is wired to `connect_close_request`; `timing::note()` added to report how many tasks were still running at exit under `KALAM_TIMING=1`. 5 new tests, all display-free: progress delivery, the cancel flag, reporting after the UI has gone, `cancel_all` over the registry, and a poisoned registry not taking the next task down |
| 2026-09-03 | A0 step 4 follow-ups, two CI failures fixed. (1) **`-D warnings`: `Update`'s three fields were never read**, because both `tasks::spawn` callers ignored progress with `\|_update\| {}`. Fixed by making both report something real rather than faking a read — the thumbnail backfill now returns how many thumbnails it actually generated (counting only the ones missing beforehand; `backfill_one` returns true for "already present", which would have made the figure just the library size) and logs progress under `KALAM_TIMING=1`, and the dictionary import reports the file being parsed, which replaces the eager toast that used to fire before the work started. (2) **The `notify` thread-safety fix broke two existing tests.** Bouncing the *whole* of `push` to the main context meant that under `cargo test` — where every test runs on its own thread and nothing owns the main context — the message was deferred to a main loop that never runs, so the history stayed empty. Split it: the history entry is recorded synchronously (plain data, readable the moment `push` returns, and `notify::history()` would otherwise be racy) and only the *display* half is bounced. New regression test pushes from a `thread::spawn` and asserts the entry lands in the history |
| 2026-09-03 | **Third `notify` bug, found by the regression test written for the second: the toast history was itself `thread_local!`.** Bouncing the display to the main thread fixed *showing* a worker's message, but `HISTORY` was a thread-local `RefCell`, so the entry was still filed in the worker's own copy — invisible in Settings → Notifications, which reads it from the main thread, and discarded when the worker exited. Background work is exactly where unattended failures happen (a failed import, a bad dictionary pack), so this was the worst half of the app to lose messages from. `HISTORY` is now a process-wide `static Mutex<VecDeque<Entry>>` with a poison-tolerant accessor — refusing to show the notification list because an unrelated thread panicked would be worse than showing it. `HOST` and `PENDING` stay `thread_local!`, which is correct: they are UI-owned. The test now raises from a `thread::spawn` and asserts the entry is readable from the main thread, which is the user-visible property |
| 2026-09-03 | **A0 step 4: the remaining off-thread work moved onto the seam — 9 sites converted, 3 deliberately left alone.** Converted: both metadata searches, the cover search, the two cover fetches and the second-hop description fetch in `metadata_editor.rs`; `series_float.rs`'s fetch; `author.rs`'s profile fetch; and the import loops behind Home and All books. **Not** converted, with reasons: `metadata/mod.rs:273` spawns one thread per source and `join()`s them immediately — it is a fan-out for parallel HTTP inside a worker that is *already* on the seam, not a UI task, and routing it through `tasks::spawn` would add a main-loop round trip to something with no UI to talk to; `notify.rs:504` is a test's thread; `tasks.rs:135` is the seam itself. The import conversion turned up that both pages carried a **byte-identical 40-line copy** of the same loop, so a fix to one silently missed the other — it is now one `spawn_import()` next to the types it uses, and the summary line is one tested `import_summary()` (3 new tests; the rest of an import needs a display). Both imports and the cover search gained a cancel check. The old import path also called `notify::error` **from the worker**; the shared helper now collects failures and raises them in `on_done`, on the main thread, which is where they belong. With `spawn_command` gone from both pages, `ImportProgress` and the `update_cmd_with_view` overrides went with it — progress is ordinary `Input` messages now, one update path per page instead of two |
| 2026-09-03 | **Bug found while migrating, not while looking: overriding `update_with_view` silently disables every `#[watch]` binding.** relm4's default calls `update` then `update_view`; an override replaces both, so unless it ends with `self.update_view(widgets, sender)` the view never re-reads the model. `home.rs` had four live `#[watch]` bindings and called it nowhere — the import status line and the `+ Add books` button's `"Importing…"` label and `set_sensitive` were frozen at their initial values for the life of the page, and only looked right because `rebuild()` repaints the parts it owns by hand. `lookup_history.rs` had the same hole under its "N lookups" count, which never moved after a search or a clear. Both fixed. `book.rs` and `series_float.rs` override too and are fine — neither has a single `#[watch]`, so there is nothing to refresh; recorded rather than "fixed" so the next pass does not re-litigate them. New pitfall §4g |
| 2026-09-03 | **A0 step 5: preloaders (`src/preload.rs`), and with them the half of step 3 that was deferred.** The grid was still decoding *every* cover synchronously while building — step 3 made each decode cheap (a 256×408 thumbnail, not a 1000×1500 cover) but 400 cheap decodes before the first frame is still a stall, and there was no laziness for a preloader to fill. Cards now use `cover_widget_deferred`: an uncached cover gets a placeholder immediately and the frame is recorded, while a worker decodes to raw RGBA. **A `gdk::Texture` cannot cross a thread** — it is a GObject owned by the main thread, and `tasks::spawn` refuses to return one, which is the seam working as intended. What crosses is a `Vec<u8>`; the main thread wraps it in a `MemoryTexture`, which is a pointer copy rather than a decode. That needed a new primitive: `tasks::spawn_stream(work, on_item)` with an `Emit<T>` handle, because a preloader yields results one at a time and `spawn`'s single-result shape cannot express "twenty covers, each visible the moment it is ready". `Emit::send` returns `false` once the UI side is gone, which is a worker's cue to stop. Pending frames are held **weakly** — a page can be destroyed long before its covers finish — and dead entries are reaped when a grid is built, because a cover that never decodes (missing or corrupt file) is never swapped and would otherwise sit in the list for the life of the process. `cover_widget` keeps its old synchronous behaviour for the 13 detail-page call sites, where a placeholder that fills in a moment later would just read as a flicker. *Chapter half:* the reader warms the next chapter's file on book open and on every turn. Only the **read** is preloadable — rendering needs a main-thread `WebView`, and the HTML is built from live theme/font settings, so a cached string would be stale the moment the user changed anything, and a wrong chapter rendered is worse than a slow one. 8 new headless tests |
| 2026-09-03 | **CI: `report_passes_ok_through_and_flags_errors` failed — a race the step-4 `notify` fix created and step 5 exposed.** Moving `HISTORY` from `thread_local!` to a process-wide `Mutex` was necessary (a worker's message has to be readable from the main thread) but it also means the four tests that touch the history now share one, and `cargo test` runs them on parallel threads of a single process. `clear_history()` at the top of a test stopped being isolation the moment the state stopped being thread-local: another test can push between that call and the assertion, which is exactly what happened. Fixed with a poison-tolerant test-only `Mutex<()>` taken at the top of all four. Nothing in `src/preload.rs` was involved — it was a pre-existing race that happened to lose the coin flip on this run. New pitfall §4f2: when you widen the scope of some state, re-read its tests, because they may have been relying on the old scope for isolation without saying so |
| 2026-09-03 | **Made A0 step 5 measurable, because it shipped without a single number attached to the thing it changed.** Added a `grid_build` span around `build_book_grid` plus `grid_cards` / `covers_queued` notes, so `KALAM_TIMING=1` now reports what deferring the covers actually bought — previously the only evidence would have been "does it feel faster". Added `KALAM_NO_PRELOAD=1` (same shape as `KALAM_NO_WEBVIEW_POOL=1`, and unit-tested as a pure function so it needs no env mutation mid-test-run) to A/B the old synchronous behaviour in one session. Note it has to switch off **both** halves: disabling only `warm_covers` would leave a grid of permanent placeholders, so `cover_widget_deferred` falls back to `cover_widget` when it is set. New `docs/testing-a0-step5.md` with the recipes, including the honest caveat that the chapter preloader's effect is invisible on a warm page cache and that a small library may show no `grid_build` gap at all — which would itself be evidence against A0 step 6 (grid virtualization), whose roadmap entry is already gated on "only if the numbers earn it" |
| 2026-09-03 | **First real-machine numbers for step 5, and they found two bugs CI could never have caught.** On the user's Arch box (139 books, release build) the preloader is doing its job: `grid_build` **2752.7 ms → 650.2 ms** cold, 58.8 ms on a revisit. But the screen told a different story than the numbers did. **(1) Only the first two rows of covers ever loaded.** `PRELOAD_AHEAD = 24` was written as a budget — decode what is visible, request the rest on scroll — except the "request the rest on scroll" half was never built, so `ahead_of`'s `.take(24)` was simply a cap and 115 of 139 cards kept their placeholder for the life of the page. That is worse than the synchronous version it replaced, which at least finished. `ahead_of` now returns **every** uncached cover, nearest-first (wrapping past the end to pick up what was scrolled by), and the batching moved into `warm_covers`, which sleeps 4 ms between covers once past the visible batch — the decode is off the UI thread but each *finished* cover swaps a widget on it, and a few hundred back-to-back swaps is its own stutter. **(2) Home's covers never loaded at all.** `warm_covers` was reachable from exactly one place, `build_book_grid`, but `home.rs` and `author.rs` call `build_book_card` directly to build their own strips — so those cards deferred their decode and then waited on a worker nobody had started. Fixed by making `preload::warm_books` the single entry point, so a page cannot be handed deferred cards without also queueing them; it also deduplicates, because Home legitimately shows the same book in two strips. Both bugs shipped through a green CI run: **CI cannot see a placeholder.** New pitfall §16. The stale `assert_eq!(got.len(), PRELOAD_AHEAD, "capped at PRELOAD_AHEAD")` was rewritten rather than deleted — it had been faithfully asserting the bug |
| 2026-09-03 | **A0 step 6 (grid virtualization) moves from "only if the numbers earn it" to justified.** The step-5 measurement was meant to settle that question and it did, in the opposite direction to my earlier framing. 2752.7 ms to build a grid of **139** books, unpreloaded, is ~20 ms per book on the UI thread — and that is a *small* library, well under the 2,000-book figure in the A0 acceptance criteria. Preloading hides the cover decode but not the per-card widget construction: the 650.2 ms that remains is GTK building 139 cards of ~8 widgets each, and it scales linearly, so ~2,000 books implies a multi-second freeze that no amount of background decoding can remove. The honest read is that step 5 raised the ceiling and step 6 is what actually removes it |
| 2026-09-03 | **CI gets a screen: headless sway, a synthetic library, and a 2,000-book scale check.** Two bugs in a row shipped through a green CI run and were caught by a human in seconds — Home's covers never loaded, and only two rows of the grid ever filled — because compiling proves the code builds, not that anything appeared. New `screenshots` job runs the release binary under headless sway (`WLR_BACKENDS=headless`, `WLR_RENDERER=pixman`, `GSK_RENDERER=cairo` since there is no GPU) and uploads PNGs from `grim`. New `scale` job does the same at 2,000 books and records peak RSS. Both are **`continue-on-error: true` and separate from `build`**: they are diagnostics, not gates, and a flaky compositor must never block a correct change — CI's authority stays exactly where it was, on fmt/clippy/test/build. `docs/ci/seed-library.py` writes a synthetic library straight to the catalog schema (no importer, no real EPUBs) and *refuses to run without `XDG_DATA_HOME`* so it can never touch a real `~/.local/share/kalam`; it deliberately includes the untidy states a tidy fixture would miss — no-cover books, an empty author, an empty description, a title long enough to wrap, RTL text. Covers are per-book colours rather than grey, because an all-grey fixture would hide the exact bug being hunted: a placeholder that never fills looks identical to a grey cover that did. Caveats written down rather than discovered later, in `docs/ci/README-screenshots.md`: the Cairo renderer is a fallback and not what a real desktop uses, runner fonts differ so text-wrapping questions still need a human, WebKit may not render under software at all, and runner *timings* are noise (peak memory is the only number worth trusting). The honest one is last: an agent that writes the code, the test and the judgement is marking its own homework — these images are a regression check, not an approval |
| 2026-09-03 | **Third correction to the A0 step 6 (virtualization) estimate, and the reason the estimates kept moving.** Three clean runs on the user's box gave `grid_build` 23.1 / 61.9 / 23.2 ms for 139 cards. The decisive line was a *second* visit to All books in the same session: `covers_queued 0` and `grid_build` **45.1 ms — slower than the 23.1 ms first visit that had 123 covers to queue.** So `grid_build` is no longer measuring cover work at all; it is measuring widget construction plus scheduling noise. Real cost is ~0.17 ms/card, i.e. ~0.33 s at 2,000 books — noticeable, not a freeze. **Step 6 is not justified on build time.** I had previously called it "justified" (from 650 ms) and before that "may not be needed", each time from a single sample; the fault was treating one measurement as a finding, not the data being unstable. What the same analysis *did* surface is a real scaling problem I introduced: `COVER_CACHE` is an unbounded `HashMap` with no eviction, and removing `ahead_of`'s `.take(24)` cap removed the accidental limit on how much it holds. At 128×204 RGBA that is ~102 KB per cover, so a 2,000-book library now decodes and permanently retains ~200 MB. A bounded (LRU) cache is a much smaller change than virtualization and attacks the thing that actually does not scale |
| 2026-09-03 | **First screenshot run: green build, both new jobs failed — which is the design working, and it immediately exposed a hole in my own plan.** The user copied the workflow across and ran it; `build` passed, `screenshots` and `scale` both failed at their screenshot step, and the run still reported success because both are `continue-on-error`. So a broken diagnostic did not block anything, as intended. The hole: **I could not read why.** Artifacts download from a blob host the Arena sandbox cannot reach — the exact limitation that already forces clippy failures to be committed into `ci-logs/` — so I had built a diagnostic whose output I had no way to see, which is the same mistake as shipping a preloader with no timing on it. Both jobs now write a plain-text `report.txt` (binary present, seed result, sway version or its log, window count, the `[timing]` lines, cover analysis, peak RSS, and the app's own stderr tail on a crash) and **commit it back to the branch**, on success and on failure — failure is precisely when it matters. Also added `docs/ci/check-shot.py`: it decodes the PNGs with stdlib zlib (no Pillow, no pip step) and reports what percentage of pixels are strongly coloured, which answers "did the covers load?" as a number, since the seeded covers are saturated and the placeholder is grey. It deliberately reports rather than asserts — a threshold invented before seeing a real run is a guess, and one that fails good builds is worse than none. Verified locally against grey, coloured and black fixtures. The two jobs are now serialised (`scale` needs `screenshots`) because both push a report to the same branch and racing pushes lose |
| 2026-09-03 | **The committed report paid for itself on its first run: two lines of text named the exact cause.** `ci-logs/screenshots-latest.txt` came back with `Cannot find Xwayland binary "/usr/bin/Xwayland"` — sway treats a failed Xwayland start as fatal even though Kalam is a native GTK4 Wayland app that never touches X11. Fixed twice over, deliberately: `xwayland disable` in the generated sway config (the real fix — there is nothing to lose, and it removes the dependency rather than satisfying it) *and* the `xwayland` package added to both jobs' apt lists, because a second round-trip to discover a second missing package is the expensive failure mode here. Reading the report also surfaced the **next** bug before it cost a run: the script launches the binary directly (so its stderr is capturable) but never set `WAYLAND_DISPLAY`, so the app would have found no compositor and exited instantly — indistinguishable from a rendering bug in the resulting empty screenshot. It now discovers sway's socket in `XDG_RUNTIME_DIR`, skips the `.lock` files, exports it, prints it, and fails loudly with a directory listing if none appeared. Socket-picking logic verified locally against a fake runtime dir. This is the difference the report makes: two failures diagnosed and fixed from four lines of committed text, with no artifact download |
| 2026-09-03 | **Bounded the cover cache properly — and corrected my own claim that it had no bound at all.** It did: `if cache.len() > 400 { cache.clear(); }`, in both insert paths. That is worse than no bound in the case that matters, because it discards the covers *currently on screen* along with everything else, so crossing the limit makes the visible grid decode itself again — the cache stops helping exactly where it starts to matter. It had also been dormant: before step 5's fix the preloader stopped at 24 covers, so nothing ever approached 400, and removing that cap (correct on its own terms) quietly turned a sleeping flaw into a live one. Replaced with a real LRU capped at **300** entries (~30 MB at 128×204 RGBA): a hit moves the key to the back, an overflow drops only the front. Two subtleties are now tested rather than assumed — the order list is pruned wherever the map is (a stale key would evict a *live* entry later, surfacing as an unexplained re-decode) and **a probe is not a use** (`is_cover_cached` is the preloader asking whether to decode; counting it would let a background sweep reorder the cache away from what is on screen). Testing needed the cache to become generic over its value type, because a `gdk::Texture` cannot be constructed without an initialised GTK display and CI has none — cheaper than leaving eviction untested. 5 new tests, logic pre-verified against a line-by-line port. New pitfall §17 |
| 2026-09-03 | **A0 step 4's last leftover closed: `install_bundled_dictionaries` is off the UI thread.** First launch decompresses and imports ~6.8 MB of gzipped TSV packs, and it did that *before the window existed* — a new user waited with nothing on screen to explain why. Now on the `tasks::spawn` seam. Nothing on screen depends on it: the dictionary is read when a word is looked up in the reader, which cannot happen before the window is drawn. Deliberately **not** merged into the thumbnail-backfill task that runs beside it — two independent jobs on one worker means the slower delays the faster for no reason, and a failure in one would be reported as a failure of both. Also deliberately has **no cancel check**: the unit of work is a whole pack, and abandoning one half-imported would leave the pref unset and the rows partly written; it is bounded work that ends on its own. The error is returned as a `String` rather than an `anyhow::Error` (Send) and raised via `notify::error` in `on_done`, on the main thread, which is where the notification system can display it (§4e). The honest part: this was in neither the "converted" nor the "deliberately skipped" list in my step-4 report, because I searched for `thread::spawn` call sites when the acceptance criterion was "all slow work off the UI thread" — searching for the mechanism cannot find work that was never threaded. Recorded in §17 |
| 2026-09-03 | **Screenshot harness, round two: Xwayland fixed, and the report's own wording turned out to be lying.** With `xwayland disable` in place that error is gone — but the run still said `FATAL: sway never came up`, this time with an **empty** `sway.log`. No errors at all is not the signature of a compositor that failed to start; it is the signature of one that started fine and could not be talked to. Cause: sway names its IPC socket after its own pid and exports `SWAYSOCK` to processes *it* launches. This script is sway's **parent**, not its child, so it inherited nothing, and `swaymsg` reported the same "cannot connect" whether sway was healthy or dead. Fixed by globbing `$XDG_RUNTIME_DIR/sway-ipc.*.sock` and exporting `SWAYSOCK` before the wait loop — the same class of bug as the missing `WAYLAND_DISPLAY`, and it should have been caught at the same time. The diagnostics were the real defect though, so they were fixed too: the script now distinguishes "the process exited" from "the process is alive but unreachable", prints the socket path it settled on, dumps the runtime directory when there is nothing to connect to, says explicitly when a log is empty rather than printing nothing, and breaks the wait loop early if the process is already dead instead of burning 30 s. **A diagnostic that reports the wrong cause is worse than one that reports nothing**, because it sends the next round of work at the wrong target — this one cost a full round-trip |
| 2026-09-03 | **The screenshot harness finally ran, and the first thing it proved was the dictionary fix.** Report: sway 1.9 up, `WAYLAND_DISPLAY=wayland-1`, one toplevel window, three PNGs captured, and — the number that matters — **`window_shown 156.3 ms` with `startup_dicts 2898.2 ms` printed *after* it.** On a first launch (which every CI run is, since the library is seeded fresh) the dictionary import costs ~2.9 s, and it now lands entirely behind the window instead of in front of it. Before this change that 2.9 s was spent *before the first frame*, so a new user's first launch would have been ~3 s of nothing. This is the first hard evidence for a fix whose whole point is invisible on the user's machine, where `startup_dicts` reads 0.1 ms because the packs installed months ago. The scale run shows the same shape at 2,000 books (`startup_dicts 3727.4 ms`, still after `window_shown 156.0 ms`) |
| 2026-09-03 | **Peak memory at 2,000 books: 234 MB — versus 233 MB at 139. The cover cache bound works, and A0 step 6 is not needed for memory either.** This was the number the `scale` job existed to produce, and it settles the question three rounds of extrapolation could not. A 14× larger library costs **1 MB more**, because the LRU holds at most 300 covers regardless of library size. Without that bound the same run would have been heading for ~200 MB of textures on top. Worth stating plainly: virtualization was proposed to stop the grid from scaling badly, and neither of the two things it would fix — build time (~0.17 ms/card) nor memory (flat) — now scales badly. **A0 step 6 stays closed unless a real user complaint reopens it.** Caveat kept honest: CI never reached the All-books page this run, so 234 MB is a Home-page figure; a grid run may sit higher, though bounded by the same 300-cover ceiling |
| 2026-09-03 | **The harness also caught a defect in itself: three byte-identical screenshots and no `grid_build` line.** The Tab/Tab/Return route never left Home, so every shot photographed the same page and the run still reported success — a harness that cannot tell 'I navigated' from 'I did nothing' is worth very little. Fixed by focusing the window first (a freshly mapped window under a headless compositor does not necessarily hold focus) and, more importantly, by **checking rather than assuming**: the report now greps `kalam.log` for `grid_build` and says outright when the grid was never reached, prints `md5sum` of every screenshot so identical output is visible at a glance, and includes the app's non-timing stderr. The cheapest signal — three files with identical byte counts — was sitting in the report's own directory listing and I read past it |
| 2026-09-03 | **Noticed in the CI report, not yet acted on: `thumbs_backfill_done 130` on a 139-book library and `1882` on a 2,000-book one.** The backfill is generating a thumbnail for *every* book with a cover on a fresh library, which is correct on a genuine first run. But paired with the user's own machine printing `thumbs_backfilled 50/100/139` on **every** launch while `thumbs_backfill_done` never appears, the picture is: real work first time, then a full library listing plus one `is_file()` check per book on every subsequent start, for nothing. Harmless at 139 books; at 2,000 it is a pointless query and 2,000 stat calls per launch. Logged rather than fixed, so it does not get lost the way `install_bundled_dictionaries` did |
| 2026-09-03 | **Thumbnail backfill: stop doing 2,000 stat calls per launch to achieve nothing.** Spotted in the CI report, not by reading the code — `thumbs_backfilled 50/100/…/2000` scrolling past on a library where every thumbnail already existed. The backfill's own comment said "only missing files are generated, so it is cheap after the first pass", which was true about the *generating* and hid everything else: it called `list_books()` (every column of every book **plus** a second query joining `tags`, building a full `Book` per row) and then ran one `is_file()` per book — to read exactly two fields, `uuid` and `cover_path`. Two fixes. New `Catalog::books_with_covers()` returns just those two columns and filters cover-less books in SQL rather than carrying them out of the database to skip them in the loop. More importantly, a skip marker: the book count at the last *complete* pass, so a settled library does nothing at all. A plain "done" flag would have been wrong — an import must re-run — and the count gives that for free. Three cases reasoned through rather than assumed: a **cancelled** pass does not record the marker (it has not verified the rest of the library), a cover that **fails** to thumbnail does not count as covered (or the marker promises a completeness it does not have), and **delete-then-import nets to the same count**, so `delete_book` clears the marker explicitly. Every ambiguous case re-runs: an unnecessary pass costs one query, a wrongly-skipped one costs a book its thumbnail for good. 4 new pure tests on the decision function. New pitfall §18 |
| 2026-09-03 | **Thumbnail backfill skip confirmed on a real library — the one fix CI structurally cannot prove.** The user relaunched a settled 139-book library and the `thumbs_backfilled 50/100/139` ladder was **gone**. Worth stating why this needed a human: every CI run seeds a fresh library, so it is always a first launch and always does the full pass — the ladder appears there and *should*, which means a green CI run says nothing at all about whether the skip works. The evidence for a fix whose entire purpose is that nothing happens can only come from a library that has already settled, and CI does not have one. Same shape as the dictionary move in reverse: that one is invisible on the user's box (0.1 ms, packs installed months ago) and only CI's genuine first run could demonstrate it. Two fixes, two environments, neither able to check the other's. Remaining parts of Test 1c still unrun: that a re-import brings the ladder back exactly once, and that `startup_dicts` now prints after `window_shown` |
| 2026-09-03 | **Backfill re-arm confirmed; the dictionary check turned out to be a test that could not fail.** The user imported five books and the `thumbs_backfilled` ladder returned exactly once, running to `144` — the marker noticing the count changed and re-verifying. No `thumbs_backfill_done` line came with it, which is correct and worth recording: the importer already writes a thumbnail per book, so the pass found all 144 present and generated nothing. Both halves of the skip are now proven on a real library. The same output also showed `startup_dicts 0.6 ms` printing *before* `window_shown 711.6 ms`, which my own test doc had called a failure. The build is fine; the instruction was broken. The line prints when the work finishes, and on a settled machine the packs installed months ago so it early-outs on a pref in under a millisecond — before the window at ~700 ms. Critically **it would have printed before `window_shown` on the unfixed build too**, because sub-millisecond work delays nothing wherever it runs; the check emitted identical output for a correct and an incorrect build, so it never had the power to distinguish them. Rewritten to use a throwaway `XDG_DATA_HOME`, which forces a genuine ~2–3 s install and makes the ordering mean something. New pitfall §19, whose rule is: before writing a manual check, ask what it would print if the bug were still present — if the answer is "the same thing", it is not a test |
| 2026-09-03 | **A0 status corrected in the roadmap — it still said steps 4 and 5 were "not started".** Both shipped days ago, and step 6 was closed on measured evidence, but the summary bullet at the top of the A0 section had never been updated to match the per-step entries below it. This matters more than a normal stale line: the file's own first section orders every new agent to read the roadmap before touching code, and "Current trajectory" is named there as *the* single summary of where the project is. A fresh chat reading it would have set out to build a task manager that already exists. Now states plainly that steps 1–5 are done and CI-green, step 6 is closed with the numbers that closed it (~0.17 ms/card, 233 MB at 139 books vs 252 MB at 2,000), and **only steps 7 (perf-budget CI test) and 8 (plugin-host seam design) remain**. No code changed. Recorded because the failure mode is the same one §15–§19 keep describing: trusting a convenient summary instead of checking the thing it summarises |
| 2026-09-03 | **A0 step 8: the source seam is designed — [`docs/source-seam.md`](./docs/archive/source-seam.md).** P7 (fiction) and P9 (manga) both depend on it, so designing it once beforehand is the entire point; two phases inventing their own shape would mean rewriting one. **The biggest call is one trait, not two.** The roadmap listed `FictionSource` and `MangaSource` separately, but they differ in exactly one place — the last step returns text or image URLs — while searching, pagination, chapter lists, rate limits, the download queue and the follow scheduler are identical. Two traits means writing all of that twice and watching it drift; one trait with a two-variant `Content` enum writes it once, and a future third flavour breaks every `match` until handled, which is the good failure. Three things are load-bearing and are argued rather than asserted. **`ResultPage.has_more` from day one** — without it a search can never reach hit 21, and adding it later changes the return type of the most-used method in the API. **`WorkRef` must be complete enough to draw a result card**, because Tachiyomi's own docs warn that a missing thumbnail triggers an immediate per-row detail fetch — an N+1 over the network, the same bug this repo has now fixed three times in SQL (§16, §18). **`remote_id` must be the site's permanent id, never a URL or title slug**, because the auto-updater re-fetches by it and annotations anchor into what it returns. Also settled: rate limits are declared by the source but **enforced by the host**, since a user-written plugin cannot be trusted to sleep and one bad script gets Kalam's User-Agent blocked for everyone. **Research finding that changed the shape:** `mlua` is `!Send` (raw `*mut lua_State`), and its `send` feature buys thread-safety with a reentrant mutex on every VM access — permanent cost for a problem we do not have. Since `tasks::spawn` demands `Send`, the design sends a `SourceFactory` (path + manifest) to the worker and builds the VM *there*, born and dying on one thread; the feature stays off. Build order is deliberate: **not the Lua host first** — a plugin API with zero implementations is a guess. AO3 native, then MangaDex native, then Lua with AO3 ported as the proof. And the trait **does not land as code yet**: this is a binary crate with no `lib.rs`, so an unimplemented trait either fails `-D warnings` or adds to the 28 existing `#[allow(dead_code)]` escapes; it ships in the same commit as AO3, its first caller |
| 2026-09-03 | **[SUPERSEDED the same day — see the Lua-restored row below; this reasoning was wrong.]** **Lua reversed out of the plan: the user's scale answer removed the reason it existed.** Asked how far extensibility should go, the user said *"sources and metadata and maybe a few more, not an ecosystem, because it's for personal use. plugin system makes sense if there is a community, which isn't the case here."* That is decisive rather than a preference. A scripting runtime solves exactly one problem — **people who cannot compile the app want to extend it** — which is why Yazi, Neovim and Tachiyomi all have one and why Kalam does not need one: two authors, both of whom compile it routinely. Against that non-benefit, `mlua` costs a second language in the debugging path, the loss of type checking (a mistyped field is a 2 a.m. runtime error instead of a CI failure), a sandbox that has to be *enforced* with every hole a security bug, a host API frozen the moment a plugin exists — the exact "second API you must keep stable forever" objection recorded in `conversation.md` §5 — a vendored C interpreter in every build, and the whole `!Send` factory/VM dance in `source-seam.md` §9 that exists *only* to accommodate it. A compiled-in Rust source costs a `.rs` file and a match arm; `src/metadata/mod.rs` has been demonstrating that with two providers for several phases. **Deferred rather than refused**, and the design makes that nearly free: a `LuaSource` would be one more impl of the same `Source` trait, and `SourceFactory` already exists to carry a non-`Send` VM to a worker. Flip conditions written down: a scraped source breaking often enough that recompiling annoys, or a second person writing sources. The honest counter-argument is recorded too — when AO3 changes its HTML, a Lua fix is edit-and-restart while a Rust fix is edit-and-recompile. Also surfaced while listing the surfaces: **two of the four the user named already exist.** Metadata providers (`MetadataSource`) and themes (`Theme`) are extensible today in the only sense that matters here — adding one is a small, isolated, type-checked change. The app is already extensible along the named axes; content sources are the genuine gap, which is A0 step 8. Decided against merging `MetadataSource` into `Source`: they differ in three of four verbs (proposed edits to books you own, vs works you do not have yet), so merging would produce a trait half-full of `Unsupported`; they share the vocabulary (`SourceError`, `RateLimit`, the HTTP agent) instead. P12 renamed from "Lua plugin system" to "Extension surfaces" |
| 2026-09-03 | **[SUPERSEDED the same day — the decision this sweep propagated was itself reversed; see the Lua-restored row below.]** **Doc sweep: nine places still promised Lua after the decision to drop it.** The user asked "these Extension Surfaces, we will be doing it through Lua right?" — a fair reading of the repo at that moment, because the previous commit had updated P12, the A0 step-8 entry and the trajectory list but left the *older* sections untouched. Still standing were `conversation.md`'s standing-decisions list ("leaning Lua"), its §7 bullet ("P12, Lua leaning"), the §8 manga heading and body ("Plugins are **Lua, written by us**"), three rows of the borrow table ("port adapter logic to Lua plugins"), the P12 scope bullet ("ports to Lua"), and the A0 step-8 entry still describing "the Lua host rules". A decision recorded in one place and contradicted in nine is not recorded. All now corrected, with the superseded lines kept and annotated rather than deleted, since the *shape* they describe (one adapter per site, Tachiyomi-like, written by us) was always right — only the language changed. Two "you asked, here is the answer where you will actually see it" banners added at the top of `source-seam.md` and P12, because the answer was previously only reachable by reading to §9a. Also dropped **sandboxing** from P12's scope: it was there to contain untrusted third-party scripts, and with no scripts there is nothing to contain — a compiled-in source is reviewed at merge time like any other code. New §12b spells out the whole cost of adding an extension (one file, one match arm, using the metadata provider that has shipped since P5 as the worked example) because "no plugin system" reads as "not extensible", and the opposite is true |
| 2026-09-03 | **Lua restored to the plan, and metadata is now explicitly in scope for it — the user was right and I had answered the wrong question.** Yesterday's entry removed Lua on the reasoning that a scripting runtime exists for people who cannot compile the app. The user's reply reframed it: *"have you seen how metadata plugins in Calibre work?? there are many, many plugins in Calibre just for metadata sources. I'd say, Open Library and Google Books should be built in, but we can have option to add more sources later with Lua."* Checkable, and it checks out — Calibre's index carries **20+ third-party metadata-source plugins** (Goodreads, Amazon, Kobo, StoryGraph, FictionDB, ISFDB, Douban, DNB, Baen, noosfere, moly.hu, databazeknih.cz, Skoob, Bookline, Lira, Alexandra, Biblioman, Kitapyurdu, SF-Leihbuch), heavily regional and niche, and **almost all HTML scrapers** since Goodreads and Amazon expose no metadata API. Calibre ships a handful built in and lets the endless tail be plugins; that is the model the user is asking for and it is the right one. **The error was mine three times over.** (1) I converted a *scope* answer ("not an ecosystem, it's for personal use") into an *implementation-language* decision, renamed the user's P12 phase and wrote "deferred, probably indefinitely" — an ecosystem is about other people, a runtime is about how fast you can fix a broken parser, and the second applies to one developer as much as to a thousand. (2) I under-weighted breakage: Tachiyomi's entire extension architecture exists because *"extensions are parsers; if a website changes its structure, the extension breaks — the core app stays stable, extensions change constantly."* (3) I asserted a rebuild was "a few minutes" without opening `Cargo.toml`, where `[profile.release]` is `lto = true` + `codegen-units = 1` over 44k lines and 36 deps — the slowest configuration there is, a full relink for a one-character selector change, and the one most at risk of an OOM kill on the user's 4 GB box. **The dividing line is now "does this break when someone else changes their website", not "is it a source".** Built-in Rust: Open Library and Google Books (documented JSON APIs, already shipping), MangaDex/Komga/Kavita/OPDS, themes, export formats, dictionaries. Lua: AO3, FFN, Royal Road, scraped manga, and the add-on metadata tail. One host, one sandbox, one loader serving both `Source` and `MetadataSource` — which is where the two traits genuinely share machinery, though they stay separate traits (three of four verbs differ). **Sandboxing returns to P12 scope** now that untrusted scripts are in play again. **Sequencing unchanged and now load-bearing:** AO3 lands natively first, then the Lua host with AO3 ported as its proof — an API designed against zero implementations is a guess. Corrected across `source-seam.md` (§0 banner reversed, §9a rewritten with the Calibre and Tachiyomi evidence, §10 sandbox rules restored to enforced, §11 build order regains the Lua step, §12a surface table split built-in/Lua with the "why metadata splits down the middle" argument, §12b rewritten as two routes), `ROADMAP.md` (P12 back to "Lua plugin system", the "No Lua" banner deleted, sandboxing reinstated, trajectory + phase map + A0 step 8 entry), `ARCH.md`, `docs/conversation.md` (§5 Q1/Q2 rewritten, §6 standing decisions, §7, §8 note, three borrow-table rows). New **`docs/pitfalls.md` §20 — "Answering a question the user did not ask"**: when an answer settles one variable, change only that variable; if a second decision seems to follow, say so and ask |
| 2026-09-04 | **Self-review of the whole tree — 14 findings, written up in [`docs/review-2026-09-04.md`](./docs/archive/review-2026-09-04.md).** Not prompted by a bug; a deliberate read of ~44k lines looking for the classes of defect CI cannot see. Two were security-shaped, four were correctness, the rest performance and hygiene. Worth stating the honest caveat up front, because it applies to every row below it: **this sandbox has no Rust toolchain**, so nothing here has been compiled, clippy'd or tested. Every change was made by reading, and the CI run is the first thing that will actually check it. Findings triaged and fixed in severity order rather than file order, so the two that could damage a user's machine landed first |
| 2026-09-04 | **Zip-slip and zip-bomb: an EPUB could write outside the library directory.** `extract_zip` joined `file.name()` straight onto the destination, so an archive entry named `../../.bashrc` — or an absolute path — escaped the book directory, and an EPUB is a file the user downloads from strangers. Fixed with `enclosed_name()`, which is the `zip` crate's own answer to exactly this and returns `None` for anything that climbs out. **Hostile entries are skipped and logged, not treated as fatal**: a real book with one odd entry should still open, and refusing the whole import would turn a hardening fix into a compatibility regression. The second half is resource exhaustion — the same extract had no ceiling, so a 2 MB archive that inflates to 100 GB filled the disk. Now `MAX_EXTRACT_BYTES = 512 MB` enforced through `Read::take` (the partly-written file is removed on overflow, so no truncated book is left looking valid) and `MAX_EXTRACT_ENTRIES = 10_000`. 5 tests: traversal, absolute escape, the size ceiling, the entry cap, and — per pitfalls §19 — a happy-path case asserting a *normal* book still extracts intact, because four tests that only prove things get rejected would pass on a function that rejects everything |
| 2026-09-04 | **A panic on any multi-byte definition: `truncate_def` sliced a `String` by bytes.** `&s[..n]` panics when `n` lands inside a UTF-8 sequence, and the dictionary surfaces this constantly — an accented word, a CJK gloss, a curly quote in an English definition. The reader would take the whole app down mid-lookup. Now char-based. 4 tests walking cut points 0..12 across multi-byte content, which is the shape that would have caught it. Same file, same commit: the `&cols[0]` index in `dict.rs` that panicked on a SQLite pack whose entries table had unexpected column names is now a `pick(&[names])` closure returning a real error |
| 2026-09-04 | **Reading sessions leaked on every crash, and the streak was computed in UTC — two bugs whose combined effect is that the stats page lies.** A session row is opened when a book opens and closed when it closes; if the app dies in between, `ended_at` stays NULL forever and that book shows as permanently open. Two fixes, deliberately separate. `close_orphaned_sessions()` runs at startup and sets `ended_at = started_at` on every open row — note it does **not** invent a duration, because a made-up number is worse than a zero-length session in the one place the user goes to see real numbers. And `checkpoint_reading_session()` writes elapsed seconds as reading proceeds (leaving `ended_at` NULL), so a crash loses at most the last tick instead of the whole sitting. The clamp comment was corrected while in there: `Instant` is `CLOCK_MONOTONIC`, which does *not* advance across suspend, so the old comment's justification was backwards even though the clamp is still worth having. 4 tests, including the one that matters — crash *after* a checkpoint, then reap, and assert the checkpointed seconds survive |
| 2026-09-04 | **Reading statistics bucketed by UTC day, which breaks streaks for anyone not on UTC — full reasoning in [`docs/conversation.md`](./docs/conversation.md) §15.** At UTC+05:30 every session between local midnight and 05:30 was filed under the previous day: holes in the 14-day chart, and a genuinely-earned streak reported as broken. Timestamps **stay stored as UTC** — storing unzoned local time is unorderable across a move and silently corrupts history. Only bucketing moves. The offset comes from SQLite itself (`strftime('%s','now','localtime') - strftime('%s','now')`) rather than a new `chrono` dependency, cached in a `OnceLock`; the accepted cost is that a DST change mid-session is wrong until restart. The non-obvious call: the SQL embeds a literal `'{offset:+} seconds'` modifier instead of `datetime(col,'localtime')`, so the SQL bucket and the Rust label share **one** offset and cannot disagree — two sources of truth for where the day starts would show up as a chart bar that is permanently empty because its label never matches a bucket. Applied across `stats.rs`, `history.rs` and `metadata.rs`; `added_by_month` deliberately left UTC. **The tests inject both the clock and the offset**, because a test calling the real functions would pass in a UTC CI container and fail on the developer's IST machine — pitfalls §19 again: it would have passed on the broken code |
| 2026-09-04 | **The dictionary importer built the entire pack in RAM before writing a row.** A 500k-entry pack accumulated every word and definition in a `Vec` and inserted at the end — hundreds of MB of peak RSS on a machine with 4 GB, and a failure at 99% threw away all of it. Replaced with an `EntrySink` that flushes every 2,000 rows inside a transaction. Two details that are easy to get wrong and are handled explicitly: the dictionary's meta row is written with count 0 and **corrected in `finish`**, so an interrupted import cannot leave a row advertising entries it does not have; and any error calls `abort`, which deletes the dictionary row rather than leaving a half-populated dictionary in the picker. Progress is now reported per batch, so the settings page shows a moving count instead of a spinner |
| 2026-09-04 | **`hydrate_books` built its `IN (…)` list by formatting integers into SQL.** Not injectable — they are `i64`s straight from SQLite — so this is a hygiene fix, not a vulnerability. It is worth doing anyway: the two functions either side of it (`books_by_ids`, `finished_ids_among`) bind placeholders, and an inconsistency in a security-relevant pattern is how the genuinely-injectable version gets written later by someone copying the wrong neighbour. Now `?` placeholders and `params_from_iter`, matching the existing `vec!["?"; n].join(",")` idiom used elsewhere in the file |
| 2026-09-04 | **The catalog was opened three times per startup; now once.** `main()` opened it to verify the database, `startup_theme()` opened it again to read one preference, and `AppModel::init` opened a third — each running the whole of `migrate()` (its CREATE TABLE batch, eight `PRAGMA table_info` probes, two `COUNT(*)`s over the dictionary tables). Beyond the ~12 ms, first launch had two connections racing to build the merged dictionary store. `AppModel::Init` changed from `()` to `Arc<Catalog>` and the handle is threaded from `main`. The `startup_db_open` span is kept on the surviving open so the A0 numbers stay comparable. **The pre-existing fail-before-`app.run()` structure is deliberately preserved** — `init()` runs inside a GTK callback where a panic cannot unwind, so a database error there aborts with a core dump instead of a readable message |
| 2026-09-04 | **Three smaller correctness fixes.** (1) **CSV export had no formula guard.** Excel and LibreOffice evaluate any cell starting `=`, `+`, `-` or `@`, and dictionary content hits this innocently and constantly — suffix headwords like `-ness`, definitions written as `- to do X`. Those cells showed `#NAME?` instead of the definition, and a crafted pack could go further. Now apostrophe-prefixed and quoted, with a test asserting ordinary fields are **not** touched, since the obvious over-fix puts a stray apostrophe in every cell. (2) **`find_zip_index` was O(n) with two allocations per comparison**, called up to three times by `extract_cover` — thousands of allocations to locate one cover in a large EPUB. Now resolved off the already-parsed central directory via `file_names()`. It returns the archive's own **name** rather than an index: resolving to an index would silently couple the lookup to `file_names()` yielding entries in `by_index` order, and the symptom of that assumption breaking would be reading the wrong file out of the book. (3) **`change_token` is `sqlite3_total_changes`, a C `int` that wraps at ~2.1 billion.** Left as-is but now documented as an opaque equality token: equality survives a wrap apart from one unlucky repeated value costing a single stale page, whereas an ordering comparison would go permanently wrong |
| 2026-09-04 | **The test suite was inverted, and the largest parser had none — `src/epub.rs` now has 10.** Coverage was concentrated in pure helpers while the code that parses files produced by other people's software had zero, which is precisely backwards: leniency is where correctness is hard, and a regression there is invisible until a reader opens a book and finds it blank. Every case is a shape real EPUBs have — EPUB 2's indirect `<meta name="cover">` id versus EPUB 3's `properties="cover-image"`, `properties` as a space-separated list that must not match by substring, a `<title>` outside `<metadata>` that must **not** become the book title, percent-escaped and `..`-climbing hrefs relative to an `OEBPS/` OPF, Windows separators, and covers sniffed by magic bytes because a PNG named `.jpg` is routine and GTK refuses the texture if the extension lies. Two are honest rather than flattering: malformed XML is asserted to **fail** (a truncated download must not silently import an empty book), and `strip_html` is pinned to its current run-paragraphs-together behaviour with a comment explaining why inserting spaces would break mid-word `<i>italics</i>` — recording what the code does, not what would read better. Also instrumented rather than guessed: the page cache now counts hits and misses under `KALAM_TIMING=1`, because it invalidates on `total_changes()` and the reader's per-1%-scroll progress saves should be evicting everything — worth measuring before it is either tuned or deleted. The three module-level `#![allow(dead_code)]` escapes each gained a justification and a narrowing plan (and `dict.rs`'s was fixed: it sat on line 2, splitting the module doc comment in half) |
| 2026-09-04 | **CI went red on a dependency, not on the commit — `tinyvec` 1.13.0 does not compile.** The self-review commit came back failing with `cannot find macro `vec` in this scope` at `tinyvec-1.13.0/src/tinyvec.rs:710`, in a crate this project never names: it arrives via `unicode-normalization`, which backs the NFD headword keys. Nothing in the commit touched it. Pinned to `>=1.6, <1.13` with a note to remove the pin once upstream fixes it. **The pin is a patch; the actual defect is that `Cargo.lock` is gitignored.** For a binary crate that means every CI run re-resolves against whatever crates.io published most recently, so the build is a lottery run against the whole ecosystem — a bad release in any transitive dependency shows up as "your change broke the build", pointing the investigation at the diff, which is the most expensive possible way to find out that the diff is innocent. It also means no two builds are reproducible and a green run yesterday says nothing about today. Cargo's own guidance is to commit the lockfile for binaries; worth doing, and left as a deliberate separate decision rather than folded into an unrelated commit. Also worth recording as the process lesson: this sandbox has no Rust toolchain, so the review commit was pushed uncompiled and CI was the first check — which worked exactly as intended (rustfmt passed, meaning all the new code parses), but the first red run cost a round trip to establish it was not our fault |
| 2026-09-04 | **`Cargo.lock` un-ignored — the fix for the `tinyvec` incident, and it needs one manual CI step to take effect.** Ignoring the lockfile is right for a library (downstream consumers pick their own versions) and wrong for a binary, which Kalam is. The cost showed up the same day: a self-review commit that touched no dependencies came back red on `tinyvec 1.13.0`, a crate the project never names, pulled in transitively by `unicode-normalization`. Nothing was wrong with the diff — CI had simply re-resolved the graph and picked up a release published since the last run. That is the general failure: **the build is a lottery against everything crates.io publishes**, a green run yesterday guarantees nothing today, no two builds are reproducible, and when it breaks the evidence points at your commit, which is the most expensive possible way to discover your commit is innocent. **The agent cannot finish this one.** Generating a lockfile needs `cargo generate-lockfile`, and the Arena sandbox has no network route to crates.io (nor a Rust toolchain), so the file cannot be produced locally. The workflow step that produces it is written and sitting in [`docs/ci/README.md`](./docs/ci/README.md) under a heading marked ACTION NEEDED, because the agent also cannot push `.github/workflows/`. Two agent-side blocks on one two-line change is worth recording in itself. **Until that step is applied nothing has actually changed** — `.gitignore` no longer lists the file, but no lockfile exists to be committed, so CI still re-resolves. The `tinyvec` pin stays in `Cargo.toml` as the stand-in and should be deleted once the lock lands, since the lock is the thing that properly holds versions. Also worth stating plainly: committing the lockfile does **not** freeze dependencies forever. `cargo update` still moves them; the difference is that it moves them when someone decides to, in a reviewable commit, rather than silently between two runs of the same code |
| 2026-09-04 | **`Cargo.lock` is now committed, and the `tinyvec` pin is gone.** The workflow step landed (thank you), CI generated the lockfile on its next run and pushed it back as `d8c56b5`; the graph resolved to `tinyvec 1.12.0`, i.e. the last good release, exactly as intended. With the lock holding versions the `tinyvec = ">=1.6, <1.13"` line in `Cargo.toml` was doing nothing except stating a constraint in a second place, where it would eventually contradict the first — removed, as promised when it went in. Builds are now reproducible: the same commit resolves to the same dependency graph tomorrow as today, and a dependency only moves when someone runs `cargo update` in a reviewable commit |
| 2026-09-04 | **`strip_html` ran paragraphs together, and the `<br>` handling had never worked at all.** Removing a tag left nothing behind, so `<p>A great book.</p><p>Really.</p>` rendered as `A great book.Really.` on the book page — publishers usually write descriptions as one line of HTML with no newline between paragraphs, so nothing else supplied the gap. **Both obvious fixes are wrong**, which is why this needed more than a one-liner: "never insert a space" is the bug, and "always insert a space" turns `<i>bene</i>volent` into `bene volent`, because inline emphasis lands inside words. So the tag name decides — block-level (`p`, `div`, `li`, `br`, `h1`–`h6`, table parts, …) yields a space, everything else yields nothing, and **unknown tags are treated as inline**, the safer default: a missing space between sentences is ugly, a space inside a word is a misspelling. Found while writing the tests: the function ended with `.replace("<br/>", " ")` and `.replace("<br />", " ")`, which ran *after* tag stripping had already removed every tag — they could never match, so the line break they existed to preserve was silently dropped. Someone hit this bug before and fixed it in the wrong place. Also fixed in passing: `&amp;` is now decoded **last**, because decoding it first turns the literal text `&amp;lt;` into `&lt;` and then into `<`, fabricating markup the author escaped specifically to avoid. Tests went from 1 to 5 and assert the values, not the absence of a panic |
| 2026-09-04 | **The screenshot job now asks the app where to go instead of guessing — new `KALAM_ROUTE` env var.** The harness had no way to say "open All books", so it sent Tab/Tab/Return and hoped the focus order matched. It did not: every run since the job was built photographed Home three times, including the run that reported the dictionary fix as verified. The report *said so* ("DID NOT REACH the grid"), which is the §19 self-reporting working — but the fix was left as "brittle by design", so the visual coverage the job exists to provide has never actually existed. `KALAM_ROUTE=all-books` navigates once the window is realized, alongside `KALAM_TIMING` / `KALAM_NO_CSS` / `KALAM_NO_PRELOAD` in the existing diagnostic-env-var convention — no argument parser, no new public surface to keep stable. Names are the words a person would say (`all-books`, `settings`, `analytics`), deliberately **not** the `Debug` spelling of the enum, so renaming a Rust variant cannot silently break every caller; an unknown name prints the known list and is ignored, because a typo in a CI script should not look like a crash. 4 tests, one of which asserts every advertised name resolves — a help message listing names that do not work is worse than no help. **The more important half is that the harness now fails loudly.** Three independent checks must agree: the app logged that it accepted the route, `grid_build` proves a grid actually rendered, and — the one that would have caught this on its own — `01-home.png` and `03-after-nav-settled.png` are compared and must differ. That evidence was already in the report as three identical `md5sum` lines and was read straight past, which is the real lesson: the check existed, nothing was gated on it |
| 2026-09-04 | **The navigation fix worked; the check I wrote to verify it was wrong, and said FAILED on a working run.** First run with `KALAM_ROUTE=all-books`: route accepted, `grid_build 29.0 ms`, `grid_cards 139`. The page genuinely rendered — the thing that had never once happened before. But the report also said *"FAILED — 01-home and 03-after-nav-settled are byte-identical"*, because I kept the old comparison after changing what it was comparing. `KALAM_ROUTE` navigates at startup, so the shot named `01-home` **is already All-books**; all three shots match because they are correctly the same page. The check had encoded an assumption from the Tab/Tab/Return era that the fix itself made false. Two corrections. Files are now named after the route (`01-all-books`, not `01-home`) — the old names made the report lie twice, describing the wrong page *and* making a correct result look like a failure. And the differ-check now means something: it **relaunches with `KALAM_ROUTE=home`**, photographs Home as `04-home-for-comparison`, and asserts the requested page is not pixel-identical to it. That is the check that catches "the log claims it navigated but the screen never changed" — which the previous version could not, since it only ever compared a page to itself. The `grid_build` assertion is now conditional on the route actually being a grid, so `ROUTE=settings` does not cry wolf. Worth recording as the pattern rather than the incident: **when you fix the thing a test was watching, re-derive what the test now proves.** I changed the mechanism and carried the old oracle across unexamined, which is a close relative of §19 — the check ran, printed a confident verdict, and the verdict was noise |
| 2026-09-04 | **Owed write-ups paid: new pitfall §21, and the stale A0 step-7 claim corrected.** Two pieces of bookkeeping the repo's own hard rules require and I had skipped. **§21 — "when you fix the thing a check was watching, re-derive what the check proves"**, recording that my first `KALAM_ROUTE` verification reported `FAILED` on a run that had worked perfectly: the comparison asked "is the last shot different from the first", which was the right question while navigation happened *mid-run*, and became meaningless the moment the fix made the app navigate at *startup*. I changed the mechanism and carried the old oracle across unexamined. Filed as a sibling of §19 — that check couldn't tell a fixed build from a broken one by staying silent, this one couldn't either but cried wolf instead, and a check that fails on correct code is worse than no check because it teaches people to ignore it. **The A0 step-7 entry** asserted "CI has never reached the grid — `grid_build` appears in zero of the seven committed reports". True when written, false now: the 2026-09-04 run records `grid_build 28.9 ms` and `grid_cards 139` on a page proven distinct from Home. Corrected in place rather than deleted, because the *recommendation* it leads to (assert query counts, not milliseconds) is unaffected — the runner's ~60% timing variance is still the reason. What changed is only that step 7 is no longer blocked on CI being unable to reach the page. Flagged because `ROADMAP.md` tells every new chat to trust "Current trajectory" as *the* summary, so a stale line there sends someone off to solve a problem that no longer exists — the same failure recorded on 2026-09-03 when the A0 summary still said steps 4 and 5 were "not started" |
| 2026-09-04 | **A0 step 6 (grid virtualization) REOPENED — peak memory at 2,000 books is 502 MB, not the 252 MB that closed it.** Full reasoning in [`docs/conversation.md`](./docs/conversation.md) §16. Step 6 was closed on 2026-09-03 on the finding that memory is *flat* in library size (233 MB at 139 books vs 252 MB at 2,000), so the grid does not scale badly and virtualization buys nothing. Those numbers were not mismeasured — they were **measured on a page that was never open.** The screenshot harness had never successfully navigated, so every run sampled Home. Fixing that (`KALAM_ROUTE`) changed the answer immediately: same job, same build, **226 MB before the grid was reached and 502 MB after**, with 255 MB at 139 books. Memory roughly doubles with library size, ~0.15 MB per card, for cards overwhelmingly off screen. **The cover LRU is not the cause and the earlier analysis of it was right** — that is precisely why this hid so well: a real bound was doing real work next to an unbounded cost, and made it invisible. The unbounded cost is `build_book_grid`, which constructs and attaches a widget tree for **every** book before returning. `grid_build 421 ms` at 2,000 books is now measured rather than extrapolated. **Not fixed in this pass, deliberately**: swapping `GtkGrid` for a recycling view touches the most-used screen in the app, the user is the only visual QA, and the approach should be agreed first. Corrected in all four live places (A0 summary, the step-6 entry, the step list, the phase-map diagram); the historical changelog rows are left standing as the record of what was believed when. **The lesson:** three roadmap entries revised the step-6 estimate, each more confident than the last, all three reasoning about output from a harness that never ran the code in question — and the tell (`grid_build` absent from every report) was in front of us each time |
| 2026-09-04 | **A0 step 7 shipped — perf budgets that count SQL statements, not milliseconds.** The step as written said "seed 2,000 books; assert grid build under N ms", and that test could not have worked: the runner's wall-clock varies ~60% between identical runs (byte-identical dictionary work measured 2774 ms and 3740 ms on consecutive runs), so any ceiling loose enough to survive the noise would sail past a 2× regression. Worse, **slowness is not the bug that keeps happening here.** All three perf incidents in this repo — pitfalls §16, §18, and the cover preloader — were the same shape: a loop issuing one query per row. That is nearly invisible in a timing on a small library and fatal on a large one. So the budgets count statements. Six of them in `src/perf.rs`, using rusqlite's `trace` hook via a new `Catalog::count_queries`, and **deliberately not `#[ignore]`d** — unlike the two timing probes they sit beside, they run on every `cargo test` and gate CI, which is the entire point of a budget. **The assertions are comparisons, not literals.** Each runs the same call against a 20-book and a 60-book library and requires the counts to be *equal*; the absolute number is printed, never asserted. Hard-coding `assert_eq!(n, 2)` would bake in a free implementation choice (whether tags hydrate in one query or two), so a legitimate refactor would fail for no reason and teach the next person to edit the number until it passes — the count comparison can only be satisfied by genuinely not querying per row. Also covered: `library_stats` must serve its second call from the memo (0 statements) **and** must invalidate after a write, because a memo that never expires shows stale numbers on the dashboard. **Verified by sabotage, per §19** — a check that has never failed is not known to work. `hydrate_books` was temporarily reverted to a per-book tag query and pushed: CI went red on 4 of the 6 budgets while the other 270 tests passed, which is precisely the intended blast radius. Reverted in the following commit. This also clears the way for A0 step 6: grid virtualization now has a machine-independent before/after to be judged against, and **step 6 is the only A0 item left open** |
| 2026-09-04 | **Standing instruction added: write and speak in plain, simple English.** The user, after a reply about grid memory: *"I can't understand anything you said. I need you to use easy and simple language to explain things. Infact add this instruction somewhere so that future chats will also follow it."* Recorded as a hard rule in the "Read this first" block, with a pointer from the working agreement, so it survives into every future chat rather than lasting one conversation. The rule spells out what actually went wrong, because "be clearer" is too vague to act on: **explain the thing before naming it** (not "the `PENDING_FRAMES` weak-ref map breaks under recycling" but "each cover picture remembers which book it belongs to; if pictures get reused for different books, a slow cover can land on the wrong one"), **no unexplained jargon** (*virtualization*, *refcount*, *GObject*, *N+1*, *LRU* mean nothing on their own), **short sentences with one idea each** — which was the real problem more than the vocabulary was — **lead with the answer** instead of making the user read the analysis to reach it, **give numbers meaning** ("507 MB, which is a lot on a 4 GB machine", not "507 MB peak RSS"), and **make choices concrete**: what changes on screen, what could break, how long it takes. Explicitly scoped: this is about *communication only*. It does not lower the bar for the engineering, the testing or the documentation — keep the caveats and the numbers, just say them in words that do not need a glossary |
| 2026-09-04 | **A0 step 6 first cut: the book grid now builds only the rows you can see — switched off by default, needs a look on a real screen.** Turn it on with `KALAM_WINDOWED_GRID=1`. At 2,000 books the old grid builds 2,000 cards before the page can appear (396 ms measured) and holds ~232 MB, almost all of it cover images kept alive by cards scrolled far off screen. This builds about 48: the rows in view plus three rows of margin above and below. **The first plan for this was wrong and was thrown away.** It assumed GTK's own recycling grid (`GridView`), which insists on owning the scrolling — that would have meant restructuring the page so the header and search bar stayed fixed while only the books scrolled, plus care to avoid two scrollbars. The user's reaction to that was immediate and correct: they want the page to stay as it is. So this keeps the plain `GtkGrid` exactly where it sits, in the same box, in the same page, inside the app's existing scroller. **Nothing about the layout changes** — same 6 columns, one scrollbar, header still scrolls away with the page. The trick is a spacer sized to the full height the grid would have had, so the scrollbar is the same length and in the same place; the grid then listens to the scroller it is already inside (found by walking up the widget tree, so the three call sites needed no edits at all) and mounts and unmounts cards as rows come in and out of view. Two risks the user has to judge, both visible only on a real screen: blank patches if a fast flick outruns the mounting, and covers arriving late into a slot. The overscan margin addresses the first; the second turned out to be smaller than feared, because cards are discarded rather than reused and the pending-cover list is keyed by cover path, so a late decode cannot paint onto a slot that now belongs to a different book. 7 tests on the pure row arithmetic — it is the part that can be wrong in an interesting way and needs no display: viewport coverage checked at every scroll offset across the whole range, no skipped rows between consecutive positions, both edges (negative overscroll, past-the-end), empty and small libraries, and the case where GTK has not laid out yet and reports a viewport height of zero, which if treated as "nothing is visible" would open the page blank |
| 2026-09-04 | **Windowed grid v1 was broken on a real screen; rebuilt on `GtkFixed`.** The user switched it on and found three bugs immediately: the page scrolled about twice as far as it should with blank space past the books, some books were missing, and the covers and scrollbar jumped while scrolling. All three had one cause, now written up as pitfall §22. v1 kept the existing `GtkGrid` and added a tall spacer widget in the last row to hold the full height open. But **a grid row is as tall as its tallest child**, so the 6,532 px spacer made the last *row* 6,532 px tall on top of the 23 real rows above it — 6,796 px of books became 13,064 px of scrolling. The spacer also occupied a real cell (column 0 of the last row), so the book belonging there had nowhere to go; and rows with no mounted cards collapsed to zero height, so the total height shifted under the scrollbar as cards came and went. The general rule: **a container that derives its size from its children cannot be used to virtualize those children** — the premise is "most children do not exist", and no arrangement of spacers fixes that, since a spacer is just another child feeding the same calculation. v2 uses `GtkFixed`: every card is placed at an explicit x/y and the total size is set once from the book count, so geometry depends on the number of books and never on what is mounted. Verified pixel-identical to the existing grid — 144 books gives 6,796 px both ways. **The lesson about the testing is the part worth keeping.** The row arithmetic had seven tests and was right the whole time; the pixel arithmetic had none, because it lived inside a function that needs a display and was written off as untestable. It was not — `grid_height()` and `card_position()` are pure integer functions, and pulling them out gave 7 more tests that **fail against v1** (13,064 px vs 6,796 px). Having tests for one half of a change is not having tests for the change, and the half that had them was the half I found interesting rather than the half most likely to break |
| 2026-09-04 | **A0 step 6 shipped and is now the default — the All-books page at 2,000 books went from 502 MB to 247 MB and from 434 ms to 12 ms.** The user checked the rebuilt (`GtkFixed`) version on a real screen and confirmed all three earlier bugs were gone: the page ends where the books end, every book is present, and the scrollbar stays steady while scrolling. That check is the one thing CI cannot do, so it was the gate. The switch flipped from opt-in `KALAM_WINDOWED_GRID=1` to opt-out **`KALAM_NO_WINDOWED_GRID=1`**, matching the shape of `KALAM_NO_PRELOAD` and `KALAM_NO_WEBVIEW_POOL` — an escape hatch is only useful if it works like the other escape hatches. Measured side by side on one machine in one run: 2,000 cards → 48, 434 ms → 12 ms, 502 MB → 247 MB, with the screenshot check reporting an identical 21.7% coloured-pixel count, i.e. the page looks exactly the same. **One trap avoided while flipping the default:** the installed workflow passes `WINDOWED=1` on one run and nothing on the other, which was right while windowed was opt-in — but "nothing" now means *windowed too*, so both runs would have measured the same thing and published a green, meaningless comparison. That is pitfall §21 exactly (change the mechanism and a check watching the old one quietly stops proving anything), caught this time by asking what the check would report *after* the change rather than after it had already lied. Rather than ask for another manual workflow install, `screenshot.sh` now infers the baseline from the output directory, so the installed copy and the updated one in `docs/ci/` are both correct — and `docs/ci/README.md` now has no outstanding ACTION NEEDED items. **This closes A0 except for step 8** (the plugin-host seam), which is designed in `docs/source-seam.md` and deliberately lands with AO3 in P7 |
| 2026-09-04 | **P6.5 created — libraries land before P7.** Calibre-style: pick the folder, keep several, switch between them, copy one to another machine and it opens. Ordered before P7 deliberately, because P7 *adds books from new places* and this changes *where books live* — do both at once and a missing fic could be either one's fault. The user drew a distinction the roadmap had blurred: Kalam's shelves are Calibre's **virtual** libraries (a saved view over everything), while a real **library** is a hard wall — fanfiction in one, technical books in another, genuinely absent from each other. Both exist afterwards; shelves are unchanged. Reading the code first turned up two things that make this smaller than it looked: **nothing machine-specific is stored** (the `books` table holds `"book.epub"` and `"cover.jpg"`, never absolute paths — those are rebuilt at read time, so a Kalam folder is *already* portable), and **every path derives from one function**, `data_dir()`. The real work is elsewhere: the library list must live *outside* the libraries (a setting stored in a library cannot be read before the library is found), and per-library must be split from global — books and highlights travel, dictionaries and theme must not, or you reinstall dictionaries per library. Also in scope: a `kalam.json` per book folder as a **backup copy**, database still authoritative and never read from it during normal use, plus a rebuild-from-folders command — that is what makes a library self-describing if `catalog.db` is ever lost. JSON not OPF, because OPF cannot express highlights or reading sessions without abuse and nothing else reads a stray `metadata.opf`. Out of scope: two machines using one library at once. Discussion in [`docs/libraries-and-portability.md`](./docs/archive/libraries-and-portability.md) |
| 2026-09-04 | **P7 sources settled: AO3 → Royal Road → Literotica → FFN, each proving something different.** Webnovel dropped — nearly everything worth reading sits behind their coin paywall, so a downloader gets a few free chapters and stops, and bypassing a paywall is out of scope. The ordering is deliberate rather than by popularity: **AO3 needs no text parsing at all**, because `download.archiveofourown.org/downloads/<id>/fic.epub` is a real EPUB that AO3 builds with Calibre and lists on their own FAQ, so scraping AO3 is only for *finding* things — which means AO3 alone would prove nothing about parsing. **Royal Road is second** because it is the first source where we build an EPUB ourselves, the biggest untested piece, and it is a gentle place to get that wrong. **Literotica** stresses the assumption that every source has neat chapters. **FFN is last and goes through FicHub, not scraping** — this is the finding that changed the plan. FFN sits behind Cloudflare and FanFicFare effectively abandoned it, but [FicHub](https://fichub.net/api) has a documented public API returning metadata plus a ready-made EPUB, and absorbs the Cloudflare problem on their side. Their conditions are conditions, not suggestions: identify the project in the user-agent with contact info, **never** concurrent requests, honour `429`/`Retry-After`, no bulk export. The dependency has to be visible in the UI, because if FicHub is down FFN silently stops working and the user deserves to know why. **The WebKit-as-fetcher idea is deferred, not rejected** — it was right when FFN looked impossible, and FicLab's extension proves the browser-session route works, but building a second fetching mechanism (heavier, slower, tied to the UI thread) cannot be justified when one JSON call does the job. It stays the fallback and the reasoning is recorded so it is not rediscovered from scratch. Also settled: **one shared EPUB assembler** rather than one per source, and highlights that survive an update by re-anchoring on their saved text. See [`docs/fichub-and-ffn.md`](./docs/archive/fichub-and-ffn.md) |
| 2026-09-04 | **P7 scope corrected: it is a browsing client, not a downloader — and FicHub does not solve FanFiction.net after all.** The user stopped the plan: *"not just a downloader, but surfing, exploring, etc."* They were right, and the tell had been sitting in `source-seam.md` §1 the whole time. Its four verbs — search → detail → chapters → content — **all begin from "I already know which work I want"**, which is a downloader's shape and cannot express wandering. Missing entirely: category and fandom browsing, author pages (their works, favourites, follows, profile), the site's own sort orders, your favourites and follows, series and collections, reviews. **The trait must be rewritten before anything is built**, because retrofitting a browse model onto a download-shaped API means changing every source and every screen. **The correction I most need to own: FicHub does not solve FFN.** I presented it as the answer one turn earlier. It has exactly two endpoints, `/api/v0/epub` and `/api/v0/meta`, and **both require a fic URL you already have** — no search, no browse, no author pages. So it solves *downloading* from FFN and does nothing for *browsing* it, which is most of this phase. My reasoning was sound only while the goal was downloading; the moment the goal is browsing, it collapses. **The WebKit-as-fetcher idea is therefore un-deferred the same day it was deferred** — browsing FFN means fetching FFN pages, means Cloudflare, means the browser engine we already ship. Best answer is probably both: WebKit for browsing, FicHub for the download once a fic is chosen. **Login also moves in-scope**, having been parked in `source-seam.md` §13 as "probably out of scope until someone asks" — someone asked, since favourites and follows live behind a site login, so credential storage needs doing properly. AO3, FFN and Literotica get full browsing; the trait must let other sources offer less without the UI breaking. P7 is now clearly several phases of work rather than one. Full write-up in [`docs/p7-scope-correction.md`](./docs/archive/p7-scope-correction.md) |
| 2026-09-04 | **P7 split into six stages; read-online decided; login pushed last and password storage ruled out; multiple-readers parked.** The phase is several phases of work, so it is now **P7a** trait+browse+search → **P7b** reading online → **P7c** author pages → **P7d** the other three sources → **P7e** accounts (if wanted) → **P7f** download/follow/auto-update. Note the inversion: downloading, which was the entire original plan, is now the *last* stage. **Read-online costs far less than expected** because the reader already works that way — it never reads an EPUB directly, it unpacks into `cache/reader/<uuid>/` and reads the unpacked files, and `prune_reader_cache` already sweeps old ones at startup. So the reader does not care whether the EPUB came from disk or the network; what is missing is a temporary identity for a non-library fic and a "keep this" promote action. Two refinements on the user's sketch: **do not delete on app close** (a few days or a size cap instead, so closing and returning an hour later does not re-download), and **very long serials cannot use this at all** — downloading a 2,000-chapter Royal Road work to read one chapter is not viable, so those need per-chapter fetching, which is why the trait still needs a chapter-content verb that AO3 will never use. **Writing actions are out entirely** — no kudos, bookmarking, posting or reading reviews — which keeps Kalam read-only against every source. **Login goes last and passwords are ruled out.** AO3's own mobile-apps post says that if a third-party app asks for your AO3 login you provide it *at your own risk*, and r/AO3 auto-replies with that link on every app question; it is a warning rather than a ban, but **we are that third party**, and there is no sanctioned route since AO3 still has no public API thirteen years after calling one "several major releases away". If accounts happen it is session-cookie-only, obtained by logging in through a real AO3 page in a WebKit window we never read the password from, stored in config rather than in a library folder so it cannot travel with copied books. Worth stating plainly: **Kalam's own follows already work across all four sources without an account**, which is a better feature than the site-side list login would buy. Also **parked for a later thread at the user's request: multiple books/readers open at once** — recorded in a new "Parked for later discussion" section so it is not rediscovered, and flagged as touching the reader, the WebView pool (which parks exactly one view today) and reading-session bookkeeping |
| 2026-09-04 | **P7 moved behind the comics phases; P6.5 (libraries) is next; reading model settled as three tiers with per-chapter fetching.** The user: *"push this step back at the very last. this is a complex step and I am still working things out."* P7 now runs **after P8/P9** rather than before. Two reasons and the second is better: the design is still moving, and the `Source` trait gets proven against the **image** flavour first — which it has never been tested against at all, especially since MangaDex moved into the comics phase. Discussion continues; nothing is frozen. **Reading model.** Three explicit tiers: browsing keeps nothing, **Save** keeps a fic in the temp area without it becoming a library book, **Download** makes it a real book. Fetching is **per-chapter**, not whole-work — required, because a 2,000-chapter Royal Road serial cannot be downloaded to read one chapter. That has a design consequence worth writing down: **the trait needs a fetch-one-chapter verb that AO3 will never use**, because AO3 hands over a complete EPUB — so *AO3 is the unusual source, not the template*, and designing the trait around it would produce the wrong shape. **I withdrew a suggestion here and the user's model was better.** I had proposed keeping every temporary cache for a few days with a size cap instead of deleting on close; they rejected it for the explicit Save tier. They were right: time-and-size pruning means the *app* decides what to keep, disk usage drifts on its own, and "why is this still here, why did that vanish" has no answer a user can predict, whereas an explicit Save is a decision you made and can see. It also dissolved the problem I was solving — losing a big download by closing the app only bites if reading requires downloading the whole work first, which per-chapter reading removes. Still open and flagged: **what actually separates Save from Download**, since both keep the fic; the answer decides whether saved fics need their own screen. **Login stays undecided** ("we will see") and is unblocking, since it is the last stage. Also corrected while here: the top-of-file "agreed order" list was badly stale — it still showed A0 as NEXT (finished), P7 before comics, and Webnovel in scope |
| 2026-09-04 | **P6.5 started: the library registry and the Libraries settings card are in, CI green.** `~/.config/kalam/libraries.json` records every library and which one is open, and `paths::data_dir()` now reads it — so the ~30 path helpers built on that one function follow automatically, which is why making libraries switchable was a change at the root rather than a sweep through the app. **Existing installs are untouched**: with no library chosen it falls back to the old fixed location, so an upgrade is a no-op until the user asks for something else. The registry is written temp-file-then-rename, the same rule `epub_write.rs` follows, because a truncated write here would lose the record of where every library lives. Settings → Storage now has a Libraries card with Add / Open / Forget; Forget removes the list entry only and the message says plainly that the folder and books were not touched, since "forget" and "delete my library" must never be confusable. 10 tests, deliberately concentrated on the index arithmetic — `active` is an index into the list, so forgetting an *earlier* entry shifts it, and an off-by-one there does not crash, it silently opens somebody else's library. **Worth recording how this went:** the first two pushes failed CI on dead code, because `-D warnings` rejects methods with no caller — precisely the rule `source-seam.md` §12 gives for why the `Source` trait cannot land before its first implementation. The fix both times was to build the caller rather than silence the lint, which is the right pressure: no API lands in this repo without something that uses it. Still to come: the relaunch flow (selecting currently takes effect on next start), migrating an existing library into the scheme, the per-library/global split so dictionaries stay shared, and the `kalam.json` per-book backup |
| 2026-09-04 | **P6.5 green: the per-library/global preference split is in, and CI is healthy again after eight red runs from three unrelated causes.** The split matters because `app_prefs` lives in `catalog.db`, i.e. *inside* a library — so without it, switching library would silently reset the theme, reader font size, dictionary settings and API keys, since the new library's database has never heard of them. App-level prefs now mirror to `~/.config/kalam/prefs.json`, read from there first, falling back to the library so existing installs carry across untouched. Unknown keys default to **per-library** on purpose: a global pref that should have been per-library silently applies one library's value to another and reads as corruption, while the reverse is merely "set it again". Three of the key names were wrong on the first attempt — the app theme is `ui.theme` not `theme`, the writeback flag is `epub.write_metadata`, dictionary markers are `bundled_dictionary_*` — all now read out of the code rather than guessed, because a *nearly* right key classifies as per-library and the setting quietly stops following the user. **The eight red runs are the more useful record.** Two were dead code (`-D warnings` rejecting methods with no caller, the same rule that keeps the `Source` trait from landing early). **Five were a formatting step failing the build** — rustfmt auto-committed and pushed, the push was rejected, and a non-zero exit killed the run, so a cosmetic check masked clippy, the tests *and* the build. The real defect sat behind it untouched for all five: my append-tests script cuts at the file's last `}`, which stopped being the test module's once functions were added after it, so 70 lines of `#[test]` were spliced **inside `set_global_pref`'s body** — braces balanced, so nothing short of a parser could see it. rustfmt is now report-only, publishes its diff to `ci-logs/`, and cannot fail a run (§23). The last was a pre-existing service test that began reading the developer's own `~/.config/kalam/prefs.json` the moment `dict_history_enabled` became global, making its result depend on the machine rather than the code (§24). Three lessons: a cosmetic check must never gate a build; do not append code by locating the last brace; and moving state outside the repository turns every test that touches it into a test of the machine unless you isolate it in the same change |
| 2026-09-04 | **P6.5: dictionaries no longer live inside a library, and the existing library now appears in the list.** Two fixes, the first caught by reading the paths module rather than by a failing test — which is the only way it *would* have been caught, since nothing breaks until someone actually switches library. `dictionaries_dir()` hung off `data_dir()`, and `data_dir()` is now the **active library**. Left alone, switching would have looked for dictionaries in the new folder, found none, and reinstalled the bundled packs — roughly 4 MB compressed and several seconds of import — **once per library, permanently**. Exactly the "you would reinstall dictionaries every time you switched" failure the phase description warned about, quietly reintroduced by the change that made libraries switchable. They now hang off a shared root, and the path is unchanged for anyone who never switches. The neighbouring judgement call is recorded in the code: **author photos and series covers stay per-library**, even though they are internet-fetched and would be byte-identical everywhere, because they are caches of *this library's* authors — deleting a library should take its cached photos with it, and a shared folder would accumulate photos for authors nobody owns any more with nothing to prune it. Both are caches, so the decision costs only a refetch to reverse. Second: **the pre-existing library is now adopted into the registry at startup.** The fallback already made it work, but it left the user's own books as the single library absent from Settings, so Open and Forget applied to every library except theirs and adding a second would make the first appear to vanish. Nothing is moved or copied — only the list learns the folder exists. A folder is judged a library by containing `catalog.db`, deliberately not by being non-empty: picking `~/Documents` by mistake must not be treated as an existing library, while picking an empty folder is the normal way to start a new one. The Add button now reports which of the two happened, since "library added" is ambiguous between "created an empty one" and "found my books" — alarming in one direction, confusing in the other. CI green first try |
| 2026-09-04 | **P6.5 complete — libraries are switchable, portable and self-describing. Next is P6 (Downloads hub).** Two final pieces. **Switching now restarts the app** instead of taking effect "next launch", which was an odd thing to ask of someone who had just clicked Open. It confirms first (a restart closes whatever you are reading), saves the choice, *then* re-execs — that order matters, because restarting first would reopen the old library and look like the click did nothing. `exec` rather than spawn-then-quit, so two Kalams never hold the same catalog open at once; and a restart rather than repointing in place, because `data_dir()` is cached for the process and pages hold live database handles, so switching underneath them would leave some reading the old library while writing the new one — corruption rather than a visual glitch. **And `kalam.json` beside every book** (`src/sidecar.rs`): title, authors, series, tags, rating, reading position and highlights, each highlight carrying its own text — the field that makes it findable again after a file changes, the same insight as the P7 re-anchoring plan. Explicitly a **backup, never the truth**: the database stays authoritative and these are never read during normal operation, so cross-book screens remain one query and no conflict rule is needed. Names not paths, or a copied folder would be full of wrong locations. **A clippy dead-code error turned out to be the useful part of this commit.** `read_sidecar` had no caller outside tests — and that was not a lint technicality, it was the observation that I had built a backup with no way to check or complete it. The fix was a Settings card showing how many books have a backup copy, plus a button to write the missing ones. That matters on its own terms: "your library describes itself" is worth nothing unless it is verifiable, since sidecars are written on code paths that could quietly stop running and the failure would stay invisible until the day someone actually needed them (§19). Left deliberately unbuilt: the rebuild-from-folders command itself. The survey proves the data is there; nothing consumes it yet, and inventing that flow before anyone has lost a database would be guessing at the recovery experience |
| 2026-09-04 | **Two P6.5 defects found by re-reading after "done", plus the README brought up to date.** Neither would have failed a test, which is why the check was worth doing. **(1) The `kalam.json` backup was going stale.** It was refreshed on import, on metadata edits and on adding a highlight — but *not* when a tag chip was added or removed, a highlight deleted, or a note or colour changed. So an ordinary edit left the backup out of date while the recovery card still reported "all books covered", because it counts files rather than freshness — a promise that quietly degrades until the day it matters. Every edit now refreshes it. Reading progress deliberately still does not: it fires on every page turn, and your exact place is the least valuable field and the quickest to re-find; that is now a comment rather than an omission the next reader would take for a bug. **(2) A library on a removable drive would have looked like data loss.** If the selected folder is gone — unplugged drive, unmounted share, folder renamed — nothing failed: `create_dir_all` recreated it and `Catalog::open` built a fresh empty database inside. The user sees an empty library and concludes their books are gone, when the drive is merely not plugged in; worse, the recreated folder then *looks* like a real library, so reconnecting the drive does not obviously fix anything. Kalam now refuses to start, names the missing folder, states plainly that nothing was changed, and says where to look. Refusing rather than falling back to another library is deliberate: writing into a library the user did not choose is how books end up scattered across two places. **README** was several phases stale — it still described A0 as in progress, never mentioned libraries, and its Data section called `~/.config/kalam` "future" when it now holds the library registry. It also gains a **Switches** table: `KALAM_NO_WINDOWED_GRID`, `KALAM_NO_PRELOAD`, `KALAM_NO_WEBVIEW_POOL`, `KALAM_NO_CSS`, `KALAM_TIMING`, `KALAM_ROUTE` all existed but were discoverable only by grepping the source, which makes an escape hatch useless to the person who needs it. Also fixed: `ci-logs/test-latest.txt` and `clippy-latest.txt` were published **only on failure**, so a fixed problem left its old failing log committed and every later green run still showed red — it misled me three times in one session, the last reading "1 failed" from a run two hours dead (pitfall §25) |
| 2026-09-04 | **P6 and P7 combined into one phase; P5.5 (UI overhaul) moved to the very end.** Both at the user's request, and both are the right call for the same underlying reason: **do not design against imagined content.** *Combining P6+P7* — a downloads queue with nothing to download is a shell, and a fiction client that fetches things needs somewhere for those jobs to live. Built apart, the queue would be designed against guessed callers and then reworked when the real ones arrived; built together, its shape answers to what P7 actually needs. They stay separate sections in this file because they remain distinct bodies of work — the queue is infrastructure, the client is the feature — but they ship as one phase. *Moving P5.5 last* — the user: *"we will makeover the UI at last when everything is ready."* Everything still ahead adds screens: a downloads queue, a browsing client, author pages, a comics pager, a PDF view. Restyling Home and Library now would mean restyling them again once those exist, and a design settled before its content is a guess. Doing it last is one pass over a known set of screens instead of several passes over a moving one. What is already shipped stays shipped (colour system, 13 themes, Settings v2, book page, series float); only the remaining screens move. **New running order:** P8/P9 comics and manga → P6+P7 downloads and fiction → renderer slice → EPUB path → P10 PDF, P11 tools, P12 Lua → P5.5 UI last. Phase map, the "agreed order" list and the README status table all updated to match, since a stale order at the top of this file is exactly what sent an earlier chat off to build something that already existed |
| 2026-09-05 | **Code Review Batch 2 (Medium priority) completed — gitignore, Makefile, metadata unit tests, and date math tests.** (1) Updated `.gitignore` from `ci-shots/` to `ci-shots*/` to properly ignore all automated screenshot directories (`ci-shots-gui`, `ci-shots-1`). (2) Created a root `Makefile` with `all`, `build`, `dev`, `test`, `check`, `clean`, `install`, `uninstall` targets for installing Kalam (`kalam` binary, desktop entry, icon). (3) Added unit tests across `src/metadata/google_books.rs`, `src/metadata/openlibrary.rs`, and `src/metadata/series.rs` for author list formatting, thumbnail fallbacks, category filtering, published date fallback, and series normalization/sorting. (4) Added unit tests for Hinnant date math functions in `src/db.rs` (`civil_from_days`, `days_from_civil`, `days_from_iso`, `format_unix_utc`), verifying exact round-tripping for leap days, century non-leap years, pre-epoch dates, and ISO string formatting. Test suite expanded to 336 passing tests; `cargo check` clean with 0 warnings. |
| 2026-09-05 | **Code Review Batch 3 (Low priority) completed — external CSS, bundled-dictionaries feature flag, LibraryService migration.** (1) Extracted monolithic 3,574-line CSS string from `src/style.rs` into `resources/style.css`, loaded via `include_str!`. (2) Added `bundled-dictionaries` default feature flag to `Cargo.toml`. (3) Completed `LibraryService` struct field integration across remaining pages (`author.rs`, `series_float.rs`, `settings.rs`). All 336 tests passing clean with 0 compiler warnings. |
| 2026-09-05 | **Phase 8 (P8) — Comics local + Moku-style reader completed cleanly.** Integrated CBZ/CBR comic archive reading (`src/comics.rs`) with natural alphanumeric sorting (`page2.jpg` < `page10.jpg`) and image extraction. Built interactive Relm4 `ComicsReaderModel` (`src/pages/comics_reader/`) with black immersive stage, top bar controls (title, page index, LTR/RTL/Webtoon reading direction toggles, Fit Width/Height/Original mode toggles), bottom bar scrub slider and prev/next page navigation. Includes bounded viewport texture caching (current page ± 2 adjacent pages) for memory safety. Automated routing in `AppMsg::OpenReader` to automatically open CBZ/CBR files in the comics reader. All 336 unit tests passing cleanly with 0 compiler warnings. |
| 2026-09-05 | **Phase 9 (P9) — Manga Platform completed.** We survived the Great Aggregator Crisis. After attempting to build Comick (API disabled images) and Manganato (domain entirely hijacked/CF blocked), we successfully pivoted to **WeebCentral**. Built a seamless native scraper in `src/sources/weebcentral.rs` that taps directly into their HTMX API, bypasses Cloudflare entirely, and natively streams official, high-quality simulpubs straight into the immersive comic reader. Re-wired the UI in `remote_detail.rs` to strip away the "Open Web" fallback entirely and properly constrain cover image ratios using `Pixbuf::from_stream_at_scale` so they don't break GTK's layout engine. |
| 2026-09-18 | **Part 1 Offline Master Plan Locked (`docs/offline-roadmap.md`).** Consolidated all offline requirements into 6 dedicated modules (Reading Engines & Inline Editor, Yazi Architecture & Service Layer, Content Sanitizer & Deep Content Search, Library Management & Metadata Editors, Annotations & Vocabulary Hub, and UI/Performance Closure). All online scraper/plugin features strictly relegated to Part 2, and the Material 3 UI design overhaul positioned at the very end of Part 1. |
| 2026-09-18 | **CI slimmed from three jobs to two, and the `--workspace` fix landed.** Two changes, both to `.github/workflows/ci.yml` (and mirrored in `docs/ci/github-actions-ci.yml`). **(1) The gate was only running a tenth of the workspace.** The workspace has 11 members and no `default-members` key, so with no `--workspace` flag Cargo selected only the root `kalam` package — `--all-targets` picks *targets*, not *packages*. Both `cargo clippy` and `cargo test` were scoped that way, so 8 crates under `crates/` and 2 under `tools/` were compiled as dependencies but never linted or tested, and `chapbook-viewer-gtk`, `chapbook-cli` and `kalam-reader-demo` — which nothing depends on — were never built at all. They could have failed to compile and the run stayed green. Roughly 420 tests under `crates/*/tests` and `tools/*/tests` (pagination, reader conformance, locators, render goldens, the stability policy) never executed. Fixed with `--workspace` in the workflow *and* in the `Makefile`. **Confirmed green on run `35364884201` (2026-09-18): 52 test binaries, 751 tests passed, 0 failed, 6 ignored, no panics.** It took four runs to get there, and every failure was exactly the class the fix was meant to expose — see the three fix commits that follow this row. Before the fix one test binary ran; now 52 do. **(2) The `scale` job was deleted and `screenshots` cut to one run.** The scale job re-measured a question settled on 2026-09-04, whose answer is committed in `ci-logs/scale-2000-comparison.txt` (windowed 247 MB / 7.1 ms vs the old build-every-card grid 352 MB / 139.7 ms). The PNG artifact upload went to nobody — the sandbox cannot download artifacts, and the text report carries the same facts — and the all-books screenshot pass duplicated the `read-1` run for the one thing still worth proving: the app launches and a book opens without panicking. That single run remains, renamed *smoke test*, still `continue-on-error: true`, so `build` is still the only job that can fail a run. `ci-logs/screenshots-latest.txt` and `ci-logs/scale-2000-*.txt` are now frozen records rather than current output. **Also corrected:** the workflow's own comment undercounted the skipped members as "seven chapbook/kalam-reader crates"; the real figure is eight crates plus two tools. `docs/ci/github-actions-ci.yml` had drifted 134 lines from the file that actually runs; it was re-synced, and then **deleted** once the owner granted the GitHub App the `workflows` permission, which removed the only reason it existed |
| 2026-09-18 | **Deleted the duplicate workflow copy and the manual-handoff machinery around it.** The owner granted the GitHub App GitHub's `workflows` permission, so the agent edits `.github/workflows/ci.yml` directly and pushes it — no copy for the user to install. `docs/ci/github-actions-ci.yml` is gone, and with it the three "ACTION NEEDED (2026-09-04)" sections in `docs/ci/README.md`, the "Working agreement (manual CI handoff)" table, and the paste-and-push instructions; every one of them existed only to route a workflow change through the user's account. The *reasoning* behind those sections was worth keeping and is now a short "Design decisions worth keeping" section instead: why rustfmt reports rather than pushes (a rejected push failed four runs for a reason unrelated to the code), why clippy and the test step are `continue-on-error` with a separate fail step (so the publish step still runs and the log explaining the failure is not skipped), and why the toolchain is left unpinned. The drift was the argument: a second copy you must remember to re-sync had already diverged by 134 lines, and the file that actually runs is the one an agent reads |
| 2026-09-18 | **The `--workspace` fix went green, and it took four rounds — every failure was exactly what the fix was built to expose.** Run `35364884201`: **52 test binaries, 751 tests passed, 0 failed, 6 ignored, no panics.** Before the fix, one test binary ran. Round 1 (`a4c4d54`) → compile error: `chapbook_paint::Selection` had gained a `style: SelectionStyle` field and `crates/chapbook-layout/tests/pagination.rs` was never updated, because nothing compiled it. That is the precise failure mode `--workspace` exists to catch: a struct changes in `src/`, and a test file that no build ever touched drifts behind it silently. Fixed at :1433 and :1528 with `SelectionStyle::Band` spelled out rather than `::default()`, because both tests assert on a `DisplayOp::Band` and a future change to the default arm must not quietly change what they check. Round 2 (`bba2e00`) → 11 clippy findings in `kalam-reader/src/view.rs`, also never linted: ten `explicit_auto_deref` (`&mut *s` on a `RefMut<Session>`, where `&mut s` auto-derefs identically) and one `unnecessary_cast` (`(single_w * scale) as f32` where both are already `f32`, verified from the declarations at :1296 and :1275 rather than taken on the lint's word). Round 3 (`bb43750`) → 8 findings in the root package, and these are **new lints, not new code**: the workflow installs `dtolnay/rust-toolchain@stable` unpinned, and its own comment already warned "a new Rust release that adds a lint can turn this red without the code changing." All eight mechanical — a `map_err` converting `ImageError` to itself, an if-let-Ok that is `.ok()`, `% 2 == 0` that is `is_multiple_of(2)`, a `let _ =` on a function returning `()`, two blank lines between a doc comment and its `fn`, a collapsible `if`, and `map_or(false, f)` that is `is_some_and(f)`. **Round 4 also found a real bug the tests could not see:** the smoke-test log carried `Gtk-WARNING: Theme parser warning: Unterminated block at end of document`. `resources/style.css` was **truncated mid-rule** — 4330 lines, braces unbalanced by one, ending at `.kalam-reader-location-text { font-size: 0.78rem;` with no closing brace. Pre-existing since `29788b4`, not introduced here. GTK drops an unterminated rule entirely, and the class *is* live (`src/pages/reader/mod.rs:148`), so the reader's location text has been rendering with no `font-size` at all. Closed the block; both CSS files now balance. **This is the second thing the slimmed CI caught that the build job could not** — which is the argument for keeping the smoke test rather than cutting it to nothing |
| 2026-09-19 | **Part 1 reorganised from six modules into eight phases.** A module list says what is wanted; it does not say what to do next. Phases are a queue, each with a "Done when" line. The phases alternate invisible/visible on purpose: the async service layer is genuinely load-bearing and must come first, but doing *all* the groundwork before anything appears on screen means six months with no way to tell whether an abstraction is right. PDF dropped to a deferred section — the owner reads EPUB, PDFs rarely. Also corrected a false claim from the previous row: the reader binds twelve keyboard shortcuts, not just Ctrl+F and Escape, so dictionary lookup by selection already works. |
| 2026-09-19 | **Bubbles designed; logger installed to make the deciding number visible.** The owner described Android-style bubbles: several books open at once as stacked circles over any screen, tapping one opening a floating reader, with books openable straight into a bubble and minimizable back into one. The only prior record was one line in `docs/conversation.md` — half of it dead, since it assumed a shared WebKit process. Recorded decisions: in-app only; one expanded at a time with the others lined up; cover art with a progress ring; the bubble reader is a reading surface plus a TOC-only sidebar; **the full reader keeps every feature it has today** (its 5,845 lines and five sidebar tabs are untouched). The engine already has the memory machinery this needs — `Session::suspend`, `set_cache_budget`, `cache_bytes`, and a 192 MB default budget whose comment records 676 MB after forty comic pages before it existed. The open question is the *fixed* cost, which `suspend()` does not release: `Session::open` runs `build_font_system` → `db.load_system_fonts()`, a whole-system font scan, once per book. `open.rs` has always timed that split and logged it, but **no logger was installed, so all 17 engine `log::` calls were discarded.** Installed one (`src/logging.rs`, `env_logger`): writes to stderr *and* `~/.local/share/kalam/kalam.log` because a `.desktop` launch has no terminal, and is a complete no-op unless `RUST_LOG` is set, so a normal launch is unchanged. Three tests: the tee reports the file and not the terminal, an unwritable path degrades to `None` rather than panicking, and the log lives in the shared dir so it does not move when the library changes. **The shared-font-system change is deliberately not built** — it is only worth doing if the measurement says the scan dominates, and nobody has measured it yet |
| 2026-09-19 | **Measured what opening a book costs; the expected engine change turned out not to be needed.** Four books on the owner's Arch machine via the new logger. The font scan — the whole reason a shared font system was on the table — is **1–5 ms** out of a 32–78 ms open, with 8 faces. **Decision: do not build it.** What the numbers show instead: opening plus the once-per-book chapter character count puts a book ready-to-read at 100–220 ms, and first paint is 53–183 ms, which *confirms* the rule that a bubble must hold no book. Chapter layout ranges from 4 ms to **603 ms**, the slow cases being image-heavy (141 pages / 194 images in 547 ms; 603 ms with 487 images). **Memory, not time, is the constraint**: single-chapter caches measured at 34 MB, 37 MB, 13 MB and 12 MB against a 192 MB-per-session default budget, so bubbles must call `set_cache_budget` down hard on every book not being read and `suspend()` the rest. Net engine work for bubbles: **use the cache controls that already exist, add nothing.** Also silenced a warning that was burying the log — `xml5ever` 0.39.0 (third-party) warns once per parsed document that it does not implement `stop_parsing` for XML5, hundreds of times per book; held to `error` in `src/logging.rs`, liftable with `RUST_LOG=info,xml5ever=warn` because a suppression that cannot be turned off is a trap |
| 2026-09-19 | **Chapter images now decode to the size a page can draw — measured 71% less memory.** Every `<img>` in an EPUB chapter was decoded at full native resolution with no knowledge of how big it would be shown; `collect_images` took no page size even though `PageMetrics` was in scope at its one production call site. Raw RGBA costs 4 bytes a pixel, so a 3000x4000 scan is 48 MB decoded while occupying at most the reading column. `collect_images` now takes a max edge and scales anything larger down, preserving aspect ratio; the caller passes `content_width * dpi_scale`. **Measured on the owner's Arch machine, *The Dragonet Prophecy*:** four chapters went from 85,082 KB to 24,361 KB — 83.1 MB to 23.8 MB, **71% less**. The engine's own `images:` line reports 77.1 MB decoded against 17.9 MB kept. **The number that matters is that the cache now fits**: those four chapters were 2.6x over the 32 MB budget before, so an image-heavy book sat permanently over budget and re-decoded on scroll at the 77-120 ms/page the engine measures; they are now under it. Cost is not zero — the resize stage added ~50 ms on small-image chapters (ch 7: 49 to 106 ms) while *reducing* it on large ones (ch 6: 487 to 385 ms, because `ImageStore::insert` premultiplies every stored pixel, so fewer pixels is less work there). An image that already fits is returned as the same bytes, not resized to the same size, so a no-op resize cannot shift a byte-exact golden; every fixture image is 64x48 or 120x60, so goldens are unaffected — which also means they cover none of this, hence six unit tests on the shrink. Text-only books are untouched: *Immortals of Meluha* logs no `images:` line at all. Two call sites outside `crates/` were missed on the first push and broke `chapbook-cli`; `--all-targets` compiles examples too, so grepping one directory is not enough. Comics deliberately untouched — they decode on a background loader with no page metrics, and they zoom, so shrinking a comic page to fit would soften a zoomed one. The disk page cache is now very unlikely to be needed |
| 2026-09-19 | **Lazy loading audited; most of it already exists, so only the gaps are planned.** Asked whether chapters could load and unload on demand. Verified rather than assumed: **they already do.** `layout_unit` builds a chapter only when asked, `evict_keeping` drops the least-recently-read ones under the byte budget while pinning the chapter on screen and the visible ones, `prefetch_one` reaches exactly one adjacent chapter, and `suspend()` drops everything but the page showing. Three real gaps: eviction is per *chapter* not per page (a 141-page chapter is one lump); there is one budget for one book where bubbles need one per state; and the chapter character count runs eagerly on open (30-147 ms) when it is only needed on first use. **Per-book budgets go to Phase 2 with the bubbles** — `set_cache_budget` already exists, only the call is missing. **Page-level eviction is recorded as a tripwire, not a task**: it was worth doing when such a chapter cached 34 MB against a 32 MB budget, and the image fix put the same chapter at 13 MB, under budget on its own. Building it now would repeat the shared-font-system mistake — solving a problem the previous fix dissolved. The check is written down so the tripwire is testable: a `laid out unit N` line above 32,768 KB. Eager char count goes to Phase 7. Also corrected a stale figure in the Bubbles section: it said the budget was 192 MB per book, which is the *engine* default; `kalam-reader` overrides it to 32 MB and that is what runs |
| 2026-09-19 | **Full Part 1 audit: every work item is now permanently numbered, and eleven items were found that the plan did not have.** The owner asked for an extremely thorough check of everything that must be finished before Part 2 — including breakage left over from earlier sessions — with the report first and the roadmap edit only after confirmation, so that work proceeds *in sequence* rather than haphazardly. The audit was done by reading the code, not from memory, and **one finding was caught as a false positive before it was reported**: a grep for reader preferences referenced only once flagged thirteen keys as dead settings, but `reader.ui.back_chip_size_px` and `reader.scrolled` are both read — the string appears once because the key is centralised in a `ReaderUiSetting` key function and a `PREF_SCROLLED` constant respectively. Good design, bad grep; not reported. **What the audit actually found.** Three things broken today: text PDFs paint a grey bar per line with page mode the default (`reflow_mode: false`, `pdf_reader.rs:89`); the dictionary popup has no keyboard at all — no `k-def-focus`, no focused-sense concept anywhere in `src/`, so you can open a card but not move between meanings or save one; and its 55-test jsdom harness is gone. **Four engine settings exist with no control for them**, each confirmed by grep: `font_family` and `publisher_styles` have **zero** references in `src/`; the nine `justify` hits are all GTK label alignment, not `ReadingSettings::justify`; and hyphenation runs in the engine with no switch. `Session::font_families` already returns the installed faces, so the font picker is missing only its UI. **Five documents describe an app that no longer exists**, which is the finding with the longest shadow: the README claims P6 and P7 shipped together when there is **zero** `impl Source` and `SourceManager` holds an empty `Vec`; it claims comics are "next" when they shipped at 1,685 lines; its status table predates the engine swap entirely; `js_bridge.rs` is the dictionary popover under a WebKit name; and `conversation.md` §20 still recommends Poppler where §22 chose MuPDF. **Reachability came back clean** — `NavItem` hides three routes (Downloads, RemoteBrowse, Fanfiction) that are all Part 2, all eight `LibrarySection` variants are routed, and `PlaceholderPageModel` is only on the error paths. **Two placements follow from the owner's decisions.** The cheap PDF fix moved *out* of Deferred into Part 1 as 2.11, because PDF bubbles were wanted and cannot exist over grey stripes — the tension between "PDFs are rare" and "PDFs get bubbles", resolved by paying a few hours and leaving MuPDF deferred. The dictionary keyboard became 2.1, ahead of bubbles at 2.12, as the oldest breakage; bubbles moved last because they need 1.2's async layer. **The numbering is the actual deliverable**: numbers never change, finished items keep theirs struck through, and nothing is built that is not on the list — a new want gets a number and a place *before* it gets written, with one exception for a defect in code being touched right now, which gets a number in its commit's changelog row. That rule exists because drifting is precisely what happened before. Also resolved two stale duplicates that both said "discuss before designing" about multiple books open, which was designed the same week |
| 2026-09-19 | **Started Phase 1 at 1.2 and immediately found the item itself was wrong; measurement then cancelled the work it described.** Beginning an item is how you find out whether it was written correctly. **"Make `LibraryService` asynchronous" contradicted a decision already recorded in the code**: `src/tasks.rs` has a section headed *"No tokio"* — `thread::spawn` + `async-channel` + the GLib main loop, because relm4 is already the actor framework and a second runtime is a second scheduler fighting the first. Making the service `async fn` requires exactly that runtime. The design that *was* built is the opposite shape and is already complete: methods stay synchronous, return `Send` snapshots, and the *caller* runs them on a worker via `tasks::spawn` — `service.rs` says so, and `snapshots_are_send()` asserts `Send` on the service and all 12 snapshots so a future edit cannot quietly break it. The real gap is that nobody uses it: **92** service call sites across `src/pages/`, of which **2** are near a `tasks::spawn`, sitting in `init()` and `reload()` — both UI-thread paths. Rather than convert 92 blind, 1.2a instrumented all 16 snapshot methods through the existing `timing` harness (`measure` returns a `Guard` that reports via `Drop`, so an early return cannot leave a span open and no `_end` call can be forgotten; `#[must_use]` plus CI's `-D warnings` is the enforcement against binding it to `_`, which would report ~0 ms and read as a fast query — the one way to get this wrong silently). **The owner's log then cancelled 1.2b outright: worst case 10.8 ms (`dashboard`), median about 1.2 ms.** A thread round-trip plus a loading state would cost more than the query it replaces, so all 92 stay on the UI thread. Four methods were never visited; they read the same tables at the same scale. **This is the second time measurement dissolved a planned task** — the shared font system was the first — and the standing rule holds. **The log's worst number was an accident of that instrumentation: `window_shown` = 9,166.6 ms**, and the three existing startup spans accounted for only 2,410 ms of it. **6,757 ms — 74% of a nine-second cold start — was attributed to nothing**, in the one place the user feels first. That is new item **1.12**: instrument `main()`'s uninstrumented blocks plus a `pre_run` marker, which splits the gap into "before `app.run`" and "inside `AppModel::init` and GTK's first realize" in a single run — `app.run` never returns, so it cannot be spanned, and the marker is the last instant `main()` can time. Fixes follow in Phase 7. The same log also showed `grid_build` at 260.2 ms for 48 of 150 cards against 7.1 ms for 2,000 in the benchmark, recorded under 1.12 as a Phase 7 question. **Also recovered from the recurring moved-base fault**: HEAD had silently fallen back to the branch point `29788b4`, so the first `git diff` showed `mod logging;` and `logging::init()` as additions — code that had been committed and pushed turns earlier. Nothing was lost; the working tree held every change. Recovery was `git reset FETCH_HEAD` (mixed), which re-points HEAD and the index *without touching the working tree*, leaving only the two genuinely-changed files, followed by `git checkout -- ci-logs/` to discard stale generated logs rather than push them over fresh ones. Worth recording because the symptom is alarming and the fix is not what it looks like: `--hard` or `stash` would both have destroyed work here |
| 2026-09-19 | **Warm start is 44% faster — 1,366 ms to 762 ms — from moving one line off the startup path.** The owner ran the instrumented build four times cold-then-warm and the sequence of findings mattered more than any of them. **The nine-second cold start was not a defect**: it was a cold OS page cache on a spinning disk, and `startup_db_open` proved it — 2,049–2,593 ms cold against **9 ms warm**. Two alarms were withdrawn on that evidence. What survived was `icons_theme` at 493–505 ms warm, 36% of the start, with `icons_write` at **0.1 ms** proving the cost was GTK rescanning every icon theme on the system rather than writing the files. Two of the four icons turned out to be **dead** — `kalam-dictionary-symbolic` and `kalam-copy-symbolic` have zero references, the toolbar having switched to `accessories-dictionary-symbolic` and `edit-copy-symbolic` and left the custom SVGs orphaned on disk — and the two survivors are used only by the reader's selection toolbar, so nothing at startup draws them. Moving `icons::init()` to an idle callback behind a `OnceLock`, with the toolbar calling it as a fallback, produced **more than the predicted saving**: `icons_theme` fell from 496 ms to **4.3–5.9 ms**, so the rescan did not merely move, it became cheap — most likely because `add_search_path` before GTK had loaded any theme forced a full load of every theme, whereas afterwards it is a cheap invalidation. Recorded as an inference. Measured over three before and four after runs: `pre_run` 686.9 → 224.6, `AppModel::init` 567.5 → 424.9, GTK realize unchanged at ~112, **`window_shown` 1,365.5 → 762.1**. The owner confirmed the highlight and quote icons still render, which was the one risk. **The unexplained part is recorded rather than papered over**: `AppModel::init` also lost 142.6 ms, which nothing predicted — plausibly because `add_search_path` invalidates the theme and every `from_icon_name` during widget building then resolved against a dirty one. **The method is the point of the row.** Three of the four biggest numbers this week turned out to be measurement artifacts or misattributions, and each was caught by refusing to accept the first reading: a 2 s database open that was 9 ms warm, a 527 ms first page that was 8 ms warm, and a 496 ms icon load that was 6 ms when asked later. Cold runs ranged 8,871–10,884 ms while warm runs agreed within 14 ms, so the warm number is the only one that measures code. Clippy failed one push on the way — a `redundant_closure` written as a hedge against type inference, which `-D warnings` turns into a build error. **Phase 7 is numbered 7.1–7.6** and takes the two remaining findings: `AppModel::init` at 424.9 ms, now 56% of a warm start, and a `grid_build` of 260.2 ms for 48 cards against a benchmark's 7.1 ms for 2,000 |
| 2026-09-24 | **PDF reader rebuilt from scratch with native MuPDF rasterization and unified reader chrome.** Replaced the heuristic `lopdf` viewer with `mupdf` v0.8.0 compiling cleanly from source on CI (`libclang-dev libfontconfig1-dev`). Renders vector text, shapes, fonts, and images natively to crisp RGBA pixmaps. Built unified reader chrome matching Kalam's EPUB reader: edge-hover autohiding (top edge for Library back pill, bottom edge for bottom navigation pill, left edge Zen-browser style hover for left TOC outlines sidebar). Both PDF and EPUB readers now feature smooth Revealer `SlideDown` and `SlideUp` transitions. Left sidebar header features book cover, title, author, and reading progress bar (`kalam-reader-book-head`) with direct panel scroll. Continuous vertical scroll is viewer default and persists globally across restarts; initial open pre-renders active page synchronously to eliminate blank placeholder flicker; background rendering triggers immediate viewport redraws. MuPDF `system-fonts` enabled via `font-kit` and fontconfig. Smart crop uses background paper luminance detection with 5.5% (min 44px) margin padding. CI runs passing green. |

**Rows are append-only.** Do not edit or delete an old row — if a decision is
later reversed, add a new row saying so. A plan that quietly changes is worse
than one that visibly changes.

---

## Where the old content went

- **The P0–P12 phase write-ups, A0/A1 tracks, and the dictionary overhaul
  brief** → [`docs/archive/roadmap-phases-p0-p12.md`](./docs/archive/roadmap-phases-p0-p12.md).
  Kept for the lessons in them, not for planning.
- **`docs/offline-roadmap.md`** → merged into this file as Part 1; that file is
  now a pointer.
- **Working agreements, anti-bloat rules, and the engine boundary** →
  [`docs/WORKING.md`](./docs/WORKING.md).
