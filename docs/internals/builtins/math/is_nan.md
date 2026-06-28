---
title: "is_nan() — internals"
description: "Compiler internals for is_nan(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 236
---

## `is_nan()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math.rs`:113](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math.rs#L113) (`lower_is_nan`)
- **Function symbol**: `lower_is_nan()`


### Lowering notes

- Lowers `is_nan()` by checking whether the normalized float is unordered with itself.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function is_nan(float $num): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `is_nan()`](../../../php/builtins/math/is_nan.md)

