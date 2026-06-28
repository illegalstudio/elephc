---
title: "unlink() — internals"
description: "Compiler internals for unlink(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 135
---

## `unlink()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3764](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3764) (`lower_unlink`)
- **Function symbol**: `lower_unlink()`


### Lowering notes

- Lowers `unlink(path)` through the target-aware runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_mkdir`
- `__rt_rmdir`
- `__rt_unlink`

## Signature summary

```php
function unlink(string $filename): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `unlink()`](../../../php/builtins/filesystem/unlink.md)

