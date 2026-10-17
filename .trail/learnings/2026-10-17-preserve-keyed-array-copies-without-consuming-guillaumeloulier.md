---
title: Preserve keyed array copies without consuming provisional local owners
author: Guillaumeloulier
date: 2026-10-17
branch: fix/1861-empty-array-copy
cost: { minutes: 1321, tokens: 10415949 }
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

Array-to-hash assignment contextualization follows the destination storage contract for every RHS shape, including a variable copy. Reuse coerce_typed_assign_value, but exclude value_is_owned_unboxed_local_load from consumable owners: these loads are provisional and their compensating release is pruned if the frame slot stays concrete. Conversion must retain an independent input lease before consuming it, just as merge-temp payload conversion already does.

A negative literal integer key can never be a packed slot, even after an array has acquired a concrete element type. Share that proof across statement checking and loop storage analysis, and enforce keyed promotion at the EIR write site as well. Branch joins and loop rebinding can still expose indexed storage there. The existing string-key promoter is now lower_keyed_array_promotion; numeric keys are coerced once before HashSet. Empty nonzero static keys also take that path.

## Tried and ruled out

The frozen main6459 compiler drops negative writes into empty and populated copies, and zero-fills positive sparse writes into empty copies. Generalizing the assignment conversion alone consumed a borrowed source and made JSON inspection crash. Calling the existing typed helper without correcting its provisional-owner test did not solve that: minimal frozen-library IR still showed ArrayToHash directly consuming LoadLocal. Candidates1/2 are invalid and retained separately.

Candidate3 passes eight new PHP value/warning/heap cases in both IR modes and93 scoped associative-array/GC/harness tests per mode. Tests cover independent copies, populated input, references taken on existing entries, negative integer-string/float normalization, conditional writes, and object payloads under allocation pressure. The original draft used unsupported literal reference-element syntax; the corrected fixture uses an alias to an existing element. Final broader CLI and target validation is recorded separately with frozen artifact hashes.

## Watch out

A provisional LoadLocal owner is not an independent reference. Do not rely on balanced heap totals alone: verify source values and payload lifetime after intervening allocation. ArrayToHash consumes its input; a local mutation moves its binding owner, while an assignment conversion must acquire its own input.

PHP8.2's next-index policy for an empty array is tracked independently by1494/PR1568. Same-profile copy/direct checks prove copy invariance, not native PHP8.2 parity; only PHP8.5 is installed here. Keep all source, worktrees, temporary files and artifacts inside Elephc. Freeze linked probes/runtime emitters before another worktree replaces shared Cargo dependencies. iOS assembly witnesses must put the relevant code in an emitted function.
