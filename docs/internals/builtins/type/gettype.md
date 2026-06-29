---
title: "gettype() — internals"
description: "Compiler internals for gettype(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 401
---

## `gettype()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:639](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L639) (`lower_gettype`)
- **Function symbol**: `lower_gettype()`


### Lowering notes

- Lowers `gettype(value)` for statically concrete PHP types.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function gettype(mixed $value): string
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `gettype()`](../../../php/builtins/type/gettype.md)

