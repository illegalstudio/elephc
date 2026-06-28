---
title: "fileinode() — internals"
description: "Compiler internals for fileinode(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 95
---

## `fileinode()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4865](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4865) (`lower_fileinode`)
- **Function symbol**: `lower_fileinode()`


### Lowering notes

- Lowers `fileinode(path)` and boxes the runtime integer-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fileinode`
- `__rt_filetype`
- `__rt_lstat_array`
- `__rt_stat_array`

## Signature summary

```php
function fileinode(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `fileinode()`](../../../php/builtins/filesystem/fileinode.md)

