---
title: "opendir() — internals"
description: "Compiler internals for opendir(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 189
---

## `opendir()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/opendir.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/opendir.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3547](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3547) (`lower_opendir`)
- **Function symbol**: `lower_opendir()`


### Lowering notes

- Lowers `opendir(path)` and boxes the directory stream as `resource|false`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_opendir`
- `__rt_readdir`
- `__rt_user_wrapper_dir_readdir`

## Signature summary

```php
function opendir(string $directory): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/opendir.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/opendir.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `opendir()`](../../../php/builtins/io/opendir.md)
