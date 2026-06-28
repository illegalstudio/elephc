---
title: "array_splice() — internals"
description: "Compiler internals for array_splice(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 26
---

## `array_splice()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:949](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L949) (`lower_array_splice`)
- **Function symbol**: `lower_array_splice()`


### Lowering notes

- Lowers `array_splice()` by mutating an indexed source array and returning removed elements.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_splice(array $array, int $offset, int $length, array $replacement): array
```

## What the type checker enforces

- **Arity**: takes 3–4 arguments (1 optional).
- **By-reference parameters**: `$array`.

## Cross-references

- [User reference for `array_splice()`](../../../php/builtins/array/array_splice.md)

