---
title: "is_executable() — internals"
description: "Compiler internals for is_executable(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 121
---

## `is_executable()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:5632](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L5632) (`lower_is_executable`)
- **Function symbol**: `lower_is_executable()`


### Lowering notes

- Lowers `is_executable(path)` through the target-aware runtime access helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_is_executable`
- `__rt_is_link`
- `__rt_path_is_wrapper`
- `__rt_readfile`

## Signature summary

```php
function is_executable(string $filename): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `is_executable()`](../../../php/builtins/filesystem/is_executable.md)

