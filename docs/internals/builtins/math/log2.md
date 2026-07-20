---
title: "log2() — internals"
description: "Compiler internals for log2(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 273
---

## `log2()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/log2.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/log2.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/math/libm.rs`:22](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/math/libm.rs#L22) (`lower_unary_libm`)
- **Function symbol**: `lower_unary_libm()`


### Lowering notes

- Lowers a one-argument libm builtin such as `sin()`, `cos()`, or `exp()`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function log2(float $num): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/log2.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/log2.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `log2()`](../../../php/builtins/math/log2.md)
