#!/usr/bin/env bash
# Purpose:
# Runs a real headless VS Code debug session against `elephc debug --dap` through
# the minimal development extension in tests/editor/vscode-extension.
#
# Called from:
# - A developer or CI job with Node, `@vscode/test-electron`, and (on Linux)
#   Xvfb installed.
#
# Key details:
# - `VSCODE_TEST_ELECTRON_DIR` must point at the node_modules directory that
#   contains `@vscode/test-electron`; it is exposed through NODE_PATH.
# - The first run downloads the VS Code test build into `~/.vscode-test`
#   (network required). Linux headless runs go through `xvfb-run`.
# - The fixture is compiled once before VS Code starts, so the extension adapter
#   proxy starts a debug binary rather than a PHP compile.
# - The suite writes `vscode-smoke.json`/`vscode-smoke.ok`; this script prints the
#   report and fails if any breakpoint, frame, local or step assertion failed.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
ELEPHC_BIN="${ELEPHC_BIN:-$ROOT/target/debug/elephc}"
VSCODE_TEST_ELECTRON_DIR="${VSCODE_TEST_ELECTRON_DIR:?set VSCODE_TEST_ELECTRON_DIR to the node_modules directory containing @vscode/test-electron}"

tmp="$(mktemp -d "${TMPDIR:-/tmp}/elephc-vscode.XXXXXX")"
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

export ELEPHC_BIN
export ELEPHC_SMOKE_DIR="$tmp"
export ELEPHC_SMOKE_PROGRAM=main
export NODE_PATH="$VSCODE_TEST_ELECTRON_DIR"

if command -v xvfb-run >/dev/null 2>&1; then
  runner=(xvfb-run -a node)
else
  runner=(node)
fi

"${runner[@]}" "$ROOT/tests/editor/vscode-run.js"
test -f "$tmp/vscode-smoke.ok"
cat "$tmp/vscode-smoke.json"
