# Book details page: design spec (draft)

Source boards: https://claude.ai/artifact/VxJw6sDJFH4kqpEEAg1cfW (Terracotta, light and dark). Tokens are from `design-system.md`. Decisions 77 to 93 and 102 in `decisions.md`. Applies to EPUBs and PDFs. Comics keep their series page.

## 1. Layout (1280 wide in the mockups)

A scrolling page, no band, on plain `bg`. Margin 56.

| Area | Position | Notes |
|---|---|---|
| Back pill | top-left, 16 inset, 40 high | `card` fill |
| Cover | left 56, top 96, 200 x 300, radius 10 | cover pencil 36 circle at bottom-right, 8 inset |
| Progress | under the cover, 200 wide | percent and "Ch. N / M" row, 3px bar, start date |
| Header block | left 288, top 96 | kind label, title, series link, credits, facts, rating, tags, description, actions |
| Sections | top 540 | two columns (below) |
| File line | below the taller column | 11px, `text-2` |

**Header block, in order:** kind label (Label style, `accent`); title (Display 46); series link (Playfair italic 16, accent, chevron); credits; facts line (Meta); rating row; tag chips and "+" chip; description (Meta, line height 1.6, 3 lines, "More"); actions row (top margin 20).

**Sections:**

| Column | Left 56, width 560 | Left 668, width 556 |
|---|---|---|
| Content | Contents (outlined box) | Your impression (outlined box, if any), then Annotations |

An unfinished book has no impression box: Annotations moves to the top of the right column.

## 2. Components

- **Credits:** role labels (Label style) appear only when there are two or more roles; one role is just the linked name. Names link to the Author page.
- **Series link:** "Harrow Maps · Book 1 of 4 ›". It opens the series page.
- **Rating:** 5 stars, 20px, half steps, letter spacing 3. Empty: stars in `border` colour and "Not rated yet". Value text ("4.5") in Meta. Focus ring around the group.
- **Tag chip:** the 28 high chip from the design system. The "+" chip is the same height with an icon.
- **Actions row:** Read (`accent-fill`, white text, 44 high pill); Finished toggle (ghost; selected = `card` fill with an accent tick); Reading list (ghost); pencil and ⋯ (44 circle icon buttons).
- **Finished toggle and Next pill:** Finished has the `card` fill; the pill sits behind it (margin-left -24, padding-left 38, 44 high, `bg` fill, 1px `border`). Following buttons shift right. Disabled pill: opaque fill, text at 40%.
- **Outlined box:** 1px `border`, radius 14, padding 20, `bg` fill (no fill change). Contents and Your impression.
- **Contents timeline:** rows 64 high, `padding-left` 44. Node 24 circle at left 0. Line 2px wide at left 11, drawn as an upper segment (0 to 20) and a lower one (44 to 64) per row: accent where the chapters on both ends are read or current, `border` after. Read node: `accent-fill` with a white 14 tick. Current node: 2px accent ring, accent eye icon (14), `bg` fill. Upcoming node: 1.5px `text-2` ring at 50% opacity. Text: "Chapter N" (Meta) over the title (14; current is 700). Right: date (Meta), or "42%" (accent, 500). Bookmark indicator before the date: a 14 bookmark icon in accent, and "×2" or "×3" (12, 500) only when there is more than one.
- **Contents expanded:** the same outlined box grows to a fixed height of 640 (header, 8 rows = 512, then "Show less"), `bg` fill, opaque. Rows scroll inside; 48 high fade to `bg` at the bottom of the list; no scrollbar.
- **Annotations:** heading (Section style) with a "See all N" link; highlights each have a 3px colour bar, Quote 16 text, a Meta line; a hairline above each; a final count line.
- **Your impression box:** heading with a 36 pencil icon button; stars and "4.5 · Finished 12 Sep 2026"; reaction chips; the note in Quote style 16, line height 1.7. Skipped state: the same box with a 1.5px dashed `text-2` border, text "You marked this finished on 12 Sep. How was it?" and a ghost "Add your impression" button.
- **Popovers and menus** (`card` fill, 1px `border`, radius 14, padding 6): items 40 high, radius 8, 12 side padding; dividers 1px `border` with 6 and 8 margins. Cover popover 210 wide; ⋯ menu 240; Shelves submenu 240 (tick circle 22, 12 gap, a divider, "New shelf…" in accent). The "Book edits" badge is a 22 high chip with an accent border and number.

## 3. Dialogs (on the `overlay` scrim, decision 75)

| Dialog | Size | Contents |
|---|---|---|
| Tag dialog | 420 wide, padding 24 | title and book name, close; add-a-tag field (44, radius 10); On this book; Suggested from the file; Your library (sort switch, rows 44 with tick circle, count, ⋯); Done |
| Tag row menu | 210 wide | Rename everywhere, Merge into…, Delete from library (accent) |
| Delete tag confirm | 420 wide | title, body with the count, ghost Cancel, accent-fill Delete tag |
| Annotations | **560 wide**, 660 high, padding 24 | title, book name, close; tab pill (Highlights / Bookmarks / Words, counts); list body |
| Impression | 420 wide, padding 24 | see decision 91 |

Tab pill: `bg` fill, 1px `border`, padding 4, 36 high tabs, active `accent-fill` with white text. Reaction chips use the standard chip; selected has an accent border and `text` colour.

## 4. Tokens used

`bg`, `text`, `text-2`, `accent`, `accent-fill`, `card`, `border`, `overlay`, the five highlight colours. `text-3` is not used. Fonts: Playfair Display (titles, quotes, headwords) and DM Sans. No new colours or radii (radii 4, 6, 10, 14, 999 only). The 560 dialog width is the one new size (decision 92).

## 5. States

Drawn: in progress; finished with an impression; no cover; file missing; files-only PDF (also stands for unread); Contents expanded; Annotations dialog (three tabs); Impression dialog over the reader; Tag dialog with a row menu and the delete confirmation; cover popover; ⋯ menu with the Shelves submenu; Finished-button idle, hover and disabled Next; skipped-impression prompt; control states for the primary, ghost, Finished toggle, icon, chip, tab, tag row, star rating, menu item and chapter row.

Not drawn: see "Not designed yet" in `decisions.md`.

## 6. GTK widgets and custom drawing

- **Page:** a `Gtk.ScrolledWindow` (external scrollbar policy) over a `Gtk.Box` or a `Gtk.Overlay` for the Back pill. Two columns with `Gtk.Box` or `Gtk.Grid`.
- **Contents timeline:** a `Gtk.ListView` row factory with a **custom-drawn gutter widget** (line segments and nodes via Cairo); the eye and tick are symbolic SVG icons. Expanded state: a `Gtk.Revealer` or an overlay child with a fixed size and a `Gtk.ScrolledWindow`; the bottom fade is a plain linear gradient.
- **Next-in-series slide-out:** two widgets in a `Gtk.Overlay`, the Next pill in its own `Gtk.Revealer` (slide right) behind the Finished button; the following buttons move through a second revealer or an animated margin. Do not rely on CSS transforms.
- **Rating:** a **custom-drawn widget** (painted stars with half fill, hover preview, keyboard arrows, accessible value).
- **Menus and popovers:** `Gtk.Popover` with no arrow (cover popover, ⋯ menu, Shelves submenu, tag row menu). The ⋯ menu is a popover with a custom list, not `Gio.Menu`, so the badge and the submenu match the design.
- **Dialogs:** a modal window or overlay widget on an `overlay` scrim.
- **Tag dialog:** a `Gtk.SearchEntry` (or `Gtk.Entry`) with a popover for autocomplete; a `Gtk.FlowBox` for chips; a `Gtk.ListView` for the library list.
- **Impression dialog:** `Gtk.TextView` for the note; `Gtk.ToggleButton` chips in a `Gtk.FlowBox`.
- **Painted by the reader engine (not here):** the impression dialog's page backdrop is the reader's own page.
- No conic gradients, `mask-image`, `backdrop-filter` or blurred glows. No visible scrollbars anywhere (decision 90).

## 7. Notes for the builder

- The existing `book.rs` hero and card grid are replaced; reuse its data (progress, notes, shelves, edits) and its actions.
- The series position is read from metadata. Do not count books in the library.
- The Next pill needs the next book's title from the series metadata, and whether it is in the library.
- Add keyboard focus rings everywhere (2px accent, 2px offset). Hover uses `border` colour as a fill. Every icon-only button needs a tooltip and an accessible label.
- No tap-to-look-up (decision 9).
