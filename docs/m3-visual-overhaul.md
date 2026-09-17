# Material 3 & Libadwaita Visual Overhaul

## Overview
Implemented Step 5 of Part 1: Material 3 / Libadwaita visual overhaul across all application views in `src/pages/`, `src/widgets/book_row.rs`, and `resources/style.css`.

## 1. Material 3 Design System Tokens (`resources/style.css`)
Added design tokens mapped dynamically from Kalam theme variables:
- `@m3_surface`: Elevated surface background (`@kalam_surface`).
- `@m3_surface_variant`: Secondary surface (`@kalam_surface_2`).
- `@m3_primary`: Primary action & selection accent (`@kalam_accent`).
- `@m3_on_primary`: Contrast text on primary elements (`@kalam_bg`).
- `@m3_outline`: Subtle border outlines (`@kalam_border`).
- `@m3_shadow`: Soft elevation shadows (`alpha(#000000, 0.35)`).

## 2. Component & View Refinements
- **Home Dashboard (`src/pages/home.rs`)**:
  - Hero card ("Continue Reading") styled with 16px M3 elevated surface, outline border, and subtle shadow (`0 4px 16px @m3_shadow`).
  - Stat tiles and shelf cards refined with Libadwaita card elevation and smooth hover transitions.
- **Library Grid (`src/pages/all_books.rs` & `src/widgets/book_row.rs`)**:
  - Cover frames (`.kalam-cover-frame`) enhanced with 8px rounded corners and crisp cover shadows (`0 4px 14px @m3_shadow`).
  - Libadwaita card hover elevation (`0 10px 26px alpha(#000000, 0.55)` and `translateY(-3px)`).
  - Material 3 pill selection badges (`.kalam-selection-badge`) with `@m3_primary` background and `@m3_on_primary` text.
- **Book Detail & Floating Card (`src/pages/book.rs` & `src/pages/book_float.rs`)**:
  - Metadata cards (`.kalam-detail-card`, `.kalam-float`) updated with 16px–20px rounded corners and M3 elevated shadow (`0 24px 64px alpha(#000, 0.45)`).
  - Pill-shaped action buttons (`.kalam-primary-btn`, `.kalam-btn-read`, `.kalam-secondary-btn`, `.kalam-icon-btn`) using `9999px` border radius and hover elevation transitions.
  - Star ratings (`.kalam-star-picker`) styled with warm golden rating stars and smooth scale/color hover transitions.
  - Author and tag chips (`.kalam-chip`) updated to pill shape (`9999px` border radius).
- **Settings Page (`src/pages/settings.rs`)**:
  - Grouped settings into clean Libadwaita preference group cards (`.kalam-section-card`) with 16px rounded corners, header dividers (`.kalam-section-divider`), and subtle row hover highlights.
  - Material-You / Libadwaita pill switches (`switch.kalam-switch`) with `9999px` border radius and smooth color/transform transitions for slider and trough.
- **Analytics Page (`src/pages/analytics.rs`)**:
  - Material 3 elevated card styling for reading time charts (`.kalam-chart-card`), headline hero cards (`.kalam-hero-card`), and bar charts.
  - Daily streak strip (`.kalam-streak-day`) with rounded pill containers and active day highlighting.
  - Top author/tag leaderboards (`.kalam-rank-row`) updated with M3 rounded card styling.
