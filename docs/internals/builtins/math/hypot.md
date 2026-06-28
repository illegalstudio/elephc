---
title: "hypot() — internals"
description: "Compiler internals for hypot(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 232
---

## `hypot()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math/libm.rs`:43](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math/libm.rs#L43) (`lower_hypot`)
- **Function symbol**: `lower_hypot()`


### Lowering notes

- Lowers `hypot()` using the C ABI argument order `x, y`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function hypot(float $x, float $y): float
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `hypot()`](../../../php/builtins/math/hypot.md)

