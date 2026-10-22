# Archived `kalam-engine` README

> **Archive notice:** This file describes the abandoned from-scratch engine
> plan. Kalam uses the local Chapbook crates in the current workspace; see the
> repository-root `README.md`, `ARCH.md` and `ROADMAP.md` for current project
> information.

**`kalam-engine`** was the proposed standalone, pure-Rust reading and text-rendering engine for **Kalam** (`calibre-alt`).

It replaces heavy browser-based webviews (`WebKitGTK`) with a lightweight, sub-millisecond, pure-Rust rendering pipeline powered by `lol_html`, `cosmic-text`, and `tiny-skia`.

---

## 🎯 Purpose & Scope

The sole mission of `kalam-engine` is to provide a fast, memory-efficient, pixel-perfect text reading experience for reflowable content (EPUBs, clean HTML, plain text, and fiction).

It acts as a library crate imported directly by the main Kalam application. Kalam provides the GTK application shell, library database, dictionary engines (`src/dict.rs`), and sidebar controls; `kalam-engine` provides the reading canvas, layout calculations, and pagination model.

---

## 🏛 Architecture Summary

1. **Normalization Pipeline (`lol_html`):** Strips complex, publisher-supplied CSS, cleans XML/HTML structures, and normalizes documents into clean Semantic HTML (headings, paragraphs, strong, em, images).
2. **Text Layout & Shaping (`cosmic-text`):** Performs font fallback, bidirectional text layout, line breaking, and exact pixel-bound measurement.
3. **Pagination & Display List:** Computes discrete page splits into a vector of render commands (`DisplayList`).
4. **Rasterization (`tiny-skia` / GTK4):** Paints display list commands onto GTK drawing surfaces at native 60fps+ performance.
5. **Quote-Anchored Position Tracking (`LayeredLocator`):** Tracks reading progress, bookmarks, and highlights using multi-layered text quote anchors rather than volatile DOM offsets.

---

## 🚀 Getting Started

### Prerequisites
Ensure GTK4 and Rust toolchains are installed on your system.

### Running the Demo App
To test EPUB parsing and text rendering in a standalone GTK4 window:

```bash
cargo run --example demo
```

---

## 📚 Documentation Map
- **[`docs/ARCHITECTURE.md`](./ARCHITECTURE.md):** Detailed breakdown of the rendering pipeline, pagination data structures, and GTK painting model.
- **[`docs/RESTRICTIONS.md`](./RESTRICTIONS.md):** Non-negotiable architectural boundaries (No C++ `stylo`, No WebKit/WebViews, No JS bridges).
- **[Root `ROADMAP.md`](../../../ROADMAP.md):** the current project roadmap;
  the abandoned six-step `kalam-engine` proposal is summarized under
  “First custom-engine proposal.”
