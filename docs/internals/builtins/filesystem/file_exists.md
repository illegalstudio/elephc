---
title: "file_exists() — internals"
description: "Compiler internals for file_exists(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 112
---

## `file_exists()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/file_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/file_exists.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4395](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4395) (`lower_file_exists`)
- **Function symbol**: `lower_file_exists()`


### Lowering notes

- Lowers `file_exists(path)` through the target-aware runtime stat helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mkdir`
- `__rt_unlink`

## Signature summary

```php
function file_exists(string $filename): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/file_exists.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/file_exists.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `file_exists()`](../../../php/builtins/filesystem/file_exists.md)
