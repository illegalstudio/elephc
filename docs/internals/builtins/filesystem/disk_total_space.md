---
title: "disk_total_space() — internals"
description: "Compiler internals for disk_total_space(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 90
---

## `disk_total_space()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3157](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3157) (`lower_disk_total_space`)
- **Function symbol**: `lower_disk_total_space()`


### Lowering notes

- Lowers `disk_total_space(path)` through the shared disk-space runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_disk_space`

## Signature summary

```php
function disk_total_space(string $directory): float
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `disk_total_space()`](../../../php/builtins/filesystem/disk_total_space.md)

