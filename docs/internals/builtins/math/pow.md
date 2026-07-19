---
title: "pow() — internals"
description: "Compiler internals for pow(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 278
---

## `pow()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/pow.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/pow.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/math/binary.rs`:120](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/math/binary.rs#L120) (`lower_pow`)
- **Function symbol**: `lower_pow()`


### Lowering notes

- Lowers `pow()` for concrete integer-like and floating operands.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function pow(float $num, float $exponent): float
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/pow.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/pow.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `pow()`](../../../php/builtins/math/pow.md)
