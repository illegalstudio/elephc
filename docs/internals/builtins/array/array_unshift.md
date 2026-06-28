---
title: "array_unshift() — internals"
description: "Compiler internals for array_unshift(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 29
---

## `array_unshift()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays/unshift.rs`:23](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays/unshift.rs#L23) (`lower_array_unshift`)
- **Function symbol**: `lower_array_unshift()`


### Lowering notes

- Lowers `array_unshift()` by ensuring uniqueness, prepending one scalar value, and returning count.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_unshift(array $array, ...$values): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **By-reference parameters**: `$array`.
- **Variadic**: collects excess arguments into `$values`.

## Cross-references

- [User reference for `array_unshift()`](../../../php/builtins/array/array_unshift.md)

