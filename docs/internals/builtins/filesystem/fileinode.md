---
title: "fileinode() — internals"
description: "Compiler internals for fileinode(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 116
---

## `fileinode()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/fileinode.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/fileinode.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5508](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5508) (`lower_fileinode`)
- **Function symbol**: `lower_fileinode()`


### Lowering notes

- Lowers `fileinode(path)` and boxes the runtime integer-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fileinode`
- `__rt_filetype`
- `__rt_lstat_array`
- `__rt_stat_array`

## Signature summary

```php
function fileinode(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/fileinode.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/fileinode.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `fileinode()`](../../../php/builtins/filesystem/fileinode.md)
