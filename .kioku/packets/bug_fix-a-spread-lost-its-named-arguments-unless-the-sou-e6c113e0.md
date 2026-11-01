---
id: bug_fix-a-spread-lost-its-named-arguments-unless-the-sou-e6c113e0
type: bug_fix
title: "A spread lost its named arguments unless the source was a literal or an assoc-typed local"
description: "PHP binds string keys to parameter names and integer keys positionally in the SAME unpack; elephc chose one or the other at compile time"
created: 2026-09-22
verified_by: "a six-case repro byte-identical to php 8.5.10; cargo test --release --lib -p elephc keeps its 29-failure baseline"
sources:
  - path: src/ir_lower/expr/positional_spreads.rs
    blob: e9617d0205a46a3f01b306f9797653f4e2325438
    lines: 71-105
    snip: 73ab776ee1bb
    anchor: "for param_idx in first_spread_param_idx..regular_param_count {"
  - path: src/ir_lower/expr/named_spreads.rs
    blob: 00a46b41a218ac4abf85274cdcccf193e014c204
    lines: 270-355
    snip: cfbfe9f4a414
    anchor: "pub(super) fn spread_element_expr_for_ir("
---

# A spread lost its named arguments unless the source was a literal or an assoc-typed local

## Fact

`f(...$a)` is decided PER KEY at runtime: `$a['name']` binds the parameter named `name`, `$a[0]`
fills the first position, in one unpack. elephc decided per ARRAY at compile time --
`is_assoc_spread_source` required a statically-known `AssocArray` -- and `positional_spreads.rs`,
which handles every source whose type is `array`/`mixed`/`union`, passed `param_name = None` and
`prefer_named_key = false` unconditionally.

MEASURED against php 8.5.10 (`class Target { __construct(string $name = 'none', ?string
$description = null) }`):

    new Target(...['name' => 'inline', …])   both correct
    new Target(...$keyedLocal)               both correct
    new Target(...namedArgs())               php: fromCall/desc    elephc: none/NULL
    new $class(...namedArgs())               php: fromCall/desc    elephc: none/NULL

A function declared `: array` carries `PhpType::Array` whatever keys it returns, so a call source
was ruled "a packed list" and its names dropped. Symfony's console dies on exactly that:
`ReflectionAttribute::newInstance()`'s fallback is `new $this->__name(...$this->__args)`.

FIX: the element readers ask for the NAME first and fall back to the POSITION, which is right for a
keyed array, a packed list and the mixed form alike; the optional-parameter guard admits either
spelling. `positional_spreads.rs` now passes the parameter name whenever the source is not a
proven packed `array<T>`.

COST, AND IT IS NOT SMALL: the probe is inlined per spread parameter (an `array_key_exists` call,
a ternary and a second array read), and it added 16.8 MB of assembly and ~20% CPU to the Symfony
`--web` compile. Nesting the ternaries instead of OR-ing two guards recovered only 1.4 MB -- the
cost is the probe existing, not its shape. The compression left to do is one runtime helper
(`spread ptr, index, name`) replacing the inlined form.

STILL OPEN, found by the same repro: a function called ONLY through a spread
(`build(...$args)`) is pruned from the program -- `Call to undefined function build()`.

## Why

ReflectionAttribute::newInstance() does new $class(...$args) with a string-keyed array, so every #[AsCommand(name: 'x')] was constructed empty
