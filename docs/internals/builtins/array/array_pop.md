---
title: "array_pop() — internals"
description: "Compiler internals for array_pop(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 17
---

## `array_pop()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1048](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1048) (`lower_array_pop`)
- **Function symbol**: `lower_array_pop()`


### Lowering notes

- Lowers `array_pop()` for indexed arrays by mutating length and boxing `T|null` as Mixed.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_sort_int`
- `__rt_sort_str`

## Signature summary

```php
function array_pop(array $array): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `array_pop()`](../../../php/builtins/array/array_pop.md)

