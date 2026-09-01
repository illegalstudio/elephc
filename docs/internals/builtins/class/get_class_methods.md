---
title: "get_class_methods() — internals"
description: "Compiler internals for get_class_methods(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 86
---

## `get_class_methods()` — internals

## Where it lives

- **Signature**: [`crates/elephc-builtin-contract/src/catalog_surfaces.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-builtin-contract/src/catalog_surfaces.rs)
- **Lowering**: [`(not lowered)`:0]()
- **Function symbol**: `(none — type-checker only)()`


## Semantic descriptor

Shared contract intentionally unsupported by the AOT backend.

## EIR and runtime boundary

_No compiled lowering: this surface is intentionally eval-only._

## Signature summary

```php
function get_class_methods(mixed $object_or_class): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/get_class_methods.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_class_methods.rs) (`eval_builtin!`)
- **Execution**: Magician interpreter adapter.
- **Adapter reason**: `callable-or-reflection`.
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `get_class_methods()`](../../../php/builtins/class/get_class_methods.md)
