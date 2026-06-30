---
title: "filetype() — internals"
description: "Compiler internals for filetype(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 115
---

## `filetype()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:5516](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L5516) (`lower_filetype`)
- **Function symbol**: `lower_filetype()`


### Lowering notes

- Lowers `filetype(path)` and boxes the runtime string-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_filetype`
- `__rt_lstat_array`
- `__rt_stat_array`

## Signature summary

```php
function filetype(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `filetype()`](../../../php/builtins/filesystem/filetype.md)

