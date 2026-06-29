---
title: "glob() — internals"
description: "Compiler internals for glob(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 104
---

## `glob()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4457](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4457) (`lower_glob`)
- **Function symbol**: `lower_glob()`


### Lowering notes

- Lowers `glob(pattern)` through the target-aware runtime glob expansion helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_glob`

## Signature summary

```php
function glob(string $pattern, int $flags): array
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `glob()`](../../../php/builtins/filesystem/glob.md)

