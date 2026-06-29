---
title: "is_writable() — internals"
description: "Compiler internals for is_writable(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 110
---

## `is_writable()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:5610](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L5610) (`lower_is_writable`)
- **Function symbol**: `lower_is_writable()`


### Lowering notes

- Lowers `is_writable(path)` through the target-aware runtime access helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_is_executable`
- `__rt_is_link`
- `__rt_is_writable`

## Signature summary

```php
function is_writable(string $filename): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `is_writable()`](../../../php/builtins/filesystem/is_writable.md)

