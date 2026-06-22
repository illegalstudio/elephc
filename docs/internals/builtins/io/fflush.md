---
title: "fflush() — internals"
description: "Compiler internals for fflush(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 140
---

## `fflush()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3264](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3264) (`lower_fflush`)
- **Function symbol**: `lower_fflush()`


### Lowering notes

- Lowers `fflush(stream)` through the shared fd flush runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fflush`

## Signature summary

```php
function fflush(resource $stream): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `fflush()`](../../../php/builtins/io/fflush.md)

