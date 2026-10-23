# Kalam redesign — repo verification & implementation audit

**Date:** 2026-10-22 · **Code base:** `2d7ebb0` (main) · **Design set:** `docs/design/`
(archived in `6d780a6`, verified byte-for-byte on receipt; one stale-reference fix in
`pdf-reader-spec.md` applied afterwards with the owner's approval).

This document is the builder's map from the approved design set to the current code.
It owns **code facts only** (what exists, where, and what each design element costs).
Appearance, tokens and behavior remain authoritative in `docs/design/`; product scope
remains authoritative in `ARCH.md` / `plan.md`. Where a spec's "notes for the builder"
asserts a code fact, this audit checked it — corrections are in §10.

Method: static audit of `src/`, `crates/kalam-reader`, `crates/chapbook-*`,
`resources/style.css`, `src/theme.rs`, `src/service.rs`, the SQLite schema and the
guardrail tests. The GUI was not run for this audit.

---

## 1. Owner rulings recorded 2026-10-22

These govern how the design set is to be read by any builder:

1. **The new designs supersede the previous designs.** Where the old app look and the
   design set disagree, the design set wins. In particular the 13 current dark
   palettes in `src/theme.rs` will be **replaced** by the new palette system — they
   are not to be preserved alongside it.
2. **Everything marked *Proposed* is approved.** Status labels in `design-system.md`
   and `decisions.md`, and "(draft)" in spec titles, are bookkeeping lag, not open
   approval. *TBD* items (specific values that were never written down, §9) are still
   unspecified numbers, not unapproved concepts.
3. **Terracotta is the structural canonical.** Other palettes were deliberately not
   boarded; they differ only in token values, never in layout. Builders implement
   every screen once against tokens and verify the other palettes by value
   substitution and contrast math, not by new design work.
4. **Designed-but-unbuilt features stay designed.** Several specs describe behavior
   that does not exist yet (§8). They were designed while the screens were being
   designed and will be scheduled later. Their presence in a spec is **not** a
   commitment to build them with the redesign, and the redesign must not silently
   drop them from the designs.
5. **The deep editor is design-only for now.** `editor-spec.md` and decisions
   118–143 are stored as design. The EPUB editor feature remains removed
   (`ARCH.md`, `plan.md`): no editor route, no picker, no in-reader editing
   (including the selection chip's "Fix typo", decision 26/118), and no new
   GtkSourceView dependency until the owner explicitly reopens the feature.
   The editor rail entry (decision 102) is therefore also deferred.

## 2. Classification legend

Every design element below is tagged with what implementing it touches:

| Tag | Meaning |
|---|---|
| `STYLE` | Tokens / `resources/style.css` only. |
| `UI` | Widget layout or new reusable component; data already reaches the page. |
| `DATA` | New or extended `LibraryService` snapshot/query; no schema change. |
| `ENGINE` | `kalam-reader` / Chapbook prefs or API, or PDF/comics canvas painting code. |
| `FEATURE` | New product behavior, schema, or file mutation. Per ruling 4 these are inventoried, not scheduled with the redesign unless the owner says so. |
| `GAP` | Not designed yet; the current UI is restyled only when its design arrives. |

Effort hints (S/M/L/XL) are relative within this redesign, assuming the §7
groundwork (tokens + components) lands first.

## 3. Executive verdict

The design set is implementable on the current architecture without replacing the
reading engines, the catalog, or the service layer. The work concentrates in four
places: (1) replacing the theme system (13 dark palettes → 4 palettes × light/dark,
decision 19 also re-bases the reader themes on them), (2) a component library the
current pages mostly lack, (3) re-laying-out the big page files
(`pdf_reader.rs` 5,878 lines; `reader/mod.rs` 2,953; `comics_reader/mod.rs` 2,855;
`settings.rs` 2,552; `book.rs` 2,294; `app.rs` 2,244), and (4) a small number of
`ENGINE` items where the reader paints (search match tints, PDF tint/invert, comic
image adjustments). Data readiness is better than expected: half-star ratings,
highlight color+style storage, author bibliography caching, series caching, smart
shelf rules, FTS5 content search, spaced-repetition review and word exports already
exist underneath the screens that need them.

| Designed area | Verdict | Dominant tags | Effort |
|---|---|---|---|
| Design tokens & themes (system §2–4, 11) | Replace `theme.rs` model; light mode is new | `STYLE` `UI` | M |
| EPUB reader (decisions 14–34) | Chrome redesign on existing engine; panels swap sides | `UI` `ENGINE` | L |
| PDF reader (decisions 52–76) | Chrome phase feasible now; page surgery & ink tools are `FEATURE` | `UI` `ENGINE` `FEATURE` | L (chrome) + XL (features) |
| Comic reader (decisions ~35–51) | Best-aligned already; panels + image pane are the delta | `UI` `ENGINE` | M |
| Book details (decisions 77–93, 102) | Data mostly ready; Impression is `FEATURE` | `UI` `DATA` | M–L |
| Series page (decisions 94–101) | New route; retires `series_float.rs`; data partial | `UI` `DATA` | M |
| Author page (decisions 104–117) | Data mostly ready; highlights row needs snapshot work | `UI` `DATA` | M |
| Deep editor (decisions 118–143) | **Design-only (ruling 5)** — excluded from build scope | — | — |
| Everything else (78 checklist items) | `GAP` — designed later; inventory verified in §6 | — | as designed |

## 4. Design system → code mapping

### 4.1 Palettes and modes (system §2, §11; decision 19)

- Current: `Theme` struct with 13 fields and 13 constants (`ONEDARK` …
  `AYU_DARKER`, `NORD`), all dark; `main.rs` forces `adw::ColorScheme::ForceDark`;
  stored under pref `ui.theme`; theme tests assert the darker-variant pairing.
- Target: a palette × mode model — Terracotta / Forest / Plum / Teal in light and
  dark (5 core tokens `bg`, `text`, `text-2`, `text-3`, `accent` plus derived
  `accent-fill`, `card`, `border`, `overlay`). The `Theme` struct, `css_variables`,
  the Settings picker and the theme tests are all replaced, not extended.
- Migration: old `ui.theme` ids are unrecognized by the new model; since every old
  theme was dark, first run after the change should default to Terracotta dark
  rather than surprising the user with a light window. `by_id` already falls back
  quietly — keep that behavior, change the default.
- Light mode is genuinely new work, not a value swap: remove `ForceDark` (use
  Adwaita `PreferLight`/`PreferDark`/follow-system so stock widgets agree with the
  stylesheet), and audit `resources/style.css` for rules that assume dark (cover
  placeholders, reader stage literals). The reader's Light/Dark/**System** switch
  (decision 23) needs the same plumbing.
- Reader themes (decision 19): `KalamTheme::{Light, Sepia, Dark, Ink}` in
  `crates/kalam-reader/src/prefs.rs` and `ReadingTheme` in `src/epub_book.rs` are
  replaced by palette+mode. Stored `reader.theme` values (`light/sepia/dark/ink`)
  need a one-time mapping (suggest: light/sepia → palette Light, dark/ink →
  palette Dark). The duplicated palette constants in `prefs.rs` ("when Kalam
  changes a colour, change it here too") should stop being duplicates: the app
  should pass the resolved `Palette` into `KalamPrefs` from its token table.
- Missing values to derive at token time (not design blockers, per rulings 2–3):
  `card` for Forest/Plum/Teal (only Terracotta dark `#241D19` is written); PDF
  paper tint for non-Terracotta palettes (system §13 says derive a light neutral
  from the hue); semantic status colors — the current UI uses
  `danger/success/warning/info` for toasts, badges and charts, while the design
  deliberately uses `accent` for destructive items (§17) and leaves status colors
  TBD (§7). Keep four internal semantic tokens defined per palette at token time
  so toasts/charts keep working; they are builder tokens, not a fifth accent.
- The stylesheet already references two undefined tokens, `@kalam_hover` and
  `@kalam_text_bright` (6 uses near the end of `resources/style.css`), which GTK
  falls back on silently. The token rewrite must define or delete them; the
  `variables_cover_every_token_the_stylesheet_uses` test pattern should be kept
  and extended to catch this class of error automatically.

### 4.2 Type, shape, fonts (system §3, §4)

- Radii (6/10/14/22/999), spacing scale and the type scale land in
  `resources/style.css` (shape lives there; colors only via `@kalam_*` names —
  the guardrail test `stylesheet_does_not_gain_hex_colours` keeps it that way).
  Token *names* will change with §4.1; that is the moment to also generate the
  type/spacing scale from `src/style.rs` constants instead of hand-writing
  numbers through 4,700 lines.
- Fonts split in two, and the two halves load differently:
  - **Book text** (Playfair Display 18) is engine territory: add the Playfair
    faces beside Literata in `crates/kalam-reader/fonts/` (compiled-in `Faces`,
    `BODY_FONT` in `prefs.rs`). Literata stays available in the reader font
    picker; `reader.font_family` already plumbs a user choice through
    `KalamPrefs`. Default size moves 17 → 18 px (`FONT_PX_RANGE` 13–24 already
    allows it); line-height default 1.75 vs 1.8 is an open question in the
    design set itself — pick one when the reader settings panel is built.
  - **GTK UI text** (Playfair Display + DM Sans) is Pango/fontconfig territory:
    the app currently registers no fonts at all (system fonts only). Bundling
    needs a delivery decision — install into the app's fontconfig search path at
    packaging time, or register the files at startup — to be prototyped in the
    token phase. Both families are OFL; license texts must travel with the
    files, as the Literata/Noto ones do today.
- Nav rail 68px with 44px circular items replaces the current 48px rail
  (`.kalam-sidebar`); the brand mark becomes `logo.svg` materialized from
  `logo-svg.md` (one `currentColor` path) instead of `assets/logo.png`
  (`brand_logo()` in `app.rs`). Check the hairline loop at 32px as the design
  itself warns. The splash (`src/splash.rs`) carries its own hardcoded CSS
  (`#0c110f`, `#35c08e`) because it paints before the catalog opens; the approved
  splash design's asset (`splash.html`) was not in the upload — restyle from it
  when it arrives.

### 4.3 Icons (system §7 TBD)

Boards use placeholder 1.8px stroke icons; the app uses symbolic theme names
(`NavItem::icon`, `LibrarySection::icon`) plus two custom SVGs written by
`icons::init()` (whose icon-theme rescan cost ~500ms is documented and deferred
to idle). Recommendation: keep symbolic theme names as the icon system (they
recolor via CSS `color`, exactly what the specs assume), map each placeholder
glyph to a symbolic name during the component phase, and add custom SVGs only
where no name exists (bookmark ribbon outline/filled, decision 16). Every icon
added through `icons::init()` inherits that rescan cost discussion — prefer
theme names.

### 4.4 Scrollbars (decision 90; system §24)

Target: no visible scrollbar anywhere; a 48px fade hints that a list continues.
Current code is mixed — `PolicyType::Automatic` (~23 sites), `Never` (~51),
`External` (~5), `Always` (~5) — plus the carefully bug-hardened hover-reveal
scrollbar CSS (see `src/style.rs` header and `docs/pitfalls.md` §5: never
`opacity < 1` on collapsible widgets, zero margin+border+padding). Moving every
scrolled widget to `External`/`Never` deletes most of that CSS and its traps;
wheel, touchpad and keyboard scrolling are unaffected. Two builder notes: the
fade hint is an overlay gradient widget per scrolled area (a shared helper, not
per-page code), and long lists lose position feedback — accepted by the design,
worth one screenshot review on the longest list (All books) during the shell
phase. The pitfalls document stays as the record of why the old code looked
like that.

### 4.5 Motion (system §9; decisions 15, 23)

The specs define motion in text (250ms hover delay, 220ms Revealer slides, 300ms
grace, reduced-motion fades). All of it is `Gtk::Revealer` + timers, and the
readers already run hover-zone/timer state machines
(`chrome.rs::connect_hover_zone`, `TopEdgeHover/…HideTimerTick` in the PDF
reader), so this is re-tuning existing machinery, not new infrastructure.
Reduced-motion should read the GTK/Adwaita animation setting rather than a new
app pref.

### 4.6 Window sizes

Boards are 1280×800 (editor 1440×860, design-only); the app window defaults to
1100×720 (`app.rs`). Two 340px reader panels plus a 620px column do not fit
narrow windows side by side — panels float over the page by design, which
absorbs most of it, but the book/author pages' fixed 56px margins and two-column
sections need a narrow rule. Narrow behavior is explicitly undesigned
(`GAP`, checklist §9); implement a single-column collapse as a builder pattern
and surface it for design review rather than inventing per-page rules.

## 5. Screen-by-screen audit (designed screens)

### 5.1 EPUB reader — `epub-reader-spec.md`, decisions 14–34

Code: `src/pages/reader/*` (8,000 lines across 10 files) over
`crates/kalam-reader` (`ReaderView`, a `GtkDrawingArea` blitting
tiny-skia pixmaps; Chapbook `Session` underneath). Painting stays in the engine:
the app already controls paper/ink/selection via `KalamPrefs` → engine
`Palette` + `skin_css()` (real CSS consumed by stylo), which is where the new
reader tokens land (§4.1). No part of this redesign requires moving rendering
into the app.

Exists and maps directly: paged mode; continuous scroll (`reader.scrolled`,
*undesigned visually* — keep behavior, apply new chrome); facing-page spreads in
the engine (undesigned in the boards, `GAP` for its chrome); TOC via
`ReaderView::toc()`; whole-book search (`ReaderView::search`, capped 500) with a
search bar already in `mod.rs`; selection chip (colors + Note + Define in
`engine.rs`); annotation popover data (annotations store `color` + `style`, and
Chapbook paint already implements Solid/Underline/Squiggly/Strikeout/Dotted);
dictionary popover over `src/dict.rs`; saved-word dotted underlines + hover
cards via `set_word_memory` (scope pref `reader.word_memory_scope` exists);
editable keybinds with conflict handling (`keybinds.rs`, decision 24 to verify
against "refused, names the owner"); image lightbox (`connect_image_tap`,
undesigned, keep); Bubbles/split reading (`src/bubbles.rs`) behind the Back
pill's Bubble peek — its own design pass is still pending (`GAP`), restyle the
peek button only.

Deltas, by tag:

- `UI` **Panels swap sides and merge.** Today: left = TOC + Settings
  (`LeftSidebarTab`), right = Highlights + Bookmarks + Words
  (`RightSidebarTab`, `panels.rs`). Design: left = Contents + Notes (Highlights/
  Bookmarks/Words as filter chips inside one Notes tab), right = Settings only
  (Reading + Shortcuts). This is the single largest reader change; it also
  retires the third settings surface below.
- `UI` **Delete the UI-tuning subsystem** (decision 18): `ui_prefs.rs` (604
  lines) implements exactly the 12 chrome-tuning numbers the design removes
  (sidebar widths/radius/shadow, bottom-pill and back-chip sizes, dim strength)
  plus a runtime CSS provider. Delete the settings pane, the prefs keys become
  orphaned (harmless; stop reading them), delete the bottom pill.
- `UI` Pure-page chrome: hidden Back pill + Bubble slide-out (generalized by
  system §23 — build once as the shared "slide-out pill" component; the book
  page's Next pill reuses it), bookmark **ribbon** (today bookmarks are panel
  rows; ribbon is a page-overlay toggle reflecting "a bookmark at this
  position"), location text with hover chapter title.
- `DATA` Location text must name the TOC chapter (decision 17 records the bug:
  today the number is spine/file order from `connect_position`). The TOC is
  already loaded for the panel; map spine index → deepest containing TOC entry
  in the page, no engine change.
- `ENGINE` Palette-driven selection tint + accent handles (decision 25:
  replaces the fixed rose tints and `#ffd166` dark handles in `prefs.rs`);
  highlight colors change values (decision 27: `#E8B93A`… at ~30% alpha vs
  current `#f4d35e`… at ~64%) — stored annotations keep color *names*, so this
  is a presentation change in `HighlightColor::css()`, plus a migration
  question for rows stored with the old "underline" *color* (decision 28 drops
  it in favor of the Underline *style*: map `color=underline` →
  `style=underline` with a default color, in the same change).
- `ENGINE` Search match painting: today search returns results and the app
  jumps/selects; painting *all* matches faint + current accented (decision 34,
  system §9) needs transient match ranges in the paint pipeline. The host-
  highlight mechanism (`host_highlights.rs`) is the natural carrier — additive
  API on `ReaderView` (match tint separate from annotation tint). Chapter/Book
  scope (Chapter default) is app-side filtering of the same results (`UI`).
- `ENGINE` Note dot at the end of a highlight span + hover preview (decision
  29): paint + hit-testing addition; small, but engine-side.
- `FEATURE` Undo for deletions (decision 30): no deletion-undo stack exists
  (the existing "undo" is navigation jump-back). Session-scoped stack over
  annotation/bookmark/word deletions; catalog already supports re-insert.
- Deferred with the editor (ruling 5): "Fix typo" chip action. Ship the chip
  without it.
- Setting removal: `dict_sense_hint` exists in code and in the reader Settings
  UI (`settings_panel.rs`, `mod.rs`); decision 32 removes the toggle and the
  "likely here" tag from the popup. Hide the toggle, stop rendering the tag;
  the pref and lookup-order code can stay dormant pending the word-sense
  proposal (not in this design set).

### 5.2 PDF reader — `pdf-reader-spec.md`, decisions 52–76

Code: `src/pages/pdf_reader.rs` (5,878 lines, the largest file in the app) over
MuPDF in `src/pdf.rs`; pages are `gdk::Texture`s in `GtkPicture`s — this reader
*is* app-painted already. It shares **no** chrome code with the EPUB reader
today (its own hover timers, sidebar, search), which is why the unified-chrome
design is mostly a convergence project here.

Exists: spread modes (`PdfSpreadMode` odd/even/none, cover-alone pairing ≈
design's "Cover on its own"), four scroll modes (page/vertical/horizontal/
wrapped — a superset of the design's Paged/Scroll; map, don't delete), spread
gap, smart crop (`calculate_ink_box_for_image`, `SetSmartCrop` — the design's
"Trim margins"), full-document search + snippets popover, text selection over
both vector text and OCR (`PdfPageText`, `page_ocr_cache`, OCR worker already
wired — the checklist's `[~]` OCR flow is a *design* remainder, the pipeline
exists), text annotations (`SaveAnnotation { color, style, note }` into the
same `annotations` table), `LookUpWord`, bookmarks, a single sidebar
(Toc/Bookmarks/Settings) with `sidebar_pinned`, zoom OSD, `JumpToPage`.

Deltas:

- `UI` Two floating panels (Contents/Pages/Notes left; Reading/Image/Shortcuts
  right) replace the single sidebar; pin behavior already has a state bit to
  build on. Back pill/ribbon/location converge with §5.1 components — this is
  where shared reader-chrome components pay for themselves.
- `UI` Pages thumbnail grid (no thumbnails exist today; MuPDF rendering at low
  scale into the existing texture cache, `GtkGridView` lazy — the design's own
  GTK section says the same). Without Edit mode (below) it is display + jump.
- `UI` Page-jump field on `G` (message exists), search results into the left
  panel (today: popover), Notes tab from existing annotations/saved words
  (`DATA`: ReaderSnapshot is EPUB-shaped; add a PDF variant or generalize).
- `ENGINE` Image tab Tint/Invert (decisions 55): no color-matrix step exists in
  the render path; add at texture creation (tint toward palette paper,
  `#CFC6BB` Terracotta dark) — files untouched, as designed. Saved-word dotted
  underlines on PDF text need word geometry overlays from `PdfPageText`
  (positions exist; painting is new).
- `FEATURE` **Pages Edit mode** — reorder/rotate/extract/delete/insert writing
  the PDF copy: `src/pdf.rs` exposes no page-mutation or save API at all. This
  is a MuPDF editing feature with staged undo, annotation/bookmark re-pointing
  (decision 69) and atomic file+metadata writes. Inventoried per ruling 4; not
  part of the chrome redesign.
- `FEATURE` **Drawing annotations** (pen/highlighter strokes, shapes, text
  boxes, sticky notes, eraser): no stroke model, storage or painting exists.
  The spec says store them "in `kalam.json` against a stable page ID" — that
  contradicts the repo rule (SQLite is the source of truth; `kalam.json` is a
  backup snapshot, `docs/WORKING.md`). If scheduled later, design storage
  DB-first with sidecar export, and define the stable page ID (MuPDF has no
  durable per-page identity across the file rewrites Edit mode performs — the
  two features interact and must be designed together).
- `FEATURE` Password-protected PDFs: no authenticate path in `pdf.rs` today
  (open simply fails). MuPDF supports it; needs API + the designed dialogs.

Recommended split: chrome phase = panels, ribbon/location, search-into-panel,
page-jump, thumbnails (view-only), Image tab. Everything else stays designed
and undeferred only by an explicit feature decision.

### 5.3 Comic reader + comics series page — `comic-reader-spec.md`

Code: `src/pages/comics_reader/` (2,855-line `mod.rs`) and the hub
`src/pages/comics.rs`; data in `comic_series`/`comic_chapters` tables with a
first-class `ComicSeries` model — separate from book series (§5.6), in code as
in the designs.

Exists and aligns unusually well: Single/Double/Long-strip page styles,
LTR/RTL/Webtoon direction (RTL spread order swap implemented), spread gap, four
fit modes, zoom OSD, categorized settings panel, header with cover/progress.
The reader chrome redesign here is mostly adopting the shared Back
pill/ribbon/location/panels (`UI`, S–M).

Deltas:

- `UI` Chapters + Pages panel tabs: chapter data (`chapters_for_series`) and
  page counting exist; thumbnail grid + sticky heading are new widgets (share
  the PDF Pages grid work where possible).
- `ENGINE` Image pane: brightness/contrast-style adjustments and Crop-blank-
  borders do not exist for comics (the PDF smart-crop ink-box in `src/pdf.rs`
  is reusable analysis; draw-time adjustment of `GtkPicture` content needs a
  paintable-level solution — snapshot with a color matrix or pre-adjusted
  textures, cached per page/settings).
- The comic spec's §3 "Series page" (band header, chapter list) is the
  *comics* series page (decision 44, kept aside). Today's `Route::ComicSeries`
  renders through `comics.rs` grouping; whether it adopts the new general
  series layout is an open design question — do not merge the two series pages
  in code until that is answered.
- The remaster dialog (`widgets/remaster_dialog.rs`) exists and is presented
  from comic book pages; the inventory struck it as "not a screen" (decision
  36). It keeps working and gets restyled by the shared dialog components;
  no design work needed.

### 5.4 Book details page — `book-details-spec.md`, decisions 77–93, 102

Code: `src/pages/book.rs` (2,294 lines; hero + "Reading journey" card + author
card) fed by `BookPageSnapshot`; floats in `book_float.rs` (opened from grids:
click → float, Ctrl+click → full page, per `widgets/book_row.rs`).

Exists: half-star rating end-to-end (`rating` 0..=10, `set_book_rating`,
star picker redraws from it — the designed custom rating widget replaces the
picker's painting, not the model); Finished toggle (`set_book_finished`,
`finished_at`); reading-list membership; shelf menu flow
(`ShowShelfMenu` → shelves float today; design moves it to a ⋯ submenu,
decision 86 — same underlying shelf operations); tags via a tags float today
(design: Tag dialog with rename/merge/delete — tag rename/merge need checking
against `db/tags.rs` when built, delete exists library-wide in the tags page);
annotations list + full dialog entry (`ViewHighlights`); chapter-titles parse
on a worker already feeding the journey card — the designed Contents timeline
is that card's successor, needing per-chapter read state and bookmark counts
(`DATA`: derive from progress + `reading_bookmarks`; chapter *dates* on the
timeline have no stored source today — reading events are per-book, not
per-chapter; either derive "read by" ordering without dates or accept the
design's date only where an event boundary proves it — flag at build time);
file-missing and files-only states have data (`file_size`, metadata presence).

Deltas:

- `UI` The whole layout (cover+progress rail, header block, outlined Sections,
  timeline with custom-drawn gutter, slide-out Next pill — §23 component).
- `DATA` Next-in-series: needs next title + in-library flag. Owned ordering
  exists (`books_in_series`, `series_index`); unowned next comes from
  `series_cache` (`SeriesWork { title, year, cover_path, author }`) — add to
  `BookPageSnapshot`.
- `DATA` Annotations dialog Words tab: saved words are per-book queryable
  (ReaderSnapshot already loads them); reuse at book scope.
- `FEATURE` **Your impression** (stars exist; the note, reaction chips,
  finished-date line and skipped-prompt do not): no impression/reaction
  storage exists. Small schema addition (one row per book; re-reads question
  in the design set is still open) + sidecar field. Schedule separately; the
  page layout must work without the box (design already specifies that).
- `GAP` Book float: explicitly not redesigned and "may now disagree with the
  new page." Keep it, restyle by tokens, revisit when designed. Note the float
  is the *primary* click target from every grid, so token-level restyling is
  not optional polish — it is most of what users see.
- The ⋯ menu's "Book edits" badge (decision 86) counts editor edits; with the
  editor design-only (ruling 5) the badge and Edit-book entry are omitted
  until reopening. Legacy `patches` rows remain read-only compatibility data.

### 5.5 Author page — `author-page-spec.md`, decisions 104–117

Code: `src/pages/author.rs` (1,096 lines; hero + facts + stat discs + series
cards) over `src/author.rs` + `author_profiles` cache. Data readiness is high:
`AuthorProfile` carries bio, birth/death dates, photo file, works, aliases,
`fetched_at` and source URL (the designed footer "updated N days ago" is a
format of `fetched_at`); Open Library fetch+cache with photo and work covers
exists (`fetch_and_cache_author`); `works_not_in_library` implements the
Not-in-library matching; `SeriesProgress { owned, missing }` groups owned
books; `status_counts` gives read/reading/unread for the facts panel's
segmented bar.

Deltas:

- `UI` Facts panel as outlined section, series cards with cover **fan**
  (overlay of three pictures, no rotation — as the spec itself specifies),
  open-series panel (overlay anchored to the card, sideways-scrolling covers),
  highlights sliding row, quiet notice line. All new components (§7), no new
  data except below.
- `DATA` Highlights row + author-scope Annotations dialog: a helper exists but
  is explicitly unused (`AuthorQuote` / `saved_quotes_for_books` — "not built
  yet" in code). Extend `AuthorPageSnapshot` with author annotations (grouped
  by book) instead of page-side catalog reads.
- `DATA`/`UI` Create-shelf dialog: smart rule "author is X" is representable
  today (`RuleField::Author` in `shelf_rules.rs`); "author already has a
  shelf" detection (Open shelf vs Create) needs a rule/name lookup in the
  snapshot. Name-linked-to-author switch is dialog-local state.
- `FEATURE`/`GAP` Edit author and Merge into another author (decision 112):
  aliases table exists, merge semantics and both dialogs are undesigned in
  the set and unbuilt in code. The ⋯ menu ships without them (or disabled)
  until designed. Fetch (+) on missing works stays disabled (Part 2, decision
  109) — the design already says so.
- Authors index page + rail entry (decision 117): no route exists
  (`Route::AuthorPage` only). `GAP` (own design chat per the inventory).

### 5.6 Series page (books) — `series-page-spec.md`, decisions 94–101

No such route exists today: book series are a `series` string + `series_index`
on `Book`, rendered via `series_float.rs` (616 lines, explicitly retired by the
spec §8) and the book page's series card. New `Route::SeriesPage { name , author? }`
(shop for the exact key: `series_key(series, first_author)` in `db/series.rs`
is the existing normalized identity — reuse it).

Data: owned books + order (`books_in_series`), unowned works with year/cover
from `series_cache`, per-book progress/finished for segmented progress and
strip badges — all present. Gaps: unowned-book *descriptions* and a series
summary are not in `SeriesWork` (title/key/cover/year/author only) — the
header summary and focused-row description for missing books need either a
metadata enrichment fetch (Open Library work record, as `metadata/series.rs`
already parses search docs) or graceful absence (`DATA`, decide at build).
"Book N of M" total comes from cache length vs owned indexes — spec §8 already
warns not to count library books. Strip/List per-series memory is one new pref
keyed by series (`DATA`, trivial). The strip itself is the one genuinely
custom widget (two cover sizes, snap focus, keyboard Left/Right/Enter/Esc,
sideways scroll keeping focus visible) — build it as a component with the
cover widget shared with §5.4/§5.5.

Entry points to rewire when it lands: book page series link (decision 81),
author page "Open series ›" (decision 108), and every current
`OpenSeriesFloat` caller in `app.rs`.

### 5.7 Deep editor — design-only

Per ruling 5: nothing in `editor-spec.md` is in build scope. For the record,
the audit confirms the starting point if it is ever reopened: no editor code,
route or GtkSourceView dependency exists; `epub_writer.rs` and
`epub_sanitizer.rs` cover import-time rewriting only; legacy `patches` storage
is inert. Editor-only design tokens (`diff-add`, flat tabs §33, DM Mono) are
not adopted by other screens unless a designed screen references them.
`Proofread` (decision 129) has no entry point anywhere until then.

### 5.8 Settings — `settings-spec.md`, decisions 144–158 (added 2026-10-23, design chat 8)

Code: `src/pages/settings.rs` (2,587 lines, 6 tabs) over
`SettingsSnapshot` + a page-owned FS worker. Data readiness is high — the
spec's facts were taken from this file, and every pane's backend already
exists: theme choice (`ui.theme`, palette + Light/Dark/System) in
`theme.rs`; libraries registry + `restart_now` in `libraries.rs`; sidecar
survey/backfill; `catalog.backup_to`; reader cache size/clear in
`paths.rs`; exports in `export.rs`; sanitizer/writeback toggles; EPUB
backup list/delete in `epub_metadata.rs`; dictionary priority column,
`move_dictionary_priority`, bundled-pack markers (`bundled_dictionary_*`
prefs) and `install_bundled_dictionaries`; metadata source toggles and
Google Books key/country prefs; notification history (`notify::history`,
200 entries, `HH:MM` stamps). Prefs split per-library (DB `app_prefs`)
vs global (`~/.config/kalam/prefs.json`) by `is_global_pref`; `ui.theme`
is the only `ui.*` key today, so `ui.` joins the global prefixes for the
new Appearance/Shortcuts prefs.

Deltas:

- `UI` Page rebuilt: 11 panes in 5 groups, 240 pane list + 640 column
  (§41). New components: dropdown (§43 — built on the codebase's §17
  popover machinery rather than `Gtk.DropDown`, whose list cannot do
  own-font rows plus a footer action; the visual spec is what binds),
  palette mini-page cards (§42, Cairo), key-recording rows (§44;
  `key_chip` already exists). Existing `setting_row`/`segmented` widgets
  are reused.
- `UI`/`DATA` Appearance (144–146): palette+mode is one choice — the
  picker stops offering System (stored `*-system` prefs keep parsing and
  display as their resolved mode). Interface/heading font and text size
  ride the theme CSS channel (`install_css` gains a generated override
  block: base family, display-class families, the `.kalam-ui-*` type
  table scaled). Page-specific literal sizes outside the type table do
  not scale in this pass. Reduce motion stores System/On/Off and drives
  `gtk-enable-animations` plus a helper for new transitions; existing
  revealers are not retrofitted in this phase.
- `DATA` Reading (147): the pane edits the existing global `reader.*`
  prefs (typeface/size/line-height/column/justify/word-memory) with a
  live preview; "Ask for an impression" stores a new pref ahead of the
  impression feature. The reader-side per-book switch is not drawn and
  belongs to the reader phases, as the spec itself defers it.
- `UI` Rail bell + notification panel (148): `notify::Entry` gains a day
  stamp (Today/Earlier) and an unseen counter for the dot; the Settings
  Notifications pane is removed, per the decision.
- `UI`/`FEATURE` Shortcuts (155): the binding registry, recording and
  conflict rules are new; actions that exist (navigation, Back,
  Settings, bell, fullscreen, quit, search, import) wire to the shell,
  the rest store bindings ahead of their features (no global undo stack
  exists for Ctrl+Z). Keys are placeholders until built (decision 76's
  precedent).
- `GAP` by design: file/folder choosers and Export dialogs are not
  drawn (existing flows keep working behind the new buttons);
  Integrations ships fully disabled (154); the reader Theme card
  redraw (145) waits for the reader boards.
- Implementation note (Settings tranche 2b, owner confirmation
  wanted): decision 151's watch-folder rows have no place for the
  legacy per-folder `enabled` pause or the per-format `import_*`
  toggles. The build keeps the pause as one extra “Watch this
  folder” switch — otherwise removing the folder would be the only
  way to stop it — and drops the per-format toggles, treating the
  shelf-routing choice itself as enabling that format. Removing the
  extra row to match the board exactly first needs a drawn answer
  for how a paused folder is represented.

## 6. Verified inventory of everything else (not yet designed)

`kalam-screens.md` was written without code access; the table below is the
repo-verified reality for the remaining surfaces, so future design chats can
trust it. "Snapshot" = a `LibraryService` snapshot already feeds the page.

| Surface | Code today | Data path | Notes vs inventory |
|---|---|---|---|
| Shell / nav rail | `app.rs` (2,244) + `NavItem::ALL` = Home, Library, Shelves, Comics, Settings | — | Inventory's rail (Home, Library, Editor, Settings) is wrong today: Shelves & Comics are shown; Downloads/Browse/Fanfic exist as hidden items with working routes. Final rail composition is `GAP` (draft only). |
| Splash | `splash.rs` (97), own CSS | — | Exists; approved design's `splash.html` missing from set. |
| Toasts + history | `notify.rs` (attach/success/error/info/activity + in-memory history) | — | Both inventory items exist. |
| Task manager | `pages/task_manager.rs` + sidebar badge button in `app.rs` | tasks registry | A route (`LibrarySection::TaskManager`) *and* a sidebar dialog; inventory lists it only as cross-cutting. |
| Home | `home.rs` (573) | `HomeSnapshot` | A richer `DashboardSnapshot` (now-reading, feed, quotes, lookups) also exists and feeds Library — two overlapping snapshot concepts to reconcile when Home is designed. |
| Library hub | `library.rs` (1,005) | `DashboardSnapshot` | Sections route via `LibrarySection` (10 sections). |
| All books | `all_books.rs` (1,597) + `book_row::build_book_grid[_selectable]` | `AllBooksSnapshot` + search syntax in `db/search.rs` | Grid only; no list view. Windowed loading is inside `book_row.rs`. Bulk-select exists (selectable grid + bulk delete confirm). |
| Full-text search | `content_index.rs` (FTS5) | `ContentSearchSnapshot` | Exists as designed-against in inventory. |
| Shelves grid/detail/editor | `shelves_grid.rs`, `shelf_detail.rs`, `shelf_editor.rs`; rules in `shelf_rules.rs` | `ShelvesSnapshot`, `ShelfDetailSnapshot`, pickers | Rule fields/operators match the inventory exactly (tag/author/series/format/progress/title/added). |
| Reading list | `reading_list.rs` | `ReadingListSnapshot` + picker | Ordered, reorderable. |
| History | `history.rs` | `HistorySnapshot` (reading_events) | Grouping/filtering in page. |
| Tags + tag books | `tags.rs`, `Route::TagBooks` | `TagsSnapshot`, `TagBooksSnapshot` | Usage-weighted cloud in page. |
| Analytics | `analytics.rs` + `widgets/charts.rs` (Cairo) | `AnalyticsSnapshot` (goal, week, finished_this_year) + stats queries | Charts are Cairo-drawn today; a redesign restyles via tokens + redraw, matching the progress-ring note (§4 TBD → Cairo widget). |
| Saved quotes | `saved_quotes.rs` | `QuotesSnapshot` | Page exports quotes to `~/Quotes.md`; the all-data Markdown export is `export.rs`. |
| Saved words / Review | `saved_words.rs`, `review.rs` (SM-2-style flashcards) | `WordsSnapshot` | CSV + Anki + Markdown exports exist (page messages). Review is a full screen, not just filters. |
| Lookup history | `lookup_history.rs` | `LookupHistorySnapshot` | Includes misses (`dict_lookups`). |
| Comics hub | `comics.rs` (1,301) | comic series queries | Groups local series + remote placeholders; `Route::ComicSeries`. |
| Metadata editor | `metadata_editor.rs` (1,156) | Open Library + Google Books (`src/metadata/`) | MyAnimeList / Comic Vine providers do **not** exist — inventory's metadata-chat sources list is aspirational (`FEATURE` if pursued). Locked hand-edited fields: `metadata_overrides` table exists. |
| Settings | `settings.rs` (2,552), 6 tabs: Appearance, Storage & Backup, Dictionaries, Book Files, Metadata Sources, Notifications | `SettingsSnapshot` + FS worker | Designed in chat 8 (decisions 144–158) — see §5.8. The old draft-pane names (Reading, Integrations, Keyboard Shortcuts) are now real panes; watch folders & libraries move out of Storage/Book Files into their own. |
| Dialogs | `widgets/in_app_dialog.rs` (scrim, focus trap `focus_trap.rs`), 23 present-sites | — | The designed dialog (420/560, decision 75) maps onto this component + tokens; one restyle propagates widely. Native file choosers remain native. |
| Browse / Downloads / Remote | `browse.rs`, `downloads.rs`, `remote_detail.rs`, `sources/` scaffolding | remote tables | Part 2: routes and queue exist, no registered source (README/ARCH agree). Stay hidden. |
| Placeholder | `placeholder.rs` | — | For hidden modules; disappears with Part 2 decisions, not redesign. |

## 7. Groundwork: separating the UI layer (before/with page work)

The owner is still designing the remaining screens; the point of this order is
that each finished design drops into a prepared slot. The codebase is already
partway there — `Catalog` → `LibraryService` snapshots → pages, heavy work in
`tasks.rs`, guardrail ratchets in `tests/guardrails.rs` — so groundwork
*finishes* seams instead of inventing them:

1. **Tokens first** (§4.1–4.2): new `Theme`/palette model, mode handling, pref
   migrations, generated type/spacing/radii, fonts + logo. Every later phase
   consumes this; nothing else should start before it compiles.
2. **Component library in `src/widgets/`**, one file per component, plain data
   in / messages out, each owning its CSS classes: pill/ghost/icon buttons,
   slide-out pill (§23), chip, segmented/tab pills (§10/§22), switch/slider
   rows, inset card (§9), outlined section (§14), menu/submenu (§17, custom
   popover list — not `Gio.Menu` — as specs direct), dialog on
   `in_app_dialog` (§13/§18), rating (§16, custom-drawn), segmented progress
   (§19, custom-drawn), cover widget + fallback + missing + finished badge
   (§20), timeline gutter (§15, Cairo), state card/quiet notice (§10/§30),
   reader side-panel shell (§9) and selection-chip shell. Grow
   `book_row.rs`'s grid into this library rather than beside it.
3. **Finish the service boundary per page touched**: pages that still hold
   `Arc<Catalog>` (ratcheted in guardrails) move their reads into snapshots
   *as part of* that page's redesign commit, lowering the ratchet each time —
   no separate big-bang refactor.
4. **Screenshot loop**: extend `docs/ci/screenshot.sh` routes per redesigned
   screen (it seeds 139 books and captures routes headlessly today) and keep a
   small component-preview route (hidden, like `placeholder`) rendering every
   widget in every state for both modes — the GTK equivalent of the designs'
   Components artboard.
5. **Reader chrome convergence**: extract the Back pill/Bubble peek, ribbon,
   location text, panel shell and search bar into shared components used by
   all three readers *before* restyling the second reader; today the three
   readers share almost no chrome code, and doing this once is the difference
   between three reader projects and one plus two adaptations.

Suggested build order after groundwork (data-readiness and component reuse,
not screen importance): shell → book details (proves components on ready
data) → series page (new route, retires float) → author page → EPUB reader
(panel surgery + theme/prefs migration) → comic reader chrome → PDF reader
chrome. Settings' Appearance tab lands with the token phase (it *is* the
palette picker); the rest of Settings waited for its design, which
arrived in chat 8 (decisions 144–158) and is now the active phase (§5.8).

**Status (2026-10-23):** items 1 and 2 are complete and CI-green (tokens
`5fbd12b`; component library through `c9d43cb`, exercised by the hidden
`KALAM_ROUTE=components` gallery). Item 4's loop is in place: CI photographs
the gallery in Terracotta Light and Dark on every push and commits the
settled frames to `ci-logs/components-terracotta-{light,dark}.png` (see
`docs/ci/README-screenshots.md`). Its first captures found — and `7b37de5`
fixed, verified by re-capture — stretched selection-chip/chip labels,
ellipsized cover kind tokens, and fixed-size covers inflated by wrap
labels' one-line natural width. Items 3 and 5 remain, done per page/reader
as scheduled above. The shell phase is also complete (`9870baf`): the rail
is 68px with 44px circular items and the accent-fill active state, the Back
pill matches the page specs, and the brand mark re-rasterizes on theme
changes via `theme::on_theme_applied`. Its infinity loop was then
strengthened slightly (`fb75f9d`) and verified in capture at 32px in both
modes. Book details is complete and CI-green: the page (`b786ab6`, layout
correction `29db77b`), the Annotations dialog (`ffcee6a`), and the Tag
dialog (`bad84b8`). The screenshot loop now photographs the page plus both
dialogs in Terracotta Light and Dark — populated tag chips/ticked rows on
seeded book 4 and the Annotations empty states — and `9417679` fixed the
oval highlight-filter dots those first dialog captures exposed. The
impression feature remains separately scheduled, and the app-level shelves
float remains for its book-float entry point. The series page is complete
and CI-green (`a8f7e1f`): `Route::SeriesPage` and `src/pages/series.rs`
replace the retired series float, fed by `LibraryService::series_page`
merging the cached Open Library listing with owned books — missing books
and the "of M" total come only from the listing, never from counting the
library, and the book page's series link now routes to it (comic series
keep `Route::ComicSeries`). The screenshot loop photographs the Strip in
Light and Dark and the List in Light on a seeded four-book fixture
(finished / 42% / unread / missing, `--series-fixture` in the seeder); its
first captures exposed a wrong seeded `series_cache` key, covers
top-aligned instead of bottom-aligned (no `vexpand` for the strip slot's
align to act on), the compact generated cover's ~107px minimum height
inflating 36×50 list thumbnails, and cold deferred thumbnails under the
view hook — all fixed in `288eed5` (the cover fix is a micro form of
`fallback_cover`: kind token only below 80×110, same threshold as the
missing face). No series summary or missing-book descriptions are shown:
no source exists — the §5.6 build-time decision, taken as graceful
absence. Next in the order above is the author page, whose "Open series ›"
entry (decision 108) now has its target route. The author page is
complete and CI-green (`8dc632b`, fixes to `029cdd6`): `src/pages/
author.rs` is rebuilt on `LibraryService::author_page`, whose snapshot
merges the cached bibliography with owned books into standalone-first
groups (duplicate editions collapse; unmatched owned books join their
file-metadata series), plus chunked author-scope marks (annotations,
bookmarks, words at 500-id batches) and finished ids. Opening the page
never fetches (decision 110) — the bibliography is whatever Find
metadata last cached, the photo's existence is settled on the worker,
and a missing bibliography degrades to the quiet notice, not an empty
chart. The old `works_not_in_library` / `series_progress` /
`status_counts` chain and `charts::stars_label` are deleted with their
last caller. Both this page's dialogs are built: Create shelf (§31,
author-linked name via the `author.shelf.<normalized>` pref) and the
author-scope Annotations dialog (decision 115, grouped by book). Edit
author and Merge stay out of the ⋯ menu — their dialogs are undesigned
(§5.5), following the Edit book precedent. The screenshot loop gained
`--author-fixture` (the series fixture's author, cached bibliography
plus one mark of each kind) and the `author-book-<id>` route; five
captures (page Light/Dark, panel, shelf, annotations) exposed and
closed: GTK4 clamping the ⋯ host's negative margin (now a CSS
`translateY` into the Back band — the popover's anchoring under that
transform is the one thing headless shots cannot verify), the plain
ghost pill kind being borderless by design (boxed ghost instead), a
lone grid card stretching full-width, a phantom bio "More" from
measuring ellipsis before the label's width settled, and the panel
anchoring to an unallocated card — idle callbacks burn through any
retry cap before the first frame, so both waits now ride the frame
clock. Comics credits still open the comics page's own author
handling; routing them here (decision 116) belongs to the comics
chrome phase. Next in the order above is the EPUB reader.

## 8. Designed-but-unbuilt features (inventory only — ruling 4)

Recorded so they are neither built by accident nor designed away:

| Feature | Where designed | Missing underneath |
|---|---|---|
| Book impression note + reactions | book spec §2–3, decision 91 | storage (schema + sidecar), dialog logic |
| Deletion undo (reader) | decision 30 | session undo stack |
| Search match tinting (EPUB) | decision 34, §9 | transient ranges in paint pipeline |
| Note dot + hover on highlights | decision 29 | paint + hit-test in engine |
| PDF Pages edit mode (reorder/rotate/extract/delete/insert) | pdf spec §3, decision 69 | MuPDF page-mutation API, staged apply/rollback |
| PDF ink annotations (pen/shapes/text/sticky/eraser) | pdf spec §3, decision 66 | stroke model, storage (DB-first, see §5.2), painting |
| PDF password open | pdf spec §4 states | MuPDF authenticate + dialogs |
| PDF Image Tint/Invert | decision 55 | color-matrix step in render path |
| Comic image adjustments + blank-border crop | comic spec §2 Image pane | draw-time adjust + cached crop analysis |
| Authors index page | decision 117 | route + page (design pending) |
| Edit author / Merge authors | decision 112 | dialogs undesigned; merge semantics unbuilt |
| Fetch missing works (+) | decision 109 | online sources (Part 2) — stays disabled |
| Find-metadata shared dialog + MAL/Comic Vine | inventory §4, decision 48 | dialog undesigned; two providers unbuilt |
| In-reader edit / Fix typo | decision 118 | gated on editor reopening (ruling 5) |

## 9. Still unspecified in the design set (values, not approvals)

To be derived or decided inside the relevant build phase, per rulings 2–3 —
listed so nobody blocks on them or invents them silently:

- `card` values for Forest/Plum/Teal; PDF paper tints for non-Terracotta
  palettes (derive by the stated rules, review in screenshots).
- Semantic status colors for toasts/badges/charts (§4.1).
- Reader body line-height default, 1.75 vs 1.8 (open in the design set).
- Icon set: symbolic-theme mapping proposed in §4.3.
- Narrow-window behavior for book/author/series pages and dual-panel readers.
- Final rail composition (which entries, given Shelves/Comics exist and the
  Editor entry is deferred).
- Book float redesign; Bubbles design pass; EPUB continuous-scroll, spread and
  light-panel boards (structure follows tokens meanwhile).

## 10. Corrections to design-set builder notes (verified against code)

- `epub-reader-spec.md` §6: "The README says tap any word to look it up" —
  already false: the current README and code use explicit selection/Dictionary
  action; taps open Word Memory cards only. No deletion needed.
- `epub-reader-spec.md` §6: `dict_sense_hint` — exists as a real toggle with
  lookup-order behavior (§5.1); decision 32 is implemented by hiding the
  toggle and the tag, not merely relabeling.
- `epub-reader-spec.md` §6 / decision 17: chapter number from file order —
  confirmed mechanism (`connect_position` reports spine index); fix is
  app-side TOC mapping as recorded in §5.1.
- `pdf-reader-spec.md` §6: "stored in `kalam.json`" — superseded by repo rule
  (SQLite source of truth, sidecar is a snapshot); see §5.2. Page-turn
  progress likewise never rewrites the sidecar.
- `author-page-spec.md` §7: "Never fetch on open" — matches current behavior
  (cache-then-fetch exists behind explicit actions); the snapshot extension in
  §5.5 must preserve it.
- `book-details-spec.md` §7: "reuse book.rs data (progress, notes, shelves,
  edits)" — progress/notes/shelves yes; "edits" refers to legacy patch data
  the app no longer creates or applies; there is nothing to reuse until the
  editor is reopened.
- `kalam-screens.md` generally: items tagged (R) "confirmed in the repo
  README" were confirmed against a different/older README and inferential
  page list. §6 of this audit supersedes its code claims; its design-coverage
  checklist (26 done / 7 partly / 78 to do at chat 7) remains the design
  record. The file itself is left unchanged — it belongs to the design
  handoff process.

## 11. What did not change

No application code was modified by this audit. Repository changes on this
branch so far are documentation only: `docs/design/` archived verbatim
(`6d780a6`), the `pdf-reader-spec.md` reference fix (§10/owner item (a),
`HANDOFF.md` checker now passes with zero errors), and this file. The next
step requiring a decision is starting §7 phase 1 (tokens) — no page code
should be touched before that compiles.
