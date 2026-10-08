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
