---
title: "array_fill() — internals"
description: "Compiler internals for array_fill(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 6
---

## `array_fill()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:115](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L115) (`lower_array_fill`)
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

## Cross-references

- [User reference for `array_fill()`](../../../php/builtins/array/array_fill.md)

