---
title: "array_product() — internals"
description: "Compiler internals for array_product(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 28
---

## `array_product()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_product.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_product.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/arrays.rs`:55](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/arrays.rs#L55) (`lower_array_product`)
- **Function symbol**: `lower_array_product()`


### Lowering notes

- Lowers `array_product()` over supported indexed-array payloads.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_product`

## Signature summary

```php
function array_product(array $array): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/array/array_product.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_product.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `array_product()`](../../../php/builtins/array/array_product.md)
