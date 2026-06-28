---
title: "file() — internals"
description: "Compiler internals for file(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 144
---

## `file()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3460](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3460) (`lower_file`)
- **Function symbol**: `lower_file()`


### Lowering notes

- Lowers `file(path)` through the target-aware runtime line-array helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_file`
- `__rt_realpath`

## Signature summary

```php
function file(string $filename, int $flags, mixed $context): array
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Cross-references

- [User reference for `file()`](../../../php/builtins/io/file.md)

