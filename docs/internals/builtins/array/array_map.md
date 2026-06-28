---
title: "array_map() — internals"
description: "Compiler internals for array_map(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 14
---

## `array_map()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:312](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L312) (`lower_array_map`)
- **Function symbol**: `lower_array_map()`


### Lowering notes

- Lowers `array_map()` through the callback runtime helper matching the callback result type.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_map(callable $callback, array $array, ...$arrays): array
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.
- **Variadic**: collects excess arguments into `$arrays`.

## Cross-references

- [User reference for `array_map()`](../../../php/builtins/array/array_map.md)

