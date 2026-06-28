---
title: "array_reduce() — internals"
description: "Compiler internals for array_reduce(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 21
---

## `array_reduce()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:701](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L701) (`lower_array_reduce`)
- **Function symbol**: `lower_array_reduce()`


### Lowering notes

- Lowers `array_reduce()` through the callback-driven runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_reduce`

## Signature summary

```php
function array_reduce(array $array, callable $callback, mixed $initial): int
```

## What the type checker enforces

- **Arity**: takes 2–3 arguments (1 optional).

## Cross-references

- [User reference for `array_reduce()`](../../../php/builtins/array/array_reduce.md)

