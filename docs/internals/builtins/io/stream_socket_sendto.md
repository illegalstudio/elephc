---
title: "stream_socket_sendto() — internals"
description: "Compiler internals for stream_socket_sendto(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 205
---

## `stream_socket_sendto()` — internals

## Where it lives

- **Signature**: [`src/types/signatures.rs`](https://github.com/illegalstudio/elephc/blob/main/src/types/signatures.rs)
- **Lowering**: [`src/codegen_ir/lower_inst/builtins/io.rs`:2422](https://github.com/illegalstudio/elephc/blob/main/src/codegen_ir/lower_inst/builtins/io.rs#L2422) (`lower_stream_socket_sendto`)
- **Function symbol**: `lower_stream_socket_sendto()`


### Lowering notes

- Lowers `stream_socket_sendto(socket, data, flags?, address?)` and boxes `int|false`.

## Runtime helpers

_No direct `__rt_*` helpers captured — the lowering is inlined or routes through another builtin._

## Signature summary

```php
function stream_socket_sendto(resource $socket, string $data, int $flags, string $address): mixed
```

## What the type checker enforces

- **Arity**: takes 2–4 arguments (2 optional).

## Cross-references

- [User reference for `stream_socket_sendto()`](../../../php/builtins/io/stream_socket_sendto.md)

