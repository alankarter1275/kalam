# Kalam UI/UX Design System & AI Guidelines

This document outlines the strict design philosophy, layout invariants, and UX interaction models for Kalam. Any AI agent working on Kalam's frontend, GTK layout, CSS, or HTML mockups **must read and adhere to these rules implicitly**.

## 1. The Core Philosophy: "Zero Distraction"

Kalam is a reading app, not a SaaS dashboard. The reading experience must be completely pure, edge-to-edge, and devoid of any artificial "app-like" structural elements.

* **NO BORDERS**: Never enclose the reading text in a bordered box, card, or container with a different background color. 
* **NO BOOK SKEUOMORPHISM**: Do not add fake book spine creases, page drop-shadows, or 3D paper textures. The background is a solid color (`var(--bg)` or `var(--paper)`), and the text sits directly on it.
* **Edge-to-Edge Canvas**: The main text column has a `max-width`, but the container holding it spans the entire width and height of the screen.

## 2. The Interaction Model: Hidden Chrome

All app-related UI controls (chrome) must be completely invisible during reading and only reveal themselves through deliberate user action.

* **Strict Edge Hover Zones**: Interactive chrome (Back buttons, Top Docks, Timeline scrubbers) should be `opacity: 0` by default. They only appear when the user moves their mouse into a specific absolute hover zone (e.g., the top 80px of the screen, or the extreme top-left corner).
* **Never Use Global Mousemove**: Do not make chrome appear globally anytime the mouse moves. It must be tied to specific edge-hover interactions (or GTK's `mouse_in_top_edge` equivalents).
* **Static "Printed" Elements**: Elements that belong to a printed book (e.g., page numbers, progress percentage, running chapter titles) can remain statically visible at the bottom or top corners, provided they are styled with faint, subtle typography (e.g., `color: var(--text-3)`).

## 3. Sidebar Architecture: Floating Overlays (Arc-style)

Sidebars (TOC, Notes, Settings) must **never** push or shrink the reading canvas when they open.

* **Absolute Positioning**: Sidebars are floating panels that sit *on top* of the reading canvas (`z-index: 100`).
* **Arc Browser Aesthetics**: They should have a margin from the screen edge (e.g., `12px` or `16px`), rounded corners, and a drop shadow. They are *not* flush, full-height OS panes.
* **Edge Triggers**: Sidebars are triggered by invisible 15px-24px hover zones on the extreme left and right edges of the screen, or via keyboard shortcuts (`T`, `S`).

## 4. Zen Browser-style Sidebar Navigation

Sidebar internals should take inspiration from Zen Browser's vertical tab interaction model.

* **Bottom Action Dock**: The tabs for switching sidebar panels (e.g., TOC, Highlights, Settings) live in a dock at the *bottom* of the sidebar.
* **Touchpad Swiping**: Users must be able to switch between these sidebar panels without clicking, simply by doing a 2-finger horizontal swipe on their touchpad.
* **Icon-only**: The bottom dock should use icons only, with the active tab highlighted by `var(--accent-pale)` background and `var(--accent)` color.

## 5. Clever Micro-Interactions

Kalam demands elegant, highly specific micro-interactions for its controls.

* **The Back Pill Slide-out**: The top-left "Library" back button is a pill. When the user hovers over this pill, a secondary action (like a "Minimize to Bubble" button) must elegantly slide out from *behind* the back button. It should look physically attached and emerge via a `transform: translateX()` from a lower `z-index`.
* **Integrated Branding**: The global Kalam logo must be rendered using the transparent PNG (`assets/logo.png`) applied via a CSS `-webkit-mask-image` over a background color of `var(--accent)`. **Never** use a text 'k' or raw emoji for the logo.

## 6. Colors and Typography

* **No Emojis**: (Enforced by `GEMINI.md`) Never use emojis in the UI. Use Material Symbols Rounded.
* **Typefaces**: 
  * `DM Sans` for UI chrome, tags, metadata, and buttons.
  * `Playfair Display` for serif headings, book titles, and the drop-cap of the first paragraph.
  * `Literata` for the actual book prose.
* **Theme Variables**: All UI components must use the standard CSS variables: `--bg`, `--text`, `--text-2`, `--text-3`, `--accent`, and `--border`.
