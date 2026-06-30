---
title: "array_replace_recursive() — internals"
description: "Compiler internals for array_replace_recursive(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 33
---

## `array_replace_recursive()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1509](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1509) (`lower_array_replace_recursive`)
- **Function symbol**: `lower_array_replace_recursive()`


### Lowering notes

- Lowers `array_replace_recursive()` (recursive right-wins hash merge).

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_replace_recursive`
- `__rt_assoc_diff_intersect`

## Signature summary

```php
function array_replace_recursive(array $array, array $replacements): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `array_replace_recursive()`](../../../php/builtins/array/array_replace_recursive.md)

