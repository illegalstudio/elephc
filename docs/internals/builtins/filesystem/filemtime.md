---
title: "filemtime() — internals"
description: "Compiler internals for filemtime(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 117
---

## `filemtime()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/filemtime.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/filemtime.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5432](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5432) (`lower_filemtime`)
- **Function symbol**: `lower_filemtime()`


### Lowering notes

- Lowers `filemtime(path)` through the target-aware runtime stat helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_filemtime`
- `__rt_link`
- `__rt_linkinfo`
- `__rt_readlink`
- `__rt_symlink`

## Signature summary

```php
function filemtime(string $filename): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/filemtime.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/filemtime.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `filemtime()`](../../../php/builtins/filesystem/filemtime.md)
