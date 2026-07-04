---
title: "uasort() — internals"
description: "Compiler internals for uasort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 61
---

## `uasort()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/uasort.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/uasort.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1302](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1302) (`lower_uasort`)
- **Function symbol**: `lower_uasort()`


### Lowering notes

- Lowers `uasort()` through the legacy user-sort helper for static comparators.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_is_list`

## Signature summary

```php
function uasort(array $array, callable $callback): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `uasort()`](../../../php/builtins/array/uasort.md)

