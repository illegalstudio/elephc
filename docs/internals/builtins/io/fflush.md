---
title: "fflush() — internals"
description: "Compiler internals for fflush(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 161
---

## `fflush()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fflush.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fflush.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3267](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3267) (`lower_fflush`)
- **Function symbol**: `lower_fflush()`


### Lowering notes

- Lowers `fflush(stream)` through the shared fd flush runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fflush`

## Signature summary

```php
function fflush(resource $stream): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/fflush.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fflush.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `fflush()`](../../../php/builtins/io/fflush.md)
