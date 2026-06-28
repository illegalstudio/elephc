---
title: "asort() — internals"
description: "Compiler internals for asort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 33
---

## `asort()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1086](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1086) (`lower_asort`)
- **Function symbol**: `lower_asort()`


### Lowering notes

- Lowers `asort()` for indexed integer arrays through the value-sort runtime wrapper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_arsort`
- `__rt_asort`
- `__rt_krsort`
- `__rt_ksort`
- `__rt_natcasesort`
- `__rt_natsort`

## Signature summary

```php
function asort(array $array, int $flags): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `asort()`](../../../php/builtins/array/asort.md)

