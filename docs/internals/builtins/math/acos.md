---
title: "acos() — internals"
description: "Compiler internals for acos(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 253
---

## `acos()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/acos.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/acos.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/math/libm.rs`:22](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/math/libm.rs#L22) (`lower_unary_libm`)
- **Function symbol**: `lower_unary_libm()`


### Lowering notes

- Lowers a one-argument libm builtin such as `sin()`, `cos()`, or `exp()`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function acos(float $num): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/acos.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/acos.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `acos()`](../../../php/builtins/math/acos.md)
