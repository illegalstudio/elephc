---
title: "stream_filter_prepend() — internals"
description: "Compiler internals for stream_filter_prepend(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 333
---

## `stream_filter_prepend()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_filter_prepend.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_filter_prepend.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:1550](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L1550) (`lower_stream_filter_attach`)
- **Function symbol**: `lower_stream_filter_attach()`


### Lowering notes

- Lowers `stream_filter_append` and `stream_filter_prepend`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_filter_prepend(resource $stream, string $filtername, int $read_write = 3, mixed $params = null): mixed
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Cross-references

- [User reference for `stream_filter_prepend()`](../../../php/builtins/streams/stream_filter_prepend.md)

