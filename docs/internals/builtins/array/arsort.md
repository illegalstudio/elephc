---
title: "arsort() — internals"
description: "Compiler internals for arsort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 32
---

## `arsort()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1091](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1091) (`lower_arsort`)
- **Function symbol**: `lower_arsort()`


### Lowering notes

- Lowers `arsort()` for indexed integer arrays through the descending value-sort wrapper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_arsort`
- `__rt_krsort`
- `__rt_ksort`
- `__rt_natcasesort`
- `__rt_natsort`

## Signature summary

```php
function arsort(array $array, int $flags): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `arsort()`](../../../php/builtins/array/arsort.md)

