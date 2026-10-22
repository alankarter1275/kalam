# Author page: design spec

Source boards: https://claude.ai/artifact/AJvyQ17bqLL6bcFcqTcEkQ (Terracotta, light and dark). Tokens are from `design-system.md` (sections 25 to 31 are new in v0.6). Decisions 104 to 117 in `decisions.md`. Neighbouring specs: `series-page-spec.md` and `book-details-spec.md`.

## 1. Layout (1280 wide in the mockups)

A scrolling page on plain `bg`, margin 56, content starts at 96. No band, tint, shadow or glow (decision 104).

| Area | Position | Notes |
|---|---|---|
| Back pill | top-left, 16 inset, 40 high | `card` fill |
| ⋯ button | top 12, right 56, 44 circle | opens the menu (section 2); pressed state uses `border` as fill |
| Header, left block | left 56, up to 640 wide | Author label, name, bio, two pills |
| Facts panel | right, 340 wide, level with the label | design system section 25 |
| In library | Label heading, 48 above it, 16 below | two-column grid of series cards, gap 16 |
| Not in library | same pattern | same cards, all missing |
| Your highlights | heading row, 48 above | sliding row, bleeds off the right edge |
| Footer line | 32 below the last section | 11px, `text-2` |

**Header block, in order:** "Author" (Label style, `accent`); the name (Display 46); the bio (Meta, line height 1.6, up to three lines, "More" in `accent`); a pills row, 20 above it: ghost pills, 40 high, **Create shelf** and **Find metadata**.

**Order inside each group:** Standalone works first, then series in series order (decision 106).

## 2. Components

- **Facts panel** (section 25): portrait, lifespan, and rows for Works, Series (hidden when none) and You've read with a segmented bar. With no details: "No details yet" in place of the lifespan.
- **Series card** (section 26): title, segmented bar (read, unread, missing), Meta line "N of M owned · N read · first year to last year", cover fan, chevron. On the Not in library group the Meta line has no read count and every segment is dashed.
- **Open series panel** (section 27): header row with "Open series ›", then the covers; owned covers show title, year and the finished badge or progress bar (section 20); missing covers are dashed with the fetch button (section 28).
- **Highlights row** (section 29): heading with "See all N" and two arrow buttons; cards with a header, hairline and body. Hidden when there are no highlights.
- **Quiet notice** (section 30): "No bibliography yet…" line with a Find metadata link.
- **⋯ menu** (a no-arrow popover, section 17, 240 wide): Edit author…; Create shelf… (Open shelf when one exists); divider; Find metadata for N books…; Add all to reading list; Show books in library; Export highlights…; divider; Merge into another author…. Hover uses `border` as fill. Destructive items are not in this menu.
- **Footer:** "Bibliography from Open Library · updated 3 days ago". Plain information (decision 110).

## 3. Dialogs (on the `overlay` scrim, decision 75)

| Dialog | Size | Contents |
|---|---|---|
| Create shelf | 420 wide, padding 24 | section 31: Name field, "Name the shelf after the author" switch (on = field disabled), Smart / Manual, live count, Cancel and Create shelf |
| Annotations (author scope) | 560 wide, 660 high, padding 24 | as decision 92, tabs Highlights / Bookmarks / Words with counts, All chip and colour dots, items grouped by **book** with chapter and date in Meta, bottom fade |
| Find metadata (all books) | not here | the shared dialog from the metadata chat (decision 48) |
| Edit author, Merge into another author | not designed | see decision 112 |

## 4. Tokens used

`bg`, `text`, `text-2`, `accent`, `accent-fill`, `card`, `border`, `overlay`, the five highlight colours (the dot on highlight cards and the filter dots). `text-3` is not used. Radii: 4 (fan covers), 6 (highlight covers), 10 (covers, fields), 14 (cards, panels, dialogs), 999 (pills, portrait, icon buttons). Fonts: Playfair Display and DM Sans. New sizes (design system v0.6): 420 highlight card, 72 x 108 highlight cover, 56 x 84 fan cover, 124 series card, 64 portrait. No new colours.

## 5. States

Drawn (dark and light): full page with In library, Not in library and highlights; series open with hover on a missing-cover fetch button and a focus ring on another; no bibliography yet; files only (initials, "No details yet", Find metadata pill); loading skeleton; one book owned with no highlights and no Series row; ⋯ menu open with a hovered item; Create shelf dialog (Smart, name switch on); Annotations dialog, Highlights tab. Control states drawn: series card hover and focus, fetch button hover and focus, arrow button disabled, menu item hover.

Not drawn: see "Not designed yet" in `decisions.md`.

## 6. GTK widgets and custom drawing

- **Page:** `Gtk.ScrolledWindow` (external scrollbar policy) over a vertical `Gtk.Box`; Back pill and ⋯ in a `Gtk.Overlay`.
- **Series grid:** a two-column `Gtk.FlowBox` or `Gtk.Grid` of card widgets (a `Gtk.Button` with a custom child).
- **Cover fan:** a `Gtk.Overlay` of three `Gtk.Picture` widgets with left margins 0, 22 and 44. **No rotation.** The dashed missing cover is custom-drawn (Cairo dash), as on the series page.
- **Segmented progress:** a small custom-drawn widget (read, unread, missing segments) shared with the series page and the facts panel.
- **Open series panel:** an overlay child anchored to the card with a fixed width and a `Gtk.Revealer` (slide down); a horizontal `Gtk.ScrolledWindow` for the covers.
- **Highlights row:** a horizontal `Gtk.ScrolledWindow` (external scrollbar policy) with no visible bar; the arrows call `scroll_to` by one card. The four-line quote clamp is a `Gtk.Label` with `lines` set and ellipsize on.
- **Menu:** `Gtk.Popover` with no arrow and a custom list (not `Gio.Menu`).
- **Create shelf dialog:** a modal window or overlay widget; the name switch is a `Gtk.Switch`; Smart / Manual are linked `Gtk.ToggleButton`s; a smart shelf stores a rule, not a list of books.
- **Portrait:** a circular clip of a `Gtk.Picture`, or a custom-drawn widget with initials.
- No conic gradients, `mask-image`, `backdrop-filter`, widget rotation or blurred glows. No visible scrollbars anywhere (decision 90).

## 7. Notes for the builder

- The bibliography is cached locally. Never fetch on open. Show "updated N days ago" from the cache time (decision 110).
- Match owned books to cached works. Duplicate editions and translations collapse into one work. Owned books that match nothing go in their series if the file metadata names one, otherwise under Standalone works.
- "N of M owned" and the segments come from the cached bibliography. Without it (no bibliography yet) omit the counts, the Not in library group and the Series row.
- Hide the highlights section when the author has no highlights. Clicking a card opens the book at that highlight.
- **Create shelf:** with the name switch on, keep the shelf name linked to the author name. A Smart shelf is the rule "author is <name>" (the smart shelf rule builder already supports the author field). If the author already has a shelf, the menu item reads Open shelf.
- **Find metadata for N books** opens the shared Find metadata dialog scoped to this author's owned books. It is only an entry point here.
- The + fetch button is disabled until online sources exist (Part 2, decision 8). Do not build a download flow for it yet.
- Credits on the comics series page now open this page (decision 116). Make a credit a link only when the person has an Author page.
- Every icon-only button needs a tooltip and an accessible label. Add keyboard focus rings everywhere (2px accent, 2px offset). Hover uses `border` colour as a fill. Cards and the strip rows must be reachable with the keyboard (Enter opens a card, Esc closes it).
- No tap-to-look-up (decision 9).
