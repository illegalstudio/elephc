---
title: "stream_filter_remove() — internals"
description: "Compiler internals for stream_filter_remove(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 217
---

## `stream_filter_remove()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_filter_remove.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_filter_remove.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:1938](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L1938) (`lower_stream_filter_remove`)
- **Function symbol**: `lower_stream_filter_remove()`


### Lowering notes

- Lowers `stream_filter_remove(filter)` and clears both direction tables for the fd.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_user_filter_release_fd`

## Signature summary

```php
function stream_filter_remove(resource $stream_filter): bool
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_filter_remove.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_filter_remove.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_filter_remove()`](../../../php/builtins/io/stream_filter_remove.md)
