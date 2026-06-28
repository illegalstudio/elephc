---
title: "pow() — internals"
description: "Compiler internals for pow(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 244
---

## `pow()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math/binary.rs`:114](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math/binary.rs#L114) (`lower_pow`)
- **Function symbol**: `lower_pow()`


### Lowering notes

- Lowers `pow()` for concrete integer-like and floating operands.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function pow(float $num, float $exponent): float
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `pow()`](../../../php/builtins/math/pow.md)

