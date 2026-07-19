---
title: "fstat() — internals"
description: "Compiler internals for fstat(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 176
---

## `fstat()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fstat.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fstat.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5540](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5540) (`lower_fstat`)
- **Function symbol**: `lower_fstat()`


### Lowering notes

- Lowers `fstat(stream)` and boxes the runtime stat array or PHP false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fstat_array`

## Signature summary

```php
function fstat(resource $stream): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/fstat.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fstat.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `fstat()`](../../../php/builtins/io/fstat.md)
