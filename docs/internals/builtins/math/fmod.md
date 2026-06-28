---
title: "fmod() — internals"
description: "Compiler internals for fmod(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 231
---

## `fmod()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math/binary.rs`:85](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math/binary.rs#L85) (`lower_fmod`)
- **Function symbol**: `lower_fmod()`


### Lowering notes

- Lowers `fmod()` for concrete integer-like and floating operands.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function fmod(float $num1, float $num2): float
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `fmod()`](../../../php/builtins/math/fmod.md)

