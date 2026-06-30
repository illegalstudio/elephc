---
title: "stream_socket_accept() — internals"
description: "Compiler internals for stream_socket_accept(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 214
---

## `stream_socket_accept()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2436](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2436) (`lower_stream_socket_accept`)
- **Function symbol**: `lower_stream_socket_accept()`


### Lowering notes

- Lowers `stream_socket_accept(server, timeout?, peer_name?)`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_stream_socket_accept`

## Signature summary

```php
function stream_socket_accept(resource $socket, float $timeout, string $peer_name): mixed
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).
- **By-reference parameters**: `$peer_name`.

## Cross-references

- [User reference for `stream_socket_accept()`](../../../php/builtins/io/stream_socket_accept.md)

