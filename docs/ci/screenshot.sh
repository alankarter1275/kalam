#!/usr/bin/env bash
# Run Kalam on a headless display and photograph it.
#
# Why this exists: CI has been green through two bugs a human spotted in ten
# seconds -- Home's covers never loaded, and only two rows of the grid ever
# filled. Compiling proves the code builds, not that anything appeared on
# screen. This step produces PNGs a person (or the agent) can look at.
#
# It also writes report.txt, a plain-text summary. That matters because the
# Arena sandbox CANNOT download Actions artifacts (the blob host is
# unreachable from it -- the same reason clippy logs are committed to
# ci-logs/). Pictures are for humans; report.txt is what the agent can read.
#
# Read the caveats in docs/ci/README-screenshots.md before trusting a green
# run here. In particular the software renderer is not the renderer you use,
# so this is evidence, not proof.
set -uo pipefail

OUT="${OUT:-ci-shots}"
BOOKS="${BOOKS:-139}"
# SETTLE is now a readiness *budget*, not a fixed sleep. The harness
# waits for the app window and route line, gives the route a short
# minimum dwell for asynchronous work to begin, and then looks for two
# coarsely-identical frames. SETTLE (plus a small slack in the script)
# remains the generous upper bound for a cold binary on a shared
# runner: a screenshot taken too early shows an empty window and looks
# exactly like a bug, so timing out still photographs the latest frame.
SETTLE="${SETTLE:-25}"
MIN_SETTLE="${MIN_SETTLE:-}"
STABLE_MAX_CHANGED_PCT="${STABLE_MAX_CHANGED_PCT:-0.5}"
# Headless output size. The default matches a laptop-ish window. A tall
# route (the component gallery is ~3,000 px of stacked sections) can ask
# for a taller output instead of synthetic scrolling, which headless sway
# does not deliver reliably (see the TAP probe's status note below). The
# shot then shows the whole page in one frame, at the cost of a window
# shape no user has -- fine for an artboard, wrong for judging a real page,
# so real routes keep the default.
OUTPUT_W="${OUTPUT_W:-1600}"
OUTPUT_H="${OUTPUT_H:-1000}"

mkdir -p "$OUT"
REPORT="$OUT/report.txt"
: > "$REPORT"

# Everything interesting goes to BOTH the console and report.txt.
say() { echo "$*" | tee -a "$REPORT"; }

say "=== kalam screenshot run ==="
say "books=$BOOKS out=$OUT settle=$SETTLE output=${OUTPUT_W}x${OUTPUT_H}"

export XDG_DATA_HOME="${XDG_DATA_HOME:-/tmp/kalam-ci-data}"
# Global prefs and the libraries registry live under XDG_CONFIG_HOME.
# Give each screenshot invocation a fresh one under OUT, unconditionally:
# the seeded catalog is rebuilt on every run, so sharing a real config
# across runs lets one run's bundled-dictionary markers or library
# registry silently change the next run's supposedly fresh library.
# (An earlier "unless the caller set one" guard never fired in CI —
# the runner image exports XDG_CONFIG_HOME=/home/runner/.config, which
# the Data and export capture then photographed as the app-settings
# path. Nothing in the workflow sets it deliberately.)
case "$OUT" in
  /*) export XDG_CONFIG_HOME="$OUT/config-home" ;;
  *) export XDG_CONFIG_HOME="$PWD/$OUT/config-home" ;;
esac
rm -rf "$XDG_CONFIG_HOME"
mkdir -p "$XDG_CONFIG_HOME"
say "config home: $XDG_CONFIG_HOME"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/tmp/kalam-ci-run}"
mkdir -p "$XDG_RUNTIME_DIR"
chmod 700 "$XDG_RUNTIME_DIR"

# No GPU on a runner. GSK's Cairo renderer is the documented software
# fallback; without this GTK4 tries GL, fails, and may render nothing.
export GSK_RENDERER="${GSK_RENDERER:-cairo}"
export LIBGL_ALWAYS_SOFTWARE=1
export GDK_BACKEND=wayland
export KALAM_TIMING=1
# GTK will happily run for ever waiting for a display that is not coming;
# these make failures loud instead of silent.
export G_MESSAGES_DEBUG="${G_MESSAGES_DEBUG:-}"

BIN="$PWD/target/release/kalam"
if [ ! -x "$BIN" ]; then
  say "FATAL: no binary at $BIN"
  exit 0   # never fail the build from here; the report says what happened
fi
say "binary: $(ls -la "$BIN" | awk '{print $5" bytes"}')"

say ""
say "=== seeding $BOOKS books ==="
# SERIES_FIXTURE adds the seeder's one coherent series (3 owned of a
# cached 4) after the generic books, so `series-book-$((BOOKS+1))` opens
# the series page's drawn reading state with no network fetch.
SERIES_FIXTURE="${SERIES_FIXTURE:-}"
SEED_ARGS=(--books "$BOOKS")
if [ -n "$SERIES_FIXTURE" ]; then
  SEED_ARGS+=(--series-fixture)
fi
# AUTHOR_FIXTURE adds the same author's two owned standalones, her
# cached bibliography and three marks across her books, so
# `author-book-$((BOOKS+1))` opens the author page with both groups,
# highlights and facts populated — still no network fetch (the page
# never fetches on open).
AUTHOR_FIXTURE="${AUTHOR_FIXTURE:-}"
if [ -n "$AUTHOR_FIXTURE" ]; then
  SEED_ARGS+=(--author-fixture)
fi
# READER_FIXTURE plants the three-word dictionary (bank/river/shore) the
# reader popover frames look up; the bundled packs install at startup
# and land after the snapshot, so a capture cannot wait for them.
READER_FIXTURE="${READER_FIXTURE:-}"
if [ -n "$READER_FIXTURE" ]; then
  SEED_ARGS+=(--reader-fixture)
fi
# PDF_FIXTURE adds the seeder's real outlined PDF after the
# generic books, so `pdf-read-$((BOOKS+1))` opens page 4 as a
# bookmarked 4-5 spread — the PDF reader's drawn state.
PDF_FIXTURE="${PDF_FIXTURE:-}"
if [ -n "$PDF_FIXTURE" ]; then
  SEED_ARGS+=(--pdf-fixture)
fi
# PDF_NOTES_EMPTY photographs Notes before anything has been kept:
# the same fixture with no highlights, bookmarks or saved words.
PDF_NOTES_EMPTY="${PDF_NOTES_EMPTY:-}"
if [ -n "$PDF_NOTES_EMPTY" ]; then
  SEED_ARGS+=(--pdf-notes-empty)
fi
# PDF_FIXTURE_STATE chooses the same book with an encrypted,
# damaged or absent file, for the whole-book open-state frames.
PDF_FIXTURE_STATE="${PDF_FIXTURE_STATE:-}"
if [ -n "$PDF_FIXTURE_STATE" ]; then
  SEED_ARGS+=(--pdf-fixture-state "$PDF_FIXTURE_STATE")
fi
# COMIC_FIXTURE adds the seeder's two-chapter CBZ series after the
# generic books, so `comic-read-$((BOOKS+2))` opens chapter 2 at
# page 4 with a bookmark on it — the comics reader's drawn state.
COMIC_FIXTURE="${COMIC_FIXTURE:-}"
if [ -n "$COMIC_FIXTURE" ]; then
  SEED_ARGS+=(--comic-fixture)
fi
# SETTINGS_FIXTURE gives the Settings panes the populated states a bare
# library cannot show: known/missing libraries, shelves and a watch rule,
# rejected Google Books key state, EPUB original backups, and the four
# bundled dictionary packs as catalog rows. The empty-dicts variant
# leaves that table empty instead; both suppress the startup install,
# which finishes after the Settings snapshot is taken.
SETTINGS_FIXTURE="${SETTINGS_FIXTURE:-}"
SETTINGS_EMPTY_DICTS="${SETTINGS_EMPTY_DICTS:-}"
if [ -n "$SETTINGS_FIXTURE" ]; then
  SEED_ARGS+=(--settings-fixture)
fi
if [ -n "$SETTINGS_EMPTY_DICTS" ]; then
  SEED_ARGS+=(--settings-empty-dicts)
fi
if ! python3 docs/ci/seed-library.py "${SEED_ARGS[@]}" 2>&1 | tee -a "$REPORT"; then
  say "FATAL: seeding failed"
  exit 0
fi

say ""
say "=== starting headless sway ==="
export WLR_BACKENDS=headless
export WLR_LIBINPUT_NO_DEVICES=1
export WLR_RENDERER=pixman          # software renderer for wlroots
export WLR_HEADLESS_OUTPUTS=1

SWAY_CONF="$(mktemp)"
cat > "$SWAY_CONF" <<'EOF'
# `xwayland disable` is load-bearing, not tidiness. Without it sway tries to
# start Xwayland, cannot find the binary on a runner, and treats that as fatal
# -- which is exactly how the first two screenshot runs died. Kalam is a native
# Wayland (GTK4) app and never needs X11, so there is nothing to lose here.
xwayland disable
default_border none
focus_follows_mouse no
EOF
# Appended, not interpolated: the block above is a quoted heredoc because it
# contains backticks, so $OUTPUT_W would not expand inside it.
echo "output HEADLESS-1 resolution ${OUTPUT_W}x${OUTPUT_H}" >> "$SWAY_CONF"

say "sway binary: $(command -v sway || echo MISSING) $(sway --version 2>&1 | head -1)"

sway --config "$SWAY_CONF" > "$OUT/sway.log" 2>&1 &
SWAY_PID=$!

# Find sway's IPC socket and export SWAYSOCK ourselves.
#
# This is not belt-and-braces, it is the fix for the second failure: sway
# names its socket after its own pid and exports SWAYSOCK to processes *it*
# launches. This script is sway's parent, not its child, so it inherits
# nothing -- swaymsg then has no idea where to connect and reports the same
# "cannot connect" whether sway is healthy or dead. The first run showed
# exactly that: an empty sway.log (no errors at all) next to "sway never came
# up", which is the signature of a running compositor we simply could not
# talk to.
for _ in $(seq 1 300); do
  if [ -z "${SWAYSOCK:-}" ]; then
    CANDIDATE="$(ls -t "$XDG_RUNTIME_DIR"/sway-ipc.*.sock 2>/dev/null | head -1)"
    [ -n "$CANDIDATE" ] && export SWAYSOCK="$CANDIDATE"
  fi
  if [ -n "${SWAYSOCK:-}" ] && swaymsg -t get_version >/dev/null 2>&1; then
    break
  fi
  # A dead compositor will never produce a socket; stop waiting 30s for it.
  if ! kill -0 "$SWAY_PID" 2>/dev/null; then
    break
  fi
  sleep 0.1
done

say "SWAYSOCK=${SWAYSOCK:-<none found>}"
if ! swaymsg -t get_version >/dev/null 2>&1; then
  # Distinguish the two cases explicitly. Reporting "sway never came up" for
  # a sway that is alive and well cost a whole round-trip.
  if kill -0 "$SWAY_PID" 2>/dev/null; then
    say "FATAL: sway IS RUNNING but its IPC socket was unreachable."
    say "sockets present in $XDG_RUNTIME_DIR:"
    ls -la "$XDG_RUNTIME_DIR" 2>&1 | sed 's/^/  /' | tee -a "$REPORT"
  else
    say "FATAL: the sway process exited."
  fi
  say "sway log (${OUT}/sway.log):"
  if [ -s "$OUT/sway.log" ]; then
    sed 's/^/  /' "$OUT/sway.log" | tee -a "$REPORT"
  else
    say "  (empty -- sway logged nothing, which usually means it started fine)"
  fi
  exit 0
fi
say "sway up: $(swaymsg -t get_version -r | head -c 120)"

# Find the socket sway just created and point clients at it. Without this the
# app inherits no WAYLAND_DISPLAY, finds no compositor, and exits immediately
# -- which would look identical to a rendering bug in the screenshots.
if [ -z "${WAYLAND_DISPLAY:-}" ]; then
  SOCK="$(ls -t "$XDG_RUNTIME_DIR"/wayland-* 2>/dev/null \
          | grep -v '\.lock$' | head -1)"
  if [ -n "$SOCK" ]; then
    export WAYLAND_DISPLAY="$(basename "$SOCK")"
  fi
fi
say "WAYLAND_DISPLAY=${WAYLAND_DISPLAY:-<unset>}"
if [ -z "${WAYLAND_DISPLAY:-}" ]; then
  say "FATAL: sway is running but published no wayland socket in $XDG_RUNTIME_DIR"
  ls -la "$XDG_RUNTIME_DIR" | sed 's/^/  /' | tee -a "$REPORT"
  swaymsg exit >/dev/null 2>&1 || true
  exit 0
fi

shot() { # shot <name>
  local name="$1"
  if grim "$OUT/$name.png" 2>>"$OUT/grim.log"; then
    say "  shot: $name ($(stat -c%s "$OUT/$name.png") bytes)"
  else
    say "  shot FAILED: $name -- $(tail -1 "$OUT/grim.log" 2>/dev/null)"
  fi
}

# (The Tab/Tab/Return `key()` helper was removed on 2026-09-04: navigation
# is requested with KALAM_ROUTE now, so nothing needs synthetic keystrokes.)

# Sample the app's own memory. `/usr/bin/time` cannot help here: sway starts
# kalam, so it is not a child of this script and its RSS is never reported.
# Peak RSS is the one number from a CI runner worth trusting -- wall-clock
# timings on a shared VM are noise, but memory is memory. It is also the
# number that decides whether the unbounded cover cache actually matters.
PEAK_KB=0
sample_rss() {
  local pid rss
  pid="$(pgrep -n -x kalam 2>/dev/null || true)"
  [ -z "$pid" ] && return 0
  rss="$(awk '/^VmRSS:/ {print $2}' "/proc/$pid/status" 2>/dev/null || true)"
  [ -z "$rss" ] && return 0
  [ "$rss" -gt "$PEAK_KB" ] && PEAK_KB="$rss"
  return 0
}

now_s() {
  date +%s
}

app_window_seen() {
  swaymsg -t get_tree 2>/dev/null | grep -q '"app_id"'
}

route_logged() { # route_logged <log> <route>
  grep -q "KALAM_ROUTE=$2" "$1" 2>/dev/null
}

# Poll instead of assuming that a fixed number of seconds is either
# necessary or sufficient. Success needs both halves of the evidence:
# the app says it accepted the route, and the compositor has a real
# toplevel for it. The caller still takes diagnostic screenshots on a
# timeout; a blank or missing signal should be visible in the report,
# not hidden by an early exit.
wait_for_app_ready() { # wait_for_app_ready <pid> <log> <route> <timeout>
  local pid="$1" log="$2" route="$3" timeout="$4"
  local start elapsed
  start="$(now_s)"
  while :; do
    if ! kill -0 "$pid" 2>/dev/null; then
      return 1
    fi
    sample_rss
    if route_logged "$log" "$route" && app_window_seen; then
      return 0
    fi
    elapsed=$(( $(now_s) - start ))
    [ "$elapsed" -ge "$timeout" ] && return 2
    sleep 0.25
  done
}

wait_until_elapsed() { # wait_until_elapsed <start-seconds> <minimum-seconds> <pid>
  local start="$1" minimum="$2" pid="$3"
  while [ $(( $(now_s) - start )) -lt "$minimum" ]; do
    if ! kill -0 "$pid" 2>/dev/null; then
      return 1
    fi
    sample_rss
    sleep 0.25
  done
  return 0
}

frame_stable() { # frame_stable <a.png> <b.png>
  if cmp -s "$1" "$2"; then
    return 0
  fi
  python3 docs/ci/check-shot.py --compare "$1" "$2" \
    "$STABLE_MAX_CHANGED_PCT" >/dev/null 2>&1
}

capture_candidate() { # capture_candidate <path>
  grim "$1" 2>>"$OUT/grim.log"
}

# Starting from an already-photographed reference frame, poll once a
# second until the next frame is coarsely the same picture or the
# deadline arrives. The target always receives the latest candidate:
# on a genuinely animated or unusually slow page, the newest frame is
# still better evidence than refusing to publish one at all.
wait_for_stable_frame() { # wait_for_stable_frame <pid> <reference> <target> <deadline>
  local pid="$1" target="$3" deadline="$4"
  local previous="$OUT/.stable-previous.png"
  local candidate="$OUT/.stable-candidate.png"
  if ! cp "$2" "$previous" 2>/dev/null; then
    capture_candidate "$target" || true
    return 1
  fi

  while [ "$(now_s)" -lt "$deadline" ]; do
    if ! kill -0 "$pid" 2>/dev/null; then
      cp "$previous" "$target" 2>/dev/null || true
      return 1
    fi
    sample_rss
    sleep 1
    if ! capture_candidate "$candidate"; then
      cp "$previous" "$target" 2>/dev/null || true
      return 1
    fi
    if frame_stable "$previous" "$candidate"; then
      cp "$candidate" "$target"
      rm -f "$previous" "$candidate"
      return 0
    fi
    mv "$candidate" "$previous"
  done

  cp "$previous" "$target" 2>/dev/null || true
  rm -f "$previous" "$candidate"
  return 2
}

say ""
say "=== launching kalam ==="
# Run it directly rather than via `swaymsg exec`, so we own the process and
# can read its stderr. KALAM_TIMING output lands in the log, which is how the
# agent sees grid_build / covers_queued without downloading an artifact.
#
# ROUTE names a page for the app to open on its own (KALAM_ROUTE, added
# 2026-09-04). Previously this script sent Tab/Tab/Return and hoped the focus
# order was what it guessed; when it was not, the run photographed Home three
# times and still reported success.
ROUTE="${ROUTE:-all-books}"
say "requested route: $ROUTE"
# THEME names a palette/mode choice id for the app to apply for this run
# only (KALAM_THEME, not persisted). The app prints the resolved id, and
# that line lands in the report's app-stderr section -- a typo totalises
# to the default theme, so the printed id, not this request, is the
# evidence of which theme was photographed.
THEME="${THEME:-}"
say "requested theme: ${THEME:-<saved/default>}"
# DIALOG asks the loaded page to open one of its dialogs over itself
# (KALAM_DIALOG values include book tags/annotations, author edit/merge,
# and comic-series edit/find/find-review). No route reaches a dialog,
# so without this the loop could photograph every page and still never
# see the dialogs those pages own.
DIALOG="${DIALOG:-}"
say "requested dialog: ${DIALOG:-<none>}"
# SERIES_VIEW asks the series page to open on its strip or list view
# (KALAM_SERIES_VIEW) without writing the per-series pref, so both views
# can be photographed against the same seeded library.
SERIES_VIEW="${SERIES_VIEW:-}"
say "requested series view: ${SERIES_VIEW:-<remembered>}"
# SETTINGS_PANE preselects one of Settings' decision-144 panes. The route
# alone always opens Appearance, so without this pass-through ten of the
# eleven redesigned panes could never be photographed.
SETTINGS_PANE="${SETTINGS_PANE:-}"
say "requested settings pane: ${SETTINGS_PANE:-<default>}"
# READER_ACTION asks the EPUB reader to stand its summoned chrome up at
# open (KALAM_READER_ACTION=chrome): the redesigned page opens bare, so
# a proof frame of the Back+Bubble unit would otherwise need a pointer.
READER_ACTION="${READER_ACTION:-}"
say "requested reader action: ${READER_ACTION:-<none>}"
# COMIC_ACTION is the same ask for the comics reader
# (KALAM_COMIC_ACTION: chrome, chapters, pages, reading, presets,
# image, shortcuts, jump, jump-range).
COMIC_ACTION="${COMIC_ACTION:-}"
say "requested comic action: ${COMIC_ACTION:-<none>}"
# PDF_ACTION is the same ask for the PDF reader (KALAM_PDF_ACTION:
# chrome, contents, contents-pinned, pages, notes, notes-words,
# notes-bookmarks, notes-empty, search-results, reading, shortcuts,
# jump, jump-range, drawing, drawing-selected, drawing-shape,
# drawing-text, drawing-sticky).
PDF_ACTION="${PDF_ACTION:-}"
say "requested PDF action: ${PDF_ACTION:-<none>}"
say "settings fixture: ${SETTINGS_FIXTURE:-0} empty-dicts=${SETTINGS_EMPTY_DICTS:-0}"
# WINDOWED controls the A0 step 6 windowed grid (build only the cards on
# screen). It became the app's default on 2026-09-04, so the app flag is now
# the negative one, KALAM_NO_WINDOWED_GRID.
#
# This used to be inferred from the output directory. The reason was a
# comparison job: its baseline run passed nothing and its windowed run passed
# WINDOWED=1, and once windowed became the default, "nothing" would have meant
# windowed too -- both runs would have measured the same thing and published a
# green, meaningless comparison. That is the pitfalls §21 failure exactly,
# and naming the directory was how the two runs were told apart.
#
# That job was deleted on 2026-09-18. The question it answered had been
# settled on 2026-09-04 and its answer is committed in
# ci-logs/scale-2000-comparison.txt: windowed 247 MB / 7.1 ms against the old
# build-every-card grid at 352 MB / 139.7 ms. The heuristic outlived the only
# thing that needed it, and its fallback branch -- a plain `ci-shots-*` path
# means the old grid -- then applied to every one of the 138 captures in
# ci.yml, all of which use that naming. So the whole screenshot suite
# photographed a grid the app had stopped building eight weeks earlier, and
# the committed reports described it as current. Nothing failed, because
# nothing checked: the reports recorded "windowed grid: 0" faithfully.
#
# The default is now what the app actually does. An explicit WINDOWED= still
# wins, so a future comparison run can ask for the legacy grid by name.
if [ -n "${WINDOWED:-}" ]; then
  :
else
  WINDOWED=1
fi
if [ "$WINDOWED" = "0" ]; then
  NO_WINDOWED=1
else
  NO_WINDOWED=0
fi
say "windowed grid: $WINDOWED (KALAM_NO_WINDOWED_GRID=$NO_WINDOWED)"

# Readers do real asynchronous work after their route has opened --
# opening a book, decoding a comic page, rendering a PDF spread and
# applying the screenshot action that proof frame exists to show. A
# five-second floor lets that work get started before stability is
# judged; ordinary pages and Settings need only enough time for their
# staged dialog or pane action to land. Callers can override either
# bound with MIN_SETTLE/SETTLE when a future route proves slower.
if [ -z "$MIN_SETTLE" ]; then
  case "$ROUTE" in
    read-*|comic-read-*|pdf-read-*) MIN_SETTLE=5 ;;
    components) MIN_SETTLE=3 ;;
    *) MIN_SETTLE=2 ;;
  esac
  # Six EPUB proof states are deliberately scheduled by the app nine
  # seconds after reader construction (the annotation tap follows
  # another 1.2 seconds later), so frame stability before then only
  # proves the bare page is stable. Give those actions time to exist.
  case "$READER_ACTION" in
    dict|dict-missing|search|search-missing|chip|annotation)
      MIN_SETTLE=13
      ;;
  esac
  # The selected-drawing popover is scheduled the same way and for
  # the same reason, as are the Pages Edit proof states.
  case "$PDF_ACTION" in
    drawing-selected|drawing-text|drawing-sticky|pages-edit|pages-edit-drag|pages-edit-delete|pages-edit-unsaved)
      MIN_SETTLE=13
      ;;
  esac
  # The notification proof states seed their events through the live
  # notifier at 0.9 seconds; the seeded error toast then lives for
  # nine seconds. The frames are of the history panel and bell dot,
  # not of that transient toast stack, so let it dismiss first.
  case "$DIALOG" in
    notifications|notifications-dot)
      MIN_SETTLE=11
      ;;
  esac
fi
MAX_SETTLE=$((SETTLE + 10))
if [ "$MAX_SETTLE" -lt $((MIN_SETTLE + 2)) ]; then
  MAX_SETTLE=$((MIN_SETTLE + 2))
fi
say "readiness: min=${MIN_SETTLE}s max=${MAX_SETTLE}s stable_max_changed=${STABLE_MAX_CHANGED_PCT}%"

APP_START="$(now_s)"
KALAM_ROUTE="$ROUTE" KALAM_THEME="$THEME" KALAM_DIALOG="$DIALOG" KALAM_SERIES_VIEW="$SERIES_VIEW" KALAM_SETTINGS_PANE="$SETTINGS_PANE" KALAM_READER_ACTION="$READER_ACTION" KALAM_COMIC_ACTION="$COMIC_ACTION" KALAM_PDF_ACTION="$PDF_ACTION" KALAM_NO_WINDOWED_GRID="$NO_WINDOWED" "$BIN" > "$OUT/kalam.log" 2>&1 &
APP_PID=$!

wait_for_app_ready "$APP_PID" "$OUT/kalam.log" "$ROUTE" "$SETTLE"
READY_STATUS=$?
if [ "$READY_STATUS" -eq 1 ] || ! kill -0 "$APP_PID" 2>/dev/null; then
  say "FATAL: kalam exited during startup. Its output:"
  sed 's/^/  /' "$OUT/kalam.log" | tail -40 | tee -a "$REPORT"
  swaymsg exit >/dev/null 2>&1 || true
  exit 0
elif [ "$READY_STATUS" -eq 2 ]; then
  say "readiness: route/window signal not seen within ${SETTLE}s; photographing anyway"
else
  say "readiness: route and window seen after $(( $(now_s) - APP_START ))s"
fi

WINDOWS="$(swaymsg -t get_tree | grep -c '"app_id"' || true)"
say "toplevel windows seen: $WINDOWS"
swaymsg -t get_tree > "$OUT/tree.json" 2>/dev/null || true

# NOTE the file name. With KALAM_ROUTE the app opens on the requested page, so
# this first shot is already "$ROUTE" -- it is NOT Home. Naming it 01-home
# (as this script did until 2026-09-04) made the report lie twice over: it
# described the wrong page, and it made the identical-fingerprint check look
# like a navigation failure when the three shots were correctly the same page.
shot "01-$ROUTE"

# Focus is worth setting even though nothing needs keystrokes now: some GTK
# paint paths differ on an unfocused window under a headless compositor, and
# an unfocused window is not what a user sees.
swaymsg '[app_id=".*"] focus' >/dev/null 2>&1 \
  || swaymsg focus >/dev/null 2>&1 || true
say "focused: $(swaymsg -t get_tree | grep -c '"focused": true' || echo 0)"

# The old sequence then slept for another fixed 18 seconds, on the
# theory that covers arrive in the background and anything still grey
# after that is never coming. Frame stability expresses that theory
# directly: photograph once the minimum dwell has elapsed, then keep
# looking until the picture stops materially changing (or the budget
# expires and the latest frame is used).
wait_until_elapsed "$APP_START" "$MIN_SETTLE" "$APP_PID" || true
shot "02-$ROUTE-settling"
STABLE_DEADLINE=$((APP_START + MAX_SETTLE))
if wait_for_stable_frame "$APP_PID" "$OUT/02-$ROUTE-settling.png" \
    "$OUT/03-$ROUTE-settled.png" "$STABLE_DEADLINE"; then
  say "  shot: 03-$ROUTE-settled (stable after $(( $(now_s) - APP_START ))s)"
else
  say "  shot: 03-$ROUTE-settled (latest frame; no stable pair within ${MAX_SETTLE}s)"
fi

# --- does a tap on the page turn it? (TAP=1) -------------------------------
#
# This is the one reader behaviour a screenshot cannot see. Kalam removed
# tap-to-look-up before the engine swap, so a tap on text must fall through
# to the widget's page-turn zones -- the widget only claims taps when the
# host connects a word handler, and Kalam does not. If that regressed, the
# page would sit there and every other check would still pass.
#
# A tap is a press and a release in the same place, and the honest signal is
# "did the pixels change". The pointer is placed from the window's own
# geometry, not the output size: the two are equal only when the window is
# fullscreen, and a tap outside the window is indistinguishable from a tap
# the app ignored -- exactly the false negative this check exists to avoid.
#
# STATUS, 2026-09-11: this probe does NOT work in CI and is off by default.
# Run on the reader route it reports "window: 1600x1000 at 0,0" -- a
# fullscreen window, so the coordinates are right -- and then no change at
# 0.75/0.25/0.50 of the width, with both a one-second hold and an 80 ms one.
# Nothing changes at any position, including the centre, which the reader
# does react to for other reasons; the simplest explanation is that
# `seat ... cursor press` events do not reach the app under a headless sway
# (no libinput devices). Do not read "no change" as a defect: it is a
# property of the harness. The gesture is verified by hand instead.
if [ "${TAP:-0}" = "1" ]; then
  say ""
  say "=== tap check (TAP=1) ==="
  SEAT="$(swaymsg -t get_seats 2>/dev/null \
    | python3 -c 'import json,sys; print(json.load(sys.stdin)[0]["name"])' 2>/dev/null \
    || echo seat0)"
  GEOM="$(swaymsg -t get_tree 2>/dev/null | python3 -c '
import json, sys


def walk(node):
    yield node
    for child in node.get("nodes", []) + node.get("floating_nodes", []):
        yield from walk(child)


best = None
for node in walk(json.load(sys.stdin)):
    app_id = node.get("app_id") or ""
    rect = node.get("rect", {})
    if "kalam" in app_id.lower() and rect.get("width", 0) > 200:
        best = rect
print("{} {} {} {}".format(best["x"], best["y"], best["width"], best["height"]) if best else "")
' 2>/dev/null || true)"
  if [ -z "$GEOM" ]; then
    say "tap check: SKIPPED -- no kalam window in sway's tree"
  else
    WIN_X="$(printf '%s' "$GEOM" | cut -d' ' -f1)"
    WIN_Y="$(printf '%s' "$GEOM" | cut -d' ' -f2)"
    WIN_W="$(printf '%s' "$GEOM" | cut -d' ' -f3)"
    WIN_H="$(printf '%s' "$GEOM" | cut -d' ' -f4)"
    say "window: ${WIN_W}x${WIN_H} at ${WIN_X},${WIN_Y}  seat=$SEAT"
    # Mid-height, three across: the next-page zone on the right, the
    # previous-page zone on the left, and the middle.
    TAP_FRACS="${TAP_FRACS:-0.75 0.25 0.50}"
    n=0
    for frac in $TAP_FRACS; do
      n=$((n + 1))
      TAP_X="$(python3 -c "print(int($WIN_X + $WIN_W * $frac))")"
      TAP_Y="$(python3 -c "print(int($WIN_Y + $WIN_H * 0.5))")"
      # A *tap* is a short press: hold the button down for a second and the
      # reader treats it as a long press (the selection anchor), which is a
      # different gesture and does not turn the page. 80 ms is under the
      # usual threshold and well above sway's round trip.
      swaymsg "seat $SEAT cursor set $TAP_X $TAP_Y" >/dev/null 2>&1 || true
      sleep 1
      swaymsg "seat $SEAT cursor press button1" >/dev/null 2>&1 || true
      sleep "${TAP_HOLD:-0.08}"
      swaymsg "seat $SEAT cursor release button1" >/dev/null 2>&1 || true
      sleep 2
      shot "05-after-tap-$n"
      if cmp -s "$OUT/03-$ROUTE-settled.png" "$OUT/05-after-tap-$n.png"; then
        say "tap $n at $TAP_X,$TAP_Y (frac $frac): no change"
      else
        say "tap $n at $TAP_X,$TAP_Y (frac $frac): CHANGED"
      fi
    done
    say "tap check: each line compares its shot with 03-$ROUTE-settled.png."
    say "           A change means the tap reached the page. No change at any"
    say "           of the three means the widget ignored it or sway did not"
    say "           deliver it -- the owner's own tap decides which."
  fi
fi

# --- do real keystrokes reach the app? (KEYS=1) ---------------------------
#
# Every other check in this harness asks the app to pose. KALAM_ROUTE opens a
# page, KALAM_DIALOG opens a dialog, KALAM_*_ACTION stages a drawn state --
# the app performs the action itself, so a broken wire between a control and
# its action is invisible here: every frame still looks right.
#
# This probe sends real input instead. It types key combos into the focused
# window with `wtype`, installed for this job since the harness was built and
# never used until now, and asks three independent witnesses whether the app
# acted:
#
#   1. the app's own log. Every page construction prints a `[timing]
#      route_open:<label>` line, so a shortcut that navigates cannot help
#      but leave evidence (route_label in src/app.rs).
#   2. the compositor's window tree. Fullscreen shows up there as
#      `fullscreen_mode`, which sway reports and Kalam does not, so it is
#      genuinely independent of witness 1.
#   3. the process. Quit closes the window and the app exits, so a dead pid
#      is proof a keystroke arrived and was acted on. That is the strongest
#      evidence available here, and the reason Quit runs last.
#
# Keys and not clicks, deliberately. The TAP probe above established that
# synthetic pointer events do not reach the app under headless sway. wtype
# differs in kind: it creates a virtual keyboard through the compositor's
# virtual-keyboard protocol instead of injecting into a seat that has no
# pointer device at all (WLR_LIBINPUT_NO_DEVICES=1). Whether that difference
# holds in practice is exactly what this probe is for.
#
# The combos are the app's own defaults (default_binding in src/shortcuts.rs):
# Alt+2 library, Alt+1 home, Ctrl+comma settings, Alt+Left back, Ctrl+slash the
# shortcut pane, Alt+1 home again, F11 fullscreen, Ctrl+Q quit. Every one
# carries a modifier or is an F-key, so fires_while_typing should let it
# through a focused text entry -- and none of them actually leaves one
# focused, which is deliberate; see STATUS. The first two are a control pair
# whose job is to say whether the rest of the report means anything.
#
# ROUTE=home is the intended pairing. With home the script skips its
# relaunch-and-compare step below, so nothing later in the run depends on the
# app still being alive after Ctrl+Q ends it.
#
# STATUS: two runs so far, both 2 of 6, and neither result is about the
# shortcut table yet.
#
# Run 1 put Ctrl+F first. It worked, and it is the one default shortcut that
# leaves a text entry holding focus; every modified chord after it did
# nothing, while F11 still worked and sway still saw the window go fullscreen.
# Run 2 dropped Ctrl+F for Alt+1 and led with Ctrl+comma instead. Ctrl+comma
# worked from home -- so run 1's four failures were at least partly the
# focused search box -- but then Alt+Left, Ctrl+slash, Alt+1 and Ctrl+Q all
# did nothing, and F11 again was the only later key that landed. The app's own
# log confirms it opened settings exactly once and then logged no route at all,
# with no main-thread stall and nothing on stderr.
#
# So the shape survived the change: the first modified chord from home always
# works, every modified chord after it does nothing, and the only later key
# that works is the one carrying no modifier. Two readings fit and they have
# opposite conclusions -- either the app goes deaf to modified keys once the
# page has changed, which is a real bug, or wtype stops applying modifiers
# after its first chord, which is this harness breaking and says nothing about
# Kalam. F11 cannot separate them, precisely because it has no modifier.
#
# Run 3 settled it, and the answer was neither. It led with a control pair --
# Alt+2 then Alt+1, two chords of the same kind back to back, each with its own
# route witness and no text entry near them -- and BOTH PASSED, so the injector
# keeps applying modifiers and the "app goes deaf" reading died with it. Five
# of eight probes passed and, decisively, Ctrl+Q quit the app: keystrokes reach
# it.
#
# The three failures were then all traced to this probe's witnesses, not to the
# app, by reading its own log and source:
#
#   - Alt+Left (Back) was a correct no-op. Every navigation shortcut calls
#     swap_page with push_history=false, and swap_page's else branch is
#     history.clear(). So after Ctrl+comma there was nothing to pop, and
#     AppMsg::Back is `if let Some(prev) = self.history.pop()`. The frame after
#     it was byte-identical to the frame before, which is what a no-op looks
#     like. Back can only be tested after the one shortcut that pushes history,
#     which is Ctrl+F -- hence steps 6 and 7 below, which test Back and answer
#     run 1's focused-search-box question with one variable changed.
#   - Ctrl+slash DID work. The log carries `settings_fill:shortcuts 1.2 ms`,
#     which only the SelectTab half of that handler can produce, and the frame
#     after it differs from the one before. No route line was ever possible:
#     the swap to Settings is a no-op when Settings is already open. It is now
#     witnessed by the tab filling instead.
#   - The second Alt+1 DID work. The last line of the app's log is
#     `page_cache_hit`. route_open: is logged inside build_page, and build_page
#     is skipped for a cached page -- the app's own comment: "The page cache's
#     hit path never enters this function -- a cached page is reparented, not
#     rebuilt." A route-only witness therefore reports a working shortcut as a
#     failure the second time you visit a page. Every navigation step now
#     accepts either line and says which one it saw.
#
# Run 4 used the corrected witnesses and reported 5 of 9 -- and disagreed with
# run 3 about which chords worked. Alt+1 had passed in run 3 and failed here;
# Ctrl+slash had passed and failed; the second Alt+1 had failed and passed. The
# missing lines never turned up late in the log either, so they were dropped
# rather than slow, and there was no main-thread stall and nothing on stderr.
#
# The rhythm explains it. All three dropped chords had been sent within
# milliseconds of the previous chord succeeding, and every chord sent after a
# pause arrived: when a witness appears the await returns at once, so the next
# wtype invocation builds a fresh virtual keyboard before the previous one has
# finished releasing its modifier. After a FAILED chord the await has already
# burned its whole timeout, which is why the results alternated instead of
# clustering. Every chord now gets a settle before it is sent.
#
# Run 5, with the settle, reported 8 of 9, losing only Alt+Left. Run 6, with
# the identical settle, reported 7 of 9 -- and lost Ctrl+comma and Ctrl+Q
# instead, while Alt+Left passed. So the settle lowered the drop rate and did
# not remove it, and the failures move between runs. Across all six, every one
# of the nine shortcuts has been witnessed firing at least once. That is why
# each chord is now sent twice: a binding that lands on the second attempt is
# wired, and the first key simply never arrived.
#
# The one failure left is Alt+Left, and it is NOT an app bug so far as anything
# readable here can tell. It is a real result rather than a witness problem --
# Ctrl+F pushed the history, so there was something to pop, and the missing
# line never appeared late -- but every candidate explanation was checked
# against source and ruled out:
#
#   - gtktext.c binds Alt+Left and Alt+Right only inside #ifdef __APPLE__, so
#     on Linux the focused text widget has no claim on them.
#   - gtkwindow.c's add_arrow_bindings binds arrows with GDK_NO_MODIFIER_MASK
#     and GDK_CONTROL_MASK, never with Alt.
#   - gtk_search_entry_is_keynav does test GDK_ALT_MASK, but it is a filter
#     that says "this is navigation, not typing" -- it does not stop the event.
#   - src/pages/all_books.rs installs no EventControllerKey at all.
#   - The shell table matches the chord correctly: default_binding and capture
#     both key on normalized_key_name, so Alt+Left compares equal to Alt+Left,
#     and fires_while_typing returns true for any alt binding.
#   - Ctrl+Q is sent afterwards with the same search box still focused, and it
#     quits the app.
#
# What is left is delivery, and note the correction: an earlier version of this
# comment concluded that Alt+Left must be a keysym the injector cannot send,
# because it had failed in all five runs to that point while Alt+1 and Alt+2
# had passed in all five. Run 6 passed Alt+Left -- same search box focused,
# same injector, same settle -- and dropped two chords that had never given
# trouble before. Five samples of a flaky thing is not a pattern. The retry
# exists so that one run can settle this by itself instead of needing six.
#
# No app code was changed on the strength of any of it, and none should be: the
# source above says nothing on this platform claims Alt+Left, and the app has
# now been seen handling it. The human check in docs/manual-test-checklist.md
# stays, because a harness that drops keys cannot prove a negative.
#
# Still open, and the only check that has never passed in any of the six runs:
# the second F11 does not leave fullscreen. The handler calls unfullscreen()
# only when root.is_fullscreen() agrees, so under a headless compositor GTK may
# never learn the state it is in and may ask to go fullscreen a second time
# instead. It is retried like everything else, so a dropped key cannot be
# mistaken for the toggle being one-way -- which is the interesting possibility
# and the one a human can settle in two seconds. Reported as a NOTE, not
# counted as a probe.
#
# Read the VERDICT line before the individual results, and read
# docs/ci/README-screenshots.md.
if [ "${KEYS:-0}" = "1" ]; then
  say ""
  say "=== keystroke probe (KEYS=1) ==="

  KEYS_SEND_FAILED=0
  KEYS_PASSED=0
  KEYS_TRIED=0
  QUIT_PROVEN=0

  # How many times the app logged a given page opening. `grep -c` exits 1 when
  # nothing matched, so `|| true` keeps pipefail from ending the probe; the
  # count is still printed. A missing log prints nothing at all, which is why
  # every caller defaults the result to 0.
  # Anchored on `[timing] route_open:` because the app prints a second line,
  # `[timing] frame_after:route_open:<label>`, once the page has painted. An
  # unanchored grep counts both, so one navigation reads as two and every
  # baseline is inflated -- the first run's counted 4 openings of home where
  # the app had logged 2.
  count_route() {
    grep -c "\[timing\] route_open:$1" "$OUT/kalam.log" 2>/dev/null || true
  }

  count_all_routes() {
    grep -c '\[timing\] route_open:' "$OUT/kalam.log" 2>/dev/null || true
  }

  # Any timing label, for witnesses that are not route lines.
  count_label() {
    grep -c "\[timing\] $1" "$OUT/kalam.log" 2>/dev/null || true
  }

  # How many times the app changed page, by EITHER route. `route_open:` is
  # logged inside build_page, and build_page is skipped when the page comes
  # out of the cache -- the app's own comment says so: "The page cache's hit
  # path never enters this function -- a cached page is reparented, not
  # rebuilt." So counting route lines alone reports a successful navigation as
  # a failure whenever the landing page was cached, which is exactly what the
  # third run did. page_cache_hit is the other half of the same event.
  count_navigations() {
    local built reused
    built="$(count_all_routes)"
    reused="$(grep -c '\[timing\] page_cache_hit' "$OUT/kalam.log" 2>/dev/null || true)"
    echo $(( ${built:-0} + ${reused:-0} ))
  }

  last_route_label() {
    grep -oE '\[timing\] (route_open:[a-z_]*|page_cache_hit)' "$OUT/kalam.log" \
      | tail -1 | sed 's/^\[timing\] //'
  }

  # Re-focus before every press. A window that does not hold focus sends the
  # keys nowhere, and "nowhere" is indistinguishable from "the app ignored
  # it" -- the false negative the September tap probe drowned in.
  refocus() {
    swaymsg '[app_id=".*"] focus' >/dev/null 2>&1 \
      || swaymsg focus >/dev/null 2>&1 || true
  }

  # The log line and the paint are not the same instant. The first run
  # photographed a page that had been navigated to but not yet drawn, so
  # the old 05-keys-search was byte-identical to the frame before it and the picture
  # appeared to contradict the log. A short dwell keeps each frame about the
  # same moment as the claim next to it.
  probe_shot() { # probe_shot <name>
    sleep 2
    shot "$1"
  }

  # Every chord gets a settle before it is sent, and the reason is measured
  # rather than guessed. Run 4 dropped three chords and the pattern was exact:
  # all three had been sent within milliseconds of the previous chord
  # succeeding, and every chord sent after a pause arrived. When a witness
  # appears the await returns at once, so the next wtype invocation builds a
  # fresh virtual keyboard before the previous one has finished releasing its
  # modifier -- and that chord is lost. After a failed chord the await has
  # already burned its whole timeout, which is why the failures alternated
  # with successes instead of clustering. It costs about fourteen seconds a
  # run and buys the difference between a result and a coin toss.
  KEY_SETTLE="${KEY_SETTLE:-1.5}"

  press() { # press <human label> <wtype args...>
    local label="$1"
    shift
    sleep "$KEY_SETTLE"
    refocus
    if wtype "$@" >/dev/null 2>&1; then
      say "keys: sent $label"
    else
      say "keys: WARNING -- wtype could not send $label"
      KEYS_SEND_FAILED=1
    fi
  }

  # Poll rather than sleep a fixed time: the settings page measures ~5 ms
  # warm, but the runner is shared and cold, so a fixed sleep is either too
  # short or wasted. Give up early if the app dies, or the probe would sit
  # out its whole budget against a process that Quit already ended.
  await_label() { # await_label <timing-label> <wanted-count> <timeout-s>
    local label="$1" want="$2" timeout="$3" deadline have spins
    deadline=$(( $(now_s) + timeout ))
    spins=$(( timeout * 20 + 40 ))
    while :; do
      have="$(count_label "$label")"
      [ "${have:-0}" -ge "$want" ] && return 0
      [ "$(now_s)" -ge "$deadline" ] && return 1
      kill -0 "$APP_PID" 2>/dev/null || return 1
      spins=$(( spins - 1 ))
      [ "$spins" -le 0 ] && return 1
      sleep 0.25
    done
  }

  # A page change is witnessed two ways, because the app logs a different line
  # depending on whether it built the page or lifted it out of the cache:
  # `route_open:<label>` for a build, `page_cache_hit` for a reuse. A witness
  # that only knows the first reports a working shortcut as a failure the
  # second time you visit a page, which the third run did. Sets PAGE_WITNESS
  # to whichever line was seen, so the report can say how it knew.
  # The baseline has to be taken BEFORE the key is sent, which is why this is
  # two functions rather than one. Folded into the wait, it reads the count
  # after the press -- and a fast app has often written the evidence in the
  # microseconds between wtype returning and the wait starting, so the wait
  # then looks for a second copy that never comes and reports a working
  # shortcut as a failure. Stub-testing caught this: the fake app logged every
  # navigation correctly and the probe still failed all of them.
  mark_page() { # mark_page <label> -- call BEFORE press
    MARK_BUILT="$(count_route "$1")"
    MARK_NAV="$(count_navigations)"
  }

  await_page() { # await_page <label> <timeout-s> -- call AFTER press
    local label="$1" timeout="$2" deadline want_built want_nav spins
    # The deadline is the intended exit; `spins` is a backstop, because this
    # loop has no other way out and a wait whose only exit depends on
    # arithmetic will spin forever if that arithmetic ever yields nothing. A
    # hung step costs CI hours rather than seconds.
    PAGE_WITNESS=""
    if [ -z "${MARK_BUILT:-}" ] || [ -z "${MARK_NAV:-}" ]; then
      say "keys: BUG -- await_page ran with no mark_page before the press, so"
      say "       it had no baseline to compare against. Reported as a failure"
      say "       rather than guessed at."
      return 1
    fi
    want_built=$(( MARK_BUILT + 1 ))
    want_nav=$(( MARK_NAV + 1 ))
    MARK_BUILT=""; MARK_NAV=""
    deadline=$(( $(now_s) + timeout ))
    spins=$(( timeout * 20 + 40 ))
    while :; do
      if [ "$(count_route "$label")" -ge "$want_built" ]; then
        PAGE_WITNESS="route_open:$label"
        return 0
      fi
      if [ "$(count_navigations)" -ge "$want_nav" ]; then
        PAGE_WITNESS="$(last_route_label) (cached, so the log names no page)"
        return 0
      fi
      [ "$(now_s)" -ge "$deadline" ] && return 1
      kill -0 "$APP_PID" 2>/dev/null || return 1
      spins=$(( spins - 1 ))
      [ "$spins" -le 0 ] && return 1
      sleep 0.25
    done
  }

  # One attempt at a chord: take the baseline, send the key, wait for the
  # witness. Split out from the steps below so that a retry is the same code
  # path as the first try rather than a copy of it.
  try_page() { # try_page <route-label> <timeout-s> <press-label> <wtype...>
    local label="$1" timeout="$2"
    shift 2
    mark_page "$label"
    press "$@"
    await_page "$label" "$timeout"
  }

  try_label() { # try_label <timing-label> <timeout-s> <press-label> <wtype...>
    local label="$1" timeout="$2" before
    shift 2
    before="$(count_label "$label")"
    press "$@"
    await_label "$label" $(( ${before:-0} + 1 )) "$timeout"
  }

  # The compositor's own view of the window, walked rather than grepped:
  # `fullscreen_mode` appears on every node in the tree, so a text match
  # would report whichever node came first rather than the app's window.
  fullscreen_mode() {
    swaymsg -t get_tree 2>/dev/null | python3 -c '
import json, sys

try:
    tree = json.load(sys.stdin)
except Exception:
    print("unknown")
    raise SystemExit(0)

found = ["0"]


def walk(node):
    mode = node.get("fullscreen_mode")
    if mode:
        found[0] = str(mode)
    for child in node.get("nodes", []) + node.get("floating_nodes", []):
        walk(child)


walk(tree)
print(found[0])
'
  }

  await_fullscreen() { # await_fullscreen <1=want-fullscreen|0=want-normal> <timeout-s>
    local want="$1" timeout="$2" deadline mode spins
    deadline=$(( $(now_s) + timeout ))
    spins=$(( timeout * 10 + 40 ))
    while :; do
      mode="$(fullscreen_mode)"
      case "$want" in
        1)
          if [ -n "$mode" ] && [ "$mode" != "0" ] && [ "$mode" != "unknown" ]; then
            return 0
          fi
          ;;
        0)
          [ "$mode" = "0" ] && return 0
          ;;
      esac
      [ "$(now_s)" -ge "$deadline" ] && return 1
      spins=$(( spins - 1 ))
      [ "$spins" -le 0 ] && return 1
      sleep 0.5
    done
  }

  if ! command -v wtype >/dev/null 2>&1; then
    say "keys: FAILED -- wtype is not installed, so no keystroke was sent"
    say "keys: VERDICT -- NOT RUN. The job installs wtype; if this fires,"
    say "       the install list changed."
  elif ! kill -0 "$APP_PID" 2>/dev/null; then
    say "keys: FAILED -- the app is not running, so there was nothing to send"
    say "keys: VERDICT -- NOT RUN. Read the startup section above first."
  else
    BASE_TOTAL="$(count_all_routes)"
    BASE_HOME="$(count_route home)"
    BASE_LIBRARY="$(count_route library)"
    BASE_SETTINGS="$(count_route settings)"
    say "keys: baseline -- ${BASE_TOTAL:-0} route_open lines so far (home=${BASE_HOME:-0}, library=${BASE_LIBRARY:-0}, settings=${BASE_SETTINGS:-0})"
    say "keys: focused windows before starting: $(swaymsg -t get_tree | grep -c '"focused": true' || echo 0)"

    # Steps 1 and 2 are a control pair, and they are the reason this probe is
    # ordered the way it is. Two runs reported the same shape: the first chord
    # sent from home worked, every modified chord after it did nothing, and
    # the only later key that still worked was F11 -- the one with no modifier
    # at all. That shape has two possible readings and they have opposite
    # conclusions. Either the app stops hearing modified keys once the page
    # has changed, which is a real bug, or wtype stops applying modifiers
    # after its first chord, which is this harness breaking and says nothing
    # about Kalam. Sending the same kind of chord twice back to back, with a
    # route witness on each and no text entry anywhere near it, is the cheap
    # way to tell those apart. Read step 2 before reading anything below it.
    FIRST_CHORD_OK=0
    MARK_BUILT=""; MARK_NAV=""
    SECOND_CHORD_OK=0

    # Every chord below is sent twice if the first attempt produces nothing,
    # and the report distinguishes the two outcomes. Runs 5 and 6 forced this:
    # with a settle in place they still lost chords, but never the same ones --
    # run 5 lost Alt+Left and run 6 lost Ctrl+comma and Ctrl+Q, and every one
    # of the nine has now been seen working in some run. A single attempt
    # therefore cannot tell "the app refused this binding" from "the injector
    # dropped this key", which is the only distinction the probe exists to
    # draw. A retry can: if the second attempt lands, the binding is wired and
    # the first key never arrived, and saying so costs one line.
    RETRIES=0

    # 1. Alt+2 -- GoLibrary. The first chord, sent from home.
    KEYS_TRIED=$((KEYS_TRIED + 1))
    if try_page library 10 "Alt+2 (GoLibrary)" -M alt -k 2 -m alt; then
      say "keys: OK -- chord 1: Alt+2 opened the library ($PAGE_WITNESS)"
      KEYS_PASSED=$((KEYS_PASSED + 1))
      FIRST_CHORD_OK=1
    elif try_page library 10 "Alt+2 (GoLibrary), second attempt" -M alt -k 2 -m alt; then
      say "keys: OK on RETRY -- chord 1: Alt+2 opened the library ($PAGE_WITNESS)"
      say "       The first attempt produced nothing, so that key was dropped"
      say "       in transit rather than refused by the app."
      KEYS_PASSED=$((KEYS_PASSED + 1))
      FIRST_CHORD_OK=1
      RETRIES=$((RETRIES + 1))
    else
      say "keys: FAILED -- chord 1: Alt+2 opened no page in two attempts"
    fi
    probe_shot "05-keys-library"

    # 2. Alt+1 -- GoHome. The control: a second modified chord, sent after one
    #    navigation.
    KEYS_TRIED=$((KEYS_TRIED + 1))
    if try_page home 10 "Alt+1 (GoHome)" -M alt -k 1 -m alt; then
      say "keys: OK -- chord 2: Alt+1 went home ($PAGE_WITNESS)"
      KEYS_PASSED=$((KEYS_PASSED + 1))
      SECOND_CHORD_OK=1
    elif try_page home 10 "Alt+1 (GoHome), second attempt" -M alt -k 1 -m alt; then
      say "keys: OK on RETRY -- chord 2: Alt+1 went home ($PAGE_WITNESS)"
      KEYS_PASSED=$((KEYS_PASSED + 1))
      SECOND_CHORD_OK=1
      RETRIES=$((RETRIES + 1))
    else
      say "keys: FAILED -- chord 2: Alt+1 opened no page in two attempts"
    fi
    if [ "$SECOND_CHORD_OK" = "1" ] && [ "$FIRST_CHORD_OK" = "1" ]; then
      say "keys: CONTROL PASSED -- a second modified chord still arrives, so the"
      say "       failures below, if any, are about those bindings and not about"
      say "       input delivery."
    elif [ "$FIRST_CHORD_OK" = "1" ]; then
      say "keys: CONTROL FAILED -- the first modified chord arrived and the"
      say "       second did not, on two attempts each. Read every line below"
      say "       as uninterpretable: the app is not necessarily deaf, the"
      say "       injector may simply have stopped applying modifiers. Settle"
      say "       that before going anywhere near the shortcut table."
    fi
    probe_shot "06-keys-home"

    # 3. Ctrl+comma -- OpenSettings.
    KEYS_TRIED=$((KEYS_TRIED + 1))
    if try_page settings 10 "Ctrl+comma (OpenSettings)" -M ctrl -k comma -m ctrl; then
      say "keys: OK -- Ctrl+comma opened Settings ($PAGE_WITNESS)"
      KEYS_PASSED=$((KEYS_PASSED + 1))
    elif try_page settings 10 "Ctrl+comma (OpenSettings), second attempt" -M ctrl -k comma -m ctrl; then
      say "keys: OK on RETRY -- Ctrl+comma opened Settings ($PAGE_WITNESS)"
      KEYS_PASSED=$((KEYS_PASSED + 1))
      RETRIES=$((RETRIES + 1))
    else
      say "keys: FAILED -- Ctrl+comma opened no page in two attempts"
    fi
    probe_shot "07-keys-settings"

    # 4. Ctrl+slash -- ShowShortcuts. Witnessed by the Shortcuts tab filling,
    #    NOT by a route line. The handler swaps to Settings and then selects
    #    that tab, and the swap is a no-op when Settings is already open -- so
    #    a route witness is impossible from here, and the third run duly
    #    reported a working shortcut as a failure. settings_fill:shortcuts
    #    fires whether or not the swap happened.
    #
    #    Read it together with step 3. If Ctrl+comma was dropped, this chord
    #    does both halves at once and the log shows route_open:settings next to
    #    settings_fill:shortcuts; that is step 4 covering for step 3, not step
    #    3 having worked.
    KEYS_TRIED=$((KEYS_TRIED + 1))
    if try_label settings_fill:shortcuts 12 "Ctrl+slash (ShowShortcuts)" -M ctrl -k slash -m ctrl; then
      say "keys: OK -- Ctrl+slash filled the Shortcuts tab (settings_fill:shortcuts)"
      KEYS_PASSED=$((KEYS_PASSED + 1))
    elif try_label settings_fill:shortcuts 12 "Ctrl+slash (ShowShortcuts), second attempt" -M ctrl -k slash -m ctrl; then
      say "keys: OK on RETRY -- Ctrl+slash filled the Shortcuts tab"
      KEYS_PASSED=$((KEYS_PASSED + 1))
      RETRIES=$((RETRIES + 1))
    else
      say "keys: FAILED -- Ctrl+slash logged no settings_fill:shortcuts in two attempts"
    fi
    probe_shot "08-keys-shortcuts"

    # 5. Alt+1 -- GoHome again, so the rest runs from a known page.
    KEYS_TRIED=$((KEYS_TRIED + 1))
    if try_page home 10 "Alt+1 (GoHome, second time)" -M alt -k 1 -m alt; then
      say "keys: OK -- Alt+1 went home a second time ($PAGE_WITNESS)"
      KEYS_PASSED=$((KEYS_PASSED + 1))
    elif try_page home 10 "Alt+1 (GoHome, second time), second attempt" -M alt -k 1 -m alt; then
      say "keys: OK on RETRY -- Alt+1 went home a second time ($PAGE_WITNESS)"
      KEYS_PASSED=$((KEYS_PASSED + 1))
      RETRIES=$((RETRIES + 1))
    else
      say "keys: FAILED -- the second Alt+1 opened no page in two attempts"
    fi

    # 6. Ctrl+F -- SearchLibrary. Two jobs at once. It goes to All Books, and
    #    it is the only default shortcut that pushes the history stack -- every
    #    other navigation shortcut passes push_history=false, which clears it,
    #    so Back has nothing to pop after them and a no-op result there proves
    #    nothing. It also ends with the library's search box holding focus,
    #    which is what step 7 then measures.
    KEYS_TRIED=$((KEYS_TRIED + 1))
    SEARCH_FOCUSED=0
    if try_page all_books 10 "Ctrl+f (SearchLibrary)" -M ctrl -k f -m ctrl; then
      say "keys: OK -- Ctrl+f reached the library search ($PAGE_WITNESS)"
      KEYS_PASSED=$((KEYS_PASSED + 1))
      SEARCH_FOCUSED=1
    elif try_page all_books 10 "Ctrl+f (SearchLibrary), second attempt" -M ctrl -k f -m ctrl; then
      say "keys: OK on RETRY -- Ctrl+f reached the library search ($PAGE_WITNESS)"
      KEYS_PASSED=$((KEYS_PASSED + 1))
      SEARCH_FOCUSED=1
      RETRIES=$((RETRIES + 1))
    else
      say "keys: FAILED -- Ctrl+f opened no page in two attempts"
    fi
    if [ "$SEARCH_FOCUSED" = "1" ]; then
      say "       The search box now holds focus, which is the point of the"
      say "       next step and the reason this one runs where it does."
    fi
    probe_shot "09-keys-search"

    # 7. Alt+Left -- Back. Now there IS a history entry to pop, so a no-op
    #    would be a real result rather than a consequence of the design.
    KEYS_TRIED=$((KEYS_TRIED + 1))
    if try_page home 10 "Alt+Left (Back)" -M alt -k Left -m alt; then
      say "keys: OK -- Alt+Left popped the history ($PAGE_WITNESS)"
      KEYS_PASSED=$((KEYS_PASSED + 1))
    elif try_page home 10 "Alt+Left (Back), second attempt" -M alt -k Left -m alt; then
      say "keys: OK on RETRY -- Alt+Left popped the history ($PAGE_WITNESS)"
      KEYS_PASSED=$((KEYS_PASSED + 1))
      RETRIES=$((RETRIES + 1))
    else
      say "keys: FAILED -- Alt+Left opened no page in two attempts"
      if [ "$SEARCH_FOCUSED" = "1" ]; then
        say "       The search box is focused, but do not blame it: run 6 saw"
        say "       this chord land with the same box focused, and nothing on"
        say "       this platform claims Alt+Left (gtktext.c binds it only"
        say "       under #ifdef __APPLE__, gtkwindow.c binds arrows with no"
        say "       modifier and with Ctrl but never Alt, and all_books.rs"
        say "       installs no key controller). Two attempts failing is what"
        say "       a real refusal would look like, so it is worth a human"
        say "       hand -- see docs/manual-test-checklist.md."
      else
        say "       Ctrl+f did not get us to the search box, so there may have"
        say "       been no history to pop. Uninterpretable."
      fi
    fi
    probe_shot "10-keys-back"

    # 8. F11 -- Fullscreen, witnessed by the compositor and not by the app.
    KEYS_TRIED=$((KEYS_TRIED + 1))
    FULLSCREEN_SEEN=0
    press "F11 (Fullscreen)" -k F11
    if await_fullscreen 1 10; then
      say "keys: OK -- sway reports fullscreen_mode=$(fullscreen_mode) after F11"
      KEYS_PASSED=$((KEYS_PASSED + 1))
      FULLSCREEN_SEEN=1
    else
      press "F11 (Fullscreen), second attempt" -k F11
      if await_fullscreen 1 10; then
        say "keys: OK on RETRY -- sway reports fullscreen_mode=$(fullscreen_mode)"
        KEYS_PASSED=$((KEYS_PASSED + 1))
        FULLSCREEN_SEEN=1
        RETRIES=$((RETRIES + 1))
      else
        say "keys: FAILED -- sway reports fullscreen_mode=$(fullscreen_mode) after two F11s"
      fi
    fi
    probe_shot "11-keys-fullscreen"

    # And back again: the toggle proven in both directions, and the window a
    # normal shape for whatever follows. Not counted as a separate probe,
    # because it is the same shortcut and the same witness.
    #
    # Conditional on the first press having got there. With the window already
    # normal, "still normal" proves nothing at all, and printing OK here would
    # put a green line into a run where no keystroke arrived -- the failure
    # mode this whole harness has been bitten by before (docs/pitfalls.md §21:
    # a check that passes on broken code teaches you to ignore the checks).
    #
    # This is the one check that has never passed, in any run. It is retried
    # like the others so that a dropped key cannot be mistaken for the toggle
    # being one-way, which is the interesting possibility.
    if [ "$FULLSCREEN_SEEN" = "1" ]; then
      press "F11 again (leave fullscreen)" -k F11
      if await_fullscreen 0 10; then
        say "keys: OK -- the second F11 left fullscreen (fullscreen_mode=0)"
      else
        press "F11 again (leave fullscreen), second attempt" -k F11
        if await_fullscreen 0 10; then
          say "keys: OK on RETRY -- the second F11 left fullscreen"
          say "       So the toggle is not one-way; the first attempt was dropped."
        else
          say "keys: NOTE -- still fullscreen_mode=$(fullscreen_mode) after two"
          say "       attempts. The handler calls unfullscreen() only when"
          say "       root.is_fullscreen() agrees, so under a headless"
          say "       compositor GTK may never learn the state it is in. This is"
          say "       the one check that has never passed in any run."
        fi
      fi
    else
      say "keys: SKIPPED -- the second F11; the first never reached fullscreen,"
      say "       so 'still normal' would prove nothing and is not reported as OK."
    fi

    # 9. Ctrl+Q -- Quit. Last, because it ends the app, and a dead pid is the
    #    one proof here that cannot be argued with.
    KEYS_TRIED=$((KEYS_TRIED + 1))
    quit_wait() { # quit_wait <seconds>
      local deadline spins
      deadline=$(( $(now_s) + $1 ))
      spins=$(( $1 * 4 + 40 ))
      while kill -0 "$APP_PID" 2>/dev/null; do
        [ "$(now_s)" -ge "$deadline" ] && break
        spins=$(( spins - 1 ))
        [ "$spins" -le 0 ] && break
        sleep 0.25
      done
      ! kill -0 "$APP_PID" 2>/dev/null
    }
    press "Ctrl+q (Quit)" -M ctrl -k q -m ctrl
    if quit_wait 15; then
      say "keys: OK -- the app exited on Ctrl+q"
      KEYS_PASSED=$((KEYS_PASSED + 1))
      QUIT_PROVEN=1
    else
      press "Ctrl+q (Quit), second attempt" -M ctrl -k q -m ctrl
      if quit_wait 15; then
        say "keys: OK on RETRY -- the app exited on the second Ctrl+q"
        KEYS_PASSED=$((KEYS_PASSED + 1))
        QUIT_PROVEN=1
        RETRIES=$((RETRIES + 1))
      else
        say "keys: FAILED -- the app was still alive after two Ctrl+q attempts"
      fi
    fi
    # No shot here on purpose. The window is gone, so a frame would be black,
    # and a black frame in the pixel sample reads as a rendering failure.

    say ""
    say "keys: $KEYS_PASSED of $KEYS_TRIED keystroke probes passed"
    if [ "${RETRIES:-0}" -gt 0 ]; then
      say "keys: NOTE -- $RETRIES of them only landed on a second attempt. Those"
      say "       bindings are wired; the injector dropped the first key. A"
      say "       retry count that climbs run over run is the harness wearing"
      say "       out, not the app breaking, and is worth watching."
    fi
    if [ "$QUIT_PROVEN" = "1" ]; then
      say "keys: VERDICT -- keystrokes DO reach the app: it quit on Ctrl+q."
      say "       Every FAILED line above is then a question about that one"
      say "       shortcut's wiring, not about input delivery."
    elif [ "$FIRST_CHORD_OK" = "1" ] && [ "$SECOND_CHORD_OK" = "0" ]; then
      say "keys: VERDICT -- INCONCLUSIVE, and deliberately so. The first"
      say "       modified chord arrived and the second did not, which is the"
      say "       injector's signature rather than the app's. Do not go reading"
      say "       the shortcut table yet: settle whether wtype still applies"
      say "       modifiers after its first chord. F11 passing below would not"
      say "       argue against this, because F11 carries no modifier."
    elif [ "$KEYS_PASSED" = "0" ]; then
      say "keys: VERDICT -- nothing responded, Quit included. Read that as"
      say "       'input did not arrive', which is the pointer's fate in the"
      say "       TAP probe above, and NOT as a pile of broken shortcuts. Doubt"
      say "       wtype's virtual keyboard against this compositor first."
    else
      say "keys: VERDICT -- some probes passed, so input does arrive; the"
      say "       failures above are individual and worth reading one by one."
    fi
    if [ "$KEYS_SEND_FAILED" = "1" ]; then
      say "keys: NOTE -- wtype itself errored at least once, so delivery and"
      say "       not the app is the first thing to doubt."
    fi
  fi
fi

# --- did we actually render the requested page? ---------------------------
#
# Three checks, because each catches a different failure and the first two can
# both pass while the screen is wrong.
NAV_OK=1

# 1. Did the app accept the route? It prints on both paths, so silence means a
#    binary built before KALAM_ROUTE existed.
if grep -q "KALAM_ROUTE=$ROUTE" "$OUT/kalam.log" 2>/dev/null \
   && ! grep -q "unknown route" "$OUT/kalam.log" 2>/dev/null; then
  say "navigation: app accepted route '$ROUTE'"
elif grep -q "unknown route" "$OUT/kalam.log" 2>/dev/null; then
  say "navigation: FAILED -- app rejected '$ROUTE' as unknown"
  grep "known:" "$OUT/kalam.log" | sed 's/^/    /' | tee -a "$REPORT"
  NAV_OK=0
else
  say "navigation: FAILED -- no KALAM_ROUTE line; binary predates the flag?"
  NAV_OK=0
fi

# 2. Did a grid actually build? Only build_book_grid emits this, so for a grid
#    route it is positive proof the page rendered rather than merely being
#    asked for. Not every route is a grid, so this only judges the ones that
#    are -- a blanket check would cry wolf on ROUTE=settings.
case "$ROUTE" in
  all-books|allbooks|reading-list|history|tags|shelves)
    if grep -q "grid_build" "$OUT/kalam.log" 2>/dev/null; then
      CARDS="$(grep "grid_cards" "$OUT/kalam.log" | tail -1 | awk '{print $NF}')"
      say "navigation: grid rendered (grid_build seen, grid_cards=${CARDS:-?})"
    else
      say "navigation: FAILED -- '$ROUTE' is a grid page but no grid_build line"
      NAV_OK=0
    fi
    ;;
  *)
    say "navigation: '$ROUTE' is not a grid page; no grid_build expected"
    ;;
esac

# 3. Does the requested page actually look different from Home?
#
#    This is the check that catches "everything above lied", and it needs a
#    second launch to mean anything: with KALAM_ROUTE every shot in this run
#    is the same page, so comparing them to each other proves nothing. The
#    first version of this check did exactly that and reported FAILED on a
#    working run -- three identical fingerprints are the *expected* result
#    here, not a bug.
#
#    So: relaunch on Home, photograph it, and compare. If the pixels match,
#    the app ignored the route no matter what its log claimed.
if [ "$ROUTE" != "home" ]; then
  kill "$APP_PID" 2>/dev/null || true
  wait "$APP_PID" 2>/dev/null || true
  sleep 0.25
  # `sample_rss` uses `pgrep -n` (newest), so it follows this process too.
  # That keeps the peak-memory figure honest -- it is still the high-water
  # mark of a single kalam process, just possibly the second one.
  HOME_START="$(now_s)"
  KALAM_ROUTE=home KALAM_THEME="$THEME" KALAM_NO_WINDOWED_GRID="$NO_WINDOWED" "$BIN" > "$OUT/kalam-home.log" 2>&1 &
  HOME_PID=$!
  wait_for_app_ready "$HOME_PID" "$OUT/kalam-home.log" home 10
  HOME_READY_STATUS=$?
  if [ "$HOME_READY_STATUS" -eq 2 ]; then
    say "readiness: Home route/window signal not seen within 10s; photographing anyway"
  elif [ "$HOME_READY_STATUS" -eq 0 ]; then
    say "readiness: Home seen after $(( $(now_s) - HOME_START ))s"
  fi
  wait_until_elapsed "$HOME_START" 2 "$HOME_PID" || true
  shot "04-home-for-comparison"
  # Home is only the comparison negative, but an early splash or a
  # still-loading cover grid would make that comparison meaningless.
  # Give it the same stable-frame treatment with a much smaller cap.
  if wait_for_stable_frame "$HOME_PID" "$OUT/04-home-for-comparison.png" \
      "$OUT/04-home-for-comparison.png" $((HOME_START + 10)); then
    say "readiness: Home stable after $(( $(now_s) - HOME_START ))s"
  else
    say "readiness: Home latest frame used; no stable pair within 10s"
  fi
  kill "$HOME_PID" 2>/dev/null || true

  ROUTE_SUM="$(md5sum "$OUT/03-$ROUTE-settled.png" 2>/dev/null | cut -d" " -f1)"
  HOME_SUM="$(md5sum "$OUT/04-home-for-comparison.png" 2>/dev/null | cut -d" " -f1)"
  if [ -z "$ROUTE_SUM" ] || [ -z "$HOME_SUM" ]; then
    say "navigation: could not compare against Home (a screenshot is missing)"
    NAV_OK=0
  elif [ "$ROUTE_SUM" = "$HOME_SUM" ]; then
    say "navigation: FAILED -- '$ROUTE' is pixel-identical to Home."
    say "  The app reported navigating but the screen never changed."
    NAV_OK=0
  else
    say "navigation: '$ROUTE' differs from Home -- the page really did change"
  fi
fi

# Route acceptance and a changed screenshot do not prove the reader opened
# its EPUB. Treat an explicit open failure as a failed reader smoke check.
case "$ROUTE" in
  read-*)
    if grep -q "open epub failed" "$OUT/kalam.log" 2>/dev/null; then
      say "navigation: FAILED -- the reader route could not open its seeded EPUB"
      NAV_OK=0
    fi
    ;;
esac

if [ "$NAV_OK" = "1" ]; then
  say "navigation: OK -- the images below are '$ROUTE'"
else
  say "navigation: NOT PROVEN -- do not trust the page names below"
fi

say ""
say "=== timing lines from the app ==="
# The whole point of KALAM_TIMING. Reproduced in the report so the numbers are
# readable without downloading anything.
grep -E "^\[timing\]" "$OUT/kalam.log" | sed 's/^/  /' | tee -a "$REPORT" \
  || say "  (none -- KALAM_TIMING produced no output)"

say ""
say "=== main-thread stalls (roadmap 7.1 watchdog) ==="
# Every [stall] line the watchdog printed during this session. None is the
# pass condition: the UI loop never blocked past ~100 ms while the session
# ran. (A teardown line at the very end of a run can be ignored -- the loop
# stopping as the process exits looks, briefly, like a block.)
STALL_LINES="$(grep -E '^\[stall\]' "$OUT/kalam.log" 2>/dev/null || true)"
if [ -n "$STALL_LINES" ]; then
  printf '%s\n' "$STALL_LINES" | sed 's/^/  /' | tee -a "$REPORT"
  say "  stalls seen -- name the screen and fix it before it spreads"
else
  say "  (none -- no UI-loop block over ~100 ms was detected)"
fi

say ""
say "=== screenshot pixel sample ==="
# Strong colour is a rough cover-loading proxy only for a library-grid route.
# On reader or tasks routes the percentage is descriptive, not a pass
# or failure signal.
say "  low colour is a cover warning only on a seeded library-grid route."
# [0-9]* rather than 0*: the keystroke probe now runs to eleven frames, and a
# glob that only matches a leading zero silently drops the last two from both
# the pixel sample and the fingerprints below. They are exactly the frames that
# would show whether the final chords changed anything.
python3 docs/ci/check-shot.py "$OUT"/[0-9]*.png 2>&1 | tee -a "$REPORT" \
  || say "  (pixel sample failed to run)"

say ""
say "=== screenshot fingerprints ==="
# Identical checksums mean the screen never changed -- the single cheapest
# signal that navigation did nothing, and the one that would have caught this
# on the first working run.
md5sum "$OUT"/[0-9]*.png 2>/dev/null | sed 's/^/  /' | tee -a "$REPORT" \
  || say "  (no shots to fingerprint)"

say ""
say "=== app stderr (non-timing lines) ==="
grep -vE "^\[timing\]" "$OUT/kalam.log" 2>/dev/null | tail -20 | sed 's/^/  /' \
  | tee -a "$REPORT" || say "  (none)"

say ""
say "=== peak memory ==="
printf 'books=%s peak_rss_kb=%s peak_rss_mb=%s\n' \
  "$BOOKS" "$PEAK_KB" "$((PEAK_KB / 1024))" | tee -a "$REPORT"
if [ "$PEAK_KB" -eq 0 ]; then
  say "WARNING: never sampled a running kalam process -- treat all of the"
  say "         above as meaningless."
fi

say ""
say "=== shutting down ==="
# Both, and both tolerant of already being dead: the comparison launch kills
# APP_PID and starts HOME_PID, so which of the two is still running depends on
# whether that branch ran at all.
kill "$APP_PID" 2>/dev/null || true
kill "${HOME_PID:-}" 2>/dev/null || true
swaymsg exit >/dev/null 2>&1 || true
for _ in $(seq 1 20); do
  kill -0 "$SWAY_PID" 2>/dev/null || break
  sleep 0.1
done
kill "$SWAY_PID" 2>/dev/null || true
wait "$SWAY_PID" 2>/dev/null || true

say ""
say "=== artifacts ==="
ls -la "$OUT" | sed 's/^/  /' | tee -a "$REPORT"
# Never fail the build on a screenshot problem: this step is diagnostic, and a
# flaky compositor must not block a correct code change.
exit 0
