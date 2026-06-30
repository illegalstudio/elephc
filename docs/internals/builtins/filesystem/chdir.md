---
title: "chdir() — internals"
description: "Compiler internals for chdir(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 97
---

## `chdir()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4438](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4438) (`lower_chdir`)
- **Function symbol**: `lower_chdir()`


### Lowering notes

- Lowers `chdir(path)` through the target-aware runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_chdir`
- `__rt_copy`
- `__rt_glob`
- `__rt_scandir`
- `__rt_tempnam`

## Signature summary

```php
function chdir(string $directory): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `chdir()`](../../../php/builtins/filesystem/chdir.md)

