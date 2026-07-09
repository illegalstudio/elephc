---
title: "array_intersect_assoc() — internals"
description: "Compiler internals for array_intersect_assoc(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 15
---

## `array_intersect_assoc()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_intersect_assoc.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_intersect_assoc.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/arrays.rs`:1371](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/arrays.rs#L1371) (`lower_array_intersect_assoc`)
- **Function symbol**: `lower_array_intersect_assoc()`


### Lowering notes

- Lowers `array_intersect_assoc()` via the shared associative diff/intersect helper (mode 1 = intersect).

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_find_any_all`
- `__rt_array_merge_recursive`
- `__rt_array_udiff_uintersect`
- `__rt_assoc_diff_intersect`

## Signature summary

```php
function array_intersect_assoc(array $array, ...$arrays): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$arrays`.

## Cross-references

- [User reference for `array_intersect_assoc()`](../../../php/builtins/array/array_intersect_assoc.md)
