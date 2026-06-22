---
title: "fgetc() — internals"
description: "Compiler internals for fgetc(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 141
---

## `fgetc()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2978](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2978) (`lower_fgetc`)
- **Function symbol**: `lower_fgetc()`


### Lowering notes

- Lowers `fgetc(stream)` and boxes the one-byte string or PHP false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fgetc`
- `__rt_fgetcsv`

## Signature summary

```php
function fgetc(resource $stream): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `fgetc()`](../../../php/builtins/io/fgetc.md)

