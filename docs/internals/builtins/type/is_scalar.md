---
title: "is_scalar() — internals"
description: "Compiler internals for is_scalar(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 428
---

## `is_scalar()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins.rs`:1553](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins.rs#L1553) (`lower_is_scalar`)
- **Function symbol**: `lower_is_scalar()`


### Lowering notes

- Lowers `is_scalar()`: true for int/float/string/bool, a non-null tagged scalar, or a boxed
- Mixed/Union value whose runtime tag is int (0), string (1), float (2), or bool (3). Null,
- arrays, objects, and resources are not scalars, matching PHP.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function is_scalar(mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `is_scalar()`](../../../php/builtins/type/is_scalar.md)

