---
title: "ftell() — internals"
description: "Compiler internals for ftell(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 172
---

## `ftell()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3133](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3133) (`lower_ftell`)
- **Function symbol**: `lower_ftell()`


### Lowering notes

- Lowers `ftell(stream)` as `lseek(fd, 0, SEEK_CUR)`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_user_wrapper_ftell`

## Signature summary

```php
function ftell(resource $stream): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `ftell()`](../../../php/builtins/io/ftell.md)

