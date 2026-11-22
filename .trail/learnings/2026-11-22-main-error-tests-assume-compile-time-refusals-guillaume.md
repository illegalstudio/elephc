---
title: "Main's error tests assume compile-time refusals the branch defers to run time"
author: Guillaume
date: 2026-11-22
branch: bead-errtests
touches:
  - tests/error_tests/nullable_fallback_flow.rs
  - src/types/checker/functions/param_binding.rs
  - src/types/traits.rs
  - src/magic_constants/walker/exprs.rs
  - src/types/checker/type_compat/object_types.rs
---

## Decided
- A nullable local (`?int`, `Base` vs an interface) at a declared by-value parameter compiles in the default build and the call throws php's `TypeError`; `--strict-locals` refuses it through `strict_locals_static_param_contradiction`. A nullable CLASS at a class parameter stays accepted in both modes (php's ObjectGuard binding). Tests that main wrote as `expect_error` become `expect_error_strict`, or `expect_php_accepts` when strict mode cannot see the flow either (by-reference nulls, `$ints[$key]` misses, alias/global rebinds).
- A trait's abstract method that the parent class already implements is NOT dropped at flatten time any more: `inherited_method_satisfies_requirement` (schema) validates the inherited signature, so php's "Declaration of P::foo() must be compatible with T::foo()" fatal is kept. The old `drop_abstract_methods_implemented_by_ancestor` silently accepted any signature.
- `absent_class_defers_to_runtime` is false for an enum and for a class the schema has declared but not built yet: an unrelated return type in an override is provably incompatible, not "absent from the closed world".
- `magic_constants::walker::walk_expr` walks a left-deep `BinaryOp` chain with a loop: 400 operands of `&&` overflowed the 2 MB test-thread stack through the generic walker (the same frame is the goto-lowering walker).

## Watch out
- `type_system::test_nested_spread_literal_merges_type_in_linear_time` still overflows the default 2 MB stack in a debug test build (before and after this work); it passes with `RUST_MIN_STACK=67108864`.
- A list bound by reference to `array<K, V> &` still crashes (segfault / heap exhaustion) for a global, a `mixed &` earlier slot of the same call and an int list at `array<int, float> &`: the caller's local is retyped to the parameter's layout before `require_boxed_by_ref_storage` runs, so its refusal never fires. Repros: `put($g)` with `global $g`, `two($ids, $ids)` with `mixed &$a, array<int,int> &$b`, `put($ids)` with `array<int,float> &$m`.
- The `Mixed`-vs-`Int` return assertions of `r41`..`r44` fail because the branch does not prune the fall-through of `if ($value !== null) { continue/return }` once `$value` is a proven non-null int: `$result = 2.5` stays reachable, so the return type is `int|float`.
