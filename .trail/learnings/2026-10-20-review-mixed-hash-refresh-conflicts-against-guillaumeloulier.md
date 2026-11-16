---
title: Review Mixed hash refresh conflicts against actual EIR ownership and reload paths
author: Guillaumeloulier
date: 2026-10-20
branch: fix/1817-ci-refresh-20261017
touches:
  - src/codegen/lower_inst/builtins/arrays.rs
  - src/codegen/lower_inst/builtins/arrays/basic.rs
  - src/codegen/lower_inst/builtins/arrays/boxed_mutation.rs
  - src/codegen/lower_inst/builtins/arrays/boxed_unshift.rs
  - src/codegen/lower_inst/builtins/arrays/shift.rs
  - src/codegen/lower_inst/builtins/arrays/sort_dispatch.rs
  - src/codegen/lower_inst/builtins/arrays/unshift.rs
  - src/codegen_support/runtime/arrays/array_take_boxed.rs
  - tests/codegen/arrays/assoc_helpers.rs
  - tests/codegen/runtime_gc.rs
---

## Decided

The 20 Trail triplet matches on this refresh are contextual word overlap, not a change back to the old failure modes. These implementations use active EIR runtime-function lowering. The source reload remains in unshift, removed values are retained and boxed before hash unlink, and the five-target emitter test verifies that order together with pop append-history handling. Six fixtures also execute under both Linux architectures.

No serializer projection, class-layout name map, or Synthetic-root inference code is changed. Null receivers are rejected before helper allocation or payload access. The helper operands are already evaluated PHP values; no Stringable conversion or source-pin retirement path is introduced here.

## Tried and ruled out

A three-word match alone cannot distinguish a removed failure description from its regression assertion.

## Watch out

Keep the ownership/reload assertions and executable COW controls as evidence for this rationale. Preserve diagnostics, including missing-key warnings, when checking null receivers. Callable property capture and the boxed runtime callable argument ABI remain separate work; this refresh does not expand either boundary.
