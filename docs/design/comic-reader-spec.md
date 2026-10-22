# Comic reader and series page: design spec (draft)

Source boards: https://claude.ai/artifact/STQiE5abQevijypxoRx2S3 (Terracotta, light and dark). Tokens are from `design-system.md`. Reader chrome is inherited from `epub-reader-spec.md`.

## 1. Comic reader

| Surface | Position | Summoned by |
|---|---|---|
| Back pill + Bubble peek | top-left, 16 inset, 44 high | cursor rests in the top-left zone |
| Bookmark ribbon | top-right, 48 from edge | always visible; `B` toggles |
| Location text | bottom-right, 24 / 16 | always visible ("12 / 180") |
| Left panel (Chapters, Pages) | left, 16 inset, 340 wide | edge zone or shortcut |
| Right panel (Reading, Image, Shortcuts) | right, 16 inset, 340 wide | edge zone or `S` |
| Zoom readout | bottom centre, 32 high pill | while zooming only |
| Page-jump field | top centre | shortcut (not drawn) |

**Page modes:** single, spread, long strip. Long strip shows a continuous column with an optional gap between pages. Direction LTR or RTL (RTL swaps next/previous and spread order). Fit: width, height, screen, original.

**Zoom and pan:** scroll to zoom, drag to pan, double-click toggles fit and 100%.

## 2. Panels

**Panel shell:** `card` fill, 1px `border`, radius 22, padding 20 (16 for the right panel). Tabs: a pill container (`bg` fill, 1px `border`, padding 4) with 36-high tabs; the active tab is `accent` fill with white text; inactive `text-2`.

**Pages tab:** sticky heading (Label style + 11px meta, 40 high, `card` fill); grid below with 3 columns, gap 12, thumbnails 112 high, radius 6, page number over the bottom. States: loaded, skeleton (`border` fill), failed (1.5px dashed `text-2`, "Failed to load"), current (2px `accent` outline, 2 offset). Bottom fade 72 high (linear gradient to `card`). Opens scrolled to the current page.

**Chapters tab:** header (cover 64 x 96 radius 6, title Playfair 18, author, counts), Label-style volume headings, rows 44 high. Current: 3px `accent` bar, bold. Read: 55% opacity. Missing: 55% opacity, "Missing".

**Reading pane:** inset cards (`bg` fill, 1px `border`, radius 14, rows 48). Icon tiles 68 high, radius 10, 1px `border`; selected: `accent` border, bold label. Switches 40 x 24. Slider: 4px track, 18px knob, `accent`. Segmented control: 28-high options; selected `accent` fill.

**Image pane:** colour dots (24, selected has a 2px `accent` ring), three sliders, two switches, Crop switch, ghost pill "Reset image settings".

## 3. Series page (full page)

Canvas 1280 x 800 in the mockups. No nav rail drawn.

- **Header band:** 340 high, plain gradient from `accent` at 12 to 16% to transparent.
- **Cover:** 200 x 300, radius 10, left 56, top 56.
- **Title block:** left 288, top 64. Kind label, title (Display 46, Playfair 500), original title (Playfair italic, `text-2`), credits (Label-style role names + names), facts line (Meta), genre chips (28 high, pill, 1px `border`), actions row (44 high pills; Continue is `accent` fill).
- **Left column** (left 56, top 384, 360 wide): summary (Meta, line height 1.6, 4 lines, "More" in `accent`), details grid (120 label column, 13px), collapsed rows (Credits, Tags) with hairline dividers, source footer (11px).
- **Right column** (left 468, top 384, 756 wide): toolbar (underlined search 240 wide, chips Newest first / Unread only, List/Grid segmented control), then chapters.
- **List rows:** 64 high, hairline top border, thumbnail 36 x 50 radius 4, chapter number (Meta), title (600 if unread, 700 if current), meta line, right-hand state (Read / progress bar 100 x 3 / Locate file). Current: 3px `accent` bar 12 left of the row.
- **Grid:** 5 columns, gap 16, thumbnails 200 high radius 6, number and meta below. Fade at the bottom edge.

## 4. States

| State | Where | Treatment |
|---|---|---|
| Page loading | reader | skeleton at page ratio |
| Page failed | reader | dashed card, "Page could not be read", Retry |
| File not found | reader and series row | dashed card or dashed thumbnail, "Locate file" |
| Archive empty or corrupt | reader | dashed card, "Remove from library" |
| Files only | series page | title, "Find metadata", format/pages/size only |
| Metadata fetching, failed, no match | series page | **designed in the metadata chat** |

## 5. GTK widgets and custom drawing

- **Reader canvas:** `Gtk.Overlay` over a picture surface; panels and Back pill as overlay children with margins. Slides use `Gtk.Revealer`.
- **Page grid:** `Gtk.GridView` with lazy thumbnails; skeleton and failed as widget variants. Sticky chapter heading: an overlay label updated from the first visible row.
- **Chapter lists:** `Gtk.ListView`; series grid view: `Gtk.GridView`.
- **Tabs and segmented controls:** linked toggle buttons. Switches: `Gtk.Switch`. Sliders: `Gtk.Scale`.
- **Image adjustments:** applied at draw time (Cairo or a colour matrix in a GL shader). Crop blank borders needs an analysis pass per page, cached.
- **Series header band:** a widget with a CSS linear gradient. No blur.
- **Painted, not CSS:** the zoomed and panned page surface, the long-strip column, and the progress bars if CSS can't draw them.
- No conic gradients, `mask-image` or `backdrop-filter` anywhere.

## 6. Additions (second pass)

- **Two-page spread:** two pages side by side, vertically centred, no gutter chrome; location text shows "12–13 / 180"; RTL swaps the order.
- **Shortcuts pane:** inset groups (Navigation, View, Panels), rows 48 high, keys as 24-high chips (radius 6). A row being recorded gets a 2px `accent` outline and an `accent` "Press a key…" chip. A conflict shows an `accent` 12px message under the group and the key is refused (no silent swap).
- **Page-jump field:** `Gtk.Popover`-style bar, top centre, 52 high, 480 wide: label, input (underlined in `accent`), "of 180", Page/Chapter switch, close. Out of range: input and hint in `accent` ("Pages 1–180").
- **Credits and Tags (series page):** collapsible rows. Credits is a 2-column grid (role in `text-2`, name in `accent` as a link that opens the person's Author page when they have one, otherwise plain text; decision 116). Tags are chips, with "+N more" at the end.
- **Related series:** a "Related series" heading below the chapters, then 128 x 192 cards with a relation chip (Sequel, Prequel, Side story, Spin-off, Adaptation) and title/year below. A series not in the library is a dashed card marked "Not in library".
- **Control states:** see `design-system.md` section 12; the board shows every control in idle, hover, focus, selected and disabled.
- **White text** sits on `accent-fill` only (decision 49).
