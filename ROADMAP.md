# Kalam — Detailed Roadmap

Living plan. Phases are **sequential gates**: we do not start phase N+1 until
phase N compiles on CI **and** you have run it on Arch and signed off (or filed
change requests).

This file is updated whenever product/UX decisions change.

---

## ⚠️ Read this first — every chat, every agent

If you are an AI agent starting work on this repo, read ALL of this before
touching code:

1. **`README.md`** — what the app is, current status table.
2. **`ROADMAP.md` (this file)** — the plan, the order, the gates, the locked
   decisions. The **"Current trajectory"** section below is the single summary
   of where we are and what comes next.
3. **`docs/conversation.md`** — the living design-conversation log. Every
   accepted / rejected idea, with reasons. **Do not re-litigate settled
   decisions** — if you think one is wrong, raise it in chat first.
4. **[`docs/pitfalls.md`](./docs/pitfalls.md)** — mistakes already made in this
   repo and how they were fixed. Every entry cost a CI cycle or shipped a bug
   once already. Skim it before you write code, and **read the relevant section
   in full** before you touch GTK sizing, scrollbars, keyboard shortcuts,
   error paths, or anything that deletes a call site.

**Hard rules for every change you make:**

- **Write and speak in plain, simple English.** This is a standing instruction
  from the user, given 2026-09-04: *"I can't understand anything you said. I
  need you to use easy and simple language to explain things."* It applies to
  chat replies first, and to docs and comments too.

  What that means in practice:

  - **Explain the thing before naming it.** Not "the `PENDING_FRAMES` weak-ref
    map breaks under recycling" — instead "each cover picture remembers which
    book it belongs to. If we start reusing pictures for different books, a
    slow-loading cover can land on the wrong book."
  - **No unexplained jargon.** Words like *virtualization*, *refcount*,
    *GObject*, *N+1*, *LRU* mean nothing on their own. Either say it in
    ordinary words, or give a one-line explanation the first time.
  - **Short sentences. One idea each.** Long sentences with three clauses and
    two dashes are the main problem, not the vocabulary.
  - **Lead with the answer**, then the reasoning. Do not make the user read a
    wall of analysis to reach the recommendation.
  - **Numbers need meaning.** Not "507 MB peak RSS at 2,000 books" on its own —
    "it uses 507 MB of memory, which is a lot on a 4 GB machine."
  - **When asking the user to choose, make the options concrete**: what changes
    on screen, what could break, how long it takes.

  Being simple is not the same as leaving things out. Keep the honesty, the
  caveats and the numbers — just say them in words that do not need a glossary.
  Note that this instruction is *about communication only*. It does not lower
  the bar for the engineering, the testing, or the docs.

- **Keep the docs current — always, in the same commit as the code.** When you
  finish a phase / feature / decision, update: README (status table),
  ROADMAP (phase status, changelog table, next steps), and
  `docs/conversation.md` (if the work changes a decision). A change that
  leaves the roadmap stale is **not done**.
- **Commit work regularly after major changes or completed batches.** This is a standing instruction given 2026-09-05. Do not leave large diffs accumulating uncommitted. Make clean, descriptive git commits after completing a major task or logical batch, bundling code changes and updated docs together.
- **When you get something wrong, write it down.** If a mistake cost a CI
  failure, produced a user-visible bug, or was only caught by re-reading an
  existing warning, add it to `docs/pitfalls.md` in the same commit as the fix:
  what went wrong, *why*, and what to do instead. The file exists so the next
  agent does not repeat it — an unrecorded mistake will be made again.
- **Never skip the changelog.** ROADMAP ends with a "Changelog of plan
  decisions" table — append a dated row for every phase shipped or decision
  locked. This is how a new chat catches up in one glance.
- **CI is the gate.** No local Rust toolchain in the sandbox: push and watch
  GitHub Actions (`gh run list`). Never force-push. ~~The App token cannot push
  `.github/workflows/`~~ — **corrected 2026-09-18:** it can, proven by commit
  `a4c4d54` / run `35362407132`. Edit the real workflow file and push it; then
  re-sync the copy at `docs/ci/github-actions-ci.yml`, which exists only as a
  fallback in case that permission is ever lost.
- **You cannot see the screen.** The user is the QA loop for anything visual:
  ask for error text (not screenshots — you can't view them), and have the
  user run the app on Arch at phase boundaries.
- **Branch:** each Arena session is pinned to its own `arena/<id>-calibre-alt`
  branch. Work only on the branch the current session names, and push only to
  it. Do not copy the branch id out of this file — it changes every session.

---

## Working agreement

| Who | Does |
|-----|------|
| **Agent** | Implements the phase, pushes code **and workflow files**, keeps CI green, updates this doc |
| **CI (GitHub Actions)** | `fmt` · `clippy` · `test` · `cargo build` on every push, plus a non-blocking GUI smoke test |
| **You** | (1) Paste failed CI step logs only when `ci-logs/` does not have them. (2) Run the app on Arch **once per completed phase** for UX feedback |

You do **not** need to build between small commits. Only at phase boundaries.

**How the agent should talk to you:** plain, simple English — see the "Write and
speak in plain, simple English" rule in the "Read this first" block above. If a
reply is hard to follow, say so; that is a bug in the reply, not in you.

**Workflow files:** the agent edits `.github/workflows/ci.yml` directly and
pushes it (permission confirmed 2026-09-18), then re-syncs the copy at
`docs/ci/github-actions-ci.yml`. See `docs/ci/README.md`.

### Definition of Done (every phase)

1. CI green on the branch  
2. Feature list for that phase implemented (see below)  
3. README / ARCH / **this ROADMAP** updated if behaviour or UX targets changed  
4. You ran it on Arch (or explicitly deferred) and listed change requests  

### Documentation discipline (non-negotiable)

- **Every phase ships with its docs.** README status table, ROADMAP phase
  status + changelog row + next-steps update, and (if a decision changed)
  `docs/conversation.md` — in the **same commit** as the code, never "later".
- **The changelog table is the memory.** A new chat must be able to catch up
  by reading: README status, ROADMAP "Current trajectory" + changelog tail,
  and `docs/conversation.md` §6 (standing decisions).
- **A fresh chat must be able to continue without asking the user what the
  plan is.** If that is not true after your change, the change is not done.
- **Decisions change only through conversation.** Mark reversals in
  `docs/conversation.md` with the old position struck through and the new one
  recorded with the date.

### Parked for later discussion

Raised, deliberately not decided yet, and **not** dropped. Listed here so a new
chat does not have to rediscover them.

- **Multiple books/readers open at once** (raised 2026-09-04). Reading two
  things side by side, or keeping several open and switching. Touches the
  reader, the WebView pool (which currently parks exactly *one* view — see the
  A0 notes), routing, and reading-session bookkeeping (two open books must not
  both count reading time). **Discuss before designing**; the user asked to
  return to this as its own thread.

### Non-goals (whole project)

- Z-Library / unauthorized shadow libraries  
- Calibre multi-app suite, content server, fetch news  
- ~~Plugin API~~ — **reversed 2026-09-02**: plugins are wanted; see
  "Architecture & performance track" below and `docs/conversation.md` §5
- Windows / macOS  

---

## North star

> One native Linux app: fast personal library for a few thousand books, excellent
> EPUB reading & annotation, honest metadata, modular sources (AO3, fanfic,
> comics) later — no bloat.

**Stack:** Rust · GTK4 · Relm4 · custom CSS · SQLite · WebKitGTK (EPUB) · image
pipeline (comics) · MuPDF later (PDF) · cosmic-text (custom renderer, A0)

---

## Current trajectory (locked 2026-09-02)

**Where we are:** P0–P5 shipped and CI-green; dictionary track Phases 1–10
shipped and CI-green (173 unit tests). The product is now a **content
platform**: fiction (AO3 / FFN / webnovels) and manga sources with native tag
search, downloads, offline reading, auto-updates — on a fast, Yazi-style
architecture.

**The agreed order — do not reorder without asking:**

1. **A0 — Architecture & performance track** — ✅ **done 2026-09-04** except the
   plugin seam. measure → `LibraryService` behind `Catalog` → thumbnails at
   import + async cover decode → task manager → preloaders → grid
   virtualization (the numbers did say so in the end: 502 MB → 247 MB, 434 ms →
   12 ms) → perf-budget CI test (query counts, not milliseconds). The
   **plugin-host seam** is designed in `docs/source-seam.md` and lands as code
   with its first implementation, not before.
2. **P6.5 — Libraries** — ✅ **done 2026-09-04.** Pick the folder, keep several
   libraries, switch between them, copy one to another machine and it opens.
3. **P8 — Comics local** — ✅ **done 2026-09-05** (CBZ/CBR import + Moku-style image
   pager; cover-format fix; Webtoon + LTR/RTL drawer; all loose ends tied). Next:
   **P9 — Manga platform** (same `Source` trait, `ContentKind::Images`;
   MangaDex official API built in — moved here from P7 on 2026-09-04 — then
   Komga/Kavita/OPDS clients, scraped sites as pure Rust + TOML config).
4. **P6 + P7 — Downloads hub and the fiction client**, **combined 2026-09-04**
   and done together: a queue with nothing to download is a shell, and a client
   that fetches things needs somewhere for those jobs to live. Building them
   apart would design the queue against imagined callers. A browsing *client*,
   not a downloader: category browsing, author pages, the site's own sort
   orders, search with full filters — downloading is one thing you can do while
   in there. Sources AO3 → Royal Road → Literotica → FFN (Webnovel dropped:
   paywalled). Split into stages P7a–P7f; see that section.
5. **Renderer vertical slice** — custom renderer on **cosmic-text** for fiction
   content. The **calibration milestone**: 2–4 weeks of sessions; if it takes
   longer, stop and reassess before sinking months in.
6. **EPUB path** → custom renderer takes EPUBs: either a normalization pipeline
   (lol_html + rules) or **stylo** (Firefox's CSS engine, via chapbook);
   WebKit demoted to fallback for exotic EPUBs (may be cut later). **Adopt
   quote-anchored locators (LayeredLocator, chapbook's model) for annotations
   regardless** — note that P7's re-anchoring decision (match on the saved
   highlight text) is the same idea arrived at independently, and the two
   should converge rather than both being built.
7. **P10 — PDF** (MuPDF) · **P11 — Tools** · **P12 — WebAssembly (Wasm) plugin system** for
   the surfaces that rot (scrapers), with stable-API providers staying
   built-in; **not** a marketplace (`docs/source-seam.md` §0, §9a).
8. **P5.5 — UI overhaul, LAST** (moved 2026-09-04). Every phase above adds
   screens, so restyling now means restyling again later; a design settled
   before its content is a guess.

**Locked decisions (full reasoning in `docs/conversation.md`):**

- **Stay Rust.** No language rewrite — performance is architecture, not
  language (§1–2).
- **No browse mode.** Kalam never renders arbitrary websites; sources return
  structured data via plugins; search UI is native (§8).
- **Custom renderer is the endgame for ALL reflowable text** (cosmic-text
  based). WebKit = EPUB fallback only, may be cut. crengine rejected as the
  base: GPL-2/AGPL license mismatch with our GPL-3.0, C++ FFI burden, partial
  CSS 2.1; at most a separate dynamically-linked fallback bridge (§8, §10).
  **chapbook (ophymx, Apache-2.0) is a candidate foundation** — it is this
  exact architecture, already built (stylo + cosmic-text + tiny-skia/vello,
  no webview, GTK4 viewer, quote-anchored locators). Re-evaluate at
  vertical-slice time (§11).
- **Manga = Tachiyomi-shaped `Source` adapter API; Lua plugins we write for
  scraped sites, built-in Rust for API-backed ones** (MangaDex, Komga, Kavita,
  OPDS). The split is "does it rot", not "is it a source" — `source-seam.md` §9a.
  No Kotlin extension bridge (Android APKs — wrong shape); no Suwayomi server
  rewrite; optional Suwayomi-server *client* adapter later (§7–8).
- **PDF = MuPDF** (fixed-layout, AGPL — acceptable; Poppler/GPL the
  alternative; **hayro** — Apache-2.0 pure-Rust — also on the P10 shortlist,
  §11). **Comics = image decode + GTK pager** — no engine (§8).
- **Perf order:** measure → thumbnails/async decode → virtualize if numbers
  say so (§2).
- **Renderer effort estimate:** 2–4 wk vertical slice; 6–12 months total for
  "no WebKit for text". **The user is the QA loop** — the agent has no
  display (§9).
- **Borrow list** (license-compatible with GPL-3.0): FanFicFare (fiction
  adapters), Tachiyomi extensions (pattern), MangaDex API, Komga/Kavita,
  KOReader + crengine (reference), cosmic-text/swash/fontdb/vello (Rust text
  stack), lol_html/ammonia (sanitizing), Yazi, Foliate (§8).

---

## Two readers (architecture — locked)

Kalam has **two distinct viewing modes**. Same library; `Read` routes by format.

| | **Text reader** | **Comics reader** |
|--|-----------------|-------------------|
| **Formats** | EPUB now; PDF later; AO3/fanfic as downloaded EPUB/HTML | CBZ/CBR local first; remote later |
| **Engine** | WebKitGTK (EPUB/HTML); MuPDF/Poppler later for PDF | Decode images from zip/rar on demand |
| **Motion** | Continuous long scroll, **chapter-wise** for EPUB | Page mode and/or webtoon long-strip |
| **Look target** | Immersive “tablet book” (cream/sepia page, floating chrome) | Immersive “Moku-like” (black stage, top meta + bottom scrub) |
| **Chrome** | Top-left close/crumb; bottom pill: ‹ ☰ ch Aa › | Top: ✕ title chapter pages; bottom: scrubber + zoom |
| **Progress** | Chapter index + in-chapter fraction → SQLite | Page index (+ chapter/volume) → SQLite |
| **Phase** | **P2** shipped (shell + read); **P3** annotations | **P8** local; **P9** sources |

**Text reader visual rules (agreed):**

- Default theme: **sepia** (cream paper, brown ink)  
- Body text is **never** browser-blue; EPUB `<a>` wrappers forced to ink color  
- **No underlines** on body/links while reading  
- Selection highlight tint reserved for P3 (pink-ish mark, Apple Books–like)  
- Heavy top toolbars avoided; controls live in floating pills  

**Comics reader visual rules (agreed — Moku reference):**

- Pure **black** letterbox; art centered  
- Thin **top bar**: close, prev/next chapter, title, page `i / N`, zoom %  
- **Bottom bar**: page scrubber, zoom control, prev/next page  
- Fit width / fit height / RTL; optional continuous strip mode  
- Memory-safe: only nearby pages decoded (4 GB RAM)  

**Renderer trajectory (2026-09, supersedes the engine column above):** the
custom renderer (cosmic-text) is the endgame for ALL reflowable text — source
fiction first, EPUB after a normalization pipeline. WebKit becomes the EPUB
fallback only (exotic EPUBs), then may be cut. The table above remains the
*current* engine map; see "Current trajectory" and `docs/conversation.md` §8.

---

## Phase map (overview)

```text
P0  Shell ───────────── sidebar, routing, float/detail shells     ✅ done
P1  Library core ────── SQLite, import EPUB, cover cards, search  ✅ done
P2  Text reader ─────── WebKit EPUB, themes, TOC, progress, UI    ✅ done
P3  Annotations ─────── highlights, quotes, offline dictionary    ✅ done
P4  Library depth ───── shelves engine, lists, tags, analytics    ✅ done
P5  Metadata ────────── edit metadata, cover pick, Open Library      ✅ done
P6.5 Libraries ──────── pick the folder, several of them, portable  ✅ done
P8  Comics local ────── CBZ/CBR + Moku-style comics reader           ✅ done
P9  Manga platform ──── Suwayomi-class sources, same Source trait
                        (MangaDex API built in; scrapers via Lua)
P6+P7 Downloads +────── combined 2026-09-04, built together: unified queue
      fiction client    + folder watch, and a browsing client for AO3 /
                        Royal Road / Literotica / FFN — browse, search with
                        full filters, author pages, read online, download,
                        auto-update
P10 PDF ─────────────── MuPDF in text-reader family + basic marks
P11 Tools ───────────── convert (external), polish, Calibre import
A0  Architecture track ─ service layer + task manager + preloaders +
                        thumbnails + source seam + windowed grid (done);
                        only the plugin-host seam is left, with P7
P5.5 UI overhaul ────── LAST: restyle every screen once they all exist
P12 Lua plugins ─────── for surfaces that rot (scrapers, add-on metadata);
                        stable-API providers stay built-in Rust
```

**Order lock:** P0→P5 stay sequential. P6–P11 may reorder after P3 if priorities shift.
Comics UI design is frozen in this doc now; **implementation is P8**.

**Architecture track (A0)** is the performance/async work discussed in
`docs/conversation.md` §§1–3 — the **next big work item** (detailed in the
"A0 — Architecture & performance track" section below). It is a track, not
a phase: it may interleave with P6–P11. The **renderer decision** (WebKit
vs custom text engine) belongs to it — resolved direction: **no browse
mode**; **custom renderer is the endgame for ALL reflowable text**
(cosmic-text based; fiction first, EPUB after a normalization pipeline);
WebKit = fallback only (exotic EPUBs), may be cut later; PDF = MuPDF;
comics = image pager (see `docs/conversation.md` §8).

> **Track phases are separate from P0–P11.** The dictionary overhaul uses its
> own numbering (Phases 1–10, in the "Dictionary overhaul" section below) —
> those are *not* P8/P9/P10. Global P8/P9/P10 remain Comics local / Comics
> sources / PDF.

---

## P0 — Application shell  ✅ done

**Goal:** Real window + navigation skeleton.

### Shipped

- [x] `kalam` / `app.kalam.Kalam`
- [x] Slim sidebar: Home, Library, Shelves, Downloads, Comics, AO3, Fanfic, Settings
- [x] Settings bottom-pinned
- [x] Route stack + Back
- [x] Draft dark CSS; later refined with library/reader skins
- [x] CI workflow (manual install of `.github/workflows` by you)
- [x] Arch smoke-test signed off

### Out

Real DB, import, reader (those are P1/P2).

---

## P1 — Library core  ✅ done

**Goal:** Real EPUBs live in Kalam.

### Shipped

- [x] SQLite `~/.local/share/kalam/catalog.db`
- [x] Import EPUB (file dialog); copy to `library/<uuid>/`; cover extract
- [x] OPF title/authors/tags/description (HTML stripped to plain text)
- [x] Cover-card grid (Goodreads-like): fixed **1.6:1** cover, title+author under
- [x] Click cover → float; Ctrl+click → full book page
- [x] Search + sort (title / author / added)
- [x] Remove book (DB + files)
- [x] Home: continue + recently added (real data)
- [x] Float panel: Suwayomi-style compact detail (cover rail, badges, Read, meta)
- [x] Settings shows data paths

### UX notes locked in P1

- Cards must stay **uniform grid cells**; long titles **ellipsize**, full text on tooltip  
- No full-width stretched single covers  
- Float: no open/close morph animations for now (can return later)  

### Out (still later)

Shelves rules engine, network metadata, comics import.

---

## P2 — Text reader (EPUB)  ✅ done

**Goal:** Comfortable EPUB reading; position restores.

### Shipped

- [x] WebKitGTK 6 reader surface
- [x] Open from book page / float **Read**
- [x] Parse spine + NAV/NCX TOC; chapter-wise load
- [x] Themes: Light / Sepia / Dark (default **Sepia**)
- [x] Font size A± ; keyboard N/P, arrows, Esc
- [x] TOC via bottom pill popover
- [x] Progress in SQLite (`reading_progress` + `books.progress`)
- [x] Immersive chrome: sidebar/topbar hidden while reading
- [x] **Restyle (P2.1):** top-left close+crumb; bottom floating pill (‹ ☰ ch Aa ›)
- [x] Reading CSS: force ink color, kill blue + underlines on body/links
- [x] No background FlushProgress timer (crash fix on leave)

### Partial / deferred

- [ ] Near-end auto-advance (JS bridge removed for CI stability; use N/›)
- [ ] True multi-chapter DOM buffer (still one chapter WebView load)
- [ ] Instant CSS var updates without reload (currently reload chapter on Aa/theme)
- [x] Selection toolbar: compact themed icon actions with tooltips (P3)

### Arch check

Read long EPUB, change theme/font, TOC jump, quit, reopen mid-book; no crash on leave; text not blue/underlined.

### Exit criteria

Daily-driver EPUB reading without annotations — **met for P2 scope**.

---

## Reader chrome restyle (P2.1)  ✅ done

**Decision (2026-07-26):** the reader is a tablet-book, not a browser: no heavy
top toolbar. Restyled while P2 was still open.

- [x] Top-left close + crumb (book → chapter); no reader top bar
- [x] Bottom floating pill: `‹ ☰ ch Aa ›` (prev / TOC / chapter label / font+theme)
- [x] Immersive mode: app sidebar + topbar hidden while reading
- [x] Reading CSS: body/links forced to ink color (never browser-blue), no
      underlines on body text; selection tint reserved for P3
- [x] Chapter reload on theme/font change (accepted; instant CSS-var swap deferred)

**Look target:** immersive "tablet book" — cream/sepia page, floating chrome.

---

## P3 — Annotations & dictionary  ✅ done

**Goal:** “Editor in the viewer” on the text-reader surface.

### Shipped

- [x] Selection in WebView → floating chip: **Highlight** (yellow/green/blue/pink/orange) / **Save quote** (❝) / **Dictionary** (Aa) / copy
- [x] Shortcut **`d`** → dictionary popover near word (via JS + GTK popover search)
- [x] Offline dict packs: **StarDict** (.ifo/.idx/.dict[.dz]), **SQLite** .db with entries(word,definition), **TSV** (word<TAB>def)
- [x] Bundled English WordNet 2025 starter pack (about 127k headwords, about 4.2 MB compressed), enabled on first run with attribution and licence notices
- [x] Bundled English Idioms and Expressions pack (1,024 phrase-to-meaning entries, about 16 KB compressed), kept separate and enabled on first run with source-quality and licence notices
- [x] Import via Settings → Offline dictionaries → + Import dictionary; list & remove
- [x] Persist annotations: chapter_index + DOM path (nodePath) + offsets, color, text_excerpt, note, kind
- [x] Reinject highlights on chapter load (`kalamInjectHighlights` + `wrapRangeByPaths`)
- [x] Annotations list: reader bottom pill **✎** shows highlights/quotes for current book, Jump & Delete
- [x] Reader selection toolbar uses compact themed icon actions with useful tooltips; default WebKit context menus are suppressed without changing text selection or the automatic selection-actions toolbar
- [x] Annotation workflow: exact cross-chapter jumps, near-top positioning, temporary focus emphasis, and annotation-ID-first restoration
- [x] Annotation cards: dark rounded cards with subtle pastel tints, colored left edges, saved note previews, expandable multiline note editing, and autosave without Save/Cancel controls
- [x] My Library → **Saved quotes** (real data, search, delete, Export Markdown → `~/Quotes.md`)
- [x] My Library → **Saved words** (real data, search, delete, saved from dict lookup with context)
- [x] Export quotes → Markdown with book title, chapter, color, timestamp, quote block
- [x] Selection chip UI: semi-transparent dark pill above selection, color dots + ❝ Aa ⧉
- [x] Dictionary popup inside WebView: single merged entry with numbered senses and POS, bookmark + copy in the sticky header, synonym/antonym chips and idiom cards, did-you-mean suggestions, "Show N more" for long entries
- [x] Reader typography popover now includes dictionary search (prefix → substring fallback) + Save/Clear
- [x] Highlight storage: SQLite `annotations` table, `saved_words`, `dictionaries`, `dict_entries`
- [x] CSS: soft highlight tints (yellow 0.62, green, blue, pink 0.70, orange), chip & dict popup styling, badge colors for annotation list, P3 GTK rows

### Look

- Selection UI inspired by tablet readers (compact chip above selection) — implemented as `#kalam-chip` inside WebView
- Highlight colors soft (incl. pink/rose option like reference photo) — `kalam-hl-pink` rgba(251,207,232,0.70)
- Must not reintroduce blue underlines on body text — preserved via reading CSS `!important`

### Known issue (deferred)

- [ ] Triple-click paragraph selection can still render an italic run in a different temporary selection text colour from the preceding roman text. Revisit the WebKit selection rendering later.

### Out (deferred)

- Full EPUB HTML editing, sync, and collaborative notes
- CFI spec (using path+offset for now; excerpt fallback is tracked in the reader-improvements section below)
- Dictionary definition HTML rendering (currently stripped to plain text for GTK popover, WebView popup escapes HTML)

### Arch check

- Highlight, quit, reopen → highlight persists and re-injects
- Offline dictionary import (StarDict + SQLite) → lookup via chip or D, save word, appears in Saved words
- Export quotes → `~/Quotes.md` with Markdown

### Exit criteria

Annotations trustworthy enough you stop using another app for EPUB markup — **met**: highlights survive reload, quotes & words saved, dictionary offline.

### Notes on implementation

- JS bridge via `window.webkit.messageHandlers.kalam.postMessage` + fallback `kalam://` iframe + `title` notify; Rust side via `UserContentManager::register_script_message_handler` (world None) + `connect_script_message_received` + `decide_policy` fallback
- Progress still via JS bridge (`progress` payload) + fraction restore
- `evaluate_javascript` signature: 5 args (script, world_name, source_uri, cancellable, callback) — fixed after CI errors
- `set_data`/`data` require unsafe blocks in gtk-rs 0.9; handled via `unsafe {}` 
- CI now auto-formats and pushes fix commits (`cargo fmt --all` + push) to avoid fmt blockers in sandbox without rustfmt binary
- Clippy -D warnings enforced; dead_code allowed for some P3 structs/methods still evolving

---

## Deferred features from the reference designs  (tracked, not scheduled)

Pulled from the collected design references so nothing is lost. These are
**deferred** — they do not block any phase, and are recorded here so the plan
stays honest about what the reference imagery showed that Kalam does not yet
have.

| Feature | From | Status |
|---------|------|--------|
| Reading goal progress ring/bar on Home | Home/dashboard reference | deferred — goal pref + count exist (`finished_this_year`); no ring UI yet |
| "Up next" / Continue shelf on Home | Home reference | partially shipped (Continue row); shelf-style "Up next" peek deferred |
| Half-star rating in book grid rows | Library reference | deferred — half-stars shipped on book page only |
| Similar-from-your-shelf on book page | Book page reference | deferred to its own UI round (P5.5 next list) |
| Annotation design polish (cards, colors) | Annotation reference | deferred until reader feature work is complete (milestone order) |
| Series card on book page | Book page mockup | deliberately **not** a card — series lives in the hero + float (decision) |
| Social features (sharing, activity feed) | Analytics reference | explicitly out of scope (decision 2026-07-27) |

**Rule:** a reference image is direction, not specification. When a reference
feature is requested, the mockup-first workflow applies (see P5.5 working
method) before any Rust.

---

## Reader improvements — annotation workflow  ◀ current track

This track follows the shipped P2/P3 text reader. It keeps the work in the
order we agreed: finish the annotation workflow first, then improve anchoring,
annotation controls, and dictionary behaviour. Larger reader architecture
changes come last.

### Milestone 1 — annotation workflow  ✅ complete

A user can:

1. [x] Click an annotation in the right panel.
2. [x] Load its chapter when necessary.
3. [x] Restore the exact saved text location.
4. [x] Scroll that location into view near the top.
5. [x] Briefly emphasize the location without changing the permanent highlight.
6. [x] Edit the annotation note in an expandable multiline editor with autosave.

This uses the existing chapter index, DOM paths, start/end offsets, text
excerpt, note field, and `update_annotation_note`. The current restoration
prefers an injected annotation ID when available, then validates the existing
DOM path/offset range against the saved excerpt. If that anchor is missing or
points to different text, it falls back to matching the saved text excerpt.

Related reader work already completed:

- [x] Compact themed selection toolbar with tooltips, rounded ends, and no
      decorative pointer.
- [x] Temporary selection handles are draggable for pointer/touch input while
      preserving native selection, copy, and annotation actions.
- [x] Temporary selection bands update live while a fresh mouse/touch selection
      is being extended; handles and actions wait until pointer-up.
- [x] Default WebKit context menu suppressed without affecting text selection
      or the automatic selection-actions toolbar.
- [x] Dark rounded annotation cards with subtle pastel tints, colored left
      edges, no color dot, and improved quote presentation.
- [x] Saved note previews as note indicators.
- [x] Annotation filtering by color/type.
- [x] Existing highlights can be recolored from each card using the current
      pastel palette.
- [x] Annotation hover styling fixed so quote buttons do not add a second light
      highlight.
- [x] CI green for the current reader changes: rustfmt, Clippy with `-D
      warnings`, debug build, and release build.
- [x] Arch UX sign-off for this completed milestone.

### After milestone 1 — agreed order

1. **Hybrid anchoring**  ✅ complete
   - [x] Try the existing DOM path and start/end offsets first.
   - [x] Validate that path result against the saved text excerpt.
   - [x] Fall back to matching the saved text excerpt when that location is
         missing or points to different text.
   - [x] Keep full EPUB CFI for later; the current system was not replaced.

2. **Improve annotation controls**  ◀ current reader work
   - [x] Edit notes.
   - [x] Recolor existing highlights.
   - [x] Show note indicators through saved note previews.
   - [x] Add text search across saved highlight text and notes; keep the
         existing color/type filters.
   - [x] Improve quote/highlight presentation with the approved dark card design.

   The next isolated reader change is dictionary behavior. Annotation-control
   design polish remains deferred until the feature work is complete.

3. **Improve dictionary behavior**
   - [x] Better phrase selection: dictionary lookup preserves the selected
         phrase instead of reducing it to the first word.
   - [x] Punctuation and simple inflection handling for lookup terms.
   - [x] Multiple results, with up to five entries and separate save/copy
         actions.
   - [x] Safe formatting for dictionary text shown in the WebView popup.
   - [x] Separate bundled English idiom and expression entries, while keeping
         ordinary phrase lookup and future phrase-composition policy separate.

   Dictionary feature work is deferred for now. When it resumes, follow the
   planned dictionary overhaul below in phase order; the bundled phrase pack
   still does not make every compositional phrase meaningful automatically.
   **Phases 1–5.5 of that overhaul are shipped (precomputed headword key
   index, WordNet exception-list lemmatization, phrase decomposition,
   merged dictionary store, popup redesign, likely-sense hint; the POS
   pill shipped but POS grouping dividers remain deferred). Offline
   pronunciation (CMU Pronouncing Dictionary → IPA, Phase 5.6) is
   shipped too. The Settings reorder UI remains deferred.**

4. **Only later consider architecture changes**
   - [ ] Multi-chapter buffering.
   - [ ] Book-wide continuous scrolling.
   - [ ] Automatic chapter advance redesign.
   - [ ] Advanced CFI support.

### Reader constraints that remain locked

- Keep temporary text selection separate from permanent saved highlights.
- Do not add multi-chapter buffering until anchoring and progress behaviour are
  settled.
- Do not reintroduce an always-running background progress timer; use
  event-based or debounced persistence instead.

---

## Dictionary overhaul — planned reader-improvement track

**Status: Phases 1–10 shipped (headword key index, WordNet exception
lemmatization, phrase decomposition, merged dictionary store, popup
redesign, likely-sense hint, offline pronunciation, tap-to-look-up +
popup keyboard + find in chapter, vocabulary review + CSV/Anki export,
POS grouping dividers, dictionary priority reorder UI, lookup history).
Phase numbering is track-local — separate from the P0–P11 project
phases.** The phases are recorded here as the implementation brief for
future isolated reader-improvement steps.

### Implementation brief: Kalam dictionary overhaul

#### Context for the implementing AI

Kalam is a Rust + GTK4 + Relm4 + WebKitGTK ebook reader. Work on the branch
your session names (see "Hard rules" above). The dictionary spans three areas:

- `src/db.rs` — schema/migrations. `migrate()` uses `CREATE TABLE IF NOT EXISTS`
  plus guarded `ALTER TABLE ... ADD COLUMN` plus a `SCHEMA_VERSION` constant /
  `schema_version` table. Structs: `DictEntry { id, dict_id, word, definition }`,
  `Dictionary { id, name, lang, entry_count, added_at }`, `SavedWord`.
  `dict_entries(id, dict_id, word, definition)`;
  `saved_words(id, word, definition, dict_name, book_id, chapter_index,
  context_text, created_at)`.

- `src/db/dictionaries.rs` — `search_dict`, `search_dict_exact_or_prefix`,
  `search_dict_substring`, and query-planning helpers
  `dictionary_query_variants`, `normalize_dictionary_term`,
  `dictionary_possessive_base`, `simple_inflection_variants`.

- `src/dict.rs` — importers (`import_stardict`/`import_sqlite_pack`/`import_tsv`),
  `install_bundled_dictionaries` (WordNet + idioms shipped as
  `resources/dictionaries/*.tsv.gz` via `include_bytes!`), and
  `strip_dict_html`.

- `src/epub_book.rs` — reader WebView JS + CSS: `kalamHandleDict`,
  `showDictPopup`/`ensureDictPopup`, `window.kalamShowDict`, and the
  `#kalam-dict-popup` / `.kalam-dict-*` CSS.

- `src/pages/reader.rs` — Relm4 messages `ReaderMsg::DictSearch`,
  `DictSearchSelect`, the `dict-lookup` and `save-word` bridge handlers,
  `show_dict_in_webview(...)`, and fields `dict_lookup_word/def`, `dict_context`.

#### Conventions to follow strictly

- Migrations: add tables with `CREATE TABLE IF NOT EXISTS`, add columns with
  guarded `ALTER TABLE`, bump `SCHEMA_VERSION`, and backfill existing rows
  (users already have imported dicts + 127k-entry WordNet). Never drop/recreate
  `dict_entries`.

- All work is offline, single-process. No network calls, no new services.

- The dictionary popup is app chrome: keep it dark regardless of the reader's
  Light/Sepia/Dark paper theme. Match the existing chip:
  `border-radius: 16px`, `backdrop-filter: blur(22px)`, the current shadow, and
  `@kalam`/`--kalam-*` color variables. Do not introduce literal hex where a
  theme variable exists (see `ARCH.md` note on `style.rs`).

- Add `#[cfg(test)]` unit tests next to new pure functions (the query-planner
  already has tests — extend them).

- Verify with `cargo build` and `cargo test` after each phase.

- Do **not** rewrite unrelated code. Keep diffs scoped.

Build in the phase order below; each phase compiles and is independently useful.

#### Phase 1 — Precomputed headword index  ✅ complete

**Goal:** exact/lemma lookups hit an index instead of `LIKE` scans; kill the
`LENGTH(word)` tiebreak proxy.

- [x] `migrate()` adds `dict_entries.key TEXT` guarded by `PRAGMA table_info`
      and `CREATE INDEX idx_dict_entries_key ON dict_entries(key COLLATE NOCASE)`;
      `SCHEMA_VERSION` bumped to 11.
- [x] `fold_key(word)` in `src/db/dictionaries.rs`: lowercase, NFD + drop
      combining marks, collapse whitespace, trim surrounding non-alphanumerics
      per token — reuses `normalize_dictionary_term`.
- [x] All imports (StarDict / SQLite / TSV / bundled packs) write
      `key = fold_key(word)` at insert time via
      `insert_dict_entry` / `batch_insert_dict_entries`.
- [x] One-time backfill for pre-v11 rows: batched in 2,000-row write
      transactions, guarded by `key IS NULL` so it runs once and no-ops on
      fresh databases.
- [x] `search_dict_exact_or_prefix` matches `key = ?` (exact) then
      `key LIKE ?||'%'` (prefix), exact-first; the `LENGTH(word)` tiebreak is
      gone (prefix follows index order).

**Acceptance:** `Run`, `run`, `rún` all resolve to `run` — unit-tested with an
in-memory catalog, including an `EXPLAIN QUERY PLAN` assertion that the exact
lookup uses `idx_dict_entries_key`; existing databases upgrade in place (no
reimport, no re-download).

#### Phase 2 — Real lemmatization from WordNet data  ✅ complete

**Goal:** irregulars resolve (`went→go`, `mice→mouse`, `better→good`), with suffix
rules as fallback only.

- [x] Princeton WordNet 3.0 exception lists (`noun.exc`, `verb.exc`,
      `adj.exc`, `adv.exc`) shipped gzipped under `resources/dictionaries/`
      as `wordnet-3.0-*.exc.gz`, embedded via `include_bytes!` like the
      existing WordNet TSV. Provenance, SHA-256 checksums and the WordNet
      licence reference live in `resources/dictionaries/wordnet-3.0-exc.NOTICE.txt`.
- [x] Loaded once into a `HashMap<String, Vec<String>>` (surface → lemmas),
      lazily via `OnceLock` in `src/db/dictionaries.rs`.
- [x] `dictionary_query_variants` consults the exception map (lowercased
      surface) before `simple_inflection_variants`; the suffix rules run only
      when the surface is not in the lists. Dedup stays in
      `push_dictionary_variant`.
- [x] Tests extended: irregular variants (`went→go`, `mice→mouse`,
      `better→good`+`well`, `children’s→child`, capitalized `Went`), the
      suffix fallback (`walked→walk`), no junk stems on irregular hits
      (`better` ≠ `bett`), and end-to-end `search_dict` resolution.

**Acceptance:** `went→go`, `mice→mouse`, `better→good`, `running→run` all return
a headword (unit-tested against the shipped lists); regular cases still work.

#### Phase 3 — Phrase decomposition  ✅ complete

**Goal:** `odd mixture` yields something useful instead of a dead end.

- [x] `search_phrase(phrase, limit) -> PhraseLookup` in `db/dictionaries.rs`:
      (a) the whole phrase as a headword via the query variants (exact then
      prefix on the precomputed key); (b) the longest contained multi-word
      headword, sliding a window from longest to shortest with an exact-only
      lookup (catches `run out of steam` inside a longer selection);
      (c) per-token single-word `search_dict` results (lemmas included, so
      `went` inside a phrase still resolves to `go`). Tokens with no hits are
      omitted from the breakdown.
- [x] `PhraseLookup` enum: `Phrase(Vec<DictEntry>)` vs
      `Breakdown(Vec<(token, entries)>)` vs `Empty`.
- [x] Every reader lookup path routes through the shared phrase pipeline
      (`lookup_dict`): the selection popup's `dict-lookup` bridge and the
      sidebar Words search box both call `search_phrase` for queries with
      `>1` token, so a phrase typed in the sidebar never dead-ends either.
      A breakdown renders each token's best hit in the existing popup /
      sidebar list (capped at five / thirty) until Phase 5's popup redesign
      turns it into clickable breakdown chips. Single-word lookups are
      unchanged.
- [x] Unit tests: full-phrase headword, contained phrase headword,
      per-token breakdown, tokens without hits omitted, irregular token
      inside a phrase, and the empty case.

One deliberate deviation from the brief: step (a) uses the headword variants
(exact/prefix) rather than `search_dict`'s definition-substring fallback, so
a phrase lookup can never "match" an entry whose *definition* merely
contains the words. Substring-in-definition results stay single-word-only
until Phase 4 tiers them.

**Acceptance:** `odd mixture` (no headword) returns a breakdown for `odd` and
`mixture`; `run a risk` (if present) returns the phrase entry; single words
unchanged.

#### Phase 4 — Merged dictionary store (one word = one entry)  ✅ complete

**Goal (user decision):** one word appears exactly once, with one dictionary
speaking for it — no duplicate cards, no per-dictionary boxes, no source
clutter in the reader.

**Design (agreed):**

- **Priority, not ranking.** Every dictionary has a `priority` (lower =
  consulted first). The dictionary with the lowest priority that has the
  word provides the whole entry — every sense of *that* dictionary's word.
  Priority wins even when a lower-priority dictionary has more senses; the
  other dictionaries' copies of the word stay in the database but are never
  shown. If no dictionary has the word, the next in priority order speaks.
- **A combined store.** `combined_words` (schema v12) holds one row per
  headword key: the winning dictionary's id plus its senses as a JSON array
  (deduplicated, in entry order). The reader searches only this table, never
  the raw entries.
- **Automatic rebuild.** The store is rebuilt whenever the dictionary set
  changes — bundled install, import, removal — and once at migration for
  existing databases. Rebuild is one pass over the entries ordered by
  priority (first key seen wins) plus one bulk insert, all in one
  transaction.
- Bundled priorities: WordNet 10, Idioms 20, Synonyms 30, Antonyms 40;
  imported packs default to 100. A Settings reorder UI is deferred.

**Acceptance:** a word in both WordNet and an imported dictionary shows the
WordNet entry only; `set` with 15 senses in one dictionary and 15 in another
shows 15, not 30; removing a dictionary drops its words (or falls back to the
next in priority); identical duplicate definitions collapse to one sense.

#### Phase 5 — Popup redesign (app chrome, dark)  ✅ complete

**Goal:** fix the fake result, structure the entry, label sources. All in
`epub_book.rs` (`showDictPopup` + CSS) and the `reader.rs` handler that feeds it.

**Do:**

- Empty state: remove the `No definition found... Total dict entries: N` string
  entirely. When there's no hit, render a distinct empty-state block (not a
  `.kalam-dict-result`): a short `No entry for '{query}'.` plus, for phrases,
  the breakdown chips from Phase 3 (`[odd] [mixture]`, each clickable → re-fires
  `dict-lookup` for that token via `kalamBridge`). Also a `Search in book` action
  (Phase 6).

- Kill `RESULT n` and the fake result: replace the subheading with a quiet
  count line. The Phase 4 merged store means one entry per word, so the old
  per-dictionary tabs idea is obsolete — do not reintroduce tabs.

- Structure the entry: headword once at top. The senses now arrive numbered
  from the merged store; render them as separate lines (the popup body
  already preserves newlines). For imported HTML dicts, sanitize (allowlist
  `b/i/em/strong/br/p/ul/li/span`, drop scripts/handlers) and render instead
  of `strip_dict_html` flattening — add a `sanitize_dict_html` fn.

- One action bar: a single Save / Copy / Highlight-in-book row acting on the
  entry, instead of per-result button pairs.

- Anchor discipline: keep the existing rect-anchored placement + above/below
  flip; add a small caret pointing at the word and ensure it never overlaps
  the selection rect (nudge if it would).

- Theming: keep dark chrome on all paper themes. Reuse chip tokens (radius,
  blur, shadow, `--kalam-*`). Add a subtle border for contrast over light/sepia
  pages.

**Acceptance:** the fake `1 RESULT / No definition / Total dict entries` is
gone; one clean entry per word with numbered senses; phrase misses show
tappable word chips; imported HTML renders formatted; popup stays dark on
sepia.

#### Phase 5.5 — POS grouping + Lesk "likely sense" hint  ✅ complete

**Goal:** make senses easier to scan by grouping on part of speech, and
optionally mark the sense most likely to fit the reader's sentence — without
ever hiding a sense. Fully offline, using WordNet data already shipped in
`resources/dictionaries/`. Applies only to the bundled WordNet entries; imported
dicts render as-is.

**Hard rule for the implementing AI:** this feature may reorder or highlight
senses; it must never remove or collapse them. Every sense that matched stays
visible. A wrong guess must cost at most a misplaced highlight, never a hidden
answer.

**Do:**

- **Expose the context sentence.** The `dict-lookup` bridge already sends
  context (the surrounding text) and `reader.rs` stores it as `dict_context`.
  Ensure the full sentence — not just the selected word — reaches the ranking
  step. If tap-to-lookup (Phase 6) is present, use the sentence the tapped word
  sits in.

- **POS grouping (primary, low-risk).**

  - Parse the WordNet definition text into senses tagged by part of speech.
    WordNet glosses carry POS; if your bundled TSV flattened that, derive POS
    from the WordNet data files instead (extend the resource shipped in Phase 2
    to retain POS per sense).

  - In the popup (`showDictPopup` in `epub_book.rs`), render senses grouped
    under POS dividers — verb, noun, adjective, adverb — in that fixed order,
    numbered within each group. This is the main clarity win and carries
    essentially no risk.

  - Optional light POS preference from context: if the token is preceded by an
    article/adjective ("an odd mixture") lean noun; if by "to"/a subject
    pronoun, lean verb. Use this only to decide which POS group is shown first,
    never to drop groups. Keep the heuristic in a small pure fn with
    `#[cfg(test)]` cases.

- **Simplified Lesk soft-highlight (optional, transparent).**

  - Add a pure fn `likely_sense(context_sentence, senses) -> Option<sense_index>`
    in `db/dictionaries.rs` (or a new `wsd.rs`): lowercase + tokenize the
    context sentence into a set; for each sense, build a bag of words from its
    gloss and examples; score by set overlap (ignore stopwords — ship a tiny
    stopword list). Return the top-scoring sense index, or `None` if the best
    overlap is zero (no evidence → no hint).

  - In the popup, mark that sense with a subtle "likely here" badge/accent (use
    `--kalam-*` accent, app-chrome dark, consistent with the chip). Do not move
    it out of its POS group and do not restyle the others into looking disabled.

  - Ties or zero-overlap: show no hint rather than an arbitrary one.

- **Settings toggle.** Add a pref (via the existing `app_prefs` table / prefs
  module) `dict_sense_hint` defaulting to on, so the user can disable the Lesk
  highlight while keeping POS grouping. POS grouping itself is always on.

- **Tests.** Unit-test `likely_sense` on 2–3 hand-built cases (a sentence that
  clearly favors one gloss returns that index; a neutral sentence returns
  `None`). Unit-test the POS-preference heuristic.

**Acceptance:**

- WordNet lookups show senses grouped under POS headers, numbered within each
  group; all senses remain visible.

- With a context sentence that clearly matches one gloss, that sense gets a
  "likely here" marker and its POS group sorts first; a neutral/empty context
  produces no marker and no sense is hidden.

- Turning off `dict_sense_hint` removes the highlight but keeps POS grouping.

- No model files, no network, no new runtime dependency; `cargo build` and
  `cargo test` pass.

**Out of scope for this phase:** embedding/transformer WSD, knowledge-graph
(UKB) methods, and anything that picks a single sense and hides the rest.

### Done

- **POS pill (primary item, shipped earlier)** — the header shows the
  entry's parts of speech from the WordNet pack groups; senses are never
  collapsed.
- **Lesk "likely here" hint** — `likely_sense_index` in `db/dictionaries.rs`
  (pure fn, unit-tested): stopword-filtered content-token overlap between the
  context sentence and each sense's gloss + example. Returns `None` on zero
  overlap and on ties, so a neutral sentence shows no marker and a wrong
  guess costs at most a misplaced highlight. Two refinements keep real-world
  hints honest: the looked-up headword itself is excluded from the context
  bag (it appears in many of its own glosses and would manufacture ties —
  "deposit" is what decides the financial sense of "bank"), and both sides
  are lightly stemmed by a tiny rule-based stemmer so inflected words match
  gloss base forms ("deposits" ≈ "deposit", "running" ≈ "run").
- **Full sentence context** — the bridge already carried `context`; the popup
  now sends the whole sentence around the selection
  (`getContextSentence` in `epub_book.rs`), not just the selected word, to
  the ranking step.
- **Badge** — the hinted sense gets a small accent "likely here" pill
  (app-chrome `--kalam-*` tokens, dark on all paper themes), stays inside its
  list position, and no other sense is restyled.
- **Toggle** — reader settings → Dictionary → "Sense hint" switch persists
  `dict_sense_hint` in `app_prefs` (default on). Off removes the highlight;
  POS grouping stays always on.
- **Scope guard** — the hint is only computed for WordNet entries (senses
  carry POS) and only when a context sentence exists (sidebar searches get
  none).

#### Phase 5.6 — Offline pronunciation  ✅ done

**Goal:** the popup shows how a word is pronounced, fully offline, with no
new runtime dependency.

- [x] **Data** — CMU Pronouncing Dictionary 0.7a (BSD-style redistribution
      licence) packed as `resources/dictionaries/cmudict-0.7a.tsv.gz`:
      133,737 `word\tARPABET` rows (123,455 unique words), variants in
      counter order, compiled into the binary via `include_bytes!`.
      Provenance, licence text and the SHA-256 checksum live in
      `resources/dictionaries/cmudict-0.7a.NOTICE.txt`.
- [x] **`src/db/pronunciation.rs`** — lazy `OnceLock` parse (same pattern as
      the WordNet `.exc` lists) into a `fold_key`-keyed map; ARPABET →
      compact IPA (`B AE1 NG K` → `ˈbæŋk`, stress marks included); lookups
      fall back through `dictionary_query_variants` so inflected forms
      resolve. Unit tests cover the mapping, stress marks, case/
      diacritic-insensitive lookup and the packed table's coverage.
- [x] **Popup** — the `#kalam-pronunciation` slot (monospace, dim, hidden
      while `:empty`) is now filled by the reader payload, e.g. `bank` →
      `/ˈbæŋk/`; words absent from cmudict simply show no transcription and
      the popup layout is unchanged.
- [x] **Docs** — README feature list + this roadmap entry updated; popup
      preview regenerated with a pronunciation line.

**Acceptance:** fully offline (no network, no new crate); `bank` →
`/ˈbæŋk/`, `run` → `/ˈrʌn/`; unknown words render without a pronunciation
line; `cargo build` and `cargo test` pass.

**Deferred:** multi-pronunciation variants (first variant is shown), the
Settings reorder UI, and anything involving network pronunciation services.

#### Phase 6 — Interaction  ✅ done

**Goal:** tap-to-look-up and keyboard parity.

- [x] **Tap-a-word** — a plain click on book content (no drag-select, no
      link/image/UI node) resolves the word under the caret via
      `caretFromPoint` + `wordFromCaret` (letters, digits, apostrophes,
      hyphens; expanded to word boundaries) and fires `dict-lookup` with
      that word, its surrounding sentence (`sentenceAroundText`, shared
      with selection lookups) and a rect for anchoring. A short tap delay
      (240 ms) keeps double-click-to-select working (the pending tap is
      cancelled on the second pointer press and on `dblclick`). Tapping a
      word while the popup is open swaps the entry instead of closing it;
      tapping empty space closes it. Drag-select → phrase is unchanged.
      The `D` key with no selection re-looks-up the last tapped word,
      falling back to the sidebar dict-shortcut when there was no tap.
- [x] **Keyboard** — Esc closes the popup (existing); ↑/↓ move a sense
      focus ring (`k-def-focus` accent card, wraps around, reveals hidden
      "Show N more" senses when focus lands there); Enter saves the word
      with the *focused* sense's definition (header Save still uses the
      first sense). **Deviations from the brief:** ←/→ are deliberately
      not bound — the Phase 4 merged store removed dictionary tabs, and
      GTK reserves ←/→ for chapter navigation.
- [x] **Find in chapter** — the popup header gains a magnifier button
      (between Save and Copy). The reader has no in-book search yet, so
      per the brief this is scoped to highlighting every occurrence of the
      headword in the current chapter: `kalamSearchInBook` wraps each
      match in a temporary accent `kalam-search-hit` span (case-
      insensitive, skipping annotations/links/UI), scrolls to the first,
      and reports the count back for a toast ("N matches in this
      chapter"). Esc or the next lookup clears the hits.
- [x] **Tests** — the popup preview harness
      (`docs/files/test_kalam_dict_preview.js`, jsdom) grew to 55 checks:
      word-from-caret expansion, case-insensitive sentence context, the
      tap→bridge flow (word + sentence + rect), popup keyboard (focus
      movement, wrap, reveal-hidden, Enter-save with the focused sense),
      and find-in-chapter (wrapping, count, no double-wrap, annotation
      skip, clearing).

**Acceptance:** single tap on a word opens the popup with its sentence
context; Esc closes it; ↑/↓ + Enter work; Find in chapter highlights all
occurrences and reports the count.

#### Phase 7 — Vocabulary tools  ✅ done

**Goal:** make Saved Words more than a list.

- [x] **Schema v13** — `saved_words.known INTEGER NOT NULL DEFAULT 0` via the
      guarded `add_column_if_missing` ALTER (existing rows default to
      to-review; no backfill needed). `SCHEMA_VERSION` bumped to 13.
- [x] **Review view** — the Saved Words page
      (`src/pages/saved_words.rs`) gains a review scope: **All / To review /
      Known** radio toggles (same `group_toggles` pattern as History),
      each word row gets a **mark known** check button (accent-filled when
      known; click again to move back to review), and known rows recede
      visually (dimmed title/definition). The status line reports counts
      ("3 to review · 12 words saved · 9 known"). The reader-sidebar
      vocabulary list is unchanged.
- [x] **DB layer** — `list_saved_words(query, known: Option<bool>)` filters
      by review status (combined with search); `set_saved_word_known(id,
      known)` toggles the flag. Unit test covers the flag round-trip, both
      filter directions, toggling back, and search+filter combination.
- [x] **Exports** — **Export CSV** writes `~/SavedWords.csv`
      (`word,definition,context,dictionary,known,created_at`, RFC-4180
      quoting, definition newlines collapsed) and **Export Anki** writes
      `~/SavedWords-Anki.txt` (tab-separated `word / definition / context`
      with `#separator:tab` header — Anki's default import format), both
      mirroring the `~/Quotes.md` pattern (fixed home path + toast with
      count and path). The pure string builders are unit-tested for
      escaping and column counts.

**Acceptance:** saved words can be marked known (and back), filtered by
review status, and exported to CSV and to Anki's TSV format.

#### Phase 8 — POS grouping dividers (finishes P5.5)  ✅ complete

**Goal:** group the senses in the dictionary popup under part-of-speech
dividers instead of one flat numbered list.

**Already in place — do not rebuild:** `Sense { number, pos: Option<String>,
def, example }` and `EntryData.pos: Vec<String>` are populated by
`parse_pos_blob`/`distinct_pos` in `db/dictionaries.rs`, and `pos` already
reaches the popup as `payload.pos` (per-entry at reader.rs:1927, per-sense at
1930). This is a **rendering-only** change in `showDictPopup`
(`src/epub_book.rs`, around the `defItem` helper) plus CSS.

### Shipped

- [x] In `showDictPopup`, senses are grouped by `s.pos` with a divider row
      before each group: **noun, verb, adjective, adverb**, then remaining
      POS in first-appearance order; senses with `pos == null` go in a final
      unlabelled group with no divider. A group straddling the "Show N more"
      fold keeps a single divider at its true start.
- [x] **Flat-index invariant held:** `Sense.number` stays sequential across
      the whole entry and the flat senses array is untouched, so
      `payload.hint` and the ↑/↓ keyboard walk still index the same senses.
- [x] `.k-pos-divider` CSS rule added in the popup block (small uppercase
      label + hairline rule, `--kalam-*` variables).
- [x] The header `.k-pos` chip is dropped when there are 2+ groups and kept
      for a single group.

**Acceptance:** a multi-POS word (e.g. `run`) shows `noun` / `verb` dividers
with senses grouped beneath; sense numbers remain continuous 1..n across the
whole entry; the "likely here" badge still lands on the same sense as before;
↑/↓ still walks every sense in flat order; a single-POS word looks unchanged
apart from one divider or the retained header chip.

#### Phase 9 — Settings: dictionary priority reorder UI  ✅ complete

**Goal:** let the user order their dictionaries, so the merged store's
"which dictionary speaks for this word" is user-controllable.

**Context (verified against the code):** schema v12 added
`dictionaries.priority` (lower = consulted first, imports default 100).
`set_dictionary_priority()` and `rebuild_combined_dictionary()` exist and
work. But **`priority` is currently write-only** — it is missing from the
`Dictionary` struct (`db.rs:170` — only id, name, lang, entry_count,
added_at) and from `list_dictionaries()` (`db/dictionaries.rs:122`, which
selects no priority and orders by `name ASC`). The plumbing must be added,
not just buttons. Verified already wired: `rebuild_combined_dictionary()`
runs after import (`dict.rs:219`), bundled install (`dict.rs:95`) and
removal (`db/dictionaries.rs:170`) — so step 4 below is about *priority
changes*, not the other paths.

### Shipped

- [x] `Dictionary` gains `pub priority: i64`; `list_dictionaries()` selects it
      and orders `ORDER BY priority ASC, name ASC` — list order *is*
      effective order.
- [x] `SettingsTab::Dictionaries` rows get **↑ / ↓** buttons (reading-list
      reorder pattern). On reorder: all dictionaries are **renumbered
      compactly** (0, 1, 2, …) via `set_dictionary_priority()`, then
      **`rebuild_combined_dictionary()`** runs, then the list refreshes; ↑ is
      disabled on the first row and ↓ on the last.
- [x] The top row carries a quiet **"speaks first"** chip so the order's
      meaning is self-explanatory.

**Acceptance:** the Dictionaries tab lists packs in priority order; moving a
pack to the top makes its definition the one the reader popup shows for a
word both packs contain; the change survives a restart; importing a new pack
lands it last, not first.

#### Phase 10 — Lookup history (optional, lower priority)  ✅ complete

**Goal:** an append-only record of every dictionary lookup, separate from the
deliberate saves in Saved Words. Enables "what was that word?", surfaces
repeat lookups, and can propose study sets for the Phase 7 vocabulary tools.

### Shipped

- [x] **Schema v14** `dict_lookups(id, word, book_id → books ON DELETE
      SET NULL, chapter_index, context_text, found INTEGER NOT NULL, at
      TEXT)` + `idx_dict_lookups_word` (NOCASE) and `idx_dict_lookups_at`.
- [x] **Misses and hits are both logged** (`found = 0` when the entry has no
      senses). Logging lives in `lookup_dict` in `src/pages/reader.rs` — the
      funnel both the `"dict-lookup"` selection/tap path and the sidebar
      search go through — so sidebar lookups are recorded too (a literal
      `lookup_dict`-only pin was rejected: it silently skips sidebar
      searches). Per-keystroke search prefixes are **not** logged.
- [x] **Repeat collapse:** the insert is skipped when the same word (NOCASE)
      + same `IFNULL(book_id, -1)` was logged within the same ISO hour — the
      `reading_events` pattern, so flipping back to a word doesn't flood the
      log.
- [x] Pref `dict_history_enabled`, default `1`, wired like `dict_sense_hint`
      with a **Lookup history toggle in reader Settings**.
- [x] **Lookup History page** (Library section): day-grouped list, search
      filter, "not found" chip, **Clear history** button (with outcome
      notification), 500-row read limit.
- [x] `repeat_lookup_words(limit)` — words looked up more than once, most
      frequent first — feeds **"Suggest from history"** on the Saved Words
      page (word ×N chips) and a **"Last 3 lookups" dashboard card** on
      Library with a miss badge.

**Privacy requirement (shipped with the feature):** the reader Settings
toggle stops all writes, and Clear empties the table — both landed in the
same phase, not after.

**Acceptance (CI-verified, 156 unit tests green):** looking up a word writes
one row; looking it up again immediately writes none; looking up a word with
no definition writes a row with `found = 0`; disabling the pref stops all
writes; clear empties the table; the day-grouped list renders and the
repeat-lookups query returns sensible counts.


## P4 — Library depth  ✅ done

**Goal:** Home, shelves, lists feel like *your* library.

### Shipped

- [x] **Schema v4**: `shelves`, `shelf_books`, `reading_list`,
      `reading_events`, `reading_sessions`, plus `books.last_opened_at` /
      `books.finished_at` added via an idempotent `ALTER` helper
- [x] **Shelves engine**
  - Manual shelf (membership rows, hand-sortable with ↑/↓)
  - Smart shelf — **flat rule list + one All/Any switch** (decision B)
  - Fields: tag, author, series, format, progress, title, added
  - Operators: is / is not / contains / does not contain / in the last N
    days / more than N days ago
  - Rules stored as JSON, compiled to a parameterised SQL `WHERE`
  - Shelves grid: 2 columns, kind badge, live counts, rule summary
  - Rule editor with **live “N books match”** readout
- [x] **Reading list** — ordered TBR, ↑/↓ reorder, bulk picker, Read button
- [x] **History** — append-only event log (opened / finished / unfinished /
      imported), grouped by day, filterable, same-hour dedupe on opens
- [x] **Reading time tracking** — a `reading_sessions` row per reader visit,
      closed on shutdown, clamped at 6h so an idle window can't fake a marathon
- [x] **Tags browse** — usage-weighted tag cloud → per-tag book grid
- [x] **Analytics** — books/finished/reading/unread, time read (all time,
      7d, 30d), current & longest streak, highlights/quotes/words, 14-day
      reading bar chart, books-added-per-month, most read / top tags / top authors
- [x] **Home polish** — counts strip, multi-book Continue row driven by
      `last_opened_at`, Up-next peek from the reading list
- [x] **Book page** — add/remove reading list, Mark finished / Mark unread,
      manual-shelf checklist, shelf chips
- [x] Auto-finish at ≥99% progress (once per book), with manual override
- [x] Unit tests over an in-memory catalog: rule compilation, membership,
      reorder, auto-finish, session clamping, stats

### Design decisions locked in P4

- Smart shelves stay **flat** (no nested boolean groups). The stored JSON is
  forward compatible, so nested groups can arrive later without a migration.
- An empty smart shelf matches **nothing**, not everything — less surprising.
- Analytics never invents data: no "hours read" before sessions existed.
- Deleting a shelf never deletes books.

### Out

Online sources.

### Arch check

- Create a smart shelf (tag is X **and** progress is unread) → live count moves
  as you type → save → grid shows the count → open it
- Create a manual shelf → add books from the book page and the picker → reorder
- Read a book for a few minutes → History shows "Opened" → Analytics shows time
- Finish a book → it leaves the reading list and appears as Finished

---

## P5 — Metadata  ✅ done

**Goal:** Fix messy imports without leaving Kalam.

### Shipped

- [x] Edit metadata dialog: title, authors, series, tags, description
- [x] Replace cover from disk (PNG/JPEG/WebP/GIF, sniffed by magic bytes)
- [x] **Open Library** lookup: search, pick a candidate, pull description
      and cover
- [x] User-triggered only; results are staged into the form for review and
      nothing is written until you press Save
- [x] Sparse matches only fill fields they actually have, so a thin result
      cannot blank out good local metadata
- [x] Network on worker threads via async-channel — the dialog never blocks
- [x] uuid paths unchanged; covers get a fresh file name per replacement
- [x] Unit tests over captured Open Library payloads

### Notes

- `ureq` with rustls, so there is no OpenSSL system dependency to install
- Covers are written as `cover-<n>.<ext>` rather than overwritten: GTK caches
  textures by path, so reuse would show the old image until restart
- Open Library's `description` is sometimes a string and sometimes
  `{ "value": … }`; both are handled

### Out

Bulk metadata edit and cover refresh across many books — those live in P11.

---

## P6.5 — Libraries: choose where books live, and have more than one  ✅ done

**Goal:** Calibre-style libraries. You pick the folder. You can have several,
completely separate from each other, and you switch between them. Copy the
folder to another machine and it opens there.

**Decided 2026-09-04. This is the next phase.** It was ordered before P7 because
P7 *adds books from new places* while this changes *where books live*, and doing
both at once means a missing fic could be either one's fault. P7 has since moved
behind the comics phases, so the gap is wider still.

### Why this is not shelves

Calibre has two features and Kalam had conflated them:

| Calibre | What it is | Kalam |
|---|---|---|
| **Library** | Separate folder, separate database. Books in one are invisible in the other. | ✗ this phase |
| **Virtual library** | A saved filter over one library. A view. | ✓ our shelves |

Our shelves — manual and rule-based — are Calibre's *virtual* libraries and
stay exactly as they are. What is missing is the hard wall: fanfiction in one
library, technical books in another, genuinely absent from each other. A shelf
cannot do that because a shelf is a view over everything.

**Both exist afterwards.** Libraries separate; shelves organise within one.

### The good news, from reading the code first

- **Nothing machine-specific is stored.** `books` holds `file_name`
  ("book.epub") and `cover_name` ("cover.jpg") — plain names, never absolute
  paths, which are rebuilt at read time. A Kalam folder is *already* portable;
  there is just no way to point the app at one.
- **Every path derives from one function**, `data_dir()` in `src/paths.rs`. So
  "let the user choose" is a small change at the root, not a sweep.

### Progress

- ✅ **The registry and the switch** (`src/libraries.rs`, 2026-09-04).
  `~/.config/kalam/libraries.json` lists every library and which is open;
  `paths::data_dir()` reads it and falls back to the old fixed location when
  nothing is chosen, so **an existing install is untouched until the user asks
  for something else**. Written temp-file-then-rename, since a truncated
  registry would lose the record of where every library is. 10 tests,
  concentrated on the index arithmetic: `active` is an index, so forgetting an
  earlier entry shifts it, and an off-by-one there silently opens the wrong
  library.
- ✅ **Settings → Storage → Libraries**: lists known libraries, which is open,
  Add / Open / Forget. Forget removes the entry only and says so plainly —
  "forget" and "delete my library" must never be confusable.
- ✅ **The per-library / global split** (2026-09-04). `app_prefs` lives in
  `catalog.db`, i.e. *inside* a library — so switching would have silently
  reset the theme, reader font size, dictionary settings and API keys, because
  the new library's database has never heard of them. App-level prefs now
  mirror to `~/.config/kalam/prefs.json` and are read from there first, falling
  back to the library so existing installs carry across. **Unknown keys default
  to per-library**, deliberately: a global pref that should have been
  per-library silently applies one library's value to another, which reads as
  corruption, whereas the reverse is just "set it again". Global prefs are
  inert under `cargo test` — see pitfall §24.
- ✅ **Dictionaries kept out of libraries** (2026-09-04). `dictionaries_dir()`
  hung off `data_dir()`, which is now the *active library* — so switching would
  have found no dictionaries and reinstalled the bundled packs, about 4 MB and
  a few seconds, **once per library, for ever**. They now live under a shared
  root; the path is unchanged for anyone who never switches. Author photos and
  series covers deliberately stay per-library: they are caches of *this*
  library's authors, deleting a library should take them with it, and a shared
  folder would accumulate entries for authors nobody owns with nothing to prune
  it. Both are caches, so that call is cheap to revisit.
- ✅ **The existing library is adopted into the list on first run.** The
  fallback already made it *work*, but it left the user's own books as the one
  library that never appears in Settings — Open and Forget would apply to every
  library except theirs, and adding a second would make the first seem to
  vanish. Nothing is moved or copied; only the list learns about the folder. A
  folder counts as a library if it contains `catalog.db`, deliberately not "the
  folder is non-empty": picking `~/Documents` by mistake must not read as an
  existing library, and picking an empty folder is how you start a new one.
  Add-library now says which of the two happened.
- ✅ **Switching restarts the app** (2026-09-04). Selecting a library used to
  take effect "next launch", which is an odd thing to ask of someone who just
  clicked Open. It now confirms, saves the choice, then re-execs. `exec` rather
  than spawn-then-quit, so there is never a second Kalam alive with the same
  catalog open. A restart rather than switching in place because `data_dir()`
  is cached for the process and pages hold open database handles — repointing
  mid-session would leave some of them reading the old library and writing the
  new one, which corrupts rather than merely looking wrong. The choice is saved
  *before* the restart; the other order would reopen the old library and look
  like the click did nothing.
- ✅ **`kalam.json` beside every book** (`src/sidecar.rs`). Title, authors,
  series, tags, rating, reading position and highlights — including each
  highlight's text, which is the field that lets it be found again after a file
  changes. **A backup, never the truth:** the database stays authoritative and
  these are never read during normal operation, so cross-book screens stay one
  query and there is no question of which copy wins. Written on import, on
  metadata edits and when a highlight is added; best-effort throughout, since a
  stale backup deserves a log line and never a failed edit. Stores *names*, not
  paths, or the folder would stop being portable the moment it moved.
- ✅ **A recovery card in Settings.** Shows how many books have a backup copy
  and writes the missing ones. This exists because "your library describes
  itself" is worth nothing unless it is checkable — the sidecars are written on
  code paths that could quietly stop running, and the failure would be
  invisible until the day someone needed them (§19).

### Still open

- Nothing blocking. Possible follow-ups when wanted: an actual
  rebuild-from-folders command (the survey proves the data is there; nothing
  consumes it yet), and moving a library's folder from inside the app rather
  than by hand.

**A note on how this went — eight red runs, three separate causes.** Worth
reading before the next phase, because only one of the three was about the code
being wrong.

1. **Dead code** (2 runs). `-D warnings` rejected the registry's methods for
   having no caller — exactly the rule `source-seam.md` §12 gives for the
   `Source` trait. Fixed by building the settings card rather than silencing
   the lint, which is the right pressure: no API lands here without a user.
2. **rustfmt failing the build** (5 runs). The step auto-committed and pushed,
   the push kept being rejected, and a rejected push failed the whole run — so
   a *formatting* step masked clippy, the tests and the build entirely. The
   real bug was hiding behind it the whole time: my script for appending tests
   cuts at the file's last `}`, which had stopped being the test module's once
   functions were added after it, so 70 lines of `#[test]` were spliced **inside
   `set_global_pref`'s body**. Braces balanced, so nothing short of a parser
   could see it. rustfmt is now report-only and cannot fail a run
   (pitfall §23).
3. **A test that read the developer's own home directory** (1 run). Making
   `dict_history_enabled` global meant a pre-existing service test suddenly
   consulted `~/.config/kalam/prefs.json`, so its result depended on the
   machine rather than the code (pitfall §24).

### Scope

- Pick a library folder; create, open and switch between several
- A small config outside the libraries (`~/.config/kalam/`) holding the library
  list and which was last open. It has to live outside — a setting stored *in*
  a library cannot be read before the library is found. This is the one
  machine-specific thing, correctly so: it is about this machine, not the books
- **Split per-library from global.** Books, covers, highlights, shelves,
  reading history belong to a library. Dictionaries, theme and preferences must
  stay global or you reinstall dictionaries per library. Getting this wrong is
  painful to undo
- Migrate the existing fixed-location library into the new scheme as "your
  first library"; offer to move it. Copy-verify-delete, never move-and-hope
- **The reader's unpacked-EPUB cache must NOT travel** with a library — it is
  throwaway and should rebuild on the new machine
- `kalam.json` per book folder: a **backup copy** of that book's metadata
  (title, author, tags, highlights, progress, source). Database stays the
  source of truth and is never read from these during normal use — on conflict
  the database wins. Makes a library self-describing and rebuildable if
  `catalog.db` is ever lost. JSON not OPF: OPF is an ebook-packaging format and
  cannot express highlights or reading sessions without abuse, and nothing else
  reads a stray `metadata.opf` anyway
- A "rebuild database from folders" command, which is what makes the backup
  copies worth writing

### Out

- Sharing one library between two machines at the same time (sync/locking).
  Copying a folder is in scope; two apps writing at once is not.

Full discussion: [`docs/libraries-and-portability.md`](./docs/archive/libraries-and-portability.md).

---

## P8 — Comics local + Moku-style reader ✅ complete (2026-09-05)

**Goal:** Local CBZ/CBR with a dedicated comics viewer.

### Scope

- Import CBZ/CBR into same catalog (`format = cbz|cbr`)  
- **Comics reader UI (Moku reference — locked):**  
  - Black immersive stage, art centered  
  - Top bar: close, chapter/title, page `i / N`, zoom %  
  - Bottom: scrubber, zoom, prev/next page  
  - Page mode LTR/RTL; webtoon long-strip mode  
  - Fit width / fit height  
  - Tap center toggle chrome (optional)  
- Memory-safe decode (viewport ± neighbors only)  
- Progress per book  
- Book page / float: **Read** routes to comics viewer when format is comic  

### Out

Remote catalogues (P9).

### Arch check

Open large CBZ, scrub pages, zoom, RTL, quit/restore page; RAM stays reasonable.

---

## P9 — Manga platform (Suwayomi-class)

**Goal:** Browse/download manga into library → open in the P8 comics viewer.

### Scope

- **The same `Source` trait as P7**, with `ContentKind::Images` — see
  [`docs/source-seam.md`](./docs/archive/source-seam.md) §2. Same adapter shape as
  Tachiyomi/Suwayomi extensions: search, chapter list, page fetch  
- **No Suwayomi server rewrite:** Suwayomi's value is its Kotlin extension
  ecosystem, which can't run in Rust; we reimplement the *adapter concept*
  natively (porting an extension's scraping logic is hours — they are simple
  scrapers). Optional later: a "Suwayomi server" adapter so Kalam can talk
  to a user's existing Suwayomi instance via its API — the cheapest bridge
  to the whole ecosystem  
- Sources: MangaDex, Komga, Kavita, own archive, OPDS, … (legal /
  self-hosted first)  
- Downloads hub integration; per-source rate limits  
- Reader is an **image pager** (P8) — no WebKit involved  

### Policy

Only sources you’re allowed to use. No unauthorized scraper assistance.

---

## P6 + P7 — Downloads hub and the fiction client  ◀ done together

**Combined 2026-09-04, at the user's request.** They were always going to
collide: a downloads queue with nothing to download is a shell, and a fiction
client that fetches things needs somewhere for those jobs to live. Building
them apart would have meant designing the queue against imagined callers and
then reworking it once real ones arrived.

The two sections below stay separate because they are still separate bodies of
work — the queue is infrastructure, the client is the feature — but they ship
as one phase and the queue's design answers to P7's real needs rather than to a
guess.

---

### P6 — Downloads hub

**Goal:** One place for inbound files/jobs.

### Scope

- Queue: queued / active / done / failed  
- Sidebar Downloads UI  
- Folder-watch import  
- Hooks for AO3/comics jobs  

---

### P7 — Fiction platform (AO3 first, then more)

> **Reordered 2026-09-04, at the user's request:** *"push this step back at the
> very last. this is a complex step and I am still working things out."* P7 now
> runs **after the comics phases (P8, P9)** rather than before them. Two
> reasons, and the second is the better one: the design is still moving, and
> the `Source` trait gets proven on comics first — which is the flavour it has
> never been tested against, since MangaDex was also moved into the comics
> phase. Discussion continues in the meantime; nothing here is frozen.

**Goal:** A FanFiction.net-app-class fiction **client** inside Kalam — not a
downloader. Browsing, surfing and exploring the sites from inside the app:
category and fandom browsing, the site's own sort orders, author pages (their
works, favourites, follows, profile), search with the site's full filters, your
own favourites and follows, series and collections. Downloading, offline
reading and **auto-update** of followed fics are things you can do while you
are in there — not the point of being there.

> **Scope correction, 2026-09-04.** This goal was previously written as
> download-and-read, and the plan built from it was a downloader. The user
> stopped it: *"not just a downloader, but surfing, exploring, etc."* The tell
> was in `source-seam.md` §1 — its four verbs (search → detail → chapters →
> content) all begin from "I already know which work I want", which is a
> downloader's shape and cannot express wandering. See
> [`docs/p7-scope-correction.md`](./docs/archive/p7-scope-correction.md). **The `Source`
> trait needs rewriting before any of it is built**, because retrofitting a
> browse model onto a download-shaped API means changing every source and every
> screen.

**AO3, FanFiction.net and Literotica get full browsing**; other sources may
support less, and the trait must let a source say "I do search but not author
pages" with the UI adapting rather than breaking.

### Split into stages (agreed 2026-09-04)

P7 is several phases of work, not one. Each stage ends somewhere usable, so we
can stop, reorder or change course without leaving a half-built thing.

- **P7a — the trait, browse and search.** Rewrite `source-seam.md`'s verbs into
  a browsing shape *first*; land them with AO3. Category/fandom browsing, the
  site's sort orders, search with AO3's full filter set in our own
  presentation. Ends with: you can wander AO3 inside Kalam.
- **P7b — reading online.** Open a fic and read it per-chapter without adding
  it to your library; **Save** keeps it in the temp area, **Download** promotes
  it to a real book. Ends with: Kalam is usable as a reading client.
- **P7c — author pages and sideways links.** Their works, favourites, follows,
  profile; series and collections. Ends with: you can follow a trail rather
  than only search.
- **P7d — the other sources.** Royal Road (first EPUB we build ourselves),
  Literotica, then FFN (WebKit browsing + FicHub download). Ends with: the
  trait is proven against four genuinely different sites.
- **P7e — accounts, if wanted.** Site-side favourites/follows/history and
  registered-only works. Deliberately last: everything above works logged out,
  and doing it later means deciding with evidence about whether it is missed.
  See [`docs/p7-login-and-reading.md`](./docs/archive/p7-login-and-reading.md).
- **P7f — download, follow and auto-update.** The original P7 plan, now the
  *end* rather than the whole thing.

### Reading online (decided 2026-09-04)

**Read-online first**, download second, and **per-chapter** rather than
whole-work. Three explicit tiers, decided by the user:

| What you did | What is kept |
|---|---|
| Just reading / browsing | nothing lasting |
| **Save** | kept in the temp area, *not* a library book |
| **Download** | a real book in your library |

**Per-chapter fetching, not whole-work.** Required rather than merely nicer: a
2,000-chapter Royal Road serial cannot be downloaded to read one chapter. It
also makes opening anything feel instant, and it is what makes the casual tier
cost almost nothing. **Consequence: the trait needs a fetch-one-chapter verb
even though AO3 never uses it** — AO3 hands over a whole EPUB, so *AO3 is the
unusual case*, not the template. Designing the trait around AO3 alone would
produce the wrong shape.

**An earlier suggestion of mine was withdrawn.** I proposed keeping every
temporary cache for a few days with a size cap, instead of the user's
delete-on-close. They rejected it in favour of the explicit Save tier above,
and were right: time-and-size pruning means the *app* decides what to keep,
disk usage moves on its own, and "why is this still here / why did it vanish"
has no answer a user can predict. An explicit Save is a decision you made and
can see. It also dissolved the problem I was solving — my worry was losing a
big download by closing the app, which only bites if reading requires
downloading the whole work up front, which per-chapter reading removes.

**Still open:** what exactly separates **Save** from **Download**. Both keep
the fic. Candidates: Save keeps it readable without cluttering the library,
Download makes it a full book with highlights, progress, shelves and
auto-update; or Save is a bookmark that happens to keep the text. Worth pinning
down before either is built — the answer decides whether saved fics need their
own screen.

This costs less than it sounds: the reader **already** works this way. It never
reads an EPUB directly — `EpubBook::open(epub, cache_dir)` unpacks into
`cache/reader/<uuid>/` and reads the unpacked files, and `prune_reader_cache`
already sweeps old extractions at startup. So the reader does not care whether
the EPUB came from disk or the network. What is missing is a temporary identity
for a fic that is not a library book, and a promote action.

Two refinements on the original idea:

- **Do not delete on app close.** Keep temporary caches a few days or until a
  size cap, whichever comes first, so closing the app and returning an hour
  later does not re-download. The pruning machinery already exists.
- **Very long serials cannot use this.** Downloading a whole 2,000-chapter
  Royal Road work to read one chapter is not viable, so those need per-chapter
  fetching — which is why the trait still needs a chapter-content verb even
  though AO3 never uses it.

### Out (P7)

- **Writing actions of any kind** (decided 2026-09-04): no kudos, no
  bookmarking on the site, no posting or reading reviews/comments. This keeps
  the app read-only against every source, which also means a leaked session
  cannot be used through Kalam to damage an account.

### Scope

- **One `Source` trait, not a separate `FictionSource`** — see
  [`docs/source-seam.md`](./docs/archive/source-seam.md) §2. Fiction and manga differ
  only in the final step (text vs image URLs), which is a two-variant
  `Content` enum; searching, pagination, chapter lists, rate limits, the
  download queue and the follow scheduler are identical and must not be
  written twice. AO3 lands native first to prove the trait, then scrapers
  move to Lua plugins — `source-seam.md` §9a, §11
- **Sources, decided 2026-09-04: AO3 → Royal Road → Literotica → FFN.**
  Chosen so each proves something different rather than repeating the last:

  | Source | How the text arrives | What it proves |
  |---|---|---|
  | AO3 | their own EPUB endpoint | search, filters, following |
  | Royal Road | we parse and build the EPUB | the assembler, real parsing |
  | Literotica | we parse and build the EPUB | messy structure, no clean chapters |
  | FanFiction.net | the FicHub API | using a third-party bridge |

  **AO3 needs no text parsing at all** — `download.archiveofourown.org/downloads/<id>/fic.epub`
  is a real EPUB, built by AO3 with Calibre and listed on their own FAQ. So
  scraping AO3 is only for *finding* things. That is why AO3 alone would prove
  nothing about parsing, and why the other three matter.
  **Royal Road is second on purpose**: it is the first source where we build an
  EPUB ourselves, which is the biggest untested piece, and it is a gentle place
  to get that wrong.
  **Webnovel dropped** (2026-09-04): almost everything worth reading is behind
  their coin paywall, so a downloader gets a handful of free chapters and
  stops. Bypassing a paywall is out of scope.
- **One shared EPUB assembler**, not one per source: a source returns chapter
  text, one common builder turns chapters into an EPUB. Otherwise every scraper
  reinvents it slightly differently. This is new code — `epub_write.rs` *edits*
  existing EPUBs and cannot create one — but `zip` is already compiled with
  write support, so it is contained
- **FFN goes through [FicHub](https://fichub.net/api), not scraping.** FFN sits
  behind Cloudflare and the established tool (FanFicFare) effectively gave up
  on it. FicHub has a documented public API returning metadata plus a ready
  EPUB, and deals with Cloudflare on their side. Their rules are conditions of
  use, not suggestions: identify ourselves in the user-agent with contact info,
  **never** issue concurrent requests, honour `429`/`Retry-After`, and no bulk
  export. The dependency must be visible in the UI — if FicHub is down, FFN
  stops working, and the user should know why. See
  [`docs/fichub-and-ffn.md`](./docs/archive/fichub-and-ffn.md)
- **The WebKit-as-fetcher idea is BACK IN SCOPE** (un-deferred 2026-09-04, the
  same day it was deferred). Deferring it assumed FicHub solved FFN. It does
  not: FicHub has exactly two endpoints, `/api/v0/epub` and `/api/v0/meta`, and
  **both require a fic URL you already have**. There is no search, no category
  browse, no author page. So FicHub solves *downloading* from FFN and does
  nothing at all for *browsing* it — which is most of what this phase is.
  Browsing FFN means fetching FFN pages, which means Cloudflare, which means
  the browser engine we already ship. Likely both: WebKit for browsing, FicHub
  for the download once a fic is chosen, since they already handle multi-chapter
  assembly
- **Login is in-scope but LAST (P7e), and password storage is ruled out.**
  `source-seam.md` §13 parked it as "probably out of scope until someone asks";
  someone asked, since site-side favourites, follows, history and
  registered-only works need an account. But everything else in this phase
  works logged out, so it goes at the end where it can be decided with
  evidence. **AO3's own position matters here** — their mobile-apps post says
  plainly that if a third-party app asks for your AO3 login you are providing
  it *at your own risk*, and r/AO3 auto-replies with that link on every app
  question. It is a warning, not a ban (AO3 leaves unofficial apps alone unless
  they impersonate AO3) — but **we are that third party**, and there is no
  sanctioned route: AO3 still has no public API, thirteen years after saying
  one was "several major releases away". So: **never store a password.** If
  accounts happen, log in through a real AO3 page in a WebKit window, keep only
  the session cookie, store it in config (never inside a library folder, or it
  would travel with copied books), make log-out actually delete it, and keep
  the whole thing read-only. Note also that Kalam's *own* follows already work
  across all four sources without any account, which is a better feature than
  the site-side list it would replace. Full discussion:
  [`docs/p7-login-and-reading.md`](./docs/archive/p7-login-and-reading.md)
- **Structured search UI** (native): fandom, tags, characters, ships,
  rating, status — fed by plugin-parsed results (AO3 has no public API;
  plugins parse the site, Tachiyomi-style)
- Download into library with `source` + `remote_id`; offline reading
- **Follow + automatic updater:** background scheduler (A0 task manager +
  glib timers) polls followed fics, downloads new chapters, notifies
  (replaces "Manual Check updates")
- Rate limits / clear errors / polite polling (respect sites)
- **Highlights survive an update** (decided 2026-09-04, replaces the old
  "best-effort, may lose anchors" caveat). Annotations already store
  `text_excerpt`, the highlighted words themselves. Positions break when a file
  changes; words do not. So on replace, re-find each highlight by searching the
  new chapter for its saved text; anything that cannot be placed is **kept and
  reported, never deleted**. No storage change and no migration — the data is
  already there. AO3 appends chapters at the end, so most highlights re-anchor
  trivially. See [`docs/p7-storage.md`](./docs/archive/p7-storage.md)
- **`source` + `remote_id` columns on `books`.** Today "do I have this?" is
  answered by hashing the file, so a fic that gained a chapter looks like a
  different book and imports as a **second copy**. Recording where a book came
  from makes it a lookup, and lets the updater replace in place while keeping
  the book's id, reading position, highlights and shelves. `source-seam.md` §6
  already calls `remote_id` load-bearing
- **Search: all of AO3's filters, presented our way** (decided 2026-09-04).
  Source-declared filters in the trait, since AO3's set is too large and too
  specific to hard-code a common subset — but the screen is Kalam's own design,
  not a generic widget dump of their search page
- MangaDex is **not** in P7 — it moves to the comics phase (decided
  2026-09-04). So P7 is text-only, and the image half of the two-variant
  `Content` enum stays unimplemented until then, which by the `source-seam.md`
  §12 dead-code rule means it cannot land as code during this phase

### Out

Piracy sources.

---

## P10 — PDF (text-reader family)

**Goal:** Read PDFs with light marks.

### Scope

- MuPDF (or Poppler) view inside **text-reader chrome family** (not comics shell)  
- Continuous or page mode  
- Basic highlight/underline stored like EPUB annotations where possible  
- Same library entry model  

---

## P11 — Tools

**Goal:** Occasional Calibre-class jobs.

### Scope

- Convert via external `ebook-convert` / `pandoc` if present  
- EPUB polish (strip junk CSS, etc.)  
- Batch metadata / cover refresh  
- Optional Calibre `metadata.db` one-shot import  

---

## P12 — WebAssembly (Wasm) plugin system (+ built-in Rust seams)

> **Yes, there is a Lua plugin system.** It covers the surfaces that **break
> when someone else changes their website**: content sources (AO3, FFN, scraped
> manga) and add-on metadata providers. The built-ins that ship with the app —
> Open Library, Google Books, MangaDex, themes, export formats — stay compiled
> Rust. Full reasoning: [`docs/source-seam.md`](./docs/archive/source-seam.md) §9a.

**Status: settled 2026-09-03 after two wrong turns** (recorded in the changelog
because the reasoning matters). Not an ecosystem — no marketplace, no
third-party repo, no API-stability promises. A plugin system **for us**, so a
broken scraper is a one-line edit and a restart instead of a full fat-LTO
rebuild.

**The dividing line is not "source vs. other". It is "does this rot?"**

| | Rots? | Implementation |
| --- | --- | --- |
| Open Library, Google Books | rarely — documented JSON APIs | built-in Rust (**already shipping**) |
| MangaDex, Komga, Kavita, OPDS | rarely — official APIs | built-in Rust |
| Themes, export formats, dictionaries | never — pure data | built-in Rust (themes **already shipping**) |
| AO3, FFN, Royal Road, Webnovel | **often** — HTML scraping | **Lua** |
| Scraped manga sites | **often** | **Lua** |
| Goodreads, StoryGraph, Kobo, regional metadata sites | **often** — scraping, no public API | **Lua** |

**The evidence, which is what settled it.** Calibre ships a handful of metadata
sources built in and has **20+ third-party metadata plugins** in its index —
Goodreads, Amazon, Kobo, StoryGraph, FictionDB, ISFDB, Douban, DNB, Baen,
Barnes & Noble, noosfere, moly.hu, databazeknih.cz, Skoob, Bookline, Lira,
Alexandra, Biblioman, Kitapyurdu, SF-Leihbuch. The tail is regional and niche,
exactly what a built-in list cannot serve, and **almost all of them are
scrapers** with changelogs full of "fixed for site change". Tachiyomi says the
same thing more bluntly: *"Extensions are parsers. If a website changes its
structure, the extension breaks. The core app stays stable; extensions change
constantly."*

**Why a rebuild is the wrong fix loop here.** `[profile.release]` is
`lto = true` + `codegen-units = 1` over 44k lines and 36 dependencies — the
slowest rebuild configuration there is, and on a 4 GB machine fat LTO is the
setting most at risk of an OOM kill. Changing one CSS selector re-links the
entire binary. Fine once for a stable API; wrong every few weeks for a scraper.

**Sequenced, not skipped.** AO3 lands **native first** so the trait is
extracted from working code rather than guessed, then the Lua host follows with
AO3 ported as its proof. See `source-seam.md` §11.

**Goal:** a plugin surface for content sources and add-on metadata providers.
One host, one sandbox, one loader, serving both `Source` and `MetadataSource`.

### Scope (design points, refine in conversation)

- **Language: Lua** via `mlua` — tiny, embeddable, battle-tested (Yazi,
  Neovim, AwesomeWM). WebAssembly set aside (heavy tooling). **Compiled-in
  Rust is not an alternative but a complement** — it is what the stable
  built-ins use.
- Plugin API surface: `search(query, filters) → results`, `details(url)`,
  `chapters(url) → list`, `content(chapter) → clean text` (fiction) or
  `pages(chapter) → image URLs` (manga). A metadata plugin implements
  `search` alone.
- **`mlua`'s `send` feature stays OFF.** The VM is `!Send`; a `SourceFactory`
  (path + manifest, trivially `Send`) crosses to the worker and builds the VM
  there, so it is born and dies on one thread. Turning the feature on would
  buy a reentrant mutex on every VM access for a problem we do not have
  (`source-seam.md` §9).
- **No Kotlin-extension bridge** (Tachiyomi extensions are Android APKs —
  wrong shape). **No Suwayomi server rewrite** — optional later: a "Suwayomi
  server" *client* adapter that talks to a user's existing instance.
- Sandbox: HTTP through the host's rate-limited agent, an HTML selector, a
  JSON decoder, a logger. **No filesystem, no catalog, no sockets, no
  processes.** A bad plugin yields wrong results, never a corrupted library.
- Rate limits / polite polling: per-source schedules (user-controlled, default
  daily), **declared by the plugin and enforced by the host** — a plugin
  cannot be trusted to sleep.
- **Borrow:** FanFicFare adapter logic (AO3/FFN/RoyalRoad/…) ports to Lua
  plugins; MangaDex's official API needs no scraping.

### Out

Running Tachiyomi/Suwayomi Kotlin extensions. A plugin marketplace (later,
if ever).

## P5.5 — UI overhaul  ◀ LAST, after everything else

> **Moved to the end 2026-09-04, at the user's request:** *"push UI overhaul to
> absolute last, we will makeover the UI at last when everything is ready."*
> The work already done (colour system, 13 themes, Settings v2, book page,
> series float) stays shipped — this is about the *remaining* screens.
>
> It is the right call. Every phase left adds screens: a downloads queue, a
> browsing client, author pages, a comics pager. Restyling Home and Library
> now would mean restyling them again once those exist, and a design settled
> before its content is a guess. Doing it last means one pass over a known set
> of screens instead of several passes over a moving one.

**Goal:** Redesign the interface, one window at a time. The app grew screen by
screen and looks it; this is the pass that makes it feel like one product.

**Working method (agreed):** one window per round. The agent mocks the screen
up as an image first, the user looks at it, and only then does it become Rust.
The agent cannot see the GUI, and shipping layout blind has repeatedly wasted
rounds.

### Done
- **Colour system** — `src/theme.rs` owns every colour; `style.rs` holds only
  shape (padding, radii, type scale). Adding a theme is one struct.
- **13 dark themes**, grouped standard + darker per family: One Dark (default
  is One Dark Darker), Tokyo Night, Everforest, Catppuccin, Gruvbox, Ayu, and
  Nord (no darker variant). Light themes are out of scope.
- **Scrollbars** — invisible until hovered, thin pill, hugging the edge.
- **Sidebar** — 48px icon-only rail, logo pinned top, nav centred, Settings
  bottom, circular active state.
- **Logo** — `assets/logo.png`, embedded with `include_bytes!`.
- **Settings (`src/pages/settings.rs`)** — redesigned into a two-column layout: a 220px navigation rail with 6 categories (Appearance, Storage & Backup, Dictionaries, Book Files, Metadata Sources, Notifications) and rounded cards (`kalam-card`) grouping individual setting rows.
- **Settings v2** — rebuilt again in the P5.5 design language from the user's
  `settings.html` reference, reconciled against shipped features: grouped nav
  rail (Appearance / Library / Sources / App overlines), section cards
  (accent glyph + title + description + divider + rows), hairline-separated
  setting rows, Material-You-style pill switches for every real pref
  (EPUB writeback, Open Library, Google Books), theme picker grouped by family
  with per-variant cards (mini UI preview + swatch strip + ✓ seal on the
  active theme), dictionary rows with icon blocks, notification history with
  colour-correct badges, and an Export quotes card sharing the saved-quotes
  Markdown exporter. Nothing faked: every control backs a real mechanism.
- **Book detail page (`src/pages/book.rs`)** — full-page rewrite to the
  `docs/files/book_detail.html` mockup. Fixed chrome row (back pill left,
  metadata pencil right), hero with 3D cover + progress + Format/Series/
  Publisher meta, title/author/half-stars/inline tag chips (add + remove),
  action row (Read / reading list / shelves / finished / Remove), serif
  description. Card grid: reading stats (4 tiles: time, estimate, minutes per
  chapter, sessions) + 7-day bars + timeline (recent sessions, finished,
  first opened); highlights (≤4, View-all dialog with delete); author
  (avatar, birth year only when known, bio, other owned books clickable);
  reading journey (✓/●/○ chapters from the EPUB spine, +N more expand);
  book file (name/size/imported/hash, show in file manager). The app's back
  chip hides on this page — the page owns its own. Series is **not** a card.
- **Series float (`src/pages/series_float.rs`, schema v10)** — the series name
  in the hero opens a floating window with the full listing. Fetched once from
  Open Library (series field, quoted-query fallback), cached in
  `series_cache` keyed by normalised `series|first_author`; covers cached in
  `series-covers/`. Works you own are title-matched and get live badges
  (Read / N% read / Not started) and open their book page; the rest read
  "Not in library". Footer says where and when it was fetched; ⟳ is the only
  re-fetch. An empty OL result is shown but not cached.


### Next
1. Home / dashboard — the two-column layout the design references imply.
2. Library, Reader chrome, dialogs.
3. "Similar from your shelf" on the book page — deferred from the mockup to
   its own round.

### Hard-won GTK/CSS rules

These are written up properly in the module header of `src/style.rs`. Read that
before touching the scrollbar block — the summary:

1. **This stylesheet already outranks Adwaita.** `relm4::set_global_css` loads
   at `APPLICATION` (600), the theme at `THEME` (200), and priority beats
   specificity. Long `:not()` chains are unnecessary, and GTK drops an entire
   comma-separated rule when one selector in the group fails to parse.
2. **Never use `opacity` below 1 on a widget that can collapse.** It forces an
   offscreen surface; a collapsed overlay scrollbar's is zero-sized and pixman
   rejects it. Hide with a transparent `background-color` instead.
3. **`margin`/`border`/`padding` are subtracted from the allocation.** Adwaita's
   slider carries 16px of them, so resetting only the border still leaves 8px
   and every `min-width` comes out negative. Zero all three, then set the size.
4. **`scrolledwindow:hover` matches the whole content area**, not the scrollbar.
   Use `scrollbar:hover` / `scrollbar.hovering`.
5. **`KALAM_NO_CSS=1` runs with no custom stylesheet** — use it to confirm a
   warning is even ours before theorising. `GTK_DEBUG=interactive` shows which
   rule actually wins on a node.

Cost of learning this the wrong way: about a dozen rounds on one scrollbar.
Each fix was plausible, none was verified against the toolkit's actual
behaviour first. When a warning carries a number, do the arithmetic across
runs — the constant that keeps appearing is the answer.

### Notes
- Reader *page* theming (Light/Sepia/Dark paper) stays separate from app
  chrome: a sepia page inside a dark app is a legitimate combination.
- The images in `docs/design/` are **inspiration the user collected**, not
  their own designs. Treat them as direction, not specification.

---

## A0 — Architecture & performance track  ✅ done (except the plugin seam)

**Status:** decided 2026-09-02 (design in `docs/conversation.md` §§1–3).
**A0 is done apart from step 8, which is designed and lands with AO3 in P7.**
Steps 1–5 shipped and are CI-green. Step 7 (perf-budget CI test) shipped
2026-09-04 as query-count budgets rather than time thresholds. Step 6 (grid
virtualization) shipped 2026-09-04 after being wrongly closed and then
reopened: at 2,000 books the All-books page went from 502 MB to 247 MB and from
434 ms to 12 ms.
A track, not a phase — interleaves with P6–P11.
- **Step 1 (measure) — done.** Data layer (headless `src/perf.rs`) confirmed all
  list-page queries < ~20 ms for 2,000 books; GUI (`src/timing.rs`,
  `KALAM_TIMING=1`) confirmed cold start ~0.9 s warm, book open 3.5 ms revisit,
  chapter turn ~50 ms warm, ~400 ms after a WebView re-spawn. The DB and warm
  reader are not the bottleneck; the cost is first-open + WebKit re-spawn +
  per-card decode of full covers.
- **Measured on the user's Arch machine, 2026-09-02 (post-WebView-pool).** The
  steady state is good: chapter turns settle at **37–48 ms**, repeat book opens
  at **2.8–215 ms**, and **no ~400 ms WebKit re-spawn appears after the first
  book** — the pool works. First book of a session still pays WebKit process
  startup (`chapter_load` 2772.9 then 697.2 ms); unavoidable without pre-warming.
- **Cold start, resolved 2026-09-02 — there was no regression.** The alarming
  8018.7 ms was a *first-run-after-build* artefact. Six consecutive runs:
  6290 → 1596 → 965 → 852 → 945 → 831 ms, i.e. **~900 ms steady, exactly the
  documented baseline**. The decay is the OS page cache warming on a
  freshly-linked binary (and its GTK/WebKit/ICU shared libraries), not app work.
  **My stated suspect — the bundled-dictionary import — was wrong**:
  `startup_dicts` measured **0.1–0.2 ms on every run including the first**, so
  the pref early-out was already doing its job. Splitting the span is what
  disproved it; the guess would have sent a fix at the wrong code.
- **What the breakdown does show.** Of a steady ~898 ms cold start, our
  instrumented work is **~138 ms (16%)**: `startup_db_open` ~6 ms (nothing to
  win), `startup_dicts` ~0.1 ms (nothing to win), `startup_first_page`
  ~137 ms — the only app-side target, and the one A0 can actually move.
  The remaining **~754 ms (84%) is un-instrumented**: GTK/libadwaita init,
  WebKit process setup, CSS parsing and GTK's first layout/realize, most of it
  before `AppModel::init` runs. So cold start is **not** a data-layer or
  page-construction problem, and further service-layer work will not touch it.
  A0 step 5 should treat ~750 ms of toolkit startup as the floor unless the
  first paint is decoupled from full initialisation.
- **Step 3 (thumbnails) — done.** `src/thumbs.rs` generates a persistent
  256×408 thumbnail (`cache/thumbs/<uuid>.png`) at import and on cover
  replacement; the grid decodes that instead of the full cover when the slot is
  small enough (never upscales it). New dep: `image` (default features off; only
  png/jpeg/gif/webp). **Existing books** are backfilled on a background thread at
  startup so nothing needs re-importing. **Next on this front:** the async
  *swap-in* (placeholder → texture on a worker) belongs to the task manager
  (step 4), where it architecturally lives.
- **Step 2 (LibraryService) — started; the seam exists, 3 pages converted.**
  `src/service.rs` answers a page's whole data question in **one call
  returning one owned snapshot** (`service.home()`), instead of a page making
  four direct `Catalog` reads and swallowing each error. Snapshots are plain
  owned `Send` structs *on purpose*: that is what lets the same call move to a
  worker thread later without touching the page, and a compile-time
  `snapshots_are_send()` test stops a future edit from breaking the property.
  The error policy now lives in one place — a failed read degrades to empty
  **and records the reason**, which pages surface as a toast (the service does
  not call `notify` itself: it must stay worker-callable, and `notify` is
  UI-thread-only). Converted: **Home** (4 reads → 1), **Analytics** (4 → 1),
  **Tags** cloud + tag-books, **Reading list**, **Shelves**, **All books**,
  **History**, **Lookup History**, **My Library** (8 reads → 1, the worst
  offender: it swallowed all eight), **Saved quotes** (N+1 removed),
  **Vocabulary**, **Book float**, **Series float** (N+1 removed),
  **Shelf detail**, **Book page**, **Reader**, plus the error-reporting pass
  over **Settings**, **Metadata editor** and **Shelf editor**. **Step 2 is
  complete: no page swallows a database read any more.** Home's
  "continue reading"
  fallback chain moved
  into the service and is unit-tested. **Remaining pages still hold an
  `Arc<Catalog>` and that is fine** — `LibraryService` borrows the same `Arc`,
  so both styles coexist; converting the next page is: add a snapshot method,
  swap the field, delete its `unwrap_or_default()`s. Writes (import) still go
  straight to the catalog — they belong to step 4.
- **Step 4 (task manager) — done.** `src/tasks.rs`; every slow job listed in
  the scope entry below is on the seam, including `install_bundled_dictionaries`
  at startup, which was the last leftover.
- **Step 5 (preloaders) — done.** `src/preload.rs` + `tasks::spawn_stream`;
  covers decode off the UI thread and swap in per card, chapters are warmed on
  open and on every turn.
- **Step 6 (grid virtualization) — ~~CLOSED~~ reopened, then ✅ DONE
  2026-09-04. Shipped and on by default: 502 MB → 247 MB, 434 ms → 12 ms at
  2,000 books.** The history below is kept because *how* it was wrongly closed
  is the more useful lesson. The evidence that closed it was measured on a run
  that never reached the grid.**
  The original finding read: build cost ~0.17 ms/card, and peak memory **flat**
  at 233 MB (139 books) vs 252 MB (2,000) because the cover cache is a bounded
  300-entry LRU — so virtualization fixes nothing. Both halves of that came
  from reports where `grid_build` never appears, because the screenshot harness
  was sending Tab/Tab/Return and silently staying on Home (see pitfall §21).
  **The memory figure was Home's, not the grid's.**
  Now that `KALAM_ROUTE=all-books` makes CI actually open the page, the same
  2,000-book job reports **502 MB** against 226 MB for the identical build
  before the page was reached, and 255 MB at 139 books:

  | Books | Grid reached? | Peak RSS |
  |---|---|---|
  | 139 | no (Home only) | 231 MB |
  | 2,000 | no (Home only) | 226 MB |
  | 139 | **yes** | 255 MB |
  | 2,000 | **yes** | **502 MB** |

  So memory is *not* flat in library size — it roughly doubles, +276 MB for
  1,861 extra cards, ~0.15 MB per card. **The 300-cover LRU is not the leak
  and was never the question**: it is working exactly as designed, and that is
  the point — the cost is the 2,000 GTK widget trees themselves.
  `build_book_grid` constructs one card per book unconditionally and attaches
  them all before returning, so every book in the library holds live widgets
  whether or not it is on screen. That is the precise thing virtualization
  exists to fix, and the reason it was ruled out has evaporated.
  Not yet fixed — recorded, with the correction, so the next decision is made
  against real numbers. `grid_build 421 ms` at 2,000 books is also now a real
  measurement rather than an extrapolation.
- **Step 8 (plugin-host seam) — designed 2026-09-03**, written up in
  [`docs/source-seam.md`](./docs/archive/source-seam.md). Code lands with AO3 in P7,
  deliberately (see the scope entry below).
- **Step 7 (perf-budget CI test) — ✅ DONE 2026-09-04.** The reasoning that
  reshaped it is kept below because it is the argument for *why* the budgets
  count queries instead of milliseconds: the runner's timings swing ~60% between
  identical runs (`startup_first_page` 7.1 / 13.7 / 16.4 ms at 2,000 books;
  `startup_dicts`, byte-identical work, 2774 → 3740 ms), so a time threshold
  loose enough not to flake cannot catch anything short of a 3× regression.
  Recommended replacement: assert **query counts** ("the book float issues 2
  queries"), which are machine-independent and catch the N+1 class that has
  actually bitten three times; plus un-`#[ignore]` the existing
  `src/perf.rs` probes, which already seed 2,000 books and assert ceilings but
  never run in CI.
  **Updated 2026-09-04:** the second half of this entry used to read "CI has
  never reached the grid — `grid_build` appears in zero of the seven committed
  reports", which was true and is now fixed. `KALAM_ROUTE` makes the
  screenshot job open a real page, and the run of 2026-09-04 records
  `grid_build 28.9 ms` / `grid_cards 139` with the rendered page proven
  different from Home. So a grid *timing* is now available to CI — but the
  variance argument above still stands, and query counts are still the better
  assertion. What actually changed is that step 7 is no longer blocked on
  "CI cannot get to the page".
- **WebView reuse — done.** The cheap win the timing surfaced: the reader used
  to call `webkit6::WebView::new()` in `init()`, so every book open spawned a
  WebKit process (~400 ms). `src/webview_pool.rs` now parks exactly one view
  between readers; the reader acquires it in `init()` and releases it in
  `shutdown()`. The reader's own lifecycle is unchanged (session start/end and
  progress save still run on every entry/exit) — only the expensive object is
  pooled, deliberately *not* the whole page, which would keep a reading session
  counting while the user browsed the library. Handlers that capture the
  component's `Sender` are recorded as `SignalHandlerId`s and disconnected
  before parking; sizing, context-menu suppression and the `"kalam"`
  script-message *registration* are permanent and live in the pool (WebKit
  rejects a second registration of that name). Escape hatch:
  `KALAM_NO_WEBVIEW_POOL=1` restores the old spawn-per-open behaviour for A/B
  measurement with `KALAM_TIMING=1`. Cost: the WebKit process (~100–200 MB)
  stays resident after the first book instead of being released on leave; the
  page is blanked on release so the book's DOM is still freed.
  **Needs an Arch smoke-test:** open book A → leave → open book B → return to
  A, checking highlights, dictionary popup, tap-to-look-up and progress restore
  all still work on the second and third opens (that is what a stale handler or
  a missed re-registration would break).

**Goal:** make Kalam feel instant (Yazi philosophy: *"don't make the UI
fast — make it never wait"*) and lay the seams the source platform needs.

### Scope (in order)

1. **Measure first.** `perf` + sysprof + GTK inspector on: cold start, book
   open, chapter turn, dictionary lookup, library scroll. Record the numbers
   — they decide what gets fixed (asserted bottlenecks get measured before
   being trusted).
2. **`LibraryService` behind `Catalog`.** ✅ seam built (`src/service.rs`),
   Home / Analytics / Tags converted; other pages migrate incrementally.
   Pages stop calling the DB directly and *ask* the service, which answers in
   one call with one owned `Send` snapshot. Moving queries off the UI thread
   then becomes a change in one place. (`Catalog.conn` is already
   `Mutex`-wrapped — feasible without a rewrite.)
3. **Thumbnails at import + async cover decode.** ~200px thumbnails into
   `cache/thumbs/<uuid>.png` at import time; the grid decodes tiny files that
   survive restarts; cards show a placeholder and swap in the texture when a
   worker finishes decoding. (The in-memory `COVER_CACHE` dies every launch.)
4. **Task manager (`src/tasks.rs`).** ✅ **seam built.** `tasks::spawn(work,
   on_progress, on_done)` — `work` is `Send` and gets a `Reporter` (progress +
   cooperative cancellation) but no UI access; `on_progress`/`on_done` are
   *not* `Send` and run on the main thread, so they may touch widgets and
   raise toasts. The type system enforces the split. `thread::spawn` +
   `async-channel` + the GLib main loop, no tokio. `cancel_all()` runs on
   window close. **Migration done:** dictionary import (was freezing the UI),
   the thumbnail backfill (now cancellable), all six metadata-editor fetches,
   the series and author fetches, and the Home / All-books imports (now one
   shared `spawn_import`, cancellable, no `notify` from a worker). Left as-is
   on purpose: `metadata::search_all`'s per-source fan-out, which `join()`s
   immediately and runs *inside* a task already. Remaining: downloads, when P6
   lands, and `install_bundled_dictionaries` at startup (~6.8 MB gunzip on
   first run, still on the UI thread — needs its own progress story).
5. **Preloaders.** ✅ **done.** `src/preload.rs`, plus `tasks::spawn_stream`
   for work that yields many results over time rather than one at the end.
   *Covers:* grid cards now use `cover_widget_deferred` — a placeholder goes
   up immediately and a worker decodes to raw RGBA, which the main thread
   wraps in a `MemoryTexture` and swaps in as each one lands. This is also the
   third clause of step 3, deliberately deferred to here. *Chapters:* the
   reader warms the next chapter's file on open and on every turn, so
   `chapter_html`'s read is served from the page cache. Only the read is
   preloadable — the render needs a main-thread `WebView`, and the HTML
   depends on live theme/font settings, so a cached string would go stale.
6. **Grid virtualization** — ✅ **done 2026-09-04, on by default.** Only the
   book cards on screen are built (plus 3 rows of margin). Measured on one
   machine in one CI run, 2,000 books: **502 MB → 247 MB**, `grid_build`
   **434 ms → 12 ms**, 2,000 cards → 48. Layout is unchanged — `GtkFixed`
   geometry matches the old `GtkGrid` exactly, same 6 columns, one scrollbar,
   header still scrolls with the page — and the user confirmed it on a real
   screen. `KALAM_NO_WINDOWED_GRID=1` restores the old behaviour.
7. **Perf-budget CI test** — ✅ **done 2026-09-04, reshaped: query counts, not
   milliseconds.** Six budgets in `src/perf.rs`, not `#[ignore]`d, gating every
   CI run. Each asserts that a call's SQL *statement count* does not grow with
   the number of rows it touches (20 books vs 60), which is the N+1 class that
   has actually bitten three times. Counting uses rusqlite's `trace` hook
   (`Catalog::count_queries`). Time thresholds were rejected on evidence: the
   runner's wall-clock swings ~60% between identical runs, so a ceiling loose
   enough not to flake is too loose to catch anything. **Verified by
   sabotage** — `hydrate_books` was temporarily reverted to a per-book query
   and CI went red on 4 of the 6 with 270 other tests passing.
8. **Plugin-host seam design** (the dependency for P7/P9): ✅ **designed
   2026-09-03 — [`docs/source-seam.md`](./docs/archive/source-seam.md).** Defines the
   `Source` adapter API (search / detail / chapters / content, with a
   two-variant `Content` for the text and image flavours). Scraped sources are
   **Lua plugins**; API-backed ones stay built-in Rust (§9a of that doc).
   **The trait deliberately does not land as code yet**: this is a
   binary crate with no `lib.rs`, so an unimplemented trait fails `-D warnings`
   or adds more `#[allow(dead_code)]`. It ships in the same commit as AO3,
   its first implementation and first caller (P7's opening move).

**Acceptance:** library grid stays smooth with 2,000+ books; book open and
page turns feel instant; all slow work is off the UI thread; pages are thin
(no DB calls, no decoding); a fresh chat can add a source plugin from the
documented API alone.

**DoD:** CI green; README / ROADMAP / `docs/conversation.md` updated; user
runs it on Arch.

---

## Schema (current + planned)

Current `SCHEMA_VERSION` = **14** (`src/db.rs`). Migrations run on open and are
additive; there is no downgrade path, so take a copy of
`~/.local/share/kalam/catalog.db` before testing a build that bumps it.

```text
books              id, uuid, title, sort_title, authors, series, description,
                   format, file_name, file_hash, cover_name, added_at, progress
                   + last_opened_at, finished_at                  -- v4
                   + rating (0..=10 half-stars)                   -- v5
                   + publisher, published, series_index (REAL)    -- v6
tags / book_tags
reading_progress   book_id, chapter_index, fraction, updated_at   -- P2
annotations        id, book_id, kind, loc, color, body, …         -- P3 (v3)
saved_words        …                                              -- P3 (v3)
dictionaries       id, name, lang, entry_count, …                 -- P3 (v3)
shelves            id, name, kind, description, rules(JSON), position  -- P4 (v4)
shelf_books        shelf_id, book_id, position, added_at             -- P4 (v4)
reading_list       book_id, position, note, added_at                 -- P4 (v4)
reading_events     id, book_id, kind, at, detail                     -- P4 (v4)
reading_sessions   id, book_id, started_at, ended_at, seconds, pct   -- P4 (v4)
reading_goals      year, target_books                                -- v5
app_prefs          key, value
metadata_overrides keyed on file_hash, NOT cascaded from books      -- v7
sources_state / download_jobs                                     -- P6+
```

`metadata_overrides` is deliberately **not** `ON DELETE CASCADE`: surviving a
book's deletion is the entire point, so edits come back when the same file is
re-imported. Its cover lives in `covers/<file_hash>.<ext>`, not in
`library/<uuid>/`, which is removed with the book.

---

## CI plan

| Check | When | Status |
|-------|------|--------|
| `cargo fmt --check` (auto-fix + push fix commit when it differs) | every push | active |
| `cargo clippy -D warnings` | every push | active |
| `cargo test --all-targets` — compiles **and runs** the unit tests (in-memory SQLite, headless); failures publish to `ci-logs/test-latest.txt` | every push | **active — installed by the user (`bc6473f`), first run green (`1aae8f1`)** |
| `cargo build` / `release` | every push | active |
| GUI smoke | **your Arch machine** at phase end | user |

Deps include `webkitgtk-6.0` for P2+.

**Workflow files:** ~~the App cannot push `.github/workflows/`~~ — corrected
2026-09-18, it can (commit `a4c4d54`, run `35362407132`). The agent edits the
workflow in place and pushes it. `docs/ci/github-actions-ci.yml` is kept as a
re-synced copy for the fallback case where that permission is lost again; see
`docs/ci/README.md` for the manual commands.

---

## Risk register

| Risk | Mitigation |
|------|------------|
| WebKit RAM on 4 GB | One WebView; chapter-wise load |
| EPUB blue link spam | Aggressive reading CSS; inject at end of body |
| Reader timer after drop | No background progress timer; save on nav/close |
| Comics RAM | Decode only nearby pages (P8) |
| Scope creep | Phase gates; comics design locked, code in P8 |
| Annotation loc vs re-download | P7 best-effort; may reset on spine change |

---

## Post-P5 work (done, between P5 and P6)

### Performance pass ✅
- **Query storm**: ~99 SQL queries per Library click with 50 books → ~8.
  N+1 tag lookups collapsed, `list_books()` no longer loads the whole library
  to draw 6 covers, prepared-statement cache, `library_stats()` memoised
  against SQLite's `total_changes()` so no write path has to remember to
  invalidate. `synchronous=NORMAL`, 64 MB cache, `temp_store=MEMORY`, mmap.
  Indexes on `progress`, `last_opened_at`, `finished_at`.
- **Widget rebuilds**: pages cached in `AppModel.cache` keyed by route.
  Reader/BookPage/ShelfDetail/TagBooks/LibrarySection deliberately excluded
  (they own a WebView or per-book state). Invalidated via `cache_token`.
- **Blocking imports**: `Catalog` moved `Rc` → `Arc`; the import loop runs on
  `spawn_command` and reports per-file progress.

### Hardening pass ✅
1. **Notifications** (`src/notify.rs`) — toast overlay per
   `docs/design/notifications.png`; history panel in Settings.
2. **Library backup** — `VACUUM INTO`, consistent even while running.
3. **Reader cache pruning** — orphaned + 14-day-stale extracts dropped at
   startup.
4. **Poison-safe locks** — 77 `expect("db lock")` → `Catalog::conn()`.
5. **`db.rs` split** — 3,367 lines → 1,702 + 7 focused modules.

### Bug fixes worth remembering
- **Metadata overrides** keyed on `file_hash` (schema v7) so edits survive
  delete → re-import, including the cover, which is stashed in
  `covers/<hash>.<ext>` because `library/<uuid>/` goes with the book.
- **EPUB writeback on single-line OPFs** — `rewrite_opf` filtered the
  metadata block line by line, which only works on pretty-printed files.
  Real EPUBs often put the whole block on one line, leaving the old
  `<dc:title>` beside the new one; readers showed the stale one. Now walks
  elements, not lines.
- **Notifications only fired on failure** — `notify::report` stayed silent on
  `Ok`, so every successful action looked broken. Added
  `notify::outcome`/`outcome_info`.

---

## A1 — In-app dialogs (requested 2026-09-02, scheduled after A0 step 2)

**Why:** on a tiling compositor (the user runs Sway) a `gtk::Window` is a real
top-level window. Sway will happily send it to another workspace, tile it
beside the main window, or leave it behind when you switch — none of which is
what a modal dialog means. The app already agrees with this in principle:
`app.rs` has an in-app float layer (`float_host` + `float_scrim` over a
`gtk::Overlay`) and a comment on `open_annotations_floating` stating floats
live there "never a separate window — so the compositor can't move it to
another workspace". **The conversion was simply never finished.**

**Current state (audited 2026-09-02):**

*Already in-app (5):* book float, series float, annotations panel, shelves
panel, tags panel. **Audited and brought in line 2026-09-03** — see "Making
the older dialogs match" below; they were not consistent with each other, let
alone with the five new ones.

*Still separate `gtk::Window`s: **none**. All five converted 2026-09-03.*

| Where | What it is | Exit kind |
|---|---|---|
| `metadata_editor.rs` | Edit metadata (the big one — scrolling content) | `UnsavedInput` |
| `shelf_editor.rs` | New / edit shelf (manual **and** smart-rule builder) | `UnsavedInput` |
| `reading_list.rs` | "Add to reading list" book picker | `OwnButtons` |
| `shelf_detail.rs` | "Add books to shelf" book picker | `OwnButtons` |
| `shelves_grid.rs` | "Delete shelf" confirmation | `OwnButtons` |

All five now go through `src/widgets/in_app_dialog.rs`. `gtk::Window::builder`
no longer appears anywhere in `src/`.

*Deliberately staying native (4):* every `gtk::FileDialog` (`all_books.rs`,
`home.rs`, `metadata_editor.rs`, `settings.rs` ×2). These are portal-backed
file pickers — the compositor and the desktop portal own them, the user
expects their normal file manager, and re-implementing a file browser in-app
would be strictly worse.

**Requirement (revised by the user 2026-09-03).** The earlier rule was
"every in-app dialog needs a visible ✕". The user corrected it: **the dialogs
exist for different reasons, so they should not all be dismissed the same
way.** The affordance must match what the dialog *is*:

| Dialog is… | Affordance | Why |
|---|---|---|
| A **detour** you came to from somewhere (book float, series float) | **‹ Back** | You are returning to where you were, not discarding something. Back says that. |
| A **transient panel** layered on the current context (annotations, shelves, tags) | **✕** | Nothing to return to; you are dismissing an overlay. |
| A **form with unsaved input** (metadata editor, shelf editor) | **Cancel** + explicit Save | "✕" is ambiguous next to unsaved edits: does it discard? Cancel is unambiguous. |
| A **confirmation** (delete shelf) | **Cancel / Delete** | Two named outcomes; a ✕ is a third, vaguer one. |

**Universal, on top of the above: clicking the dimmed backdrop closes the
dialog, and Esc closes it.** Backdrop-click is now implemented (see the
changelog entry for 2026-09-03) — the scrim already intercepted those clicks
so they could not reach the page behind it, but its handler was empty, which
made the dim look interactive and do nothing.

**Open question the user raised:** with backdrop-click and Esc both working,
can the explicit button be dropped entirely? **Decision: no, not for all of
them.** Backdrop-click and Esc are both *invisible* affordances — nothing on
screen advertises them, so a dialog whose only exits are invisible is still a
trap for anyone who does not already know the trick. Keep one visible control
per dialog, but let it be the *right* one from the table above rather than a
reflexive ✕. Forms and confirmations especially must keep a named button,
because for those the question is not only "how do I leave" but "what happens
to my edits when I do". The one place a bare ✕ can go is where the visible
control would be pure duplication of an already obvious action.

**Order (cheapest and safest first):** delete-shelf confirmation → the two
book pickers (they are near-identical, so one helper serves both) → shelf
editor → metadata editor last, because it is the largest and has its own
scrolling/sizing logic tuned for short laptop screens.

**Risk to watch:** the float layer is a `gtk::Box` in an overlay, not a
window, so it has no built-in focus containment. The pickers contain long
scrollable lists and the metadata editor contains many entries — Tab order and
initial focus need checking on each conversion, and the scrim already blocks
click-through.

### What the conversion actually needed (2026-09-03)

`src/widgets/in_app_dialog.rs` — one helper, `present(anchor, title, exit,
content) -> Option<InAppDialog>`. It walks up from any widget to the app's
root `gtk::Overlay`, then adds its own scrim + centred panel. It deliberately
does **not** reuse the `AppMsg` float layer: these dialogs are plain functions
taking an `on_confirm: impl Fn()` closure, and a closure cannot travel through
a `#[derive(Debug)]` message enum.

`DialogExit` has two variants rather than the table's four, because the two
kinds that were real windows both bring their own buttons:

* `OwnButtons` — content supplies the named buttons (Done, or Cancel/Delete).
  Backdrop-click dismisses; nothing is lost.
* `UnsavedInput` — a form. Backdrop-click is **disabled**: silently discarding
  a half-typed description because a click landed slightly off target is a bad
  trade. Esc still works and Cancel is right there.

The ‹ Back and ✕ rows of the table describe the book/series floats, which
already live in `app.rs` with their own headers; they join `DialogExit` when
those headers are revisited.

**Three things a `gtk::Window` had been doing for free**, each of which had to
be replaced by hand:

1. **Teardown.** `window.close()` destroys the widget tree, which breaks the
   reference loop between a widget and the callback that captures it. Removing
   an overlay child does not, so `teardown()` also empties the host — without
   it every dialog ever opened would stay in memory.
2. **Height bounds.** A window has a default height; a panel centred in an
   overlay is sized by its content. The metadata form and the smart-shelf rule
   list both got `max_content_height` + `propagate_natural_height`, or a long
   description / twenty rules would push Save off a 768px screen.
3. **A real window handle** for the cover `gtk::FileDialog`, which is
   portal-backed and needs a genuine top-level parent. Resolved from the
   anchor's root instead of from the (now non-existent) dialog window.

Also removed: three copies of a `window_of()` helper that existed only to find
a parent window for these dialogs, and `shelf_editor`'s and `metadata_editor`'s
hand-rolled Esc handlers, now that the helper provides Esc for all of them.

### Making the older dialogs match (2026-09-03)

The user asked whether the five dialogs that were *already* in-app matched the
five new ones. They did not, and they did not match each other either. Four
differences found, all fixed:

1. **Two floats taught different shortcuts for the same keys.** The book
   float's ✕ was tooltipped "Close (Q)", the series float's "Close (Esc)" —
   both keys worked on both.
2. **`q` closed a float while you were typing in it.** The handler in
   `app.rs` fired on `q`/`Q`/Esc whenever a float was visible, without asking
   whether a text box had focus. The tags panel has an entry, so typing the
   letter `q` into it dismissed the panel. Esc now always closes; `q` is
   ignored while a `gtk::Entry`, `SearchEntry` or `Text` has focus.
3. **The two floats had a ✕ that the user did not want.** Decision
   (2026-09-03): **remove it, and do not replace it with ‹ Back.** Both floats
   are read-only detours, so a misclick on the backdrop costs nothing — "I
   won't lose anything if I accidentally misclicked on the outside". This is
   the one case where the "keep one visible control" rule is waived, and
   deliberately: the rule exists to protect against *losing something*, and
   there is nothing here to lose. `SeriesFloatMsg::Close` and
   `SeriesFloatOut::Close` became unreachable and were deleted with it.
4. **Three panels had no title, and a different shell.** The annotations,
   shelves and tags panels rendered with no heading at all, and with their own
   14px-corner CSS (three byte-identical copies) against the dialogs' 20px.
   They now use a shared `panel_title()` helper with the same markup and CSS
   class as the A1 dialog header, and one merged CSS rule.

Note the corrected finding: the series float opens from the **book page**
(`book.rs:695`), not from the book float — the book float's series line is a
plain label. So closing it to the page was always right; an earlier reading
that it "skipped a step" was wrong.

## Immediate next steps

**Next, in order (locked 2026-09-02):**

1. **A0 — Architecture & performance track** (the section above). Start with
   measurement, then `LibraryService` + thumbnails/async decode; design the
   plugin-host seam. This is the foundation for everything after.
2. ~~**A1 — In-app dialogs**~~ **done 2026-09-03, including the polish.** All
   five remaining `gtk::Window` dialogs draw inside the main window via
   `src/widgets/in_app_dialog.rs`. Focus containment is done:
   `src/widgets/focus_trap.rs` confines Tab to the open panel and is used by
   both dialog systems. Folding the float headers into `DialogExit` was
   **considered and rejected** — the floats deliberately have no visible exit,
   so the variant would be uncallable dead code (a `-D warnings` failure), and
   they are Relm4 components hosted by `app.rs` rather than users of the
   helper. The two systems share behaviour, not types.
3. **P6 — Downloads hub** (unified queue + folder watch).
3. **P7 — Fiction platform** (AO3 first) via Lua source plugins; native tag
   search; download; follow + auto-updater.
4. **Renderer vertical slice** (alongside P7) — cosmic-text fiction renderer;
   the 2–4 week calibration milestone.
5. **P8 — Comics local** → **P9 — Manga platform** → **P10 — PDF (MuPDF)** →
   **P11 — Tools** → **P12 — WebAssembly (Wasm) plugin system matures.**

**Recently completed (do not redo):** dictionary track Phases 8–10 (shipped,
CI-green: POS dividers `be11c07`, priority reorder `d12c905`, lookup history
`7ce8bfb`+fixes); CI workflow with the `cargo test` step installed and green
(173 unit tests, failures publish to `ci-logs/test-latest.txt`); backend
review done (one latent bug fixed, dict importers hardened, 9 new tests);
reader milestones 1–3 shipped (annotation workflow, hybrid anchoring, dict
multi-result popup).

**Deferred (revisit only after the above is underway):** annotation design
polish (without changing saved-highlight anchoring); multi-chapter
buffering; continuous book-wide scrolling; chapter auto-advance redesign;
advanced CFI; UI-overhaul screen work (`library_look.png` two-column
dashboard — the app is currently a single vertical stack).

---

## Changelog of plan decisions

| Date | Decision |
|------|----------|
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
| 2026-07-27 | P5 shipped: metadata editor, cover replacement, Open Library lookup (staged for review, never auto-applied) |
| 2026-07-27 | Ratings (half-star), yearly reading goals and quote notes added from the reference designs; social elements deliberately skipped |
| 2026-07-27 | P4 shipped: shelves engine (manual + flat-rule smart shelves), reading list, event-log history, reading-time sessions, tags browse, analytics with streaks |
| 2026-07-27 | Smart shelves locked as flat rules + All/Any; nested groups deferred and kept JSON-compatible |
| 2026-07-26 | P3 annotations & dictionary shipped: highlights (5 colors), quotes, offline dict packs (StarDict/SQLite/TSV), Saved quotes/words real data, export Markdown, annotations list, dictionary popup, Settings import |
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
| 2026-09-05 | **Code Review Batch 2 (Medium priority) completed — gitignore, Makefile, metadata unit tests, and date math tests.** (1) Updated `.gitignore` from `ci-shots/` to `ci-shots*/` to properly ignore all automated screenshot directories (`ci-shots-gui`, `ci-shots-1`). (2) Created a root `Makefile` with `all`, `build`, `dev`, `test`, `check`, `clean`, `install`, `uninstall` targets for installing Kalam (`kalam` binary, desktop entry, icon). (3) Added unit tests across `src/metadata/google_books.rs`, `src/metadata/openlibrary.rs`, and `src/metadata/series.rs` for author list formatting, thumbnail fallbacks, category filtering, published date fallback, and series normalization/sorting. (4) Added unit tests for Hinnant date math functions in `src/db.rs` (`civil_from_days`, `days_from_civil`, `days_from_iso`, `format_unix_utc`), verifying exact round-tripping for leap days, century non-leap years, pre-epoch dates, and ISO string formatting. Test suite expanded to 336 passing tests; `cargo check` clean with 0 warnings. |
| 2026-09-05 | **Code Review Batch 3 (Low priority) completed — external CSS, bundled-dictionaries feature flag, LibraryService migration.** (1) Extracted monolithic 3,574-line CSS string from `src/style.rs` into `resources/style.css`, loaded via `include_str!`. (2) Added `bundled-dictionaries` default feature flag to `Cargo.toml`. (3) Completed `LibraryService` struct field integration across remaining pages (`author.rs`, `series_float.rs`, `settings.rs`). All 336 tests passing clean with 0 compiler warnings. |
| 2026-09-04 | **P6.5 started: the library registry and the Libraries settings card are in, CI green.** `~/.config/kalam/libraries.json` records every library and which one is open, and `paths::data_dir()` now reads it — so the ~30 path helpers built on that one function follow automatically, which is why making libraries switchable was a change at the root rather than a sweep through the app. **Existing installs are untouched**: with no library chosen it falls back to the old fixed location, so an upgrade is a no-op until the user asks for something else. The registry is written temp-file-then-rename, the same rule `epub_write.rs` follows, because a truncated write here would lose the record of where every library lives. Settings → Storage now has a Libraries card with Add / Open / Forget; Forget removes the list entry only and the message says plainly that the folder and books were not touched, since "forget" and "delete my library" must never be confusable. 10 tests, deliberately concentrated on the index arithmetic — `active` is an index into the list, so forgetting an *earlier* entry shifts it, and an off-by-one there does not crash, it silently opens somebody else's library. **Worth recording how this went:** the first two pushes failed CI on dead code, because `-D warnings` rejects methods with no caller — precisely the rule `source-seam.md` §12 gives for why the `Source` trait cannot land before its first implementation. The fix both times was to build the caller rather than silence the lint, which is the right pressure: no API lands in this repo without something that uses it. Still to come: the relaunch flow (selecting currently takes effect on next start), migrating an existing library into the scheme, the per-library/global split so dictionaries stay shared, and the `kalam.json` per-book backup |
| 2026-09-04 | **P6.5 green: the per-library/global preference split is in, and CI is healthy again after eight red runs from three unrelated causes.** The split matters because `app_prefs` lives in `catalog.db`, i.e. *inside* a library — so without it, switching library would silently reset the theme, reader font size, dictionary settings and API keys, since the new library's database has never heard of them. App-level prefs now mirror to `~/.config/kalam/prefs.json`, read from there first, falling back to the library so existing installs carry across untouched. Unknown keys default to **per-library** on purpose: a global pref that should have been per-library silently applies one library's value to another and reads as corruption, while the reverse is merely "set it again". Three of the key names were wrong on the first attempt — the app theme is `ui.theme` not `theme`, the writeback flag is `epub.write_metadata`, dictionary markers are `bundled_dictionary_*` — all now read out of the code rather than guessed, because a *nearly* right key classifies as per-library and the setting quietly stops following the user. **The eight red runs are the more useful record.** Two were dead code (`-D warnings` rejecting methods with no caller, the same rule that keeps the `Source` trait from landing early). **Five were a formatting step failing the build** — rustfmt auto-committed and pushed, the push was rejected, and a non-zero exit killed the run, so a cosmetic check masked clippy, the tests *and* the build. The real defect sat behind it untouched for all five: my append-tests script cuts at the file's last `}`, which stopped being the test module's once functions were added after it, so 70 lines of `#[test]` were spliced **inside `set_global_pref`'s body** — braces balanced, so nothing short of a parser could see it. rustfmt is now report-only, publishes its diff to `ci-logs/`, and cannot fail a run (§23). The last was a pre-existing service test that began reading the developer's own `~/.config/kalam/prefs.json` the moment `dict_history_enabled` became global, making its result depend on the machine rather than the code (§24). Three lessons: a cosmetic check must never gate a build; do not append code by locating the last brace; and moving state outside the repository turns every test that touches it into a test of the machine unless you isolate it in the same change |
| 2026-09-04 | **P6.5: dictionaries no longer live inside a library, and the existing library now appears in the list.** Two fixes, the first caught by reading the paths module rather than by a failing test — which is the only way it *would* have been caught, since nothing breaks until someone actually switches library. `dictionaries_dir()` hung off `data_dir()`, and `data_dir()` is now the **active library**. Left alone, switching would have looked for dictionaries in the new folder, found none, and reinstalled the bundled packs — roughly 4 MB compressed and several seconds of import — **once per library, permanently**. Exactly the "you would reinstall dictionaries every time you switched" failure the phase description warned about, quietly reintroduced by the change that made libraries switchable. They now hang off a shared root, and the path is unchanged for anyone who never switches. The neighbouring judgement call is recorded in the code: **author photos and series covers stay per-library**, even though they are internet-fetched and would be byte-identical everywhere, because they are caches of *this library's* authors — deleting a library should take its cached photos with it, and a shared folder would accumulate photos for authors nobody owns any more with nothing to prune it. Both are caches, so the decision costs only a refetch to reverse. Second: **the pre-existing library is now adopted into the registry at startup.** The fallback already made it work, but it left the user's own books as the single library absent from Settings, so Open and Forget applied to every library except theirs and adding a second would make the first appear to vanish. Nothing is moved or copied — only the list learns the folder exists. A folder is judged a library by containing `catalog.db`, deliberately not by being non-empty: picking `~/Documents` by mistake must not be treated as an existing library, while picking an empty folder is the normal way to start a new one. The Add button now reports which of the two happened, since "library added" is ambiguous between "created an empty one" and "found my books" — alarming in one direction, confusing in the other. CI green first try |
| 2026-09-04 | **P6.5 complete — libraries are switchable, portable and self-describing. Next is P6 (Downloads hub).** Two final pieces. **Switching now restarts the app** instead of taking effect "next launch", which was an odd thing to ask of someone who had just clicked Open. It confirms first (a restart closes whatever you are reading), saves the choice, *then* re-execs — that order matters, because restarting first would reopen the old library and look like the click did nothing. `exec` rather than spawn-then-quit, so two Kalams never hold the same catalog open at once; and a restart rather than repointing in place, because `data_dir()` is cached for the process and pages hold live database handles, so switching underneath them would leave some reading the old library while writing the new one — corruption rather than a visual glitch. **And `kalam.json` beside every book** (`src/sidecar.rs`): title, authors, series, tags, rating, reading position and highlights, each highlight carrying its own text — the field that makes it findable again after a file changes, the same insight as the P7 re-anchoring plan. Explicitly a **backup, never the truth**: the database stays authoritative and these are never read during normal operation, so cross-book screens remain one query and no conflict rule is needed. Names not paths, or a copied folder would be full of wrong locations. **A clippy dead-code error turned out to be the useful part of this commit.** `read_sidecar` had no caller outside tests — and that was not a lint technicality, it was the observation that I had built a backup with no way to check or complete it. The fix was a Settings card showing how many books have a backup copy, plus a button to write the missing ones. That matters on its own terms: "your library describes itself" is worth nothing unless it is verifiable, since sidecars are written on code paths that could quietly stop running and the failure would stay invisible until the day someone actually needed them (§19). Left deliberately unbuilt: the rebuild-from-folders command itself. The survey proves the data is there; nothing consumes it yet, and inventing that flow before anyone has lost a database would be guessing at the recovery experience |
| 2026-09-04 | **Two P6.5 defects found by re-reading after "done", plus the README brought up to date.** Neither would have failed a test, which is why the check was worth doing. **(1) The `kalam.json` backup was going stale.** It was refreshed on import, on metadata edits and on adding a highlight — but *not* when a tag chip was added or removed, a highlight deleted, or a note or colour changed. So an ordinary edit left the backup out of date while the recovery card still reported "all books covered", because it counts files rather than freshness — a promise that quietly degrades until the day it matters. Every edit now refreshes it. Reading progress deliberately still does not: it fires on every page turn, and your exact place is the least valuable field and the quickest to re-find; that is now a comment rather than an omission the next reader would take for a bug. **(2) A library on a removable drive would have looked like data loss.** If the selected folder is gone — unplugged drive, unmounted share, folder renamed — nothing failed: `create_dir_all` recreated it and `Catalog::open` built a fresh empty database inside. The user sees an empty library and concludes their books are gone, when the drive is merely not plugged in; worse, the recreated folder then *looks* like a real library, so reconnecting the drive does not obviously fix anything. Kalam now refuses to start, names the missing folder, states plainly that nothing was changed, and says where to look. Refusing rather than falling back to another library is deliberate: writing into a library the user did not choose is how books end up scattered across two places. **README** was several phases stale — it still described A0 as in progress, never mentioned libraries, and its Data section called `~/.config/kalam` "future" when it now holds the library registry. It also gains a **Switches** table: `KALAM_NO_WINDOWED_GRID`, `KALAM_NO_PRELOAD`, `KALAM_NO_WEBVIEW_POOL`, `KALAM_NO_CSS`, `KALAM_TIMING`, `KALAM_ROUTE` all existed but were discoverable only by grepping the source, which makes an escape hatch useless to the person who needs it. Also fixed: `ci-logs/test-latest.txt` and `clippy-latest.txt` were published **only on failure**, so a fixed problem left its old failing log committed and every later green run still showed red — it misled me three times in one session, the last reading "1 failed" from a run two hours dead (pitfall §25) |
| 2026-09-04 | **P6 and P7 combined into one phase; P5.5 (UI overhaul) moved to the very end.** Both at the user's request, and both are the right call for the same underlying reason: **do not design against imagined content.** *Combining P6+P7* — a downloads queue with nothing to download is a shell, and a fiction client that fetches things needs somewhere for those jobs to live. Built apart, the queue would be designed against guessed callers and then reworked when the real ones arrived; built together, its shape answers to what P7 actually needs. They stay separate sections in this file because they remain distinct bodies of work — the queue is infrastructure, the client is the feature — but they ship as one phase. *Moving P5.5 last* — the user: *"we will makeover the UI at last when everything is ready."* Everything still ahead adds screens: a downloads queue, a browsing client, author pages, a comics pager, a PDF view. Restyling Home and Library now would mean restyling them again once those exist, and a design settled before its content is a guess. Doing it last is one pass over a known set of screens instead of several passes over a moving one. What is already shipped stays shipped (colour system, 13 themes, Settings v2, book page, series float); only the remaining screens move. **New running order:** P8/P9 comics and manga → P6+P7 downloads and fiction → renderer slice → EPUB path → P10 PDF, P11 tools, P12 Lua → P5.5 UI last. Phase map, the "agreed order" list and the README status table all updated to match, since a stale order at the top of this file is exactly what sent an earlier chat off to build something that already existed |
| 2026-09-05 | **Phase 8 (P8) — Comics local + Moku-style reader completed cleanly.** Integrated CBZ/CBR comic archive reading (`src/comics.rs`) with natural alphanumeric sorting (`page2.jpg` < `page10.jpg`) and image extraction. Built interactive Relm4 `ComicsReaderModel` (`src/pages/comics_reader/`) with black immersive stage, top bar controls (title, page index, LTR/RTL/Webtoon reading direction toggles, Fit Width/Height/Original mode toggles), bottom bar scrub slider and prev/next page navigation. Includes bounded viewport texture caching (current page ± 2 adjacent pages) for memory safety. Automated routing in `AppMsg::OpenReader` to automatically open CBZ/CBR files in the comics reader. All 336 unit tests passing cleanly with 0 compiler warnings. |
| 2026-09-05 | **Phase 9 (P9) — Manga Platform completed.** We survived the Great Aggregator Crisis. After attempting to build Comick (API disabled images) and Manganato (domain entirely hijacked/CF blocked), we successfully pivoted to **WeebCentral**. Built a seamless native scraper in `src/sources/weebcentral.rs` that taps directly into their HTMX API, bypasses Cloudflare entirely, and natively streams official, high-quality simulpubs straight into the immersive comic reader. Re-wired the UI in `remote_detail.rs` to strip away the "Open Web" fallback entirely and properly constrain cover image ratios using `Pixbuf::from_stream_at_scale` so they don't break GTK's layout engine. |
- **Future Consideration**: Literotica Account Sync (Login system to sync user's personal favorites and reading history).

---

## Master Roadmap Redux (Updated 2026-09-08 via /grill-me)

**The New "Offline Library First" Sequence:**
We have officially halted all online scraper work (P7/P9) and removed hardcoded scrapers to prioritize the ultimate offline reading sanctuary.

1. **P1.5 - Library Organization & UI:**
   - **Inline Editing (`book.rs`):** `gtk::Stack` to allow click-to-type inline metadata editing for Titles and Authors.
   - **Cover Stacks & Smart Shelves:** Visually collapse books in a series. Implement dynamic Smart Shelves.
   - **Dedicated Metadata Fetcher:** Elevate the Open Library `in_app_dialog` to a full route.
2. **P10 - PDF Engine:** Zathura-style smart crop default (auto-detect ink bounds). Reflow engine toggle using heuristic text extraction via PDFium.
3. **P11 - The Pure-Rust Custom Renderer:** Replace WebKit with `lol_html` (CSS stripping/normalization) + `cosmic-text` (layout). Forces all EPUBs to perfectly obey our themes. WebKit demoted to fallback for exotic EPUBs.
4. **P8.5 - Polish Comics Reader:** Add right-to-left manga mode, background image preloading.
5. **P8.6 - The Remaster Tool:** A heavy offline tool to permanently upscale low-res CBZs using CPU Lanczos3 interpolation.
6. **P3.5 - The Sanitizer Pipeline:** A background worker that intercepts imported EPUBs, strips hardcoded CSS, fixes broken XML, and repacks them cleanly.
7. **P3.6 - Deep Content Search:** Implement a default `tantivy` indexer to provide 20ms full-text search across the actual contents of the entire library.
8. **P6 - Download Queue / Sync Hub.**
9. **P5.5 - UI Overhaul:** "Two Worlds" design (Offline Library vs Online Hub).
10. **P12 - Wasm Plugin Ecosystem:** Build the WebAssembly host architecture to power dynamic scrapers (`wit/kalam.wit`), officially replacing hardcoded Rust scrapers.
11. **P7/P9 - Reintroduce Online Sources:** Write Wasm plugins for AO3, MangaDex, etc., and pair them with bespoke hardcoded GTK UI "Husks" inside Kalam.

| 2026-09-18 | **Part 1 Offline Master Plan Locked (`docs/offline-roadmap.md`).** Consolidated all offline requirements into 6 dedicated modules (Reading Engines & Inline Editor, Yazi Architecture & Service Layer, Content Sanitizer & Deep Content Search, Library Management & Metadata Editors, Annotations & Vocabulary Hub, and UI/Performance Closure). All online scraper/plugin features strictly relegated to Part 2, and the Material 3 UI design overhaul positioned at the very end of Part 1. |
| 2026-09-18 | **CI slimmed from three jobs to two, and the `--workspace` fix landed.** Two changes, both to `.github/workflows/ci.yml` (and mirrored in `docs/ci/github-actions-ci.yml`). **(1) The gate was only running a tenth of the workspace.** The workspace has 11 members and no `default-members` key, so with no `--workspace` flag Cargo selected only the root `kalam` package — `--all-targets` picks *targets*, not *packages*. Both `cargo clippy` and `cargo test` were scoped that way, so 8 crates under `crates/` and 2 under `tools/` were compiled as dependencies but never linted or tested, and `chapbook-viewer-gtk`, `chapbook-cli` and `kalam-reader-demo` — which nothing depends on — were never built at all. They could have failed to compile and the run stayed green. Roughly 420 tests under `crates/*/tests` and `tools/*/tests` (pagination, reader conformance, locators, render goldens, the stability policy) never executed. Fixed with `--workspace` in the workflow *and* in the `Makefile`. **Not yet confirmed green by an actual run** — the sandbox has no route to crates.io, so CI is the only compiler available, and this is the first time these crates get linted and tested together. Expect the first run to surface real failures; that is the fix working. **(2) The `scale` job was deleted and `screenshots` cut to one run.** The scale job re-measured a question settled on 2026-09-04, whose answer is committed in `ci-logs/scale-2000-comparison.txt` (windowed 247 MB / 7.1 ms vs the old build-every-card grid 352 MB / 139.7 ms). The PNG artifact upload went to nobody — the sandbox cannot download artifacts, and the text report carries the same facts — and the all-books screenshot pass duplicated the `read-1` run for the one thing still worth proving: the app launches and a book opens without panicking. That single run remains, renamed *smoke test*, still `continue-on-error: true`, so `build` is still the only job that can fail a run. `ci-logs/screenshots-latest.txt` and `ci-logs/scale-2000-*.txt` are now frozen records rather than current output. **Also corrected:** the workflow's own comment undercounted the skipped members as "seven chapbook/kalam-reader crates"; the real figure is eight crates plus two tools. `docs/ci/github-actions-ci.yml` had drifted 134 lines from the file that actually runs and is re-synced; the reason it exists at all — the App token cannot push `.github/workflows/` — is unchanged, so it stays |
