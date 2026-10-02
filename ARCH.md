# Kalam — architecture principles

Prescriptive, not descriptive. This document says how the app is built and
how every change must be built; where it conflicts with the code, the code
is wrong or this document is stale — fix one of them in the same change.

**Every `plan.md` states how the change it proposes complies with this
document.** That sentence is the enforcement mechanism; the tests in
`src/perf.rs`, `tests/guardrails.rs` and the stall watchdog (`src/stall.rs`)
make the loud violations visible in CI instead of waiting for a field
report.

## The one rule

> **The UI thread never touches disk, database, or parsing.**

The UI thread builds widgets and applies messages. Everything else —
every read, every write, every file, every parse — runs on a worker, and
its result arrives as a message. Owner directive, 2026-10-01; the whole
document is this sentence applied.

## Threading model

Three kinds of thread, and only one of them may hold GTK objects:

1. **The main (UI) thread.** Runs the GTK loop, builds widget trees,
   applies model updates. It may send work out; it may never do the work.
   `tasks::spawn` / `tasks::spawn_internal` (a hidden housekeeping task)
   are the standard way out; the readers' doc-service threads are the
   long-lived variant.
2. **Task workers** (`src/tasks.rs`). One-shot units of work with a label,
   progress, cancellation and a result delivered back on the main thread.
   Page snapshots, imports, deletes, OCR — everything a user waits for or
   that runs quietly in the background.
3. **Service threads** (readers). Own a resource for the process's life
   (an open document, a render queue), answer requests, never touch GTK.

The **stall watchdog** watches the main loop's heartbeat and logs any
block over ~100 ms with the route or dialog being built at the time
(`[stall]` lines, stderr and `kalam.log`). A new `[stall]` line in a
field run or the CI smoke session is a finding, not noise.

## Layering

```text
pages/ (widgets + models)      — view and input only
  └─ LibraryService            — one snapshot method per screen, all reads
      └─ Catalog (SQLite)      — the only code that knows SQL
```

- **Pages ask `LibraryService`; they do not hold `Arc<Catalog>`.** The
  ratchet in `tests/guardrails.rs` (`MAX_ARC_CATALOG_IN_PAGES`) only goes
  down. The service is cheap to clone (`Arc` bump), so workers take one
  along instead of a raw catalog.
- **A snapshot is the answer to a whole screen's data question** — one
  call, one owned `Send` struct (`service.rs` asserts `Send` at compile
  time). Wrapped getters would mean N round trips and N futures; the
  snapshot means one.
- **The service never calls `notify`** and never touches GTK; it must be
  callable from a worker. Errors travel in the snapshot (`errors` rows);
  the page decides what the user sees.
- **Filesystem questions stay with the caller's worker** (a file-size
  stat is not a catalog read), and disk layout truth lives in
  `comic_folders.rs` / `folders.rs`, never in page code.

## State flow — the screen-open recipe

Every screen follows the same four steps (the book page and book float are
the reference implementations, roadmap 7.1 step 2a):

1. **`init` builds the skeleton.** Structure, layout, loading states.
   Zero queries, zero file reads, zero parsing. The user sees the screen
   instantly.
2. **A worker produces the snapshot.** `spawn_internal` + the screen's
   service method. Expensive extras (an EPUB spine walk for chapter
   titles) are separate workers that arrive when they arrive.
3. **The snapshot arrives as a message; the page applies it.** A rebuild
   is then *pure view-from-model* — the fill functions read model state,
   never the catalog.
4. **A stale answer never wins.** Each request carries what it was for;
   an answer is applied only if it is still current (the readers'
   generation rule; a per-book page checks the id).

Mutations are tasks too (a delete removes a directory). A small single-row
write that reports its outcome inline is tolerated while the ratchets
allow it — the static boundary check (below) decides the end state, not
this sentence.

**Timing spans** (`route_open:<name>`, `dialog_open:<name>`) bracket every
construction, and `service_*` spans every snapshot read, under
`KALAM_TIMING=1`. A screen's construction cost is a measured number, not
an opinion. The apply path carries an activity guard and a span from the
day it is written (pitfalls §45): the watchdog can only name what has a
label, and an async apply handler is a new place the UI thread can
block. The same goes for main-thread task callbacks and idle jobs —
label them, or their blocks report as nameless.

## Memory

- The book grid is windowed (2,000 books: 247 MB peak, not 502).
- Covers are deferred: placeholders first, background decode via
  `preload::warm_covers` — a deferral is always paired with its warm-up
  (pitfalls §16).
- Caches are bounded by design, not by emptying (`set_cache_budget`, the
  PDF text/image windows; pitfalls §17).
- A bubble holds no book (id + thumbnail + position, all database rows);
  the reading book gets the cache budget, warm books are suspended.

## Enforcement (built in migration order)

1. **Count-shaped budgets** — `src/perf.rs`: statement counts that do not
   grow with row count, gating CI. Wall-clock flaked ~60 % between
   runners and was rejected; integers do not flake.
2. **The stall watchdog** — `src/stall.rs`; the CI smoke report carries
   its lines.
3. **Ratchets** — `tests/guardrails.rs`: production panic sites,
   `Arc<Catalog>` in pages, hex colours in the stylesheet. The numbers
   only go down.
4. **The static boundary check** (planned, 7.7 layer 3): a test scanning
   `src/pages/**` for direct fs/catalog/parser calls outside the task
   layers, with an allowlist of written reasons — last, once the
   allowlist is small.

Tripwires, not proofs: the document plus plan compliance is the primary
control; the tests make regressions loud instead of silent.

---

Historical architecture notes (the old living document, including the
stale `lopdf` mention) live in git history; navigation and UX decisions
are recorded in `ROADMAP.md` and `docs/conversation.md`.
