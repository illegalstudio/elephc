---
title: "array_column() — internals"
description: "Compiler internals for array_column(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 2
---

## `array_column()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays/column.rs`:23](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays/column.rs#L23) (`lower_array_column`)
- **Function symbol**: `lower_array_column()`


### Lowering notes

- Lowers `array_column()` by dispatching to the helper matching row value ownership.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_column(array $array, string $column_key, string $index_key): array
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Cross-references

- [User reference for `array_column()`](../../../php/builtins/array/array_column.md)

