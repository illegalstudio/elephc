---
id: bug_fix-rt-hash-reindex-gives-a-hash-its-preserve-keys-f-0c9b703c
type: bug_fix
title: "__rt_hash_reindex gives a hash its preserve_keys=false arm, and boxing a two-arm result must be conditional"
description: "array_reverse over a hash had no false arm at all; the new helper supplies php's renumbering rule, and the dynamic-flag lowering only boxes when the arms really differ"
created: 2026-09-18
verified_by: "tests/codegen/regressions/arrays.rs::test_array_reverse_over_a_hash_renumbers_only_its_integer_keys, seven shapes byte-checked against php -n; the emitted linux-x86_64 runtime cross-assembles clean with clang -target x86_64-unknown-linux-gnu; error_tests 1431/99, codegen::arrays 665/27, codegen::regressions 413/6, all with the same failure names"
sources:
  - path: src/codegen_support/runtime/arrays/hash_reindex.rs
    blob: e8648f0b36ab3b95f64840305e1609ff5dc5da0b
  - path: src/codegen/lower_inst/builtins/arrays/basic.rs
    blob: 4f59a82e85f07f36819f6b2205cb4cb67ee8a0bf
  - path: src/builtins/array/array_reverse.rs
    blob: 616ea05390a5be65afdfe8e5de11238176dca578
---

# __rt_hash_reindex gives a hash its preserve_keys=false arm, and boxing a two-arm result must be conditional

## Fact

`array_reverse($hash, false)` did not compile AT ALL:

    unsupported EIR backend feature: array_reverse for PHP type AssocArray

`__rt_hash_to_hash_reverse` preserves every key, and the `false` helper walks the fixed-size slots
of an indexed array, which a hash does not have. php's rule for the flag is not "drop the keys": it
renumbers INTEGER keys from zero in the NEW order and leaves STRING keys exactly where they are.

`__rt_hash_reindex(hash) -> hash` implements that one rule and nothing else, so the callers compose
instead of duplicating: the key-preserving helper runs first, its result goes through the
renumberer. `array_slice` and `array_chunk` can take the same route.

THE KEY ENCODING, which is the thing to know before touching any hash helper: `__rt_hash_set(hash,
key_lo, key_hi, val_lo, val_hi, tag)` reads `key_hi == -1` as "this is an INTEGER key and `key_lo`
is the integer". Anything else is a string pointer/length pair. `__rt_array_to_hash_reverse` writes
integer keys that way and the entry layout stores the pair at entry[8]/entry[16]. Walk order comes
from the doubly linked insertion chain: head at header[24], next at entry[56] (tail at header[32],
prev at entry[48] for a reverse walk). Advance the cursor BEFORE any call -- every per-entry helper
clobbers the caller-saved registers it would live in.

THE SECOND BUG, and it segfaulted: a flag known only at run time emits both arms, and the earlier
`iterator_to_array` pattern boxes each one as Mixed so they share a representation. That is only
correct WHEN THE ARMS DIFFER. A hash reverses to a hash either way, so the checker answers ONE
concrete type for it, and boxing then hands the caller a cell where its own declared type says
container pointer: `count()` on the result crashed while `implode(array_keys(...))` happened to
survive. The lowering now boxes only when `inst.result_php_type` is `Mixed` or a `Union`.

STILL REFUSED, deliberately: a GRADUAL source with a run-time flag. There are no two statically
known arms to emit, and the diagnostic is the gradual wording the literal-`true` case already used.
That is also why twig's `array_slice($item, $start, $length, $preserveKeys)` is still blocked --
`$item` there is gradual, so the hash support alone does not reach it.

## Why

It is the shared primitive behind every  container result, so array_slice and array_chunk can compose it the same way
