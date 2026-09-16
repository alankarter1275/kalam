# Kalam Engine Reader Parity & Revamp Walkthrough

## Completed Enhancements

### 1. Image Lightbox Zoom & Controls
- **Blurred Backdrop & Glassmorphism Toolbar**: Implemented a floating glassmorphism control toolbar positioned top-center when an image is tapped.
- **Controls**:
  - `[ + ]`: Zoom In (up to 5x)
  - `[ - ]`: Zoom Out (down to 0.2x)
  - `[ 1:1 ]`: Reset Zoom & Rotation
  - `[ ↺ ]`: Rotate Left (90° counter-clockwise)
  - `[ ↻ ]`: Rotate Right (90° clockwise)
  - `[ ✕ ]`: Close Lightbox (or press Esc)

### 2. GTK UI Revamps
- **Dictionary Popover**: Built strictly to `docs/files/kalam_dictionary_popup_v3.html` specification (380px width, sticky header, Fraunces serif word title, IBM Plex Mono pronunciation, part-of-speech italic pill, numbered definitions with LIKELY HERE badge, italic examples, synonyms/antonyms chips, and idioms cards).
- **Selection Actions Chip**: Redesigned GTK selection popover toolbar with sleek pill design, 1px dividers (`.kalam-reader-chip-sep`), aligned Cairo vector icons, and 5 color swatches (Yellow, Green, Blue, Pink, Orange).
- **Selection Hover & Handles**: Refined interaction and positioning relative to text bounds.

### 3. Chrome Auto-Hide Timer & Edge Hover Zones
- **Inactivity Timer**: Top back button dock and bottom progress pill automatically hide after 3 seconds of mouse inactivity (or during continuous scroll).
- **Edge Hover Revealer**: Moving mouse near top (top 32px edge) or bottom (bottom 32px edge) reveals the respective back button or progress pill.

---

## Verification Results
- `cargo check --workspace` in `kalam-engine` — **PASSED** (Exit 0)
- `cargo check` in `calibre-alt` — **PASSED** (Exit 0)
- Code committed & pushed to `main` (`f304c0b`).

