# Kalam design system (v0.10)

Status key: **Final** = approved. **Proposed** = my suggestion, not yet confirmed. **TBD** = undecided.

Kalam is a GTK4 / Relm4 desktop app. Every rule below must be buildable in GTK4 CSS. Colour tokens map to `theme.rs`; shape tokens (spacing, radii, type) map to `style.rs`.

## 1. Character

Calm, bookish, warm. Playfair Display for headlines and quotes, DM Sans for the interface. Generous whitespace, quiet borders, one accent per palette. The UI should recede behind the book.

## 2. Colour tokens (Final palettes: Terracotta, Forest. Proposed: Plum, Teal)

Five tokens per palette and mode: `bg`, `text`, `text-2`, `text-3`, `accent`.

| Palette | Mode | bg | text | text-2 | text-3 | accent |
|---|---|---|---|---|---|---|
| Terracotta | Light | F7F3EC | 1C1917 | 726B66 | A8A29E | C1440E |
| Terracotta | Dark | 1A1512 | E8DDD5 | B3A8A0 | 807670 | D4552A |
| Forest | Light | F3F6F3 | 191E1A | 627165 | 9CA89E | 3A7D44 |
| Forest | Dark | 121A13 | DCE8DC | 9CB0A0 | 6B7A6E | 5A9E64 |
| Plum | Light | F6F2F4 | 1E1720 | 776875 | A89BA6 | 7B2D5B |
| Plum | Dark | 181017 | E8DCE6 | B3A3B0 | 7D6E7B | B8609A |
| Teal | Light | F1F5F5 | 171E1E | 5F7171 | 9AAAAA | 1B6F70 |
| Teal | Dark | 0F1819 | D6E6E6 | 98B0B0 | 678080 | 3FA6A6 |

Ink and Amber from the first mockups are dropped. Plum and Teal are used in the reader's theme picker and are treated as confirmed for the reader.

### Usage rules
- `text`: headings and body.
- `text-2`: secondary text and metadata. It meets 4.5:1 on its own background in every palette and mode.
- `text-3`: **decoration and placeholders only** (about 2.3:1 on light backgrounds). Never use it for information the user needs. The reader does not use it at all.
- `accent`: active nav item, primary buttons, links, progress, the quote rule, selection handles, the set bookmark. One accent per screen area, never two competing.
- Text on `accent` fills is white and must sit on `accent-fill` (section 11), never on plain `accent` in the dark modes.

### Derived tokens (Proposed, not final)
- `border`: `text` at 6 to 10% alpha. Never a hardcoded black. Panels and popups use 10%.
- `card` / `surface`: light = white or near-white, dark = `bg` lifted slightly (Terracotta dark uses `#241D19`). The other palettes still need their values.
- `overlay`: `bg` at 85% alpha, for sheets and popovers.
- Hardcoded `#fff`, `rgba(0,0,0,x)` and `rgba(193,68,14,x)` in the old mockups must all become tokens, or they break in every palette except one.

## 3. Type (Proposed)

| Role | Font | Size | Notes |
|---|---|---|---|
| Display | Playfair Display 500 | 46 | line-height 1.05, letter-spacing -0.7px; page titles |
| Section | Playfair Display 500 | 18 | section headings, dictionary headword |
| Card title | Playfair Display 600 | 16 | shelf and book titles, saved-word headwords, idiom phrases (14) |
| Quote | Playfair Display italic | 14 | line-height 1.78; highlight excerpts, dictionary examples |
| Body | DM Sans 400 | 14 | default UI text |
| Meta | DM Sans 400 | 12.5 | counts, dates, bylines |
| Caption | DM Sans 400 | 11 | smallest readable text |
| Label | DM Sans 700 | 10 | uppercase, 0.12em tracking |
| Reader body | Playfair Display 400 | 18 (user adjustable) | line-height 1.75 to 1.8 (default to confirm); the book text only |
| Quote large | Playfair Display italic | 18 | line-height 1.6; the Home quote only (Proposed, section 63) |

Fonts must be bundled with the app, not loaded from Google Fonts.

## 4. Shape (Proposed)

- Radii: 6, 10, 14, 22, pill (999). Cards 14, large feature cards and side panels 22, buttons and chips pill, fields 10.
- Spacing: 4, 8, 12, 16, 24, 32, 48 (20 is also used for padding inside side panels). The old mockups used 36, 40 and 44 for section gaps; use 32 or 48.
- Nav rail: 68 wide, 44 circular items, active item filled with `accent-fill` and its icon drawn in `bg` (section 56).
- Touch and click targets: 44. Compact controls inside cards may be 32 to 36.

## 5. Logo (Final)

Calligraphic K with an infinity-style loop. Stored as SVG text in `logo-svg.md`: one path filled with `currentColor`, so it takes the accent colour. No CSS mask needed. The loop is a hairline, so check it at 32px.

## 6. Components designed so far

Nav item (active and inactive), primary and ghost pill buttons, search field, shelf card, quote block. All are on the Components artboard, in Terracotta light and dark only.

From the EPUB reader (Terracotta dark, some light): floating side panel, inset settings card, pill tabs, segmented control, filter chip, switch, stepper, circular save button (idle and saved), dictionary popup, selection chip, annotation popover, search bar, bookmark ribbon.

From the Author page (chat 6): facts panel, series card with cover fan, open series panel, fetch button, highlight card and its sliding row, quiet notice line, Create shelf dialog (sections 25 to 31).

From the deep editor (chat 7): workspace layout, flat tabs and ghost buttons, file tree, editor tabs, source editor, diff tints, problem rows, command palette, option rows, after-tool pill, picker row (sections 32 to 40).

From Settings (chat 8): settings page layout, palette and mode picker, dropdown field and list, rows and chips, rail bell and notification panel, control states (sections 41 to 46).

From Library browsing and My Library (chat 10): library header and toolbar, cover grid and card markers, bulk action bar, list view and columns, search results and syntax help, reading timeline and its card, saved-words sphere, My Library page layout, library control states (sections 47 to 55).

From the app shell and Home (chat 9): navigation rail, search bar and box, search pills and tag marks, Home book, Updates timeline, shelf and timeline ruler, window controls, Home state cards (sections 56 to 66).

## 7. Not yet designed (TBD)

- **Progress ring (also the timeline and the saved-words sphere, sections 51 and 52, which are Cairo-drawn too):** the mockups use `conic-gradient`. GTK CSS has no conic gradients, so build it as a Cairo-drawn widget.
- **Feature surface:** resolved in chat 9. Home has no feature card. Continue reading is the open book (section 60), made of `card`, `border` and cover art. The navy card is dropped.
- **Status and source colours:** green for done (`#10B981` and `#4CAF50` both appear), plus per-source bar colours (MangaDex, AO3, FFN). Pick one set.
- **Monospace font:** the deep editor's source view needs one and none is defined. Proposed: bundle DM Mono at 13 / 22 (section 34).
- **Icons:** the mockups use Material Symbols Rounded (a web font). The app needs symbolic SVG icons; choose the set. The reader boards use simple 1.8px stroke icons as placeholders. Chat 9 drew Lucide-style outline icons (22, stroke 1.8) as the working proposal; the set is still to be chosen and bundled.
- **Cover fallback:** resolved in section 20. **Empty, loading and error states** outside the readers and the book and series pages are still open. Hover, focus and disabled states follow sections 12 and 13 and are drawn per screen.

## 8. GTK constraints for every design

- No conic gradients, `mask-image`, `backdrop-filter`, or blurred glows. Use plain colours, linear or radial gradients, or a drawn widget.
- Keep animations simple (opacity, small transforms). Avoid `transition: all`. For slide animations use `Gtk.Revealer`.
- The app should work at the window sizes a desktop GTK app really gets, including narrow ones.
- Every interactive element needs a visible focus ring and keyboard access.

## 9. Reader additions (new in v0.2, Proposed)

**Selection and search tints** (computed from the palette):

| Token | Dark | Light |
|---|---|---|
| `selection` | `text` at 16% | `text` at 14% |
| `match` (other search hits) | `text` at 10% | `text` at 8% |
| `match-current` | `accent` at 38%, plus 1px `accent` outline | same |

Selection handles: `accent`.

**Highlight colours** (same in every palette): yellow `#E8B93A`, green `#6FB36F`, blue `#5B9BD5`, pink `#D97BA8`, orange `#E08A4B`. Solid highlights use them at about 30% alpha; underline, squiggly and strikeout use them at full strength. Check each against all eight palette modes.

**Inset card** (recessed section inside a side panel): fill = the page `bg`, 1px `border`, radius 14, rows 48 high with hairline dividers.

**Dotted underline** for saved words: 1.5px dotted, `accent` at about 75% alpha, offset about 5.

**Side panels:** 340 wide, 16 from the window edge, radius 22, `card` fill, 1px `border`, no shadow, no dim layer.

**Motion:** 220 ms slide ease-out for reveal; 180 to 220 ms ease-in for hide; 300 ms grace before hiding; 250 ms hover delay before revealing; fade only when reduced motion is on.

---

# Additions v0.3 (comic reader chat)

All Proposed unless stated. Apply to Terracotta first; check the other palettes.

## 10. Comic reader and series page additions

**Tabs (changes section 6):** the active tab in a side-panel tab pill is `accent` fill with white text; inactive is `text-2`. The deep editor uses the flat underlined tab instead (section 33). Container: `bg` fill, 1px `border`, padding 4, tabs 36 high. The EPUB panel tabs should follow.

**Icon tile:** 68 high, radius 10, 1px `border`, `bg` fill, 22px icon above an 11px label. Selected: `accent` border, `text` colour, bold label.

**Switch:** 40 x 24, radius 999. Off: `border` fill, knob `text`. On: `accent` fill, white knob. Knob 18, inset 3.

**Slider:** 4px track in `border`, filled part and 18px knob in `accent`.

**Chip:** 28 high pill, padding 12, 1px `border`, 12.5px `text-2`. Active chip: `text` colour, `accent` border. Search-bar chips are 44 high.

**Progress bar:** 3px, radius 2, track `border`, fill `accent`.

**Header band (series page):** 340 high, linear gradient from `accent` at 12% (light) or 16% (dark) to transparent. No blur. A tint derived from the cover colour would be a new computed token (TBD).

**State card:** 1.5px dashed `text-2` border, radius 6, centred Card-title heading and a Meta line, optional ghost pill action.

**Thumbnails:** radius 6 (4 in 36 x 50 list thumbnails). Skeleton fill: `border`. Failed: dashed border. Current: 2px `accent` outline, 2 offset.

**Row dividers:** 1px `border`. Chapter rows on the series page have no surrounding card.

**Comic art placeholders** (page and cover stand-ins) are neutral and the same in every palette: they represent content, not UI.

## Updates to existing sections
- Section 6: add the comic reader panels, series page header, chapter list and grid, and the components above.
- Section 2 (Usage rules): replace the open contrast note with the `accent-fill` token in section 11.
- Section 7, Cover fallback: still undesigned; the series page and Chapters tab use a plain gradient block.

## 11. `accent-fill` token (resolves the contrast question)

Text on a filled accent surface is white and must reach 4.5:1. Use `accent-fill` for those fills; keep `accent` for text, links, rings, bars and outlines. Computed values (white on each):

| Palette | Mode | accent | white on accent | accent-fill | white on fill |
|---|---|---|---|---|---|
| Terracotta | Light | C1440E | 5.1:1 | C1440E | 5.1:1 |
| Terracotta | Dark | D4552A | 4.1:1 | C54F27 | 4.6:1 |
| Forest | Light | 3A7D44 | 5.0:1 | 3A7D44 | 5.0:1 |
| Forest | Dark | 5A9E64 | 3.2:1 | 498051 | 4.7:1 |
| Plum | Light | 7B2D5B | 8.9:1 | 7B2D5B | 8.9:1 |
| Plum | Dark | B8609A | 4.1:1 | AB598F | 4.6:1 |
| Teal | Light | 1B6F70 | 5.9:1 | 1B6F70 | 5.9:1 |
| Teal | Dark | 3FA6A6 | 2.9:1 | 318080 | 4.6:1 |

Only Terracotta is applied in the boards. The other palettes' values are computed here and not yet drawn.

## 12. Control states (new)
Hover: `border` colour as fill (primary: 10% white overlay on `accent-fill`). Focus: 2px `accent` ring, 2px offset. Disabled: 40% opacity. Switch hover: 5px `border` halo. Thumbnail hover: 2px `text-2` ring. Drawn for the Settings controls in section 46 and the library controls in section 55.

---

# Additions v0.4 (PDF reader chat)

All Proposed unless stated. Applied to Terracotta only.

## 13. PDF reader additions

**Page paper.** A PDF page stays pale in every mode. Dark mode Tint dims the paper toward the palette. Terracotta dark: `#CFC6BB`. Light: `#FFFFFF`. The other palettes' paper values are TBD (derive a light neutral from each palette hue). Page placeholder ink: `#2A2420`.

**Selection tint on paper (changes section 9).** The tint is computed from the page ink, not from `text`: dark mode ink at 18%, light mode `text` at 14%. `text` at 16% is invisible on pale paper in dark mode. Handles stay `accent`.

**Ink colour.** `#2A2420`, a sixth annotation colour beside the five highlight colours. Same in every palette.

**Overlay.** `overlay` (page `bg` at 85%) is now used behind dialogs.

**Dialog (new).** 420 wide, padding 24, radius 14, `card` fill, 1px `border`, no shadow. Title Section style (Playfair 500, 18), body 13.5 in `text-2`, buttons 40 high pills: ghost for the safe choice, `accent-fill` for the main one.

**Floating annotation toolbar (new).** 48 high pill, `card` fill, 1px `border`, padding 6, 36 circular buttons, 4 gap. Active tool: `accent-fill` with a white icon. A 3px corner arrow marks a tool with a menu.

**Tool popover (new).** `card` fill, 1px `border`, radius 14, padding 12, 12 gap. Contents: colour row (22 dots, 10 gap, selected = 2px `accent` ring, 2 offset), thickness tiles (36, radius 10, selected = `accent` border), segmented controls, switch rows.

**Icon button.** 32 circle (36 in tool popovers and the toolbar), transparent, icon `text`. Hover: `border` fill. Selected: `accent-fill` with a white icon.

**Key chip.** 24 high, radius 6, 1px `border`, 12 `text`. Recording: `accent` border and text.

**Selection chip (changes decision 26 wording).** Icon-only: five 20 dots, a divider, Note and Define as 32 icon buttons. Pill, 44 high.

**Pin button (new).** 36 circle in a panel header row, beside the tabs. Unpinned: 1px `border`, icon `text`. Pinned: `accent-fill` with a white icon.

**State card.** Unchanged from section 10. Used for the PDF errors.

**Drag and drop.** Drop bar: 3px, radius 2, `accent`. Dragged items: originals at 40% opacity, a ghost with a count badge (`accent-fill`, white, 22 high).

**Edit selection.** Selected thumbnail: 2px `accent` outline with 2 offset and an 18 tick badge in `accent-fill`.

## Updates to existing sections
- Section 6: add the components above to "Components designed so far".
- Section 7: Cover fallback, Feature surface, Progress ring and Icons are still open. The PDF boards use placeholder stroke icons: chevron, search, close, list, up and down arrows, rotate left and right, trash, plus, download, pencil, pen, highlighter, eraser, shapes, text, note, lock, eye, file with warning, file with question, image, info, tick, speaker, book, ribbon, grid (2 and 3 columns), split view.
- Section 12 (control states): drawn for primary button, ghost button, icon button, fit tile, filter chip and switch.
- Section 9: replace the selection tint rule with the "on paper" rule above for PDF pages.

## Open (design system)
- Paper values for Forest, Plum and Teal.
- Whether the ink colour should be a token or stay a constant.
- Dialog and toolbar need hover, focus and disabled states in the boards.

---

# Additions v0.5 (book details and series page chat)

All Proposed unless stated; applied to Terracotta only (Forest, Plum and Teal not checked). No new colour tokens were added.

## 14. Outlined section
A section with a boundary but no fill: 1px `border`, radius 14, padding 20, `bg` fill (the page colour; it does not change). Used for Contents and Your impression on the book details page, and for the Author facts panel (section 25). The Contents overlay keeps the same look and grows to a fixed height.

## 15. Timeline row
Row 64 high, left padding 44. Node 24 circle at left 0; connecting line 2px at left 11 (upper segment 0 to 20, lower segment 44 to 64).
- Read: `accent-fill` circle, white 14 tick. Current: 2px `accent` ring, `bg` fill, accent eye icon. Upcoming: 1.5px `text-2` ring at 50% opacity.
- Line: `accent` between read and current nodes, `border` colour after.
- Right side: Meta date; the current row shows the percentage in `accent` (500).
- **Bookmark indicator:** a 14 bookmark icon in `accent`; "×2" or "×3" (12, 500) beside it when there is more than one. Not clickable.

## 16. Rating
Five stars, 20px, letter spacing 3, half steps. Set: `accent`; empty: `border` colour. The half star is painted (a custom-drawn widget in GTK). Hover previews the value at 60% opacity; clicking the current value clears it; keyboard arrows step by half. Empty label "Not rated yet" in Meta.

## 17. Menu and submenu (popover)
`card` fill, 1px `border`, radius 14, padding 6, no arrow. Items 40 high, radius 8, 12 side padding. Dividers: 1px `border`, margins 6 and 8. Widths: 210 (short menus), 240 (the ⋯ menus on the book and Author pages, and the Shelves submenu), 220 (dropdown lists, section 43), and for the library 260 (Filter menu and its submenus), 280 (shelves menu), 300 (Add books menu and Columns menu). Destructive item: `accent` text. A trailing chevron marks a submenu; the open parent item has the `border` fill. **Count badge:** a 22 high chip with an accent border and accent text. Tick circles in a checklist: 22 circle, `accent-fill` with a white tick when on, 1.5px `text-2` ring when off.

## 18. Dialog sizes (extends decision 75 and section 13)
420 wide for forms and confirmations. **560 wide** for list dialogs (the Annotations dialog; 660 high, padding 24). Everything else about dialogs is unchanged.

## 19. Segmented progress
One 3px high segment (radius 2) per item, 4 gap, 220 wide in total. Read `accent`; in progress `accent` to a percentage then `border`; unread `border`; missing dashed `text-2`. Used for series progress and, with one segment per work, on the Author series cards (width 220) and in the Author facts panel (full panel width; sections 25 and 26). (The single 3px progress bar from section 10 is unchanged.)

## 20. Cover states
- **Fallback (resolves the "Cover fallback" TBD):** `card` fill, 1px `border`, radius 10, padding 20, content at the bottom: Label-style kind, title (Playfair 500, 22), author (Meta, `text-2`). **No accent tint**, so it can sit anywhere.
- **Missing book (series):** 1.5px dashed `text-2` border, radius 10, centred title and "Not in library" (11px). On the Author page it also carries the fetch button (section 28).
- **Finished badge:** 22 circle, `accent-fill`, white 14 tick, 8 from the top right. (Library cards use the double tick of section 48 instead.)
- **File missing (book page):** cover at 45% opacity, a dashed "File not found" state card under it.

## 21. Series strip
Covers are left-aligned, bottom-aligned, gap 28. Focused 260 x 390; others 180 x 270; radius 10. No shadow, no glow, no underline. Title Playfair 14 (focused 700); year in Meta.

## 22. Small segmented switch
Container: `bg` fill, 1px `border`, padding 4, radius 999. Options 28 high, padding 0 16, 12.5px. Selected: `accent-fill`, white text (500). (The 36 high tab pill in section 10 is unchanged and is used for panel and dialog tabs.)

## 23. Slide-out pill (generalises decision 15)
A secondary pill that slides out from behind a primary one on hover or focus. The primary takes the `card` fill; the pill takes `bg` with a 1px `border` and sits behind it (margin-left -24, padding-left 38, 44 high). Controls to its right **slide aside**, they are not covered. Timings as decision 15 (hover delay, 200 ms ease-out slide, 300 ms grace). Disabled: opaque fill, text at 40%.

## 24. Scrolling (new rule, decision 90)
**No scrollbars are visible anywhere.** Use the external scrollbar policy on every scrolled widget. A 48 high gradient fade to the surface colour at the bottom of a scrolled list hints that it continues.

## Updates to existing sections
- **Section 6 (components designed so far):** add outlined section, timeline row, rating, menu and submenu, 560 dialog, segmented progress, cover states, series strip, small segmented switch, slide-out pill.
- **Section 7 (TBD):** "Cover fallback" is **resolved** by section 20. Icons are still placeholder strokes: add eye, bookmark, tick, pencil, ⋯, plus, close, chevrons (left, right, down), star.
- **Section 10, header band:** the accent band is **not used** on the book details or series pages (decision 77). Whether the comics series page keeps it is open.
- **Section 12 (control states):** now also drawn for the Finished toggle, chip, tab, tag row, star rating, menu item, chapter row, strip cover, arrow button, small segmented switch, list row and the Continue button.
- **Section 8:** add "no visible scrollbars" (section 24).

## Open (design system)
- Whether the accent band stays on the comics series page.
- Forest, Plum and Teal versions of the new boards. In particular check the eye and tick icons on `accent-fill` in each palette.
- A light version of the menus board (cover popover, ⋯ menu, Shelves submenu, Next-in-series states).
- Hover, focus and disabled states for the Annotations and Tag dialogs.


---

# Additions v0.6 (author page chat)

All Proposed unless stated; applied to Terracotta only (Forest, Plum and Teal not checked). No new colour tokens. New sizes: 420 highlight card width, 72 x 108 highlight cover, 56 x 84 fan cover (radius 4, as list thumbnails), 124 series card height, 64 portrait.

## 25. Author facts panel
An outlined section (section 14): 340 wide, 1px `border`, radius 14, padding 20, `bg` fill. Top row: portrait, a 64 circle (`card` fill, 1px `border`, Playfair 500 18 initials when there is no image; no tint), 16 gap, and the lifespan beside it (a Label-style "Lifespan" key in `text-2` over Playfair 500 18 text). With no lifespan the row reads "No details yet" (Meta). Below, rows each separated by a 1px `border` hairline with 14 spacing above and below: key (Label style, `text-2`) over value (14, `text`). The **You've read** row adds a segmented bar (section 19) at full panel width: read `accent`, unread `border`. The Series row is left out when there are no series.

## 26. Series card and cover fan
Card: `card` fill, 1px `border`, radius 14, padding 20, 124 high, 24 between info, fan and chevron; two columns, gap 16. Info: title (Card title, Playfair 600 16), 10 below it a segmented bar 220 wide (section 19), 8 below that a Meta line. Fan: 100 x 84; three covers 56 x 84, radius 4, 1px `border`, left offset 0, 22 and 44, the first on top, **no rotation** (GTK cannot rotate widgets in CSS). A missing cover in the fan is `bg` fill with a 1.5px dashed `text-2` border. Chevron: 20, `text-2` (down when closed, up when open; two icons). Hover: `border` as fill. Focus: 2px `accent` ring, 2 offset. Open: bottom border transparent, bottom radii 0, so it joins the panel (section 27).

## 27. Open series panel
Directly under its card, over the content below (nothing moves): `bg` fill, 1px `border` with no top border, radius 0 0 14 14, padding 20. Header row, 16 below: "Series · N books" (Meta) and an accent "Open series ›" link (12.5px). Covers: the section 21 unfocused cover (180 x 270, radius 10), gap 20, in a row that scrolls sideways with no visible scrollbar (section 24); 4 of padding around the row so the focus ring (2px, 2 offset) is not clipped. Owned cover: title (Playfair 600 14) and year (Meta), the finished badge (section 20) or a 3px progress bar. Missing cover: section 20, plus the fetch button (section 28).

## 28. Fetch button
A 36 circle icon button: transparent, 1px `border`, plus icon 18 in `text`. Inside the dashed missing cover, centred, 8 below the "Not in library" line. Hover: `border` fill. Focus: 2px `accent` ring, 2 offset. Disabled: 40% opacity, with a tooltip, until online sources exist (Part 2). Accessible label "Fetch <title>".

## 29. Highlight card and sliding row
Card: 420 wide, `bg` fill, 1px `border`, radius 14. Header: padding 16 20, a 1px `border` hairline below. It holds the book title (Card title style), a 10 circle in the highlight colour (section 9) at the right, and a Meta line ("Chapter 4 · Saved 14 Mar 2026"). Body: padding 20, 16 gap, a 72 x 108 cover (radius 6) and the quote in Quote style (Playfair italic 14, line height 1.78) clamped to four lines. Row: cards 16 apart, bleeding off the right page edge (margin 56), scrolling sideways with no visible scrollbar. Heading: Section style, then an accent "See all N" link and two 36 circle arrow buttons (1px `border`, 12 gap); the left one is at 40% opacity at the start. The arrows scroll by one card.

## 30. Quiet notice line
A single line for a state that is not an error: 12.5px `text-2`, 1px `border` hairline above it, 16 padding-top, 32 above the hairline, with an accent link inside the sentence. Used for "No bibliography yet". (The dashed state card of section 10 is for blocking states.)

## 31. Create shelf dialog
A form dialog (sections 13 and 18): 420 wide, padding 24, radius 14, `card` fill, 1px `border`. Title (Section style) and a Meta line "for <author>". Fields: Label-style caption, then a 44 high field (radius 10, `bg` fill, 1px `border`; disabled = 40% opacity). A switch row: a 14 label over a Meta caption, with the section 10 switch (40 x 24) on the right. A Smart / Manual choice uses the small segmented switch (section 22), with a Meta explanation under it. Buttons, 28 below: ghost Cancel and `accent-fill` pill, 40 high, right-aligned, 12 gap.

## Updates to existing sections
- **Section 6:** add the components above (sections 25 to 31).
- **Section 7:** the placeholder icons also include plus, chevron up and down, and close. Cover fallback and Feature surface are unchanged.
- **Section 14, 17, 19 and 20:** extended in place (facts panel, ⋯ menu width, Author progress bars, fetch button on missing works).
- **Section 9:** the highlight colours also appear as the dot on highlight cards (section 29).

## Open (design system)
- Forest, Plum and Teal versions of the Author boards.
- Whether the 420 highlight card width should become a shared size or stay specific to the Author page.
- The fetch button's fetching, progress and failed states (Part 2).

---

# Additions v0.7 (deep editor chat)

All Proposed unless stated; applied to Terracotta only (Forest, Plum and Teal not checked). New colour token: `diff-add` (section 35). New sizes: 36 tree row, 48 tab row, 56 top bar, 32 status bar, 440 right panel, 240 file tree, 56 problem row, 72 picker row, 52 option row, 40 palette row. A monospace font is proposed (section 34).

## 32. Editor workspace
A full page on plain `bg`. Top bar 56 high with a 1px `border` line below: Back pill (`card` fill, 1px `border`, as elsewhere), title (Playfair 600 16) over a Caption line in `text-2`, then flat text buttons and one filled Save (`accent-fill`, white, 36 high, padding 0 24). File tree 240 wide, right panel 440 wide, each separated by a 1px `border` vertical line. Status bar 32 high, 12px `text-2`, the problem count in `accent`, the view switch at its right end. No panel has a radius, a fill or a shadow (they are docked, not floating).

## 33. Flat tabs and ghost buttons (editor)
**Tab or switch:** plain 13px text, `text-2`; active = `text`, weight 500, with a 2px `accent` underline. Gap 20. **Tab row (right panel):** 48 high, 16 side padding, a 1px `border` line below, the active underline sits on that line (buttons 44 high, margin-bottom -1). **Ghost button:** no border, no fill, `text-2`, 36 high, padding 0 12, hover `border` fill. **Boxed ghost button** (Back pill, and every button inside a dialog): 1px `border`, as section 6. Chips and fields keep their section 10 look.

## 34. Source editor
Monospace 13 / 22 (font proposed: DM Mono, bundled). Gutter 48 wide, numbers right-aligned in `text-2` at 70%. Syntax: tags `accent`, attribute names and text `text`, attribute values and comments `text-2`, comments italic. Current line: `border` as fill. Cursor: 2px `accent`. Formatting toolbar: a 48 high row above the tabs, 32 circle icon buttons (Bold, Italic, Link, Heading, List), Find & replace as a ghost button at the right. No scrollbars (section 24).

## 35. Diff tints
Removed line: the `match` tint (`text` at 10% dark, 8% light). Added line: **new token `diff-add`** = `accent` at 20%. Line numbers 32 wide in diffs. Check the tint against all eight palette modes.

## 36. File tree and editor tabs
Tree row 36 high (a new, denser size), 16 side padding, 13px. Section header: Label style with a chevron (down when open, right when closed) and a count when collapsed. Open file: `border` fill, weight 700, 3px `accent` bar at the left. Changed file: 8 `accent` dot after the name. Editor tab strip: 40 high, 13px; active tab weight 700; a changed tab has an `accent` dot; a **temporary tab** has an italic title in `text-2`; × shows on hover.

## 37. Problem row and fix preview
Row 56 high: marker (12 filled dot in `accent` = error, 12 hollow ring = warning), title (14) over file and line (Caption, `text-2`), a 28 high boxed ghost button (12px: Fix, Go to, Remove). Group headings in Label style. **Fix preview:** the row grows; a mini diff (section 35, 12px mono) and two 32 high buttons, ghost Skip and `accent-fill` Apply fix.

## 38. Command palette, option rows and tool dialogs
Palette: 640 wide, `card` fill, 1px `border`, radius 14; a 52 high input row with a hairline below, then 6 padding and 40 high rows (radius 8; matched letters bold; the first row has the `border` fill; shortcuts as 24 high key chips). **Option row** (scope questions, Split, Insert dialogs): 52 to 56 high, hairline above, a 22 tick circle (section 17) and a 14 label with an 11 Meta line. Dialogs are 420 wide, or 560 for lists (section 18); a 640 wide table card is used for Reports. Progress: the 3px bar of section 10.

## 39. After-tool pill
A message after an instant tool: 48 high pill, 380 wide, `card` fill, 1px `border`, 13px text, an `accent` "Undo" link at the right. Shown at the bottom centre above the status bar; it hides after a few seconds (fade with reduced motion).

## 40. Picker row
Row 72 high, hairline above, a 40 x 56 cover (radius 4; the cover fallback of section 20), title (Playfair 600 16) over the author (Meta), the state at the right: an `accent` outlined chip (12px, 1px `accent` border, padding 2 10) for edits waiting, otherwise Meta text. Search field: the underlined style of decision 45, 320 wide, 40 high.

## Updates to existing sections
- **Section 6:** extended in place with the deep editor components.
- **Section 7 (TBD):** the monospace font is added. Icons: the editor boards use placeholder text glyphs for bold, italic, link, heading, list, drag handle, close and arrows; a symbolic set is still to choose. Cover fallback, Feature surface and Progress ring are unchanged.
- **Section 10:** a note points to the flat tab (section 33).
- **Section 12 (control states):** also drawn for the primary button (idle, hover, focus, disabled) on the editor States board.
- **Section 17 (menu):** the Tools menu uses it with four submenus, one open at a time; the open parent item has the `border` fill, and the submenu is 260 wide.

## Open (design system)
- Which monospace font to bundle.
- Whether the flat tab and borderless ghost button should spread beyond the editor.
- The `diff-add` value for the other palettes.
- Forest, Plum and Teal versions of the editor boards.
- The 36 high tree row and 48 high tab row are new sizes.

---

# Additions v0.8 (settings chat)

All Proposed unless stated; applied to Terracotta only (Forest, Plum and Teal not checked). No new colour tokens. New sizes: 640 settings column, 240 pane list, 34 pane list row, 220 dropdown list, 36 dropdown field (the field reuses the 36 high control size), 18 selection tick badge on cards (as section 13).

## 41. Settings page
A full page on plain `bg`. Pane list: 240 wide with 56 left and top padding; Label-style group headings (20 above, 6 below); items 34 high, 13px, `text-2`, 12 left padding. The active item is `text` at weight 700 with a 3px `accent` bar (radius 2, 8 in from the top and bottom). Content column: 640 wide, 64 right of the list, 56 from the top. Title in Display, a Meta line 12 below it and 28 above the first heading. Each group is a Label heading and then an outlined section (section 14) with padding 0 20 and rows 48 high separated by hairlines; a row has its label (14) over an optional caption (Caption, `text-2`) at the left and its control at the right. Rows nested under a switch row (Google Books) keep the same hairlines and add 20 left padding; the parent row carries a chevron beside its switch (two icons, up and down). 24 between sections.

## 42. Palette and mode picker
An outlined box (radius 14, padding 16) per palette in a 2 x 2 grid, gap 16. Inside: the palette name in Label style, then two mini-page cards side by side (gap 12), each 64 high with radius 10, a 1px `border`, padding 10 and that palette's `bg` for the mode, three 4px lines (the palette `text` at 50% and 30%, then `accent` at 30% width), and a Caption "Light" or "Dark" in `text-2` below. Selected: a 2px `accent` outline with 2 offset and an 18 tick badge (`accent-fill`, white 11 tick) 7 outside the top-right corner. Hover: a 2px `text-2` ring (section 12). The mini-page card is a custom-drawn widget (Cairo), because it draws other palettes' colours.

## 43. Dropdown field and list
Field: 36 high, 1px `border`, radius 10, `bg` fill, padding 0 12, 13px `text`, a 12 chevron 12 after the text. Hover: `border` fill. Focus: 2px `accent` ring with 2 offset. Open: `accent` border. List: the popover of section 17 (no arrow), 220 wide, right-aligned to the field and 4 below it, rows 40 high with radius 8 and 12 side padding. The current choice has a 14 `accent` tick at the right. The hovered row has the `border` fill. A typeface option is set in its own font. A divider and an `accent` "New shelf…" row can close a list. Long lists scroll without a visible bar and end with a 48 high fade (section 24).

## 44. Rows, chips and key chips in Settings
- **Not available yet:** a 28 high chip (section 10), 11px `text-2`; the row's switch or button is at 40%.
- **Accent chip** ("Open", "Speaks first"): the same chip with an `accent` border and `accent` text.
- **Key row:** 40 high; label 14; key chips (section 13: 24 high, radius 6, 12px) at the right, 6 apart. Recording: an `accent` border and text, "Press a key…". A conflict message sits under the group: 12px `accent`, 24 below, naming the action that owns the key.
- **Dictionary row:** 56 high; a 16 drag handle in `text-2` with 12 gap, name (14) over a Meta count, the accent chip, a ghost Remove. Dragging: the original at 40% opacity and a 3px `accent` drop bar (section 4's drop bar, as sections 13 and 38).
- **Missing state:** dashed state cards (section 10) in Watch folders and Dictionaries; an `accent` Meta line ("Folder not found") in the Libraries row.
- **Error line under a field:** 12px `accent`, with an `accent` border on the field.

## 45. Rail bell and notification panel
Rail item: 44 circle, 22 icon, `text-2`; the panel-open item is `accent-fill` with a white icon. New-event dot: 8 `accent`, top-right of the item (not drawn). Panel: the side panel of section 9 (340 wide, radius 22, `card`, 1px `border`) at 16 from the rail, top and bottom, padding 20. Heading in Section style with an `accent` 12.5px link, 16 below. Group headings in Label style. Event row: top hairline, 12 vertical padding, 12 gap, 13px message over a Meta time, an 8 circle marker (`accent` problem, `text-2` otherwise) 6 down. Empty: a hairline and a 12.5px `text-2` line.

## 46. Control states (Settings)
Rules as section 12, drawn for the ghost button, primary button, dropdown field, switch (hover halo), key chip, palette card, pane list item and segmented option (idle, hover, focus, selected, disabled). Hover on the pane list item uses the `border` fill with radius 6. A selected palette card adds the tick badge of section 42 so selected and focused differ.

## Updates to existing sections
- **Section 6:** extended in place with the Settings components (sections 41 to 46).
- **Section 7 (TBD):** the placeholder icons also include home, book, editor, gear and bell in the rail, and the chevron up. Cover fallback, Feature surface and Progress ring are unchanged.
- **Section 12 and section 17:** extended in place (Settings controls; the dropdown list width).
- **Section 3 and the Text size setting (decision 146):** the type table values are the Default; Small and Large scale all of them together, interface only.

## Open (design system)
- Confirm the 640 settings column, the 220 dropdown list and the Text size percentages.
- Forest, Plum and Teal versions of the Settings boards.
- The reader's Theme card should use the section 42 picker.

---

# Additions v0.9 (library chat)

All Proposed unless stated; applied to Terracotta only (Forest, Plum and Teal not checked). No new colour tokens. New sizes: 1.6 : 1 cover ratio (height 1.6 times width), 132 x 211 grid cover, 16 grid gap, 18 format chip, 32 quick-action button, 20 double tick, 18 selection circle, 48 bulk bar, 608 recently added list, 380 x 124 shelf card, 400 x 196 timeline card, 520 x 480 sphere stage, 300 columns menu.

## 47. Library header and toolbar
Title row: Label "Library" (accent) over the Display title with a 24 chevron in `text-2` (the title is a button), and the filled **Add books** pill at the right (`accent-fill`, white text weight 500, 44 high, padding 0 24, 18 plus icon, 8 gap). Search: a 44 high pill, `card` fill, 1px `border`, padding 0 6 0 18, a 16 search icon, 10 gap, placeholder in `text-2`, and a 32 circle `?` icon button at the right. Toolbar row, 14 below: a **ghost Filter button** (32 high, pill, no border, 12.5px, hover `border` fill) with the count badge (section 17); filter chips (section 10, active chip, 28 high) with a ×; an accent "Clear filters" link; at the right a Meta count, two dropdown fields (section 43, 36 high), a ghost Select button and the small segmented switch (section 22). Popovers: the shelves menu (280 wide, rows with a Meta count, "Manage shelves…", accent "New shelf…"), the Add books menu (300 wide, key chips of section 13, a disabled row at 40% with a "Not available yet" chip). Narrow (about 640): Add books is a 44 circle, the toolbar wraps to two rows, and Group, Select and the view switch move into a 32 ⋯ button.

## 48. Cover grid and card
Grid: 8 columns at 1280, 12 at 1920, 4 at 640; gap 16; margin 56 (24 at narrow). Cover: ratio 1.6 : 1 (aspect 5 : 8), radius 10, placeholder gradients neutral and the same in every palette. **Format chip:** bottom-left, 6 in, 18 high, padding 0 6, radius 6, `card` fill, 11px bold `text`. **Quick action:** 32 circle, top-right 6 in, `card` fill, 1px `border`, ⋯ icon, shown on hover and focus. **Finished:** a 20 double tick, stroke 1.8, `accent`, no background, 8 from the bottom-right. **Progress:** the 3px bar of section 10, 10 below the cover. **Title:** Playfair 600 14, line height 1.25, two lines then ellipsis, 10 below; **author:** Meta, one line. **Selection mode:** an 18 circle at the top-left (8 in), 1.5px white ring and no fill; selected = `accent-fill` fill, white 12 tick, plus a 2px `accent` outline with 2 offset on the cover. **Hover:** 2px `text-2` ring; **focus:** 2px `accent` ring with 2 offset. **Skeleton card:** a cover and two bars (12 and 10 high, radius 6) in `border`. A 72 high gradient to `bg` fades the last row. **Group heading:** Label style `text-2` with a Meta count, 24 above and 12 below. **Jump strip:** a column of 26 letters, 11px, line height 17, `text-2`, the current letter bold `accent`, 20 from the right edge.

## 49. Bulk action bar
A floating pill, bottom centre, 32 up: 48 high, `card` fill, 1px `border`, padding 6 8 6 20, 8 gap. "N selected" (13px, count bold), an accent "Select all" (12.5px), ghost buttons (section 33 ghost, 32 high), the accent Remove, and a 32 circle close icon button. A button that needs a minimum selection is disabled at 40%.

## 50. List view and columns
Row 64 high, hairline above, columns by grid with 16 gap: 36 thumbnail (36 x 50, radius 4), flexible title, then fixed columns (Series 140, Format 64, Progress 120, Rating 70, Added 90, Last read 96, a 32 ⋯). Header: Label style captions in `text-2`, 8 below; the sorted caption is `accent` with an arrow. **Progress cell:** a 72 x 3 bar and the Meta percentage, or an accent double tick (18) and "Read", or "Unread". **Rating cell:** an accent star and the value in Meta. **Hover row:** `border` fill, radius 10. **Columns menu:** the menu of section 17, 300 wide, rows 40 high with a tick circle (22) and the field name, a locked row at 40% with "Always shown", a 48 high bottom fade, a divider, an accent "Reset to default" row and a Meta hint. **Drag:** the dragged column is tinted with `border`, radius 12, from the caption to the last row; the other columns slide aside; a 3px `accent` bar (radius 2) marks the slot. **Resize:** a 2px `accent` line (radius 2) down the list, a 14 x 26 `accent-fill` grip (radius 7) in the header, and a label pill (`card` fill, 1px `border`, radius 14, padding 6 12, 12px) above.

## 51. Reading timeline
A header line (Label caption at the left; at the right a Meta hint, an accent 12.5px "All impressions ›", an optional "N hidden · Show" and a ghost Expand or Collapse button with a 14 chevron). Axis row 30 high: month initials (11px `text-2`, 1px `border` tick at each month start, 6 padding) over a 1px `border` hairline. Lines: 3px high, radius 3, one of the five highlight colours (section 9) cycling, lanes 16 apart from 34 down, 8 minimum gap, opacity 1 (30% when another line is open). **Open line:** 5px; a 1.5px connector in its colour; the card (400 x 196, `bg` fill, 1px `border`, radius 14, padding 16, cover 48 x 72 radius 6, a 32 circle dustbin button with 1px `border` and `bg` fill, a hairline, the note in Quote style 13, a mood chip, an accent link bottom-right). **Today:** a 1.5px `accent` line with an 11px bold accent label in the axis row. **Current read:** a 6 `accent` dot at the line's end. **Edge fade:** 64 wide gradient from `bg`. **Expand:** lanes 64 high; under each line a title (Playfair 600 12.5) and a Meta 11px line. **Focus:** 2px `accent` ring with 2 offset (3 offset on a 3px line).

## 52. Saved-words sphere
A stage 520 x 480, clipped, on plain `bg`. Words: Playfair, upright, horizontal or rotated -90 degrees. Size 9.5 to about 38 from frequency, scaled 0.72 to 1 by depth; weight 400, 500, 600 or 700 by frequency bucket; opacity 0.2 at the back to 1 at the front; colour `text`. The hovered word is `accent` weight 700 and opens a 270 wide popover (section 17 look, `card` fill, 1px `border`, radius 14, padding 16): headword Playfair 600 16, an `accent-fill` 22 tick circle, Meta meaning, a hairline and a Meta "Seen N times in M books" line. The small accent link "All words ›" sits at the top right. Packing: front words are placed largest first on the sphere's visible side, centre outwards, each in the first free spot as horizontal or vertical with a 2 gap; back words are packed the same way among themselves and drawn faint. Build as a Cairo and Pango widget (section 8); drift stops with Reduce motion.

## 53. My Library page layout
Section heading row: 36 high, 48 above and 16 below, Section-style title at the left and at the right an accent 12.5px link plus, for sliding rows, two 36 circle arrow buttons (1px `border`, 12 gap; the left one at 40% at the start). Masthead: the Display title at the left; at the right three blocks of a Label caption over a Display-size Playfair numeral (46, 8 below, with an 18 `text-2` unit), 48 apart, and an accent "All analytics ›" 12 below. **Recently added list:** 608 wide, a fixed height equal to the sphere stage so both end on one line, 8 rows with a hairline above each (not the first) and one below the last, title Playfair 600 14 with ellipsis, the date in Meta. **Shelf card:** 380 x 124, `card` fill, 1px `border`, radius 14, padding 20, title Playfair 600 16, a Meta count, and the cover fan of section 26 at the right. **Sliding rows** (Shelves, Reading list, Highlights) bleed off the right page edge (margin 56), scroll sideways with no visible scrollbar (section 24). **Reading list:** covers 132 x 211, a Label number (01) 8 above. **Series row:** thumbnail 36 x 50, bold title, the segmented bar (section 19) and a Meta line. **Authors:** a 64 circle (`card` fill, 1px `border`, Playfair 500 18 initials), 13px name and a Meta count, 146 wide each. **Empty state:** zeros in the numerals, an empty axis and a dashed state card (section 10) 1168 x 300. **Loading:** `border` blocks for the numerals (90 x 46), timeline lines, list rows, a 400 disc at 50% for the sphere and 380 x 124 shelf cards.

## 54. Search results, syntax help and states
Result rows as the list rows (section 50). **Book text result:** a 36 x 50 thumbnail, the book title (600) with a Meta chapter, and a 13.5 snippet with line height 1.5 where the match uses the `match` tint in bold with radius 3. A skeleton row (`border` bars) stands for results still loading. Operators in the field: `accent`, weight 600. **Syntax help popover:** 360 wide, `card` fill, 1px `border`, radius 14, padding 20, anchored 8 under the `?` button and right-aligned, Section-style title, rows of an accent 600 operator (84 wide) and a Meta description, a hairline and a Meta closing line. **No matches:** the dashed state card (section 10) with a "Did you mean" suggestion in accent. **Indexing:** the quiet notice (section 30) with a 220 wide 3px progress bar at its right.

## 55. Control states (Library)
Rules as section 12, drawn for Add books (hover: 10% white overlay), the ghost button (hover `border` fill; the Done state is accent text on a `border` fill), the dropdown (open = accent border), the Grid / List option, the filter chip, the menu item, the cover (section 48), a timeline line (hover 5px, focus ring, dimmed 30% when another is open) and the bulk bar button (disabled 40%).

## Updates to existing sections
- **Section 6:** extended in place with the library components (sections 47 to 55).
- **Section 7 (TBD):** the progress ring, the timeline and the saved-words sphere are Cairo-drawn widgets. Placeholder icons also include plus, ⋯, trash, columns, double tick and the arrows. Cover fallback, Feature surface, Progress ring and Icons are otherwise unchanged.
- **Section 12 and section 17:** extended in place (library controls; popover widths 260, 280 and 300).
- **Section 20:** library cards use the double tick for finished; the circle badge stays on the series strip and the Author page panel until decided.

## Open (design system)
- Whether the accent double tick and the white selection ring stay readable on real cover art.
- Whether the finished double tick replaces the circle badge on the series strip, the Author page panel and the book page.
- Confirm the 8 column grid at 1280 and the 132 x 211 cover.
- Forest, Plum and Teal versions of the library boards and My Library.

---

# Additions v0.10 (app shell and Home chat)

All Proposed unless stated; applied to Terracotta only (Forest, Plum and Teal not checked). No new colour tokens. New type size: Quote large (section 63). New sizes: 520 search bar and box, 560 x 360 Home book, 470 and 600 Home columns, 278 hover card, 260 option menu. One new derived use: `text-2` at 25 to 70% opacity for ruler ticks and leader lines (section 61).

## 56. Navigation rail
68 wide, no fill, no border. A 2px wide divider with rounded ends on the right edge in the `border` colour, inset 28 from the top and 24 from the bottom. The logo (`logo-svg.md`, 38 x 37, `accent`) at the top, 28 down. The main group (Home, My Library, Shelves, Comics, Editor) is centred vertically, 8 gap; the bottom group (bell, Settings) sits above a 24 bottom padding. Items are 44 circles, icons 22 with stroke 1.8 (Lucide-style outline; set not chosen). Idle `text-2`. Hover: `border` fill. Focus: 2px `accent` ring, 2 offset. **Active: `accent-fill` circle with the icon in the `bg` colour** (it reads as cut out). Bell dot: 8, `accent`, 8 from the top and 9 from the right. Tooltip: 32 high, `card` fill, 1px `border`, radius 10, 13px name and an 11px Meta shortcut, to the right of the item after a short delay. Decisions 179 and 180.

## 57. Search bar and box
Bar: 520 wide, 40 high, 1px `border` underline at rest and in `accent` when focused; magnifier 18, text 14 (hint in `text-2`), a 1.5px `accent` caret; "N results" in Meta at the right, then the **option picker**: a bare 32 x 32 button (radius 8, no outline) with a three-row sliders icon 20 in `text-2`, `border` fill on hover or while its menu is open. Box: attached under the bar, 520 wide, `bg` fill, 1px `border`, no top border, radius 0 0 14 14, scrolling inside with a 48 to 56 fade (section 24). Group heading: Label style, "Name · count", 14 20 4 padding, an accent "See all N ›" (12) at the right. Rows 52 (books, series), 48 (authors), 44 (history); the highlighted row has the `border` fill. The matched text is bold on the `match` tint. Snippets: Quote style 14 with the book and chapter in 11px Meta. History rows: a clock icon, the query, a delete cross. The no-match message is a dashed state card (section 10) inside the box. Option menu: the menu of section 17, 260 wide, with a switch row (section 10) and a Meta caption. Decisions 196 to 200.

## 58. Search pills, dropdown lists and tag marks
**Pill:** 28 high, 1px `border`, radius 999, 12.5px `text-2`, a 12 chevron; open or active: `accent` border and `text` colour, the chevron flipped. The pills row sits at the top of the box (10 16 padding, a 1px `border` line below). **Apply:** the section 11 primary at 28 high. **Sort and In lists:** the dropdown list of section 43, 220 and 240 wide, rows 36, an accent tick on the current choice. **Tag mark:** a 20 circle. Included: `accent-fill` with a white plus. Excluded: 1.5px `text-2` ring with a minus. Neutral: 1.5px `text-2` ring at 50%. **Tag pill:** 28 high, 1px border, 13px; included has an `accent` border and a plus, excluded is `text-2` and struck through with a minus; a 12 cross. **Facet row:** 44 high, 1px top line, the name (bold when open), the picked count in `accent`, a chevron; open lists use 40 high rows with a mark, the name and a Meta count. **Field:** 36 high, radius 10 (as section 43). Decisions 202 to 205.

## 59. Search results from outside the library (AO3)
Work blurb: padding 12 20, a 1px `border` line below; title (Playfair 600 14) and "by author" (Meta) on one line, the fandom (Meta), a facts line in `text` Meta (rating, status, words, kudos), a summary in 13px `text-2`. Dashed state cards (section 10) for rate limited, no results and cannot reach, each with a 36 high ghost button.

## 60. Home book
560 x 360: two pages of 280. Left: cover art with its title printed in it (content, not UI), radius 10 on the outer corners, a 1px `border`. Right: `card` fill, 1px `border`, padding 40 32, radius 10 on the outer corners; a Label chapter line, a Card title chapter name, an excerpt in Playfair 13 with line height 1.85, a 96 fade to `card`, the bookmark ribbon (18 x 36, `accent`, 32 from the right) and a **play button**: 40 circle, `accent-fill`, the triangle in `bg`, 20 from the bottom and right. **Pager:** a column of 5 dots, gap 10: the current one an 8 x 22 `accent` pill, the others 8 circles in `text-2` at 45%. Decision 188.

## 61. Shelf and timeline ruler
Base line: 2px, `text-2` at 50%, rounded; it is also the axis. Bars: horizontal, radius 0 3 3 0, no border, cover colour (content colour), a 2px band of white at 20% 8 from the left and 6 from the right (content). Thickness 8 to 12 (16 in Weeks with the title inside at 11px, 4 in Years), length 50 to 150 from the page count (Weeks 110 to 140). Dots on the axis: 3.5 `accent` (2 in Years). Ticks: day and week `text-2` at 25 to 30%, month 44 to 54 long at 70%, year 34 to 54 long at full strength; labels 11px `text-2`, month names in Weeks and the year label in Playfair 600 12 `text`; Years adds a Meta "N books" under each year. Today: a 5 radius ring (`bg` fill, 2px `accent`) with "Today" in `accent` 11px. Top fade 40 to `bg`. **Elastic spacing:** a minimum of 17 between bar centres (Months), 18 (Weeks), 6 (Years); dates map to positions by one piecewise function shared by the ruler and the bars. Hover: the bar slides out 14, a 1px `text-2` leader at 40%, and the card. **Hover card:** 278 wide, padding 16, `bg` fill, 1px `border`, radius 14: cover 56 x 84 radius 6, title (Card title), author, stars in `accent` with the number, the date; a row of three Label-over-13px stats; reaction chips (section 10); the note in Quote style. **Zoom readout:** a 32 high pill, `card` fill, 1px `border`, shown only while zooming. Month filter: other bars at 25%, the month label in `accent`, a 28 high accent-bordered pill with a cross. Decisions 190 to 193.

## 62. Updates timeline
Heading Section style with a Meta line at the right. Line 2px `border` at the left, nodes 10 (filled `accent` for new, 1.5px `text-2` ring on `bg` for seen), rows 56 with the title (13, 600), a Meta line, and the time at the right; 4 rows visible and a 48 fade; scrolls without a visible bar. Decision 189.

## 63. Quote large
Playfair italic 18, line height 1.6, in `text`; a 1.5px `accent` left rule with 20 padding; the attribution in 11px Meta, 8 below. Home only. Decision 187.

## 64. Home state cards
The dashed state card of section 10, larger: 600 x 360 (560 x 360 for Nothing in progress), padding 24, centred content: a Section-style title (18), a Meta line, and ghost or primary pills (40 high). The empty shelf is the base line with the "Today" ring and one Quote-style line in `text-2`. Skeletons use `border` fill, radius 6. Decision 194.

## 65. Window controls
Three 32 round icon buttons (minimise, maximise, close), gap 4, icons 16 in `text-2`, transparent, `border` fill on hover (all three alike). At the top right over the page. Decision 181.

## 66. Toast with a link
The after-tool pill (section 39) with a trailing `accent` link ("Undo", "View"). Decision 184.

## Updates to existing sections
- **Section 3:** the type table has the Quote large row.
- **Section 4:** the nav rail line points to section 56.
- **Section 6:** extended in place with the chat 9 components (sections 56 to 66).
- **Section 7 (TBD):** Feature surface is resolved (Home has no feature card); Icons gets the Lucide-style proposal; Progress ring is unchanged (Home uses 3px bars, section 19).
- **Section 17 and 43:** the option menu (260 wide) and the Sort and In lists reuse them.

## Open (design system)
- Choose and bundle the icon set (Lucide-style icons were used).
- Whether Quote large becomes a shared size or stays Home only.
- Whether the `text-2` at 25 to 70% opacity for ticks and leader lines should be a named token.
- Forest, Plum and Teal versions of the chat 9 boards, in particular the stencil icon in `bg` on `accent-fill`.
- Cover dominant colours are content, not tokens; confirm that rule.
