#!/usr/bin/env bash
# Purpose:
# Runs the real nvim-dap smoke test against `elephc debug --dap`.
#
# Called from:
# - A developer or CI job with Neovim and the nvim-dap/nvim-nio checkouts.
#
# Key details:
# - `NVIM_BIN`, `NVIM_DAP_DIR` and `NVIM_NIO_DIR` are required; `ELEPHC_BIN`
#   defaults to the repository's `target/debug/elephc`.
# - The fixture is compiled once before Neovim starts, so the DAP handshake is
#   what the adapter timeout measures.
# - Exit status is non-zero when the breakpoint, step, local values, or normal
#   termination do not match the script's assertions.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ELEPHC_BIN="${ELEPHC_BIN:-$ROOT/target/debug/elephc}"
NVIM_BIN="${NVIM_BIN:-nvim}"
NVIM_DAP_DIR="${NVIM_DAP_DIR:?set NVIM_DAP_DIR to a nvim-dap checkout}"
NVIM_NIO_DIR="${NVIM_NIO_DIR:?set NVIM_NIO_DIR to a nvim-nio checkout}"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/elephc-nvim-dap.XXXXXX")"
trap 'rm -rf "$tmp"' EXIT

cat >"$tmp/main.php" <<'PHP'
<?php
function add(int $a, int $b): int {
    return $a + $b;
}
function run(): void {
    $x = add(2, 3);
    echo $x, "\n";
}
run();
PHP

"$ELEPHC_BIN" --debug "$tmp/main.php" >/dev/null
output="$(
  cd "$tmp"
  ELEPHC_BIN="$ELEPHC_BIN" \
  ELEPHC_SMOKE_DIR="$tmp" \
  ELEPHC_SMOKE_LINE=6 \
  NVIM_DAP_DIR="$NVIM_DAP_DIR" \
  NVIM_NIO_DIR="$NVIM_NIO_DIR" \
  "$NVIM_BIN" --headless -u NONE -n -l "$ROOT/tests/editor/nvim-dap-smoke.lua" 2>&1
)"
printf '%s\n' "$output"

grep -q 'STOPPED 1 reason=breakpoint' <<<"$output"
grep -q 'FRAME run line=6' <<<"$output"
grep -q 'LOCAL \$x = int(0)' <<<"$output"
grep -q 'STOPPED 2 reason=step' <<<"$output"
grep -q 'FRAME run line=7' <<<"$output"
grep -q 'LOCAL \$x = int(5)' <<<"$output"
grep -q 'TERMINATED' <<<"$output"
grep -q 'SMOKE OK' <<<"$output"
