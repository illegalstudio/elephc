#!/usr/bin/env bash
# Purpose:
# Runs the focused native-debugger suites on a macOS arm64 host: terminal/DAP,
# TCP attach, postmortem, web disconnect/resume, runtime units and CLI parsing.
#
# Called from:
# - A macOS developer, or manually to reproduce the `debug-tests-macos-aarch64`
#   merge-queue job outside the archived-nextest CI path.
#
# Key details:
# - This is intentionally a focused subset, not the full suite. The archived CI
#   job selects the same families from nextest; running `cargo test` here builds
#   the bridge staticlibs on demand, which is what a local macOS checkout needs.
# - `RUST_MIN_STACK` mirrors the CI value so deep lowering fixtures see the same
#   worker stack on ARM64.
# - `ELEPHC_TEST_PREBUILT_BRIDGES` must stay unset locally: the archives are not
#   in a nextest extraction here.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
cd "$ROOT"
export RUST_MIN_STACK="${RUST_MIN_STACK:-33554432}"

cargo test --test debugger_spike_macos_aarch64
cargo test --test codegen_tests test_cli_debug
cargo test --test codegen_tests test_cli_elephc_break
cargo test --test web_tests web_debug_disconnect
cargo test -p elephc-debug
cargo test --bin elephc debug_cli
