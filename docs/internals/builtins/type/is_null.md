---
title: "is_null() — internals"
description: "Compiler internals for is_null(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 444
---

## `is_null()` — internals

## Where it lives

- **Signature**: [`src/builtins/types/is_null.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/types/is_null.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins.rs`:1611](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins.rs#L1611) (`lower_is_null_builtin`)
- **Function symbol**: `lower_is_null_builtin()`


### Lowering notes

- Lowers `is_null()` for concrete scalar values and boxed Mixed payloads.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function is_null(mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/types/is_null.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_null.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `is_null()`](../../../php/builtins/type/is_null.md)
