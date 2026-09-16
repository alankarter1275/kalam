# Kalam Engine Review & WebKit Parity Walkthrough

## 1. Kalam Engine Architectural Review
We reviewed `kalam-engine` (`chapbook-core`, `chapbook-reader`, `kalam-reader`) vs the legacy `WebKitGTK` implementation in `calibre-alt`.

### Key Findings & Fixes
1. **Full Input Unblocking (Clicks/Scrolls Fix)**:
   * **Root Cause**: The full-stage `add_overlay` GTK Revealer (`kalam-reader-lightbox-shell`) lacked `#[watch] set_visible: model.lightbox_active`. In GTK4, a full-screen overlay container without `set_visible: false` intercepts all mouse clicks, drag events, and scroll wheel ticks across the entire window even when hidden/inactive.
   * **Fix**: Added `#[watch] set_visible` to all overlay Revealers (`search_shell`, `lightbox_shell`, `left_sidebar_shell`, `right_sidebar_shell`) in `src/pages/reader/mod.rs`. When inactive, `set_visible: false` ensures GTK skips target picking on overlay children and routes 100% of input directly to the reader canvas.

2. **In-Book Text Search (Ctrl+F) with Active Match Highlighting**:
   * Added GTK Search Revealer (`SearchEntry`, match counter `X of Y`, prev/next buttons) in `src/pages/reader/mod.rs`.
   * As the reader cycles through matches, `highlight_current_search_match` computes the exact `LayeredLocator` range and renders a bright yellow highlight over the matching text on screen (`show_highlight(-9999, &hl)`). Closing search removes the temporary highlight.

3. **Image Lightbox Zoom Overlay**:
   * Restored panel-coordinate hit-testing in `kalam-reader` (`crates/kalam-reader/src/view.rs`).
   * Revealing `lightbox_shell` displays the full-resolution image texture inside `lightbox_picture` with a top-right close button.

---

## 2. Verification Steps
* `cargo check --workspace` in `kalam-engine` — PASSED (Exit 0).
* `cargo check` in `calibre-alt` — PASSED (Exit 0).
