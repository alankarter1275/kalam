# Plan — 2.22: comic series folders on disk

**Status: IMPLEMENTED (this branch), awaiting CI and the owner's field
test.** Approved 2026-10-02 with the short padded file names
(`0010.cbz`). The implementation notes at the bottom record one
mechanism deviation from the draft — same behavior, simpler machinery —
and where each piece landed.

Per the working agreement: research below is done and cited (code with
file/line evidence, read 2026-10-02); pitfalls consulted (§§36–38 and
the 2026-09-04 "build something that already existed" lesson — this
plan exists because the research found the series model already
shipped, collapsing the owner's ask to the folders alone).

---

## Goal

Comic series live on disk the way they already live in the app: one
folder per series under `library/`, holding the chapter files and
their covers. New imports land there directly; existing comics migrate
once, in the background, with reading positions, shelves, history and
annotations untouched (they are keyed by ids, not paths). Regular
books (EPUB/PDF) keep today's per-book folders. Shelves stay virtual.

Target shape, using the owner's real series:

```
library/
  Horimiya (Official)/
    0010.cbz  0011.cbz  0102.cbz
    covers/ 0010.jpg 0011.jpg 0102.jpg
  Naruto – Digital Colored Comics/
    0003.cbz  0004.cbz
    covers/ 0003.jpg 0004.jpg
  <existing per-book folders for EPUBs and PDFs, unchanged>
```

## What is true today (verified, with evidence)

1. **Series detection is solved and already ran on the owner's
   library.** ComicInfo.xml parser plus filename heuristics
   (`parse_comic_info`, `src/comics.rs:326–431`); the import
   auto-catalogs every CBZ/CBR into the `comic_series` /
   `comic_chapters` hierarchy (`get_or_create_comic_series` +
   `add_comic_chapter`, `src/epub.rs:273–288`), with title-parsing
   fallbacks. The owner's pasted folder names prove it grouped
   correctly: "Hero - Horimiya (Official) - Ch. 10 cdd99250",
   "Naruto – Digital Colored Comics - Ch. 3 ef64c6c9" — series name
   and chapter number both detected. His files carry ComicInfo.xml
   (owner-confirmed).
2. **The in-app half the owner asked for already shipped
   (2026-09-27).** Comics hub groups by series with cards, drawer,
   resume and read badges (`src/pages/comics.rs`); the main library
   routes comic clicks through the series drawer
   (`src/pages/all_books.rs:1183–1204`); the reader has the
   end-of-chapter card, webtoon auto-flow and Chapters sidebar. The
   reading flow the owner confirmed ("yup") is the shipped one.
3. **Book paths are computed, not stored.** `books` stores `file_name`
   and `cover_name` relative to the book's folder
   (`BOOK_COLUMNS`, `src/db.rs:1691`); `file_path` is joined at read
   time via `book_dir(&uuid)` (`src/db.rs:1457/1496/1742`, and the
   series queries at `src/db/series.rs:322–348, 406`).
4. **The folder resolver is the one thing a series folder cannot
   use.** `book_dir(uuid)` resolves through an in-memory index built
   by scanning `library/` for folders whose **last space-separated
   token** is the book's 8-hex short id or full uuid
   (`src/paths.rs:217–300`); folders ending in anything else are
   ignored. A folder named "Naruto – Digital Colored Comics" is
   invisible to it, and one folder cannot carry 700 chapter ids in its
   name. Comic chapter paths must therefore resolve through the
   series, not the scan.
5. **`comic_series` has no folder column yet** (schema at
   `src/db.rs:908–926`: id, title UNIQUE COLLATE NOCASE, sort_title,
   author, description, cover_book_id, status, timestamps).
6. **Import order allows the decision before the copy.** The metadata
   tuple (series, number, title) is computed before folder creation
   and `fs::copy` (`src/epub.rs:85–230`); only the series-row
   creation happens after the copy (`:273`) and can move up.
7. **Folder-name sanitization is Bengali-safe and already accepts the
   owner's names** — "(Official)" and the en dash survive it today
   (his current folders prove it; the byte-cap logic is
   `src/paths.rs:87–170`).
8. **Covers are extracted at import** (`extract_comic_cover`) and
   written beside the file as `cover.<ext>` — per chapter, today
   spread across per-chapter folders.

## The design

1. **`comic_series.folder_name`** — new column (next schema version).
   Computed once when the series row is created:
   `sanitize_folder_text(series title, 120 bytes)`; if the name is
   already taken by another series' folder, append the series' own
   distinguishing suffix (the book-folder pattern: short id, then
   full). Stored, so it never changes and never has to be re-derived.
2. **Comic chapter paths resolve through the series.** One shared
   helper `comic_path(folder_name, file_name)` = 
   `library_dir().join(folder_name).join(file_name)`; the book-loading
   queries (`db.rs:1457/1496/1742`, `series.rs:322–348/406`) LEFT JOIN
   `comic_chapters → comic_series` and use it for comics, `book_dir`
   for everything else — same query, no extra roundtrip, so the grid
   stays as fast as today. Regular books are untouched.
3. **Chapter file naming:** the chapter number, zero-padded to 4
   digits, with the original extension: `0010.cbz`. Fractional
   chapters keep the fraction: `0010.5.cbz`. Unnumbered chapters, or
   two chapters claiming the same number, get the book's short uuid
   appended before the extension (`0010 8fa1afa3.cbz`) — the same
   disambiguation style folders already use. `cover_name` becomes
   `covers/<same stem>.<ext>`.
4. **Import:** compute the series name with the existing fallback
   chain *before* the copy; get-or-create the series row (its
   `folder_name` comes along); copy into
   `library/<folder_name>/<padded>.<ext>`; write the cover to
   `covers/`. Everything after the copy is unchanged.
5. **Migration** — one-time, background (`spawn_internal`), guarded by
   a settings flag, run at startup before the library grid loads:
   - per series: create `library/<folder_name>/` and `covers/`;
   - per chapter, ordered by `chapter_number`: `rename` the file and
     cover into place (same filesystem, so a rename, not a copy),
     update `file_name` + `cover_name`;
   - idempotent: a chapter whose `file_name` already matches the new
     shape is skipped, so a re-run costs nothing;
   - a chapter whose file is missing is logged and skipped, and the
     flag is **not** set — the pass retries on the next start;
   - old per-book folders are removed once empty;
   - reading positions, shelves, history, annotations: keyed by ids,
     untouched by construction.
6. **What deliberately does not change:** the in-app model (series,
   chapters, drawer, reader flow, progress), EPUB/PDF folders, the
   uuid folder scan for regular books, the remaster flow (it consumes
   the resolved `file_path`), and the owner's decided points —
   shelves stay virtual, static first-chapter cover in the grid with
   the current chapter's cover on the book page (the hub's series card
   already shows `cover_book_id`'s cover; per-chapter covers were only
   ever the drawer's chapter thumbnails and stay available at
   `covers/`).

## Steps

1. **db**: the `folder_name` column + schema migration; the
   `comic_path` helper wired into the five path-resolution sites;
   tests for resolution (comic vs regular) and folder-name collision.
2. **import**: series-folder destination, padded chapter file names,
   `covers/` destination; tests for padding, fractions, collisions,
   unnumbered fallbacks (reuse the `parse_comic_info` fixtures).
3. **migration**: the startup task with flag, idempotency and retry;
   tests against a temp library (build series + chapter files, migrate,
   assert paths and row updates, re-run = no-op, missing file = flag
   stays unset).
4. **verification pass**: hub cover paths, drawer thumbnails, reader
   open, remaster — all through the resolved `file_path`; fix what the
   pass finds.
5. **docs**: ROADMAP 2.22 close + changelog row; pitfalls for anything
   that bites.

## Open question for the owner (the only one)

**Chapter file naming.** I propose the short padded number —
`0010.cbz` — because the folder already says the series name; files
like "Naruto – Digital Colored Comics - 0010.cbz" would repeat it 700
times. If you prefer the long form, say so; it is a one-line change
either way.

**Answer: approved as proposed, 2026-10-02 ("no. it's alright. go with
the plan").**

---

## Implementation notes (2026-10-02, same day)

One mechanism deviation from the draft above, discovered while reading
the resolution sites: **there is no `comic_series.folder_name` column
and no LEFT JOIN.** A stored name that contains `/`
(`Naruto – Digital Colored Comics/0003.cbz`) resolves against the
library root; a bare name (`book.epub`) against the book's own uuid
folder — one rule, `paths::resolve_library_file`, and the separator is
a marker no other stored name can carry. This keeps every goal of the
draft (no extra query per book, id-keyed data untouched) while being
per-chapter atomic instead of per-series: file moves and the row's
stored names change in one step, so a half-migrated series is
impossible rather than merely guarded against. No schema change
follows; `SCHEMA_VERSION` stays 17.

Where each piece landed:

- `src/paths.rs` — `series_folder_name` (title sanitization, 110-byte
  cap), `comic_chapter_stem` (zero-padded 4, fraction kept, rendered
  from the shortest form of the whole number so `10.1` never becomes
  `10.10000038`), `resolve_library_file` (the separator rule);
  `sanitize_folder_text` made public.
- `src/db/series.rs` — `ensure_series_folder` (name + create, used by
  import) and `series_folder_name_for` (name only, used by the
  migration so a fileless series never materializes a folder); clash
  fallback `#<series id>` — `#` chosen because the folder scan parses
  trailing id tokens and a bare number could be mistaken for one. The
  series cover paths in `get_comic_series`/`list_comic_series` resolve
  through the new rule.
- `src/epub.rs` — the comic branch of `import_epub` resolves its
  series row before the copy, copies into `library/<Series>/` as
  `<stem>.<ext>` (short-id suffix on a name clash), writes the cover
  to `covers/`, and stores library-relative names. `replace_cover_bytes`
  writes a comic's replacement cover into `covers/` and stores the
  prefixed name.
- `src/comic_folders.rs` (new) — the startup pass `migrate_comic_library`
  plus `place_chapter`: move file, cover and sidecar, repoint the row,
  remove the emptied per-book folder. Idempotent by the stored name,
  self-healing (a move whose row update failed is adopted on the next
  pass), retried per chapter on failure.
- `src/db.rs` — the three book-loading sites resolve through the new
  rule; `delete_book` removes a comic chapter's own files and takes
  the series folder when the last chapter goes; `set_book_file_names`
  is the row-repoint primitive.
- `src/sidecar.rs` — a comic chapter's sidecar lives at
  `library/<Series>/.kalam/<uuid>.json` (hidden, travels with the
  library); the recovery survey counts books from the catalog instead
  of library folders, since folders no longer map one-to-one onto
  books.
- `src/thumbs.rs`, `src/db/annotations.rs`, `src/db/metadata.rs` —
  cover resolution through the same rule (thumbnail backfill, quotes
  list, cover restore on re-import).
- `src/folders.rs` — the book-folder align pass skips comics (they are
  the comic pass's to move; also removes the two-pass race).
- `src/app.rs` — the pass runs at startup after the folder pass, as a
  `spawn_internal` task; only the timing note reports movement.

Tests: stem padding and fractions (including the owner's real chapter
numbers 3/4/10/11/102), folder-name sanitization with his exact series
titles, the separator rule, a full CBZ import landing in its series
folder (real zip built with ComicInfo.xml, cover extracted, chapter
cataloged), placement with cover + sidecar + row repoint + old-folder
cleanup, idempotency, missing files leaving row and library alone, and
the same-number collision suffix.
