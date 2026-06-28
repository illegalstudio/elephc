---
title: "max() — internals"
description: "Compiler internals for max(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 240
---

## `max()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math.rs`:204](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math.rs#L204) (`lower_min_max`)
- **Function symbol**: `lower_min_max()`


### Lowering notes

- Lowers numeric `min()` and `max()` over concrete integer-like or float operands.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function max(mixed $value, ...$values): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.
- **Variadic**: collects excess arguments into `$values`.

## Cross-references

- [User reference for `max()`](../../../php/builtins/math/max.md)

