---
title: "fsync() — internals"
description: "Compiler internals for fsync(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 156
---

## `fsync()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3040](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3040) (`lower_fsync`)
- **Function symbol**: `lower_fsync()`


### Lowering notes

- Lowers `fsync(stream)` through the shared fd sync runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fflush`
- `__rt_fsync`

## Signature summary

```php
function fsync(resource $stream): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `fsync()`](../../../php/builtins/io/fsync.md)

