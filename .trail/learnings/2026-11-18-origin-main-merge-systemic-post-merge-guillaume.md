---
title: "origin/main merge: systemic post-merge breakage and how it showed"
author: Guillaume
date: 2026-11-18
branch: merge-main
cost: { minutes: 112, tokens: 328607 }
touches:
  - src/builtins/registry.rs
  - src/codegen_support/emit.rs
  - src/codegen_support/runtime/exceptions
  - crates/elephc-magician/src/interpreter/builtins/registry/mod.rs
---

## Decided
- Merge (not rebase) origin/main into the branch: one resolution pass for 558 conflicted files / 1486 hunks, batched by
  area to subagents; generated docs and scripts/docs registries taken from main and regenerated later.
- Resolution rules: main's design wins, branch behaviour is ported onto it; php -n decides behaviour; Symfony support
  stays; main's renames are real; never delete a test or feature to go green.
- After `cargo check` is clean, run the FULL suites before anything else: compile-clean merges still broke ~98% of
  codegen tests through a handful of shared causes.

## Tried and ruled out
- Trusting `cargo check`: it passed while every compile panicked at run time on registry invariants.
- Letting a batch "take main wholesale" for a file: it silently dropped branch fixes (eval interpreter, mysqli TLS key);
  every wholesale choice needed a re-port pass.

## Watch out
- Registry duplicates only fail at run time: a builtin present in both the shared contract and the AOT registry
  (setlocale) panics registry.rs:148; a duplicate eval builtin name (join) panics the magician inventory.
- Content-hash renaming of shared invoker bodies must not rename the out-of-line `__gc_argument_owners` visitors;
  parallel codegen chunks must emit shared visitors once (helper marker) or the assembler sees duplicate symbols.
- Hard-coded runtime frame offsets on one side (TRY_HANDLER_SLOT_SIZE 224 → 336 on the branch) break the other side's
  callers that read 224/232/240/248 directly: grep for literal offsets when a layout constant changed.
- Runtime-created Throwables must use throwable_layout (payload size / PREVIOUS_OFFSET changed in the merge).
- `cb.sh test` in the background must read stdin from /dev/null (a readline test waits on stdin).
