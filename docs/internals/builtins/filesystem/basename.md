---
title: "basename() — internals"
description: "Compiler internals for basename(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 96
---

## `basename()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:4536](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L4536) (`lower_basename`)
- **Function symbol**: `lower_basename()`


### Lowering notes

- Lowers `basename(path, suffix?)` through the target-aware runtime helper.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function basename(string $path, string $suffix): string
```

## What the type checker enforces

- **Arity**: takes 1–2 arguments (1 optional).

## Cross-references

- [User reference for `basename()`](../../../php/builtins/filesystem/basename.md)

