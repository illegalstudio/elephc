---
id: runbook-the-runtime-preserve-keys-pattern-emit-both-arms-af2c0cb9
type: runbook
title: "The runtime preserve_keys pattern: emit both arms, box only when they differ, and never for a gradual source"
description: "array_reverse, array_slice and array_chunk now take a flag known only at run time; the recipe, the two traps, and the one source shape that still cannot"
created: 2026-09-18
verified_by: "tests/codegen/regressions/arrays.rs::test_array_slice_and_chunk_take_a_runtime_preserve_keys_flag and ::test_array_reverse_over_a_hash_renumbers_only_its_integer_keys, all value-checked against php -n; error_tests 1431/99 with identical names after retargeting the three now-obsolete error tests"
sources:
  - path: src/codegen/lower_inst/builtins/arrays/basic.rs
    blob: 4cc7c4746484c2b80af6f6824478705a6ff789cf
  - path: src/codegen/lower_inst/builtins/arrays/reduce_sets.rs
    blob: 07935744e40b21514995d21a69c930c795fbbcc3
  - path: src/builtins/array/array_slice.rs
    blob: 162ab6f0b768f1e939b5fe54a43410e618358ef4
---

# The runtime preserve_keys pattern: emit both arms, box only when they differ, and never for a gradual source

## Fact

THE RECIPE, for a flag whose value decides the RESULT SHAPE:

 1. CHECKER: when the flag is not a literal, compute BOTH arm types and answer
    `normalize_union_type(vec![a, b])`. Refuse when either arm has no native lowering.
 2. LOWERING: `emit_preserve_keys_truthiness(ctx, flag)` (shared, in `builtins::spl`), branch on
    zero, emit each arm, converge at a label.
 3. Factor each arm into a function that takes ITS OWN result type as an argument rather than
    reading `inst.result_php_type` -- at the branch the instruction's type is the union, and the
    arm needs its own.

TRAP 1, and it segfaults: BOX EACH ARM ONLY WHEN THE ARMS DIFFER. `array_reverse($hash, $flag)`
answers one concrete type, because a hash reverses to a hash either way; boxing there hands the
caller a cell where its declared type says container pointer. `count()` on the result crashed while
`implode(array_keys(...))` survived, so the shape of the consumer decides whether you see it.

TRAP 2: a GRADUAL source cannot take this route, and the reason is not the flag. `array_slice` and
`array_chunk` answer their gradual forms through COMPATIBILITY PRELUDE FUNCTIONS
(`__elephc_array_chunk_gradual` and friends), which are PHP source the frontend redirects the call
to -- not a native lowering the backend can branch to. Extending the prelude with a flagged variant
was tried and reverted: the helper's chunks carry the source keys, so they are hashes, while the
two-argument helper's are dense arrays, and the checker's declared result type must be the one the
helper's BODY infers. Getting that wrong does not fail to build, it segfaults at the first
consumer.

WHERE IT LEAVES TWIG. `CoreExtension::slice()` is cleared -- its source is indexed. `::batch()` is
NOT: it chunks `self::toArray($items, $preserveKeys)`, which is gradual, so the prelude problem
above is the whole blocker there.

## Why

It is the third builtin to need it, the recipe is now stable, and two of the obvious variations crash
