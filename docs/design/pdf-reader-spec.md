# PDF reader: design spec (draft)

Source boards: https://claude.ai/artifact/V8ZF1hC4zNnh6ZvPmiDKmE (Terracotta, light and dark). Tokens are from `design-system.md` and `design-system-update.md`. Chrome is inherited from `epub-reader-spec.md` and `comic-reader-spec.md`. Decisions 52 to 76 in `decisions-update.md`.

## 1. Layout

| Surface | Position | Summoned by |
|---|---|---|
| Back pill (+ Bubble peek) | top-left, 16 inset, 44 high | cursor rests in the top-left zone |
| Bookmark ribbon | top-right, 48 from edge | always visible; `B` toggles |
| Location text | bottom-right, 24 / 16 ("12–13 / 612") | always visible |
| Left panel (Contents, Pages, Notes) | left, 16 inset, 340 wide | edge zone, `T`, `P`, `H` |
| Right panel (Reading, Image, Shortcuts) | right, 16 inset, 340 wide | edge zone or `S` |
| Search bar | top centre of the visible page area, 640 x 52 | Ctrl+F |
| Page-jump field | top centre, 480 x 52 | `G` |
| Selection chip | above the selection, 245 x 44 | selecting text |
| Annotation popover | below the highlight, 272 wide | tap a highlight |
| Dictionary popup | below the selection, 380 wide, flips above near the bottom | Define on the chip |
| Annotation toolbar | bottom centre, 48 high | cursor rests at the bottom centre |
| Tool popover | above the toolbar, 10 gap, 240 to 270 wide | picking a tool |

Page area: spread (two pages, 3 gap) or single page, fit to height by default. Panels float over the page. A **pin button** (36 circle, in the panel header beside the tabs) keeps the panel open and moves the page area beside it (decision 57). The ribbon and location text are not shown under a pinned right panel.

## 2. Tokens used

- Colours: `bg`, `text`, `text-2`, `accent`, `accent-fill`, `card`, `border`, `overlay`, `match`, `match-current`. `text-3` is not used.
- Page paper: Terracotta dark Tint `#CFC6BB`, light `#FFFFFF`. Page ink for placeholder text and the selection tint on paper: `#2A2420`. Neutral placeholders for illustrations are the same in every palette.
- Highlight colours (five) plus the ink colour `#2A2420` for pens and text.
- Radii: 6 (thumbnails, key chips), 10 (tiles, fields), 14 (cards, popovers, dialogs), 22 (panels), 999 (pills).
- New component sizes: pin button 36, icon button 32 (36 in the toolbar and tool popovers), tile 68, chip 28, key chip 24, switch 40 x 24.

## 3. Behaviour

**Page style.** Layout (Single, Spread) and Flow (Paged, Scroll) are independent. Spread auto-selects when the window is wide enough. Cover on its own changes the pairing. Gap between pages applies to Scroll. Choice remembered per book.

**Zoom and pan.** As the comic reader: scroll to zoom, drag to pan, double-click toggles fit and 100%, zoom readout only while zooming.

**Image tab.** Off, Tint (default in dark), Invert. Applied at draw time with a colour matrix. Files are never changed.

**Contents.** 44 px rows, indent 16 per level, current chapter 3px accent bar and bold, page label on the right in `text-2`. Filter field (40 high) filters by title and keeps parent headings.

**Pages.** 3 or 2 columns. Click jumps to the page. Lazy thumbnails: skeleton, loaded, failed. Current spread outlined 2px `accent`, 2 offset.

**Edit mode** (pencil button in the Pages header). Click selects, Ctrl and Shift extend, Ctrl+A selects all. Header bar: "N selected", Apply, Rotate left, Rotate right, Extract, Delete, Insert. Drag reorders, with a 3px accent drop bar and a ghost with a count badge. Staged changes, Ctrl+Z undoes them. Esc leaves; if there are staged changes it asks (Discard, Keep editing, Apply). Apply writes the PDF copy and `kalam.json` together. Delete asks only when the pages carry annotations or bookmarks. Rules for page-bound data: decision 69.

**Search.** Enter and Shift+Enter move between matches. The results button opens the list in the left panel (replaces the tabs, close returns). Scope defaults to Chapter. No text layer: "This book has no searchable text".

**Selection chip.** Five colour dots (one tap highlights with the last-used style), Note, Define. Selection tint on paper: ink at 18% in dark, `text` at 14% in light. Handles: 2px accent bar with a round grip, painted by the engine.

**Text-highlight popover.** Colour dots, trash, four style buttons, note text. Autosaves on close.

**Annotation toolbar.** Slide up 220 ms ease-out after a 250 ms hover delay; hide after 300 ms grace (fade with reduced motion). Active tool stays until Esc or Select. Tools and popovers: decision 66. Selecting a drawing, shape or text box opens a small popover with its options and delete. Delete is direct; Ctrl+Z undoes (decision 30).

**Saved words.** Dotted underline (1.5px, accent at 75%, offset 5). 300 ms hover, then a read-only card with all meanings and a tick; flips below near the top.

**Shortcuts.** As decision 24. Placeholder keys: T, P, H for the panels, G for page jump.

## 4. States

Drawn: chrome revealed; Contents, Contents with no outline; Pages, Pages edit mode, dragging, delete confirmation, unsaved changes; Notes (highlights, empty, Words, Words empty, Bookmarks, Bookmarks empty); Reading, Image, Shortcuts (key recording and conflict); selection and chip; text-highlight popover; dictionary popup; saved-word hover card; search (results list, no matches, no searchable text); page-jump field and out of range; continuous scroll of spreads; annotation toolbar and each tool (highlighter, pen, shape, text box, sticky note, eraser, selected drawing); page loading; page couldn't be drawn; scanned page notice; password, wrong password, can't be opened, file not found; control states (idle, hover, focus, selected, disabled).

Not drawn: see "Not designed yet" in `decisions-update.md`.

## 5. GTK widgets and custom drawing

- **Stack:** `Gtk.Overlay` over the page canvas. Panels, Back pill, toolbar and popovers are overlay children or `Gtk.Popover` (no arrow). Slides use `Gtk.Revealer`, not CSS transforms.
- **Page grid and results list:** `Gtk.GridView` and `Gtk.ListView` with lazy loading. The Pages grid needs `Gtk.DragSource` and `Gtk.DropTarget` for reordering; the drop bar and the ghost are drawn by a custom widget.
- **Tabs, tiles, segmented controls:** linked toggle buttons. Switches: `Gtk.Switch`. Sliders: `Gtk.Scale`. Dialogs: a modal window or overlay widget on an `overlay` scrim.
- **Painted by the reader engine (not GTK CSS):** the page surface and colour matrix, selection tint and handles, search match tints, highlight styles, pen and highlighter strokes, shapes, the note pin, the dotted underline on saved words.
- **Text box:** an editable overlay (for example a `Gtk.TextView`) positioned in page coordinates while editing, painted by the engine when not.
- No conic gradients, `mask-image`, `backdrop-filter` or blurred glows anywhere.

## 6. Notes for the builder

- Annotations, strokes, shapes and text boxes are stored in `kalam.json` against a **stable page ID** with geometry in page coordinates. Never store page numbers.
- A page edit rewrites the PDF copy and `kalam.json` in one step. Failure of either must roll back both.
- Delete must also remove the page's annotations and bookmarks, re-point Contents entries to the next page and move the reading position.
- Text boxes use one sans font so they can be baked into a PDF with a standard font later.
- All icons in the boards are placeholder 1.8px stroke icons.
- Add keyboard focus rings everywhere (2px accent, 2px offset). Hover uses `border` colour as a fill. Every icon-only button needs an accessible label and a tooltip.
- Do not design or build tap-to-look-up (decision 9).
