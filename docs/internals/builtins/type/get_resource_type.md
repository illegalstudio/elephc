---
title: "get_resource_type() — internals"
description: "Compiler internals for get_resource_type(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 421
---

## `get_resource_type()` — internals

## Where it lives

- **Signature**: [`src/builtins/types/get_resource_type.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/types/get_resource_type.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/types.rs`:419](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/types.rs#L419) (`lower_get_resource_type`)
- **Function symbol**: `lower_get_resource_type()`


### Lowering notes

- Lowers `get_resource_type(resource)` to elephc's current resource type label.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function get_resource_type(resource $resource): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/get_resource_type.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_resource_type.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `get_resource_type()`](../../../php/builtins/type/get_resource_type.md)
