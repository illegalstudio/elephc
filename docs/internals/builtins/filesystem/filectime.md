---
title: "filectime() — internals"
description: "Compiler internals for filectime(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 93
---

## `filectime()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4833](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4833) (`lower_filectime`)
- **Function symbol**: `lower_filectime()`


### Lowering notes

- Lowers `filectime(path)` and boxes the runtime integer-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_filectime`
- `__rt_filegroup`
- `__rt_fileowner`
- `__rt_fileperms`

## Signature summary

```php
function filectime(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `filectime()`](../../../php/builtins/filesystem/filectime.md)

