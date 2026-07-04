---
title: "rsort() — internals"
description: "Compiler internals for rsort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 58
---

## `rsort()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/rsort.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/rsort.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1245](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1245) (`lower_rsort`)
- **Function symbol**: `lower_rsort()`


### Lowering notes

- Lowers `rsort()` for indexed integer arrays by mutating the source array in place.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_arsort`
- `__rt_asort`
- `__rt_krsort`
- `__rt_ksort`
- `__rt_rsort_float`
- `__rt_rsort_int`
- `__rt_rsort_str`

## Signature summary

```php
function rsort(array $array): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `rsort()`](../../../php/builtins/array/rsort.md)

