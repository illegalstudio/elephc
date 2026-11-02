---
title: Post-rebase eval numbers and the relocated catalog guard
author: Guillaume
date: 2026-10-21
branch: perf/build-performance
touches:
  - examples/eval/main.php
  - src/codegen/lower_inst/builtins/eval/context_initialization.rs
  - src/codegen/eval_inventory_helper.rs
---

## Decided
After rebasing onto `origin/main` (666 commits ahead, which restructured the eval initialization), the numbers in "Eval bridge inventory is emitted once per program" are superseded by these, measured A/B on the same machine with `examples/eval/main.php`:

- pristine `origin/main`: 32,380,799 lines / 770 MB / 477 s
- this branch: 5,554,220 lines / 156 MB / 81 s
- program stdout: byte-identical

The upstream restructure moved the registration block out of `ensure_eval_context` into `ensure_eval_native_catalog` in a NEW file, `src/codegen/lower_inst/builtins/eval/context_initialization.rs`. That is where the guard (`__elephc_eval_context_native_catalog_ready`) and the `publish_native_catalog` call now live, so the module-level helper (`register_eval_inventory`, body in `context_registration.rs`, helper emitted by `src/codegen/eval_inventory_helper.rs`) reproduces that whole transaction rather than just the old inline list.

`ensure_eval_native_catalog` now simply loads the context handle and calls the helper. Both loaders survive in `scope_context.rs`: `load_eval_context_local_to_arg` (frame slot, still used by the initialization path) and `load_eval_context_source_to_arg` (module symbol, used by the registration emitters).

## Watch out
A rebase of this area is not a text merge. Upstream renamed and relocated the enclosing function, so re-derive where the guard lives before resolving; a textual "take mine" silently reinstates the quadratic block under a new name.

The stale staticlib trap: after such a rebase, `libelephc_magician.a` in a reused `CARGO_TARGET_DIR` still lacks the new `__elephc_eval_context_*` bridge symbols, and every eval program fails to link with `symbol(s) not found`. Run a full `cargo build` (all default members) before believing a link failure is real. Confirm against a pristine `origin/main` worktree before calling anything a regression — `walk3`-style duplicate-declaration fixtures already fail there.
