---
title: "array_key_last() — internals"
description: "Compiler internals for array_key_last(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 20
---

## `array_key_last()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1336](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1336) (`lower_array_key_last`)
- **Function symbol**: `lower_array_key_last()`


### Lowering notes

- Lowers `array_key_last()` through the shared edge-key helper with selector `1`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_edge_key`
- `__rt_mixed_from_value`

## Signature summary

```php
function array_key_last(array $array): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `array_key_last()`](../../../php/builtins/array/array_key_last.md)

