---
title: "get_called_class() - internals"
description: "Compiler internals for get_called_class(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 83
---

## `get_called_class()` - internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: unavailable for this backend.
- **Function symbol**: none.


## Semantic descriptor

Shared contract intentionally unsupported by the AOT backend.

## EIR and runtime boundary

_No compiled lowering: this surface is intentionally eval-only._

## Signature summary

```php
function get_called_class(): mixed
```

## Signature constraints

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/get_called_class.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_called_class.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `callable-or-reflection`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `get_called_class()`](../../../php/builtins/class/get_called_class.md)
