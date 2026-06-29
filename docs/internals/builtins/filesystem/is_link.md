---
title: "is_link() — internals"
description: "Compiler internals for is_link(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 108
---

## `is_link()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:5634](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L5634) (`lower_is_link`)
- **Function symbol**: `lower_is_link()`


### Lowering notes

- Lowers `is_link(path)` through the target-aware runtime lstat helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_is_link`
- `__rt_path_is_wrapper`
- `__rt_readfile`
- `__rt_readfile_wrapper`

## Signature summary

```php
function is_link(string $filename): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `is_link()`](../../../php/builtins/filesystem/is_link.md)

