---
title: "array_is_list() — internals"
description: "Compiler internals for array_is_list(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 17
---

## `array_is_list()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_is_list.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_is_list.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1315](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1315) (`lower_array_is_list`)
- **Function symbol**: `lower_array_is_list()`


### Lowering notes

- Lowers `array_is_list()` to the `__rt_array_is_list` runtime predicate, returning a bool.
- The runtime helper accepts any array kind (indexed, associative hash, or boxed mixed cell) and
- reports `1` when the keys are the sequential integers `0..n-1` in insertion order, `0` otherwise.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_edge_key`
- `__rt_array_is_list`
- `__rt_mixed_from_value`

## Signature summary

```php
function array_is_list(mixed $array): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `array_is_list()`](../../../php/builtins/array/array_is_list.md)
