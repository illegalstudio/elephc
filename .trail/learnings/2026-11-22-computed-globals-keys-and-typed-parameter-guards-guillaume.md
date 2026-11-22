---
title: Computed $GLOBALS keys, open callable ladder runtime features, mixed parameters in effect summaries
author: guillaume
date: 2026-11-22
branch: bead-opt-leftovers2
touches:
  - src/globals_dynamic.rs
  - src/globals_snapshot_prelude.rs
  - src/parser/expr/pratt.rs
  - src/ir_lower/program/runtime_features.rs
  - src/optimize/effect_analysis.rs
---

1. `$GLOBALS[$expr]` (php 8.1+: read, write, compound, ??=, ++/--, isset, unset, `[]=` below the element). The parser keeps the raw `$GLOBALS[$key]` ArrayAccess; `crate::globals_dynamic::lower` (called in pipeline.rs and the codegen/ir_lower test harnesses, before backend_gap_prelude) turns reads into elements of the `__elephc_globals_array()` snapshot, writes into `__elephc_globals_set($k,$v)`, unsets into `__elephc_globals_unset($k)`. globals_snapshot_prelude declares those helpers as a dispatch over every global name the program can spell, plus a static side table (`__elephc_globals_extra`) for names nothing spells (visible only through the snapshot). Refused loudly: `$r = &$GLOBALS[$k]` and nested writes used as an expression. A harness that builds its own pipeline must call `globals_dynamic::lower` or a computed key stays a raw `$GLOBALS` array access.
2. The open callable ladder (a callable PARAMETER, `$callback()`) gets one wrapper per builtin, whose bodies call runtime routines (`__rt_member_exists`, `__rt_core_error_handler_pop`, `__rt_object_clone_shallow_boxed`) nothing in the instruction stream mentions. `lowered_runtime_features_with` now enables class_methods_introspection/handler_state/object_clone whenever a function has an open boxed callable site (`function_has_open_callable_site`). Not done: giving callable parameters provenance across call boundaries so the ladder is smaller.
3. Effect summaries mark every function with a typed parameter may_throw (`callable_has_typed_parameters` in effect_analysis.rs): binding can raise a TypeError, and callable values (`$f = 'accept_int'; $f('x')`) see only the summary. `mixed` (and a `mixed` variadic) no longer counts, since binding it never throws. Tried and reverted: per-call-site guards (summary PURE, may_throw added at `function_call_effect` when the argument is not provably the declared type); it broke the branch test `test_program_function_effects_preserve_typed_parameter_checks` and leaves callable-value calls unsound. main's `test_function_effects_keep_declared_int_return_verification` still expects `nativeInt(int)`/`relay` PURE: open conflict, tracked in elephc-309c.
