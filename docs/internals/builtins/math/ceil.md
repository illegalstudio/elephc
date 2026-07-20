---
title: "ceil() — internals"
description: "Compiler internals for ceil(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 257
---

## `ceil()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/ceil.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/ceil.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/math.rs`:75](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/math.rs#L75) (`lower_ceil`)
- **Function symbol**: `lower_ceil()`


### Lowering notes

- Lowers `ceil()` for concrete integer-like and floating operands.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ceil(float $num): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/ceil.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/ceil.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ceil()`](../../../php/builtins/math/ceil.md)
