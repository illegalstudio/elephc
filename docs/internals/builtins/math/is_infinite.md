---
title: "is_infinite() — internals"
description: "Compiler internals for is_infinite(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 235
---

## `is_infinite()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math.rs`:132](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math.rs#L132) (`lower_is_infinite`)
- **Function symbol**: `lower_is_infinite()`


### Lowering notes

- Lowers `is_infinite()` by comparing the normalized float against +/- infinity.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function is_infinite(float $num): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `is_infinite()`](../../../php/builtins/math/is_infinite.md)

