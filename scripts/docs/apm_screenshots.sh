#!/usr/bin/env bash
# Regenerate the screenshots in docs/images/apm/ from a live service.
#
# Like scripts/docs/profiling_screenshots.sh, the images in the transaction
# monitoring guide are not mockups: this script runs the examples/web-apm demo
# (a service started with --apm, its collector with a store, budgets and the
# monitor page) on ports of its own, lets its load run, and photographs the
# page the collector serves. Run it whenever the page changes, and commit
# whatever moved.
#
#   scripts/docs/apm_screenshots.sh
#
# Needs a headless Chromium: $CHROME, then Playwright's cache, then the usual
# application bundles. ELEPHC picks the compiler (default: target/debug/elephc).
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUT="$ROOT/docs/images/apm"
WORK="$(mktemp -d)"
DEMO_PID=""
cleanup () {
  if [ -n "$DEMO_PID" ]; then kill -TERM "$DEMO_PID" 2>/dev/null || true; wait "$DEMO_PID" 2>/dev/null || true; fi
  rm -rf "$WORK"
}
trap cleanup EXIT

# --- the browser -------------------------------------------------------------
find_browser () {
  if [ -n "${CHROME:-}" ] && [ -x "${CHROME}" ]; then echo "$CHROME"; return; fi
  local c
  for c in "$HOME"/Library/Caches/ms-playwright/chromium_headless_shell-*/chrome-headless-shell-*/chrome-headless-shell \
           "$HOME"/Library/Caches/ms-playwright/chromium-*/chrome-mac*/Chromium.app/Contents/MacOS/Chromium \
           "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome" \
           "/Applications/Brave Browser.app/Contents/MacOS/Brave Browser"; do
    [ -x "$c" ] && { echo "$c"; return; }
  done
  echo "no headless Chromium found; set CHROME=/path/to/chrome" >&2
  exit 1
}
BROWSER="$(find_browser)"
echo "browser: $BROWSER"

# --- the service and its collector -------------------------------------------
export ELEPHC="${ELEPHC:-$ROOT/target/debug/elephc}"
[ -x "$ELEPHC" ] || { echo "building elephc..."; (cd "$ROOT" && cargo build -q); }
echo "elephc:  $ELEPHC"
# Ports of its own, so a demo already running on the defaults is left alone.
export HTTP_PORT="${HTTP_PORT:-18080}" PROBE_PORT="${PROBE_PORT:-19411}" DASHBOARD_PORT="${DASHBOARD_PORT:-19000}"
export STORE="$WORK/store"
"$ROOT/examples/web-apm/demo.sh" >"$WORK/demo.log" 2>&1 &
DEMO_PID=$!
PAGE="http://127.0.0.1:$DASHBOARD_PORT/"
for _ in $(seq 600); do
  curl -s -o /dev/null "$PAGE" && break
  kill -0 "$DEMO_PID" 2>/dev/null || { cat "$WORK/demo.log" >&2; exit 1; }
  sleep 0.5
done
# Long enough for the charts to draw a curve, the budgets to settle and a few
# requests to be captured.
SETTLE="${SETTLE:-120}"
echo "service up; letting its load run for ${SETTLE}s"
sleep "$SETTLE"

# --- the shots ---------------------------------------------------------------
mkdir -p "$OUT"
shot () {  # shot <name> <height> <hash>
  # --timeout, not --virtual-time-budget: the page holds an event stream open,
  # so the network never goes idle and a virtual-time budget never ends. Six
  # seconds of real time is enough for the page to fetch and draw.
  "$BROWSER" --headless --disable-gpu --hide-scrollbars --no-sandbox \
    --user-data-dir="$WORK/.profile-$1" --window-size="1440,$2" \
    --screenshot="$OUT/$1.png" --timeout=6000 \
    "$PAGE$3" >/dev/null 2>&1 || true
  printf '  %-20s %s bytes\n' "$1.png" "$(stat -f%z "$OUT/$1.png" 2>/dev/null || echo MISSING)"
}

echo "writing $OUT"
# The last five minutes, so the load the script let run fills the charts.
# The top of the page: the sections sidebar, the overview and the resources.
shot transactions 1000 '#a=1&at=300'
# The whole page, down to the budgets.
shot transactions-full 2150 '#a=1&at=300'

# --- programs that send to a collector -----------------------------------------
# The Scripts and Apps sections show only what programs push to a listening
# collector, which the web demo does not: examples/apm-apps runs two versions of
# a worker against one, and the whole page is photographed down to its Apps.
kill -TERM "$DEMO_PID" 2>/dev/null || true
wait "$DEMO_PID" 2>/dev/null || true
export COLLECTOR_PORT="${COLLECTOR_PORT:-19475}" DASHBOARD_PORT="${APPS_DASHBOARD_PORT:-19100}"
export STORE="$WORK/apps-store"
"$ROOT/examples/apm-apps/demo.sh" >"$WORK/apps.log" 2>&1 &
DEMO_PID=$!
PAGE="http://127.0.0.1:$DASHBOARD_PORT/"
for _ in $(seq 600); do
  curl -s -o /dev/null "$PAGE" && break
  kill -0 "$DEMO_PID" 2>/dev/null || { cat "$WORK/apps.log" >&2; exit 1; }
  sleep 0.5
done
APPS_SETTLE="${APPS_SETTLE:-90}"
echo "collector up; letting the workers run for ${APPS_SETTLE}s"
sleep "$APPS_SETTLE"
# Opened on the Scripts section (`as`): a mark per run and the exit code of each
# failed one, then the Apps section below it, each version's error rate, the
# versions and their instances. The transactions table above lists every
# instance, so the whole page would push both out of the picture.
shot scripts-apps 1500 '#a=1&at=300&as=apm-s-scripts'
