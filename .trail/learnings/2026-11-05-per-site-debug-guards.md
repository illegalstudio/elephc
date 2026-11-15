---
title: Per-site debug guards need a preserving generated wrapper, not a bare Rust call
author: OpenCode
date: 2026-11-05
branch: detached/db62f09226
cost: { minutes: 120 }
touches:
  - src/codegen/debug_sites.rs
  - src/codegen/block_emit.rs
  - src/codegen/frame.rs
  - src/codegen/shared_state.rs
  - src/codegen/mod.rs
  - crates/elephc-debug
  - src/linker/bridges.rs
  - tests/codegen/cli.rs
  - docs/compiling/debug-metadata.md
---

## Decided

- A `--debug` source-line transition gets a stable `site=` id on its `@src`
  marker and a call to the generated `__elephc_debug_site` wrapper. Guards are
  deduplicated within each basic block (`debug_last_source`), and the metadata
  parser uses the same id, so runtime counters and `.edbg.json` line up.
- The wrapper saves the complete caller-saved set at the instruction boundary:
  AArch64 x0-x17/q0-q7/NZCV, x86_64 rax/rcx/rdx/rsi/rdi/r8-r11/xmm0-xmm15/flags.
  It then calls `elephc_debug_site_inner` from `crates/elephc-debug`.
- Each site has a one-byte BSS flag (`elephc_debug_site_flags`). The guard loads
  that flag and skips the wrapper when zero. `elephc_debug_boot` arms all flags
  only when `ELEPHC_DEBUG_SITES=1`.
- `elephc_debug_dump` runs in the main/web epilogue. Without the environment
  variable the flags stay zero, no hook is called, and a debug run stays quiet.
- The runtime crate is a bridge (`libelephc_debug.a`, flag `debug`); `--debug`
  force-links it. The bridge declares the infrastructure monitoring policy.

## Tried and ruled out

- Calling the Rust hook directly from the site: the call clobbers caller-saved
  EIR values at an arbitrary instruction boundary. The generated wrapper is the
  ABI firewall; the spike had already proved its register set.
- A separate per-site dump from assembly: the Rust hook owns fixed atomic
  counters and the opt-in summary, which is simpler and keeps the hook
  async-signal-free (ordinary call, no allocator).
- Leaving the guard always-on: the first slice did that only to validate the
  codegen path. The current path uses the per-site flag, so a normal `--debug`
  run pays one load+branch per line transition and no Rust hook call.

## Watch out

- The guard is deduplicated per basic block, not globally per line: a line that
  appears in several blocks keeps one guard per block. That is intentional for
  re-entry, but it means two site ids can name the same source line.
- `ELEPHC_DEBUG_SITES` now arms the flags as well as the dump. Without it, the
  hook call is skipped; only the guard load/branch remains.
- The wrapper is emitted only when `--debug` is set; a normal binary must not
  reference `elephc_debug_site_inner`.
- Every target's wrapper is emitter-level assembly. The macOS path is
  compile-tested for symbol prefixes but has not run on a macOS host yet.
- The generated metadata site ids are compiler-local and stable only for one
  build; never persist them across rebuilds.
