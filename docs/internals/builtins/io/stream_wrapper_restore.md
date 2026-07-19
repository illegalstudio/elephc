---
title: "stream_wrapper_restore() — internals"
description: "Compiler internals for stream_wrapper_restore(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 244
---

## `stream_wrapper_restore()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_wrapper_restore.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_wrapper_restore.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:1051](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L1051) (`lower_stream_wrapper_restore`)
- **Function symbol**: `lower_stream_wrapper_restore()`


### Lowering notes

- Lowers `stream_wrapper_restore(protocol)` as a successful no-op.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_wrapper_restore(string $protocol): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_wrapper_restore.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_wrapper_restore.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_wrapper_restore()`](../../../php/builtins/io/stream_wrapper_restore.md)
