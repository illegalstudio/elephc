---
title: Review lexical Trail conflicts for copied array storage
author: Guillaumeloulier
date: 2026-10-17
branch: fix/1861-empty-array-copy
cost: { minutes: 1473, tokens: 11053309 }
touches:
  - src/ir_lower/stmt/array_write_core.rs
  - src/ir_lower/stmt/local_assignments.rs
  - src/ir_lower/stmt/nested_array_writes.rs
  - src/ir_lower/stmt/typed_foreach.rs
  - src/types/array_keys.rs
  - src/types/checker/loop_storage.rs
  - src/types/checker/stmt_check/assignments/arrays.rs
  - src/types/mod.rs
  - tests/codegen/arrays/assoc.rs
  - tests/codegen/runtime_gc.rs
---

## Decided

The #1861 implementation is retained after reviewing all eight Trail check lexical conflicts against exact source and final artifacts. The contextual conversion retains borrowed/provisional concrete local loads before consuming ArrayToHash. Negative/sparse writes promote indexed storage to hashes, so no dropped key or zero-filled gap is restored. No parser Synthetic grouping or runtime conditional declaration logic changed.

## Tried and ruled out

Trail matched the words for the bad baseline and rejected candidates inside the same learning against the passing fix and its regression tests. It also treated passing-test descriptions as a ruled-out implementation. Those matches are not source-level contradictions: source consumption candidates 1 and 2 stay rejected; the final source uses an explicit retained input lease and passes 888 focused tests.

## Watch out

The tests/codegen/runtime_gc.rs overlap with upstream ownership work is only a module registration; preserve both registrations when rebasing. Final receipts pin the eight PHP fixtures, 80 all-target object combinations, 48 native macOS/Linux executions, 16 original-shape executions and clean heaps. Seven PHP profile copy/direct comparisons do not establish native old-PHP parity; #1494/#1568 owns that cursor policy. Never edit the previous learnings to silence a lexical check.
