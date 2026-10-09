# Plan — Phase 6: the EPUB editor

**Status: IMPLEMENTING — plan approved; steps 1 (patch store), 2 (matcher)
and 3 (reader seam) shipped and CI-green; step 4 (the [Fix Typo]
popover) next. Each step lands as its own CI-gated commit with the
owner's go.**

The settled design (2026-10-08, recorded in ROADMAP's Phase 6 section, the
record of authority): two surfaces kept; Markdown-style presentation over
XHTML storage (Obsidian-Live-Preview default surface, no Markdown
round-trips); raw HTML/CSS mode with a debounced side-by-side preview; every
edit a pending patch in the single `kalam.json` sidecar with a database
mirror; pending patches apply virtually at render; an on-demand review panel
with a badge; apply-all or apply-selected through the sanitizer's bake
safety; applied patches kept as collapsed history; structural edits as a
second pending-op kind with staged binary assets.

---

## Research findings (2026-10-08; files and lines cited)

The pitfalls were re-read at phase start (all 81 entries; the ones shaping
this work: §29 warnings, §31 side-effect re-homing, §32/§34/§35 gesture
coordination — the popover joins the chip, never a new gesture — §38
field-verify file writes outside our stack, §41 ratchets, §45 spans on every
apply path, §48 no container-mutation widget swaps, §52 exact anchors on
insertion edits, §54 dialogs are list surfaces and CSS specificity).

### How a chapter reaches the screen today

`.epub` → `chapbook_epub::Book` (wraps `rbook`; resources lazy in the zip;
`crates/chapbook-epub/src/book.rs`) → `unit_bytes(spine_index)` — the
`Publication` trait's raw-XHTML accessor (`crates/chapbook-core/src/book.rs:120`,
impl at `crates/chapbook-epub/src/book.rs:254`) → parsed on the loader
thread (`crates/chapbook-reader/src/loader.rs:143`; "may block for seconds"
is documented there) by `parse_xhtml` — XML first, lenient HTML fallback
(`crates/chapbook-layout/src/dom/parse.rs`) — into a **static arena DOM**
(`dom/tree.rs`; "documents are static after parse" per `dom/mod.rs`) →
layout → paint. The app opens the engine via `engine::open_engine`
(`src/pages/reader/mod.rs:679`).

**The consequences that shape the design:**

1. **`unit_bytes` is the one seam every chapter passes through.** A
   host-settable patch filter applied inside it means the entire engine —
   layout, painting, selection, highlights, locators — sees patched
   content with zero other engine changes, on the correct thread. The
   reader passes the book's pending patches at open; a patch saved
   mid-session reopens the engine (the `PdfDocRequest::Reopen` precedent).
2. **There is no serializer and no source spans.** Parsing is one-way:
   bytes → arena tree; the original formatting is not retained in the DOM.
   The original bytes are always available (zip via rbook / the `zip`
   crate). So patches must be defined **against the source bytes**, not
   the DOM — which is exactly the byte-stability property we promised:
   everything outside a patch's span is untouched by construction.
3. **Locators already solve "find this text again."** `LayeredLocator`
   (`crates/chapbook-core/src/locator.rs`: `Locator`, `Quote`, capture at
   :174, `resolve_in_text` at :247, `find_quote_nearest` at :266) layers
   exact offset + quote context + fraction with graceful degradation.
   Patches reuse the same philosophy: find-text **plus context anchors**
   (before/after), literal match in source space, ordered application, and
   a flagged-not-silently-skipped outcome on mismatch — the settled
   design's safety rule.
4. **Annotations and patches compose in one fixed order.** Bytes →
   patches → parse → locator text → anchors. Highlights made on patched
   text anchor against patched text; a highlight whose quote a patch
   changed degrades through the existing quote chain (never "gone"). The
   Phase 6 verification item is a fixture proving both.

### The stores, today

- `src/sidecar.rs`: `Sidecar` (SIDECAR_VERSION 1) written as pretty JSON
  (`write_sidecar`, :146), refreshed per book (`refresh_for_book`, :184),
  surveyed for recovery (:228). Patches extend `Sidecar` with a
  `#[serde(default)]` field — old files read clean — and the version bumps
  to 2.
- `src/db.rs`: `SCHEMA_VERSION = 17` (:72), the `schema_version` table and
  the migration list (:639+). The patches table is migration v18.
- `src/epub_sanitizer.rs`: the bake skeleton already exists —
  `sanitize_epub(path, backup)` (:52) rewrites the zip with `mimetype`
  first and stored uncompressed (:163–176), writes to a temp file,
  `verify_archive`s (:199, :814), then swaps, with a `.orig` backup.
  Baking is the same skeleton with "replace these entries with patched
  bytes" instead of "sanitize each entry".
- The selection chip is `build_selection_chip`
  (`src/pages/reader/engine.rs:499`) — `[Fix Typo]` joins it as a button;
  no new gestures (§32/§34/§35).
- Raw-mode widget: `sourceview5` (gtk-rs GtkSourceView 5 bindings, on
  crates.io, releases in lockstep with our `gtk4` 0.11). A new dependency:
  needs the README "what it enables" note, and the CI image needs the
  `gtksourceview5` system package. Exact version verified at that step.

### Patch kinds (three tiers, one review panel)

1. **Text patches** (typo popover, proofreading paragraph edits): find +
   replace strings with context anchors, in source space. Find-strings are
   entity-escaped DOM→source; at save time the matcher resolves the span
   against the real entry bytes and refuses (visibly) if it cannot match
   exactly once — so entity-encoded oddities are caught at save, not
   silently at render.
2. **File patches** (raw HTML/CSS mode): whole-entry replacement, guarded
   by a before-hash; still pending until apply.
3. **Structural ops** (split/merge/reorder/TOC, asset replacement): a
   second operation kind applied at bake; replaced assets stage as files
   in the book folder referenced by the patch record.

The one genuinely new piece of machinery is the **source-span mapper** for
paragraph-level edits: mapping an element in the parsed tree to its byte
range in the entry (a small span-tracking scan; reliable for the XML path —
the common case — with the HTML-fallback path handled conservatively). It
is isolated in its own step with fixture tests before any UI depends on it.

---

## Implementation plan (each step CI-sized; small first, big last)

1. **Patch store.** `SidecarPatch` in `src/sidecar.rs` (id, href, spine
   index, find, replace, context anchors, source mode, status, created);
   sidecar v2 with tolerant read; `patches` table, schema v18, with
   insert/list/mark-applied/delete queries; the sidecar-refresh invariant
   extended to patch writes. Tests: old-sidecar round-trip, migration,
   db round-trip.
2. **The matcher.** New `src/epub_patches.rs`: ordered application of text
   patches to entry bytes; context-anchored literal matching;
   per-patch outcome (Applied / NoMatch→flagged). Tests: entities,
   repeated-text disambiguation, mismatch flagging, and byte-stability
   (zero patches = identity; one patch = only its span changes).
3. **The reader seam.** `unit_bytes` filter hook in `chapbook-epub`
   (host-set, default none — a minimal, documented change to a kept
   crate); `open_engine` gains the patch set; loader thread applies
   patches before parse. Fixtures: patched render; annotation re-anchor
   survival (on patched text) and graceful degradation (quote changed).
4. **Inline selection editing** (redesigned 2026-10-08 at the owner's
   direction — no popover). A pencil button on the selection chip makes
   the selection itself editable: a text field overlaid exactly on the
   selection's rectangle, in the reader's own typeface and size,
   outlined like a focused field, pre-filled with the selected text, a
   live caret in it. The reader is a canvas renderer, so this is an
   overlay made to match the page — on commit the chapter re-renders
   through the step-3 seam, which is what makes the change look instant
   and in-place. Enter or clicking elsewhere commits; Escape cancels.
   Commit verifies the run against the entry bytes (`resolve_span`) and
   refuses — toast, the box stays open — an unchanged text, an
   unlocatable or ambiguous selection, or a selection crossing inline
   markup (that is paragraph editing, step 7's scope). Commit stores the
   patch (source: `typo`), reloads the chapter, confirms with a toast.
   **The no-glitch contract (the owner, 2026-10-08):** the edit box is
   anchored to the text, not the screen — repositioned on every repaint
   from the same geometry highlights paint with, in the same frame the
   page moves, so scrolling never leaves it behind or lagging. Scrolling
   or turning while editing never commits, cancels or loses the half-
   typed text; a paragraph scrolled off-view takes its box with it and
   gives it back on return. While the box is open the selection chip
   hides and the reader's idle cursor-hiding is suspended. Commit
   reloads without a blank flash (repaint-in-place, position held by the
   step-3 locator re-anchor) — on the test list. Fallback if fast
   scrolling still swims: hold the viewport still while editing.
5. **Review panel + bake.** Shared panel component (pending list with
   before → after, subset selection, collapsed history); bake worker on
   the sanitizer skeleton (mimetype-first, temp, verify — including a
   parse check of every patched entry — `.orig` backup, swap, hash
   update, reader reopen); badge + entry point on the book details page's
   action row (no context menus — the Remaster placement rule). §38: the
   done-when check opens the baked file in another reader.
6. **Source-span mapper + element serializer.** The paragraph machinery:
   element → byte-span mapping for the XML parse path, serializer for
   edited elements (deterministic output for spans we author). Fixtures:
   round-trip stability, spans across inline markup, the HTML-fallback
   path's conservative behavior.
7. **Proofreading edit mode — the same overlay, paragraph-scoped.** With
   proofreading on (a pencil toggle in the reader chrome), clicking a
   paragraph opens the step-4 inline editor over the whole paragraph;
   commit = a paragraph patch via step 6's span machinery, so rewording
   that crosses inline markup works. One editing feel for both scopes.
8. **Full editor, default surface.** New route with a way back (the
   anti-bloat rule); the editor opens its own `chapbook_epub::Book`;
   chapter list; rendered chapter; paragraph editing as in step 7; edits
   saved as patches (source: Editor); patches badge + panel in the editor
   chrome. Every apply path gets an activity guard + timing span (§45);
   no container-mutation swaps (§48). **Entry point from the reader
   (the owner's 2026-10-08 addition): a small pencil button at the top
   of the reader's left sidebar TOC, beside the cover, opening the
   editor for the current book** — it ships with this step, because
   before the editor exists it would be a button to nowhere.
9. **Raw HTML/CSS mode.** `sourceview5` dependency (README + CI image
   notes); whole-entry file patches with before-hash guards; debounced
   preview rendered on a service thread; the Preview toggle and the
   side-by-side layout.
10. **Structural ops + asset manager + visual TOC editor.** Pending-op
    kinds; staged asset files; bake composition in order. The done-when
    fixture set ends the phase: fix a typo mid-paragraph, restructure a
    bad TOC, produce a clean `.epub` that opens correctly in another
    reader (§38).

## Open questions for the owner

None blocking. The UX is settled; the plan above implements it. The go for
step 1 is the only thing awaited.

## Step log

- **Step 1 (patch store) — implemented 2026-10-08.** `patches` table
  (schema v18), `PatchRecord` + queries in the new `src/db/patches.rs`
  (every write refreshes the sidecar), and the `kalam.json` mirror (sidecar
  v2, `#[serde(default)]` so every v1 file reads clean — tested). Applied
  patches kept as history with `applied_at`. Implementation note: the plan
  said "sidecar v2"; the version bump is a signal of the shape change, not
  a break — the serde default is what makes v1 files readable, and that is
  what the tolerance test proves. **Pitfalls §72 recorded** from this
  step: parallel edits to one file raced (three vanished edits, three
  corrupted regions — two stray tails, one line join); caught by the
  full-diff read, repaired one edit at a time. The rule going forward: one
  edit per file per message. Awaiting CI; step 2 (the matcher) next.
- **Step 2 (the matcher) — implemented 2026-10-08.** `src/epub_patches.rs`:
  `resolve_span` (find + immediate context anchors; one hit applies, zero is
  `NotFound`, several are `Ambiguous` — flagged, never guessed) and
  `apply_text_patches` (creation order, each patch sees the previous one's
  result; filters to pending/text/this-href). Literal matching in source
  space — the byte-stability property the design promised, now proven by
  tests: zero patches is the identity, one patch touches only its span.
  The matcher has no production caller until step 3's seam — the same
  test-only shape step 1's queries shipped with.
- **Step 2 CI outcome — green on the third run.** The first run
  (37717908065) failed on the dead-code boundary: free `pub` functions
  with only test callers are flagged in the bin target where step 1's
  impl-methods were not — the module now carries `#![allow(dead_code)]`
  with the reason and the step that removes it (pitfalls §73). The
  second (37718889665) failed because the move-fix was applied
  backwards — the clone belongs in the earlier array; third run
  (37719896642, commit `d62d49c`) green: 558 kalam tests, all 8 matcher
  tests named in the full log, rustfmt clean.
- **Step 3 (the reader seam) — implemented 2026-10-08.** The
  virtual-edit chain, bottom up: `EntryFilter` in `chapbook-epub` (a
  newtype, default none, applied inside `unit_bytes` so every consumer
  sees the filtered view), `Session::set_entry_filter` (right after
  open, before the first read — the same rule as settings),
  `ReaderOptions.entry_filter`, and `open_engine` taking the book's
  patch list (empty list installs nothing — unedited books keep
  full-exactness locator resolution). The exact-offset consequence is
  handled where it lives: `href_matches` and `goto_layered` treat a
  filtered session as never-same-source, so stored locators re-anchor
  through their quote context and degrade to the fraction layer when an
  edit consumed the quote. Step 2's dead-code allow came off. Fixtures
  in `crates/chapbook-reader/tests/entry_filter.rs` (patched text layer,
  patched paint, re-anchor +1, destroyed-quote degradation) and the
  hook-contract tests in `chapbook-epub`. **Known gap for a later
  step:** the app's content index (search, vocabulary:
  `src/content_index.rs`) opens its own `Book` and still indexes
  unpatched text — wire the filter there when the review panel lands
  (step 5) so search matches what the reader shows.
- **Step 3 CI outcome — green on the fifth run, four fix-forwards.** Each
  run failed on one thing and moved one crate further down the chain:
  (1) 37723814937 — `clippy::type_complexity` on the filter newtype's raw
  closure field → private `EntryTransform` alias (§74); (2) 37724717910 —
  the type was `pub` in a private module but missing from the crate
  root's selective re-export list → `pub use book::EntryFilter;` (§75);
  (3) 37725602605 — `OpenBook`'s other variants are feature-gated, so a
  single-variant build made the seam's if-let irrefutable and its
  wildcard unreachable → cfg-gated fallback arms, the enum's own
  pattern (§76); (4) 37726510604 — the flag-logging compared a bare
  `Resolution::Found` (a payload-carrying variant, so a constructor,
  not a value) → `!matches!(..., Found(_))` (§77). Run 37727355176
  (commit `dcb1009`) green: all four session fixtures
  (`patched_text_reaches_the_session_layer`,
  `the_patched_view_paints`, `a_highlight_re_anchors_over_edits_before_it`,
  `a_highlight_whose_words_were_edited_degrades_gracefully`) and both
  hook-contract tests named and passing in the full log.
- **Design change for step 4 (the owner, 2026-10-08): no popover —
  inline selection editing.** The owner's view: select something, click
  the pencil on the actions box (the selection chip), and the selection
  itself becomes editable — an outline around it, a cursor in it, the
  paragraph seemingly edited in realtime on the page. The step-4
  implementation is that feel: a text field overlaid on the selection's
  rectangle in the reader's own typeface (the canvas renderer makes a
  matched overlay the honest way to get a real caret), Enter/click-away
  commits, Escape cancels, commit-time verification and refusals
  unchanged. Step 7 reuses the same overlay paragraph-scoped. Also the
  owner's addition for step 8: a small pencil button at the top of the
  reader's left sidebar TOC, beside the cover, opening the full editor —
  ships with the editor, not before.
- **Step 4 (inline selection editing) — implemented 2026-10-08.** The
  pencil joins the chip's pill (`document-edit-symbolic`, "Fix typo")
  and `BeginInlineEdit` opens the editor: a `gtk::Entry` laid over the
  selection's rect as a child of the reader's overlay — start-aligned
  margins in view-widget coordinates (the strip scrollbar's coordinate
  system), size-request floored at 160×28, pre-filled with the
  selection, and set in the reader's own family and size through a
  per-edit CssProvider registered on the display (house style — no
  widget-local StyleContext) and unregistered when the edit closes.
  Enter commits, Escape cancels, focus leaving commits; a done-flag
  shared by the three makes them one-shot, and the model re-guards on
  every message, so doubles are benign. The page itself is not
  focusable, so click-away is caught where it shows up instead: any
  `EngineSelection` while an edit stands (a fresh drag or a
  selection-clearing tap) commits it.

  The no-glitch contract is met by the follow machinery, not by luck:
  `kalam-reader` gained `connect_selection_moved`. `place_handles` —
  which both paged and scrolled draws already run every frame to paint
  the handles — now also records the selection's union rect, and
  `after_draw`'s idle reports it to the shell only on change. The box
  repositions from that report, which lands after the frame that moved
  the text is already painted: the box converges with the text, never
  chases it. Scrolls move geometry, not the selection, so half-typed
  text is never lost; when the text scrolls off screen the box holds
  its position and reunites with the text on the way back. The chip
  and idle cursor-hiding stand down while the editor is open; the
  cursor returns when it closes.

  Commit-time verification is off the UI thread: the book is opened
  (`chapbook_epub::Book::open`), the chapter's bytes read, the book's
  earlier pending patches applied, and the new pure planner
  `plan_text_patch` asked to verify the correction against exactly the
  bytes the reader shows. `escape_to_source` re-escapes the DOM text
  to source form first (`&`, `<`, `>`); the span resolves anchor-free
  (zero matches → NotFound, more than one → Ambiguous); 32-byte
  context anchors are captured around the verified span, rounded to
  char boundaries. The verdict crosses back as plain data over an
  `async_channel` — the tasks manager's worker→main-loop shape: no GTK
  types cross the thread, a local future on the main loop turns it into
  `InlineEditVerified`. Unchanged is answered inline
  without opening the book; every refusal toasts and leaves the box
  open with its text. A verified patch stores with source `typo`, and
  the book reopens through the step-3 seam: the reader's overlay is
  persistent (created once in `init`), the view swaps inside it with
  `set_child` (§48-clean), the new view is wired and re-given every
  mode pref, and the chapter/fraction are restored from what the
  position callback had been persisting all along. The reopen runs on
  the main thread under a `reader_reload` timing span — same bounded
  cost as the initial open; CI and the step log will record it
  honestly. Seven new unit tests cover the planner: escape
  round-trip, verified-with-context, unchanged, markup-crossing,
  ambiguous, prior-patch composition, and char-boundary anchors.
- **Step 4 CI outcome — green on the fourth run, three fix-forwards.**
  (1) 37733497947 — the verdict bridge was written from remembered
  gtk-rs docs: glib 0.22 has neither `MainContext::channel` nor a
  root `Sender`, and `connect_leave` passes the controller → the
  house worker→main-loop shape instead (`async_channel` +
  `send_blocking` + `spawn_future_local`, straight out of
  `tasks.rs`) and the one-argument closure (§78); (2) 37734781134 —
  three `move` closures shared one `Rc<Cell<bool>>` binding → a
  clone per closure, the drawer's own shape (§79); (3) 37735334310 —
  clippy fully green and 564 tests passing; the one failure was the
  planner's own context-anchor test, its expected value computed in
  DOM space while the planner matches in source space (the span
  covers the entity's semicolon) → the expectation now derives from
  the source string (§80). Run 37736281896 (commit `466e2e3`)
  green: all seven new planner tests named and passing in the full
  log (`dom_text_is_escaped_back_to_source_form`,
  `a_correction_verifies_with_context_captured_around_it`,
  `an_unchanged_correction_is_refused`,
  `a_selection_crossing_markup_is_not_found`,
  `an_ambiguous_run_is_refused_not_guessed`,
  `verification_sees_earlier_pending_fixes`,
  `context_windows_respect_char_boundaries`), the bin at 565 passed
  / 0 failed, and the smoke-test job green with its report
  published. The reader reload cost is instrumented as
  `reader_reload` in the timing spans — the step log for it belongs
  to the first field run.
- **Step 5 (review panel + bake) — implemented 2026-10-08.** The
  book details page's action row gains the edits entry point: a pencil
  button, EPUB-only, badged with the pending count from the page
  snapshot's new `pending_edits` field. It opens the review panel as
  an in-app float — a shared panel component the full editor (step 8)
  will host in its own chrome: pending edits as before → after rows
  (struck dim over bright, the highlights panel's hierarchy) with live
  checkboxes, per-edit discard, source · chapter · date meta, an Apply
  button that counts its selection, and the applied history collapsed
  under a disclosure toggle.

  Apply runs the bake on a worker thread: `bake_epub` reuses the
  sanitizer's repack skeleton — mimetype first and stored, other
  entries raw-copied, patched entries deflated, temp file,
  `verify_archive`, swap — with the first bake's `.orig` kept as the
  pristine undo and a stricter parse gate than the reader's:
  roxmltree (no HTML fallback) must still parse every entry that
  parsed before, so a splice that would break a chapter refuses the
  whole bake and nothing is written. `BakeReport` says what applied
  and what could not match (nothing is silently skipped; a no-op bake
  rewrites nothing). The worker then re-hashes the book row with
  `rehash_book` (the remaster path, which also moves metadata
  overrides), marks the applied patches, and reindexes search — all
  on the same thread, because reindexing is not a main-loop job.

  Search now matches the reader: the content index extracts through
  the step-3 entry-filter seam with the book's pending patches (no
  filter when there are none — the established verbatim rule). And
  every render-side patch read became pending-only — the reader's
  open, the inline-edit verification, the reload, and the index all
  moved to `get_pending_patches_for_book`; the sidecar alone keeps
  the full history, because a backup documents everything, not just
  what is still to do. The bake's reader-reopen clause is dormant on
  this surface (the float opens over the book page, where the reader
  is unmounted; the next open reads the baked file from disk) and
  activates with step 8's editor.

  **CI — green on run 4 (37747385207), after three fix rounds**, each
  caught by a different layer of the project's own defenses, in the
  order rustc stages them:

  1. *Run 1 (E0599):* `set_margin_all` is relm4's `RelmWidgetExt`,
     not gtk's — the working precedent imported the trait, my file
     didn't. §81.
  2. *Run 2 (E0382 + E0505):* the by-value `for p in pending` loop
     consumed the Vec the refresh tail still needed, and the Apply
     handler captured the very `apply` binding it was connected to
     (receiver borrowed, closure moving the same value — the closure
     now holds a differently-named clone). Both were invisible in
     run 1: type errors mask the borrow-check pass. §82.
  3. *Run 3 (guardrails):* `pages_touch_disk_or_documents_only_
     inside_tasks` flagged one `path.is_file()` on the UI thread —
     redundant, since `bake_epub` re-checks on the worker. Removed.
     §83.

- **Step 6 (source-span mapper + serializer) — implemented
  2026-10-08.** New `src/epub_spans.rs`, deliberately unwired (step 7
  is its first caller; module carries the dead-code allow with that
  reason, the step-2 precedent). The parser stack builds its DOM
  without source positions and markup5ever's `TreeSink` has nowhere
  to put them, so the mapper is a standalone span-tracking scan: a
  small strict-XML tokenizer over the raw bytes that builds an
  element tree with byte ranges attached. Strict where the HTML
  fallback is loose — implied closes, mis-nested tags, stray `<`,
  unquoted attributes, missing single root all make the entry
  `NotMappable` — with one documented tolerance: the HTML void set
  unslashed (`<br>`), because html5ever voids exactly those too, so
  the trees agree.

  Paragraph identity is the reader's text: the scan's extraction
  mirrors `chapbook-layout`'s `extract_text_at` (entities decoded —
  the five XML names plus numeric refs, unknown ones left verbatim so
  an HTML-parsed entry mismatches and refuses rather than maps wrong;
  CSS whitespace collapse; `<br>` as hard newline; block boundaries),
  and `locate_paragraph(bytes, text, ordinal)` matches it against the
  scanned paragraphs — the deepest text-bearing non-inline elements,
  head/script/style/template skipped, disambiguated by an ordinal
  among the equal-text ones. Refusals are explicit: `NotFound`,
  `NotCurrent(n)`, `NotMappable` — never a guessed span.

  The serializer writes spans this module authored: verbatim opening
  tag (class, id, `epub:type` are content), the new text escaped to
  source with `\n` as `<br/>` and whitespace collapsed (the reader's
  normalization inverted), verbatim closing tag. Empty text writes
  the empty element. `plan_paragraph_patch` is the paragraph sibling
  of `plan_text_patch`: unchanged-check in identity space, locate,
  find = the paragraph's whole outer source, replace = serialized,
  context anchors captured around the outer span.

  One change to the existing pipeline: `apply_text_patches`' kind
  filter widened from `"text"` to `"text" | "paragraph"` — a
  paragraph patch carries literal source-space find/replace like any
  text patch, so render filter, bake and search index apply it with
  no further changes. 13 fixture tests: exact spans, reader text
  semantics (wrapping, entities, CDATA, skipped head), ordinal
  disambiguation, unchanged refusal, flattening round-trip through
  the matcher (byte-identical outside the span), `<br>` round-trip,
  conservative refusals (unclosed `li`, mis-nested close, stray `<`,
  two roots, non-UTF-8, `&nbsp;` text mismatch), void tolerance,
  paragraph-emptying as a real edit.

  **CI — green on run 2 (37753823314, 16m).** Run 1 was three
  clippy complaints about one helper's signature (`&mut Vec` where
  only indexing happens, `&mut` flowing into an `&[usize]` — §84);
  the slice-and-immutable fix went through with all 13 new tests
  passing on their first execution. A stray 2-second cancelled
  duplicate run appeared beside the green one — the ci-logs
  publishing pushes racing, not a code signal.

- **Step 7 (proofreading edit mode) — implemented 2026-10-08.** The
  reader names a paragraph; the app opens the step-4 editor over it;
  commit stores a paragraph patch. A pencil `ToggleButton`
  (`document-edit-symbolic`) sits in the reader's back dock after
  Search, visible on EPUBs only (`model.book_format`, set at init
  from the catalog row); nothing persists it — the flag is view
  state, re-applied on the reader's view swaps so a reopen mid-book
  keeps the mode. With it on, a tap that lands on a paragraph
  selects the paragraph (the selection's moved-rect stream anchors
  the editor from the next frame, as for the selection editor) and
  fires a new `paragraph_tap` callback carrying the paragraph's
  identity and the union rect of its visible lines. A tap that lands
  elsewhere behaves exactly as before; a tap that lands on a
  paragraph while a selection stands replaces it — one tap opens the
  editor, the old selection's report having committed any open one
  as its click-away. The identity comes from the reader crate:
  `paragraph_tag_at_page` (the tap's hit, now carrying the block's
  layout tag) and `Session::paragraph_identity`, which walks the
  chapter's tagged blocks in document order and returns the tapped
  one's reader-extracted text with its neighbours' — `None`
  neighbours asserting first/last.

  **One design change to step 6, settled with the owner before the
  code:** paragraphs are disambiguated by their neighbours, not by
  an ordinal into the chapter's paragraph list. The ordinal coupled
  the reader's enumeration to the mapper's — an anonymous text block
  shifts every ordinal after it, and a mismatch edits the wrong
  paragraph or refuses the right one — while neighbours disagree
  only locally, around the paragraph itself, and a disagreement is a
  refusal. `locate_paragraph` takes `(want, prev, next)`, matching
  `None` as a hard first/last assertion; the enums renamed to match
  (`NotCurrent(n)` → `Ambiguous(n)` in both `ParagraphLocate` and
  `ParagraphRefusal`), and the reader side was written to the same
  contract: its enumeration is the layout's own (the blocks that
  got line fragments, anonymous text runs borrowing their parent's
  tag), its text is `extract_text_at` with the ends trimmed — the
  same normalization `epub_spans`'s scan applies, whitespace runs
  collapsed, `<br>` a hard newline, U+00A0 content. Where the two
  enumerations can disagree (a hidden footnote the layout never
  tags, a mixed div>text+p container), the locate refuses —
  paragraph editing declines, the selection editor still works.

  The editor is the step-4 box grown to a paragraph:
  `build_paragraph_editor` (a `gtk::TextView`, `WordChar` wrap, tabs
  to the reader, Enter/KP_Enter commit, Escape cancel, focus-out
  commit, the reader's own family and size, the same overlay
  positioning and the same `textview.k-inline-edit` styling beside
  the entry's — its internal `text` child transparent so the
  bordered box is the only background). `EditScope` carries which
  kind of edit is open; `EditorWidget` carries which widget;
  `CommitInlineEdit` branches to `verify_paragraph_edit`, the
  step-4 worker with `plan_paragraph_patch` in place of the text
  matcher, over the same virtual chapter (prior patches applied).
  Refusal toasts name the way out; `Ambiguous(n)` points at the
  full editor (step 8).

  **Two races in the step-4 commit path closed.** Enter, click-away
  and focus-out can all arrive for one edit, and the engine's
  `done` flag only knows about its own two controllers — a
  selection-path commit followed by a later focus-out verified the
  same editor twice. The model now carries `InlineEdit::committed`,
  set when a commit is dispatched, and every commit path passes it;
  a refusal carries the editor's serial back (`Err(toast, serial)`)
  and re-arms exactly the editor it refused — the retry loop step 4
  promised, now actually safe. `next_edit_serial` hands out the
  serials; verdicts store their patch regardless (the editor they
  verified may already be gone), and close-and-reload only when the
  serial is the editor still standing.

  Tests: the epub_spans suite rewritten to the neighbour contract
  (the chapter fixture's refrain pair now asserting
  `Ambiguous(2)`, first/last asserting `None`), and a new
  chapbook-reader integration test (`paragraph_identity.rs`) pinning
  the reader half of the contract on `fixtures/epub/minimal.epub`:
  markup-carrying paragraph, document-order neighbours, firstness,
  and the refusal for a tag the chapter does not carry.

  **CI — green on run 4 (37766393364, 11m).** Three runs, three
  layers, one lesson apiece, none a design fault. Run 1
  (37762409883): `gtk::IsA` is not a path — gtk-rs traits come from
  the prelude (§86). Run 2 (37763660225): the layer run 1's
  resolution error had masked — relm4's prelude is not gtk's, so
  types.rs's first widget method calls needed `use gtk::prelude::*`,
  and `VerifiedEdit` in a message payload needs the enum's derives
  (§87). Run 3 (37765207494): the private-interfaces layer — a
  `pub(crate)` type cannot sit in a `pub` enum's variant, even in a
  private module (§88). Run 4 compiled, clippy'd, formatted, passed
  every test — the 14 rewritten epub_spans tests and the 4
  paragraph_identity integration tests on their first execution —
  and the headless-sway smoke test booted the app. The heredoc
  string-continuation near-miss (§85) was caught in review, before
  any run wasted on it.

- **Step 8 (full editor) — implemented 2026-10-09.** A route of its
  own (`Route::Editor { book_id }`), a way back (its own bar, Esc,
  Back), and the reader's machinery doing the editing — the editor
  page is a chrome around its own engine: `open_engine` on the
  book's file with the reader's prefs and the book's pending
  patches, a flat chapter list (TOC titles, `toc_title` promoted to
  `pub(crate)`) on the left, the rendered chapter in the middle, and
  the step-5 review panel in a right-edge revealer behind a
  `Patches · N` badge. Proofreading is not a mode here —
  `set_proofreading(true)` at open and at every reload; the
  pointer never auto-hides over a work surface.

  The owner's entry point ships with it: a pencil beside the cover
  at the top of the reader's left sidebar (`document-edit-symbolic`,
  EPUB-gated by the step-7 `book_format` watch), emitting
  `ReaderOut::OpenEditor` as a route push, so Back returns to the
  reader exactly where it was.

  Reuse is by sharing, not by copying: `build_paragraph_editor` was
  generalized over its commit/cancel callbacks (the reader passes
  closures over its `ReaderMsg`, the editor over its own
  `EpubEditorMsg`; the commit callback is shared by the key and
  focus controllers, so it travels as a clone), and the step-7
  types — `InlineEdit`, `EditScope`, `EditorWidget`,
  `VerifiedEdit` — went `pub` and are re-exported from the reader
  module (§88's shape: pub in a private module is crate-local).
  `verify_paragraph_edit` was already model-agnostic. The editor's
  commits store with the source `Editor`; its commit pipeline is
  the reader's, including the `committed` gate and the serial-carrying
  refusals.

  The editor's own seams: its `wire` installs only the four
  callbacks it uses (selection, selection-moved, paragraph-tap,
  position — the chapter list follows the position, nothing
  persists); its reload is the reader's shape (open before
  touching the old view, swap inside the overlay, restore chapter,
  `editor_reload` span + activity — §45, §48); the panel is
  rebuilt after every commit and, because the panel can bake,
  accept or reject on its own, the pending count is snapshotted
  when it opens and compared when it closes — a change reloads the
  engine. Sharp edge, accepted for this step: while the panel
  stands open, a bake or reject inside it shows in the view only
  when it closes.

  Route plumbing: `cache_key` refuses to cache editor pages (the
  same None as readers), `route_label` gets `route_open:editor`,
  `is_reader` includes it (full-bleed content classes, no shell
  back chip), and both force-rebuild lists (leaving an editor
  rebuilds the landing page, like leaving a reader).

  **CI — green on run 2 (37830650342, 10m).** Run 1 (37829171113)
  failed three ways (§89): the shared commit callback cloned after
  the first `move` closure took it (the §82 family), and the two
  exhaustive matches the new variants broke that this plan never
  named — the bubble windows' own reader forwarding in `bubbles.rs`
  and `Route::sidebar_item` in `models.rs`. The sweep lesson — grep
  the enum's last variant to find every exhaustive match — is now
  in §89. Run 2 compiled, clippy'd, formatted, passed every test
  and the headless-sway smoke boot.

  Workspace note, recorded here because it happened mid-step: the
  sandbox was recycled between steps and re-cloned at the session's
  base commit while the working tree kept the full phase-1–7 state
  plus this step's edits; the branch was re-anchored to the remote
  tip (`git reset origin/arena/…`) with the tree untouched, and
  `git status` confirmed the diff was exactly this step's set
  before the commit.

- **Step 9 (raw source mode) — implemented 2026-10-09.** A Source
  toggle in the editor's bar swaps the stage's proofreading surface
  for a raw pane on the current chapter's entry: GtkSourceView
  (`sourceview5` 0.11, lockstep with gtk4 0.11; `libgtksourceview-5-dev`
  added to both CI apt installs, `gtksourceview5` to the README's Arch
  line) with line numbers, XML highlighting by default (CSS by
  extension — the machinery is href-generic, CSS files could ride
  later; v1 scope is spine XHTML chapters).

  A source edit is a whole-entry patch, kind `file`: find empty,
  replace = the buffer, source `raw`, and a new `before_hash` column
  (`add_column_if_missing`, the books-v4 pattern) carrying the
  entry's SHA-256 at load time — the guard. `apply_text_patches`
  takes kind `file`: hash matches → whole-entry replace; mismatch →
  the new `Resolution::Stale` (bake reports it like its other
  refusals). The guard is computed over the bytes *as the patch loop
  has them*, so a pending reader fix made before the raw save still
  chains — proven by test. Empty guard is always Stale: a file patch
  without a hash is never a blank cheque. `insert_patch` delegates to
  a new `insert_guarded_patch` (same row, plus the guard); the
  review panel's rows summarize a file patch ("Whole file → edited
  source (N chars)") instead of printing a chapter.

  The pane's lifecycle: built once in init (hidden until toggled —
  visibility swap, never a child swap, so the engine keeps its
  scroll and the buffer keeps its undo); loads through a worker
  (Book open → unit_bytes → pending patches applied → the virtual
  bytes are both the buffer's content and the hash baseline); saves
  through a guard-check worker (re-hash now vs. the baseline — Fresh
  stores the patch, Stale raises the pane's banner and disables Save
  until Revert), then refreshes counts, panel and engine. Escapes
  ladder: dirty buffer reverts, clean pane closes, then the page.
  Dirty buffers hold Back, chapter clicks and the toggle with a
  toast. A bake or an inline fix under a clean pane reloads it
  automatically; a dirty one is protected by its save-time guard.

  The Preview toggle (visible while Source is open) renders the
  buffer as it will read: 700 ms debounce, generation counter, a
  service thread re-opens a headless `Session` per tick (the engine
  has no cache-invalidation API, so nothing is shared) with an
  `EntryFilter` substituting the buffer for its entry and the pending
  patches everywhere else, stacks ~3 pages at the reader's own
  column width into one tall pixmap, and the main loop turns the
  RGBA bytes into a `gdk::MemoryTexture` on a `gtk::Picture` beside
  the editor. The headless render itself is kalam-reader's
  `preview::render` — the widget crate owns Session usage; the app
  only ever builds the filter.

  **CI — green on run 4 (37840405710, 12m).** Three failures on the
  way, one per run, each a different class. Run 1 (2m, clippy): the
  §89 sweep for `Resolution::Stale` had excluded the enum's own
  module — `plan_text_patch` matches `resolve_span`'s result
  exhaustively there — §90. Run 2 (clippy): three lints — a
  collapsible guard pair, `SourcePane.view` dead after its last
  reader moved to the engine view, and the preview thread body
  missing its wrapper's `too_many_arguments` allow. Run 3 (tests):
  two epub_patches tests — my predecessor test asserted a 10-byte
  span for an 11-byte replacement, and the kinds test's "other
  kinds" probe *was* the new file kind, which now runs by design
  (the probe is an unknown `note` kind again, the test renamed).
  Run 4: build 8m42s (fmt clean, clippy clean, 590 tests), smoke
  2m39s. CI's lock step pushed the refreshed Cargo.lock
  (sourceview5 0.11.2) with run 1's publish.

- **Step 10 (structural ops + asset manager + visual TOC editor) —
  implemented 2026-10-09.** Two new pending-op kinds beside the text
  kinds, both file operations rather than text matches, both guarded
  by `before_hash` exactly like the raw mode's whole-file edits, both
  one-per-file by construction (the composing surface replaces its own
  pending op rather than stacking one).

  **Spine ops** (`kind "spine"`, source `toc`): the visual TOC editor
  is the editor's chapter list with reorder arrows. Every move swaps
  two rows and stores the whole new order as the book's one pending
  spine op — `href` the package's zip path, `replace_text` the order
  as old chapter positions (`"2,0,1"`), `before_hash` the package's
  current SHA-256. The identity order stores nothing: deleting the op
  *is* the reset. The list shows the *pending* order; the engine is
  never reordered virtually — `model.chapter` stays a real spine index
  everywhere, and only the list (and the arrows' message) translate
  between position and chapter.

  The bake arm (epub_sanitizer) resolves the package entry through
  the same raw→percent-decoded lookup as chapters, refuses on a hash
  mismatch (stale, like every guard), then rewrites the spine with
  `rewrite_spine_order`: a quick-xml scan over the package bytes that
  captures each `<itemref>`'s raw byte slice by event positions
  (buffer_position before/after each event — the v0.37/0.42 API is the
  same here; the root crate pins 0.37, rbook and chapbook-epub ride
  0.42) and rebuilds the file as everything before the spine's
  content, the itemref slices reordered (their attributes never
  re-serialized — `linear=`, `idref=` move verbatim), everything from
  `</spine>` on. The separator is the original's own inter-itemref
  whitespace when that is all it was. Permutation is validated against
  the itemref count; a second spine, a missing spine, a non-permutation
  are op failures, never bakes of garbage. The XML parse gate the text
  patches pass applies: a package that parsed must still parse.

  **Asset ops** (`kind "asset"`, source `asset`): a Cover button in
  the editor's bar (insensitive until the book names a manifest cover)
  opens a `gtk::FileDialog` (the import picker's pattern, image
  filters), stages the chosen file off the UI thread as
  `.kalam-staged/<sha8>-cover.<ext>` beside the book, and stores the
  op — `href` the cover entry's zip path, `replace_text` the staged
  *relative* name, `before_hash` the cover entry's current hash. The
  bake arm swaps the entry's bytes for the staged file's, after the
  same hash guard and a path-safety check (a database row never gets
  a file handle on trust: no `..`, no absolute paths, no backslashes).

  **The structure snapshot**: one worker at editor open (and after
  every bake — the package's hash moves with it) answers both
  questions the structural controls need: which entry is the package,
  which is the cover, what each hashes to now. The arrows and the
  Cover button stay insensitive until it lands; a book that cannot
  answer keeps them that way rather than erroring at open.

  **After a bake that applied a spine op**, the book's spine-keyed
  rows travel with it: `Catalog::remap_spine_indices` translates
  reading positions, annotations, saved words and bookmarks through
  the new order in one `CASE` statement per table (a pair-by-pair
  `UPDATE` run would undo itself on a swap), and rewrites the
  annotations' stored locators' `spine_index` field by field —
  `spine_href` is the primary anchor there, so this is belt and
  braces, but a true href beside a stale index is a bug factory. The
  pending `patches` rows are history, not state: href-keyed, never
  remapped. An asset op that landed re-dresses the library too: the
  staged image goes through the existing `replace_cover_bytes`
  service (fresh cover file in the book's folder, old file and its
  thumbnail cleaned, catalog pointed, new thumbnail made — the fresh
  name each time is what keeps GTK's path-keyed texture cache
  honest), then the staging itself is deleted. Discarded ops delete
  their staging off-thread on the way out.

  **The editor's reload now restores position by href**, not chapter
  index: a spine bake moves indices underneath, and the old "min"
  clamp would land somewhere else. The locator (`spine_href`, quote
  re-anchor) survives the move; the index is only the no-locator
  fallback.

  Done-when fixture set: the phase's own composition test bakes a
  typo fix, a spine reorder and a cover replacement in one bake into
  one clean `.epub` (mimetype first, all three entries patched,
  chapters intact) — `the_done_when_composition_bakes_all_three`.
  The §38 field half (opens correctly in another reader) is the
  owner's check.

  **CI — green on run 6 (37852991711, 9m: build 6m9s, smoke 2m35s).**
  Five failures on the way, each a different class, three of them
  lessons now in the pitfalls ledger. Run 1: four compile errors —
  the sanitizer's test module imports none of the db types (its main
  code is fully qualified), a filter closure over `(usize, &usize)`
  whose binding modes shift with the pattern, and an `Fn` click
  handler moving its captured `Arc` into a spawned thread (§93).
  Run 2: the same filter, flipped — adding `&old` had turned `new`
  into a reference; `new != *old` is the shape that holds either way.
  Run 3–4: clippy's double-ended-iterator lints, a family — `.last()`
  wants `.next_back()`, and `filter(..).next_back()` is its own lint;
  `iter().rev().find(..)` ends the family (§94). Run 5: the
  pages-touch-disk guardrail caught all eight of the new disk calls
  in `src/pages` — staging, jacket update, staged deletions, every
  one on a worker thread but none of them the guardrail's to excuse:
  the follow-ups are the cover service's work, and they moved to
  `src/epub.rs` beside `replace_cover_bytes` (§95). Run 6 green with
  all 602 tests (12 new: 6 sanitizer structural, 1 remap, 1
  pass-through, 4 rewrite units — plus the phase's composition
  fixture among them).

- **Follow-up (the editor's own rail entry) — implemented 2026-10-09.**
  The owner's request, verbatim intent: an Editor button in the main
  sidebar — where Home and My Library sit — that opens the epub
  editor, with the book chosen *from* there, like Calibre's e-book
  editor. The step-8 entry points (the reader's TOC pencil) all know
  the book already; the rail entry knows none, so it gets the
  editor's front door: a new page (`src/pages/editor_picker.rs`)
  listing every EPUB in the library — search by title or author,
  one click on a cover and the full editor opens on that book
  (`Route::Editor`, pushed, so Back returns to the picker).
  Non-EPUBs are not listed: the editor's pipeline is EPUB-shaped, so
  a card it would refuse to open is a broken promise. Wiring:
  `NavItem::Editor` (icon `text-editor-symbolic` — not the reader
  pencil's `document-edit-symbolic`, which is a per-book
  afterthought's icon; `is_bottom` stays Settings-only, so Editor is
  the last of the top group), `route_by_name("editor")` for the CI
  harness, `route_label` arm, and `sidebar_item` now maps
  `Route::Editor` to `NavItem::Editor` so the rail lights the
  Editor button whichever door the editor was reached through.
  One shared widget changed honestly: the book card's tooltip hard
  promised "Click: float · Ctrl+click: full page", which is a lie on
  a picker where every gesture opens the editor — the hint is now a
  parameter (`CLICK_HINT_LIBRARY` for the library pages,
  `build_book_grid_open` for the picker), and a third CI screenshot
  pass (`ROUTE=editor`, report in `ci-logs/editor-latest.txt`)
  photographs the new page over the 139 seeded real EPUBs.

  **CI — green on run 2 (37901417687, 15m47s).** Run 1 failed on
  one line: the new `PageSlot::EditorPicker` variant's arm in
  `impl PageSlot::widget()`, an exhaustive match sitting quietly
  below the enum (E0004; pitfalls §96). Run 2 green with all 602
  tests (the route-name test grew an "editor" assertion), and the new
  screenshot pass proves the page on screen: route accepted, the
  shot differs from Home, `grid_cards 139`, covers decoded with no
  main-thread stall, and — because CI runs with
  `KALAM_NO_WINDOWED_GRID=1` — the plain grid path rendered while
  the windowed path stays the default in real use, both paths
  verified in one run.
