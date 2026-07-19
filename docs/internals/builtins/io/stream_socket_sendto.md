---
title: "stream_socket_sendto() — internals"
description: "Compiler internals for stream_socket_sendto(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 239
---

## `stream_socket_sendto()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_socket_sendto.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_socket_sendto.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:2640](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L2640) (`lower_stream_socket_sendto`)
- **Function symbol**: `lower_stream_socket_sendto()`


### Lowering notes

- Lowers `stream_socket_sendto(socket, data, flags?, address?)` and boxes `int|false`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_socket_sendto(resource $socket, string $data, int $flags = 0, string $address = ''): mixed
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_sendto.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_sendto.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_socket_sendto()`](../../../php/builtins/io/stream_socket_sendto.md)
