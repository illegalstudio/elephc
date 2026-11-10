---
title: "transliterator_transliterate() - internals"
description: "Compiler internals for transliterator_transliterate(): lowering path, type checks, and runtime helpers."
---

## `transliterator_transliterate()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`transliterator_transliterate`)
- **Function symbol**: `transliterator_transliterate()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function transliterator_transliterate(mixed $transliterator, string $string, int $start = 0, int $end = -1): string|false
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `transliterator_transliterate()`](../../../php/builtins/intl/transliterator_transliterate.md)
