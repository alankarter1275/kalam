# Plan — the app-wide async migration (7.1 + 7.7; the app-wide half of 2.20)

**Status: APPROVED and in flight. The owner answered both questions on
2026-10-02 — skeletons everywhere ("yup"), watchdog log-only — and had
already confirmed the campaign and delegated the migration order. Step 0
(measurement) is implemented; see the step log at the end.**

---

## What the owner decided (2026-10-02)

- This campaign is the next phase ("yup").
- Scope: the whole app **except** the two things that already feel fast —
  the readers and the All Books windowed grid.
- Migration order: delegated ("all up to you, as everything would need to
  be migrated"). Ordered by **measurement**, not guesswork — which is why
  step 0 is instrumentation.
- The owner is not yet using the app daily; the plan does not depend on a
  daily-use ranking. The watchdog produces the ranked list instead.

## The principle (owner, verbatim)

**"The UI thread never touches disk, database, or parsing."** The UI is for
clicking; every process runs in the background.

## Research — what the code says today (evidence)

1. **Every page opens the same way.** `AppModel::build_page`
   (`src/app.rs:465`) constructs the page model on the UI thread, and each
   model's `init` runs its full data read inline:
   - **Book page** (`src/pages/book.rs:605`): `service.book_detail` in
     `init` **and again** in `reload_state` (`src/pages/book.rs:844`) —
     the same three queries paid twice; then `chapter_titles()`
     (`src/pages/book.rs:857`) **opens and parses the EPUB file**
     (`OpenBook::open` — disk, zip, XML) on the UI thread just to list
     spine titles; then `rebuild` fills cards, several of which query:
     `fill_progress` (takes the catalog), `fill_highlights_card` →
     `get_annotations_for_book` (`src/pages/book.rs:1397`),
     `fill_stats_card`, `fill_author_card`. The field-reported pause on
     tapping a book is confirmed — and is worse than reported.
   - **Book float / dialog** (`open_floating`, `src/app.rs:324` →
     `src/pages/book_float.rs:646`): the same synchronous `book_detail`
     runs before the dialog can appear on screen.
   - **Library dashboard** (`src/pages/library.rs:67`): the whole
     `service.dashboard(FEED_LIMIT)` read plus `build_dashboard`'s
     per-row lookups, inline.
   - **Home** (`src/pages/home.rs:172`): `rebuild(&widgets, &model.service,
     …)` reads everything inline.
   - The remaining pages follow the same init-reads-inline pattern; their
     individual costs get measured in step 0 rather than asserted here.
2. **Mutations are already async.** 13 pages call `tasks::spawn` for
   imports, deletes and other operations; the book page's own delete says
   "the page must not block" (`src/pages/book.rs:656+`). The pattern we
   migrate reads onto is proven, not new.
3. **The right read primitive exists**: `tasks::spawn_internal`
   (`src/tasks.rs:327`) — worker thread, result delivered on the main
   thread, no task-manager noise (it is housekeeping, not a user job).
4. **The service layer already concentrates every read** into one snapshot
   per page (`src/service.rs:280`–`626`: `home`, `dashboard`,
   `book_detail`, `history`, `words`, `quotes`, `shelves`, `tags`,
   `analytics`, …), each with a timing span. The migration surface is
   therefore uniform: move the snapshot call from `init` into a worker
   and deliver it back as a message.
5. **The PDF reader is the reference implementation** (2.20, field-verified
   2026-10-01): background service owns the work; the UI thread sends
   requests and applies answers as messages; the window paints its
   skeleton instantly and fills in when answers land; a stale answer
   never wins.
6. **Startup** (`AppModel::init`, 7.5): 424.9 ms of a 762 ms warm start,
   306 lines, unmeasured inside. Its cost is widget-tree construction,
   not queries (one catalog call found in the init range).
7. **The budget hook already exists**: `Catalog::count_queries`
   (`src/db.rs:514`, `#[cfg(test)]`, thread-local trace hook) — the same
   harness the six `src/perf.rs` budgets use. "A migrated screen's
   construction issues zero queries" is an extension of a working
   pattern, not a new invention.
8. **No stall watchdog exists** (repo-wide grep — none).
9. **Pitfalls re-read at phase start**, the ones that govern this work:
   §16 (a deferral must be paired with its warm-up; grep every builder of
   the lazy thing), §18 (dropped work must say who picks it up), §31 (an
   async rewrite must re-home EVERY side effect of the sync path —
   enumerate before rewriting, diff-read after), §27 (claims from the
   repo, never memory), §38 (green CI is not correct in the field — the
   owner field-tests each batch), §30/§36 (HEAD check before every
   commit).

## The design — what every screen migrates TO

One rule, copied from the readers, applied everywhere:

- A screen's `init` builds **only the skeleton** — structure, layout,
  loading states. Zero queries, zero file reads, zero parsing.
- The page's existing service snapshot runs on a worker via
  `tasks::spawn_internal` and arrives as an Input message; the page fills
  itself in when it lands.
- **A stale answer never wins**: each request carries what it was for
  (book id, query, sort key) and an answer is applied only if still
  current — the PDF reader's rule.
- Mutations stay as they are (already tasks).
- Per §31, before each screen's rewrite: enumerate the sync path's every
  side effect (queries, caches read/written, files touched, state set,
  triggers fired) here in the plan, and tick each off in the new path.

## Steps

- **Step 0 — measure** (7.1's mandated first move; also 7.7 layer 2).
  Timing spans bracket every route and dialog construction
  (`route_open:<name>`, `dialog_open:<name>`) in `build_page` and the
  float openers. The **stall watchdog**: a repeating main-loop tick that
  logs any block over ~100 ms together with the span currently open —
  names culprits in `kalam.log` and in the CI smoke session. Output: the
  ranked list of screens by measured open cost; the migration order is
  that list minus the two exclusions. This step also re-measures
  `grid_build`, which either confirms or retires 7.6's 35× discrepancy.
- **Step 1 — ARCH.md.** The prescriptive principles document (owner
  approved 2026-10-01): threading model, layering, state flow, budgets —
  short, copied from what the readers already do. Every later step checks
  against it; every future plan.md states compliance.
- **Step 2 — migrate the book page + book float** (the worst confirmed
  offenders and the field-reported ones). Skeleton first; one worker
  snapshot (book_detail once, annotations, stats, author rows); the EPUB
  chapter-titles parse moved off the open path entirely and filled in
  when it arrives; covers through the existing deferred path. Budget test
  added as it lands. Owner field-test after this step (§38).
- **Step 3 — migrate the remaining screens in measured order**, batched
  (dashboards: home, library; list pages: history, quotes, words, reading
  list, lookup history, tags, shelves, shelf detail, author page,
  analytics, comics pages, settings reads). Same recipe per screen:
  skeleton → snapshot in worker → fill on arrival → budget test.
  **Startup rides here** (7.5): spans first inside `AppModel::init`, then
  its blocks move off the open path as the measurements dictate.
- **Step 4 — enforcement completes** (7.7). Count-shaped budgets per
  migrated screen: construction issues zero queries; snapshot statement
  counts bounded and non-growing via `count_queries`. The static boundary
  check comes **last** — scan `src/pages/**` for direct fs/catalog/parser
  calls outside the task layers, with an allowlist of written reasons —
  once the allowlist is small. Owner field-test after this step.

CI green after every step; every commit HEAD-checked (§36).

## Exclusions

- The readers (EPUB, PDF, comics) and the All Books windowed grid:
  already async / windowed. Not migrated; the watchdog still observes
  them (freezes there are findings too).
- No feature work rides along (scope discipline).

## Risks

- The risky rewrites are those with side effects beyond the return value
  (the book page's EPUB parse and its caches, progress/journey state).
  §31's enumerate-then-tick-off rule covers each, and the diff-read at
  the end of every step.
- **A visible behavior change**: today a screen pauses then arrives
  complete; after migration it appears instantly and fills in. The
  readers already behave this way and the owner calls them the fast
  part — but it is question 1 below, not an assumption.

## Questions for the owner — both answered (2026-10-02)

1. **Skeletons.** Confirmed ("yup"): after migration a screen appears
   instantly with its structure and the data fills in, exactly as the PDF
   reader already opens. The behavior everywhere.
2. **Watchdog output.** **Log-only** — the owner's explicit choice. No
   on-screen notice; `[stall]` lines go to stderr and
   `~/.local/share/kalam/kalam.log`.

## Step log

- **Step 0 — measure: implemented 2026-10-02.**
  - `src/timing.rs` gained an always-on **activity stack** (not gated by
    `KALAM_TIMING`, because the watchdog must attribute stalls on ordinary
    launches): `activity(label)` → `ActivityGuard` (RAII, pop-by-label so a
    mis-nested pair cannot mis-attribute), `current_activity()` and
    `last_ended_activity()` for the watchdog's reads. Unit-tested (one test
    function, deliberately — parallel tests would race on the shared
    static, the db.rs thread-local lesson).
  - `src/stall.rs` — the **watchdog**: a GLib heartbeat source bumps an
    atomic counter every 50 ms; a watcher thread polls it every 100 ms and
    reports any provable block (≥ 100 ms after one heartbeat period of
    grace, so it cannot cry wolf on normal main-loop batching) twice —
    once when first proven (a permanent freeze still gets its line), once
    at resume with the final lower bound. Attribution: the route or dialog
    being constructed while blocked, else the last one that ended. Output
    log-only: stderr plus `kalam.log` (file created by this event only —
    the `logging.rs` no-file-on-ordinary-launch contract is preserved;
    `open_log_file` became `pub(crate)` for this). Installed in `main()`
    right after `RelmApp::new`, before the first page; it does not judge
    until the first heartbeat, so startup-before-loop is not a false
    stall.
  - **Route spans**: `route_label` maps every `Route` variant (exhaustive —
    a new variant without a label is a compile error) to
    `route_open:<name>`; `build_page` wraps all three call sites (init,
    take_or_build miss, refresh_if_stale) with a `measure` span plus an
    activity guard. The page cache's hit path never enters `build_page`,
    so the numbers are construction cost only.
  - **Dialog spans**: all six app-level float openers
    (`book/series/tasks/annotations/shelves/tags_float`) and the metadata
    editor's funnel `open_editor_inner` (covers both entry and the
    edit-next-book re-entry) carry `dialog_open:<name>` guards.
  - **CI**: the smoke session's report gains a "main-thread stalls"
    section (`[stall]` lines from its captured kalam.log); the
    `route_open:`/`dialog_open:` spans appear in the existing timing
    section because the harness already sets `KALAM_TIMING=1`.
  - Tests added: the activity-stack behaviour; every advertised route name
    (plus both id forms) maps to a distinct label.
  - **Process note, recorded in pitfalls §36**: the first push of this
    step landed only `src/stall.rs` — a moved-base reset was run over a
    dirty tree and wiped the tracked-file edits (untracked files survive
    `git reset --hard`; tracked edits do not). Everything was redone and
    this commit carries the complete step.
  - **The ranked list comes next**: `KALAM_TIMING=1 kalam` on the owner's
    machine, click through the library, read the `route_open:` /
    `dialog_open:` lines (and the `[stall]` lines the watchdog prints
    without any env var). That list orders step 2/3.
