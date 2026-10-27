# App shell and Home: design spec (draft)

Source boards: https://claude.ai/artifact/9rt3qyi4FnmEFrfKte5TBD (Terracotta, light and dark). Tokens are from `design-system.md` (sections 56 to 66 are new in v0.10). Decisions 179 to 207 in `decisions.md`. Neighbouring specs: `library-spec.md`, `book-details-spec.md`, `settings-spec.md` and `epub-reader-spec.md`. The first-run flow is not designed (decision 207).

## 1. Layout (1280 x 800 in the mockups)

A full page on plain `bg`. No band, tint, shadow or glow.

| Area | Position | Notes |
|---|---|---|
| Rail | left, 68 wide | section 56; logo top, five entries centred, bell and Settings at the bottom |
| Search | top left of the main area, 28 from the top | bar 520 x 40 (section 57); the box drops under it |
| Left column | 470 wide, 56 from the rail | greeting (two lines, Display), quote, Your shelf |
| Right column | 600 wide, 52 below the search row, nudged 24 past the right margin | Home book (560 x 360 plus the pager), then Updates |
| Window controls | top right, over the page | section 65 |

**Left column, in order:** greeting; the quote with its `accent` rule (section 63), 28 below; Your shelf (heading with the Arrange by chip, an area 340 high, a caption), 24 below the quote. **Right column:** the open book, then Updates 36 below (heading, four rows of 56, a fade). When there is no saved source, Updates is not drawn.

## 2. Components

- **Rail** (section 56, decisions 179, 180): five entries (Home, My Library, Shelves, Comics, Editor), bell and Settings. Active is an `accent-fill` circle with a `bg` glyph. Tooltips with shortcuts.
- **Search bar and box** (section 57, decisions 196 to 201): the underlined bar, the bare three-row options icon, "N results", the attached box with history, results groups, snippets, loading and no-match states. Option menu with the **Search inside books** switch.
- **Online search (AO3)** (sections 58 and 59, decisions 202 to 205): the same bar, the In, Sort and Filters pills, the tag field, Selected pills, facet lists with the three-state mark, results blurbs, error states. Disabled until Part 2.
- **Home book** (section 60, decision 188): cover page, reading page with ribbon, play button, pager of 5 dots.
- **Your shelf** (section 61, decisions 190 to 193): vertical base line, horizontal bars, elastic date ruler, Today ring, Arrange by chip, hover card, month filter, zoom readout. Weeks, Months (default) and Years views.
- **Updates** (section 62, decision 189): thin vertical timeline, four rows visible, scrolls.
- **Quote** (section 63, decision 187): Quote large with the rule.
- **State cards** (section 64, decision 194): empty library, loading skeletons, error, nothing in progress.
- **Notification panel** (design system section 45, decision 195): floating beside the rail.
- **Window controls and toast with a link** (sections 65 and 66, decisions 181 and 184).

## 3. Dialogs

None are new. The standard dialog (design system section 13) is drawn on the Shell board with the focus rules of decision 183. The Add books, import and folder choosers belong to the Library chat and chat 17.

## 4. Tokens used

`bg`, `text`, `text-2`, `accent`, `accent-fill`, `card`, `border`, `match`. `text-3` is not used. White appears only on `accent-fill` fills and as the 20% band on cover-colour bars (content). Fonts: Playfair Display and DM Sans. Radii 3 (bar ends), 4, 6, 8, 10, 14, 22 and 999. New: the Quote large size (18 italic), the 520 search width, the 560 x 360 book, the 278 hover card, and `text-2` at 25 to 70% for ruler ticks and leader lines. No new colours. Cover dominant colours are content.

## 5. States

Drawn (dark and light): Home; Home with the search box open (typing, results); the search states board (resting, focused with history, typing, inside books switched on and loading, no matches, option menu open); the Inside books results board; the AO3 board (typing a title, results, In list, Sort list, Filters open, tag suggestions, facets empty, rate limited, no results, cannot reach AO3); the shelf at Weeks and Years zoom; shelf states (Arrange by list, by colour, empty, one month filtered); Home states (empty library, loading, error, nothing in progress, notification panel open); the Shell board (rail states, tooltip, window controls, toasts, standard dialog with focus ring); the Shell patterns board in light and dark. Hover and focus are drawn for the rail, bars, pills and picker.

Not drawn: see "Not designed yet (app shell and Home)" in `decisions.md`.

## 6. GTK widgets and custom drawing

- **Page:** a `Gtk.Box` with the rail and a `Gtk.ScrolledWindow` (external scrollbar policy, decision 90) over two columns of `Gtk.Box`.
- **Rail:** a vertical `Gtk.Box` of toggle buttons (round, custom CSS for the stencil glyph), a `Gtk.Overlay` for the divider and the bell dot, `Gtk.Tooltip` or a small popover for names.
- **Search bar:** a `Gtk.Entry` (or `Gtk.SearchEntry`) in a box with a bottom border, a `Gtk.MenuButton` with a custom `Gtk.Popover` (no arrow) for the options. The results box is a `Gtk.Popover` or an overlay child anchored to the bar, with a `Gtk.ListView` per group inside a `Gtk.ScrolledWindow`. Match tint through `Pango.AttrList`. The Inside books switch is a `Gtk.Switch`.
- **AO3 pills and filters:** `Gtk.ToggleButton` pills; Sort and In open the dropdown list of section 43; the filter list is a `Gtk.ListView` of expander rows; the three-state mark is a small custom-drawn widget (Cairo) or a `Gtk.CheckButton` with custom drawing.
- **Home book:** a `Gtk.Overlay` of two `Gtk.Picture` or drawing widgets (cover page, reading page); the excerpt in a `Gtk.Label` with a fade as a plain linear gradient; the ribbon is the reader's two icons; the play button is a `Gtk.Button`; the pager is a `Gtk.Box` of drawn dots. Scroll and swipe use `Gtk.EventControllerScroll`.
- **Your shelf and ruler:** **one custom-drawn widget** (`Gtk.DrawingArea`, Cairo and Pango) for the base line, bars, ruler, dots and Today ring, with one date-to-position function shared by all parts, hit testing for hover and click, `Gtk.EventControllerScroll` (plain scroll moves, Ctrl + scroll zooms), keyboard focus (Left, Right, Enter). The hover card is a `Gtk.Popover` to the right. The accessible alternative is a `Gtk.ListView` with the same books.
- **Updates:** a `Gtk.ListView` in a `Gtk.ScrolledWindow` (external policy) with a bottom fade; the timeline line and nodes are drawn by a small custom widget.
- **State cards:** a custom-drawn dashed border (Cairo dash), as on the series and Settings pages. Skeletons are `border`-filled boxes.
- No conic gradients, `mask-image`, `backdrop-filter`, widget rotation or blurred glows. No visible scrollbars anywhere.

## 7. Notes for the builder

- Opening Home never touches the network (decision 110). Updates come from background checks. Online search runs only when started, and only the tag suggestions load while typing (after about 300 ms, cached).
- Library search never touches the network. Inside books is off by default; the switch stays on until turned off and resets at every restart (decision 199). Full text needs a background index (decision 199).
- The Home book needs an excerpt from the reader engine at the saved position. Clicking the cover opens the Book details page; play resumes in the reader.
- The shelf needs a dominant colour per cover (cached at import) and page counts. Reading time needs a per-book session log; "Took" uses the first opened and finished dates. Check what Analytics already records.
- Date mapping: proportional, with a minimum spacing between bar centres (17 at Months, 18 at Weeks, 6 at Years); the ruler stretches, the bars do not move off their dates.
- The name in the greeting is the user's own; "Harvey" is a placeholder. The quotes are a separate offline collection, not the reader's highlights.
- Online search is disabled until Part 2 (decision 8): the picker shows the sites dimmed. AO3 has no public API; throttle, cache, honour Retry-After, and check the terms first.
- Add keyboard focus rings everywhere (2px accent, 2px offset). Hover uses `border` colour as a fill. Every icon-only button needs a tooltip and an accessible label. `/` or Ctrl+F focuses the search, Esc closes the box. Reduce motion turns slides and fades off.
- No tap-to-look-up (decision 9).
