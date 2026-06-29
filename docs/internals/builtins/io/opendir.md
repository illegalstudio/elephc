---
title: "opendir() — internals"
description: "Compiler internals for opendir(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 168
---

## `opendir()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3545](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3545) (`lower_opendir`)
- **Function symbol**: `lower_opendir()`


### Lowering notes

- Lowers `opendir(path)` and boxes the directory stream as `resource|false`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_opendir`
- `__rt_readdir`
- `__rt_user_wrapper_dir_readdir`

## Signature summary

```php
function opendir(string $directory): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `opendir()`](../../../php/builtins/io/opendir.md)

