---
title: "mkdir() — internals"
description: "Compiler internals for mkdir(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 132
---

## `mkdir()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4428](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4428) (`lower_mkdir`)
- **Function symbol**: `lower_mkdir()`


### Lowering notes

- Lowers `mkdir(path)` through the target-aware runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_chdir`
- `__rt_copy`
- `__rt_mkdir`
- `__rt_rmdir`
- `__rt_tempnam`

## Signature summary

```php
function mkdir(string $directory, int $permissions, bool $recursive, bool $context): bool
```

## What the type checker enforces

- **Arity**: takes exactly 4 arguments.

## Cross-references

- [User reference for `mkdir()`](../../../php/builtins/filesystem/mkdir.md)

