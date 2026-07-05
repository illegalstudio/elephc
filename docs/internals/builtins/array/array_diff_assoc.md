---
title: "array_diff_assoc() — internals"
description: "Compiler internals for array_diff_assoc(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 7
---

## `array_diff_assoc()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_diff_assoc.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_diff_assoc.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1347](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1347) (`lower_array_diff_assoc`)
- **Function symbol**: `lower_array_diff_assoc()`


### Lowering notes

- Lowers `array_diff_assoc()` via the shared associative diff/intersect helper (mode 0 = diff).

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_merge_recursive`
- `__rt_assoc_diff_intersect`

## Signature summary

```php
function array_diff_assoc(array $array, ...$arrays): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$arrays`.

## Cross-references

- [User reference for `array_diff_assoc()`](../../../php/builtins/array/array_diff_assoc.md)
