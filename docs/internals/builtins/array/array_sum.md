---
title: "array_sum() — internals"
description: "Compiler internals for array_sum(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 39
---

## `array_sum()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_sum.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_sum.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:51](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L51) (`lower_array_sum`)
- **Function symbol**: `lower_array_sum()`


### Lowering notes

- Lowers `array_sum()` over supported indexed-array payloads.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_product`
- `__rt_array_product_float`
- `__rt_array_sum`
- `__rt_array_sum_float`

## Signature summary

```php
function array_sum(array $array): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `array_sum()`](../../../php/builtins/array/array_sum.md)

