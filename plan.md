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
4. **`[Fix Typo]` popover.** Chip button → popover (selected text,
   correction, Save). Save resolves the span against the entry bytes,
   stores the patch, reloads the chapter, confirms with a toast.
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
7. **Proofreading edit mode.** Pencil toggle in the reader chrome; click a
   paragraph → in-place editing bound to that paragraph; commit = a
   paragraph patch via step 6's machinery.
8. **Full editor, default surface.** New route with a way back (the
   anti-bloat rule); the editor opens its own `chapbook_epub::Book`;
   chapter list; rendered chapter; paragraph editing as in step 7; edits
   saved as patches (source: Editor); patches badge + panel in the editor
   chrome. Every apply path gets an activity guard + timing span (§45);
   no container-mutation swaps (§48).
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
