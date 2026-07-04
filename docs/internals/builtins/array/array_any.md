---
title: "array_any() — internals"
description: "Compiler internals for array_any(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 2
---

## `array_any()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_any.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_any.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1702](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1702) (`lower_array_any`)
- **Function symbol**: `lower_array_any()`


### Lowering notes

- Lowers `array_any()`: returns true when some element satisfies the predicate.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_walk_recursive`

## Signature summary

```php
function array_any(mixed $array, mixed $callback): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `array_any()`](../../../php/builtins/array/array_any.md)

