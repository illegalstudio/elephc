---
title: Concrete by-ref property slots stay unboxed; only object/mixed need ref cells
author: Guillaume
date: 2026-10-28
branch: feat/gmp-extension
cost: { minutes: 60, tokens: 597558 }
touches:
  - src/ir_lower/expr/ref_place_args.rs
  - src/ir_lower/expr/function_calls.rs
  - src/ir_lower/expr/static_method_calls.rs
  - src/types/param_binding.rs
  - src/types/checker/functions/resolution/call.rs
  - src/types/checker/functions/resolution/resolved.rs
---

Generalizing the property by-reference rewrite exposed a storage split:

1. Object and `mixed` property temps cross the user-call ABI as boxed Mixed ref cells:
   promote with `promote_local_mixed_ref_cell`, then unbox objects on write-back.
2. Concrete scalar/string/array temps must NOT be promoted. The backend passes a concrete
   by-value operand by address; promoting turns the int/string slot into a Mixed cell and the
   caller's `load_ref_cell` + cast reads the cell pointer (an int property came back as 0).
   Write these back directly from the typed temp.
3. PHP's declared `array` type is a `Union([Array, AssocArray])`. Do not reject it as a union:
   `Ty::is_php_array()` is the supported exception. Inspect the DECLARED type in the checker
   (a union collapses to `Mixed` under `codegen_repr`, which would wrongly accept nullable and
   arbitrary unions).
4. Free functions go through two checker paths (`resolution/resolved.rs` for ordinary
   declarations and `resolution/call.rs` for generic templates). `call.rs` had to resolve the
   declared parameter type before the lvalue check so the same compatibility helper can run.
5. The argument `TypeError` prefix is only available during argument lowering. Each call path
   sets `LoweringContext::call_description` (`Class::method`, `Class::__construct`, or the
   function name) around `lower_args_with_signature`, restores it afterwards, and the guard
   appends `called in <source_path> on line N`.

Known remaining gap: the write-back runs after a normal return, so a callee that mutates the
reference and then throws leaves the property unchanged (elephc-n6j).
