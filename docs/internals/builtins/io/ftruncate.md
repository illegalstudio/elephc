---
title: "ftruncate() — internals"
description: "Compiler internals for ftruncate(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 158
---

## `ftruncate()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:3208](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L3208) (`lower_ftruncate`)
- **Function symbol**: `lower_ftruncate()`


### Lowering notes

- Lowers `ftruncate(stream, size)` through the shared fd truncate runtime helper.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function ftruncate(resource $stream, int $size): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `ftruncate()`](../../../php/builtins/io/ftruncate.md)

