---
title: "mkdir() — internals"
description: "Compiler internals for mkdir(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 138
---

## `mkdir()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/mkdir.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/mkdir.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4425](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4425) (`lower_mkdir`)
- **Function symbol**: `lower_mkdir()`


### Lowering notes

- Lowers `mkdir(path)` through the target-aware runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_chdir`
- `__rt_copy`
- `__rt_mkdir`
- `__rt_rmdir`
- `__rt_tempnam`

## Signature summary

```php
function mkdir(string $directory): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/mkdir.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/mkdir.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `mkdir()`](../../../php/builtins/filesystem/mkdir.md)
