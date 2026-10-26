# GUI smoke checks

The GUI smoke job checks that Kalam launches and can construct selected pages
under a headless Wayland session. It is diagnostic, not a visual acceptance
test and not a required CI gate.

The captures run in six parallel matrix groups — EPUB, comics, PDF,
general pages, and two Settings halves. Each group uploads its reports
and settled frames as a short-lived artifact; one final publish job
merges them and commits the familiar `ci-logs/` files, avoiding six
racing pushes to the same branch.

## Routes exercised

The job runs `docs/ci/screenshot.sh` for each route (139 synthetic books
for the first two; the gallery needs no library to speak of and seeds 20):

| Route | What it opens |
|---|---|
| `read-1` | The EPUB reader with the first seeded EPUB, including its chrome, panels, selection, dictionary and search proof states. |
| `comic-read-22` + `COMIC_FIXTURE=1` | The comics reader on the second seeded chapter, including Chapters, Pages, Reading, Image, Shortcuts and page-jump states. |
| `pdf-read-21` + `PDF_FIXTURE=1` | The PDF reader on the seeded outlined document, including chrome, Contents, Pages, Search, Notes, Reading, Image/Invert, Shortcuts, page-jump and drawing states over seeded strokes, shapes, a text box and a sticky note. `PDF_FIXTURE_STATE` also seeds genuinely encrypted, damaged and missing variants; their password, wrong-password, unlocked, corrupt and missing captures exercise the whole-book open states. |
| `tasks` | The task manager's current empty/idle state. |
| `components` | The hidden redesign component gallery, twice: Terracotta Light and Dark on a tall 1600×3400 output. |
| `book-1` | The redesigned book details page for the first seeded EPUB, twice: Terracotta Light and Dark at 1600×1200. |
| `book-4` + `DIALOG=tags` | The book page's Tag dialog opened over it (`KALAM_DIALOG`), twice: Light and Dark. Book 4 is the first seeded book carrying three tags, so chips and ticked library rows render for real. |
| `book-1` + `DIALOG=annotations` | The Annotations dialog the same way, twice. The seeder writes no annotations, so these photograph the empty states. |
| `series-book-21` + `SERIES_FIXTURE=1` | The series page over the seeder's coherent fixture series (Book 1 finished, Book 2 at 42%, Book 3 unread, Book 4 only in the seeded listing cache): the strip in Light and Dark, plus the list view in Light via `SERIES_VIEW=list`. The route anchors on a book id because a series route's name/author do not belong in a shell variable. |
| `author-book-21` + `SERIES_FIXTURE=1` | The author page for the fixture series' author (`--author-fixture` in the seeder: cached bibliography plus one mark of each kind): the page in Light and Dark, the open-series panel, the Create shelf dialog, the author-scope Annotations dialog, and the Edit author / Merge author dialogs in Dark. |
| `authors` + `SERIES_FIXTURE=1` + `AUTHOR_FIXTURE=1` | The Authors index over the seeded library and cached author profile, in Light and Dark. |
| `settings` | The redesigned Settings page, 23 captures alternating Terracotta Light/Dark. `SETTINGS_PANE` picks the pane and `SETTINGS_ACTION` stages a drawn state (typeface dropdown open, restart/delete/remove confirms, shelf dropdown, dictionary drag, shortcut recording, notification panel with seeded events / empty / closed-bell unseen dot). The seeder's Settings fixtures supply the library registry, a watch folder (or none with `SETTINGS_EMPTY_WATCH=1`), Google Books key/country/rejected state, EPUB `.orig` backups, and the four bundled dictionary rows (or none with `SETTINGS_EMPTY_DICTS=1`). |

The seeder writes real EPUBs and covers under readable author/title folders
with short-id suffixes. It creates a minimal schema-version-1 catalog; Kalam
runs migrations when it opens the library. Every capture runs under a fresh
disposable `XDG_CONFIG_HOME` (`screenshot.sh` force-assigns it), so prefs
and fixture writes can never leak between routes or from the runner's own
`~/.config`.

For each route, the script waits until the app has logged the route
and Sway has a toplevel window. It then applies a short minimum dwell
(five seconds for the three readers, less for ordinary pages, with
longer floors for proof actions the app itself schedules later or
transient toasts that must dismiss) and
polls until two coarsely identical frames say the page has stopped
materially changing. `SETTLE` is the maximum readiness budget rather
than a fixed delay; on timeout the latest frame is kept and the report
records that no stable pair was seen. Home is launched and settled the
same way for comparison. The route and Home captures should differ.
A difference proves navigation changed the screen; it does not prove every
control or piece of content is correct.

For `read-*` routes, the report also fails navigation if Kalam logs
`open epub failed`. Check the report and app log, not only the line saying the
route was accepted. The checked-in report reflects the last workflow run and
is refreshed by CI, not by local edits to the harness.

## Reading the reports

The job commits text reports to:

- `ci-logs/reader-latest.txt`
- `ci-logs/reader-*-latest.txt` for the EPUB proof states
- `ci-logs/comics-*-latest.txt`
- `ci-logs/pdf-*-latest.txt`
- `ci-logs/tasks-latest.txt`
- `ci-logs/components-light-latest.txt`
- `ci-logs/components-dark-latest.txt`
- `ci-logs/book-light-latest.txt`
- `ci-logs/book-dark-latest.txt`
- `ci-logs/book-tags-light-latest.txt`
- `ci-logs/book-tags-dark-latest.txt`
- `ci-logs/book-annotations-light-latest.txt`
- `ci-logs/book-annotations-dark-latest.txt`
- `ci-logs/series-light-latest.txt`
- `ci-logs/series-dark-latest.txt`
- `ci-logs/series-list-light-latest.txt`
- `ci-logs/author-{light,dark,panel-light,shelf-light,annotations-dark,edit-dark,merge-dark}-latest.txt`
- `ci-logs/authors-{light,dark}-latest.txt`
- `ci-logs/settings-*-latest.txt` (one per Settings capture, 23 today)

The report includes route checks, timing spans, main-loop stalls, pixel
samples, screenshot fingerprints, app output and peak RSS. Compositor
logs, seeded config homes and intermediate stability candidates stay
in each shard's temporary workspace. A shard uploads only its reports
and settled PNGs, with one-day retention, so the final publish job can
merge all six groups; those are internal handoff artifacts, not a
second public copy of the proof set. The exceptions committed for
direct inspection are the redesigned surfaces: the
gallery, the book page and its two dialogs, the series page, the author
page and its dialogs, and the Settings captures. Their settled frames
are committed as
`ci-logs/components-terracotta-{light,dark}.png`,
`ci-logs/book-terracotta-{light,dark}.png`,
`ci-logs/book-tags-terracotta-{light,dark}.png`,
`ci-logs/book-annotations-terracotta-{light,dark}.png`,
`ci-logs/series-terracotta-{light,dark}.png`,
`ci-logs/series-list-terracotta-light.png`,
`ci-logs/author-terracotta-{light,dark}.png`,
`ci-logs/author-panel-terracotta-light.png`,
`ci-logs/author-shelf-terracotta-light.png`,
`ci-logs/author-annotations-terracotta-dark.png` and
`ci-logs/settings-*.png`, because a redesign
nobody can look at is not verified and the agent cannot download artifacts.

A passing route check is not a visual verdict. Compare the app's logs and
fingerprints with what the selected route is expected to display. A reader
open error, identical route/Home images, missing screenshots or an early app
exit means the route is not proven.

## Pixel samples

`docs/ci/check-shot.py` reports the percentage of strongly coloured pixels and
non-black pixels. The seeded covers are saturated and their placeholders are
grey, so colour is a rough cover-loading proxy **only for a library-grid
screenshot**. Reader and task-manager pages can legitimately contain little
colour; a low colour percentage on those routes does not mean covers failed.
The script reports measurements and does not set a pass threshold.

## Limits

- **Software rendering:** CI uses Cairo for GTK and Pixman for headless Sway.
  This is not the same rendering path as a desktop with hardware acceleration.
  It is useful for launch and broad layout smoke checks, not pixel-perfect
  approval.
- **Fonts:** Kalam registers its bundled faces (DM Sans, Playfair Display,
  and the reader's Literata/Noto files) with fontconfig at startup, so the
  captures render in the designed faces; the runner's fallback chain still
  differs from a desktop's, and a user's installed fonts can change line
  wrapping.
- **Input:** synthetic pointer taps are not part of the required workflow.
  Headless input delivery has not been reliable enough to treat it as proof of
  reader gestures.
- **Timing:** shared-runner milliseconds are not a performance benchmark.
  Use `KALAM_TIMING=1` on the target desktop for timing decisions. Peak memory
  is reported as a diagnostic.
- **Screenshots:** the internal shard artifacts are still not reachable
  from the Arena sandbox, so the committed report is what an agent can
  inspect from the repository for most routes: fingerprints and
  metrics, not the image itself. The redesign frames listed above are
  committed as PNGs, so those can and should be eyeballed directly —
  remembering the gallery's 1600×3400 software-rendered output is a shape
  no user has.

## Run locally

Build Kalam first, then set a disposable data directory and select one route:

```bash
cargo build --release
XDG_DATA_HOME=/tmp/kalam-ci-data \
  BOOKS=20 OUT=/tmp/kalam-ci-reader ROUTE=read-1 \
  docs/ci/screenshot.sh
```

For the component gallery in one mode:

```bash
XDG_DATA_HOME=/tmp/kalam-ci-data \
  BOOKS=20 OUT=/tmp/kalam-ci-components ROUTE=components \
  THEME=terracotta-light OUTPUT_W=1600 OUTPUT_H=3400 \
  docs/ci/screenshot.sh
```

The script seeds `<XDG_DATA_HOME>/kalam` and requires a working Wayland
compositor plus `sway`, `grim`, `wtype` and Python 3. It writes screenshots,
logs and `report.txt` under `OUT`. Do not point `XDG_DATA_HOME` at a real
library.
