---
title: PHP constants separate definition and fetch identity
author: Guillaumeloulier
date: 2026-10-14
cost: { minutes: 0, tokens: 0 }
touches:
  - crates/elephc-builtin-contract/src/constant_spec.rs
  - crates/elephc-magician/src/context/functions.rs
  - src/codegen_support/prescan.rs
  - src/ir_lower/context.rs
  - src/names.rs
  - src/builtins/system/constant.rs
  - src/builtins/system/define.rs
---

## Decided
PHP 8.5.10 registers constant names with every literal leading separator intact and folds ASCII case only in the namespace. Fetches remove exactly one root separator before the same folding. Share constant_definition_key and constant_lookup_key across AOT and Magician. Canonicalize private type/lowering maps, but preserve first declaration spelling in public inventories. Keep a set of registration identities while collecting constants so alias redefinitions cannot replace the first value or make collection quadratic.

## Tried and ruled out
Do not strip a root separator in a define guard: PHP const FOO=4 followed by define('\FOO',5) returns true and creates a distinct key. Do not lowercase the basename, strip all separators, or expose folded keys through get_defined_constants(). Updating backend flags alone misses checker and prescan lookups.

## Watch out
Verify the registration/fetch distinction with PHP and php-src tag php-8.5.10, Zend/zend_constants.c (zend_register_constant versus zend_get_constant_ex). Native eval identity output is covered separately from retained-cell cleanup; an ordinary unprefixed fresh eval definition also retains a 40-byte cell in the independent ownership path. No time or token cost attribution was measured for this learning; zero cost denotes unmeasured work.
