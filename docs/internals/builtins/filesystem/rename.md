---
title: "rename() — internals"
description: "Compiler internals for rename(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 125
---

## `rename()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4442](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4442) (`lower_rename`)
- **Function symbol**: `lower_rename()`


### Lowering notes

- Lowers `rename(from, to)` through the target-aware runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_glob`
- `__rt_scandir`
- `__rt_tempnam`

## Signature summary

```php
function rename(string $from, string $to, mixed $context): bool
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Cross-references

- [User reference for `rename()`](../../../php/builtins/filesystem/rename.md)

