# Kalam Engine Review & WebKit Parity Walkthrough

## 1. Kalam Engine Architectural Review
We reviewed `kalam-engine` (`chapbook-core`, `chapbook-reader`, `kalam-reader`) vs the legacy `WebKitGTK` implementation in `calibre-alt`.

### Key Findings & Fixes
1. **Raw RGBA Pixbuf Lightbox Fix**:
   * **Root Cause**: `show_image_lightbox` attempted to parse `rgba` byte vectors via `Pixbuf::from_stream`. Because `kalam-reader` emits raw uncompressed RGBA pixel buffers (without PNG/JPEG headers), `from_stream` failed every time and silently aborted without displaying the texture.
   * **Fix**: Replaced stream loading with `Pixbuf::from_bytes(&bytes, Colorspace::Rgb, true, 8, w, h, w * 4)`.

2. **Full Input Unblocking (Clicks/Scrolls Fix)**:
   * Added `#[watch] set_visible` to all overlay Revealers in `src/pages/reader/mod.rs` so inactive overlays do not block mouse clicks, drags, or scroll wheel events.

3. **In-Book Text Search (Ctrl+F) with Active Match Highlighting**:
   * Added GTK Search Revealer and live match highlighting via `show_highlight(-9999, &hl)`.

---

## 2. Verification Steps
* `cargo check --workspace` in `kalam-engine` — PASSED (Exit 0).
* `cargo check` in `calibre-alt` — PASSED (Exit 0).
