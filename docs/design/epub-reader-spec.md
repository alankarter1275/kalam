# EPUB reader: design spec (draft)

Source boards: https://claude.ai/artifact/LP8aFc62jYGWwFDbuPLux2 (Terracotta dark unless stated). Tokens are from `design-system.md`.

## 1. Layout

The window is the page. Nothing is visible except text, a small bookmark ribbon (top-right) and location text (bottom-right). Everything else floats over the page and is summoned.

| Surface | Position | Summoned by |
|---|---|---|
| Back pill (+ Bubble peek) | top-left, 16 from edges, 44 high | cursor rests in the top-left zone |
| Bookmark ribbon | top-right, 48 from the right edge | always visible; `B` toggles |
| Location text | bottom-right, 24 right, 16 bottom | always visible |
| Left panel (Contents / Notes) | left, 16 inset, 340 wide, full height minus 32 | edge zone or `T` / `H` / `W` |
| Right panel (Settings) | right, 16 inset, 340 wide | edge zone or `S` |
| Search bar | top centre, 52 high | Ctrl+F |
| Selection chip | above the selection, 44 high | selecting text |
| Dictionary popup | below the selection, 380 wide, flips above near the bottom | Define on the chip, or `D` |

Page text column: 620 wide, centred. Book text is Playfair Display 18, line height 1.75 to 1.8 (default to confirm).

## 2. Tokens used

- Colours: `bg`, `text`, `text-2`, `accent`, `card`, `border` (text at 10% alpha). `text-3` is not used anywhere in the reader.
- Panels and popups: `card` fill, 1px `border`. Panels radius 22. Popups and inset cards radius 14. Pills radius 999. Fields radius 10.
- Spacing: 4, 8, 12, 16, 20, 24 inside panels; panel inset 16.
- New tokens (see `design-system.md` section 9): `selection`, `match`, `match-current`, five highlight colours, `inset` (the page `bg` used as a recessed card inside a panel).

## 3. Behaviour

**Back pill.** 250 ms hover delay, slide down 220 ms ease-out. Bubble slides out from behind it 150 ms later (200 ms). On leave (after 300 ms grace) both hide together: Bubble in 180 ms, whole unit up 220 ms. Esc exits the book at any time. Keyboard focus on the pill also reveals it. Reduced motion: fade.

**Bookmark ribbon.** Idle: V-notched outline in `border` colour, fill `bg`, 26 wide, 32 high. Set: filled `accent`, 38 high. In continuous scroll it marks the current position.

**Location text.** Percentage in `text-2`. On hover: `Chapter title · NN%` in `text`.

**Panels.** No dim layer. Esc or a click on the page closes the open panel. Only one side panel per side.

**Contents tab.** 44 px rows. Indent 16 per level. Current chapter: 3px accent bar at the left edge and bold text. Parts are bold.

**Notes tab.** Chips: All, Highlights, Bookmarks, Words. Under Highlights: row of five colour dots plus a quote filter. Items grouped under uppercase chapter headings (Label style). Highlight item: 3px colour bar and an italic Playfair 14 excerpt, plus the note (one line, `text-2`) if present. Hover shows a delete icon. Delete is immediate; Ctrl+Z undoes it.

**Words view.** Search field (44, pill), switch Chapter / Book / All books, rows of headword (Playfair 600 16) and one-line meaning.

**Selection.** Tint: `text` at 16% (dark) or 14% (light). Handles: 2px accent bar with a round grip. These are painted by the reader engine, not by GTK CSS.

**Chip.** Five colour dots, divider, Note, Define, Fix typo. A dot tap highlights with the last-used style. Note highlights and opens the note box.

**Annotation popover** (tap an existing highlight): colour dots, four style samples (Solid, Underline, Squiggly, Strikeout), note box, delete. Autosaves on close.

**Dictionary popup.** See `decisions.md` 31. Senses are numbered per part-of-speech group. Chips navigate within the popup (history with a back arrow, Esc closes the whole popup). Save button toggles; Ctrl+Z undoes an unsave.

**Saved-word memory.** Dotted underline (1.5px, accent at about 75% alpha, offset about 5) on saved words, per the Word memory setting. 300 ms hover delay, then a read-only card above the word with all meanings and a saved tick. Flip below near the top edge.

**Search (Ctrl+F).** Enter and Shift+Enter move between matches. Esc closes. Scope defaults to the current chapter.

**Settings.** See `decisions.md` 23 and 24. The Reading pane's preview should use the current page text, not a fixed sample.

## 4. States

Drawn: idle, chrome revealed, bookmark set, location hover, Contents, Notes (highlights, empty), Words, settings (Reading top and scrolled, Shortcuts with a key being recorded), selection and chip, annotation popover, dictionary (full, opened from a chip, loading, not found), search (matches, no matches), saved word hover card.

Not drawn: see the end of `decisions.md`.

## 5. GTK widgets and custom drawing

- Stack: `Gtk.Overlay` over the reader canvas. Panels and the Back pill are children with alignment and margins.
- Slides: `Gtk.Revealer` (slide down for the Back pill, slide right for Bubble, slide left/right for panels). Put Bubble in its own revealer behind Back in an overlay. Do not rely on CSS transform animation.
- Chip and annotation popover: `Gtk.Popover` with no arrow. Dictionary popup: `Gtk.Popover` with a scrolled body and a bottom fade (a plain linear gradient).
- Toggles: `Gtk.Switch`. Steppers: two buttons and a label. Segmented controls: linked toggle buttons. Typeface: `Gtk.DropDown`.
- Icons: symbolic SVGs recoloured by CSS `color`. Bookmark: two icons (outline, filled).
- Painted by the reader engine (not GTK CSS): selection tint and handles, search match tints, highlight styles (solid, underline, squiggly, strikeout), the note dot at the end of a highlight, and the dotted underline on saved words.
- No conic gradients, `mask-image`, `backdrop-filter` or blurred glows anywhere.

## 6. Notes for the builder

- The README says "tap any word to look it up". That is wrong (decision 9). Delete the paragraph.
- Chapter number in the location text is taken from the file order, not the TOC. Fix it.
- Remove the `dict_sense_hint` toggle from the settings UI until the word-sense work lands (see word-sense proposal; not stored in this project), or relabel it then.
- Proofread lives in the deep editor's Tools menu (decision 129); the reader has no entry point for it.
- The in-reader editor (Edit on the selection chip, decision 118) is not drawn on these boards. It still needs its own pass here.
- Add keyboard focus rings everywhere (2px accent, 2px offset). Hover states use `border` colour as a fill.
