---
title: "stat() — internals"
description: "Compiler internals for stat(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 128
---

## `stat()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4886](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4886) (`lower_stat`)
- **Function symbol**: `lower_stat()`


### Lowering notes

- Lowers `stat(path)` and boxes the runtime stat array or PHP false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fstat_array`
- `__rt_lstat_array`
- `__rt_stat_array`

## Signature summary

```php
function stat(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `stat()`](../../../php/builtins/filesystem/stat.md)

