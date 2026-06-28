---
title: "natcasesort() — internals"
description: "Compiler internals for natcasesort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 38
---

## `natcasesort()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1111](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1111) (`lower_natcasesort`)
- **Function symbol**: `lower_natcasesort()`


### Lowering notes

- Lowers `natcasesort()` for indexed integer arrays through the case-insensitive wrapper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_natcasesort`

## Signature summary

```php
function natcasesort(array $array): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `natcasesort()`](../../../php/builtins/array/natcasesort.md)

