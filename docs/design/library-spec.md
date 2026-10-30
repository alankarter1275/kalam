# Library browsing and My Library: design spec

Source boards: https://claude.ai/artifact/ApKgpEtW6jhiAJUTkSQekz (Terracotta, light and dark). Tokens are from `design-system.md` (sections 47 to 55 are new in v0.9). Decisions 159 to 178 in `decisions.md`. Neighbouring specs: `book-details-spec.md`, `author-page-spec.md`, `series-page-spec.md` and `settings-spec.md`. The rail is not drawn here (chat 9).

## 1. Layout (1280 x 900 in the mockups; 1920 wide and 640 wide variants)

A full page on plain `bg`, margin 56 (24 at 640). No band, tint, shadow or glow.

| Area | Position | Notes |
|---|---|---|
| Title row | top 56 | Label, Display title with a chevron (opens the shelves menu), Add books pill at the right |
| Search | 24 below | 44 high pill, `?` icon inside at the right |
| Toolbar | 14 below | Filter, chips, Clear filters at the left; count, Group, Sort, Select, Grid / List at the right |
| Grid | 24 below | 8 columns at 1280, 12 at 1920, 4 at 640; gap 16; covers 1.6 : 1 |
| Bulk bar | bottom centre, 32 up | selection mode only |
| Jump strip | right edge, 20 in | title or author sort only |

**My Library** (one long page, margin 56, content 1168 wide), in order: masthead with three numerals; reading timeline; Recently added (608 wide list) beside the saved-words sphere (520 x 480); Shelves (one sliding line); Reading list (sliding line); Series you follow (two columns); Highlights (sliding row); Authors you read most. **Impressions page** and **Timeline** are separate boards.

## 2. Components

- **Title dropdown** (section 47): the title is a button; the popover is 280 wide with All books, shelves with Manual or Smart and counts, Manage shelves…, New shelf….
- **Add books menu** (section 47): Import files… Ctrl+I, Import a folder… Ctrl+Shift+I, a disabled Find online….
- **Toolbar** (section 47) and the Filter menu with Shelf and Tag submenus (decision 168).
- **Cover card** (section 48): format chip, progress bar, title, author, hover ⋯ quick actions, accent double tick for finished, fallback cover, skeleton.
- **Selection mode and bulk bar** (sections 48, 49): outline circles, accent-outlined selected cards, the 48 high floating bar.
- **Right-click menu** (decision 166, section 17), **Sort and Group lists** (decision 167).
- **List view** (section 50): configurable columns, Columns menu, drag and resize states.
- **Search** (section 54): operators in accent, syntax popover, grouped results, no-matches and indexing states.
- **Timeline** (section 51): thin lines, axis, Today, card, expand.
- **Saved-words sphere** (section 52).
- **My Library sections** (section 53): masthead numerals, Recently added list, shelf cards, sliding rows, Series rows, Authors, empty and loading states.
- **Impressions page** (decision 177): mood chips, dropdowns, card grid, quiet line.

## 3. Dialogs

None are designed here. The Remove confirmation, import progress and the file and folder choosers belong to chat 17 (native `Gtk.FileDialog`). The Add your impression dialog is decision 91. The Bubbles screen is chat 16.

## 4. Tokens used

`bg`, `text`, `text-2`, `accent`, `accent-fill`, `card`, `border`, `overlay`, `match`, and the five highlight colours (timeline lines). `text-3` is not used. White appears on `accent-fill` fills and, as a proposed exception, as the unselected selection ring on cover art (decision 178). Fonts: Playfair Display and DM Sans. Radii 4, 6, 10, 12, 14, 999 (12 only for the dragged-column tint). New sizes: see design system v0.9. No new colours.

## 5. States

Drawn (dark and light): All books grid (idle, hover, focus, finished, no-cover card, second row loading as skeletons); Shelves menu open; Add books menu open; multi-select with the bulk bar; drop-to-import overlay; grouped by author with the jump strip; Search (syntax popover, book-text results, a skeleton row); Menus (Sort, Group, Filter, right-click); Shelf view; Filter with the Shelf and the Tag submenu; list view; Columns menu; column drag; column resize; narrow window (640); wide window (1920); States (no matches, empty library, library error); Search states (no matches, indexing); control states for the library controls; My Library; My Library empty; My Library loading; Timeline (collapsed, a line open, expanded, after removing with Undo); Timeline with no impression; Timeline line focus; Impressions page.

Not drawn: see "Not designed yet (library)" in `decisions.md`.

## 6. GTK widgets and custom drawing

- **Page:** `Gtk.ScrolledWindow` (external scrollbar policy) over a `Gtk.Box`; the bulk bar, drop overlay and jump strip are `Gtk.Overlay` children.
- **Grid:** `Gtk.GridView` with lazy covers and a fixed cover width, column count from the window width; skeletons are item variants. The group headings need a section header (`Gtk.ListView` sections or a header row type). List view: `Gtk.ColumnView` with resizable, reorderable columns; the tinted dragged column and the accent drop bar are drawn by a custom overlay, and the slide-aside is an animated margin on the sibling column headers.
- **Title dropdown, Add books menu, Sort, Group, Filter, right-click, Columns menus:** `Gtk.Popover` with no arrow and a custom list (not `Gio.Menu`), as in `book-details-spec.md`. The Tag submenu uses a `Gtk.SearchEntry` over a `Gtk.ListView`.
- **Search:** `Gtk.SearchEntry` styled as the pill; operator highlighting through a `Pango.AttrList` on a `Gtk.Entry`; results in `Gtk.ListView`; match tint via Pango attributes.
- **Timeline:** a **custom-drawn widget** (Cairo): axis, lines, Today line, hit testing for hover and click, keyboard focus with Tab and Enter, a horizontal scroll by drag or `Gtk.GestureDrag` and scroll events. The open card is a child widget in a `Gtk.Fixed` or `Gtk.Overlay` placed under the line, with the lines below shifted by the card height (animated); the card can be a `Gtk.Revealer`. Expand repacks the lanes with labels.
- **Saved-words sphere:** a **custom-drawn widget** (`Gtk.DrawingArea`, Cairo and Pango): Fibonacci points rotated in 3D, depth sorting, cached `Pango.Layout`s per word and weight, a vertical word is a rotation of the Cairo context, drag with `Gtk.GestureDrag` and inertia, drift driven by `add_tick_callback` (stopped by Reduce motion and while hovering), `Gtk.EventControllerMotion` for the hover card, `Gtk.GestureClick` for the click. Never capture the scroll event.
- **Switches, segmented control, sliders:** as in the other specs. Arrow buttons in the section heads call `scroll_to` by one card.
- No conic gradients, `mask-image`, `backdrop-filter`, widget rotation by CSS or blurred glows. No visible scrollbars anywhere (decision 90).

## 7. Notes for the builder

- Word frequency for the sphere comes from the full-text index (`fts5vocab`), computed in the background and cached; the saved words themselves come from the vocabulary store. The sphere draws only the most-seen words that fit; the radius grows with the square root of the count up to the stage size.
- Timeline data comes from reading sessions and finish dates. A book removed from the timeline is hidden there only and stays in the library; store the flag with the book in `kalam.json`.
- Column choice, width and order, Sort, Group and the Grid or List choice are remembered per library.
- Search never touches the network. Indexing runs in the background and the page says so while it runs.
- Open in Bubble needs at least two selected books; the right-click item opens the book beside the current Bubble.
- The finished double tick and the white selection ring sit on cover art. Check contrast on real covers and fall back to a `card`-filled chip if needed.
- Every icon-only button needs a tooltip and an accessible label. Add keyboard focus rings everywhere (2px accent, 2px offset). Hover uses `border` colour as a fill. The grid, list, timeline lines and sliding rows must be reachable with the keyboard.
- No tap-to-look-up (decision 9). Clicking a word in the sphere opens the saved word, not a new lookup.
