---
title: macOS arm64 debug execution is a named CI gate; libelephc_debug.a now ships
author: OpenCode
date: 2026-11-06
cost: { minutes: 120 }
branch: detached/db62f09226
touches:
  - .config/nextest.toml
  - .github/workflows/ci.yml
  - .github/workflows/release.yml
  - scripts/ci/test-debug-macos.sh
  - tests/codegen/cli.rs
  - CONTRIBUTING.md
  - docs/beyond-php/debugging.md
  - .plans/native-debugger.md
---

## Decided

- Added `debug-tests-macos-aarch64` to the macOS test tier (`macos-15`,
  `extended == true`, so the merge queue or a `ci:macos` label runs it). It
  selects from the full nextest archive:
  - `debugger_spike_macos_aarch64` (AArch64 guard/ABI/frame-read spike);
  - `codegen_tests` `test_cli_debug*` and `test_cli_elephc_break*` (terminal,
    DAP, included files, fibers, exceptions, TCP attach, postmortem, traces);
  - `web_tests` `web_debug*` (worker/pool/request route debugging);
  - the `elephc-debug` unit suite and the `elephc` `debug_cli` parser tests.
- Added `scripts/ci/test-debug-macos.sh` for a local macOS checkout; it runs
  the same focused families with `cargo test` and `RUST_MIN_STACK`.
- The macOS job is part of the `test` aggregate `needs` and the extended tier,
  so a missing/failed run blocks the merge queue.
- Found and fixed a real packaging gap: `libelephc_debug.a` was absent from:
  - `$BRIDGE_CRATES` in the build-archive jobs;
  - `.config/nextest.toml`'s archive include list;
  - the release bridge build, tarball and Homebrew formula.
  `every_bridge_staticlib_ships_in_the_release_tarball` was failing before the
  fix and passes after it. Without the archive copy, archived macOS/Linux
  shards would try to build the debug bridge offline; without the release copy,
  an installed `elephc --debug` would fail to find its bridge.
- Added `test_cli_debug_dap_exception_stops_in_destructor`: an uncaught
  destructor throw stops on the `uncaught` filter with `RuntimeException`, a
  `__destruct` frame, and exit 255; statements after the throw do not run.
  This closes the destructor case in the exception-debugging acceptance.

## Tried and ruled out

- Claiming local macOS execution: this host is Linux AArch64. The job is the
  execution gate; only the merge queue/`ci:macos` run can produce the pass/fail
  evidence. The local script makes the same suite reproducible on a Mac.
- Running the full macOS matrix for a debug-only change: the focused job is
  cheaper and names the debugger surface; the existing codegen/web shards still
  cover the rest.
- Shipping `libelephc_debug.a` only via the compiler's on-demand cargo build:
  release installs have no workspace/source tree, so the bridge must be in the
  tarball/formula. The packaging test enforces both lists separately.
- Using a `throw` filter for the destructor test: a destructor throw can stop
  first as caught while the runtime unwinds cleanup, then again as uncaught.
  The test selects `uncaught` so it asserts one deterministic stop before the
  fatal report.

## Watch out

- The macOS debug job uses the **full** nextest archive, not the codegen-only
  one, because it needs `web_tests`, `elephc_debug` and the macOS spike binary.
- `libelephc_debug.a` must be built before archiving; `BRIDGE_CRATES` now does
  that. The archive include list is shared by the Linux and macOS archives.
- The job's nextest filter is a single expression; adding a new debug test
  family requires extending it (or it still runs in the broad codegen/web
  shards, but not in the focused gate).
- `--web-isolation` source builds are covered by the parser and prebuilt
  request/pool fixtures; the macOS job runs the prebuilt web debug tests.
- The release packaging test is the guard: do not add a bridge to `BRIDGES`
  without adding its archive to the tarball and formula.
