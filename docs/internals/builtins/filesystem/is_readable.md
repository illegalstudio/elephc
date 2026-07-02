---
title: "is_readable() — internals"
description: "Compiler internals for is_readable(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 126
---

## `is_readable()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/is_readable.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/is_readable.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:5608](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L5608) (`lower_is_readable`)
- **Function symbol**: `lower_is_readable()`


### Lowering notes

- Lowers `is_readable(path)` through the target-aware runtime access helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_is_executable`
- `__rt_is_readable`
- `__rt_is_writable`

## Signature summary

```php
function is_readable(string $filename): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `is_readable()`](../../../php/builtins/filesystem/is_readable.md)

