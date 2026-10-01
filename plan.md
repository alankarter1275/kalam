# Plan — working document

**Status: 2.21 withdrawn by the owner on 2026-10-02.** The feature was
implemented and CI-green, but his field test found the text not
embedded correctly; the code is removed in the same commit as this
note. The full record is in the ROADMAP changelog rows for 2026-10-02
and in pitfalls §38 — read that before the feature is ever attempted
again. Revisit when the app is complete, per the owner.

**Next up: comics in the library folder layout (owner request,
2026-10-02, discussion open — not yet a plan).** The owner browsed the
on-disk library and does not want one folder per comic; he wants comics
in a single folder under a single title. Open questions before this
becomes a plan:

1. The exact shape: one flat "Comics" folder with each file named by
   its title, or one folder per series with the issues inside?
2. Where extracted covers live (today each book folder holds
   `book.<ext>` plus `cover.<ext>` read from the archive's first page).
3. Name collisions between comics with the same title (today the
   folder's short-id suffix prevents them; flat files need their own
   rule).
4. Comics only, or does the owner want the whole per-book-folder
   pattern reconsidered for EPUBs and PDFs too?
5. Migrating the existing library (the uuid is the book's key, so
   paths can be rewritten safely; `note_folder` and the resolver scan
   already handle renames).

Research already done for the discussion (verified 2026-10-02): every
import copies the file into `library/<Author - Title shortid>/` and
renames it `book.<ext>` (`src/epub.rs:120–260`, folder naming at
`src/paths.rs:66–110`); comic covers are extracted from the archive at
import (`crate::comics::extract_comic_cover`, written beside the file
as `cover.<ext>`); the folder name is a label only — the app resolves
book paths through the uuid (`src/paths.rs:217`, `note_folder` at
`:232`), which is what makes a layout change safe.
