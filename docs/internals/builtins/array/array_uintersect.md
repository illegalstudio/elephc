---
title: "array_uintersect() — internals"
description: "Compiler internals for array_uintersect(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 41
---

## `array_uintersect()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_uintersect.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_uintersect.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/arrays.rs`:1708](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/arrays.rs#L1708) (`lower_array_uintersect`)
- **Function symbol**: `lower_array_uintersect()`


### Lowering notes

- Lowers `array_uintersect()`: keeps first-array elements equal (per comparator) to some second-array element.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_uintersect(array $array1, array $array2, callable $callback): array
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

_Not callable from eval'd code — the magician interpreter has no entry for this builtin._

## Cross-references

- [User reference for `array_uintersect()`](../../../php/builtins/array/array_uintersect.md)
