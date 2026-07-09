---
title: "array_find() — internals"
description: "Compiler internals for array_find(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 12
---

## `array_find()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_find.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_find.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/arrays.rs`:1566](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/arrays.rs#L1566) (`lower_array_find`)
- **Function symbol**: `lower_array_find()`


### Lowering notes

- Lowers `array_find()`: returns the first element satisfying the predicate, boxed as Mixed (or null).

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_walk_recursive`

## Signature summary

```php
function array_find(mixed $array, mixed $callback): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `array_find()`](../../../php/builtins/array/array_find.md)
