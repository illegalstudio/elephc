---
title: "datefmt_get_locale() - internals"
description: "Compiler internals for datefmt_get_locale(): lowering path, type checks, and runtime helpers."
---

## `datefmt_get_locale()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_intl.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_intl.rs)
- **Lowering**: [`src/intl/functions.rs`](https://github.com/illegalstudio/elephc/blob/main/src/intl/functions.rs) (`datefmt_get_locale`)
- **Function symbol**: `datefmt_get_locale()`


### Lowering notes

- Implemented by the compiler-injected Intl prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function datefmt_get_locale(object $formatter, int $type = 0): string|false
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `datefmt_get_locale()`](../../../php/builtins/intl/datefmt_get_locale.md)
