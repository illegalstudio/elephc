---
title: "array_merge_recursive() — internals"
description: "Compiler internals for array_merge_recursive(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 24
---

## `array_merge_recursive()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_merge_recursive.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_merge_recursive.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1537](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1537) (`lower_array_merge_recursive`)
- **Function symbol**: `lower_array_merge_recursive()`


### Lowering notes

- Lowers `array_merge_recursive()` (recursive merge with scalar collisions combined into lists).

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_find_any_all`
- `__rt_array_merge_recursive`
- `__rt_array_udiff_uintersect`

## Signature summary

```php
function array_merge_recursive(...$arrays): array
```

## What the type checker enforces

- **Arity**: takes no arguments.
- **Variadic**: collects excess arguments into `$arrays`.

## Cross-references

- [User reference for `array_merge_recursive()`](../../../php/builtins/array/array_merge_recursive.md)

