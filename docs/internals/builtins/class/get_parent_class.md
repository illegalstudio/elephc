---
title: "get_parent_class() — internals"
description: "Compiler internals for get_parent_class(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 84
---

## `get_parent_class()` — internals

## Where it lives

- **Signature**: [`src/builtins/callables/get_parent_class.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/callables/get_parent_class.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/types.rs`:331](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/types.rs#L331) (`lower_class_name_lookup`)
- **Function symbol**: `lower_class_name_lookup()`


### Lowering notes

- Lowers `get_class()` and `get_parent_class()` through static or dynamic class metadata.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function get_parent_class(mixed $object_or_class = null): string
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/symbols/get_parent_class.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/symbols/get_parent_class.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `get_parent_class()`](../../../php/builtins/class/get_parent_class.md)
