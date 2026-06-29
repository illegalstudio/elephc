---
title: "is_numeric() — internals"
description: "Compiler internals for is_numeric(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 410
---

## `is_numeric()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/is_numeric.rs`:22](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/is_numeric.rs#L22) (`lower_is_numeric`)
- **Function symbol**: `lower_is_numeric()`


### Lowering notes

- Lowers `is_numeric()` for concrete scalar values.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function is_numeric(mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `is_numeric()`](../../../php/builtins/type/is_numeric.md)

