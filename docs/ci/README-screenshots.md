# Screenshots in CI — what they prove, and what they don't

## Why this exists

CI compiles the code. It has now been green through two bugs that were obvious
to a human in about ten seconds:

- Home's book covers never loaded at all — not slowly, never.
- Only the first two rows of the All-books grid ever filled in.

Both were pure "look at the screen" failures. No compiler, linter or unit test
can see a grey rectangle that should have been a book cover. This job takes
photographs so somebody (or the agent) can look.

## What runs

| Job | Blocks the build? | What it does |
|---|---|---|
| `build` | **Yes** | fmt, clippy `-D warnings`, tests, debug + release build |
| `screenshots` | No | 139-book library, headless sway, PNGs as artifacts |
| `scale` | No | 2,000-book library, peak memory |

The two new jobs are `continue-on-error: true` **on purpose**. They are a
diagnostic, not a gate. A flaky compositor must never block a correct code
change. `build` remains the only thing that can fail the run.

## The pieces

- `docs/ci/seed-library.py` — writes a synthetic library straight to the
  catalog schema under `XDG_DATA_HOME`. It refuses to run without that variable
  set, so it can never scribble on a real `~/.local/share/kalam`.
- `docs/ci/screenshot.sh` — starts headless sway, launches the release binary,
  waits, grabs PNGs with `grim`, samples the app's peak memory, and writes
  `report.txt`.
- `docs/ci/check-shot.py` — reads the PNGs and reports what fraction of pixels
  are strongly coloured. The seeded covers are saturated colours and the
  placeholder is grey, so this answers "did the covers load?" as a number
  rather than needing someone to squint at an image. It reports numbers and
  never fails: a threshold invented before we have seen real runs would be a
  guess, and a wrong threshold that fails good builds is worse than none.

## How the agent reads the results

The Arena sandbox **cannot download Actions artifacts** — the blob host is
unreachable from it, the same limitation that makes clippy failures get
committed to `ci-logs/`. So both jobs copy their `report.txt` into
`ci-logs/screenshots-latest.txt` and `ci-logs/scale-2000-latest.txt` and commit
it back to the branch, on success *and* failure.

The PNGs are for you. The committed text report is for the agent. It contains
the `[timing]` lines, the cover-colour percentages, peak memory, and any fatal
error with the sway or app log attached.

## Caveats — read these before believing a screenshot

**1. The renderer is not your renderer.** There is no GPU on a runner, so this
uses `GSK_RENDERER=cairo`. GTK's own maintainers describe the Cairo renderer as
a last-resort fallback that is missing features and is not tested daily.
Ordinary layout, text and CSS should be faithful. Anything involving non-affine
transforms or shaders may not be. **A screenshot here is evidence, not proof.**

**2. Fonts differ.** The runner gets `fonts-dejavu-core`, which is almost
certainly not what you have. Font metrics change line wrapping, so "does this
long title wrap to two lines?" can pass here and fail on your machine, or the
reverse. Layout questions that hinge on exact text width still need your eyes.

**3. Headless sway is not your sway.** One virtual 1600x1000 output, no tiling
session, no real input devices. It is much closer to your setup than Xvfb would
be — which matters, because the whole A1 dialog effort was about Sway
mistreating separate windows — but it is not the same thing.

**4. The reader may not render.** WebKit under software rendering is
unreliable. If the reader screenshot is blank, suspect the harness before the
app.

**5. Navigation is requested, not guessed** *(fixed 2026-09-04)*. The script
used to send Tab/Tab/Return because the app had no way to be told where to go.
That guessed the focus order, and the first real run guessed wrong: three
byte-identical screenshots of Home, and the job reported success. A visual
check that cannot distinguish "I navigated" from "I did nothing" is worth
nothing — the same rule as `pitfalls.md` §19.

The app now accepts `KALAM_ROUTE=<name>` (`all-books`, `settings`, `shelves`,
`analytics`, … — an unknown name prints the full list and is ignored rather
than fatal). Pages that need a book take its id after the name: `book-<id>`
opens the book page and `read-<id>` opens the engine reader, which is how the
reader gets exercised without the owner's desk. Set `ROUTE=` to change which
page is photographed; it defaults to `all-books`.

Three independent checks now have to agree before the report says navigation
was OK, because each catches a different failure:

| Check | Catches |
|---|---|
| `KALAM_ROUTE=… — navigating` in the log | binary predates the flag, or the name was rejected |
| `grid_build` emitted | the route was accepted but the page never rendered |
| `01-home.png` and `03-after-nav-settled.png` differ | everything else lied |

The third would have caught the original bug on its own. The evidence was
already in the report — three identical `md5sum`s — and was read past.

**6. Timing numbers from CI are worthless.** Shared, throttled VMs. The peak
memory number is trustworthy; the milliseconds are not. Real timings come from
running on your own machine with `KALAM_TIMING=1`.

## The honest limitation

If the same agent writes the code, writes the test, and judges the photograph,
that is marking its own homework. These images are strongest as a **regression**
check — compare against an image already agreed to be good — and weakest as an
approval of something new. They reduce how often a human has to look. They do
not remove the need.

## Reading the artifacts

Download the `screenshots` artifact from the run. Files:

- `01-home.png` — Home. Both strips must show real covers, not grey boxes.
- `02-after-nav.png` — just after navigating; some covers still filling is fine.
- `03-after-nav-settled.png` — **the important one.** Anything still grey here
  is a cover that is never coming.
- `sway.log`, `tree.json` — for when nothing appeared at all.
- `peak-rss.txt` — peak memory, in KB and MB.

`peak_rss_kb=0` means the app never started. In that case the screenshots are
meaningless and the log is the only useful artifact.

## Cost

The repository is private, so Actions minutes are metered. These jobs add
roughly 4-6 minutes per run, mostly rebuilding the release binary in a fresh
job. If that becomes annoying, the cheapest fix is to gate them on a label or
run them only on `workflow_dispatch`.
