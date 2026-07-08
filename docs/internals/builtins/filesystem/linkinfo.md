---
title: "linkinfo() — internals"
description: "Compiler internals for linkinfo(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 132
---

## `linkinfo()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/linkinfo.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/linkinfo.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:5440](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L5440) (`lower_linkinfo`)
- **Function symbol**: `lower_linkinfo()`


### Lowering notes

- Lowers `linkinfo(path)` through the target-aware runtime lstat helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_link`
- `__rt_linkinfo`
- `__rt_readlink`
- `__rt_symlink`

## Signature summary

```php
function linkinfo(string $path): int
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `linkinfo()`](../../../php/builtins/filesystem/linkinfo.md)
