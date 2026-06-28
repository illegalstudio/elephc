---
title: "abs() — internals"
description: "Compiler internals for abs(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 218
---

## `abs()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/math.rs`:43](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/math.rs#L43) (`lower_abs`)
- **Function symbol**: `lower_abs()`


### Lowering notes

- Lowers `abs()` for concrete integer-like and floating operands.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_abs_mixed`

## Signature summary

```php
function abs(int $num): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `abs()`](../../../php/builtins/math/abs.md)

