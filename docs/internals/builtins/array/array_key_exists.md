---
title: "array_key_exists() — internals"
description: "Compiler internals for array_key_exists(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 18
---

## `array_key_exists()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_key_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_key_exists.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/arrays/key_exists.rs`:22](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/arrays/key_exists.rs#L22) (`lower_array_key_exists`)
- **Function symbol**: `lower_array_key_exists()`


### Lowering notes

- Lowers `array_key_exists()` for indexed arrays and associative arrays.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_hash_get`

## Signature summary

```php
function array_key_exists(string $key, array $array): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/array/array_key_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_key_exists.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `array_key_exists()`](../../../php/builtins/array/array_key_exists.md)
