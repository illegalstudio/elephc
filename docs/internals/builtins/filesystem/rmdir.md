---
title: "rmdir() — internals"
description: "Compiler internals for rmdir(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 147
---

## `rmdir()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/rmdir.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/rmdir.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4430](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4430) (`lower_rmdir`)
- **Function symbol**: `lower_rmdir()`


### Lowering notes

- Lowers `rmdir(path)` through the target-aware runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_chdir`
- `__rt_copy`
- `__rt_rmdir`
- `__rt_scandir`
- `__rt_tempnam`

## Signature summary

```php
function rmdir(string $directory): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/rmdir.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/rmdir.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `rmdir()`](../../../php/builtins/filesystem/rmdir.md)
