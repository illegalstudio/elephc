---
title: "array_sum() — internals"
description: "Compiler internals for array_sum(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 27
---

## `array_sum()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:51](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L51) (`lower_array_sum`)
- **Function symbol**: `lower_array_sum()`


### Lowering notes

- Lowers `array_sum()` over supported indexed-array payloads.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_product`
- `__rt_array_sum`

## Signature summary

```php
function array_sum(array $array): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `array_sum()`](../../../php/builtins/array/array_sum.md)

