---
title: "is_dir() — internals"
description: "Compiler internals for is_dir(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 105
---

## `is_dir()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4957](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4957) (`lower_is_dir`)
- **Function symbol**: `lower_is_dir()`


### Lowering notes

- Lowers `is_dir(path)` through the target-aware runtime stat helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_is_dir`
- `__rt_is_readable`
- `__rt_is_writable`

## Signature summary

```php
function is_dir(string $filename): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `is_dir()`](../../../php/builtins/filesystem/is_dir.md)

