---
id: gotcha-what-still-stops-twig-compiling-and-why-the-symf-41524d9f
type: gotcha
title: "What still stops Twig compiling, and why the Symfony / route cannot go under 10ms until it does"
description: "The exact blocker list behind index_preload_twig.php, measured with a 29-second --check loop, plus the two entries already cleared"
created: 2026-09-18
verified_by: "elephc --check examples/symfony-app/public/index_preload_twig.php, 29s per iteration; the count went 24 -> 15 -> 13 -> 12 as the entries below were cleared"
sources:
  - path: examples/symfony-app/public/index_preload_hotpath.php
    blob: e8b652d89f5a0ff3c110d05122a91cc8517e574b
  - path: src/builtins/array/array_reverse.rs
    blob: 259e58a3b57ee09c7a7959bffbd6664dd14a63e4
  - path: src/types/checker/builtins/spl.rs
    blob: ece6ff5072618554803e7caf38cb40fd7d9f5ecf
superseded_by: "gotcha-twig-compile-blockers-24-down-to-8-with-the-two--73dec47c"
superseded_why: "same subject, current counts and the two reverted relaxations"
---

# What still stops Twig compiling, and why the Symfony / route cannot go under 10ms until it does

## Fact

THE FEEDBACK LOOP. `elephc --check <entry>.php` type-checks the whole closed world in 29 SECONDS
where a full `--web` build takes 13 MINUTES. Every entry below was found and re-measured with
`--check`; only build once the count reaches zero.

WHY CoreExtension IS UNAVOIDABLE. The goal is to compile the Twig TEMPLATE class -- Twig writes one
per template into `var/cache/prod/twig/<xx>/<hash>.php` and the class name is derived from the
template's cache key, so requiring the cache file declares exactly the class
`Environment::loadTemplate` asks `class_exists()` for. But the generated body calls
`CoreExtension::getAttribute()` and `CoreExtension::upper()` directly, so the closed world pulls the
whole file in and every method in it must check, reachable or not.

CLEARED IN THIS SESSION:
 - `iterator_to_array()` refused a gradual `preserve_keys`. `emit_preserve_keys_truthiness` already
   lowered `Mixed` through `__rt_mixed_cast_bool`; only `preserve_keys_type_supported` refused it.
 - `array_reverse()` refused a non-literal `preserve_keys`. The two arms produce DIFFERENT
   representations -- a dense array and a hash -- so the call now answers with their union and the
   backend branches at run time, boxing each arm as Mixed. Only an INDEXED source can take that
   route: a hash's `false` arm has to renumber integer keys while keeping string ones, and
   `__rt_hash_to_hash_reverse` preserves every key. That helper is the missing piece.

STILL BLOCKING, 12 sites:
 - `array_slice`, `array_chunk`: same non-literal `preserve_keys` refusal, same fix shape.
 - `array_reverse` twice more, over sources that reach it as hashes.
 - `array_rand($values, 1)`, `strip_tags($s, $allowed)`, `array_column($a, $name, $index)`: the
   CONTRACT declares fewer parameters than PHP has. Each needs the optional parameter plus its
   compiled lowering and its interpreter binding.
 - `strtr($str, self::toArray($from))` and `shuffle($item)`: a gradual/union argument where an array
   is required.
 - `throw $propertyNotAllowedError` twice: the local is `null` joined with a catch binding and the
   `if ($e)` guard does not narrow the null away. NOTE the reduction attempts did NOT reproduce it --
   `catchbind.php` in the scratchpad tries flat, nested, twice-entered and first-only shapes and all
   five match php.
 - `IncludeNode::addGetTemplate($compiler, $template)` called with 2 arguments on a method declared
   with 1 (the second is COMMENTED OUT for BC and the subclass declares it). PHP allows extra
   arguments to a userland function; elephc refuses. Careful: a probe showed that when elephc DOES
   accept extra arguments, the override drops them -- `Child::build($label, $extra)` reached through
   a `Base::build($label)` call site printed `base/` where php prints `base/extra`. Relaxing the
   checker without the ABI would turn a refusal into a silent wrong answer.
 - `ChainLoader::getLoaders` declares `array` and returns `iterable`; a child of
   `BinaryOperatorExpressionParser` passes `array` where the parent declares `string|void`.

ONE THING NOT TO REPEAT: relaxing "undefined method on an interface-typed receiver" to defer to run
time clears 9 of the original 24 errors and BREAKS THREE ERROR TESTS -- `narrowing::
test_this_instanceof_rejects_method_absent_from_interface` and friends pin that diagnostic on
purpose, with `method_exists()` as the only admitted guard. Twig's
`$env->getExtension(SandboxExtension::class)->isSandboxed()` needs `SandboxExtension` in the closed
world instead, or the `@return TExtension` template binding read off the docblock.

## Why

Every filter and every escape on the rendered page runs interpreted until CoreExtension compiles, and that is the whole gap between the measured 12ms compiled-controller route and the 148ms Twig route
