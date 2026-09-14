---
title: "ini_get_all() - internals"
description: "Compiler internals for ini_get_all(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 944
---

## `ini_get_all()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_data.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_data.rs)
- **Lowering**: [`src/shared_ini_prelude.rs`:61](https://github.com/illegalstudio/elephc/blob/main/src/shared_ini_prelude.rs#L61) (`ini_get_all`)
- **Function symbol**: `ini_get_all()`


### Lowering notes

- Implemented by the compiler-injected web prelude.

## Semantic descriptor

Shared contract implemented by an injected elephc-PHP prelude.

## EIR and runtime boundary

_Implemented by an injected elephc-PHP prelude._

## Signature summary

```php
function ini_get_all(?string $extension = null, bool $details = true): array|false
```

## What the type checker enforces

- **Arity**: takes 0–2 arguments (2 optional).

## Eval interpreter (magician)

_Not callable from eval'd code - the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `ini_get_all()`](../../../php/builtins/web/ini_get_all.md)
