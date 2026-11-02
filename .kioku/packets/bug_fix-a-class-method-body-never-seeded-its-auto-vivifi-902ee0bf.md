---
id: bug_fix-a-class-method-body-never-seeded-its-auto-vivifi-902ee0bf
type: bug_fix
title: "A class method body never seeded its auto-vivified array locals"
description: "Every other scope seeded them; a method did not, so the checker refused code the backend could lower"
created: 2026-09-20
sources:
  - path: src/types/checker/method_pass.rs
    blob: 2ed0e785a9ce78f56ad9e249f51ee6a0eca222d4
    lines: 150-166
    snip: afa7199b6d29
    anchor: ".unwrap_or(fallback_ty);"
  - path: src/append_vivify.rs
    blob: 9085397528a17249d310a223fb18a8dfdbd40f79
    lines: 30-46
    snip: 6a9046cb01d0
    anchor: "pub(crate) fn vivified_array_locals<'a>("
---

# A class method body never seeded its auto-vivified array locals

## Fact

`$keys[] = $k` against a name nothing has assigned creates the array -- PHP auto-vivifies it --
and `Checker::seed_vivified_array_locals()` binds those names at scope entry so a later read in
the same scope is not "Undefined variable". It was called for a plain function
(`resolve_function_signature`), for a closure (`infer_closure_type_with_param_hints`) and for
file scope (`check_top_level_program`), and never for a class method: the method pass builds its
own environment.

EIR lowering runs the same scan for EVERY body (`lower_body_into_function`), so the two sides
disagreed and the checker refused code the backend could lower. Symfony's `StubCaster::castEnum()`
is the shape, and it is what blocked compiling symfony/var-dumper.

The seed goes in after the parameters, the variadic and the promoted constructor properties are
bound, because the scan treats everything already in the environment as bound -- seeding earlier
would make a parameter look like a vivification.

Reducing it: build UP from the shape that works, not down from the one that fails.
`scratchpad/vivify/case_ns.php` (same body, free function, namespaced) compiles and
`case_static.php` (same body, static method) does not -- that one difference is the whole bug.
Four cut-down variants of the failing file all still failed and named nothing.

Attribution: `codegen_tests arrays` 913 passed/31 failed without the seed, 914/30 with it.

Still open alongside: `isset()` on a local only an append would create answers TRUE where PHP
answers false, because the entry seed stores an empty array. It predates this and is identical
for a function, a method and a static method (`scratchpad/vivify/case_isset.php`).

## Why

Blocks compiling any vendor code that fills an array in a loop inside a method, which is ordinary PHP
