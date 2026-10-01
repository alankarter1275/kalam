# Plan — working document

**Status: 2.21 withdrawn by the owner on 2026-10-02.** The feature was
implemented and CI-green, but his field test found the text not
embedded correctly; the code is removed in the same commit as this
note. The full record is in the ROADMAP changelog rows for 2026-10-02
and in pitfalls §38 — read that before the feature is ever attempted
again. Revisit when the app is complete, per the owner.

**Next up: comics as series books in the library (owner request,
2026-10-02, discussion nearly settled).** Decisions so far, from the
owner's own words:

1. **A comic series is one book.** One entry in the library, one
   folder: `library/<Comic-name>/` holding all the chapter/volume
   files, and `covers/` inside for the chapter covers ("like 700
   chapters of Naruto"). Not a `library/Comics/` group — the series
   folder sits beside the regular book folders.
2. **Covers (decided):** static first-chapter cover in the grid; the
   book page shows the current chapter's cover beside Continue. All
   chapter covers are extracted at import (that work already happens
   today, one cover per chapter-as-book).
3. **Shelves stay virtual (owner agreeing after the Calibre check,
   2026-10-02):** Calibre's on-disk layout is one folder per book
   under author folders, with covers/metadata sidecars inside, and
   shelves/tags never become folders; multiple libraries are separate
   roots. Kalam already matches that architecture; the comics change
   is the only deviation (a series gets the per-book folder, chapters
   inside).
4. **EPUBs/PDFs keep today's per-book folders.**

Still open before this becomes a plan:

- **Series detection:** ComicInfo.xml inside the CBZ, filename
  pattern, or both — the owner does not know what his files carry;
  asked for 3-5 real comic file/folder names from his library and a
  peek inside one CBZ. The app already parses both
  (`read_comic_info_from_zip`, `parse_comic_filename_internal`,
  `sanitize_comic_series` in `src/comics.rs`), so the plan is
  detection-agnostic; the samples tune the weight and the manual
  grouping UI.
- Reading flow assumption (stated, unconfirmed): open series ->
  chapter list + continue where you left off; finishing a chapter
  flows into the next.
- Migration of existing comics: automatic merge into series folders,
  reading progress preserved (chapter files unchanged, only grouping
  and paths move; the uuid is the key, so renames are safe).

Research already done for the discussion (verified 2026-10-02): every
import copies the file into `library/<Author - Title shortid>/` and
renames it `book.<ext>` (`src/epub.rs:120–260`, folder naming at
`src/paths.rs:66–110`); comic covers are extracted from the archive at
import (`crate::comics::extract_comic_cover`, written beside the file
as `cover.<ext>`); the folder name is a label only — the app resolves
book paths through the uuid (`src/paths.rs:217`, `note_folder` at
`:232`), which is what makes a layout change safe. Comic series
metadata sources already parsed: ComicInfo.xml (ComicRack schema),
filename patterns, parent-folder fallback (`src/comics.rs:326–431`).
