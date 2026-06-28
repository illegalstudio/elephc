---
title: "filemtime() — internals"
description: "Compiler internals for filemtime(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 96
---

## `filemtime()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4789](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4789) (`lower_filemtime`)
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

## Cross-references

- [User reference for `filemtime()`](../../../php/builtins/filesystem/filemtime.md)

