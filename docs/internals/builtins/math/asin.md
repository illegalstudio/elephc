---
title: "asin() — internals"
description: "Compiler internals for asin(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 237
---

## `asin()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/asin.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/asin.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math/libm.rs`:22](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math/libm.rs#L22) (`lower_unary_libm`)
- **Function symbol**: `lower_unary_libm()`


### Lowering notes

- Lowers a one-argument libm builtin such as `sin()`, `cos()`, or `exp()`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function asin(float $num): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `asin()`](../../../php/builtins/math/asin.md)
