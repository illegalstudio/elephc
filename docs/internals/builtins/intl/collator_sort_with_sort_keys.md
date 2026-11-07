---
title: "collator_sort_with_sort_keys() - internals"
description: "Compiler internals for collator_sort_with_sort_keys(): lowering path, type checks, and runtime helpers."
---

## `collator_sort_with_sort_keys()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`collator_sort_with_sort_keys`)
- **Function symbol**: `collator_sort_with_sort_keys()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function collator_sort_with_sort_keys(object $object, array $array): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **By-reference parameters**: `$array`.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `collator_sort_with_sort_keys()`](../../../php/builtins/intl/collator_sort_with_sort_keys.md)
