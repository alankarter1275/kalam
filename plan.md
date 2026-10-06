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
- **Step 2a.2 — DONE and CLOSED by the field run (2026-10-02, second
  run). (1) and (2) implemented; (3) accepted-and-remeasured per the
  owner. The fixes landed where they aimed; what remains names the
  next screen.**
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
    honest options went to the owner with the recommendation, and he
    chose **accept and re-measure** (2026-10-02): do nothing about
    icons for now; the field run for (1)+(2) shows whether the rescan
    still matters inside the post-fix blocks. Direct render (load the
    11 reader glyphs — only the PDF reader references them — straight
    from the app, bypassing the icon theme; kills both costs; needs
    the SVGs' `currentColor` baked to a light gray and one visual
    check) is the agreed permanent fix if the run still shows the
    ~730 ms as a standalone bother. Moving the cost into the first
    reader open was declined — the readers are the owner's fast part.
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
- **Step 2b — Home and the library dashboard off the UI thread: DONE
  and CLOSED by the field run (2026-10-02, third run). Home's share of
  the startup blocks is gone and the Home-scroll freeze the owner
  reported is gone with it. One stubborn block remains — see the step
  log.**
  1. **Home first** — it is inside the startup convergence block the
     owner measured (2956 + 1552 ms). Today `init` calls `rebuild()`
     inline (`home.rs:184`), which runs `service.home()` (`home.rs:318`)
     and builds stat tiles + continue/tbr/recent cards on the UI
     thread, then warms covers. The service seam already exists (A0
     step 2). New shape: skeleton in `view!` (page structure + hosts +
     a light loading row), `HomeMsg::Loaded(Box<HomeSnapshot>)` from a
     `spawn_internal("Reading home")` worker, apply = the existing
     `rebuild` body minus the service call. `ImportFinished` re-requests
     the snapshot instead of rebuilding inline. Cover warming unchanged
     (deferred frames + layout-neutral swaps already shipped).
  2. **Library dashboard second** — `init` runs
     `service.dashboard(FEED_LIMIT)` inline (`library.rs:76`) and
     `build_dashboard` (`library.rs:100`) does per-row catalog lookups
     (progress, format) during building. Same skeleton/worker/apply
     shape, plus: fold the per-row lookups into `DashboardSnapshot`
     (the A0-step-2 move Home already received) so the apply is pure
     widget building. The lookups are enumerated from the code at
     implementation start, not from memory.
  3. **Acceptance:** neither `init` contains a service call
     (grep-checkable, stated in the step log); one field run after —
     the 2956/1552 ms startup blocks should lose Home's share, leaving
     the accepted icon cost and stream pacing.
  4. **Not in scope:** the all-books windowed grid and the readers
     (owner-excluded); the icon rescan (accepted, revisit only if it
     still bothers after 2b); GTK's realize/layout cost of the
     migrated screens (a later question, only if it still matters).
- **Step 2b.1 — kill the ~3 s startup convergence block (owner's pick,
  2026-10-02): idle-priority, yielding task drains + pacing the first
  cover batch.** The theory survived the code reading: both task
  drains run as one `spawn_future_local` async task looping
  `while let Ok(item) = rx.recv().await` (`tasks.rs:402`,
  `tasks.rs:489`) — a buffered burst is processed entirely inside ONE
  main-loop dispatch (nothing else, not input nor the watchdog's
  heartbeat timeout, can interrupt a running dispatch), and the source
  sits at default priority where a continuous stream ties with the
  heartbeat (`stall.rs:85`) and GDK input. The cover preloader makes
  it concrete: the first `PRELOAD_AHEAD` (24) covers decode with no
  pacing at all (`preload.rs:166`) — a burst the comment itself admits
  stutters. Fix, all surgical:
  1. ~~`tasks.rs`: drain-side fairness~~ — **withdrawn after three CI
     hangs** (pitfalls §50's revision): the idle-priority move hung
     runs one and two; the per-item yield alone hung run three (the
     hung test executes exactly one yield — the first diagnosis had
     cleared the wrong test). The drains are byte-for-byte the proven
     delivery. Fairness is producer-side only, and any drain-side
     change reopens with a local reproduction.
  2. `preload.rs`: pace every cover (4 ms before each decode after the
     first). The unpaced 24-batch was the startup burst; `PRELOAD_AHEAD`
     loses its code role and is removed, comments updated.
  3. Not touched: the accepted icon rescan (~500 ms, one dispatch);
     the thumbnail backfill (steps only on missing thumbnails, and its
     drain now yields); GTK's first layout passes (step 3's question).
  Acceptance: one more field run — the ~3 s block should break into
  its parts (rescan ~0.5 s + first layout passes) or vanish between
  them; if it survives unchanged, the mechanism is elsewhere and the
  investigation reopens with a span on the backfill.
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
- **Step 5 — proactive warm-up (the owner's idea, discussed and adopted
  for planning 2026-10-04; not started — begins after step 4 and its
  field test, and after the owner's go).** The owner asked: instead of
  reacting to clicks, presume what the user will open and have it ready,
  loading in the background, fully async. The discussion's conclusion,
  and his logs are the evidence:
  - **Data is already cheap — warm widgets, not data.** Every service
    read measures 0.2–12 ms; prefetching data saves nothing. What makes
    first opens heavy is widget construction and GTK's
    once-per-session first styling of a widget type. So the warm-up
    pass builds screens, not snapshots.
  - **The pass, idle-paced, after home paints:** build once, off-screen,
    the screens his logs measured expensive on first open — book page
    (753 ms first realize), settings (550), picker (351), reading list
    page (322), book float (250). The user's real first click then
    finds the cost already paid.
  - **The one genuinely slow read:** the library dashboard (~300 ms
    worker-side on first visit) — prefetch it at boot.
  - **Rides on step 4's cache decision.** The owner's instinct landed
    independently on "keep visited pages alive" — a vote for **fix**
    over delete, now the leading option for that decision; a fixed
    cache can also keep warmed pages alive instead of discarding them.
  - **Risks, on record:** background work starts only after first paint
    and is paced (the splash-pump convergence lesson — bg work landing
    together froze the UI after startup); warmed-and-kept pages cost
    RAM, warmed-and-discarded costs CPU; wrong predictions are free
    (wasted background work, no jank in front of the user).
  - **Acceptance:** the owner's log shows the warm-up itself causing
    zero stalls, and the warmed screens' first opens measuring clean.
    No performance conclusion from a single field run.

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

- **Step 2a.2 — field run (owner, 2026-10-02, after the fixes): the
  migrated screens improved everywhere; the two big startup blocks
  barely moved because they were never the migrated screens' code —
  they are Home.** Previous run → this run:
  - First frame: 940 ms → 444 ms (no activity open → into
    icons_init, which itself measured 219 ms code).
  - Book page fill: 16 ms → 3 ms; its post-fill block 650 ms → 250 ms
    warm (851 ms for the first open of the session, which includes
    the route + skeleton realize).
  - Float fill: 1 → 3 ms; its block 250 → 350 ms (GTK window map +
    tree realize — same class, unchanged, as expected).
  - Startup convergence: 3456 ms → 2956 ms, and the follow-on block
    1453 → 1552 ms — **unchanged in substance**. These blocks sit on
    Home's synchronous build (`home.rs:184` `rebuild()` inline in
    `init` — `service.home()` + stat tiles + continue/tbr/recent
    cards, all on the UI thread inside the window's first layout
    window), plus the accepted icon rescan, plus the cover/thumbnail
    streams arriving while Home builds. The organizer's done-callback
    is 0 ms and triggers nothing UI-side (`app.rs:1943` — completion
    only writes a timing note); it is a coincidental attribution
    marker, not a cause.
  - One open signal: the log's last line is a block still in progress
    with no activity for 4363 ms — unattributed. Asked the owner what
    he was doing at that moment (scrolling the grid? opening a
    reader?) before drawing conclusions.
  - Conclusion: 2a.2's work is done — apply code on the migrated
    screens is 1-3 ms and their blocks are GTK's realize/layout, not
    ours. The log points at step 2b: Home and the library dashboard,
    the two unmigrated screens, built with the proven recipe.

- **Step 2b — implemented 2026-10-02 (both pages, the proven recipe;
  the owner's go included the confirmation that the unnamed 4363 ms
  freeze was scrolling Home — more evidence for this step).**
  - **Home**: skeleton in `view!` (one loading row inside the continue
    host, cleared by the first apply), `HomeMsg::Loaded` from a
    `spawn_internal("Reading home")` worker, `apply_snapshot` = the old
    `rebuild` body minus the service call. `ImportFinished` re-requests
    the snapshot. Cover warming unchanged. Apply labelled `home_fill`
    (measure + activity) so the next field run attributes.
  - **Library dashboard**: same shape (`Reading library dashboard`,
    `library_fill`), plus the per-row lookups folded into the service:
    `DashboardSnapshot.now_reading` (the "Now reading" book *plus* its
    EPUB spine and chapter index — the spine is a file open + parse the
    page used to do on the UI thread mid-build, a 2.20 violation) and
    `DashboardSnapshot.feed` (the merged events + sessions feed with
    every per-event progress/format lookup already baked into the row
    subtitles; `FeedItem` + `history_feed` moved from the page into
    `service.rs` as `DashboardFeedRow` + `dashboard_feed`). The three
    remaining sync `cover_widget` calls (now-reading 72×104, continue
    120×170, quote 48×68) became `cover_widget_deferred`.
  - Acceptance holds by construction: neither `init` contains a service
    call (both grep-clean), and the page switch from `SimpleComponent`
    to `Component` matches home/book.

- **Step 2b — field run (owner, 2026-10-02, third run): Home's share
  is gone, the scroll freeze is gone, the book page's covers now
  arrive with the data — and one ~3 s startup block survives all three
  runs unchanged.** Three-run comparison:
  - Follow-on startup block (home's old build + cover stream):
    1453 → 1552 → **751 ms** — halved. `home_fill` never even appears
    in the log's attributions (0-4 ms), and the owner scrolled Home
    this time: **no stall line at all** — the unnamed 4363 ms scroll
    freeze from run two is gone.
  - Book page: fills 4/6 ms; blocks 655/451 ms — and for the first
    time the blocks end on `book_page_rebuild` itself, not on cover
    arrivals: the covers decoded on the page's own worker (2a.2 part
    2) are arriving with the snapshot exactly as designed. What
    remains is the known "later question": GTK realize/style/layout of
    the freshly filled page.
  - Float: 250 ms, fill 4 ms — GTK window map, unchanged class.
  - First frame: 384 ms (icons_init's own code measured 74 ms this
    run).
  - **The survivor: the ~3 s startup convergence block (3456 → 2956 →
    3055 ms).** It starts ~100-250 ms after `icons_init` ends and ends
    near the organizer's 0-ms done-callback (coincidental marker, per
    app.rs:1943). With home's fill now 0-4 ms, this block is no longer
    any page's code: it is the startup *stream convergence* — the
    icon-theme rescan (accepted, ~500 ms), the cover preload's first
    24-cover burst (PRELOAD_AHEAD decodes with no pacing before it),
    the thumbnail backfill's progress events, and GTK's first
    layout/paint cycles of the filled home tree, all interleaved
    continuously enough that the 50 ms heartbeat never wins. The
    likely mechanism is priority: if the task/cover channel drains at
    default priority, a back-to-back event stream starves both the
    heartbeat timeout AND user input — the freeze is real, not a
    watchdog artifact. Next: read `tasks.rs` channel priorities and
    `warm_covers` pacing (investigation, then likely a small fix:
    pace the first batch too, and/or drain at idle priority so input
    and timeouts always win).

- **Step 2b.1 — implemented 2026-10-02 (the owner picked it over 2c):
  the task drains yield and run at idle priority; the first cover
  batch is paced.** `tasks.rs`: both drains (spawn's progress+result,
  stream's items) moved from `spawn_future_local` to
  `MainContext::spawn_local_with_priority(PRIORITY_DEFAULT_IDLE, …)`
  and now await a hand-rolled `yield_to_the_loop()` after every item —
  one item per dispatch, below input/heartbeat/resize priorities
  (pitfalls §50 records both halves: a buffered burst inside one
  dispatch, and a default-priority stream tying with input).
  `preload.rs`: every cover sleeps 4 ms (the first batch included);
  `PRELOAD_AHEAD` lost its code role and is removed; the backfill was
  re-checked and left alone (it steps only on missing thumbnails, and
  its drain now yields). Field run owed — acceptance: the ~3 s block
  breaks into its parts or vanishes; if it survives unchanged, the
  mechanism is elsewhere and the investigation reopens with a span on
  the backfill.

- **Step 2b.1 — field run (owner, 2026-10-02, fourth run): the pacing
  did not kill the big block — it grew, 3055 → 4259 ms — and the log
  finally shows the mechanism directly: the main loop is saturated by
  the arrival drip, and everything else queues behind it.**
  - The big block now ends **while `home_fill` is open, 78 ms in**.
    Home's snapshot read finished long before (its worker starts at
    app init), but the `Loaded` apply could not get a dispatch slot
    until the flood subsided — **the page's own fill was queued ~4 s
    behind cover arrivals and GTK work**. That is queue starvation
    caught in the act, and it also explains why `home_fill` measured
    78 ms (contended) instead of run three's ≤ 4 ms.
  - Everything downstream also grew: the follow-on block 751 → 1352
    ms, the book page 655 → 951 ms, the float 250 → 450 ms. The
    direction is consistent across every pair: **spreading the
    arrivals over a longer window made the busy period longer, not
    shorter** — each arrival's GTK aftermath (repaint/resize cycles)
    bridges the gap to the next arrival, so the loop never gets the
    ~50 ms of quiet the heartbeat (and input) needs.
  - New, separate signal: `route_open:settings` measures **429 ms** —
    the settings page is still synchronous (2c's confirmed target,
    with a number now). A late unnamed 350 ms block ~3 s after
    settings opened (nothing recent) — unexplained, noted for 2c/3.
  - Conclusion: producer-side pacing is insufficient and slightly
    harmful (revert to the run-three shape, which measured best). The
    fix must reduce the flood's *main-thread widget work*, not its
    timing: the designed next step is a swap coalescer — the drain
    inserts into the texture cache only (no widgets), and all pending
    swaps apply in one batch from a standard
    `idle_add_local_once(DEFAULT_IDLE)` — a classic GLib pattern (the
    same family as the icons deferral), no future-source scheduling
    semantics, nothing like the two changes that hung CI. If the
    owner prefers, the startup block parks until a reproduction
    environment exists and 2c (settings 429 ms, author page) proceeds
    first.

- **Step 2b.1 — the owner's repeated-launch experiment (2026-10-02,
  same release, four consecutive launches): conclusive. Cold start was
  the amplifier; warm, the multi-second startup block does not exist.**
  | launch | biggest block | notes |
  |---|---|---|
  | 1st (cold) | 4259 ms | `home_fill` queued behind the flood, 78 ms |
  | 2nd | 2783 ms | `home_fill` 4 ms again, ran at the block's end |
  | 3rd | **450 ms** | nothing above 450 anywhere in the log |
  | 4th | **450 ms** | same, reproducible |
  - **Warm steady state achieved.** Two consecutive warm launches show
    no block above 450 ms: first frame 350-363 ms, two icons-window
    blocks (250 + 450), one post-organizing 350, book page 250-351,
    float 250. Every fill measures 1-11 ms in every run
    (`home_fill` 4, `book_page_rebuild` 3-11, `book_float_fill` 1-2,
    every task callback 0).
  - **The icon rescan is now isolated cleanly:** in the warm runs
    `icons_init`'s own code is 4 ms (SVGs cached on disk) and a
    ~450 ms block follows it — the theme rescan, matching its old
    standalone 493-505 ms measurement. The accepted cost, now visible
    as its own line item.
  - **The queue-starvation mechanism is threshold-dependent:** cold,
    slow decodes stretch the arrival stream past saturation (the 2.8
    -4.3 s block, home's apply queued seconds behind it); warm, the
    stream is short enough that the loop gets its gaps. The pacing's
    warm A/B was never measured and is moot — warm behavior is good
    either way; it stays.
  - **What remains:** (1) the cold-start block, once per boot — the
    only multi-second freeze left, and the coalescer's exact target if
    ever taken; (2) warm hiccups of 250-450 ms (first frame, icon
    rescan, book page realize, float map, settings 429 ms — 2c's
    confirmed target); (3) one unnamed late 350 ms block after
    settings (unexplained, small).

- **Step 2b.1 — CLOSED (2026-10-02, late): the cold-start block is
  handled by roadmap 1.15's infrastructure plus one flag.** The owner
  remembered `warm-cache.sh`/`kalam-warm.service` (1.15, built for the
  cold *window open*); the service never passed `--covers`, so covers
  stayed cold and became the post-window block. His test on a genuine
  reboot: biggest stall **1051 ms** (vs 4259 unwarmed), window_shown
  2266 ms, `icons_theme` down to 11.5 ms. The service in-repo now
  passes `--covers`; the owner reinstalls the unit. What remains of a
  cold boot is step-3 startup work (`pre_run` 1980 ms: libraries 157,
  db_open 133, theme 126) plus the 250-650 ms blocks also present
  warm; settings 522 ms is 2c's target. The coalescer is shelved
  unless the block returns unexplained. Startup campaign (7.1 step 2
  family) is DONE: warm steady state ≤ 450 ms stalls, cold boot ≤
  ~1 s with the service installed, every fill 1-11 ms.

- **Step 2c — planned and implemented 2026-10-03 (owner said go):
  settings + author to the skeleton/worker/apply recipe.** Research
  first, and it corrected the step-0 note that settings "has no reads
  at all": the page reads plenty, just invisibly — direct `catalog`
  and helper calls, not `service_*` spans, so the step-0 log showed
  none of them. Full enumeration (every read the old `init` did on
  the UI thread):
  - DB: `list_dictionaries`; `theme::current` (1 pref);
    `sidecar::survey`'s `list_books`; `list_shelves`;
    `load_watch_rules`; `source_enabled` x2; prefs
    `meta.googlebooks.key`, `meta.googlebooks.country`;
    `clean_on_import_enabled`; `write_enabled`.
  - FS: `sidecar::survey`'s per-book sidecar reads (one file read
    per book — the heaviest single item, likely the bulk of the
    465-522 ms); `libraries::load_registry` (file);
    `epub_metadata::list_backups` (a directory walk).
  - In-memory only: `notify::history` (stays on the UI thread).
  Design, matching the book-page precedent for splitting a heavy
  secondary read (its chapter-titles worker):
  - **`SettingsSnapshot` (DB-pure, `LibraryService::settings()`,
    span `service_settings`):** dicts + dicts_error, theme_id,
    watch_rules, shelves, the two source flags, the google key and
    country prefs, clean_on_import, writeback. Fast — ~10 quick
    statements — so the page fills almost immediately.
  - **`SettingsFs` (the page's own worker, `spawn_internal`
    "Checking storage", like `BookPageSnapshot::file_size` the
    service stays DB-pure):** the sidecar survey, the libraries
    registry, the epub backup listing — the slow disk-flavored
    reads, in their own task so they never delay the page fill.
    Rows that depend on it show "Checking…" until it lands; tabs
    built before it arrives are rebuilt when it does.
  - **Lazy tabs:** the old `init` built all six tabs' widgets; now
    only the active tab builds at `Loaded`, the rest build on first
    visit from the snapshot already in the model (no reads), each
    with its own `settings_fill:<tab>` span + activity so the field
    run measures the per-tab split for free. Tab switches after
    first visit are instant (already built).
  - **Dict refresh paths** (`Refresh` after import, `DeleteDict`,
    `MoveDict`): the inline `list_dictionaries` re-read becomes a
    light worker read (`DictsLoaded`) that rebuilds the dict tab if
    built. The single-row writes themselves stay synchronous this
    step (7.7 arbitrates the end state, same decision as 2a's flag
    writes).
  - **Author page — `AuthorPageSnapshot` (`LibraryService::author_page`
    , span `service_author`):** profile, owned_books, series (the
    grouping computed worker-side). init paints the skeleton with
    `loading = true`; the network photo fetch now starts only when
    the snapshot arrives and the profile is absent (init can no
    longer know that); `Fetched` stores the profile then re-requests
    the snapshot instead of re-reading `owned_books` inline; every
    fill goes through `Loaded` (span `author_fill`).
  - Budget tests, mirroring `book_page_statement_count_is_fixed`:
    `settings_statement_count_is_fixed` (empty vs seeded dicts +
    shelves + prefs — same count) and
    `author_page_statement_count_is_fixed` (0-book vs 3-book author
    — same count). `assert_send` for both snapshots.
  - Click-path reads that pre-date 2c and stay (noted, not touched):
    `render_watch_cards`' re-read after add/remove watch folder, the
    toggle handlers' small pref writes. Step 4's static check
    arbitrates them.
  Acceptance (owner field run): `route_open:settings` drops from
  ~522 ms to skeleton-only (tens of ms); `settings_fill:*` lines
  appear and each stays well under the 150 ms watchdog line;
  `route_open:author` likewise with an `author_fill` line; no new
  stall anywhere; behavior identical apart from appear-then-fill.

- **Step 2c — implementation notes (2026-10-03), written as the code
  landed.**
  - Two corrections from the deep read of `get_pref` (pitfalls §51 in
    the making): several of the settings reads are *global* prefs
    served from `prefs.json` before the database (theme, the metadata
    sources, the Google key/country), so the service function is
    "DB-pure" in the sense of no library-shaped reads — the global
    file lookups ride along inside `get_pref`, still off the UI
    thread. And the first settings budget test draft seeded prefs,
    which flips `load_watch_rules`' legacy fallback and changes which
    statements fire; the shipped test seeds table rows only
    (dictionary + shelf) and asserts the pref defaults by value.
    §51 records the rule — twice: CI then caught its second face in
    the author test, where empty-vs-busy is also a control-flow
    difference (`hydrate_books` skips its batched tag query for an
    empty list, so an unknown author is one statement cheaper than
    one with books). The shipped author test compares one book vs
    three — flat in book count, the property that matters.
  - The dict refresh paths (`Refresh`/`DeleteDict`/`MoveDict`) all
    re-request the whole snapshot rather than a dicts-only read: ten
    quick statements, one code path, and `Loaded` already knows how
    to rebuild the visible tab plus a built-but-hidden dict tab.
  - The author page's network fetch decision moved from `init` to the
    first `Loaded` (init can no longer know whether a profile is
    cached); `snapshot_seen` gates it so a later `Loaded` (the
    post-fetch re-read) never re-fires the fetch. After `Fetched`,
    the page briefly shows the new profile beside the previous
    owned-books list until the re-read lands — imperceptible, noted
    here so a future diff of the behavior is not a surprise.
  - Every tab build is announced as `settings_fill:<tab>` (span +
    activity), the author fill as `author_fill`, the worker reads as
    `service_settings` / `service_author` — the field run gets the
    per-tab split for free.
  - Two CI catches on this step, both recorded: the author budget's
    first draft compared an empty author against a busy one and
    failed on the statement count (§51's second face — `hydrate_books`
    skips its batched tag query for an empty list); and the step's
    first push carried an invariant `.expect` in `build_tab` that
    tripped the panic ratchet (§41's recurrence — the shipped shape
    is a `&self` method whose missing-snapshot case is a documented
    early return, so the count stays 12).

- **Step 2c — first field run (owner, 2026-10-03): the migration
  targets are confirmed; the run itself was a cold launch without the
  warm-up, which answered the owner's "book details took too long"
  question on its own.**
  - **Settings: complete success.** `route_open:settings` **5.4 ms**
    (was 465-522), `service_settings` 1.2 ms on the worker,
    `settings_fill:appearance` **22.5 ms** — and reopening later was a
    page-cache hit with zero work. The old number is gone.
  - **Author: opens instantly, but its fill is now the heaviest named
    piece of our code.** `route_open:author` **0.5 ms**,
    `service_author` 0.9 ms, `author_fill` **362 ms** (one watchdog
    line at 450 ms around it — preload tail + GTK realize of the
    built tree). Pre-migration the same work sat inside the old
    232 ms `route_open:author`; not a regression, but the biggest
    remaining named fill — a split fill (hero first, cards streamed)
    is the candidate fix if the owner feels it.
  - **The whole launch was cold without warming**: `startup_gtk_init`
    1764 ms (vs ~100 warm), `startup_db_open` 2248 ms (vs 133),
    `pre_run` 9284 ms, `window_shown` 10403 ms (vs 2266 in the
    warmed reboot test). Even the worker reads were cold
    (`service_home` 328 ms vs 4 warm). The settings survey rode it
    out off-thread ("Checking storage" finished ~12 s in).
  - **The owner's felt delay was real**: first book float blocked
    ~1.1 s and the first book page ~2 s *after* our code returned
    (route 2.6 ms, rebuild 5.6 ms) — cold-disk cover decodes
    saturating the loop, the same threshold-dependent starvation as
    the old startup block, now landing on first opens. Later book
    pages: 250 ms block, then none — the machine warmed under him.
  - Open questions put to the owner: was this first-launch-after-boot,
    is the updated `--covers` service installed/enabled (the cold
    db_open says the warm-up did not run), and which author the
    362 ms fill was. Two targeted runs requested, not 3-4 blind ones:
    one reboot with the service installed (expect window ~2 s, no
    multi-second first-open delays), one warm launch right after
    (expect the clean steady state).
  - **Correction (owner, same day): there was no boot.** The run came
    from the same session as the 2026-10-02 measurements — he
    `git pull`ed, ran `cargo build --release`, and launched the fresh
    binary. That explains every cold number by itself: **a release
    build evicts the page cache on a 4 GB machine** — the linker
    writes a fresh ~100 MB binary and the whole toolchain's memory
    pressure pushes GTK's libraries, catalog.db and the covers back
    out of RAM. First-launch-after-rebuild is cold *by construction*;
    his own log shows the machine warming under him (first book page
    ~2 s blocked → later 250 ms → none). The "service did not run"
    inference was wrong; the service question that still stands is
    only whether the **updated** `--covers` unit is installed (the
    2026-09-20 install predates the flag).
  - **Protocol rule for field measurements (owner-adopted):** after a
    rebuild, discard the first launch — or run
    `./scripts/warm-cache.sh <bin> --covers` once — and measure on
    the second. Never draw a cold/warm conclusion from a
    first-launch-after-build. The owner asked whether the warm script
    must be run manually every time: **no** — once the updated unit
    is installed it runs itself at login, and every launch after the
    first keeps things warm naturally; the only event that re-colds
    the machine mid-session is a build (or a memory-hungry app
    evicting it, the standing 1.15 caveat).

- **Step 2c — CLOSED (2026-10-03, the paired reboot/warm runs): the
  service is proven, the first-open delays are gone, and every 2c
  target holds in both states.** The step-2 migration family
  (2a.2, 2b, 2b.1, 2c) is validated end to end.
  - **Run 1 (reboot, service installed, first launch):** gtk_init
    99.6 ms, db_open 168.4, window_shown **2246.9 ms** — matching the
    earlier warmed test (2266) almost exactly, against 10403 for the
    unwarmed rebuild run. Biggest stall **751 ms**, nothing
    multi-second. The felt first-open delays are gone: the first
    book float fills 0.3-6.6 ms with **no stall line at all** —
    pre-warmed covers mean fast decodes mean no queue starvation.
    Settings 5.1 ms + fill 17.9; author_fill 254.8 (first launch);
    first PDF open 637.8 total. The service did its job.
  - **Run 2 (warm, second launch):** window_shown **1188.9 ms** — the
    best full-launch number ever recorded. Biggest stall **450 ms**
    (the organizer's post-done block, present in every run — a
    step-3 span target), then 350/250s. Every fill 1-7 ms: settings
    5.1 + 17.1, home 4.9, book page 3.9, floats 1.3-4.3.
  - **The author question is answered: Blake Crouch, warm, fills in
    80.5 ms.** The 362 ms was mid-recovery contention, the 254.8 the
    first-launch cost — no fix needed; the split-fill idea is parked
    unless a warm fill ever exceeds the watchdog bar.
  - 2c final ledger: settings open 465-522 ms → **5 ms** (fill 17-23);
    author open 232 ms → **0.5 ms** (fill 80 warm). Warm steady
    state: window 1.19 s, stalls ≤ 450 ms, our code 1-7 ms
    everywhere. Cold boot with the service: window 2.25 s, stalls
    ≤ 751 ms, first opens clean.
  - **Step 3 targets, now measured:** `pre_run` 1173-1977 ms (its
    biggest named parts: libraries 0.2-169, db_open 3.9-168, theme
    29-105, style 51, gtk_init 99 — the majority is still
    unaccounted inside it, so 7.5's spans-inside-init come first);
    the once-per-boot icons write 81.8 ms + its 450-527 ms
    neighborhood; the organizer's post-done 450 ms; the ~350-650 ms
    first-frame draw. Small list pages ride along (most already
    ≤ 16 ms).

- **Step 3 — startup + small pages: planned 2026-10-03 (owner said
  go). Research first; it reshaped 7.5's original question.**
  - **7.5's target dissolved.** "Break down `AppModel::init` (424.9
    ms)" was written before the migration; the latest warm run shows
    `pre_run` 1173.1 → `init_done` 1183.4 — **init is ~10 ms warm**
    (262 ms cold), because the first page is now a skeleton. Nothing
    inside it needs spans beyond the existing `startup_first_page`.
  - **The unaccounted ~980 ms lives in `main()`, between the spans.**
    Warm run arithmetic: named spans (gtk_init 98.6 + style 51.7 +
    icons 4.5 + libraries 0.2 + db_open 3.9 + theme 28.8) sum to
    ~192, yet `pre_run` marks 1173 elapsed. The unspanned region is
    exactly: `stall::install` (trivial), the icons idle registration
    (trivial), **`splash::show()`** (window + CSS + maximize +
    present), **`splash::pump()`** (a fixed 120 ms of
    drain-and-sleep, inside which the icons callback fires and — the
    prime suspect, per the watchdog's own "blocked 350 ms after
    icons_init" line — the splash's **first paint dispatch**: one
    `ctx.iteration` containing the first frame draw and the GL
    one-time setup 1.14 recorded at 2.6-3.3 s cold / ~350-450 warm),
    and `logging::init` (trivial). Cold run: same gap ~1290 ms.
    Hypothesis, to be measured not assumed: show + pump's first-paint
    dispatch ≈ 900+ of the 980.
  - **Phase 3.0 (this slice): spans only, no behavior change.**
    `measure` + `activity` around `splash::show` and `splash::pump`
    (the icons_init idiom), so the next field run splits show / pump
    / first-paint and the watchdog names blocks inside the pump
    "while splash_pump" instead of "no activity was open".
  - **Phase 3.1: the account, from one field run** (warm; cold
    optional). Then phase 3.2 decisions WITH the owner, because every
    lever is a visible tradeoff: the pump's fixed 120 ms sleep (drop
    to fewer frames?), the splash's shape (maximized full-screen
    paint vs a small centered card — cheaper first frame), the
    cold-only db_open overlap (open on a worker while pumping —
    rejected ideas on record: theme must apply before first window or
    the default palette flashes), the once-per-boot icons write
    (81.8 ms — owner already accepted), and the organizer-post-done
    450 ms block (present every run; the done-callback is 0 ms, so it
    is attribution context — the real content is the cover-preload
    tail + first realize; activity labels on preload items, labels
    only, no drain-side scheduling — §50's rule).
  - **Small pages survey (the code, ahead of the field data):**
    - Already async (Loaded from workers): history, saved_quotes,
      tags.
    - Snapshot-shaped but read inline on the UI thread:
      saved_words (`service.words` in `reload`), shelf_detail
      (`service.shelf_detail` in `reload`), and likely
      lookup_history, reading_list, analytics (same `reload` shape —
      to confirm against their route_open numbers before touching
      any).
    - **Comics is the exception: fully synchronous and heavy** —
      `reload` calls `migrate_comic_series_and_chapters()` (a
      migration, on the UI thread), `list_books` (the whole
      library), `list_remote_books`, `list_comic_series`, then
      **`chapters_for_series` per series (an N+1)**. It gets the
      full skeleton/worker/apply recipe when its turn comes.
    - Field run must visit each small page once so every route_open
      is measured; migrations then go in descending-cost order, each
      with its statement-count budget test per the recipe.
  - Acceptance: the startup account fully attributes `pre_run` warm
    and cold (no unaccounted residue over ~50 ms); every small page
    has a measured route_open; the pages over the watchdog bar are
    migrated or explicitly accepted by the owner.

- **Step 3.0 — the account closes (owner field run, 2026-10-03;
  first-launch-after-rebuild, cold page cache).** The spans sum to
  `pre_run` exactly: gtk_init 526.2 + style 51.8 + **splash_show
  3694.8** + splash_pump 534.3 (icons 59.7 inside) + libraries 121.0
  + db_open 383.2 + theme 60.5 = 5371.8 vs `pre_run` 5372.3. **Zero
  unaccounted.**
  - **`splash_show` is the biggest item in a launch — and it is not
    our bug.** What it measures is GTK's *lazy* one-time work,
    front-loaded by creating the first window: the base theme's CSS
    parse and first style machinery, the windowing backend, first
    font resolution — costs that would otherwise land in the first
    real frame after `app.run`. The `startup_style` span (51.8) only
    covers `set_color_scheme`; the actual theme load happens at
    first window. Without the splash the same costs would be paid
    into a blank screen; the splash exists to own exactly this dead
    time. Warm, `splash_show` derives to **~580 ms** (980 gap minus
    pump's ~400) — direct warm measurement owed from one more run.
  - **`splash_pump` 534.3 cold**: icons (59.7, once per boot) plus
    the splash's first-paint dispatch (the block reports show
    430 ms inside it — first frame draw + GL one-time setup, the
    1.14 cost class).
  - **The verdict on warm startup, reframed by the account:** of the
    ~1190 ms warm launch, our code is **~45 ms** (libraries 0.2 +
    db_open 3.9 + theme apply 28.8 + first page 0.8 + init ~10).
    The rest is GTK's one-time costs (gtk_init 99 + style 52 +
    splash_show ~580 + first paint ~400), paid once per process no
    matter what draws first. There is no startup *defect* left to
    fix — only the accepted once-per-process GTK bill, now fully
    attributed.
  - **Small pages, first five measured — all fine, no migration
    needed:** history 31.5, saved_words 5.0, tags 4.6, analytics
    15.6, reading_list 1.9, comic_series 3.0-8.3 (ms, route_open;
    every service read 0.1-0.8 on its worker). The inline-read pages
    (words, analytics, reading_list) are so cheap the recipe would
    be ceremony — recorded as accepted-synchronous.
  - **Still unmeasured (not visited this run): the Comics hub — the
    one with the migration call + whole-library read + per-series
    N+1 — plus saved_quotes, lookup_history, shelf_detail.** Owed
    from a supplemental warm run.
  - Cold-run remainder, consistent with every cold run before: home
    first fill 282.8 contended with a 1653 block (cold cover
    arrival stream), preload tail 1351 — the threshold-dependent
    starvation, gone warm; the warm service covers it at boot.

- **Step 3.1 — comics as series on every surface (owner bug report,
  2026-10-03): implemented, CI green (run 37126083231 — 510 tests,
  0 failed, both new tests in the log; clippy -D warnings clean;
  headless smoke session clean).** The owner, at his shelf, furious:
  comics show as *individual chapters* on the shelf — "I told you I never
  want individual chapters!!! I have repeatedly told you this." His
  screenshot could not be viewed (no vision this session — told him
  plainly); his words plus the code are the spec, and the code
  confirms it exactly.
  - **Evidence.** `All Books` collapses (`collapse_comic_chapters`,
    service.rs `all_books`; db/series.rs:819) and routes comic cards
    to the series page (`AllBooksOut::ComicSeries`,
    pages/all_books.rs:1188-1207). Home's strips are series-deduped
    in SQL (`recent_books`/`recently_opened`, db.rs:1225,
    db/history.rs:233 — the 100-chapter-import lesson) and route to
    `ComicSeries`. But the shelf leaks end to end: `shelf_detail`
    (service.rs:495) never collapses, the page grid
    (pages/shelf_detail.rs:326) has no comic routing, the manual
    remove strip removes ONE chapter, and the shelves-grid badge
    counts chapters raw (`SELECT shelf_id, COUNT(*) … GROUP BY`,
    db/shelves.rs:31 — one 700-chapter series shows as "701 books").
    Same class, same fix: reading list (service.rs:856,
    db/shelves.rs:308 — raw chapters, no routing), tag books
    (service.rs:879, db/stats.rs:28), author page owned books
    (service.rs:619 → author.rs:187), and the dashboard's continue and
    now-reading cards (pages/library.rs — series-deduped lists but
    clicks open the *chapter's* book dialog). Watch folders assign per chapter
    (docs/conversation.md:2080) — that stays; membership stays
    id-keyed (the 2.22 design); only display, counts, removal and
    routing collapse.
  - **Design.** (1) Service: `shelf_detail`, `reading_list`,
    `tag_books`, `author_page` collapse their book lists via the
    existing, tested `collapse_comic_chapters` (reading list keeps
    the surviving entries' position/note by collapsing the books and
    retaining matching entries). (2) Pages: `ComicSeries {
    series_name }` output + comics_map routing copied from all_books
    into shelf_detail, tags, reading_list, author, and the dashboard
    continue card; app.rs wires each to `Route::ComicSeries`. (3)
    Removal is series-scoped: new `comic_series_peers(book_id)` db
    helper (registered peers via comic_chapters.series_id; heuristic
    peers by the same series-key rule collapse uses, title-parse
    included, resolved in Rust not SQL) → `remove_series_from_shelf`
    and `remove_series_from_reading_list` delete every chapter;
    non-comics keep the single-book remove. (4) Counts match what
    the cards show: manual grouped count, `shelf_book_count`, and
    `count_matching_rules` dedupe with the same key rule
    (series_id, else series-or-parsed-title) so a badge never says
    701 again. The smart-shelf editor count inherits the fix.
    (5) Resume/play buttons and the reader stay chapter-precise —
    "continue reading" must open the exact chapter he left.
  - **Tests.** db: peers helper (registered + heuristic), both
    series-scoped removes, collapsed counts for manual and rules;
    service: shelf_detail collapses to one card per series (mirrors
    the existing db-level collapse test at db/series.rs:1229).
  - **Verification.** CI green (clippy -D warnings), then the owner:
    a shelf holding a comic series shows one card, one count, click
    opens the series page, remove clears the series.
  - **Implemented as designed, plus:** the dashboard's now-reading
    cover got the same comic routing (same page, same class); the
    heuristic series rule moved into one shared `heuristic_series_key`
    (db/series.rs) so collapse, peers and counts cannot drift; the
    grouped manual count became a per-shelf key-dedup in Rust.
    Pitfalls §52 records the edit-anchor slip caught during the work.
    Owner field verification owed — the checklist above.

- **Step 3.2 — My Library covers (owner report, 2026-10-03, same
  session's log): implemented, CI green (run 37148814722 — the new
  guardrail ran and passed; 548 tests, 0 failed).** "In My Library
  the covers are not shown" — found and confirmed in the code: a
  **step-2b regression, not a cold-cache artifact.** Step 2b (db94bbd) converted
  the dashboard's three synchronous `cover_widget` calls to
  `cover_widget_deferred` without adding the warm calls. A deferred
  cover fills only when a texture with its exact `(path, w, h)` key
  is decoded, and the only producer is `warm_books`/`warm_covers` at
  that size. The dashboard asks for 72×104, 120×170 and 48×68; home
  and the grids warm 128×204, the comics hub 150×210, the book page
  its own sizes — nobody ever decodes the dashboard's keys, so its
  placeholders stayed for the life of the page on **every** launch,
  cold or warm (the in-process cache starts empty per process). The
  same audit says every other surface pairs its deferred calls with a
  warm of the exact size — the dashboard was the only miss, exactly
  the failure mode documented in `warm_books`' doc ("Home originally
  built deferred cards and never warmed them").
  - **Fix:** the dashboard fill now warms its three sizes — the
    now-reading card (72×104), the continue strip (120×170), the
    quote cards (48×68, paths deduped). Sizes unchanged; each surface
    warms its own.
  - **Tripwire:** `deferred_covers_are_warmed_by_the_page_that_builds_
    them` in tests/guardrails.rs — any file calling
    `cover_widget_deferred(` without a `warm_books(`/`warm_covers(`
    fails CI. Zero offenders after the fix; the 2b bug itself would
    have been this test's first red.
  - **Pitfalls §53** records the half-contract lesson.
  - **Same log, the shelf fix confirmed in the data:** shelf_detail
    visits show `grid_cards 2, grid_cards_total 2` then `1/1` — the
    collapsed grid no longer grows as chapter windows arrive (the
    pre-fix log grew 1→4); `service_shelf_detail` 0.4-1.6 ms,
    `service_shelves` 0.7 ms and `route_open:shelves_grid` 3.8 ms —
    the collapsed count query costs nothing measurable. The owner
    walked shelves → shelf detail ×3 → home → library → comic series
    → task manager and raised no shelf complaint.
  - **The launch itself was cold** (splash_show 4458.2, pre_run
    6427.8, window_shown 7379.7, home_fill 1254.3 with a 1452 ms
    block, the cover stream still landing 9 s in): the
    rebuild-evicts-the-page-cache protocol from 2c says the first
    launch after a rebuild is cold by construction and is discarded.
    Asked the owner whether this was that first launch; the covers
    bug stands regardless — it reproduces warm.

- **Step 3.3 — the "+ Add books" picker (owner report, 2026-10-03,
  third session): implemented, CI green (run 37151158507 — after one
  red build CI caught: `tx.commit()`'s `rusqlite::Error` tail versus
  the file's `DbError` alias, fixed in `a2ae134`; 511 tests, 0
  failed, the picker test among them).** "When I clicked on the add
  books options on the shelf… it's cut off from the sides! …in that
  list, the comics books are listed by chapter… again." The picker
  was the one list surface the 3.1 audit missed — a dialog, not a
  page, so it never appeared in the page-by-page sweep. Two defects,
  one screen:
  - **Chapters, again.** The picker filled from raw `list_books`:
    a 700-chapter import = 700 rows. Fixed in both pickers (shelf
    and reading list — the same dialog shape): the list collapses
    via `collapse_comic_chapters`; a comic row is the series, ticked
    when **any** chapter is already a member, tick/untick
    add/remove **every** chapter (`add_series_to_shelf` /
    `add_series_to_reading_list`, new single-transaction bulk adds —
    hundreds of chapters, one commit, positions continuing the
    order; for a non-comic they are exactly the old single-row
    calls). Regular books skip the peers lookup so the per-keystroke
    fill costs what it did before. `any_in_reading_list` answers the
    tick state for a whole peer set in one query.
  - **Cut off from the sides.** Two causes, both real: (1) the
    picker rows were un-ellipsized `CheckButton`s in a
    never-scrolls-horizontally window — one long comic chapter title
    set the list's minimum width past the panel and GTK clipped both
    sides; rows now ellipsize and fill. (2) A dead CSS rule:
    `.kalam-in-app-dialog { min-width: 0 }` was written to free
    in-app dialogs from the book float's fixed 700×368, but
    `.kalam-float { min-width: 700px }` appears later in the file
    and equal specificity loses to source order — every in-app
    dialog has been forced to ≥700 px wide since the rule was
    written. Fixed with a compound selector
    (`.kalam-float.kalam-in-app-dialog`), which beats source order.
    Pitfalls §54 records both lessons.
  - **Tests:** `pickers_add_and_remove_a_series_as_one` (db level —
    series tick adds both chapters with one card on the badge,
    untick clears them, the epub stays; `any_in_reading_list` sees a
    series through any chapter).
  - **Same log closes two 3.0/3.2 accounts.** The covers fix is
    confirmed live: `covers_queued 1` + `covers_queued 6` at
    library_fill — the dashboard's three sizes are being warmed, and
    the owner raised no covers complaint. And the **direct warm
    splash readings owed since 3.0**: `splash_show` **139.8 ms**,
    `splash_pump` **900.0 ms** (holding the 350+450 ms first-paint
    and GL one-time blocks — the once-per-process bill, now measured
    directly), `pre_run` 1396.8, `window_shown` **1416.7 — his best
    launch ever recorded.** The 3.0 warm derivation (~580 show /
    ~400 pump) had the total about right but the split wrong: warm,
    GTK's lazy init inside `splash_show` is cheap; the first-paint
    cost sits inside `pump`. The owner says he ran the warm script;
    whether this was the first or second launch after it, the
    earlier 4458 ms splash reading (also stated to be launch 1 with
    the script run) does not fit this one — noted honestly, not
    chased; the discard-first-launch protocol stands. Warm blocks
    this run: 250-651 ms (cover arrivals + realize of freshly built
    trees, the known class), `service_dashboard` 131.5 on first
    call.

- **Step 3.4 — the picker width, second attempt (owner report,
  2026-10-03, fourth session): implemented, CI green (run 37154255853
  — after one red build CI caught, an ambiguous `as_ref()` on
  `&gtk::Box`; 511 tests, 0 failed).** The series
  collapse held ("the comics appear in just one entry" — his
  words), but the dialog still ran edge to edge: "the width, I
  cannot see the left and right side of the dialog box." The
  picker's attachment did not reach this session's workspace, so
  the Pillow measurement he suggested (good idea — installed and
  ready) had nothing to measure; his description was the spec, and
  it named the mechanism precisely: **the panel spans the window.**
  - **The real cause, found by reasoning from his words:** a
    centred dialog panel sizes itself to its content's **natural**
    width — and `set_ellipsize` caps a label's **minimum** width
    only, never its natural width. The first fix was therefore the
    wrong half of the same contract as pitfalls §53: it stopped the
    rows from *forcing* the width, but one long text still *sized*
    it. Before the collapse fix the wide row was a scanlation
    chapter name; after it, some long title/author string was still
    doing the same job. `set_max_width_chars(44)` caps the natural
    width — the row now truly cannot size the panel past ~520 px.
  - **Belt and braces:** the picker's width request is now
    window-aware — `min(520, window_width - 120)`, floored at 320 —
    so a narrow or tiled window gets a dialog that fits it with
    borders visible. Both pickers (shelf and reading list).
  - **Pitfalls §55** records the minimum-vs-natural lesson: the
    3.3 "fix" was verified by reasoning, not by measurement — and
    reasoning about which of the two widths a mechanism uses is
    exactly where it went wrong.
  - **His launch answer settles the warm-script question:** the
    1416.7 ms log was **the very first launch after running the
    script after compilation** — the warm script makes launch one
    of a fresh binary warm. The earlier 4458 ms splash (also
    claimed as launch 1 with the script) most plausibly had the
    script run against the pre-build binary or before the compile
    finished; recorded as the likely story, unproven.
  - **Second-run log, same build:** `pre_run` 1190.2,
    `window_shown` **1206.2 — new best**, `db_open` 6.4,
    `splash_show` 110.8 / `splash_pump` 889.5 (the same two
    once-per-process first-paint/GL blocks inside), one 450 ms
    block after `route_open:home` (first home realize), nothing
    else above the bar. `shelf_detail` 1 card, 1 total — the
    collapse steady. `covers_queued 0` on re-open — the dashboard
    covers came from cache.
  - **Third run (same build, later session): the photo channel is
    broken on the platform side.** The owner attached the picker
    screenshot again; the platform reports it saved to the
    workspace, but the file never arrives — a full filesystem
    search finds no image. Told him plainly (third time). The
    replacement measurement, in this project's own idiom: the
    pickers now log `picker_window` (the window's width at open)
    and `picker_width` (the picker's final allocated width, noted
    from an idle dispatch after the first layout pass) — the next
    log answers the fit question with numbers, no photo required.
    The run itself: `window_shown` 1199.8 (steady ~1.2 s), shelf
    pages clean (1 card, four visits, no stall), settings 4.8 +
    18.8 first-tab fill with the known 550 ms realize block,
    library covers warmed again (1 + 6). The one-bit question —
    borders visible or still edge to edge — put to the owner in
    words, with the log notes as the standing answer.
  - **Fourth session — the data arrives and convicts the row fix:
    `picker_window 1350`, `picker_width 1906`.** The dialog is
    556 px wider than the window — wider than his entire screen —
    matching "still the same as before" exactly. (The first open
    logged `picker_width 0`: the idle fired before the first
    allocation; the second open measured for real.) So the
    `max_width_chars` cap set through `with_label`'s internal
    child never took effect — reaching into a widget's internals
    to constrain it was the weak link, unprovable until the picker
    could measure itself.
  - **Third fix, layered so it cannot silently fail again:**
    (1) the rows now carry an explicit label we built ourselves
    (`CheckButton::new()` + `set_child`), ellipsized and capped;
    (2) CSS `max-width: 640px` on `.kalam-float.kalam-in-app-dialog`
    caps the *panel's* natural width whatever any row does — the
    hard backstop for this whole failure family (GTK ≥ 4.6; the
    app's builds require newer); (3) `picker_natural` joins the
    log notes, so the next run names the failing layer if one
    ever does: natural still huge = the rows; natural small but
    width huge = the allocation. Pitfalls §56.
- **Step 3.4 — closed (owner field verification, 2026-10-04, fifth
  session): fixed, and his log convicted a second mistake.** His log:
  `picker_window 1350`, `picker_natural 520`, `picker_width 520` — on
  both opens, 830 px to spare. The owner confirmed it fixed. But the
  same log carried `Theme parser error: <data>:725:5-14: No property
  named "max-width"`: GTK CSS has no max-width property — the CSS
  "backstop" of the third fix never existed, and §56's "GTK >= 4.6"
  claim was recalled from web CSS, not checked against GTK's property
  list. The layer that fixed the picker was the rows' owned labels —
  `picker_natural 520` is the rows themselves asking small. Correction
  shipped: the invalid property removed (the same rule's
  `min-width: 0px` stays — load-bearing against `.kalam-float`'s
  700px floor), the CSS comment now names where the cap lives, and
  the width tripwire is `picker_natural` in the log. The same warning
  was sitting unread in CI's green-run smoke logs
  (`tasks-latest.txt:82`, `reader-latest.txt:88`) — green is not
  warning-free. One intermediate red run (37157210097, E0308): gtk-rs
  `measure()` returns a four-tuple, not a pair — fixed, Pitfalls §57.
  The CSS lesson is Pitfalls §58. **New observation, owner's call:**
  opening the picker blocked the UI thread ≥ 250 ms twice in his log
  (stall lines right after `picker_window`) — the series row list is
  built on open, on the UI thread; the async-build pattern other
  surfaces already use would apply if he queues it.
- **Step 3.5 — the picker open stall (owner-ordered, 2026-10-04,
  sixth session): implemented; plus the supplemental small-page run
  that closes the step-3 survey.** The owner's word was "fix", so the
  picker stall goes ahead of step 4.
  - **The supplemental run (his machine, warm):** comics hub
    `route_open:comics` **8.3 ms**, saved quotes **6.1 ms** first /
    **2.4 ms** warm, lookup history **12.4 ms**. The step-3 small-page
    survey closes with **no performance migration owed**: every small
    page measured ≤ 12.4 ms. The comics hub's scary code shape
    (migration call + whole-library read + per-series N+1, all inline)
    is measured harmless warm on his library — the shape is still a
    7.7 violation and is handled by step 4's enforcement, not by a
    speed migration. Also in that run: `service_dashboard` 315.3 ms
    (worker-side; warm variance on an earlier 163.6), the known
    cover-preload stall family recurring (250–550 ms blocks across
    pages, already documented), and the first `page_cache_hit` lines
    ever logged — 3 hits against ~15 misses, so the cache is still
    mostly dead and step 4 still decides fix-or-delete.
  - **The fix, both pickers (shelf + reading list), the history-page
    recipe applied to a dialog:** the panel presents immediately with
    a "Loading…" skeleton row — the open now does zero reads and
    builds zero rows; `fill` spawns `tasks::spawn("Loading books", …)`
    whose work closure (worker thread) does everything the old
    UI-thread fill did — `list_books`, `collapse_comic_chapters`,
    membership (shelf set / `any_in_reading_list`), per-comic peers —
    and returns ready-made `PickerRow`s (id, title, toast line, tick);
    `on_done` (main loop) clears the skeleton and builds the
    CheckButtons. A generation counter drops superseded answers
    (keystrokes re-fill; the list keeps what it shows and swaps on
    arrival — it never blanks mid-read). Search semantics are
    unchanged: every fill still goes through `list_books`' real query
    engine — the search grammar (`parse_search_query` /
    `build_search_sql`) is a query engine, not a substring filter,
    so in-memory filtering would have silently changed what matches.
    Toggle writes stay as they were (single small writes, owner-
    accepted). The width diagnostics moved: `picker_natural` /
    `picker_width` now fire once after the first rows arrive — the
    skeleton's width says nothing about cut-off rows. New notes for
    the next log: `picker_read` (worker ms), `picker_fill` (UI ms),
    `picker_rows` (count) — if any stall remains, the log names the
    side: read vs widget-build. No new pitfall — recipe application,
    not a new mistake.
  - **Owner-verified (2026-10-04, his log): the fix works, and the
    log separated what remains.** `picker_read` 0.8–2.5 ms (worker,
    even on a cold launch), `picker_rows` 13, `picker_fill`
    0.9–1.1 ms, `picker_natural`/`picker_width` 520 on 1350 —
    second and third opens completely clean. The first open still
    blocked the thread ≥ 351 ms, but the numbers acquit the picker:
    the read and the build together are ~2 ms. It is the
    once-per-session first-realize family (GTK's first styling of a
    widget type) — the same log shows it on the first book float
    (250 ms) and the first book page (753 ms). Known family; no new
    scope.
  - **New finding, the last inline page confirmed: the reading list
    PAGE builds its rows on the UI thread.** `route_open:reading_list`
    **322.1 ms** with `service_reading_list` **0.7 ms** — the read is
    innocent; `reload()` reads inline (fast) and `rebuild()`
    constructs every row inline (the ~321 ms). This was the "likely"
    entry never confirmed; now confirmed heavy. Same recipe applies
    (skeleton + worker read + apply). Queued for the owner's word —
    fix it, or straight to step 4.
  - Run context: this was a cold launch (`startup_db_open` 470.3 ms
    vs the usual 6.5–25.7; `pre_run` 1823.5 vs ~1200 steady) — the
    picker verdict is unaffected (sub-3 ms reads even cold), but
    step-4 measurements use the second launch as usual.
- **Step 3.6 — the reading list page (owner's go, 2026-10-04, sixth
  session): implemented.** His word was "yup" to fixing it, then step 4.
  The 322 ms was never the read (`service_reading_list` 0.7 ms) — it was
  `cover_widget`, the decode-on-the-spot cover variant, called per row:
  every cover on the page was decoded on the UI thread at open, and again
  on every rebuild (Move/Remove/Refresh). The fix is the deferred-cover
  contract, both halves: `cover_widget_deferred` in `build_row`
  (placeholders instantly) and `warm_books(&books, 0, 44, 70)` after
  `rebuild` queues the decodes at the exact key — after the first warm the
  cache serves every rebuild instantly, so reordering stays cheap too. New
  notes: `reading_list_build` (span) and `reading_list_rows`, so the next
  log reads the build cost directly. The read stays inline — 0.7 ms,
  bounded, the accepted snapshot shape; step 4's budgets arbitrate the
  final state. Pitfalls §59 records the guardrail gap (the warming test
  cannot catch a page that should have used deferred and didn't) and the
  same-family suspects for the next field log: author page group covers
  (232 ms first open, 2026-10-02, cause never verified) and comics hub
  group covers (8.3 ms — fine at the owner's scale). Neither is owed
  without data. After this fix ships and CI is green: **step 4 begins**
  (owner's instruction, 2026-10-04).

  (owner's instruction, 2026-10-04).
  - **Step 3.6 owner-verified (2026-10-04, his log): closed.** First
    open `route_open:reading_list` **2.5 ms** (was 322.1) with
    `reading_list_build` 1.1 ms and `reading_list_rows` 3; his
    reorder/remove rebuilds ran 3.5–5.3 ms with `covers_queued` 0 —
    every cover from cache after the first warm, exactly the
    contract's second half. One `page_cache_hit` also appeared on the
    revisit — the first repeat-visit hit on this page in any field
    log. The same-family suspects from §59 stay suspects (author page
    group covers, comics hub group covers) until a log measures them.
- **Step 4 — enforcement: BEGUN (2026-10-04, seventh session; the
  owner's instruction: "when that happens, start step 4").**
  - **Increment 1 — per-screen snapshot budgets, shipped:** six budget
    tests in `src/perf.rs` (the proven pattern, extended): the
    statement count of each step-3 survey page's service read must not
    depend on how many rows the page shows — `reading_list`,
    `shelf_detail`, `quotes`, `words`, `history`, `lookup_history`,
    each seeded FEW (5) vs MANY (15) page rows over the same 20-book
    library. The row counts are asserted to differ before the
    statement counts compare, so a silently-broken seed cannot pass
    vacuously.
  - **Deliberately not covered yet, in order:** the step-2 migrated
    pages' reads (home, dashboard, book page/detail, settings, author,
    all books) and the picker read (its worker does
    `comic_series_peers` per comic series — an N+1 shape that a budget
    would catch; the read needs to move into a service method first to
    be budgetable); then the page-cache eviction decision; the static
    boundary check comes last.
  - **Debt on record (§19):** the new tests reuse the
    sabotage-verified harness but have not themselves been
    sabotage-verified — one deliberate break-and-revert cycle is owed
    before step 4 closes, same discipline as the original six.
  - *Session note:* the workspace was re-cloned between turns and this
    increment's first commit accidentally squashed the whole branch
    history; the push was rejected (the safety net), the changes were
    re-parented on the real history with byte-identical trees, and the
    incident + recovery recipe are pitfalls §60. No remote history was
    ever at risk.
- **Step 4, increment 2 — the big pages, test-first, and the first live
  catch (2026-10-04, eighth session): shipped, green (run 37172160623,
  921 tests).**
  - Six more budgets in `perf.rs`: `home`, `dashboard`, `book_stats`,
    `book_page`, `settings`, `author_page` — same invariant, growth axis
    = the page's own rows (reading-list entries, events, sessions,
    annotations, shelves, author's books).
  - **Test-first on purpose, and it caught a real one.** The tests landed
    before the fix (commit `9a61ba9`); red run `37170951890` failed
    exactly one test — `dashboard_snapshot_does_not_scale_with_events`,
    **38 statements for 5 events vs 48 for 15**: `dashboard_feed` called
    `get_reading_progress` once per Opened event (and `get_book` per
    Imported). Bounded by the feed limit, but the one-query-per-row shape
    exactly. 522 other tests passed — the intended blast radius. The fix
    (next commit): a batched `Catalog::reading_progress_by_ids` (same
    shape as `books_by_ids`: dedupe, chunk at 500, empty input issues
    nothing; unit-tested) and `dashboard_feed` reading both lookups from
    batched maps. This observed red also settles the §19 debt for the
    shared harness of increments 1–2 — `assert_constant_in_page_rows`
    has now failed on a real regression in CI, which is stronger than a
    staged sabotage; each test's seed axis stays guarded by its
    row-count assert.
  - **The fix run then failed the panic ratchet and exposed a latent
    scanner bug (pitfalls §62):** `src/db.rs` "grew" one production
    panic — a helper's `.expect` *inside* `#[cfg(test)] mod tests`. The
    scanner's suppression was a single level: an inner `#[test]` fn
    overwrote the mod's level and, on closing, cleared it entirely, so
    an unannotated helper after the first inner test leaked into the
    count. It had never fired because every file's helpers happened to
    sit before its first test. Fixed with a proper stack (push on a test
    item, pop when depth falls below the top, outer levels resume) plus
    the nested regression case in the scanner's self-test.
  - **Known axis recorded, not silently avoided:** smart shelves are
    counted individually in `list_shelves` (rules compiled per shelf) —
    a settings budget over smart shelves would fail today. Later
    increment's decision: batch it or allowlist it in writing.
    `all_books` is skipped on purpose (its core read, `list_books`, is
    already budgeted at the catalog level).
- **Step 4, increment 3 — the comic axis: the pickers move into the
  service, batched, and budgeted (2026-10-04, eighth session): shipped,
  green (run 37175756396, 925 tests).**
  - The pickers' read lived inline in the pages, out of the budgets'
  reach — and it was the worst N+1 of the set: after collapsing comics
  to one row per series, the reading-list picker ran
  `any_in_reading_list` once per row and `comic_series_peers` (one to
  three queries) once per comic row; the shelf picker ran the peers
  lookup once per comic row. On a comic-heavy library the dialog's read
  scaled with every row it showed.
  - New `service.reading_list_picker` / `service.shelf_picker`: the
  whole read (list, collapse, peers, membership) as one call — one
  batched `Catalog::comic_series_peers_by_ids` (books_by_ids shape:
  dedupe, chunk 500, empty issues nothing; registered chapters in one
  self-join, the heuristic fallback once per batch) plus one membership
  query (`reading_list_book_ids` / `shelf_book_ids`). The pages keep
  their skeleton/worker/generation shape; `picker_read` measures inside
  the service so field logs stay comparable.
  - Two budgets in `perf.rs`, the **first comic-format seeds in the
  file** — the axis is comic series (registered, two chapters each, one
  chapter pre-queued/pre-shelved so the ticked half is exercised) plus a
  constant heuristic pair, so `collapse_comic_chapters` and both peers
  paths are finally exercised by the budgets.
  - The owner's rule now pinned in the **service layer** too:
  `pickers_collapse_comics_and_tick_through_any_chapter` asserts one row
  per series (never a chapter title), and that one queued/shelved
  chapter ticks the series row — registered and heuristic both.
  - **The new unit test caught a real bug before it shipped:** asking
  the batched peers lookup for BOTH chapters of a heuristic pair
  returned nothing for the second — the grouping consumed its group
  with `remove`, so the first book of a series starved any later book
  sharing the key. The pickers only ever ask representatives, so
  production would not have hit it — but the API's contract is "any
  ids", and the test pins the contract, not the caller. Fixed with
  `get` + clone (commit `9bdb444`).
  - Two CI cycles burned on the way in (pitfalls §63): a scripted
  rewrite replace() that did not assert it fired silently missed the
  shelf picker's call site (a type mismatch three files from the cause);
  `add_comic_chapter` takes f32, not f64.
- **Step 4, increment 4 — the page-cache fix: reading activity no
  longer evicts (2026-10-04, ninth session): shipped, green (run
  37205713716, 926 tests, first try).**
  - The field run had measured the cache mostly dead: every write —
  including the progress save on each 1% of scroll — bumps
  `total_changes()`, which the cache invalidated on, so reading a book
  wiped every cached page. Fix: `Catalog::page_cache_token()` =
  `change_token()` minus the rows written by reading activity. Exactly
  five methods count as activity, all pure reading telemetry:
  `set_reading_progress`, `mark_book_opened`,
  `start/checkpoint/end_reading_session`. The set stays that small and
  named because a wrongly-counted content write is the one unsafe
  direction (a stale page); forgetting to count a new activity write
  only costs one spurious rebuild. `auto_finish_if_complete`
  deliberately stays content (it mutates the reading list and the
  finished flag). The stats memo keeps the raw token — progress and
  sessions feed its aggregates.
  - **Leaving a reader forces one fresh build of the landing page**
  (new `page_cache_forced_miss` counter): without it, the home you
  return to would come from the cache showing the reading position from
  before you read — the continue card is too prominent for that. Every
  other cached page now survives a reading session; the landing page
  costs exactly one rebuild, which is what today's behavior costs every
  page.
  - Known trade, offered to the owner as a veto: cached pages that show
  activity data (the dashboard feed, history) can lag by the reading
  they missed until the next content write. The field test's hit/miss
  counters (`page_cache_hit` / `_miss` / `_forced_miss`) will judge
  whether the trade reads right in practice.
  - **Pitfalls §60 recurred mid-chunk** — second re-clone of the day:
  HEAD at the fork point again, work intact in the working tree. The
  recorded recipe worked, plus a refinement now in the entry: after the
  soft reset the *index* still held the base tree, so `git diff
  --cached` showed the session in reverse (180 files, 46k deletions) —
  a mixed reset is the step that shows the true diff.
- **Step 4, increment 5 — the static boundary check (7.7, step 4's
  last item): shipped, green (run 37208539920, 928 tests, first
  try).**
  - The disk and document legs of ARCH.md's one rule are now enforced
  by a guardrails scanner: `pages_touch_disk_or_documents_only_inside_
  tasks` walks `src/pages/**` line by line, suppressing exactly the
  first closure after a `tasks::spawn` trigger (the worker — the task
  layer); the progress and done callbacks run on the main thread and
  stay scanned. Reader files are excluded file-level (the owner
  excluded them from the campaign). The database leg stays with the
  existing enforcement: the `Arc<Catalog>` ratchet and the per-screen
  statement budgets. Every allowlist entry carries its written reason
  in the table, and a stale entry (matches nothing) fails the test —
  the list cannot rot into decoration. A self-test pins the
  suppression semantics.
  - **The check arbitrated as planned — its first pass found real
  violations, and they are fixed:** six export paths wrote files on
  the UI thread (quotes page Export + ExportAllData, words page
  CSV/Anki/Markdown, settings' two export buttons) and the comics
  page expanded dropped folders with `read_dir` on the UI thread. All
  now run as tasks, the established recipe; the quotes page's status
  line returns as a new `ExportDone` message so the label updates
  when the file is real.
  - **Allowlisted with written reasons:** four micro-op stats
  (cover/avatar `is_file`/`exists` checks — the 7.1 step-2a
  allowlist question), the export/expansion helper bodies (they write
  files by design; they run only from task workers now, and a
  synchronous call site is still caught by the call-name patterns),
  remote-detail's bounded 160×230 thumbnail decode in its
  apply-on-arrival step, and two recorded follow-ups: the metadata
  editor's save flow (cover read in the file-dialog callback, cover
  written into the EPUB in the save handler) and remote-detail's
  ReadChapter cache flow (`OpenBook::open` ×2, `remove_dir_all`).
  Each is its own migration, not forgotten.
  - Step 4 is now **complete**: per-screen budgets (small pages + big
  pages + the comic axis), the page-cache fix, and the static
  boundary check — with the smart-shelf counting axis recorded for a
  later increment and the two follow-ups above. The owner's
  field-test build is next.
- **Step 4 field run #1 (2026-10-05, owner's machine, KALAM_TIMING=1)
  — big wins across the board, but the build predates today's two
  increments; a second run on the latest tip is owed.**
  - **Build identification:** the reader → home transition logged a
  plain `page_cache_miss`, not `page_cache_forced_miss` — the forced
  landing-page rebuild only exists since increment 4 (today). So the
  run tested roughly the increment-3 tip: the page-cache fix and the
  boundary-check increment's export/comics fixes are NOT in it. In
  this build, reading a book still evicts the cache (visible in the
  log at the reader exit).
  - **Wins vs the 2026-10-02 run:** post-window startup blocks 3.9 s +
  1.8 s → worst ~0.55 s (window shown at 1.26 s); home build 459 ms →
  `home_fill` 3.5–4.6 ms over a 3.3 ms snapshot read; the book float
  250 ms → `dialog_open` 2–4 ms; the book page 753 ms → 6.7 ms
  rebuild over an 80.8 ms worker-side snapshot; reading list stays
  1–4 ms (owner-verified earlier); dashboard read 269.3 ms → 114.6 ms
  worker-side (`route_open:library` 0.2 ms — the UI never waits);
  every small page 1–12 ms; numerous `page_cache_hit`s on revisits
  even pre-fix (they die only when a write intervenes — e.g. the
  startup "Organizing comic series folders" task evicted home once).
  - **Remaining, ranked from this log:** (1) the recurring 250–550 ms
  stalls tagged `task_item:Preloading covers` — the successor to the
  "recurring 650 ms after library_fill" family: cover decodes run on
  a worker, but each finished cover lands on the main thread
  (`cache_decoded_cover` → texture per cover) and the watchdog names
  the last item; the real blocker (texture uploads bunching, or GTK
  layout triggered by the swaps) needs spans on that path before
  anyone chases it. (2) Reader first open 587 ms with a ≥550 ms stall
  inside `route_open:reader` — readers are excluded from the campaign
  by the owner's scope, noted only. (3) ~350 ms blocks during the
  941 ms startup splash pump — pre-window, watchdog attribution fuzzy
  there. (4) `service_dashboard` 114.6 ms is the heaviest read left
  (worker-side; the UI never blocks on it). (5) One stall attributed
  "after task_done:Checking storage (ended 18 s ago)" — a known
  watchdog attribution limitation, not a finding.
  - **Action:** the owner rebuilds from the latest tip and runs once
    more; that run judges the page-cache fix (hits should survive
    reading; the reader exit should log `page_cache_forced_miss` once).
- **Step 4 field run #2 (2026-10-05, owner's machine, latest tip) —
  the two field questions answered yes, and it came back with two
  reader bug reports plus one gap of mine.**
  - **Increment 3 is field-confirmed:** the reading-list picker logged
    `picker_read 0.6 ms`, `picker_fill 1.3 ms`, 13 rows,
    `picker_window 1350 ms` — no scaling stall anywhere near the old
    N+1 (run #1's build already had it, but this run seals it).
  - **Increment 4 is field-confirmed on its main half:** a
    `page_cache_hit` landed immediately after the EPUB reader closed —
    a cached page survived a whole reading session for the first time
    in the app's history.
  - **And increment 4's second half exposed its one gap:** not a single
    `page_cache_forced_miss` in the whole log. The forced rebuild was
    wired into `swap_page` only, but reader close buttons send
    `AppMsg::Back`, which navigates around `swap_page` — so the landing
    page after a reader came from the cache with the pre-session
    position. (The PDF reader's close did land as a plain miss, not a
    forced one: his spread-toggle testing had written prefs, and a
    non-activity write evicting is the designed conservative rule, not
    a defect.) Fixed this session — see the reader-fixes entry.
  - **Numbers vs run #1:** window 1291 ms, splash pump 965.9 ms with
    ~350 ms blocks, post-window worst ~550 ms ("Organizing comic
    series folders"), dashboard 49.0 ms (was 114.6), epub reader open
    171.8/99.5 ms, pdf `route_open` 121.1/146.6 ms,
    `pdf_open_total` 672.3/224.1 ms, comics reader 175.1 ms.
  - **New leads for the next hunts:** (1) `author_fill 301.9 ms` with
    a ≥350 ms stall while it runs — the heaviest UI-thread fill left
    after the big pages were migrated; needs the same
    skeleton/worker/budget treatment (its read is batched already, the
    fill is the cost). (2) one ≥951 ms stall after
    `book_page_rebuild`. (3) the `Preloading covers` 250–550 ms family
    persists — still the #1 hunt, still waiting on spans for that
    path. (4) Watchdog misattributions recurred (stale task_done /
    home_fill anchors — known limitation, not findings).
- **Reader fixes (2026-10-05, tenth session): the three reports of
  run #2 — two from the owner, one mine — root-caused and fixed.**
  - **PDF: blank pages after a settings toggle (owner: "unless I
    scroll a bit").** The three viewport-rebuilding settings (scroll
    mode, spreads, gap) all did `set_child` →
    `scroll_to_current_page` **immediately** — but right after
    `set_child` the scrolled window's adjustment still describes the
    OLD layout; GTK allocates the new child on the next layout pass.
    The scroll therefore computed its position against stale geometry,
    landed the view in the wrong place, and the value-changed handler
    (`UpdateScrollPage`) then re-derived `current_page` from wherever
    the stale scroll value sat in the NEW geometry — far from where
    the reader actually stood. `prune_textures` + `trigger_loads` then
    ran around that wrong page: the pages on screen kept placeholder
    styling until the user scrolled, which re-ran the whole recompute
    against finally-settled geometry and loaded what was actually
    visible. The proof the mechanism is right was in the file all
    along: `DocumentLoaded` (initial open) restores the reading
    position through a **`glib::idle_add_local_once`** — the open path
    already defers this exact scroll past the allocation, which is
    exactly why opens never showed the bug. Fix: the three settings
    now set `viewport_rebuild_pending` instead of scrolling; a new
    `LayoutSettled` message fires from the adjustment's `upper`
    notify (the "new extent is real" moment — `page_size_notify` was
    already wired but only tracks viewport resizes, and the child swap
    changes `upper`, not `page_size`) and runs the deferred scroll +
    a follow-up `trigger_loads`. Page-scrolling mode never scrolls
    (unchanged); page jumps (next/prev/first/last/bookmark) still
    scroll immediately — no child swap, the adjustment is already
    truthful.
  - **EPUB: a dark band along the upper edge (owner).**
    `.kalam-reader-stage` carried `padding-top: 12px` from the mockup
    shell era, and the stage's background is the app background — so
    a 12px strip of app bg sat above the reading surface on the top
    edge only (left/right/bottom were flush). Whenever the reading
    theme's paper did not match the app background, the strip read as
    a dark band. Fix: the padding is gone; the paper runs flush on
    all four edges.
  - **Mine: reader close could serve a stale landing page.**
    `force_rebuild_next` (increment 4's forced fresh build of the page
    you land on after a reader) was set in `swap_page` only; reader
    close buttons send `AppMsg::Back`, whose handler does its own
    detach → route → take_or_build and never set the flag. Fix: the
    Back handler now applies the same `matches!` rule (same three
    reader routes) before detaching. `take_or_build` has exactly two
    callers (swap_page, Back) — no other gap exists.
  - **§19 sabotage-verify, owed since increment 4:** the page-cache
    token test shipped green first try and had never failed (the
    harness red of increment 2 settled increments 1–2; increment 3's
    unit test caught the peers bug; increment 5's scanner caught six
    export paths — increment 4's test was the one check with no
    observed red). The break-and-revert cycle ran across three runs:
    the fixes run green (`37330958345`), the sabotage (`page_cache_token`
    back to raw `total_changes()`, the exact pre-fix bug — every
    progress save evicts) went red in `37332625604` failing **exactly
    one of 529 tests** (528 passed) on its intended assert — "a whole
    reading round trip must not evict the page cache", token 5 vs 13,
    the eight activity rows that used to evict the whole cache eight
    times over — and the revert (`06861ea`) came back green in
    `37334144108`. The debt is settled: every step-4 check has now
    failed for real or by staged sabotage; none is decoration.
- **Step 4 field run #3 (2026-10-05, owner's machine, the
  reader-fixes build) — the close-path cache fix is confirmed by the
  log; the covers family now owns every stall that has a fresh
  anchor.**
  - **The Back-path fix is field-confirmed:** `page_cache_forced_miss`
    appears TWICE — once after the EPUB reader close, once after the
    PDF reader close (run #2 had zero). Both landings are instant
    (route open 0.6/0.8 ms, home_fill 4.2/4.7 ms). The pdf-blank and
    epub-band fixes are visual — he used both readers in this run;
    his eyes are the verdict there.
  - **Everything the campaign touched stays fast:** home 4.2–4.8 ms
    on every visit; book float 2.2 ms + 1.1–3.4 ms fills; pdf open
    124.9/161.0/350.1 ms (first open better than run #2's 672);
    `covers_queued 12` at startup, `0` on the revisit; window_shown
    1276.4 ms (same family as runs #1–2).
  - **The covers storm is now the whole fresh-anchor story:** from
    startup through the EPUB open, blocks of 250–951 ms whose
    anchors are 16–223 ms fresh (covers landing, one per block). The
    951 ms block — the biggest attributed to covers yet — sat INSIDE
    `route_open:reader` (979.9 ms total; `book_open` 513.7 vs run
    #2's 80.4). How much of that open was the book and how much the
    storm cannot be separated without spans; reader opens stay out
    of scope either way, but the storm does not.
  - **A second, unattributed family appeared:** during PDF reading
    and after the last home_fill, 350–450 ms blocks whose anchors
    are 1.4–8.9 s stale — no task_item logged nearby (the watchdog's
    known limitation: it names the last thing it saw). Could be
    covers aftermath, could be something else. No claim until spans
    cover those paths.
  - Splash pump 948.5 ms with ~350 + ~250 ms blocks inside
    (pre-window, unchanged family); the comics-organize ~550 ms
    post-window block (unchanged from run #2).
  - **§60 recurred a fifth time**, new variant: a single-commit
    clone at the fork point (only `29788b4` in the local history,
    the whole branch state as working-tree changes). Caught by the
    log-first rule before any staging; the recorded recipe worked
    unchanged (fetch → mixed reset → checkout ci-logs + Cargo.lock →
    clean status). Pitfalls entry updated.
- **Reader fixes owner-verified; the covers hunt opened with its
  probe increment (2026-10-05, eleventh session): both reader
  reports confirmed fixed by the owner's own eyes ("the epub and pdf
  readers are fixed"), and the go given for the covers hunt.**
  - The reader-fixes chunk is closed: pdf blank-after-toggle and the
    epub top band both verified visually, the Back-path cache fix
    already log-verified in run #3 (two `page_cache_forced_miss`).
  - **Probes before the chase, per doctrine.** The plan's run-#1
    record said the covers family "needs spans on that path before
    anyone chases it" — so this increment adds exactly those spans,
    and the fix waits for the field run that reads them.
  - What the code reading established first: the landing path is
    already the cheap shape — `cache_decoded_cover` wraps ~100 KB of
    RGBA in a `MemoryTexture` and `swap_in_cover` swaps in place
    (§48: a repaint, not a relayout). So the synchronous cost of one
    landing should be ~1–3 ms, which cannot explain 250–951 ms
    blocks. The remaining suspects: (a) **burst-drain** — the 4 ms
    pacing paces the *producer*; the channel is unbounded and the
    `spawn_future_local` consumer loop drains any backlog the main
    thread's busyness created back-to-back, without yielding to the
    frame clock, bunching every swap (and their paints) into one
    dispatch; (b) the paint/texture-upload cycle the swaps schedule;
    (c) something outside covers entirely (the stale-anchor blocks).
  - **The probes** (all `KALAM_TIMING=1`-gated, zero cost otherwise):
    worker half — `covers_decoded` (count), `covers_decode` (total),
    `covers_decode_max` (worst single decode): the producer cadence.
    Consumer half — `covers_land` per landing (the full synchronous
    main-thread cost: texture wrap + cache insert + in-place swap),
    and `covers_gap` printed only when two landings were under 2 ms
    of idle apart (back-to-back): the burst detector. Read together:
    cheap landings + back-to-back gaps under a stall = the drain;
    expensive landings = the swap; slow decodes = the worker.
    Early exits (cancel, closed channel) now `break` instead of
    `return` so the aggregates always print; behavior is otherwise
    identical.
  - The next field run judges. The stale-anchor family (350–450 ms
    blocks with 1.4–8.9 s stale anchors during pdf reading) stays
    recorded as unknown — out of this chunk's scope by discipline;
    if it survives the covers fix, it gets its own probes.
- **Step 4 field run #4 (2026-10-05, owner's machine, the probe
  build, second run of it) — the probes named the culprit in one
  run, and it is not the landing: covers decode from FULL images
  because comic covers can never find their thumbnails. Fixed the
  same session. The owner's question "why is the startup lagging"
  has a two-part answer; the machine was also slow this run.**
  - **Probe verdict:** `covers_land` 0.0–0.2 ms for every single
    landing — the texture wrap + cache insert + in-place swap are
    exonerated, and so is the burst-drain theory as a stall cause
    (the first 12 covers DID land back-to-back, `covers_gap 0.0`,
    all twelve inside ~2 ms — harmless). The cost is the DECODE:
    `covers_decode 1806.8 ms` for 12 startup covers,
    `covers_decode_max 653.4`; the author page batch: 1284 ms for
    4, worst 1166.8. A 256×408 thumbnail decodes in single-digit
    milliseconds — these were full-cover decodes.
  - **Root cause, in the code:** a comic chapter's cover lives at
    `library/<Series>/covers/0010.jpg` (library-relative, item
    2.22), but `thumbnail_for_cover` derived the thumbnail key from
    the cover path's PARENT FOLDER — correct for a plain book's
    `library/<uuid>/cover.jpg`, and always wrong for a comic: it
    looked for `thumbs/covers.png`, a file that can never exist.
    The backfill had generated the right thumbnails all along (keyed
    by book uuid — the log's `thumbs_backfilled 15` with no
    `thumbs_backfill_done` means all 15 covered books had theirs);
    the lookups simply could not name them. Every comic cover
    decoded from the full scanned page, every session, on every
    card — and the same wrong key served the on-demand grid decode
    and the split bubbles.
  - **The fix:** thumbnails are now keyed by the cover's
    library-relative path, mirrored under the thumbs dir
    (`thumbs/<uuid>/cover.jpg.png`, `thumbs/<Series>/covers/
    0010.jpg.png`). Every caller shape can compute it from what it
    already holds — the preloader, the on-demand decode, the
    bubbles, the import, the cover-change regeneration, the
    backfill — no signature changes anywhere. The startup backfill
    migrates: it generates under the new key and removes the legacy
    uuid-keyed file once the new one exists (one-time); delete and
    cover-change clean both shapes. Two new probe counters verify
    the fix in the field: `covers_thumb` (decodes served by a
    thumbnail) and `covers_full` (fallbacks to the full image).
    Expect `covers_full` at (or near) zero and `covers_decode` to
    collapse from ~1800 ms to tens of ms.
  - **Tests:** the mapping itself (plain + series + outside-library
    + jpg/png non-collision), the migration (legacy file removed
    once replaced), the outside-library refusal, and the comic
    import end-to-end (thumbnail now found at the path-keyed
    location). One test rewrite worth recording: the backfill
    closure in tests must map the cover to a destination DISTINCT
    from the source — mapping it onto its own path made
    `thumb.is_file()` true because the source exists, a test that
    could not fail (§19, caught before commit, not by CI).
  - **The other half of the owner's question — the machine was slow
    this run:** spans that complete BEFORE any app work exists were
    several times slower than the same spans an hour earlier:
    `startup_db_open 857.9 ms` (run #3: 10.4), `startup_gtk_init
    340.0` (101.6), `startup_splash_show 417.1` (116.2),
    `startup_libraries 23.3` (0.3) — no app thread has started at
    those points, and no code on those paths changed between the
    builds. Meanwhile `startup_splash_pump` got FASTER (375 vs
    948). That signature (disk-path spans catastrophically slow,
    in-memory spans unchanged) says machine state — cold cache,
    busy disk, another program, throttling. The app-side decode CPU
    (1.8 s at startup, another ~1 s on the author page) burned on
    top of it, which is what "sluggish again" felt like. The fix
    removes that CPU; if `startup_db_open` is still in the hundreds
    on a warm next run, that becomes its own hunt with its own
    probes. Reader opens stay out of scope (this run's
    `route_open:reader` 1178.9 with `book_open` 877.9 — slower
    than run #3's 979.9/513.7, same machine-slow signature).
  - The 1251 ms block right after `settings_fill:appearance` (26.6
    ms) is the known post-construction GTK freeze family on a slow
    machine; the stale-anchor family persists unchanged; the splash
    blocks were smaller this run (~350 ms). §60 recurred a seventh
    time before this chunk (same variant, same recipe, zero
    damage).
- **Step 4 field run #5 (2026-10-05, owner's machine, the covers-fix
  build) — the fix is field-confirmed with the strongest numbers of
  the campaign, the machine theory from run #4 is confirmed too,
  and the covers hunt closes. What remains is one family: the paint
  cycle.**
  - **The fix, judged:** `covers_full 0` on every batch — not one
    fallback to a full image all session. Startup's 12 covers:
    `covers_decode 57.4 ms` total (was 1806.8 — a 31× drop),
    `covers_decode_max 5.5 ms` (was 653.4 — 119×). Every later
    batch 3.7–12.2 ms total, maxes 3.3–8.5 ms: thumbnail territory,
    exactly as designed. `covers_land` 0.0–0.1 ms, unchanged.
  - **The machine theory confirmed:** `startup_db_open 8.3 ms`
    (run #4: 857.9), `startup_gtk_init 100.3` (340),
    `startup_splash_show 117.1` (417) — the same pre-app spans that
    proved run #4's machine slowness are back at their run-#3
    levels, no code change on those paths. Run #4's sluggishness was
    machine state + the decode CPU; both are gone.
  - **Everything the campaign touched is now at its best field
    numbers:** `window_shown 1199.0 ms` (best yet; runs #1–4:
    1260–2745); home fill 3.6–4.6 ms; dashboard read 3.9 ms (run #4:
    65.7 — the storm was inflating it); shelves 0.8; book page
    rebuild 4.0; **the EPUB reader open collapsed to
    `route_open:reader 189.4 ms` / `book_open 126.4 ms`** (runs
    #1–4: 587–1179 / 126–878 — the decode storm was polluting
    reader opens all along, from *outside* the readers); pdf
    15.8/86.7/259.4. Readers stay out of scope, but the number is
    recorded.
  - **What remains, ranked from this log:** (1) the **paint-cycle
    family** — blocks of 250–550 ms around page builds and cover
    landings that the probes have now cleared of every app-side
    suspect: decodes are 3–8 ms, landings 0.1 ms, fills 3–6 ms, yet
    a 550 ms block still sits on the library grid while its covers
    land (fresh anchor) and a 450 ms block sits right after the
    first home build. What is left between those spans is GTK's own
    work: realizing, laying out, styling, and painting the freshly
    built trees, and uploading the swapped textures — the
    "post-construction freeze" family named in the 2026-10-02 run.
    Frame-level probes (first-frame cost per page build) are the
    next instrument if the owner points there. (2)
    **`author_fill 323.0 ms`** — the heaviest measured UI fill left,
    with its own 350 ms block. (3) A new, small, clear one: the
    remote comics browse decodes each downloaded cover ON the UI
    thread (`Texture::from_bytes` in the done-callback of
    "Loading comic cover", 410 ms download + a 450 ms block after
    its done) — the worker-decodes-main-wraps recipe (the
    preloader's own pattern) is the established fix. (4) The splash
    pump's ~350 ms blocks, unchanged, pre-window.
  - The two `page_cache_forced_miss` lines are present again (both
    reader closes), cache hits everywhere else. §60 recurred an
    eighth time before this chunk (same variant, same recipe, zero
    damage).


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
