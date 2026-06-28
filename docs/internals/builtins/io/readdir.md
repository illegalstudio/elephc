---
title: "readdir() — internals"
description: "Compiler internals for readdir(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 169
---

## `readdir()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3336](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3336) (`lower_readdir`)
- **Function symbol**: `lower_readdir()`


### Lowering notes

- Lowers `readdir(dir_handle)` for libc, glob, and userspace-wrapper handles.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_closedir`
- `__rt_readdir`
- `__rt_user_wrapper_dir_closedir`
- `__rt_user_wrapper_dir_readdir`

## Signature summary

```php
function readdir(resource $dir_handle): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `readdir()`](../../../php/builtins/io/readdir.md)

