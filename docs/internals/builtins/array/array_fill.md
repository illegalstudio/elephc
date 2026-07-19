---
title: "array_fill() — internals"
description: "Compiler internals for array_fill(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 9
---

## `array_fill()` — internals

## Where it lives

- **Signature**: [`src/builtins/array/array_fill.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/array/array_fill.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/arrays.rs`:116](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/arrays.rs#L116) (`lower_array_fill`)
- **Function symbol**: `lower_array_fill()`


### Lowering notes

- Lowers `array_fill()` for pointer-sized scalar and refcounted payloads.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_fill(int $start_index, int $count, mixed $value): array
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/array/array_fill.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/array/array_fill.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `array_fill()`](../../../php/builtins/array/array_fill.md)
