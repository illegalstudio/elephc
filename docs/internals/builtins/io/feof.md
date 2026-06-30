---
title: "feof() — internals"
description: "Compiler internals for feof(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 154
---

## `feof()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3121](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3121) (`lower_feof`)
- **Function symbol**: `lower_feof()`


### Lowering notes

- Lowers `feof(stream)` through the runtime EOF-flag table helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_feof`
- `__rt_user_wrapper_ftell`

## Signature summary

```php
function feof(resource $stream): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `feof()`](../../../php/builtins/io/feof.md)

