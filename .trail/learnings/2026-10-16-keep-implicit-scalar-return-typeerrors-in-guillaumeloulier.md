---
title: Keep implicit scalar return TypeErrors in closure call summaries
author: Guillaumeloulier
date: 2026-10-16
branch: fix/1835-nullable-scalar-coalesce
cost: { minutes: 0, tokens: 0 }
touches:
  - src/optimize/effects/calls.rs
  - src/optimize/exception_flow.rs
  - tests/codegen/optimizer/nullable_return_coalesce.rs
---

## Decided
Closure call summaries must include implicit declared scalar return-boundary TypeErrors, just like named functions and methods. Feed the closure return annotation and body to scalar_return_boundary_may_throw from both direct expression effects and alias effects. Add the same exact TypeError source to direct closure exception summaries so catch routing preserves its handler.

## Tried and ruled out
Checking only the closure body's explicit expressions marked a discarded nullable coalescing call pure/nonthrowing. Both IR modes deleted the reviewed direct closure call and its catch, producing no output instead of caught. Preserving only the call without its throw summary would still allow catch pruning.

## Watch out
Cover direct, assigned and arrow closures, all four scalar return types, and heap cleanup after each caught TypeError. The focused final replay passed 20 PHP codegen cases and 17 harness tests in each IR mode. This review follow-up changes shared AST analysis only; full target CI must replay after the existing fork PR is updated.
