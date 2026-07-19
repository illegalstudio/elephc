---
title: "stream_socket_pair() — internals"
description: "Compiler internals for stream_socket_pair(): lowering path, type checks, and runtime helpers."
sidebar:
  order: 224
---

## `stream_socket_pair()` — internals

## Where it lives

- **Signature**: [`src/builtins/io/stream_socket_pair.rs`](https://github.com/illegalstudio/elephc/blob/main/src/builtins/io/stream_socket_pair.rs)
- **Lowering**: [`src/codegen/lower_inst/builtins/io.rs`:2465](https://github.com/illegalstudio/elephc/blob/main/src/codegen/lower_inst/builtins/io.rs#L2465) (`lower_stream_socket_pair`)
- **Function symbol**: `lower_stream_socket_pair()`


### Lowering notes

- Lowers `stream_socket_pair(domain, type, protocol)` and boxes `array|false`.

## Runtime helpers

The following runtime helpers are referenced:
- `__rt_stream_socket_pair`

## Signature summary

```php
function stream_socket_pair(int $domain, int $type, int $protocol): mixed
```

## What the type checker enforces

- **Arity**: takes exactly 3 arguments.

## Eval interpreter (magician)

- **Declaration**: [`crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_pair.rs`](https://github.com/illegalstudio/elephc/blob/main/crates/elephc-magician/src/interpreter/builtins/filesystem/stream_socket_pair.rs) (`eval_builtin!`)
- **Dispatch hooks**: `direct`, `values`

## Cross-references

- [User reference for `stream_socket_pair()`](../../../php/builtins/io/stream_socket_pair.md)
