---
title: "array_combine() — internals"
description: "Compiler internals for array_combine(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 3
---

## `array_combine()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:152](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L152) (`lower_array_combine`)
- **Function symbol**: `lower_array_combine()`


### Lowering notes

- Lowers `array_combine()` through the legacy hash-building runtime helpers.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_combine(array $keys, array $values): array
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `array_combine()`](../../../php/builtins/array/array_combine.md)

