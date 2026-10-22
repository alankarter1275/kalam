# Series page: design spec (draft)

Source boards: https://claude.ai/artifact/FJeo3h751XuLvXvSmt32Ha (Terracotta, light and dark). Tokens are from `design-system.md`. Decisions 94 to 101 in `decisions.md`. The comics series page (decision 44) is kept aside and not changed here.

## 1. Layout (1280 wide in the mockups)

A scrolling page, plain `bg`, margin 56. No band, tint, drop shadow or glow.

| Area | Position | Notes |
|---|---|---|
| Back pill | top-left, 16 inset, 40 high | `card` fill |
| Header block | left 56, top 96, 700 wide | Series label, Display title, author link with facts beside it, segmented progress, progress line, two-line summary |
| Actions | top-right, 56 from the edge, top 116 | Continue (primary, 44 high) and ⋯ (44 circle) |
| Books row | top 340 | "Books" label (Label style) on the left; Strip / List switch on the right |
| Strip | left 56, top 392 | covers, gap 28 |
| Focused-book row | top 880, full width | hairline above, padding-top 20 |
| Source line | below the content | 11px, `text-2` |

## 2. The strip

- **Focused cover:** 260 x 390, radius 10. **Other covers:** 180 x 270. All bottom-aligned on a 390 high row.
- **Under each cover:** title (Playfair 14; 700 when focused, 600 otherwise) and the year (Meta).
- **In progress:** a 3px progress bar under the cover (cover width), above the title.
- **Finished:** a 22 circle badge, `accent-fill`, white 14 tick, 8 from the cover's top right.
- **Missing book:** a 1.5px dashed `text-2` border, radius 10, the title (Playfair, `text-2`) and "Not in library" (11px) centred, in its numbered position.
- **No underline** under the focused title, and no shadow. Focus is shown by size and the bold title.
- **Placeholders** for cover art are neutral gradients, the same in every palette.
- **Long series:** the strip continues past the right edge and scrolls to keep the focused book in view (not drawn).

**Default focus:** the next owned book you have not finished; for a finished series, the last book.

**Interactions:** click the focused cover to open that book's details page; click another cover to focus it; arrows (beside the counter) move the focus; Left and Right keys move the focus; Enter opens the focused book; Esc goes back. A missing book can be focused but not opened (its info row would read "Not in library"; not drawn). No "Open book" button.

## 3. Header and focused-book row

- **Segmented progress:** 220 wide in total, 3px high, 4 gap, one segment per book: read = `accent`; in progress = `accent` to 42% then `border`; unread = `border`; missing = dashed `text-2`.
- **Progress line** (Meta): "0 of 2 books read · You own 2 of 4"; "4 of 4 books read · Series complete".
- **Continue label:** "Continue Book N", "Start Book N" (nothing started in the owned books), or "Read Book 1 again" (all read). Continue opens the focused book's next unread position in the reader.
- **Focused-book row:** three parts in a row (gap 48): "Book N · year" (Label style) with the title (Playfair 500, 22) and the state (a 240 x 3 progress bar with "42% · Chapter 9 of 31", or a tick and "Finished 3 Oct", or "Unread"); the description (Meta, line height 1.6, up to 520 wide); the arrows (36 circle icon buttons) with a counter ("1 / 4", Meta).

## 4. List view

Plain rows on the page, 64 high, 1px `border` dividers, no boxes, full width.

| Part | Spec |
|---|---|
| Thumbnail | 36 x 50, radius 4; missing = 1.5px dashed `text-2` |
| "Book N" | Meta, 56 wide |
| Title and year | title 700 (current), 600 (unread), regular (read); year in Meta |
| State (right) | read: tick and "Read"; in progress: 100 x 3 bar and the percentage; unread: nothing; missing: "Not in library" |
| Current row | 3px `accent` bar, 14 to the left of the row |
| Missing row | 55% opacity |

The Strip / List choice is remembered per series. The switch is a pill container (`bg` fill, 1px `border`, padding 4) with 28 high options; the selected one is `accent-fill` with white text.

## 5. Tokens used

`bg`, `text`, `text-2`, `accent`, `accent-fill`, `card`, `border`. `text-3` is not used. Radii: 4, 10, 999. Fonts: Playfair Display and DM Sans. No new colours or radii.

## 6. States

Drawn (dark and light): reading (two books owned, one in progress, two missing); books missing (only Books 2 and 4 owned); finished series; list view for a 15-book series; control states (strip cover, arrow button, Strip / List switch, list row, Continue button: idle, hover, focus, selected, disabled).

Not drawn: a missing book focused, the strip scrolled for a long series, narrow windows, loading and error states, Forest, Plum and Teal.

## 7. GTK widgets and custom drawing

- **Strip:** a **custom widget**: a horizontally scrolling box of cover widgets in a `Gtk.ScrolledWindow` (external scrollbar policy, no visible bar), with the focus state animating between the two sizes by snapping. No shadows. A short slide on focus change is optional.
- **Cover widget:** a `Gtk.Picture` with a rounded clip, or a custom-drawn widget for the dashed missing cover and the badge. The tick and the progress bar can be CSS or painted.
- **Segmented progress:** a small custom-drawn widget (n segments with partial fill), or a `Gtk.Box` of CSS-styled children.
- **List view:** a `Gtk.ListView` with a row factory.
- **Strip / List switch:** linked `Gtk.ToggleButton`s.
- **Arrows:** icon buttons. **Counter:** a `Gtk.Label`.
- **Page:** `Gtk.ScrolledWindow` over a vertical `Gtk.Box`, with the Back pill in an overlay.
- No conic gradients, `mask-image`, `backdrop-filter` or blurred glows. No visible scrollbars anywhere.

## 8. Notes for the builder

- The series name, order, position ("Book N of M"), total count, years, titles and descriptions of books you do not own all come from metadata sources. Owned books are matched to them; unmatched series entries become missing books.
- Continue and "Next in series" (book details page, decision 87) share the same ordering.
- Every cover and arrow needs a tooltip and an accessible name; the strip needs keyboard focus handling (Left, Right, Enter, Esc) and visible focus rings (2px accent, 2px offset).
- The series float in `series_float.rs` is retired; its entry points open this page instead. Entry points: the series link on the book details page (decision 81) and "Open series ›" in the open series panel on the Author page (decision 108).
- The comics series page is not changed by this spec. Reconcile the two later (open question in `decisions.md`).
