---
title: "closedir() — internals"
description: "Compiler internals for closedir(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 151
---

## `closedir()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3572](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3572) (`lower_closedir`)
- **Function symbol**: `lower_closedir()`


### Lowering notes

- Lowers `closedir(dir_handle)` for libc, glob, and userspace-wrapper handles.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_closedir`
- `__rt_rewinddir`
- `__rt_user_wrapper_dir_closedir`
- `__rt_user_wrapper_dir_rewinddir`

## Signature summary

```php
function closedir(resource $dir_handle): void
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `closedir()`](../../../php/builtins/io/closedir.md)

