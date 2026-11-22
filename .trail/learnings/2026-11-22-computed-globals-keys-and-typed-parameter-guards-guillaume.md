---
title: Computed $GLOBALS keys, and typed-parameter TypeErrors checked per call site
author: guillaume
date: 2026-11-22
branch: bead-opt-leftovers2
touches:
  - src/globals_dynamic.rs
  - src/globals_snapshot_prelude.rs
  - src/parser/expr/pratt.rs
  - src/ir_lower/program/runtime_features.rs
  - src/optimize/return_boundaries.rs
  - src/optimize/effect_analysis.rs
  - src/optimize/effects/calls.rs
---

1. `$GLOBALS[$expr]` (php 8.1+: read, write, compound, ??=, ++/--, isset, unset, `[]=` below the element). The parser keeps the raw `$GLOBALS[$key]` ArrayAccess; `crate::globals_dynamic::lower` (called in pipeline.rs and the codegen/ir_lower test harnesses, before backend_gap_prelude) turns reads into elements of the `__elephc_globals_array()` snapshot, writes into `__elephc_globals_set($k,$v)`, unsets into `__elephc_globals_unset($k)`. globals_snapshot_prelude declares those helpers as a dispatch over every global name the program can spell, plus a static side table (`__elephc_globals_extra`) for names nothing spells (visible only through the snapshot). Refused loudly: `$r = &$GLOBALS[$k]` and nested writes used as an expression. A harness that builds its own pipeline must call `globals_dynamic::lower` or a computed key stays a raw `$GLOBALS` array access.
2. The open callable ladder (a callable PARAMETER, `$callback()`) gets one wrapper per builtin, whose bodies call runtime routines (`__rt_member_exists`, `__rt_core_error_handler_pop`, `__rt_object_clone_shallow_boxed`) nothing in the instruction stream mentions. `lowered_runtime_features_with` now enables class_methods_introspection/handler_state/object_clone whenever a function has an open boxed callable site (`function_has_open_callable_site`). Not done: giving callable parameters provenance across call boundaries so the ladder is smaller.
3. Effect summaries used to mark every function with a typed parameter may_throw (callable_boundary_effect). main's test `test_function_effects_keep_declared_int_return_verification` wants `nativeInt(int $x): int` and `relay` PURE. Functions now carry `ParamGuards` (return_boundaries.rs) and the call site (`function_call_effect`) adds may_throw only when an argument's source type is not provably the declared one (literals, stable parameters of a declared-int function); spread/named args, class/opaque types and unknown arguments stay may_throw. `mixed` parameters never count. Methods keep the blanket rule. Calls that do not go through `function_call_effect` (callable values) see the summary without the guard.
