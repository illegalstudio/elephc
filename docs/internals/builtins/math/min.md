---
title: "min() — internals"
description: "Compiler internals for min(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 262
---

## `min()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/min.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/min.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/math.rs`:204](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/math.rs#L204) (`lower_min_max`)
- **Function symbol**: `lower_min_max()`


### Lowering notes

- Lowers numeric `min()` and `max()` over concrete integer-like or float operands.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function min(mixed $value, ...$values): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$values`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/math/min.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/math/min.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`
- **Variadic**: collects excess arguments into `$values`.

## Cross-references

- [User reference for `min()`](../../../php/builtins/math/min.md)
