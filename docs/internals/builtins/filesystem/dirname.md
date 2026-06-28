---
title: "dirname() — internals"
description: "Compiler internals for dirname(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 88
---

## `dirname()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3932](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3932) (`lower_dirname`)
- **Function symbol**: `lower_dirname()`


### Lowering notes

- Lowers `dirname(path, levels?)` through the target-aware runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_dirname`
- `__rt_dirname_levels`

## Signature summary

```php
function dirname(string $path, int $levels): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `dirname()`](../../../php/builtins/filesystem/dirname.md)

