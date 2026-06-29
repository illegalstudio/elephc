---
title: "umask() — internals"
description: "Compiler internals for umask(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 134
---

## `umask()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4487](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4487) (`lower_umask`)
- **Function symbol**: `lower_umask()`


### Lowering notes

- Lowers `umask(mask?)` through the target-aware runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_umask`

## Signature summary

```php
function umask(int $mask): int
```

## What the type checker enforces

- **Arity**: takes 0–1 arguments (1 optional).

## Cross-references

- [User reference for `umask()`](../../../php/builtins/filesystem/umask.md)

