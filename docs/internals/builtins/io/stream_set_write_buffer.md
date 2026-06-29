---
title: "stream_set_write_buffer() — internals"
description: "Compiler internals for stream_set_write_buffer(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 198
---

## `stream_set_write_buffer()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2254](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2254) (`lower_stream_set_buffer`)
- **Function symbol**: `lower_stream_set_buffer()`


### Lowering notes

- Lowers stream read/write buffer setters as successful no-ops.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_set_write_buffer(resource $stream, int $size): int
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `stream_set_write_buffer()`](../../../php/builtins/io/stream_set_write_buffer.md)

