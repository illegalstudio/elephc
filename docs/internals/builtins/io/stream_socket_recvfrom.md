---
title: "stream_socket_recvfrom() — internals"
description: "Compiler internals for stream_socket_recvfrom(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 238
---

## `stream_socket_recvfrom()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_socket_recvfrom.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_socket_recvfrom.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:2597](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L2597) (`lower_stream_socket_recvfrom`)
- **Function symbol**: `lower_stream_socket_recvfrom()`


### Lowering notes

- Lowers `stream_socket_recvfrom(socket, length, flags?, address?)`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_socket_recvfrom(resource $socket, int $length, int $flags = 0, string $address = ''): mixed
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).
- **By-reference parameters**: `$address`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_recvfrom.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_recvfrom.rs) (`eval_builtin!`)
- **Dispatch hooks**: `values`
- **By-reference parameters**: `$address`.

## Cross-references

- [User reference for `stream_socket_recvfrom()`](../../../php/builtins/io/stream_socket_recvfrom.md)
