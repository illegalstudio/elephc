---
title: "array_diff() — internals"
description: "Compiler internals for array_diff(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 4
---

## `array_diff()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:874](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L874) (`lower_array_diff`)
- **Function symbol**: `lower_array_diff()`


### Lowering notes

- Lowers `array_diff()` for two compatible indexed arrays with pointer-sized payload slots.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_diff`
- `__rt_array_diff_key`
- `__rt_array_diff_refcounted`
- `__rt_array_intersect`
- `__rt_array_intersect_key`
- `__rt_array_intersect_refcounted`

## Signature summary

```php
function array_diff(array $array, ...$arrays): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$arrays`.

## Cross-references

- [User reference for `array_diff()`](../../../php/builtins/array/array_diff.md)

