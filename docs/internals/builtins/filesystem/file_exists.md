---
title: "file_exists() — internals"
description: "Compiler internals for file_exists(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 106
---

## `file_exists()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4399](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4399) (`lower_file_exists`)
- **Function symbol**: `lower_file_exists()`


### Lowering notes

- Lowers `file_exists(path)` through the target-aware runtime stat helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mkdir`
- `__rt_unlink`

## Signature summary

```php
function file_exists(string $filename): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `file_exists()`](../../../php/builtins/filesystem/file_exists.md)

