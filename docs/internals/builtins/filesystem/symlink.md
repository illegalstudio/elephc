---
title: "symlink() — internals"
description: "Compiler internals for symlink(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 129
---

## `symlink()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4805](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4805) (`lower_symlink`)
- **Function symbol**: `lower_symlink()`


### Lowering notes

- Lowers `symlink(target, link)` through the target-aware libc wrapper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fileatime`
- `__rt_link`
- `__rt_readlink`
- `__rt_symlink`

## Signature summary

```php
function symlink(string $target, string $link): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `symlink()`](../../../php/builtins/filesystem/symlink.md)

