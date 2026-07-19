---
title: "filetype() — internals"
description: "Compiler internals for filetype(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 121
---

## `filetype()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/filetype.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/filetype.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5513](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5513) (`lower_filetype`)
- **Function symbol**: `lower_filetype()`


### Lowering notes

- Lowers `filetype(path)` and boxes the runtime string-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_filetype`
- `__rt_lstat_array`
- `__rt_stat_array`

## Signature summary

```php
function filetype(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/filetype.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/filetype.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `filetype()`](../../../php/builtins/filesystem/filetype.md)
