---
title: Share saturated eval cursors with normalized native default spreads
author: Guillaumeloulier
date: 2026-10-20
branch: fix/1568-eval-max-key-reviews
touches:
  - ".plans/issue-1494-review-followup.md"
  - crates/elephc-magician/src/interpreter/tests/array_literals.rs
  - tests/codegen/eval.rs
  - tests/codegen/eval/native_default_array_overflow.rs
---

## Decided
Rebase PR1568 onto main487cae6abc7d98c1f16bb99042e491bb8a32e27b preserving normalized native-default spread handling, pop/next-index semantics and both GC test modules. Ordinary implicit entries and private normalized spread entries share take_next_array_key and validate_array_append_key. Keep an integer cursor saturated at PHP_INT_MAX, allow that last slot when absent, and query occupancy before another insertion. Check only after value evaluation and keep all owners in construction leases.

## Tried and ruled out
Choosing the main helper unchanged keeps a floating overflow cursor; combining it with the feature's saturated explicit cursor and a type-only validator can overwrite an occupied maximum in a normalized spread. Two new interpreter regressions exercise five normalized shapes: final-free numeric, occupied, one-free-then-overflow, string-only and empty. The last-free ordinary literal test also passes. Original seven native max-key fixtures stay byte-identical, with exact output and clean heaps across PHP82/85 and both IR modes. Reference and resource cases also remain strict.

A new public native-function default-with-spread fixture aborts in the unchanged callable materializer on both the candidate and a main487+unrelatedPR1874 control. Free-function and static-method Reflection have a separate missing-default witness. Preserve the complete failing fixture and PHP82/85 oracles on the dedicated internal fix/native-default-spread-materialization branch; do not confuse those earlier pipeline failures with this cursor. They are pending global-goal work, not a pass or a weakened original1568 regression.

## Watch out
PHP82 literal construction still continues from a negative integer key; its already-constructed empty-array constant starts at zero. Match full keys and values through foreach rather than unrelated iterator/JSON lifetime baselines. Current evidence includes136 focused Magician units,80 original/reference/resource CLI runs,22 profile controls,24 issue1647 native runs and40 five-target program/runtime pairs with24 clean native executions. Both Linux Magician checks pass. Both iOS Magician checks still fail first in byte-identical main pcntl OS-cfg source; assembly is a separate checkpoint. Freeze compiler, bridge archives and matching dependencies, keep staged historical integration worktrees untouched, and publish only to the existing fork branch with an exact lease.
