---
id: gotcha-every-remaining-twig-blocker-needs-a-runtime-hel-d0fd4012
type: gotcha
title: "Every remaining Twig blocker needs a runtime helper that does not exist, and array_rand shows why"
description: "Closing the contract arity on array_rand moved the error rather than removing it: the argument arrives gradual, and the indexed helper is the only one there is"
created: 2026-09-18
verified_by: "elephc --check index_preload_twig.php; error_tests back to 1431/99 after retargeting test_error_array_rand_wrong_args; elephc-builtin-contract 24/24"
sources:
  - path: src/builtins/array/array_rand.rs
    blob: 5d1a99078db1c792c9dc4823f6239edc29b6a7fd
  - path: crates/elephc-builtin-contract/src/catalog_data.rs
    blob: 793954f7bef705651e27a3d7f1162b872f4d2f35
  - path: src/codegen/lower_inst/builtins/arrays/misc_dispatch.rs
    blob: aff4c99a2b5d84479c320af211c0c6dbcfacd677
---

# Every remaining Twig blocker needs a runtime helper that does not exist, and array_rand shows why

## Fact

`array_rand($values, 1)` failed as `takes exactly 1 argument`. The contract now declares the
optional `$num`, the checker accepts the LITERAL 1 -- which php-src answers identically to the
one-argument form, a single key -- and refuses anything else, and the lowering ignores the extra
operand. The arity error is gone.

THE ERROR DID NOT GO AWAY. It became:

    error[CoreExtension.php:509]: array_rand() argument must be array

because two lines above it twig does `$values = self::toArray($values);` and `toArray()` is
untyped, so `$values` is gradual. `lower_array_rand` calls `require_indexed_array_builtin`, and
`__rt_array_rand` walks fixed-size slots: there is no hash random-key helper and no runtime
array-vs-hash dispatch for it.

THAT IS THE SHAPE OF EVERY ONE THAT IS LEFT. Each needs a runtime routine that does not exist yet,
hand-written for ARM64 AND x86_64:
 - a hash random-key helper (`array_rand` over a gradual value);
 - a hash reverse/slice/chunk that RENUMBERS integer keys and keeps string ones (the `false` arm of
   a non-literal `preserve_keys`, three call sites);
 - `strip_tags($s, $allowed)` -- the allowed-tag filter, and the interpreter has no `strip_tags` at
   all, so both sides start from nothing;
 - `array_column($a, $col, $index)` -- the re-keying third argument, missing in both backends;
 - a real iterable-to-array CONVERSION where the checker currently would only re-type (re-typing
   segfaults, measured);
 - parameter storage for arguments beyond an override's ancestor arity.

WHAT THE CHECKER COULD STILL REACH IS DONE. The remaining distance is runtime code.

NOTE, unrelated and PRE-EXISTING: `builtin_parity_tests::non_registry_surfaces_have_complete_backend_contracts`
asserts `exceptional.len() == 410` and the tree answers 420. Adding a PARAMETER cannot move that
count -- it counts contracts -- so the drift came in with the catalog edits already in the working
tree at the start of the session.

## Why

It is the shape of ALL six remaining blockers, and it says plainly that the rest is assembly work, not checker work
