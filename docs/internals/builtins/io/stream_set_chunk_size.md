---
title: "stream_set_chunk_size() — internals"
description: "Compiler internals for stream_set_chunk_size(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 195
---

## `stream_set_chunk_size()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1975](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1975) (`lower_stream_set_chunk_size`)
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

## Cross-references

- [User reference for `stream_set_chunk_size()`](../../../php/builtins/io/stream_set_chunk_size.md)

