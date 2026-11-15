---
title: The debug preset is useful without the agent: DWARF, symbols, no source-erasing passes
author: OpenCode
date: 2026-11-05
branch: detached/db62f09226
cost: { minutes: 70 }
touches:
  - src/cli.rs
  - src/pipeline.rs
  - tests/codegen/cli.rs
  - docs/compiling/cli-reference.md
  - docs/compiling/output-and-diagnostics.md
---

## Decided

- `--debug` is a real compile preset, not an alias for `--debug-info`:
  - implies `--debug-info`;
  - keeps symbols;
  - forces EIR optimization off;
  - sets `debug_build` so the AST pipeline skips folding, propagation,
    post-opt pruning, DCE and unreachable-declaration pruning;
  - refuses `--emit cdylib|staticlib` because there is no process entry point to
    launch or attach a debugger to.
- `--debug-info` alone remains the metadata-only artifact and does not change
  optimization or symbol stripping.
- The debug preset is the artifact half of the debugger. The in-process agent,
  site guards and DAP/CLI are still the later `elephc-pt3u/i21x/ms89` work.

## Tried and ruled out

- Reusing only `--debug-info` for `--debug`: it leaves EIR optimization and the
  AST folding/DCE passes active, so a breakpoint can bind to a line the
  optimizer erased.
- Skipping `normalize`: it is required for codegen semantics; only the
  source-erasing passes are skipped.
- Allowing `--debug --emit cdylib`: there is no process lifecycle to stop,
  attach or step. The CLI refuses before codegen.

## Watch out

- `--debug` intentionally keeps unreachable declarations. That is larger and
  sometimes exercises code paths a release build prunes; the focused fixture
  passes, but broader debug-build coverage is a follow-up.
- Keep the default path untouched: only `debug_build` gates the skipped passes.
- The flag currently produces a DWARF/symbols/no-optimization artifact; do not
  document it as a complete interactive debugger until the agent and guards
  are linked.
