---
title: Enum constants bind self to the consuming enum
author: Guillaumeloulier
date: 2026-10-14
cost: { minutes: 0, tokens: 0 }
touches:
  - src/types/checker/schema/classes/constants.rs
  - src/types/checker/schema/enums.rs
  - src/types/traits.rs
  - src/types/traits/validation.rs
  - tests/codegen/oop/traits.rs
  - tests/error_tests/enum_trait_constants.rs
---

## Decided
Normalize enum constant value expressions with the existing recursive class-constant lexical rewriter, using the consuming enum name and no parent. This keeps imported self::A and self::class independent of the caller and binds a shared nested trait separately for each enum. Validate case/constant collisions after trait expansion and before enum metadata is built; inspect each directly used expanded trait so nested conflicts name the consuming trait.

## Tried and ruled out
Leaving parsed self receivers in metadata made global/function reads fail and a caller C with A=99 read 100:C instead of PHP 8.5.10's 2:E. A constant-order loop that merely skipped existing case names still accepted invalid declarations. Member matching is case-sensitive, so constant a and case A coexist.

## Watch out
Own enum constant/case collisions use Cannot redefine class constant; imported ones use Cannot use trait T, because T::A conflicts with enum case E::A. Reflection output is checked independently of its unchanged context-retention path, reproduced on the reviewed baseline. Cost attribution is unmeasured; zero cost is a placeholder, not a benchmark.
