---
title: "sinh() — internals"
description: "Compiler internals for sinh(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 267
---

## `sinh()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/sinh.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/sinh.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math/libm.rs`:22](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math/libm.rs#L22) (`lower_unary_libm`)
- **Function symbol**: `lower_unary_libm()`


### Lowering notes

- Lowers a one-argument libm builtin such as `sin()`, `cos()`, or `exp()`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function sinh(float $num): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `sinh()`](../../../php/builtins/math/sinh.md)

