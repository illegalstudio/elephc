---
title: "krsort() — internals"
description: "Compiler internals for krsort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 36
---

## `krsort()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1101](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1101) (`lower_krsort`)
- **Function symbol**: `lower_krsort()`


### Lowering notes

- Lowers `krsort()` through the legacy reverse key-sort helper surface.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_krsort`
- `__rt_natcasesort`
- `__rt_natsort`

## Signature summary

```php
function krsort(array $array, int $flags): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `krsort()`](../../../php/builtins/array/krsort.md)

