---
title: Preserve enum trait constant declaration scope and catchable access semantics
author: Guillaumeloulier
date: 2026-10-20
branch: fix/1317-enum-trait-constant
touches:
  - docs/php/classes.md
  - src/ir_lower/expr/scoped_values.rs
  - src/magic_constants.rs
  - src/magic_constants/file_pass.rs
  - src/magic_constants/scope_pass.rs
  - src/magic_constants/trait_binding.rs
  - src/magic_constants/walker/members.rs
  - src/magic_constants/walker/mod.rs
  - src/optimize/effects.rs
  - src/types/checker/inference/expr/class_refs.rs
---

## Decided
PR1827 restores lexical enum constant normalization and direct/nested case collisions. Magic-constant passes opt into initializer traversal while type-only walkers retain their prior behavior. Lower trait/file/namespace magic constants before flattening, then replace only the trait class placeholder for each consuming enum. Reuse the existing recursive lexical-default normalizer so self is bound and missing parent remains a catchable Error until evaluation.

## Tried and ruled out
The current remote d21 had lost the earlier8bd scope/collision correction although its PR description still claimed it. Preserve both SHAs and reproduce current source. Binding with the eager class-constant normalizer would reject unused parent expressions. Returning Never for an inaccessible read made a free function fail EIR NeverFunctionReturns; reuse the existing catchable access-error helper and its conservative boxed result instead. Keeping scoped reads pure let DCE remove visible exceptions. Visibility must be checked before inferring a private initializer, and a public initializer must evaluate its private references in the declaring enum scope.

## Watch out
Keep enum constant visibility policy shared by inference and EIR; protected/private enum constants permit only the enum lexical scope because enums are final. All strict scalar/static materialization cases require zero leaks. Instance/closure/Reflection output controls remain separate: an enum-case receiver retains the identical40bytes on exact baseline and candidate, even without a closure. Preserve that witness rather than misattributing it or relaxing unrelated assertions. Final local evidence:12 standard tests per IR mode,32 strict CLI argv cases,50 target assembly pairs,102 clean native macOS/Linux executions and20 diagnostic target checks. iOS is assembly-only, with explicit function bodies; the runtime feature probe must lower magic constants too. Final publication needs current-head CI and the independent TLS advisory dependency refresh.
