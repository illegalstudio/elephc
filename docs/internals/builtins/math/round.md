---
title: "round() — internals"
description: "Compiler internals for round(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 265
---

## `round()` — internals

## Where it lives

- **Signature**: [`src/builtins/math/round.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/math/round.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math.rs`:186](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math.rs#L186) (`lower_round`)
- **Function symbol**: `lower_round()`


### Lowering notes

- Lowers `round()` for concrete integer-like and floating operands.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function round(float $num, int $precision = 0, int $mode = 1): float
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).

## Cross-references

- [User reference for `round()`](../../../php/builtins/math/round.md)
