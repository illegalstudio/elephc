---
title: "fdatasync() — internals"
description: "Compiler internals for fdatasync(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 159
---

## `fdatasync()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fdatasync.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fdatasync.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3301](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3301) (`lower_fdatasync`)
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

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/fdatasync.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fdatasync.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `fdatasync()`](../../../php/builtins/io/fdatasync.md)
