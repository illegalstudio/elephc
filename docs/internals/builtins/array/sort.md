---
title: "sort() — internals"
description: "Compiler internals for sort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 58
---

## `sort()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1238](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1238) (`lower_sort`)
- **Function symbol**: `lower_sort()`


### Lowering notes

- Lowers `sort()` for indexed integer arrays by mutating the source array in place.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_arsort`
- `__rt_asort`
- `__rt_rsort_float`
- `__rt_rsort_int`
- `__rt_rsort_str`
- `__rt_sort_float`
- `__rt_sort_int`
- `__rt_sort_str`

## Signature summary

```php
function sort(array $value, int $flags): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `sort()`](../../../php/builtins/array/sort.md)

