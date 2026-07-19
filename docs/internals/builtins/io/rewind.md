---
title: "rewind() — internals"
description: "Compiler internals for rewind(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 191
---

## `rewind()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/rewind.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/rewind.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:3197](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L3197) (`lower_rewind`)
- **Function symbol**: `lower_rewind()`


### Lowering notes

- Lowers `rewind(stream)` as `lseek(fd, 0, SEEK_SET)` and clears EOF state on success.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function rewind(resource $stream): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/rewind.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/rewind.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `rewind()`](../../../php/builtins/io/rewind.md)
