# Kalam design decisions

Newest at the bottom. Add to this file at the end of every design chat. Rules: `HANDOFF.md`.

1. **Theme scope:** light and dark, with 4 palettes. Terracotta and Forest are kept; Ink and Amber are dropped; Plum and Teal are added (Proposed, not yet confirmed; see 21).
2. **Splash screen is final.** It is approved as is (`splash.html`).
3. **Everything else is a draft** unless a later decision says otherwise: Home, My Library, Author, all Settings panes outside the reader, and the navigation rail. The rail will gain more entries later. *Superseded in part by 144 (Settings panes) and 148 (the bell in the rail).*
4. **Contrast:** `text-2` is darkened in the light palettes to reach 4.5:1. `text-3` is decoration and placeholders only.
5. **Scales:** type, radii and spacing are collapsed to the short scales in `design-system.md`.
6. **PDF reader and EPUB reader have different appearance panels.** Design them separately.
7. **Bubbles (split-screen, several books open) gets its own design pass.**
8. **Part 2 (downloads, online sources, plugins) is not built.** Design it last. *Superseded in part by 154 (the Settings Integrations pane).*
9. **From the repo:** tap-a-word-to-open-dictionary is deliberately not a feature. Words are looked up by selecting them. Do not design a tap-to-look-up interaction. (The README still describes tap-to-look-up. That paragraph is wrong and should be deleted.)
10. **Workflow:** one Claude Project, one chat per screen or small group. At the end of each chat, produce an updated `decisions.md` and a spec for the finished screen. *Superseded by decision 103.*
11. **Logo is done.** Traced to SVG, stored as text in `logo-svg.md` (one path, `currentColor`). The loop is a hairline; check it at 32px in the app.
12. **Splash needs a palette update at build time.** Swap Ink and Amber for Plum and Teal in its theme switcher. The design itself does not change.
13. **Light `text-2` values were darkened** to reach 4.5:1: Terracotta 726B66, Forest 627165, Plum 776875, Teal 5F7171. The splash file still has the old values; use the table in `design-system.md`.

## EPUB reader (design chat 2)

Status: boards drawn in Terracotta dark (some also light). Not yet approved as final. See `epub-reader-spec.md`.

14. **The reader is a pure page.** No top bar, no bottom bar, no bottom pill. All chrome is hidden until summoned. Esc still exits the book.
15. **Back pill, top-left, hidden.** It slides down when the cursor rests in the top-left zone (about 250 ms delay, 220 ms slide, ease-out). 150 ms later, if the cursor is still on it, the Bubble button slides out from behind it (200 ms, ease-out). The Bubble pill sits behind Back, shows only its icon, and uses the page colour while Back uses the card colour, so the two read as one pill. On leave, both move at the same time: Bubble slides in (180 ms) while the whole unit slides up (220 ms, ease-in), after a 300 ms grace period. Reduced motion: fade instead. No visible hint for the hidden pill.
16. **Bookmark is a ribbon, top-right.** V-notched ribbon hanging from the top edge. Idle: page colour with a hairline outline. Set: solid accent, a little longer. `B` toggles it. Build it as two symbolic SVG icons (outline and filled), not a custom-drawn widget.
17. **Location text, bottom-right.** No pill. Plain text on the page showing the percentage. On hover it also shows the chapter title. Use `text-2`, never `text-3`. Known bug: the "Ch. N" number comes from the file order, not the table of contents (it said Ch. 3 on the first page). Use the TOC entry.
18. **Removed from the reader chrome:** the bottom pill, the Search, Bookmark and Proofread buttons in the top dock, and the whole "UI" settings pane (the 12 chrome-tuning numbers). The look of the chrome is fixed by the design system.
19. **Reader themes are the four app palettes in light and dark**, replacing Light, Sepia, Dark and Ink.
20. **Left panel: Contents and Notes.** Floats over the page, 340 wide, 16 from the window edge, radius 22. No dim layer. Esc or a click on the page closes it. Head: cover, title, author link, edit pencil (no progress bar). Contents: plain chapter list, indent for nesting, current chapter has an accent bar and bold text. Notes: filter chips All, Highlights, Bookmarks, Words; Highlights adds a colour and style filter row; items are in book order under chapter headings.
21. **Plum and Teal** are used in the reader's theme picker. Treat them as confirmed unless you say otherwise.
22. **Words view in Notes:** a search field (searches saved words) and a Chapter / Book / All books switch. Each row is the headword plus a one-line meaning. Hover shows delete.
23. **Right panel: settings only.** Two panes: Reading and Shortcuts. Reading has a live preview card, then inset cards per section with icons: Theme (mini-page swatches plus Light/Dark/System), Typography, Layout, Navigation, Dictionary. Shortcuts uses the same card treatment. *Superseded in part by 145 (the Theme card).*
24. **Shortcut conflicts:** a key that is already taken is refused, and the message names the action that owns it. No silent swap.
25. **Selection highlight follows the palette.** A neutral tint: `text` at 16% alpha on dark, 14% on light. Handles are the accent colour in both modes. This replaces the fixed rose tint and the yellow dark-mode handles.
26. **Selection chip:** five colour dots (one tap highlights, using the last-used style), a Note button (highlights and opens the note box), Define, Fix typo. Copy is Ctrl+C only.
27. **Highlight colours** (proposed, accepted): `#E8B93A`, `#6FB36F`, `#5B9BD5`, `#D97BA8`, `#E08A4B`. Not palette-dependent.
28. **Annotation styles** (existing in the app): Solid tint, Underline, Squiggly, Strikeout. The extra "Underline" colour is dropped, because the Underline style covers it.
29. **Tapping an existing highlight** opens one popover: colour dots, four style samples, note box, delete. A highlight that carries a note shows a small dot of its colour at the end of the span; hovering the dot shows the note.
30. **Deleting is direct.** No confirm, no toast. Ctrl+Z undoes it. The undo stack holds the last several deletions (highlights, bookmarks, words) for the reader session.
31. **Dictionary popup:** 380 wide. Header: word (Playfair), IPA, speaker button, circular save button. Save button: a ring with a plus when idle; when saved, a filled accent circle with a white tick; clicking again unsaves (Ctrl+Z undoes). Body: part-of-speech dividers, numbered senses with an italic example, 3 senses per group then "Show N more", synonym and antonym chips, idiom cards. Synonym and antonym chips open that word inside the same popup, with a back arrow. No footer. States: loading skeleton, not found with did-you-mean chips.
32. **"Likely here" tag and the "Sense hint" setting are removed from the design.** Context-aware meaning order is a separate proposal (word-sense proposal; not stored in this project). It is not decided.
33. **Saved words** get a dotted underline (about 1.5 px, accent at about 75% alpha) in every book, controlled by the Word memory setting (Library, Series, Book, Off). Hovering one shows a read-only card with all its meanings and a small saved tick.
34. **Search (Ctrl+F):** a floating bar at the top centre: query, "n of N", previous and next, a Chapter / Book switch (Chapter by default), close. The current match has an accent tint and outline; other matches a faint neutral tint. No-match state shows "No matches" in the accent colour and hides the arrows.

## Open questions
- ~~Is the Editor a top-level rail entry, or only reachable from a book?~~ **Resolved in chat 5 (decision 102): both.**
- ~~Proofread had a button in the reader dock and now has no entry point. Is that intended?~~ **Resolved in chat 7 (decision 129): Proofread is an entry in the deep editor's Tools menu; the reader has none.**
- White text on the accent: Terracotta dark is about 4.1:1 (fails 4.5:1). The other dark palettes are not measured. Needs a decision (darker accent for fills, or dark text on accent).
- Reader body line height: boards use 1.75, the settings preview 1.8. Pick the default.
- Word sense (proposal only): see word-sense proposal; not stored in this project.

## Not designed yet in the reader
Continuous-scroll mode, two-page spread, image lightbox, error states (corrupt EPUB), light mode for the panels and settings, Forest/Plum/Teal versions of the boards, phrase breakdown in the dictionary, hover states for settings cards, note hover preview.

Numbering continues from 34. Status: drawn in Terracotta light and dark (Forest, Plum and Teal not checked). **Approved by the user at the end of design chat 3** (palettes other than Terracotta still unchecked). Canvas: https://claude.ai/artifact/STQiE5abQevijypxoRx2S3

## Comic reader and series page (design chat 3)

35. **The comic reader reuses the EPUB reader chrome:** pure page, hidden Back pill with the Bubble peek (top-left), bookmark ribbon (top-right), plain location text bottom-right ("12 / 180", or "3 / 74" in a chapter-relative view). Panels float, 340 wide, 16 from the edge, radius 22, no dim layer. Esc exits.
36. **Comic remaster is out of scope.** It is a Lanczos3 upscaler, not a screen. No upscaler control appears in the reader. Removed from `kalam-screens.md`.
37. **Side-panel tabs use an accent fill for the active tab** (white text), not a card fill. Applies to Chapters/Pages, Reading/Image/Shortcuts. The EPUB panel tabs (Contents/Notes) should match; check those boards.
38. **Left panel: Chapters and Pages.**
    - **Pages** shows every page of the whole series in one scrolling grid (3 columns, thumbnails 112 high), with a sticky chapter heading ("Chapter 2 · 36 pages · reading"). It opens scrolled to the current page. Tapping a thumbnail jumps there, even across chapters. Thumbnails load lazily (windowed grid): skeleton, loaded, failed ("Failed to load"). Current page has a 2px accent outline.
    - **Chapters** has a header (cover 64 x 96, title, author, "N chapters · N pages"), then rows grouped by volume. Current chapter: 3px accent bar and bold. Read: dimmed. In progress: "12 / 36". Missing file: dimmed, "Missing".
39. **Right panel: Reading, Image, Shortcuts.** Reading, top to bottom: "Settings for this series" switch (off = global); Page style tiles (Single, Spread, Long strip); "Gap between pages" (long strip only); Fit tiles (Width, Height, Screen, Original); Direction (LTR/RTL) and Zoom slider; Behaviour (Auto next chapter, Mark read on chapter advance). From Moku's panel, skipped: bar position (no bar in Kalam) and the Fade page style. Saved presets are deferred.
40. **Per-series settings are all-or-nothing** (one switch covers the whole panel). *Proposed; approved with the rest of the chat unless you say otherwise.* *Extended by 147.*
41. **Image tab:** Background colour; Brightness, Contrast, Warmth sliders; Grayscale and Invert switches; Crop blank borders; Reset image settings. Applied when the page is drawn; files are never changed.
42. **Zoom and navigation:** scroll to zoom, double-click toggles fit and 100%, a small zoom readout only while zooming. Page-jump field in the style of the search bar (not drawn). No persistent scrubber.
43. **Page and file states:** loading (skeleton at page ratio), page could not be read (Retry), file not found (Locate file), archive empty or corrupt (Remove from library).
44. **Series page is a full page** (not the "Series float"). The global nav rail exists in the app but is not drawn in these boards.
    - **Header:** a band tinted from the accent (plain linear gradient, no blur), a large cover (200 x 300) overlapping it, kind label ("Manga"), title (Display), original title (italic), credits line with role labels, facts line (year, status, publisher, age rating), genre chips.
    - **Actions:** Continue (primary), Add to shelf, Reading list, star rating, More (⋯: mark all read/unread, edit metadata, refresh metadata, change cover, open folder, remove).
    - **Left column:** summary clamped to four lines with "More"; details grid; Credits and Tags as collapsed rows; source footer ("Metadata from MyAnimeList · updated 3 days ago").
45. **Chapters on the series page have no box.** Plain rows on the page background with hairline dividers and a first-page thumbnail. Search is an underlined field, not a boxed pill. List/Grid switch on the right. Grid is 5 columns of page-shaped cards (read = dimmed with a "Read" badge, current = accent outline and progress line, missing = dashed).
46. **Metadata sources:** MyAnimeList (manga) and Comic Vine (western comics). *Assumed "MangaList" means MyAnimeList; confirm.* The page is source-neutral: manga credits show as Story and Art, western credits as writer, penciller, inker, colorist, letterer. A field edited by hand is marked "· edited" and is never overwritten by a refresh.
47. **Files-only state:** title, a "Find metadata" button, and only what the file provides (format, pages, size). Every metadata tier disappears cleanly when empty.
48. **The Find metadata dialog, the fetching state and the failed-fetch state are out of this chat.** They get their own chat, shared by EPUBs and comics. Carry over the staged review with per-field accept and locked hand-edited fields.
49. **Contrast resolved with an `accent-fill` token.** White text sits only on `accent-fill`, never on plain `accent`. Light modes already pass (accent-fill = accent). Dark modes use a slightly darker fill: Terracotta dark C54F27 (4.6:1, was 4.1:1), Forest dark 498051, Plum dark AB598F, Teal dark 318080. `accent` itself is unchanged for text, rings, bars and outlines. Applied to the active tab, segmented control, switch (on), Continue button and the nav item.
50. **Newly drawn (comics):** two-page spread; Shortcuts pane with a key being recorded and a conflict message ("Space is already used by Next page", per decision 24); page-jump field (top centre, Page/Chapter switch, out-of-range state "Pages 1–180"); Credits and Tags expanded on the series page (role names, names as accent links to a library search); Related series (cards with a relation label: Sequel, Prequel, Side story, Spin-off; a series not in the library is dashed); a Control states board (idle, hover, focus, selected, disabled). *Superseded in part by 116 (credit names open the Author page, not a library search).*
51. **Control states:** hover = `border` colour as fill (primary buttons: a 10% white overlay on the fill); focus = 2px `accent` ring, 2 offset; disabled = 40% opacity; selected as per component. Switch hover = a 5px `border` halo.

## Open questions (added)
- Per-series settings: all-or-nothing (drawn) or per group?
- Confirm the manga source is MyAnimeList.
- The header tint is the accent, not the cover. A tint derived from the cover colour would need a computed token.

## Not designed yet (comics)
Saved presets, Series float, the metadata fetching, failed and no-match states (own chat), Forest/Plum/Teal versions, and the other palettes' accent-fill values in the real boards (table in `design-system.md`).

Numbering continues from 51. Status: drawn in Terracotta light and dark (Forest, Plum and Teal not checked). Boards: https://claude.ai/artifact/V8ZF1hC4zNnh6ZvPmiDKmE. The user confirmed the boards during the chat; items marked *Proposed* were not explicitly questioned.

## PDF reader (design chat 4)

Audience: picture-heavy books such as encyclopedias and textbooks. The page is treated as an image first, text second.

52. **Chrome is inherited** from the EPUB and comic readers: pure page, hidden Back pill with Bubble peek, bookmark ribbon (top-right), plain location text ("12–13 / 612"), floating panels 340 wide. The Back pill is **not** drawn on boards where the left panel is open, because it overlapped the panel. It is drawn (without the Bubble peek) on the whole-book error screens.
53. **Page style has two independent axes:** Layout (Single, Spread) and Flow (Paged, Scroll). Spread plus Scroll is a column of two-page spreads (the user's way of reading). Default: Spread when the window is wide enough, fit to height. "Cover on its own" decides how pages pair. "Gap between pages" applies to Scroll. In Scroll with Spread, fit is to the width of the spread. The choice is remembered per book (no visible switch).
54. **Right panel: Reading, Image, Shortcuts.** Reading: Page style tiles (Single, Spread), Flow (Paged / Scroll), Gap, Cover on its own; Fit tiles (Width, Height, Screen, Original); Zoom slider; Rotate left and right. No Direction (RTL) setting for PDFs (*Proposed*; add it if RTL books matter). Image: Page colour (Off / Tint / Invert), Tint strength, Brightness, Contrast, Warmth, Grayscale, Trim margins, Reset image settings. Applied at draw time; the file is never changed by these.
55. **Dark mode keeps the page pale.** Default is **Tint**: white paper is dimmed toward the palette (Terracotta dark paper `#CFC6BB`). **Invert** is available and captioned "works best on text-only pages" because it turns photos into negatives.
56. **Selection tint on a PDF page comes from the page ink, not from `text`.** The decision 25 tint (`text` at 16%) is invisible on pale paper in dark mode. On PDFs: dark mode uses ink `#2A2420` at about 18%; light mode keeps `text` at 14%. Handles stay `accent`. *Proposed; applies to design-system.md.*
57. **Panels float over the page by default** (decisions 20 and 35). Each panel has a **pin button** in its header row, next to the tabs. Pinned: the panel stays open and the page area sits beside it instead of under it. The individual panel boards show the pinned layout. The bookmark ribbon and location text are not drawn under the right panel. Whether the pin state is remembered per side is an assumption (*Proposed*).
58. **Contents:** filter field (titles only; Ctrl+F searches the text), collapsible levels with the current branch open, page labels on the right, current chapter with accent bar. No outline: "No contents in this book" with an **Open Pages** button.
59. **Pages:** thumbnail grid with a column toggle (3 or 2, icon buttons), sticky heading, current spread outlined (both pages), skeleton and failed thumbnails, bottom fade. Opens scrolled to the current page.
60. **Notes:** chips All, Highlights, Bookmarks, Words. Highlights: colour dots and a filter button; items grouped by chapter with colour bar, italic excerpt, page number and note; area highlights show a small thumbnail. Words: search, Chapter / Book / All books switch, headword and one-line meaning. Bookmarks: accent ribbon, page, title, chapter. Empty states drawn for Highlights, Words and Bookmarks.
61. **Search (Ctrl+F):** top bar centred over the visible page area (640 x 52): query, "n of N", previous, next, results-list button, Chapter / Book switch, close. The **results list** replaces the tabs in the left panel: query and count, grouped by chapter, page number and snippet, match in bold with `match` tint, current result with `match-current` and an accent bar. Results load lazily. States: **No matches**; **No searchable text** (scanned books, no OCR, switch dimmed).
62. **Selection chip is icon-only:** five colour dots, divider, Note, Define. Copy is Ctrl+C. **No "Fix typo"** on PDFs (text cannot be edited), and Proofread is not needed for PDFs (confirmed). Lookup is still by selecting text (decision 9).
63. **Text-highlight popover (tap an existing highlight):** colour dots with a trash icon, four icon buttons for Solid, Underline, Squiggly, Strikeout (selected = accent fill), a hairline, then the note as plain text. Autosaves on close.
64. **Dictionary popup follows decision 31.** For words with no real synonyms the chip group is labelled "Related" (*Proposed*).
65. **Saved words:** dotted underline in the page; a read-only card above the word (flips below near the top edge) with all meanings and the saved tick.
66. **Annotation toolbar (new):** bottom-centre floating pill, 48 high, hidden until the cursor rests there. Seven icon-only tools: Select, Highlighter, Pen, Eraser, Shape, Text box, Sticky note. The active tool has the accent fill and stays active until Esc or Select. Each tool opens a popover above the toolbar:
    - Highlighter: colour, opacity (40, 60, 80%).
    - Pen: colour, three thicknesses.
    - Shape: Rectangle, Ellipse, Line, Arrow; Outline or Filled; colour; thickness. A filled translucent rectangle **replaces the earlier "area highlight" idea** (no Shift+drag).
    - Text box: size (S, M, L), colour, Paper background switch. One sans font only (so it can be baked into a PDF with a standard font). Click to place, double-click to edit, Esc to finish.
    - Sticky note: pin on the page with a small card (colours, delete, text).
    - Eraser: removes a whole stroke or object on touch.
    - Selecting a drawing opens a small popover (colour, thickness, delete).
    Skipped on purpose: callouts, stamps, signatures, form filling, layers.
67. **Ink colour:** a sixth colour, `#2A2420`, next to the five highlight colours. Fixed in every palette. *Proposed.*
68. **Page editing happens in Edit mode** in the Pages panel (pencil button in the header). Clicking a thumbnail selects it (Ctrl and Shift extend, Ctrl+A selects all). Selected pages have an accent outline and a tick badge; there are no checkboxes. The header becomes a bar: "N selected", **Apply**, and icon-only Rotate left, Rotate right, Extract, Delete, plus Insert. Drag to reorder (accent drop bar, dimmed originals, ghost with a count). Edits are staged; Ctrl+Z undoes them until applied.
69. **Rules for page-bound data** (highlights, notes, bookmarks, strokes, shapes, text boxes, area highlights, Contents entries, reading position):
    - Reorder: everything follows its page.
    - Rotate: drawings and shapes rotate with the page.
    - Delete: the page's annotations and bookmarks are deleted with it. If the deleted pages carry any, a prompt shows the counts ("Delete 2 pages? ... 3 highlights and 1 bookmark"). Plain pages need no prompt.
    - A Contents entry that pointed at a deleted page re-points to the next remaining page.
    - Reading position moves to the nearest remaining page.
    - Saved words belong to the book and are unaffected.
    - Annotations are keyed to a **stable page ID**, not a page number.
70. **Edits change Kalam's copy only** (the book is copied into the app's data folder), never the user's original. Pressing Apply rewrites the PDF copy **and** `kalam.json` in one step so they cannot disagree. Leaving Edit mode with staged changes asks first (Discard, Keep editing, Apply). Open views (Bubbles, library page counts) refresh after an edit.
71. **Extract** saves the selected pages as a new PDF outside the library, without the user's annotations (for now).
72. **Annotation storage:** `kalam.json` now. Objects are stored with the page ID and geometry in page coordinates, so they follow zoom, rotation and reordering. A later feature, **"Save highlights into PDF"** (book ⋯ menu), bakes them in as standard PDF annotations (highlight, ink, square, circle, line, free text, text). Not designed. Extract could then offer "Include my highlights".
73. **Errors and states:** password protected (Unlock), wrong password (accent message), can't be opened (Remove from library), file not found (Locate file), page couldn't be drawn (Retry, inside the spread), page loading (skeleton), scanned page (dismissible pill at the bottom). Uses the comic state card (decision 43). Kalam does not store the password (confirmed; an option to remember it may be added later).
74. **Page-jump field** (top centre, 480 x 52): Go to, number, "of N", Page / Chapter switch, close. Out of range: number and hint turn accent ("Pages 1–612").
75. **Dialogs:** 420 wide, radius 14, `card` fill, 1px `border`, on an `overlay` scrim (page `bg` at 85%). Buttons: ghost for the safe choice, accent fill for the main one. *Proposed new component.*
76. **Shortcuts pane** follows decision 24 (a taken key is refused, the message names the action). Panel keys T (Contents), P (Pages), H (Notes) are placeholders so they agree with the EPUB reader.

## Open questions (added)
- **Delete the old "Area highlight" board** from the canvas (replaced by the Shape tool). It still exists.
- **Pin button:** is the pinned state remembered per side or per panel? Drawn as one pin per panel header.
- **Page labels in the location text** ("iv · 8 / 612"): specified, not drawn.
- **RTL PDFs:** no Direction setting. Add if needed.
- **Bubbles** with PDFs in a narrow window: still needs its own pass (decision 7).
- Stable page IDs: confirm `kalam.json` can carry them.

## Not designed yet (PDF reader)
Ellipse, line and unfilled shapes on a page; selected text box popover; toolbar slide-in; hover cards for saved words on scanned pages; empty Pages state; Forest, Plum and Teal versions; paper tint values for the other palettes; "Save highlights into PDF"; Insert pages dialog (system file chooser); the Bubbles pass.

---

# Book details page and series page (design chat 5)

Boards (Terracotta light and dark; Forest, Plum and Teal not checked):
- Book details page: https://claude.ai/artifact/VxJw6sDJFH4kqpEEAg1cfW
- Series page: https://claude.ai/artifact/FJeo3h751XuLvXvSmt32Ha

Status: **approved by the user at the end of design chat 5** (book details page and series page). Items marked *Proposed* were not explicitly questioned.

## Book details page (EPUBs and PDFs; comics keep their series page)

77. **Full page, no tint.** One page for any EPUB or PDF. The accent band from the comics series page is **not used**: a cover of any colour clashes with it. The hero sits on plain `bg`. The existing 720 x 420 Book float is unchanged and not redesigned here; clicking its cover opens this page.
78. **No cards.** The old page's card grid is gone. Sections sit on the page with hairlines. Two sections are outlined boxes (1px `border`, radius 14, padding 20, no fill change): **Contents** and **Your impression**.
79. **Content moved or dropped from the old page:**
    - Reading stats, history and the 7-day timeline are **removed completely**.
    - The author card, the "Also in your library" shelf and the details grid (ISBN, publisher, language) are **removed**. Facts sit in one line under the title; the rest lives in the Edit metadata dialog. *Proposed.*
    - Book file becomes one footer line: file name, size, added date, "Show in file manager". The hash is not shown. *Proposed.*
    - The source credit footer ("Metadata from Open Library") is not on the page. *Proposed; confirm where it lives (see open questions).*
80. **Hero.** Cover 200 x 300, radius 10, with a **cover pencil** (36 circle, bottom-right, 8 inset) that opens a popover: Choose image…, Find online, Restore original. Under the cover: a 3px progress bar the width of the cover, "42%" and "Ch. 9 / 31", and the start date. Finished: a full bar, "Finished" and the date. Unread: "Not started".
81. **Header lines.** Kind label (accent), Display title, series link, credits, facts line, rating, tags, description (3 lines and "More"), actions.
    - **Credits** show role labels (Author, Translator) only when there are two or more roles. With one role it is just the name as a link. Empty roles never show.
    - **Series link** reads "Series · Book N of M ›". N and M come from the metadata **even if you own only one book**. If M is unknown: "Book N". A decimal N is shown as is ("Book 2.5"). It opens the full series page (decision 94). The Series float is retired.
82. **Rating** sits on its own row, under the facts line and above the tags. Half-star steps. Hover previews the half star; clicking the current value clears it. Empty: hollow stars and "Not rated yet". It is the same value as the impression rating (decision 91).
83. **Tags.** Chips, then a "+" chip that opens the Tag dialog. Genres and tags are one thing (the code has only tags).
84. **Tag dialog.** 420 wide, instant apply, Ctrl+Z undoes.
    - Add-a-tag field with autocomplete from the library (book counts) and a Create "…" row.
    - "On this book": chips with a remove ×.
    - "Suggested from the file": one-tap + chips (shown only when the file or metadata provides subjects).
    - "Your library": all tags with a tick for those on this book, book count and a ⋯ per row. Sort: Most used or A–Z.
    - Row ⋯: Rename everywhere, Merge into…, Delete from library. Library-wide actions confirm with counts ("It will be removed from 38 books. The books themselves are not changed."), as decision 69.
    - No tag colours (would need new tokens).
85. **Actions row.** Read (primary, `accent-fill`), Finished toggle, Reading list, pencil (Edit metadata, opens the shared dialog from the metadata chat), ⋯.
    - **Finished toggle:** Mark finished → "Finished ✓" (card fill, accent tick). Pressing it again marks unread. When finished, the primary reads "Read again".
    - Shelves is **not** in the row (decision 86).
86. **⋯ menu** (240 wide): Edit book…, Book edits (accent badge with the pending count), **Shelves ›**, Show in file manager, a divider, Remove from library (accent).
    - **Shelves ›** opens a submenu: a tick circle per shelf, then a divider and "New shelf…".
    - **Edit book…** opens the book in the editor. For PDFs it is "Edit pages" and opens the reader's Pages edit mode (decision 68). *Proposed.*
    - **Edit metadata is not in the menu** (the pencil does it). **Refresh metadata is removed**: re-fetching belongs to the Find metadata dialog (metadata chat), which protects hand-edited fields.
87. **Next in series.** When a book is finished, hovering or focusing the Finished button slides **"Next: <title> ›"** out from behind it, the same way the reader's Bubble pill does (decision 15). Finished has the `card` fill; the pill uses `bg` with a hairline border so they read as one shape. The buttons after it **slide aside** (they are not covered). The pill takes the reader's timings (short hover delay, 200 ms ease-out slide, 300 ms grace on leave). Next not in the library: the pill is disabled and reads "Next: <title> · Not in library", with no download prompt (downloads are Part 2). Last book of a series: no pill. *Narrow windows: see open questions.*
88. **Section layout.** Below the hero, two columns:
    - **Left:** Contents (outlined box, level with the top of the right column).
    - **Right:** Your impression (outlined box, only when the book has an impression), then Annotations.
    - Then the file line. An unfinished book has no impression box, so Annotations starts at the top of the right column.
89. **Contents is a timeline.** A line runs down the left edge with a 24 node per chapter. Read chapter: `accent-fill` circle with a white tick. Current chapter: an **eye icon** in an accent ring, bold title. Upcoming: a hollow circle. The line is accent up to the current chapter and `border` colour after it. Rows are 64 high: "Chapter N" in Meta above the title. Right side: the **completion date** for read chapters, the percentage (accent) for the current one, nothing for upcoming ones.
    - **Bookmarks** show as one bookmark icon in the row; with several: the icon and "×2" or "×3". An indicator only, not clickable.
    - The inline list shows 5 rows and "Show all N".
    - **Show all** opens in place: same `bg`, same outline, **fixed height (8 rows)**, scrolls inside, **no search field**, a "Show less" button at the bottom. It covers what is below it; nothing moves.
90. **No visible scrollbars anywhere in the app.** In GTK, every `Gtk.ScrolledWindow` uses the external scrollbar policy. A short fade at the bottom edge hints that a list continues.
91. **Impression.** A private note after finishing a book.
    - **Triggers:** Mark finished on the page, or reaching the end of an **EPUB** in the reader. PDFs only via Mark finished (the last page of an encyclopedia means nothing).
    - **Dialog** (420 wide): cover, "You finished", title, "How was it?", half-star rating, "It felt…" with seven fixed chips (Page-turner, Thought-provoking, Beautiful writing, Slow burn, Heartbreaking, Funny, Stayed with me; multi-select), "A few words for future you" (private note), the finished date (defaults to today; Change date), ghost **Not now** and accent **Save impression**. Esc = Not now.
    - **Skipping** is final: no nagging. A finished book with no impression shows a dashed "Your impression" prompt with "Add your impression".
    - **On the page:** an outlined "Your impression" box: stars, "4.5 · Finished 12 Sep 2026", chips, the note in Quote style, a pencil that reopens the same dialog.
    - **Setting:** "Ask for an impression when I finish a book" (Settings, Reading). *Proposed.*
    - One impression per book. Re-reads are not covered (see open questions).
92. **Annotations** on the page: two highlights (colour bar, Quote style at 16, chapter and note in Meta) and one count line ("14 highlights · 3 bookmarks · 23 saved words"), with "See all 40" in the heading. "See all" opens a **floating dialog**, 560 wide (a new dialog size, for lists), with tabs **Highlights / Bookmarks / Words** and a count on each (active tab `accent-fill`, as decision 37):
    - Highlights: five colour dots as a filter, items grouped under chapter headings.
    - Bookmarks: chapter and date, one row per bookmark.
    - Words: headword (Playfair 600, 16) and a one-line meaning.
93. **States** (Terracotta, dark and light):
    - **No cover:** a neutral block (`card` fill, 1px `border`, Label kind, Playfair title, author in Meta). No accent. This resolves the "Cover fallback" TBD and also serves library cards and panel headers.
    - **File missing:** cover dimmed to 45%, a dashed "File not found" card replaces the progress, the primary becomes **Locate file**, the other actions stay.
    - **Files only (PDF), unread:** title from the file name, "PDF · 612 pages · 48 MB", empty stars, an "Add tags" chip, "Not started", and **Find metadata** next to Read. Empty sections are not drawn (decision 47). This also stands in for the Unread state of any book.
    - **Control states** drawn for the primary, ghost, Finished toggle, icon, chip, tab, tag row, star rating, menu item and chapter row (decision 51 rules).

## Series page (any series; the comics page from decision 44 is kept aside)

94. **A full page** opened from the series link (decision 81). The Series float is **retired**. The comics series page (decision 44) is unchanged and not redrawn; whether it adopts this layout is open.
95. **Layout, top to bottom:** Back pill; a compact header; a "Books" row with the Strip / List switch; the strip; the focused-book row; a source footer line. No band, no tint, no drop shadows or glows.
    - **Header:** Series label (accent), Display title, the author link with the facts line beside it, the segmented progress bar (decision 98), a progress line ("0 of 2 books read · You own 2 of 4"), and a two-line summary. **Continue** (primary) and ⋯ sit at the top right.
    - **Continue label:** "Continue Book N", "Start Book N", or "Read Book 1 again" when every owned book is read.
96. **The strip.** Covers in order, left-aligned at the page margin, bottom-aligned, gap 28. The **focused** book is 260 x 390 with a bold title; the others are 180 x 270. Title and year sit under each cover. A book in progress has a 3px progress bar under its cover; a finished book has a 22 `accent-fill` tick badge at the cover's top right. **There is no underline under the focused title** (it read as a second progress bar). Missing books keep their place as dashed outlines (1.5px `text-2`, radius 10) with the title and "Not in library".
    - A long series continues past the right edge and scrolls to keep the focused book in view. *Proposed; the strip scrolled is not drawn.*
    - **Default focus:** the next book you have not finished and own; for a finished series, the last book.
97. **Strip interactions.** **Clicking the focused cover opens that book's details page.** Clicking another cover moves the focus to it. The arrows (beside the counter) move the focus, not the scroll position. Left and Right keys move the focus, Enter opens the focused book, Esc goes back. A missing book can take focus but cannot open; its info row reads "Not in library" (*not drawn*). There is **no "Open book" button**.
98. **Segmented progress.** One segment per book, 220 wide in total: read = accent, in progress = partly accent, unread = `border`, missing = dashed. It is deliberately short.
99. **Focused-book row** under the strip (hairline above): "Book N · year", title (Playfair 500, 22), the state (progress bar with "42% · Chapter 9 of 31", "Finished 3 Oct", or "Unread"), the description, and the two arrows with a counter ("1 / 4").
100. **List view** (Strip / List switch, top right): plain rows on the page, 64 high, hairline dividers, no boxes. Thumbnail 36 x 50 (radius 4), "Book N", title (700 for the current book, 600 for unread, regular for read) with the year, then the state on the right (tick and "Read", a 100 x 3 progress bar with the percentage, nothing for unread, "Not in library" for missing; missing rows at 55% with a dashed thumbnail). The current book has a 3px accent bar 14 to the left. Meant for long series (the user's longest has 15 books). The choice is remembered per series. *Proposed.*
101. **Series position is always taken from the metadata** (see decision 81).

## Editor
102. **The Editor is a top-level rail entry and is also reachable from a book** (Book ⋯ → Edit book…). This closes the open question from design chat 1.

## Process
103. **Handoff system.** The rules for files, numbering, chat start and end, and the check script live in `HANDOFF.md`. Every chat ends by handing back full replacement files built from the project copies (never partial snippets, never update files), after the check script reports no errors. This replaces the workflow in decision 10.

## Open questions (added)
- **EPUB reader boards are not changed.** Things that now differ from them: the Notes panel's chips versus the Annotations dialog's tabs (decision 92); the Contents panel versus the timeline (decision 89); the impression dialog over the real last page (decision 91); the new Reading setting (decision 91); no visible scrollbars (decision 90). Decide whether to bring the reader in line.
- Where does the metadata source credit and "last updated" live now that the page footer dropped it? (The Author page keeps it in its footer, decision 110; the book page is still open.)
- What does the description's **More** link do (expand inline or open a popover)? Not drawn.
- The action row on **narrow windows**: while Next is open the row is wider. Wrap to a second line (my suggestion) or close the pill.
- **Re-reads:** one impression per book for now. Do re-reads keep several?
- The comics series page: keep its accent band and chapter list, or adopt this layout with chapters as rows? The band clashes with covers in the same way.
- The Book float (720 x 420) is not redesigned and may now disagree with the page.
- **Tag dialog:** the autocomplete dropdown and the Merge picker are not drawn.
- Reaction chips are a fixed list of seven. Make them editable?
- "Not in library" focus state in the series info row and the strip scrolled for long series are not drawn.
- "Quotes" saved in the old page are treated as highlights with a note. Confirm.

## Not designed yet (this chat)
Forest, Plum and Teal versions of both screens. A light version of the Menus board (cover popover, ⋯ menu, Shelves submenu, Next-in-series states, skipped-impression prompt). Empty states for Annotations. The Edit-impression state (it reuses the dialog). Hover cards for tags.

Numbering continues from 103.

---

# Author page (design chat 6)

Boards (Terracotta light and dark; Forest, Plum and Teal not checked): https://claude.ai/artifact/AJvyQ17bqLL6bcFcqTcEkQ

Status: **approved by the user at the end of design chat 6.** Items marked *Proposed* were not explicitly questioned. See `author-page-spec.md`.

104. **Full page, plain `bg`.** The Author page replaces the old draft. No band, tint, drop shadow, glow or blur (decisions 77 and 94; GTK cannot blur). It is reached from author links (book page credits, series page author link, comics credits) and, later, from the Authors index (decision 117).
105. **Header.** Left: "Author" label (accent), the name (Display), a bio (Meta, 3 lines, "More"), then two ghost pills, **Create shelf** and **Find metadata**. Right: an **outlined facts panel**, 340 wide (design system section 25): a 64 circle portrait with the lifespan beside it, then rows with hairlines: **Works** ("33 · 13 in your library"), **Series** (hidden when the author has none), **You've read** ("8 of 13" with a segmented bar). The ⋯ button (44 circle) sits top-right, level with the Back pill. The portrait fallback is neutral (`card` fill, 1px `border`, Playfair initials), as in decision 93. Reading stats beyond "You've read X of Y" are **not shown** (rating, pages, since-year), in line with decision 79. No author quote in the header (the highlights row covers it). Bio, birthplace, nationality and website are not promised: only what the metadata source provides appears (*Proposed*).
106. **The page shows the author's wider bibliography with ownership marked.** Works are grouped under two Label headings, **In library** (any book owned) and **Not in library** (none owned). Within each, **Standalone works is always the first card**, then series in series order. There are no All / In library chips and no Cover grid / List switch: the two headings do that job. Missing works are never mixed into a row without being marked (decisions 20 and 96).
107. **Series card (collapsed)** (design system section 26). Two columns, gap 16, 124 high. Left: title (Card title), a segmented progress bar (one segment per work: read `accent`, owned and unread `border`, missing dashed), and a Meta line ("2 of 4 owned · 1 read · 1998–2011"). Right: a cover fan of three covers and a chevron. The fan is **not rotated** (GTK cannot rotate widgets in CSS): covers overlap with a 22 offset. Standalone works is a card of the same kind. Hover and focus follow decision 51.
108. **Open series panel** (section 27). Clicking a card opens a panel directly under it, **over the content below** (nothing moves), as the Contents overlay does (decision 89). Header row: "<Series> · N books" in Meta and an accent **Open series ›** link to the series page (decision 94). Below: the covers at the series strip's unfocused size (180 x 270, radius 10, gap 20) in a row that scrolls sideways with no visible scrollbar (decision 90). Owned books show title and year, a finished tick badge or a 3px progress bar (decision 96); missing books are dashed. Esc or clicking the card again closes it. *Whether several panels can be open at once is not decided; the boards show one.*
109. **Fetch button on missing works** (section 28). A 36 circle "+" button sits inside the dashed cover under "Not in library", with an accessible label ("Fetch <title>"). It **fetches the book itself** (user ruling), not its metadata. This needs online sources, which are Part 2 (decision 8): until then the button is disabled with a tooltip. Fetching, progress and failed states are **not designed**. Extends decision 96.
110. **Bibliography is fetched only through Find metadata and cached locally.** Opening the page never touches the network, so it opens instantly and works offline. "Updated 3 days ago" is plain information, with no nag and no auto-refresh. The footer line reads "Bibliography from Open Library · updated 3 days ago" (11px, `text-2`). Hand-edited fields stay locked (decision 48). Owned books added after the last fetch still appear: in their series row if the file metadata names a series, otherwise under Standalone works, without missing neighbours until the next fetch. Duplicate editions and translations collapse into one work (the matcher's job). Answers the source-credit question for this page only.
111. **No bibliography yet** is one quiet state (replaces the dashed card first drawn in this chat): a single Meta line under a hairline, "No bibliography yet. Find metadata to see this author's other works. Your own books are shown below.", with "Find metadata" as an accent link. It covers a new author, a failed fetch and an offline first run. The owned books are still grouped as usual; the Not in library group, the "N of M" counts and the Series row are left out, and Works reads "3 in your library". Files-only authors (no bio, initials only, "No details yet" in the panel) and a loading skeleton are drawn.
112. **⋯ menu** (a no-arrow popover, 240 wide, design system section 17): Edit author…, Create shelf… (reads **Open shelf** when one already exists), a divider, Find metadata for N books…, Add all to reading list, Show books in library (runs the author search), Export highlights… (Markdown, this author only, as the existing export), a divider, Merge into another author…. There is no Refresh item (decision 86). *Remove all books from library* is deliberately left out: it would need a confirmation with counts (decision 69); add it later if wanted. **Find metadata for N books** is only an entry point; the dialog belongs to the metadata chat (decision 48). The Edit author and Merge dialogs are not designed.
113. **Create shelf dialog** (section 31): 420 wide, a form dialog (decisions 75 and 18). A Name field with a switch, **"Name the shelf after the author"**, on by default: the field is disabled while it is on, and the shelf is renamed if the author is renamed. A **Smart / Manual** switch (Smart by default): Smart saves the rule "author is <name>" so new books join automatically, with a live count ("13 books match now"); Manual adds the current books once. Buttons: ghost Cancel, `accent-fill` Create shelf. The Manual type is not drawn. *Proposed.*
114. **Your highlights** (section 29): a **sideways-sliding row of cards** with a "See all 23" link and two 36 circle arrow buttons in the heading (left disabled at the start). Each card is 420 wide, `bg` fill, 1px `border`, radius 14. A header (book title in Card title style, a 10 dot in the highlight colour, a Meta line "Chapter 4 · Saved 14 Mar 2026") is separated by a **hairline** from the body: a 72 x 108 cover and the quote (Quote style, clamped to four lines). The row bleeds off the right edge, scrolls with no visible scrollbar, and is hidden when the author has no highlights. **Clicking a card opens the book at that highlight** (user ruling). Replaces the three layouts tried earlier in this chat.
115. **"See all" opens the Annotations dialog** (560 wide, decision 92) at author scope: tabs Highlights / Bookmarks / Words with counts; Highlights has an All chip and the five colour dots, and items are grouped under **book** headings (Label style) with the chapter and date in each item's Meta line. This replaces the chapter grouping of decision 92 for the author scope only. Bookmarks and Words tabs are not drawn.
116. **Comics credits open the Author page.** Supersedes part of decision 50: names in the comics series page's Credits no longer run a library search. A name is a link only when that person has an Author page in the library; otherwise it is plain text (*Proposed*). The book page already worked this way (decision 79 and the book-details spec).
117. **Authors index page** (new): a top-level page listing every author in the library, and an **Authors** entry in the navigation rail. Not designed; added to the checklist (user ruling). The rail stays a draft (decision 3).

## Open questions (added)
- The **Authors index page** and its rail entry need their own chat (decision 117).
- Where does the author portrait come from (file, metadata source, user image)? It is not designed; Edit author is the likely place.
- The **Edit author** and **Merge into another author** dialogs are not designed. Edit author must lock hand-edited fields (decision 48).
- Can several series panels be open at once (decision 108)?
- Segmented bars on long standalone lists (14 or more segments) become thin; check at the real width.
- What does **Create shelf** do when a shelf with that name already exists (rename, or open it)?
- The **+ fetch button** needs its states once Part 2 exists (decision 109).

## Not designed yet (Author page)
Forest, Plum and Teal versions; narrow windows (facts panel beside the bio, the highlights row); Edit author and Merge dialogs; the Manual type in Create shelf; Bookmarks and Words tabs and the empty and filtered states of the author Annotations dialog; the Open shelf menu label; the + fetch states; the Authors index.

Numbering continues from 117.

---

# Deep editor (design chat 7)

Boards (Terracotta light and dark; Forest, Plum and Teal not drawn): https://claude.ai/artifact/2nMT5SDCKGPKjBNPs3YKBe

Status: **approved by the user at the end of design chat 7** (the user asked for the handoff after the last review). Items marked *Proposed* were not explicitly questioned. The in-reader editor is not designed in this chat. See `editor-spec.md`.

118. **Two editors (user ruling).** A *deep editor* (a full page, this chat) and an *in-reader editor* (small changes inside the EPUB reader). In-reader: select text, choose Edit on the selection chip, a cursor appears in place and nothing else changes, so it still feels like a book. A small floating format bar near the cursor (Bold, Italic, Link, paragraph style) writes real markup so no tags are typed. The format bar and the chip's "Edit" replacing "Fix typo" (decision 26) are *Proposed*. **Not designed here:** it is bundled with the EPUB reader boards in a later chat. Typing reflows paginated text, so the reader engine must hold the reading position.
119. **EPUB only.** Comics and PDFs have no deep editor. PDF page editing stays in the reader (decisions 68 to 72). *Proposed.*
120. **Markdown is dropped (user ruling).** The deep editor edits HTML, CSS and XML only. A guarded Markdown mode (live preview, per-file format switch, an Editor setting) was discussed and removed, because a round trip can lose classes, ids, `epub:type` and layout. It could return later as an optional "write in Markdown" mode.
121. **Workspace layout.** A full page with five parts: a top bar (56 high), a hideable file tree on the left (240 wide), the editing area in the middle, a right panel (440 wide) and a status bar (32 high). A 1px `border` line separates the top bar from the content (user ruling). The top bar holds the Back pill (boxed, `card` fill, as on every other page), the book name with "Editing Kalam's copy · original untouched", then Tools, History and Save. Save is the only filled button.
122. **Code / Split / Preview switch lives in the bottom bar (user ruling),** at the right end of the status bar, as small underlined text. Split is the default. Code gives the editor the full width, Preview gives the preview the full width. The status bar also shows the cursor position, the file type, the problem count and "Snapshot kept before each save".
123. **Right panel tabs: Preview, Outline, Problems, Live CSS.** The tab row is 48 high with a 1px `border` line under it, and the active tab's 2px accent underline sits on that line (user ruling). **Preview:** its menu offers "As the book ships" (default, tick) or "As in Kalam" (the reader theme applied); the arrow shows only on the active Preview tab (user ruling). **Outline:** the headings of the open file, indented, with the heading under the cursor marked by the accent bar; clicking one jumps there (*Proposed*). Clicking in the preview jumps to that spot in the source and the reverse (*Proposed*; needs the preview engine to report the clicked element).
124. **Flat editor chrome (user ruling: "too many boxes").** In the editor, tabs and switches are plain text with a 2px accent underline on the active one (no pill container, no filled tab), and ghost buttons have no border. Only the Back pill (boxed) and the main action (`accent-fill`) are boxed or filled. Dialogs keep the standard bordered ghost pill (decision 75). This replaces the pill tabs of decision 37 inside the deep editor only. Design system section 33.
125. **File tree.** Sections Text, Styles, Images and Fonts, and Contents. Each section header has a chevron and collapses on click (user ruling); the state is remembered per book. The open file has the 3px accent bar; a changed file has an accent dot. Rows are 36 high (a new size, *Proposed*: the 44 rows of other panels are too tall for a tree). A right-click menu (*Proposed*): Rename, Delete, Merge selected, Add files, Export file, Replace from file, Export for editing, Open in external app, Mark as cover, Set semantics, Link stylesheets.
126. **Files open in tabs (user ruling).** A single-click in the tree opens a temporary tab (italic title) that the next single-click replaces; a double-click, or typing, pins it. Each tab has its own undo history and remembers its cursor. Changed tabs show an accent dot; × closes. Images open in an image tab (preview, size, "used in N files", Replace image, Compress, Show where used) and fonts in a specimen tab (name, three sizes, where used, Subset font, Remove). The image and font tabs are *Proposed*.
127. **Source editor.** `GtkSourceView` with a custom style scheme built from tokens only (*Proposed*): tags in `accent`, attribute names and text in `text`, attribute values and comments in `text-2`, comments italic. Line numbers in `text-2` at 70%. The current line uses `border` as a fill; the cursor is a 2px `accent` bar. A monospace font is needed and the design system has none: bundle one (DM Mono suggested; *Proposed*), 13 / 22. No visible scrollbars (decision 90). A **formatting toolbar** above the tabs (Bold, Italic, Link, Heading, List) writes real tags (`strong`, `em`) into the source and toggles them off when pressed again; it is kept (user ruling). There is no Underline button, because underlined text reads as a link in an e-reader (*Proposed*).
128. **Tools run in three ways (user ruling).** Instant with an Undo message (Smarten punctuation, Reformat code, Remove unused CSS): they first ask "This file or Whole book?" in a 420 dialog and afterwards show a pill ("12 rules removed from style.css · Undo"). A dialog or panel for tools that need choices. A tab for tools with something to show (Check book opens Problems; Edit contents, Reports). Every tool takes a snapshot first (decision 135). Tools that need a page or dialog get one.
129. **Tools menu and command palette (*Proposed*).** The menu (240 wide, design system section 17) has direct items (Check book with a count badge, Find & replace, Edit contents, Spell check, Reports, Proofread, Command palette) and four submenus: **Insert** (Link, Image, Character, Inline contents page), **Structure** (Split, Merge, Rename, Reorder files, Set semantics, Link stylesheets), **Clean up** (Reformat code, Smarten punctuation, Remove unused CSS, Fix HTML, Filter style information, Tag mapper, Upgrade book internals), **Fonts and images** (Subset fonts, Embed fonts, Compress images, Replace cover, Check external links). The command palette (Ctrl+Shift+P, 640 wide) reaches every tool by typing and shows shortcuts as key chips. **Proofread** lives here, which resolves the open question from chat 2. What Proofread does is not specified (see open questions).
130. **Calibre coverage (user ruling).** The deep editor copies what Calibre's editor can do, not how it looks. All tools of the first and second waves are designed in this chat (decisions 128 to 141). Skipped on purpose (user ruling): Calibre's function-mode searches (replacements written as Python) and its built-in image editor. Kalam's route for images is Export for editing (decision 140).
131. **Find and replace** (*Proposed*) is a floating panel, 440 wide: scope tabs (This file, All text files, Whole book), Find and Replace fields, chips Match case, Whole word and Regex, a match count, results grouped by file with the match tinted, a Saved searches link, Replace one and Replace all (filled). Replace all applies at once; the snapshot and Undo cover mistakes.
132. **Problems and Check book** (*Proposed*). Rows are 56 high: a marker (filled dot for an error, hollow for a warning, so they differ by shape), the problem, the file and line, and a small button (Fix, Go to, Remove). Fix **previews the exact line change** (before and after, using the diff tints) with Skip and Apply fix; applying is one undo step. A problem with no safe fix only offers Go to. Errors and Warnings are grouped. Saving with errors asks first (decision 143).
133. **Live CSS (user ruling).** The fourth right-panel tab. For the element under the cursor it lists every CSS rule that applies, with file and line; overridden properties are struck through; clicking a rule jumps to it. It only has content in HTML files.
134. **History and Compare** (*Proposed*). History is a 560 dialog listing snapshots (Before save, Before Replace all, Original file) with time and the number of files changed, and Compare and Restore this version. Restore takes a snapshot first. **Compare** is a side-by-side view in the editing area with a header (Back to editor, Restore this version). Removed lines use the `match` tint; added lines use a new `diff-add` tint (`accent` at 20%). 
135. **Snapshots.** A snapshot is kept before every save, before every tool that changes the book and before every restore or import. Edits change Kalam's copy only, as for PDF page edits (decision 70); the original is backed up per the Library setting. *Proposed.*
136. **Edit contents (user ruling, approved).** The book's navigation (`toc.ncx` and `nav.xhtml`) is edited in a tab in the workspace, with the same chrome as any file. The centre is a compact list of entries (36 high rows, parts in medium weight, chapters indented, target file on the right, accent bar on the selected entry). Tab and Shift+Tab indent and outdent, drag reorders, double-click renames in place. Buttons in the formatting-toolbar position: "+ Add entry" and "Generate from headings". The right panel is the Preview: the target heading has an accent outline and a click moves it. It saves with the book through Save (no separate Save and Cancel). It opens from Tools, the command palette or a double-click on the navigation file. It replaces the small dialog and the full-page version drawn earlier in the chat. This is different from the inline contents page (decision 138).
137. **Spell check and personal dictionary** (*Proposed*). Spell check (F7) is a 420 dialog: the sentence with the word dotted-underlined in `accent`, suggestions, and Add to dictionary, Ignore, Change. The Personal dictionary is a 420 dialog listing added words, each removable.
138. **Insert dialogs** (*Proposed*), all 420 wide: **Insert character** (name search, a glyph grid, the selected character's name and code, Close and Insert); **Insert link** (switch In this book / Web address, searchable files and headings); **Insert image** (images from the book as thumbnails, Add from file, alt text); **Insert inline contents page** (insert after, Parts and chapters or Chapters only, add to the reading order).
139. **Structure tools** (*Proposed*): **Split** is a 420 dialog with At the cursor, At every heading and By size (max size field and "This creates 4 files"); links are fixed automatically. **Reorder files** is a 560 dialog with the reading order, drag handles and a 3px accent drop bar. **Replace cover** is a 420 dialog with the current cover, Choose image and Replace (disabled until an image is chosen).
140. **Export for editing (user ruling).** Any file, mostly images, can be exported from the file tree's right-click menu. Kalam saves a copy to an editing folder (`~/Kalam/Editing/`, *Proposed* path), shows a 420 dialog (Show in folder, Open in app, Import changes), watches the file and reports "Changed 2 min ago". Import changes brings it back after a snapshot. Replace from file is the manual alternative.
141. **Second-wave tools (user ruling, drawn as dialogs, *Proposed* layouts):** **Reports** (560 to 640, tabs Files, Words, Images, Links, CSS, Fonts; table with sortable columns; unused images are flagged); **Subset fonts** (before and after size per font); **Embed fonts** (fonts the CSS uses that are not in the book; the ones missing on this computer are marked; a licence reminder); **Compress images** (Lossless or Smaller, estimated saving); **Check external links** (560; only runs when started, needs a network, shows progress and then Works, Not found, Moved; opening a book never touches the network); **Tag mapper** (rules such as `b` to `strong`, count of elements and files affected, Add rule).
142. **Editor picker page** (*Proposed*). The rail's Editor entry opens a plain page: "Editor" label, Display title "Edit a book", an underlined search field, then a list of EPUBs with **Edits waiting** first and All EPUBs after. Rows are 72 high with a cover fallback (40 x 56, radius 4), title, author and a state on the right (an accent chip "3 edits waiting", "Not edited" or "Edited 2 Oct"). Opening a row opens the workspace. "Edit book…" on a book's ⋯ menu goes straight to the workspace (decision 86). Empty state: "No books to edit".
143. **Save and error states** (*Proposed*). **Unsaved changes** (420 dialog): "Save changes to 3 files?", Discard, Keep editing, Save. **Save with errors**: "2 errors found" with the first error, Save anyway (ghost) and Show problems (filled). A book that cannot be opened shows the dashed state card "This EPUB can't be opened. The file is damaged. Kalam has not changed it." with Show in file manager.

## Open questions (added)
- **In-reader editor** is not designed (decision 118): the Edit button on the chip, the floating format bar, reflow while typing, and how edits are saved. It belongs with the EPUB reader boards.
- What does the **Book edits** badge in the book page ⋯ menu (decision 86) count? Assumed: queued typo fixes from the reader. Not confirmed, and the queue is not designed.
- What does **Proofread** do in the Tools menu? It is listed (decision 129) but has no screen.
- Is `~/Kalam/Editing/` the right export folder, and is it cleaned up after Import changes?
- Can the preview engine report the clicked element for click-to-jump and for Edit contents targets (decisions 123 and 136)?
- Bundle a monospace font: which one (decision 127)?
- Reports, Find and replace, Check book: narrow windows are not designed. The right panel may need to be wider than 440 for four tabs.
- Find and replace saved searches have a link but no screen.

## Not designed yet (deep editor)
The in-reader editor; Forest, Plum and Teal versions; the Reports tabs other than Images; the Saved searches screen; Find and replace no-match and regex-error states; loading skeleton for the workspace; DRM or read-only EPUB; save failed; Edit contents empty state; Compare across several files; narrow windows; the editor's keyboard shortcut map; a light version of the Tools menu with submenus beyond Clean up; hover cards for Problems.

Numbering continues from 143.

---

# Settings (design chat 8)

Boards (Terracotta light and dark; Forest, Plum and Teal not drawn): https://claude.ai/artifact/82NsKxiZver3BBCiADgzen

Status: **approved by the user at the end of design chat 8** (the user asked for the handoff after the last review). Items marked *Proposed* were not explicitly questioned. See `settings-spec.md`. The facts about today's panes come from `settings.rs`, which the user shared in the chat; the README's "13 dark themes" is stale (decisions 1 and 19 stand).

144. **Settings is a full page.** Plain `bg`, no band, tint or glow. A pane list on the left (240 wide) and the active pane in one 640 wide column (new size, *Proposed*). The list has Label-style group headings and flat 13px items; the active item is bold `text` with a 3px `accent` bar (the flat style of decision 124, with a bar because the list is vertical). Groups and panes: **Appearance** (Appearance); **Reading** (Reading); **Library** (Libraries and storage, Data and export, Book files, Watch folders, Dictionaries); **Sources** (Metadata sources, Integrations); **App** (Shortcuts, About). This follows the groups in `settings.rs`, with Storage & Backup split into Libraries and storage and Data and export, Watch folders split out of Book Files, and Notifications removed (decision 148). Reading, Watch folders, Integrations, Shortcuts and About are new panes. Replaces the five draft panes of the old mockup and supersedes part of decision 3.
145. **Appearance is one choice of palette and mode (user ruling).** There is no Light / Dark / System switch. Four outlined boxes in a 2 x 2 grid, one per palette (Terracotta, Forest, Plum, Teal), each holding a Light card and a Dark card side by side. Picking a card sets palette and mode together. The app does not follow the operating system's light or dark setting. The selected card has a 2px accent ring and an 18 tick badge, so selected differs from focused (*Proposed*). Supersedes part of decision 23 (the reader's Theme card with Light / Dark / System): the reader follows the app theme unless a book overrides it (147).
146. **Appearance also holds Text and motion** (*Proposed*; the options were chosen from what GTK exposes and what the design allows). **Interface font** (a dropdown: DM Sans by default, System font; any other family must be bundled), **Heading font** (Playfair Display by default, or Same as interface), **Text size** (Small, Default, Large, about 90, 100 and 115%; it scales every size of the type table in the interface and never the book), **Reduce motion** (System by default, On, Off; On turns slides into fades). Left out on purpose: accent colour (one per palette), density (44 click targets) and the book typeface (Reading).
147. **Reading pane holds global defaults; every format can override per book (user rulings).** Meta line: "Defaults for every book. A book can still keep its own." Sections: a live preview card, Typography (Typeface, Size, Line height), Layout (Column width, Alignment), Behaviour (Ask for an impression when I finish a book, decision 91; Word memory Library / Series / Book / Off, decision 33). **Reader side, not drawn (*Proposed*):** each reader's Reading pane gets one switch, "Settings for this book" ("this series" for comics); off means the book follows the defaults; editing a value while it is off turns it on. A ghost pill **Use these settings as my defaults** at the bottom of the pane copies only what that pane shows, never across formats; it is disabled and reads "Matches your defaults" when nothing differs. **Reset to my defaults** is the same as turning the switch off. Both confirm with the after-tool pill and Undo (decision 39). Extends decision 40 (all-or-nothing per book or series, now in every format). The user ruled on the defaults, the "use as my defaults" action and per-book overrides in every format; the switch wording and the pill are *Proposed*.
148. **Notification history is a bell in the rail (user ruling: it must be easy to reach).** The bell is a 44 circle at the bottom of the rail, above Settings, with an 8 `accent` dot when something is new; opening the panel clears the dot. The panel floats beside the rail (340 wide, 16 inset, radius 22, `card` fill, no dim layer; Esc or a click outside closes it, decisions 20 and 35): heading "Notifications", an accent "Clear history" link, events newest first under Today and Earlier. A row has an 8 marker (accent for a problem, `text-2` otherwise), the message and a Meta time. Empty: "Nothing yet. Imports, backups and problems show up here." (no Clear history). The readers have no rail and no bell (decision 14). Removes the Activity pane from Settings. The rail stays a draft (decision 3); its icons in the boards are placeholders. The dot behaviour and the panel details are *Proposed*.
149. **Libraries and storage, Data and export, Book files.** *Libraries:* a row per library (name, path, book count); the open one has an accent "Open" chip, the others Open and Forget; Add library. A Meta line says settings and dictionaries are shared by every library. Opening another library asks first in a 420 dialog ("Open Work? Kalam restarts to open this library. Your books are not changed.", ghost Cancel, filled "Restart and open"). A library whose folder is gone shows "Folder not found" in accent and **Locate folder** in place of Open. *Recovery:* "308 of 312 books have a backup copy" with Write missing copies. *Backup and cache:* Back up library… and Reader cache (size) with Clear. *Data and export:* four read-only locations (library folder, catalog database, dictionaries, app settings), each with Show, then Export all data… and Export quotes… (Markdown); Export dialogs are not drawn. *Book files:* Clean EPUBs on import (sanitizer), Write edits into the EPUB (writeback), Original backups (count and size) with Delete backups…. Names, paths and numbers in the boards are invented.
150. **Destructive actions in Settings** (*Proposed*). Permanent ones confirm in a 420 dialog with counts (decision 69; ghost Cancel, filled `accent-fill` action): **Delete backups** ("42 files (1.2 GB) will be deleted. The books in your library are not changed. This cannot be undone.") and **Remove dictionary** (says Kalam will not add the pack back unless it is imported again). Forget library, Remove watch folder, Clear cache and Clear history stay direct: they only edit a list or remove rebuildable data.
151. **Watch folders is its own pane.** One section per folder: the path with a Remove ghost button, then a shelf dropdown each for Books (EPUB), Documents (PDF) and Comics, a **Shelves from subfolders** switch and a **Delete the original after import** switch (off by default). Add watch folder below. Empty: a dashed state card "No watch folders" with Add watch folder.
152. **Dictionaries.** A priority list: drag handle, name, Meta count, an accent "Speaks first" chip on the top row, Remove on each row; Import dictionary and Rescan below; a Meta line says the first dictionary that has a word answers and that the list is shared by every library. Drag handles replace the code's up and down buttons (*Proposed*); while dragging, the dragged row is at 40% opacity and a 3px `accent` drop bar shows the target (decision 68). Empty: a dashed card "No dictionaries" with **Import dictionary** and **Restore bundled packs** (user ruling; the code remembers removed bundled packs, so this is the way back; it restores all of them at once).
153. **Metadata sources.** *Books:* Open Library (switch, "No key needed") and Google Books (switch with a chevron). With Google Books on, **API key** and **Country code** ("Used to rank results") sit under it as ordinary rows indented by 20 in the same section, with no box and no line between them (user ruling). A rejected key: accent border on the field and "Google Books rejected this key." *Comics:* MyAnimeList and Comic Vine as disabled switches with a "Not available yet" chip (not built). A Meta line says nothing is fetched when a page opens (decision 110).
154. **Integrations is designed now, as a Part 2 pane (user ruling).** Supersedes part of decision 8: only this Settings pane is designed early; the Downloads hub, Browse and plugin management stay for later. Sections: **Content sources** (AO3, Royal Road, MangaDex: disabled switch and a "Not available yet" chip), **Plugins** (an empty row "No plugins installed" and a disabled Add plugin), **Downloads** (download folder with Change, Downloads at once). Everything is disabled until Part 2 exists. Sign-in, when it exists, shows "Signed in as …" and keeps secrets in the system keyring (not drawn).
155. **Shortcuts pane (user approved the list and that it is rebindable).** Four outlined groups with 40 high rows and key chips. *Navigation:* Go to Home Alt+1, Go to Library Alt+2, Open Settings Ctrl+, , Back Alt+Left, Notification history Ctrl+Shift+N. *Library:* Search the library Ctrl+F (inside a book it stays Find in book), Import EPUB Ctrl+I, Import from a folder Ctrl+Shift+I. *Window:* Full screen F11, Show shortcuts Ctrl+/, Quit Ctrl+Q. *Everywhere:* Undo the last delete Ctrl+Z, Close the open panel Esc. Rebinding uses the reader's rules (decision 24): the row shows an accent "Press a key…" chip, a taken key is refused and an accent line names the action that owns it. Placeholder keys until built, like decision 76. The agreed proposal also had Alt+3 for the Editor; it is missing from the drawn list (open question). The reader's groups follow below in the real pane and the cheat-sheet overlay (Ctrl+/) is a read-only view of the same list: neither is drawn.
156. **About.** The logo in `accent` (from `logo-svg.md`), the name in Display, a Meta line with the version, then Licence, Source code (accent link) and Dictionary credits with Show licences. The version, licence and link are placeholders to confirm (*Proposed*).
157. **Dropdown (new component; design system section 43).** A closed field is 36 high with a 1px `border`, radius 10, a `bg` fill and a chevron. Open: an accent border and a no-arrow popover in the menu style (section 17), 220 wide, right-aligned, rows 40 high, an accent tick on the current choice, the hovered row filled with `border`; typeface options are shown in their own font; a shelf list ends with a divider and an accent "New shelf…". Long lists scroll without a visible bar and fade at the bottom (decision 90). Built from `Gtk.DropDown`.
158. **Control states for the Settings controls** (decision 51 rules). Drawn for the ghost and primary buttons, dropdown field, switch, key chip, palette card, pane list item and segmented option in idle, hover, focus, selected and disabled. A selected palette card gets the tick badge of decision 145.

## Open questions (added)
- The **reader's Theme card** (decision 23) should become the same palette-and-mode grid as Appearance (decision 145). The reader boards are not redrawn.
- The **reader-side controls** of decision 147 (the "Settings for this book" switch, Use as my defaults, Reset) are not drawn. They belong with the EPUB reader boards and the Reading panes of the PDF and comic readers.
- **Alt+3 for the Editor** is missing from the drawn shortcut list (decision 155). Add it, or drop it?
- Confirm the **Text size** percentages (decision 146), the 640 column and the 220 dropdown width as new sizes.
- **Restore bundled packs** restores all packs at once. Is a per-pack restore wanted?
- Where does the **Google Books API key** live: `prefs.json` or the keyring (decision 153)?
- Do **Content sources** and **Plugins** merge once sources become plugins (decision 154)?
- Should **Remove watch folder** and **Clear history** ask first (decision 150)? And does Forget library need an Undo?
- The **bell** dot: should a problem set it more strongly than an info event? Does a toast link to the panel?
- Version, licence and links in About are placeholders (decision 156).
- The rail still needs its own pass (icons, the Authors entry of decision 117).

## Not designed yet (Settings)
Forest, Plum and Teal versions; the reader-side controls of decision 147; the reader groups in Shortcuts and the cheat-sheet overlay; the Export dialogs; the file and folder choosers (Add library, Add watch folder, Import dictionary); progress states (backup, rescan, export); Add library and first-run flows; sign-in, a filled Plugins list and a downloading state in Integrations; a key-check state in Metadata sources; narrow windows; the Settings list with a collapsed Google Books; the Authors entry in the rail.

Numbering continues from 158.
