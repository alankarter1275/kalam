# Archived: the P0–P12 phase plan (ROADMAP.md, 2026-07 → 2026-09-08)

> **Archived 2026-09-18. Do not plan from this file.**
>
> This is the old phase-numbered roadmap (P0–P12, plus the A0/A1 tracks and the
> dictionary overhaul brief). It was moved here when `ROADMAP.md` was rebuilt
> around the six-module Part 1 plan. It is kept for two reasons, and neither is
> "what to build next":
>
> 1. **The lessons.** The A0 step 6 entry is the best thing in it: three
>    increasingly confident estimates of the same problem, each wrong, each
>    reasoning about a test harness that never actually ran the code. Several
>    other entries record a mistake and what it cost.
> 2. **The record.** Many statements here are now false — the reader is no
>    longer WebKit, `lol_html` was abandoned in favour of stylo, the PDF engine
>    is MuPDF not PDFium, and P6+P7 is marked done but is not shipped. They are
>    left as they were written, because that is what was believed at the time
>    and the changelog rows in `ROADMAP.md` reference them.

**For the current plan, read [`ROADMAP.md`](../../ROADMAP.md).**

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

Full discussion: [`docs/libraries-and-portability.md`](./libraries-and-portability.md).

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
  [`docs/source-seam.md`](./source-seam.md) §2. Same adapter shape as
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
> [`docs/p7-scope-correction.md`](./p7-scope-correction.md). **The `Source`
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
  See [`docs/p7-login-and-reading.md`](./p7-login-and-reading.md).
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
  [`docs/source-seam.md`](./source-seam.md) §2. Fiction and manga differ
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
  [`docs/fichub-and-ffn.md`](./fichub-and-ffn.md)
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
  [`docs/p7-login-and-reading.md`](./p7-login-and-reading.md)
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
  trivially. See [`docs/p7-storage.md`](./p7-storage.md)
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
> Rust. Full reasoning: [`docs/source-seam.md`](./source-seam.md) §9a.

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
  [`docs/source-seam.md`](./source-seam.md). Code lands with AO3 in P7,
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
   2026-09-03 — [`docs/source-seam.md`](./source-seam.md).** Defines the
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

**Workflow files:** the agent edits `.github/workflows/ci.yml` directly and
pushes it — the GitHub App has the `workflows` permission (granted 2026-09-18).
The duplicate copy that used to live at `docs/ci/github-actions-ci.yml` is
deleted; see `docs/ci/README.md`.

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
