---
title: "grapheme_extract() - internals"
description: "Compiler internals for grapheme_extract(): lowering path, type checks, and runtime helpers."
---

## `grapheme_extract()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`grapheme_extract`)
- **Function symbol**: `grapheme_extract()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function grapheme_extract(string $haystack, int $size, int $type = GRAPHEME_EXTR_COUNT, int $offset = 0, mixed $next = null): string|false
```

## What the type checker enforces

- **Arity**: takes 2–5 arguments (3 optional).
- **By-reference parameters**: `$next`.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `grapheme_extract()`](../../../php/builtins/intl/grapheme_extract.md)
