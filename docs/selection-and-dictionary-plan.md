# Selection Highlights, Teardrop Handles, Actions Toolbar, and Dictionary Popover

## 1. Overview & Requirements
- **Selection Highlight**: Decrease band height and introduce a clean 3px–4px gap between lines on multi-line selections (no more solid colored blocks).
- **Click Behavior**: Double-click selects the tapped word; triple-click selects the full paragraph. Single-click turns pages or clears selection.
- **Handlers**: The vertical line matches the height of the edge of the selection highlight. At the start handle, teardrop shape sits at the top with its narrow point joining the line. At the end handle, teardrop shape sits at the bottom with its narrow point joining the line. Bulb diameter is standard ~9px.
- **Actions Dialog Box**: Appears automatically upon selection (drag, double-click, triple-click). Contains 4 actions: **Highlight** (expands color picker), **Quote**, **Dictionary**, **Copy**, styled after sidebar aesthetics.
- **Dictionary Popover**: Built from `docs/files/kalam_dictionary_popup_v3.html` as a compact anchored popover (~320px × ~400px) with headword, IPA pronunciation, POS tag badge, and scrollable senses matching sidebar styling.

## 2. Components Affected & Implementation Status
- `../kalam-engine/crates/chapbook-paint/src/page.rs`:
  - Enforced `MIN_LINE_GAP: 3.5` and `BAND_PADDING: 0.5` in `LineFragment::band_extent()`.
  - Ensures a crisp 3.5px vertical gap between consecutive lines of multi-line selection highlights.
- `../kalam-engine/crates/kalam-reader/src/handles.rs`:
  - Updated teardrop diameter to ~9px (`GRIP: 9.0`).
  - Seamlessly joined teardrop tangent points to the ends of the vertical bar (`self.top + BAR_WIDTH/2.0` and `self.bottom - BAR_WIDTH/2.0`).
  - Bar height exactly matches the selection highlight edge (`bottom - top`).
- `../kalam-engine/crates/chapbook-reader/src/scroll.rs` & `src/text_surface.rs`:
  - Added `paragraph_at_page`, `select_paragraph_at_page`, and `select_paragraph_at` to accurately select full paragraphs by matching block tags (`Fragment.tag`).
  - Added `select_word_at_page` for scrolled mode.
- `../kalam-engine/crates/kalam-reader/src/view.rs`:
  - Added `gtk::GestureClick` alongside `GestureDrag` with `multi_click` state tracking.
  - Double-click (`n_press == 2`) selects the word and triggers the actions chip.
  - Triple-click (`n_press >= 3`) selects the entire paragraph and triggers the actions chip.
  - Single-click tap clears active selection before considering page turn zones.
- `src/pages/reader/mod.rs` & `src/pages/reader/js_bridge.rs`:
  - Restored `ReaderMsg::EngineSelection` to build and popup `self.selection_chip`.
  - Restored `ReaderMsg::LookUpSelection` to invoke `dict_lookup` and display `self.dict_popover`.
  - Cleaned up unused annotations.
- `src/pages/reader/engine.rs` & `resources/style.css`:
  - Implemented `build_selection_chip` with 4 action buttons (Highlight, Quote, Define/Dictionary, Copy) styled as `.k-sel-toolbar` with pure symbolic icons (no text words) and compact pill geometry.
  - Added `gtk::Revealer` (slide-right) for the 5-color palette: initially collapsed/hidden, smoothly slides out when clicking the Highlight action button, and slides back in when clicked again.
  - Fixed stretched circular buttons: `.k-color-dot` and `.k-save-btn` now use fixed 16px/24px dimensions with `9999px` border radius and `padding: 0`.
  - Scaled down dictionary popover: width 240px, max-height 160px scroll area, compact header with 17px headword and proportional layout matching mockup.
  - In `chapbook-paint/src/page.rs`: decreased selection highlight band height from top by adding `TOP_INSET: 2.5px`, fixing the bottom boundary where it is so it sits snug against uppercase glyph tops.

## 3. Popover Refinements & GTK CSS Warning Elimination
- **GTK CSS Warning Elimination**:
  - Removed unsupported `max-width`, `max-height`, and `-gtk-outline-radius` properties from `resources/style.css` (`.k-sel-action`, `.k-sel-divider`, `.k-color-dot`, and `.k-save-btn`), preventing runtime `Gtk-WARNING` parser errors.
  - Explicit sizing is strictly enforced through GTK `set_size_request(...)` and CSS `min-width`/`min-height`/`border-radius`.
- **Custom Symbolic Icons**:
  - Added `src/icons.rs` embedded SVG registration for `kalam-highlight-symbolic`, `kalam-quote-symbolic`, `kalam-dictionary-symbolic`, and `kalam-copy-symbolic`.
  - Registered to `~/.local/share/kalam/icons` search path via `gtk::IconTheme::for_display(...)` during application startup (`src/main.rs`).
  - Eliminates GTK missing-icon fallback glyphs (🚫 circle/box with diagonal line) completely.
- **Color Selection Palette Slide-out**:
  - `build_selection_chip` binds `highlight.connect_clicked` and `colors_revealer.connect_child_revealed_notify` with `popover.present()`.
  - Dynamically recalculates popup layout and repositioning so the 5 solid vivid swatches slide smoothly out beside the highlight button without clipping.
- **Dictionary Popover Triangle Removal & Compact Layout**:
  - Set `popover.set_has_arrow(false)` on `build_dict_popover` to remove the top pointer arrow.
  - Reduced popup width to 240px and max content height to 160px for a refined, compact aesthetic.


