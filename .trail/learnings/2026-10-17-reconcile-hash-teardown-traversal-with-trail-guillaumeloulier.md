---
title: Reconcile hash teardown traversal with Trail lexical checks
author: Guillaumeloulier
date: 2026-10-17
branch: fix/hash-destruction-insertion-order
touches:
  - ".plans/hash-destructor-insertion-order.md"
  - docs/internals/memory-model.md
  - src/codegen_support/runtime/arrays/hash_free_deep.rs
  - src/codegen_support/runtime/arrays/hash_layout.rs
  - tests/codegen/runtime_gc.rs
  - tests/codegen/runtime_gc/hash_destruction_order.rs
---

## Decided

The insertion-order hash teardown patch intentionally replaces physical bucket scanning with the existing head/next chain on both architectures. Trail check reported 12 lexical matches in the plan, memory-model prose and regression tests. They are false positives: the patch does not flatten Synthetic groups, change heap-kind/successor-resynchronization call arguments, move worktrees, or alter key tags. It preserves exact PHP oracles and native outputs as required by the earlier learning; quoting that requirement is not a reversal.

## Tried and ruled out

Do not weaken the tests or remove evidence language to silence lexical checks. A clean heap alone cannot establish ordering. The regressions compare complete output, exercise insertion-order links after growth/deletion/COW, and check cleanup with throwing destructors. The implementation changes traversal only, retaining pending-exception storage, per-entry ownership and final allocation releases.

## Watch out

The reported overlaps are in memory-model documentation and test module registration. Keep other branches' changes when rebasing. Record this reconciliation in the commit and PR description; existing learnings are immutable.
