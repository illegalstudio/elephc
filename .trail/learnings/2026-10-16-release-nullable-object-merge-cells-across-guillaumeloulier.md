---
title: Release nullable object merge cells across nominal return widening
author: Guillaumeloulier
date: 2026-10-16
branch: fix/1836-nullable-array-merges
cost: { minutes: 0, tokens: 0 }
touches:
  - src/ir_lower/stmt/return_coercions.rs
  - src/optimize/control/prune/expr.rs
  - tests/codegen/types/nullable_array_merge.rs
---

## Decided
Project a nullable object merge to a concrete object return ABI based on payload kind, not nominal class-name equality. A child object and its parent/interface return share the same refcounted object handle; frontend checking owns PHP assignability. Acquire the payload with owning MixedUnbox, then retire the old owning merge cell. Callable projection remains callable-only, and primitive/dynamic Mixed casts keep their ordinary paths.

## Tried and ruled out
Requiring every union member to equal the declared return class skipped cleanup for a valid child-to-parent return. A 40-call positive inheritance control produced the correct checksum but leaked 80 blocks/3808 bytes, because both cells and their retained objects stayed live. The kind-compatible projection passes the same loop with a clean heap.

## Watch out
Pair valid subtype-return controls with the miss cases; identical-class tests alone do not exercise nominal widening. Keep the PHP checker contract separate from physical payload ownership. A constant-null coalesce retained for a read must have its own match arm so it cannot fall through to the known-non-null pruning rule. A suppressed populated read is a positive branch control.
