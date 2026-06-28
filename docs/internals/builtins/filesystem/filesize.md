---
title: "filesize() — internals"
description: "Compiler internals for filesize(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 99
---

## `filesize()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4781](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4781) (`lower_filesize`)
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

## Cross-references

- [User reference for `filesize()`](../../../php/builtins/filesystem/filesize.md)

