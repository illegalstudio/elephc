---
title: "tempnam() — internals"
description: "Compiler internals for tempnam(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 146
---

## `tempnam()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4453](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4453) (`lower_tempnam`)
- **Function symbol**: `lower_tempnam()`


### Lowering notes

- Lowers `tempnam(directory, prefix)` through the target-aware runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_glob`
- `__rt_scandir`
- `__rt_tempnam`

## Signature summary

```php
function tempnam(string $directory, string $prefix): string
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `tempnam()`](../../../php/builtins/filesystem/tempnam.md)

