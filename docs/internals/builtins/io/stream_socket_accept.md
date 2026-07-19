---
title: "stream_socket_accept() — internals"
description: "Compiler internals for stream_socket_accept(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 220
---

## `stream_socket_accept()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_socket_accept.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_socket_accept.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:2436](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L2436) (`lower_stream_socket_accept`)
- **Function symbol**: `lower_stream_socket_accept()`


### Lowering notes

- Lowers `stream_socket_accept(server, timeout?, peer_name?)`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_stream_socket_accept`

## Signature summary

```php
function stream_socket_accept(resource $socket, float $timeout = null, string $peer_name = null): mixed
```

## What the type checker enforces

- **Arity**: takes 1–3 arguments (2 optional).
- **By-reference parameters**: `$peer_name`.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_accept.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_accept.rs) (`eval_builtin!`)
- **Dispatch hooks**: `values`
- **By-reference parameters**: `$peer_name`.

## Cross-references

- [User reference for `stream_socket_accept()`](../../../php/builtins/io/stream_socket_accept.md)
