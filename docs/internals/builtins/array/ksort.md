---
title: "ksort() — internals"
description: "Compiler internals for ksort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 54
---

## `ksort()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/ksort.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/ksort.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1267](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1267) (`lower_ksort`)
- **Function symbol**: `lower_ksort()`


### Lowering notes

- Lowers `ksort()` through the legacy key-sort helper surface.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_krsort`
- `__rt_ksort`
- `__rt_natcasesort`
- `__rt_natsort`

## Signature summary

```php
function ksort(array $array): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `ksort()`](../../../php/builtins/array/ksort.md)

