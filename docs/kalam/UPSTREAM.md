# Chapbook: where the engine came from

**We no longer track upstream.** Decided 2026-09-19.

The reading engine in `crates/chapbook-*` was not written for Kalam. It was
imported wholesale from [ophymx/chapbook](https://github.com/ophymx/chapbook),
and this file used to describe a monthly routine for pulling the author's bug
fixes back in. That routine is retired.

## Why

Kalam has modified **74 inherited files** — the table below. The engine's own
library and database layer was removed because Kalam has its own, one file was
deleted outright, and a theme system was added that upstream does not have.
The previous version of this document already singled out
`crates/chapbook-reader/src/open.rs` as *"the file most likely to conflict on
a cherry-pick."*

Editing files in place while also pulling the author's changes to the same
files means resolving conflicts by hand, every month, indefinitely — and the
routine ran exactly once. On 2026-09-10 it reviewed `ab14cb7..7ace24a` (PRs
#32–#36, 42 files) and took **nothing**: upstream's work was phone and Windows
shells, FFI/JNI navigation, OPDS catalogue bindings and Swift/.NET surfaces,
none of which Kalam has. The upkeep was real and the yield was zero.

The old document's closing section already contained the answer: *"Nothing
breaks. The last version we took keeps working forever; we simply stop doing
the review. That is the point of owning the copy."* This is that.

## Provenance

| | |
|---|---|
| Upstream | https://github.com/ophymx/chapbook |
| Imported at | commit `ab14cb78d2f7e63e8e2f7bc066bfc8ee57318cc9`, 2026-09-07 |
| How | `git merge --allow-unrelated-histories` — the full upstream history is in this repository, so `git log` on an engine file still shows where it came from |
| Reviews done | One: 2026-09-10, range `ab14cb7..7ace24a`, nothing taken |
| Tracking ended | 2026-09-19 |

Nothing is lost by this. The history is still here, so if a specific upstream
fix is ever wanted it can be found and brought over by hand — there is simply
no standing obligation to go looking.

Five of Kalam's own changes below are marked *"Candidate to send upstream."*
They are real bugs in the original project; one is a reading setting the
author persisted but never read. The owner's decision is to leave them
recorded here rather than open issues or pull requests.

## What Kalam changed inside inherited files

Kept because it records how this engine differs from the one that was
imported, which is worth knowing when debugging it. It no longer predicts
merge conflicts, because there is nothing left to merge with. The `kalam:`
commit prefix is still worth using, since it makes these changes findable.

| File | What | Why |
|---|---|---|
| `.github/workflows/ci.yml` | Replaced Chapbook's 9-job matrix with a single Linux job; a `lockfile` step (`cargo metadata --locked`) fails the run when `Cargo.lock` is not what cargo would write | We ship to one platform; the lock file is edited by hand from a sandbox without cargo, and drift there blocks the owner's `git pull` |
| `README.md` | Replaced | Describes this fork, not Chapbook |
| `crates/chapbook-reader/src/layout.rs` | Honour `ReadingSettings::publisher_styles` (pass no author sheets when off) | Upstream flag was persisted but never read — a bug. **Candidate to send upstream.** |
| `crates/chapbook-viewer-gtk/src/linux.rs` | `f` (force reader font) and `s` (publisher styles off) toggles | Testing aid; Kalam's adapter sets both permanently |
| `crates/chapbook-core/src/diagnostics.rs` | Filter html5ever's stale "foster parenting not implemented" warning from the stderr logger | It fires per malformed table row in real books and means nothing |
| `Cargo.toml`, `crates/*/Cargo.toml`, `tools/chapbook-cli/Cargo.toml` | Removed members, dependencies and features that belonged to deleted crates | Strip, PLAN §4. Expect conflicts in these on every cherry-pick that touches a manifest; resolve by hand |
| `crates/chapbook-reader/Cargo.toml` | `cbz`/`pdf`/`opds`/`ureq` features removed; a `[lints.rust] unexpected_cfgs` check-cfg line names them so the untouched `cfg` sites in `src/` and `tests/` stay warning-free | Strip round 2. `src/` itself is unchanged |
| `crates/chapbook-reader/tests/*.rs` | Tests that open a CBZ/PDF fixture gated with `#[cfg(feature = "cbz")]`/`"pdf"` (same style as the two upstream already gated); `sources.rs` splits the `Format` import the same way | Fixtures are gone; the tests compile out instead of failing to find files |
| `crates/chapbook-core/tests/sniff.rs` | CBZ/PDF fixture tests removed; the "bytes beat the extension" test keeps its EPUB half | Fixtures are gone; `Format::sniff` itself is untouched |
| `tools/chapbook-cli/src/{main,commands}.rs`, `tests/`, `examples/show.rs` | `opds`, `lib add`, `lib sync` subcommands and image-book paths removed; `open_publication` is EPUB-only | The crates behind them are gone |
| `crates/chapbook-viewer-gtk/src/linux.rs` | Usage string says `<book.epub>` | Only format left |
| `crates/chapbook-core/src/page.rs`, `src/lib.rs` | New `Palette` type; `ReadingSettings.palette: Option<Palette>` (+ `palette()` accessor, hashed into `cache_key`) | Kalam's four themes have exact paper/ink colours; upstream's `Theme` is three fixed presets. `None` everywhere upstream constructs settings, so upstream behaviour is unchanged. **Candidate to send upstream.** |
| `crates/chapbook-layout/src/cascade/engine.rs` | `theme_css` takes `&ReadingSettings`, uses the effective palette's colours; `Light`+palette gets the Sepia-style sheet | Same feature |
| `crates/chapbook-reader/src/frame.rs` | Background / selection / highlight colours from `settings.palette()` | Same feature |
| `crates/chapbook-layout/tests/pagination.rs` | One new test (`a_palette_supplies_the_colours_and_the_theme_the_rules`) | Covers the feature; appended, nothing existing touched |
| `crates/chapbook-reader/src/lib.rs` | `mod host_position;` and `mod host_highlights;`; `Highlight` lives here (was in `annotations.rs`); `UnitState.resolved_host_highlights`, `Session.host_highlights`; the `library` field set, `OpenedBookId`, `book_id()`, `with_library_dir` and `SessionConfig.library_dir` removed; `save_position()` removed (the host saves what `layered_locator()` returns); `SettingsScope` kept but documented as inert | Host-owned highlights and positions; the library is gone (PLAN §7 step 4) |
| `crates/chapbook-reader/src/frame.rs` | Paints the host's highlights (the library's are gone); `mark_range`/`mark_rect`/`range_damage` ungated | Same |
| `crates/chapbook-reader/src/open.rs` | Library handshake removed (`shelve`, `restored_start`, fingerprinting, stored-annotation load, settings load, OPDS URL branch); every book opens at unit 0 with `ReadingSettings::default()`; initialises `host_highlights` | Same. **This is now the file most likely to conflict on a cherry-pick**; resolve by keeping ours and re-applying only the non-library part of the upstream change |
| `crates/chapbook-reader/tests/cache_budget.rs` | `PAGE` constant no longer gated on the removed `cbz` feature (the EPUB test uses it too) | Strip leftover; the file did not compile until CI ran the tests |
| `docs/STABILITY.md` | Rewritten for the eleven crates that remain, with `kalam-reader` and the demo placed in tiers | The `stability` test in `tools/chapbook-cli` checks the doc against the workspace; upstream's text named twelve crates the strip removed |
| `tools/chapbook-cli/tests/stability.rs` | Member-count floor 15 → 11 → 10 | Same test, same strip (then the library) |
| `crates/chapbook-reader/src/layout.rs`, `src/open.rs` | One `info` log line per chapter laid out (parse / fonts / images / style / paginate, in ms, plus pages and KB) and one per session opened (total, and the font system's share) | Finding where a slow first page spends its time; silent at the default `warn` level |
| `crates/chapbook-reader/src/annotations.rs` | **Deleted** (the library's highlight/note/bookmark storage) | Replaced by `host_highlights.rs`; `UnitCharContext`/`unit_char_context` moved to `host_position.rs` |
| `crates/chapbook-reader/src/cache.rs` | `suspend()` only releases caches; `library_mut()` removed | No database to close |
| `crates/chapbook-reader/src/nav.rs` | `persist_settings` is a no-op; `clear_book_settings` removed | Nothing to persist to |
| `crates/chapbook-reader/src/zoom.rs` | `view_rect`/`map_rect` ungated | They were gated on `library` only because their one caller was |
| `crates/chapbook-reader/src/conformance.rs` | `PositionSurvivesARestart` carries a `LayeredLocator` across the reopen (`layered_locator` → `goto_layered`) instead of `save_position` | Same check, the host's road |
| `crates/chapbook-reader/tests/{common/mod.rs,bidi,cache_budget,conformance,diagnostics,events,frames,lifecycle,positions,session,settings,sources,text_surface}.rs` | Library-dir plumbing removed; tests that asserted library behaviour (persistence across sessions, the shelf, per-book settings) deleted or rewritten as session-scoped / host-driven; `annotations.rs` deleted, `host_highlights.rs` added | Same |
| `crates/chapbook-viewer-gtk/src/linux.rs` | `h` key prints a note and clears the selection (no store to add to); `save_position` calls removed | Same |
| `tools/chapbook-cli/src/{main,commands}.rs`, `tests/lib_shelf.rs` | `lib` subcommand family removed; `lib_shelf.rs` deleted | Same |
| `Cargo.toml`, `crates/chapbook-reader/Cargo.toml`, consumers' `Cargo.toml` | `crates/chapbook-library` member, `rusqlite`, the `library` feature and `chapbook-library` dependencies removed | Same |
| `crates/chapbook-layout/src/paginate.rs` | Records, per page break, the flow space the break discarded (`gaps`, via `new_page`, `new_page_keeping` and `commit_margin`); `finish()` returns it as a third value | Continuous scrolling (PLAN §7 step 5, round G): pages glued end to end need the block margin the break threw away, or every seam reads as a missing blank line. Layout of the pages themselves is untouched |
| `crates/chapbook-layout/src/lib.rs` | `ChapterLayout.gaps`, `used_height(page)`, `gap_before(page)` | Same |
| `crates/chapbook-layout/tests/pagination.rs` | Five tests appended for `gaps`/`used_height` | Covers the feature |
| `crates/chapbook-reader/src/text_surface.rs` | `speakable_page` body moved to `speakable_unit_page(spine, page)`; `speakable_page` calls it for the current page | The scroll surface's tap-to-look-up needs words on a page that is not the session's |
| `crates/chapbook-reader/src/lib.rs` | `mod scroll;`, `pub use scroll::PageExtent` | Same feature |
| `crates/chapbook-reader/src/layout.rs` | The two fabricated image-book `ChapterLayout`s carry `gaps: vec![0.0]` | Struct gained a field |
| `crates/chapbook-reader/src/frame.rs` | The pending-jump landing at the top of `page_display_list` moved to `Session::land_pending` (in `scroll.rs`), called from there | `settle()` lands the same jumps for a frameless shell; one copy of the rule |
| `crates/chapbook-reader/src/cache.rs` | `evict_keeping` also spares `pinned_units`; `drop_metrics_dependent` and `release_caches` bump `layout_generation` | A scroll shell's visible band must not be evicted under it, and it needs to hear that layouts were dropped |
| `crates/chapbook-reader/src/lib.rs`, `src/open.rs` | `Session.layout_generation`, `Session.pinned_units` | Same |
| `crates/chapbook-paint/src/page.rs`, `src/display.rs`, `src/lib.rs` | `LineFragment` gains `ascent`/`descent`; `LineFragment::band_extent()`, `BAND_PADDING`; `rects_for_range` answers with the band, not the line box; new `DisplayOp::Band { rect, color, radius, blend }` and `Blend`, `BAND_RADIUS`; `Selection` gains `blend`; `push_selection_rect` emits `Band`s | The selection band (R12j): glyph box + 2 px, 2 px corners, multiply/screen — the old reader's look. Upstream filled the whole line box square with no blend |
| `crates/chapbook-layout/src/paginate.rs` | `ShapedLine` carries `ascent`/`descent` read off the `LayoutLine` a run came from (found by `ptr::eq` on its `glyphs`); the three `LineFragment` constructors fill them | Same |
| `crates/chapbook-reader/src/layout.rs` | PDF hidden-text lines fill `ascent`/`descent` (0.8/0.2 of the line) | Struct gained fields |
| `crates/chapbook-reader/src/frame.rs`, `src/scroll.rs` | `Selection { blend }`: stored highlights `Normal`, the live selection `selection_blend()` | Same |
| `crates/chapbook-reader/src/text_surface.rs` | `Session::selection_grab_end(start)` (a handle press: makes that end the focus so `selection_drag` moves it) and `Session::selection_blend()` (luma of the palette ground → `Multiply`/`Screen`) | Draggable handles (R12j) |
| `crates/chapbook-reader/src/zoom.rs` | `apply_view` scales `Band` like `FillRect` | New op |
| `crates/chapbook-render-tinyskia/src/lib.rs` | `Band` → `fill_band`: rounded `PathBuilder` path, `Paint.blend_mode` | New op |
| `crates/chapbook-layout/tests/pagination.rs`, `crates/chapbook-reader/tests/{bidi,session}.rs` | Selection fills matched as `Band`; tests for the band geometry, `selection_grab_end`, `selection_blend` | Covers the feature |
| `crates/chapbook-layout/src/dom/parse.rs` | `parse_xhtml` parses as XML first (xml5ever, `parse_as_xml`) and falls back to the HTML algorithm (`parse_as_html`) when the XML parser reported any error or the root is not an XHTML `<html>`; `Sink` counts `parse_error`s and `finish()` returns them; the shared post-parse passes moved to `finish()` | R15: the HTML algorithm has no `<a id="x"/>` shorthand, so a self-closed anchor swallowed the rest of a Penguin Random House chapter and every paragraph flattened into one. Browsers read `.xhtml` as XML; so does the engine now |
| `crates/chapbook-layout/Cargo.toml`, `src/dom/mod.rs`, `docs/ARCHITECTURE.md` | `xml5ever` is a plain dependency; the `strict-xml` feature (which no code ever read) is gone | Same |
| `crates/chapbook-core/src/locator.rs` | `LOCATOR_VERSION` 2 → 3 | The XML tree keeps the whitespace text node between `<html>` and `<head>` that the HTML parser drops (one `\n` at the front of a typical chapter's locator text), and mis-nested trees change shape; stored version-2 offsets heal through the quote path |
| `crates/chapbook-layout/tests/parse_extract.rs`, `tests/snapshots/locator_text__golden_offset_map_chapter1.snap`, `tools/chapbook-cli/tests/snapshots/layout_snapshot__*.snap` | Six parse tests appended (the PRH shape, fallback cases, XML prologue, the `xml:lang`/`lang` pair); goldens re-baselined at +1 once the fixtures actually took the XML path (R17) | Covers the feature |
| `crates/chapbook-layout/src/dom/parse.rs` | `Sink::parse_error` disregards xml5ever's "Duplicate attribute" report | R17: xml5ever 0.39 compares attribute names by local name only, so `xml:lang="en" lang="en"` — on the `<html>` of most EPUBs — was a "duplicate" and sent every such book to the HTML parser, re-opening the R15 bug. Fixed upstream in xml5ever 0.40 (servo/html5ever#780); drop this line when the dependency moves |
| `crates/chapbook-core/src/diagnostics.rs`, `src/lib.rs` | `is_dependency_noise(&Record)` (public) names the two stale parser warnings; `Stderr::log` uses it | R17: xml5ever warns "stop_parsing for XML5 not implemented" at the end of every document it finishes — once per chapter, on success. Same class as the html5ever foster-parenting line, so one predicate, exported for shells with their own `log` backend |
| `crates/chapbook-reader/tests/diagnostics.rs` | The capturing logger drops `is_dependency_noise` records | A host backend would; `a_book_that_opens_cleanly_says_nothing_alarming` otherwise fails on the xml5ever line |
| `crates/chapbook-core/src/page.rs` | `ReadingSettings.user_css: Option<String>` (default `None`, hashed into `cache_key`) | R16: a shell's own stylesheet — Kalam's reading skin — appended at user origin. `None` everywhere upstream constructs settings. **Candidate to send upstream.** |
| `crates/chapbook-layout/src/cascade/engine.rs` | `StyleEngine::new` appends `user_css` after the theme and typeface sheets; `font_family_css` selects `html, body` instead of `*` | R16: `*` overrode every family the publisher chose on purpose (sans headings, script faces); the body font is the reader's, the rest is the book's |
| `crates/chapbook-layout/src/style_to_attrs.rs` | New `family_for(style, known)` walks the computed `font-family` list (first known name, else its generic, else serif); `attrs_for` takes the resolved `Family` | R16: upstream passed only the first name to cosmic-text, whose fallback is per glyph, so `Georgia, …, serif` on a machine without Georgia landed in an arbitrary face. Unit tests in the module. **Candidate to send upstream.** |
| `crates/chapbook-layout/src/paginate.rs` | `Paginator::family_of` + `known_families` cache; `shape_inline` resolves families per run | Same feature |
| `crates/chapbook-layout/tests/cascade.rs` | Two font-family tests rewritten to the `html, body` contract; four `user_css` tests appended | Covers the feature |
| `crates/chapbook-layout/src/boxtree.rs` | `has_block_child` is a function; an inline child that wraps a placeable replaced element (`wraps_replaced`/`replaced_present`) counts as block-level, and `collect_container` builds it as a block instead of flattening it into the line | R21: any `<img>` behind two or more inline elements (`<a><img/></a>` inside a span, PRH's `div.squeeze > div.squeeze > img`) was dropped — the whole Part I page of a real book rendered blank. Upstream carries the same limit (documented as "images nested deeper than one level … are skipped"). **Candidate to send upstream.** |
| `crates/chapbook-layout/tests/pagination.rs` | Six tests appended under "replaced elements inside inline wrappers" | Covers the fix and its edges (text-only wrapper, undecoded image, hidden image, floated image behind a link) |
| `crates/chapbook-paint/src/images.rs` | `ImageLook` (`Paper`/`Picture`, judged from a pixel sample at `insert`, kept on `StoredImage.look`), `ImageStore::look()`, `ImageStore::negate_paper()` (+ `StoredImage.negative`) | R22: the old reader's night themes inverted every image and excepted covers/photos by class name; the engine tells paper from pictures by their pixels. Negating paper once per store lets a dark session screen plates straight from the store instead of copying a full-page plate per draw |
| `crates/chapbook-paint/src/display.rs` | `DisplayOp::Image` gains `treatment: ImageTreatment` (`Plain`/`Multiply`/`Invert`, chosen per image from its look and the page ground); `build_display_list` takes the `ImageStore` as a fourth argument; `is_dark_ground()` is public | R22. Every caller in the workspace passes the store (`frame.rs`, `scroll.rs`, the CLI, `timings.rs`, two layout tests) |
| `crates/chapbook-render-tinyskia/src/lib.rs` | `draw_image` honours the treatment: multiply as is, screen the negative (from the store when it holds one, else a per-draw `inverted_copy`) | R22 |
| `crates/chapbook-reader/src/layout.rs` | `layout_text_unit` calls `negate_paper()` on a dark ground | R22; the store is rebuilt on a theme change, so a light ground never sees negatives |
| `crates/chapbook-reader/src/text_surface.rs` | `selection_blend` uses `chapbook_paint::is_dark_ground` | One definition of "dark ground" |
| `crates/chapbook-reader/tests/session.rs`, `fixtures/epub/plates.epub` (+ `src/plates/`) | Two tests: a picture stays plain on every ground; a PRH-shaped plate is plain on white, multiplied on sepia, inverted on dark, with the pixels checked | Covers R22 end to end |

New files inside inherited crates (no conflict risk, listed for completeness):

| File | What |
|---|---|
| `crates/chapbook-reader/src/host_position.rs` | `Session::layered_locator()`, `layered_locator_at()`, `goto_layered()`, `unit_fraction()`, `chapter_char_count()`, `word_at_exact()` — positions as *values* for a host with its own database, and the tap hit-test |
| `crates/chapbook-reader/src/host_highlights.rs` | `HostHighlight` + `Session::set_host_highlights()`, `show_host_highlight()`, `recolor_host_highlight()`, `hide_host_highlight()`, `host_highlights()`, `host_highlight_at()`, `goto_host_highlight()` — highlights the host stores itself, held in memory and resolved like stored annotations; not gated on `library` |
| `crates/chapbook-reader/src/scroll.rs` | `PageExtent` + `Session::page_count_of()`, `is_laid_out()`, `page_extent()`/`page_extents()`, `page_frame()`, `render_page()`, `offset_at_page()`, `word_at_page()`, `link_at_page()`, `host_highlight_at_page()`, `range_rects_on_page()`, `selection_begin_on_page()`/`selection_drag_on_page()`, `set_position()`, `page_of()`, `page_of_anchor()`, `settle()`, `layout_generation()`, `pin_units()` — the by-page surface a scrolling shell composes a continuous view from; the paged API is untouched |
| `crates/chapbook-reader/tests/scroll.rs` | Ten tests over that surface on `long.epub` |
| `crates/chapbook-reader/src/host_position.rs` | The whole-book char count moved out of `unit_char_context` into `pub fn chapter_char_counts()`, which it now calls; the count is timed at `info` | The scrolling widget (round H) guesses unmeasured chapters' heights from their lengths; same pass, shared |
| `crates/chapbook-reader/src/scroll.rs` | `line_at_page()`, `line_rect_at_page()`, `speakable_page_of()` | Round H: keeping the reading line on the same text across a relayout, and tap-to-look-up on any band |
