---
title: "feof() — internals"
description: "Compiler internals for feof(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 160
---

## `feof()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/feof.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/feof.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3118](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3118) (`lower_feof`)
- **Function symbol**: `lower_feof()`


### Lowering notes

- Lowers `feof(stream)` through the runtime EOF-flag table helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_feof`
- `__rt_user_wrapper_ftell`

## Signature summary

```php
function feof(resource $stream): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/feof.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/feof.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `feof()`](../../../php/builtins/io/feof.md)
