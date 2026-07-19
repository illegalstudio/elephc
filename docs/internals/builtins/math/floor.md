---
title: "floor() — internals"
description: "Compiler internals for floor(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 264
---

## `floor()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/floor.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/floor.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/math.rs`:70](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/math.rs#L70) (`lower_floor`)
- **Function symbol**: `lower_floor()`


### Lowering notes

- Lowers `floor()` for concrete integer-like and floating operands.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function floor(float $num): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/floor.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/floor.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `floor()`](../../../php/builtins/math/floor.md)
