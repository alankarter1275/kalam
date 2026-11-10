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
| `home` | The Home page, twice: Terracotta Light and Dark at 1280×800. That is the design board's own size, and at that width the columns, the card and the 520px search bar sit at the numbers `docs/design/home-spec.md` gives, so the frame can be held against the board without rescaling. Twice more in Light: with the search box open over result groups, staged by `KALAM_HOME_SEARCH=dune` (a launch variable Home reads and nothing else does, because synthetic typing has never been trustworthy enough here to stage a photograph with), and whole, in a 1280×1600 window so the Updates timeline and the quote card sit above the fold instead of under it. |
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
- `ci-logs/home-light-latest.txt`
- `ci-logs/home-dark-latest.txt`
- `ci-logs/home-search-light-latest.txt`
- `ci-logs/home-tall-light-latest.txt`
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
`ci-logs/home-terracotta-{light,dark}.png`,
`ci-logs/home-search-terracotta-light.png`,
`ci-logs/home-tall-terracotta-light.png`,
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

Those paths are main's. A branch gets its own copy under
`ci-logs/branches/<branch>/` — every text report, plus a short curated list of
pictures: Home in both modes at the board's size, Home with its search box
open, Home whole in a taller window, and the keystroke probe's frame of Home
after a real Ctrl+f (`keys-home-search.png`) — the same box opened by a real
chord through a real compositor rather than by a launch variable.
The names are stable and overwritten on every run, so a branch carries one
copy of the current truth rather than a picture per commit, and main deletes
the whole directory on its first run after a merge. Add a frame to that list
in the workflow's branch publish step when a stage needs eyes on it: the
internal shard artifacts are unreachable from the sandbox, and a photograph
nobody can open proves nothing.

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

It runs on `ROUTE=home` in the `pages` group and types the app's own default
shortcuts (`default_binding` in `src/shortcuts.rs`) with `wtype` across
eleven steps, then asks three independent witnesses whether the app acted:

| # | Keys | Shortcut | The witness |
|---|---|---|---|
| 1 | `Alt+2` | The library | a page changed: `route_open:library`, or `page_cache_hit` |
| 2 | `Alt+1` | Home | a page changed |
| 3 | `Ctrl+,` | Open Settings | a page changed |
| 4 | `Ctrl+/` | The shortcut pane | the app logs `settings_fill:shortcuts` |
| 5 | `Alt+1` | Home again | a page changed |
| 6 | `Ctrl+F` | Home's own search field (B22) | the page logs `home_search:ctrl_f`, and **no page changes** — a `route_open` here would be the shell's binding running on Home, the regression this step catches |
| 6b | `Alt+2` | The library, to stand off Home | a page changed |
| 6c | `Ctrl+F` | Search the library — the shell's half of B22 | a page changed — and it pushes the history stack |
| 7 | `Alt+Left` | Back | a page changed — to the library, which is what 6c pushed |
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

Steps 6, 6c and 7 are a triple now, and they exist to hold one ruling (B22)
from both ends. Home keeps `Ctrl+F` while Home is showing, so step 6 sends it
from Home and expects the page's own witness line and no navigation at all;
the frame it takes shows the results box the chord opens. Step 6b then leaves
Home, because the shell's binding can only be proved from a page that does
not claim the chord, and 6c proves it there. Every navigation shortcut calls
`swap_page` with `push_history=false`, and that branch is `history.clear()`,
so after any of them `Back` has nothing to pop and `AppMsg::Back` —
`if let Some(prev) = self.history.pop()` — is a correct no-op. `Ctrl+F` is the
one default shortcut that pushes, so 6c is the only way to give `Back`
something to do, and what it pushed is the library — which is why step 7 now
expects the library back, where before B22 the chord was sent from Home and
Home was what came back. 6c also leaves the search box focused, which is what
step 7 then measures.

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

**Run 5**, with the settle, reported **8 of 9**, losing only `Alt+Left`.
**Run 6**, with the identical settle, reported **7 of 9** — and lost `Ctrl+,`
and `Ctrl+Q` instead, while `Alt+Left` passed. So the settle lowered the drop
rate and did not remove it, and the failures move between runs. Across all six,
**every one of the nine shortcuts has been witnessed firing at least once.**

That is why each chord is now sent twice. A binding that lands on the second
attempt is wired, and the first key simply never arrived — which is the only
distinction this probe exists to draw, and one a single attempt cannot make. The
report prints `OK on RETRY` for those, and the retry count with the score, so a
harness that is wearing out shows up as a climbing number rather than as a pile
of failures that look like the app breaking.

`Alt+Left` was the stubborn one through run 5, and the tidy story was that
`Ctrl+F` leaves the library's search box focused, so the entry swallows the
chord before the window's bubble-phase handler sees it. That story is
unsupported — every candidate was checked against source and ruled out:

- `gtktext.c` binds `Alt+Left` and `Alt+Right` only inside `#ifdef __APPLE__`,
  so on Linux the focused text widget has no claim on them.
- `gtkwindow.c`'s `add_arrow_bindings` binds arrows with no modifier and with
  `Ctrl`, never with `Alt`.
- `gtk_search_entry_is_keynav` does test the Alt mask, but it is a filter that
  says "this is navigation, not typing" — it does not stop the event.
- `src/pages/all_books.rs` installs no `EventControllerKey` at all.
- The shell table matches the chord correctly, and `fires_while_typing` returns
  true for any Alt binding.
- `Ctrl+Q` is sent afterwards with the same search box still focused, and it
  quits the app.

What is left is **delivery**, and one claim here has to be retracted. An
earlier version of this section concluded that `Alt+Left` must be a keysym the
injector cannot send, because it had failed in all five runs to that point while
`Alt+1` and `Alt+2` had passed in all five. **Run 6 passed `Alt+Left`** — same
search box focused, same injector, same settle — and dropped two chords that had
never given trouble before. Five samples of a flaky thing is not a pattern. No
app code was changed on the strength of any of it, and none should have been:
the source says nothing on this platform claims `Alt+Left`, and the app has now
been seen handling it. The human check in
[`docs/manual-test-checklist.md`](../manual-test-checklist.md) stays anyway,
because a harness that drops keys cannot prove a negative.

**Settled by the owner on 2026-11-03: it was the headless compositor, not the
app.** The *second* `F11` never worked in any of seven runs — the window went
full screen and did not come back — and this section previously called it the
only open candidate for a real defect. On real hardware every shortcut works,
including that one, first try. Under headless sway GTK never learns the window's
state, so the handler's `unfullscreen()` branch, which is guarded by
`root.is_fullscreen()`, is never taken and the second press asks to go full
screen again. The probe keeps reporting it as a `NOTE` rather than silently
dropping the check, so the report still shows what the headless environment does
— but it is a property of the runner, and it is not counted as a probe.

That makes seven runs, nine shortcuts, and **no app bugs at all**: every failure
this probe ever reported was its own. Read a low score as "the harness is not
yet asking properly", never as "a shortcut is broken".

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
- **`OK on RETRY`** — the binding is wired and the first key was dropped in
  transit. Not a failure, but not clean either: watch the retry count.
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
- **Screenshots:** the internal shard artifacts are not reachable from the
  Arena sandbox, so what an agent can inspect is what got committed. For
  most routes that is still the text report — fingerprints and metrics, not
  the image. The frames listed above are committed as PNGs on main, and a
  branch gets the short curated list under `ci-logs/branches/<branch>/`, so
  those can and should be eyeballed directly — remembering the gallery's
  1600×3400 software-rendered output is a shape no user has, and that
  Home's two frames are the board's own 1280×800.

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
