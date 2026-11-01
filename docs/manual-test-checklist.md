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

**Only those ten fields come back, and that is by design.** Deleting a book also
removes its reading position, bookmarks, annotations, full-text search index and
OCR cache; re-importing does not restore them. The search index and the OCR are
rebuilt as the book is used again. A missing bookmark or annotation here is
expected behaviour, not a failure of this check.

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

## Report failures

Record the book format, the steps that led to the problem, the expected result,
and what appeared instead. For app or CI logs, see [`docs/ci/README.md`](ci/README.md).
