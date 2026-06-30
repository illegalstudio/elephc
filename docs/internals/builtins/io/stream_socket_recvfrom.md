---
title: "stream_socket_recvfrom() — internals"
description: "Compiler internals for stream_socket_recvfrom(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 219
---

## `stream_socket_recvfrom()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2599](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2599) (`lower_stream_socket_recvfrom`)
- **Function symbol**: `lower_stream_socket_recvfrom()`


### Lowering notes

- Lowers `stream_socket_recvfrom(socket, length, flags?, address?)`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_socket_recvfrom(resource $socket, int $length, int $flags, string $address): mixed
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).
- **By-reference parameters**: `$address`.

## Cross-references

- [User reference for `stream_socket_recvfrom()`](../../../php/builtins/io/stream_socket_recvfrom.md)

