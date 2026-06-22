---
title: "is_file() — internals"
description: "Compiler internals for is_file(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 107
---

## `is_file()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:5586](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L5586) (`lower_is_file`)
- **Function symbol**: `lower_is_file()`


### Lowering notes

- Lowers `is_file(path)` through the target-aware runtime stat helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_is_dir`
- `__rt_is_readable`
- `__rt_is_writable`

## Signature summary

```php
function is_file(string $filename): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `is_file()`](../../../php/builtins/filesystem/is_file.md)

