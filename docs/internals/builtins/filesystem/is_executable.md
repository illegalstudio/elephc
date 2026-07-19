---
title: "is_executable() — internals"
description: "Compiler internals for is_executable(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 127
---

## `is_executable()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/is_executable.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/is_executable.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5633](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5633) (`lower_is_executable`)
- **Function symbol**: `lower_is_executable()`


### Lowering notes

- Lowers `is_executable(path)` through the target-aware runtime access helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_is_executable`
- `__rt_is_link`
- `__rt_path_is_wrapper`
- `__rt_readfile`

## Signature summary

```php
function is_executable(string $filename): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/is_executable.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/is_executable.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `is_executable()`](../../../php/builtins/filesystem/is_executable.md)
