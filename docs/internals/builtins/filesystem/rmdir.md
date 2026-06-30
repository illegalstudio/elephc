---
title: "rmdir() — internals"
description: "Compiler internals for rmdir(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 141
---

## `rmdir()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4433](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4433) (`lower_rmdir`)
- **Function symbol**: `lower_rmdir()`


### Lowering notes

- Lowers `rmdir(path)` through the target-aware runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_chdir`
- `__rt_copy`
- `__rt_rmdir`
- `__rt_scandir`
- `__rt_tempnam`

## Signature summary

```php
function rmdir(string $directory, mixed $context = null): bool
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `rmdir()`](../../../php/builtins/filesystem/rmdir.md)

