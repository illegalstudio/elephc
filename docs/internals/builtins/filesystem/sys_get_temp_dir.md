---
title: "sys_get_temp_dir() — internals"
description: "Compiler internals for sys_get_temp_dir(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 130
---

## `sys_get_temp_dir()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4760](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4760) (`lower_sys_get_temp_dir`)
- **Function symbol**: `lower_sys_get_temp_dir()`


### Lowering notes

- Lowers `sys_get_temp_dir()` as the project's hardcoded `/tmp` string.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_tmpfile`

## Signature summary

```php
function sys_get_temp_dir(): string
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `sys_get_temp_dir()`](../../../php/builtins/filesystem/sys_get_temp_dir.md)

