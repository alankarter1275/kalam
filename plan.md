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

- **Series detection: settled.** The owner confirms his CBZ files carry
  ComicInfo.xml, and the app already parses it (plus filename
  heuristics) and auto-catalogs every comic into a series at import
  (`get_or_create_comic_series` + `add_comic_chapter`,
  `src/epub.rs:273–288`; `backfill_comic_series` for older rows).
  Screenshots of his current folder names did not reach the agent (no
  vision; files never landed in the workspace) — 2–3 names pasted as
  text would still sanity-check how detection grouped his library,
  but nothing blocks on it.
- **Reading flow: confirmed** ("yup") — and already shipped: the
  comics hub's series drawer (resume, read badges), the reader's
  end-of-chapter card and webtoon auto-flow, the Chapters sidebar
  (ROADMAP changelog 2026-09-27).

**Scope-collapsing finding (verified 2026-10-02):** the in-app half of
the owner's ask already exists. The comics hub groups by series
(`ComicViewMode::Series`, `src/pages/comics.rs`), the main library
routes comic clicks through the series drawer
(`src/pages/all_books.rs:1183–1204`), and the DB has the
`comic_series`/`comic_chapters` hierarchy (db v13). What does NOT
exist is the on-disk half the owner was actually looking at: every
chapter still gets its own per-book folder
(`library/<Author - Title shortid>/book.cbz + cover.*`). So this item
is the disk layout only:

1. New imports: comics land in `library/<Series-name>/` as
   number-named chapter files (`0007.cbz`, `0007.5.cbz`; unnumbered
   fallback and collision rule to be settled in the plan) with
   `covers/<same-stem>.<ext>` beside them.
2. One-time migration of existing comics into that shape, grouped by
   the already-populated `comic_series` rows; reading positions,
   shelves and history are keyed by book ids and survive untouched
   (books store only `file_name` relative to their folder — paths are
   resolved at read time through the uuid→folder registry,
   `src/db.rs:1457/1496/1742`, `src/paths.rs:217` — so a move is a
   file move plus a `file_name`/`cover_name` update plus a registry
   update).
3. Folder-name sanitization reuses the existing Bengali-safe byte-cap
   rules (`src/paths.rs:66–110`); a series-name collision falls back
   to the same short-id/full-uuid suffix rules books use.

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
