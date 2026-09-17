# Comic & CBZ Reader Polish & Lanczos3 Remaster Tool

## Overview
Sub-Step B of Step 4 adds full-featured reading polish for comic archives (CBZ/CBR) in `src/pages/comics_reader/` and a Lanczos3 image upscaler remaster tool in `src/comics.rs`.

---

## 1. Comic Reader Features (`src/pages/comics_reader/`)
* **RTL Manga Mode**:
  * Added dynamic RTL navigation logic (`ReadingDirection::Rtl`).
  * Tap gestures on left/right 35% of the viewport and keyboard Left/Right arrows adapt based on direction (in RTL: Left tap / Left Arrow turns to next page, Right tap / Right Arrow turns to previous page).
* **Webtoon Mode**:
  * Smooth continuous vertical strip scrolling.
  * Pages load dynamically into Webtoon item slots without rebuilding the `ScrolledWindow` viewport child, preventing scroll position resets.
  * Added vertical scroll position listener (`vadjustment`) to update `current_page` and trigger preloading dynamically while scrolling.
* **Double-Page Spread Mode**:
  * Renders 2 pages side-by-side in landscape.
  * Supports LTR (`[Page A | Page B]`) and RTL Manga spreads (`[Page B | Page A]`).
  * Handles single-page end/cover boundaries with empty expanding container placeholders to preserve layout alignment.
* **0-Latency Viewport Preloading**:
  * `trigger_loads` fetches and decodes image bytes into `gdk::MemoryTexture` off the main GTK thread in background tasks (`crate::tasks::spawn`).
  * Window preloads `current_page ± 2` (or `current_page - 4 ..= current_page + 6` in Webtoon mode).
  * Retains textures bounded to the active window to bound memory usage.

---

## 2. Lanczos3 Remaster Tool (`src/comics.rs`)
* **Upscaler Core (`remaster_comic_cbz`)**:
  * Unpacks `.cbz` / `.cbr` zip entry images.
  * Rescales low-res scans using `image::imageops::FilterType::Lanczos3` to preserve ink line sharpness and smooth screen tones.
  * Encodes rescaled images and repacks with deflated ZIP compression into a high-res CBZ archive.
  * Preserves metadata and non-image entry files untouched.
* **UI Action "Remaster Comic"**:
  * Integrated into book detail page (`src/pages/book.rs`) and floating book details (`src/pages/book_float.rs`).
  * Background task execution with progress reporting and notifications.

## 3. Review Fixes & Architectural Refinements
* **Webtoon Navigation & Scroll Sync**:
  * Fixed issue where page navigation actions (`SetPage`, `NextPage`, `PrevPage`, arrow keys) in Webtoon mode updated `current_page` without moving the `ScrolledWindow` position.
  * Added programmatic `vadjustment.set_value` synchronization on navigation actions in Webtoon mode.
  * Fixed Fit Mode switching in Webtoon mode so all strip images update their `content_fit` properties.
* **Double-Page Spread Pair Alignment**:
  * Fixed page boundary stepping in Double-Page mode so even/odd page pairing is preserved at the end of the comic archive without misaligning subsequent page turns.
* **0-Latency Preloading Optimization & De-duplication**:
  * Added `pending_loads` set to track in-flight background decoding tasks and prevent duplicate parallel decodes of the same page during rapid page turns.
  * Replaced slice copy with zero-copy `glib::Bytes::from_owned(rgba.into_raw())` for background `gdk::MemoryTexture` creation.
* **Lanczos3 Remaster Quality & Format Support**:
  * Upgraded JPEG encoder in `remaster_comic_cbz` to `JpegEncoder::new_with_quality(..., 92)` to eliminate compression artifacts on ink lines and screentones.
  * Added PNG fallback encoding for WebP/GIF scans when direct encoding is unsupported in the image codec, ensuring WebP comics are properly remastered instead of skipped.

---

## 4. Verification & Testing
* Unit tests in `src/comics.rs`:
  * Image filename filtering (`is_image_filename`).
  * Natural sorting (`sort_comic_pages`).
  * Lanczos3 CBZ upscaling (`test_remaster_comic_cbz`).
* Unit tests in `src/pages/comics_reader/mod.rs`:
  * `test_missing_pages_preloading_bounds`
  * `test_double_page_step_and_navigation` (including boundary pair alignment)
  * `test_pending_loads_deduplication`
  * `test_reading_direction_cycle`
  * `test_webtoon_mode_preloading_range`
* All unit tests pass cleanly (`cargo test`).

