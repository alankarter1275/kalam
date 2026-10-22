# Kalam: screen & surface inventory

A checklist of everything that needs designing, from full pages down to right-click menus.

**How to read this**
- `[x]` = designed and approved (Terracotta light and dark; Forest, Plum and Teal still to check). `[~]` = partly designed; the note says what is missing. `[ ]` = still to do. Last updated after design chat 7.
- `[ ]` = still to do. Items tagged **draft** have an incomplete mockup in the uploaded files (Home and Library in `kalam-mockup.html`, Author, Settings); the rest have none.
- **(R)** = confirmed in the repo README. **(?)** = my inference; please confirm or strike.
- Sources: the README's feature list and `pages/` listing, plus your four mockups. I couldn't open `src/` or `docs/offline-roadmap.md`, so this was not checked against the code. Run `ls src/pages src/widgets` and diff it against this list.
- Part 2 (online sources, downloads, plugins) is listed last and is not built yet.
- Process and file rules: `HANDOFF.md`.

---

## 1. App shell & global

- [x] Splash screen (R)  ← approved, good to go
- [ ] First-run / library check: choose a library folder when none exists (R)
- [ ] Navigation rail: Home, Library, Editor, Settings (R). Downloads is hidden. An **Authors** entry is proposed (decision 117). **Draft**; more entries will be added later.
- [ ] Window chrome and title bar (?)
- [ ] Toast notifications (R)
- [ ] Notification history panel (R)
- [ ] Page transition behaviour (?)
- [ ] Global empty state, loading state, and error state patterns (?)
- [ ] Keyboard focus ring and focus trap in dialogs (R: `focus trap` widget)

## 2. Home

- [ ] Home: greeting, saved-quote strip, search, shelves row, continue-reading card with "journey" timeline, activity feed (R) **draft**
- [ ] Home: counts strip and "Up next" peek (R, in the click-through demo)
- [ ] Home: empty library state (?)

## 3. Library

- [ ] My Library hub with quick links (R) **draft**
- [ ] All books grid: search, sort, filter, cover cards (R)
- [ ] All books: list view alternative (?)
- [ ] Windowed grid loading behaviour (R)
- [ ] Library-wide full-text search results (FTS5) (R)
- [ ] Search syntax help: `tag:`, `author:`, `status:`, `rating:` (R)

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
- [ ] Authors index page: a top-level page listing every author in the library, opened from an Authors rail entry (?, decision 117)
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

Mockup panes: Appearance, Reading, Library, Integrations, Keyboard Shortcuts. The readers' own settings panes (Reading, Image, Shortcuts) are done inside each reader's right panel.

- [ ] Settings: Appearance (theme palettes, light/dark) **draft**
- [ ] Settings: Reading (typeface, size, spacing, margins, alignment) **draft**
- [ ] Settings: Library (folder, catalog DB, metadata writeback, original backups, EPUB sanitizer) **draft**
- [ ] Settings: Integrations **draft**
- [ ] Settings: Keyboard Shortcuts **draft**
- [ ] Settings: Dictionaries: import, list, remove, priority reorder (R)
- [ ] Settings: Storage / Libraries: add, switch (restart), copy to another machine (R)
- [ ] Settings: watch folders (multi-folder import) (R)
- [ ] Settings: data paths (R)

## 7. Dialogs & overlays

- [ ] File chooser: import EPUB (R)
- [ ] File chooser: import dictionary (StarDict, SQLite, TSV) (R)
- [ ] Folder chooser: library folder and watch folders (?)
- [~] Confirm dialogs: delete book, shelf, highlight, dictionary pack, tag (?). The dialog component is defined (decision 75); tag delete and PDF page delete are drawn.
- [ ] Restart prompt after switching library (R)
- [ ] Rename dialog (shelf, tag) (?). Tag rename from the Tag dialog row menu is not drawn.
- [x] ~~Shelf checklist dialog~~ replaced by the Shelves submenu in the book ⋯ menu (decision 86)
- [x] ~~Half-star rating popover~~ replaced by an inline half-star rating on the book page (decision 82)
- [ ] Yearly reading goal dialog (R)
- [ ] Export dialogs: quotes to Markdown, words to CSV/Anki (R)
- [~] Bulk tag edit dialog (R: "bulk metadata edits"). The Tag dialog for one book is drawn (decision 84); bulk editing across books is not.
- [ ] Keyboard shortcut cheat sheet overlay (?)
- [ ] About dialog (?)

## 8. Menus & popovers

- [x] Book page ⋯ menu with the Shelves submenu, and the cover pencil popover (chat 5)
- [x] Author page ⋯ menu (chat 6, decision 112)
- [ ] Sort dropdown (R: sort exists)
- [ ] Filter menu (the filter button on the search bar in the mockup)
- [ ] Book card right-click menu: open, add to shelf, add to reading list, mark finished, edit metadata, delete (?)
- [ ] Shelf card right-click menu (?)
- [ ] Tag right-click menu (?)
- [ ] Reader: right-click on selected text (?)
- [ ] Reader: right-click on a highlight (?)
- [ ] Reader: right-click on blank page area (?)
- [ ] Annotation row menu (?)
- [ ] Smart-shelf rule dropdowns (field / operator / value) (R)
- [ ] Dropdown and select styling in Settings (e.g. typeface, alignment) (R)

## 9. Cross-cutting states

Design these once, then reuse them on every screen.

- [ ] Empty states: no books, no shelves, no quotes, no results
- [~] Loading and skeleton states: reader thumbnails and page skeletons are drawn; library cover placeholders are not
- [~] Error states: PDF, comic and book-page file-missing states are drawn; corrupt EPUB and import failure are not
- [ ] Progress and cancellable background tasks (R: task manager)
- [x] Book cover fallback when a cover is missing: a neutral block, no tint (chat 5, design system section 20)
- [~] Selected, hover, focus and disabled states for every control: drawn for the controls designed so far (readers, book page, series page); every new screen must include them
- [ ] Narrow and wide window behaviour (?)

## 10. Part 2, not built yet (R)

Designed later; the routes and job queue exist but have no content behind them.

- [ ] Downloads hub (hidden from the sidebar)
- [ ] Browse / fiction client (AO3, Royal Road)
- [ ] Manga source browsing (MangaDex)
- [ ] Plugin management UI
- [ ] Fetch a missing book from the Author page (the + button, decision 109): fetching, progress and failed states

---

## Decisions so far

Full list in `decisions.md` (numbered 1 to 143). In short:
- Splash screen is final. Home, Library, Author and the app Settings panes are still draft.
- The readers (EPUB, PDF, comic), the book details and series pages, the Author page and the deep editor are designed in Terracotta light and dark. Forest, Plum and Teal are still to check.
- The PDF reader and EPUB reader have different appearance panels. Bubbles / split-screen gets its own design pass.
- The navigation rail will gain more entries. The Editor is a top-level entry and is also reachable from a book. The deep editor is HTML, CSS and XML only (Markdown was dropped, decision 120).
- No scrollbars are visible anywhere (decision 90).
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
