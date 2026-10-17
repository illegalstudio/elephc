---
title: Include named function boundary checks in first-class throw domains
author: Guillaumeloulier
date: 2026-10-16
branch: fix/1835-nullable-scalar-coalesce
cost: { minutes: 0, tokens: 0 }
touches:
  - src/optimize/exception_flow.rs
  - src/optimize/exception_flow/callables.rs
  - tests/codegen/optimizer/nullable_return_coalesce.rs
---

## Decided
A first-class named-function invocation crosses the same declaration-level entry/return checks as a direct named call. ExceptionFlowAnalysis::callable_expr_throws must combine its body summary with the authoritative function_object_ref_checks set, adding exact TypeError when that boundary can reject the value.

## Tried and ruled out
Reading only function_throws lost the lazy scalar return guard. The call stayed observable through effects, but catch routing removed its TypeError handler. The reviewed program then exited with an uncaught TypeError in both IR modes. Unknown-call fallback did not help because the named body had a known pure summary.

## Watch out
Keep call effects and exception domains consistent. Test first-class literals and assigned descriptors for all four scalar return types, plus a successful nonthrowing result. The focused replay passes 21 PHP codegen cases and 17 harness controls in each IR mode, including clean heaps after caught exceptions. The guard set also covers declared entry checks, so a descriptor must not bypass it.
