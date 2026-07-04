---
title: "array_slice() — internals"
description: "Compiler internals for array_slice(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 37
---

## `array_slice()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_slice.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_slice.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1028](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1028) (`lower_array_slice`)
- **Function symbol**: `lower_array_slice()`


### Lowering notes

- Lowers `array_slice()` for indexed arrays with pointer-sized payload slots.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_slice_preserve`

## Signature summary

```php
function array_slice(array $array, int $offset, int $length = null, bool $preserve_keys = false): array
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Cross-references

- [User reference for `array_slice()`](../../../php/builtins/array/array_slice.md)

