---
title: "scandir() — internals"
description: "Compiler internals for scandir(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 127
---

## `scandir()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4452](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4452) (`lower_scandir`)
- **Function symbol**: `lower_scandir()`


### Lowering notes

- Lowers `scandir(path)` through the target-aware runtime directory listing helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_glob`
- `__rt_scandir`

## Signature summary

```php
function scandir(string $directory, int $sorting_order, mixed $context): array
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Cross-references

- [User reference for `scandir()`](../../../php/builtins/filesystem/scandir.md)

