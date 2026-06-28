---
title: "fdiv() — internals"
description: "Compiler internals for fdiv(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 229
---

## `fdiv()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math/binary.rs`:60](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math/binary.rs#L60) (`lower_fdiv`)
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

## Cross-references

- [User reference for `fdiv()`](../../../php/builtins/math/fdiv.md)

