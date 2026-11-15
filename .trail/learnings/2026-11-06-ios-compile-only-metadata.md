---
title: iOS debug is compile-only metadata; interactive launch stays refused
author: OpenCode
date: 2026-11-06
branch: detached/db62f09226
cost: { minutes: 75 }
touches:
  - src/cli.rs
  - tests/codegen/cli.rs
  - docs/beyond-php/debugging.md
  - docs/compiling/cli-reference.md
  - docs/compiling/targets.md
  - .plans/native-debugger.md
---

## Decided

- `--debug` on `ios-arm64` / `ios-sim-arm64` now accepts the compile-only forms:
  - `--emit-asm` (writes `<stem>.s` and `<stem>.edbg.json`, no link);
  - `--emit staticlib` / `--emit cdylib` (library carries embedded metadata);
  - `--check` / `--emit-ir` remain analysis-only and emit no metadata.
- A default interactive `--debug --target ios-*` (executable) still fails before
  codegen with a message naming `--emit-asm`, `elephc monitor --calls` and
  `elephc test --coverage`.
- `validate_target_output` gained an `emit_asm` argument. The generic iOS
  executable rule now also allows `--emit-asm`, which is how the metadata build
  reaches the backend without linking.
- The `--debug` library refusal is now `emit.is_library() && !target.is_ios()`:
  non-iOS libraries still have no process entry point, while an iOS library is
  the documented host-consumed compile-only artifact.
- Simulator debugging is gated exactly like device debugging in v1; no socket or
  sandbox claim is made without an execution runner.

## Tried and ruled out

- Allowing `--debug` on iOS with no output mode and relying on the later linker:
  a CLI executable is not a signed `.app` and cannot be launched. Rejecting early
  with the capture-tool guidance is the actionable path.
- Enabling `--with-monitoring` for iOS libraries: that runtime needs a `main` to
  initialize its control channel, exactly the reason non-iOS libraries refuse it.
  Compile-only debug metadata does not need the monitoring probe.
- Claiming Simulator debug support from `ios-sim-arm64` build success: no host
  process/socket smoke test exists on this Linux runner. The gate stays explicit.
- Emitting metadata only via `--source-map`: `.edbg.json` is the debugger contract
  (functions, locals, sites, classes), while the source map is a separate schema.
  `--emit-asm` already writes both the `.s` and the `.edbg.json`.

## Watch out

- `--debug --emit-asm` is a compile artifact: the debug hooks reference the
  `elephc_debug` runtime symbols, but no link resolves them. That is expected for
  an assembly stop.
- A linked iOS library still needs macOS + the matching Xcode SDK. This Linux
  runner can produce the assembly and metadata but cannot produce the final `.a`.
- `--emit-asm` on iOS no longer trips the "iOS executable" rule because no
  executable is produced; the rule now checks `emit_asm`.
- The e2e test compiles both iOS targets with `--debug --emit-asm`, asserts
  `@fn name=traceMe`, `__elephc_debug_site`, and non-empty sources/functions/sites
  in `main.edbg.json`.
- Documentation matrix now states "compile-only metadata" for both iOS targets
  and "refused: no attachable process" for interactive/DAP.
