---
title: "stream_socket_server() — internals"
description: "Compiler internals for stream_socket_server(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 223
---

## `stream_socket_server()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_socket_server.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_socket_server.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2374](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2374) (`lower_stream_socket_server`)
- **Function symbol**: `lower_stream_socket_server()`


### Lowering notes

- Lowers `stream_socket_server(address)` and boxes `resource|false`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_stream_socket_server`

## Signature summary

```php
function stream_socket_server(string $address): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 1 argument.

## Cross-references

- [User reference for `stream_socket_server()`](../../../php/builtins/io/stream_socket_server.md)

