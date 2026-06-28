---
title: "round() — internals"
description: "Compiler internals for round(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 248
---

## `round()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math.rs`:186](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math.rs#L186) (`lower_round`)
- **Function symbol**: `lower_round()`


### Lowering notes

- Lowers `round()` for concrete integer-like and floating operands.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function round(float $num, int $precision): float
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `round()`](../../../php/builtins/math/round.md)

