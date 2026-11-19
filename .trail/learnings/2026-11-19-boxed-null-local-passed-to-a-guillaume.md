---
title: Boxed null local passed to a ?ptr parameter
author: Guillaume
date: 2026-11-19
branch: bead-ptr-null
touches:
  - src/ir_lower/gradual_coercions.rs
---

## Decided
A by-value Mixed argument bound to a `?ptr<T>` parameter is narrowed with an integer cast in `coerce_gradual_value_to_boundary` (pointer words cross a boxed local as plain ints, null is 0).

## Tried and ruled out
The mixed-storage scan (`disqualify_named_method_arguments`) and `is_final_mixed_local` were fine: `$p = null` widens the slot to Mixed on purpose (store_local_impl, Void store). Static calls only looked right because the null load was folded to const_null.

## Watch out
23 nullable-pointer codegen tests (nullable_pointers, packed_*, through_methods) fail on merge-main independently: by-ref `?ptr` binding of a null local expects pointer-word storage but the null store boxes the slot to Mixed.
