---
title: "stream_filter_remove() — internals"
description: "Compiler internals for stream_filter_remove(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 183
---

## `stream_filter_remove()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1939](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1939) (`lower_stream_filter_remove`)
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

## Cross-references

- [User reference for `stream_filter_remove()`](../../../php/builtins/io/stream_filter_remove.md)

