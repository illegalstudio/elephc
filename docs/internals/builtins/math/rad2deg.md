---
title: "rad2deg() — internals"
description: "Compiler internals for rad2deg(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 245
---

## `rad2deg()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math/libm.rs`:83](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math/libm.rs#L83) (`lower_rad2deg`)
- **Function symbol**: `lower_rad2deg()`


### Lowering notes

- Lowers `rad2deg()` by multiplying with `180 / PI`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function rad2deg(float $num): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `rad2deg()`](../../../php/builtins/math/rad2deg.md)

