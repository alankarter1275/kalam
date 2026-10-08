# Plan — Phase 6: the EPUB editor

**Status: PLANNING — research complete, implementation plan below. Awaiting
the owner's go. Nothing is built until then (his 2026-09-29 rule).**

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
