---
title: "array_unique() — internals"
description: "Compiler internals for array_unique(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 28
---

## `array_unique()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:198](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L198) (`lower_array_unique`)
- **Function symbol**: `lower_array_unique()`


### Lowering notes

- Lowers `array_unique()` for indexed arrays with 8-byte payload slots.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_filter`
- `__rt_array_filter_refcounted`

## Signature summary

```php
function array_unique(array $array, int $flags): array
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `array_unique()`](../../../php/builtins/array/array_unique.md)

