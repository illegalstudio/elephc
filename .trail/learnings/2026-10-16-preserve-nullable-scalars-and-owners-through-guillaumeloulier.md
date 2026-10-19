---
title: Preserve nullable scalars and owners through lazy value merges
author: Guillaumeloulier
date: 2026-10-16
branch: fix/1835-nullable-scalar-coalesce
cost: { minutes: 0, tokens: 0 }
touches:
  - src/ir_lower/expr/lazy_branches.rs
  - src/ir_lower/expr/merge_temps.rs
  - src/ir_lower/expr/property_access.rs
  - src/ir_lower/expr/assignments.rs
  - src/ir_lower/stmt/return_coercions.rs
  - src/ir_lower/context.rs
  - src/types/checker/inference/syntactic.rs
  - src/types/return_alias.rs
  - src/codegen/lower_inst/predicates.rs
  - src/opcache_prelude/detect.rs
  - src/optimize/effect_analysis.rs
  - src/optimize/exception_flow.rs
  - tests/codegen/types/nullable_scalar_coalesce.rs
  - tests/codegen/optimizer/nullable_return_coalesce.rs
---

## Decided

Preserve logical PHP nullability until lazy merge storage is selected. `??` removes null only from its kept value arm; a null/nullable fallback and the falsy branch of `?:` remain part of the result. Nullable integer truthiness must use the existing payload/tag helper on every target.

A null fallback is not the neutral `Never` type in checker/syntactic coalesce inference. Use the dedicated non-coercing merge so inferred returns can carry null while declared scalar returns keep their runtime boundary.

Initialized-aware instance/static property probes use owned merge temporaries, join initialization state and clear the backing slot when transferring the result. A `??=` write borrows its result slot in EIR ownership metadata as well as lowering cleanup. Inferred array element storage must widen before the probe/write when the fallback needs another representation.

A lazy merge can retain or detach its own owner even when its payload comes from a parameter. Keep possible argument aliasing conservative and let the PHP-call ownership marker decide result cleanup instead of proving a borrowed passthrough from that payload alias.

Scope nullable scalar return guards and their throwing effects with the same exhaustive syntactic detector for lazy value merges. Keep unused calls and matching TypeError handlers, while preserving ordinary pure typed method summaries. Update every exhaustive SymbolKind consumer when adding a syntactic query.

## Tried and ruled out

A normalized nullable checker union rejected valid explicitly typed scalar returns over non-null parameters. Dropping the null fallback lost inferred results instead. The dedicated Mixed merge plus the declared return boundary preserves both behaviors.

Changing cleanup decisions without marking the EIR write load borrowed still let an instance property adopt the expression consumer's reference. Clearing probe slots alone fixed static properties but did not fix this independent ownership transfer.

Marking every declared scalar return as throwing made existing pure getters/forwarders impure. The shared lazy-merge query keeps the effect model bounded to the changed path.

## Watch out

Compare complete PHP output as well as heap diagnostics: corrupted values previously had a clean final heap. Cover null, falsy values, integer endpoints, lazy fallbacks, repeated strings and both IR modes. Keep iOS compilation/assembly distinct from executable macOS/Linux evidence.

Trail check can lexically match TypeError/ownership and conservative generic-control wording from earlier learnings. Those rules still apply: full values and independent owners are tested, and the existing generic folding controls remain intact.
