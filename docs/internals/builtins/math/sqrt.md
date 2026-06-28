---
title: "sqrt() — internals"
description: "Compiler internals for sqrt(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 251
---

## `sqrt()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math.rs`:97](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math.rs#L97) (`lower_sqrt`)
- **Function symbol**: `lower_sqrt()`


### Lowering notes

- Lowers `sqrt()` for concrete integer-like and floating operands.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function sqrt(float $num): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `sqrt()`](../../../php/builtins/math/sqrt.md)

