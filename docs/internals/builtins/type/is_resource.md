---
title: "is_resource() — internals"
description: "Compiler internals for is_resource(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 410
---

## `is_resource()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/types.rs`:400](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/types.rs#L400) (`lower_is_resource`)
- **Function symbol**: `lower_is_resource()`


### Lowering notes

- Lowers `is_resource(value)` for static resources and boxed Mixed resource cells.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function is_resource(mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `is_resource()`](../../../php/builtins/type/is_resource.md)

