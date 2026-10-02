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

- **Step 0 — measure: DONE** (see the step log). Spans + watchdog +
  smoke-report section shipped CI-green; the owner's field run produced
  the ranked list above.
- **Step 1 — ARCH.md: DONE** (landed with step 2a, 2026-10-02). The
  prescriptive principles document (owner approved 2026-10-01): the one
  rule, threading model, layering, the screen-open recipe, memory
  budgets, enforcement order. The old living doc (navigation notes,
  the stale `lopdf` mention) stays in git history; the new document
  ends with a pointer. Every later step checks against it; every future
  plan.md states compliance.
- **Step 2a — migrate the book page + book float: DONE** (2026-10-02;
  field-tested same day — see the step log and pitfalls §45. The reads
  moved; the freeze that remains is a different cost class).
- **Step 2a.1 — attribute the post-apply blocks: DONE and CLOSED**
  (owner approved 2026-10-02; field run same day — see the step log:
  our code is innocent everywhere, the freezes are GTK-side).
- **Step 2a.2 — fix what the log named: (1) and (2) implemented
  2026-10-02 (owner's go); (3)'s premise failed verification and the
  decision returns to the owner; field run owed.**
  - **(1) Layout-neutral cover swap — done.** The deferred cover frame
    now has exactly one child forever: a fixed-size picture whose
    paintable is swapped in place. The placeholder gradient is the
    frame's own background (no css class is ever toggled); the old
    `remove`-and-`append` (`swap_in_cover`) is gone. A paintable swap
    on a fixed-size picture is a repaint, not a relayout (§48).
  - **(2) Page/float covers decoded on their own worker — done.** The
    book page's snapshot worker decodes the hero, the four author
    thumbnails and the author avatar; the float's decodes its hero;
    the `Loaded` handlers put them in the cache *before* any cover
    frame is built, so these screens take the already-cached branch
    and never schedule a swap at all. The `warm_covers` calls stay
    (they skip the cached), covering `Refresh` and worker misses.
  - **(3) The GResource premise failed verification — not built.** The
    GTK docs describe `IconTheme`'s resource path as behaving
    "similar to search paths", and both mark the theme dirty, so
    `add_resource_path` would keep the ~500 ms rescan that is the
    actual cost; only the 232 ms of SVG-writing code would go. The
    honest options are back with the owner: (a) render the reader's
    custom icons directly from resources, bypassing the icon theme
    entirely (kills both costs; small visual risk — the icons lose
    theme recoloring; owner is visual QA), (b) move `icons::init` to
    the first reader open, which already calls it as a fallback
    (startup never pays; the first reader open pays ~700 ms once),
    (c) accept it and re-measure after (1)+(2) — the cover storm was
    interleaving with the rescan in the 3.4 s block, and it may no
    longer matter.
  - **(4) Re-measure after the owner's next run:** the 3.4 s
    convergence block and the cover-tailed blocks should collapse;
    whatever "after book_page_rebuild" remains on a cold open is the
    page's own GTK layout cost — attacked only if it still matters. The apply path has no span, so the
  watchdog cannot say whether the 0.7-1.4 s blocks after
  `route_open:book` / `dialog_open:book_float` are apply code, GTK
  realize/style/layout of the filled tree, or the idle work. The
  design, from the code:
  - **Apply paths get a guard inside the function that does the work:**
    `book.rs::rebuild()` (runs after every message — the `Loaded` arm
    itself only assigns fields, the rebuild is the cost) and
    `book_float.rs::fill()`. Activity guard + timing span, so the log
    splits "while <name> (open X ms)" (the code) from "after <name>
    (took X ms)" (GTK realize/paint of what it built).
  - **The icons idle gets named:** `main.rs:76`
    `idle_add_local_once(icons::init)` — unguarded today; the field
    run's 667 ms no-span block matches its measured 493-505 ms shape.
  - **Every task callback gets named by its task:** the two main-loop
    dispatch sites in `tasks.rs` (`spawn_with`'s progress + done loop,
    `spawn_stream_with`'s item loop) wrap their callbacks with an
    activity labelled `task_progress:<name>` / `task_done:<name>` /
    `task_item:<name>` — one wrapping point each attributes the
    thumbnail backfill, the cover preloader, the dictionary import and
    every user task's main-thread work.
  - **`timing.rs` activity labels widen from `&'static str` to owned
    strings** so a task's name can ride along; every existing call
    site passes a literal, and the watchdog only formats, so nothing
    else changes shape.
  - No behavior change anywhere; no new test surface beyond the
    existing activity-stack tests (the change is labelling, not
    logic). The owner repeats today's session once; the log then names
    the book/float split, the 667 ms block, and home's 3.9 s — and the
    fix (or the decision that the remainder is GTK-internal) follows
    from that evidence.
- **Step 2b — migrate the library dashboard + home.** The two heaviest
  constructions (897 / 459-858 ms first open; reads already ≤ 11 ms) and
  the two landing screens. Same recipe — **after 2a.1 says what the
  recipe must add**. Home also carries the field run's biggest blocks
  (3.9 s + 2.9 s after its span) and the cover-swap stream question.
- **Step 2c — migrate settings + author.** 465 / 232 ms of widget
  building with trivial or no reads.
- **Step 3 — startup (7.5), now with measured targets**: the 2.3 s
  catalog open (before the loop, splash up — decide what can move or
  overlap), the 1.2 s `AppModel::init` (spans first inside it, per 7.5),
  and the 3.9 s + 1.8 s post-paint idle blocks (spans on the idle work:
  icons, dicts check, thumbs backfill). The remaining small list pages
  (history, quotes, words, lookup history, shelf detail, comics) ride
  here in the same recipe — most are already ≤ 16 ms and will mostly need
  only their budget tests.
- **Step 4 — enforcement completes** (7.7). Count-shaped budgets per
  migrated screen: construction issues zero queries; snapshot statement
  counts bounded and non-growing via `count_queries`. The static boundary
  check comes **last** — scan `src/pages/**` for direct fs/catalog/parser
  calls outside the task layers, with an allowlist of written reasons —
  once the allowlist is small. Also here: the page-cache eviction
  question (progress writes currently evict everything — the owner's log
  shows near-100 % misses; decide fix or delete). Owner field-test after
  this step.

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
  - **Two CI-caught defects, both recorded in pitfalls**: (1) §36 eighth
    occurrence — the step's first push carried only `src/stall.rs`
    because a moved-base reset ran over a dirty tree and wiped the
    tracked-file edits; the complete step is the follow-up commit.
    (2) §41 — the new activity stack used `.expect()` on its lock and
    tripped the `production_code_panics_do_not_grow` ratchet (12 → 14);
    fixed with `unwrap_or_else(into_inner)` on both write sites, matching
    the read paths — diagnostics must not be able to panic the UI
    thread. Count verified back at 12 with the documented per-file
    distribution.
  - **The ranked list arrived (owner's machine, 2026-10-02)** — see the
    next section. It orders steps 2–3 below.

- **Step 2a — book page + book float migrated: implemented 2026-10-02.**
  - **`service.rs`**: new `BookPageSnapshot` — `detail`, `progress`
    (`Option<(usize, f64)>`, `get_reading_progress`'s actual return; the
    plan's `(i64, f32)` guess was corrected against the code), `stats`,
    `annotations` + `annotations_error` (the highlights toast keeps its
    own wording), `author_profile`, `author_other_books`
    (`owned_books_for_author`, the read §44 found hiding deep in
    `fill_author_card`), and `file_size: Option<u64>` — which the
    service deliberately leaves `None`; the page's worker fills it with
    the `fs::metadata` stat, keeping the service DB-pure and the stat
    off the UI thread. `book_page()` composes the existing
    `book_detail`/`book_stats` plus the three reads the fill functions
    used to do inline; `Send` asserted with the others.
  - **`book.rs`**: `init` builds the skeleton (`loading: true`, empty
    model) and `request_snapshot` sends `book_page` + the file-size stat
    to a `spawn_internal` worker; `Loaded` applies the snapshot, wires
    `file_open_btn` once (`file_btn_wired`), and — for EPUBs only —
    starts the chapter-titles parse on its own worker (the 686 ms
    violation; now off the open path entirely, cached per book id as
    before). `ChaptersLoaded` applies it whenever it lands; the journey
    card says "Loading chapter list…" until then (distinguished from
    "unavailable for this format"). `Refresh` re-requests through the
    same worker path using `book_id` (known from `init`, so a Refresh
    racing the first snapshot still works). The three flag mutations
    (reading list, finished, rating) flip local state instead of
    re-reading — the flag is the write's outcome. `DeleteDone` clears
    the snapshot state with the book. All six fill functions are now
    pure view-from-model. The `rebuild` skeleton branch (loading, no
    book) says "Loading…" instead of flashing "was removed".
  - **`book_float.rs`**: same recipe — skeleton, `FloatSnapshot`
    (detail + progress) on a worker, `Loaded` applies it, `Refresh`
    re-requests. The float's `Remove` was the last synchronous delete
    (a directory removal on the UI thread); it is now a `tasks::spawn`
    task reporting `RemoveDone`, with the same toast wording. The
    progress-location line reads `model.progress`, not the catalog.
  - **Deliberate leftovers, recorded for later steps:** the four
    single-row flag writes (both files) stay synchronous in 2a — they
    are single-statement writes whose result the branch already knows,
    and the 7.7 static boundary check arbitrates the end state, not
    this step; the avatar's `photo.is_file()` stat (pre-existing,
    micro-op) stays for the same reason and joins the 7.7 allowlist
    question; `open_metadata_editor` is 2c/3 scope.
  - **Tests:** `book_page_statement_count_is_fixed` — a book with
    progress, five highlights and a same-author sibling issues exactly
    the same number of statements as a bare book (the `perf.rs`
    count-shaped pattern), and the snapshot carries the seeded state.
    Missing-book behaviour was already covered by the `book_detail`
    tests (`book None`, no errors) and the page's three-way rebuild
    branch was diff-read against it.
  - **Two mistakes on the way, both in pitfalls:** §43 (let-chains in
    an edition-2021 repo — caught on read, no local compiler) and §44
    (the enumeration that stopped above `owned_books_for_author`). One
    near-miss: `FloatSnapshot` was module-private inside a `pub enum`
    (E0446) — caught on read, made `pub`.
  - **Behaviour notes for the field test:** the page and float now
    appear instantly and fill in (the approved skeleton behavior);
    read-error toasts fire once per load instead of once per rebuild
    (the old path re-reported the same stats errors on every message);
    after a mutation the flags update without a re-read, so a failed
    write leaves them unchanged (as before — the write reported the
    error either way).

- **Step 2a — field test (owner, 2026-10-02): the reads moved; the
  freeze that remains is a different cost class.** The owner's report:
  "seems faster than before, but still not instant — the window and
  the info appear at once" (no visible skeleton), plus this log:
  - `route_open:book` **1 ms** (was 96-686 ms) and
    `dialog_open:book_float` **6 ms** (was 31 ms). The 2a migration
    did what it claimed: construction is now instant, the reads are
    off the UI thread.
  - But then: **1355 ms block** starting ~118 ms after the book span
    ended; **751 ms block** after the float span; **667 ms block with
    no span open**; home (unmigrated, 2b scope) **858 ms span + 3958 ms
    and 2955 ms blocks after it**.
  - **Why no skeleton was visible:** the worker round-trip is ~100 ms
    on his machine, the apply (`Loaded` handler) then blocks the loop
    through the first paint of the filled tree — so the first frame the
    user ever sees already carries the data. A skeleton that draws for
    <100 ms is below perception. Skeleton visibility was never the
    goal; **time to first painted content** is, and it is still
    ~0.8-1.4 s.
  - **What the block is not:** apply code. The rebuild builds on the
    order of a hundred widgets; the benchmark builds 2,000 cards in
    7.1 ms. Review of the apply paths (book.rs `Loaded`/`rebuild`,
    book_float.rs `fill`) finds no expensive code — the suspects are
    GTK realize/style/layout/paint of the freshly filled tree (and for
    the float, the new window's first map), the same cost class step 0
    flagged as "post-construction freezes", now the dominant term.
  - **But that split is unproven, and proving it is cheap:** the apply
    handlers carry no `activity` guard and no span, so the watchdog
    attributes their blocks to whatever route span ended last. The
    idle work is equally blind: `icons::init` runs at the first idle
    (`src/main.rs:76`, no guard — measured 493-505 ms in an earlier
    item, matching the 667 ms no-span block), the thumbnail backfill
    and cover-swap stream arrive from housekeeping with no labels
    (`src/app.rs:1850` `start_housekeeping`; `warm_books` at
    `src/pages/home.rs:530`), and home's 3.9 s block could be any of
    them.
  - **One principle violation found in review while chasing this:**
    `cover_widget_deferred` (`src/widgets/book_row.rs`) calls
    `path.is_file()` — a disk stat on the UI thread — for every
    deferred cover, and `fill_author_card`'s avatar check does the
    same. Micro-ops, but they are exactly what ARCH.md forbids; 2a.1
    decides fix (probe on the worker, or accept and allowlist with a
    written reason for the 7.7 static check).
  - **Conclusion (per §38 and the plan's own risk note):** stop before
    2b. Migrating eight more screens against a recipe that moved the
    reads but not the freeze would produce eight more "seems faster,
    still not instant" reports. Step 2a.1: span and guard the apply
    paths and the idle work, one more field run, then choose the fix
    from evidence. Recorded as pitfalls §45.

- **Step 2a.1 — the apply paths and idle work got names: implemented
  2026-10-02.** Exactly the design above: `timing.rs` activity labels
  are owned strings (a task's runtime name rides along; every existing
  literal call site compiles unchanged); `tasks.rs` labels its two
  main-thread dispatch points `task_progress:<name>` /
  `task_done:<name>` / `task_item:<name>` — one wrapping point each
  covers the thumbnail backfill, the cover preloader, the dictionary
  import and every user task; `icons::init`'s idle callback is
  `icons_init`; the book page's `rebuild` and the float's `fill` carry
  `book_page_rebuild` / `book_float_fill` (guard + span), so a stall
  inside the apply code says "while", and one after it says "after" —
  the split between apply code and GTK realize/paint the field log
  could not make. The watchdog's unnamed fallback line now reads
  "no activity was open" (it is no longer only routes and dialogs).
  No behavior change. **One CI-caught defect, recorded as pitfalls
  §46:** the Copy-to-owned label refactor broke `last_ended_activity()`
  (E0507 — the old tuple was `Copy`, so `.map()` had silently copied
  the `Option` out through the `MutexGuard`'s deref; owned labels made
  it a move out of a borrow) and one test's `ActivityGuard { label:
  "literal" }` construction (E0308). Both failures were in lines the
  diff never touched — the lesson is in the entry. Fixed with
  `.as_ref()` in the getter and `.to_string()` in the test. ARCH.md
  gained the rule in the screen-open recipe.
  - **CI round two caught something older than the step: the comic
    series-name heuristic truncated titles containing c/v-words (§47).**
    `importing_a_comic_lands_it_in_its_series_folder` failed once — its
    uuid-padded series name "Import Series cd272d85-…" was truncated to
    "Import Series" because the `" c"` chapter-marker keyword matched
    the uuid's first character and the old number scanner skipped
    letters until it found digits ("chapter 272"). A ~6-8 % flake by
    uuid shape, and a real bug for users ("The Chronicles 1950" →
    "The", chapter 1950). Fixed in `comics.rs`: a strict `marker_number`
    (complete word, optional short alphabetic prefix, at most five
    digits — the app's own chapter-stem ceiling) replaces the lenient
    scanner at all seven sites in both parsers, and both
    trailing-digit rules get the same five-digit cap. Two regression
    tests: the uuid shapes (all four failure forms) and every marker
    form that must keep parsing. Not this step's code — 2.22-era — but
    the gate is the gate. **The owner's next field
  run — the same session as today's — is the gate: its log decides
  whether the 0.7-1.4 s blocks are apply code (optimize the fills) or
  realize/paint (a different fix), what the 667 ms block was, and what
  home's 3.9 s is made of.**

- **Step 2a.1 — field run (owner, 2026-10-02): the log names it. Our
  code is innocent everywhere; the freezes are GTK-side, and the cover
  swap is a relayout storm.** The owner's log, with the new labels:
  - **The apply code is fast.** `book_page_rebuild` 16 / 5 / 6 ms,
    `book_float_fill` 1 ms, `task_done:Organizing comic series
    folders` 0 ms, every `task_item:Preloading covers` 0 ms,
    `icons_init`'s own code 232 ms. The migration recipe is vindicated
    — the 2a shape (skeleton + worker + apply) is not where the
    remaining time goes.
  - **The cover swap is a relayout storm.** `swap_in_cover`
    (`src/widgets/book_row.rs:984-987`) removes the placeholder and
    appends a picture — a container mutation, so every cover arrival
    re-layouts its ancestors. Each callback is 0 ms and invisible in
    the log; the relayouts it schedules are the block. Three of the
    six blocks end right after a cover arrival, and the 1.45 s and
    3.4 s startup blocks sit on the home-screen cover stream landing
    while GTK is already busy with the icon rescan and the first
    layout passes.
  - **The 940 ms startup block is mostly the first frame.** At first
    proof no activity was open; the block ended 41 ms *into*
    `icons_init` — so ~900 ms of it is the app's first draw, before
    icons even started. `icons_init`'s own code is 232 ms; its theme
    rescan (previously measured 493-505 ms standalone) lands in the
    next block.
  - **The 3.4 s block is the startup convergence:** icon-theme rescan
    + home's cover stream + the comic-series organizer + thumbnail
    backfill, all landing while the loop is busy — each of our
    callbacks 0 ms, the cost between them.
  - **The book page's remaining 650 ms (cold) / 250 ms blocks are
    GTK realize/style/layout of the filled tree** — after our 16 ms
    fill returns. The float is the same shape at 250 ms. Same class
    as step 0's "post-construction freezes".
  - Conclusion: 2a.1 did its job — the split it was built to make is
    made. The fix list is now concrete and evidence-backed (step
    2a.2), and the biggest single lever is the layout-neutral cover
    swap, which touches every screen that shows covers.

## The measured ranked list (owner field run, 2026-10-02)

`KALAM_TIMING=1`, one session through the real library. The headline:
**the service reads are innocent** — every `service_*` span is
0.2–11 ms. The costs are widget-tree construction, one EPUB parse, and
startup. That confirms the migration shape (skeleton + async read is
right, but the reads are not the weight — the widget building is) and
adds a target the plan had not sized: what happens *after* construction.

First-open costs, his machine, excluding the two out-of-scope screens:

| Rank | Screen | First open | Warm | Notes |
|---|---|---|---|---|
| 1 | `route_open:library` | **897 ms** | — | dashboard read: 11 ms; the rest is widget building + per-row lookups |
| 2 | `route_open:book` | 96 ms / **686 ms** | 16 ms | the 686 is the EPUB chapter-titles parse on the UI thread — the "parsing" violation; varies by book |
| 3 | `route_open:settings` | **465 ms** | — | no service read at all; pure widget construction |
| 4 | `route_open:home` | **459 ms** | 5–17 ms | first build; `service_home` 344 ms cold then 5.5 ms warm |
| 5 | `route_open:author` | **232 ms** | — | |
| 6 | `dialog_open:book_float` | 31 ms | 3–10 ms | plus a **1.15 s freeze immediately after the first open** — the most-touched dialog |

Findings around the list:

- **Post-construction freezes.** The watchdog caught 250 ms–1.4 s blocks
  *after* routes completed (`after route_open:book …`, `after
  dialog_open:book_float …`): GTK realizing/styling the freshly built
  tree. Construction spans end; the cost lands right after. Skeletons
  shrink it (smaller trees at once); each migration re-measures.
- **Startup is ~10 s cold on his machine**: catalog open **2.3 s**
  (before the loop, splash up), `AppModel::init` **1.2 s** (cold; 7.5's
  425 ms was warm) including the 459 ms home build, then **3.9 s and
  1.8 s UI blocks after the window appeared** — post-paint idle work
  (icons, dicts check, thumbs backfill landing together). The watchdog
  cannot see before the main loop runs, so startup-block boundaries are
  fuzzy — the 7.5 spans-first step must add spans to the idle work.
- **The page cache is mostly dead**: nearly every navigation logged
  `page_cache_miss` — reading-progress writes bump the change token and
  evict everything, exactly as the code comment predicted. Every
  navigation pays full construction, which raises the migration's value.
  Cheap cache fix (progress writes should not evict) is noted for step 4.
- **One late 250 ms block** attributed `after route_open:reading_list
  (ended 5430 ms ago)` — a stale attribution; that block is background
  work no span covers yet. A finding, not a defect in the watchdog.
- **A `Gtk-CRITICAL` (`gtk_widget_is_ancestor`) during reader → home
  navigation** — third appearance of a known family (two mitigations
  already in `app.rs`, each commented with this exact message). Recorded
  as pitfalls §42; the chase is owed on the next reader-teardown touch.
- **Out of scope, flagged honestly:** the EPUB reader's first open was
  **895 ms (576 ms engine open)** on his machine — the bubbles-era
  measurements said 32–78 ms. Warm re-open 138 ms. The owner excluded
  readers from this campaign; the numbers say a look is warranted
  someday. The **PDF reader is the model citizen**: 60 ms construction,
  document open off-thread (507 ms total, UI never blocked during it).
- The small list pages are already fine: shelves 2 ms, review 2 ms,
  reading list 2 ms, analytics 16 ms, tags 14 ms, all-books 8 ms.
