# GUI smoke checks

The GUI smoke job checks that Kalam launches and can construct selected pages
under a headless Wayland session. It is diagnostic, not a visual acceptance
test and not a required CI gate.

## Routes exercised

The job runs `docs/ci/screenshot.sh` for each route (139 synthetic books
for the first two; the gallery needs no library to speak of and seeds 20):

| Route | What it opens |
|---|---|
| `read-1` | The EPUB reader with the first seeded EPUB. |
| `tasks` | The task manager's current empty/idle state. |
| `components` | The hidden redesign component gallery, twice: Terracotta Light and Dark on a tall 1600×3400 output. |
| `book-1` | The redesigned book details page for the first seeded EPUB, twice: Terracotta Light and Dark at 1600×1200. |

The seeder writes real EPUBs and covers under readable author/title folders
with short-id suffixes. It creates a minimal schema-version-1 catalog; Kalam
runs migrations when it opens the library.

For each route, the script captures the requested page, waits, captures it
again, and then launches Home for comparison. The repeated captures should be
identical when the page has settled. The route and Home captures should differ.
A difference proves navigation changed the screen; it does not prove every
control or piece of content is correct.

For `read-*` routes, the report also fails navigation if Kalam logs
`open epub failed`. Check the report and app log, not only the line saying the
route was accepted. The checked-in report reflects the last workflow run and
is refreshed by CI, not by local edits to the harness.

## Reading the reports

The job commits text reports to:

- `ci-logs/reader-latest.txt`
- `ci-logs/tasks-latest.txt`
- `ci-logs/components-light-latest.txt`
- `ci-logs/components-dark-latest.txt`
- `ci-logs/book-light-latest.txt`
- `ci-logs/book-dark-latest.txt`

The report includes route checks, timing spans, main-loop stalls, pixel
samples, screenshot fingerprints, app output and peak RSS. PNG images and
compositor logs stay in the temporary Actions workspace; the workflow does not
upload them as artifacts. The only uploaded artifact is the release binary
used by the smoke job. The exceptions are the gallery and the book page:
their settled frames in each mode are committed as
`ci-logs/components-terracotta-light.png`,
`ci-logs/components-terracotta-dark.png`,
`ci-logs/book-terracotta-light.png` and
`ci-logs/book-terracotta-dark.png`, because a redesign nobody can look at
is not verified and the agent cannot download artifacts.

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
- **Fonts:** the runner has DejaVu fonts; a user's installed fonts and bundled
  font configuration can change line wrapping.
- **Input:** synthetic pointer taps are not part of the required workflow.
  Headless input delivery has not been reliable enough to treat it as proof of
  reader gestures.
- **Timing:** shared-runner milliseconds are not a performance benchmark.
  Use `KALAM_TIMING=1` on the target desktop for timing decisions. Peak memory
  is reported as a diagnostic.
- **Screenshots:** because PNGs are not uploaded, the committed report is what
  an agent can inspect from the repository for most routes: fingerprints and
  metrics, not the image itself. The gallery and book-page frames are
  committed as PNGs (above), so those can and should be eyeballed directly —
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
