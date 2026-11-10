---
title: "collator_asort() - internals"
description: "Compiler internals for collator_asort(): lowering path, type checks, and runtime helpers."
---

## `collator_asort()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`collator_asort`)
- **Function symbol**: `collator_asort()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function collator_asort(object $object, array $array, int $flags = Collator::SORT_REGULAR): bool
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).
- **By-reference parameters**: `$array`.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `collator_asort()`](../../../php/builtins/intl/collator_asort.md)
