#!/usr/bin/env bash
# Regenerate the screenshots in docs/images/profiling/ from a real capture.
#
# The images in the profiling guide are not mockups: this script compiles the
# demo service under scripts/docs/profiling_demo/, profiles it, and photographs
# the pages the compiler actually produced. Run it whenever the page changes,
# and commit whatever moved.
#
#   scripts/docs/profiling_screenshots.sh
#
# Needs macOS (elephc monitor's sampler is /usr/bin/sample) and a headless
# Chromium. It looks for one in $CHROME, then Playwright's cache, then the
# usual application bundles.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
DEMO="$ROOT/scripts/docs/profiling_demo"
OUT="$ROOT/docs/images/profiling"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

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

# --- the binary --------------------------------------------------------------
ELEPHC="${ELEPHC:-$ROOT/target/release/elephc}"
[ -x "$ELEPHC" ] || { echo "building elephc..."; (cd "$ROOT" && cargo build --release -q); }
echo "elephc:  $ELEPHC"

# --- the captures ------------------------------------------------------------
cp "$DEMO"/shop.php "$DEMO"/shop_v2.php "$WORK/"
cd "$WORK"

# Exact: every dimension, the query panel, the per-function source view, checks.
cp "$DEMO/budget.elephc" "$WORK/.elephc"
"$ELEPHC" monitor shop.php --instrument --html exact.html >/dev/null 2>&1 || true

# Sampled: the per-line source view, which needs the dSYM rather than hooks.
"$ELEPHC" monitor shop.php --duration 6 --html sampled.html >/dev/null 2>&1 || true

# A/B: the same service after the N+1 is folded into one query.
"$ELEPHC" monitor shop.php --instrument --save before.json >/dev/null 2>&1 || true
"$ELEPHC" monitor shop_v2.php --instrument --baseline before.json --html diff.html >/dev/null 2>&1 || true

for f in exact.html sampled.html diff.html; do
  [ -s "$f" ] || { echo "capture failed: $f was not written" >&2; exit 1; }
done

# The page's own dark choice lives in localStorage, which a one-shot headless
# screenshot cannot seed (file:// has an opaque origin, and the shell exits
# before the store is flushed). Patch the copy's saved-choice read instead —
# the same state picking Dark in the ☰ menu produces.
python3 - <<'PY'
needle = "const savedTheme = localStorage.getItem(THEME_KEY);"
t = open("exact.html", encoding="utf8").read()
assert needle in t, "theme hook moved; update profiling_screenshots.sh"
open("exact_dark.html", "w", encoding="utf8").write(
    t.replace(needle, "const savedTheme = 'dark';", 1))
PY

# --- the shots ---------------------------------------------------------------
mkdir -p "$OUT"
shot () {  # shot <name> <page> <hash>
  "$BROWSER" --headless --disable-gpu --hide-scrollbars --no-sandbox \
    --user-data-dir="$WORK/.profile-$1" --window-size=1440,900 \
    --screenshot="$OUT/$1.png" --virtual-time-budget=6000 \
    "file://$WORK/$2$3" >/dev/null 2>&1 || true
  printf '  %-18s %s bytes\n' "$1.png" "$(stat -f%z "$OUT/$1.png" 2>/dev/null || echo MISSING)"
}

echo "writing $OUT"
shot call-graph   exact.html      '#m=time'
shot call-graph-dark exact_dark.html '#m=time'
shot memory       exact.html      '#m=mem'
shot flame        exact.html      '#v=flame'
shot queries      exact.html      '#v=sql'
shot source       exact.html      '#v=src'
shot checks       exact.html      '#v=chk'
shot source-lines sampled.html    '#v=src'
shot diff         diff.html       ''
