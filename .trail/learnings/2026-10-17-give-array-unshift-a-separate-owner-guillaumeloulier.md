---
title: Give array_unshift a separate owner for consuming reference-cell COW
author: Guillaumeloulier
date: 2026-10-17
branch: fix/unshift-reference-cow
touches:
  - src/codegen/lower_inst/builtins/arrays/unshift.rs
  - tests/codegen/runtime_gc.rs
  - tests/codegen/runtime_gc/unshift_reference_cow.rs
---

## Decided
Before packed array_unshift consumes a reference-cell receiver through array ensure-unique, acquire a separate helper owner with ReceiverPlace::prepare_consuming_storeback. Its COW split retires that owner; reference-cell publication retires the independently stored old owner. Keep raw local and global publication conventions unchanged.

## Tried and ruled out
The failure exposed while capturing a property receiver looked like a clone defect. LLDB showed singleton-cell cloning correctly incrementing its array from 1 to 2. The subsequent unshift split plus store-back decremented the old array twice. A smaller main reproducer needs no property or clone: bind a local reference, copy the array by value, prepend through the reference, then read the copy. It exits with heap-debug bad refcount before this fix. Two focused native regressions pass with EIR optimization on and off after the fix, covering retained copies, unique references, growth, multiple values, and no-value calls.

## Watch out
A clean result with no value copy does not exercise consuming COW. Keep the surviving copy readable after unsetting the original reference and array, and assert the complete output plus clean heap. This is an independent existing-main bug exposed by issue1815, so publish its small correction separately before integrating the property-reference work. Owned reference-cell clone code was not changed.
