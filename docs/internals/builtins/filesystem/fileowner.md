---
title: "fileowner() — internals"
description: "Compiler internals for fileowner(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 97
---

## `fileowner()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4849](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4849) (`lower_fileowner`)
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

## Cross-references

- [User reference for `fileowner()`](../../../php/builtins/filesystem/fileowner.md)

