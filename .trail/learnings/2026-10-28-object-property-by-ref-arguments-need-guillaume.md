---
title: Object property by-ref arguments need a boxed ref cell and an unboxed write-back
author: Guillaume
date: 2026-10-28
branch: feat/gmp-extension
cost: { minutes: 75, tokens: 1372464 }
touches:
  - src/ir_lower/expr/ref_place_args.rs
  - src/ir_lower/expr/call_arg_coercion.rs
  - src/types/param_binding.rs
  - src/types/checker/functions/call_validation.rs
  - src/ir_lower/expr/method_calls.rs
  - src/ir_lower/expr/object_construction.rs
  - src/ir_lower/expr/nullable_method_calls.rs
---

PHP lets an object property be passed to a by-reference parameter of a user method or
constructor. Elephc's user-call ABI materializes every typed by-reference parameter as a
**boxed Mixed ref cell**, so the rewrite cannot reuse the builtin array temp path verbatim:

1. `rewrite_typed_property_ref_args` reads the property into a synthetic local, promotes it
   with `promote_local_mixed_ref_cell`, passes that local, then writes back after the call.
2. The write-back local is Mixed, so storing it straight into the typed property sends the box
   pointer into a raw property slot ("Cannot assign object to property Box::$value"). Unbox
   with `Op::MixedUnbox` into a second local declared with the property type, then run the
   ordinary assignment lowering on that typed local.
3. Wire the rewrite into every instance-call lowering that `check_user_declared_call`
   validates: `lower_method_call`, `lower_method_call_with_binding` (nullsafe/enum), and
   `lower_new_object`. Static calls, free functions, closures, and descriptor invokers keep
   rejecting property places, so the checker flag stays scoped to methods/constructors.
4. The checker must require BOTH the declared parameter and the property to be object-typed.
   Scalar/array places would need a typed unbox/coercion path; accepting them without it
   silently stores a Mixed pointer.
5. The write-back runs only after a normal return. A callee that mutates the reference and then
   throws leaves the property unchanged, unlike PHP; an exception-guard cleanup would be needed
   to match that path.

For `Class|false` / `Class|null` arguments, add `ParamBinding::ObjectGuard`, accept it in the
checker, and lower an `Op::InstanceOf` branch that unboxes on success and throws `TypeError`
on failure. Restrict it to the exact two-member union so wider unions stay rejected.
