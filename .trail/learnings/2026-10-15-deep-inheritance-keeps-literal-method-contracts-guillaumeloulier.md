---
title: Deep inheritance keeps literal method contracts without ordinary call scans
author: Guillaumeloulier
date: 2026-10-15
branch: review/1822-finalize
cost: { minutes: 0, tokens: 0 }
touches:
  - src/codegen/literal_method_index.rs
  - src/codegen/lower_inst/method_dispatch.rs
  - src/codegen/lower_inst/method_intrinsics.rs
  - src/codegen/lower_inst/static_method_calls.rs
  - src/codegen/shared_state.rs
  - tests/codegen/generics_review.rs
---

## Decided
Virtual instance, nullable and late-static calls must check the selected implementation's declared literal contract before ABI coercion. Traverse the complete ancestor chain with a visited set: a fixed 64-link budget silently omitted valid deep subclasses. Keep declaring-class error names, widening overrides and null-receiver error precedence.

Use a module-local parameter-only index keyed by method, staticness and argument count. Cache empty lookups as well as candidates, reject ordinary signatures before entering the index, and sort receiver candidates by class id. Guard discovery needs declared signatures, not emitted-body resolution; mixed dispatch itself still resolves its actual call targets separately.

## Tried and ruled out
The reviewed compiler accepted true through an inherited false method at 70 levels; PHP threw TypeError. A clean heap alone did not expose that semantic bypass. Checking the static base signature unconditionally would reject valid widened overrides.

## Watch out
Ordinary standalone null method parameters are rejected by the current checker even on the reviewed baseline. Use the supported promoted-null constructor shape to exercise deep null dispatch rather than claiming that unrelated declaration gap is fixed. Test instance, nullable and late-static paths, including depths 64 and 70, with both IR optimization modes. A storage-full error poisoned one initial in-process runtime cache; retain that failed run separately and restart the process before interpreting later failures.

Time and token costs were not measured; zero is supplied to disable automatic historical attribution.
