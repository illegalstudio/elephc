---
title: "fmod() — internals"
description: "Compiler internals for fmod(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 265
---

## `fmod()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/fmod.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/fmod.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/math/binary.rs`:92](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/math/binary.rs#L92) (`lower_fmod`)
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

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/fmod.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/fmod.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `fmod()`](../../../php/builtins/math/fmod.md)
