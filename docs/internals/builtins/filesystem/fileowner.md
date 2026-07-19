---
title: "fileowner() — internals"
description: "Compiler internals for fileowner(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 118
---

## `fileowner()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fileowner.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fileowner.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5492](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5492) (`lower_fileowner`)
- **Function symbol**: `lower_fileowner()`


### Lowering notes

- Lowers `fileowner(path)` and boxes the runtime integer-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_filegroup`
- `__rt_fileinode`
- `__rt_fileowner`

## Signature summary

```php
function fileowner(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/fileowner.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fileowner.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `fileowner()`](../../../php/builtins/filesystem/fileowner.md)
