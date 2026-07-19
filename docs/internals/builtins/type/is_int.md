---
title: "is_int() — internals"
description: "Compiler internals for is_int(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 442
---

## `is_int()` — internals

## Where it lives

- **Signature**: [`src/builtins/types/is_int.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/types/is_int.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins.rs`:1351](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins.rs#L1351) (`lower_static_type_predicate`)
- **Function symbol**: `lower_static_type_predicate()`


### Lowering notes

- Lowers a static `is_*` predicate for concrete non-Mixed values.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function is_int(mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/types/is_int.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/is_int.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `is_int()`](../../../php/builtins/type/is_int.md)
