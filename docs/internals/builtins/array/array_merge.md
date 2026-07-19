---
title: "array_merge() — internals"
description: "Compiler internals for array_merge(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 23
---

## `array_merge()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_merge.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_merge.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/arrays.rs`:846](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/arrays.rs#L846) (`lower_array_merge`)
- **Function symbol**: `lower_array_merge()`


### Lowering notes

- Lowers `array_merge()` for two compatible indexed arrays with 8-byte payload slots.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_diff`
- `__rt_array_diff_refcounted`

## Signature summary

```php
function array_merge(...$arrays): array
```

## What the type checker enforces

- **Arity**: takes no arguments.
- **Variadic**: collects excess arguments into `$arrays`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/array/array_merge.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_merge.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`
- **Variadic**: collects excess arguments into `$arrays`.

## Cross-references

- [User reference for `array_merge()`](../../../php/builtins/array/array_merge.md)
