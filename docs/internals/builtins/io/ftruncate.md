---
title: "ftruncate() — internals"
description: "Compiler internals for ftruncate(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 179
---

## `ftruncate()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/ftruncate.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/ftruncate.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3210](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3210) (`lower_ftruncate`)
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

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/ftruncate.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/ftruncate.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `ftruncate()`](../../../php/builtins/io/ftruncate.md)
