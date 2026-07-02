---
title: "array_slice() — internals"
description: "Compiler internals for array_slice(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 37
---

## `array_slice()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1033](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1033) (`lower_array_slice`)
- **Function symbol**: `lower_array_slice()`


### Lowering notes

- Lowers `array_slice()` for indexed arrays with pointer-sized payload slots.
- With a literal `preserve_keys=true` 4th argument the checker types the result as an
- integer-keyed associative array; that result shape is the signal to build a key-preserving
- hash through `__rt_array_slice_preserve` instead of a reindexed indexed array.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_slice_preserve`

## Signature summary

```php
function array_slice(array $array, int $offset, int $length, bool $preserve_keys): array
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Cross-references

- [User reference for `array_slice()`](../../../php/builtins/array/array_slice.md)

