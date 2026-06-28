---
title: "fgets() — internals"
description: "Compiler internals for fgets(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 143
---

## `fgets()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2746](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2746) (`lower_fgets`)
- **Function symbol**: `lower_fgets()`


### Lowering notes

- Lowers `fgets(stream)` through the shared line-read runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fgetc`
- `__rt_fgets`

## Signature summary

```php
function fgets(resource $stream, int $length): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `fgets()`](../../../php/builtins/io/fgets.md)

