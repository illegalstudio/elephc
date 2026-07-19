---
title: "stream_set_chunk_size() — internals"
description: "Compiler internals for stream_set_chunk_size(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 229
---

## `stream_set_chunk_size()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_set_chunk_size.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_set_chunk_size.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:2192](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L2192) (`lower_stream_set_chunk_size`)
- **Function symbol**: `lower_stream_set_chunk_size()`


### Lowering notes

- Lowers `stream_set_chunk_size(stream, size)` and returns the previous size.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_set_chunk_size(resource $stream, int $size): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_set_chunk_size.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_set_chunk_size.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_set_chunk_size()`](../../../php/builtins/io/stream_set_chunk_size.md)
