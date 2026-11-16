---
title: Share unpack storage classification across call dispatch paths
author: Guillaumeloulier
date: 2026-10-17
branch: fix/1836-nullable-array-merges
cost: { minutes: 3247, tokens: 20905339 }
touches:
  - docs/internals/the-ir.md
  - src/ir_lower/expr/array_access.rs
  - src/ir_lower/expr/dynamic_spreads.rs
  - src/ir_lower/expr/indexed_array_literals.rs
  - src/ir_lower/expr/positional_spreads.rs
  - tests/codegen/runtime_gc.rs
  - tests/codegen/types/nullable_array_merge.rs
  - tests/codegen/runtime_gc/nullable_array_read_owners.rs
  - ".plans/issue-1836-nullable-array-merges.md"
  - src/codegen/lower_inst/arrays.rs
---

## Decided

All unpack-source classifiers must use spread_source_storage_type_for_ir: literal destination selection, positional argument selection and the early boxed-spread binder selector. ErrorSuppress delegates its storage classification to the inner source. Ordinary literal elements and lazy merge consumers keep the logical nullable type. Function, instance-method and captured-closure calls now preserve hits and reject normal/suppressed misses with the unpack diagnostic.

## Tried and ruled out

Fixing only positional_spreads left named functions on the early boxed binder: a missed native array read became an ArgumentCountError instead of the unpack error. The one remaining selector now uses the same storage projection. Candidate 5 builds warning-free and passes all 32 focused tests in each IR mode, including both original CI failures and throwing-offset receiver cleanup. Earlier candidate artifacts remain labeled separately; full target validation is recorded with the final artifact.

## Watch out

The existing main compiler also leaks one 72-byte destination array when a caught array-literal unpack rejects null. It also reports Error rather than PHP TypeError for a native function unpack miss. Those are separately reproduced baseline bugs, not proof that this branch is clean in those cases. Target receipts flag the known heap leak explicitly; never count those executions as clean heaps. Ordinary object read elements must still box PHP null, and offset-on-null warnings and key side effects must be preserved. Never generalize the proven-null receiver pin lifetime to a destructor-bearing payload.
