---
title: "array_flip() — internals"
description: "Compiler internals for array_flip(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 9
---

## `array_flip()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/arrays.rs`:171](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/arrays.rs#L171) (`lower_array_flip`)
- **Function symbol**: `lower_array_flip()`


### Lowering notes

- Lowers `array_flip()` through the legacy hash-building runtime helpers.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function array_flip(array $array): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `array_flip()`](../../../php/builtins/array/array_flip.md)

