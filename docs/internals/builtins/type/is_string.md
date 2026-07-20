---
title: "is_string() — internals"
description: "Compiler internals for is_string(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 449
---

## `is_string()` — internals

## Where it lives

- **Signature**: [`src/builtins/types/is_string.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/types/is_string.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins.rs`:1354](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins.rs#L1354) (`lower_static_type_predicate`)
- **Function symbol**: `lower_static_type_predicate()`


### Lowering notes

- Lowers a static `is_*` predicate for concrete non-Mixed values.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function is_string(mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/types/is_string.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_string.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `is_string()`](../../../php/builtins/type/is_string.md)
