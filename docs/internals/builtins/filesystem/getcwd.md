---
title: "getcwd() — internals"
description: "Compiler internals for getcwd(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 102
---

## `getcwd()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4753](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4753) (`lower_getcwd`)
- **Function symbol**: `lower_getcwd()`


### Lowering notes

- Lowers `getcwd()` through the target-aware runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_getcwd`
- `__rt_tmpfile`

## Signature summary

```php
function getcwd(): string
```

## What the type checker enforces

- **Arity**: takes no arguments.

## Cross-references

- [User reference for `getcwd()`](../../../php/builtins/filesystem/getcwd.md)

