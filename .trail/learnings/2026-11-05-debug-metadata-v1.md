---
title: Debug metadata v1 lives in assembly markers, sidecar and a rooted JSON table
author: OpenCode
date: 2026-11-05
branch: detached/db62f09226
cost: { minutes: 140 }
touches:
  - src/debug_metadata.rs
  - src/codegen/block_emit.rs
  - src/codegen/context.rs
  - src/codegen/shared_state.rs
  - src/pipeline/backend.rs
  - src/pipeline/output.rs
  - src/link_plan.rs
  - src/linker/bridges.rs
  - src/linker/command.rs
  - docs/compiling/debug-metadata.md
  - tests/codegen/cli.rs
---

## Decided

- Metadata v1 is derived from the assembly markers, not a second compiler walk:
  `@file` gives sources, `@fn` gives function kind/flags/ranges, `@src` gives
  sites, and new `@local` markers give name, `fp-offset`, PHP type, LocalKind,
  storage representation, ref-cell state offset and parameter flags.
- `src/debug_metadata.rs` writes `<stem>.edbg.json` and embeds the same JSON
  behind `elephc_debug_metadata` / `elephc_debug_metadata_len`.
  `elephc_debug_metadata_len` points at a `.quad` byte count, not at a symbol
  size.
- The embedded table is unreferenced by generated code. `--debug` therefore adds
  a linker root (`-Wl,-u,elephc_debug_metadata` on Linux, `-u` on macOS) after
  planning. Without it, `--gc-sections` / `-dead_strip` silently removed the
  table; `nm` and `strings` both came back empty.
- Source ids are canonicalized by physical path, so include variants sharing a
  path produce one source entry.

## Tried and ruled out

- Relying on `.globl` alone to keep the metadata: the linker dropped the section.
  A `.data` pointer would need its own root and would still not make the runtime
  address discoverable; the linker `-u` is the direct contract.
- Building metadata from a second EIR walk: the assembly markers already carry
  the source/function/local contract and stay aligned with source maps/DWARF.
- Manual JSON formatting for the complex nested document: `serde_json` keeps the
  schema valid and the tests readable; the source map keeps its lean manual
  renderer because its shape is simpler.

## Watch out

- `LinkPlan::without_redundant_embedded_bridges` and `bridges::resolve_with`
  rebuild plans with `LinkPlan::from_items`. They must carry `undefined_symbols`
  forward; without that the root silently disappears on programs that resolve
  any bridge (this was the second regression caught while verifying `nm`).
- `@local` currently emits named `PhpLocal`/`StaticLocal`/`RefCell`/
  `ReturnRefCell` slots. Compiler-internal names such as `*#cow` and
  `__elephc_*` still appear in metadata; filter them before the IDE surface if
  they become noise.
- The embedded JSON exists and is symbol-addressable, but no runtime reader/agent
  consumes it yet. Do not claim interactive debugging from metadata alone.
