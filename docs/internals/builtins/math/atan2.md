---
title: "atan2() — internals"
description: "Compiler internals for atan2(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 256
---

## `atan2()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/atan2.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/atan2.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/math/libm.rs`:35](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/math/libm.rs#L35) (`lower_atan2`)
- **Function symbol**: `lower_atan2()`


### Lowering notes

- Lowers `atan2()` using the C ABI argument order `y, x`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function atan2(float $y, float $x): float
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/atan2.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/atan2.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `atan2()`](../../../php/builtins/math/atan2.md)
