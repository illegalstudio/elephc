---
id: gotcha-twig-compile-blockers-24-down-to-8-with-the-two--73dec47c
type: gotcha
title: "Twig compile blockers: 24 down to 8, with the two relaxations that had to be reverted"
description: "The current list behind index_preload_twig.php, what cleared each one, and the two that looked like fixes and were miscompiles"
created: 2026-09-18
verified_by: "elephc --check index_preload_twig.php, 29s per iteration, 24 -> 15 -> 13 -> 12 -> 11 -> 9 -> 8; error_tests 1431/99, codegen::regressions 409/6, codegen::types 343/16, spl::autoload 104/2, type_builtins 134/6, codegen::arrays 665/27, all with the same failure names as before the session"
sources:
  - path: examples/symfony-app/public/index_preload_hotpath.php
    blob: e8b652d89f5a0ff3c110d05122a91cc8517e574b
  - path: src/builtins/string/strtr.rs
    blob: e6d388ac74ba8ac86abd4a00189a82d80b56ab42
  - path: src/types/checker/stmt_check/narrowing.rs
    blob: 725d6a847cef1215bbe7fa721a0dd1ae31d2cf9c
  - path: src/builtins/array/shuffle.rs
    blob: 37d98c759875782381e3e9fe10a8084c73441dae
supersedes: "gotcha-what-still-stops-twig-compiling-and-why-the-symf-41524d9f"
---

# Twig compile blockers: 24 down to 8, with the two relaxations that had to be reverted

## Fact

THE LOOP. `elephc --check <entry>.php` type-checks the whole closed world in 29 SECONDS where a
`--web` build takes 13 minutes. Build only when the count reaches zero.

CLEARED, and how:
 - EIGHT `Undefined method: ExtensionInterface::…` sites: NOT a compiler change. `SandboxExtension`
   was missing from the closed world because its only mention is `SandboxExtension::class`, which
   PHP never autoloads. One `require_once` of `twig/twig/src/Extension/SandboxExtension.php` in the
   preload list gives `subtype_dispatch_return_types` a candidate and the existing lenient path
   takes over. DO NOT relax "undefined method on an interface receiver" instead -- three
   `error_tests::narrowing` tests pin that diagnostic on purpose.
 - `iterator_to_array()` with a gradual `preserve_keys`: the backend already lowered `Mixed`
   through `__rt_mixed_cast_bool`; only `preserve_keys_type_supported` refused it. Its dynamic
   return is now `Mixed`, NOT the union of both shapes -- a union of `Array` and `AssocArray`
   stored in a local types as `iterable`, and every array builtin refuses `iterable`.
 - `array_reverse()` with a runtime flag, for INDEXED sources: both arms emitted, each boxed,
   result type the union. A hash's `false` arm needs a renumbering helper that does not exist.
 - TWO `throw` sites: `goto` is desugared by CLONING the label's tail at the jump, so the original
   tail keeps the variable at its `= null`. `if ($e) { throw $e; }` there is dead, and the truthy
   guard now narrows a null-only local to `Never`, which `throw` accepts.
 - `strtr($s, $gradualMap)`: this one was a SILENT MISCOMPILE, not a refusal. The lowering fell
   through to the THREE-argument byte-translation form for any non-static-array map; that form
   reads a `$to` that is not there and returns the subject untouched. `strtr('abc', (array) $map)`
   answered `abc`. Two arguments can no longer reach the pairwise lowering, and a gradual map
   dispatches on the runtime tag.
 - `shuffle()`: it demanded a concrete array where `sort()` and the rest of the by-reference family
   use `array_arg_is_gradually_acceptable`.

REVERTED -- both looked right and both are miscompiles:
 - "An undefined method on an INTERFACE receiver defers to run time": clears 9 errors, breaks
   `narrowing::test_this_instanceof_rejects_method_absent_from_interface` and two siblings.
 - "`iterable` minus `Traversable` is `array`": clears the SandboxExtension error and SEGFAULTS.
   The narrowing is type-level only; the runtime representation of an `iterable` local is not a
   dense array, so re-typing it to `Array(Mixed)` and passing it to an `array` parameter hands the
   callee something it walks as fixed-size slots. A sound version has to CONVERT, not re-type.

STILL BLOCKING, 8 sites:
 - `SandboxExtension.php:165` -- the iterable/array one above.
 - `IncludeNode.php:50` -- 2 arguments to a method declared with 1 (the second is commented out for
   BC and the subclass declares it). PHP allows extra arguments to a userland function. WARNING: a
   probe showed elephc DROPS them when it does accept -- `Child::build($label, $extra)` reached
   through a `Base::build($label)` site printed `base/` where php prints `base/extra`.
 - `array_rand($values, 1)`, `strip_tags($s, $allowed)`, `array_column($a, $name, $index)` -- the
   contract declares fewer parameters than PHP. Each needs the parameter, a compiled lowering and
   an interpreter binding.
 - `array_slice`, `array_chunk`, and `array_reverse` over a hash -- non-literal `preserve_keys`.
   `array_reverse` shows the pattern to copy; the hash arm needs a renumbering runtime helper.

## Why

It is the remaining distance between the compiled Symfony app and a sub-10ms Twig route, and two of the obvious shortcuts crash
