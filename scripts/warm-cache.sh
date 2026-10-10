#!/usr/bin/env bash
#
# Warm the page cache so Kalam's first launch after a reboot is a warm one.
# Roadmap 1.15.
#
# Cold start is ~9.9 s against ~762 ms warm, and the entire difference is
# reading the binary, its shared libraries and the catalogue database off a
# spinning disk. No code change makes that faster. This script does the
# reading at a moment nobody is waiting through — shortly after login — so
# the first click finds everything already in RAM.
#
# It is a MITIGATION, not a fix. With 4 GB of RAM, opening a browser
# afterwards can evict what was just warmed. The real fix is faster storage.
#
# TEST BEFORE AUTOMATING (per the roadmap):
#   1. reboot
#   2. run:  ./scripts/warm-cache.sh
#   3. launch Kalam with:  KALAM_TIMING=1 kalam
#   4. confirm window_shown is near the warm ~760 ms, not ~9 s
# Only once that holds, wire it into a systemd user unit or autostart.
#
# Usage: warm-cache.sh [path-to-kalam] [--covers]
#   path-to-kalam   the binary to warm (default: `kalam` on PATH, else
#                   target/release/kalam, else target/debug/kalam)
#   --covers        also warm the cover thumbnails (slower; helps first paint
#                   of the library grid, not the window-shown figure)

set -euo pipefail

warm_covers=0
bin=""
for arg in "$@"; do
  case "$arg" in
    --covers) warm_covers=1 ;;
    *) bin="$arg" ;;
  esac
done

# Locate the binary.
if [[ -z "$bin" ]]; then
  bin="$(command -v kalam 2>/dev/null || true)"
fi
if [[ -z "$bin" || ! -e "$bin" ]]; then
  for cand in target/release/kalam target/debug/kalam; do
    if [[ -e "$cand" ]]; then bin="$cand"; break; fi
  done
fi
if [[ -z "$bin" || ! -e "$bin" ]]; then
  echo "kalam binary not found — pass its path as the first argument." >&2
  exit 1
fi

# Read a file into the page cache. `cat` to /dev/null is enough: the kernel
# keeps the pages. vmtouch is used when present because it can report and is
# a little quicker, but it is never required.
warmed=0
warm() {
  local f="$1"
  [[ -f "$f" ]] || return 0
  if command -v vmtouch >/dev/null 2>&1; then
    vmtouch -t -q "$f" >/dev/null 2>&1 || cat "$f" >/dev/null 2>&1 || true
  else
    cat "$f" >/dev/null 2>&1 || true
  fi
  warmed=$((warmed + 1))
}

warm_tree() {  # every regular file under a directory
  local dir="$1"
  [[ -d "$dir" ]] || return 0
  while IFS= read -r -d '' f; do warm "$f"; done \
    < <(find "$dir" -type f -print0 2>/dev/null)
}

echo "warming kalam: $bin"
warm "$bin"

# Shared libraries the binary links against — the bulk of the cold-start
# cost is the dynamic loader pulling these off the disk one by one.
if command -v ldd >/dev/null 2>&1; then
  while IFS= read -r lib; do
    [[ -n "$lib" ]] && warm "$lib"
  done < <(ldd "$bin" 2>/dev/null | awk '/=> \//{print $3} /^\t\//{print $1}')
fi

# The graphics stack, so the first window does not page GTK in from disk.
# Resolved through ldconfig rather than hardcoded paths, which differ per
# distribution.
if command -v ldconfig >/dev/null 2>&1; then
  while IFS= read -r lib; do
    [[ -n "$lib" ]] && warm "$lib"
  done < <(ldconfig -p 2>/dev/null \
    | awk '/libgtk-4|libgdk|libadwaita|libglib-2|libgobject-2|libgio-2|libpango|libcairo|libgdk_pixbuf/{print $NF}')
fi

# The catalogue database — opening it cold is the other half of the delay.
data="${XDG_DATA_HOME:-$HOME/.local/share}/kalam"
warm "$data/catalog.db"

# Fontconfig and icon caches, consulted on first draw.
warm_tree "$HOME/.cache/fontconfig"

if [[ "$warm_covers" -eq 1 ]]; then
  echo "warming cover thumbnails (this can take a while on a spinning disk)"
  warm_tree "$data/covers"
  warm_tree "$data/cache/thumbs"
fi

echo "warmed $warmed files into the page cache."
echo "now launch:  KALAM_TIMING=1 kalam   and check window_shown."
