---
title: "fdiv() — internals"
description: "Compiler internals for fdiv(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 250
---

## `fdiv()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/fdiv.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/fdiv.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/math/binary.rs`:67](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/math/binary.rs#L67) (`lower_fdiv`)
- **Function symbol**: `lower_fdiv()`


### Lowering notes

- Lowers `fdiv()` for concrete integer-like and floating operands.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function fdiv(float $num1, float $num2): float
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/fdiv.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/fdiv.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `fdiv()`](../../../php/builtins/math/fdiv.md)
