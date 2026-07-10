---
title: "array_diff_key() — internals"
description: "Compiler internals for array_diff_key(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 8
---

## `array_diff_key()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_diff_key.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_diff_key.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/arrays.rs`:895](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/arrays.rs#L895) (`lower_array_diff_key`)
- **Function symbol**: `lower_array_diff_key()`


### Lowering notes

- Lowers `array_diff_key()` for two associative arrays by filtering first-operand keys.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_diff_key`
- `__rt_array_intersect_key`

## Signature summary

```php
function array_diff_key(array $array, ...$arrays): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$arrays`.

## Cross-references

- [User reference for `array_diff_key()`](../../../php/builtins/array/array_diff_key.md)
