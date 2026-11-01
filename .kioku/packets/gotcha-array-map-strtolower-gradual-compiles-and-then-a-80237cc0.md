---
id: gotcha-array-map-strtolower-gradual-compiles-and-then-a-80237cc0
type: gotcha
title: "array_map('strtolower', $gradual) compiles and then aborts at run time - the string builtin's callable policy refuses a Mixed source"
description: "The candidate IS compiled; what is missing is a wrapper case for a Mixed element, and the abort now names the candidates"
created: 2026-09-18
verified_by: "18-line reduction (scratchpad/mapstr2.php) against php -n 8.5: php prints alpha,beta;gamma,delta, the compiled binary dies with 'array_map callback string does not name a supported callable (compiled candidates: strtolower)'"
sources:
  - path: src/builtins/semantics.rs
    blob: 741a8f0fe57244910393cf70a084b2da555dd93b
  - path: src/codegen/lower_inst/runtime_wrappers.rs
    blob: c5b3c07ea2747aedfe9afcc99a0c795264736540
  - path: src/codegen/lower_inst/builtins/arrays/map_dispatch.rs
    blob: 548333494bc4212b458f19616aeebeb29fc7a1dc
---

# array_map('strtolower', $gradual) compiles and then aborts at run time - the string builtin's callable policy refuses a Mixed source

## Fact

NOT FIXED. Recorded with its reduction so the next attempt starts from the right place.

    function lowerAll($items): array { return array_map('strtolower', $items); }
    lowerAll(['Alpha', 'Beta']);              // one call site makes $items an array<string>
    lowerAll(['x' => 'Gamma', 'y' => 'Delta']); // a second makes it GRADUAL

`array_map` with a `Str`-typed callback takes `lower_runtime_string_descriptor_callback`, which
emits one case per compiled candidate and aborts if the runtime string matches none. The candidate
list here is `[strtolower]` -- the abort message now prints it -- so the name is reachable; what is
missing is the CASE. `runtime_builtin_descriptor_cases` drops it because
`runtime_builtin_wrapper_supported("strtolower", Some(Mixed))` is false:
`string_runtime_fn_semantics` uses `callable_accepts_string_source`, which demands a `Str` element.

WHY THE POLICY EXISTS: the wrapper is a synthetic EIR function whose parameters ARE the specialized
source types (`build_runtime_call_wrapper_function` -> `wrapper_param_operands` ->
`lower_registry_call`), so a `Mixed` parameter would reach `strtolower`'s lowering, which needs a
concrete string. `strlen` is the exception that shows the shape: it uses
`callable_accepts_strlen_source`, which DOES admit `Mixed`.

WHAT A FIX LOOKS LIKE: coerce each `Mixed` wrapper operand to the builtin's DECLARED parameter type
before `lower_registry_call` (an `Op::RuntimeCall` conversion, the same one
`coerce_operands_to_params` emits for a Mixed argument), then let
`callable_accepts_string_source` admit `Mixed`. That widens every string builtin used as a callback
over a gradual array at once, so it needs the full codegen suite behind it.

WHERE IT BITES IN THE APP: `twig/twig`'s `CoreExtension::getAttribute` does
`array_map('strtolower', $methods)` over `get_class_methods($object)` on EVERY property access of a
rendered template -- and `get_class_methods()` itself has no compiled lowering either
("Call to undefined function get_class_methods()"), which is a second gap on the same line.

## Why

It is the first RUNTIME blocker of the Twig-in-the-closed-world build, which is the lever for a sub-10ms Symfony Twig route
