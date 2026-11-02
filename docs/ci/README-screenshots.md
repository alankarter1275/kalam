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
| `comic-read-22` + `COMIC_FIXTURE=1` | The comics reader on the second seeded chapter, including Chapters, Pages, Reading, saved presets, Image, Shortcuts and page-jump states. |
| `comic-series-book-22` + `COMIC_FIXTURE=1` | The comic-series drawer for that chapter's series, its Edit details dialog, the Find metadata search dialog, and a synthetic local Find review state staged by `DIALOG=find-comic-series-review`, so the metadata proof makes no provider request. |
| `pdf-read-21` + `PDF_FIXTURE=1` | The PDF reader on the seeded outlined document, including chrome, Contents, Pages, Search, Notes, Reading, Image/Invert, Shortcuts, page-jump and drawing states over seeded strokes, shapes, a text box and a sticky note. `PDF_FIXTURE_STATE` also seeds genuinely encrypted, damaged and missing variants; their password, wrong-password, unlocked, corrupt and missing captures exercise the whole-book open states. |
| `tasks` | The task manager's current empty/idle state. |
| `components` | The hidden redesign component gallery, twice: Terracotta Light and Dark on a tall 1600×3400 output. |
| `book-1` | The redesigned book details page for the first seeded EPUB, twice: Terracotta Light and Dark at 1600×1200. |
| `book-4` + `DIALOG=tags` | The book page's Tag dialog opened over it (`KALAM_DIALOG`), twice: Light and Dark. Book 4 is the first seeded book carrying three tags, so chips and ticked library rows render for real. |
| `book-1` + `DIALOG=annotations` | The Annotations dialog the same way, twice. The seeder writes no annotations, so these photograph the empty states. |
| `series-book-21` + `SERIES_FIXTURE=1` | The series page over the seeder's coherent fixture series (Book 1 finished, Book 2 at 42%, Book 3 unread, Book 4 only in the seeded listing cache): the strip in Light and Dark, plus the list view in Light via `SERIES_VIEW=list`. The route anchors on a book id because a series route's name/author do not belong in a shell variable. |
| `author-book-21` + `SERIES_FIXTURE=1` | The author page for the fixture series' author (`--author-fixture` in the seeder: cached bibliography plus one mark of each kind): the page in Light and Dark, the open-series panel, the Create shelf dialog, the author-scope Annotations dialog, and the Edit author / Merge author dialogs in Dark. |
| `authors` + `SERIES_FIXTURE=1` + `AUTHOR_FIXTURE=1` | The Authors index over the seeded library and cached author profile, in Light and Dark. |
| `settings` | The redesigned Settings page, 23 captures alternating Terracotta Light/Dark. `SETTINGS_PANE` picks the pane and `SETTINGS_ACTION` stages a drawn state (typeface dropdown open, restart/delete/remove confirms, shelf dropdown, dictionary drag, shortcut recording, notification panel with seeded events / empty / closed-bell unseen dot). The seeder's Settings fixtures supply the library registry, a watch folder (or none with `SETTINGS_EMPTY_WATCH=1`), Google Books key/country/rejected state, rejected MyAnimeList and present Comic Vine fixture credentials, EPUB `.orig` backups, and the four bundled dictionary rows (or none with `SETTINGS_EMPTY_DICTS=1`). |

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
- `ci-logs/comic-series-*-latest.txt`
- `ci-logs/pdf-*-latest.txt`
- `ci-logs/tasks-latest.txt`
- `ci-logs/keys-latest.txt` (the keystroke probe; its frames are committed as
  `ci-logs/keys-*.png`)
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
`ci-logs/author-annotations-terracotta-dark.png`,
`ci-logs/comics-*.png`, `ci-logs/comic-series-*.png` and
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

## The keystroke probe

`KEYS=1` is the one run that sends real input instead of asking the app to
pose. Everywhere else in this harness the app performs the action itself —
`KALAM_ROUTE` opens a page, `KALAM_DIALOG` opens a dialog, `KALAM_*_ACTION`
stages a drawn state — so a control whose wiring to its action has broken
still produces a perfect frame. This probe is the check that can notice.

It runs on `ROUTE=home` in the `pages` group and types nine of the app's own
default shortcuts (`default_binding` in `src/shortcuts.rs`) with `wtype`,
then asks three independent witnesses whether the app acted:

| # | Keys | Shortcut | The witness |
|---|---|---|---|
| 1 | `Alt+2` | The library | a page changed: `route_open:library`, or `page_cache_hit` |
| 2 | `Alt+1` | Home | a page changed |
| 3 | `Ctrl+,` | Open Settings | a page changed |
| 4 | `Ctrl+/` | The shortcut pane | the app logs `settings_fill:shortcuts` |
| 5 | `Alt+1` | Home again | a page changed |
| 6 | `Ctrl+F` | Search the library | a page changed — and it pushes the history stack |
| 7 | `Alt+Left` | Back | a page changed — now there is something to pop |
| 8 | `F11` | Full screen, and back | **sway** reports `fullscreen_mode`, not Kalam |
| 9 | `Ctrl+Q` | Quit | the process is gone |

Steps 1 and 2 are a **control pair**, not two more shortcuts. Their job is to
say whether the rest of the report means anything; see below.

Two of those witnesses are not what a first draft would reach for, and both
were chosen after a run proved the obvious witness wrong:

- **"A page changed", not "a route line appeared".** `route_open:` is logged
  inside `build_page`, and `build_page` is skipped when the page comes out of
  the cache. The app says so itself: *"The page cache's hit path never enters
  this function — a cached page is reparented, not rebuilt."* So the second
  time you visit a page, a route-only witness reports a working shortcut as a
  failure. Each navigation step now accepts either line and prints which one it
  saw.
- **`Ctrl+/` is witnessed by the tab filling.** Its handler swaps to Settings
  and *then* selects the Shortcuts tab — and the swap is a no-op when Settings
  is already open, so no route line is possible. `settings_fill:shortcuts`
  fires either way.

Steps 6 and 7 are a pair too. Every navigation shortcut calls `swap_page` with
`push_history=false`, and that branch is `history.clear()`, so after any of
them `Back` has nothing to pop and `AppMsg::Back` — `if let Some(prev) =
self.history.pop()` — is a correct no-op. `Ctrl+F` is the one default shortcut
that pushes, so it is the only way to give `Back` something to do. It also
leaves the search box focused, which is what step 7 then measures.

The last two matter most. `fullscreen_mode` comes from the compositor, so the
app cannot supply it by logging something optimistic, and a dead process is
proof no argument can soften. `Ctrl+Q` therefore runs last, because it ends
the run — which is also why the route is `home`: with `home` the script skips
its relaunch-and-compare step, so nothing after the probe needs the app alive.

Every one of these combos carries a modifier or is an F-key, so
`fires_while_typing` should let it through a focused text entry.

### What five runs taught

Runs 1 and 2 both reported 2 of 6, and neither result was about the shortcut
table.

**Run 1** had `Ctrl+F` first. It worked, and it is the one default shortcut
that leaves a text entry holding focus. Every modified chord after it did
nothing, while `F11` still worked and sway still saw the window go full
screen.

**Run 2** dropped `Ctrl+F` for `Alt+1` and led with `Ctrl+,` instead. `Ctrl+,`
worked from home — so run 1's failures were at least partly the focused search
box — but then `Alt+Left`, `Ctrl+/`, `Alt+1` and `Ctrl+Q` all did nothing, and
`F11` again was the only later key that landed. The app's own log confirms it
opened settings exactly once and then logged no route at all, with no
main-thread stall and nothing on stderr.

So the shape survived the change: **the first modified chord from home always
works, every modified chord after it does nothing, and the only later key that
works is the one carrying no modifier.** Two readings fit and they have
opposite conclusions:

- the app goes deaf to modified keys once the page has changed — a real bug;
- `wtype` stops applying modifiers after its first chord — this harness
  breaking, and it says nothing about Kalam.

`F11` cannot separate them, precisely *because* it has no modifier: it passes
under both readings. Hence the control pair. Two chords of the same kind, back
to back, each with its own witness and no text entry anywhere near them. If
chord 1 arrives and chord 2 does not, the verdict says **INCONCLUSIVE** and
points at the injector, because at that point no line below it measures the
app.

**Run 3 settled it, and the answer was neither.** Both chords passed, so the
injector keeps applying modifiers and the "app goes deaf" reading died with it.
Five of eight probes passed and, decisively, `Ctrl+Q` quit the app — so
keystrokes do reach it. The three failures were then all traced to this probe's
witnesses rather than to the app, by reading the app's own log and source:
`Back` was a correct no-op on an emptied history stack, `Ctrl+/` had actually
worked (`settings_fill:shortcuts` is in the log, and the frame after it differs
from the one before), and the second `Alt+1` had actually worked too — the last
line of the log is `page_cache_hit`.

**Run 4** used the corrected witnesses, reported 5 of 9 — and disagreed with
run 3 about *which* chords worked. `Alt+1` had passed and now failed; `Ctrl+/`
had passed and now failed; the second `Alt+1` had failed and now passed. The
missing lines never turned up late in the log, so they were dropped rather than
slow, and there was no main-thread stall and nothing on stderr.

The rhythm explains it, and it is exact. **All three dropped chords had been
sent within milliseconds of the previous chord succeeding, and every chord sent
after a pause arrived.** When a witness appears the await returns at once, so
the next `wtype` invocation builds a fresh virtual keyboard before the previous
one has finished releasing its modifier — and that chord is lost. After a
*failed* chord the await has already burned its whole timeout, which is why the
results alternated instead of clustering. Every chord now gets a settle before
it is sent (`KEY_SETTLE`, 1.5 s by default), which costs about fourteen seconds
a run.

**Run 5**, with the settle, reported **8 of 9**. Every chord that had been
flaky passed, including the two that runs 3 and 4 had disagreed about. The
settle was the whole of it.

The one failure left is `Alt+Left` after `Ctrl+F` has focused the library's
search box. It is a real result and not a witness problem: `Ctrl+F` pushed the
history, so there was something to pop, and the missing line never appeared
late. It is also **narrower than it first looks**, because `Ctrl+Q` is sent
afterwards with that same search box still focused and it *does* quit the app —
so modified chords are not being swallowed wholesale, and this is specific to
`Alt+Left`. The All Books page installs no `EventControllerKey` of its own,
which leaves the `GtkSearchEntry` as the thing claiming it. In plain terms:
**Back is dead while you are typing in the search box**, and
`fires_while_typing` says it should not be. Whether that is worth changing is
the owner's call, not this probe's.

One thing is still open and unexplained, in every run so far: **the second
`F11` does not leave full screen.** The handler calls `unfullscreen()` only when
`root.is_fullscreen()`, so under a headless compositor GTK may simply never
learn the state it is in. It is reported as a `NOTE` and is not counted as a
probe.

Three smaller things the same runs exposed, all now fixed:

- **Route counts were doubled.** The app prints `[timing] route_open:<label>`
  when a page opens and `[timing] frame_after:route_open:<label>` when it has
  painted. An unanchored grep matched both, so one navigation read as two and
  every baseline was inflated. The counting now anchors on `[timing]
  route_open:`.
- **The pictures lagged the claims.** A shot taken the instant the log line
  appeared caught the page before it drew, so run 1's fifth frame came out
  byte-identical to the one before it and seemed to contradict the log. Each
  probe shot now dwells two seconds first.
- **Four wait loops had no exit but a deadline computed by arithmetic.** If
  that arithmetic ever yields nothing, the deadline test errors instead of
  ending the wait and the loop spins forever — observed here while
  stub-testing. A hung CI step costs hours rather than seconds, so each loop
  now carries an iteration backstop.

Read the `keys: VERDICT` line before the individual results:

- **Quit worked** — keystrokes reach the app. Any `FAILED` line above it is a
  question about that one shortcut's wiring, not about input delivery.
- **INCONCLUSIVE** — chord 1 arrived and chord 2 did not. Nothing below the
  pair measures the app; settle the injector first. Run 3 did not report this,
  which is why the runs after it mean something.
- **Nothing worked, Quit included** — input did not arrive. That is the
  pointer's fate in the `TAP=1` probe in the script, a property of the harness
  rather than a pile of broken shortcuts. Do not go reading the wiring first.
- **Some worked** — input arrives; read the failures one by one.

What this does *not* prove: that clicking a button works. The `TAP=1` probe
established that synthetic pointer events do not reach the app under headless
sway, and nothing here changes that. The shortcut table and the actions
behind it are now covered; the widgets that call the same actions are still
covered only by a human hand.

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
  reader gestures. Synthetic *keystrokes* are a different mechanism and are
  probed by `KEYS=1` above — but only for the shortcut table. No click path is
  covered, so a button wired to nothing still passes every check here.
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

For the keystroke probe:

```bash
XDG_DATA_HOME=/tmp/kalam-ci-data \
  BOOKS=20 OUT=/tmp/kalam-ci-keys ROUTE=home KEYS=1 \
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
