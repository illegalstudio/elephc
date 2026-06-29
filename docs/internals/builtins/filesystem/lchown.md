---
title: "lchown() — internals"
description: "Compiler internals for lchown(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 113
---

## `lchown()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4477](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4477) (`lower_lchown`)
- **Function symbol**: `lower_lchown()`


### Lowering notes

- Lowers `lchown(path, owner)` for integer UIDs and string user names without following symlinks.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_umask`

## Signature summary

```php
function lchown(string $filename, int $user): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `lchown()`](../../../php/builtins/filesystem/lchown.md)

