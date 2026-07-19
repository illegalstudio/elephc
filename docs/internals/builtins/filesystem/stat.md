---
title: "stat() — internals"
description: "Compiler internals for stat(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 149
---

## `stat()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stat.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stat.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5525](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5525) (`lower_stat`)
- **Function symbol**: `lower_stat()`


### Lowering notes

- Lowers `stat(path)` and boxes the runtime stat array or PHP false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fstat_array`
- `__rt_lstat_array`
- `__rt_stat_array`

## Signature summary

```php
function stat(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stat.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stat.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stat()`](../../../php/builtins/filesystem/stat.md)
