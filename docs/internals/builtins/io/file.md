---
title: "file() — internals"
description: "Compiler internals for file(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 161
---

## `file()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/file.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/file.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3685](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3685) (`lower_file`)
- **Function symbol**: `lower_file()`


### Lowering notes

- Lowers `file(path)` through the target-aware runtime line-array helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_file`
- `__rt_realpath`

## Signature summary

```php
function file(string $filename): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `file()`](../../../php/builtins/io/file.md)

