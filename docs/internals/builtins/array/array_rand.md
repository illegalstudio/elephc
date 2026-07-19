---
title: "array_rand() — internals"
description: "Compiler internals for array_rand(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 30
---

## `array_rand()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_rand.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_rand.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/arrays.rs`:1010](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/arrays.rs#L1010) (`lower_array_rand`)
- **Function symbol**: `lower_array_rand()`


### Lowering notes

- Lowers `array_rand()` for indexed arrays.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_rand`
- `__rt_mixed_cast_int`

## Signature summary

```php
function array_rand(array $array): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/array/array_rand.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_rand.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `array_rand()`](../../../php/builtins/array/array_rand.md)
