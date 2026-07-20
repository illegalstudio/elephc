---
title: "deg2rad() — internals"
description: "Compiler internals for deg2rad(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 261
---

## `deg2rad()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/deg2rad.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/deg2rad.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/math/libm.rs`:75](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/math/libm.rs#L75) (`lower_deg2rad`)
- **Function symbol**: `lower_deg2rad()`


### Lowering notes

- Lowers `deg2rad()` by multiplying with `PI / 180`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function deg2rad(float $num): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/deg2rad.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/deg2rad.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `deg2rad()`](../../../php/builtins/math/deg2rad.md)
