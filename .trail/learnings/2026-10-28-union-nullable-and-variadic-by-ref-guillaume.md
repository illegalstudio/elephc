---
title: "Union, nullable, and variadic by-ref property places: declared types and boxed variadic cells"
author: Guillaume
date: 2026-10-28
branch: feat/gmp-extension
cost: { minutes: 45, tokens: 424136 }
touches:
  - src/ir_lower/expr/ref_place_args.rs
  - src/types/param_binding.rs
  - src/types/checker/functions/call_validation.rs
  - tests/codegen/references.rs
  - tests/error_tests/type_system.rs
---

Extending property by-reference places past objects exposed two traps:

1. Support checks must inspect the DECLARED type. `PhpType::codegen_repr()` collapses every
   union to `Mixed`, so a `codegen_repr`-based allowlist silently accepts nullable and
   arbitrary-union properties. The supported list now matches raw `PhpType` variants, with
   `is_php_array()` as the explicit union exception for PHP's `array` contract. The
   compatibility match then uses codegen reps for scalar identity and adds
   `(TaggedScalar, TaggedScalar)` for nullable ints.

2. A by-reference variadic collector transports reference markers in a boxed array. Fixed
   scalar/string array temps stay unboxed and are passed by address, but variadic property
   temps must be declared `Mixed`, `mixed_box`ed, and promoted before the call; the write-back
   then narrows the boxed cell back to the declared property type. Without the box, a variadic
   array append corrupted the ref cell and the property store reported "Cannot assign object
   to property ... of type array". Array properties are still rejected in variadic position
   because the collector needs a container repack; scalar/string/mixed/object/union/nullable
   and static property places work.

Named arguments in a variadic tail still skip the property rewrite. Exceptional-path write-back
is tracked by elephc-n6j.
