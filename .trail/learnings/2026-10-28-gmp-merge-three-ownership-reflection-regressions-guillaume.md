---
title: "GMP merge: three ownership/reflection regressions and their real routes"
author: Guillaume
date: 2026-10-28
branch: feat/gmp-extension
cost: { minutes: 90 }
touches:
  - src/codegen/lower_inst/runtime_wrappers.rs
  - src/types/return_alias.rs
  - src/ir_lower/expr/reflection_filters.rs
  - src/codegen/lower_inst/objects/reflection/class_members.rs
  - tests/codegen/gmp_class.rs
---

Merging origin/main into the GMP branch exposed three non-obvious routes:

1. First-class-callable wrappers: widening `build_runtime_call_wrapper_function` to every
   `BuiltinRuntimeFunctions::None` + `BuiltinLowering::Eir` home sent `gc_status` through
   `lower_prepared_builtin_callable`, doubling its allocation graph (28 vs 14) and breaking the
   ownership assertion. GMP's prelude-backed homes are the ones that need prepared lowering, so
   the predicate is restricted to contracts in `PhpModule::Gmp`.

2. Method inlining vs return aliasing: adding `BinaryOp::Concat => ReturnArgAlias::None` made
   `prepublish_user_call_result` stage every concat-returning method result and root its
   arguments, so `method_effects_splice_string_building_on_every_target` no longer inlined
   `Label::wrap` (the arg became an Acquire instead of a stable local load). `Unknown` is the
   conservative alias value main relies on; revert concat to it rather than tightening alias
   analysis for a leak fix.

3. GMP's visible-method contract must be applied on EVERY reflection route. Filtering only
   `class_members::reflection_class_method_names` left `ReflectionClass::getMethods()` listing
   all 24 adapters because statically-known class-string calls lower through
   `ir_lower::expr::reflection_filters::reflection_class_method_names_for_filter`. Filter both,
   and report the contract's `php_visible_methods` order because the synthetic prelude's
   HashMap order is not php-src's declaration order.
