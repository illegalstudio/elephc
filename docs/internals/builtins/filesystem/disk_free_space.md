---
title: "disk_free_space() — internals"
description: "Compiler internals for disk_free_space(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 89
---

## `disk_free_space()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3368](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3368) (`lower_disk_free_space`)
- **Function symbol**: `lower_disk_free_space()`


### Lowering notes

- Lowers `disk_free_space(path)` through the shared disk-space runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_disk_space`

## Signature summary

```php
function disk_free_space(string $directory): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `disk_free_space()`](../../../php/builtins/filesystem/disk_free_space.md)

