---
title: "array_walk() — internals"
description: "Compiler internals for array_walk(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 31
---

## `array_walk()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:783](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L783) (`lower_array_walk`)
- **Function symbol**: `lower_array_walk()`


### Lowering notes

- Lowers `array_walk()` through the callback-driven runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_array_walk`

## Signature summary

```php
function array_walk(array $array, callable $callback, mixed $arg): void
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `array_walk()`](../../../php/builtins/array/array_walk.md)

