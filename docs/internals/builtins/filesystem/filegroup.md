---
title: "filegroup() — internals"
description: "Compiler internals for filegroup(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 94
---

## `filegroup()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:5494](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L5494) (`lower_filegroup`)
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

## Cross-references

- [User reference for `filegroup()`](../../../php/builtins/filesystem/filegroup.md)

