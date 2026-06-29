---
title: "stream_socket_get_name() — internals"
description: "Compiler internals for stream_socket_get_name(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 202
---

## `stream_socket_get_name()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2496](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2496) (`lower_stream_socket_get_name`)
- **Function symbol**: `lower_stream_socket_get_name()`


### Lowering notes

- Lowers `stream_socket_get_name(socket, remote)` and boxes `string|false`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_stream_socket_get_name`

## Signature summary

```php
function stream_socket_get_name(resource $socket, bool $remote): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 2 arguments.

## Cross-references

- [User reference for `stream_socket_get_name()`](../../../php/builtins/io/stream_socket_get_name.md)

