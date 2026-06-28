---
title: "array_intersect() — internals"
description: "Compiler internals for array_intersect(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 10
---

## `array_intersect()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:885](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L885) (`lower_array_intersect`)
- **Function symbol**: `lower_array_intersect()`


### Lowering notes

- Lowers `array_intersect()` for two compatible indexed arrays with pointer-sized payload slots.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_diff_key`
- `__rt_array_intersect`
- `__rt_array_intersect_key`
- `__rt_array_intersect_refcounted`

## Signature summary

```php
function array_intersect(array $array, ...$arrays): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$arrays`.

## Cross-references

- [User reference for `array_intersect()`](../../../php/builtins/array/array_intersect.md)

