---
title: "fileperms() — internals"
description: "Compiler internals for fileperms(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 98
---

## `fileperms()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:5478](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L5478) (`lower_fileperms`)
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

## Cross-references

- [User reference for `fileperms()`](../../../php/builtins/filesystem/fileperms.md)

