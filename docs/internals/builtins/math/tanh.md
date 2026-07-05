---
title: "tanh() — internals"
description: "Compiler internals for tanh(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 270
---

## `tanh()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/tanh.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/tanh.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math/libm.rs`:22](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math/libm.rs#L22) (`lower_unary_libm`)
- **Function symbol**: `lower_unary_libm()`


### Lowering notes

- Lowers a one-argument libm builtin such as `sin()`, `cos()`, or `exp()`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function tanh(float $num): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `tanh()`](../../../php/builtins/math/tanh.md)
