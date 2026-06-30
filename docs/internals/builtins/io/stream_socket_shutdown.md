---
title: "stream_socket_shutdown() — internals"
description: "Compiler internals for stream_socket_shutdown(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 222
---

## `stream_socket_shutdown()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2522](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2522) (`lower_stream_socket_shutdown`)
- **Function symbol**: `lower_stream_socket_shutdown()`


### Lowering notes

- Lowers `stream_socket_shutdown(stream, mode)`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_stream_socket_shutdown`

## Signature summary

```php
function stream_socket_shutdown(resource $stream, int $mode): bool
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `stream_socket_shutdown()`](../../../php/builtins/io/stream_socket_shutdown.md)

