---
title: "array_merge() — internals"
description: "Compiler internals for array_merge(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 15
---

## `array_merge()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:850](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L850) (`lower_array_merge`)
- **Function symbol**: `lower_array_merge()`


### Lowering notes

- Lowers `array_merge()` for two compatible indexed arrays with 8-byte payload slots.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_diff`
- `__rt_array_diff_refcounted`

## Signature summary

```php
function array_merge(...$arrays): array
```

## What the type checker enforces

- **Arity**: takes no arguments.
- **Variadic**: collects excess arguments into `$arrays`.

## Cross-references

- [User reference for `array_merge()`](../../../php/builtins/array/array_merge.md)

