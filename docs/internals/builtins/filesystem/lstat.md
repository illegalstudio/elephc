---
title: "lstat() — internals"
description: "Compiler internals for lstat(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 116
---

## `lstat()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4891](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4891) (`lower_lstat`)
- **Function symbol**: `lower_lstat()`


### Lowering notes

- Lowers `lstat(path)` and boxes the runtime lstat array or PHP false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fstat_array`
- `__rt_lstat_array`

## Signature summary

```php
function lstat(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `lstat()`](../../../php/builtins/filesystem/lstat.md)

