---
title: "rsort() — internals"
description: "Compiler internals for rsort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 56
---

## `rsort()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1250](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1250) (`lower_rsort`)
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
function rsort(array $value, int $flags): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `rsort()`](../../../php/builtins/array/rsort.md)

