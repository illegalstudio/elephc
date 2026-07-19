---
title: "lstat() — internals"
description: "Compiler internals for lstat(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 137
---

## `lstat()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/lstat.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/lstat.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5531](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5531) (`lower_lstat`)
- **Function symbol**: `lower_lstat()`


### Lowering notes

- Lowers `lstat(path)` and boxes the runtime lstat array or PHP false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fstat_array`
- `__rt_lstat_array`

## Signature summary

```php
function lstat(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/lstat.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/lstat.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `lstat()`](../../../php/builtins/filesystem/lstat.md)
