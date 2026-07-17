---
title: "boolval() — internals"
description: "Compiler internals for boolval(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 415
---

## `boolval()` — internals

## Where it lives

- **Signature**: [`src/builtins/types/boolval.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/types/boolval.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins.rs`:1190](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins.rs#L1190) (`lower_boolval`)
- **Function symbol**: `lower_boolval()`


### Lowering notes

- Lowers `boolval()` using the same concrete scalar PHP truthiness rules as `IsTruthy`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mixed_cast_bool`

## Signature summary

```php
function boolval(mixed $value): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/types/boolval.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/types/boolval.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `boolval()`](../../../php/builtins/type/boolval.md)
