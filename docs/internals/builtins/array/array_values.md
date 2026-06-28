---
title: "array_values() — internals"
description: "Compiler internals for array_values(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 30
---

## `array_values()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays/values.rs`:22](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays/values.rs#L22) (`lower_array_values`)
- **Function symbol**: `lower_array_values()`


### Lowering notes

- Lowers `array_values()` for indexed arrays as an alias or associative arrays as a new values array.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_values(array $array): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `array_values()`](../../../php/builtins/array/array_values.md)

