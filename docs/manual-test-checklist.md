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
- [ ] When offline, confirm the library, readers and offline dictionary remain
  usable. Remote source browsing and downloads are not available because no
  online source implementation is registered.

## Report failures

Record the book format, the steps that led to the problem, the expected result,
and what appeared instead. For app or CI logs, see [`docs/ci/README.md`](ci/README.md).
