---
title: "setlocale() - internals"
description: "Compiler internals for setlocale(): lowering path, type checks, and runtime helpers."
---

## `setlocale()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/locale_prelude.rs`](https://github.com/illegalstudio/elephc/blob/main/src/locale_prelude.rs) (`setlocale`)
- **Function symbol**: `setlocale()`


### Lowering notes

- Implemented by the compiler-injected locale prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function setlocale(int $category, mixed $locales, ...$rest): string|false
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **Variadic**: collects excess arguments into `$rest`.

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `setlocale()`](../../../php/builtins/string/setlocale.md)
