# Manual test checklist

Use this checklist for the current app. It covers the main user flows; it is not
a substitute for automated tests. Use copies when testing workflows that write
book files, such as import sanitization or metadata and cover replacement.

## Test data

Prepare a small library containing:

- A reflowable EPUB with several chapters, a nested table of contents, and
  selectable text.
- A text-based PDF with several pages. A scanned PDF is useful for checking
  OCR, but is optional.
- A CBZ or CBR comic. A second book is useful for testing bubbles and split
  reading.
- An offline dictionary installed in Kalam.

## Start, import and library

- [ ] Launch Kalam and confirm the app window and main page render without a
  crash or a persistent error dialog.
- [ ] Import one EPUB, one PDF and one comic archive. Confirm each appears in
  the library and opens in its matching reader.
- [ ] Import the same file again. Confirm it is recognized as a duplicate and
  does not create a second catalog entry.
- [ ] Search by title, author and book text. Clear the search and confirm the
  full library returns.
- [ ] Change the library sort order and open a book's details page.
- [ ] Edit metadata, tags and rating. Return to the library and confirm the
  changes appear there.
- [ ] Create a manual shelf and a smart shelf. Add a book to the manual shelf,
  then confirm the smart shelf follows its saved rules.
- [ ] With two or more shelves, confirm the line above the grid reads
  "2 shelves · 1 manual · 1 smart". It used to print "shelfes"; with exactly one
  shelf it should still read "1 shelf · 1 manual · 0 smart".
- [ ] Add a book to a reading list and verify it remains there after leaving
  and reopening the page.
- [ ] If multiple library folders are configured, switch libraries and confirm
  the selected library's books appear.
- [ ] If a watch folder is configured, add a supported book file to it and
  confirm it is imported once.

## Edits surviving a delete and re-import

Hand-edited metadata is keyed to the file a book came from rather than to its
row in the database, so deleting a book and importing the same file again should
bring the edits back. That is what `books.import_hash` is for, added 2026-10-30.

There is automated coverage — `edits_survive_a_hash_change_then_delete_and_reimport`
in `src/db.rs` — but it uses seeded books and a synthetic hash change. The check
below is against a real library and real edits, and **it has not been run yet**.

This deletes and rewrites book files, so the rule at the top of this checklist
applies: work against a copy of the library, not the one in daily use.

- [ ] Pick a book whose metadata you edited by hand. Write down what you
  changed: title, authors, series and its number, publisher, published date,
  description, tags, rating, or the cover.
- [ ] Delete the book from Kalam.
- [ ] Import the same file again, unmodified.
- [ ] Open the re-imported book and confirm every edit you wrote down is back.
- [ ] Now take a book you have **never** edited. Delete it, import the same file
  again, and read the message that appears when the import finishes. It must not
  say that the book kept your earlier metadata edits. There were none to keep.
- [ ] Edit a book's metadata, delete it, import the same file again, and confirm
  the message *does* say your edits were kept. The two cases must not both stay
  quiet or both speak up.

Your reading position, bookmarks, highlights, quotes and PDF ink come back too,
but they are a separate question and the app asks you about them — see
"Keeping your notes when a book is removed and comes back" below.

**The full-text search index and the OCR cache do not come back, and that is by
design.** Both are rebuilt as the book is used again, so a book that has just
been re-imported may not appear in a deep search until it has been opened. That
is expected here, not a failure of this check.

**Keep the backup until this passes.** `~/kalam-backup-before-rekey/` holds the
`catalog.db`, `catalog.db-wal` and `catalog.db-shm` copied before the
`import_hash` change went in. It can be deleted once the steps above have
succeeded once.

## EPUB reader

- [ ] Open the reflowable EPUB. Move between chapters and pages, then use the
  nested table of contents to jump to a chapter.
- [ ] Switch between paginated and continuous reading modes. If the window is
  wide enough, check the facing-page layout.
- [ ] Close and reopen the book. Confirm the saved reading position is restored.
- [ ] Add and remove a bookmark. Confirm the bookmark appears in the reader's
  bookmark list and returns to the saved location.
- [ ] Select text and create an annotation. Reopen the annotations list and
  confirm the saved annotation is present. If an older annotation has no usable
  locator, it should remain in the list even though it cannot be painted at a
  text position.
- [ ] Select a word or phrase and use the explicit Dictionary action. Confirm
  the offline dictionary opens. Ordinary taps are not dictionary lookup;
  tapping a marked Word Memory term opens its saved word card.
- [ ] Save a word to Word Memory, find it in the saved-words page, and remove
  or mark it there. If available, check an export.
- [ ] Use `Ctrl+F` to search for text in another chapter. Confirm results can
  be opened and the search can be closed.
- [ ] Follow an in-book note or footnote and return to the reading location.
  If the book has images or external links, check the image viewer and link
  handling.
- [ ] Open a fixed-layout EPUB, if one is available. Confirm the current reader
  rejects it rather than treating it as a reflowable book.

## EPUB content editing (removed)

- [ ] Confirm the reader, book details page and app navigation expose no EPUB
  content editor, source-edit entry point or pending-edits review action.
- [ ] Confirm ordinary text selection, including triple-click paragraph
  selection, remains available and does not open an inline editing box.

## PDF reader

- [ ] Open a text-based PDF. Turn pages, use the page outline when present, and
  try both paged and continuous viewing.
- [ ] Change zoom and use PDF text search. Open a result and confirm it lands
  on the matching page.
- [ ] Add and remove a PDF bookmark, then reopen the PDF and confirm reading
  progress is restored.
- [ ] If using a scanned PDF, confirm pages render and OCR text becomes
  searchable when OCR is available.

## Comics reader

- [ ] Open a CBZ or CBR archive and move through several pages in both
  directions.
- [ ] Check that the reader displays the archive's cover when one is
  available.
- [ ] If the book has series/chapter metadata, confirm the series and chapter
  information are shown consistently in the library and reader.
- [ ] Save a named Reading/Image preset, change the panel, reapply the
  preset, and confirm the same page is rebuilt with the preset. Save the
  same name again and confirm it replaces rather than duplicates it.
- [ ] From a comic-series drawer, hand-edit details, then use Find
  metadata and confirm those hand-edited fields stay locked in review
  while other fields can still be selected.

## Bubbles and split reading

- [ ] Open one book in a bubble, return to another app page, and reopen the
  book from its bubble.
- [ ] Keep bubbles for several books and switch between them.
- [ ] Drag a second book into split mode. Confirm two readers are visible side
  by side and that each can navigate independently.
- [ ] Close or minimize the floating reader and confirm the remaining bubbles
  still open their books.

## Optional online metadata

- [ ] When network access is available, search Open Library or Google Books
  from the metadata editor. Preview a result and apply it only through the
  editor's confirmation flow.
- [ ] With comic credentials saved in Settings, search MyAnimeList and
  Comic Vine from a comic-series drawer, review individual fields before
  applying, and confirm chapter titles/covers were not rewritten. Reopen
  Find metadata and use Refresh linked record on the stored source ID.
- [ ] When offline, confirm the library, readers and offline dictionary remain
  usable. Remote source browsing and downloads are not available because no
  online source implementation is registered.

## The navigation rail

Rebuilt on 2026-11-08 to design system §56 and the rulings in
`docs/build-decisions.md` (B1 to B11). **None of this has been run by hand
yet** — CI photographs the rail but cannot judge it.

- [ ] The rail holds eight entries: Home, My Library, All books, Authors,
  Comics, Online sources and Editor, then the bell and Settings at the bottom.
  There is no Shelves disc and no Tasks disc.
- [ ] Hover an entry. After a short delay a card appears to its right, 32 high,
  with the entry's name and its shortcut (`Alt+1` to `Alt+7`; Settings shows
  `Ctrl+,`). Moving the pointer away hides it; clicking hides it at once.
- [ ] The active entry is a filled disc whose glyph is the page colour, so it
  reads as cut out (B8). Hovering an idle entry fills its disc with the border
  colour and the glyph does not change.
- [ ] The rail has no fill of its own and no full-height line. The only line is
  2px with rounded ends, stopping 28 from the top and 24 from the bottom.
- [ ] `Alt+3` opens All books and `Alt+7` opens the Editor. The Editor used to
  be `Alt+3`; if you rebound it, your own binding still fires (B7).
- [ ] Shelves is reachable from My Library's quick links (B9), and from there
  the grid, a shelf and both shelf editors still work.
- [ ] Online sources opens a page saying nothing reaches the internet yet (B4),
  and no request leaves the machine while it is open.
- [ ] Start a background job (an import, or a watch-folder scan). Open the
  bell: the panel has a nested box at the bottom listing what is running, and
  both its Open button and each row go to the tasks page (B5).
- [ ] Hover the bell while a job runs. Its card carries a third line —
  "1 task running", "2 tasks running" — and the line is gone once nothing is
  running.
- [ ] With an unseen event the bell shows its dot, 8px, 8 from the top and 9
  from the right; opening the panel clears it.
- [ ] The bell while its panel is open is a filled disc with a **white** glyph
  (§45) — not the cut-out the active entry uses (B8).
- [ ] In Settings → Shortcuts, the Navigation group lists ten rows: Home, My
  Library, All books, Authors, Comics, Online sources, Editor, then Settings,
  Back and Notifications. The seven rail rows show Alt+1 to Alt+7, and each is
  still rebindable there.
- [ ] Switch theme in Settings → Appearance with the rail in view. Every glyph
  changes colour with it, the cut-out active glyph included, and the brand mark
  follows the accent.

## Home

Stage 1 of the rebuild (four stages, `docs/build-decisions.md` B12), to
`docs/design/home-spec.md` and design-system §57–§66. **None of this has been
run by hand yet** — CI photographs Home but cannot judge it. The reading
ruler (§61) and the search bar (§57) are stages 2 and 3: their absence is
deliberate, not a defect.

- [ ] Top left: the greeting on two lines — "Good morning", "Good afternoon"
  or "Good evening" by the hour, in the big serif face — with today's date
  under it. No name yet: that waits on the customization area in Settings
  (B14).
- [ ] Under it the quote: an italic serif couplet behind a thin accent rule,
  attribution below (§63). It is the placeholder — Dickinson — until the real
  source is settled (B13).
- [ ] "YOUR SHELF" heading with an "Arrange by · Most recent" chip at its
  right, and below them a dashed card filling the ruler's 340px band saying
  the ruler arrives next stage (B12). The chip does nothing yet.
- [ ] Right column: a 560×360 card — left half the cover of the book last
  read, right half its title in small capitals and, under it, "Where you
  left off" with a quiet line: the chapter name and its first words wait
  for their own store (B17) and return in a later stage (§60).
- [ ] A round accent play button at the card's bottom-right resumes the
  reader at the saved position; an 18×36 accent ribbon hangs at its top,
  32 from the right.
- [ ] Clicking the cover opens the book's details page.
- [ ] With several books in progress, a column of up to five dots stands
  right of the card: the current one an accent pill, the rest faint circles.
  Clicking a dot swaps the card to that book and moves the pill (§60).
- [ ] Home opens at once on a real library: no freeze at the splash screen,
  no stall lines in the terminal. Check it with a book that has a saved
  reading position — that is the state the pager reads, and the state both
  startup freezes needed (B17, pitfalls §105–§106).
- [ ] With an empty library: a dashed "Your library is empty" card whose
  "Add books" button opens the import chooser; after importing, Home refreshes
  without leaving the page (§64).
- [ ] With books but none opened: a dashed "Nothing in progress" card — the
  pager shows only books with a saved position, never a forced first book
  (§64).
- [ ] Above the left column: 28px of air, then an empty 520×40 strip — the
  search row §57 reserves from stage 1 (B12). Nothing below it moves when the
  bar arrives in stage 3.
- [ ] No Updates block anywhere on the page: §62 draws nothing until something
  feeds it (B15).
- [ ] Top-right over the page: three round 32px buttons — minimize, maximize,
  close (§65, B16). Minimize shrinks the window; maximize fills the screen
  and its glyph swaps to the restore pair; close quits, cancelling running
  jobs as before.
- [ ] The three buttons show on every page except inside the reader, which
  brings its own chrome. Hovering any of them gives the same border and fill.
- [ ] Switching theme in Settings recolours the quote rule, the play triangle,
  the ribbon and the window glyphs along with everything else — the glyphs
  are textures and repaint through the rail's machinery (B6).

## Keyboard shortcuts

CI now has a keystroke probe (`KEYS=1` in `docs/ci/screenshot.sh`) that types
the app's own default shortcuts with `wtype` and asks three independent
witnesses whether the app acted: its timing log, the compositor's window state,
and whether the process died. Across six runs it has seen every one of the nine
default shortcuts fire at least once, so the shortcut table looks sound. The
harness itself is not: it drops keystrokes at a low rate and the chords it drops
move from run to run, so each is now sent twice and the report says which
attempt landed. Two checks needed a human hand, because CI can produce a result
it cannot interpret. **Both were run by the owner on 2026-11-03 and both
passed** — every shortcut works on real hardware, and the one CI could never
reproduce turned out to be the headless compositor after all.

- [x] **PASSED 2026-11-03.** Press `Ctrl+F` to focus the library's search box,
  type nothing, then press `Alt+Left`. Expected: you leave All Books and go
  back. **It works.** CI failed this in runs 1 and 5 and passed it in run 6,
  with the same search box focused each time — so the failures were the harness
  dropping the key, not the app refusing it. For a while the reports claimed a
  bug here that never existed. Every readable source agrees nothing
  should claim that key: `gtktext.c` binds `Alt+Left` only under `#ifdef
  __APPLE__`, `gtkwindow.c` binds arrows with no modifier and with `Ctrl` but
  never with `Alt`, and `src/pages/all_books.rs` installs no key controller at
  all. **If Back does nothing for you, say so** — that would make it real.
- [x] **PASSED 2026-11-03.** Press `F11` twice on any page. Expected: full
  screen, then back to windowed. **It works** — the window goes full screen and
  comes back. This settles the one question CI could not: the second `F11` never
  worked in any of seven headless runs, and I had written it up as the only
  remaining candidate for a real defect. It was never a defect. It is the
  headless compositor, exactly as the handler's `root.is_fullscreen()` guard
  suggested — under headless sway GTK does not learn the window's state, so it
  asks to go full screen a second time instead of leaving. The handler calls `unfullscreen()` only when
  `root.is_fullscreen()` says the window is already full screen, so if GTK
  never learns the state under a headless compositor it would ask to go full
  screen a second time instead.

## Keeping your notes when a book is removed and comes back

Added 2026-11-04. These check the work that lets you choose what survives a
book being removed, and what comes back when you import the same file again.
Nothing here touches your book files. It does write to the catalog, so use a
library you are willing to test on.

There are five kinds of saved work: your place in the book, bookmarks,
highlights, quotes, and ink on PDF pages. An underline is a highlight drawn in
a different style, not a sixth kind. Which shelf a book sits on and whether it
is on your reading list are always put back and are never offered as a choice,
because they are where the book lives rather than work you did in it.

**Removing one book**

- [ ] Open a book's details page. Make some marks in it: a highlight, a quote,
  a bookmark, and read a little way in so it has a position.
- [ ] Open the ⋯ menu. Confirm "Remove from library" is now a submenu with two
  items: one that removes the book, and one that clears its notes.
- [ ] Choose the remove item. Confirm the dialog shows one checkbox, "Also
  remove my notes and marks", and a small "choose what to keep" link under it.
- [ ] Click the link. Confirm the five kinds appear and can be ticked
  separately, and that a line under them says in words what you are keeping.
- [ ] Click "Keep book". Confirm nothing was removed and the book is still there.
- [ ] Remove the book again, leave the checkbox unticked, and confirm. Then
  import the same file again. Confirm your marks came back.
- [ ] Now try the other choice: remove the book with the checkbox ticked, then
  import the same file again. Confirm it comes back clean, with no marks.

**Clearing notes without removing the book**

- [ ] On a book's details page, open the ⋯ submenu and choose the clear-notes
  item. Untick one or two kinds and confirm.
- [ ] Check that only the kinds you unticked are gone, and that the rest are
  still there. Confirm a message says what was removed, naming the kinds and
  how many.
- [ ] Choose the clear action again but leave everything unticked. Confirm it
  says there is nothing to clear rather than claiming it removed something.
- [ ] Remove the book, then import the same file again. Confirm the kinds you
  cleared do **not** come back. This is the important one: clearing is meant to
  be permanent, and the copy Kalam keeps aside has to go with it or the cleared
  marks would walk back in.

**Removing several books at once**

- [ ] In All Books, select two or three books and choose delete.
- [ ] Confirm a table appears with one row per book, a search box, and a column
  per kind.
- [ ] Type in the search box. Confirm the list narrows, and that the rows you
  cannot see any more are still decided by their own ticks — hiding a row does
  not change its answer.
- [ ] Untick different kinds on different rows, then confirm. Check afterwards
  that each book kept exactly what its own row said.
- [ ] Import those files again. Confirm each comes back with only what its row
  kept.

**Comics**

- [ ] Select a comic series and delete it. Confirm the table shows one row per
  card, not one row per chapter, and that the count in the title matches the
  number of cards.
- [ ] Confirm one tick on a card row applies to every chapter of that series.

**The question after an import**

- [ ] Remove a book that has marks in it, leaving the marks kept aside. Then
  import the same file again.
- [ ] Confirm the import finishes first, and only then a dialog appears titled
  "Your notes came back", naming the book by its real title rather than its
  file name.
- [ ] Confirm the two buttons read "Ask me later" and "Keep these" — not
  "Cancel", because closing this dialog leaves something outstanding.
- [ ] Choose "Keep these" with everything ticked. Confirm your marks stay and no
  further question is asked.
- [ ] Repeat, but untick some kinds before choosing "Keep these". Confirm only
  the ticked kinds stayed.
- [ ] Import several files at once that all have marks kept aside. Confirm you
  get the searchable table rather than one dialog per book.

**"Ask me later"**

- [ ] Import a file that has marks kept aside, and choose "Ask me later".
- [ ] Confirm nothing is removed and no message claims a choice was saved.
- [ ] Leave the book and open it again. Confirm the same question appears, with
  the same five choices.
- [ ] Answer it. Confirm you are not asked again on the next refresh of that
  page, and not again the next time you open the book.
- [ ] Choose "Ask me later" again, then open a *different* book. Confirm the
  question does not appear for the other book — it belongs to one book only.

**Imports that happen by themselves**

- [ ] If you use a watch folder, drop in a file that has marks kept aside.
  Confirm no dialog appears, and that a message says your notes came back and
  tells you to open the book to choose. Confirm the message does not offer an
  undo — the marks are already back, so there is nothing to take back.
- [ ] Open that book. Confirm the question appears then.
- [ ] If you use downloads, do the same with a download that replaces a book you
  had removed.

**The reader's own remove dialog**

- [ ] Open a PDF whose file has gone missing or will not open, so the reader
  shows its "cannot open" state with a remove button.
- [ ] Confirm the dialog there is the same granular one, with the checkbox and
  the "choose what to keep" link, rather than a plain two-button confirmation.

**Settings → Remembered data**

- [ ] Open Settings and find "Remembered data" in the Library group, next to
  "Data and export".
- [ ] Confirm it explains what these copies are before listing anything.
- [ ] With at least one removed book that has a copy kept aside, confirm the
  copies for books no longer in your library are listed, each saying what it
  holds and when it was kept.
- [ ] Remove a book and import it again so the book is back in the library. Then
  look at the pane again. Confirm that book's copy is listed too, under its own
  heading. This is not a leftover: Kalam keeps the copy on purpose so that a
  second remove-and-import still works, and you should be able to let go of it.
- [ ] Confirm a book that is still waiting for an answer is listed first, under
  "Waiting for your answer", and that its row says to open the book.
- [ ] Choose "Let go" on one copy. Confirm the copy goes and that nothing
  changed in any book still in your library.
- [ ] Choose "Let go of every copy". Confirm it asks first, and that the warning
  says plainly that books still in your library keep their notes and marks.
- [ ] Confirm it, then check a book that was in the list. Confirm its marks are
  all still there.
- [ ] Remove that book and import the same file again. Confirm it now comes back
  clean, since the copy is gone.
- [ ] If you have more than 200 copies, confirm the pane says it is showing the
  newest 200 of the real total, and that clearing still lets go of all of them.

**The hover card asks too**

- [ ] Hover over a book anywhere it shows a small remove button (the hover card),
  and click it. Confirm a question appears asking what to keep, with the same
  checkbox and the same "choose what to keep" link as everywhere else.
- [ ] Confirm nothing is removed until you answer. Clicking the button alone must
  not delete the book — it used to.
- [ ] Answer it, then confirm the book is gone and the toast says it was removed.
- [ ] Repeat, but this time untick the box so your notes are let go. Re-import the
  same file and confirm it comes back clean, with no notes and no question.

**Saved words**

- [ ] Confirm the saved-words section on a book's details page is unchanged by
  all of the above. Saved words are one personal dictionary and are never
  touched by removing a book, clearing notes, or letting go of a copy.

## Report failures

Record the book format, the steps that led to the problem, the expected result,
and what appeared instead. For app or CI logs, see [`docs/ci/README.md`](ci/README.md).
