---
title: "array_rand() — internals"
description: "Compiler internals for array_rand(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 20
---

## `array_rand()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1007](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1007) (`lower_array_rand`)
- **Function symbol**: `lower_array_rand()`


### Lowering notes

- Lowers `array_rand()` for indexed arrays.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_rand`
- `__rt_mixed_cast_int`

## Signature summary

```php
function array_rand(array $array, int $num): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `array_rand()`](../../../php/builtins/array/array_rand.md)

