---
title: "readlink() — internals"
description: "Compiler internals for readlink(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 121
---

## `readlink()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:5452](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L5452) (`lower_readlink`)
- **Function symbol**: `lower_readlink()`


### Lowering notes

- Lowers `readlink(path)` and boxes the owned runtime string-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fileatime`
- `__rt_filectime`
- `__rt_fileperms`
- `__rt_readlink`

## Signature summary

```php
function readlink(string $path): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `readlink()`](../../../php/builtins/filesystem/readlink.md)

