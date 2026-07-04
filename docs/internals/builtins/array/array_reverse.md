---
title: "array_reverse() — internals"
description: "Compiler internals for array_reverse(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 34
---

## `array_reverse()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_reverse.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_reverse.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:185](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L185) (`lower_array_reverse`)
- **Function symbol**: `lower_array_reverse()`


### Lowering notes

- Lowers `array_reverse()` for indexed arrays with 8-byte payload slots.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_reverse(array $array): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `array_reverse()`](../../../php/builtins/array/array_reverse.md)

