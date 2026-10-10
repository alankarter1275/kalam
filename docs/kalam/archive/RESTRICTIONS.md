# Architectural Restrictions & Guardrails

> ## ⚠️ SUPERSEDED — two of these four rules are now false
>
> The live document is [`../RESTRICTIONS.md`](../RESTRICTIONS.md). Read that.
>
> **Rule 1's ban on `stylo` was lifted on 2026-09-08.** The stated reason for
> the ban — "C++ toolchain requirements" — was incorrect; stylo is Rust. Five
> stylo crates are now pinned in the workspace root and twelve files in
> `chapbook-layout` are built on them.
>
> **Rules 1 and 4's `lol_html` sanitizer was never built** and is not a
> dependency anywhere. Publisher CSS is no longer stripped; it goes through a
> real cascade, with the host's sheets appended at user origin. That means
> rule 4's "MUST be stripped" does not hold — see the origin table in the live
> document for what theme authority actually is.
>
> Rules 2 and 3 still hold. Kept below for the record of what the first plan
> intended.

To ensure `kalam-engine` remains faithful to its core mission and never degenerates into a bloated web browser engine, every contributor and AI agent must strictly enforce the following rules.

---

## 🚫 1. Absolute Prohibition of C++ Web Engines (No WebKit, No Stylo)
- **NO `webkit2gtk` or WebViews:** `kalam-engine` MUST NEVER instantiate or depend on a browser engine.
- **NO `stylo` (Firefox CSS Engine):** Do not bring in `stylo` or Gecko C++ dependencies. `stylo` introduces massive compilation overhead and C++ toolchain requirements.
- **Rule:** If a feature cannot be rendered via pure-Rust text layout (`cosmic-text`), it must be stripped or simplified via `lol_html`.

---

## 🚫 2. No JavaScript & No JS Bridges
- `kalam-engine` operates entirely in compiled Rust code.
- Selection detection, cursor movement, word-hitting (`hit_test`), and pagination MUST be calculated in Rust.
- Never inject JavaScript scripts, V8/JSC runtimes, or IPC message handlers into the rendering pipeline.

---

## 🚫 3. No Feature Creep Beyond Engine Scope
- **NO Database Logic:** `kalam-engine` does NOT touch SQLite, database files, or library management. That is the responsibility of the host app (`kalam`).
- **NO Network / Scraping Logic:** `kalam-engine` does NOT make HTTP requests or parse online websites. It consumes local byte streams (EPUB files, HTML buffers).
- **NO Comic / PDF Engines:** `kalam-engine` focuses strictly on reflowable text. PDF rasterization and CBZ archive paging are handled by native GTK components in Kalam.

---

## 🔒 4. Strict Theme Obedience
- Publisher CSS rule overrides (e.g. fixed font-family, fixed text colors, fixed line heights, forced backgrounds) MUST be stripped by the `lol_html` sanitizer.
- The engine MUST enforce Kalam's active theme parameters (colors, font family, line height, paragraph spacing, margins) as authoritative.
