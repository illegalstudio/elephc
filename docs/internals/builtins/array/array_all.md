---
title: "array_all() — internals"
description: "Compiler internals for array_all(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 1
---

## `array_all()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1712](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1712) (`lower_array_all`)
- **Function symbol**: `lower_array_all()`


### Lowering notes

- Lowers `array_all()`: returns true when every element satisfies the predicate.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_udiff_uintersect`
- `__rt_array_walk_recursive`

## Signature summary

```php
function array_all(array $array, mixed $callback): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `array_all()`](../../../php/builtins/array/array_all.md)

