---
title: Guard literal callable arguments before coercion and preserve null source owners
author: Guillaumeloulier
date: 2026-10-16
branch: review/1822-finalize
cost: { minutes: 0, tokens: 0 }
touches:
  - src/codegen/runtime_callable_invoker.rs
  - src/codegen/runtime_callable_invoker/owned_value_args.rs
  - src/codegen/lower_inst/literal_parameter_guards.rs
  - src/codegen/lower_inst/callables.rs
  - src/codegen/lower_inst/callable_descriptors.rs
  - tests/codegen/generics_review.rs
---

## Decided

Validate declared false/null value arguments while descriptor invokers still hold the original boxed values. Both indexed and named containers use the existing binder, then the same guard before ABI coercion. Read the diagnostic name from the descriptor because invoker bodies can share signatures. Preserve the actual receiver/called-class lookup key separately from the declaring method's diagnostic name.

## Tried and ruled out

Checking only the legacy direct array-call emitter misses the descriptor-invoker path. A correct PHP-visible TypeError is also insufficient ownership evidence: fresh reference-cell boxes leaked when Void/null was not considered a conversion target. Releasing that source must account for null's zero-slot result; the ordinary preserve-result helper assumes a pushed result slot.

## Watch out

x86_64 returns strings in rax/rdx and newly allocated objects in rax. Pop a persisted error message into separate ABI scratch registers before filling the new TypeError, rather than overwriting the object pointer. Assembly success did not reveal that collision; actual Linux execution did. Final controls cover named binding, earlier converted string cleanup, inherited array/first-class methods, wider overrides, cached invoker names, and scalar arrays. All work and probe artifacts stay inside Elephc. Keep intermediate failures separate from final passing evidence.
