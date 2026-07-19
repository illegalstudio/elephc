---
title: "array_reduce() — internals"
description: "Compiler internals for array_reduce(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 31
---

## `array_reduce()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_reduce.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_reduce.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/arrays.rs`:695](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/arrays.rs#L695) (`lower_array_reduce`)
- **Function symbol**: `lower_array_reduce()`


### Lowering notes

- Lowers `array_reduce()` through the callback-driven runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_reduce`

## Signature summary

```php
function array_reduce(array $array, callable $callback, mixed $initial = null): int
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/array/array_reduce.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_reduce.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `array_reduce()`](../../../php/builtins/array/array_reduce.md)
