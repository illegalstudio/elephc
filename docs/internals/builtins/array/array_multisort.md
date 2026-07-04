---
title: "array_multisort() — internals"
description: "Compiler internals for array_multisort(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 25
---

## `array_multisort()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_multisort.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_multisort.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1842](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1842) (`lower_array_multisort`)
- **Function symbol**: `lower_array_multisort()`


### Lowering notes

- Lowers `array_multisort()`: stable-sorts the first indexed array ascending and reorders the second
- in tandem, both in place. Both arguments are by-reference, so each is copy-on-write split with
- `ensure_unique_sort_source` and the (possibly relocated) pointer is written back to its local
- before the runtime mutates the storage. Returns `true`. Supports 8-byte scalar indexed arrays.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_multisort`

## Signature summary

```php
function array_multisort(array $array1, int $array2): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **By-reference parameters**: `$array1`, `$array2`.

## Cross-references

- [User reference for `array_multisort()`](../../../php/builtins/array/array_multisort.md)

