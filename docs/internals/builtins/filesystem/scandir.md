---
title: "scandir() — internals"
description: "Compiler internals for scandir(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 144
---

## `scandir()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/scandir.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/scandir.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4458](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4458) (`lower_scandir`)
- **Function symbol**: `lower_scandir()`


### Lowering notes

- Lowers `scandir(path)` through the target-aware runtime directory listing helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_glob`
- `__rt_scandir`

## Signature summary

```php
function scandir(string $directory): array
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `scandir()`](../../../php/builtins/filesystem/scandir.md)
