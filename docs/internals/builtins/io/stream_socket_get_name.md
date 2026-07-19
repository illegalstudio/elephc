---
title: "stream_socket_get_name() — internals"
description: "Compiler internals for stream_socket_get_name(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 236
---

## `stream_socket_get_name()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_socket_get_name.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_socket_get_name.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:2495](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L2495) (`lower_stream_socket_get_name`)
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

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_get_name.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_get_name.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_socket_get_name()`](../../../php/builtins/io/stream_socket_get_name.md)
