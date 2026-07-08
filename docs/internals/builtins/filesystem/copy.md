---
title: "copy() — internals"
description: "Compiler internals for copy(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 104
---

## `copy()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/copy.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/copy.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:4443](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L4443) (`lower_copy`)
- **Function symbol**: `lower_copy()`


### Lowering notes

- Lowers `copy(source, dest)` through the target-aware runtime helper.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_copy`
- `__rt_glob`
- `__rt_scandir`
- `__rt_tempnam`

## Signature summary

```php
function copy(string $from, string $to): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `copy()`](../../../php/builtins/filesystem/copy.md)
