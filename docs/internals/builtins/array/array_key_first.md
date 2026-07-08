---
title: "array_key_first() — internals"
description: "Compiler internals for array_key_first(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 19
---

## `array_key_first()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_key_first.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_key_first.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/arrays.rs`:1161](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/arrays.rs#L1161) (`lower_array_key_first`)
- **Function symbol**: `lower_array_key_first()`


### Lowering notes

- Lowers `array_key_first()` through the shared edge-key helper with selector `0`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_edge_key`
- `__rt_mixed_from_value`

## Signature summary

```php
function array_key_first(array $array): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `array_key_first()`](../../../php/builtins/array/array_key_first.md)
