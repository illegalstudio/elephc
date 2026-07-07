---
title: "krsort() — internals"
description: "Compiler internals for krsort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 53
---

## `krsort()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/krsort.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/krsort.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/arrays.rs`:1104](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/arrays.rs#L1104) (`lower_krsort`)
- **Function symbol**: `lower_krsort()`


### Lowering notes

- Lowers `krsort()` through the reverse key-sort helper surface.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_krsort`
- `__rt_natcasesort`
- `__rt_natsort`

## Signature summary

```php
function krsort(array $array): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `krsort()`](../../../php/builtins/array/krsort.md)
