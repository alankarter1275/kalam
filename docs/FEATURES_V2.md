# Feature Architecture v2.0 (Post-WebKit Parity)

This document outlines the architectural blueprints for replacing the final features lost during the WebKit-to-Kalam-Engine migration. These features will bring the pure-Rust engine to complete functional parity with browser-based readers, while maintaining zero DOM-overhead and sub-millisecond performance.

---

## 1. Persistent Highlights & Annotations (Engine-Level Paint)
**Goal:** Render user-saved highlights behind text without modifying layout or text shaping.

**Architecture:**
*   **API:** Engine exposes `set_highlights(Vec<(LayeredLocator, Color)>)`.
*   **Math:** During layout, the engine's fragmentation pass calculates the physical `(x, y, w, h)` bounding boxes for the text spans corresponding to the `LayeredLocator`.
*   **Painting:** These bounding boxes are saved to the `DisplayList`. During the `tiny-skia` render pass, `fill_rect` is called *before* the text glyphs are drawn.
*   **Why:** Guaranteeing that text anti-aliasing is never blurred by GTK overlays, and avoiding horizontal layout shifts that would occur if we modified `cosmic-text` styling attributes inside ligatures.

## 2. In-Book Text Search (KOReader Approach)
**Goal:** Implement `Ctrl+F` search with visual highlighting that does not clash with persistent user annotations.

**Architecture (Unified Math, Segregated State):**
*   **API:** Engine exposes `search(query: &str) -> Vec<LayeredLocator>`.
*   **State:** The engine maintains two distinct memory layers: `persistent_highlights` and `transient_search_results`.
*   **Painting Order:** The engine calculates bounding boxes for both lists using the exact same math pipeline. It paints persistent highlights first, and transient search results (e.g., in bright orange) on top. 
*   **Why:** Prevents clearing search results from accidentally wiping user data, while keeping the geometric math perfectly DRY (Don't Repeat Yourself).

## 3. Image Zoom / Lightbox (Hit-Testing)
**Goal:** Allow users to zoom in on detailed illustrations or maps.

**Architecture (Decoupled UI):**
*   **Engine State:** The engine is purely mathematical. It tracks the bounding boxes of images in the `DisplayList` but performs zero UI zooming.
*   **Hit-Test:** When the user clicks the canvas, the engine checks if `(x, y)` intersects an image bounding box.
*   **Signal:** If hit, the engine emits an `ImageClicked(Arc<[u8]>)` signal containing the raw image data.
*   **Host UI:** Kalam (`calibre-alt`) catches the signal and spawns a native GTK4 Lightbox overlay, utilizing hardware-accelerated pan and zoom gestures.

## 4. Hyperlinks (Footnotes & Cross-References)
**Goal:** Allow users to click `[1]` footnote superscripts and instantly jump to the reference.

**Architecture:**
*   **Layout:** The `DisplayList` tracks `LinkBox(x, y, w, h, href)`.
*   **Hit-Test:** Clicking the canvas checks against `LinkBox` coordinates.
*   **Navigation:** If hit, the engine instantly repaginates or scrolls to the target `href` anchor (translating the anchor to a `LayeredLocator` internally).
