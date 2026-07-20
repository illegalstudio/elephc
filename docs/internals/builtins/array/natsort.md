---
title: "natsort() — internals"
description: "Compiler internals for natsort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 56
---

## `natsort()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/natsort.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/natsort.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/arrays.rs`:1109](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/arrays.rs#L1109) (`lower_natsort`)
- **Function symbol**: `lower_natsort()`


### Lowering notes

- Lowers `natsort()` for indexed integer arrays through the natural-sort runtime wrapper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_natcasesort`
- `__rt_natsort`

## Signature summary

```php
function natsort(array $array): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **By-reference parameters**: `$array`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/array/natsort.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/natsort.rs) (`eval_builtin!`)
- **Dispatch hooks**: `values`
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `natsort()`](../../../php/builtins/array/natsort.md)
