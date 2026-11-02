---
id: bug_fix-asort-and-arsort-renumbered-the-keys-of-a-list-i-36323117
type: bug_fix
title: "asort and arsort renumbered the keys of a list instead of carrying them"
description: "The values sorted correctly and the keys came out 0,1,2; an indexed array cannot represent the result, so the receiver is promoted to a hash first"
created: 2026-09-17
sources:
  - path: src/ir_lower/expr/array_builtin_args.rs
    blob: 92e3e576411cf69a0c461522d64fd1464305074c
  - path: src/codegen/lower_inst/builtins/arrays/misc_dispatch.rs
    blob: 676921de98d3a3e6d694088a878930a37ac283dc
---

# asort and arsort renumbered the keys of a list instead of carrying them

## Fact

`asort()` and `arsort()` renumbered the keys of a LIST; PHP carries each key with its value.

    $a = [3, 1, 2]; asort($a);
    php -n : keys 1|2|0 => values 1|2|3
    elephc : keys 0|1|2 => values 1|2|3

The values were ordered correctly, so the fault only shows if you look at the keys — and carrying
the key is the entire difference between `asort()` and `sort()`.

Measured across the whole family on `[3, 1, 2]`: `ksort`, `krsort`, `sort`, `rsort` and `usort`
were already correct. `asort`, `arsort` and `uasort` were not.

An indexed array has no key storage for a slot permuter to carry, so the result is not
representable as a list at all: it has to be a hash. `lower_unset_indexed_element` already performs
exactly that promotion at its own site (`ArrayToHash` plus a retype of the local), and the codegen
already routes a hash receiver to `__rt_hash_asort` / `__rt_hash_arsort`, so the fix is a three-line
pre-pass in `lower_builtin_call_args` and the sorters are untouched.

It also unblocked Twig's `Lexer::getOperatorRegex()`, which `arsort()`s an `array_combine()` result
that arrives typed `array<mixed>` — the backend refused it with
`unsupported EIR backend feature: arsort indexed-array element PHP type Mixed`.

`uasort` is deliberately LEFT WRONG: there is no `__rt_hash_uasort`, so promoting its receiver would
replace a wrong answer with a refusal to compile. The regression test asserts its current behaviour
so that adding that helper has a stated expectation to meet.
