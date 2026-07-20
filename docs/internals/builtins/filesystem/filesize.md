---
title: "filesize() — internals"
description: "Compiler internals for filesize(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 120
---

## `filesize()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/filesize.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/filesize.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5420](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5420) (`lower_filesize`)
- **Function symbol**: `lower_filesize()`


### Lowering notes

- Lowers `filesize(path)` through the target-aware runtime stat helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_filemtime`
- `__rt_link`
- `__rt_linkinfo`
- `__rt_symlink`

## Signature summary

```php
function filesize(string $filename): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/filesize.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/filesize.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `filesize()`](../../../php/builtins/filesystem/filesize.md)
