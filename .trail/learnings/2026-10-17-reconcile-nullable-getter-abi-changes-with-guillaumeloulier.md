---
title: Reconcile nullable getter ABI changes with Trail prose matches
author: Guillaumeloulier
date: 2026-10-17
branch: fix/1836-nullable-array-merges
cost: { minutes: 3399, tokens: 21525978 }
touches:
  - docs/internals/the-ir.md
  - src/ir_lower/expr/array_access.rs
  - tests/codegen/runtime_gc/nullable_array_read_owners.rs
  - tests/codegen/types/nullable_array_merge.rs
  - ".plans/issue-1836-nullable-array-merges.md"
  - src/codegen/lower_inst/arrays.rs
  - src/codegen/lower_inst/hashes.rs
  - src/ir_lower/expr/array_merge_values.rs
  - src/ir_lower/expr/assoc_array_literals.rs
  - src/ir_lower/expr/dynamic_spreads.rs
---

## Decided

Keep the #1859 canonical-null correction after reviewing all twenty Trail lexical conflicts. Every match is in plan or internals prose, not a new executable contradiction. The null arm releases any original owner, creates a canonical boxed-null receiver and pins that cell; it never forwards the raw nullable source to the dynamic getter. Merge-bound scalar reads still request a boxed getter result before conversion. Native unpack source classification is shared across all three binder selection paths.

## Tried and ruled out

The check matches descriptions of already rejected scalar post-conversion boxing, original-null pinning, incomplete binder fixes and broad Synthetic flattening against prose describing their replacement. Those approaches remain rejected. A mention of iOS evidence is not executable top-level coverage: target fixtures are function-wrapped for library builds and native-only originals are excluded from the iOS assembly set. No prior learning is changed to silence the check.

## Watch out

Document the twenty false matches in the amended commit/PR description and retain the known main-only leak controls separately. The upstream runtime_gc.rs overlaps are module registrations; preserve unrelated registrations during rebases. Validate the exact final linked CLI after the last implementation edit, including the review's null-coalesce example, the existing undefined-root CI test, throwing keys and throwing warning handlers in both IR modes.
