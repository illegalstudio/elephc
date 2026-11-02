---
id: bug_fix-the-int-inference-seed-for-an-undeclared-paramet-cae7044a
type: bug_fix
title: "The int inference SEED for an undeclared parameter reached the backend as a real int - the body said mixed, the ABI said int"
description: "method_body_param_type already answered mixed for an unrefined seed, but only in the ENVIRONMENT; the signature EIR lowers kept the seed"
created: 2026-09-18
verified_by: "ELEPHC_DEBUG_SPECIALIZE trace on the Symfony --web check printed 'body param Twig\\Extension\\CoreExtension::filter #2 seed=int seen=false' while --emit-ir printed 'array: I64 php=int' for the same parameter; after the fix both blockers on that function disappeared"
sources:
  - path: src/types/checker/method_pass.rs
    blob: f3673188aac4e205858b891e87db9a645bc23d93
  - path: src/types/checker/driver/mod.rs
    blob: a6d27037483c95873bdd97f7fc368e94c95bffbc
  - path: src/codegen/lower_inst/objects/iterator_iterator.rs
    blob: 41afd3a066f7c54771dc87c50ece66ce37059996
---

# The int inference SEED for an undeclared parameter reached the backend as a real int - the body said mixed, the ABI said int

## Fact

`initial_function_param_types` seeds an undeclared parameter with `PhpType::Int` as a gradual
starting point that the first CALL SITE replaces. For a method no compiled caller reaches, that
call site never comes, and `method_body_param_type` already knows it: it answers `mixed` when
`param_specialization_seen` has no entry for the slot.

Only the ENVIRONMENT moved. `class_info.methods[..].params[i]` kept the `int`, and that signature
is what EIR lowers the parameter as -- so the checker reasoned about a boxed value while the
backend read a raw integer register. `twig/twig`'s
`CoreExtension::filter($env, $isSandboxed, $array, $arrow)` has no compiled caller (its only one is
the deprecated `twig_array_filter` shim, which is not even emitted), and the Symfony `--web` build
stopped twice on it:

    unsupported EIR backend feature: IteratorIterator source PHP type Int
    unsupported EIR backend feature: object_new assigning PHP type Int to
        CallbackFilterIterator::$callback with PHP type Callable

THE FIX. `type_check_methods_until_stable` clears `unspecialized_seed_params` at the top of every
round and records each slot the round bound as `mixed`; the driver calls
`publish_unspecialized_method_param_seeds()` once the loop settles, which writes `mixed` into those
signature slots. Only the LAST round counts -- on an earlier round the call site that refines the
seed may simply not have been walked yet -- and only a slot still holding the seed is touched.
The free-function half of the same rule already existed:
`widen_unrefined_params_of_an_uncalled_function`.

WHAT IT DOES NOT FIX: a parameter whose type is a fabricated scalar for some OTHER reason still
reaches a constructor that needs an object. `new \IteratorIterator($scalar)` now emits php's own
TypeError instead of refusing the build, which is what php does with that declared type.

## Why

It is the shape behind 'a class without its dependencies gets a fabricated type' when the class is present and the CALLER is missing
