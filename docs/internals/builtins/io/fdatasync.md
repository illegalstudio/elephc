---
title: "fdatasync() — internals"
description: "Compiler internals for fdatasync(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 138
---

## `fdatasync()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3302](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3302) (`lower_fdatasync`)
- **Function symbol**: `lower_fdatasync()`


### Lowering notes

- Lowers `fdatasync(stream)` through the shared fd data-sync runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fdatasync`

## Signature summary

```php
function fdatasync(resource $stream): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `fdatasync()`](../../../php/builtins/io/fdatasync.md)

