---
title: "filegroup() — internals"
description: "Compiler internals for filegroup(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 115
---

## `filegroup()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/filegroup.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/filegroup.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5500](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5500) (`lower_filegroup`)
- **Function symbol**: `lower_filegroup()`


### Lowering notes

- Lowers `filegroup(path)` and boxes the runtime integer-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_filegroup`
- `__rt_fileinode`
- `__rt_filetype`
- `__rt_stat_array`

## Signature summary

```php
function filegroup(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/filegroup.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/filegroup.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `filegroup()`](../../../php/builtins/filesystem/filegroup.md)
