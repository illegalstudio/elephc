---
title: Materialize native spread defaults lazily and check saturated hash indices
author: Guillaumeloulier
date: 2026-10-20
branch: fix/native-default-spread-materialization
touches:
  - crates/elephc-magician/src/interpreter/reflection/parameter_lazy_defaults.rs
  - docs/php/classes.md
  - src/codegen/lower_inst/builtins/eval/default_errors.rs
  - src/codegen_support/runtime/arrays/hash_spread.rs
  - src/ir/instr.rs
  - src/ir_lower/expr/indexed_array_literals.rs
  - src/ir_lower/function.rs
  - src/ir_lower/tests/property_default_scope.rs
  - src/types/deferred_defaults.rs
  - tests/codegen/eval.rs
---

## Decided
Native parameter defaults containing array unpacking use the existing executable EIR default helpers and owned expression registration. The private spread marker always keeps Reflection metadata lazy, so has-default checks and explicit arguments do not materialize a failing default. Preserve declaration class scope for static, instance and constructor helpers.

## Tried and ruled out
The compact native default resolver rejected spreads, and registering only the value did not provide lazy Reflection/callable factories. Native hash spread also scanned max key and incremented through PHP_INT_MAX, wrapping into negative indices. Fix the shared runtime with checked destination next-index history rather than normalizing away the witness. Checking overflow after retaining the rejected value or omitting EIR unwind pins leaked temporary owners; all original strict heap assertions remain.

## Watch out
HashSpread can now throw: mark MAY_THROW and expose destination, promoted source and original independent owning source to cleanup. Probe availability before acquiring an insertion value; retire the runtime frame before entering the common append Error path. String-only/empty spreads stay valid at saturation; object projection does not reindex. Candidate5 passes 80 exact-output/clean-heap profile/IR cases plus macOS/Linux native target shapes, with iOS assembly-only. Reflection ownership depends on PR1953; keep separate pre-existing enum witnesses and do not duplicate PR1256 or close all of issue1340 from this narrower fix.
