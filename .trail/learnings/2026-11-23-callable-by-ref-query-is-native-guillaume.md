---
title: Callable by-ref query is native in a closed world
author: Guillaume
date: 2026-11-23
branch: bead-byrefquery
touches:
  - src/ir_lower/expr/descriptor_native_selections.rs
  - src/codegen/lower_inst/callables.rs
  - src/codegen_support/runtime/callables/param_is_by_ref.rs
---

CallableParameterIsByRef used to declare an eval context local (linking the interpreter, rerunning the closed-world build with eval). Now: lowering declares no eval context when crate::closed_world::is_enabled() or the owner is a compiler helper (__elephc_*), and codegen (lower_callable_parameter_is_by_ref) answers natively when the module has no eval_bridge: it resolves the boxed callable with emit_runtime_mixed_callable_descriptor_value_impl (same scope and errors as the call, DirectCall miss), then calls the shared __rt_callable_param_is_by_ref over the descriptor's signature record (visible count, variadic index, name table, ref flag table; names select by name else variadic, positions clamp to the variadic). Pitfalls found: a statically known callee with by-ref array element args is refused by the checker (validate_static_cufa_reference_arguments), so test fixtures must reach the callee through an array/foreach; by-ref array elements and properties through descriptors are by-value at run time (only SimpleXML selections observe the query: a by-ref callee materializes a missing node); a direct call of a literal [$obj,'undefinedMethod'] leaks in closed and interpreter builds alike; ['C','undefinedStatic'] via __callStatic aborts 'callable array did not resolve'; a by-ref variadic foreach-by-ref body crashes in _rt_hash_free_deep. Test: tests/codegen/simplexml/reference_arguments.rs test_callable_parameter_query_answers_every_callable_form_natively (php -n parity, heap clean, no bridge symbol); unit: ir_lower::tests::simplexml_reference_arguments closed_world_passing_mode_query_declares_no_eval_context.
