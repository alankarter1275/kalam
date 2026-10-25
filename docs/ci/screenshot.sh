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
# DIALOG asks the book page to open one of its dialogs over itself once
# loaded (KALAM_DIALOG=tags|annotations). No route reaches a dialog, so
# without this the loop could photograph every page and still never see
# the two dialogs the book page owns.
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
# (KALAM_COMIC_ACTION: chrome, chapters, pages, reading, shortcuts,
# jump, jump-range).
COMIC_ACTION="${COMIC_ACTION:-}"
say "requested comic action: ${COMIC_ACTION:-<none>}"
# PDF_ACTION is the same ask for the PDF reader (KALAM_PDF_ACTION:
# chrome, contents, contents-pinned, pages, notes, notes-words,
# notes-bookmarks, notes-empty, search-results, reading, shortcuts,
# jump, jump-range).
PDF_ACTION="${PDF_ACTION:-}"
say "requested PDF action: ${PDF_ACTION:-<none>}"
say "settings fixture: ${SETTINGS_FIXTURE:-0} empty-dicts=${SETTINGS_EMPTY_DICTS:-0}"
# WINDOWED controls the A0 step 6 windowed grid (build only the cards on
# screen). It became the default on 2026-09-04, so the app flag is now the
# negative one, KALAM_NO_WINDOWED_GRID.
#
# Back-compat matters here. The installed workflow was written while the grid
# was opt-in: its baseline run passes nothing and its comparison run passes
# WINDOWED=1. Now that windowed is the default, "nothing" would mean windowed
# too -- both runs would measure the same thing and publish a green,
# meaningless comparison. That is the pitfalls §21 failure exactly. So the
# *output directory* picks the baseline: a run writing to a plain `ci-shots-*`
# path (not `-windowed`, and not the 139-book `ci-shots`) is the old grid
# unless told otherwise. An explicit WINDOWED= always wins, so the updated
# workflow in this directory works unchanged too.
if [ -n "${WINDOWED:-}" ]; then
  :
elif [ "${OUT%-windowed}" = "$OUT" ] && [ "$OUT" != "ci-shots" ]; then
  WINDOWED=0
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
python3 docs/ci/check-shot.py "$OUT"/0*.png 2>&1 | tee -a "$REPORT" \
  || say "  (pixel sample failed to run)"

say ""
say "=== screenshot fingerprints ==="
# Identical checksums mean the screen never changed -- the single cheapest
# signal that navigation did nothing, and the one that would have caught this
# on the first working run.
md5sum "$OUT"/0*.png 2>/dev/null | sed 's/^/  /' | tee -a "$REPORT" \
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
