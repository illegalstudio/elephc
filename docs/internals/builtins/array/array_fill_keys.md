---
title: "array_fill_keys() — internals"
description: "Compiler internals for array_fill_keys(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 7
---

## `array_fill_keys()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:138](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L138) (`lower_array_fill_keys`)
- **Function symbol**: `lower_array_fill_keys()`


### Lowering notes

- Lowers `array_fill_keys()` through the legacy hash-building runtime helpers.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_fill_keys(array $keys, mixed $value): array
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `array_fill_keys()`](../../../php/builtins/array/array_fill_keys.md)

