---
title: "array_udiff() — internals"
description: "Compiler internals for array_udiff(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 40
---

## `array_udiff()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:1834](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L1834) (`lower_array_udiff`)
- **Function symbol**: `lower_array_udiff()`


### Lowering notes

- Lowers `array_udiff()`: keeps first-array elements not equal (per comparator) to any second-array element.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_udiff(array $array1, array $array2, callable $callback): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Cross-references

- [User reference for `array_udiff()`](../../../php/builtins/array/array_udiff.md)

