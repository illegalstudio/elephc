---
title: "is_object() — internals"
description: "Compiler internals for is_object(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 411
---

## `is_object()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:1522](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L1522) (`lower_is_object`)
- **Function symbol**: `lower_is_object()`


### Lowering notes

- Lowers `is_object()`: true for statically-known objects, or a boxed Mixed/Union value whose
- runtime tag is an object (6).

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function is_object(mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `is_object()`](../../../php/builtins/type/is_object.md)

