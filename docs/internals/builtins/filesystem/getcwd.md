---
title: "getcwd() — internals"
description: "Compiler internals for getcwd(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 123
---

## `getcwd()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/getcwd.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/getcwd.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5392](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5392) (`lower_getcwd`)
- **Function symbol**: `lower_getcwd()`


### Lowering notes

- Lowers `getcwd()` through the target-aware runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_getcwd`
- `__rt_tmpfile`

## Signature summary

```php
function getcwd(): string
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/getcwd.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/getcwd.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `getcwd()`](../../../php/builtins/filesystem/getcwd.md)
