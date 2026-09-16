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
# Generous: a cold binary on a shared runner is slow to first paint, and a
# screenshot taken too early shows an empty window and looks exactly like a bug.
SETTLE="${SETTLE:-25}"

mkdir -p "$OUT"
REPORT="$OUT/report.txt"
: > "$REPORT"

# Everything interesting goes to BOTH the console and report.txt.
say() { echo "$*" | tee -a "$REPORT"; }

say "=== kalam screenshot run ==="
say "books=$BOOKS out=$OUT settle=$SETTLE"

export XDG_DATA_HOME="${XDG_DATA_HOME:-/tmp/kalam-ci-data}"
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
if ! python3 docs/ci/seed-library.py --books "$BOOKS" 2>&1 | tee -a "$REPORT"; then
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
output HEADLESS-1 resolution 1600x1000
default_border none
focus_follows_mouse no
EOF

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
for _ in $(seq 1 30); do
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
  sleep 1
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
KALAM_ROUTE="$ROUTE" KALAM_NO_WINDOWED_GRID="$NO_WINDOWED" "$BIN" > "$OUT/kalam.log" 2>&1 &
APP_PID=$!

for _ in $(seq 1 "$SETTLE"); do
  sample_rss
  sleep 1
done

if ! kill -0 "$APP_PID" 2>/dev/null; then
  say "FATAL: kalam exited during startup. Its output:"
  sed 's/^/  /' "$OUT/kalam.log" | tail -40 | tee -a "$REPORT"
  swaymsg exit >/dev/null 2>&1 || true
  exit 0
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

for _ in $(seq 1 6); do sample_rss; sleep 1; done
shot "02-$ROUTE-settling"
# Covers arrive in the background, so the interesting screenshot is the later
# one: anything still grey here is a cover that is never coming.
for _ in $(seq 1 12); do sample_rss; sleep 1; done
shot "03-$ROUTE-settled"

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
  sleep 2
  # `sample_rss` uses `pgrep -n` (newest), so it follows this process too.
  # That keeps the peak-memory figure honest -- it is still the high-water
  # mark of a single kalam process, just possibly the second one.
  KALAM_ROUTE=home KALAM_NO_WINDOWED_GRID="$NO_WINDOWED" "$BIN" > "$OUT/kalam-home.log" 2>&1 &
  HOME_PID=$!
  for _ in $(seq 1 12); do sample_rss; sleep 1; done
  shot "04-home-for-comparison"
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
say "=== did the covers actually load? ==="
# The seeded covers are deliberately colourful; the placeholder is grey. So
# "how many strongly-coloured pixels are there" is a machine-checkable proxy
# for "did the covers appear", and it does not need a human to squint at a PNG.
python3 docs/ci/check-shot.py "$OUT"/0*.png 2>&1 | tee -a "$REPORT" \
  || say "  (cover check failed to run)"

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
sleep 2
kill "$SWAY_PID" 2>/dev/null || true
wait "$SWAY_PID" 2>/dev/null || true

say ""
say "=== artifacts ==="
ls -la "$OUT" | sed 's/^/  /' | tee -a "$REPORT"
# Never fail the build on a screenshot problem: this step is diagnostic, and a
# flaky compositor must not block a correct code change.
exit 0
