---
title: "link() — internals"
description: "Compiler internals for link(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 114
---

## `link()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4810](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4810) (`lower_link`)
- **Function symbol**: `lower_link()`


### Lowering notes

- Lowers `link(oldpath, newpath)` through the target-aware libc wrapper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_fileatime`
- `__rt_filectime`
- `__rt_link`
- `__rt_readlink`

## Signature summary

```php
function link(string $target, string $link): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `link()`](../../../php/builtins/filesystem/link.md)

