---
title: "filectime() — internals"
description: "Compiler internals for filectime(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 114
---

## `filectime()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/filectime.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/filectime.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5473](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5473) (`lower_filectime`)
- **Function symbol**: `lower_filectime()`


### Lowering notes

- Lowers `filectime(path)` and boxes the runtime integer-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_filectime`
- `__rt_filegroup`
- `__rt_fileowner`
- `__rt_fileperms`

## Signature summary

```php
function filectime(string $filename): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/filectime.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/filectime.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `filectime()`](../../../php/builtins/filesystem/filectime.md)
