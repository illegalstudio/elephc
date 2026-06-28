---
title: "array_intersect_key() — internals"
description: "Compiler internals for array_intersect_key(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 11
---

## `array_intersect_key()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:901](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L901) (`lower_array_intersect_key`)
- **Function symbol**: `lower_array_intersect_key()`


### Lowering notes

- Lowers `array_intersect_key()` for two associative arrays by keeping shared first-operand keys.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_intersect_key`

## Signature summary

```php
function array_intersect_key(array $array, ...$arrays): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$arrays`.

## Cross-references

- [User reference for `array_intersect_key()`](../../../php/builtins/array/array_intersect_key.md)

