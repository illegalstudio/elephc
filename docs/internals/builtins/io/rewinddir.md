---
title: "rewinddir() — internals"
description: "Compiler internals for rewinddir(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 186
---

## `rewinddir()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3588](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3588) (`lower_rewinddir`)
- **Function symbol**: `lower_rewinddir()`


### Lowering notes

- Lowers `rewinddir(dir_handle)` for libc, glob, and userspace-wrapper handles.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_rewinddir`
- `__rt_user_wrapper_dir_rewinddir`

## Signature summary

```php
function rewinddir(resource $dir_handle): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `rewinddir()`](../../../php/builtins/io/rewinddir.md)

