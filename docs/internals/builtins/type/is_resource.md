---
title: "is_resource() — internals"
description: "Compiler internals for is_resource(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 433
---

## `is_resource()` — internals

## Where it lives

- **Signature**: [`src/builtins/types/is_resource.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/types/is_resource.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/types.rs`:407](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/types.rs#L407) (`lower_is_resource`)
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

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/types/is_resource.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_resource.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `is_resource()`](../../../php/builtins/type/is_resource.md)
