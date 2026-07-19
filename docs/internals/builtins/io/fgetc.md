---
title: "fgetc() — internals"
description: "Compiler internals for fgetc(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 162
---

## `fgetc()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fgetc.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fgetc.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:2979](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L2979) (`lower_fgetc`)
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

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/fgetc.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fgetc.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `fgetc()`](../../../php/builtins/io/fgetc.md)
