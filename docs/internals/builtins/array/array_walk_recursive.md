---
title: "array_walk_recursive() — internals"
description: "Compiler internals for array_walk_recursive(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 46
---

## `array_walk_recursive()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_walk_recursive.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_walk_recursive.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1713](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1713) (`lower_array_walk_recursive`)
- **Function symbol**: `lower_array_walk_recursive()`


### Lowering notes

- Lowers `array_walk_recursive()`: invokes the callback on each scalar leaf of a (possibly nested)
- array, descending into array-valued elements. Returns void; leaves are passed as 8-byte scalars.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_udiff_uintersect`
- `__rt_array_walk_recursive`

## Signature summary

```php
function array_walk_recursive(array $array, callable $callback): void
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `array_walk_recursive()`](../../../php/builtins/array/array_walk_recursive.md)

