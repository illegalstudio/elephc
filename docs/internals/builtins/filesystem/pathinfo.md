---
title: "pathinfo() — internals"
description: "Compiler internals for pathinfo(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 118
---

## `pathinfo()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4638](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4638) (`lower_pathinfo`)
- **Function symbol**: `lower_pathinfo()`


### Lowering notes

- Lowers `pathinfo(path, flags?)` through string, array, or boxed dynamic helpers.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_pathinfo_array`

## Signature summary

```php
function pathinfo(string $path, int $flags): mixed
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `pathinfo()`](../../../php/builtins/filesystem/pathinfo.md)

