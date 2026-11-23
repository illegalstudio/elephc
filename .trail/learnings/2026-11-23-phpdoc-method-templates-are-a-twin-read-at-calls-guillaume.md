---
title: "PHPDoc method templates are a twin read at each call, not a monomorphized copy"
author: Guillaume
date: 2026-11-23
branch: bead-docmethods
touches:
  - src/docblock/members.rs
  - src/docblock.rs
  - src/generics.rs
  - src/generics/methods.rs
  - src/generics/erasure.rs
  - src/generics/stats.rs
  - src/types/checker/generic_method.rs
  - src/types/checker/inference/objects/methods.rs
  - src/ir_lower/gradual_coercions.rs
  - src/ir_lower/expr/mod.rs
---

## Decided
- A method whose type parameters all come from PHPDoc (`@template` in its own docblock) is no longer adopted destructively. `members::apply` runs the old adoption on a copy and keeps the result as a hidden TWIN method `<name>__elephc_doc_template` (same suffix as the function twin), appended to the class. The written method is untouched (one body, native types, written name), so reflection, `get_class_methods`, backtraces and `method_exists` see only the written name. `methods::strip_templates` drops the twin before lowering, exactly as it drops native method templates. The twin rides the name resolver, so annotation types resolve in the file's namespace and imports for free; a process-wide side table would not.
- The checker reads the twin at each plain `MethodCall` / `StaticMethodCall` (`apply_doc_method_overlay`, called from `infer_method_call_on_class_type` and `infer_static_method_call_type`): it binds `T` with `infer_bindings_with_args`, settles native hints, checks bounds (a violated PHPDoc bound still only warns), substitutes `@return`, and types the call by it. No instantiation, no extra check round (lib test `phpdoc_method_templates_cost_one_check`: exactly 1 check).
- The method's native result has the native storage, so the call is only narrowed where that is sound: a scalar or a declared class, from a native `mixed` (or a class from an object type that accepts it). The site is recorded in `CheckResult::doc_return_sites` (keyed like `generic_call_sites`) and `ir_lower::gradual_coercions::narrow_doc_call_result` boxes the result if needed and converts (scalar) or guards (class, `MixedUnbox` with a class name) it. Containers (`array<T>`), unions and callables keep the native type: their storage is not what the native body builds.
- `@param class-string<T> $c` is read for the twin only (`DocBlock::class_strings`, spelled `class-string<T>` as a twin parameter type name): a `Foo::class` argument binds `T = Foo`. The type grammar has no `class-string`, so the annotation was ignored everywhere before; it stays ignored on functions and native-generic classes.
- Kept destructive (unchanged): constructors, methods of a class with NATIVE type parameters (`class Box<T>`; the class copies carry them), and bodiless interface/abstract methods (never adopted). Native `<T>` methods and functions are still monomorphized.
- A written `$o->id<int>(7)` on a twin renames the call to the written method and checks its arguments against the real signature, then types and narrows it from the written arguments.
- `ELEPHC_GENERICS_STATS` counts method instantiations in `instances=` and adds `doc_method_instances=` (it was 0 for methods because only class instantiations were recorded).

## Tried and ruled out
- A global table keyed by class and method: tests share one process (and the autoload pass parses on worker threads), so a thread-local is wrong and a global collides across tests; and the annotation types would reach the checker unresolved.
- Typing the call `Foo` without a guard: a class-typed receiver dispatches through that class's tables, so a lying annotation would call the wrong method.

## Watch out
- The returned value is narrowed, not trusted: a scalar lie is converted (`@return int` over a body returning "7" gives 7), a class lie raises a `TypeError`. php returns the body's value untouched. When the body's own inferred return type is already precise (not `mixed`) the native type wins and no narrowing happens.
- A class that is never referenced except through `Foo::class` is pruned from the class table, so `get(Foo::class)` stays native there; instantiate the class somewhere in a probe.
- The single body takes boxed `mixed` for an untyped parameter that two call sites feed with different types: a benchmark that used to specialize `id<int>` now boxes (see the bead comment for the measurement). That is the price of "one body".
- PHPDoc FUNCTION templates still instantiate: the twin function is a real template spliced per call. Same change, but needs the function-call lowering hook too; the Demo showed functions=0, so it was not done.
- A `continue` inside a block moved into a function became `return` through a textual replace; check moved loops by reading them.
- Demo `--check` (release, LTO off): checks 3 -> 1, `rounds=0`, instructions 2.42T -> 0.96T, check phase 1536 s -> 469 s wall under load (128 s user). 65 -> 63 warnings: the two `TypeFactoryTrait::intersection` PHPDoc-bound warnings no longer appear (the call is read without erroring), the `TableEditor::renameColumnInConstraints` ones stay, named by the written method.
