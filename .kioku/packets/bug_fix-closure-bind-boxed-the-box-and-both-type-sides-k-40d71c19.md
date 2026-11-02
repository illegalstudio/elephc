---
id: bug_fix-closure-bind-boxed-the-box-and-both-type-sides-k-40d71c19
type: bug_fix
title: "Closure::bind boxed the box, and both type sides kept the enclosing class"
description: "A gradual receiver reached __rt_closure_bind already boxed so it boxed it again and the closure read a Mixed cell as an object header, printing 0 where PHP prints 7; separately the checker and lowering both typed a bound $this as the enclosing class unless the body was exactly return $this->prop"
created: 2026-09-17
verified_by: "A fixture covering all three receivers PHP's ?object allows — named class, gradual object, gradual null — plus a writing body, byte-identical to php -n; error_tests at its 99-failure baseline with identical names"
sources:
  - path: src/codegen/lower_inst/builtins/count_empty.rs
    blob: e89695b247562bc92b07d27f9179244cfec9729f
  - path: src/ir_lower/expr/static_method_calls.rs
    blob: 86af0e35cf5b064c4018d4629e6a6f2802df4b7b
  - path: src/types/checker/inference/objects/methods.rs
    blob: 41165febac884fb04decd20c4bc0e74b01afe31e
supersedes: "gotcha-the-runtime-closure-bind-does-not-write-the-new--82153f10"
---

# Closure::bind boxed the box, and both type sides kept the enclosing class

## Fact

Three defects behind one Symfony line,

    \Closure::bind(function () { $this->options['exceptions'] = false; }, $options, $options)();

in `RedisTrait`, where `$options` is `clone $redis->getOptions()` over Redis classes the build does
not have.

1. **The receiver was double-boxed.** `__rt_closure_bind` takes the new receiver as a RAW object
   pointer — it stores it into the descriptor's capture slot and boxes it there itself when the
   capture is a Mixed one. A gradual receiver arrives ALREADY boxed, and `lower_closure_bind`
   passed it straight through. Measured with nothing else changed:

       Closure::bind(fn () => $this->n, $known)   -> 7
       Closure::bind(fn () => $this->n, $gradual) -> Warning: Undefined property: ::$n
                                                     0            (php -n prints 7)

   The EMPTY class name is the double box being read as an object header. A wrong ANSWER, not a
   refusal. `lower_closure_bind` now unboxes and dispatches on the tag: object binds, null unbinds
   (an unbound closure is what the source descriptor already is, which is what the literal-`null`
   path returns), anything else is PHP's TypeError with the runtime type named.

2. **The checker typed a bound `$this` as the ENCLOSING class**, unless the body was exactly
   `return $this->prop;`.

3. **Lowering had the same narrow restriction**, from `build_bound_closure_binding`.

2 and 3 MUST move together. Widening only the checker was tried first and backed out: it types
`$this->p` against one class while the backend emits it against another, which is a silent
miscompile rather than a diagnostic — and widening only the backend fails the same way in the
other direction, which is exactly what happened on the first re-land (the known-class case then
failed with `receiver PHP type Object("User")`). `bound_this_capture`
(src/ir_lower/expr/static_method_calls.rs) and `closure_bind_property_receiver_type`
(src/types/checker/inference/objects/methods.rs) now answer the same two ways — precise class, or
gradual — from the same two arguments, and each doc comment points at the other.

## Why

The earlier packet blamed the runtime helper; the helper was right and its caller was not
