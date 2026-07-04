---
title: "min() — internals"
description: "Compiler internals for min(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 258
---

## `min()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/min.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/min.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math.rs`:228](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math.rs#L228) (`lower_min_max`)
- **Function symbol**: `lower_min_max()`


### Lowering notes

- Lowers numeric `min()` and `max()` over concrete integer-like or float operands.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mixed_cast_float`

## Signature summary

```php
function min(mixed $value, ...$values): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$values`.

## Cross-references

- [User reference for `min()`](../../../php/builtins/math/min.md)

