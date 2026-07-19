---
title: "readlink() — internals"
description: "Compiler internals for readlink(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 142
---

## `readlink()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/readlink.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/readlink.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5455](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5455) (`lower_readlink`)
- **Function symbol**: `lower_readlink()`


### Lowering notes

- Lowers `readlink(path)` and boxes the owned runtime string-or-false result.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fileatime`
- `__rt_filectime`
- `__rt_fileperms`
- `__rt_readlink`

## Signature summary

```php
function readlink(string $path): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/readlink.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/readlink.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `readlink()`](../../../php/builtins/filesystem/readlink.md)
