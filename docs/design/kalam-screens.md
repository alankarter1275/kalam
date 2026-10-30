# Kalam: screen & surface inventory

A checklist of everything that needs designing, from full pages down to right-click menus.

**How to read this**
- `[x]` = designed and approved (Terracotta light and dark; Forest, Plum and Teal still to check). `[~]` = partly designed; the note says what is missing. `[ ]` = still to do. Last updated after design chats 9 and 10 (the work plan for chats 9 to 19 is below).
- `[ ]` = still to do. Items tagged **draft** have an incomplete mockup in the uploaded files (Home and Library in `kalam-mockup.html`); the rest have none.
- **(R)** = confirmed in the repo README. **(?)** = my inference; please confirm or strike.
- Sources: the README's feature list and `pages/` listing, plus your four mockups. I couldn't open `src/` or `docs/offline-roadmap.md`, so this was not checked against the code. Run `ls src/pages src/widgets` and diff it against this list.
- Part 2 (online sources, downloads, plugins) is listed last and is not built yet.

---

## Work plan (design chats 9 to 19)

Chats 1 to 8 are done. The remaining work is grouped as below. Each chat makes its screen group as one canvas in light and dark, with empty, loading and error states and control states, and ends by handing back full replacement files. **Status** is updated at each handoff: `todo`, `active`, `done`. Chat numbers match the design chat numbering used in `decisions.md` headings. The grouping is the user's ruling; the "Also added" and "Open questions inherited" columns are my additions (*Proposed*).

| Chat | Group | Covers (checklist sections) | Also added (*Proposed*) | Open questions inherited | Status |
|---|---|---|---|---|---|
| 9 | App shell and Home | 1 (rail with icons, bell, active and hover states; window chrome and title bar; toasts; page transitions; focus trap in dialogs; global empty, loading and error patterns, defined once for reuse), 2 (Home in full, with its empty-library state), and the Home search (Library and AO3). **First-run is not designed** (user ruling) and has no chat yet | Toast that links to the bell panel (done, decision 184) | none | done |
| 10 | Library browsing | 3 (My Library hub; All books grid and list; windowed loading and cover skeletons; library-wide search results, the full search page opened by clicking "N results" in the Home search bar (decision 197), and the search-syntax help), 8 (Sort dropdown, Filter menu, Book card right-click menu), 9 (narrow and wide behaviour for the grid) | none | none | done |
| 11 | Shelves | 3 (Shelves overview and detail; Manual shelf editor with reorder and add-books picker; Smart shelf editor with rule dropdowns), 8 (Shelf card right-click menu, Smart-shelf rule dropdowns), 7 (Rename dialog, shared with tags, including the tag rename from the Tag dialog row menu) | none | 8 | todo |
| 12 | Lists, history and tags | 3 (Reading list and its bulk picker; History; Tags cloud and Tag detail), 8 (Tag right-click menu), 7 (Bulk tag edit dialog) | Could merge with chat 11; split only for the 16-file canvas limit | none | todo |
| 13 | Insights and vocabulary | 3 (Analytics; Reading goal and streak, now off Home (decision 185); Saved quotes; Saved words; Vocabulary review; Lookup history), 7 (Yearly goal dialog; the three Export dialogs: quotes to Markdown, words to CSV and Anki, all data) | none | none | todo |
| 14 | Metadata and people | 4 (Find metadata dialog with fetching, failed, no-match and staged-review states, decision 48; Metadata editor; Edit author and Merge into another author; Authors index page; remaining author Annotations tabs and states; Book float redone for the new book page) | Where the metadata credit lives on the book page; re-reads | 1, 4, 6, 7 | todo |
| 15 | EPUB reader pass | 4 (In-reader editor, decision 118), 5 (Continuous-scroll mode), 6 (Reader-side Reading controls, decision 147; reader groups in Shortcuts), 7 (Ctrl+/ cheat-sheet overlay), 8 (reader right-click menus for selection, highlight and blank area; Annotation row menu); Reader Theme card as the Appearance picker; Notes and Contents panels in line with the book page | Editor shortcut map (gap found: no chat owned it); Book edits badge and Proofread; editor export folder check against the code | 2, 9, 10, 11, 12, 13 | todo |
| 16 | Bubbles, OCR and comics reconciliation | 5 (Bubbles; scanned-PDF OCR flow), 4 (comics series page versus the new series layout), 6 (the "Settings for this series" controls in the comic and PDF readers) | none | 3 | todo |
| 17 | System dialogs and background work | 9 (progress and cancellable background tasks: backup, rescan, export, import), 7 (remaining confirm dialogs: delete book, shelf, highlight; About dialog; file and folder choosers as a spec only), 6 (Add library and Add watch folder flows as spec) | Native choosers: agreed, see note below | 14 (restore bundled packs, Google Books key location) | todo |
| 18 | Part 2 (online sources) | 10 (Downloads hub; Browse client for AO3 and Royal Road; MangaDex browsing; Plugin management; fetch states for the Author page "+" button; sign-in states in Integrations; search bars for Fanfiction.net, Royal Road and MangaDex, following the AO3 template of decisions 202 to 205; the AO3 work page and Browse page) | none | 15 | todo |
| 19 | Palette pass | Forest, Plum and Teal for every screen; paper values, `accent-fill`, `diff-add` and highlight contrast checks across all eight modes; fixing whatever breaks | Narrow-window pass for pages drawn only at full width (book page action row while Next is open, Settings, editor right panel, Author header) | 5 | todo |
| 20 | Taste map | A graph of books joined by author, series, shelf and tag, mainly eye candy, gently moving and not static, on its own page (decision 206) | Reconcile with My Library's reading timeline (decision 174) | none | todo |

**Notes on the plan**
- **Chat count:** the plan lists ten design chats (9 to 18) plus the palette pass (19).
- **File and folder choosers (chat 17):** I agree they should be the native chooser and not drawn. GTK 4.10 deprecated `Gtk.FileChooserNative` in favour of `Gtk.FileDialog`, which also uses the system portal. The spec should name `Gtk.FileDialog` for new code and keep `Gtk.FileChooserNative` as the fallback if the project targets an older GTK. Kalam only specifies the title, the filters (EPUB, PDF, CBZ and so on; StarDict, SQLite and TSV for dictionaries) and the folder-only mode.
- **Chats 11 and 12** could be one chat; they are split for the 16-file canvas limit.
- **Dependencies:** chat 9 defines the global empty, loading and error patterns and the rail; chats 10 to 18 reuse them rather than redraw them. Chat 15 depends on decisions 145 and 147. Chat 19 runs last because it needs every other screen.
- **Nothing is ticked below by this plan.** Items are ticked only when the user approves them in their chat.

---

## 1. App shell & global

- [x] Splash screen (R)  ← approved, good to go
- [ ] First-run / library check: choose a library folder when none exists (R). **Not designed (user ruling, chat 9): the user will do it later.** Ideas discussed: the user's name, the library folder, the look, a few options (decision 207).
- [x] Navigation rail: Home, My Library, Shelves, Comics, Editor centred; logo at the top; bell and Settings at the bottom; inset rounded divider; stencil active item (chat 9, decisions 179, 180; `home-spec.md`). The Authors entry was dropped. Icons are a Lucide-style proposal. More entries will be added later. The rail state and tooltip board was not reviewed one by one.
- [~] Window chrome and title bar: no title bar, three window controls at the top right (decision 181). Drawn on the Shell board, not reviewed; check against the code.
- [~] Toast notifications: the after-tool pill with a link (decision 184). Drawn, not reviewed.
- [x] Notification history: a bell in the rail opens a floating panel; events and empty state drawn (chat 8, decision 148; `settings-spec.md`)
- [~] Page transition behaviour: a 180 ms fade (decision 182). A spec note, nothing to draw.
- [~] Global empty state, loading state, and error state patterns: dashed state cards and skeletons on the Shell patterns board (dark and light) and in the Home states (decision 194). Drawn, not reviewed.
- [~] Keyboard focus ring and focus trap in dialogs: the rules are in decision 183 and the standard dialog is on the Shell board. Drawn, not reviewed.

## 2. Home

- [x] Home: greeting, saved quote, search, Continue reading (the open book), Your shelf with its timeline and zoom levels, Updates (chat 9, decisions 185 to 195; `home-spec.md`). Removed from the old draft: shelves row, Fresh pages, Up next, stats, streak, goal, Add books.
- ~~Home: counts strip and "Up next" peek~~ dropped (decision 185)
- [~] Home states: empty library, loading, error, nothing in progress, notification panel open (decisions 194, 195). Drawn in dark and light, not reviewed one by one.
- [~] Shelf states: Arrange by list, Arranged by colour, empty shelf, one month filtered (decision 193). Drawn, not reviewed.

## 3. Library

- ~~[ ] My Library hub with quick links (R) **draft**~~ replaced by the My Library page below (chat 10, decision 173)
- [x] My Library: one long page with a masthead, a reading timeline, Recently added beside the saved-words sphere, Shelves, Reading list, Series, Highlights and Authors; empty and loading states (chat 10, decisions 173 to 176; `library-spec.md`)
- [x] Impressions page behind "All impressions ›" in the timeline card (chat 10, decision 177)
- [x] All books grid: title dropdown to shelves, Add books, search, Filter, Group, Sort, Select, cover cards, bulk bar, drop overlay, shelf view (chat 10, decisions 159 to 168; `library-spec.md`)
- [x] All books: list view with configurable columns, drag and resize (chat 10, decisions 171, 172)
- [x] Windowed grid loading behaviour: cover skeletons and a bottom fade (chat 10, decision 170)
- [x] Library-wide full-text search results (FTS5), with no-matches and indexing states (chat 10, decision 169)
- [x] Search syntax help: `tag:`, `author:`, `status:`, `rating:` (chat 10, decision 169)
- [x] Home search, Library: the underlined bar with the option picker, history, results, loading, no matches, the Inside books switch (chat 9, decisions 196 to 201; `home-spec.md`). Library page search (decision 169) and Home search differ on book-text results; see open questions.
- [x] Home search, online (AO3): title bar, In / Sort / Filters pills, tag field and pills, merged facet lists, results, rate limited, no results, cannot reach (chat 9, decisions 202 to 205). Disabled until Part 2; the user said it looked good for now and will adjust it as it is used.

### Shelves
- [ ] Shelves overview (manual + smart shelf cards, 2-column grid) (R)
- [ ] Shelf detail (R)
- [ ] Manual shelf editor: add books picker, "Manage shelf order" with up/down reorder (R)
- [ ] Smart shelf editor: rule builder, live match count, All/Any switch (R)
  - fields: tag, author, series, format, progress, title, added
  - operators: is, is not, contains, date windows

### Lists, history, stats
- [ ] Reading list: ordered TBR with up/down reorder (R)
- [ ] Reading list: bulk picker (R)
- [ ] History: grouped by day, filterable (R)
- [ ] Tags: usage-weighted tag cloud (R)
- [ ] Tag detail: grid of books with that tag (R)
- [ ] Taste map: a graph of books joined by author, series, shelf and tag, mainly eye candy, gently moving and not static, on its own page (decision 206). Own chat.
- [ ] Analytics: counts, time read (7d/30d/all), streaks, 14-day bar chart, books-per-month, most read, top tags and authors (R)
- [ ] Reading goal and streak display (R)

### Quotes & vocabulary
- [ ] Saved quotes list, with Markdown export (R)
- [ ] Saved words list (R)
- [ ] Vocabulary review: known / to-review filters, CSV and Anki export (R)
- [ ] Lookup history (dictionary lookups, misses included) (R)

## 4. Book, author, series & editing

- [x] Book page: rating, reading list, mark finished, edit metadata, tags, annotations, contents, impression (chat 5; `book-details-spec.md`). The shelf checklist is now a submenu in the ⋯ menu (decision 86).
- [x] Book page dialogs and menus: Tag dialog, Annotations dialog, Impression dialog, Contents overlay, cover popover, ⋯ menu (chat 5)
- [ ] Book float / detail panel (Suwayomi-style) (R). Not redesigned; the existing 720 x 420 float stays and may now disagree with the new page.
- [x] Author page: header with facts panel, In library and Not in library series cards with the cover fan and open panel, + fetch on missing works, highlights row, quiet no-bibliography state, files-only, one-book and loading states (chat 6, decisions 104 to 116; `author-page-spec.md`)
- [x] Author page menu and dialogs: ⋯ menu, Create shelf dialog, Annotations dialog at author scope (chat 6, decisions 112, 113, 115)
- [x] Editor menus and dialogs: Tools menu with submenus, command palette, Find and replace, Spell check, Insert character, link, image and inline contents page, Split, Reorder files, Replace cover, Export for editing, Reports, Subset and Embed fonts, Compress images, Check external links, Tag mapper, Personal dictionary, History, Compare, unsaved-changes and save-with-errors dialogs (chat 7)
- [ ] Authors index page: a top-level page listing every author in the library. The Authors rail entry was dropped (decision 179); the entry point is open (decision 117)
- [ ] Edit author dialog and Merge into another author dialog (decision 112). Edit author must lock hand-edited fields (decision 48).
- [~] Author Annotations dialog: the Highlights tab is drawn; Bookmarks, Words, empty and filtered states are not
- [x] ~~Series float~~ replaced by the full **Series page** (chat 5, decision 94; `series-page-spec.md`): strip, books missing, finished, list view, control states.
- [ ] Metadata editor: title, authors, series, tags, description, replace cover (R). The book page pencil and the cover pencil open it.
- [ ] Find metadata dialog and its fetching, failed and no-match states: **own chat**, shared by EPUBs and comics. Sources: Open Library, MyAnimeList, Comic Vine. Carry over the staged review with per-field accept and locked hand-edited fields.
- [x] Editor picker, the rail's Editor entry (R). Decided (decision 102): top-level rail entry, also reachable from a book (Edit book… in the ⋯ menu). Drawn in chat 7 (decision 142; `editor-spec.md`).
- [x] Deep editor (full EPUB editor): workspace, file tree, tabs, source editor with formatting toolbar, Preview / Outline / Problems / Live CSS, History and Compare, Find and replace, Edit contents, all Calibre-style tools and dialogs (chat 7, decisions 118 to 143; `editor-spec.md`). Markdown was dropped (decision 120).
- [ ] In-reader editor: Edit on the selection chip, a cursor in place, a floating format bar (decision 118). **Own design pass with the EPUB reader boards.**
- [ ] Editor Settings or shortcuts: no Editor settings pane is needed yet; the editor's shortcut map is not drawn.
- ~~Comic remaster dialog~~ removed: it is an upscaler, not a screen (decision 36).

## 5. Readers

### EPUB reader
- [x] Reader main view: paged mode (R)
- [ ] Reader main view: continuous-scroll mode (R). Not designed.
- [x] Reader chrome: a pure page with a hidden Back pill, bookmark ribbon and location text (decisions 14 to 18; the top bar, bottom bar and annotations pill were removed)
- [x] Table of contents panel (the Contents tab of the left panel)
- [x] Reader appearance: the right panel with Reading and Shortcuts (decision 23)
- [x] Text selection chip: five highlight colours, Note, Define, Fix typo; copy is Ctrl+C (decision 26)
- [x] Dictionary popup (decision 31; the likely-sense hint was removed, decision 32)
- [x] Find in chapter / in-reader search (decision 34)
- [x] Annotations list for the current book (the Notes tab of the left panel)
- [ ] Bubbles: several books open at once, split-screen reading (R); **gets its own design pass**

### PDF reader
- [x] PDF reader with the unified reader chrome (decisions 52 to 76)
- [x] PDF smart crop controls (Trim margins in the Image tab)
- [~] Scanned-PDF OCR flow: the scanned-page notice is drawn; the OCR flow itself is not

### Comic reader
- [x] Comic reader (image pager)
- [x] Comic series and chapters view (chat 3; kept aside, to reconcile with the new series page)

## 6. Settings

Full page with a pane list and one 640 wide column (chat 8, decisions 144 to 158; `settings-spec.md`). The readers' own settings panes (Reading, Image, Shortcuts) are done inside each reader's right panel.

- [x] Settings: Appearance, one choice of palette and mode, plus Text and motion (decisions 145, 146)
- [~] Settings: Reading, global defaults with a preview (decision 147). The Settings pane is drawn; the reader-side "Settings for this book" switch, "Use these settings as my defaults" and "Reset to my defaults" are not.
- [x] Settings: Libraries and storage (libraries, recovery, backup and cache; restart prompt; folder missing) (decision 149)
- [x] Settings: Data and export (locations and the two export buttons) (decision 149). The Export dialogs are not drawn.
- [x] Settings: Book files (sanitizer, writeback, original backups) (decisions 149, 150)
- [x] Settings: Watch folders, with the shelf dropdown and the empty state (decision 151)
- [x] Settings: Dictionaries, priority list with drag, Import, Rescan, empty state and Restore bundled packs (decision 152)
- [x] Settings: Metadata sources (decision 153)
- [x] Settings: Integrations, a Part 2 pane with every row disabled (decision 154)
- [~] Settings: Shortcuts, app-wide groups with rebinding and conflicts (decision 155). The reader's groups below them are not drawn.
- [x] Settings: About (decision 156). Version, licence and links are placeholders.
- [ ] Settings: storage and libraries first-run flow, Add library and Add watch folder choosers (see section 7)
- [ ] Settings: your name, shown in the Home greeting ("Harvey" is a placeholder, decision 185), and where the Home quotes come from (decision 187)
- ~~Settings: Library, Storage / Libraries, watch folders, data paths~~ replaced by the panes above.
- ~~Settings: Notifications / Activity pane~~ replaced by the bell and panel in the rail (decision 148).

## 7. Dialogs & overlays

- [ ] File chooser: import EPUB (R)
- [ ] File chooser: import dictionary (StarDict, SQLite, TSV) (R)
- [ ] Folder chooser: library folder and watch folders (?)
- [~] Confirm dialogs: delete book, shelf, highlight, dictionary pack, tag (?). The dialog component is defined (decision 75); tag delete and PDF page delete are drawn; Delete backups and Remove dictionary are drawn (chat 8, decision 150).
- [x] Restart prompt after switching library (chat 8, decision 149)
- [ ] Rename dialog (shelf, tag) (?). Tag rename from the Tag dialog row menu is not drawn.
- [x] ~~Shelf checklist dialog~~ replaced by the Shelves submenu in the book ⋯ menu (decision 86)
- [x] ~~Half-star rating popover~~ replaced by an inline half-star rating on the book page (decision 82)
- [ ] Yearly reading goal dialog (R)
- [ ] Export dialogs: quotes to Markdown, words to CSV/Anki (R). The Export buttons are drawn in Settings, Data and export; the dialogs are not.
- [~] Bulk tag edit dialog (R: "bulk metadata edits"). The Tag dialog for one book is drawn (decision 84); bulk editing across books is not.
- [ ] Keyboard shortcut cheat sheet overlay (?). The list is agreed (decision 155); the overlay is a read-only view of it and is not drawn.
- [ ] About dialog (?)

## 8. Menus & popovers

- [x] Book page ⋯ menu with the Shelves submenu, and the cover pencil popover (chat 5)
- [x] Author page ⋯ menu (chat 6, decision 112)
- [x] Sort dropdown, with a Group dropdown (chat 10, decision 167)
- [x] Filter menu with Shelf and Tag submenus (chat 10, decision 168)
- [x] Book card right-click menu: open, open in Bubble, add to shelf, add to reading list, mark finished, edit metadata, select, show in file manager, remove (chat 10, decision 166)
- [ ] Shelf card right-click menu (?)
- [ ] Tag right-click menu (?)
- [ ] Reader: right-click on selected text (?)
- [ ] Reader: right-click on a highlight (?)
- [ ] Reader: right-click on blank page area (?)
- [ ] Annotation row menu (?)
- [ ] Smart-shelf rule dropdowns (field / operator / value) (R)
- [x] Dropdown and select styling in Settings: closed and open, typeface and shelf lists (chat 8, decision 157)

## 9. Cross-cutting states

Design these once, then reuse them on every screen.

- [~] Empty states: no books, no shelves, no quotes, no results. Drawn so far for Settings: no watch folders, no dictionaries, no notifications.
- [~] Loading and skeleton states: reader thumbnails and page skeletons are drawn; library cover placeholders are not
- [~] Error states: PDF, comic and book-page file-missing states are drawn; corrupt EPUB and import failure are not
- [ ] Progress and cancellable background tasks (R: task manager)
- [x] Book cover fallback when a cover is missing: a neutral block, no tint (chat 5, design system section 20)
- [~] Selected, hover, focus and disabled states for every control: drawn for the controls designed so far (readers, book page, series page); every new screen must include them
- [~] Narrow and wide window behaviour: the grid is drawn at 640 and 1920 (chat 10, decision 161); every other page is not, and windows narrower than 640 are not designed

## 10. Part 2, not built yet (R)

Designed later; the routes and job queue exist but have no content behind them.

- [ ] Downloads hub (hidden from the sidebar)
- [ ] Browse / fiction client (AO3, Royal Road)
- [ ] Manga source browsing (MangaDex)
- [ ] Plugin management UI
- [ ] Online search for Fanfiction.net, Royal Road and MangaDex, following the AO3 template (decision 206). The AO3 work page and Browse page.
- [ ] Fetch a missing book from the Author page (the + button, decision 109): fetching, progress and failed states

---

## Decisions so far

Full list in `decisions.md` (numbered 1 to 207). In short:
- Splash screen is final. Home, Library, Author and the app Settings panes are still draft.
- The readers (EPUB, PDF, comic), the book details and series pages, the Author page and the deep editor are designed in Terracotta light and dark. Forest, Plum and Teal are still to check.
- The PDF reader and EPUB reader have different appearance panels. Bubbles / split-screen gets its own design pass.
- The navigation rail will gain more entries. The Editor is a top-level entry and is also reachable from a book. The deep editor is HTML, CSS and XML only (Markdown was dropped, decision 120).
- No scrollbars are visible anywhere (decision 90).
- Settings is a full page with five groups of panes. Appearance is one palette-and-mode choice (no System option); Reading holds global defaults and every format can override per book; notification history is a bell in the rail; Integrations is designed early and disabled (decisions 144 to 158).
- The Library page is a grid of 1.6 : 1 covers with the title as the shelves menu, an Add books button, multi-select with a bulk bar and Open in Bubble, a list view with configurable columns, and a search with operators (decisions 159 to 172).
- My Library is one long page of the user's own space in a fixed order, with a thin-line reading timeline and a saved-words sphere; Impressions live in the timeline card (decisions 173 to 177).
- Home is a calm two-column page: the open book for Continue reading, Your shelf with a zoomable timeline, Updates, and a small underlined search bar that opens into a box. The rail is Home, My Library, Shelves, Comics, Editor, with the bell and Settings below (decisions 179 to 207).
- First-run is not designed. The Authors rail entry was dropped.
- New screens will be added to this list as they turn up in `src/pages/`.

## Open questions

1. Where does the metadata source credit live now that the book page footer dropped it? (chat 5)
2. Do the EPUB reader's Notes and Contents panels, the end-of-book moment and the Reading settings follow the new book page (decisions 89 to 92)?
3. Does the comics series page adopt the new series layout, or keep its band and chapter list?
4. Re-reads: one impression per book, or several?
5. Narrow windows: the book page action row while Next is open.
6. Authors index page and its rail entry: own chat (decision 117).
7. Where does the author portrait come from, and how do Edit author and Merge into another author work? (decisions 105, 112)
8. Does Create shelf reuse an existing shelf with the same name? (decision 113)
9. The in-reader editor (decision 118): Edit on the chip, the floating format bar, reflow while typing, how edits are saved. Own chat with the EPUB reader boards.
10. What does the Book edits badge count, and what does Proofread do in the Tools menu? (decisions 86, 129)
11. Editor export folder (`~/Kalam/Editing/`), the monospace font, and click-to-jump between preview and source need confirming with the code (decisions 123, 127, 140).
12. The reader's Theme card and the reader-side Reading controls (decisions 145, 147) are not redrawn: the EPUB, PDF and comic reader boards need a pass.
13. Alt+3 for the Editor is missing from the Shortcuts list; add or drop? (decision 155)
14. Restore bundled packs restores all at once; per pack? Where does the Google Books API key live? (decisions 152, 153)
15. Do Content sources and Plugins merge once sources become plugins? (decision 154)
16. Does the rail's Library entry open My Library, with the grid behind "All books ›" (decision 173)?
17. Should the finished double tick replace the circle badge on the series strip, the Author page panel and the book page (decision 162)?
18. Is the jump-to-letter strip only for title and author sort (decision 167)?
19. Is the timeline dustbin "remove from the timeline only", with Undo (decision 175)?
20. Does clicking a word in the sphere open the saved word, and where do "All words ›", "All series ›", "All authors ›" and the Reading list "Open ›" lead (decision 176)?
21. Group by shelf is not offered; confirm (decision 167). Alt+Left and Alt+Right move a focused column; confirm (decision 172).
22. Where is the user's name set, and where do the Home quotes come from (decisions 185, 187, 207)?
23. Is reading time recorded per book, and can a cover's dominant colour be cached at import (decisions 190, 192)?
24. Can the reader engine give an excerpt at the saved position, and how big is the full-text index (decisions 188, 199)?
25. Which icon set, and is the window drawn with client-side decoration (decisions 179, 181)?
26. Does "Save this search as a shelf" come back (decision 201), do AO3's terms allow the client (decision 205), and should the Library page and Home search agree on book-text results (decisions 169, 199)?

