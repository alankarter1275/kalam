# Kalam — Design Conversation Log

This file is a **living record of design discussions**. It is edited as ideas
are accepted, refined, or rejected. Rejected ideas are marked with `~~strike~~`
and a one-line reason, so we remember *why* — not just *what* we decided.

Status emoji: ✅ accepted · 🔶 under discussion · ~~struck~~ rejected.

---

## 1. The performance conversation

**Question asked (2026-09):** "The app is Rust only but not fast/snappy enough.
Should we rewrite parts in another language for speed, polish in Rust, or
something else?"

### Accepted: language is not the bottleneck — architecture is

**Verdict: do NOT rewrite in another language. Polish in Rust, but the polish
is architecture, not syntax.**

Why: Kalam is "Rust only" in our code, but the runtime doing almost all the
work is already C (GTK4), C++ (WebKitGTK), and C (SQLite). Rewriting our code
in C++/Zig/Go moves the same bytes across the same FFI boundaries into the
same libraries — months of work, a worse safety story, identical perceived
speed.

Reference implementations prove the point:

| App | Stack | Snappy? |
|---|---|---|
| Foliate | JS + GTK + WebKitGTK | Same profile as ours |
| Calibre | Python + Qt + Chromium | Heavier |
| KOReader | C++ + MuPDF + custom renderer | Genuinely snappy |

Lesson: the one snappy reader in the group is the one **without a browser
engine**. The engine, not the language, is the performance ceiling.

Our SQLite layer is already tuned exactly as we'd tune it: WAL,
`synchronous=NORMAL`, 64 MB cache, `temp_store=MEMORY`, 256 MB mmap,
`prepare_cached` everywhere, foreign keys on. The dict rebuild is guarded so
it doesn't run every startup. That layer would have been the tempting rewrite
target, and it's already right.

### Where the time actually goes

- A cold WebKitGTK process is 300–600 ms and 100+ MB before it paints.
- Reader does `webkit6::WebView::new()` + `load_html()` per chapter — a full
  browser engine parse + layout + rasterize on every book open and chapter
  change.
- Dictionary popup, annotations, find-in-chapter all cross the JS bridge
  into that engine.

### How we'd have built it (accepted approach)

Same core stack — Rust + GTK4 + WebKitGTK + SQLite — then attack the render
pipeline:

1. **One webview, created lazily.** Don't touch WebKit until the first book
   opens; prewarm the web process in the background while the user browses
   the library.
2. **Page by CSS multi-column, never re-layout on turn.** Load chapter once,
   let WebKit paginate via fixed-height columns; page turn = pure
   scroll/transform. Settings changes should be CSS-variable injection, not
   `load_html` again.
3. **Thin the bridge.** Every `postMessage` is an async hop across process
   boundaries. Batch where possible; dict popup should be pure DOM/CSS
   in-page — one Rust call per lookup. (Phase 10 already did the right
   version of this: don't log per-keystroke.)
4. **Keep chapters small.** `load_html` with a giant string = WebKit parsing
   + laying out a huge document on the main thread. Split or lazy-load
   sections for 100k+ word chapters.
5. **Profile for two days, fix the top three.** `perf` + sysprof + GTK
   inspector: cold start → window, book open, chapter turn, dict lookup.

### The one honest rewrite — and why even that is Rust

**~~Throw out the browser engine and render the book ourselves~~** — 🔶
**under discussion** (user: "we will talk about this in detail").

If we made the fastest possible ebook reader on Linux, the ceiling-move is
the KOReader play: a purpose-built text engine (skia/cosmic-text or pango)
where pagination, selection, and dict lookup are in-process operations
measured in microseconds instead of bridge round-trips. But that's a 1–3
person-year project reimplementing a slice of CSS paged media, and we'd lose
WebKit's accessibility, IME, and CSS completeness for free. **Even in that
extreme case we'd write it in Rust.** There is no scenario where the answer
is another language.

**To discuss in detail:** what we keep (anchoring, selection, dictionary
integration), what we lose (CSS, a11y, IME), what we'd write first.

---

## 2. Second opinion reviewed (another AI's analysis)

**Question:** another AI's answer claimed the bottleneck is the update model,
not Rust, with three specific costs:

1. `build_book_grid` at `src/widgets/book_row.rs:136` — 400 books = 400
   fully-realized card trees; GTK4 has `GtkGridView` + `GListModel` for
   ~30 widgets regardless of library size.
2. `rebuild_list` at `src/pages/all_books.rs:395` — teardown and rebuild the
   whole grid on every sort/filter/refresh.
3. Nothing off the main thread except network calls — every SQLite query and
   image decode runs on the GTK main loop; first paint = N synchronous JPEG
   decode-and-scale ops.
   Plus: cover cache is thread-local + in-memory (dies every launch, decode
   still synchronous on first paint); `style.rs` is 3,638 lines of CSS,
   `set_global_css` re-parses the whole sheet; `backdrop-filter: blur` on the
   dictionary popup.

**Verified against the code — all accurate.** `build_book_grid` really is at
book_row.rs:136, `rebuild_list` really does the remove-and-rebuild dance at
all_books.rs:395, the cover cache really is in-memory with a 400-entry bound,
the CSS really is 3,638 lines, `async-channel` really is in Cargo.toml. The
blur is 16px (not 22px) and is on the popup chip, not the whole popup —
minor, doesn't change the point.

### Where we agree

The problem is architecture, not language — same conclusion reached
independently.

### Where we push back

1. **"Single biggest cost" is asserted, not measured.** If "not snappy"
   means the library page lagging on scroll/search: yes, grid + decodes. If
   it means opening a book or turning pages: that's WebKit, and their list
   does nothing for it. 30 minutes of `perf` data before crowning a
   "single biggest cost."
2. **Their fix order puts the hardest thing first.** `GListModel` behind
   `GtkGridView` is the right end-state but real work. The cheaper fixes —
   **persist ~200px thumbnails at import time** (decode 30 tiny files instead
   of 400 big ones, cache survives restarts) and **async cover binding**
   (card shows placeholder, texture swaps in) — probably capture 80% of the
   perceived win at 20% of the effort. Order: thumbnails → async decode →
   measure again → virtualize only if the numbers say so.
3. **DB-off-main-thread nuance.** `Catalog` already wraps the connection in a
   `Mutex` — moving queries to a worker thread is feasible without rebuilding
   everything. But for a few thousand books most SQLite queries are
   sub-millisecond; the pain is image decode, LIKE-scans across the
   dictionary (no FTS index), and widget construction. The practical rule:
   "nothing *slow* on the UI thread" — decode is the slow one.
4. **Small nits.** Split-CSS is lower value than implied — re-parsing 3,600
   lines on a theme switch is a one-time few-ms cost; per-widget style
   resolution shrinks when the widget count shrinks. Fix the widget count and
   the CSS problem mostly fixes itself.

### What we'd actually do (accepted plan)

1. **Measure for a day.** `perf`, sysprof, GTK inspector → numbered list:
   cold start, book open, chapter turn, dict lookup, library scroll.
2. **Thumbnails at import + async cover decode.** Big, simple, survives
   restarts.
3. **Virtualize the grid if the numbers earn it.**
4. **The reader** — the big lever; the "throw out the browser engine"
   conversation plugs in here.

---

## 3. The Yazi philosophy conversation

**Question:** "Let's make Kalam with the same philosophy as Yazi file manager —
blazing fast, async tasks, clean architecture."

### What Yazi actually is

Four ideas:

1. **The UI never does work.** Render layer draws and sends commands; copying,
   renaming, previewing, filtering are all requests answered asynchronously.
2. **A task system.** Heavy operations become background tasks with progress,
   cancellation, queue. UI shows progress bars, never blocks.
3. **Preloading.** Hover a file → preview loads before you need it. It feels
   instant because it's *early*, not fast.
4. **Clean layering.** `yazi-core` is pure logic with no UI; UI is a thin
   shell. Testable, stays clean.

Philosophy in one sentence: **"don't make the UI fast — make it never wait."**

### The good news: Kalam already has the skeleton

- **relm4 is already an actor framework.** `AppMsg`, `ComponentSender`,
  `PageSlot` = Yazi's event bus, type-safe, GTK-integrated. Yazi built its
  own event system because it had to; we already have one.
- **`async-channel` is already in Cargo.toml** — the same hand-off pattern
  Yazi uses, already proven in the metadata fetch.
- **`src/db/` is already separate from the UI** (headless tests) — that's our
  `yazi-core`; it just needs a *service layer* on top.

So: not adopting Yazi's architecture from scratch — finishing the job already
started.

### What Yazi's shape looks like in Kalam

| Yazi | Kalam equivalent | Status |
|---|---|---|
| `yazi-core` (pure logic, no UI) | `src/db/` | ✅ done |
| Event bus | relm4 components | ✅ done |
| Task system (progress + cancel) | **`src/tasks.rs` TaskManager** | ❌ missing |
| Preloaders (preview cache) | **cover preloader + chapter preloader** | ❌ missing |
| Thin UI that only renders state | **pages still do their own DB calls** | ❌ missing |

The missing pieces:

1. **A service layer.** One `LibraryService` owns the `Catalog` and exposes
   requests: "books matching filter", "load cover". Pages stop calling the DB
   directly and start *asking*. Moving queries off the UI thread becomes a
   change in one place, not in every page. Most Yazi-like thing we can do;
   makes everything after it easy.
2. **A task manager.** Import, dictionary rebuild, metadata fetch, EPUB
   conversion → tasks with progress + cancellation. Metadata fetch already
   half-does this; extend to everything slow.
3. **Preloaders.** Cover preloader: rows 1–30 visible → worker decodes
   31–60 in background; scroll down and everything's already there.
   Reader: preload the *next chapter* while reading the current one —
   kills most of "book open feels slow" even before the renderer
   conversation.
4. **Thin pages.** After 1–3, pages become what Yazi's UI is: render the
   state given, send commands on click. No DB calls, no decoding, no loops
   that build 400 widgets.

### What does NOT translate

- **~~Tokio~~** — rejected. Yazi runs on tokio because a TUI has one tiny
  render loop; GTK has its own main loop and its own async. We don't need
  tokio; we need `thread::spawn` + `async-channel` + `glib::idle_add` back to
  the UI — the pattern we already have. Adding tokio to a GTK app is a cost,
  not a feature.
- **~~The plugin system~~** — rejected, reversed, narrowed, then **settled
  2026-09-03: Lua for the surfaces that rot, Rust for the rest.** Yazi's Lua
  plugins work because a file manager's verbs are tiny; an ebook reader's
  verbs are huge. That objection was half right: it holds for the *reader*,
  not for *sources*, whose verbs are exactly four. So extensibility only
  where the verbs are small — and within that, **Lua where the code breaks
  when a website changes** (scrapers), **compiled Rust where it does not**
  (documented APIs, themes, export). See §5 and
  [`source-seam.md`](./archive/source-seam.md) §9a.
- **The "blazing fast" bar itself.** A TUI renders text in microseconds and
  reads a directory listing. We render books. Even with perfect architecture,
  WebKitGTK dominates. That's why the renderer conversation is the other
  half: **Yazi's architecture makes the app feel instant around the engine;
  the custom renderer replaces the engine. Do both, architecture first.**

### What we'd add, beyond what was asked

1. **Startup prewarm.** Kalam will never start in milliseconds (GTK+WebKit),
   but it can *look* like it: don't build the WebView until the first book
   opens, prewarm in background while browsing, warm the dictionary at idle.
2. **A perf budget as a habit.** CI runs 156 tests every push — add a rule
   like "library with 2,000 seeded books must build its grid in under N ms"
   as a test. It won't be precise, but it catches regressions — the day
   someone adds a `for book in books` loop again, the test screams.
3. **Live config reload.** Themes live in `theme.rs`; a "reload config" that
   re-applies without restart is small and makes the app feel alive.
   (Caveat: `set_global_css` re-parses 3,600 lines; a few ms, fine for a
   manual action.)
4. **The testability dividend.** Headless db tests already caught real bugs
   (the `quote_ident` escape, the Phase 10 FK violation). A clean service
   layer makes more of the app testable the same way.

### Where we'd start on Monday

1. `LibraryService` behind the existing `Catalog` — pages ask, service
   answers. (Weeks, mostly mechanical.)
2. Cover preloader + thumbnail persistence — fastest visible win.
3. Task manager for import/rebuild/metadata.
4. Grid virtualization — now *easy*, because the service layer feeds it.
5. Then the renderer conversation.

**Closing thought:** Yazi's real secret isn't speed — it's that **the
architecture makes speed inevitable**. Every decision pushes work away from
the render path. That fits Kalam perfectly, and unlike the language question,
it requires throwing nothing away.

---

## 4. Scope conversation — the roadmap's future phases

**Question:** "go through the Roadmap once and then we will discuss."

### What's currently in the roadmap

- **P6 — Downloads hub:** unified queue (queued/active/done/failed),
  sidebar Downloads UI, folder-watch import, hooks for AO3/comics jobs.
- **P7 — Fiction sources (AO3 first):** `FictionSource` trait, AO3
  search/detail/download EPUB into library, `source` + `remote_id`, manual
  "Check updates", rate limits / clear errors, open in text reader.
  Out: piracy sources.
- **P8 — Comics local + Moku-style reader:** import CBZ/CBR into same
  catalog, black immersive stage, top bar (close/title/page/zoom), bottom
  scrubber, page LTR/RTL + webtoon long-strip, fit width/height, tap-center
  toggle chrome, memory-safe decode (viewport ± neighbors only), progress per
  book, Read routes by format. Out: remote catalogues.
- **P9 — Comics sources:** source framework, self-hosted/legitimate backends
  first (OPDS, Komga, Kavita, own archive), Downloads hub integration,
  Suwayomi-like module depth only as needed.
- **P10 — PDF (text-reader family):** MuPDF (or Poppler) in the text-reader
  chrome family (not comics shell), continuous or page mode, basic
  highlight/underline stored like EPUB annotations, same library entry model.
- **P11 — Tools:** convert via external `ebook-convert`/`pandoc` if present,
  EPUB polish, batch metadata/cover refresh, optional Calibre `metadata.db`
  one-shot import.

### Non-goals currently locked

- ~~Z-Library / unauthorized shadow libraries~~ (still a hard non-goal — this
  is legal/safety, not a phase)
- Calibre multi-app suite, content server, fetch news (still non-goal)
- **~~Plugin API~~** — **being reversed, see section 5**
- Windows / macOS (still non-goal)

### What's worth a closer look (open discussion points)

1. **The "two readers" lock.** The roadmap locks a separate WebKit text
   reader and a separate image-based comics reader. That's fine, but the
   architecture work (service layer, task manager) applies to both — worth
   deciding whether the custom renderer conversation changes the comics
   reader plan too (decode-only is simpler than a text engine).
2. **P7 dependency on P6.** "Downloads hub is a prerequisite for P7" — fair,
   but the folder-watch import could ship standalone; the queue is the
   dependency, not the watch.
3. **P10 PDF via MuPDF/Poppler** — this is a *third* rendering engine. With
   the renderer conversation pending, PDF is the one place a purpose-built
   renderer (not WebKit) is already the plan.
4. **P11 Tools scope is healthy** — external tools if present, no bundling.
   Keep it that way.
5. **The performance/architecture track is not yet in the roadmap as a
   phase.** It's discussed above but not scheduled. Needs a home — probably
   an "architecture track" alongside P6–P11.
6. **"Check updates" for fanfic (P7)** — the roadmap says manual only. With a
   task manager, scheduled updates become cheap. Worth revisiting.
7. **Annotations in comics (P8) and PDF (P10)** — roadmap says "basic marks"
   for PDF, nothing for comics. Worth deciding scope early so the service
   layer supports it.

---

## 5. Plugin system — settled (after two wrong turns)

**Status: ✅ seam designed 2026-09-03; Lua removed, then restored the same day
on the user's Calibre argument. Final answer: **Lua for the surfaces that rot,
compiled Rust for the ones that do not** — [`source-seam.md`](./archive/source-seam.md)
§0, §9a, §12a.**

The user settled the audience question, which is what the whole thing turned
on: *"sources and metadata and maybe a few more, not an ecosystem, because
it's for personal use. plugin system makes sense if there is a community,
which isn't the case here."*

- **Q1 (Lua vs alternatives) → Lua stays, for the surfaces that rot.** My
  first answer was "compiled-in Rust everywhere", reasoning that a scripting
  runtime exists so non-compiling users can extend an app and both authors
  here compile routinely. **That was wrong, and the user's counter settled
  it** (2026-09-03): *"there are many, many plugins in Calibre just for
  metadata sources. I'd say, Open Library and Google Books should be built
  in, but we can have option to add more sources later with Lua."* Calibre's
  index carries 20+ third-party metadata-source plugins — Goodreads, Amazon,
  Kobo, StoryGraph, ISFDB, Douban, DNB, moly.hu, databazeknih.cz, Skoob,
  Kitapyurdu — heavily regional and niche, and almost all **scrapers**. The
  mistake was conflating *audience size* with *iteration speed*: a runtime is
  not only for strangers, it is for anyone who has to fix a parser often.
  Sharpened by this repo's `[profile.release]` (`lto = true`,
  `codegen-units = 1`, 44k lines) — a one-character selector change re-links
  the entire binary, with OOM risk on a 4 GB box. **The real line is "does
  this break when someone else changes their website"**, not "is it a
  source".
- **Q2 (scope) → a short list, split by whether it rots.** Content sources
  (missing, = A0 step 8) → **Lua** when scraped, Rust when API-backed.
  Metadata providers → **two built-in Rust** (Open Library, Google Books,
  both shipping) **plus a Lua long tail**; I first wrote this one off as
  "already extensible, done", which the Calibre evidence disproves — 20+
  third-party metadata plugins exist there precisely because the tail is
  regional and endless. Themes (`Theme`, shipping) and possibly export
  formats and dictionaries stay Rust: pure data, nothing to rot. UI
  extension, reader/renderer hooks and anything that writes to the library
  are out.
- **Q2a (no ecosystem) → still true, and unrelated.** No marketplace, no
  third-party repo, no API-stability promises. That is about *other people*;
  Lua is about *fix speed*. Conflating the two is what produced the wrong
  answer above.
- **Q3 (when) → answered.** The architecture track is done, so the seam was
  safe to design now.

The original objection at the top of this section — that an ebook reader's
verbs are too big for a plugin system — turns out to have been half right. It
is true of the reader; it is false of *sources*, whose verbs are exactly four.
The resolution is not "no extensibility", it is "extensibility only where the
verbs are small", which is what §12a lists.

The roadmap listed **Plugin API** as a non-goal. The user changed their mind
— "it will help in the future phases" — and the final shape, after the detour
recorded above, is: **a Lua plugin host for scrapers, built-in Rust for stable
APIs, sequenced after AO3 proves the trait natively.**

### Initial rejection (context)

Yazi's Lua plugins work because a file manager's verbs are tiny ("copy",
"rename", "filter"). An ebook reader's verbs are huge ("render this chapter",
"annotate this selection"). Plugin systems are also a massive compatibility
surface — they become a second API you must keep stable forever.

### Why we're now considering it

- Future phases (P6–P11) are source adapters, import/export, and tools —
  exactly the kind of small-verb surface that plugins suit well (a
  `FictionSource` plugin, a metadata-source plugin, an export plugin).
- The architecture track (service layer) is the natural plugin host: if all
  I/O goes through a service, a plugin boundary is a thin seam, not a
  rewrite.
- A clean core is the prerequisite for plugins — plugins become *possible*
  later without being built now.

### Open questions (to discuss)

1. **Lua vs alternatives?**
   - **Lua (mlua)** — the default choice; tiny, embeddable, used by Yazi,
     Neovim, AwesomeWM. Good for config + scripting. Risk: a DSL-shaped
     surface that grows. **Finding (2026-09-03):** `mlua` is `!Send` by
     default — the VM holds a raw `*mut lua_State`. Its `send` feature exists
     but works by putting a reentrant mutex around every VM access, a
     permanent cost. Since `tasks::spawn` requires `Send`, the resolution is
     to send a `SourceFactory` (a path + manifest, trivially `Send`) to the
     worker and build the VM *there*, so it is born and dies on one thread.
     **The `send` feature should not be enabled** — see `source-seam.md` §9,
     recorded because "just turn the feature on" is the obvious wrong answer.
   - **WebAssembly (wasmtime)** — sandboxed, language-agnostic (plugins in
     Rust/C/Go), fast, but more tooling complexity and a steeper authoring
     curve.
   - **Rust dyn traits compiled in** — simplest, no runtime, but plugins
     must be compiled with the app (no user-authored scripts).
   - **A tiny JSON/config "plugin" surface first** — not a script runtime at
     all; define the *seams* (source adapters, export formats) as stable
     APIs now, add scripting later. **This is the author's lean** — the
     service layer IS the plugin API; scripts can come later.
2. **Scope:** config scripts? source adapters? export/import formats? UI
   extensions? All of the above?
3. **When:** after the architecture track (service layer + task manager),
   not before. Plugins are a consumer of clean seams, not a way to create
   them.

---

## 6. Standing decisions & open threads

- **Language:** stay Rust. No rewrite in another language. (✅ locked)
- **Stack:** Rust + GTK4 + Relm4 + SQLite + WebKitGTK (EPUB) + image pipeline
  (comics) + MuPDF later (PDF). (✅ locked)
- **Architecture:** Yazi-style service layer + task manager + preloaders +
  thin UI. (✅ accepted, not yet implemented)
- **Custom text renderer:** 🔶 under discussion → **endgame: custom renderer
  for ALL reflowable text** (cosmic-text; fiction first, EPUB after
  normalization); WebKit = fallback only, may be cut (see §8).
- **Plugin system:** ✅ **settled 2026-09-03 — Lua for scrapers, Rust for
  stable APIs.** The `Source` adapter is the engine for fiction + manga (see
  §7). Built in: Open Library, Google Books, MangaDex, themes, export
  formats. Lua: AO3, FFN, scraped manga, and add-on metadata providers —
  Calibre's model, and the surfaces that break. Sequenced after AO3 lands
  natively so the API is extracted, not guessed (§5,
  [`source-seam.md`](./archive/source-seam.md) §9a, §11).
- **Scope:** confirmed as a **content platform** — fiction sources
  (AO3/FFN/webnovel, tag search, downloads, auto-updates) + manga sources
  (Suwayomi-class) + a Lua plugin seam for scrapers + fast architecture (§7).
- **Perf work order:** measure → thumbnails/async decode → virtualize if
  numbers say so → reader. (✅ accepted)
- **Docs discipline:** keep README.md and ROADMAP.md updated as work
  progresses; this file is the design-conversation record. (✅ standing)

---

## 7. The content-platform scope + renderer decision

**User (2026-09-02):** "Do you understand the scope now?" — restated:
Kalam is not just a local reader; it is a **content platform**: fiction
sources (AO3, FanFiction.net, Webnovel, Royal Road, …) with rich tag
search, download, offline reading, and **automatic updates** as new
chapters release — FanFiction.net-app-class features — plus **manga
sources** with a Suwayomi-class browse/read experience. Plugins power the
sources.

### Accepted: the scope

- **Fiction:** search (tags, fandom, characters, ships, rating, status) →
  results → read or download → follow → auto-update + notify. Sources
  mostly have no public APIs (AO3 especially) — source plugins parse the
  site and return structured data (Tachiyomi pattern).
- **Manga:** same shape, but content is images — the reader is an image
  pager (P8), not a text engine.
- **Extension surfaces** are the source-adapter engine (P12), built on the
  A0 service layer. **Lua plugins for scraped sources and add-on metadata
  providers; compiled-in Rust for API-backed ones** — settled 2026-09-03
  (§5).

### ~~Rewrite the Suwayomi server in Rust~~ — rejected

Suwayomi = Tachiyomi's engine as a server. Its value is the **Kotlin
extension ecosystem**, which cannot run in Rust. We do not need the
server — Kalam already has (or will have) the DB, download queue (P6),
task manager (A0), and readers. What we need is the **adapter concept**:
a `Source` plugin API (search / popular / chapter list / fetch content).
Porting an existing extension's *logic* is hours (they are simple
scrapers); running its Kotlin is impossible. Optional later: a "Suwayomi
server" adapter that talks to a user's running instance via its API — the
cheapest bridge to the whole ecosystem.

### WebKit vs custom renderer — the framework (under discussion)

**What WebKit is:** a full web-browser engine (Safari's). Reading a
chapter today = running a web page: HTML parsing, CSS layout, JS, fonts,
accessibility tree. Power: displays ANY web content perfectly. Cost:
300–600 ms cold start, 100–200 MB RAM, bridge tax (Rust ↔ JS ↔ DOM) for
every feature (dict popup, highlights), and black-box internals we can
only poke with CSS/JS.

**What a custom renderer is:** we draw the text ourselves (like a PDF
reader / KOReader / e-ink reader). We define a clean content format
(paragraphs, headings, images), lay it out with a Rust text engine
(cosmic-text / skia), paginate, and paint. Cost: 1–3 person-years for a
good engine (line breaking, hyphenation, justification, RTL, CJK, font
fallback, selection, **accessibility**). Gain: page turns in ~1 ms, tens
of MB, and every feature (dict lookup, themes, annotations) is native GTK
beside the text — no bridge.

**The unlock:** we control what reaches the renderer. Sources fetch →
**sanitize → convert to clean chapters** (FanFicFare's whole job; AO3
even ships official EPUBs). Manga is images only. So the renderer never
has to be a browser — it only ever sees clean content. Industry proof:
Tachiyomi reads images, FanFicFare converts to EPUB, KOReader renders its
own format.

**Leaning — hybrid, resolved by §8 (no browse mode):**
- Custom renderer for **reading** fiction (clean format) and comics
  (image pager) — the fast, light, integrated reader.
- WebKit stays as the **EPUB engine** (and fallback for exotic EPUBs we
  haven't normalized) — created lazily when an EPUB opens.
- ~~Browse mode~~ — rejected: Kalam never renders arbitrary websites;
  sources return structured data via plugins.
- Sequencing: sources + architecture first (the product), the custom
  renderer as the A0 crown afterward.

### Honest caveats

- **Auto-updates vs annotations:** re-downloading a fic can shift its
  spine; existing annotation anchors may reset (already in the risk
  register). Best-effort.
- **Polling etiquette:** per-source rate limits and user-controlled
  schedules (daily, not per-minute).
- **A11y:** a custom text engine must expose text to screen readers
  (AT-SPI); WebKit gives this free. Budget for it.

---

## 8. Scope sharpened: no browse mode + the Tachiyomi/Suwayomi picture + borrow list

**User (2026-09-02):** "I am not making a web browser right? I just want it
for Epub, and then AO3, fanfiction, etc."

### Accepted: no web-browse mode

- Kalam never renders arbitrary websites. Sources return **structured data**
  (title, author, tags, chapters) via plugins; the search UI is native.
- This **resolves the renderer fork**: WebKit's only remaining job is
  **EPUB rendering** (EPUBs are HTML/CSS internally — the one place web
  content is unavoidable today).

### Renderer decision, sharpened (user pushed back — WebKit days numbered)

**User (2026-09-02):** "WebKit stays as the EPUB engine only — isn't that
the same as now? shouldn't we shift to something better? crengine (C++) or
cosmic-text (Rust)?"

**Conceded: yes, EPUB-on-WebKit is the same cost today.** Lazy creation +
fiction moving off WebKit only helps startup/new-content; EPUB reading
itself stays on WebKit until replaced.

**Sharpened position (accepted):**

- **Custom renderer is the endgame for ALL reflowable text** (fiction AND
  EPUB) — not just fiction. WebKit is a temporary fallback, not a
  foundation, and may be cut entirely later.
- **cosmic-text is NOT an EPUB renderer** — it's a Rust text-layout
  library (System76, MIT). We build parsing/normalization/pagination/
  painting around it. **crengine** is a complete C++ EPUB engine (GPL) —
  a ready-made meal via FFI, but less control over dict-popup/theme/
  annotation integration, and C++ in the stack.
- **Lean: cosmic-text.** The hard part either way is EPUB→clean-content
  normalization (lol_html + rules); that work is ours regardless. Fight
  our own code, not a foreign engine's API. crengine stays a legitimate
  shortcut if we want EPUB rendering before the custom engine matures.
- **WebKit's only honest role: fallback for exotic EPUBs** the normalizer
  can't handle (compatibility mode). Optional — could be cut if we accept
  imperfect rendering of rare weird EPUBs.

**Sequencing (accepted):**

1. Custom renderer v1 on cosmic-text for **fiction first** — content is
   ours (clean plugin output), bounded, proves the engine.
2. **EPUB normalization pipeline** (lol_html + rules) → same renderer.
   WebKit drops to fallback.
3. PDF = MuPDF (P10); comics = image pager (P8) — never in this question.

### Manga architecture confirmed (Tachiyomi shape, no Kotlin)

> **Note (2026-09-03):** "Lua plugins" below is correct, after a detour. It
> was briefly changed to "compiled-in Rust modules" and then changed back. The
> *shape* — one adapter per site, written by us, Tachiyomi-like — was never in
> doubt. The final split: **scraped sites are Lua, API-backed sites
> (MangaDex, Komga, Kavita, OPDS) are built-in Rust.** See
> [`source-seam.md`](./archive/source-seam.md) §9a.

**User (2026-09-02):** "We could have a similar architecture… plugins which
we will write. Also, no need for a bridge with the Kotlin extensions —
those are apk… too much work, maybe even impossible."

**Agreed, fully:**

- Manga reader uses the same `Source` adapter shape: `search / popular /
  chapter list / pages`. Adapters are **written by us**, one per site —
  **Lua** for scraped sites, **Rust** for API-backed ones (see the note
  above).
- **No Kotlin extension bridge — confirmed.** Tachiyomi extensions are
  Android APKs calling Android APIs; running them needs an Android runtime
  or JVM emulation — fundamentally wrong shape for a desktop app. We lose
  nothing: MangaDex has an official API (zero scraping), Komga/Kavita/OPDS
  are clean REST, and scraping logic ports from the Apache-2.0 extensions
  (license-compatible).
- **Fiction uses the same architecture** — one plugin system, text flavor
  + image flavor.

### Engine map (final)

| Content | Engine | Notes |
|---|---|---|
| Source fiction | Custom renderer (cosmic-text) | Clean content we define — build first |
| EPUB | Custom renderer + normalizer; WebKit fallback | Normalization is the hard part |
| PDF | **MuPDF** (P10) | Fixed-layout; never WebKit; AGPL (or Poppler/GPL) |
| Comics | Image decode (gdk-pixbuf) + GTK pager | No engine at all — decode and paint; already P8 |

### The complete Tachiyomi / Suwayomi picture

- **Tachiyomi** = Android manga reader. App knows ONE `Source` interface
  (search / popular / details / chapter list / page list). **Extensions**
  = small Kotlin APKs, 200–500 lines each, that implement that interface
  for one site (MangaDex = official JSON API; Komga = REST; random site =
  HTML scrape with Jsoup). Hundreds of sites, one tiny adapter surface.
- **Suwayomi** = Tachiyomi's engine extracted into a JVM **server**; loads
  the same Kotlin extensions, exposes REST + GraphQL; clients are thin.
- **The Kotlin obstacle:** Kotlin compiles to JVM bytecode; a Rust app
  cannot execute it (no JVM inside). The *concepts* are trivial; only the
  runtime is incompatible.
- **Three workarounds:** (1) ship a JVM subprocess — heavy, rejected;
  (2) bridge to a user's existing Suwayomi instance via its API — cheap
  optional plugin later; (3) **reimplement the adapter pattern natively**
  — recommended. The scraping logic is "fetch URL, parse HTML/JSON, return
  fields" — hours per source, and MangaDex/Komga/Kavita/OPDS need
  **no scraping at all** (official APIs).
- **Fiction side** needs no server concept: same adapter shape, plus
  FanFicFare (below).

### ~~Rewrite the Suwayomi server in Rust~~ — rejected (confirmed)

Nothing to rewrite: we want the adapter *concept*, not the server.
Kalam already has (or will have) the DB, downloads (P6), task manager
(A0), and readers.

### PDF — NOT forgotten (P10)

PDFs are fixed-layout; they were never a WebKit question. P10 uses
**MuPDF** (purpose-built renderer, already on the custom side of the
fence). License note: MuPDF is **AGPL-3.0**; combining with our GPL-3.0
app pulls the app to AGPL (fine for personal open source — KOReader does
it; Poppler/GPL is the alternative if we ever want to avoid AGPL).

### Open-source borrow list (repo is GPL-3.0-or-later — compatible)

| Project | License | What to take |
|---|---|---|
| **FanFicFare** | GPL-3, Python | **P7 already built**: site adapters for AO3, FFN, Royal Road, ScribbleHub, SpaceBattles, Wattpad… port adapter logic to Lua source plugins (AO3 native first as the reference), or shell out to its CLI as a "FanFicFare source" |
| **Tachiyomi extensions** | Apache-2.0 | The adapter pattern + per-site logic to port to Lua plugins |
| **MangaDex API** | public API | First P9 source — zero scraping |
| **Komga / Kavita** | GPL | Self-hosted manga servers; Kalam as a client via their REST APIs |
| **Suwayomi** | MPL-2.0 | Mirror its extension-API shape |
| **KOReader** | AGPL-3.0 | Proof of no-browser reading; its Lua plugin architecture is worth **studying** — same language, same embedding problem (`source-seam.md` §9a) |
| **crengine** | GPL-family | Ready-made EPUB/HTML rendering engine (Path B) — bind via FFI |
| **cosmic-text** | MIT | Rust text layout/shaping (Path B, if we build our own) |
| **swash / fontdb / ab_glyph** | MIT/Apache | Rust font loading/shaping |
| **vello / skia-safe** | Apache/MIT | 2D/GPU painting for custom engine |
| **lol_html** | Apache/MIT (Cloudflare) | HTML parsing/sanitizing for scrapers + EPUB normalization |
| **ammonia** | MIT | HTML sanitizer for plugin output |
| **chapbook** | Apache-2.0 | **Candidate foundation for the custom renderer** — stylo + cosmic-text + tiny-skia/vello, no webview, GTK4 viewer, LayeredLocator, pagination-first (§11) |
| **hayro** | Apache-2.0 | Pure-Rust PDF rasterizer — AGPL-free alternative to MuPDF for P10 (§11) |
| **Yazi** | MIT | Task system + preloader architecture (already discussed) |
| **Foliate** | GPL | GTK+WebKit reference; CSS pagination tricks |

---

## 15. Timestamps stay UTC; only bucketing goes local (2026-09-04)

Came out of the code review in [`review-2026-09-04.md`](./archive/review-2026-09-04.md),
finding 5. Every reading statistic — the 14-day chart, "days active", the
reading streak, "finished this year", the per-book day breakdown — bucketed by
`substr(started_at, 1, 10)`, and `started_at` is stored as ISO-8601 **UTC**.

**The bug is not cosmetic and it is worst for the heaviest users.** At UTC+05:30
every session between local midnight and 05:30 is filed under the *previous*
day. Reading at 1 a.m. on consecutive nights produces a chart with holes in it
and, far more annoyingly, **breaks a streak the user genuinely earned** — the
one number in the app whose entire value is that it is not wrong. Anywhere west
of UTC the error runs the other way: late-evening reading is credited to
tomorrow.

Three options were considered.

1. **Store local time instead.** Rejected outright. Timestamps that carry no
   zone and are not UTC are unorderable across a move or a DST change, and the
   damage is permanent and silent — a row written before the change cannot be
   distinguished from one written after.
2. **Add `chrono`/`time` and do it properly.** The correct answer in general.
   Rejected *here* because the dependency exists solely to answer "what is the
   UTC offset right now", which the process already has an answer for.
3. **Ask SQLite.** Adopted. It is already linked, already reads the OS
   timezone, and `strftime('%s','now','localtime') - strftime('%s','now')`
   yields the current offset in seconds.

**The decisions inside option 3, which are the part worth recording:**

*The offset is cached in a `OnceLock`.* It is read once, from a scratch
in-memory connection, and never re-read. This means **a DST transition while
the app is running is wrong until the next restart** — for at most one evening,
in the two regions-worth of the year that observe one, on a chart. Accepted
deliberately: the alternative is carrying a timezone database to be right about
a boundary the user crosses twice a year while an ebook reader happens to be
open. If it ever bites, the fix is to invalidate the `OnceLock` on resume, not
to add a dependency.

*The SQL embeds a literal `'{offset:+} seconds'` modifier rather than using
`datetime(col, 'localtime')`.* This is the more important of the two. Both
would work, but they would be **two independent sources of truth for where the
day starts** — SQLite's own conversion for the bucketing, and Rust's arithmetic
for the labels the zero-fill loop matches against. Any disagreement between
them shows up as a chart bar that is silently always empty, because the label
never matches a bucket. Sharing one number makes that class of bug
unrepresentable. The cost is that these statements cannot use `prepare_cached`
(the SQL text now varies), which is noted at each call site.

*`added_by_month` stays UTC.* Month boundaries are not what anyone is looking
at on that chart, and leaving it alone keeps the diff to the queries where the
bug is observable.

*The tests inject the clock and the offset.* A test that simply calls the real
functions would pass on the broken code in a UTC CI container and fail on the
developer's machine in IST — [`pitfalls.md`](./pitfalls.md) §19, the rule that
a check which cannot distinguish the fixed build from the broken one is not a
test. Private `*_at(days, now, offset)` cores take both as parameters, so the
suite asserts IST and EST behaviour explicitly and is identical everywhere.

---

## 16. A0 step 6 (grid virtualization) reopened — the evidence that closed it was measured on the wrong page (2026-09-04)

Step 6 was closed on 2026-09-03 with an unusually confident entry: peak memory
is **flat** in library size, 233 MB at 139 books versus 252 MB at 2,000, so the
grid does not scale badly and virtualization fixes nothing. It was a good
decision made from the numbers available.

The numbers were wrong. Not mismeasured — **measured on a page that was never
open.** The CI screenshot harness navigated by sending `Tab Tab Return` and
had never once succeeded; every run photographed Home and sampled Home's
memory. `grid_build` appears in none of those reports, which the roadmap
itself noted as a curiosity without drawing the obvious conclusion: if the
grid never built, the memory figure cannot describe the grid.

Fixing navigation (`KALAM_ROUTE`, see pitfall §21) changed the measurement:

| Books | Grid reached? | Peak RSS |
|---|---|---|
| 139 | no | 231 MB |
| 2,000 | no | 226 MB |
| 139 | **yes** | 255 MB |
| 2,000 | **yes** | **502 MB** |

Memory roughly doubles with library size. +276 MB for 1,861 additional cards
is about **0.15 MB per card**, for cards that are overwhelmingly off screen.

**What this is not.** It is not the cover cache. That is a bounded 300-entry
LRU, it works, and the earlier analysis of it was correct — which is exactly
why the flat-memory reading was so persuasive: there *was* a real bound doing
real work, and it made an unbounded cost next to it invisible. The unbounded
cost is the widgets. `build_book_grid` loops over every book, constructs a
card, wraps it in a sizing cell and attaches it to the `GtkGrid` before
returning. A 2,000-book library therefore holds 2,000 live widget trees.

That is the textbook case for virtualization, and the sole reason it was ruled
out has evaporated. Step 6 is **reopened**, and `grid_build 421 ms` at 2,000
books is now a measurement rather than the extrapolation the estimates kept
being revised against.

**Not fixed in this pass, deliberately.** Replacing `GtkGrid` with a recycling
view is a real change to the most-used screen in the app, the user is the only
visual QA, and the correct next move is to agree the approach first. Recorded
now so the decision is made against true numbers.

**The lesson worth keeping.** Three separate roadmap entries revised the step-6
estimate, each more confident than the last, and all three were reasoning about
output from a harness that was not exercising the code under discussion. The
tell was in every report — no `grid_build` line — and was read as noise. When a
measurement settles an architectural question, check that the thing being
measured actually ran.

---

*Last updated: 2026-09-04.*

---

## 9. Renderer effort estimate (2026-09-02)

**User asked:** "for the custom renderer, you have to do all the work. how
much time do you reckon it will take you?"

**Honest answer:**

- Writing code is the fast part; the constraint is that the agent has **no
  display** — the renderer is visual, so the **user is the QA loop** (Arch
  machine GUI smoke tests). The user's testing cadence sets the schedule.
- Layout/pagination/hit-testing are pure functions → unit-testable headless
  (agent-strength); visual polish needs user eyes (agent-weak).

| Milestone | Scope | Estimate (sessions) |
|---|---|---|
| 1. Vertical slice | one clean fiction chapter: content format → cosmic-text → pagination → painted GTK pages → page turn → themes → dict popup hook → position save/restore | **2–4 weeks** |
| 2. Fiction reader complete | all plugin-produced formats, images, footnotes, 100k+ word chapters, selection/copy, perf at scale | +1–2 months |
| 3. EPUB normalization | EPUB HTML/CSS → clean format (lol_html + rules); WebKit fallback when normalization fails; position mapping | +2–4 months |
| 4. Hardening | RTL, CJK, hyphenation, justification, a11y (AT-SPI), incremental layout, edge cases | +2–4 months, ongoing |
| **Total** | "WebKit gone for reflowable text" | **6–12 months** |

- Benchmark: a strong human dev solo ≈ same (6–14 months) — renderer is a
  1–3 person-year thing for anyone.
- **Faster:** user tests in small batches (15 min per slice). **Slower:**
  big-bang testing → visual bugs in bulk, blind debugging.
- **Key risk:** custom engine choking on one weird EPUB feels like a
  regression → WebKit stays as fallback until milestone 3 earns trust.
- **Milestone 1 is the calibration test:** if the slice takes > ~4 weeks,
  we learn the renderer is harder than estimated before sinking months in.

---

## 15. Timestamps stay UTC; only bucketing goes local (2026-09-04)

Came out of the code review in [`review-2026-09-04.md`](./archive/review-2026-09-04.md),
finding 5. Every reading statistic — the 14-day chart, "days active", the
reading streak, "finished this year", the per-book day breakdown — bucketed by
`substr(started_at, 1, 10)`, and `started_at` is stored as ISO-8601 **UTC**.

**The bug is not cosmetic and it is worst for the heaviest users.** At UTC+05:30
every session between local midnight and 05:30 is filed under the *previous*
day. Reading at 1 a.m. on consecutive nights produces a chart with holes in it
and, far more annoyingly, **breaks a streak the user genuinely earned** — the
one number in the app whose entire value is that it is not wrong. Anywhere west
of UTC the error runs the other way: late-evening reading is credited to
tomorrow.

Three options were considered.

1. **Store local time instead.** Rejected outright. Timestamps that carry no
   zone and are not UTC are unorderable across a move or a DST change, and the
   damage is permanent and silent — a row written before the change cannot be
   distinguished from one written after.
2. **Add `chrono`/`time` and do it properly.** The correct answer in general.
   Rejected *here* because the dependency exists solely to answer "what is the
   UTC offset right now", which the process already has an answer for.
3. **Ask SQLite.** Adopted. It is already linked, already reads the OS
   timezone, and `strftime('%s','now','localtime') - strftime('%s','now')`
   yields the current offset in seconds.

**The decisions inside option 3, which are the part worth recording:**

*The offset is cached in a `OnceLock`.* It is read once, from a scratch
in-memory connection, and never re-read. This means **a DST transition while
the app is running is wrong until the next restart** — for at most one evening,
in the two regions-worth of the year that observe one, on a chart. Accepted
deliberately: the alternative is carrying a timezone database to be right about
a boundary the user crosses twice a year while an ebook reader happens to be
open. If it ever bites, the fix is to invalidate the `OnceLock` on resume, not
to add a dependency.

*The SQL embeds a literal `'{offset:+} seconds'` modifier rather than using
`datetime(col, 'localtime')`.* This is the more important of the two. Both
would work, but they would be **two independent sources of truth for where the
day starts** — SQLite's own conversion for the bucketing, and Rust's arithmetic
for the labels the zero-fill loop matches against. Any disagreement between
them shows up as a chart bar that is silently always empty, because the label
never matches a bucket. Sharing one number makes that class of bug
unrepresentable. The cost is that these statements cannot use `prepare_cached`
(the SQL text now varies), which is noted at each call site.

*`added_by_month` stays UTC.* Month boundaries are not what anyone is looking
at on that chart, and leaving it alone keeps the diff to the queries where the
bug is observable.

*The tests inject the clock and the offset.* A test that simply calls the real
functions would pass on the broken code in a UTC CI container and fail on the
developer's machine in IST — [`pitfalls.md`](./pitfalls.md) §19, the rule that
a check which cannot distinguish the fixed build from the broken one is not a
test. Private `*_at(days, now, offset)` cores take both as parameters, so the
suite asserts IST and EST behaviour explicitly and is identical everywhere.

---

*Last updated: 2026-09-04.*

---

## 10. When to build the renderer + the crengine deep-dive (2026-09-02)

**User asked:** when should we do the renderer? and can we just use crengine?

### When: not now, not at the end — start right after sources, grow alongside

- **Not now:** the renderer is a *consumer* — nothing to render until
  sources (P7/P9) and architecture (A0) exist. Building now = building in a
  vacuum.
- **Not at the end:** the renderer is the *riskiest, most uncertain* piece
  (1–3 person-year unknown). Leaving the highest-risk, highest-value work
  to last bets the whole timeline on it landing on schedule.
- **Right after sources (recommended):** sources give it real content, the
  architecture gives it a home, and the fiction-first slice is a bounded
  on-ramp that proves the whole plan is buildable.
- **Order:** A0 architecture → P7 fiction sources → renderer vertical slice
  (alongside) → EPUB normalization → PDF (P10) / comics (P8).

### crengine deep-dive — why "just use it" doesn't work

1. **License mismatch (hard blocker).** crengine is **GPL-2.0**; KOReader's
   fork is **AGPL-3.0**. Our repo is **GPL-3.0-or-later**. GPL-2 and GPL-3
   aren't cleanly compatible; you can't just drop crengine into a GPL-3
   project without relicensing ours or keeping crengine as a separate,
   dynamically-linked component.
2. **C++ codebase, no Rust bindings.** 5,500+ commits, 2 decades old, full
   DOM/XML/CSS engine. Writing + maintaining a C FFI wrapper is a big
   ongoing task. Built for e-ink/embedded (Qt/wxWidgets/XCB) — no GTK
   frontend; KOReader integration is deeply Lua-based.
3. **Partial CSS — exactly the risk we worried about.** crengine's CSS is a
   *subset of CSS 2.1, not CSS 3*. `float`, `clear`, `border`,
   `border-width`, `font-variant`, `text-transform`, `border-collapse` are
   missing/partial — precisely what real publisher EPUBs use. KOReader
   compensates with a huge curated `epub.css` + style-tweaks; its own devs
   admit it "adds strange things when playing with publishers' CSS."
4. **Dict/annotation integration = same fight.** Exposing "word at this
   pixel" / "anchor at this position" through FFI is the same integration
   work as building our own layout — against an API we don't control.
5. **Verdict:** crengine = "EPUB reading this year" shortcut, ONLY as a
   separate dynamically-linked fallback component with a clean interface,
   swappable when our custom engine matures. **Recommendation stands:
   cosmic-text + our own normalizer.** The hard part (messy HTML/CSS →
   clean content) is ours either way; fight our own code, not a foreign
   engine's API with license baggage.

---

## 15. Timestamps stay UTC; only bucketing goes local (2026-09-04)

Came out of the code review in [`review-2026-09-04.md`](./archive/review-2026-09-04.md),
finding 5. Every reading statistic — the 14-day chart, "days active", the
reading streak, "finished this year", the per-book day breakdown — bucketed by
`substr(started_at, 1, 10)`, and `started_at` is stored as ISO-8601 **UTC**.

**The bug is not cosmetic and it is worst for the heaviest users.** At UTC+05:30
every session between local midnight and 05:30 is filed under the *previous*
day. Reading at 1 a.m. on consecutive nights produces a chart with holes in it
and, far more annoyingly, **breaks a streak the user genuinely earned** — the
one number in the app whose entire value is that it is not wrong. Anywhere west
of UTC the error runs the other way: late-evening reading is credited to
tomorrow.

Three options were considered.

1. **Store local time instead.** Rejected outright. Timestamps that carry no
   zone and are not UTC are unorderable across a move or a DST change, and the
   damage is permanent and silent — a row written before the change cannot be
   distinguished from one written after.
2. **Add `chrono`/`time` and do it properly.** The correct answer in general.
   Rejected *here* because the dependency exists solely to answer "what is the
   UTC offset right now", which the process already has an answer for.
3. **Ask SQLite.** Adopted. It is already linked, already reads the OS
   timezone, and `strftime('%s','now','localtime') - strftime('%s','now')`
   yields the current offset in seconds.

**The decisions inside option 3, which are the part worth recording:**

*The offset is cached in a `OnceLock`.* It is read once, from a scratch
in-memory connection, and never re-read. This means **a DST transition while
the app is running is wrong until the next restart** — for at most one evening,
in the two regions-worth of the year that observe one, on a chart. Accepted
deliberately: the alternative is carrying a timezone database to be right about
a boundary the user crosses twice a year while an ebook reader happens to be
open. If it ever bites, the fix is to invalidate the `OnceLock` on resume, not
to add a dependency.

*The SQL embeds a literal `'{offset:+} seconds'` modifier rather than using
`datetime(col, 'localtime')`.* This is the more important of the two. Both
would work, but they would be **two independent sources of truth for where the
day starts** — SQLite's own conversion for the bucketing, and Rust's arithmetic
for the labels the zero-fill loop matches against. Any disagreement between
them shows up as a chart bar that is silently always empty, because the label
never matches a bucket. Sharing one number makes that class of bug
unrepresentable. The cost is that these statements cannot use `prepare_cached`
(the SQL text now varies), which is noted at each call site.

*`added_by_month` stays UTC.* Month boundaries are not what anyone is looking
at on that chart, and leaving it alone keeps the diff to the queries where the
bug is observable.

*The tests inject the clock and the offset.* A test that simply calls the real
functions would pass on the broken code in a UTC CI container and fail on the
developer's machine in IST — [`pitfalls.md`](./pitfalls.md) §19, the rule that
a check which cannot distinguish the fixed build from the broken one is not a
test. Private `*_at(days, now, offset)` cores take both as parameters, so the
suite asserts IST and EST behaviour explicitly and is identical everywhere.

---

*Last updated: 2026-09-04.*

---

## 11. chapbook — the project that already IS our renderer plan (2026-09-02)

**User asked:** "do you know about chapbook? it's attempting something similar
to what we want, with cosmic-text I mean."

**Found:** `ophymx/chapbook` — "Core components for a lightweight ereader, in
Rust. EPUB 3, CBZ and PDF on stylo + cosmic-text + tiny-skia — no webview."

### Facts (verified)

- **License: Apache-2.0** — fully compatible with our GPL-3.0-or-later (Apache
  2.0 is permissive; can be incorporated into GPL projects).
- **Age: 1 week** (created 2026-08-25), 196 commits, 0 stars/forks, 1 open
  issue, heavy AI-assist (Claude co-authored). **Very early, API churn
  guaranteed** (STABILITY.md defines tiers).
- **Stack:** stylo (the CSS engine behind Firefox/Servo) for the cascade +
  cosmic-text for shaping/line layout + tiny-skia (CPU) / vello (GPU) for
  rasterization + rbook (EPUB container). No webview.
- **Workspace:** core · epub · layout (Arena DOM + stylo) · paint ·
  render-tinyskia · render-vello · opds-client · opds · cbz · **pdf (via
  hayro, pure-Rust rasterizer, Apache-2.0)** · library (SQLite) · reader ·
  viewer (winit) · **viewer-gtk (GTK4, Linux)** · ffi (C ABI) · jni (Android).
- Fonts/HTTP/credentials/storage are **injected by the host**; hosts speak
  C ABI / JNI / wasm-bindgen.

### Why it validates our plan (and teaches us three things)

1. **It IS our "custom renderer endgame," built independently.** Pagination
   is the model (break rules, widows/orphans first-class — a sidecar cascade
   for the fragmentation properties stylo doesn't carry); pages leave the
   engine as paint-neutral display lists; e-ink is a first-class target. The
   architecture we converged on is real and buildable.
2. **The stylo answer to "EPUB normalization is the hard part":** instead of
   normalizing EPUB HTML/CSS to clean content (lol_html + rules, our §8
   plan), chapbook feeds real XHTML + the EPUB 3 CSS profile into **stylo** —
   Firefox's actual CSS engine — and adds a sidecar for fragmentation.
   Real CSS fidelity without a browser. **This is now the preferred option
   for our EPUB path** (fiction-first still needs no CSS at all).
3. **The locator answer to our "auto-updater breaks annotations" risk:**
   `LayeredLocator` = quote context → spine fraction → whole-book
   progression. A position survives relayout, font-size change, screen size
   change, and — via the quote layer — **a replaced edition of the same
   book**. Highlights re-anchor the same way; positions exchange as EPUB
   CFIs. **Steal this idea regardless of adoption** — it directly fixes our
   P7 auto-updater anchor risk.

### Honest assessment for Kalam

- **Not a foundation yet:** 1 week old, 0 users. Building our platform on a
  week-old API is a bet on its trajectory. Re-evaluate when we start the
  renderer vertical slice (A0 milestone) — it will be months old by then.
- **Its stated limit matches our plan:** "a real subset of what publishers
  ship… the wrong one for an app whose job is rendering arbitrary publisher
  EPUBs faithfully." That is exactly why we keep WebKit as the fallback.
- **What remains ours regardless:** the platform (sources, plugins,
  downloads, auto-updater), the library/DB, the dict integration, the native
  UI. Chapbook is the reading engine, not the app.
- **PDF:** chapbook uses **hayro** (Apache-2.0, pure Rust) — a credible
  AGPL-free alternative to MuPDF for P10. Early-stage (no encryption,
  blending), but pure Rust + permissive. Add to the P10 shortlist.

**Decision recorded:** add chapbook to the renderer section as a *candidate
foundation* (re-evaluate at vertical-slice time); adopt the **LayeredLocator
idea** for annotations now; add hayro to the P10 shortlist; keep WebKit
fallback.

---

## 15. Timestamps stay UTC; only bucketing goes local (2026-09-04)

Came out of the code review in [`review-2026-09-04.md`](./archive/review-2026-09-04.md),
finding 5. Every reading statistic — the 14-day chart, "days active", the
reading streak, "finished this year", the per-book day breakdown — bucketed by
`substr(started_at, 1, 10)`, and `started_at` is stored as ISO-8601 **UTC**.

**The bug is not cosmetic and it is worst for the heaviest users.** At UTC+05:30
every session between local midnight and 05:30 is filed under the *previous*
day. Reading at 1 a.m. on consecutive nights produces a chart with holes in it
and, far more annoyingly, **breaks a streak the user genuinely earned** — the
one number in the app whose entire value is that it is not wrong. Anywhere west
of UTC the error runs the other way: late-evening reading is credited to
tomorrow.

Three options were considered.

1. **Store local time instead.** Rejected outright. Timestamps that carry no
   zone and are not UTC are unorderable across a move or a DST change, and the
   damage is permanent and silent — a row written before the change cannot be
   distinguished from one written after.
2. **Add `chrono`/`time` and do it properly.** The correct answer in general.
   Rejected *here* because the dependency exists solely to answer "what is the
   UTC offset right now", which the process already has an answer for.
3. **Ask SQLite.** Adopted. It is already linked, already reads the OS
   timezone, and `strftime('%s','now','localtime') - strftime('%s','now')`
   yields the current offset in seconds.

**The decisions inside option 3, which are the part worth recording:**

*The offset is cached in a `OnceLock`.* It is read once, from a scratch
in-memory connection, and never re-read. This means **a DST transition while
the app is running is wrong until the next restart** — for at most one evening,
in the two regions-worth of the year that observe one, on a chart. Accepted
deliberately: the alternative is carrying a timezone database to be right about
a boundary the user crosses twice a year while an ebook reader happens to be
open. If it ever bites, the fix is to invalidate the `OnceLock` on resume, not
to add a dependency.

*The SQL embeds a literal `'{offset:+} seconds'` modifier rather than using
`datetime(col, 'localtime')`.* This is the more important of the two. Both
would work, but they would be **two independent sources of truth for where the
day starts** — SQLite's own conversion for the bucketing, and Rust's arithmetic
for the labels the zero-fill loop matches against. Any disagreement between
them shows up as a chart bar that is silently always empty, because the label
never matches a bucket. Sharing one number makes that class of bug
unrepresentable. The cost is that these statements cannot use `prepare_cached`
(the SQL text now varies), which is noted at each call site.

*`added_by_month` stays UTC.* Month boundaries are not what anyone is looking
at on that chart, and leaving it alone keeps the diff to the queries where the
bug is observable.

*The tests inject the clock and the offset.* A test that simply calls the real
functions would pass on the broken code in a UTC CI container and fail on the
developer's machine in IST — [`pitfalls.md`](./pitfalls.md) §19, the rule that
a check which cannot distinguish the fixed build from the broken one is not a
test. Private `*_at(days, now, offset)` cores take both as parameters, so the
suite asserts IST and EST behaviour explicitly and is identical everywhere.

---

*Last updated: 2026-09-04.*

---

## 12. chapbook deep-dive: fork? adopt? PDF? platforms? alternatives? (2026-09-02)

**User asked:** can we copy it and develop it? any caveats? does it mean no
MuPDF? is it Linux or Windows? should we develop it ourselves? what else
exists?

### Verified facts

- **License Apache-2.0** — compatible with our GPL-3.0-or-later (permissive,
  incorporable with attribution). But a **fork stays Apache-2.0** (cannot
  relicense) → **dual-license tangle** (some files Apache, some GPL) if we
  fork into our repo. Real maintenance cost.
- **Cross-platform, not Linux-only**: `android/` (JNI), `ios/` (Swift),
  `viewer` (winit — Linux/macOS/Windows), `viewer-gtk` (GTK4, Linux), C ABI
  FFI for embedders. Commit history shows fontconfig excluded on
  iOS/Android/macOS. So: a portable engine with Linux as one target.
- **Not a passive library**: active, opinionated project, 196 commits/week,
  heavy AI-assist, fast-moving author. Forking = competing with its
  trajectory.
- **PDF is real but young**: `chapbook-pdf` = hayro (pure-Rust, Apache-2.0,
  most complete pure-Rust PDF rasterizer, 1000+ test PDFs) — but hayro has
  **no encryption, no blending/knockout, no color-key masking, no perf
  work yet**. So **MuPDF stays on the P10 shortlist** — hayro is the
  AGPL-free candidate, not a replacement.
- **stylo dependency is heavy**: pulls in Firefox's C++ CSS engine — real
  build complexity + big dependency (the thing we avoided with cosmic-text).
  Buys real CSS fidelity.
- **Its stated limit = our plan**: "a subset of publisher EPUBs" → **WebKit
  fallback stays**.

### Alternatives survey (nothing else close)

- **bookokrat** (MIT) — terminal EPUB/PDF/DJVU reader, full HTML rendering
  (html5ever), MathML, tables — but a **TUI**, not an embeddable renderer.
- **epub-rs / eGust/epub-reader / rustic-reader** — small or webview-based.
- **KOReader / crengine** — C++ reference (license/FFI issues, §10).
- **Tachiyomi/Suwayomi** — manga reference (§7).
- **chapbook is the only "EPUB/CBZ/PDF on cosmic-text/stylo, no webview,
  native" project.**

### Decision (locked)

1. **Do NOT fork** (license tangle + competing with a fast-moving author).
2. **Do NOT adopt as a dependency yet** (1 week old, API churn).
3. **Watch for 3–6 months; re-evaluate at renderer vertical-slice time.**
   If solid → depend on it (clean Apache dep). If stalled → no loss.
4. **Steal the ideas now** (done, §11): LayeredLocator, pagination-as-model,
   stylo cascade, display-list architecture.
5. **Build our own renderer swappable** so adopting chapbook later is a
   clean swap.
6. **MuPDF stays** on the P10 shortlist (hayro young; re-evaluate at P10).

---

## 15. Timestamps stay UTC; only bucketing goes local (2026-09-04)

Came out of the code review in [`review-2026-09-04.md`](./archive/review-2026-09-04.md),
finding 5. Every reading statistic — the 14-day chart, "days active", the
reading streak, "finished this year", the per-book day breakdown — bucketed by
`substr(started_at, 1, 10)`, and `started_at` is stored as ISO-8601 **UTC**.

**The bug is not cosmetic and it is worst for the heaviest users.** At UTC+05:30
every session between local midnight and 05:30 is filed under the *previous*
day. Reading at 1 a.m. on consecutive nights produces a chart with holes in it
and, far more annoyingly, **breaks a streak the user genuinely earned** — the
one number in the app whose entire value is that it is not wrong. Anywhere west
of UTC the error runs the other way: late-evening reading is credited to
tomorrow.

Three options were considered.

1. **Store local time instead.** Rejected outright. Timestamps that carry no
   zone and are not UTC are unorderable across a move or a DST change, and the
   damage is permanent and silent — a row written before the change cannot be
   distinguished from one written after.
2. **Add `chrono`/`time` and do it properly.** The correct answer in general.
   Rejected *here* because the dependency exists solely to answer "what is the
   UTC offset right now", which the process already has an answer for.
3. **Ask SQLite.** Adopted. It is already linked, already reads the OS
   timezone, and `strftime('%s','now','localtime') - strftime('%s','now')`
   yields the current offset in seconds.

**The decisions inside option 3, which are the part worth recording:**

*The offset is cached in a `OnceLock`.* It is read once, from a scratch
in-memory connection, and never re-read. This means **a DST transition while
the app is running is wrong until the next restart** — for at most one evening,
in the two regions-worth of the year that observe one, on a chart. Accepted
deliberately: the alternative is carrying a timezone database to be right about
a boundary the user crosses twice a year while an ebook reader happens to be
open. If it ever bites, the fix is to invalidate the `OnceLock` on resume, not
to add a dependency.

*The SQL embeds a literal `'{offset:+} seconds'` modifier rather than using
`datetime(col, 'localtime')`.* This is the more important of the two. Both
would work, but they would be **two independent sources of truth for where the
day starts** — SQLite's own conversion for the bucketing, and Rust's arithmetic
for the labels the zero-fill loop matches against. Any disagreement between
them shows up as a chart bar that is silently always empty, because the label
never matches a bucket. Sharing one number makes that class of bug
unrepresentable. The cost is that these statements cannot use `prepare_cached`
(the SQL text now varies), which is noted at each call site.

*`added_by_month` stays UTC.* Month boundaries are not what anyone is looking
at on that chart, and leaving it alone keeps the diff to the queries where the
bug is observable.

*The tests inject the clock and the offset.* A test that simply calls the real
functions would pass on the broken code in a UTC CI container and fail on the
developer's machine in IST — [`pitfalls.md`](./pitfalls.md) §19, the rule that
a check which cannot distinguish the fixed build from the broken one is not a
test. Private `*_at(days, now, offset)` cores take both as parameters, so the
suite asserts IST and EST behaviour explicitly and is identical everywhere.

---

*Last updated: 2026-09-04.*

---

## 13. Locked: own Linux-only renderer; chapbook watched, not adopted (2026-09-02)

**User decisions (confirmed):**

1. **Build our own renderer, Linux-only.** Kalam is a desktop Linux app, not
   a cross-platform engine — so no Android/JNI, iOS/Swift, wasm, C ABI, or
   per-platform font tables (all of chapbook's portability surface
   evaporates for us). Trajectories differ from chapbook (portable engine
   for embedders vs Linux content platform).
2. **Steal chapbook's ideas** (locators, pagination-as-model, stylo cascade,
   display-list architecture — already recorded §11–12); **keep watching**,
   steal more in future. **No fork, no adopt.**

**License check (user asked to verify):** chapbook is **Apache-2.0, not MIT**
(repo has `LICENSE-APACHE` only). No practical difference for us: both are
permissive; ideas are free; code copying allowed with attribution/NOTICE;
Apache-2.0 is compatible with our GPL-3.0-or-later. (Apache-2.0 has a patent
grant + NOTICE-preservation requirement; MIT is simpler. Neither infects our
GPL code.)

### Plugin system does NOT depend on the engine

Pipeline: plugin fetches/parses → sanitize (lol_html) → clean content
format → library/db/downloads/auto-updater (plugins live here) → renderer
consumes at the very end. Plugins never touch rendering; they produce
content. The plugin host seam (A0 step 8) can be built before the renderer
exists; the renderer could be WebKit today, cosmic-text tomorrow, or even
chapbook later, without plugins noticing. Only the *reading UX* (dict
popup position, annotation rendering, selection) depends on the engine —
and that depends on OUR renderer API, which we define.

**Build order confirmed:** A0 service layer + plugin seam → P6 → P7
(plugins, no engine needed) → renderer vertical slice. The engine is the
last thing to arrive; nothing before it waits for it.

---

## 15. Timestamps stay UTC; only bucketing goes local (2026-09-04)

Came out of the code review in [`review-2026-09-04.md`](./archive/review-2026-09-04.md),
finding 5. Every reading statistic — the 14-day chart, "days active", the
reading streak, "finished this year", the per-book day breakdown — bucketed by
`substr(started_at, 1, 10)`, and `started_at` is stored as ISO-8601 **UTC**.

**The bug is not cosmetic and it is worst for the heaviest users.** At UTC+05:30
every session between local midnight and 05:30 is filed under the *previous*
day. Reading at 1 a.m. on consecutive nights produces a chart with holes in it
and, far more annoyingly, **breaks a streak the user genuinely earned** — the
one number in the app whose entire value is that it is not wrong. Anywhere west
of UTC the error runs the other way: late-evening reading is credited to
tomorrow.

Three options were considered.

1. **Store local time instead.** Rejected outright. Timestamps that carry no
   zone and are not UTC are unorderable across a move or a DST change, and the
   damage is permanent and silent — a row written before the change cannot be
   distinguished from one written after.
2. **Add `chrono`/`time` and do it properly.** The correct answer in general.
   Rejected *here* because the dependency exists solely to answer "what is the
   UTC offset right now", which the process already has an answer for.
3. **Ask SQLite.** Adopted. It is already linked, already reads the OS
   timezone, and `strftime('%s','now','localtime') - strftime('%s','now')`
   yields the current offset in seconds.

**The decisions inside option 3, which are the part worth recording:**

*The offset is cached in a `OnceLock`.* It is read once, from a scratch
in-memory connection, and never re-read. This means **a DST transition while
the app is running is wrong until the next restart** — for at most one evening,
in the two regions-worth of the year that observe one, on a chart. Accepted
deliberately: the alternative is carrying a timezone database to be right about
a boundary the user crosses twice a year while an ebook reader happens to be
open. If it ever bites, the fix is to invalidate the `OnceLock` on resume, not
to add a dependency.

*The SQL embeds a literal `'{offset:+} seconds'` modifier rather than using
`datetime(col, 'localtime')`.* This is the more important of the two. Both
would work, but they would be **two independent sources of truth for where the
day starts** — SQLite's own conversion for the bucketing, and Rust's arithmetic
for the labels the zero-fill loop matches against. Any disagreement between
them shows up as a chart bar that is silently always empty, because the label
never matches a bucket. Sharing one number makes that class of bug
unrepresentable. The cost is that these statements cannot use `prepare_cached`
(the SQL text now varies), which is noted at each call site.

*`added_by_month` stays UTC.* Month boundaries are not what anyone is looking
at on that chart, and leaving it alone keeps the diff to the queries where the
bug is observable.

*The tests inject the clock and the offset.* A test that simply calls the real
functions would pass on the broken code in a UTC CI container and fail on the
developer's machine in IST — [`pitfalls.md`](./pitfalls.md) §19, the rule that
a check which cannot distinguish the fixed build from the broken one is not a
test. Private `*_at(days, now, offset)` cores take both as parameters, so the
suite asserts IST and EST behaviour explicitly and is identical everywhere.

---

*Last updated: 2026-09-04.*

---

## 14. Peer review of the renderer plan — adopted reframings (2026-09-02)

An independent AI reviewed our renderer plan against the actual code (all
its code claims verified: `cache_key()` → `None` for `Route::Reader` at
app.rs:175; `wrapRangeByPaths` nodePath+offset highlights; five highlight
colours; `LoadEvent::Finished` → `restore_pending_annotation`; find-in-
chapter is JS; theme change → `load_html` reload).

### Adopted: the justification is control, not speed

Every awkward thing in the reader traces to the **engine boundary**, not
layout: highlights as nodePath+offset (can't hold a DOM ref across load),
dict popup injected into another document, selection in JS crossing
`postMessage` with a `kalam://` iframe fallback, theme change = chapter
reload, find-in-chapter = JS. In our own renderer all five become ordinary
Rust calls against an in-memory layout tree. **That is the prize** — the
reader's complexity budget, not milliseconds.

Tractable because of the scope decision: no browse mode + sanitized
content ⇒ a box model over clean markup, not "write a browser."

### Adopted: plan changes

1. **Annotation/dict parity becomes its own milestone** (was folded into
   M2 as "selection/copy"). WebKit-side is mature: 5 highlight colours,
   range re-anchoring, popup positioning, tap-to-lookup, POS-grouped
   senses + flat-index hint, find-in-chapter. Rebuilding on our own
   hit-testing ≈ layout-engine size. Budget it like one. M2 + parity ≈
   2–3 months.
2. **WebKit stays permanently as the EPUB fallback** (replaces "may be cut
   later"). "WebKit unused for 95% of reading" is the win; the last 5% of
   pathological publisher EPUBs is a tar pit that buys nothing.
3. **First slice renders to PNG, GTK wired second** — typography converges
   without the user in the loop. Est. 3–6 sessions (~1 week) to first
   painted, paginated, themed chapter. §9's M1 (with GTK widget, page
   turn, dict hook, position restore) stands at 2–4 weeks.
4. **Committed golden corpus** (20–30 chapters: fiction, footnote-heavy
   nonfiction, CJK, RTL, image-heavy, one pathological EPUB + golden PNGs)
   — converts "does this look right" from conversation into test. The
   golden-image harness is built **before** layout code.
5. **One WebView alive across opens** — `cache_key()` returns `None` for
   `Route::Reader` (app.rs:175) so a WebView is rebuilt per book open;
   keeping one alive is a cheap A0 win, not renderer work.
6. **Restate justification after A0 + measurement** as control + memory +
   annotation architecture (perf alone is the weakest leg; A0 may already
   capture most perceived slowness).

### Nuances

- Renderer's other legs beyond speed: memory (~100–200 MB per WebKit
  instance) and startup (no engine spawn).
- Golden PNGs earn their keep through **automated pixel-diff regression
  testing** (CI-runnable, agent-testable) — not through the agent "looking"
  at them; treat goldens as tests.
- No need to vendor crates; CI fetches deps fine (the agent sandbox simply
  can't build locally).
- Total "WebKit unused for 95% of reading": 6–12 months stands;
  "verification-bound forever" framing accepted — hardening is gated on the
  user looking at real books.

---

## 15. Timestamps stay UTC; only bucketing goes local (2026-09-04)

Came out of the code review in [`review-2026-09-04.md`](./archive/review-2026-09-04.md),
finding 5. Every reading statistic — the 14-day chart, "days active", the
reading streak, "finished this year", the per-book day breakdown — bucketed by
`substr(started_at, 1, 10)`, and `started_at` is stored as ISO-8601 **UTC**.

**The bug is not cosmetic and it is worst for the heaviest users.** At UTC+05:30
every session between local midnight and 05:30 is filed under the *previous*
day. Reading at 1 a.m. on consecutive nights produces a chart with holes in it
and, far more annoyingly, **breaks a streak the user genuinely earned** — the
one number in the app whose entire value is that it is not wrong. Anywhere west
of UTC the error runs the other way: late-evening reading is credited to
tomorrow.

Three options were considered.

1. **Store local time instead.** Rejected outright. Timestamps that carry no
   zone and are not UTC are unorderable across a move or a DST change, and the
   damage is permanent and silent — a row written before the change cannot be
   distinguished from one written after.
2. **Add `chrono`/`time` and do it properly.** The correct answer in general.
   Rejected *here* because the dependency exists solely to answer "what is the
   UTC offset right now", which the process already has an answer for.
3. **Ask SQLite.** Adopted. It is already linked, already reads the OS
   timezone, and `strftime('%s','now','localtime') - strftime('%s','now')`
   yields the current offset in seconds.

**The decisions inside option 3, which are the part worth recording:**

*The offset is cached in a `OnceLock`.* It is read once, from a scratch
in-memory connection, and never re-read. This means **a DST transition while
the app is running is wrong until the next restart** — for at most one evening,
in the two regions-worth of the year that observe one, on a chart. Accepted
deliberately: the alternative is carrying a timezone database to be right about
a boundary the user crosses twice a year while an ebook reader happens to be
open. If it ever bites, the fix is to invalidate the `OnceLock` on resume, not
to add a dependency.

*The SQL embeds a literal `'{offset:+} seconds'` modifier rather than using
`datetime(col, 'localtime')`.* This is the more important of the two. Both
would work, but they would be **two independent sources of truth for where the
day starts** — SQLite's own conversion for the bucketing, and Rust's arithmetic
for the labels the zero-fill loop matches against. Any disagreement between
them shows up as a chart bar that is silently always empty, because the label
never matches a bucket. Sharing one number makes that class of bug
unrepresentable. The cost is that these statements cannot use `prepare_cached`
(the SQL text now varies), which is noted at each call site.

*`added_by_month` stays UTC.* Month boundaries are not what anyone is looking
at on that chart, and leaving it alone keeps the diff to the queries where the
bug is observable.

*The tests inject the clock and the offset.* A test that simply calls the real
functions would pass on the broken code in a UTC CI container and fail on the
developer's machine in IST — [`pitfalls.md`](./pitfalls.md) §19, the rule that
a check which cannot distinguish the fixed build from the broken one is not a
test. Private `*_at(days, now, offset)` cores take both as parameters, so the
suite asserts IST and EST behaviour explicitly and is identical everywhere.

---

## 16. Reader modularization, EPUB test expansion, and blanket dead-code removal (2026-09-05)

Came out of the high-priority refactoring pass in the comprehensive code review.

1. **Splitting `reader.rs` into sub-modules (`src/pages/reader/`):**
   The 4,452-line Relm4 component was split into 10 cohesive sub-modules (`mod.rs`, `types.rs`, `mod_model.rs`, `chapter.rs`, `session.rs`, `js_bridge.rs`, `ui_prefs.rs`, `settings_panel.rs`, `panels.rs`, `lists.rs`, `chrome.rs`). All 58 fields on `ReaderModel` were made `pub(crate)`, keeping the split 100% internal — external callers continue to import `ReaderModel` and `ReaderOut` from `src/pages/reader.rs` / `src/pages/reader/mod.rs` without API breaking changes.

2. **EPUB Parser Test Coverage Expansion:**
   Added comprehensive unit tests in `src/epub.rs` and `src/epub_book.rs` covering edge cases in OPF metadata parsing (multiple creators/subjects, EPUB3 `belongs-to-collection`), path join underflow (`../../path`), percent decoding, HTML tag stripping, container.xml parsing, and reading theme lossy parsing.

3. **Elimination of Blanket `#![allow(dead_code)]`:**
   Removed module-level `#![allow(dead_code)]` from `src/db.rs`. Based on a thorough audit, obsolete annotations were removed from active types (`SavedWord`, `QuoteRef`, `Book.file_hash`), and explicit per-item `#[allow(dead_code)]` annotations were added only to genuinely unused accessor methods and unread schema fields.

---

## 17. Medium-priority fixes: gitignore, Makefile, metadata & date math tests (2026-09-05)

Came out of the medium-priority pass in the code review (Batch 2).

1. **`.gitignore` update:**
   Updated `ci-shots/` pattern to `ci-shots*/` to properly ignore all screenshot directories produced during automated UI testing (e.g. `ci-shots-gui`, `ci-shots-1`).

2. **`Makefile` build & install target:**
   Added a root `Makefile` supporting `all`, `build` (`cargo build --release`), `dev`, `test`, `check`, `clean`, `install`, and `uninstall` targets. The `install` target copies the binary (`kalam`) to `$(DESTDIR)$(PREFIX)/bin`, the desktop file (`app.kalam.Kalam.desktop`) to `$(DESTDIR)$(PREFIX)/share/applications`, and the application icon (`assets/logo.png`) to `$(DESTDIR)$(PREFIX)/share/icons/hicolor/512x512/apps/app.kalam.Kalam.png`.

3. **Metadata Fetcher Unit Tests (`src/metadata/`):**
   Added comprehensive unit tests across `google_books.rs`, `openlibrary.rs`, and `series.rs` for JSON parsing edge cases, author list joining, small thumbnail fallback, category filtering, published date fallback, series doc extraction, and series title normalization and sorting.

4. **Date Math Unit Tests (`src/db.rs`):**
   Added unit tests for Howard Hinnant date algorithm functions (`civil_from_days`, `days_from_civil`, `days_from_iso`, `format_unix_utc`), verifying exact round-tripping for leap days (2024-02-29), non-leap centuries (1900-02-28), epoch boundaries (1970-01-01, 1969-12-31), and ISO date string parsing.

---

## 18. Low-priority fixes: external CSS stylesheet, bundled-dictionaries feature flag, LibraryService migration (2026-09-05)

Came out of the low-priority pass in the code review (Batch 3).

1. **External CSS Stylesheet (`resources/style.css`):**
   Extracted the monolithic 3,574-line string literal in `src/style.rs` out into a dedicated external file `resources/style.css`, referenced via `include_str!("../resources/style.css")`. This provides syntax highlighting, linting, and proper file separation for all application styling.

2. **`bundled-dictionaries` Feature Flag (`Cargo.toml`):**
   Added `bundled-dictionaries` as a default feature flag in `Cargo.toml`. When enabled, dictionary byte blobs are embedded via `include_bytes!`. When built with `--no-default-features`, bundled dictionary arrays resolve to empty slices (`&[]`), reducing binary size for custom/minimal builds.

3. **`LibraryService` Migration:**
   Extended `LibraryService` integration to remaining page components (`src/pages/author.rs`, `src/pages/series_float.rs`, `src/pages/settings.rs`), ensuring uniform high-level service usage across all UI views.

---

## 19. Phase 8 (P8) — Comics Local & Moku-Style Reader (2026-09-05)

Implemented local comic archive reading and interactive Relm4 comics viewer component on dedicated branch `p8-comics-local`.

1. **CBZ / CBR Local Archive Parsing (`src/comics.rs`):**
   Added zip archive reading for `.cbz` / `.cbr` files. Integrated natural alphanumeric sorting helper (`page2.jpg` < `page10.jpg`) to order archive image entries naturally regardless of digit padding. Added `extract_comic_page` and `extract_comic_cover` with robust unit tests (`image_filename_filtering`, `natural_sorting_orders_numbers_correctly`, `natural_sorting_handles_nested_paths`).

2. **Moku-Style Interactive Reader (`src/pages/comics_reader/`):**
   Created `ComicsReaderModel` Relm4 component with dark stage styling (`.kalam-comics-stage`), auto-hiding chrome top bar and bottom bar.
   - Top Bar: Close button, title + page count display (`Page X of Y`), reading direction toggle (LTR, RTL / Manga, Webtoon vertical scroll), fit mode toggle (Fit Width, Fit Height, Original).
   - Bottom Bar: Prev/Next page navigation, interactive page scrubber `gtk::Scale`.

3. **Viewport Memory Safety:**
   Implemented lazy image texture decoding for `current_page ± 2` adjacent pages into `HashMap<usize, gdk::Texture>`. Texture entries outside the active window are discarded to keep memory usage strictly bounded even when reading high-resolution multi-hundred-page comic archives.

5. **CBZ / CBR Import Support (`src/epub.rs` & file pickers):**
   Updated `import_epub` to accept `.cbz` and `.cbr` comic archives alongside `.epub`. Covers are automatically extracted from the first page of the comic archive via `extract_comic_cover` and thumbnail PNGs are generated. GTK file filters in `all_books.rs`, `home.rs`, and `comics.rs` updated to filter `*.cbz` and `*.cbr`.

6. **Dedicated Comics Hub Page (`src/pages/comics.rs`):**
   Replaced the placeholder page for `NavItem::Comics` with a dedicated `ComicsModel` Relm4 page. Features a header with book count status, "+ Import Comics" button, empty state view with call-to-action, search entry, and book grid. Clicking any comic card opens book details or directly launches the `ComicsReader`.

---

*Last updated: 2026-09-05.*



| 2026-09-05 | **Lua pivot reversed: Pure Rust + Unified Source Trait.** After evaluating the complexity of embedding a Lua runtime (`mlua`, `!Send` thread safety, API boundaries) for a personal app with 1-2 developers, we agreed to build all sources in **pure Rust**. For scraped sites that break often, fragile CSS selectors will live in a **TOML configuration file**, providing the "hot-reload" benefit without the scripting engine tax. Additionally, **Phase 7 (Fiction) and Phase 9 (Manga)** will share the exact same `Source` trait and database tables, using a `ChapterContent` enum (`Images` vs `Html`) to route to the correct reader. Komga/Kavita are dropped from the plan. |

## Master Roadmap Redux & Architecture (Sept 8)
- **Two Worlds UI:** Offline Tranquil Library default vs Online Hub.
- **Bubble Memory:** Single WebKit process SPA for multiple open books. In-app `gtk::Overlay` floating chat head outside reader.
  *(2026-09-19: expanded into a real design in §23. The WebKit half of this line is dead — there is no shared web process any more.)*
- **Inline EPUB Editing:** Non-destructive sidecar patches in `kalam.json`.
- **PDF Engine:** Zathura-style smart-crop default, Reflow toggle.
- **Scrapers & Metadata:** WebAssembly (Wasm) plugin ecosystem replacing Lua.

---

## 20. Which PDF engine? (2026-09-18)

**Trigger.** `docs/offline-roadmap.md` Module 1 calls for "crisp PDF rendering
backed by Google's PDFium engine (`pdfium-render`)". Checking that against the
code found the assumption underneath it was wrong, so the decision was taken
back up.

### What is actually there

`src/pdf.rs` parses PDFs with `lopdf` and can extract text, but **it has no
page rasterizer**. `render_page_image_uncropped` walks the page's resources
for an embedded image XObject and returns it if one exists. That means:

- **Scanned PDFs work.** A scan is one image per page, so page mode shows the
  real page, and `calculate_ink_box_for_image` + smart crop trim the margins
  properly. This is the good case and it is genuinely useful.
- **Text PDFs do not.** With no embedded image, the fallback is
  `render_text_to_canvas`, which paints a black bar per line of text so the
  ink-bounds detection has something to measure. Page mode is the default
  (`reflow_mode: false` in `src/pages/pdf_reader.rs`), so opening an ordinary
  text PDF shows a page of black rectangles.
- **Reflow mode does work** — it extracts real text and shows it as
  paragraphs.

So the first fix is not a library choice at all: default to reflow when the
page has no embedded image, and label it. Small change, removes a visibly
broken screen.

### The candidates, for the real-rendering decision

| | Rendering | Cost to Kalam | Licence |
|---|---|---|---|
| **Poppler** (`poppler-rs`) | Very good — it is what GNOME Document Viewer uses | One system package (`pacman -S poppler`), already present on most Linux desktops because browsers and Evince pull it in. GLib-based, which Kalam already is. | LGPL — no effect on a personal app |
| **PDFium** (`pdfium-render`) | Excellent — it is Chrome's engine | **Not bundled.** You must ship or download `libpdfium.so`, a ~10–25 MB prebuilt binary, and manage its version. | Apache-2.0 / BSD |
| **MuPDF** (`mupdf` crate) | Excellent, often fastest | Builds from C source; slow first build, needs a C toolchain | **AGPL-3.0** — would make Kalam AGPL |
| **`lopdf` alone** (today) | Parses only; no rasterization | Free | MIT/Apache |
| Pure-Rust renderers (`pdf`, `printpdf`) | Poor coverage of real-world PDFs | Free | permissive |

Writing our own rasterizer is not an option — it is a multi-year project and
PDF is a hostile format to do it in.

### Recommendation

**Poppler**, for the stated goals of lightweight, fast and not bloated:

1. Kalam is already GTK4 + GLib, so Poppler adds no new *kind* of dependency.
2. "Lightweight" is about what the user must install, and Poppler is already
   installed on nearly every Linux desktop. Bundling a 20 MB PDFium binary to
   avoid one pacman line is the heavier choice, not the lighter one.
3. It is what ARCH.md's original stack line implied anyway ("MuPDF for PDF" —
   a system renderer, just a different one).

PDFium becomes the right answer if Kalam ever needs to run where Poppler is
not available (Flatpak sandbox without the runtime, or a non-GNOME distro
image). MuPDF only if AGPL stops being a concern and build time stops
matterting.

**Not decided here.** This records the analysis and a recommendation; the
choice is the owner's, and nothing has been changed in `Cargo.toml`.


---

## 21. Guidelines for the app half (2026-09-18)

**Trigger.** `kalam-engine` carries `RESTRICTIONS.md` — four hard rules that
keep it from turning into a browser engine. `src/` has nothing equivalent. The
question was whether the app needs the same treatment.

### First, an uncomfortable observation about the engine's own rules

`docs/kalam/archive/RESTRICTIONS.md` says, verbatim:

> **NO `stylo` (Firefox CSS Engine):** Do not bring in `stylo` or Gecko C++
> dependencies.

The root `Cargo.toml` pins **five** stylo crates (`stylo`, `stylo_traits`,
`stylo_atoms`, `stylo_static_prefs`, `stylo_dom`, all at `=0.20.0`), and
twelve files under `crates/chapbook-layout/src/` reference stylo. The whole
cascade is built on it.

Rules 1 and 4 also mandate `lol_html` as the CSS sanitizer. **`lol_html` is
not a dependency anywhere in the workspace**, and there is no sanitizer in the
crates at all.

So two of the engine's four guardrails are already false, and nobody noticed.
That is not a criticism of whoever wrote them — it is the predictable result
of writing a rule that nothing checks. The document is in `archive/` now,
which is the right place for it.

**The lesson is the actual answer to the question.** Rules in a markdown file
do not constrain an AI agent. Rules with a test attached do.

### What already works, and why

The engine has two guardrails that *are* effective, and both are tests rather
than prose:

- `tools/chapbook-cli/tests/stability.rs` reads `Cargo.toml`'s member list and
  asserts every member is named in `docs/STABILITY.md`, and that the document
  names no crate that has left. Add a crate without updating the policy and
  the build fails.
- `crates/chapbook-core/tests/fixture_discipline.rs` walks every `.rs` file in
  `crates/` and `tools/` and fails if any default-running test reads the
  downloaded corpus.

Those work because they are checkable and because they fail loudly. Copy that
pattern, not the `RESTRICTIONS.md` pattern.

### The ratchet

Most of the rules worth having are already broken in a few places, and
fixing all of them first is not realistic. So the pattern is:

1. Count today's violations.
2. Write a test asserting the count is **no greater than** that number.
3. Fix some, lower the number in the test.

The count can never go up. That is the whole mechanism, and it is what makes
this practical for a project where an AI writes most of the code.

### Proposed rules for `src/`, each with its check

| Rule | Check | Broken today |
|---|---|---|
| No `unwrap()`/`expect()` outside `#[cfg(test)]` — already in `docs/WORKING.md` §1 | grep test, ratcheted | 6, all in `src/downloads.rs` |
| Pages ask `LibraryService`, not `Catalog` | count `Arc<Catalog>` fields in `src/pages/`, ratchet | many; ARCH.md tracks which pages are converted |
| Colours live in `theme.rs`; `style.rs` and `resources/style.css` hold shape only, no literal hex outside an allowlist | scan the CSS for `#[0-9a-fA-F]{3,8}` | unknown, small |
| Every annotation/progress write refreshes the sidecar — `docs/WORKING.md` §2 | grep test over the mutation methods | **0 — currently honoured** |
| Every `#[allow(dead_code)]` carries a reason comment | scan for the attribute with no adjacent comment | most of the 90 |
| No source file over N lines without being a generated view | line-count test, ratchet | 8 files over 1,200 |
| Every workspace member is named in the README project layout | the `stability.rs` pattern, applied to README | **0 — fixed 2026-09-18** |

### Anti-bloat rules (not checkable, so keep them few)

These have to be prose, because nothing can test for "unnecessary". Keep the
list short enough that it can actually be read:

- **No new abstraction before there is a second caller.** One caller means
  write it inline; extract when the second one appears.
- **No new dependency without saying what it replaces or enables**, in the
  README's layout section.
- **No new environment-variable switch without removing one.** There are five;
  that is the ceiling. One was already dead (`KALAM_NO_WEBVIEW_POOL`) and has
  been removed from the README.
- **No new page without a route and a way back out of it.**
- **Part 2 stays out of Part 1.** `docs/offline-roadmap.md` is the boundary;
  anything that touches the network belongs on the other side of it.

### Recommendation

Do not write a `RESTRICTIONS.md` for `src/`. Write **three ratchet tests** —
the `unwrap` one, the `Arc<Catalog>`-in-pages one, and the literal-hex one —
plus a short prose list of the anti-bloat rules in `docs/WORKING.md`. That is
less documentation than the engine has and more enforcement.


---

## 22. Two decisions locked (2026-09-18)

### PDF engine: MuPDF

Owner's decision, on rendering quality as the deciding criterion and with
licence explicitly out of scope ("strictly personal, not community"). The
comparison is in §20; the short version is that MuPDF is fastest in every
measurement found and won the one serious published fidelity study, with
PDFium a close second and Poppler clearly behind both.

**Consequences to handle when this is picked up:**

- The `mupdf` crate compiles MuPDF from vendored source — **no system PDF
  package**, which is what "lightweight" means here. But it needs a C/C++
  toolchain, `libclang` (for bindgen), and Fontconfig headers on Linux.
- Its **default features pull in XPS, SVG, EPUB, HTML, Tesseract OCR, Brotli
  and DOCX output.** Use `default-features = false` and enable only PDF, or
  the "not bloated" goal dies at build time rather than at install time.
- This does not remove the need for the small fix first: page mode currently
  draws black bars for a text PDF (see `docs/offline-roadmap.md`, "The PDF
  reader is not doing what the roadmap assumes"). That fix stands on its own
  and should land before the engine swap, because it makes the broken state go
  away today.
- `lopdf` stays for now — it is still doing text extraction and the ink-bounds
  measurement that smart crop depends on.

### Plugin substrate: WebAssembly

**Set aside — Part 2 work (owner's instruction, 2026-09-18).** The decision
below stands, but nothing is being built. This section is the record for
whoever picks it up in Part 2. `ARCH.md`'s Source-seam section is now
banner-marked as set aside and points here, so the stale Lua text there is no
longer silently authoritative.

Owner's decision, and it is the one recorded in the most recent discussion. It
supersedes both earlier answers — `ARCH.md`'s Lua and the "pure Rust + TOML
selectors" pivot.

It is a defensible choice. Worth being clear about what it costs and what it
buys, because the cost lands in a specific place:

**What it buys.** Crash isolation — a scraper that panics cannot take the
reader down. Sandboxing — plugin code cannot reach the filesystem, which
matters for anything downloaded. Both are real.

**What it costs.** `wasmtime` is a large dependency tree (the archived CI logs
show `wasmtime`, `wasmtime-cache`, `wasmtime-environ` and
`wasmtime-wit-bindgen` all compiling), which is build time and binary size on
a machine described in `docs/kalam/WORKING.md` as 4 GB of RAM and a hard disk.
Writing or fixing a plugin then needs a wasm toolchain rather than an edit.
And the sandbox's main value is containing *untrusted* code — in a project
with no community and one author, it is mostly protecting the author from the
author.

**So, three conditions that make it work well:**

1. **Put `wasmtime` behind a Cargo feature**, off by default, so the ordinary
   build and the CI build do not pay for it.
2. **Do not ship the plugin host until there is a second plugin.** One plugin
   behind a wasm boundary is all of the cost and none of the benefit.
3. **Keep the TOML selector config alongside it**, not instead of it.
   `src/sources/scrapers/mangaball.toml` is already that shape. Most scraper
   breakage is a changed CSS selector, not changed logic — and a selector fix
   that needs a wasm rebuild is not the "fix loop measured in seconds" the
   original design was reaching for. Wasm for sources that need logic, TOML
   for sources that need a selector. That is a tiered design, not a
   contradiction, and the existing scaffolding already half-supports it.


---

## 23. Bubbles — multiple books open at once (2026-09-19)

**Status: ✅ design accepted, 🔶 not built.** Part 1, Phase 2.

**Asked:** *"you know about android 17 bubble feature? how you can have
multiple apps kind of floating on the screen all the time? and move them
around? ... opening multiple books all at once, and then they look like stacked
bubbles on top of each other on the screen, despite wherever I am, and clicking
them open them in a floating window, all the books lined up on the top."*

The only prior record was the one line in *Master Roadmap Redux* above. Half of
it was dead on arrival — it assumed a single WebKit process holding several
books, and there is no WebKit any more.

### Accepted

- **Android-style stacked bubbles**, over any screen in the app. Tap one to
  read it in a floating window; the rest line up along the top.
- **In-app only**, never an OS window. A separate always-on-top window fights a
  tiling compositor, and Kalam already hit exactly that with dialogs — which is
  why the in-app float layer exists at all.
- **Open into a bubble** straight from the library, and **minimize back** into
  one from the reader.
- **Bubble face:** the cover with a thin progress ring.
- **A bubble holds no book.** It is a bookmark — book id, cover thumbnail,
  position, all already database rows. The book is built on tap. This is the
  load-bearing rule: opening a book is 100–220 ms, far too slow to do for ten
  bubbles at startup and far too much memory to hold.
- **Three states:** bubble (kilobytes) · warm (opened, budget trimmed,
  suspended — one or two) · reading (full budget — one).
- **All three readers are bubble-eligible** — EPUB, PDF and comics — and all
  three also serve online content later: the EPUB reader for fanfiction and
  AO3, the comics reader for manga. The online half is Part 2; the bubble
  architecture is Part 1.

### The constraint that keeps it buildable

**Two readers, and the full one loses nothing.**

| | Full reader | Bubble reader |
|---|---|---|
| Left sidebar | TOC **and Settings** | TOC only |
| Right sidebar | **Highlights, Bookmarks, Words** | none |
| Floating pills | **all** — selection chip, highlight colours, dictionary popup, quote/copy | none |
| Status | **unchanged; nothing removed** | new, thin |

The full reader is 5,845 lines across 12 files with five sidebar tabs; the
bubble reader keeps one of the five.

Worth recording *why* that helps, because the obvious reason is not the real
one. **It is not a memory saving** — memory lives in the engine's book object,
not in the sidebars, which are ordinary widgets over database rows. The saving
is complexity: a small floating window cannot fit the full chrome anyway, and a
bubble reader wanting the full feature set would mean maintaining 5,845 lines
twice forever. Instead the reading surface becomes one component with two
shells.

### Not decided

- How strict a background book's budget should be. That number should be picked
  after the bubbles exist and can be felt, not before.

---

## 24. Reader memory — measured, with two ideas rejected (2026-09-19)

**Trigger:** the owner's hardware — **4 GB RAM, 1 TB HDD, Pentium Silver
N5030, Intel UHD 605.** Constraints that make the engine's defaults worth
questioning.

### ✅ Chapter images now decode to the size a page can draw

Every `<img>` was decoded at full native resolution. `collect_images` took no
page size even though `PageMetrics` was in scope at its one production call
site. Raw RGBA costs 4 bytes a pixel, so a 3000×4000 scan is 48 MB decoded
while occupying at most the reading column.

Measured on the owner's machine, *The Dragonet Prophecy*: four chapters went
from **85,082 KB to 24,361 KB — 83.1 MB to 23.8 MB, 71% less**. The engine
reports 77.1 MB decoded against 17.9 MB kept.

**The number that matters is not the percentage, it is that the cache now
fits.** Those chapters were 2.6× over the 32 MB budget before, so an
image-heavy book sat permanently over budget and re-decoded on scroll at the
77–120 ms/page the engine measures.

Cost is not zero: the resize added ~50 ms on small-image chapters while
*reducing* it on large ones (487 → 385 ms), because `ImageStore::insert`
premultiplies every stored pixel, so fewer pixels is less work there.

### ✅ Which budget is actually in force

Worth pinning down, because reading the code gave the wrong answer once. The
*engine's* `DEFAULT_CACHE_BUDGET` is **192 MB**. `kalam-reader` overrides it
with **32 MB**, and the comment beside it says it was chosen *"on a machine
with 4 GB in total"* — the target machine. Kalam runs at 32 MB, confirmed by
log rather than by reading.

### ~~Shared font system across sessions~~ — rejected, not needed

**Proposed:** every `Session::open` runs `build_font_system`, which scans the
whole system font database again, so with several books open the cost is paid
several times. Sharing one seemed likely to be the centrepiece of the engine
work for bubbles.

**Rejected on measurement.** Four books, all reporting **8 faces**: the scan is
**1–5 ms** out of a 32–78 ms open. Sharing it would save milliseconds and cost
real complexity. The engine already times this split and always has — the
number was invisible only because no logger was installed.

This is the reason the logger was built before the design was settled: *the
measurement existed and nobody could see it.*

### ~~Disk-backed page cache~~ — rejected for now, probably unnecessary

**Proposed:** a weak CPU and a 1 TB drive argue for trading decode time for
disk reads — decode a page once, write it downsampled, read it back rather than
re-decoding. Genuinely the right trade *on this hardware*.

**Held off rather than refused.** It was justified when a chapter cached 34 MB
against a 32 MB budget and eviction meant constant re-decoding. The image fix
put the same chapter at 13 MB, under budget on its own, so the thrash it was
answering largely stopped. Revisit only if re-decodes show up again.

### ✅ Lazy loading mostly already exists

Asked whether chapters could load and unload on demand. **They already do:**
`layout_unit` builds a chapter only when asked; `evict_keeping` drops the
least-recently-read under the budget while pinning the current and visible
chapters; `prefetch_one` reaches exactly one adjacent chapter; `suspend()`
drops everything but the page on screen.

Three real gaps: eviction is per *chapter* not per page; there is one budget
for one book where bubbles need one per state; and the chapter character count
runs eagerly on open (30–147 ms).

### ✅ Page-level eviction — a tripwire, not a task

Holding a 141-page chapter as one lump was a real problem when it cached 34 MB
against a 32 MB budget. The same chapter is now **13 MB and fits on its own**.
**Do not build this until a single chapter again exceeds the budget.** The
check, so the tripwire is testable rather than a matter of opinion:
`RUST_LOG=info kalam`, open an illustrated book, look for a
`laid out unit N (... KB)` line above 32,768.

### The meta-lesson

Three confident claims about this codebase turned out to be wrong in one week,
all from reading code and reporting it as fact: that the reader had only two
keyboard shortcuts, that the cache budget was 192 MB, and that the font scan
might dominate open time. The first two were corrected by reading more
carefully; the third by measuring. **Where a number decides a design, log the
number rather than inferring it.** That is why the logger landed before the
bubble design was settled, and why the image fix reports its own savings.

*Last updated: 2026-09-19.*
