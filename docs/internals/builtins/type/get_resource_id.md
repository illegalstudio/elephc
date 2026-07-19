---
title: "get_resource_id() — internals"
description: "Compiler internals for get_resource_id(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 421
---

## `get_resource_id()` — internals

## Where it lives

- **Signature**: [`src/builtins/types/get_resource_id.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/types/get_resource_id.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/types.rs`:431](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/types.rs#L431) (`lower_get_resource_id`)
- **Function symbol**: `lower_get_resource_id()`


### Lowering notes

- Lowers `get_resource_id(resource)` by unboxing the native handle and making it one-based.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function get_resource_id(resource $resource): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/get_resource_id.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_resource_id.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `get_resource_id()`](../../../php/builtins/type/get_resource_id.md)
