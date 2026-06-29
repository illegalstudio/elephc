---
title: "boolval() — internals"
description: "Compiler internals for boolval(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 393
---

## `boolval()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:1091](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L1091) (`lower_boolval`)
- **Function symbol**: `lower_boolval()`


### Lowering notes

- Lowers `boolval()` using the same concrete scalar PHP truthiness rules as `IsTruthy`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function boolval(mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `boolval()`](../../../php/builtins/type/boolval.md)

