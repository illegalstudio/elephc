---
title: "fileperms() — internals"
description: "Compiler internals for fileperms(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 119
---

## `fileperms()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fileperms.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fileperms.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5485](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5485) (`lower_fileperms`)
- **Function symbol**: `lower_fileperms()`


### Lowering notes

- Lowers `fileperms(path)` and boxes the runtime integer-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_filegroup`
- `__rt_fileinode`
- `__rt_fileowner`
- `__rt_fileperms`

## Signature summary

```php
function fileperms(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/fileperms.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fileperms.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `fileperms()`](../../../php/builtins/filesystem/fileperms.md)
