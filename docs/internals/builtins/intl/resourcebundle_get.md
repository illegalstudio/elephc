---
title: "resourcebundle_get() - internals"
description: "Compiler internals for resourcebundle_get(): lowering path, type checks, and runtime helpers."
---

## `resourcebundle_get()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`resourcebundle_get`)
- **Function symbol**: `resourcebundle_get()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function resourcebundle_get(object $bundle, string|int $index, bool $fallback = true): object|array|string|int|null
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `resourcebundle_get()`](../../../php/builtins/intl/resourcebundle_get.md)
