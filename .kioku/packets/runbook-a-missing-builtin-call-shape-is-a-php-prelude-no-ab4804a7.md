---
id: runbook-a-missing-builtin-call-shape-is-a-php-prelude-no-ab4804a7
type: runbook
title: "A missing builtin call shape is a PHP prelude, not new assembly - and the one rule that makes it not segfault"
description: "array_column 3-arg, gradual array_rand, gradual flagged array_chunk and 2-arg strip_tags all landed as compatibility-prelude PHP in an afternoon; the write-site rule is what separates working from crashing"
created: 2026-09-18
verified_by: "tests/codegen/regressions/arrays.rs::test_prelude_backed_array_column_rand_and_chunk_shapes and string_memory.rs::test_strip_tags_keeps_its_allowed_tags, both value-checked against php -n; error_tests 1431/99 and codegen::arrays 665/27 with identical failure names"
sources:
  - path: src/backend_gap_prelude.rs
    blob: 77147c4f522093b6e844bbd388790cc0de737d34
  - path: src/ir_lower/expr/compat_preludes.rs
    blob: 1154a6c432bab1a674d285d4df7524927bc98324
---

# A missing builtin call shape is a PHP prelude, not new assembly - and the one rule that makes it not segfault

## Fact

THE MOVE. When a builtin's call shape has no native lowering -- an extra parameter, a result whose
keys come from the data, a gradual container the dense helpers cannot walk -- write it as PHP in
`backend_gap_prelude.rs` and redirect the call in `compat_preludes.rs`. It compiles like any other
user code. It is not a fallback to the interpreter; the prelude IS compiled.

THREE SHAPES, all three used here:
 - A NEW HELPER plus a redirect keyed on arity or on the argument's type:
   `array_column($a, $col, $index)`, `array_rand($gradual)`, `array_chunk($gradual, $n, $flag)`.
 - EXTENDING THE PHP-VISIBLE WRAPPER, when the builtin already is one. `strip_tags` was
   `function strip_tags(string $string): string`, so the two-argument call was an ARITY error, not
   a lowering gap. Giving the wrapper `$allowed = null` and dispatching inside PHP was the whole
   fix -- no redirect at all.
 - Leaving the native path alone for the shapes it already serves. The redirect condition must
   match the checker's arms EXACTLY. Routing a dense three-argument `array_chunk()` to the prelude
   broke `test_array_chunk_preserve_keys_mixed_payloads`: the call site kept its own result type
   while the callee returned the helper's, and the chunks printed empty.

THE RULE THAT MAKES IT NOT SEGFAULT. **Every write into a result must go through an explicit key.**
`$result[] = $chunk;` beside `$chunk[$k] = $v;` is what made the first gradual-chunk helper crash:
one level was inferred as a dense array, the other as a hash, and the caller then read boxed cells
where the callee had stored raw pointers. Rewriting both as `$result[$outer] = $chunk;` and
`$chunk[$key] = $value;` fixed it with no other change. The checker's declared result type must then
be what that body INFERS -- `AssocArray { key: Mixed, value: Mixed }` for a keyed result -- which is
the rule the existing `array_chunk` comment already stated and which is worth reading before adding
a helper.

WHERE PHP'S OWN BEHAVIOUR IS NOT THE OBVIOUS ONE, met while writing these:
 - `array_column`'s append key is not a plain counter: an integer index key pushes the counter past
   itself, exactly as `$result[] =` would.
 - `strip_tags` only treats `<` as a tag start when the next byte is a letter, `/`, `!` or `?`,
   which is why `'a < b and c > d'` survives untouched; comments are dropped whatever the allowlist
   says; an unterminated tag swallows the rest of the string. Twenty cases were diffed against
   `php -n` before the scanner went into the prelude.

## Why

Four twig blockers were sized as days of two-architecture assembly and were hours of PHP instead; the twig blocker count went 24 -> 2
