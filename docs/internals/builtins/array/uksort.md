---
title: "uksort() — internals"
description: "Compiler internals for uksort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 62
---

## `uksort()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/uksort.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/uksort.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1297](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1297) (`lower_uksort`)
- **Function symbol**: `lower_uksort()`


### Lowering notes

- Lowers `uksort()` through the legacy user-sort helper for static comparators.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_is_list`

## Signature summary

```php
function uksort(array $array, callable $callback): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `uksort()`](../../../php/builtins/array/uksort.md)
