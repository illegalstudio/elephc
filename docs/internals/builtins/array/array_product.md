---
title: "array_product() — internals"
description: "Compiler internals for array_product(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 18
---

## `array_product()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:56](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L56) (`lower_array_product`)
- **Function symbol**: `lower_array_product()`


### Lowering notes

- Lowers `array_product()` over supported indexed-array payloads.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_product`

## Signature summary

```php
function array_product(array $array): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `array_product()`](../../../php/builtins/array/array_product.md)

